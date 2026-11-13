import { createHash } from 'node:crypto';
import { prepareNativeTransfer, computeNativeRecipientCommitment } from './native-adapters.mjs';
import { prepareEvmOwnerApprovedExecution } from './evm-exact-execution.mjs';
import { prepareSolanaOwnerApprovedExecution } from './solana-exact-execution.mjs';
import { buildNearEnvelopeAuthorizationDispatch } from './near-envelope-dispatch.mjs';
import { canonicalizeCrossL1AuthorizationEnvelope, computeCrossL1AuthorizationCommitment } from './cross-l1-envelope.mjs';

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
const executions = new WeakMap();
const evmExecutions = new WeakMap();
const solanaExecutions = new WeakMap();
const verifiedSettlements = new WeakMap();
const HASH = /^0x[0-9a-f]{64}$/;
const B58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
const SYSTEM = '11111111111111111111111111111111';
const UPGRADEABLE_LOADER = 'BPFLoaderUpgradeab1e11111111111111111111111';
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
  exact(options, ['chain', 'trustedProfile', 'rpcUrl', 'nearContract', 'evmContract', 'solanaProgram'], ['chain', 'trustedProfile']);
  check(Object.hasOwn(TESTNET_NETWORKS, options.chain), 'unsupported testnet');
  const network = TESTNET_NETWORKS[options.chain];
  // No arbitrary URL, redirect, credential forwarding, request transport, or verifier callback.
  check(options.rpcUrl === undefined || options.rpcUrl === network.rpcUrl, 'RPC endpoint is not allowlisted');
  check(options.trustedProfile.destinationNetworkId === network.networkId && options.trustedProfile.destinationNativeChainId === network.nativeChainId, 'testnet trusted profile mismatch');
  if (options.nearContract !== undefined) {
    check(options.chain === 'near', 'NEAR contract requires NEAR adapter');
    exact(options.nearContract, ['accountId', 'codeHash']);
    nearAccount(options.nearContract.accountId);
    base58(options.nearContract.codeHash, 32);
  }
  if (options.evmContract !== undefined) {
    check(['ethereum', 'bsc'].includes(options.chain), 'EVM contract requires EVM testnet adapter');
    exact(options.evmContract, ['address', 'codeHashSha256', 'owner']);
    check([options.evmContract.address, options.evmContract.owner].every(v => typeof v === 'string' && /^0x[0-9a-f]{40}$/.test(v) && v !== `0x${'0'.repeat(40)}`) && options.evmContract.address !== options.evmContract.owner, 'invalid EVM contract/owner');
    check(typeof options.evmContract.codeHashSha256 === 'string' && /^[0-9a-f]{64}$/.test(options.evmContract.codeHashSha256), 'invalid EVM runtime SHA256 code pin');
  }
  if (options.solanaProgram !== undefined) {
    check(options.chain === 'solana', 'Solana program requires Solana devnet adapter');
    exact(options.solanaProgram, ['programId', 'owner', 'codeHashSha256']);
    base58(options.solanaProgram.programId, 32); base58(options.solanaProgram.owner, 32);
    check(options.solanaProgram.programId !== options.solanaProgram.owner && ![SYSTEM, UPGRADEABLE_LOADER].includes(options.solanaProgram.programId), 'invalid Solana program/owner');
    check(typeof options.solanaProgram.codeHashSha256 === 'string' && /^[0-9a-f]{64}$/.test(options.solanaProgram.codeHashSha256), 'invalid Solana program SHA256 code pin');
  }
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
async function evm(c, p, id, boundCall) {
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
  if (boundCall) {
    check(tx.to === c.evmContract.address && receipt.to === c.evmContract.address && tx.from === p.execute.from && receipt.from === p.execute.from, 'EVM executor/agent mismatch');
    check(hex(tx.value) === BigInt(p.execute.value) && tx.input === `0xe751f271${p.authorizationCommitment.slice(2)}`, 'EVM execution amount/calldata mismatch');
  } else {
    check(tx.to === p.request.recipient && receipt.to === p.request.recipient, 'recipient mismatch');
    check(hex(tx.value) === BigInt(p.request.amountAtomic) && tx.input === '0x', 'amount/actions mismatch');
  }
  check(receipt.status === '0x1', 'transaction failed');
  const blockHash = hash(receipt.blockHash), height = hex(receipt.blockNumber);
  check(tx.blockHash === blockHash && hex(tx.blockNumber) === height, 'transaction inclusion mismatch');
  const block = await rpc(c, 'eth_getBlockByNumber', [receipt.blockNumber, false]);
  const final = await rpc(c, 'eth_getBlockByNumber', ['finalized', false]);
  check(block && hash(block.hash) === blockHash && hex(block.number) === height && block.transactions?.filter(t => t === id).length === 1, 'noncanonical block or missing transaction');
  check(final && hash(final.hash) && hex(final.number) >= height && hex(final.number) - height + 1n >= BigInt(p.authorization.settlement.minConfirmations), 'finality threshold not met');
  windowAndFee(p, Number(hex(block.timestamp)), hex(receipt.gasUsed) * hex(receipt.effectiveGasPrice));
  if (boundCall) await boundCall.validate(tx, receipt, block);
  const again = await rpc(c, 'eth_getBlockByNumber', [receipt.blockNumber, false]);
  check(again?.hash === blockHash, 'reorg detected');
  return { transactionId: id, inclusionReference: blockHash, finality: 'RPC_FINALIZED', amountAtomic: p.request.amountAtomic, recipient: p.request.recipient };
}
function nearSuccess(status) { return status && Object.keys(status).length === 1 && typeof status.SuccessValue === 'string'; }
// Fixed QPCNativeTestnetExecutor ABI selectors/topics, derived with Keccak-256.
// No caller-provided ABI, calldata, endpoint or verification callback is used.
const EVM_EXECUTED_TOPIC = '0x20f591e22f01104e4c2a646a938001e2382a73446550a3b72a94549b841f8bf8';
function abiWord(value) {
  if (typeof value === 'string' && /^0x(?:[0-9a-f]{40}|[0-9a-f]{64})$/.test(value)) return value.slice(2).padStart(64, '0');
  check((typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value)) || (Number.isSafeInteger(value) && value >= 0), 'invalid static ABI value');
  const hex = BigInt(value).toString(16); check(hex.length <= 64, 'static ABI overflow'); return hex.padStart(64, '0');
}
function abiWords(values) { return `0x${values.map(abiWord).join('')}`; }

