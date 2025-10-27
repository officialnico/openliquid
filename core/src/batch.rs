use crate::orders::{LimitOrderParams, TimeInForce};
use crate::types::*;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

/// Single order request in batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub asset: AssetId,
    pub side: Side,
    pub params: LimitOrderParams,
}

impl OrderRequest {
    pub fn new(asset: AssetId, side: Side, price: Price, size: Size) -> Self {
        Self {
            asset,
            side,
            params: LimitOrderParams::new(price, size),
        }
    }
    
    pub fn with_time_in_force(mut self, tif: TimeInForce) -> Self {
        self.params.time_in_force = tif;
        self
    }
    
    pub fn with_reduce_only(mut self, reduce_only: bool) -> Self {
        self.params.reduce_only = reduce_only;
        self
    }
    
    pub fn with_post_only(mut self, post_only: bool) -> Self {
        self.params.post_only = post_only;
        self
    }
}

/// Batch order placement request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOrderRequest {
    pub orders: Vec<OrderRequest>,
    pub atomic: bool,  // If true, all orders must succeed or all fail
}

impl BatchOrderRequest {
    pub fn new(orders: Vec<OrderRequest>) -> Self {
        Self {
            orders,
            atomic: false,
        }
    }
    
    pub fn atomic(mut self) -> Self {
        self.atomic = true;
        self
    }
    
    pub fn best_effort(mut self) -> Self {
        self.atomic = false;
        self
    }
}

/// Batch cancellation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCancelRequest {
    pub order_ids: Vec<OrderId>,
    pub atomic: bool,
}

impl BatchCancelRequest {
    pub fn new(order_ids: Vec<OrderId>) -> Self {
        Self {
            order_ids,
            atomic: false,
        }
    }
    
    pub fn atomic(mut self) -> Self {
        self.atomic = true;
        self
    }
}

/// Batch operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResult {
    pub successful: Vec<OrderId>,
    pub failed: Vec<(usize, String)>,  // (index, error message)
    pub total: usize,
}

impl BatchResult {
    pub fn new() -> Self {
        Self {
            successful: Vec::new(),
            failed: Vec::new(),
            total: 0,
        }
    }
    
    pub fn is_complete_success(&self) -> bool {
        self.failed.is_empty() && self.total > 0
    }
    
    pub fn is_complete_failure(&self) -> bool {
        self.successful.is_empty() && self.total > 0
    }
    
    pub fn is_partial_success(&self) -> bool {
        !self.successful.is_empty() && !self.failed.is_empty()
    }
    
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.successful.len() as f64 / self.total as f64
    }
}

impl Default for BatchResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch cancel result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchCancelResult {
    pub cancelled: Vec<OrderId>,
    pub failed: Vec<(OrderId, String)>,  // (order_id, error message)
}

impl BatchCancelResult {
    pub fn new() -> Self {
        Self {
            cancelled: Vec::new(),
            failed: Vec::new(),
        }
    }
    
    pub fn is_complete_success(&self) -> bool {
        self.failed.is_empty() && !self.cancelled.is_empty()
    }
}

impl Default for BatchCancelResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch operations manager
pub struct BatchOperations {
    /// Maximum orders per batch
    max_batch_size: usize,
}

impl BatchOperations {
    pub fn new(max_batch_size: usize) -> Self {
        Self { max_batch_size }
    }
    
    /// Validate batch order request
    pub fn validate_batch_request(&self, request: &BatchOrderRequest) -> Result<()> {
        if request.orders.is_empty() {
            return Err(anyhow!("Batch request cannot be empty"));
        }
        
        if request.orders.len() > self.max_batch_size {
            return Err(anyhow!(
                "Batch size {} exceeds maximum {}",
                request.orders.len(),
                self.max_batch_size
            ));
        }
        
        Ok(())
    }
    
    /// Validate batch cancel request
    pub fn validate_cancel_request(&self, request: &BatchCancelRequest) -> Result<()> {
        if request.order_ids.is_empty() {
            return Err(anyhow!("Batch cancel request cannot be empty"));
        }
        
        if request.order_ids.len() > self.max_batch_size {
            return Err(anyhow!(
                "Batch size {} exceeds maximum {}",
                request.order_ids.len(),
                self.max_batch_size
            ));
        }
        
        Ok(())
    }
    
    /// Create batch result from individual results
    pub fn create_batch_result(
        &self,
        results: Vec<Result<OrderId>>,
    ) -> BatchResult {
        let mut batch_result = BatchResult::new();
        batch_result.total = results.len();
        
        for (idx, result) in results.into_iter().enumerate() {
            match result {
                Ok(order_id) => batch_result.successful.push(order_id),
                Err(e) => batch_result.failed.push((idx, e.to_string())),
            }
        }
        
        batch_result
    }
    
