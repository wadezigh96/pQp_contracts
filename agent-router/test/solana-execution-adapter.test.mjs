import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { FileIntentStore } from '../src/intent-store.mjs';
import { computeNativeRecipientCommitment } from '../src/native-adapters.mjs';
import { createTestnetAdapter, TESTNET_NETWORKS, prepareSolanaExecution, inspectSolanaExecutionTransaction,
  verifySolanaExecutionSettlement, verifyTestnetSettlement, readVerifiedNativeSettlement } from '../src/testnet-adapters.mjs';

const NOW = 1800000000, SYSTEM = '11111111111111111111111111111111';
const LOADER = 'BPFLoaderUpgradeab1e11111111111111111111111';
function b58(bytes) {
  const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
  let n = 0n; for (const v of bytes) n = n * 256n + BigInt(v);
  let s = ''; while (n) { s = alphabet[Number(n % 58n)] + s; n /= 58n; }
  let zeros = 0; while (zeros < bytes.length && bytes[zeros] === 0) zeros++;
  return '1'.repeat(zeros) + s;
}
const key = seed => b58(Buffer.alloc(32, seed));
const PROGRAM = key(6), OWNER = key(7), AGENT = key(8), RECIPIENT = key(9), POLICY = key(10), INTENT = key(11), PROGRAMDATA = key(12);
const BLOCK = key(13), SIGNATURE = b58(Buffer.alloc(64, 14)), APPROVAL_SIGNATURE = b58(Buffer.alloc(64, 16));
// Fixture bytes deliberately aren't an SBF executable. These tests establish
// parser/security behavior under an operator code pin, not a deployment/runtime.
const CODE = Buffer.from('fixture-only-not-deployed-sbf');
const CODE_HASH = createHash('sha256').update(CODE).digest('hex');
function fixture() {
  const network = TESTNET_NETWORKS.solana;
  const side = { networkId: network.networkId, nativeChainId: network.nativeChainId, adapterId: PROGRAM };
  const authorization = {
    schema: 'qpc.cross-l1.authorization', version: 1, intentId: 'solana-execution-1',
    agentIdentity: { namespace: 'solana', id: AGENT, keyId: AGENT },
    source: { ...side, accountId: AGENT },
    destination: { ...side, recipientCommitment: computeNativeRecipientCommitment({ chain: 'solana', nativeChainId: network.nativeChainId, recipient: RECIPIENT }) },
    action: { type: 'asset-transfer', operation: 'pay' },
    asset: { sourceAssetId: 'solana:native', destinationAssetId: 'solana:native', sourceDecimals: 9, destinationDecimals: 9, mappingId: 'solana-native-identity-v1' },
    amount: { sourceAtomic: '7', destinationAtomic: '7' },
    fees: { maxFeeAtomic: '5000', feeAssetId: 'solana:native', maxSlippageBps: 0 },
    nonce: '1', replayDomain: { authorityId: PROGRAM, scope: 'native-payment' },
    policy: { id: 'policy', version: 1, commitment: 'a'.repeat(64) },
    time: { issuedAtUnix: NOW, notBeforeUnix: NOW, expiresAtUnix: NOW + 100 },
    settlement: { evidenceClass: 'solana-owner-attested-native-v1', finalityPolicy: 'solana-finalized', minConfirmations: 2 },
    cancellation: { authorityId: OWNER, revocationEpoch: 1 },
  };
  const adapterInput = { chain: 'solana', trustedProfile: { destinationNetworkId: network.networkId, destinationNativeChainId: network.nativeChainId },
    solanaProgram: { programId: PROGRAM, owner: OWNER, codeHashSha256: CODE_HASH } };
  const adapter = createTestnetAdapter(adapterInput);
  const input = { adapter, authorization, recipient: RECIPIENT, policyAddress: POLICY, intentAddress: INTENT, nowUnix: NOW };
  const prepared = prepareSolanaExecution(input);
  return { adapter, authorization, prepared, input, adapterInput };
}
const account = (bytes, owner = PROGRAM, executable = false) => ({ data: [Buffer.from(bytes).toString('base64'), 'base64'], owner, executable, lamports: 1, rentEpoch: 1 });
function rpcFixture(f) {
  const program = Buffer.alloc(36); program.writeUInt32LE(2); Buffer.alloc(32, 12).copy(program, 4);
  const code = Buffer.alloc(45 + CODE.length); code.writeUInt32LE(3); code.writeBigUInt64LE(90n, 4); CODE.copy(code, 45);
  const accounts = [
    account(program, LOADER, true), account(code, LOADER),
    account(Buffer.from(f.prepared.initialize.dataHex.slice(2), 'hex')),
    account(Buffer.from(`${f.prepared.approve.dataHex.slice(2, -2)}01`, 'hex')),
  ];
  const keys = [...f.prepared.execute.accounts.map(a => ({ pubkey: a.pubkey, signer: a.isSigner, writable: a.isWritable })), { pubkey: PROGRAM, signer: false, writable: false }];
  const preBalances = [20000, 1, 1, 100, 1, 1], postBalances = [14993, 1, 1, 107, 1, 1];
  const state = {
    accounts, context: { slot: 110 },
    tx: { slot: 100, blockTime: NOW + 1, version: 'legacy', transaction: { signatures: [SIGNATURE], message: {
      recentBlockhash: key(15), accountKeys: keys,
      instructions: [{ programId: PROGRAM, accounts: f.prepared.execute.accounts.map(a => a.pubkey), data: b58(Buffer.from(f.prepared.execute.dataHex, 'hex')), stackHeight: 1 }],
    } }, meta: { err: null, fee: 5000, preBalances, postBalances,
      innerInstructions: [{ index: 0, instructions: [{ program: 'system', programId: SYSTEM, stackHeight: 2, parsed: { type: 'transfer', info: { source: AGENT, destination: RECIPIENT, lamports: 7 } } }] }],
    } },
    block: { blockhash: BLOCK, blockHeight: 95, blockTime: NOW + 1, signatures: [SIGNATURE] },
    statuses: { value: [{ slot: 100, err: null, confirmationStatus: 'finalized' }] },
    blocks: [100, 101], finalSlot: 110,
    approval: { slot: 95, blockTime: NOW, version: 'legacy', transaction: { signatures: [APPROVAL_SIGNATURE], message: {
      recentBlockhash: key(17), accountKeys: [...f.prepared.approve.accounts.map(a => ({ pubkey: a.pubkey, signer: a.isSigner, writable: a.isWritable })), { pubkey: PROGRAM, signer: false, writable: false }],
      instructions: [{ programId: PROGRAM, accounts: f.prepared.approve.accounts.map(a => a.pubkey), data: b58(Buffer.from(f.prepared.approve.dataHex, 'hex')) }],
    } }, meta: { err: null } },
    approvalBlock: { blockhash: key(18), blockHeight: 90, blockTime: NOW, signatures: [APPROVAL_SIGNATURE] },
    history: [{ signature: SIGNATURE, slot: 100, err: null, confirmationStatus: 'finalized' }, { signature: APPROVAL_SIGNATURE, slot: 95, err: null, confirmationStatus: 'finalized' }],
  };
  state.handler = async (method, params) => {
    if (method === 'getGenesisHash') return TESTNET_NETWORKS.solana.nativeChainId;
    if (method === 'getTransaction') return params[0] === APPROVAL_SIGNATURE ? state.approval : state.tx;
    if (method === 'getSignaturesForAddress') {
      assert.equal(params[0], INTENT);
      const start = params[1].before === undefined ? 0 : state.history.findIndex(h => h.signature === params[1].before) + 1;
      assert(start >= 0);
      return state.history.slice(start, start + params[1].limit);
    }
    if (method === 'getSignatureStatuses') return state.statuses;
    if (method === 'getSlot') return state.finalSlot;
    if (method === 'getBlocks') return state.blocks;
    if (method === 'getBlock') return params[0] === 95 ? state.approvalBlock : state.block;
    if (method === 'getAccountInfo') { assert.equal(params[0], PROGRAM); return { context: state.context, value: state.accounts[0] }; }
    if (method === 'getMultipleAccounts') { assert.deepEqual(params[0], [PROGRAM, PROGRAMDATA, POLICY, INTENT]); return { context: state.context, value: state.accounts }; }
    throw new Error(`Unexpected RPC ${method}`);
  };
  return state;
}
async function withRpc(handler, run) {
  const original = globalThis.fetch, calls = [];
  globalThis.fetch = async (url, options) => {
    assert.equal(url, TESTNET_NETWORKS.solana.rpcUrl); assert.equal(options.redirect, 'error');
    const body = JSON.parse(options.body); calls.push(body);
    return new Response(JSON.stringify({ jsonrpc: '2.0', id: 1, result: await handler(body.method, body.params, calls) }));
  };
  try { return await run(calls); } finally { globalThis.fetch = original; }
}
const verify = f => verifySolanaExecutionSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: SIGNATURE });
function changeAccount(s, index, change) { const b = Buffer.from(s.accounts[index].data[0], 'base64'); change(b); s.accounts[index].data[0] = b.toString('base64'); }

