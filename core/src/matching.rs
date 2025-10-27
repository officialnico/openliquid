use crate::orderbook::OrderBook;
use crate::types::*;
use alloy_primitives::{Address, U256};
use anyhow::Result;

/// Matching engine for executing orders
pub struct MatchingEngine;

impl MatchingEngine {
    /// Execute a market order against the book
    pub fn execute_market_order(
        book: &mut OrderBook,
        trader: Address,
        side: Side,
        mut size: Size,
        timestamp: u64,
    ) -> Result<Vec<Fill>> {
        let mut fills = Vec::new();
        
        // Market buy: match against asks (ascending)
        // Market sell: match against bids (descending)
        match side {
            Side::Bid => {
                // Buy: take from asks
                while size.0 > U256::ZERO {
                    let best_ask = match book.best_ask() {
                        Some(price) => price,
                        None => break, // No more liquidity
                    };
                    
                    let new_fills = Self::match_at_price(
                        book,
                        best_ask,
                        Side::Ask,
                        &mut size,
                        trader,
                        timestamp,
                    )?;
                    
                    fills.extend(new_fills);
                    
                    if size.0 == U256::ZERO {
                        break;
                    }
                }
            }
            Side::Ask => {
                // Sell: take from bids
                while size.0 > U256::ZERO {
                    let best_bid = match book.best_bid() {
                        Some(price) => price,
                        None => break,
                    };
                    
                    let new_fills = Self::match_at_price(
                        book,
                        best_bid,
                        Side::Bid,
                        &mut size,
                        trader,
                        timestamp,
                    )?;
                    
                    fills.extend(new_fills);
                    
                    if size.0 == U256::ZERO {
                        break;
                    }
                }
            }
        }
        
        Ok(fills)
    }
    
    /// Match against a specific price level
    fn match_at_price(
        book: &mut OrderBook,
        price: Price,
        side: Side,
        remaining: &mut Size,
        taker: Address,
        timestamp: u64,
    ) -> Result<Vec<Fill>> {
        let mut fills = Vec::new();
        let mut orders_to_remove = Vec::new();
        
        // Scope the mutable borrow of the tree
        {
            // Get the appropriate tree
            let tree = match side {
                Side::Bid => book.bids_mut(),
                Side::Ask => book.asks_mut(),
            };
            
            // Get the price level
            let level = match tree.get_mut(&price) {
                Some(l) => l,
                None => return Ok(fills),
            };
            
            // Process orders in FIFO order
            for order in level.orders.iter_mut() {
                if remaining.0 == U256::ZERO {
                    break;
                }
                
                let order_remaining = order.remaining();
                let fill_size = if remaining.0 < order_remaining.0 {
                    remaining.0
                } else {
                    order_remaining.0
                };
                
                // Create fill
                let fill = Fill {
                    order_id: order.id,
                    price,
                    size: Size(fill_size),
                    maker: order.trader,
                    taker,
                    timestamp,
                };
                
                // Update order
                order.filled.0 += fill_size;
                
                // Update remaining
                remaining.0 -= fill_size;
                
                // Update level total size
                level.total_size -= fill_size;
                
                fills.push(fill);
                
                // Mark fully filled orders for removal
                if order.is_filled() {
                    orders_to_remove.push(order.id);
                }
            }
            
            // Remove from the level's queue
            level.orders.retain(|o| !orders_to_remove.contains(&o.id));
        }
        
        // Now we can safely access order_index_mut
        // Remove filled orders from the order index
        for order_id in &orders_to_remove {
            book.order_index_mut().remove(order_id);
        }
        
        // Clean up empty price level
        let should_remove = {
            let tree = match side {
                Side::Bid => book.bids_mut(),
                Side::Ask => book.asks_mut(),
            };
            tree.get(&price).map_or(false, |l| l.is_empty())
        };
        
        if should_remove {
            let tree = match side {
                Side::Bid => book.bids_mut(),
                Side::Ask => book.asks_mut(),
            };
            tree.remove(&price);
        }
        
        Ok(fills)
    }
    
