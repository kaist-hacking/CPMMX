#!/usr/bin/env python3

import argparse, csv, os, glob
from datetime import datetime
import re

def convert_to_seconds(time_str):
    # Split the time string by '-' and remove 'h', 'm', 's' characters to extract numbers
    parts = time_str.replace('h', '').replace('m', '').replace('s', '').split('-')
    
    # Convert each part to an integer: hours, minutes, seconds
    hours, minutes, seconds = map(int, parts)
    
    # Calculate total seconds
    total_seconds = hours * 3600 + minutes * 60 + seconds
    
    return total_seconds


def parse_echidna_result_file(file_path):
    assert(file_path.endswith("result.txt"))

    result_pattern = "echidna_profit_generated_eth: "
    start_run_time = None
    latest_run_time = None
    datetime_format = "[%Y-%m-%d %H:%M:%S.%f]"

    with open(file_path, 'r') as file:
        for line in file:
            if line.startswith('[2024-'):
                timestamp = line.split(']')[0] + ']'
                if start_run_time == None:
                    start_run_time = datetime.strptime(timestamp, datetime_format)
                latest_run_time = (datetime.strptime(timestamp, datetime_format) - start_run_time).total_seconds()
                if latest_run_time > 1200:
                    return (False, latest_run_time)
            if result_pattern in line:
                if line.split(':')[-1] == "passing":
                    return (False, latest_run_time)
                else:
                    # print("INVARIANT BROKEN?")
                    # print(line)
                    return (True, latest_run_time)
            
    print("ERROR: log ended before 20 minutes")

    return (False, latest_run_time)

def parse_ityfuzz_result_file(file_path):
    assert(file_path.endswith(".result"))

    run_time_pattern = "run time: "
    vulnerability_found_pattern = "Found vulnerabilities!"
    latest_run_time = None

    with open(file_path, 'r') as file:
        for line in file:
            if run_time_pattern in line:
                start_index = line.find(run_time_pattern) + len(run_time_pattern)
                end_index = line.find(',', start_index)
                latest_run_time = convert_to_seconds(line[start_index:end_index])
                if latest_run_time > 1200:
                    return (False, latest_run_time)
            if vulnerability_found_pattern in line:
                return (True, latest_run_time)
            
    print("ERROR: log ended before 20 minutes")

    return (False, latest_run_time)

def parse_midas_result_file(file_path):
    assert(file_path.endswith(".result"))

    run_time_pattern = "run time: "
    vulnerability_found_pattern = "Found violations!"
    latest_run_time = None

    with open(file_path, 'r') as file:
        print(file_path)
        for line in file:
            if run_time_pattern in line:
                start_index = line.find(run_time_pattern) + len(run_time_pattern)
                end_index = line.find(',', start_index)
                latest_run_time = convert_to_seconds(line[start_index:end_index])
                if latest_run_time > 1200:
                    return (False, latest_run_time)
            if vulnerability_found_pattern in line:
                return (True, latest_run_time)
            
    print("ERROR: log ended before 20 minutes")

    return (False, latest_run_time)

def parse_ours_result_file(file_path):
    assert(file_path.endswith(".result"))

    # Regular expression to capture the seconds and nanoseconds
    time_pattern = r"time elapsed: Duration \{ secs: (\d+), nanos: (\d+) \}"
    success_pattern = r"Exploit found"
    fail_pattern = r"Could not find profitable testcase"

    time_elapsed = 1200
    ex_found = False

    with open(file_path, 'r') as file:
        for line in file:
            match_time = re.search(time_pattern, line)
            if match_time:
                # Extract secs and nanos
                secs = int(match_time.group(1))
                nanos = int(match_time.group(2))
                
                # Convert nanos to seconds and add to secs
                total_seconds = secs + nanos / 1_000_000_000
                
                time_elapsed = total_seconds
            match_success = re.search(success_pattern, line)
            if match_success:
                ex_found = True
            match_fail = re.search(fail_pattern, line)
            if match_fail:
                ex_found = False

    if time_elapsed >= 1200:
        ex_found = False

    # If no matching line is found, return None
    return (ex_found, time_elapsed)


def parse_result_file(tool, path):

    if tool == "ityfuzz":
        (detected, seconds_to_bug) = parse_ityfuzz_result_file(path)
    elif tool == "echidna":
        (detected, seconds_to_bug) = parse_echidna_result_file(path)
    elif tool == "midas":
        (detected, seconds_to_bug) = parse_midas_result_file(path)
    elif tool == "ours":
        (detected, seconds_to_bug) = parse_ours_result_file(path)

    return (detected, seconds_to_bug)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description='Parse results')
    
    parser.add_argument('tool', type=str, help='Which tool\'s result file are we parsing?', choices=['ityfuzz', 'echidna', 'midas', 'ours'])
    parser.add_argument('path', type=str, help='path to file or directory of files')

    args = parser.parse_args()

    path = args.path

    if os.path.isfile(path):
        (detected, seconds_to_bug) = parse_result_file(args.tool, path)
        if detected:
            print(f"detected: {detected}, seconds_to_bug: {seconds_to_bug}")
        else:
            print(f"detected: {detected}, execution_time: {seconds_to_bug}")

    elif os.path.isdir(path):
        if args.tool == "ityfuzz":
            search_pattern = os.path.join(path, '*.result')
        elif args.tool == "echidna":
            search_pattern = os.path.join(path, '**/result.txt')
        elif args.tool == "midas":
            search_pattern = os.path.join(path, '*.result')
        elif args.tool == "ours":
            search_pattern = os.path.join(path, "*.result")
        result_files = glob.glob(search_pattern)
        result_files = sorted(result_files, key=lambda x: x.lower())
        for file in result_files:
            if file.endswith("success_file.result") or file.endswith("panic_file.result"):
                continue
            (detected, seconds_to_bug) = parse_result_file(args.tool, file)
            if detected:
                print(f"{file}, {detected}, {seconds_to_bug}")
            else:
                print(f"{file}, {detected}, {seconds_to_bug}")

    else:
        print(f"{path} does not exist or is neither a file nor a directory.")