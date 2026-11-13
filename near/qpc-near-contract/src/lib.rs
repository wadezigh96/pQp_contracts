use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, near, AccountId, PanicOnDefault};
use std::collections::{BTreeMap, BTreeSet};

const ACTION_ID: &str = "pay_zcash_shielded";
const DESTINATION_CHAIN: &str = "zcash:testnet";
const PRIVACY_POLICY: &str = "SHIELDED_ONLY";
const HASH_DOMAIN: &str = "QPC-NEAR-ZCASH-POLICY-ARGS-V1\0";

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct AgentPolicy {
    pub policy_id: String,
    pub policy_version: u32,
    pub zcash_router_account: AccountId,
    pub max_amount_zatoshis: u128,
    pub expires_at_ns: u64,
    pub active: bool,
    pub next_nonce: u64,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct QpcNearContract {
    owner_id: AccountId,
    policies: BTreeMap<AccountId, AgentPolicy>,
    used_payment_ids: BTreeSet<String>,
}

#[near]
impl QpcNearContract {
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        assert!(!env::state_exists(), "already initialized");
        Self { owner_id, policies: BTreeMap::new(), used_payment_ids: BTreeSet::new() }
    }

    pub fn set_agent_policy(
        &mut self,
        agent_id: AccountId,
        policy_id: String,
        policy_version: u32,
        zcash_router_account: AccountId,
        max_amount_zatoshis: String,
        expires_at_ns: u64,
    ) {
        assert_eq!(env::predecessor_account_id(), self.owner_id, "ERR_OWNER_ONLY");
        assert_ne!(agent_id, self.owner_id, "ERR_INVALID_AGENT");
        assert_ne!(agent_id, env::current_account_id(), "ERR_INVALID_AGENT");
        assert!(!policy_id.is_empty() && policy_id.len() <= 128, "ERR_POLICY_ID");
        assert!(policy_version > 0, "ERR_POLICY_VERSION");
        assert!(!zcash_router_account.as_str().is_empty(), "ERR_ROUTER");
        let cap = parse_positive_u128(&max_amount_zatoshis);
        assert!(expires_at_ns > env::block_timestamp(), "ERR_POLICY_EXPIRED");
        let previous_nonce = self.policies.get(&agent_id).map(|p| p.next_nonce).unwrap_or(0);
        self.policies.insert(agent_id, AgentPolicy {
            policy_id, policy_version, zcash_router_account,
            max_amount_zatoshis: cap, expires_at_ns, active: true,
            next_nonce: previous_nonce,
        });
    }

    pub fn revoke_agent(&mut self, agent_id: AccountId) {
        assert_eq!(env::predecessor_account_id(), self.owner_id, "ERR_OWNER_ONLY");
        let policy = self.policies.get_mut(&agent_id).expect("ERR_POLICY_NOT_FOUND");
        policy.active = false;
    }

    pub fn get_agent_policy(&self, agent_id: AccountId) -> Option<AgentPolicy> {
        self.policies.get(&agent_id).cloned()
    }

    #[payable]
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
    ) -> bool {
        // Intentionally failing test seam. The policy checks are added in the next TDD slice.
        let _ = (request_id, policy_id, policy_version, nonce, action_id,
            destination_chain, destination_router, payment_id, amount_zatoshis,
            opaque_recipient_ref, expires_at_unix, privacy_policy, policy_args_hash);
        false
    }
}

fn parse_positive_u128(value: &str) -> u128 {
    assert!(!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()), "ERR_AMOUNT_FORMAT");
    assert!(value == "0" || !value.starts_with('0'), "ERR_AMOUNT_FORMAT");
    let amount = value.parse::<u128>().expect("ERR_AMOUNT_RANGE");
    assert!(amount > 0, "ERR_AMOUNT_ZERO");
    amount
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::{testing_env, NearToken};

    const OWNER: &str = "owner.testnet";
    const AGENT: &str = "agent.testnet";
    const CONTRACT: &str = "qpc.testnet";
    const ROUTER: &str = "router.testnet";
    const NOW_NS: u64 = 1_800_000_000_000_000_000;
    const EXPIRY_UNIX: u64 = 1_800_000_600;

    fn account(value: &str) -> AccountId { value.parse().unwrap() }

    fn context(predecessor: &str) {
        let mut builder = VMContextBuilder::new();
        builder.current_account_id(account(CONTRACT))
            .signer_account_id(account(predecessor))
            .predecessor_account_id(account(predecessor))
            .block_timestamp(NOW_NS)
            .attached_deposit(NearToken::from_yoctonear(0));
        testing_env!(builder.build());
    }

    fn setup() -> QpcNearContract {
        context(OWNER);
        let mut contract = QpcNearContract::new(account(OWNER));
        contract.set_agent_policy(
            account(AGENT), "policy-1".into(), 1, account(ROUTER),
            "50000".into(), NOW_NS + 3_600_000_000_000,
        );
        contract
    }

    fn args_hash() -> String {
        // SHA-256 of the canonical policy-intent encoding documented in the JS router.
        "REPLACE_HASH_VECTOR".into()
    }

    fn dispatch(contract: &mut QpcNearContract, nonce: u64, payment_id: &str, hash: String) -> bool {
        context(AGENT);
        contract.dispatch_action(
            "request-1".into(), "policy-1".into(), 1, nonce,
            ACTION_ID.into(), DESTINATION_CHAIN.into(), account(ROUTER),
            payment_id.into(), "25000".into(), "opaque-recipient-1".into(),
            EXPIRY_UNIX, PRIVACY_POLICY.into(), hash,
        )
    }

    #[test]
    fn accepts_authorized_shielded_dispatch_and_consumes_nonce() {
        let mut contract = setup();
        assert!(dispatch(&mut contract, 0, "payment-1", args_hash()));
        let policy = contract.get_agent_policy(account(AGENT)).unwrap();
        assert_eq!(policy.next_nonce, 1);
    }

    #[test]
    #[should_panic(expected = "ERR_OWNER_ONLY")]
    fn rejects_dispatch_from_non_agent() {
        let mut contract = setup();
        context("attacker.testnet");
        contract.dispatch_action(
            "request-1".into(), "policy-1".into(), 1, 0,
            ACTION_ID.into(), DESTINATION_CHAIN.into(), account(ROUTER),
            "payment-1".into(), "25000".into(), "opaque-recipient-1".into(),
            EXPIRY_UNIX, PRIVACY_POLICY.into(), args_hash(),
        );
    }

    #[test]
    fn rejects_replayed_payment_id() {
        let mut contract = setup();
        assert!(dispatch(&mut contract, 0, "payment-1", args_hash()));
        assert!(!dispatch(&mut contract, 1, "payment-1", args_hash()));
    }
}
