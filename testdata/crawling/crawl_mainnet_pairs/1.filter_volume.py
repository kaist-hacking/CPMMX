from web3 import Web3
from web3.middleware import geth_poa_middleware
import eth_abi
from Crypto.Hash import keccak
import csv
from tqdm import tqdm
from collections import deque
import argparse

threshold = 1000 * 10**18

WETH = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
USDT = "0xdAC17F958D2ee523a2206206994597C13D831ec7"
USDC = "0x55d398326f99059fF775485246999027B3197955"
BNB = "0xB8c77482e45F1F44dE1745F52C74426C631bDD52"
WBTC = "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599"

prices = {
    WETH: 2645.,
    USDT: 1.,
    USDC: 1.,
    BNB: 600.,
    WBTC: 67630.,
}

starts = [WETH, USDT, USDC, BNB, WBTC]

def filter_pairs(allpairs_path, output_file_path):

    with open(allpairs_path) as f:
        rdr = csv.reader(f)
        assert next(rdr) == ['i', 'pair', 'token0', 'token1', 'reserve0', 'reserve1', 'timestamp']
        data = list(rdr)

    v = []

    for line in tqdm(data):
        pair, token0, token1, reserve0, reserve1 = line[1], line[2], line[3], line[4], line[5]
        reserve0 = int(reserve0)
        reserve1 = int(reserve1)
        if reserve0 == 0 or reserve1 == 0:
            continue

        if token0 in starts and token1 not in starts:
            v.append([token1, token0, pair, reserve0 * prices[token0]])

        if token1 in starts and token0 not in starts:
            v.append([token0, token1, pair, reserve1 * prices[token1]])
        
    v.sort(key=lambda x: -x[3])
    v = list(filter(lambda x: x[3] > threshold, v))

    print("token-pair count", len(v))

    tokens = set()
    for x in v:
        tokens.add(x[0])
    print("tokens count", len(tokens))

    with open(output_file_path, 'w') as f:
        wtr = csv.writer(f)
        wtr.writerow(['target', 'base', 'pair', 'volume'])
        wtr.writerows([[x[0], x[1], x[2], str(x[3])] for x in v])

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description='Filter pairs by financial value')
    parser.add_argument('allpairs_path', type=str, help="path to allpairs file")
    parser.add_argument('output_file_path', type=str, help="path to output file")

    args = parser.parse_args()

    filter_pairs(args.allpairs_path, args.output_file_path)