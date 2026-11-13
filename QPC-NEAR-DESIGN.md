# QPC di NEAR — desain dan threat model

## Ringkasan

**QPC** adalah nama proyek yang dipilih pengguna. Dokumen ini tidak menetapkan kepanjangan baru untuk akronim tersebut.

QPC adalah execution guard untuk AI agent: AI/automation menyusun intent, kunci ML-DSA-65 yang terdaftar menandatangani payload terstruktur, dan kontrak NEAR memverifikasi signature serta policy sebelum transfer native NEAR dijadwalkan. Relayer boleh mengirim transaksi; relayer bukan otoritas intent. Kontrak membatasi agent ke satu penerima, destination chain ID yang sama dengan chain NEAR runtime, derivation-path label terdaftar, nilai per transaksi dan kumulatif, nonce, policy version, expiry, jumlah receipt pending, serta pause/revoke.

QPC tidak membuat AI aman atau kontrak kebal hacker. Ia memperkecil kewenangan kunci agent dan menggunakan algoritme tanda tangan post-quantum untuk otorisasi intent.

## Mengapa QPC berbeda dari security tooling biasa

QPC membawa enforcement ke jalur transaksi on-chain. Ia tidak hanya memindai kode atau memberi peringatan setelah aksi terjadi: recipient, jumlah, nonce, expiry, versi policy, alamat kontrak, source chain ID, destination chain ID, dan derivation-path label menjadi bagian dari pesan yang ditandatangani. Contract memvalidasi chain/path binding sebelum promise transfer dibuat.

AI tidak dianggap sebagai batas keamanan. Model dapat salah, prompt dapat dimanipulasi, dan runtime agent dapat dikompromikan. Sistem hanya menerima intent yang signature-nya valid dan masih berada dalam policy. Untuk mengurangi risiko pencurian kunci, proses AI sebaiknya hanya mengusulkan intent; signer ML-DSA ditempatkan di komponen terpisah dengan kontrol operasional yang sesuai.

## Alur otorisasi

```text
AI agent menyusun usulan
        ↓
Signer terpisah membentuk dan menandatangani payload QPC
        ↓
Relayer mengirim intent ke kontrak NEAR
        ↓
QPC memeriksa format, source/destination chain ID, derivation-path label,
domain, key hash, signature ML-DSA-65,
nonce, versi policy, expiry, recipient, cap, pause, dan saldo
        ↓
QPC mencadangkan jumlah sebelum mengirim Promise transfer
        ↓
Callback menyelesaikan pencatatan: sukses → spent; gagal → dana tersedia dipulihkan
```

Pesan menggunakan encoding Borsh deterministik dan mencakup domain `QPC/NEAR/ML-DSA-65/INTENT`, versi format, `env::chain_id()` dari runtime, `destination_chain_id`, `derivation_path`, account kontrak, agent, recipient, jumlah yoctoNEAR, nonce, batas waktu intent, dan policy version. Untuk transfer native NEAR saat ini, destination chain ID harus sama dengan runtime chain ID dan dengan nilai policy. Host verifier NEAR memverifikasi bytes pesan apa adanya dengan context kosong; QPC tidak melakukan pre-hash atas pesan intent.

Derivation path dibatasi ke format label hierarkis kanonis, disimpan dalam policy, dan harus cocok persis dengan path di signed intent. Kontrak **tidak** melakukan key derivation dan tidak membuktikan bahwa public key ML-DSA berasal dari path itu. Artinya path adalah replay/domain metadata yang diikat on-chain; signer eksternal tetap bertanggung jawab atas relasi path-key.

Untuk signed agent intent, QPC menyimpan `ExecutionAuditRecord` di state selama transfer masih pending dan mengeluarkan NEP-297-style `EVENT_JSON` `execution_audit` saat pending dan saat settlement. Data event berisi `operation_id`, `source_chain_id`, `destination_chain_id`, `derivation_path`, `agent`, `recipient`, `amount_yocto`, `nonce`, `policy_version`, dan `status`; angka diserialisasi sebagai string. Pending state record dihapus ketika callback/reconciliation settle, sehingga riwayat setelah itu bergantung pada event logs. Ini bukan audit trail lengkap: caller/relayer, signature, key fingerprint, dan detail kegagalan tidak dicatat. Owner withdrawals juga tidak termasuk dalam event ini.

