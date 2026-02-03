//! Tests for DATUM protocol internals

use blvm_datum::datum_protocol::DatumProtocolClient;
use blvm_datum::messages::{DatumCommand, DatumMessageHeader};

#[test]
fn test_header_serialization_roundtrip() {
    let header = DatumMessageHeader {
        cmd_len: 100,
        reserved: 0,
        is_signed: true,
        is_encrypted_pubkey: false,
        is_encrypted_channel: true,
        proto_cmd: DatumCommand::Handshake as u8,
    };
    
    let serialized = DatumProtocolClient::serialize_header_static(&header).unwrap();
    assert_eq!(serialized.len(), 4);
    
    let deserialized = DatumProtocolClient::deserialize_header_static(&serialized).unwrap();
    assert_eq!(deserialized.cmd_len, 100);
    assert_eq!(deserialized.reserved, 0);
    assert_eq!(deserialized.is_signed, true);
    assert_eq!(deserialized.is_encrypted_pubkey, false);
    assert_eq!(deserialized.is_encrypted_channel, true);
    assert_eq!(deserialized.proto_cmd, DatumCommand::Handshake as u8);
}

#[test]
fn test_header_serialization_all_commands() {
    // Note: Header only supports 5-bit commands (0-31), but DATUM protocol uses larger values
    // The header serialization masks the command to 5 bits, so we test the valid range
    let commands = [
        (DatumCommand::Handshake, 0u8),
        (DatumCommand::SubmitPow, 2u8),
        (DatumCommand::JobUpdate, 3u8),
    ];
    
    for (cmd, expected_value) in commands.iter() {
        let header = DatumMessageHeader {
            cmd_len: 42,
            reserved: 0,
            is_signed: true,
            is_encrypted_pubkey: false,
            is_encrypted_channel: true,
            proto_cmd: *cmd as u8,
        };
        
        let serialized = DatumProtocolClient::serialize_header_static(&header).unwrap();
        let deserialized = DatumProtocolClient::deserialize_header_static(&serialized).unwrap();
        assert_eq!(deserialized.proto_cmd, *expected_value, "Command {:?} should serialize to {}", cmd, expected_value);
    }
    
    // Test that larger command values are masked to 5 bits
    let large_commands = [
        DatumCommand::FetchCoinbaser,  // 0x11 -> masked to 0x11 & 0x1F = 0x11 (17, valid)
        DatumCommand::ClientConfig,    // 0x99 -> masked to 0x99 & 0x1F = 0x19 (25)
        DatumCommand::JobValidation,   // 0x50 -> masked to 0x50 & 0x1F = 0x10 (16)
        DatumCommand::ShareResponse,   // 0x8F -> masked to 0x8F & 0x1F = 0x0F (15)
        DatumCommand::BlockNotify,     // 0xF9 -> masked to 0xF9 & 0x1F = 0x19 (25)
    ];
    
    for cmd in large_commands.iter() {
        let header = DatumMessageHeader {
            cmd_len: 42,
            reserved: 0,
            is_signed: true,
            is_encrypted_pubkey: false,
            is_encrypted_channel: true,
            proto_cmd: *cmd as u8,
        };
        
        let serialized = DatumProtocolClient::serialize_header_static(&header).unwrap();
        let deserialized = DatumProtocolClient::deserialize_header_static(&serialized).unwrap();
        // After serialization, command is masked to 5 bits
        let expected_masked = (*cmd as u8) & 0x1F;
        assert_eq!(deserialized.proto_cmd, expected_masked, "Command {:?} should be masked to {}", cmd, expected_masked);
    }
}

#[test]
fn test_header_serialization_flags() {
    let test_cases = vec![
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
        (true, true, false),
        (true, false, true),
        (false, true, true),
        (true, true, true),
    ];
    
    for (signed, pubkey, channel) in test_cases {
        let header = DatumMessageHeader {
            cmd_len: 100,
            reserved: 0,
            is_signed: signed,
            is_encrypted_pubkey: pubkey,
            is_encrypted_channel: channel,
            proto_cmd: DatumCommand::Handshake as u8,
        };
        
        let serialized = DatumProtocolClient::serialize_header_static(&header).unwrap();
        let deserialized = DatumProtocolClient::deserialize_header_static(&serialized).unwrap();
        assert_eq!(deserialized.is_signed, signed);
        assert_eq!(deserialized.is_encrypted_pubkey, pubkey);
        assert_eq!(deserialized.is_encrypted_channel, channel);
    }
}

