# QPC Agent-to-Agent Cross-Chain Payment Network

**Status: architecture specification plus chain-specific prototypes. Cross-chain routing, provider settlement, escrow interoperability, and end-to-end agent runtime are not yet implemented or deployed.**

## Product definition

QPC is intended to let each agent using QPC initiate and receive payments across supported networks, including NEAR, Ethereum, BNB Smart Chain, Solana, and Zcash. A logical agent identity may expose several independently verified chain endpoints. A payment is complete only after the source-side authorization and destination-side receipt have both been independently verified.

QPC is not itself a bridge, exchange, custodian, or universal token. It does not make unrelated chains share balances or signatures. Cross-chain value movement requires a separately selected interoperability provider, intent solver, bridge, or liquidity route with an explicit trust and failure model. Do not claim atomicity, trustlessness, or anonymity unless the exact route proves those properties.

## Core model: one agent, multiple chain payment endpoints

An agent has one logical QPC identity and may register chain-specific endpoints:

- NEAR account ID and QPC NEAR policy/escrow references.
- Ethereum address and Ethereum QPC/escrow references.
- BNB Smart Chain address and BSC QPC/escrow references.
- Solana public key and Solana QPC program/escrow references.
- Zcash Unified Address (UA), with a supported shielded receiver, held privately by the Zcash wallet adapter.

Endpoint ownership must be authenticated by the agent and independently verifiable. A registry entry alone is not proof of ownership. Endpoint rotation, revocation, recovery, and compromised-key handling must be specified. Never reuse one chain's private key as another chain's key.

A Zcash endpoint is not an EVM contract address. Zcash is a UTXO-based Layer 1 with transparent and shielded pools. QPC's Zcash integration is a Rust wallet/transaction adapter that works with Zcash wallet state and transaction semantics; it is not a Solidity contract deployed on Zcash. QPC policy for Zcash actions is enforced by the agent runtime and durable local policy/replay state, not by a QPC smart contract running on Zcash.

## Components

1. **Agent identity and endpoint registry:** signed agent manifest maps one logical identity to independently verified chain endpoints, supported assets, capabilities, and key references.
2. **Agent runtime / payment router:** discovers recipient endpoints, requests quotes, compares routes, constructs typed payment intents, checks policy, and monitors settlement.
3. **Chain-native QPC policy adapter:** NEAR, Ethereum, BSC, and Solana enforce their own on-chain policy checks. Zcash authorization is performed by the agent runtime and Zcash adapter boundary; Zcash itself does not execute QPC smart contracts.
4. **Chain-local payment adapter / escrow:** performs chain-specific source settlement and destination receipt handling. The Zcash adapter uses a maintained Rust wallet stack, SQLite wallet state, compact-block synchronization, and shielded transaction capabilities when those capabilities are implemented and reviewed.
5. **Interoperability / liquidity network:** an established bridge, intent solver, messaging protocol, or liquidity provider transports value or coordinates settlement. QPC must not invent its own bridge.
6. **Settlement monitor and evidence store:** independently verifies source and destination chain evidence, confirmation/finality, payment IDs, reorgs, timeouts, and recovery.

## Typed payment intent

Every cross-chain intent should bind, at minimum:

- protocol version and unique opaque payment ID;
- source and destination chain identifiers;
- source and destination agent identities;
- exact source and destination endpoints (private endpoint material must not be placed in public events);
- source and destination asset identifiers and decimals;
- source amount, minimum destination amount, maximum fees, and slippage ceiling;
- route/provider identifier and quote expiry;
- source-chain nonce or durable adapter nonce, with a clearly defined replay domain;
- refund/recovery endpoint and expiry;
- required confirmation/finality policy and privacy class.

Use typed canonical payloads and domain-separated signatures. Do not rely only on hashing arbitrary JSON. Never put seed phrases, spending keys, viewing keys, Zcash UAs, shielded note plaintext, or unnecessary personal data into public manifests, source-chain events, route messages, logs, or receipts.

## Example A: Agent A on NEAR pays Agent B on Zcash

