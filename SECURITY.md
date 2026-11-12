# Security Model and Review Notes

## Scope

QPC contains two implementations with different targets:

- `near/` is the new NEAR Rust contract for bounded native-NEAR payments by agent accounts.
- `contracts/POAAgentFinance.sol` is the earlier Solidity/EVM baseline. It is not deployed on NEAR and its behavior must not be conflated with the NEAR contract.

Neither implementation has an independent security audit. Do not use production funds until the relevant contract has undergone independent review and adversarial testing.

## NEAR contract trust boundary

The owner is the root authority. The owner configures agent policies and recipient permissions and can pause or revoke access. The contract currently limits native NEAR transfers only. These controls can reduce the blast radius of an agent key or application compromise, but they do not detect hackers, repair general software vulnerabilities, or guarantee safe AI behavior.

NEAR documents ML-DSA-65 as a post-quantum access-key scheme ([Access Keys](https://docs.near.org/protocol/accounts-contracts/access-keys.md)). QPC does not provision, inspect, or enforce the signing scheme of owner or agent accounts. A deployment must separately configure and verify the keys that hold authority. The broader NEAR post-quantum migration also includes protocol and ecosystem work ([roadmap](https://www.near.org/blog/near-quantum-safe-roadmap)); supporting ML-DSA keys does not make every account or the entire NEAR stack quantum-safe by default.

The NEAR spend callback is asynchronous. A request reserves cap before scheduling the transfer. The callback records spend only on successful transfer and releases the reservation on failure. Users and integrations must distinguish a scheduled request from a settled transfer. Callback, storage, refund, and race behavior require further adversarial testing before production use.

## Known review findings in the legacy Solidity baseline

1. **Standard ERC-20 selector bypass mitigation added and regression-tested:** `execute` rejects the standard `transfer`, `approve`, `transferFrom`, `increaseAllowance`, and `decreaseAllowance` selectors; Foundry tests exercise generic `transfer`. This is not a general sandbox: owner-approved application-specific calls can have other token or asset side effects, so review every target and selector before granting permission.
2. **Token address validation added and tested:** `transferToken` rejects addresses without bytecode, closing the EOA false-success path found in static review. The implementation does not claim complete ERC-20 compatibility or protect against malicious token contracts; use only reviewed tokens and run adversarial tests.
3. **Revoked agent ACLs persist:** call and recipient permissions remain associated with an address after revocation. Re-activating the same address can inherit old permissions unless the owner explicitly removes them.
4. **Expiry reconfiguration behavior updated and tested:** `configureAgent` allows an expired active record to be replaced, and the Foundry regression exercises the exact expiry timestamp.

`forge test -vv` ran the Solidity test suites and invariant campaigns successfully during this review. Foundry lint output still flags areas for human review, including arbitrary calls to configured targets, external-call/event ordering, and timestamp comparisons. Passing tests do not constitute an independent audit.

## Post-quantum claims

No source in this repository implements or enforces ML-DSA signing. “Quantum Persistence Contract” is the project name; “persistence” refers to policy state stored on-chain, not a cryptographic property. QPC can be deployed alongside quantum-resistant NEAR access keys, but the current contract is key-scheme agnostic. Do not describe the contract itself as quantum-resistant, or claim it prevents all hacker, AI, software-bug, or quantum-computer attacks.

## Before deployment

- Run unit, integration, adversarial, and state-machine tests against the exact target network and toolchain.
- Review all asynchronous callbacks, storage costs, gas limits, account-balance checks, and failure/refund paths.
- Review the owner key and agent key operational model.
- Verify deployed Wasm bytecode, contract account, initialization arguments, and policy state.
- Obtain an independent security audit.
