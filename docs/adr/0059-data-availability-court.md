# ADR-0059 — The data-availability court: stop a vote from taking a bond

Status: **Proposed** (2026-08-30). Supersedes nothing; completes ADR-0042 Decision 7.

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
