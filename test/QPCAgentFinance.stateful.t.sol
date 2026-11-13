// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {QPCAgentFinance} from "../contracts/QPCAgentFinance.sol";

interface StatefulVm {
    function deal(address account, uint256 newBalance) external;
    function warp(uint256 newTimestamp) external;
}

contract StatefulTarget {
    uint256 public calls;

    function ping() external payable {
        calls += msg.value;
    }
}

contract StatefulAgentActor {
    QPCAgentFinance public immutable qpc;

    constructor(address payable _qpc) {
        qpc = QPCAgentFinance(_qpc);
    }

    function execute(address target, uint256 value, bytes calldata data) external {
        qpc.execute(target, value, data);
    }
}

/// @notice Fuzz handler: the QPC owner controls the agent and funds actual native executions.
contract QPCAgentFinanceHandler {
    StatefulVm internal constant vm = StatefulVm(address(uint160(uint256(keccak256("hevm cheat code")))));

    QPCAgentFinance public immutable qpc;
    StatefulTarget public immutable target;
    StatefulAgentActor public immutable agent;
    uint256 public configuredCap;
    uint256 public configuredSpent;
    uint256 public totalExecuted;

    constructor() {
        qpc = new QPCAgentFinance(address(this));
        target = new StatefulTarget();
        agent = new StatefulAgentActor(payable(address(qpc)));
    }

    function configure(uint256 cap, uint256 txLimit, uint256 duration) external {
        cap = bound(cap, 1, 100 ether);
        txLimit = bound(txLimit, 1, cap);
        duration = bound(duration, 1, qpc.MAX_POLICY_DURATION());
        (bool active,,,,) = qpc.agents(address(agent));
        if (active) qpc.revokeAgent(address(agent));
        uint64 expiry = uint64(block.timestamp + duration);
        qpc.configureAgent(address(agent), expiry, cap, txLimit);
        qpc.setCallPermission(address(agent), address(target), StatefulTarget.ping.selector, true);
        configuredCap = cap;
        configuredSpent = 0;
    }

    function execute(uint256 amount) external {
        (bool active, uint64 expiry, uint256 cap, uint256 spent, uint256 txLimit) = qpc.agents(address(agent));
        if (!active || expiry <= block.timestamp || qpc.paused() || spent >= cap || txLimit == 0) return;
        uint256 remaining = cap - spent;
        uint256 maxAmount = remaining < txLimit ? remaining : txLimit;
        amount = bound(amount, 0, maxAmount);
        if (amount == 0 || amount > address(this).balance) return;

        (bool funded,) = address(qpc).call{value: amount}("");
        if (!funded) return;
        (bool ok,) = address(agent).call(abi.encodeWithSelector(
            StatefulAgentActor.execute.selector,
            address(target),
            amount,
            abi.encodeWithSelector(StatefulTarget.ping.selector)
        ));
        if (ok) {
            configuredSpent += amount;
            totalExecuted += amount;
        }
    }

    function revoke() external {
        qpc.revokeAgent(address(agent));
    }

    function pause() external {
        qpc.setPaused(true);
    }

    function unpause() external {
        qpc.setPaused(false);
    }

    function advanceTime(uint256 secondsForward) external {
        secondsForward = bound(secondsForward, 1, qpc.MAX_POLICY_DURATION());
        vm.warp(block.timestamp + secondsForward);
    }

    function bound(uint256 x, uint256 min, uint256 max) internal pure returns (uint256) {
        if (max <= min) return min;
        return min + (x % (max - min + 1));
    }

    receive() external payable {}
}

contract QPCAgentFinanceStatefulTest {
    StatefulVm internal constant vm = StatefulVm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function testStatefulHandlerExecutesAndAccounts() public {
        QPCAgentFinanceHandler handler = new QPCAgentFinanceHandler();
        vm.deal(address(handler), 10 ether);
        handler.configure(5 ether, 2 ether, 1 days);
        handler.execute(1 ether);

        (,, uint256 cap, uint256 spent,) = handler.qpc().agents(address(handler.agent()));
        require(cap == 5 ether && spent == 1 ether, "policy accounting mismatch");
        require(handler.target().calls() == 1 ether, "target did not receive execution");
        require(handler.configuredSpent() == 1 ether, "handler accounting mismatch");
    }
}