## Primitive quantum yang digunakan

- **ML-DSA-65 / FIPS 204** adalah skema tanda tangan post-quantum yang digunakan untuk menandatangani intent. NIST menyatakan ML-DSA dirancang agar dipercaya aman terhadap adversary dengan komputer kuantum skala besar; ini bukan perlindungan dari pencurian private key atau semua kemungkinan serangan masa depan. [1]
- NEAR SDK menyediakan `env::ml_dsa_verify`; API menerima signature mentah 3.309 byte, pesan, dan public key mentah 1.952 byte. Panjang salah membuat host call gagal, sehingga QPC memeriksa panjang sebelum memanggil verifier. [2]
- Dokumentasi SDK menyatakan host function tersedia mulai protocol version 87; release nearcore 2.14/protocol 88 memperkenalkannya. Pembacaan status RPC read-only pada 12 November 2026 melaporkan protocol version 88 untuk mainnet dan testnet. [2] [3] [9] [10]
- NEAR juga mendukung access key ML-DSA-65 untuk penandatanganan transaksi. QPC signed-intent signature adalah lapisan aplikasi terpisah; transaction yang membawa intent dapat dibayar/dikirim relayer. Menggunakan ML-DSA untuk intent tidak secara otomatis mengubah skema kunci akun relayer. [5] [6]
- Public key ML-DSA-65 besar dan keypair acak tidak memiliki seed phrase standar. Backup dan custody key harus menjadi bagian desain operasional; jangan menaruh private key di prompt, log, atau proses model.

Istilah yang dipakai adalah **post-quantum** atau **quantum-resistant terhadap asumsi saat ini**, bukan “quantum-proof”. Tidak ada jaminan bahwa algoritme, implementasi, compiler, runtime, atau seluruh sistem akan aman dari semua penemuan di masa depan.

## Properti keamanan yang dituju

1. **Intent tidak dapat diubah:** setiap perubahan pada recipient, amount, nonce, expiry, policy version, agent, chain ID, atau kontrak menghasilkan payload berbeda dan signature tidak cocok.
2. **Binding chain/path eksplisit:** `destination_chain_id` harus valid, sama dengan chain ID runtime dan policy, sementara `derivation_path` harus valid dan cocok dengan policy sebelum Promise dibuat. Kedua field ikut ditandatangani dan disimpan dalam pending audit record serta execution events.
3. **Tidak ada replay:** nonce harus sama dengan `next_nonce`, lalu dinaikkan hanya setelah signature lolos pemeriksaan dan transfer dicadangkan.
4. **Policy tidak dapat dilampaui:** `spent + reserved + requested <= total_cap`, dan `requested <= per_tx_limit`.
5. **Penerima dibatasi:** policy tiap agent menunjuk satu recipient native NEAR.
6. **Izin berumur pendek:** policy maksimal 30 hari; tiap intent maksimal 5 menit dan tidak boleh melewati expiry policy.
7. **Perubahan policy membatalkan intent lama:** policy version meningkat pada konfigurasi ulang.
8. **Receipt async dicatat konservatif:** jumlah pending masuk `reserved`; callback sukses memindahkannya ke `spent`; callback transfer gagal memulihkannya ke saldo tersedia.
9. **Operasi darurat:** owner dapat pause/revoke; permintaan baru ditolak saat paused/inactive.
10. **Batas pending:** maksimal 8 transfer agent belum selesai secara bersamaan.
11. **Saldo operasional:** pemakaian agent dan withdrawal owner mempertahankan reserve minimum 0,1 NEAR dari saldo yang dikelola kontrak.

## Threat model dan batas residual

