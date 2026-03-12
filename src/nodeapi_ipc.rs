//! NodeAPI IPC wrapper for DATUM module
//!
//! Provides NodeAPI trait implementation over IPC for the DATUM module.

use blvm_node::module::inter_module::api::ModuleAPI;
use blvm_node::module::ipc::client::ModuleIpcClient;
use blvm_node::module::ipc::protocol::{
    EventPayload, MessageType, RequestMessage, RequestPayload, ResponsePayload,
};
use blvm_node::module::traits::EventType;
use blvm_node::module::traits::{ModuleError, NodeAPI, SubmitBlockResult};
use blvm_protocol::{Block, BlockHeader, Hash, OutPoint, Transaction, UTXO};
use std::sync::Arc;
use tokio::sync::Mutex;

/// NodeAPI implementation over IPC
pub struct NodeApiIpc {
    ipc_client: Arc<Mutex<ModuleIpcClient>>,
    correlation_id: Arc<Mutex<u64>>,
}

impl NodeApiIpc {
    /// Create a new NodeAPI IPC wrapper
    pub fn new(ipc_client: Arc<Mutex<ModuleIpcClient>>) -> Self {
        Self {
            ipc_client,
            correlation_id: Arc::new(Mutex::new(0)),
        }
    }

    /// Get next correlation ID
    async fn next_correlation_id(&self) -> u64 {
        let mut id = self.correlation_id.lock().await;
        *id += 1;
        *id
    }

