# QPC Security Model

## Project status

QPC (Quantum Persistence Contract) is experimental. The existing EVM policy core is a prototype, not an independently audited production security system. Ethereum, BSC, Solana and post-quantum cryptography are separate implementation boundaries.

## Separation requirements

- Ethereum: its own adapter, deployment script, chain-ID assertion, tests and release configuration.
- BNB Smart Chain: its own adapter, deployment script, chain-ID assertion, tests and release configuration.
- Solana: independent Rust program with native account/signer/PDA/CPI validation.
- PQC: separate cryptographic integration; do not represent a roadmap or interface specification as working cryptography.

## EVM threat model

The EVM prototype includes owner-managed agent policies, expiry, native/token spending caps, target/selector allowlists, recipient allowlists, pause, nonce sequencing and reentrancy protection.

Known limitations requiring remediation before production:
- Selector allowlisting does not validate semantic calldata arguments.
- Token calls must reject EOAs and malformed/non-standard return behavior safely.
- Owner key compromise remains a root trust failure.
- External target contracts can be upgraded or change behavior.
- The contract does not provide post-quantum signature verification.
- No independent audit has been completed.

## PQ cryptography

Target standards:
- NIST FIPS 204 ML-DSA: digital signatures.
- NIST FIPS 205 SLH-DSA: hash-based digital signatures.
- NIST FIPS 203 ML-KEM: key establishment only, not digital signatures.

Integration requires validated implementations, known-answer test vectors, strict input bounds, canonical encodings, domain separation, chain/application binding, replay protection and independent cryptographic review. Do not invent algorithms or claim quantum resistance from naming alone.

## Deployment safety

No deployment is authorized by code changes or passing tests. Before deployment, independently verify chain ID, RPC endpoint, deployer, constructor owner, bytecode, compiler settings, balances, permissions and recovery procedures. Testnet deployments also require explicit approval.
