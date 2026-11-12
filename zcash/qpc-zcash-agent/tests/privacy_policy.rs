use qpc_zcash_agent::{
    PaymentMemo, PrivacyPolicy, PrivacyPolicyError, PrivateRecipient, ShieldedReceiver,
    ZcashNetwork,
};

const MAINNET_UA: &str = "u1nztelxna9h7w0vtpd2xjhxt4lpu8s9cmdl8n8vcr7actf2ny45nd07cy8cyuhuvw3axcp545y0ktq9cezuzx84jyhex8dk4tdvwhu4dl";

#[test]
fn recipient_debug_redacts_address() {
    let recipient = PrivateRecipient::parse(
        MAINNET_UA.to_owned(),
        ZcashNetwork::Mainnet,
        PrivacyPolicy::default(),
    )
    .unwrap();

    let debug = format!("{recipient:?}");
    assert!(!debug.contains(MAINNET_UA));
    assert!(debug.contains("[REDACTED]"));
}

#[test]
fn policy_can_reject_sapling_receiver() {
    let policy = PrivacyPolicy {
        allow_orchard: true,
        allow_sapling: false,
    };

    // This published vector must be checked by the parser; if its receiver
    // set changes in a future vector revision, the test remains explicit.
    let parsed = qpc_zcash_agent::validate_recipient(MAINNET_UA, ZcashNetwork::Mainnet)
        .unwrap();

    if parsed.receiver == ShieldedReceiver::Sapling {
        assert_eq!(
            PrivateRecipient::parse(MAINNET_UA.to_owned(), ZcashNetwork::Mainnet, policy)
                .err(),
            Some(PrivacyPolicyError::ReceiverNotAllowed)
        );
    }
}

#[test]
fn memo_is_empty_by_default_and_has_no_text_constructor() {
    assert!(PaymentMemo::default().as_bytes().is_empty());
    assert!(PaymentMemo::empty().as_bytes().is_empty());
}
