//! DATUM protocol message types
//!
//! Defines message types for DATUM protocol communication

use serde::{Deserialize, Serialize};

/// DATUM protocol message header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatumMessageHeader {
    /// Command length (22 bits, max 4MB)
    pub cmd_len: u32,
    /// Reserved (2 bits)
    pub reserved: u8,
    /// Is signed
    pub is_signed: bool,
    /// Is encrypted with public key
    pub is_encrypted_pubkey: bool,
    /// Is encrypted channel
    pub is_encrypted_channel: bool,
    /// Protocol command (5 bits, 32 commands)
    pub proto_cmd: u8,
}

/// DATUM protocol commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatumCommand {
    /// Handshake
    Handshake = 0,
    /// Fetch coinbaser
    FetchCoinbaser = 0x11,
    /// Submit POW
    SubmitPow = 2,
    /// Job update
    JobUpdate = 3,
    /// Client configuration
    ClientConfig = 0x99,
    /// Job validation commands
    JobValidation = 0x50,
    /// Share response
    ShareResponse = 0x8F,
    /// Block notify
    BlockNotify = 0xF9,
}

impl DatumCommand {
    /// Convert u8 to DatumCommand
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Handshake),
            0x11 => Some(Self::FetchCoinbaser),
            2 => Some(Self::SubmitPow),
            3 => Some(Self::JobUpdate),
            0x99 => Some(Self::ClientConfig),
            0x50 => Some(Self::JobValidation),
            0x8F => Some(Self::ShareResponse),
            0xF9 => Some(Self::BlockNotify),
            _ => None,
        }
    }
}

