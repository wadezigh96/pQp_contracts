# Phase 2 authorization lab — SIMULATED

An offline, deterministic dashboard using the shared literal authorization fixture and actual router validation gates. No dependencies beyond Node.js and Python 3 for optional static serving.

```bash
# Run from repository root. Regenerates the checked-in static dashboard/report.
node agent-router/demo/simulate.mjs
node --test agent-router/test/phase2-demo.test.mjs

# Optional alternate output directory (same bytes, isolated replay storage).
node agent-router/demo/simulate.mjs /tmp/qpc-phase2-demo

# View the checked-in or regenerated dashboard.
python3 -m http.server 4382 --bind 0.0.0.0 --directory agent-router/demo/generated
# Open http://localhost:4382/
```

The fixed clock comes from `docs/fixtures/cross-l1-v1.json`, not the wall clock. Each run starts a fresh private temporary `FileIntentStore` directory and deletes it afterward. Within the run, authorization consumes replay keys atomically and the replay attempt reopens persisted state through a new store instance. HTML and JSON output contain no temporary paths, random IDs, or generation timestamps. The generator fails if any expected gate outcome changes. Repeated output is byte-identical.

## Eleven scenarios

- Unauthorized submission reservation is rejected from `CREATED`.
- The exact fixture is authorized using an explicitly mocked commitment allowlist.
- Replaying the same nonce/intent is rejected from reopened durable local state.
- Authorization at the exact expiry boundary is rejected before identity verification.
- A changed envelope fails the mock identity allowlist.
- Wrong destination chain, recipient, and amount each fail real evidence-binding checks.
- Structurally valid fabricated evidence fails the explicit mock evidence callback.
- Matching synthetic evidence passes fixture consistency checks only; displayed as `MOCK_VALIDATED_NOT_SETTLED`.
- Native settlement remains blocked by the real router's fail-closed settlement gate.

## What this proves, and what it does not

The implementation calls `FileIntentStore`, `verifyCrossL1SettlementEvidence`, `buildNearEnvelopeAuthorizationDispatch`, and `settleNativeTransfer`; it does not reproduce their validation logic in the UI. The unsigned NEAR dispatch is prepared but never sent. The final persisted state is `AUTHORIZED`, never `CONFIRMED` or `SETTLED`.

Agent authentication is **mocked** by equality with the known fixture commitment, not a signature or on-chain authorization. Destination verification is **mocked** by equality with synthetic evidence, not an RPC response, proof verifier, or chain observation. Synthetic transaction/inclusion references explicitly say `SIMULATED-NOT-A-TRANSACTION` and `SIMULATED-NOT-A-BLOCK`. Synthetic `FINALIZED` is an input field, not observed finality. Generic evidence success is never promoted to verified production settlement.

There is no wallet, key, real transaction, external request, price source, deployed contract, cross-chain asset movement, or live connectivity. The local store is a prototype, not production replay protection across hosts or disk loss. The generated dashboard is static and has no client scripts, external assets, or analytics; opening/closing scenario details uses native HTML.

## Screenshot

After generating and serving, open `/` in a browser, expand any scenario with its native summary control, and capture the page. The `SIMULATED` banner, trust-boundary panel, and zero-real-transaction/settlement counters must remain visible in evidence. For a narrow viewport, the same content stacks without horizontal scrolling. No browser package is required by the generator or tests.
