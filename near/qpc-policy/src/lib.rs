use near_sdk::{
    env,
    json_types::{U128, U64},
    near, AccountId, PanicOnDefault,
    store::LookupMap,
};

const MAX_POLICY_DURATION_NS: u64 = 30 * 24 * 60 * 60 * 1_000_000_000;
const MAX_ALLOWED_RECIPIENTS: usize = 64;
const REQUEST_HASH_HEX_LENGTH: usize = 64;

/// Policy state is public on NEAR. This prototype is an authorization gate,
/// not a confidential storage or shielded-payment implementation.
#[near(serializers = [borsh, json])]
#[derive(Clone, PartialEq, Eq)]
pub struct AgentPolicy {
    pub active: bool,
    pub expires_at_ns: u64,
    pub max_per_payment: u128,
    pub authorized_cap: u128,
    pub authorized_total: u128,
    pub next_nonce: u64,
    pub version: u64,
    pub allowed_recipients: Vec<AccountId>,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, PartialEq, Eq)]
pub struct AuthorizationReceipt {
    pub agent_id: AccountId,
    pub policy_version: u64,
    pub nonce: u64,
    pub request_hash: String,
}

/// QPC NEAR policy gate.
///
/// This contract authorizes and accounts for bounded payment intents. It does
/// not transfer NEAR or FT tokens and must not be described as a settlement
/// contract until a separately audited adapter is implemented.
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct QpcPolicyContract {
    owner_id: AccountId,
    paused: bool,
    policies: LookupMap<AccountId, AgentPolicy>,
}

#[near]
impl QpcPolicyContract {
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        assert_eq!(
            env::predecessor_account_id(),
            owner_id,
            "initializer must be the owner"
        );
        Self {
            owner_id,
            paused: false,
            policies: LookupMap::new(b"qpc:policies".to_vec()),
        }
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner_id.clone()
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn get_policy(&self, agent_id: AccountId) -> Option<AgentPolicy> {
        self.policies.get(&agent_id).cloned()
    }

    /// Configure an agent. Reconfiguration is permitted only after revocation.
    /// Spending counters and nonce reset only when a new policy version is created.
    pub fn configure_agent(
        &mut self,
        agent_id: AccountId,
        expires_at_ns: U64,
        max_per_payment: U128,
        authorized_cap: U128,
        allowed_recipients: Vec<AccountId>,
    ) {
        self.assert_owner();
        assert_ne!(agent_id, self.owner_id, "owner cannot be an agent");
        assert!(
            !allowed_recipients.is_empty()
                && allowed_recipients.len() <= MAX_ALLOWED_RECIPIENTS,
            "recipient allowlist size is invalid"
        );
        for (index, recipient) in allowed_recipients.iter().enumerate() {
            assert!(
                !allowed_recipients[..index].contains(recipient),
                "duplicate recipient"
            );
        }

        let now = env::block_timestamp();
        assert!(
            expires_at_ns.0 > now
                && expires_at_ns.0.saturating_sub(now) <= MAX_POLICY_DURATION_NS,
            "policy expiry must be within 30 days"
        );
        assert!(
            max_per_payment.0 > 0
                && authorized_cap.0 > 0
                && max_per_payment.0 <= authorized_cap.0,
            "invalid spending limits"
        );

        let previous_version = self
            .policies
            .get(&agent_id)
            .map(|policy| {
                assert!(!policy.active, "revoke agent before reconfiguration");
                policy.version
            })
            .unwrap_or(0);
        let version = previous_version
            .checked_add(1)
            .unwrap_or_else(|| env::panic_str("policy version overflow"));

        let policy = AgentPolicy {
            active: true,
            expires_at_ns: expires_at_ns.0,
            max_per_payment: max_per_payment.0,
            authorized_cap: authorized_cap.0,
            authorized_total: 0,
            next_nonce: 0,
            version,
            allowed_recipients,
        };
        self.policies.insert(agent_id.clone(), policy);
        env::log_str(&format!(
            "QPC_POLICY_CONFIGURED|agent={}|version={}",
            agent_id, version
        ));
    }

