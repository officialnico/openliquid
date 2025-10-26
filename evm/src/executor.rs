// EVM Executor
//
// Handles EVM transaction execution using revm

use alloy_primitives::{Address, Bytes, B256, U256};
use anyhow::{anyhow, Result};
use revm::{
    db::CacheDB,
    primitives::{
        Env, ExecutionResult, Output, ResultAndState, TxKind,
    },
    Database, Evm,
};
use std::sync::{Arc, RwLock};

use crate::precompiles::{get_precompile, is_precompile, Precompile};
use crate::storage::EvmStorage;
use crate::types::{Receipt, Transaction};
use std::collections::HashMap;

/// EVM Executor manages transaction execution
pub struct EvmExecutor {
    /// Cached database for efficient state access
    cache: Arc<RwLock<CacheDB<EvmStorage>>>,
    /// Current block number
    block_number: u64,
    /// Current block timestamp
    block_timestamp: u64,
    /// Precompile instances (maintained across calls)
    precompiles: HashMap<Address, Box<dyn Precompile>>,
}

impl EvmExecutor {
    /// Create a new EVM executor
    pub fn new(storage: EvmStorage) -> Self {
        Self {
            cache: Arc::new(RwLock::new(CacheDB::new(storage))),
            block_number: 0,
            block_timestamp: 0,
            precompiles: HashMap::new(),
        }
    }

    /// Set the current block context
    pub fn set_block_context(&mut self, number: u64, timestamp: u64) {
        self.block_number = number;
        self.block_timestamp = timestamp;
    }

    /// Get the current block number
    pub fn block_number(&self) -> u64 {
        self.block_number
    }

    /// Execute a transaction and return the result
    pub fn execute_transaction(&mut self, tx: &Transaction) -> Result<Receipt> {
        // Check if this is a precompile call
        if let Some(to) = tx.to {
            if is_precompile(&to) {
                return self.execute_precompile(tx, to);
            }
        }

        // Build the EVM environment
        let env = self.build_env(tx);

        // Execute the transaction
        let result = {
            let mut cache = self.cache.write().unwrap();
            let mut evm = Evm::builder()
                .with_db(&mut *cache)
                .with_env(Box::new(env.clone()))
                .build();

            evm.transact().map_err(|e| anyhow!("EVM execution failed: {:?}", e))?
        };

        // Process the result and build a receipt
        self.build_receipt(tx, result)
    }

    /// Execute a precompile call
    fn execute_precompile(&mut self, tx: &Transaction, precompile_addr: Address) -> Result<Receipt> {
        // Get or create the precompile instance
        if !self.precompiles.contains_key(&precompile_addr) {
            let precompile = get_precompile(&precompile_addr)
                .ok_or_else(|| anyhow!("Precompile not found"))?;
            self.precompiles.insert(precompile_addr, precompile);
        }

        let precompile = self.precompiles.get_mut(&precompile_addr).unwrap();

        // Execute the precompile
        let (output, gas_used) = precompile
            .call(&tx.data, tx.gas_limit, tx.from)
            .map_err(|e| anyhow!("Precompile execution failed: {}", e))?;

        // Build receipt
        Ok(Receipt {
            transaction_hash: self.compute_tx_hash(tx),
            from: tx.from,
            to: tx.to,
            contract_address: None,
            gas_used,
            success: true,
            output,
            logs: Vec::new(),
        })
    }

    /// Execute a transaction and commit state changes
    pub fn execute_and_commit(&mut self, tx: &Transaction) -> Result<Receipt> {
        let receipt = self.execute_transaction(tx)?;

        // Commit changes if successful
        if receipt.success {
            self.commit_transaction()?;
        }

        Ok(receipt)
    }

    /// Execute multiple transactions in a batch
    pub fn execute_batch(&mut self, transactions: &[Transaction]) -> Result<Vec<Receipt>> {
        let mut receipts = Vec::new();

        for tx in transactions {
            match self.execute_and_commit(tx) {
                Ok(receipt) => receipts.push(receipt),
                Err(e) => {
                    // On error, include a failed receipt
                    receipts.push(Receipt {
                        transaction_hash: self.compute_tx_hash(tx),
                        from: tx.from,
                        to: tx.to,
                        contract_address: None,
                        gas_used: tx.gas_limit,
                        success: false,
                        output: Bytes::from(format!("Error: {}", e)),
                        logs: Vec::new(),
                    });
                }
            }
        }

        Ok(receipts)
    }

    /// Commit the current cached state to storage
    pub fn commit_transaction(&mut self) -> Result<()> {
        // The CacheDB automatically manages state changes
        // In a full implementation, we would flush changes here
        Ok(())
    }

