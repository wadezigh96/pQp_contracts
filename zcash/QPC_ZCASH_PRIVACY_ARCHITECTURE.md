# QPC Zcash Privacy Adapter — Architecture and Threat Model

**Status: design specification only.** This document does not claim that a Zcash wallet, shielded transaction builder, light-client verifier, bridge, or production payment flow has been implemented. No private keys, viewing keys, seed phrases, or real funds belong in this repository.

## 1. Goal

Provide a Zcash-native payment adapter for QPC agent payments, with privacy as a first-class requirement. An agent on NEAR, Ethereum, BNB Smart Chain, or Solana must be able to request a payment to an agent controlling a Zcash Unified Address (UA). The Zcash adapter is responsible for Zcash wallet and transaction semantics; QPC policy remains responsible for agent authorization, spending limits, expiry, replay protection, and recipient policy.

Zcash is not an EVM execution environment. This integration is a wallet/transaction adapter, not a Solidity contract deployed on Zcash.

## 2. Privacy objectives

1. Prefer shielded receiving and spending. For new integrations, accept a Zcash Unified Address and select an available shielded receiver (Orchard first where supported by the selected wallet stack; Sapling only where compatibility requires it).
2. Do not silently fall back to a transparent t-address. If no compatible shielded receiver is available, fail closed and require an explicit policy decision.
3. Keep seed phrases, spending keys, incoming viewing keys, and full viewing keys out of logs, telemetry, API responses, Git history, and cross-chain messages.
4. Keep the receiver's Zcash address and shielded note details off public source-chain events wherever possible. Use an opaque payment reference and a privacy-preserving claim/reconciliation flow.
5. Minimize metadata: do not put agent names, email, identity documents, source-chain addresses, or business descriptions in Zcash memos.
6. Separate payment authorization from payment disclosure. Viewing-key access must be scoped, revocable where supported, audited, and never granted to a bridge/relayer by default.
7. Make the trust model explicit. A lightwalletd connection is not equivalent to independently validating Zcash consensus.

## 3. Proposed module boundaries

    zcash/
      README.md
      QPC_ZCASH_PRIVACY_ARCHITECTURE.md
      qpc-zcash-agent/
        Cargo.toml
        src/
          lib.rs
          address.rs          # UA parsing and receiver capability checks
          wallet.rs           # wallet backend boundary; no key export by default
          transaction.rs      # shielded transaction construction boundary
          confirmation.rs     # confirmation/finality policy
          privacy_policy.rs   # transparent fallback and metadata controls
          payment_router.rs   # QPC payment request/receipt boundary
        tests/
          address_policy.rs
          privacy_policy.rs
          payment_state.rs

This is a proposed layout, not a statement that these source files already exist. Choose and pin a maintained Zcash Rust wallet/library stack only after compatibility, maintenance, licensing, and security review. Do not invent SDK APIs or implement cryptography locally.

## 4. Cross-chain payment flow

### A. Agent A requests payment

1. Agent A submits a QPC payment intent with destination chain zcash, amount, asset, expiry, and a destination commitment or recipient handle.
2. QPC checks Agent A's policy: active status, allowed destination chain, asset allowlist, per-payment and cumulative limits, nonce, expiry, and pause state.
3. A route/solver quotes the conversion and fees. The quote must identify the trust model and all parties that can observe source and destination metadata.
4. Before authorization, the user/agent policy must explicitly approve the route's privacy class and any unavoidable disclosure.

### B. Destination preparation

1. Agent B supplies a Zcash Unified Address through a private channel.
2. The wallet backend parses the UA and determines which receivers it supports.
3. The privacy policy requires a shielded receiver. Transparent-only fallback is rejected unless a separate explicit policy permits it.
4. The adapter creates a short-lived opaque payment reference. It must not encode the address, viewing key, agent identity, or amount.

### C. Settlement and receipt

1. The cross-chain route settles into the Zcash wallet using the selected shielded pool.
2. The wallet backend tracks the transaction and shielded note using its wallet state; public transaction IDs alone are not proof that the intended recipient received a spendable note.
3. Confirmation policy waits for a configured number of confirmations and handles reorgs / wallet synchronization lag.
4. A receipt reports only the minimum necessary status and opaque reference. It must not disclose note plaintext, viewing keys, memo contents, or recipient address.
5. QPC marks the intent complete only after the adapter's documented receipt criteria are met. A route provider's HTTP success response is not settlement proof.

## 5. Privacy boundaries and unavoidable leakage

Shielded Zcash protects transaction graph details within its shielded pools, but it does not make the entire cross-chain route invisible.

