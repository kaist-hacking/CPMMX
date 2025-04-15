use bytes::Bytes;
use ethers::{abi::{Address, Uint, AbiEncode, AbiDecode, Abi}, types::{H160, Bytes as EtherBytes, U256}};
use revm::{Database, EVMData, Inspector, Interpreter, Return, opcode, CallInputs, Gas};
use tracing::trace;
use std::{fmt, option::Option, collections::{HashMap, BTreeMap}, sync::{RwLock, Arc}, str::FromStr};
use eyre::Result;
use crate::{abi::{HEVMCalls, EVMCall}, executor::{backend::DatabaseExt}};
use foundry_utils::scan::Scanner;

// The oracle handler address (0x502be16aa82BAD01FDc3fEB3c5F8C431F8eeB8AE).
pub const ORACLE_ADDRESS: Address = H160([
    0x50, 0x2b, 0xe1, 0x6a, 0xa8, 0x2B, 0xAD, 0x01, 0xFD, 0xc3, 0xfE, 0xB3, 0xc5, 0xF8, 0xC4, 0x31,
    0xF8, 0xee, 0xB8, 0xAE,
]);

// initial address (0x00a329c0648769a73afac7f9381e08fb43dbea72).
pub const INITIAL_ADDRESS: Address = H160([
    0x00, 0xa3, 0x29, 0xc0, 0x64, 0x87, 0x69, 0xa7, 0x3a, 0xfa, 0xc7, 0xf9, 0x38, 0x1e, 0x08, 0xfb, 
    0x43, 0xdb, 0xea, 0x72,
]);

pub const SWAP_EXACT_ETH_FOR_TOKENS: [u8; 4] = [0x7f, 0xf3, 0x6a, 0xb5];
pub const SWAP_EXACT_ETH_FOR_TOKENS_SUPPORTING_FEE_ON_TRANSFER_TOKENS: [u8; 4] = [0xb6, 0xf9, 0xde, 0x95];

pub const SWAP_EXACT_TOKENS_FOR_ETH: [u8; 4] = [0x18, 0xcb, 0xaf, 0xe5];
pub const SWAP_EXACT_TOKENS_FOR_ETH_SUPPORTING_FEE_ON_TRANSFER_TOKENS: [u8; 4] = [0x79, 0x1a, 0xc9, 0x47];

pub const SWAP_EXACT_TOKENS_FOR_TOKENS_SUPPORTING_FEE_ON_TRANSFER_TOKENS: [u8; 4] = [0x5c, 0x11, 0xd7, 0x95];
pub const TRANSFER: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

pub const BURN_UINT: [u8; 4] = [0x42, 0x96, 0x6c, 0x68];
pub const BURN_ADDRESS_UINT: [u8; 4] = [0x9d, 0xc2, 0x9f, 0xac];
pub const DELIVER: [u8; 4] = [0x3b, 0xd5, 0xd1, 0x73];

pub const ZERO_BALANCE: U256 = U256([0x0000000000000000, 0x0000000000000000, 0x0000000000000000, 0x0000000000000000]);
pub const THIS_BALANCE: U256 = U256([0x1000000000000000, 0x0000000000000000, 0x0000000000000000, 0x0000000000000000]);
pub const PAIR_BALANCE: U256 = U256([0x2000000000000000, 0x0000000000000000, 0x0000000000000000, 0x0000000000000000]);
pub const PAIR_BALANCE_MINUS_ONE: U256 = U256([0x3000000000000000, 0x0000000000000000, 0x0000000000000000, 0x0000000000000000]);
pub const BURN_AMOUNT: U256 = U256([0x4000000000000000, 0x0000000000000000, 0x0000000000000000, 0x0000000000000000]);

#[derive(Debug, Clone, PartialEq)]
pub enum CallType {
    Swap,
    AttackerTransferPair,
    PairSkimPair,
    PairSkimThis,
    AttackerTransferAttacker,
}

