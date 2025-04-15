// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.0;

import "../OracleSupport.sol";
import "../lib/openzeppelin-contracts/contracts/utils/Strings.sol";

struct TestCase {
    EVMCall[] calls;
    EVMCall[] subcalls;
    EVMCall[] fallbacks;
}

contract Pier {

    Oracle oracle = Oracle(0x502be16aa82BAD01FDc3fEB3c5F8C431F8eeB8AE);
    IERC20 targetToken;
    IERC20 baseToken;
    IUniswapV2Router02 router;
    IUniswapV2Pair pair;
    address[] tokenAddrs;
    uint initialBaseTokenAmount;
    uint targetTokenReserveNo;
    uint baseTokenReserveNo;

    event CallAdded(
        When at,
        address to,
        bytes data,
        uint256 value
    );

    mapping(When => EVMCall[]) public db;

    function insert(
        When at, 
        EVMCall[] calldata calls) public {        
        for (uint i = 0; i < calls.length; i++) {
            db[at].push(calls[i]);
            // NOTE: For debugging
            // emit CallAdded(at, calls[i].to, calls[i].data, calls[i].value);
        }
    }

    function _run(When at) private {
        // TODO: Support multiple db if reply is called multiple times
        EVMCall[] storage calls = db[at];
        uint initialBaseTokenBalance;
        uint finalBaseTokenBalance;
        bool initialSwapSucceeded = false;

        if (calls.length == 0) {
            oracle.notifyOutOfCall(at);
            return;
        }
        
        if (address(targetToken) == pair.token0()) {
            targetTokenReserveNo = 0;
            baseTokenReserveNo = 1;
        } else if (address(targetToken) == pair.token1()) {
            targetTokenReserveNo = 1;
            baseTokenReserveNo = 0;
        } else {
            oracle.debug("Pair does not have have target token");
            oracle.panic();
        }

        initialBaseTokenBalance = baseToken.balanceOf(address(this));
        // oracle.debug(string.concat("initialBaseTokenBalance: ", Strings.toString(initialBaseTokenBalance)));

        _approveAllTokenTransfers();
        _updateOracleTokenBalances();

        for (uint i = 0; i < calls.length; i++) {
            bytes memory callData = calls[i].data;
            bytes memory newCallData = oracle.replacePlaceholderValue(calls[i].data);
            if (newCallData.length != 0) {
                // replace placeholder amount to balanceOf(this)
                // ETHtoToken calls should not enter here 
                callData = newCallData;
            }
            (bool success, ) = calls[i].to.call(callData);
            if (!initialSwapSucceeded) {
                if (success) {
                    initialSwapSucceeded = true;
                } else {
                    oracle.notifyInitialSwapFailed();
                    require(success, "Initial swap failed");
                }
            } else {
                require(success, "Call failed");
            }
            _updateOracleTokenBalances();
        }

        finalBaseTokenBalance = baseToken.balanceOf(address(this));
        // oracle.debug(string.concat("finalBaseTokenBalance: ", Strings.toString(finalBaseTokenBalance)));

        if (finalBaseTokenBalance > initialBaseTokenBalance) {
            uint profit = finalBaseTokenBalance - initialBaseTokenBalance;
            oracle.notifyExploitSuccess(profit);
        }
    }

    function run() public {
        targetToken = IERC20(oracle.getTargetTokenAddr());
        baseToken = IERC20(oracle.getBaseTokenAddr());
        router = IUniswapV2Router02(oracle.getRouterAddr());
        pair = IUniswapV2Pair(oracle.getPairAddr());
        tokenAddrs = oracle.getRelevantTokenAddrs();
        _run(When.NORMAL);
    }

    fallback() external payable {
        // oracle.debug("in fallback");
        // _run(When.FALLBACK);
    }

    receive() external payable {
        // oracle.debug("in receive");
    }

    function swapTargetTokenToBaseToken() public {

        uint[] memory reserves = new uint[](2);
        (reserves[0], reserves[1], ) = pair.getReserves();
        // oracle.debug(string.concat("targetTokenReserve: ", Strings.toString(reserves[targetTokenReserveNo])));

        uint fee = oracle.getFee();

        uint targetTokenBalance = targetToken.balanceOf(address(this));
        // oracle.debug(string.concat("targetTokenBalance: ", Strings.toString(targetTokenBalance)));

        uint targetTokenBalanceTransferFee = targetTokenBalance * (fee) / 100;

        address[] memory path = new address[](2);
        path[0] = address(targetToken);
        path[1] = address(baseToken);
        router.swapExactTokensForTokensSupportingFeeOnTransferTokens(targetTokenBalance - targetTokenBalanceTransferFee, 0, path, address(this), type(uint).max);
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
        }

        uint[] memory reserves = new uint[](2);
        (reserves[0], reserves[1], ) = pair.getReserves();
        oracle.updateTokenBalance(address(pair), address(0x0), reserves[baseTokenReserveNo]);
        oracle.updateTokenBalance(address(pair), address(0x1), reserves[targetTokenReserveNo]);
    }

    function _approveAllTokenTransfers() private {
        address[] memory targetAddrs = oracle.getTargetAddrs();
        for (uint j = 0; j < targetAddrs.length; j++) {
            address targetAddr = targetAddrs[j];
            for (uint i = 0; i < tokenAddrs.length; i++) {
                address tokenAddr = tokenAddrs[i];
                IERC20 tokenContract = IERC20(tokenAddr);
                tokenContract.approve(targetAddr, type(uint256).max);
            }
        }
    }
}

