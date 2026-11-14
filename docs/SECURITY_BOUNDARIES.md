# QPC security boundaries and claims

## Owner and account authority

The NEAR contract's storage owner is pinned to `wkams.testnet`; initialization also requires the NEAR predecessor to equal the supplied owner and rejects reinitialization. This limits contract-level administration only. Account-level access keys, FullAccess permissions, and deployment/upgrade authority for the intended target `funzest6203.testnet` remain outside WASM control. No contract method can revoke those keys. The target account has not been deployed or modified in this task.

Never give an agent or router FullAccess, change Owner keys, use existing account balances, or include secret material in source, logs, examples, CI, or artifacts. Public deployment or transaction submission requires separate explicit approval.

## Cross-chain authorization vs execution vs settlement

The canonical envelope binds route, amount, recipient commitment, policy, nonce, time, and settlement profile. Its required commitment remains `c72c9a07dfd9ce8aecbd21c9c6bf858e5f089072a9ec84bafb03aadada653333`. The V2 replay-domain key additionally isolates source adapters. Existing durable stores created with the prior key derivation must not be upgraded/reset without an explicit atomic migration that preserves consumed nonces and intents; this repository does not implement that migration. A valid commitment or authorization is not a signature, broadcast, chain proof, or settlement.

The prototype is not atomic cross-chain settlement. Authorization, transaction signing, broadcast, inclusion, confirmation, receipt verification, and any recovery/refund are separate trust steps. Generic callbacks/provider responses do not establish settlement.

## Zcash privacy boundary

The adapter can validate Unified Address network/receiver policy and protect opaque recipient references in its local API. It does not currently create or broadcast a Zcash shielded payment, sync received notes, or verify a shielded receipt. No transparent-only fallback may be described as private. Do not expose seeds, spending/viewing keys, backups, raw shielded addresses, or unnecessary wallet metadata.

## Post-quantum boundary

Post-quantum protection is **not active end to end**. The optional off-chain Ed25519 + ML-DSA-65 hybrid verifier is a prototype. ML-KEM key establishment, full versioned protocol/key lifecycle, on-chain enforcement, and production integration are not implemented. SHA-256 commitments are not signatures and do not provide post-quantum authorization.

## Evidence boundary

Unit tests, offline Wasm VM tests, NEAR Sandbox, Zcash regtest, and Zcash Testnet transactions are different evidence classes. This branch has no demonstrated local NEAR Sandbox ↔ Zcash regtest transfer. A real result requires actual chain/wallet responses, verified network identity, transaction IDs, receipt evidence, confirmation state, and replay-safe durable processing. No external professional audit is claimed.