#[derive(Debug, Clone)]
pub struct AnalyzeTemplate {
    pub call_type: CallType,
    pub call: EVMCall,
    pub amount: U256,
    pub sender_balance_diff: U256,
    pub receiver_balance_diff: U256,
}

#[derive(Debug, Clone, Default)]
pub struct CageEnv {
    // target contracts to send transactions to
    pub target_token: Option<Address>,
    pub base_token: Option<Address>,
    pub pair: Option<Address>,
    pub targets: BTreeMap<H160, Abi>,

    // for managing native currency & ERC20 tokens
    pub relevant_token_addrs: Vec<Address>,
    // fixed during fuzzing
    pub system_addrs: BTreeMap<String, H160>,

    pub fee_on_transfer: Option<U256>,
    pub initial_token_percent: Option<U256>,

    // calls to execute
    pub calls_to_execute: Option<Vec<AnalyzeTemplate>>,

    pub deep_search_phase: bool,
}

impl fmt::Display for CageEnv {
 fn fmt (&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "targets\n{:?}\nrelevant_token_addrs\n{:?}\n ",
        self.targets.keys(), self.relevant_token_addrs)
 }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Bug {
    RequirementViolation,
    PairTokenLoss,
    AttackerTokenGain,
    ProfitGenerated(U256), // profit
    InitialSwapFailed,
}

impl fmt::Display for Bug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Bug::RequirementViolation => {
                write!(f, "Requirement violation")
            },
            Bug::PairTokenLoss => {
                write!(f, "PairTokenLoss")
            },
            Bug::AttackerTokenGain => {
                write!(f, "AttackerTokenGain")
            },
            Bug::ProfitGenerated(profit) => {
                write!(f, "ProfitGenerated, profit: {}", profit)
            }
            Bug::InitialSwapFailed => {
                write!(f, "InitialSwapFailed")
            }
        }
    }
}

pub struct Oracle {
    pub bug: Option<Bug>, 
    pub cage_env: Arc<RwLock<CageEnv>>,
    pub balances: HashMap<Address, Uint>, 
    pub pair_balances: HashMap<Address, Uint>, // 0x0 baseTokenReserve, 0x1 targetTokenReserve
    pub balances_snapshot: Option<HashMap<Address, Uint>>,
    pub pair_balances_snapshot: Option<HashMap<Address, Uint>>,
    scanner: Arc<Scanner>,
    burn_amount: Option<U256>,
}

impl fmt::Debug for Oracle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Oracle")
            .field("bug", &self.bug)
            .field("cage_env", &self.cage_env)
            .field("balances", &self.balances)
            .finish()
    }
}

impl fmt::Display for Oracle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(bug) = &self.bug {
            write!(f, "Oracle\nbug: {}", bug)
        } else {
            write!(f, "Oracle\nbug: None")
        }
    }
}

impl Oracle {

    pub fn new(
        cage_env: Arc<RwLock<CageEnv>>,
        scanner: Arc<Scanner>) -> Self {
        Oracle {
            bug: None,
            cage_env,
            balances: HashMap::new(),
            pair_balances: HashMap::new(),
            balances_snapshot: None,
            pair_balances_snapshot: None,
            scanner,
            burn_amount: None,
        }
    }

    fn update_oracle<DB: DatabaseExt>(
        &mut self,
        data: &mut EVMData<'_, DB>,
        _caller: Address,
        call: &CallInputs,
    ) -> Result<Bytes, Bytes> {
        let caller = call.context.caller;
        // TODO: Error handling
        let decoded: HEVMCalls = HEVMCalls::decode(&call.input).map_err(|err| {
            panic!("Call to oracle cannot be decoded, {}", err);
            err.to_string().encode()
        })?;
        Self::apply(self, data, caller, &decoded).ok_or_else(|| "Invalid call to oracle.".to_string().encode())?
    }