| Ancaman | Mitigasi QPC saat ini | Yang tetap tidak diselesaikan |
|---|---|---|
| AI agent disusupi, prompt injection, atau bertindak di luar instruksi | AI tidak dapat melewati policy recipient, cap, nonce, expiry, signature, pause | Bila signer otomatis menandatangani intent berbahaya, kontrak tidak memahami semantik instruksi AI; pisahkan signer dari runtime agent dan terapkan monitoring/approval di luar kontrak |
| Intent dipakai pada chain atau derivation path yang berbeda | Source chain berasal dari runtime; destination chain harus sama dengan chain aktif dan policy; path divalidasi, dicocokkan ke policy, dimasukkan ke signature, dan ditulis ke audit record | Path tidak membuktikan cara key dibuat. Implementasi tidak mengirim ke chain lain; bridge atau derivasi HD memerlukan mekanisme terpisah |
| Pencurian private key agent | Cap per aksi/kumulatif, recipient tunggal, expiry, nonce, revoke/pause | Penyerang yang menguasai key dapat membelanjakan dalam batas aktif sampai policy dicabut; ML-DSA tidak mencegah pencurian key |
| Ancaman quantum | ML-DSA-65 untuk signature intent; chain ID/nonce/expiry mencegah replay | Hanya jalur intent QPC di NEAR; bukan migrasi semua kunci di EVM/Solana, bukan perlindungan key custody, dan bukan jaminan terhadap kriptanalisis masa depan |
| Owner/governance dikompromi | Isolasi wewenang agent dari owner; pause/revoke per agent | Owner adalah root authority dan bisa mengatur policy atau menarik saldo; gunakan multisig/DAO sebagai `owner`. Tidak ada timelock/threshold governance bawaan pada prototipe |
| Serangan upgrade | Versi payload dan versi policy membatalkan signed intent lama | Upgrade account NEAR dapat mengganti code bila otoritas account mengizinkan; QPC belum mengunci upgrade atau menerapkan timelock |
| Bridge/cross-chain | Pesan mengikat chain ID runtime dan alamat kontrak | QPC tidak mengimplementasikan bridge, light client, proof lintas-chain, atau cross-chain authorization |
| Supply-chain dependency/compiler/deploy | Rust toolchain, near-sdk, Borsh, dan `near-sys` dipin; `Cargo.lock` harus dipertahankan | Belum ada verifikasi provenance penuh, vendor lock, reproducible-build attestation, atau audit compiler. CI action dan proses release masih perlu hardening |
| Serangan ekonomi / MEV / censorship | Transfer agent terbatas ke satu recipient dan jumlah yang ditandatangani; request tidak mengeksekusi arbitrary calldata | Relayer dapat menahan/mengirim lebih dulu intent yang sudah signed; recipient dapat menjadi kontrak berbahaya; tidak ada model risiko ekonomi protokol/DEX/oracle |
| Promise/callback failure | Callback dialokasikan 40 Tgas; pending transfer dapat direkonsiliasi owner setelah 10 menit; callback terlambat tidak menyelesaikan operasi dua kali | Gas belum diukur di Sandbox. Reconciliation memakai pernyataan owner setelah pemeriksaan receipt eksternal; pilihan outcome yang salah dapat merusak accounting. Pause/revoke tidak membatalkan promise yang sudah dibuat |
| Direct transfer masuk | Hanya `deposit()` menambah saldo internal yang dapat digunakan | Transfer NEAR langsung yang tidak melewati `deposit()` tidak masuk pembukuan QPC dan tidak dapat dibelanjakan melalui ledger tanpa prosedur pemulihan yang ditambahkan |

## Batas implementasi saat ini

Implementasi NEAR sekarang hanya memproses transfer **native NEAR** ke satu recipient per agent. `destination_chain_id` harus sama dengan chain ID runtime; ini mencegah intent lintas-chain berpura-pura dieksekusi oleh transfer lokal. `derivation_path` hanya binding label dan bukan derivasi key. Belum ada arbitrary contract calls, fungible token, NFT, bridge, common multi-chain key registry, timelock, guardian threshold, atau upgrade controller.

Kontrak menetapkan `owner` sebagai account NEAR; untuk penggunaan bernilai, owner harus menjadi multisig/DAO yang sudah ditinjau. QPC tidak menjadi AI model, wallet custody, auditor, oracles, atau protocol-level defense. Ia menegakkan data yang ditandatangani dan policy yang dikonfigurasi.

## Pemisahan per chain

