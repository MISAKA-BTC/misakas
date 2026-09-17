# `kaspad` — MISAKA node

`kaspad` is the MISAKA full-node daemon. The binary name and many crate names are inherited from rusty-kaspa, but MISAKA has its own PQ-only transaction rules, networks and genesis.

## Current public network

```bash
cargo build --release -p kaspad
./target/release/kaspad \
  --testnet --netsuffix=11 \
  --utxoindex \
  --rpclisten-borsh=default \
  --addpeer=169.58.39.220:26311
```

Verify consensus fingerprint:

```text
ab4e7b9c7e20d14cbadc0874312b8c6dff89ca66ed7e9be3af2f5523dc58b5f2
```

A normal full node needs no model artifact or Bond. PALW production is configured with `misaka --network testnet-11 mining setup`; an external hash miner cannot produce Testnet-11 PALW blocks.

## Default Testnet endpoints

- P2P: `26311` for suffix 11
- gRPC: `127.0.0.1:26210`
- wRPC Borsh: `127.0.0.1:27210` when enabled
- wRPC JSON: `127.0.0.1:28210` when enabled

## Documentation

- [Root README](../README.md)
- [Node operator guide](../docs/testnet11-node-operator.md)
- [PALW producer guide](../docs/testnet11-join-mining.md)
- [Documentation map](../docs/README.md)
- [Upstream rusty-kaspa](https://github.com/kaspanet/rusty-kaspa)
