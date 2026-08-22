# ADR-0049: The adjudication contract — what a court opens, and the bound that makes it model-size-independent

Status: **Proposed.** Activates nothing, registers no class, and changes no proof-of-work. It
specifies what a refutation may ask for, what the court may open to answer it, and the admission
gate that makes both bounded by a class's own geometry rather than by its model's size.

Date: 2026-08-21

Relates to: ADR-0038 (PALW is the consensus work; assumption **A4**, catalog coverage, is the
clause this ADR corrects), ADR-0030 (the step space and shape profile), ADR-0040 (`PALW-BASE-0`,
whose all-`int8` operands are the reason Decision A's defect stayed invisible), ADR-0042 (the
mainnet-candidate ruleset, whose court parameters gain four companions here), ADR-0046 (chain
carriage and its size expectations, which this ADR is what makes achievable),
`consensus/core/src/palw_step_refute.rs`, `consensus/core/src/palw_artifact.rs`,
`consensus/core/src/palw_class_admission_v2.rs`.

**Two premises this ADR does not touch, and is written to protect.** Block production stays a PALW
lottery — hash-function-independent, algo-6, one solved attempt per block. And the free-prompt lane
stays: a miner earns by running a *useful* inference for a real prompt. Every decision below exists
because those two premises require a court whose cost does not grow with the model, and the court
does not have one today.

---

## Context — the central claim was measured and does not hold

ADR-0038 W1 says a full node adjudicates every dispute while holding no model. The mechanism is
proof-carrying evidence: a refuter opens the operands it used against the class's registered
`artifact_root`, and the node recomputes one step. `PalwNoWeightsV1` exists as a production type
precisely so that "a node with no weights" is the normal case rather than a degradation.

An external audit of the 2026-08-21 snapshot reported that this does not hold in the code, and
every load-bearing part of that report reproduces on the integration branch:

**The terminal adjudication opens the whole weight matrix.** `palw_step_refute.rs:459`:

```rust
let wanted = out_dim.checked_mul(x.len()).ok_or(PalwStepRefuteError::Unadjudicable)?;
```

`out_dim × in_len` is the entire matrix, and `:479`/`:494` reject an operand of any other length.
The adjudicator then recomputes every output row and compares one tile at the end. Measured against
real geometry:

| matrix | whole-matrix opening | the tile actually disputed |
|---|---|---|
| BASE-0 `output` 4096 × 256 | 1.0 MiB | 16 KiB at `tile_len` 64 |
| Qwen2.5-1.5B `unembed` 151,936 × 1,536 | ~223 MiB | 192 KiB at `tile_len` 128 |

ADR-0046 sizes a court close well under 152 KB. The gap is two to three orders of magnitude, and it
scales with the model — which is the one thing the design promised it would not do.

**The operand API disagrees with itself about its unit.** The trait says
(`palw_step_refute.rs:617`) *"Little-endian raw bytes of `elements` values … in the tensor's own
dtype"*, so `elements` counts VALUES. The production oracle returns `elements` BYTES
(`palw_artifact.rs:187`, `operand.bytes[..elements as usize]`). The two coincide for exactly one
dtype width, and `PALW-BASE-0` is `int8` throughout — so the defect is invisible in the only class
that exists. `Rescale`, `Requantize` and `RopeTable` read multi-byte parameters through this same
oracle, so the first class with a wider operand adjudicates nothing and reports it as
`InputSetNotCanonical` rather than as the API mismatch it is. The test double `FixedRow` returns the
whole row regardless of the requested size, which is why no test sees it.

**Coverage passes on a class with unadjudicable steps.** `palw_step_refute.rs:394`:

```rust
if coord.call_index != 0 {
    return Err(PalwStepRefuteError::Unadjudicable);
}
```

