from web3 import Web3
from web3.middleware import geth_poa_middleware
import eth_abi
from Crypto.Hash import keccak
import time
import csv
from tqdm import trange
import argparse

WBNB = "0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c"
BUSD = "0xe9e7CEA3DedcA5984780Bafc599bD69ADd087D56"
USDT = "0x55d398326f99059fF775485246999027B3197955"
USDC = "0x8AC76a51cc950d9822D68b83fE1Ad97B32Cd580d"
STABLECOINS = [WBNB, BUSD, USDT, USDC]

NODE_HTTP_PATH = 'https://rpc.ankr.com/bsc' 
BSC_PANCAKE_FACTORY_ADDR = '0xcA143Ce32Fe78f1f7019d7d551a6402fC5350c73'

def sig(data):
    k = keccak.new(digest_bits=256)
    k.update(data.encode())
    return "0x" + k.hexdigest()[:8]

def get_pairs(factory, output_file):
    w3 = Web3(Web3.HTTPProvider(NODE_HTTP_PATH))

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

    get_pairs(BSC_PANCAKE_FACTORY_ADDR, args.output_file_path)

