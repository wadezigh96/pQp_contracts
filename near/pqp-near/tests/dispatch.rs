use near_sdk::test_utils::{get_created_receipts, VMContextBuilder};
use near_sdk::{
    env, test_vm_config, testing_env, AccountId, NearToken, PromiseResult, RuntimeFeesConfig,
};
use pqp_near::{
    ActionRequest, ActionRule, ExecutionStatus, PqpNear, PolicyInput,
};
use std::collections::HashMap;

const NOW: u64 = 1_000_000_000_000;
const ARGS: &[u8] = br#"{}"#;

fn context(predecessor: &str, timestamp: u64, deposit: u128) {
    let mut builder = VMContextBuilder::new();
    builder
        .predecessor_account_id(predecessor.parse().unwrap())
        .current_account_id("pqp.near".parse().unwrap())
        .account_balance(NearToken::from_near(100))
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .block_timestamp(timestamp);
    testing_env!(builder.build());
}

fn callback_context(result: PromiseResult) {
    let mut builder = VMContextBuilder::new();
    builder
        .predecessor_account_id("pqp.near".parse().unwrap())
        .current_account_id("pqp.near".parse().unwrap())
        .account_balance(NearToken::from_near(100))
        .block_timestamp(NOW + 1);
    testing_env!(
        builder.build(),
        test_vm_config(),
        RuntimeFeesConfig::test(),
        HashMap::default(),
        vec![result],
    );
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
            max_calls: 2,
        }],
    }
}

fn setup() -> PqpNear {
    context("owner.near", NOW, 0);
    let mut contract = PqpNear::new("owner.near".parse().unwrap());
    contract.create_policy("policy_1".to_string(), input());
    contract
}

fn request(nonce: u64, request_id: &str, amount: u128) -> ActionRequest {
    ActionRequest {
        policy_id: "policy_1".to_string(),
        expected_version: 1,
        request_id: request_id.to_string(),
        nonce,
        action_id: "vault_deposit".to_string(),
        target: "vault.near".parse().unwrap(),
        method_name: "deposit".to_string(),
        args: ARGS.to_vec(),
        attached_deposit: amount,
    }
}

#[test]
fn dispatch_creates_target_call_and_authenticated_callback() {
    let mut contract = setup();
    context("agent.near", NOW, 7);

    let _promise = contract.dispatch_action(request(0, "req_1", 7));

    let execution = contract.get_execution("policy_1".to_string(), "req_1".to_string()).unwrap();
    assert_eq!(execution.status, ExecutionStatus::Dispatched);
    assert_eq!(execution.nonce, 0);
    assert_eq!(execution.attached_deposit, 7);
    assert_eq!(contract.get_usage("policy_1".to_string()).next_nonce, 1);
    assert_eq!(
        contract.get_action_call_count("policy_1".to_string(), "vault_deposit".to_string()),
        1
    );

    let receivers: Vec<AccountId> = get_created_receipts()
        .into_iter()
        .map(|receipt| receipt.receiver_id)
        .collect();
    assert!(receivers.contains(&"vault.near".parse().unwrap()));
    assert!(receivers.contains(&"pqp.near".parse().unwrap()));
}

#[test]
fn failed_target_call_is_terminal_and_callback_replay_is_idempotent() {
    let mut contract = setup();
    context("agent.near", NOW, 5);
    let _promise = contract.dispatch_action(request(0, "req_1", 5));

    callback_context(PromiseResult::Failed);
    let first = contract.on_action_result(
        "policy_1".to_string(),
        "req_1".to_string(),
        0,
    );
    assert_eq!(first, ExecutionStatus::Failed);

    // A repeated callback cannot rewrite a terminal result.
    callback_context(PromiseResult::Successful(Vec::new()));
    let replay = contract.on_action_result(
        "policy_1".to_string(),
        "req_1".to_string(),
        0,
    );
    assert_eq!(replay, ExecutionStatus::Failed);
    assert_eq!(
        contract.get_execution("policy_1".to_string(), "req_1".to_string()).unwrap().status,
        ExecutionStatus::Failed
    );
}

#[test]
fn oversized_success_result_is_not_misreported_as_target_failure() {
    let mut contract = setup();
    context("agent.near", NOW, 5);
    let _promise = contract.dispatch_action(request(0, "req_1", 5));

    callback_context(PromiseResult::Successful(vec![9; 257]));
    let status = contract.on_action_result(
        "policy_1".to_string(),
        "req_1".to_string(),
        0,
    );

    assert_eq!(status, ExecutionStatus::SucceededWithOversizedResult);
    let execution = contract.get_execution("policy_1".to_string(), "req_1".to_string()).unwrap();
    assert_eq!(execution.result_len, Some(257));
}

#[test]
#[should_panic(expected = "arguments hash mismatch")]
fn dispatch_rejects_argument_bytes_not_committed_by_policy() {
    let mut contract = setup();
    context("agent.near", NOW, 1);
    let mut malicious = request(0, "req_1", 1);
    malicious.args = br#"{"recipient":"attacker.near"}"#.to_vec();
    let _promise = contract.dispatch_action(malicious);
}

#[test]
#[should_panic(expected = "action call count exceeded")]
fn zero_deposit_calls_are_still_bounded_by_policy() {
    let mut contract = setup();
    context("agent.near", NOW, 0);
    let _first = contract.dispatch_action(request(0, "req_1", 0));
    let _second = contract.dispatch_action(request(1, "req_2", 0));
    let _third = contract.dispatch_action(request(2, "req_3", 0));
}

#[test]
#[should_panic(expected = "Method on_action_result is private")]
fn external_account_cannot_forge_execution_callback() {
    let mut contract = setup();
    context("attacker.near", NOW + 1, 0);
    contract.on_action_result(
        "policy_1".to_string(),
        "req_1".to_string(),
        0,
    );
}
