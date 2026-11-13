# QPC NEAR Policy Gate — Security Model

**Status:** Reservation/settlement accounting lifecycle is implemented in source. CI verification is required for the exact commit; independent audit is pending. This is not a security certification.

## Security objectives

1. Keep policy administration under explicit owner authority.
2. Limit the blast radius of compromised agent credentials.
3. Reject unauthorized, expired, replayed, over-limit, and out-of-allowlist requests.
4. Allow a separate emergency role to stop new authorizations.
5. Bind an authorization receipt to canonical intent fields.
6. Prevent duplicate settlement and release transitions.
7. Bound the number of persistent agent policy records.

## Controls in source

| Control | Implementation | Residual limitation |
|---|---|---|
| Owner-only administration | Predecessor account equality check | Owner account compromise remains critical; use NEAR multisig as owner |
| Two-step ownership transfer | Proposed owner must call accept | No time delay or guardian veto yet |
| Emergency guardian | Guardian can pause only | Guardian cannot unpause; owner can unpause |
| Least privilege | Per-agent recipient and spending limits | No asset/service scoping in current API |
| Reservation lifecycle | Pending → Settled or Released; terminal states cannot transition again | Adapter is trusted to verify external settlement/failure |
| Settlement replay | Unique settlement reference map | Reference is only as meaningful as adapter verification |
| Cap accounting | Reserved + settled amounts consume cap; released amounts restore capacity | No refunds/chargebacks lifecycle yet |
| Reconfiguration safety | Pending reservations must be cleared before policy version changes | Old reservation retention and pruning not yet designed |
| Expiry | Maximum 30-day policy duration | Timestamp semantics rely on NEAR block timestamp |
| Replay resistance | Sequential per-agent nonce | Future cross-chain adapters need independent domain and idempotency |
| Request binding | Contract-derived SHA-256 digest | Current digest omits asset, service, network and settlement expiry |
| Arithmetic safety | Checked add for totals, nonce, version and policy count | Continue fuzz/property testing |
| State-growth bound | Maximum 256 unique agent records | Lifetime cap; revoked records are not deleted |
| Emergency stop | Paused flag gates authorization | Does not reverse already executed external actions |

## Important threat boundaries

- NEAR contract state, method arguments, and transaction metadata are public. This contract is not a shielded-payment system.
- A request hash is not a signature, proof of settlement, or proof that an external payload is truthful.
- The contract does not custody or transfer funds in this prototype. It records reservation/settlement accounting only.
- No x402 facilitator, NEP-141 transfer, cross-contract promise, bridge, or cross-chain execution is implemented. A settlement adapter is a trusted account, not an on-chain proof verifier.
- No TEE attestation verification or post-quantum signature verification is implemented.
- Smart-contract controls cannot protect a compromised owner key. Use multisig and hardware-backed account security.
- Pause does not stop the underlying NEAR network or unrelated contracts.

## Required adversarial verification before deployment

- Owner/agent/guardian role confusion and unauthorized method calls.
- Wrong owner acceptance, owner proposal replacement, and ownership rotation.
- Guardian pause, guardian privilege escalation, and owner-only unpause.
- Replay, nonce exhaustion, stale policy version, and policy revocation.
- Amount zero, per-payment cap, cumulative cap, and u128 boundary conditions.
- Settlement by non-adapter, repeated settlement, duplicate settlement reference, release by unauthorized caller, and release/settle race.
- Reconfiguration while a pending reservation exists; settled amounts remain charged to the cap; released amounts restore capacity.
- Storage growth from reservation and settlement-reference maps; no deletion/pruning is currently implemented.
- Expiry at exact boundary and maximum duration.
- Empty, duplicate, oversized, and malformed allowlists.
- 256-policy boundary and attempted 257th policy.
- Request digest changes when any bound field changes.
- Storage growth, account balance exhaustion, and deployment migration.
- Build reproducibility, dependency review, fuzzing, property tests, and independent audit.

## Release gate

Do not deploy or connect real funds until Rust tests and WASM build pass on the exact commit, the reservation-to-settlement accounting lifecycle passes adversarial tests, an independently verified settlement adapter exists, storage costs and retention are addressed, the storage-cost model is tested, and an independent security review has no unresolved critical/high findings.
