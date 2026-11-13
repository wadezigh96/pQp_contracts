use qpc_zcash_agent::{PaymentId,PaymentIntent,PersistentPaymentStore,PersistentStoreError,SourceChain};
fn intent(id:u8,agent:[u8;32],nonce:u64)->PaymentIntent{PaymentIntent{id:PaymentId::new([id;32]),source_chain:SourceChain::Near,source_agent_id:agent,amount_zatoshis:25000,nonce,expires_at_unix:2000}}
#[test] fn nonce_replay_rejected_after_reopen(){
 let dir=tempfile::tempdir().unwrap();let path=dir.path().join("replay.sqlite");
 {let mut s=PersistentPaymentStore::open(&path).unwrap();s.register(&intent(1,[7;32],44),1000).unwrap();}
 let mut s=PersistentPaymentStore::open(path).unwrap();
 assert!(matches!(s.register(&intent(2,[7;32],44),1000),Err(PersistentStoreError::DuplicateNonce)));
}
#[test] fn nonce_scope_and_full_u64_range(){
 let dir=tempfile::tempdir().unwrap();let mut s=PersistentPaymentStore::open(dir.path().join("replay.sqlite")).unwrap();
 s.register(&intent(1,[7;32],u64::MAX),1000).unwrap();s.register(&intent(2,[8;32],u64::MAX),1000).unwrap();
 assert!(s.contains_nonce(SourceChain::Near,[7;32],u64::MAX).unwrap());
 assert!(!s.contains_nonce(SourceChain::Ethereum,[7;32],u64::MAX).unwrap());
}
