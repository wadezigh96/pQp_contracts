import test from 'node:test';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { FileIntentStore } from '../src/intent-store.mjs';
import assert from 'node:assert/strict';
import {
  createTestnetAdapter, prepareTestnetTransfer, inspectTestnetTransaction,
  verifyTestnetSettlement, inspectZcashTestnet, inspectTestnetNetwork, TESTNET_NETWORKS,
  inspectNearAuthorizationTransaction, prepareNearExecution, inspectNearExecutionTransaction,
  verifyNearExecutionSettlement, readVerifiedNativeSettlement,
  prepareEvmExecution, inspectEvmExecutionTransaction, verifyEvmExecutionSettlement,
} from '../src/testnet-adapters.mjs';
import { computeNativeRecipientCommitment } from '../src/native-adapters.mjs';
import { buildNearEnvelopeAuthorizationDispatch } from '../src/near-envelope-dispatch.mjs';

const NOW = 1800000000;
const ADDRESS = `0x${'1'.repeat(40)}`;
const TX = `0x${'a'.repeat(64)}`, BLOCK = `0x${'b'.repeat(64)}`;
const KEY = '11111111111111111111111111111111';
const PAYER = 'Vote111111111111111111111111111111111111111';
const RECIPIENT = 'So11111111111111111111111111111111111111112';
const SIGNATURE = '1'.repeat(64);
const NEAR_ID = '1'.repeat(32);
function fixture(chain = 'ethereum') {
  const { networkId, nativeChainId } = TESTNET_NETWORKS[chain];
  const decimals = { near: 24, ethereum: 18, bsc: 18, solana: 9, zcash: 8 }[chain];
  const recipient = chain === 'near' ? 'bob.testnet' : chain === 'solana' ? RECIPIENT : chain === 'zcash' ? 'utest1unsupported' : ADDRESS;
  const asset = `${chain}:native`;
  const authorization = {
    schema: 'qpc.cross-l1.authorization', version: 1, intentId: 'testnet-1',
    agentIdentity: { namespace: 'qpc', id: 'agent-1', keyId: 'key-1' },
    source: { networkId: 'near:testnet', nativeChainId: 'testnet', accountId: 'agent.testnet', adapterId: 'near-native-v1' },
    destination: { networkId, nativeChainId, adapterId: `${chain}-native-v1`, recipientCommitment: computeNativeRecipientCommitment({ chain, nativeChainId, recipient }) },
    action: { type: 'asset-transfer', operation: 'pay' },
    asset: { sourceAssetId: 'near:native', destinationAssetId: asset, sourceDecimals: 24, destinationDecimals: decimals, mappingId: 'testnet-map' },
    amount: { sourceAtomic: '10', destinationAtomic: '7' },
    fees: { maxFeeAtomic: '10000', feeAssetId: asset, maxSlippageBps: 0 }, nonce: '1',
    replayDomain: { authorityId: 'router', scope: 'native' }, policy: { id: 'policy', version: 1, commitment: 'a'.repeat(64) },
    time: { issuedAtUnix: NOW, notBeforeUnix: NOW, expiresAtUnix: NOW + 100 },
    settlement: { evidenceClass: `${chain}-native-v1`, finalityPolicy: { near: 'near-final', ethereum: 'ethereum-finalized', bsc: 'bsc-finalized', solana: 'solana-finalized', zcash: 'zcash-confirmations' }[chain], minConfirmations: 2 },
    cancellation: { authorityId: 'owner', revocationEpoch: 0 },
  };
  const trustedProfile = {
    sourceNetworkId: 'near:testnet', sourceNativeChainId: 'testnet', destinationNetworkId: networkId, destinationNativeChainId: nativeChainId,
    mappingId: 'testnet-map', sourceAssetId: 'near:native', destinationAssetId: asset,
    sourceDecimals: 24, destinationDecimals: decimals, rateNumerator: '7', rateDenominator: '10',
    ...(chain === 'solana' ? { expectedGenesisHash: nativeChainId } : {}),
  };
  let request = { nativeChainId, recipient, amountAtomic: '7', maxFeeAtomic: '10000' };
  if (chain === 'ethereum' || chain === 'bsc') request.calldata = '0x';
  if (chain === 'near') request.actions = [{ type: 'Transfer', deposit: '7' }];
  if (chain === 'solana') request = {
    ...request, genesisHash: nativeChainId, recentBlockhash: KEY, blockHeight: 90, lastValidBlockHeight: 150, feePayer: PAYER, signers: [PAYER],
    instructions: [{ programId: KEY, type: 'transfer', lamports: '7', accounts: [
      { pubkey: PAYER, isSigner: true, isWritable: true }, { pubkey: recipient, isSigner: false, isWritable: true },
    ] }],
  };
  const adapter = createTestnetAdapter({ chain, trustedProfile });
  return { adapter, authorization, request, trustedProfile, nowUnix: NOW, chain };
}
function preparedFixture(chain) {
  const f = fixture(chain);
  return { ...f, prepared: prepareTestnetTransfer(f) };
}
// Tests control the HTTP transport only. The public adapter never accepts an RPC
// callback, proof callback, evidence object or arbitrary endpoint from a request.
async function withRpc(handler, run) {
  const original = globalThis.fetch, calls = [];
  globalThis.fetch = async (url, options) => {
    const body = JSON.parse(options.body);
    assert.equal(options.redirect, 'error');
    assert.equal(options.method, 'POST');
    calls.push({ url, method: body.method, params: body.params });
    return new Response(JSON.stringify({ jsonrpc: '2.0', id: 1, result: await handler(body.method, body.params, calls) }), { status: 200 });
  };
  try { return await run(calls); } finally { globalThis.fetch = original; }
}
function evmRpc(chain = 'ethereum') {
  const id = TESTNET_NETWORKS[chain].nativeChainId;
  const state = {
    tx: { type: '0x2', hash: TX, chainId: `0x${BigInt(id).toString(16)}`, to: ADDRESS, value: '0x7', input: '0x', blockHash: BLOCK, blockNumber: '0x64' },
    receipt: { type: '0x2', transactionHash: TX, to: ADDRESS, blockHash: BLOCK, blockNumber: '0x64', status: '0x1', gasUsed: '0x1', effectiveGasPrice: '0x1' },
    block: { hash: BLOCK, number: '0x64', timestamp: `0x${NOW.toString(16)}`, transactions: [TX] },
    final: { hash: BLOCK, number: '0x65' },
  };
  state.handler = async (method, params) => {
    if (method === 'eth_chainId') return `0x${BigInt(id).toString(16)}`;
    if (method === 'eth_getTransactionByHash') return state.tx;
    if (method === 'eth_getTransactionReceipt') return state.receipt;
    if (method === 'eth_getBlockByNumber') return params[0] === '0x0' ? { number: '0x0', hash: TESTNET_NETWORKS[chain].expectedGenesisHash } : params[0] === 'finalized' ? state.final : state.block;
    throw Error(`unexpected method ${method}`);
  };
  return state;
}
for (const chain of ['ethereum', 'bsc']) {
  test(`${chain}: fixed testnet RPC validates a native transfer but NEVER settles it`, async () => {
    const f = preparedFixture(chain), rpc = evmRpc(chain);
    await withRpc(rpc.handler, async calls => {
      const result = await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: TX, authorization: f.authorization });
      assert.equal(result.state, 'NOT_IMPLEMENTED');
      assert.equal(result.observation.state, 'OBSERVED_NATIVE_TRANSFER_NOT_SETTLED');
      assert.match(result.reason, /policy authorization, nonce, replay consumption, expiry/);
      assert.equal(calls.length, 7);
      assert(calls.every(c => c.url === TESTNET_NETWORKS[chain].rpcUrl));
    });
  });
  for (const [name, mutate] of [
    ['hash', s => { s.tx.hash = `0x${'c'.repeat(64)}`; }],
    ['receipt hash', s => { s.receipt.transactionHash = `0x${'c'.repeat(64)}`; }],
    ['recipient', s => { s.tx.to = `0x${'2'.repeat(40)}`; }],
    ['receipt recipient', s => { s.receipt.to = `0x${'2'.repeat(40)}`; }],
    ['amount', s => { s.tx.value = '0x8'; }],
    ['chain', s => { s.tx.chainId = '0x1'; }],
    ['failed status', s => { s.receipt.status = '0x0'; }],
    ['missing transaction', s => { s.tx = null; }],
    ['noncanonical block', s => { s.block.hash = `0x${'c'.repeat(64)}`; }],
    ['missing inclusion', s => { s.block.transactions = []; }],
    ['finality', s => { s.final.number = '0x63'; }],
    ['confirmations', s => { s.final.number = '0x64'; }],
    ['expiry', s => { s.block.timestamp = `0x${(NOW + 100).toString(16)}`; }],
    ['fee', s => { s.receipt.gasUsed = '0xffff'; }],
    ['calldata', s => { s.tx.input = '0xaabb'; }],
  ]) test(`${chain}: rejects native RPC ${name} forgery`, async () => {
    const f = preparedFixture(chain), state = evmRpc(chain); mutate(state);
    await withRpc(state.handler, async () => assert.equal((await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: TX })).state, 'NOT_VERIFIED'));
  });
  for (const type of ['0x0', '0x1', '0x2']) test(`${chain}: permits only supported gas-only type ${type} observations`, async () => {
    const f = preparedFixture(chain), state = evmRpc(chain);
    state.tx.type = type; state.receipt.type = type;
    await withRpc(state.handler, async () => assert.equal((await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: TX })).state, 'NOT_IMPLEMENTED'));
  });
  for (const [name, mutate] of [
    ['blob transaction type', s => { s.tx.type = '0x3'; s.receipt.type = '0x3'; }],
    ['delegation transaction type', s => { s.tx.type = '0x4'; s.receipt.type = '0x4'; }],
    ['unknown transaction type', s => { s.tx.type = '0x7e'; }],
    ['missing transaction type', s => { delete s.tx.type; }],
    ['receipt type substitution', s => { s.receipt.type = '0x0'; }],
    ['blob hashes', s => { s.tx.blobVersionedHashes = [BLOCK]; }],
    ['blob fee cap', s => { s.tx.maxFeePerBlobGas = '0xffff'; }],
    ['blob receipt gas', s => { s.receipt.blobGasUsed = '0xffff'; }],
    ['blob receipt price', s => { s.receipt.blobGasPrice = '0xffff'; }],
    ['delegation authorization', s => { s.tx.authorizationList = [{ chainId: s.tx.chainId, address: ADDRESS, nonce: '0x1' }]; }],
  ]) test(`${chain}: rejects unsupported ${name}`, async () => {
    const f = preparedFixture(chain), state = evmRpc(chain); mutate(state);
    await withRpc(state.handler, async () => {
      const result = await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: TX });
      assert.equal(result.state, 'NOT_VERIFIED');
      assert.match(result.reason, /unsupported native EVM/);
    });
  });
  test(`${chain}: rejects mid-inspection reorg and wrong RPC chain`, async () => {
    const f = preparedFixture(chain), s = evmRpc(chain); let reads = 0;
    await withRpc(async (method, params) => method === 'eth_getBlockByNumber' && params[0] === '0x64' && ++reads > 1 ? { ...s.block, hash: `0x${'c'.repeat(64)}` } : s.handler(method, params), async () => {
      assert.match((await inspectTestnetTransaction({ adapter: f.adapter, prepared: f.prepared, transactionId: TX })).reason, /reorg/);
    });
    await withRpc(async (method, params) => method === 'eth_chainId' ? '0x1' : s.handler(method, params), async () => {
      assert.match((await inspectTestnetTransaction({ adapter: f.adapter, prepared: f.prepared, transactionId: TX })).reason, /chain mismatch/);
    });
  });
}

