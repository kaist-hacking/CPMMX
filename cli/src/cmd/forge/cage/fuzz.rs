use super::{*, corpus::Corpus, 
    testcase::{TestCase, EVMCall}, rawtestcase::RawTestCase, exploit_template::ExploitTemplate};
use std::{fmt::Debug, mem::swap, str::FromStr, sync::{Arc, RwLock}, collections::BTreeMap, fs::File, io::Write};
use cast::executor::{Executor, inspector::{oracle::Bug, oracle::CageEnv}, RawCallResult};
use ethers::{
    abi::{Abi, StateMutability, Token, Tokenizable},
    types::{U256, Address, Log},
    solc::{utils::RuntimeOrHandle}};
use forge::{trace::{CallTraceDecoderBuilder, identifier::{EtherscanIdentifier, SignaturesIdentifier}, CallTraceArena}, decode::decode_console_logs};
use foundry_utils::scan::{Scanner};
use chrono::{DateTime, Local};

const REPEAT_STEP: i32 = 10;
const REPEAT_MAX_STEP: i32 = 100;

#[derive(Debug)]
pub struct Cage {
    pub corpus: Corpus,
    config: CageConfig,
    executor: Executor,
    pub env: Arc<RwLock<CageEnv>>,
    bridge: Option<DeployedContract>,
    pub scanner: Arc<Scanner>,
    testcase_save_path: String,
    start_time: Option<DateTime<Local>>,
}

#[derive(Debug, Clone)]
pub struct DeployedContract {
    pub contract: Abi,
    pub address: H160
}

impl Cage {
    pub fn new(executor: Executor, 
                config: CageConfig, 
                env: Arc<RwLock<CageEnv>>, 
                scanner: Arc<Scanner>) -> Self {
        Self {
            corpus: Corpus::new(),
            config,
            executor,
            env,
            bridge: None,
            scanner,
            testcase_save_path: "".to_string(),
            start_time: None,
        }
    }

    pub fn found_exploit_exit(&self) {
        let end_time = Local::now();
        let time_elapsed = end_time.signed_duration_since(self.start_time.unwrap());
        println!("time elapsed: {:?}", time_elapsed);
        std::process::exit(0);
    }

    pub fn start(&mut self) {
        self.start_time = Some(Local::now());

        let block_number = self.executor.env().block.number;
        println!("Block number: {}", block_number);
        // Initialize corpus with basic test cases
        self.corpus.init(self.scanner.clone(), self.env.clone(), self.bridge.as_ref().unwrap().address);

        // Check whether any of the basic test cases result in invariant violation
        let basic_exploit_templates = self.corpus.get_basic_exploit_templates();
        let mut invariant_breaking_templates = self.try_break_invariant(&basic_exploit_templates).unwrap_or_else(|e| {
            panic!("{}", e);
        });
        println!("Found {} templates breaking invariant!", invariant_breaking_templates.len());


        // Check whether exploit has already been found
        for (bug, _) in invariant_breaking_templates.iter() {
            match bug {
                Bug::ProfitGenerated(profit) => {
                    println!("Exploit found early!\nprofit: {}\n", profit);
                    self.found_exploit_exit();
                },
                _ => {
                    // do nothing
                }          
            }
        }

        // Increase call diversity
        let diverse_call_exploit_templates = self.introduce_state_changing_functions(&basic_exploit_templates).unwrap();
        let more_invariant_breaking_templates = self.try_break_invariant(&diverse_call_exploit_templates).unwrap_or_else(|e| {
            panic!("{}", e);
        });
        invariant_breaking_templates = [invariant_breaking_templates, more_invariant_breaking_templates].concat();
        println!("After introducing state-changing calls, found {} templates breaking invariant!", invariant_breaking_templates.len());
        // Check whether exploit has already been found
        for (bug, _) in invariant_breaking_templates.iter() {
            match bug {
                Bug::ProfitGenerated(profit) => {
                    println!("Exploit found early!\nprofit: {}\n", profit);
                    self.found_exploit_exit();
                },
                _ => {
                    // do nothing
                }          
            }
        }

        if invariant_breaking_templates.len() == 0 {
            let end_time = Local::now();
            let time_elapsed = end_time.signed_duration_since(self.start_time.unwrap());
            println!("time elapsed: {:?}", time_elapsed);
            println!("Could not find invariant-breaking testcase. Exiting...");
            std::process::exit(135);
        }

        // Execute test cases with repetitions to find profitable testcase
        let mut writable_cage_env = self.env.write().unwrap();
        writable_cage_env.deep_search_phase = true;
        drop(writable_cage_env);

        let mut invariant_breaking_templates_with_loop_num: Vec<(Bug, ExploitTemplate, i32)> = invariant_breaking_templates.iter().map(|t|{(t.0.clone(), t.1.clone(), 2)}).collect();
        while invariant_breaking_templates_with_loop_num.len() > 0 {
            let (_survived_invariant_breaking_templates_with_loop_num, exploit_found)  = self.execute_with_repeat(invariant_breaking_templates_with_loop_num);
            invariant_breaking_templates_with_loop_num = _survived_invariant_breaking_templates_with_loop_num;
            match exploit_found {
                Ok((_, _)) => {
                    self.found_exploit_exit();
                },
                Err(_) => {
                    // println!("{}", e)
                }
            }
        }

        let end_time = Local::now();
        let time_elapsed = end_time.signed_duration_since(self.start_time.unwrap());
        println!("time elapsed: {:?}", time_elapsed);

        println!("Could not find profitable testcase. Exiting...");
        std::process::exit(136);

    }

