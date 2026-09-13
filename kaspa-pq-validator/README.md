# `kaspa-pq-validator`

The DNS-finality validator sidecar for MISAKA. It connects to a co-located `kaspad` over wRPC Borsh, tracks a stake Bond and signs canonical-ready epoch attestations with ML-DSA-87.

This is separate from a PALW producer/panel Bond. See [the current validator runbook](../docs/validator-runbook.md).

## Recommended entry point

```bash
misaka --network testnet-11 validator setup
misaka --network testnet-11 validator status
```

## Build

```bash
cargo build --release -p kaspa-pq-validator -p misaka-cli
```

## Direct Testnet-11 flow

```bash
kaspa-pq-validator keygen --out validator.seed --network testnet

kaspa-pq-validator bond \
  --node-rpc 127.0.0.1:27210 \
  --validator-key validator.seed \
  --amount 1000000000 \
  --network testnet-11

kaspa-pq-validator run \
  --node-rpc 127.0.0.1:27210 \
  --validator-key validator.seed \
  --stake-bond <txid>:<index> \
  --signed-epoch-db validator.state \
  --network testnet-11 \
  --attest-poll-secs 3
```

Testnet-11's DNS stake minimum is 10 MSK (1,000,000,000 sompi). The node must be synced, run `--utxoindex` and enable Borsh RPC with `--rpclisten-borsh=default`.

## Safety

- one validator key on one active host
- back up both the seed and `signed-epoch-db`
- use a separate anti-equivocation database per network
- keep wRPC on loopback
- stopping the process does not unbond
- inspect current `unbond --help` before starting the waiting/evidence period

The sidecar's default poll interval is 3 seconds. Testnet-11 PALW blocks target 120 seconds and its DNS attestation epoch is 2 blue-score; old 10-BPS/testnet-21 timing guidance does not apply.
