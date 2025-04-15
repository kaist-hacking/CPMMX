use ethers::{abi::{Abi, Address}, utils::to_checksum};
use ethers_providers::{Http, Middleware, Provider};
use std::{fs::File, io::{Cursor, Write}, path::Path, str::FromStr, sync::{Arc, Mutex}};
use serde_json::{Value};
use curl::easy::Easy;
use tracing::trace;
use eyre::{Result};
use ethabi::{ethereum_types::H256, Contract};

// // EtherScan
const ETHERSCAN_URL: &str = "https://api.etherscan.io/api";

// // BSCScan
const BSCSCAN_URL: &str = "https://api.bscscan.com/api";

// ERC20 Token Standard
const FUNC_NAME: &str = r#"{"constant":true,"inputs":[],"name":"name","outputs":[{"name":"","type":"string"}],"payable":false,"stateMutability":"view","type":"function"}"#;

const FUNC_APPROVE: &str = r#"{"constant":false,"inputs":[{"name":"_spender","type":"address"},{"name":"_value","type":"uint256"}],"name":"approve","outputs":[{"name":"","type":"bool"}],"payable":false,"stateMutability":"nonpayable","type":"function"}"#;

const FUNC_TOTALSUPPLY: &str = r#"{"constant":true,"inputs":[],"name":"totalSupply","outputs":[{"name":"","type":"uint256"}],"payable":false,"stateMutability":"view","type":"function"}"#;

const FUNC_TRANSFERFROM: &str = r#"{"constant":false,"inputs":[{"name":"_from","type":"address"},{"name":"_to","type":"address"},{"name":"_value","type":"uint256"}],"name":"transferFrom","outputs":[{"name":"","type":"bool"}],"payable":false,"stateMutability":"nonpayable","type":"function"}"#;

const FUNC_DECIMALS: &str = r#"{"constant":true,"inputs":[],"name":"decimals","outputs":[{"name":"","type":"uint8"}],"payable":false,"stateMutability":"view","type":"function"}"#;

const FUNC_BALANCEOF: &str = r#"{"constant":true,"inputs":[{"name":"_owner","type":"address"}],"name":"balanceOf","outputs":[{"name":"","type":"uint256"}],"payable":false,"stateMutability":"view","type":"function"}"#;

const FUNC_SYMBOL: &str = r#"{"constant":true,"inputs":[],"name":"symbol","outputs":[{"name":"","type":"string"}],"payable":false,"stateMutability":"view","type":"function"}"#;

const FUNC_TRANSFER: &str = r#"{"constant":false,"inputs":[{"name":"_to","type":"address"},{"name":"_value","type":"uint256"}],"name":"transfer","outputs":[{"name":"","type":"bool"}],"payable":false,"stateMutability":"nonpayable","type":"function"}"#;

const FUNC_ALLOWANCE: &str = r#"{"constant":true,"inputs":[{"name":"_owner","type":"address"},{"name":"_spender","type":"address"}],"name":"allowance","outputs":[{"name":"","type":"uint256"}],"payable":false,"stateMutability":"view","type":"function"}"#;

const EVENT_APPROVAL: &str = r#"{"anonymous":false,"inputs":[{"indexed":true,"name":"owner","type":"address"},{"indexed":true,"name":"spender","type":"address"},{"indexed":false,"name":"value","type":"uint256"}],"name":"Approval","type":"event"}"#;

const EVENT_TRANSFER: &str = r#"{"anonymous":false,"inputs":[{"indexed":true,"name":"from","type":"address"},{"indexed":true,"name":"to","type":"address"},{"indexed":false,"name":"value","type":"uint256"}],"name":"Transfer","type":"event"}"#;

const ERC20_STANDARD_FUNCTIONS: [&str; 9] = [FUNC_NAME, FUNC_SYMBOL, FUNC_DECIMALS, 
    FUNC_TOTALSUPPLY, FUNC_BALANCEOF, FUNC_TRANSFER, 
    FUNC_TRANSFERFROM, FUNC_APPROVE, FUNC_ALLOWANCE];