    pub fn execute_with_repeat(&mut self, mut exploit_templates_with_loop_num: Vec<(Bug, ExploitTemplate, i32)>) -> (Vec<(Bug, ExploitTemplate, i32)>, Result<(RawTestCase, U256)>) {
        
        let mut no_profit_until_max_repeat: Vec<(Bug, ExploitTemplate, i32)> = Vec::new();

        while exploit_templates_with_loop_num.len() > 0 {
            let (bug, exploit_template, loop_num) = exploit_templates_with_loop_num.pop().unwrap();
            let soft_max = loop_num + REPEAT_STEP;
            let hard_max = loop_num + REPEAT_MAX_STEP;
            match self.execute_with_repetition_to_find_exploit(bug.clone(), exploit_template.clone(), loop_num, soft_max, hard_max) {
                Ok((raw_tc, repeat_num, profit)) => {
                    if profit == 0u64.into() {
                        // Repeated until loop_num + REPEAT_STEP or loop_num + REPEAT_MAX_STEP times but still not profitable
                        no_profit_until_max_repeat.push((bug, exploit_template, repeat_num));
                    } else {
                        println!("Exploit found!\nprofit: {}\n", profit);
                        println!("Staring with {} of pair balance", exploit_template.initial_token_percent);
                        println!("{:?}", raw_tc);
                        return (no_profit_until_max_repeat, Ok((raw_tc, profit)));
                    }
                },
                Err(e) => {
                    // println!("{}", e)
                    // Discard the testcase
                }
            }
        }
        return (no_profit_until_max_repeat, Err(eyre::eyre!("Could not find profitable testcase")));
    }

