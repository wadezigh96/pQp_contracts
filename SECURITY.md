# Security model and Phase 4 threat review

## Status and scope

**Unaudited local prototype; not approved for production funds.** This document is a threat model and release checklist, not an independent audit or evidence of a deployed system. The EVM contract, NEAR authorization contract, JavaScript router and Rust Zcash wallet boundary have different powers. Never infer one component's guarantees from another's tests.

- **EVM:** classical `msg.sender`-authenticated policy execution and token transfers in `QPCAgentFinance.sol`. Separately, `QPCNativeTestnetExecutor.sol` executes caller-funded Sepolia/BSC native intents whose canonical commitment-to-parameter association is explicitly attested by the owner; it does not parse canonical JSON or enforce protocol fee budgets.
- **NEAR:** `authorize_envelope` authenticates the native predecessor, validates the owner-pinned envelope profile, consumes replay/cap state and emits `AUTHORIZED`. That entrypoint does not transfer funds. The separate `execute_near_envelope` local prototype schedules a caller-funded same-chain native payment and private result callback, with original-agent-only failed-payment refund recovery. It does not verify external settlement. The legacy `dispatch_action` always rejects.
- **Router:** strict canonical commitments, trusted application authentication seams, local replay/lifecycle storage and unsigned native preparation. Caller callbacks are not cryptographic verifiers. Generic evidence and Zcash route callbacks cannot authorize production settlement.
- **Solana:** an owner-attested native processor with strict policy/intent PDAs, signer/CPI/replay enforcement and cross-language wire tests. In addition to host tests, the SBF artifact executes in unsigned LiteSVM simulations covering native CPI and transaction rollback; this is not validator/consensus or live-cluster evidence. The narrow verifier requires immutable code predating execution plus independently verified owner approval history; missing native history fails closed.
- **Zcash:** real Unified Address parsing and shielded receiver selection, local wallet/view-only storage and read-only boundaries. No shielded payment constructor, signer, broadcaster or independent receipt verifier is implemented.

For encoding and native adapter restrictions see [canonical specification](docs/CANONICAL_SPEC.md) and [native adapters](docs/NATIVE_ADAPTERS.md). The [engineering evidence](docs/ENGINEERING_EVIDENCE.md) distinguishes historical checks from current validation.

## 1. Assets, adversaries and trust

Protect owner authority, agent policy/caps, native/token funds where a contract actually holds them, immutable authorization parameters, replay history, wallet secrets, recipient data and truthful audit/settlement state.

Assume an attacker can control agent/model output, steal an agent key, alter a request after preview, send duplicates concurrently, substitute a recipient/amount/network, return malicious contract/token data, forge RPC/provider responses, withhold receipts, trigger timeouts and exploit reorganizations. A compromised host can alter local files and verifier callbacks; the filesystem prototype does not defend against that adversary. Network observers can correlate accounts, amounts, timing and RPC queries. Consensus failure and compromised owner authority are not solved by application guardrails.

The owner/operator is trusted to choose safe policies, routes, asset mappings, native network identities and verifier roots. Agents must not select these trusted values themselves. Owner compromise allows policy changes and, on EVM, native withdrawal. This is a root-of-trust failure, not a privilege that agent limits can contain. Destination protocol economics, price/liquidity correctness and chain consensus require separate review.

## 2. Authentication and least privilege

EVM owner-only controls include configuration, permissions, pause, native withdrawal and two-step ownership transfer. An active agent can use only permitted calls/transfers subject to expiry and spending limits; it cannot become owner or reset its counters. Active EVM agent/token policies cannot be overwritten to reset spending. An owner can revoke/expire and deliberately configure a new budget; caps are policy-lifecycle budgets, not lifetime limits across owner reauthorization.

NEAR authenticates `env::predecessor_account_id()`, not a request's claimed agent, signer metadata or a commitment. `agentIdentity.id`, `keyId` and source account must match that predecessor; `keyId` currently names an account authority, not a separately verified public key. The current contract account, configured router, policy ID/version and owner cancellation authority are bound. Native network identity is owner-configured because the contract host does not expose a network/genesis ID. NEAR owner configuration may replace a policy and reset its budget while preserving nonce progression and advancing policy version. It removes the old envelope profile. On the hardening branch, initialization is one-time, state-existence guarded, and pinned to `wkams.testnet`; `get_owner()` exposes storage ownership for read-only verification. There is no NEAR ownership-transfer API because a contract method cannot control account-level deployment authority.

