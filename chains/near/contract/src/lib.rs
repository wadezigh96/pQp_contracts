use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{Base64VecU8, U128};
use near_sdk::store::LookupMap;
use near_sdk::{
    env, ext_contract, near, require, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault,
    Promise, PromiseError,
};

const MAX_POLICY_DURATION_NS: u64 = 30 * 24 * 60 * 60 * 1_000_000_000;
const MAX_INTENT_LIFETIME_NS: u64 = 5 * 60 * 1_000_000_000;
const CALLBACK_RECOVERY_DELAY_NS: u64 = 10 * 60 * 1_000_000_000;
const MAX_PENDING_PER_AGENT: u8 = 8;
const CALLBACK_GAS: Gas = Gas::from_tgas(40);
const MIN_BALANCE_RESERVE: u128 = 100_000_000_000_000_000_000_000; // 0.1 NEAR
const ML_DSA_65_PUBLIC_KEY_LEN: usize = 1_952;
const ML_DSA_65_SIGNATURE_LEN: usize = 3_309;
const MAX_CHAIN_ID_LEN: usize = 128;
const MAX_DERIVATION_PATH_LEN: usize = 128;
const INTENT_DOMAIN: &str = "QPC/NEAR/ML-DSA-65/INTENT";

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    Policies,
    PendingTransfers,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
struct AgentPolicy {
    active: bool,
    recipient: AccountId,
    destination_chain_id: String,
    derivation_path: String,
    expires_at_ns: u64,
    public_key_hash: [u8; 32],
    total_cap: u128,
    per_tx_limit: u128,
    spent: u128,
    reserved: u128,
    next_nonce: u64,
    policy_version: u64,
    pending_count: u8,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
enum TransferKind {
    Agent { agent: AccountId },
    OwnerWithdrawal,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
struct ExecutionAuditRecord {
    operation_id: u64,
    source_chain_id: String,
    destination_chain_id: String,
    derivation_path: String,
    agent: AccountId,
    recipient: AccountId,
    amount: u128,
    nonce: u64,
    policy_version: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
struct PendingTransfer {
    amount: u128,
    kind: TransferKind,
    audit_record: Option<ExecutionAuditRecord>,
    created_at_ns: u64,
}

/// Canonical Borsh encoding is signed as-is; do not pre-hash before ML-DSA verification.
#[derive(BorshSerialize)]
struct IntentMessage {
    domain: String,
    format_version: u8,
    chain_id: String,
    destination_chain_id: String,
    derivation_path: String,
    contract_id: AccountId,
    agent: AccountId,
    recipient: AccountId,
    amount: u128,
    nonce: u64,
    valid_until_ns: u64,
    policy_version: u64,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct QpcVault {
    owner: AccountId,
    paused: bool,
    available_balance: u128,
    next_operation_id: u64,
    policies: LookupMap<AccountId, AgentPolicy>,
    pending_transfers: LookupMap<u64, PendingTransfer>,
}

#[ext_contract(ext_self)]
#[allow(dead_code)]
trait ExtSelf {
    fn on_transfer_result(&mut self, operation_id: u64) -> bool;
}

#[allow(clippy::too_many_arguments)]
#[near]
impl QpcVault {
    #[init]
    pub fn new(owner: AccountId) -> Self {
        require!(
            owner != env::current_account_id(),
            "owner cannot be the contract"
        );
        Self {
            owner,
            paused: false,
            available_balance: 0,
            next_operation_id: 0,
            policies: LookupMap::new(StorageKey::Policies),
            pending_transfers: LookupMap::new(StorageKey::PendingTransfers),
        }
    }

    /// Fund the vault through the contract method; arbitrary inbound transfers are not counted.
    #[payable]
    pub fn deposit(&mut self) {
        require!(!self.paused, "contract is paused");
        let amount = env::attached_deposit().as_yoctonear();
        require!(amount > 0, "deposit must be positive");
        self.available_balance = self
            .available_balance
            .checked_add(amount)
            .expect("balance overflow");
    }

