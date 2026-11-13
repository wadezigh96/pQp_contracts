# QPC NEAR post-quantum experiment plan

**Status: experimental test plan; no post-quantum signature integration is claimed by the QPC contract.**

## What NEAR already provides

Current [NEAR access-key documentation](https://docs.near.org/protocol/accounts-contracts/access-keys) lists ML-DSA-65 (NIST FIPS 204) as a supported access-key signature scheme. A NEAR node verifies the transaction signature before executing its actions. A QPC contract call can therefore be submitted by an account whose access key is ML-DSA-65, while QPC continues to enforce its own agent policy, exact argument hash, nonce, expiry, and spending limits. Tooling support is client-specific; NEAR's current [API documentation](https://docs.near.org/tools/near-api) lists `near-kit`, `near-kit-rs`, and `near-kit-py`, and notes `near-api-js` support is pending.

This is **protocol-level transaction authentication**, not in-contract ML-DSA verification.

## Hard boundary

- QPC NEAR does not implement an ML-DSA verifier in WASM.
- A successful call only proves that the NEAR runtime accepted the transaction under an authorized access key. It does not mean QPC independently inspected or verified an ML-DSA signature.
- NEAR's [published post-quantum roadmap](https://www.near.org/blog/near-quantum-safe-roadmap) distinguishes account signing from consensus: ML-DSA-65 access keys do not make validator, staking, block, or chunk signatures post-quantum.
- SLH-DSA (NIST FIPS 205) is not a NEAR transaction-key scheme in this experiment. It may be evaluated off-chain in a separate test harness, but it must not authorize QPC execution until a supported verification and trust path exists.
- ML-KEM (NIST FIPS 203) is key establishment, not a digital signature scheme.
- The published [x402 exact-NEAR scheme](https://github.com/x402-foundation/x402/blob/main/specs/schemes/exact/scheme_exact_near.md) lists Ed25519 and secp256k1 signatures, not ML-DSA-65. Do not describe that x402 signing path as post-quantum.

## Experiment A — protocol-level ML-DSA-65 transaction authentication

1. Use a disposable NEAR testnet account and a node reporting protocol version 85 or later.
2. Generate an ML-DSA-65 keypair using a maintained NEAR-compatible signer. Keep the private key local; never place it in this repository, a contract, CI logs, or a public issue.
3. Register the ML-DSA-65 public key as an access key using an existing authorized key. Confirm the RPC access-key view returns the expected ML-DSA-65 key handle.
4. Create a QPC policy for the test account with a low-value, non-production action and a strict cap.
5. Submit a QPC contract call signed by the ML-DSA-65 access key.
6. Verify the transaction outcome and QPC execution record through RPC. Record protocol version, transaction hash, signer account, key handle, gas burnt, and receipt outcomes.
7. Negative tests: invalid signature must be rejected by the runtime; a changed transaction payload must not execute; a valid signature from an unregistered key must not authorize the account; QPC policy nonce replay and changed argument bytes must be rejected by QPC.
8. Repeat with an ordinary Ed25519 access key as a baseline. Compare serialized transaction size and gas costs without claiming the test proves quantum security.

## Experiment B — SLH-DSA off-chain research only

- Use a maintained FIPS 205 implementation and published known-answer test vectors.
- Test key generation, signing, valid verification, altered-message rejection, altered-signature rejection, malformed-length rejection, and deterministic serialization.
- Measure key/signature sizes and signing/verification latency on the target device.
- Label all output **off-chain cryptographic experiment**. Do not pass an SLH-DSA result to QPC as an authorization proof.

## Required evidence before any public demo

- Exact NEAR node/protocol version and network.
- Reproducible commands and tool versions (with no private-key material).
- Successful ML-DSA-65 access-key registration and QPC invocation.
- Negative test showing a tampered signature is rejected before contract execution.
- QPC state evidence showing policy enforcement after runtime authentication.
- Baseline comparison with Ed25519.
- Gas, transaction-size, and latency measurements with methodology.
- Clear disclosure that SLH-DSA is off-chain-only and QPC has no in-contract PQ verifier.

## Current QPC contract limitations (not solved by the PQ experiment)

The existing contract still needs NEAR Sandbox/runtime coverage for generated wrappers, actual promise receipt routing, callback privacy, gas exhaustion, and storage behavior. Unit tests that call Rust methods directly do not prove generated `#[payable]` or `#[private]` wrapper behavior. No testnet deployment is authorized by this document.
