use super::{*, testcase::EVMCall};
use ethers::{abi::{Address, Abi, Token}, types::U256};

#[derive(Debug, Clone, PartialEq)]
pub enum SwapType {
    TokenEth,
    TokenToken
}

#[derive(Debug, Clone, PartialEq)]
pub struct SwapTemplate {
    pub swap_addr: Address,
    /* order follow swap seqeuences from target token to eth
      (target_token, token1), (token1, token2), (token2, eth) */
    pub token_a: Address,
    pub token_b: Address,
    pub swap_type: SwapType
}


impl SwapTemplate {

    pub fn new_token_eth_swap(swap_addr: Address, token_a: Address, wrapped_native_token_addr: Address) -> Self {
        assert!(wrapped_native_token_addr == corpus::WBNB || wrapped_native_token_addr == corpus::WETH);
        return Self {
            swap_addr,
            token_a,
            token_b: wrapped_native_token_addr,
            swap_type: SwapType::TokenEth
        }
    }

    pub fn new_token_token_swap(swap_addr: Address, token_a: Address, token_b: Address) -> Self {
        return Self {
            swap_addr,
            token_a,
            token_b,
            swap_type: SwapType::TokenToken
        }
    }

    pub fn generate_eth_to_token_swap_call(&self, contract: &Abi, amount: U256, receiver_addr: Address) -> EVMCall {

        let swap_func = contract.function("swapExactETHForTokensSupportingFeeOnTransferTokens").unwrap();
        let swap_tokens = vec![
            Token::Uint(1u64.into()), // minimum out token amount
            Token::Array(vec![Token::Address(self.token_b), Token::Address(self.token_a)]), // path to swap
            Token::Address(receiver_addr), // addr to receive tokens
            Token::Uint(U256::MAX) // deadline for swap
        ];
        let swap_calldata = swap_func.encode_input(&swap_tokens).unwrap_or_else(|_| {
            panic!("tokens: {:?}", swap_tokens);
        });

        let swap_call = EVMCall {
            to: self.swap_addr,
            value: amount,
            name: swap_func.name.clone(),
            calldata: swap_calldata,
            args: swap_tokens,
        };

        swap_call
    }

    pub fn generate_token_to_eth_swap_call(&self, contract: &Abi, amount: U256, receiver_addr: Address) -> EVMCall {
        let swap_func = contract.function("swapExactTokensForETHSupportingFeeOnTransferTokens").unwrap();
        let swap_tokens = vec![
            Token::Uint(amount), // exact in token amount
            Token::Uint(1u64.into()), // minimum out token amount
            Token::Array(vec![Token::Address(self.token_a), Token::Address(self.token_b)]), // path to swap
            Token::Address(receiver_addr), // addr to receive tokens
            Token::Uint(U256::MAX) // deadline for swap
        ];
        let swap_calldata = swap_func.encode_input(&swap_tokens).unwrap_or_else(|_| {
            panic!("tokens: {:?}", swap_tokens);
        });

        let swap_call = EVMCall {
            to: self.swap_addr,
            value: 0u64.into(),
            name: swap_func.name.clone(),
            calldata: swap_calldata,
            args: swap_tokens
        };

        swap_call
    }

    pub fn generate_token_to_token_swap_call(&self, contract: &Abi, amount: U256, receiver_addr: Address) -> EVMCall {
        let swap_func = contract.function("swapExactTokensForTokensSupportingFeeOnTransferTokens").unwrap();
        let swap_tokens = vec![
            Token::Uint(amount), // exact in token amount
            Token::Uint(1u64.into()), // minimum out token amount
            Token::Array(vec![Token::Address(self.token_a), Token::Address(self.token_b)]), // path
            Token::Address(receiver_addr), // addr to receive tokens
            Token::Uint(U256::MAX) // deadline for swap
        ];
        let swap_calldata = swap_func.encode_input(&swap_tokens).unwrap_or_else(|_| {
            panic!("tokens: {:?}", swap_tokens);
        });

        let swap_call = EVMCall {
            to: self.swap_addr,
            value: 0u64.into(),
            name: swap_func.name.clone(),
            calldata: swap_calldata,
            args: swap_tokens
        };

        swap_call
    }

    pub fn generate_reverse_token_to_token_swap_call(&self, contract: &Abi, amount: U256, receiver_addr: Address) -> EVMCall {
        let swap_func = contract.function("swapExactTokensForTokensSupportingFeeOnTransferTokens").unwrap();
        let swap_tokens = vec![
            Token::Uint(amount), // exact in token amount
            Token::Uint(1u64.into()), // minimum out token amount
            Token::Array(vec![Token::Address(self.token_b), Token::Address(self.token_a)]), // path
            Token::Address(receiver_addr), // addr to receive tokens
            Token::Uint(U256::MAX) // deadline for swap
        ];
        let swap_calldata = swap_func.encode_input(&swap_tokens).unwrap_or_else(|_| {
            panic!("tokens: {:?}", swap_tokens);
        });

        let swap_call = EVMCall {
            to: self.swap_addr,
            value: 0u64.into(),
            name: swap_func.name.clone(),
            calldata: swap_calldata,
            args: swap_tokens
        };

        swap_call
    }

}
