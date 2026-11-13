// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {POAAgentFinance} from "../contracts/POAAgentFinance.sol";

interface Vm {
    function envAddress(string calldata name) external returns (address);
    function startBroadcast(address) external;
    function stopBroadcast() external;
}

/// @title POA Agent Finance deployment
/// @notice Minimal Foundry deployment script without external library dependencies.
/// @dev Set OWNER before running: forge script script/Deploy.s.sol --rpc-url <RPC> --broadcast
contract DeployPOAAgentFinance {
    Vm internal constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function run() external returns (POAAgentFinance deployed) {
        address owner = vm.envAddress("OWNER");
        require(owner != address(0), "OWNER is zero");

        vm.startBroadcast(owner);
        deployed = new POAAgentFinance(owner);
        vm.stopBroadcast();
    }
}
