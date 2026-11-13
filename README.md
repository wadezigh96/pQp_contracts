# QPC — On-chain Guardrails for AI Agents

**QPC** adalah prototipe kontrak NEAR yang membatasi aksi agent on-chain dengan policy, lalu mensyaratkan tanda tangan intent **ML-DSA-65 (FIPS 204)** sebelum transfer native NEAR dijalankan. AI boleh menyusun usulan aksi; kontrak memeriksa izin dan signature secara deterministik.

QPC bukan “anti-hacker” mutlak. ML-DSA melindungi autentikasi intent terhadap ancaman komputasi kuantum yang diketahui, tetapi tidak mencegah pencurian private key, kompromi owner, bug runtime, supply-chain attack, atau kesalahan manusia. Lihat [desain dan threat model QPC](QPC-NEAR-DESIGN.md).

## Implementasi saat ini

- **NEAR — QPC** (`chains/near/contract/`): kontrak native Rust/NEAR SDK. Signed intent mengikat source/destination chain ID, derivation-path label, contract, agent, penerima, jumlah, nonce, expiry, dan versi policy; semua binding dicek sebelum Promise dibuat dan dicatat dalam execution-audit events. Saat ini destination wajib sama dengan chain NEAR aktif karena hanya transfer native NEAR yang diimplementasikan.
- **Ethereum — legacy Solidity baseline** (`contracts/QPCAgentFinance.sol`, `chains/ethereum/`): policy guardrails EVM lama yang nama filenya kini konsisten dengan QPC, tetapi bukan adapter post-quantum QPC; belum memverifikasi ML-DSA dan tidak boleh dipasarkan sebagai quantum-resistant.
- **BNB Chain** (`chains/bnb/`): rencana integrasi/deployment EVM terpisah; saat ini belum ada kontrak QPC BNB atau address deployment.
- **Solana** (`chains/solana/`): spesifikasi native Solana tersendiri. Belum ada Solana program dalam repositori ini; QPC NEAR bytecode tidak dapat dijalankan di Solana.

Tidak ada bridge, common cross-chain signer, atau klaim bahwa tanda tangan post-quantum NEAR otomatis melindungi deployment di chain lain.

## Struktur repositori

```text
contracts/QPCAgentFinance.sol       Baseline EVM lama; bukan implementasi post-quantum QPC NEAR
chains/near/contract/               QPC contract native NEAR + Rust tests
chains/near/README.md               Instruksi build/test serta status NEAR
chains/ethereum/README.md           Batas deployment Ethereum
chains/bnb/README.md                Batas deployment BNB Chain
chains/solana/README.md             Desain native Solana dan status belum diimplementasikan
QPC-NEAR-DESIGN.md                  Arsitektur, properti keamanan, threat model, dan sumber
```

## Build dan tes lokal NEAR

Toolchain Rust dan dependency NEAR SDK dipin. Instal `wasm32-unknown-unknown` bila belum ada, lalu jalankan:

```bash
source "$HOME/.cargo/env"
rustup target add wasm32-unknown-unknown
cd chains/near/contract
cargo fmt --check
cargo test --locked
cargo build --locked --release --target wasm32-unknown-unknown
```

WASM hasil build berada di `target/wasm32-unknown-unknown/release/qpc_near_policy.wasm`.

Dua belas unit test saat ini membuat keypair ML-DSA-65 dan memverifikasi signature menggunakan verifier runtime/mock NEAR. Test tersebut mencakup intent valid, perubahan payload/penerima, replay/expiry/policy version, batas kumulatif, settlement callback sukses/gagal, 64 langkah state-machine pseudo-random, mismatch chain/path, validasi input kosong/control-character, audit record, serta recovery stale transfer yang owner-attested. Ini bukan fuzz campaign penuh dan belum menggantikan tes sandbox lintas-receipt, audit independen, atau formal verification.

## Status keamanan dan rilis

**Eksperimental / belum diaudit / belum dideploy.** Tidak ada dana nyata yang boleh dikirim ke kontrak ini sebelum audit independen dan validasi sandbox/testnet.

- Otorisasi ML-DSA berlaku untuk signed intent QPC. Pengirim transaksi NEAR boleh menjadi relayer; private key ML-DSA tetap harus disimpan di signer terpisah yang aman, bukan di proses LLM.
- Policy saat ini hanya mengotorisasi transfer native NEAR ke satu penerima per agent. Belum mengotorisasi arbitrary function calls, FT, NFT, bridge, atau cross-chain message.
- `destination_chain_id` harus cocok dengan NEAR runtime; `derivation_path` adalah label terikat signature/policy, bukan bukti derivasi key.
- Pause/revoke menghentikan permintaan baru, tetapi tidak membatalkan promise transfer yang sudah terkirim. Callback memakai 40 Tgas; bila callback receipt tidak settle, owner dapat merekonsiliasi setelah 10 menit dan wajib memeriksa outcome transfer on-chain terlebih dahulu. Ini jalur manual yang mempercayai owner; perilaku receipt dan kebutuhan gas tetap harus diuji di NEAR Sandbox.
- Owner adalah root authority. Gunakan akun multisig/DAO untuk deployment serius; implementasi ini belum menyediakan timelock atau threshold governance sendiri.
- Protokol ML-DSA tidak melindungi dari kompromi key, bug contract/runtime, serangan ekonomi di aplikasi penerima, atau chain outage.

## Riset lanjutan

Jalankan fuzz/property testing dan invariant campaign; simulasi callback success/failure, recipient invalid, policy expiry/revoke/pause saat transfer pending, dan beban storage/gas; lakukan pemeriksaan formal atas invariant akuntansi; serta minta audit independen untuk setiap chain adapter sebelum deployment.
