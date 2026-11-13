//! Safe initialization boundary for a local Zcash Testnet light-wallet database.
//!
//! This initializes storage only. It does not generate/import keys, create an
//! address, connect to a lightwalletd server, sync funds, sign, or broadcast.

use std::path::Path;

use rand_core::OsRng;
use zcash_client_sqlite::{
    util::SystemClock,
    wallet::init::WalletMigrator,
    WalletDb,
};
use zcash_protocol::consensus::Network;

/// Creates or migrates a local Testnet wallet database without importing a seed.
///
/// The caller must keep the resulting database in private app storage with
/// restrictive filesystem permissions. No secret material is accepted here.
pub fn initialize_testnet_wallet_database(path: impl AsRef<Path>) -> Result<(), String> {
    let mut wallet_db = WalletDb::for_path(
        path,
        Network::TestNetwork,
        SystemClock,
        OsRng,
    )
    .map_err(|err| format!("wallet database open failed: {err}"))?;

    WalletMigrator::new()
        .init_or_migrate(&mut wallet_db)
        .map_err(|err| format!("wallet database migration failed: {err}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_testnet_storage_without_seed_or_network_activity() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let db_path = temp.path().join("qpc-zcash-testnet.sqlite");

        initialize_testnet_wallet_database(&db_path).expect("initialize local wallet DB");

        assert!(db_path.exists());
        assert!(std::fs::metadata(&db_path).expect("wallet DB metadata").len() > 0);
    }

    #[test]
    fn does_not_overwrite_existing_wallet_database() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let db_path = temp.path().join("qpc-zcash-testnet.sqlite");

        initialize_testnet_wallet_database(&db_path).expect("first initialization");
        let before = std::fs::metadata(&db_path).expect("metadata").len();

        initialize_testnet_wallet_database(&db_path).expect("idempotent migration");
        let after = std::fs::metadata(&db_path).expect("metadata").len();

        assert_eq!(before, after);
    }
}
