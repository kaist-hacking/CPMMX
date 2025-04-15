use super::{*, fuzz::Cage};
use ethers::{
    abi::{Token, Tokenizable, InvalidOutputType},
    types::{H160, U256},
};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub calls: Vec<EVMCall>,
    pub subcalls: Vec<EVMCall>,
    pub callbacks: Vec<EVMCall>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EVMCall {
    pub to: H160,    
    pub value: U256,
    pub name: String,
    pub calldata: Vec<u8>,
    pub args: Vec<Token>,
}


impl Tokenizable for EVMCall {
    fn into_token(self) -> Token {
        Token::Tuple(vec![
            Token::Address(self.to),
            Token::Bytes(self.calldata),
            Token::Uint(self.value)
        ])
    }

    fn from_token(token: Token) -> Result<Self, InvalidOutputType>
        where
            Self: Sized {
        match token {
            Token::Tuple(mut tokens) => {
                let to = tokens.remove(0);
                let calldata = tokens.remove(0);
                let value = tokens.remove(0);

                let to = match to {
                    Token::Address(to) => to,
                    _ => {
                        return Err(InvalidOutputType(format!(
                            "Expected `address`, got {:?}", to
                        )).into());
                    }
                };

                let calldata = match calldata {
                    Token::Bytes(calldata) => calldata,
                    _ => {
                        return Err(InvalidOutputType(format!(
                            "Expected `bytes`, got {:?}", calldata
                        )).into());
                    }
                };

                let value = match value {
                    Token::Uint(value) => value,
                    _ => {
                        return Err(InvalidOutputType(format!(
                            "Expected `uint`, got {:?}", value
                        )).into());
                    }
                };

                Ok(Self {
                    to,
                    value,
                    name: "".to_string(), 
                    calldata,
                    args: vec![],
                })
            },
            _ => Err(InvalidOutputType(format!(
                "Expected `tuple`, got {:?}", token
            )).into())
        }
    }
}

impl Tokenizable for TestCase {
    fn into_token(self) -> Token {
        Token::Tuple(vec![
            Token::Array(self.calls.into_iter().map(|c| c.into_token()).collect()),
            Token::Array(self.subcalls.into_iter().map(|c| c.into_token()).collect()),
            Token::Array(self.callbacks.into_iter().map(|c| c.into_token()).collect())
        ])
    }

    fn from_token(token: Token) -> Result<Self, InvalidOutputType>
        where
            Self: Sized {
        match token {
            Token::Tuple(mut tokens) => {
                if tokens.len() != 3 {
                    return Err(InvalidOutputType("Invalid test case".to_string()));
                }

                let calls = tokens.remove(0);
                let subcalls = tokens.remove(0);
                let callbacks = tokens.remove(0);

                let calls = match calls {
                    Token::Array(calls) => calls.into_iter().map(|c| EVMCall::from_token(c)).collect::<Result<Vec<EVMCall>, InvalidOutputType>>()?,
                    _ => {
                        return Err(InvalidOutputType(format!(
                            "Expected `array`, got {:?}", calls
                        )).into());
                    }
                };

                let subcalls = match subcalls {
                    Token::Array(subcalls) => subcalls.into_iter().map(|c| EVMCall::from_token(c)).collect::<Result<Vec<EVMCall>, InvalidOutputType>>()?,
                    _ => {
                        return Err(InvalidOutputType(format!(
                            "Expected `array`, got {:?}", subcalls
                        )).into());
                    }
                };

                let callbacks = match callbacks {
                    Token::Array(callbacks) => callbacks.into_iter().map(|c| EVMCall::from_token(c)).collect::<Result<Vec<EVMCall>, InvalidOutputType>>()?,
                    _ => {
                        return Err(InvalidOutputType(format!(
                            "Expected `array`, got {:?}", callbacks
                        )).into());
                    }
                };

                Ok(Self {
                    calls,
                    subcalls,
                    callbacks,
                })
            },
            _ => Err(InvalidOutputType(format!(
                "Expected `tuple`, got {:?}", token
            )).into())
        } 
    }

}

impl TestCase {

    pub fn new() -> Self {
        Self {
            calls: Vec::new(),
            subcalls: Vec::new(),
            callbacks: Vec::new(),
        }
    }
    
    pub fn to_string_pretty(&self, cage: &Cage) -> Result<String> {
        let mut s = String::new();

        let readable_cage_env = cage.env.read().unwrap();
        drop(readable_cage_env);

        for call in &self.calls {
            match call.to_string_pretty(cage) {
                Ok(str) => {
                    s.push_str(&format!("call:\n {}\n", str));
                },
                Err(e) => {
                    panic!("cannot decode call {:?}", call);
                    s.push_str(&format!("call:\n unknown call\n"));
                }
            }
        }

        for call in &self.subcalls {
            s.push_str(&format!("subcall:\n {}\n", call.to_string_pretty(cage)?));
        }

        for call in &self.callbacks {
            s.push_str(&format!("callback:\n {}\n", call.to_string_pretty(cage)?));
        }

        Ok(s)
    }

    pub fn try_decode_calldata(&self, cage: &Cage) -> Option<TestCase> {
        let mut decoded_tc = self.clone();
        for call in decoded_tc.calls.iter_mut() {
            if !call.try_decode_calldata(cage) {
                return None;
            }
        }
        Some(decoded_tc)
    }

}

impl EVMCall {
    pub fn to_string_pretty(&self, cage: &Cage) -> Result<String> {
        let mut s = String::new();

        s.push_str(&format!("to: {:x}\n", self.to));
        s.push_str(&format!("value: {}\n", self.value));
        if self.calldata.len() < 4 {
            return Err(eyre::eyre!("calldata too short"));
        }

        let selector = self.calldata[0..4].to_vec();

        let readable_targets = &cage.env.read().unwrap().targets;

        let contract = match readable_targets.get(&self.to) {
            Some(_contract) => _contract,
            None => {
                return Err(eyre::eyre!("could not find {:x} in readable_targets", &self.to));
            }
        };

        let func = contract.functions()
                    .find(|f| f.short_signature().as_slice() == selector.as_slice())
                    .ok_or_else(|| eyre::eyre!("function not found"))?;

        let tokens = func.decode_input(&self.calldata[4..])?;

        s.push_str(&format!("calldata: {:?}({:?})", func.name, tokens));
        Ok(s)
    }

    pub fn try_decode_calldata(&mut self, cage: &Cage) -> bool {

        if self.calldata.len() < 4 {
            return false;
        }

        let selector = self.calldata[0..4].to_vec();

        let readable_targets = &cage.env.read().unwrap().targets;
        let contract = match readable_targets.get(&self.to) {
            Some(_contract) => _contract,
            None => {return false;}
        };
    
        match contract.functions()
                    .find(|f| f.short_signature().as_slice() == selector.as_slice())
                    .ok_or_else(|| eyre::eyre!("function not found")) {
            Ok(func) => {
                self.name = func.name.clone();
                match func.decode_input(&self.calldata[4..]) {
                    Ok(tokens) => {
                        self.args = tokens;
                        return true;
                    },
                    Err(_) => {
                        return false;
                    }
                };
            },
            Err(_) => {
                return false;
            }
        };

    }

}