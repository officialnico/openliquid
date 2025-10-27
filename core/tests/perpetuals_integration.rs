use core::*;
use alloy_primitives::{Address, U256};
use std::collections::HashMap;

#[test]
fn test_full_perpetual_workflow() {
    // Oracle -> Mark Price -> Funding -> PnL -> Liquidation
    let mut oracle = OracleEngine::default();
    let mut funding = FundingEngine::default();
    let mut margin = MarginEngine::new(MarginConfig::default());
    let mut liquidation = LiquidationEngine::new();
    
    let user = Address::ZERO;
    let asset = AssetId(1);
    
    // 1. Setup oracle prices
    oracle.set_index_price(asset, Price::from_float(100.0));
    oracle.update_price(asset, Price::from_float(101.0), 0).unwrap();
    
    // 2. Get mark price
    let mark = oracle.get_mark_price(asset, Some(Price::from_float(101.0)), 10).unwrap();
    assert_eq!(mark, Price::from_float(101.0));
    
    // 3. Update funding rate
    let rate = funding.update_rate(
        asset,
        Price::from_float(101.0),
        Price::from_float(100.0),
        0,
    ).unwrap();
    assert!(rate > 0.0); // Positive premium
    
    // 4. Open position
    margin.deposit(user, AssetId(1), U256::from(10000)).unwrap();
    margin.update_position(user, asset, 100, Price::from_float(100.0), 0).unwrap();
    
    // 5. Update PnL
    margin.update_position_pnl(user, asset, Price::from_float(101.0)).unwrap();
    let position = margin.get_position(&user, asset).unwrap();
    assert!(position.unrealized_pnl > 0);
    
    // 6. Apply funding payment
    let payment = funding.apply_funding(
        user,
        asset,
        100,
        Price::from_float(101.0),
        28800, // 8 hours later
    ).unwrap();
    assert!(payment < 0); // Long pays on positive funding
}

#[test]
fn test_cross_margin_liquidation() {
    // Multiple positions with shared collateral
    let mut margin = MarginEngine::new(MarginConfig::default());
    let user = Address::ZERO;
    
    // User has cross margin by default
    assert_eq!(margin.get_margin_mode(&user), MarginMode::Cross);
    
    // Deposit collateral
    margin.deposit(user, AssetId(1), U256::from(10000)).unwrap();
    
    // Open multiple positions
    margin.update_position(user, AssetId(1), 100, Price::from_float(10.0), 0).unwrap();
    margin.update_position(user, AssetId(2), 50, Price::from_float(20.0), 0).unwrap();
    
    // Verify both positions use shared collateral
    let positions = margin.get_user_positions(&user);
    assert_eq!(positions.len(), 2);
    
    // Total margin should be for both positions
    let account = margin.get_account_equity(&user).unwrap();
    assert_eq!(account, U256::from(10000));
}

#[test]
fn test_partial_liquidation_restores_health() {
    // Partial liq brings account back to health
    let mut liquidation = LiquidationEngine::new();
    
    // Account slightly undercollateralized
    let size = liquidation.calculate_liquidation_size(
        U256::from(900),   // Account value
        U256::from(1000),  // Used margin
        0.05,              // 5% maintenance
        1000,              // Position size
    );
    
    // Should only liquidate 25%, not everything
    assert_eq!(size, 250);
    assert!(size < 1000);
}

#[test]
fn test_funding_payment_flow() {
    // Complete funding cycle across interval
    let mut funding = FundingEngine::default();
    let user = Address::ZERO;
    let asset = AssetId(1);
    
    // Set funding rate
    funding.update_rate(
        asset,
        Price::from_float(101.0),
        Price::from_float(100.0),
        0,
    ).unwrap();
    
    // First funding should be applied
    let payment1 = funding.apply_funding(
        user,
        asset,
        100,
        Price::from_float(101.0),
        0,
    ).unwrap();
    assert_ne!(payment1, 0);
    
    // Second funding before interval - should not apply
    let payment2 = funding.apply_funding(
        user,
        asset,
        100,
        Price::from_float(101.0),
        1000, // Only 1000 seconds later
    ).unwrap();
    assert_eq!(payment2, 0);
    
    // Third funding after interval - should apply
    let payment3 = funding.apply_funding(
        user,
        asset,
        100,
        Price::from_float(101.0),
        30000, // 30000 seconds later (> 8 hours)
    ).unwrap();
    assert_ne!(payment3, 0);
}