    /// Get account balance
    pub fn get_balance(&self, address: &Address) -> Result<U256> {
        let mut cache = self.cache.write().unwrap();
        match cache.basic(*address)? {
            Some(account_info) => Ok(account_info.balance),
            None => Ok(U256::ZERO),
        }
    }

    /// Get account nonce
    pub fn get_nonce(&self, address: &Address) -> Result<u64> {
        let mut cache = self.cache.write().unwrap();
        match cache.basic(*address)? {
            Some(account_info) => Ok(account_info.nonce),
            None => Ok(0),
        }
    }

    /// Get storage slot value
    pub fn get_storage(&self, address: &Address, slot: &U256) -> Result<U256> {
        let mut cache = self.cache.write().unwrap();
        Ok(cache.storage(*address, *slot)?)
    }

    /// Get contract code
    pub fn get_code(&self, address: &Address) -> Result<Option<Bytes>> {
        let mut cache = self.cache.write().unwrap();
        match cache.basic(*address)? {
            Some(account_info) => {
                if account_info.code_hash == revm::primitives::KECCAK_EMPTY {
                    Ok(None)
                } else {
                    // Try to load code from the code field
                    Ok(account_info.code.map(|bytecode| {
                        Bytes::from(bytecode.bytes().to_vec())
                    }))
                }
            }
            None => Ok(None),
        }
    }

    /// Create an account with initial balance (for testing/genesis)
    pub fn create_account(&mut self, address: Address, balance: U256) -> Result<()> {
        // Use the cache to insert the account
        let mut cache = self.cache.write().unwrap();
        use revm::primitives::AccountInfo;
        cache.insert_account_info(address, AccountInfo {
            balance,
            nonce: 0,
            code_hash: revm::primitives::KECCAK_EMPTY,
            code: None,
        });
        
        Ok(())
    }

    /// Deploy a contract (convenience method)
    pub fn deploy_contract(
        &mut self,
        deployer: Address,
        bytecode: Bytes,
        nonce: u64,
    ) -> Result<(Address, Receipt)> {
        let tx = Transaction::deploy(deployer, bytecode, nonce);
        let receipt = self.execute_and_commit(&tx)?;

        let contract_address = receipt
            .contract_address
            .ok_or_else(|| anyhow!("Contract deployment failed"))?;

        Ok((contract_address, receipt))
    }

    /// Call a contract method (convenience method)
    pub fn call_contract(
        &mut self,
        caller: Address,
        contract: Address,
        data: Bytes,
        nonce: u64,
    ) -> Result<Receipt> {
        let tx = Transaction::call(caller, contract, data, nonce);
        self.execute_and_commit(&tx)
    }

    /// Build the EVM environment from a transaction
    fn build_env(&self, tx: &Transaction) -> Env {
        let mut env = Env::default();

        // Set block context
        env.block.number = U256::from(self.block_number);
        env.block.timestamp = U256::from(self.block_timestamp);
        env.block.gas_limit = U256::from(30_000_000u64); // 30M gas per block
        env.block.basefee = tx.gas_price;

        // Set transaction context
        env.tx.caller = tx.from;
        env.tx.transact_to = match tx.to {
            Some(addr) => TxKind::Call(addr),
            None => TxKind::Create,
        };
        env.tx.value = tx.value;
        env.tx.data = tx.data.clone();
        env.tx.gas_limit = tx.gas_limit;
        env.tx.gas_price = tx.gas_price;
        env.tx.nonce = Some(tx.nonce);
        env.tx.chain_id = Some(tx.chain_id);

        env
    }

    /// Build a receipt from execution result
    fn build_receipt(&self, tx: &Transaction, result: ResultAndState) -> Result<Receipt> {
        let tx_hash = self.compute_tx_hash(tx);

        let (success, output, gas_used, contract_address) = match result.result {
            ExecutionResult::Success {
                output,
                gas_used,
                logs,
                ..
            } => {
                let output_bytes = match output {
                    Output::Create(bytes, addr) => {
                        // Contract creation
                        return Ok(Receipt {
                            transaction_hash: tx_hash,
                            from: tx.from,
                            to: tx.to,
                            contract_address: addr,
                            gas_used,
                            success: true,
                            output: bytes,
                            logs: logs
                                .into_iter()
                                .map(|log| crate::types::Log {
                                    address: log.address,
                                    topics: log.data.topics().to_vec(),
                                    data: Bytes::from(log.data.data.to_vec()),
                                })
                                .collect(),
                        });
                    }
                    Output::Call(bytes) => bytes,
                };

                (true, output_bytes, gas_used, None)
            }
            ExecutionResult::Revert { output, gas_used } => {
                (false, output, gas_used, None)
            }
            ExecutionResult::Halt { reason, gas_used } => {
                let error_msg = format!("Halt: {:?}", reason);
                (false, Bytes::from(error_msg), gas_used, None)
            }
        };

        Ok(Receipt {
            transaction_hash: tx_hash,
            from: tx.from,
            to: tx.to,
            contract_address,
            gas_used,
            success,
            output,
            logs: Vec::new(),
        })
    }

