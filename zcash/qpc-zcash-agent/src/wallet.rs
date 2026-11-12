//! Safe boundary for a future Zcash wallet implementation.
//!
//! This module deliberately exposes observation and capability APIs only.
//! It has no signing, key-import, transaction-construction, or broadcast API.

use crate::{PaymentId, ShieldedReceiver};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WalletCapabilities {
    pub orchard: bool,
    pub sapling: bool,
}

impl WalletCapabilities {
    pub fn supports(&self, receiver: ShieldedReceiver) -> bool {
        match receiver {
            ShieldedReceiver::Orchard => self.orchard,
            ShieldedReceiver::Sapling => self.sapling,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WalletObservation {
    Unknown,
    Pending,
    Confirmed { confirmations: u32, block_height: u32 },
    Reorged,
}

/// Errors are intentionally non-descriptive to avoid leaking wallet details.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WalletError {
    Unavailable,
    SyncIncomplete,
    InternalFailure,
}

/// Implementations must keep spending keys and viewing keys inside the wallet
/// boundary. This trait cannot sign or broadcast transactions.
pub trait WalletBackend {
    fn capabilities(&self) -> Result<WalletCapabilities, WalletError>;

    fn observe_payment(
        &self,
        payment_id: PaymentId,
    ) -> Result<WalletObservation, WalletError>;
}
