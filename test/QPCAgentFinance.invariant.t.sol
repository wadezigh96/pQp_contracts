// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {QPCAgentFinance} from "../contracts/QPCAgentFinance.sol";

/// @notice Minimal invariant harness. Intended for Foundry invariant/fuzz execution.
contract QPCAgentFinanceInvariantHarness {
    QPCAgentFinance public immutable qpc;
    address public immutable owner;
    address public agent;

    constructor() {
        owner = address(this);
        qpc = new QPCAgentFinance(owner);
        agent = address(0xA11CE);
    }

    function configure(uint256 cap, uint256 txLimit, uint64 expiry) external {
        if (expiry <= block.timestamp) return;
        if (expiry > block.timestamp + qpc.MAX_POLICY_DURATION()) return;
        if (txLimit > cap) return;
        qpc.configureAgent(agent, expiry, cap, txLimit);
    }

    function invariant_remainingNativeNeverExceedsCap() external view {
        (,, uint256 cap, uint256 spent,) = qpc.agents(agent);
        require(spent <= cap, "native spent exceeded cap");
        require(qpc.agentRemainingNative(agent) <= cap, "remaining exceeded cap");
    }

    function invariant_expiredAgentHasZeroRemaining() external view {
        (bool active, uint64 expiry,,,) = qpc.agents(agent);
        if (active && expiry != 0 && block.timestamp > expiry) {
            require(qpc.agentRemainingNative(agent) == 0, "expired allowance nonzero");
        }
    }

    function invariant_pausedStateDoesNotChangePolicyAccounting() external view {
        (,, uint256 cap, uint256 spent,) = qpc.agents(agent);
        require(spent <= cap, "paused accounting invalid");
    }
}