1. **Private recipient registration:** Agent B supplies a Zcash Unified Address through a private authenticated channel. The address is held only inside the destination wallet adapter; it is not written to a NEAR event or public route message.
2. **Shielded receiver validation:** the adapter decodes the UA for the expected Zcash network and selects an allowed shielded receiver. Prefer Orchard. Sapling may be accepted when explicitly supported by the wallet and policy. If no allowed shielded receiver is present, reject the payment. Never silently fall back to a transparent t-address.
3. **Opaque destination reference:** create a random opaque payment reference or commitment that does not encode the UA, viewing key, agent identity, or note data. Bind it to the payment ID and expiry in private adapter state.
4. **Route quote:** request an executable quote from an allowlisted bridge/solver/liquidity provider. The quote must state source spend, ZEC destination amount, fees, slippage, expiry, route ID, settlement conditions, and exactly which parties can observe or correlate source and destination metadata. The payer's policy must approve the route's privacy class.
5. **NEAR policy authorization:** Agent A submits a NEAR QPC action request to the exact allowlisted adapter/escrow target. QPC checks agent identity, exact target/method/argument hash, nonce, policy version, amount cap, call quota, expiry, and pause/revocation state. The public request contains only the opaque reference, not the raw UA.
6. **Source settlement:** the NEAR-side adapter/escrow executes the approved provider mechanism. Wait for the configured NEAR finality evidence. A promise dispatch or provider HTTP response alone is not settlement proof.
7. **Zcash destination settlement:** the provider pays the validated shielded destination, or coordinates a documented shielded settlement mechanism. The wallet adapter synchronizes compact blocks and scans its wallet state for the intended incoming note. A public txid or block inclusion alone is not proof that the intended wallet received a spendable note.
8. **Confirmation and receipt:** apply the configured Zcash confirmation threshold and handle sync lag, chain reorgs, and wallet rescans. Verify the payment against private wallet state and the opaque payment reference. Publish only a minimal receipt with payment ID, status, and confirmation count; omit UA, memo, viewing keys, note plaintext, and unnecessary tx metadata.
9. **Completion or reconciliation:** mark complete only after the destination wallet evidence and source/provider evidence satisfy policy. If either side is ambiguous, move to PENDING_RECONCILIATION; do not report success. Refund or compensation follows the selected provider's documented mechanism and must not produce both a refund and a destination payment for one payment ID.

## Example B: Agent A on Zcash pays Agent B on NEAR

1. Agent B publishes a signed NEAR endpoint manifest. Agent A verifies endpoint ownership and the accepted NEAR asset and amount bounds.
2. Agent A's runtime creates a typed intent binding the Zcash source wallet, NEAR destination account, asset conversion, quote expiry, unique payment ID, and durable nonce.
3. The local QPC policy checks source wallet authorization, recipient, amount, route allowlist, fee ceiling, expiry, and privacy class before asking the wallet to construct a transaction. The Zcash chain does not enforce this QPC policy or nonce; durable adapter state and wallet authorization enforce the local policy boundary.
4. The router obtains a quote from an allowlisted solver. The quote must disclose whether the solver can observe both sides, what evidence it requires for the shielded source payment, and how it handles failed or underpaid routes.
5. The Zcash wallet adapter constructs and signs a shielded transaction only when the reviewed wallet implementation supports that operation and the user/agent approval policy permits it. No viewing key is handed to the solver by default. If the solver cannot verify the payment without additional disclosure, the route must state that limitation and obtain explicit policy approval; otherwise reject.
6. The solver/provider pays the verified NEAR destination through a real NEAR transaction. Agent B or an independent monitor verifies the NEAR chain, account, asset, amount, and finality.
7. Mark complete only after the Zcash source evidence and NEAR destination transaction are reconciled under the route's trust model. Ambiguous or partial settlement becomes PENDING_RECONCILIATION.

**Implementation boundary:** the current Zcash prototype does not yet construct, sign, or broadcast shielded transactions. Therefore Example B is a target flow, not a working payment feature. It must remain disabled until the wallet send path, policy boundary, provider integration, and tests are complete.

## Same-chain and cross-chain modes

- **Same-chain NEAR/EVM/Solana:** chain-local transfer or escrow; no bridge is involved.
- **Cross-chain into Zcash:** source chain authorizes and settles through a provider; Zcash adapter validates a private UA, requires a shielded receiver, syncs wallet state, and verifies incoming receipt.
- **Cross-chain from Zcash:** local wallet/policy authorizes a shielded source payment; a separately reviewed provider pays the destination chain. Zcash does not run QPC smart contracts.
- **Cross-chain same asset:** use a supported bridge or liquidity route with explicit source/destination evidence.
- **Cross-chain different asset:** use an intent/solver or exchange route with explicit conversion, fees, slippage, and minimum received.
- **No supported route:** reject safely. Never simulate settlement or imply that a route exists when none is configured and tested.

