# Running a testnet-11 node (PALW)

ADR-0035 §6.5 launch criterion: *"node-operator doc published with the shas and the audit-harness
check"*. This is that document.

testnet-11 is the public PALW network: a block is paid for by **one deterministic LLM inference**,
not by hashing.

> ## Read this first — you probably need neither the worker nor a GGUF
>
> **To run a full node (sync, validate, serve RPC) you need `kaspad` and nothing else.** No
> `misaka-palw-worker`, no `Qwen3.5-2B-Q4_K_M.gguf`, no llama.cpp, no `PALW_WORKER`, no
> `MISAKA_PALW_GGUF`. **Any** x86-64 or arm host, Apple Silicon included — the execution family is
> pinned integer arithmetic and has no per-architecture build (§2).
>
> ```bash
> cargo build --release -p kaspad --bin kaspad     # this is the whole build
> ```
>
> **Why**, so you can check it rather than trust it: testnet-11 runs `ConsensusV2` at **PoW algo 6**,
> where a block carries a signed attempt envelope committing to an execution, and validating one
> means checking commitments and signatures — not re-running a model. The dependency graph is the
> proof: `misaka-palw-base0` (the execution runtime) is a **dev-dependency** of `kaspa-consensus`
> (`consensus/Cargo.toml`), so a node's consensus cannot run a model even if it wanted to. That is
> ADR-0042 Decision 4, made structural by ADR-0053.
>
> The worker's own build script says the same thing and refuses to build without being asked:
> *"You almost certainly do not need this crate. No node needs it to produce or verify a block."*
> `cargo build --release` skips it by default. **The build script is right; earlier revisions of
> this document were wrong**, and §2, §4, §5, §6 and §9 below carry what they were written for.
>
> **Who does need a model** — three roles, and only these three:
>
> | role | what it runs | where |
> |---|---|---|
> | producing blocks for a model class | `kaspad --palw-produce --palw-class-artifact=<file>` | [testnet11-join-mining.md](testnet11-join-mining.md) §5–6c |
> | **seating a panel** on a model class | `kaspad --palw-panel --palw-class-artifact=<file>`, once per class | join-mining §6c — a seat re-executes the claim, so it needs the weights |
> | answering people's prompts | `misaka-palw-gateway --worker palw-a16-fp-worker` (or `palw-qwen36-fp-worker`) | join-mining §7 |
>
> None of them is this document's §2, and none of them is `misaka-palw-worker`: that crate's
> free-prompt arms were deleted by ADR-0077 Decision 5 (consensus refuses their null execution
> root, and no registered class runs on that runtime). The workers that exist are the **family**
> workers in `misaka-palw-base0`, and the gateway spawns one and keeps it resident
> (`--mode v3-serve`): the artifact is mapped, digested and validated once per process rather than
> once per job.

**What the sections below are.** This file was written for the **algo-4 lane**, where every header
was verified by re-running a pinned llama.cpp inference. The published network has not worked that
way since it moved to ConsensusV2. The measurements are real and are kept as a record; the
instructions built on them are marked where they no longer apply. If a section talks about
`palw-worker`, a GGUF sha, `CPU_THREADS`, or a per-header inference cost, it is describing the old
lane.