    pub fn introduce_state_changing_functions(&mut self, exploit_templates: &Vec<ExploitTemplate> ) -> Result<Vec<ExploitTemplate>> {
        trace!("In try_break_invariant advanced");

        // Add state changing calls
        let readable_cage_env = self.env.read().unwrap();
        let target_addr = readable_cage_env.target_token.unwrap();
        let target_abi = readable_cage_env.targets.get(&target_addr).unwrap();
        let main_pier_addr = readable_cage_env.system_addrs.get("main_pier").unwrap().clone();
        let pair_addr = readable_cage_env.pair.unwrap();

        let mut state_changing_calls: Vec<EVMCall> = Vec::new();
        
        let transfer_this_zero_call = self.corpus.exploit_ingredients.get("this_transfer_this_zero").unwrap();
        state_changing_calls.push(transfer_this_zero_call.clone());

        let transfer_pair_zero_call = self.corpus.exploit_ingredients.get("this_transfer_pair_zero").unwrap();
        state_changing_calls.push(transfer_pair_zero_call.clone());

        for function in target_abi.functions() {
            if function.name == "transfer" {
                let transfer_this_one_tokens = vec![
                    Token::Address(main_pier_addr),
                    Token::Uint(U256::one()),
                ];
                let transfer_this_one_calldata = function.encode_input(&transfer_this_one_tokens).unwrap();
                let transfer_this_one_call = EVMCall {
                    to: target_addr,
                    value: 0u64.into(),
                    name: function.name.clone(),
                    calldata: transfer_this_one_calldata,
                    args: transfer_this_one_tokens,
                };
                state_changing_calls.push(transfer_this_one_call);

                let transfer_pair_one_tokens = vec![
                    Token::Address(pair_addr),
                    Token::Uint(U256::one()),
                ];
                let transfer_pair_one_calldata = function.encode_input(&transfer_pair_one_tokens).unwrap();
                let transfer_pair_one_call = EVMCall {
                    to: target_addr,
                    value: 0u64.into(),
                    name: function.name.clone(),
                    calldata: transfer_pair_one_calldata,
                    args: transfer_pair_one_tokens,
                };
                state_changing_calls.push(transfer_pair_one_call);
            }

            // Non-view, no-argument functions
            if function.inputs.len() != 0 || function.state_mutability == StateMutability::View {
                continue;
            }
            let empty_token = vec![];
            let call = EVMCall {
                to: target_addr,
                value: 0u64.into(),
                name: function.name.clone(),
                calldata: function.encode_input(&empty_token).unwrap(),
                args: Vec::new(),
            };
            state_changing_calls.push(call);
        }

        drop(readable_cage_env);

        let mut new_exploit_templates = Vec::new();
        for original_exploit_template in exploit_templates.iter() {
            for state_changing_call in state_changing_calls.iter() {
                let mut add_call_to_repeat = original_exploit_template.clone();
                add_call_to_repeat.repeated_calls.push(state_changing_call.clone());
                new_exploit_templates.push(add_call_to_repeat);
                let mut add_call_to_suffix = original_exploit_template.clone();
                add_call_to_suffix.suffix_calls.push(state_changing_call.clone());
                new_exploit_templates.push(add_call_to_suffix);
            }
        }

        return Ok(new_exploit_templates);
    }

