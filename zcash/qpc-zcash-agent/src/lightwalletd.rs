//! Read-only Zcash Testnet lightwalletd access and checkpoint verification.
//!
//! The service is treated as an untrusted data source. A TreeState is accepted
//! only when it matches a checkpoint (height, block hash, and complete TreeState
//! digest) supplied by independently trusted application configuration.
//! Never derive the expected checkpoint from the same lightwalletd response.

use std::time::Duration;

use http::Uri;
use sha2::{Digest, Sha256};
use zcash_client_backend::proto::service::{
    compact_tx_streamer_client::CompactTxStreamerClient, BlockId, ChainSpec, Empty, TreeState,
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
    InvalidCheckpoint,
    CheckpointHeightMismatch,
    CheckpointHashMismatch,
    CheckpointTreeStateMismatch,
}

/// A trust anchor supplied by application configuration, not by the server.
///
/// The block hash and TreeState digest must be provisioned independently (for
/// example, from a separately operated full node or a reviewed release config).
/// Passing values learned from the same endpoint defeats this check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedTestnetCheckpoint {
    height: u64,
    block_hash: String,
    tree_state_sha256: String,
}

impl TrustedTestnetCheckpoint {
    pub fn new(
        height: u64,
        block_hash: impl Into<String>,
        tree_state_sha256: impl Into<String>,
    ) -> Result<Self, LightwalletdError> {
        let block_hash = block_hash.into().to_ascii_lowercase();
        let tree_state_sha256 = tree_state_sha256.into().to_ascii_lowercase();
        if height == 0
            || !is_64_hex(&block_hash)
            || !is_64_hex(&tree_state_sha256)
        {
            return Err(LightwalletdError::InvalidCheckpoint);
        }

        Ok(Self {
            height,
            block_hash,
            tree_state_sha256,
        })
    }

    pub fn height(&self) -> u64 {
        self.height
    }
}

/// A TreeState that has matched an independently configured checkpoint.
///
/// Its fields are private so wallet import cannot accidentally accept an
/// arbitrary server response.
pub struct VerifiedTestnetTreeState(TreeState);

impl VerifiedTestnetTreeState {
    pub fn verify(
        state: TreeState,
        checkpoint: &TrustedTestnetCheckpoint,
    ) -> Result<Self, LightwalletdError> {
        if state.network != "test" {
            return Err(LightwalletdError::WrongNetwork);
        }
        if state.height != checkpoint.height {
            return Err(LightwalletdError::CheckpointHeightMismatch);
        }
        if !is_64_hex(&state.hash)
            || !state.hash.eq_ignore_ascii_case(&checkpoint.block_hash)
        {
            return Err(LightwalletdError::CheckpointHashMismatch);
        }
        if !tree_state_digest(&state).eq_ignore_ascii_case(&checkpoint.tree_state_sha256) {
            return Err(LightwalletdError::CheckpointTreeStateMismatch);
        }

        Ok(Self(state))
    }

    pub(crate) fn into_tree_state(self) -> TreeState {
        self.0
    }
}

fn is_64_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn update_digest_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

