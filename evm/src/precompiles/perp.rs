use super::Precompile;
use alloy_primitives::{Address, Bytes, I256, U256};
use alloy_sol_types::{sol, SolCall, SolValue};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Define Solidity interface for perpetuals
sol! {
    /// Perpetual trading interface
    interface IPerp {
        /// Open a perpetual position
        /// @param market The market address
        /// @param size Position size (in base units)
        /// @param leverage Leverage multiplier (e.g., 10 = 10x)
        /// @param isLong True for long, false for short
        /// @return positionId The ID of the opened position
        function openPosition(
            address market,
            uint256 size,
            uint256 leverage,
            bool isLong
        ) external returns (uint256 positionId);

        /// Close a perpetual position
        /// @param positionId The position to close
        /// @return pnl Profit and loss (can be negative)
        function closePosition(uint256 positionId) external returns (int256 pnl);

        /// Liquidate an undercollateralized position
        /// @param positionId The position to liquidate
        /// @return liquidationPrice The price at which liquidation occurred
        function liquidate(uint256 positionId) external returns (uint256 liquidationPrice);

        /// Get position details
        /// @param positionId The position ID
        /// @return position The position struct
        function getPosition(uint256 positionId) external view returns (
            uint256 id,
            address trader,
            address market,
            uint256 size,
            uint256 entryPrice,
            uint256 leverage,
            bool isLong,
            bool isOpen
        );

        /// Get current mark price for a market
        /// @param market The market address
        /// @return price The current mark price
        function getMarkPrice(address market) external view returns (uint256 price);

        /// Calculate position value and PnL
        /// @param positionId The position ID
        /// @return value Current position value
        /// @return pnl Unrealized PnL
        /// @return liquidationPrice Price at which position would be liquidated
        function calculatePnL(uint256 positionId) external view returns (
            uint256 value,
            int256 pnl,
            uint256 liquidationPrice
        );
    }
}

/// Gas costs for perpetual operations
const OPEN_POSITION_GAS: u64 = 100_000;
const CLOSE_POSITION_GAS: u64 = 80_000;
const LIQUIDATE_GAS: u64 = 60_000;
const GET_POSITION_GAS: u64 = 5_000;
const GET_MARK_PRICE_GAS: u64 = 3_000;
const CALCULATE_PNL_GAS: u64 = 10_000;

/// Maximum leverage allowed (50x)
const MAX_LEVERAGE: u64 = 50;

/// Liquidation threshold (90% of initial margin)
const LIQUIDATION_THRESHOLD: u64 = 90;

/// A perpetual position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub id: u64,
    pub trader: Address,
    pub market: Address,
    pub size: U256,
    pub entry_price: U256,
    pub leverage: u64,
    pub is_long: bool,
    pub is_open: bool,
    pub timestamp: u64,
}

impl Position {
    pub fn new(
        id: u64,
        trader: Address,
        market: Address,
        size: U256,
        entry_price: U256,
        leverage: u64,
        is_long: bool,
        timestamp: u64,
    ) -> Self {
        Self {
            id,
            trader,
            market,
            size,
            entry_price,
            leverage,
            is_long,
            is_open: true,
            timestamp,
        }
    }

    /// Calculate position value at current price
    pub fn value_at_price(&self, current_price: U256) -> U256 {
        // Value = size * current_price
        self.size.saturating_mul(current_price)
    }

    /// Calculate PnL at current price (can be negative)
    pub fn pnl_at_price(&self, current_price: U256) -> I256 {
        // For longs: PnL = (current_price - entry_price) * size
        // For shorts: PnL = (entry_price - current_price) * size

        let price_diff = if self.is_long {
            I256::try_from(current_price).unwrap_or(I256::ZERO)
                - I256::try_from(self.entry_price).unwrap_or(I256::ZERO)
        } else {
            I256::try_from(self.entry_price).unwrap_or(I256::ZERO)
                - I256::try_from(current_price).unwrap_or(I256::ZERO)
        };

        price_diff.saturating_mul(I256::try_from(self.size).unwrap_or(I256::ZERO))
    }

