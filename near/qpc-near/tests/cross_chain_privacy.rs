use near_sdk::test_utils::VMContextBuilder;
use near_sdk::{env, testing_env, NearToken};
use qpc_near::{ActionRequest, ActionRule, ExecutionStatus, PolicyInput, QpcNear};

const NOW: u64 = 1_000_000_000_000;
const OPAQUE_ARGS: &[u8] = br#"{\"payment_id\":\"payment-0001\",\"destination\":\"zcash\",\"network\":\"testnet\",\"opaque_recipient_ref\":\"ref-7f2a9c8405d1b6e3f48a2d0c\",\"privacy_class\":\"shielded\",\"amount_zatoshis\":\"25000\"}"#;

fn context(predecessor: &str, deposit: u128) {
    let mut builder = VMContextBuilder::new();
    builder
        .predecessor_account_id(predecessor.parse().unwrap())
        .current_account_id("qpc.near".parse().unwrap())
        .account_balance(NearToken::from_near(100))
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .block_timestamp(NOW);
    testing_env!(builder.build());
}

fn setup() -> QpcNear {
    context("owner.near", 0);
    let mut contract = QpcNear::new("owner.near".parse().unwrap());
    contract.create_policy(
        "near_to_zcash".to_string(),
        PolicyInput {
            agent_id: "agent.near".parse().unwrap(),
            valid_after_ns: NOW,
            expires_at_ns: NOW + 60_000_000_000,
            total_deposit_cap: 10,
            rules: vec![ActionRule {
                action_id: "pay_zcash_shielded".to_string(),
                target: "zcash-router.near".parse().unwrap(),
                method_name: "request_shielded_payment".to_string(),
                args_hash: env::sha256(OPAQUE_ARGS),
                max_per_call: 10,
                cumulative_cap: 10,
                max_calls: 1,
            }],
        },
    );
    contract
}

fn request(target: &str) -> ActionRequest {
    ActionRequest {
        policy_id: "near_to_zcash".to_string(),
        expected_version: 1,
        request_id: "near-zcash-req-1".to_string(),
        nonce: 0,
        action_id: "pay_zcash_shielded".to_string(),
        target: target.parse().unwrap(),
        method_name: "request_shielded_payment".to_string(),
        args: OPAQUE_ARGS.to_vec(),
        attached_deposit: 5,
    }
}

#[test]
fn cross_chain_request_contains_only_opaque_recipient_reference() {
    let request = request("zcash-router.near");
    let payload = String::from_utf8(request.args.clone()).unwrap();

    assert!(payload.contains("opaque_recipient_ref"));
    assert!(payload.contains("privacy_class"));
    assert!(!payload.contains("u1"));
    assert!(!payload.contains("t1"));
    assert!(!payload.contains("private_key"));
    assert!(!payload.contains("seed_phrase"));
}

#[test]
fn policy_hash_binds_exact_privacy_payload_and_amount() {
    let mut contract = setup();
    context("agent.near", 5);
    let _promise = contract.dispatch_action(request("zcash-router.near"));

    let execution = contract
        .get_execution("near_to_zcash".to_string(), "near-zcash-req-1".to_string())
        .unwrap();
    assert_eq!(execution.status, ExecutionStatus::Dispatched);
    assert_eq!(execution.attached_deposit, 5);
    assert_eq!(contract.get_usage("near_to_zcash".to_string()).next_nonce, 1);
}

#[test]
#[should_panic(expected = "arguments hash mismatch")]
fn changing_opaque_recipient_reference_is_rejected() {
    let mut contract = setup();
    context("agent.near", 5);
    let mut changed = request("zcash-router.near");
    changed.args = br#"{\"payment_id\":\"payment-0001\",\"destination\":\"zcash\",\"network\":\"testnet\",\"opaque_recipient_ref\":\"ref-attacker\",\"privacy_class\":\"shielded\",\"amount_zatoshis\":\"25000\"}"#.to_vec();
    let _promise = contract.dispatch_action(changed);
}

#[test]
#[should_panic(expected = "arguments hash mismatch")]
fn changing_cross_chain_amount_is_rejected() {
    let mut contract = setup();
    context("agent.near", 5);
    let mut changed = request("zcash-router.near");
    changed.args = br#"{\"payment_id\":\"payment-0001\",\"destination\":\"zcash\",\"network\":\"testnet\",\"opaque_recipient_ref\":\"ref-7f2a9c8405d1b6e3f48a2d0c\",\"privacy_class\":\"shielded\",\"amount_zatoshis\":\"25001\"}"#.to_vec();
    let _promise = contract.dispatch_action(changed);
}

#[test]
#[should_panic(expected = "target mismatch")]
fn cross_chain_request_cannot_redirect_to_unapproved_router() {
    let mut contract = setup();
    context("agent.near", 5);
    let _promise = contract.dispatch_action(request("attacker.near"));
}

#[test]
#[should_panic(expected = "action is not permitted")]
fn transparent_action_is_not_available_when_only_shielded_rule_exists() {
    let mut contract = setup();
    context("agent.near", 5);
    let mut transparent = request("zcash-router.near");
    transparent.action_id = "pay_zcash_transparent".to_string();
    let _promise = contract.dispatch_action(transparent);
}