/** Exact owner-attested executor profile, distinct from a plain transfer. */
export function prepareEvmExecution(input = {}) {
  exact(input, ['adapter', 'authorization', 'recipient', 'nowUnix']);
  const { adapter, authorization, recipient, nowUnix } = input, c = config(adapter);
  check(['ethereum', 'bsc'].includes(c.chain) && c.evmContract, 'operator-pinned EVM executor required');
  const p = prepareEvmOwnerApprovedExecution({ authorization: data(authorization), recipient, nowUnix,
    trustedProfile: { chain: c.chain, nativeChainId: c.nativeChainId, contractAddress: c.evmContract.address, ownerAddress: c.evmContract.owner } });
  const prepared = freeze({ ...p, request: { recipient, amountAtomic: p.authorization.amount.destinationAtomic } });
  evmExecutions.set(prepared, adapter);
  return prepared;
}

/** Independently inspect approved code/owner, exact execution, and durable state. */
export async function inspectEvmExecutionTransaction(input = {}) {
  try {
    exact(input, ['adapter', 'prepared', 'transactionId']);
    const { adapter, prepared: p, transactionId } = input, c = config(adapter);
    check(evmExecutions.get(p) === adapter, 'trusted EVM execution preparation required');
    const intent = p.approveIntent.args[1];
    let actualFeeAtomic;
    const observation = await evm(c, p, transactionId, {
      async validate(tx, receipt, block) {
        const index = hex(receipt.transactionIndex);
        check(hex(tx.transactionIndex) === index && index < BigInt(block.transactions.length) && block.transactions[Number(index)] === transactionId, 'EVM transaction index mismatch');
        check(Array.isArray(receipt.logs), 'native EVM execution logs missing');
        const events = receipt.logs.filter(log => log?.address === c.evmContract.address && log.topics?.[0] === EVM_EXECUTED_TOPIC);
        check(events.length === 1, 'unique native EVM execution event required');
        const event = events[0];
        check(event.removed === false && event.transactionHash === transactionId && event.blockHash === block.hash && hex(event.blockNumber) === hex(block.number) && hex(event.transactionIndex) === index && hex(event.logIndex) >= 0n, 'native EVM log inclusion mismatch');
        check(sameData(event.topics, [EVM_EXECUTED_TOPIC, p.authorizationCommitment, abiWords([intent.agent]), abiWords([intent.recipient])]) && event.data === abiWords([intent.amount, intent.nonce]), 'native EVM execution event binding mismatch');
        for (const blockHash of [hash(block.parentHash), hash(block.hash)]) {
          const code = await rpc(c, 'eth_getCode', [c.evmContract.address, { blockHash, requireCanonical: true }]);
          check(typeof code === 'string' && /^0x(?:[0-9a-f]{2})+$/.test(code) && createHash('sha256').update(Buffer.from(code.slice(2), 'hex')).digest('hex') === c.evmContract.codeHashSha256, 'approved EVM runtime code mismatch');
        }
        const blockRef = { blockHash: block.hash, requireCanonical: true };
        const call = selector => rpc(c, 'eth_call', [{ to: c.evmContract.address, data: selector }, blockRef]);
        check(await call('0x8da5cb5b') === abiWords([c.evmContract.owner]), 'EVM executor owner mismatch');
        const suffix = p.authorizationCommitment.slice(2);
        for (const [selector, value] of [['0x7865c3ad', '1'], ['0x328a93e8', '0'], ['0xa9fcfb33', '1']]) {
          check(await call(`${selector}${suffix}`) === abiWords([value]), 'EVM intent approval/revocation/execution state mismatch');
        }
        check(await call(`0xf13c46aa${suffix}`) === abiWords(Object.values(intent)), 'EVM approved typed intent mismatch');
        check(await call(`0xcd4b90f9${abiWord(intent.agent)}`) === abiWords(['1', intent.policyIdHash, intent.policyVersion, intent.revocationEpoch]), 'EVM native policy state mismatch');
        check(await call(`0x0a2dc12f${abiWord(intent.agent)}${abiWord(intent.nonce)}`) === abiWords(['1']), 'EVM native replay reservation missing');
        actualFeeAtomic = (hex(receipt.gasUsed) * hex(receipt.effectiveGasPrice)).toString();
      },
    });
    return freeze({ state: 'OBSERVED_EVM_EXECUTION_NOT_SETTLED', authorizationCommitment: p.authorizationCommitment, observation, actualFeeAtomic, feeBudgetVerifiedByRpc: true, feeBudgetEnforcedOnChain: false, trust: 'OWNER_ATTESTED_ENVELOPE_AND_OPERATOR_PINNED_RPC_NOT_LIGHT_CLIENT' });
  } catch (error) { return Object.freeze({ state: 'NOT_VERIFIED', reason: error instanceof Error ? error.message : 'EVM execution inspection failed' }); }
}

export async function verifyEvmExecutionSettlement(input = {}) {
  try {
    exact(input, ['adapter', 'prepared', 'transactionId']);
    const { adapter, prepared, transactionId } = input;
    const observed = await inspectEvmExecutionTransaction({ adapter, prepared, transactionId });
    if (observed.state !== 'OBSERVED_EVM_EXECUTION_NOT_SETTLED') return observed;
    return nativeSettlementReceipt(prepared, adapter, observed);
  } catch (error) { return Object.freeze({ state: 'NOT_VERIFIED', reason: error instanceof Error ? error.message : 'EVM settlement inspection failed' }); }
}

