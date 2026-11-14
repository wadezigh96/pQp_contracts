//! Regtest-only view wallet account lifecycle. Tests use local fixtures only.

#[cfg(test)]
mod tests {
    use super::*;
    use zcash_protocol::consensus::BlockHeight;

    pub(super) fn regtest_params() -> zcash_protocol::local_consensus::LocalNetwork {
        zcash_protocol::local_consensus::LocalNetwork {
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
    fn imports_reopens_and_derives_regtest_shielded_address() {
        let temp = tempfile::tempdir().unwrap();
        let private = temp.path().join("private");
        std::fs::create_dir(&private).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let params = regtest_params();
        let (ufvk, address) = test_regtest_ufvk_and_address();
        let path = private.join("wallet.sqlite");
        let created =
            import_regtest_view_account(&path, params, "qpc-regtest", &ufvk, test_birthday(1))
                .expect("import disposable Regtest viewing account");
        assert_eq!(created.address, address);
        assert!(created.address.starts_with("uregtest"));
        reopen_regtest_view_account(&path, params, "qpc-regtest").expect("reopen account");
    }

    #[test]
    fn rejects_wrong_network_ufvk_without_leaking_it() {
        let (_, testnet_ufvk) = testnet_fixture_ufvk();
        let temp = tempfile::tempdir().unwrap();
        let private = temp.path().join("private");
        std::fs::create_dir(&private).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let result = import_regtest_view_account(
            private.join("wallet.sqlite"),
            regtest_params(),
            "qpc-regtest",
            &testnet_ufvk,
            test_birthday(1),
        );
        assert!(matches!(result, Err(RegtestWalletError::WrongNetwork)));
        assert!(!format!("{result:?}").contains(&testnet_ufvk));
    }

    #[test]
    fn rejects_birthday_before_regtest_sapling_activation() {
        let (ufvk, _) = test_regtest_ufvk_and_address();
        let temp = tempfile::tempdir().unwrap();
        let private = temp.path().join("private");
        std::fs::create_dir(&private).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let result = import_regtest_view_account(
            private.join("wallet.sqlite"),
            regtest_params(),
            "qpc-regtest",
            &ufvk,
            test_birthday(0),
        );
        assert!(matches!(
            result,
            Err(RegtestWalletError::BirthdayBeforeActivation)
        ));
    }

    #[test]
    fn refuses_corrupt_database_without_replacing_it() {
        let temp = tempfile::tempdir().unwrap();
        let private = temp.path().join("private");
        std::fs::create_dir(&private).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let path = private.join("wallet.sqlite");
        std::fs::write(&path, b"corrupt").unwrap();
        let before = std::fs::read(&path).unwrap();
        let (ufvk, _) = test_regtest_ufvk_and_address();
        assert!(import_regtest_view_account(
            &path,
            regtest_params(),
            "qpc-regtest",
            &ufvk,
            test_birthday(1),
        )
        .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}

use std::path::Path;

use rand_core::OsRng;
use zcash_client_backend::data_api::{
    Account, AccountBirthday, AccountPurpose, WalletRead, WalletWrite,
};
use zcash_client_backend::keys::UnifiedFullViewingKey;
use zcash_client_backend::proto::service::TreeState;
use zcash_client_sqlite::{util::SystemClock, wallet::init::WalletMigrator, WalletDb};
use zcash_keys::keys::UnifiedAddressRequest;
use zcash_protocol::{
    consensus::{NetworkType, Parameters},
    local_consensus::LocalNetwork,
};

use crate::wallet::{harden_sqlite_sidecars, prepare_database_file, restrict_regular_file};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegtestWalletError {
    WrongNetwork,
    InvalidAccountName,
    InvalidBirthday,
    BirthdayBeforeActivation,
    Database,
    AccountNotFound,
}

/// Public account information contains only a shielded address and no key material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegtestViewAccount {
    pub address: String,
    pub birthday_height: u32,
}

fn validated_birthday(
    params: &LocalNetwork,
    prior_state: TreeState,
) -> Result<AccountBirthday, RegtestWalletError> {
    if prior_state.network != "regtest" {
        return Err(RegtestWalletError::WrongNetwork);
    }
    let birthday_height = prior_state
        .height
        .checked_add(1)
        .ok_or(RegtestWalletError::InvalidBirthday)?;
    let sapling_activation = params
        .activation_height(zcash_protocol::consensus::NetworkUpgrade::Sapling)
        .map(u64::from);
    if sapling_activation.is_some_and(|activation| birthday_height < activation) {
        return Err(RegtestWalletError::BirthdayBeforeActivation);
    }
    AccountBirthday::from_treestate(prior_state, None)
        .map_err(|_| RegtestWalletError::InvalidBirthday)
}

/// Imports an externally provisioned UFVK into an isolated Regtest viewing wallet.
/// The wallet database stores viewing material, never a seed or spending key. The
/// caller must obtain `encoded_ufvk` through its own protected key-management path.
pub fn import_regtest_view_account(
    database_path: impl AsRef<Path>,
    params: LocalNetwork,
    account_name: &str,
    encoded_ufvk: &str,
    prior_state: TreeState,
) -> Result<RegtestViewAccount, RegtestWalletError> {
    if params.network_type() != NetworkType::Regtest {
        return Err(RegtestWalletError::WrongNetwork);
    }
    validate_account_name(account_name)?;
    let birthday = validated_birthday(&params, prior_state)?;
    let birthday_height = u32::from(birthday.height());
    let ufvk = UnifiedFullViewingKey::decode(&params, encoded_ufvk)
        .map_err(|_| RegtestWalletError::WrongNetwork)?;
    let (address, _) = ufvk
        .default_address(UnifiedAddressRequest::AllAvailableKeys)
        .map_err(|_| RegtestWalletError::Database)?;
    let encoded_address = address.encode(&params);
    let path = database_path.as_ref();
    prepare_database_file(path).map_err(|_| RegtestWalletError::Database)?;
    let mut db = WalletDb::for_path(path, params, SystemClock, OsRng)
        .map_err(|_| RegtestWalletError::Database)?;
    WalletMigrator::new()
        .init_or_migrate(&mut db)
        .map_err(|_| RegtestWalletError::Database)?;
    db.import_account_ufvk(
        account_name,
        &ufvk,
        &birthday,
        AccountPurpose::ViewOnly,
        Some("qpc-regtest-view-only"),
    )
    .map_err(|_| RegtestWalletError::Database)?;
    drop(db);
    restrict_regular_file(path).map_err(|_| RegtestWalletError::Database)?;
    harden_sqlite_sidecars(path).map_err(|_| RegtestWalletError::Database)?;
    Ok(RegtestViewAccount {
        address: encoded_address,
        birthday_height,
    })
}

/// Reopens a tracked account by its UFVK-free stable name and verifies database readability.
pub fn reopen_regtest_view_account(
    database_path: impl AsRef<Path>,
    params: LocalNetwork,
    account_name: &str,
) -> Result<(), RegtestWalletError> {
    if params.network_type() != NetworkType::Regtest {
        return Err(RegtestWalletError::WrongNetwork);
    }
    validate_account_name(account_name)?;
    let path = database_path.as_ref();
    restrict_regular_file(path).map_err(|_| RegtestWalletError::Database)?;
    let mut db = WalletDb::for_path(path, params, SystemClock, OsRng)
        .map_err(|_| RegtestWalletError::Database)?;
    WalletMigrator::new()
        .init_or_migrate(&mut db)
        .map_err(|_| RegtestWalletError::Database)?;
    let ids = db
        .get_account_ids()
        .map_err(|_| RegtestWalletError::Database)?;
    for id in ids {
        if let Some(account) = db
            .get_account(id)
            .map_err(|_| RegtestWalletError::Database)?
        {
            if account.name() == Some(account_name) {
                return Ok(());
            }
        }
    }
    Err(RegtestWalletError::AccountNotFound)
}

fn validate_account_name(name: &str) -> Result<(), RegtestWalletError> {
    if name.is_empty()
        || name.len() > 64
        || name.starts_with(' ')
        || name.ends_with(' ')
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b' ' | b'-' | b'_'))
    {
        Err(RegtestWalletError::InvalidAccountName)
    } else {
        Ok(())
    }
}

#[cfg(test)]
fn test_birthday(height: u32) -> TreeState {
    TreeState {
        network: "regtest".into(),
        height: u64::from(height),
        hash: "00".repeat(32),
        time: 1,
        sapling_tree: String::new(),
        orchard_tree: String::new(),
        ironwood_tree: String::new(),
    }
}

#[cfg(test)]
fn test_regtest_ufvk_and_address() -> (String, String) {
    use zcash_keys::keys::UnifiedSpendingKey;
    let params = tests::regtest_params();
    let seed = [0x37u8; 32];
    let usk = UnifiedSpendingKey::from_seed(&params, &seed, zip32::AccountId::ZERO).unwrap();
    let ufvk = usk.to_unified_full_viewing_key();
    let address = ufvk
        .default_address(UnifiedAddressRequest::AllAvailableKeys)
        .unwrap()
        .0
        .encode(&params);
    (ufvk.encode(&params), address)
}

#[cfg(test)]
fn testnet_fixture_ufvk() -> (String, String) {
    use zcash_keys::keys::UnifiedSpendingKey;
    let seed = [0x38u8; 32];
    let usk = UnifiedSpendingKey::from_seed(
        &zcash_protocol::consensus::Network::TestNetwork,
        &seed,
        zip32::AccountId::ZERO,
    )
    .unwrap();
    let ufvk = usk
        .to_unified_full_viewing_key()
        .encode(&zcash_protocol::consensus::Network::TestNetwork);
    (String::new(), ufvk)
}
