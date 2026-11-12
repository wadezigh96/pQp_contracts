// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {QPCPolicyCore} from "../../contracts/shared/QPCPolicyCore.sol";

contract StatefulTarget {
    uint256 public calls;
    function ping() external payable { calls += msg.value; }
}

contract StatefulAgentActor {
    QPCPolicyCore public immutable qpc;

    constructor(address payable _qpc) { qpc = QPCPolicyCore(_qpc); }

    function execute(address target, uint256 value, bytes calldata data) external {
        qpc.execute(target, value, data);
    }
}

contract QPCPolicyCoreHandler {
    QPCPolicyCore public immutable qpc;
    StatefulTarget public immutable target;
    StatefulAgentActor public immutable agent;
    uint256 public configuredCap;
    uint256 public configuredSpent;

    constructor() {
        qpc = new QPCPolicyCore(address(this));
        target = new StatefulTarget();
        agent = new StatefulAgentActor(address(qpc));
    }

    function configure(uint256 cap, uint256 txLimit) external {
        cap = bound(cap, 1, 100 ether);
        txLimit = bound(txLimit, 1, cap);
        uint64 expiry = uint64(block.timestamp + 1 days);
        qpc.configureAgent(address(agent), expiry, cap, txLimit);
        qpc.setCallPermission(address(agent), address(target), StatefulTarget.ping.selector, true);
        configuredCap = cap;
        configuredSpent = 0;
    }

    function execute(uint256 amount) external {
        (bool active, uint64 expiry,,,) = qpc.agents(address(agent));
        if (!active || expiry <= block.timestamp) return;
        (,, uint256 cap, uint256 spent, uint256 txLimit) = qpc.agents(address(agent));
        if (spent >= cap || txLimit == 0) return;
        uint256 remaining = cap - spent;
        uint256 maxAmount = remaining < txLimit ? remaining : txLimit;
        amount = bound(amount, 0, maxAmount);
        if (amount == 0) return;

        (bool funded,) = address(qpc).call{value: amount}("");
        if (!funded) return;

        (bool ok,) = address(agent).call(abi.encodeWithSelector(
            StatefulAgentActor.execute.selector,
            address(target),
            amount,
            abi.encodeWithSelector(StatefulTarget.ping.selector)
        ));
        if (ok) configuredSpent += amount;
    }

    function revoke() external { qpc.revokeAgent(address(agent)); }
    function pause() external { qpc.setPaused(true); }
    function unpause() external { qpc.setPaused(false); }

    function bound(uint256 x, uint256 min, uint256 max) internal pure returns (uint256) {
        if (max <= min) return min;
        return min + (x % (max - min + 1));
    }

    function invariantAccounting() external view {
        (,, uint256 cap, uint256 spent,) = qpc.agents(address(agent));
        require(spent <= cap, "spent > cap");
        require(configuredSpent <= cap || cap == 0, "handler accounting > cap");
    }

    receive() external payable {}
}

contract QPCPolicyCoreStatefulTest {
    function testStatefulHandlerCanBeConstructed() public {
        QPCPolicyCoreHandler handler = new QPCPolicyCoreHandler();
        require(address(handler.qpc()) != address(0), "QPC not deployed");
        require(address(handler.target()) != address(0), "target not deployed");
        require(address(handler.agent()) != address(0), "agent not deployed");
    }
}
