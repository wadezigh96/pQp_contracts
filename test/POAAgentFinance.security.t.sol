// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {POAAgentFinance} from "../contracts/POAAgentFinance.sol";

interface Vm {
    function prank(address) external;
    function warp(uint256) external;
}

contract MockToken {
    mapping(address => uint256) public balanceOf;
    bool public returnFalse;

    function mint(address to, uint256 amount) external { balanceOf[to] += amount; }
    function setReturnFalse(bool value) external { returnFalse = value; }
    function transfer(address to, uint256 amount) external returns (bool) {
        if (returnFalse || balanceOf[msg.sender] < amount) return false;
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        return true;
    }
}

contract ReentrantTarget {
    POAAgentFinance internal immutable poa;
    bool public attempted;

    constructor(address payable _poa) { poa = POAAgentFinance(_poa); }

    function attack() external payable {
        attempted = true;
        (bool ok,) = address(poa).call(
            abi.encodeWithSelector(
                poa.execute.selector,
                address(this),
                0,
                abi.encodeWithSelector(this.attack.selector)
            )
        );
        require(!ok, "reentrancy succeeded");
    }
}

contract POAAgentFinanceSecurityTest {
    Vm internal constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    POAAgentFinance internal poa;
    ReentrantTarget internal target;

    address internal constant OWNER = address(0x1001);
    address internal constant AGENT = address(0x1002);
    address internal constant RECIPIENT = address(0x1003);
    address internal constant NEW_OWNER = address(0x2001);
    address internal constant TOKEN = address(0x3001);
    MockToken internal token;

    function setUp() public {
        poa = new POAAgentFinance(OWNER);
        target = new ReentrantTarget(payable(address(poa)));
        token = new MockToken();
    }

    function configure(uint256 cap, uint256 txLimit, uint256 duration) internal {
        vm.prank(OWNER);
        poa.configureAgent(AGENT, uint64(block.timestamp + duration), cap, txLimit);
    }

    function allowTarget() internal {
        vm.prank(OWNER);
        poa.setCallPermission(AGENT, address(target), ReentrantTarget.attack.selector, true);
    }

    function fund(uint256 amount) internal {
        (bool ok,) = address(poa).call{value: amount}("");
        require(ok, "funding failed");
    }

    function callAsAgent(uint256 value) internal returns (bool ok) {
        vm.prank(AGENT);
        (ok,) = address(poa).call(
            abi.encodeWithSelector(
                poa.execute.selector,
                address(target),
                value,
                abi.encodeWithSelector(target.attack.selector)
            )
        );
    }

    function testOwner() public view { require(poa.owner() == OWNER, "owner"); }

    function testTxLimit() public {
        configure(10 ether, 1 ether, 1 days);
        allowTarget();
        fund(2 ether);
        require(!callAsAgent(2 ether), "tx limit bypass");
    }

    function testWithinLimit() public {
        configure(2 ether, 1 ether, 1 days);
        allowTarget();
        fund(1 ether);
        require(callAsAgent(1 ether), "valid execution rejected");
        require(target.attempted(), "target not called");
    }

    function testExpiry() public {
        configure(10 ether, 1 ether, 1 days);
        allowTarget();
        fund(1 ether);
        vm.warp(block.timestamp + 1 days);
        require(!callAsAgent(1 ether), "expired execution allowed");
        require(poa.agentRemainingNative(AGENT) == 0, "expired allowance");
    }

    function testRevoke() public {
        configure(10 ether, 1 ether, 1 days);
        allowTarget();
        fund(1 ether);
        vm.prank(OWNER);
        poa.revokeAgent(AGENT);
        require(!callAsAgent(1 ether), "revoked execution allowed");
    }

    function testPause() public {
        configure(10 ether, 1 ether, 1 days);
        allowTarget();
        fund(1 ether);
        vm.prank(OWNER);
        poa.setPaused(true);
        require(!callAsAgent(1 ether), "pause bypass");
    }

    function testUnauthorizedSelector() public {
        configure(10 ether, 1 ether, 1 days);
        fund(1 ether);
        require(!callAsAgent(1 ether), "selector bypass");
    }


    function testCallPermissionCanBeRevoked() public {
        configure(10 ether, 1 ether, 1 days);
        allowTarget();
        fund(1 ether);

        vm.prank(OWNER);
        poa.setCallPermission(AGENT, address(target), ReentrantTarget.attack.selector, false);

        require(!callAsAgent(1 ether), "revoked call permission still works");
    }

    function testExpiredAgentCanBeReconfigured() public {
        configure(10 ether, 1 ether, 1 days);

        vm.warp(block.timestamp + 1 days);

        vm.prank(OWNER);
        poa.configureAgent(AGENT, uint64(block.timestamp + 2 days), 20 ether, 5 ether);

        (bool active,,uint256 cap,,uint256 txLimit) = poa.agents(AGENT);
        require(active && cap == 20 ether && txLimit == 5 ether, "expired reconfigure failed");
    }

    function testOwnershipHandshake() public {
        vm.prank(OWNER);
        poa.transferOwnership(NEW_OWNER);
        require(poa.owner() == OWNER, "early ownership change");
        vm.prank(NEW_OWNER);
        poa.acceptOwnership();
        require(poa.owner() == NEW_OWNER, "ownership not accepted");
    }

    function testReentrancy() public {
        configure(10 ether, 1 ether, 1 days);
        allowTarget();
        fund(1 ether);
        require(callAsAgent(1 ether), "outer call failed");
        require(target.attempted(), "attack target not reached");
    }

    function testRejectZeroNativePolicy() public {
        vm.prank(OWNER);
        (bool ok,) = address(poa).call(
            abi.encodeWithSelector(
                poa.configureAgent.selector,
                AGENT,
                uint64(block.timestamp + 1 days),
                0,
                0
            )
        );
        require(!ok, "zero native policy accepted");
    }

    function testRejectTxLimitAboveNativeCap() public {
        vm.prank(OWNER);
        (bool ok,) = address(poa).call(
            abi.encodeWithSelector(
                poa.configureAgent.selector,
                AGENT,
                uint64(block.timestamp + 1 days),
                1 ether,
                2 ether
            )
        );
        require(!ok, "tx limit above cap accepted");
    }

    function testRejectZeroSelector() public {
        vm.prank(OWNER);
        (bool ok,) = address(poa).call(
            abi.encodeWithSelector(
                poa.setCallPermission.selector,
                AGENT,
                address(target),
                bytes4(0),
                true
            )
        );
        require(!ok, "zero selector accepted");
    }

    function testRejectOwnerAsTarget() public {
        vm.prank(OWNER);
        (bool ok,) = address(poa).call(
            abi.encodeWithSelector(
                poa.setCallPermission.selector,
                AGENT,
                OWNER,
                ReentrantTarget.attack.selector,
                true
            )
        );
        require(!ok, "owner target accepted");
    }

    function testRejectZeroTokenPolicy() public {
        vm.prank(OWNER);
        (bool ok,) = address(poa).call(
            abi.encodeWithSelector(
                poa.setTokenPolicy.selector,
                AGENT,
                TOKEN,
                0,
                0
            )
        );
        require(!ok, "zero token policy accepted");
    }

    function testRejectZeroTokenTransfer() public {
        configure(10 ether, 1 ether, 1 days);
        vm.prank(OWNER);
        poa.setTokenPolicy(AGENT, TOKEN, 100, 100);
        vm.prank(OWNER);
        poa.setRecipientPermission(AGENT, RECIPIENT, true);

        vm.prank(AGENT);
        (bool ok,) = address(poa).call(
            abi.encodeWithSelector(
                poa.transferToken.selector,
                TOKEN,
                RECIPIENT,
                0
            )
        );
        require(!ok, "zero token transfer accepted");
    }

    receive() external payable {}
}


