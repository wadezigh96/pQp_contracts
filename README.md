# QPC — Quantum Persistence Contract

**Persistent on-chain cybersecurity guardrails for AI agents and autonomous treasuries on NEAR.**

QPC is designed to reduce the blast radius when an AI agent, its application, or an automation workflow is compromised or behaves unsafely. Policy is enforced by a NEAR smart contract rather than relying only on rules in the agent's off-chain software. The owner sets short-lived authority, per-transaction and cumulative spending caps, recipient allowlists, and pause/revocation controls. Pending transfers reserve policy capacity until asynchronous settlement.

> **What “Quantum Persistence” means here:** QPC is the project name and describes persistent on-chain policy state. The contract does not implement quantum cryptography, detect hackers, repair software bugs, or make every computer or blockchain quantum-safe. NEAR supports the ML-DSA-65 post-quantum access-key scheme at the account/protocol layer, but this contract does not select, inspect, or enforce which keys its owner or agents use. A deployment must separately configure and verify its signing keys before claiming post-quantum account security.

## Cybersecurity innovation

A compromised agent key should not automatically grant authority to rewrite its own spending policy. QPC places deterministic limits between an agent and a treasury: the agent can request an allowed payment, but cannot change its cap, extend its expiry, add a recipient, unpause the contract, or revoke the owner. This can contain financial impact from compromised AI workflows and some application-level failures, provided the on-chain policy and owner account remain secure.

QPC is a containment and authorization layer, not a general-purpose firewall or vulnerability scanner. It cannot prevent an attacker from using a permitted action within its limits. It also cannot protect the owner account if that account is compromised, fix bugs in unrelated applications, or guarantee that every AI-generated action is safe.

## Quantum-safe account keys on NEAR

NEAR's access-key documentation lists **ML-DSA-65** as a post-quantum signature scheme, alongside classical key types. NEAR accounts can hold keys from multiple schemes. For an account-level post-quantum setup, the deployment operator must ensure that the owner and agent accounts use ML-DSA-65 signing keys for the relevant authority and remove any weaker active key that would remain an alternate way to act. See the official [NEAR Access Keys documentation](https://docs.near.org/protocol/accounts-contracts/access-keys.md).

That key configuration is not performed by this repository. The contract identifies callers by NEAR account ID and does not verify the signature scheme used for a transaction. NEAR's protocol and ecosystem quantum migration is also broader than account signatures; the [NEAR post-quantum roadmap](https://www.near.org/blog/near-quantum-safe-roadmap) describes additional protocol, consensus, wallet, and cross-chain work. Therefore, do not describe this contract or the entire NEAR stack as quantum-safe merely because ML-DSA-65 is supported.

## What is new for NEAR HACKATHON

The repository's earlier Solidity/EVM policy contract is baseline context, not a NEAR implementation. The Rust contract under `near/` is the new NEAR integration on the `hackathon/near-open-innovation` branch. Compare it with the baseline in the [branch diff](https://github.com/wadezigh96/pQp_contracts/compare/main...hackathon/near-open-innovation).

QPC targets **Open Innovation** and focuses on **AI Security & Defense** through on-chain containment of agent spending authority. It uses native NEAR account execution and asynchronous `Promise` transfers. It does not claim a partner integration, NEAR Intents integration, cross-chain settlement, or post-quantum signing implemented inside the contract.

## Try it locally

Requirements: Rust stable, Cargo, and the `wasm32-unknown-unknown` target.

```bash
rustup target add wasm32-unknown-unknown
cd near
cargo fmt --check
cargo test --locked
cargo build --locked --target wasm32-unknown-unknown --release
```

The tests cover owner-only policy changes, recipient permissions, policy duration, per-transaction and cumulative spending limits, pending asynchronous spend reservations, pause behavior, and allowlist enforcement. A push or pull request to this branch runs formatting, unit tests, and the Wasm release build in GitHub Actions. These checks are not a deployed testnet demo, a proof of formal verification, or an independent security audit.

## Contract interface

The Rust contract in [`near/src/lib.rs`](near/src/lib.rs) exposes:

- `new(owner_id)` to initialize the treasury policy contract.
- `configure_agent(agent_id, expires_at_ns, native_cap, per_tx_limit)` to create a bounded agent policy. An active unexpired policy cannot be overwritten; revoke it or wait for expiry first.
- `set_recipient(agent_id, recipient_id, allowed)` to add or remove a transfer destination.
- `request_spend(recipient_id, amount)` for an authorized agent to request a native NEAR transfer. The contract reserves pending spend before scheduling the transfer, then settles accounting in a private promise callback.
- `resolve_spend(request_id)` as an internal callback, plus `revoke_agent(agent_id)`, `set_paused(paused)`, and `deposit()`.

NEAR promises execute asynchronously. A request ID is returned when a transfer is scheduled; the callback finalizes the request after success or failure. The scheduling transaction alone does not prove that payment settled.

## Scope and known limitations

The new NEAR contract protects native NEAR payments only. It does not implement FT/NEP-141 transfers, account key provisioning, an AI planner, a frontend, cross-chain settlement, malware detection, or cryptographic verification of ML-DSA keys. Do not use it with production funds without deeper callback/state-machine testing and an independent security review.

The legacy Solidity code has separate EVM risks and review notes; see [`SECURITY.md`](SECURITY.md). “QPC” names the persistent on-chain policy contract. A deployment may separately use NEAR ML-DSA access keys, but this contract does not configure or enforce them and does not itself defeat hackers, software bugs, or quantum computers.

## Repository map

```text
contracts/                 Pre-existing Solidity/EVM baseline
script/                     Foundry deployment script for baseline
test/                       Foundry tests for baseline
near/Cargo.toml             Native NEAR Rust contract package
near/src/lib.rs             QPC policy and asynchronous payment contract
.github/workflows/near.yml  Formatting, test, and Wasm build checks
SECURITY.md                 Scope, limitations, and security review notes
ARCHITECTURE.md             Baseline and NEAR integration architecture
```
