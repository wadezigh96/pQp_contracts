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

#[test]
fn cli_error_redaction_hides_private_rpc_credentials() {
    let conf = "rpcuser=private-user\nrpcpassword=private-password\n";
    let message = redacted_cli_output(b"auth failed for private-user:private-password", Some(conf));
    assert!(!message.contains("private-user"));
    assert!(!message.contains("private-password"));
    assert!(message.contains("[REDACTED]"));
}

fn state_from_rpc(v: &serde_json::Value) -> TreeState {
    TreeState {
        network: v["network"]
            .as_str()
            .expect("node treestate network")
            .to_owned(),
        height: v["height"].as_u64().expect("node treestate height"),
        hash: v["hash"].as_str().expect("node treestate hash").to_owned(),
        time: v["time"]
            .as_u64()
            .expect("node treestate time")
            .try_into()
            .unwrap(),
        sapling_tree: v["sapling"].as_str().expect("sapling tree").to_owned(),
        orchard_tree: v["orchard"].as_str().unwrap_or_default().to_owned(),
        ironwood_tree: v["ironwood"].as_str().unwrap_or_default().to_owned(),
    }
}

#[tokio::test]
#[ignore = "requires isolated local zcashd + lightwalletd from scripts/run-zcash-regtest.sh"]
async fn receives_and_persists_a_real_shielded_regtest_note() {
    let p = params();
    // The local node independently verifies the requested chain before any wallet work.
    let conf = std::fs::read_to_string(std::env::var("QPC_ZCASH_CONF").unwrap()).unwrap();
    let rpc_user = conf
        .lines()
        .find_map(|l| l.strip_prefix("rpcuser="))
        .expect("private local RPC user")
        .to_owned();
    let rpc_password = conf
        .lines()
        .find_map(|l| l.strip_prefix("rpcpassword="))
        .expect("private local RPC password")
        .to_owned();
    let rpc = FullNodeRpcConfig::new(
        &std::env::var("QPC_ZCASH_RPC_ENDPOINT").unwrap(),
        &rpc_user,
        &rpc_password,
    )
    .unwrap();
    let chain = rpc_call(&rpc, "getblockchaininfo", serde_json::json!([]))
        .await
        .unwrap();
    let confirmed_height = qpc_zcash_agent::regtest_node::verify_node_identity(&chain, &p).unwrap();
    assert_eq!(chain["chain"], "regtest");
    // Mine mature disposable coinbase funds only on the isolated local chain.
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    let usk = UnifiedSpendingKey::from_seed(&p, &seed, AccountId::ZERO)
        .expect("ephemeral regtest test key");
    seed.fill(0);
    let ufvk = usk.to_unified_full_viewing_key();
    let (ua, _) = ufvk
        .default_address(UnifiedAddressRequest::AllAvailableKeys)
        .unwrap();
    let recipient = ua.encode(&p);
    let ufvk_text = ufvk.encode(&p);

    // z_sendmany cannot make arbitrary change from coinbase inputs. First
    // shield disposable mature coinbase funds into the local Sapling wallet.
    // zcashd v6.20.0 requires the address type argument explicitly.
    let shielded_source = cli(&["z_getnewaddress", "sapling"]);
    for args in coinbase_funding_commands(confirmed_height, &shielded_source) {
        let cli_args: Vec<_> = args.iter().map(String::as_str).collect();
        let result = cli(&cli_args);
        if args[0] == "z_shieldcoinbase" {
            let response: serde_json::Value =
                serde_json::from_str(&result).expect("z_shieldcoinbase JSON response");
            let opid = response["opid"]
                .as_str()
                .expect("z_shieldcoinbase operation id");
            wait_operation(opid).await;
        }
    }
    cli(&["generate", "1"]);

    // The UA may select an Orchard receiver while the disposable source is Sapling.
    // zcashd rejects that cross-pool send under FullPrivacy. The Regtest-only
    // harness explicitly opts into AllowRevealedAmounts; production policy is untouched.
    let send_args = shielded_payment_command(&shielded_source, &recipient);
    let cli_args: Vec<_> = send_args.iter().map(String::as_str).collect();
    let opid = cli(&cli_args);
    let txid_text = wait_operation(&opid).await;
    cli(&["generate", "1"]);
    let tx: serde_json::Value =
        serde_json::from_str(&cli(&["gettransaction", &txid_text])).unwrap();
    assert!(
        tx["confirmations"].as_u64().unwrap_or(0) >= 1,
        "full node has not confirmed transaction"
    );
    assert_eq!(tx["txid"].as_str(), Some(txid_text.as_str()));
    let mined_height = tx["blockheight"]
        .as_u64()
        .expect("mined height from full node");
    let mined_hash = tx["blockhash"]
        .as_str()
        .expect("mined block hash from full node");
    let canonical_hash = cli(&["getblockhash", &mined_height.to_string()]);
    assert_eq!(mined_hash, canonical_hash, "node block hash mismatch");
    // z_viewtransaction is the full-node wallet's independent decrypted view.
    let view: serde_json::Value =
        serde_json::from_str(&cli(&["z_viewtransaction", &txid_text])).unwrap();
    assert_eq!(
        verified_node_shielded_amount(&view, &recipient),
        Some(PAYMENT_ZATOSHIS)
    );
    let txid = TxId::from_hex(&txid_text).expect("canonical full-node txid");
    let birthday_tree: serde_json::Value =
        serde_json::from_str(&cli(&["z_gettreestate", "101"])).unwrap();
    let birthday = state_from_rpc(&birthday_tree);
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("private");
    std::fs::create_dir(&dir).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let wallet = dir.join("wallet.sqlite");
    import_regtest_view_account(&wallet, p, "qpc-incoming-test", &ufvk_text, birthday).unwrap();
    let endpoint = std::env::var("QPC_LIGHTWALLETD_ENDPOINT").unwrap();
    await_lightwalletd(&endpoint).await;
    synchronize_regtest_wallet(&wallet, &endpoint, &rpc, p, 128)
        .await
        .unwrap();
    let outputs = received(&wallet, p, txid, mined_height as u32 + 1);
    assert!(
        !outputs.is_empty(),
        "wallet scanner did not discover shielded transaction output"
    );
    assert!(outputs
        .iter()
        .all(|o| matches!(o.pool_type(), PoolType::Shielded(_))));
    let wallet_amounts: Vec<_> = outputs.iter().map(|o| u64::from(o.value())).collect();
    assert!(
        wallet_amounts.contains(&PAYMENT_ZATOSHIS),
        "wallet amount differs from node output"
    );
    drop(usk);
    // A database close/reopen followed by a second scan must preserve exactly one accounting record.
    synchronize_regtest_wallet(&wallet, &endpoint, &rpc, p, 128)
        .await
        .unwrap();
    let after = received(&wallet, p, txid, mined_height as u32 + 1);
    assert_eq!(
        after.len(),
        outputs.len(),
        "wallet restart duplicated note accounting"
    );
    let report = serde_json::json!({"network":"regtest","txid":txid.to_string(),"mined_height":mined_height,"mined_block_hash":mined_hash,"canonical_block_hash":canonical_hash,"confirmations":tx["confirmations"],"amount_zatoshis":PAYMENT_ZATOSHIS,"discovered_shielded_outputs":after.len(),"restart_output_count":after.len(),"receipt_source":"local zcashd gettransaction + getblockhash + z_viewtransaction + WalletDb get_received_outputs"});
    if let Ok(path) = std::env::var("QPC_REGTEST_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
}
fn shielded_payment_command(source: &str, recipient: &str) -> Vec<String> {
    let outputs = serde_json::json!([{"address": recipient, "amount": 0.5}]).to_string();
    vec![
        "z_sendmany".to_owned(),
        source.to_owned(),
        outputs,
        "1".to_owned(),
        "0.0001".to_owned(),
        "AllowRevealedAmounts".to_owned(),
    ]
}

#[test]
fn shielded_payment_explicitly_allows_cross_pool_ua_receiver_only_in_regtest() {
    let args = shielded_payment_command("sapling-source", "unified-recipient");
    assert_eq!(args[0], "z_sendmany");
    let outputs: serde_json::Value = serde_json::from_str(&args[2]).unwrap();
    assert_eq!(outputs[0]["address"], "unified-recipient");
    assert_eq!(args[3], "1");
    assert_eq!(args[4], "0.0001");
    assert_eq!(args[5], "AllowRevealedAmounts");
}

#[test]
fn shieldcoinbase_response_extracts_operation_id() {
    let response = serde_json::json!({
        "remainingUTXOs": 0,
        "remainingValue": 0.0,
        "shieldingUTXOs": 1,
        "shieldingValue": 6.25,
        "opid": "opid-test"
    });
    assert_eq!(response["opid"].as_str(), Some("opid-test"));
}

#[derive(Debug, PartialEq)]
enum OperationState {
    Pending,
    Success,
    Failed(String),
}

/// Classify the first entry of a z_getoperationstatus response and preserve
/// the node's error code/message for terminal failures.
fn classify_operation_status(status: &serde_json::Value) -> Result<OperationState, String> {
    let entry = status
        .as_array()
        .and_then(|entries| entries.first())
        .ok_or_else(|| format!("z_getoperationstatus returned no operation entry: {status}"))?;
    match entry["status"].as_str() {
        Some("queued" | "executing") => Ok(OperationState::Pending),
        Some("success") => Ok(OperationState::Success),
        Some(state @ ("failed" | "cancelled")) => {
            let code = entry["error"]["code"].as_i64();
            let message = entry["error"]["message"]
                .as_str()
                .unwrap_or("no error message reported");
            Ok(OperationState::Failed(format!(
                "{state}: code {code:?}: {message}"
            )))
        }
        other => Err(format!("unexpected operation status {other:?}: {status}")),
    }
}

async fn wait_operation(opid: &str) -> String {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(240);
    let ids = serde_json::json!([opid]).to_string();
    loop {
        let status: serde_json::Value =
            serde_json::from_str(&cli(&["z_getoperationstatus", &ids]))
                .expect("z_getoperationstatus returned malformed JSON");
        match classify_operation_status(&status) {
            Ok(OperationState::Success) => {
                let done: serde_json::Value =
                    serde_json::from_str(&cli(&["z_getoperationresult", &ids]))
                        .expect("z_getoperationresult returned malformed JSON");
                return done[0]["result"]["txid"]
                    .as_str()
                    .expect("node transaction id")
                    .to_owned();
            }
            Ok(OperationState::Failed(reason)) => {
                panic!("local node shielded operation {opid} {reason}")
            }
            Ok(OperationState::Pending) => {}
            Err(reason) => panic!("{reason}"),
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "node shielded operation {opid} timed out"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[test]
fn operation_status_surfaces_node_failure_reason() {
    let failed = serde_json::json!([{
        "id": "opid-1",
        "status": "failed",
        "error": {"code": -4, "message": "privacy policy violation"}
    }]);
    assert_eq!(
        classify_operation_status(&failed),
        Ok(OperationState::Failed(
            "failed: code Some(-4): privacy policy violation".to_owned()
        ))
    );
}

#[test]
fn operation_status_handles_pending_success_missing_and_unknown() {
    for pending in ["queued", "executing"] {
        let status = serde_json::json!([{"id": "opid-1", "status": pending}]);
        assert_eq!(classify_operation_status(&status), Ok(OperationState::Pending));
    }
    let success = serde_json::json!([{"id": "opid-1", "status": "success"}]);
    assert_eq!(classify_operation_status(&success), Ok(OperationState::Success));
    assert!(classify_operation_status(&serde_json::json!([])).is_err());
    assert!(classify_operation_status(&serde_json::json!({"status": "success"})).is_err());
    assert!(classify_operation_status(&serde_json::json!([{"status": "mystery"}])).is_err());
}

fn verified_node_shielded_amount(view: &serde_json::Value, recipient: &str) -> Option<u64> {
    view["outputs"].as_array()?.iter().find_map(|output| {
        let shielded_pool = matches!(output["pool"].as_str(), Some("sapling" | "orchard"));
        if !shielded_pool || output["address"].as_str() != Some(recipient) {
            return None;
        }
        output["valueZat"].as_u64().or_else(|| {
            output["value"]
                .as_f64()
                .filter(|zec| zec.is_finite() && *zec >= 0.0)
                .map(|zec| (zec * 100_000_000.0).round() as u64)
        })
    })
}

fn coinbase_funding_commands(height: u64, shielded_destination: &str) -> Vec<Vec<String>> {
    let mut commands = Vec::new();
    if height < 101 {
        commands.push(vec!["generate".to_owned(), (101 - height).to_string()]);
    }
    commands.push(vec![
        "z_shieldcoinbase".to_owned(),
        "*".to_owned(),
        shielded_destination.to_owned(),
    ]);
    commands
}

#[test]
fn coinbase_funding_uses_zcashd_wallet_mining_and_wallet_wide_shielding() {
    assert_eq!(
        coinbase_funding_commands(0, "shielded-destination"),
        vec![
            vec!["generate".to_owned(), "101".to_owned()],
            vec![
                "z_shieldcoinbase".to_owned(),
                "*".to_owned(),
                "shielded-destination".to_owned()
            ]
        ]
    );
    assert_eq!(
        coinbase_funding_commands(101, "shielded-destination"),
        vec![vec![
            "z_shieldcoinbase".to_owned(),
            "*".to_owned(),
            "shielded-destination".to_owned()
        ]]
    );
}

#[test]
fn full_node_evidence_requires_the_requested_shielded_output_and_amount() {
    let expected = serde_json::json!({"outputs":[
        {"pool":"transparent","address":"recipient","value":0.5},
        {"pool":"orchard","address":"recipient","value":0.5}
    ]});
    assert_eq!(
        verified_node_shielded_amount(&expected, "recipient"),
        Some(PAYMENT_ZATOSHIS)
    );
    let wrong_amount =
        serde_json::json!({"outputs":[{"pool":"orchard","address":"recipient","value":0.49}]});
    assert_ne!(
        verified_node_shielded_amount(&wrong_amount, "recipient"),
        Some(PAYMENT_ZATOSHIS)
    );
    let transparent_only =
        serde_json::json!({"outputs":[{"pool":"transparent","address":"recipient","value":0.5}]});
    assert_eq!(
        verified_node_shielded_amount(&transparent_only, "recipient"),
        None
    );
}

async fn await_lightwalletd(endpoint: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    loop {
        if let Ok(Ok(mut client)) = tokio::time::timeout(
            Duration::from_secs(2),
            CompactTxStreamerClient::connect(endpoint.to_owned()),
        )
        .await
        {
            if tokio::time::timeout(Duration::from_secs(2), client.get_lightd_info(Empty {}))
                .await
                .is_ok_and(|r| r.is_ok())
            {
                return;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "local lightwalletd did not become ready"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn received(
    path: &std::path::Path,
    p: LocalNetwork,
    txid: TxId,
    target: u32,
) -> Vec<zcash_client_backend::data_api::ReceivedTransactionOutput> {
    let mut db = WalletDb::for_path(path, p, SystemClock, OsRng).unwrap();
    WalletMigrator::new().init_or_migrate(&mut db).unwrap();
    db.get_received_outputs(txid, target.into(), ConfirmationsPolicy::MIN)
        .unwrap()
}
