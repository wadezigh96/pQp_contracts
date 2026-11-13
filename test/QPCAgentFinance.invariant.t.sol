// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {QPCAgentFinanceHandler} from "./QPCAgentFinance.stateful.t.sol";

interface InvariantVm {
    function deal(address account, uint256 newBalance) external;
    function targetContract(address target) external;
}

contract QPCAgentFinanceInvariantTest {
    InvariantVm internal constant vm = InvariantVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    QPCAgentFinanceHandler internal handler;

    function setUp() public {
        handler = new QPCAgentFinanceHandler();
        vm.deal(address(handler), 1_000 ether);
        vm.targetContract(address(handler));
    }

    function invariant_nativeSpendNeverExceedsCurrentCap() public view {
        (,, uint256 cap, uint256 spent,) = handler.qpc().agents(address(handler.agent()));
        require(spent <= cap, "native spent exceeded cap");
        require(handler.qpc().agentRemainingNative(address(handler.agent())) <= cap, "remaining exceeded cap");
        require(spent == handler.configuredSpent(), "handler and protocol accounting differ");
    }

    function invariant_targetReceiptsMatchSuccessfulExecution() public view {
        require(handler.target().calls() == handler.totalExecuted(), "target receipt accounting differs");
    }

    function invariant_expiredAgentHasNoRemainingNativeAllowance() public view {
        (bool active, uint64 expiry,,,) = handler.qpc().agents(address(handler.agent()));
        if (active && expiry != 0 && block.timestamp >= expiry) {
            require(handler.qpc().agentRemainingNative(address(handler.agent())) == 0, "expired allowance nonzero");
        }
    }
}
