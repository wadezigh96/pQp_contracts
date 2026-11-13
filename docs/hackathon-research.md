# QPC / Quantum Persistent L1 — Hackathon Research Notes

Tanggal riset: 2026-11-13

## Arah hackathon

NEAR HACKATHON meminta public repository dan public demo post. README harus memberi jalur reproducible dengan instruksi "do X -> see Y". Tidak ada integrasi wajib; integrasi NEAR yang nyata dan kontrak milik proyek sudah cukup. Proyek paling relevan dengan track **Privacy & Private AI**, **AI Security & Defense**, **Agent Economy**, dan **Open Innovation**.

## Temuan teknis

- **NEAR Chain Signatures** memungkinkan akun NEAR, termasuk smart contract, meminta tanda tangan untuk transaksi di banyak chain melalui MPC. Integrasi ini bersifat outbound: signing lintas-chain tidak sama dengan membaca state chain lain.
- **Derivation path harus spesifik per chain** agar payload tidak ambigu atau replayable lintas jaringan. Payload juga harus mengikat network/chain ID.
- **NEAR Intents** memodelkan desired outcome dan menggunakan solver/market maker untuk fulfillment. 1Click API dapat dipakai sebagai settlement adapter, tetapi provider SUCCESS tetap perlu diverifikasi terhadap receipt tujuan.
- **NEAR AI Cloud** menyediakan private inference melalui TEE dan verifikasi interaksi. Ini cocok untuk menjaga policy reasoning, recipient reference, dan agent context dari model/provider, tetapi tidak membuat transaksi blockchain tujuan menjadi private.
- **Private transaction semantics:** proyek harus membedakan tiga hal: private decision/input, confidential recipient mapping, dan public on-chain settlement. Hanya dua yang pertama yang dapat dijanjikan oleh desain TEE/opaque reference; transaksi yang sudah broadcast tetap dapat dianalisis di chain tujuan.

## Implikasi arsitektur

Produk tidak boleh lagi diposisikan sebagai Ethereum policy contract dengan adapter Zcash lokal. Produk menjadi **NEAR-native QPC control plane**:

1. Kontrak NEAR menyimpan policy, agent capability, nonce, expiry, target chain, derivation path, chain ID, dan action hash.
2. Agent privat membuat keputusan di NEAR AI Cloud/TEE.
3. Kontrak NEAR melakukan preflight deterministik dan meminta Chain Signature untuk payload outbound.
4. Adapter Intent/solver menangani settlement berbasis outcome bila diperlukan.
5. Verifier independen mengikat signed payload dan receipt ke request ID sebelum state menjadi SETTLED.

## Batasan yang harus dinyatakan

- Tidak boleh mengklaim Zcash shielded broadcast end-to-end tanpa router, wallet, solver, dan receipt verifier yang nyata.
- Tidak boleh mengklaim post-quantum signatures hanya karena nama Quantum Persistent. NEAR Chain Signatures saat ini memakai domain Secp256k1 atau Ed25519 pada interface yang didokumentasikan.
- Tidak boleh menganggap policy callback off-chain sebagai enforcement on-chain.
- Tidak boleh mengirim submission hackathon, deploy mainnet, push, atau membuat repository public tanpa persetujuan eksplisit pengguna.

## Sumber

- https://nearbuilders.org/build — NEAR Builders: Smart Contracts, NEAR AI Cloud, Agentic Commerce.
- https://x.com/NEARProtocol/status/2112257967818997891 — pengumuman NEAR HACKATHON dan tracks.
- https://nearhackathon.com/HACKATHON.md — submission contract, README/demo guidance, rules, dan integrasi yang eligible.
- https://docs.near.org/chain-abstraction/chain-signatures — Chain Signatures, MPC, derivation paths, sign interface, dan replay warning.
- https://docs.near.org/chain-abstraction/what-is — relasi Chain Signatures, Intents, dan OmniBridge.
- https://docs.near.ai/ — private inference melalui TEE dan verification.
- https://docs.near-intents.org/index — intents, 1Click API, solver/market maker, dan verifier contract.
