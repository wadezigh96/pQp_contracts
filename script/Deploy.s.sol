// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {QPCAgentFinance} from "../contracts/QPCAgentFinance.sol";

interface Vm {
    function envAddress(string calldata name) external returns (address);
    function startBroadcast() external;
    function stopBroadcast() external;
}

/// @title QPC Agent Finance deployment
/// @notice Minimal Foundry deployment script without external library dependencies.
/// @dev Set OWNER and configure a separate Foundry deployer signer before broadcasting.
///      Example: OWNER=<owner-address> forge script script/Deploy.s.sol --rpc-url <RPC> --broadcast --private-key <deployer-key>
contract DeployQPCAgentFinance {
    Vm internal constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function run() external returns (QPCAgentFinance deployed) {
        address owner = vm.envAddress("OWNER");
        require(owner != address(0), "OWNER is zero");

        vm.startBroadcast();
        deployed = new QPCAgentFinance(owner);
        vm.stopBroadcast();
    }
}
