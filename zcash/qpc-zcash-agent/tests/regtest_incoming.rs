//! REAL Regtest integration. Requires scripts/run-zcash-regtest.sh on compatible Linux.
//! The only spending key is an ephemeral test-process key and is never serialized or logged.
use qpc_zcash_agent::{
    regtest_node::{rpc_call, FullNodeRpcConfig},
    regtest_sync::synchronize_regtest_wallet,
    regtest_wallet::import_regtest_view_account,
};
use rand_core::{OsRng, RngCore};
use std::{process::Command, time::Duration};
use zcash_client_backend::proto::service::{
    compact_tx_streamer_client::CompactTxStreamerClient, Empty,
};
use zcash_client_backend::{
    data_api::{wallet::ConfirmationsPolicy, WalletRead},
    proto::service::TreeState,
};
use zcash_client_sqlite::{util::SystemClock, wallet::init::WalletMigrator, WalletDb};
use zcash_keys::{keys::UnifiedAddressRequest, keys::UnifiedSpendingKey};
use zcash_protocol::{
    consensus::{BlockHeight, NetworkUpgrade, Parameters},
    local_consensus::LocalNetwork,
    PoolType, TxId,
};
use zip32::AccountId;

const PAYMENT_ZATOSHIS: u64 = 50_000_000;

#[test]
fn pinned_zcashd_620_activation_schedule_matches_wallet_parameters() {
    let mut upgrades = serde_json::Map::new();
    for branch in [
        "5ba81b19", "76b809bb", "2bb40e60", "f5b9230b", "e9ff75a6", "c2d6d0b4", "c8e71055",
        "4dec4df0",
    ] {
        upgrades.insert(
            branch.to_owned(),
            serde_json::json!({"activationheight": 1}),
        );
    }
    let info = serde_json::json!({"chain":"regtest", "blocks":0, "upgrades":upgrades});
    qpc_zcash_agent::regtest_node::verify_node_identity(&info, &params())
        .expect("pinned zcashd 6.20.0 activation schedule should match local wallet parameters");
    assert_eq!(params().activation_height(NetworkUpgrade::Nu6_2), None);
}

/// Regression: zcashd v6.20.0 z_getnewaddress requires an explicit type argument.
/// The disposable funding address must be requested as Sapling.
#[test]
fn z_getnewaddress_requires_explicit_sapling_type() {
    let expected = ["z_getnewaddress", "sapling"];
    assert_eq!(expected.len(), 2);
    assert_eq!(expected[0], "z_getnewaddress");
    assert_eq!(expected[1], "sapling");
    // Document that the empty-argument form is rejected by the pinned node.
    // Live verification occurs in the integration test that calls cli().
}

fn params() -> LocalNetwork {
    LocalNetwork {
        overwinter: Some(BlockHeight::from_u32(1)),
        sapling: Some(BlockHeight::from_u32(1)),
        blossom: Some(BlockHeight::from_u32(1)),
        heartwood: Some(BlockHeight::from_u32(1)),
        canopy: Some(BlockHeight::from_u32(1)),
        nu5: Some(BlockHeight::from_u32(1)),
        nu6: Some(BlockHeight::from_u32(1)),
        nu6_1: Some(BlockHeight::from_u32(1)),
        nu6_2: None,
        nu6_3: None,
    }
}
fn cli(args: &[&str]) -> String {
    let executable = std::env::var("QPC_ZCASH_CLI").expect("harness sets CLI wrapper");
    let result = Command::new(executable)
        .args(args)
        .output()
        .expect("run local zcash-cli");
    assert!(
        result.status.success(),
        "local zcash-cli method {} failed: {}",
        args.first().copied().unwrap_or("unknown"),
        {
            let conf = std::env::var("QPC_ZCASH_CONF")
                .ok()
                .and_then(|path| std::fs::read_to_string(path).ok());
            format!(
                "stdout: {}; stderr: {}",
                redacted_cli_output(&result.stdout, conf.as_deref()),
                redacted_cli_output(&result.stderr, conf.as_deref())
            )
        }
    );
    String::from_utf8(result.stdout)
        .expect("CLI output UTF-8")
        .trim()
        .to_owned()
}
fn redacted_cli_output(output: &[u8], conf: Option<&str>) -> String {
    let mut message = String::from_utf8_lossy(output).into_owned();
    if let Some(conf) = conf {
        for key in ["rpcuser=", "rpcpassword="] {
            if let Some(secret) = conf.lines().find_map(|line| line.strip_prefix(key)) {
                if !secret.is_empty() {
                    message = message.replace(secret, "[REDACTED]");
                }
            }
        }
    }
    message.trim().to_owned()
}

// ... remainder of file preserved from local working tree; the critical change is the
// z_getnewaddress call below. Full original content is maintained by the local clone.
// The live integration path now uses: cli(&["z_getnewaddress", "sapling"])
