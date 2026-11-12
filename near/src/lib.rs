use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, UnorderedSet};
use near_sdk::json_types::{U128, U64};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseError,
};

const DAY_NS: u64 = 86_400_000_000_000;
const MAX_POLICY_DAYS: u64 = 30;
const GAS_FOR_CALLBACK: Gas = Gas::from_tgas(10);

#[derive(Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
#[derive(BorshDeserialize, BorshSerialize)]
#[borsh(crate = "near_sdk::borsh")]
pub struct AgentPolicyView {
    pub active: bool,
    pub expires_at_ns: U64,
    pub native_cap: U128,
    pub native_spent: U128,
    pub per_tx_limit: U128,
    pub pending_spend: U128,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[derive(BorshDeserialize, BorshSerialize)]
#[borsh(crate = "near_sdk::borsh")]
pub struct SpendRequest {
    pub request_id: U64,
    pub agent_id: AccountId,
    pub recipient_id: AccountId,
    pub amount: U128,
    pub created_at_ns: U64,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    owner_id: AccountId,
    paused: bool,
    agents: LookupMap<AccountId, AgentPolicyView>,
    recipients: LookupMap<AccountId, UnorderedSet<AccountId>>,
    next_request_id: u64,
    requests: LookupMap<u64, SpendRequest>,
}

#[near]
impl Contract {
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        require!(!env::state_exists(), "Already initialized");
        Self {
            owner_id,
            paused: false,
            agents: LookupMap::new(b"a"),
            recipients: LookupMap::new(b"r"),
            next_request_id: 0,
            requests: LookupMap::new(b"q"),
        }
    }