function nearAccount(value) {
  check(typeof value === 'string' && /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/.test(value) && value.length >= 2 && value.length <= 64, 'NEAR account required');
  return value;
}
async function near(c, p, id, signerAccountId, boundCall) {
  base58(id, 32);
  nearAccount(signerAccountId);
  await networkIdentity(c);
  const result = await rpc(c, 'tx', { tx_hash: id, sender_account_id: signerAccountId, wait_until: 'FINAL' });
  const tx = result?.transaction;
  check(tx && tx.hash === id && tx.signer_id === signerAccountId, 'transaction hash/signer mismatch');
  if (boundCall) {
    check(tx.receiver_id === c.nearContract.accountId, 'authorization contract mismatch');
    check(Array.isArray(tx.actions) && tx.actions.length === 1, 'exactly one native authorization action required');
    exact(tx.actions[0], ['FunctionCall']);
    const call = tx.actions[0].FunctionCall;
    exact(call, ['method_name', 'args', 'gas', 'deposit']);
    check(call.method_name === boundCall.method && call.deposit === boundCall.deposit && Number.isSafeInteger(call.gas) && call.gas > 0 && call.gas <= 300_000_000_000_000, 'authorization action method/deposit/gas mismatch');
    check(call.args === Buffer.from(JSON.stringify(boundCall.args)).toString('base64'), 'authorization call arguments mismatch');
  } else {
    check(tx.receiver_id === p.request.recipient, 'recipient mismatch');
    check(JSON.stringify(tx.actions) === JSON.stringify([{ Transfer: { deposit: p.request.amountAtomic } }]), 'amount/actions mismatch');
  }
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
  if (boundCall) {
    await boundCall.validate(result, blocks, final.header, fee);
    for (const b of blocks) check((await rpc(c, 'block', { block_id: b.height }))?.header?.hash === b.hash, 'reorg detected after contract inspection');
  }
  return { transactionId: id, inclusionReference: result.transaction_outcome.block_hash, finality: 'RPC_FINAL', amountAtomic: p.request.amountAtomic, recipient: p.request.recipient };
}
function nearContract(c, authorization) {
  check(c.chain === 'near' && c.nearContract, 'operator-pinned NEAR contract required');
  const source = authorization.source;
  check(source.networkId === c.networkId && source.nativeChainId === c.nativeChainId && source.adapterId === c.nearContract.accountId, 'NEAR source contract/network mismatch');
  check(authorization.agentIdentity.namespace === 'near' && authorization.agentIdentity.id === source.accountId && authorization.agentIdentity.keyId === source.accountId, 'NEAR predecessor identity mismatch');
  nearAccount(source.accountId);
  check(authorization.replayDomain.authorityId === c.nearContract.accountId, 'NEAR replay authority mismatch');
}
function jsonBase64(value) {
  check(typeof value === 'string' && value.length <= 1_000_000 && Buffer.from(value, 'base64').toString('base64') === value, 'invalid native JSON/base64');
  return JSON.parse(Buffer.from(value, 'base64').toString('utf8'));
}
function sameData(a, b) {
  if (a === null || b === null || typeof a !== 'object' || typeof b !== 'object') return a === b;
  return Array.isArray(a) === Array.isArray(b) && Object.keys(a).length === Object.keys(b).length && Object.keys(a).every(k => Object.hasOwn(b, k) && sameData(a[k], b[k]));
}
async function pinnedNearCode(c, result, blocks) {
  // These are historical node observations, not a proof of immutable deployment
  // provenance. Operator trust includes custody of this contract's upgrade keys.
  const contractBlocks = blocks.filter((_, i) => [result.transaction_outcome, ...result.receipts_outcome][i].outcome.executor_id === c.nearContract.accountId);
  check(contractBlocks.length > 0, 'contract execution outcome missing');
  for (const block of contractBlocks) {
    base58(block.prev_hash, 32);
    for (const blockHash of [block.prev_hash, block.hash]) {
      const account = await rpc(c, 'query', { request_type: 'view_account', account_id: c.nearContract.accountId, block_id: blockHash });
      check(account?.block_hash === blockHash && account.code_hash === c.nearContract.codeHash, 'approved NEAR contract code mismatch');
    }
  }
}
function contractEvent(outcome, event, expected) {
  check(Array.isArray(outcome?.outcome?.logs), 'contract event logs missing');
  const events = outcome.outcome.logs.filter(log => typeof log === 'string' && log.startsWith('EVENT_JSON:')).map(log => JSON.parse(log.slice(11)));
  check(events.filter(e => e.standard === 'qpc' && e.version === '1.0.0' && e.event === event && sameData(e.data, [expected])).length === 1, 'native contract event mismatch');
  check(events.filter(e => e.event === event).length === 1, 'duplicate native contract event');
}

function nearAuthorizationRecord(c, p, block) {
  const e = p.authorization;
  return { state: 'AUTHORIZED', executionStatus: 'NOT_EXECUTED', intentId: e.intentId, authorizationCommitment: p.authorizationCommitment,
    agentId: e.source.accountId, adapterId: c.nearContract.accountId, nonce: e.nonce,
    policyId: e.policy.id, policyVersion: e.policy.version, policyCommitment: e.policy.commitment,
    revocationEpoch: e.cancellation.revocationEpoch, authorizedAtUnix: Number(BigInt(block.timestamp_nanosec) / 1_000_000_000n), expiresAtUnix: e.time.expiresAtUnix, blockHeight: block.height };
}
async function nearView(c, final, method, args) {
  const account = await rpc(c, 'query', { request_type: 'view_account', account_id: c.nearContract.accountId, block_id: final.hash });
  check(account?.block_hash === final.hash && account.code_hash === c.nearContract.codeHash, 'approved NEAR final contract code mismatch');
  const result = await rpc(c, 'query', { request_type: 'call_function', account_id: c.nearContract.accountId, method_name: method, args_base64: Buffer.from(JSON.stringify(args)).toString('base64'), block_id: final.hash });
  check(result?.block_hash === final.hash && result.block_height === final.height && Array.isArray(result.result) && result.result.every(b => Number.isInteger(b) && b >= 0 && b <= 255), 'historical contract query mismatch');
  return JSON.parse(Buffer.from(result.result).toString('utf8'));
}

