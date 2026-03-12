//! Unit tests for DATUM protocol implementation

use blvm_datum::datum_protocol::DatumProtocolClient;
use blvm_datum::error::DatumError;
use blvm_datum::handlers::*;
use blvm_datum::messages::DatumCommand;

// Note: These tests require internal methods to be public or use integration testing approach
// For now, we test the public API and message parsing

#[test]
fn test_parse_client_config() {
    let mut data = vec![0u8; 13];
    data[0] = 10; // max_jobs
    data[1..5].copy_from_slice(&30u32.to_le_bytes()); // job_timeout
    data[5..13].copy_from_slice(&1000u64.to_le_bytes()); // share_difficulty

    let config = parse_client_config(&data).unwrap();
    assert_eq!(config.max_jobs, 10);
    assert_eq!(config.job_timeout, 30);
    assert_eq!(config.share_difficulty, 1000);
}

#[test]
fn test_parse_job_validation() {
    // Valid job
    let data = vec![1u8, 0u8]; // job_id=1, status=0 (valid)
    let (job_id, result) = parse_job_validation(&data).unwrap();
    assert_eq!(job_id, 1);
    matches!(result, JobValidationResult::Valid);

    // Invalid job
    let data = vec![2u8, 1u8, b'T', b'e', b's', b't']; // job_id=2, status=1 (invalid), reason="Test"
    let (job_id, result) = parse_job_validation(&data).unwrap();
    assert_eq!(job_id, 2);
    matches!(result, JobValidationResult::Invalid(_));

    // Needs update
    let data = vec![3u8, 2u8]; // job_id=3, status=2 (needs update)
    let (job_id, result) = parse_job_validation(&data).unwrap();
    assert_eq!(job_id, 3);
    matches!(result, JobValidationResult::NeedsUpdate);
}

#[test]
fn test_parse_share_response() {
    let mut data = vec![0u8; 9];
    data[0] = 1; // accepted
    data[1..9].copy_from_slice(&5000u64.to_le_bytes()); // difficulty

    let response = parse_share_response(&data).unwrap();
    assert_eq!(response.accepted, true);
    assert_eq!(response.difficulty, 5000);
}

#[test]
fn test_parse_block_notify() {
    let mut data = vec![0u8; 72];
    data[0..4].copy_from_slice(&100u32.to_le_bytes()); // height
    data[4..36].fill(0xAA); // hash
    data[36..68].fill(0xBB); // prev_hash

    let notify = parse_block_notify(&data).unwrap();
    assert_eq!(notify.height, 100);
    assert_eq!(notify.hash[0], 0xAA);
    assert_eq!(notify.prev_hash[0], 0xBB);
}

#[test]
fn test_message_handler() {
    let handler = DefaultMessageHandler;

    let config = ClientConfig {
        max_jobs: 10,
        job_timeout: 30,
        share_difficulty: 1000,
        params: vec![],
    };

    assert!(handler.handle_client_config(config).is_ok());

    let response = ShareResponse {
        accepted: true,
        difficulty: 5000,
        message: "Accepted".to_string(),
    };

    assert!(handler.handle_share_response(response).is_ok());
}