    /// Register an agent's post-quantum public key by hash; the 1952-byte key is supplied per intent.
    #[allow(clippy::too_many_arguments)]
    pub fn configure_agent(
        &mut self,
        agent: AccountId,
        recipient: AccountId,
        destination_chain_id: String,
        derivation_path: String,
        expires_at_ns: u64,
        total_cap: U128,
        per_tx_limit: U128,
        ml_dsa_65_public_key: Base64VecU8,
    ) {
        self.assert_owner();
        require!(
            valid_chain_id(&destination_chain_id),
            "invalid destination chain ID"
        );
        require!(
            destination_chain_id == env::chain_id(),
            "destination chain ID must match this NEAR network"
        );
        require!(
            valid_derivation_path(&derivation_path),
            "invalid derivation path"
        );
        require!(
            agent != self.owner && agent != env::current_account_id(),
            "invalid agent"
        );
        require!(recipient != env::current_account_id(), "invalid recipient");
        require!(
            ml_dsa_65_public_key.0.len() == ML_DSA_65_PUBLIC_KEY_LEN,
            "ML-DSA-65 public key must be 1952 bytes"
        );
        let now = env::block_timestamp();
        require!(expires_at_ns > now, "expiry must be in the future");
        require!(
            expires_at_ns <= now.saturating_add(MAX_POLICY_DURATION_NS),
            "expiry too far"
        );
        require!(
            total_cap.0 > 0 && per_tx_limit.0 > 0,
            "limits must be positive"
        );
        require!(
            per_tx_limit.0 <= total_cap.0,
            "transaction limit exceeds cap"
        );

        let next_version = if let Some(current) = self.policies.get(&agent) {
            require!(
                !current.active || current.expires_at_ns <= now,
                "agent policy is still active"
            );
            require!(current.pending_count == 0, "agent has pending transfers");
            current
                .policy_version
                .checked_add(1)
                .expect("policy version overflow")
        } else {
            1
        };

        let public_key_hash = env::sha256_array(&ml_dsa_65_public_key.0);
        self.policies.insert(
            agent,
            AgentPolicy {
                active: true,
                recipient,
                destination_chain_id,
                derivation_path,
                expires_at_ns,
                public_key_hash,
                total_cap: total_cap.0,
                per_tx_limit: per_tx_limit.0,
                spent: 0,
                reserved: 0,
                next_nonce: 0,
                policy_version: next_version,
                pending_count: 0,
            },
        );
    }

    pub fn revoke_agent(&mut self, agent: AccountId) {
        self.assert_owner();
        let mut policy = self.policies.get(&agent).cloned().expect("unknown agent");
        policy.active = false;
        self.policies.insert(agent, policy);
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.assert_owner();
        self.paused = paused;
    }

    /// Verify and consume one ML-DSA-65-signed native NEAR transfer intent.
    /// The signer may be an agent, a relayer, or an automation service; the ML-DSA signature,
    /// not the transaction submitter, authorizes the exact bounded intent.
    #[allow(clippy::too_many_arguments)]
    pub fn execute_signed_intent(
        &mut self,
        agent: AccountId,
        recipient: AccountId,
        amount: U128,
        destination_chain_id: String,
        derivation_path: String,
        nonce: u64,
        valid_until_ns: u64,
        policy_version: u64,
        ml_dsa_65_public_key: Base64VecU8,
        ml_dsa_65_signature: Base64VecU8,
    ) -> Promise {
        require!(!self.paused, "contract is paused");
        require!(
            valid_chain_id(&destination_chain_id),
            "invalid destination chain ID"
        );
        require!(
            destination_chain_id == env::chain_id(),
            "destination chain ID must match this NEAR network"
        );
        require!(
            valid_derivation_path(&derivation_path),
            "invalid derivation path"
        );
        require!(
            ml_dsa_65_public_key.0.len() == ML_DSA_65_PUBLIC_KEY_LEN,
            "ML-DSA-65 public key must be 1952 bytes"
        );
        require!(
            ml_dsa_65_signature.0.len() == ML_DSA_65_SIGNATURE_LEN,
            "ML-DSA-65 signature must be 3309 bytes"
        );
        let mut policy = self
            .policies
            .get(&agent)
            .cloned()
            .expect("agent is not configured");
        let now = env::block_timestamp();
        require!(policy.active, "agent is inactive");
        require!(now < policy.expires_at_ns, "agent policy expired");
        require!(recipient == policy.recipient, "recipient is not allowed");
        require!(
            destination_chain_id == policy.destination_chain_id,
            "destination chain ID does not match policy"
        );
        require!(
            derivation_path == policy.derivation_path,
            "derivation path does not match policy"
        );
        require!(nonce == policy.next_nonce, "invalid nonce");
        require!(
            policy_version == policy.policy_version,
            "stale policy version"
        );
        require!(valid_until_ns >= now, "intent expired");
        require!(
            valid_until_ns <= policy.expires_at_ns,
            "intent exceeds policy expiry"
        );
        require!(
            valid_until_ns <= now.saturating_add(MAX_INTENT_LIFETIME_NS),
            "intent lifetime too long"
        );
        require!(amount.0 > 0, "amount must be positive");
        require!(
            amount.0 <= policy.per_tx_limit,
            "transaction limit exceeded"
        );
        require!(
            policy.pending_count < MAX_PENDING_PER_AGENT,
            "too many pending transfers"
        );
        let committed = policy
            .spent
            .checked_add(policy.reserved)
            .and_then(|value| value.checked_add(amount.0))
            .expect("policy accounting overflow");
        require!(committed <= policy.total_cap, "cumulative cap exceeded");
        require!(
            self.available_balance >= amount.0.saturating_add(MIN_BALANCE_RESERVE),
            "insufficient available balance"
        );

        let public_key_hash = env::sha256_array(&ml_dsa_65_public_key.0);
        require!(
            public_key_hash == policy.public_key_hash,
            "unregistered ML-DSA-65 key"
        );
        let message = self.encode_intent(
            &agent,
            &recipient,
            amount.0,
            &destination_chain_id,
            &derivation_path,
            nonce,
            valid_until_ns,
            policy_version,
        );
        let signature: [u8; ML_DSA_65_SIGNATURE_LEN] = ml_dsa_65_signature
            .0
            .as_slice()
            .try_into()
            .expect("signature length checked");
        let public_key: [u8; ML_DSA_65_PUBLIC_KEY_LEN] = ml_dsa_65_public_key
            .0
            .as_slice()
            .try_into()
            .expect("public key length checked");
        require!(
            env::ml_dsa_verify(&signature, &message, &public_key),
            "invalid ML-DSA-65 signature"
        );

        let operation_id = self.allocate_operation_id();
        policy.reserved = policy
            .reserved
            .checked_add(amount.0)
            .expect("reserved overflow");
        policy.pending_count += 1;
        policy.next_nonce = policy.next_nonce.checked_add(1).expect("nonce overflow");
        self.available_balance -= amount.0;
        self.policies.insert(agent.clone(), policy);
        let audit_record = ExecutionAuditRecord {
            operation_id,
            source_chain_id: env::chain_id(),
            destination_chain_id,
            derivation_path,
            agent: agent.clone(),
            recipient: recipient.clone(),
            amount: amount.0,
            nonce,
            policy_version,
        };
        self.pending_transfers.insert(
            operation_id,
            PendingTransfer {
                amount: amount.0,
                kind: TransferKind::Agent { agent },
                audit_record: Some(audit_record.clone()),
                created_at_ns: env::block_timestamp(),
            },
        );
        emit_execution_audit(&audit_record, "pending");

        Promise::new(recipient)
            .transfer(NearToken::from_yoctonear(amount.0))
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(CALLBACK_GAS)
                    .on_transfer_result(operation_id),
            )
    }

