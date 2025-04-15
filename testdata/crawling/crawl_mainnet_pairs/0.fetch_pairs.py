from web3 import Web3
from web3.middleware import geth_poa_middleware
import eth_abi
from Crypto.Hash import keccak
import time
import csv
from tqdm import trange
import argparse

WETH = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
USDT = "0xdAC17F958D2ee523a2206206994597C13D831ec7"
USDC = "0x55d398326f99059fF775485246999027B3197955"
BNB = "0xB8c77482e45F1F44dE1745F52C74426C631bDD52"
WBTC = "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599"
STABLECOINS = [WETH, USDT, USDC, BNB, WBTC]

NODE_HTTP_PATH = 'https://rpc.ankr.com/eth'
MAINNET_UNISWAP_FACTORY_ADDR = '0x5C69bEe701ef814a2B6a3EDD4B1652CB9cc5aA6f'

def sig(data):
    k = keccak.new(digest_bits=256)
    k.update(data.encode())
    return "0x" + k.hexdigest()[:8]

def get_pairs(factory, output_file):
    w3 = Web3(Web3.HTTPProvider(NODE_HTTP_PATH))
    print(w3)
    w3.middleware_onion.inject(geth_poa_middleware, layer=0)

    assert w3.is_connected()

    start = 0

    # get number of pairs from factory
    calldata = sig('allPairsLength()')
    output = w3.eth.call({'to': factory, 'data': calldata})
    end = eth_abi.decode(['uint256'],output)[0]

    started = time.time()

    with open(output_file, 'w') as f:
        wtr = csv.writer(f)
        wtr.writerow(['i', 'pair', 'token0', 'token1', 'reserve0', 'reserve1', 'timestamp'])
        for i in trange(start, end):
            calldata = sig('allPairs(uint256)') + eth_abi.encode(['uint256'], [i]).hex()
            output = w3.eth.call({'to': factory, 'data': calldata})
            pair = w3.to_checksum_address(eth_abi.decode(['address'], output)[0])

            calldata = sig('token0()')
            output = w3.eth.call({'to': pair, 'data': calldata})
            token0 = w3.to_checksum_address(eth_abi.decode(['address'], output)[0])

            calldata = sig('token1()')
            output = w3.eth.call({'to': pair, 'data': calldata})
            token1 = w3.to_checksum_address(eth_abi.decode(['address'], output)[0])

            print(token0)
            print(token1)

            if str(token0) in STABLECOINS or str(token1) in STABLECOINS:
                calldata = sig('getReserves()')
                output = w3.eth.call({'to': pair, 'data': calldata})
                reserve0, reserve1, timestamp = eth_abi.decode(['uint112','uint112','uint32'], output)
                wtr.writerow([str(i), pair, token0, token1, str(reserve0), str(reserve1), str(timestamp)])

    print(time.time() - started)

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description='Filter pairs by financial value')
    parser.add_argument('output_file_path', type=str, help="path to output file")

    args = parser.parse_args()

    get_pairs(MAINNET_UNISWAP_FACTORY_ADDR, args.output_file_path)