    pub fn revoke_agent(&mut self, agent_id: AccountId) {
        self.assert_owner();
        let mut policy = self
            .policies
            .get(&agent_id)
            .cloned()
            .unwrap_or_else(|| env::panic_str("agent policy not found"));
        policy.active = false;
        self.policies.insert(agent_id.clone(), policy);
        env::log_str(&format!("QPC_AGENT_REVOKED|agent={}", agent_id));
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.assert_owner();
        self.paused = paused;
        env::log_str(if paused {
            "QPC_EXECUTION_PAUSED"
        } else {
            "QPC_EXECUTION_UNPAUSED"
        });
    }

    /// Authorize a payment intent and reserve its amount against policy limits.
    ///
    /// This does NOT transfer assets. The caller must be the configured agent.
    /// A downstream settlement adapter must bind the receipt to the exact
    /// payment payload and independently verify settlement.
    pub fn authorize_payment(
        &mut self,
        recipient: AccountId,
        amount: U128,
        nonce: U64,
        request_hash: String,
    ) -> AuthorizationReceipt {
        assert!(!self.paused, "QPC execution is paused");
        assert!(
            Self::is_valid_request_hash(&request_hash),
            "request_hash must be 64 hexadecimal characters"
        );

        let agent_id = env::predecessor_account_id();
        let mut policy = self
            .policies
            .get(&agent_id)
            .cloned()
            .unwrap_or_else(|| env::panic_str("agent is not configured"));

        assert!(policy.active, "agent is revoked");
        assert!(
            env::block_timestamp() < policy.expires_at_ns,
            "agent policy expired"
        );
        assert!(
            policy.allowed_recipients.contains(&recipient),
            "recipient is not allowed"
        );
        assert!(amount.0 > 0, "amount must be positive");
        assert!(
            amount.0 <= policy.max_per_payment,
            "per-payment limit exceeded"
        );
        assert_eq!(nonce.0, policy.next_nonce, "invalid or replayed nonce");

        let new_total = policy
            .authorized_total
            .checked_add(amount.0)
            .unwrap_or_else(|| env::panic_str("authorized total overflow"));
        assert!(
            new_total <= policy.authorized_cap,
            "authorized spending cap exceeded"
        );
        let next_nonce = policy
            .next_nonce
            .checked_add(1)
            .unwrap_or_else(|| env::panic_str("nonce overflow"));

        policy.authorized_total = new_total;
        policy.next_nonce = next_nonce;
        let receipt = AuthorizationReceipt {
            agent_id: agent_id.clone(),
            policy_version: policy.version,
            nonce: nonce.0,
            request_hash: request_hash.clone(),
        };
        self.policies.insert(agent_id.clone(), policy);

        // Intentionally omit recipient and amount from this event. NEAR call
        // inputs are still public; this event choice does not make them private.
        env::log_str(&format!(
            "QPC_PAYMENT_AUTHORIZED|agent={}|version={}|nonce={}|request_hash={}",
            agent_id, receipt.policy_version, receipt.nonce, request_hash
        ));
        receipt
    }
}

