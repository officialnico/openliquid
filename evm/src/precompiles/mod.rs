use alloy_primitives::{Address, Bytes};
use anyhow::Result;

pub mod orderbook;
pub mod perp;
pub mod spot;
#[cfg(test)]
mod tests;

/// Precompile addresses - fixed L1 contract addresses
pub const SPOT_PRECOMPILE: Address = Address::new([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
]);
pub const PERP_PRECOMPILE: Address = Address::new([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2,
]);

/// Trait for custom precompiles
pub trait Precompile: Send + Sync {
    /// Execute the precompile with the given input and gas limit
    /// Returns (output, gas_used)
    fn call(&mut self, input: &Bytes, gas_limit: u64, caller: Address) -> Result<(Bytes, u64)>;
}

/// Get a precompile instance by address
pub fn get_precompile(address: &Address) -> Option<Box<dyn Precompile>> {
    match *address {
        SPOT_PRECOMPILE => Some(Box::new(spot::SpotPrecompile::new())),
        PERP_PRECOMPILE => Some(Box::new(perp::PerpPrecompile::new())),
        _ => None,
    }
}

/// Check if an address is a precompile
pub fn is_precompile(address: &Address) -> bool {
    matches!(*address, SPOT_PRECOMPILE | PERP_PRECOMPILE)
}

