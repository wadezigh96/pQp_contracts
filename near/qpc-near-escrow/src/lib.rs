//! QPC NEAR same-chain NEP-141 escrow prototype.
//! This is a separate contract from the QPC policy/dispatch core.
//! It is NOT a bridge and does not settle cross-chain transfers.

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::store::LookupMap;
use near_sdk::{
    env, near, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault,
    Promise, PromiseOrValue,
};

const MAX_PAYMENT_ID_LEN: usize = 64;
const MAX_ESCROWS_LIFETIME: u64 = 10_000;
const MAX_EXPIRY_NS: u64 = 30 * 24 * 60 * 60 * 1_000_000_000;
const PAYOUT_GAS_TGAS: u64 = 20;
const CALLBACK_GAS_TGAS: u64 = 10;

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    Tokens,
    Agents,
    Payments,
}

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug, PartialEq,
)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenConfig {
    pub enabled: bool,
    pub max_per_payment: u128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct PaymentIntent {
    pub payment_id: String,
    pub recipient: AccountId,
    pub expires_at_ns: u64,
}

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug, PartialEq,
)]
#[serde(crate = "near_sdk::serde")]
pub enum PaymentStatus {
    Pending,
    ReleasePending,
    Released,
    RefundPending,
    Refunded,
}

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug, PartialEq,
)]
#[serde(crate = "near_sdk::serde")]
pub struct PaymentRecord {
    pub payment_id: String,
    pub token_id: AccountId,
    pub payer: AccountId,
    pub recipient: AccountId,
    pub amount: u128,
    pub created_at_ns: u64,
    pub expires_at_ns: u64,
    pub status: PaymentStatus,
}

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug, PartialEq,
)]
#[serde(crate = "near_sdk::serde")]
pub enum PayoutKind {
    Release,
    Refund,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct QpcNearEscrow {
    owner_id: AccountId,
    tokens: LookupMap<AccountId, TokenConfig>,
    agents: LookupMap<AccountId, bool>,
    payments: LookupMap<String, PaymentRecord>,
    lifetime_created: u64,
}

#[near]
impl QpcNearEscrow {
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        assert_eq!(env::predecessor_account_id(), owner_id, "initializer must be owner");
        Self {
            owner_id,
            tokens: LookupMap::new(StorageKey::Tokens),
            agents: LookupMap::new(StorageKey::Agents),
            payments: LookupMap::new(StorageKey::Payments),
            lifetime_created: 0,
        }
    }

    pub fn set_token(&mut self, token_id: AccountId, enabled: bool, max_per_payment: U128) {
        self.assert_owner();
        assert!(max_per_payment.0 > 0, "token payment cap must be positive");
        self.tokens.insert(token_id, TokenConfig { enabled, max_per_payment: max_per_payment.0 });
    }

    pub fn set_agent(&mut self, agent_id: AccountId, enabled: bool) {
        self.assert_owner();
        self.agents.insert(agent_id, enabled);
    }

    /// NEP-141 ft_transfer_call receiver. The FT contract is predecessor and
    /// sender_id is the authenticated payer supplied by that FT contract.
    #[payable]
    pub fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        assert_eq!(env::attached_deposit().as_yoctonear(), 1, "requires exactly 1 yoctoNEAR");
        let token_id = env::predecessor_account_id();
        let config = self.tokens.get(&token_id).expect("token is not allowlisted");
        assert!(config.enabled, "token is disabled");
        assert!(self.agents.get(&sender_id).copied().unwrap_or(false), "sender agent is not enabled");
        assert!(amount.0 > 0, "amount must be positive");
        assert!(amount.0 <= config.max_per_payment, "payment exceeds token cap");
        assert!(self.lifetime_created < MAX_ESCROWS_LIFETIME, "escrow lifetime capacity reached");

        let intent: PaymentIntent = near_sdk::serde_json::from_str(&msg)
            .unwrap_or_else(|_| env::panic_str("invalid payment intent JSON"));
        validate_payment_id(&intent.payment_id);
        assert!(self.payments.get(&intent.payment_id).is_none(), "payment id already used");
        let now = env::block_timestamp();
        assert!(intent.expires_at_ns > now, "payment expiry must be in the future");
        assert!(intent.expires_at_ns - now <= MAX_EXPIRY_NS, "payment expiry exceeds 30 days");
        assert_ne!(sender_id, intent.recipient, "self-payment is not allowed");