/** Inspect exact native predecessor authorization, never destination settlement. */
export async function inspectNearAuthorizationTransaction(input = {}) {
  try {
    exact(input, ['adapter', 'prepared', 'transactionId']);
    const { adapter, prepared: p, transactionId } = input, c = config(adapter);
    check(preparations.get(p) === adapter, 'trusted testnet preparation handle required');
    nearContract(c, p.authorization);
    const dispatch = buildNearEnvelopeAuthorizationDispatch({ envelope: p.authorization, nowUnix: p.authorization.time.notBeforeUnix });
    const observation = await near(c, p, transactionId, p.authorization.source.accountId, {
      method: 'authorize_envelope', deposit: '0', args: dispatch.qpcCall.args,
      async validate(result, blocks, final) {
        const roots = result.transaction_outcome.outcome.receipt_ids;
        check(roots.length === 1, 'single native contract receipt required');
        const root = result.receipts_outcome.find(o => o.id === roots[0]);
        check(root?.outcome.executor_id === c.nearContract.accountId && nearSuccess(root.outcome.status), 'contract authorization outcome missing');
        const expected = nearAuthorizationRecord(c, p, blocks[result.receipts_outcome.indexOf(root) + 1]);
        check(sameData(jsonBase64(root.outcome.status.SuccessValue), expected) && sameData(jsonBase64(result.status.SuccessValue), expected), 'authorization return mismatch');
        contractEvent(root, 'agent_action_authorized', expected);
        await pinnedNearCode(c, result, blocks);
        check(sameData(await nearView(c, final, 'get_authorization_receipt', { intent_id: p.authorization.intentId }), expected), 'persisted authorization receipt mismatch');
      },
    });
    return freeze({ state: 'OBSERVED_NATIVE_AUTHORIZATION_NOT_SETTLED', authorizationCommitment: p.authorizationCommitment, observation, trust: 'OPERATOR_PINNED_CONTRACT_AND_RPC_NOT_LIGHT_CLIENT' });
  } catch (error) { return Object.freeze({ state: 'NOT_VERIFIED', reason: error instanceof Error ? error.message : 'NEAR authorization inspection failed' }); }
}

/** Separate execution profile; no signing, broadcast, or claimed authorization. */
export function prepareNearExecution(input = {}) {
  exact(input, ['adapter', 'authorization', 'recipient', 'nowUnix']);
  const { adapter, recipient, nowUnix } = input, c = config(adapter);
  const authorization = canonicalizeCrossL1AuthorizationEnvelope(data(input.authorization));
  nearContract(c, authorization);
  nearAccount(recipient);
  check(recipient !== c.nearContract.accountId, 'self-transfer is unsupported');
  const e = authorization;
  check(e.destination.networkId === c.networkId && e.destination.nativeChainId === c.nativeChainId && e.destination.adapterId === c.nearContract.accountId, 'same-chain NEAR execution required');
  check(e.action.type === 'asset-transfer' && e.action.operation === 'pay' && e.asset.sourceAssetId === 'near:native' && e.asset.destinationAssetId === 'near:native' && e.asset.sourceDecimals === 24 && e.asset.destinationDecimals === 24 && e.asset.mappingId === 'near-native-identity-v1' && e.amount.sourceAtomic === e.amount.destinationAtomic, 'unsupported NEAR execution asset/action mapping');
  check(e.fees.feeAssetId === 'near:native' && e.fees.maxSlippageBps === 0 && e.settlement.evidenceClass === 'near-native-transfer-result' && e.settlement.finalityPolicy === 'near-execution-outcome' && e.settlement.minConfirmations === 0, 'unsupported NEAR execution fee/evidence profile');
  check(Number.isSafeInteger(nowUnix) && nowUnix >= e.time.issuedAtUnix && nowUnix >= e.time.notBeforeUnix && nowUnix < e.time.expiresAtUnix, 'authorization outside valid time window');
  check(e.destination.recipientCommitment === computeNativeRecipientCommitment({ chain: 'near', nativeChainId: c.nativeChainId, recipient }), 'NEAR recipient commitment mismatch');
  const authorizationCommitment = computeCrossL1AuthorizationCommitment(e);
  const prepared = freeze({ state: 'PREPARED_NEAR_EXECUTION_UNSIGNED', authorization: e, authorizationCommitment,
    request: { recipient, amountAtomic: e.amount.destinationAtomic },
    qpcCall: { receiverId: c.nearContract.accountId, methodName: 'execute_near_envelope', args: { envelope: e, authorization_commitment: authorizationCommitment, recipient }, deposit: e.amount.destinationAtomic, gas: '100000000000000' },
  });
  executions.set(prepared, adapter);
  return prepared;
}