    /// Check if batch should be rolled back (atomic mode)
    pub fn should_rollback(&self, result: &BatchResult, atomic: bool) -> bool {
        atomic && !result.failed.is_empty()
    }
    
    /// Get max batch size
    pub fn max_batch_size(&self) -> usize {
        self.max_batch_size
    }
}

impl Default for BatchOperations {
    fn default() -> Self {
        Self::new(100)  // Default max 100 orders per batch
    }
}

/// Batch order builder for convenient batch creation
pub struct BatchOrderBuilder {
    orders: Vec<OrderRequest>,
    atomic: bool,
}

impl BatchOrderBuilder {
    pub fn new() -> Self {
        Self {
            orders: Vec::new(),
            atomic: false,
        }
    }
    
    pub fn add_order(mut self, request: OrderRequest) -> Self {
        self.orders.push(request);
        self
    }
    
    pub fn add_limit_order(
        mut self,
        asset: AssetId,
        side: Side,
        price: Price,
        size: Size,
    ) -> Self {
        self.orders.push(OrderRequest::new(asset, side, price, size));
        self
    }
    
    pub fn atomic(mut self) -> Self {
        self.atomic = true;
        self
    }
    
    pub fn best_effort(mut self) -> Self {
        self.atomic = false;
        self
    }
    
    pub fn build(self) -> BatchOrderRequest {
        BatchOrderRequest {
            orders: self.orders,
            atomic: self.atomic,
        }
    }
}

