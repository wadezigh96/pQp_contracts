//! Payment intent and in-memory lifecycle tracking.
//!
//! Amounts are integer zatoshi (1 ZEC = 100,000,000 zatoshi). Public intent
//! structures contain no raw destination addresses, txids, or key material.
//! PaymentTracker is a test/helper model; PersistentPaymentStore provides
//! durable replay protection across restarts and concurrent writers.

use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PaymentId([u8; 32]);

impl PaymentId {
    pub const fn new(bytes: [u8; 32]) -> Self { Self(bytes) }
    pub const fn as_bytes(&self) -> &[u8; 32] { &self.0 }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SourceChain {
    Near,
    Ethereum,
    BnbSmartChain,
    Solana,
    Zcash,
}

impl SourceChain {
    pub(crate) const fn storage_key(self) -> &'static str {
        match self {
            Self::Near => "near",
            Self::Ethereum => "ethereum",
            Self::BnbSmartChain => "bnb_smart_chain",
            Self::Solana => "solana",
            Self::Zcash => "zcash",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaymentIntent {
    pub id: PaymentId,
    pub source_chain: SourceChain,
    /// Stable identifier for the paying agent/wallet on this source chain.
    pub source_agent_id: [u8; 32],
    pub amount_zatoshis: u64,
    pub nonce: u64,
    pub expires_at_unix: u64,
}

impl PaymentIntent {
    pub fn validate(&self, now_unix: u64) -> Result<(), IntentError> {
        if self.amount_zatoshis == 0 { return Err(IntentError::ZeroAmount); }
        if self.expires_at_unix <= now_unix { return Err(IntentError::Expired); }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntentError {
    ZeroAmount,
    Expired,
    DuplicatePaymentId,
    DuplicateNonce,
    UnknownPaymentId,
    InvalidTransition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaymentState {
    Created,
    PolicyApproved,
    RouteQuoted,
    DestinationValidated,
    /// Source-side settlement finalized on NEAR/EVM/Solana.
    SourceSettled,
    /// Zcash-originating route: shielded source transaction lifecycle.
    SourceZcashBroadcast,
    SourceZcashConfirmed,
    /// Destination-side settlement on a non-Zcash chain.
    DestinationBroadcast,
    DestinationConfirmed,
    /// Destination-side settlement and wallet receipt on Zcash.
    ZcashBroadcast,
    ZcashConfirmed,
    ReceiptVerified,
    PendingReconciliation,
    Completed,
    Failed,
    Expired,
}

impl PaymentState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Expired)
    }

    pub const fn can_transition_to(self, next: Self) -> bool {
        use PaymentState::*;
        match self {
            Created => matches!(next, PolicyApproved | Failed | Expired),
            PolicyApproved => matches!(next, RouteQuoted | Failed | Expired),
            RouteQuoted => matches!(next, DestinationValidated | Failed | Expired),
            DestinationValidated => matches!(
                next,
                SourceSettled | SourceZcashBroadcast | Failed | Expired
            ),
            SourceSettled => matches!(
                next,
                ZcashBroadcast | DestinationBroadcast | PendingReconciliation | Failed
            ),
            SourceZcashBroadcast => matches!(
                next,
                SourceZcashConfirmed | PendingReconciliation | Failed
            ),
            SourceZcashConfirmed => matches!(
                next,
                DestinationBroadcast | PendingReconciliation | Failed
            ),
            DestinationBroadcast => matches!(
                next,
                DestinationConfirmed | PendingReconciliation | Failed
            ),
            DestinationConfirmed => matches!(
                next,
                ReceiptVerified | PendingReconciliation
            ),
            ZcashBroadcast => matches!(
                next,
                ZcashConfirmed | PendingReconciliation | Failed
            ),
            ZcashConfirmed => matches!(
                next,
                ReceiptVerified | PendingReconciliation
            ),
            ReceiptVerified => matches!(next, Completed | PendingReconciliation),
            PendingReconciliation => matches!(
                next,
                SourceZcashConfirmed
                    | DestinationBroadcast
                    | DestinationConfirmed
                    | ZcashBroadcast
                    | ZcashConfirmed
                    | ReceiptVerified
                    | Failed
                    | Expired
            ),
            Completed | Failed | Expired => false,
        }
    }
}

#[derive(Default)]
pub struct PaymentTracker {
    states: HashMap<PaymentId, PaymentState>,
    consumed_nonces: HashSet<(SourceChain, [u8; 32], u64)>,
}

impl PaymentTracker {
    pub fn register(&mut self, intent: &PaymentIntent, now_unix: u64) -> Result<(), IntentError> {
        intent.validate(now_unix)?;
        if self.states.contains_key(&intent.id) {
            return Err(IntentError::DuplicatePaymentId);
        }
        let nonce_key = (intent.source_chain, intent.source_agent_id, intent.nonce);
        if self.consumed_nonces.contains(&nonce_key) {
            return Err(IntentError::DuplicateNonce);
        }
        self.states.insert(intent.id, PaymentState::Created);
        self.consumed_nonces.insert(nonce_key);
        Ok(())
    }

    /// Repeated delivery of the current state is idempotent. Older or
    /// out-of-order callbacks cannot move the payment backwards.
    pub fn transition(
        &mut self,
        id: PaymentId,
        next: PaymentState,
    ) -> Result<PaymentState, IntentError> {
        let current = self.states.get_mut(&id).ok_or(IntentError::UnknownPaymentId)?;
        if *current == next { return Ok(*current); }
        if !current.can_transition_to(next) { return Err(IntentError::InvalidTransition); }
        *current = next;
        Ok(*current)
    }

    pub fn state(&self, id: PaymentId) -> Option<PaymentState> {
        self.states.get(&id).copied()
    }
}

/// Minimal receipt. It intentionally omits txid, address, memo, and note data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaymentReceipt {
    pub payment_id: PaymentId,
    pub state: PaymentState,
    pub confirmations: u32,
}
