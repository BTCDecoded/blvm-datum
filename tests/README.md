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

1. **Start BLVM node** (operator binary is **`blvm`**):
   ```bash
   ./target/release/blvm --data-dir /tmp/blvm-test --network testnet --rpc-addr 127.0.0.1:18332
   ```

2. **Load DATUM module** (manifest name `blvm-datum`), with the node already running:
   ```bash
   ./target/release/blvm --rpc-addr 127.0.0.1:18332 module load blvm-datum
   ```

   Configure pool settings via **`blvm config set`** / `blvm.toml` **`[modules].module_configs`** as documented for your deployment — there is no `blvm-node --module=...` flag.

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

# Start node (adjust paths and RPC addr to your layout)
./target/release/blvm \
    --data-dir /tmp/blvm-test \
    --network testnet \
    --rpc-addr 127.0.0.1:18332 \
    &

NODE_PID=$!

# After the RPC server is up, load the module from a second shell:
./target/release/blvm --rpc-addr 127.0.0.1:18332 module load blvm-datum

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

