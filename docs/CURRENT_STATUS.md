# Current status on branch reason/deliver-qpc-hardening-dr-d997d6a3

Verified locally on 2026-11-14:

- `node --test agent-router/test/*.test.mjs`: **516/516 PASS**
- `npm test --prefix agent-router/pq-auth`: **26/26 PASS**
- `cargo test --manifest-path near/qpc-near-agent-finance/Cargo.toml --locked`: **27/27 PASS**
- `cargo test --manifest-path zcash/qpc-zcash-agent/Cargo.toml --locked`: **46/46 PASS**

These match the counts reported in the PR description. Older numbers appearing in historical sections of ENGINEERING_EVIDENCE.md (499 JS / 21 NEAR, 516 JS / 24 NEAR, etc.) are retained as historical checkpoints and do not reflect the current tree.

## Remaining work (from review comment)

| Item | Current status | Notes |
|------|----------------|-------|
| NEAR sandbox integration | PASS (reported + verified offline VM) | Offline WASM VM + host tests pass; not full live sandbox deployment |
| EVM Anvil integration | PASS (reported) | Foundry tests exist; Anvil-specific not re-run here |
| Zcash shielded payment | NOT IMPLEMENTED | Address/policy/wallet-storage/read-only probe only. No tx construction, signing, or broadcast. Keys stay outside agent boundary by design. |
| ML-DSA hybrid authentication | PROTOTYPE | Optional off-chain Ed25519 + ML-DSA-65 verifier in agent-router/pq-auth; 26 tests pass. Not enforced on-chain. |
| ML-KEM private transport | NOT ACTIVE | Not implemented. ML-KEM is key establishment, not a signature. |
| Solana SBF/validator | PARTIAL | Host tests + official SBF build + unsigned LiteSVM CPI/rollback tests exist. Not full validator/consensus or live cluster. |
| External professional audit | NOT DONE | Required before any production funds. Model reviews are not a substitute. |
| Legacy NEAR migration | NOT COMPLETE | Current layout is for fresh initialization. No reviewed migration of prior state. |

No production readiness, live payments, or quantum-resistance claims are made.
