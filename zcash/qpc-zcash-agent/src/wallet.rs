//! SQLite wallet storage and durable compact-block cache for Testnet sync.
//!
//! The wallet DB is mutated only through zcash_client_sqlite APIs. QPC replay
//! state remains in its own separate SQLite database. The block cache below
//! implements the backend's BlockCache contract and stores compact blocks in
//! its own SQLite file. No seed or viewing key is accepted here.

use std::{
    ops::Range,
    path::Path,
    sync::Mutex,
};

use async_trait::async_trait;
use prost::Message;
use rand_core::OsRng;
use rusqlite::{params, Connection, Error as SqliteError, TransactionBehavior};
use tonic::transport::Endpoint;
use zcash_client_backend::{
    data_api::{
        chain::{self, BlockCache, BlockSource},
        scanning::ScanRange,
    },
    proto::{compact_formats::CompactBlock, service::compact_tx_streamer_client::CompactTxStreamerClient},
    sync,
};
use zcash_client_sqlite::{
    util::SystemClock,
    wallet::init::init_wallet_db,
    WalletDb,
};
use zcash_protocol::consensus::{BlockHeight, Network};

type TestnetDb = WalletDb<Connection, Network, SystemClock, OsRng>;

#[derive(Debug)]
pub enum BlockCacheError {
    Sqlite(SqliteError),
    Protobuf(prost::DecodeError),
    MissingHeight(u32),
    InvalidHeight(u32),
}

impl From<SqliteError> for BlockCacheError {
    fn from(value: SqliteError) -> Self {
        Self::Sqlite(value)
    }
}
impl From<prost::DecodeError> for BlockCacheError {
    fn from(value: prost::DecodeError) -> Self {
        Self::Protobuf(value)
    }
}
impl core::fmt::Display for BlockCacheError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "compact-block SQLite error: {e}"),
            Self::Protobuf(e) => write!(f, "compact-block decode error: {e}"),
            Self::MissingHeight(h) => write!(f, "compact block missing at height {h}"),
            Self::InvalidHeight(h) => write!(f, "invalid compact-block height {h}"),
        }
    }
}
impl std::error::Error for BlockCacheError {}

/// Persistent compact-block cache implementing the official backend BlockCache API.
pub struct SqliteBlockCache {
    connection: Mutex<Connection>,
}

impl SqliteBlockCache {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, BlockCacheError> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS qpc_compact_blocks (
                 height INTEGER NOT NULL PRIMARY KEY,
                 data BLOB NOT NULL
             );",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn query_blocks(&self, range: Range<BlockHeight>) -> Result<Vec<CompactBlock>, BlockCacheError> {
        let connection = self.connection.lock().map_err(|_| {
            BlockCacheError::Sqlite(SqliteError::InvalidQuery)
        })?;
        let mut statement = connection.prepare(
            "SELECT height, data FROM qpc_compact_blocks
             WHERE height >= ?1 AND height < ?2 ORDER BY height ASC",
        )?;
        let rows = statement.query_map(
            params![u32::from(range.start), u32::from(range.end)],
            |row| {
                let stored_height: u32 = row.get(0)?;
                let bytes: Vec<u8> = row.get(1)?;
                Ok((stored_height, bytes))
            },
        )?;

        let mut blocks = Vec::new();
        let mut expected = u32::from(range.start);
        for row in rows {
            let (height, bytes) = row?;
            if height != expected {
                return Err(BlockCacheError::MissingHeight(expected));
            }
            let block = CompactBlock::decode(bytes.as_slice())?;
            if block.height != u64::from(height) {
                return Err(BlockCacheError::InvalidHeight(height));
            }
            blocks.push(block);
            expected = expected.saturating_add(1);
        }
        Ok(blocks)
    }
}

impl BlockSource for SqliteBlockCache {
    type Error = BlockCacheError;

