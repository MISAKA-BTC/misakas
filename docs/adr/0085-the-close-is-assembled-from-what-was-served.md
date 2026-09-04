# ADR-0085: The close is assembled from what the executor served — a disputed tile, not a capture

**Status:** PROPOSED (2026-09-04). Design complete; implementation NOT started (see §7). Consensus-inert
by construction: the refutation object (`PalwExecutionStepRefutationV1`), the adjudicator
(`check_execution_step_refutation_v1`) and every court object are unchanged; what changes is how a
party that holds no capture ASSEMBLES the close, and what the executor serves it.
**Builds on:** ADR-0084 (the seat needs the answer's ids, never the history; the material lane
serves the envelope for a capture over the cap), ADR-0077 Decision 8 / SA-2 (the interval lane,
authenticated), ADR-0082 Decisions 4, 7 and 9 (the tile-addressed bottom; the fold; the seat
recomputes the cache), ADR-0073 Decision 1c (a free prompt is carried into the close), audit3 S-01
(the close is assembled from the ACCUSED's bytes by whichever party makes it).
**Amends:** nothing. ADR-0084 §8 named this as U-07c and left it undecided; this decides it.

## 1. What was measured

On the ADR-0084 branch, a v5 attempt claim's material is **784,693,059 bytes** (the loopback
devnet, 2026-09-04 19:30 JST — the card's 748 MB to the byte), the executor stages a 336-byte
`ATA1` beside it, the transport refuses to announce it, and a seat obtains the envelope, binds it
and can certify without the capture. The court cannot close the same claim from the challenger's
side. `kaspad/src/palw_panel.rs`'s terminal arm reads:

```
let Some(accused) = accused_capture.as_deref() else {
    // **And ASK for it.** … pull_for_close.push(duty.claim_id) …
    "the close needs the ACCUSED capture and this node holds none — pulling"
```

and the pull now answers with the envelope, which `verify_material` does not match, so the arm
stalls at that line until `window_court` ends. `refutation_for_free_prompt_index(&accused_bytes,
index, ids)` (`qwen25_a16_backend.rs::refutation_with_prompt`) needs the WHOLE retention: it
re-derives every step tile (`tiles_from_material_v1`, which on a fold replays the run), the rows
root over every logits row, and the checkpoint chunks. For a class over the cap the only party that
holds those bytes is the accused. An honest executor closes its own case; a dishonest one does not
close it, and nobody else can. ADR-0084 moved that failure from certification to the court; this
ADR removes it from the court.

## 2. What a close actually needs, term by term

`base0_refutation_from_capture_capped_v1` builds the refutation from six things. Where each can
come from when the challenger holds no capture:

| term | in the refutation | source without the capture |
|---|---|---|
| the binding | `binding` | every interval opening carries it (`Base0FpIntervalOpeningV{1,2}.binding`), and it is what `execution_root` commits |
| the target's output tile | `output_preimage`, `output_opening` | **the accused's, and only the accused's**: for a real fraud the challenger's recompute differs at exactly this leaf, so its own tile opens against nothing. Must be SERVED. The opening path is derivable from the interval's range opening (leaf hashes + siblings, `step_merkle_range_siblings_v1`) once the leaf hash is known |
| the activation inputs (prior tiles of the same interval) | `inputs` rows opened against the step leg root | the challenger's own replay of the interval (`base0_fp_replay_interval_v1` with tiles captured), CHECKED against the accused's committed leaf hashes in the range opening — the dissection stops at the FIRST disagreement, so every prior leaf of the accused equals the challenger's; a prior leaf that does not is a different first fault, and the seat's `Fault { leaf_index }` names it |
| the KV anchor (checkpoint operands) | `kv_checkpoint` | the challenger's recomputed state at the interval's checkpoint (ADR-0082 D9), whose tiled root the seat already compared to the accused's committed checkpoint leaf; the leaf and its opening come with a V2 opening (`Base0FpCheckpointClaimV1`) |
| the weights | `operand_openings_for` | the challenger's own artifact, against the class root — unchanged |
| the decode pin | `decode_tokens: TiledV1 { rows_root, generated_token_ids }` | `generated_token_ids` off the ADR-0084 envelope; `rows_root` is a 64-byte value the accused committed inside `trace_root` (`tiled_logits_outer_root_v1(ctx, rows, rows_root, generated)`) and is verifiable against the binding by `check_tiled_decode_pin`. Must be SERVED — it is a function of every row |
| the prompt | `prompt_token_ids` | the envelope (`FPA1`) or the anchor's derivation (attempt lane) — ADR-0084 |

So exactly two things must come from the accused: **the disputed tile's preimage** and **the rows
root**. Everything else the challenger recomputes and checks against roots the accused committed.

## 3. Decisions

**Decision 1 — the interval opening gains a close annex, served on the same authenticated lane.**
`Base0FpIntervalOpeningV3` = V2 + `close: Option<Base0FpCloseAnnexV1 { rows_root: Hash64,
disputed: Vec<(u64, PalwStepTileLeafV1)> }>`. The executor's `open_retained_interval` consults the
chain for open court sessions on the claim (`palw_court_duties_v2` names the session and its
`terminal_index`); when a terminal index falls inside the interval being opened, the annex carries
that leaf's tile and the rows root. No new P2P message and no new request tag: a challenger asks
for the interval containing the terminal index over the existing `PalwIntervalOpeningRequest`,
and the answer is one opening, bounded by the same cap (`PALW_INTERVAL_OPENING_MAX_BYTES`; a tile
is one `PalwStepTileLeafV1`, kilobytes). A V3 opening without an annex is a V2 opening to every
existing reader: the seat's replay ignores the annex.

**Decision 2 — the refutation is assembled from an opening and the challenger's own replay.**
`base0_refutation_from_opening_v1(profile, ctx, opening_v3, replay: &Base0StepTilesV1 (the
challenger's tiles for the interval, hashed and CHECKED leaf by leaf against `opening.range.
leaf_hashes` up to the target), checkpoint_state, target, prompt_ids, cap)`:
1. every leaf hash of the challenger's replay below the target must equal the accused's committed
   hash in the range opening — otherwise the first unequal leaf is the real target and this call
   refuses by name (the caller reopens the court's question there);
2. the output tile is the annex's, its opening is derived from the range opening;
3. the inputs are the challenger's prior tiles with openings derived the same way; the KV anchor
   is the recomputed state's chunks opened against the checkpoint leaf the V2 anchor names;
4. the pin is `TiledV1 { rows_root: annex.rows_root, generated_token_ids }`, which
   `check_tiled_decode_pin` binds to the binding before any arithmetic.
The object this produces is byte-for-byte the object the capture path produces for the same
(claim, index) — pinned by a test that builds both from one fixture run.

**Decision 3 — the terminal arm tries the opening path before it pulls.** When the closer holds no
matching capture: derive the interval containing `terminal_index`, request it (the interval lane
the seat already drives), and on arrival assemble with Decision 2. The whole-capture pull stays as
the fallback for classes whose captures fit. The stall reason changes from "holds none — pulling"
to "waiting for the interval's close annex", which is a fact about the accused's serving.

**Decision 4 — a seat's `Fault { leaf_index }` is a court case it can prosecute.** The seat that
found the first unequal leaf during its interval replay already holds the challenger's side of
Decision 2's inputs (its own tiles for that interval and its recomputed state). ADR-0077 Decision 8
says a sampled verdict never slashes; this ADR does not change that. It says the seat may open a
court AT that leaf with `--palw-challenge`, and Decision 3 lets it close.

## 4. Costs

* Per close: one interval opening (≤ 4 MiB by the lane's cap, kilobytes in practice) plus the
  challenger's replay of one interval with tiles captured — the interval's rows, not the run's.
* Executor: one chain read (open sessions on the claim) per interval served; one tile per open
  session in the interval.
* Nothing on chain moves.

## 5. Invariants the tests must hold

```
X1  For one fixture run, base0_refutation_from_opening_v1 and base0_refutation_from_capture_capped_v1
    produce byte-identical refutations at every main-step leaf, on both retention forms.
X2  A tampered tile at leaf t: the seat's replay names Fault { t }; the refutation assembled from
    an opening whose annex carries the accused's tile at t convicts under
    check_execution_step_refutation_v1; the same assembly with the challenger's own tile at t
    refuses to assemble (its hash is not the committed one).
X3  A V3 opening without an annex verifies as a V2 opening on every existing seat path.
X4  The annex is served only for a leaf an open court session names; a request for any other
    interval carries none.
```

## 6. Order of work

1. `Base0FpIntervalOpeningV3` + `Base0FpCloseAnnexV1`, encode/decode, X3.
2. Per-interval replay with tiles captured (the fold's `dense_capture_from_fold_v1` restricted to
   one interval), and the leaf-path derivation from a range opening.
3. `base0_refutation_from_opening_v1`, X1, X2.
4. Executor: the annex in `open_retained_interval` (chain read of open sessions), X4.
5. Challenger: the terminal arm's opening path (Decision 3); Decision 4's seat-opened court.

## 7. Implementation record

Not started. Written on `palw-adr0084-served-answer` beside ADR-0084's implementation so the
follow-up has its design before the fleet rebuild; §6 items 1–3 are pure base0 work with fixtures
that exist (`every_qwen36_leaf_adjudicates_and_a_tampered_one_convicts` and the A16 sweep), items
4–5 are node work.

## 8. What is deliberately not decided

* Whether the annex should ride the answer envelope instead (`rows_root` could; the disputed tile
  cannot, because it is not known until a court names a leaf).
* The attention dissection's bottom (`PalwAttnDissectBottomV1`) already opens tiles on chain for a
  fused node; whether Decision 2 should read the bottom disclosure for those nodes instead of the
  annex is left to the implementation, which will find out which is smaller.

## 9. Number hygiene

This is ADR-0085; ADR-0084 is the last on this branch. A concurrent claimant renumbers the later
writer, per ADR-0036 Decision 5.
