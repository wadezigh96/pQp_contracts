//! Privacy-preserving explorer query boundary.
//!
//! ZecBlock's public v1 API documents ordinary HTTP endpoints, while the
//! private.zecblock.com announcement describes a browser-side PIR flow but does
//! not publish a stable machine-to-machine request schema here. Consequently
//! this module accepts only opaque encrypted payloads and deliberately does not
//! invent an endpoint, wire format, encryption scheme, or plaintext fallback.

use std::fmt;

pub const ZECBLOCK_PRIVATE_ORIGIN: &str = "https://private.zecblock.com";
pub const MAX_PIR_PAYLOAD_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncryptedPirQuery(Vec<u8>);

impl EncryptedPirQuery {
    /// Construct from ciphertext produced by a separately audited PIR client.
    /// This type does not encrypt plaintext; callers must not pass raw addresses,
    /// transaction IDs, block identifiers, or viewing keys.
    pub fn from_ciphertext(ciphertext: Vec<u8>) -> Result<Self, PirError> {
        if ciphertext.is_empty() || ciphertext.len() > MAX_PIR_PAYLOAD_BYTES {
            return Err(PirError::InvalidPayloadSize);
        }
        Ok(Self(ciphertext))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncryptedPirResponse(Vec<u8>);

impl EncryptedPirResponse {
    pub fn from_ciphertext(ciphertext: Vec<u8>) -> Result<Self, PirError> {
        if ciphertext.is_empty() || ciphertext.len() > MAX_PIR_PAYLOAD_BYTES {
            return Err(PirError::InvalidPayloadSize);
        }
        Ok(Self(ciphertext))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PirError {
    InvalidPayloadSize,
    TransportUnavailable,
    InvalidEncryptedResponse,
    PlaintextFallbackForbidden,
}

impl fmt::Display for PirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for PirError {}

/// A transport must implement the vendor's documented PIR wire protocol.
/// Implementations must not log payloads, add query values to URLs, or downgrade
/// to ordinary explorer endpoints. The server may still observe IP and timing.
pub trait PirTransport {
    fn exchange(
        &self,
        query: &EncryptedPirQuery,
    ) -> Result<EncryptedPirResponse, PirError>;
}

/// Read-only PIR client. There is intentionally no public-explorer fallback.
pub struct PrivateExplorerClient<T: PirTransport> {
    transport: T,
}

impl<T: PirTransport> PrivateExplorerClient<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn query(
        &self,
        encrypted_query: &EncryptedPirQuery,
    ) -> Result<EncryptedPirResponse, PirError> {
        self.transport
            .exchange(encrypted_query)
            .map_err(|_| PirError::TransportUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoCiphertextTransport;

    impl PirTransport for EchoCiphertextTransport {
        fn exchange(
            &self,
            query: &EncryptedPirQuery,
        ) -> Result<EncryptedPirResponse, PirError> {
            EncryptedPirResponse::from_ciphertext(query.as_bytes().to_vec())
        }
    }

    struct FailingTransport;

    impl PirTransport for FailingTransport {
        fn exchange(
            &self,
            _query: &EncryptedPirQuery,
        ) -> Result<EncryptedPirResponse, PirError> {
            Err(PirError::PlaintextFallbackForbidden)
        }
    }

    #[test]
    fn rejects_empty_ciphertext() {
        assert_eq!(
            EncryptedPirQuery::from_ciphertext(Vec::new()),
            Err(PirError::InvalidPayloadSize)
        );
    }

    #[test]
    fn rejects_oversized_ciphertext() {
        assert_eq!(
            EncryptedPirQuery::from_ciphertext(vec![0; MAX_PIR_PAYLOAD_BYTES + 1]),
            Err(PirError::InvalidPayloadSize)
        );
    }

    #[test]
    fn accepts_zecblock_announced_transaction_query_size() {
        // ZecBlock described transaction-query uploads of about 1.2 MB.
        // This regression test protects against a 1 MiB cap rejecting that size.
        let payload = vec![0xA5; 1_200_000];
        assert!(EncryptedPirQuery::from_ciphertext(payload).is_ok());
    }

    #[test]
    fn read_only_client_returns_opaque_encrypted_response() {
        let client = PrivateExplorerClient::new(EchoCiphertextTransport);
        let query = EncryptedPirQuery::from_ciphertext(vec![0xA5, 0x5A, 0x01]).unwrap();
        let response = client.query(&query).unwrap();
        assert_eq!(response.as_bytes(), &[0xA5, 0x5A, 0x01]);
    }

    #[test]
    fn failed_pir_never_falls_back_to_plaintext_transport() {
        let client = PrivateExplorerClient::new(FailingTransport);
        let query = EncryptedPirQuery::from_ciphertext(vec![0x10, 0x20]).unwrap();
        assert_eq!(
            client.query(&query),
            Err(PirError::TransportUnavailable)
        );
    }
}