    pub fn try_break_invariant(&mut self, exploit_templates: &Vec<ExploitTemplate>) -> Result<Vec<(Bug, ExploitTemplate)>> {
        trace!("In try_break_invariant");

        let verbosity = self.config.evm_opts.verbosity;

        let mut base_tc = self.corpus.base_testcases.get(0).unwrap().clone();

        let bridge = self.bridge.as_ref().unwrap();
        let empty_token = vec![];

        // Call to save balance snapshots
        let save_balance_snapshop_function = bridge.contract.function("saveBalanceSnapshot").unwrap();
        let save_balanace_snapshot_calldata = save_balance_snapshop_function.encode_input(&empty_token).unwrap();
        let save_balance_snapshop_call = EVMCall {
            to: bridge.address ,
            value: 0u64.into(),
            name: save_balance_snapshop_function.name.clone(),
            calldata: save_balanace_snapshot_calldata,
            args: Vec::new(),
        };

        // Call to check if invariant has been broken (detects AttackerTokenGain and PairTokenLoss)
        let check_invariant_broken_function = bridge.contract.function("checkInvariantBroken").unwrap();
        let check_invariant_broken_calldata = check_invariant_broken_function.encode_input(&empty_token).unwrap();
        let check_invariant_broken_call = EVMCall {
            to: bridge.address,
            value: 0u64.into(),
            name: check_invariant_broken_function.name.clone(),
            calldata: check_invariant_broken_calldata,
            args: Vec::new(),
        };

        let mut invariant_breaking_templates = Vec::new();

        let mut tried_calculating_fee_on_transfer = false;

        let mut potential_invariant_breaking_templates = exploit_templates.clone();

        while potential_invariant_breaking_templates.len() > 0 {
            let mut exploit_template = potential_invariant_breaking_templates.pop().unwrap();

            exploit_template.prefix_calls.push(save_balance_snapshop_call.clone());
            exploit_template.suffix_calls.push(check_invariant_broken_call.clone());
            
            let mut writable_cage_env = self.env.write().unwrap();
            writable_cage_env.initial_token_percent = Some(exploit_template.initial_token_percent);
            drop(writable_cage_env);

            let new_tc = base_tc.merge_with_exploit_template(&exploit_template, 1);
            let tc = new_tc.to_tc();

            let mut result = self.run_target(&tc)?;

            let oracle = &result.oracle.unwrap();

            if verbosity > 1 {
                // Print new_tc to execute
                println!("{}", tc.to_string_pretty(self).unwrap());
                println!("Initial target token balance: {}% of pair balance", exploit_template.initial_token_percent);
                println!("{}", oracle);
            }
            if let Some(bug) = &oracle.bug {
                match bug {
                    Bug::InitialSwapFailed => {
                        if !tried_calculating_fee_on_transfer {
                            let _ = self.calculate_fee()?;
                            tried_calculating_fee_on_transfer = true;
                            let readable_cage_env = self.env.read().unwrap();
                            if readable_cage_env.fee_on_transfer.is_some() {
                                // Rerun current and future templates with different swap template
                                base_tc = self.corpus.base_testcases.get(1).unwrap().clone();
                                exploit_template.prefix_calls.pop(); // remove saveBalanceSnapshot
                                exploit_template.suffix_calls.pop(); // remove checkInvariantBroken
                                potential_invariant_breaking_templates.insert(0, exploit_template);
                            }
                        }
                    },
                    Bug::PairTokenLoss => {
                        invariant_breaking_templates.push((bug.clone(), exploit_template.clone()));
                        // Add new test cases with sync (if not already included)
                        if exploit_template.repeated_calls.iter().find(|call| call.name == "sync").is_none() 
                            && exploit_template.suffix_calls.iter().find(|call| call.name == "sync").is_none() {
                            let sync = self.corpus.exploit_ingredients.get("sync").unwrap(); // pair.sync()
                            exploit_template.prefix_calls.pop(); // remove saveBalanceSnapshot
                            exploit_template.suffix_calls.pop(); // remove checkInvariantBroken
                            let mut exploit_template_with_repeated_sync = exploit_template.clone();
                            exploit_template_with_repeated_sync.repeated_calls.push(sync.clone());
                            potential_invariant_breaking_templates.insert(0, exploit_template_with_repeated_sync);
                            // invariant_breaking_templates.push((bug.clone(), exploit_template_with_repeated_sync));
                            let mut exploit_template_with_single_sync = exploit_template.clone();
                            exploit_template_with_single_sync.suffix_calls.push(sync.clone());
                            potential_invariant_breaking_templates.insert(0, exploit_template_with_single_sync);
                            // invariant_breaking_templates.push((bug.clone(), exploit_template_with_single_sync));
                        }
                    },
                    Bug::AttackerTokenGain => {
                        invariant_breaking_templates.push((bug.clone(), exploit_template.clone()));
                    },
                    Bug::ProfitGenerated(_)=> {
                        invariant_breaking_templates.push((bug.clone(), exploit_template.clone()));
                        break;
                    },
                    Bug::RequirementViolation => {
                        // do nothing
                    }
                }
            } 

            if verbosity > 3 {
                // Print Logs
                self.print_logs(&result.logs);
                //Print Traces
                self.print_traces(&mut result.traces, result.labels.clone())?;
            }

        }
        
        Ok(invariant_breaking_templates)
    } 

