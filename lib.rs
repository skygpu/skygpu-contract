#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(feature = "std", allow(warnings))]

#[rust_chain::contract]
#[allow(dead_code)]
mod skygpu {
    use rust_chain::{Name, Asset, Symbol, Checksum256, TimePointSec};

    #[chain(table="config", singleton)]
    pub struct Config {
        token_account: Name,
        token_symbol: Symbol
    }

    #[chain(table="users")]
    pub struct Account {
        #[chain(primary)]
        user: Name,
        balance: Asset,
        nonce: u64
    }

    #[chain(table="workers")]
    pub struct Worker {
        #[chain(primary)]
        account: Name,
        joined: TimePointSec,
        left: TimePointSec,
        url: String
    }

    #[chain(table="queue")]
    pub struct Request {
        #[chain(primary)]
        id: u64,
        user: Name,
        reward: Asset,
        min_verification: u32,
        nonce: u64,
        body: String,
        binary_data: String,
        timestamp: TimePointSec
    }

    #[chain(table="status")]
    pub struct Status {
        #[chain(primary)]
        worker: Name,
        status: String,
        started: TimePointSec
    }

    #[chain(table="results")]
    pub struct Result {
        #[chain(primary)]
        id: u64,
        #[chain(secondary)]
        request_id: u64,
        #[chain(secondary)]
        user: Name,
        #[chain(secondary)]
        worker: Name,
        #[chain(secondary)]
        result_hash: Checksum256,
        ipfs_hash: String,
        #[chain(secondary)]
        submited: TimePointSec
    }

    #[chain(main)]
    pub struct Contract {
        receiver: Name,
        first_receiver: Name,
        action: Name,
        config: Config,
        config_db: Box<ConfigMultiIndex>
    }

    impl Contract {
        pub fn new(receiver: Name, first_receiver: Name, action: Name) -> Self {
            let config_db = Config::new_table(receiver);
            let config = config_db.get().unwrap_or(Config{
                token_account: Name::from_str("eosio.token"),
                token_symbol: Symbol::new("TLOS", 4),
            });
            Self {
                receiver: receiver,
                first_receiver: first_receiver,
                action: action,
                config_db,
                config
            }
        }

        #[chain(action = "config")]
        pub fn init_config(&mut self, token_contract: Name, token_symbol: Symbol) {
            self.config.token_account = token_contract;
            self.config.token_symbol = token_symbol;
        }
    }

    impl Drop for Contract {
        fn drop(&mut self) {
            self.config_db.set(&self.config, self.receiver);
        }
    }
}

#[cfg(feature="std")]
#[no_mangle]
fn native_apply(receiver: u64, first_receiver: u64, action: u64) {
    crate::skygpu::native_apply(receiver, first_receiver, action);
}