#!/usr/bin/env python3

from concurrent.futures import ProcessPoolExecutor, as_completed
import argparse, logging, csv, os
import subprocess

logging.basicConfig(level=logging.INFO)

WORKER_NUM = 5
MAINNET_RPC_ENDPOINT = "" #MAINNET
BSC_RPC_ENDPOINT = "" #BSC
ETHERSCAN_API_KEY = ""
BSCSCAN_API_KEY = " "

def ensure_directory_exists(directory_path):
    # Check if the directory exists
    if not os.path.exists(directory_path):
        # If it doesn't exist, create it
        os.makedirs(directory_path)
        print(f"Directory created: {directory_path}")
    else:
        print(f"Directory already exists: {directory_path}")


def parse_input_file(file_path):
    with open(file_path, 'r') as csvfile:
        input_addrs = list(csv.DictReader(csvfile))

    return input_addrs

def write_to_file(file_path, content):
    with open(file_path, 'a+') as file:
        file.write(content + ' \n')

def task_function(input_addrs, time_limit, result_dir_path):

    timeout_csv = os.path.join(result_dir_path, "timeout.csv")
    panic_file = os.path.join(result_dir_path, "panic_file.result")
    success_file = os.path.join(result_dir_path, "success_file.result")
    etc_csv = os.path.join(result_dir_path, "etc.csv")
    invariant_broken_but_not_profitable_csv = os.path.join(result_dir_path, "invariant_broken_but_not_profitable.csv")

    # token_name = input_addrs['token_name']
    target_token_addr = input_addrs['target']
    base_token_addr = input_addrs['base']
    pair_addr = input_addrs['pair']
    if input_addrs['chain']  == 'eth':
        rpc_endpoint = MAINNET_RPC_ENDPOINT
        api_key = ETHERSCAN_API_KEY
    else:
        rpc_endpoint = BSC_RPC_ENDPOINT
        api_key = BSCSCAN_API_KEY
    block_number = int(input_addrs['blocknum'])-1 # one block before exploit
    # block_number = 25543755 # block number for synthetic dataset experiment
    logging.info(f"trying to generate exploit for {target_token_addr}, {base_token_addr}, {pair_addr}...")
    command = f"timeout {int(time_limit)+2} forge cage test {target_token_addr} {base_token_addr} {pair_addr} {api_key} -f {rpc_endpoint} --fork-block-number {block_number} > {result_dir_path}/{target_token_addr}.result"
    try:
        result = subprocess.run([command], shell=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=time_limit)
        print(f"returncode: {result.returncode}, inputs: {target_token_addr}, {base_token_addr}, {pair_addr}")
        if result.returncode == 0:
            # Successful completion
            write_to_file(success_file, f"Result for token: {target_token_addr} base: {base_token_addr} pair: {pair_addr}\n{result.stdout}")
        elif result.returncode == 1:
            # Err
            pass
        elif result.returncode == 134:
            # panic
            write_to_file(panic_file, f"Result for token: {target_token_addr}, base: {base_token_addr}, pair: {pair_addr}\n{result.stderr}")
        elif result.returncode == 135:
            # Could not find invariant-breaking testcase
            pass
        elif result.returncode == 136:
            # Found invariant-breaking testcase, but could not find profitable testcase
            write_to_file(invariant_broken_but_not_profitable_csv, f"{target_token_addr},{base_token_addr},{pair_addr}")
        else:
            # unknown return code
            write_to_file(etc_csv, f"{target_token_addr},{base_token_addr},{pair_addr}")
        
    except subprocess.TimeoutExpired or TimeoutError:
        write_to_file(timeout_csv, f"{target_token_addr},{base_token_addr},{pair_addr}")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description='Command-line argument example')
    
    parser.add_argument('input_file', type=str, help='path to file with `token addr, base addr, pair addr, ...` ')
    parser.add_argument('time_limit', type=int, help='time budget for each target in seconds')
    parser.add_argument('result_dir_path', type=str, help='directory to store all results')

    args = parser.parse_args()

    logging.info(f"file_path: {args.input_file}")
    logging.info(f"time_limit: {args.time_limit}")

    timeout_csv = os.path.join(args.result_dir_path, "timeout.csv")
    panic_file = os.path.join(args.result_dir_path, "panic_file.result")
    success_file = os.path.join(args.result_dir_path, "success_file.result")
    etc_csv = os.path.join(args.result_dir_path, "etc.csv")
    invariant_broken_but_not_profitable_csv = os.path.join(args.result_dir_path, "invariant_broken_but_not_profitable.csv")

    logging.info(f"timeout_file: {timeout_csv}")
    logging.info(f"panic_file: {panic_file}")
    logging.info(f"success_file: {success_file}")

    # Parse input file
    input_rows = parse_input_file(args.input_file)
    input_rows[:1]

    logging.info(f"input parsed, {len(input_rows)} rows")

    # Initialize Result Files
    ensure_directory_exists(args.result_dir_path)
    write_to_file(timeout_csv, "target,base,pair")
    write_to_file(etc_csv, "target,base,pair")
    write_to_file(invariant_broken_but_not_profitable_csv, "target,base,pair")

    processes = list()
    current_index = 0

    with ProcessPoolExecutor(max_workers=WORKER_NUM) as executor:
        futures = [executor.submit(task_function, row, args.time_limit, args.result_dir_path) for row in input_rows]
        for future in as_completed(futures):
            pass

    print("completed all tasks")
