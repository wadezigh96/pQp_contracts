use qpc_zcash_agent::{
    IntentError, PaymentId, PaymentIntent, PaymentState, PaymentTracker, SourceChain,
};

fn intent() -> PaymentIntent {
    PaymentIntent {
        id: PaymentId::new([7; 32]),
        source_chain: SourceChain::Near,
        amount_zatoshis: 25_000,
        nonce: 1,
        expires_at_unix: 2_000,
    }
}

#[test]
fn rejects_zero_amount_and_expired_intents() {
    let mut zero = intent();
    zero.amount_zatoshis = 0;
    assert_eq!(zero.validate(1_000), Err(IntentError::ZeroAmount));
    assert_eq!(intent().validate(2_000), Err(IntentError::Expired));
}

#[test]
fn rejects_duplicate_payment_ids() {
    let mut tracker = PaymentTracker::default();
    let payment = intent();
    tracker.register(&payment, 1_000).unwrap();
    assert_eq!(
        tracker.register(&payment, 1_000),
        Err(IntentError::DuplicatePaymentId)
    );
}

#[test]
fn enforces_ordered_lifecycle_and_idempotent_duplicate_callback() {
    let mut tracker = PaymentTracker::default();
    let payment = intent();
    tracker.register(&payment, 1_000).unwrap();

    assert_eq!(
        tracker.transition(payment.id, PaymentState::PolicyApproved),
        Ok(PaymentState::PolicyApproved)
    );
    // Replayed callback does not change state or fail.
    assert_eq!(
        tracker.transition(payment.id, PaymentState::PolicyApproved),
        Ok(PaymentState::PolicyApproved)
    );
    // Cannot skip route quote and claim a destination was validated.
    assert_eq!(
        tracker.transition(payment.id, PaymentState::DestinationValidated),
        Err(IntentError::InvalidTransition)
    );
}

#[test]
fn completed_payment_is_terminal() {
    assert!(PaymentState::Completed.is_terminal());
    assert!(!PaymentState::Completed.can_transition_to(PaymentState::Failed));
}
