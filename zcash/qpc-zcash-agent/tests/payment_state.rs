use qpc_zcash_agent::{
    IntentError, PaymentId, PaymentIntent, PaymentState, PaymentTracker, SourceChain,
};

fn intent(id: u8, source_agent_id: [u8; 32], nonce: u64) -> PaymentIntent {
    PaymentIntent {
        id: PaymentId::new([id; 32]),
        source_chain: SourceChain::Near,
        source_agent_id,
        amount_zatoshis: 25_000,
        nonce,
        expires_at_unix: 2_000,
    }
}

#[test]
fn rejects_zero_amount_and_expired_intents() {
    let mut zero = intent(7, [1; 32], 1);
    zero.amount_zatoshis = 0;
    assert_eq!(zero.validate(1_000), Err(IntentError::ZeroAmount));
    assert_eq!(intent(7, [1; 32], 1).validate(2_000), Err(IntentError::Expired));
}

#[test]
fn rejects_duplicate_payment_ids() {
    let mut tracker = PaymentTracker::default();
    let payment = intent(7, [1; 32], 1);
    tracker.register(&payment, 1_000).unwrap();
    assert_eq!(tracker.register(&payment, 1_000), Err(IntentError::DuplicatePaymentId));
}

#[test]
fn rejects_replayed_nonce_for_same_source_agent() {
    let mut tracker = PaymentTracker::default();
    tracker.register(&intent(7, [1; 32], 1), 1_000).unwrap();
    assert_eq!(tracker.register(&intent(8, [1; 32], 1), 1_000), Err(IntentError::DuplicateNonce));
}

#[test]
fn same_nonce_is_allowed_for_different_source_agents() {
    let mut tracker = PaymentTracker::default();
    tracker.register(&intent(7, [1; 32], 1), 1_000).unwrap();
    assert_eq!(tracker.register(&intent(8, [2; 32], 1), 1_000), Ok(()));
}

#[test]
fn enforces_ordered_lifecycle_and_idempotent_duplicate_callback() {
    let mut tracker = PaymentTracker::default();
    let payment = intent(7, [1; 32], 1);
    tracker.register(&payment, 1_000).unwrap();
    assert_eq!(tracker.transition(payment.id, PaymentState::PolicyApproved), Ok(PaymentState::PolicyApproved));
    assert_eq!(tracker.transition(payment.id, PaymentState::PolicyApproved), Ok(PaymentState::PolicyApproved));
    assert_eq!(tracker.transition(payment.id, PaymentState::DestinationValidated), Err(IntentError::InvalidTransition));
}

#[test]
fn completed_payment_is_terminal() {
    assert!(PaymentState::Completed.is_terminal());
    assert!(!PaymentState::Completed.can_transition_to(PaymentState::Failed));
}
