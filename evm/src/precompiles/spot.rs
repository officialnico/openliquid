use super::{orderbook::OrderBook, Precompile};
use crate::storage::EvmStorage;
use alloy_primitives::{Address, Bytes, U256};
use alloy_sol_types::{sol, SolCall, SolValue};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::sync::Arc;

// Define Solidity interface using alloy
sol! {
    /// Spot trading interface
    interface ISpot {
        /// Place a limit order
        /// @param asset The asset to trade
        /// @param amount Amount of asset
        /// @param price Price per unit
        /// @param isBuy True for buy, false for sell
        /// @return orderId The ID of the created order
        function placeOrder(
            address asset,
            uint256 amount,
            uint256 price,
            bool isBuy
        ) external returns (uint256 orderId);

        /// Cancel an existing order
        /// @param orderId The order to cancel
        /// @return success True if cancelled successfully
        function cancelOrder(uint256 orderId) external returns (bool success);

        /// Get order details
        /// @param orderId The order ID
        /// @return order The order struct
        function getOrder(uint256 orderId) external view returns (
            uint256 id,
            address user,
            address asset,
            uint256 amount,
            uint256 price,
            bool isBuy,
            uint256 filled
        );

        /// Get best bid and ask for an asset
        /// @param asset The asset
        /// @return bid Best bid price (0 if none)
        /// @return ask Best ask price (0 if none)
        function getBestPrices(address asset) external view returns (
            uint256 bid,
            uint256 ask
        );

        /// Get market depth
        /// @param asset The asset
        /// @param levels Number of price levels
        /// @return bids Array of bid prices
        /// @return bidAmounts Array of bid amounts
        /// @return asks Array of ask prices
        /// @return askAmounts Array of ask amounts
        function getDepth(address asset, uint256 levels) external view returns (
            uint256[] memory bidPrices,
            uint256[] memory bidAmounts,
            uint256[] memory askPrices,
            uint256[] memory askAmounts
        );
    }
}

/// Gas costs for operations
const PLACE_ORDER_BASE_GAS: u64 = 50_000;
const PLACE_ORDER_MATCH_GAS: u64 = 30_000; // Per match
const CANCEL_ORDER_GAS: u64 = 20_000;
const GET_ORDER_GAS: u64 = 5_000;
const GET_BEST_PRICES_GAS: u64 = 3_000;
const GET_DEPTH_GAS: u64 = 10_000;

/// Spot trading precompile
pub struct SpotPrecompile {
    /// Order books per asset
    order_books: HashMap<Address, OrderBook>,
    /// Global order ID to (asset, local_id) mapping
    order_map: HashMap<u64, (Address, u64)>,
    /// Next global order ID
    next_global_id: u64,
    /// Current timestamp
    timestamp: u64,
    /// Storage backend (optional for persistence)
    storage: Option<Arc<EvmStorage>>,
}

impl SpotPrecompile {
    pub fn new() -> Self {
        Self {
            order_books: HashMap::new(),
            order_map: HashMap::new(),
            next_global_id: 1,
            timestamp: 0,
            storage: None,
        }
    }

    /// Create with storage backend for persistence
    pub fn new_with_storage(storage: Arc<EvmStorage>) -> Self {
        Self {
            order_books: HashMap::new(),
            order_map: HashMap::new(),
            next_global_id: 1,
            timestamp: 0,
            storage: Some(storage),
        }
    }

    /// Restore state from storage
    pub fn restore_from_storage(&mut self) -> Result<()> {
        let storage = match &self.storage {
            Some(s) => s,
            None => return Ok(()), // No storage, nothing to restore
        };

        // Load all orders
        let orders = storage.load_all_orders()?;

        for (order_id, order) in orders {
            // Update order map
            self.order_map.insert(order_id, (order.asset, order.id));

            // Get or create book for this asset
            let book = self.get_or_create_book(order.asset);

            // Re-insert order into book
            if order.is_buy {
                book.bids.entry(order.price).or_default().push(order.clone());
            } else {
                book.asks.entry(order.price).or_default().push(order.clone());
            }
            book.orders.insert(order.id, order);
        }

        // Update next_global_id
        if let Some(max_id) = self.order_map.keys().max() {
            self.next_global_id = max_id + 1;
        }

        log::info!("Restored {} orders from storage", self.order_map.len());
        Ok(())
    }