impl QpcPolicyContract {
    fn assert_owner(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner_id,
            "only owner can call this method"
        );
    }

    fn is_valid_request_hash(value: &str) -> bool {
        value.len() == REQUEST_HASH_HEX_LENGTH
            && value
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    const DAY_NS: u64 = 24 * 60 * 60 * 1_000_000_000;
    const NOW_NS: u64 = 1_000 * DAY_NS;

    fn account(value: &str) -> AccountId {
        value.parse().expect("valid account id")
    }

    fn context(caller: &str, timestamp: u64) {
        let context = VMContextBuilder::new()
            .predecessor_account_id(account(caller))
            .block_timestamp(timestamp)
            .build();
        testing_env!(context);
    }

    fn configured_contract() -> QpcPolicyContract {
        context("owner.near", NOW_NS);
        let mut contract = QpcPolicyContract::new(account("owner.near"));
        contract.configure_agent(
            account("agent.near"),
            U64(NOW_NS + 7 * DAY_NS),
            U128(10),
            U128(25),
            vec![account("merchant.near")],
        );
        contract
    }

    fn authorize(contract: &mut QpcPolicyContract, nonce: u64, amount: u128) {
        context("agent.near", NOW_NS + 1);
        contract.authorize_payment(
            account("merchant.near"),
            U128(amount),
            U64(nonce),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        );
    }

    #[test]
    fn owner_configures_agent_and_valid_intent_is_authorized() {
        let mut contract = configured_contract();
        authorize(&mut contract, 0, 10);
        let policy = contract.get_policy(account("agent.near")).unwrap();
        assert_eq!(policy.authorized_total, 10);
        assert_eq!(policy.next_nonce, 1);
        assert!(policy.active);
    }

    #[test]
    #[should_panic(expected = "recipient is not allowed")]
    fn rejects_wrong_recipient() {
        let mut contract = configured_contract();
        context("agent.near", NOW_NS + 1);
        contract.authorize_payment(
            account("attacker.near"),
            U128(1),
            U64(0),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        );
    }

    #[test]
    #[should_panic(expected = "per-payment limit exceeded")]
    fn rejects_per_payment_limit() {
        let mut contract = configured_contract();
        authorize(&mut contract, 0, 11);
    }

    #[test]
    #[should_panic(expected = "authorized spending cap exceeded")]
    fn rejects_cumulative_cap() {
        let mut contract = configured_contract();
        authorize(&mut contract, 0, 10);
        authorize(&mut contract, 1, 10);
        authorize(&mut contract, 2, 10);
    }

    #[test]
    #[should_panic(expected = "invalid or replayed nonce")]
    fn rejects_replayed_nonce() {
        let mut contract = configured_contract();
        authorize(&mut contract, 0, 1);
        authorize(&mut contract, 0, 1);
    }

    #[test]
    #[should_panic(expected = "agent policy expired")]
    fn rejects_expired_policy() {
        let mut contract = configured_contract();
        context("agent.near", NOW_NS + 7 * DAY_NS);
        contract.authorize_payment(
            account("merchant.near"),
            U128(1),
            U64(0),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        );
    }

    #[test]
    #[should_panic(expected = "QPC execution is paused")]
    fn pause_blocks_authorization() {
        let mut contract = configured_contract();
        context("owner.near", NOW_NS + 1);
        contract.set_paused(true);
        context("agent.near", NOW_NS + 2);
        contract.authorize_payment(
            account("merchant.near"),
            U128(1),
            U64(0),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        );
    }

    #[test]
    #[should_panic(expected = "agent is revoked")]
    fn revoked_agent_cannot_authorize() {
        let mut contract = configured_contract();
        context("owner.near", NOW_NS + 1);
        contract.revoke_agent(account("agent.near"));
        context("agent.near", NOW_NS + 2);
        contract.authorize_payment(
            account("merchant.near"),
            U128(1),
            U64(0),
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        );
    }

    #[test]
    #[should_panic(expected = "only owner can call this method")]
    fn non_owner_cannot_configure_agent() {
        context("attacker.near", NOW_NS);
        let mut contract = QpcPolicyContract::new(account("owner.near"));
        contract.configure_agent(
            account("agent.near"),
            U64(NOW_NS + DAY_NS),
            U128(1),
            U128(2),
            vec![account("merchant.near")],
        );
    }

    #[test]
    #[should_panic(expected = "duplicate recipient")]
    fn rejects_duplicate_recipients() {
        context("owner.near", NOW_NS);
        let mut contract = QpcPolicyContract::new(account("owner.near"));
        contract.configure_agent(
            account("agent.near"),
            U64(NOW_NS + DAY_NS),
            U128(1),
            U128(2),
            vec![account("merchant.near"), account("merchant.near")],
        );
    }

    #[test]
    fn revoked_policy_can_be_reconfigured_with_incremented_version() {
        let mut contract = configured_contract();
        let old_version = contract.get_policy(account("agent.near")).unwrap().version;
        context("owner.near", NOW_NS + 1);
        contract.revoke_agent(account("agent.near"));
        contract.configure_agent(
            account("agent.near"),
            U64(NOW_NS + 2 * DAY_NS),
            U128(5),
            U128(10),
            vec![account("merchant.near")],
        );
        let policy = contract.get_policy(account("agent.near")).unwrap();
        assert_eq!(policy.version, old_version + 1);
        assert_eq!(policy.authorized_total, 0);
        assert_eq!(policy.next_nonce, 0);
    }
}
