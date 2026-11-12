//! PQP NEAR policy and constrained asynchronous execution core.
//!
//! Execution is limited to exact policy-committed argument bytes, one target
//! function call, fixed gas budgets, and a private idempotent callback. Attached
//! deposit and call quota are reserved at dispatch and are NOT refunded on failure.

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::store::LookupMap;
use near_sdk::{
    env, near, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault, Promise,
    PromiseError,
};
use std::collections::BTreeSet;

const MAX_POLICY_DURATION_NS: u64 = 90 * 24 * 60 * 60 * 1_000_000_000;
const MAX_RULES: usize = 16;
const MAX_ID_LEN: usize = 64;
const MAX_METHOD_LEN: usize = 64;
const MAX_ARGS_LEN: usize = 4_096;
const MAX_RESULT_LEN: usize = 256;
const TARGET_GAS_TGAS: u64 = 20;
const CALLBACK_GAS_TGAS: u64 = 15;

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    Policies,
    Usage,
    ActionUsage,
    ActionCalls,
    Requests,
    Executions,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct ActionRule {
    pub action_id: String,
    pub target: AccountId,
    pub method_name: String,
    /// SHA-256 of the exact argument bytes accepted by this rule.
    pub args_hash: Vec<u8>,
    pub max_per_call: u128,
    pub cumulative_cap: u128,
    /// Maximum dispatch count, including calls that later fail.
    pub max_calls: u32,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct PolicyInput {
    pub agent_id: AccountId,
    pub valid_after_ns: u64,
    pub expires_at_ns: u64,
    pub total_deposit_cap: u128,
    pub rules: Vec<ActionRule>,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct Policy {
    pub policy_id: String,
    pub version: u32,
    pub agent_id: AccountId,
    pub valid_after_ns: u64,
    pub expires_at_ns: u64,
    pub total_deposit_cap: u128,
    pub rules: Vec<ActionRule>,
    pub paused: bool,
    pub revoked: bool,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct Usage {
    pub next_nonce: u64,
    /// Conservatively reserved at dispatch; failed calls do not refund this amount.
    pub total_deposit_used: u128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct ActionRequest {
    pub policy_id: String,
    pub expected_version: u32,
    pub request_id: String,
    pub nonce: u64,
    pub action_id: String,
    pub target: AccountId,
    pub method_name: String,
    /// Exact bytes sent to the target. The contract hashes these bytes itself.
    pub args: Vec<u8>,
    pub attached_deposit: u128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct AuthorizationReceipt {
    pub policy_id: String,
    pub policy_version: u32,
    pub agent_id: AccountId,
    pub request_id: String,
    pub nonce: u64,
    pub action_id: String,
    pub target: AccountId,
    pub method_name: String,
    pub args_hash: Vec<u8>,
    pub authorized_deposit: u128,
}

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug, PartialEq,
)]
#[serde(crate = "near_sdk::serde")]
pub enum ExecutionStatus {
    Dispatched,
    Succeeded,
    Failed,
    SucceededWithOversizedResult,
}

#[derive(
    BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Debug, PartialEq,
)]
#[serde(crate = "near_sdk::serde")]
pub struct ExecutionRecord {
    pub policy_id: String,
    pub request_id: String,
    pub nonce: u64,
    pub policy_version: u32,
    pub action_id: String,
    pub target: AccountId,
    pub attached_deposit: u128,
    pub status: ExecutionStatus,
    pub result_len: Option<u64>,
    pub result_hash: Option<Vec<u8>>,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct PqpNear {
    owner_id: AccountId,
    policies: LookupMap<String, Policy>,
    usage: LookupMap<String, Usage>,
    action_usage: LookupMap<String, u128>,
    action_calls: LookupMap<String, u32>,
    used_requests: LookupMap<String, bool>,
    executions: LookupMap<String, ExecutionRecord>,
}

#[near]
impl PqpNear {
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        Self {
            owner_id,
            policies: LookupMap::new(StorageKey::Policies),
            usage: LookupMap::new(StorageKey::Usage),
            action_usage: LookupMap::new(StorageKey::ActionUsage),
            action_calls: LookupMap::new(StorageKey::ActionCalls),
            used_requests: LookupMap::new(StorageKey::Requests),
            executions: LookupMap::new(StorageKey::Executions),
        }
    }

    pub fn create_policy(&mut self, policy_id: String, input: PolicyInput) {
        self.assert_owner();
        assert_valid_id(&policy_id, "invalid policy id");
        assert!(self.policies.get(&policy_id).is_none(), "policy already exists");
        validate_input(&input, env::block_timestamp());
        self.policies.insert(policy_id.clone(), Policy {
            policy_id: policy_id.clone(), version: 1, agent_id: input.agent_id,
            valid_after_ns: input.valid_after_ns, expires_at_ns: input.expires_at_ns,
            total_deposit_cap: input.total_deposit_cap, rules: input.rules,
            paused: false, revoked: false,
        });
        self.usage.insert(policy_id.clone(), Usage::default());
        log_event("policy_created", &policy_id, 1);
    }

    /// Updating a policy never resets reserved deposit, call quota, or replay nonce.
    pub fn update_policy(&mut self, policy_id: String, expected_version: u32, input: PolicyInput) {
        self.assert_owner();
        let mut policy = self.policy_or_panic(&policy_id);
        assert!(!policy.revoked, "revoked policy is immutable");
        assert_eq!(policy.version, expected_version, "stale policy version");
        assert_eq!(input.agent_id, policy.agent_id, "agent identity is immutable");
        validate_input(&input, env::block_timestamp());
        let usage = self.usage.get(&policy_id).cloned().unwrap_or_default();
        assert!(input.total_deposit_cap >= usage.total_deposit_used, "total cap below reserved amount");
        for rule in &input.rules {
            let key = action_usage_key(&policy_id, &rule.action_id);
            let spent = self.action_usage.get(&key).copied().unwrap_or(0);
            let calls = self.action_calls.get(&key).copied().unwrap_or(0);
            assert!(rule.cumulative_cap >= spent, "action cap below reserved amount");
            assert!(rule.max_calls >= calls, "call quota below already-dispatched calls");
        }
        policy.version = policy.version.checked_add(1).expect("policy version overflow");
        policy.valid_after_ns = input.valid_after_ns;
        policy.expires_at_ns = input.expires_at_ns;
        policy.total_deposit_cap = input.total_deposit_cap;
        policy.rules = input.rules;
        let version = policy.version;
        self.policies.insert(policy_id.clone(), policy);
        log_event("policy_updated", &policy_id, version);
    }

    pub fn set_paused(&mut self, policy_id: String, paused: bool) {
        self.assert_owner();
        let mut policy = self.policy_or_panic(&policy_id);
        assert!(!policy.revoked, "policy revoked");
        if policy.paused != paused {
            policy.paused = paused;
            policy.version = policy.version.checked_add(1).expect("policy version overflow");
            let version = policy.version;
            self.policies.insert(policy_id.clone(), policy);
            log_event(if paused { "policy_paused" } else { "policy_unpaused" }, &policy_id, version);
        }
    }

    pub fn revoke_policy(&mut self, policy_id: String) {
        self.assert_owner();
        let mut policy = self.policy_or_panic(&policy_id);
        assert!(!policy.revoked, "policy already revoked");
        policy.revoked = true;
        policy.paused = true;
        policy.version = policy.version.checked_add(1).expect("policy version overflow");
        let version = policy.version;
        self.policies.insert(policy_id.clone(), policy);
        log_event("policy_revoked", &policy_id, version);
    }

    /// Dispatch one exact policy-approved call. Budget and nonce are reserved before
    /// the promise is created. A failed target still consumes both to prevent retry abuse.
    pub fn dispatch_action(&mut self, request: ActionRequest) -> Promise {
        let caller = env::predecessor_account_id();
        let now = env::block_timestamp();
        assert_valid_id(&request.policy_id, "invalid policy id");
        assert_valid_id(&request.request_id, "invalid request id");
        assert!(request.args.len() <= MAX_ARGS_LEN, "arguments exceed maximum length");
        assert_eq!(env::attached_deposit().as_yoctonear(), request.attached_deposit, "attached deposit must exactly fund target call");

        let policy = self.policy_or_panic(&request.policy_id);
        assert_eq!(caller, policy.agent_id, "caller is not the policy agent");
        assert!(!policy.revoked, "policy revoked");
        assert!(!policy.paused, "policy paused");
        assert!(now >= policy.valid_after_ns, "policy not active yet");
        assert!(now < policy.expires_at_ns, "policy expired");
        assert_eq!(request.expected_version, policy.version, "stale policy version");

        let mut usage = self.usage.get(&request.policy_id).cloned().unwrap_or_default();
        assert_eq!(request.nonce, usage.next_nonce, "invalid or replayed nonce");
        let request_storage_key = request_key(&request.policy_id, &request.request_id);
        assert!(self.used_requests.get(&request_storage_key).is_none(), "duplicate request id");

        let rule = policy.rules.iter().find(|rule| rule.action_id == request.action_id)
            .expect("action is not permitted");
        assert_eq!(request.target, rule.target, "target mismatch");
        assert_eq!(request.method_name, rule.method_name, "method mismatch");
        let args_hash = env::sha256(&request.args);
        assert_eq!(args_hash, rule.args_hash, "arguments hash mismatch");
        assert!(request.attached_deposit <= rule.max_per_call, "per-call limit exceeded");

        let key = action_usage_key(&request.policy_id, &request.action_id);
        let action_spent = self.action_usage.get(&key).copied().unwrap_or(0);
        let next_action_spent = action_spent.checked_add(request.attached_deposit)
            .expect("action usage overflow");
        assert!(next_action_spent <= rule.cumulative_cap, "action budget exceeded");
        let calls = self.action_calls.get(&key).copied().unwrap_or(0);
        let next_calls = calls.checked_add(1).expect("action call count overflow");
        assert!(next_calls <= rule.max_calls, "action call count exceeded");
        let next_total = usage.total_deposit_used.checked_add(request.attached_deposit)
            .expect("total usage overflow");
        assert!(next_total <= policy.total_deposit_cap, "policy budget exceeded");
        usage.next_nonce = usage.next_nonce.checked_add(1).expect("nonce overflow");
        usage.total_deposit_used = next_total;

        let record = ExecutionRecord {
            policy_id: request.policy_id.clone(), request_id: request.request_id.clone(),
            nonce: request.nonce, policy_version: policy.version,
            action_id: request.action_id.clone(), target: request.target.clone(),
            attached_deposit: request.attached_deposit, status: ExecutionStatus::Dispatched,
            result_len: None, result_hash: None,
        };

        // Checks-effects-interactions: all replay and quota state is committed in this
        // receipt before the cross-contract promise is returned.
        self.usage.insert(request.policy_id.clone(), usage);
        self.action_usage.insert(key.clone(), next_action_spent);
        self.action_calls.insert(key, next_calls);
        self.used_requests.insert(request_storage_key.clone(), true);
        self.executions.insert(request_storage_key, record);
        log_event("action_dispatched", &request.policy_id, policy.version);

        let callback_args = near_sdk::serde_json::json!({
            "policy_id": request.policy_id,
            "request_id": request.request_id,
            "nonce": request.nonce,
        }).to_string().into_bytes();

        Promise::new(request.target)
            .function_call(
                request.method_name,
                request.args,
                NearToken::from_yoctonear(request.attached_deposit),
                Gas::from_tgas(TARGET_GAS_TGAS),
            )
            .then(
                Promise::new(env::current_account_id()).function_call(
                    "on_action_result".to_string(),
                    callback_args,
                    NearToken::from_yoctonear(0),
                    Gas::from_tgas(CALLBACK_GAS_TGAS),
                ),
            )
    }

    /// NEAR runtime only: predecessor must be this contract. A bounded result read
    /// prevents an untrusted target from exhausting callback memory/gas.
    #[private]
    pub fn on_action_result(&mut self, policy_id: String, request_id: String, nonce: u64) -> ExecutionStatus {
        assert_eq!(env::promise_results_count(), 1, "expected exactly one promise result");
        let key = request_key(&policy_id, &request_id);
        let mut execution = self.executions.get(&key).cloned().expect("execution not found");
        assert_eq!(execution.nonce, nonce, "callback nonce mismatch");

        // A terminal record is immutable. This makes duplicate callbacks idempotent.
        if execution.status != ExecutionStatus::Dispatched {
            return execution.status;
        }

        let (status, result_len, result_hash) =
            match env::promise_result_checked(0, MAX_RESULT_LEN) {
                Ok(bytes) => (
                    ExecutionStatus::Succeeded,
                    Some(bytes.len() as u64),
                    Some(env::sha256(&bytes)),
                ),
                Err(PromiseError::Failed) => (ExecutionStatus::Failed, None, None),
                Err(PromiseError::TooLong(actual_len)) => (
                    ExecutionStatus::SucceededWithOversizedResult,
                    Some(actual_len as u64),
                    None,
                ),
                // PromiseError is non-exhaustive; unknown runtime errors fail closed.
                Err(_) => (ExecutionStatus::Failed, None, None),
            };

        execution.status = status.clone();
        execution.result_len = result_len;
        execution.result_hash = result_hash;
        self.executions.insert(key, execution);
        log_event(match status {
            ExecutionStatus::Succeeded => "action_succeeded",
            ExecutionStatus::Failed => "action_failed",
            ExecutionStatus::SucceededWithOversizedResult => "action_succeeded_oversized_result",
            ExecutionStatus::Dispatched => "action_dispatched",
        }, &policy_id, self.policy_or_panic(&policy_id).version);
        status
    }

    pub fn get_execution(&self, policy_id: String, request_id: String) -> Option<ExecutionRecord> {
        self.executions.get(&request_key(&policy_id, &request_id)).cloned()
    }

    pub fn get_policy(&self, policy_id: String) -> Option<Policy> {
        self.policies.get(&policy_id).cloned()
    }

    pub fn get_usage(&self, policy_id: String) -> Usage {
        self.usage.get(&policy_id).cloned().unwrap_or_default()
    }

    pub fn get_action_usage(&self, policy_id: String, action_id: String) -> u128 {
        self.action_usage.get(&action_usage_key(&policy_id, &action_id)).copied().unwrap_or(0)
    }

    pub fn get_action_call_count(&self, policy_id: String, action_id: String) -> u32 {
        self.action_calls.get(&action_usage_key(&policy_id, &action_id)).copied().unwrap_or(0)
    }

    fn assert_owner(&self) {
        assert_eq!(env::predecessor_account_id(), self.owner_id, "owner only");
    }

    fn policy_or_panic(&self, policy_id: &str) -> Policy {
        self.policies.get(&policy_id.to_string()).cloned().expect("policy not found")
    }
}

fn validate_input(input: &PolicyInput, now: u64) {
    assert!(input.valid_after_ns >= now, "valid-after must not be in the past");
    assert!(input.expires_at_ns > input.valid_after_ns, "invalid policy interval");
    assert!(input.expires_at_ns - now <= MAX_POLICY_DURATION_NS, "policy duration exceeds 90 days");
    assert!(input.total_deposit_cap > 0, "total cap must be positive");
    assert!(!input.rules.is_empty() && input.rules.len() <= MAX_RULES, "invalid rule count");
    let mut ids = BTreeSet::new();
    for rule in &input.rules {
        assert_valid_id(&rule.action_id, "invalid action id");
        assert!(ids.insert(rule.action_id.clone()), "duplicate action id");
        assert!(rule.target != env::current_account_id(), "self-target is not allowed");
        assert!(
            !rule.method_name.is_empty() && rule.method_name.len() <= MAX_METHOD_LEN
                && rule.method_name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
            "invalid method name"
        );
        assert_eq!(rule.args_hash.len(), 32, "arguments hash must be 32 bytes");
        assert!(rule.max_per_call > 0, "per-call cap must be positive");
        assert!(rule.cumulative_cap >= rule.max_per_call, "cumulative cap below per-call cap");
        assert!(rule.max_calls > 0, "maximum calls must be positive");
    }
}

fn assert_valid_id(value: &str, message: &str) {
    assert!(
        !value.is_empty() && value.len() <= MAX_ID_LEN
            && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "{}", message
    );
}

fn action_usage_key(policy_id: &str, action_id: &str) -> String {
    format!("{}:{}", policy_id, action_id)
}

fn request_key(policy_id: &str, request_id: &str) -> String {
    format!("{}:{}", policy_id, request_id)
}

fn log_event(event: &str, policy_id: &str, version: u32) {
    env::log_str(&format!(
        "EVENT_JSON:{{\"standard\":\"pqp\",\"version\":\"1.0.0\",\"event\":\"{}\",\"data\":[{{\"policy_id\":\"{}\",\"policy_version\":{}}}]}}",
        event, policy_id, version
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::VMContextBuilder;
    use near_sdk::testing_env;

    const NOW: u64 = 1_000_000_000_000;
    const ARGS: &[u8] = br#"{}"#;

    fn set_context(predecessor: &str, timestamp: u64) {
        let mut builder = VMContextBuilder::new();
        builder.predecessor_account_id(predecessor.parse().unwrap())
            .current_account_id("pqp.near".parse().unwrap())
            .block_timestamp(timestamp);
        testing_env!(builder.build());
    }

    fn input() -> PolicyInput {
        PolicyInput {
            agent_id: "agent.near".parse().unwrap(),
            valid_after_ns: NOW,
            expires_at_ns: NOW + 10_000_000_000,
            total_deposit_cap: 30,
            rules: vec![ActionRule {
                action_id: "vault_deposit".to_string(),
                target: "vault.near".parse().unwrap(),
                method_name: "deposit".to_string(),
                args_hash: env::sha256(ARGS),
                max_per_call: 10,
                cumulative_cap: 20,
                max_calls: 3,
            }],
        }
    }

    fn setup() -> PqpNear {
        set_context("owner.near", NOW);
        let mut contract = PqpNear::new("owner.near".parse().unwrap());
        contract.create_policy("policy_1".to_string(), input());
        contract
    }

    fn request(nonce: u64, request_id: &str, amount: u128) -> ActionRequest {
        ActionRequest {
            policy_id: "policy_1".to_string(), expected_version: 1,
            request_id: request_id.to_string(), nonce, action_id: "vault_deposit".to_string(),
            target: "vault.near".parse().unwrap(), method_name: "deposit".to_string(),
            args: ARGS.to_vec(), attached_deposit: amount,
        }
    }

    #[test]
    fn owner_can_create_and_agent_can_dispatch_within_policy() {
        let mut contract = setup();
        set_context("agent.near", NOW);
        let _promise = contract.dispatch_action(request(0, "req_1", 7));
        assert_eq!(contract.get_execution("policy_1".into(), "req_1".into()).unwrap().status, ExecutionStatus::Dispatched);
        assert_eq!(contract.get_usage("policy_1".into()).next_nonce, 1);
        assert_eq!(contract.get_action_usage("policy_1".into(), "vault_deposit".into()), 7);
        assert_eq!(contract.get_action_call_count("policy_1".into(), "vault_deposit".into()), 1);
    }

    #[test]
    #[should_panic(expected = "caller is not the policy agent")]
    fn unrelated_account_cannot_dispatch() {
        let mut contract = setup();
        set_context("attacker.near", NOW);
        let _promise = contract.dispatch_action(request(0, "req_1", 1));
    }

    #[test]
    #[should_panic(expected = "invalid or replayed nonce")]
    fn nonce_replay_is_rejected() {
        let mut contract = setup();
        set_context("agent.near", NOW);
        let _first = contract.dispatch_action(request(0, "req_1", 1));
        let _second = contract.dispatch_action(request(0, "req_2", 1));
    }

    #[test]
    #[should_panic(expected = "arguments hash mismatch")]
    fn argument_substitution_is_rejected() {
        let mut contract = setup();
        set_context("agent.near", NOW);
        let mut malicious = request(0, "req_1", 1);
        malicious.args = br#"{"recipient":"attacker.near"}"#.to_vec();
        let _promise = contract.dispatch_action(malicious);
    }

    #[test]
    #[should_panic(expected = "action budget exceeded")]
    fn cumulative_action_budget_is_enforced() {
        let mut contract = setup();
        set_context("agent.near", NOW);
        let _first = contract.dispatch_action(request(0, "req_1", 10));
        let _second = contract.dispatch_action(request(1, "req_2", 10));
        let _third = contract.dispatch_action(request(2, "req_3", 1));
    }

    #[test]
    #[should_panic(expected = "policy paused")]
    fn paused_policy_fails_closed() {
        let mut contract = setup();
        contract.set_paused("policy_1".to_string(), true);
        set_context("agent.near", NOW);
        let _promise = contract.dispatch_action(request(0, "req_1", 1));
    }

    #[test]
    #[should_panic(expected = "policy expired")]
    fn expired_policy_fails_closed() {
        let mut contract = setup();
        set_context("agent.near", NOW + 10_000_000_000);
        let _promise = contract.dispatch_action(request(0, "req_1", 1));
    }
}
