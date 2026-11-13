//! Fail-closed Zcash adapter. No key custody, signing, broadcast, routing or settlement yet.
use zcash_address::unified::{self, Encoding};
use zcash_protocol::{consensus::NetworkType, PoolType};
pub mod payment;
pub mod persistence;
pub mod privacy_policy;
pub mod wallet;
pub use payment::{IntentError,PaymentId,PaymentIntent,PaymentReceipt,PaymentState,PaymentTracker,SourceChain};
pub use persistence::{PersistentPaymentStore,PersistentStoreError};
pub use privacy_policy::{PaymentMemo,PrivacyPolicy,PrivacyPolicyError,PrivateRecipient};
pub use wallet::{TestnetWallet,WalletBackend,WalletCapabilities,WalletError,WalletObservation,WalletStorageError};
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub enum ZcashNetwork{Mainnet,Testnet}
impl ZcashNetwork{fn protocol_network(self)->NetworkType{match self{Self::Mainnet=>NetworkType::Main,Self::Testnet=>NetworkType::Test}}}
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub enum ShieldedReceiver{Orchard,Sapling}
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub struct ValidatedRecipient{pub network:ZcashNetwork,pub receiver:ShieldedReceiver}
#[derive(Clone,Copy,Debug,Eq,PartialEq)]
pub enum AddressPolicyError{InvalidUnifiedAddress,WrongNetwork,NoSupportedShieldedReceiver}
impl core::fmt::Display for AddressPolicyError{
 fn fmt(&self,f:&mut core::fmt::Formatter<'_>)->core::fmt::Result{
  f.write_str(match self{Self::InvalidUnifiedAddress=>"invalid canonical Unified Address",Self::WrongNetwork=>"wrong Zcash network",Self::NoSupportedShieldedReceiver=>"no supported shielded receiver"})
 }
}
impl std::error::Error for AddressPolicyError{}
pub fn validate_recipient(encoded:&str,expected:ZcashNetwork)->Result<ValidatedRecipient,AddressPolicyError>{
 let (network,address)=unified::Address::decode(encoded).map_err(|_|AddressPolicyError::InvalidUnifiedAddress)?;
 if network!=expected.protocol_network(){return Err(AddressPolicyError::WrongNetwork);}
 if address.encode(&network)!=encoded{return Err(AddressPolicyError::InvalidUnifiedAddress);}
 let receiver=if address.has_receiver_of_type(PoolType::ORCHARD){ShieldedReceiver::Orchard}
 else if address.has_receiver_of_type(PoolType::SAPLING){ShieldedReceiver::Sapling}
 else{return Err(AddressPolicyError::NoSupportedShieldedReceiver);};
 Ok(ValidatedRecipient{network:expected,receiver})
}