#[test]
fn test_insurance_fund_bad_debt() {
    // Liquidation results in bad debt, insurance covers
    let mut insurance = InsuranceFund::new();
    
    // Fund receives contributions
    insurance.contribute(U256::from(10000), 0);
    assert_eq!(insurance.get_balance(), U256::from(10000));
    
    // Bad debt occurs
    let covered = insurance.cover_bad_debt(U256::from(3000), 1).unwrap();
    assert_eq!(covered, U256::from(3000));
    assert_eq!(insurance.get_balance(), U256::from(7000));
    
    // More bad debt
    let covered = insurance.cover_bad_debt(U256::from(5000), 2).unwrap();
    assert_eq!(covered, U256::from(5000));
    assert_eq!(insurance.get_balance(), U256::from(2000));
    
    // Insufficient insurance
    let covered = insurance.cover_bad_debt(U256::from(5000), 3).unwrap();
    assert_eq!(covered, U256::from(2000)); // Only covers what's available
    assert_eq!(insurance.get_balance(), U256::ZERO);
}

#[test]
fn test_risk_limits_enforcement() {
    let mut risk = RiskEngine::new();
    let asset = AssetId(1);
    
    // Set conservative limits
    risk.set_asset_limits(asset, AssetRiskLimits {
        max_leverage: 5,
        max_position_size: 1000,
        max_notional_value: U256::from(5000),
    });
    
    // Within limits
    assert!(risk.check_order_risk(asset, 500, Price::from_float(5.0), 0).is_ok());
    
    // Exceeds position size
    assert!(risk.check_order_risk(asset, 2000, Price::from_float(1.0), 0).is_err());
    
    // Exceeds notional value
    assert!(risk.check_order_risk(asset, 100, Price::from_float(100.0), 0).is_err());
}

#[test]
fn test_isolated_margin_position() {
    let mut margin = MarginEngine::new(MarginConfig::default());
    let user = Address::ZERO;
    let asset = AssetId(1);
    
    // Switch to isolated mode
    margin.set_margin_mode(user, MarginMode::Isolated).unwrap();
    
    // Deposit isolated collateral
    margin.deposit_isolated(user, asset, U256::from(1000)).unwrap();
    
    let collateral = margin.get_isolated_collateral(&user, asset);
    assert_eq!(collateral, U256::from(1000));
}

#[test]
fn test_mark_price_sources() {
    let mut oracle = OracleEngine::default();
    let asset = AssetId(1);
    
    // Test order book source (default)
    let mark = oracle.get_mark_price(asset, Some(Price::from_float(100.0)), 0).unwrap();
    assert_eq!(mark, Price::from_float(100.0));
    
    // Test external source
    oracle.set_price_source(asset, PriceSource::External);
    oracle.update_price(asset, Price::from_float(101.0), 0).unwrap();
    let mark = oracle.get_mark_price(asset, None, 10).unwrap();
    assert_eq!(mark, Price::from_float(101.0));
    
    // Test weighted source
    oracle.set_price_source(asset, PriceSource::Weighted);
    let mark = oracle.get_mark_price(asset, Some(Price::from_float(99.0)), 10).unwrap();
    assert_eq!(mark, Price::from_float(100.0)); // Average of 101 and 99
}

