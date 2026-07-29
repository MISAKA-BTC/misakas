# `misaka` — unified MISAKA operator CLI

One user-facing front-end over the functionality that is otherwise scattered
across `kaspa-pq-cli`, the interactive wallet REPL, `kaspa-pq-validator`, and
the `evm_tx_gen` dev example. Existing binaries stay for compatibility; this
binary grows in **tiers**.

## Build

```bash
cargo build --release -p misaka-cli --bin misaka
```

The default `misaka` build is secp-free. EVM send / PREA signing commands are
behind this package's `evm-send` feature:

```bash
cargo build --release -p misaka-cli --bin misaka --features evm-send
```

## Tier A — observability (this slice)

Read-only commands over the **existing** node wRPC + EVM JSON-RPC. No new RPCs,
no private keys, no transaction construction.

```
misaka node doctor                            # ports, sync, versions, RPC surface
misaka node pruning-snapshot verify --file snapshot.borsh # offline canonical/digest check
misaka evm balance      --address 0x…         # native MSK balance (eth_getBalance)
misaka evm nonce        --address 0x…         # next nonce (eth_getTransactionCount)
misaka evm estimate-gas --from 0x… --to 0x… [--value <sompi>] [--data 0x…]
misaka evm tx status    --hash 0x…            # one-shot misaka_getEvmTxStatus
misaka evm tx wait      --hash 0x… [--timeout 1800] [--poll 2]
misaka validator status --stake-bond <txid:index> # read-only node/bond/DNS health
misaka wallet utxo consolidate --key-file validator.seed --yes --max-txs-per-run 100 --sleep-ms 200
```

### Global flags

```
--output human|json     # default human; JSON is stable for scripts/monitors
--network <id>          # default testnet-200; sets default RPC ports + match check
--rpc <host:port>       # node wRPC borsh; default derives from --network (testnet=27210)
--evm-rpc <http://…>    # default http://127.0.0.1:8545
--timeout <secs>        # default 30
--quiet
```

### Exit codes

```
0 success   3 network mismatch   4 connection failure   5 node not synced/unhealthy
6 tx rejected   7 timeout while still pending   (2 = clap argument error)
```

### Examples

```bash
misaka node doctor --network testnet-200 --rpc 127.0.0.1:27210
misaka --network testnet-200 --rpc 127.0.0.1:27210 validator status --stake-bond <txid:index>
misaka --output json evm tx status --hash 0x9b87e742…ed31
misaka evm tx wait --hash 0x9b87e742…ed31 --timeout 600 --poll 2
```

## Next tiers (not in this slice)

- **B** — keyed transactions: `evm wallet`/`evm send` (BIP-44 m/44'/60', EIP-1559,
  EIP-55), `wallet send`/`wallet utxo consolidate` (UTXO paging + ML-DSA signing).
- **C** — needs new node RPCs: `evm tx diagnose` / mempool stats
  (`misaka_getEvmMempoolInfo`, `notSelectedReason`), `getNodeCapabilities`,
  `getSyncProgressDetailed`.
