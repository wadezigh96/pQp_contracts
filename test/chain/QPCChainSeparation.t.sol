// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {EthereumQPC} from "../../contracts/ethereum/EthereumQPC.sol";
import {BSCQPC} from "../../contracts/bsc/BSCQPC.sol";

interface Vm { function chainId(uint256) external; }

contract QPCChainSeparationTest {
    Vm internal constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    function testEthereumAdapterRejectsBSC() public {
        vm.chainId(56);
        try new EthereumQPC(address(this)) returns (EthereumQPC) {
            revert("Ethereum adapter accepted BSC");
        } catch {}
    }

    function testBSCAdapterRejectsEthereum() public {
        vm.chainId(1);
        try new BSCQPC(address(this)) returns (BSCQPC) {
            revert("BSC adapter accepted Ethereum");
        } catch {}
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