Embedding is adjudicable at prefill only, and the reason given is correct: a decode token is not in
the prompt, so a challenger naming it freely would convict an honest producer. But BASE-0's own
canonical job is prefill 8 / **decode 4** (`palw_base0_profile.rs:442`). So the liveness floor
itself reaches steps its court refuses — while `verify_catalog_coverage_v1` reports 100 %, because
it compares *kernel ids* and every kernel BASE-0 reaches is catalogued.

**And four hand-written descriptions of one computation disagree.** The engine narrows after
`RmsNorm`, after each projection, after `RopeTable` and after each residual add; the profile's node
tables and the court's adjudicator carry only the un-narrowed op; the artifact inventory names
`attn_norm.weight`, `ffn_norm.weight` and `output_norm.weight`, which neither the engine nor the
court ever reads. Today that is inert because no worker commits real step legs. The moment one does,
the court recomputes a different value than the producer committed — which is a false conviction of
an honest producer, the one failure this court may never have.

The through-line is a single mistake made four times: **the coverage gate answers "is this kernel in
the catalog", and the question that decides whether a network works is "can the court close, on every
coordinate a class can reach, inside a bounded opening".**

---

## Decision A — operands are addressed in bytes, and the canonical encoding is pinned

`PalwWeightOracleV1::weight_row(tensor, layer, row_start, elements)` becomes

```
operand_bytes(tensor_name, layer, byte_offset: u64, byte_len: u32) -> Option<Vec<u8>>
```

Byte-addressed on both sides, with no dtype arithmetic anywhere in the oracle. Each catalogued
kernel's descriptor pins the canonical byte encoding of the parameters it reads, so
`byte_len` is a function of the node and its dtype rather than a number two implementations derive
separately.

*Why bytes and not "dtype + element count".* Either would remove the ambiguity. Bytes remove it in
the layer that is easiest to get wrong: a Merkle opening proves BYTES, so an oracle that speaks
bytes needs no conversion between what it proves and what it returns, and there is no second place
for the conversion to be written differently.

`FixedRow` and every other test double must honour `byte_len` exactly. A double that returns more
than it was asked for is the reason this defect had no failing test, and a double that ignores its
arguments is not a double of anything.

## Decision B — the terminal adjudication is tile-local

A refutation already names a coordinate `(call_index, node_slot, position, tile_index)`. The court
must open, and must recompute, only what that coordinate depends on:

* **operands:** the weight rows the tile's own outputs reduce over — `tile_len × in_len` values,
  never `out_dim × in_len`;
* **recomputation:** the tile's output lane only. Not every row followed by a comparison of one.

For a reduction over the input (`MatMulQuant`, `DotI8`) the tile's outputs are a contiguous slice of
output channels, so the opening is a contiguous row range and the Merkle path count is bounded by
the tile, not by the matrix.

This is the decision that makes ADR-0038 W1 true rather than aspirational, and it is what keeps a
court close inside the carriage budget ADR-0046 assumed.

## Decision C — admission bounds the court from the class's own geometry

Coverage PASS stops being sufficient for admissible. `verify_class_admission_v2` derives four
numbers from the shape profile and refuses a class that exceeds the ruleset's ceiling for any:

| bound | derived from | why it must be checked at admission |
|---|---|---|
| max opening bytes, per refutation and per court close | widest tile × its `in_len` × dtype bytes, plus Merkle paths | a proof that cannot ride a transaction is a dispute nobody can raise |
| max terminal MACs | the same tile's reduction length | terminal adjudication is a full node's own CPU cost, on peer-supplied input |
| max operand count | the node's `input_refs` and its weight operand | bounds deserialization work before any arithmetic runs |
| max Merkle path count | inventory depth × operand count | the same, for proof verification |

Each ceiling joins `PalwCourtParamsV2` and therefore `palw_ruleset_id_v2`, beside
`max_step_leaf_count`. They are frozen with the network for the same reason it is: a class deeper or
heavier than the ceiling cannot join a running chain, so the ceiling must be chosen once, at genesis,
for every class the network ever intends to admit.

