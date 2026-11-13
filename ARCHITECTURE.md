# QPC — Architecture

## Purpose

QPC (Quantum Persistence Contract) is an experimental, chain-separated security project. It explores least-privilege policies for agent-initiated actions; the name does not mean its contracts are post-quantum.

The repository separates Ethereum and BNB Smart Chain EVM adapters, NEAR-native contracts, a Solana program boundary, a Zcash wallet-adapter prototype, and a post-quantum cryptography (PQC) integration boundary.

## Component boundaries

### Ethereum
Path: `contracts/ethereum/`

An Ethereum-specific Solidity adapter. It may use the shared EVM policy core, but deployment configuration, chain ID checks, addresses and release validation remain Ethereum-specific.

### BNB Smart Chain
Path: `contracts/bsc/`

A separate BSC Solidity adapter. It may use the shared EVM policy core, but must have independent chain ID checks, deployment configuration, addresses, tests and release validation. Sharing source code does not mean sharing deployments or state.

### NEAR
Path: `near/`

NEAR-native Rust policy and same-chain NEP-141 escrow prototypes. NEAR runtime ML-DSA-65 access-key signing, where supported by the selected network and tooling, authenticates the submitted transaction; it does not add an ML-DSA verifier to the QPC contract or make NEAR consensus post-quantum. See the [NEAR post-quantum experiment plan](near/POST_QUANTUM_EXPERIMENTS.md).

### Solana
Path: `contracts/solana/`

An independent Rust program boundary. Solana account ownership, PDA derivation, signer checks, instruction data, CPI behavior, rent and compute limits must be implemented and tested natively. Solidity code is not reused here.

### Post-quantum cryptography
Path: `contracts/pqc/`

A separate cryptographic subsystem specification and integration boundary. It covers signature verification and key establishment, not a blockchain. FIPS 204 ML-DSA and FIPS 205 SLH-DSA are signature standards; FIPS 203 ML-KEM is key establishment and is not a signature scheme. No QPC-owned PQ algorithm or verifier is implemented. NEAR's support for ML-DSA access-key transaction signatures is a network capability, not an integration in this repository's contract code; see [NEAR's access-key documentation](https://docs.near.org/protocol/accounts-contracts/access-keys).

## EVM source sharing

`contracts/shared/QPCPolicyCore.sol` contains chain-neutral EVM policy logic. Ethereum and BSC adapters inherit this core through separate contract entry points. Chain-specific deployment scripts and configuration must verify the expected chain ID before broadcast.

## Trust model

- Human owner or governance defines policies.
- Agents can request only explicitly permitted actions.
- The contract validates policy, expiry, target/function permission and spending constraints.
- AI model output is never a security boundary.
- PQ signatures, once integrated, authenticate authorization material; they do not make unsafe contract logic safe.

## Current implementation limits

The EVM policy prototype is not a complete universal cybersecurity system. It does not protect validators, consensus, RPC providers, wallets, bridges, or external contracts. Its selector allowlist does not validate all semantic arguments. It is not post-quantum and has not been independently audited.

The NEAR contracts also have no in-contract ML-DSA/SLH-DSA verification. A transaction signed with a NEAR ML-DSA access key is not evidence that QPC itself verified that signature, and it does not make validator/block signatures or cross-chain threshold signing post-quantum. The Zcash adapter is a Rust prototype without shielded transaction construction, signing, broadcast, or a live cross-chain settlement path. QPC has no x402 adapter; x402's payment negotiation format alone does not provide private settlement or post-quantum signing.

## Testing strategy

Keep tests separated by trust boundary:
- shared EVM core unit/security/invariant tests
- Ethereum adapter and chain-ID tests
- BSC adapter and chain-ID tests
- Solana program unit and validator-based integration tests
- PQC known-answer tests, malformed-input tests, replay/domain-separation tests and cross-language test vectors

A passing suite is not an independent audit.
