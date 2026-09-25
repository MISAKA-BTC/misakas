<h1>MISAKA — a post-quantum, AI proof-of-work L1</h1>

[![Tests](https://github.com/MISAKA-BTC/misakas/actions/workflows/ci.yaml/badge.svg)](https://github.com/MISAKA-BTC/misakas/actions/workflows/ci.yaml)
[![License: ISC](https://img.shields.io/badge/license-ISC-blue.svg)](LICENSE)

**misakas** is the node for MISAKA: a BlockDAG whose transactions are authorized **only** by
post-quantum signatures (ML-DSA-87, FIPS 204), and whose blocks are won by **verified LLM
inference** (PALW) rather than by hashing alone.

[misakaoptions.com](https://misakaoptions.com) · [misakascan.com](https://misakascan.com) (explorer) ·
[wallet.misakascan.com](https://wallet.misakascan.com) · [Mainnet readiness](docs/mainnet-readiness.md) ·
[Architecture](docs/architecture/overview.md) · [Join testnet-12](docs/testnet12-join-mining.md) · [Docs](docs/README.md) · [ADRs](docs/adr/README.md)

## Release status

| | |
|---|---|
| **Stage** | Public testnet. **Not** a mainnet release candidate yet — see [Mainnet readiness](docs/mainnet-readiness.md) |
| **Network** | `testnet-12` (R-core+, ADR-0152 v3.1), launched 2026-09-25/26 JST |
| **Release** | commit **`0e8ec984e`** (current `main`; no tag yet) |
| **Consensus** | **Not frozen.** Two CRITICAL fixes arrive as post-launch activation fences ([launch note §2](docs/t12-launch-2026-09-25.md)) |
| **Consensus params fingerprint** | `b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f` |
| **Genesis** | `a27f8f44fe4d91a5…` · schedule id `93da24cc60f7a77e…` |
| **Mainnet** | parameter set defined; **not launched, not endorsed** — do not run `--mainnet` expecting a live network |

Your own node prints the fingerprint and the fence schedule on startup; those two lines, not this
page, are the check. Before relying on testnet-12, read the
**[launch note](docs/t12-launch-2026-09-25.md)**: what the release contains, its known issues, and
when a payment may be treated as final — **never count blocks, blue-score depth or DAA on
testnet-12**; use finality depth (600 blue, about 4 h) or a `Final` PALW anchor.

## Built on MISAKA

### MISAKA Options — the model store, live on testnet-12

**[Launch the app](https://misakaoptions.com)** · [Explorer](https://misakascan.com) ·
[How it works](web/misaka-options/README.md) · [ADR-0095](docs/adr/0095-a-position-is-a-membership-not-an-income.md)

On MISAKA the models that mine blocks are on-chain objects. Each has a registered graph, an owner,
versions, and a count kept by consensus of the inferences it served. **Every model can have a
store**, and the store is part of consensus, not a contract deployed on top. The chain runs the
curve, the reserve, the fees and the settlement:

- **Open a store** by paying the opening deposit into the model's line (at least 1,000,000 MSK on
  testnet-12). It is locked in the curve for good.
- **Join or leave** a model's store: buy whole memberships from the model's own curve, or sell them
  back to it. Each move burns 5 % of its MSK and pays the model's owner 5 %.
- **Mining feeds the store.** Part of each block reward for a model's work buys into its curve, so
  a used model's price rises with its use. **No holder is ever paid**
  ([ADR-0091](docs/adr/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)): a membership is
  access that the owner declares on chain, not an income.
- **Three ways in:** the web app ([misakaoptions.com](https://misakaoptions.com), with MISAKA
  Wallet or MetaMask), the EVM lane (chain id `0x4D534B`, standard `eth_*`), or the CLI
  (`misaka model list`, `misaka position quote | buy | sell`).

**Where it stands.** The market has been part of consensus since testnet-11, where it activated at
DAA 1,900. On testnet-12 it is active from genesis. The chain itself answers what it holds today:
`getPalwModelMarket` over wRPC, or the app's **Models** and **Rankings** pages. At testnet-12's
launch the three genesis classes had no store opened yet, and two of them are still
`Prefetching` under the model registry, so they refuse buys until their panels prove readiness.
This README carries no usage figures on purpose: a number typed here would be out of date by the
next block.

## Mainnet readiness at a glance

Only what the tree can show is marked done. The evidence for every line is in
[docs/mainnet-readiness.md](docs/mainnet-readiness.md).

| | |
|---|---|
| ✅ | One-command CI gates, reproducible locally ([`scripts/ci-gates.sh`](#engineering-discipline)) |
| ✅ | Pinned Rust toolchain, enforced in every CI job |
| ✅ | secp256k1 linked out of consensus and the node, gated in CI |
| ✅ | Dependency advisories gated in CI (`cargo-deny`) |
| ✅ | Internal consensus and PALW security audits, findings fixed in the open |
| ✅ | Activation-fence upgrade path exercised on a live public network (testnet-11) |
| ✅ | Public seeders, explorer, web wallet, RPC |
| 🟡 | Reproducible release binaries — the testnet-12 build matched byte-for-byte across two checkouts; not yet independently reproduced or checked in CI |
| 🟡 | Multi-platform binaries — the release workflow builds Linux x86_64/ARM64, Windows x86_64 and macOS ARM64; testnet-12 itself ships x86_64 Linux only |
| 🟡 | External review — commissioned static reviews of code snapshots (2026-06-22 Kaspa-diff and EVM/NFT, 2026-08-21 PALW), all answered in the tree; no full-scope independent audit of a release |
| ⏳ | Consensus freeze and a tagged release candidate |
| 🟡 | Signed checksums (Sigstore), provenance attestations and SBOM — in the release workflow ([release process](docs/release-process.md)); no release cut with them yet |
| ⏳ | Signed release tags |
| ⏳ | Fuzzing of blocks, transactions and RPC |
| ⏳ | Recovery drills (crash, DB corruption, partition) run and published as reports |
| ⏳ | Long public soak with no consensus change |
| ⏳ | Independent external security audit |

<details>
<summary>testnet-11 and earlier</summary>

testnet-11 (Relaunch 5f) was the public network until 2026-09-25. Current `main` no longer builds a
testnet-11 node; to keep running one, build commit **`1f98d3bf4`**, select it with
`--testnet --netsuffix=11`, and verify fingerprint
`79b49c238c46b0d97ab9b46d79fd5f85f8b50da623921a53f0af361515d50640`. Its flag days, fingerprint
history, relaunches and rollout instructions are kept in
**[docs/history/testnet-11.md](docs/history/testnet-11.md)**. Testnet-10 is stopped.

</details>

## What MISAKA is

**Post-quantum only.** misakas is a fork of [rusty-kaspa](https://github.com/kaspanet/rusty-kaspa)
that replaces Kaspa's secp256k1/Schnorr transaction authorization with **ML-DSA-87** (NIST
category 5) and makes every non-PQ path — legacy signatures, legacy addresses and P2SH —
**unrepresentable at the consensus, mempool and wallet layers**. It is a new, independent network
with its own genesis; it is **not** compatible with Kaspa or with any prior kaspa-pq chain state,
UTXO set or address.

**AI proof-of-work.** Blocks come from **PALW ConsensusV2** (ADR-0042): a lottery over *verified
LLM inference*, where the work a block claims is settled by other nodes re-deriving it. Three
things follow that a hash chain does not have.

**A block's work is a claim, and claims are judged.** A producer publishes an execution and the
material behind it; a panel of bonded seats re-runs the job and files signed receipts; a licensed
claim can still be disputed, and a dispute is settled by bisecting to a single arithmetic step and
adjudicating it. Weight is credited only once that lattice turns over — `safe_weight` moving off
zero is the network working, not a formality.

**Producing needs a bond.** Attempts name a bond the chain holds, so an unregistered node can sync,
serve and verify, but not produce. The genesis registry seats the initial set; model classes pass
through a lifecycle before they pay ([join guide §8](docs/testnet12-join-mining.md)).

**The cadence is frozen at 120 s per block** (`PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS`), refused at
parameter construction if anything tries to change it. Inference takes real time, and a block
interval shorter than the work it certifies is a chain that certifies nothing.

The node binary is still named `kaspad` and the crates keep their upstream `kaspa-*` names (this is
a fork, not a rename); the **network**, addresses (`misaka…` mainnet / `misakatest…` testnet /
`misakadev…` devnet), and project branding are MISAKA.

**How it fits together** — each part of the protocol, the ADRs that govern it today, and the code
that implements it, with the 94 crates sorted into core, PALW, operator, application and
development groups: **[docs/architecture/overview.md](docs/architecture/overview.md)**.

## The stack: one chain, one app

misakas is built the way [Hyperliquid](https://hyperliquid.xyz) is built, and borrows its shape on
purpose: **a purpose-built L1 whose native state machine does the product's core work, an EVM
lane beside it that reads and writes that state, and a first-party web app on its own domain.**
Hyperliquid has its L1 (HyperCore + HyperEVM) and the app at `hyperliquid.xyz`; MISAKA has this
repository and the app at **[misakaoptions.com](https://misakaoptions.com)**.

| | Hyperliquid | MISAKA |
|---|---|---|
| the L1 | HyperCore — the order book lives in consensus | **misakas** (this repo) — PQ-only BlockDAG; blocks are won by *verified LLM inference* (PALW), and the model store lives in consensus |
| the EVM lane | HyperEVM, reading and writing HyperCore through precompiles | the MISAKA EVM lane (chain id `0x4D534B`), reading the fold through read precompiles and writing through the native doors ([ADR-0089](docs/adr/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)) |
| the app | **hyperliquid.xyz** — trade perps and spot | **[misakaoptions.com](https://misakaoptions.com)** — open, join and leave AI models' stores |
| the explorer | the Hyperliquid explorer | **[misakascan.com](https://misakascan.com)** |
| the wallet | any EVM wallet | **[MISAKA Wallet](https://wallet.misakascan.com)** (ML-DSA-87), MetaMask or any EIP-1193 wallet on the EVM lane |
| run a node | [`hyperliquid-dex/node`](https://github.com/hyperliquid-dex/node) | [Running a testnet node](#running-a-testnet-node) below |

The app itself is described in [Built on MISAKA](#built-on-misaka) above.

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
| Genesis / tokenomics | new genesis; **28B MSK cap = 13B premine** (40 vaults × 0.1B + 1 main × 9B, ML-DSA-87 P2PKH) **+ 15B network emission** over 20 yr, 5%/yr exponential decay (`coinbase::SUBSIDY_BY_MONTH_TABLE`). The premine constants live in `consensus/core/src/config/premine.rs`; a change there moves every network's genesis hash and is refused at startup until re-pinned (audit M-07) |
| Block cadence | **120 s** on PALW networks (`PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS`), frozen — refused at parameter construction, because a block interval shorter than the inference it certifies certifies nothing |

Authoritative design & spec live under [`docs/`](docs/):

- [ADR-0019 — ML-DSA-87 migration](docs/adr/0019-mldsa87-migration.md) (rev 1.2 is the current governing record)
- [Design doc — `docs/kaspa-pq-design-mldsa87.md`](docs/kaspa-pq-design-mldsa87.md)
- [Spec — `docs/kaspa-pq-spec.md`](docs/kaspa-pq-spec.md)
- [Verification runbook — `docs/kaspa-pq-mldsa87-verification-runbook.md`](docs/kaspa-pq-mldsa87-verification-runbook.md)
- [Validator runbook — `docs/validator-runbook.md`](docs/validator-runbook.md)
- [PALW provenance map — `docs/palw-registry-map.md`](docs/palw-registry-map.md) — **where the model registry, the artifact hash, the runtime registry, the receipt, the output root, the verification, the metering and the capability profile already are**, field by field, plus the layers that are refused and why (ADR-0079 §7 / R-09). Read it before proposing a provenance layer.
- [ADR index — `docs/adr/README.md`](docs/adr/README.md) (what governs, and what was reversed)

**Scope of PQ claims** (per the design doc): "tx authorization uses ML-DSA-87", "secp256k1 signing disabled in PQ consensus mode", "64-byte BLAKE2b-512 consensus identity". Transport-layer (network) traffic is **not** PQ unless an ML-KEM hybrid is enabled.

## Prebuilt binaries

**testnet-12 has no GitHub release yet: build from `0e8ec984e`.** The public fleet runs the x86_64
Linux release build of that commit (glibc 2.39 floor, built with
`contrib/t12-deploy-kit/build-release-local.sh 0e8ec984e`). Two builds from separate checkouts
matched byte-for-byte; their sha256 are in the [launch note](docs/t12-launch-2026-09-25.md), so a
build of your own can be compared against them:

| binary | sha256 |
|---|---|
| `kaspad` | `5a357623c74f8e786cd244aef783855a81d8490222da15bf1a76e3dab987f149` |
| `misaka` | `c80e608aea340cd81eea83a503594568d9c109e271a360a40667b9f50ad4c176` |

Everything on the [Releases](https://github.com/MISAKA-BTC/misakas/releases) page is a testnet-11 or
earlier build, and testnet-12 refuses it at the handshake. Whatever you run, the check is never the
file name or the tag: it is the fingerprint your node prints on startup and the fence schedule on
the line after it. If either does not match [Release status](#release-status), you are on the wrong
ruleset.

From the first release candidate on, releases carry Linux x86_64 and ARM64, Windows and macOS
binaries with a Sigstore-signed `SHA256SUMS`, provenance attestations and an SBOM. How to verify
them is in [docs/release-process.md](docs/release-process.md).

The unified operator CLI is the `misaka` binary from the `misaka-cli` package. The package name is `misaka-cli`, while the installed binary name is `misaka`; build commands should name both explicitly (`-p misaka-cli --bin misaka`) so Cargo never depends on workspace defaults.

## Engineering discipline

**One command runs what CI runs**, and it is the same script the CI `Gates` job calls:

```bash
bash scripts/ci-gates.sh              # every gate
bash scripts/ci-gates.sh --list       # what they are, and which CI job each mirrors
bash scripts/ci-gates.sh --group fast # the ones that need no cargo build
```

- **A gate that ran nothing is a failure.** `cargo nextest run` that selected no tests exits 0; the
  script reports a gate that left no evidence in its log that it ran as failed. The exit status is
  the number of failed gates.
- **No drift between local and CI.** `workflow-parity` fails the build if the script and
  `.github/workflows/ci.yaml` ever spell a gate differently. Gates that need model artifacts no
  runner has are declared as manual in `--list`, not left as silent gaps.
- **The compiler is pinned.** `rust-toolchain.toml` is read by every workflow job, and
  `scripts/ci-toolchain-pin-check.py` fails the build if one installs a floating toolchain.
- **Post-quantum is enforced by the build, not by convention.** `scripts/pq-ci-guard.sh` fails CI if
  `kaspa-consensus` or `kaspad` links secp256k1, and runs `cargo-deny` advisories (`deny.toml`).
- **Consensus identity is printed, not assumed.** Every node prints its consensus params
  fingerprint and fence schedule at startup, and a peer on a different ruleset is refused at the
  handshake.
- **Audits are published with their fixes.** Findings and the commits that answer them live in the
  tree: for example the pre-arming audit
  [docs/palw-audit-2026-09-18-6001.md](docs/palw-audit-2026-09-18-6001.md) (two Critical and six
  High, all fixed), the DAA-clock audit
  [docs/palw-daa-clock-audit-2026-09-18.md](docs/palw-daa-clock-audit-2026-09-18.md), the release
  report [docs/palw-release-6001-verdict-2026-09-18.md](docs/palw-release-6001-verdict-2026-09-18.md),
  and the remediation response to the commissioned 2026-06-22 reviews
  [docs/security/MISAKA-Audit-Remediation-Response-2026-06-23.md](docs/security/MISAKA-Audit-Remediation-Response-2026-06-23.md).

What is still missing before mainnet — freeze, signed releases, fuzzing, recovery drills, soak,
independent audit — is tracked item by item in [docs/mainnet-readiness.md](docs/mainnet-readiness.md).

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

`--netsuffix=12` is required: bare `--testnet` still means `testnet-10`, which is stopped. The
startup log must show the fingerprint in [Release status](#release-status) and
`Consensus fence schedule: 1000 (schedule id 93da24cc…)`; the full walkthrough is
[docs/testnet12-join-mining.md](docs/testnet12-join-mining.md).

`=default` resolves to the network's standard loopback port, so you never have to memorize the
numbers. Add `--rpclisten-json=default` too if a JSON WebSocket client (e.g. a browser app or an
explorer backend) needs to connect locally.

- To **join the public testnet**, the node discovers peers via the misakas DNS seeders
  (`seeder1.misakascan.com` … `seeder4.misakascan.com`) automatically. **testnet-12's P2P port is
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
  `server 127.0.0.1:27210` → `connect`. (P2P is a separate, non-RPC port: `26311` on testnet-12.)
- **Headless balance (no interactive wallet).** For scripting / monitoring, query a balance in one
  shot over wRPC:
  `kaspa-pq-validator balance --node-rpc 127.0.0.1:27210 --address misakatest:q… [--address …] [--network testnet-12]`.
  It prints `address <sompi> <MSK> MSK` per line (plus `TOTAL` for several) to stdout — connection /
  sync notes go to stderr, so `… balance --address misakatest:q… | awk '{print $2}'` yields just the
  sompi. The node must run `--utxoindex`.
- Add `--enable-unsynced-mining` **only** when bootstrapping a brand-new isolated network with no peers (mining before you have synced to the public testnet would fork from genesis).

Testnet-12 cannot be mined with `kaspa-pq-miner` or `misaminer`: they do not create the required
PALW attempt envelope. Block production runs inside `kaspad --palw-produce`; use
`misaka mining setup/start` rather than an external hash miner.

## Running a validator (testnet)

> [!NOTE]
> **The example below still carries testnet-11's values.** On testnet-12 a DNS-finality validator
> bond is at least **20,000,000 MSK** ([join guide §4](docs/testnet12-join-mining.md)), and DNS
> finality stays in Bootstrap until validators are funded after launch
> ([launch note §2](docs/t12-launch-2026-09-25.md)). Name `--network testnet-12` and size the bond
> from the join guide.

The `kaspa-pq-validator` sidecar connects to a local node over wRPC and attests while its ML-DSA-87 stake bond is active. See [docs/validator-runbook.md](docs/validator-runbook.md). Quickstart:

```bash
# 1. generate a validator key + print its funding address
kaspa-pq-validator keygen --out val.seed --network testnet
# 2. send funds to the printed funding address (mine to it, or transfer from another wallet)
# 3. stake a DNS-finality bond. Testnet-11's minimum is 10 MSK = 1,000,000,000 sompi.
#    Omit --fee to auto-size it (mass-based; the flat floor is too low for the 2592-byte pubkey).
kaspa-pq-validator bond --node-rpc 127.0.0.1:27210 --validator-key val.seed \
  --amount 1000000000 --network testnet-11
# 4. run the validator daemon (attests every epoch while the bond is active, and precommits
#    where the network schedules ADR-0128's BFT gate — testnet-11 from DAA 7,101)
kaspa-pq-validator run --node-rpc 127.0.0.1:27210 --validator-key val.seed \
  --stake-bond <txid:index> --signed-epoch-db val.state --network testnet-11 --attest-poll-secs 3
```

> Note: the funding/`run`/`bond` subcommands want the **full** network id (`testnet-11`); `keygen`'s `--network` takes the short form (`testnet`). Use a **fresh** `--signed-epoch-db` per network — reusing one across networks trips the anti-equivocation guard on overlapping epoch numbers.

The validator attests the one current canonical-ready epoch per round; the poll cadence defaults to
**3 s**. Testnet-11 is not a 10-BPS network: PALW cadence is 120 seconds per block and its DNS
attestation epoch is rescaled to 2 blue-score. Keep the generated/default poll interval unless the
current validator runbook and `--help` say otherwise.

Once enough active stake has attested across the recent epochs, `getDnsConfirmation` reports
`dnsConfirmed: true` plus a `lastDnsConfirmedAnchor` (the stake-confirmed finality point — treat
this as DNS-final, not the pov-dependent `blockHash` sink). Below DAA 7,101, confirmation on
Testnet-11 is two-dimensional: it requires both anchor-relative `WorkDepth` and `StakeDepth` under the
live network parameters. **From DAA 7,101 it is a vote** (ADR-0128): the confirmed anchor is the newest
epoch anchor that validators holding more than two thirds of the counted bonded stake have attested
and precommitted to, voting power is the bond amount, a bond silent for 5,040 DAA (about seven days)
stops counting until it attests again, and the DNS stake reorg gate refuses chains that abandon the
confirmed anchor until it goes stale. The sidecar and the in-node validator precommit by themselves
from `getPrecommitDuty` and keep a second safety log, `<signed-epoch-db>.precommits.json` — back it up
with the seed. Validators are paid 20 % of each block's subsidy from DAA 7,101 (30 % before). PALW does
not wait for any of this: its payments settle on PALW anchors (`misaka palw settlement`). The current experimental mesh permits one active validator, but the 10 MSK minimum
does not bypass the work-depth, anchor-attester or freshness checks. Per-block finality is queryable:
`getDnsConfirmation` accepts an optional `blockHash` and answers whether that block is DNS-final
(`blockIsDnsFinal` / `blockIsConfirmedAnchor`); the explorer's **DNS Finality** page lists the
confirmed chain in order.

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
honoured by every workflow job, and it runs the three derived-artifact verifiers
(`misaka-palw-artifact-conformance.py`, `misaka-palw-derive-stranger.py`,
`misaka-palw-artifact-thirdparty.py`). The last one wants two foreign parsers, which are
deliberately NOT workspace dependencies:

```bash
python3 -m venv /tmp/artifact-venv
/tmp/artifact-venv/bin/pip install mido==1.3.3 numpy-stl==4.0.0
CI_GATES_THIRDPARTY_PYTHON=/tmp/artifact-venv/bin/python bash scripts/ci-gates.sh --group fast
```

The CI lints job also runs `scripts/pq-ci-guard.sh`, which hard-gates that neither `kaspa-consensus` nor `kaspad` link secp256k1.
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
