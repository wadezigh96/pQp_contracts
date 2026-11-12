// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {EthereumQPCSettlement} from "../../contracts/ethereum/EthereumQPCSettlement.sol";

interface VmSettlement {
    function prank(address) external;
    function chainId(uint256) external;
    function warp(uint256) external;
    function addr(uint256) external returns (address);
    function sign(uint256, bytes32) external returns (uint8, bytes32, bytes32);
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

    uint256 internal constant KEY_A = 0xA11CE;
    uint256 internal constant KEY_B = 0xB0B;
    uint256 internal constant KEY_C = 0xCAFE;
    uint256 internal constant KEY_ATTACKER = 0xBAD;
    address internal constant OWNER = address(0x1001);
    address internal constant RECIPIENT = address(0x1003);

    EthereumQPCSettlement internal vault;
    SettlementMockToken internal token;
    bytes32 internal constant DOMAIN = keccak256("near:mainnet");
    bytes32 internal constant PAYMENT = keccak256("qpc-payment-001");
    bytes32 internal constant SOURCE_TX = keccak256("near-tx-hash-001");

    function setUp() public {
        vm.chainId(11155111);
        address[] memory validators = new address[](3);
        validators[0] = vm.addr(KEY_A);
        validators[1] = vm.addr(KEY_B);
        validators[2] = vm.addr(KEY_C);
        vault = new EthereumQPCSettlement(OWNER, validators, 2, 11155111);
        token = new SettlementMockToken();
        vm.prank(OWNER);
        vault.setTokenAllowed(address(token), true);
        token.mint(address(vault), 1_000_000);
    }

    function digest(bytes32 paymentId, bytes32 sourceTx, uint256 amount, uint256 deadline)
        internal view returns (bytes32)
    {
        return vault.attestationDigest(
            DOMAIN, paymentId, sourceTx, address(token), RECIPIENT, amount, deadline
        );
    }

    function signaturesFor(uint256 key1, uint256 key2, bytes32 message)
        internal returns (bytes[] memory signatures)
    {
        signatures = new bytes[](2);
        (uint8 v1, bytes32 r1, bytes32 s1) = vm.sign(key1, message);
        (uint8 v2, bytes32 r2, bytes32 s2) = vm.sign(key2, message);
        address a1 = ecrecover(message, v1, r1, s1);
        address a2 = ecrecover(message, v2, r2, s2);
        if (a1 < a2) {
            signatures[0] = abi.encodePacked(r1, s1, v1);
            signatures[1] = abi.encodePacked(r2, s2, v2);
        } else {
            signatures[0] = abi.encodePacked(r2, s2, v2);
            signatures[1] = abi.encodePacked(r1, s1, v1);
        }
    }

    function settlementSignatures(bytes32 paymentId, bytes32 sourceTx, uint256 amount)
        internal returns (bytes[] memory)
    {
        uint256 deadline = block.timestamp + 1 days;
        return signaturesFor(KEY_A, KEY_B, digest(paymentId, sourceTx, amount, deadline));
    }

    function settleAsQuorum(bytes32 paymentId, bytes32 sourceTx, uint256 amount) internal {
        uint256 deadline = block.timestamp + 1 days;
        bytes[] memory signatures = signaturesFor(
            KEY_A, KEY_B, digest(paymentId, sourceTx, amount, deadline)
        );
        vault.settle(DOMAIN, paymentId, sourceTx, address(token), RECIPIENT, amount, deadline, signatures);
    }

    function testValidatorQuorumCanPayRecipient() public {
        settleAsQuorum(PAYMENT, SOURCE_TX, 500);
        require(token.balanceOf(RECIPIENT) == 500, "recipient not paid");
        require(vault.consumedSourceTransactions(SOURCE_TX), "source tx not consumed");
    }