    fn apply<DB: Database>(
        state: &mut Oracle,
        _data: &mut EVMData<'_, DB>,
        caller: Address,
        call: &HEVMCalls,
    ) -> Option<Result<Bytes, Bytes>> {
        Some(match call {
            HEVMCalls::GetRelevantTokenAddrs(_) 
                => Self::get_relevant_token_addrs(state),
            HEVMCalls::GetTargetAddrs(_)
                => Self::get_target_addrs(state),
            HEVMCalls::GetBaseTokenAddr(_)
                => Self::get_base_token_addr(state),
            HEVMCalls::GetPairAddr(_)
                => Self::get_pair_addr(state),
            HEVMCalls::GetRouterAddr(_)
                => Self::get_router_addr(state),
            HEVMCalls::UpdateTokenBalance(inner)
                => Self::update_token_balance(state, inner.0, inner.1, inner.2),
            HEVMCalls::AddRelevantTokenAddr(inner) 
                => Self::add_relevant_token_addr(state, inner.0),
            HEVMCalls::Debug(inner) 
                => Self::debug(&inner.0),
            HEVMCalls::Initialize(inner)
                => Self::initialize(state, inner.0, inner.1),
            HEVMCalls::NotifyOutOfCall(inner)
                => Self::notify_out_of_call(state, inner.0),
            HEVMCalls::ReplacePlaceholderValue(inner) 
                => Self::replace_placeholder_value(state, &inner.0),
            HEVMCalls::GetTargetTokenAddr(_)
                => Self::get_target_token_addr(state),
            HEVMCalls::SaveBalanceSnapshot(_)
                => Self::save_balance_snapshot(state),
            HEVMCalls::CheckInvariantBroken(_)
                => Self::check_invariant_broken(state),
            HEVMCalls::NotifyExploitSuccess(inner)
                => Self::notify_exploit_success(state, inner.0),
            HEVMCalls::HasNextCall(_)
                => Self::has_next_call(state),
            HEVMCalls::GetNextCall(_)
                => Self::get_next_call(state),
            HEVMCalls::NotifyInitialSwapFailed(_)
                => Self::notify_initial_swap_failed(state),
            HEVMCalls::RegisterFee(inner)
                => Self::register_fee(state, inner.0),
            HEVMCalls::GetFee(_)
                => Self::get_fee(state),
            HEVMCalls::GetInitialTokenPercent(_)
                => Self::get_initial_token_percent(state),
            HEVMCalls::RegisterBurnAmount(inner)
                => Self::register_burn_amount(state, inner.0),
            _ => { return None; },
        })
    }
    
    fn get_relevant_token_addrs(state: &mut Oracle) -> Result<Bytes, Bytes> {
        let relevant_token_addrs = &state.cage_env.read().unwrap().relevant_token_addrs;
        trace!("In get_relevant_token_addrs: {:?}", relevant_token_addrs);
        Ok(AbiEncode::encode(relevant_token_addrs.clone()).into())
    }

    fn get_base_token_addr(state: &Oracle) -> Result<Bytes, Bytes> {
        let base_addr = state.cage_env.read().unwrap().base_token.unwrap();
        Ok(AbiEncode::encode(base_addr).into())
    }

    fn get_pair_addr(state: &Oracle) -> Result<Bytes, Bytes> {
        let pair_addr = state.cage_env.read().unwrap().pair.unwrap();
        Ok(AbiEncode::encode(pair_addr).into())
    }

    fn get_router_addr(state: &Oracle) -> Result<Bytes, Bytes> {
        let router_addr = match state.scanner.network {
            foundry_utils::scan::Network::ETH => 
                Address::from_str("0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D").unwrap(),
            foundry_utils::scan::Network::BSC => 
                Address::from_str("0x10ED43C718714eb63d5aA57B78B54704E256024E").unwrap(),
        };
        Ok(AbiEncode::encode(router_addr).into())
    }

