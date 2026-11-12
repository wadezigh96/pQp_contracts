pub mod consensus;

//! QPC off-chain NEAR execution-proof reference verifier.
//!
//! Verifies an execution outcome proof against a caller-supplied trusted block
//! Merkle root. It does NOT authenticate that root as a finalized NEAR head;
//! consensus light-client verification is a separate required component.
//!
//! The Merkle/hash procedure follows the NEAR light-client protocol implementation:
//! near/near-light-client, crates/protocol/src/lib.rs and merkle_util.rs.

use near_jsonrpc_primitives::types::light_client::RpcLightClientExecutionProofResponse;
use near_primitives::merkle::{combine_hash, Direction, MerklePathItem};
use near_primitives_core::hash::CryptoHash;
use serde::Deserialize;
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedPayment {
    pub payment_id: String,
    pub token_id: String,
    pub payer: String,
    pub recipient: String,
    pub amount: u128,
    pub created_at_ns: u64,
    pub expires_at_ns: u64,
    pub executor_id: String,
    pub outcome_id: String,
    pub block_hash: String,
}

#[derive(Debug)]
pub enum VerifyError {
    Json(String),
    InvalidTrustedRoot,
    InvalidMerkleProof,
    WrongExecutor,
    FailedExecution,
    MissingQpcEvent,
    InvalidQpcEvent(String),
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(e) => write!(f, "invalid proof JSON: {e}"),
            Self::InvalidTrustedRoot => write!(f, "invalid trusted block Merkle root"),
            Self::InvalidMerkleProof => write!(f, "NEAR execution inclusion proof is invalid"),
            Self::WrongExecutor => write!(f, "execution outcome executor is not the configured QPC escrow"),
            Self::FailedExecution => write!(f, "NEAR execution outcome did not succeed"),
            Self::MissingQpcEvent => write!(f, "exactly one QPC payment_escrowed event is required"),
            Self::InvalidQpcEvent(e) => write!(f, "invalid QPC event: {e}"),
        }
    }
}
impl std::error::Error for VerifyError {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QpcEvent {
    standard: String,
    version: String,
    event: String,
    data: Vec<QpcPayment>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QpcPayment {
    payment_id: String,
    token_id: String,
    payer: String,
    recipient: String,
    amount: String,
    created_at_ns: String,
    expires_at_ns: String,
}

fn compute_root_from_path<'a>(
    path: impl Iterator<Item = &'a MerklePathItem>,
    item_hash: CryptoHash,
) -> CryptoHash {
    let mut hash = item_hash;
    for node in path {
        hash = match node.direction {
            Direction::Left => combine_hash(&node.hash, &hash),
            Direction::Right => combine_hash(&hash, &node.hash),
        };
    }
    hash
}

fn verify_execution_inclusion(
    proof: &RpcLightClientExecutionProofResponse,
    trusted_head_block_merkle_root: CryptoHash,
) -> bool {
    let block_hash = proof.block_header_lite.hash();
    if block_hash != proof.outcome_proof.block_hash {
        return false;
    }

    let outcome_hash = CryptoHash::hash_borsh(proof.outcome_proof.to_hashes());
    let shard_outcome_root = compute_root_from_path(
        proof.outcome_proof.proof.iter(),
        outcome_hash,
    );
    let outcome_leaf = CryptoHash::hash_borsh(shard_outcome_root);
    let block_outcome_root = compute_root_from_path(
        proof.outcome_root_proof.iter(),
        outcome_leaf,
    );
    let outcome_verified = block_outcome_root == proof.block_header_lite.inner_lite.outcome_root;
    let block_verified = compute_root_from_path(
        proof.block_proof.iter(),
        block_hash,
    ) == trusted_head_block_merkle_root;

    outcome_verified && block_verified
}

fn parse_qpc_escrow_event(logs: &[String]) -> Result<QpcPayment, VerifyError> {
    let mut events = Vec::new();
    for log in logs {
        let Some(payload) = log.strip_prefix("EVENT_JSON:") else { continue };
        let value: serde_json::Value = serde_json::from_str(payload)
            .map_err(|e| VerifyError::InvalidQpcEvent(e.to_string()))?;
        if value.get("standard").and_then(|v| v.as_str()) == Some("qpc") {
            events.push(serde_json::from_value::<QpcEvent>(value)
                .map_err(|e| VerifyError::InvalidQpcEvent(e.to_string()))?);
        }
    }
    if events.len() != 1 {
        return Err(VerifyError::MissingQpcEvent);
    }
    let event = events.pop().expect("length checked");
    if event.standard != "qpc" || event.version != "1.0.0" || event.event != "payment_escrowed" {
        return Err(VerifyError::InvalidQpcEvent("unexpected event identity/version".into()));
    }
    if event.data.len() != 1 {
        return Err(VerifyError::InvalidQpcEvent("event must contain exactly one payment".into()));
    }
    let payment = event.data.into_iter().next().expect("length checked");
    if payment.payment_id.is_empty() || payment.payment_id.len() > 64
        || !payment.payment_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(VerifyError::InvalidQpcEvent("invalid payment_id".into()));
    }
    if payment.token_id.is_empty() || payment.payer.is_empty() || payment.recipient.is_empty() {
        return Err(VerifyError::InvalidQpcEvent("token, payer and recipient are required".into()));
    }
    let amount = payment.amount.parse::<u128>()
        .map_err(|_| VerifyError::InvalidQpcEvent("amount must be an unsigned decimal integer".into()))?;
    let created = payment.created_at_ns.parse::<u64>()
        .map_err(|_| VerifyError::InvalidQpcEvent("created_at_ns must be an unsigned decimal integer".into()))?;
    let expires = payment.expires_at_ns.parse::<u64>()
        .map_err(|_| VerifyError::InvalidQpcEvent("expires_at_ns must be an unsigned decimal integer".into()))?;
    if amount == 0 || expires <= created {
        return Err(VerifyError::InvalidQpcEvent("amount/expiry bounds invalid".into()));
    }
    Ok(payment)
}

