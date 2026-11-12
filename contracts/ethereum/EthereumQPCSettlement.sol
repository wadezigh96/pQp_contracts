// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title QPC Ethereum Settlement Vault
/// @notice Prefunded destination-chain payout vault controlled by a trusted settlement relayer.
/// @dev This is NOT a trustless bridge. The relayer is responsible for independently verifying
///      the source-chain escrow/payment and destination route before calling settle().
interface IQPCToken {
    function transfer(address to, uint256 amount) external returns (bool);
}

contract EthereumQPCSettlement {
    error NotOwner();
    error NotPendingOwner();
    error NotRelayer();
    error Paused();
    error WrongChain();
    error InvalidAddress();
    error InvalidAmount();
    error InvalidDeadline();
    error TokenNotAllowed();
    error Replay();
    error TransferFailed();

    address public owner;
    address public pendingOwner;
    address public relayer;
    uint256 public immutable destinationChainId;
    bool public paused;

    mapping(address => bool) public allowedTokens;
    mapping(bytes32 => bool) public consumedSourcePayments;
    mapping(bytes32 => bool) public consumedSourceTransactions;

    uint256 private _lock = 1;

    event OwnershipTransferStarted(address indexed owner, address indexed pendingOwner);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event RelayerChanged(address indexed previousRelayer, address indexed newRelayer);
    event TokenAllowlistChanged(address indexed token, bool allowed);
    event PausedChanged(bool paused);
    event SettlementPaid(
        bytes32 indexed sourceDomain,
        bytes32 indexed sourcePaymentId,
        bytes32 sourceTransactionRef,
        address indexed token,
        address recipient,
        uint256 amount,
        uint256 destinationChainId
    );

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    modifier onlyRelayer() {
        if (msg.sender != relayer) revert NotRelayer();
        _;
    }

    modifier nonReentrant() {
        require(_lock == 1, "QPC: reentrant");
        _lock = 2;
        _;
        _lock = 1;
    }

    constructor(address initialOwner, address initialRelayer, uint256 expectedChainId) {
        if (initialOwner == address(0) || initialRelayer == address(0)) revert InvalidAddress();
        if (expectedChainId == 0 || block.chainid != expectedChainId) revert WrongChain();
        owner = initialOwner;
        relayer = initialRelayer;
        destinationChainId = expectedChainId;
        emit OwnershipTransferred(address(0), initialOwner);
        emit RelayerChanged(address(0), initialRelayer);
    }

    function transferOwnership(address nextOwner) external onlyOwner {
        if (nextOwner == address(0)) revert InvalidAddress();
        pendingOwner = nextOwner;
        emit OwnershipTransferStarted(owner, nextOwner);
    }

    function acceptOwnership() external {
        if (msg.sender != pendingOwner) revert NotPendingOwner();
        address previous = owner;
        owner = pendingOwner;
        pendingOwner = address(0);
        emit OwnershipTransferred(previous, owner);
    }

    function setRelayer(address nextRelayer) external onlyOwner {
        if (nextRelayer == address(0)) revert InvalidAddress();
        address previous = relayer;
        relayer = nextRelayer;
        emit RelayerChanged(previous, nextRelayer);
    }

    function setTokenAllowed(address token, bool allowed) external onlyOwner {
        if (token == address(0) || token.code.length == 0) revert InvalidAddress();
        allowedTokens[token] = allowed;
        emit TokenAllowlistChanged(token, allowed);
    }

    function setPaused(bool value) external onlyOwner {
        paused = value;
        emit PausedChanged(value);
    }

    /// @notice Release prefunded destination liquidity after off-chain source verification.
    /// @param sourceDomain Stable domain identifier, e.g. keccak256("near:mainnet").
    /// @param sourcePaymentId Unique payment ID from the source escrow.
    /// @param sourceTransactionRef Hash of the canonical source transaction identifier.
    ///        For NEAR, hash the UTF-8 transaction hash string with keccak256 off-chain.
    function settle(
        bytes32 sourceDomain,
        bytes32 sourcePaymentId,
        bytes32 sourceTransactionRef,
        address token,
        address recipient,
        uint256 amount,
        uint256 deadline
    ) external onlyRelayer nonReentrant {
        if (paused) revert Paused();
        if (
            sourceDomain == bytes32(0) || sourcePaymentId == bytes32(0) ||
            sourceTransactionRef == bytes32(0) || recipient == address(0)
        ) revert InvalidAddress();
        if (token.code.length == 0 || !allowedTokens[token]) revert TokenNotAllowed();
        if (amount == 0) revert InvalidAmount();
        if (deadline < block.timestamp) revert InvalidDeadline();

        bytes32 sourcePaymentKey = keccak256(abi.encode(sourceDomain, sourcePaymentId));
        if (consumedSourcePayments[sourcePaymentKey] || consumedSourceTransactions[sourceTransactionRef]) {
            revert Replay();
        }

        // Consume replay keys before the external token call; revert restores them on failure.
        consumedSourcePayments[sourcePaymentKey] = true;
        consumedSourceTransactions[sourceTransactionRef] = true;

        (bool ok, bytes memory result) = token.call(
            abi.encodeWithSelector(IQPCToken.transfer.selector, recipient, amount)
        );
        if (!ok || (result.length != 0 && (result.length < 32 || !abi.decode(result, (bool))))) {
            revert TransferFailed();
        }

        emit SettlementPaid(
            sourceDomain, sourcePaymentId, sourceTransactionRef,
            token, recipient, amount, destinationChainId
        );
    }

    function availableLiquidity(address token) external view returns (uint256 balance) {
        (bool ok, bytes memory result) = token.staticcall(
            abi.encodeWithSignature("balanceOf(address)", address(this))
        );
        if (!ok || result.length < 32) return 0;
        balance = abi.decode(result, (uint256));
    }
}
