# testnet-10 PALW rollout runbook — the LLM-PoW re-genesis

> ## ⚠️ STATUS 2026-08-13: the PoW this runbook deploys is DISABLED. Do not follow §"Identity"
> ## or §"Per-host prerequisites" as written.
>
> `algo_id = 5` (PALW via Ollama) was measured **forgeable without running the model** and
> switched off in `9736aec`: `TESTNET_PARAMS.pow_palw_ollama_activation = never()`. The public
> preset falls back to the sound Phase-3 BLAKE2b-SHA3 PoW (`algo_id = 3`) until the
> logits-binding replacement ships.
>
> Why: the canonical prompt makes the pinned model emit ONE constant 16-token continuation for
> every seed (27/27 seeds, 22 of them uniform random). The 72-byte tag is therefore a 64-byte
> constant, a `prompt_eval_count` drawn from ~10 values, and a constant 16 — guessable with ~31
> BLAKE2b hashes and zero inference, while an honest miner pays 12-26 s per attempt. The verifier
> replays the real model, gets the same constant, and ACCEPTS the forgery. Checkable with no
> runtime at all: the first 64 bytes of `POW_L1_PALW_OLLAMA_CALIBRATION_V1` **are** the digest of
> that boilerplate string.
>
> What this invalidates in the text below, beyond the PoW itself:
>
> * **The consensus fingerprint in §Identity is stale.** It reads `a044a672…`; the preset now
>   produces `dd720ae3353a3f04c1d96e9f4dc7854c21388e32160bf0a5fe828f5681e023c5`. Verify against
>   the pin in `consensus_params_id_tests`, never against this table.
> * **The Ollama prerequisites (§Per-host) are no longer a consensus requirement.** A node on the
>   current preset needs no Ollama server and no model pin to validate.
> * **The "fleet agrees 8/8" determinism evidence is void** wherever it is cited — it measured
>   agreement on a constant, which no rounding-order difference could have moved. See
>   §"Host class" for what replaced it.
>
> The §"Why 120 s" reasoning, the VLT window table and the rollout ordering are unaffected and
> still current. The replacement PoW binds the LOGITS via the worker's `gemm_trace_root`; its
> host requirements are in §"Determinism class of the v2 worker" below.

The release that makes the public testnet's proof-of-work one deterministic Qwen3.5-2B
inference per attempt (ADR-0021), at one block per two minutes. This is a **re-genesis**, the
"-bs3" precedent: the new chain is cryptographically distinct, an un-wiped node hits the
startup genesis-mismatch guard, and nothing of the old chain (balances included) carries over.

## Identity of the new network

| what | value |
|---|---|
| genesis hash | `477f85fc a51674f5 …` (`TESTNET_GENESIS`, "-palw" payload marker) |
| PoW model | `misaka-palw-2b-f16` digest `d5d0bc552430…` (F16 profile; canonical GGUF sha `575eddc35774…` — created from the file, NOT pulled; the registry Q8_0 blob is non-portable across ISAs) |
| consensus fingerprint | `a044a6723956b7746f04ae71f1b987ce1aee366c0affc1df119bb8d5dfd6a0a5` (pin it MATERIALIZED — `Params::from(net)` — per the 8208cd6 lesson) |
| PoW | `algo_id = 5` (PALW via **Ollama**) from genesis; fixture env **refused** on this network |
| block rate | **one block per 120 s** (`target_time_per_block = 120_000`, ghostdag k = 1) — see §"Why 120 s" |
| genesis difficulty | `0x207fffff` (p ≈ ½ per inference; DAA converges within the 150-block min window) |
| block subsidy (year 1) | 444_562_014_000 sompi ≈ 4 445.62 MSK per block — the same 37.047 MSK/s rate |
| emission fork | none: `crescendo_activation = always` (the 88_657_000 score belonged to the old chain) |
| VLT windows | wall-clock preserved (table in `TESTNET_DNS_PARAMS`): 14-day evidence/unbond = 10_080/10_083 blocks, epochs 2 blocks (4 min), gate horizon 360 (12 h) / veto TTL 120 (4 h), settlement 600 (20 h) |
| VLT shadow fork (ADR-0024 step 3) | **genesis-active** (`TESTNET_VLT_SHADOW_FORK_DAA_SCORE = 0`) — overlay crediting, committee draw, audit fee, challenge slashing AND the bond spend gate all from block 1; the weight fence (step 4) stays dormant |
| DNS seeders | unchanged records (`seeder1-4.misakascan.com`, `seeder1-3.misakachain.com`, `seeder1.misakastake.com`) — the daemons must run this build to crawl the new chain |

