// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;
import {EthereumQPC} from "../../contracts/ethereum/EthereumQPC.sol";
interface Vm { function envAddress(string calldata name) external returns (address); function startBroadcast(address) external; function stopBroadcast() external; }
contract DeployEthereumQPC {
    Vm internal constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function run() external returns (EthereumQPC deployed) {
        address owner = vm.envAddress("OWNER");
        require(owner != address(0), "OWNER is zero");
        require(block.chainid == 1, "Wrong chain: Ethereum mainnet only");
        vm.startBroadcast(owner); deployed = new EthereumQPC(owner); vm.stopBroadcast();
    }
}