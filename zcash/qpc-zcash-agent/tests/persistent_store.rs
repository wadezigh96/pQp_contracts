use qpc_zcash_agent::{
    PaymentId, PaymentIntent, PersistentPaymentStore, PersistentStoreError, SourceChain,
};

fn intent(id: u8, chain: SourceChain, agent: [u8; 32], nonce: u64) -> PaymentIntent {
    PaymentIntent {
        id: PaymentId::new([id; 32]),
        source_chain: chain,
        source_agent_id: agent,
        amount_zatoshis: 25_000,
        nonce,
        expires_at_unix: 2_000,
    }
}

#[test]
fn nonce_replay_rejected_after_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("replay.sqlite");
    {
        let mut store = PersistentPaymentStore::open(&path).unwrap();
        store.register(&intent(1, SourceChain::Near, [7; 32], 44), 1_000).unwrap();
    }
    let mut store = PersistentPaymentStore::open(path).unwrap();
    assert!(matches!(
        store.register(&intent(2, SourceChain::Near, [7; 32], 44), 1_000),
        Err(PersistentStoreError::DuplicateNonce)
    ));
}

#[test]
fn nonce_scope_and_full_u64_range_include_zcash_source() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = PersistentPaymentStore::open(dir.path().join("replay.sqlite")).unwrap();
    store.register(&intent(1, SourceChain::Zcash, [7; 32], u64::MAX), 1_000).unwrap();
    store.register(&intent(2, SourceChain::Near, [7; 32], u64::MAX), 1_000).unwrap();
    assert!(store.contains_nonce(SourceChain::Zcash, [7; 32], u64::MAX).unwrap());
    assert!(store.contains_nonce(SourceChain::Near, [7; 32], u64::MAX).unwrap());
    assert!(!store.contains_nonce(SourceChain::Ethereum, [7; 32], u64::MAX).unwrap());
}
