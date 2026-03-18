//! Module API for inter-module communication
//!
//! Allows other modules (e.g., Stratum V2) to query coinbase payout requirements

use blvm_node::module::inter_module::api::ModuleAPI;
use blvm_node::module::traits::ModuleError;
use std::sync::Arc;
use tokio::sync::RwLock;

/// DATUM module API for other modules
pub struct DatumModuleApi {
    /// Pool instance for accessing coinbase payouts
    pool: Arc<RwLock<crate::pool::DatumPool>>,
}

impl DatumModuleApi {
    /// Create a new DATUM module API
    pub fn new(pool: Arc<RwLock<crate::pool::DatumPool>>) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ModuleAPI for DatumModuleApi {
    async fn handle_request(
        &self,
        method: &str,
        params: &[u8],
        _caller_module_id: &str,
    ) -> Result<Vec<u8>, ModuleError> {
        match method {
            "get_coinbase_payout" => {
                let pool = self.pool.read().await;
                if let Some(payout) = pool.get_coinbase_payout() {
                    let json = serde_json::json!({
                        "outputs": payout.outputs.iter().map(|o| {
                            serde_json::json!({
                                "script": hex::encode(&o.script),
                                "value": o.value
                            })
                        }).collect::<Vec<_>>(),
                        "primary_tag": payout.primary_tag,
                        "unique_id": payout.unique_id
                    });
                    serde_json::to_vec(&json).map_err(|e| {
                        ModuleError::OperationError(format!("Serialization error: {}", e))
                    })
                } else {
                    serde_json::to_vec(&serde_json::json!(null)).map_err(|e| {
                        ModuleError::OperationError(format!("Serialization error: {}", e))
                    })
                }
            }
            "submit_pow" => {
                let pool = self.pool.read().await;
                match pool.submit_pow(params.to_vec()).await {
                    Ok(accepted) => serde_json::to_vec(&serde_json::json!({ "accepted": accepted }))
                        .map_err(|e| {
                            ModuleError::OperationError(format!("Serialization error: {}", e))
                        }),
                    Err(e) => Err(ModuleError::OperationError(format!(
                        "submit_pow failed: {}",
                        e
                    ))),
                }
            }
            "get_pool_status" => {
                let pool = self.pool.read().await;
                let info = pool.pool_info();
                serde_json::to_vec(&info).map_err(|e| {
                    ModuleError::OperationError(format!("Serialization error: {}", e))
                })
            }
            "get_last_block" => {
                let pool = self.pool.read().await;
                let template = pool.current_template();
                let result = template.map(|b| {
                    serde_json::json!({
                        "prev_hash": hex::encode(b.header.prev_block_hash),
                        "merkle_root": hex::encode(b.header.merkle_root),
                        "timestamp": b.header.timestamp,
                        "bits": b.header.bits,
                        "tx_count": b.transactions.len()
                    })
                });
                serde_json::to_vec(&serde_json::json!({ "block": result })).map_err(|e| {
                    ModuleError::OperationError(format!("Serialization error: {}", e))
                })
            }
            _ => Err(ModuleError::OperationError(format!(
                "Unknown method: {}",
                method
            ))),
        }
    }

    fn list_methods(&self) -> Vec<String> {
        vec![
            "get_coinbase_payout".to_string(),
            "submit_pow".to_string(),
            "get_pool_status".to_string(),
            "get_last_block".to_string(),
        ]
    }

    fn api_version(&self) -> u32 {
        1
    }
}