    function testOneSignatureCannotMeetTwoOfThreeThreshold() public {
        uint256 deadline = block.timestamp + 1 days;
        bytes[] memory signatures = new bytes[](1);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(
            KEY_A, digest(PAYMENT, SOURCE_TX, 500, deadline)
        );
        signatures[0] = abi.encodePacked(r, s, v);
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, PAYMENT, SOURCE_TX, address(token),
            RECIPIENT, 500, deadline, signatures
        ));
        require(!ok, "single signature met quorum");
    }

    function testNonValidatorSignatureRejected() public {
        uint256 deadline = block.timestamp + 1 days;
        bytes[] memory signatures = signaturesFor(
            KEY_A, KEY_ATTACKER, digest(PAYMENT, SOURCE_TX, 500, deadline)
        );
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, PAYMENT, SOURCE_TX, address(token),
            RECIPIENT, 500, deadline, signatures
        ));
        require(!ok, "non-validator accepted");
    }

    function testDuplicateSignerRejected() public {
        uint256 deadline = block.timestamp + 1 days;
        bytes32 message = digest(PAYMENT, SOURCE_TX, 500, deadline);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(KEY_A, message);
        bytes[] memory signatures = new bytes[](2);
        signatures[0] = abi.encodePacked(r, s, v);
        signatures[1] = abi.encodePacked(r, s, v);
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, PAYMENT, SOURCE_TX, address(token),
            RECIPIENT, 500, deadline, signatures
        ));
        require(!ok, "duplicate signer accepted");
    }

    function testSignatureCannotBeReusedForDifferentRecipient() public {
        uint256 deadline = block.timestamp + 1 days;
        bytes[] memory signatures = signaturesFor(
            KEY_A, KEY_B, digest(PAYMENT, SOURCE_TX, 500, deadline)
        );
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, PAYMENT, SOURCE_TX, address(token),
            address(0x9999), 500, deadline, signatures
        ));
        require(!ok, "signature reused for changed recipient");
    }

    function testPaymentReplayRejected() public {
        settleAsQuorum(PAYMENT, SOURCE_TX, 500);
        uint256 deadline = block.timestamp + 1 days;
        bytes[] memory signatures = signaturesFor(
            KEY_A, KEY_B, digest(PAYMENT, keccak256("different-tx"), 500, deadline)
        );
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, PAYMENT, keccak256("different-tx"),
            address(token), RECIPIENT, 500, deadline, signatures
        ));
        require(!ok, "payment replay accepted");
    }

    function testSourceTransactionReplayRejectedAcrossPaymentIds() public {
        settleAsQuorum(PAYMENT, SOURCE_TX, 500);
        uint256 deadline = block.timestamp + 1 days;
        bytes32 nextPayment = keccak256("different-payment");
        bytes[] memory signatures = signaturesFor(
            KEY_A, KEY_B, digest(nextPayment, SOURCE_TX, 500, deadline)
        );
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, nextPayment, SOURCE_TX,
            address(token), RECIPIENT, 500, deadline, signatures
        ));
        require(!ok, "source transaction replay accepted");
    }

    function testDisallowedTokenRejected() public {
        SettlementMockToken other = new SettlementMockToken();
        uint256 deadline = block.timestamp + 1 days;
        bytes[] memory signatures = signaturesFor(
            KEY_A, KEY_B, keccak256("unused")
        );
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, PAYMENT, SOURCE_TX, address(other),
            RECIPIENT, 500, deadline, signatures
        ));
        require(!ok, "disallowed token accepted");
    }

    function testExpiredSettlementRejected() public {
        uint256 deadline = block.timestamp + 1;
        bytes[] memory signatures = signaturesFor(
            KEY_A, KEY_B, digest(PAYMENT, SOURCE_TX, 500, deadline)
        );
        vm.warp(deadline + 1);
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, PAYMENT, SOURCE_TX, address(token),
            RECIPIENT, 500, deadline, signatures
        ));
        require(!ok, "expired settlement accepted");
    }

    function testPausedSettlementRejected() public {
        vm.prank(OWNER);
        vault.setPaused(true);
        uint256 deadline = block.timestamp + 1 days;
        bytes[] memory signatures = signaturesFor(
            KEY_A, KEY_B, digest(PAYMENT, SOURCE_TX, 500, deadline)
        );
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, PAYMENT, SOURCE_TX, address(token),
            RECIPIENT, 500, deadline, signatures
        ));
        require(!ok, "paused settlement accepted");
    }

    function testTokenTransferFailureDoesNotConsumeReplayKeys() public {
        token.setFailTransfers(true);
        uint256 deadline = block.timestamp + 1 days;
        bytes[] memory signatures = signaturesFor(
            KEY_A, KEY_B, digest(PAYMENT, SOURCE_TX, 500, deadline)
        );
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.settle.selector, DOMAIN, PAYMENT, SOURCE_TX, address(token),
            RECIPIENT, 500, deadline, signatures
        ));
        require(!ok, "failed token transfer accepted");
        require(!vault.consumedSourcePayments(keccak256(abi.encode(DOMAIN, PAYMENT))),
            "failed payment consumed payment key");
        require(!vault.consumedSourceTransactions(SOURCE_TX), "failed payment consumed tx key");
    }

    function testOwnerCannotRemoveValidatorBelowThreshold() public {
        vm.prank(OWNER);
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.setValidator.selector, vm.addr(KEY_C), false
        ));
        require(!ok, "validator set dropped below threshold");
    }

    function testOwnerCanRotateValidatorWithoutDroppingThreshold() public {
        address replacement = address(0xCAFE);
        vm.prank(OWNER);
        vault.setValidator(vm.addr(KEY_C), false);
        vm.prank(OWNER);
        vault.setValidator(replacement, true);
        require(!vault.isValidator(vm.addr(KEY_C)), "old validator remains");
        require(vault.isValidator(replacement), "replacement missing");
        require(vault.validatorCount() == 3, "validator count mismatch");
    }

    function testNonOwnerCannotChangeValidatorSet() public {
        vm.prank(address(0xDEAD));
        (bool ok,) = address(vault).call(abi.encodeWithSelector(
            vault.setValidator.selector, address(0xBEEF), true
        ));
        require(!ok, "non-owner changed validator set");
    }

    function testRejectsDeploymentOnWrongChainId() public {
        vm.chainId(1);
        (bool ok,) = address(this).call(abi.encodeWithSelector(this.deployWrongChain.selector));
        require(ok, "test wrapper failed");
    }

    function deployWrongChain() external {
        bool deployed;
        address[] memory validators = new address[](2);
        validators[0] = vm.addr(KEY_A);
        validators[1] = vm.addr(KEY_B);
        try new EthereumQPCSettlement(OWNER, validators, 2, 11155111) returns (EthereumQPCSettlement) {
            deployed = true;
        } catch {}
        require(!deployed, "wrong chain accepted");
    }
}