    /// Owner may withdraw only unreserved funds while preserving a small operational reserve.
    pub fn withdraw_available(&mut self, recipient: AccountId, amount: U128) -> Promise {
        self.assert_owner();
        require!(amount.0 > 0, "amount must be positive");
        require!(recipient != env::current_account_id(), "invalid recipient");
        require!(
            self.available_balance >= amount.0.saturating_add(MIN_BALANCE_RESERVE),
            "insufficient available balance"
        );
        let operation_id = self.allocate_operation_id();
        self.available_balance -= amount.0;
        self.pending_transfers.insert(
            operation_id,
            PendingTransfer {
                amount: amount.0,
                kind: TransferKind::OwnerWithdrawal,
                audit_record: None,
                created_at_ns: env::block_timestamp(),
            },
        );
        Promise::new(recipient)
            .transfer(NearToken::from_yoctonear(amount.0))
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(CALLBACK_GAS)
                    .on_transfer_result(operation_id),
            )
    }

    /// Owner-attested recovery when the callback receipt itself did not settle.
    /// The owner must verify the transfer receipt on NEAR before selecting its outcome.
    pub fn reconcile_stale_transfer(&mut self, operation_id: u64, transfer_succeeded: bool) {
        self.assert_owner();
        let pending = self
            .pending_transfers
            .get(&operation_id)
            .cloned()
            .expect("unknown or already settled operation");
        require!(
            env::block_timestamp()
                >= pending
                    .created_at_ns
                    .saturating_add(CALLBACK_RECOVERY_DELAY_NS),
            "callback recovery delay has not elapsed"
        );
        self.pending_transfers.remove(&operation_id);
        self.settle_transfer(
            pending,
            transfer_succeeded,
            if transfer_succeeded {
                "owner_reconciled_succeeded"
            } else {
                "owner_reconciled_failed"
            },
        );
    }

    #[private]
    pub fn on_transfer_result(
        &mut self,
        operation_id: u64,
        #[callback_result] result: Result<(), PromiseError>,
    ) -> bool {
        let Some(pending) = self.pending_transfers.remove(&operation_id) else {
            // A callback arriving after owner recovery must never settle the operation twice.
            env::log_str("QPC: ignored callback for an already reconciled transfer");
            return false;
        };
        let succeeded = result.is_ok();
        self.settle_transfer(
            pending,
            succeeded,
            if succeeded { "succeeded" } else { "failed" },
        );
        succeeded
    }

    fn settle_transfer(&mut self, pending: PendingTransfer, succeeded: bool, audit_status: &str) {
        let audit_record = pending.audit_record;
        match pending.kind {
            TransferKind::Agent { agent } => {
                let mut policy = self
                    .policies
                    .get(&agent)
                    .cloned()
                    .expect("agent policy missing");
                policy.reserved = policy
                    .reserved
                    .checked_sub(pending.amount)
                    .expect("reserved balance underflow");
                policy.pending_count = policy
                    .pending_count
                    .checked_sub(1)
                    .expect("pending underflow");
                if succeeded {
                    policy.spent = policy
                        .spent
                        .checked_add(pending.amount)
                        .expect("spent overflow");
                } else {
                    self.restore_balance(pending.amount);
                }
                self.policies.insert(agent, policy);
            }
            TransferKind::OwnerWithdrawal => {
                if !succeeded {
                    self.restore_balance(pending.amount);
                }
            }
        }
        if let Some(record) = audit_record {
            emit_execution_audit(&record, audit_status);
        }
    }

