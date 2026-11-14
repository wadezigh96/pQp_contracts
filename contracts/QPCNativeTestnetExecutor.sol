// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title Owner-attested SAME-CHAIN native testnet execution prototype
/// @notice Sepolia/BSC testnet ONLY. Emits EXECUTED, never a settlement claim.
/// @dev The owner explicitly attests that a canonical authorization commitment
/// maps to these exact parameters. This contract does NOT parse canonical JSON,
/// verify cross-chain evidence, or turn an arbitrary hash into authentication.
/// maxFee is attested metadata only: EVM contracts cannot enforce the sender's
/// all-in transaction fee (including intrinsic gas and post-call execution).
/// No generic settlement capability is created. No token, bridge, or arbitrary
/// calldata execution exists. Successful execution transfers caller-funded value.
contract QPCNativeTestnetExecutor {
    // Off-chain ID convention: SHA256("QPC-EVM-INTENT-ID-V1\\0" + UTF8 intentId)
    // and SHA256("QPC-EVM-POLICY-ID-V1\\0" + UTF8 policy.id). The contract
    // stores these owner-attested digests; it does not hash or decode IDs.
    struct Intent {
        bytes32 intentIdHash;
        bytes32 policyIdHash;
        bytes32 policyCommitment;
        address agent;
        address recipient;
        address asset;
        uint8 decimals;
        uint256 amount;
        uint256 chainId;
        address executor;
        uint256 nonce;
        uint256 policyVersion;
        uint256 revocationEpoch;
        uint64 issuedAt;
        uint64 notBefore;
        uint64 expiry;
        uint256 maxFee;
    }

    struct AgentPolicy {
        bool active;
        bytes32 policyIdHash;
        uint256 policyVersion;
        uint256 revocationEpoch;
    }

    address public immutable owner;
    bool public constant ALL_IN_FEE_ENFORCED = false;
    mapping(address => AgentPolicy) public agentPolicies;
    mapping(bytes32 => Intent) private intents;
    mapping(bytes32 => bool) public approved;
    mapping(bytes32 => bool) public revoked;
    mapping(bytes32 => bool) public executed;
    // Nonces remain reserved even after revocation, expiry or failed execution.
    mapping(address => mapping(uint256 => bool)) public nonceReserved;
    // Intent IDs share the permanent per-agent replay domain of nonces.
    // Another agent cannot squat this agent's ID; policy changes cannot recycle it.
    mapping(address => mapping(bytes32 => bool)) public intentIdReserved;
    uint256 private lock = 1;

    error NotOwner();
    error InvalidOwner();
    error WrongChain();
    error InvalidIntent();
    error InvalidPolicy();
    error UnknownIntent();
    error AlreadyApproved();
    error NonceReserved();
    error IntentIdReserved();
    error Revoked();
    error Replay();
    error NotAgent();
    error InvalidTime();
    error WrongAmount();
    error TransferFailed();
    error Reentrancy();

    event AgentConfigured(address indexed agent, bytes32 policyIdHash, uint256 policyVersion, uint256 revocationEpoch);
    event AgentRevoked(address indexed agent, uint256 revocationEpoch);
    event IntentApproved(bytes32 indexed authorizationCommitment, bytes32 intentIdHash, address indexed agent);
    event IntentRevoked(bytes32 indexed authorizationCommitment);
    event EXECUTED(
        bytes32 indexed authorizationCommitment,
        address indexed agent,
        address indexed recipient,
        uint256 amount,
        uint256 nonce
    );

    constructor(address initialOwner) {
        if (initialOwner == address(0)) revert InvalidOwner();
        _testChain();
        owner = initialOwner;
    }

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    modifier nonReentrant() {
        if (lock != 1) revert Reentrancy();
        lock = 2;
        _;
        lock = 1;
    }

    function configureAgent(address agent, bytes32 policyIdHash, uint256 policyVersion, uint256 revocationEpoch)
        external
        onlyOwner
        nonReentrant
    {
        _testChain();
        if (
            agent == address(0) || policyIdHash == bytes32(0) || policyVersion == 0
                || revocationEpoch <= agentPolicies[agent].revocationEpoch
        ) revert InvalidPolicy();
        agentPolicies[agent] = AgentPolicy(true, policyIdHash, policyVersion, revocationEpoch);
        emit AgentConfigured(agent, policyIdHash, policyVersion, revocationEpoch);
    }

    function revokeAgent(address agent) external onlyOwner nonReentrant {
        AgentPolicy storage p = agentPolicies[agent];
        p.active = false;
        p.revocationEpoch += 1;
        emit AgentRevoked(agent, p.revocationEpoch);
    }

    /// @notice Trusted owner attestation of commitment -> exact parameters.
    /// @dev Fields cannot ever be replaced, including after failure/revocation.
    function approveIntent(bytes32 authorizationCommitment, Intent calldata intent) external onlyOwner nonReentrant {
        _testChain();
        if (approved[authorizationCommitment]) revert AlreadyApproved();
        if (
            authorizationCommitment == bytes32(0) || intent.intentIdHash == bytes32(0)
                || intent.policyCommitment == bytes32(0) || intent.agent == address(0) || intent.recipient == address(0)
                || intent.recipient == address(this) || intent.asset != address(0) || intent.decimals != 18
                || intent.amount == 0 || intent.executor != address(this)
        ) revert InvalidIntent();
        if (intent.chainId != block.chainid) revert WrongChain();
        if (
            intent.issuedAt > block.timestamp || intent.issuedAt > intent.notBefore || intent.notBefore >= intent.expiry
                || block.timestamp >= intent.expiry
        ) revert InvalidTime();
        _currentPolicy(intent);
        if (nonceReserved[intent.agent][intent.nonce]) revert NonceReserved();
        if (intentIdReserved[intent.agent][intent.intentIdHash]) revert IntentIdReserved();
        nonceReserved[intent.agent][intent.nonce] = true;
        intentIdReserved[intent.agent][intent.intentIdHash] = true;
        approved[authorizationCommitment] = true;
        intents[authorizationCommitment] = intent;
        emit IntentApproved(authorizationCommitment, intent.intentIdHash, intent.agent);
    }

    function revokeIntent(bytes32 authorizationCommitment) external onlyOwner nonReentrant {
        if (!approved[authorizationCommitment]) revert UnknownIntent();
        revoked[authorizationCommitment] = true;
        emit IntentRevoked(authorizationCommitment);
    }

    function getIntent(bytes32 authorizationCommitment) external view returns (Intent memory) {
        if (!approved[authorizationCommitment]) revert UnknownIntent();
        return intents[authorizationCommitment];
    }

    /// @notice Executes only stored approved parameters; no recipient override.
    /// @dev Consumption precedes the external call. Failure reverts atomically,
    /// so only this same immutable intent may retry, never duplicate success.
    function execute(bytes32 authorizationCommitment) external payable nonReentrant {
        _testChain();
        if (!approved[authorizationCommitment]) revert UnknownIntent();
        if (revoked[authorizationCommitment]) revert Revoked();
        if (executed[authorizationCommitment]) revert Replay();
        Intent memory intent = intents[authorizationCommitment];
        if (intent.chainId != block.chainid || intent.executor != address(this)) revert WrongChain();
        if (msg.sender != intent.agent) revert NotAgent();
        if (block.timestamp < intent.notBefore || block.timestamp >= intent.expiry) revert InvalidTime();
        _currentPolicy(intent);
        if (msg.value != intent.amount) revert WrongAmount();
        executed[authorizationCommitment] = true;
        (bool ok,) = payable(intent.recipient).call{value: intent.amount}("");
        if (!ok) revert TransferFailed();
        emit EXECUTED(authorizationCommitment, intent.agent, intent.recipient, intent.amount, intent.nonce);
    }

    function _currentPolicy(Intent memory intent) private view {
        AgentPolicy memory p = agentPolicies[intent.agent];
        if (
            !p.active || p.policyIdHash != intent.policyIdHash || p.policyVersion != intent.policyVersion
                || p.revocationEpoch != intent.revocationEpoch
        ) revert InvalidPolicy();
    }

    function _testChain() private view {
        if (block.chainid != 11155111 && block.chainid != 97) revert WrongChain();
    }
}
