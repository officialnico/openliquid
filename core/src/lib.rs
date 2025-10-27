// OpenLiquid Core DEX Engine
//
// Phase 3.1: Order Book Foundation
// Phase 3.2: Order Book Persistence
// Phase 3.3: Margin Trading & Liquidations
// Phase 3.4: Advanced Margin & Perpetuals
// Phase 3.5: Advanced Trading Features & Optimizations
// Phase 3.6: Order Book Optimizations & Position Management
// Phase 3.7: Market Making & Liquidity Infrastructure
//
// This module provides the foundational order book data structures,
// matching engine, persistence layer, advanced perpetual futures
// functionality, and market making infrastructure for the OpenCore DEX.

pub mod adl;
pub mod analytics;
pub mod batch;
pub mod checkpoint;
pub mod fees;
pub mod funding;
pub mod grid_strategy;
pub mod history;
pub mod insurance;
pub mod liquidation;
pub mod liquidity_pool;
pub mod margin;
pub mod matching;
pub mod mm_analytics;
pub mod oracle;
pub mod orders;
pub mod orderbook;
pub mod position_manager;
pub mod price_protection;
pub mod quote_manager;
pub mod rebate;
pub mod risk;
pub mod state_machine;
pub mod storage;
pub mod types;
pub mod vault;

// Re-export commonly used types
pub use adl::{ADLCandidate, ADLEngine};
pub use analytics::{Analytics, AssetStats, UserStats};
pub use batch::{
    BatchCancelRequest, BatchCancelResult, BatchOperations, BatchOrderBuilder, BatchOrderRequest,
    BatchResult, OrderRequest,
};
pub use checkpoint::CheckpointManager;
pub use fees::{FeeConfig, FeeEngine, FeeTier};
pub use funding::{FundingConfig, FundingEngine, FundingPayment};
pub use grid_strategy::{GridConfig, GridStats, GridStrategy, GridStrategyManager};
pub use history::OrderHistory;
pub use insurance::InsuranceFund;
pub use liquidation::{LiquidationEngine, LiquidationMode};
pub use liquidity_pool::{LPToken, LiquidityPool, PoolId, PoolManager};
pub use margin::{MarginConfig, MarginEngine, MarginMode};
pub use matching::MatchingEngine;
pub use mm_analytics::{AggregateStats, MMAnalytics, MMPerformanceMetrics, TradeRecord};
pub use oracle::{OracleConfig, OracleEngine, PriceSource};
pub use orders::{AdvancedOrder, AdvancedOrderType, LimitOrderParams, OrderManager, TimeInForce};
pub use orderbook::{OrderBook, OrderBookCache, OrderBookSnapshot, PriceLevel};
pub use position_manager::{ManagedPosition, PositionId, PositionManager};
pub use price_protection::{PriceProtection, PriceProtectionConfig};
pub use quote_manager::{Quote, QuoteConfig, QuoteManager};
pub use rebate::{RebateEngine, RebateTier, VolumeStats};
pub use risk::{AssetRiskLimits, LeverageTier, PortfolioRiskLimits, RiskEngine};
pub use state_machine::CoreStateMachine;
pub use storage::{CheckpointMetadata, CoreStorage};
pub use types::{
    AssetId, CollateralAccount, Fill, Liquidation, MarginRequirements, Order, OrderId, OrderType,
    Position, Price, Side, Size,
};
pub use vault::{MMVault, VaultId, VaultManager, VaultStrategy};

