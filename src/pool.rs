//! DATUM pool management
//!
//! Manages connection to DATUM pool and coordinates coinbase payouts.
//! Provides coinbase requirements that can be used by other modules (e.g., Stratum V2).

use crate::error::DatumError;
use crate::datum_protocol::DatumProtocolClient;
use blvm_protocol::Block;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Coinbase payout information from DATUM pool
#[derive(Debug, Clone)]
pub struct CoinbasePayout {
    /// Output scripts for coinbase transaction
    pub outputs: Vec<CoinbaseOutput>,
    /// Primary coinbase tag
    pub primary_tag: String,
    /// Unique identifier for this payout
    pub unique_id: String,
}

/// Coinbase output specification
#[derive(Debug, Clone)]
pub struct CoinbaseOutput {
    /// Output script (address)
    pub script: Vec<u8>,
    /// Value in sats
    pub value: u64,
}

/// DATUM pool connection
pub struct DatumPool {
    /// Pool URL
    pool_url: Option<String>,
    /// Pool username
    pool_username: Option<String>,
    /// Pool password
    pool_password: Option<String>,
    /// Pool's long-term X25519 public key (for crypto_box_seal)
    pool_public_key: Option<[u8; 32]>,
    /// DATUM protocol client
    protocol_client: Option<Arc<DatumProtocolClient>>,
    /// Current coinbase payout requirements
    current_coinbase: Option<CoinbasePayout>,
    /// Current block template
    current_template: Option<Block>,
    /// Active jobs
    jobs: HashMap<u8, Block>, // job_id -> block template
}

impl DatumPool {
    /// Create a new DATUM pool
    pub fn new() -> Self {
        Self {
            pool_url: None,
            pool_username: None,
            pool_password: None,
            pool_public_key: None,
            protocol_client: None,
            current_coinbase: None,
            current_template: None,
            jobs: HashMap::new(),
        }
    }

    /// Set pool public key (for crypto_box_seal encryption)
    pub fn set_pool_public_key(&mut self, key: [u8; 32]) {
        self.pool_public_key = Some(key);
    }

    /// Connect to DATUM pool
    pub async fn connect(&mut self, url: String, username: String, password: String) -> Result<(), DatumError> {
        self.pool_url = Some(url.clone());
        self.pool_username = Some(username);
        self.pool_password = Some(password);
        
        // Create and connect DATUM protocol client (now uses interior mutability)
        let client = if let Some(pool_pk) = self.pool_public_key {
            DatumProtocolClient::new_with_pool_key(url, pool_pk)
        } else {
            DatumProtocolClient::new(url)
        };
        client.connect().await?;
        self.protocol_client = Some(Arc::new(client));
        
        info!("Connected to DATUM pool");
        Ok(())
    }

    /// Fetch coinbase payout requirements from pool
    pub async fn fetch_coinbase_payout(&mut self, coinbase_value: u64) -> Result<CoinbasePayout, DatumError> {
        let client = self.protocol_client.as_ref()
            .ok_or_else(|| DatumError::PoolConnectionError("Not connected to pool".to_string()))?;
        
        // Fetch coinbase payout information from pool (client now uses interior mutability)
        let coinbase_data = client.fetch_coinbaser(coinbase_value).await?;
        
        // Parse coinbase_data into CoinbasePayout structure
        // Format: [value(8)][length(4)][coinbase_data(length)][0xFE]
        let payout = Self::parse_coinbase_response(&coinbase_data)?;
        
        self.current_coinbase = Some(payout.clone());
        info!("Fetched coinbase payout from pool: {} outputs, tag: {}", 
              payout.outputs.len(), payout.primary_tag);
        Ok(payout)
    }

