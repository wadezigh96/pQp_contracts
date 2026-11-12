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

## NEAR implementation status

The native Rust implementation lives in `near/pqp-near`. It implements:
- owner-controlled policy creation, versioned updates, pause and irreversible revocation;
- immutable agent identity and bounded policy validity;
- exact target/method matching and on-chain SHA-256 validation of the exact argument bytes;
- per-call, per-action cumulative, policy-wide deposit limits and per-action call-count limits;
- sequential nonces and unique request IDs;
- bounded-gas NEAR promise dispatch and a private callback;
- explicit dispatched, succeeded, failed and oversized-result states;
- callback result reads capped at 256 bytes and immutable terminal results;
- conservative quota reservation at dispatch, with no refund on target failure.

This is an execution prototype, not a production custody layer. It still needs local runtime/testnet integration, gas/storage measurements, method-specific argument schemas and independent security review. See [NEAR Core](near/README.md).

## Multi-L1 strategy

PQP uses a shared policy specification, not one supposedly universal bytecode contract.

| Network family | Adapter direction |
|---|---|
| NEAR | Native Rust contract and NEAR account/access-key model |
| EVM (Ethereum, BNB Chain, Avalanche, etc.) | Solidity policy guard and chain-specific signature/verifier support |
| Solana | Native Rust program and Solana instruction/account model |
| Other L1s | Separate adapter after capability and threat-model review |

Each adapter must pass shared policy conformance cases plus chain-specific security tests. A bridge or relayer is a separate trust boundary.

## Existing EVM code

The Solidity `PQPAgentSecurity` contract is retained as a legacy EVM prototype and audit input. It is not the NEAR implementation and does not provide post-quantum cryptography.

## Security-first development

1. Implement and adversarially test the NEAR policy core.
2. Add the NEAR promise execution adapter with authenticated, idempotent callbacks.
3. Verify crypto capabilities and gas/storage costs on the target network.
4. Build EVM and Solana adapters only after the policy model and conformance suite are stable.
5. Obtain independent security review before handling real assets.

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
- NEAR quantum-safe roadmap: https://www.near.org/blog/near-quantum-safe-roadmap
- NEAR Rust SDK: https://github.com/near/near-sdk-rs

**Policy first. Verify cryptography. Execute only within bounds.**