// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {QPCNativeTestnetExecutor as Executor} from "../contracts/QPCNativeTestnetExecutor.sol";

interface NativeExecutorVm {
    function chainId(uint256) external;
    function warp(uint256) external;
    function deal(address, uint256) external;
    function prank(address) external;
    function expectRevert(bytes4) external;
    function expectEmit(bool, bool, bool, bool, address) external;
}

contract NativeToggleRecipient {
    bool public rejecting = true;
    uint256 public received;

    function accept() external {
        rejecting = false;
    }

    receive() external payable {
        require(!rejecting, "reject");
        received += msg.value;
    }
}

contract NativeReentrantAgent {
    Executor public immutable executor;
    bytes32 public commitment;
    bytes32 public secondCommitment;
    bool public entered;

    constructor(Executor target) {
        executor = target;
    }

    function run(bytes32 first, bytes32 second) external payable {
        commitment = first;
        secondCommitment = second;
        executor.execute{value: msg.value}(first);
    }

    receive() external payable {
        entered = true;
        require(executor.executed(commitment), "not consumed before call");
        (bool ok, bytes memory reason) =
            address(executor).call{value: msg.value}(abi.encodeCall(executor.execute, (commitment)));
        require(!ok && bytes4(reason) == Executor.Reentrancy.selector, "same intent reentry");
        (ok, reason) = address(executor).call{value: msg.value}(abi.encodeCall(executor.execute, (secondCommitment)));
        require(!ok && bytes4(reason) == Executor.Reentrancy.selector, "other intent reentry");
    }
}