NEAR account access keys and function-call permissions require separate deployment review. A function-call key's receiver/method restrictions are not a substitute for policy checks; indirect calls change predecessor semantics. Keep the `funzest6203.testnet` FullAccess/deployment authority and the `wkams.testnet` owner authority out of agent environments. A FullAccess key can replace account code at the protocol/account layer; QPC WASM cannot revoke or constrain it. Use a two-person artifact-hash and transaction-parameter review for upgrades, followed by read-only owner/layout/policy verification. The JS `verifyAgentAuthorization` callback is a **trusted integration dependency**, not built-in native authentication, signature verification or protection from a malicious orchestrator. Store administrative/submission methods similarly assume an authenticated trusted caller.

## 3. Replay, policy mutation and storage rollback

- The EVM execution nonce is an audit sequence, not a one-use intent nonce. An active agent can repeat identical allowlisted calldata while its caps permit it. The contract does not accept signed cross-L1 intents, so its nonce does not stop cross-chain replay.
- NEAR requires the exact next per-agent nonce and an unused intent ID, persists both on successful authorization, retains nonce progression on reconfiguration and binds policy version. Revocation removes the profile. Recomputed hashes do not approve changed policy parameters.
- The JS replay domain binds authority/scope, source network/native ID/account and agent namespace/identity. Nonce and intent ID have independent uniqueness indexes; policy/key changes must not reopen either index. Cancellation, expiry and failure do not release consumed keys.
- Prepared native handles are frozen process-local capabilities, **not** durable replay state or authorization.
- The filesystem store assumes a private single-host filesystem with atomic mkdir/rename and truthful fsync. It is not safe against malicious local writers, rollback/deletion, multi-host split brain, disk loss or restored stale backups. A corrupt store must not be silently reset. A stale lock fails closed; remove it only after proving no writer remains and reconciling records.

Production needs transactional durable replicated replay storage, tested backup/restore high-water marks and crash/power-loss testing. Restoring an older snapshot without reconciliation can re-enable a previously consumed authorization.

## 4. Parameter integrity and external calls

EVM `execute` requires both target/selector permission and the exact `keccak256(calldata)` allowlist entry. It binds encoded arguments, not the behavior or mutable implementation of an external protocol. Native `value` is separately bounded by per-call/cumulative limits; the calldata hash does not bind it. Recipient allowlists apply to `transferToken`, not universally to every recipient encoded in a generic call. Permission entries persist independently of policy reconfiguration, so review/remove stale permissions when rotating an agent.

Common `approve`, `increaseAllowance` and ERC-20/DAI `permit` selectors are rejected on generic execution. This denylist is not a complete detector of arbitrary approvals, proxy upgrades, multicalls or economic authority. Owner allowlisting and protocol-specific review remain required. Reentrancy locking and accounting-before-call protect the execution path; a failed EVM external call reverts accounting/nonce updates. Token code is required, and no-return or explicit-true transfer responses are supported; false/malformed responses fail closed. A malicious token can still lie, rebase or deduct fees. Low-level success does not prove a recipient balance delta.

The canonical envelope binds both networks/native IDs, adapters, identity, action, asset IDs/decimals/mapping, both atomic amounts, fees/slippage, recipient commitment, nonce/domain, policy/version/commitment, times, revocation epoch and settlement requirements. Hash equality alone authenticates none of those values. The owner profile and independent native policy must approve them. The v1 action has only type/operation, so native preparation must reject arbitrary calldata, token programs, extra instructions and unsupported privacy routes rather than invent missing authorization fields.

Strict field/type/range checks and immutable snapshots mitigate substitution and callback mutation. Raw JSON duplicate-key rejection is not implemented at the JS or NEAR JSON ingress; ordinary parsers retain the last duplicate. Production must reject duplicates before parsing and enforce byte/collection limits, not just nesting limits. Executable untrusted JS proxies are outside the data-object API boundary.

