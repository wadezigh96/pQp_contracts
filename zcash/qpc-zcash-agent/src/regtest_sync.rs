//! Loopback-only compact-block synchronization using the pinned lightwalletd API.
//! These APIs do not identify Regtest separately from Testnet (`chain_name` is
//! specified as `main` or `test`), so activation and current branch compatibility
//! are checked against the configured LocalNetwork before scanning.

use std::{collections::BTreeMap, io, net::IpAddr, path::Path, sync::Mutex, time::Duration};

use http::Uri;
use rand_core::OsRng;
use zcash_client_backend::{
    data_api::{
        chain::{error, BlockCache, BlockSource},
        scanning::ScanRange,
    },
    proto::{
        compact_formats::CompactBlock,
        service::{compact_tx_streamer_client::CompactTxStreamerClient, Empty},
    },
    sync,
};
use zcash_client_sqlite::{util::SystemClock, wallet::init::WalletMigrator, WalletDb};
use zcash_protocol::{
    consensus::{BlockHeight, BranchId, NetworkType, Parameters},
    local_consensus::LocalNetwork,
};

use crate::{
    regtest_node::{verify_lightwalletd_tip, FullNodeRpcConfig},
    wallet::{harden_sqlite_sidecars, prepare_database_file, restrict_regular_file},
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const INFO_TIMEOUT: Duration = Duration::from_secs(5);
const SYNC_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_CACHED_BLOCKS: usize = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegtestSyncError {
    WrongNetwork,
    InvalidEndpoint,
    EndpointNotLoopback,
    MalformedServerInfo,
    ConsensusMismatch,
    Connect,
    Timeout,
    Database,
    Sync,
    InvalidBatchSize,
    NodeIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegtestSyncObservation {
    pub server_tip_height: u64,
    pub branch_id: u32,
}

/// Accepts plaintext gRPC only on a literal loopback address. No DNS names,
/// credentials, paths, TLS redirects, public hosts, or non-loopback interfaces.
pub fn validate_regtest_lightwalletd_endpoint(endpoint: &str) -> Result<String, RegtestSyncError> {
    let uri: Uri = endpoint
        .parse()
        .map_err(|_| RegtestSyncError::InvalidEndpoint)?;
    if uri.scheme_str() != Some("http") || uri.path() != "/" || uri.query().is_some() {
        return Err(RegtestSyncError::InvalidEndpoint);
    }
    let authority = uri.authority().ok_or(RegtestSyncError::InvalidEndpoint)?;
    if authority.as_str().contains('@') {
        return Err(RegtestSyncError::InvalidEndpoint);
    }
    let host = uri.host().ok_or(RegtestSyncError::InvalidEndpoint)?;
    let normalized_host = host.trim_start_matches('[').trim_end_matches(']');
    let ip: IpAddr = normalized_host
        .parse()
        .map_err(|_| RegtestSyncError::EndpointNotLoopback)?;
    if !ip.is_loopback() {
        return Err(RegtestSyncError::EndpointNotLoopback);
    }
    Ok(endpoint.to_owned())
}

/// Checks the pinned protocol's network label, Sapling activation and active
/// consensus branch against local Regtest consensus parameters.
pub fn verify_lightd_info(
    info: &zcash_client_backend::proto::service::LightdInfo,
    params: &LocalNetwork,
) -> Result<RegtestSyncObservation, RegtestSyncError> {
    if params.network_type() != NetworkType::Regtest {
        return Err(RegtestSyncError::WrongNetwork);
    }
    // lightwalletd 0.24 protocol defines only "main" and "test" labels; Regtest
    // nodes are exposed as "test". The remaining compatibility fields are mandatory.
    if info.chain_name != "test"
        || info.block_height == 0
        || info.estimated_height < info.block_height
    {
        return Err(RegtestSyncError::MalformedServerInfo);
    }
    let sapling = params
        .activation_height(zcash_protocol::consensus::NetworkUpgrade::Sapling)
        .map(u64::from)
        .ok_or(RegtestSyncError::ConsensusMismatch)?;
    if info.sapling_activation_height != sapling {
        return Err(RegtestSyncError::ConsensusMismatch);
    }
    let reported_branch =
        u32::from_str_radix(info.consensus_branch_id.trim_start_matches("0x"), 16)
            .map_err(|_| RegtestSyncError::MalformedServerInfo)?;
    let expected_branch = u32::from(BranchId::for_height(
        params,
        BlockHeight::from_u32(
            u32::try_from(info.block_height).map_err(|_| RegtestSyncError::MalformedServerInfo)?,
        ),
    ));
    if reported_branch != expected_branch {
        return Err(RegtestSyncError::ConsensusMismatch);
    }
    Ok(RegtestSyncObservation {
        server_tip_height: info.block_height,
        branch_id: reported_branch,
    })
}

/// Scans compact blocks into the official wallet database. Wallet scan ranges
/// and discovered notes are committed by the wallet database. A later call after
/// interruption resumes from wallet progress and re-fetches any uncached blocks.
/// Trial decryption and note discovery use the official backend scanner.
pub async fn synchronize_regtest_wallet(
    wallet_path: impl AsRef<Path>,
    endpoint: &str,
    node_rpc: &FullNodeRpcConfig,
    params: LocalNetwork,
    batch_size: u32,
) -> Result<RegtestSyncObservation, RegtestSyncError> {
    let endpoint = validate_regtest_lightwalletd_endpoint(endpoint)?;
    if batch_size == 0 || batch_size > MAX_CACHED_BLOCKS as u32 {
        return Err(RegtestSyncError::InvalidBatchSize);
    }
    if params.network_type() != NetworkType::Regtest {
        return Err(RegtestSyncError::WrongNetwork);
    }
    let connect = tokio::time::timeout(CONNECT_TIMEOUT, CompactTxStreamerClient::connect(endpoint))
        .await
        .map_err(|_| RegtestSyncError::Timeout)?
        .map_err(|_| RegtestSyncError::Connect)?;
    let mut client = connect.max_decoding_message_size(MAX_MESSAGE_BYTES);
    let info = tokio::time::timeout(INFO_TIMEOUT, client.get_lightd_info(Empty {}))
        .await
        .map_err(|_| RegtestSyncError::Timeout)?
        .map_err(|_| RegtestSyncError::Connect)?
        .into_inner();
    let observation = verify_lightd_info(&info, &params)?;
    let latest = tokio::time::timeout(
        INFO_TIMEOUT,
        client.get_latest_block(zcash_client_backend::proto::service::ChainSpec {}),
    )
    .await
    .map_err(|_| RegtestSyncError::Timeout)?
    .map_err(|_| RegtestSyncError::Connect)?
    .into_inner();
    verify_lightwalletd_tip(node_rpc, &params, latest.height, &latest.hash)
        .await
        .map_err(|_| RegtestSyncError::NodeIdentity)?;

    let cache = MemoryCompactBlockCache::default();
    let wallet_path = wallet_path.as_ref();
    prepare_database_file(wallet_path).map_err(|_| RegtestSyncError::Database)?;
    let mut wallet = WalletDb::for_path(wallet_path, params, SystemClock, OsRng)
        .map_err(|_| RegtestSyncError::Database)?;
    if WalletMigrator::new().init_or_migrate(&mut wallet).is_err() {
        drop(wallet);
        restrict_regular_file(wallet_path).map_err(|_| RegtestSyncError::Database)?;
        harden_sqlite_sidecars(wallet_path).map_err(|_| RegtestSyncError::Database)?;
        return Err(RegtestSyncError::Database);
    }
    let sync_result = match tokio::time::timeout(
        SYNC_TIMEOUT,
        sync::run(&mut client, &params, &cache, &mut wallet, batch_size),
    )
    .await
    {
        Err(_) => Err(RegtestSyncError::Timeout),
        Ok(Err(_)) => Err(RegtestSyncError::Sync),
        Ok(Ok(())) => Ok(()),
    };
    drop(wallet);
    restrict_regular_file(wallet_path).map_err(|_| RegtestSyncError::Database)?;
    harden_sqlite_sidecars(wallet_path).map_err(|_| RegtestSyncError::Database)?;
    sync_result?;
    Ok(observation)
}

/// Bounded-lifetime compact-block cache. Wallet scan ranges and discovered notes
/// are committed by WalletDb; after a process interruption blocks are safely
/// fetched again. Duplicate blocks are idempotent and conflicting heights fail.
#[derive(Default)]
struct MemoryCompactBlockCache(Mutex<BTreeMap<u32, CompactBlock>>);

impl BlockSource for MemoryCompactBlockCache {
    type Error = io::Error;

    fn with_blocks<F, WalletErrT>(
        &self,
        from_height: Option<BlockHeight>,
        limit: Option<usize>,
        mut with_block: F,
    ) -> Result<(), error::Error<WalletErrT, Self::Error>>
    where
        F: FnMut(CompactBlock) -> Result<(), error::Error<WalletErrT, Self::Error>>,
    {
        let blocks = self
            .0
            .lock()
            .map_err(|_| error::Error::BlockSource(io::Error::other("cache lock poisoned")))?;
        let from = from_height.map(u32::from).unwrap_or(0);
        for (_, block) in blocks.range(from..).take(limit.unwrap_or(usize::MAX)) {
            with_block(block.clone())?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl BlockCache for MemoryCompactBlockCache {
    fn get_tip_height(
        &self,
        range: Option<&ScanRange>,
    ) -> Result<Option<BlockHeight>, Self::Error> {
        let blocks = self
            .0
            .lock()
            .map_err(|_| io::Error::other("cache lock poisoned"))?;
        let range = range.map(|r| r.block_range());
        Ok(blocks
            .keys()
            .filter(|h| {
                range.map_or(true, |r| {
                    *h >= &u32::from(r.start) && *h < &u32::from(r.end)
                })
            })
            .next_back()
            .copied()
            .map(BlockHeight::from_u32))
    }

    async fn read(&self, range: &ScanRange) -> Result<Vec<CompactBlock>, Self::Error> {
        let blocks = self
            .0
            .lock()
            .map_err(|_| io::Error::other("cache lock poisoned"))?;
        let mut result = Vec::new();
        for height in u32::from(range.block_range().start)..u32::from(range.block_range().end) {
            match blocks.get(&height) {
                Some(block) => result.push(block.clone()),
                None => break,
            }
        }
        Ok(result)
    }

    async fn insert(&self, compact_blocks: Vec<CompactBlock>) -> Result<(), Self::Error> {
        let mut blocks = self
            .0
            .lock()
            .map_err(|_| io::Error::other("cache lock poisoned"))?;
        for block in compact_blocks {
            let height = u32::try_from(block.height).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "compact block height out of range",
                )
            })?;
            if let Some(existing) = blocks.get(&height) {
                if existing != &block {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "conflicting compact block at cached height",
                    ));
                }
            } else {
                blocks.insert(height, block);
            }
        }
        while blocks.len() > MAX_CACHED_BLOCKS {
            let oldest = *blocks.first_key_value().expect("nonempty bounded cache").0;
            blocks.remove(&oldest);
        }
        Ok(())
    }

    async fn delete(&self, range: ScanRange) -> Result<(), Self::Error> {
        let mut blocks = self
            .0
            .lock()
            .map_err(|_| io::Error::other("cache lock poisoned"))?;
        let start = u32::from(range.block_range().start);
        let end = u32::from(range.block_range().end);
        blocks.retain(|height, _| *height < start || *height >= end);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zcash_protocol::consensus::BlockHeight;

    fn params() -> LocalNetwork {
        LocalNetwork {
            overwinter: Some(BlockHeight::from_u32(1)),
            sapling: Some(BlockHeight::from_u32(2)),
            blossom: Some(BlockHeight::from_u32(1)),
            heartwood: Some(BlockHeight::from_u32(1)),
            canopy: Some(BlockHeight::from_u32(1)),
            nu5: Some(BlockHeight::from_u32(1)),
            nu6: Some(BlockHeight::from_u32(1)),
            nu6_1: Some(BlockHeight::from_u32(1)),
            nu6_2: Some(BlockHeight::from_u32(1)),
            nu6_3: Some(BlockHeight::from_u32(1)),
        }
    }

    #[test]
    fn accepts_only_explicit_loopback_http_endpoints() {
        assert!(validate_regtest_lightwalletd_endpoint("http://127.0.0.1:9067").is_ok());
        assert!(validate_regtest_lightwalletd_endpoint("http://[::1]:9067").is_ok());
        assert_eq!(
            validate_regtest_lightwalletd_endpoint("https://127.0.0.1:9067"),
            Err(RegtestSyncError::InvalidEndpoint)
        );
        assert_eq!(
            validate_regtest_lightwalletd_endpoint("http://example.org:9067"),
            Err(RegtestSyncError::EndpointNotLoopback)
        );
        assert_eq!(
            validate_regtest_lightwalletd_endpoint("http://127.0.0.1:9067@evil"),
            Err(RegtestSyncError::InvalidEndpoint)
        );
    }

    #[tokio::test]
    async fn mock_cache_deduplicates_identical_blocks_and_rejects_conflicts() {
        let cache = MemoryCompactBlockCache::default();
        let block = CompactBlock {
            height: 8,
            hash: vec![8],
            ..Default::default()
        };
        cache
            .insert(vec![block.clone(), block.clone()])
            .await
            .unwrap();
        assert_eq!(
            cache.get_tip_height(None).unwrap(),
            Some(BlockHeight::from_u32(8))
        );
        let conflict = CompactBlock {
            height: 8,
            hash: vec![9],
            ..Default::default()
        };
        assert_eq!(
            cache.insert(vec![conflict]).await.unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        let range = ScanRange::from_parts(
            BlockHeight::from_u32(8)..BlockHeight::from_u32(9),
            zcash_client_backend::data_api::scanning::ScanPriority::Historic,
        );
        assert_eq!(cache.read(&range).await.unwrap(), vec![block]);
    }

    #[tokio::test]
    async fn rejects_invalid_batch_size_before_network_access() {
        let result = synchronize_regtest_wallet(
            "/not/a/real/wallet",
            "http://127.0.0.1:9067",
            &FullNodeRpcConfig::new("http://127.0.0.1:18232", "u", "p").unwrap(),
            params(),
            0,
        )
        .await;
        assert_eq!(result, Err(RegtestSyncError::InvalidBatchSize));
    }

    #[test]
    fn rejects_mainnet_lightwalletd_identity() {
        let info = zcash_client_backend::proto::service::LightdInfo {
            chain_name: "main".into(),
            block_height: 20,
            estimated_height: 20,
            sapling_activation_height: 2,
            consensus_branch_id: "76b809bb".into(),
            ..Default::default()
        };
        assert!(verify_lightd_info(&info, &params()).is_err());
    }

    #[test]
    fn checks_network_activation_and_branch_before_scanning() {
        let info = zcash_client_backend::proto::service::LightdInfo {
            chain_name: "test".into(),
            sapling_activation_height: 2,
            block_height: 10,
            estimated_height: 10,
            consensus_branch_id: format!(
                "{:08x}",
                u32::from(BranchId::for_height(&params(), BlockHeight::from_u32(10)))
            ),
            ..Default::default()
        };
        assert!(verify_lightd_info(&info, &params()).is_ok());
        let mut wrong = info.clone();
        wrong.sapling_activation_height = 419_200;
        assert_eq!(
            verify_lightd_info(&wrong, &params()),
            Err(RegtestSyncError::ConsensusMismatch)
        );
        let mut malformed = info;
        malformed.consensus_branch_id = "no-branch".into();
        assert_eq!(
            verify_lightd_info(&malformed, &params()),
            Err(RegtestSyncError::MalformedServerInfo)
        );
    }
}
