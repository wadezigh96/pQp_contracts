// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;
import {QPCPolicyCore} from "../shared/QPCPolicyCore.sol";
/// @notice Ethereum-mainnet entry point. Deployment reverts on any other chain.
contract EthereumQPC is QPCPolicyCore {
    constructor(address initialOwner) QPCPolicyCore(initialOwner) {
        require(block.chainid == 1, "QPC: Ethereum chain only");
    }
}
