//! Read-only connection probe for a Zcash Testnet lightwalletd endpoint.
//!
//! This module calls only GetLightdInfo and GetLatestBlock. It does not submit
//! transactions, query addresses, import viewing keys, or sync wallet notes.
//! The caller must supply an exact authority allowlist from trusted deployment
//! configuration; never populate it from an agent-provided URL.

use std::time::Duration;

use http::Uri;
use zcash_client_backend::proto::service::{
    compact_tx_streamer_client::CompactTxStreamerClient, ChainSpec, Empty,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const RPC_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestnetLightwalletdStatus {
    pub server_version: String,
    pub vendor: String,
    pub network: String,
    pub reported_tip_height: u64,
    pub observed_tip_height: u64,
    pub estimated_tip_height: u64,
    pub protocol_version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightwalletdError {
    InvalidEndpoint,
    EndpointNotAllowlisted,
    ConnectTimeout,
    ConnectFailed,
    RpcTimeout,
    RpcFailed,
    WrongNetwork,
    InconsistentTip,
}

/// Validates a TLS-only endpoint against an exact authority allowlist.
///
/// Example allowlist entries include the hostname and port, e.g.
/// `testnet.example.org:9067`. The endpoint cannot contain credentials,
/// paths, query strings, fragments, or an IP address unless that exact
/// authority is explicitly allowlisted.
pub fn validate_testnet_endpoint(
    endpoint: &str,
    allowed_authorities: &[&str],
) -> Result<String, LightwalletdError> {
    let uri: Uri = endpoint
        .parse()
        .map_err(|_| LightwalletdError::InvalidEndpoint)?;
    if uri.scheme_str() != Some("https") {
        return Err(LightwalletdError::InvalidEndpoint);
    }

    let authority = uri
        .authority()
        .ok_or(LightwalletdError::InvalidEndpoint)?;
    let authority_text = authority.as_str();
    if authority_text.contains('@')
        || uri.query().is_some()
        || uri.path() != "/"
    {
        return Err(LightwalletdError::InvalidEndpoint);
    }

    if !allowed_authorities
        .iter()
        .any(|allowed| authority_text.eq_ignore_ascii_case(allowed))
    {
        return Err(LightwalletdError::EndpointNotAllowlisted);
    }

    Ok(endpoint.to_owned())
}

/// Connects to the allowlisted endpoint and verifies it is serving Testnet.
///
/// The returned heights are observations from the configured lightwalletd
/// service, not independent consensus verification. Callers must not treat
/// this probe as evidence of wallet balance or transaction settlement.
pub async fn probe_testnet_lightwalletd(
    endpoint: &str,
    allowed_authorities: &[&str],
) -> Result<TestnetLightwalletdStatus, LightwalletdError> {
    let endpoint = validate_testnet_endpoint(endpoint, allowed_authorities)?;

    let mut client = tokio::time::timeout(
        CONNECT_TIMEOUT,
        CompactTxStreamerClient::connect(endpoint),
    )
    .await
    .map_err(|_| LightwalletdError::ConnectTimeout)?
    .map_err(|_| LightwalletdError::ConnectFailed)?
    .max_decoding_message_size(MAX_RESPONSE_BYTES);

    let info = tokio::time::timeout(
        RPC_TIMEOUT,
        client.get_lightd_info(Empty {}),
    )
    .await
    .map_err(|_| LightwalletdError::RpcTimeout)?
    .map_err(|_| LightwalletdError::RpcFailed)?
    .into_inner();

    if info.chain_name != "test" {
        return Err(LightwalletdError::WrongNetwork);
    }

    let tip = tokio::time::timeout(
        RPC_TIMEOUT,
        client.get_latest_block(ChainSpec {}),
    )
    .await
    .map_err(|_| LightwalletdError::RpcTimeout)?
    .map_err(|_| LightwalletdError::RpcFailed)?
    .into_inner();

    if tip.height == 0 || info.block_height == 0 || info.estimated_height < info.block_height {
        return Err(LightwalletdError::InconsistentTip);
    }

    Ok(TestnetLightwalletdStatus {
        server_version: info.version,
        vendor: info.vendor,
        network: info.chain_name,
        reported_tip_height: info.block_height,
        observed_tip_height: tip.height,
        estimated_tip_height: info.estimated_height,
        protocol_version: info.lightwallet_protocol_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALLOWLIST: &[&str] = &["testnet.example.org:9067"];

    #[test]
    fn accepts_only_https_exact_allowlisted_authority() {
        assert_eq!(
            validate_testnet_endpoint(
                "https://testnet.example.org:9067",
                ALLOWLIST
            ),
            Ok("https://testnet.example.org:9067".to_owned())
        );
    }

    #[test]
    fn rejects_plaintext_grpc() {
        assert_eq!(
            validate_testnet_endpoint(
                "http://testnet.example.org:9067",
                ALLOWLIST
            ),
            Err(LightwalletdError::InvalidEndpoint)
        );
    }

    #[test]
    fn rejects_non_allowlisted_authority() {
        assert_eq!(
            validate_testnet_endpoint(
                "https://attacker.example.org:9067",
                ALLOWLIST
            ),
            Err(LightwalletdError::EndpointNotAllowlisted)
        );
    }

    #[test]
    fn rejects_credentials_and_url_query_parameters() {
        assert_eq!(
            validate_testnet_endpoint(
                "https://user:password@testnet.example.org:9067",
                ALLOWLIST
            ),
            Err(LightwalletdError::InvalidEndpoint)
        );
        assert_eq!(
            validate_testnet_endpoint(
                "https://testnet.example.org:9067/?token=secret",
                ALLOWLIST
            ),
            Err(LightwalletdError::InvalidEndpoint)
        );
    }

    #[test]
    fn rejects_paths_and_unlisted_ports() {
        assert_eq!(
            validate_testnet_endpoint(
                "https://testnet.example.org:9067/rpc",
                ALLOWLIST
            ),
            Err(LightwalletdError::InvalidEndpoint)
        );
        assert_eq!(
            validate_testnet_endpoint(
                "https://testnet.example.org:443",
                ALLOWLIST
            ),
            Err(LightwalletdError::EndpointNotAllowlisted)
        );
    }

    #[test]
    fn rejects_malformed_uri() {
        assert_eq!(
            validate_testnet_endpoint("not a uri", ALLOWLIST),
            Err(LightwalletdError::InvalidEndpoint)
        );
    }
}
