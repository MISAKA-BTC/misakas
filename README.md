<h1>misakas — post-quantum (PQ-only) Kaspa</h1>

**misakas** is a post-quantum, **PQ-only** fork of [rusty-kaspa](https://github.com/kaspanet/rusty-kaspa). It replaces Kaspa's secp256k1/Schnorr transaction authorization with **ML-DSA-87** (FIPS 204, NIST category 5) and makes every non-PQ path — legacy secp256k1/Schnorr/ECDSA signatures, legacy addresses, and P2SH — **unrepresentable at the consensus, mempool, and wallet layers**. It is a new, independent network with its own genesis; it is **not** compatible with Kaspa or with any prior kaspa-pq chain state, UTXO set, or address.

The node binary is still named `kaspad` and the crates keep their upstream `kaspa-*` names (this is a fork, not a rename); the **network**, addresses (`misaka…` mainnet / `misakatest…` testnet / `misakadev…` devnet), and project branding are misakas.

## Release status

The public network is **`testnet-12`** (R-core+, ADR-0152 v3.1), launched on 2026-09-25/26 JST from
release commit **`0e8ec984e`**. Its post-launch releases add activation fences on the same chain;
the current identity is the one [`release.json`](release.json) declares (re-pinned in `3702124` for
the DAA-3,600 flag day). Run it with `kaspad --testnet --netsuffix=12` or `misaka --network
testnet-12` (the CLI's default) and check the two startup lines:

```
Consensus params fingerprint: 254509533bb693ced0fed823a4c25e166ba2542d576e4021b0e4b4d6fe4079e1 (network testnet-12)
Consensus fence schedule: 750, 1000, 1300, 1700, 2000, 3600 (schedule id 1e39c738b97a695c…)
```

* DAA 750: the post-launch fences, including the fixes for the two CRITICAL launch issues; a node
  still on the launch release (`b8564b88…`) is refused by upgraded peers from DAA 750.
* DAA 1,000: bond maturity (ADR-0065 D1). DAA 1,300: the second flag day (floor-refusal retry and
  the retroactive seat-lock life). DAA 1,700: the capacity fences at ρ = 10. DAA 2,000: the IR flag
  day (`palw_tir_v1`, RFC-0002). DAA 3,600: `palw_tir_fence2` alone; `palw_model_court_window` stays
  dormant.
* genesis `a27f8f44fe4d91a5…`.

A build whose fence schedule lacks a height is refused from that height. The values to trust are
the ones your own node prints, compared with `release.json`.

Read **[docs/t12-launch-2026-09-25.md](docs/t12-launch-2026-09-25.md)** before relying on the
network: what the release contains, the known issues, and when a payment may be treated as final —
**never count blocks, blue-score depth or DAA on testnet-12**; use finality depth (600 blue, about
4 h) or a `Final` PALW anchor (`misaka palw settlement --daa <d> --min-depth <n>`; `misaka wallet
utxo list` shows each output's depth). To produce or verify:
[docs/testnet12-join-mining.md](docs/testnet12-join-mining.md).

PQ-only consensus and the DNS-finality overlay are active from genesis on every defined network
(`pq_activation_daa_score = 0`, `dns_activation_daa_score = 0`). The `mainnet` parameter set is
**defined but NOT launched or endorsed for production** — do not run `--mainnet` expecting a live or
supported network. testnet-10 and testnet-11 are retired.

## What testnet-12 is

testnet-12 runs **PALW ConsensusV2** (ADR-0042): blocks are won by a lottery over *verified LLM
inference* rather than by hashing alone, and the work a block claims is settled by other nodes
re-deriving it. Three things follow that a hash chain does not have.

**A block's work is a claim, and claims are judged.** A producer publishes an execution and the
material behind it; a panel of bonded seats re-runs the job and files signed receipts; a licensed
claim can still be disputed, and a dispute is settled by bisecting to a single arithmetic step and
adjudicating it. Weight is credited only once that lattice turns over — `safe_weight` moving off
zero is the network working, not a formality.

**Producing needs a bond.** Attempts name a bond the chain holds, so an unregistered node can sync,
serve and verify, but not produce. The genesis registry seats the initial set.

**The cadence is frozen at 120 s per block** (`PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS`), refused at
parameter construction if anything tries to change it. Inference takes real time, and a block
interval shorter than the work it certifies is a chain that certifies nothing.

Three execution classes ship in the genesis:

| class | model | needs a model file? |
|---|---|---|
| **PALW-BASE-0** (the floor) | deterministic integer model, pure Rust in this tree | **no** — no GPU, no download |
| **Qwen2.5-1.5B graph-v7 @8,192** | Qwen2.5-1.5B-Instruct, A16 conversion at an 8,192 context | yes (`qwen25-convert --a16 --n-ctx 8192`) |
| **Qwen2.5-1.5B graph-v7 @2,097,152** | the same weights at a 2,097,152 context | yes (`qwen25-convert --a16 --n-ctx 2097152`) |

Running or verifying a node needs none of them for the floor; producing in a model class needs that
class's artifact, which derives deterministically from the public `Qwen/Qwen2.5-1.5B-Instruct`
weights, and every panel seat re-derives the same bytes or the class does not license. More classes
arrive by on-chain registration (ADR-0135). See [docs/palw-classes-runbook.md](docs/palw-classes-runbook.md)
and the genesis record in [docs/testnet-12-regenesis-2026-09-23.md](docs/testnet-12-regenesis-2026-09-23.md).

Since ADR-0058 a block does not need to win tip selection for its work to count: the whole
mergeset — reds included, which at 120 s cadence and `ghostdag_k = 1` is every block of every
class slower than the floor — creates claims, is verified and is paid. Since ADR-0137 a block draws
against one network-wide work target, and a class's share is a result of the draw, not an input.

## Joining testnet-12

The recommended operator path is the ADR-0122 CLI. It verifies the node, identity, model, key,
funds, Bond registry, artifact, panel capability and fee output before writing
`~/.misaka/mining.toml`:

```bash
misaka --network testnet-12 mining setup
misaka --network testnet-12 mining start --print-command
misaka --network testnet-12 mining start
```

For an existing Bond, inspect the exact outpoint without a key or class id:

```bash
misaka --network testnet-12 bond status --bond <txid>:<index>
```

`REGISTERED` and sufficient sustained collateral are separate results. Never re-run
`--palw-register-bond` for an already registered key; collateral cannot be topped up and one key
registers one Bond for the life of the chain.

The network is permissionless. DNS seeding is live (`seeder1.misakascan.com`), so a fresh node
needs **no flags beyond the network selection**:

```bash
cargo build --release -p kaspad
./target/release/kaspad --testnet --netsuffix=12 --utxoindex
```

The log must show the fingerprint and fence schedule in [Release status](#release-status), or you
are on the wrong ruleset. A datadir from an older chain is refused with a `Genesis mismatch`; move
it aside and resync (details in [docs/testnet12-join-mining.md](docs/testnet12-join-mining.md)).

If DNS seeding is blocked where you run, resolve a seeder once and pass the result for that
invocation (the peer flag currently accepts IP addresses, not hostnames):
`SEEDER_IP=$(dig +short A seeder1.misakascan.com | tail -n1); kaspad ... --addpeer="$SEEDER_IP:26311"`.
Do not copy that resolved IP into permanent configuration; the address behind a seeder is
operational state and can change or be withdrawn.

| I want to… | read |
|---|---|
| run a node / verify the chain | [docs/node-operator.md](docs/node-operator.md) |
| join as a PALW verifier / panel seat | [docs/wiki/Testnet-12-Verification-Participation-JA.md](docs/wiki/Testnet-12-Verification-Participation-JA.md) |
| produce blocks (floor class, no model needed) | [docs/testnet12-join-mining.md](docs/testnet12-join-mining.md) |
| produce or verify with the LLM classes | [docs/palw-classes-runbook.md](docs/palw-classes-runbook.md) |
| run a DNS-finality validator | [docs/validator-runbook.md](docs/validator-runbook.md) |
| know what the release contains and its known issues | [docs/t12-launch-2026-09-25.md](docs/t12-launch-2026-09-25.md) |

## What's different from Kaspa

| Area | misakas (PQ-only) |
|---|---|
| Tx signature | **ML-DSA-87** (pk 2592 B / sig 4627 B); secp256k1/Schnorr/ECDSA disabled at consensus |
| Tx signature context | `kaspa-pq-v2/tx/mldsa87` |
| Sighash | `calc_mldsa87_signature_hash` → 64-byte `Hash64` (domain `kaspa-pq-v2/sighash/mldsa87`) |
| Address | `PubKeyHashMlDsa87` only; payload = **keyed** BLAKE2b-512(`kaspa-pq-v2/address/mldsa87`, vk), 64 B |
| Standard script | ML-DSA-87 P2PKH only (`OP_DUP OP_BLAKE2B_512 OP_DATA64 <64B> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87`); P2SH disabled |
| Consensus identity | 64-byte BLAKE2b-512 (`Hash64`): block hash / txid / merkle roots / UTXO commitment / parents |
| secp256k1 | feature-gated out of `kaspa-consensus` (default `pq-only`); the default `kaspad` links a secp curve only through the EVM lane's `ecrecover` (revm), and a `--no-default-features` `kaspad` links none |
| Script caps | `MAX_SCRIPT_ELEMENT_SIZE` = 8192, `MAX_SCRIPTS_SIZE` / `max_signature_script_len` = 16_384 |
| Genesis / tokenomics | new genesis; **25B MSK max supply = 10B premine** (one main-wallet ML-DSA-87 P2PKH UTXO; community allocations, genesis-bond collateral and fee floats are carved out of it, never minted beside it) **+ 15B network emission** over 20 yr, 5%/yr exponential decay (`coinbase::SUBSIDY_BY_MONTH_TABLE`). The premine constants live in `consensus/core/src/config/premine.rs`; a change there moves every network's genesis hash and is refused at startup until re-pinned (audit M-07) |
| Block cadence | **120 s** on PALW networks (`PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS`), frozen — refused at parameter construction, because a block interval shorter than the inference it certifies certifies nothing |

Authoritative design & spec live under [`docs/`](docs/):

- [ADR-0019 — ML-DSA-87 migration](docs/adr/0019-mldsa87-migration.md) (rev 1.2 is the current governing record)
- [Design doc — `docs/design/kaspa-pq-design-mldsa87.md`](docs/design/kaspa-pq-design-mldsa87.md)
- [Spec — `docs/kaspa-pq-spec.md`](docs/kaspa-pq-spec.md)
- [Verification runbook — `docs/archive/kaspa-pq-mldsa87-verification-runbook.md`](docs/archive/kaspa-pq-mldsa87-verification-runbook.md)
- [Validator runbook — `docs/validator-runbook.md`](docs/validator-runbook.md)
- [PALW provenance map — `docs/palw-registry-map.md`](docs/palw-registry-map.md) — **where the model registry, the artifact hash, the runtime registry, the receipt, the output root, the verification, the metering and the capability profile already are**, field by field, plus the layers that are refused and why (ADR-0079 §7 / R-09). Read it before proposing a provenance layer.
- [ADR index — `docs/adr/README.md`](docs/adr/README.md) (what governs, and what was reversed)

**Scope of PQ claims** (per the design doc): "tx authorization uses ML-DSA-87", "secp256k1 signing disabled in PQ consensus mode", "64-byte BLAKE2b-512 consensus identity". Transport-layer (network) traffic is **not** PQ unless an ML-KEM hybrid is enabled.

## Prebuilt binaries

**Build from source for the live chain today.** testnet-12 shipped as a commit with no tag and no
GitHub release (see [docs/release-process.md](docs/release-process.md) for the signed-tag process
that is meant to replace this). The releases under
[Releases](https://github.com/MISAKA-BTC/misakas/releases) are earlier, retired chains: their
binaries are refused at the handshake. Downloading one is the most common way to end up with a node
that peers, drops after minutes, and mines blocks nobody accepts.

The check is never the tag: it is the fingerprint your node prints at startup and the fence
schedule on the line after it. If either does not match [Release status](#release-status), you are
on the wrong ruleset whatever you downloaded.

The unified operator CLI is the `misaka` binary from the `misaka-cli` package. The package name is `misaka-cli`, while the installed binary name is `misaka`; build commands should name both explicitly (`-p misaka-cli --bin misaka`) so Cargo never depends on workspace defaults.

## Building from source

`cargo build --release` at the workspace root builds everything you need to run a node, a miner, a
validator and the CLI. It does **not** build `misaka-palw-worker`, which is excluded from
`default-members` on purpose: that crate links a **pinned llama.cpp static build** which this
repository deliberately does not contain, so including it would mean a fresh clone could not build
at all. Nothing needs it to run a node — since ADR-0053 there is one execution family and it is BASE-0's,
which is pure Rust in this tree. The worker survives as the runtime the ADR-0034 capability probe
interrogates. If you do want it, build the pinned tree first and say where it is:

```bash
MISAKA_LLAMA_SRC=/path/to/built/llama.cpp cargo build --release -p misaka-palw-worker
```

The build script refuses with instructions rather than guessing a path, and it checks the tree is
**built** (it hashes the CMake cache and the static libraries it links, so the runtime manifest
describes the artifacts the binary is actually made of).


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
     The EVM lane is a default feature of `kaspad` (testnet-12 activates it at DAA 0 and refuses a
     binary built without it), so no extra feature flag is needed. A secp-free `kaspad` needs
     `--no-default-features` and cannot run testnet-12.
     `misaka-cli`'s optional EVM send / PREA signing commands use the `evm-send` feature:
      ```bash
      cargo build --release -p misaka-cli --bin misaka --features evm-send
      ```
  </details>

  <details>
  <summary>Building on Windows</summary>

  **What a Windows build gives you, and what it does not.** `kaspad` builds and runs on
  `x86_64-pc-windows-msvc`: it syncs, serves RPC and runs the validator daemon. The **PALW v2
  agent** path (`--compute-endpoint`) is *not* served on Windows — `misaka-palw-agent-borsh/v1`
  is a Unix-domain-socket protocol whose admission check is peer credentials (`SO_PEERCRED` /
  `getpeereid`), and neither has a Windows equivalent. Passing `--compute-endpoint` on Windows is
  accepted, logs one warning, and leaves v2 compute capability withdrawn; the node keeps
  validating. If you want to run a `palw-agent`, use Linux or macOS — WSL2 and containers both
  work and are what the fleet runs.

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
  7. Build the node + the operator CLI
      ```bash
      cargo build --release -p kaspad -p misaka-cli
      ```
     These two are what CI's `Check (x86_64-pc-windows-msvc)` job builds and smoke-runs on every
     push, so this command is verified rather than assumed. `kaspa-pq-signer` and `palw-agent`
     build on Windows but refuse to run there — both are Unix-domain-socket daemons.
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

Start a misakas testnet node on the live network (`testnet-12`; PQ rules, the DNS-finality overlay
and PALW ConsensusV2 are all active from genesis):

```bash
cargo run --release --bin kaspad -- --testnet --netsuffix=12 --utxoindex --rpclisten-borsh=default
```

`--netsuffix=12` is required: bare `--testnet` selects suffix 10, a retired network.

`=default` resolves to the network's standard loopback port, so you never have to memorize the
numbers. Add `--rpclisten-json=default` too if a JSON WebSocket client (e.g. a browser app or an
explorer backend) needs to connect locally.

- To **join the public testnet**, the node discovers peers via the misakas DNS seeders
  (`seeder1.misakascan.com` … `seeder4.misakascan.com`) automatically. **testnet-12's P2P port is
  `26311`** (mainnet `26111`, devnet `26611`) — make sure it isn't blocked outbound. The four
  seeder names are shared with the other networks and that is safe: a record hands out an IP, each
  network dials it on its OWN default P2P port, and `consensus_params_id` refuses the handshake if
  a node of another network ever answered. A seeder that has nothing healthy to advertise returns
  an empty answer rather than a wrong peer.

  If discovery is slow, resolve a seeder for that invocation and pass the resulting IP to
  `--addpeer` (or `--connect` to use only that peer). The flags currently accept IP addresses,
  not hostnames; do not hard-code a DNS answer's IP in a permanent config. Block explorer:
  **[misakascan.com](https://misakascan.com)**.
- `--utxoindex` is required for wallet/validator funding lookups.
- **gRPC is always on by default** (loopback, `127.0.0.1:26210` on testnet) even with no RPC flag.
  **wRPC (Borsh / JSON) is off by default** and must be enabled with `--rpclisten-borsh` /
  `--rpclisten-json`; it is required by the CLI wallet, `misaka` and the `kaspa-pq-validator`
  sidecar (which speak wRPC, **not** gRPC).
- **Connecting a wallet / RPC client — pick the right port.** The RPC ports are per network TYPE,
  so **every testnet suffix shares them**; only P2P carries the suffix. Default **testnet** ports:
  **gRPC** `26210` (protobuf over TCP, default-on at loopback), **wRPC Borsh** `27210`
  (the CLI wallet & validator transport, WebSocket — enable with `--rpclisten-borsh=default`),
  **wRPC JSON** `28210` (WebSocket — enable with `--rpclisten-json=default`). Mainnet uses
  `26110/27110/28110`; devnet `26610/27610/28610`.
  The `kaspa-pq` CLI wallet connects over **wRPC Borsh** — point it at `27210`, **not** the gRPC
  port `26210` (a wallet pointed at gRPC fails with `WebSocket protocol error: httparse err`
  or `WebSocket is not connected`, because gRPC is not a WebSocket). In the wallet REPL:
  `server 127.0.0.1:27210` → `connect`. (P2P is a separate, non-RPC port: `26311` on testnet-12.)
- **Headless balance (no interactive wallet).** For scripting / monitoring, query a balance in one
  shot over wRPC:
  `kaspa-pq-validator balance --node-rpc 127.0.0.1:27210 --address misakatest:q… [--address …] [--network testnet-12]`.
  It prints `address <sompi> <MSK> MSK` per line (plus `TOTAL` for several) to stdout — connection /
  sync notes go to stderr, so `… balance --address misakatest:q… | awk '{print $2}'` yields just the
  sompi. The node must run `--utxoindex`.
- Add `--enable-unsynced-mining` **only** when bootstrapping a brand-new isolated network with no peers (mining before you have synced to the public testnet would fork from genesis).

testnet-12 cannot be mined with `kaspa-pq-miner` or `misaminer`: they do not create the required
PALW attempt envelope. Block production runs inside `kaspad --palw-produce`; use
`misaka mining setup/start` rather than an external hash miner.

## Running a validator (testnet)

The `kaspa-pq-validator` sidecar connects to a local node over wRPC and attests while its ML-DSA-87
stake bond is active. `misaka --network testnet-12 validator setup` walks through key, funding, bond
and service; the sidecar's own subcommands (`keygen`, `bond`, `unbond`, `run`, `status`, …) are
forwarded by `misaka validator` with `--network` and the RPC endpoint filled in. The step-by-step,
including the flags each subcommand needs, is
[docs/validator-runbook.md](docs/validator-runbook.md).

testnet-12 runs mainnet's DNS-finality set: a validator bond of at least **20,000,000 MSK**, and DNS
finality activates only once **at least 6 validators** hold **at least 120,000,000 MSK** of active
stake between them. Until then no anchor is DNS-confirmed (Bootstrap). Use a **fresh**
`--signed-epoch-db` per network — reusing one across networks trips the anti-equivocation guard on
overlapping epoch numbers.

DNS finality is a vote (ADR-0128): the confirmed anchor is the newest epoch anchor that validators
holding more than two thirds of the counted bonded stake have attested and precommitted to, and the
DNS stake reorg gate refuses chains that abandon it. `getDnsConfirmation` reports `dnsConfirmed`
and `lastDnsConfirmedAnchor` (treat that as DNS-final, not the pov-dependent `blockHash` sink), and
with an optional `blockHash` answers whether that block is DNS-final (`blockIsDnsFinal` /
`blockIsConfirmedAnchor`). The sidecar and the in-node validator precommit by themselves from
`getPrecommitDuty` and keep a second safety log beside the signed-epoch db (`val.state` → `val.precommits.json`) — back it up
with the seed. PALW does not wait for any of this: its payments settle on PALW anchors
(`misaka palw settlement`).

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
MISAKA_PALW_POW_FIXTURE=1 cargo test --workspace --features "devnet-prealloc,evm"
```

Two of those are not optional and the failures they prevent are confusing:

- **`MISAKA_PALW_POW_FIXTURE=1`** — the devnet daemon tests verify PALW attempts, and without the
  fixture they demand a real worker runtime at startup and take the test process down with them.
- **`--features "devnet-prealloc,evm"`** — the integration crate needs the preallocated devnet UTXO
  set, and `evm` is part of the shipped `kaspad`, so testing without it tests a different binary.

`cargo test --workspace` names every member **including** `misaka-palw-worker`, which
`default-members` otherwise excludes — so a full-workspace run also wants
`MISAKA_LLAMA_SRC=/path/to/built/llama.cpp`. Without the pinned tree, scope the run instead
(`-p kaspa-consensus-core`, `-p kaspad`, …).
</details>

<details>
<summary>Lints and CI gates</summary>

**One command runs what CI runs:**

```bash
cd misakas
bash scripts/ci-gates.sh              # every gate
bash scripts/ci-gates.sh --list       # what they are, and which CI job each mirrors
bash scripts/ci-gates.sh --group fast # the ones that need no cargo build (seconds, not minutes)
bash scripts/ci-gates.sh fmt clippy   # just these
```

It exits non-zero if any gate fails — the exit status is the NUMBER of failed gates — and it
prints one line per gate with that gate's own exit code, plus what each gate actually covered.
A gate that exits 0 without leaving evidence in its log that it ran is reported as a failure,
because `cargo nextest run` that selected nothing also exits 0. Logs land in `target/ci-gates/`.

The `Gates` job in `.github/workflows/ci.yaml` runs this same script, so there is one spelling
of each gate; `workflow-parity` inside it fails the build if the script and the workflow ever
drift apart.

`./check` (or `./check.ps1` on Windows) still works and now delegates its lint half here, then
runs the wasm32 clippy passes on top.

The `fast` group needs no Rust toolchain at all: it checks that `rust-toolchain.toml` is
honoured by every workflow job, that `ci.yaml` spells each gate the same way (`workflow-parity`),
and it runs the derived-artifact verifiers (`misaka-palw-artifact-conformance.py`,
`misaka-palw-derive-stranger.py`, a round trip between them, and
`misaka-palw-artifact-thirdparty.py`). The last one wants two foreign parsers, which are
deliberately NOT workspace dependencies:

```bash
python3 -m venv /tmp/artifact-venv
/tmp/artifact-venv/bin/pip install mido==1.3.3 numpy-stl==4.0.0
CI_GATES_THIRDPARTY_PYTHON=/tmp/artifact-venv/bin/python bash scripts/ci-gates.sh --group fast
```

The `pq-guard` gate runs `scripts/pq-ci-guard.sh`, which hard-gates that `kaspa-consensus`, the wallet and validator crates, and a `--no-default-features` `kaspad` link no secp curve (the default `kaspad` carries the EVM lane and its `ecrecover`).
</details>

<details>
<summary>The pinned toolchain</summary>

`rust-toolchain.toml` pins the compiler. Every workflow job reads that file rather than
repeating the version, and `scripts/ci-toolchain-pin-check.py` fails the build if any job stops
doing so — before this file existed, every job installed `dtolnay/rust-toolchain@<sha>  # stable`,
which pins the ACTION and leaves the COMPILER floating (the action's `toolchain` input defaults
to `stable`), while the Lints job alone pinned a different version.

`rustup` picks the pinned toolchain up automatically for any `cargo` command run inside the
checkout. The version and the measurement that chose it are documented in the file itself.
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