pub fn verify_qpc_payment_proof(
    proof_json: &str,
    trusted_head_block_merkle_root: &str,
    expected_escrow_account: &str,
) -> Result<VerifiedPayment, VerifyError> {
    let trusted_root = CryptoHash::from_str(trusted_head_block_merkle_root)
        .map_err(|_| VerifyError::InvalidTrustedRoot)?;
    let proof: RpcLightClientExecutionProofResponse =
        serde_json::from_str(proof_json).map_err(|e| VerifyError::Json(e.to_string()))?;

    let executor_id = proof.outcome_proof.outcome.executor_id.to_string();
    if executor_id != expected_escrow_account { return Err(VerifyError::WrongExecutor); }

    let outcome_value = serde_json::to_value(&proof.outcome_proof.outcome)
        .map_err(|e| VerifyError::Json(e.to_string()))?;
    let successful = outcome_value.get("status")
        .and_then(serde_json::Value::as_object)
        .map(|status| status.contains_key("SuccessValue"))
        .unwrap_or(false);
    if !successful { return Err(VerifyError::FailedExecution); }

    let outcome_id = proof.outcome_proof.id.to_string();
    let block_hash = proof.outcome_proof.block_hash.to_string();
    let payment = {
        let payment = parse_qpc_escrow_event(&proof.outcome_proof.outcome.logs)?;
        if !verify_execution_inclusion(&proof, trusted_root) {
            return Err(VerifyError::InvalidMerkleProof);
        }
        payment
    };

    Ok(VerifiedPayment {
        payment_id: payment.payment_id,
        token_id: payment.token_id,
        payer: payment.payer,
        recipient: payment.recipient,
        amount: payment.amount.parse().map_err(|e: std::num::ParseIntError|
            VerifyError::InvalidQpcEvent(e.to_string()))?,
        created_at_ns: payment.created_at_ns.parse().map_err(|e: std::num::ParseIntError|
            VerifyError::InvalidQpcEvent(e.to_string()))?,
        expires_at_ns: payment.expires_at_ns.parse().map_err(|e: std::num::ParseIntError|
            VerifyError::InvalidQpcEvent(e.to_string()))?,
        executor_id,
        outcome_id,
        block_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const ROOT: &str = "WWrLWbWHwSmjtTn5oBZPYgRCuCYn6fkYVa4yhPWNK4L";
    const FIXTURE: &str = include_str!("../fixtures/near-light-client-new.json");

    #[test]
    fn official_near_fixture_proof_is_included_in_trusted_root() {
        let root = CryptoHash::from_str(ROOT).unwrap();
        let proof: RpcLightClientExecutionProofResponse = serde_json::from_str(FIXTURE).unwrap();
        assert!(verify_execution_inclusion(&proof, root));
    }

    #[test]
    fn tampered_merkle_path_is_rejected() {
        let mut value: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
        value["block_proof"][0]["hash"] = serde_json::Value::String(
            "11111111111111111111111111111111".into());
        let root = CryptoHash::from_str(ROOT).unwrap();
        let proof: RpcLightClientExecutionProofResponse = serde_json::from_value(value).unwrap();
        assert!(!verify_execution_inclusion(&proof, root));
    }

    fn log(event: &str, amount: &str) -> String {
        format!("EVENT_JSON:{{\"standard\":\"qpc\",\"version\":\"1.0.0\",\"event\":\"{}\",\"data\":[{{\"payment_id\":\"pay-1\",\"token_id\":\"usdc.near\",\"payer\":\"agent-a.near\",\"recipient\":\"agent-b.near\",\"amount\":\"{}\",\"created_at_ns\":\"100\",\"expires_at_ns\":\"200\"}}]}}", event, amount)
    }

    #[test]
    fn parses_canonical_qpc_payment_event() {
        let payment = parse_qpc_escrow_event(&[log("payment_escrowed", "1000000")]).unwrap();
        assert_eq!(payment.payment_id, "pay-1");
        assert_eq!(payment.amount, "1000000");
    }
    #[test]
    fn rejects_non_decimal_amount() {
        assert!(parse_qpc_escrow_event(&[log("payment_escrowed", "1e9")]).is_err());
    }
    #[test]
    fn rejects_duplicate_qpc_events() {
        assert!(parse_qpc_escrow_event(&[
            log("payment_escrowed", "10"), log("payment_escrowed", "10")
        ]).is_err());
    }
    #[test]
    fn rejects_unknown_event_fields() {
        let payload = r#"{"standard":"qpc","version":"1.0.0","event":"payment_escrowed","extra":true,"data":[]}"#;
        assert!(serde_json::from_str::<QpcEvent>(payload).is_err());
    }
}
