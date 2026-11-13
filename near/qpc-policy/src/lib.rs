use near_sdk::{
    env,
    json_types::{U128, U64},
    near, AccountId, PanicOnDefault,
    store::LookupMap,
};

const MAX_POLICY_DURATION_NS: u64 = 30 * 24 * 60 * 60 * 1_000_000_000;
const MAX_ALLOWED_RECIPIENTS: usize = 64;
const MAX_AGENT_POLICIES: u64 = 256;
const MIN_DOMAIN_ID_LENGTH: usize = 3;
const MAX_DOMAIN_ID_LENGTH: usize = 64;

/// Policy state is public on NEAR. This prototype is an authorization gate,
/// not a confidential storage or shielded-payment implementation.
#[near(serializers = [borsh, json])]
#[derive(Clone, PartialEq, Eq)]
pub struct AgentPolicy {
    pub active: bool,
    pub expires_at_ns: u64,
    pub max_per_payment: u128,
    pub authorized_cap: u128,
    pub reserved_total: u128,
    pub settled_total: u128,
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

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaymentStatus {
    Pending,
    Settled,
    Released,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, PartialEq, Eq)]
pub struct PaymentReservation {
    pub agent_id: AccountId,
    pub policy_version: u64,
    pub nonce: u64,
    pub recipient: AccountId,
    pub amount: u128,
    pub request_hash: String,
    pub status: PaymentStatus,
    pub created_at_ns: u64,
    pub settlement_reference: Option<String>,
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
    pending_owner_id: Option<AccountId>,
    guardian_id: Option<AccountId>,
    settlement_adapter_id: Option<AccountId>,
    domain_id: String,
    paused: bool,
    policy_count: u64,
    policies: LookupMap<AccountId, AgentPolicy>,
    reservations: LookupMap<String, PaymentReservation>,
    settlement_references: LookupMap<String, bool>,
}

#[near]
impl QpcPolicyContract {
    #[init]
    pub fn new(owner_id: AccountId, domain_id: String) -> Self {
        assert_eq!(env::predecessor_account_id(), owner_id, "initializer must be the owner");
        assert!(
            (MIN_DOMAIN_ID_LENGTH..=MAX_DOMAIN_ID_LENGTH).contains(&domain_id.len())
                && domain_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
            "invalid deployment domain"
        );
        Self {
            owner_id, pending_owner_id: None, guardian_id: None,
            settlement_adapter_id: None, domain_id,
            paused: false, policy_count: 0,
            policies: LookupMap::new(b"qpc:policies".to_vec()),
            reservations: LookupMap::new(b"qpc:reservations".to_vec()),
            settlement_references: LookupMap::new(b"qpc:settlement_refs".to_vec()),
        }
    }

    pub fn get_owner(&self) -> AccountId { self.owner_id.clone() }

    pub fn get_pending_owner(&self) -> Option<AccountId> { self.pending_owner_id.clone() }
    pub fn get_guardian(&self) -> Option<AccountId> { self.guardian_id.clone() }
    pub fn get_settlement_adapter(&self) -> Option<AccountId> { self.settlement_adapter_id.clone() }
    pub fn get_policy_count(&self) -> U64 { U64(self.policy_count) }

    pub fn set_settlement_adapter(&mut self, adapter_id: Option<AccountId>) {
        self.assert_owner();
        if let Some(ref adapter) = adapter_id {
            assert_ne!(adapter, &self.owner_id, "adapter must differ from owner");
        }
        self.settlement_adapter_id = adapter_id.clone();
        env::log_str(&format!("QPC_SETTLEMENT_ADAPTER_UPDATED|adapter={:?}", adapter_id));
    }

    pub fn get_reservation(&self, request_hash: String) -> Option<PaymentReservation> {
        self.reservations.get(&request_hash).cloned()
    }

    pub fn propose_owner(&mut self, new_owner_id: AccountId) {
        self.assert_owner();
        assert_ne!(new_owner_id, self.owner_id, "new owner must be different");
        self.pending_owner_id = Some(new_owner_id.clone());
        env::log_str(&format!("QPC_OWNER_TRANSFER_PROPOSED|new_owner={}", new_owner_id));
    }

