#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(feature = "std", allow(warnings))]

#[rust_chain::contract]
#[allow(dead_code)]
mod skygpu {
    use rust_chain::{
        ACTIVE,
        Name, Asset, Symbol, Checksum256, TimePointSec,
        Action, PermissionLevel,
        name,
        require_auth, check
    };

    #[chain(packer)]
    struct TransferParams {
        from: Name,
        to: Name,
        quantity: Asset,
        memo: Vec<u8>
    }

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
    }

    impl Contract {
        pub fn new(receiver: Name, first_receiver: Name, action: Name) -> Self {
            Self {
                receiver: receiver,
                first_receiver: first_receiver,
                action: action,
            }
        }

        #[chain(action = "config")]
        pub fn init_config(&mut self, token_account: Name, token_symbol: Symbol) {
            require_auth(self.receiver);
            let config_db = Config::new_table(self.receiver);
            config_db.set(&Config{token_account, token_symbol}, self.receiver);
        }

        pub fn get_config(&self) -> Config {
            let config_db = Config::new_table(self.receiver);
            check(config_db.get().is_some(), "gpu contract not configured yet");
            config_db.get().unwrap()
        }

        #[chain(action = "clean")]
        pub fn clean(&mut self) {
            require_auth(self.receiver);
            let queue = Request::new_table_with_scope(self.receiver, self.receiver);
            let mut it = queue.lower_bound(0);
            while !it.is_end() {
                let scope = Name::from_u64(it.get_value().unwrap().id);
                let status = Status::new_table_with_scope(self.receiver, scope);

                let mut status_it = status.lower_bound(0);
                while !status_it.is_end() {
                    status.remove(&status_it);
                    status_it = status.lower_bound(0);
                }

                queue.remove(&it);
                it = queue.lower_bound(0);
            }

            let results = Result::new_table_with_scope(self.receiver, self.receiver);
            let mut it = results.lower_bound(0);
            while !it.is_end() {
                results.remove(&it);
                it = results.lower_bound(0);
            }
        }

        pub fn add_balance(
            &mut self,
            owner: Name,
            quantity: Asset
        ) {
            let accounts_db = Account::new_table_with_scope(self.receiver, self.receiver);
            let it = accounts_db.find(owner.n);
            if it.is_end() {
                accounts_db.store(&Account{
                    user: owner,
                    balance: quantity,
                    nonce: 0
                }, owner);
            } else {
                let mut acc = it.get_value().unwrap();
                acc.balance += quantity;
                accounts_db.update(&it, &acc, owner);
            }
        }

        pub fn sub_balance(
            &mut self,
            owner: Name,
            quantity: Asset
        ) {
            let accounts_db = Account::new_table_with_scope(self.receiver, self.receiver);
            let it = accounts_db.find(owner.n);
            check(!it.is_end(), "no user account found");
            let mut acc = it.get_value().unwrap();
            check(quantity.amount() > acc.balance.amount(), "overdrawn balance");
            acc.balance -= quantity;
            accounts_db.update(&it, &acc, owner);
        }

        #[chain(action = "transfer", notify)]
        pub fn deposit(
            &mut self,
            from: Name,
            to: Name,
            quantity: Asset,
            _memo: Vec<u8>
        ) {
            if (from == self.receiver) && (to != self.receiver) {
                return;
            }

            let config = self.get_config();

            check(self.first_receiver == config.token_account, "wrong token contract");

            check(quantity.amount() > 0, "can only deposit non zero amount");
            check(quantity.symbol() == config.token_symbol, "sent wrong token");

            self.add_balance(to, quantity);
        }

        #[chain(action = "withdraw")]
        pub fn withdraw(&mut self, user: Name, quantity: Asset) {
            require_auth(user);
            let config = self.get_config();
            self.sub_balance(user, quantity);

            let perm = PermissionLevel{actor: self.receiver, permission: ACTIVE};
            let params = TransferParams{from: self.receiver, to: user, quantity, memo: Vec::new()};
            let action = Action::new(config.token_account, name!("transfer"), perm, &params);
            action.send();
        }
    }
}

#[cfg(feature="std")]
#[no_mangle]
fn native_apply(receiver: u64, first_receiver: u64, action: u64) {
    crate::skygpu::native_apply(receiver, first_receiver, action);
}