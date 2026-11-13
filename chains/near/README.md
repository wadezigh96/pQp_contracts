# QPC di NEAR

Implementasi QPC yang aktif adalah kontrak Rust native NEAR di `contract/`. Kontrak memverifikasi signed intent ML-DSA-65 menggunakan `env::ml_dsa_verify`, lalu mengotorisasi transfer native NEAR yang dibatasi oleh policy.

## Binding policy dan execution

Setiap policy agent menyimpan `destination_chain_id` dan `derivation_path`. Untuk implementasi native NEAR saat ini, destination chain ID wajib valid dan sama dengan `env::chain_id()` dari runtime aktif. Ini menolak payload yang ditujukan untuk chain lain; QPC belum mengirim transaksi lintas-chain dan tidak mengklaim sebagai bridge.

`derivation_path` menggunakan format label path hierarkis terbatas seperti `m/44'/397'/0'`. Kontrak memvalidasi sintaks, panjang, segmen, dan kesamaan dengan policy, lalu mengikat nilainya ke signed payload Borsh. **Kontrak tidak menurunkan key ML-DSA dari path dan tidak membuktikan bahwa public key berasal dari path tersebut.** Path adalah metadata binding yang harus disepakati dan diverifikasi signer di luar chain.

Sebelum membuat Promise transfer, kontrak memvalidasi panjang/karakter chain ID dan path, memastikan chain ID sama dengan runtime dan policy, memastikan path sama dengan policy, lalu memverifikasi signature yang mengikat field tersebut bersama source chain ID, contract ID, agent, recipient, amount, nonce, expiry, dan policy version.

Signed agent-intent execution menyimpan source chain ID, destination chain ID, derivation-path label, operation ID, agent, recipient, jumlah, nonce, dan policy version dalam pending audit record. Kontrak mengeluarkan event `EVENT_JSON` NEP-297 `execution_audit` saat status `pending`, lalu saat `succeeded`, `failed`, atau owner-attested reconciliation. Angka dicatat sebagai string. Record in-state pending dihapus ketika settled; riwayat bergantung pada event logs. Owner withdrawal belum termasuk dalam event audit ini.

Callback dialokasikan 40 Tgas, tetapi gas aktual belum diukur dalam NEAR Sandbox. Jika callback receipt sendiri tidak settle selama 10 menit, method `reconcile_stale_transfer` hanya dapat dipanggil owner. Owner wajib memeriksa receipt transfer on-chain dan menentukan apakah transfer sukses sebelum memilih hasil rekonsiliasi. Salah memilih `transfer_succeeded` dapat merusak accounting; ini emergency recovery berbasis trust owner, bukan proof otomatis. Callback yang datang setelah rekonsiliasi tidak mengubah pembukuan lagi.

## Perilaku guardrail

- Policy menyimpan key hash ML-DSA-65, recipient tunggal, chain ID tujuan, path label, cap per transaksi dan total, expiry, nonce, serta policy version.
- Signed intent memakai encoding Borsh dan nonce monotonik. Chain ID sumber berasal dari runtime NEAR, bukan input caller.
- Maksimum policy 30 hari; signed intent berlaku maksimal 5 menit; maksimum 8 transfer agent pending.
- Pembukuan mencadangkan jumlah sebagai `reserved`; callback sukses memindahkannya menjadi `spent`, sementara callback transfer gagal memulihkan saldo tersedia.
- Owner dapat pause/revoke. Pause/revoke tidak membatalkan Promise yang sudah dikirim.
- Implementasi hanya mengatur transfer native NEAR ke satu recipient per agent. Tidak ada FT/NFT, arbitrary contract calls, bridge, light client, atau multi-chain execution.

## Build dan test lokal

Toolchain dan dependency dipin pada `rust-toolchain.toml`, `Cargo.toml`, dan `Cargo.lock`.

```bash
source "$HOME/.cargo/env"
rustup target add wasm32-unknown-unknown
cd chains/near/contract
cargo fmt --check
cargo test --locked
cargo build --locked --release --target wasm32-unknown-unknown
```

WASM output: `target/wasm32-unknown-unknown/release/qpc_near_policy.wasm`.

Dua belas tes lokal yang lulus mencakup signature ML-DSA nyata, replay/tampering/expiry/policy-version, cap kumulatif dan 64 langkah pseudo-random accounting invariant, callback sukses/gagal, mismatch destination chain dan derivation path sebelum Promise, input kosong/control-character pada konfigurasi maupun execution, binding pada pending record dan settlement events, serta recovery stale transfer sukses/gagal dan callback idempotency. Tes ini tidak menjalankan callback receipt yang benar-benar out-of-gas dan belum membuktikan runtime receipt, gas, storage, atau perilaku Sandbox.

## Status dan deployment

Prototipe lokal ini **belum diaudit dan belum dideploy**. Tidak ada deployment testnet yang dijalankan dalam pekerjaan ini. Sebelum deployment, tambahkan NEAR Sandbox integration tests untuk receipt success/failure, callback failure, gas/storage, pause/revoke ketika transfer pending, lalu lakukan fuzz/invariant campaign dan audit independen. Jangan menyimpan private key ML-DSA di agent/LLM process.

Lihat [QPC-NEAR-DESIGN.md](../../QPC-NEAR-DESIGN.md) untuk threat model dan batas keamanan. Ethereum memiliki baseline Solidity lama terpisah tetapi belum adapter ML-DSA QPC; BNB Chain dan Solana belum memiliki implementasi QPC.
