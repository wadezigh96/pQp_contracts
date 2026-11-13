mod canonical;
use canonical::{
    canonical_json, decimal, envelope_commitment, integer, static_profile, string,
    validate_envelope,
};

use near_sdk::json_types::U128;
use near_sdk::serde::Serialize;
use near_sdk::serde_json::{json, Value};
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    collections::LookupMap,
    env, near, AccountId, BorshStorageKey, PanicOnDefault,
};

#[cfg(test)]
const HASH_DOMAIN: &str = "QPC-NEAR-ZCASH-POLICY-ARGS-V1\0";

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    Agents,
    UsedPayments,
    AllowedRecipients,
    EnvelopeProfiles,
}

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct AgentPolicy {
    active: bool,
    policy_id: String,
    router: AccountId,
    expires_at_unix: u64,
    max_per_payment: u128,
    total_cap: u128,
    spent: u128,
    next_nonce: u64,
    policy_version: u32,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct AgentPolicyView {
    pub active: bool,
    pub policy_id: String,
    pub router: AccountId,
    pub expires_at_unix: u64,
    pub max_per_payment: String,
    pub total_cap: String,
    pub spent: String,
    pub next_nonce: u64,
    pub policy_version: u32,
}

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
pub struct DispatchReceipt {
    pub state: String,
    pub payment_id: String,
    pub amount_zatoshis: String,
    pub nonce: u64,
    pub policy_version: u32,
    pub policy_args_hash: String,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct QPCNear {
    owner: AccountId,
    agents: LookupMap<AccountId, AgentPolicy>,
    used_payments: LookupMap<String, bool>,
    allowed_recipients: LookupMap<(AccountId, String), bool>,
    envelope_profiles: LookupMap<AccountId, String>,
}

#[near]
impl QPCNear {
    #[init]
    pub fn new(owner: AccountId) -> Self {
        assert!(!env::state_exists(), "already initialized");
        Self {
            owner,
            agents: LookupMap::new(StorageKey::Agents),
            used_payments: LookupMap::new(StorageKey::UsedPayments),
            allowed_recipients: LookupMap::new(StorageKey::AllowedRecipients),
            envelope_profiles: LookupMap::new(StorageKey::EnvelopeProfiles),
        }
    }

    pub fn configure_agent(
        &mut self,
        agent: AccountId,
        policy_id: String,
        router: AccountId,
        expires_at_unix: u64,
        max_per_payment: U128,
        total_cap: U128,
    ) {
        self.assert_owner();
        assert!(agent != self.owner, "owner cannot be an agent");
        assert!(is_identifier(&policy_id), "invalid policy id");
        assert!(expires_at_unix > now_unix(), "policy already expired");
        let max_per_payment = max_per_payment.0;
        let total_cap = total_cap.0;
        assert!(
            max_per_payment > 0 && total_cap >= max_per_payment,
            "invalid spending limits"
        );

        let (next_version, next_nonce) = self
            .agents
            .get(&agent)
            .map(|old| {
                (
                    old.policy_version
                        .checked_add(1)
                        .expect("policy version overflow"),
                    old.next_nonce,
                )
            })
            .unwrap_or((1, 0));
        self.envelope_profiles.remove(&agent);
        self.agents.insert(
            &agent,
            &AgentPolicy {
                active: true,
                policy_id,
                router,
                expires_at_unix,
                max_per_payment,
                total_cap,
                spent: 0,
                next_nonce,
                policy_version: next_version,
            },
        );
    }

    pub fn set_recipient_permission(
        &mut self,
        agent: AccountId,
        opaque_recipient_ref: String,
        allowed: bool,
    ) {
        self.assert_owner();
        assert!(
            is_identifier(&opaque_recipient_ref),
            "invalid opaque recipient reference"
        );
        self.allowed_recipients
            .insert(&(agent, opaque_recipient_ref), &allowed);
    }

    pub fn revoke_agent(&mut self, agent: AccountId) {
        self.assert_owner();
        if let Some(mut policy) = self.agents.get(&agent) {
            policy.active = false;
            self.envelope_profiles.remove(&agent);
            self.agents.insert(&agent, &policy);
        }
    }

    pub fn get_agent_policy(&self, agent: AccountId) -> Option<AgentPolicyView> {
        self.agents.get(&agent).map(|p| AgentPolicyView {
            active: p.active,
            policy_id: p.policy_id,
            router: p.router,
            expires_at_unix: p.expires_at_unix,
            max_per_payment: p.max_per_payment.to_string(),
            total_cap: p.total_cap.to_string(),
            spent: p.spent.to_string(),
            next_nonce: p.next_nonce,
            policy_version: p.policy_version,
        })
    }

    /// Retired legacy ABI. Always rejects: the narrow payment hash is not an
    /// alternative to the complete canonical envelope authorization boundary.
    pub fn dispatch_action(
        &mut self,
        request_id: String,
        policy_id: String,
        policy_version: u32,
        nonce: u64,
        action_id: String,
        destination_chain: String,
        destination_router: AccountId,
        payment_id: String,
        amount_zatoshis: String,
        opaque_recipient_ref: String,
        expires_at_unix: u64,
        privacy_policy: String,
        policy_args_hash: String,
    ) -> DispatchReceipt {
        // Kept only as an ABI tombstone. A narrow legacy payment hash must not
        // authorize actions outside the complete canonical envelope boundary.
        let _ = (
            request_id,
            policy_id,
            policy_version,
            nonce,
            action_id,
            destination_chain,
            destination_router,
            payment_id,
            amount_zatoshis,
            opaque_recipient_ref,
            expires_at_unix,
            privacy_policy,
            policy_args_hash,
        );
        panic!("legacy dispatch disabled; use authorize_envelope");
    }

    /// Owner approves an exact route/asset/amount/fee/policy/settlement profile.
    /// Only intent, nonce, bounded times and an allowlisted recipient may vary.
    /// Source network/genesis IDs are explicit owner configuration: NEAR's host
    /// API exposes the current account, not the network/genesis identity.
    pub fn configure_envelope_profile(&mut self, agent: AccountId, envelope: Value) {
        self.assert_owner();
        validate_envelope(&envelope);
        let policy = self.agents.get(&agent).expect("agent is not configured");
        assert!(policy.active, "agent is revoked");
        self.assert_native_bindings(&agent, &policy, &envelope);
        self.envelope_profiles
            .insert(&agent, &canonical_json(&envelope));
    }

    /// Native predecessor authorization only. No transfer, cross-contract call,
    /// signature, destination receipt, or settlement is produced by this method.
    pub fn authorize_envelope(
        &mut self,
        envelope: Value,
        authorization_commitment: String,
    ) -> Value {
        validate_envelope(&envelope);
        let agent = env::predecessor_account_id();
        let mut policy = self.agents.get(&agent).expect("agent is not configured");
        assert!(policy.active, "agent is revoked");
        self.assert_native_bindings(&agent, &policy, &envelope);
        let profile: Value = near_sdk::serde_json::from_str(
            &self
                .envelope_profiles
                .get(&agent)
                .expect("full envelope profile required"),
        )
        .expect("stored profile invalid");
        assert_eq!(
            static_profile(&envelope),
            static_profile(&profile),
            "envelope profile mismatch"
        );
        let now = now_unix();
        assert!(policy.expires_at_unix > now, "agent policy expired");
        assert!(
            integer(&envelope["time"]["issuedAtUnix"]) <= now,
            "issued in future"
        );
        assert!(
            integer(&envelope["time"]["notBeforeUnix"]) <= now,
            "not yet valid"
        );
        let expiry = integer(&envelope["time"]["expiresAtUnix"]);
        assert!(
            expiry > now && expiry <= policy.expires_at_unix,
            "authorization expired or exceeds policy expiry"
        );
        let nonce = decimal(&envelope["nonce"], false);
        assert_eq!(
            nonce, policy.next_nonce as u128,
            "invalid or replayed nonce"
        );
        let intent = string(&envelope["intentId"]);
        assert!(
            self.used_payments.get(&intent.to_owned()).is_none(),
            "intent already used"
        );
        let recipient = string(&envelope["destination"]["recipientCommitment"]);
        assert!(
            self.allowed_recipients
                .get(&(agent.clone(), recipient.to_owned()))
                .unwrap_or(false),
            "recipient is not allowed"
        );
        let amount = decimal(&envelope["amount"]["destinationAtomic"], true);
        assert!(
            amount <= policy.max_per_payment,
            "per-payment limit exceeded"
        );
        let spent = policy.spent.checked_add(amount).expect("spending overflow");
        assert!(
            spent <= policy.total_cap,
            "cumulative spending cap exceeded"
        );
        let expected = envelope_commitment(&envelope);
        assert_eq!(
            authorization_commitment, expected,
            "authorization commitment mismatch"
        );
        let next_nonce = policy.next_nonce.checked_add(1).expect("nonce overflow");
        // All validation precedes state mutation; NEAR rolls back a failed call.
        policy.spent = spent;
        policy.next_nonce = next_nonce;
        self.agents.insert(&agent, &policy);
        self.used_payments.insert(&intent.to_owned(), &true);
        let receipt = json!({"state":"AUTHORIZED", "intentId":intent,
            "authorizationCommitment":expected, "nonce":envelope["nonce"]});
        // No raw recipient, amounts, or envelope are duplicated in the event.
        env::log_str(&format!(
            "EVENT_JSON:{}",
            json!({"standard":"qpc", "version":"1.0.0",
            "event":"agent_action_authorized", "data":[receipt.clone()]})
        ));
        receipt
    }

    fn assert_native_bindings(&self, agent: &AccountId, policy: &AgentPolicy, envelope: &Value) {
        assert_eq!(
            string(&envelope["agentIdentity"]["namespace"]),
            "near",
            "native namespace mismatch"
        );
        assert_eq!(
            string(&envelope["agentIdentity"]["id"]),
            agent.as_str(),
            "native agent mismatch"
        );
        // keyId denotes the native account authority, not a verified public key.
        assert_eq!(
            string(&envelope["agentIdentity"]["keyId"]),
            agent.as_str(),
            "native key authority mismatch"
        );
        assert_eq!(
            string(&envelope["source"]["accountId"]),
            agent.as_str(),
            "source account mismatch"
        );
        assert_eq!(
            string(&envelope["source"]["adapterId"]),
            env::current_account_id().as_str(),
            "source adapter mismatch"
        );
        assert_eq!(
            string(&envelope["replayDomain"]["authorityId"]),
            env::current_account_id().as_str(),
            "replay authority mismatch"
        );
        assert_eq!(
            string(&envelope["cancellation"]["authorityId"]),
            self.owner.as_str(),
            "cancellation authority mismatch"
        );
        assert_eq!(
            string(&envelope["destination"]["adapterId"]),
            policy.router.as_str(),
            "router mismatch"
        );
        assert_eq!(
            string(&envelope["policy"]["id"]),
            policy.policy_id,
            "policy id mismatch"
        );
        assert_eq!(
            integer(&envelope["policy"]["version"]),
            policy.policy_version as u64,
            "stale policy version"
        );
    }

    fn assert_owner(&self) {
        assert_eq!(env::predecessor_account_id(), self.owner, "owner only");
    }
}

fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

#[cfg(test)]
fn compute_policy_args_hash(
    payment_id: &str,
    opaque_recipient_ref: &str,
    amount_zatoshis: &str,
    expires_at_unix: u64,
) -> String {
    let canonical = format!(
        "{}{{\"paymentId\":{},\"opaqueRecipientRef\":{},\"amountZatoshis\":{},\"expiresAtUnix\":{}}}",
        HASH_DOMAIN,
        near_sdk::serde_json::to_string(payment_id).expect("serialize payment id"),
        near_sdk::serde_json::to_string(opaque_recipient_ref).expect("serialize recipient ref"),
        near_sdk::serde_json::to_string(amount_zatoshis).expect("serialize amount"),
        expires_at_unix,
    );
    hex_string(&env::sha256(canonical.as_bytes()))
}

fn now_unix() -> u64 {
    env::block_timestamp() / 1_000_000_000
}

fn hex_string(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests;
