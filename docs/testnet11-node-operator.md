# Running a testnet-11 node (PALW)

ADR-0035 §6.5 launch criterion: *"node-operator doc published with the shas and the audit-harness
check"*. This is that document.

testnet-11 is the public PALW network. Every post-genesis block's proof-of-work is **one
deterministic LLM inference**, not a hash. There is no hash lane to fall back to — that is the
point, and it is what makes the rest of this document necessary.

> **Pending change (not yet deployed).** Two independent re-mints are now merged and waiting:
>
> 1. the free-prompt receipt quantum lowered 1,000 → 100 (with `pwu_per_quantum` 100 → 10, holding
>    weight-per-CU constant) — at 1,000 no registered class could reach a single quantum;
> 2. the genesis bond registry grown from six seats to eight, so ADR-0065 D1's maturity fence can
>    be armed by configuration instead of by another re-mint. This one also moves the GENESIS:
>    `d2789338…` → `572f80c0…`, coinbase marker `11,3` → `11,4` ("Relaunch 4").
>
> Together they move this network's fingerprint to
> `5ccdd6841c7510b9fa87b2c69aba8018a3d2eb5ec1709d09dbed3a4cb1f67e44`. **That is neither change's own
> value** — a fingerprint is a function of the whole ruleset, not a sum of diffs, so the figures the
> two branches carried separately (`d29e5667…` and `4f89ec82…`) are both stale now that they are
> merged. Do not deploy against either.
>
> Until this is deployed the live chain is still the value below; when it is, this whole block is
> re-minted and every node wipes again. The value here is what is running NOW, not what the branch
> builds — and because the genesis moves too, an un-wiped node is refused at startup by the
> genesis-mismatch guard rather than silently resuming.
>
> **Current identity (2026-08-30 re-mint, ADR-0059 "Relaunch 3", the 10B premine cap) — wipe
> your datadir if you joined earlier.**
> The live chain's consensus fingerprint is **`f3bf86b4e9327f8b02ab2ad1d121d62ecd11bd78cca1455d8bcd7372595153d8`**,
> genesis **`d2789338…`** (coinbase marker `11,3`; three execution classes + the 547M MSK
> community allocation, now carved out of the single 10B main wallet — the 40-vault block is
> gone and every network's genesis mints exactly 10B).
> A node holding any earlier testnet-11 state does not resume on this chain — it is refused at
> handshake by fingerprint, which is the intended behaviour; `rm -rf` the appdir (or use a new
> one). Earlier identities, all dead: `95265934…` (2026-08-29 audit3 re-mint, genesis
> `c664a224…`), `15bab795…`, `49ff9628…` (Relaunch 2, 2026-08-20, genesis `3564ea39…`),
> `048e6902…`, `25a74c81…`, `bb3e06b4…`, and `bb0a3ad3…`. See
> [adr/0059-the-10b-premine-cap.md](adr/0059-the-10b-premine-cap.md) for what this re-mint
> changed and [testnet11-relaunch2-genesis.md](testnet11-relaunch2-genesis.md) for how the
> allocation constants derive. DNS seeding is live (`seeder1.misakascan.com`): a fresh node
> needs no `--addpeer`.

Read §2 before you build anything. A node outside the determinism class does not sync slowly or
mine badly; it computes different tags, rejects every honest block, and has its own rejected. It
looks like a network fault and is not one.

---

## 1. What you need

| | |
|---|---|
| CPU | **x86-64**. See §2 — Apple Silicon and other arm hosts cannot join this class. |
| cores | 4 free (the worker pins `CPU_THREADS = 4`). More cores do **not** help; see §6. |
| RAM | ~1.4 GiB per resident model on top of the node. 8 GiB comfortable. |
| disk | ~2 GiB for the model + chain data |
| time | **The first sync is measured in hours to days.** §5 gives real numbers. |

---

## 2. The determinism class — check yours before you join

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
x86-64 — 0 of 61 GEMM kernels agreed. Apple Silicon cannot join testnet-11 at launch. That is
stated rather than hidden; an arm class would arrive as its own pin and ADR addendum, not as a
silent widening.

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
PALW_WORKER=/opt/misaka/palw-worker \
MISAKA_PALW_GGUF=/opt/misaka/Qwen3.5-2B-Q4_K_M.gguf \
  ./kaspad --testnet --netsuffix=11 --appdir=/var/lib/misaka-t11 \
           --listen=0.0.0.0:37711 --rpclisten=127.0.0.1:37710
```

`PALW_WORKER` and `MISAKA_PALW_GGUF` are **required**. A node that starts without them refuses at
the startup rail rather than syncing and then failing per-header — the message names both variables.

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

## 4. Verification cost, and the two knobs that change it

By default a node spawns one worker process per header: SHA-256 of the whole 1.28 GiB model, then a
model load, then the inference. Measured on an 8-vCPU EPYC (the reference fleet host):

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

## 5. How long the first sync takes

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

## 6. Things that will not help

* **More cores.** `CPU_THREADS = 4` is pinned by the determinism class. A host cannot trade worker
  count against threads per worker; see §4 for what concurrency actually costs.
* **A GPU.** The x86-64 CPU class is what testnet-11 pins. A Metal or CUDA build is a different
  class and computes different tags.
* **A different quantisation, a re-converted GGUF, or a newer llama.cpp.** All three change the
  class. The size check catches a truncated download; the sha256 catches everything else.

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

* `palw-worker --mode manifest` output (this identifies your class),
* `bash scripts/misaka-palw-headroom.sh <appdir>`,
* the node's participation state line,
* whether any other PALW process runs on the same host.

The first two answer most questions before anyone has to guess.