    fn get_or_create_book(&mut self, asset: Address) -> &mut OrderBook {
        self.order_books.entry(asset).or_insert_with(|| OrderBook::new(asset))
    }

    fn place_order_impl(
        &mut self,
        caller: Address,
        asset: Address,
        amount: U256,
        price: U256,
        is_buy: bool,
    ) -> Result<(U256, u64)> {
        // Validate inputs
        if amount == U256::ZERO {
            return Err(anyhow!("Amount must be greater than zero"));
        }
        if price == U256::ZERO {
            return Err(anyhow!("Price must be greater than zero"));
        }

        // Capture timestamp and storage before borrowing book
        let timestamp = self.timestamp;
        let storage = self.storage.clone();

        // Generate global order ID before any borrows
        let global_id = self.next_global_id;
        self.next_global_id += 1;

        // Get order book for asset
        let book = self.get_or_create_book(asset);

        // Place order
        let (local_id, trades) = book.place_order(caller, amount, price, is_buy, timestamp);

        // Calculate gas based on number of trades
        let gas_used = PLACE_ORDER_BASE_GAS + (trades.len() as u64 * PLACE_ORDER_MATCH_GAS);

        // Now we can add to order_map (after book is done being used)
        self.order_map.insert(global_id, (asset, local_id));

        // Persist order if storage is available and order wasn't fully filled
        if let Some(storage) = storage {
            if let Some(order) = self.order_books.get(&asset).and_then(|b| b.get_order(local_id)) {
                storage.store_order(global_id, order)?;
            }
        }

        Ok((U256::from(global_id), gas_used))
    }

    fn cancel_order_impl(&mut self, caller: Address, order_id: U256) -> Result<(bool, u64)> {
        let order_id_u64 = order_id.to::<u64>();

        // Look up order
        let (asset, local_id) = self
            .order_map
            .get(&order_id_u64)
            .copied()
            .ok_or_else(|| anyhow!("Order not found"))?;

        // Get order book
        let book = self
            .order_books
            .get_mut(&asset)
            .ok_or_else(|| anyhow!("Order book not found"))?;

        // Cancel order
        let cancelled = book.cancel_order(local_id, caller);

        if cancelled.is_some() {
            self.order_map.remove(&order_id_u64);
            
            // Delete from storage if available
            if let Some(storage) = &self.storage {
                storage.delete_order(order_id_u64)?;
            }
            
            Ok((true, CANCEL_ORDER_GAS))
        } else {
            Err(anyhow!("Failed to cancel order (not owner or already filled)"))
        }
    }

    fn get_order_impl(&self, order_id: U256) -> Result<(
        U256,
        Address,
        Address,
        U256,
        U256,
        bool,
        U256,
        u64,
    )> {
        let order_id_u64 = order_id.to::<u64>();

        // Look up order
        let (asset, local_id) = self
            .order_map
            .get(&order_id_u64)
            .copied()
            .ok_or_else(|| anyhow!("Order not found"))?;

        // Get order book
        let book = self
            .order_books
            .get(&asset)
            .ok_or_else(|| anyhow!("Order book not found"))?;

        // Get order
        let order = book
            .get_order(local_id)
            .ok_or_else(|| anyhow!("Order not found in book"))?;

        Ok((
            order_id,
            order.user,
            order.asset,
            order.amount,
            order.price,
            order.is_buy,
            order.filled,
            GET_ORDER_GAS,
        ))
    }

    fn get_best_prices_impl(&self, asset: Address) -> Result<(U256, U256, u64)> {
        let book = self.order_books.get(&asset);

        let bid = book.and_then(|b| b.best_bid()).unwrap_or(U256::ZERO);
        let ask = book.and_then(|b| b.best_ask()).unwrap_or(U256::ZERO);

        Ok((bid, ask, GET_BEST_PRICES_GAS))
    }

