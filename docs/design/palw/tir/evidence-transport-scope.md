# IR court evidence for large classes — scope (2026-09-28)

Status: scope only (no code). Follow-up lane after the F7 node update. Owner: tir-node, with Phase F
for the consensus half.

## Problem

Every IR court move a challenger files is an assertion about the ACCUSED's commitments, and today the
node reads those out of the accused capture (`TirCaptureV1`) held in the panel's material pool. The
pool is fed by gossip, capped at `PALW_MATERIAL_MAX_BYTES` = 16 MiB (`protocol/flows/src/palw_gossip.rs`).
The D-F1 class (Qwen2.5-1.5B A16, 512 positions, logits tile 1,024) does not fit:

| capture | size | why |
|---|---|---|
| honest attempt (a fold) | ≈ 39.5 MB | the committed logits rows ride whole: 65 rows × 151,936 lanes × 4 B |
| drill tamper (always dense) | ≈ 625 MB | every leaf preimage |

So for any class of this size no seat holds the accused capture. A lie there is refused its licence (the
seats' replays do not match) but is **never convicted**: the one-move pass (`TirShardCourtAccused`),
the F6 closes, F7's named-leaf challenge and F7's challenger bottom all file nothing. What already works
without the capture: licence votes (a replay compares roots), the IR DA court's `TirEvent` answers (one
row at a time), and every move the RESPONDER makes (from its own capture, or by re-execution).

## What a challenger actually needs

The one-move case names the FIRST leaf at which the accused parts from the challenger's own execution,
so every leaf before it is the challenger's own. `TirEvidenceV1::challenger` already builds a close
from the challenger's own leaves plus ONE accused leaf and the committed trace. Per door:

- cone close / F7 named leaf: the accused's leaf `L` (its preimage, at most one tile) and its opening under
  the step root (about 1.4 KB for a 2^21-leaf ladder);
- F7 bottom: the history operands of one `h_tile`, all before `L`, so the challenger's own; plus leaf `L`,
  which is already on chain in the accused's root claim (`PalwTirRootClaimV1::finalize`);
- logits and decode-token doors: one row's trace tile and its openings, plus the generated ids (the
  `TirEvent` disclosure's shape).

That is kilobytes per accusation, not the capture.

## Options

- **A. Raise the cap or stream captures.** 40–625 MB per claim per seat. ADR-0084 measured 748 MB × 5 peers
  tearing down the p2p flows. Rejected.
- **B. A served IR annex (ADR-0084's shape, consensus-inert).** On request, the producer serves one small
  authenticated object per `(claim, leaf)`: leaf `L`'s preimage and opening, the row's trace tile and
  openings, and the ids. A seat pulls it once its replay parts from the claim, then builds its accusation
  over `TirEvidenceV1::challenger`. Node-only, no fence. But a lying producer can simply not serve it.
- **C. A DA-demanded step leaf (consensus).** A new IR DA unit `TirStepLeaf { index }`: the accused must
  disclose leaf `index` (preimage and opening) inside `W_disclose`, or it is defaulted, which is a
  conviction under the DA court's rules. This closes B's withholding hole. It needs a fence (a new DA
  unit variant, acceptance and fold) and the held-DA carrier rules.
- **D. Read the chain for F7.** The challenger's bottom takes leaf `L` from the accused's own root claim
  on chain (as the attention route reads its filings, `attn_root_filings_from_chain_v1`). Node-only.

## Recommendation

**B as the fast path, C as the enforcement, and D for F7's bottom.** A seat pulls the annex first. If
the producer does not serve it before the accusation's due date, the seat files the `TirStepLeaf` demand
and accuses from the disclosure; the producer's silence is the default. Everything the seat files is
built over the existing `TirEvidenceV1::challenger` store, so no new court object is needed. The
one-move accusation and every close stay exactly as the chain adjudicates them today.

## Estimate

| part | who | size |
|---|---|---|
| B: annex object, serve/pull on the gossip lane, panel wiring, tests | tir-node | ~1 day |
| C: `TirStepLeaf` unit, acceptance + fold + fence (Phase F); node demand/answer (tir-node) | both | ~2–3 days, rides the next t12 fence |
| D: F7 bottom from the on-chain root claim | tir-node | ~0.5 day |
| drill: a D-F1 lie convicted live (B, then C with the producer withholding) | tir-node | ~0.5 day |

Until this lands, Stage 2's live court battery runs on a small IR class whose captures fit under 16 MiB
(coordinator, 2026-09-28). D-F1 keeps Active → claims → Final and the offline `palw-class certify` of
every kind.
