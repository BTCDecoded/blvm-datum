# blvm-datum

DATUM Gateway mining protocol module for blvm-node.

## Overview

Pool communication for the [DATUM Gateway](https://github.com/OCEAN-xyz/datum_gateway) protocol (Ocean). Miners connect through **`blvm-stratum-v2`**; this module talks to the DATUM pool only.

- **DATUM protocol client** — encrypted pool channel when `pool_public_key` is configured
- **Local templates** — block templates via NodeAPI (`get_block_template`)
- **Coinbase coordination** — payout outputs from the pool at runtime (`fetch_coinbaser` / inter-module API)

## Configuration

Pin both modules in node `blvm.toml`:

```toml
[modules]
blvm-stratum-v2 = "0.1.*"
blvm-datum = "0.1.*"

[modules.blvm-stratum-v2]
listen_addr = "0.0.0.0:3333"

[modules.blvm-datum]
pool_url = "https://ocean.xyz/datum"
pool_username = "user"
pool_password = "pass"
# pool_public_key = "hex_encoded_32_byte_public_key"  # optional
# reconnect_interval = 30
# min_difficulty = 1
```

Module data-dir `config.toml` uses the same keys (no `[mining]` table, no `coinbase_tag_*` or `pool_address` — those come from the pool).

Operator docs: [Datum module](https://docs.thebitcoincommons.org/modules/datum.html) in the BLVM book.

## Dependencies

- `blvm-node` — module system + NodeAPI
- `libsodium` — DATUM encryption
- `tokio` — async runtime

## Status

In development — see crate tests and the book module page for current event/API surface.

## References

- [DATUM Gateway](https://github.com/OCEAN-xyz/datum_gateway)
- [Ocean DATUM documentation](https://ocean.xyz/docs/datum)
