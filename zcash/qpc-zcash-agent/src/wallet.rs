//! Safe initialization boundary for a local Zcash Testnet light-wallet database.
//!
//! This initializes storage only. It does not generate/import keys, create an
//! address, connect to a lightwalletd server, sync funds, sign, or broadcast.

use std::fs::{self, OpenOptions};
use std::path::Path;

use rand_core::OsRng;
use zcash_client_sqlite::{util::SystemClock, wallet::init::WalletMigrator, WalletDb};
use zcash_protocol::consensus::Network;

#[cfg(unix)]
fn validate_private_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::symlink_metadata(path)
        .map_err(|_| "wallet parent directory is unavailable".to_owned())?;
    let mode = metadata.permissions().mode();
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || mode & 0o077 != 0
        || mode & 0o700 != 0o700
    {
        return Err("wallet parent directory must be a private, non-symlink directory".into());
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| "wallet parent directory is unavailable".to_owned())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("wallet parent directory must be a non-symlink directory".into());
    }
    Ok(())
}

pub(crate) fn restrict_regular_file(path: &Path) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| "wallet database file is unavailable".to_owned())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("wallet database path must be a regular non-symlink file".into());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.nlink() != 1 {
            return Err("wallet database files with multiple hard links are forbidden".into());
        }
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions)
            .map_err(|_| "wallet database permissions could not be restricted".to_owned())?;
    }
    Ok(())
}

pub(crate) fn prepare_database_file(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    validate_private_directory(parent)?;

    match fs::symlink_metadata(path) {
        Ok(_) => restrict_regular_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options
                .open(path)
                .map_err(|_| "wallet database file could not be created safely".to_owned())?;
            restrict_regular_file(path)
        }
        Err(_) => Err("wallet database path could not be inspected".into()),
    }
}

fn restrict_existing_sidecar(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(_) => restrict_regular_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("wallet database sidecar could not be inspected".into()),
    }
}

pub(crate) fn harden_sqlite_sidecars(path: &Path) -> Result<(), String> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        restrict_existing_sidecar(Path::new(&sidecar))?;
    }
    Ok(())
}

/// Creates or migrates a local Testnet wallet database without importing a seed.
///
/// The parent directory must already exist and be private (0700 or stricter on
/// Unix). The database and any SQLite sidecars are restricted to owner access.
/// Existing symlinks, non-regular files, and multiply-linked files are rejected.
pub fn initialize_testnet_wallet_database(path: impl AsRef<Path>) -> Result<(), String> {
    let path = path.as_ref();
    prepare_database_file(path)?;

    let mut wallet_db = WalletDb::for_path(path, Network::TestNetwork, SystemClock, OsRng)
        .map_err(|_| "wallet database open failed".to_owned())?;

    WalletMigrator::new()
        .init_or_migrate(&mut wallet_db)
        .map_err(|_| "wallet database migration failed".to_owned())?;

    drop(wallet_db);
    restrict_regular_file(path)?;
    harden_sqlite_sidecars(path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn initializes_testnet_storage_without_seed_or_network_activity() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let private = private_test_directory(&temp, "private-wallet");
        let db_path = private.join("qpc-zcash-testnet.sqlite");

        initialize_testnet_wallet_database(&db_path).expect("initialize local wallet DB");

        assert!(db_path.exists());
        assert!(fs::metadata(&db_path).expect("wallet DB metadata").len() > 0);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&db_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn does_not_overwrite_existing_wallet_database() {
        let temp = tempfile::tempdir().expect("temporary directory");
        let private = private_test_directory(&temp, "private-wallet");
        let db_path = private.join("qpc-zcash-testnet.sqlite");

        initialize_testnet_wallet_database(&db_path).expect("first initialization");
        let before = fs::metadata(&db_path).expect("metadata").len();

        initialize_testnet_wallet_database(&db_path).expect("idempotent migration");
        let after = fs::metadata(&db_path).expect("metadata").len();

        assert_eq!(before, after);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symlink_wallet_database_path() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().expect("temporary directory");
        let target = temp.path().join("real.sqlite");
        fs::write(&target, b"not a wallet database").unwrap();
        let link = temp.path().join("linked.sqlite");
        symlink(&target, &link).unwrap();

        assert!(initialize_testnet_wallet_database(&link).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_non_private_parent_directory() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().expect("temporary directory");
        let public_dir = temp.path().join("public");
        fs::create_dir(&public_dir).unwrap();
        fs::set_permissions(&public_dir, fs::Permissions::from_mode(0o755)).unwrap();

        assert!(initialize_testnet_wallet_database(public_dir.join("wallet.sqlite")).is_err());
    }
}
