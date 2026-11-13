// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title POA Agent Finance
/// @notice Human-controlled, least-privilege spending authority for autonomous/AI agents.
/// @dev Defense-in-depth. AI/agent keys never become owner keys.
interface IERC20 { function transfer(address to, uint256 amount) external returns (bool); }

contract POAAgentFinance {
    uint256 public constant MAX_POLICY_DURATION = 30 days;
    struct AgentPolicy { bool active; uint64 expiresAt; uint256 nativeCap; uint256 nativeSpent; uint256 nativeTxLimit; }
    struct TokenPolicy { uint256 cap; uint256 spent; uint256 txLimit; }

    address public owner;
    address public pendingOwner;
    bool public paused;
    uint256 public nonce;

    mapping(address => AgentPolicy) public agents;
    mapping(address => mapping(address => mapping(bytes4 => bool))) public allowedCalls;
    mapping(address => mapping(address => TokenPolicy)) public tokenPolicies;
    mapping(address => mapping(address => bool)) public allowedRecipients;

    error NotOwner(); error NotPendingOwner(); error NotAgent(); error Paused(); error InvalidAgent(); error Expired();
    error InvalidExpiry(); error CapExceeded(); error TxLimitExceeded(); error TargetNotAllowed(); error RecipientNotAllowed();
    error InvalidCallData(); error CallFailed(bytes reason); error TransferFailed(); error InvalidOwner(); error Reentrancy();
    error InvalidPolicy(); error InvalidTarget(); error InvalidRecipient(); error AgentMustBeInactive();

    event OwnershipTransferStarted(address indexed previousOwner, address indexed pendingOwner);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event AgentConfigured(address indexed agent, uint64 expiresAt, uint256 nativeCap, uint256 nativeTxLimit);
    event AgentRevoked(address indexed agent);
    event CallPermissionSet(address indexed agent, address indexed target, bytes4 indexed selector, bool allowed);
    event RecipientPermissionSet(address indexed agent, address indexed recipient, bool allowed);
    event TokenPolicySet(address indexed agent, address indexed token, uint256 cap, uint256 txLimit);
    event NativeDeposited(address indexed from, uint256 amount);
    event NativeWithdrawn(address indexed to, uint256 amount);
    event AgentCall(address indexed agent, address indexed target, uint256 value, bytes4 selector, uint256 nonce);
    event AgentTokenTransfer(address indexed agent, address indexed token, address indexed recipient, uint256 amount, uint256 nonce);
    event PausedSet(bool paused);

    uint256 private _lock = 1;
    modifier onlyOwner(){if(msg.sender!=owner)revert NotOwner();_;} 
    modifier whenNotPaused(){if(paused)revert Paused();_;} 
    modifier onlyActiveAgent(){AgentPolicy memory p=agents[msg.sender];if(!p.active)revert NotAgent();if(p.expiresAt==0||block.timestamp>=p.expiresAt)revert Expired();if(paused)revert Paused();_;} 
    modifier nonReentrant(){if(_lock!=1)revert Reentrancy();_lock=2;_;_lock=1;}

    constructor(address initialOwner){if(initialOwner==address(0))revert InvalidOwner();owner=initialOwner;emit OwnershipTransferred(address(0),initialOwner);}
    receive()external payable{emit NativeDeposited(msg.sender,msg.value);}

    function transferOwnership(address newOwner)external onlyOwner{if(newOwner==address(0))revert InvalidOwner();pendingOwner=newOwner;emit OwnershipTransferStarted(owner,newOwner);}
    function acceptOwnership()external{if(msg.sender!=pendingOwner)revert NotPendingOwner();address oldOwner=owner;owner=pendingOwner;pendingOwner=address(0);emit OwnershipTransferred(oldOwner,owner);}
    function setPaused(bool value)external onlyOwner{paused=value;emit PausedSet(value);}

    function configureAgent(address agent,uint64 expiresAt,uint256 nativeCap,uint256 nativeTxLimit)external onlyOwner{
        if(agent==address(0)||agent==owner)revert InvalidAgent();
        if(agents[agent].active)revert AgentMustBeInactive();
        if(expiresAt==0||expiresAt<=block.timestamp||expiresAt>block.timestamp+MAX_POLICY_DURATION)revert InvalidExpiry();
        if(nativeCap==0||nativeTxLimit==0||nativeTxLimit>nativeCap)revert InvalidPolicy();
        agents[agent]=AgentPolicy(true,expiresAt,nativeCap,0,nativeTxLimit);
        emit AgentConfigured(agent,expiresAt,nativeCap,nativeTxLimit);
    }
    function revokeAgent(address agent)external onlyOwner{if(agent==address(0))revert InvalidAgent();agents[agent].active=false;emit AgentRevoked(agent);}
    function setCallPermission(address agent,address target,bytes4 selector,bool allowed)external onlyOwner{
        if(agent==address(0)||agent==owner)revert InvalidAgent(); if(target==address(0)||target==owner)revert InvalidTarget();
        if(selector==bytes4(0))revert InvalidCallData(); allowedCalls[agent][target][selector]=allowed;emit CallPermissionSet(agent,target,selector,allowed);
    }
    function setRecipientPermission(address agent,address recipient,bool allowed)external onlyOwner{
        if(agent==address(0)||agent==owner)revert InvalidAgent();if(recipient==address(0))revert InvalidRecipient();
        allowedRecipients[agent][recipient]=allowed;emit RecipientPermissionSet(agent,recipient,allowed);
    }
    function setTokenPolicy(address agent,address token,uint256 cap,uint256 txLimit)external onlyOwner{
        if(agent==address(0)||agent==owner)revert InvalidAgent();if(token==address(0))revert InvalidTarget();
        if(agents[agent].active)revert AgentMustBeInactive();
        if(cap==0||txLimit==0||txLimit>cap)revert InvalidPolicy();tokenPolicies[agent][token]=TokenPolicy(cap,0,txLimit);emit TokenPolicySet(agent,token,cap,txLimit);
    }
    function deposit()external payable whenNotPaused{emit NativeDeposited(msg.sender,msg.value);}
    function withdrawNative(address payable to,uint256 amount)external onlyOwner nonReentrant{if(to==address(0)||amount>address(this).balance)revert CapExceeded();(bool ok,bytes memory reason)=to.call{value:amount}("");if(!ok)revert CallFailed(reason);emit NativeWithdrawn(to,amount);}

    function execute(address target,uint256 value,bytes calldata data)external onlyActiveAgent nonReentrant returns(bytes memory result){
        if(target==address(0)||data.length<4)revert InvalidCallData();bytes4 selector;assembly{selector:=calldataload(data.offset)}
        if(!allowedCalls[msg.sender][target][selector])revert TargetNotAllowed();AgentPolicy storage p=agents[msg.sender];
        if(value>p.nativeTxLimit)revert TxLimitExceeded();if(p.nativeSpent+value>p.nativeCap)revert CapExceeded();if(value>address(this).balance)revert CapExceeded();
        p.nativeSpent+=value;uint256 currentNonce=nonce++;(bool ok,bytes memory ret)=target.call{value:value}(data);if(!ok)revert CallFailed(ret);
        emit AgentCall(msg.sender,target,value,selector,currentNonce);return ret;
    }
    function transferToken(address token,address recipient,uint256 amount)external onlyActiveAgent nonReentrant{
        if(token==address(0)||recipient==address(0))revert InvalidAgent();if(!allowedRecipients[msg.sender][recipient])revert RecipientNotAllowed();
        TokenPolicy storage p=tokenPolicies[msg.sender][token];if(amount==0)revert InvalidPolicy();if(amount>p.txLimit)revert TxLimitExceeded();if(p.spent+amount>p.cap)revert CapExceeded();
        p.spent+=amount;uint256 currentNonce=nonce++;(bool success,bytes memory ret)=token.call(abi.encodeWithSelector(IERC20.transfer.selector,recipient,amount));
        if(!success||(ret.length!=0&&!abi.decode(ret,(bool))))revert TransferFailed();emit AgentTokenTransfer(msg.sender,token,recipient,amount,currentNonce);
    }
    function agentRemainingNative(address agent)external view returns(uint256){AgentPolicy memory p=agents[agent];if(!p.active||p.expiresAt==0||block.timestamp>=p.expiresAt||p.nativeSpent>=p.nativeCap)return 0;return p.nativeCap-p.nativeSpent;}
    function tokenRemaining(address agent,address token)external view returns(uint256){TokenPolicy memory p=tokenPolicies[agent][token];if(p.spent>=p.cap)return 0;return p.cap-p.spent;}
}