test('caller booleans, evidence and callbacks are rejected without invoking them or RPC', async () => {
  const f = preparedFixture(); let invoked = false;
  await withRpc(() => { throw Error('must not call RPC'); }, async () => {
    for (const extra of [{ verified: true }, { success: true, canonical: true }, { evidence: { valid: true } }, { verifyDestinationEvidence() { invoked = true; return true; } }, { fetch() { invoked = true; } }, { rpcUrl: 'https://attacker.invalid' }]) {
      assert.equal((await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: TX, ...extra })).state, 'NOT_VERIFIED');
    }
    assert.equal((await verifyTestnetSettlement({ adapter: f.adapter, prepared: { ...f.prepared }, transactionId: TX })).state, 'NOT_VERIFIED');
    assert.equal((await verifyTestnetSettlement({ adapter: { ...f.adapter }, prepared: f.prepared, transactionId: TX })).state, 'NOT_VERIFIED');
    assert.equal(invoked, false);
  });
});
test('exact authorization and operator-owned preparation binding cannot be replaced or reused across adapters', async () => {
  const f = preparedFixture(), other = preparedFixture();
  const authorization = structuredClone(f.authorization); authorization.nonce = '2';
  await withRpc(() => { throw Error('must not call RPC'); }, async () => {
    for (const replacement of [authorization, { ...f.authorization, policy: { ...f.authorization.policy, version: 2 } }, { ...f.authorization, replayDomain: { ...f.authorization.replayDomain, scope: 'other' } }, { ...f.authorization, time: { ...f.authorization.time, expiresAtUnix: NOW + 200 } }]) {
      assert.match((await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: TX, authorization: replacement })).reason, /authorization mismatch/);
    }
    assert.match((await verifyTestnetSettlement({ adapter: other.adapter, prepared: f.prepared, transactionId: TX })).reason, /preparation handle/);
  });
});
test('factory blocks request-selected endpoints, mainnet profiles and verifier injection', () => {
  const f = fixture();
  for (const rpcUrl of ['https://attacker.invalid', 'http://localhost:1234', `${TESTNET_NETWORKS.ethereum.rpcUrl}/other`, 'https://user:password@ethereum-sepolia-rpc.publicnode.com']) assert.throws(() => createTestnetAdapter({ chain: 'ethereum', trustedProfile: f.trustedProfile, rpcUrl }), /allowlisted/);
  assert.throws(() => createTestnetAdapter({ chain: 'ethereum', trustedProfile: { ...f.trustedProfile, destinationNativeChainId: '1' } }), /testnet/);
  assert.throws(() => createTestnetAdapter({ chain: 'ethereum', trustedProfile: f.trustedProfile, verify: () => true }), /data/);
  const profile = structuredClone(f.trustedProfile); const adapter = createTestnetAdapter({ chain: 'ethereum', trustedProfile: profile });
  profile.rateNumerator = '999';
  assert.equal(prepareTestnetTransfer({ ...f, adapter }).state, 'PREPARED_UNSIGNED');
  for (const change of [f => { f.request.amountAtomic = '8'; }, f => { f.request.recipient = `0x${'2'.repeat(40)}`; }, f => { f.nowUnix = NOW + 100; }, f => { f.authorization.destination.networkId = 'eip155:1'; }]) {
    const input = fixture(); change(input); assert.throws(() => prepareTestnetTransfer(input));
  }
});
function nearRpc() {
  const receipt = '4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY', receiptBlock = PAYER;
  const outcome = (id, status, receipt_ids, block_hash) => ({ id, block_hash, outcome: { status, receipt_ids, tokens_burnt: '1' } });
  const state = {
    result: { transaction: { hash: NEAR_ID, signer_id: 'alice.testnet', receiver_id: 'bob.testnet', actions: [{ Transfer: { deposit: '7' } }] }, final_execution_status: 'FINAL', status: { SuccessValue: '' },
      transaction_outcome: outcome(NEAR_ID, { SuccessReceiptId: receipt }, [receipt], KEY), receipts_outcome: [outcome(receipt, { SuccessValue: '' }, [], receiptBlock)] },
    block: { header: { hash: KEY, height: 100, timestamp_nanosec: `${NOW}000000000` } },
    receiptBlock: { header: { hash: receiptBlock, height: 101, timestamp_nanosec: `${NOW}000000000` } },
    final: { header: { hash: KEY, height: 102 } },
  };
  state.handler = async (method, params) => {
    if (method === 'status') return { chain_id: 'testnet', genesis_hash: TESTNET_NETWORKS.near.expectedGenesisHash };
    if (method === 'tx') return state.result;
    if (method === 'block') return params.finality ? state.final : [100, KEY].includes(params.block_id) ? state.block : state.receiptBlock;
    throw Error(`unexpected method ${method}`);
  };
  return state;
}
test('NEAR independently inspects finalized transaction and every receipt, without settling', async () => {
  const f = preparedFixture('near'), s = nearRpc();
  await withRpc(s.handler, async () => {
    const result = await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: NEAR_ID, signerAccountId: 'alice.testnet' });
    assert.equal(result.state, 'NOT_IMPLEMENTED', result.reason);
    assert.equal(result.observation.observation.amountAtomic, '7');
  });
});
for (const [name, mutate] of [
  ['hash', s => { s.result.transaction.hash = KEY + '1'; }],
  ['recipient', s => { s.result.transaction.receiver_id = 'mallory.testnet'; }],
  ['amount', s => { s.result.transaction.actions[0].Transfer.deposit = '8'; }],
  ['actions', s => { s.result.transaction.actions.push({ FunctionCall: {} }); }],
  ['failed receipt', s => { s.result.receipts_outcome[0].outcome.status = { Failure: {} }; }],
  ['missing receipt', s => { s.result.receipts_outcome = []; }],
  ['unresolved receipt', s => { s.result.receipts_outcome[0].outcome.receipt_ids = ['missing']; }],
  ['cycle', s => { s.result.receipts_outcome[0].outcome.receipt_ids = [NEAR_ID]; }],
  ['disconnected receipt', s => { s.result.transaction_outcome.outcome.receipt_ids = []; }],
  ['transaction outcome ID', s => { s.result.transaction_outcome.id = PAYER; }],
  ['finality', s => { s.final.header.height = 100; }],
  ['block substitution', s => { s.receiptBlock.header.hash = KEY; }],
]) test(`NEAR rejects ${name}`, async () => {
  const f = preparedFixture('near'), s = nearRpc(); mutate(s);
  await withRpc(s.handler, async () => assert.equal((await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: NEAR_ID, signerAccountId: 'alice.testnet' })).state, 'NOT_VERIFIED'));
});
test('NEAR rejects wrong network and canonical-height reorg', async () => {
  const f = preparedFixture('near'), s = nearRpc();
  for (const handler of [async (m, p) => m === 'status' ? { chain_id: 'mainnet' } : s.handler(m, p), async (m, p) => m === 'block' && p.block_id === 100 ? { header: { ...s.block.header, hash: '2'.repeat(32) } } : s.handler(m, p)]) {
    await withRpc(handler, async () => assert.equal((await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: NEAR_ID, signerAccountId: 'alice.testnet' })).state, 'NOT_VERIFIED'));
  }
});
function nearAuthorizationFixture() {
  const f = fixture('near');
  f.authorization.agentIdentity = { namespace: 'near', id: 'agent.testnet', keyId: 'agent.testnet' };
  f.authorization.source.adapterId = 'contract.testnet';
  f.authorization.replayDomain.authorityId = 'contract.testnet';
  f.adapter = createTestnetAdapter({ chain: 'near', trustedProfile: f.trustedProfile, nearContract: { accountId: 'contract.testnet', codeHash: KEY } });
  f.prepared = prepareTestnetTransfer(f);
  return f;
}
function nearAuthorizationRpc(f) {
  const s = nearRpc(), p = f.prepared;
  const e = p.authorization;
  const expected = { state: 'AUTHORIZED', executionStatus: 'NOT_EXECUTED', intentId: e.intentId, authorizationCommitment: p.authorizationCommitment,
    agentId: e.source.accountId, adapterId: 'contract.testnet', nonce: e.nonce, policyId: e.policy.id, policyVersion: e.policy.version,
    policyCommitment: e.policy.commitment, revocationEpoch: e.cancellation.revocationEpoch, authorizedAtUnix: NOW, expiresAtUnix: e.time.expiresAtUnix, blockHeight: 101 };
  s.authorizationRecord = expected;
  const success = { SuccessValue: Buffer.from(JSON.stringify(expected)).toString('base64') };
  s.result.transaction.signer_id = 'agent.testnet';
  s.result.transaction.receiver_id = 'contract.testnet';
  s.result.transaction.actions = [{ FunctionCall: { method_name: 'authorize_envelope', args: Buffer.from(JSON.stringify(buildNearEnvelopeAuthorizationDispatch({ envelope: p.authorization, nowUnix: NOW }).qpcCall.args)).toString('base64'), deposit: '0', gas: 100000000000000 } }];
  s.result.status = success;
  s.result.transaction_outcome.outcome.executor_id = 'agent.testnet';
  const root = s.result.receipts_outcome[0];
  root.outcome.executor_id = 'contract.testnet'; root.outcome.status = success;
  root.outcome.logs = [`EVENT_JSON:${JSON.stringify({ standard: 'qpc', version: '1.0.0', event: 'agent_action_authorized', data: [expected] })}`];
  s.block.header.prev_hash = KEY; s.receiptBlock.header.prev_hash = KEY;
  const original = s.handler;
  s.handler = (m, p) => {
    if (m === 'query' && p.request_type !== 'view_account') assert.deepEqual(JSON.parse(Buffer.from(p.args_base64, 'base64')), { agent_id: f.authorization.agentIdentity.id, intent_id: f.authorization.intentId });
    return m === 'query' ? p.request_type === 'view_account' ? { block_hash: p.block_id, code_hash: KEY } : { block_hash: p.block_id, block_height: s.final.header.height, result: [...Buffer.from(JSON.stringify(s.authorizationRecord))] } : original(m, p);
  };
  return s;
}
test('NEAR authorization: independently binds exact call, predecessor, contract event and pinned historical code, NOT settlement', async () => {
  const f = nearAuthorizationFixture(), s = nearAuthorizationRpc(f);
  await withRpc(s.handler, async calls => {
    const result = await inspectNearAuthorizationTransaction({ adapter: f.adapter, prepared: f.prepared, transactionId: NEAR_ID });
    assert.equal(result.state, 'OBSERVED_NATIVE_AUTHORIZATION_NOT_SETTLED', result.reason);
    assert.equal(result.authorizationCommitment, f.prepared.authorizationCommitment);
    assert(calls.some(c => c.method === 'query' && c.params.request_type === 'view_account'));
  });
});
for (const [name, mutate] of [
  ['signer', s => { s.result.transaction.signer_id = 'mallory.testnet'; }],
  ['contract', s => { s.result.transaction.receiver_id = 'mallory.testnet'; }],
  ['method', s => { s.result.transaction.actions[0].FunctionCall.method_name = 'execute_near_envelope'; }],
  ['deposit', s => { s.result.transaction.actions[0].FunctionCall.deposit = '1'; }],
  ['args', s => { s.result.transaction.actions[0].FunctionCall.args = Buffer.from('{}').toString('base64'); }],
  ['executor', s => { s.result.receipts_outcome[0].outcome.executor_id = 'mallory.testnet'; }],
  ['event', s => { s.result.receipts_outcome[0].outcome.logs = []; }],
  ['return', s => { s.result.status = { SuccessValue: Buffer.from('{}').toString('base64') }; }],
  ['duplicate event', s => { s.result.receipts_outcome[0].outcome.logs.push(s.result.receipts_outcome[0].outcome.logs[0]); }],
]) test(`NEAR authorization rejects ${name} substitution`, async () => {
  const f = nearAuthorizationFixture(), s = nearAuthorizationRpc(f); mutate(s);
  await withRpc(s.handler, async () => assert.equal((await inspectNearAuthorizationTransaction({ adapter: f.adapter, prepared: f.prepared, transactionId: NEAR_ID })).state, 'NOT_VERIFIED'));
});
test('NEAR authorization rejects unpinned code and forged preparation without consulting caller proof', async () => {
  const f = nearAuthorizationFixture(), s = nearAuthorizationRpc(f);
  await withRpc((m, p) => m === 'query' ? { block_hash: p.block_id, code_hash: PAYER } : s.handler(m, p), async () => {
    assert.match((await inspectNearAuthorizationTransaction({ adapter: f.adapter, prepared: f.prepared, transactionId: NEAR_ID })).reason, /code mismatch/);
  });
  await withRpc(() => { throw Error('must not call RPC'); }, async () => {
    assert.equal((await inspectNearAuthorizationTransaction({ adapter: f.adapter, prepared: { ...f.prepared }, transactionId: NEAR_ID })).state, 'NOT_VERIFIED');
    assert.equal((await inspectNearAuthorizationTransaction({ adapter: f.adapter, prepared: f.prepared, transactionId: NEAR_ID, verified: true })).state, 'NOT_VERIFIED');
  });
});
function nearExecutionFixture() {
  const f = nearAuthorizationFixture(), e = f.authorization;
  e.destination.adapterId = 'contract.testnet'; e.asset.mappingId = 'near-native-identity-v1'; e.amount.sourceAtomic = e.amount.destinationAtomic;
  e.settlement = { evidenceClass: 'near-native-transfer-result', finalityPolicy: 'near-execution-outcome', minConfirmations: 0 };
  f.prepared = prepareNearExecution({ adapter: f.adapter, authorization: e, recipient: 'bob.testnet', nowUnix: NOW });
  return f;
}
test('NEAR execution preparation is exact, unsigned, same-chain only and deeply frozen', () => {
  const f = nearExecutionFixture(), p = f.prepared;
  assert.equal(p.state, 'PREPARED_NEAR_EXECUTION_UNSIGNED');
  assert.equal(p.qpcCall.methodName, 'execute_near_envelope');
  assert.equal(p.qpcCall.deposit, '7');
  assert.equal(p.qpcCall.args.authorization_commitment, p.authorizationCommitment);
  assert(Object.isFrozen(p.qpcCall.args.envelope));
  for (const mutate of [e => { e.source.adapterId = 'mallory.testnet'; }, e => { e.agentIdentity.id = 'mallory.testnet'; }, e => { e.destination.networkId = 'near:mainnet'; }, e => { e.amount.sourceAtomic = '8'; }, e => { e.asset.mappingId = 'other'; }, e => { e.destination.recipientCommitment = 'a'.repeat(64); }, e => { e.settlement.evidenceClass = 'near-native-v1'; }]) {
    const authorization = structuredClone(f.authorization); mutate(authorization);
    assert.throws(() => prepareNearExecution({ adapter: f.adapter, authorization, recipient: 'bob.testnet', nowUnix: NOW }));
  }
  assert.throws(() => prepareNearExecution({ adapter: f.adapter, authorization: f.authorization, recipient: 'bob.testnet', nowUnix: NOW + 100 }), /time window/);
});