    /// Calculate liquidation price
    pub fn liquidation_price(&self) -> U256 {
        // LIQUIDATION_THRESHOLD % of margin can be lost before liquidation
        // Initial margin = 1 / leverage of position value  
        // Max loss = LIQUIDATION_THRESHOLD% of initial margin
        // = (LIQUIDATION_THRESHOLD / 100) * (1 / leverage) of position value
        // = LIQUIDATION_THRESHOLD / (100 * leverage) of position value
        
        // For 20x leverage and 90% threshold: 90/(100*20) = 4.5% price move
        let price_move_percent = LIQUIDATION_THRESHOLD * 100 / self.leverage;

        if self.is_long {
            // For long: liq_price = entry_price * (1 - price_move_percent/10000)
            let factor = 10000u64.saturating_sub(price_move_percent);
            self.entry_price.saturating_mul(U256::from(factor)) / U256::from(10000)
        } else {
            // For short: liq_price = entry_price * (1 + price_move_percent/10000)
            let factor = 10000u64.saturating_add(price_move_percent);
            self.entry_price.saturating_mul(U256::from(factor)) / U256::from(10000)
        }
    }

    /// Check if position should be liquidated at current price
    pub fn should_liquidate(&self, current_price: U256) -> bool {
        let liq_price = self.liquidation_price();
        if self.is_long {
            current_price <= liq_price
        } else {
            current_price >= liq_price
        }
    }
}

/// Perpetual trading precompile
pub struct PerpPrecompile {
    /// All positions
    positions: HashMap<u64, Position>,
    /// Next position ID
    next_position_id: u64,
    /// Mark prices per market (simulated oracle)
    mark_prices: HashMap<Address, U256>,
    /// Current timestamp
    timestamp: u64,
}

impl PerpPrecompile {
    pub fn new() -> Self {
        Self {
            positions: HashMap::new(),
            next_position_id: 1,
            mark_prices: HashMap::new(),
            timestamp: 0,
        }
    }

    /// Set mark price for a market (for testing/simulation)
    pub fn set_mark_price(&mut self, market: Address, price: U256) {
        self.mark_prices.insert(market, price);
    }

    fn get_mark_price(&self, market: Address) -> Result<U256> {
        self.mark_prices
            .get(&market)
            .copied()
            .ok_or_else(|| anyhow!("No mark price for market"))
    }

    fn open_position_impl(
        &mut self,
        trader: Address,
        market: Address,
        size: U256,
        leverage: U256,
        is_long: bool,
    ) -> Result<(U256, u64)> {
        // Validate inputs
        if size == U256::ZERO {
            return Err(anyhow!("Size must be greater than zero"));
        }

        let leverage_u64 = leverage.to::<u64>();
        if leverage_u64 == 0 || leverage_u64 > MAX_LEVERAGE {
            return Err(anyhow!(
                "Leverage must be between 1 and {}",
                MAX_LEVERAGE
            ));
        }

        // Get current mark price
        let entry_price = self.get_mark_price(market)?;

        // Create position
        let position_id = self.next_position_id;
        self.next_position_id += 1;

        let position = Position::new(
            position_id,
            trader,
            market,
            size,
            entry_price,
            leverage_u64,
            is_long,
            self.timestamp,
        );

        self.positions.insert(position_id, position);

        Ok((U256::from(position_id), OPEN_POSITION_GAS))
    }

    fn close_position_impl(&mut self, trader: Address, position_id: U256) -> Result<(I256, u64)> {
        let position_id_u64 = position_id.to::<u64>();

        // Get position and validate
        let (_market, pnl) = {
            let position = self
                .positions
                .get(&position_id_u64)
                .ok_or_else(|| anyhow!("Position not found"))?;

            // Verify ownership
            if position.trader != trader {
                return Err(anyhow!("Not position owner"));
            }

            // Verify position is open
            if !position.is_open {
                return Err(anyhow!("Position already closed"));
            }

            // Get current price
            let current_price = self.get_mark_price(position.market)?;

            // Calculate PnL
            let pnl = position.pnl_at_price(current_price);

            (position.market, pnl)
        };

        // Now mark position as closed (separate borrow)
        if let Some(position) = self.positions.get_mut(&position_id_u64) {
            position.is_open = false;
        }

        Ok((pnl, CLOSE_POSITION_GAS))
    }

