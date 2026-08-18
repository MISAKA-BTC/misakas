# ADR-0041: PALW pruning-proof verification — exhaustive and amortised, not sampled

Status: **Landed.** Activates nothing on a shipped preset. Decision 1′ (the resident verification
agent, opt-in behind `MISAKA_PALW_AGENT=1`) and Decision 2 (bounded concurrency, `MISAKA_PALW_CONCURRENCY`,
default 1) are landed and measured; Decision 3 (the header cap) and Decision 4 (cheap checks before
the inference) were already landed. Decision 1 (sampling) is withdrawn as unsound.
Governs how a node validates a pruning-point proof and a trusted set on a network whose PoW is a
PALW inference (`pow_palw_activation` / `pow_palw_ollama_activation` active — testnet-11 and devnet
today).

Date: 2026-08-18
Relates to: ADR-0007/0008 (the Layer-0 512-bit PoW the proof headers carry), ADR-0038/0039 (PALW
is the consensus work; there is no hash lane), the pruning-proof machinery in
`consensus/src/processes/pruning_proof/` (`build.rs`, `validate.rs`, `apply.rs`, `mod.rs`), the
Layer-1 finalizer `consensus/pow/src/palw.rs`, and the 2026-08-18 PALW audit finding H1.

## Context — one inference per proof header does not scale, and the block interval cannot fix it

A pruning-point proof lets a fresh node accept the current pruning point without replaying history.
It carries, per block level, the headers in `future(root) ∩ past(tip)` for a root at blue-depth
`≈ 2·pruning_proof_m` below the level tip (`build.rs:353`, `validate.rs:291`). The validator checks
each header's PoW: `calc_block_level_check_pow_layer0` at `validate.rs:205`, and again per
trusted-set block at `apply.rs:69`.

On a hash network that check is microseconds. On a PALW network it is **one full LLM inference** —
`StateLayer0::check_pow_layer0` → algo 4 → `native::tag_for_seed`, which spawns `palw-worker
--mode verify` (`palw.rs:257-260`). Every such call is serialized process-globally on a single
`SPAWN_GATE` mutex (`palw.rs:186`), and each spawn re-reads and SHA-256s the whole 1.28 GiB pinned
GGUF. Measured on the fleet: ~12 s per header on an EPYC core, ~26 s on the slowest host.

Two consequences, one for honesty and one for safety.

