# QPC — Quantum Persistence Contract

**A chain-separated security project with EVM and NEAR policy prototypes, a Solana program boundary, a Zcash wallet-adapter prototype, and a post-quantum integration boundary.**

QPC explores explicit, least-privilege policies for agent-initiated on-chain actions. Its post-quantum work distinguishes chain-provided transaction authentication from cryptography implemented by QPC itself.

> **Important status:** the Solidity and NEAR contracts are experimental policy prototypes; neither has an in-contract post-quantum verifier, and the repository has not been independently audited. The Solana path is a separate program boundary, not the Solidity implementation. Do not use these prototypes to protect production funds.

## Separate implementation boundaries

| Component | Directory | Role |
|---|---|---|
| Ethereum | `contracts/ethereum/` | Ethereum-specific QPC deployment adapter |
| BNB Smart Chain | `contracts/bsc/` | BSC-specific QPC deployment adapter |
| NEAR | `near/` | NEAR-native policy and same-chain escrow prototypes |
| Solana | `contracts/solana/` | Independent Rust/Anchor program workspace boundary |
| Zcash | `zcash/` | Rust wallet-adapter prototype; no QPC smart contract on Zcash |
| Post-quantum cryptography | `contracts/pqc/` | PQ signature/key-establishment design and integration boundary |

The Ethereum and BSC adapters share the EVM policy core in `contracts/shared/`, but use separate entry-point contracts, deployment scripts, chain configuration and test targets. NEAR and Solana are separate Rust program boundaries and must not inherit EVM assumptions. The Zcash code is a wallet/transaction adapter, not a deployed QPC contract. PQC is a separate cryptographic subsystem, not a chain or token.

## Security principles

- Human or governance authority defines policy; agents receive only bounded execution permissions.
- Least privilege, expiry, allowlists, spending caps, nonce sequencing and emergency pause.
- Chain-specific adapters are isolated; no cross-chain execution is implied.
- Fail closed when authorization or cryptographic verification is unavailable.
- No claim of being unhackable or quantum-proof.

## Cybersecurity architecture

QPC treats an on-chain agent as an untrusted executor, not as a security authority. The NEAR policy contract is the enforcement point for the action that an agent is allowed to request. AI reasoning, route providers, solvers and external wallets remain outside the trust boundary and cannot expand the policy by returning a favorable result.

```text
┌──────────────────────┐
│ Private Agent Runtime │  NEAR AI Cloud / TEE boundary
│ intent + risk signal  │  no private key in model context
└──────────┬───────────┘
           │ commitment + bounded action request
           ▼
┌──────────────────────────────┐
│ QPC NEAR Cyber Defense Layer  │
│ policy/version/expiry checks  │
│ nonce + request replay guard  │
│ target + method + args hash   │
│ spending + call quotas        │
│ pause / quarantine boundary   │
└──────────┬───────────────────┘
           │ approved, chain-bound payload
           ▼
┌──────────────────────────────┐
│ Cross-L1 Execution Adapter    │
│ Chain Signatures or Intents   │
│ chain ID + derivation path    │
│ independent receipt verifier  │
└──────────┬───────────────────┘
           ▼
      Target L1 settlement
```

The current NEAR contract already enforces policy versioning, exact argument hashes, nonce sequencing, request uniqueness, expiry, target/method allowlists, destination-chain and derivation-path binding, spending caps, bounded calls, pause/revoke behavior and idempotent callbacks. The repository also includes unit coverage for opaque cross-chain recipient references and fail-closed privacy payload changes. The remaining planned Cyber Defense extension adds a security guardian, global circuit breaker, per-policy quarantine, incident IDs, payload limits and action cooldowns. Chain binding is an authorization commitment only; it does not implement Chain Signatures or destination-chain receipt verification.

The security lifecycle is:

```text
OBSERVE → CLASSIFY → COMMIT → ENFORCE → SIGN/SETTLE → VERIFY
                │
                └── risk threshold → QUARANTINE / GLOBAL HALT
```

Quarantine must stop new dispatches without deleting prior execution records. A failed target call consumes its reserved nonce and quota rather than becoming silently retryable. A provider `SUCCESS` response is not a settlement proof until an independent verifier confirms the expected target, amount, recipient commitment and transaction receipt.

## Quantum-resistant encryption architecture

QPC uses “quantum-resistant” as an explicit cryptographic design target, not as a claim that the current contracts are quantum-safe. The intended private envelope is a hybrid construction:

1. **ML-KEM** (NIST FIPS 203) establishes a session key for encrypted agent context or recipient metadata. ML-KEM is key establishment; it is not a transaction signature and must not be used as an authorization check.
2. **AEAD** such as AES-256-GCM or ChaCha20-Poly1305 encrypts the actual envelope after authenticated key establishment. The envelope must include a version, algorithm identifiers, protocol/domain label, chain ID, policy ID, request ID, expiry and ciphertext hash as authenticated data.
3. **ML-DSA** (NIST FIPS 204) or **SLH-DSA** (NIST FIPS 205) can provide post-quantum signatures for an off-chain decision envelope once a maintained implementation, canonical encoding and verifier are selected. The signature must be bound to the exact application, target chain, policy version and request nonce.
4. **Existing NEAR authentication and Chain Signatures remain separate layers.** A NEAR ML-DSA access key, where supported by the protocol and tooling, authenticates a NEAR transaction at the runtime boundary. It does not mean that QPC verifies ML-DSA inside WASM, that NEAR consensus is post-quantum, or that a Chain Signatures/MPC signature is post-quantum.

