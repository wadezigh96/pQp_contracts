// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;
import {BSCQPC} from "../../contracts/bsc/BSCQPC.sol";
interface Vm { function envAddress(string calldata name) external returns (address); function startBroadcast(address) external; function stopBroadcast() external; }
contract DeployBSCQPC {
    Vm internal constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function run() external returns (BSCQPC deployed) {
        address owner = vm.envAddress("OWNER");
        require(owner != address(0), "OWNER is zero");
        require(block.chainid == 56, "Wrong chain: BSC mainnet only");
        vm.startBroadcast(owner); deployed = new BSCQPC(owner); vm.stopBroadcast();
    }
}