        let record = PaymentRecord {
            payment_id: intent.payment_id.clone(),
            token_id,
            payer: sender_id,
            recipient: intent.recipient,
            amount: amount.0,
            created_at_ns: now,
            expires_at_ns: intent.expires_at_ns,
            status: PaymentStatus::Pending,
        };
        self.payments.insert(intent.payment_id.clone(), record);
        self.lifetime_created = self.lifetime_created.checked_add(1)
            .expect("lifetime escrow counter overflow");
        log_payment("payment_escrowed", &intent.payment_id);
        PromiseOrValue::Value(U128(0))
    }

    /// Only the exact recipient may claim, and only before expiry.
    pub fn claim_payment(&mut self, payment_id: String) -> Promise {
        let mut payment = self.payment_or_panic(&payment_id);
        assert_eq!(env::predecessor_account_id(), payment.recipient, "recipient only");
        assert_eq!(payment.status, PaymentStatus::Pending, "payment is not claimable");
        assert!(env::block_timestamp() < payment.expires_at_ns, "payment expired");
        payment.status = PaymentStatus::ReleasePending;
        self.payments.insert(payment_id.clone(), payment.clone());
        log_payment("payment_release_started", &payment_id);
        let recipient = payment.recipient.clone();
        self.payout(payment, recipient, PayoutKind::Release)
    }

    /// Only the original payer may reclaim an unclaimed payment after expiry.
    pub fn refund_payment(&mut self, payment_id: String) -> Promise {
        let mut payment = self.payment_or_panic(&payment_id);
        assert_eq!(env::predecessor_account_id(), payment.payer, "payer only");
        assert_eq!(payment.status, PaymentStatus::Pending, "payment is not refundable");
        assert!(env::block_timestamp() >= payment.expires_at_ns, "payment has not expired");
        payment.status = PaymentStatus::RefundPending;
        self.payments.insert(payment_id.clone(), payment.clone());
        log_payment("payment_refund_started", &payment_id);
        self.payout(payment.clone(), payment.payer.clone(), PayoutKind::Refund)
    }

    #[private]
    pub fn on_payout_result(&mut self, payment_id: String, kind: PayoutKind) -> PaymentStatus {
        assert_eq!(env::promise_results_count(), 1, "expected one FT transfer result");
        let mut payment = self.payment_or_panic(&payment_id);
        let expected_pending = match kind {
            PayoutKind::Release => PaymentStatus::ReleasePending,
            PayoutKind::Refund => PaymentStatus::RefundPending,
        };
        assert_eq!(payment.status, expected_pending, "payout state mismatch");

        let payout_succeeded = env::promise_result_checked(0, 256).is_ok();
        payment.status = match (kind, payout_succeeded) {
            (PayoutKind::Release, true) => PaymentStatus::Released,
            (PayoutKind::Refund, true) => PaymentStatus::Refunded,
            // A failed or unreadable FT transfer leaves escrow pending for reconciliation.
            (_, false) => PaymentStatus::Pending,
        };
        self.payments.insert(payment_id.clone(), payment.clone());
        log_payment(match payment.status {
            PaymentStatus::Released => "payment_released",
            PaymentStatus::Refunded => "payment_refunded",
            PaymentStatus::Pending => "payment_payout_failed",
            PaymentStatus::ReleasePending => "payment_release_started",
            PaymentStatus::RefundPending => "payment_refund_started",
        }, &payment_id);
        payment.status
    }

    pub fn get_payment(&self, payment_id: String) -> Option<PaymentRecord> {
        self.payments.get(&payment_id).cloned()
    }

    pub fn get_token_config(&self, token_id: AccountId) -> Option<TokenConfig> {
        self.tokens.get(&token_id).cloned()
    }

    pub fn is_agent_enabled(&self, agent_id: AccountId) -> bool {
        self.agents.get(&agent_id).copied().unwrap_or(false)
    }

    pub fn get_lifetime_created(&self) -> u64 {
        self.lifetime_created
    }

    fn payout(&self, payment: PaymentRecord, receiver_id: AccountId, kind: PayoutKind) -> Promise {
        let args = near_sdk::serde_json::json!({
            "receiver_id": receiver_id,
            "amount": U128(payment.amount),
            "memo": format!("QPC:{}:{:?}", payment.payment_id, kind),
        }).to_string().into_bytes();
        let callback_args = near_sdk::serde_json::json!({
            "payment_id": payment.payment_id,
            "kind": kind,
        }).to_string().into_bytes();
        Promise::new(payment.token_id)
            .function_call(
                "ft_transfer".to_string(),
                args,
                NearToken::from_yoctonear(1),
                Gas::from_tgas(PAYOUT_GAS_TGAS),
            )
            .then(Promise::new(env::current_account_id()).function_call(
                "on_payout_result".to_string(),
                callback_args,
                NearToken::from_yoctonear(0),
                Gas::from_tgas(CALLBACK_GAS_TGAS),
            ))
    }

    fn assert_owner(&self) {
        assert_eq!(env::predecessor_account_id(), self.owner_id, "owner only");
    }

    fn payment_or_panic(&self, payment_id: &str) -> PaymentRecord {
        self.payments.get(&payment_id.to_string()).cloned().expect("payment not found")
    }
}