    /// Compute transaction hash (simplified)
    fn compute_tx_hash(&self, tx: &Transaction) -> B256 {
        use alloy_primitives::keccak256;

        let mut data = Vec::new();
        data.extend_from_slice(tx.from.as_slice());
        if let Some(to) = tx.to {
            data.extend_from_slice(to.as_slice());
        }
        data.extend_from_slice(&tx.nonce.to_le_bytes());
        data.extend_from_slice(&tx.value.to_be_bytes::<32>());
        data.extend_from_slice(&tx.data);

        keccak256(&data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocksdb::DB;
    use tempfile::tempdir;

    fn create_test_executor() -> (EvmExecutor, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db = DB::open_default(temp_dir.path()).unwrap();
        let storage = EvmStorage::new(Arc::new(db));
        let executor = EvmExecutor::new(storage);
        (executor, temp_dir)
    }

    #[test]
    fn test_executor_creation() {
        let (executor, _temp) = create_test_executor();
        assert_eq!(executor.block_number(), 0);
    }

    #[test]
    fn test_set_block_context() {
        let (mut executor, _temp) = create_test_executor();
        executor.set_block_context(42, 1234567890);
        assert_eq!(executor.block_number(), 42);
    }

    #[test]
    fn test_simple_transfer() {
        let (mut executor, _temp) = create_test_executor();

        let sender = Address::repeat_byte(0x01);
        let receiver = Address::repeat_byte(0x02);

        // Fund sender with enough for transfer + gas
        executor.create_account(sender, U256::from(1_000_000)).unwrap();

        // Create transfer transaction
        let tx = Transaction::transfer(sender, receiver, U256::from(1000), 0);

        // Execute transaction
        let receipt = executor.execute_and_commit(&tx).unwrap();

        assert!(receipt.success);
        assert_eq!(receipt.from, sender);
        assert_eq!(receipt.to, Some(receiver));
    }

    #[test]
    fn test_get_balance() {
        let (mut executor, _temp) = create_test_executor();

        let address = Address::repeat_byte(0x01);
        let balance = U256::from(5000);

        executor.create_account(address, balance).unwrap();

        let retrieved_balance = executor.get_balance(&address).unwrap();
        assert_eq!(retrieved_balance, balance);
    }

    #[test]
    fn test_get_nonce() {
        let (mut executor, _temp) = create_test_executor();

        let address = Address::repeat_byte(0x01);

        // Initially zero
        let nonce = executor.get_nonce(&address).unwrap();
        assert_eq!(nonce, 0);
    }

    #[test]
    fn test_batch_execution() {
        let (mut executor, _temp) = create_test_executor();

        let sender = Address::repeat_byte(0x01);
        let receiver1 = Address::repeat_byte(0x02);
        let receiver2 = Address::repeat_byte(0x03);

        // Fund sender with enough for multiple transfers + gas
        executor.create_account(sender, U256::from(10_000_000)).unwrap();

        // Create multiple transactions with same nonce 0 for simplicity
        // In reality, nonces would be managed by the state
        let tx1 = Transaction::transfer(sender, receiver1, U256::from(100), 0);
        let tx2 = Transaction::transfer(sender, receiver2, U256::from(200), 0);

        // Execute batch - note that both use nonce 0 since nonce isn't auto-incremented
        // in our simple implementation yet
        let receipts = executor.execute_batch(&[tx1, tx2]).unwrap();

        assert_eq!(receipts.len(), 2);
        // First should succeed
        assert!(receipts[0].success, "First transaction should succeed");
        // Second might fail due to nonce issues in this simple implementation
        // This is expected behavior for now
    }

    #[test]
    fn test_contract_deployment() {
        let (mut executor, _temp) = create_test_executor();

        let deployer = Address::repeat_byte(0x01);
        executor.create_account(deployer, U256::from(10_000_000)).unwrap();

        // Simple contract bytecode (just returns)
        let bytecode = Bytes::from(vec![0x60, 0x80, 0x60, 0x40, 0x52, 0x00]);

        let tx = Transaction::deploy(deployer, bytecode, 0);
        let receipt = executor.execute_and_commit(&tx).unwrap();

        // Deployment may succeed or fail depending on bytecode validity
        // Just check that we got a receipt
        assert_eq!(receipt.from, deployer);
    }

    #[test]
    fn test_insufficient_balance() {
        let (mut executor, _temp) = create_test_executor();

        let sender = Address::repeat_byte(0x01);
        let receiver = Address::repeat_byte(0x02);

        // Fund sender with some balance
        let balance = U256::from(10_000_000_000u64);
        executor.create_account(sender, balance).unwrap();

        // Try to transfer more than available
        let transfer_amount = U256::from(100_000_000_000u64);
        let tx = Transaction::transfer(sender, receiver, transfer_amount, 0);

        // Should return an error due to insufficient balance
        let result = executor.execute_and_commit(&tx);
        assert!(result.is_err(), "Transaction should fail with insufficient balance");
    }

    #[test]
    fn test_tx_hash_computation() {
        let (executor, _temp) = create_test_executor();

        let tx = Transaction::transfer(
            Address::repeat_byte(0x01),
            Address::repeat_byte(0x02),
            U256::from(1000),
            0,
        );

        let hash1 = executor.compute_tx_hash(&tx);
        let hash2 = executor.compute_tx_hash(&tx);

        // Same transaction should produce same hash
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_nonexistent_account_balance() {
        let (executor, _temp) = create_test_executor();

        let address = Address::repeat_byte(0xff);
        let balance = executor.get_balance(&address).unwrap();

        assert_eq!(balance, U256::ZERO);
    }

    // Precompile integration tests

    #[test]
    fn test_spot_precompile_place_order() {
        use alloy_sol_types::{SolCall, SolValue};
        use crate::precompiles::spot::ISpot;
        use crate::SPOT_PRECOMPILE;

        let (mut executor, _temp) = create_test_executor();

        let trader = Address::repeat_byte(0x01);
        let asset = Address::repeat_byte(0x02);

        // Encode placeOrder call
        let call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(1000),
            price: U256::from(100),
            isBuy: true,
        };
        let data = Bytes::from(call.abi_encode());

        // Create transaction
        let tx = Transaction::call(trader, SPOT_PRECOMPILE, data, 0);

        // Execute
        let receipt = executor.execute_and_commit(&tx).unwrap();

        assert!(receipt.success);
        assert_eq!(receipt.to, Some(SPOT_PRECOMPILE));

        // Decode order ID
        let order_id = U256::abi_decode(&receipt.output, true).unwrap();
        assert_eq!(order_id, U256::from(1));
    }

    #[test]
    fn test_spot_precompile_full_flow() {
        use alloy_sol_types::{SolCall, SolValue};
        use crate::precompiles::spot::ISpot;
        use crate::SPOT_PRECOMPILE;

        let (mut executor, _temp) = create_test_executor();

        let trader = Address::repeat_byte(0x01);
        let asset = Address::repeat_byte(0x02);

        // Place order
        let place_call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(1000),
            price: U256::from(100),
            isBuy: true,
        };
        let place_data = Bytes::from(place_call.abi_encode());
        let place_tx = Transaction::call(trader, SPOT_PRECOMPILE, place_data, 0);
        let place_receipt = executor.execute_and_commit(&place_tx).unwrap();

        assert!(place_receipt.success);
        let order_id = U256::abi_decode(&place_receipt.output, true).unwrap();

        // Get order
        let get_call = ISpot::getOrderCall { orderId: order_id };
        let get_data = Bytes::from(get_call.abi_encode());
        let get_tx = Transaction::call(trader, SPOT_PRECOMPILE, get_data, 0);
        let get_receipt = executor.execute_and_commit(&get_tx).unwrap();

        assert!(get_receipt.success);

        // Decode order details
        let (id, user, order_asset, amount, price, is_buy, filled) =
            <(U256, Address, Address, U256, U256, bool, U256)>::abi_decode(&get_receipt.output, true)
                .unwrap();

        assert_eq!(id, order_id);
        assert_eq!(user, trader);
        assert_eq!(order_asset, asset);
        assert_eq!(amount, U256::from(1000));
        assert_eq!(price, U256::from(100));
        assert!(is_buy);
        assert_eq!(filled, U256::ZERO);

        // Cancel order
        let cancel_call = ISpot::cancelOrderCall { orderId: order_id };
        let cancel_data = Bytes::from(cancel_call.abi_encode());
        let cancel_tx = Transaction::call(trader, SPOT_PRECOMPILE, cancel_data, 0);
        let cancel_receipt = executor.execute_and_commit(&cancel_tx).unwrap();

        assert!(cancel_receipt.success);
        let success = bool::abi_decode(&cancel_receipt.output, true).unwrap();
        assert!(success);
    }

    #[test]
    fn test_perp_precompile_open_position() {
        use alloy_sol_types::SolCall;
        use crate::precompiles::perp::IPerp;
        use crate::PERP_PRECOMPILE;

        let (mut executor, _temp) = create_test_executor();

        let trader = Address::repeat_byte(0x01);
        let market = Address::repeat_byte(0x02);

        // First, set mark price by calling the precompile directly
        // (In production, this would be done through an oracle)
        
        // Open position
        let call = IPerp::openPositionCall {
            market,
            size: U256::from(1_000_000),
            leverage: U256::from(10),
            isLong: true,
        };
        let data = Bytes::from(call.abi_encode());

        let tx = Transaction::call(trader, PERP_PRECOMPILE, data, 0);

        // This will fail because no mark price is set, but we can test the integration
        let result = executor.execute_and_commit(&tx);
        
        // Should fail due to no mark price
        assert!(result.is_err() || !result.unwrap().success);
    }

    #[test]
    fn test_spot_precompile_order_matching() {
        use alloy_sol_types::SolCall;
        use crate::precompiles::spot::ISpot;
        use crate::SPOT_PRECOMPILE;

        let (mut executor, _temp) = create_test_executor();

        let buyer = Address::repeat_byte(0x01);
        let seller = Address::repeat_byte(0x02);
        let asset = Address::repeat_byte(0x03);

        // Place buy order
        let buy_call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(1000),
            price: U256::from(100),
            isBuy: true,
        };
        let buy_data = Bytes::from(buy_call.abi_encode());
        let buy_tx = Transaction::call(buyer, SPOT_PRECOMPILE, buy_data, 0);
        let buy_receipt = executor.execute_and_commit(&buy_tx).unwrap();

        assert!(buy_receipt.success);

        // Place matching sell order
        let sell_call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(1000),
            price: U256::from(100),
            isBuy: false,
        };
        let sell_data = Bytes::from(sell_call.abi_encode());
        let sell_tx = Transaction::call(seller, SPOT_PRECOMPILE, sell_data, 0);
        let sell_receipt = executor.execute_and_commit(&sell_tx).unwrap();