    /// Execute a limit order (add to book + match if crosses)
    pub fn execute_limit_order(
        book: &mut OrderBook,
        trader: Address,
        side: Side,
        price: Price,
        size: Size,
        timestamp: u64,
    ) -> Result<(OrderId, Vec<Fill>)> {
        let mut fills = Vec::new();
        let mut remaining = size;
        
        // Check if order crosses the spread (can be filled immediately)
        let mut crosses = match side {
            Side::Bid => {
                // Bid crosses if price >= best ask
                book.best_ask().map_or(false, |ask| price.0 >= ask.0)
            }
            Side::Ask => {
                // Ask crosses if price <= best bid
                book.best_bid().map_or(false, |bid| price.0 <= bid.0)
            }
        };
        
        // Match as much as possible at better prices
        while crosses && remaining.0 > U256::ZERO {
            let match_price = match side {
                Side::Bid => {
                    if let Some(ask) = book.best_ask() {
                        if price.0 >= ask.0 {
                            ask
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                Side::Ask => {
                    if let Some(bid) = book.best_bid() {
                        if price.0 <= bid.0 {
                            bid
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            };
            
            let opposite_side = match side {
                Side::Bid => Side::Ask,
                Side::Ask => Side::Bid,
            };
            
            let new_fills = Self::match_at_price(
                book,
                match_price,
                opposite_side,
                &mut remaining,
                trader,
                timestamp,
            )?;
            
            fills.extend(new_fills);
            
            // Check if still crosses
            crosses = match side {
                Side::Bid => book.best_ask().map_or(false, |ask| price.0 >= ask.0),
                Side::Ask => book.best_bid().map_or(false, |bid| price.0 <= bid.0),
            };
        }
        
        // Add remaining size to book if any
        let order_id = if remaining.0 > U256::ZERO {
            book.add_limit_order(trader, side, price, remaining, timestamp)
        } else {
            // Order was fully filled, generate ID but don't add to book
            let id = book.next_order_id;
            book.next_order_id += 1;
            id
        };
        
        Ok((order_id, fills))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orderbook::OrderBook;

    fn setup_book_with_liquidity() -> OrderBook {
        let mut book = OrderBook::new(AssetId(1));
        
        // Add some bid liquidity
        book.add_limit_order(
            Address::from([1u8; 20]),
            Side::Bid,
            Price(1_000_000),
            Size(U256::from(100)),
            0,
        );
        book.add_limit_order(
            Address::from([2u8; 20]),
            Side::Bid,
            Price(990_000),
            Size(U256::from(200)),
            1,
        );
        
        // Add some ask liquidity
        book.add_limit_order(
            Address::from([3u8; 20]),
            Side::Ask,
            Price(1_010_000),
            Size(U256::from(150)),
            2,
        );
        book.add_limit_order(
            Address::from([4u8; 20]),
            Side::Ask,
            Price(1_020_000),
            Size(U256::from(250)),
            3,
        );
        
        book
    }

    #[test]
    fn test_market_buy_single_level() {
        let mut book = setup_book_with_liquidity();
        let taker = Address::from([10u8; 20]);
        
        let fills = MatchingEngine::execute_market_order(
            &mut book,
            taker,
            Side::Bid,
            Size(U256::from(100)),
            100,
        )
        .unwrap();
        
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].price, Price(1_010_000));
        assert_eq!(fills[0].size.0, U256::from(100));
        assert_eq!(fills[0].taker, taker);
        
        // Best ask should still be at 1.01 with reduced size
        assert_eq!(book.best_ask(), Some(Price(1_010_000)));
        assert_eq!(book.depth_at_price(Price(1_010_000), Side::Ask), U256::from(50));
    }

    #[test]
    fn test_market_buy_multiple_levels() {
        let mut book = setup_book_with_liquidity();
        let taker = Address::from([10u8; 20]);
        
        let fills = MatchingEngine::execute_market_order(
            &mut book,
            taker,
            Side::Bid,
            Size(U256::from(200)),
            100,
        )
        .unwrap();
        
        // Should fill against both ask levels
        assert_eq!(fills.len(), 2);
        assert_eq!(fills[0].price, Price(1_010_000));
        assert_eq!(fills[0].size.0, U256::from(150));
        assert_eq!(fills[1].price, Price(1_020_000));
        assert_eq!(fills[1].size.0, U256::from(50));
        
        // First ask level should be gone
        assert_eq!(book.best_ask(), Some(Price(1_020_000)));
        assert_eq!(book.depth_at_price(Price(1_020_000), Side::Ask), U256::from(200));
    }

    #[test]
    fn test_market_sell_single_level() {
        let mut book = setup_book_with_liquidity();
        let taker = Address::from([10u8; 20]);
        
        let fills = MatchingEngine::execute_market_order(
            &mut book,
            taker,
            Side::Ask,
            Size(U256::from(50)),
            100,
        )
        .unwrap();
        
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].price, Price(1_000_000));
        assert_eq!(fills[0].size.0, U256::from(50));
        
        assert_eq!(book.depth_at_price(Price(1_000_000), Side::Bid), U256::from(50));
    }

    #[test]
    fn test_market_order_exceeds_liquidity() {
        let mut book = setup_book_with_liquidity();
        let taker = Address::from([10u8; 20]);
        
        let fills = MatchingEngine::execute_market_order(
            &mut book,
            taker,
            Side::Bid,
            Size(U256::from(1000)),
            100,
        )
        .unwrap();
        
        // Should fill all available liquidity
        assert_eq!(fills.len(), 2);
        let total_filled: U256 = fills.iter().map(|f| f.size.0).sum();
        assert_eq!(total_filled, U256::from(400)); // 150 + 250
        
        // No more asks
        assert_eq!(book.best_ask(), None);
    }

    #[test]
    fn test_limit_order_no_cross() {
        let mut book = setup_book_with_liquidity();
        let trader = Address::from([10u8; 20]);
        
        let (order_id, fills) = MatchingEngine::execute_limit_order(
            &mut book,
            trader,
            Side::Bid,
            Price(1_005_000),
            Size(U256::from(100)),
            100,
        )
        .unwrap();
        
        // Should not fill, just add to book
        assert_eq!(fills.len(), 0);
        assert!(order_id > 0);
        
        // Should improve best bid
        assert_eq!(book.best_bid(), Some(Price(1_005_000)));
    }

    #[test]
    fn test_limit_order_crosses_spread() {
        let mut book = setup_book_with_liquidity();
        let trader = Address::from([10u8; 20]);
        
        let (order_id, fills) = MatchingEngine::execute_limit_order(
            &mut book,
            trader,
            Side::Bid,
            Price(1_015_000),  // Above best ask but below second ask
            Size(U256::from(200)),
            100,
        )
        .unwrap();
        
        // Should fill against first ask level only (1.01)
        // Second ask at 1.02 is above our limit price of 1.015
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].price, Price(1_010_000));
        assert_eq!(fills[0].size.0, U256::from(150));
        
        // Remaining 50 should be in the book as a limit order
        assert!(order_id > 0);
        assert_eq!(book.best_bid(), Some(Price(1_015_000)));
        assert_eq!(book.depth_at_price(Price(1_015_000), Side::Bid), U256::from(50));
    }

    #[test]
    fn test_limit_order_partial_fill() {
        let mut book = setup_book_with_liquidity();
        let trader = Address::from([10u8; 20]);
        
        let (order_id, fills) = MatchingEngine::execute_limit_order(
            &mut book,
            trader,
            Side::Bid,
            Price(1_010_000),  // Matches best ask
            Size(U256::from(200)),
            100,
        )
        .unwrap();
        
        // Should fill 150 from first ask level
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].size.0, U256::from(150));
        
