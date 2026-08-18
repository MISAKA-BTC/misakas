# ADR-0041: PALW pruning-proof verification — sampled, not exhaustive

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

**Pruning-proof PoW verification is SAMPLED, not exhaustive**, backed by a hard header cap and by
parallelism. Three layers, in order of how much each changes the numbers.

### Decision 1 — Sample the PoW checks (this is the exponent-changer)

The validator does **not** run an inference on every proof header. It selects a random subset and
verifies only those. If any sampled header's PoW fails, the proof is rejected.

Two facts make this sound where it would be unsound for block validity:

* **Proof acceptance is a LOCAL sync decision, not a consensus rule.** Two nodes that accept the
  same pruning point having sampled *different* headers do not fork — `validate_pruning_proof_
  standalone` (`consensus/mod.rs:2005`) gates whether THIS node adopts the point, and a rejected
  proof means "ask another peer," not "this block is invalid." So each node may use its **own
  local randomness**, which a malicious prover cannot predict or grind. This is the property a
  hash chain never needed and PALW can use.
* **A prover must corrupt a LARGE fraction to move the decision.** The proof's weight comes from
  the claimed work of its headers; to make the validator adopt a false pruning point, a prover
  must fabricate enough headers to outweigh the honest chain, which is a large fraction `f` of the
  sampled population, not one header. Detection probability over a sample of size `s` is
  `1 − (1 − f)^s`:

  | sample `s` | `f = 5 %` | `f = 10 %` | `f = 25 %` | cost @12 s |
  |---|---|---|---|---|
  | 64 | 96.25 % | 99.88 % | ~100 % | 12.8 min |
  | 128 | 99.86 % | 99.9999 % | ~100 % | 25.6 min |

  64 samples turns 30 hours into ~13 minutes and detects any fabrication large enough to matter
  with probability > 96 %; 128 pushes the miss probability below `10⁻³` at 5 %.

**Not uniform: top levels in full, base levels sampled.** A header at level `L` claims `2^L` times
the work of a base header, so a fabricated high-level header is worth far more and the high levels
have far fewer headers (the proof is a pyramid — the top levels hold `O(m)` headers total). So the
scheme verifies **every header at the top levels in full** (cheap: few headers, and they carry the
most weight) and **samples the base levels** (many headers, each low-weight). The exact split — how
many top levels are exhaustive, the base sample size, and how the sample is drawn from the node's
CSPRNG seeded independently per sync attempt — is fixed in the implementation and pinned by test;
it must never read a prover-supplied value.

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

## What this ADR does NOT decide, and why the interim cap is not the whole fix

The interim cap (Decision 3) bounds the attacker to the honest envelope; it does **not** make
honest IBD fast, because the honest cost (30 h exhaustive) *is* the problem. Only Decision 1
(sampling) changes that, and Decision 1 is the larger change — it touches the validation flow, the
CSPRNG wiring, and the top-vs-base split — so it lands after the cap and the ordering, each with
its own regression test. Until sampling lands, a PALW network's pruning-proof IBD remains
exhaustive and slow but is no longer unboundedly amplifiable and no longer stalls a worker-less
node into banning its peers (the latter fixed separately for the header path in the same audit
cycle).

The sampling scheme's constants (top-level count, base sample size, concurrency bound) are
implementation decisions pinned by test, not consensus constants — because proof acceptance is
local, two nodes may hold different values without forking. A node that sets its sample too small
weakens only its OWN resistance to a false pruning point; it cannot make another node accept one.

## Consequences

* IBD on a PALW network becomes minutes, not tens of hours, once Decision 1 lands, with a stated
  and tunable false-accept probability.
* No consensus rule changes; no preset changes; the fingerprint does not move. Every change here is
  a local sync policy.
* The security argument rests on proof acceptance being local. If a future change ever made proof
  acceptance a consensus verdict (two nodes must agree on which headers were checked), sampling
  with per-node randomness would become unsound and this ADR would need revisiting — so that
  property is load-bearing and is stated here to be guarded.