const ERC20_STANDARD_EVENTS: [&str; 2] = [EVENT_TRANSFER, EVENT_APPROVAL];

// BEP20 Token Standard
const FUNC_OWNEROF: &str = r#"
    {
        "constant": true,
        "inputs": [],
        "name": "getOwner",
        "outputs": [
            {
                "internalType": "address",
                "name": "",
                "type": "address"
            }
        ],
        "payable": false,
        "stateMutability": "view",
        "type": "function"
    }"#;

#[derive(Debug, PartialEq, Clone)]
pub enum Network {
    ETH,
    BSC,
}

#[derive(Debug, Clone)]
pub struct Scanner {
    pub network: Network,
    pub fork_url: String,
    api_key: String,
    api_url: String
}

impl Scanner {

    // pub fn new(fork_url: String, fork_block_number: u64) -> Self {
    pub fn new(fork_url: String, etherscan_api_key: String) -> Self {

        // TODO: support more network url?
        let scan = match &fork_url[..] {
            "https://rpc.ankr.com/eth"
            | "https://eth.public-rpc.com" => {
                Network::ETH
            },
            "https://rpc.ankr.com/bsc" 
            | "https://bscrpc.com" => {
                Network::BSC
            },
            _ => {
                // Network::ETH
                panic!("UNKNOWN fork_url")
            }
        };

        

        match scan {
            Network::ETH => {
                Self {
                    network: scan,
                    fork_url,
                    api_key: etherscan_api_key,
                    api_url: ETHERSCAN_URL.to_string()
                }
            },
            Network::BSC => {
                Self {
                    network: scan,
                    fork_url,
                    api_key: etherscan_api_key,
                    api_url: BSCSCAN_URL.to_string(),
                }
            }
        }
    }

    pub fn get_contract_abi(&self, addr: &Address) -> Result<Contract> {
        let abi_str = self.get_abi_string(addr)?;
        let reader = Cursor::new(abi_str);
        let formatted_abi = Abi::load(reader).expect("Failed to get formatted_abi");

        Ok(formatted_abi)
    }
    
    // Get ABI from Etherscan and check if ERC20
    pub fn is_erc20_or_bep20_addr(&self, addr: &Address) -> bool {
        match self.get_abi_string(addr) {
            Ok(abi_str) => {
                let abi_array = serde_json::from_str::<Vec<Value>>(&abi_str).unwrap();
                self.is_erc20_or_bep20_abi(&abi_array)
            },
            Err(_) => {
                false  
            }
        }
    }

    pub fn get_default_erc20_abi(&self) -> Contract {
        let cache_dir = self.cache_dir().unwrap();
        let response_file_str = format!("./erc20");

        let response_json = {
            let response_str = std::fs::read_to_string(response_file_str).unwrap();
            serde_json::from_str::<Value>(&response_str).unwrap()
        };

        let abi_str = response_json["result"].as_str().unwrap().to_string();

        let reader = Cursor::new(abi_str);
        let formatted_abi = Abi::load(reader).expect("Failed to get formatted_abi");

        formatted_abi
    }

    pub fn get_default_pair_abi(&self) -> Contract {
        let cache_dir = self.cache_dir().unwrap();
        let response_file_str = format!("./uniswap_v2_pair");

        let response_json = {
            let response_str = std::fs::read_to_string(response_file_str).unwrap();
            serde_json::from_str::<Value>(&response_str).unwrap()
        };

        let abi_str = response_json["result"].as_str().unwrap().to_string();

        let reader = Cursor::new(abi_str);
        let formatted_abi = Abi::load(reader).expect("Failed to get formatted_abi");

        formatted_abi
    }

