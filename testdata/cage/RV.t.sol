pragma solidity >=0.8.0;

import "ds-test/test.sol";
// import "./Cheats.sol";

contract RequirementViolation {

    function shouldRevert() public {
        require(false);
    }

}

/*

contract RVTest is DSTest {

    Cheats constant cheats = Cheats(HEVM_ADDRESS);
    RequirementViolation requirementViolation = new RequirementViolation();

    function testExploit() public { 
        requirementViolation.shouldRevert();
    }

}

*/