test('Solana owner approval discovery paginates beyond twenty dust transactions', async () => {
  const f = fixture(), s = rpcFixture(f);
  const dust = Array.from({ length: 42 }, (_, i) => ({ signature: b58(Buffer.alloc(64, i + 30)), slot: 100, err: null, confirmationStatus: 'finalized' }));
  s.history = [...dust, ...s.history];
  await withRpc(s.handler, async calls => {
    const result = await verify(f);
    assert.equal(result.state, 'NATIVE_SETTLEMENT_VERIFIED', result.reason);
    const scans = calls.filter(c => c.method === 'getSignaturesForAddress');
    assert.equal(scans.length, 3);
    assert.equal(scans[1].params[1].before, dust[19].signature);
    assert.equal(scans[2].params[1].before, dust[39].signature);
  });
});
test('Solana bounded history scan stops at deployment and rejects unseen approval', async () => {
  const f = fixture(), s = rpcFixture(f);
  s.history = Array.from({ length: 120 }, (_, i) => ({ signature: b58(Buffer.concat([Buffer.from([i + 1]), Buffer.alloc(63, 24)])), slot: 100, err: null, confirmationStatus: 'finalized' }));
  await withRpc(s.handler, async calls => {
    assert.equal((await verify(f)).state, 'NOT_VERIFIED');
    assert.equal(calls.filter(c => c.method === 'getSignaturesForAddress').length, 5);
  });
  s.history = s.history.slice(0, 20); s.history.at(-1).slot = 90;
  await withRpc(s.handler, async calls => {
    assert.equal((await verify(f)).state, 'NOT_VERIFIED');
    assert.equal(calls.filter(c => c.method === 'getSignaturesForAddress').length, 1);
  });
});
test('Solana approval pagination rejects a repeated exclusive cursor across pages', async () => {
  const f = fixture(), s = rpcFixture(f);
  const history = Array.from({ length: 20 }, (_, i) => ({ signature: b58(Buffer.alloc(64, i + 30)), slot: 100, err: null, confirmationStatus: 'finalized' }));
  await withRpc((m, p) => m === 'getSignaturesForAddress' ? history : s.handler(m, p), async calls => {
    assert.equal((await verify(f)).state, 'NOT_VERIFIED');
    assert.equal(calls.filter(c => c.method === 'getSignaturesForAddress').length, 2);
  });
});
for (const [name, mutate] of [
  ['ascending slots', s => { s.history[1].slot = 101; }],
  ['duplicate cursor', s => { s.history = Array.from({ length: 20 }, () => s.history[0]); }],
  ['malformed signature', s => { s.history[0].signature = 'bad'; }],
  ['invalid slot', s => { s.history[0].slot = -1; }],
]) test(`Solana approval history rejects ${name}`, async () => {
  const f = fixture(), s = rpcFixture(f); mutate(s);
  await withRpc(s.handler, async () => assert.equal((await verify(f)).state, 'NOT_VERIFIED'));
});
test('Solana exact instruction/native CPI/finality/code pin yields opaque RPC-trusted receipt (fixture ONLY)', async () => {
  const f = fixture(), s = rpcFixture(f);
  await withRpc(s.handler, async calls => {
    const observation = await inspectSolanaExecutionTransaction({ adapter: f.adapter, prepared: f.prepared, transactionId: SIGNATURE });
    assert.equal(observation.state, 'OBSERVED_SOLANA_EXECUTION_NOT_SETTLED', observation.reason);
    assert.equal(observation.programIdentity.deploymentSlot, '90');
    const result = await verifyTestnetSettlement({ adapter: f.adapter, prepared: f.prepared, transactionId: SIGNATURE });
    assert.equal(result.state, 'NATIVE_SETTLEMENT_VERIFIED', result.reason);
    const proof = readVerifiedNativeSettlement(result.receipt);
    assert.equal(proof.actualFeeAtomic, '5000'); assert.equal(proof.authorizationCommitment, f.prepared.authorizationCommitment);
    assert.deepEqual(proof.verifierIdentity, { chain: 'solana', networkId: 'solana:devnet', nativeChainId: TESTNET_NETWORKS.solana.nativeChainId, contractAddress: PROGRAM, owner: OWNER, codeHash: CODE_HASH });
    assert.equal(readVerifiedNativeSettlement(structuredClone(result.receipt)), null);
    assert(Object.isFrozen(proof) && Object.isFrozen(proof.authorization));
    assert(calls.filter(c => c.method === 'getMultipleAccounts').every(c => c.params[1].commitment === 'finalized' && c.params[1].minContextSlot === 100));
    assert(!calls.some(c => /^(send|sign|simulate|requestAirdrop)/i.test(c.method)));
  });
});