#[test]
fn test_header_serialization_max_length() {
    let header = DatumMessageHeader {
        cmd_len: 0x3FFFFF, // Max 22-bit value
        reserved: 0,
        is_signed: true,
        is_encrypted_pubkey: false,
        is_encrypted_channel: true,
        proto_cmd: DatumCommand::Handshake as u8,
    };
    
    let serialized = DatumProtocolClient::serialize_header_static(&header).unwrap();
    let deserialized = DatumProtocolClient::deserialize_header_static(&serialized).unwrap();
    assert_eq!(deserialized.cmd_len, 0x3FFFFF);
}

#[test]
fn test_session_nonce_initialization() {
    let header_key = 0x12345678u32;
    let nonce1 = DatumProtocolClient::initialize_session_nonce_static(header_key);
    let nonce2 = DatumProtocolClient::initialize_session_nonce_static(header_key);
    
    // Nonces should be deterministic (same input = same output)
    assert_eq!(nonce1, nonce2);
    
    // Different header key should produce different nonce
    let nonce3 = DatumProtocolClient::initialize_session_nonce_static(0x87654321u32);
    assert_ne!(nonce1, nonce3);
    
    // Nonce should be non-zero for valid keys
    assert_ne!(nonce1, 0);
    assert_ne!(nonce3, 0);
}

#[test]
fn test_key_derivation() {
    let shared_secret = b"test_shared_secret_32_bytes!!";
    let send_key = DatumProtocolClient::derive_key_from_shared_secret_static(shared_secret, b"send");
    let recv_key = DatumProtocolClient::derive_key_from_shared_secret_static(shared_secret, b"recv");
    
    // Send and recv keys should be different
    assert_ne!(send_key, recv_key);
    
    // Keys should be 32 bytes
    assert_eq!(send_key.len(), 32);
    assert_eq!(recv_key.len(), 32);
    
    // Same input should produce same output
    let send_key2 = DatumProtocolClient::derive_key_from_shared_secret_static(shared_secret, b"send");
    assert_eq!(send_key, send_key2);
    
    // Different labels should produce different keys
    let other_key = DatumProtocolClient::derive_key_from_shared_secret_static(shared_secret, b"other");
    assert_ne!(send_key, other_key);
    assert_ne!(recv_key, other_key);
}

#[test]
fn test_header_deserialize_errors() {
    // Too short
    let data = vec![0u8; 3];
    assert!(DatumProtocolClient::deserialize_header_static(&data).is_err());
    
    // Empty
    let data = vec![];
    assert!(DatumProtocolClient::deserialize_header_static(&data).is_err());
}

#[test]
fn test_parse_client_config_errors() {
    use blvm_datum::handlers::parse_client_config;
    
    // Too short
    let data = vec![0u8; 10];
    assert!(parse_client_config(&data).is_err());
    
    // Empty
    let data = vec![];
    assert!(parse_client_config(&data).is_err());
}

#[test]
fn test_parse_job_validation_errors() {
    use blvm_datum::handlers::parse_job_validation;
    
    // Empty
    let data = vec![];
    assert!(parse_job_validation(&data).is_err());
}

#[test]
fn test_parse_share_response_errors() {
    use blvm_datum::handlers::parse_share_response;
    
    // Too short
    let data = vec![0u8; 5];
    assert!(parse_share_response(&data).is_err());
    
    // Empty
    let data = vec![];
    assert!(parse_share_response(&data).is_err());
}

#[test]
fn test_parse_block_notify_errors() {
    use blvm_datum::handlers::parse_block_notify;
    
    // Too short
    let data = vec![0u8; 50];
    assert!(parse_block_notify(&data).is_err());
    
    // Empty
    let data = vec![];
    assert!(parse_block_notify(&data).is_err());
}

