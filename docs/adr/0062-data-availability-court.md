# ADR-0062 — The data-availability court: stop a vote from taking a bond

> Renumbered 0059 → 0062 at the 2026-08-30 branch merge: 0059 was taken the same day by the
> 10B premine cap on the parallel line (0060 the liveness doctrine, 0061 zero-seat genesis),
> and this document was the cheaper of the two to move — one file, no code references. The
> original branch commit (`554ca77c`) carries the old number in its message; this file is the
> authority.

Status: **Proposed** (2026-08-30). Supersedes nothing; completes ADR-0042 Decision 7.

> **Standing (index reconciliation, 2026-09-02).** Still Proposed, not landed. The harm it was
> written to stop — an `Unavailable` quorum voiding a claim and slashing the producer's bond with no
> proof — is removed on armed presets by [ADR-0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md)
> Decision 4 (`palw_unavailable_abstains`, armed from genesis on testnet-11 since Relaunch 5:
> `ProducerDefaulted` is unlicensable; the claim falls to a redraw and a receipt timeout, which voids
> without slashing). What this ADR still adds is the positive half — a provable default that CAN
> take a bond. The DA fields it lists as "already committed" are pinned by equality since
> [ADR-0072](0072-the-ticket-is-the-execution.md) Decision 8. The carriage ADR cited as ADR-0046 is
> [0046](0046-palw-v2-consensus-object-carriage.md). Map: [`README.md`](README.md).

> **Security amendment appended (2026-09-02)** — see the last section: the accusation becomes a bonded, singular object; the disclosure is hash-checked and bounded; "silence" is a fold fact with a majority-proof window; abstaining seats pay nothing (ADR-0065 D4); the lattice is re-derived.

## The defect

Every other judgement in this lattice is arithmetic. A claim's execution can be disputed by
anyone holding a bond, the dispute bisects to a single step, and the verdict is derived from
operand openings against the class's registered artifact root — a captured panel cannot make a
wrong execution right, because `slash_dissenting_seats` charges a seat for contradicting the
quorum and the *court* charges the quorum for contradicting arithmetic.

**One judgement is decided by a vote: data availability.**

```rust
// palw_state_v2.rs, the ProducerDefaulted arm
let verdicts = palw_seat_verdicts_of_v2(receipts);
builder.slash_dissenting_seats(&claim, &verdicts, false)?;
builder.slash_silent_seats(claim_id, &claim, &verdicts)?;
builder.void_and_slash(*claim_id, &claim, ctx.daa_score, PalwVoidReasonV2::ProducerWithholding)?;
```

`void_and_slash` is terminal and immediate. A quorum that signs `Unavailable` destroys the claim
and takes `claim.reserved` from the producer's bond, and there is no window, no session, and no
evidence anyone must produce. The accusation does not even have to say *what* was withheld — the
object carries receipts and a claim id, nothing more.

The minority case is already handled and handled well: a seat that signs `Valid` against a
defaulting quorum is charged, and one that signs `Unavailable` against a licensing quorum is
charged. **The hole is the majority.** Take `k` of `n` seats — 3 of 5 on the shipped panel — and
every producer on the network is slashable at will, for the price of holding three bonds.

This is also why the bond floor cannot fix it. The 2026-08-30 economics pass priced quorum
capture and found that making it cost real money requires per-seat collateral that prices out
every honest participant (~17.7M MSK/seat to make one capture cost $1,000 at the traded rate).
A cheap seat is only safe if capturing the panel *buys* little, and today it buys a slash.

## What the chain already has

Three facts make the remedy cheap, and all three are already committed:

1. **`claim.trace_root`** — the Merkle root over the ordered trace events, bound to the job
   context by `full_logits_trace_root_v2`. Fixed at claim creation; the producer cannot restate it.
2. **`trace_event_opening_v2` / `trace_event_opening_root_v2`** — a complete membership proof and
   verifier, index-bound at the leaf, odd nodes promoted unchanged (no CVE-2012-2459 ambiguity),
   with the event count taken from the commitment rather than the opening.