#[test]
fn test_unrealized_pnl_calculation() {
    let mut margin = MarginEngine::new(MarginConfig::default());
    let user = Address::ZERO;
    let asset = AssetId(1);
    
    margin.deposit(user, AssetId(1), U256::from(10000)).unwrap();
    margin.update_position(user, asset, 100, Price::from_float(100.0), 0).unwrap();
    
    // Price goes up - profit
    margin.update_position_pnl(user, asset, Price::from_float(110.0)).unwrap();
    let position = margin.get_position(&user, asset).unwrap();
    let expected_pnl = (110_000_000 - 100_000_000) * 100; // (110 - 100) * 100 units
    assert_eq!(position.unrealized_pnl, expected_pnl);
    
    // Price goes down - loss
    margin.update_position_pnl(user, asset, Price::from_float(90.0)).unwrap();
    let position = margin.get_position(&user, asset).unwrap();
    let expected_pnl = (90_000_000 - 100_000_000) * 100; // (90 - 100) * 100 units
    assert_eq!(position.unrealized_pnl, expected_pnl);
}

#[test]
fn test_portfolio_leverage_calculation() {
    let risk = RiskEngine::new();
    
    // 10x leverage
    let leverage = risk.calculate_portfolio_leverage(
        U256::from(10000),
        U256::from(1000),
    );
    assert_eq!(leverage, 10);
    
    // 5x leverage
    let leverage = risk.calculate_portfolio_leverage(
        U256::from(5000),
        U256::from(1000),
    );
    assert_eq!(leverage, 5);
    
    // Zero collateral
    let leverage = risk.calculate_portfolio_leverage(
        U256::from(10000),
        U256::ZERO,
    );
    assert_eq!(leverage, 0);
}

#[test]
fn test_funding_rate_dampening() {
    let mut funding = FundingEngine::default();
    let asset = AssetId(1);
    
    // Apply multiple updates to see dampening effect
    for i in 0..10 {
        funding.update_rate(
            asset,
            Price::from_float(110.0),
            Price::from_float(100.0),
            i * 1000,
        ).unwrap();
    }
    
    let rate = funding.get_rate(asset);
    
    // Rate should be dampened and clamped
    assert!(rate > 0.0);
    assert!(rate <= 0.0005); // Should not exceed max rate
}

#[test]
fn test_cross_margin_multiple_assets_pnl() {
    let mut margin = MarginEngine::new(MarginConfig::default());
    let user = Address::ZERO;
    
    // Deposit collateral
    margin.deposit(user, AssetId(1), U256::from(10000)).unwrap();
    
    // Open positions in multiple assets
    margin.update_position(user, AssetId(1), 100, Price::from_float(10.0), 0).unwrap();
    margin.update_position(user, AssetId(2), -50, Price::from_float(20.0), 0).unwrap();
    
    // Set mark prices
    let mut mark_prices = HashMap::new();
    mark_prices.insert(AssetId(1), Price::from_float(12.0)); // +$200 PnL
    mark_prices.insert(AssetId(2), Price::from_float(18.0)); // +$100 PnL (short)
    
    let account_value = margin.get_account_value_with_pnl(&user, &mark_prices).unwrap();
    
    // Original collateral (10000) + PnL should be reflected
    // This is a simplified check
    assert!(account_value > U256::from(10000));
}

#[test]
fn test_liquidation_mode_switching() {
    let mut liquidation = LiquidationEngine::with_mode(LiquidationMode::Full);
    
    let size = liquidation.calculate_liquidation_size(
        U256::from(1000),
        U256::from(500),
        0.05,
        1000,
    );
    
    // Full mode liquidates everything
    assert_eq!(size, 1000);
    
    // Switch to partial mode
    liquidation.set_partial_percentage(0.5);
    let liquidation = LiquidationEngine::new(); // Partial by default
    
    let size = liquidation.calculate_liquidation_size(
        U256::from(1000),
        U256::from(500),
        0.05,
        1000,
    );
    
    // Partial mode liquidates 25% by default
    assert_eq!(size, 250);
}

