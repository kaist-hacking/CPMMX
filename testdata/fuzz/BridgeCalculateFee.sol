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
    uint precision = 100; // should divide result by 100 to get percent fee/bonus percent
    uint targetTokenReserveNo;
    uint baseTokenReserveNo;

    constructor() {
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

    function _approveAllTokenTransfers() private {
        for (uint i = 0; i < tokenAddrs.length; i++) {
            address tokenAddr = tokenAddrs[i];
            IERC20 tokenContract = IERC20(tokenAddr);
            tokenContract.approve(address(router), type(uint256).max);
            tokenContract.approve(address(pair), type(uint256).max);
        }
    }

    function run() public {
        uint[] memory reserves = new uint[](2);

        _approveAllTokenTransfers();

        // swap to targetToken
        uint initial_eth_amount = 0.1 ether;

        uint pairBaseTokenBalance = baseToken.balanceOf(address(pair));
        uint pairTargetTokenBalance = targetToken.balanceOf(address(pair));

        if (address(targetToken) == pair.token0()) {
            targetTokenReserveNo = 0;
            baseTokenReserveNo = 1;
        } else if (address(targetToken) == pair.token1()) {
            targetTokenReserveNo = 1;
            baseTokenReserveNo = 0;
        } else {
            oracle.panic();
        }

        // A. Prepare targetToken

        // 1. Swap to base token
        {
            address WBNBaddr = address(0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c);
            address WETHaddr = address(0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2);

            if (address(baseToken) == WBNBaddr || address(baseToken) == WETHaddr) {
                IWBNB(address(baseToken)).deposit{value: initial_eth_amount}();
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
                router.swapExactETHForTokensSupportingFeeOnTransferTokens{value: initial_eth_amount}(0, path, address(this), type(uint).max);
            }
        }

        // 2. Swap to targetToken
        {
            uint baseTokenBalance = baseToken.balanceOf(address(this));
            baseToken.transfer(address(pair), baseTokenBalance);

            (reserves[0], reserves[1], ) = pair.getReserves();
            uint amountOut = router.getAmountOut(baseTokenBalance, reserves[baseTokenReserveNo], reserves[targetTokenReserveNo]);
            // works up to 100% fee
            if (targetTokenReserveNo == 0) {
                pair.swap(amountOut / 2, 0, address(this), "");
            } else {
                pair.swap(0, amountOut / 2, address(this), "");
            }

            // oracle.debug(Strings.toString(baseToken.balanceOf(address(this))));
        }
    
        // B. Calculate fees on transfer

        oracle.debug(string.concat("precision: ", Strings.toString(precision)));

        {
            uint beforeAttackerBalance;
            uint beforePairBalance;
            uint afterAttackerBalance;
            uint afterPairBalance;
            uint transactionAmount;
            uint senderBalanceLoss;
            uint receiverBalanceGain;

            (beforeAttackerBalance, beforePairBalance) = getCurrentAttackerPairBalances();

            oracle.debug("reach 1");

            transactionAmount = beforeAttackerBalance / 2 ;
            targetToken.transfer(address(pair), transactionAmount);

            oracle.debug("reach 2");

            (afterAttackerBalance, afterPairBalance) = getCurrentAttackerPairBalances();

            senderBalanceLoss = beforeAttackerBalance - afterAttackerBalance;
            receiverBalanceGain = afterPairBalance - beforePairBalance;

            oracle.debug("reach 3");

            calculateFeesOrBonuses(transactionAmount, senderBalanceLoss, receiverBalanceGain);
        }

    }

    function calculateFeesOrBonuses(uint transactionAmount, uint senderBalanceLoss, uint receiverBalanceGain) private {
        uint fee;
        uint fee_percent;
        uint bonus;
        uint bonus_percent;
        
        oracle.debug("in calculateFeesorBonuses");
        
        if (senderBalanceLoss > transactionAmount) {
            fee = senderBalanceLoss - transactionAmount;
            fee_percent = fee * 100 * precision / transactionAmount;
            oracle.debug(string.concat("sender fee: ", Strings.toString(fee_percent)));
            oracle.registerFee((fee_percent + 99) / 100);
        } else if (senderBalanceLoss < transactionAmount) {
            bonus = transactionAmount - senderBalanceLoss;
            bonus_percent = bonus * 100 * precision / transactionAmount;
            oracle.debug(string.concat("sender bonus: ", Strings.toString(bonus_percent)));
        } else {
            oracle.debug("No fee or bonus to sender");
        }
    }

    function getCurrentAttackerPairBalances() private view returns (uint, uint) {
        uint attacker_balance = targetToken.balanceOf(address(this));
        uint pair_balance = targetToken.balanceOf(address(pair));
        return (attacker_balance, pair_balance);
    }

}