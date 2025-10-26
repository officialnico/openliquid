use super::*;
use alloy_primitives::{Address, Bytes, I256, U256};
use alloy_sol_types::{SolCall, SolValue};

// Re-import the interfaces
use crate::precompiles::perp::IPerp;
use crate::precompiles::spot::ISpot;

#[test]
fn test_precompile_addresses() {
    assert_eq!(
        SPOT_PRECOMPILE,
        Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1])
    );
    assert_eq!(
        PERP_PRECOMPILE,
        Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2])
    );
}

#[test]
fn test_is_precompile() {
    assert!(is_precompile(&SPOT_PRECOMPILE));
    assert!(is_precompile(&PERP_PRECOMPILE));
    assert!(!is_precompile(&Address::repeat_byte(0xff)));
}

#[test]
fn test_get_precompile_spot() {
    let precompile = get_precompile(&SPOT_PRECOMPILE);
    assert!(precompile.is_some());
}

#[test]
fn test_get_precompile_perp() {
    let precompile = get_precompile(&PERP_PRECOMPILE);
    assert!(precompile.is_some());
}

#[test]
fn test_get_precompile_invalid() {
    let precompile = get_precompile(&Address::repeat_byte(0xff));
    assert!(precompile.is_none());
}

// Spot trading integration tests

#[test]
fn test_spot_full_trading_flow() {
    let mut precompile = spot::SpotPrecompile::new();
    let trader1 = Address::repeat_byte(0x01);
    let trader2 = Address::repeat_byte(0x02);
    let asset = Address::repeat_byte(0x03);

    // Trader 1 places a buy order at 100
    let buy_call = ISpot::placeOrderCall {
        asset,
        amount: U256::from(1000),
        price: U256::from(100),
        isBuy: true,
    };
    let buy_input = Bytes::from(buy_call.abi_encode());
    let (buy_output, _) = precompile.call(&buy_input, 1_000_000, trader1).unwrap();
    let buy_order_id = U256::abi_decode(&buy_output, true).unwrap();

    // Check order exists
    let get_call = ISpot::getOrderCall {
        orderId: buy_order_id,
    };
    let get_input = Bytes::from(get_call.abi_encode());
    let (get_output, _) = precompile.call(&get_input, 1_000_000, trader1).unwrap();
    let (id, user, _, amount, price, is_buy, filled) =
        <(U256, Address, Address, U256, U256, bool, U256)>::abi_decode(&get_output, true)
            .unwrap();
    assert_eq!(id, buy_order_id);
    assert_eq!(user, trader1);
    assert_eq!(amount, U256::from(1000));
    assert_eq!(price, U256::from(100));
    assert!(is_buy);
    assert_eq!(filled, U256::ZERO);

    // Trader 2 places a sell order at 100 (should match)
    let sell_call = ISpot::placeOrderCall {
        asset,
        amount: U256::from(1000),
        price: U256::from(100),
        isBuy: false,
    };
    let sell_input = Bytes::from(sell_call.abi_encode());
    precompile.call(&sell_input, 1_000_000, trader2).unwrap();

    // Check best prices (should be empty after match)
    let prices_call = ISpot::getBestPricesCall { asset };
    let prices_input = Bytes::from(prices_call.abi_encode());
    let (prices_output, _) = precompile.call(&prices_input, 1_000_000, trader1).unwrap();
    let (bid, ask) = <(U256, U256)>::abi_decode(&prices_output, true).unwrap();
    assert_eq!(bid, U256::ZERO);
    assert_eq!(ask, U256::ZERO);
}

#[test]
fn test_spot_partial_fill() {
    let mut precompile = spot::SpotPrecompile::new();
    let trader1 = Address::repeat_byte(0x01);
    let trader2 = Address::repeat_byte(0x02);
    let trader3 = Address::repeat_byte(0x03);
    let asset = Address::repeat_byte(0x04);

    // Place large buy order
    let buy_call = ISpot::placeOrderCall {
        asset,
        amount: U256::from(10000),
        price: U256::from(100),
        isBuy: true,
    };
    let buy_input = Bytes::from(buy_call.abi_encode());
    precompile.call(&buy_input, 1_000_000, trader1).unwrap();

    // Partial fill with smaller sell
    let sell1_call = ISpot::placeOrderCall {
        asset,
        amount: U256::from(3000),
        price: U256::from(100),
        isBuy: false,
    };
    let sell1_input = Bytes::from(sell1_call.abi_encode());
    precompile.call(&sell1_input, 1_000_000, trader2).unwrap();

    // Another partial fill
    let sell2_call = ISpot::placeOrderCall {
        asset,
        amount: U256::from(2000),
        price: U256::from(100),
        isBuy: false,
    };
    let sell2_input = Bytes::from(sell2_call.abi_encode());
    precompile.call(&sell2_input, 1_000_000, trader3).unwrap();

    // Check remaining order book
    let depth_call = ISpot::getDepthCall {
        asset,
        levels: U256::from(5),
    };
    let depth_input = Bytes::from(depth_call.abi_encode());
    let (depth_output, _) = precompile.call(&depth_input, 1_000_000, trader1).unwrap();
    let (bid_prices, bid_amounts, ask_prices, ask_amounts) =
        <(Vec<U256>, Vec<U256>, Vec<U256>, Vec<U256>)>::abi_decode(&depth_output, true).unwrap();

    // Should have remaining buy order of 5000
    assert_eq!(bid_prices.len(), 1);
    assert_eq!(bid_amounts[0], U256::from(5000));
    assert_eq!(ask_prices.len(), 0);
    assert_eq!(ask_amounts.len(), 0);
}

