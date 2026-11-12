// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {EthereumQPCSettlement} from "../../contracts/ethereum/EthereumQPCSettlement.sol";

interface VmSettlement {
    function prank(address) external;
    function chainId(uint256) external;
    function warp(uint256) external;
}

contract SettlementMockToken {
    mapping(address => uint256) public balanceOf;
    bool public failTransfers;

    function mint(address to, uint256 amount) external { balanceOf[to] += amount; }
    function setFailTransfers(bool value) external { failTransfers = value; }
    function transfer(address to, uint256 amount) external returns (bool) {
        if (failTransfers || balanceOf[msg.sender] < amount) return false;
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        return true;
    }
}

contract EthereumQPCSettlementTest {
    VmSettlement internal constant vm =
        VmSettlement(address(uint160(uint256(keccak256("hevm cheat code")))));

    address internal constant OWNER = address(0x1001);
    address internal constant RELAYER = address(0x1002);
    address internal constant RECIPIENT = address(0x1003);
    address internal constant ATTACKER = address(0x1004);

    EthereumQPCSettlement internal vault;
    SettlementMockToken internal token;
    bytes32 internal constant DOMAIN = keccak256("near:mainnet");
    bytes32 internal constant PAYMENT = keccak256("qpc-payment-001");
    bytes32 internal constant SOURCE_TX = keccak256("near-tx-hash-001");

    function setUp() public {
        vm.chainId(11155111); // Ethereum Sepolia test chain ID
        vault = new EthereumQPCSettlement(OWNER, RELAYER, 11155111);
        token = new SettlementMockToken();
        vm.prank(OWNER);
        vault.setTokenAllowed(address(token), true);
        token.mint(address(vault), 1_000_000);
    }

    function settleAsRelayer(bytes32 paymentId, bytes32 sourceTx, uint256 amount) internal {
        vm.prank(RELAYER);
        vault.settle(DOMAIN, paymentId, sourceTx, address(token), RECIPIENT, amount, block.timestamp + 1 days);
    }

    function testRelayerCanPayRecipientFromPrefundedLiquidity() public {
        settleAsRelayer(PAYMENT, SOURCE_TX, 500);
        require(token.balanceOf(RECIPIENT) == 500, "recipient not paid");
        require(vault.consumedSourceTransactions(SOURCE_TX), "source tx not consumed");
    }

    function testOnlyRelayerCanSettle() public {
        vm.prank(ATTACKER);
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, PAYMENT, SOURCE_TX, address(token),
            RECIPIENT, 500, block.timestamp + 1 days
        ));
        require(!ok, "unauthorized settlement");
    }

    function testPaymentReplayRejected() public {
        settleAsRelayer(PAYMENT, SOURCE_TX, 500);
        vm.prank(RELAYER);
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, PAYMENT, keccak256("near-tx-hash-002"),
            address(token), RECIPIENT, 500, block.timestamp + 1 days
        ));
        require(!ok, "payment replay accepted");
    }

    function testSourceTransactionReplayRejectedAcrossPaymentIds() public {
        settleAsRelayer(PAYMENT, SOURCE_TX, 500);
        vm.prank(RELAYER);
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, keccak256("different-payment"),
            SOURCE_TX, address(token), RECIPIENT, 500, block.timestamp + 1 days
        ));
        require(!ok, "source transaction replay accepted");
    }

    function testDisallowedTokenRejected() public {
        SettlementMockToken other = new SettlementMockToken();
        vm.prank(RELAYER);
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, PAYMENT, SOURCE_TX, address(other),
            RECIPIENT, 500, block.timestamp + 1 days
        ));
        require(!ok, "disallowed token accepted");
    }

    function testExpiredSettlementRejected() public {
        vm.warp(block.timestamp + 2 days);
        vm.prank(RELAYER);
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, PAYMENT, SOURCE_TX, address(token),
            RECIPIENT, 500, block.timestamp - 1
        ));
        require(!ok, "expired settlement accepted");
    }

    function testPausedSettlementRejected() public {
        vm.prank(OWNER);
        vault.setPaused(true);
        vm.prank(RELAYER);
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, PAYMENT, SOURCE_TX, address(token),
            RECIPIENT, 500, block.timestamp + 1 days
        ));
        require(!ok, "paused settlement accepted");
    }

    function testTokenTransferFailureDoesNotConsumeReplayKeys() public {
        token.setFailTransfers(true);
        vm.prank(RELAYER);
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, PAYMENT, SOURCE_TX, address(token),
            RECIPIENT, 500, block.timestamp + 1 days
        ));
        require(!ok, "failed token transfer accepted");
        require(!vault.consumedSourcePayments(keccak256(abi.encode(DOMAIN, PAYMENT))),
            "failed payment consumed payment key");
        require(!vault.consumedSourceTransactions(SOURCE_TX), "failed payment consumed tx key");
    }

    function testRejectsDeploymentOnWrongChainId() public {
        vm.chainId(1);
        (bool ok,) = address(this).call(abi.encodeWithSelector(
            this.deployWrongChain.selector
        ));
        require(ok, "test wrapper failed");
    }

    function deployWrongChain() external {
        bool deployed;
        try new EthereumQPCSettlement(OWNER, RELAYER, 11155111) returns (EthereumQPCSettlement) {
            deployed = true;
        } catch {}
        require(!deployed, "wrong chain accepted");
    }
}
