//! Zcash SQLite light-client storage and explicit Testnet sync boundary.
//! The wallet DB is only mutated through zcash_client_sqlite APIs. No seed/key import.
use std::path::Path;
use rand_core::OsRng;
use tonic::transport::Endpoint;
use zcash_client_backend::{proto::service::compact_tx_streamer_client::CompactTxStreamerClient, util::SystemClock};
use zcash_client_sqlite::{chain::init::init_cache_database, wallet::init::init_wallet_db, BlockDb, WalletDb};
use zcash_protocol::consensus::Network;

type TestnetDb = WalletDb<rusqlite::Connection, Network, SystemClock, OsRng>;

pub struct TestnetWallet { wallet: TestnetDb, cache: BlockDb }

#[derive(Debug)]
pub enum WalletStorageError { Wallet(String), Cache(String), SamePath, Endpoint(String), Transport(String), Sync(String), InvalidBatch }
impl core::fmt::Display for WalletStorageError {
 fn fmt(&self,f:&mut core::fmt::Formatter<'_>)->core::fmt::Result {
  match self {
   Self::Wallet(e)=>write!(f,"wallet DB: {e}"), Self::Cache(e)=>write!(f,"cache DB: {e}"),
   Self::SamePath=>f.write_str("wallet and cache paths must differ"),
   Self::Endpoint(e)=>write!(f,"endpoint: {e}"), Self::Transport(e)=>write!(f,"transport: {e}"),
   Self::Sync(e)=>write!(f,"sync: {e}"), Self::InvalidBatch=>f.write_str("batch size must be > 0")
  }
 }
}
impl std::error::Error for WalletStorageError {}

#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub struct WalletCapabilities { pub orchard: bool, pub sapling: bool }
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub enum WalletObservation { Unknown, Pending, Confirmed{confirmations:u32,block_height:u32}, Reorged }
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub enum WalletError { Unavailable, SyncIncomplete, InternalFailure }
pub trait WalletBackend {
 fn capabilities(&self)->Result<WalletCapabilities,WalletError>;
 fn observe_payment(&self,id:crate::PaymentId)->Result<WalletObservation,WalletError>;
}

impl TestnetWallet {
 /// Initializes a blank Testnet wallet and a separate compact-block cache.
 /// Existing seed-required migrations fail closed; this method never imports keys.
 pub fn open(wallet_path:impl AsRef<Path>,cache_path:impl AsRef<Path>)->Result<Self,WalletStorageError>{
  if wallet_path.as_ref()==cache_path.as_ref(){return Err(WalletStorageError::SamePath);}
  let mut wallet=WalletDb::for_path(wallet_path,Network::TestNetwork,SystemClock,OsRng)
   .map_err(|e|WalletStorageError::Wallet(e.to_string()))?;
  init_wallet_db(&mut wallet,None).map_err(|e|WalletStorageError::Wallet(e.to_string()))?;
  let cache=BlockDb::for_path(cache_path).map_err(|e|WalletStorageError::Cache(e.to_string()))?;
  init_cache_database(&cache).map_err(|e|WalletStorageError::Cache(e.to_string()))?;
  Ok(Self{wallet,cache})
 }

 /// Runs the official librustzcash light-client sync against HTTPS lightwalletd.
 /// This is not independent consensus verification; the server remains trusted.
 pub async fn sync_testnet(&mut self,url:&str,batch_size:u32)->Result<(),WalletStorageError>{
  if batch_size==0{return Err(WalletStorageError::InvalidBatch);}
  if !url.starts_with("https://"){return Err(WalletStorageError::Endpoint("HTTPS required".into()));}
  let endpoint=Endpoint::from_shared(url.to_owned()).map_err(|e|WalletStorageError::Endpoint(e.to_string()))?;
  let channel=endpoint.connect_timeout(std::time::Duration::from_secs(15)).connect().await
   .map_err(|e|WalletStorageError::Transport(e.to_string()))?;
  let mut client=CompactTxStreamerClient::new(channel);
  zcash_client_backend::sync::run(&mut client,&Network::TestNetwork,&self.cache,&mut self.wallet,batch_size)
   .await.map_err(|e|WalletStorageError::Sync(format!("{e:?}")))
 }
}
