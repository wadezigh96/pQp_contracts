// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;
import {QPCPolicyCore} from "../shared/QPCPolicyCore.sol";
/// @notice BNB Smart Chain mainnet entry point. Deployment reverts on any other chain.
contract BSCQPC is QPCPolicyCore {
    constructor(address initialOwner) QPCPolicyCore(initialOwner) {
        require(block.chainid == 56, "QPC: BSC chain only");
    }
}
