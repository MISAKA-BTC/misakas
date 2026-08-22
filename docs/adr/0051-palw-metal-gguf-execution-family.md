# ADR-0051: The Metal/GGUF execution family — native-speed inference as half the work, quorum-verified beside the deterministic floor

Status: **Proposed.** Activates nothing. Defines the second execution *family*, its verification
scheme, its economy cap, and the one structural consensus change it needs (per-class panel
parameters). Every number here is a proposal until the family's first class is registered.

Date: 2026-08-22

Relates to: ADR-0026 (borrow Ambient's shape, strengthen the proof — **partially and deliberately
walked back here, for one capped family**), ADR-0028 (challenge sampling), ADR-0038/0039 (PALW is
the consensus work; BASE-0 is the floor — both premises kept), ADR-0040 (the integer family this
one does NOT replace), ADR-0044 (free-prompt receipts — the UX carrier), ADR-0045 (the share
table — where "half" lives), ADR-0049 (the adjudication contract this family deliberately never
enters), `docs/ambient-pol-binary-audit-2026-08-15.md`,
`docs/palw-qwen25-block-generation-blocked-2026-08-22.md` (the measurement that motivated this).

---

## Context — what one real model cost, and what it bought

Making Qwen2.5-1.5B a MISAKA-arithmetic class took, in one day: a canonical IR projection to close
an 842-disagreement graph gap (ADR-0049 Decision F, landed), an arithmetic canonicalization to
close a two-epsilons-under-one-model-id split (landed), a per-model converter, a 1.7 GiB re-quantized
artifact — and the result still produces a degraded argmax (`[11,11,11,11]`), because static PTQ
into BASE-0's int8 stream loses the model. The execution path is honest and adjudicable; the model
it executes is not worth using. Meanwhile the SAME checkpoint, in its native GGUF quantization on a
consumer Mac, runs at full quality at ~40 tok/s.

Every future model repeats this cost, because the deterministic family's whole value — a court that
can convict a lying executor from one opened tile, with no model in hand — is priced in exactly the
work that makes models slow and lossy to admit: one pinned arithmetic, one canonical graph, one
inventory, bit-exact across every participant.

This ADR stops bending models into MISAKA arithmetic *for the lane whose job is UX*, and inverts
the dependency there: MISAKA rides on top of the existing runtime, commits to what the inference
SAID rather than to every arithmetic step it took, and verifies by bonded same-hardware replay
within a tolerance. The deterministic family keeps the other half of the network, unchanged,
because it is the only half that can produce slashing-grade guilt.

### What Ambient actually does, per our own audit — and what ADR-0026 decided

The 2026-08-15 binary audit (`ambient-pol-binary-audit-2026-08-15.md`) found: verifier embedded in
their llama.cpp fork; logits committed during generation; validators recompute ~1 token;
comparison is **tolerant** (RBO / p95-band), not exact. ADR-0026's thesis was: take that
architecture (runtime/verification separation, Merkle-commit → post-commit challenge → recompute,
async verification, bonds), refuse that proof model, and build exact-within-pinned-class instead.

**ADR-0026's thesis stands — for the family that can afford it.** What this ADR adds is the
honest complement: a family where the tolerant proof model is accepted *by name*, capped at half
the economy, and barred from the court. 0026 rejected tolerance as "weaker than PALW needs";
the two-family split re-scopes that sentence: weaker than *slashing* needs. A family that never
slashes on arithmetic may verify the way the hardware actually behaves.

### Why not bit-exact on Metal

No vendor guarantees float reproducibility across Apple Silicon generations, Metal/OS versions, or
kernel dispatch shapes; reduction order moves with batch and threadgroup configuration. Same-build
same-device replay agrees byte-for-byte often enough to be a fast path, and never reliably enough
to be a consensus rule. A family built on Metal therefore verifies within a tolerance — which
means it can never distinguish "lied by ε" from "rounded by ε", which means **no arithmetic
conviction, ever**. That is not a gap to close later. It is the boundary that defines the family.

---

## Decision 1 — Two families, one economy, half each

An **execution family** is a verification scheme. Two exist:

