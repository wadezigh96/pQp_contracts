// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {QPCAgentFinance} from "../contracts/QPCAgentFinance.sol";

interface Vm {
    function envAddress(string calldata name) external returns (address);
    function startBroadcast(address) external;
    function stopBroadcast() external;
}

/// @title QPC Agent Finance deployment
/// @notice Minimal Foundry deployment script without external library dependencies.
/// @dev Set OWNER before running: forge script script/Deploy.s.sol --rpc-url <RPC> --broadcast
contract DeployQPCAgentFinance {
    Vm internal constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function run() external returns (QPCAgentFinance deployed) {
        address owner = vm.envAddress("OWNER");
        require(owner != address(0), "OWNER is zero");

        vm.startBroadcast(owner);
        deployed = new QPCAgentFinance(owner);
        vm.stopBroadcast();
    }
}
