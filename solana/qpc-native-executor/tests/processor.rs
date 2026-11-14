//! Host processor tests only. Syscall stubs are NOT the Solana bank/runtime.
#![allow(deprecated)]
use qpc_native_executor::{
    intent_address, policy_address, process_instruction, Error, Instruction, Intent, Policy,
    INTENT_LEN, POLICY_LEN,
};
use solana_program::{
    account_info::AccountInfo,
    clock::Clock,
    entrypoint::ProgramResult,
    instruction::Instruction as CpiInstruction,
    program_error::ProgramError,
    program_stubs::{set_syscall_stubs, SyscallStubs},
    pubkey::Pubkey,
    rent::Rent,
    system_program,
};
use std::sync::{Arc, Mutex, MutexGuard};
static LOCK: Mutex<()> = Mutex::new(());
#[derive(Default)]
struct Observations {
    now: i64,
    fail_transfer: bool,
    cpis: Vec<CpiInstruction>,
    events: Vec<Vec<Vec<u8>>>,
}
struct Stubs {
    program: Pubkey,
    state: Arc<Mutex<Observations>>,
}
fn word(b: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(b[offset..offset + 8].try_into().unwrap())
}
fn debit_credit(from: &AccountInfo, to: &AccountInfo, value: u64) -> ProgramResult {
    let from_balance = from
        .lamports()
        .checked_sub(value)
        .ok_or(ProgramError::InsufficientFunds)?;
    let to_balance = to
        .lamports()
        .checked_add(value)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    **from.try_borrow_mut_lamports()? = from_balance;
    **to.try_borrow_mut_lamports()? = to_balance;
    Ok(())
}
impl SyscallStubs for Stubs {
    fn sol_log_data(&self, data: &[&[u8]]) {
        self.state
            .lock()
            .unwrap()
            .events
            .push(data.iter().map(|v| v.to_vec()).collect());
    }
    fn sol_get_clock_sysvar(&self, ptr: *mut u8) -> u64 {
        // Syscall ABI supplies a correctly sized, aligned Clock output pointer.
        unsafe {
            std::ptr::write(
                ptr.cast::<Clock>(),
                Clock {
                    unix_timestamp: self.state.lock().unwrap().now,
                    ..Clock::default()
                },
            );
        }
        0
    }
    fn sol_get_rent_sysvar(&self, ptr: *mut u8) -> u64 {
        unsafe {
            std::ptr::write(ptr.cast::<Rent>(), Rent::default());
        }
        0
    }
    fn sol_invoke_signed(
        &self,
        ix: &CpiInstruction,
        infos: &[AccountInfo],
        seeds: &[&[&[u8]]],
    ) -> ProgramResult {
        assert_eq!(ix.program_id, system_program::id());
        // Explicitly check the requested CPI privilege/signature domain; this
        // remains only a focused emulation, not bank account-lock verification.
        for meta in &ix.accounts {
            let ai = infos.iter().find(|a| a.key == &meta.pubkey).unwrap();
            assert!(!meta.is_writable || ai.is_writable);
            if meta.is_signer && !ai.is_signer {
                assert!(seeds
                    .iter()
                    .any(|s| Pubkey::create_program_address(s, &self.program).ok()
                        == Some(meta.pubkey)));
            }
        }
        self.state.lock().unwrap().cpis.push(ix.clone());
        let tag = u32::from_le_bytes(ix.data[..4].try_into().unwrap());
        let a = &infos[0];
        match tag {
            0 => {
                // CreateAccount { lamports, space, owner }
                assert_eq!(ix.data.len(), 52);
                let b = &infos[1];
                assert_eq!(b.owner, &system_program::id());
                assert!(b.data_is_empty());
                debit_credit(a, b, word(&ix.data, 4))?;
                *b.try_borrow_mut_data()? =
                    Box::leak(vec![0; word(&ix.data, 12) as usize].into_boxed_slice());
                b.assign(&Pubkey::new_from_array(ix.data[20..52].try_into().unwrap()));
            }
            2 => {
                // Transfer { lamports }
                assert_eq!(ix.data.len(), 12);
                if self.state.lock().unwrap().fail_transfer {
                    return Err(ProgramError::InsufficientFunds);
                }
                debit_credit(a, &infos[1], word(&ix.data, 4))?;
            }
            8 => {
                assert_eq!(ix.data.len(), 12);
                *a.try_borrow_mut_data()? =
                    Box::leak(vec![0; word(&ix.data, 4) as usize].into_boxed_slice());
            }
            1 => {
                assert_eq!(ix.data.len(), 36);
                a.assign(&Pubkey::new_from_array(ix.data[4..36].try_into().unwrap()));
            }
            _ => panic!("unexpected SystemProgram instruction {tag}"),
        };
        Ok(())
    }
}
struct Environment {
    _guard: MutexGuard<'static, ()>,
    prior: Option<Box<dyn SyscallStubs>>,
    state: Arc<Mutex<Observations>>,
    program: Pubkey,
}
impl Environment {
    fn new() -> Self {
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let program = Pubkey::new_unique();
        let state = Arc::new(Mutex::new(Observations {
            now: 100,
            ..Observations::default()
        }));
        let prior = set_syscall_stubs(Box::new(Stubs {
            program,
            state: state.clone(),
        }));
        Self {
            _guard: guard,
            prior: Some(prior),
            state,
            program,
        }
    }
    fn time(&self, time: i64) {
        self.state.lock().unwrap().now = time;
    }
    fn call(&self, a: &[AccountInfo], ix: Instruction) -> ProgramResult {
        process_instruction(&self.program, a, &ix.encode())
    }
}
impl Drop for Environment {
    fn drop(&mut self) {
        set_syscall_stubs(self.prior.take().unwrap());
    }
}
fn account(
    key: Pubkey,
    owner: Pubkey,
    signer: bool,
    writable: bool,
    lamports: u64,
    data: Vec<u8>,
) -> AccountInfo<'static> {
    AccountInfo::new(
        Box::leak(Box::new(key)),
        signer,
        writable,
        Box::leak(Box::new(lamports)),
        Box::leak(data.into_boxed_slice()),
        Box::leak(Box::new(owner)),
        false,
        0,
    )
}
struct Fixture {
    env: Environment,
    owner: AccountInfo<'static>,
    agent: AccountInfo<'static>,
    policy: AccountInfo<'static>,
    intent: AccountInfo<'static>,
    recipient: AccountInfo<'static>,
    sys: AccountInfo<'static>,
    p: Policy,
    i: Intent,
}
impl Fixture {
    fn new() -> Self {
        Self::prefunded(0)
    }
    fn prefunded(prefund: u64) -> Self {
        let env = Environment::new();
        let owner = account(
            Pubkey::new_unique(),
            system_program::id(),
            true,
            true,
            100_000_000,
            vec![],
        );
        let agent = account(
            Pubkey::new_unique(),
            system_program::id(),
            true,
            true,
            100_000,
            vec![],
        );
        let recipient = account(
            Pubkey::new_unique(),
            system_program::id(),
            false,
            true,
            5,
            vec![],
        );
        let policy = account(
            policy_address(&env.program, owner.key, agent.key).0,
            system_program::id(),
            false,
            true,
            prefund,
            vec![],
        );
        let intent = account(
            intent_address(&env.program, policy.key, 7).0,
            system_program::id(),
            false,
            true,
            prefund,
            vec![],
        );
        let mut sys = account(
            system_program::id(),
            Pubkey::new_unique(),
            false,
            false,
            1,
            vec![],
        );
        sys.executable = true;
        let p = Policy {
            owner: *owner.key,
            agent: *agent.key,
            genesis: [1; 32],
            policy_id: [2; 32],
            version: 1,
            epoch: 1,
            active: true,
        };
        let i = Intent {
            policy: *policy.key,
            authorization_commitment: [3; 32],
            intent_id: [4; 32],
            policy_commitment: [5; 32],
            recipient: *recipient.key,
            nonce: 7,
            amount: 100,
            version: 1,
            epoch: 1,
            issued_at: 90,
            not_before: 100,
            expires_at: 200,
            max_fee: 10,
            executed: false,
        };
        env.call(
            &[owner.clone(), policy.clone(), sys.clone()],
            Instruction::Initialize(p.clone()),
        )
        .unwrap();
        Self {
            env,
            owner,
            agent,
            policy,
            intent,
            recipient,
            sys,
            p,
            i,
        }
    }
    fn approve(&self, i: Intent) -> ProgramResult {
        self.env.call(
            &[
                self.owner.clone(),
                self.policy.clone(),
                self.intent.clone(),
                self.sys.clone(),
            ],
            Instruction::Approve(i),
        )
    }
    fn args(&self) -> Vec<AccountInfo<'static>> {
        vec![
            self.agent.clone(),
            self.policy.clone(),
            self.intent.clone(),
            self.recipient.clone(),
            self.sys.clone(),
        ]
    }
    fn execute(&self) -> ProgramResult {
        self.env
            .call(&self.args(), Instruction::Execute { nonce: 7 })
    }
    fn configure(&self, version: u64, epoch: u64, active: bool) -> ProgramResult {
        self.env.call(
            &[self.owner.clone(), self.policy.clone()],
            Instruction::Configure {
                version,
                epoch,
                active,
            },
        )
    }
    fn stored(&self) -> Intent {
        Intent::decode(&self.intent.try_borrow_data().unwrap()).unwrap()
    }
}
fn err(e: Error) -> Result<(), ProgramError> {
    Err(e.into())
}
#[test]
fn actual_entrypoint_initializes_pd_as_and_executes_exact_lamports_once() {
    let f = Fixture::new();
    assert_eq!(f.policy.owner, &f.env.program);
    assert_eq!(f.policy.data_len(), POLICY_LEN);
    assert_eq!(
        Policy::decode(&f.policy.try_borrow_data().unwrap()).unwrap(),
        f.p
    );
    f.approve(f.i.clone()).unwrap();
    assert_eq!(f.intent.owner, &f.env.program);
    assert_eq!(f.intent.data_len(), INTENT_LEN);
    let before = f.agent.lamports();
    f.execute().unwrap();
    assert_eq!(f.agent.lamports(), before - 100);
    assert_eq!(f.recipient.lamports(), 105);
    assert!(f.stored().executed);
    assert_eq!(f.execute(), err(Error::AlreadyExecuted));
    assert_eq!(f.recipient.lamports(), 105);
    let s = f.env.state.lock().unwrap();
    let transfer = s.cpis.last().unwrap();
    assert_eq!(transfer.accounts[0].pubkey, *f.agent.key);
    assert!(transfer.accounts[0].is_signer);
    assert_eq!(transfer.accounts[1].pubkey, *f.recipient.key);
    assert_eq!(word(&transfer.data, 4), 100);
    assert_eq!(
        s.events,
        vec![vec![
            b"QPC_EXECUTED_V1".to_vec(),
            f.i.authorization_commitment.to_vec(),
            f.policy.key.to_bytes().to_vec(),
            7u64.to_le_bytes().to_vec(),
            100u64.to_le_bytes().to_vec()
        ]]
    );
}
#[test]
fn prefunded_pd_as_allocate_assign_and_top_up_rent() {
    let f = Fixture::prefunded(1);
    f.approve(f.i.clone()).unwrap();
    f.execute().unwrap();
    assert_eq!(
        f.policy.lamports(),
        Rent::default().minimum_balance(POLICY_LEN)
    );
    assert_eq!(
        f.intent.lamports(),
        Rent::default().minimum_balance(INTENT_LEN)
    );
    let tags: Vec<_> = f
        .env
        .state
        .lock()
        .unwrap()
        .cpis
        .iter()
        .map(|ix| u32::from_le_bytes(ix.data[..4].try_into().unwrap()))
        .collect();
    assert_eq!(tags, vec![2, 8, 1, 2, 8, 1, 2]);
}
#[test]
fn approval_immutable_nonce_reservation_and_no_executed_forgery() {
    let f = Fixture::new();
    let mut invalid = f.i.clone();
    invalid.executed = true;
    assert_eq!(f.approve(invalid), err(Error::InvalidData));
    f.approve(f.i.clone()).unwrap();
    let snapshot = f.stored();
    let mut replacement = f.i.clone();
    replacement.recipient = Pubkey::new_unique();
    assert_eq!(f.approve(replacement), err(Error::AlreadyInitialized));
    assert_eq!(f.stored(), snapshot);
    assert_eq!(
        f.env.call(
            &[f.owner.clone(), f.policy.clone(), f.sys.clone()],
            Instruction::Initialize(f.p.clone())
        ),
        err(Error::AlreadyInitialized)
    );
}
#[test]
fn signer_recipient_program_owner_and_pda_substitution_rejected() {
    let f = Fixture::new();
    f.approve(f.i.clone()).unwrap();
    let mut a = f.args();
    a[0].is_signer = false;
    assert_eq!(
        f.env.call(&a, Instruction::Execute { nonce: 7 }),
        err(Error::MissingSignature)
    );
    a = f.args();
    a[3] = account(
        Pubkey::new_unique(),
        system_program::id(),
        false,
        true,
        0,
        vec![],
    );
    assert_eq!(
        f.env.call(&a, Instruction::Execute { nonce: 7 }),
        err(Error::WrongAccount)
    );
    a = f.args();
    a[0] = account(
        Pubkey::new_unique(),
        system_program::id(),
        true,
        true,
        1000,
        vec![],
    );
    assert_eq!(
        f.env.call(&a, Instruction::Execute { nonce: 7 }),
        err(Error::WrongAccount)
    );
    a = f.args();
    a[2] = account(
        Pubkey::new_unique(),
        f.env.program,
        false,
        true,
        1,
        f.i.encode(),
    );
    assert_eq!(
        f.env.call(&a, Instruction::Execute { nonce: 7 }),
        err(Error::WrongAccount)
    );
    a = f.args();
    a[2] = account(
        *f.intent.key,
        Pubkey::new_unique(),
        false,
        true,
        1,
        f.i.encode(),
    );
    assert_eq!(
        f.env.call(&a, Instruction::Execute { nonce: 7 }),
        err(Error::WrongOwner)
    );
    a = f.args();
    a[1] = account(
        Pubkey::new_unique(),
        f.env.program,
        false,
        false,
        1,
        f.p.encode(),
    );
    assert_eq!(
        f.env.call(&a, Instruction::Execute { nonce: 7 }),
        err(Error::WrongAccount)
    );
    assert_eq!(
        process_instruction(
            &Pubkey::new_unique(),
            &f.args(),
            &Instruction::Execute { nonce: 7 }.encode()
        ),
        err(Error::WrongOwner)
    );
    assert!(!f.stored().executed);
}
#[test]
fn writable_and_system_program_privileges_are_required() {
    let f = Fixture::new();
    f.approve(f.i.clone()).unwrap();
    for n in [0, 2, 3] {
        let mut a = f.args();
        a[n].is_writable = false;
        assert_eq!(
            f.env.call(&a, Instruction::Execute { nonce: 7 }),
            err(Error::NotWritable)
        );
    }
    let mut a = f.args();
    a[4].executable = false;
    assert_eq!(
        f.env.call(&a, Instruction::Execute { nonce: 7 }),
        err(Error::InvalidSystemProgram)
    );
    a = f.args();
    a[4] = account(
        Pubkey::new_unique(),
        system_program::id(),
        false,
        false,
        1,
        vec![],
    );
    assert_eq!(
        f.env.call(&a, Instruction::Execute { nonce: 7 }),
        err(Error::InvalidSystemProgram)
    );
}
#[test]
fn expiry_exact_boundary_and_not_before_use_clock_syscall() {
    let f = Fixture::new();
    f.approve(f.i.clone()).unwrap();
    f.env.time(99);
    assert_eq!(f.execute(), err(Error::InvalidTime));
    f.env.time(200);
    assert_eq!(f.execute(), err(Error::InvalidTime));
    f.env.time(-1);
    assert_eq!(f.execute(), err(Error::InvalidTime));
    f.env.time(199);
    f.execute().unwrap();
}
#[test]
fn revocation_and_monotonic_policy_epoch_invalidate_pending_approval() {
    let f = Fixture::new();
    f.approve(f.i.clone()).unwrap();
    assert_eq!(f.configure(1, 1, false), err(Error::InvalidPolicy));
    assert_eq!(f.configure(0, 2, false), err(Error::InvalidPolicy));
    f.configure(1, 2, false).unwrap();
    assert_eq!(f.execute(), err(Error::Revoked));
    f.configure(2, 3, true).unwrap();
    assert_eq!(f.execute(), err(Error::Revoked));
    assert_eq!(f.approve(f.i.clone()), err(Error::Revoked));
    assert!(!f.stored().executed);
}
#[test]
fn unauthorized_owner_cannot_approve_or_revoke() {
    let f = Fixture::new();
    let mut owner = f.owner.clone();
    owner.is_signer = false;
    assert_eq!(
        f.env.call(
            &[
                owner.clone(),
                f.policy.clone(),
                f.intent.clone(),
                f.sys.clone()
            ],
            Instruction::Approve(f.i.clone())
        ),
        err(Error::MissingSignature)
    );
    owner = account(
        Pubkey::new_unique(),
        system_program::id(),
        true,
        true,
        100_000,
        vec![],
    );
    assert_eq!(
        f.env.call(
            &[
                owner.clone(),
                f.policy.clone(),
                f.intent.clone(),
                f.sys.clone()
            ],
            Instruction::Approve(f.i.clone())
        ),
        err(Error::WrongAccount)
    );
    assert_eq!(
        f.env.call(
            &[owner, f.policy.clone()],
            Instruction::Configure {
                version: 1,
                epoch: 2,
                active: false
            }
        ),
        err(Error::WrongAccount)
    );
}
#[test]
fn approval_validates_amount_recipient_hashes_and_time() {
    let f = Fixture::new();
    let mut i = f.i.clone();
    i.amount = 0;
    assert_eq!(f.approve(i), err(Error::InvalidAmount));
    for recipient in [
        Pubkey::default(),
        *f.agent.key,
        *f.policy.key,
        *f.intent.key,
        f.env.program,
    ] {
        let mut i = f.i.clone();
        i.recipient = recipient;
        assert_eq!(f.approve(i), err(Error::InvalidAmount));
    }
    let mut i = f.i.clone();
    i.authorization_commitment = [0; 32];
    assert_eq!(f.approve(i), err(Error::InvalidData));
    for (issued, nb, exp) in [
        (101, 101, 200),
        (90, 89, 200),
        (90, 100, 100),
        (90, 200, 200),
    ] {
        let mut i = f.i.clone();
        i.issued_at = issued;
        i.not_before = nb;
        i.expires_at = exp;
        assert_eq!(f.approve(i), err(Error::InvalidTime));
    }
    let mut i = f.i.clone();
    i.epoch = 2;
    assert_eq!(f.approve(i), err(Error::Revoked));
    let mut i = f.i.clone();
    i.version = 2;
    assert_eq!(f.approve(i), err(Error::Revoked));
}
#[test]
fn strict_wire_lengths_booleans_and_overflow_cannot_change_execution_params() {
    let f = Fixture::new();
    f.approve(f.i.clone()).unwrap();
    for i in [
        Instruction::Initialize(f.p.clone()),
        Instruction::Approve(f.i.clone()),
        Instruction::Configure {
            version: 1,
            epoch: 2,
            active: false,
        },
        Instruction::Execute { nonce: u64::MAX },
    ] {
        let bytes = i.encode();
        assert_eq!(Instruction::decode(&bytes).unwrap(), i);
        for n in 0..bytes.len() {
            assert!(Instruction::decode(&bytes[..n]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(Instruction::decode(&trailing).is_err());
    }
    for mut bytes in [
        Instruction::Initialize(f.p.clone()).encode(),
        Instruction::Approve(f.i.clone()).encode(),
        Instruction::Configure {
            version: 1,
            epoch: 2,
            active: true,
        }
        .encode(),
    ] {
        *bytes.last_mut().unwrap() = 2;
        assert!(Instruction::decode(&bytes).is_err());
    }
    let mut forged = Instruction::Execute { nonce: 7 }.encode();
    forged.extend(999u64.to_le_bytes());
    assert_eq!(
        process_instruction(&f.env.program, &f.args(), &forged),
        err(Error::InvalidData)
    );
    assert_eq!(
        f.env
            .call(&f.args(), Instruction::Execute { nonce: u64::MAX }),
        err(Error::WrongAccount)
    );
    assert!(!f.stored().executed);
}
#[test]
fn failed_cpi_host_does_not_fake_bank_rollback_or_emit_success() {
    let f = Fixture::new();
    f.approve(f.i.clone()).unwrap();
    let before = f.agent.lamports();
    f.env.state.lock().unwrap().fail_transfer = true;
    assert_eq!(f.execute(), Err(ProgramError::InsufficientFunds));
    assert_eq!(f.agent.lamports(), before);
    assert_eq!(f.recipient.lamports(), 5);
    // This proves the write precedes CPI. Host invocation DOES NOT roll it back!
    // Real transaction failure is expected to roll back via the Solana bank,
    // which has NOT been exercised by this test. Never report this as runtime integration.
    assert!(f.stored().executed);
    assert!(f.env.state.lock().unwrap().events.is_empty());
}
#[test]
fn initialization_rejects_arbitrary_accounts_and_invalid_policy_metadata() {
    let f = Fixture::new();
    let arbitrary = account(
        Pubkey::new_unique(),
        system_program::id(),
        false,
        true,
        0,
        vec![],
    );
    assert_eq!(
        f.env.call(
            &[f.owner.clone(), arbitrary, f.sys.clone()],
            Instruction::Initialize(f.p.clone())
        ),
        err(Error::WrongAccount)
    );
    for change in 0..6 {
        let mut p = f.p.clone();
        match change {
            0 => p.genesis = [0; 32],
            1 => p.policy_id = [0; 32],
            2 => p.epoch = 0,
            3 => p.version = 0,
            4 => p.active = false,
            _ => p.owner = Pubkey::new_unique(),
        }
        assert_eq!(
            f.env.call(
                &[f.owner.clone(), f.policy.clone(), f.sys.clone()],
                Instruction::Initialize(p)
            ),
            err(Error::InvalidPolicy)
        );
    }
    let mut owner = f.owner.clone();
    owner.is_signer = false;
    assert_eq!(
        f.env.call(
            &[owner, f.policy.clone(), f.sys.clone()],
            Instruction::Initialize(f.p.clone())
        ),
        err(Error::MissingSignature)
    );
}

#[test]
fn maximum_u64_amount_is_not_narrowed_or_wrapped() {
    let f = Fixture::new();
    let mut i = f.i.clone();
    i.amount = u64::MAX;
    f.approve(i.clone()).unwrap();
    assert_eq!(f.stored().amount, u64::MAX);
    let before = f.agent.lamports();
    assert_eq!(f.execute(), Err(ProgramError::InsufficientFunds));
    assert_eq!(f.agent.lamports(), before);
    assert_eq!(f.recipient.lamports(), 5);
    let state = f.env.state.lock().unwrap();
    assert_eq!(word(&state.cpis.last().unwrap().data, 4), u64::MAX);
    assert!(state.events.is_empty());
}

#[test]
fn empty_accounts_malformed_policy_and_instruction_fail_without_panics() {
    let f = Fixture::new();
    assert_eq!(
        f.env.call(&[], Instruction::Execute { nonce: 7 }),
        Err(ProgramError::NotEnoughAccountKeys)
    );
    assert_eq!(
        process_instruction(&f.env.program, &[], &[255]),
        err(Error::InvalidData)
    );
    let fake = account(
        *f.policy.key,
        f.env.program,
        false,
        false,
        1,
        vec![0; POLICY_LEN],
    );
    assert_eq!(
        f.env.call(
            &[f.owner.clone(), fake],
            Instruction::Configure {
                version: 2,
                epoch: 2,
                active: false
            }
        ),
        err(Error::InvalidData)
    );
}
