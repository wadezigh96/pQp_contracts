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
    env, near, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault, Promise, PromiseResult,
};

#[cfg(test)]
const HASH_DOMAIN: &str = "QPC-NEAR-ZCASH-POLICY-ARGS-V1\0";

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    Agents,
    UsedPayments,
    AllowedRecipients,
    EnvelopeProfiles,
    AuthorizationReceipts,
    ExecutionResults,
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

/// Durable evidence of native predecessor authorization, not payment execution.
/// This record is historical: expiry or later revocation never changes its state
/// into an executable capability or a settlement proof.
#[derive(BorshDeserialize, BorshSerialize, Serialize, Clone)]
#[serde(crate = "near_sdk::serde", rename_all = "camelCase")]
pub struct AuthorizationReceipt {
    pub state: String,
    pub execution_status: String,
    pub intent_id: String,
    pub authorization_commitment: String,
    pub agent_id: AccountId,
    pub adapter_id: AccountId,
    pub nonce: String,
    pub policy_id: String,
    pub policy_version: u32,
    pub policy_commitment: String,
    pub revocation_epoch: u64,
    pub authorized_at_unix: u64,
    pub expires_at_unix: u64,
    pub block_height: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Clone)]
#[serde(crate = "near_sdk::serde", rename_all = "camelCase")]
pub struct ExecutionResult {
    pub state: String,
    pub intent_id: String,
    pub authorization_commitment: String,
    pub recipient: AccountId,
    pub amount_atomic: String,
    pub authorized_at_unix: u64,
    pub completed_at_unix: Option<u64>,
    pub contract_fee_atomic: String,
    pub protocol_gas_fee: String,
    pub fee_budget_enforced: bool,
    pub refund_state: String,
    pub refund_attempt: u64,
}

// Every root read (including generated ABI wrappers) validates this header
// before interpreting collection prefixes. Bump on ANY incompatible root or
// collection key/value encoding change; a successful Borsh decode alone is not
// evidence of compatibility. There is deliberately no automatic migration.
const STATE_MAGIC: [u8; 16] = *b"QPCNEAR\0LAYOUT\0\0";
const STATE_LAYOUT_VERSION: u32 = 1;
const INTENDED_OWNER: &str = "wkams.testnet";

struct StateLayout;

impl BorshSerialize for StateLayout {
    fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
        BorshSerialize::serialize(&STATE_MAGIC, writer)?;
        BorshSerialize::serialize(&STATE_LAYOUT_VERSION, writer)
    }
}

impl BorshDeserialize for StateLayout {
    fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
        let magic = <[u8; 16]>::deserialize_reader(reader)?;
        if magic != STATE_MAGIC {
            return Err(borsh::io::Error::new(
                borsh::io::ErrorKind::InvalidData,
                "incompatible QPC state layout: missing magic; migration required",
            ));
        }
        let version = u32::deserialize_reader(reader)?;
        if version != STATE_LAYOUT_VERSION {
            return Err(borsh::io::Error::new(
                borsh::io::ErrorKind::InvalidData,
                "unsupported QPC state layout version; migration required",
            ));
        }
        Ok(Self)
    }
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct QPCNear {
    layout: StateLayout,
    owner: AccountId,
    agents: LookupMap<AccountId, AgentPolicy>,
    // Fresh-state layout: intent names are scoped to their native agent.
    used_payments: LookupMap<(AccountId, String), String>,
    allowed_recipients: LookupMap<(AccountId, String), bool>,
    envelope_profiles: LookupMap<AccountId, String>,
    authorization_receipts: LookupMap<String, AuthorizationReceipt>,
    execution_results: LookupMap<String, ExecutionResult>,
}

#[near]
impl QPCNear {
    #[init]
    pub fn new(owner: AccountId) -> Self {
        assert!(!env::state_exists(), "already initialized");
        assert_eq!(owner.as_str(), INTENDED_OWNER, "unexpected owner");
        Self {
            layout: StateLayout,
            owner,
            agents: LookupMap::new(StorageKey::Agents),
            used_payments: LookupMap::new(StorageKey::UsedPayments),
            allowed_recipients: LookupMap::new(StorageKey::AllowedRecipients),
            envelope_profiles: LookupMap::new(StorageKey::EnvelopeProfiles),
            authorization_receipts: LookupMap::new(StorageKey::AuthorizationReceipts),
            execution_results: LookupMap::new(StorageKey::ExecutionResults),
        }
    }

