use qpc_zcash_agent::{
    PaymentId, PaymentIntent, PaymentMemo, PaymentState, PaymentTracker, PrivateRecipient,
    PrivacyPolicy, SourceChain, ZcashNetwork, ShieldedReceiver,
};

const MAINNET_UA: &str = "u1nztelxna9h7w0vtpd2xjhxt4lpu8s9cmdl8n8vcr7actf2ny45nd07cy8cyuhuvw3axcp545y0ktq9cezuzx84jyhex8dk4tdvwhu4dl";

#[test]
fn near_to_zcash_requires_shielded_private_recipient_and_redacts_address() {
    let recipient = PrivateRecipient::parse(
        MAINNET_UA.to_string(),
        ZcashNetwork::Mainnet,
        PrivacyPolicy::default(),
    )
    .expect("published Unified Address should validate");
    assert!(matches!(
        recipient.validated().receiver,
        ShieldedReceiver::Orchard | ShieldedReceiver::Sapling
    ));
    let debug = format!("{recipient:?}");
    assert!(!debug.contains(MAINNET_UA));
    assert!(debug.contains("[REDACTED]"));
    assert_eq!(PaymentMemo::empty().as_bytes(), b"");
}

#[test]
fn shielded_receipt_cannot_complete_before_confirmation_and_verification() {
    let intent = PaymentIntent {
        id: PaymentId::new([0x42; 32]),
        source_chain: SourceChain::Near,
        source_agent_id: [0x11; 32],
        amount_zatoshis: 25_000,
        nonce: 9,
        expires_at_unix: 2_000,
    };
    let mut tracker = PaymentTracker::default();
    tracker.register(&intent, 1_000).unwrap();

    for state in [
        PaymentState::PolicyApproved,
        PaymentState::RouteQuoted,
        PaymentState::DestinationValidated,
        PaymentState::SourceSettled,
        PaymentState::ZcashBroadcast,
        PaymentState::ZcashConfirmed,
        PaymentState::ReceiptVerified,
        PaymentState::Completed,
    ] {
        assert_eq!(tracker.transition(intent.id, state), Ok(state));
    }
    assert_eq!(tracker.state(intent.id), Some(PaymentState::Completed));
}

#[test]
fn ambiguous_zcash_sync_or_reorg_remains_reconcilable_not_completed() {
    let intent = PaymentIntent {
        id: PaymentId::new([0x43; 32]),
        source_chain: SourceChain::Near,
        source_agent_id: [0x12; 32],
        amount_zatoshis: 25_000,
        nonce: 10,
        expires_at_unix: 2_000,
    };
    let mut tracker = PaymentTracker::default();
    tracker.register(&intent, 1_000).unwrap();
    for state in [
        PaymentState::PolicyApproved,
        PaymentState::RouteQuoted,
        PaymentState::DestinationValidated,
        PaymentState::SourceSettled,
        PaymentState::ZcashBroadcast,
        PaymentState::PendingReconciliation,
    ] {
        tracker.transition(intent.id, state).unwrap();
    }
    assert_eq!(
        tracker.state(intent.id),
        Some(PaymentState::PendingReconciliation)
    );
    assert_ne!(tracker.state(intent.id), Some(PaymentState::Completed));
}
