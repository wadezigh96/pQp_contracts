# QPC native execution processor — local/host-tested only

This is a real `solana-program` entrypoint/processor with SystemProgram CPIs, not a standalone transfer simulator. It has **not** been compiled for SBF, deployed, signed, broadcast, or tested in a Solana bank/runtime. Host AccountInfo/syscall-stub tests do not establish runtime execution or transaction rollback. No funds or network connections are used by these tests.

It records **execution**, never finality, settlement, a bridge completion, or a capability accepted by the generic intent store. Asset support is fixed to native SOL, **lamports / 9 decimals**. No SPL tokens or wrapped SOL.

## Trust boundary

An owner signer explicitly attests that the supplied canonical authorization commitment matches every typed parameter and the immutable policy metadata. The processor does **not** parse canonical envelope JSON, verify its SHA-256 commitment, authenticate an agent identity from text, derive recipient commitments, or prove that source/destination networks match. A forged hash approved by a trusted owner is still an owner attestation, not independent evidence. An off-chain approval builder/owner must validate the canonical same-chain native envelope first, including source account = agent, source/destination adapter = this program, exact amount/recipient/nonce/policy/time/asset fields, and configured genesis.

All hash fields are raw 32-byte values, not ASCII hex. `authorization_commitment` is the existing canonical envelope commitment. For off-chain ID hashing use SHA-256 over UTF-8 `QPC-SOLANA-INTENT-ID-V1\0` + intent ID and `QPC-SOLANA-POLICY-ID-V1\0` + policy ID. The processor never recomputes these text-to-digest attestations. Genesis is the 32 bytes decoded from the cluster genesis hash, not a textual network label.

The owner-pinned genesis is **metadata only**: Solana programs cannot query/prove their current genesis hash. An independent inspector must pin the actual RPC genesis, program address, deployed code/upgrade authority, successful transaction, signer, exact instruction bytes/account ordering, PDA owners/data, native transfer CPI and canonical/finalized result before making any native observation. No such inspector or deployment is established by this crate. A malicious program upgrade can change every invariant; the operator must pin/audit upgrade authority or an immutable deployment.

`max_fee` is immutable attested metadata; the processor **cannot enforce total transaction fees**, priority fees, or fees of other instructions. Approval rent is paid by the owner, execution amount by the agent, transaction fees by the transaction fee payer. Other instructions in a transaction are outside this processor's authority. This crate is not a complete all-in budget guard.

## Exact account setup and instruction API

Public `Instruction::encode/decode`, `Policy::encode/decode`, `Intent::encode/decode`, `policy_address` and `intent_address` are the wire reference. All integers are unsigned **u64 little endian**, bytes32/pubkeys are raw bytes, booleans exactly 0 or 1. Wrong lengths, invalid booleans and trailing bytes fail. There is no integer narrowing/coercion or JSON number parsing in this program.

1. `Initialize(Policy)` (tag `0`, followed by 153-byte Policy): accounts `[owner signer+writable/payer, policy PDA writable, executable SystemProgram]`. Policy PDA seeds are `[b"policy", owner bytes32, agent bytes32]` under this program ID. Owner/agent/genesis/policy ID are immutable. Initial policy must be active, nonzero keys/hashes, version and epoch at least 1. The owner may initialize only its own policy. No global admin or owner registration exists; consumers must pin the intended owner/policy.
2. `Configure { version, epoch, active }` (tag `1`, 8+8+1 bytes): `[owner signer, policy PDA writable]`. Version must not decrease and epoch must strictly increase. Every update invalidates pending approvals from earlier epochs. Reactivation cannot resurrect those approvals. Owner/agent/genesis/policy ID cannot be replaced.
3. `Approve(Intent)` (tag `2`, followed by 233-byte Intent): `[owner signer+writable/payer, policy PDA, intent PDA writable, executable SystemProgram]`. Intent PDA seeds are `[b"intent", policy PDA bytes32, nonce u64 LE]`. A nonce is permanently reserved once approved, even after expiration or revocation. No close/reset/reapproval instruction exists. Approval must match active policy version/epoch, have positive amount, nonzero hash attestations, distinct/nonzero recipient, valid `issued_at <= not_before < expires_at`, issuance not in future and expiry strictly in future. Not-before may be in future. All fields, except executed, are immutable. Initial executed must be false.
4. `Execute { nonce }` (tag `3`, 8 bytes): `[agent signer+writable, policy PDA, intent PDA writable, recipient writable, executable SystemProgram]`. No caller-supplied amount/recipient override exists. Agent must match the fixed policy agent and be a system-owned empty-data account. Policy/intent must be owned by this exact program and have correct PDA derivations. Policy must remain active with matching version/epoch. Runtime `Clock::get` requires `not_before <= now < expires_at`. A negative clock timestamp rejects. Exact stored lamports are transferred from the agent via SystemProgram, never from a program vault. Success marks executed permanently; duplicate execution fails.