fn validate_payment_id(value: &str) {
    assert!(
        !value.is_empty() && value.len() <= MAX_PAYMENT_ID_LEN
            && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid payment id"
    );
}

fn log_payment(event: &str, payment_id: &str) {
    env::log_str(&format!(
        "EVENT_JSON:{{\"standard\":\"qpc\",\"version\":\"1.0.0\",\"event\":\"{}\",\"data\":[{{\"payment_id\":\"{}\"}}]}}",
        event, payment_id
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    const NOW: u64 = 1_000_000_000_000;

    fn context(predecessor: &str, timestamp: u64, deposit: u128) {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor.parse().unwrap())
            .current_account_id("qpc-escrow.near".parse().unwrap())
            .block_timestamp(timestamp)
            .attached_deposit(NearToken::from_yoctonear(deposit));
        testing_env!(builder.build());
    }

    fn setup() -> QpcNearEscrow {
        context("owner.near", NOW, 0);
        let mut contract = QpcNearEscrow::new("owner.near".parse().unwrap());
        contract.set_token("token.near".parse().unwrap(), true, U128(1_000_000));
        contract.set_agent("agent-a.near".parse().unwrap(), true);
        contract
    }

    fn intent(id: &str, recipient: &str, expiry: u64) -> String {
        near_sdk::serde_json::json!({
            "payment_id": id,
            "recipient": recipient,
            "expires_at_ns": expiry
        }).to_string()
    }

    fn escrow(contract: &mut QpcNearEscrow, id: &str) {
        context("token.near", NOW, 1);
        let _ = contract.ft_on_transfer(
            "agent-a.near".parse().unwrap(),
            U128(500),
            intent(id, "agent-b.near", NOW + 1_000_000_000),
        );
    }

    #[test]
    fn allowlisted_agent_can_create_escrow() {
        let mut contract = setup();
        escrow(&mut contract, "pay-1");
        let payment = contract.get_payment("pay-1".to_string()).unwrap();
        assert_eq!(payment.payer, "agent-a.near".parse::<AccountId>().unwrap());
        assert_eq!(payment.recipient, "agent-b.near".parse::<AccountId>().unwrap());
        assert_eq!(payment.amount, 500);
        assert_eq!(payment.status, PaymentStatus::Pending);
    }

    #[test]
    #[should_panic(expected = "sender agent is not enabled")]
    fn unregistered_agent_cannot_create_escrow() {
        let mut contract = setup();
        context("token.near", NOW, 1);
        let _ = contract.ft_on_transfer(
            "attacker.near".parse().unwrap(), U128(500),
            intent("pay-2", "agent-b.near", NOW + 1_000_000_000),
        );
    }

    #[test]
    #[should_panic(expected = "token is not allowlisted")]
    fn unknown_token_cannot_create_escrow() {
        let mut contract = setup();
        context("fake-token.near", NOW, 1);
        let _ = contract.ft_on_transfer(
            "agent-a.near".parse().unwrap(), U128(500),
            intent("pay-3", "agent-b.near", NOW + 1_000_000_000),
        );
    }

    #[test]
    #[should_panic(expected = "payment id already used")]
    fn payment_id_cannot_be_replayed() {
        let mut contract = setup();
        escrow(&mut contract, "pay-4");
        escrow(&mut contract, "pay-4");
    }

    #[test]
    #[should_panic(expected = "recipient only")]
    fn unrelated_agent_cannot_claim() {
        let mut contract = setup();
        escrow(&mut contract, "pay-5");
        context("attacker.near", NOW, 0);
        let _ = contract.claim_payment("pay-5".to_string());
    }

    #[test]
    fn recipient_claim_moves_state_to_release_pending() {
        let mut contract = setup();
        escrow(&mut contract, "pay-6");
        context("agent-b.near", NOW, 0);
        let _ = contract.claim_payment("pay-6".to_string());
        assert_eq!(
            contract.get_payment("pay-6".to_string()).unwrap().status,
            PaymentStatus::ReleasePending
        );
    }

    #[test]
    #[should_panic(expected = "payment has not expired")]
    fn payer_cannot_refund_before_expiry() {
        let mut contract = setup();
        escrow(&mut contract, "pay-7");
        context("agent-a.near", NOW, 0);
        let _ = contract.refund_payment("pay-7".to_string());
    }

    #[test]
    fn payer_can_refund_after_expiry() {
        let mut contract = setup();
        escrow(&mut contract, "pay-8");
        context("agent-a.near", NOW + 1_000_000_000, 0);
        let _ = contract.refund_payment("pay-8".to_string());
        assert_eq!(
            contract.get_payment("pay-8".to_string()).unwrap().status,
            PaymentStatus::RefundPending
        );
    }
}
