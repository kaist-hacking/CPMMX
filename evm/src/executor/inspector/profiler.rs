use std::{mem::size_of, sync::{RwLock, Arc}};

use revm::{Database, EVMData, Inspector, Interpreter, Return};

use super::oracle::CageEnv;

const MAP_SIZE: usize = 1 << 16;

#[derive(Debug, Clone)]
pub struct BitMaps(pub Box<[usize; MAP_SIZE]>);

impl Default for BitMaps {
    fn default() -> Self {
        Self(Box::new([0; MAP_SIZE]))
    }
}

impl BitMaps {
    pub fn set(&mut self, index: usize) {
        let nbits = size_of::<usize>();
        let norm = index % (MAP_SIZE * nbits);
        let (word, bit) = (norm / nbits, norm % nbits);
        self.0[word] |= 1 << bit;
    }

    pub fn union(&mut self, other: &Self) {
        for (a, b) in self.0.iter_mut().zip(other.0.iter()) {
            *a |= *b;
        }
    }

    pub fn intersection(&mut self, other: &Self) {
        for (a, b) in self.0.iter_mut().zip(other.0.iter()) {
            *a &= *b;
        }
    }

    pub fn is_subset(&self, other: &Self) -> bool {
        for (a, b) in self.0.iter().zip(other.0.iter()) {
            if *a & *b != *a {
                return false;
            }
        }
        true
    }

    pub fn len(&self) -> usize {
        self.0.iter().map(|x| x.count_ones() as usize).sum()
    }

    pub fn difference(&mut self, other: &Self) {
        for (a, b) in self.0.iter_mut().zip(other.0.iter()) {
            *a &= !*b;
        }
    }
}

#[derive(Debug, Clone)]
pub struct Profiler {
    /// Maps that track instruction hit data.
    pub maps: BitMaps,
    pub cage_env: Arc<RwLock<CageEnv>>,
}

impl Profiler {
    pub fn new(cage_env: Arc<RwLock<CageEnv>>) -> Self {
        Profiler {
            maps: BitMaps::default(),
            cage_env,
        }
    }
}

impl<DB> Inspector<DB> for Profiler
where
    DB: Database,
{
    fn step(
        &mut self,
        interpreter: &mut Interpreter,
        _: &mut EVMData<'_, DB>,
        _is_static: bool,
    ) -> Return {

        let readable_cage_env = self.cage_env.read().unwrap();
        // if !readable_cage_env.executing_testcase_call {
        //     return Return::Continue; // only track coverage for testcase functions
        // }
        if readable_cage_env.target_token.unwrap() != interpreter.contract.address {
            return Return::Continue; // only track coverage for main target contract
        }

        let contract_hash = interpreter.contract.bytecode.hash().to_low_u64_be() as usize;
        let pc = interpreter.program_counter();
        self.maps.set(contract_hash ^ pc);
        Return::Continue
    }
}
