# ADR-0041: PALW pruning-proof verification — exhaustive and amortised, not sampled

Status: **Proposed.** Activates nothing on a shipped preset. Governs how a node validates a
pruning-point proof and a trusted set on a network whose PoW is a PALW inference
(`pow_palw_activation` / `pow_palw_ollama_activation` active — testnet-11 and devnet today).

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

### Decision 2 — Parallelise verification during IBD

The `SPAWN_GATE` mutex (`palw.rs:186`) serialises every inference in the process, which is correct
for steady-state block validation (one at a time, low load) but is the wrong policy for a sync
burst. During pruning-proof verification the validator may run up to a bounded number of workers
concurrently. Measured: 8-way concurrency turns the sampled 64-inference cost from ~13 min to
~1.6 min. The bound is a config constant, not consensus; it does not change what is accepted, only
how fast.

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

### Decision 4 — Cheap checks gate the inference

Every check in the per-header loop that does **not** consume the PoW result runs first, so a header
that fails a shape, level, or ancestry rule never buys an inference. This extends the P0-3
ordering fix (already applied to the ordinary header path at `header_processor/processor.rs:318`)
to the two proof paths — `validate.rs`'s loop and `apply.rs`'s trusted-set loop — which the P0-3
comment did not cover, and which a reader of that comment should not assume are covered.

## Where this leaves the numbers, honestly

Exhaustive verification stays. The honest one-year testnet-11 proof is ~9,000 headers, and the three
landing decisions compose multiplicatively rather than changing the exponent:

| | per header | 9,000 headers |
|---|---|---|
| today | ~12 s | 30.0 h |
| + persistent agent (Decision 1′, ~5×) | ~2.4 s | 6.0 h |
| + 8-way concurrency (Decision 2) | — | **~45 min** |

45 minutes for a one-time sync of a year of history is a different proposition from 30 hours, and it
is reached without weakening any security argument. It is still `O(m·log(history))` inferences, so it
grows with history logarithmically and with the model's cost linearly — if the model gets slower,
this gets worse, and the mitigation is the agent's throughput, not the protocol.

The interim cap (Decision 3) is orthogonal to all of this: it bounds the ATTACKER's amplification,
not the honest cost. It landed first because it has zero liveness risk.

## Consequences

* IBD on a PALW network goes from ~30 h to ~45 min once Decisions 1′ and 2 land, with NO change to
  what is accepted — every header is still verified in full.
* No consensus rule changes; no preset changes; the fingerprint does not move. Every change here is
  a local sync policy.
* Nothing here rests on proof acceptance being a local decision, because nothing here samples. That
  property was load-bearing for the withdrawn Decision 1 and is now merely true.
* The remaining exposure is throughput, not soundness: a node whose verification agent is slow syncs
  slowly. That is a operational property with a visible symptom, unlike a false-accept probability,
  which is why exhaustive-and-amortised is preferred over sampled-and-fast.
