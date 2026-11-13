use qpc_zcash_agent::{
    IntentError, PaymentId, PaymentIntent, PaymentState, PaymentTracker, SourceChain,
};

#[test]
fn zcash_to_near_requires_shielded_source_confirmation_then_near_finality() {
    let intent = PaymentIntent {
        id: PaymentId::new([0x51; 32]),
        source_chain: SourceChain::Zcash,
        source_agent_id: [0x21; 32],
        amount_zatoshis: 50_000,
        nonce: 17,
        expires_at_unix: 5_000,
    };
    let mut tracker = PaymentTracker::default();
    tracker.register(&intent, 1_000).unwrap();

    for state in [
        PaymentState::PolicyApproved,
        PaymentState::RouteQuoted,
        PaymentState::DestinationValidated,
        PaymentState::SourceZcashBroadcast,
        PaymentState::SourceZcashConfirmed,
        PaymentState::DestinationBroadcast,
        PaymentState::DestinationConfirmed,
        PaymentState::ReceiptVerified,
        PaymentState::Completed,
    ] {
        assert_eq!(tracker.transition(intent.id, state), Ok(state));
    }
    assert_eq!(tracker.state(intent.id), Some(PaymentState::Completed));
}

#[test]
fn zcash_source_cannot_skip_source_confirmation_or_destination_verification() {
    let intent = PaymentIntent {
        id: PaymentId::new([0x52; 32]),
        source_chain: SourceChain::Zcash,
        source_agent_id: [0x22; 32],
        amount_zatoshis: 50_000,
        nonce: 18,
        expires_at_unix: 5_000,
    };
    let mut tracker = PaymentTracker::default();
    tracker.register(&intent, 1_000).unwrap();
    for state in [
        PaymentState::PolicyApproved,
        PaymentState::RouteQuoted,
        PaymentState::DestinationValidated,
    ] {
        tracker.transition(intent.id, state).unwrap();
    }
    assert_eq!(
        tracker.transition(intent.id, PaymentState::DestinationBroadcast),
        Err(IntentError::InvalidTransition)
    );
    tracker.transition(intent.id, PaymentState::SourceZcashBroadcast).unwrap();
    assert_eq!(
        tracker.transition(intent.id, PaymentState::DestinationBroadcast),
        Err(IntentError::InvalidTransition)
    );
}

#[test]
fn zcash_reorg_enters_reconciliation_and_cannot_be_marked_complete_early() {
    let intent = PaymentIntent {
        id: PaymentId::new([0x53; 32]),
        source_chain: SourceChain::Zcash,
        source_agent_id: [0x23; 32],
        amount_zatoshis: 50_000,
        nonce: 19,
        expires_at_unix: 5_000,
    };
    let mut tracker = PaymentTracker::default();
    tracker.register(&intent, 1_000).unwrap();
    for state in [
        PaymentState::PolicyApproved,
        PaymentState::RouteQuoted,
        PaymentState::DestinationValidated,
        PaymentState::SourceZcashBroadcast,
        PaymentState::PendingReconciliation,
    ] {
        tracker.transition(intent.id, state).unwrap();
    }
    assert_eq!(
        tracker.transition(intent.id, PaymentState::Completed),
        Err(IntentError::InvalidTransition)
    );
    tracker.transition(intent.id, PaymentState::SourceZcashConfirmed).unwrap();
    assert_eq!(
        tracker.state(intent.id),
        Some(PaymentState::SourceZcashConfirmed)
    );
}
