# ADR-0062 — The data-availability court: stop a vote from taking a bond

> Renumbered 0059 → 0062 at the 2026-08-30 branch merge: 0059 was taken the same day by the
> 10B premine cap on the parallel line (0060 the liveness doctrine, 0061 zero-seat genesis),
> and this document was the cheaper of the two to move — one file, no code references. The
> original branch commit (`554ca77c`) carries the old number in its message; this file is the
> authority.

Status: **Implemented behind `Params::palw_da_court`, dormant** (the amended form landed 2026-09-02; SA-7 widened the same fence 2026-09-03 — see the implementation sections below). The "Proposed, not landed" reading that stood in this line and in the README until 2026-09-05 was stale. Supersedes nothing; completes ADR-0042 Decision 7.

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

## Second security amendment (2026-09-03) — SA-7: what an accusation costs, what it buys, and what it may not suspend

An adversarial read of the landed SA-1…SA-6 code found the mechanism exploitable in **both**
directions at once, which SA-1…SA-6 do not address because they are about the accusation's *form*
and this is about its *price*. All four are one question, so they are one decision.

**SA-7(a) — A data-availability accusation does not suspend the arithmetic fraud court.** DA and
computation are orthogonal: a producer can serve every byte it committed to and still have computed
the wrong answer. `validate_court_opened_v2` read `claim.phase` directly, so an open
`DefaultDisputed` was not `ReceiptLicensed` and therefore not challengeable — for the *whole
remaining challenge window*, since `W_disclose = window_challenge`. A producer whose arithmetic is
fraudulent therefore had a purchase available to it: have any bond accuse it (its own second bond
will do — nothing in the ruleset makes two bonds strangers), answer the single named index honestly
out of a complete trace, and let the challenge window lapse underneath the session. Price
`min(⌈reserved/seat_count⌉, min_collateral_sompi)`; avoided, a `CourtFraud` conviction worth
`claim.reserved` *and* the escrow, with the fraudulent `pwu` entering `safe_weight` permanently.
Strictly profitable whenever `seat_count ≥ 2`.

So the challenge surface is read **through** the disputed phase, at the phase the accusation found
(`resumed`), and the part of the window the open session has already consumed is added back. A
claim that is not `Final` is challengeable; a claim under accusation cannot be `Final`; therefore a
claim under accusation is challengeable. A court that opens on a disputed claim leaves the
accusation's own deadline armed — disarming it would make opening a court the way to cancel an
accusation.

