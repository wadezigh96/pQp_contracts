//! Testnet-only, view-only UFVK import boundary.
//!
//! This module imports a Unified Full Viewing Key into the official SQLite
//! wallet database using AccountPurpose::ViewOnly. It never accepts a seed or
//! spending key and does not construct, sign, or broadcast transactions.
//!
//! Import is intentionally separate from synchronization: a successful import
//! does not mean the wallet has scanned any blocks or verified a payment.

use std::path::Path;

use rand_core::OsRng;
use zcash_client_backend::keys::UnifiedFullViewingKey;
use zcash_client_backend::{
    data_api::{AccountBirthday, AccountPurpose, WalletWrite},
    proto::service::TreeState,
};
use zcash_client_sqlite::{
    util::SystemClock,
    wallet::{init::WalletMigrator, WalletDb},
};
use zcash_client_backend::keys::UnifiedFullViewingKey;
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
        || !account_name
            .bytes()
            .all(|b| b.is_ascii_graphic() || b == b' ')
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
        let state = TreeState {
            network: "test".to_owned(),
            ..Default::default()
        };

        let result = import_testnet_ufvk_view_only(
            "/path/that/must/not/be/touched/wallet.sqlite",
            "../qpc",
            "not-a-ufvk",
            state,
        );

        assert_eq!(result, Err("view-only account name is invalid".to_owned()));
    }
}
