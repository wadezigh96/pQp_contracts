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
| Contract initializes only once with intended owner `wkams.testnet` | `env::state_exists()` plus owner pin in `new` | `initialized_owner_is_exactly_the_intended_testnet_account`, `initializer_rejects_owner_substitution_before_state_creation`, `initializer_is_one_time_even_when_reinitialized_with_intended_owner` |
| Agent never becomes owner | Contract access control | Foundry + NEAR unit tests |
| Exact calldata hash required for EVM calls | `QPCAgentFinance` | Foundry security tests |
| NEAR predecessor (not claimed identity) authenticates | `authorize_envelope` | NEAR unit tests |
| Unversioned / wrong-version NEAR state rejected | `StateLayout` magic + version | `incompatible_unversioned_state_is_rejected...`, `versioned_state_roundtrips...` |
| Migration is owner-only and idempotent for current version | `migrate` | `migrate_is_idempotent...`, `migrate_cannot_be_reached_on_incompatible_state` |
| Administrative calls cannot be made by agent/attacker | `assert_owner` on configuration methods | `non_owner_cannot_administer_even_when_claiming_owner_in_request_metadata`, `owner_only_configuration_and_recipient_changes` |
| Generic evidence cannot settle | `FileIntentStore` + verifiers | JS settlement-boundary tests |
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
| Incompatible NEAR state | Deserialization fails | No silent reset; explicit migrate required |
| Zcash provider success | Still `NOT_IMPLEMENTED` | No settlement |

## 5. Dependency inventory (selected)

- NEAR: `near-sdk =5.17.2`, `borsh =1.8.1`
- Zcash: `zcash_client_backend =0.24.0` (orchard), `zcash_client_sqlite =0.22.0`, `zcash_address =0.13.0`
- Solana: program crates + official platform-tools for SBF
- PQ: `@noble/post-quantum` 0.7.1 (ML-DSA-65)
- EVM: Foundry / Solc 0.8.24

Lockfiles are present. No full vulnerability scan is claimed here.

## 6. Reproducible test commands (current branch)

```bash
node --test agent-router/test/*.test.mjs          # 516
npm test --prefix agent-router/pq-auth            # 26
cargo test --manifest-path near/qpc-near-agent-finance/Cargo.toml --locked   # 33 expected after owner hardening; execution currently blocked
cargo test --manifest-path zcash/qpc-zcash-agent/Cargo.toml --locked         # 46
# Solana host + SBF + LiteSVM: see solana/qpc-native-executor/scripts/
# Foundry: forge test --summary
```

Exact counts and historical checkpoints are recorded in `ENGINEERING_EVIDENCE.md`. The owner-hardening branch is currently blocked from executing Rust/Sandbox checks when the required toolchain is unavailable.

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
- [ ] Confirm `migrate` cannot be used to reset or reinterpret incompatible state.
- [ ] Review Solana PDA, CPI, and replay logic (including LiteSVM rollback tests).
- [ ] Review Zcash address/policy boundary and confirm no key material reaches the agent.
- [ ] Review optional hybrid verifier (domain separation, epoch pinning, rejection cases).
- [ ] Examine dependency lockfiles and known CVEs.
- [ ] Assess operational key custody and incident response procedures (currently incomplete).
- [ ] Verify that no path promotes callback-only or mock evidence to settlement.

**Scope statement:** This package covers the local prototype on branch `reason/deliver-qpc-hardening-dr-d997d6a3`. It does not constitute an external audit, a deployment attestation, or permission to use real funds.