for (const [name, mutate] of [
  ['signature', s => { s.tx.transaction.signatures[0] = b58(Buffer.alloc(64, 22)); }],
  ['failed tx', s => { s.tx.meta.err = { InstructionError: [0, 'Custom'] }; }],
  ['versioned tx', s => { s.tx.version = 0; }],
  ['extra signer', s => { s.tx.transaction.message.accountKeys[1].signer = true; }],
  ['readonly recipient', s => { s.tx.transaction.message.accountKeys[3].writable = false; }],
  ['extra account', s => { s.tx.transaction.message.accountKeys.push({ pubkey: key(23), signer: false, writable: false }); }],
  ['duplicate account', s => { s.tx.transaction.message.accountKeys[1] = s.tx.transaction.message.accountKeys[0]; }],
  ['outer program', s => { s.tx.transaction.message.instructions[0].programId = SYSTEM; }],
  ['outer nonce', s => { s.tx.transaction.message.instructions[0].data = b58(Buffer.from('030200000000000000', 'hex')); }],
  ['outer account order', s => { s.tx.transaction.message.instructions[0].accounts.reverse(); }],
  ['extra outer instruction', s => { s.tx.transaction.message.instructions.push(s.tx.transaction.message.instructions[0]); }],
  ['missing inner instructions', s => { delete s.tx.meta.innerInstructions; }],
  ['extra inner instruction', s => { s.tx.meta.innerInstructions[0].instructions.push(s.tx.meta.innerInstructions[0].instructions[0]); }],
  ['wrong inner index', s => { s.tx.meta.innerInstructions[0].index = 1; }],
  ['wrong inner program', s => { s.tx.meta.innerInstructions[0].instructions[0].programId = PROGRAM; }],
  ['wrong inner amount', s => { s.tx.meta.innerInstructions[0].instructions[0].parsed.info.lamports = 8; }],
  ['wrong inner recipient', s => { s.tx.meta.innerInstructions[0].instructions[0].parsed.info.destination = OWNER; }],
  ['wrong inner source', s => { s.tx.meta.innerInstructions[0].instructions[0].parsed.info.source = OWNER; }],
  ['nested inner invocation', s => { s.tx.meta.innerInstructions[0].instructions[0].stackHeight = 3; }],
  ['unsafe inner amount', s => { s.tx.meta.innerInstructions[0].instructions[0].parsed.info.lamports = Number.MAX_SAFE_INTEGER + 1; }],
  ['fee budget exceeded', s => { s.tx.meta.fee = 5001; }],
  ['expiry', s => { s.tx.blockTime = NOW + 100; }],
  ['too early', s => { s.tx.blockTime = NOW - 1; }],
  ['balance mismatch', s => { s.tx.meta.postBalances[3] = 108; }],
  ['loaded addresses', s => { s.tx.meta.loadedAddresses = { readonly: [OWNER], writable: [] }; }],
  ['not finalized', s => { s.statuses.value[0].confirmationStatus = 'confirmed'; }],
  ['status slot', s => { s.statuses.value[0].slot = 99; }],
  ['status error', s => { s.statuses.value[0].err = 'failed'; }],
  ['not enough finality', s => { s.finalSlot = 100; }],
  ['skipped confirmation slot', s => { s.blocks = [100]; }],
  ['duplicate confirmations', s => { s.blocks = [100, 100]; }],
  ['canonical inclusion missing', s => { s.block.signatures = []; }],
  ['duplicate inclusion', s => { s.block.signatures.push(SIGNATURE); }],
  ['block timestamp mismatch', s => { s.block.blockTime++; }],
  ['snapshot stale', s => { s.context.slot = 99; }],
  ['program not executable', s => { s.accounts[0].executable = false; }],
  ['program loader wrong', s => { s.accounts[0].owner = PROGRAM; }],
  ['program data pointer', s => { changeAccount(s, 0, b => { b[4] = 1; }); }],
  ['program data owner wrong', s => { s.accounts[1].owner = PROGRAM; }],
  ['program data executable', s => { s.accounts[1].executable = true; }],
  ['wrong loader state', s => { changeAccount(s, 1, b => { b.writeUInt32LE(2); }); }],
  ['same-slot deployment ambiguous', s => { changeAccount(s, 1, b => { b.writeBigUInt64LE(100n, 4); }); }],
  ['newer deployment cannot prove execution code', s => { changeAccount(s, 1, b => { b.writeBigUInt64LE(105n, 4); }); }],
  ['mutable upgrade authority', s => { changeAccount(s, 1, b => { b[12] = 1; }); }],
  ['wrong executable hash', s => { changeAccount(s, 1, b => { b[45] ^= 1; }); }],
  ['policy account owner', s => { s.accounts[2].owner = OWNER; }],
  ['intent account owner', s => { s.accounts[3].owner = OWNER; }],
  ['policy owner bytes', s => { changeAccount(s, 2, b => { b[8] ^= 1; }); }],
  ['policy agent bytes', s => { changeAccount(s, 2, b => { b[40] ^= 1; }); }],
  ['policy genesis bytes', s => { changeAccount(s, 2, b => { b[72] ^= 1; }); }],
  ['policy epoch bytes', s => { changeAccount(s, 2, b => { b[144] ^= 1; }); }],
  ['policy revoked', s => { changeAccount(s, 2, b => { b[152] = 0; }); }],
  ['intent commitment bytes', s => { changeAccount(s, 3, b => { b[40] ^= 1; }); }],
  ['intent recipient bytes', s => { changeAccount(s, 3, b => { b[136] ^= 1; }); }],
  ['intent fee bytes', s => { changeAccount(s, 3, b => { b[224] ^= 1; }); }],
  ['intent not executed', s => { changeAccount(s, 3, b => { b[232] = 0; }); }],
  ['intent invalid boolean', s => { changeAccount(s, 3, b => { b[232] = 2; }); }],
  ['missing genuine approval history', s => { s.history = []; }],
  ['approval predates pinned deployment', s => { s.history[1].slot = 89; }],
  ['approval same slot as deployment ambiguous', s => { s.history[1].slot = 90; }],
  ['approval same slot as execution unsupported', s => { s.history[1].slot = 100; }],
  ['approval history duplicates', s => { s.history.push(s.history[1]); }],
  ['approval history failed', s => { s.history[1].err = 'failed'; }],
  ['approval history unfinalized', s => { s.history[1].confirmationStatus = 'confirmed'; }],
  ['approval transaction failure', s => { s.approval.meta.err = 'failed'; }],
  ['approval owner signer substitution', s => { s.approval.transaction.message.accountKeys[0].pubkey = AGENT; }],
  ['approval owner not signer', s => { s.approval.transaction.message.accountKeys[0].signer = false; }],
  ['approval exact commitment substitution', s => { const b = Buffer.from(fixture().prepared.approve.dataHex, 'hex'); b[41] ^= 1; s.approval.transaction.message.instructions[0].data = b58(b); }],
  ['approval outer extra instruction', s => { s.approval.transaction.message.instructions.push(s.approval.transaction.message.instructions[0]); }],
  ['approval extra signer', s => { s.approval.transaction.message.accountKeys[1].signer = true; }],
  ['approval signature mismatch', s => { s.approval.transaction.signatures[0] = SIGNATURE; }],
  ['approval outside issued window', s => { s.approval.blockTime = NOW - 1; }],
  ['approval block inclusion missing', s => { s.approvalBlock.signatures = []; }],
  ['approval block timestamp mismatch', s => { s.approvalBlock.blockTime++; }],
]) test(`Solana execution rejects ${name} without receipt`, async () => {
  const f = fixture(), s = rpcFixture(f); mutate(s);
  await withRpc(s.handler, async () => { const result = await verify(f); assert.equal(result.state, 'NOT_VERIFIED'); assert.equal(result.receipt, undefined); });
});