    /// Helper method to make IPC requests
    async fn request<T, F>(&self, payload: RequestPayload, mapper: F) -> Result<T, ModuleError>
    where
        F: FnOnce(ResponsePayload) -> Result<T, ModuleError>,
    {
        let correlation_id = self.next_correlation_id().await;

        // Infer MessageType from payload
        let request_type = match &payload {
            RequestPayload::GetBlockTemplate { .. } => MessageType::GetBlockTemplate,
            RequestPayload::SubmitBlock { .. } => MessageType::SubmitBlock,
            RequestPayload::GetBlock { .. } => MessageType::GetBlock,
            RequestPayload::GetChainTip => MessageType::GetChainTip,
            RequestPayload::GetBlockHeight => MessageType::GetBlockHeight,
            RequestPayload::GetMempoolTransactions => MessageType::GetMempoolTransactions,
            RequestPayload::GetMempoolTransaction { .. } => MessageType::GetMempoolTransaction,
            _ => {
                return Err(ModuleError::OperationError(
                    "Unsupported request payload".to_string(),
                ))
            }
        };

        let request = RequestMessage {
            correlation_id,
            request_type,
            payload,
        };

        let response = self.ipc_client.lock().await.request(request).await?;

        if !response.success {
            return Err(ModuleError::OperationError(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        match response.payload {
            Some(payload) => mapper(payload),
            None => Err(ModuleError::OperationError(
                "Empty response payload".to_string(),
            )),
        }
    }
}

#[async_trait::async_trait]
impl NodeAPI for NodeApiIpc {
    async fn get_block(&self, hash: &Hash) -> Result<Option<Block>, ModuleError> {
        self.request(
            RequestPayload::GetBlock { hash: *hash },
            |payload| match payload {
                ResponsePayload::Block(block) => Ok(block),
                _ => Ok(None),
            },
        )
        .await
    }

    async fn get_block_header(&self, hash: &Hash) -> Result<Option<BlockHeader>, ModuleError> {
        // Implementation similar to get_block
        self.get_block(hash).await.map(|opt| opt.map(|b| b.header))
    }

    async fn get_transaction(&self, hash: &Hash) -> Result<Option<Transaction>, ModuleError> {
        self.request(
            RequestPayload::GetTransaction { hash: *hash },
            |payload| match payload {
                ResponsePayload::Transaction(tx) => Ok(tx),
                _ => Ok(None),
            },
        )
        .await
    }

    async fn has_transaction(&self, hash: &Hash) -> Result<bool, ModuleError> {
        self.get_transaction(hash).await.map(|opt| opt.is_some())
    }

    async fn get_chain_tip(&self) -> Result<Hash, ModuleError> {
        self.request(RequestPayload::GetChainTip, |payload| match payload {
            ResponsePayload::Hash(hash) => Ok(hash),
            _ => Err(ModuleError::OperationError(
                "Unexpected response type".to_string(),
            )),
        })
        .await
    }

    async fn get_block_height(&self) -> Result<u64, ModuleError> {
        self.request(RequestPayload::GetBlockHeight, |payload| match payload {
            ResponsePayload::U64(height) => Ok(height),
            _ => Err(ModuleError::OperationError(
                "Unexpected response type".to_string(),
            )),
        })
        .await
    }

    async fn get_utxo(&self, outpoint: &OutPoint) -> Result<Option<UTXO>, ModuleError> {
        // OutPoint implements Clone but not Copy
        let outpoint_copy = outpoint.clone();
        self.request(
            RequestPayload::GetUtxo {
                outpoint: outpoint_copy,
            },
            |payload| match payload {
                ResponsePayload::Utxo(utxo) => Ok(utxo),
                _ => Ok(None),
            },
        )
        .await
    }

    async fn subscribe_events(
        &self,
        event_types: Vec<EventType>,
    ) -> Result<
        tokio::sync::mpsc::Receiver<blvm_node::module::ipc::protocol::ModuleMessage>,
        ModuleError,
    > {
        // This is handled by ModuleClient, not directly here
        Err(ModuleError::OperationError(
            "Use ModuleClient for event subscription".to_string(),
        ))
    }

    async fn get_mempool_transactions(&self) -> Result<Vec<Hash>, ModuleError> {
        self.request(
            RequestPayload::GetMempoolTransactions,
            |payload| match payload {
                ResponsePayload::MempoolTransactions(txs) => Ok(txs),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn get_mempool_transaction(
        &self,
        tx_hash: &Hash,
    ) -> Result<Option<Transaction>, ModuleError> {
        self.request(
            RequestPayload::GetMempoolTransaction { tx_hash: *tx_hash },
            |payload| match payload {
                ResponsePayload::MempoolTransaction(tx) => Ok(tx),
                _ => Ok(None),
            },
        )
        .await
    }

    async fn get_mempool_size(
        &self,
    ) -> Result<blvm_node::module::traits::MempoolSize, ModuleError> {
        self.request(RequestPayload::GetMempoolSize, |payload| match payload {
            ResponsePayload::MempoolSize(size) => Ok(size),
            _ => Err(ModuleError::OperationError(
                "Unexpected response type".to_string(),
            )),
        })
        .await
    }

    async fn get_network_stats(
        &self,
    ) -> Result<blvm_node::module::traits::NetworkStats, ModuleError> {
        self.request(RequestPayload::GetNetworkStats, |payload| match payload {
            ResponsePayload::NetworkStats(stats) => Ok(stats),
            _ => Err(ModuleError::OperationError(
                "Unexpected response type".to_string(),
            )),
        })
        .await
    }

    async fn get_network_peers(
        &self,
    ) -> Result<Vec<blvm_node::module::traits::PeerInfo>, ModuleError> {
        self.request(RequestPayload::GetNetworkPeers, |payload| match payload {
            ResponsePayload::NetworkPeers(peers) => Ok(peers),
            _ => Err(ModuleError::OperationError(
                "Unexpected response type".to_string(),
            )),
        })
        .await
    }

    async fn get_chain_info(&self) -> Result<blvm_node::module::traits::ChainInfo, ModuleError> {
        self.request(RequestPayload::GetChainInfo, |payload| match payload {
            ResponsePayload::ChainInfo(info) => Ok(info),
            _ => Err(ModuleError::OperationError(
                "Unexpected response type".to_string(),
            )),
        })
        .await
    }

    async fn get_block_by_height(&self, height: u64) -> Result<Option<Block>, ModuleError> {
        self.request(
            RequestPayload::GetBlockByHeight { height },
            |payload| match payload {
                ResponsePayload::BlockByHeight(block) => Ok(block),
                _ => Ok(None),
            },
        )
        .await
    }

    async fn get_lightning_node_url(&self) -> Result<Option<String>, ModuleError> {
        self.request(
            RequestPayload::GetLightningNodeUrl,
            |payload| match payload {
                ResponsePayload::LightningNodeUrl(url) => Ok(url),
                _ => Ok(None),
            },
        )
        .await
    }

    async fn get_lightning_info(
        &self,
    ) -> Result<Option<blvm_node::module::traits::LightningInfo>, ModuleError> {
        self.request(RequestPayload::GetLightningInfo, |payload| match payload {
            ResponsePayload::LightningInfo(info) => Ok(info),
            _ => Ok(None),
        })
        .await
    }

    async fn get_payment_state(
        &self,
        payment_id: &str,
    ) -> Result<Option<blvm_node::module::traits::PaymentState>, ModuleError> {
        self.request(
            RequestPayload::GetPaymentState {
                payment_id: payment_id.to_string(),
            },
            |payload| match payload {
                ResponsePayload::PaymentState(state) => Ok(state),
                _ => Ok(None),
            },
        )
        .await
    }

    async fn check_transaction_in_mempool(&self, tx_hash: &Hash) -> Result<bool, ModuleError> {
        self.request(
            RequestPayload::CheckTransactionInMempool { tx_hash: *tx_hash },
            |payload| match payload {
                ResponsePayload::CheckTransactionInMempool(exists) => Ok(exists),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn get_fee_estimate(&self, target_blocks: u32) -> Result<u64, ModuleError> {
        self.request(
            RequestPayload::GetFeeEstimate { target_blocks },
            |payload| match payload {
                ResponsePayload::FeeEstimate(fee) => Ok(fee),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn register_rpc_endpoint(
        &self,
        method: String,
        description: String,
    ) -> Result<(), ModuleError> {
        self.request(
            RequestPayload::RegisterRpcEndpoint {
                method,
                description,
            },
            |payload| match payload {
                ResponsePayload::RpcEndpointRegistered => Ok(()),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn unregister_rpc_endpoint(&self, method: &str) -> Result<(), ModuleError> {
        self.request(
            RequestPayload::UnregisterRpcEndpoint {
                method: method.to_string(),
            },
            |payload| match payload {
                ResponsePayload::RpcEndpointUnregistered => Ok(()),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn register_timer(
        &self,
        interval_seconds: u64,
        callback: Arc<dyn blvm_node::module::timers::manager::TimerCallback>,
    ) -> Result<blvm_node::module::timers::manager::TimerId, ModuleError> {
        // Timer registration handled differently - would need callback serialization
        Err(ModuleError::OperationError(
            "Timer registration not supported via IPC".to_string(),
        ))
    }

    async fn cancel_timer(
        &self,
        timer_id: blvm_node::module::timers::manager::TimerId,
    ) -> Result<(), ModuleError> {
        self.request(
            RequestPayload::CancelTimer { timer_id },
            |payload| match payload {
                ResponsePayload::TimerCancelled => Ok(()),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn schedule_task(
        &self,
        delay_seconds: u64,
        callback: Arc<dyn blvm_node::module::timers::manager::TaskCallback>,
    ) -> Result<blvm_node::module::timers::manager::TaskId, ModuleError> {
        // Task scheduling handled differently
        Err(ModuleError::OperationError(
            "Task scheduling not supported via IPC".to_string(),
        ))
    }

    async fn report_metric(
        &self,
        metric: blvm_node::module::metrics::manager::Metric,
    ) -> Result<(), ModuleError> {
        self.request(
            RequestPayload::ReportMetric { metric },
            |payload| match payload {
                ResponsePayload::MetricReported => Ok(()),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn get_module_metrics(
        &self,
        module_id: &str,
    ) -> Result<Vec<blvm_node::module::metrics::manager::Metric>, ModuleError> {
        self.request(
            RequestPayload::GetModuleMetrics {
                module_id: module_id.to_string(),
            },
            |payload| match payload {
                ResponsePayload::ModuleMetrics(metrics) => Ok(metrics),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn get_all_metrics(
        &self,
    ) -> Result<
        std::collections::HashMap<String, Vec<blvm_node::module::metrics::manager::Metric>>,
        ModuleError,
    > {
        self.request(RequestPayload::GetAllMetrics, |payload| match payload {
            ResponsePayload::AllMetrics(metrics) => Ok(metrics),
            _ => Err(ModuleError::OperationError(
                "Unexpected response type".to_string(),
            )),
        })
        .await
    }

    async fn read_file(&self, path: String) -> Result<Vec<u8>, ModuleError> {
        self.request(RequestPayload::ReadFile { path }, |payload| match payload {
            ResponsePayload::FileData(data) => Ok(data),
            _ => Err(ModuleError::OperationError(
                "Unexpected response type".to_string(),
            )),
        })
        .await
    }

    async fn write_file(&self, path: String, data: Vec<u8>) -> Result<(), ModuleError> {
        self.request(
            RequestPayload::WriteFile { path, data },
            |payload| match payload {
                ResponsePayload::Bool(true) => Ok(()),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn delete_file(&self, path: String) -> Result<(), ModuleError> {
        self.request(
            RequestPayload::DeleteFile { path },
            |payload| match payload {
                ResponsePayload::Bool(true) => Ok(()),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn list_directory(&self, path: String) -> Result<Vec<String>, ModuleError> {
        self.request(
            RequestPayload::ListDirectory { path },
            |payload| match payload {
                ResponsePayload::DirectoryListing(entries) => Ok(entries),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn create_directory(&self, path: String) -> Result<(), ModuleError> {
        self.request(
            RequestPayload::CreateDirectory { path },
            |payload| match payload {
                ResponsePayload::Bool(true) => Ok(()),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn get_file_metadata(
        &self,
        path: String,
    ) -> Result<blvm_node::module::ipc::protocol::FileMetadata, ModuleError> {
        self.request(
            RequestPayload::GetFileMetadata { path },
            |payload| match payload {
                ResponsePayload::FileMetadata(metadata) => Ok(metadata),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn storage_open_tree(&self, name: String) -> Result<String, ModuleError> {
        self.request(
            RequestPayload::StorageOpenTree { name },
            |payload| match payload {
                ResponsePayload::StorageTreeId(tree_id) => Ok(tree_id),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn storage_insert(
        &self,
        tree_id: String,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<(), ModuleError> {
        self.request(
            RequestPayload::StorageInsert {
                tree_id,
                key,
                value,
            },
            |payload| match payload {
                ResponsePayload::Bool(true) => Ok(()),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn storage_get(
        &self,
        tree_id: String,
        key: Vec<u8>,
    ) -> Result<Option<Vec<u8>>, ModuleError> {
        self.request(
            RequestPayload::StorageGet { tree_id, key },
            |payload| match payload {
                ResponsePayload::StorageValue(value) => Ok(value),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn storage_remove(&self, tree_id: String, key: Vec<u8>) -> Result<(), ModuleError> {
        self.request(
            RequestPayload::StorageRemove { tree_id, key },
            |payload| match payload {
                ResponsePayload::Bool(true) => Ok(()),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn storage_contains_key(
        &self,
        tree_id: String,
        key: Vec<u8>,
    ) -> Result<bool, ModuleError> {
        self.request(
            RequestPayload::StorageContainsKey { tree_id, key },
            |payload| match payload {
                ResponsePayload::Bool(exists) => Ok(exists),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn storage_iter(&self, tree_id: String) -> Result<Vec<(Vec<u8>, Vec<u8>)>, ModuleError> {
        self.request(
            RequestPayload::StorageIter { tree_id },
            |payload| match payload {
                ResponsePayload::StorageKeyValuePairs(pairs) => Ok(pairs),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn storage_transaction(
        &self,
        tree_id: String,
        operations: Vec<blvm_node::module::ipc::protocol::StorageOperation>,
    ) -> Result<(), ModuleError> {
        self.request(
            RequestPayload::StorageTransaction {
                tree_id,
                operations,
            },
            |payload| match payload {
                ResponsePayload::Bool(true) => Ok(()),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn discover_modules(
        &self,
    ) -> Result<Vec<blvm_node::module::traits::ModuleInfo>, ModuleError> {
        self.request(RequestPayload::DiscoverModules, |payload| match payload {
            ResponsePayload::ModuleList(modules) => Ok(modules),
            _ => Err(ModuleError::OperationError(
                "Unexpected response type".to_string(),
            )),
        })
        .await
    }

    async fn get_module_info(
        &self,
        module_id: &str,
    ) -> Result<Option<blvm_node::module::traits::ModuleInfo>, ModuleError> {
        self.request(
            RequestPayload::GetModuleInfo {
                module_id: module_id.to_string(),
            },
            |payload| match payload {
                ResponsePayload::ModuleInfo(info) => Ok(info),
                _ => Ok(None),
            },
        )
        .await
    }

    async fn is_module_available(&self, module_id: &str) -> Result<bool, ModuleError> {
        self.request(
            RequestPayload::IsModuleAvailable {
                module_id: module_id.to_string(),
            },
            |payload| match payload {
                ResponsePayload::ModuleAvailable(available) => Ok(available),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn publish_event(
        &self,
        event_type: EventType,
        payload: EventPayload,
    ) -> Result<(), ModuleError> {
        self.request(
            RequestPayload::PublishEvent {
                event_type,
                payload,
            },
            |payload| match payload {
                ResponsePayload::EventPublished => Ok(()),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn call_module(
        &self,
        target_module_id: Option<&str>,
        method: &str,
        params: Vec<u8>,
    ) -> Result<Vec<u8>, ModuleError> {
        self.request(
            RequestPayload::CallModule {
                target_module_id: target_module_id.map(|s| s.to_string()),
                method: method.to_string(),
                params,
            },
            |payload| match payload {
                ResponsePayload::ModuleApiResponse(response) => Ok(response),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn register_module_api(&self, api: Arc<dyn ModuleAPI>) -> Result<(), ModuleError> {
        // Module API registration handled differently
        Err(ModuleError::OperationError(
            "Module API registration not supported via IPC".to_string(),
        ))
    }

    async fn unregister_module_api(&self) -> Result<(), ModuleError> {
        self.request(
            RequestPayload::UnregisterModuleApi,
            |payload| match payload {
                ResponsePayload::ModuleApiUnregistered => Ok(()),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn send_mesh_packet_to_peer(
        &self,
        peer_addr: String,
        packet_data: Vec<u8>,
    ) -> Result<(), ModuleError> {
        self.request(
            RequestPayload::SendMeshPacketToPeer {
                peer_addr,
                packet_data,
            },
            |payload| match payload {
                ResponsePayload::Bool(true) => Ok(()),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn send_stratum_v2_message_to_peer(
        &self,
        peer_addr: String,
        message_data: Vec<u8>,
    ) -> Result<(), ModuleError> {
        self.request(
            RequestPayload::SendStratumV2MessageToPeer {
                peer_addr,
                message_data,
            },
            |payload| match payload {
                ResponsePayload::Bool(true) => Ok(()),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn get_module_health(
        &self,
        module_id: &str,
    ) -> Result<Option<blvm_node::module::process::monitor::ModuleHealth>, ModuleError> {
        self.request(
            RequestPayload::GetModuleHealth {
                module_id: module_id.to_string(),
            },
            |payload| match payload {
                ResponsePayload::ModuleHealth(health) => Ok(health),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn get_all_module_health(
        &self,
    ) -> Result<Vec<(String, blvm_node::module::process::monitor::ModuleHealth)>, ModuleError> {
        self.request(
            RequestPayload::GetAllModuleHealth,
            |payload| match payload {
                ResponsePayload::AllModuleHealth(health) => Ok(health),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn report_module_health(
        &self,
        health: blvm_node::module::process::monitor::ModuleHealth,
    ) -> Result<(), ModuleError> {
        self.request(
            RequestPayload::ReportModuleHealth { health },
            |payload| match payload {
                ResponsePayload::HealthReported => Ok(()),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn get_block_template(
        &self,
        rules: Vec<String>,
        coinbase_script: Option<Vec<u8>>,
        coinbase_address: Option<String>,
    ) -> Result<blvm_protocol::mining::BlockTemplate, ModuleError> {
        self.request(
            RequestPayload::GetBlockTemplate {
                rules,
                coinbase_script,
                coinbase_address,
            },
            |payload| match payload {
                ResponsePayload::BlockTemplate(template) => Ok(template),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn submit_block(&self, block: Block) -> Result<SubmitBlockResult, ModuleError> {
        self.request(
            RequestPayload::SubmitBlock { block },
            |payload| match payload {
                ResponsePayload::SubmitBlockResult(result) => Ok(result),
                _ => Err(ModuleError::OperationError(
                    "Unexpected response type".to_string(),
                )),
            },
        )
        .await
    }

    async fn initialize_module(
        &self,
        _module_id: String,
        _module_data_dir: std::path::PathBuf,
        _base_data_dir: std::path::PathBuf,
    ) -> Result<(), ModuleError> {
        // Module initialization is handled automatically during handshake
        // This method is a no-op for IPC-based modules
        Ok(())
    }

    async fn send_mesh_packet_to_module(
        &self,
        _module_id: &str,
        _packet_data: Vec<u8>,
        _peer_addr: String,
    ) -> Result<(), ModuleError> {
        // This method is for the node to send packets to modules, not for modules to call over IPC.
        // Modules should use send_mesh_packet_to_peer() instead.
        Err(ModuleError::OperationError(
            "send_mesh_packet_to_module is not available over IPC - use send_mesh_packet_to_peer instead".to_string(),
        ))
    }
}