* **Family D (deterministic)** — BASE-0 (the floor, unchanged) and every future class whose
  arithmetic is pinned, whose graph is projected from a canonical IR, and whose disputes end in
  the ADR-0049 court. The integer Qwen class (Decision F projection, 2026-08-22) belongs here:
  registered weightless, promoted if PTQ quality ever justifies it. BitNet, when it comes, comes
  here (`relu2` stays out of the catalog until then).
* **Family M (Metal/GGUF)** — classes that run a pinned GGUF on a pinned Apple-Silicon/Metal
  runtime build, commit what the inference said, and are verified by bonded same-family replay
  within a registered tolerance.

The split lives where ADR-0045 already put every cross-class question: the **share table**.
Family M's classes are granted shares summing to **500‰**; Family D holds the other 500‰ (all of
it on BASE-0 until the integer Qwen earns weight). Per-class DAA (already landed) holds each
class's cadence at target regardless of hardware speed, and epoch budgets (`budget_blocks`, from
shares) make "half of block production" a literal, enforced block count — not an aspiration.

The deterministic floor's 500‰ is a **floor, not a default**: no Family-M registration may push
Family D below half. Rationale: Family M's finality is quorum-attested (Decision 5); its worst
case is a colluding panel minting its own class's weight. Capping the family at half bounds that
worst case at half, and keeps the half that can convict liars in charge of the tie.

## Decision 2 — A Family-M class is defined by what it pins, not how it computes

A Family-M catalog entry (= class registration carriage) pins, by hash or by value:

```
device family        Apple Silicon (Metal)          — the only entry at launch
runtime_id           sha256 of the pinned runtime build (a specific llama.cpp-fork commit,
                     built with pinned Metal shader options)
model                GGUF file sha256               (precedent: qwen35_pins::GGUF_SHA256)
quantization         e.g. Q4_K_M                    (informative; the sha256 is the pin)
tokenizer            tokenizer.json commitment      (precedent: tokenizer_commitment, in-tree)
n_ctx / decode budget
sampling             greedy (argmax), pinned        — the only checkable rule
n_threads / n_batch  pinned                         — dispatch shape moves float sums
tolerance            ε (fixed-point), K (top-K width), m (spot-check count) — consensus constants
```

The runtime is a black box *with an identity*. MISAKA specifies what is committed about an
inference, never how the runtime computes it — which is precisely what makes the next model a
**registration instead of an engineering project**: no new arithmetic, no IR projection, no
converter, no artifact re-quantization. `CAT-M-0001` is proposed as Qwen2.5-1.5B GGUF Q4_K_M;
subsequent entries (Qwen3-4B, Llama-3.1-8B, …) differ only in pinned hashes and budgets.

The admission gate for Family M checks catalog coherence (hashes present, sampling greedy, budget
within family bounds, tolerance within network bounds) *instead of* step-space counts — there is
no step space. The gate refuses a Family-M registration that names a court window, for the same
reason it refuses a Family-D registration that omits one.

## Decision 3 — The commitment is the v2 trace scheme, which already exists

Family M does not get a new commitment format. The **v2 full-logits trace scheme** — built for a
pinned float runtime, golden-frozen, and already carried in the attempt's slots
(`full_logits_trace_root_v2`: per-position logits event hashes, Merkle event tree, outer root
binding job/network/runtime/class/budget; `output_token_ids_hash_v2` binding the token stream) —
is exactly the chain-of-custody this family needs, and it is stronger than a bare hash chain: it
is openable per position.