    pub fn execute_with_repetition_to_find_exploit(&mut self, bug: Bug, _exploit_template: ExploitTemplate, start_repeat: i32, soft_max: i32, hard_max: i32) -> Result<(RawTestCase, i32, U256)> {

        let mut exploit_template = _exploit_template.clone();

        let mut base_tc = self.corpus.base_testcases.get(0).unwrap().clone();

        let readable_cage_env = self.env.read().unwrap();
        if readable_cage_env.fee_on_transfer.is_some() {
            base_tc = self.corpus.base_testcases.get(1).unwrap().clone();
        }
        let base_token_addr = readable_cage_env.base_token.unwrap();
        drop(readable_cage_env);

        let mut loop_num = start_repeat;
        let mut previous_final_balance = U256::zero();
        let mut duplicate_final_balance_count = 0;
        let mut increasing_final_balance = false;

        // Find number of repeats required for exploit
        loop {

            if loop_num >= hard_max {
                // Stop repetition (even though final balance is increasing)
                return Ok((base_tc, loop_num, 0u64.into()));
            }

            if !increasing_final_balance && loop_num >= soft_max {
                // Stop repetition for now because final balance is decreasing and tried repeating for REPEAT_STEP times
                return Ok((base_tc, loop_num, 0u64.into()));
            }

            println!("loop_num: {}", loop_num);
            
            let mut writable_cage_env = self.env.write().unwrap();
            writable_cage_env.initial_token_percent = Some(exploit_template.initial_token_percent);
            drop(writable_cage_env);

            let new_tc = base_tc.merge_with_exploit_template(&exploit_template, loop_num);
            let tc = new_tc.to_tc();

            let mut result = self.run_target(&tc)?;

            if let Some(oracle) = &result.oracle {
                println!("{}", oracle);
                if let Some(bug) = &oracle.bug {
                    trace!(target: "cage", "Found a bug: {:?}", bug);
                    match bug {
                        Bug::RequirementViolation => {
                            return Err(eyre::eyre!("RequirementViolation during repeat"));
                        },
                        Bug::ProfitGenerated(profit) => {
                            return Ok((new_tc, loop_num, profit.clone()));
                        },
                        Bug::InitialSwapFailed => {
                            unreachable!();  
                        },
                        _ => { // PairBalanceLoss or AttackerTokenGain
                            // do not terminate
                        },
                    }
                } else {
                    return Err(eyre::eyre!("Invariant not broken"));
                }
                // need more loops
                let final_balance = oracle.balances.get(&base_token_addr).unwrap().clone();
                if final_balance <= previous_final_balance {
                    increasing_final_balance = false;
                    println!("Loop did not increase final balance final_balance: {:}, previous_final_balance: {:}",
                        final_balance, previous_final_balance);
                    if duplicate_final_balance_count > 3 {
                        return Err(eyre::eyre!("Loop stuck at final_balance {:} for 3 times", final_balance));
                    } 
                    if final_balance == previous_final_balance {
                        duplicate_final_balance_count = duplicate_final_balance_count + 1;
                    } else {
                        duplicate_final_balance_count = 0;
                    }
                } else {
                    increasing_final_balance = true;
                    duplicate_final_balance_count = 0;
                }
                previous_final_balance = final_balance;
                loop_num = loop_num + 1;
            }

        }

    }

    pub fn execute_tc(&mut self, testcase_to_execute: String) -> Result<()> {
        let tc_str = std::fs::read_to_string(testcase_to_execute)?;
        let tc: TestCase = serde_json::from_str(&tc_str)?;
        let mut result = self.run_target(&tc)?;

        let verbosity = self.config.evm_opts.verbosity;
        if verbosity > 0 {
            // Print Logs
            self.print_logs(&result.logs);
            // Print Traces
            self.print_traces(&mut result.traces, result.labels.clone())?;
        }

        if let Some(oracle) = &result.oracle {
            println!("{}", oracle);
            if let Some(bug) = &oracle.bug {
                trace!(target: "cage", "Found a bug: {:?}", bug);
                match bug {
                    Bug::RequirementViolation => {
                        return Err(eyre::eyre!("requirement violation"));
                    }
                    _ => {
                    },
                }
            }
        }
        Ok(())
    }

    pub fn execute_sol(&mut self) -> Result<()> {
        
        // Calculate % fees in all transfers
        let mut result = self.start_bridge()?;
        
        let verbosity = self.config.evm_opts.verbosity;
        if verbosity > 0 {
            // Print Logs
            self.print_logs(&result.logs);
            // Print Traces
            self.print_traces(&mut result.traces, result.labels.clone())?;
        }

        Ok(())
    }

