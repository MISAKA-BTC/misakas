# `misaka` — unified operator CLI

`misaka` is the current operator entry point for Testnet-11 mining, verification, model, validator, wallet and observability tasks.

## Build

```bash
cargo build --release -p misaka-cli
./target/release/misaka --help
```

Always select the current live network explicitly:

```bash
misaka --network testnet-11 <command>
```

The code-level default remains testnet-10 for compatibility, but that public network is stopped.

## Start mining

```bash
misaka --network testnet-11 mining setup
misaka --network testnet-11 mining start --print-command
misaka --network testnet-11 mining start
misaka --network testnet-11 mining status --watch 5
```

The setup wizard is resumable and checks node identity, model, key, funds, Bond, artifact, panel capability and fee output before writing `~/.misaka/mining.toml`.

## Bond inspection

```bash
misaka --network testnet-11 bond status --bond <txid>:<index>
```

Exact-outpoint status is read-only and keyless. `--class-id` is optional; when omitted, BASE-0/Floor is used for the lookup and sizing output.

## Main command groups

- `mining` — setup/start/status/stop/run
- `doctor` — node and producer readiness
- `work`, `logs`, `rewards` — lifecycle and payout
- `verifier` — unpaid PALW panel seat
- `model`, `position` — class/catalog/market operations
- `dashboard` — read-only UI on `127.0.0.1:8791`
- `validator` — DNS-finality setup/status and sidecar forwarding
- `node`, `wallet`, `bond`, `palw`, `evm`, `key`

Use each subcommand's `--help`; it is generated from the same argument definitions as the binary.

## Endpoints

| flag | transport | Testnet default |
|---|---|---:|
| `--rpc` | node wRPC Borsh | 127.0.0.1:27210 |
| `--node-grpc` | node gRPC | 127.0.0.1:26210 |
| `--evm-rpc` | EVM JSON-RPC | http://127.0.0.1:8545 |

## Output and exit behavior

`--output human|json` selects human or script output. `--quiet` suppresses non-essential human text. Non-zero exits distinguish argument/configuration, network identity, connection, readiness, transaction rejection and timeout failures; scripts should inspect JSON `exitCode` or the process status rather than matching prose.
