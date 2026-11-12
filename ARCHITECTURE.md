# QPC Architecture

## Project boundary

QPC separates a pre-existing EVM policy prototype from a new native NEAR cybersecurity contract. Only the code under `near/` is the NEAR implementation. The Solidity code under `contracts/` is historical baseline context and is not called by QPC.

## NEAR execution model

```text
Owner
  ├── configure bounded, expiring policy
  ├── allow recipient
  ├── pause / revoke
  └── fund treasury
         │
Agent account calls request_spend
         ▼
Validate policy, expiry, pause, recipient and limits
         │
Reserve pending spend and store request ID
         ▼
Asynchronous NEAR Promise transfer
         │
Private callback resolves transfer result
         ├── success: release reservation, add to spent
         └── failure: release reservation, leave spent unchanged
```

The pending-spend reservation is included in cap checks so that several outstanding transfers cannot collectively exceed the configured cap. This persistent on-chain policy can limit the financial impact of a compromised agent, but it does not detect attacks or repair software bugs. Transfers are native NEAR only. The implementation does not contain an AI planner, frontend, NEP-141 integration, bridge, or post-quantum signature code.

NEAR supports ML-DSA-65 account access keys, but QPC does not provision, inspect, or enforce a key scheme. The owner and agent accounts must be configured and checked separately if a deployment wants post-quantum account authentication. See [NEAR Access Keys](https://docs.near.org/protocol/accounts-contracts/access-keys.md) and the [NEAR post-quantum roadmap](https://www.near.org/blog/near-quantum-safe-roadmap).

## NEAR contract state

- `owner_id`: account authorized to manage policy.
- `paused`: global transfer-request stop.
- `agents`: active flag, expiry, cumulative native cap, per-transaction limit, settled spending, and pending spending.
- `recipients`: per-agent destination allowlist.
- `requests`: pending asynchronous transfer records, keyed by monotonic request ID.

## Baseline Solidity contract

`contracts/POAAgentFinance.sol` is an earlier EVM prototype with generic target/selector execution and native/ERC-20 policy paths. It remains independent from QPC. See `SECURITY.md` for static-review findings that must be resolved before relying on the EVM baseline.

## Validation

The NEAR package is checked with Rust unit tests and a `wasm32-unknown-unknown` release build. These checks do not replace deployment tests, callback-failure simulation, invariant testing, or an independent audit.