    fn get_target_addrs(state: &mut Oracle) -> Result<Bytes, Bytes> {
        let target_addrs: Vec<H160> = state.cage_env.read().unwrap().targets.keys().cloned().collect();
        trace!("In get_target_addrs: {:?}", target_addrs);
        Ok(AbiEncode::encode(target_addrs).into())
    }

    fn update_token_balance(state: &mut Oracle, token_holder_addr: Address, token_addr: Address, token_balance: Uint) -> Result<Bytes, Bytes> {
        trace!("In update_token_balance, token_holder_addr: {:x}, token: {:x}, balance {:x}", token_holder_addr, token_addr, token_balance,);
        let readable_cage_env = state.cage_env.read().unwrap();
        let main_pier_addr = readable_cage_env.system_addrs.get("main_pier").unwrap().clone();
        let pier_addr = readable_cage_env.pair.unwrap();
        let balances = if token_holder_addr == main_pier_addr {
                &mut state.balances
            } else if token_holder_addr == pier_addr {
                &mut state.pair_balances
            } else {
                panic!("called update_token_balances for unknown addr: {:x}", token_holder_addr);
            };
        
        balances.insert(token_addr, token_balance);

        Ok(Bytes::new())
    }

    fn add_relevant_token_addr(state: &mut Oracle, token_addr: Address) -> Result<Bytes, Bytes> {
        let readable_cage_env = state.cage_env.read().unwrap();
        if !readable_cage_env.relevant_token_addrs.contains(&token_addr) {
            drop(readable_cage_env);
            trace!("In add_relevant_token_addr, adding {:?} to relevant_token_addrs", &token_addr);
            let mut writable_cage_env: std::sync::RwLockWriteGuard<'_, CageEnv> = state.cage_env.write().unwrap();
            (*writable_cage_env).relevant_token_addrs.push(token_addr);
        }
        Ok(Bytes::new())   
    }

    fn debug(s: &str) -> Result<Bytes, Bytes> {
        println!("Oracle debug: {}", s);
        Ok(Bytes::new())
    }

    fn initialize(state: &mut Oracle, bridge_addr: Address,  main_pier_addr: Address) -> Result<Bytes, Bytes> {
        let mut writable_cage_env = state.cage_env.write().unwrap();
        writable_cage_env.system_addrs.insert("oracle".to_string(), ORACLE_ADDRESS);
        writable_cage_env.system_addrs.insert("init".to_string(), INITIAL_ADDRESS);
        writable_cage_env.system_addrs.insert("bridge".to_string(), bridge_addr);
        writable_cage_env.system_addrs.insert("main_pier".to_string(), main_pier_addr);
        println!("{:?}", writable_cage_env.system_addrs);
        Ok(Bytes::new())
    }

    fn notify_out_of_call(state: &mut Oracle, when: u8) -> Result<Bytes, Bytes> {
        println!("when: {}", when);
        Ok(Bytes::new())
    }

