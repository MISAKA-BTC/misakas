# testnet-10 PALW rollout runbook — the LLM-PoW re-genesis

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
masks the flag. Check `grep -o f16c /proc/cpuinfo` when provisioning: a host without it still
validates *correctly* (determinism is unaffected — that is why the fleet agrees 8/8) but carries
several times the load, and it is the host that sets the network's floor.

Consequences to plan for: 35B adoption additionally needs **≥ 32 GB RAM hosts** (its Q4_K_M blob
is 23.9 GB — the current 11/15/23 GB fleet cannot hold it), and at 120 s the DAA window (264
blocks) is 8.8 h of difficulty memory, so a hashrate change takes that long to be fully absorbed.

## Per-host prerequisites — the Ollama runtime (user decision 2026-08-11)

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
