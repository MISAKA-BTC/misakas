# ADR-0145 — Canonical work is derived, not declared; admission is earned, not registered

**Status:** DESIGN 2026-09-19; **§1–§5 implemented dormant** (ADR-0147/0148/0149, the economic
bundle, `None` on every preset). **§6 prefix-STATE: object family implemented 2026-09-21**
(`PalwFpPrefixStateV1`, `fp_derive_work_from_state_v1` derives `KvReused`). The V3 commitment
wire is unchanged — a field addition would be a new object family, and live claims persist. A
V3 commitment that names no state is genesis. No new fence.

ADR-0144 fixed what PALW is for. This fixes the two mechanisms the 2026-09-19 reward audit proved
cannot carry it: a unit of work the registrant writes, and an admission the registrant can pass
alone.

---

## 1. The defect, stated precisely

The audit's headline is usually told as "leaves are not arithmetic". That is true and it is not the
sharpest form. The sharp form is this.

Class difficulty is seeded so that a class's share of blocks converges on its share of the network
(`palw_class_daa.rs:684-696`):

```
T_c ∝ share_c / r_c        where r_c = canonical inferences per second
r_c ∝ 1 / w_c              where w_c = pwu_per_inference, "the only hardware-free reading"
⟹  claim.pwu = expected_attempts × w_c  ∝  1 / share_c
```

**`w_c` cancels.** The design is coherent: a class that declares a bigger job wins proportionally
less often, and the product is invariant. A registrant inflating `w_c` should gain nothing.

It gains everything, because the cancellation assumes **the producer runs the job it declared**, and
since `palw_prefill_draw` armed at DAA 4,000 it does not. An attempt executes
`exact_decode_tokens = 1` (`palw_attempt_v2.rs:227`) while `w_c` still counts every declared decode
call. So `r_c` is not `∝ 1/w_c`; it is far larger, and the cancellation fails by exactly the ratio
between the declared job and the executed one.

Measured, on one artifact and one kernel set (`docs/audit/2026-09-19-llm-mining/`):

| declared canonical (P,D) | `w_c` | MAC-eq executed | weight index | pay index |
|---|---|---|---|---|
| (63, 2) shipped | 6,630,544 | 83.1 G | 100 | 100 |
| (1, 256) | 51,535,376 | 83.1 G | **777** | 100 |
| (1, 432) | 52,714,368 | 1.5 G | **2,457,197** | 107 |

**The defect is not that leaves are the wrong unit. It is that the unit is DECLARED and the
execution is not bound to the declaration.** Any unit — leaves, MACs, a vector — inherits the same
defect if a registrant writes it and nothing checks the run against it.

That is why this ADR's first rule is about derivation rather than about units.

---

## 2. Four invariants, ahead of any mechanism

**I1 — No self-reported work.** `claim.pwu` and `work_leaves` are never trusted. A validating node
derives the work from the class's canonical description and the execution facts, and a claim whose
declared value differs is refused — not silently corrected, because a silent correction is a wire
format nobody reads.

**I2 — Representation neutrality.** The same model, the same input and the same effective execution
yield the same canonical work, whatever `tile_len`, graph partition, node order, serialization,
commitment segmentation, registrant metadata or declared cost were chosen.

**I3 — Admission independence.** A class owner's own bonds cannot admit that class. Registrant
stake alone MUST NOT carry an admission quorum.

**I4 — No cross-class repricing.** Registering a class MUST NOT change the reward rate, weight,
difficulty or accounting coefficients of any unrelated existing class.

I1 and I2 kill C1 at the root. I3 kills C3. I4 makes C2 unrepresentable rather than superseded.

---

## 3. Canonical class identity

Reward-bearing facts are separated from a registrant's free choices. The canonical descriptor is
derived from what the model IS:

```
artifact hash · architecture · layer structure · tensor shapes
attention structure · MoE routing structure · quantisation format
tokenizer commitment · runtime-relevant execution semantics
```

and explicitly NOT from `tile_len`, commitment segmentation or serialization order. The same weights
packaged two ways normalise to one canonical representation.

**This is a change to what a class IS.** Today `class_id = PalwShapeProfile::shape_profile_id()` —
canonical borsh over the whole profile, `tile_len` included — so two tilings of one model are two
classes with two prices. Under this ADR they are one canonical model at two commitment
representations, which is what they physically are.

