# QPC Security Model

## Project status

QPC (Quantum Persistence Contract) is experimental. The EVM and NEAR policy contracts are prototypes, not independently audited production security systems. Ethereum, BSC, NEAR, Solana, Zcash wallet tooling, and post-quantum cryptography are separate implementation boundaries.

## Separation requirements

- Ethereum: its own adapter, deployment script, chain-ID assertion, tests and release configuration.
- BNB Smart Chain: its own adapter, deployment script, chain-ID assertion, tests and release configuration.
- NEAR: native policy/escrow code; protocol-supported ML-DSA access-key signing is distinct from contract-level verification and consensus signatures.
- Solana: independent Rust program with native account/signer/PDA/CPI validation.
- Zcash: Rust wallet-adapter prototype, not a smart contract or a live shielded-payment service.
- PQC: separate cryptographic integration; do not represent a roadmap or interface specification as working cryptography.

## EVM threat model

The EVM prototype includes owner-managed agent policies, expiry, native/token spending caps, target/selector allowlists, recipient allowlists, pause, nonce sequencing and reentrancy protection.

Known limitations requiring remediation before production:
- Selector allowlisting does not validate semantic calldata arguments.
- Token calls must reject EOAs and malformed/non-standard return behavior safely.
- Owner key compromise remains a root trust failure.
- External target contracts can be upgraded or change behavior.
- The EVM contract does not provide post-quantum signature verification.
- No independent audit has been completed.

## NEAR, agent payments, and x402 boundaries

NEAR supports ML-DSA-65 access keys for transaction signing, but the QPC NEAR contracts do not implement an in-contract ML-DSA verifier. This account-authentication path does not make NEAR validator consensus, cross-chain threshold signing, or other chain contracts post-quantum. The current NEAR-to-Zcash helper prepares a request only; the Zcash prototype does not construct, sign, or broadcast shielded payments. QPC has no x402 integration. Even where a network has an x402 scheme, the scheme and facilitator determine signing and settlement behavior; x402 itself does not guarantee shielded privacy or quantum resistance. The published NEAR exact scheme specifies Ed25519 and secp256k1, not ML-DSA.

## PQ cryptography

Target standards:
- NIST FIPS 204 ML-DSA: digital signatures.
- NIST FIPS 205 SLH-DSA: hash-based digital signatures.
- NIST FIPS 203 ML-KEM: key establishment only, not digital signatures.

Integration requires validated implementations, known-answer test vectors, strict input bounds, canonical encodings, domain separation, chain/application binding, replay protection and independent cryptographic review. Do not invent algorithms or claim quantum resistance from naming alone.

## Deployment safety

No deployment is authorized by code changes or passing tests. Before deployment, independently verify chain ID, RPC endpoint, deployer, constructor owner, bytecode, compiler settings, balances, permissions and recovery procedures. Testnet deployments also require explicit approval.
