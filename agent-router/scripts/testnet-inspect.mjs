#!/usr/bin/env node
// Usage: node agent-router/scripts/testnet-inspect.mjs [near ethereum bsc solana zcash]
// Read-only identity checks only: no wallet, key, signing or submission.
import { createTestnetAdapter, inspectTestnetNetwork, TESTNET_NETWORKS } from '../src/testnet-adapters.mjs';

const chains = process.argv.slice(2);
if (!chains.length) chains.push(...Object.keys(TESTNET_NETWORKS));
if (chains.some(chain => !Object.hasOwn(TESTNET_NETWORKS, chain))) {
  console.error('Supported testnets: near ethereum bsc solana zcash');
  process.exitCode = 2;
} else {
  const results = await Promise.all(chains.map(async chain => {
    const network = TESTNET_NETWORKS[chain];
    // No transfer mapping is configured: this CLI cannot prepare any payment.
    const adapter = createTestnetAdapter({ chain, trustedProfile: {
      destinationNetworkId: network.networkId, destinationNativeChainId: network.nativeChainId,
    } });
    return { chain, endpoint: network.rpcUrl, ...await inspectTestnetNetwork(adapter) };
  }));
  console.log(JSON.stringify({ readOnly: true, settlementImplemented: false, results }, null, 2));
  if (results.some(result => result.state === 'NOT_VERIFIED')) process.exitCode = 1;
}