function nearExecutionRpc(f) {
  // Reuse the exact authorization event but execute the dedicated attached-value
  // method, with a successful transfer and data-dependent private callback.
  const p = f.prepared;
  const authPrepared = { ...p, authorization: { ...p.authorization } };
  const s = nearAuthorizationRpc({ ...f, prepared: authPrepared });
  const root = s.result.receipts_outcome[0];
  const record = { state: 'EXECUTED', intentId: p.authorization.intentId, authorizationCommitment: p.authorizationCommitment, recipient: 'bob.testnet', amountAtomic: '7', authorizedAtUnix: NOW, completedAtUnix: NOW, contractFeeAtomic: '0', protocolGasFee: 'NOT_VERIFIED', feeBudgetEnforced: false, refundState: 'NOT_APPLICABLE', refundAttempt: 0 };
  s.executionRecord = record;
  const encoded = Buffer.from(JSON.stringify(record)).toString('base64');
  s.result.transaction.actions = [{ FunctionCall: { method_name: 'execute_near_envelope', args: Buffer.from(JSON.stringify(p.qpcCall.args)).toString('base64'), deposit: '7', gas: Number(p.qpcCall.gas) } }];
  s.result.status = { SuccessValue: encoded };
  root.outcome.status = { SuccessReceiptId: PAYER }; root.outcome.receipt_ids = [RECIPIENT, PAYER];
  s.result.receipts_outcome.push(
    { id: RECIPIENT, block_hash: PAYER, outcome: { executor_id: 'bob.testnet', status: { SuccessValue: '' }, receipt_ids: [], tokens_burnt: '1', logs: [] } },
    { id: PAYER, block_hash: PAYER, outcome: { executor_id: 'contract.testnet', status: { SuccessValue: encoded }, receipt_ids: [], tokens_burnt: '1', logs: [`EVENT_JSON:${JSON.stringify({ standard: 'qpc', version: '1.0.0', event: 'near_transfer_result', data: [record] })}`] } },
  );
  s.transferReceipt = { receipt_id: RECIPIENT, predecessor_id: 'contract.testnet', receiver_id: 'bob.testnet', receipt: { Action: { signer_id: 'agent.testnet', actions: [{ Transfer: { deposit: '7' } }], output_data_receivers: [{ data_id: KEY, receiver_id: 'contract.testnet' }] } } };
  s.callbackReceipt = { receipt_id: PAYER, predecessor_id: 'contract.testnet', receiver_id: 'contract.testnet', receipt: { Action: { signer_id: 'agent.testnet', actions: [{ FunctionCall: { method_name: 'on_near_transfer_result', args: Buffer.from(JSON.stringify({ intent_id: p.authorization.intentId, authorization_commitment: p.authorizationCommitment })).toString('base64'), gas: 10000000000000, deposit: '0' } }], input_data_ids: [KEY] } } };
  const original = s.handler;
  s.handler = (m, q) => {
    if (m === 'query' && q.method_name === 'get_execution_result') assert.deepEqual(JSON.parse(Buffer.from(q.args_base64, 'base64')), { agent_id: f.authorization.agentIdentity.id, intent_id: f.authorization.intentId });
    return m === 'EXPERIMENTAL_receipt' ? q.receipt_id === RECIPIENT ? s.transferReceipt : s.callbackReceipt : m === 'query' && q.method_name === 'get_execution_result' ? { block_hash: q.block_id, block_height: s.final.header.height, result: [...Buffer.from(JSON.stringify(s.executionRecord))] } : original(m, q);
  };
  return s;
}
test('NEAR execution independently observes authorized transfer, callback dependency, immutable records, canonical finality and all-in fee', async () => {
  const f = nearExecutionFixture(), s = nearExecutionRpc(f);
  await withRpc(s.handler, async calls => {
    const result = await inspectNearExecutionTransaction({ adapter: f.adapter, prepared: f.prepared, transactionId: NEAR_ID });
    assert.equal(result.state, 'OBSERVED_NEAR_EXECUTION_NOT_SETTLED', result.reason);
    assert.equal(result.actualFeeAtomic, '4');
    assert.equal(result.feeBudgetVerifiedByRpc, true);
    assert.equal(result.feeBudgetEnforcedOnChain, false);
    assert(calls.some(c => c.method === 'EXPERIMENTAL_receipt'));
    assert.equal((await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: NEAR_ID })).state, 'NATIVE_SETTLEMENT_VERIFIED');
  });
});
for (const [name, mutate] of [
  ['transfer recipient', s => { s.transferReceipt.receiver_id = 'mallory.testnet'; }],
  ['transfer amount', s => { s.transferReceipt.receipt.Action.actions[0].Transfer.deposit = '8'; }],
  ['transfer predecessor', s => { s.transferReceipt.predecessor_id = 'mallory.testnet'; }],
  ['transfer failure', s => { s.result.receipts_outcome[1].outcome.status = { Failure: {} }; }],
  ['callback dependency', s => { s.callbackReceipt.receipt.Action.input_data_ids = [PAYER]; }],
  ['callback predecessor', s => { s.callbackReceipt.predecessor_id = 'mallory.testnet'; }],
  ['callback replay args', s => { s.callbackReceipt.receipt.Action.actions[0].FunctionCall.args = Buffer.from('{}').toString('base64'); }],
  ['callback missing', s => { s.result.receipts_outcome.pop(); }],
  ['callback record amount', s => { s.executionRecord.amountAtomic = '8'; }],
  ['failed stored record', s => { s.executionRecord.state = 'FAILED'; }],
  ['forged stored commitment', s => { s.executionRecord.authorizationCommitment = 'b'.repeat(64); }],
  ['unbounded fee', s => { s.result.receipts_outcome[2].outcome.tokens_burnt = '10001'; }],
  ['duplicate transfer', s => { s.result.receipts_outcome[2].outcome.executor_id = 'bob.testnet'; }],
  ['expired execution', s => { s.receiptBlock.header.timestamp_nanosec = `${NOW + 100}000000000`; }],
  ['persisted authorization changed', s => { s.authorizationRecord.nonce = '9'; }],
]) test(`NEAR execution rejects ${name}`, async () => {
  const f = nearExecutionFixture(), s = nearExecutionRpc(f); mutate(s);
  await withRpc(s.handler, async () => assert.equal((await inspectNearExecutionTransaction({ adapter: f.adapter, prepared: f.prepared, transactionId: NEAR_ID })).state, 'NOT_VERIFIED'));
});
test('NEAR execution rejects caller booleans, copied handles and network/code/reorg substitutions', async () => {
  const f = nearExecutionFixture(), s = nearExecutionRpc(f);
  for (const extra of [{ verified: true }, { evidence: s.executionRecord }, { prepared: { ...f.prepared } }]) {
    await withRpc(() => { throw Error('must not call RPC'); }, async () => assert.equal((await inspectNearExecutionTransaction({ adapter: f.adapter, prepared: f.prepared, transactionId: NEAR_ID, ...extra })).state, 'NOT_VERIFIED'));
  }
  let reads = 0;
  for (const handler of [
    (m, p) => m === 'status' ? { chain_id: 'mainnet' } : s.handler(m, p),
    (m, p) => m === 'query' && p.request_type === 'view_account' ? { block_hash: p.block_id, code_hash: PAYER } : s.handler(m, p),
    (m, p) => m === 'block' && p.block_id === 101 && ++reads > 4 ? { header: { ...s.receiptBlock.header, hash: KEY } } : s.handler(m, p),
  ]) await withRpc(handler, async () => assert.equal((await inspectNearExecutionTransaction({ adapter: f.adapter, prepared: f.prepared, transactionId: NEAR_ID })).state, 'NOT_VERIFIED'));
});

