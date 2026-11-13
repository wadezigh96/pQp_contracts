# Capability Map: Quantum Persistent L1 on NEAR

## Produk target

**Quantum Persistent L1 (QPC)** adalah control plane NEAR-native untuk agent on-chain. Agent dapat membuat keputusan privat, meminta policy yang terikat pada target L1 tertentu, lalu mengeksekusi transaksi outbound melalui NEAR Chain Signatures atau settlement berbasis NEAR Intents. Kontrak NEAR tetap menjadi enforcement layer; AI, solver, dan provider tidak dipercaya sebagai otoritas.

## Modul dan dependency

| Module id | Responsibility | Depends on |
|---|---|---|
| `near-policy-core` | Policy, agent capabilities, nonce, expiry, pause/revoke, action hash, chain/network binding | — |
| `cyber-defense-core` | Risk controls, destination/selector allowlist, rate limits, circuit breaker, incident quarantine, and security events | `near-policy-core` |
| `chain-signature-executor` | Membentuk payload outbound yang chain-specific dan mengorkestrasi request signature MPC | `near-policy-core` |
| `intent-settlement-adapter` | Adapter outcome/solver untuk NEAR Intents, status, dan receipt verification | `near-policy-core` |
| `private-agent-runtime` | Policy reasoning privat dan verifiable decision envelope melalui NEAR AI Cloud/TEE | `near-policy-core` |
| `privacy-envelope` | Opaque recipient reference, encrypted metadata boundary, commitment/hash, anti-replay binding | `near-policy-core`, `private-agent-runtime` |
| `onchain-observability` | Events, request lifecycle, signed payload hash, receipt proof, audit query model | semua modul |
| `hackathon-demo` | Reproducible demo: agent decision -> NEAR policy -> outbound L1 request -> verified result | semua modul |

## Build order

`near-policy-core` -> `cyber-defense-core` -> `privacy-envelope` -> `chain-signature-executor` -> `private-agent-runtime` -> `intent-settlement-adapter` -> `onchain-observability` -> `hackathon-demo`

## Boundary decisions

Kontrak NEAR tidak menyimpan private key dan tidak mempercayai hasil model. Chain Signatures dipakai untuk outbound signing dengan derivation path serta network/chain ID yang eksplisit. NEAR Intents adalah adapter settlement opsional, bukan bagian dari policy authority. Private runtime menjaga input/keputusan sensitif, tetapi settlement yang telah broadcast tetap diperlakukan sebagai data publik pada chain tujuan.

Modul `cyber-defense-core` adalah firewall/policy layer on-chain untuk agent. Ia membatasi target chain, chain ID, derivation path, method/action, amount, call frequency, request nonce, dan emergency state. Ia juga menyediakan quarantine/circuit-breaker berbasis incident ID agar owner atau security guardian dapat menghentikan satu agent atau seluruh outbound execution tanpa menghapus audit trail. Ia tidak mencoba mendeteksi malware dari bytecode atau menggantikan audit smart contract.

Zcash bukan implementasi utama pada tahap pertama. Ia menjadi target adapter setelah executor, privacy envelope, dan receipt verification sudah nyata. Demo pertama harus menggunakan satu L1 target yang memiliki payload dan verifier yang dapat diuji secara reproducible, lalu menunjukkan bagaimana target lain dapat ditambahkan tanpa mengubah policy core.
