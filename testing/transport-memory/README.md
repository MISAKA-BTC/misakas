Run the real generated P2P/RPC types, production shared bank/adapter, and pinned
tonic 0.14.5 without a consensus/Node build:

```sh
CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=/tmp/misaka-transport-target cargo test --locked --manifest-path testing/transport-memory/Cargo.toml
CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=/tmp/misaka-transport-target cargo test --locked --manifest-path testing/transport-memory/Cargo.toml legacy_evm_256mib -- --ignored --test-threads=1 --nocapture
```

The opt-in case temporarily holds about 1 GiB of actual messages and codec data.
It checks 256 MiB EVM and near-320 MiB P2P frames, plain/gzip round trips, bounded
reservation peaks, and final lease return. It does not measure process RSS or
cover consensus, DB, native model execution, RPC response construction, inbound
decoded-message lifetimes, or critical notification replay.

The generated `WirePreflight` inspectors are tested independently against actual
prost-decoded capacities (nested empty values, recursive Box, repeated primitive
packing, strings, duplicate fragments, unknown fields, malformed input and depth).
They are not connected to the runtime decoder yet. Their peak describes logical
decoded capacities through tonic's contiguous input buffer, excluding receive/
decompression buffers, stack and allocator metadata.

Repeated occurrences of a singular message or multiple message-valued variants
of one oneof are explicitly **unsupported** by the inspector. Prost accepts and
merges these encodings, but independent fragment estimates can undercount merged
Vec growth. The inspector returns an error instead of a heap bound; this is not
connected to runtime rejection. Supporting all protobuf-compatible merges needs
merge-aware field state before runtime integration. Packed scalar batches include
the intermediate geometric reallocations in their peak estimate.

Run the large field check:

```sh
CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=/tmp/misaka-transport-target cargo test --locked --manifest-path testing/transport-memory/Cargo.toml largest_legitimate_bytes_field -- --ignored --test-threads=1 --nocapture
```
