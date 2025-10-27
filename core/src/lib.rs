// OpenLiquid Core DEX Engine
//
// Phase 3.1: Order Book Foundation
//
// This module provides the foundational order book data structures
// and matching engine for the OpenCore DEX.

pub mod matching;
pub mod orderbook;
pub mod state_machine;
pub mod types;

// Re-export commonly used types
pub use orderbook::{OrderBook, OrderBookSnapshot, PriceLevel};
pub use state_machine::CoreStateMachine;
pub use types::{AssetId, Fill, Order, OrderId, OrderType, Price, Side, Size};