    pub fn start_bridge(&mut self) -> Result<RawCallResult> {
        let bridge = self.bridge.as_ref().unwrap();
        let f = bridge.contract.function("run")?;
        let calldata = f
                .encode_input(&Vec::new())?;

        let result = self.executor.call_raw(
            self.config.evm_opts.sender.clone(),
            bridge.address,
            calldata.into(),
            0.into(),
        );

        result
    }

    pub fn run_target(&mut self, tc: &TestCase) -> Result<RawCallResult> {
        let bridge = self.bridge.as_ref().unwrap();
        let f = bridge.contract.function("run")?;
        let calldata = f
                .encode_input(&[tc.clone().into_token()])?;

        let result = self.executor.call_raw(
            self.config.evm_opts.sender.clone(),
            bridge.address,
            calldata.into(),
            0.into(),
        );

        result
    }

    pub fn calculate_fee(&mut self) -> Result<RawCallResult> {
        let bridge_calculate_fee = self.setup_bridge("./fuzz/BridgeCalculateFee.sol".to_owned()).unwrap();

        // Set balance of BridgeCalculateFee to max
        self.executor.set_balance(bridge_calculate_fee.address, U256::MAX)?;

        let f = bridge_calculate_fee.contract.function("run")?;
        let calldata = f
                .encode_input(&Vec::new())?;

        let result = self.executor.call_raw(
            self.config.evm_opts.sender.clone(),
            bridge_calculate_fee.address,
            calldata.into(),
            0.into(),
        );

        result
    }

    pub fn setup(&mut self, frontend_path: String) -> Result<()> {
        // Set the balance of the default sender to max
        self.executor.set_balance(self.config.evm_opts.sender, U256::MAX)?;

        // Setup target
        let target: DeployedContract = self.setup_target()?;
        trace!(target: "cage", "Setup a target: {:?}", target);

        // Setup bridge
        let bridge = self.setup_bridge(frontend_path)?;
        self.bridge = Some(bridge.clone());
        trace!(target: "cage", "Setup a bridge: {:?}", self.bridge);
        // Set the balance of bridge to max
        self.executor.set_balance(self.bridge.as_ref().unwrap().address, U256::MAX)?;

        let mut writable_cage_env = self.env.write().unwrap();
        writable_cage_env.targets.insert(bridge.address, bridge.contract);
        drop(writable_cage_env);

        // Create directory to store generated testcases
        // let curr_date_time = Local::now();
        // let testcase_storage_path = format!("./TC_{}", curr_date_time.format("%Y%m%d_%H%M%S"));
        // if !Path::new(&testcase_storage_path).exists() {
        //     std::fs::create_dir_all(&testcase_storage_path);
        // }
        // self.testcase_save_path = testcase_storage_path;

        Ok(())
    }
    

    fn setup_bridge(&mut self, frontend_path: String) -> Result<DeployedContract> {
        let bridge = self.compile_bridge(frontend_path)?;
        let result = self.executor.deploy(
            self.config.evm_opts.sender.clone(), 
            bridge.bin.clone().unwrap().as_bytes().unwrap().0.clone(),
            0.into(), bridge.abi.as_ref())?;
        Ok(DeployedContract {
            contract: bridge.abi.unwrap(),
            address: result.address
        })
    }

    fn compile_bridge(&self, frontend_path: String) -> Result<CompactContract> {
        let source = std::fs::read_to_string(frontend_path).unwrap();

        let info = ContractInfo::new("Bridge");
        let mut sources = Sources::new();
        sources.insert("Bridge.sol".into(), Source { content: source });

        let project = self.config.foundry_config.project()?;
        let output = ProjectCompiler::with_sources(&project, sources)?.compile()?;
        let artifact = output.find_contract(info).unwrap();
        Ok(artifact.clone().into())
    }
    