The private envelope must never place private keys, seed phrases or raw Zcash Unified Addresses into contract state, event logs, public README fixtures or agent prompts. On-chain state stores only the minimum commitment, opaque reference, action hash and audit metadata needed to enforce policy. Encryption also does not hide a transaction after it is broadcast to a transparent target L1; source timing, amount, route-provider data and target-chain metadata may still be correlated.

### Current status and required evidence

No QPC-owned ML-KEM, ML-DSA or SLH-DSA verifier is implemented in this repository. The `contracts/pqc/` directory and `near/POST_QUANTUM_EXPERIMENTS.md` describe an integration boundary and experiments, not production cryptography. Before calling any envelope quantum-resistant, the project must use a maintained implementation, pass published known-answer tests, reject malformed lengths and altered ciphertexts/signatures, enforce domain separation and chain binding, measure key/signature/ciphertext overhead, and receive independent cryptographic review. Until then, the correct status is **quantum-resistant design under evaluation**, not quantum-safe production security.

## Post-quantum standards

NIST FIPS 204 (ML-DSA) and FIPS 205 (SLH-DSA) are digital-signature standards. FIPS 203 (ML-KEM) is for key establishment, not transaction signing. No QPC-owned PQ signature verifier is implemented in this repository.

NEAR's protocol supports ML-DSA-65 access keys for signing transactions, as documented in [NEAR Access Keys](https://docs.near.org/protocol/accounts-contracts/access-keys). This protects only the relevant account-transaction signing path. It does not mean the QPC NEAR contract verifies ML-DSA itself, nor that NEAR consensus, cross-chain threshold signing, Ethereum settlement, or every wallet/tooling path is post-quantum. See the [NEAR experiment plan](near/POST_QUANTUM_EXPERIMENTS.md) and [official NEAR roadmap](https://www.near.org/blog/near-quantum-safe-roadmap).

## Agentic payments, privacy, and x402

The repository contains a NEAR-to-Ethereum router prototype using NEAR Intents 1Click, a local-only x402 exact-NEAR policy preflight, and a separate NEAR-to-Zcash helper that prepares a local shielded-request payload. Local tests do not prove live provider settlement. The x402 helper does not sign, verify, or settle payment. The Zcash adapter does not yet construct, sign, or broadcast shielded transactions, so this is not an end-to-end private payment feature.

There is no x402 adapter in QPC today. x402 is a payment negotiation and payload standard with scheme- and network-specific implementations; the official SDK documents an `exact` NEAR implementation in TypeScript. That NEAR scheme is x402 v2 and specifies `ed25519`/`secp256k1` signatures, not ML-DSA-65. x402 itself does not provide shielded-payment privacy or a post-quantum guarantee. See the [x402 SDK support matrix](https://docs.x402.org/sdk-features) and [NEAR exact scheme specification](https://github.com/x402-foundation/x402/blob/main/specs/schemes/exact/scheme_exact_near.md).

## Repository layout

```
near/             # NEAR-native policy and same-chain escrow prototypes
zcash/            # Rust wallet-adapter prototype; no shielded transaction send path
agent-router/     # NEAR-to-Ethereum 1Click prototype and local-only NEAR-to-Zcash request builder
contracts/
  shared/       # EVM policy core shared by ETH and BSC adapters
  ethereum/     # Ethereum-only adapter and deployment config
  bsc/          # BNB Smart Chain-only adapter and deployment config
  solana/       # Independent Rust program workspace boundary
  pqc/          # PQ signature and key-establishment integration boundary
test/
  ethereum/     # Ethereum adapter tests
  bsc/          # BSC adapter tests
  shared/       # Shared EVM core tests
script/
  ethereum/     # Ethereum deployment script
  bsc/          # BSC deployment script
```

## Build and test

EVM (requires Foundry):
```bash
forge build
forge test
```

NEAR, Solana, Zcash and PQC components have separate build/test requirements and are not covered by the EVM test command. Passing one component's tests does not establish another component's integration or production readiness.

## Audit and deployment policy

This repository has not received an independent audit. Passing tests do not prove safety. No deployment is authorized by this repository change. Before any testnet or mainnet deployment, review the exact chain, bytecode, constructor authority, RPC, environment variables, gas assumptions and upgrade/governance model.

**Separate chains. Separate contracts. Separate cryptographic trust boundaries.**


## Zcash privacy adapter (prototype; no payment send path)

QPC has a separate Rust Zcash wallet-adapter prototype under `zcash/`; it is **not** a smart contract deployed on Zcash. Current code covers Unified Address parsing, shielded-receiver policy, SQLite wallet/cache scaffolding, and payment-ID/nonce state. It does not import keys, construct/sign/broadcast shielded transactions, prove a live recipient receipt, or settle through a bridge/provider. The privacy design rejects transparent-only receivers and documents metadata leakage from routes and wallet infrastructure.

See [the Zcash privacy architecture and threat model](zcash/QPC_ZCASH_PRIVACY_ARCHITECTURE.md) and [the adapter README](zcash/README.md) for the exact implementation boundary.

A shielded Zcash destination does not make a cross-chain payment fully anonymous: source-chain activity, amounts, timing, route providers, and network metadata may still permit correlation. QPC must disclose those limits and fail closed when the required privacy properties cannot be met.