The per-class scheme dispatch already exists twice over: `PalwJobContextV2.trace_scheme_id`, and
the lane dispatch landed 2026-08-22 ("one slot, two occupants, dispatched on the class's
registered lane"). Family M classes register `lane = Float32` — the lane the v2 scheme was
written for, whose decode-token court arm is *already refused by name*. One amendment: the
committed logits rows are **fixed-point quantized before hashing** (a registered scale, e.g.
Q8.8 over the top-K per position, full-vector hash optional per class). Reason: a tolerance rule
over raw f32 commitments makes the commitment itself platform-dependent; quantizing first makes
"what was committed" exact and moves ALL platform slack into the verifier's ε, where it is a
consensus constant instead of an accident.

## Decision 4 — Verification is teacher-forced spot replay by the panel that already exists

Nothing new reaches consensus here. The existing lattice — material broadcast, panel draw, seat
receipts, quorum, `ReceiptLicensed`, `Final` — carries Family M unchanged. What changes is only
what a seat DOES before signing:

1. The seat holds the same pinned runtime + GGUF (it is a Family-M operator by definition).
2. From the broadcast material (prompt/request, committed token stream, per-position quantized
   logits commitments) it picks **m positions it chooses itself**, unpredictably (anchor-derived,
   per ADR-0028's sampling discipline — the producer must not know which positions get checked).
3. For each picked position i it replays **teacher-forced**: prefix = the producer's committed
   tokens `t_0..t_{i-1}`, one forward pass. Teacher-forcing is what makes divergence non-cascading
   — near-tie disagreement at position j cannot poison position j+1, because the check at every
   position is against the committed stream, not against the seat's own continuation.
4. Verdicts, per position:
   * **token rule** — the committed token's recomputed logit must be ≥ (recomputed max − ε).
     Greedy sampling plus this rule means: an honest producer always passes; at a genuine
     near-tie either twin passes; a token more than ε from the top fails.
   * **logits rule** — the committed quantized top-K row must match the seat's recomputed,
     identically-quantized row within the registered per-element tolerance (the p95-band shape
     the Ambient audit measured, made a consensus constant by the fixed-point step).
5. All m pass → sign `PalwSeatReceiptV2` Valid. Any fail → the seat signs nothing on the merits
   (a claim that gathers no quorum voids at `ReceiptTimeout` — burned escrow, no slash), or files
   `Unavailable` if the material itself was not served (the existing `ProducerWithholding` path).

Byte-equal full-trace agreement remains a legal fast path (same device family, same build — it
will often hold) but is never required: the rule is the tolerance, so an OS update that shifts
float sums degrades the family to voided claims, never to slashed bonds.

## Decision 5 — What this family cannot do, said out loud

**No court.** A challenge against a Family-M claim has no arithmetic terminal: the bisection
ladder, the step space, operand openings, `ComputationMismatch`, `DecodeTokenMismatch` — none
exist for a black-box runtime, and a tolerance can acquit but never convict. The only slashable
offenses in Family M are the **objective** ones that need no arithmetic: seat equivocation (two
contradictory signed receipts — the ContradictoryVerification rule), producer equivocation, and
data-withholding defaults. Everything else fails *soft*: no quorum → void → escrow burned →
producer earned nothing.

Family M's fraud resistance is therefore exactly: the collusion resistance of 5 bonded seats from
5 distinct operators, times the 500‰ cap, times the unpredictability of spot-check positions.
That is a weaker guarantee than Family D's, it is priced as such (Decision 1), and this ADR's
whole honesty is refusing to pretend otherwise. The audit rule this repo already lives by —
*fraud proofs must be provable; only contradictions may slash at acceptance* — applies verbatim.

## Decision 6 — The one structural change: per-class panel parameters

Panel size and quorum are bundle-global today (`PalwPanelParamsV2`, inside the ruleset id). The
shipped 5-of-5-distinct-operators rule means a Family-M claim needs **six distinct Mac-holding
bonded operators** (five seats + the executor) before a single claim can license. The current
fleet is four x86 Linux hosts and zero deployed Macs; requiring six on day one is requiring the
family to not exist.

So: class registration gains optional panel parameters `(seats, quorum)`, **floored by network
minimums carried in the bundle** (proposal: floor 2 seats / 2 quorum; BASE-0 keeps the global
5). A class registered at 2-of-2 is registered as *visibly thinner* — the parameters are in the
registration carriage, on chain, priced into anyone's trust in that class's weight — and can be
superseded by registering a successor class at full panel when the operator set exists. This is a
ruleset change (it moves the ruleset id) and must ride the next re-mint. It is the only consensus
schema change this ADR requires.

## Decision 7 — pwu, and why "work coefficient" games dissolve

Family M keeps ADR-0045's `DerivedV1` discipline with one substitution: `pwu_per_inference` is
**counted from the canonical job as its decode-token budget** (there are no step leaves to count).
It stays a counted fact — the admission gate recounts it from the carried canonical job and
refuses a mismatch, exactly as it refuses a mis-declared leaf count today. Cross-class weight is
NOT proportional to tokens-per-second or model size: the share table fixes each class's weight
fraction at registration, and per-class DAA absorbs speed. Racing a small model mints nothing
extra; the "token × 1 gaming" failure mode has no purchase because tokens were never the unit.

## Decision 8 — UX: the use IS the work

The user-request path is ADR-0044's free-prompt lane, unchanged: beacon-anchored, quantized
tickets, grinding already priced. A Mac node answering its owner's real prompts produces
commitments that spend into receipt blocks — "run your model; the run is the work" — and the
attempt lane's template-derived jobs keep the class producing when nobody is asking anything.
Nothing in the block-critical path changes for either lane: an M-class attempt is an ordinary
algo-6 block (one inference per template, nonce ground over the free `l1_tag`), verified
statelessly at admission and panel-verified asynchronously — the ADR-0037/0038 shape, untouched.

---

## What this walks back, and what it does not

| | walked back? |
|---|---|
| ADR-0026 "strengthen the proof" | **For Family M only.** The tolerant proof is accepted, named, capped at 500‰, and barred from the court. Family D keeps 0026 in full. |
| Canonical IR / Decision F | **No.** It closed a real gap (842→0) and is the machinery of every Family-D class. It stops being the *cost of admitting a model for UX*. |
| Integer Qwen artifact | **No.** Re-labeled: Family D's second class, weightless until quality justifies weight — same plan as before, minus the pretense it was the UX lane. |
| BASE-0 floor | **Untouched.** Liveness anchor, 500‰ floor, court intact. |
| BitNet / relu2 | **Still excluded** (unchanged scoping). Family D future. |

## Risks, stated

* **Tolerance forgery.** An attacker who can produce a committed stream passing ε-checks without
  running the model has beaten the family. Cost: the logits rule checks K values per sampled
  position against a commitment fixed *before* the positions are known; passing m positions ×
  K values within ε without the model is not known to be cheaper than running the model. This is
  Ambient's core bet; we inherit it knowingly, capped at half.
* **Panel thinness at launch** (Decision 6): 2-of-2 is two colluding Macs. Mitigation: the
  parameters are public, the class's share can start at the minimum grantable permille and grow
  with the operator set, and the 500‰ family cap bounds the blast radius.
* **Runtime drift.** An OS/Metal update that moves floats past ε turns the family off (claims
  void, cadence stalls) until a successor class re-pins. That is the safe direction; the runbook
  cost is real and is an operational item, not a consensus one.
* **Apple monoculture.** Accepted for launch, by construction; Decision 2's catalog is how CUDA
  or CPU families join later without touching consensus again.

## Implementation order (each PR-sized, none started)

1. `PalwBackend` seam in the worker: `load / infer(teacher_forced) / commit / verify` — the
   ADR-0026 adapter surface, made a trait. `IntegerBackend` (existing engine) moves behind it.
2. `MetalBackend`: pinned llama.cpp-fork build (runtime_id = build sha), logits callback capture,
   fixed-point quantization of committed rows.
3. Seat verdict rule (Decision 4) in the panel service, behind the family dispatch.
4. Admission arm for Family M registrations (Decision 2) + per-class panel params (Decision 6,
   the ruleset change — rides the next re-mint).
5. `CAT-M-0001` registration carriage + two-Mac drill: produce → broadcast → spot replay →
   quorum → Final, then a lying token at one position → no quorum → void.
6. Block generation on a devnet with the 500/500 share table; then the public network.

## Reading order for a reviewer

1. `ambient-pol-binary-audit-2026-08-15.md` — the measured shape this family adopts.
2. ADR-0026 — the thesis this ADR scopes down to one family.
3. `palw-qwen25-block-generation-blocked-2026-08-22.md` — what admitting one model into Family D
   cost, which is the price signal this ADR responds to.
4. ADR-0045 — the share table; Decision 1 is one row of it.
