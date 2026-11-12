# PQP — Policy-Guarded Payments for Agents

**On-chain spending guardrails for agentic treasuries, implemented as a native NEAR smart contract.**

PQP lets a human-controlled treasury authorize an agent to request bounded NEAR transfers. Each policy has a short expiry, a cumulative cap, a per-transaction limit, an explicit recipient allowlist, and a revocation path. Payments settle asynchronously on NEAR, and spending is counted only after the transfer callback reports success.

> **Security and scope:** This project does not implement post-quantum cryptography, a post-quantum signature scheme, or a quantum-resistant ledger. “PQP” is a project name, not a cryptographic guarantee. The new NEAR contract uses NEAR’s ordinary account authentication; its security comes from explicit on-chain spending policy. The existing Solidity/EVM contract is pre-existing baseline code and is not a NEAR deployment.

## What problem it solves

Giving an autonomous agent a full-access treasury key grants much more authority than most payment tasks require. PQP puts a deterministic policy layer between an agent account and a treasury: the owner defines how much an agent may spend, who may receive funds, and when the authority ends. The agent can request a transfer but cannot change its own policy.

## What is new for NEAR HACKATHON

This repository contains an earlier Solidity/EVM policy contract. That implementation predates the NEAR integration and is included as baseline context, not as a NEAR contract. The `near/` Rust contract and this NEAR integration were added for the NEAR HACKATHON development window on the `hackathon/near-open-innovation` branch. The relevant baseline-to-hackathon diff is available in the [branch comparison](https://github.com/wadezigh96/pQp_contracts/compare/main...hackathon/near-open-innovation).

The NEAR build uses native NEAR account execution and asynchronous `Promise` transfers, not an EVM deployment, bridge, Chain Signatures, NEAR Intents, or a post-quantum key. The project targets **Open Innovation**. Its agent-spending-guardrail design also fits **AI Security & Defense**. No partner integration is claimed.

## Try it locally

Requirements: Rust stable, Cargo, and the `wasm32-unknown-unknown` target.

```bash
rustup target add wasm32-unknown-unknown
cd near
cargo test --locked
cargo build --locked --target wasm32-unknown-unknown --release
```

Expected results:

1. `cargo test --locked` runs unit tests for owner-only policy changes, recipient permissions, policy duration, per-transaction and cumulative spending limits, pending asynchronous spend reservations, pause behavior, and allowlist enforcement. The process exits successfully when all tests pass.
2. The build command exits successfully and writes a WebAssembly module under `near/target/wasm32-unknown-unknown/release/`.
3. A push or pull request to this branch checks Rust formatting, runs the same unit tests, and builds the Wasm contract in GitHub Actions.

These are local unit/build checks, not a deployed testnet demo, independent audit, or proof of formal verification.

## Contract interface

The Rust contract in [`near/src/lib.rs`](near/src/lib.rs) exposes:

- `new(owner_id)` to initialize the treasury policy contract.
- `configure_agent(agent_id, expires_at_ns, native_cap, per_tx_limit)` to create a bounded agent policy. Active, unexpired policies cannot be overwritten; revoke or wait for expiry first.
- `set_recipient(agent_id, recipient_id, allowed)` to add or remove a transfer destination.
- `request_spend(recipient_id, amount)` for an authorized agent to request a native NEAR transfer. The contract reserves pending spend so concurrent requests cannot exceed the cap, then settles accounting in a private promise callback.
- `resolve_spend(request_id)` as an internal callback, `revoke_agent(agent_id)`, `set_paused(paused)`, and `deposit()`.

NEAR promises execute asynchronously. A request ID is returned when a transfer is scheduled; the request is finalized in a callback after success or failure. The scheduling transaction alone does not prove that payment settled.

## Security model and limitations

The owner is trusted to set policies and can pause or revoke agents. An agent can request transfers only while its policy is active, unexpired, unpaused, within its cap and per-transaction limit, and to an allowed recipient. Pending transfers count against the cap before an asynchronous promise is submitted; a failed transfer releases that reservation, while a successful callback records the spend.

The NEAR implementation currently protects native NEAR payments only. It does not implement FT/NEP-141 transfers, cross-chain settlement, agent key provisioning, a frontend, or cryptographic post-quantum protection. Do not use it to custody production funds without deeper callback/state-machine review, adversarial testing, and an independent security audit. The legacy Solidity code has known review concerns; see [`SECURITY.md`](SECURITY.md).

## Repository map

```text
contracts/                 Pre-existing Solidity/EVM baseline
script/                    Foundry deployment script for baseline
test/                       Foundry tests for baseline
near/Cargo.toml             Native NEAR Rust contract package
near/src/lib.rs             NEAR policy and asynchronous payment contract
.github/workflows/near.yml  NEAR test and Wasm build checks
SECURITY.md                 Scope, limitations, and security review notes
ARCHITECTURE.md             Baseline and NEAR integration architecture
```
