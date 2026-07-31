<h1>misakas — post-quantum (PQ-only) Kaspa</h1>

**misakas** is a post-quantum, **PQ-only** fork of [rusty-kaspa](https://github.com/kaspanet/rusty-kaspa). It replaces Kaspa's secp256k1/Schnorr transaction authorization with **ML-DSA-87** (FIPS 204, NIST category 5) and makes every non-PQ path — legacy secp256k1/Schnorr/ECDSA signatures, legacy addresses, and P2SH — **unrepresentable at the consensus, mempool, and wallet layers**. It is a new, independent network with its own genesis; it is **not** compatible with Kaspa or with any prior kaspa-pq chain state, UTXO set, or address.

The node binary is still named `kaspad` and the crates keep their upstream `kaspa-*` names (this is a fork, not a rename); the **network**, addresses (`misaka…` mainnet / `misakatest…` testnet / `misakadev…` devnet), and project branding are misakas.

> Status: **`testnet-20` is the public testnet.** It inherits the ADR-0048 staging-mainnet PALW
> shape — the audited-compute lane and algo-4 acceptance are active, unlisted peers are accepted —
> and adds the ADR-MA **Compute Set registry, open from genesis** on a v5 genesis block. The MISAKA
> DNS seed domains advertise testnet-20 peers on P2P port `26521`.
> `testnet-200` is **deprecated**: it is kept compilable for operators still holding its ledger, but
> it has no seeders and its replay is unsafe (see the migration note below). `testnet-10` is retired
> from public operation and remains only as a compatibility preset.
>
> PQ-only consensus and the DNS-finality reward overlay are **active from genesis on every defined
> network** (`pq_activation_daa_score = 0`, `dns_activation_daa_score = 0`). The `testnet`/`mainnet`
> parameter sets both use **two-dimensional** DNS-finality confirmation. testnet-20 keeps the
> staging-mainnet consensus shape (real PoW, non-inert anti-spam, full-scale depths) but lowers the
> validator ENTRY floors to testnet scale — a **10 MSK** bond and **one** active validator — because
> §17.3's validator quorum has to be mining-fundable within minutes of a fresh re-genesis. The
> `mainnet` parameter set is **defined but NOT launched or endorsed for production** — do not run
> `--mainnet` expecting a live or supported network. (The earlier experimental `devnet` has been
> retired.)

## Join the testnet

**→ [docs/testnet-participation.md](docs/testnet-participation.md) is the entry point**: which
network to join, the hardware that is actually supported, build, node, mining, validator, and an
explicit list of what is *not* possible yet.

**→ [docs/testnet-20-migration.md](docs/testnet-20-migration.md)** is the operator cutover note for
the current public net: why testnet-200 was replaced, the new defaults and ports, and the fact that
no prior testnet state can be carried across. (The earlier 10→200 note is kept as
[docs/testnet-200-migration.md](docs/testnet-200-migration.md) for history.)

**→ [docs/palw-llm-receipts.md](docs/palw-llm-receipts.md)** covers the other half — the PALW
audited-compute side: the pinned Qwen3.6-35B-A3B model, issuing a `ComputeReceipt`, verifying one in
a separate process, and what a receipt does and does not prove. The runtime itself lives in
**[LLM-Validation](https://github.com/MISAKA-BTC/LLM-Validation)**.

> **Everything here is a testnet. There is no mainnet.** MSK on these networks is valueless test
> currency, and Testnet Points (MTP) are a participation record — not a token, not a balance, not
> tradable, with no monetary value and no promise of conversion. Anyone offering to buy or sell
> MISAKA coins or points is running a scam. Earning points is documented in
> [§8 of the participation guide](docs/testnet-participation.md#8-testnet-points-mtp); it takes one
> key, one invitation, and an offline signature.

**`testnet-20`** — the public PALW testnet. Peers are found automatically via the public DNS seeders:

```bash
git clone https://github.com/MISAKA-BTC/misakas
cd misakas
cargo build --release -p kaspad -p misaka-cli --bin misaka -p kaspa-pq-miner
./target/release/misaka join -- --utxoindex --rpclisten-borsh=default   # --network defaults to testnet-20
./target/release/misaka node doctor                                     # ports, sync, versions, RPC surface
```

If DNS is unavailable, dial a public peer explicitly (testnet-20 listens on `26521`):

```bash
./target/release/kaspad --testnet --netsuffix=20 \
  --addpeer=95.111.236.186:26521 --utxoindex
```

`testnet-110` and `devnet-111` still ship `dns_seeders: &[]` and remain closed-mesh presets.

### Hardware — what is actually supported

**The node and both miners are CPU-only.** `kaspad`, `misaminer` and `kaspa-pq-miner` contain no
GPU code path; Layer-0 mining is BLAKE2b-512 on the CPU (rayon). There is no GPU miner to install.
macOS (Apple Silicon and Intel), Linux (x86_64 and arm64) and Windows are all supported hosts.

GPU acceleration exists **only** in the optional, off-by-default PALW inference backend of the
`misaka-palw` crate — `qwen-metal` for Apple Silicon and `qwen-cuda` for NVIDIA. It does **not**
accelerate mining or block validation, and a default node build does not compile it. Metal and CUDA
are the only GPU backends the crate exposes. On testnet-20, PALW and algo-4 acceptance are active;
the repository includes receipt-v3 verification and the real-provider lifecycle path. Running a
continuous GPU/LLM provider is still an operator deployment choice, not a requirement for ordinary
nodes. The LLM side, the provider bond, and how to check both gates from a node are in
[§7 of the participation guide](docs/testnet-participation.md#7-the-llm-side-palw-audited-compute);
the wider "not yet" list is [§9](docs/testnet-participation.md#9-what-you-cannot-do-yet).

## What's different from Kaspa

| Area | misakas (PQ-only) |
|---|---|
| Tx signature | **ML-DSA-87** (pk 2592 B / sig 4627 B); secp256k1/Schnorr/ECDSA disabled at consensus |
| Tx signature context | `kaspa-pq-v2/tx/mldsa87` |
| Sighash | `calc_mldsa87_signature_hash` → 64-byte `Hash64` (domain `kaspa-pq-v2/sighash/mldsa87`) |
| Address | `PubKeyHashMlDsa87` only; payload = **keyed** BLAKE2b-512(`kaspa-pq-v2/address/mldsa87`, vk), 64 B |
| Standard script | ML-DSA-87 P2PKH only (`OP_DUP OP_BLAKE2B_512 OP_DATA64 <64B> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87`); P2SH disabled |
| Consensus identity | 64-byte BLAKE2b-512 (`Hash64`): block hash / txid / merkle roots / UTXO commitment / parents |
| secp256k1 | feature-gated out of both `kaspa-consensus` and the `kaspad` node binary (default `pq-only`) |
| Script caps | `MAX_SCRIPT_ELEMENT_SIZE` = 8192, `MAX_SCRIPTS_SIZE` / `max_signature_script_len` = 16_384 |
| Genesis / tokenomics | new genesis; **~26.013224875B MSK theoretical max = 10B premine** (one main UTXO per network, ML-DSA-87 P2PKH) **+ ~16.013224875B emission** over 30 yr, 1.4%/yr exponential decay (q = 0.986, `coinbase::SUBSIDY_BY_MONTH_TABLE`). `MAX_SOMPI` is a per-amount sanity cap, **not** a hard emission cap — per-block rounding puts live issuance ≈ +44 MSK above the theoretical figure at 10 BPS |

Authoritative design & spec live under [`docs/`](docs/):

- [ADR-0019 — ML-DSA-87 migration](docs/adr/0019-mldsa87-migration.md) (rev 1.2 is the current governing record)
- [Design doc — `docs/kaspa-pq-design-mldsa87.md`](docs/kaspa-pq-design-mldsa87.md)
- [Spec — `docs/kaspa-pq-spec.md`](docs/kaspa-pq-spec.md)
- [Verification runbook — `docs/kaspa-pq-mldsa87-verification-runbook.md`](docs/kaspa-pq-mldsa87-verification-runbook.md)
- [Validator runbook — `docs/validator-runbook.md`](docs/validator-runbook.md)

**Scope of PQ claims** (per the design doc): "tx authorization uses ML-DSA-87", "secp256k1 signing disabled in PQ consensus mode", "64-byte BLAKE2b-512 consensus identity". Transport-layer (network) traffic is **not** PQ unless an ML-KEM hybrid is enabled.

## Releases

Tagged source snapshots are published under [Releases](https://github.com/MISAKA-BTC/misakas/releases). **No prebuilt binaries are attached** — build the tools you need from the tag using [Building from source](#building-from-source) below. If you would rather not set up a local toolchain, the Docker path (`docker/Dockerfile.kaspad`, or `./build-docker-multi-arch.sh` for `linux/amd64` + `linux/arm64`) builds the same tree.

The unified operator CLI is the `misaka` binary from the `misaka-cli` package. The package name is `misaka-cli`, while the installed binary name is `misaka`; build commands should name both explicitly (`-p misaka-cli --bin misaka`) so Cargo never depends on workspace defaults.

DNS hard-inclusion is deliberately liveness-gated: the hard mandatory attestation rule only engages when the active validator set can satisfy the configured quality floor within one block under the conservative single-shard capacity invariant (`max_block_mass / max_attestation_shard_mass`). With the current production cap (`max_block_mass = 500_000`, `max_attestation_shard_mass = 50_200`), a block can carry at most 9 single-attestation shards for the capacity check. If the validator set becomes too large or too evenly distributed for that one-block invariant, the hard gate stays dormant instead of halting the base ledger; monitor rollout/health and validator count before relying on it as an anti-censorship backstop. Because `max_attestation_shard_mass` is a consensus parameter, changing it on a live testnet requires a coordinated upgrade or reset.

## Building from source

  <details>
  <summary>Building on Linux</summary>

  1. Install general prerequisites

      ```bash
      sudo apt install curl git build-essential libssl-dev pkg-config
      ```

  2. Install Protobuf (required for gRPC)

      ```bash
      sudo apt install protobuf-compiler libprotobuf-dev #Required for gRPC
      ```
  3. Install the clang toolchain (required for RocksDB; and for WASM secp256k1 in the optional WASM SDK build)

      ```bash
      sudo apt-get install clang-format clang-tidy \
      clang-tools clang clangd libc++-dev \
      libc++1 libc++abi-dev libc++abi1 \
      libclang-dev libclang1 liblldb-dev \
      libllvm-ocaml-dev libomp-dev libomp5 \
      lld lldb llvm-dev llvm-runtime \
      llvm python3-clang
      ```
  4. Install the [rust toolchain](https://rustup.rs/)

     If you already have rust installed, update it by running: `rustup update`
  5. (optional, WASM SDK only) Install wasm-pack + the wasm32 target
      ```bash
      cargo install wasm-pack
      rustup target add wasm32-unknown-unknown
      ```
  6. Clone the repo
      ```bash
      git clone https://github.com/MISAKA-BTC/misakas
      cd misakas
      ```
  7. Build the node + tools
      ```bash
      cargo build --release -p kaspad -p kaspa-pq-miner -p kaspa-pq-validator -p kaspa-pq-signer -p misaka-cli
      ```
     To check only the unified operator CLI:
      ```bash
      cargo build --release -p misaka-cli --bin misaka
      ls -la target/release/misaka
      ```
     To build `kaspad` with its EVM lane and include `misaka`:
      ```bash
      cargo build --release \
        -p kaspad \
        -p kaspa-pq-miner \
        -p kaspa-pq-validator \
        -p kaspa-pq-signer \
        -p misaka-cli \
        --features kaspad/evm
      ```
     `misaka-cli`'s optional EVM send / PREA signing commands use the `evm-send` feature:
      ```bash
      cargo build --release -p misaka-cli --bin misaka --features evm-send
      ```
  </details>

  <details>
  <summary>Building on Windows</summary>

  1. [Install Git for Windows](https://gitforwindows.org/) or an alternative Git distribution.

  2. Install [Protocol Buffers](https://github.com/protocolbuffers/protobuf/releases/download/v21.10/protoc-21.10-win64.zip) and add the `bin` directory to your `Path`

  3. Install [LLVM-15.0.6-win64.exe](https://github.com/llvm/llvm-project/releases/download/llvmorg-15.0.6/LLVM-15.0.6-win64.exe)

      Add the `bin` directory of the LLVM installation (`C:\Program Files\LLVM\bin`) to PATH, and set `LIBCLANG_PATH` to point to the `bin` directory as well.

      **IMPORTANT (WASM SDK only):** Due to C++ dependency configuration issues, LLVM `AR` on Windows may misbehave when switching between WASM and native C++ compilation. After installing LLVM, copy or rename `LLVM_AR.exe` to `AR.exe` in the target `bin` directory.

  4. Install the [rust toolchain](https://rustup.rs/) (`rustup update` if already installed)
  5. (optional, WASM SDK only) `cargo install wasm-pack` and `rustup target add wasm32-unknown-unknown`
  6. Clone the repo
      ```bash
      git clone https://github.com/MISAKA-BTC/misakas
      cd misakas
      ```
 </details>

  <details>
  <summary>Building on Mac OS</summary>

  1. Install Protobuf (required for gRPC)
      ```bash
      brew install protobuf
      ```
  2. Install llvm.

      The default XCode `llvm` does not support WASM build targets. To build the optional WASM SDK on macOS, install `llvm` from homebrew:
      ```bash
      brew install llvm
      ```

      **NOTE:** Homebrew keg locations vary; use `brew list llvm` to find yours and adjust the paths below. Then add to your `~/.zshrc`:
      ```bash
      export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
      export LDFLAGS="-L/opt/homebrew/opt/llvm/lib"
      export CPPFLAGS="-I/opt/homebrew/opt/llvm/include"
      export AR=/opt/homebrew/opt/llvm/bin/llvm-ar
      ```
      and `source ~/.zshrc`.
  3. Install the [rust toolchain](https://rustup.rs/) (`rustup update` if already installed)
  4. (optional, WASM SDK only) `cargo install wasm-pack` and `rustup target add wasm32-unknown-unknown`
  5. Clone the repo
      ```bash
      git clone https://github.com/MISAKA-BTC/misakas
      cd misakas
      ```
 </details>

 <details>
 <summary>Building with Docker</summary>

  ```sh
  docker build -f docker/Dockerfile.kaspad -t kaspad:latest .
  ```

  Replace `Dockerfile.kaspad` with the appropriate Dockerfile for your target. For multi-arch builds use `./build-docker-multi-arch.sh --tag <tag> --artifact kaspad [--arches "linux/amd64 linux/arm64"] [--push]` (requires Docker Buildx).
 </details>

## Running a testnet node

Start a misakas testnet node (network id `testnet-20`; PQ, DNS finality, PALW, algo-4 and the Compute Set registry are all active from genesis):

```bash
cargo run --release --bin kaspad -- --testnet --netsuffix=20 --utxoindex --rpclisten-borsh=default
```

`=default` resolves to the network's standard loopback port, so you never have to memorize the
numbers. Add `--rpclisten-json=default` too if a JSON WebSocket client (e.g. a browser app or an
explorer backend) needs to connect locally.

- To **join the public testnet**, the node discovers peers via the misakas DNS seeders
  (`seeder1.misakascan.com` / `seeder3.misakascan.com`) automatically — the same seed names, now
  resolving testnet-20. **testnet-20's P2P port is `26521`** (testnet-200 used `26511`; each public
  net gets its own port so a node on the wrong suffix fails fast instead of cross-handshaking) —
  make sure it isn't blocked outbound. If discovery is slow, bootstrap explicitly against a public
  node: `--addpeer=95.111.236.186:26521` — the address the seeders themselves advertise — (or
  `--connect=…` to use only that peer).
  Block explorer: [misakascan.com](https://misakascan.com).
- `--utxoindex` is required for wallet/validator funding lookups.
- **gRPC is always on by default** (loopback, `127.0.0.1:26210` on testnet) even with no RPC flag,
  so the **miner needs no extra flag** — it connects over gRPC. **wRPC (Borsh / JSON) is off by
  default** and must be enabled with `--rpclisten-borsh` / `--rpclisten-json`; it is required by the
  CLI wallet and the `kaspa-pq-validator` sidecar (which speak wRPC, **not** gRPC).
- **Connecting a wallet / RPC client — pick the right port.** Default **testnet-20** ports:
  **gRPC** `26210` (protobuf over TCP, default-on at loopback), **wRPC Borsh** `27210`
  (the CLI wallet & validator transport, WebSocket — enable with `--rpclisten-borsh=default`),
  **wRPC JSON** `28210` (WebSocket — enable with `--rpclisten-json=default`). Mainnet uses
  `26110/27110/28110`; devnet `26610/27610/28610`. (Only the P2P port differs between testnet-20
  and the deprecated testnet-200; the RPC ports are shared by every testnet suffix.)
  The `kaspa-pq` CLI wallet connects over **wRPC Borsh** — point it at `27210`, **not** the gRPC
  port `26210` (a wallet pointed at gRPC fails with `WebSocket protocol error: httparse err`
  or `WebSocket is not connected`, because gRPC is not a WebSocket). In the wallet REPL:
  `server 127.0.0.1:27210` → `connect`. (P2P is a separate, non-RPC port: `26521`.)
- **Headless balance (no interactive wallet).** For scripting / monitoring, query a balance in one
  shot over wRPC:
  `kaspa-pq-validator balance --node-rpc 127.0.0.1:27210 --address misakatest:q… [--address …] [--network testnet-20]`.
  It prints `address <sompi> <MSK> MSK` per line (plus `TOTAL` for several) to stdout — connection /
  sync notes go to stderr, so `… balance --address misakatest:q… | awk '{print $2}'` yields just the
  sompi. The node must run `--utxoindex`.
- Add `--enable-unsynced-mining` **only** when bootstrapping a brand-new isolated network with no peers (mining before you have synced to the public testnet would fork from genesis).

Mine to a **64-byte** ML-DSA-87 (`misakatest:`) address — legacy 32-byte addresses are rejected:

```bash
cargo run --release --bin kaspa-pq-miner -- --node-grpc 127.0.0.1:26210 --network-id testnet-20 \
  --blocks 0 --min-block-interval-ms 250 --pay-address <misakatest:...>
```

## Running a validator (testnet)

The `kaspa-pq-validator` sidecar connects to a local node over wRPC and attests while its ML-DSA-87 stake bond is active. See [docs/validator-runbook.md](docs/validator-runbook.md). Quickstart:

```bash
# 1. generate a validator key + print its funding address
kaspa-pq-validator keygen --out val.seed --network testnet
# 2. send funds to the printed funding address (mine to it, or transfer from another wallet)
# 3. stake a bond. testnet-20's entry floor is 10 MSK = 1e9 sompi (testnet-200 kept the
#    production 20,000,000 MSK floor; this net lowers ENTRY so a quorum is mining-fundable).
#    Omit --fee to auto-size it (mass-based; the flat floor is too low for the 2592-byte pubkey).
kaspa-pq-validator bond --node-rpc 127.0.0.1:27210 --validator-key val.seed \
  --amount 1000000000 --network testnet-20
# 4. run the validator daemon (attests every epoch while the bond is active)
kaspa-pq-validator run --node-rpc 127.0.0.1:27210 --validator-key val.seed \
  --stake-bond <txid:index> --signed-epoch-db val.testnet-20.state --network testnet-20 --attest-poll-secs 3
```

> Note: the funding/`run`/`bond` subcommands want the **full** network id (`testnet-20`); `keygen`'s `--network` takes the short form (`testnet`). Use a **fresh** `--signed-epoch-db` per network — reusing one across networks trips the anti-equivocation guard on overlapping epoch numbers.

The validator attests the one current canonical-ready epoch per round; the round cadence is `--attest-poll-secs` (default **3 s**). Every misakas network runs at **10 BPS**, so an attestation epoch (`attestation_epoch_length_blue_score = 100`) is only ~10 s of wall-clock — the 3 s default keeps a single validator caught up on every network.

Once enough stake has attested across the recent epochs, `getDnsConfirmation` reports `dnsConfirmed: true` plus a `lastDnsConfirmedAnchor` (the stake-confirmed finality point — treat THIS as DNS-final, not the pov-dependent `blockHash` sink). testnet-20 confirmation is **two-dimensional**: `WorkDepth ≥ 100` and `StakeDepth ≥ 5000` (inherited from the staging shape), with the validator-entry floors lowered to one validator and a 10 MSK bond. Per-block finality is queryable: `getDnsConfirmation` accepts an optional `blockHash` and answers whether that block is DNS-final (`blockIsDnsFinal` / `blockIsConfirmedAnchor`); the explorer's **DNS Finality** page lists the confirmed chain in order.

### Remote signer / HSM (optional, ADR-0015)

`kaspa-pq-signer` is a standalone daemon that holds the ML-DSA-87 validator key **outside** the validator process and answers sign requests over a `0700` (owner-only) Unix domain socket, enforcing a signing policy (`permissive` / `audit-only` / `strict`), a `strict`-policy anti-equivocation guard (backed by a crash-consistent `SignedEpochStore`), and a tamper-evident hash-chained audit log. A compromised validator node then cannot exfiltrate the key or double-sign.

```bash
kaspa-pq-signer --socket /run/kaspa-pq-signer.sock --key val.seed \
  --state-dir ./kpq-signer-state --policy strict
```

This is a **software** signer; a hardware-HSM / PKCS#11 backend and HA failover are out of scope (see [docs/adr/0015-remote-signer-hsm-protocol.md](docs/adr/0015-remote-signer-hsm-protocol.md)). The local key-file signer used by `kaspa-pq-validator` above remains the default.

<details>
<summary>Using a configuration file</summary>

```bash
cargo run --release --bin kaspad -- --configfile /path/to/configfile.toml   # or -C /path/...
```
The config file is a list of `<CLI argument> = <value>` lines. Pass `--help` to view all arguments:
```bash
cargo run --release --bin kaspad -- --help
```
</details>

<details>
<summary>wRPC</summary>

The wRPC subsystem is disabled by default in `kaspad` and is enabled via `--rpclisten-json=<interface:port>` (or `=default`) and `--rpclisten-borsh=<interface:port>` (or `=default`). It is a WebSocket-framed RPC supporting [Borsh](https://borsh.io/) (inter-process; client and server must be built from the same codebase) and JSON (data-structure-version-agnostic; connect with any WebSocket library) encodings.
</details>

## Benchmarking & Testing

<details>
<summary>Tests</summary>

```bash
cd misakas
cargo test --release
# or, with nextest installed:
cargo nextest run --release
```
</details>

<details>
<summary>Lints</summary>

```bash
cd misakas
./check
```
The CI lints job also runs `scripts/pq-ci-guard.sh`, which hard-gates that neither `kaspa-consensus` nor `kaspad` link secp256k1.
</details>

<details>
<summary>Benchmarks</summary>

```bash
cd misakas
cargo bench
```
</details>

<details>
<summary>Simulation framework (Simpa)</summary>

```bash
cargo run --release --bin simpa -- --help
```
Note: ML-DSA mass caps the per-block tx count (~197), so very high `--tpb` may exceed the compute-mass limit.
</details>

<details>
<summary>Logging</summary>

Logging in `kaspad` and `simpa` is [filtered](https://docs.rs/env_logger/0.10.0/env_logger/#filtering-results) via the `RUST_LOG` env var or the `--loglevel` argument, e.g.:
```
(cargo run --bin kaspad -- --loglevel info,kaspa_rpc_core=trace,consensus=trace) 2>&1 | tee ~/misakas.log
```
</details>

<details>
<summary>Override consensus parameters</summary>

Experiment with non-standard consensus parameters in non-mainnet environments via `--override-params-file <path>`. See [docs/override-params.md](docs/override-params.md).
</details>

## Upstream & License

misakas is a fork of [rusty-kaspa](https://github.com/kaspanet/rusty-kaspa) (the Rust Kaspa full-node by the Kaspa developers). All upstream credit goes to the Kaspa project; the post-quantum migration is layered on top. Distributed under the same ISC license — see [LICENSE](LICENSE).
