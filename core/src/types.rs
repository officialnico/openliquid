use alloy_primitives::{Address, U256};
use serde::{Deserialize, Serialize};

/// Unique order identifier
pub type OrderId = u64;

/// Asset identifier (trading pair)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssetId(pub u32);

/// Price in fixed-point representation (6 decimals)
/// Example: 1_500_000 = $1.50
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Price(pub u64);

impl Price {
    pub const DECIMALS: u32 = 6;
    pub const SCALE: u64 = 1_000_000;
    
    pub fn from_float(price: f64) -> Self {
        Self((price * Self::SCALE as f64) as u64)
    }
    
    pub fn to_float(&self) -> f64 {
        self.0 as f64 / Self::SCALE as f64
    }
}

/// Order size in base asset units
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size(pub U256);

/// Order side
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Bid,  // Buy order
    Ask,  // Sell order
}

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    Limit,   // Limit order at specific price
    Market,  // Market order (best available price)
}

/// Limit order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub asset: AssetId,
    pub trader: Address,
    pub side: Side,
    pub price: Price,
    pub size: Size,
    pub filled: Size,  // Amount already filled
    pub timestamp: u64,
}

impl Order {
    pub fn new(
        id: OrderId,
        asset: AssetId,
        trader: Address,
        side: Side,
        price: Price,
        size: Size,
        timestamp: u64,
    ) -> Self {
        Self {
            id,
            asset,
            trader,
            side,
            price,
            size,
            filled: Size(U256::ZERO),
            timestamp,
        }
    }
    
    pub fn remaining(&self) -> Size {
        Size(self.size.0 - self.filled.0)
    }
    
    pub fn is_filled(&self) -> bool {
        self.filled.0 >= self.size.0
    }
}

/// Trade execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub order_id: OrderId,
    pub price: Price,
    pub size: Size,
    pub maker: Address,
    pub taker: Address,
    pub timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_conversion() {
        let price = Price::from_float(1.50);
        assert_eq!(price.0, 1_500_000);
        assert!((price.to_float() - 1.50).abs() < 0.0001);
    }

    #[test]
    fn test_price_ordering() {
        let p1 = Price::from_float(1.00);
        let p2 = Price::from_float(1.50);
        let p3 = Price::from_float(2.00);
        
        assert!(p1 < p2);
        assert!(p2 < p3);
        assert!(p1 < p3);
    }

    #[test]
    fn test_price_zero() {
        let price = Price::from_float(0.0);
        assert_eq!(price.0, 0);
        assert_eq!(price.to_float(), 0.0);
    }

    #[test]
    fn test_order_remaining() {
        let mut order = Order::new(
            1,
            AssetId(1),
            Address::ZERO,
            Side::Bid,
            Price::from_float(1.00),
            Size(U256::from(100)),
            0,
        );
        
        assert_eq!(order.remaining().0, U256::from(100));
        assert!(!order.is_filled());
        
        order.filled = Size(U256::from(50));
        assert_eq!(order.remaining().0, U256::from(50));
        assert!(!order.is_filled());
        
        order.filled = Size(U256::from(100));
        assert_eq!(order.remaining().0, U256::ZERO);
        assert!(order.is_filled());
    }

    #[test]
    fn test_order_overfill() {
        let mut order = Order::new(
            1,
            AssetId(1),
            Address::ZERO,
            Side::Bid,
            Price::from_float(1.00),
            Size(U256::from(100)),
            0,
        );
        
        order.filled = Size(U256::from(150));
        assert!(order.is_filled());
    }

    #[test]
    fn test_asset_id_equality() {
        let asset1 = AssetId(1);
        let asset2 = AssetId(1);
        let asset3 = AssetId(2);
        
        assert_eq!(asset1, asset2);
        assert_ne!(asset1, asset3);
    }

    #[test]
    fn test_side_enum() {
        let bid = Side::Bid;
        let ask = Side::Ask;
        
        assert_ne!(bid, ask);
        assert_eq!(bid, Side::Bid);
    }

    #[test]
    fn test_order_type_enum() {
        let limit = OrderType::Limit;
        let market = OrderType::Market;
        
        assert_ne!(limit, market);
        assert_eq!(limit, OrderType::Limit);
    }

    #[test]
    fn test_size_operations() {
        let size1 = Size(U256::from(100));
        let size2 = Size(U256::from(100));
        let size3 = Size(U256::from(200));
        
        assert_eq!(size1, size2);
        assert_ne!(size1, size3);
    }
}