contract Bridge {
    Pier public mainPier;
    IERC20 targetToken;
    IERC20 baseToken;
    IUniswapV2Router02 router;
    IUniswapV2Pair pair;
    uint targetTokenReserveNo;
    uint baseTokenReserveNo;
    Oracle oracle = Oracle(0x502be16aa82BAD01FDc3fEB3c5F8C431F8eeB8AE);
    
    constructor() {
        mainPier = new Pier();
        oracle.initialize(address(this), address(mainPier));
    }

    function run(TestCase calldata tc) public {
        mainPier.insert(When.NORMAL, tc.calls);
        mainPier.insert(When.FALLBACK, tc.fallbacks);

        targetToken = IERC20(oracle.getTargetTokenAddr());
        baseToken = IERC20(oracle.getBaseTokenAddr());
        router = IUniswapV2Router02(oracle.getRouterAddr());
        pair = IUniswapV2Pair(oracle.getPairAddr());
        if (address(targetToken) == pair.token0()) {
            targetTokenReserveNo = 0;
            baseTokenReserveNo = 1;
        } else if (address(targetToken) == pair.token1()) {
            targetTokenReserveNo = 1;
            baseTokenReserveNo = 0;
        } else {
            oracle.debug("Bridge, pair does not have target token");
            oracle.panic();
        }

        setupInitialBalance();
        mainPier.run();
    }

    function setupInitialBalance() public {

        uint pairBaseTokenBalance = baseToken.balanceOf(address(pair));
        uint pairTargetTokenBalance = targetToken.balanceOf(address(pair));
        // oracle.debug(string.concat("pair base token balance: ", Strings.toString(pairBaseTokenBalance)));
        // oracle.debug(string.concat("pair target token balance: ", Strings.toString(pairTargetTokenBalance)));

        uint[] memory reserves = new uint[](2);
        (reserves[0], reserves[1], ) = pair.getReserves();
        // oracle.debug(string.concat("baseTokenReserve: ", Strings.toString(reserves[baseTokenReserveNo])));
        // oracle.debug(string.concat("targetTokenReserve: ", Strings.toString(reserves[targetTokenReserveNo])));

        // 1. Calculate appropriate amount of base tokens
        uint percentOfPair = oracle.getInitialTokenPercent();
        uint initialBaseTokenAmount = router.getAmountIn(pairTargetTokenBalance * percentOfPair / 100, reserves[baseTokenReserveNo], reserves[targetTokenReserveNo]);
        // oracle.debug(string.concat("initialBaseTokenAmount: ", Strings.toString(initialBaseTokenAmount)));

        // 2. Swap to base token
        {
            address WBNBaddr = address(0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c);
            address WETHaddr = address(0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2);

            if (address(baseToken) == WBNBaddr || address(baseToken) == WETHaddr) {
                IWBNB(address(baseToken)).deposit{value: initialBaseTokenAmount}();
                IWBNB(address(baseToken)).transfer(address(mainPier), initialBaseTokenAmount);
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
                router.swapETHForExactTokens{value: 1000000 ether}(initialBaseTokenAmount, path, address(mainPier), type(uint).max);
            }
        }

    }

    function saveBalanceSnapshot() public {
        oracle.saveBalanceSnapshot();
    }

    function checkInvariantBroken() public {
        oracle.checkInvariantBroken();
    }

    function swapBaseTokenToTargetToken() public {
        uint fee = oracle.getFee();

        uint[] memory reserves = new uint[](2);
        (reserves[0], reserves[1], ) = pair.getReserves();
        uint baseTokenBalance = baseToken.balanceOf(address(mainPier));

        uint amountOut = router.getAmountOut(baseTokenBalance, reserves[baseTokenReserveNo], reserves[targetTokenReserveNo]);
        
        baseToken.transferFrom(address(mainPier), address(pair), baseTokenBalance);
        // additional 2% fee for rounding errors
        if (targetTokenReserveNo == 0) {
            pair.swap(amountOut * 100 / (102 + fee), 0, address(mainPier), "");
        } else {
            pair.swap(0, amountOut * 100 / (102 + fee) , address(mainPier), "");
        }
    }

    function swapTargetTokenToBaseToken() public {
        mainPier.swapTargetTokenToBaseToken();
    }

    fallback() external payable {
        // oracle.debug("Bridge, in fallback");
    }

    receive() external payable {
        // oracle.debug("Bridge, in receive");
    }

    function calculateBurnAmount() public {
        uint256 burnAmount = targetToken.totalSupply() - 2 * (targetToken.totalSupply() / targetToken.balanceOf(address(pair)));
        oracle.registerBurnAmount(burnAmount);
    }

}