## Why 120 s (the one parameter that cannot be forked later)

On a chain whose PoW *is* an inference, the block interval is a capacity decision, not a UX one.
A validator replays every OTHER miner's winning attempt, so its steady-state load is
`(M-1)/M · r / T` — per-header replay cost `r` over interval `T`. Fleet measurements
(2B/F16 profile, N = 16 decode tokens, `num_gpu = 0`):

| profile / host | r (per-header replay) | load at T = 10 s | at T = 60 s | **at T = 120 s** |
|---|---|---|---|---|
| 2B F16, EPYC + F16C (measured) | **4.4 s** / **9.0 s** | 44-90 % | 7-15 % | **4-8 %** |
| 2B F16, Broadwell **without F16C** (measured) | **33 s** | 330 % — impossible | 55 % | **18-28 %** |
| Qwen3.6-35B-A3B, ≥ 32 GB hosts (est., prefix-cached) | ~30-60 s | — | 33-67 % | **17-33 %** |

Measured 2026-08-12 on the three-host fleet at N = 16, `num_gpu = 0`: h1 EPYC-6c 4.4 s, h3
EPYC-8c 9.0 s, h2 Broadwell-8c 33 s steady state (its first samples — 97-244 s — are backlog
contention, not the steady cost). Load figures assume every host also mines; a pure validator
replays every block instead of `(M-1)/M` of them, which is the upper number in each range.

The **model** can be replaced by an algo-id fork on the same chain (that is what `algo_id` is
for). The **block rate** cannot: there is no forked-blockrate machinery, so changing it later
means another re-genesis. T = 120 s is therefore chosen as the smallest interval that keeps the
slowest honest validator comfortable on today's 2B profile *and* still fits the 35B runtime the
network intends to adopt — at 60 s the 35B fork would arrive at 33-67 % load with no headroom for
the VLT overlay, RPC and seeder work these hosts also carry.