| Surface | Potential leakage | Required control |
|---|---|---|
| Source chain | Agent identity, source wallet, amount, timing, destination intent | Avoid publishing destination UA; use opaque references; minimize event fields |
| Bridge / solver / relayer | Source and destination linkage, amount, IP/session metadata | Explicit trust disclosure; minimize data; no default access to viewing keys |
| Zcash wallet provider | Wallet addresses, wallet sync queries, IP, transaction timing | Prefer self-hosted / privacy-reviewed backend; document network metadata |
| Transparent pool | Sender, receiver, amount and transaction graph visibility | Shielded-only default; transparent fallback disabled |
| Memo field | Arbitrary text can identify agents or link payments | Empty by default; strict size/content policy if enabled |
| Viewing keys | Historical transaction disclosure | Never transmit by default; scoped access and secure storage |
| Timing and amounts | Correlation with source-chain payment | Quote/settlement design must assess amount and timing correlation; do not promise unlinkability |
| Logs and analytics | Addresses, txids, note data, keys, user identifiers | Redaction, short retention, access control, no secrets in telemetry |

**Important:** A shielded destination does not by itself hide the source-chain transfer, route provider, amount conversion, timing, or network metadata. If a route provider sees both sides, it may be able to correlate the payment.

## 6. Trust and verification model

The first implementation must label which model it uses:

- **Wallet-provider trust:** a hosted wallet or service reports transaction status. Convenient, but the provider is trusted for wallet state and may observe metadata.
- **lightwalletd-assisted light client:** lightwalletd serves compact blockchain data to a light client and obtains data from a full node. The client must document which consensus and witness checks it performs locally and which data it trusts the server to provide.
- **Independent validation:** a separately reviewed client validates the relevant consensus data and shielded wallet state. Do not claim this until implemented and tested.

Do not call a lightwalletd response a trustless Zcash proof. Do not treat a transaction ID, block inclusion, or explorer result as proof that a shielded note is spendable by the intended wallet.

## 7. Required state machine

    CREATED
      -> POLICY_APPROVED
      -> ROUTE_QUOTED
      -> DESTINATION_VALIDATED
      -> SOURCE_SETTLED
      -> ZCASH_BROADCAST
      -> ZCASH_CONFIRMED
      -> RECEIPT_VERIFIED
      -> COMPLETED

    Any nonterminal state may enter FAILED or EXPIRED under explicit transition rules.
    Reorgs or wallet-sync uncertainty must enter a recoverable PENDING_RECONCILIATION state.

Transitions must be idempotent. Use a unique payment intent ID and replay protection. Never mark a payment completed solely because a relayer callback succeeded. Define refund/compensation semantics per route; do not imply cross-chain atomicity unless the selected protocol actually guarantees it.

## 8. Security and privacy acceptance tests

Before calling the adapter implemented:

- Reject malformed UAs and UAs without an allowed shielded receiver.
- Reject transparent-only destination by default.
- Verify no secret material appears in logs, errors, traces, fixtures, or serialized receipts.
- Verify memo is empty by default and rejects prohibited identifying fields.
- Verify source-chain events contain no raw Zcash UA, viewing key, or agent personal data.
- Verify route quotes disclose observer/trust assumptions and privacy class.
- Verify payment IDs are unique, expiry is enforced, and replayed callbacks are rejected.
- Verify confirmation handling, reorg recovery, wallet rescan/sync delay, and duplicate notifications.
- Verify receipts do not disclose shielded note plaintext or wallet secrets.
- Test that missing light-client data or an unavailable wallet backend fails closed.
- Add property/fuzz tests for UA parsing and state transitions.
- Conduct an independent review of the selected wallet stack and transaction construction before handling production funds.

## 9. Non-goals and claims policy

- No claim that QPC makes cross-chain payments anonymous or untraceable.
- No claim that shielded Zcash hides metadata visible to source chains, solvers, exchanges, or network providers.
- No custom cryptographic implementation.
- No automatic transparent fallback.
- No claim that ZIP-300 or any proposed cross-chain atomic transaction feature is deployed.
- No production-funds use until the wallet implementation, verification model, route design, threat model, and independent audit are complete.

## 10. Primary references

- Zcash Protocol Specification: https://zips.z.cash/protocol/protocol.pdf
- ZIP 316 — Unified Addresses: https://zips.z.cash/zip-0316
- ZIP 224 — Orchard Shielded Protocol: https://zips.z.cash/zip-0224
- Zcash lightwalletd: https://github.com/zcash/lightwalletd
- Zcash light-client support and trust assumptions: https://zcash.readthedocs.io/en/master/rtd_pages/lightclient_support.html
- ZIP index (proposal status must be checked before relying on any cross-chain proposal): https://zips.z.cash/
