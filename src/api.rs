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
        _params: &[u8],
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
            _ => Err(ModuleError::OperationError(format!(
                "Unknown method: {}",
                method
            ))),
        }
    }

    fn list_methods(&self) -> Vec<String> {
        vec!["get_coinbase_payout".to_string()]
    }

    fn api_version(&self) -> u32 {
        1
    }
}
