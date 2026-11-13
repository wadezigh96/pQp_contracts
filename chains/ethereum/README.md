# QPC di Ethereum

## Status

**Adapter QPC Ethereum post-quantum belum diimplementasikan atau dideploy.** `contracts/QPCAgentFinance.sol` adalah baseline Solidity lama dengan policy guardrails, terpisah dari QPC NEAR. Ia tidak memverifikasi tanda tangan ML-DSA dan tidak boleh disebut quantum-resistant.

## Rencana pemisahan

Ethereum memakai deployment sendiri (chain ID `1`), address kontrak, owner/multisig, policy storage, test, release artifact, dan key management yang terpisah dari NEAR maupun BNB Chain. Signed intent masa depan harus memasukkan chain ID dan alamat verifying contract pada domain yang ditandatangani untuk mencegah replay lintas deployment.

NEAR `env::ml_dsa_verify` adalah host function NEAR; ia tidak tersedia otomatis di EVM. Implementasi QPC Ethereum memerlukan verifier yang secara khusus sesuai dengan EVM (misalnya primitive/protocol support yang tersedia atau implementasi terverifikasi), perhitungan gas, test vector FIPS 204, fuzzing, dan audit independen. Jangan menyalin asumsi gas atau serialization NEAR ke Solidity.

## Gerbang sebelum implementasi

1. Pilih arsitektur signer/relayer serta format intent EIP-712 yang chain- dan contract-bound.
2. Tentukan ML-DSA verification path yang dapat dipertanggungjawabkan pada EVM.
3. Bangun policy contract/wallet adapter tersendiri; jangan menjadikan hasil NEAR sebagai otoritas.
4. Jalankan unit, fuzz, invariant, fork/simulation tests, static analysis, serta audit independen.
5. Deploy hanya setelah review deployment key, compiler, bytecode reproducibility, dan multisig owner.

QPC NEAR tidak melindungi deployment Ethereum, bridge, atau EVM wallet.
