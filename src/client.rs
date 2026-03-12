//! IPC client helper for module connection

use blvm_node::module::ipc::client::ModuleIpcClient;
use blvm_node::module::ipc::protocol::{
    EventMessage, LogLevel, ModuleMessage, RequestMessage, RequestPayload, ResponsePayload,
};
use blvm_node::module::traits::{EventType, ModuleError};
use futures::StreamExt;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Module client that handles IPC connection and event subscription
pub struct ModuleClient {
    ipc_client: Arc<tokio::sync::Mutex<ModuleIpcClient>>,
    module_id: String,
    module_name: String,
    version: String,
    event_receiver: mpsc::Receiver<ModuleMessage>,
}

impl ModuleClient {
    /// Connect to node and perform handshake
    pub async fn connect(
        socket_path: PathBuf,
        module_id: String,
        module_name: String,
        version: String,
    ) -> Result<Self, ModuleError> {
        info!(
            "Connecting to node IPC socket: {:?} (module: {})",
            socket_path, module_name
        );

        // Connect to IPC socket
        let mut ipc_client = ModuleIpcClient::connect(&socket_path).await?;
        info!("Connected to IPC socket");

        // Perform handshake
        let correlation_id = ipc_client.next_correlation_id();
        let handshake_request = RequestMessage {
            correlation_id,
            request_type: blvm_node::module::ipc::protocol::MessageType::Handshake,
            payload: RequestPayload::Handshake {
                module_id: module_id.clone(),
                module_name: module_name.clone(),
                version: version.clone(),
            },
        };

        let response = ipc_client.request(handshake_request).await?;
        match response.payload {
            Some(ResponsePayload::HandshakeAck { node_version }) => {
                info!(
                    "Handshake successful! Node version: {}, Module: {} v{}",
                    node_version, module_name, version
                );
            }
            _ => {
                return Err(ModuleError::IpcError(
                    "Invalid handshake response".to_string(),
                ));
            }
        }

        // Create event channel
        let (event_tx, event_rx) = mpsc::channel(1000);

        // Spawn event receiver task
        let ipc_client_arc = Arc::new(tokio::sync::Mutex::new(ipc_client));
        let ipc_client_for_events = Arc::clone(&ipc_client_arc);
        let module_id_for_events = module_id.clone();
        tokio::spawn(async move {
            loop {
                match ipc_client_for_events.lock().await.receive_event().await {
                    Ok(Some(ModuleMessage::Event(event))) => {
                        if event_tx.send(ModuleMessage::Event(event)).await.is_err() {
                            break; // Receiver dropped
                        }
                    }
                    Ok(Some(_)) => {
                        // Non-event message - ignore
                    }
                    Ok(None) => {
                        // No event available - continue
                    }
                    Err(e) => {
                        error!(
                            "Error receiving event for module {}: {}",
                            module_id_for_events, e
                        );
                        break;
                    }
                }
            }
        });

        Ok(Self {
            ipc_client: ipc_client_arc,
            module_id,
            module_name,
            version,
            event_receiver: event_rx,
        })
    }

    /// Subscribe to events
    pub async fn subscribe_events(
        &mut self,
        event_types: Vec<EventType>,
    ) -> Result<(), ModuleError> {
        let event_count = event_types.len();
        let mut client = self.ipc_client.lock().await;
        let correlation_id = client.next_correlation_id();
        let request = RequestMessage {
            correlation_id,
            request_type: blvm_node::module::ipc::protocol::MessageType::SubscribeEvents,
            payload: RequestPayload::SubscribeEvents { event_types },
        };
        let response = client.request(request).await?;
        if response.success {
            info!("Subscribed to {} event types", event_count);
            Ok(())
        } else {
            Err(ModuleError::OperationError(
                response
                    .error
                    .unwrap_or_else(|| "Subscription failed".to_string()),
            ))
        }
    }

    /// Get IPC client for NodeAPI
    pub fn get_ipc_client(&self) -> Arc<tokio::sync::Mutex<ModuleIpcClient>> {
        Arc::clone(&self.ipc_client)
    }

    /// Get event receiver
    pub fn event_receiver(&mut self) -> &mut mpsc::Receiver<ModuleMessage> {
        &mut self.event_receiver
    }
}
