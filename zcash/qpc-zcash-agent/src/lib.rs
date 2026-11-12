//! QPC's fail-closed Zcash adapter boundary.
//!
//! Implemented today: Unified Address validation, privacy policy, intent
//! lifecycle tracking, and a wallet observation interface.
//! Not implemented: wallet synchronization, key custody, shielded transaction
//! construction, signing, broadcasting, cross-chain routing, or settlement.

use zcash_address::unified::{self, Encoding};
use zcash_protocol::{consensus::NetworkType, PoolType};

pub mod payment;
pub mod privacy_policy;
pub mod wallet;

pub use payment::{
    IntentError, PaymentId, PaymentIntent, PaymentReceipt, PaymentState, PaymentTracker,
    SourceChain,
};
pub use privacy_policy::{PaymentMemo, PrivacyPolicy, PrivacyPolicyError, PrivateRecipient};
pub use wallet::{WalletBackend, WalletCapabilities, WalletError, WalletObservation};

/// The network an agent payment is intended to settle on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZcashNetwork {
    Mainnet,
    Testnet,
}

impl ZcashNetwork {
    fn protocol_network(self) -> NetworkType {
        match self {
            Self::Mainnet => NetworkType::Main,
            Self::Testnet => NetworkType::Test,
        }
    }
}

/// Shielded receiver policy. Orchard is preferred when available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShieldedReceiver {
    Orchard,
    Sapling,
}

/// Validated address metadata. The raw address is deliberately not retained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedRecipient {
    pub network: ZcashNetwork,
    pub receiver: ShieldedReceiver,
}

/// Address validation failures. No error path permits transparent fallback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressPolicyError {
    InvalidUnifiedAddress,
    WrongNetwork,
    NoSupportedShieldedReceiver,
}

impl core::fmt::Display for AddressPolicyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let message = match self {
            Self::InvalidUnifiedAddress => "recipient is not a valid Zcash Unified Address",
            Self::WrongNetwork => "recipient Unified Address is for a different network",
            Self::NoSupportedShieldedReceiver => {
                "recipient Unified Address has no supported shielded receiver"
            }
        };
        f.write_str(message)
    }
}

impl std::error::Error for AddressPolicyError {}

/// Validate a Unified Address for the requested network.
///
/// Orchard is selected before Sapling. Transparent-only addresses and
/// non-Unified address formats are rejected; there is no transparent fallback.
pub fn validate_recipient(
    encoded: &str,
    expected_network: ZcashNetwork,
) -> Result<ValidatedRecipient, AddressPolicyError> {
    let (network, address) = unified::Address::decode(encoded)
        .map_err(|_| AddressPolicyError::InvalidUnifiedAddress)?;

    if network != expected_network.protocol_network() {
        return Err(AddressPolicyError::WrongNetwork);
    }

    let receiver = if address.has_receiver_of_type(PoolType::ORCHARD) {
        ShieldedReceiver::Orchard
    } else if address.has_receiver_of_type(PoolType::SAPLING) {
        ShieldedReceiver::Sapling
    } else {
        return Err(AddressPolicyError::NoSupportedShieldedReceiver);
    };

    Ok(ValidatedRecipient {
        network: expected_network,
        receiver,
    })
}
