// SPDX-License-Identifier: MIT
pragma solidity >=0.8.0;

import "ds-test/test.sol";
import "../cheats/Cheats.sol";
import "../OracleSupport.sol";

contract GainWETH {

    IERC20 WETH = IERC20(0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2);
    IERC20 WBTC = IERC20(0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599);
    Cheats cheats = Cheats(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);
    address curveVyper_contract_addr = 0xD51a44d3FaE010294C616388b506AcdA1bfAAE46;
    Oracle oracle = Oracle(0x502be16aa82BAD01FDc3fEB3c5F8C431F8eeB8AE);

    function testGainWETH() public {

        cheats.prank(curveVyper_contract_addr);
        WETH.approve(address(this), 1);

        WETH.transferFrom(curveVyper_contract_addr, address(this), 1);

    }

}