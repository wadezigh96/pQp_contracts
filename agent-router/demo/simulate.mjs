import { readFile, mkdtemp, rm, mkdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { computeCrossL1AuthorizationCommitment, verifyCrossL1SettlementEvidence } from '../src/cross-l1-envelope.mjs';
import { FileIntentStore } from '../src/intent-store.mjs';
import { buildNearEnvelopeAuthorizationDispatch } from '../src/near-envelope-dispatch.mjs';
import { settleNativeTransfer } from '../src/native-adapters.mjs';
import { renderDashboard } from './dashboard.mjs';

export const fixtureUrl = new URL('../../docs/fixtures/cross-l1-v1.json', import.meta.url);

/** Offline demonstration only. The allowlist is NOT a signature verifier or an
 * on-chain identity check. Fresh private temporary storage keeps runs isolated;
 * the real FileIntentStore performs atomic replay consumption within each run.
 */
export async function runSimulation() {
  const { envelope, authorizationCommitment } = JSON.parse(await readFile(fixtureUrl, 'utf8'));
  const nowUnix = envelope.time.notBeforeUnix + 1;
  const root = await mkdtemp(join(tmpdir(), 'qpc-phase2-sim-'));
  const store = new FileIntentStore(root);
  const cases = [];
  let mockVerifierCalls = 0;
  const mockAuthorize = async ({ authorizationCommitment: candidate }) => {
    mockVerifierCalls++;
    return candidate === authorizationCommitment;
  };
  async function scenario(id, title, gate, expected, action) {
    try {
      const result = await action();
      if (expected !== null) throw new Error(`Expected rejection: ${expected}`);
      cases.push({ id, title, gate, outcome: 'ACCEPTED_SIMULATION_ONLY', detail: result.detail, state: result.state });
    } catch (error) {
      if (expected === null || !error.message.includes(expected)) throw error;
      cases.push({ id, title, gate, outcome: 'REJECTED', detail: error.message, state: 'NO_EXECUTION' });
    }
  }
  try {
    if (computeCrossL1AuthorizationCommitment(envelope) !== authorizationCommitment) throw new Error('Fixture commitment drift');
    const dispatch = buildNearEnvelopeAuthorizationDispatch({ envelope, nowUnix });
    await store.create(envelope, { nowUnix });
    await scenario('unauthorized-execution', 'Execute before authorization', 'FileIntentStore.reserveSubmission', 'invalid transition CREATED -> SUBMITTING',
      () => store.reserveSubmission(authorizationCommitment, { nowUnix }));
    await scenario('valid-authorization', 'Exact approved envelope', 'FileIntentStore.authorize', null, async () => {
      const record = await store.authorize(envelope, { nowUnix, verifyAgentAuthorization: mockAuthorize });
      return { state: record.state, detail: 'Real authorization gate accepted the fixture commitment using an explicit mock allowlist; no signature or chain was checked.' };
    });
    await scenario('replay', 'Reuse nonce and intent', 'FileIntentStore.authorize', 'already consumed',
      () => new FileIntentStore(root).authorize(envelope, { nowUnix, verifyAgentAuthorization: mockAuthorize }));
    await scenario('expiry', 'At the exact expiry boundary', 'FileIntentStore.authorize', 'authorization has expired',
      () => store.authorize(envelope, { nowUnix: envelope.time.expiresAtUnix, verifyAgentAuthorization: mockAuthorize }));
    const altered = structuredClone(envelope); altered.amount.destinationAtomic = '1';
    await scenario('rejected-authorization', 'Alter an approved envelope', 'FileIntentStore.authorize', 'agent authorization verification failed',
      () => store.authorize(altered, { nowUnix, verifyAgentAuthorization: mockAuthorize }));
    const evidence = {
      schema: 'qpc.cross-l1.settlement-evidence', version: 1, authorizationCommitment,
      evidenceClass: envelope.settlement.evidenceClass,
      destinationNetworkId: envelope.destination.networkId, destinationAdapterId: envelope.destination.adapterId,
      destinationAssetId: envelope.asset.destinationAssetId, amountAtomic: envelope.amount.destinationAtomic,
      recipientCommitment: envelope.destination.recipientCommitment,
      transactionId: 'SIMULATED-NOT-A-TRANSACTION', inclusionReference: 'SIMULATED-NOT-A-BLOCK',
      confirmations: envelope.settlement.minConfirmations, finalityStatus: 'FINALIZED',
      proofDigest: 'e'.repeat(64), observedAtUnix: nowUnix,
    };
    // Fixture equality only: no chain-specific evidence verifier is implemented here.
    const mockEvidenceVerifier = async ({ evidence: candidate }) => JSON.stringify(candidate) === JSON.stringify(evidence);
    for (const [id, title, patch, expected] of [
      ['wrong-chain', 'Evidence from another chain', { destinationNetworkId: 'eip155:11155111' }, 'destination network mismatch'],
      ['wrong-recipient', 'Evidence pays another recipient', { recipientCommitment: 'c'.repeat(64) }, 'recipient commitment mismatch'],
      ['wrong-amount', 'Evidence changes the amount', { amountAtomic: '1' }, 'settlement amount mismatch'],
      ['forged-evidence', 'Well-shaped fabricated proof', { proofDigest: 'f'.repeat(64) }, 'chain-specific evidence verification failed'],
    ]) {
      await scenario(id, title, 'verifyCrossL1SettlementEvidence', expected, () => verifyCrossL1SettlementEvidence({
        authorization: envelope, evidence: { ...evidence, ...patch }, verifyDestinationEvidence: mockEvidenceVerifier,
      }));
    }
    await scenario('matching-mock-evidence', 'Matching synthetic evidence', 'verifyCrossL1SettlementEvidence', null, async () => {
      await verifyCrossL1SettlementEvidence({ authorization: envelope, evidence, verifyDestinationEvidence: mockEvidenceVerifier });
      return { state: 'MOCK_VALIDATED_NOT_SETTLED', detail: 'Bindings and explicit mock fixture equality passed. Synthetic FINALIZED is input data, not observed chain finality.' };
    });
    await scenario('settlement-blocked', 'Try to claim native settlement', 'settleNativeTransfer', 'Settlement BLOCKED', () => settleNativeTransfer());
    const persisted = await new FileIntentStore(root).get(authorizationCommitment);
    return {
      schema: 'qpc.phase2.simulation.v1', mode: 'SIMULATED', nowUnix,
      source: 'docs/fixtures/cross-l1-v1.json', authorizationCommitment,
      sourceNetwork: envelope.source.networkId, destinationNetwork: envelope.destination.networkId,
      amountAtomic: envelope.amount.destinationAtomic, recipientCommitment: envelope.destination.recipientCommitment,
      realTransactions: 0, networkRequests: 0, realSettlements: 0,
      mocks: ['Agent identity: exact fixture commitment allowlist, not cryptographic authentication', 'Destination evidence: exact synthetic fixture equality, not a chain verifier'],
      replayStorage: 'Real FileIntentStore; fresh local temporary directory per run, removed afterward',
      unsignedDispatch: { state: dispatch.state, methodName: dispatch.qpcCall.methodName, execution: dispatch.execution },
      mockVerifierCalls, persistedState: persisted.state, history: persisted.history, cases,
    };
  } finally { await rm(root, { recursive: true, force: true }); }
}

export async function generateDemo(output = fileURLToPath(new URL('./generated/', import.meta.url))) {
  const report = await runSimulation();
  await mkdir(output, { recursive: true });
  await writeFile(join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n');
  await writeFile(join(output, 'index.html'), renderDashboard(report));
  return report;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const args = process.argv.slice(2);
  if (args.length > 1 || args[0]?.startsWith('-')) throw new Error('Usage: node agent-router/demo/simulate.mjs [output-directory]');
  const report = await generateDemo(args[0]);
  console.log(`SIMULATED: ${report.cases.length} expected outcomes verified; 0 real transactions; 0 real settlements.`);
}