test('NEAR settlement mints only fresh process-owned fully bound receipt; metadata, clones and booleans cannot become proof', async () => {
  const f = nearExecutionFixture(), s = nearExecutionRpc(f);
  await withRpc(s.handler, async () => {
    const result = await verifyNearExecutionSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: NEAR_ID });
    assert.equal(result.state, 'NATIVE_SETTLEMENT_VERIFIED', result.reason);
    const record = readVerifiedNativeSettlement(result.receipt);
    assert.equal(record.authorizationCommitment, f.prepared.authorizationCommitment);
    assert.equal(record.sourceNetworkId, 'near:testnet');
    assert.equal(record.destinationNetworkId, record.sourceNetworkId);
    assert.equal(record.actualFeeAtomic, '4');
    assert(Object.isFrozen(record.authorization.policy));
    for (const forged of [true, record, { ...result.receipt }, structuredClone(result.receipt), JSON.parse(JSON.stringify(result.receipt)), { kind: 'QPC_NATIVE_SETTLEMENT_RECEIPT_V1' }]) assert.equal(readVerifiedNativeSettlement(forged), null);
    const originalNow = Date.now;
    try {
      Date.now = () => (record.verifiedAtUnix + 62) * 1000;
      assert.equal(readVerifiedNativeSettlement(result.receipt), null);
      Date.now = () => (record.verifiedAtUnix - 1) * 1000;
      assert.equal(readVerifiedNativeSettlement(result.receipt), null);
    } finally { Date.now = originalNow; }
  });
});

