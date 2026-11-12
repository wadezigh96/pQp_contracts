// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title QPC Ethereum Settlement Vault
/// @notice Prefunded destination payout vault requiring a validator quorum.
/// @dev EXPERIMENTAL: validators attest to source-chain facts off-chain. This contract
///      does not verify NEAR light-client proofs and is NOT a trustless bridge.
interface IQPCToken {
    function transfer(address to, uint256 amount) external returns (bool);
}

contract EthereumQPCSettlement {
    error NotOwner();
    error NotPendingOwner();
    error Paused();
    error WrongChain();
    error InvalidAddress();
    error InvalidAmount();
    error InvalidDeadline();
    error TokenNotAllowed();
    error Replay();
    error TransferFailed();
    error InvalidValidatorSet();
    error NotValidator();
    error InvalidSignatures();

    bytes32 private constant ATTESTATION_TYPEHASH = keccak256(
        "QPCSettlement(bytes32 sourceDomain,bytes32 sourcePaymentId,bytes32 sourceTransactionRef,address token,address recipient,uint256 amount,uint256 deadline,uint256 destinationChainId,address vault)"
    );
    uint256 private constant SECP256K1_HALF_ORDER =
        0x7fffffffffffffffffffffffffffffff5d576e7357a4501ddfe92f46681b20a0;

    address public owner;
    address public pendingOwner;
    uint256 public immutable destinationChainId;
    uint256 public validatorThreshold;
    uint256 public validatorCount;
    bool public paused;

    mapping(address => bool) public isValidator;
    mapping(address => bool) public allowedTokens;
    mapping(bytes32 => bool) public consumedSourcePayments;
    mapping(bytes32 => bool) public consumedSourceTransactions;

    uint256 private _lock = 1;

    event OwnershipTransferStarted(address indexed owner, address indexed pendingOwner);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event ValidatorChanged(address indexed validator, bool enabled);
    event ValidatorThresholdChanged(uint256 previousThreshold, uint256 newThreshold);
    event TokenAllowlistChanged(address indexed token, bool allowed);
    event PausedChanged(bool paused);
    event SettlementPaid(
        bytes32 indexed sourceDomain,
        bytes32 indexed sourcePaymentId,
        bytes32 sourceTransactionRef,
        address indexed token,
        address recipient,
        uint256 amount,
        uint256 destinationChainId,
        uint256 attestations
    );

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    modifier nonReentrant() {
        require(_lock == 1, "QPC: reentrant");
        _lock = 2;
        _;
        _lock = 1;
    }

    constructor(
        address initialOwner,
        address[] memory initialValidators,
        uint256 initialThreshold,
        uint256 expectedChainId
    ) {
        if (initialOwner == address(0)) revert InvalidAddress();
        if (expectedChainId == 0 || block.chainid != expectedChainId) revert WrongChain();
        owner = initialOwner;
        destinationChainId = expectedChainId;
        emit OwnershipTransferred(address(0), initialOwner);
        if (
            initialValidators.length == 0 ||
            initialThreshold == 0 ||
            initialThreshold > initialValidators.length
        ) revert InvalidValidatorSet();
        for (uint256 i; i < initialValidators.length; ++i) {
            address validator = initialValidators[i];
            if (validator == address(0) || isValidator[validator]) revert InvalidValidatorSet();
            isValidator[validator] = true;
            ++validatorCount;
            emit ValidatorChanged(validator, true);
        }
        validatorThreshold = initialThreshold;
        emit ValidatorThresholdChanged(0, initialThreshold);
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

    function setValidator(address validator, bool enabled) external onlyOwner {
        if (validator == address(0)) revert InvalidAddress();
        if (isValidator[validator] == enabled) revert InvalidValidatorSet();
        if (enabled) {
            isValidator[validator] = true;
            ++validatorCount;
        } else {
            if (validatorCount - 1 < validatorThreshold) revert InvalidValidatorSet();
            isValidator[validator] = false;
            --validatorCount;
        }
        emit ValidatorChanged(validator, enabled);
    }

    function setValidatorThreshold(uint256 nextThreshold) external onlyOwner {
        if (nextThreshold == 0 || nextThreshold > validatorCount) revert InvalidValidatorSet();
        uint256 previous = validatorThreshold;
        validatorThreshold = nextThreshold;
        emit ValidatorThresholdChanged(previous, nextThreshold);
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

    /// @notice Release prefunded liquidity after a threshold of validators attest to source payment.
    /// @dev Signatures are EIP-191 personal_sign signatures over the domain-bound attestation digest.
    ///      Signers must be strictly ascending by address; duplicate or non-validator signatures fail.
    function settle(
        bytes32 sourceDomain,
        bytes32 sourcePaymentId,
        bytes32 sourceTransactionRef,
        address token,
        address recipient,
        uint256 amount,
        uint256 deadline,
        bytes[] calldata signatures
    ) external nonReentrant {
        if (paused) revert Paused();
        if (
            sourceDomain == bytes32(0) || sourcePaymentId == bytes32(0) ||
            sourceTransactionRef == bytes32(0) || recipient == address(0)
        ) revert InvalidAddress();
        if (token.code.length == 0 || !allowedTokens[token]) revert TokenNotAllowed();
        if (amount == 0) revert InvalidAmount();
        if (deadline < block.timestamp) revert InvalidDeadline();

        bytes32 paymentKey = keccak256(abi.encode(sourceDomain, sourcePaymentId));
        if (consumedSourcePayments[paymentKey] || consumedSourceTransactions[sourceTransactionRef]) {
            revert Replay();
        }

        bytes32 structHash = keccak256(abi.encode(
            ATTESTATION_TYPEHASH, sourceDomain, sourcePaymentId, sourceTransactionRef,
            token, recipient, amount, deadline, destinationChainId, address(this)
        ));
        bytes32 digest = keccak256(abi.encodePacked(
            "\x19Ethereum Signed Message:\n32", structHash
        ));
        _verifyQuorum(digest, signatures);

        // Replay keys are consumed before token interaction; a failed transfer reverts all state.
        consumedSourcePayments[paymentKey] = true;
        consumedSourceTransactions[sourceTransactionRef] = true;

        (bool ok, bytes memory result) = token.call(
            abi.encodeWithSelector(IQPCToken.transfer.selector, recipient, amount)
        );
        if (!ok || (result.length != 0 && (result.length < 32 || !abi.decode(result, (bool))))) {
            revert TransferFailed();
        }

        emit SettlementPaid(
            sourceDomain, sourcePaymentId, sourceTransactionRef,
            token, recipient, amount, destinationChainId, signatures.length
        );
    }

    function attestationDigest(
        bytes32 sourceDomain,
        bytes32 sourcePaymentId,
        bytes32 sourceTransactionRef,
        address token,
        address recipient,
        uint256 amount,
        uint256 deadline
    ) external view returns (bytes32) {
        bytes32 structHash = keccak256(abi.encode(
            ATTESTATION_TYPEHASH, sourceDomain, sourcePaymentId, sourceTransactionRef,
            token, recipient, amount, deadline, destinationChainId, address(this)
        ));
        return keccak256(abi.encodePacked("\x19Ethereum Signed Message:\n32", structHash));
    }

    function _verifyQuorum(bytes32 digest, bytes[] calldata signatures) private view {
        if (signatures.length < validatorThreshold || signatures.length > validatorCount) {
            revert InvalidSignatures();
        }
        address previous;
        for (uint256 i; i < signatures.length; ++i) {
            address signer = _recover(digest, signatures[i]);
            if (signer <= previous || !isValidator[signer]) revert InvalidSignatures();
            previous = signer;
        }
    }

    function _recover(bytes32 digest, bytes calldata signature) private pure returns (address signer) {
        if (signature.length != 65) revert InvalidSignatures();
        bytes32 r;
        bytes32 s;
        uint8 v;
        assembly {
            r := calldataload(signature.offset)
            s := calldataload(add(signature.offset, 32))
            v := byte(0, calldataload(add(signature.offset, 64)))
        }
        if (uint256(s) > SECP256K1_HALF_ORDER || (v != 27 && v != 28)) {
            revert InvalidSignatures();
        }
        signer = ecrecover(digest, v, r, s);
        if (signer == address(0)) revert InvalidSignatures();
    }

    function availableLiquidity(address token) external view returns (uint256 balance) {
        (bool ok, bytes memory result) = token.staticcall(
            abi.encodeWithSignature("balanceOf(address)", address(this))
        );
        if (!ok || result.length < 32) return 0;
        balance = abi.decode(result, (uint256));
    }
}