    pub fn get_policy(&self, agent: AccountId) -> Option<AgentPolicyView> {
        self.policies.get(&agent).map(|policy| AgentPolicyView {
            active: policy.active,
            recipient: policy.recipient.clone(),
            destination_chain_id: policy.destination_chain_id.clone(),
            derivation_path: policy.derivation_path.clone(),
            expires_at_ns: policy.expires_at_ns,
            total_cap: U128(policy.total_cap),
            per_tx_limit: U128(policy.per_tx_limit),
            spent: U128(policy.spent),
            reserved: U128(policy.reserved),
            next_nonce: policy.next_nonce,
            policy_version: policy.policy_version,
            pending_count: policy.pending_count,
        })
    }

    pub fn get_status(&self) -> VaultStatus {
        VaultStatus {
            owner: self.owner.clone(),
            chain_id: env::chain_id(),
            paused: self.paused,
            available_balance: U128(self.available_balance),
            next_operation_id: self.next_operation_id,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn encode_intent(
        &self,
        agent: &AccountId,
        recipient: &AccountId,
        amount: u128,
        destination_chain_id: &str,
        derivation_path: &str,
        nonce: u64,
        valid_until_ns: u64,
        policy_version: u64,
    ) -> Vec<u8> {
        borsh::to_vec(&IntentMessage {
            domain: INTENT_DOMAIN.to_owned(),
            format_version: 1,
            chain_id: env::chain_id(),
            destination_chain_id: destination_chain_id.to_owned(),
            derivation_path: derivation_path.to_owned(),
            contract_id: env::current_account_id(),
            agent: agent.clone(),
            recipient: recipient.clone(),
            amount,
            nonce,
            valid_until_ns,
            policy_version,
        })
        .expect("intent serialization failed")
    }

    fn assert_owner(&self) {
        require!(env::predecessor_account_id() == self.owner, "owner only");
    }

    fn allocate_operation_id(&mut self) -> u64 {
        let id = self.next_operation_id;
        self.next_operation_id = id.checked_add(1).expect("operation id overflow");
        id
    }

    fn restore_balance(&mut self, amount: u128) {
        self.available_balance = self
            .available_balance
            .checked_add(amount)
            .expect("balance overflow");
    }
}

fn valid_chain_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CHAIN_ID_LEN
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

/// Accept a canonical, bounded BIP-32-style path label; this contract binds the label but does
/// not derive or prove that an ML-DSA key was generated from the path.
fn valid_derivation_path(value: &str) -> bool {
    if value.len() > MAX_DERIVATION_PATH_LEN || !value.starts_with("m/") {
        return false;
    }
    let mut components = 0usize;
    for component in value[2..].split('/') {
        components += 1;
        if components > 16 || component.is_empty() {
            return false;
        }
        let index = component.strip_suffix('\'').unwrap_or(component);
        if index.is_empty()
            || !index.bytes().all(|byte| byte.is_ascii_digit())
            || (index.len() > 1 && index.starts_with('0'))
            || index.parse::<u32>().is_err()
        {
            return false;
        }
    }
    components > 0
}

fn emit_execution_audit(record: &ExecutionAuditRecord, status: &str) {
    let event = near_sdk::serde_json::json!({
        "standard": "qpc",
        "version": "1.0.0",
        "event": "execution_audit",
        "data": [{
            "operation_id": record.operation_id.to_string(),
            "source_chain_id": record.source_chain_id,
            "destination_chain_id": record.destination_chain_id,
            "derivation_path": record.derivation_path,
            "agent": record.agent.as_str(),
            "recipient": record.recipient.as_str(),
            "amount_yocto": record.amount.to_string(),
            "nonce": record.nonce.to_string(),
            "policy_version": record.policy_version.to_string(),
            "status": status,
        }]
    });
    env::log_str(&format!("EVENT_JSON:{event}"));
}

#[near(serializers = [json])]
pub struct AgentPolicyView {
    pub active: bool,
    pub recipient: AccountId,
    pub destination_chain_id: String,
    pub derivation_path: String,
    pub expires_at_ns: u64,
    pub total_cap: U128,
    pub per_tx_limit: U128,
    pub spent: U128,
    pub reserved: U128,
    pub next_nonce: u64,
    pub policy_version: u64,
    pub pending_count: u8,
}

#[near(serializers = [json])]
pub struct VaultStatus {
    pub owner: AccountId,
    pub chain_id: String,
    pub paused: bool,
    pub available_balance: U128,
    pub next_operation_id: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_crypto::{KeyType, PublicKey, SecretKey, Signature};
    use near_sdk::test_utils::{get_logs, VMContextBuilder};
    use near_sdk::testing_env;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    const NOW: u64 = 1_000_000_000_000;
    const EXPIRY: u64 = NOW + 20 * 60 * 1_000_000_000;
    const TEST_CHAIN_ID: &str = "test";
    const TEST_DERIVATION_PATH: &str = "m/44'/397'/0'";

    struct TestFixture {
        contract: QpcVault,
        key: SecretKey,
        public_key: Vec<u8>,
    }

    fn account(value: &str) -> AccountId {
        value.parse().unwrap()
    }

    fn setup() -> TestFixture {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(account("qpc.testnet"))
            .predecessor_account_id(account("owner.testnet"))
            .block_timestamp(NOW);
        testing_env!(builder.build());

        let key = SecretKey::from_random(KeyType::MLDSA65);
        let public_key = match key.public_key() {
            PublicKey::MLDSA65(key) => key.as_ref().to_vec(),
            _ => unreachable!(),
        };
        let mut contract = QpcVault::new(account("owner.testnet"));
        testing_env!(VMContextBuilder::new()
            .current_account_id(account("qpc.testnet"))
            .predecessor_account_id(account("owner.testnet"))
            .block_timestamp(NOW)
            .attached_deposit(NearToken::from_yoctonear(2 * MIN_BALANCE_RESERVE + 1_000))
            .build());
        contract.deposit();
        contract.configure_agent(
            account("agent.testnet"),
            account("merchant.testnet"),
            TEST_CHAIN_ID.to_owned(),
            TEST_DERIVATION_PATH.to_owned(),
            EXPIRY,
            U128(600),
            U128(300),
            Base64VecU8(public_key.clone()),
        );
        TestFixture {
            contract,
            key,
            public_key,
        }
    }

    fn signed_intent(
        fixture: &TestFixture,
        agent: &str,
        recipient: &str,
        amount: u128,
        nonce: u64,
        valid_until_ns: u64,
        policy_version: u64,
    ) -> Vec<u8> {
        let policy = fixture
            .contract
            .get_policy(account(agent))
            .expect("test agent policy missing");
        let message = fixture.contract.encode_intent(
            &account(agent),
            &account(recipient),
            amount,
            &policy.destination_chain_id,
            &policy.derivation_path,
            nonce,
            valid_until_ns,
            policy_version,
        );
        match fixture.key.sign(&message) {
            Signature::MLDSA65(signature) => signature.as_ref().to_vec(),
            _ => unreachable!(),
        }
    }

    fn execute(
        fixture: &mut TestFixture,
        recipient: &str,
        amount: u128,
        nonce: u64,
        expiry: u64,
        version: u64,
        sig: Vec<u8>,
    ) {
        let policy = fixture
            .contract
            .get_policy(account("agent.testnet"))
            .expect("test agent policy missing");
        execute_with_binding(
            fixture,
            recipient,
            amount,
            &policy.destination_chain_id,
            &policy.derivation_path,
            nonce,
            expiry,
            version,
            sig,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_with_binding(
        fixture: &mut TestFixture,
        recipient: &str,
        amount: u128,
        destination_chain_id: &str,
        derivation_path: &str,
        nonce: u64,
        expiry: u64,
        version: u64,
        sig: Vec<u8>,
    ) {
        let _promise = fixture.contract.execute_signed_intent(
            account("agent.testnet"),
            account(recipient),
            U128(amount),
            destination_chain_id.to_owned(),
            derivation_path.to_owned(),
            nonce,
            expiry,
            version,
            Base64VecU8(fixture.public_key.clone()),
            Base64VecU8(sig),
        );
    }

    #[test]
    fn valid_ml_dsa_intent_reserves_spend_and_advances_nonce() {
        let mut fixture = setup();
        let sig = signed_intent(
            &fixture,
            "agent.testnet",
            "merchant.testnet",
            250,
            0,
            NOW + 60 * 1_000_000_000,
            1,
        );
        execute(
            &mut fixture,
            "merchant.testnet",
            250,
            0,
            NOW + 60 * 1_000_000_000,
            1,
            sig,
        );
        let policy = fixture
            .contract
            .get_policy(account("agent.testnet"))
            .unwrap();
        assert_eq!(policy.reserved, U128(250));
        assert_eq!(policy.spent, U128(0));
        assert_eq!(policy.next_nonce, 1);
        assert_eq!(policy.pending_count, 1);
    }

    #[test]
    fn tampering_amount_or_recipient_invalidates_signature_or_policy() {
        let fixture = setup();
        let sig = signed_intent(
            &fixture,
            "agent.testnet",
            "merchant.testnet",
            250,
            0,
            NOW + 60 * 1_000_000_000,
            1,
        );
        let mut changed_amount = TestFixture {
            contract: fixture.contract,
            key: fixture.key,
            public_key: fixture.public_key,
        };
        let bad = catch_unwind(AssertUnwindSafe(|| {
            execute(
                &mut changed_amount,
                "merchant.testnet",
                251,
                0,
                NOW + 60 * 1_000_000_000,
                1,
                sig.clone(),
            );
        }));
        assert!(bad.is_err());
        let bad_recipient = catch_unwind(AssertUnwindSafe(|| {
            execute(
                &mut changed_amount,
                "attacker.testnet",
                250,
                0,
                NOW + 60 * 1_000_000_000,
                1,
                sig,
            );
        }));
        assert!(bad_recipient.is_err());
    }

    #[test]
    fn replay_wrong_key_expiry_and_stale_generation_are_rejected() {
        let mut fixture = setup();
        let sig = signed_intent(
            &fixture,
            "agent.testnet",
            "merchant.testnet",
            100,
            0,
            NOW + 30 * 1_000_000_000,
            1,
        );
        execute(
            &mut fixture,
            "merchant.testnet",
            100,
            0,
            NOW + 30 * 1_000_000_000,
            1,
            sig.clone(),
        );
        let replay = catch_unwind(AssertUnwindSafe(|| {
            execute(
                &mut fixture,
                "merchant.testnet",
                100,
                0,
                NOW + 30 * 1_000_000_000,
                1,
                sig.clone(),
            );
        }));
        assert!(replay.is_err());

        let expired = catch_unwind(AssertUnwindSafe(|| {
            execute(
                &mut fixture,
                "merchant.testnet",
                100,
                1,
                NOW - 1,
                1,
                vec![0; ML_DSA_65_SIGNATURE_LEN],
            );
        }));
        assert!(expired.is_err());
        let stale = catch_unwind(AssertUnwindSafe(|| {
            execute(
                &mut fixture,
                "merchant.testnet",
                100,
                1,
                NOW + 30 * 1_000_000_000,
                2,
                vec![0; ML_DSA_65_SIGNATURE_LEN],
            );
        }));
        assert!(stale.is_err());
    }

    #[test]
    fn cumulative_cap_accounts_for_in_flight_reservations() {
        let mut fixture = setup();
        let first = signed_intent(
            &fixture,
            "agent.testnet",
            "merchant.testnet",
            300,
            0,
            NOW + 30 * 1_000_000_000,
            1,
        );
        execute(
            &mut fixture,
            "merchant.testnet",
            300,
            0,
            NOW + 30 * 1_000_000_000,
            1,
            first,
        );
        let second = signed_intent(
            &fixture,
            "agent.testnet",
            "merchant.testnet",
            300,
            1,
            NOW + 30 * 1_000_000_000,
            1,
        );
        execute(
            &mut fixture,
            "merchant.testnet",
            300,
            1,
            NOW + 30 * 1_000_000_000,
            1,
            second,
        );
        let policy = fixture
            .contract
            .get_policy(account("agent.testnet"))
            .unwrap();
        assert_eq!(policy.reserved, U128(600));
        assert_eq!(policy.next_nonce, 2);

        let over_cap = signed_intent(
            &fixture,
            "agent.testnet",
            "merchant.testnet",
            1,
            2,
            NOW + 30 * 1_000_000_000,
            1,
        );
        let denied = catch_unwind(AssertUnwindSafe(|| {
            execute(
                &mut fixture,
                "merchant.testnet",
                1,
                2,
                NOW + 30 * 1_000_000_000,
                1,
                over_cap,
            );
        }));
        assert!(denied.is_err());
    }

    #[test]
    fn successful_callback_commits_spend_and_clears_reservation() {
        let mut fixture = setup();
        let initial_balance = fixture.contract.get_status().available_balance;
        let sig = signed_intent(
            &fixture,
            "agent.testnet",
            "merchant.testnet",
            250,
            0,
            NOW + 30 * 1_000_000_000,
            1,
        );
        execute(
            &mut fixture,
            "merchant.testnet",
            250,
            0,
            NOW + 30 * 1_000_000_000,
            1,
            sig,
        );

        assert!(fixture.contract.on_transfer_result(0, Ok(())));
        let policy = fixture
            .contract
            .get_policy(account("agent.testnet"))
            .unwrap();
        assert_eq!(policy.spent, U128(250));
        assert_eq!(policy.reserved, U128(0));
        assert_eq!(policy.pending_count, 0);
        assert_eq!(
            fixture.contract.get_status().available_balance,
            U128(initial_balance.0 - 250)
        );
    }

    #[test]
    fn failed_callback_refunds_available_balance_and_reservation() {
        let mut fixture = setup();
        let initial_balance = fixture.contract.get_status().available_balance;
        let sig = signed_intent(
            &fixture,
            "agent.testnet",
            "merchant.testnet",
            250,
            0,
            NOW + 30 * 1_000_000_000,
            1,
        );
        execute(
            &mut fixture,
            "merchant.testnet",
            250,
            0,
            NOW + 30 * 1_000_000_000,
            1,
            sig,
        );

        assert!(!fixture
            .contract
            .on_transfer_result(0, Err(PromiseError::Failed)));
        let policy = fixture
            .contract
            .get_policy(account("agent.testnet"))
            .unwrap();
        assert_eq!(policy.spent, U128(0));
        assert_eq!(policy.reserved, U128(0));
        assert_eq!(policy.pending_count, 0);
        assert_eq!(
            fixture.contract.get_status().available_balance,
            initial_balance
        );
    }

    #[test]
    fn randomized_state_machine_preserves_accounting_invariants() {
        let mut fixture = setup();
        let mut seed = 0x5143_5051_2026_1101_u64;

        for _ in 0..64 {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let amount = (seed % 350 + 1) as u128;
            let before = fixture
                .contract
                .get_policy(account("agent.testnet"))
                .unwrap();
            let allowed = amount <= before.per_tx_limit.0
                && before.spent.0 + before.reserved.0 + amount <= before.total_cap.0
                && before.pending_count < MAX_PENDING_PER_AGENT
                && fixture.contract.get_status().available_balance.0
                    >= amount.saturating_add(MIN_BALANCE_RESERVE);
            let expiry = NOW + 30 * 1_000_000_000;
            let signature = signed_intent(
                &fixture,
                "agent.testnet",
                "merchant.testnet",
                amount,
                before.next_nonce,
                expiry,
                before.policy_version,
            );
            let result = catch_unwind(AssertUnwindSafe(|| {
                execute(
                    &mut fixture,
                    "merchant.testnet",
                    amount,
                    before.next_nonce,
                    expiry,
                    before.policy_version,
                    signature,
                );
            }));
            assert_eq!(
                result.is_ok(),
                allowed,
                "unexpected decision for amount {amount}"
            );

            let after = fixture
                .contract
                .get_policy(account("agent.testnet"))
                .unwrap();
            assert!(after.spent.0 + after.reserved.0 <= after.total_cap.0);
            assert!(after.pending_count <= MAX_PENDING_PER_AGENT);
            assert_eq!(after.next_nonce, before.next_nonce + u64::from(allowed));
        }
    }

    #[test]
    fn destination_chain_and_derivation_path_mismatches_fail_before_promise() {
        let mut fixture = setup();
        let initial_balance = fixture.contract.get_status().available_balance;
        let expiry = NOW + 30 * 1_000_000_000;
        let signature = signed_intent(
            &fixture,
            "agent.testnet",
            "merchant.testnet",
            100,
            0,
            expiry,
            1,
        );

        let wrong_chain = catch_unwind(AssertUnwindSafe(|| {
            execute_with_binding(
                &mut fixture,
                "merchant.testnet",
                100,
                "mainnet",
                TEST_DERIVATION_PATH,
                0,
                expiry,
                1,
                signature.clone(),
            );
        }));
        assert!(wrong_chain.is_err());

        let wrong_path = catch_unwind(AssertUnwindSafe(|| {
            execute_with_binding(
                &mut fixture,
                "merchant.testnet",
                100,
                TEST_CHAIN_ID,
                "m/44'/397'/1'",
                0,
                expiry,
                1,
                signature,
            );
        }));
        assert!(wrong_path.is_err());
        assert_eq!(fixture.contract.next_operation_id, 0);
        assert!(fixture.contract.pending_transfers.get(&0).is_none());
        assert_eq!(
            fixture.contract.get_status().available_balance,
            initial_balance
        );
        assert!(get_logs().iter().all(|log| !log.starts_with("EVENT_JSON:")));
    }

    #[test]
    fn empty_and_control_character_chain_or_path_inputs_are_rejected() {
        let mut fixture = setup();
        let expiry = NOW + 30 * 1_000_000_000;
        let signature = vec![0; ML_DSA_65_SIGNATURE_LEN];

        for (chain_id, path) in [
            ("", TEST_DERIVATION_PATH),
            ("test\n", TEST_DERIVATION_PATH),
            (TEST_CHAIN_ID, ""),
            (TEST_CHAIN_ID, "m/44'/397'/0'\n"),
        ] {
            let denied = catch_unwind(AssertUnwindSafe(|| {
                execute_with_binding(
                    &mut fixture,
                    "merchant.testnet",
                    100,
                    chain_id,
                    path,
                    0,
                    expiry,
                    1,
                    signature.clone(),
                );
            }));
            assert!(
                denied.is_err(),
                "accepted chain/path input {chain_id:?} {path:?}"
            );
        }

        for (chain_id, path) in [
            ("", TEST_DERIVATION_PATH),
            ("test\t", TEST_DERIVATION_PATH),
            (TEST_CHAIN_ID, ""),
            (TEST_CHAIN_ID, "m/44'/397'/0'\u{0001}"),
        ] {
            let denied = catch_unwind(AssertUnwindSafe(|| {
                fixture.contract.configure_agent(
                    account("agent2.testnet"),
                    account("merchant.testnet"),
                    chain_id.to_owned(),
                    path.to_owned(),
                    EXPIRY,
                    U128(100),
                    U128(100),
                    Base64VecU8(fixture.public_key.clone()),
                );
            }));
            assert!(
                denied.is_err(),
                "registered chain/path input {chain_id:?} {path:?}"
            );
        }

        assert_eq!(fixture.contract.next_operation_id, 0);
        assert!(fixture.contract.pending_transfers.get(&0).is_none());
    }

    #[test]
    fn execution_audit_persists_chain_binding_and_derivation_path() {
        let mut fixture = setup();
        let expiry = NOW + 30 * 1_000_000_000;
        let signature = signed_intent(
            &fixture,
            "agent.testnet",
            "merchant.testnet",
            100,
            0,
            expiry,
            1,
        );
        execute(
            &mut fixture,
            "merchant.testnet",
            100,
            0,
            expiry,
            1,
            signature,
        );

        let pending = fixture.contract.pending_transfers.get(&0).unwrap();
        let audit = pending.audit_record.as_ref().unwrap();
        assert_eq!(audit.source_chain_id, TEST_CHAIN_ID);
        assert_eq!(audit.destination_chain_id, TEST_CHAIN_ID);
        assert_eq!(audit.derivation_path, TEST_DERIVATION_PATH);
        assert_eq!(audit.nonce, 0);

        let logs = get_logs();
        let submitted = logs
            .iter()
            .find(|log| log.starts_with("EVENT_JSON:"))
            .expect("missing execution audit event");
        let submitted_json: near_sdk::serde_json::Value =
            near_sdk::serde_json::from_str(&submitted["EVENT_JSON:".len()..]).unwrap();
        assert_eq!(submitted_json["data"][0]["source_chain_id"], TEST_CHAIN_ID);
        assert_eq!(
            submitted_json["data"][0]["destination_chain_id"],
            TEST_CHAIN_ID
        );
        assert_eq!(
            submitted_json["data"][0]["derivation_path"],
            TEST_DERIVATION_PATH
        );
        assert_eq!(submitted_json["data"][0]["status"], "pending");

        fixture.contract.on_transfer_result(0, Ok(()));
        let settled = get_logs()
            .into_iter()
            .rev()
            .find(|log| log.starts_with("EVENT_JSON:"))
            .expect("missing settlement audit event");
        let settled_json: near_sdk::serde_json::Value =
            near_sdk::serde_json::from_str(&settled["EVENT_JSON:".len()..]).unwrap();
        assert_eq!(
            settled_json["data"][0]["destination_chain_id"],
            TEST_CHAIN_ID
        );
        assert_eq!(
            settled_json["data"][0]["derivation_path"],
            TEST_DERIVATION_PATH
        );
        assert_eq!(settled_json["data"][0]["status"], "succeeded");
    }

    #[test]
    fn stale_transfer_reconciliation_requires_grace_and_is_idempotent() {
        let mut fixture = setup();
        let initial_balance = fixture.contract.get_status().available_balance;
        let expiry = NOW + 30 * 1_000_000_000;
        let signature = signed_intent(
            &fixture,
            "agent.testnet",
            "merchant.testnet",
            100,
            0,
            expiry,
            1,
        );
        execute(
            &mut fixture,
            "merchant.testnet",
            100,
            0,
            expiry,
            1,
            signature,
        );

        testing_env!(VMContextBuilder::new()
            .current_account_id(account("qpc.testnet"))
            .predecessor_account_id(account("owner.testnet"))
            .block_timestamp(NOW + CALLBACK_RECOVERY_DELAY_NS - 1)
            .build());
        let too_early = catch_unwind(AssertUnwindSafe(|| {
            fixture.contract.reconcile_stale_transfer(0, false);
        }));
        assert!(too_early.is_err());
        assert!(fixture.contract.pending_transfers.get(&0).is_some());

        testing_env!(VMContextBuilder::new()
            .current_account_id(account("qpc.testnet"))
            .predecessor_account_id(account("agent.testnet"))
            .block_timestamp(NOW + CALLBACK_RECOVERY_DELAY_NS + 1)
            .build());
        let unauthorized = catch_unwind(AssertUnwindSafe(|| {
            fixture.contract.reconcile_stale_transfer(0, false);
        }));
        assert!(unauthorized.is_err());
        assert!(fixture.contract.pending_transfers.get(&0).is_some());

        testing_env!(VMContextBuilder::new()
            .current_account_id(account("qpc.testnet"))
            .predecessor_account_id(account("owner.testnet"))
            .block_timestamp(NOW + CALLBACK_RECOVERY_DELAY_NS + 1)
            .build());
        fixture.contract.reconcile_stale_transfer(0, false);
        let policy = fixture
            .contract
            .get_policy(account("agent.testnet"))
            .unwrap();
        assert_eq!(policy.spent, U128(0));
        assert_eq!(policy.reserved, U128(0));
        assert_eq!(policy.pending_count, 0);
        assert_eq!(policy.next_nonce, 1);
        assert_eq!(
            fixture.contract.get_status().available_balance,
            initial_balance
        );
        assert!(fixture.contract.pending_transfers.get(&0).is_none());
        assert!(get_logs().iter().any(|log| {
            log.contains("owner_reconciled_failed")
                && log.contains(TEST_CHAIN_ID)
                && log.contains(TEST_DERIVATION_PATH)
        }));

        // The callback may arrive after manual reconciliation; it must not change balances twice.
        assert!(!fixture
            .contract
            .on_transfer_result(0, Err(PromiseError::Failed)));
        assert_eq!(
            fixture.contract.get_status().available_balance,
            initial_balance
        );
    }

    #[test]
    fn stale_transfer_success_reconciliation_commits_spend() {
        let mut fixture = setup();
        let initial_balance = fixture.contract.get_status().available_balance;
        let expiry = NOW + 30 * 1_000_000_000;
        let signature = signed_intent(
            &fixture,
            "agent.testnet",
            "merchant.testnet",
            100,
            0,
            expiry,
            1,
        );
        execute(
            &mut fixture,
            "merchant.testnet",
            100,
            0,
            expiry,
            1,
            signature,
        );

        testing_env!(VMContextBuilder::new()
            .current_account_id(account("qpc.testnet"))
            .predecessor_account_id(account("owner.testnet"))
            .block_timestamp(NOW + CALLBACK_RECOVERY_DELAY_NS + 1)
            .build());
        fixture.contract.reconcile_stale_transfer(0, true);
        let policy = fixture
            .contract
            .get_policy(account("agent.testnet"))
            .unwrap();
        assert_eq!(policy.spent, U128(100));
        assert_eq!(policy.reserved, U128(0));
        assert_eq!(policy.pending_count, 0);
        assert_eq!(
            fixture.contract.get_status().available_balance,
            U128(initial_balance.0 - 100)
        );
    }
}
