//! Offline WASM ABI checks, not a nearcore node or receipt scheduler.
#[cfg(test)]
mod tests {
    use near_parameters::{vm::VMKind, RuntimeConfigStore};
    use near_primitives_core::version::PROTOCOL_VERSION;
    use near_vm_runner::{
        logic::{
            mocks::mock_external::{MockAction, MockedExternal},
            types::{PromiseResult, ReturnData},
            VMContext, VMOutcome,
        },
        ContractCode, FilesystemContractRuntimeCache,
    };
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use std::{path::PathBuf, sync::Arc};

    const NOW: u64 = 1_800_000_000;
    const CONTRACT: &str = "qpc.testnet";
    const AGENT: &str = "agent.testnet";
    const OWNER: &str = "wkams.testnet";
    const RECIPIENT: &str = "recipient.testnet";

    // serde_json's default map is key-sorted; this is the same canonical ASCII
    // profile as the contract, not a host invocation of contract Rust methods.
    fn hash(domain: &str, value: &Value) -> String {
        format!(
            "{:x}",
            Sha256::digest(format!("{domain}{value}").as_bytes())
        )
    }
    fn commitment(envelope: &Value) -> String {
        hash("QPC-CROSS-L1-AUTHORIZATION-V1\0", envelope)
    }

    struct Vm {
        external: MockedExternal,
        context: VMContext,
        cache: FilesystemContractRuntimeCache,
    }
    impl Vm {
        fn new() -> Self {
            let wasm = std::env::var_os("QPC_NEAR_WASM").map(PathBuf::from).unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../qpc-near-agent-finance/target/wasm32-unknown-unknown/release/qpc_near_agent_finance.compatible.wasm")
            });
            let bytes = std::fs::read(&wasm)
                .unwrap_or_else(|e| panic!("build release WASM first ({}): {e}", wasm.display()));
            assert_eq!(&bytes[..4], b"\0asm");
            eprintln!(
                "Executing WASM sha256={:x}, protocol={PROTOCOL_VERSION}",
                Sha256::digest(&bytes)
            );
            Self {
                external: MockedExternal::with_code(ContractCode::new(bytes, None)),
                cache: FilesystemContractRuntimeCache::test().unwrap(),
                context: VMContext {
                    current_account_id: CONTRACT.parse().unwrap(),
                    signer_account_id: AGENT.parse().unwrap(),
                    signer_account_pk: vec![0; 33],
                    predecessor_account_id: OWNER.parse().unwrap(),
                    input: vec![],
                    promise_results: vec![].into(),
                    block_height: 100,
                    block_timestamp: NOW * 1_000_000_000,
                    epoch_height: 1,
                    account_balance: 100 * 10u128.pow(24),
                    account_locked_balance: 0,
                    storage_usage: 10_000,
                    attached_deposit: 0,
                    prepaid_gas: 100_000_000_000_000,
                    random_seed: vec![0; 32],
                    view_config: None,
                    output_data_receivers: vec![],
                },
            }
        }
        fn invoke(
            &mut self,
            caller: &str,
            method: &str,
            args: Value,
            deposit: u128,
            results: Vec<PromiseResult>,
        ) -> VMOutcome {
            self.context.predecessor_account_id = caller.parse().unwrap();
            self.context.input = serde_json::to_vec(&args).unwrap();
            self.context.attached_deposit = deposit;
            self.context.promise_results = results.into();
            self.external.action_log.clear();
            // The VM runner delegates transaction rollback to its caller. Emulate
            // that boundary explicitly instead of retaining failed call writes.
            let trie = self.external.fake_trie.clone();
            let store = RuntimeConfigStore::new(None);
            let runtime = store.get_config(PROTOCOL_VERSION);
            let mut config = (*runtime.wasm_config).clone();
            config.vm_kind = VMKind::Wasmtime;
            let gas = self.context.make_gas_counter(&config);
            let prepared = near_vm_runner::prepare(
                &self.external,
                Arc::new(config),
                Some(&self.cache),
                gas,
                method,
            );
            let outcome = near_vm_runner::run(
                prepared,
                &mut self.external,
                &self.context,
                runtime.fees.clone(),
            )
            .expect("VM runner internal error");
            if outcome.aborted.is_some() {
                self.external.fake_trie = trie;
                self.external.action_log.clear();
            } else {
                self.context.storage_usage = outcome.storage_usage;
            }
            outcome
        }
        fn ok(
            &mut self,
            caller: &str,
            method: &str,
            args: Value,
            deposit: u128,
            results: Vec<PromiseResult>,
        ) -> VMOutcome {
            let outcome = self.invoke(caller, method, args, deposit, results);
            assert!(outcome.aborted.is_none(), "{method}: {:?}", outcome.aborted);
            assert!(outcome.burnt_gas > 0, "WASM execution must be metered");
            assert!(outcome.used_gas <= self.context.prepaid_gas);
            eprintln!(
                "{method}: burnt_gas={} used_gas={}",
                outcome.burnt_gas, outcome.used_gas
            );
            outcome
        }
        fn reject(
            &mut self,
            caller: &str,
            method: &str,
            args: Value,
            deposit: u128,
            results: Vec<PromiseResult>,
            expected: &str,
        ) {
            let trie = self.external.fake_trie.clone();
            let outcome = self.invoke(caller, method, args, deposit, results);
            let error = format!("{:?}", outcome.aborted.expect("call must reject"));
            assert!(
                error.contains(expected),
                "{method}: {error}, expected {expected}"
            );
            assert_eq!(self.external.fake_trie, trie);
            assert!(self.external.action_log.is_empty());
        }
        fn get(&mut self, method: &str, intent: &str) -> Value {
            let trie = self.external.fake_trie.clone();
            self.context.view_config = Some(near_primitives_core::config::ViewConfig {
                max_gas_burnt: 10_000_000_000_000,
            });
            let outcome = self.ok(
                AGENT,
                method,
                json!({"agent_id":AGENT,"intent_id":intent}),
                0,
                vec![],
            );
            self.context.view_config = None;
            assert_eq!(self.external.fake_trie, trie, "view getter mutated storage");
            match outcome.return_data {
                ReturnData::Value(bytes) => serde_json::from_slice(&bytes).unwrap(),
                other => panic!("expected JSON getter value: {other:?}"),
            }
        }
        fn setup(&mut self) -> Value {
            self.ok(OWNER, "new", json!({"owner":OWNER}), 0, vec![]);
            self.ok(OWNER, "configure_agent", json!({"agent":AGENT,"policy_id":"policy-1","router":CONTRACT,"expires_at_unix":NOW+10000,"max_per_payment":"5000","total_cap":"10000"}), 0, vec![]);
            let fixture: Value =
                serde_json::from_str(include_str!("../../../docs/fixtures/cross-l1-v1.json"))
                    .unwrap();
            let mut e = fixture["envelope"].clone();
            e["policy"]["version"] = json!(1);
            e["nonce"] = json!("0");
            e["destination"] = json!({"networkId":"near:testnet","nativeChainId":"testnet","adapterId":CONTRACT,"recipientCommitment":hash("QPC-NATIVE-RECIPIENT-V1\0", &json!({"chain":"near","nativeChainId":"testnet","recipient":RECIPIENT}))});
            e["asset"] = json!({"sourceAssetId":"near:native","destinationAssetId":"near:native","sourceDecimals":24,"destinationDecimals":24,"mappingId":"near-native-identity-v1"});
            e["action"] = json!({"type":"asset-transfer","operation":"pay"});
            e["amount"] = json!({"sourceAtomic":"1000","destinationAtomic":"1000"});
            e["fees"] = json!({"maxFeeAtomic":"1000000000000000000000","feeAssetId":"near:native","maxSlippageBps":0});
            e["settlement"] = json!({"evidenceClass":"near-native-transfer-result","finalityPolicy":"near-execution-outcome","minConfirmations":0});
            self.ok(
                OWNER,
                "configure_envelope_profile",
                json!({"agent":AGENT,"envelope":e}),
                0,
                vec![],
            );
            self.ok(OWNER, "set_recipient_permission", json!({"agent":AGENT,"opaque_recipient_ref":e["destination"]["recipientCommitment"],"allowed":true}), 0, vec![]);
            e
        }
        fn execution_args(e: &Value) -> Value {
            json!({"envelope":e,"authorization_commitment":commitment(e),"recipient":RECIPIENT})
        }
        // Inspect VM host actions including the transfer dependency; do not
        // pretend that the mocked external itself executes the transfer.
        fn scheduled_callback(&self, receiver: &str, method: &str) -> Value {
            let actions = &self.external.action_log;
            let (transfer_index, amount) = actions
                .iter()
                .find_map(|a| match a {
                    MockAction::Transfer {
                        receipt_index,
                        deposit,
                    } => Some((*receipt_index, *deposit)),
                    _ => None,
                })
                .expect("transfer scheduled");
            assert_eq!(amount, 1000);
            assert!(actions.iter().any(|a| matches!(a, MockAction::CreateReceipt { receiver_id, receipt_indices, .. } if receiver_id.as_str()==receiver && receipt_indices.is_empty())));
            let (callback_index, args) = actions
                .iter()
                .find_map(|a| match a {
                    MockAction::FunctionCallWeight {
                        receipt_index,
                        method_name,
                        args,
                        attached_deposit,
                        prepaid_gas,
                        gas_weight,
                        ..
                    } if method_name == method.as_bytes() => {
                        assert_eq!(*prepaid_gas, 10_000_000_000_000);
                        assert_eq!(gas_weight.0, 0);
                        assert_eq!(*attached_deposit, 0);
                        Some((
                            *receipt_index,
                            serde_json::from_slice::<Value>(args).unwrap(),
                        ))
                    }
                    _ => None,
                })
                .expect("private callback scheduled");
            // Mock receipt indices are positions in action_log, not ordinal IDs.
            assert!(
                matches!(&actions[transfer_index as usize], MockAction::CreateReceipt { receiver_id, .. } if receiver_id.as_str()==receiver)
            );
            assert!(
                matches!(&actions[callback_index as usize], MockAction::CreateReceipt { receiver_id, receipt_indices } if receiver_id.as_str()==CONTRACT && receipt_indices==&vec![transfer_index])
            );
            args
        }
    }

    #[test]
    fn wasm_rejects_incompatible_root_before_views_mutations_or_reinitialization() {
        let mut vm = Vm::new();
        vm.setup();
        let root = vm
            .external
            .fake_trie
            .get(b"STATE".as_slice())
            .unwrap()
            .clone();
        assert_eq!(&root[..16], b"QPCNEAR\0LAYOUT\0\0");
        assert_eq!(&root[16..20], &1_u32.to_le_bytes());
        let outcome = vm.ok(AGENT, "get_state_layout_version", json!({}), 0, vec![]);
        assert!(matches!(outcome.return_data, ReturnData::Value(bytes) if bytes == b"1"));
        for incompatible in [
            root[20..].to_vec(), // Exact previous unversioned root encoding.
            {
                let mut b = root.clone();
                b[0] ^= 1;
                b
            },
            {
                let mut b = root.clone();
                b[16..20].copy_from_slice(&2_u32.to_le_bytes());
                b
            },
            root[..19].to_vec(),
        ] {
            vm.external
                .fake_trie
                .insert(b"STATE".to_vec(), incompatible);
            vm.reject(
                AGENT,
                "get_agent_policy",
                json!({"agent":AGENT}),
                0,
                vec![],
                "Cannot deserialize the contract state",
            );
            vm.reject(
                OWNER,
                "revoke_agent",
                json!({"agent":AGENT}),
                0,
                vec![],
                "Cannot deserialize the contract state",
            );
            vm.reject(
                OWNER,
                "new",
                json!({"owner":OWNER}),
                0,
                vec![],
                "initialized",
            );
        }
        vm.external.fake_trie.insert(b"STATE".to_vec(), root);
        vm.ok(AGENT, "get_agent_policy", json!({"agent":AGENT}), 0, vec![]);
    }

    #[test]
    fn wasm_authorization_is_durable_but_cannot_execute_or_settle() {
        let mut vm = Vm::new();
        let e = vm.setup();
        let intent = e["intentId"].as_str().unwrap();
        vm.ok(
            AGENT,
            "authorize_envelope",
            json!({"envelope":e,"authorization_commitment":commitment(&e)}),
            0,
            vec![],
        );
        let receipt = vm.get("get_authorization_receipt", intent);
        assert_eq!(receipt["authorizationCommitment"], commitment(&e));
        assert!(vm.get("get_execution_result", intent).is_null());
        vm.reject(
            AGENT,
            "execute_near_envelope",
            Vm::execution_args(&e),
            1000,
            vec![],
            "nonce",
        );
        vm.reject(
            AGENT,
            "settle_envelope",
            json!({"intent_id":intent,"authorization_commitment":commitment(&e)}),
            0,
            vec![],
            "native external settlement not implemented",
        );
    }

    #[test]
    fn wasm_execution_success_private_callback_and_replay_boundary() {
        let mut vm = Vm::new();
        let e = vm.setup();
        let intent = e["intentId"].as_str().unwrap();
        vm.reject(
            AGENT,
            "execute_near_envelope",
            Vm::execution_args(&e),
            999,
            vec![],
            "deposit",
        );
        vm.ok(
            AGENT,
            "execute_near_envelope",
            Vm::execution_args(&e),
            1000,
            vec![],
        );
        let args = vm.scheduled_callback(RECIPIENT, "on_near_transfer_result");
        assert_eq!(args["authorization_commitment"], commitment(&e));
        assert_eq!(vm.get("get_execution_result", intent)["state"], "PENDING");
        vm.reject(
            AGENT,
            "on_near_transfer_result",
            args.clone(),
            0,
            vec![PromiseResult::Successful(vec![])],
            "private",
        );
        vm.context.prepaid_gas = 10_000_000_000_000;
        vm.ok(
            CONTRACT,
            "on_near_transfer_result",
            args.clone(),
            0,
            vec![PromiseResult::Successful(vec![])],
        );
        vm.context.prepaid_gas = 100_000_000_000_000;
        let result = vm.get("get_execution_result", intent);
        assert_eq!(result["state"], "EXECUTED");
        assert_eq!(result["feeBudgetEnforced"], false);
        assert_eq!(result["protocolGasFee"], "NOT_VERIFIED");
        vm.reject(
            CONTRACT,
            "on_near_transfer_result",
            args,
            0,
            vec![PromiseResult::Successful(vec![])],
            "execution already completed",
        );
        vm.reject(
            AGENT,
            "execute_near_envelope",
            Vm::execution_args(&e),
            1000,
            vec![],
            "execution already exists",
        );
        vm.reject(
            AGENT,
            "claim_failed_near_refund",
            json!({"intent_id":intent}),
            0,
            vec![],
            "failed",
        );
    }

    #[test]
    fn wasm_gas_expiry_and_revocation_fail_without_state_consumption() {
        let mut vm = Vm::new();
        let e = vm.setup();
        let intent = e["intentId"].as_str().unwrap();
        vm.context.prepaid_gas = 39_000_000_000_000;
        vm.reject(
            AGENT,
            "execute_near_envelope",
            Vm::execution_args(&e),
            1000,
            vec![],
            "gas",
        );
        vm.context.prepaid_gas = 100_000_000_000_000;
        assert!(vm.get("get_execution_result", intent).is_null());
        vm.context.block_timestamp = e["time"]["expiresAtUnix"].as_u64().unwrap() * 1_000_000_000;
        vm.reject(
            AGENT,
            "execute_near_envelope",
            Vm::execution_args(&e),
            1000,
            vec![],
            "expired",
        );
        vm.context.block_timestamp = NOW * 1_000_000_000;
        vm.ok(OWNER, "revoke_agent", json!({"agent":AGENT}), 0, vec![]);
        vm.reject(
            AGENT,
            "execute_near_envelope",
            Vm::execution_args(&e),
            1000,
            vec![],
            "revoked",
        );
        assert!(vm.get("get_authorization_receipt", intent).is_null());
        assert!(vm.get("get_execution_result", intent).is_null());
    }

    #[test]
    fn wasm_failed_transfer_refund_failure_retry_and_exactly_once_success() {
        let mut vm = Vm::new();
        let e = vm.setup();
        let intent = e["intentId"].as_str().unwrap();
        vm.ok(
            AGENT,
            "execute_near_envelope",
            Vm::execution_args(&e),
            1000,
            vec![],
        );
        let args = vm.scheduled_callback(RECIPIENT, "on_near_transfer_result");
        vm.ok(
            CONTRACT,
            "on_near_transfer_result",
            args,
            0,
            vec![PromiseResult::Failed],
        );
        assert_eq!(
            vm.get("get_execution_result", intent)["refundState"],
            "AVAILABLE"
        );
        vm.reject(
            OWNER,
            "claim_failed_near_refund",
            json!({"intent_id":intent}),
            0,
            vec![],
            "agent",
        );
        vm.ok(
            AGENT,
            "claim_failed_near_refund",
            json!({"intent_id":intent}),
            0,
            vec![],
        );
        let first = vm.scheduled_callback(AGENT, "on_near_refund_result");
        vm.reject(
            AGENT,
            "claim_failed_near_refund",
            json!({"intent_id":intent}),
            0,
            vec![],
            "pending",
        );
        vm.reject(
            AGENT,
            "on_near_refund_result",
            first.clone(),
            0,
            vec![PromiseResult::Successful(vec![])],
            "private",
        );
        vm.ok(
            CONTRACT,
            "on_near_refund_result",
            first.clone(),
            0,
            vec![PromiseResult::Failed],
        );
        assert_eq!(
            vm.get("get_execution_result", intent)["refundState"],
            "RETRYABLE"
        );
        vm.ok(
            AGENT,
            "claim_failed_near_refund",
            json!({"intent_id":intent}),
            0,
            vec![],
        );
        let second = vm.scheduled_callback(AGENT, "on_near_refund_result");
        assert_eq!(second["refund_attempt"], 2);
        vm.reject(
            CONTRACT,
            "on_near_refund_result",
            first,
            0,
            vec![PromiseResult::Successful(vec![])],
            "attempt",
        );
        vm.context.prepaid_gas = 10_000_000_000_000;
        vm.ok(
            CONTRACT,
            "on_near_refund_result",
            second.clone(),
            0,
            vec![PromiseResult::Successful(vec![])],
        );
        vm.context.prepaid_gas = 100_000_000_000_000;
        let result = vm.get("get_execution_result", intent);
        assert_eq!(result["state"], "FAILED");
        assert_eq!(result["refundState"], "REFUNDED");
        vm.reject(
            CONTRACT,
            "on_near_refund_result",
            second,
            0,
            vec![PromiseResult::Successful(vec![])],
            "pending",
        );
        vm.reject(
            AGENT,
            "claim_failed_near_refund",
            json!({"intent_id":intent}),
            0,
            vec![],
            "pending",
        );
        vm.reject(
            AGENT,
            "execute_near_envelope",
            Vm::execution_args(&e),
            1000,
            vec![],
            "execution already exists",
        );
    }
}
