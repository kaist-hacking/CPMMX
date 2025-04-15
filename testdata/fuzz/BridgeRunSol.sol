// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.0;

import "../OracleSupport.sol";
import "../lib/openzeppelin-contracts/contracts/utils/Strings.sol";

contract Bridge {

    Oracle oracle = Oracle(0x502be16aa82BAD01FDc3fEB3c5F8C431F8eeB8AE);
    IERC20 targetToken;
    IERC20 baseToken;
    IUniswapV2Pair pair;
    IUniswapV2Router02 router;
    address[] tokenAddrs;
    uint targetTokenReserveNo;
    uint baseTokenReserveNo;

    constructor() {
        oracle.initialize(address(this), address(this));

        targetToken = IERC20(oracle.getTargetTokenAddr());
        baseToken = IERC20(oracle.getBaseTokenAddr());
        router = IUniswapV2Router02(oracle.getRouterAddr());
        pair = IUniswapV2Pair(oracle.getPairAddr());
        tokenAddrs = oracle.getRelevantTokenAddrs();
    }

    fallback() external payable {
        oracle.debug("in fallback");
    }

    receive() external payable {
        oracle.debug("in receive");
    }

    function _updateOracleTokenBalances() private {
        address[] memory addrs = new address[](2);
        (addrs[0], addrs[1]) = (address(this), address(pair));

        for (uint j = 0; j < addrs.length; j++) {
            for (uint i = 0; i < tokenAddrs.length; i++) {
                address tokenAddr = tokenAddrs[i];
                IERC20 tokenContract = IERC20(tokenAddr);
                uint tokenBalance = tokenContract.balanceOf(addrs[j]);
                oracle.updateTokenBalance(addrs[j], tokenAddr, tokenBalance);
            }
            oracle.updateTokenBalance(addrs[j], address(0x0), addrs[j].balance);
        }

    }

    function _approveAllTokenTransfers() private {
        address[] memory targetAddrs = oracle.getTargetAddrs();
        for (uint i = 0; i < tokenAddrs.length; i++) {
            address tokenAddr = tokenAddrs[i];
            IERC20 tokenContract = IERC20(tokenAddr);
            tokenContract.approve(address(router), type(uint256).max);
            tokenContract.approve(address(pair), type(uint256).max);
        }
    }

    function run() public {

        uint fee_on_transfer;

        _approveAllTokenTransfers();

        if (address(targetToken) == pair.token0()) {
            targetTokenReserveNo = 0;
            baseTokenReserveNo = 1;
        } else if (address(targetToken) == pair.token1()) {
            targetTokenReserveNo = 1;
            baseTokenReserveNo = 0;
        } else {
            oracle.panic();
        }

        uint[] memory reserves = new uint[](2);
        (reserves[0], reserves[1], ) = pair.getReserves();
        oracle.debug(string.concat("baseTokenReserve: ", Strings.toString(reserves[baseTokenReserveNo])));
        oracle.debug(string.concat("targetTokenReserve: ", Strings.toString(reserves[targetTokenReserveNo])));

        // 1. Swap to base token

        uint targetTokenBalance = targetToken.balanceOf(address(pair)) * 1 / 100;
        oracle.debug(string.concat("initial targetTokenBalance: ", Strings.toString(targetTokenBalance)));
        uint initialBaseTokenAmount = router.getAmountIn(targetTokenBalance, reserves[baseTokenReserveNo], reserves[targetTokenReserveNo]);
        oracle.debug(string.concat("initialBaseTokenAmount: ", Strings.toString(initialBaseTokenAmount)));

        {
            address WBNBaddr = address(0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c);
            address BSCUSDaddr = address(0x55d398326f99059fF775485246999027B3197955);
            address WETHaddr = address(0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2);
            address USDTaddr = address(0xdAC17F958D2ee523a2206206994597C13D831ec7);
            // uint initialEthAmount;

            if (address(baseToken) == WBNBaddr || address(baseToken) == WETHaddr) {
                IWBNB(address(baseToken)).deposit{value: initialBaseTokenAmount}();
                IWBNB(address(baseToken)).transfer(address(this), initialBaseTokenAmount);
            } else {
                address[] memory path = new address[](2);
                if (address(router) == address(0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D)) {
                    // mainnet
                    path[0] = WETHaddr;
                } else if (address(router) == address(0x10ED43C718714eb63d5aA57B78B54704E256024E)) {
                    // bsc
                    path[0] = WBNBaddr;
                } else {
                    oracle.debug("unknown router address");
                    oracle.panic();
                }
                path[1] = address(baseToken);
                router.swapETHForExactTokens{value: 1000000 ether}(initialBaseTokenAmount, path, address(this), type(uint).max);
            }
        }

        oracle.debug(string.concat("attacker base token balance: ", Strings.toString(baseToken.balanceOf(address(this)))));
        oracle.debug(string.concat("attacker target token balance: ", Strings.toString(targetToken.balanceOf(address(this)))));
        oracle.debug(string.concat("pair base token balance: ", Strings.toString(baseToken.balanceOf(address(pair)))));
        oracle.debug(string.concat("pair target token balance: ", Strings.toString(targetToken.balanceOf(address(pair)))));

        fee_on_transfer = 0;

        // 2. Swap to targetToken
        if (fee_on_transfer > 0) {
            uint baseTokenBalance = baseToken.balanceOf(address(this));
            baseToken.transfer(address(pair), baseTokenBalance);

            (reserves[0], reserves[1], ) = pair.getReserves();
            uint amountOut = router.getAmountOut(baseTokenBalance, reserves[baseTokenReserveNo], reserves[targetTokenReserveNo]);
            if (fee_on_transfer > 0) {
                amountOut = amountOut * 100 / (102 + fee_on_transfer); // amountOut considering fee
            }
    
            oracle.debug(string.concat("pair swap value: ", Strings.toString(amountOut)));
            if (targetTokenReserveNo == 0) {
                pair.swap(amountOut, 0, address(this), "");
            } else {
                pair.swap(0, amountOut, address(this), "");
            }
        } else {
            address[] memory path = new address[](2);
            path[0] = address(baseToken);
            path[1] = address(targetToken);
            router.swapExactTokensForTokensSupportingFeeOnTransferTokens(baseToken.balanceOf(address(this)), 0, path, address(this), type(uint).max);
        }

        oracle.debug(string.concat("attacker base token balance: ", Strings.toString(baseToken.balanceOf(address(this)))));
        oracle.debug(string.concat("attacker target token balance: ", Strings.toString(targetToken.balanceOf(address(this)))));
        oracle.debug(string.concat("pair base token balance: ", Strings.toString(baseToken.balanceOf(address(pair)))));
        oracle.debug(string.concat("pair target token balance: ", Strings.toString(targetToken.balanceOf(address(pair)))));

        // 3. Exploit

        uint loop_num = 0;
        uint amountOut = 0;
        uint prevAmountOut = 0;

        // Test here


        // 4. Swap back to base token
        targetTokenBalance = targetToken.balanceOf(address(this));
        address[] memory path = new address[](2);
        path[0] = address(targetToken);
        path[1] = address(baseToken);

        uint fee = targetTokenBalance * fee_on_transfer / 100;
        router.swapExactTokensForTokensSupportingFeeOnTransferTokens(targetTokenBalance - fee, 0, path, address(this), type(uint).max);
        uint finalBaseTokenBalance = baseToken.balanceOf(address(this));

        oracle.debug(string.concat("attacker target token balance: ", Strings.toString(targetToken.balanceOf(address(this)))));
        oracle.debug(string.concat("attacker base token balance: ", Strings.toString(baseToken.balanceOf(address(this)))));
        oracle.debug(string.concat("pair target token balance: ", Strings.toString(targetToken.balanceOf(address(pair)))));
        oracle.debug(string.concat("pair base token balance: ", Strings.toString(baseToken.balanceOf(address(pair)))));

        if (initialBaseTokenAmount >= finalBaseTokenBalance) {
            oracle.debug(string.concat("loss: ", Strings.toString((initialBaseTokenAmount - finalBaseTokenBalance))));

        } else {
            oracle.debug(string.concat("profit: ", Strings.toString((finalBaseTokenBalance - initialBaseTokenAmount))));
        }

    }

    function getCurrentAttackerPairBalances() private view returns (uint, uint) {
        uint attacker_balance = targetToken.balanceOf(address(this));
        uint pair_balance = targetToken.balanceOf(address(pair));
        return (attacker_balance, pair_balance);
    }

}