        // Remaining 50 should be added to book
        assert_eq!(book.best_bid(), Some(Price(1_010_000)));
        assert_eq!(book.depth_at_price(Price(1_010_000), Side::Bid), U256::from(50));
        
        assert!(order_id > 0);
    }

    #[test]
    fn test_limit_order_full_fill() {
        let mut book = setup_book_with_liquidity();
        let trader = Address::from([10u8; 20]);
        
        let (order_id, fills) = MatchingEngine::execute_limit_order(
            &mut book,
            trader,
            Side::Bid,
            Price(1_010_000),
            Size(U256::from(150)),
            100,
        )
        .unwrap();
        
        // Should fully fill
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].size.0, U256::from(150));
        
        // Nothing added to book
        assert_eq!(book.best_bid(), Some(Price(1_000_000)));
        
        assert!(order_id > 0);
    }

    #[test]
    fn test_fifo_order_priority() {
        let mut book = OrderBook::new(AssetId(1));
        
        // Add three orders at same price
        book.add_limit_order(
            Address::from([1u8; 20]),
            Side::Ask,
            Price(1_000_000),
            Size(U256::from(100)),
            0,
        );
        book.add_limit_order(
            Address::from([2u8; 20]),
            Side::Ask,
            Price(1_000_000),
            Size(U256::from(100)),
            1,
        );
        book.add_limit_order(
            Address::from([3u8; 20]),
            Side::Ask,
            Price(1_000_000),
            Size(U256::from(100)),
            2,
        );
        
        // Market buy should fill in FIFO order
        let fills = MatchingEngine::execute_market_order(
            &mut book,
            Address::from([10u8; 20]),
            Side::Bid,
            Size(U256::from(150)),
            100,
        )
        .unwrap();
        
        assert_eq!(fills.len(), 2);
        assert_eq!(fills[0].maker, Address::from([1u8; 20]));
        assert_eq!(fills[0].size.0, U256::from(100));
        assert_eq!(fills[1].maker, Address::from([2u8; 20]));
        assert_eq!(fills[1].size.0, U256::from(50));
    }

    #[test]
    fn test_partial_order_fill() {
        let mut book = OrderBook::new(AssetId(1));
        
        book.add_limit_order(
            Address::from([1u8; 20]),
            Side::Ask,
            Price(1_000_000),
            Size(U256::from(100)),
            0,
        );
        
        // Fill only part of the order
        let fills = MatchingEngine::execute_market_order(
            &mut book,
            Address::from([10u8; 20]),
            Side::Bid,
            Size(U256::from(50)),
            100,
        )
        .unwrap();
        
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].size.0, U256::from(50));
        
        // Order should still be in book with reduced size
        assert_eq!(book.depth_at_price(Price(1_000_000), Side::Ask), U256::from(50));
    }

    #[test]
    fn test_empty_book_market_order() {
        let mut book = OrderBook::new(AssetId(1));
        
        let fills = MatchingEngine::execute_market_order(
            &mut book,
            Address::from([10u8; 20]),
            Side::Bid,
            Size(U256::from(100)),
            100,
        )
        .unwrap();
        
        assert_eq!(fills.len(), 0);
    }

    #[test]
    fn test_limit_order_empty_book() {
        let mut book = OrderBook::new(AssetId(1));
        
        let (order_id, fills) = MatchingEngine::execute_limit_order(
            &mut book,
            Address::from([10u8; 20]),
            Side::Bid,
            Price(1_000_000),
            Size(U256::from(100)),
            100,
        )
        .unwrap();
        
        assert_eq!(fills.len(), 0);
        assert!(order_id > 0);
        assert_eq!(book.best_bid(), Some(Price(1_000_000)));
    }
}

