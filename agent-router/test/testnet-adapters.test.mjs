import test from 'node:test';
import assert from 'node:assert/strict';
import {
  createTestnetAdapter, prepareTestnetTransfer, inspectTestnetTransaction,
  verifyTestnetSettlement, inspectZcashTestnet, inspectTestnetNetwork, TESTNET_NETWORKS,
} from '../src/testnet-adapters.mjs';
import { computeNativeRecipientCommitment } from '../src/native-adapters.mjs';

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
