use alloy_primitives::U256;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Insurance fund contribution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contribution {
    pub amount: U256,
    pub timestamp: u64,
}

/// Insurance fund payout record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payout {
    pub amount: U256,
    pub timestamp: u64,
}

/// Insurance fund for bad debt
pub struct InsuranceFund {
    /// Total fund balance
    balance: U256,
    /// Contributions (from liquidation fees)
    contributions: Vec<Contribution>,
    /// Payouts (for bad debt)
    payouts: Vec<Payout>,
}

impl InsuranceFund {
    pub fn new() -> Self {
        Self {
            balance: U256::ZERO,
            contributions: Vec::new(),
            payouts: Vec::new(),
        }
    }
    
    /// Add funds to insurance pool
    pub fn contribute(&mut self, amount: U256, timestamp: u64) {
        self.balance = self.balance.saturating_add(amount);
        self.contributions.push(Contribution { amount, timestamp });
    }
    
    /// Use insurance fund to cover bad debt
    pub fn cover_bad_debt(
        &mut self,
        amount: U256,
        timestamp: u64,
    ) -> Result<U256> {
        if self.balance < amount {
            // Partial coverage
            let covered = self.balance;
            self.balance = U256::ZERO;
            self.payouts.push(Payout { amount: covered, timestamp });
            return Ok(covered);
        }
        
        self.balance = self.balance.saturating_sub(amount);
        self.payouts.push(Payout { amount, timestamp });
        Ok(amount)
    }
    
    /// Get current balance
    pub fn get_balance(&self) -> U256 {
        self.balance
    }
    
    /// Get total contributions
    pub fn total_contributions(&self) -> U256 {
        self.contributions.iter().fold(U256::ZERO, |acc, c| acc.saturating_add(c.amount))
    }
    
    /// Get total payouts
    pub fn total_payouts(&self) -> U256 {
        self.payouts.iter().fold(U256::ZERO, |acc, p| acc.saturating_add(p.amount))
    }
    
    /// Get contribution history
    pub fn get_contributions(&self) -> &[Contribution] {
        &self.contributions
    }
    
    /// Get payout history
    pub fn get_payouts(&self) -> &[Payout] {
        &self.payouts
    }
    
    /// Check if fund can cover amount
    pub fn can_cover(&self, amount: U256) -> bool {
        self.balance >= amount
    }
}

impl Default for InsuranceFund {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insurance_contribution() {
        let mut fund = InsuranceFund::new();
        
        fund.contribute(U256::from(1000), 0);
        
        assert_eq!(fund.get_balance(), U256::from(1000));
        assert_eq!(fund.total_contributions(), U256::from(1000));
    }

    #[test]
    fn test_multiple_contributions() {
        let mut fund = InsuranceFund::new();
        
        fund.contribute(U256::from(500), 0);
        fund.contribute(U256::from(300), 1);
        fund.contribute(U256::from(200), 2);
        
        assert_eq!(fund.get_balance(), U256::from(1000));
        assert_eq!(fund.total_contributions(), U256::from(1000));
        assert_eq!(fund.get_contributions().len(), 3);
    }

    #[test]
    fn test_bad_debt_coverage() {
        let mut fund = InsuranceFund::new();
        
        fund.contribute(U256::from(1000), 0);
        
        let covered = fund.cover_bad_debt(U256::from(400), 1).unwrap();
        
        assert_eq!(covered, U256::from(400));
        assert_eq!(fund.get_balance(), U256::from(600));
        assert_eq!(fund.total_payouts(), U256::from(400));
    }

    #[test]
    fn test_insufficient_insurance() {
        let mut fund = InsuranceFund::new();
        
        fund.contribute(U256::from(500), 0);
        
        // Try to cover more than available
        let covered = fund.cover_bad_debt(U256::from(1000), 1).unwrap();
        
        // Should only cover what's available
        assert_eq!(covered, U256::from(500));
        assert_eq!(fund.get_balance(), U256::ZERO);
    }

    #[test]
    fn test_can_cover_check() {
        let mut fund = InsuranceFund::new();
        
        fund.contribute(U256::from(1000), 0);
        
        assert!(fund.can_cover(U256::from(500)));
        assert!(fund.can_cover(U256::from(1000)));
        assert!(!fund.can_cover(U256::from(1001)));
    }

    #[test]
    fn test_empty_fund() {
        let fund = InsuranceFund::new();
        
        assert_eq!(fund.get_balance(), U256::ZERO);
        assert!(!fund.can_cover(U256::from(1)));
    }

    #[test]
    fn test_multiple_payouts() {
        let mut fund = InsuranceFund::new();
        
        fund.contribute(U256::from(1000), 0);
        
        fund.cover_bad_debt(U256::from(300), 1).unwrap();
        fund.cover_bad_debt(U256::from(200), 2).unwrap();
        fund.cover_bad_debt(U256::from(100), 3).unwrap();
        
        assert_eq!(fund.get_balance(), U256::from(400));
        assert_eq!(fund.total_payouts(), U256::from(600));
        assert_eq!(fund.get_payouts().len(), 3);
    }

    #[test]
    fn test_contribution_after_payout() {
        let mut fund = InsuranceFund::new();
        
        fund.contribute(U256::from(1000), 0);
        fund.cover_bad_debt(U256::from(600), 1).unwrap();
        fund.contribute(U256::from(500), 2);
        
        assert_eq!(fund.get_balance(), U256::from(900));
        assert_eq!(fund.total_contributions(), U256::from(1500));
        assert_eq!(fund.total_payouts(), U256::from(600));
    }
}

