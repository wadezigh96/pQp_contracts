# QPC — Quantum Persistence Contract

**A chain-separated security architecture for Ethereum, BNB Smart Chain, Solana, and post-quantum cryptography.**

QPC is a security protocol project. Its goal is to enforce explicit, least-privilege policies around agent-initiated on-chain actions and to prepare cryptographic identity and authorization for post-quantum standards.

> **Important status:** the current Solidity implementation is an EVM policy prototype. It is not yet a post-quantum cryptographic implementation, is not a Solana program, and has not been independently audited. Do not use it to protect production funds.

## Four deliberately separated components

| Component | Directory | Role |
|---|---|---|
| Ethereum | `contracts/ethereum/` | Ethereum-specific QPC deployment adapter |
| BNB Smart Chain | `contracts/bsc/` | BSC-specific QPC deployment adapter |
| Solana | `contracts/solana/` | Independent Rust/Anchor program workspace boundary |
| Post-quantum cryptography | `contracts/pqc/` | PQ signature/key-establishment design and integration boundary |

The Ethereum and BSC adapters share only the reviewed EVM policy core in `contracts/shared/`. They have separate entry-point contracts, deployment scripts, chain configuration and test targets. Solana is a separate Rust program; it must not import Solidity or inherit EVM assumptions. PQC is a separate cryptographic subsystem and must not be confused with a chain-specific token or contract.

## Security principles

- Human or governance authority defines policy; agents receive only bounded execution permissions.
- Least privilege, expiry, allowlists, spending caps, nonce sequencing and emergency pause.
- Chain-specific adapters are isolated; no cross-chain execution is implied.
- Fail closed when authorization or cryptographic verification is unavailable.
- No claim of being unhackable or quantum-proof.

## Post-quantum standards

The intended standards track is NIST FIPS 204 (ML-DSA) and FIPS 205 (SLH-DSA) for digital signatures. FIPS 203 (ML-KEM) is for key establishment, not transaction signing. No algorithm is considered active until a maintained implementation, test vectors, key formats, domain separation, replay protection and chain-specific verification costs have been independently reviewed.

## Repository layout

```
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

Solana workspace and PQC integrations are not production-ready and are not included in the EVM test command. Each must gain its own reproducible build, unit tests, integration tests and security review before being advertised as implemented.

## Audit and deployment policy

This repository has not received an independent audit. Passing tests do not prove safety. No deployment is authorized by this repository change. Before any testnet or mainnet deployment, review the exact chain, bytecode, constructor authority, RPC, environment variables, gas assumptions and upgrade/governance model.

**Separate chains. Separate contracts. Separate cryptographic trust boundaries.**