    fn liquidate_impl(
        &mut self,
        _liquidator: Address,
        position_id: U256,
    ) -> Result<(U256, u64)> {
        let position_id_u64 = position_id.to::<u64>();

        // Get position and validate
        let current_price = {
            let position = self
                .positions
                .get(&position_id_u64)
                .ok_or_else(|| anyhow!("Position not found"))?;

            // Verify position is open
            if !position.is_open {
                return Err(anyhow!("Position already closed"));
            }

            // Get current price
            let current_price = self.get_mark_price(position.market)?;

            // Check if position should be liquidated
            if !position.should_liquidate(current_price) {
                return Err(anyhow!("Position not eligible for liquidation"));
            }

            current_price
        };

        // Now mark position as closed (separate borrow)
        if let Some(position) = self.positions.get_mut(&position_id_u64) {
            position.is_open = false;
        }

        Ok((current_price, LIQUIDATE_GAS))
    }

    fn get_position_impl(
        &self,
        position_id: U256,
    ) -> Result<(U256, Address, Address, U256, U256, U256, bool, bool, u64)> {
        let position_id_u64 = position_id.to::<u64>();

        let position = self
            .positions
            .get(&position_id_u64)
            .ok_or_else(|| anyhow!("Position not found"))?;

        Ok((
            U256::from(position.id),
            position.trader,
            position.market,
            position.size,
            position.entry_price,
            U256::from(position.leverage),
            position.is_long,
            position.is_open,
            GET_POSITION_GAS,
        ))
    }

    fn get_mark_price_impl(&self, market: Address) -> Result<(U256, u64)> {
        let price = self.get_mark_price(market)?;
        Ok((price, GET_MARK_PRICE_GAS))
    }

    fn calculate_pnl_impl(&self, position_id: U256) -> Result<(U256, I256, U256, u64)> {
        let position_id_u64 = position_id.to::<u64>();

        let position = self
            .positions
            .get(&position_id_u64)
            .ok_or_else(|| anyhow!("Position not found"))?;

        let current_price = self.get_mark_price(position.market)?;
        let value = position.value_at_price(current_price);
        let pnl = position.pnl_at_price(current_price);
        let liq_price = position.liquidation_price();

        Ok((value, pnl, liq_price, CALCULATE_PNL_GAS))
    }
}