#[test]
fn test_spot_order_cancellation() {
    let mut precompile = spot::SpotPrecompile::new();
    let trader = Address::repeat_byte(0x01);
    let other = Address::repeat_byte(0x02);
    let asset = Address::repeat_byte(0x03);

    // Place order
    let place_call = ISpot::placeOrderCall {
        asset,
        amount: U256::from(1000),
        price: U256::from(100),
        isBuy: true,
    };
    let place_input = Bytes::from(place_call.abi_encode());
    let (place_output, _) = precompile.call(&place_input, 1_000_000, trader).unwrap();
    let order_id = U256::abi_decode(&place_output, true).unwrap();

    // Try to cancel with wrong trader (should fail)
    let cancel_call = ISpot::cancelOrderCall { orderId: order_id };
    let cancel_input = Bytes::from(cancel_call.abi_encode());
    let result = precompile.call(&cancel_input, 1_000_000, other);
    assert!(result.is_err());

    // Cancel with correct trader (should succeed)
    let (cancel_output, _) = precompile.call(&cancel_input, 1_000_000, trader).unwrap();
    let success = bool::abi_decode(&cancel_output, true).unwrap();
    assert!(success);
}

#[test]
fn test_spot_market_depth() {
    let mut precompile = spot::SpotPrecompile::new();
    let trader = Address::repeat_byte(0x01);
    let asset = Address::repeat_byte(0x02);

    // Place multiple orders at different prices
    for i in 1..=5 {
        let buy_call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(100 * i),
            price: U256::from(100 - i),
            isBuy: true,
        };
        let buy_input = Bytes::from(buy_call.abi_encode());
        precompile.call(&buy_input, 1_000_000, trader).unwrap();

        let sell_call = ISpot::placeOrderCall {
            asset,
            amount: U256::from(100 * i),
            price: U256::from(100 + i),
            isBuy: false,
        };
        let sell_input = Bytes::from(sell_call.abi_encode());
        precompile.call(&sell_input, 1_000_000, trader).unwrap();
    }

    // Get market depth
    let depth_call = ISpot::getDepthCall {
        asset,
        levels: U256::from(3),
    };
    let depth_input = Bytes::from(depth_call.abi_encode());
    let (depth_output, _) = precompile.call(&depth_input, 1_000_000, trader).unwrap();
    let (bid_prices, bid_amounts, ask_prices, ask_amounts) =
        <(Vec<U256>, Vec<U256>, Vec<U256>, Vec<U256>)>::abi_decode(&depth_output, true).unwrap();

    // Should have 3 levels on each side
    assert_eq!(bid_prices.len(), 3);
    assert_eq!(bid_amounts.len(), 3);
    assert_eq!(ask_prices.len(), 3);
    assert_eq!(ask_amounts.len(), 3);

    // Bids should be in descending order
    assert!(bid_prices[0] > bid_prices[1]);
    assert!(bid_prices[1] > bid_prices[2]);

    // Asks should be in ascending order
    assert!(ask_prices[0] < ask_prices[1]);
    assert!(ask_prices[1] < ask_prices[2]);
}

// Perpetuals integration tests