/// Computes a domain-separated digest over every TreeState field.
/// Exposed only within this crate so tests/config tooling can verify fixtures.
pub(crate) fn tree_state_digest(state: &TreeState) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"qpc-zcash-treestate-v1\0");
    update_digest_field(&mut hasher, state.network.as_bytes());
    update_digest_field(&mut hasher, &state.height.to_be_bytes());
    update_digest_field(&mut hasher, state.hash.to_ascii_lowercase().as_bytes());
    update_digest_field(&mut hasher, &state.time.to_be_bytes());
    update_digest_field(&mut hasher, state.sapling_tree.as_bytes());
    update_digest_field(&mut hasher, state.orchard_tree.as_bytes());
    update_digest_field(&mut hasher, state.ironwood_tree.as_bytes());

    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
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

    let authority = uri.authority().ok_or(LightwalletdError::InvalidEndpoint)?;
    let authority_text = authority.as_str();
    if authority_text.contains('@') || uri.query().is_some() || uri.path() != "/" {
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

/// Connects to the allowlisted endpoint and verifies that it serves Testnet.
///
/// The returned heights are observations from the configured lightwalletd
/// service, not independent consensus verification.
pub async fn probe_testnet_lightwalletd(
    endpoint: &str,
    allowed_authorities: &[&str],
) -> Result<TestnetLightwalletdStatus, LightwalletdError> {
    let endpoint = validate_testnet_endpoint(endpoint, allowed_authorities)?;

    let mut client =
        tokio::time::timeout(CONNECT_TIMEOUT, CompactTxStreamerClient::connect(endpoint))
            .await
            .map_err(|_| LightwalletdError::ConnectTimeout)?
            .map_err(|_| LightwalletdError::ConnectFailed)?
            .max_decoding_message_size(MAX_RESPONSE_BYTES);

    let info = tokio::time::timeout(RPC_TIMEOUT, client.get_lightd_info(Empty {}))
        .await
        .map_err(|_| LightwalletdError::RpcTimeout)?
        .map_err(|_| LightwalletdError::RpcFailed)?
        .into_inner();

    if info.chain_name != "test" {
        return Err(LightwalletdError::WrongNetwork);
    }

    let tip = tokio::time::timeout(RPC_TIMEOUT, client.get_latest_block(ChainSpec {}))
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

/// Fetches a TreeState at a pinned height and validates all fields against an
/// independently trusted checkpoint before returning an importable capability.
///
/// This does not independently validate consensus or prove the server is honest.
/// The configured block hash and TreeState digest are the trust anchor.
pub async fn fetch_verified_testnet_tree_state(
    endpoint: &str,
    allowed_authorities: &[&str],
    checkpoint: &TrustedTestnetCheckpoint,
) -> Result<VerifiedTestnetTreeState, LightwalletdError> {
    let endpoint = validate_testnet_endpoint(endpoint, allowed_authorities)?;

    let mut client =
        tokio::time::timeout(CONNECT_TIMEOUT, CompactTxStreamerClient::connect(endpoint))
            .await
            .map_err(|_| LightwalletdError::ConnectTimeout)?
            .map_err(|_| LightwalletdError::ConnectFailed)?
            .max_decoding_message_size(MAX_RESPONSE_BYTES);

    let info = tokio::time::timeout(RPC_TIMEOUT, client.get_lightd_info(Empty {}))
        .await
        .map_err(|_| LightwalletdError::RpcTimeout)?
        .map_err(|_| LightwalletdError::RpcFailed)?
        .into_inner();

    if info.chain_name != "test" {
        return Err(LightwalletdError::WrongNetwork);
    }

    let tip = tokio::time::timeout(RPC_TIMEOUT, client.get_latest_block(ChainSpec {}))
        .await
        .map_err(|_| LightwalletdError::RpcTimeout)?
        .map_err(|_| LightwalletdError::RpcFailed)?
        .into_inner();

    if tip.height < checkpoint.height {
        return Err(LightwalletdError::InconsistentTip);
    }

    let state = tokio::time::timeout(
        RPC_TIMEOUT,
        client.get_tree_state(BlockId {
            height: checkpoint.height,
            hash: Vec::new(),
        }),
    )
    .await
    .map_err(|_| LightwalletdError::RpcTimeout)?
    .map_err(|_| LightwalletdError::RpcFailed)?
    .into_inner();

    VerifiedTestnetTreeState::verify(state, checkpoint)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALLOWLIST: &[&str] = &["testnet.example.org:9067"];

    fn test_tree_state() -> TreeState {
        TreeState {
            network: "test".to_owned(),
            height: 1_000,
            hash: "ab".repeat(32),
            time: 1_700_000_000,
            sapling_tree: "sapling-fixture".to_owned(),
            orchard_tree: "orchard-fixture".to_owned(),
            ironwood_tree: String::new(),
        }
    }

    fn trusted_checkpoint(state: &TreeState) -> TrustedTestnetCheckpoint {
        TrustedTestnetCheckpoint::new(
            state.height,
            state.hash.clone(),
            tree_state_digest(state),
        )
        .unwrap()
    }

    #[test]
    fn accepts_only_https_exact_allowlisted_authority() {
        assert_eq!(
            validate_testnet_endpoint("https://testnet.example.org:9067", ALLOWLIST),
            Ok("https://testnet.example.org:9067".to_owned())
        );
    }

    #[test]
    fn rejects_plaintext_grpc() {
        assert_eq!(
            validate_testnet_endpoint("http://testnet.example.org:9067", ALLOWLIST),
            Err(LightwalletdError::InvalidEndpoint)
        );
    }

    #[test]
    fn rejects_non_allowlisted_authority() {
        assert_eq!(
            validate_testnet_endpoint("https://attacker.example.org:9067", ALLOWLIST),
            Err(LightwalletdError::EndpointNotAllowlisted)
        );
    }

    #[test]
    fn rejects_credentials_and_url_query_parameters() {
        assert_eq!(
            validate_testnet_endpoint("https://user:password@testnet.example.org:9067", ALLOWLIST),
            Err(LightwalletdError::InvalidEndpoint)
        );
        assert_eq!(
            validate_testnet_endpoint("https://testnet.example.org:9067/?token=secret", ALLOWLIST),
            Err(LightwalletdError::InvalidEndpoint)
        );
    }

    #[test]
    fn rejects_paths_and_unlisted_ports() {
        assert_eq!(
            validate_testnet_endpoint("https://testnet.example.org:9067/rpc", ALLOWLIST),
            Err(LightwalletdError::InvalidEndpoint)
        );
        assert_eq!(
            validate_testnet_endpoint("https://testnet.example.org:443", ALLOWLIST),
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

    #[test]
    fn accepts_tree_state_only_when_all_checkpoint_fields_match() {
        let state = test_tree_state();
        let checkpoint = trusted_checkpoint(&state);
        assert!(VerifiedTestnetTreeState::verify(state, &checkpoint).is_ok());
    }

    #[test]
    fn rejects_wrong_network_even_when_hash_and_digest_are_pinned() {
        let state = test_tree_state();
        let checkpoint = trusted_checkpoint(&state);
        let wrong_network = TreeState {
            network: "main".to_owned(),
            ..state
        };
        assert_eq!(
            VerifiedTestnetTreeState::verify(wrong_network, &checkpoint).err(),
            Some(LightwalletdError::WrongNetwork)
        );
    }

    #[test]
    fn rejects_checkpoint_height_mismatch() {
        let state = test_tree_state();
        let mut checkpoint = trusted_checkpoint(&state);
        checkpoint.height += 1;
        assert_eq!(
            VerifiedTestnetTreeState::verify(state, &checkpoint).err(),
            Some(LightwalletdError::CheckpointHeightMismatch)
        );
    }

    #[test]
    fn rejects_checkpoint_block_hash_mismatch() {
        let state = test_tree_state();
        let mut checkpoint = trusted_checkpoint(&state);
        checkpoint.block_hash = "cd".repeat(32);
        assert_eq!(
            VerifiedTestnetTreeState::verify(state, &checkpoint).err(),
            Some(LightwalletdError::CheckpointHashMismatch)
        );
    }

    #[test]
    fn rejects_checkpoint_tree_state_mismatch() {
        let state = test_tree_state();
        let checkpoint = trusted_checkpoint(&state);
        let changed_tree = TreeState {
            orchard_tree: "different-tree".to_owned(),
            ..state
        };
        assert_eq!(
            VerifiedTestnetTreeState::verify(changed_tree, &checkpoint).err(),
            Some(LightwalletdError::CheckpointTreeStateMismatch)
        );
    }

    #[test]
    fn rejects_malformed_trusted_checkpoint() {
        assert_eq!(
            TrustedTestnetCheckpoint::new(0, "not-a-hash", "not-a-digest").err(),
            Some(LightwalletdError::InvalidCheckpoint)
        );
    }
}