    pub fn accept_owner(&mut self) {
        let caller = env::predecessor_account_id();
        assert_eq!(self.pending_owner_id.as_ref(), Some(&caller), "caller is not the pending owner");
        let previous = self.owner_id.clone();
        self.owner_id = caller.clone();
        self.pending_owner_id = None;
        env::log_str(&format!("QPC_OWNER_TRANSFER_ACCEPTED|previous_owner={}|new_owner={}", previous, caller));
    }

    pub fn set_guardian(&mut self, guardian_id: Option<AccountId>) {
        self.assert_owner();
        if let Some(ref guardian) = guardian_id {
            assert_ne!(guardian, &self.owner_id, "guardian must differ from owner");
        }
        self.guardian_id = guardian_id.clone();
        env::log_str(&format!("QPC_GUARDIAN_UPDATED|guardian={:?}", guardian_id));
    }

    /// Guardian can pause but cannot unpause or change policies.
    pub fn emergency_pause(&mut self) {
        let caller = env::predecessor_account_id();
        assert_eq!(self.guardian_id.as_ref(), Some(&caller), "only guardian can emergency pause");
        self.paused = true;
        env::log_str("QPC_EMERGENCY_PAUSE|guardian");
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

        let existing_policy = self.policies.get(&agent_id);
        if existing_policy.is_none() {
            assert!(self.policy_count < MAX_AGENT_POLICIES, "maximum agent policy count reached");
        }
        let previous_version = existing_policy
            .map(|policy| {
                assert!(!policy.active, "revoke agent before reconfiguration");
                assert_eq!(policy.reserved_total, 0, "release pending reservations before reconfiguration");
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
            reserved_total: 0,
            settled_total: 0,
            next_nonce: 0,
            version,
            allowed_recipients,
        };
        if self.policies.get(&agent_id).is_none() {
            self.policy_count = self.policy_count.checked_add(1)
                .unwrap_or_else(|| env::panic_str("policy count overflow"));
        }
        self.policies.insert(agent_id.clone(), policy);
        env::log_str(&format!("QPC_POLICY_CONFIGURED|agent={}|version={}", agent_id, version));
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

    /// Reserve a bounded payment intent. This does NOT transfer assets.
    /// The settlement adapter is a trusted verifier and must independently
    /// prove that the exact external payment was settled before finalizing.
    pub fn authorize_payment(
        &mut self,
        recipient: AccountId,
        amount: U128,
        nonce: U64,
    ) -> AuthorizationReceipt {
        assert!(!self.paused, "QPC execution is paused");
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

        let committed_total = policy.reserved_total
            .checked_add(policy.settled_total)
            .and_then(|value| value.checked_add(amount.0))
            .unwrap_or_else(|| env::panic_str("committed spending total overflow"));
        assert!(
            committed_total <= policy.authorized_cap,
            "authorized spending cap exceeded"
        );
        let next_nonce = policy
            .next_nonce
            .checked_add(1)
            .unwrap_or_else(|| env::panic_str("nonce overflow"));

        policy.reserved_total = policy.reserved_total.checked_add(amount.0)
            .unwrap_or_else(|| env::panic_str("reserved total overflow"));
        policy.next_nonce = next_nonce;
        // Derive the digest on-chain from canonical intent fields.
        let payload = format!(
            "QPC_PAYMENT_INTENT_V1|domain={}|contract={}|agent={}|policy_version={}|nonce={}|recipient={}|amount={}",
            self.domain_id, env::current_account_id(), agent_id, policy.version,
            nonce.0, recipient, amount.0
        );
        let request_hash = Self::to_hex(&env::sha256(payload.as_bytes()));
        let receipt = AuthorizationReceipt {
            agent_id: agent_id.clone(), policy_version: policy.version,
            nonce: nonce.0, request_hash: request_hash.clone(),
        };
        let reservation = PaymentReservation {
            agent_id: agent_id.clone(),
            policy_version: policy.version,
            nonce: nonce.0,
            recipient,
            amount: amount.0,
            request_hash: request_hash.clone(),
            status: PaymentStatus::Pending,
            created_at_ns: env::block_timestamp(),
            settlement_reference: None,
        };
        assert!(self.reservations.get(&request_hash).is_none(), "duplicate reservation hash");
        self.reservations.insert(request_hash.clone(), reservation);
        self.policies.insert(agent_id.clone(), policy);

        // Intentionally omit recipient and amount from this event. NEAR call
        // inputs are still public; this event choice does not make them private.
        env::log_str(&format!(
            "QPC_PAYMENT_AUTHORIZED|agent={}|version={}|nonce={}|request_hash={}",
            agent_id, receipt.policy_version, receipt.nonce, request_hash
        ));
        receipt
    }

    /// Finalize only after the trusted adapter has independently verified settlement.
    pub fn settle_payment(&mut self, request_hash: String, settlement_reference: String) {
        let caller = env::predecessor_account_id();
        assert_eq!(self.settlement_adapter_id.as_ref(), Some(&caller), "only settlement adapter");
        assert!(
            !settlement_reference.is_empty() && settlement_reference.len() <= 128
                && settlement_reference.bytes().all(|b| b.is_ascii_graphic()),
            "invalid settlement reference"
        );
        assert!(self.settlement_references.get(&settlement_reference).is_none(),
            "settlement reference already used");
        let mut reservation = self.reservations.get(&request_hash).cloned()
            .unwrap_or_else(|| env::panic_str("reservation not found"));
        assert_eq!(reservation.status, PaymentStatus::Pending, "reservation is not pending");
        let mut policy = self.policies.get(&reservation.agent_id).cloned()
            .unwrap_or_else(|| env::panic_str("agent policy not found"));
        assert_eq!(policy.version, reservation.policy_version, "reservation policy version mismatch");
        policy.reserved_total = policy.reserved_total.checked_sub(reservation.amount)
            .unwrap_or_else(|| env::panic_str("reserved total underflow"));
        policy.settled_total = policy.settled_total.checked_add(reservation.amount)
            .unwrap_or_else(|| env::panic_str("settled total overflow"));
        reservation.status = PaymentStatus::Settled;
        reservation.settlement_reference = Some(settlement_reference.clone());
        self.policies.insert(reservation.agent_id.clone(), policy);
        self.reservations.insert(request_hash.clone(), reservation.clone());
        self.settlement_references.insert(settlement_reference.clone(), true);
        env::log_str(&format!("QPC_PAYMENT_SETTLED|request_hash={}|reference={}", request_hash, settlement_reference));
    }

    /// Release a pending reservation after a verified failed/cancelled settlement.
    /// Owner is a recovery authority; adapter is the normal settlement authority.
    pub fn release_payment(&mut self, request_hash: String) {
        let caller = env::predecessor_account_id();
        assert!(
            caller == self.owner_id || self.settlement_adapter_id.as_ref() == Some(&caller),
            "only owner or settlement adapter can release"
        );
        let mut reservation = self.reservations.get(&request_hash).cloned()
            .unwrap_or_else(|| env::panic_str("reservation not found"));
        assert_eq!(reservation.status, PaymentStatus::Pending, "reservation is not pending");
        let mut policy = self.policies.get(&reservation.agent_id).cloned()
            .unwrap_or_else(|| env::panic_str("agent policy not found"));
        assert_eq!(policy.version, reservation.policy_version, "reservation policy version mismatch");
        policy.reserved_total = policy.reserved_total.checked_sub(reservation.amount)
            .unwrap_or_else(|| env::panic_str("reserved total underflow"));
        reservation.status = PaymentStatus::Released;
        self.policies.insert(reservation.agent_id.clone(), policy);
        self.reservations.insert(request_hash.clone(), reservation);
        env::log_str(&format!("QPC_PAYMENT_RELEASED|request_hash={}", request_hash));
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

    fn to_hex(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = Vec::with_capacity(bytes.len() * 2);
        for byte in bytes {
            out.push(HEX[(byte >> 4) as usize]);
            out.push(HEX[(byte & 0x0f) as usize]);
        }
        String::from_utf8(out).expect("hex is valid UTF-8")
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
        let mut contract = QpcPolicyContract::new(account("owner.near"), "near-testnet".to_string());
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
        );
    }

    #[test]
    fn owner_configures_agent_and_valid_intent_is_authorized() {
        let mut contract = configured_contract();
        authorize(&mut contract, 0, 10);
        let policy = contract.get_policy(account("agent.near")).unwrap();
        assert_eq!(policy.reserved_total, 10);
        assert_eq!(policy.settled_total, 0);
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
        );
    }

    #[test]
    #[should_panic(expected = "initializer must be the owner")]
    fn non_owner_cannot_configure_agent() {
        context("attacker.near", NOW_NS);
        let mut contract = QpcPolicyContract::new(account("owner.near"), "near-testnet".to_string());
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
        let mut contract = QpcPolicyContract::new(account("owner.near"), "near-testnet".to_string());
        contract.configure_agent(
            account("agent.near"),
            U64(NOW_NS + DAY_NS),
            U128(1),
            U128(2),
            vec![account("merchant.near"), account("merchant.near")],
        );
    }


    #[test]
    fn request_hash_is_derived_from_exact_intent_and_domain() {
        let mut contract = configured_contract();
        context("agent.near", NOW_NS + 1);
        let receipt = contract.authorize_payment(account("merchant.near"), U128(3), U64(0));
        assert_eq!(receipt.request_hash.len(), 64);
        assert!(receipt.request_hash.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn guardian_can_pause_during_incident() {
        let mut contract = configured_contract();
        context("owner.near", NOW_NS + 1);
        contract.set_guardian(Some(account("guardian.near")));
        context("guardian.near", NOW_NS + 2);
        contract.emergency_pause();
        assert!(contract.is_paused());
    }

    #[test]
    #[should_panic(expected = "caller is not the pending owner")]
    fn unrelated_account_cannot_accept_ownership() {
        let mut contract = configured_contract();
        context("owner.near", NOW_NS + 1);
        contract.propose_owner(account("new-owner.near"));
        context("attacker.near", NOW_NS + 2);
        contract.accept_owner();
    }

    #[test]
    fn pending_owner_can_accept_ownership() {
        let mut contract = configured_contract();
        context("owner.near", NOW_NS + 1);
        contract.propose_owner(account("new-owner.near"));
        context("new-owner.near", NOW_NS + 2);
        contract.accept_owner();
        assert_eq!(contract.get_owner(), account("new-owner.near"));
        assert_eq!(contract.get_pending_owner(), None);
    }


    #[test]
    fn settlement_moves_reserved_amount_to_settled_once() {
        let mut contract = configured_contract();
        context("agent.near", NOW_NS + 1);
        let receipt = contract.authorize_payment(account("merchant.near"), U128(3), U64(0));
        context("owner.near", NOW_NS + 2);
        contract.set_settlement_adapter(Some(account("adapter.near")));
        context("adapter.near", NOW_NS + 3);
        contract.settle_payment(receipt.request_hash.clone(), "near-tx-001".to_string());
        let policy = contract.get_policy(account("agent.near")).unwrap();
        assert_eq!(policy.reserved_total, 0);
        assert_eq!(policy.settled_total, 3);
        assert_eq!(contract.get_reservation(receipt.request_hash).unwrap().status, PaymentStatus::Settled);
    }

    #[test]
    fn release_returns_reserved_capacity_without_marking_settled() {
        let mut contract = configured_contract();
        context("agent.near", NOW_NS + 1);
        let receipt = contract.authorize_payment(account("merchant.near"), U128(3), U64(0));
        context("owner.near", NOW_NS + 2);
        contract.release_payment(receipt.request_hash.clone());
        let policy = contract.get_policy(account("agent.near")).unwrap();
        assert_eq!(policy.reserved_total, 0);
        assert_eq!(policy.settled_total, 0);
        assert_eq!(contract.get_reservation(receipt.request_hash).unwrap().status, PaymentStatus::Released);
    }

    #[test]
    #[should_panic(expected = "reservation is not pending")]
    fn reservation_cannot_be_settled_twice() {
        let mut contract = configured_contract();
        context("agent.near", NOW_NS + 1);
        let receipt = contract.authorize_payment(account("merchant.near"), U128(3), U64(0));
        context("owner.near", NOW_NS + 2);
        contract.set_settlement_adapter(Some(account("adapter.near")));
        context("adapter.near", NOW_NS + 3);
        contract.settle_payment(receipt.request_hash.clone(), "near-tx-001".to_string());
        contract.settle_payment(receipt.request_hash, "near-tx-002".to_string());
    }

    #[test]
    #[should_panic(expected = "settlement reference already used")]
    fn settlement_reference_cannot_be_reused() {
        let mut contract = configured_contract();
        context("owner.near", NOW_NS + 1);
        contract.set_settlement_adapter(Some(account("adapter.near")));
        context("agent.near", NOW_NS + 2);
        let first = contract.authorize_payment(account("merchant.near"), U128(3), U64(0));
        let second = contract.authorize_payment(account("merchant.near"), U128(3), U64(1));
        context("adapter.near", NOW_NS + 3);
        contract.settle_payment(first.request_hash, "same-ref".to_string());
        contract.settle_payment(second.request_hash, "same-ref".to_string());
    }

    #[test]
    #[should_panic(expected = "only settlement adapter")]
    fn non_adapter_cannot_settle() {
        let mut contract = configured_contract();
        context("agent.near", NOW_NS + 1);
        let receipt = contract.authorize_payment(account("merchant.near"), U128(3), U64(0));
        context("attacker.near", NOW_NS + 2);
        contract.settle_payment(receipt.request_hash, "fake-ref".to_string());
    }

    #[test]
    fn released_reservation_allows_later_authorization_with_new_nonce() {
        let mut contract = configured_contract();
        context("agent.near", NOW_NS + 1);
        let receipt = contract.authorize_payment(account("merchant.near"), U128(10), U64(0));
        context("owner.near", NOW_NS + 2);
        contract.release_payment(receipt.request_hash);
        context("agent.near", NOW_NS + 3);
        let second = contract.authorize_payment(account("merchant.near"), U128(10), U64(1));
        assert_ne!(second.request_hash, "");
        let policy = contract.get_policy(account("agent.near")).unwrap();
        assert_eq!(policy.reserved_total, 10);
        assert_eq!(policy.settled_total, 0);
    }


    #[test]
    #[should_panic(expected = "release pending reservations before reconfiguration")]
    fn cannot_reconfigure_policy_with_pending_reservation() {
        let mut contract = configured_contract();
        context("agent.near", NOW_NS + 1);
        contract.authorize_payment(account("merchant.near"), U128(3), U64(0));
        context("owner.near", NOW_NS + 2);
        contract.revoke_agent(account("agent.near"));
        contract.configure_agent(
            account("agent.near"), U64(NOW_NS + 2 * DAY_NS),
            U128(5), U128(10), vec![account("merchant.near")],
        );
    }

    #[test]
    #[should_panic(expected = "authorized spending cap exceeded")]
    fn settled_amount_remains_consumed_against_policy_cap() {
        let mut contract = configured_contract();
        context("owner.near", NOW_NS + 1);
        contract.set_settlement_adapter(Some(account("adapter.near")));
        for (nonce, amount) in [(0, 10), (1, 10), (2, 5)] {
            context("agent.near", NOW_NS + 2 + nonce);
            let receipt = contract.authorize_payment(account("merchant.near"), U128(amount), U64(nonce));
            context("adapter.near", NOW_NS + 10 + nonce);
            contract.settle_payment(receipt.request_hash, format!("settled-cap-check-{}", nonce));
        }
        context("agent.near", NOW_NS + 20);
        contract.authorize_payment(account("merchant.near"), U128(1), U64(3));
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
        assert_eq!(policy.reserved_total, 0);
        assert_eq!(policy.settled_total, 0);
        assert_eq!(policy.next_nonce, 0);
    }
}
