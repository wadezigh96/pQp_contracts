//! Offline SBF runtime verification. No signing, RPC, or deployment.
//! Signature verification is explicitly disabled; signer account metadata is
//! exercised, cryptographic authorization is NOT tested by this harness.
//! LiteSVM internally creates an unused ephemeral airdrop keypair; the harness
//! never uses it, signs with it, or persists any key.
#[cfg(test)]
mod tests {
    use litesvm::LiteSVM;
    use qpc_native_executor::{intent_address, policy_address, Instruction as Qpc, Intent, Policy};
    use solana_account::Account;
    use solana_message::Message;
    #[allow(deprecated)] // Same pinned SDK SystemProgram identifier as processor.
    use solana_program::{
        clock::Clock,
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        system_program,
    };
    use solana_transaction::Transaction;

    const NOW: u64 = 1_800_000_000;
    struct Fixture {
        svm: LiteSVM,
        program: Pubkey,
        owner: Pubkey,
        agent: Pubkey,
        recipient: Pubkey,
        policy: Pubkey,
        intent: Pubkey,
        approved: Intent,
    }
    fn balance(svm: &LiteSVM, key: &Pubkey) -> u64 {
        svm.get_account(key).map_or(0, |a| a.lamports)
    }
    fn system_account(lamports: u64) -> Account {
        Account {
            lamports,
            data: vec![],
            owner: system_program::id(),
            executable: false,
            rent_epoch: 0,
        }
    }
    impl Fixture {
        fn new() -> Self {
            let program = Pubkey::new_from_array([51; 32]);
            let owner = Pubkey::new_from_array([52; 32]);
            let agent = Pubkey::new_from_array([53; 32]);
            let recipient = Pubkey::new_from_array([54; 32]);
            let policy = policy_address(&program, &owner, &agent).0;
            let intent = intent_address(&program, &policy, 7).0;
            let mut svm = LiteSVM::new()
                .with_sigverify(false)
                .with_transaction_history(0);
            // Seed synthetic balances directly; LiteSVM::airdrop signs internally,
            // so it is deliberately not used under this task's no-signing limit.
            for key in [owner, agent, recipient] {
                svm.set_account(key, system_account(100_000_000)).unwrap();
            }
            let artifact = std::env::var("QPC_SBF_ELF")
                .expect("QPC_SBF_ELF must point at the freshly built program ELF");
            svm.add_program_from_file(program, artifact)
                .expect("load real compiled SBF ELF");
            let mut clock = svm.get_sysvar::<Clock>();
            clock.unix_timestamp = NOW as i64;
            svm.set_sysvar(&clock);
            let approved = Intent {
                policy,
                authorization_commitment: [1; 32],
                intent_id: [2; 32],
                policy_commitment: [3; 32],
                recipient,
                nonce: 7,
                amount: 1_000_000,
                version: 1,
                epoch: 1,
                issued_at: NOW,
                not_before: NOW,
                expires_at: NOW + 60,
                max_fee: 10_000,
                executed: false,
            };
            let mut f = Self {
                svm,
                program,
                owner,
                agent,
                recipient,
                policy,
                intent,
                approved,
            };
            let p = Policy {
                owner,
                agent,
                genesis: [4; 32],
                policy_id: [5; 32],
                version: 1,
                epoch: 1,
                active: true,
            };
            let init = f.ix(
                Qpc::Initialize(p),
                vec![
                    AccountMeta::new(owner, true),
                    AccountMeta::new(policy, false),
                    AccountMeta::new_readonly(system_program::id(), false),
                ],
            );
            f.run(init, owner).unwrap();
            assert_eq!(f.svm.get_account(&policy).unwrap().owner, program);
            let approve = f.ix(
                Qpc::Approve(f.approved.clone()),
                vec![
                    AccountMeta::new(owner, true),
                    AccountMeta::new_readonly(policy, false),
                    AccountMeta::new(intent, false),
                    AccountMeta::new_readonly(system_program::id(), false),
                ],
            );
            f.run(approve, owner).unwrap();
            assert_eq!(
                f.svm.get_account(&intent).unwrap().data,
                f.approved.encode()
            );
            f
        }
        fn ix(&self, q: Qpc, accounts: Vec<AccountMeta>) -> Instruction {
            Instruction {
                program_id: self.program,
                accounts,
                data: q.encode(),
            }
        }
        fn execute(&self) -> Instruction {
            self.ix(
                Qpc::Execute { nonce: 7 },
                vec![
                    AccountMeta::new(self.agent, true),
                    AccountMeta::new_readonly(self.policy, false),
                    AccountMeta::new(self.intent, false),
                    AccountMeta::new(self.recipient, false),
                    AccountMeta::new_readonly(system_program::id(), false),
                ],
            )
        }
        fn run(
            &mut self,
            ix: Instruction,
            payer: Pubkey,
        ) -> Result<
            litesvm::types::TransactionMetadata,
            Box<litesvm::types::FailedTransactionMetadata>,
        > {
            let message =
                Message::new_with_blockhash(&[ix], Some(&payer), &self.svm.latest_blockhash());
            // new_unsigned fills required signature slots with zero defaults;
            // no Keypair, signing API, or broadcast path is used.
            self.svm
                .send_transaction(Transaction::new_unsigned(message))
                .map_err(Box::new)
        }
        fn record(&self) -> Intent {
            Intent::decode(&self.svm.get_account(&self.intent).unwrap().data).unwrap()
        }
    }
    #[test]
    fn sbf_initializes_pdas_executes_exact_transfer_and_rejects_replay() {
        let mut f = Fixture::new();
        let before = balance(&f.svm, &f.recipient);
        let agent_before = balance(&f.svm, &f.agent);
        let result = f.run(f.execute(), f.owner).unwrap();
        assert_eq!(balance(&f.svm, &f.recipient) - before, f.approved.amount);
        assert_eq!(agent_before - balance(&f.svm, &f.agent), f.approved.amount);
        assert!(f.record().executed);
        assert!(result
            .logs
            .iter()
            .any(|l| l.contains("QPC_EXECUTED nonce=7 amount=1000000")));
        let after = balance(&f.svm, &f.recipient);
        assert!(f.run(f.execute(), f.owner).is_err());
        assert_eq!(balance(&f.svm, &f.recipient), after);
    }
    #[test]
    fn failed_system_cpi_rolls_back_intent_and_balances_then_exact_retry_succeeds() {
        let mut f = Fixture::new();
        f.svm.set_account(f.agent, system_account(890_880)).unwrap();
        let before_record = f.svm.get_account(&f.intent).unwrap();
        let recipient_before = balance(&f.svm, &f.recipient);
        let agent_before = balance(&f.svm, &f.agent);
        let failed = f.run(f.execute(), f.owner).unwrap_err();
        assert!(
            failed
                .meta
                .logs
                .iter()
                .any(|l| l.contains("insufficient lamports")),
            "{:?}",
            failed.meta.logs
        );
        assert_eq!(
            f.svm.get_account(&f.intent).unwrap().data,
            before_record.data
        );
        assert!(!f.record().executed);
        assert_eq!(balance(&f.svm, &f.recipient), recipient_before);
        assert_eq!(balance(&f.svm, &f.agent), agent_before);
        f.svm
            .set_account(f.agent, system_account(100_000_000))
            .unwrap();
        f.run(f.execute(), f.owner).unwrap();
        assert!(f.record().executed);
        assert_eq!(
            balance(&f.svm, &f.recipient) - recipient_before,
            f.approved.amount
        );
        assert!(f.run(f.execute(), f.owner).is_err());
    }
    #[test]
    fn transaction_late_failure_rolls_back_successful_cpi_and_record() {
        let mut f = Fixture::new();
        let before = f.svm.get_account(&f.intent).unwrap().data;
        let recipient_before = balance(&f.svm, &f.recipient);
        let execute = f.execute();
        let malformed = Instruction {
            program_id: f.program,
            accounts: vec![],
            data: vec![255],
        };
        let message = Message::new_with_blockhash(
            &[execute, malformed],
            Some(&f.owner),
            &f.svm.latest_blockhash(),
        );
        let failed = f
            .svm
            .send_transaction(Transaction::new_unsigned(message))
            .unwrap_err();
        assert!(failed
            .meta
            .logs
            .iter()
            .any(|l| l.contains("QPC_EXECUTED nonce=7")));
        assert_eq!(f.svm.get_account(&f.intent).unwrap().data, before);
        assert_eq!(balance(&f.svm, &f.recipient), recipient_before);
        f.run(f.execute(), f.owner).unwrap();
    }
    #[test]
    fn sbf_rejects_wrong_recipient_missing_signer_expiry_and_revocation() {
        let mut f = Fixture::new();
        let before = f.svm.get_account(&f.intent).unwrap().data;
        let mut wrong = f.execute();
        wrong.accounts[3].pubkey = f.owner;
        assert!(f.run(wrong, f.owner).is_err());
        let mut unsigned_agent = f.execute();
        unsigned_agent.accounts[0].is_signer = false;
        assert!(f.run(unsigned_agent, f.owner).is_err());
        let mut clock = f.svm.get_sysvar::<Clock>();
        clock.unix_timestamp = (NOW + 60) as i64;
        f.svm.set_sysvar(&clock);
        assert!(f.run(f.execute(), f.owner).is_err());
        clock.unix_timestamp = NOW as i64;
        f.svm.set_sysvar(&clock);
        let revoke = f.ix(
            Qpc::Configure {
                version: 1,
                epoch: 2,
                active: false,
            },
            vec![
                AccountMeta::new_readonly(f.owner, true),
                AccountMeta::new(f.policy, false),
            ],
        );
        f.run(revoke, f.owner).unwrap();
        assert!(f.run(f.execute(), f.owner).is_err());
        assert_eq!(f.svm.get_account(&f.intent).unwrap().data, before);
    }
}
