//! Owner-attested exact native execution. NOT a canonical-JSON verifier, cluster
//! verifier, fee limiter, finality verifier, bridge, or settlement capability.
#![allow(deprecated)] // Pinned SDK's SystemProgram helpers retain stable wire ABI.
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    system_instruction, system_program,
    sysvar::Sysvar,
};

#[cfg(not(feature = "no-entrypoint"))]
solana_program::entrypoint!(process_instruction);

pub const POLICY_LEN: usize = 153;
pub const INTENT_LEN: usize = 233;
const POLICY_MAGIC: &[u8; 8] = b"QPCPOL01";
const INTENT_MAGIC: &[u8; 8] = b"QPCINT01";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Error {
    InvalidData = 1,
    WrongAccount,
    MissingSignature,
    WrongOwner,
    NotWritable,
    AlreadyInitialized,
    InvalidPolicy,
    Revoked,
    InvalidTime,
    InvalidAmount,
    AlreadyExecuted,
    InvalidSystemProgram,
}
impl From<Error> for ProgramError {
    fn from(e: Error) -> Self {
        ProgramError::Custom(e as u32)
    }
}
fn require(ok: bool, error: Error) -> ProgramResult {
    if ok {
        Ok(())
    } else {
        Err(error.into())
    }
}

/// Owner and agent, genesis and policy ID never change after initialization.
/// Version/epoch only advance; every update invalidates previous approvals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub owner: Pubkey,
    pub agent: Pubkey,
    pub genesis: [u8; 32],
    pub policy_id: [u8; 32],
    pub version: u64,
    pub epoch: u64,
    pub active: bool,
}
/// `policy` binds immutable owner/agent/genesis/policy ID; PDA binds program+nonce.
/// Asset is ALWAYS native SOL/lamports (9 decimals), never tokens.
/// Hashes are trusted owner attestations, not parsed/recomputed canonical JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intent {
    pub policy: Pubkey,
    pub authorization_commitment: [u8; 32],
    pub intent_id: [u8; 32],
    pub policy_commitment: [u8; 32],
    pub recipient: Pubkey,
    pub nonce: u64,
    pub amount: u64,
    pub version: u64,
    pub epoch: u64,
    pub issued_at: u64,
    pub not_before: u64,
    pub expires_at: u64,
    /// Attested metadata ONLY. Runtime fees cannot be bounded by this program.
    pub max_fee: u64,
    pub executed: bool,
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N], ProgramError> {
        if self.0.len() < N {
            return Err(Error::InvalidData.into());
        }
        let (a, b) = self.0.split_at(N);
        self.0 = b;
        Ok(a.try_into().unwrap())
    }
    fn u64(&mut self) -> Result<u64, ProgramError> {
        Ok(u64::from_le_bytes(self.bytes()?))
    }
    fn key(&mut self) -> Result<Pubkey, ProgramError> {
        Ok(Pubkey::new_from_array(self.bytes()?))
    }
    fn boolean(&mut self) -> Result<bool, ProgramError> {
        match self.bytes::<1>()?[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::InvalidData.into()),
        }
    }
    fn end(&self) -> ProgramResult {
        require(self.0.is_empty(), Error::InvalidData)
    }
}
impl Policy {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = POLICY_MAGIC.to_vec();
        for v in [
            self.owner.to_bytes(),
            self.agent.to_bytes(),
            self.genesis,
            self.policy_id,
        ] {
            b.extend(v);
        }
        b.extend(self.version.to_le_bytes());
        b.extend(self.epoch.to_le_bytes());
        b.push(self.active as u8);
        b
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, ProgramError> {
        let mut r = Reader(bytes);
        require(&r.bytes::<8>()? == POLICY_MAGIC, Error::InvalidData)?;
        let p = Self {
            owner: r.key()?,
            agent: r.key()?,
            genesis: r.bytes()?,
            policy_id: r.bytes()?,
            version: r.u64()?,
            epoch: r.u64()?,
            active: r.boolean()?,
        };
        r.end()?;
        Ok(p)
    }
}
impl Intent {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = INTENT_MAGIC.to_vec();
        b.extend(self.policy.to_bytes());
        for v in [
            self.authorization_commitment,
            self.intent_id,
            self.policy_commitment,
            self.recipient.to_bytes(),
        ] {
            b.extend(v);
        }
        for v in [
            self.nonce,
            self.amount,
            self.version,
            self.epoch,
            self.issued_at,
            self.not_before,
            self.expires_at,
            self.max_fee,
        ] {
            b.extend(v.to_le_bytes());
        }
        b.push(self.executed as u8);
        b
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, ProgramError> {
        let mut r = Reader(bytes);
        require(&r.bytes::<8>()? == INTENT_MAGIC, Error::InvalidData)?;
        let i = Self {
            policy: r.key()?,
            authorization_commitment: r.bytes()?,
            intent_id: r.bytes()?,
            policy_commitment: r.bytes()?,
            recipient: r.key()?,
            nonce: r.u64()?,
            amount: r.u64()?,
            version: r.u64()?,
            epoch: r.u64()?,
            issued_at: r.u64()?,
            not_before: r.u64()?,
            expires_at: r.u64()?,
            max_fee: r.u64()?,
            executed: r.boolean()?,
        };
        r.end()?;
        Ok(i)
    }
}
/// Exact binary protocol; all integer fields u64 LE, bool strictly 0 or 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instruction {
    Initialize(Policy),
    Configure {
        version: u64,
        epoch: u64,
        active: bool,
    },
    Approve(Intent),
    Execute {
        nonce: u64,
    },
}
impl Instruction {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Initialize(p) => {
                let mut b = vec![0];
                b.extend(p.encode());
                b
            }
            Self::Configure {
                version,
                epoch,
                active,
            } => {
                let mut b = vec![1];
                b.extend(version.to_le_bytes());
                b.extend(epoch.to_le_bytes());
                b.push(*active as u8);
                b
            }
            Self::Approve(i) => {
                let mut b = vec![2];
                b.extend(i.encode());
                b
            }
            Self::Execute { nonce } => {
                let mut b = vec![3];
                b.extend(nonce.to_le_bytes());
                b
            }
        }
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, ProgramError> {
        let mut r = Reader(bytes);
        let value = match r.bytes::<1>()?[0] {
            0 => {
                let p = Policy::decode(r.0)?;
                r.0 = &[];
                Self::Initialize(p)
            }
            1 => Self::Configure {
                version: r.u64()?,
                epoch: r.u64()?,
                active: r.boolean()?,
            },
            2 => {
                let i = Intent::decode(r.0)?;
                r.0 = &[];
                Self::Approve(i)
            }
            3 => Self::Execute { nonce: r.u64()? },
            _ => return Err(Error::InvalidData.into()),
        };
        r.end()?;
        Ok(value)
    }
}
pub fn policy_address(program: &Pubkey, owner: &Pubkey, agent: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"policy", owner.as_ref(), agent.as_ref()], program)
}
pub fn intent_address(program: &Pubkey, policy: &Pubkey, nonce: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"intent", policy.as_ref(), &nonce.to_le_bytes()], program)
}
fn writable(a: &AccountInfo) -> ProgramResult {
    require(a.is_writable, Error::NotWritable)
}
fn signer(a: &AccountInfo) -> ProgramResult {
    require(a.is_signer, Error::MissingSignature)
}
fn system(a: &AccountInfo) -> ProgramResult {
    require(
        *a.key == system_program::id() && a.executable,
        Error::InvalidSystemProgram,
    )
}
fn load_policy(program: &Pubkey, a: &AccountInfo) -> Result<Policy, ProgramError> {
    require(a.owner == program, Error::WrongOwner)?;
    let p = Policy::decode(&a.try_borrow_data()?)?;
    require(
        policy_address(program, &p.owner, &p.agent).0 == *a.key,
        Error::WrongAccount,
    )?;
    Ok(p)
}
fn now() -> Result<u64, ProgramError> {
    u64::try_from(Clock::get()?.unix_timestamp).map_err(|_| Error::InvalidTime.into())
}
fn valid_policy(p: &Policy) -> ProgramResult {
    require(
        p.owner != Pubkey::default()
            && p.agent != Pubkey::default()
            && p.version > 0
            && p.epoch > 0
            && p.genesis != [0; 32]
            && p.policy_id != [0; 32],
        Error::InvalidPolicy,
    )
}
/// Handles prefunded PDA addresses too: top up rent, allocate, assign. Only the
/// derived PDA can sign; arbitrary externally-created program accounts fail.
fn allocate<'a>(
    program: &Pubkey,
    payer: &AccountInfo<'a>,
    target: &AccountInfo<'a>,
    sys: &AccountInfo<'a>,
    size: usize,
    seeds: &[&[u8]],
) -> ProgramResult {
    signer(payer)?;
    writable(payer)?;
    writable(target)?;
    system(sys)?;
    require(
        payer.owner == &system_program::id() && payer.data_is_empty(),
        Error::WrongOwner,
    )?;
    require(
        target.owner == &system_program::id() && target.data_is_empty(),
        Error::AlreadyInitialized,
    )?;
    let needed = Rent::get()?
        .minimum_balance(size)
        .saturating_sub(target.lamports());
    if target.lamports() == 0 {
        invoke_signed(
            &system_instruction::create_account(
                payer.key,
                target.key,
                needed,
                size as u64,
                program,
            ),
            &[payer.clone(), target.clone(), sys.clone()],
            &[seeds],
        )?;
    } else {
        if needed > 0 {
            invoke(
                &system_instruction::transfer(payer.key, target.key, needed),
                &[payer.clone(), target.clone(), sys.clone()],
            )?;
        }
        invoke_signed(
            &system_instruction::allocate(target.key, size as u64),
            &[target.clone(), sys.clone()],
            &[seeds],
        )?;
        invoke_signed(
            &system_instruction::assign(target.key, program),
            &[target.clone(), sys.clone()],
            &[seeds],
        )?;
    }
    // In runtime CPI changed these fields; host tests must emulate that honestly.
    require(
        target.owner == program && target.data_len() == size,
        Error::WrongOwner,
    )
}
fn save(a: &AccountInfo, bytes: &[u8]) -> ProgramResult {
    writable(a)?;
    let mut data = a.try_borrow_mut_data()?;
    require(data.len() == bytes.len(), Error::InvalidData)?;
    data.copy_from_slice(bytes);
    Ok(())
}
pub fn process_instruction(
    program: &Pubkey,
    accounts: &[AccountInfo],
    data: &[u8],
) -> ProgramResult {
    let mut accounts = accounts.iter();
    match Instruction::decode(data)? {
        Instruction::Initialize(p) => {
            let owner = next_account_info(&mut accounts)?;
            let policy = next_account_info(&mut accounts)?;
            let sys = next_account_info(&mut accounts)?;
            require(accounts.next().is_none(), Error::WrongAccount)?;
            signer(owner)?;
            valid_policy(&p)?;
            require(p.active && p.owner == *owner.key, Error::InvalidPolicy)?;
            let (address, bump) = policy_address(program, &p.owner, &p.agent);
            require(address == *policy.key, Error::WrongAccount)?;
            allocate(
                program,
                owner,
                policy,
                sys,
                POLICY_LEN,
                &[b"policy", p.owner.as_ref(), p.agent.as_ref(), &[bump]],
            )?;
            save(policy, &p.encode())?;
            msg!("QPC_POLICY_INITIALIZED");
        }
        Instruction::Configure {
            version,
            epoch,
            active,
        } => {
            let owner = next_account_info(&mut accounts)?;
            let policy = next_account_info(&mut accounts)?;
            require(accounts.next().is_none(), Error::WrongAccount)?;
            signer(owner)?;
            let mut p = load_policy(program, policy)?;
            require(p.owner == *owner.key, Error::WrongAccount)?;
            require(
                version >= p.version && epoch > p.epoch,
                Error::InvalidPolicy,
            )?;
            p.version = version;
            p.epoch = epoch;
            p.active = active;
            save(policy, &p.encode())?;
            msg!("QPC_POLICY_UPDATED");
        }
        Instruction::Approve(i) => {
            let owner = next_account_info(&mut accounts)?;
            let policy = next_account_info(&mut accounts)?;
            let intent = next_account_info(&mut accounts)?;
            let sys = next_account_info(&mut accounts)?;
            require(accounts.next().is_none(), Error::WrongAccount)?;
            signer(owner)?;
            let p = load_policy(program, policy)?;
            require(
                p.owner == *owner.key && i.policy == *policy.key,
                Error::WrongAccount,
            )?;
            require(
                p.active && i.version == p.version && i.epoch == p.epoch,
                Error::Revoked,
            )?;
            require(
                i.authorization_commitment != [0; 32]
                    && i.intent_id != [0; 32]
                    && i.policy_commitment != [0; 32]
                    && !i.executed,
                Error::InvalidData,
            )?;
            require(
                i.amount > 0
                    && i.recipient != Pubkey::default()
                    && i.recipient != p.agent
                    && i.recipient != *program
                    && i.recipient != *policy.key
                    && i.recipient != *intent.key,
                Error::InvalidAmount,
            )?;
            let time = now()?;
            require(
                i.issued_at <= i.not_before
                    && i.issued_at <= time
                    && i.not_before < i.expires_at
                    && time < i.expires_at,
                Error::InvalidTime,
            )?;
            let (address, bump) = intent_address(program, policy.key, i.nonce);
            require(address == *intent.key, Error::WrongAccount)?;
            allocate(
                program,
                owner,
                intent,
                sys,
                INTENT_LEN,
                &[
                    b"intent",
                    policy.key.as_ref(),
                    &i.nonce.to_le_bytes(),
                    &[bump],
                ],
            )?;
            save(intent, &i.encode())?;
            msg!("QPC_INTENT_APPROVED");
        }
        Instruction::Execute { nonce } => {
            let agent = next_account_info(&mut accounts)?;
            let policy = next_account_info(&mut accounts)?;
            let intent = next_account_info(&mut accounts)?;
            let recipient = next_account_info(&mut accounts)?;
            let sys = next_account_info(&mut accounts)?;
            require(accounts.next().is_none(), Error::WrongAccount)?;
            signer(agent)?;
            writable(agent)?;
            writable(intent)?;
            writable(recipient)?;
            system(sys)?;
            let p = load_policy(program, policy)?;
            require(
                *agent.key == p.agent
                    && agent.owner == &system_program::id()
                    && agent.data_is_empty(),
                Error::WrongAccount,
            )?;
            require(intent.owner == program, Error::WrongOwner)?;
            let mut i = Intent::decode(&intent.try_borrow_data()?)?;
            require(
                i.policy == *policy.key
                    && i.nonce == nonce
                    && intent_address(program, policy.key, nonce).0 == *intent.key
                    && i.recipient == *recipient.key,
                Error::WrongAccount,
            )?;
            require(!i.executed, Error::AlreadyExecuted)?;
            require(
                p.active && i.version == p.version && i.epoch == p.epoch,
                Error::Revoked,
            )?;
            let time = now()?;
            require(
                time >= i.not_before && time < i.expires_at,
                Error::InvalidTime,
            )?;
            // Native SystemProgram transfer cannot invoke recipient code. Marking
            // before CPI also makes replay fail if this instruction is reentered.
            i.executed = true;
            save(intent, &i.encode())?;
            invoke(
                &system_instruction::transfer(agent.key, recipient.key, i.amount),
                &[agent.clone(), recipient.clone(), sys.clone()],
            )?;
            solana_program::log::sol_log_data(&[
                b"QPC_EXECUTED_V1",
                &i.authorization_commitment,
                policy.key.as_ref(),
                &nonce.to_le_bytes(),
                &i.amount.to_le_bytes(),
            ]);
            msg!("QPC_EXECUTED nonce={} amount={}", nonce, i.amount);
        }
    }
    Ok(())
}