/** Native same-chain execution observation, never a durable settlement token. */
export async function inspectNearExecutionTransaction(input = {}) {
  try {
    exact(input, ['adapter', 'prepared', 'transactionId']);
    const { adapter, prepared: p, transactionId } = input, c = config(adapter);
    check(executions.get(p) === adapter, 'trusted NEAR execution preparation required');
    nearContract(c, p.authorization);
    let actualFeeAtomic;
    const observation = await near(c, p, transactionId, p.authorization.source.accountId, {
      method: 'execute_near_envelope', deposit: p.qpcCall.deposit, args: p.qpcCall.args,
      async validate(result, blocks, final, fee) {
        const roots = result.transaction_outcome.outcome.receipt_ids;
        check(roots.length === 1, 'single native execution receipt required');
        const rootIndex = result.receipts_outcome.findIndex(o => o.id === roots[0]), root = result.receipts_outcome[rootIndex];
        check(root?.outcome.executor_id === c.nearContract.accountId, 'native execution contract outcome missing');
        const authorizationRecord = nearAuthorizationRecord(c, p, blocks[rootIndex + 1]);
        contractEvent(root, 'agent_action_authorized', authorizationRecord);
        const record = jsonBase64(result.status.SuccessValue);
        const callbacks = result.receipts_outcome.filter(o => o.outcome.executor_id === c.nearContract.accountId && nearSuccess(o.outcome.status) && o.outcome.status.SuccessValue === result.status.SuccessValue);
        check(callbacks.length === 1, 'unique native execution callback required');
        const callback = callbacks[0], callbackBlock = blocks[result.receipts_outcome.indexOf(callback) + 1];
        const expected = { state: 'EXECUTED', intentId: p.authorization.intentId, authorizationCommitment: p.authorizationCommitment, recipient: p.request.recipient, amountAtomic: p.request.amountAtomic,
          authorizedAtUnix: authorizationRecord.authorizedAtUnix, completedAtUnix: Number(BigInt(callbackBlock.timestamp_nanosec) / 1_000_000_000n), contractFeeAtomic: '0', protocolGasFee: 'NOT_VERIFIED', feeBudgetEnforced: false, refundState: 'NOT_APPLICABLE', refundAttempt: 0 };
        check(sameData(record, expected), 'native execution result mismatch or failed');
        contractEvent(callback, 'near_transfer_result', expected);
        const transfers = result.receipts_outcome.filter(o => o.outcome.executor_id === p.request.recipient && nearSuccess(o.outcome.status) && o.outcome.status.SuccessValue === '');
        check(transfers.length === 1, 'unique successful recipient outcome required');
        const transfer = await rpc(c, 'EXPERIMENTAL_receipt', { receipt_id: transfers[0].id });
        check(transfer?.receipt_id === transfers[0].id && transfer.predecessor_id === c.nearContract.accountId && transfer.receiver_id === p.request.recipient && transfer.receipt?.Action?.signer_id === p.authorization.source.accountId && sameData(transfer.receipt.Action.actions, [{ Transfer: { deposit: p.request.amountAtomic } }]), 'native transfer receipt actions/recipient mismatch');
        const callbackReceipt = await rpc(c, 'EXPERIMENTAL_receipt', { receipt_id: callback.id });
        check(callbackReceipt?.receipt_id === callback.id && callbackReceipt.predecessor_id === c.nearContract.accountId && callbackReceipt.receiver_id === c.nearContract.accountId && sameData(callbackReceipt.receipt?.Action?.actions, [{ FunctionCall: { method_name: 'on_near_transfer_result', args: Buffer.from(JSON.stringify({ intent_id: p.authorization.intentId, authorization_commitment: p.authorizationCommitment })).toString('base64'), gas: 10000000000000, deposit: '0' } }]), 'native callback receipt mismatch');
        const dependency = callbackReceipt.receipt.Action.input_data_ids;
        check(Array.isArray(dependency) && dependency.length === 1 && base58(dependency[0], 32) && sameData(transfer.receipt.Action.output_data_receivers, [{ data_id: dependency[0], receiver_id: c.nearContract.accountId }]), 'transfer/callback native dependency mismatch');
        await pinnedNearCode(c, result, blocks);
        check(sameData(await nearView(c, final, 'get_execution_result', { intent_id: p.authorization.intentId }), expected), 'persisted execution result mismatch');
        check(sameData(await nearView(c, final, 'get_authorization_receipt', { intent_id: p.authorization.intentId }), authorizationRecord), 'persisted authorization receipt mismatch');
        actualFeeAtomic = fee.toString();
      },
    });
    return freeze({ state: 'OBSERVED_NEAR_EXECUTION_NOT_SETTLED', authorizationCommitment: p.authorizationCommitment, observation, actualFeeAtomic, feeBudgetVerifiedByRpc: true, feeBudgetEnforcedOnChain: false, trust: 'OPERATOR_PINNED_CONTRACT_AND_RPC_NOT_LIGHT_CLIENT' });
  } catch (error) { return Object.freeze({ state: 'NOT_VERIFIED', reason: error instanceof Error ? error.message : 'NEAR execution inspection failed' }); }
}

function b58Bytes(value) {
  check(typeof value === 'string' && value.length > 0 && value.length <= 2048, 'invalid base58 data');
  let n = 0n; for (const ch of value) { const d = B58.indexOf(ch); check(d >= 0, 'invalid base58 data'); n = n * 58n + BigInt(d); }
  const tail = []; for (; n; n >>= 8n) tail.unshift(Number(n & 255n));
  return Buffer.concat([Buffer.alloc(value.match(/^1*/)[0].length), Buffer.from(tail)]);
}
function b58String(bytes) {
  let n = 0n; for (const b of bytes) n = n * 256n + BigInt(b);
  let text = ''; for (; n; n /= 58n) text = B58[Number(n % 58n)] + text;
  let zeros = 0; while (zeros < bytes.length && bytes[zeros] === 0) zeros++;
  return '1'.repeat(zeros) + text;
}
function accountBytes(account) {
  check(account && Array.isArray(account.data) && account.data.length === 2 && account.data[1] === 'base64' && typeof account.data[0] === 'string', 'Solana account bytes missing');
  const bytes = Buffer.from(account.data[0], 'base64');
  check(bytes.toString('base64') === account.data[0], 'noncanonical Solana account encoding');
  return bytes;
}