    fn replace_placeholder_value(state: &Oracle, calldata: &EtherBytes) -> Result<Bytes, Bytes> {
        // println!("calldata: {:x}", calldata);
        let mut call_signature: [u8; 4] = Default::default();
        call_signature.copy_from_slice(&calldata[..4]);

        match call_signature {
            SWAP_EXACT_TOKENS_FOR_ETH | SWAP_EXACT_TOKENS_FOR_TOKENS_SUPPORTING_FEE_ON_TRANSFER_TOKENS 
                | SWAP_EXACT_TOKENS_FOR_ETH_SUPPORTING_FEE_ON_TRANSFER_TOKENS => {
                let token_addr = Address::from_slice(&calldata[208..228]);
                let curr_balance = state.balances.get(&token_addr).unwrap_or_else(|| {
                            panic!("cannot get curr_balance for token_addr {:x}", token_addr);
                        });       
                let placeholder_amount = &calldata[4..36];
                // println!("Replacing amount {:?} to {:x}", placeholder_amount, curr_balance);

                // Build new calldata, cannot modify ehter::types::Bytes
                let mut new_calldata: Vec<u8> = Vec::new();
                for i in 0..4 {
                    new_calldata.push(calldata[i]);
                }
                for i in 0..32 {
                    new_calldata.push(curr_balance.byte(31-i));
                }
                for i in 36..calldata.len() {
                    new_calldata.push(calldata[i]);
                }
                let new_calldata_bytes = Bytes::from(new_calldata);
                return Ok(AbiEncode::encode(new_calldata_bytes).into());
            },
            TRANSFER => {
                let readable_cage_env = state.cage_env.read().unwrap();
                let token_addr = readable_cage_env.target_token.unwrap();

                let mut placeholder_value_array: [u8; 32] = Default::default();
                placeholder_value_array.copy_from_slice(&calldata[36..68]);
                let mut transaction_amount = U256::from(placeholder_value_array);

                if transaction_amount == PAIR_BALANCE {
                    transaction_amount = *state.pair_balances.get(&token_addr).unwrap();
                } else if transaction_amount == THIS_BALANCE {
                    transaction_amount = *state.balances.get(&token_addr).unwrap();
                    if let Some(fee_percent) = readable_cage_env.fee_on_transfer { // calldata[36..68] == THIS_BALANCE
                        let fee_on_transfer = transaction_amount.checked_mul(fee_percent).unwrap().checked_div(100u64.into()).unwrap();
                        transaction_amount = transaction_amount.checked_sub(fee_on_transfer).unwrap();
                    }
                } // use whatever value was in placeholder amount

                // let placeholder_amount = &calldata[36..68];
                // println!("Replacing amount {:?} to {:x}", placeholder_amount, curr_balance);

                let mut new_calldata: Vec<u8> = Vec::new();
                for i in 0..36 {
                    new_calldata.push(calldata[i]);
                }
                for i in 0..32 {
                    new_calldata.push(transaction_amount.byte(31-i));
                }
                let new_calldata_bytes = Bytes::from(new_calldata);
                return Ok(AbiEncode::encode(new_calldata_bytes).into());
            },
            BURN_UINT => {
                let mut placeholder_value_array: [u8; 32] = Default::default();
                placeholder_value_array.copy_from_slice(&calldata[4..36]);
                let placeholder_value = U256::from(placeholder_value_array);

                let readable_cage_env = state.cage_env.read().unwrap();
                let token_addr = readable_cage_env.target_token.unwrap();

                let mut transaction_amount = U256::zero();

                if placeholder_value == PAIR_BALANCE_MINUS_ONE {
                    transaction_amount = state.pair_balances.get(&token_addr).unwrap().checked_sub(1u64.into()).unwrap();
                } else if placeholder_value == BURN_AMOUNT {
                    transaction_amount = state.burn_amount.unwrap();
                } else {
                    panic!("UNKNOWN BURN AMOUNT in BURN_UINT");
                }

                // let placeholder_amount = &calldata[4..36];
                // println!("Replacing amount {:?} to {:x}", placeholder_amount, transaction_amount);

                let mut new_calldata: Vec<u8> = Vec::new();
                for i in 0..4 {
                    new_calldata.push(calldata[i]);
                }
                for i in 0..32 {
                    new_calldata.push(transaction_amount.byte(31-i));
                }
                let new_calldata_bytes = Bytes::from(new_calldata);
                return Ok(AbiEncode::encode(new_calldata_bytes).into());
            },
            BURN_ADDRESS_UINT => {
                let mut placeholder_value_array: [u8; 32] = Default::default();
                placeholder_value_array.copy_from_slice(&calldata[36..68]);
                let placeholder_value = U256::from(placeholder_value_array);

                let readable_cage_env = state.cage_env.read().unwrap();
                let token_addr = readable_cage_env.target_token.unwrap();

                let mut transaction_amount = U256::zero();

                if placeholder_value == PAIR_BALANCE_MINUS_ONE {
                    transaction_amount = state.pair_balances.get(&token_addr).unwrap().checked_sub(1u64.into()).unwrap();
                } else if placeholder_value == BURN_AMOUNT {
                    transaction_amount = state.burn_amount.unwrap();
                } else {
                    panic!("UNKNOWN BURN AMOUNT in BURN_ADDRESS_UINT");
                }
                // let placeholder_amount = &calldata[36..68];
                // println!("Replacing amount {:?} to {:x}", placeholder_amount, transaction_amount);

                let mut new_calldata: Vec<u8> = Vec::new();
                for i in 0..36 {
                    new_calldata.push(calldata[i]);
                }
                for i in 0..32 {
                    new_calldata.push(transaction_amount.byte(31-i));
                }
                let new_calldata_bytes = Bytes::from(new_calldata);
                return Ok(AbiEncode::encode(new_calldata_bytes).into());
            },
            _ => {
                // do nothing
            }
        }

        Ok(AbiEncode::encode(Bytes::new()).into())
    }

