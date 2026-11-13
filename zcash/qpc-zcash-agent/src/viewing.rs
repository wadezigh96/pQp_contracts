//! Testnet-only, view-only UFVK import boundary.
//!
//! This module imports a Unified Full Viewing Key into the official SQLite
//! wallet database using AccountPurpose::ViewOnly. It never accepts a seed or
//! spending key and does not construct, sign, or broadcast transactions.
//!
//! Import is intentionally separate from synchronization: a successful import
//! does not mean the wallet has scanned any blocks or verified a payment.

use std::path::Path;

use crate::wallet::{harden_sqlite_sidecars, prepare_database_file, restrict_regular_file};

use rand_core::OsRng;
use zcash_client_backend::keys::UnifiedFullViewingKey;
use zcash_client_backend::{
    data_api::{AccountBirthday, AccountPurpose, WalletWrite},
    proto::service::TreeState,
};
use zcash_client_sqlite::{util::SystemClock, wallet::init::WalletMigrator, WalletDb};
use zcash_protocol::consensus::Network;

const ACCOUNT_NAME_MAX_BYTES: usize = 64;

/// Imports a Testnet UFVK as a view-only account.
///
/// The TreeState must be fetched from the same trusted Testnet lightwalletd
/// endpoint that was checked by the caller. The network marker is checked
/// before it is used to construct the account birthday.
///
/// The UFVK is sensitive viewing material: callers must not log it, pass it to
/// agents, or store it outside private application storage. The SQLite wallet
/// database is already required to live in a private directory.
pub fn import_testnet_ufvk_view_only(
    database_path: impl AsRef<Path>,
    account_name: &str,
    encoded_ufvk: &str,
    prior_tree_state: TreeState,
) -> Result<(), String> {
    if account_name.is_empty()
        || account_name.len() > ACCOUNT_NAME_MAX_BYTES
        || account_name.starts_with(' ')
        || account_name.ends_with(' ')
        || !account_name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b' ' || b == b'-' || b == b'_')
    {
        return Err("view-only account name is invalid".into());
    }

    if prior_tree_state.network != "test" {
        return Err("view-only import requires a Testnet TreeState".into());
    }

    let ufvk = UnifiedFullViewingKey::decode(&Network::TestNetwork, encoded_ufvk)
        .map_err(|_| "UFVK is invalid or does not belong to Testnet".to_owned())?;

    let birthday = AccountBirthday::from_treestate(prior_tree_state, None)
        .map_err(|_| "Testnet wallet birthday state is invalid".to_owned())?;

    let path = database_path.as_ref();
    prepare_database_file(path)?;

    let mut wallet_db = WalletDb::for_path(path, Network::TestNetwork, SystemClock, OsRng)
        .map_err(|_| "wallet database open failed".to_owned())?;

    WalletMigrator::new()
        .init_or_migrate(&mut wallet_db)
        .map_err(|_| "wallet database migration failed".to_owned())?;

    wallet_db
        .import_account_ufvk(
            account_name,
            &ufvk,
            &birthday,
            AccountPurpose::ViewOnly,
            Some("qpc-view-only-testnet"),
        )
        .map_err(|_| "view-only UFVK import failed".to_owned())?;

    drop(wallet_db);
    restrict_regular_file(path)?;
    harden_sqlite_sidecars(path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn private_test_directory(temp: &tempfile::TempDir, name: &str) -> std::path::PathBuf {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = temp.path().join(name);
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            path
        }
        #[cfg(not(unix))]
        {
            let path = temp.path().join(name);
            fs::create_dir(&path).unwrap();
            path
        }
    }

    fn testnet_tree_state() -> TreeState {
        TreeState {
            network: "test".to_owned(),
            height: 1_000_000,
            hash: "0".repeat(64),
            time: 1_700_000_000,
            sapling_tree: String::new(),
            orchard_tree: String::new(),
            ironwood_tree: String::new(),
        }
    }

    fn testnet_ufvk() -> String {
        use zcash_keys::keys::UnifiedSpendingKey;
        let usk = UnifiedSpendingKey::from_seed(
            &Network::TestNetwork,
            &[0x42u8; 32],
            zip32::AccountId::ZERO,
        )
        .expect("derive testnet spending key");
        usk.to_unified_full_viewing_key()
            .encode(&Network::TestNetwork)
    }

    #[test]
    fn rejects_non_testnet_tree_state_before_wallet_database_access() {
        let state = TreeState {
            network: "main".to_owned(),
            ..Default::default()
        };

        let result = import_testnet_ufvk_view_only(
            "/path/that/must/not/be/touched/wallet.sqlite",
            "qpc-view",
            "not-a-ufvk",
            state,
        );

        assert_eq!(
            result,
            Err("view-only import requires a Testnet TreeState".to_owned())
        );
    }

    #[test]
    fn rejects_invalid_account_name_before_wallet_database_access() {
        let state = testnet_tree_state();

        let result = import_testnet_ufvk_view_only(
            "/path/that/must/not/be/touched/wallet.sqlite",
            "../qpc",
            "not-a-ufvk",
            state,
        );

        assert_eq!(result, Err("view-only account name is invalid".to_owned()));
    }

    #[test]
    fn rejects_invalid_ufvk_before_wallet_database_access() {
        let result = import_testnet_ufvk_view_only(
            "/path/that/must/not/be/touched/wallet.sqlite",
            "qpc-view",
            "not-a-ufvk",
            testnet_tree_state(),
        );

        assert_eq!(
            result,
            Err("UFVK is invalid or does not belong to Testnet".to_owned())
        );
    }

    #[test]
    fn rejects_mainnet_ufvk_on_testnet_import() {
        use zcash_keys::keys::UnifiedSpendingKey;
        let usk = UnifiedSpendingKey::from_seed(
            &Network::MainNetwork,
            &[0x42u8; 32],
            zip32::AccountId::ZERO,
        )
        .expect("derive mainnet spending key");
        let mainnet_ufvk = usk
            .to_unified_full_viewing_key()
            .encode(&Network::MainNetwork);

        let result = import_testnet_ufvk_view_only(
            "/path/that/must/not/be/touched/wallet.sqlite",
            "qpc-view",
            &mainnet_ufvk,
            testnet_tree_state(),
        );

        assert_eq!(
            result,
            Err("UFVK is invalid or does not belong to Testnet".to_owned())
        );
    }

    #[test]
    fn imports_testnet_view_only_ufvk_and_rejects_duplicate() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let private = private_test_directory(&temp, "private-wallet");
        let db_path = private.join("qpc-view-only.sqlite");
        let ufvk = testnet_ufvk();
        let state = testnet_tree_state();

        import_testnet_ufvk_view_only(&db_path, "qpc-view", &ufvk, state.clone())
            .expect("first view-only import");

        assert!(db_path.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&db_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let second = import_testnet_ufvk_view_only(&db_path, "qpc-view-2", &ufvk, state);
        assert!(
            second.is_err(),
            "duplicate UFVK import must fail: {second:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_storage_in_non_private_directory() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().expect("temporary directory");
        let public_dir = temp.path().join("public");
        fs::create_dir(&public_dir).unwrap();
        fs::set_permissions(&public_dir, fs::Permissions::from_mode(0o755)).unwrap();

        let result = import_testnet_ufvk_view_only(
            public_dir.join("wallet.sqlite"),
            "qpc-view",
            &testnet_ufvk(),
            testnet_tree_state(),
        );

        assert!(result.is_err(), "non-private parent must be rejected");
    }
}
