# blvm-datum

DATUM Gateway mining protocol module for blvm-node.

## Overview

This module implements the DATUM Gateway protocol for pool communication, enabling decentralized mining with Ocean pool support. It provides:

- **DATUM Protocol Client**: Encrypted communication with DATUM pools (Ocean)
- **Decentralized Templates**: Block templates generated locally via NodeAPI
- **Coinbase Coordination**: Coordinates coinbase payouts with DATUM pool

**Note**: This module handles pool communication only. Miners connect via the `blvm-stratum-v2` module.

## Architecture

```
┌─────────────────┐
│   blvm-node     │
│  (Core Node)    │
└────────┬────────┘
         │ NodeAPI
         │ (get_block_template, submit_block)
         │
    ┌────┴────┐
    │         │
    ▼         ▼
┌─────────┐ ┌──────────────┐
│ blvm-   │ │ blvm-datum   │
│ stratum │ │ (Module)     │
│ v2      │ │              │
│         │ │ ┌──────────┐ │
│ ┌─────┐ │ │ │ DATUM   │ │◄─── DATUM Pool (Ocean)
│ │ SV2 │ │ │ │ Client  │ │     (Encrypted Protocol)
│ │Server│ │ │ └──────────┘ │
│ └─────┘ │ └──────────────┘
│         │
│    │    │
│    ▼    │
│ Mining  │
│Hardware │
└─────────┘
```

**Key Points**:
- `blvm-datum`: Handles DATUM pool communication only
- `blvm-stratum-v2`: Handles miner connections
- Both modules share block templates via NodeAPI
- Both modules can submit blocks independently

## Features

- **Decentralized Mining**: Miners construct their own block templates
- **Pool Integration**: Coordinates with DATUM pools for reward distribution
- **Template Sharing**: Uses shared NodeAPI for efficient template generation
- **Module Cooperation**: Works with `blvm-stratum-v2` for complete mining solution

## Configuration

```toml
[modules.blvm-stratum-v2]
enabled = true
listen_addr = "0.0.0.0:3333"
mode = "solo"  # or "pool"

[modules.blvm-datum]
enabled = true
pool_url = "https://ocean.xyz/datum"
pool_username = "user"
pool_password = "pass"

[modules.blvm-datum.mining]
coinbase_tag_primary = "DATUM Gateway"
coinbase_tag_secondary = "BLVM User"
pool_address = "bc1q..."  # Bitcoin address for pool payouts
```

**Note**: Both modules should be enabled for full DATUM Gateway functionality:
- `blvm-stratum-v2`: Handles miner connections
- `blvm-datum`: Handles DATUM pool communication

## Dependencies

- `blvm-node`: Module system integration
- `libsodium`: Encryption for DATUM protocol
- `tokio`: Async runtime

## Status

🚧 **In Development** - Initial implementation phase

## References

- [DATUM Gateway](https://github.com/OCEAN-xyz/datum_gateway)
- [DATUM Documentation](https://ocean.xyz/docs/datum)
- [Integration Analysis](../docs/DATUM_STRATUM_V2_INTEGRATION_ANALYSIS.md)