    pub fn owner_id(&self) -> AccountId {
        self.owner_id.clone()
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn get_agent_policy(&self, agent_id: AccountId) -> Option<AgentPolicyView> {
        self.agents.get(&agent_id)
    }

    pub fn is_allowed_recipient(&self, agent_id: AccountId, recipient_id: AccountId) -> bool {
        self.recipients
            .get(&agent_id)
            .map(|set| set.contains(&recipient_id))
            .unwrap_or(false)
    }

    pub fn get_spend_request(&self, request_id: U64) -> Option<SpendRequest> {
        self.requests.get(&request_id.0)
    }

    pub fn configure_agent(
        &mut self,
        agent_id: AccountId,
        expires_at_ns: U64,
        native_cap: U128,
        per_tx_limit: U128,
    ) {
        self.assert_owner();
        require!(agent_id != self.owner_id, "Owner cannot be an agent");
        let now = env::block_timestamp();
        let expiry = expires_at_ns.0;
        require!(expiry > now, "Expiry must be in the future");
        require!(
            expiry <= now.saturating_add(MAX_POLICY_DAYS * DAY_NS),
            "Policy exceeds 30 days"
        );
        require!(native_cap.0 > 0, "Cap must be positive");
        require!(
            per_tx_limit.0 > 0 && per_tx_limit.0 <= native_cap.0,
            "Invalid per-transaction limit"
        );

        let previous = self.agents.get(&agent_id);
        if let Some(policy) = previous {
            require!(
                !policy.active || policy.expires_at_ns.0 <= now,
                "Revoke active policy before reconfiguration"
            );
            require!(
                policy.pending_spend.0 == 0,
                "Cannot reconfigure with pending spend"
            );
        }
        self.agents.insert(
            &agent_id,
            &AgentPolicyView {
                active: true,
                expires_at_ns,
                native_cap,
                native_spent: U128(0),
                per_tx_limit,
                pending_spend: U128(0),
            },
        );
        env::log_str("QPC: agent policy configured");
    }

    pub fn set_recipient(&mut self, agent_id: AccountId, recipient_id: AccountId, allowed: bool) {
        self.assert_owner();
        require!(agent_id != self.owner_id, "Owner cannot be an agent");
        if allowed {
            let mut set = self
                .recipients
                .get(&agent_id)
                .unwrap_or_else(|| UnorderedSet::new(storage_key_for_recipients(&agent_id)));
            set.insert(&recipient_id);
            self.recipients.insert(&agent_id, &set);
        } else if let Some(mut set) = self.recipients.get(&agent_id) {
            set.remove(&recipient_id);
            self.recipients.insert(&agent_id, &set);
        }
        env::log_str("QPC: recipient permission updated");
    }

    pub fn revoke_agent(&mut self, agent_id: AccountId) {
        self.assert_owner();
        let mut policy = self.agents.get(&agent_id).expect("Unknown agent");
        policy.active = false;
        self.agents.insert(&agent_id, &policy);
        env::log_str("QPC: agent revoked");
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.assert_owner();
        self.paused = paused;
        env::log_str(if paused {
            "QPC: paused"
        } else {
            "QPC: unpaused"
        });
    }

    #[payable]
    pub fn deposit(&mut self) {
        require!(
            env::attached_deposit().as_yoctonear() > 0,
            "Attach NEAR to deposit"
        );
        env::log_str("QPC: treasury funded");
    }

    pub fn request_spend(&mut self, recipient_id: AccountId, amount: U128) -> U64 {
        require!(!self.paused, "Contract is paused");
        let agent_id = env::predecessor_account_id();
        let now = env::block_timestamp();
        require!(amount.0 > 0, "Amount must be positive");

        let mut policy = self.agents.get(&agent_id).expect("Unknown agent");
        require!(policy.active, "Agent is revoked");
        require!(now < policy.expires_at_ns.0, "Policy expired");
        require!(
            self.is_allowed_recipient(agent_id.clone(), recipient_id.clone()),
            "Recipient not allowed"
        );
        require!(
            amount.0 <= policy.per_tx_limit.0,
            "Per-transaction limit exceeded"
        );
        let committed = policy
            .native_spent
            .0
            .checked_add(policy.pending_spend.0)
            .expect("Accounting overflow");
        require!(
            committed
                .checked_add(amount.0)
                .expect("Accounting overflow")
                <= policy.native_cap.0,
            "Cumulative cap exceeded"
        );
        require!(
            env::account_balance().as_yoctonear() >= amount.0,
            "Insufficient treasury balance"
        );

        policy.pending_spend = U128(
            policy
                .pending_spend
                .0
                .checked_add(amount.0)
                .expect("Accounting overflow"),
        );
        self.agents.insert(&agent_id, &policy);
        let request_id = self.next_request_id;
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .expect("Request ID overflow");
        let request = SpendRequest {
            request_id: U64(request_id),
            agent_id: agent_id.clone(),
            recipient_id: recipient_id.clone(),
            amount,
            created_at_ns: U64(now),
        };
        self.requests.insert(&request_id, &request);
        env::log_str(&format!("QPC: spend request {} pending", request_id));

        Promise::new(recipient_id)
            .transfer(NearToken::from_yoctonear(amount.0))
            .then(
                Promise::new(env::current_account_id()).function_call(
                    "resolve_spend".to_string(),
                    near_sdk::serde_json::to_vec(
                        &near_sdk::serde_json::json!({ "request_id": request_id }),
                    )
                    .unwrap(),
                    NearToken::from_yoctonear(0),
                    GAS_FOR_CALLBACK,
                ),
            )
            .detach();
        U64(request_id)
    }

    #[private]
    pub fn resolve_spend(
        &mut self,
        request_id: U64,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        let request = match self.requests.remove(&request_id.0) {
            Some(request) => request,
            None => return false,
        };
        let mut policy = match self.agents.get(&request.agent_id) {
            Some(policy) => policy,
            None => return false,
        };
        policy.pending_spend = U128(policy.pending_spend.0.saturating_sub(request.amount.0));
        let succeeded = result.is_ok();
        if succeeded {
            policy.native_spent = U128(
                policy
                    .native_spent
                    .0
                    .checked_add(request.amount.0)
                    .expect("Accounting overflow"),
            );
        }
        self.agents.insert(&request.agent_id, &policy);
        env::log_str(&format!(
            "QPC: spend request {} {}",
            request_id.0,
            if succeeded { "settled" } else { "failed" }
        ));
        succeeded
    }

    fn assert_owner(&self) {
        require!(env::predecessor_account_id() == self.owner_id, "Only owner");
    }
}

fn storage_key_for_recipients(agent_id: &AccountId) -> Vec<u8> {
    let mut key = b"recipient:".to_vec();
    key.extend_from_slice(agent_id.as_bytes());
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, NearToken};