**A consequence to state plainly**: the tokenizer has no chain identity today
(`palw_freeprompt_v3.rs` says so in its own header — "the job's `tokenizer_id` … no such check
exists"). Binding one is part of this work, not a check to add later, because two tokenisations of
one prompt are two different executions.

---

## 4. CanonicalWorkVector

One scalar cannot price dense GEMM, routed experts, attention, KV traffic and quantised kernels,
because the hardware does not. Neither candidate survives: STEP leaves count activations while cost
counts weights (a 6.8× spread, monotone in model width, live today), and MAC-equivalents carry no
memory-traffic term at all.

So the derived quantity is a vector:

```rust
CanonicalWorkVector {
    dense_matmul, routed_expert_matmul,
    attention_prefill, attention_decode,
    kv_read, kv_write,
    normalization, other_verified_ops,
}
```

derived as `canonical graph × actual execution facts → vector`. A registrant supplies no number in
it. Prompt length 1,000, decode 200, 2-of-8 experts activated: the vector follows from those facts
and the graph, both of which a verifier holds.

**The coefficients that turn a vector into an economic unit are protocol-set, versioned, and
consensus-critical.** They are not in this ADR, they get their own, and they must be calibrated by
an experiment nobody has run: both live classes' draw jobs replayed warm on one host, artifact
resident and then not. Until that runs, any coefficient is a guess wearing a number.

**Dimensions are provisional.** They will be decided by implementation and adversarial review, not
by this list. The commitment here is the shape — derived, multi-dimensional, registrant-free — and
that `claim.pwu` becomes `derive_canonical_work(claim, class)` rather than a field.

---

## 5. One derivation for both lanes

Attempt and FreePrompt must not keep separate accounting, or they will diverge again.

```
Attempt    ─┐
            ├→ derive_canonical_work(...) → ProtocolEconomicUnit ─┬→ fork-choice contribution
FreePrompt ─┘                                                     └→ payout
```

`work_leaves` may stay on the wire for compatibility, but **declared ≠ consensus work**: a validator
derives independently every time and refuses a mismatch. The repo's own test today does the
opposite — `palw_fp_objects_v3.rs:885` multiplies `work_leaves` by ten and asserts the walk still
passes. Under this ADR that test inverts and becomes a regression fixture.

**Weight and pay take the same input.** They need not be the same scalar, but a registrant must not
find a path that makes weight large while pay stays ordinary. The audit found weight and pay on
different paths, and that is how one of them went unexamined.

---

## 6. Cache is an execution fact, never a claim

A normal long chat re-sends a context the executor already holds. Paying 32k of prefill when 30k was
cached is I1 violated through a different door, and the free-prompt credit rule does exactly that:
pay scales ~62× with prompt length while a producer holding the prefix's KV cache spends ~3 % more.

So the receipt carries what makes the answer checkable:

```
input commitment · prefix-state commitment · new token range
output commitment · model class · execution mode
```

and a verifier reconstructs **new work only**. A miner's word for "that was a cache hit" is never an
input. `execution mode` is explicit — uncached, prefix-reused, KV-reused — because a mode the
protocol cannot name is a mode it cannot price.

---

## 7. Admission: registered is not eligible

`ClassRegistered` becomes existence and nothing else.

```
register → Candidate → artifact/graph validation → canonical work derivation
         → independent admission → Probation → limited claims → full eligibility
```

A **Candidate** can exist, be inspected, be benchmarked and run shadow inference. It cannot earn
reward and cannot contribute fork-choice weight.

**Admission independence (I3).** The eligible admission panel is network-wide independent bonds,
minus owner-controlled bonds, minus the same beneficial-owner cluster. Today the panel is drawn
exclusively from bonds that declared capability for the class, and capability is
registrant-controlled — so a stranger's class is normally judged by its own registrant. Promotion to
Probation requires canonical graph verification, artifact ownership, deterministic replay, agreement
between independently derived work vectors, adversarial profile tests and independent panel approval.

**Probation bounds VOLUME, not price.** Per ADR-0144 P7, what grows with verified use is how much
reward-eligible work a model may contribute; what one unit of canonical work is worth never moves.
A per-epoch ceiling on a new class's eligible work is safer than a discount on its rate, because it
bounds the blast radius of an accounting bug that survives review.

### What already exists, so this is built on the tree rather than beside it

* `PalwModelLifecycleStateV1::admission_permille()` — Probation 50 ‰, ActiveLimited 100 ‰, Active
  1,000 ‰ — with entry at `Probation { probes_passed: 0 }` and promotion driven by ten COMPLETED
  CLAIMS, which are protocol-verified events.
* `palw_uncertified_weightless` is armed from genesis on testnet-11: a class with zero share bears
  zero weight (`palw_state_v2.rs:1422, :1441`).
* A post-genesis entrant's `share_permille` is **enforced, not chosen** — it joins at the minimum
  grantable share, and the two post-genesis classes on testnet-11 both read `sharePermille: 1`
  against the genesis classes' 490/488/20.

**The containment is therefore the first form of this design, not throwaway code.** Making a
post-genesis entrant's required share zero until it is admitted uses machinery that is already armed
and already enforced, and it states the end state — *only explicitly admitted classes may affect
consensus* — in the smallest change that can express it. It is still a consensus change and still
needs its own fence and a drill that crosses it.

---

## 8. What must be proved, adversarially

Success examples prove nothing here. Every property is stated as something an attacker cannot do.

**Representation invariance.** Generate a class fixture, then vary `tile_len`, graph partition, node
order, serialization, canonical (P,D), metadata, owner and bond layout, holding model, input and
output fixed. Canonical work, weight and reward are identical across every variation.

**Registrant independence.** No registrant-writable field increases canonical work, reward or weight.

**Executor independence.** Tamper with every declared workload value; consensus derives the same
answer or refuses the claim.

**Model neutrality.** The reward per unit of canonical work does not vary with the class owner's
choices.

**Efficiency preservation.** The same canonical work on faster or cheaper hardware earns no less.

**Cache correctness.** Work already computed cannot be paid twice; a cached prefix cannot be
presented as new.

**Unlimited use, scarce reward.** Local inference is unbounded; eligible work never exceeds the
protocol budget.

**Fork determinism.** Archival, pruned, IBD, pruning-proof join and post-reorg nodes derive the same
work, eligibility, weight and reward.

**Self-admission is impossible.** A class whose owner controls 100 % of the seats declaring
capability for it cannot be admitted.

The five counterexamples the red-team built (`docs/audit/2026-09-19-llm-mining/`) become regression
fixtures: the decode declaration, the re-tiled dense row, the better-model-paid-less table, the
padded prefix, and the class admitted at a ladder its own legal jobs exceed. Each must FAIL or
neutralise under the new design, and each must fail for a structural reason.

**A finding is not closed because a test passes.** It is closed when the attack path cannot be
expressed. "The test is green" is how a policy becomes an assumption.

---

## 9. What this ADR does not decide

The coefficient table and its governance. The exact dimensions of the vector. The eligibility
lottery's parameters. The height of any fence — ADR-0144 §6 item 0 stands: **the design is proven
first and the date is chosen after**, because promising a DAA score for unfinished work is how the
last emergency was scheduled.

---

## 10. Done means

> Anyone may register a class. Registration alone earns no sompi of reward and no unit of
> fork-choice weight. Only a class that passed independent verification and has an established
> canonical work accounting receives a bounded claim eligibility. And for the same real inference,
> no choice of registrant, executor or profile representation improves the reward-to-work ratio.

At which point new models are something the network can safely have more of, rather than something
it has to be protected from.

---

## 11. Implementation (2026-09-21)

§1–§5 remain behind the economic bundle (`palw_canonical_work` / `palw_admission_independence` /
`palw_fp_derived_work`), `None` on every preset. **§6 prefix-STATE is now an object family**:
`PalwFpPrefixStateV1` (own domain, own id). A V3 commitment that names no state is genesis —
Uncached or PrefixReused from paid prompt ids. A named non-genesis state of this class derives
`KvReused` and credits the tail only (`fp_derive_work_from_state_v1`). Declaring more cache cannot
raise the pay. The V3 commitment wire is unchanged (golden-vector rule); extraction reads genesis
until a later payload version carries the object. No new fence. No testnet-11 height.