contract POAAgentFinanceTokenSecurityTest {
    Vm internal constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    POAAgentFinance internal poa;
    MockToken internal token;

    address internal constant OWNER = address(0x5001);
    address internal constant AGENT = address(0x5002);
    address internal constant RECIPIENT = address(0x5003);

    function setUp() public {
        poa = new POAAgentFinance(OWNER);
        token = new MockToken();
    }

    function configureToken(uint256 cap, uint256 txLimit) internal {
        vm.prank(OWNER);
        poa.setTokenPolicy(AGENT, address(token), cap, txLimit);
        vm.prank(OWNER);
        poa.setRecipientPermission(AGENT, RECIPIENT, true);
    }

    function activateAgent() internal {
        vm.prank(OWNER);
        poa.configureAgent(AGENT, uint64(block.timestamp + 1 days), 10 ether, 10 ether);
    }

    function testTokenTransferWithinLimit() public {
        configureToken(100, 40);
        activateAgent();
        token.mint(address(poa), 100);

        vm.prank(AGENT);
        poa.transferToken(address(token), RECIPIENT, 40);

        require(token.balanceOf(RECIPIENT) == 40, "token not transferred");
        require(poa.tokenRemaining(AGENT, address(token)) == 60, "remaining token cap wrong");
    }

    function testTokenTxLimitEnforced() public {
        configureToken(100, 40);
        activateAgent();
        token.mint(address(poa), 100);

        vm.prank(AGENT);
        (bool ok,) = address(poa).call(abi.encodeWithSelector(
            poa.transferToken.selector, address(token), RECIPIENT, 41
        ));
        require(!ok, "token tx limit bypass");
    }

    function testTokenCumulativeCapEnforced() public {
        configureToken(50, 50);
        activateAgent();
        token.mint(address(poa), 100);

        vm.prank(AGENT);
        poa.transferToken(address(token), RECIPIENT, 30);

        vm.prank(AGENT);
        (bool ok,) = address(poa).call(abi.encodeWithSelector(
            poa.transferToken.selector, address(token), RECIPIENT, 21
        ));
        require(!ok, "token cumulative cap bypass");
        require(poa.tokenRemaining(AGENT, address(token)) == 20, "token accounting changed");
    }

    function testRecipientAllowlistEnforced() public {
        configureToken(100, 100);
        activateAgent();
        token.mint(address(poa), 100);

        vm.prank(AGENT);
        (bool ok,) = address(poa).call(abi.encodeWithSelector(
            poa.transferToken.selector, address(token), address(0x9999), 1
        ));
        require(!ok, "recipient allowlist bypass");
    }

    function testFailedTokenTransferDoesNotConsumeCap() public {
        configureToken(100, 100);
        activateAgent();
        token.mint(address(poa), 100);
        token.setReturnFalse(true);

        vm.prank(AGENT);
        (bool ok,) = address(poa).call(abi.encodeWithSelector(
            poa.transferToken.selector, address(token), RECIPIENT, 25
        ));
        require(!ok, "failed transfer accepted");
        require(poa.tokenRemaining(AGENT, address(token)) == 100, "failed transfer consumed cap");
    }

    function testActiveAgentCannotResetNativeSpent() public {
        activateAgent();

        vm.prank(OWNER);
        (bool ok,) = address(poa).call(abi.encodeWithSelector(
            poa.configureAgent.selector,
            AGENT,
            uint64(block.timestamp + 2 days),
            100 ether,
            100 ether
        ));
        require(!ok, "active policy reset allowed");
    }

    function testActiveAgentCannotResetTokenSpent() public {
        configureToken(100, 100);
        activateAgent();
        token.mint(address(poa), 100);

        vm.prank(AGENT);
        poa.transferToken(address(token), RECIPIENT, 25);

        vm.prank(OWNER);
        (bool ok,) = address(poa).call(abi.encodeWithSelector(
            poa.setTokenPolicy.selector, AGENT, address(token), 1000, 1000
        ));
        require(!ok, "active token policy reset allowed");
        require(poa.tokenRemaining(AGENT, address(token)) == 75, "token spent reset");
    }

    function testRevokeThenReconfigureIsAllowed() public {
        activateAgent();

        vm.prank(OWNER);
        poa.revokeAgent(AGENT);

        vm.prank(OWNER);
        poa.configureAgent(AGENT, uint64(block.timestamp + 2 days), 20 ether, 5 ether);

        (bool active,,uint256 cap,,uint256 txLimit) = poa.agents(AGENT);
        require(active && cap == 20 ether && txLimit == 5 ether, "reconfigure failed");
    }
}
