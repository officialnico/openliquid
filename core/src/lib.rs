// OpenLiquid Core DEX Engine
//
// Phase 3.1: Order Book Foundation
// Phase 3.2: Order Book Persistence
//
// This module provides the foundational order book data structures,
// matching engine, and persistence layer for the OpenCore DEX.

pub mod checkpoint;
pub mod history;
pub mod matching;
pub mod orderbook;
pub mod state_machine;
pub mod storage;
pub mod types;

// Re-export commonly used types
pub use checkpoint::CheckpointManager;
pub use history::OrderHistory;
pub use orderbook::{OrderBook, OrderBookSnapshot, PriceLevel};
pub use state_machine::CoreStateMachine;
pub use storage::{CheckpointMetadata, CoreStorage};
pub use types::{AssetId, Fill, Order, OrderId, OrderType, Price, Side, Size};