## Network and adapter matrix

| Network | QPC enforcement | Payment layer | Endpoint |
|---|---|---|---|
| NEAR | NEAR-native Rust contract | NEAR native asset / NEP-141 FT; same-chain escrow prototype | NEAR account |
| Ethereum | Solidity contract | ETH / allowlisted ERC-20 | EVM address |
| BNB Smart Chain | Separate BSC deployment/configuration | BNB / allowlisted BEP-20-compatible token | BSC address |
| Solana | Rust/Anchor program | SOL / allowlisted SPL token | Solana public key |
| Zcash | Agent runtime + Rust wallet adapter; no QPC contract on Zcash | ZEC UTXO wallet; shielded Orchard/Sapling receiving and spending as supported | Private Unified Address |

Each network has its own chain/program ID, token registry, authority, nonce domain, event format, confirmation/finality rules, and recovery process. A shared agent identity does not merge chain state or balances.

## Zcash shielded-first privacy requirements

- Accept a canonical Unified Address and select Orchard first where supported; Sapling is an explicit compatibility fallback only when allowed by policy.
- Reject transparent-only addresses and UAs without an allowed shielded receiver. No silent transparent fallback.
- Keep raw UAs and receiver details private to the wallet adapter. Do not publish them in source-chain events, agent manifests, route IDs, or public receipts.
- Keep memos empty by default. Never put agent names, identity documents, emails, source-chain addresses, or business context in a memo.
- Never transmit spending keys, seed phrases, incoming viewing keys, or full viewing keys to bridges, solvers, relayers, analytics, or logs by default.
- Use opaque payment references and minimum-disclosure receipts. Redact sensitive values from debug output, errors, telemetry, and fixtures.
- If a route provider requires disclosure that violates the configured privacy class, reject or request explicit approval; do not silently weaken privacy.

### Privacy is not end-to-end invisibility

Shielded Zcash protects transaction graph details inside its shielded pools, but it does not hide all metadata across the route. The source chain may reveal the payer identity, amount, timing, and provider interaction. A bridge, solver, relayer, exchange, or liquidity provider may correlate source and destination activity, amounts, timing, IP/session metadata, and route identifiers. A wallet provider or lightwalletd operator may observe wallet queries and network metadata. Transparent pool activity remains publicly visible. A shielded destination therefore does not make the complete cross-chain payment anonymous or unlinkable.

| Surface | Possible leakage | Required control |
|---|---|---|
| Source chain | payer identity, source amount, timing, route call | opaque reference; minimal event fields |
| Bridge / solver / relayer | source-destination linkage, amount, timing, IP/session metadata | explicit trust disclosure; minimize metadata; privacy-class allowlist |
| Zcash wallet provider / lightwalletd | wallet sync queries, IP, timing, server-provided chain data | document operator trust; prefer reviewed/self-hosted infrastructure where feasible |
| Transparent pool | sender, receiver, amount, transaction graph | shielded-only default; fail closed |
| Memo | arbitrary identifying text | empty by default |
| Viewing keys | historical transaction disclosure | never share by default; scoped, revocable access only when necessary |
| Logs / analytics | address, txid, note data, keys, user identifiers | redaction, access controls, short retention |

A lightwalletd response is not equivalent to independently validating Zcash consensus. The client must document what it verifies locally and what it trusts the server to provide.

## Nonce, confirmation, synchronization, and reconciliation

### Nonces and replay protection

- NEAR contract nonce and request ID are enforced on NEAR and consumed when dispatch is accepted; failed downstream calls do not automatically restore them.
- EVM and Solana nonce/replay domains are enforced by their own chain adapters.
- Zcash has no QPC smart-contract nonce. The Zcash agent adapter must durably consume a nonce scoped to (source network, source agent/wallet identity, nonce) and enforce a unique payment ID in SQLite using an atomic transaction and uniqueness constraints.
- Persist nonce/payment registration before any externally visible settlement action. Reopening the process must not make a consumed nonce reusable.
- Idempotently accept duplicate delivery of the same state/event; reject conflicting payment IDs, nonce reuse, stale callbacks, and out-of-order transitions.

### Synchronization and confirmations

