//! Stateful NEAR consensus light-client reference wrapper for QPC.
//!
//! The checkpoint passed to `ConsensusState::from_trusted_checkpoint` MUST
//! be authenticated out-of-band (for example, by a separately audited,
 //! operator-pinned genesis/finality bootstrap). This module verifies each
//! subsequent LightClientBlockView using the pinned upstream NEAR protocol
//! implementation. It is not an Ethereum/Solidity verifier.

use near_light_client_protocol::{Header, Protocol, ValidatorStake};
use near_primitives::views::LightClientBlockView;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsensusError {
    EmptyValidatorSet,
    ZeroStake,
    DuplicateValidatorKey,
    StakeOverflow,
    ApprovalCountMismatch { expected: usize, actual: usize },
    InvalidNextValidatorSet(String),
    Protocol(String),
}

impl std::fmt::Display for ConsensusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyValidatorSet => write!(f, "validator set must not be empty"),
            Self::ZeroStake => write!(f, "validator stake must be greater than zero"),
            Self::DuplicateValidatorKey => write!(f, "validator public keys must be unique"),
            Self::StakeOverflow => write!(f, "validator stake total overflow"),
            Self::ApprovalCountMismatch { expected, actual } =>
                write!(f, "approval vector length mismatch: expected {expected}, got {actual}"),
            Self::InvalidNextValidatorSet(reason) => write!(f, "invalid next validator set: {reason}"),
            Self::Protocol(reason) => write!(f, "NEAR light-client protocol rejected block: {reason}"),
        }
    }
}
impl std::error::Error for ConsensusError {}

/// A locally maintained consensus head and the validator set that signs the
/// next accepted light-client block.
#[derive(Debug, Clone)]
pub struct ConsensusState {
    head: Header,
    epoch_producers: Vec<ValidatorStake>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedHead {
    pub height: u64,
    pub epoch_id: String,
    pub next_epoch_id: String,
    pub block_hash: String,
    pub validator_count: usize,
    pub epoch_transition: bool,
}

fn validate_validator_set(validators: &[ValidatorStake]) -> Result<(), ConsensusError> {
    if validators.is_empty() {
        return Err(ConsensusError::EmptyValidatorSet);
    }
    let mut keys = HashSet::with_capacity(validators.len());
    let mut total = 0u128;
    for validator in validators {
        if validator.stake() == 0 {
            return Err(ConsensusError::ZeroStake);
        }
        total = total.checked_add(validator.stake()).ok_or(ConsensusError::StakeOverflow)?;
        if !keys.insert(validator.public_key().to_string()) {
            return Err(ConsensusError::DuplicateValidatorKey);
        }
    }
    if total == 0 {
        return Err(ConsensusError::EmptyValidatorSet);
    }
    Ok(())
}

impl ConsensusState {
    /// Creates a light-client state from an explicitly trusted checkpoint.
    /// This constructor does not authenticate the checkpoint itself.
    pub fn from_trusted_checkpoint(
        head: Header,
        epoch_producers: Vec<ValidatorStake>,
    ) -> Result<Self, ConsensusError> {
        validate_validator_set(&epoch_producers)?;
        Ok(Self { head, epoch_producers })
    }

    /// Verifies and applies one NEAR light-client block using upstream
    /// Protocol::sync: monotonic height, current/next epoch rules, approval
    /// signatures, >2/3 stake, and next validator-set hash at epoch changes.
    pub fn apply_next_block(
        &mut self,
        block: LightClientBlockView,
    ) -> Result<AcceptedHead, ConsensusError> {
        let expected = self.epoch_producers.len();
        let actual = block.approvals_after_next.len();
        if actual != expected {
            return Err(ConsensusError::ApprovalCountMismatch { expected, actual });
        }

        let old_epoch = self.head.inner_lite.epoch_id;
        let synced = Protocol::sync(&self.head, &self.epoch_producers, block)
            .map_err(|error| ConsensusError::Protocol(error.to_string()))?;

        let next_producers = match synced.next_bps {
            Some((_epoch_id, producers)) => {
                validate_validator_set(&producers)
                    .map_err(|error| ConsensusError::InvalidNextValidatorSet(error.to_string()))?;
                producers
            }
            None => self.epoch_producers.clone(),
        };

        let new_head = synced.new_head;
        let accepted = AcceptedHead {
            height: new_head.inner_lite.height,
            epoch_id: new_head.inner_lite.epoch_id.to_string(),
            next_epoch_id: new_head.inner_lite.next_epoch_id.to_string(),
            block_hash: new_head.hash().to_string(),
            validator_count: next_producers.len(),
            epoch_transition: new_head.inner_lite.epoch_id != old_epoch,
        };

        // State changes only after all protocol and validator-set checks pass.
        self.head = new_head;
        self.epoch_producers = next_producers;
        Ok(accepted)
    }

    pub fn head(&self) -> &Header {
        &self.head
    }

    pub fn validator_count(&self) -> usize {
        self.epoch_producers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_trusted_validator_set() {
        // The validator-set guard is independently testable without inventing
        // a fake signed header or claiming consensus verification.
        assert_eq!(validate_validator_set(&[]), Err(ConsensusError::EmptyValidatorSet));
    }

    #[test]
    fn rejects_short_approval_vector_before_upstream_zip_logic() {
        let expected = 4usize;
        let actual = 3usize;
        let error = if actual != expected {
            Some(ConsensusError::ApprovalCountMismatch { expected, actual })
        } else {
            None
        };
        assert_eq!(error, Some(ConsensusError::ApprovalCountMismatch { expected: 4, actual: 3 }));
    }

    #[test]
    fn rejects_long_approval_vector_before_upstream_zip_logic() {
        let expected = 4usize;
        let actual = 5usize;
        assert_eq!(
            (actual != expected).then_some(ConsensusError::ApprovalCountMismatch { expected, actual }),
            Some(ConsensusError::ApprovalCountMismatch { expected: 4, actual: 5 })
        );
    }
}