3. **`claim.trace_chunk_count`** and **`claim.trace_retention_daa`** — how many pieces the
   obligation covers and how long it lasts.

So "the producer can still show the material" is already a checkable arithmetic statement. Nothing
new needs to be committed; what is missing is a transition that asks the question.

## Decision

`ProducerDefaulted` stops being terminal. It opens a **data-availability session** that resolves
the way the arithmetic court does — by a proof, or by a deadline nobody met.

### D1. An accusation must name what is missing

`ProducerDefaulted` gains `missing_event_index: u32`, validated `< claim.trace_chunk_count`.

An accusation that names nothing cannot be refuted, and one that names everything is not an
accusation. This is the same move ADR-0042 made for court closes: a verdict that carries no proof
is an assertion, and the acceptance layer refuses it.

### D2. Accepting the accusation opens a session, it does not void

The claim moves to a new phase `DefaultDisputed { opened_daa, missing_event_index }` and arms a
deadline at `opened_daa + window_challenge`. The reservation is held (the claim is still live) and
the escrow is neither paid nor destroyed.

`slash_dissenting_seats` still runs at this point — a seat that signed `Valid` against the
accusing quorum has contradicted its own panel, which is true whatever the session decides.
`void_and_slash` does **not**.

### D3. The producer answers by publishing the event

A new object:

```rust
MaterialDisclosed {
    claim: Hash64,
    opening: PalwTraceEventOpeningV2,
    /// What `logits_event_hash_v2` hashes: (phase, phase_step, event_index, n_vocab, logits).
    /// For a class on `tiled_logits_scheme_id_v1` this is the TILED form — see the costs below,
    /// where the flat one is measured and does not fit.
    event: PalwDisclosedEventV1,
    signature: Vec<u8>,           // under the claim's bond key
}
```

Accepted only while the claim is `DefaultDisputed`, and only when all three hold:

* `opening.event_index == the session's missing_event_index` — answering a different index is not
  an answer;
* the event re-hashes to `opening.event_hash` under `logits_event_hash_v2` — the preimage is the
  one the opening names, so a prover cannot open a hash it never has to produce;
* `trace_event_opening_root_v2(claim.trace_chunk_count, &opening) == claim.trace_root` — the event
  is the one this claim committed to, at that index.

The signature is checked for the same reason `CourtDisclosed`'s is: an unsigned disclosure would
let a third party bind the producer to material it never published.

### D4. A verified disclosure refutes the accusation

The claim returns to `ReceiptLicensed { licensed_daa: <the disclosure's DAA> }` and resumes its
path to `Final`. Every seat whose receipt said `Unavailable` is charged `claim.reserved`, capped at
`min_collateral_sompi` exactly as `slash_seat` already caps a dissent — they signed that this data
could not be obtained, and it is now on the chain.

**This is the whole point.** The accusing quorum is not charged for losing a vote; it is charged
for signing a statement the chain can now see is false. That is the same standard the arithmetic
court holds an executor to.

### D5. Silence past the window confirms the default

The sweep finds a `DefaultDisputed` claim whose deadline passed with no accepted disclosure and
does exactly what today's arm does immediately: `void_and_slash(ProducerWithholding)`. A producer
that cannot open one committed event of its own trace has not served the material, and the panel
was right.

## Costs, measured

| | |
|---|---|
| opening, worst case | `4 + 64 + 13x64` = **900 bytes** (`PALW_V2_MAX_TRACE_EVENTS` = 4096, so at most 13 siblings) |
| flat event preimage | `n_vocab x 4` bytes — `logits_event_hash_v2` hashes the whole logits vector |
| ...at Qwen's vocab (151,936) | **607,744 bytes** |
| court close ceiling | `DEFAULT_MAX_CLOSE_BYTES` = **80 KiB**, frozen for testnet-11 |
| added lattice length | `window_challenge`, once per disputed claim |