Initialization and approval **allocate and initialize their own PDA accounts by signed CPI**, not arbitrary caller-created program accounts. System-owned empty-data PDA addresses are required. Unfunded PDAs use `create_account`; prefunded PDAs are rent-topped-up, `allocate`d and `assign`ed with verified PDA signing seeds, so a dust prefund does not block initialization. Rent uses `Rent::get`. Existing initialized accounts cannot be overwritten. The SystemProgram must have its canonical address and be executable. Every instruction rejects extra accounts as well as missing accounts.

Policy wire order after `QPCPOL01` (8 bytes): owner32, agent32, genesis32, policyId32, version8, epoch8, active1. Length 153.

Intent wire order after `QPCINT01` (8 bytes): policyPubkey32, authorizationCommitment32, intentId32, policyCommitment32, recipient32, nonce8, amount8, version8, epoch8, issuedAt8, notBefore8, expiresAt8, maxFee8, executed1. Length 233. Policy pointer binds immutable owner/agent/genesis/policy ID; PDA derivation binds program and nonce. Uniqueness is per policy+nonce, not a global duplicate-commitment registry. Owners must not attest the same canonical envelope under inconsistent nonces or policies.

## Success evidence and rollback boundary

After CPI succeeds, binary `sol_log_data` emits these five slices: `QPC_EXECUTED_V1`, authorizationCommitment32, policyPubkey32, nonceLE8, amountLE8. A text `QPC_EXECUTED nonce=… amount=…` log accompanies it. Neither is proof of finality. Inspectors must reject failed transactions even if nested logs are present.

The executed flag is written **before** transfer CPI. SystemProgram native transfer does not call recipient code; there is no arbitrary external-program callback surface. The processor propagates CPI failure to its caller. In a real Solana transaction, unsuccessful execution relies on bank/runtime atomic rollback to revert state, enabling retry against the unchanged approval without a duplicate success. **Host tests do not provide that rollback**: the failed-CPI test intentionally demonstrates that the host AccountInfo state remains marked executed while balances stay unchanged and no execution event is emitted. This is not a runtime failure/retry test; bank/SBF integration remains required before deployment.

## Local verification

```bash
cargo test --manifest-path solana/qpc-native-executor/Cargo.toml --locked
cargo fmt --manifest-path solana/qpc-native-executor/Cargo.toml --check
cargo clippy --manifest-path solana/qpc-native-executor/Cargo.toml --locked --all-targets -- -D warnings
```

Dependency `solana-program = "=2.3.0"` and the transitive `Cargo.lock` are pinned. Host tests invoke the actual `process_instruction`, with stubs only at Clock/Rent/SystemProgram syscall boundaries. They exercise PDA allocation/signing requests, prefunding, exact transfer, replay, immutable approval, authorization/owner/program/account substitutions, revocation, expiry, strict wire decoding and failed CPI ordering. The stub is focused verification of requested CPI arguments, not a general SystemProgram/runtime emulator. No SBF toolchain/runtime integration check has been run.