    fn with_blocks<F, WalletErrT>(
        &self,
        from_height: Option<BlockHeight>,
        limit: Option<usize>,
        mut with_block: F,
    ) -> Result<(), chain::error::Error<WalletErrT, Self::Error>>
    where
        F: FnMut(CompactBlock) -> Result<(), chain::error::Error<WalletErrT, Self::Error>>,
    {
        let connection = self.connection.lock().map_err(|_| {
            chain::error::Error::BlockSource(BlockCacheError::Sqlite(SqliteError::InvalidQuery))
        })?;
        let from = from_height.map_or(0, u32::from);
        let limit = limit
            .and_then(|value| i64::try_from(value).ok())
            .unwrap_or(i64::MAX);
        let mut statement = connection
            .prepare(
                "SELECT height, data FROM qpc_compact_blocks
                 WHERE height >= ?1 ORDER BY height ASC LIMIT ?2",
            )
            .map_err(|error| chain::error::Error::BlockSource(BlockCacheError::Sqlite(error)))?;
        let rows = statement
            .query_map(params![from, limit], |row| {
                let stored_height: u32 = row.get(0)?;
                let bytes: Vec<u8> = row.get(1)?;
                Ok((stored_height, bytes))
            })
            .map_err(|error| chain::error::Error::BlockSource(BlockCacheError::Sqlite(error)))?;

        let mut blocks = Vec::new();
        for row in rows {
            let (height, bytes) =
                row.map_err(|error| chain::error::Error::BlockSource(BlockCacheError::Sqlite(error)))?;
            let block = CompactBlock::decode(bytes.as_slice())
                .map_err(|error| chain::error::Error::BlockSource(BlockCacheError::Protobuf(error)))?;
            if block.height != u64::from(height) {
                return Err(chain::error::Error::BlockSource(
                    BlockCacheError::InvalidHeight(height),
                ));
            }
            blocks.push(block);
        }
        drop(statement);
        drop(connection);

        for block in blocks {
            with_block(block)?;
        }
        Ok(())
    }
}