The opening fits with three orders of magnitude to spare. **The preimage does not, and that is a
measured fact rather than a caveat**: a flat disclosure at Qwen's vocabulary is 7.4x the entire
close budget, so for any class on `tiled_logits_scheme_id_v1` the tiled form is not an optimisation
but the only representable answer — the same conclusion ADR-0046 reached for court closes, where a
flat pin was found to be inadmissible at *any* (tile, context) and the two-tile disclosure is what
made those classes prosecutable at all.

This ADR therefore does not introduce a second disclosure format. `MaterialDisclosed` carries
whatever form the class's `logits_scheme_id` names, bounded by the class's own registered
`max_close_bytes` exactly as a close is, and `verify_class_admission_v2` must refuse a class whose
event cannot be disclosed within that ceiling. A class whose data obligation cannot be adjudicated
is unprosecutable in the DA dimension for the same reason ADR-0049 Decision C refuses one that is
unprosecutable in the arithmetic dimension — and the check belongs in the same place, so a class
cannot pass one gate and fail the other silently.

The lattice bound in `Params::validate` becomes
`2 x (bind + receipt) + challenge + court + challenge`, the last term being this session. It is
already `2 x (bind + receipt) + challenge + court` after the 2026-08-30 redraw.

## What this does not fix

**A producer that publishes late still profits from having withheld.** D4 restores the claim in
full, so withholding until accused, then disclosing, costs the producer only the disclosure. The
panel's wasted verification is not compensated — this ADR deliberately does not invent a payment
rule, for the reason `slash_silent_seats` gives: seats are never paid anywhere in this lattice, and
fixing that is one decision, made once, not a side effect of a remediation. Until it is made, the
honest reading is that D4 refutes an *accusation* and not a *delay*.

**One disclosure refutes one index.** A panel that genuinely could not obtain most of the trace
must accuse at one index and can be refuted at that index. Sampling — accuse at `m` indices, refute
all `m` — is the natural extension and is left out on purpose: `m = 1` is the shape whose costs are
measured above, and a sampling rule needs its own analysis of what `m` buys against `max_close_bytes`.

## Why not the alternatives

**Charge the accusing quorum on any refuted default, without a disclosure.** There is nothing to
refute with — that is the defect.

**Use `PalwMaterialRequest` (protocol 104) re-serving as the evidence.** A seat re-serving material
it heard is hearsay: it proves some node has bytes, not that those bytes are this claim's, and it
puts the verdict back on testimony. The opening puts it on the commitment.

**Raise the panel bond until capture is unaffordable.** Priced at the traded rate this costs the
network its independent seats and buys, per the 2026-08-30 pass, a capture cost still under a
thousand dollars. Cheap seats plus arithmetic beats expensive seats plus votes.

## Consensus impact

`PALW_STATE_V2_VERSION` 13 → 14. New claim phase, new consensus object, and `ProducerDefaulted`
gains a field — the claim schema and the object schema both move, so an old build cannot decode
the state rather than merely disagreeing about it. Deploying is a re-mint, and the fingerprint
moves for testnet-11 (mainnet carries no PALW bundle).

## Security amendment (2026-09-02) — hardening before implementation

Written against the shipped tree of Relaunch 5e, where ADR-0065 Decision 4 is armed from genesis on
testnet-11 (`Unavailable` abstains; the `ProducerDefaulted` quorum is unreachable) and ADR-0072
Decision 8 pins `trace_manifest_root`, `trace_chunk_count` and `trace_retention_daa` by equality.
Six things the Decision above must gain before it is coded, each with the attack it closes.