impl Default for BatchOrderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::U256;

    #[test]
    fn test_order_request_new() {
        let req = OrderRequest::new(
            AssetId(1),
            Side::Bid,
            Price::from_float(100.0),
            Size(U256::from(10)),
        );
        
        assert_eq!(req.asset, AssetId(1));
        assert_eq!(req.side, Side::Bid);
        assert_eq!(req.params.price, Price::from_float(100.0));
    }

    #[test]
    fn test_order_request_builder() {
        let req = OrderRequest::new(
            AssetId(1),
            Side::Bid,
            Price::from_float(100.0),
            Size(U256::from(10)),
        )
        .with_time_in_force(TimeInForce::IOC)
        .with_reduce_only(true)
        .with_post_only(false);
        
        assert_eq!(req.params.time_in_force, TimeInForce::IOC);
        assert!(req.params.reduce_only);
    }

    #[test]
    fn test_batch_order_request_new() {
        let orders = vec![
            OrderRequest::new(AssetId(1), Side::Bid, Price::from_float(100.0), Size(U256::from(10))),
            OrderRequest::new(AssetId(2), Side::Ask, Price::from_float(200.0), Size(U256::from(20))),
        ];
        
        let batch = BatchOrderRequest::new(orders);
        assert_eq!(batch.orders.len(), 2);
        assert!(!batch.atomic);
    }

    #[test]
    fn test_batch_order_request_atomic() {
        let orders = vec![
            OrderRequest::new(AssetId(1), Side::Bid, Price::from_float(100.0), Size(U256::from(10))),
        ];
        
        let batch = BatchOrderRequest::new(orders).atomic();
        assert!(batch.atomic);
    }

    #[test]
    fn test_batch_cancel_request() {
        let req = BatchCancelRequest::new(vec![1, 2, 3]);
        assert_eq!(req.order_ids.len(), 3);
        assert!(!req.atomic);
        
        let atomic_req = req.atomic();
        assert!(atomic_req.atomic);
    }

    #[test]
    fn test_batch_result_new() {
        let result = BatchResult::new();
        assert_eq!(result.successful.len(), 0);
        assert_eq!(result.failed.len(), 0);
        assert_eq!(result.total, 0);
    }

    #[test]
    fn test_batch_result_complete_success() {
        let mut result = BatchResult::new();
        result.successful = vec![1, 2, 3];
        result.total = 3;
        
        assert!(result.is_complete_success());
        assert!(!result.is_complete_failure());
        assert!(!result.is_partial_success());
    }

    #[test]
    fn test_batch_result_complete_failure() {
        let mut result = BatchResult::new();
        result.failed = vec![(0, "error1".to_string()), (1, "error2".to_string())];
        result.total = 2;
        
        assert!(!result.is_complete_success());
        assert!(result.is_complete_failure());
        assert!(!result.is_partial_success());
    }

    #[test]
    fn test_batch_result_partial_success() {
        let mut result = BatchResult::new();
        result.successful = vec![1];
        result.failed = vec![(1, "error".to_string())];
        result.total = 2;
        
        assert!(!result.is_complete_success());
        assert!(!result.is_complete_failure());
        assert!(result.is_partial_success());
    }

    #[test]
    fn test_batch_result_success_rate() {
        let mut result = BatchResult::new();
        result.successful = vec![1, 2, 3];
        result.failed = vec![(3, "error".to_string())];
        result.total = 4;
        
        assert_eq!(result.success_rate(), 0.75);
    }

    #[test]
    fn test_batch_result_success_rate_empty() {
        let result = BatchResult::new();
        assert_eq!(result.success_rate(), 0.0);
    }

    #[test]
    fn test_batch_operations_new() {
        let ops = BatchOperations::new(50);
        assert_eq!(ops.max_batch_size(), 50);
    }

    #[test]
    fn test_batch_operations_default() {
        let ops = BatchOperations::default();
        assert_eq!(ops.max_batch_size(), 100);
    }

    #[test]
    fn test_validate_batch_request_empty() {
        let ops = BatchOperations::default();
        let req = BatchOrderRequest::new(vec![]);
        
        assert!(ops.validate_batch_request(&req).is_err());
    }

    #[test]
    fn test_validate_batch_request_too_large() {
        let ops = BatchOperations::new(2);
        let orders = vec![
            OrderRequest::new(AssetId(1), Side::Bid, Price::from_float(100.0), Size(U256::from(10))),
            OrderRequest::new(AssetId(2), Side::Bid, Price::from_float(100.0), Size(U256::from(10))),
            OrderRequest::new(AssetId(3), Side::Bid, Price::from_float(100.0), Size(U256::from(10))),
        ];
        let req = BatchOrderRequest::new(orders);
        
        assert!(ops.validate_batch_request(&req).is_err());
    }

    #[test]
    fn test_validate_batch_request_valid() {
        let ops = BatchOperations::default();
        let orders = vec![
            OrderRequest::new(AssetId(1), Side::Bid, Price::from_float(100.0), Size(U256::from(10))),
            OrderRequest::new(AssetId(2), Side::Bid, Price::from_float(100.0), Size(U256::from(10))),
        ];
        let req = BatchOrderRequest::new(orders);
        
        assert!(ops.validate_batch_request(&req).is_ok());
    }

    #[test]
    fn test_create_batch_result() {
        let ops = BatchOperations::default();
        let results = vec![
            Ok(1),
            Err(anyhow!("error1")),
            Ok(2),
            Err(anyhow!("error2")),
        ];
        
        let batch_result = ops.create_batch_result(results);
        
        assert_eq!(batch_result.total, 4);
        assert_eq!(batch_result.successful.len(), 2);
        assert_eq!(batch_result.failed.len(), 2);
        assert!(batch_result.is_partial_success());
    }

    #[test]
    fn test_should_rollback() {
        let ops = BatchOperations::default();
        let mut result = BatchResult::new();
        result.failed.push((0, "error".to_string()));
        
        // Atomic mode - should rollback
        assert!(ops.should_rollback(&result, true));
        
        // Best effort mode - should not rollback
        assert!(!ops.should_rollback(&result, false));
    }

    #[test]
    fn test_batch_order_builder() {
        let batch = BatchOrderBuilder::new()
            .add_limit_order(AssetId(1), Side::Bid, Price::from_float(100.0), Size(U256::from(10)))
            .add_limit_order(AssetId(2), Side::Ask, Price::from_float(200.0), Size(U256::from(20)))
            .atomic()
            .build();
        
        assert_eq!(batch.orders.len(), 2);
        assert!(batch.atomic);
    }

    #[test]
    fn test_batch_order_builder_with_order_request() {
        let req = OrderRequest::new(AssetId(1), Side::Bid, Price::from_float(100.0), Size(U256::from(10)))
            .with_time_in_force(TimeInForce::IOC);
        
        let batch = BatchOrderBuilder::new()
            .add_order(req)
            .build();
        
        assert_eq!(batch.orders.len(), 1);
        assert_eq!(batch.orders[0].params.time_in_force, TimeInForce::IOC);
    }

    #[test]
    fn test_batch_cancel_result() {
        let mut result = BatchCancelResult::new();
        result.cancelled = vec![1, 2];
        result.failed = vec![];
        
        assert!(result.is_complete_success());
    }

    #[test]
    fn test_batch_cancel_result_with_failures() {
        let mut result = BatchCancelResult::new();
        result.cancelled = vec![1];
        result.failed = vec![(2, "error".to_string())];
        
        assert!(!result.is_complete_success());
    }

    #[test]
    fn test_validate_cancel_request_empty() {
        let ops = BatchOperations::default();
        let req = BatchCancelRequest::new(vec![]);
        
        assert!(ops.validate_cancel_request(&req).is_err());
    }

    #[test]
    fn test_validate_cancel_request_too_large() {
        let ops = BatchOperations::new(2);
        let req = BatchCancelRequest::new(vec![1, 2, 3]);
        
        assert!(ops.validate_cancel_request(&req).is_err());
    }

    #[test]
    fn test_validate_cancel_request_valid() {
        let ops = BatchOperations::default();
        let req = BatchCancelRequest::new(vec![1, 2, 3]);
        
        assert!(ops.validate_cancel_request(&req).is_ok());
    }
}

