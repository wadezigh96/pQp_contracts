# QPC Audit Package

**Status:** Prepared for independent review. **Not** an external professional audit.
No production readiness, quantum-resistance, or live-payment claims are made.

## 1. System architecture and trust boundaries

- **Owner** holds root authority (policy configuration, pause/revoke, ownership transfer on EVM).
- **Agent** receives limited, time-bound, amount-capped authority only.
- **Contract / adapter** enforces policy deterministically. Agent logic is never a security boundary.
- Separate components:
  - EVM: `QPCAgentFinance.sol` (policy + exact calldata hash) + `QPCNativeTestnetExecutor.sol` (owner-attested testnet executor).
  - NEAR: `near/qpc-near-agent-finance` — authorization recording + narrow same-chain native execution/refund prototype. State layout version 1; no automatic migration.
  - Solana: `solana/qpc-native-executor` — owner-attested program, host tests + official SBF + unsigned LiteSVM.
  - Zcash: `zcash/qpc-zcash-agent` — Unified Address parsing, wallet storage init, replay, read-only lightwalletd probe. No transaction construction or signing.
  - Router: `agent-router` — unsigned preparation, local intent store, optional off-chain Ed25519 + ML-DSA-65 hybrid verifier.
- Private keys and spending keys remain outside the agent process by design.

## 2. Threat model and attack trees (summary)

Adversary can:
- Control agent/model output, steal an agent key, alter a request after preview.
- Replay intents, substitute recipient/amount/network, forge RPC responses.
- Trigger reorgs, timeouts, partial failures.
- Compromise the local host (filesystem store does not defend against malicious local writers).

Key attack trees (see `SECURITY.md` for full model):
- Replay across agents/chains/policy versions.
- Parameter substitution (recipient, amount, calldata).
- Callback / evidence forgery promoting settlement.
- State layout confusion / silent reset on NEAR.
- Downgrade or key substitution in hybrid authentication.
- Transparent fallback on Zcash routes.

## 3. Security invariants and traceability

| Invariant | Enforced by | Test evidence |
|-----------|-------------|---------------|
| Contract initializes only once, only when the predecessor equals the pinned owner `wkams.testnet` | `env::state_exists()`, predecessor/owner equality, and owner pin in `new` | `initialized_owner_is_exactly_the_intended_testnet_account`, `initializer_rejects_owner_substitution_before_state_creation`, `initializer_rejects_unauthorized_predecessor_even_with_intended_owner_argument`, `initializer_is_one_time_even_when_reinitialized_with_intended_owner` |
| Agent never becomes owner | Contract access control | Foundry + NEAR unit tests |
| Exact calldata hash required for EVM calls | `QPCAgentFinance` | Foundry security tests |
| NEAR predecessor (not claimed identity) authenticates | `authorize_envelope` | NEAR unit tests |
| Unversioned / wrong-version NEAR state rejected | `StateLayout` magic + version | `incompatible_unversioned_state_is_rejected...`, `versioned_state_roundtrips...` |
| Unsupported NEAR state layouts reject before collection access; no migration entrypoint exists | `StateLayout` magic/version deserialization guard | `incompatible_unversioned_state_is_rejected_without_storage_changes`, `versioned_state_roundtrips_and_rejects_unknown_corrupt_or_truncated_headers`, `existing_incompatible_state_cannot_be_reset_by_new` |
| Administrative calls cannot be made by agent/attacker | `assert_owner` on configuration methods | `non_owner_cannot_administer_even_when_claiming_owner_in_request_metadata`, `owner_only_configuration_and_recipient_changes` |
| Generic evidence cannot settle | `FileIntentStore` + verifiers | JS settlement-boundary tests |
| Replay domain isolates source adapters | `computeCrossL1ReplayDomainKey` V2 includes `source.adapterId` | Cross-L1 replay-domain test; legacy persisted V1 replay-key migration is not implemented |
| Zcash route cannot produce settlement | `verifyNearToZcashRoute` returns `NOT_IMPLEMENTED` | `zcash-route.test.mjs` |
| Hybrid auth requires both Ed25519 and ML-DSA-65 | `agent-router/pq-auth` | 26 hybrid tests |
| Solana replay / CPI constraints | Program + LiteSVM | Host + runtime tests |

## 4. Cross-chain failure and recovery matrix