*This is deliberately the same shape as the existing ladder gate.* `max_step_leaf_count` bounds how
many rounds a dispute takes; these bound how much each round costs. A ruleset that fixed one and left
the other free was bounding the number of steps in a walk without bounding the length of a step.

## Decision D — coverage is over reachable coordinates, not over kernel ids

A class is adjudicable **iff every reachable `(call_index, node_slot, position, tile_index)`
adjudicates.** The kernel-id set remains necessary and stops being sufficient.

`verify_catalog_coverage_v1` keeps its job — it answers "can this build execute this kernel at all" —
and gains a companion that asks the question A4 actually needs: enumerate the coordinate classes a
profile reaches (prefill and decode calls, every node slot, the tile shapes each produces) and require
an adjudicating arm for each. C-04 is exactly what this catches: every kernel id is catalogued and a
whole call class is refused.

## Decision E — decode is adjudicable, by challenging the argmax rather than proving it

The `call_index != 0` refusal was right about the danger and wrong to stop there: on the free-prompt
lane the **generated text is the product**, so a class whose decode steps cannot be adjudicated cannot
carry the lane the network exists to sell.

The producer commits, per decode position, the logits vector's Merkle root — it already computes the
vector. The decode token is `argmax_lowest(logits)`, the tie rule the engine already pins. The court
never verifies the argmax, which would cost the whole vocabulary. It adjudicates a **challenge** to
it, and a challenge is one index:

> A challenger names `j` and opens `logits[j]`. The token is refuted iff
> `logits[j] > logits[token]`, or `logits[j] == logits[token] && j < token`.

One opening, one comparison, independent of vocabulary size. The embedding step at a decode position
then adjudicates against a token the chain has pinned, and the challenger who names a token freely is
refuted by the same opening rather than believed.

*Proving is expensive and refuting is cheap, so refute.* That is the same asymmetry the whole dispute
model rests on, applied to the one place it had not been.

