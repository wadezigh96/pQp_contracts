use qpc_zcash_agent::TestnetWallet;

#[test]
fn opens_distinct_testnet_wallet_and_cache() {
    let dir = tempfile::tempdir().unwrap();
    let wallet_path = dir.path().join("wallet.sqlite");
    let cache_path = dir.path().join("compact-block-cache.sqlite");

    let result = TestnetWallet::open(&wallet_path, &cache_path);
    assert!(result.is_ok());
    assert!(wallet_path.exists());
    assert!(cache_path.exists());
}

#[test]
fn rejects_same_database_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("same.sqlite");

    assert!(matches!(
        TestnetWallet::open(&path, &path),
        Err(qpc_zcash_agent::WalletStorageError::SamePath)
    ));
}