> ## Second flag day, 2026-09-06 — a node built before this is ALREADY cut off
>
> **Rebuild now. Do not wait for the height.** This release schedules five fences at **DAA 1900**:
> ADR-0062's `palw_da_court` and ADR-0077 Decision 16's `palw_panel_da`, plus ADR-0090 §7's
> `palw_model_market`, `palw_model_lines` and `palw_model_evm`. The height is still ~2 days out at
> the measured rate — and **the fork-id gate does not wait for it.** An un-upgraded node's gate is
> already armed by ADR-0083's fence at 1150; the upgraded peer announces `next = 1900`; 1900 is not
> on the old schedule; so **the old side sends the reject and closes**. The partition arrives the
> moment the first upgraded node connects, not at 1900. On 2026-09-06 the fleet's node0 log carried
> fork-id rejects from **nine addresses outside the fleet** (and one stray un-upgraded process of our
> own), plus handshake refusals from two nodes still on archived genesis hashes — `8d2002cc…`
> (Relaunch 4) and `4b619a1a…` — which no rebuild of this release will admit; those must wipe and
> resync.
>
> **What it looks like from the un-upgraded side** — and every one of these is the same cause:
>
> * the peer count never settles (3-of-8 and falling), connections drop after minutes with
>   `broken pipe` / `stream ended`;
> * blocks you produce are accepted by your own node and never appear on
>   [misakascan](https://misakascan.com), because they are on your arm of the partition;
> * your log carries the reject, and it names both schedules:
>
> ```
> [WARN ] P2P, got reject message: Fork-id mismatch on network misaka-testnet-11 at DAA 1679 -
>         this node has crossed fence 1150; the peer agrees about every fence crossed so far but
>         expects fence 1900 next, not 2125000.
> ```
>
> Read that line from the sender: `1900` is what the **upgraded peer** expects next, `2125000` is
> what **this build** expects. Seeing `not 2125000` means *you* are the old build. Nothing deployed
> on the fleet can end this for you — the refusal is sent by your own binary.
>
> **No datadir wipe.** Genesis and the peering identity are unchanged from 5f; only the fingerprint
> moves. Rebuild, restart, keep your appdir. Confirm with the startup line:
>
> ```
> [INFO ] Consensus params fingerprint: 060e3597cd2950bc183b215b5ff87538e72dd788cab43829dca6bc72bcb5ac89 (network testnet-11)
> ```

> **Current identity — Relaunch 5f (genesis re-minted 2026-09-03) carrying THREE flag days: ADR-0083's
> fence at DAA 1150 (2026-09-04; the difficulty window counts only rows priced by `bits`, because the
> heartbeat lane had priced every attempt lane off the chain — card §10b) and the DA-court/model-market
> fences at DAA 1900 (2026-09-06; the block above), and the refutation ladder at DAA 2150
> (ADR-0084 U-08; ADR-0092). Wipe your datadir only if you joined before 5f — no flag day re-minted
> the genesis. Build from `main` at `06bf5118` or later: every earlier fingerprint is refused, the
> pre-5f ones at the handshake and the pre-flag-day ones by the fork-id gate, now rather than at the
> height.**
>
> > **If you already rebuilt today, rebuild again.** `4fcce4b0` and `6dea4f5f` are superseded and
> > **must not be run past DAA 1900**. They schedule the 1900 fences but not the ladder's own height,
> > and the gate compares HEIGHTS: from 1900 a node on either will refuse a node carrying the ladder,
> > and it is the older side that sends the reject, so the fleet cannot fix it for you. The ladder
> > first rode 1900, where the gate could not see it at all — two builds peering and then disagreeing
> > about which closes are valid. Moving it to 2150 is what makes the difference visible.
>
> * consensus fingerprint: **`060e3597cd2950bc183b215b5ff87538e72dd788cab43829dca6bc72bcb5ac89`**
>   (2026-09-06 flag day; the pre-flag-day value `71b35c25…` names the same genesis and the same
>   peering identity, and is refused by the gate as above).
> * genesis hash: **`ad30b5cb965ad305dfa1dc7516935763ea2623105581b83bb9359c7247157d36b0f8003b337cdad366e3895c8f159e99332be16e258b144dddf483bf9b33edb7`** (`PALW_RC_GENESIS`,
>   coinbase payload marker `misaka-palw-rc`) — measured from the running node's own startup line,
>   which is the only value your node will be judged by.
> * build: **`main` at `06bf5118` or later**;
>   the fleet's eight nodes all run kaspad sha256 `7181cc07eb57a2f8`. Any commit whose build announces
>   the fingerprint above will do — and that, not a commit id, is the check:
>   `kaspad --testnet --netsuffix=11` prints it on the second line of its startup log. Everything
>   earlier — the 5f tag `testnet-11-relaunch-5f-adr0083-h1150` (`16a2f277`), the fleet's previous
>   `cef2ecdb`, and the 5f cut `2222e054…` — is on the far side of the gate **now**, not at 1900; and
>   `4fcce4b0` / `6dea4f5f` (fingerprints `b511dd1e…` / `ebd3b321…`) are on the far side of it from
>   **1900**, not from 2150.
> * classes registered at genesis: BASE-0 `f1c5635c…` (the floor, no model, 22‰), PALW-QWEN25-A16
>   graph-v5@512 `4277d84f…` (489‰, artifact root `bcf2d9eb…`), PALW-QWEN36 graph-v3 `5bd9ae3d…`
>   (489‰, artifact root `f4aad4fd…`). Post-genesis registrations are chain data: read them from the
>   explorer or `--palw-dump-classes` on a scratch node, not from this page.
> * what a miner will meet: a floor block is ~12,664 expected class draws, each also needing the
>   Layer-0 digest under `bits` — which the fence restored to MAX (p = 0.5) at DAA 1150 after the
>   heartbeat lane had tightened it ×3,100. Mining the floor is the slow lane, on purpose.
>
> Predecessors `a7baab79…` (Relaunch 5e, genesis `08e9c8a4…`), `e2b91c16…` (Relaunch 5d), `d38abe44…` (5c), `accaadce…` and `f0e50f83…` (all
> 2026-09-02) and `5ccdd684…` (Relaunch 4, genesis `8d2002cc…`) are archived, not continued. The 10B premine cap,
> the eight genesis bond seats and the community allocations are unchanged from Relaunch 4.
> Post-genesis class registrations from any previous chain do not carry over and must be
> re-registered on this chain. What changed in consensus is recorded in
> [testnet11-relaunch5-runbook.md](testnet11-relaunch5-runbook.md).

---

## 1. What you need

**To run a full node:**

| | |
|---|---|
| CPU | **any** x86-64 or arm host, Apple Silicon included. See §2: the execution family is pinned integer arithmetic with no host-dependent branch, so there is no determinism class to be outside of — for producers either. |
| cores | whatever the node itself wants. There is no pinned thread count on this path. |
| RAM | the node's own working set. Measured on the operator fleet, a validating node settles near **8–11 GiB**, so give it a `MemoryMax` (see §6) rather than assuming it stays small. |
| disk | chain data only — no model to store. |
| time | **not measured on this lane yet.** §5's hours-to-days figures were dominated by a per-header inference a node no longer runs, so they are an upper bound rather than an estimate. If you time a first sync, please report it — §9. |

**To PRODUCE blocks for a model class** you additionally need that class's artifact and the flags in
[testnet11-join-mining.md](testnet11-join-mining.md) — plus RAM and time for the model itself, which
is a size question and not an architecture one. Producing for the model-free floor class needs no
artifact at all, on any host.

---

## 2. The determinism class — **withdrawn; nothing on this network has one**

> **Superseded, and not only for verifiers.** Everything below describes the pinned llama.cpp
> worker that verified algo-4 headers. The execution family that replaced it is **pinned integer
> arithmetic in this tree's own Rust** (ADR-0053), and integer arithmetic does not vary by host.
> There is no determinism class to be inside or outside of — **for a producer either**, which is
> where an earlier revision of this correction still got it wrong.
>
> Three facts, each checkable in the tree rather than taken on trust:
>
> * **No architecture branch exists on the execution path.** `misaka-palw-base0`'s `engine.rs`,
>   `engine_a16.rs` and `qwen36.rs`, and `palw_base0_ops.rs` / `palw_qwen36_ops.rs` in
>   consensus-core, contain no `#[cfg(target_arch)]`, no `is_x86_feature_detected!`, and no
>   `aarch64` path. There is one implementation and every host runs it.
> * **The runtime is not part of a class's identity.** `runtime_class_id` and
>   `runtime_manifest_hash` are `Hash64::default()` everywhere on this path — the integer family's
>   identity IS its graph (`backend.rs`). A CPU class cannot scope a class that does not carry one.
> * **The pins below pin a thing no node runs.** GGUF sha, llama.cpp commit, `GGML_*` flags,
>   `CPU_THREADS = 4`, `misaka-palw-lite-cpu/x86_64/v1` — all properties of the float worker.
>
> So **Apple Silicon, arm servers and x86-64 are the same class, which is to say no class**, whether
> you verify or produce. The float lane needed the scoping because llama.cpp's kernels differ by
> instruction set; that is exactly why the network left it.
>
> **And it is measured, on a real class, across architectures — both halves.**
>
> * **Execution.** `palw-fp-on-registered-classes.md` records the same prompt ("What is 2+2?", 7
>   tokens, decode 9) run on the real 1.7 GiB A16 artifact on *this repo's arm64 dev machine and
>   the x86_64 fleet host*: **identical output ids, all four leg roots, execution root, CU and
>   claim id.** Its own words: "the integer family's determinism claim, held across architectures
>   on the real class — not the unit-test geometry."
> * **Conversion.** `palw-public-testnet-classes-runbook.md` records the Qwen3.6 artifact
>   reproduced by "two full conversions on x86-64/Linux (byte-compared), one full conversion on
>   arm64/macOS streaming the public URL … every route lands on this root" — different instruction
>   sets, the same 36 GiB of int8 codes, byte for byte.
>
> An earlier revision of this section said the repository held no cross-architecture measurement.
> It holds two, on the real artifacts, and they are the reason the paragraph above is a statement
> rather than an expectation.

The tag a node computes must be bit-identical to every other node's, so the runtime is pinned, not
merely recommended.

| what | value |
|---|---|
| GGUF | `Qwen3.5-2B-Q4_K_M.gguf` |
| GGUF sha256 | `aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223` |
| GGUF size | `1280835840` bytes exactly |
| base model | `Qwen/Qwen3.5-2B` @ `15852e8c16360a2fea060d615a32b45270f8a8fc` |
| llama.cpp | commit `030ebb558a5820b444a8f836ed5cdd46c9b4bd7a`, build `10358`, unpatched |
| worker build profile | `release/cpu-only/single-variant/no-native/no-lto/no-blas/no-openmp/threads-4/gpu-off/static/v1` |
| worker sha256 (fleet 2026-08-18) | `2bd857f805baa55f…` |
| runtime class | `misaka-palw-lite-cpu/x86_64/v1` |

**The class is scoped to the instruction set, and that scoping is deliberate.** An arm build is a
different class (`misaka-palw-lite-cpu/aarch64-dotprod/v1`) and measured out of class against
x86-64 — 0 of 61 GEMM kernels agreed. Apple Silicon could not run an algo-4 *worker*, which is what
this sentence originally said and what the banner above corrects: it was never a statement about a
node. An arm class would arrive as its own pin and ADR addendum, not as a silent widening.

Two build flags are not optional, and neither is a preference:

* `GGML_CPU_ALL_VARIANTS=OFF` — ON compiles several kernel variants and picks one by runtime CPUID,
  which splits hosts *inside* one class label. That is precisely the failure the class exists to
  prevent.
* `GGML_OPENMP=OFF` — under OpenMP the matmul's work split and reduction order come from an
  external runtime's scheduling rather than from ggml's threadpool at the pinned thread count.

Both are read out of the tree's own `CMakeCache.txt` at build time and hashed into the manifest, so
a mismatch changes the runtime's *identity* instead of silently changing its arithmetic.

### Check your own build

```bash
MISAKA_PALW_GGUF=/path/to/Qwen3.5-2B-Q4_K_M.gguf ./palw-worker --mode manifest
```

`runtime_class_id` must match the fleet's published value. If it does not, you are out of class —
do not join, and do not assume it will "mostly work".

The forgery-audit harness doubles as a self-service class check, and is the same methodology that
produced the pin:

```bash
MISAKA_PALW_GGUF=/path/to/Qwen3.5-2B-Q4_K_M.gguf \
  bash scripts/misaka-palw-v2-class-probe.sh ./palw-worker my-host > class-my-host.json
python3 scripts/misaka-palw-v2-class-compare.py class-*.json     # want: ONE-CLASS
```

A `ONE-CLASS` verdict is **evidence, not proof** — the probe corpus is four fixed jobs compiled
into the worker. Treat `BUILD-MISMATCH` as "determinism untested", not as "hosts disagree".

---

## 3. Running the node

### Build kaspad — the EVM lane is part of the default build

```bash
cargo build --release -p kaspad --bin kaspad
```

testnet-11 inherits `evm_activation_daa_score: 0` from the testnet params, so the EVM lane is
active from the first block — and since 2026-08-21 the `evm` feature is a **default** feature of
kaspad, so a plain build already carries the in-process revm executor. There is no separate EVM
daemon and nothing extra to run: starting kaspad IS starting the lane, and whether it executes is
decided by the network's own params.

The history matters only if you build with `--no-default-features`: such a binary **refuses to
start** on this network (`EvmLaneRequiresEvmBuild`, a startup message naming the fix) rather than
panicking at its first block template minutes after a healthy-looking start, which is what the
pre-guard builds did. `--features evm` in older scripts is now a harmless no-op.

### Run

```bash
./kaspad --testnet --netsuffix=11 --appdir=/var/lib/misaka-t11 \
         --listen=0.0.0.0:37711 --rpclisten=127.0.0.1:37710
```

**No environment variables.** `PALW_WORKER` and `MISAKA_PALW_GGUF` are **not** required, and this
document said they were until 2026-09-01 — the error a community operator found and reported.

The startup rail that demands them exists, and it is conditional: it fires only when the network's
`pow_palw_activation` is in force. On testnet-11 that activation is `never`, so the rail is not
reached and a node without either variable starts normally. You can check the same fact the code
does:

```bash
# testnet-11: pow_palw_activation = false, consensus mode = ConsensusV2, algorithm_id = 6
```

If you set them anyway, nothing happens — no node process on this network reads them.

Discovery (seeder names for testnet-11) is an operator item that is **not settled yet**; until it
is, join with `--addpeer=<host>:37711` against a published peer. `n11-seed*.misakascan.com` do not
resolve today — do not configure them.

### Mining

> **This section described the algo-4 lane and no longer applies.** The published testnet-11 runs
> ConsensusV2 (PoW algo **6**), where a block carries a signed attempt envelope and the nonce is won
> by inference. `misaminer` cannot mine it — it refuses with a message saying so rather than
> searching a target it cannot win — and neither can any other external client. Blocks are produced
> by `kaspad --palw-produce` with a bonded key.
>
> The current path, including how a node that is on no genesis registry obtains a bond, is
> [testnet11-join-mining.md](testnet11-join-mining.md). Ports and the consensus fingerprint there
> are the live ones; the ones elsewhere in this document are not.

---

## 4. Verification cost — **the algo-4 lane's numbers; not today's**

> **Superseded.** A node on the published network spawns no worker and loads no model, so none of
> the knobs below apply and none of the costs below are paid. Validating a ConsensusV2 header is
> signature and commitment checking. Kept as the record of what the old lane cost, which is also
> why the network left it.
>
> `MISAKA_PALW_AGENT`, `MISAKA_PALW_CONCURRENCY` and `MISAKA_PALW_LEASE_DIR` are read by the
> **worker**, not by the node. Setting them on a full node does nothing.

By default an algo-4 node spawned one worker process per header: SHA-256 of the whole 1.28 GiB
model, then a model load, then the inference. Measured on an 8-vCPU EPYC (the reference fleet
host):

| | per header |
|---|---|
| pin SHA-256 + model load | 4.64 s |
| the inference itself | 13.9–16.0 s |
| **one-shot total** | **~18.6–20.7 s** |

**`MISAKA_PALW_AGENT=1`** keeps one worker resident with the model loaded and feeds it headers,
removing the 4.64 s. Expect **~1.3×** on this class. (On a GPU-class host it is 5.7×, because there
the inference is a rounding error and the overhead was everything. On the CPU class the inference
*is* the cost.) It changes no consensus rule — the projection is byte-identical — and every failure
falls back to the one-shot path.

**`MISAKA_PALW_CONCURRENCY=N`** (default 1) allows N inferences at once. **Measure before raising
it.** On the reference host two concurrent inferences took **73.6 s each** against 13.9 s alone —
0.38× the serial throughput, i.e. a sync **2.6× slower**. The limit is memory bandwidth, not cores:
two workers × 4 pinned threads is exactly the 8 available cores and `vmstat` reports zero steal.
A 12-core laptop saturates at 1.77×. The honest range across two real hosts is **0.38× to 1.77×** —
one of them worse than doing nothing.

**`MISAKA_PALW_LEASE_DIR=<dir>`** makes the concurrency bound cover the **host** rather than one
process. Set it if the machine runs more than one PALW node: two node processes are two concurrent
inferences whatever `MISAKA_PALW_CONCURRENCY` says, which is the harmful configuration entered
without touching the knob. It uses `flock`, so a crashed node releases its slot rather than leaking
it forever.

**Do not co-locate two PALW nodes without it.** Measured on the reference host, which runs two: the
syncing node's per-header time has its mode at 10–15 s (its uncontended cost) but a median of
19.8 s, a mean of 29.1 s and a maximum of 303 s. Separating them is worth roughly **+60 % sync
rate**.

---

## 5. How long the first sync takes — **measured on the algo-4 lane**

> **Superseded.** The hours-to-days figures below are dominated by a ~19 s per-header inference that
> a node no longer performs. Today the per-header cost is ordinary signature and commitment
> checking, so a first sync is hours at most and the "headroom" margin below is not a constraint a
> full node can fail. The section is kept because the *shape* of the argument still applies to a
> PRODUCER, whose block rate is bounded by its own inference time.

Measured, on the reference host, joining from genesis over the public path:

* **7 h 45 m** for a chain ~1,300 blocks old (~1.5 days of history).
* Sustained rate ~163 headers/hour against a chain producing ~40 blocks/hour.

The cost grows with the chain's age until the pruning proof caps it, at which point a newcomer
verifies the proof's headers instead of all of them — roughly 35–50 hours at this class's per-header
cost. **Budget 1.5–2 days for a first sync on a mature chain**, and do not plan around minutes.

### The margin to watch

The number that decides whether the network can admit new nodes at all is

```
headroom = block interval ÷ per-header verification cost
```

Below 1× a node can never finish syncing. Measured on the reference host: **3.0–5.4×**. It falls if
the block interval shortens, the model gets slower, the host gets busier, or a second PALW node is
co-located.

```bash
bash scripts/misaka-palw-headroom.sh /var/lib/misaka-t11
# hdr_p50=15.7s spb=84s headroom=5.4x workers=1
```

`HEADROOM-LOW` fires under 2× **and** when the margin could not be measured — a missing number is
not reassurance. `CO-LOCATED` firing means co-location; its silence does not prove isolation.

---

## 6. Running more than one node on a host

**Give every node a memory limit.** Measured on the operator fleet 2026-09-01: a host running three
validating nodes with `MemoryMax = infinity` had them grow to 20.1 GiB of a 23 GiB box, and the
kernel OOM-killed one of them 29 times in a day. The consumption is the node's own heap — its
consensus caches — and **not** any model: the same measurement found 0.00 GiB resident in the
mapped artifacts, which are `mmap`ped and never touched by a validating node.

A drop-in like the reference fleet's:

```ini
# /etc/systemd/system/<your-unit>.d/memory.conf
[Service]
MemoryHigh=13G
MemoryMax=17G
```

Sizes are the host's to choose; what matters is that a limit exists, so the kernel throttles one
node instead of killing whichever it likes.

### Things that will not help

* **More cores, a GPU, a different quantisation.** These were the algo-4 lane's levers, when a node
  ran an inference per header. A node runs none, so none of them change its cost.
* **`--palw-class-artifact` on a validating seat.** Measured: resident 0.00 GiB. A panel seat
  verifies material by recomputing roots from the material itself, not by running the model. Give
  the artifact to producers.

---

## 7. If your node says `quarantined`

`Chain participation held: state=quarantined. Not mining, not attesting, reporting unsynced.`

The node has synced a chain it cannot vouch for and will not act on it. It is deliberate and does
not clear on its own. Resolve it, then:

```bash
./kaspad … --clear-quarantine      # fires once per boot; REMOVE it from the unit afterwards
```

Left in the unit file it re-clears on every restart, and a quarantine a flag always clears is not a
quarantine — the node logs exactly that at WARN each time it fires.

> **Known issue, fixed but not yet everywhere.** Until the fix in `fix(ibd): a refused chain switch
> no longer feeds the counter that refused it` is in your binary, a node could reach the switch cap
> without ever having switched chains — every refused candidate advanced the counter that refused
> it — and `--clear-quarantine` could not recover it, because the count it preserved was what
> re-quarantined the node seconds later. A node showing `switched chains N times` for an N far above
> 5 has hit this. Update before troubleshooting anything else.

---

## 8. Emission at launch

* Full schedule **4445.62 MSK/block** (rate-preserving 120 s table).
* **Until validators bond, only the worker BASE share is minted: 62 %, 2756.28 MSK/block to the
  miner.** The validator 30 % is not minted, and the 8 % inclusion share follows its own pool path.
* The fixed-difficulty launch window is exactly `min_difficulty_window_size = 150` blocks and runs
  at roughly target cadence, so there is no burst-emission window.

---

## 9. Reporting a problem

Include, always:

* the node's **consensus params fingerprint** and **genesis hash**, copied from its own startup
  log rather than from any document — every fingerprint written down anywhere goes stale, and the
  one in your logs is the one your node will be judged by,
* the node's participation state line,
* `kaspad --version`,
* whether any other node runs on the same host, and whether it has a `MemoryMax` (§6).

If you are producing rather than only validating, add the class id your producer was started with
and the `PALW court certified end-to-end for: …` line from startup.

`palw-worker --mode manifest` was the first item here and is no longer meaningful: a full node has
no worker to ask, and `palw-worker` no longer has that mode. If you are running the free-prompt
gateway, the equivalent two lines are the gateway's own `/health` (which names `class_id`, `n_ctx`,
`confinement_backend`, and the chain's `registered` / `fp_certified` / `bond_active` /
`exposure_room`) and:

```bash
misaka node security-report --worker ./target/release/palw-a16-fp-worker
```

which prints the posture from live state — the backend actually in force, every listening socket
and whether its bind needed an acknowledgement, which processes hold key material, and the
artifact digests. It exits 13 (EXPOSED) or 14 (DEGRADED) rather than telling you it is fine.