const EVM_AGENT = `0x${'2'.repeat(40)}`, EVM_EXECUTOR = `0x${'3'.repeat(40)}`, EVM_OWNER = `0x${'4'.repeat(40)}`;
const RUNTIME = '0x60006000';
function evmExecutionFixture(chain = 'ethereum') {
  const f = fixture(chain), e = f.authorization;
  e.agentIdentity = { namespace: 'eip155', id: EVM_AGENT, keyId: EVM_AGENT };
  e.source = { networkId: e.destination.networkId, nativeChainId: e.destination.nativeChainId, accountId: EVM_AGENT, adapterId: EVM_EXECUTOR };
  e.destination.adapterId = EVM_EXECUTOR;
  e.asset = { sourceAssetId: `${chain}:native`, destinationAssetId: `${chain}:native`, sourceDecimals: 18, destinationDecimals: 18, mappingId: `${chain}-native-identity-v1` };
  e.amount.sourceAtomic = '7'; e.replayDomain = { authorityId: EVM_EXECUTOR, scope: 'native-payment' };
  e.cancellation = { authorityId: EVM_OWNER, revocationEpoch: 1 };
  e.settlement.evidenceClass = 'evm-owner-attested-native-v1'; e.settlement.finalityPolicy = `${chain}-finalized`;
  f.adapter = createTestnetAdapter({ chain, trustedProfile: f.trustedProfile, evmContract: { address: EVM_EXECUTOR, owner: EVM_OWNER, codeHashSha256: createHash('sha256').update(Buffer.from(RUNTIME.slice(2), 'hex')).digest('hex') } });
  f.prepared = prepareEvmExecution({ adapter: f.adapter, authorization: e, recipient: ADDRESS, nowUnix: NOW });
  return f;
}
const word = value => typeof value === 'string' && value.startsWith('0x') ? value.slice(2).padStart(64, '0') : BigInt(value).toString(16).padStart(64, '0');
const encodedWords = values => `0x${values.map(word).join('')}`;
function evmExecutionRpc(f) {
  const s = evmRpc(f.chain), p = f.prepared, intent = p.approveIntent.args[1];
  s.tx.to = EVM_EXECUTOR; s.tx.from = EVM_AGENT; s.tx.input = `0xe751f271${p.authorizationCommitment.slice(2)}`; s.tx.transactionIndex = '0x0';
  s.receipt.to = EVM_EXECUTOR; s.receipt.from = EVM_AGENT; s.receipt.transactionIndex = '0x0';
  s.block.parentHash = `0x${'c'.repeat(64)}`;
  s.receipt.logs = [{ address: EVM_EXECUTOR, topics: ['0x20f591e22f01104e4c2a646a938001e2382a73446550a3b72a94549b841f8bf8', p.authorizationCommitment, encodedWords([EVM_AGENT]), encodedWords([ADDRESS])], data: encodedWords([intent.amount, intent.nonce]), transactionHash: TX, blockHash: BLOCK, blockNumber: '0x64', transactionIndex: '0x0', logIndex: '0x0', removed: false }];
  s.code = RUNTIME; s.owner = EVM_OWNER; s.intent = structuredClone(intent);
  s.flags = { '0x7865c3ad': '1', '0x328a93e8': '0', '0xa9fcfb33': '1', '0x0a2dc12f': '1', '0x301fcb6e': '1' };
  s.policy = ['1', intent.policyIdHash, intent.policyVersion, intent.revocationEpoch];
  const original = s.handler;
  s.handler = (m, q) => {
    if (m === 'eth_getCode') return s.code;
    if (m === 'eth_call') {
      const selector = q[0].data.slice(0, 10);
      if (selector === '0x8da5cb5b') return encodedWords([s.owner]);
      if (selector === '0xf13c46aa') return encodedWords(Object.values(s.intent));
      if (selector === '0xcd4b90f9') return encodedWords(s.policy);
      if (Object.hasOwn(s.flags, selector)) return encodedWords([s.flags[selector]]);
      throw Error(`unexpected call ${selector}`);
    }
    return original(m, q);
  };
  return s;
}
for (const chain of ['ethereum', 'bsc']) {
  test(`${chain}: exact owner-attested native execution independently binds runtime, owner, typed record, event and all-in fee`, async () => {
    const f = evmExecutionFixture(chain), s = evmExecutionRpc(f);
    await withRpc(s.handler, async calls => {
      const result = await inspectEvmExecutionTransaction({ adapter: f.adapter, prepared: f.prepared, transactionId: TX });
      assert.equal(result.state, 'OBSERVED_EVM_EXECUTION_NOT_SETTLED', result.reason);
      assert.equal(result.actualFeeAtomic, '1');
      assert(calls.some(c => c.method === 'eth_getCode' && c.params[1].requireCanonical));
      const verified = await verifyEvmExecutionSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: TX });
      assert.equal(verified.state, 'NATIVE_SETTLEMENT_VERIFIED', verified.reason);
      const metadata = readVerifiedNativeSettlement(verified.receipt);
      assert.equal(metadata.authorizationCommitment, f.prepared.authorizationCommitment.slice(2));
      assert.equal(metadata.sourceNetworkId, metadata.destinationNetworkId);
      assert.equal(metadata.amountAtomic, '7');
    });
  });
  for (const [name, mutate] of [
    ['sender', s => { s.tx.from = EVM_OWNER; }], ['receipt sender', s => { s.receipt.from = EVM_OWNER; }],
    ['executor', s => { s.tx.to = ADDRESS; }], ['calldata commitment', s => { s.tx.input = `0xe751f271${'c'.repeat(64)}`; }],
    ['calldata suffix', s => { s.tx.input += '00'; }], ['amount', s => { s.tx.value = '0x8'; }],
    ['runtime code', s => { s.code = '0x60016000'; }], ['owner', s => { s.owner = EVM_AGENT; }],
    ['typed recipient', s => { s.intent.recipient = EVM_OWNER; }], ['typed amount', s => { s.intent.amount = '8'; }],
    ['typed nonce', s => { s.intent.nonce = '2'; }], ['typed policy', s => { s.intent.policyVersion = '2'; }],
    ['typed expiry', s => { s.intent.expiry = '1'; }], ['typed chain', s => { s.intent.chainId = '1'; }],
    ['not approved', s => { s.flags['0x7865c3ad'] = '0'; }], ['revoked', s => { s.flags['0x328a93e8'] = '1'; }],
    ['not executed', s => { s.flags['0xa9fcfb33'] = '0'; }], ['nonce not reserved', s => { s.flags['0x0a2dc12f'] = '0'; }],
    ['intent ID not reserved', s => { s.flags['0x301fcb6e'] = '0'; }],
    ['native policy inactive', s => { s.policy[0] = '0'; }], ['native policy epoch', s => { s.policy[3] = '2'; }],
    ['event emitter', s => { s.receipt.logs[0].address = EVM_OWNER; }], ['event amount', s => { s.receipt.logs[0].data = encodedWords(['8', '1']); }],
    ['event recipient', s => { s.receipt.logs[0].topics[3] = encodedWords([EVM_OWNER]); }], ['event commitment', s => { s.receipt.logs[0].topics[1] = BLOCK; }],
    ['removed log', s => { s.receipt.logs[0].removed = true; }], ['event transaction', s => { s.receipt.logs[0].transactionHash = BLOCK; }],
    ['duplicate event', s => { s.receipt.logs.push(structuredClone(s.receipt.logs[0])); }], ['transaction index', s => { s.tx.transactionIndex = '0x1'; }],
    ['failed execution', s => { s.receipt.status = '0x0'; }], ['all-in fee', s => { s.receipt.gasUsed = '0xffff'; }],
    ['blob envelope', s => { s.tx.type = '0x3'; s.receipt.type = '0x3'; }], ['delegation', s => { s.tx.authorizationList = []; }],
  ]) test(`${chain}: native execution rejects ${name}`, async () => {
    const f = evmExecutionFixture(chain), s = evmExecutionRpc(f); mutate(s);
    await withRpc(s.handler, async () => {
      const result = await verifyEvmExecutionSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: TX });
      assert.equal(result.state, 'NOT_VERIFIED', name);
      assert.equal(result.receipt, undefined);
    });
  });
  test(`${chain}: copied preparation/callback claims cannot mint receipt; late reorg rejected`, async () => {
    const f = evmExecutionFixture(chain), s = evmExecutionRpc(f);
    await withRpc(() => { throw Error('must not call RPC'); }, async () => {
      for (const extra of [{ prepared: { ...f.prepared } }, { verified: true }, { verify: () => true }]) assert.equal((await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: TX, ...extra })).state, 'NOT_VERIFIED');
    });
    let reads = 0;
    await withRpc((m, p) => m === 'eth_getBlockByNumber' && p[0] === '0x64' && ++reads > 1 ? { ...s.block, hash: TX } : s.handler(m, p), async () => assert.match((await verifyEvmExecutionSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: TX })).reason, /reorg/));
  });
}

