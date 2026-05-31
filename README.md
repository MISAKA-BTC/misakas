<h1>misakas — post-quantum (PQ-only) Kaspa</h1>

**misakas** is a post-quantum, **PQ-only** fork of [rusty-kaspa](https://github.com/kaspanet/rusty-kaspa). It replaces Kaspa's secp256k1/Schnorr transaction authorization with **ML-DSA-87** (FIPS 204, NIST category 5) and makes every non-PQ path — legacy secp256k1/Schnorr/ECDSA signatures, legacy addresses, and P2SH — **unrepresentable at the consensus, mempool, and wallet layers**. It is a new, independent network with its own genesis; it is **not** compatible with Kaspa or with any prior kaspa-pq chain state, UTXO set, or address.

The node binary is still named `kaspad` and the crates keep their upstream `kaspa-*` names (this is a fork, not a rename); the **network**, addresses (`misaka…` / `misakadev…`), and project branding are misakas.

> Status: **devnet** (experimental). The DNS-finality reward overlay is **active on devnet** (`dns_activation_daa_score = 0`) so a real bond → attestation → reward-bearing coinbase can be exercised. No mainnet.

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
| Genesis / premine | new genesis; 15B premine locked to a single-key ML-DSA-87 P2PKH |

Authoritative design & spec live under [`docs/`](docs/):

- [ADR-0019 — ML-DSA-87 migration](docs/adr/0019-mldsa87-migration.md) (rev 1.2 is the current governing record)
- [Design doc — `docs/kaspa-pq-design-mldsa87.md`](docs/kaspa-pq-design-mldsa87.md)
- [Spec — `docs/kaspa-pq-spec.md`](docs/kaspa-pq-spec.md)
- [Verification runbook — `docs/kaspa-pq-mldsa87-verification-runbook.md`](docs/kaspa-pq-mldsa87-verification-runbook.md)
- [Validator runbook — `docs/validator-runbook.md`](docs/validator-runbook.md)

**Scope of PQ claims** (per the design doc): "tx authorization uses ML-DSA-87", "secp256k1 signing disabled in PQ consensus mode", "64-byte BLAKE2b-512 consensus identity". Transport-layer (network) traffic is **not** PQ unless an ML-KEM hybrid is enabled.

## Prebuilt binaries

Linux x86_64 devnet binaries (`kaspad`, `kaspa-pq-miner`, `kaspa-pq-validator`) are published under [Releases](https://github.com/MISAKA-BTC/misakas/releases). Each release is built from the source snapshot of the same tag.

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
      cargo build --release -p kaspad -p kaspa-pq-miner -p kaspa-pq-validator
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

## Running a devnet node

Start a misakas devnet node (the overlay + PQ rules are active from genesis on devnet):

```bash
cargo run --release --bin kaspad -- --devnet --enable-unsynced-mining --utxoindex \
  --rpclisten=127.0.0.1:26610 --rpclisten-borsh=127.0.0.1:27610 --rpclisten-json=127.0.0.1:28610
```

- `--enable-unsynced-mining` is required on first launch (no peers yet).
- `--utxoindex` is required for wallet/validator funding lookups.
- `--rpclisten-borsh` is required by the miner and the `kaspa-pq-validator` sidecar.

Mine to a **64-byte** ML-DSA-87 (`misakadev:`) address — legacy 32-byte addresses are rejected:

```bash
cargo run --release --bin kaspa-pq-miner -- --rpc 127.0.0.1:26610 --network-id devnet \
  --blocks 0 --min-block-interval-ms 1000 --pay-address <misakadev:...>
```

## Running a validator (devnet)

The `kaspa-pq-validator` sidecar connects to a local node over wRPC and attests while its ML-DSA-87 stake bond is active. See [docs/validator-runbook.md](docs/validator-runbook.md). Quickstart:

```bash
# 1. generate a validator key + print its funding address
kaspa-pq-validator keygen --out val.seed --network devnet
# 2. send funds to the printed funding address (mine to it, or send from the premine)
# 3. stake a bond (active immediately with activation-daa-score 0)
kaspa-pq-validator bond --node-rpc 127.0.0.1:27610 --validator-key val.seed \
  --amount 10000000 --activation-daa-score 0 --network devnet
# 4. run the validator daemon (attests every epoch while the bond is active)
kaspa-pq-validator run --node-rpc 127.0.0.1:27610 --validator-key val.seed \
  --stake-bond <txid:index> --signed-epoch-db val.state --network devnet
```

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