- Zcash wallet sync must use the configured Zcash network and a trusted/verified lightwalletd endpoint over HTTPS. Keep wallet DB and compact-block cache in separate durable SQLite files.
- A successful sync call means the light-client sync routine completed against that endpoint; it does not mean the endpoint is trustless or that a recipient received funds.
- Track sync tip, wallet scan progress, missing compact blocks, rescan needs, and reorg recovery. Fail closed on incomplete or stale wallet state.
- Require a configurable Zcash confirmation threshold before receipt verification. Treat Pending, Unknown, Reorged, wallet lag, or missing note data as nonterminal.
- NEAR and other source/destination chains must independently satisfy their own finality rules. Never infer destination finality from source finality or a provider callback.

### Payment state machine

CREATED -> POLICY_APPROVED -> ROUTE_QUOTED -> DESTINATION_VALIDATED -> SOURCE_SETTLED -> ZCASH_BROADCAST -> ZCASH_CONFIRMED -> RECEIPT_VERIFIED -> COMPLETED

- PENDING_RECONCILIATION is recoverable and used for reorgs, sync uncertainty, provider disagreement, partial settlement, and ambiguous refund/payment outcomes.
- FAILED and EXPIRED are terminal only when the route's recovery/refund semantics establish that no destination payment can later complete.
- Transitions must be idempotent and monotonic. Never mark complete solely because an HTTP callback, transaction ID, or block inclusion was observed.
- For a Zcash-originating payment, the route-specific source evidence and destination-chain receipt must both be verified before completion; do not reuse the inbound-only state labels as proof that the current prototype can broadcast.

## Security invariants

- Bind every payment to both chains, both agent identities, exact recipient commitment, assets, decimals, amount bounds, fee ceiling, expiry, route, and unique payment ID.
- Enforce per-agent, per-job, per-token, per-destination-chain, and per-provider limits.
- Restrict providers to an allowlist; support revocation, pause, and incident recovery.
- Treat provider/solver/relayer as an explicit trust assumption; never trust a plain HTTP success response.
- Model provider outage, liquidity failure, partial fill, fee changes, token freeze, chain halt, reorg, duplicate message, and refund failure.
- Keep private keys out of contracts, manifests, repository files, CI, and prompts. AI model output is never transaction authorization.
- Do not advertise post-quantum cross-chain authentication as active unless each relevant verification path is implemented and independently reviewed.

## Implementation sequence and demo acceptance

1. **Specification and threat model:** freeze typed intent, quote, receipt, nonce domains, finality requirements, privacy class, and recovery state machine.
2. **Local simulation:** simulate NEAR-to-Zcash and Zcash-to-NEAR with deterministic mock provider; clearly label mock settlements.
3. **NEAR testnet source authorization:** test exact adapter target, argument hash, amount cap, nonce, expiry, duplicate request, and failure behavior without mainnet funds.
4. **Zcash testnet wallet receiving:** use a testnet UA, shielded-only validation, isolated SQLite wallet/cache, testnet synchronization, and confirmation/reorg tests. No production keys or funds.
5. **Provider testnet route:** select a provider only after its supported network pair, trust model, privacy disclosures, and refund semantics are reviewed. Verify both chain transactions independently.
6. **Zcash source-send path:** separately implement and audit wallet transaction construction, signing boundary, broadcast, spend confirmation, and source evidence before enabling Zcash-to-other-chain payments.
7. **Production hardening:** independent review, monitoring, route revocation, recovery drills, and incident response.

A truthful end-to-end demo must show a real supported provider quote, policy authorization, source transaction, destination transaction, shielded wallet receipt evidence, confirmation/finality, privacy leakage disclosure, and failure/replay/reorg handling. A local mock or adapter unit test is not a live cross-chain payment demo.

## Current implementation status

- **NEAR:** policy-constrained asynchronous dispatch and a same-chain NEP-141 escrow prototype exist. The tests exercise contract state and policy boundaries; unit tests do not execute the full NEAR receipt graph or prove a bridge route.
- **Zcash:** the Rust adapter contains Unified Address parsing/shielded receiver policy, a wallet SQLite initialization boundary, compact-block SQLite cache, Testnet light-client sync entry point, payment state tracking, and durable SQLite nonce/replay storage. The current prototype does **not** construct, sign, or broadcast Zcash transactions; it does not import a seed or viewing key; and no live Testnet synchronization or cross-chain provider settlement is claimed by the local tests.
- **Cross-chain:** no production bridge/provider route, universal agent runtime, or trustless atomic settlement is implemented or deployed. Treat cross-chain flows above as specifications until the stated acceptance evidence exists.

No deployment, mainnet transaction, wallet key import, or real payment is authorized by this document.