    fn get_target_token_addr(state: &Oracle) -> Result<Bytes, Bytes> {
        let readable_cage_env = state.cage_env.read().unwrap();
        let target_token_addr = readable_cage_env.target_token.unwrap();
        Ok(AbiEncode::encode(target_token_addr).into())
    }

    fn save_balance_snapshot(state: &mut Oracle) -> Result<Bytes, Bytes> {
        state.balances_snapshot = Some(state.balances.clone());
        state.pair_balances_snapshot = Some(state.pair_balances.clone());
        Ok(AbiEncode::encode(Bytes::new()).into())
    }

    fn check_invariant_broken(state: &mut Oracle) -> Result<Bytes, Bytes> {
        let target_token_addr = state.cage_env.read().unwrap().target_token.unwrap();

        let previous_attacker_balances = state.balances_snapshot.as_ref().unwrap();
        let previous_pair_balances = state.pair_balances_snapshot.as_ref().unwrap();

        let previous_pair_balance = previous_pair_balances.get(&target_token_addr).unwrap();
        let current_pair_balance = state.pair_balances.get(&target_token_addr).unwrap();
        if previous_pair_balance > current_pair_balance {
            if state.bug.is_none() {
                state.bug = Some(Bug::PairTokenLoss);
            }
        }

        let previous_target_token_reserve = previous_pair_balances.get(&Address::from_str("0x0000000000000000000000000000000000000001").unwrap()).unwrap();
        let current_target_token_reserve = state.pair_balances.get(&Address::from_str("0x0000000000000000000000000000000000000001").unwrap()).unwrap();

        let previous_balance = previous_attacker_balances.get(&target_token_addr).unwrap();
        let current_balance = state.balances.get(&target_token_addr).unwrap();

        let previous_attacker_asset = previous_balance.checked_add(*previous_pair_balance).unwrap_or_else(|| {
            U256::zero()
        }).checked_sub(*previous_target_token_reserve).unwrap_or_else(|| {
            U256::zero()
        });
        let current_attacker_asset = current_balance.checked_add(*current_pair_balance).unwrap_or_else(|| {
            U256::zero()
        }).checked_sub(*current_target_token_reserve).unwrap_or_else(|| {
            U256::zero()
        });
        if state.bug.is_none() && (current_attacker_asset > previous_attacker_asset) {
            state.bug = Some(Bug::AttackerTokenGain);
        }
        
        Ok(AbiEncode::encode(Bytes::new()).into())
    }

    fn notify_exploit_success(state: &mut Oracle, profit: U256) -> Result<Bytes, Bytes> {
        state.bug = Some(Bug::ProfitGenerated(profit));
        Ok(AbiEncode::encode(Bytes::new()).into())
    }

