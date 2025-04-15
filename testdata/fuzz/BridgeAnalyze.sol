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
        uint[] memory reserves = new uint[](2);

        _approveAllTokenTransfers();

        // while (oracle.hasNextCall()) {
        //     EVMCall memory call = oracle.getNextCall();
        //     (bool success, ) = call.to.call{value: call.value}(call.data);
        //     _updateOracleTokenBalances();
        // }

        // swap to targetToken
        uint initial_eth_amount = 0.01 ether;

        uint pairBaseTokenBalance = baseToken.balanceOf(address(pair));
        uint pairTargetTokenBalance = targetToken.balanceOf(address(pair));

        oracle.debug(string.concat("pair base token balance: ", Strings.toString(pairBaseTokenBalance / baseToken.decimals())));
        oracle.debug(string.concat("pair target token balance: ", Strings.toString(pairTargetTokenBalance / targetToken.decimals())));

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
            // uint amountOut = 9970 * baseTokenBalance * reserves[targetTokenReserveNo] / (reserves[baseTokenReserveNo] * 10000 + 9970 * baseTokenBalance);
            uint amountOut = router.getAmountOut(baseTokenBalance, reserves[baseTokenReserveNo], reserves[targetTokenReserveNo]);
            if (targetTokenReserveNo == 0) {
                pair.swap(amountOut / 2, 0, address(this), "");
            } else {
                pair.swap(0, amountOut / 2, address(this), "");
            }

            oracle.debug(Strings.toString(baseToken.balanceOf(address(this))));
            oracle.debug(Strings.toString(targetToken.balanceOf(address(this))));
        }

        // if (address(baseToken) == WBNBaddr) {
        //     address[] memory path = new address[](2);
        //     path[0] = WBNBaddr;
        //     path[1] = address(targetToken);
        //     router.swapExactETHForTokensSupportingFeeOnTransferTokens{value: initial_eth_amount}(0, path, address(this), type(uint).max);
        // } else if (address(baseToken) == USDCaddr) {
        //     address[] memory path = new address[](3);
        //     path[0] = WBNBaddr;
        //     path[1] = USDCaddr;
        //     path[2] = address(targetToken);        
        //     router.swapExactETHForTokensSupportingFeeOnTransferTokens{value: initial_eth_amount}(0, path, address(this), type(uint).max);
        // } else if (address(baseToken) == WETHaddr) {
        //     address[] memory path = new address[](2);
        //     path[0] = WETHaddr;
        //     path[1] = address(targetToken);
        //     router.swapExactETHForTokensSupportingFeeOnTransferTokens{value: initial_eth_amount}(0, path, address(this), type(uint).max);
        // } else if (address(baseToken) == USDTaddr) {
        //     address[] memory path = new address[](3);
        //     path[0] = WETHaddr;
        //     path[1] = USDTaddr;
        //     path[2] = address(targetToken);        
        //     router.swapExactETHForTokensSupportingFeeOnTransferTokens{value: initial_eth_amount}(0, path, address(this), type(uint).max);
        // } else {
        //     oracle.panic();
        // }
    
        // B. Calculate fees and bonuses

        oracle.debug(string.concat("precision: ", Strings.toString(precision)));

        // 1. attacker transfer to pair
        {
            uint beforeAttackerBalance;
            uint beforePairBalance;
            uint afterAttackerBalance;
            uint afterPairBalance;
            uint transactionAmount;
            uint senderBalanceLoss;
            uint receiverBalanceGain;

            oracle.debug("1. attacker transfer to pair");
            oracle.debug("sender: attacker, receiver: pair");

            (beforeAttackerBalance, beforePairBalance) = getCurrentAttackerPairBalances();

            transactionAmount = beforeAttackerBalance / 2 ;
            targetToken.transfer(address(pair), transactionAmount);

            (afterAttackerBalance, afterPairBalance) = getCurrentAttackerPairBalances();

            senderBalanceLoss = beforeAttackerBalance - afterAttackerBalance;
            receiverBalanceGain = afterPairBalance - beforePairBalance;

            calculateFeesOrBonuses(transactionAmount, senderBalanceLoss, receiverBalanceGain);
        }

        // 2. pair skim pair
        {
            uint beforePairBalance;
            uint afterPairBalance;
            uint transactionAmount;
            uint beforeAttackerAsset;
            uint afterAttackerAsset;

            oracle.debug("2. pair skim pair");

            (, beforePairBalance) = getCurrentAttackerPairBalances();
            beforeAttackerAsset = getCurrentAttackerAsset();

            (reserves[0], reserves[1], ) = pair.getReserves();
            transactionAmount = beforePairBalance - reserves[targetTokenReserveNo];

            pair.skim(address(pair));

            (, afterPairBalance) = getCurrentAttackerPairBalances();
            afterAttackerAsset = getCurrentAttackerAsset();

            calculateFeeOrBonus(transactionAmount, beforePairBalance, afterPairBalance);
            if (afterAttackerAsset > beforeAttackerAsset) {
                oracle.debug(string.concat("attacker asset increase: ", Strings.toString(afterAttackerAsset - beforeAttackerAsset)));
            } else if (afterAttackerAsset < beforeAttackerAsset) {
                oracle.debug(string.concat("attacker asset decrease: ", Strings.toString(beforeAttackerAsset - afterAttackerAsset)));
            }
        }


        // 3. pair skim this
        {
            uint beforeAttackerBalance;
            uint beforePairBalance;
            uint afterAttackerBalance;
            uint afterPairBalance;
            uint transactionAmount;
            uint senderBalanceLoss;
            uint receiverBalanceGain;

            oracle.debug("3. pair skim this");
            oracle.debug("sender: pair, receiver: attacker");

            (beforeAttackerBalance, beforePairBalance) = getCurrentAttackerPairBalances();

            (reserves[0], reserves[1], ) = pair.getReserves();
            transactionAmount = beforePairBalance - reserves[targetTokenReserveNo];
            pair.skim(address(this));

            (afterAttackerBalance, afterPairBalance) = getCurrentAttackerPairBalances();

            senderBalanceLoss = beforePairBalance - afterPairBalance;
            receiverBalanceGain = afterAttackerBalance - beforeAttackerBalance;

            calculateFeesOrBonuses(transactionAmount, senderBalanceLoss, receiverBalanceGain);
        }


        // 4. this transfer this
        {
            uint beforeAttackerBalance;
            uint beforePairBalance;
            uint afterAttackerBalance;
            uint afterPairBalance;
            uint transactionAmount;
            uint beforeAttackerAsset;
            uint afterAttackerAsset;

            oracle.debug("4. this transfer this");

            (beforeAttackerBalance, beforePairBalance) = getCurrentAttackerPairBalances();
            beforeAttackerAsset = getCurrentAttackerAsset();

            transactionAmount = beforeAttackerBalance / 2;
            targetToken.transfer(address(this), transactionAmount);

            (afterAttackerBalance, afterPairBalance) = getCurrentAttackerPairBalances();
            afterAttackerAsset = getCurrentAttackerAsset();

            oracle.debug("attacker fee or bonus");
            calculateFeeOrBonus(transactionAmount, beforeAttackerBalance, afterAttackerBalance);
            oracle.debug("pair fee or bonus");
            calculateFeeOrBonus(transactionAmount, beforePairBalance, afterPairBalance);
            if (afterAttackerAsset > beforeAttackerAsset) {
                oracle.debug(string.concat("attacker asset increase: ", Strings.toString(afterAttackerAsset - beforeAttackerAsset)));
            } else if (afterAttackerAsset < beforeAttackerAsset) {
                oracle.debug(string.concat("attacker asset decrease: ", Strings.toString(beforeAttackerAsset - afterAttackerAsset)));
            }
        }

    }

    function calculateFeesOrBonuses(uint transactionAmount, uint senderBalanceLoss, uint receiverBalanceGain) private {
        uint fee;
        uint fee_percent;
        uint bonus;
        uint bonus_percent;
        
        if (senderBalanceLoss > transactionAmount) {
            fee = senderBalanceLoss - transactionAmount;
            fee_percent = fee * 100 * precision / transactionAmount;
            oracle.debug(string.concat("sender fee: ", Strings.toString(fee_percent)));
        } else if (senderBalanceLoss < transactionAmount) {
            bonus = transactionAmount - senderBalanceLoss;
            bonus_percent = bonus * 100 * precision / transactionAmount;
            oracle.debug(string.concat("sender bonus: ", Strings.toString(bonus_percent)));
        } else {
            oracle.debug("No fee or bonus to sender");
        }

        if (receiverBalanceGain > transactionAmount) {
            bonus = receiverBalanceGain - transactionAmount;
            bonus_percent = bonus * 100 * precision / transactionAmount;
            oracle.debug(string.concat("receiver bonus: ", Strings.toString(bonus_percent)));
        } else if (receiverBalanceGain < transactionAmount) {
            fee = transactionAmount - receiverBalanceGain;
            fee_percent = fee * 100 * precision / transactionAmount;
            oracle.debug(string.concat("receiver fee: ", Strings.toString(fee_percent)));
        } else {
            oracle.debug("No fee or bonus to receiver");
        }

    }

    function calculateFeeOrBonus(uint transactionAmount, uint before_balance, uint after_balance) private {
        uint fee;
        uint fee_percent;
        uint bonus;
        uint bonus_percent;

        if (before_balance > after_balance) {
            fee = before_balance - after_balance;
            fee_percent = fee * 100 * precision / transactionAmount;
            oracle.debug(string.concat("fee: ", Strings.toString(fee_percent)));
        } else if (after_balance > before_balance) {
            bonus = after_balance - before_balance;
            bonus_percent = bonus * 100 * precision / transactionAmount;
            oracle.debug(string.concat("bonus: ", Strings.toString(bonus_percent)));
        } else {
            oracle.debug("No fee or bonus");
        }

    }

    function getCurrentAttackerPairBalances() private view returns (uint, uint) {
        uint attacker_balance = targetToken.balanceOf(address(this));
        uint pair_balance = targetToken.balanceOf(address(pair));
        return (attacker_balance, pair_balance);
    }

    function getCurrentAttackerAsset() private returns (uint) {
        uint[] memory reserves = new uint[](2);
        (reserves[0], reserves[1], ) = pair.getReserves();
        uint attacker_balance = targetToken.balanceOf(address(this));
        // oracle.debug(string.concat("attacker_balance: ", Strings.toString(attacker_balance)));
        uint skim_balance = targetToken.balanceOf(address(pair)) - reserves[targetTokenReserveNo];
        // oracle.debug(string.concat("skim_balance: ", Strings.toString(skim_balance)));
        return attacker_balance + skim_balance;
    }

}