    // Returns cache_dir path as Result<String>
    // Creates cache_dir if it does not exist
    fn cache_dir(&self) -> Result<String> {
        let home_dir = dirs::home_dir().ok_or(eyre::eyre!("Failed to grab home directory"))?;
        let home_dir_str =
            home_dir.to_str().ok_or(eyre::eyre!("Failed to convert home directory to string"))?;
        let network = match self.network {
            Network::ETH => "eth",
            Network::BSC => "bsc",
        };
        let cache_dir_str = format!("{home_dir_str}/.foundry/cache/scan/{network}");
        if !Path::new(&cache_dir_str).exists() {
            std::fs::create_dir_all(&cache_dir_str)?;
        }
        Ok(cache_dir_str)
    }
    
    fn is_erc20_or_bep20_abi(&self, abi_array: &Vec<Value>) -> bool {
        for erc20_standard_function in ERC20_STANDARD_FUNCTIONS {
            let erc20_standard_function_exists = abi_array.iter().any(|x| {
                match x["type"].as_str() {
                    Some("function") => Self::compare_function_abi(x, 
                        &serde_json::from_str::<Value>(erc20_standard_function).unwrap()),
                    _ => false,
                }
            });
            if !erc20_standard_function_exists {
                trace!("Below ERC20 function does not exist:\n{}", erc20_standard_function);
                return false;
            }
        }
        for erc_20_standard_event in ERC20_STANDARD_EVENTS {
            let erc20_standard_event_exists = abi_array.iter().any(|x| {
                match x["type"].as_str() {
                    Some("event") => Self::compare_event_abi(x, 
                        &serde_json::from_str::<Value>(erc_20_standard_event).unwrap()),
                    _ => false,
                }
            });
            if !erc20_standard_event_exists {
                trace!("Below ERC20 event does not exist:\n{}", erc_20_standard_event);
                return false;
            }  
        }
        // BEP20 standard has one more function
        if self.network == Network::BSC {
            let bep20_standard_function_exists = abi_array.iter().any(|x| {
                match x["type"].as_str() {
                    Some("function") => Self::compare_function_abi(x, 
                        &serde_json::from_str::<Value>(FUNC_OWNEROF).unwrap()),
                    _ => false,
                }
            });
            if !bep20_standard_function_exists {
                trace!("Below BEP20 function does not exist:\n{}", FUNC_OWNEROF);
                return false;
            }
        }
    
        return true;
    }

    fn get_local_abi(path: &str) -> Result<String> {
        let json_str = std::fs::read_to_string(path).unwrap();
        let json = serde_json::from_str::<Value>(&json_str).unwrap();
        return Ok(json["abi"].to_string());
    }

    fn get_abi_string(&self, addr: &Address) -> Result<String> {

        let abi_file_path = self.get_abi_file(addr)?;

        let response_json = {
            let response_str = std::fs::read_to_string(abi_file_path.clone()).unwrap();
            serde_json::from_str::<Value>(&response_str).unwrap()
        };

        let status = response_json["status"].as_str().unwrap();

        if status == "1" {
            let abi_str = response_json["result"].as_str().unwrap().to_string();
            return Ok(abi_str);
        }
        
        if response_json["result"] == "Contract source code not verified" {
            let extracted_abi_file_path = match self.network {
                Network::ETH => abi_file_path.replace("eth", "eth_noabi_extracted"),
                Network::BSC => abi_file_path.replace("bsc", "bsc_noabi_extracted"),
            };
            let extracted_response_json = match std::fs::read_to_string(extracted_abi_file_path) {
                Ok(response_str) => {
                    serde_json::from_str::<Value>(&response_str).unwrap()
                },
                Err(_) => {
                    return Err(eyre::eyre!("Cannot get abi for target contract at address: {:x}", addr))
                }
            };
            if extracted_response_json["status"] == "1" {
                let abi_str = extracted_response_json["result"].as_str().unwrap().to_string();
                return Ok(abi_str);
            }
        } 

        Err(eyre::eyre!("Cannot get abi for target contract at address: {:x}", addr))
    }

