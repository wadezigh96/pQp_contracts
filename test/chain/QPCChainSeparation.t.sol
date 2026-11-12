// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;
import {EthereumQPC} from "../../contracts/ethereum/EthereumQPC.sol";
import {BSCQPC} from "../../contracts/bsc/BSCQPC.sol";
interface Vm { function chainId(uint256) external; }
contract QPCChainSeparationTest {
    Vm internal constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    function testEthereumAdapterRejectsBSC() public {
        vm.chainId(56);
        (bool ok,) = address(new EthereumQPC(address(this))).call("");
        require(!ok, "Ethereum adapter accepted BSC");
    }
    function testBSCAdapterRejectsEthereum() public {
        vm.chainId(1);
        (bool ok,) = address(new BSCQPC(address(this))).call("");
        require(!ok, "BSC adapter accepted Ethereum");
    }
    function testEthereumAdapterAcceptsEthereum() public {
        vm.chainId(1);
        EthereumQPC qpc = new EthereumQPC(address(this));
        require(qpc.owner() == address(this), "wrong Ethereum owner");
    }
    function testBSCAdapterAcceptsBSC() public {
        vm.chainId(56);
        BSCQPC qpc = new BSCQPC(address(this));
        require(qpc.owner() == address(this), "wrong BSC owner");
    }
}