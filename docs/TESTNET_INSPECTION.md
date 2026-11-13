# Phase 3: read-only network and authorization-bearing execution inspection

The router public index exports `TESTNET_NETWORKS`, `createTestnetAdapter`, `prepareTestnetTransfer`, `inspectTestnetNetwork`, `inspectTestnetTransaction`, `verifyTestnetSettlement` and `inspectZcashTestnet`. See the strict input shapes and executable fixtures in [adapter tests](../agent-router/test/testnet-adapters.test.mjs). Operator-owned mapping profiles must not come from an agent's request. No signer, wallet, transaction submission or broadcast method exists in this module.

Run the read-only endpoint identity check:

```bash
node agent-router/scripts/testnet-inspect.mjs
```

The command compares native chain/genesis identity at fixed NEAR testnet, Ethereum Sepolia, BSC testnet and Solana devnet HTTPS endpoints. Zcash inspection requires an operator's local testnet RPC node; no public shielded-proof service or credentials are assumed. It exits nonzero when any identity check cannot be verified. A network identity observation does not prove a transaction exists.

`prepareTestnetTransfer` delegates exact recipient, native asset/decimals, amount, fee budget, action and mapping validation to the existing native preparation module, constrained to testnets. Output is unsigned preparation data, not native wire bytes or approval to execute. Zcash shielded preparation remains `NOT_IMPLEMENTED` without validated Rust parser/wallet integration; there is no transparent fallback.

`inspectTestnetTransaction` independently reads native RPC fields against its local, immutable preparation. It does not accept caller-provided `canonical`, `success`, `finalized`, evidence or verifier callbacks as proof. Supported observations inspect:

- EVM chain identity, transaction/receipt hashes and fields, exact native recipient/value/empty calldata, gas fee, successful receipt, canonical block hash and finalized head; only types 0/1/2, no blob or delegation actions.
- NEAR signer, receiver, exact Transfer action/deposit, transaction and receipt success, inclusion and final block evidence.
- Solana pinned devnet genesis, signature status and finalized/error fields, exact signer/accounts/System Program transfer/lamports, fee, block and slot evidence.

Malformed, missing, mismatched, failed and noncanonical responses fail closed as `NOT_VERIFIED`. These are narrow native transfer observations, not trustless consensus proofs: RPC operators, endpoint availability, JSON precision and chain-specific proof limitations remain relevant. Adversarial tests use synthetic RPC responses and never claim live transaction execution.

**Plain native transfers never settle a QPC intent.** A native transfer alone does not prove execution under the full QPC envelope, including authorization identity, nonce/replay domain, policy version, expiry and revocation. Even a matching native transfer observation therefore produces `NOT_IMPLEMENTED`, not `CONFIRMED`, `FINALIZED` or `SETTLED` QPC intent state. The separate owned preparations from `prepareNearExecution`, `prepareEvmExecution` and `prepareSolanaExecution` target the new authorization-bearing executors. Their verification requires exact native transaction/action/receipt linkage, historical operator-pinned code and owner, recorded authorization/execution, canonical finality and actual fees. Solana additionally requires immutable loader code predating execution and a canonical owner-signed approval found after that deployment; missing/pruned history or ambiguous code never settles. Only these paths can return a fresh module-owned verification receipt, accepted by `FileIntentStore.settleNative` with its own pinned `nativeTrust` configuration. This is fixture-tested integration under explicit RPC/upgrade-authority trust, not live executor payment evidence. See [local execution](LOCAL_EXECUTION.md). Zcash shielded recipient/amount/nullifier/witness verification is not implemented and cannot be inferred from a transaction hash or provider success.

See [current engineering evidence](ENGINEERING_EVIDENCE.md) for exact test results, live identity observations and delivery blockers, and [security boundaries](../SECURITY.md) before any production integration.