**SA-1 — The accusation is its own bonded object, singular per claim, inside the retention
window.** Decision 1 rides on `ProducerDefaulted { missing_event_index }`, a quorum verdict that a
D4-armed preset never reaches. Replace it with `DefaultAccused { claim, missing_event_index,
accuser: bond, signature }`: signed by an Active bond that is a seat of that claim's panel or a
bonded challenger; refused if `missing_event_index ≥ trace_chunk_count`, if the claim is outside
`[accepted_daa, trace_retention_daa]`, or if an accusation is already open on the claim. The
accuser's bond reserves `ACCUSATION_EXPOSURE = claim.reserved / seat_count` on the ledger the
claim's own exposure lives on (ADR-0056 Decision 3's shape); a valid disclosure charges it to the
accuser and refunds the producer's disclosure fee out of it. Griefing an honest producer then costs
the griefer more than the producer per attempt, and ten accusations cannot drain one fee float.

**SA-2 — The disclosure is checked by hash arithmetic, never by execution, and it is bounded.**
`MaterialDisclosed` carries the event preimage at `missing_event_index`, its Merkle opening against
the claim's pinned `trace_manifest_root`, and the producer's bond signature. Every validator checks
the opening and the index; none runs a model. Its size is bounded by `max_close_bytes` (80 KiB,
the tiled form), and `derive_court_cost_v1` must include the DA event's worst opening in the
class's priced close, so a class whose event cannot be disclosed inside the budget is refused at
admission (ADR-0049 Decision C) rather than being undefendable afterwards.

**SA-3 — "Silence" is a fold fact on this chain, and the window is wide enough that suppressing
the disclosure needs a majority.** Decision 5 confirms default when no disclosure lands in the
window. That is checkable — unlike ADR-0064 Fact A's "the network was silent" — only because it is
*absence of an object on this chain within `W_disclose` DAA of the accusation's acceptance*,
recomputed from the fold on every branch. Two rules make it safe: the disclosure may ride ANY
block (permissionless carriage, fee-priced, any producer), so excluding it needs every producer for
the whole window; and `W_disclose ≥ 2 ×` the finality window, so a reorg across the deadline cannot
flip the verdict without a finality violation. No node-local timer participates.

**SA-4 — Abstaining seats pay nothing.** Decisions 2 and 4 charge "dissenting" and `Unavailable`
seats. Under ADR-0065 Decision 4 an `Unavailable` vote is an abstention, not a finding, and
charging it re-creates the transport-loss slashing D4 removed (a third of remote seats' verdicts
were transport). Only the accuser — who made a positive, falsifiable claim — pays on refutation.

**SA-5 — Poverty is not default, and the producer's loss is bounded.** A producer that cannot fund
the disclosure's carriage fee draws it from the claim's escrow (the escrow exists to pay the claim's
obligations); the fee is mass-priced like every lifecycle object. On confirmed default the slash
stays `claim.reserved`, never the bond.

