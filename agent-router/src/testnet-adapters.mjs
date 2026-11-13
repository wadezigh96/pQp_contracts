import { prepareNativeTransfer } from './native-adapters.mjs';
import { computeCrossL1AuthorizationCommitment } from './cross-l1-envelope.mjs';

// This module never signs, broadcasts, consumes a nonce, or marks an intent settled.
// RPC observations trust the configured node, not caller-supplied evidence. Even a
// finalized plain transfer cannot prove the envelope's authorization/policy/replay.
export const TESTNET_NETWORKS = Object.freeze({
  near: Object.freeze({ networkId: 'near:testnet', nativeChainId: 'testnet', expectedGenesisHash: 'FWJ9kR6KFWoyMoNjpLXXGHeuiy7tEY6GmoFeCA5yuc6b', rpcUrl: 'https://rpc.testnet.near.org' }),
  ethereum: Object.freeze({ networkId: 'eip155:11155111', nativeChainId: '11155111', expectedGenesisHash: '0x25a5cc106eea7138acab33231d7160d69cb777ee0c2c553fcddf5138993e6dd9', rpcUrl: 'https://ethereum-sepolia-rpc.publicnode.com' }),
  bsc: Object.freeze({ networkId: 'eip155:97', nativeChainId: '97', expectedGenesisHash: '0x6d3c66c5357ec91d5c43af47e234a939b22557cbb552dc45bebbceeed90fbe34', rpcUrl: 'https://bsc-testnet-rpc.publicnode.com' }),
  solana: Object.freeze({ networkId: 'solana:devnet', nativeChainId: 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG', rpcUrl: 'https://api.devnet.solana.com' }),
  zcash: Object.freeze({ networkId: 'zcash:testnet', nativeChainId: 'testnet', rpcUrl: 'http://127.0.0.1:18232' }),
});
const adapters = new WeakMap();
const preparations = new WeakMap();
const HASH = /^0x[0-9a-f]{64}$/;
const B58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
const SYSTEM = '11111111111111111111111111111111';
function check(ok, reason) { if (!ok) throw new Error(reason); }
function freeze(value) {
  if (value && typeof value === 'object') { Object.values(value).forEach(freeze); Object.freeze(value); }
  return value;
}
function data(value, depth = 0) {
  check(depth <= 16, 'input nesting limit');
  if (value === null || typeof value !== 'object') {
    check(value === null || ['string', 'number', 'boolean'].includes(typeof value), 'plain data required');
    return value;
  }
  const array = Array.isArray(value), descriptors = Object.getOwnPropertyDescriptors(value);
  check(Object.getPrototypeOf(value) === (array ? Array.prototype : Object.prototype), 'plain data required');
  if (array) delete descriptors.length;
  check(Reflect.ownKeys(value).length === Object.keys(descriptors).length + (array ? 1 : 0) && Object.values(descriptors).every(d => 'value' in d && d.enumerable), 'plain data fields required');
  if (array) {
    check(Object.keys(descriptors).every((key, i) => key === String(i)) && Object.keys(descriptors).length === value.length, 'dense arrays required');
    return Object.values(descriptors).map(d => data(d.value, depth + 1));
  }
  return Object.fromEntries(Object.entries(descriptors).map(([key, d]) => [key, data(d.value, depth + 1)]));
}
function exact(value, allowed, required = allowed) {
  check(value && typeof value === 'object' && !Array.isArray(value), 'object required');
  check(Reflect.ownKeys(value).every(k => allowed.includes(k)) && required.every(k => Object.hasOwn(value, k)), 'missing or unexpected fields');
  check(Object.values(Object.getOwnPropertyDescriptors(value)).every(d => 'value' in d && d.enumerable), 'data fields required');
}
function uint(v) { check(Number.isSafeInteger(v) && v >= 0, 'invalid RPC integer'); return v; }
function hex(v) { check(typeof v === 'string' && /^0x(?:0|[1-9a-f][0-9a-f]*)$/.test(v), 'invalid RPC quantity'); return BigInt(v); }
function hash(v) { check(typeof v === 'string' && HASH.test(v), 'invalid transaction/block hash'); return v; }
function base58(v, size) {
  check(typeof v === 'string' && v.length <= 100 && v.length > 0, 'invalid base58 identifier');
  let n = 0n; for (const c of v) { const d = B58.indexOf(c); check(d >= 0, 'invalid base58 identifier'); n = n * 58n + BigInt(d); }
  let bytes = 0; for (; n; n >>= 8n) bytes++;
  check(bytes + v.match(/^1*/)[0].length === size, 'invalid base58 identifier length');
  return v;
}
function config(adapter) { check(adapters.has(adapter), 'operator-created testnet adapter required'); return adapters.get(adapter); }

/** Call once in operator code, not with configuration taken from a payment request. */
export function createTestnetAdapter(input) {
  const options = data(input);
  exact(options, ['chain', 'trustedProfile', 'rpcUrl'], ['chain', 'trustedProfile']);
  check(Object.hasOwn(TESTNET_NETWORKS, options.chain), 'unsupported testnet');
  const network = TESTNET_NETWORKS[options.chain];
  // No arbitrary URL, redirect, credential forwarding, request transport, or verifier callback.
  check(options.rpcUrl === undefined || options.rpcUrl === network.rpcUrl, 'RPC endpoint is not allowlisted');
  check(options.trustedProfile.destinationNetworkId === network.networkId && options.trustedProfile.destinationNativeChainId === network.nativeChainId, 'testnet trusted profile mismatch');
  const adapter = Object.freeze({ chain: options.chain, networkId: network.networkId });
  adapters.set(adapter, freeze({ ...options, ...network }));
  return adapter;
}

/** A validated unsigned transfer description, NOT authenticated authorization. */
export function prepareTestnetTransfer({ adapter, authorization, request, nowUnix } = {}) {
  const c = config(adapter);
  const envelope = data(authorization);
  check(envelope.destination.networkId === c.networkId && envelope.destination.nativeChainId === c.nativeChainId, 'testnet destination required');
  const prepared = prepareNativeTransfer({ chain: c.chain, authorization: envelope, request, nowUnix, trustedProfile: c.trustedProfile });
  preparations.set(prepared, adapter);
  return prepared;
}

async function rpc(c, method, params) {
  // All method names are module constants and are read-only.
  const response = await fetch(c.rpcUrl, {
    method: 'POST', redirect: 'error', signal: AbortSignal.timeout(10000),
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
  });
  check(response.ok, 'RPC HTTP failure');
  check(response.body, 'RPC response body missing');
  const reader = response.body.getReader(), chunks = [];
  let bytes = 0;
  try {
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      bytes += value.byteLength;
      check(bytes <= 2_000_000, 'RPC response exceeds limit');
      chunks.push(Buffer.from(value));
    }
  } finally { await reader.cancel(); reader.releaseLock(); }
  const envelope = JSON.parse(Buffer.concat(chunks).toString('utf8'));
  check(envelope && envelope.id === 1 && !envelope.error && Object.hasOwn(envelope, 'result'), 'RPC error or malformed response');
  return envelope.result;
}
async function networkIdentity(c) {
  if (c.chain === 'ethereum' || c.chain === 'bsc') {
    check(hex(await rpc(c, 'eth_chainId', [])) === BigInt(c.nativeChainId), 'RPC chain mismatch');
    const genesis = await rpc(c, 'eth_getBlockByNumber', ['0x0', false]);
    check(genesis && hash(genesis.hash) === c.expectedGenesisHash && hex(genesis.number) === 0n, 'RPC genesis mismatch');
    return { nativeChainId: c.nativeChainId, genesisHash: genesis.hash };
  }
  if (c.chain === 'near') {
    const status = await rpc(c, 'status', []);
    check(status?.chain_id === c.nativeChainId, 'RPC chain mismatch');
    check(status.genesis_hash === c.expectedGenesisHash, 'RPC genesis mismatch');
    return { nativeChainId: status.chain_id, genesisHash: status.genesis_hash };
  }
  if (c.chain === 'solana') {
    const genesis = await rpc(c, 'getGenesisHash', []);
    check(genesis === c.nativeChainId, 'RPC genesis mismatch');
    return { nativeChainId: genesis, genesisHash: genesis };
  }
  const info = await rpc(c, 'getblockchaininfo', []);
  check(info?.chain === 'test' && Number.isSafeInteger(info.blocks) && info.blocks >= 0 && typeof info.bestblockhash === 'string' && /^[0-9a-f]{64}$/.test(info.bestblockhash), 'Zcash testnet node identity/status mismatch');
  return { nativeChainId: 'testnet', chain: info.chain, blocks: info.blocks, bestblockhash: info.bestblockhash };
}

/** A read-only connection/identity probe, not a light client or settlement proof. */
export async function inspectTestnetNetwork(adapter) {
  try {
    const c = config(adapter), identity = await networkIdentity(c);
    return freeze({ state: 'NETWORK_IDENTITY_OBSERVED_NOT_SETTLED', chain: c.chain, networkId: c.networkId, rpcUrl: c.rpcUrl, identity });
  } catch (error) { return Object.freeze({ state: 'NOT_VERIFIED', reason: error instanceof Error ? error.message : 'RPC identity inspection failed' }); }
}

function windowAndFee(p, timestamp, fee) {
  check(Number.isSafeInteger(timestamp) && timestamp >= p.authorization.time.notBeforeUnix && timestamp < p.authorization.time.expiresAtUnix, 'execution outside authorization time window');
  check(fee >= 0n && fee <= BigInt(p.authorization.fees.maxFeeAtomic), 'native fee exceeds budget');
}
async function evm(c, p, id) {
  hash(id);
  await networkIdentity(c);
  const tx = await rpc(c, 'eth_getTransactionByHash', [id]);
  const receipt = await rpc(c, 'eth_getTransactionReceipt', [id]);
  check(tx && receipt && hash(tx.hash) === id && hash(receipt.transactionHash) === id, 'transaction hash mismatch or pending');
  // Only gas-only transaction envelopes are implemented. Blob fees and EIP-7702
  // delegation introduce additional fees/actions not described by this request.
  check(['0x0', '0x1', '0x2'].includes(tx.type) && receipt.type === tx.type, 'unsupported native EVM transaction/receipt type');
  check(['blobVersionedHashes', 'maxFeePerBlobGas', 'blobGasUsed', 'blobGasPrice', 'authorizationList'].every(field => !Object.hasOwn(tx, field) && !Object.hasOwn(receipt, field)), 'unsupported native EVM blob/delegation fields');
  check(hex(tx.chainId) === BigInt(c.nativeChainId), 'transaction chain mismatch');
  check(tx.to === p.request.recipient && receipt.to === p.request.recipient, 'recipient mismatch');
  check(hex(tx.value) === BigInt(p.request.amountAtomic) && tx.input === '0x', 'amount/actions mismatch');
  check(receipt.status === '0x1', 'transaction failed');
  const blockHash = hash(receipt.blockHash), height = hex(receipt.blockNumber);
  check(tx.blockHash === blockHash && hex(tx.blockNumber) === height, 'transaction inclusion mismatch');
  const block = await rpc(c, 'eth_getBlockByNumber', [receipt.blockNumber, false]);
  const final = await rpc(c, 'eth_getBlockByNumber', ['finalized', false]);
  check(block && hash(block.hash) === blockHash && hex(block.number) === height && block.transactions?.filter(t => t === id).length === 1, 'noncanonical block or missing transaction');
  check(final && hash(final.hash) && hex(final.number) >= height && hex(final.number) - height + 1n >= BigInt(p.authorization.settlement.minConfirmations), 'finality threshold not met');
  windowAndFee(p, Number(hex(block.timestamp)), hex(receipt.gasUsed) * hex(receipt.effectiveGasPrice));
  const again = await rpc(c, 'eth_getBlockByNumber', [receipt.blockNumber, false]);
  check(again?.hash === blockHash, 'reorg detected');
  return { transactionId: id, inclusionReference: blockHash, finality: 'RPC_FINALIZED', amountAtomic: p.request.amountAtomic, recipient: p.request.recipient };
}
function nearSuccess(status) { return status && Object.keys(status).length === 1 && typeof status.SuccessValue === 'string'; }
async function near(c, p, id, signerAccountId) {
  base58(id, 32);
  check(typeof signerAccountId === 'string' && /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/.test(signerAccountId) && signerAccountId.length >= 2 && signerAccountId.length <= 64, 'NEAR signer account required');
  await networkIdentity(c);
  const result = await rpc(c, 'tx', { tx_hash: id, sender_account_id: signerAccountId, wait_until: 'FINAL' });
  const tx = result?.transaction;
  check(tx && tx.hash === id && tx.signer_id === signerAccountId, 'transaction hash/signer mismatch');
  check(tx.receiver_id === p.request.recipient, 'recipient mismatch');
  check(JSON.stringify(tx.actions) === JSON.stringify([{ Transfer: { deposit: p.request.amountAtomic } }]), 'amount/actions mismatch');
  check(result.final_execution_status === 'FINAL' && nearSuccess(result.status), 'transaction not final or failed');
  check(Array.isArray(result.receipts_outcome) && result.receipts_outcome.length > 0 && result.receipts_outcome.length <= 64 && result.transaction_outcome?.id === id, 'receipt outcomes missing or unsupported receipt count');
  const outcomes = [result.transaction_outcome, ...result.receipts_outcome];
  const ids = new Set(outcomes.map(o => base58(o?.id, 32)));
  check(ids.size === outcomes.length, 'duplicate transaction/receipt outcome');
  const byId = new Map(outcomes.map(o => [o.id, o]));
  const reachable = new Set(), pending = [id];
  while (pending.length) {
    const current = pending.pop();
    check(!reachable.has(current), 'cyclic/duplicate receipt reference');
    reachable.add(current);
    const children = byId.get(current)?.outcome?.receipt_ids;
    check(Array.isArray(children) && children.every(r => ids.has(r)), 'partial receipt tree');
    pending.push(...children);
  }
  check(reachable.size === ids.size, 'disconnected receipt outcomes');
  let fee = 0n;
  const blocks = [];
  for (const outcome of outcomes) {
    check(outcome?.outcome && (nearSuccess(outcome.outcome.status) || (Object.keys(outcome.outcome.status || {}).length === 1 && ids.has(outcome.outcome.status.SuccessReceiptId))), 'failed or unresolved receipt');
    check(Array.isArray(outcome.outcome.receipt_ids) && outcome.outcome.receipt_ids.every(r => ids.has(r)), 'partial receipt tree');
    check(typeof outcome.outcome.tokens_burnt === 'string' && /^(0|[1-9][0-9]*)$/.test(outcome.outcome.tokens_burnt), 'invalid native fee');
    fee += BigInt(outcome.outcome.tokens_burnt);
    const block = await rpc(c, 'block', { block_id: base58(outcome.block_hash, 32) });
    check(block?.header?.hash === outcome.block_hash, 'receipt block mismatch');
    uint(block.header.height);
    const canonical = await rpc(c, 'block', { block_id: block.header.height });
    check(canonical?.header?.hash === outcome.block_hash, 'noncanonical receipt block');
    check(typeof block.header.timestamp_nanosec === 'string' && /^[0-9]+$/.test(block.header.timestamp_nanosec), 'invalid block timestamp');
    windowAndFee(p, Number(BigInt(block.header.timestamp_nanosec) / 1_000_000_000n), 0n);
    blocks.push(block.header);
  }
  const final = await rpc(c, 'block', { finality: 'final' });
  uint(final?.header?.height); base58(final.header.hash, 32);
  check(blocks.every(b => final.header.height >= b.height && final.header.height - b.height + 1 >= p.authorization.settlement.minConfirmations), 'finality threshold not met');
  check(fee <= BigInt(p.authorization.fees.maxFeeAtomic), 'native fee exceeds budget');
  for (const b of blocks) check((await rpc(c, 'block', { block_id: b.height }))?.header?.hash === b.hash, 'reorg detected');
  return { transactionId: id, inclusionReference: result.transaction_outcome.block_hash, finality: 'RPC_FINAL', amountAtomic: p.request.amountAtomic, recipient: p.request.recipient };
}
async function solana(c, p, id) {
  base58(id, 64);
  await networkIdentity(c);
  const tx = await rpc(c, 'getTransaction', [id, { commitment: 'finalized', encoding: 'jsonParsed', maxSupportedTransactionVersion: 0 }]);
  check(tx && tx.meta?.err === null && tx.meta && tx.version === 'legacy', 'missing/failed/unsupported Solana transaction');
  uint(tx.slot);
  const message = tx.transaction?.message;
  check(tx.transaction.signatures?.length === 1 && tx.transaction.signatures[0] === id, 'transaction signature mismatch');
  check(message?.recentBlockhash === p.request.recentBlockhash && Array.isArray(message.accountKeys) && message.accountKeys.length === 3, 'blockhash/accounts mismatch');
  const keys = message.accountKeys;
  check(keys[0].pubkey === p.request.feePayer && keys[0].signer === true && keys[0].writable === true && keys[1].pubkey === p.request.recipient && keys[1].signer === false && keys[1].writable === true && keys[2].pubkey === SYSTEM && keys[2].signer === false && keys[2].writable === false, 'signer/accounts mismatch');
  check(message.instructions?.length === 1 && (!tx.meta.innerInstructions || tx.meta.innerInstructions.length === 0), 'extra instructions');
  const i = message.instructions[0];
  check(i.program === 'system' && i.programId === SYSTEM && i.parsed?.type === 'transfer' && i.parsed.info?.source === p.request.feePayer && i.parsed.info.destination === p.request.recipient && Number.isSafeInteger(i.parsed.info.lamports) && String(i.parsed.info.lamports) === p.request.amountAtomic, 'recipient/amount/program mismatch');
  uint(tx.meta.fee); windowAndFee(p, tx.blockTime, BigInt(tx.meta.fee));
  check(Array.isArray(tx.meta.preBalances) && Array.isArray(tx.meta.postBalances) && tx.meta.preBalances.length === 3 && tx.meta.postBalances.length === 3, 'balance observations missing');
  tx.meta.preBalances.forEach(uint); tx.meta.postBalances.forEach(uint);
  check(BigInt(tx.meta.preBalances[0]) - BigInt(tx.meta.postBalances[0]) === BigInt(p.request.amountAtomic) + BigInt(tx.meta.fee) && BigInt(tx.meta.postBalances[1]) - BigInt(tx.meta.preBalances[1]) === BigInt(p.request.amountAtomic) && tx.meta.preBalances[2] === tx.meta.postBalances[2], 'native balance delta mismatch');
  const statuses = await rpc(c, 'getSignatureStatuses', [[id], { searchTransactionHistory: true }]);
  check(statuses?.value?.length === 1 && statuses.value[0]?.slot === tx.slot && statuses.value[0].err === null && statuses.value[0].confirmationStatus === 'finalized', 'signature not finalized');
  const finalSlot = uint(await rpc(c, 'getSlot', [{ commitment: 'finalized' }]));
  check(finalSlot >= tx.slot, 'slot not finalized');
  // Skipped slots are not confirmations; require actual finalized block entries.
  const end = tx.slot + Math.max(0, p.authorization.settlement.minConfirmations - 1);
  check(Number.isSafeInteger(end) && end <= finalSlot && end - tx.slot <= 1000, 'confirmation range unsupported');
  const blocks = await rpc(c, 'getBlocks', [tx.slot, end, { commitment: 'finalized' }]);
  check(Array.isArray(blocks) && blocks.length >= p.authorization.settlement.minConfirmations && new Set(blocks).size === blocks.length && blocks.every(s => Number.isSafeInteger(s) && s >= tx.slot && s <= end), 'insufficient finalized blocks');
  const block = await rpc(c, 'getBlock', [tx.slot, { commitment: 'finalized', transactionDetails: 'signatures', rewards: false }]);
  base58(block?.blockhash, 32);
  uint(block.blockHeight);
  check(block.blockHeight >= p.request.blockHeight && block.blockHeight <= p.request.lastValidBlockHeight, 'transaction block height outside prepared validity');
  check(block.signatures?.filter(s => s === id).length === 1 && block.blockTime === tx.blockTime, 'canonical block inclusion mismatch');
  const again = await rpc(c, 'getBlock', [tx.slot, { commitment: 'finalized', transactionDetails: 'signatures', rewards: false }]);
  check(again?.blockhash === block.blockhash, 'reorg detected');
  return { transactionId: id, inclusionReference: block.blockhash, finality: 'RPC_FINALIZED', amountAtomic: p.request.amountAtomic, recipient: p.request.recipient };
}

/** Narrow node observations, deliberately not settlement evidence or authentication. */
export async function inspectTestnetTransaction(input = {}) {
  try {
    exact(input, ['adapter', 'prepared', 'transactionId', 'signerAccountId', 'authorization'], ['adapter', 'prepared', 'transactionId']);
    const { adapter, prepared, transactionId, signerAccountId, authorization } = input;
    const c = config(adapter);
    check(c.chain !== 'zcash', 'Zcash shielded transaction binding NOT_IMPLEMENTED; use node-only inspectZcashTestnet');
    check(preparations.get(prepared) === adapter, 'trusted testnet preparation handle required');
    if (authorization !== undefined) check(computeCrossL1AuthorizationCommitment(authorization) === prepared.authorizationCommitment, 'authorization mismatch');
    const observation = await (c.chain === 'near' ? near(c, prepared, transactionId, signerAccountId) : c.chain === 'solana' ? solana(c, prepared, transactionId) : evm(c, prepared, transactionId));
    return freeze({ state: 'OBSERVED_NATIVE_TRANSFER_NOT_SETTLED', chain: c.chain, networkId: c.networkId, authorizationCommitment: prepared.authorizationCommitment, requestDigest: prepared.requestDigest, observation });
  } catch (error) {
    return Object.freeze({ state: 'NOT_VERIFIED', reason: error instanceof Error ? error.message : 'RPC inspection failed' });
  }
}

/** Never treat booleans, callbacks, a hash, or a plain transfer as envelope proof. */
export async function verifyTestnetSettlement(input = {}) {
  const observation = await inspectTestnetTransaction(input);
  if (observation.state === 'NOT_VERIFIED') return observation;
  return freeze({ state: 'NOT_IMPLEMENTED', reason: 'Native transfer has no on-chain envelope binding: policy authorization, nonce, replay consumption, expiry and cancellation verification are not implemented', observation });
}

/** Zcash public RPC cannot prove shielded recipient/amount or envelope binding. */
export async function inspectZcashTestnet({ adapter } = {}) {
  try {
    const c = config(adapter); check(c.chain === 'zcash', 'Zcash adapter required');
    const node = await networkIdentity(c);
    return freeze({ state: 'NOT_IMPLEMENTED', reason: 'Shielded address parsing, recipient/amount proof and on-chain envelope binding are not implemented; no transparent fallback', node });
  } catch (error) { return Object.freeze({ state: 'NOT_VERIFIED', reason: error instanceof Error ? error.message : 'RPC inspection failed' }); }
}
