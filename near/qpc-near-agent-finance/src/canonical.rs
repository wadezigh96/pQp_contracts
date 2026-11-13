//! The shared JS/Rust QPC v1 byte profile. This module does not authenticate.
use near_sdk::{env, serde_json::Value};

const DOMAIN: &str = "QPC-CROSS-L1-AUTHORIZATION-V1\0";
const MAX_SAFE: u64 = 9_007_199_254_740_991;

pub(crate) fn string(value: &Value) -> &str {
    value.as_str().expect("expected string")
}
pub(crate) fn integer(value: &Value) -> u64 {
    let value = value.as_u64().expect("expected unsigned JSON integer");
    assert!(value <= MAX_SAFE, "integer exceeds JS safe range");
    value
}
pub(crate) fn decimal(value: &Value, positive: bool) -> u128 {
    let text = string(value);
    assert!(
        !text.is_empty() && text.len() <= 39 && text.bytes().all(|b| b.is_ascii_digit()),
        "invalid decimal"
    );
    assert!(
        text == "0" || !text.starts_with('0'),
        "non-canonical decimal"
    );
    let number = text.parse::<u128>().expect("decimal exceeds u128");
    assert!(!positive || number > 0, "amount must be positive");
    number
}
fn identifier(value: &Value) {
    let text = string(value);
    assert!(
        !text.is_empty()
            && text.len() <= 256
            && text.as_bytes()[0].is_ascii_alphanumeric()
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b)),
        "invalid ASCII identifier"
    );
}
fn commitment(value: &Value) {
    let text = string(value);
    assert!(
        text.len() == 64
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid commitment"
    );
}
fn exact(value: &Value, keys: &[&str]) {
    let object = value.as_object().expect("expected object");
    assert!(
        object.len() == keys.len() && keys.iter().all(|k| object.contains_key(*k)),
        "missing or unexpected field"
    );
}

pub(crate) fn validate_envelope(e: &Value) {
    assert!(canonical_json(e).len() <= 16_384, "envelope too large");
    exact(
        e,
        &[
            "schema",
            "version",
            "intentId",
            "agentIdentity",
            "source",
            "destination",
            "action",
            "asset",
            "amount",
            "fees",
            "nonce",
            "replayDomain",
            "policy",
            "time",
            "settlement",
            "cancellation",
        ],
    );
    assert_eq!(
        string(&e["schema"]),
        "qpc.cross-l1.authorization",
        "unsupported schema"
    );
    assert_eq!(integer(&e["version"]), 1, "unsupported version");
    identifier(&e["intentId"]);
    for (section, keys) in [
        ("agentIdentity", vec!["namespace", "id", "keyId"]),
        (
            "source",
            vec!["networkId", "nativeChainId", "accountId", "adapterId"],
        ),
        (
            "destination",
            vec![
                "networkId",
                "nativeChainId",
                "adapterId",
                "recipientCommitment",
            ],
        ),
        ("action", vec!["type", "operation"]),
        (
            "asset",
            vec![
                "sourceAssetId",
                "destinationAssetId",
                "sourceDecimals",
                "destinationDecimals",
                "mappingId",
            ],
        ),
        ("amount", vec!["sourceAtomic", "destinationAtomic"]),
        ("fees", vec!["maxFeeAtomic", "feeAssetId", "maxSlippageBps"]),
        ("replayDomain", vec!["authorityId", "scope"]),
        ("policy", vec!["id", "version", "commitment"]),
        (
            "time",
            vec!["issuedAtUnix", "notBeforeUnix", "expiresAtUnix"],
        ),
        (
            "settlement",
            vec!["evidenceClass", "finalityPolicy", "minConfirmations"],
        ),
        ("cancellation", vec!["authorityId", "revocationEpoch"]),
    ] {
        exact(&e[section], &keys);
        for key in keys {
            match (section, key) {
                ("amount", _) => {
                    decimal(&e[section][key], true);
                }
                ("fees", "maxFeeAtomic") => {
                    decimal(&e[section][key], false);
                }
                ("destination", "recipientCommitment") | ("policy", "commitment") => {
                    commitment(&e[section][key])
                }
                ("asset", "sourceDecimals" | "destinationDecimals") => {
                    assert!(integer(&e[section][key]) <= 255, "decimals exceed 255")
                }
                ("fees", "maxSlippageBps") => assert!(
                    integer(&e[section][key]) <= 10_000,
                    "slippage exceeds 10000"
                ),
                ("policy", "version") => {
                    assert!(integer(&e[section][key]) >= 1, "invalid policy version")
                }
                ("time", _)
                | ("settlement", "minConfirmations")
                | ("cancellation", "revocationEpoch") => {
                    integer(&e[section][key]);
                }
                _ => identifier(&e[section][key]),
            }
        }
    }
    decimal(&e["nonce"], false);
    assert!(
        integer(&e["time"]["notBeforeUnix"]) >= integer(&e["time"]["issuedAtUnix"])
            && integer(&e["time"]["expiresAtUnix"]) > integer(&e["time"]["notBeforeUnix"]),
        "invalid time window"
    );
}

pub(crate) fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort();
            format!(
                "{{{}}}",
                keys.iter()
                    .map(|key| format!(
                        "{}:{}",
                        near_sdk::serde_json::to_string(key).unwrap(),
                        canonical_json(&object[*key])
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => near_sdk::serde_json::to_string(value).expect("JSON serialization"),
    }
}
pub(crate) fn envelope_commitment(value: &Value) -> String {
    validate_envelope(value);
    super::hex_string(&env::sha256(
        format!("{}{}", DOMAIN, canonical_json(value)).as_bytes(),
    ))
}
pub(crate) fn static_profile(value: &Value) -> Value {
    let mut profile = value.clone();
    profile["intentId"] = Value::Null;
    profile["nonce"] = Value::Null;
    profile["time"] = Value::Null;
    profile["destination"]["recipientCommitment"] = Value::Null;
    profile
}
