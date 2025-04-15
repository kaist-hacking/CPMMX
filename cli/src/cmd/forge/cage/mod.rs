use std::sync::{RwLock, Arc};

use crate::{
    utils,
    cmd::{
    LoadConfig,
    forge::{build::CoreBuildArgs},
    }
};
use cast::executor::{
    inspector::{CheatsConfig}, 
    opts::EvmOpts,
    Backend,
    ExecutorBuilder
};
use clap::{
    Parser, Subcommand,
};
use ethers::{
    solc::{
        info::ContractInfo,
        utils::canonicalize,
        artifacts::{Sources, Source, CompactContract},
        project::ProjectCompiler
    },
    types::{U256, H160},
};
use forge::executor::inspector::oracle::CageEnv;
use foundry_config::{
    figment::{
        self,
        Metadata, 
        value::{Dict, Map}, 
        Provider, Profile
    },
    Config
};
use eyre::Result;
use foundry_common::{
    compile,
    evm::EvmArgs,
};
use foundry_utils::scan::Scanner;
use tracing::trace;

mod fuzz;
use fuzz::Cage;
mod corpus;
mod testcase;
mod rawtestcase;
mod swap_template;
mod exploit_template;

// Loads project's figment and merges the build cli arguments into it
foundry_config::merge_impl_figment_convert!(TestArgs, opts, evm_opts);

#[derive(Debug, Parser)]
pub struct CageArgs {
    #[clap(subcommand)]
    pub sub: CageSubcommands,
}

#[derive(Debug, Subcommand)]
pub enum CageSubcommands {
    #[clap(about = "Try generate exploit")]
    Test(TestArgs),

    #[clap(about = "Execute testcase")]
    RunTc(TestArgs),

    #[clap(about = "Calculate fees")]
    Analyze(TestArgs),

    #[clap(about = "Execute solidity code")]
    RunSol(TestArgs),

}


#[derive(Debug, Clone)]
pub struct CageConfig {
    pub foundry_config: Config,
    pub evm_opts: EvmOpts,
    pub target_token: String,
    pub base_token: String,
    pub pair: String,
}

/// CLI arguments for `forge cage test`.
#[derive(Debug, Parser)]
pub struct TestArgs {
    #[clap(
        help = "Target token address",
        value_name = "TARGET_TOKEN"
    )]
    pub target_token: String,

    #[clap(
        help = "Base token address",
        value_name = "BASE_TOKEN"
    )]
    pub base_token: String,

    #[clap(
        help = "Target pair address",
        value_name = "PAIR"
    )]
    pub pair: String,

    #[clap(
        help = "Etherscan API key",
        value_name = "ETHERSCAN_API_KEY"
    )]
    pub etherscan_api_key: String,
    
    #[clap(flatten)]
    evm_opts: EvmArgs,

    #[clap(flatten)]
    opts: CoreBuildArgs,

    
    #[clap(
        long,
        help = "Testcase to execute",
        value_name = "TESTCASE_PATH"
    )]
    testcase_file: Option<String>
}

impl TestArgs {

    fn setup_cage(self) -> Result<Cage> {
        let (foundry_config, evm_opts) = self.load_config_and_evm_opts_emit_warnings()?;
        let env = evm_opts.evm_env_blocking()?;

        // Create an in-memory backend
        let backend = Backend::spawn(
            evm_opts.get_fork(&foundry_config, env.clone()),
        );

        // Set up cage_env (shared with inspector and cage)
        let cage_env = Arc::new(RwLock::new(CageEnv::default()));

        // Set up scanner (shared with inspector and cage)
        let fork_url = evm_opts.fork_url.as_ref()
            .expect("Must provide fork_url with target contract address").clone();
        let scanner = Arc::new(Scanner::new(fork_url, self.etherscan_api_key));

        // Build a new executor
        let executor = ExecutorBuilder::default()
            .with_config(env)
            .set_tracing(true)
            .set_profiler(true)
            .set_oracle(true, Arc::clone(&cage_env), Arc::clone(&scanner))
            .with_spec(utils::evm_spec(&foundry_config.evm_version))
            .with_gas_limit(evm_opts.gas_limit())
            .with_cheatcodes(CheatsConfig::new(&foundry_config, &evm_opts))
            .build(backend);

        let config = CageConfig {
            foundry_config,
            evm_opts,
            target_token: self.target_token,
            base_token: self.base_token,
            pair: self.pair,
        };

        Ok(Cage::new(executor, config, cage_env, scanner))
    }

    pub fn test(self) -> Result<()> {
        let mut cage = self.setup_cage()?;
        cage.setup("./fuzz/Bridge.sol".to_string())?;
        cage.start();
        Ok(())
    }

    pub fn run_tc(self) -> Result<()> {
        let testcase_file = self.testcase_file.clone().unwrap();

        let mut cage = self.setup_cage()?;
        cage.setup("./fuzz/Bridge.sol".to_string())?;
        cage.execute_tc(testcase_file)?;
        Ok(())
    }

    pub fn analyze(self) -> Result<()> {
        let mut cage = self.setup_cage()?;
        cage.setup("./fuzz/BridgeAnalyze.sol".to_string())?;
        cage.execute_sol()?;
        Ok(())
    }

    pub fn run_sol(self) -> Result<()> {
        let mut cage = self.setup_cage()?;
        cage.setup("./fuzz/BridgeRunSol.sol".to_string())?;
        cage.execute_sol()?;
        Ok(())
    }

}

// Make this args a `figment::Provider` so that it can be merged into the `Config`
impl Provider for TestArgs {
    fn metadata(&self) -> Metadata {
        Metadata::named("Build Args Provider")
    }

    fn data(&self) -> Result<Map<Profile, Dict>, figment::Error> {
        let dict = Dict::default();
        Ok(Map::from([(Config::selected_profile(), dict)]))
    }
}