#[async_trait]
impl BlockCache for SqliteBlockCache {
    fn get_tip_height(
        &self,
        range: Option<&ScanRange>,
    ) -> Result<Option<BlockHeight>, Self::Error> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| BlockCacheError::Sqlite(SqliteError::InvalidQuery))?;
        let height: Option<u32> = if let Some(range) = range {
            let bounds = range.block_range();
            connection.query_row(
                "SELECT MAX(height) FROM qpc_compact_blocks WHERE height >= ?1 AND height < ?2",
                params![u32::from(bounds.start), u32::from(bounds.end)],
                |row| row.get(0),
            )?
        } else {
            connection.query_row(
                "SELECT MAX(height) FROM qpc_compact_blocks",
                [],
                |row| row.get(0),
            )?
        };
        Ok(height.map(BlockHeight::from_u32))
    }

    async fn read(&self, range: &ScanRange) -> Result<Vec<CompactBlock>, Self::Error> {
        self.query_blocks(range.block_range().clone())
    }

    async fn insert(&self, compact_blocks: Vec<CompactBlock>) -> Result<(), Self::Error> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| BlockCacheError::Sqlite(SqliteError::InvalidQuery))?;
        let transaction =
            connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        {
            let mut statement = transaction.prepare(
                "INSERT INTO qpc_compact_blocks(height, data) VALUES(?1, ?2)
                 ON CONFLICT(height) DO UPDATE SET data = excluded.data",
            )?;
            for block in compact_blocks {
                statement.execute(params![block.height, block.encode_to_vec()])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    async fn delete(&self, range: ScanRange) -> Result<(), Self::Error> {
        let bounds = range.block_range();
        let connection = self
            .connection
            .lock()
            .map_err(|_| BlockCacheError::Sqlite(SqliteError::InvalidQuery))?;
        connection.execute(
            "DELETE FROM qpc_compact_blocks WHERE height >= ?1 AND height < ?2",
            params![u32::from(bounds.start), u32::from(bounds.end)],
        )?;
        Ok(())
    }
}

pub struct TestnetWallet {
    wallet: TestnetDb,
    cache: SqliteBlockCache,
}

#[derive(Debug)]
pub enum WalletStorageError {
    Wallet(String),
    Cache(String),
    SamePath,
    Endpoint(String),
    Transport(String),
    Sync(String),
    InvalidBatch,
}

impl core::fmt::Display for WalletStorageError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Wallet(e) => write!(f, "wallet DB: {e}"),
            Self::Cache(e) => write!(f, "block cache DB: {e}"),
            Self::SamePath => f.write_str("wallet and cache paths must differ"),
            Self::Endpoint(e) => write!(f, "endpoint: {e}"),
            Self::Transport(e) => write!(f, "transport: {e}"),
            Self::Sync(e) => write!(f, "sync: {e}"),
            Self::InvalidBatch => f.write_str("batch size must be greater than zero"),
        }
    }
}
impl std::error::Error for WalletStorageError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WalletCapabilities {
    pub orchard: bool,
    pub sapling: bool,
}
impl WalletCapabilities {
    pub fn supports(&self, receiver: crate::ShieldedReceiver) -> bool {
        match receiver {
            crate::ShieldedReceiver::Orchard => self.orchard,
            crate::ShieldedReceiver::Sapling => self.sapling,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WalletObservation {
    Unknown,
    Pending,
    Confirmed {
        confirmations: u32,
        block_height: u32,
    },
    Reorged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WalletError {
    Unavailable,
    SyncIncomplete,
    InternalFailure,
}

pub trait WalletBackend {
    fn capabilities(&self) -> Result<WalletCapabilities, WalletError>;
    fn observe_payment(
        &self,
        payment_id: crate::PaymentId,
    ) -> Result<WalletObservation, WalletError>;
}

impl TestnetWallet {
    /// Initializes a blank Testnet wallet DB and persistent compact-block cache.
    /// Existing seed-required migrations fail closed; this method never imports keys.
    pub fn open(
        wallet_path: impl AsRef<Path>,
        cache_path: impl AsRef<Path>,
    ) -> Result<Self, WalletStorageError> {
        if wallet_path.as_ref() == cache_path.as_ref() {
            return Err(WalletStorageError::SamePath);
        }
        let mut wallet = WalletDb::for_path(
            wallet_path,
            Network::TestNetwork,
            SystemClock,
            OsRng,
        )
        .map_err(|error| WalletStorageError::Wallet(error.to_string()))?;
        init_wallet_db(&mut wallet, None)
            .map_err(|error| WalletStorageError::Wallet(error.to_string()))?;
        let cache = SqliteBlockCache::open(cache_path)
            .map_err(|error| WalletStorageError::Cache(error.to_string()))?;
        Ok(Self { wallet, cache })
    }

    /// Connects to an HTTPS lightwalletd endpoint and runs the official Testnet
    /// light-client sync state machine. This is not full-node consensus validation;
    /// trust in the lightwalletd operator remains part of the model.
    pub async fn sync_testnet(
        &mut self,
        url: &str,
        batch_size: u32,
    ) -> Result<(), WalletStorageError> {
        if batch_size == 0 {
            return Err(WalletStorageError::InvalidBatch);
        }
        if !url.starts_with("https://") {
            return Err(WalletStorageError::Endpoint("HTTPS required".into()));
        }
        let endpoint = Endpoint::from_shared(url.to_owned())
            .map_err(|error| WalletStorageError::Endpoint(error.to_string()))?;
        let channel = endpoint
            .connect_timeout(std::time::Duration::from_secs(15))
            .connect()
            .await
            .map_err(|error| WalletStorageError::Transport(error.to_string()))?;
        let mut client = CompactTxStreamerClient::new(channel);
        sync::run(
            &mut client,
            &Network::TestNetwork,
            &self.cache,
            &mut self.wallet,
            batch_size,
        )
        .await
        .map_err(|error| WalletStorageError::Sync(error.to_string()))
    }
}
