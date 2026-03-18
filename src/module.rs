//! DATUM module: unified CLI via #[module] macro.
//!
//! Replaces separate DatumCli with a single struct using #[command] methods.

use blvm_node::module::ipc::protocol::{EventMessage, ModuleMessage};
use blvm_sdk::module::prelude::*;
use blvm_sdk_macros::module;
use std::path::PathBuf;
use std::sync::Arc;

use crate::server::DatumServer;

/// DATUM module: server + CLI in one struct.
#[derive(Clone)]
pub struct DatumModule {
    pub server: Arc<DatumServer>,
    pub data_dir: PathBuf,
}

#[module]
impl DatumModule {
    /// Show datum module status (pool connection, running).
    #[command]
    fn status(&self, _ctx: &InvocationContext) -> Result<String, ModuleError> {
        let server = Arc::clone(&self.server);
        run_async(async move {
            let status = server.get_status().await;
            Ok::<_, String>(format!(
                "DATUM Gateway module\n\
                 Running: {}\n\
                 Pool connected: {}",
                status.running, status.pool_connected
            ))
        })
    }

    /// Show pool info (URL, coinbase, jobs, template).
    #[command]
    fn datum_info(&self, _ctx: &InvocationContext) -> Result<String, ModuleError> {
        let server = Arc::clone(&self.server);
        run_async(async move {
            let info = server.get_pool_info().await;
            Ok::<_, String>(format!(
                "DATUM pool info\n\
                 Pool URL: {}\n\
                 Pool connected: {}\n\
                 Has coinbase: {}\n\
                 Active jobs: {}\n\
                 Has template: {}",
                info.pool_url,
                info.pool_connected,
                info.has_coinbase,
                info.job_count,
                info.has_template
            ))
        })
    }

    /// Show pool connection status (URL, connected, jobs, template).
    #[command]
    fn pool_status(&self, _ctx: &InvocationContext) -> Result<String, ModuleError> {
        let server = Arc::clone(&self.server);
        run_async(async move {
            let info = server.get_pool_info().await;
            Ok::<_, String>(format!(
                "Pool: {} | Connected: {} | Jobs: {} | Template: {}",
                info.pool_url,
                info.pool_connected,
                info.job_count,
                info.has_template
            ))
        })
    }

    /// Reconnect to DATUM pool.
    #[command]
    fn reconnect(&self, _ctx: &InvocationContext) -> Result<String, ModuleError> {
        let server = Arc::clone(&self.server);
        run_async(async move {
            server
                .reconnect_pool()
                .await
                .map_err(|e| anyhow::anyhow!("Reconnect failed: {}", e))
        })
    }

    /// Print path to config file (config.toml).
    #[command]
    fn config_path(&self, _ctx: &InvocationContext) -> Result<String, ModuleError> {
        Ok(self.data_dir.join("config.toml").display().to_string())
    }

    #[on_event(BlockMined, BlockTemplateUpdated, MiningDifficultyChanged, NewBlock, ChainReorg, ShareSubmitted)]
    async fn on_mining_event(&self, event: &EventMessage, ctx: &InvocationContext) -> Result<(), ModuleError> {
        let msg = ModuleMessage::Event(event.clone());
        let api = ctx.node_api().expect("node_api required");
        self.server
            .handle_event(&msg, api.as_ref())
            .await
            .map_err(|e| ModuleError::Other(e.to_string().into()))
    }

    /// Manually submit PoW to pool (hex-encoded payload; for testing).
    #[command]
    fn submit_pow(&self, _ctx: &InvocationContext, payload: String) -> Result<String, ModuleError> {
        let hex_trimmed = payload.trim_start_matches("0x");
        let pow_data =
            hex::decode(hex_trimmed).map_err(|e| ModuleError::Other(e.to_string()))?;
        let server = Arc::clone(&self.server);
        run_async::<_, String, anyhow::Error>(async move {
            let accepted = server
                .submit_pow(pow_data)
                .await
                .map_err(|e| anyhow::anyhow!("submit-pow failed: {}", e))?;
            Ok(format!("Submitted: {}\n", if accepted { "accepted" } else { "rejected" }))
        })
    }
}
