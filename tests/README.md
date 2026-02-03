# DATUM Module Testing

This directory contains tests for the DATUM module implementation.

## Test Structure

- **protocol_tests.rs**: Unit tests for protocol implementation (header serialization, XOR, nonces, message parsing)
- **integration_tests.rs**: Integration tests for coinbase parsing and command handling

## Running Tests

### Unit Tests
```bash
cd blvm-datum
cargo test --lib
```

### Integration Tests
```bash
cargo test --test integration_tests
```

### All Tests
```bash
cargo test
```

## End-to-End Testing

End-to-end testing with actual DATUM pools requires:

1. **Testnet DATUM Pool**: Access to a DATUM pool testnet (e.g., Ocean testnet)
2. **Pool Configuration**: 
   - Pool URL (host:port)
   - Pool username/password
   - Pool's X25519 public key (for crypto_box_seal)

### Configuration Example

Create a test configuration file or set environment variables:

```toml
[pool]
url = "testnet.ocean.xyz:8443"
username = "test_user"
password = "test_password"
public_key = "hex_encoded_32_byte_x25519_public_key"
```

### Manual End-to-End Test

1. **Start BLVM Node**:
   ```bash
   ./target/release/blvm-node --datadir=/tmp/blvm-test
   ```

2. **Load DATUM Module**:
   ```bash
   ./target/release/blvm-node --module=blvm-datum \
     --module-config=pool_url=testnet.ocean.xyz:8443 \
     --module-config=pool_username=test_user \
     --module-config=pool_password=test_password \
     --module-config=pool_public_key=<hex_key>
   ```

3. **Verify Connection**:
   - Check logs for "Successfully connected to DATUM pool"
   - Verify handshake completed
   - Check for coinbase payout requests

4. **Test Block Template Generation**:
   - Trigger block template update
   - Verify coinbase requirements are fetched
   - Check coinbase payout parsing

5. **Test Share Submission**:
   - Submit a test share
   - Verify response handling

### Automated End-to-End Test Script

```bash
#!/bin/bash
# test_e2e.sh

set -e

POOL_URL="${DATUM_POOL_URL:-testnet.ocean.xyz:8443}"
POOL_USER="${DATUM_POOL_USER:-test_user}"
POOL_PASS="${DATUM_POOL_PASS:-test_password}"
POOL_KEY="${DATUM_POOL_KEY}"

if [ -z "$POOL_KEY" ]; then
    echo "Error: DATUM_POOL_KEY environment variable not set"
    exit 1
fi

# Start node with DATUM module
./target/release/blvm-node \
    --datadir=/tmp/blvm-test \
    --module=blvm-datum \
    --module-config="pool_url=$POOL_URL" \
    --module-config="pool_username=$POOL_USER" \
    --module-config="pool_password=$POOL_PASS" \
    --module-config="pool_public_key=$POOL_KEY" \
    &

NODE_PID=$!

# Wait for connection
sleep 5

# Check if connected
if grep -q "Successfully connected to DATUM pool" /tmp/blvm-test/logs/node.log; then
    echo "✓ DATUM pool connection successful"
else
    echo "✗ DATUM pool connection failed"
    kill $NODE_PID
    exit 1
fi

# Cleanup
kill $NODE_PID
echo "✓ End-to-end test completed"
```

## Mock Pool Server

For development and CI/CD, a mock DATUM pool server can be created:

### Requirements
- Implement DATUM protocol handshake
- Respond to coinbase fetch requests
- Handle share submissions
- Send configuration and notification messages

### Reference Implementation
See Ocean's open-source DATUM Gateway implementation at:
- `~/src/datum/src/datum_protocol.c`
- `~/src/datum/src/datum_gateway.c`

## Test Coverage Goals

- [x] Protocol header serialization/deserialization
- [x] Header XOR obfuscation with feedback
- [x] Session nonce initialization
- [x] Message parsing (client config, job validation, share response, block notify)
- [x] Coinbase payout parsing
- [ ] Full handshake flow (requires mock pool)
- [ ] Encrypted message exchange (requires mock pool)
- [ ] Share submission flow (requires mock pool)
- [ ] Error handling and reconnection (requires mock pool)

## Continuous Integration

Tests should be run in CI/CD pipeline:
- Unit tests: Fast, no external dependencies
- Integration tests: Medium speed, may require mock server
- End-to-end tests: Slow, require testnet access (optional)

