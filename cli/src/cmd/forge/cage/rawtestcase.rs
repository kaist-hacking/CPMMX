use std::sync::{Arc, RwLock};

use super::{swap_template::{SwapTemplate, SwapType}, testcase::{EVMCall, TestCase}, exploit_template::ExploitTemplate};
use ethers::{types::{U256}};
use forge::executor::inspector::oracle::{self, CageEnv};

#[derive(Debug, Clone)]
pub struct RawTestCase {
    pub initial_eth_amount: U256,
    pub prefix_swaps: Vec<EVMCall>,
    pub suffix_swaps: Vec<EVMCall>,
    pub mutable_calls: Vec<EVMCall>
}

impl RawTestCase {

    pub fn new(env: Arc<RwLock<CageEnv>>, initial_eth_amount: U256, swap_templates: Vec<SwapTemplate>) -> Self {

        let mut prefix_swaps = Vec::new();
        let mut suffix_swaps = Vec::new();

        let readable_cage_env = env.read().unwrap();

        for swap_template in swap_templates.iter() {
            let main_pier_addr = readable_cage_env.system_addrs.get("main_pier").unwrap().clone();

            let swap_abi = match readable_cage_env.targets.get(&swap_template.swap_addr) {
                Some(_abi) => _abi,
                None => {
                    panic!("Could not find {:x} in targets", swap_template.swap_addr);
                }
            };
            let (swap_call, reverse_swap_call) = match swap_template.swap_type {
                SwapType::TokenEth => {
                    unreachable!();
                    let _swap_call = swap_template.generate_eth_to_token_swap_call(swap_abi, initial_eth_amount, main_pier_addr);
                    let _reverse_swap_call = swap_template.generate_token_to_eth_swap_call(swap_abi, oracle::THIS_BALANCE, main_pier_addr);
                    (_swap_call, _reverse_swap_call)
                },
                SwapType::TokenToken => {
                    let _swap_call = swap_template.generate_reverse_token_to_token_swap_call(swap_abi, oracle::THIS_BALANCE, main_pier_addr);
                    let _reverse_swap_call = swap_template.generate_token_to_token_swap_call(swap_abi, oracle::THIS_BALANCE, main_pier_addr);
                    (_swap_call, _reverse_swap_call)
                }
            };

            prefix_swaps.insert(0, swap_call);
            suffix_swaps.push(reverse_swap_call);
        }

        Self {
            initial_eth_amount,
            prefix_swaps,
            suffix_swaps,
            mutable_calls: Vec::new(),
        }

    }

    pub fn merge_with_exploit_template(&self, exploit_template: &ExploitTemplate, repeat: i32) -> RawTestCase {        
        let mut new_raw_tc = self.clone();
        new_raw_tc.mutable_calls = exploit_template.prefix_calls.clone();
        for _ in 0..repeat {
            new_raw_tc.mutable_calls = [new_raw_tc.mutable_calls, exploit_template.repeated_calls.clone()].concat();
        }
        new_raw_tc.mutable_calls = [new_raw_tc.mutable_calls, exploit_template.suffix_calls.clone()].concat();
        new_raw_tc
    }

    pub fn to_tc(&self) -> TestCase {
        let mut calls = self.prefix_swaps.clone();
        calls = [calls, self.mutable_calls.clone()].concat();
        calls = [calls, self.suffix_swaps.clone()].concat();

        TestCase {
            calls,
            subcalls: Vec::new(),
            callbacks: Vec::new(),
        }
    }

}