    /// Reports only a successfully decoded supported root. Incompatible state
    /// fails in the generated ABI wrapper before this method is entered.
    pub fn get_state_layout_version(&self) -> u32 {
        STATE_LAYOUT_VERSION
    }

    /// Read-only verification of contract-storage ownership. This does not
    /// grant or revoke NEAR account keys and cannot control deployment rights.
    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
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
            self.used_payments
                .get(&(agent.clone(), intent.to_owned()))
                .is_none(),
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
        self.used_payments
            .insert(&(agent.clone(), intent.to_owned()), &expected);
        let record = AuthorizationReceipt {
            state: "AUTHORIZED".into(),
            execution_status: "NOT_EXECUTED".into(),
            intent_id: intent.to_owned(),
            authorization_commitment: expected,
            agent_id: agent,
            adapter_id: env::current_account_id(),
            nonce: string(&envelope["nonce"]).to_owned(),
            policy_id: policy.policy_id,
            policy_version: policy.policy_version,
            policy_commitment: string(&envelope["policy"]["commitment"]).to_owned(),
            revocation_epoch: integer(&envelope["cancellation"]["revocationEpoch"]),
            authorized_at_unix: now,
            expires_at_unix: expiry,
            block_height: env::block_height(),
        };
        self.authorization_receipts
            .insert(&record.authorization_commitment, &record);
        let receipt = near_sdk::serde_json::to_value(record).expect("serialize receipt");
        // No raw recipient, amounts, or envelope are duplicated in the event.
        env::log_str(&format!(
            "EVENT_JSON:{}",
            json!({"standard":"qpc", "version":"1.0.0",
            "event":"agent_action_authorized", "data":[receipt.clone()]})
        ));
        receipt
    }

    /// Same-chain testnet-only native transfer, funded by the native agent's
    /// exact attached deposit. Protocol gas is caller-paid, not an adapter fee.
    /// Failure consumes authorization permanently. The original agent can claim
    /// returned funds through the separately tracked failure-only refund path.
    #[payable]
    pub fn execute_near_envelope(
        &mut self,
        envelope: Value,
        authorization_commitment: String,
        recipient: AccountId,
    ) -> Promise {
        validate_envelope(&envelope);
        for section in ["source", "destination"] {
            assert_eq!(
                string(&envelope[section]["networkId"]),
                "near:testnet",
                "testnet only"
            );
            assert_eq!(
                string(&envelope[section]["nativeChainId"]),
                "testnet",
                "testnet only"
            );
        }
        assert_eq!(
            string(&envelope["destination"]["adapterId"]),
            env::current_account_id().as_str(),
            "same contract only"
        );
        assert_eq!(
            string(&envelope["action"]["type"]),
            "asset-transfer",
            "unsupported action"
        );
        assert_eq!(
            string(&envelope["action"]["operation"]),
            "pay",
            "unsupported operation"
        );
        for key in ["sourceAssetId", "destinationAssetId"] {
            assert_eq!(
                string(&envelope["asset"][key]),
                "near:native",
                "native NEAR only"
            );
        }
        for key in ["sourceDecimals", "destinationDecimals"] {
            assert_eq!(
                integer(&envelope["asset"][key]),
                24,
                "NEAR decimals mismatch"
            );
        }
        assert_eq!(
            string(&envelope["asset"]["mappingId"]),
            "near-native-identity-v1",
            "identity mapping required"
        );
        let amount = decimal(&envelope["amount"]["sourceAtomic"], true);
        assert_eq!(
            amount,
            decimal(&envelope["amount"]["destinationAtomic"], true),
            "amount mismatch"
        );
        assert_eq!(
            string(&envelope["fees"]["feeAssetId"]),
            "near:native",
            "fee asset mismatch"
        );
        // The host does not expose gas price. The owner-approved all-in fee
        // budget stays bound in the envelope but must be checked independently
        // against actual transaction/receipt tokens_burnt; this result does not
        // attest fee-budget conformance.
        assert_eq!(
            integer(&envelope["fees"]["maxSlippageBps"]),
            0,
            "slippage forbidden"
        );
        assert_eq!(
            string(&envelope["settlement"]["evidenceClass"]),
            "near-native-transfer-result",
            "unsupported evidence class"
        );
        assert_eq!(
            string(&envelope["settlement"]["finalityPolicy"]),
            "near-execution-outcome",
            "unsupported result policy"
        );
        assert_eq!(
            integer(&envelope["settlement"]["minConfirmations"]),
            0,
            "not a finality verifier"
        );
        assert_ne!(
            recipient,
            env::current_account_id(),
            "self transfer forbidden"
        );
        let recipient_hash = hex_string(&env::sha256(
            format!(
                "QPC-NATIVE-RECIPIENT-V1\0{}",
                canonical_json(
                    &json!({"chain":"near", "nativeChainId":"testnet", "recipient":recipient})
                )
            )
            .as_bytes(),
        ));
        assert_eq!(
            string(&envelope["destination"]["recipientCommitment"]),
            recipient_hash,
            "recipient commitment mismatch"
        );
        assert_eq!(
            env::attached_deposit().as_yoctonear(),
            amount,
            "exact transfer deposit required"
        );
        assert!(
            env::prepaid_gas() >= Gas::from_tgas(40),
            "at least 40 Tgas required"
        );
        // Retain existing storage stake plus a bounded 16 KiB receipt reserve.
        // The envelope has a 16 KiB bound and all persisted fields are bounded.
        let storage_bytes = env::storage_usage()
            .checked_add(16_384)
            .expect("storage overflow");
        let reserve = env::storage_byte_cost()
            .as_yoctonear()
            .checked_mul(storage_bytes as u128)
            .expect("storage cost overflow");
        assert!(
            env::account_balance()
                .as_yoctonear()
                .checked_sub(amount)
                .is_some_and(|balance| balance >= reserve),
            "insufficient storage reserve"
        );
        let intent_id = string(&envelope["intentId"]).to_owned();
        assert!(
            self.execution_results
                .get(&authorization_commitment)
                .is_none(),
            "execution already exists"
        );
        self.authorize_envelope(envelope, authorization_commitment.clone());
        let result = ExecutionResult {
            state: "PENDING".into(),
            intent_id: intent_id.clone(),
            authorization_commitment: authorization_commitment.clone(),
            recipient: recipient.clone(),
            amount_atomic: amount.to_string(),
            authorized_at_unix: now_unix(),
            completed_at_unix: None,
            contract_fee_atomic: "0".into(),
            protocol_gas_fee: "NOT_VERIFIED".into(),
            fee_budget_enforced: false,
            refund_state: "NOT_APPLICABLE".into(),
            refund_attempt: 0,
        };
        self.execution_results
            .insert(&authorization_commitment, &result);
        Promise::new(recipient)
            .transfer(NearToken::from_yoctonear(amount))
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(10))
                    .with_unused_gas_weight(0)
                    .on_near_transfer_result(intent_id, authorization_commitment),
            )
    }

    /// Only the contract's own promise callback can record a native outcome.
    /// EXECUTED describes this transfer result, never cross-chain settlement or
    /// independent chain finality. Duplicate callbacks cannot rewrite history.
    #[private]
    pub fn on_near_transfer_result(
        &mut self,
        intent_id: String,
        authorization_commitment: String,
    ) -> ExecutionResult {
        assert_eq!(
            env::predecessor_account_id(),
            env::current_account_id(),
            "private callback only"
        );
        assert_eq!(
            env::promise_results_count(),
            1,
            "one transfer result required"
        );
        let mut result = self
            .execution_results
            .get(&authorization_commitment)
            .expect("unknown execution");
        assert_eq!(result.intent_id, intent_id, "intent mismatch");
        assert_eq!(
            result.authorization_commitment, authorization_commitment,
            "authorization commitment mismatch"
        );
        assert_eq!(result.state, "PENDING", "execution already completed");
        result.state = match env::promise_result(0) {
            PromiseResult::Successful(bytes) if bytes.is_empty() => "EXECUTED",
            PromiseResult::Failed => {
                result.refund_state = "AVAILABLE".into();
                "FAILED"
            }
            // An unexpected successful return does not prove returned funds.
            // Fail closed without enabling a potentially double-paying refund.
            _ => {
                result.refund_state = "BLOCKED_INVALID_RESULT".into();
                "FAILED"
            }
        }
        .into();
        result.completed_at_unix = Some(now_unix());
        self.execution_results
            .insert(&authorization_commitment, &result);
        env::log_str(&format!(
            "EVENT_JSON:{}",
            json!({"standard":"qpc", "version":"1.0.0", "event":"near_transfer_result", "data":[result.clone()]})
        ));
        result
    }

    /// Refund only funds returned by a failed native transfer. This never
    /// releases the original payment nonce, intent ID, or spending reservation.
    pub fn claim_failed_near_refund(&mut self, intent_id: String) -> Promise {
        let authorization_commitment = self
            .used_payments
            .get(&(env::predecessor_account_id(), intent_id.clone()))
            .expect("unknown agent intent");
        let receipt = self
            .authorization_receipts
            .get(&authorization_commitment)
            .expect("unknown authorization");
        assert_eq!(
            env::predecessor_account_id(),
            receipt.agent_id,
            "original agent only"
        );
        let mut result = self
            .execution_results
            .get(&authorization_commitment)
            .expect("unknown execution");
        assert_eq!(
            result.authorization_commitment, receipt.authorization_commitment,
            "authorization commitment mismatch"
        );
        assert_eq!(
            result.state, "FAILED",
            "only failed execution is refundable"
        );
        assert!(
            result.refund_state == "AVAILABLE" || result.refund_state == "RETRYABLE",
            "refund unavailable or pending"
        );
        assert!(
            env::prepaid_gas() >= Gas::from_tgas(30),
            "at least 30 Tgas required"
        );
        let amount = result
            .amount_atomic
            .parse::<u128>()
            .expect("stored amount invalid");
        let reserve = env::storage_byte_cost()
            .as_yoctonear()
            .checked_mul(env::storage_usage() as u128 + 1024)
            .expect("storage cost overflow");
        assert!(
            env::account_balance()
                .as_yoctonear()
                .checked_sub(amount)
                .is_some_and(|balance| balance >= reserve),
            "insufficient refund storage reserve"
        );
        result.refund_attempt = result
            .refund_attempt
            .checked_add(1)
            .expect("refund attempt overflow");
        result.refund_state = "PENDING".into();
        self.execution_results
            .insert(&authorization_commitment, &result);
        Promise::new(receipt.agent_id)
            .transfer(NearToken::from_yoctonear(amount))
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(Gas::from_tgas(10))
                    .with_unused_gas_weight(0)
                    .on_near_refund_result(
                        intent_id,
                        result.authorization_commitment,
                        result.refund_attempt,
                    ),
            )
    }

    #[private]
    pub fn on_near_refund_result(
        &mut self,
        intent_id: String,
        authorization_commitment: String,
        refund_attempt: u64,
    ) -> ExecutionResult {
        assert_eq!(
            env::predecessor_account_id(),
            env::current_account_id(),
            "private callback only"
        );
        assert_eq!(
            env::promise_results_count(),
            1,
            "one refund result required"
        );
        let mut result = self
            .execution_results
            .get(&authorization_commitment)
            .expect("unknown execution");
        assert_eq!(result.intent_id, intent_id, "intent mismatch");
        assert_eq!(
            result.authorization_commitment, authorization_commitment,
            "authorization commitment mismatch"
        );
        assert_eq!(
            result.state, "FAILED",
            "only failed execution is refundable"
        );
        assert_eq!(result.refund_state, "PENDING", "refund not pending");
        assert_eq!(
            result.refund_attempt, refund_attempt,
            "stale refund attempt"
        );
        result.refund_state = match env::promise_result(0) {
            PromiseResult::Successful(bytes) if bytes.is_empty() => "REFUNDED",
            PromiseResult::Failed => "RETRYABLE",
            _ => "BLOCKED_INVALID_RESULT",
        }
        .into();
        self.execution_results
            .insert(&authorization_commitment, &result);
        env::log_str(&format!(
            "EVENT_JSON:{}",
            json!({"standard":"qpc", "version":"1.0.0", "event":"near_refund_result", "data":[result.clone()]})
        ));
        result
    }

    pub fn get_execution_result(
        &self,
        agent_id: AccountId,
        intent_id: String,
    ) -> Option<ExecutionResult> {
        let commitment = self.used_payments.get(&(agent_id, intent_id))?;
        self.execution_results.get(&commitment)
    }

    /// Public historical record only. It does not prove current policy validity,
    /// destination execution, canonical finality, or shielded receipt of funds.
    pub fn get_authorization_receipt(
        &self,
        agent_id: AccountId,
        intent_id: String,
    ) -> Option<AuthorizationReceipt> {
        let commitment = self.used_payments.get(&(agent_id, intent_id))?;
        self.authorization_receipts.get(&commitment)
    }

    /// Deliberately no success path: neither a commitment nor an injected result
    /// proves native destination execution. No untrusted result is persisted.
    pub fn settle_envelope(&mut self, intent_id: String, authorization_commitment: String) {
        let receipt = self
            .authorization_receipts
            .get(&authorization_commitment)
            .expect("unknown intent");
        assert_eq!(receipt.intent_id, intent_id, "intent mismatch");
        assert_eq!(
            receipt.authorization_commitment, authorization_commitment,
            "authorization commitment mismatch"
        );
        panic!("native external settlement not implemented; authorization is not execution");
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
