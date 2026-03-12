//! DATUM server implementation
//!
//! Main server that coordinates DATUM pool client.
//! Note: Miners connect via Stratum V2 (blvm-stratum-v2 module), not directly to this module.

use crate::api::DatumModuleApi;
use crate::error::DatumError;
use crate::pool::DatumPool;
use crate::template::BlockTemplateGenerator;
use blvm_node::module::ipc::protocol::{EventPayload, ModuleMessage};
use blvm_node::module::traits::EventType;
use blvm_node::module::traits::{ModuleContext, NodeAPI};
use blvm_protocol::Block;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// DATUM Gateway server
///
/// Handles DATUM pool communication and coordinates with Stratum V2 module
/// for miner connections. This module does NOT serve miners directly.
pub struct DatumServer {
    /// DATUM pool client
    pool: Arc<RwLock<DatumPool>>,
    /// Node API for querying node state
    node_api: Arc<dyn NodeAPI>,
    /// Block template generator
    template_generator: Arc<BlockTemplateGenerator>,
    /// Whether server is running
    running: Arc<RwLock<bool>>,
}

impl DatumServer {
    /// Create a new DATUM server
    pub async fn new(ctx: &ModuleContext, node_api: Arc<dyn NodeAPI>) -> Result<Self, DatumError> {
        let template_generator = Arc::new(BlockTemplateGenerator::new(Arc::clone(&node_api)));
        let pool = Arc::new(RwLock::new(DatumPool::new()));

        // Get pool configuration
        let pool_url = ctx.config.get("pool_url").cloned();
        let pool_username = ctx.config.get("pool_username").cloned();
        let pool_password = ctx.config.get("pool_password").cloned();
        let pool_public_key = ctx
            .config
            .get("pool_public_key")
            .and_then(|s| hex::decode(s).ok())
            .and_then(|bytes| {
                if bytes.len() == 32 {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&bytes);
                    Some(key)
                } else {
                    None
                }
            });

        // Connect to DATUM pool if configured
        if let (Some(url), Some(username), Some(password)) =
            (pool_url, pool_username, pool_password)
        {
            let mut pool_guard = pool.write().await;
            if let Some(pk) = pool_public_key {
                pool_guard.set_pool_public_key(pk);
            }
            pool_guard.connect(url, username, password).await?;
            info!("Connected to DATUM pool");
        } else {
            warn!("DATUM pool not configured - running in solo mode");
        }

        // Register module API for inter-module communication
        let pool_for_api = Arc::clone(&pool);
        let module_api = Arc::new(DatumModuleApi::new(pool_for_api));
        node_api
            .register_module_api(module_api)
            .await
            .map_err(|e| {
                DatumError::NodeApiError(format!("Failed to register module API: {}", e))
            })?;

        Ok(Self {
            pool,
            node_api,
            template_generator,
            running: Arc::new(RwLock::new(false)),
        })
    }

    /// Start the server
    pub async fn start(&self) -> Result<(), DatumError> {
        let mut running = self.running.write().await;
        *running = true;
        info!("DATUM Gateway server started (pool communication only)");
        info!("Note: Miners should connect via blvm-stratum-v2 module");
        Ok(())
    }

    /// Handle node events
    pub async fn handle_event(
        &self,
        event: &ModuleMessage,
        node_api: &dyn NodeAPI,
    ) -> Result<(), DatumError> {
        match event {
            ModuleMessage::Event(event_msg) => {
                match event_msg.event_type {
                    EventType::BlockTemplateUpdated => {
                        debug!("Block template updated, generating new template");
                        let template = self.template_generator.generate_template().await?;
                        self.update_block_template(template).await?;
                    }
                    EventType::NewBlock => {
                        debug!("New block mined, invalidating current template");
                        // Invalidate current template, wait for new one
                    }
                    EventType::ChainReorg => {
                        warn!("Chain reorganization detected, updating templates");
                        let template = self.template_generator.generate_template().await?;
                        self.update_block_template(template).await?;
                    }
                    _ => {
                        // Other events not handled
                    }
                }
            }
            _ => {
                // Other message types not handled
            }
        }
        Ok(())
    }

    /// Update block template and coordinate with pool
    async fn update_block_template(&self, template: Block) -> Result<(), DatumError> {
        let mut pool = self.pool.write().await;
        pool.set_template(template.clone()).await?;

        // Get coinbase payout requirements (if connected to pool)
        if let Some(coinbase_payout) = pool.get_coinbase_payout() {
            info!(
                "Coinbase payout requirements: {} outputs, tag: {}",
                coinbase_payout.outputs.len(),
                coinbase_payout.primary_tag
            );
            // TODO: Share coinbase requirements with Stratum V2 module if needed
            // This can be done via inter-module communication
        }

        info!("Updated block template in pool");
        Ok(())
    }

    /// Get coinbase payout requirements (for other modules)
    pub async fn get_coinbase_payout(&self) -> Option<crate::pool::CoinbasePayout> {
        let pool = self.pool.read().await;
        pool.get_coinbase_payout()
    }
}
