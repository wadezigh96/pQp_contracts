# Spec: QPC NEAR Cyber Defense Contract

## Objective

QPC NEAR menjadi control plane on-chain untuk agent yang mengeksekusi transaksi ke L1 lain. Fitur Cyber Defense menambahkan enforcement keamanan yang dapat diverifikasi di kontrak: target chain dan network harus eksplisit, derivation path harus terikat ke action, payload tidak boleh diubah, nonce tidak dapat diulang, frekuensi dan kuota dibatasi, serta owner/security guardian dapat melakukan quarantine atau global circuit breaker.

Fitur ini ditujukan untuk demo NEAR HACKATHON pada track **AI Security & Defense** dan **Open Innovation**, dengan relevansi tambahan ke **Privacy & Private AI**. Private agent reasoning tetap berada di private runtime/TEE; kontrak hanya menerima commitment dan metadata minimum yang dibutuhkan untuk enforcement. Kontrak tidak mengklaim mendeteksi malware, membuktikan keamanan solver, atau membuat transaksi tujuan tidak terlihat.

## Tech Stack

- Rust 2021.
- `near-sdk` yang sudah digunakan oleh `near/qpc-near`.
- Kontrak NEAR WASM pada crate `near/qpc-near`.
- Unit dan sandbox-oriented tests pada `near/qpc-near/src/lib.rs` dan `near/qpc-near/tests/`.
- Tidak menambah dependency eksternal untuk fase pertama.

## Commands

```sh
cd near
cargo fmt --all -- --check
cargo test --workspace
cargo check --workspace
cargo build --release --target wasm32-unknown-unknown -p qpc-near
```

## Project Structure

- `near/qpc-near/src/lib.rs` — policy core, dispatch, Cyber Defense state, incident lifecycle.
- `near/qpc-near/tests/dispatch.rs` — dispatch, replay, callback, dan fail-closed tests.
- `near/qpc-near/tests/cyber_defense.rs` — security-specific regression tests.
- `near/qpc-near/tests/zcash_route_policy.rs` — opaque recipient policy regression; tetap dipertahankan sebagai adapter test, bukan produk utama.
- `docs/CAPABILITY-MAP.md` — dependency dan build order.
- `docs/hackathon-research.md` — sumber resmi dan batasan klaim.

## Data model and interface

`ActionRule` mempertahankan target dan method yang telah ada lalu menambahkan:

- `destination_chain_id: String` — chain/network tujuan yang harus diikat ke rule.
- `derivation_path: String` — path Chain Signatures yang spesifik untuk chain tersebut.
- `max_payload_bytes: u32` — batas payload outbound.
- `cooldown_ns: u64` — jarak waktu minimum antara dispatch action yang sama; `0` berarti tidak ada cooldown selain quota.

`ActionRequest` mempertahankan `policy_id`, `expected_version`, `request_id`, `nonce`, `action_id`, `target`, `method_name`, `args`, dan `attached_deposit`, lalu menambahkan:

- `destination_chain_id: String`.
- `derivation_path: String`.

Dispatch hanya diterima bila kedua nilai request sama persis dengan `ActionRule`. Kontrak menghitung hash argumen seperti saat ini, sehingga model/provider tidak dapat mengganti target chain, path, recipient commitment, atau payload setelah policy dibuat.

`SecurityState` pada kontrak menyimpan:

- `global_halted: bool`.
- `security_guardian: AccountId`.
- `incident_nonce: u64`.

`Policy` menyimpan `quarantined: bool` dan `quarantine_incident_id: Option<String>`. `Usage` menyimpan `last_action_dispatch_ns` per action melalui map baru yang tidak mereset saat policy update.

Public methods:

- `set_security_guardian(account_id)` — owner only; guardian tidak boleh zero account.
- `set_global_halt(halted)` — owner atau security guardian; setiap perubahan menghasilkan security event.
- `quarantine_policy(policy_id, incident_id, reason_code)` — owner atau guardian; menambah incident nonce, mengunci dispatch policy, dan menyimpan incident ID serta bounded reason code.
- `clear_policy_quarantine(policy_id, incident_id)` — owner only; hanya incident ID yang sedang tersimpan yang boleh dibersihkan.
- `get_security_state()` — read-only.
- `get_policy_security(policy_id)` — read-only.

`ExecutionRecord` dan event log harus menyimpan destination chain ID, derivation path hash, request ID, nonce, dan policy version agar auditor dapat menghubungkan keputusan private dengan payload outbound tanpa menyimpan secret plaintext.

## Security behavior

Sebelum membuat Promise, `dispatch_action` harus memeriksa secara berurutan:

1. Kontrak tidak berada pada `global_halted`.
2. Policy tidak `paused`, `revoked`, atau `quarantined`.
3. Caller sama dengan agent.
4. Policy aktif dan version cocok.
5. Nonce serta request ID belum pernah dipakai.
6. Target, method, `destination_chain_id`, dan `derivation_path` sama dengan rule.
7. Payload tidak melampaui `max_payload_bytes` dan hash args sama dengan commitment rule.
8. Cooldown action telah lewat.
9. Batas per-call, cumulative cap, policy cap, dan max call tetap terpenuhi.

