// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {PQPAgentSecurity} from "../contracts/PQPAgentSecurity.sol";

interface Vm {
    function envAddress(string calldata name) external returns (address);
    function startBroadcast(address) external;
    function stopBroadcast() external;
}

/// @title PQP Legacy EVM Adapter deployment
/// @notice Deploys only the legacy EVM adapter; this is not the NEAR PQP deployment.
/// @dev Set OWNER before running: forge script script/DeployPQP.s.sol --rpc-url <RPC> --broadcast
contract DeployPQPEVMAdapter {
    Vm internal constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function run() external returns (PQPAgentSecurity deployed) {
        address owner = vm.envAddress("OWNER");
        require(owner != address(0), "OWNER is zero");

        vm.startBroadcast(owner);
        deployed = new PQPAgentSecurity(owner);
        vm.stopBroadcast();
    }
}
