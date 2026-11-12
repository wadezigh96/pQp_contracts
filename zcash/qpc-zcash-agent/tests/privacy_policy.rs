use qpc_zcash_agent::{
    PaymentMemo, PrivacyPolicy, PrivacyPolicyError, PrivateRecipient, ZcashNetwork,
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
fn rejects_recipient_when_all_shielded_receivers_are_disabled() {
    let policy = PrivacyPolicy {
        allow_orchard: false,
        allow_sapling: false,
    };

    assert_eq!(
        PrivateRecipient::parse(MAINNET_UA.to_owned(), ZcashNetwork::Mainnet, policy).err(),
        Some(PrivacyPolicyError::ReceiverNotAllowed)
    );
}

#[test]
fn memo_is_empty_by_default_and_has_no_text_constructor() {
    assert!(PaymentMemo::default().as_bytes().is_empty());
    assert!(PaymentMemo::empty().as_bytes().is_empty());
}