    fn get_depth_impl(
        &self,
        asset: Address,
        levels: U256,
    ) -> Result<(Vec<U256>, Vec<U256>, Vec<U256>, Vec<U256>, u64)> {
        let levels_usize = levels.to::<usize>().min(100); // Cap at 100 levels

        let book = self.order_books.get(&asset);

        let (bids, asks) = if let Some(book) = book {
            book.get_depth(levels_usize)
        } else {
            (Vec::new(), Vec::new())
        };

        let bid_prices: Vec<U256> = bids.iter().map(|(p, _)| *p).collect();
        let bid_amounts: Vec<U256> = bids.iter().map(|(_, a)| *a).collect();
        let ask_prices: Vec<U256> = asks.iter().map(|(p, _)| *p).collect();
        let ask_amounts: Vec<U256> = asks.iter().map(|(_, a)| *a).collect();

        Ok((
            bid_prices,
            bid_amounts,
            ask_prices,
            ask_amounts,
            GET_DEPTH_GAS,
        ))
    }
}

impl Precompile for SpotPrecompile {
    fn call(&mut self, input: &Bytes, gas_limit: u64, caller: Address) -> Result<(Bytes, u64)> {
        if input.len() < 4 {
            return Err(anyhow!("Input too short"));
        }

        // Increment timestamp for each call
        self.timestamp += 1;

        // Extract function selector (first 4 bytes)
        let selector = &input[..4];

        // Route based on selector
        match selector {
            // placeOrder(address,uint256,uint256,bool)
            sel if sel == ISpot::placeOrderCall::SELECTOR => {
                let call = ISpot::placeOrderCall::abi_decode(input, false)?;
                let (order_id, gas) =
                    self.place_order_impl(caller, call.asset, call.amount, call.price, call.isBuy)?;

                if gas > gas_limit {
                    return Err(anyhow!("Out of gas"));
                }

                Ok((Bytes::from(order_id.abi_encode()), gas))
            }

            // cancelOrder(uint256)
            sel if sel == ISpot::cancelOrderCall::SELECTOR => {
                let call = ISpot::cancelOrderCall::abi_decode(input, false)?;
                let (success, gas) = self.cancel_order_impl(caller, call.orderId)?;

                if gas > gas_limit {
                    return Err(anyhow!("Out of gas"));
                }

                Ok((Bytes::from(success.abi_encode()), gas))
            }

            // getOrder(uint256)
            sel if sel == ISpot::getOrderCall::SELECTOR => {
                let call = ISpot::getOrderCall::abi_decode(input, false)?;
                let (id, user, asset, amount, price, is_buy, filled, gas) =
                    self.get_order_impl(call.orderId)?;

                if gas > gas_limit {
                    return Err(anyhow!("Out of gas"));
                }

                let result = (id, user, asset, amount, price, is_buy, filled);
                Ok((Bytes::from(result.abi_encode()), gas))
            }

            // getBestPrices(address)
            sel if sel == ISpot::getBestPricesCall::SELECTOR => {
                let call = ISpot::getBestPricesCall::abi_decode(input, false)?;
                let (bid, ask, gas) = self.get_best_prices_impl(call.asset)?;

                if gas > gas_limit {
                    return Err(anyhow!("Out of gas"));
                }

                let result = (bid, ask);
                Ok((Bytes::from(result.abi_encode()), gas))
            }

            // getDepth(address,uint256)
            sel if sel == ISpot::getDepthCall::SELECTOR => {
                let call = ISpot::getDepthCall::abi_decode(input, false)?;
                let (bid_prices, bid_amounts, ask_prices, ask_amounts, gas) =
                    self.get_depth_impl(call.asset, call.levels)?;

                if gas > gas_limit {
                    return Err(anyhow!("Out of gas"));
                }

                let result = (bid_prices, bid_amounts, ask_prices, ask_amounts);
                Ok((Bytes::from(result.abi_encode()), gas))
            }

            _ => Err(anyhow!("Unknown function selector: {:?}", selector)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_place_order() {
        let mut precompile = SpotPrecompile::new();
        let caller = Address::repeat_byte(0x01);
        let asset = Address::repeat_byte(0x02);

        // Encode placeOrder call
        let call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(1000),
            price: U256::from(100),
            isBuy: true,
        };
        let input = Bytes::from(call.abi_encode());

        let (output, gas) = precompile.call(&input, 1_000_000, caller).unwrap();

        assert!(gas > 0);
        assert!(gas <= PLACE_ORDER_BASE_GAS);

        // Decode order ID
        let order_id = U256::abi_decode(&output, true).unwrap();
        assert_eq!(order_id, U256::from(1));
    }

    #[test]
    fn test_place_and_match_orders() {
        let mut precompile = SpotPrecompile::new();
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
        let buy_input = Bytes::from(buy_call.abi_encode());
        precompile.call(&buy_input, 1_000_000, buyer).unwrap();

        // Place sell order (should match)
        let sell_call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(1000),
            price: U256::from(100),
            isBuy: false,
        };
        let sell_input = Bytes::from(sell_call.abi_encode());
        let (_, gas) = precompile.call(&sell_input, 1_000_000, seller).unwrap();

        // Should use extra gas for matching
        assert!(gas > PLACE_ORDER_BASE_GAS);
    }

    #[test]
    fn test_cancel_order() {
        let mut precompile = SpotPrecompile::new();
        let caller = Address::repeat_byte(0x01);
        let asset = Address::repeat_byte(0x02);

        // Place order
        let place_call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(1000),
            price: U256::from(100),
            isBuy: true,
        };
        let place_input = Bytes::from([place_call.abi_encode()].concat());
        let (output, _) = precompile.call(&place_input, 1_000_000, caller).unwrap();
        let order_id = U256::abi_decode(&output, true).unwrap();

        // Cancel order
        let cancel_call = ISpot::cancelOrderCall { orderId: order_id };
        let cancel_input = Bytes::from(cancel_call.abi_encode());
        let (output, gas) = precompile.call(&cancel_input, 1_000_000, caller).unwrap();

        let success = bool::abi_decode(&output, true).unwrap();
        assert!(success);
        assert_eq!(gas, CANCEL_ORDER_GAS);
    }

