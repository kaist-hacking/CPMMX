# CPMMX

## Notice

This repository contains the code and dataset used in the paper [Automated Attack Synthesis for Constant Product Market Makers (accepted at ISSTA 2025)](https://arxiv.org/abs/2404.05297).

Our tool, CPMMX, is built on top of [Foundry](https://github.com/foundry-rs/foundry).

The dataset `./testdata/defihacklabs_exploits.csv` is a subset of exploits in the [DeFiHackLabs repository](https://github.com/SunWeb3Sec/DeFiHackLabs).

The dataset `./testdata/blocksec_exploits.csv` is a modified version of [exploits reported by BlockSec](https://x.com/BlockSecTeam/status/1624077078852210691).

Please refer to our [paper](https://arxiv.org/abs/2404.05297) for more details.

[![DOI](https://zenodo.org/badge/966579709.svg)](https://doi.org/10.5281/zenodo.15223115)

## Environment Setup

### Sync submodules
```
git submodule update --init --recursive
```
- Needed to download openzeppelin-contracts

### Rust
- Install [rustup](https://www.rust-lang.org/tools/install)
- Set rustc version for this repository
```
rustup override set 1.73.0
```
- Check rustc version with `rustc --version`, result should be
```
rustc 1.73.0 (cc66ad468 2023-10-03)
```

### Install dependencies
```
sudo apt install build-essential cmake pkg-config openssl libssl-dev libclang-dev clang
```
### Install foundry
```
cd ./foundryup && ./install
```

### Change foundry to local version
```
foundryup --path <PATH TO THIS REPOSITORY>
```
- use absolute path

## Testing individual contracts
Navigate to `./testdata`
```
cd ./testdata
```

Command
```
forge cage test <VULNERABLE TOKEN ADDR> <STABLECOIN ADDR> <DEX ADDR> <ETHERSCAN API KEY> -f <RPC_URL> --fork-block-number <FORK_BLOCK_NUMBER>
```
- `fork-block-number` is optional

Example: ANCH Exploit
```
forge cage test 0xA4f5d4aFd6b9226b3004dD276A9F778EB75f2e9e 0x55d398326f99059fF775485246999027B3197955 0xaD0dA05b9C20fa541012eE2e89AC99A864CC68Bb <ETHERSCAN API KEY> -f https://rpc.ankr.com/bsc --fork-block-number 20302534
```
- (Added 25/04/15) Ankr nodes are not working, use quicknode instead
- To use quicknode urls (or any other urls), you have to add the url to `./utils/src/scan.rs`

## Running on Datasets
Navigate to `./testdata`
```
cd ./testdata
```

Command
```
./run_on_network.py <PATH_TO_DATASET> <TIMEOUT_IN_SECONDS> <DIR_TO_SAVE_RESULTS>
```
- Recommended to use reliable node rpc url (ex. QuickNode subscription) rather than the default free one (most likely will not run on free RPC URLs)
- Add URL and API_KEY to `./run_on_network.py`

Example: DeFiHackLabs Dataset
```
./run_on_network.py ./datasets/defihacklab_exploits.csv 1200 ./results/
```
- You can see example result files in `./testdata/defihacklab_results` and `./testdata/blocksec_results`

## Running on blockchains
Navigate to `./testdata`
```
cd ./testdata
```
Install dependencies
```
pip install -r requirement.txt
```
Command
```
./auto_exploit.py
```
- Recommended to run on a local blockchain node (most likely will not run on free RPC URLs)