**Honest cost.** The proof's own size formula is `estimate_proof_unique_size ≈ (⌊log₂(history/m)⌋ +
1)·m` (`mod.rs:283`). At testnet-11 (`m = 1000`, ~262,800 blocks/year at 120 s) an honest
one-year proof is ≈ 9,000 distinct headers. At 12 s serialized that is **30 hours** of stalled
IBD; 33 hours on the slowest host. The tag cache does not save this: it is keyed by seed, most
proof headers have distinct seeds, and it clears wholesale at `TAG_CACHE_MAX = 8_192`
(`palw.rs:178, 211`), below the honest distinct-header count — so it thrashes rather than
deduplicates.

**The block interval is the wrong lever, and lengthening it makes two other things worse.** Proof
size is `Θ(m·log(history))` — logarithmic in history length, which the interval scales *linearly*.
Computed from the code's own formula:

| interval | 1-year blocks | proof headers | verify @12 s |
|---|---|---|---|
| 120 s | 262,800 | ~9,000 | 30.0 h |
| 300 s | 105,120 | ~7,000 | 23.3 h |
| 600 s | 52,560 | ~6,000 | 20.0 h |
| 3600 s | 8,760 | ~4,000 | 13.3 h |

Five minutes buys a **22 %** reduction, not a fix; halving the proof would take squaring the
interval. And lengthening the interval shrinks the pruning horizon (`pruning_depth` 1,144 → 892 at
120 s → 300 s), which tightens the per-class DAA fold's memory bound (ADR-0038/this branch) and
changes finality depth — costs paid network-wide to remove 7 hours from a one-time sync. The
interval is the steady-state load lever (one 12 s inference per 120 s block ≈ 10 %); it is not the
burst lever.

**Safety cost (audit H1).** Nothing caps the *number* of headers. The pre-loop checks are on the
level count only (`validate.rs:104`), the wire limit is `P2P_MAX_MESSAGE_SIZE = 1 GiB`, and header
PoW is public — so a peer can replay the network's own historical headers into a level and buy the
victim tens to hundreds of hours of serialized inference for a few MB of bandwidth. Header PoW is
public, so "the padding must be valid PoW" is not a barrier: the network's real headers are valid.

## The decision

**Pruning-proof PoW verification stays EXHAUSTIVE.** The cost is removed by amortising the
per-header overhead and by parallelism, not by checking fewer headers — sampling was drafted here and
withdrawn as unsound (below). Four decisions, in order of how much each changes the numbers.

### Decision 1 — WITHDRAWN: sampling the PoW checks is UNSOUND in this proof structure

The first draft of this ADR proposed sampling the per-header PoW check, on the argument that a
prover must corrupt a large fraction `f` of headers to move the decision, so a sample of 64 detects
it with probability `1 − (1 − f)^64`. **Settling the scheme's constants disproved the argument.**
Recorded here rather than deleted, because the reasoning is the kind a reader would re-derive.

A proof's weight is not a header count. `compare_proofs_inner` compares `blue_work_diff`, read from
the RECOMPUTED ghostdag (`validate.rs:84-89`), and ghostdag accumulates

```
blue_work = Σ over mergeset blues of  max( calc_work(header.bits), level_work(level) )
```

(`ghostdag/protocol.rs:155-161`). Both terms are prover-chosen and both are validated by the PoW and
by nothing else:

* `header.bits` is peer-supplied. Its only validator is `pow_passes` at `validate.rs:250` — the PoW
  value must meet the declared target. Skip that check and a header may declare an arbitrarily hard
  `bits`, hence arbitrary `calc_work`.
* the LEVEL a header sits at is the prover's choice of which array it appears in.
  `level_work(level, max_block_level) = 1 << (level + 256 − max_block_level)`
  (`difficulty.rs:273-281`) is a function of that position, not of the header. Its only validator is
  `header_level < level → reject` at `validate.rs:247`, and `header_level` comes from the PoW.

So **one** unverified header can claim unbounded work. The detection probability is not
`1 − (1 − f)^s` with a large `f`; it is `s/N` for a single planted header — 64/9,000 ≈ **0.7 %**. The
draft's table was answering a question the attacker does not have to ask.

The two obvious repairs both fail:

* *Credit an unsampled header only `level_work`* — no: the level is exactly what is unvalidated, so
  the prover places the garbage header at level 250 and collects `level_work(250)`.
* *Credit an unsampled header zero* — no: the DEFENDER's proof is the node's own and is trusted in
  full (`ProofContext::from_proof(self, &defender_proof, false).expect("local")`), so measuring only
  the challenger at `s/N` of its work means an honest heavier chain can never win. That is not a
  weakened security bound, it is a broken IBD.

Sampling would become available only if the proof carried a commitment binding each header's claimed
work to its level independently of the PoW — a change to the proof format and to the MLS argument,
which is not this ADR's scope. **Verification stays exhaustive.**

### Decision 1′ — Amortise the per-header cost (this is the real lever)

The 12 s is not 12 s of inference. Each tag spawns a fresh `palw-worker` process which re-reads and
SHA-256s the whole 1,280,835,840-byte pinned GGUF and reloads the model, then runs an inference the
module itself quotes at ~1–3 s. The worker's own source already names the fix and the reason it was
deferred rather than dismissed: *"Costs a full read of the 1.2 GB file per job process; the
persistent agent (P1) amortizes it, correctness does not wait for it"* (`misaka-palw-worker/src/
main.rs:419`).

A persistent verification agent — one process, model resident, pin checked once at startup, seeds
fed over a framed protocol — removes the SHA-256, the model load and the spawn from the per-header
cost, leaving the inference. That is a constant-factor win of roughly 4–6× at the fleet's measured
numbers, and it costs NO security argument: the tag is a pure function of the seed, every header is
still verified in full, and the artifact pin is still the full read (once, not per header).

The pin's integrity is what makes this safe to amortise and must not be weakened in the process: the
audit trail records a v1 `(path, size, mtime)` cache that let any same-sized model pass the pin, on
the consensus PoW path (`main.rs:422-431`). The persistent agent re-reads and re-digests at startup
and holds the model immutably for its lifetime; it must not reopen the path, and a model file that
changes underneath it must be a hard failure, not a re-read.

#### Implementation 2026-08-18: LANDED and measured

Measured on this machine with the real pinned model
(`Qwen3.5-2B-Q4_K_M.gguf`, `--n-predict 16`), and the numbers settle the design question:

| | measured |
|---|---|
| one-shot `--mode verify`: model load | **7.44 s** |
| one-shot: the inference itself | **0.18 s** |
| resident agent: model load (warm page cache) | 0.28 s |
| resident agent: the inference itself | 0.16 s |

**Overhead is ~97 % of a one-shot verification.** Decision 1′ is not a marginal optimisation; the
inference is a rounding error next to the artifact read and the model load.

The equivalence the whole design rests on was also **proved empirically, not argued**: a resident
agent holding the model produced a projection document for its first job that is **byte-identical to
the one-shot document, in every field** — including all five the PoW tag is derived from
(`output_commitment`, `gemm_trace_root`, `operation_schedule_commitment`, `prefill_tokens`,
`decode_tokens`). A resident model computes the same tag.

**The blocker was self-inflicted and is now resolved.** Running a second job on one context failed —
`shim_ctx` read damaged afterwards (`lctx` = `0x8078` or `0x0`, `n_vocab` = 0). The cause was one
line: production `execute` closes the context it opens, and the extraction that produced a
context-taking `execute_on_context` took the close with it. Every job freed its CALLER'S context and
the next job read freed memory. Fixed by giving the close to the owner: `execute` opens, delegates
and closes; `execute_on_context` never closes.

Two hypotheses were raised before that and both were disproved by measurement. They are recorded
because each looked convincing and each would have sent an implementer somewhere useless:

* *"`execute` corrupts the heap."* No. Two independent `execute()` calls in one process are clean; a
  probe reading `shim_n_vocab` around every shim call of one job showed the struct intact throughout
  (`n_vocab = 248320`); Guard Malloc over the unmodified one-shot found no overflow.
* *"It is the Metal backend's resource lifecycle"* — suggested by an exit-time
  `ggml-metal-device.m:657: GGML_ASSERT([rsets->data count] == 0)`. No: a CPU-only llama tree was
  configured and built out of tree (`-DGGML_METAL=OFF -DGGML_BLAS=OFF -DBUILD_SHARED_LIBS=OFF`,
  linked via `MISAKA_PALW_CPU=1`, leaving the pinned Metal build untouched) and the same failure
  occurred with no Metal device in the process. That assertion was the probe leaking a context — a
  second symptom of the same self-inflicted bug.

**The equivalence is reproduced, not claimed.** Three jobs on ONE context with
`shim_reset_context` between them (backend synchronize, then `llama_memory_clear`) produce
projections byte-identical to each other AND to a fresh one-shot process, across all five fields the
PoW tag derives from. The one-shot's own output is unchanged by the refactor, byte for byte.

**Both halves are now landed.** `palw-worker --mode pow-agent` holds the model and serves jobs over
newline-delimited JSON; `kaspa-pow`'s `native::resident` is the validator-side child that drives it.
Measured on this machine, 12 distinct seeds, `--n-predict 128`:

| | per seed (median) | 12 seeds |
|---|---|---|
| one-shot, a process each | 3.28 s | 39.2 s |
| resident agent | **0.57 s** | 9.6 s (2.70 s of it the one model load) |

**5.7× per seed** — the projected ~5× constant factor, confirmed rather than assumed. Note this
machine's page cache is warm, so its one-shot baseline (3.28 s) is well under the fleet's 12 s; the
fleet number is the one that should be re-measured there, and the ratio is what transfers.

Four design points, each of which is a way this could have been got wrong:

* **One reader.** The agent returns the same projection document the one-shot prints, and both are
  read by one function (`native::tag_from_doc`). A tag that depended on which transport delivered
  the document would be a consensus bug; the way not to have one is not to have a second parser.
* **Marked lines, not length-prefixed frames.** llama.cpp and ggml are third-party code sharing the
  child's stdout. One stray byte desynchronises a length-prefixed stream silently and permanently;
  with a marker, noise is skipped instead. The v2 compute path can use Borsh frames because its
  worker exits after one job — a resident one cannot.
* **The agent is an accelerator, never an authority.** It runs only under `MISAKA_PALW_AGENT=1`, and
  every failure — spawn, handshake, timeout, a frame out of order, a child that died — falls back to
  the one-shot path that ships today. So it can change how fast a tag arrives and not which tag, and
  it cannot wedge a node the old path would have synced. A test pins the sharpest case: with the
  agent enabled and the worker binary missing, the caller still gets the same `PalwUnavailable`,
  naming the same path.
* **A dead child costs a delay, not a tag.** A resident process can be OOM-killed between two seeds,
  and the validator finds out by writing to a pipe whose far end is gone — one `SIGPIPE` away from
  killing the node instead of returning an error. Tested by killing the agent mid-run: the next seed
  still produces the right tag.

Two measured facts worth recording because both are counter-intuitive:

* **The per-seed entropy is `gemm_trace_root`, not the generated text.** Across distinct seeds the
  greedy OUTPUT is the same generic continuation, so `output_commitment`, `decode_tokens` and the
  schedule commitment are all constant; the tag varies because the digest of the full logits of
  every decode call varies. Anyone testing this path who asserts distinctness on the output will
  write a vacuous test — the first version of ours was exactly that.
* **The overhead is not the inference.** At 0.57 s served versus 3.28 s spawned, ~83 % of a one-shot
  verification on a WARM machine is still setup. The 97 % figure above was the cold case.

What remains of the 30 h → 45 min claim is Decision 2 (concurrency), which is deliberately untouched
here: the agent runs inside the existing `SPAWN_GATE`, so exactly one inference is in flight either
way and the two paths above were measured under the same policy.

Three suspects were eliminated along the way and are recorded so nobody re-investigates them: the
worker's own heap handling, a buffer overflow, and the Metal backend.

### Decision 2 — Parallelise verification during IBD — **landed, and it buys far less than drafted**

The `SPAWN_GATE` mutex serialised every inference in the process, which is right for steady-state
block validation (one at a time, low load) and wrong for a sync burst. It is now a counting
semaphore whose permit count is `MISAKA_PALW_CONCURRENCY` (default **1**, i.e. exactly the old
behaviour), the resident agent is a pool that grows to that count, and the two proof loops feed it:

* `validate.rs` computes each level's header PoW in **bounded batches**. The walk stays strictly
  sequential — every line after the PoW checks mutates stores whose order *is* the validation — and
  errors stay in header order, so nothing about what is accepted changes. The batch is bounded, not
  whole-level, because a PoW-derived error can stop the walk: up to `batch − 1` inferences may be
  spent on headers the walk never reaches, and that waste must be a constant rather than a level.
* `apply.rs` batches both of its sites (the trusted set, and the level recompute for every distinct
  proof header). Neither loop has an early exit once its shape gate has passed, so there is **no**
  speculation there at all — the same work, in less wall clock. Hoisting the gate out of the walk
  also means a proof that fails it now aborts having written nothing to the headers store, where
  before it had written every header up to the bad one.

**The drafted "8-way concurrency ≈ 8×" is not supported by measurement.** Throughput of N resident
agents, 6 jobs each, on a 12-core M-series host (`CPU_THREADS = 4`, so N workers occupy 4N threads):

| agents | threads | jobs/s | speedup | per-job latency |
|---|---|---|---|---|
| 1 | 4/12 | 1.17 | 1.00× | 0.86 s |
| 2 | 8/12 | 1.62 | 1.39× | 1.24 s (1.44×) |
| 3 | 12/12 | 2.06 | **1.77×** | 1.46 s (1.70×) |
| 4 | 16/12 | 2.07 | 1.77× | 1.93 s (2.26×) |
| 6 | 24/12 | 2.07 | 1.77× | 2.90 s (3.38×) |

Throughput **saturates at 1.77×** and adding agents past that buys exactly nothing — beyond 3, every
extra agent only lengthens everyone's latency in proportion.

The binding constraint is **not cores**, and the table says so: at 2 agents only 8 of 12 cores are
busy, yet per-job latency is already 1.44× the solo figure. If cores were the limit, two agents
would not slow each other at all. What is shared and saturated is memory bandwidth — batch-1 decode
streams the entire ~1.28 GiB weight set per token, so concurrent models compete for the memory bus
rather than for arithmetic. (Consistent with the numbers and with how batch-1 inference is known to
behave; not separately confirmed with a bandwidth counter.)

Two consequences worth stating plainly:

* **`CPU_THREADS = 4` is pinned by the determinism class**, so a host cannot trade worker count
  against threads per worker to dodge this — the shape string is part of the runtime identity.
* **The fleet number is not this number.** A server host with 8–12 memory channels has far more
  aggregate bandwidth than a unified-memory laptop, so it may scale further, and it may not. The
  honest instruction is to measure `MISAKA_PALW_CONCURRENCY` on the host that will sync, with the
  test that produced the table above, rather than to inherit either 8× or 1.77×.

### Decision 3 — Hard header cap, enforced before any inference

Independent of sampling, the validator caps the total header slots it will consider, derived from
its OWN params — never from the proof's claimed `daa_score`. This closes the amplification (audit
H1): a proof larger than any honest builder could produce is rejected before a single worker is
spawned. The cap is generous by design — its job is to bound the pathological case (a 1 GiB message
of junk), not to be tight; tightness is what sampling provides. The interim cap this ADR ships is
`(max_block_level + 1) · 2 · pruning_proof_m` total header slots, the builder's own per-level
working-set capacity (`build.rs:197, 366`) times the level count. A proof exceeding it is refused
as oversized, not run.

The same discipline applies to the trusted set (`apply.rs`): its length is capped before the loop
that runs one inference per block.

### Decision 4 — Cheap checks gate the inference — **already landed**

Every check in the per-header loop that does **not** consume the PoW result runs first, so a header
that fails a shape, level, or ancestry rule never buys an inference. This extends the P0-3 ordering
fix (already applied to the ordinary header path at `header_processor/processor.rs:318`) to the
proof paths — `validate.rs`'s loop and both of `apply.rs`'s.

Both were already in this shape when this ADR's implementation started, under the P0-1/P0-2 audit
work: `check_proof_header_shape` runs immediately before every `calc_block_level_*` call on those
paths. Nothing was needed here.

The batching in Decision 2 had to be built so as not to quietly undo it, which is the part worth
knowing: the batch scan applies the same gate and **stops at the first header that fails it**, so a
header the walk will reject is never pulled into a batch, and neither is anything behind it. A batch
that ignored the gate would have re-introduced exactly the amplification this decision removes.

## Where this leaves the numbers, honestly

Exhaustive verification stays. The honest one-year testnet-11 proof is ~9,000 headers, and the three
landing decisions compose multiplicatively rather than changing the exponent:

| | per header | 9,000 headers |
|---|---|---|
| today | ~12 s | 30.0 h |
| + persistent agent (Decision 1′ — landed, **5.7×** measured) | ~2.1 s | 5.3 h |
| + concurrency (Decision 2 — landed, **1.77×** measured) | ~1.2 s | **~3.0 h** |

Both factors are now measured rather than projected, and the second one is much smaller than this
ADR first assumed: the draft claimed 8× from 8-way concurrency and got 1.77×, because the resource
that saturates is memory bandwidth and not cores (Decision 2 above). **The headline is 30 h → ~3 h,
not 30 h → 45 min.** That is still a different proposition from 30 hours, and it is reached without
weakening any security argument — but it is an hours-scale sync, and anyone planning around 45
minutes should stop.

The 12 s is the fleet's per-header cost; the two factors were measured on a dev machine. Applying
them to the fleet's baseline is the honest composition available today, and re-measuring both on the
host that will actually sync is the remaining gap.

45 minutes for a one-time sync of a year of history is a different proposition from 30 hours, and it
is reached without weakening any security argument. It is still `O(m·log(history))` inferences, so it
grows with history logarithmically and with the model's cost linearly — if the model gets slower,
this gets worse, and the mitigation is the agent's throughput, not the protocol.

The interim cap (Decision 3) is orthogonal to all of this: it bounds the ATTACKER's amplification,
not the honest cost. It landed first because it has zero liveness risk.

## Consequences

* IBD on a PALW network goes from ~30 h to ~3 h with Decisions 1′ and 2 landed, with NO change to
  what is accepted — every header is still verified in full. The first draft of this ADR said 45
  minutes; that rested on an 8× from concurrency which measurement did not support.
* No consensus rule changes; no preset changes; the fingerprint does not move. Every change here is
  a local sync policy.
* Nothing here rests on proof acceptance being a local decision, because nothing here samples. That
  property was load-bearing for the withdrawn Decision 1 and is now merely true.
* The remaining exposure is throughput, not soundness: a node whose verification agent is slow syncs
  slowly. That is a operational property with a visible symptom, unlike a false-accept probability,
  which is why exhaustive-and-amortised is preferred over sampled-and-fast.
