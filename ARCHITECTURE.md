# QPC — Architecture

## Purpose

QPC (Quantum Persistence Contract) is designed as a chain-separated security protocol. It aims to enforce least-privilege policies for agent-initiated actions and to provide a reviewed path toward post-quantum authorization.

The repository intentionally separates four components: Ethereum, BNB Smart Chain, Solana, and post-quantum cryptography (PQC).

## Component boundaries

### Ethereum
Path: `contracts/ethereum/`

An Ethereum-specific Solidity adapter. It may use the shared EVM policy core, but deployment configuration, chain ID checks, addresses and release validation remain Ethereum-specific.

### BNB Smart Chain
Path: `contracts/bsc/`

A separate BSC Solidity adapter. It may use the shared EVM policy core, but must have independent chain ID checks, deployment configuration, addresses, tests and release validation. Sharing source code does not mean sharing deployments or state.

### Solana
Path: `contracts/solana/`

An independent Rust program boundary. Solana account ownership, PDA derivation, signer checks, instruction data, CPI behavior, rent and compute limits must be implemented and tested natively. Solidity code is not reused here.

### Post-quantum cryptography
Path: `contracts/pqc/`

A separate cryptographic subsystem specification and integration boundary. It covers signature verification and key establishment, not a fourth blockchain. FIPS 204 ML-DSA and FIPS 205 SLH-DSA are signature standards; FIPS 203 ML-KEM is key establishment and is not a signature scheme. No cryptographic algorithm is claimed to be implemented until a maintained implementation and test vectors are integrated and reviewed.

## EVM source sharing

`contracts/shared/QPCPolicyCore.sol` contains chain-neutral EVM policy logic. Ethereum and BSC adapters inherit this core through separate contract entry points. Chain-specific deployment scripts and configuration must verify the expected chain ID before broadcast.

## Trust model

- Human owner or governance defines policies.
- Agents can request only explicitly permitted actions.
- The contract validates policy, expiry, target/function permission and spending constraints.
- AI model output is never a security boundary.
- PQ signatures, once integrated, authenticate authorization material; they do not make unsafe contract logic safe.

## Current implementation limits

The inherited EVM prototype is not a complete universal cybersecurity system. It does not protect validators, consensus, RPC providers, wallets, bridges, or external contracts. Its selector allowlist does not validate all semantic arguments. It is not post-quantum and has not been independently audited.

## Testing strategy

Keep tests separated by trust boundary:
- shared EVM core unit/security/invariant tests
- Ethereum adapter and chain-ID tests
- BSC adapter and chain-ID tests
- Solana program unit and validator-based integration tests
- PQC known-answer tests, malformed-input tests, replay/domain-separation tests and cross-language test vectors

A passing suite is not an independent audit.