for (const chain of ['near', 'ethereum', 'bsc']) test(`${chain}: caller mutation during RPC cannot retarget an issued native receipt`, async () => {
  const f = chain === 'near' ? nearExecutionFixture() : evmExecutionFixture(chain);
  const other = chain === 'near' ? nearExecutionFixture() : evmExecutionFixture(chain);
  const changed = structuredClone(other.authorization); changed.intentId = 'different-intent'; changed.nonce = '2';
  const replacement = chain === 'near'
    ? prepareNearExecution({ adapter: other.adapter, authorization: changed, recipient: 'bob.testnet', nowUnix: NOW })
    : prepareEvmExecution({ adapter: other.adapter, authorization: changed, recipient: ADDRESS, nowUnix: NOW });
  const s = chain === 'near' ? nearExecutionRpc(f) : evmExecutionRpc(f);
  const input = { adapter: f.adapter, prepared: f.prepared, transactionId: chain === 'near' ? NEAR_ID : TX };
  let mutated = false;
  await withRpc((method, params) => {
    if (!mutated) { mutated = true; input.adapter = other.adapter; input.prepared = replacement; input.transactionId = 'forged-after-await'; }
    return s.handler(method, params);
  }, async () => {
    const result = await verifyTestnetSettlement(input);
    assert.equal(result.state, 'NATIVE_SETTLEMENT_VERIFIED', result.reason);
    const metadata = readVerifiedNativeSettlement(result.receipt);
    assert.equal(metadata.intentId, f.authorization.intentId);
    assert.notEqual(metadata.intentId, changed.intentId);
    assert.equal(metadata.authorization.nonce, f.authorization.nonce);
    assert.equal(metadata.transactionId, chain === 'near' ? NEAR_ID : TX);
  });
});