/** Owner-attested native program profile; no transaction is signed or submitted. */
export function prepareSolanaExecution(input = {}) {
  exact(input, ['adapter', 'authorization', 'recipient', 'policyAddress', 'intentAddress', 'nowUnix']);
  const { adapter, authorization, recipient, policyAddress, intentAddress, nowUnix } = input, c = config(adapter);
  check(c.chain === 'solana' && c.solanaProgram, 'operator-pinned Solana program required');
  const p = prepareSolanaOwnerApprovedExecution({ authorization: data(authorization), recipient, policyAddress, intentAddress, nowUnix,
    trustedProfile: { programId: c.solanaProgram.programId, owner: c.solanaProgram.owner, genesisHash: c.nativeChainId } });
  const prepared = freeze({ ...p, request: { recipient, amountAtomic: p.authorization.amount.destinationAtomic } });
  solanaExecutions.set(prepared, adapter);
  return prepared;
}

async function solanaProgramSnapshot(c, p, programDataAddress, executionSlot) {
  // minContextSlot is ONLY a freshness floor, never a historical-state request.
  const response = await rpc(c, 'getMultipleAccounts', [[c.solanaProgram.programId, programDataAddress, p.policyAddress, p.intentAddress], { encoding: 'base64', commitment: 'finalized', minContextSlot: executionSlot }]);
  check(uint(response?.context?.slot) >= executionSlot && response.value?.length === 4, 'Solana program/state snapshot missing or stale');
  const [program, programData, policy, intent] = response.value;
  const programBytes = accountBytes(program), code = accountBytes(programData);
  check(program.executable === true && program.owner === UPGRADEABLE_LOADER && programBytes.length === 36 && programBytes.readUInt32LE(0) === 2 && b58String(programBytes.subarray(4)) === programDataAddress, 'Solana executable loader binding mismatch');
  check(programData.executable === false && programData.owner === UPGRADEABLE_LOADER && code.length > 45 && code.readUInt32LE(0) === 3, 'Solana ProgramData loader metadata missing');
  // An upgrade in the execution slot could occur after this transaction. Current
  // getAccountInfo cannot disambiguate that ordering: reject, do not infer history.
  check(code.readBigUInt64LE(4) < BigInt(executionSlot), 'NOT_VERIFIED: Solana code-at-execution unavailable (deployment is same-slot or newer)');
  check(code[12] === 0, 'NOT_VERIFIED: mutable Solana upgrade authority unsupported');
  check(createHash('sha256').update(code.subarray(45)).digest('hex') === c.solanaProgram.codeHashSha256, 'Solana executable code pin mismatch');
  check(policy?.owner === c.solanaProgram.programId && intent?.owner === c.solanaProgram.programId && policy.executable === false && intent.executable === false, 'Solana intent/policy account owner mismatch');
  // Runtime PDA checks and owner approval are attested by exact successful code.
  // These are current monotonic records, not falsely labelled historical reads.
  const policyHex = p.initialize.dataHex.slice(2), intentHex = `${p.approve.dataHex.slice(2, -2)}01`;
  check(accountBytes(policy).toString('hex') === policyHex && accountBytes(intent).toString('hex') === intentHex, 'Solana owner-attested policy/intent record mismatch');
  return { programDataAddress, deploymentSlot: code.readBigUInt64LE(4).toString(), codeHash: c.solanaProgram.codeHashSha256 };
}

async function solanaOwnerApproval(c, p, executionSlot, deploymentSlot) {
  // A prior malicious upgrade could have planted otherwise plausible records.
  // Independently discover the genuine owner-signed approval after this code's
  // last deployment. No history, pruned history, or bounded scan miss => no proof.
  const history = await rpc(c, 'getSignaturesForAddress', [p.intentAddress, { commitment: 'finalized', limit: 20 }]);
  check(Array.isArray(history) && history.length <= 20 && new Set(history.map(h => h?.signature)).size === history.length, 'Solana approval history unavailable');
  for (const entry of history) {
    if (entry?.err !== null || entry.confirmationStatus !== 'finalized' || !Number.isSafeInteger(entry.slot) || entry.slot >= executionSlot || BigInt(entry.slot) <= BigInt(deploymentSlot)) continue;
    base58(entry.signature, 64);
    const tx = await rpc(c, 'getTransaction', [entry.signature, { commitment: 'finalized', encoding: 'jsonParsed', maxSupportedTransactionVersion: 0 }]);
    const message = tx?.transaction?.message, expected = p.approve.accounts;
    if (!tx || tx.version !== 'legacy' || tx.meta?.err !== null || tx.slot !== entry.slot || tx.transaction?.signatures?.length !== 1 || tx.transaction.signatures[0] !== entry.signature) continue;
    const keys = message?.accountKeys;
    if (!Array.isArray(keys) || keys.length !== expected.length + 1 || keys[0]?.pubkey !== c.solanaProgram.owner || new Set(keys.map(k => k.pubkey)).size !== keys.length) continue;
    if (!keys.every(k => { const e = expected.find(a => a.pubkey === k.pubkey); return e ? k.signer === e.isSigner && k.writable === e.isWritable : k.pubkey === c.solanaProgram.programId && k.signer === false && k.writable === false; })) continue;
    const outer = message.instructions?.[0];
    if (message.instructions?.length !== 1 || outer?.programId !== c.solanaProgram.programId || !sameData(outer.accounts, expected.map(a => a.pubkey)) || b58Bytes(outer.data).toString('hex') !== p.approve.dataHex) continue;
    base58(message.recentBlockhash, 32);
    check(Number.isSafeInteger(tx.blockTime) && tx.blockTime >= p.authorization.time.issuedAtUnix && tx.blockTime < p.authorization.time.expiresAtUnix, 'Solana owner approval outside authorization window');
    const params = [entry.slot, { commitment: 'finalized', transactionDetails: 'signatures', rewards: false }];
    const block = await rpc(c, 'getBlock', params);
    base58(block?.blockhash, 32);
    check(block.signatures?.filter(s => s === entry.signature).length === 1 && block.blockTime === tx.blockTime, 'Solana owner approval canonical inclusion mismatch');
    check((await rpc(c, 'getBlock', params))?.blockhash === block.blockhash, 'Solana owner approval reorg detected');
    return { transactionId: entry.signature, slot: entry.slot, inclusionReference: block.blockhash };
  }
  throw new Error('NOT_VERIFIED: genuine owner-signed Solana approval after pinned code deployment unavailable in bounded RPC history');
}