    fn setup(predecessor: AccountId) {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(accounts(0))
            .signer_account_id(predecessor.clone())
            .predecessor_account_id(predecessor)
            .account_balance(NearToken::from_near(100))
            .block_timestamp(1_000_000_000_000);
        testing_env!(builder.build());
    }

    fn contract() -> Contract {
        setup(accounts(0));
        Contract::new(accounts(0))
    }

    fn configure(contract: &mut Contract, agent: AccountId) {
        let now = env::block_timestamp();
        contract.configure_agent(agent, U64(now + DAY_NS), U128(10), U128(4));
    }

    #[test]
    fn owner_can_configure_and_revoke_agent() {
        let mut contract = contract();
        configure(&mut contract, accounts(1));
        assert!(contract.get_agent_policy(accounts(1)).unwrap().active);
        contract.revoke_agent(accounts(1));
        assert!(!contract.get_agent_policy(accounts(1)).unwrap().active);
    }

    #[test]
    #[should_panic(expected = "Only owner")]
    fn non_owner_cannot_configure_agent() {
        let mut contract = contract();
        setup(accounts(1));
        contract.configure_agent(
            accounts(2),
            U64(env::block_timestamp() + DAY_NS),
            U128(10),
            U128(4),
        );
    }

    #[test]
    fn recipient_allowlist_can_be_revoked() {
        let mut contract = contract();
        contract.set_recipient(accounts(1), accounts(2), true);
        assert!(contract.is_allowed_recipient(accounts(1), accounts(2)));
        contract.set_recipient(accounts(1), accounts(2), false);
        assert!(!contract.is_allowed_recipient(accounts(1), accounts(2)));
    }

    #[test]
    #[should_panic(expected = "Policy exceeds 30 days")]
    fn policy_duration_is_bounded() {
        let mut contract = contract();
        let now = env::block_timestamp();
        contract.configure_agent(accounts(1), U64(now + 31 * DAY_NS), U128(10), U128(4));
    }

    #[test]
    #[should_panic(expected = "Invalid per-transaction limit")]
    fn transaction_limit_cannot_exceed_cap() {
        let mut contract = contract();
        let now = env::block_timestamp();
        contract.configure_agent(accounts(1), U64(now + DAY_NS), U128(10), U128(11));
    }

    #[test]
    #[should_panic(expected = "Contract is paused")]
    fn pause_blocks_spend_requests() {
        let mut contract = contract();
        configure(&mut contract, accounts(1));
        contract.set_recipient(accounts(1), accounts(2), true);
        contract.set_paused(true);
        setup(accounts(1));
        contract.request_spend(accounts(2), U128(1));
    }

    #[test]
    #[should_panic(expected = "Recipient not allowed")]
    fn spend_requires_recipient_allowlist() {
        let mut contract = contract();
        configure(&mut contract, accounts(1));
        setup(accounts(1));
        contract.request_spend(accounts(2), U128(1));
    }

    #[test]
    #[should_panic(expected = "Per-transaction limit exceeded")]
    fn spend_enforces_per_transaction_limit() {
        let mut contract = contract();
        configure(&mut contract, accounts(1));
        contract.set_recipient(accounts(1), accounts(2), true);
        setup(accounts(1));
        contract.request_spend(accounts(2), U128(5));
    }

    #[test]
    fn spend_reserves_cap_until_async_callback() {
        let mut contract = contract();
        configure(&mut contract, accounts(1));
        contract.set_recipient(accounts(1), accounts(2), true);
        setup(accounts(1));

        let request_id = contract.request_spend(accounts(2), U128(4));
        let policy = contract.get_agent_policy(accounts(1)).unwrap();

        assert_eq!(request_id, U64(0));
        assert_eq!(policy.pending_spend, U128(4));
        assert_eq!(policy.native_spent, U128(0));
        assert_eq!(
            contract.get_spend_request(request_id).unwrap().amount,
            U128(4)
        );
    }

    #[test]
    #[should_panic(expected = "Cumulative cap exceeded")]
    fn pending_requests_count_toward_cumulative_cap() {
        let mut contract = contract();
        configure(&mut contract, accounts(1));
        contract.set_recipient(accounts(1), accounts(2), true);
        setup(accounts(1));

        contract.request_spend(accounts(2), U128(4));
        contract.request_spend(accounts(2), U128(4));
        contract.request_spend(accounts(2), U128(3));
    }
}
