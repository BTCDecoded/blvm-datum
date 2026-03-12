//! DATUM protocol message handlers
//!
//! Handles incoming protocol messages from DATUM pools

use crate::error::DatumError;
use crate::messages::DatumCommand;
use blvm_protocol::Block;
use tracing::{debug, info, warn};

/// Client configuration from pool
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Maximum job count
    pub max_jobs: u8,
    /// Job timeout in seconds
    pub job_timeout: u32,
    /// Share difficulty
    pub share_difficulty: u64,
    /// Other configuration parameters
    pub params: Vec<u8>,
}

/// Job validation result
#[derive(Debug, Clone)]
pub enum JobValidationResult {
    /// Job is valid
    Valid,
    /// Job is invalid
    Invalid(String),
    /// Job needs update
    NeedsUpdate,
}

/// Share submission result
#[derive(Debug, Clone)]
pub struct ShareResponse {
    /// Whether share was accepted
    pub accepted: bool,
    /// Share difficulty
    pub difficulty: u64,
    /// Message from pool
    pub message: String,
}

/// Block notification
#[derive(Debug, Clone)]
pub struct BlockNotify {
    /// Block height
    pub height: u32,
    /// Block hash
    pub hash: [u8; 32],
    /// Previous block hash
    pub prev_hash: [u8; 32],
}

/// Message handler trait for DATUM protocol messages
pub trait MessageHandler: Send + Sync {
    /// Handle client configuration message (0x99)
    fn handle_client_config(&self, config: ClientConfig) -> Result<(), DatumError> {
        debug!(
            "Received client config: max_jobs={}, job_timeout={}",
            config.max_jobs, config.job_timeout
        );
        Ok(())
    }

    /// Handle job validation message (0x50)
    fn handle_job_validation(
        &self,
        job_id: u8,
        result: JobValidationResult,
    ) -> Result<(), DatumError> {
        match &result {
            JobValidationResult::Valid => {
                debug!("Job {} validated successfully", job_id);
            }
            JobValidationResult::Invalid(reason) => {
                warn!("Job {} invalid: {}", job_id, reason);
            }
            JobValidationResult::NeedsUpdate => {
                info!("Job {} needs update", job_id);
            }
        }
        Ok(())
    }

    /// Handle share response message (0x8F)
    fn handle_share_response(&self, response: ShareResponse) -> Result<(), DatumError> {
        if response.accepted {
            info!(
                "Share accepted: difficulty={}, message={}",
                response.difficulty, response.message
            );
        } else {
            warn!("Share rejected: {}", response.message);
        }
        Ok(())
    }

    /// Handle block notify message (0xF9)
    fn handle_block_notify(&self, notify: BlockNotify) -> Result<(), DatumError> {
        info!(
            "Block notify: height={}, hash={:?}",
            notify.height,
            hex::encode(notify.hash)
        );
        Ok(())
    }
}

/// Default message handler implementation
pub struct DefaultMessageHandler;

impl MessageHandler for DefaultMessageHandler {}

/// Parse client configuration message (0x99)
pub fn parse_client_config(data: &[u8]) -> Result<ClientConfig, DatumError> {
    if data.len() < 13 {
        return Err(DatumError::ProtocolError(
            "Client config message too short".to_string(),
        ));
    }

    let max_jobs = data[0];
    let job_timeout = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
    let share_difficulty = u64::from_le_bytes([
        data[5], data[6], data[7], data[8], data[9], data[10], data[11], data[12],
    ]);
    let params = if data.len() > 13 {
        data[13..].to_vec()
    } else {
        vec![]
    };

    Ok(ClientConfig {
        max_jobs,
        job_timeout,
        share_difficulty,
        params,
    })
}

/// Parse job validation message (0x50)
pub fn parse_job_validation(data: &[u8]) -> Result<(u8, JobValidationResult), DatumError> {
    if data.is_empty() {
        return Err(DatumError::ProtocolError(
            "Job validation message empty".to_string(),
        ));
    }

    let job_id = data[0];
    let result = if data.len() > 1 {
        match data[1] {
            0 => JobValidationResult::Valid,
            1 => {
                let reason = if data.len() > 2 {
                    String::from_utf8_lossy(&data[2..]).to_string()
                } else {
                    "Unknown error".to_string()
                };
                JobValidationResult::Invalid(reason)
            }
            2 => JobValidationResult::NeedsUpdate,
            _ => JobValidationResult::Invalid("Unknown status code".to_string()),
        }
    } else {
        JobValidationResult::Valid
    };

    Ok((job_id, result))
}

/// Parse share response message (0x8F)
pub fn parse_share_response(data: &[u8]) -> Result<ShareResponse, DatumError> {
    if data.len() < 9 {
        return Err(DatumError::ProtocolError(
            "Share response message too short".to_string(),
        ));
    }

    let accepted = data[0] != 0;
    let difficulty = u64::from_le_bytes([
        data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8],
    ]);
    let message = if data.len() > 9 {
        String::from_utf8_lossy(&data[9..]).to_string()
    } else {
        String::new()
    };

    Ok(ShareResponse {
        accepted,
        difficulty,
        message,
    })
}

/// Parse block notify message (0xF9)
pub fn parse_block_notify(data: &[u8]) -> Result<BlockNotify, DatumError> {
    if data.len() < 72 {
        return Err(DatumError::ProtocolError(
            "Block notify message too short".to_string(),
        ));
    }

    let height = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);

    let mut hash = [0u8; 32];
    hash.copy_from_slice(&data[4..36]);

    let mut prev_hash = [0u8; 32];
    prev_hash.copy_from_slice(&data[36..68]);

    Ok(BlockNotify {
        height,
        hash,
        prev_hash,
    })
}
