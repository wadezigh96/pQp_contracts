use super::*;
use near_sdk::{test_utils::VMContextBuilder, testing_env};

const NOW: u64 = 1_800_000_000;
fn account(value: &str) -> AccountId {
    value.parse().unwrap()
}
fn context(caller: &str, now: u64) {
    testing_env!(VMContextBuilder::new()
        .current_account_id(account("qpc.testnet"))
        .predecessor_account_id(account(caller))
        .block_timestamp(now * 1_000_000_000)
        .build());
}
fn fixture() -> Value {
    near_sdk::serde_json::from_str(include_str!("../../../docs/fixtures/cross-l1-v1.json")).unwrap()
}
fn setup() -> (QPCNear, Value) {
    context("owner.testnet", NOW);
    let mut contract = QPCNear::new(account("owner.testnet"));
    contract.configure_agent(
        account("agent.testnet"),
        "policy-1".into(),
        account("router.testnet"),
        NOW + 10_000,
        U128(5_000),
        U128(10_000),
    );
    let mut envelope = fixture()["envelope"].clone();
    envelope["policy"]["version"] = json!(1);
    envelope["nonce"] = json!("0");
    envelope["amount"]["destinationAtomic"] = json!("1000");
    contract.configure_envelope_profile(account("agent.testnet"), envelope.clone());
    contract.set_recipient_permission(account("agent.testnet"), "a".repeat(64), true);
    context("agent.testnet", NOW);
    (contract, envelope)
}
fn authorize(contract: &mut QPCNear, envelope: Value) -> Value {
    let hash = envelope_commitment(&envelope);
    contract.authorize_envelope(envelope, hash)
}
fn rejects(f: impl FnOnce()) {
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err());
}
#[test]
fn shared_js_rust_bytes_and_hash_match() {
    context("owner.testnet", NOW);
    let vector = fixture();
    validate_envelope(&vector["envelope"]);
    assert_eq!(
        canonical_json(&vector["envelope"]),
        string(&vector["canonicalJson"])
    );
    assert_eq!(
        envelope_commitment(&vector["envelope"]),
        string(&vector["authorizationCommitment"])
    );
    assert_eq!(
        compute_policy_args_hash(
            "pay-2026-0001",
            "ref-7f2a9c8405d1b6e3f48a2d0c",
            "25000",
            1_800_000_300
        ),
        "afba8520c1b56d0635f02f6d02df0e0f3fa70dc87c170822be11230db55d04a5"
    );
}
#[test]
fn actual_public_authorization_entrypoint_consumes_durable_state_only() {
    let (mut contract, envelope) = setup();
    let receipt = authorize(&mut contract, envelope.clone());
    assert_eq!(receipt["state"], "AUTHORIZED");
    assert_eq!(
        receipt["authorizationCommitment"].as_str().unwrap().len(),
        64
    );
    let policy = contract.get_agent_policy(account("agent.testnet")).unwrap();
    assert_eq!(policy.spent, "1000");
    assert_eq!(policy.next_nonce, 1);
    // Recreate the contract from Borsh, preserving the host's persisted maps.
    let bytes = borsh::to_vec(&contract).unwrap();
    let mut reloaded = QPCNear::try_from_slice(&bytes).unwrap();
    rejects(|| {
        authorize(&mut reloaded, envelope);
    });
    assert_eq!(
        reloaded
            .get_agent_policy(account("agent.testnet"))
            .unwrap()
            .next_nonce,
        1
    );
}
fn leaves(value: &Value, path: Vec<String>, out: &mut Vec<Vec<String>>) {
    if let Some(object) = value.as_object() {
        for (key, value) in object {
            let mut p = path.clone();
            p.push(key.clone());
            leaves(value, p, out);
        }
    } else {
        out.push(path);
    }
}
fn field_mut<'a>(value: &'a mut Value, path: &[String]) -> &'a mut Value {
    let mut result = value;
    for key in path {
        result = &mut result[key];
    }
    result
}
#[test]
fn every_leaf_mutation_rejected_by_actual_entrypoint_without_state_consumption() {
    let (mut contract, envelope) = setup();
    let hash = envelope_commitment(&envelope);
    let mut paths = vec![];
    leaves(&envelope, vec![], &mut paths);
    for path in paths {
        let mut mutated = envelope.clone();
        let field = field_mut(&mut mutated, &path);
        *field = if field.is_string() {
            json!(format!("{}x", field.as_str().unwrap()))
        } else {
            json!(field.as_u64().unwrap() + 1)
        };
        rejects(|| {
            contract.authorize_envelope(mutated, hash.clone());
        });
        assert_eq!(
            contract
                .get_agent_policy(account("agent.testnet"))
                .unwrap()
                .next_nonce,
            0,
            "{}",
            path.join(".")
        );
    }
    authorize(&mut contract, envelope);
}
#[test]
fn recomputed_hash_does_not_self_approve_route_policy_or_amount() {
    let (mut contract, envelope) = setup();
    for (section, key, replacement) in [
        ("source", "networkId", json!("near:mainnet")),
        ("source", "nativeChainId", json!("mainnet")),
        ("destination", "networkId", json!("zcash:mainnet")),
        ("destination", "nativeChainId", json!("mainnet")),
        ("destination", "adapterId", json!("evil.testnet")),
        ("action", "operation", json!("evil")),
        ("asset", "mappingId", json!("evil")),
        ("asset", "sourceDecimals", json!(8)),
        ("amount", "sourceAtomic", json!("1")),
        ("amount", "destinationAtomic", json!("1")),
        ("fees", "maxFeeAtomic", json!("1001")),
        ("policy", "commitment", json!("c".repeat(64))),
        ("settlement", "evidenceClass", json!("rpc-trust")),
        ("cancellation", "revocationEpoch", json!(4)),
        ("replayDomain", "scope", json!("evil")),
    ] {
        let mut mutated = envelope.clone();
        mutated[section][key] = replacement;
        rejects(|| {
            authorize(&mut contract, mutated);
        });
    }
    authorize(&mut contract, envelope);
}
#[test]
fn strict_malformed_unknown_nonascii_and_overflow_rejected() {
    let (mut contract, envelope) = setup();
    for (section, key, replacement) in [
        ("source", "unexpected", json!("x")),
        ("source", "nativeChainId", json!("téstnet")),
        ("amount", "sourceAtomic", json!("01")),
        ("amount", "destinationAtomic", json!("0")),
        (
            "amount",
            "sourceAtomic",
            json!("340282366920938463463374607431768211456"),
        ),
        ("fees", "maxSlippageBps", json!(10001)),
        ("asset", "sourceDecimals", json!(256)),
        ("policy", "version", json!(9007199254740992_u64)),
        ("policy", "version", json!(1.0)),
        ("destination", "recipientCommitment", json!("A".repeat(64))),
    ] {
        let mut mutated = envelope.clone();
        mutated[section][key] = replacement;
        rejects(|| {
            contract.authorize_envelope(mutated, "0".repeat(64));
        });
    }
    let mut missing = envelope.clone();
    missing["source"]
        .as_object_mut()
        .unwrap()
        .remove("nativeChainId");
    rejects(|| {
        contract.authorize_envelope(missing, "0".repeat(64));
    });
}
#[test]
fn native_predecessor_not_signer_metadata_authenticates() {
    let (mut contract, envelope) = setup();
    context("attacker.testnet", NOW);
    rejects(|| {
        authorize(&mut contract, envelope.clone());
    });
    context("owner.testnet", NOW);
    rejects(|| {
        contract.configure_envelope_profile(account("attacker.testnet"), envelope);
    });
}
#[test]
fn recipient_revocation_and_time_checks_enforced() {
    let (mut contract, envelope) = setup();
    let mut changed = envelope.clone();
    changed["destination"]["recipientCommitment"] = json!("c".repeat(64));
    rejects(|| {
        authorize(&mut contract, changed);
    });
    for (key, value) in [
        ("issuedAtUnix", NOW + 1),
        ("notBeforeUnix", NOW + 1),
        ("expiresAtUnix", NOW),
        ("expiresAtUnix", NOW + 10_001),
    ] {
        let mut changed = envelope.clone();
        changed["time"][key] = json!(value);
        rejects(|| {
            authorize(&mut contract, changed);
        });
    }
    context("owner.testnet", NOW);
    contract.revoke_agent(account("agent.testnet"));
    context("agent.testnet", NOW);
    rejects(|| {
        authorize(&mut contract, envelope);
    });
}
#[test]
fn nonce_intent_caps_and_reconfiguration_cannot_reset_replay() {
    let (mut contract, mut envelope) = setup();
    authorize(&mut contract, envelope.clone());
    rejects(|| {
        authorize(&mut contract, envelope.clone());
    });
    envelope["nonce"] = json!("1");
    rejects(|| {
        authorize(&mut contract, envelope.clone());
    }); // same intent, new nonce
    for nonce in 1..10 {
        envelope["nonce"] = json!(nonce.to_string());
        envelope["intentId"] = json!(format!("intent-{nonce}"));
        authorize(&mut contract, envelope.clone());
    }
    envelope["nonce"] = json!("10");
    envelope["intentId"] = json!("over-cap");
    rejects(|| {
        authorize(&mut contract, envelope.clone());
    });
    context("owner.testnet", NOW);
    contract.revoke_agent(account("agent.testnet"));
    contract.configure_agent(
        account("agent.testnet"),
        "policy-1".into(),
        account("router.testnet"),
        NOW + 10_000,
        U128(5000),
        U128(10000),
    );
    let policy = contract.get_agent_policy(account("agent.testnet")).unwrap();
    assert_eq!(policy.next_nonce, 10);
    assert_eq!(policy.policy_version, 2);
    context("agent.testnet", NOW);
    rejects(|| {
        authorize(&mut contract, envelope.clone());
    });
    envelope["policy"]["version"] = json!(2);
    rejects(|| {
        authorize(&mut contract, envelope);
    }); // explicit new owner profile needed
}
#[test]
fn per_payment_limit_expired_policy_and_nonce_overflow_fail_without_consumption() {
    let (mut contract, mut envelope) = setup();
    context("owner.testnet", NOW);
    envelope["amount"]["destinationAtomic"] = json!("5001");
    contract.configure_envelope_profile(account("agent.testnet"), envelope.clone());
    context("agent.testnet", NOW);
    rejects(|| {
        authorize(&mut contract, envelope.clone());
    });
    assert_eq!(
        contract
            .get_agent_policy(account("agent.testnet"))
            .unwrap()
            .spent,
        "0"
    );
    context("agent.testnet", NOW + 10_000);
    rejects(|| {
        authorize(&mut contract, envelope.clone());
    });
    context("owner.testnet", NOW);
    envelope["amount"]["destinationAtomic"] = json!("1000");
    contract.configure_envelope_profile(account("agent.testnet"), envelope.clone());
    let mut policy = contract.agents.get(&account("agent.testnet")).unwrap();
    policy.next_nonce = u64::MAX;
    contract.agents.insert(&account("agent.testnet"), &policy);
    envelope["nonce"] = json!(u64::MAX.to_string());
    context("agent.testnet", NOW);
    rejects(|| {
        authorize(&mut contract, envelope);
    });
    let policy = contract.get_agent_policy(account("agent.testnet")).unwrap();
    assert_eq!(policy.next_nonce, u64::MAX);
    assert_eq!(policy.spent, "0");
}

#[test]
fn owner_only_configuration_and_recipient_changes() {
    let (mut contract, envelope) = setup();
    rejects(|| {
        contract.configure_envelope_profile(account("agent.testnet"), envelope);
    });
    rejects(|| {
        contract.set_recipient_permission(account("agent.testnet"), "a".repeat(64), true);
    });
    rejects(|| {
        contract.revoke_agent(account("agent.testnet"));
    });
}
#[test]
#[should_panic(expected = "legacy dispatch disabled")]
fn legacy_entrypoint_is_not_an_authorization_bypass() {
    let (mut contract, _) = setup();
    contract.dispatch_action(
        "request".into(),
        "policy-1".into(),
        1,
        0,
        "pay_zcash_shielded".into(),
        "zcash:testnet".into(),
        account("router.testnet"),
        "pay".into(),
        "1000".into(),
        "recipient".into(),
        NOW + 100,
        "SHIELDED_ONLY".into(),
        "0".repeat(64),
    );
}
