# `kaspad` — MISAKA node

`kaspad` is the MISAKA full-node daemon. The binary name and many crate names are inherited from rusty-kaspa, but MISAKA has its own PQ-only transaction rules, networks and genesis.

## Current public network

```bash
cargo build --release -p kaspad
./target/release/kaspad \
  --testnet --netsuffix=12 \
  --utxoindex \
  --rpclisten-borsh=default
```

Testnet-12 uses DNS seeders by default. If an explicit bootstrap is required, resolve a seeder
for that invocation and pass its IP to `--addpeer`; do not persist a resolved seeder IP.

Verify the consensus params fingerprint the node prints at startup against the one
[`release.json`](../release.json) declares (and the [root README](../README.md#release-status) repeats):

```text
254509533bb693ced0fed823a4c25e166ba2542d576e4021b0e4b4d6fe4079e1
```

A normal full node needs no model artifact or Bond. PALW production is configured with `misaka --network testnet-12 mining setup`; an external hash miner cannot produce Testnet-12 PALW blocks.

## Default Testnet endpoints

- P2P: `26311` for suffix 12
- gRPC: `127.0.0.1:26210`
- wRPC Borsh: `127.0.0.1:27210` when enabled
- wRPC JSON: `127.0.0.1:28210` when enabled

## Documentation

- [Root README](../README.md)
- [Node operator guide](../docs/node-operator.md)
- [PALW producer guide](../docs/testnet12-join-mining.md)
- [Documentation map](../docs/README.md)
- [Upstream rusty-kaspa](https://github.com/kaspanet/rusty-kaspa)