async function nativeStoreFixture(chain = 'near', { trust = true, transactionId, mutateTrust } = {}) {
  const f = chain === 'near' ? nearExecutionFixture() : evmExecutionFixture(chain);
  const s = chain === 'near' ? nearExecutionRpc(f) : evmExecutionRpc(f);
  const nativeId = chain === 'near' ? NEAR_ID : TX;
  const verified = await withRpc(s.handler, () => verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: nativeId }));
  assert.equal(verified.state, 'NATIVE_SETTLEMENT_VERIFIED', verified.reason);
  const identity = structuredClone(verified.verification.verifierIdentity);
  if (mutateTrust) mutateTrust(identity);
  const root = await mkdtemp(join(tmpdir(), 'qpc-native-settlement-'));
  const store = new FileIntentStore(root, { nativeTrust: trust ? [identity] : [] });
  const record = await store.create(f.authorization, { nowUnix: NOW });
  // This callback is only the existing trusted local authorization integration;
  // it never supplies destination proof, which is independently parsed above.
  await store.authorize(f.authorization, { nowUnix: NOW, verifyAgentAuthorization: async () => true });
  await store.reserveSubmission(record.id, { nowUnix: NOW + 1 });
  await store.recordSubmission(record.id, { nowUnix: NOW + 2, transactionId: transactionId ?? nativeId });
  return { ...f, rpc: s, store, root, id: record.id, verified, nativeId };
}
for (const chain of ['near', 'ethereum', 'bsc']) {
  test(`${chain}: durable store atomically settles ONLY fresh module-owned proof matching independent operator trust`, async () => {
    const f = await nativeStoreFixture(chain);
    try {
      const before = await f.store.get(f.id);
      assert.equal(before.state, 'SUBMITTED');
      const settled = await f.store.settleNative(f.id, { verification: f.verified.receipt, nowUnix: NOW + 3, expectedRevision: before.revision });
      assert.equal(settled.state, 'SETTLED');
      assert.equal(settled.revision, before.revision + 2);
      assert.deepEqual(settled.history.slice(-2).map(h => h.state), ['CONFIRMED', 'SETTLED']);
      assert.equal(settled.confirmation.authorizationCommitment, f.id);
      assert.equal(settled.confirmation.actualFeeAtomic, chain === 'near' ? '4' : '1');
      assert.deepEqual(settled.confirmation.verifierIdentity, f.verified.verification.verifierIdentity);
      await assert.rejects(f.store.settleNative(f.id, { verification: f.verified.receipt, nowUnix: NOW + 4 }), /invalid transition/);
      assert.equal((await f.store.get(f.id)).revision, settled.revision);
      const reopened = new FileIntentStore(f.root, { nativeTrust: [f.verified.verification.verifierIdentity] });
      assert.equal((await reopened.get(f.id)).state, 'SETTLED');
      assert.deepEqual(await reopened.recover({ nowUnix: NOW + 4 }), []);
      assert.deepEqual(await reopened.get(f.id), settled);
      assert.deepEqual(await reopened.recover({ nowUnix: f.authorization.time.expiresAtUnix + 1 }), []);
      assert.deepEqual(await reopened.get(f.id), settled);
      // Persisted metadata is not a process-owned receipt on restart.
      await assert.rejects(reopened.settleNative(f.id, { verification: settled.confirmation, nowUnix: NOW + 4 }), /fresh native verification handle/);
    } finally { await rm(f.root, { recursive: true, force: true }); }
  });
}
for (const [name, corrupt] of [
  ['state', p => { p.state = 'EVIDENCE_VERIFIED'; }],
  ['commitment', p => { p.authorizationCommitment = '0'.repeat(64); }],
  ['authorization', p => { p.authorization.nonce = '99'; }],
  ['transaction', p => { p.transactionId = 'wrong'; }],
  ['network', p => { p.destinationNetworkId = 'near:mainnet'; }],
  ['amount', p => { p.amountAtomic = '999'; }],
]) test(`durable native recovery quarantines corrupted ${name} proof metadata`, async () => {
  const f = await nativeStoreFixture();
  try {
    await f.store.settleNative(f.id, { verification: f.verified.receipt, nowUnix: NOW + 3 });
    const path = join(f.root, 'state.json'), state = JSON.parse(await readFile(path, 'utf8'));
    corrupt(state.records[f.id].confirmation);
    await writeFile(path, JSON.stringify(state));
    const reopened = new FileIntentStore(f.root, { nativeTrust: [f.verified.verification.verifierIdentity] });
    assert.equal((await reopened.recover({ nowUnix: NOW + 4 })).length, 1);
    assert.equal((await reopened.get(f.id)).state, 'RECONCILING');
  } finally { await rm(f.root, { recursive: true, force: true }); }
});
for (const changedTrust of ['removed', 'wrong']) test(`durable native recovery quarantines ${changedTrust} operator trust`, async () => {
  const f = await nativeStoreFixture();
  try {
    await f.store.settleNative(f.id, { verification: f.verified.receipt, nowUnix: NOW + 3 });
    const identity = structuredClone(f.verified.verification.verifierIdentity);
    identity.codeHash = 'changed';
    const reopened = new FileIntentStore(f.root, { nativeTrust: changedTrust === 'removed' ? [] : [identity] });
    const recovered = await reopened.recover({ nowUnix: NOW + 4 });
    assert.equal(recovered.length, 1);
    assert.equal((await reopened.get(f.id)).state, 'RECONCILING');
  } finally { await rm(f.root, { recursive: true, force: true }); }
});
test('durable native settlement rejects metadata/clone/boolean/callback proofs without state mutation', async () => {
  const f = await nativeStoreFixture();
  try {
    for (const verification of [true, f.verified.verification, { ...f.verified.receipt }, JSON.parse(JSON.stringify(f.verified.receipt)), () => true]) {
      await assert.rejects(f.store.settleNative(f.id, { verification, nowUnix: NOW + 3 }), /fresh native verification handle/);
      assert.equal((await f.store.get(f.id)).state, 'SUBMITTED');
    }
    await assert.rejects(f.store.confirm(f.id, { nowUnix: NOW + 3, evidence: { verified: true }, verifyDestinationEvidence: async () => true }), /NOT_VERIFIED|evidence/);
    await assert.rejects(f.store.settle(f.id, { nowUnix: NOW + 3 }), /NOT_IMPLEMENTED/);
  } finally { await rm(f.root, { recursive: true, force: true }); }
});
for (const [name, options, expected] of [
  ['missing independent operator trust', { trust: false }, /trust identity/],
  ['wrong contract pin', { mutateTrust: identity => { identity.codeHash = 'wrong'; } }, /trust identity/],
  ['wrong owner trust', { mutateTrust: identity => { identity.owner = 'mallory.testnet'; } }, /trust identity/],
  ['wrong submitted transaction', { transactionId: PAYER }, /transaction mismatch/],
]) test(`durable native settlement rejects ${name}`, async () => {
  const f = await nativeStoreFixture('near', options);
  try {
    await assert.rejects(f.store.settleNative(f.id, { verification: f.verified.receipt, nowUnix: NOW + 3 }), expected);
    assert.equal((await f.store.get(f.id)).state, 'SUBMITTED');
  } finally { await rm(f.root, { recursive: true, force: true }); }
});
test('durable native settlement rejects wrong intent and wall-clock expiry before and inside lock', async () => {
  const f = await nativeStoreFixture();
  try {
    await assert.rejects(f.store.settleNative('b'.repeat(64), { verification: f.verified.receipt, nowUnix: NOW + 3 }), /commitment mismatch/);
    const originalNow = Date.now, base = originalNow();
    try {
      Date.now = () => base + 62_000;
      await assert.rejects(f.store.settleNative(f.id, { verification: f.verified.receipt, nowUnix: NOW + 3 }), /fresh native verification handle/);
      let reads = 0;
      Date.now = () => base + (++reads >= 3 ? 62_000 : 0);
      await assert.rejects(f.store.settleNative(f.id, { verification: f.verified.receipt, nowUnix: NOW + 3 }), /expired native verification handle/);
    } finally { Date.now = originalNow; }
    assert.equal((await f.store.get(f.id)).state, 'SUBMITTED');
  } finally { await rm(f.root, { recursive: true, force: true }); }
});
test('durable native settlement CAS races reconciliation without overwriting either winner', async () => {
  const f = await nativeStoreFixture();
  try {
    const before = await f.store.get(f.id);
    const results = await Promise.allSettled([
      f.store.settleNative(f.id, { verification: f.verified.receipt, nowUnix: NOW + 3, expectedRevision: before.revision }),
      f.store.reconcile(f.id, { nowUnix: NOW + 3, expectedRevision: before.revision, reason: 'reorg-check' }),
    ]);
    assert.equal(results.filter(r => r.status === 'fulfilled').length, 1);
    assert.match(results.find(r => r.status === 'rejected').reason.message, /stale intent revision/);
    const after = await f.store.get(f.id);
    assert(['SETTLED', 'RECONCILING'].includes(after.state));
    if (after.state === 'RECONCILING') assert.equal((await f.store.settleNative(f.id, { verification: f.verified.receipt, nowUnix: NOW + 4, expectedRevision: after.revision })).state, 'SETTLED');
  } finally { await rm(f.root, { recursive: true, force: true }); }
});
test('durable native settlement prevents the same native transaction from reserving a second intent', async () => {
  const f = await nativeStoreFixture();
  try {
    const second = structuredClone(f.authorization);
    second.intentId = 'testnet-2'; second.nonce = '2';
    const secondRecord = await f.store.create(second, { nowUnix: NOW });
    await f.store.authorize(second, { nowUnix: NOW, verifyAgentAuthorization: async () => true });
    await f.store.reserveSubmission(secondRecord.id, { nowUnix: NOW + 1 });
    await f.store.recordSubmission(secondRecord.id, { nowUnix: NOW + 2, transactionId: NEAR_ID });
    await assert.rejects(f.store.settleNative(f.id, { verification: f.verified.receipt, nowUnix: NOW + 3 }), /transaction already reserved/);
    assert.equal((await f.store.get(f.id)).state, 'SUBMITTED');
    assert.equal((await f.store.get(secondRecord.id)).state, 'SUBMITTED');
  } finally { await rm(f.root, { recursive: true, force: true }); }
});

