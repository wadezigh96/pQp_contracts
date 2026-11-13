# QPC di BNB Chain

## Status

**QPC belum diimplementasikan atau dideploy di BNB Chain.** BNB Chain kompatibel dengan EVM, sehingga kontrak Solidity dapat menjadi basis code setelah adaptasi dan review; itu tidak membuat deployment berbagi state, governance, saldo, atau keamanan dengan Ethereum.

`contracts/QPCAgentFinance.sol` adalah code EVM sebelumnya. Ia tidak memakai verifier ML-DSA dan bukan quantum-resistant. Tidak ada bridge atau sinkronisasi otomatis dengan QPC NEAR.

## Ketentuan deployment terpisah

- BNB Smart Chain mainnet menggunakan chain ID `56`; testnet menggunakan chain ID `97` ([dokumentasi JSON-RPC resmi BNB Chain](https://docs.bnbchain.org/bnb-smart-chain/developers/json_rpc/json-rpc-endpoint/)).
- Gunakan address contract, owner/multisig, konfigurasi, RPC, monitoring, deployment key, dan audit tersendiri.
- Signed-intent domain wajib memuat chain ID dan verifying contract address agar tidak dapat dipakai ulang pada Ethereum atau BNB deployment lain.
- Periksa perilaku gas, opcode/runtime compatibility, token, upgrade governance, dan liquidity/MEV risk pada target chain; jangan menganggap hasil tes Ethereum otomatis berlaku.
- Host ML-DSA NEAR tidak tersedia otomatis di BNB EVM. Quantum-resistance memerlukan verifier dan test khusus EVM yang belum dibangun.

Sebelum ada implementasi, lakukan unit/fuzz/invariant tests, simulasi fork, static analysis, review compiler/dependency, audit independen, dan pengujian recovery/multisig. QPC di NEAR tidak mengamankan BNB Chain.
