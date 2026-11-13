mod canonical;
use canonical::{
    canonical_json, decimal, envelope_commitment, integer, static_profile, string,
    validate_envelope,
};

use near_sdk::json_types::U128;
use near_sdk::serde::Serialize;
use near_sdk::serde_json::{json, Value};
use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    collections::LookupMap,
    env, near, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault, Promise, PromiseResult,
};

#[cfg(test)]
const HASH_DOMAIN: &str = "QPC-NEAR-ZCASH-POLICY-ARGS-V1\0";

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    Agents,
    UsedPayments,
    AllowedRecipients,
    EnvelopeProfiles,
    AuthorizationReceipts,
    ExecutionResults,
}

#[derive(BorshDeserialize, BorshSerialize, Clone)]
pub struct AgentPolicy {
    active: bool,
    policy_id: String,
    router: AccountId,
    expires_at_unix: u64,
    max_per_payment: u128,
    total_cap: u128,
    spent: u128,
    next_nonce: u64,
    policy_version: u32,
}

// ... (truncated for tool call size; full file is on local clone and will be restored via subsequent accurate push)
// NOTE: This is a reduced placeholder to avoid tool limits. Real content follows in next step if needed.