| Scenario | Current behavior | Recovery path |
|----------|------------------|---------------|
| Ambiguous submission | Enter `RECONCILING` | Manual reconciliation; no auto-retry |
| Failed NEAR native transfer | Replay consumed; refund state machine | Agent-authorized refund only |
| Callback true but no native proof | `NOT_VERIFIED` / rejected | Fail closed |
| Expired / revoked intent | Rejected | Cannot reopen nonce |
| Incompatible NEAR state | Deserialization fails | No silent reset and no on-chain migration; any future migration needs a separately designed and reviewed recovery/redeployment procedure |
| Zcash provider success | Still `NOT_IMPLEMENTED` | No settlement |

## 5. Dependency inventory (selected)

- NEAR: `near-sdk =5.17.2`, `borsh =1.8.1`
- Zcash: `zcash_client_backend =0.24.0` (orchard), `zcash_client_sqlite =0.22.0`, `zcash_address =0.13.0`
- Solana: program crates + official platform-tools for SBF
- PQ: `@noble/post-quantum` 0.7.1 (ML-DSA-65)
- EVM: Foundry / Solc 0.8.24

Lockfiles are present. No full vulnerability scan is claimed here.

## 6. Reproducible test commands and current CI evidence

```bash
node --test agent-router/test/*.test.mjs          # 517 local tests
node --test agent-router/test/cross-l1-envelope.test.mjs # 60 local tests
node --test agent-router/test/intent-store-recovery.test.mjs # 10 local tests
npm test --prefix agent-router/pq-auth            # 26 local tests
cargo test --manifest-path near/qpc-near-agent-finance/Cargo.toml --locked   # 32 in current code-revision CI
cargo test --manifest-path zcash/qpc-zcash-agent/Cargo.toml --locked         # 46 in current code-revision CI
# Solana host + SBF + LiteSVM: see solana/qpc-native-executor/scripts/
# Foundry: forge test --summary
```

At code revision `db9f7178f593a10096a8d40f61fe60d77ae583ca`, [NEAR CI run 43545644121](https://github.com/wadezigh96/pQp_contracts/actions/runs/43545644121) reports 32/32 Rust unit tests passing and, separately, 5/5 offline-VM tests passing after a compatible WASM build. Offline VM is not NEAR Sandbox. [Zcash CI run 43545644024](https://github.com/wadezigh96/pQp_contracts/actions/runs/43545644024) reports 46/46 adapter unit tests passing; it is not Zcash regtest. [PR #1](https://github.com/wadezigh96/pQp_contracts/pull/1) reported 11/11 status checks successful at that code revision. The local environment lacks Cargo/Rust and the Sandbox/Zcash node tools, so local Rust, NEAR Sandbox, and Zcash regtest were not run.

The starting revision had 31 NEAR unit tests, reconciled as 27 pre-owner-hardening tests plus four owner tests. This change adds one unauthorized-predecessor regression; current source and CI both report 32 tests. Exact scope limits and replay-store migration release gate are recorded in `ENGINEERING_EVIDENCE.md`.

## 7. Known limitations and unresolved risks

- No authentic historical NEAR state fixtures → no real data migration implemented.
- Zcash shielded transaction construction, signing, and independent receipt verification are not implemented.
- ML-KEM key establishment and full hybrid protocol with rotation/recovery are not implemented. Off-chain ML-DSA hybrid is a prototype only.
- Solana evidence is host + SBF + unsigned LiteSVM, not local validator or live cluster.
- No external professional audit has been performed.
- Account-level FullAccess/deployment authority remains outside WASM control; no contract method can revoke it.
- Filesystem intent store is single-host and not rollback-resistant against a malicious local writer.
- No production custody, key recovery, or operational monitoring is in place.

## 8. Auditor checklist

- [ ] Review exact calldata hash binding and allowance-selector denylist on EVM.
- [ ] Review NEAR predecessor authentication, nonce/intent scoping, and state layout rejection.
- [ ] Confirm no on-chain `migrate` entrypoint exists; review any future recovery or redeployment path separately.
- [ ] Review Solana PDA, CPI, and replay logic (including LiteSVM rollback tests).
- [ ] Review Zcash address/policy boundary and confirm no key material reaches the agent.
- [ ] Review optional hybrid verifier (domain separation, epoch pinning, rejection cases).
- [ ] Examine dependency lockfiles and known CVEs.
- [ ] Assess operational key custody and incident response procedures (currently incomplete).
- [ ] Verify that no path promotes callback-only or mock evidence to settlement.

**Scope statement:** This package covers the local prototype on branch `reason/deliver-qpc-hardening-dr-d997d6a3`. It does not constitute an external audit, a deployment attestation, or permission to use real funds.
