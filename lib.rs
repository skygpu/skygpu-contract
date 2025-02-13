#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(feature = "std", allow(warnings))]

pub mod math;

#[rust_chain::contract]
#[allow(dead_code)]
mod skygpu {
    use rust_chain::{
        ACTIVE,
        Name, Asset, Symbol, Checksum256, TimePointSec, Action, PermissionLevel,
        name, chain_println,
        string::ToString,
        require_auth, check, assert_sha256, require_recipient
    };
    use crate::math;

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
        token_symbol: Symbol,
        global_nonce: u64
    }

    #[chain(table="users")]
    pub struct Account {
        #[chain(primary)]
        user: Name,
        balance: Asset
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
        body: String,
        binary_data: String,
        timestamp: TimePointSec
    }

    impl Request {
        pub fn hash_str(&self) -> String {
            self.id.to_string() + &self.body + &self.binary_data
        }
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

        // SYSTEM

        #[chain(action = "config")]
        pub fn init_config(&mut self, token_account: Name, token_symbol: Symbol) {
            require_auth(self.receiver);
            let config_db = Config::new_table_with_scope(self.receiver, self.receiver);
            config_db.set(&Config{token_account, token_symbol, global_nonce: 0}, self.receiver);
        }

        pub fn get_config(&self) -> Config {
            let config_db = Config::new_table(self.receiver);
            check(config_db.get().is_some(), "gpu contract not configured yet");
            config_db.get().unwrap()
        }

        pub fn increment_nonce(&mut self) -> u64 {
            let config_db = Config::new_table_with_scope(self.receiver, self.receiver);
            let mut cfg = self.get_config();
            let prev_nonce = cfg.global_nonce;
            cfg.global_nonce += 1;
            config_db.set(&cfg, self.receiver);
            prev_nonce
        }

        #[chain(action = "clean")]
        pub fn clean(&mut self, accounts: bool) {
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

            if accounts {
                let accounts = Account::new_table_with_scope(self.receiver, self.receiver);
                let mut it = accounts.lower_bound(0);
                while !it.is_end() {
                    accounts.remove(&it);
                    it = accounts.next(&it);
                }
            }
        }

        // BALANCE

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

        // USER

        #[chain(action = "enqueue")]
        pub fn enqueue(
            &mut self,
            user: Name,
            request_body: String,
            binary_data: String,
            reward: Asset,
            min_verification: u32
        ) {
            require_auth(user);

            // escrow funds during request life
            self.sub_balance(user, reward);

            let prev_nonce = self.increment_nonce();

            let queue_db = Request::new_table_with_scope(self.receiver, self.receiver);
            queue_db.store(&Request{
                id: prev_nonce,
                user,
                reward,
                min_verification,
                body: request_body,
                binary_data,
                timestamp: TimePointSec::current()
            }, user);
        }

        #[chain(action = "dequeue")]
        pub fn dequeue(&mut self, user: Name, request_id: u64) {
            require_auth(user);
            let queue_db = Request::new_table_with_scope(self.receiver, self.receiver);
            let it = queue_db.find(request_id);
            check(it.is_end(), "request not found");

            let req = it.get_value().unwrap();

            // release reward escrow
            self.add_balance(user, req.reward);

            queue_db.remove(&it);
        }

        // WORKER

        #[chain(action = "regworker")]
        pub fn register_worker(
            &mut self,
            account: Name,
            url: String
        ) {
            require_auth(account);
            let worker_db = Worker::new_table_with_scope(self.receiver, self.receiver);
            let it = worker_db.find(account.n);
            check(it.is_end(), "worker already registered");

            worker_db.store(&Worker{
                account,
                joined: TimePointSec::current(),
                left: Default::default(),
                url,
            }, account);
        }


        #[chain(action = "unregworker")]
        pub fn unregister_worker(
            &mut self,
            account: Name,
            unreg_reason: String
        ) {
            require_auth(account);
            let worker_db = Worker::new_table_with_scope(self.receiver, self.receiver);
            let it = worker_db.find(account.n);
            check(!it.is_end(), "worker not registered");
            worker_db.remove(&it);
        }


        #[chain(action = "workbegin")]
        pub fn accept_work(
            &mut self,
            worker: Name,
            request_id: u64,
            max_workers: u32
        ) {
            require_auth(worker);
            let queue_db = Request::new_table_with_scope(self.receiver, self.receiver);
            let it = queue_db.find(request_id);
            check(it.is_end(), "request not found");

            let status_db = Status::new_table_with_scope(self.receiver, Name::from_u64(request_id));
            let it = status_db.find(worker.n);
            check(it.is_end(), "request already started");

            let mut it = status_db.lower_bound(0);
            let mut status_counter = 0;
            while !it.is_end() {
                it = status_db.next(&it);
                status_counter += 1;
            }
            check(status_counter >= max_workers, "too many workers already on this request");

            status_db.store(&Status{
                worker,
                status: String::from("started"),
                started: TimePointSec::current()
            }, worker);
        }

        #[chain(action = "workcancel")]
        pub fn cancel_work(
            &mut self,
            worker: Name,
            request_id: u64,
            reason: String
        ) {
            require_auth(worker);

            let status_db = Status::new_table_with_scope(self.receiver, Name::from_u64(request_id));
            let it = status_db.find(worker.n);
            check(!it.is_end(), "status not found");

            status_db.remove(&it);
        }

        #[chain(action = "submit")]
        pub fn submit_work(
            &mut self,
            worker: Name,
            request_id: u64,
            request_hash: Checksum256,
            result_hash: Checksum256,
            ipfs_hash: String
        ) {
            let config = self.get_config();
            require_auth(worker);

            let queue_db = Request::new_table_with_scope(self.receiver, self.receiver);
            let rit = queue_db.find(request_id);
            check(rit.is_end(), "request not found");

            let req = rit.get_value().unwrap();

            let hash_str = req.hash_str();
            chain_println!("hashing: ".to_string() + &hash_str);
            assert_sha256(hash_str.as_bytes(), &request_hash);

            let status_db = Status::new_table_with_scope(self.receiver, Name::from_u64(request_id));
            let it = status_db.find(worker.n);
            check(!it.is_end(), "status not found");
            status_db.remove(&it);

            let results_db = Result::new_table_with_scope(self.receiver, self.receiver);
            let result_worker_idx = results_db.get_idx_by_worker();
            let it = result_worker_idx.find(worker);
            check(it.is_end(), "already submitted result");

            let mut result_id = 0;
            let mut result_it = results_db.find(0);
            while !result_it.is_end() {
                result_id += 1;
                result_it = results_db.next(&result_it);
            }
            
            results_db.store(&Result{
                id: result_id,
                request_id,
                user: req.user,
                worker,
                result_hash,
                ipfs_hash,
                submited: TimePointSec::current(),
            }, worker);

            let result_hash_idx = results_db.get_idx_by_result_hash();
            let mut it = result_hash_idx.find(result_hash);
            let mut match_count = 0;
            while !it.is_end() {
                match_count += 1;
                it = result_hash_idx.next(&it);
            }

            if match_count > req.min_verification {
                // got enough matches, split reward between miners,
                // clear request results, status & queue

                let status_db = Status::new_table_with_scope(self.receiver, Name::from_u64(request_id));
                let mut it = status_db.lower_bound(0);
                while !it.is_end() {
                    status_db.remove(&it);
                    it = status_db.next(&it);
                }

                let mut payments: Vec<Name> = Vec::default();

                // iterate over results by ascending timestamp
                // delete all related to this request but store
                // first n miners (n == verification_amount)
                let results_time_index = results_db.get_idx_by_submited();
                let (mut it, _ts) = results_time_index.lower_bound(req.timestamp);

                while !it.is_end() {
                    let res = results_db.find(it.primary).get_value().unwrap();
                    if res.request_id == request_id {
                        if payments.len() < req.min_verification as usize {
                            payments.push(res.worker);
                        }

                        results_time_index.remove(&it);
                    }
                    it = results_time_index.next(&it);
                }

                payments.push(worker);

                chain_println!("paying:");
                payments.iter().for_each(|w| { chain_println!(w); } );

                // split reward and  add it to miner balances
                let split_factor = math::divide(
                    Asset::new(1, config.token_symbol),
                    Asset::new(req.min_verification as i64, config.token_symbol)
                );

                chain_println!("reward: ", req.reward);
                chain_println!("factor: ", split_factor);

                let payment = math::multiply(req.reward, split_factor);
                chain_println!("payment: ", payment);

                payments.iter().for_each(|miner| {
                    self.add_balance(*miner, payment);
                    require_recipient(*miner);
                });

                require_recipient(req.user);

                queue_db.remove(&rit);
            }
        }
    }
}

#[cfg(feature="std")]
#[no_mangle]
fn native_apply(receiver: u64, first_receiver: u64, action: u64) {
    crate::skygpu::native_apply(receiver, first_receiver, action);
}
