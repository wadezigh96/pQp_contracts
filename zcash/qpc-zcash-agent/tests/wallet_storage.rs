use qpc_zcash_agent::TestnetWallet;
#[test] fn opens_distinct_testnet_wallet_and_cache(){
 let dir=tempfile::tempdir().unwrap();let w=dir.path().join("wallet.sqlite");let c=dir.path().join("cache.sqlite");
 let result=TestnetWallet::open(&w,&c);assert!(result.is_ok(),"{result:?}");assert!(w.exists());assert!(c.exists());
}
#[test] fn rejects_same_database_path(){
 let dir=tempfile::tempdir().unwrap();let p=dir.path().join("same.sqlite");
 assert!(matches!(TestnetWallet::open(&p,&p),Err(qpc_zcash_agent::WalletStorageError::SamePath)));
}