/** Exact native instruction, CPI, finalized inclusion, owner records and code. */
export async function inspectSolanaExecutionTransaction(input = {}) {
  try {
    exact(input, ['adapter', 'prepared', 'transactionId']);
    const { adapter, prepared: p, transactionId: id } = input, c = config(adapter);
    check(solanaExecutions.get(p) === adapter, 'trusted Solana execution preparation required');
    base58(id, 64); await networkIdentity(c);
    const tx = await rpc(c, 'getTransaction', [id, { commitment: 'finalized', encoding: 'jsonParsed', maxSupportedTransactionVersion: 0 }]);
    check(tx && tx.version === 'legacy' && tx.meta?.err === null, 'missing/failed/unsupported Solana execution');
    const slot = uint(tx.slot), message = tx.transaction?.message, meta = tx.meta;
    check(tx.transaction?.signatures?.length === 1 && tx.transaction.signatures[0] === id, 'Solana execution signature mismatch');
    base58(message?.recentBlockhash, 32);
    const keys = message.accountKeys, expected = p.execute.accounts;
    check(Array.isArray(keys) && keys.length === expected.length + 1 && new Set(keys.map(k => k.pubkey)).size === keys.length && keys[0]?.pubkey === p.authorization.agentIdentity.id, 'Solana execution account set/payer mismatch');
    for (const k of keys) {
      const e = expected.find(a => a.pubkey === k.pubkey);
      check(e ? k.signer === e.isSigner && k.writable === e.isWritable : k.pubkey === c.solanaProgram.programId && k.signer === false && k.writable === false, 'Solana execution account metas mismatch');
    }
    check(message.instructions?.length === 1, 'extra Solana outer instructions');
    const outer = message.instructions[0];
    check(outer.programId === c.solanaProgram.programId && sameData(outer.accounts, expected.map(a => a.pubkey)) && b58Bytes(outer.data).toString('hex') === p.execute.dataHex, 'Solana execute instruction bytes/accounts mismatch');
    check(meta.innerInstructions?.length === 1 && meta.innerInstructions[0].index === 0 && meta.innerInstructions[0].instructions?.length === 1, 'exactly one Solana inner transfer required');
    const inner = meta.innerInstructions[0].instructions[0], transfer = inner.parsed?.info;
    check(inner.programId === SYSTEM && inner.program === 'system' && inner.parsed?.type === 'transfer' && inner.stackHeight === 2 && transfer?.source === p.authorization.agentIdentity.id && transfer.destination === p.request.recipient && Number.isSafeInteger(transfer.lamports) && String(transfer.lamports) === p.request.amountAtomic, 'Solana inner transfer amount/recipient/program mismatch');
    check(!meta.loadedAddresses || (meta.loadedAddresses.readonly?.length === 0 && meta.loadedAddresses.writable?.length === 0), 'Solana loaded addresses unsupported');
    uint(meta.fee); windowAndFee(p, tx.blockTime, BigInt(meta.fee));
    check(Array.isArray(meta.preBalances) && Array.isArray(meta.postBalances) && meta.preBalances.length === keys.length && meta.postBalances.length === keys.length, 'Solana execution balances missing');
    keys.forEach((key, i) => {
      const before = BigInt(uint(meta.preBalances[i])), after = BigInt(uint(meta.postBalances[i]));
      const delta = key.pubkey === p.authorization.agentIdentity.id ? -BigInt(p.request.amountAtomic) - BigInt(meta.fee) : key.pubkey === p.request.recipient ? BigInt(p.request.amountAtomic) : 0n;
      check(after - before === delta, 'Solana execution balance mismatch');
    });
    const statuses = await rpc(c, 'getSignatureStatuses', [[id], { searchTransactionHistory: true }]);
    check(statuses?.value?.length === 1 && statuses.value[0]?.slot === slot && statuses.value[0].err === null && statuses.value[0].confirmationStatus === 'finalized', 'Solana execution signature not finalized');
    const finalSlot = uint(await rpc(c, 'getSlot', [{ commitment: 'finalized' }]));
    const end = slot + p.authorization.settlement.minConfirmations - 1;
    check(Number.isSafeInteger(end) && end >= slot && end <= finalSlot && end - slot <= 1000, 'Solana execution finality range unsupported');
    const blocks = await rpc(c, 'getBlocks', [slot, end, { commitment: 'finalized' }]);
    check(Array.isArray(blocks) && blocks.length >= p.authorization.settlement.minConfirmations && new Set(blocks).size === blocks.length && blocks.every(s => Number.isSafeInteger(s) && s >= slot && s <= end), 'Solana execution confirmation threshold unmet');
    const blockParams = [slot, { commitment: 'finalized', transactionDetails: 'signatures', rewards: false }];
    const block = await rpc(c, 'getBlock', blockParams);
    base58(block?.blockhash, 32); uint(block.blockHeight);
    check(block.signatures?.filter(s => s === id).length === 1 && block.blockTime === tx.blockTime, 'Solana execution canonical inclusion mismatch');
    const programResponse = await rpc(c, 'getAccountInfo', [c.solanaProgram.programId, { encoding: 'base64', commitment: 'finalized', minContextSlot: slot }]);
    check(uint(programResponse?.context?.slot) >= slot, 'Solana executable account snapshot stale');
    const program = accountBytes(programResponse.value);
    check(program.length === 36 && program.readUInt32LE(0) === 2, 'Solana UpgradeableLoader program required');
    const programDataAddress = b58String(program.subarray(4));
    const programIdentity = await solanaProgramSnapshot(c, p, programDataAddress, slot);
    const ownerApproval = await solanaOwnerApproval(c, p, slot, programIdentity.deploymentSlot);
    check((await rpc(c, 'getBlock', blockParams))?.blockhash === block.blockhash, 'Solana execution reorg detected');
    check(sameData(await solanaProgramSnapshot(c, p, programDataAddress, slot), programIdentity), 'Solana executable changed during inspection');
    return freeze({ state: 'OBSERVED_SOLANA_EXECUTION_NOT_SETTLED', authorizationCommitment: p.authorizationCommitment,
      observation: { transactionId: id, inclusionReference: block.blockhash, finality: 'RPC_FINALIZED', recipient: p.request.recipient, amountAtomic: p.request.amountAtomic, slot },
      actualFeeAtomic: String(meta.fee), programIdentity, ownerApproval, feeBudgetVerifiedByRpc: true, feeBudgetEnforcedOnChain: false,
      trust: 'OWNER_ATTESTED_ENVELOPE_IMMUTABLE_PREEXECUTION_CODE_AND_OPERATOR_RPC_NOT_LIGHT_CLIENT' });
  } catch (error) { return Object.freeze({ state: 'NOT_VERIFIED', reason: error instanceof Error ? error.message : 'Solana execution inspection failed' }); }
}

