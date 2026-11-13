# QPC di Solana

## Status

**Belum ada program Solana QPC dalam repositori ini.** Ini adalah batas arsitektur dan rencana implementasi native; file Rust/NEAR tidak dapat dijalankan sebagai Solana program.

## Arsitektur yang diperlukan

Implementasi Solana perlu program dan test tersendiri (misalnya Anchor), instruksi policy/execution, PDA untuk menyimpan state/authority, validasi account constraints, aturan lamports dan SPL Token, serta mekanisme emergency pause dan revoke yang cocok dengan model Solana. Semua akun yang writable/signers dan setiap CPI harus dibatasi eksplisit.

Verifier `env::ml_dsa_verify` hanya disediakan runtime NEAR; itu bukan precompile atau syscall Solana. Sebelum menyebut QPC Solana post-quantum, harus dipilih dan diaudit jalur verifikasi ML-DSA yang sesuai untuk compute budget Solana, disertai test vector FIPS 204, benchmark, fuzzing, invariant tests, dan audit independen.

## Isolasi lintas-chain

- NEAR signature tidak mengotorisasi Solana instruction.
- Jangan mempercayai event/API dari NEAR sebagai bukti lintas-chain tanpa mekanisme verifikasi proof dan finality yang dirancang khusus.
- Setiap intent harus mengikat Solana cluster/genesis hash, program ID, agent, instruction, account set, amount, nonce, expiry, serta policy version.
- Bridge, relayer, validator, dan signer governance merupakan threat domain terpisah dan belum termasuk implementasi QPC.

Tidak ada Solana deployment yang dibuat. QPC Solana harus menjadi audit target independen, bukan port langsung dari kontrak NEAR atau Solidity.
