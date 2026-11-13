use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    collections::LookupMap,
    env, near, AccountId, BorshStorageKey, PanicOnDefault,
};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::serde_json::json;

const HASH_DOMAIN: &str = "QPC-NEAR-ZCASH-POLICY-ARGS-V1\0";
const ALLOWED_ACTION: &str = "pay_zcash_shielded";
const ALLOWED_DESTINATION: &str = "zcash:testnet";
const ALLOWED_PRIVACY: &str = "SHIELDED_ONLY";

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    Agents,
    UsedPayments,
}

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct AgentPolicy {
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
        }
    }

    pub fn configure_agent(
        &mut self,
        agent: AccountId,
        policy_id: String,
        router: AccountId,
        expires_at_unix: u64,
        max_per_payment: u128,
        total_cap: u128,
    ) {
        self.assert_owner();
        assert!(agent != self.owner, "owner cannot be an agent");
        assert!(!policy_id.is_empty() && policy_id.len() <= 128, "invalid policy id");
        assert!(expires_at_unix > now_unix(), "policy already expired");
        assert!(max_per_payment > 0 && total_cap >= max_per_payment, "invalid spending limits");

        let next_version = self.agents.get(&agent)
            .map(|old| old.policy_version.checked_add(1).expect("policy version overflow"))
            .unwrap_or(1);
        self.agents.insert(&agent, &AgentPolicy {
            policy_id,
            router,
            expires_at_unix,
            max_per_payment,
            total_cap,
            spent: 0,
            next_nonce: 0,
            policy_version: next_version,
        });
    }

    pub fn revoke_agent(&mut self, agent: AccountId) {
        self.assert_owner();
        self.agents.remove(&agent);
    }

    pub fn get_agent_policy(&self, agent: AccountId) -> Option<AgentPolicyView> {
        self.agents.get(&agent).map(|p| AgentPolicyView {
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

    /// Records a policy-authorized cross-chain action. This does not call a
    /// router, sign a wallet transaction, or prove Zcash settlement.
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
        let agent = env::predecessor_account_id();
        let mut policy = self.agents.get(&agent).expect("agent is not configured");
        let now = now_unix();

        assert!(!request_id.is_empty() && request_id.len() <= 128, "invalid request id");
        assert!(policy.expires_at_unix > now, "agent policy expired");
        assert!(expires_at_unix > now && expires_at_unix <= policy.expires_at_unix,
            "payment intent expired or exceeds policy expiry");
        assert_eq!(policy_id, policy.policy_id, "policy id mismatch");
        assert_eq!(policy_version, policy.policy_version, "stale policy version");
        assert_eq!(nonce, policy.next_nonce, "invalid or replayed nonce");
        assert_eq!(action_id, ALLOWED_ACTION, "action is not allowed");
        assert_eq!(destination_chain, ALLOWED_DESTINATION, "destination chain is not allowed");
        assert_eq!(destination_router, policy.router, "destination router is not allowed");
        assert_eq!(privacy_policy, ALLOWED_PRIVACY, "shielded-only policy required");
        assert!(!payment_id.is_empty() && payment_id.len() <= 128, "invalid payment id");
        assert!(!opaque_recipient_ref.is_empty() && opaque_recipient_ref.len() <= 128,
            "invalid opaque recipient reference");
        assert!(self.used_payments.get(&payment_id).is_none(), "payment id already used");

        let amount: u128 = amount_zatoshis.parse().expect("invalid zatoshi amount");
        assert!(amount > 0 && amount.to_string() == amount_zatoshis, "non-canonical amount");
        assert!(amount <= policy.max_per_payment, "per-payment limit exceeded");
        let new_spent = policy.spent.checked_add(amount).expect("spending overflow");
        assert!(new_spent <= policy.total_cap, "cumulative spending cap exceeded");

        let canonical = format!(
            "{}{{\"paymentId\":{},\"opaqueRecipientRef\":{},\"amountZatoshis\":{},\"expiresAtUnix\":{}}}",
            HASH_DOMAIN,
            near_sdk::serde_json::to_string(&payment_id).expect("serialize payment id"),
            near_sdk::serde_json::to_string(&opaque_recipient_ref).expect("serialize recipient ref"),
            near_sdk::serde_json::to_string(&amount_zatoshis).expect("serialize amount"),
            expires_at_unix,
        );
        let expected_hash = hex_string(&env::sha256(canonical.as_bytes()));
        assert_eq!(policy_args_hash, expected_hash, "policy argument hash mismatch");

        policy.spent = new_spent;
        policy.next_nonce = policy.next_nonce.checked_add(1).expect("nonce overflow");
        self.agents.insert(&agent, &policy);
        self.used_payments.insert(&payment_id, &true);

        env::log_str(&format!(
            "EVENT_JSON:{}",
            json!({
                "standard": "qpc",
                "version": "1.0.0",
                "event": "agent_action_authorized",
                "data": [{
                    "agent": agent,
                    "request_id": request_id,
                    "payment_id": payment_id,
                    "destination_chain": ALLOWED_DESTINATION,
                    "amount_zatoshis": amount_zatoshis,
                    "nonce": nonce,
                    "policy_version": policy_version,
                    "privacy_policy": ALLOWED_PRIVACY,
                    "policy_args_hash": expected_hash
                }]
            })
        ));

        DispatchReceipt {
            state: "POLICY_AUTHORIZED".to_string(),
            payment_id,
            amount_zatoshis,
            nonce,
            policy_version,
            policy_args_hash: expected_hash,
        }
    }

    fn assert_owner(&self) {
        assert_eq!(env::predecessor_account_id(), self.owner, "owner only");
    }
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
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::{testing_env, AccountId};

    const NOW: u64 = 1_800_000_000;
    const EXPECTED_HASH: &str = "5fb8d28e7147d27a86946efb815d27cd4e8c8626d5d12c4c9c3ba0df0a8f758f";

    fn account(value: &str) -> AccountId {
        value.parse().unwrap()
    }

    fn context(caller: &str, timestamp: u64) {
        let context = VMContextBuilder::new()
            .current_account_id(account("qpc.testnet"))
            .predecessor_account_id(account(caller))
            .block_timestamp(timestamp * 1_000_000_000)
            .build();
        testing_env!(context);
    }

    fn setup() -> QPCNear {
        context("owner.testnet", NOW);
        let mut contract = QPCNear::new(account("owner.testnet"));
        contract.configure_agent(
            account("agent.testnet"),
            "policy-main".to_string(),
            account("router.testnet"),
            NOW + 10_000,
            5_000,
            10_000,
        );
        contract
    }

    fn dispatch(contract: &mut QPCNear, caller: &str, nonce: u64, amount: &str, hash: &str) -> DispatchReceipt {
        context(caller, NOW);
        contract.dispatch_action(
            format!("request-{nonce}"),
            "policy-main".to_string(),
            1,
            nonce,
            ALLOWED_ACTION.to_string(),
            ALLOWED_DESTINATION.to_string(),
            account("router.testnet"),
            format!("pay-{nonce:03}"),
            amount.to_string(),
            "recipient-opaque-1".to_string(),
            NOW + 100,
            ALLOWED_PRIVACY.to_string(),
            hash.to_string(),
        )
    }

    #[test]
    fn authorizes_policy_bound_shielded_dispatch() {
        let mut contract = setup();
        let receipt = dispatch(&mut contract, "agent.testnet", 0, "1000", EXPECTED_HASH);
        assert_eq!(receipt.state, "POLICY_AUTHORIZED");
        assert_eq!(receipt.payment_id, "pay-000");
        let policy = contract.get_agent_policy(account("agent.testnet")).unwrap();
        assert_eq!(policy.spent, "1000");
        assert_eq!(policy.next_nonce, 1);
    }

    #[test]
    #[should_panic(expected = "owner only")]
    fn rejects_non_owner_policy_changes() {
        let mut contract = setup();
        context("attacker.testnet", NOW);
        contract.configure_agent(
            account("victim.testnet"), "evil".to_string(), account("router.testnet"),
            NOW + 100, 100, 100,
        );
    }

    #[test]
    #[should_panic(expected = "agent is not configured")]
    fn rejects_unconfigured_caller() {
        let mut contract = setup();
        dispatch(&mut contract, "attacker.testnet", 0, "1000", EXPECTED_HASH);
    }

    #[test]
    #[should_panic(expected = "destination router is not allowed")]
    fn rejects_router_substitution() {
        let mut contract = setup();
        context("agent.testnet", NOW);
        contract.dispatch_action(
            "request-0".into(), "policy-main".into(), 1, 0, ALLOWED_ACTION.into(),
            ALLOWED_DESTINATION.into(), account("attacker.testnet"), "pay-000".into(),
            "1000".into(), "recipient-opaque-1".into(), NOW + 100,
            ALLOWED_PRIVACY.into(), EXPECTED_HASH.into(),
        );
    }

    #[test]
    #[should_panic(expected = "policy argument hash mismatch")]
    fn rejects_tampered_payment_arguments() {
        let mut contract = setup();
        dispatch(&mut contract, "agent.testnet", 0, "1001", EXPECTED_HASH);
    }

    #[test]
    #[should_panic(expected = "invalid or replayed nonce")]
    fn rejects_nonce_replay() {
        let mut contract = setup();
        dispatch(&mut contract, "agent.testnet", 0, "1000", EXPECTED_HASH);
        dispatch(&mut contract, "agent.testnet", 0, "1000", EXPECTED_HASH);
    }

    #[test]
    #[should_panic(expected = "per-payment limit exceeded")]
    fn enforces_per_payment_limit() {
        let mut contract = setup();
        dispatch(&mut contract, "agent.testnet", 0, "5001", EXPECTED_HASH);
    }

    #[test]
    fn rejects_expired_policy() {
        let mut contract = setup();
        context("agent.testnet", NOW + 10_001);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            contract.dispatch_action(
                "request-0".into(), "policy-main".into(), 1, 0, ALLOWED_ACTION.into(),
                ALLOWED_DESTINATION.into(), account("router.testnet"), "pay-000".into(),
                "1000".into(), "recipient-opaque-1".into(), NOW + 100,
                ALLOWED_PRIVACY.into(), EXPECTED_HASH.into(),
            )
        }));
        assert!(result.is_err());
    }
}