test('Solana rejects wrong genesis, reorg, and code/state mutation between snapshots', async () => {
  for (const mode of ['genesis', 'reorg', 'approval-reorg', 'code', 'intent']) {
    const f = fixture(), s = rpcFixture(f); let blockReads = 0, approvalReads = 0, accountReads = 0;
    await withRpc((method, params) => {
      if (method === 'getGenesisHash' && mode === 'genesis') return key(1);
      if (method === 'getBlock' && params[0] === 100 && ++blockReads === 2 && mode === 'reorg') return { ...s.block, blockhash: key(2) };
      if (method === 'getBlock' && params[0] === 95 && ++approvalReads === 2 && mode === 'approval-reorg') return { ...s.approvalBlock, blockhash: key(2) };
      if (method === 'getMultipleAccounts' && ++accountReads === 2) {
        if (mode === 'code') changeAccount(s, 1, b => { b[45] ^= 1; });
        if (mode === 'intent') changeAccount(s, 3, b => { b[232] = 0; });
      }
      return s.handler(method, params);
    }, async () => { assert.equal((await verify(f)).state, 'NOT_VERIFIED', mode); });
  }
});

test('Solana rejects caller proofs, cloned/untrusted preparations and endpoint/profile overrides', async () => {
  const f = fixture(), s = rpcFixture(f);
  await withRpc(s.handler, async () => {
    for (const input of [
      { adapter: f.adapter, prepared: structuredClone(f.prepared), transactionId: SIGNATURE },
      { adapter: f.adapter, prepared: f.prepared, transactionId: SIGNATURE, evidence: true },
      { adapter: f.adapter, prepared: f.prepared, transactionId: SIGNATURE, verifyNative: () => true },
      { adapter: f.adapter, prepared: f.prepared, transactionId: 'forged' },
    ]) assert.equal((await verifySolanaExecutionSettlement(input)).state, 'NOT_VERIFIED');
  });
  assert.throws(() => createTestnetAdapter({ ...f.adapterInput, rpcUrl: 'https://caller.invalid' }), /allowlisted/);
  const cloned = structuredClone(f.authorization); cloned.nonce = '2';
  const prepared = prepareSolanaExecution({ ...f.input, authorization: cloned }); cloned.nonce = '3';
  assert.equal(prepared.authorization.nonce, '2');
});

