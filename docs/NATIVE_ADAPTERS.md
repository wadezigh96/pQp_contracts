# Native adapter safety boundary

The separate [Phase 3 testnet inspection layer](TESTNET_INSPECTION.md) now performs read-only native RPC observations. It does not change the local-only contract of this module or enable QPC settlement.

## What exists

`agent-router/src/native-adapters.mjs` implements **local validation and unsigned preparation**, not an executing bridge. It imports the strict canonical authorization envelope, including both native chain IDs. `prepareNativeTransfer` returns `PREPARED_UNSIGNED`; this is **not** `AUTHORIZED`, `SUBMITTED`, `CONFIRMED`, or `SETTLED`. The caller must separately authenticate the agent, check the current policy and revocation epoch, and atomically consume replay authorization. The module does not authenticate a signature, contact an RPC, own a wallet, sign, broadcast, deploy, or transfer funds.

`settleNativeTransfer` always throws: real native verification and durable settlement integration are not implemented. An injected callback cannot unlock it. `validateMockNativeEvidence` is deliberately named and returns only `MOCK_VALIDATED_NOT_SETTLED`. It compares **synthetic normalized fixtures**, not raw chain responses. A fixture claiming `canonical: true` or `FINALIZED` is not proof of canonicality or finality. It cannot be passed off as a production verifier.

## Shared preparation contract

Only `action.type = asset-transfer` and `action.operation = pay` are supported. Arbitrary contract calls, token contracts, arbitrary instructions, wrapped assets, transparent Zcash routes, and extra request fields fail closed. Both source and destination must identify supported native networks; the source native asset and decimals are checked too.

A separate, operator-owned `trustedProfile` is required. Do not derive it from the untrusted request or authorization. Its exact fields are `sourceNetworkId`, `sourceNativeChainId`, `destinationNetworkId`, `destinationNativeChainId`, `mappingId`, `sourceAssetId`, `destinationAssetId`, `sourceDecimals`, `destinationDecimals`, `rateNumerator`, and `rateDenominator`; Solana also requires `expectedGenesisHash`. Networks, native IDs, mapping ID, assets and decimals must exactly match the envelope. The positive decimal rate is an **atomic-unit** ratio: `sourceAtomic * rateNumerator == destinationAtomic * rateDenominator`. There is no rounding, implicit decimal conversion or unapproved exchange-rate fallback. This configured ratio is not a price oracle or proof of liquidity.

The request must exactly match authorized destination amount, recipient commitment, and maximum fee. Fees must use the destination native asset and zero slippage. Mock evidence also supplies a canonical decimal `feeAtomic` no greater than the budget; no actual chain fee has been observed.

Recipient commitment is SHA-256 over UTF-8 `QPC-NATIVE-RECIPIENT-V1\0` followed by recursively key-sorted, whitespace-free JSON of `{ chain, nativeChainId, recipient }`. It binds the exact spelling and chain; it is not an address proof or privacy encryption. EVM addresses must already be lowercase. This is an adapter-specific recipient format, not interchangeable with the existing Zcash route commitment.

The returned request and envelope are deep-copied and frozen. A process-local prepared handle is required by mock validation so copied/forged handles fail. Its request digest uses the separate `QPC-NATIVE-PREPARATION-V1\0` domain over canonical `{ chain, request }`; this digest is not a signature. Prepared handles are ephemeral, not a durable replay store.

## Chain-specific implementation and gaps

### NEAR

