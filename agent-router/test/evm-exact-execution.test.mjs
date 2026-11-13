import test from 'node:test';
import assert from 'node:assert/strict';
import { prepareEvmOwnerApprovedExecution } from '../src/evm-exact-execution.mjs';
import { computeNativeRecipientCommitment } from '../src/native-adapters.mjs';
import { computeCrossL1AuthorizationCommitment } from '../src/cross-l1-envelope.mjs';
const NOW = 1800000000;
const addr = n => `0x${String(n).repeat(40)}`;
function fixture(chain = 'ethereum') {
  const nativeChainId = chain === 'ethereum' ? '11155111' : '97';
  const contractAddress = addr(1), ownerAddress = addr(2), agent = addr(3), recipient = addr(4);
  const authorization = {
    schema: 'qpc.cross-l1.authorization', version: 1, intentId: 'local-native-1',
    agentIdentity: { namespace: 'eip155', id: agent, keyId: agent },
    source: { networkId: `eip155:${nativeChainId}`, nativeChainId, accountId: agent, adapterId: contractAddress },
    destination: { networkId: `eip155:${nativeChainId}`, nativeChainId, adapterId: contractAddress,
      recipientCommitment: computeNativeRecipientCommitment({ chain, nativeChainId, recipient }) },
    action: { type: 'asset-transfer', operation: 'pay' },
    asset: { sourceAssetId: `${chain}:native`, destinationAssetId: `${chain}:native`, sourceDecimals: 18, destinationDecimals: 18, mappingId: `${chain}-native-identity-v1` },
    amount: { sourceAtomic: '123', destinationAtomic: '123' },
    fees: { feeAssetId: `${chain}:native`, maxFeeAtomic: '1000000000000000', maxSlippageBps: 0 }, nonce: '7',
    replayDomain: { authorityId: contractAddress, scope: 'native-payment' },
    policy: { id: 'policy-1', version: 2, commitment: 'a'.repeat(64) },
    time: { issuedAtUnix: NOW, notBeforeUnix: NOW, expiresAtUnix: NOW + 300 },
    settlement: { evidenceClass: 'evm-owner-attested-native-v1', finalityPolicy: `${chain}-finalized`, minConfirmations: 2 },
    cancellation: { authorityId: ownerAddress, revocationEpoch: 1 },
  };
  return { authorization, recipient, nowUnix: NOW, trustedProfile: { chain, nativeChainId, contractAddress, ownerAddress } };
}
for (const chain of ['ethereum', 'bsc']) {
  test(`${chain}: unsigned owner approval binds every execution field without claiming authorization`, () => {
    const f = fixture(chain), p = prepareEvmOwnerApprovedExecution(f), i = p.approveIntent.args[1];
    assert.equal(p.authorizationCommitment, `0x${computeCrossL1AuthorizationCommitment(f.authorization)}`);
    assert.equal(p.execute.value, '123'); assert.equal(p.execute.to, f.trustedProfile.contractAddress);
    assert.equal(i.agent, f.authorization.source.accountId); assert.equal(i.recipient, f.recipient);
    assert.equal(i.chainId, f.trustedProfile.nativeChainId); assert.equal(i.nonce, '7');
    assert.equal(i.policyVersion, '2'); assert.equal(i.revocationEpoch, '1'); assert.equal(i.decimals, 18);
    assert.equal(i.policyCommitment, `0x${f.authorization.policy.commitment}`);
    assert.equal(i.maxFee, f.authorization.fees.maxFeeAtomic);
    assert.equal(p.ownerAttestationRequired, true); assert.equal(p.feeBudgetEnforcedByContract, false);
    assert.equal(p.settlement, 'NOT_IMPLEMENTED'); assert.ok(Object.isFrozen(i));
    f.authorization.amount.destinationAtomic = '999'; assert.equal(i.amount, '123');
  });
  const changes = [
    ['chain', f => f.authorization.source.nativeChainId = '1'],
    ['recipient', f => f.recipient = addr(5)],
    ['amount', f => f.authorization.amount.destinationAtomic = '124'],
    ['decimals', f => f.authorization.asset.destinationDecimals = 6],
    ['token', f => f.authorization.asset.destinationAssetId = 'erc20:token'],
    ['executor', f => f.authorization.destination.adapterId = addr(5)],
    ['agent', f => f.authorization.agentIdentity.id = addr(5)],
    ['action', f => f.authorization.action.operation = 'approve'],
    ['replay', f => f.authorization.replayDomain.scope = 'other'],
    ['owner', f => f.authorization.cancellation.authorityId = addr(5)],
    ['epoch', f => f.authorization.cancellation.revocationEpoch = 0],
    ['expiry', f => f.nowUnix = f.authorization.time.expiresAtUnix],
    ['future', f => f.nowUnix = NOW - 1],
    ['fee', f => f.authorization.fees.maxFeeAtomic = '0'],
    ['slippage', f => f.authorization.fees.maxSlippageBps = 1],
    ['finality', f => f.authorization.settlement.finalityPolicy = 'safe'],
    ['self-executor recipient', f => f.recipient = f.trustedProfile.contractAddress],
    ['extra profile', f => f.trustedProfile.untrusted = true],
    ['mainnet', f => f.trustedProfile.nativeChainId = '1'],
  ];
  for (const [name, mutate] of changes) test(`${chain}: reject ${name} substitution`, () => {
    const f = fixture(chain); mutate(f); assert.throws(() => prepareEvmOwnerApprovedExecution(f));
  });
}
test('reject mutable accessors without reading them', () => {
  const f = fixture(); let reads = 0;
  Object.defineProperty(f.trustedProfile, 'chain', { enumerable: true, get() { reads++; return 'ethereum'; } });
  assert.throws(() => prepareEvmOwnerApprovedExecution(f), /data fields/); assert.equal(reads, 0);
});