export async function verifySolanaExecutionSettlement(input = {}) {
  try {
    exact(input, ['adapter', 'prepared', 'transactionId']);
    const { adapter, prepared, transactionId } = input;
    const observed = await inspectSolanaExecutionTransaction({ adapter, prepared, transactionId });
    if (observed.state !== 'OBSERVED_SOLANA_EXECUTION_NOT_SETTLED') return observed;
    return nativeSettlementReceipt(prepared, adapter, observed);
  } catch (error) { return Object.freeze({ state: 'NOT_VERIFIED', reason: error instanceof Error ? error.message : 'Solana settlement inspection failed' }); }
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

function nativeSettlementReceipt(p, adapter, result) {
  const c = config(adapter);
  const receipt = Object.freeze({ kind: 'QPC_NATIVE_SETTLEMENT_RECEIPT_V1' });
  const verifiedAtMs = Date.now();
  const metadata = freeze({ state: 'NATIVE_SETTLEMENT_VERIFIED', chain: c.chain, networkId: c.networkId, nativeChainId: c.nativeChainId,
    sourceNetworkId: p.authorization.source.networkId, sourceNativeChainId: p.authorization.source.nativeChainId,
    destinationNetworkId: p.authorization.destination.networkId, destinationNativeChainId: p.authorization.destination.nativeChainId,
    verifiedAtUnix: Math.floor(verifiedAtMs / 1000),
    intentId: p.authorization.intentId, authorization: p.authorization,
    authorizationCommitment: computeCrossL1AuthorizationCommitment(p.authorization),
    transactionId: result.observation.transactionId, inclusionReference: result.observation.inclusionReference,
    recipient: p.request.recipient, amountAtomic: p.request.amountAtomic, actualFeeAtomic: result.actualFeeAtomic,
    trust: result.trust,
    verifierIdentity: { chain: c.chain, networkId: c.networkId, nativeChainId: c.nativeChainId,
      contractAddress: c.chain === 'near' ? c.nearContract.accountId : c.chain === 'solana' ? c.solanaProgram.programId : c.evmContract.address,
      codeHash: c.chain === 'near' ? c.nearContract.codeHash : c.chain === 'solana' ? c.solanaProgram.codeHashSha256 : c.evmContract.codeHashSha256,
      owner: c.chain === 'near' ? p.authorization.cancellation.authorityId : c.chain === 'solana' ? c.solanaProgram.owner : c.evmContract.owner },
  });
  verifiedSettlements.set(receipt, { metadata, verifiedAtMs });
  return freeze({ state: 'NATIVE_SETTLEMENT_VERIFIED', receipt, verification: metadata });
}

/** The handle cannot be serialized, copied, or manufactured by a caller. */
export function readVerifiedNativeSettlement(receipt) {
  const entry = verifiedSettlements.get(receipt), now = Date.now();
  return entry && now >= entry.verifiedAtMs && now - entry.verifiedAtMs <= 60_000 ? entry.metadata : null;
}

/** Only the fully bound native execution path may mint an in-process receipt. */
export async function verifyNearExecutionSettlement(input = {}) {
  try {
    exact(input, ['adapter', 'prepared', 'transactionId']);
    const { adapter, prepared, transactionId } = input;
    const observed = await inspectNearExecutionTransaction({ adapter, prepared, transactionId });
    if (observed.state !== 'OBSERVED_NEAR_EXECUTION_NOT_SETTLED') return observed;
    return nativeSettlementReceipt(prepared, adapter, observed);
  } catch (error) { return Object.freeze({ state: 'NOT_VERIFIED', reason: error instanceof Error ? error.message : 'NEAR settlement inspection failed' }); }
}

/** Never treat booleans, callbacks, a hash, or a plain transfer as envelope proof. */
export async function verifyTestnetSettlement(input = {}) {
  const prepared = input && typeof input === 'object' ? Object.getOwnPropertyDescriptor(input, 'prepared')?.value : undefined;
  if (executions.has(prepared)) return verifyNearExecutionSettlement(input);
  if (evmExecutions.has(prepared)) return verifyEvmExecutionSettlement(input);
  if (solanaExecutions.has(prepared)) return verifySolanaExecutionSettlement(input);
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