function solanaRpc() {
  const state = {
    tx: { slot: 100, blockTime: NOW, version: 'legacy', meta: { err: null, fee: 1, innerInstructions: [], preBalances: [100, 0, 0], postBalances: [92, 7, 0] }, transaction: { signatures: [SIGNATURE], message: { recentBlockhash: KEY, accountKeys: [
      { pubkey: PAYER, signer: true, writable: true }, { pubkey: RECIPIENT, signer: false, writable: true }, { pubkey: KEY, signer: false, writable: false },
    ], instructions: [{ program: 'system', programId: KEY, parsed: { type: 'transfer', info: { source: PAYER, destination: RECIPIENT, lamports: 7 } } }] } } },
    block: { blockhash: KEY, blockHeight: 100, blockTime: NOW, signatures: [SIGNATURE] },
    status: { value: [{ slot: 100, err: null, confirmationStatus: 'finalized' }] },
  };
  state.handler = async method => {
    if (method === 'getGenesisHash') return TESTNET_NETWORKS.solana.nativeChainId;
    if (method === 'getTransaction') return state.tx;
    if (method === 'getSignatureStatuses') return state.status;
    if (method === 'getSlot') return 102;
    if (method === 'getBlocks') return [100, 101];
    if (method === 'getBlock') return state.block;
    throw Error(`unexpected method ${method}`);
  };
  return state;
}
test('Solana devnet checks genesis, transfer program, signer, finality and canonical signature inclusion; never settles', async () => {
  const f = preparedFixture('solana'), s = solanaRpc();
  await withRpc(s.handler, async calls => {
    assert.equal((await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: SIGNATURE })).state, 'NOT_IMPLEMENTED');
    assert(calls.every(c => c.url === TESTNET_NETWORKS.solana.rpcUrl));
  });
});
for (const [name, mutate] of [
  ['signature', s => { s.tx.transaction.signatures = ['2'.repeat(64)]; }],
  ['recipient', s => { s.tx.transaction.message.instructions[0].parsed.info.destination = PAYER; }],
  ['amount', s => { s.tx.transaction.message.instructions[0].parsed.info.lamports = 8; }],
  ['unsafe amount', s => { s.tx.transaction.message.instructions[0].parsed.info.lamports = Number.MAX_SAFE_INTEGER + 1; }],
  ['program', s => { s.tx.transaction.message.instructions[0].programId = PAYER; }],
  ['extra instruction', s => { s.tx.transaction.message.instructions.push(s.tx.transaction.message.instructions[0]); }],
  ['failed transaction', s => { s.tx.meta.err = { InstructionError: [0, 'error'] }; }],
  ['nonfinal status', s => { s.status.value[0].confirmationStatus = 'confirmed'; }],
  ['block signature', s => { s.block.signatures = []; }],
  ['expiry', s => { s.tx.blockTime = NOW + 100; }],
  ['block height expiry', s => { s.block.blockHeight = 151; }],
  ['balance delta', s => { s.tx.meta.postBalances[1] = 8; }],
]) test(`Solana rejects ${name}`, async () => {
  const f = preparedFixture('solana'), s = solanaRpc(); mutate(s);
  await withRpc(s.handler, async () => assert.equal((await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: SIGNATURE })).state, 'NOT_VERIFIED'));
});
test('Solana rejects wrong genesis, skipped-slot confirmation inflation and reorg', async () => {
  const f = preparedFixture('solana'), s = solanaRpc(); let blocks = 0;
  for (const handler of [async m => m === 'getGenesisHash' ? KEY : s.handler(m), async m => m === 'getBlocks' ? [100] : s.handler(m), async m => m === 'getBlock' && ++blocks > 1 ? { ...s.block, blockhash: '2'.repeat(32) } : s.handler(m)]) {
    await withRpc(handler, async () => assert.equal((await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: SIGNATURE })).state, 'NOT_VERIFIED'));
  }
});
test('Zcash inspects only testnet node identity and never prepares shielded or transparent fallback transfers', async () => {
  const f = fixture('zcash');
  assert.throws(() => prepareTestnetTransfer(f), /shielded.*unsupported/);
  await withRpc(async method => { assert.equal(method, 'getblockchaininfo'); return { chain: 'test', blocks: 100, bestblockhash: 'a'.repeat(64) }; }, async calls => {
    assert.equal((await inspectZcashTestnet({ adapter: f.adapter })).state, 'NOT_IMPLEMENTED');
    assert(calls.every(c => c.url === 'http://127.0.0.1:18232'));
  });
  await withRpc(async () => ({ chain: 'main', blocks: 100, bestblockhash: 'a'.repeat(64) }), async () => assert.equal((await inspectZcashTestnet({ adapter: f.adapter })).state, 'NOT_VERIFIED'));
});
test('RPC transport errors and malformed JSON/results fail closed', async () => {
  const f = preparedFixture(); const original = globalThis.fetch;
  try {
    for (const response of [async () => { throw Error('timeout'); }, async () => new Response('no', { status: 503 }), async () => new Response('not json'), async () => new Response(JSON.stringify({ id: 1, error: { code: -1 } })), async () => new Response(JSON.stringify({ id: 2, result: '0xaa36a7' })), async () => new Response('x'.repeat(2_000_001))]) {
      globalThis.fetch = response;
      assert.equal((await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: TX })).state, 'NOT_VERIFIED');
    }
  } finally { globalThis.fetch = original; }
});

for (const chain of ['near', 'ethereum', 'bsc', 'solana', 'zcash']) {
  test(`${chain}: read-only network probe checks pinned testnet identity without a transfer`, async () => {
    const f = fixture(chain);
    const handler = chain === 'near' ? nearRpc().handler : chain === 'solana' ? solanaRpc().handler : chain === 'zcash' ? async () => ({ chain: 'test', blocks: 100, bestblockhash: 'a'.repeat(64) }) : evmRpc(chain).handler;
    await withRpc(handler, async calls => {
      const result = await inspectTestnetNetwork(f.adapter);
      assert.equal(result.state, 'NETWORK_IDENTITY_OBSERVED_NOT_SETTLED', result.reason);
      assert.equal(result.networkId, TESTNET_NETWORKS[chain].networkId);
      assert(calls.every(c => c.url === TESTNET_NETWORKS[chain].rpcUrl));
    });
    await withRpc(async () => chain === 'near' ? { chain_id: 'testnet', genesis_hash: KEY } : chain === 'solana' ? KEY : chain === 'zcash' ? { chain: 'main' } : '0x1', async () => {
      assert.equal((await inspectTestnetNetwork(f.adapter)).state, 'NOT_VERIFIED');
    });
  });
}
for (const chain of ['ethereum', 'bsc']) {
  test(`${chain}: correct chain ID but substituted genesis is rejected`, async () => {
    const f = preparedFixture(chain), state = evmRpc(chain);
    await withRpc(async (method, params) => method === 'eth_getBlockByNumber' && params[0] === '0x0' ? { number: '0x0', hash: BLOCK } : state.handler(method, params), async () => {
      assert.match((await inspectTestnetNetwork(f.adapter)).reason, /genesis mismatch/);
      assert.match((await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: TX })).reason, /genesis mismatch/);
    });
  });
}