    fn setup_target(&mut self) -> Result<DeployedContract> {

        let target_contract = &self.config.target_token;

        // Get contract ABI from etherscan
        let target_token_addr = Address::from_str(target_contract).unwrap_or_else(|_| {
            panic!("Invalid <CONTRACT> format for target token");
        });
        let base_token_addr = Address::from_str(&self.config.base_token).unwrap_or_else(|_| {
            panic!("Invalid <CONTRACT> format for base token");
        });
        let pair_addr = Address::from_str(&self.config.pair).unwrap_or_else(|_| {
            panic!("Invalid <CONTRACT> format for pair");
        });

        let target_token_abi_addr = if let Ok(impl_addr) = self.scanner.is_proxy_addr(&target_token_addr) {
            println!("Is proxy contract");
            impl_addr
        } else {
            target_token_addr
        };

        let target_token_abi = match self.scanner.get_contract_abi(&target_token_abi_addr) {
            Ok(abi) => abi,
            Err(_) => self.scanner.get_default_erc20_abi(),
        };

        let base_token_abi = match self.scanner.get_contract_abi(&base_token_addr) {
            Ok(abi) => abi,
            Err(_) => self.scanner.get_default_erc20_abi(),
        };
        let pair_abi = match self.scanner.get_contract_abi(&pair_addr) {
            Ok(abi) => abi,
            Err(_) => self.scanner.get_default_pair_abi(),
        };

        // Insert to targets
        let mut writable_cage_env = self.env.write().unwrap();
        writable_cage_env.target_token = Some(target_token_addr);
        writable_cage_env.targets.insert(target_token_addr, target_token_abi.clone());
        writable_cage_env.base_token = Some(base_token_addr);
        writable_cage_env.targets.insert(base_token_addr, base_token_abi);
        writable_cage_env.pair = Some(pair_addr);
        writable_cage_env.targets.insert(pair_addr, pair_abi);

        println!("target_token: 0x{:x}", target_token_addr);
        println!("base_token: 0x{:x}", base_token_addr);
        println!("pair: 0x{:x}", pair_addr);

        // Add target token and base token to targets
        writable_cage_env.relevant_token_addrs.push(target_token_addr);
        writable_cage_env.relevant_token_addrs.push(base_token_addr);

        drop(writable_cage_env);

        let deployed_contract =  DeployedContract {
            contract: target_token_abi,
            address: target_token_addr,
        };

        return Ok(deployed_contract)
    }

    fn print_logs(&self, logs: &Vec<Log>) {
        let console_logs = decode_console_logs(logs);
        if !console_logs.is_empty() {
            println!("Logs:");
            for log in console_logs {
                println!("  {log}");
            }   
            println!();
        }
    }

    fn print_traces(&self, traces: &mut Option<CallTraceArena>, labels: BTreeMap<H160, String>) -> Result<()>{
        let verbosity = self.config.evm_opts.verbosity;
        let remote_chain_id = self.config.evm_opts.get_remote_chain_id();
        let mut etherscan_identifier = EtherscanIdentifier::new(&self.config.foundry_config, remote_chain_id)?;
        let sig_identifier = SignaturesIdentifier::new(Config::foundry_cache_dir(), self.config.foundry_config.offline)?;

        let mut decoder = CallTraceDecoderBuilder::new()
            .with_labels(labels)
            .with_verbosity(verbosity)
            .build();
        decoder.add_signature_identifier(sig_identifier.clone());

        let mut decoded_traces: Vec<String> = Vec::new();
        let rt = RuntimeOrHandle::new();
        for trace in traces {
            decoder.identify(trace, &mut etherscan_identifier);
            
            rt.block_on(decoder.decode(trace));
            decoded_traces.push(trace.to_string());
        }

        if !decoded_traces.is_empty() {
            println!("Traces:");
            decoded_traces.into_iter().for_each(|trace| println!("{trace}"));
        }

        Ok(())
    }

    fn save_tc(&self, tc: &TestCase, count: u32) -> Result<()> {
        let tc_save_file_name = format!("{}/{}", self.testcase_save_path, count);
        let mut file = File::create(&tc_save_file_name)?;
        let serialized_testcase = serde_json::to_string(tc)?;
        file.write_all(serialized_testcase.as_bytes())?;
        Ok(())
    }

}