impl Precompile for PerpPrecompile {
    fn call(&mut self, input: &Bytes, gas_limit: u64, caller: Address) -> Result<(Bytes, u64)> {
        if input.len() < 4 {
            return Err(anyhow!("Input too short"));
        }

        // Increment timestamp
        self.timestamp += 1;

        let selector = &input[..4];

        match selector {
            // openPosition(address,uint256,uint256,bool)
            sel if sel == IPerp::openPositionCall::SELECTOR => {
                let call = IPerp::openPositionCall::abi_decode(input, false)?;
                let (position_id, gas) = self.open_position_impl(
                    caller,
                    call.market,
                    call.size,
                    call.leverage,
                    call.isLong,
                )?;

                if gas > gas_limit {
                    return Err(anyhow!("Out of gas"));
                }

                Ok((Bytes::from(position_id.abi_encode()), gas))
            }

            // closePosition(uint256)
            sel if sel == IPerp::closePositionCall::SELECTOR => {
                let call = IPerp::closePositionCall::abi_decode(input, false)?;
                let (pnl, gas) = self.close_position_impl(caller, call.positionId)?;

                if gas > gas_limit {
                    return Err(anyhow!("Out of gas"));
                }

                Ok((Bytes::from(pnl.abi_encode()), gas))
            }

            // liquidate(uint256)
            sel if sel == IPerp::liquidateCall::SELECTOR => {
                let call = IPerp::liquidateCall::abi_decode(input, false)?;
                let (liq_price, gas) = self.liquidate_impl(caller, call.positionId)?;

                if gas > gas_limit {
                    return Err(anyhow!("Out of gas"));
                }

                Ok((Bytes::from(liq_price.abi_encode()), gas))
            }

            // getPosition(uint256)
            sel if sel == IPerp::getPositionCall::SELECTOR => {
                let call = IPerp::getPositionCall::abi_decode(input, false)?;
                let (id, trader, market, size, entry_price, leverage, is_long, is_open, gas) =
                    self.get_position_impl(call.positionId)?;

                if gas > gas_limit {
                    return Err(anyhow!("Out of gas"));
                }

                let result = (id, trader, market, size, entry_price, leverage, is_long, is_open);
                Ok((Bytes::from(result.abi_encode()), gas))
            }

            // getMarkPrice(address)
            sel if sel == IPerp::getMarkPriceCall::SELECTOR => {
                let call = IPerp::getMarkPriceCall::abi_decode(input, false)?;
                let (price, gas) = self.get_mark_price_impl(call.market)?;

                if gas > gas_limit {
                    return Err(anyhow!("Out of gas"));
                }

                Ok((Bytes::from(price.abi_encode()), gas))
            }

            // calculatePnL(uint256)
            sel if sel == IPerp::calculatePnLCall::SELECTOR => {
                let call = IPerp::calculatePnLCall::abi_decode(input, false)?;
                let (value, pnl, liq_price, gas) = self.calculate_pnl_impl(call.positionId)?;

                if gas > gas_limit {
                    return Err(anyhow!("Out of gas"));
                }

                let result = (value, pnl, liq_price);
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
    fn test_open_position() {
        let mut precompile = PerpPrecompile::new();
        let trader = Address::repeat_byte(0x01);
        let market = Address::repeat_byte(0x02);

        // Set mark price
        precompile.set_mark_price(market, U256::from(50_000));

        // Open position
        let call = IPerp::openPositionCall {
            market,
            size: U256::from(1_000_000),
            leverage: U256::from(10),
            isLong: true,
        };
        let input = Bytes::from(call.abi_encode());

        let (output, gas) = precompile.call(&input, 1_000_000, trader).unwrap();

        assert_eq!(gas, OPEN_POSITION_GAS);

        let position_id = U256::abi_decode(&output, true).unwrap();
        assert_eq!(position_id, U256::from(1));
    }

    #[test]
    fn test_close_position_with_profit() {
        let mut precompile = PerpPrecompile::new();
        let trader = Address::repeat_byte(0x01);
        let market = Address::repeat_byte(0x02);

        // Set initial price
        precompile.set_mark_price(market, U256::from(50_000));

        // Open long position
        let open_call = IPerp::openPositionCall {
            market,
            size: U256::from(1_000_000),
            leverage: U256::from(10),
            isLong: true,
        };
        let open_input = Bytes::from(open_call.abi_encode());
        let (output, _) = precompile.call(&open_input, 1_000_000, trader).unwrap();
        let position_id = U256::abi_decode(&output, true).unwrap();

        // Price increases (profit for long)
        precompile.set_mark_price(market, U256::from(55_000));

        // Close position
        let close_call = IPerp::closePositionCall { positionId: position_id };
        let close_input = Bytes::from(close_call.abi_encode());
        let (output, gas) = precompile.call(&close_input, 1_000_000, trader).unwrap();

        assert_eq!(gas, CLOSE_POSITION_GAS);

        let pnl = I256::abi_decode(&output, true).unwrap();
        // PnL = (55_000 - 50_000) * 1_000_000 = 5_000_000_000
        assert!(pnl > I256::ZERO);
    }

    #[test]
    fn test_liquidation() {
        let mut precompile = PerpPrecompile::new();
        let trader = Address::repeat_byte(0x01);
        let liquidator = Address::repeat_byte(0x03);
        let market = Address::repeat_byte(0x02);

        // Set initial price
        precompile.set_mark_price(market, U256::from(50_000));

        // Open long position with 10x leverage
        let open_call = IPerp::openPositionCall {
            market,
            size: U256::from(1_000_000),
            leverage: U256::from(10),
            isLong: true,
        };
        let open_input = Bytes::from(open_call.abi_encode());
        let (output, _) = precompile.call(&open_input, 1_000_000, trader).unwrap();
        let position_id = U256::abi_decode(&output, true).unwrap();

        // Price drops significantly (trigger liquidation)
        // With 10x leverage, ~10% move triggers liquidation
        precompile.set_mark_price(market, U256::from(45_000));

        // Liquidate position
        let liq_call = IPerp::liquidateCall { positionId: position_id };
        let liq_input = Bytes::from(liq_call.abi_encode());
        let (output, gas) = precompile.call(&liq_input, 1_000_000, liquidator).unwrap();

        assert_eq!(gas, LIQUIDATE_GAS);

        let liq_price = U256::abi_decode(&output, true).unwrap();
        assert_eq!(liq_price, U256::from(45_000));
    }

    #[test]
    fn test_get_position() {
        let mut precompile = PerpPrecompile::new();
        let trader = Address::repeat_byte(0x01);
        let market = Address::repeat_byte(0x02);

        precompile.set_mark_price(market, U256::from(50_000));

        // Open position
        let open_call = IPerp::openPositionCall {
            market,
            size: U256::from(1_000_000),
            leverage: U256::from(10),
            isLong: true,
        };
        let open_input = Bytes::from(open_call.abi_encode());
        let (output, _) = precompile.call(&open_input, 1_000_000, trader).unwrap();
        let position_id = U256::abi_decode(&output, true).unwrap();

        // Get position
        let get_call = IPerp::getPositionCall { positionId: position_id };
        let get_input = Bytes::from(get_call.abi_encode());
        let (output, _) = precompile.call(&get_input, 1_000_000, trader).unwrap();

        let (id, pos_trader, pos_market, size, entry_price, leverage, is_long, is_open) =
            <(U256, Address, Address, U256, U256, U256, bool, bool)>::abi_decode(&output, true)
                .unwrap();

        assert_eq!(id, position_id);
        assert_eq!(pos_trader, trader);
        assert_eq!(pos_market, market);
        assert_eq!(size, U256::from(1_000_000));
        assert_eq!(entry_price, U256::from(50_000));
        assert_eq!(leverage, U256::from(10));
        assert!(is_long);
        assert!(is_open);
    }

    #[test]
    fn test_calculate_pnl() {
        let mut precompile = PerpPrecompile::new();
        let trader = Address::repeat_byte(0x01);
        let market = Address::repeat_byte(0x02);

        precompile.set_mark_price(market, U256::from(50_000));

        // Open position
        let open_call = IPerp::openPositionCall {
            market,
            size: U256::from(1_000_000),
            leverage: U256::from(10),
            isLong: true,
        };
        let open_input = Bytes::from(open_call.abi_encode());
        let (output, _) = precompile.call(&open_input, 1_000_000, trader).unwrap();
        let position_id = U256::abi_decode(&output, true).unwrap();

        // Price increases
        precompile.set_mark_price(market, U256::from(52_000));

        // Calculate PnL
        let calc_call = IPerp::calculatePnLCall { positionId: position_id };
        let calc_input = Bytes::from(calc_call.abi_encode());
        let (output, _) = precompile.call(&calc_input, 1_000_000, trader).unwrap();

        let (value, pnl, liq_price) =
            <(U256, I256, U256)>::abi_decode(&output, true).unwrap();

        // Value = size * current_price
        assert!(value > U256::ZERO);
        // PnL should be positive
        assert!(pnl > I256::ZERO);
        // Liquidation price should be below entry price for longs
        assert!(liq_price < U256::from(50_000));
    }

    #[test]
    fn test_invalid_leverage() {
        let mut precompile = PerpPrecompile::new();
        let trader = Address::repeat_byte(0x01);
        let market = Address::repeat_byte(0x02);

        precompile.set_mark_price(market, U256::from(50_000));

        // Try to open with 100x leverage (above max)
        let call = IPerp::openPositionCall {
            market,
            size: U256::from(1_000_000),
            leverage: U256::from(100),
            isLong: true,
        };
        let input = Bytes::from(call.abi_encode());

        let result = precompile.call(&input, 1_000_000, trader);
        assert!(result.is_err());
    }
}