    /// Parse coinbase payout response from pool
    /// Format: [value(8)][length(4)][coinbase_data(length)][0xFE]
    /// coinbase_data format: [primary_tag_len(1)][primary_tag][unique_id_len(1)][unique_id][output_count(1)][outputs...]
    /// Each output: [value(8)][script_len(1)][script(script_len)]
    pub fn parse_coinbase_response(data: &[u8]) -> Result<CoinbasePayout, DatumError> {
        if data.len() < 13 {
            return Err(DatumError::ProtocolError("Coinbase response too short".to_string()));
        }

        let mut offset = 0;

        // Read value (8 bytes, little-endian)
        if offset + 8 > data.len() {
            return Err(DatumError::ProtocolError("Invalid coinbase response format".to_string()));
        }
        let _value = u64::from_le_bytes([
            data[offset], data[offset+1], data[offset+2], data[offset+3],
            data[offset+4], data[offset+5], data[offset+6], data[offset+7],
        ]);
        offset += 8;

        // Read length (4 bytes, little-endian)
        if offset + 4 > data.len() {
            return Err(DatumError::ProtocolError("Invalid coinbase response format".to_string()));
        }
        let coinbase_data_len = u32::from_le_bytes([
            data[offset], data[offset+1], data[offset+2], data[offset+3],
        ]) as usize;
        offset += 4;

        // Check for terminator and validate length
        if offset + coinbase_data_len > data.len() {
            return Err(DatumError::ProtocolError("Coinbase data length exceeds buffer".to_string()));
        }

        // Verify terminator (0xFE) at end
        if data[offset + coinbase_data_len] != 0xFE {
            return Err(DatumError::ProtocolError("Missing coinbase response terminator".to_string()));
        }

        // Parse coinbase_data
        let coinbase_data = &data[offset..offset + coinbase_data_len];
        let mut data_offset = 0;

        // Read primary tag
        if data_offset >= coinbase_data.len() {
            return Err(DatumError::ProtocolError("Coinbase data too short for primary tag".to_string()));
        }
        let primary_tag_len = coinbase_data[data_offset] as usize;
        data_offset += 1;
        
        if data_offset + primary_tag_len > coinbase_data.len() {
            return Err(DatumError::ProtocolError("Primary tag length exceeds data".to_string()));
        }
        let primary_tag = String::from_utf8_lossy(&coinbase_data[data_offset..data_offset + primary_tag_len]).to_string();
        data_offset += primary_tag_len;

        // Read unique ID
        if data_offset >= coinbase_data.len() {
            return Err(DatumError::ProtocolError("Coinbase data too short for unique ID".to_string()));
        }
        let unique_id_len = coinbase_data[data_offset] as usize;
        data_offset += 1;
        
        if data_offset + unique_id_len > coinbase_data.len() {
            return Err(DatumError::ProtocolError("Unique ID length exceeds data".to_string()));
        }
        let unique_id = String::from_utf8_lossy(&coinbase_data[data_offset..data_offset + unique_id_len]).to_string();
        data_offset += unique_id_len;

        // Read output count
        if data_offset >= coinbase_data.len() {
            return Err(DatumError::ProtocolError("Coinbase data too short for output count".to_string()));
        }
        let output_count = coinbase_data[data_offset] as usize;
        data_offset += 1;

        // Parse outputs
        let mut outputs = Vec::new();
        for _ in 0..output_count {
            // Read value (8 bytes)
            if data_offset + 8 > coinbase_data.len() {
                return Err(DatumError::ProtocolError("Coinbase data too short for output value".to_string()));
            }
            let value = u64::from_le_bytes([
                coinbase_data[data_offset], coinbase_data[data_offset+1],
                coinbase_data[data_offset+2], coinbase_data[data_offset+3],
                coinbase_data[data_offset+4], coinbase_data[data_offset+5],
                coinbase_data[data_offset+6], coinbase_data[data_offset+7],
            ]);
            data_offset += 8;

            // Read script length
            if data_offset >= coinbase_data.len() {
                return Err(DatumError::ProtocolError("Coinbase data too short for script length".to_string()));
            }
            let script_len = coinbase_data[data_offset] as usize;
            data_offset += 1;

            // Read script
            if data_offset + script_len > coinbase_data.len() {
                return Err(DatumError::ProtocolError("Script length exceeds data".to_string()));
            }
            let script = coinbase_data[data_offset..data_offset + script_len].to_vec();
            data_offset += script_len;

            outputs.push(CoinbaseOutput {
                script,
                value,
            });
        }

        Ok(CoinbasePayout {
            outputs,
            primary_tag,
            unique_id,
        })
    }

    /// Get current coinbase payout requirements
    pub fn get_coinbase_payout(&self) -> Option<CoinbasePayout> {
        self.current_coinbase.clone()
    }

    /// Set block template (with coinbase coordination)
    pub async fn set_template(&mut self, template: Block) -> Result<(), DatumError> {
        self.current_template = Some(template.clone());
        
        // If connected to pool, fetch coinbase requirements
        if self.protocol_client.is_some() {
            // Calculate coinbase value from template (convert i64 to u64)
            let coinbase_value = template.transactions[0].outputs[0].value as u64;
            self.fetch_coinbase_payout(coinbase_value).await?;
        }
        
        debug!("Template set in pool");
        Ok(())
    }

    /// Submit proof of work to pool
    pub async fn submit_pow(&self, pow_data: Vec<u8>) -> Result<bool, DatumError> {
        let client = self.protocol_client.as_ref()
            .ok_or_else(|| DatumError::PoolConnectionError("Not connected to pool".to_string()))?;
        
        // Client now uses interior mutability, so we can call it with &self
        client.submit_pow(pow_data).await
    }
}

