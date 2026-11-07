# Build MISAKA

> **整理前の保存版。** 当時のcurrent / live / PASS・数値・手順を現在の保証にしません。新しい入口は [README](../../../../README.md)。元バイトはoriginal-markdown.zipに保持しています。

> 旧ルート README（2026-10-12 まで）から移した手順です。例に testnet-11 や古い値が含まれる場合があります。現行ネットワーク（testnet-12）の値は [README](../../README.md) と [docs/README](../../../../README.md) を正とし、`--help` と chain RPC で確認してください。

## Prebuilt binaries

**Build from source for the live chain today.** The newest `testnet-main-*` release carries the
current ruleset in its notes but attaches no binaries, and the last release that *does* attach them
— `testnet-11-relaunch-5f-adr0083-h1150` — is on the far side of the fork-id gate and is refused.
Downloading it is the single most common way to end up with a node that peers, drops after minutes,
and mines blocks nobody accepts.

The check is never the tag: it is the fingerprint your node prints on its second startup line and
the fence schedule on the line after it. If either does not match the Status block above, you are
on the wrong ruleset whatever you downloaded. Older releases on that page are earlier chains and are refused at the handshake.

Linux x86_64 binaries (`kaspad`, `kaspa-pq-miner`, `kaspa-pq-validator`, `kaspa-pq-signer`, `misaka`) are published under [Releases](https://github.com/MISAKA-BTC/misakas/releases). Each release is built from the source snapshot of the same tag; verify with the `SHA256SUMS` attached to the release.

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
