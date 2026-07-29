# Joining the MISAKA testnet

This is the entry point for **participating** in the network: which network to join, what you can
run, what hardware you need, and how to verify you are actually connected.

Everything below is derived from the shipped source in this repository. Where a capability is
**not** implemented, this document says so rather than leaving it implied.

---

## 1. Which network do I join?

**Join `testnet-200`.** It is the public testnet, the network shown by
[misakascan.com](https://misakascan.com), and the network scored by MTP.
`testnet-10` is retired from public operation and remains only as a compatibility preset.

| Network | Select with | P2P port | How you find peers | PALW audited-compute lane |
|---|---|---|---|---|
| **`testnet-200`** | `--testnet --netsuffix=200` | `26511` | **DNS seeders** (2 authoritative names) | **active; algo-4 accepted** |
| `testnet-10` | `--testnet` | `26211` | retired — explicit legacy peer only | inert |
| `testnet-110` | `--testnet --netsuffix=110` | `26411` | closed — allowlist gate | active, closed preset |
| `devnet-111` | `--devnet --netsuffix=111` | `26611` | closed — allowlist gate | **active**, single-node preset |
| `mainnet` | `--mainnet` | `26111` | — | **defined but NOT launched** |

The MISAKA seed domains belong exclusively to testnet-200
([`consensus/core/src/config/params.rs`](../consensus/core/src/config/params.rs)).
`testnet-110` and `devnet-111` remain closed-mesh presets you run yourself.

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
2. **A GPU is not required to join.** PALW and algo-4 acceptance are active on testnet-200, and
   receipt-v3 provider rewards have been exercised end-to-end. A continuous GPU/LLM provider is an
   optional operator role; ordinary nodes validate commitments and signatures without rerunning
   model inference. §7 covers the provider path and its gates.

---

## 3. Build

Prerequisites per OS (protobuf, clang/LLVM, the Rust toolchain) are in the
[README](../README.md#building-from-source). Then:

```bash
git clone https://github.com/MISAKA-BTC/misakas
cd misakas
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
names the DNS seeds for you (`--network` defaults to `testnet-200`; extra `kaspad` arguments go
after `--`):

```bash
./target/release/misaka join -- --utxoindex --rpclisten-borsh=default
```

Or drive `kaspad` directly:

```bash
./target/release/kaspad --testnet --netsuffix=200 --utxoindex --rpclisten-borsh=default
```

- Peers are discovered through the public DNS seeders automatically. **Outbound TCP `26511` must
  not be blocked.** If discovery is slow, bootstrap explicitly:
  `--addpeer=95.111.236.186:26511`.
- `--utxoindex` is required for wallet and validator funding lookups.
- **gRPC is on by default** at loopback `127.0.0.1:26210`, so the miner needs no extra flag.
  **wRPC is off by default** and must be enabled — the CLI wallet and `kaspa-pq-validator` speak
  wRPC Borsh (`27210`), *not* gRPC.
- Do **not** pass `--enable-unsynced-mining` when joining the public testnet. It exists for
  bootstrapping a brand-new isolated network; using it here mines a fork from genesis.

`testnet-200` default ports:

| Purpose | Port | Default state |
|---|---|---|
| P2P | `26511` | on |
| gRPC | `26210` | on (loopback) |
| wRPC Borsh | `27210` | off — `--rpclisten-borsh=default` |
| wRPC JSON | `28210` | off — `--rpclisten-json=default` |
| EVM HTTP RPC | `8545` | EVM lane is not active on testnet-200 |

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

Compare your tip against [misakascan.com](https://misakascan.com), which serves testnet-200.

`misaka setup` additionally provides a guided VPS path (preflight, node service, status).

---

## 5. Mine (CPU)

Mine to a **64-byte ML-DSA-87** address (`misakatest:` prefix). Legacy 32-byte addresses are
rejected at consensus — there is no compatibility path.

```bash
./target/release/kaspa-pq-miner \
  --node-grpc 127.0.0.1:26210 --network-id testnet-200 \
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

# 3. bond. testnet-200 keeps the production minimum: 20,000,000 MSK = 2e15 sompi.
kaspa-pq-validator bond --node-rpc 127.0.0.1:27210 --validator-key val.seed \
  --amount 2000000000000000 --network testnet-200

# 4. attest
kaspa-pq-validator run --node-rpc 127.0.0.1:27210 --validator-key val.seed \
  --stake-bond <txid:index> --signed-epoch-db val.testnet-200.state \
  --network testnet-200 --attest-poll-secs 3
```

Three details that cost people time:

- **`keygen` takes the short network name (`testnet`); `bond` and `run` take the full id
  (`testnet-200`).** They are not interchangeable.
- **Use a fresh `--signed-epoch-db` per network.** The anti-equivocation guard keys on epoch
  numbers, and epoch numbers overlap across networks.
- **testnet-200 keeps the production staking economics**: three active validators and
  20,000,000 MSK per bond. The public mesh is already provisioned; a new validator needs an
  operator-funded bond.

Confirmation is **two-dimensional** — it needs both accumulated blue work (`WorkDepth`) and
attested stake (`StakeDepth`). testnet-200 uses reachable rehearsal thresholds:
`required_work_depth = 100` and `required_stake_depth = StakeScore(5000)`, while retaining the
production validator-count and bond-amount floors.

`getDnsConfirmation` reports `dnsConfirmed` plus `lastDnsConfirmedAnchor`; treat the **anchor** as
the finality point, not the point-of-view dependent `blockHash`.

---

## 7. The LLM side (PALW audited compute)

PALW's algo-4 lane is the audited-compute half of the system: a provider runs a **pinned** language
model, issues a `ComputeReceipt`, and a **separate** process verifies it. None of this is needed to
run a node or mine.

Read [palw-llm-receipts.md](palw-llm-receipts.md) first for the full procedure and, more
importantly, for what a receipt does and does not prove — it is a self-attestation checkable against
pinned artifacts, **not** a TEE attestation, not a zero-knowledge proof, and not evidence about the
model's output.

### What runs today

The receipt loop and the chain-consumption path run end to end. A Phase-1 runtime can export two
independent receipt-v3 results; this repository's `palw-real-provider` helper verifies their
ML-DSA-87 signatures, checks byte-identical inference results, reconstructs the canonical
Object-v2 DA payload, and derives the inference-bound ticket consumed by the miner.

```bash
cargo run --release -p palw-real-provider -- \
  --receipt-a /abs/receipt-a.json --receipt-b /abs/receipt-b.json \
  --ticket-authority-seed /abs/ticket-authority.seed --out-dir /abs/provider-out
```

The completed public proof bundle is
[`artifacts/testnet-200-real-qwen-20260729`](../artifacts/testnet-200-real-qwen-20260729/README.md).
It records the source receipts, canonical DA bytes, accepted algo-4 block, settlement block, and
provider payout verification from all three validators.

### Registering as a provider — and the bond that is not the one you think

A **PALW provider bond** and a **DNS-finality stake bond** are different objects with different
floors, and conflating them is the easiest mistake on this page:

| | PALW provider bond | DNS-finality stake bond |
|---|---|---|
| Minimum | **10 MSK** (`min_provider_bond_sompi`, [`consensus/core/src/palw.rs`](../consensus/core/src/palw.rs)) | **20,000,000 MSK** on `testnet-200` |
| Exit delay | 6 epochs (`provider_unbond_floor_epochs`) | 14 days + reorg horizon |
| Registered by | `kaspa-pq-validator palw-payload provider-bond` → `palw-submit` | `kaspa-pq-validator bond` (§6) |

```bash
kaspa-pq-validator palw-payload provider-bond \
  --network testnet-200 --validator-key /abs/provider.seed \
  --operator-group-id <hash64> --runtime-class <hash64> \
  --capacity "<shape-id>=1" --reward-key-root <hash64> \
  --amount 10MSK --unbond-delay-epochs 6 --out /abs/provider-bond.borsh

kaspa-pq-validator palw-submit --network testnet-200 \
  --node-wrpc-borsh 127.0.0.1:27220 --validator-key /abs/provider.seed \
  --kind provider-bond --payload-file /abs/provider-bond.borsh
```

`--network` here accepts only `testnet-110`, `devnet-111` and `testnet-200` — the three presets
where PALW is genesis-active. There is no provider role on retired testnet-10. testnet-200 is
publicly discoverable through the DNS seeds.

### Activation state

PALW has **three independent levers**
([`consensus/core/src/config/params.rs`](../consensus/core/src/config/params.rs)):

| Lever | Knob | State on testnet-200 |
|---|---|---|
| land | the code being shipped at all | released |
| accept | `palw_algo4_accept` | **`true`** |
| weight | `palw_compute_work_scale > 0` | `0` (accepted/measured, no fork-choice bonus) |

The DNS-finality gate uses the production three-validator and 20,000,000-MSK bond floors, with
testnet-reachable `WorkDepth = 100` and `StakeDepth = 5000`. The rolling lifecycle keeps a successor
batch in flight so the active window does not expire between inference jobs.

### Check it yourself

The acceptance lever is a compile-time constant, while accepted blocks and payouts are
chain-derived. The proof bundle pins a concrete example:

```bash
kaspa-pq-validator get-block \
  --network testnet-200 --node-wrpc-borsh 127.0.0.1:27210 \
  --hash c7ffe7678dce891dd4a5679985033c8d74e0587336c5f1dbddb8e98afd621bc8b49553a2284f00c394b8a6fb081594f30c2fea9c535c2f7fdf329f35584c2e70
```

That block has `pow_algo = 4`; its later settlement pays both receipt providers. The proof bundle
contains the complete commands and expected hashes so the result can be checked against any synced
testnet-200 node.

---

## 8. Testnet points (MTP)

Points are a mirror of **ML-DSA-87-signed epoch ledgers**, so they are independently checkable
rather than something you have to trust. New epochs score **`testnet-200` only**; the retired
testnet-10 ledger history remains verifiable but does not accrue new points.

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

### What earns points

| | Category | Earned by | Works today? |
|---|---|---|---|
| C1 | node | node uptime over the window, with a geo-diversity bonus (×1.5), a version-currency bonus (×1.2), and rank halving so a second node of the same operator counts half and a fourth counts zero | ✅ |
| C1 | validator | attestation participation over the window; a slashed week forfeits the whole week | ✅ |
| C2 | bug | a reported bug, priced by severity; a duplicate report is 10% of a first report | ✅ |
| C3 | verify | independent verification work | ✅ |
| C4 | infra | infrastructure contribution | ✅ |

Both C1 rows were first run end-to-end against the former testnet-10 before migration: a real
peer observed from the JP vantage scored **100 points**, and a real attesting validator indexed out
of blocks scored **200 points** over 30 epochs. Both ledgers passed `verify-epoch --facts`, which
re-runs the scoring and byte-compares it against the signature — so these are reproduced results,
not claims about the code.

C2/C3/C4 are awarded by hand after review, and need no registration because the operator asserts
the attribution directly:

```bash
misaka mtp award --epoch <N> --network testnet-200 --id gh:<you> --category bug    --severity S1
misaka mtp award --epoch <N> --network testnet-200 --id gh:<you> --category verify --points 250
misaka mtp award --epoch <N> --network testnet-200 --id gh:<you> --category infra  --points 500
```

### How to start earning

> **This is testnet-only.** MTP points are a testnet participation record. They are **not** a token,
> not a balance, not tradable, and carry **no monetary value**. `testnet-200` MSK is likewise
> valueless test currency. There is no mainnet, no sale, and no promise that points convert into
> anything — §11-B defines a *claim* mechanism for a possible future TGE, and a mechanism existing
> is not a commitment that it pays. Anyone offering to buy or sell MISAKA points or testnet MSK is
> running a scam.

**1 — make a key.** This key *is* your identity. Back it up; it cannot be recovered.

```bash
misaka key gen --network testnet-200 --out mtp.seed        # prints your misakatest: address
```

**2 — ask for an invitation.** Open an issue on this repository with your GitHub handle and the
address from step 1. You get back an invitation JSON: a one-shot nonce bound to that pair.

**3 — sign it offline.**

```bash
misaka mtp register --network testnet-200 \
  --invitation invitation.json --key-file mtp.seed --out registration.json
```

Nothing is transmitted. The MTP HTTP surface is **read-only by design** (ADR-0038 D3), so there is
no registration endpoint to post to — and therefore none that could accept a forged registration.

**4 — submit `registration.json`** through the same issue or a pull request. From the next epoch
run, facts about you resolve to `gh:<your-handle>`. Nothing before registration is retroactive.

**5 — run something worth scoring.**

- **A node** → C1 node. Keep it up and in sync on `testnet-200`. A peer still in IBD is reachable
  but not usable and does not count. You run no collector: the operator's vantage hosts observe you
  as an ordinary peer.
- **A validator** → C1 validator. Bond, attest, stay unslashed. Attestations are read out of
  blocks, so participation is chain-derived and needs nothing from you beyond attesting.
- **A bug report, a verification, infrastructure** → C2/C3/C4 above.

Check yourself at any time — no account, no login:

```bash
curl -s https://misakascan.com/mtp/v1/points/gh:<your-handle>
```

### How collection works, so you can audit it

A points programme nobody can check is worth nothing, so the operator side is four commands:

```bash
# C1 node — observe peers from a vantage, then attribute them via an explicit roster
misaka mtp collect --network testnet-200 --vantage jp --rpc 127.0.0.1:27210 --out probes.jsonl
misaka-mtp-service ingest-probes --data-dir DIR --file probes.jsonl --roster roster.jsonl

# C1 validator — attestations out of blocks, bond/slash state out of the registry
misaka mtp attestations --network testnet-200 --rpc 127.0.0.1:27210 --out att.jsonl
misaka mtp validators   --network testnet-200 --rpc 127.0.0.1:27210 --out bonds.jsonl
misaka-mtp-service ingest-attestations --data-dir DIR --file att.jsonl \
  --roster vroster.jsonl --bonds bonds.jsonl

# publish the signed ledger for the window
misaka-mtp-service run-epoch --data-dir DIR --operator-key op.seed \
  --epoch 1 --start 2026-07-28T00:00:00Z --end 2026-11-01T00:00:00Z
```

On a deployed host the fact store belongs to the service user, so every command that writes to
`--data-dir` must run as that user — `sudo -u misaka-mtp misaka-mtp-service …`. Running one as
`root` succeeds (root ignores the mode) but leaves a `root`-owned file behind, and the service can
no longer append to its own store. The failure is silent and permanent until the file is chowned
back.

Four properties of that pipeline, each guarding a place points programmes normally go wrong:

- **Attribution is explicit.** A peer cannot assert ownership on the wire, so `collect` records a
  `node_key` and stops. Only a roster line the operator wrote turns it into someone's uptime.
  Unrostered peers are counted and skipped, never attributed to a guess.
- **Scoring is fail-closed on registration.** `run-epoch` keeps only facts whose id is currently
  registered and drops the rest rather than bucketing them, so an unregistered id cannot reach a
  signed ledger even if an ingest is buggy.
- **Duplicates are collapsed.** A DAG puts one attestation transaction in several blocks — the live
  index returned 407 rows for 186 distinct `(validator, epoch)` pairs. Ingesting raw would push
  participation above 100 %.
- **Every fact carries evidence.** Uptime rows carry the vantage and user agent; attestation rows
  carry the block and transaction they came from. `verify-epoch --facts` re-derives the scores from
  those inputs and byte-compares against the signed ledger.

### Epochs — when points start counting

An epoch is a window the operator publishes over:

```bash
misaka-mtp-service run-epoch --data-dir DIR --operator-key FILE \
  --epoch N --start 2026-07-28T00:00:00Z --end 2026-08-04T00:00:00Z --network testnet-200
```

Two consequences worth being explicit about:

- **Nothing accrues before the first published epoch.** The service does not backfill. Contributions
  made before epoch 1's window exist only if the operator awards them into an epoch.
- **Awards carry an epoch number.** `--epoch N` on the award must match the `run-epoch` that merges
  it, so the operator decides which window a contribution lands in.

**Epoch 1 is open: `2026-07-28T00:00:00Z` → `2026-11-01T00:00:00Z`.** It is published (issue 0) and
visible at `/mtp/v1/epoch/1`, with zero participants so far — the window is open, not finished.

Corrections are the designed path, not an exception: a reissue is a new fully-signed
`epoch-<n>.<issue>.jsonl`, old issues are never deleted, and `index.json` records the supersede
ordering. So awards made during the window land in a later issue of epoch 1. An epoch becomes
immutable only once the finality horizon passes it (I-MTP-13).

The network scope is part of rules version 3; a service configured for retired testnet-10 fails
closed instead of silently publishing a mixed-network ledger.

---

## 9. What you cannot do yet

Stated explicitly so nobody builds on an assumption:

- **Assume every node performs the LLM inference.** Providers produce signed receipt-v3 artifacts;
  ordinary nodes validate the canonical receipt and DA commitments. A continuous public provider
  still needs its own model deployment, keys, capacity policy, and lifecycle automation. See §7.
- **Earn MTP points anonymously.** Every C1 point resolves to a registered ledger id, so uptime and
  attestation from an unregistered node are dropped, not banked. Register first — see §8.
- **Earn new MTP points on retired `testnet-10`.** Rules version 3 scopes testnet-200 only.

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

## 10. Tokenomics, as implemented

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

## 11. Where to go next

| Topic | Document |
|---|---|
| Validator procedure and failure modes | [validator-runbook.md](validator-runbook.md) |
| ML-DSA-87 design | [kaspa-pq-design-mldsa87.md](kaspa-pq-design-mldsa87.md) |
| Consensus spec | [kaspa-pq-spec.md](kaspa-pq-spec.md) |
| Verification runbook | [kaspa-pq-mldsa87-verification-runbook.md](kaspa-pq-mldsa87-verification-runbook.md) |
| Governing migration record | [adr/0019-mldsa87-migration.md](adr/0019-mldsa87-migration.md) |
| PALW audited-compute lane | [adr/0039 onward](adr/) |
| **Qwen3.6 receipts — issue, verify, what they prove** | [palw-llm-receipts.md](palw-llm-receipts.md) |
| **The LLM runtime itself** | [LLM-Validation](https://github.com/MISAKA-BTC/LLM-Validation) |
| Reporting a vulnerability | [../SECURITY.md](../SECURITY.md) |
