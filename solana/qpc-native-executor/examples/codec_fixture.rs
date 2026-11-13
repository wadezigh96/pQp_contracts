//! Regenerate with `cargo run --locked --manifest-path
//! solana/qpc-native-executor/Cargo.toml --example codec_fixture -- <commitment-hex>`
//! and redirect stdout to `solana/qpc-native-executor/fixtures/codec-v1.json`.
//!
//! Commitment is independently computed from the JS test's full envelope; this
//! example only encodes owner-attested bytes, it does not parse canonical JSON.
use qpc_native_executor::{intent_address, policy_address, Instruction, Intent, Policy};
use solana_program::{hash::hashv, pubkey::Pubkey};
use std::str::FromStr;
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn main() {
    let commitment = std::env::args()
        .nth(1)
        .expect("canonical commitment hex required");
    assert!(
        commitment.len() == 64
            && commitment
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    let mut digest = [0; 32];
    for (n, value) in digest.iter_mut().enumerate() {
        *value = u8::from_str_radix(&commitment[n * 2..n * 2 + 2], 16).unwrap();
    }
    let program = Pubkey::new_from_array([1; 32]);
    let owner = Pubkey::new_from_array([2; 32]);
    let agent = Pubkey::new_from_array([3; 32]);
    let recipient = Pubkey::new_from_array([4; 32]);
    let (policy_address, policy_bump) = policy_address(&program, &owner, &agent);
    let (intent_address, intent_bump) = intent_address(&program, &policy_address, 7);
    let genesis = Pubkey::from_str("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG").unwrap();
    let policy = Policy {
        owner,
        agent,
        genesis: genesis.to_bytes(),
        policy_id: hashv(&[b"QPC-SOLANA-POLICY-ID-V1\0", b"policy-1"]).to_bytes(),
        version: 2,
        epoch: 1,
        active: true,
    };
    let intent = Intent {
        policy: policy_address,
        authorization_commitment: digest,
        intent_id: hashv(&[b"QPC-SOLANA-INTENT-ID-V1\0", b"local-native-1"]).to_bytes(),
        policy_commitment: [0xaa; 32],
        recipient,
        nonce: 7,
        amount: 123,
        version: 2,
        epoch: 1,
        issued_at: 1800000000,
        not_before: 1800000000,
        expires_at: 1800000300,
        max_fee: 5000,
        executed: false,
    };
    let initialize = Instruction::Initialize(policy).encode();
    let configure = Instruction::Configure {
        version: 2,
        epoch: 1,
        active: true,
    }
    .encode();
    let approve = Instruction::Approve(intent).encode();
    let execute = Instruction::Execute { nonce: 7 }.encode();
    for bytes in [&initialize, &configure, &approve, &execute] {
        assert_eq!(Instruction::decode(bytes).unwrap().encode(), *bytes);
    }
    println!("{{\n  \"provenance\": \"Rust Instruction::encode; synthetic local codec fixture, not a deployed program\",\n  \"programId\": \"{program}\",\n  \"owner\": \"{owner}\",\n  \"agent\": \"{agent}\",\n  \"recipient\": \"{recipient}\",\n  \"genesisHash\": \"{genesis}\",\n  \"policyAddress\": \"{policy_address}\",\n  \"intentAddress\": \"{intent_address}\",\n  \"policyBump\": {policy_bump},\n  \"intentBump\": {intent_bump},\n  \"authorizationCommitment\": \"{commitment}\",\n  \"initialize\": \"{}\",\n  \"configure\": \"{}\",\n  \"approve\": \"{}\",\n  \"execute\": \"{}\"\n}}", hex(&initialize), hex(&configure), hex(&approve), hex(&execute));
}