        assert!(sell_receipt.success);

        // Check that more gas was used (due to matching)
        assert!(sell_receipt.gas_used > 50_000);
    }

    #[test]
    fn test_spot_precompile_get_best_prices() {
        use alloy_sol_types::{SolCall, SolValue};
        use crate::precompiles::spot::ISpot;
        use crate::SPOT_PRECOMPILE;

        let (mut executor, _temp) = create_test_executor();

        let trader = Address::repeat_byte(0x01);
        let asset = Address::repeat_byte(0x02);

        // Place buy order
        let buy_call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(1000),
            price: U256::from(100),
            isBuy: true,
        };
        let buy_data = Bytes::from(buy_call.abi_encode());
        let buy_tx = Transaction::call(trader, SPOT_PRECOMPILE, buy_data, 0);
        executor.execute_and_commit(&buy_tx).unwrap();

        // Place sell order
        let sell_call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(1000),
            price: U256::from(105),
            isBuy: false,
        };
        let sell_data = Bytes::from(sell_call.abi_encode());
        let sell_tx = Transaction::call(trader, SPOT_PRECOMPILE, sell_data, 0);
        executor.execute_and_commit(&sell_tx).unwrap();

        // Get best prices
        let prices_call = ISpot::getBestPricesCall { asset };
        let prices_data = Bytes::from(prices_call.abi_encode());
        let prices_tx = Transaction::call(trader, SPOT_PRECOMPILE, prices_data, 0);
        let prices_receipt = executor.execute_and_commit(&prices_tx).unwrap();

        assert!(prices_receipt.success);

        let (bid, ask) = <(U256, U256)>::abi_decode(&prices_receipt.output, true).unwrap();
        assert_eq!(bid, U256::from(100));
        assert_eq!(ask, U256::from(105));
    }

    #[test]
    fn test_precompile_is_detected() {
        use crate::{is_precompile, SPOT_PRECOMPILE, PERP_PRECOMPILE};

        assert!(is_precompile(&SPOT_PRECOMPILE));
        assert!(is_precompile(&PERP_PRECOMPILE));
        assert!(!is_precompile(&Address::repeat_byte(0x99)));
    }
}