## 5. Cross-chain verification and fail-closed settlement

Authorization, unsigned preparation, submission reports, observed inclusion and independently verified final settlement are separate facts. No local label, provider `SUCCESS`, callback returning `true`, transaction hash, confirmation count or RPC `finalized` field alone is a consensus proof.

Current settlement paths deliberately stop short:

- `settleNativeTransfer` rejects production settlement; synthetic native evidence can return only `MOCK_VALIDATED_NOT_SETTLED`.
- Generic `verifyCrossL1SettlementEvidence` validates consistency but returns `NOT_VERIFIED`, even when the caller callback returns true. `FileIntentStore.confirm` cannot promote it to `CONFIRMED`; generic `settle` rejects, including historical callback-created confirmation records. The separate `settleNative` path requires a fresh (60-second wall-clock), unforgeable module-owned native verification receipt, independent operator-pinned verifier identity, exact authorization/transaction/network binding, transaction uniqueness and atomic lifecycle update. Deserialized persisted confirmations cannot activate it.
- `verifyNearToZcashRoute` cannot convert provider success plus a true callback into settlement; it returns `NOT_IMPLEMENTED` after consistency checks.

The narrow NEAR/EVM/Solana native execution verifiers currently have mocked-RPC integration evidence, plus separate read-only live RPC schema probes, not live deployed-executor payment evidence. Trust depends on the approved RPC endpoint, pinned deployed code/owner and upgrade-key custody. See [local execution details](docs/LOCAL_EXECUTION.md). Before production activation, independently authenticate native transaction bytes, signer/fee payer, exact authorization binding and execution effects. Verify inclusion/finality against an operator-pinned trust root and prevent transaction/evidence reuse. Enforce actual fees as well as transfer amount. A read-only RPC testnet probe or unsigned serialization check is useful compatibility evidence, not live execution or native finality verification.

Chain-specific requirements include:

- **NEAR:** complete transaction/action and receipt/outcome graph, all relevant asynchronous outcomes, canonical inclusion and finality. Caller success does not imply promise success or atomic rollback across receipts.
- **Ethereum:** chain ID, nonce, target/value/input, code/token semantics, successful execution, actual gas fees and finalized canonical inclusion. Empty calldata can still execute receive/fallback code.
- **BSC:** separate chain IDs and BSC consensus/finality assumptions; `safe`/justified is not `finalized`, and EVM compatibility is not Ethereum finality.
- **Solana:** trusted genesis/cluster, exact decoded instructions/account metas/signers, fee-payer authority, fresh blockhash and last-valid **block height**, successful execution, actual fees and finalized inclusion. Slot, block height and commitment are not interchangeable.
- **Zcash:** actual parser/network and shielded receiver policy, wallet scan/checkpoint trust, notes/witnesses/nullifiers, inclusion/confirmation policy and a privacy-preserving receipt model. A public txid cannot expose shielded recipient/amount evidence. Never silently downgrade to transparent delivery.

## 6. Private agents versus public metadata

An agent process running locally, a redacted Debug view, opaque recipient reference, encrypted message or commitment does **not** create private on-chain execution. There is no end-to-end private agent transport or unlinkable identity layer implemented here.

NEAR/EVM calldata, policy storage, accounts, transaction timing and metadata remain observable. NEAR events avoid duplicating raw recipient/amount, but the submitted envelope and configured profile are public. Deterministic native recipient commitments permit dictionary testing of candidate public recipients. The Rust Zcash opaque reference is random and keeps the raw UA inside its wallet boundary; source-chain identifiers, route timing, amount correlation and provider/network observations can still leak relationships.

Keep spending/seed/viewing keys and raw shielded addresses out of agents, logs, public state, route payloads and CI. Viewing keys are sensitive even without spending power. Local intent records retain full envelopes/evidence summaries and are not encrypted or anonymized; use least-privilege host permissions, encrypted storage and a documented retention/access policy. Read-only lightwalletd or privacy-query transport tests do not establish anonymous networking, PIR cryptographic correctness or independent consensus verification. State the actual observers and trust model for every route.