    #[test]
    fn test_get_order() {
        let mut precompile = SpotPrecompile::new();
        let caller = Address::repeat_byte(0x01);
        let asset = Address::repeat_byte(0x02);

        // Place order
        let place_call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(1000),
            price: U256::from(100),
            isBuy: true,
        };
        let place_input = Bytes::from([place_call.abi_encode()].concat());
        let (output, _) = precompile.call(&place_input, 1_000_000, caller).unwrap();
        let order_id = U256::abi_decode(&output, true).unwrap();

        // Get order
        let get_call = ISpot::getOrderCall { orderId: order_id };
        let get_input = Bytes::from(get_call.abi_encode());
        let (output, _) = precompile.call(&get_input, 1_000_000, caller).unwrap();

        let (id, user, order_asset, amount, price, is_buy, filled) =
            <(U256, Address, Address, U256, U256, bool, U256)>::abi_decode(&output, true)
                .unwrap();

        assert_eq!(id, order_id);
        assert_eq!(user, caller);
        assert_eq!(order_asset, asset);
        assert_eq!(amount, U256::from(1000));
        assert_eq!(price, U256::from(100));
        assert!(is_buy);
        assert_eq!(filled, U256::ZERO);
    }

    #[test]
    fn test_get_best_prices() {
        let mut precompile = SpotPrecompile::new();
        let caller = Address::repeat_byte(0x01);
        let asset = Address::repeat_byte(0x02);

        // Place buy order at 100
        let buy_call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(1000),
            price: U256::from(100),
            isBuy: true,
        };
        let buy_input = Bytes::from(buy_call.abi_encode());
        precompile.call(&buy_input, 1_000_000, caller).unwrap();

        // Place sell order at 105
        let sell_call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(1000),
            price: U256::from(105),
            isBuy: false,
        };
        let sell_input = Bytes::from(sell_call.abi_encode());
        precompile.call(&sell_input, 1_000_000, caller).unwrap();

        // Get best prices
        let prices_call = ISpot::getBestPricesCall { asset };
        let prices_input = Bytes::from(prices_call.abi_encode());
        let (output, _) = precompile.call(&prices_input, 1_000_000, caller).unwrap();

        let (bid, ask) = <(U256, U256)>::abi_decode(&output, true).unwrap();
        assert_eq!(bid, U256::from(100));
        assert_eq!(ask, U256::from(105));
    }

    #[test]
    fn test_zero_amount_fails() {
        let mut precompile = SpotPrecompile::new();
        let caller = Address::repeat_byte(0x01);
        let asset = Address::repeat_byte(0x02);

        let call = ISpot::placeOrderCall {
            asset,
            amount: U256::ZERO,
            price: U256::from(100),
            isBuy: true,
        };
        let input = Bytes::from([call.abi_encode()].concat());

        let result = precompile.call(&input, 1_000_000, caller);
        assert!(result.is_err());
    }
}