Semua accounting, nonce, request marker, dan last-dispatch timestamp ditulis sebelum Promise dibuat. Jika target gagal, quota tidak dikembalikan. Callback terminal tetap idempotent. Quarantine dan global halt menghentikan dispatch baru, tetapi tidak menghapus execution records atau mengubah hasil callback yang sudah berjalan.

Security events harus mencakup minimal `security_guardian_set`, `global_halt_set`, `policy_quarantined`, `policy_quarantine_cleared`, dan `security_rejection`. `reason_code` hanya berupa bounded ASCII identifier; tidak boleh menerima raw secret, prompt, private key, atau recipient address.

## Code style

Ikuti pola crate saat ini: tipe publik memakai `PascalCase`, field memakai `snake_case`, validasi menggunakan `assert!`/`assert_eq!` dengan pesan fail-closed, dan key storage dipisah dalam `StorageKey`. Contoh validasi:

```rust
fn assert_chain_binding(rule: &ActionRule, request: &ActionRequest) {
    assert_eq!(request.destination_chain_id, rule.destination_chain_id, "destination chain mismatch");
    assert_eq!(request.derivation_path, rule.derivation_path, "derivation path mismatch");
}
```

Tidak boleh menaruh private key, raw Unified Address, model prompt, atau secret TEE dalam state/event kontrak.

## Testing Strategy

Tambahkan `near/qpc-near/tests/cyber_defense.rs` dengan test berikut:

- owner dapat menetapkan guardian, tetapi account tidak sah ditolak;
- guardian dapat mengaktifkan global halt dan agent dispatch gagal closed;
- hanya owner/guardian dapat quarantine policy;
- policy yang di-quarantine menolak dispatch dan menyimpan incident ID;
- hanya owner dapat clear quarantine dengan incident ID yang benar;
- stale atau salah `destination_chain_id` ditolak;
- derivation path yang berbeda ditolak untuk mencegah cross-chain path confusion;
- payload di atas `max_payload_bytes` ditolak;
- cooldown action menolak dispatch terlalu cepat;
- nonce/request replay tetap ditolak setelah update policy, halt, dan clear quarantine;
- security event tidak memuat raw recipient atau raw secret fixture;
- callback dan execution record tetap immutable/idempotent setelah quarantine atau global halt.

Acceptance test tingkat demo harus menunjukkan:

```text
private agent decision -> policy commitment -> QPC dispatch preflight -> security-approved outbound request -> terminal receipt record
```

Demo boleh memakai mock Chain Signature/receipt adapter, tetapi README harus menandainya sebagai mock dan menunjukkan field chain ID, derivation path, payload hash, incident state, dan receipt verification secara eksplisit.

## Boundaries

- Always: fail closed; chain/network binding wajib; derivation path per chain wajib berbeda; nonce dan request ID anti-replay; secret tidak pernah masuk state/event; jalankan format, test, check, dan WASM build sebelum commit.
- Ask first: perubahan schema yang merusak state lama; penambahan dependency; penggunaan live Chain Signature signer; deploy testnet/mainnet; penggunaan NEAR AI Cloud API key; perubahan workflow CI; push atau submission hackathon.
- Never: menyimpan private key atau raw private recipient; menganggap callback/off-chain policy sebagai security proof; mengklaim PQ signature tanpa verifier yang nyata; menganggap provider SUCCESS sebagai settlement; menyiarkan transaksi atau mengirim submission tanpa konfirmasi manusia.

## Success Criteria

1. `qpc-near` mengkompilasi ke WASM dan seluruh workspace test lulus.
2. Dispatch lintas-L1 yang memiliki chain ID atau derivation path salah selalu revert sebelum Promise outbound dibuat.
3. Global halt dan per-policy quarantine menghentikan agent baru tanpa menghapus audit trail.
4. Cooldown, payload bound, nonce, request ID, dan existing budget cap bekerja bersama tanpa reset melalui policy update.
5. Test fixture membuktikan tidak ada raw secret atau recipient address dalam request yang disimpan on-chain.
6. README menyediakan demo reproducible dengan langkah `do X -> see Y` dan menyatakan mana yang mock versus live.
7. Arsitektur tetap kompatibel dengan adapter Chain Signatures dan NEAR Intents tanpa menganggap keduanya sebagai policy authority.

## Open Questions

- L1 pertama untuk demo harus dipilih: EVM testnet paling mudah diverifikasi, atau chain lain yang lebih kuat untuk narasi privacy.
- Apakah security guardian berupa satu account guardian cukup untuk demo, atau perlu threshold/multisig sebagai modul terpisah?
- Apakah cooldown berbasis block timestamp cukup, atau demo memerlukan window berbasis block height?
- Apakah target hackathon memerlukan deploy testnet nyata atau demo lokal/sandbox sudah cukup untuk jalur submission?