    fn notify_initial_swap_failed(state: &mut Oracle) -> Result<Bytes, Bytes> {
        state.bug = Some(Bug::InitialSwapFailed);
        Err(AbiEncode::encode(Bytes::new()).into())
    }

    fn register_fee(state: &mut Oracle, fee: U256) -> Result<Bytes, Bytes> {
        let mut writable_cage_env = state.cage_env.write().unwrap();
        writable_cage_env.fee_on_transfer = Some(fee);
        Ok(AbiEncode::encode(Bytes::new()).into())
    }

    fn get_fee(state: &Oracle) -> Result<Bytes, Bytes> {
        let readable_cage_env = state.cage_env.read().unwrap();
        if let Some(fee) = readable_cage_env.fee_on_transfer {
            Ok(AbiEncode::encode(fee).into())
        } else {
            println!("In get_fee, but no fee");
            Ok(AbiEncode::encode(U256::zero()).into())
        }
    }

    fn get_initial_token_percent(state: &Oracle) -> Result<Bytes, Bytes> {
        let readable_cage_env = state.cage_env.read().unwrap();
        let percent = readable_cage_env.initial_token_percent.unwrap();
        Ok(AbiEncode::encode(percent).into())
    }

    fn register_burn_amount(state: &mut Oracle, burn_amount: U256) -> Result<Bytes, Bytes> {
        state.burn_amount = Some(burn_amount);
        Ok(AbiEncode::encode(Bytes::new()).into())
    }


    fn has_next_call(state: &Oracle) -> Result<Bytes, Bytes> {
        Ok(true.encode().into())
    }

    fn get_next_call(state: &Oracle) -> Result<Bytes, Bytes> {

        panic!("hello");

        Ok(AbiEncode::encode(Bytes::new()).into())
    }


}


impl<DB> Inspector<DB> for Oracle
where
    DB: DatabaseExt,
{

    fn initialize_interp(&mut self,_interp: &mut Interpreter,_data: &mut EVMData<'_,DB> ,_is_static:bool,) -> Return {
        Return::Continue
    }

    fn step(&mut self, interpreter: &mut Interpreter, _data: &mut EVMData<'_,DB>, _is_static:bool) -> Return { 

        match interpreter.contract.bytecode.bytecode()[interpreter.program_counter()] {
            opcode::REVERT => {
                trace!(target: "cage", "RequirementViolation is detected");
                if self.bug.is_some() {
                    if *self.bug.as_ref().unwrap() == Bug::InitialSwapFailed {
                        return Return::Continue
                    }
                    let readable_cage_env = self.cage_env.read().unwrap(); 
                    if readable_cage_env.deep_search_phase {
                        self.bug = Some(Bug::RequirementViolation);
                    }
                    drop(readable_cage_env);
                } else {
                    self.bug = Some(Bug::RequirementViolation);
                }
            },
            _ => {
            }
        }
        Return::Continue
    }

    fn call(
        &mut self,
        data: &mut EVMData<'_, DB>,
        call: &mut CallInputs,
        _is_static: bool,
    ) -> (Return, Gas, Bytes) {

        let caller = call.context.caller;
        let callee = call.contract;

        // println!("in call, call from {:x} to {:x}", caller, callee);

        // Handle calls to oracle address
        if callee == ORACLE_ADDRESS {
            match self.update_oracle(data, call.context.caller, call) {
                // Ok(retdata) => return (Return::Return, Gas::new(call.gas_limit), retdata),
                Ok(retdata) => return (Return::Continue, Gas::new(call.gas_limit), retdata),
                // Early exit before reaching symbolic executor
                Err(err) => {
                    // DEBUG
                    println!("Err in update_oracle");
                    return (Return::Revert, Gas::new(call.gas_limit), err)
                }
            };
        }

        (Return::Continue, Gas::new(0), Bytes::new())
    }

}