- `nativeChainId` is `mainnet` or `testnet`; `networkId` must be exactly `near:<nativeChainId>`.
- Native asset identifier `near:native`, decimals 24, adapter/evidence class `near-native-v1`, local policy label `near-final`.
- Named/implicit account spelling is validated. Only one exact `Transfer` action with the authorized deposit is accepted. No `FunctionCall` is accepted.
- Mock observations require at least one distinct receipt outcome, every outcome successful and not pending. This catches synthetic partial success, but **does not reconstruct or prove the complete receipt graph**.
- Production needs authenticated transaction/action decoding, complete receipt/outcome linkage and final canonical inclusion. NEAR cross-contract calls are asynchronous and independent; success of the initiating function does not imply success of its promises, and failed external calls do not roll back the caller automatically. [Official cross-contract semantics](https://docs.near.org/smart-contracts/anatomy/crosscontract).

### Ethereum

- Separate Ethereum allowlist: chain ID strings `1` and `11155111`; corresponding `eip155:<id>` network. BSC IDs are not accepted.
- Native `ethereum:native`, decimals 18, adapter/evidence `ethereum-native-v1`, policy label `ethereum-finalized`.
- Exact lowercase 20-byte recipient, authorized value, and empty calldata `0x` only. Empty calldata is not proof that the recipient is an externally owned account; contracts may execute receive/fallback logic. The module neither checks code nor simulates execution.
- Production needs chain-ID verification, decoded transaction `to/value/input`, successful receipt and matching canonical inclusion, finality anchored to a trusted chain view, and actual gas fees. The official API distinguishes byte data from compact hex quantities and `safe` from `finalized`; a receipt by itself is not the required finality proof. [Ethereum JSON-RPC API](https://ethereum.org/en/developers/docs/apis/json-rpc/).

### BNB Smart Chain (BSC)

- An **independent** adapter: chain IDs `56` and `97`, native `bsc:native`, decimals 18, adapter/evidence `bsc-native-v1`, policy label `bsc-finalized`.
- The shared EVM-shaped address/value/calldata checks do not imply Ethereum consensus or finality. Ethereum's IDs and policy labels are rejected.
- Production requires BSC-specific canonical/finalized chain verification and exact execution/fee checks. BSC documents `safe` as the latest **justified** block and `finalized` separately; do not turn a safe observation or a confirmation count into finalized evidence.
- Official references: [BSC mainnet/testnet chain IDs](https://docs.bnbchain.org/bnb-smart-chain/developers/json_rpc/json-rpc-endpoint/) and [BSC API/finality tags](https://docs.bnbchain.org/bnb-smart-chain/developers/json_rpc/bsc-api-list/).

### Solana

- `nativeChainId` is the full 32-byte base58 genesis hash; labels are restricted to `solana:mainnet-beta`, `solana:testnet`, or `solana:devnet`. The independently configured `trustedProfile.expectedGenesisHash`, envelope native ID, and request genesis hash must agree. There is no invented hardcoded cluster/genesis mapping; the operator must provision and verify the intended profile.
- Native `solana:native`, decimals 9, adapter/evidence `solana-native-v1`, policy label `solana-finalized`.
- Exactly one decoded System Program (`11111111111111111111111111111111`) transfer with exact u64 lamports. Two exact ordered account metas: writable signing fee payer, then writable non-signing recipient. The only signer must be that fee payer; self-transfer and extra instructions/accounts/signers fail.
- Recent blockhash and keys must decode to 32 bytes. Current `blockHeight` must be below `lastValidBlockHeight`; **this is block height, not slot**. Local inputs are not proof of freshness. No transaction serialization or cryptographic signer authorization is implemented; the fee payer is preparation context, not an envelope-authenticated destination wallet.
- Production needs trusted `getGenesisHash`, fresh `getLatestBlockhash` and current block height, exact decoded message/instruction/account matching, signatures/fee-payer authority, successful `meta.err`, actual fee, and trusted finalized inclusion. [Official getGenesisHash](https://solana.com/docs/rpc/http/getgenesishash) and [getLatestBlockhash](https://solana.com/docs/rpc/http/getlatestblockhash). These docs identify the RPC contracts; mock tests do not exercise either endpoint.

### Zcash — blocked

All JavaScript Zcash preparations, including apparently shielded `u...`/`utest...` recipients, are rejected. Zcash source preparations also reject. Prefix/regex acceptance is not Unified Address validation. There is no transparent fallback, no fake decoder callback, no mock Zcash success, and no settlement claim. `computeNativeRecipientCommitment` may hash an opaque Zcash string; that utility alone does not validate it and cannot produce a prepared handle.

The repository's Rust shielded route parser is the appropriate integration boundary; this new JS adapter does **not** invoke it. Before enabling a route, integrate the actual parser, validate its network and shielded receiver selection, and implement genuine shielded destination evidence. Do not log a UA or viewing key merely to prove it was parsed.

[ZIP 316](https://zips.z.cash/zip-0316) currently marks **Revision 0 Active**, Revision 1 Withdrawn, **Revision 2 Draft**. Revision 2 introduces `zu`/`tu` variants and expiration/MUST-understand metadata; do not claim those draft semantics are deployed consensus or active Revision 0 semantics.

## Mock evidence and adversarial tests

Run `node --test agent-router/test/native-adapters.test.mjs`. The test-local callback harness supplies synthetic observations and rejects timeouts and malformed RPC-shaped responses. No callback is installed in production code.

Coverage includes independent NEAR/Ethereum/BSC/Solana success fixtures; chain/recipient/amount/fee substitution; source/destination identity and decimals; unapproved mapping/rate changes; action/calldata changes; policy/finality mismatch; expired authorization; reorg flag; failed/partial execution; insufficient confirmations; forged commitments/handles; malformed observations; duplicate transaction IDs in a caller-provided mock set; NEAR pending/failed/duplicate receipts; Solana genesis/program/instruction/account/signer/blockhash/expiry-height mutation; Zcash plausible-UA and transparent-address rejection.

The optional mock `seenTransactionIds` set is caller-owned and checked, not automatically persisted or consumed. These tests are not network integration, chain consensus verification, authentication, a durable duplicate guard, fee estimation, proof of shielded payment, or a production settlement demo. No private keys, RPC secrets, real signatures, broadcasts, transfers, deployments, or merges are needed or performed.
