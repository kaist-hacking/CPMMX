// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.12 <0.9.0;

// For arbitrary external call vulnerability
contract AEC {
  function externalCall(address to, bytes calldata data) public {
    to.call(data);
  }
}