#[test]
fn test_perp_full_trading_flow() {
    let mut precompile = perp::PerpPrecompile::new();
    let trader = Address::repeat_byte(0x01);
    let market = Address::repeat_byte(0x02);

    // Set mark price
    precompile.set_mark_price(market, U256::from(50_000));

    // Open long position
    let open_call = IPerp::openPositionCall {
        market,
        size: U256::from(1_000_000),
        leverage: U256::from(10),
        isLong: true,
    };
    let open_input = Bytes::from(open_call.abi_encode());
    let (open_output, _) = precompile.call(&open_input, 1_000_000, trader).unwrap();
    let position_id = U256::abi_decode(&open_output, true).unwrap();

    // Get position details
    let get_call = IPerp::getPositionCall { positionId: position_id };
    let get_input = Bytes::from(get_call.abi_encode());
    let (get_output, _) = precompile.call(&get_input, 1_000_000, trader).unwrap();
    let (id, pos_trader, _, size, entry_price, leverage, is_long, is_open) =
        <(U256, Address, Address, U256, U256, U256, bool, bool)>::abi_decode(&get_output, true)
            .unwrap();

    assert_eq!(id, position_id);
    assert_eq!(pos_trader, trader);
    assert_eq!(size, U256::from(1_000_000));
    assert_eq!(entry_price, U256::from(50_000));
    assert_eq!(leverage, U256::from(10));
    assert!(is_long);
    assert!(is_open);

    // Price increases (profit)
    precompile.set_mark_price(market, U256::from(55_000));

    // Calculate PnL
    let calc_call = IPerp::calculatePnLCall { positionId: position_id };
    let calc_input = Bytes::from(calc_call.abi_encode());
    let (calc_output, _) = precompile.call(&calc_input, 1_000_000, trader).unwrap();
    let (value, pnl, _liq_price) =
        <(U256, I256, U256)>::abi_decode(&calc_output, true).unwrap();

    assert!(value > U256::ZERO);
    assert!(pnl > I256::ZERO);

    // Close position
    let close_call = IPerp::closePositionCall { positionId: position_id };
    let close_input = Bytes::from(close_call.abi_encode());
    let (close_output, _) = precompile.call(&close_input, 1_000_000, trader).unwrap();
    let final_pnl = I256::abi_decode(&close_output, true).unwrap();

    assert!(final_pnl > I256::ZERO);

    // Verify position is closed
    let (get_output2, _) = precompile.call(&get_input, 1_000_000, trader).unwrap();
    let (_, _, _, _, _, _, _, is_open2) =
        <(U256, Address, Address, U256, U256, U256, bool, bool)>::abi_decode(&get_output2, true)
            .unwrap();
    assert!(!is_open2);
}

#[test]
fn test_perp_short_position() {
    let mut precompile = perp::PerpPrecompile::new();
    let trader = Address::repeat_byte(0x01);
    let market = Address::repeat_byte(0x02);

    // Set mark price
    precompile.set_mark_price(market, U256::from(50_000));

    // Open short position
    let open_call = IPerp::openPositionCall {
        market,
        size: U256::from(1_000_000),
        leverage: U256::from(5),
        isLong: false,
    };
    let open_input = Bytes::from(open_call.abi_encode());
    let (open_output, _) = precompile.call(&open_input, 1_000_000, trader).unwrap();
    let position_id = U256::abi_decode(&open_output, true).unwrap();

    // Price decreases (profit for short)
    precompile.set_mark_price(market, U256::from(45_000));

    // Close position
    let close_call = IPerp::closePositionCall { positionId: position_id };
    let close_input = Bytes::from(close_call.abi_encode());
    let (close_output, _) = precompile.call(&close_input, 1_000_000, trader).unwrap();
    let pnl = I256::abi_decode(&close_output, true).unwrap();

    // Should be profitable
    assert!(pnl > I256::ZERO);
}

#[test]
fn test_perp_liquidation_long() {
    let mut precompile = perp::PerpPrecompile::new();
    let trader = Address::repeat_byte(0x01);
    let liquidator = Address::repeat_byte(0x02);
    let market = Address::repeat_byte(0x03);

    // Set initial price
    precompile.set_mark_price(market, U256::from(100_000));

    // Open long with high leverage
    let open_call = IPerp::openPositionCall {
        market,
        size: U256::from(1_000_000),
        leverage: U256::from(20),
        isLong: true,
    };
    let open_input = Bytes::from(open_call.abi_encode());
    let (open_output, _) = precompile.call(&open_input, 1_000_000, trader).unwrap();
    let position_id = U256::abi_decode(&open_output, true).unwrap();

    // Price drops significantly
    precompile.set_mark_price(market, U256::from(95_000));

    // Liquidate
    let liq_call = IPerp::liquidateCall { positionId: position_id };
    let liq_input = Bytes::from(liq_call.abi_encode());
    let (liq_output, _) = precompile.call(&liq_input, 1_000_000, liquidator).unwrap();
    let liq_price = U256::abi_decode(&liq_output, true).unwrap();

    assert_eq!(liq_price, U256::from(95_000));
}