> **Implemented 2026-08-22 — integer-first, both halves.** The selection rule is one pinned
> function, `base0_decode_token_select_v1` (argmax, ties to the LOWEST index), which the engine's
> decode loop and the court both call — the engine's `argmax_lowest` delegates to it, so
> "selected" cannot mean two things. On-chain refutation is the new close arm
> `PalwCourtVerdictProofV2::DecodeToken { binding, pin, position }` with fault
> `PalwStepFaultV1::DecodeTokenMismatch { position }` (evidence kind 6): the pin carries the
> integer class's logits rows and generated ids, authenticated by recomputing
> `base0_logits_trace_root_v1` against the binding the claim's own `execution_root` pins — the
> evidence is the commitment itself, no artifact opening involved. For BASE-0 the "one index"
> economy is unnecessary: its vocabulary is small by construction, so the pin carries whole rows
> (bounded by the court's opening-byte ceiling at the cost gate) and the court runs the whole
> argmax. A `Float32` class is refused by name — its per-position openings arrive with the class
> that needs them (Gate 3).
>
> The same pin closes the **integer-leg dispatch**: `full_logits_trace_root` is one header slot
> with two occupants, and the step-refutation decode check used to recompute only the v2
> event-tree root, so a BASE-0 decode-embed gather could never authenticate its generated ids —
> 4 of the floor's 914 leaves ended `Unadjudicable`. The check now dispatches on the class's
> registered `PalwStepLaneV1`, and the sweep
> (`the_court_convicts_no_leaf_of_an_honest_execution`) demands **914/914 adjudicated, zero
> unadjudicable** — a new hole fails the test by name. Money path verified end to end
> (`palw_v2_a_lying_decode_token_convicts_through_the_court_close`): the lying claim voids as
> `CourtFraud` and its bond is slashed; the honest one survives the same close with its stake.
> Mutation-checked in both directions: reverting the dispatch reddens the sweep, and re-tying the
> selection rule to the highest index reddens the tie test.

## Decision F — the engine, the profile, the adjudicator and the inventory are projections of one description

Decisions B and D require the engine's tile boundaries and the court's tile boundaries to be the same
object, not two objects that agree. Four independently authored descriptions of one computation is
how the narrowing steps came to exist in one and not the others.

Each class carries one canonical execution description. The shape profile, the engine's op sequence,
the adjudicator's node table and the artifact inventory are each **generated** from it, and a golden
test asserts the four projections agree — node for node, tile for tile, tensor for tensor.

Until the generator exists, the interim obligation is narrower and immediate: **no worker may commit a
step leg for a class whose profile does not name every narrowing the engine performs.** A commitment
the court cannot reproduce is a false conviction waiting for its first dispute.

## Decision G — the canonical artifact inventory, and one meaning for "class id"

`artifact_root` is the Merkle root over a canonical inventory manifest, one leaf per operand row.
Each entry carries:

```
tensor name · layer · dtype · shape · byte_offset · byte_len · quantization record · order index
```

and the manifest is refused for any of: a duplicate entry, a missing tensor the profile names, an
overlapping byte range, a byte of the artifact no entry covers, or a non-canonical order. "Every byte
is covered exactly once, in one order" is what makes an opening's absence meaningful.

This also settles the two things called a class id. **`execution_class_id` is the shape profile id** —
a class is its graph, which is what the chain already keys on. The artifact digest is
`artifact_root` and is a separate value with a separate job: the graph says what is computed, the root
says what it is computed against. A flat hash of a whole artifact is neither, because nothing can be
opened against it.

## Decision H — post-genesis class registration is allowed, at the minimum share, gated by Decision C

Three policies coexist today: the lifecycle carriage refuses `ClassRegistered` outright
(`palw_lifecycle_objects_v2.rs:104`), `verify_class_admission_v2` would admit at the minimum grantable
share, and the state machine implements a weightless activation clock. Consensus does not benefit from
three answers.

The carriage's objection is the correct one and it is a statement about *checking*, not about
*forbidding*: a class entering a live chain moves the share table and brings its own `pwu_rule`, and
nothing checked either. Decision C is that check. So:

**a class may register post-genesis, at `min_grantable_share_permille`, iff it passes coverage over
coordinates (D), the four cost bounds (C), and the derived-pwu rule the genesis loader already
enforces.** The carriage's refusal is replaced by that gate rather than removed, and the object gains
the shape profile the gate needs — which it must carry anyway, because nothing else on a running chain
can tell the court what the class computes.

---

## Consequences

* **The 1-tile claim becomes true, and must be re-measured before it is repeated.** Every statement
  of the form "a full node adjudicates one tile without holding the model" is unsupported until
  Decision B lands and the opening sizes are measured on real geometry.
* **Four ruleset parameters are added, and freeze with the network.** Choosing them is a genesis
  decision with the same expiry as the ladder: too small forecloses classes, too large admits a
  proof nobody can verify in time.
* **Existing golden values move.** The operand API change (A) and the inventory (G) both change what
  is hashed. Nothing is registered yet, which is why this is the moment.
* **`FixedRow`-style doubles must be re-written before they can witness anything.** A test double
  that ignores the size it was asked for cannot detect a size defect, and one did not.
* **The audit's NO-GO stands until B, C and D land.** A class can be coverage-PASS, ladder-admissible
  and still have no closeable court, which is precisely the state measured today.

## What this ADR does not decide

* **The residual amplification.** Whether `BASE-0`'s residual add gains an amplifying `Rescale` is a
  separate decision, pending its own measurement; it changes the arithmetic, not the adjudication
  contract.
* **Qwen3.6's MoE / GatedDeltaNet / SSM primitives.** New ops need their own accumulator and state
  bounds, which is an ADR of its own. This one is a precondition for it: a new op set is not worth
  specifying against an adjudication contract that does not close.
* **Performance.** Nothing here is a throughput decision, and no bound below may be relaxed for one.
