use near_sdk::test_utils::VMContextBuilder;
use near_sdk::{env, testing_env, NearToken};
use qpc_near::{ActionRequest, ActionRule, ExecutionStatus, PolicyInput, QpcNear};

const NOW: u64 = 1_000_000_000_000;

// The NEAR-side test deliberately carries only an opaque destination reference.
// The raw Zcash Unified Address must remain inside the off-chain wallet adapter.
const OPAQUE_ZCASH_REQUEST: &[u8] =
    br#"{"payment_id":"payment-0001","destination":"zcash","opaque_recipient_ref":"ref-7f2a","privacy_class":"shielded","amount":"25000"}"#;

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
    let args_hash = env::sha256(OPAQUE_ZCASH_REQUEST);
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
                destination_chain_id: "zcash:testnet".to_string(),
                derivation_path: "qpc/zcash-testnet/shielded".to_string(),
                args_hash,
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
        destination_chain_id: "zcash:testnet".to_string(),
        derivation_path: "qpc/zcash-testnet/shielded".to_string(),
        args: OPAQUE_ZCASH_REQUEST.to_vec(),
        attached_deposit: 5,
    }
}

#[test]
fn near_policy_dispatches_only_the_approved_opaque_zcash_request() {
    let mut contract = setup();
    context("agent.near", 5);

    let _promise = contract.dispatch_action(request("zcash-router.near"));

    let execution = contract
        .get_execution("near_to_zcash".to_string(), "near-zcash-req-1".to_string())
        .expect("dispatch record");
    assert_eq!(execution.status, ExecutionStatus::Dispatched);
    assert_eq!(execution.target.as_str(), "zcash-router.near");
    assert_eq!(execution.nonce, 0);
    assert_eq!(execution.attached_deposit, 5);
    assert_eq!(execution.destination_chain_id, "zcash:testnet");
    assert_eq!(execution.derivation_path, "qpc/zcash-testnet/shielded");
    assert_eq!(
        contract.get_usage("near_to_zcash".to_string()).next_nonce,
        1
    );
    assert!(!OPAQUE_ZCASH_REQUEST.windows(2).any(|w| w == b"u1"));
}

#[test]
#[should_panic(expected = "target mismatch")]
fn near_policy_rejects_a_different_zcash_route_target() {
    let mut contract = setup();
    context("agent.near", 5);
    let _promise = contract.dispatch_action(request("attacker.near"));
}

#[test]
#[should_panic(expected = "arguments hash mismatch")]
fn near_policy_rejects_a_request_that_changes_the_private_recipient_reference() {
    let mut contract = setup();
    context("agent.near", 5);
    let mut changed = request("zcash-router.near");
    changed.args = br#"{"payment_id":"payment-0001","destination":"zcash","opaque_recipient_ref":"attacker","privacy_class":"shielded","amount":"25000"}"#.to_vec();
    let _promise = contract.dispatch_action(changed);
}