## 7. Incident response and recovery

1. **Contain:** on EVM, pause and revoke affected agents using an uncompromised owner; guarded deposits and agent execution stop, direct `receive()` deposits and owner withdrawal remain possible. On NEAR, revoke affected agents; there is no global pause in this implementation. Stop external orchestration/signing without deleting replay state.
2. **Preserve:** retain immutable intent/request digests, replay indexes, revisions, transaction/inclusion references and minimal necessary logs. Do not publish wallet secrets or recipient material as incident evidence.
3. **Reconcile:** reserve submission before any future external send. A crash/timeout after reservation or broadcast is ambiguous, not permission to resend. Investigate native inclusion, partial outcomes and finality changes; move uncertain in-flight/legacy confirmation records into reconciliation. Expiry/cancellation cannot undo a broadcast or refund funds.
4. **Recover authority:** revoke/rotate compromised agent authority under owner control, advance policy versions, repin profiles and preserve replay high-water marks. EVM offers two-step owner transfer; NEAR owner rotation and state/API migration need explicit future design. A compromised root owner has no automatic recovery mechanism here.
5. **Resume only after review:** verify backup integrity, native chain state, pending submissions, ownership, budgets and revocation propagation. Cross-chain compensation/refunds, escrow release and atomic recovery are not implemented. Do not mark an obligation paid to clear an alert.

## 8. Post-quantum migration — optional off-chain prototype only

The project name is a direction, not a current security property. Active authentication is classical chain-native authority. SHA-256 commitments and nonces are neither signatures nor a post-quantum authorization scheme.

Future evaluation should distinguish standardized **ML-DSA** (FIPS 204, signatures), **SLH-DSA** (FIPS 205, hash-based signatures) and **ML-KEM** (FIPS 203, key establishment). ML-KEM is not a signature and cannot by itself authorize an intent; PQ transport also cannot make classical L1 account control quantum-resistant. ML-DSA-65 is now an explicitly pinned dependency of the optional [off-chain hybrid authentication prototype](agent-router/pq-auth/README.md). It is not enabled by default or enforced by the L1 contracts. SLH-DSA and ML-KEM remain migration candidates, not implemented capabilities.

A future release must specify versioned algorithm/parameter IDs, canonical signed bytes/domain separation, identity/key ownership and rotation, hybrid verification semantics, downgrade/replay rejection across migrations, key compromise/revocation and recovery. Evaluate chain verification support, signature/key sizes, gas/storage/latency, randomness, side channels, library provenance and interoperability. A reviewed off-chain PQ signature is insufficient if the enforcing chain still accepts only a compromised classical owner key. The optional hybrid hook requires both Ed25519 and ML-DSA-65 over a domain-separated canonical envelope commitment, pins operator identity/key epochs, and rejects signature/algorithm downgrade. Its tests are local, self-generated interoperability and adversarial checks, not independent cryptographic validation. No PQ chain account control, KEM or PQ recovery implementation is active. Independent cryptographic review is required before any quantum-resistance claim.

## 9. Production blockers and audit requirements

Production remains blocked on externally audited authorization-bound native verification and deployed-executor integration evidence beyond mocked RPCs; isolated audited signer/executor integration; Zcash wallet scanning/construction/receipt verification; distributed durable replay and reconciliation; native network/trust-root provisioning; strict ingress/resource limits; NEAR state-layout/API migration and gas/storage analysis; operational key custody/revocation/backup/monitoring; and an explicit refund/compensation/trust/liquidity design for enabled routes. Do not treat unsigned testnet adapters or a simulated dashboard as removing these blockers.

Before handling real funds, run reproducible unit/security and property/invariant/fuzz campaigns, dependency/static analysis and native sandbox/fork integration tests. Independently review contracts, serialization, authentication/replay, adapters, cryptography, privacy and economic failure modes. After separately authorized deployment, verify artifact/owner/configuration identities and actual behavior on that build. Record the exact revision, commands, pass/fail results and untested assumptions. Historical review approval and passing CI are not a production audit, a deployment attestation or proof that later changes are safe.
