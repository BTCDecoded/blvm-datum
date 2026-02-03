//! Integration tests for DATUM protocol
//!
//! These tests require a mock DATUM pool server or can be run against
//! a testnet DATUM pool if available.

use blvm_datum::pool::DatumPool;

/// Test coinbase payout parsing with various formats
#[test]
fn test_coinbase_payout_parsing() {
    // Test case 1: Simple payout with one output
    let mut data = vec![0u8; 60];
    let mut offset = 0;
    
    // value (8 bytes)
    data[offset..offset+8].copy_from_slice(&1000000u64.to_le_bytes());
    offset += 8;
    
    // length (4 bytes) - will set after building coinbase_data
    let length_pos = offset;
    offset += 4;
    
    // coinbase_data
    let coinbase_data_start = offset;
    data[offset] = 5; // primary_tag_len
    offset += 1;
    data[offset..offset+5].copy_from_slice(b"Ocean");
    offset += 5;
    
    data[offset] = 3; // unique_id_len
    offset += 1;
    data[offset..offset+3].copy_from_slice(b"123");
    offset += 3;
    
    data[offset] = 1; // output_count
    offset += 1;
    
    data[offset..offset+8].copy_from_slice(&500000u64.to_le_bytes()); // value
    offset += 8;
    data[offset] = 25; // script_len
    offset += 1;
    data[offset..offset+25].fill(0x76); // script (dummy)
    offset += 25;
    
    // Set length
    let coinbase_data_len = offset - coinbase_data_start;
    data[length_pos..length_pos+4].copy_from_slice(&(coinbase_data_len as u32).to_le_bytes());
    
    // Terminator
    data[offset] = 0xFE;
    offset += 1;
    
    let payout = DatumPool::parse_coinbase_response(&data[..offset]).unwrap();
    assert_eq!(payout.primary_tag, "Ocean");
    assert_eq!(payout.unique_id, "123");
    assert_eq!(payout.outputs.len(), 1);
    assert_eq!(payout.outputs[0].value, 500000);
    assert_eq!(payout.outputs[0].script.len(), 25);
}

#[test]
fn test_coinbase_payout_multiple_outputs() {
    let mut data = vec![0u8; 100];
    let mut offset = 0;
    
    // value (8 bytes)
    data[offset..offset+8].copy_from_slice(&2000000u64.to_le_bytes());
    offset += 8;
    
    // length placeholder
    let length_pos = offset;
    offset += 4;
    
    // coinbase_data
    let coinbase_data_start = offset;
    data[offset] = 8; // primary_tag_len
    offset += 1;
    data[offset..offset+8].copy_from_slice(b"TestPool");
    offset += 8;
    
    data[offset] = 4; // unique_id_len
    offset += 1;
    data[offset..offset+4].copy_from_slice(b"abcd");
    offset += 4;
    
    data[offset] = 2; // output_count
    offset += 1;
    
    // First output
    data[offset..offset+8].copy_from_slice(&1000000u64.to_le_bytes());
    offset += 8;
    data[offset] = 20; // script_len
    offset += 1;
    data[offset..offset+20].fill(0xAA);
    offset += 20;
    
    // Second output
    data[offset..offset+8].copy_from_slice(&500000u64.to_le_bytes());
    offset += 8;
    data[offset] = 20; // script_len
    offset += 1;
    data[offset..offset+20].fill(0xBB);
    offset += 20;
    
    // Set length
    let coinbase_data_len = offset - coinbase_data_start;
    data[length_pos..length_pos+4].copy_from_slice(&(coinbase_data_len as u32).to_le_bytes());
    
    // Terminator
    data[offset] = 0xFE;
    
    let payout = DatumPool::parse_coinbase_response(&data[..offset+1]).unwrap();
    assert_eq!(payout.primary_tag, "TestPool");
    assert_eq!(payout.unique_id, "abcd");
    assert_eq!(payout.outputs.len(), 2);
    assert_eq!(payout.outputs[0].value, 1000000);
    assert_eq!(payout.outputs[1].value, 500000);
    assert_eq!(payout.outputs[0].script[0], 0xAA);
    assert_eq!(payout.outputs[1].script[0], 0xBB);
}

/// Test error handling for malformed coinbase responses
#[test]
fn test_coinbase_payout_errors() {
    // Too short
    let data = vec![0u8; 10];
    assert!(DatumPool::parse_coinbase_response(&data).is_err());
    
    // Missing terminator
    let mut data = vec![0u8; 20];
    data[0..8].copy_from_slice(&1000u64.to_le_bytes());
    data[8..12].copy_from_slice(&5u32.to_le_bytes());
    // No terminator
    assert!(DatumPool::parse_coinbase_response(&data).is_err());
}

/// Test command enum conversion
#[test]
fn test_command_enum() {
    use blvm_datum::messages::DatumCommand;
    
    assert_eq!(DatumCommand::from_u8(0), Some(DatumCommand::Handshake));
    assert_eq!(DatumCommand::from_u8(0x11), Some(DatumCommand::FetchCoinbaser));
    assert_eq!(DatumCommand::from_u8(0x99), Some(DatumCommand::ClientConfig));
    assert_eq!(DatumCommand::from_u8(0x50), Some(DatumCommand::JobValidation));
    assert_eq!(DatumCommand::from_u8(0x8F), Some(DatumCommand::ShareResponse));
    assert_eq!(DatumCommand::from_u8(0xF9), Some(DatumCommand::BlockNotify));
    assert_eq!(DatumCommand::from_u8(0xFF), None);
}