**Host class: F16C matters more than core count.** The same F16 model, same tokens, same thread
count is **4-7× slower on a CPU without the F16C flag** (h2: 33 s vs h1's 4.4 s on FEWER cores) —
without hardware fp16↔fp32 conversion ggml does it in software, and an F16 model is nothing but
that conversion. F16C has shipped on x86 since ~2013; h2 only lacks it because its hypervisor
masks the flag. Check `grep -o f16c /proc/cpuinfo` when provisioning: a host without it carries
several times the load on the F16 profile, and it is the host that sets the network's floor.

A host with the flag masked still validates *correctly* — but **not** for the reason this section
used to give. The old justification was "determinism is unaffected, that is why the fleet agrees
8/8", and that 8-seed campaign is void: it compared a constant continuation, which no
reduction-order difference could have moved (see the STATUS banner). The claim now rests on a
direct measurement of the v2 worker instead, and on two facts about how the flag is used:

* **The masked CPUID bit does not stop the instructions.** Measured on h2 (Broadwell, flag absent
  from `/proc/cpuinfo`): a `-mf16c` binary executing `VCVTPS2PH`/`VCVTPH2PS` returns cleanly
  (`exit=0`). The hypervisor hides the bit; the silicon still runs it. So a `GGML_F16C=ON` build
  runs on h2 rather than trapping, which is why h2 can be in the class at all.
* **The manifest pins what the BUILD requires, not what the host advertises.**
  `runtime_cpu_feature_mask` is the constant `"build-required:unpinned"`
  (`misaka-palw-worker/src/main.rs`), deliberately NOT the CPUID probe —
  `host_cpu_features_string()` exists but is display-only. Had CPUID fed the manifest, h1 and h2
  would report different `runtime_manifest_hash` values for identical arithmetic and could never
  share a class. This is the single decision that makes a mixed-hypervisor fleet workable.

Measured 2026-08-13, one binary across all four x86_64 hosts, full-logits v2 trace: **ONE-CLASS,
4/4 hosts, 4/4 probe jobs byte-identical**, including h1 (EPYC, flag present) vs h2 (Broadwell,
flag masked). Golden-set files identical on all four (`81fa2fca…`), and the cross-check —
h1 verifying h2's set and h2 verifying h1's through the real load-and-verify path — passes both
ways. Reproduce with `scripts/misaka-palw-v2-class-probe.sh` per host, then
`scripts/misaka-palw-v2-class-compare.py` over the collected class lines.

Consequences to plan for: 35B adoption additionally needs **≥ 32 GB RAM hosts** (its Q4_K_M blob
is 23.9 GB — the current 11/15/23 GB fleet cannot hold it), and at 120 s the DAA window (264
blocks) is 8.8 h of difficulty memory, so a hashrate change takes that long to be fully absorbed.

## Determinism class of the v2 worker — build ONCE, distribute the binary

This section supersedes §"Per-host prerequisites" for the logits-binding replacement. It states
one rule, because getting it wrong produces a fleet that computes identical arithmetic and still
cannot form a committee.

**Build the worker once and ship that binary to every host. Never build per host.**

`worker_binary_sha256` is inside the `RuntimeManifestV2` preimage, and a validator's
`PalwCapabilityDeclarationV2` bonds the resulting `runtime_manifest_hash`. Two hosts that compile
the same source with different toolchains therefore *declare different runtimes* even though they
agree on every trace. Measured on this fleet: **h1 has rustc 1.95.0 and h2 has rustc 1.97.0**, so
per-host builds are not a hypothetical. The consequences are concrete:

* Their `runtime_manifest_hash` values differ, so the identity each bonds on-chain differs.
* The golden-set gate refuses across them — `cmake_cache_sha256` /
  `llama_static_library_sha256` are compared on load, so h1's set is rejected by h2 with
  "the vectors were MEASURED under a build this worker is not".
* Any cross-host determinism measurement becomes inconclusive: the comparator reports
  BUILD-MISMATCH and correctly declines to answer the arithmetic question.

The binary is safe to distribute: llama.cpp/ggml are linked **statically**, so only libstdc++ and
libc are dynamic, and the fleet is uniform there (Ubuntu 24.04, glibc 2.39 on all four hosts).
Distribution recipe, per the 2026-08-13 run:

```bash
# ONE build host, against the pinned CPU-profile llama.cpp tree
MISAKA_PALW_CPU=1 MISAKA_LLAMA_SRC=$HOME/llama.cpp-cpu \
  cargo build --release -p misaka-palw-worker
# ship the SAME bytes everywhere, then confirm they are the same bytes
sha256sum target/release/palw-worker      # 5a3bdc9a5ea7b146… on the 2026-08-13 fleet
```

Per host, the remaining prerequisites are only data and a pinned tree:

| what | value |
|---|---|
| llama.cpp tree | commit `030ebb558a5820b444a8f836ed5cdd46c9b4bd7a` = `qwen35_pins::LLAMA_COMMIT` |
| CPU-profile flags | `NATIVE=OFF F16C=ON AVX2=ON FMA=ON SSE42=ON CPU_ALL_VARIANTS=OFF OPENMP=OFF BLAS=OFF METAL=OFF` |
| model | `Qwen3.5-2B-Q4_K_M.gguf`, **1 280 835 840 bytes** (`qwen35_pins::GGUF_SIZE`) at `MISAKA_PALW_GGUF` |
| fp environment | must probe canonical — `rounding=rne,ftz=0,daz=0` |

`GGML_CPU_ALL_VARIANTS=OFF` is not optional: ON compiles several kernel variants and selects one
by runtime CPUID, which would split hosts *inside* a single class label — precisely the failure
the class mechanism exists to prevent. `GGML_OPENMP=OFF` likewise, because under OpenMP the
matmul's work split and reduction order come from an external runtime's scheduling rather than
from ggml's threadpool at the pinned thread count. Both flags are read out of the tree's own
`CMakeCache.txt` at build time and hashed into the manifest, so a mismatch changes the runtime's
identity instead of silently changing its arithmetic.

Verify a fleet is one class before relying on it:

```bash
# on each host
MISAKA_PALW_GGUF=/tmp/Qwen3.5-2B-Q4_K_M.gguf \
  bash scripts/misaka-palw-v2-class-probe.sh ./palw-worker <label> > class-<label>.json
# then, with all class lines collected
python3 scripts/misaka-palw-v2-class-compare.py class-*.json    # want: ONE-CLASS
```

A `ONE-CLASS` verdict is evidence, not proof — the probe corpus is 4 fixed jobs compiled into the
worker. Treat a `BUILD-MISMATCH` as "determinism untested", not as "hosts disagree".

### Optional: the resident verification agent (`MISAKA_PALW_AGENT=1`)

By default a node spawns one `palw-worker` process per PoW seed, and each spawn SHA-256s the whole
1.28 GiB model and reloads it before doing any inference. Setting `MISAKA_PALW_AGENT=1` makes the
node keep ONE `palw-worker --mode pow-agent` child with the model resident and feed it seeds
instead (ADR-0041 Decision 1′).

**How much it buys depends on the class, and on the CPU class it is modest.** Measured per header:

| | dev box (Metal class) | `misaka-ibm` (CPU class, 8 vCPU EPYC) |
|---|---|---|
| pin SHA-256 + model load (what the agent removes) | 2.70 s | **4.64 s** |
| the inference itself (what it cannot remove) | 0.18–0.57 s | **13.9–16.0 s** |
| one-shot total | ~3.3 s | ~18.6–20.7 s |
| **with the agent** | 0.57 s (**5.7×**) | 13.9–16.0 s (**~1.3×**) |

On the CPU class the inference dominates, so expect roughly **1.3×**, not the 5.7× a GPU-class box
shows. It is still free of any security argument and still worth enabling for a sync.

```bash
PALW_WORKER=/opt/misaka/palw-worker MISAKA_PALW_GGUF=/opt/misaka/Qwen3.5-2B-Q4_K_M.gguf MISAKA_PALW_AGENT=1 ./kaspad --testnet --netsuffix=11 …
```

What an operator needs to know about it:

* **It cannot change consensus.** The agent computes the same document a one-shot process does —
  verified byte-identically, in every field the tag derives from — and the node reads both with the
  same parser. Enabling it on some hosts and not others does not split a network.
* **Every failure falls back.** If the agent cannot spawn, hangs, dies, or answers something
  unparseable, the node uses the one-shot path for that seed and logs a `warn`. The worst case of
  turning it on is the cost of leaving it off.
* **It is one more long-lived process holding the model.** Budget ~1.4 GiB resident for it, on top
  of kaspad. It exits by itself when kaspad does (its stdin closes), so it leaves no orphan.
* **To confirm it is actually in use**, look for `resident agent ready` at `info` on the first
  verified header. `palw-pow: resident agent unavailable …; using one-shot workers` means it fell
  back — the node is correct but slow, and the reason is on the same line.

### Optional: verification concurrency (`MISAKA_PALW_CONCURRENCY=N`)

Defaults to **1** — one inference in flight, the behaviour this path has always had. Raising it lets
the pruning-proof validator verify header PoW in batches of `N` (ADR-0041 Decision 2). It changes
nothing about what is accepted; it costs `N × ~1.4 GiB` of resident model, since the agent pool
grows to `N` and never shrinks.

**On `misaka-ibm` it is actively harmful — leave it at 1.** Measured 2026-08-18 with the node
stopped: one worker does an inference in 13.9 s; **two concurrent workers take 73.6 s each**, which
is 0.38× the serial throughput — a sync would be **2.6× slower**, not faster. N = 3 and N = 4 did
not finish inside a 400 s cap.

That is not oversubscription. Two workers × 4 pinned threads is exactly the 8 cores available,
`vmstat` reports `st = 0` (the hypervisor is not stealing CPU), and the guest has one NUMA node.
The scarce resource is memory bandwidth — batch-1 decode streams the whole 1.28 GiB weight set per
token — and on a KVM guest that bandwidth is shared with co-tenants, which steal accounting does not
show.

For contrast, a 12-core M-series dev box saturates at 1.77× with 3 workers and gains nothing past
that. So the range across two real hosts is **0.38× to 1.77×** — one of them worse than doing
nothing.

**Rule: do not raise `MISAKA_PALW_CONCURRENCY` on a host you have not measured.** Assume a new host
behaves like `misaka-ibm` until shown otherwise. `CPU_THREADS = 4` is pinned by the determinism
class, so trading worker count against threads per worker is not available either. Reproducer:
`cargo test -p kaspa-pow --release --test palw_agent_concurrency -- --ignored --nocapture`.

**The knob is per PROCESS, so co-locating two PALW nodes bypasses it.** Two node processes are two
concurrent inferences whatever the setting says — the harmful configuration, entered without anyone
touching the knob. Measured live on `misaka-ibm`, which runs two testnet-11 nodes: the syncing
node's per-header time has its mode at 10–15 s (its uncontended cost) but a median of 19.8 s, a mean
of 29.1 s and a **maximum of 303 s**; the node it competes with sits at a 35 s median. Separating
them is worth roughly **+60 % sync rate** (163 → ~259 headers/hour).

### `MISAKA_PALW_LEASE_DIR=<dir>` — make the bound cover the host

Set it to **one directory shared by every PALW process on the machine** and the concurrency bound
stops being per-process: a permit then also requires an exclusive `flock` on one of
`MISAKA_PALW_CONCURRENCY` slot files there.

```bash
MISAKA_PALW_AGENT=1 MISAKA_PALW_CONCURRENCY=1 MISAKA_PALW_LEASE_DIR=/var/lock/misaka-palw ./kaspad …
```

* Use it whenever a host runs more than one PALW node, or a node alongside any other PALW consumer.
  With `CONCURRENCY=1` and a shared lease dir, two co-located nodes take turns instead of halving
  each other's throughput.
* `flock`, not a PID file or a named semaphore, because **the kernel releases it when the holder
  dies** — a crashed node must not permanently consume a slot.
* Waiting for a slot is unbounded, exactly as waiting on the in-process semaphore already is. A wait
  past a minute logs which directory is full.
* The directory must be writable by every PALW process (they may run as different users). A
  directory that cannot be used logs a warning **once** and leaves only the per-process bound — a
  performance control failing open, deliberately, because refusing to validate over a lock directory
  would wedge the node.
* Unix only. Elsewhere the variable is reported as unsupported and ignored.

### Watching the margin: `scripts/misaka-palw-headroom.sh`

```bash
bash scripts/misaka-palw-headroom.sh /root/.palw-soak      # read-only, ~2 s
# hdr_p50=27.6s spb=92s headroom=3.3x workers=3 CO-LOCATED
```

`headroom` is `block interval ÷ per-header verification cost` — the margin described above.
`hdr_p50` is the median of the node's own `validate(A,parallelizable)`, which on a PALW network *is*
the inference. `HEADROOM-LOW` fires under 2× **and when the margin could not be measured at all**,
because a missing number is not reassurance. `CO-LOCATED` fires when more than one PALW worker is
seen on the host during a short sample — its firing means co-location, but its silence does not
prove isolation (workers are short-lived on the one-shot path).

`misaka-palw-soak-status.sh` calls this script, so the two cannot drift apart. Measured on
`misaka-ibm` 2026-08-18: the soak node reads `3.3×` and the syncing node `5.4×`, the gap between
them being exactly the contention the lease directory is there to remove.

### The margin to watch: sync rate vs chain growth

The number that decides whether a PALW network can admit new nodes at all is
`headers synced per hour ÷ blocks produced per hour`. On `misaka-ibm` / testnet-11, measured over one
7 h 34 m sync: **163 headers/hour against ~40 blocks/hour ≈ 4×** (≈ 6.5× uncontended).

Below 1× a node can never finish syncing and the network is closed to newcomers. The margin shrinks
if the block interval shortens, the model gets slower, the host gets busier, or a second PALW node is
co-located. Track it per host; it is a better alarm than any absolute hour count.

## Per-host prerequisites — the Ollama runtime (user decision 2026-08-11) — ⚠️ SUPERSEDED

> Kept for the record only. `algo_id = 5` is disabled (STATUS banner), so **none of this is a
> consensus requirement**: a node on the current preset validates with no Ollama server, no model
> pin and no calibration. For the replacement worker's requirements see
> §"Determinism class of the v2 worker" above. Do not provision a new host from this section.

Every **validating** node replays one inference per header against a **host-local Ollama server**
running the pinned Qwen model — the runtime an Ubuntu VPS fleet actually operates (the
Metal-pinned worker stays devnet's algo-4 runtime). Per host:

1. **Provision with `scripts/misaka-palw-ollama-setup.sh`** (run ON the VPS):
   installs Ollama (systemd unit, 127.0.0.1:11434), pulls the model, prints the **model digest**
   and the **calibration line**. Every fleet host must print the same digest, and hosts of the
   same architecture must print identical calibration lines — different ⇒ STOP (version/blob
   skew; an executor would refute honest verifiers).
2. **The miner must run in the VERIFIERS' class — a GPU miner cannot feed a CPU-verifying fleet.**
   Measured 2026-08-11 on one host, one model blob, one Ollama build: the Metal (GPU) backend and
   the CPU backend produced **different greedy continuations** for the identical canonical PoW
   request. Different continuation ⇒ different tag ⇒ the block a GPU miner solves fails PoW for
   every CPU verifier, and vice versa. This is not a tuning knob; it is what "the inference IS the
   proof" means.
   The protocol therefore pins `num_gpu = 0` (`POW_L1_PALW_OLLAMA_NUM_GPU_V1`): every host — miner
   and validator, GPU-equipped or not — computes on the CPU backend, so the GPU/CPU dimension
   cannot split the network. Mining cannot exploit a GPU on this network, deliberately (the same
   trade the VLT portable CPU profile makes).
   Thread count is safe to leave host-chosen: measured invariant across `num_thread` 1/4/8.
3. **Determinism class = (Ollama version, model digest, architecture).** Greedy decoding is
   reproducible within one class; ACROSS architectures (NEON vs AVX2 reduction order) it is not
   promised — the same arch-scoping the VLT CPU compute class documents. The public chain is
   therefore mined AND validated by the fleet's class (x86-64 Ubuntu). An arm64 machine (the
   M4 Pro dev box) forms its own class: fine for local E2E, not a validator of the public chain
   unless its calibration line happens to match — verify, never assume.
4. **The model blob is PINNED IN CONSENSUS** (`POW_L1_PALW_OLLAMA_MODEL_DIGEST_V1` =
   `324d162be6ca…`, 2_741_192_820 bytes). kaspad verifies it against `GET /api/tags` at startup
   and the tag runner re-checks once per process, so a host serving a different blob **refuses to
   start** instead of silently forking (it would otherwise reject every honest block and have its
   own rejected). Re-pulling a model that upstream has re-published under the same tag changes the
   digest — if that happens, the pin (and the network) must be updated deliberately, not silently.
5. **Environment for kaspad AND misaminer**:
   ```
   export MISAKA_PALW_OLLAMA_MODEL=misaka-palw-2b-f16   # the PINNED F16 class model (see the table)
   # export MISAKA_PALW_OLLAMA_URL=http://127.0.0.1:11434   # default; set only if changed
   ```
   kaspad checks at startup on PALW networks that the model env is set and the server is
   reachable, and exits with instructions otherwise — no first-header panic.
   `MISAKA_PALW_POW_FIXTURE=1` is refused outside devnet.
6. **Tag semantics under Ollama** (ADR-0021 addendum): the API exposes no per-decode logits, so
   the algo-5 tag commits to the greedy response bytes + token counts — weaker binding than the
   worker's `gemm_trace_root`, still model-work-priced. Devnet keeps the stronger algo-4 worker
   tag; a future Ollama fork exposing logits (or a logits-serving shim) can restore full-trace
   binding as algo 6.

## Rollout order (single coordinated window — this is a flag day AND a re-genesis)

0. **The plan is decided (2026-08-12): Plan B, with Plan A folded into genesis.** Both used to be
   live options on this branch — Plan A (`docs/testnet10-shadow-release-DEPLOY.md`) was the VLT
   shadow release scheduled at DAA 30_200_000 on the OLD 1-bps chain, Plan B is this re-genesis.
   They were mutually exclusive because A's fence is a height on a chain B throws away: carried
   over unchanged it would have landed ~115 years past the new genesis, leaving the overlay AND
   the bond spend gate (a 2026-08-11 audit P0) silently inert forever.
   The resolution is better than either: `TESTNET_VLT_SHADOW_FORK_DAA_SCORE = 0`, so step 3 ships
   **in the genesis rules**. A rebuilt chain is the one situation where a fork needs no scheduling
   at all — no height to miss, no fleet to update ahead of it, and no window where challenge
   slashing is on while the spend gate that protects the collateral is off.
   What this does NOT include is step 4: `vlt_activation_daa_score` stays `u64::MAX`, so compute
   weight still does not decide finality. The overlay credits, draws committees, pays the audit
   fee and slashes settled challenges from block 1 — that is what "shadow" means, and the soak it
   is meant to provide now starts at block 1 instead of after a flag day.
1. Build this branch on every host: `cargo build --release -p kaspad --features evm -p misaminer`.
   The `--features evm` requirement is unchanged from the current fleet. Cut the release from a
   clean checkout of the release commit (a tree another session is editing poisons the build —
   the 8208cd6 lesson), into a private target dir.
2. Rehearse locally first (any host with Ollama + the model pulled):
   ```
   NET=testnet-10 IBD=1 MISAKA_PALW_OLLAMA_MODEL=misaka-palw-2b-f16 BLOCKS=6 \
     bash scripts/misaka-palw-pow-e2e.sh
   ```
   PASS = mine → independent replay validation → fail-fast probe → fresh-node from-genesis IBD.
3. Stop the old fleet (nodes, miners, seeder daemons). Back up, then remove the old datadirs —
   the genesis-mismatch guard would refuse them anyway.
4. Start kaspad on the seeder-backed hosts first (with the env), then the seeder daemons from
   this build, then the remaining nodes.
5. Start the miner(s) (same `MISAKA_PALW_OLLAMA_MODEL` env; one inference per nonce against the
   local Ollama): the first template on a fresh chain reports `is_synced=false`, so the cold
   start needs the explicit consent flag, exactly like devnet bootstrap:
   ```
   misaminer --rpc=127.0.0.1:26210 --network-id=testnet-10 --wallet=<payout> --mine-when-not-synced
   ```
   Drop `--mine-when-not-synced` once the chain is moving (F3: it must never run in normal
   operation). Expect blocks much faster than 120 s for the first ~150 (the genesis target is
   the easiest representable and difficulty is fixed until the min window fills), then the DAA
   walks onto the 120 s cadence.
6. Verify the public path from a machine that is NOT in the fleet: start a kaspad with only the
   env set (no `--addpeer`) and watch it discover via DNS, IBD from genesis (one inference per
   header — ~1 s/header hot), and report the fleet's sink. That is the full public flow.

## Known limits (verified extent of the implementation)

* **Verified**: solo mining (`misaminer`), independent per-header replay validation by peers, the
  fail-fast rails, and **from-genesis headers-first IBD** — all exercised end-to-end on t10 params
  with the real model.
* **Full multi-machine chain: MEASURED (2026-08-12) at the final T = 120 s / N = 16.** Three
  hosts, real CPU inference: h1 mined 10 blocks, h2 and h3 independently replay-validated all 10
  (identical accepted-block sets, identical tip `1c8be6cac21e9a2c`, zero bad-PoW/ban events), and
  a FRESH node on h3 did a from-genesis IBD over the network — 10 blocks, 10 UTXO-validated, 0
  bad events. This is the first run where mining, validation and IBD each happened on a
  *different physical machine*.
* **Cross-machine agreement: MEASURED (2026-08-12), and it is what forced the F16 profile.**
  8-seed canonical probes across five surfaces — M4 Pro Metal, M4 Pro CPU (arm64), AMD EPYC ×2
  and Intel Broadwell (x86-64, one without f16c):
  - registry Q8_0 blob: EPYC ≡ EPYC but ≠ Broadwell on 4/8 seeds, and ≠ Metal, ≠ arm64-CPU —
    unusable as a class (the single-seed probe that once "passed" was luck).
  - F16 profile: **x86-64 8/8 across vendors** (the fleet class this release pins), and Metal ≡
    arm64-CPU ≡ x86 on 7/8 — one seed still flips arm64-vs-x86 in the batched prefill GEMM, so
    **arm64 is NOT in the class** (7/8 is a fork, not a pass). NVIDIA is unmeasured here.
  - ⚠️ **RETRACTED 2026-08-13 — the F16 half of this bullet is void, and the way it is void is the
    whole lesson.** This campaign measured HOST AGREEMENT and never measured SEED DIVERSITY. At the
    shipped `num_predict = 16` the F16 profile emits ONE constant continuation for every seed
    (27/27, 22 uniform random), so host agreement is guaranteed a priori and certifies nothing
    about anyone's arithmetic. Note what that implies about the selection above: the Q8_0 numbers
    are real — its outputs genuinely varied by host, which is what a sensitive quantity looks like
    — and F16 was chosen *because* it made that variance disappear. The variance did not disappear
    because the arithmetic became portable; it disappeared because the output stopped depending on
    the input. The cure and the defect are the same phenomenon, and the same collapse is what made
    the PoW forgeable (see the STATUS banner). The campaign also ran at `num_predict = 48`, where
    the continuation is already partly collapsed (2 distinct over 3 seeds measured), and was never
    re-run at 16. Superseded by the v2 full-logits measurement in §"Determinism class of the v2
    worker": one binary, 4/4 x86_64 hosts, 4/4 jobs byte-identical, with a negative control
    (arm64 vs x86 correctly reports BUILD-MISMATCH) proving the verdict discriminates.
  - Bringing Mac/NVIDIA into one class is exactly what the Qwen3.6-35B PALW runtime's patched
    llama.cpp (serial n_batch=1 execution policy, fp32 accumulation) exists for; porting that
    runtime to the 2B worker is the follow-up phase. Stock Ollama cannot express it
    (`num_batch=1` asserts in its server).
* **NOT exercised: pruning-proof IBD.** Once the chain passes the pruning depth (10_800 blocks ≈
  30 h), a new node syncs via a pruning proof instead, and `calc_block_level_check_pow_layer0`
  runs **one inference per proof header** (`pruning_proof_m = 1000` per level). That path has not
  been measured and is the first thing to test on the live chain before it is 30 h old — it is
  also the strongest argument for the sampled-audit tier / trusted-checkpoint IBD in ADR-0021's
  consequences.
* **Pooled mining is not available.** The stratum bridge (`bridge/`) validates shares with the
  legacy 256-bit `check_pow`, which predates the Layer-0 PoW entirely — it is not PALW-capable
  (and was already not algo-3-capable). Solo `misaminer` is the supported miner.
* **`pq-miner` refuses PALW templates** with a pointer to `misaminer`: its all-nonce rayon scan
  would grind a stale template forever behind the runtime's serialization gate.

## Operating notes

* **IBD cost grows with the chain**: a day of chain is 720 headers ≈ 2.5-5 h of replay on one
  host (the 120 s interval cut this 12× versus the earlier 10 s plan). Before the chain is months old, ship either the Open-then-Audit sampled-audit tier or
  trusted-checkpoint IBD (ADR-0021 consequences).
* **Miners**: one attempt = one `/api/generate` call; Ollama keeps the model RESIDENT between
  calls (its keep_alive default), so there is no per-attempt model-load cost. Measured on an
  M4 Pro CPU backend: ~2-4.5 s per attempt; a VPS core will be slower (~5-15 s). At the genesis
  max target (p ≈ ½ per attempt) one miner therefore lands a block every ~10-30 s — **the 10 s
  cadence needs 2-3 miners**, and until it does the difficulty stays pinned at the floor (the DAA
  cannot make a max target easier). On a testnet that is acceptable; it does mean PoW contributes
  no difficulty margin at the floor and the VLT overlay is the real finality.
* **VLT**: bonding floors are unchanged (10 KAS); epochs are 4 min of wall clock (2 blocks —
  the floor, see the DnsParams table), and the 14-day evidence/unbonding windows are the same
  14 days they always were.
* **Rollback**: the old chain's datadir backups + the previous build restore the pre-PALW
  testnet exactly (its fingerprint pins are in git history); the two chains can never confuse
  each other (distinct genesis + fingerprint + the "-palw" marker).
