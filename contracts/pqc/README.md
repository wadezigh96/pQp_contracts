# QPC Post-Quantum Cryptography Boundary

PQC is a cryptographic subsystem, not a blockchain adapter.

Target standards:
- FIPS 204 ML-DSA and FIPS 205 SLH-DSA: digital signatures.
- FIPS 203 ML-KEM: key establishment, not transaction signing.

No PQC algorithm is implemented in this directory yet. Integration requires a maintained audited implementation, official known-answer tests, strict input bounds, canonical encoding, domain separation, chain/application binding, replay protection and independent cryptographic review. Do not claim quantum-safe or quantum-proof status before this work is complete.
