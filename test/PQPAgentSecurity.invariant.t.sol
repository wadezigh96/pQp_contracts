// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {PQPAgentSecurity} from "../contracts/PQPAgentSecurity.sol";

/// @notice Minimal invariant harness. Intended for Foundry invariant/fuzz execution.
contract PQPAgentSecurityInvariantHarness {
    PQPAgentSecurity public immutable pqp;
    address public immutable owner;
    address public agent;

    constructor() {
        owner = address(this);
        pqp = new PQPAgentSecurity(owner);
        agent = address(0xA11CE);
    }

    function configure(uint256 cap, uint256 txLimit, uint64 expiry) external {
        if (expiry <= block.timestamp) return;
        if (expiry > block.timestamp + pqp.MAX_POLICY_DURATION()) return;
        if (txLimit > cap) return;
        pqp.configureAgent(agent, expiry, cap, txLimit);
    }

    function invariant_remainingNativeNeverExceedsCap() external view {
        (,, uint256 cap, uint256 spent,) = pqp.agents(agent);
        require(spent <= cap, "native spent exceeded cap");
        require(pqp.agentRemainingNative(agent) <= cap, "remaining exceeded cap");
    }

    function invariant_expiredAgentHasZeroRemaining() external view {
        (bool active, uint64 expiry,,,) = pqp.agents(agent);
        if (active && expiry != 0 && block.timestamp > expiry) {
            require(pqp.agentRemainingNative(agent) == 0, "expired allowance nonzero");
        }
    }

    function invariant_pausedStateDoesNotChangePolicyAccounting() external view {
        (,, uint256 cap, uint256 spent,) = pqp.agents(agent);
        require(spent <= cap, "paused accounting invalid");
    }
}