/// @dev Local EVM mock runtime only. No deployment, signature or live RPC.
contract QPCNativeTestnetExecutorTest {
    NativeExecutorVm private constant vm = NativeExecutorVm(address(uint160(uint256(keccak256("hevm cheat code")))));
    address private constant OWNER = address(0x1001);
    address private constant AGENT = address(0x1002);
    address private constant RECIPIENT = address(0x1003);
    bytes32 private constant COMMITMENT = 0x8f09314e4136b751c746d6b83fd51f37daabf6776348c2325460eb4a6da29113;
    // SHA256(domain + UTF8 ID), matching the canonical-envelope JS builder.
    bytes32 private constant POLICY = 0xc76192938f4f85e57127a0ac08b96f5fd63d31bbcd57df018dbc424b0cea49eb;
    Executor private executor;

    event EXECUTED(
        bytes32 indexed authorizationCommitment,
        address indexed agent,
        address indexed recipient,
        uint256 amount,
        uint256 nonce
    );

    function setUp() public {
        vm.chainId(11155111);
        vm.warp(1000);
        vm.deal(AGENT, 20 ether);
        vm.deal(address(this), 20 ether);
        executor = new Executor(OWNER);
        vm.prank(OWNER);
        executor.configureAgent(AGENT, POLICY, 4, 3);
    }

    function intent() private view returns (Executor.Intent memory i) {
        i = Executor.Intent({
            intentIdHash: 0x7d96da0c235aa8dcdffeac26498f2949e5c9d0fb2c5ffe4a827b278bb632ebc2,
            policyIdHash: POLICY,
            policyCommitment: 0x73570d63948a49a7d55815969a64ad76fb213383ea1a0f8d1755908e38ed3da0,
            agent: AGENT,
            recipient: RECIPIENT,
            asset: address(0),
            decimals: 18,
            amount: 1 ether,
            chainId: block.chainid,
            executor: address(executor),
            nonce: 7,
            policyVersion: 4,
            revocationEpoch: 3,
            issuedAt: 1000,
            notBefore: 1000,
            expiry: 1300,
            maxFee: 1000
        });
    }

    function approve(Executor.Intent memory i) private {
        vm.prank(OWNER);
        executor.approveIntent(COMMITMENT, i);
    }

    function run() private {
        vm.prank(AGENT);
        executor.execute{value: 1 ether}(COMMITMENT);
    }

    function testSepoliaExactTransferAndEventNotSettlement() public {
        Executor.Intent memory i = intent();
        approve(i);
        require(keccak256(abi.encode(executor.getIntent(COMMITMENT))) == keccak256(abi.encode(i)), "record differs");
        vm.expectEmit(true, true, true, true, address(executor));
        emit EXECUTED(COMMITMENT, AGENT, RECIPIENT, 1 ether, 7);
        run();
        require(RECIPIENT.balance == 1 ether && address(executor).balance == 0, "exact caller transfer");
        require(executor.executed(COMMITMENT), "not consumed");
        require(!executor.ALL_IN_FEE_ENFORCED(), "false fee enforcement claim");
    }

    function testBsc97ExactTransfer() public {
        vm.chainId(97);
        executor = new Executor(OWNER);
        vm.prank(OWNER);
        executor.configureAgent(AGENT, POLICY, 4, 3);
        approve(intent());
        run();
        require(RECIPIENT.balance == 1 ether, "BSC transfer");
    }

    function testUnsupportedDeploymentChain() public {
        vm.chainId(1);
        vm.expectRevert(Executor.WrongChain.selector);
        new Executor(OWNER);
    }

    function testWrongChainApprovalAndExecutionIncludingOtherAllowedChain() public {
        Executor.Intent memory i = intent();
        i.chainId = 97;
        vm.expectRevert(Executor.WrongChain.selector);
        approve(i);
        approve(intent());
        vm.chainId(97);
        vm.expectRevert(Executor.WrongChain.selector);
        run();
        vm.chainId(1);
        vm.expectRevert(Executor.WrongChain.selector);
        run();
        require(!executor.executed(COMMITMENT), "wrong chain consumed");
    }

    function testOnlyOwnerApprovalPolicyRevocation() public {
        vm.expectRevert(Executor.NotOwner.selector);
        executor.approveIntent(COMMITMENT, intent());
        vm.expectRevert(Executor.NotOwner.selector);
        executor.configureAgent(AGENT, POLICY, 5, 4);
        vm.expectRevert(Executor.NotOwner.selector);
        executor.revokeAgent(AGENT);
        vm.expectRevert(Executor.NotOwner.selector);
        executor.revokeIntent(COMMITMENT);
    }

    function testApprovalImmutableAndNonceReservedAcrossCommitments() public {
        approve(intent());
        Executor.Intent memory changed = intent();
        changed.recipient = address(0xBAD);
        changed.amount = 2 ether;
        vm.expectRevert(Executor.AlreadyApproved.selector);
        approve(changed);
        vm.prank(OWNER);
        vm.expectRevert(Executor.NonceReserved.selector);
        executor.approveIntent(bytes32(0xb4425785315f8a035bbaf89427288abcd591c40509258beb11d5da53eb16dba0), changed);
        run();
        require(RECIPIENT.balance == 1 ether && address(0xBAD).balance == 0, "override transfer");
    }

    function testWrongAgentUnknownCommitmentAndWrongValue() public {
        approve(intent());
        vm.expectRevert(Executor.NotAgent.selector);
        executor.execute{value: 1 ether}(COMMITMENT);
        vm.prank(AGENT);
        vm.expectRevert(Executor.UnknownIntent.selector);
        executor.execute{value: 1 ether}(bytes32(0xccdd35168ab474fa5764a526cfb83621351e23682c5075b2e18d56bddf96aa30));
        vm.prank(AGENT);
        vm.expectRevert(Executor.WrongAmount.selector);
        executor.execute{value: 1 ether - 1}(COMMITMENT);
        vm.prank(AGENT);
        vm.expectRevert(Executor.WrongAmount.selector);
        executor.execute{value: 1 ether + 1}(COMMITMENT);
        require(!executor.executed(COMMITMENT), "invalid call consumed");
        run();
    }

    function testNotBeforeAndExactExpiry() public {
        Executor.Intent memory i = intent();
        i.notBefore = 1100;
        approve(i);
        vm.expectRevert(Executor.InvalidTime.selector);
        run();
        vm.warp(1300);
        vm.expectRevert(Executor.InvalidTime.selector);
        run();
        vm.warp(1100);
        run();
    }

    function testLastSecondBeforeExpirySucceeds() public {
        approve(intent());
        vm.warp(1299);
        run();
    }

    function testMalformedTimeApproval() public {
        Executor.Intent memory i = intent();
        i.issuedAt = 1001;
        vm.expectRevert(Executor.InvalidTime.selector);
        approve(i);
        i = intent();
        i.notBefore = 999;
        vm.expectRevert(Executor.InvalidTime.selector);
        approve(i);
        i = intent();
        i.notBefore = i.expiry;
        vm.expectRevert(Executor.InvalidTime.selector);
        approve(i);
        i = intent();
        vm.warp(i.expiry);
        vm.expectRevert(Executor.InvalidTime.selector);
        approve(i);
    }

    function testNativeOnlyRecipientAndExecutorValidation() public {
        Executor.Intent memory i = intent();
        i.asset = address(1);
        vm.expectRevert(Executor.InvalidIntent.selector);
        approve(i);
        i = intent();
        i.decimals = 6;
        vm.expectRevert(Executor.InvalidIntent.selector);
        approve(i);
        i = intent();
        i.recipient = address(0);
        vm.expectRevert(Executor.InvalidIntent.selector);
        approve(i);
        i = intent();
        i.recipient = address(executor);
        vm.expectRevert(Executor.InvalidIntent.selector);
        approve(i);
        i = intent();
        i.executor = address(1);
        vm.expectRevert(Executor.InvalidIntent.selector);
        approve(i);
        i = intent();
        i.amount = 0;
        vm.expectRevert(Executor.InvalidIntent.selector);
        approve(i);
    }

    function testInvalidPolicyAndMetadataApproval() public {
        Executor.Intent memory i = intent();
        i.policyIdHash = bytes32(0x8810ad581e59f2bc3928b261707a71308f7e139eb04820366dc4d5c18d980225);
        vm.expectRevert(Executor.InvalidPolicy.selector);
        approve(i);
        i = intent();
        i.policyVersion++;
        vm.expectRevert(Executor.InvalidPolicy.selector);
        approve(i);
        i = intent();
        i.revocationEpoch++;
        vm.expectRevert(Executor.InvalidPolicy.selector);
        approve(i);
        i = intent();
        i.policyCommitment = 0;
        vm.expectRevert(Executor.InvalidIntent.selector);
        approve(i);
        i = intent();
        i.intentIdHash = 0;
        vm.expectRevert(Executor.InvalidIntent.selector);
        approve(i);
    }

    function testPolicyRevokeAndReenableCannotResurrectApproval() public {
        approve(intent());
        vm.prank(OWNER);
        executor.revokeAgent(AGENT);
        vm.expectRevert(Executor.InvalidPolicy.selector);
        run();
        vm.prank(OWNER);
        vm.expectRevert(Executor.InvalidPolicy.selector);
        executor.configureAgent(AGENT, POLICY, 4, 3);
        vm.prank(OWNER);
        executor.configureAgent(AGENT, POLICY, 4, 5);
        vm.expectRevert(Executor.InvalidPolicy.selector);
        run();
        require(!executor.executed(COMMITMENT), "revoked consumed");
    }

    function testPolicyVersionUpdateRevokesOutstandingApprovals() public {
        approve(intent());
        vm.prank(OWNER);
        executor.configureAgent(AGENT, POLICY, 5, 4);
        vm.expectRevert(Executor.InvalidPolicy.selector);
        run();
    }

    function testIntentRevocationPermanentAndNonceNotRecycled() public {
        approve(intent());
        vm.prank(OWNER);
        executor.revokeIntent(COMMITMENT);
        vm.expectRevert(Executor.Revoked.selector);
        run();
        vm.expectRevert(Executor.AlreadyApproved.selector);
        approve(intent());
        vm.prank(OWNER);
        vm.expectRevert(Executor.NonceReserved.selector);
        executor.approveIntent(bytes32(0x95713e9cbdd1dfcb2d4080c2537f418d43ca0da25f0d7d6631f4f7c97b89dc47), intent());
    }

    function testReplayAndDoubleSuccessRejected() public {
        approve(intent());
        run();
        vm.expectRevert(Executor.Replay.selector);
        run();
        require(RECIPIENT.balance == 1 ether, "duplicate success");
    }

    function testTransferFailureRollsBackAndSameExactIntentCanRetry() public {
        NativeToggleRecipient target = new NativeToggleRecipient();
        Executor.Intent memory i = intent();
        i.recipient = address(target);
        approve(i);
        uint256 beforeBalance = AGENT.balance;
        vm.expectRevert(Executor.TransferFailed.selector);
        run();
        require(!executor.executed(COMMITMENT), "failed consumed");
        require(AGENT.balance == beforeBalance && address(executor).balance == 0, "failed value leaked");
        require(executor.nonceReserved(AGENT, 7), "failed released nonce");
        i.recipient = RECIPIENT;
        vm.expectRevert(Executor.AlreadyApproved.selector);
        approve(i);
        target.accept();
        run();
        require(target.received() == 1 ether && RECIPIENT.balance == 0, "retry retargeted");
        vm.expectRevert(Executor.Replay.selector);
        run();
    }

    function testReentrantAgentRecipientBlockedForSameAndDifferentIntent() public {
        NativeReentrantAgent target = new NativeReentrantAgent(executor);
        vm.prank(OWNER);
        executor.configureAgent(address(target), POLICY, 4, 3);
        Executor.Intent memory i = intent();
        i.agent = address(target);
        i.recipient = address(target);
        approve(i);
        i.nonce++;
        bytes32 second = bytes32(0x098e590fb61a9e6ad3ee37748012db4b419c6e61ee4011dd66f90f0b9992c8bd);
        vm.prank(OWNER);
        executor.approveIntent(second, i);
        target.run{value: 1 ether}(COMMITMENT, second);
        require(target.entered() && executor.executed(COMMITMENT), "outer failed");
        require(!executor.executed(second), "nested consumed");
        require(address(target).balance == 1 ether, "reentrant duplicated value");
    }

    function testFuzzExactAmount(uint96 amount) public {
        if (amount == 0) amount = 1;
        Executor.Intent memory i = intent();
        i.amount = uint256(amount);
        approve(i);
        vm.deal(AGENT, uint256(amount));
        vm.prank(AGENT);
        executor.execute{value: uint256(amount)}(COMMITMENT);
        require(RECIPIENT.balance == uint256(amount), "amount mismatch");
    }
}