**SA-6 — The lattice is re-derived with the new phase.** `DefaultDisputed` adds `W_accuse +
W_disclose` to a claim's life: `2·(bind + receipt) + challenge + court + accuse + disclose ≤
pruning_depth` and `≤ MAX_CLAIM_EXPOSURE_DAA` must hold at every preset, pinned by the existing
lattice test. A phase that outlives the trace's retention is a phase in which the honest producer
was already allowed to delete what it is asked to open.

Invariants: **DA-1** an accusation outside the claim's retention window is refused; **DA-2** a
second open accusation on one claim is refused; **DA-3** a valid disclosure charges the accuser and
nobody else; **DA-4** a disclosure carried by a block the producer did not mine is accepted;
**DA-5** two nodes that saw the disclosure at different wall-clock times reach the same verdict.

## Implementation, 2026-09-02 — the amended form, behind `palw_da_court`

Landed dormant. The fence is a top-level `Params` field, `None` on every shipped preset, so every
fingerprint, identity and state root is byte-identical to the tree before it; `PALW_STATE_V2_VERSION`
stays at 16 for the same reason, and moving it is the *arming* release's job, not this one.

What the code does, against each amendment clause:

* **SA-1** — `PalwConsensusObjectV2::DefaultAccused { claim, missing_event_index, accuser,
  signature }`, riding an ordinary lifecycle transaction. Refused if the index is at or past
  `trace_chunk_count`, if the whole disclose window does not fit inside the claim's own
  `trace_retention_daa`, if the claim is terminal or has no bound panel, if the accuser is the
  producer, is not Active, or is under the registry floor — and if an accusation is already open,
  which is structural: a claim has one phase. The accuser reserves
  `⌈claim.reserved / seat_count⌉` on the claim's own exposure ledger.
* **SA-2** — `MaterialDisclosed { claim, event_index, preimage, opening, signature }`, checked by
  hash arithmetic only: the preimage re-keys to the opening's event hash, and the opening
  reconstructs the claim's pinned `trace_root` at `trace_chunk_count` leaves. Bounded by the ride
  list at `DEFAULT_MAX_CLOSE_BYTES` and at acceptance by the class's own `max_close_bytes`.
* **SA-3** — the phase `DefaultDisputed { accused_daa, … }`, whose deadline is
  `accused_daa + W_disclose` with `W_disclose = window_challenge`, re-derived from the record by
  `assert_deadline_consistency` and `rebuild_deadline_index_v2` on every branch. `validate_palw_v2`
  proves `W_disclose ≥ 2 × finality_depth` past the fence. Carriage is permissionless.
* **SA-4** — only the accuser is charged, capped at `min_collateral_sompi`. No seat is touched.
* **SA-5** — the confirmed default slashes `claim.reserved`, never the bond. **The escrow-funded
  carriage fee is NOT implemented**: this ruleset has no credit primitive (slashed value is burned,
  and the only payment path is the coinbase payout queue keyed by claim id), so a refund would be a
  new payment rule — the one decision this ADR says is made once and not as a side effect. What
  stands in for it is SA-3's permissionless carriage: a producer with no fee can have anyone carry
  its signed disclosure.
* **SA-6** — the DA term enters both lattice bounds (`with_palw_v2_depths` and `validate_palw_v2`)
  and is zero while dormant. Per claim the session is *contained* in the retention obligation, so it
  adds nothing to `MAX_CLAIM_EXPOSURE_DAA`; the existing lattice walk in `palw_fp_devnet_v3` now
  asserts that containment rather than assuming it.

`DefaultDisputed` is the first phase in this ruleset that holds a **second bond's** reservation,
and that is the whole of its accounting risk. A claim under accusation is not terminal, so every arm
that voids a live claim can land on one — the panel's own `ProducerDefaulted`, and a `CourtClosed
{ ExecutorGuilty }` on a session that was already open when the accusation arrived — and none of
them knows what an accusation cost: `release_for_claim` gives back what the *producer* put up. The
release therefore lives in **one place, `write_claim`**, beside the unresolved and work-id indices
and for the reason `write_court` states about the challenger's stake: a release each new arm has to
remember is a release one of them eventually will not. Left stranded it is not merely wrong
accounting — `assert_internal_consistency` rebuilds that ledger from the claims, so it would be
state the fold writes and cannot read back, and the next `load_tip` would refuse the snapshot the
chain had just written, on every node at once.

The same reasoning polices `resumed`, the one field of this state nothing else re-derives. The
accusation arm refuses a terminal claim and refuses an already-disputed one, so the phase it
snapshots is always live and undisputed; the loader asserts that rather than assuming it, because
the alternative is meeting a bad record at a disclosure a window later with nothing left to say
where it came from.

Deliberately not restored from Decision 4: the refuted claim resumes **the phase it held**, not a
fresh `ReceiptLicensed` dated at the disclosure. Restarting the challenge clock would punish a
producer the chain has just proven honest and would let serial accusations hold a claim — and its
reservation — open a window at a time.

**Not shipped, and the reason the fence must stay dormant:** nothing in the tree constructs either
object. Armed with no disclosure responder in the field, every accusation would succeed on silence
and every producer would be slashable for the price of one bond — audit3 H4's shape, and strictly
worse than the captured-panel defect this ADR exists to close.