| Chain | Implementasi | Status dan aturan pemisahan |
|---|---|---|
| NEAR | Rust + near-sdk, QPC `QpcVault` | Implementasi hackathon aktif; memakai host ML-DSA NEAR dan native NEAR Promise semantics |
| Ethereum | Solidity `QPCAgentFinance.sol` | EVM legacy terpisah; belum memverifikasi ML-DSA. Deploy dengan `chainId=1`, alamat owner, konfigurasi, test, dan audit sendiri |
| BNB Chain | Belum ada code QPC; README rencana chain-spesifik | Jika kelak memakai code EVM yang direview, deployment/config/owner/address harus tersendiri (`56` mainnet, `97` testnet). Tidak ada BNB address/deployment saat ini dan bukan shared state dengan Ethereum. [11] |
| Solana | Program native Solana (misalnya Anchor) | Belum diimplementasikan; perlu instruction/PDA/SPL-specific design dan signer/verifier terpisah. Host function NEAR tidak tersedia otomatis di Solana |

Signature NEAR tidak memberi keamanan quantum pada deployment Ethereum, BNB, atau Solana. Tidak ada chain adapter yang boleh mengklaim dukungan sebelum implementasi, test, dan review chain-spesifik selesai.

## Validasi dan gerbang sebelum testnet/mainnet

- **Sudah dijalankan lokal:** 12 unit test berhasil, termasuk keypair/signature ML-DSA-65 nyata, intent valid, payload/recipient tampering, replay, expiry, versi policy, cap atas dana pending, callback settlement sukses/gagal, 64 langkah state-machine pseudo-random untuk accounting invariant, mismatch chain/path sebelum Promise, empty/control-character input, binding pada pending record/audit events, dan owner-attested recovery. `cargo clippy --locked --all-targets -- -D warnings` dan `cargo build --locked --release --target wasm32-unknown-unknown` juga berhasil.
- **Belum tercakup di suite saat ini:** signature dengan panjang salah, semua jalur pause/revoke/withdrawal, rotasi generation setelah expiry, dan pengujian permintaan pending maksimum; tambahkan regresi sebelum deployment.
- NEAR Sandbox integration test untuk beberapa akun, attached deposit, Promise success/failure, callback failures, storage usage, gas, concurrent pending transfers, dan pause/revoke saat receipt pending.
- Property/fuzz testing atas arus state: `spent + reserved <= total_cap`, `reserved` cocok dengan operasi pending, dan saldo internal tidak menjadi negatif/overflow.
- Formalisasi invariant akuntansi dan review setiap entry point; verifikasi formal terukur belum dilakukan.
- Audit independen terhadap contract, dependency graph, build artifact, compiler/toolchain, deployment key, dan rencana upgrade.
- Reproducible WASM build, provenance/SBOM, dan verifikasi hash artefak sebelum deployment.

Status saat ini: **prototipe lokal; 12 unit test, Clippy, dan WASM build lulus; EVM legacy contracts/tests dapat dikompilasi oleh solc 0.8.24 tetapi Foundry test runner tidak tersedia lokal; belum ada deployment, Sandbox integration campaign, fuzz campaign penuh, formal verification, atau audit independen.** Unit test recovery tidak mensimulasikan callback receipt yang benar-benar gagal gas; gas dan receipt harus divalidasi sebelum testnet/mainnet. Hasil unit test tidak membuktikan keamanan produksi.

## References

[1]: https://csrc.nist.gov/pubs/fips/204/final "FIPS 204 Module-Lattice-Based Digital Signature Standard"
[2]: https://github.com/near/near-sdk-rs/releases/tag/near-sdk-v5.30.0 "NEAR SDK 5.30.0 release"
[3]: https://github.com/near/nearcore/releases/tag/2.14.0 "NEARcore 2.14.0 release"
[4]: https://docs.near.org/smart-contracts/anatomy/environment "NEAR contract environment"
[5]: https://docs.near.org/tools/near-api "NEAR API and account keys"
[6]: https://docs.near.org/tools/cli "NEAR CLI"
[7]: https://docs.near.org/smart-contracts/anatomy/crosscontract "NEAR cross-contract calls"
[8]: https://docs.near.org/smart-contracts/testing/unit-test "NEAR contract unit testing"
[9]: https://rpc.mainnet.near.org "NEAR Mainnet RPC endpoint"
[10]: https://rpc.testnet.near.org "NEAR Testnet RPC endpoint"
[11]: https://docs.bnbchain.org/bnb-smart-chain/developers/json_rpc/json-rpc-endpoint/ "BNB Smart Chain JSON-RPC endpoints and chain IDs"
