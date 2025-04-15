pragma solidity >=0.8.0;

import "ds-test/test.sol";
// import "./Cheats.sol";

contract AssertionFailure {
    int256 private x;
    int256 private y;
    constructor() public {
        x = 0;
        y = 0;
    }
    function Bar() public view {
        if(x == 42) {
            // ASSERTION FAILURE
            assert(false);
        }
    }
    function SetY(int256 ny) public { y = ny; }
    function CopyY() public { x = y; }

    function assertFalse() public {
        assert(false);
    }
}

/*

contract AFTest is DSTest {

    Cheats constant cheats = Cheats(HEVM_ADDRESS);
    // address assertionFailure = address(new AssertionFailure());
    AssertionFailure assertionFailure = new AssertionFailure();

    function testExploit() public { 
        // assertionFailure.call{value: 0}("0x8EB85729000000000000000000000000000000000000000000000000000000000000002A");
        // assertionFailure.call{value:0}("0x31a6ff9a");
        // assertionFailure.call{value: 0}("0xb0a378b0");
        // assertionFailure.SetY(42);
        // assertionFailure.CopyY();
        // assertionFailure.Bar();
        assertionFailure.assertFalse();
    }

}

*/