test('Solana caller mutation during RPC cannot retarget the issued proof', async () => {
  const f = fixture(), s = rpcFixture(f);
  const input = { adapter: f.adapter, prepared: f.prepared, transactionId: SIGNATURE }; let changed = false;
  await withRpc((method, params) => {
    if (!changed) { changed = true; input.adapter = {}; input.prepared = {}; input.transactionId = 'forged'; }
    return s.handler(method, params);
  }, async () => {
    const result = await verifySolanaExecutionSettlement(input);
    assert.equal(result.state, 'NATIVE_SETTLEMENT_VERIFIED', result.reason);
    assert.equal(result.verification.authorizationCommitment, f.prepared.authorizationCommitment);
    assert.equal(result.verification.transactionId, SIGNATURE);
  });
});

test('Solana verified opaque receipt integrates durable store with independent operator identity', async () => {
  const f = fixture(), s = rpcFixture(f), root = await mkdtemp(join(tmpdir(), 'qpc-solana-store-'));
  try {
    const result = await withRpc(s.handler, () => verify(f)); assert.equal(result.state, 'NATIVE_SETTLEMENT_VERIFIED', result.reason);
    const identity = result.verification.verifierIdentity;
    const store = new FileIntentStore(root, { nativeTrust: [identity] });
    const record = await store.create(f.authorization, { nowUnix: NOW });
    await store.authorize(f.authorization, { nowUnix: NOW, verifyAgentAuthorization: async () => true });
    await store.reserveSubmission(record.id, { nowUnix: NOW + 1 });
    await store.recordSubmission(record.id, { nowUnix: NOW + 2, transactionId: SIGNATURE });
    const wrong = new FileIntentStore(root, { nativeTrust: [{ ...identity, codeHash: 'a'.repeat(64) }] });
    await assert.rejects(wrong.settleNative(record.id, { verification: result.receipt, nowUnix: NOW + 3 }), /trust/);
    for (const verification of [true, result.verification, structuredClone(result.receipt), () => true])
      await assert.rejects(store.settleNative(record.id, { verification, nowUnix: NOW + 3 }), /fresh native verification handle/);
    const before = await store.get(record.id);
    const settled = await store.settleNative(record.id, { verification: result.receipt, nowUnix: NOW + 3, expectedRevision: before.revision });
    assert.equal(settled.state, 'SETTLED'); assert.equal(settled.confirmation.actualFeeAtomic, '5000');
    assert.deepEqual(settled.confirmation.verifierIdentity, identity);
    await assert.rejects(store.settleNative(record.id, { verification: result.receipt, nowUnix: NOW + 4 }), /invalid transition/);
  } finally { await rm(root, { recursive: true, force: true }); }
});