#[test]
fn test_perp_liquidation_short() {
    let mut precompile = perp::PerpPrecompile::new();
    let trader = Address::repeat_byte(0x01);
    let liquidator = Address::repeat_byte(0x02);
    let market = Address::repeat_byte(0x03);

    // Set initial price
    precompile.set_mark_price(market, U256::from(100_000));

    // Open short with high leverage
    let open_call = IPerp::openPositionCall {
        market,
        size: U256::from(1_000_000),
        leverage: U256::from(20),
        isLong: false,
    };
    let open_input = Bytes::from(open_call.abi_encode());
    let (open_output, _) = precompile.call(&open_input, 1_000_000, trader).unwrap();
    let position_id = U256::abi_decode(&open_output, true).unwrap();

    // Price increases significantly
    precompile.set_mark_price(market, U256::from(105_500));

    // Liquidate
    let liq_call = IPerp::liquidateCall { positionId: position_id };
    let liq_input = Bytes::from(liq_call.abi_encode());
    let (liq_output, _) = precompile.call(&liq_input, 1_000_000, liquidator).unwrap();
    let liq_price = U256::abi_decode(&liq_output, true).unwrap();

    assert_eq!(liq_price, U256::from(105_500));
}

#[test]
fn test_perp_cannot_liquidate_healthy_position() {
    let mut precompile = perp::PerpPrecompile::new();
    let trader = Address::repeat_byte(0x01);
    let liquidator = Address::repeat_byte(0x02);
    let market = Address::repeat_byte(0x03);

    // Set initial price
    precompile.set_mark_price(market, U256::from(100_000));

    // Open position
    let open_call = IPerp::openPositionCall {
        market,
        size: U256::from(1_000_000),
        leverage: U256::from(10),
        isLong: true,
    };
    let open_input = Bytes::from(open_call.abi_encode());
    let (open_output, _) = precompile.call(&open_input, 1_000_000, trader).unwrap();
    let position_id = U256::abi_decode(&open_output, true).unwrap();

    // Price moves slightly (not enough to liquidate)
    precompile.set_mark_price(market, U256::from(98_000));

    // Try to liquidate (should fail)
    let liq_call = IPerp::liquidateCall { positionId: position_id };
    let liq_input = Bytes::from(liq_call.abi_encode());
    let result = precompile.call(&liq_input, 1_000_000, liquidator);

    assert!(result.is_err());
}

#[test]
fn test_perp_get_mark_price() {
    let mut precompile = perp::PerpPrecompile::new();
    let trader = Address::repeat_byte(0x01);
    let market = Address::repeat_byte(0x02);

    // Set mark price
    precompile.set_mark_price(market, U256::from(50_000));

    // Get mark price
    let get_call = IPerp::getMarkPriceCall { market };
    let get_input = Bytes::from(get_call.abi_encode());
    let (get_output, _) = precompile.call(&get_input, 1_000_000, trader).unwrap();
    let price = U256::abi_decode(&get_output, true).unwrap();

    assert_eq!(price, U256::from(50_000));
}

#[test]
fn test_perp_position_value_calculation() {
    let trader = Address::repeat_byte(0x01);
    let market = Address::repeat_byte(0x02);

    let position = perp::Position::new(
        1,
        trader,
        market,
        U256::from(1_000_000),
        U256::from(50_000),
        10,
        true,
        0,
    );

    // Calculate value at different price
    let value = position.value_at_price(U256::from(55_000));
    assert_eq!(value, U256::from(55_000_000_000u64));
}

#[test]
fn test_perp_liquidation_price_calculation() {
    let trader = Address::repeat_byte(0x01);
    let market = Address::repeat_byte(0x02);

    // Long position with 10x leverage
    let long_position = perp::Position::new(
        1,
        trader,
        market,
        U256::from(1_000_000),
        U256::from(50_000),
        10,
        true,
        0,
    );

    let liq_price = long_position.liquidation_price();
    // With 10x leverage, liquidation at ~10% move = 45,000
    assert!(liq_price < U256::from(50_000));
    assert!(liq_price > U256::from(44_000));

    // Short position with 10x leverage
    let short_position = perp::Position::new(
        2,
        trader,
        market,
        U256::from(1_000_000),
        U256::from(50_000),
        10,
        false,
        0,
    );

    let liq_price = short_position.liquidation_price();
    // With 10x leverage, liquidation at ~10% move = 55,000
    assert!(liq_price > U256::from(50_000));
    assert!(liq_price < U256::from(56_000));
}

