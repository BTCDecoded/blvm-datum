//! blvm-datum - DATUM Gateway mining protocol module
//!
//! When spawned by the node: reads MODULE_ID, SOCKET_PATH, DATA_DIR from env.
//! For manual testing: blvm-datum --module-id <id> --socket-path <path> --data-dir <dir>

use anyhow::Result;
use blvm_datum::{DatumConfig, DatumModule, DatumServer};
use blvm_sdk::module::{ModuleBootstrap, ModuleDb};
use std::sync::Arc;
use tracing::error;

const MODULE_NAME: &str = "blvm-datum";

#[tokio::main]
async fn main() -> Result<()> {
    let bootstrap = ModuleBootstrap::init_module(MODULE_NAME);
    let db = ModuleDb::open(&bootstrap.data_dir)?;

    let setup = |node_api: Arc<dyn blvm_node::module::traits::NodeAPI>,
                 _db: Arc<dyn blvm_node::storage::database::Database>,
                 data_dir: &std::path::Path| {
        let bootstrap = bootstrap.clone();
        let data_dir = data_dir.to_path_buf();
        async move {
            let (ctx, _config) = bootstrap.context_with_config::<DatumConfig>(&data_dir);

            let server = DatumServer::new(&ctx, Arc::clone(&node_api))
                .await
                .map_err(|e| {
                    blvm_node::module::traits::ModuleError::Other(format!(
                        "Failed to create server: {}",
                        e
                    ))
                })?;
            if let Err(e) = server.start().await {
                error!("Failed to start DATUM server: {}", e);
                return Err(blvm_node::module::traits::ModuleError::Other(format!(
                    "Server startup failed: {}",
                    e
                )));
            }
            tracing::info!("DATUM Gateway module initialized and running");

            let server = Arc::new(server);
            let module = DatumModule {
                server: Arc::clone(&server),
                data_dir,
            };
            Ok((module.clone(), module))
        }
    };

    blvm_sdk::run_module! {
        bootstrap: &bootstrap,
        module_name: MODULE_NAME,
        module_type: DatumModule,
        cli_type: DatumModule,
        db: db.as_db(),
        setup: setup,
        event_types: DatumModule::event_types(),
    }?;

    Ok(())
}
