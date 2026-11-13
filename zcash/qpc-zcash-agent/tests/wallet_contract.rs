use qpc_zcash_agent::{
    PaymentId, WalletBackend, WalletCapabilities, WalletError, WalletObservation,
};

struct ReadOnlyWallet;

impl WalletBackend for ReadOnlyWallet {
    fn capabilities(&self) -> Result<WalletCapabilities, WalletError> {
        Ok(WalletCapabilities {
            orchard: true,
            sapling: true,
        })
    }

    fn observe_payment(&self, _payment_id: PaymentId) -> Result<WalletObservation, WalletError> {
        Ok(WalletObservation::Unknown)
    }
}

#[test]
fn wallet_boundary_reports_capabilities_without_exposing_keys_or_broadcast_api() {
    let wallet = ReadOnlyWallet;
    let capabilities = wallet.capabilities().unwrap();
    assert!(capabilities.orchard);
    assert!(capabilities.sapling);
    assert_eq!(
        wallet.observe_payment(PaymentId::new([0; 32])).unwrap(),
        WalletObservation::Unknown
    );
}
