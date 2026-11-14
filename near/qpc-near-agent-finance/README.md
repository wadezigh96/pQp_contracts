# QPC NEAR authorization contract

This contract records native-account-authorized QPC cross-L1 intents. The original `authorize_envelope` entrypoint **does not execute payments**. A separate narrow `execute_near_envelope` entrypoint now supports caller-funded same-chain native NEAR transfers and private result/refund callbacks in local mock-runtime tests. Neither method signs/broadcasts wallet transactions, verifies external destination consensus or proves shielded settlement. See [local execution and remaining limits](../../docs/LOCAL_EXECUTION.md).

## Actual authorization boundary

1. The owner calls `configure_agent` to bind the native agent, router account, policy ID/version, policy expiry, destination-atomic per-payment limit and cumulative cap.
2. The owner calls `configure_envelope_profile(agent, envelope)` to approve the exact canonical v1 profile. The complete schema and shared JS/Rust vector are in [the canonical specification](../../docs/CANONICAL_SPEC.md) and [the shared fixture](../../docs/fixtures/cross-l1-v1.json).
3. The owner allowlists each lowercase 64-hex recipient commitment with `set_recipient_permission`. The legacy parameter name `opaque_recipient_ref` remains for this administration method; the new authorization path accepts only a commitment, not an opaque plaintext reference.
4. The native agent account calls `authorize_envelope(envelope, authorization_commitment)`.

Authorization checks the **NEAR predecessor account**, not caller-supplied metadata or the transaction signer. `agentIdentity.namespace` must be `near`; `id`, `keyId` and `source.accountId` must equal that predecessor. `keyId` denotes native account authority, **not a public-key signature proof**. `source.adapterId` and `replayDomain.authorityId` must equal the running contract account. The destination adapter equals the configured router; cancellation authority equals the owner. Policy ID/version must match active contract state.

The owner profile pins **all** static envelope fields: both network and native-chain IDs, identity, adapters, action, assets/decimals/mapping, **both exact amounts**, fees/slippage, replay scope, policy commitment, settlement/finality requirements, cancellation authority and revocation epoch. Only the intent ID, monotonic nonce, time window and allowlisted recipient commitment can vary. Changing an amount or route requires a new owner-approved profile; an agent cannot self-approve a new profile by recomputing a hash. The destination atomic amount is charged against the policy's spending caps. No exchange-rate correctness or fee execution is asserted.

The full canonical commitment is checked after validation and before consuming the intent ID, nonce or spending cap. Time windows must be internally ordered, currently valid and within policy expiry. Intent IDs are consumed per native agent and never released; the per-agent nonce survives reconfiguration and revocation. Another configured agent may independently use the same intent ID. Receipt/result getters require both `agent_id` and `intent_id`; callback resolution remains bound to the authorization commitment.

Revocation disables the policy and deletes its profile. Reconfiguration increments the policy version, preserves the nonce, resets that version's spending budget, and deletes its profile; a fresh explicit owner profile is required. Old envelopes cannot reactivate through an epoch-only change. Recipient permissions persist across versions under owner control and can be revoked independently.

## Breaking compatibility and limitations

