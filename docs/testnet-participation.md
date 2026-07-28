# Joining the MISAKA testnet

This is the entry point for **participating** in the network: which network to join, what you can
run, what hardware you need, and how to verify you are actually connected.

Everything below is derived from the shipped source in this repository. Where a capability is
**not** implemented, this document says so rather than leaving it implied.

---

## 1. Which network do I join?

**Two networks are open to you**, and they are for different things:

- **`testnet-10`** — the general testnet. Public DNS seeders, so you join by discovery. PALW is
  inert here (algo-3 hash floor only). This is the network the MTP points programme scores.
- **`testnet-200`** — the ADR-0048 staging-mainnet PALW rehearsal, where the audited-compute lane is
  genesis-active. Reachable since ADR-0042 改訂 A1, but it ships **no seeders**, so you join by
  naming a peer. Not scored by MTP. Explorer: **[misakascan.com](https://misakascan.com)**.

| Network | Select with | P2P port | How you find peers | PALW audited-compute lane |
|---|---|---|---|---|
| **`testnet-10`** | `--testnet` | `26211` | **DNS seeders** (7 records) | inert — algo-3 hash floor only |
| **`testnet-200`** | `--testnet --netsuffix=200` | `26511` | **`--addpeer` only** (no seeders) | **active** from genesis |
| `testnet-110` | `--testnet --netsuffix=110` | `26411` | closed — allowlist gate | inert until a weight-0 re-genesis |
| `devnet-111` | `--devnet --netsuffix=111` | `26611` | closed — allowlist gate | **active**, single-node preset |
| `mainnet` | `--mainnet` | `26111` | — | **defined but NOT launched** |

`dns_seeders` is empty for every preset except `testnet-10`
([`consensus/core/src/config/params.rs`](../consensus/core/src/config/params.rs)). For
`testnet-200` that means *undiscoverable*, not *unreachable* — A1 removed its allowlist gate, so an
explicit `--addpeer` is enough. `testnet-110` and `devnet-111` keep both restrictions and remain
closed-mesh presets you run yourself.

**Explorer, currently:** misakascan.com serves `testnet-200`. `testnet-10` has no explorer at the
moment.

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
2. **No network is paying for it yet.** PALW is inert on `testnet-10`. `testnet-200` has it
   genesis-active and is now reachable, but its audited-compute lane is halted until three bonded
   validators bring the DNS overlay up (see §8), so no algo-4 block has been accepted there. Running
   a GPU provider is still an exercise rather than participation in a live market — the difference
   now is that the blocker is the validator set, not reachability.

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
  `--addpeer=160.16.131.119:26211`. (`95.111.236.186` was the documented bootstrap peer until that
  host was repurposed to `testnet-200`; it no longer serves testnet-10.)

To run `testnet-200` instead — no seeders, so the peer is not optional:

```bash
./target/release/kaspad --testnet --netsuffix=200 \
  --addpeer=95.111.236.186:26511 --utxoindex --rpclisten-borsh=default
```

Build from `a2437e1` or later. A1's allowlist flip changes `consensus_identity_hash`, so an older
binary is not stale — it is running different consensus rules.
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

On `testnet-200` you can compare your tip against [misakascan.com](https://misakascan.com), which
serves that network. `testnet-10` has no explorer at present, so there `misaka node doctor` and your
peer count are the check.

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

Points are a mirror of **ML-DSA-87-signed epoch ledgers**, so they are independently checkable
rather than something you have to trust. The programme scores **`testnet-10` only**.

### The public endpoint

```
MISAKA_MTP_ENDPOINT = https://misakascan.com
```

| Route | What it returns |
|---|---|
| `https://misakascan.com/mtp/v1/points` | the full leaderboard |
| `https://misakascan.com/mtp/v1/points/<id>` | one identity, e.g. `gh:alice` |
| `https://misakascan.com/mtp/v1/operator` | the operator's 2592-byte ML-DSA-87 public key + its pins |

The operator key is pinned out-of-band so you can check the endpoint is not lying about who signs:

```
misakatest:qtu8yq0psff2leaz35rqrh5kcz20kug5jce2ecca9hx7ed6cxpghrzhnjg650ugu7esa8snj2ltz4v0dkdzu0dn7s90xmakw0fneety0pvngw4r0
```

`/mtp/v1/operator` must surface that same string in its `pins`. If it does not, do not trust the
ledgers it serves.

### Querying it

**The bundled CLI cannot reach the HTTPS endpoint.** Its MTP client is a hand-rolled HTTP/1.1 GET
with no TLS, so `misaka mtp points` / `leaderboard` only work against an `http://` instance — a
local service, or a tunnel to one. Against `https://` it refuses and says so.

That is a client limitation, not a hole in the trust model: **verification is offline**, and it is
the part that proves anything. Fetch with any HTTPS client, verify locally:

```bash
curl -s https://misakascan.com/mtp/v1/points                       # leaderboard
curl -s https://misakascan.com/mtp/v1/points/gh:alice              # one identity
curl -s https://misakascan.com/mtp/v1/operator \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["operator_pubkey_mldsa87_hex"])' > operator.pub

misaka mtp verify-epoch epoch-7.0.jsonl --pubkey-file operator.pub
misaka mtp verify-epoch epoch-7.0.jsonl --pubkey-file operator.pub --facts epoch-7.0.input.json
```

`verify-epoch` checks the ML-DSA-87 signature and the rules hash; with `--facts` it additionally
re-runs the scoring deterministically and byte-compares the result. A ledger that passes both did
not come from trusting the server.

### Current state

The service is live and boot-persistent (`misaka-mtp.service` on the explorer host), but **no epoch
has been published yet**, so the leaderboard is empty and `network` reads as `""`. That is a fresh
install, not a fault. Points start accruing when the operator runs `run-epoch` over a window;
auto-collected node/validator facts merge with any hand-reviewed `bug` / `verify` / `infra` awards.

`testnet-200` earns nothing — it is out of scope by design (see §8).

---

## 8. What you cannot do yet

Stated explicitly so nobody builds on an assumption:

- **Earn anything as a PALW compute provider.** `testnet-200` is reachable and PALW is genesis-active
  there, so this is no longer a reachability problem — but **no algo-4 block has been accepted yet**.
  The lane is halted: the beacon needs the DNS overlay healthy, `PRODUCTION_DNS_PARAMS` requires
  `min_active_validators = 3` each bonded at 20,000,000 MSK, and only one validator is bonded today.
  Until that clears, `testnet-200` produces algo-3 blocks only — measured at ~2.6 BPS against the
  2 + 8 design (hash lane on target, PALW lane contributing nothing).
- **Earn MTP points on `testnet-200`.** The points programme scopes `testnet-10` only.

  Issuing and verifying a Qwen3.6 `ComputeReceipt` locally *does* work today — that is a separate,
  reproducible exercise. See [palw-llm-receipts.md](palw-llm-receipts.md), which also spells out what
  a receipt does and does not prove.
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
| **Qwen3.6 receipts — issue, verify, what they prove** | [palw-llm-receipts.md](palw-llm-receipts.md) |
| **The LLM runtime itself** | [misaka-proof-of-llm](https://github.com/MISAKA-BTC/misaka-proof-of-llm) |
| Reporting a vulnerability | [../SECURITY.md](../SECURITY.md) |
