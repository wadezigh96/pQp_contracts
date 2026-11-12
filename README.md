# PQP — Post-Quantum Persistence

**Universal On-Chain Cybersecurity & Agent Protection Protocol**

PQP is a chain-adaptable security framework for smart contracts, digital assets, and autonomous agents. It combines deterministic on-chain policy enforcement with post-quantum cryptographic agility and chain-specific security adapters.

> Agents may act within explicit policy. They never receive unlimited authority.

## What PQP is — and is not

PQP is designed to provide reusable security controls across multiple Layer-1 networks. It is not a promise that any blockchain, application, wallet, bridge, or contract is unhackable or fully quantum-safe.

PQP separates three jobs:
1. On-chain enforcement: permissions, spending limits, expiry, replay protection, pause/revoke, and auditable state transitions.
2. Post-quantum readiness: crypto-agile identity and authorization interfaces that can adopt standardized post-quantum signatures where the host chain supports them.
3. Threat monitoring: off-chain detection and alerting. Monitoring can recommend or request a pause, but it is not a substitute for deterministic contract checks or authorized governance.

## Security layers

| Layer | Responsibility |
|---|---|
| PQP Policy Core | Least-privilege roles, limits, allowlists, expiry, nonces, pause/revoke |
| PQP Crypto Agility | Versioned key/signature schemes and migration paths |
| PQP Agent Guard | Bounded agent actions, budgets, destinations, method permissions |
| PQP Contract Guard | Contract-call constraints and fail-closed validation |
| PQP Monitor | Off-chain anomaly detection, alerting, incident evidence |
| PQP Chain Adapters | Native enforcement and account semantics per L1 |
| PQP Recovery | Timelock, multisig, key rotation, emergency controls |

## Initial implementation: NEAR

The first native implementation will target NEAR using Rust and the NEAR SDK.

Planned modules:
- pqp-policy: policy lifecycle, roles, limits, expiry and revocation.
- pqp-agent-guard: constrained agent requests and per-action accounting.
- pqp-audit: events and versioned policy records.
- pqp-crypto-agility: algorithm identifiers and migration metadata; no home-grown cryptography.
- pqp-near-adapter: NEAR account, access-key and cross-contract-call integration.

NEAR's published post-quantum roadmap reports ML-DSA account-signing support and describes Chain Signatures / NEAR Intents, while explaining that validator-consensus migration is a separate and harder task. PQP will rely only on capabilities verified in the actual target network and SDK version. A PQP contract alone does not upgrade NEAR consensus.

## Multi-L1 strategy

PQP uses a shared policy specification, not one supposedly universal bytecode contract.

| Network family | Adapter direction |
|---|---|
| NEAR | Native Rust contract and NEAR account/access-key model |
| EVM (Ethereum, BNB Chain, Avalanche, etc.) | Solidity policy guard and chain-specific signature/verifier support |
| Solana | Native Rust program and Solana instruction/account model |
| Other L1s | Separate adapter after capability and threat-model review |

Each adapter must pass shared policy conformance cases plus chain-specific security tests. A bridge or relayer is a separate trust boundary.

## Repository status

The existing Solidity POAAgentFinance contract is retained as a legacy EVM prototype and audit input. It is not the PQP implementation and does not provide post-quantum cryptography.

This branch is a design and implementation workspace. No production deployment, external audit, or post-quantum verification claim is made.

## Security-first development

1. Audit the inherited EVM prototype and preserve findings.
2. Define a chain-neutral policy specification and threat model.
3. Implement the smallest NEAR-native policy core in Rust.
4. Add unit, property, adversarial and integration tests.
5. Verify crypto capabilities and gas/storage costs on the target network.
6. Build EVM and Solana adapters only after the NEAR policy model is stable.
7. Obtain independent security review before handling real assets.

## Important limitations

- A policy contract cannot prevent vulnerabilities in L1 consensus, validators, wallets, RPCs, bridges, oracles, or external protocols.
- Off-chain AI anomaly detection is probabilistic. It must not silently gain authority to move funds.
- ML-DSA, SLH-DSA and ML-KEM have different purposes. Digital signatures and key establishment are not interchangeable.
- A chain is post-quantum ready only to the extent its transaction authentication, account keys, consensus, bridges, and dependent infrastructure have migrated.
- Passing tests is evidence, not a security guarantee.

## Standards and primary references

- NIST FIPS 203 — ML-KEM: https://csrc.nist.gov/pubs/fips/203/final
- NIST FIPS 204 — ML-DSA: https://csrc.nist.gov/pubs/fips/204/final
- NIST FIPS 205 — SLH-DSA: https://csrc.nist.gov/pubs/fips/205/final
- NEAR quantum-safe roadmap (24 August 2026): https://www.near.org/blog/near-quantum-safe-roadmap

**Policy first. Verify cryptography. Execute only within bounds.**
