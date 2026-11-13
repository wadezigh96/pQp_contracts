# Optional hybrid agent authentication prototype

This module implements **off-chain** AND-composition of Ed25519 and ML-DSA-65 for the existing `verifyAgentAuthorization` integration hook. Both signatures must verify over identical domain-separated bytes. It does not activate itself, replace chain account authentication, implement a PQ wallet, or make the NEAR/EVM/Solana owner keys quantum-resistant. It is unaudited and must not be described as production-ready.

## Reproduce

```bash
npm ci --prefix agent-router/pq-auth --ignore-scripts --no-audit --no-fund
npm test --prefix agent-router/pq-auth
```

The pinned dependency is `@noble/post-quantum` 0.7.1; the lockfile pins transitive packages and integrity hashes. The tests generate ephemeral dummy keys in memory only; one rejection test serializes a dummy private key in memory to confirm that trust configuration refuses it. No private keys are written to disk, logged or sent. Native ML-DSA is not available through this machine's Node 22 crypto API, so the implementation uses the explicit library rather than silently substituting a classical signature.

## Operator integration

Import `createHybridAgentVerifier` and `hybridAuthorizationMessage` from `index.mjs`. Construct a verifier with an operator-provided `trustedKeys` array and an untrusted `lookupProof` function. Pass the returned function as `verifyAgentAuthorization` to `authorizeCrossL1Envelope`. For enforced application-level authentication, configure `new FileIntentStore(root, { authorizationVerifier: verifier })`: this store instance rejects any per-call replacement hook. The optional legacy per-call hook remains available only when no constructor verifier is pinned. Reopened store instances must receive the same operator configuration; this does not defend against a privileged process opening the database with a different configuration or writing it directly.

Each trusted entry contains:

- `agentIdentity`: exact `namespace`, `id`, `keyId` tuple matching the canonical envelope;
- `keyEpoch`: positive integer identifying the currently approved key generation;
- `ed25519PublicKey`: a public `KeyObject` or SPKI `PUBLIC KEY` PEM string; private keys, PKCS8 inputs and ambiguous input objects are rejected;
- `mlDsa65PublicKey`: ML-DSA-65 public-key bytes (`Uint8Array`).

The lookup receives only the canonical commitment, immutable identity, exact suite and epoch. It must return exactly `suite`, `keyEpoch`, `ed25519Signature` and `mlDsa65Signature`. Signature fields are byte arrays, not JSON strings. Any transport must decode with strict size limits before invoking the verifier. Unknown keys, malformed evidence, exceptions, wrong epoch/suite, missing signatures and failed verification return false. The lookup cannot select public keys or substitute a verifier. Lookup timeout defaults to 5000 ms and can be configured with `lookupTimeoutMs` in the range 1..30000 ms. Timeout returns false and aborts the supplied second argument's `signal`; transports should honor it. An ignored abort cannot cancel remote work, and synchronous event-loop blocking must be isolated at the service/process boundary.

Both signatures cover `hybridAuthorizationMessage(envelope, keyEpoch)`, which binds the fixed suite, key epoch, identity tuple and SHA-256 commitment of the complete canonical envelope. The fixed protocol domain is separate from any chain transaction-signing domain. Envelope timestamps and durable replay consumption remain the responsibility of the existing authorization path, not this signature-only primitive.

Trusted keys are copied at construction. Rotation or revocation requires constructing and installing a new verifier atomically; existing closures intentionally retain their old snapshot. Operators must not leave old closures active. Reusing an epoch with different keys, configuration rollback, malicious operator key replacement, distributed configuration rollout, custody, compromised signing clients and side channels are not solved here. Only store instances configured with the constructor-pinned verifier enforce this hook. Callers using unconfigured store instances, a different trusted hook or direct chain-owner access remain outside its enforcement.

## Limits

These are self-generated local interoperability and adversarial tests, not an independent FIPS validation, ACVP known-answer campaign, external cryptographic audit, proof of secure key custody, chain gas benchmark, or on-chain enforcement. ML-KEM is not an authorization signature and is not implemented. SLH-DSA, PQ recovery and transport privacy are not implemented. A complete quantum-resistant product requires reviewed enforcement and recovery across every authority capable of moving funds, including classical L1 owners.