**SA-7(b) — The claim's clock is paused, which means it is given back.** `W_disclose =
window_challenge` is strictly longer than both `window_receipt` and `window_bind` on every
ConsensusV2 preset (shipped reference bundle: bind 600, receipt 600, challenge 1200). A claim
accused while `PanelBound` and restored to the deadline it held *before* the session therefore came
back to a receipt window that had already lapsed — guaranteed by the inequality, not by timing — and
the next sweep redrew it. The redraw leaves the old panel record in place, so a second accusation
was admissible immediately, and the second honest answer came back to an expired *bind* deadline:
`Voided { BindTimeout }`. Two accusations at the registry floor, and an honest producer that
answered both correctly and on time lost its escrow. A disclosure responder does not help, because
*answering* is what triggers it.

On refutation the resumed phase's anchor therefore advances by exactly the elapsed session —
`bound_daa` for `PanelBound`, `rebound_daa` for `Provisional` (never `accepted_daa`, which anchors
the retention obligation and must not grow because someone accused). `ReceiptLicensed` resumes
**unmoved**: that phase names no obligation the session prevented the producer from meeting — it has
finished and is only waiting — so extending it would only delay an honest `Final` further. The rule
is "return the time the session took *from* the producer", not "extend every window".

**SA-7(c) — One bond's total data-availability liability is its exposure ceiling.** Being `Active`
and at or above the registry floor is a check on the accuser's *state*, and reserving does not
change that state: `write_exposure` moves the exposure ledger and leaves `collateral` alone. So a
single bond holding exactly `min_collateral_sompi` passed the same two checks once per live claim on
the network and froze all *K* of them — in one block if it liked, since each accusation is on a
distinct claim and the per-claim singularity never fires. And because `slash_bond` debits
`min(amount, collateral)` and returns early at zero, the *first* refutation emptied the bond and the
remaining `K−1` refutations cost nothing: **K frozen claims for one registry floor.** The exposure
ceiling the `CourtOpened` arm says "does the counting" is enforced only in
`check_palw_attempt_admission_v2`, i.e. only against a bond that wants to *produce*.

The accusation therefore meets the same ceiling — `reserved_exposure + registration_exposure +
this accusation ≤ collateral × max_exposure_ratio_permille / 1000` — and, with SA-7(d), the figure
counted is one the fold can actually collect, so the K-th accusation is funded exactly like the
first.

**SA-7(c) applies to *both* arms that accuse, and the first pass fixed only one of them.** The
paragraph above quotes the `CourtOpened` arm's claim that "the ceiling it already lives under does
the counting" in order to refute it — and then left that arm exactly as it was. The two arms are one
mechanism: a bonded party freezes somebody else's claim by *reserving* against its own collateral,
and reserving does not touch collateral, so `Active` plus the registry floor is a pair of checks the
same bond passes once per live claim on the network. Opening a court is in fact the *cheaper* of the
two, because the challenger-side close charges `min(claim.reserved, min_collateral_sompi)` and
`slash_bond` clamps at collateral and returns `Ok` early at a zero debit: one bond at the floor could
open a court on every licensed claim in a single block, freeze each of them until its `window_court`
backstop, and pay for at most the first close. Both arms now reserve through one helper
(`reserve_accuser_exposure_v2`) against one ledger and one ceiling, so a bond's total liability for
*accusing* — by either object — is its exposure ceiling. Spelling it once is the fix; two spellings
is how the arms drifted apart in the first place, and one test walks both.

Widening the *same* fence, `palw_da_court`, is what makes this a rule change a network opts into
rather than a silent one: with the fence dormant the `CourtOpened` arm reserves exactly what it
always reserved. The consequence, stated rather than left for a reader to find: **a `ConsensusV2`
network that does not arm `palw_da_court` still has the unbounded court-opening gap**, and closing
it there is a fence decision somebody has to make knowingly.

**SA-7(c) also needs `max_exposure_ratio_permille ≤ 1000`, and now says so in code.** The ceiling is
`collateral × ratio / 1000` while `slash_bond` can never debit more than `collateral` in total. Above
unity a bond holds more concurrent exposure than it can ever pay: the first refutations empty it and
the rest are free — precisely the behaviour this clause removes, restored by a genesis-time constant.
The bound was prose in the fold's margin; `PalwAdmissionParamsV2::new` refused only zero, and
`palw_mode_v2` required the admission and state copies to be *equal*, not bounded. Both constructors
refuse above 1000 now, and `validate_ruleset_shape` refuses it again for a bundle that arrives
deserialized rather than constructed — which is how a ruleset actually reaches a node.

**SA-7(d) — An accusation reserves exactly what the fold can take from it.** `slash_seat` caps the
charge at `min_collateral_sompi`, for the reason SA-4 gives: both factors of `claim.reserved` are
chosen by whoever registered the class, so an uncapped charge would let a registrant make accusing
its own class ruinous and buy itself immunity. The *reservation*, however, was the uncapped
fraction, so for every claim with `reserved / seat_count > min_collateral_sompi` the ledger recorded
a liability that could never be collected and SA-7(c)'s ceiling would have counted money that does
not exist. Reservation and charge are now the same number.

**Not decided here, and the exposure until it is — restated, because the first statement of it
rested on a false fact.** A *correct* accuser is still paid nothing. The reason given here was that
"the only payment path is the coinbase payout queue keyed by claim id, which a voided claim never
enters, and a bond has collateral but no payee script", citing a comment in
`virtual_processor/processor.rs` that said exactly that. **The comment was stale and the second half
of that sentence is wrong**: `PalwBondStateV2` carries `payout_payload`, registration refuses an
empty one, `finalize_claim` writes `PalwPayoutV2 { payload: bond.payout_payload, amount }`, and
nothing on the paying side — neither `pending_payouts_iter` nor the drain — asks which phase enqueued
an entry. The queue is generic; there *is* a payee to resolve, for any bond. (The comment has been
corrected in the same commit as this paragraph.)

So the residual is narrower than it was written. What is missing is not a payment mechanism but a
*supply decision* and the transition-side write that spends it: at a confirmed DA default the claim
is voided and its escrow — carved from the accepted block's own subsidy — is simply never paid, so
funding an accuser out of that escrow moves no new coins and does not touch `slashed`, which
`palw_bond_burn_obligation_v2` holds as a burn obligation. That is one `write_payout` at the void,
keyed by the claim id the void frees, and a rule about how much of the escrow it may take. It is an
ADR-sized decision — who is paid, how much, and what stops a producer collecting it through a second
bond of its own — but "the ruleset has no payment path" was not the reason, and this ADR should not
be read as claiming that any longer.

What is *unchanged* is the incentive statement, and it is the part that carries the deferral:
crediting an accuser's `collateral` still would not move coins (it would raise that bond's exposure
ceiling against nothing), no payout to an accuser exists today, and therefore policing data
availability costs transaction fees
plus a window's use of collateral and returns zero, and the expected value of accusing is negative
for an honest participant. **SA-7 makes the accusation safe to leave armed; it does not make it
attractive to use.** Paying an accuser is a new payment *rule* — the one decision this ADR says is
made deliberately and not as a side effect — and it needs its own ADR alongside SA-5's escrow-funded
carriage fee, which is unimplemented for the same reason; what it does **not** need is new
machinery. Until then the DA
court is a deterrent that depends on someone accusing for a non-monetary reason (a panel seat whose
own material never arrived, or a competitor), and the accuse-window arithmetic bounds what it can
ever be: a claim can absorb at most `retention / W_disclose` sessions (shipped: `(600 + 600 + 1200 +
3000) / 1200 = 4`), each opening one accuser-chosen index, so a producer that deleted a fraction *f*
of its trace is caught with probability about `4f`. It is a bounded spot check, not an availability
guarantee, and this ADR should not be read as claiming otherwise.

Invariants added: **DA-6** an open accusation leaves the claim challengeable at every DAA of the
session; **DA-7** a claim that answers every accusation correctly and on time reaches its own
terminal phase unslashed; **DA-8** a bond cannot hold more exposure than its ceiling for *accusing*,
counting a `DefaultAccused` and a `CourtOpened` against the same ledger and the same figure, and the
ceiling is at most the collateral because the ratio is at most unity;
**DA-9** the reservation an accusation takes equals the charge a refutation collects.

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
  list at `DEFAULT_MAX_CLOSE_BYTES` and at acceptance by `PalwCourtParamsV2::max_close_bytes` —
  the RULESET's one bundle-wide court ceiling, fixed by `palw_ruleset_id_v2`. There is no per-class
  disclosure ceiling on this path and none is read; an earlier draft of this paragraph said "the
  class's own `max_close_bytes`", which is a field this code never consults.
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
reservation — open a window at a time. Under SA-7(b) that phase's *anchor* advances by exactly the
elapsed session, which is a pause and not a restart: the producer gets back the window it had left
and not one DAA more.

### SA-7, 2026-09-03 — the same fence, widened

No new fence and no new state: `palw_da_court` already gates every path SA-7 touches, and
`DefaultDisputed` is unconstructible while it is dormant, so the challenge-surface lookthrough, the
pause credit and the resume rebase are all unreachable on every shipped preset. `PALW_STATE_V2_VERSION`
still stays at 16, and still must move in the release that arms the fence.

* SA-7(a) — `palw_challenge_surface_phase_v2` and `palw_da_paused_daa_v2` (both the identity off the
  fence) in `validate_court_opened_v2`; the `CourtOpened` fold arm's `disarm_deadline` deliberately
  excludes `DefaultDisputed`. Test `an_open_accusation_does_not_close_the_arithmetic_court`.
* SA-7(b) — `resume_claim_after_da_session_v2`. Test
  `two_answered_accusations_do_not_destroy_an_honest_producers_claim`, which walks the exact
  two-session sequence that voided an honest claim before it.
* SA-7(c) — `TransitionBuilder::reserve_accuser_exposure_v2` is the ceiling, and **both** arms that
  accuse call it: the `DefaultAccused` arm always, the `CourtOpened` arm behind `builder.da_court`
  (the same fence, widened — dormant, that arm reserves exactly what it always did). It refuses with
  `AccusationExposureCeiling`, whose `edge` names which object was refused. Tests
  `one_bond_at_the_floor_can_freeze_exactly_one_claim` and — one test walking both arms, so they
  cannot drift apart again — `one_exposure_ceiling_binds_both_arms_that_accuse`, which spends the
  ceiling through the court, tops it up through the accusation, watches both arms refuse the next
  object against the same figure, and then refutes both and asserts every debit landed.
* SA-7(c), the ratio — `PalwAdmissionParamsV2::new` and `PalwConsensusParamsV2::validate_ruleset_shape`
  refuse `max_exposure_ratio_permille > 1000`. Tests
  `an_exposure_ratio_above_unity_is_refused_where_the_value_is_admitted` and
  `an_exposure_ratio_above_unity_does_not_boot` (which smuggles the value past the constructor by
  deserializing it, because that is how a bundle reaches a node).
* SA-7(d) — `palw_da_accusation_exposure_v2`. Test
  `an_accusation_reserves_exactly_what_the_fold_can_take`.

**Not shipped, and the reason the fence must stay dormant — this is a hard precondition, not a
scheduling note.** Nothing in this tree constructs a `MaterialDisclosed`. `palw_panel.rs` answers
`CourtDisclosed` for the arithmetic ladder and **nothing answers `DefaultAccused`**: there is no
producer-side responder that opens the accused index out of the capture the producer already holds.
Armed without one, every accusation wins on silence, and SA-7's ceiling does not bound that case —
a *successful* accuser is never charged (the DA sweep voids and slashes the producer while
`write_claim` releases the accuser's reservation intact), so its collateral is immediately free to
accuse the next claim. The ceiling bounds *concurrent* accusations; sequential winning accusations
are free and unbounded. One floor bond would therefore destroy every live claim on a responder-less
network, in sequence, at the price of postage.

**`palw_da_court` may not be scheduled from this ADR until that responder exists and ships.** Whoever
sets the fence must be able to name the binary that answers an accusation. What building it takes,
concretely: the responder belongs in the node's panel service beside the existing
`CourtDisclosed` path (`kaspad/src/palw_panel.rs`, which already builds and submits that object from
a duty it polls); the material is the producer's own trace capture, the same
one `trace_event_merkle_root_v2` was computed over at attempt time, so answering is opening event
`missing_event_index` — preimage plus Merkle path — and nothing needs to be recomputed or re-executed;
the object reaches the chain as an ordinary lifecycle transaction, exactly like the `CourtDisclosed`
the same service already submits. The two open questions are retention on the producer side (the
capture must still be on disk for the whole `trace_retention_daa`, which is a node-operations
property nothing currently verifies) and the fee float the responder spends to answer. Neither is in
this batch, and `PALW_STATE_V2_VERSION` must move in the same release that arms the fence.

## Implementation, 2026-09-06 — the disclosure the shipped commitments allow, and a responder (`01e034c4`, `f7363db9`)

The 2026-09-02 implementation verified a disclosed event against a Merkle root over event hashes.
No shipped class commits such a root — every claim's `trace_root` is the flat
`base0_logits_trace_root_v1` or the tiled `tiled_logits_trace_root_v1` — so, armed, every
accusation would have won by silence: the precondition this ADR names ("a responder") had a second,
unnamed half, the arithmetic. SA-2's own words are what is implemented now: the disclosure is
"the event's own bytes in whatever form the class's `logits_scheme_id` names" —
`PalwTraceEventDisclosureV1::{Flat, Tiled, OutOfRange}`, each pinned to the claim's `trace_root` and
`execution_root` through the binding, bounded by the ruleset's close ceiling, signed over a keyed
digest. The event index is `(row << 8) | tile`, bounded by `trace_chunk_count × 256`.

The responder is the panel's `palw_da_duties_v2` → `PalwBackend::disclose_trace_event` (the floor
and both model tiers), filed as a lifecycle carrier inside the disclose window. The accuser is an
operator: `misaka palw da-accuse` builds and signs `DefaultAccused`; `palw submit-object` files it.
No node accuses on its own — ADR-0065 D4 already answers "I never received it".

Armed on a carded mainnet from block one (`mainnet_card_base_v1`), with the withdrawal delay derived
past `liability + accuse + disclose` (`palw_v2_bond_outlasting_da_court`; `validate_palw_v2` refuses
a delay inside that sum wherever the court is armed). testnet-11 and devnet stay dormant; arming
them requires the state-version move this ADR asks for, and a test now says so. Design record:
`docs/palw-private-prompts-design-2026-09-05.md`.

## Arming on testnet-11, 2026-09-06 — a scheduled fence, and what the state version really means

Scheduled at **DAA 1,900** on testnet-11 (`PALW_RC_DA_COURT_FENCE_DAA`), together with ADR-0077
Decision 16's `palw_panel_da` and ADR-0087/0088/0089's three model fences. The chain is live and
carries value, so this is ADR-0083's path (a) — a height — and not the re-mint this ADR's
Consensus-impact section assumed.

**The state version does not move, and the sentence that said it must was written for a re-mint.**
`PALW_STATE_V2_VERSION` is hashed into `state_root`, and every header commits `palw_state_root`, so
moving it invalidates the chain from block one: on a live network the bump IS the re-mint rather
than a step towards one. What the bump was for — an old build must not fold under new rules and
call the result the same state — is bought here by the fence itself: below 1,900 the fold is
byte-identical (`the_da_court_fence_off_is_byte_identical`), and past it an un-upgraded node forks
VISIBLY, because `palw_da_court` now arms the fork-id gate (`fork_id_gate_fences_v1`), which
refuses a peer whose schedule is not this node's. The rule as it now stands, pinned by
`a_scheduled_da_court_needs_no_state_version_move_and_a_genesis_one_would`: a court armed at
genesis on a preset that carries history needs the version move (it is a re-mint); a scheduled one
does not, and must arm the gate instead.

**Two values ride with the fence, and neither is in the ruleset.**

* *The withdrawal delay.* A defaulted claim adds `accuse + disclose` to the path from acceptance to
  a verdict, so past the fence a retiring bond stays locked for the bundle's delay plus that
  lattice (`palw_v2_bond_withdrawal_delay_at_v1`, resolved at each block by the fold). Rewriting the
  bundle's own `withdrawal_delay` would have moved `palw_ruleset_id_v2` — every fingerprint and
  identity with it — and a live fleet cannot upgrade one host at a time through an identity change.
  This closes the mainnet audit's "should fix" at `palw_mode_v2.rs:938`.
* *The pruning horizon.* SA-6's lattice grows by the same term, so the depth goes 6,602 → 12,002.
  The depth is not a fence and is hashed into `consensus_params_id` directly, so
  `consensus_identity_id` normalises it back with the fence: two builds that differ only about a
  future height announce one identity and stay peers, which is the whole rollout. Proven by
  `scheduling_the_da_court_moves_the_fingerprint_and_leaves_the_identity_alone`, which reconstructs
  the running fleet's ruleset and pins its fingerprint to the one testnet-11's nodes print.

The horizon change is invisible in substance until blue score 6,602 — a depth this chain has never
reached (tip 1,619 at 6.2 DAA/h) — so no header's pruning point differs under either build.

testnet-11 keeps the FLAT prompt commitment: `palw_prompt_ids_merkle` is genesis-only
(`validate_palw_v2`), because no reader of a `prompt_token_ids_hash` holds a job's anchor height.

### Rollout record, 2026-09-06 05:33–05:47Z

Built on ibm from `9f2aad95` (`--features evm`, 14m19s), kaspad sha256 `7181cc07eb57a2f8`; the
previous binary (`b6cf47ea3a66480b`) is kept on each host as `/root/t11/kaspad.pre-flagday-b6cf47ea`.
Restarted in order — ibm node0, ibm node1, .113 node, .113 pool slots 02/04/05/06, 5.104 seat2 —
each confirming `Consensus params fingerprint: b511dd1e…` before the next. After the roll: node0
7 peers, .113 9 peers, both at DAA 1,627, no panics, no refusals inside the fleet.

**What the roll measured, and it is not what the fork-id module's doc promises.** Adding `1900` to
the schedule partitions old from new IMMEDIATELY. The new build keeps an old peer ("keeping the
peer"); the old build does not keep the new one — its gate is already armed by ADR-0083's fence, the
new node announces `next = 1900`, and 1900 is not on the old schedule, so the old side rejects and
closes. Six third-party seats were cut off within four minutes of the first restart
(164.68.119.212, 217.178.131.170, 113.155.23.105, 60.114.127.4, 207.180.230.3, 121.81.248.189).
The height's lead time buys the FLEET an ordering; it does not buy the network a grace period, and
anyone running a testnet-11 seat has to upgrade now rather than before DAA 1,900. Recorded at
`fork_id_gate_fences_v1`, where the next operator will read it.

One node on the old build is deliberately untouched: `/root/misakas-user/target/release/kaspad`
on .113 (`--appdir=/root/palw-user/appdir`, `--palw-register-class=Qwen/Qwen3.8-27B/`), the manual
node from the Qwen3.8-27B add-model walk. It is the operator's experiment, and restarting it is
theirs to time.
