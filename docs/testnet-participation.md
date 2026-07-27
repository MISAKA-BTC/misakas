# Joining the MISAKA testnet

This is the entry point for **participating** in the network: which network to join, what you can
run, what hardware you need, and how to verify you are actually connected.

Everything below is derived from the shipped source in this repository. Where a capability is
**not** implemented, this document says so rather than leaving it implied.

---

## 1. Which network do I join?

**`testnet-10`** — it is the only network in this repository with public DNS seeders, so it is the
only one you can join by discovery. Explorer: **[misakascan.com](https://misakascan.com)**.

| Network | Select with | P2P port | Public seeders | PALW audited-compute lane |
|---|---|---|---|---|
| **`testnet-10`** | `--testnet` | `26211` | **yes** (7 records) | inert — algo-3 hash floor only |
| `testnet-110` | `--testnet --netsuffix=110` | `26411` | no | inert until a weight-0 re-genesis |
| `devnet-111` | `--devnet --netsuffix=111` | `26611` | no | **active**, single-node preset |
| `testnet-200` | `--testnet --netsuffix=200` | `26511` | no | **active**, Header-v4 staging rehearsal |
| `mainnet` | `--mainnet` | `26111` | — | **defined but NOT launched** |

`dns_seeders` is empty for `testnet-110`, `devnet-111` and `testnet-200`
([`consensus/core/src/config/params.rs`](../consensus/core/src/config/params.rs)) — those are local
and closed-mesh presets. You can run them, but only by pointing nodes at each other explicitly with
`--addpeer`; there is no public mesh to join.

**Do not run `--mainnet` expecting a live network.** The mainnet parameter set exists so the
consensus rules can be tested; it is not launched or endorsed for production.

---

## 2. Supported platforms and hardware

### The node and the miners are CPU-only

`kaspad`, `misaminer` and `kaspa-pq-miner` contain **no GPU code path of any kind**. Layer-0 mining
is BLAKE2b-512 on the CPU, parallelised with rayon. This is a deliberate property, not a gap to
work around: there is no GPU miner to install and no GPU tuning to do.

| Host | Node (`kaspad`) | CPU mining | Notes |
|---|---|---|---|
| macOS, Apple Silicon (arm64) | yes | yes | primary development platform |
| macOS, Intel (x86_64) | yes | yes | |
| Linux x86_64 | yes | yes | |
| Linux arm64 | yes | yes | Docker multi-arch images build for `linux/arm64` |
| Windows | yes | yes | build instructions in the README |

### GPU acceleration exists only for the optional PALW inference backend

The `misaka-palw` crate carries an **optional, off-by-default** real-inference backend
([`mil/palw/Cargo.toml`](../mil/palw/Cargo.toml)):

| Cargo feature | Device | Availability |
|---|---|---|
| `qwen-backend` | CPU | works everywhere; slow for large models |
| `qwen-metal` | Apple Silicon GPU (Metal) | macOS on Apple Silicon |
| `qwen-cuda` | NVIDIA GPU (CUDA) | Linux/Windows with a CUDA toolchain |

A default node build does **not** compile any of this — the heavy `candle` / `tokenizers` stack is
pulled in only by `--features qwen-backend`.

**Two things this does not mean:**

1. **It does not accelerate mining or block validation.** These features drive the PALW
   audited-compute provider path, not Layer-0 proof-of-work and not consensus verification.
2. **There is no public network where it does anything today.** PALW is inert on `testnet-10`, and
   the PALW-active presets (`devnet-111`, `testnet-200`) have no public seeders. Running a GPU
   provider is currently a local exercise, not participation in a live market.

---

## 3. Build

Prerequisites per OS (protobuf, clang/LLVM, the Rust toolchain) are in the
[README](../README.md#building-from-source). Then:

```bash
git clone https://github.com/MISAKA-BTC/MisakaLLM
cd MisakaLLM
cargo build --release -p kaspad -p misaminer -p kaspa-pq-miner \
                      -p kaspa-pq-validator -p kaspa-pq-signer \
                      -p misaka-cli --bin misaka
```

Binaries land in `target/release/`:

| Binary | Package | Role |
|---|---|---|
| `kaspad` | `kaspad` | the full node |
| `misaminer` | `misaminer` | CPU miner (wallet-oriented front end) |
| `kaspa-pq-miner` | `pq-miner` | CPU miner (low-level grinder) |
| `kaspa-pq-validator` | `kaspa-pq-validator` | DNS-finality validator sidecar |
| `kaspa-pq-signer` | `kaspa-pq-signer` | optional out-of-process ML-DSA-87 signer |
| `misaka` | `misaka-cli` | unified operator CLI |

Note the package/binary split for the CLI: the package is `misaka-cli`, the binary is `misaka`, so
name both (`-p misaka-cli --bin misaka`) rather than relying on workspace defaults.

The optional GPU inference backend is a separate build and is not needed to run a node:

```bash
# Apple Silicon
cargo build --release -p misaka-palw --features qwen-metal
# NVIDIA
cargo build --release -p misaka-palw --features qwen-cuda
```

---

## 4. Run a node

The newcomer path is `misaka join`, a front end over `node start` that selects the network and
names the DNS seeds for you (`--network` defaults to `testnet-10`; extra `kaspad` arguments go
after `--`):

```bash
./target/release/misaka join -- --utxoindex --rpclisten-borsh=default
```

Or drive `kaspad` directly:

```bash
./target/release/kaspad --testnet --utxoindex --rpclisten-borsh=default
```

- Peers are discovered through the public DNS seeders automatically. **Outbound TCP `26211` must
  not be blocked.** If discovery is slow, bootstrap explicitly:
  `--addpeer=95.111.236.186:26211`.
- `--utxoindex` is required for wallet and validator funding lookups.
- **gRPC is on by default** at loopback `127.0.0.1:26210`, so the miner needs no extra flag.
  **wRPC is off by default** and must be enabled — the CLI wallet and `kaspa-pq-validator` speak
  wRPC Borsh (`27210`), *not* gRPC.
- Do **not** pass `--enable-unsynced-mining` when joining the public testnet. It exists for
  bootstrapping a brand-new isolated network; using it here mines a fork from genesis.

`testnet-10` default ports:

| Purpose | Port | Default state |
|---|---|---|
| P2P | `26211` | on |
| gRPC | `26210` | on (loopback) |
| wRPC Borsh | `27210` | off — `--rpclisten-borsh=default` |
| wRPC JSON | `28210` | off — `--rpclisten-json=default` |
| EVM HTTP RPC | `8545` | requires an `--features kaspad/evm` build |

### Verify you actually joined

Do not infer sync from the absence of errors — a node that is connected but not syncing looks
identical to a healthy one in the log tail. There is a one-shot health check that covers ports,
sync state, versions and the RPC surface:

```bash
./target/release/misaka node doctor
```

Two more that answer specific questions:

```bash
./target/release/misaka node endpoints    # which RPC endpoints miner/validator will auto-connect to
./target/release/misaka bootstrap --help  # the DNS seeds and the peers they actually resolve to
```

Then compare your tip against [misakascan.com](https://misakascan.com).

`misaka setup` additionally provides a guided VPS path (preflight, node service, status).

---

## 5. Mine (CPU)

Mine to a **64-byte ML-DSA-87** address (`misakatest:` prefix). Legacy 32-byte addresses are
rejected at consensus — there is no compatibility path.

```bash
./target/release/kaspa-pq-miner \
  --node-grpc 127.0.0.1:26210 --network-id testnet-10 \
  --blocks 0 --min-block-interval-ms 250 --pay-address <misakatest:...>
```

The network runs at **10 BPS**. Coinbase output arrives as small fragments rather than one large
UTXO; the `bond` subcommand below aggregates them for you, so manual consolidation is not needed.

---

## 6. Run a validator

The validator attests to DNS finality while its ML-DSA-87 stake bond is active. Full procedure and
failure modes: [validator-runbook.md](validator-runbook.md).

```bash
# 1. generate a key and print its funding address
kaspa-pq-validator keygen --out val.seed --network testnet

# 2. fund that address (mine to it, or transfer in)

# 3. bond. testnet minimum is 10 MSK = 1_000_000_000 sompi.
kaspa-pq-validator bond --node-rpc 127.0.0.1:27210 --validator-key val.seed \
  --amount 1000000000 --network testnet-10

# 4. attest
kaspa-pq-validator run --node-rpc 127.0.0.1:27210 --validator-key val.seed \
  --stake-bond <txid:index> --signed-epoch-db val.state \
  --network testnet-10 --attest-poll-secs 3
```

Three details that cost people time:

- **`keygen` takes the short network name (`testnet`); `bond` and `run` take the full id
  (`testnet-10`).** They are not interchangeable.
- **Use a fresh `--signed-epoch-db` per network.** The anti-equivocation guard keys on epoch
  numbers, and epoch numbers overlap across networks.
- **Testnet lowers the staking floors to 10 MSK** (`min_bond_amount_sompi` /
  `min_active_stake_sompi` in `TESTNET_DNS_PARAMS`) so a tester can mine a bondable amount in
  seconds. **Mainnet keeps 20,000,000 MSK.** Testnet also pins `min_active_validators = 1` for the
  single-operator mesh.

Confirmation is **two-dimensional** — it needs both accumulated blue work (`WorkDepth`) and
attested stake (`StakeDepth`). Testnet deliberately pins `required_stake_depth = StakeScore(5000)`,
which is ~5e-6 of one fully-participated epoch's accrual, so even a small validator clears the
**stake** dimension within its *first* attested epoch; the work dimension
(`required_work_depth = 100`) is the gate that remains. This is a fast-confirmation testnet
setting, **not** a mirror of production's ten-epoch burial — do not calibrate expectations for
mainnet from it.

`getDnsConfirmation` reports `dnsConfirmed` plus `lastDnsConfirmedAnchor`; treat the **anchor** as
the finality point, not the point-of-view dependent `blockHash`.

---

## 7. Testnet points (MTP)

The CLI ships a testnet-only points surface. Points are a mirror of **ML-DSA-87-signed epoch
ledgers**, so they are independently checkable rather than something you have to trust:

```bash
misaka mtp points gh:<your-id>          # your points
misaka mtp leaderboard --top 50         # full ranking
misaka mtp verify-epoch points/epoch-12.0.jsonl --pubkey-file <operator.pub>
```

`verify-epoch` checks the signature and the rules hash locally, and with `--facts` performs a full
deterministic recompute and byte-compare.

The service URL is **not baked into this repository** — the CLI defaults to
`http://127.0.0.1:8790`. Point it at the operator's service with `--endpoint` or the
`MISAKA_MTP_ENDPOINT` environment variable.

---

## 8. What you cannot do yet

Stated explicitly so nobody builds on an assumption:

- **Run a PALW compute provider on a public network.** PALW is inert on `testnet-10`, and every
  PALW-active preset ships with no seeders.
- **Join mainnet.** Not launched. The genesis constants are a governance decision that has not been
  made.
- **Use a GPU backend other than Metal or CUDA.** Those two are the only devices the optional
  inference backend exposes; every other host runs the CPU path.
- **Rely on PQ transport security.** ML-DSA-87 covers *transaction authorization* and the 64-byte
  BLAKE2b-512 consensus identity. Peer-to-peer transport is not post-quantum unless an ML-KEM
  hybrid is enabled.

---

## 9. Tokenomics, as implemented

From [`consensus/core/src/constants.rs`](../consensus/core/src/constants.rs) and
[`consensus/src/processes/coinbase.rs`](../consensus/src/processes/coinbase.rs):

- Theoretical max supply **~26,013,224,875 MSK** (`MAX_SOMPI`), composed of a **10B genesis
  premine** (one main UTXO per network) plus **~16,013,224,875 MSK** of issuance.
- Issuance runs **30 years** (360 monthly entries plus a terminal zero in
  `SUBSIDY_BY_MONTH_TABLE`), decaying **1.4 %/yr** (q = 0.986), ending at ~1.36848463 MSK/block in
  year 30 at 10 BPS.
- `MAX_SOMPI` is a **per-amount sanity cap, not a hard emission cap.** Each block reward is rounded
  up to an integer sompi, so live cumulative issuance ends a few tens of MSK above the theoretical
  figure (≈ +44 MSK at 10 BPS). Emission stops via the terminal-zero table entry, not via this
  constant.

---

## 10. Where to go next

| Topic | Document |
|---|---|
| Validator procedure and failure modes | [validator-runbook.md](validator-runbook.md) |
| ML-DSA-87 design | [kaspa-pq-design-mldsa87.md](kaspa-pq-design-mldsa87.md) |
| Consensus spec | [kaspa-pq-spec.md](kaspa-pq-spec.md) |
| Verification runbook | [kaspa-pq-mldsa87-verification-runbook.md](kaspa-pq-mldsa87-verification-runbook.md) |
| Governing migration record | [adr/0019-mldsa87-migration.md](adr/0019-mldsa87-migration.md) |
| PALW audited-compute lane | [adr/0039 onward](adr/) |
| Reporting a vulnerability | [../SECURITY.md](../SECURITY.md) |
