//! Privacy-first destination and memo policy.
//!
//! Raw recipient addresses stay in this private wrapper and are redacted from
//! Debug output. Callers should never serialize this type or publish it on a
//! source chain.

use core::fmt;

use crate::{
    validate_recipient, AddressPolicyError, ShieldedReceiver, ValidatedRecipient, ZcashNetwork,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrivacyPolicy {
    pub allow_orchard: bool,
    pub allow_sapling: bool,
}

impl Default for PrivacyPolicy {
    fn default() -> Self {
        Self {
            allow_orchard: true,
            allow_sapling: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivacyPolicyError {
    Address(AddressPolicyError),
    ReceiverNotAllowed,
}

impl From<AddressPolicyError> for PrivacyPolicyError {
    fn from(value: AddressPolicyError) -> Self {
        Self::Address(value)
    }
}

/// Holds the destination only inside the adapter boundary.
/// Debug intentionally never prints the encoded address.
pub struct PrivateRecipient {
    encoded: String,
    validated: ValidatedRecipient,
}

impl PrivateRecipient {
    pub fn parse(
        encoded: String,
        network: ZcashNetwork,
        policy: PrivacyPolicy,
    ) -> Result<Self, PrivacyPolicyError> {
        let validated = validate_recipient(&encoded, network)?;
        let allowed = match validated.receiver {
            ShieldedReceiver::Orchard => policy.allow_orchard,
            ShieldedReceiver::Sapling => policy.allow_sapling,
        };
        if !allowed {
            return Err(PrivacyPolicyError::ReceiverNotAllowed);
        }

        Ok(Self { encoded, validated })
    }

    pub fn validated(&self) -> ValidatedRecipient {
        self.validated
    }

    pub(crate) fn encoded_for_wallet(&self) -> &str {
        &self.encoded
    }
}

impl fmt::Debug for PrivateRecipient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrivateRecipient")
            .field("encoded", &"[REDACTED]")
            .field("validated", &self.validated)
            .finish()
    }
}

/// The only memo value exposed by the initial privacy policy is empty.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PaymentMemo;

impl PaymentMemo {
    pub const fn empty() -> Self {
        Self
    }

    pub const fn as_bytes(&self) -> &'static [u8] {
        &[]
    }
}
