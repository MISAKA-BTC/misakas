# Run a node

> 旧ルート README（2026-10-12 まで）から移した手順です。例に testnet-11 や古い値が含まれる場合があります。現行ネットワーク（testnet-12）の値は [README](../../README.md) と [docs/README](../README.md) を正とし、`--help` と chain RPC で確認してください。

## Running a testnet node

Start a misakas testnet node on the live network (`testnet-11`; PQ rules, the DNS-finality overlay
and PALW ConsensusV2 are all active from genesis):

```bash
cargo run --release --bin kaspad -- --testnet --netsuffix=11 --utxoindex --rpclisten-borsh=default
```

`--netsuffix=11` is required: bare `--testnet` still means `testnet-10`, which is stopped.

`=default` resolves to the network's standard loopback port, so you never have to memorize the
numbers. Add `--rpclisten-json=default` too if a JSON WebSocket client (e.g. a browser app or an
explorer backend) needs to connect locally.

- To **join the public testnet**, the node discovers peers via the misakas DNS seeders
  (`seeder1.misakascan.com` … `seeder4.misakascan.com`) automatically. **testnet-11's P2P port is
  `26311`** (testnet-10 `26211`, mainnet `26111`, devnet `26611`) — make sure it isn't blocked
  outbound. The four seeder names are shared with the other networks and that is safe: a record
  hands out an IP, each network dials it on its OWN default P2P port, and `consensus_params_id`
  refuses the handshake if one ever answered. A seeder that has nothing healthy to advertise
  returns an empty answer rather than a wrong peer.

  If discovery is slow, resolve a seeder for that invocation and pass the resulting IP to
  `--addpeer` (or `--connect` to use only that peer). The flags currently accept IP addresses,
  not hostnames; do not hard-code a DNS answer's IP in a permanent config. Block explorer:
  **[misakascan.com](https://misakascan.com)**.
- `--utxoindex` is required for wallet/validator funding lookups.
- **gRPC is always on by default** (loopback, `127.0.0.1:26210` on testnet) even with no RPC flag,
  so the **miner needs no extra flag** — it connects over gRPC. **wRPC (Borsh / JSON) is off by
  default** and must be enabled with `--rpclisten-borsh` / `--rpclisten-json`; it is required by the
  CLI wallet and the `kaspa-pq-validator` sidecar (which speak wRPC, **not** gRPC).
- **Connecting a wallet / RPC client — pick the right port.** The RPC ports are per network TYPE,
  so **every testnet suffix shares them**; only P2P carries the suffix. Default **testnet** ports:
  **gRPC** `26210` (protobuf over TCP, default-on at loopback), **wRPC Borsh** `27210`
  (the CLI wallet & validator transport, WebSocket — enable with `--rpclisten-borsh=default`),
  **wRPC JSON** `28210` (WebSocket — enable with `--rpclisten-json=default`). Mainnet uses
  `26110/27110/28110`; devnet `26610/27610/28610`.
  The `kaspa-pq` CLI wallet connects over **wRPC Borsh** — point it at `27210`, **not** the gRPC
  port `26210` (a wallet pointed at gRPC fails with `WebSocket protocol error: httparse err`
  or `WebSocket is not connected`, because gRPC is not a WebSocket). In the wallet REPL:
  `server 127.0.0.1:27210` → `connect`. (P2P is a separate, non-RPC port: `26311` on testnet-11.)
- **Headless balance (no interactive wallet).** For scripting / monitoring, query a balance in one
  shot over wRPC:
  `kaspa-pq-validator balance --node-rpc 127.0.0.1:27210 --address misakatest:q… [--address …] [--network testnet-11]`.
  It prints `address <sompi> <BILI> BILI` per line (plus `TOTAL` for several) to stdout — connection /
  sync notes go to stderr, so `… balance --address misakatest:q… | awk '{print $2}'` yields just the
  sompi. The node must run `--utxoindex`.
- Add `--enable-unsynced-mining` **only** when bootstrapping a brand-new isolated network with no peers (mining before you have synced to the public testnet would fork from genesis).

Testnet-11 cannot be mined with `kaspa-pq-miner` or `misaminer`: they do not create the required
PALW attempt envelope. Block production runs inside `kaspad --palw-produce`; use
`misaka mining setup/start` rather than an external hash miner.
