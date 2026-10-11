# Benchmarks and tests

> 旧ルート README（2026-10-12 まで）から移した手順です。例に testnet-11 や古い値が含まれる場合があります。現行ネットワーク（testnet-12）の値は [README](../../README.md) と [docs/README](../README.md) を正とし、`--help` と chain RPC で確認してください。

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

Experiment with non-standard consensus parameters in non-mainnet environments via `--override-params-file <path>`. See [docs/override-params.md](../override-params.md).
</details>
