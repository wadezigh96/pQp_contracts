use near_sdk::test_utils::VMContextBuilder;
use near_sdk::{env, testing_env, NearToken};
use qpc_near::{ActionRequest, ActionRule, ExecutionStatus, PolicyInput, QpcNear};

const NOW: u64 = 1_000_000_000_000;
const ARGS: &[u8] = br#"{}"#;

fn context(predecessor: &str, timestamp: u64, deposit: u128) {
    let mut builder = VMContextBuilder::new();
    builder
        .predecessor_account_id(predecessor.parse().unwrap())
        .current_account_id("qpc.near".parse().unwrap())
        .account_balance(NearToken::from_near(100))
        .attached_deposit(NearToken::from_yoctonear(deposit))
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
            destination_chain_id: "near:mainnet".to_string(),
            derivation_path: "qpc/near-mainnet/vault".to_string(),
            args_hash: env::sha256(ARGS),
            max_per_call: 10,
            cumulative_cap: 20,
            max_calls: 2,
        }],
    }
}

fn setup() -> QpcNear {
    context("owner.near", NOW, 0);
    let mut contract = QpcNear::new("owner.near".parse().unwrap());
    contract.create_policy("policy_1".to_string(), input());
    contract
}

#[test]
#[should_panic(expected = "invalid destination chain id")]
fn policy_rejects_empty_destination_chain_id() {
    context("owner.near", NOW, 0);
    let mut contract = QpcNear::new("owner.near".parse().unwrap());
    let mut invalid = input();
    invalid.rules[0].destination_chain_id.clear();
    contract.create_policy("invalid_chain_policy".to_string(), invalid);
}

#[test]
#[should_panic(expected = "invalid derivation path")]
fn policy_rejects_control_character_in_derivation_path() {
    context("owner.near", NOW, 0);
    let mut contract = QpcNear::new("owner.near".parse().unwrap());
    let mut invalid = input();
    invalid.rules[0].derivation_path.push('\n');
    contract.create_policy("invalid_path_policy".to_string(), invalid);
}

fn request(expected_version: u32, nonce: u64, request_id: &str, amount: u128) -> ActionRequest {
    ActionRequest {
        policy_id: "policy_1".to_string(),
        expected_version,
        request_id: request_id.to_string(),
        nonce,
        action_id: "vault_deposit".to_string(),
        target: "vault.near".parse().unwrap(),
        method_name: "deposit".to_string(),
        destination_chain_id: "near:mainnet".to_string(),
        derivation_path: "qpc/near-mainnet/vault".to_string(),
        args: ARGS.to_vec(),
        attached_deposit: amount,
    }
}

#[test]
fn policy_update_preserves_reserved_budget_and_nonce() {
    let mut contract = setup();
    context("agent.near", NOW, 7);
    let _promise = contract.dispatch_action(request(1, 0, "req_1", 7));

    context("owner.near", NOW, 0);
    let mut updated = input();
    updated.total_deposit_cap = 30;
    updated.rules[0].cumulative_cap = 20;
    updated.rules[0].max_calls = 2;
    contract.update_policy("policy_1".to_string(), 1, updated);

    let usage = contract.get_usage("policy_1".to_string());
    assert_eq!(usage.next_nonce, 1);
    assert_eq!(usage.total_deposit_used, 7);
    assert_eq!(contract.get_action_usage("policy_1".to_string(), "vault_deposit".to_string()), 7);
    assert_eq!(contract.get_action_call_count("policy_1".to_string(), "vault_deposit".to_string()), 1);
}

#[test]
#[should_panic(expected = "stale policy version")]
fn stale_policy_version_is_rejected_after_owner_update() {
    let mut contract = setup();
    context("owner.near", NOW, 0);
    contract.update_policy("policy_1".to_string(), 1, input());

    context("agent.near", NOW, 1);
    let _promise = contract.dispatch_action(request(1, 0, "req_stale", 1));
}

#[test]
#[should_panic(expected = "arguments exceed maximum length")]
fn oversized_outbound_payload_is_rejected_before_target_call() {
    let mut contract = setup();
    context("agent.near", NOW, 1);
    let mut oversized = request(1, 0, "req_large", 1);
    oversized.args = vec![0u8; 4_097];
    let _promise = contract.dispatch_action(oversized);
}

#[test]
#[should_panic(expected = "invalid or replayed nonce")]
fn nonce_replay_is_rejected_even_with_a_new_request_id() {
    let mut contract = setup();
    context("agent.near", NOW, 1);
    let _first = contract.dispatch_action(request(1, 0, "req_1", 1));
    let _replay = contract.dispatch_action(request(1, 0, "req_2", 1));
}

#[test]
#[should_panic(expected = "invalid or replayed nonce")]
fn nonce_gap_is_rejected_to_prevent_untracked_execution() {
    let mut contract = setup();
    context("agent.near", NOW, 1);
    let _promise = contract.dispatch_action(request(1, 2, "req_gap", 1));
}

#[test]
#[should_panic(expected = "callback nonce mismatch")]
fn callback_with_wrong_nonce_cannot_rewrite_execution_record() {
    let mut contract = setup();
    context("agent.near", NOW, 1);
    let _promise = contract.dispatch_action(request(1, 0, "req_1", 1));

    let mut callback = VMContextBuilder::new();
    callback
        .predecessor_account_id("qpc.near".parse().unwrap())
        .current_account_id("qpc.near".parse().unwrap())
        .block_timestamp(NOW + 1);
    testing_env!(callback.build(), near_sdk::test_utils::test_vm_config(), near_sdk::RuntimeFeesConfig::test(), std::collections::HashMap::new(), vec![near_sdk::PromiseResult::Failed]);
    contract.on_action_result("policy_1".to_string(), "req_1".to_string(), 99);
}

#[test]
fn failed_execution_remains_terminal_and_retains_security_audit_fields() {
    let mut contract = setup();
    context("agent.near", NOW, 1);
    let _promise = contract.dispatch_action(request(1, 0, "req_1", 1));

    let mut callback = VMContextBuilder::new();
    callback
        .predecessor_account_id("qpc.near".parse().unwrap())
        .current_account_id("qpc.near".parse().unwrap())
        .block_timestamp(NOW + 1);
    testing_env!(callback.build(), near_sdk::test_utils::test_vm_config(), near_sdk::RuntimeFeesConfig::test(), std::collections::HashMap::new(), vec![near_sdk::PromiseResult::Failed]);
    assert_eq!(contract.on_action_result("policy_1".to_string(), "req_1".to_string(), 0), ExecutionStatus::Failed);

    let execution = contract.get_execution("policy_1".to_string(), "req_1".to_string()).unwrap();
    assert_eq!(execution.nonce, 0);
    assert_eq!(execution.policy_version, 1);
    assert_eq!(execution.status, ExecutionStatus::Failed);
}
