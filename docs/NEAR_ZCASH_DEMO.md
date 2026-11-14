# NEAR ↔ Zcash private-agent demo status

## Result

**Overall: BLOCKED / NOT RUN.** No local NEAR Sandbox ↔ Zcash regtest shielded payment has been executed. No transaction was broadcast, no wallet synchronized, no receipt/confirmation was verified, and no transaction IDs are available. The offline VM, unit-test callback fixtures, and simulated authorization are not network settlement.

## Exact blockers

1. This environment has `cargo` and `rustc`, but lacks `near-sandbox`, `zcashd`, `zebrad`, `lightwalletd`, and Docker. The requested `near/sandbox-tests/` harness is also absent.
2. The Regtest view-only wallet account and compact-block note synchronization adapter are implemented, but have not yet passed a live integration run. The repository still lacks the outgoing Zcash transaction builder, external signer, and broadcaster; the router explicitly reports `NOT_IMPLEMENTED` for native shielded settlement.
3. The NEAR CI offline VM uses mocked host context and promises; it is not a nearcore Sandbox with real local accounts, deployed WASM, or a receipt scheduler.

Installing node binaries alone would not complete the missing payment implementation. No keys or funds should be supplied to work around these blockers.

## Scenario status

| Scenario | Status | Evidence required before it can be called PASS |
|---|---|---|
| NEAR → Zcash shielded | **BLOCKED** | Local Sandbox authorization plus actual Zcash regtest shielded broadcast, real txid, receiving-wallet sync, and verified confirmations. |
| Zcash → NEAR | **BLOCKED** | Actual regtest shielded event/txid and note receipt, independent network/amount/recipient/finality checks, then an actual Sandbox operation and resulting local transaction receipt. |
| Invalid recipient, replay, wrong network, expiry | **UNIT TESTED IN PART** | Existing suites cover selected policy/replay boundaries; not the full end-to-end failure matrix. |
| Restart/recovery/refund after partial transfer | **NOT IMPLEMENTED END TO END** | A real, deterministic local backend, durable state transitions, idempotent recovery, and explicit result evidence. |

## Reproducible checks (not a payment demo)

```bash
node --test agent-router/test/*.test.mjs
npm ci --prefix agent-router/pq-auth --ignore-scripts --no-audit --no-fund
npm test --prefix agent-router/pq-auth
cargo test --manifest-path near/qpc-near-agent-finance/Cargo.toml --locked
cargo test --manifest-path zcash/qpc-zcash-agent/Cargo.toml --locked
```

The Rust commands require Cargo. The JavaScript tests create local fixtures and mocks only. Use `scripts/preflight-near-zcash.sh` to generate a machine-readable preflight result; it does not start nodes, use network RPC, open a wallet, or send transactions. It is read-only with respect to external network/wallet/chain state and writes the report under `test-artifacts/`.

## Safety requirements for future work

Use only local Sandbox and Zcash regtest with disposable accounts/wallets. Never use Mainnet, real funds, public Testnet deployment, Owner keys, FullAccess agent/router accounts, or production wallet material. A genuine test result must separately identify authorization, signed transaction, broadcast, network inclusion, confirmations, and recipient-wallet receipt. Do not call a transaction “settled” based on a mock callback, provider response, txid-shaped string, or offline-VM fixture.
