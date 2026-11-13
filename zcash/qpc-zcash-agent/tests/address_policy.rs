use qpc_zcash_agent::{
    validate_recipient, AddressPolicyError, ShieldedReceiver, ZcashNetwork,
};
use zcash_address::unified::Encoding;

// Published mainnet Unified Address test vector.
const MAINNET_UA: &str = "u1nztelxna9h7w0vtpd2xjhxt4lpu8s9cmdl8n8vcr7actf2ny45nd07cy8cyuhuvw3axcp545y0ktq9cezuzx84jyhex8dk4tdvwhu4dl";

#[test]
fn accepts_mainnet_unified_address_with_shielded_receiver() {
    let result = validate_recipient(MAINNET_UA, ZcashNetwork::Mainnet)
        .expect("published mainnet UA should parse");
    assert_eq!(result.network, ZcashNetwork::Mainnet);
    assert!(matches!(
        result.receiver,
        ShieldedReceiver::Orchard | ShieldedReceiver::Sapling
    ));
}

#[test]
fn rejects_address_for_wrong_network() {
    assert_eq!(
        validate_recipient(MAINNET_UA, ZcashNetwork::Testnet),
        Err(AddressPolicyError::WrongNetwork)
    );
}

#[test]
fn rejects_malformed_address() {
    assert_eq!(
        validate_recipient("not-a-zcash-address", ZcashNetwork::Mainnet),
        Err(AddressPolicyError::InvalidUnifiedAddress)
    );
}

#[test]
fn rejects_non_unified_transparent_address_without_fallback() {
    // A legacy transparent address is not accepted as a Unified Address.
    assert_eq!(
        validate_recipient(
            "t1ZcashTransparentAddressExample111111111",
            ZcashNetwork::Mainnet
        ),
        Err(AddressPolicyError::InvalidUnifiedAddress)
    );
}

#[test]
fn canonical_unified_address_round_trips_without_mutation() {
    // The canonical representation is stable under decode -> encode.
    // The production validator enforces the same equality for every input.
    let canonical = zcash_address::unified::Address::decode(MAINNET_UA)
        .expect("published vector should decode");
    let (network, address) = canonical;
    assert_eq!(address.encode(&network), MAINNET_UA);
    assert!(validate_recipient(MAINNET_UA, ZcashNetwork::Mainnet).is_ok());
}