    fn get_abi_file(&self, addr: &Address) -> Result<String> {
        // second argument is chain_id to encode using EIP-1191 extension
    
        let url = &self.api_url;
        let key = &self.api_key;
    
        let checksum_addr = to_checksum(addr, None);
    
        let cache_dir = self.cache_dir()?;
        let response_file_str = format!("{cache_dir}/{:x}", addr);
        let response_file_str_for_move = response_file_str.clone();

        if !Path::new(&response_file_str_for_move).exists() {
            let buf = Arc::new(Mutex::new(Vec::new()));
            let write_buf = buf.clone();
            trace!("request abi for {}", checksum_addr);
            let http_request = format!("{url}?module=contract&action=getabi&address={checksum_addr}&apikey={key}");
            let mut easy = Easy::new();
            easy.url(&http_request).unwrap();
            easy.write_function(move |data| {
                let mut buf = write_buf.lock().unwrap();
                buf.extend_from_slice(data);
                Ok(data.len())
            }).unwrap();
            match easy.perform() {
                Ok(_) => (),
                Err(err) => {
                    return Err(err.into());
                }
            }

            let read_buf = buf.lock().unwrap();

            let data_json = match serde_json::from_slice::<Value>(&read_buf) {
                Ok(data_json) => data_json,
                Err(_) => {
                    println!("failed to parse data, data:\n{}", String::from_utf8_lossy(&read_buf));
                    return Err(eyre::eyre!("failed to parse data"));
                }
            };
            let result = data_json["result"].as_str().unwrap();
            if result == "Max rate limit reached" {
                // do not store cache if rate limit is reached
                return Err(eyre::eyre!("MAX RATE LIMIT REACEHD for SCAN"));
            }
            let mut file = File::create(&response_file_str_for_move).unwrap();
            file.write_all(&read_buf).unwrap();

        }

        Ok(response_file_str)
    }
    
    fn compare_function_abi(function_a: &Value, function_b: &Value) -> bool {
        for property in ["name", "stateMutability"] {
            // some abi don't include "payable", "constant"
            if function_a[property] != function_b[property] {
                return false;
            }
        }
        for arg_type in ["inputs", "outputs"] {
            let args_array_a = function_a[arg_type].as_array().unwrap();
            let args_array_b = function_b[arg_type].as_array().unwrap();
            if args_array_a.len() != args_array_b.len() {
                return false;
            }
            for i in 0..args_array_a.len() {
                if args_array_a[i]["type"] != args_array_b[i]["type"] {
                    return false;
                }
            }
        }
        true
    }
    
    fn compare_event_abi(event_a: &Value, event_b: &Value) -> bool {
        for property in ["anonymous", "name"] {
            if event_a[property] != event_b[property] {
                return false;
            }
        }
        for arg_type in ["inputs"] {
            let args_array_a = event_a[arg_type].as_array().unwrap();
            let args_array_b = event_b[arg_type].as_array().unwrap();
            if args_array_a.len() != args_array_b.len() {
                return false;
            }
            for i in 0..args_array_a.len() {
                if (args_array_a[i]["type"] != args_array_b[i]["type"])
                    || (args_array_a[i]["indexed"] != args_array_b[i]["indexed"]) {
                    return false;
                }
            }
        }
        true
    }
    
    // Checks proxy based on whether or not the contract has implementation address
    // Based on ERC-1967
    pub fn is_proxy_addr(&self, addr: &Address) -> Result<Address> {

        let provider = Provider::<Http>::try_from(&self.fork_url)?;

        // Get implementation address (position = keccak-256 hash of "eip1967.proxy.implementation" subtracted by 1)
        let key = H256::from_str("0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc").unwrap();
    
        let rt = tokio::runtime::Runtime::new()?;

        let result = rt.block_on(async {
            provider.get_storage_at(*addr, key, None).await
        })?;

        let address_bytes: [u8; 20] = result[12..].try_into().expect("Incorrect result length while getting proxy address");

        let impl_addr = Address::from(address_bytes);

        if impl_addr == Address::zero() {
            return Err(eyre::eyre!("No implementation address"));
        }

        Ok(impl_addr)
    }

    // For debugging
    fn bytes_to_string(input: &Vec<u8>) -> String {
        let str_vec: Vec<String> = input.iter().map(|b| format!("{:02x}", b)).collect();
        str_vec.connect("")
    }
}