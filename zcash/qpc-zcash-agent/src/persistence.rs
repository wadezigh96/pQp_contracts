//! Durable, atomic replay protection. Separate from Zcash wallet tables.
use std::path::Path;
use rusqlite::{params, Connection, Error as SqliteError, TransactionBehavior};
use crate::{IntentError, PaymentIntent, SourceChain};

pub struct PersistentPaymentStore { connection: Connection }

#[derive(Debug)]
pub enum PersistentStoreError { InvalidIntent(IntentError), DuplicatePaymentId, DuplicateNonce, Database(SqliteError) }

impl From<SqliteError> for PersistentStoreError {
    fn from(value: SqliteError) -> Self { Self::Database(value) }
}
impl core::fmt::Display for PersistentStoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidIntent(e) => write!(f, "invalid payment intent: {e:?}"),
            Self::DuplicatePaymentId => f.write_str("payment ID already registered"),
            Self::DuplicateNonce => f.write_str("source agent nonce already consumed"),
            Self::Database(e) => write!(f, "SQLite store error: {e}"),
        }
    }
}
impl std::error::Error for PersistentStoreError {}

impl PersistentPaymentStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, PersistentStoreError> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS qpc_payment_intents (
               payment_id BLOB PRIMARY KEY NOT NULL CHECK(length(payment_id)=32),
               source_chain TEXT NOT NULL,
               source_agent_id BLOB NOT NULL CHECK(length(source_agent_id)=32),
               nonce BLOB NOT NULL CHECK(length(nonce)=8),
               amount_zatoshis TEXT NOT NULL,
               expires_at_unix TEXT NOT NULL,
               UNIQUE(source_chain, source_agent_id, nonce)
             );"
        )?;
        Ok(Self { connection })
    }

    /// Atomically consumes a nonce scoped to (source chain, source agent).
    /// Big-endian BLOB preserves the full u64 range; IMMEDIATE serializes writers.
    pub fn register(&mut self, intent: &PaymentIntent, now: u64) -> Result<(), PersistentStoreError> {
        intent.validate(now).map_err(PersistentStoreError::InvalidIntent)?;
        let tx = self.connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM qpc_payment_intents WHERE payment_id=?1)",
            params![intent.id.as_bytes().as_slice()], |r| r.get(0)
        )?;
        if exists { return Err(PersistentStoreError::DuplicatePaymentId); }
        let nonce = intent.nonce.to_be_bytes();
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM qpc_payment_intents WHERE source_chain=?1 AND source_agent_id=?2 AND nonce=?3)",
            params![intent.source_chain.storage_key(), intent.source_agent_id.as_slice(), nonce.as_slice()],
            |r| r.get(0)
        )?;
        if exists { return Err(PersistentStoreError::DuplicateNonce); }
        tx.execute(
            "INSERT INTO qpc_payment_intents(payment_id,source_chain,source_agent_id,nonce,amount_zatoshis,expires_at_unix) VALUES(?1,?2,?3,?4,?5,?6)",
            params![intent.id.as_bytes().as_slice(), intent.source_chain.storage_key(), intent.source_agent_id.as_slice(), nonce.as_slice(), intent.amount_zatoshis.to_string(), intent.expires_at_unix.to_string()]
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn contains_nonce(&self, chain: SourceChain, agent: [u8;32], nonce: u64) -> Result<bool, PersistentStoreError> {
        self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM qpc_payment_intents WHERE source_chain=?1 AND source_agent_id=?2 AND nonce=?3)",
            params![chain.storage_key(), agent.as_slice(), nonce.to_be_bytes().as_slice()],
            |r| r.get(0)
        ).map_err(Into::into)
    }
}