- **Owner initialization is testnet-pinned for this hardening branch.** `new` is one-time, rejects any existing state, rejects owner substitution, and accepts only `wkams.testnet`. `get_owner()` is a read-only verification method. Contract-storage ownership is enforced by `env::predecessor_account_id()` for administrative methods; the initializer does not control the NEAR account's access keys.
- **`dispatch_action` is globally disabled**, retained only as an ABI tombstone that always rejects. The old narrow payment-argument hash is not an alternative authorization route. Its old hash fixture remains only as a compatibility test.
- **State is explicitly versioned and incompatible state fails closed.** Every serialized root starts with the 16-byte `QPCNEAR\0LAYOUT\0\0` magic and little-endian `u32` layout version `1`. Borsh validates this header before loading any root collection handle, including in generated ABI wrappers. `get_state_layout_version()` returns `1` only after a supported state root is decoded. Missing/corrupt headers, unsupported versions and truncated roots reject; `new` cannot replace any existing `STATE`. There is no reset, upgrade or migration entrypoint.
- This guarded layout is **not compatible with prior unversioned deployments**, including the structurally identical unversioned root at `b7eddc19aaa65b0e63eb93b701f7aa3c8544c087`. No legacy migration is claimed: `LookupMap` stores key/value encodings outside the root, and the earlier global-intent namespace cannot be safely inferred from root decoding alone. Do not deploy this code over legacy state: normal reads, execution and pending recovery callbacks would reject, not migrate. A separately reviewed migration must identify the actual deployed layout/code and preserve every replay reservation, policy, receipt, in-flight execution and refund before such an upgrade. No deployment or migration is performed here. Future incompatible root **or collection encoding** changes must bump the layout version.
- NEAR's host API supplies the current contract account, not its network/genesis ID. The owner must configure source `networkId` and `nativeChainId` from independently trusted deployment configuration. Authorization pins that profile; it cannot independently discover the live network identity.
- The profile hash binds recipient commitments and declared privacy/evidence requirements; this is neither private NEAR state nor verification of shielded notes. Input arguments and state remain publicly observable. Events avoid duplicating recipient references, amounts and full envelopes.
- An `AUTHORIZED` receipt is only a policy authorization commitment. It is not an execution, source finality, destination receipt, settlement, bridge or cross-chain atomicity claim.
- Native NEAR account authentication remains classical. No post-quantum signature, hybrid authentication, ZK proof, cross-chain escrow/refund or external destination verifier is implemented. Same-chain failed-transfer refund recovery is separate and does not release consumed replay state.
- Unit tests use the NEAR SDK host mock. WASM compilation is checked, but no sandbox deployment, RPC integration, gas/storage economics audit or mainnet activation is claimed.

## Owner and upgrade authority procedure

`get_owner()` reports the contract-storage owner only. It does not report or control the NEAR account's FullAccess keys, function-call keys, deployment permission, or account recovery. A FullAccess key on `funzest6203.testnet` can deploy replacement code at the account layer if the account protocol permits that action; the contract cannot revoke or constrain that key from inside WASM. No agent, router, or contract method should receive that key.

Before any future upgrade, require two independent operators to review the exact WASM hash, state-layout version, migration plan, owner account, and deployment account. Operator A prepares and records the artifact; Operator B independently recomputes the hash and approves the same artifact and transaction parameters. Deployment is then performed only with a separately managed deployment authority, followed by read-only verification of `get_owner()`, `get_state_layout_version()`, and critical policy state. This repository does not implement an in-contract upgrade method or ownership transfer because neither can safely govern account-level deployment authority.

## Checks

From the repository root:

```bash
cargo fmt --manifest-path near/qpc-near-agent-finance/Cargo.toml -- --check
CARGO_BUILD_JOBS=1 cargo test --manifest-path near/qpc-near-agent-finance/Cargo.toml
rustup target add wasm32-unknown-unknown
CARGO_BUILD_JOBS=1 cargo build --manifest-path near/qpc-near-agent-finance/Cargo.toml --target wasm32-unknown-unknown --release
```

If the shared toolchain directory is read-only, install a local toolchain without modifying it:

```bash
RUSTUP_HOME=/tmp/qpc-near-rustup rustup toolchain install stable --profile minimal --target wasm32-unknown-unknown --no-self-update
RUSTUP_HOME=/tmp/qpc-near-rustup CARGO_BUILD_JOBS=1 cargo build --manifest-path near/qpc-near-agent-finance/Cargo.toml --target wasm32-unknown-unknown --release
```

Tests exercise the actual public entrypoint: exact shared JS/Rust bytes/hash; every leaf mutated with the original commitment; recomputed malicious routes/assets/amounts/policies; native predecessor identity; strict malformed/unknown/non-ASCII/overflow inputs; time windows; allowlist/revocation; durable Borsh-reloaded replay state; nonce/intent replay; per-payment/cumulative limits; nonce overflow; reconfiguration; owner-only controls; and legacy bypass rejection. No signer, deployer or wallet credential is needed.
