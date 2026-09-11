# ADR-0111 — A seat may demand the committed leaf it needs to judge

* Status: PROPOSED 2026-09-11 on `feat/adr-0103-held-context` (continuing ADR-0103 at `36f115c4`),
  written from the operator's decision on ADR-0103 §10.5; **IMPLEMENTED the same day (§8), both
  paths drilled to a slash on a live devnet**. Nothing is armed: the new unit rides the held regime
  (`Params::palw_held_context`, `None` on every shipped preset) and ADR-0062's court, so no shipped
  fingerprint, identity, schedule or fork id moves. Testnet-11 does not move.
* Builds on: [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (Decision 1: the one-move court is the court; §10.3 item 11: the prompt tile rides the accusation;
  §10.5: the gap this ADR closes), [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
  (the one-move court and its false-accusation charge), [0062](0062-data-availability-court.md)
  (an accusation names what is missing; SA-2 an answer is hash arithmetic; SA-4 a refuted accuser
  pays; Decision 5 silence is the default), [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md)
  Decision 8 and SA-2 (the seat's draw; the authenticated interval lane),
  [0086](0086-the-opening-carries-the-fold-not-the-leaves.md) Decision 6 (a block's leaves name the
  leaf), 0085 §6 item 4 (the close annex: the committed tiles of a disputed leaf),
  [0081](0081-long-context-the-input-is-a-state-chain.md) Decision 3 (the prompt's tiled root).
* Amends: ADR-0103 Decision 1 (who holds a refutation's inputs) and §10.3 item 8 (the
  data-availability court is required at the fence after all — §2 Decision 5); ADR-0062 Decision 1
  (what an accusation may name: a leaf's committed evidence); ADR-0085 §6 item 4 (the annex is
  served for a leaf a seat names, not only for a leaf an open session names). Supersedes nothing.

## 0. The sentence this ADR is

**A seat need not retain the whole capture. A committed leaf needed by a valid court challenge
must be obtainable from the executor. The fast path is a signed off-chain leaf-evidence request.
Failure of that path opens a bounded on-chain availability request. Failure to open the committed
leaf before the deadline voids the claim. Opening it feeds the existing one-move adjudicator; it
does not create a second execution court.**

## 1. What was found

ADR-0103's live drill convicted a tampered free-prompt claim under the held fence (§10.4, run 2),
and every seat that convicted held the claim's whole capture. At the widths ADR-0103 is for, no
seat does. What a seat holds there, and what the court needs, do not meet:

* **A refutation reveals committed tiles.** The one-move court recomputes a leaf from its committed
  inputs and compares the committed output, so the accusation carries the output tile and every
  input row, each opened against the step root, plus the artifact rows and the prompt tile
  (ADR-0100, ADR-0103 §10.3 item 11).
* **A seat holds hashes.** An interval opening carries the interval's committed leaves as hashes
  (block roots where the capture is retained sparse), and the block-leaves lane carries leaf hashes
  (ADR-0086 Decisions 1 and 6). A seat can NAME the leaf off the chain and holds none of its tiles.
* **The executor serves tiles only to a session.** The close annex carries a disputed leaf's tiles
  for a leaf an open court session names (ADR-0085 §6 item 4, "read off the chain, never off the
  request"), and under the held fence no session opens (ADR-0103 Decision 1).
* **The held DA court cannot compel them.** Its units are a prompt tile, a state chunk and a range
  of leaf hashes (ADR-0103 Decision 4) — none is a step tile.

So a liar that commits one wrong leaf is named and not convicted. Serving the tiles on request is
not enough by itself: an honest executor answers, and a liar simply does not.

## 2. Decisions

**Decision 1 — the unit is a leaf's evidence, and it is the one-move court's object.**
`PalwLeafEvidenceV1` is a refutation's committed half — the output tile and its opening, the
canonical input rows and their run siblings, the KV anchor where the class checkpoints, the decode
pin where the leaf is a decode gather — plus the artifact rows its recomputation reads (opened
against the class root) and the prompt tile its gather reads (ADR-0103 §10.3 item 11's carriage).
It is exactly a `ShardCourtAccused` accusation's content without the accuser. One builder produces
it and one function adjudicates it (`palw_one_move_verdict_v1`), whether it arrives inside an
accusation or inside a disclosure; there is no second court. It carries no prompt list and no
weights beyond the rows the recomputation reads, and it is bounded by the court's close ceiling.

**Decision 2 — the fast path: a signed leaf request on the interval lane.** The seat asks the
executor for the evidence of the leaf it named, on the transport that already carries its interval
requests (ADR-0077 Decision 8): the request index sets bit 29 above the leaf's interval, and the
leaf itself rides a new field that the request's signature binds. The executor answers with the
evidence built from its retention by ADR-0085's path — the leaf's interval replayed from its
anchor, the annex for that one leaf, the refutation assembled from it — so an honest executor's
answer costs one interval's replay, never the job's. (Where that path cannot serve the leaf — a
family that commits its logits flat, or an interval whose committed leaves are not the executor's
own replay, which is a liar's — the builder takes the whole-capture prover, ADR-0085 X1's same
object, at the cost of the executor's own retention.) It is served only to a key the lane already
authorises for the claim, charged to that bond's request rate and throttled per `(peer, claim,
request)` exactly as every interval request is. The seat runs Decision 1's verdict at its own artifact root:
guilty, it files `ShardCourtAccused` (ADR-0100); not guilty, it files nothing and says that its
replay and the court disagree, which is a determinism fault and not a claim's.

**Decision 3 — the slow path: a demand on chain, bounded by chain facts.** When the fast path
does not answer, the seat files `DefaultAccusedHeld` naming `PalwHeldMissingV1::StepLeaf { leaf }`.
It is admitted only when:

* the accuser is a seat of the claim's bound panel;
* the leaf lies in one of the intervals that seat's draw assigned it —
  `palw_fp_interval_draw_v1` over the claim's panel anchor, the seat's index and the claim's own
  interval count, derived from the binding the demand carries — so a seat can demand only a leaf the
  chain told it to check, never one it chose;
* the leaf is not a fused-attention site (that leaf's terminal is ADR-0103 Decision 5's
  dissection, whose responder is already clocked);
* the seat has not demanded on this claim before, for the claim's whole life;
* and every gate of ADR-0062's court holds: an Active bond at or above the floor, not the claim's
  own, a bound panel, the whole disclose window inside the claim's retention, one session at a time.

**Decision 4 — the answer is an adjudication.** The executor answers with `MaterialDisclosedHeld`
carrying `PalwHeldDisclosureV1::StepLeaf { evidence }`, signed by the claim's bond. The fold runs
Decision 1's verdict on it at the class's artifact root and the ruleset's ladder:

* `ExecutorGuilty` — the claim voids `CourtFraud` and its executor is slashed exactly as a
  `ShardCourtAccused` conviction does it; the accuser's stake comes back.
* `FalseAccusation` — the leaf recomputes: the session closes refuted, the accuser pays what it
  reserved (ADR-0062 SA-4), and the claim resumes.
* anything else — the evidence does not adjudicate, so it is not an answer; the session runs on.

No answer inside `W_disclose` is ADR-0062 Decision 5's default: `ProducerWithholding`, void and
slash. **Hiding the evidence becomes the conviction.**

**Decision 5 — the held regime needs the data-availability court.** ADR-0103 §10.3 item 8 left the
court optional at the fence. Without it Decision 3 does not exist and a withheld leaf is
unprosecutable, so `palw_held_context` now refuses to arm unless `palw_da_court` and
`palw_fp_da_pins` are armed at or below its height (the pins are what make a free-prompt claim's
retention obligation a chain fact rather than the producer's number), and `palw_held_context_mint_v1`
arms both from genesis.

**Decision 6 — the node carries both halves, and they ship together.** A seat that names a leaf and
holds no capture it could accuse from asks the fast path; if no answer arrives within
`PALW_LEAF_EVIDENCE_FAST_PATH_DAA_V1`, it files the demand. An executor answers leaf requests, and
answers every held demand its retention can answer — a prompt tile, a state chunk, a range, a
leaf's evidence — because an executor that cannot answer is one the court slashes. The answering
half is part of the court, not an optional extra.

**Decision 7 — the bound on an executor's burden is the chain's, not the requester's.** At most one
demand per seat per claim, each for a leaf the chain assigned that seat, each answered by one object
inside the close ceiling: per claim, at most the panel's size in disclosures. A requester pays the
carrier of its demand and stakes what an accuser stakes; an executor that is honest and answers is
paid back by the charge on the requester.

## 3. What this costs

* **Chain, honest path:** nothing. The fast path is off chain.
* **Chain, withheld leaf:** one demand (a binding and a leaf index) and one disclosure (at most the
  close ceiling), or one default.
* **Executor:** one interval replay per leaf request, and the same to build a disclosure.
* **Seat:** one request and one one-move verdict, at its own artifact root.

## 4. Invariants the tests must hold

```
1  One verdict: a leaf's evidence adjudicates identically inside ShardCourtAccused and inside a
   disclosure, and the fast and slow paths carry the same evidence bytes for one (claim, leaf).
2  A demand is refused by name from a bond that is not a seat of the claim, for a leaf outside the
   seat's drawn intervals, for a fused-attention leaf, a second time by the same seat, and while
   the held fence or the DA court is dormant.
3  A disclosure of guilty evidence voids the claim CourtFraud; of honest evidence resumes it and
   charges the accuser; of evidence that does not adjudicate is no answer; silence past W_disclose
   voids it ProducerWithholding.
4  palw_held_context refuses to arm without palw_da_court and palw_fp_da_pins at or below it; the
   mint arms them; both shipped fingerprints are byte-identical.
5  Live: a seat that holds no capture convicts a tampered claim through the fast path, and through
   the slow path when the executor refuses the fast one.
```

## 5. Order of work

1. The evidence and the one verdict (core); the unit, its admission and its adjudication (fold and
   acceptance); the fence rule and the mint.
2. The duty the executor answers (the held unit on `PalwDaDutyV2`); the executor's answers.
3. The leaf request on the interval lane; the seat's fast path, then its demand.
4. The drill: a seat without the capture, then an executor that refuses the fast path.

## 6. What is deliberately not decided

* **Activating a wider context.** How a context limit is armed from reproducible public evidence
  rather than a maintainer's workstation is the next ADR's (the verification vectors; §7).
* **A fused leaf's evidence.** Its terminal is the dissection (ADR-0103 Decision 5); the responder
  is clocked there, and silence already convicts.
* **Who pays a seat's fetch bandwidth** (ADR-0103 §8), unchanged.

## 7. Number hygiene

0104 (the close cut, `fix/panel-pays-consensus-rent`), 0105 (the heartbeat, on
`docs/renumber-heartbeat-adr-0105`), 0106 (the streaming inventory, `feat/adr-0099-sharded-seat`)
and 0107 (share growth, `fix/share-growth-counts-final-work`) are resident on other branches as of
2026-09-11. The operator's draft called this ADR and the verification-vector one 0104 and 0105.

This ADR has been renumbered twice on 2026-09-11, each time by the rule it states:

1. It was first committed as 0108 (`e251c751`, 15:19:45 +0900), 102 seconds after `f608a066`
   (15:18:03) committed ADR-0108, "an extension is a manifest the verifier recomputes", on a
   concurrent session's `feat/adr-0108-extension-envelope`. That made this ADR the later writer,
   and it moved to 0109 (`8856efc0`, 16:45:14).
2. At 17:02:16 the bridge-liveness ADR, "a lock is its own claim, and finality is a label, not a
   pause", was committed as 0109 on `feat/adr-0109-bridge-liveness`. At 17:37 it and ADR-0108 were
   merged to `origin/main`. This ADR was the earlier writer of 0109 by seventeen minutes, but the
   other one is published and this one was not. An unpublished document moves for nothing, and a
   published one moves for every reader, so this one moved again. The number it took is 0111,
   because 0110 was already this branch's verification-vector ADR, whose vector names embed its
   number.

Every reference on this branch moved with each renumbering: code comments, log lines and the drill's
patterns. The commit subjects of the day still say `adr-0108` and `adr-0109`.

## 8. Implementation record (2026-09-11)

Built on `feat/adr-0103-held-context`, continuing ADR-0103 at `36f115c4`: commits `e251c751` …
the branch head. **Nothing on a shipped preset moves**: the unit, its admission and its
adjudication ride the held regime and ADR-0062's court, both `None` on every shipped preset, so
both shipped fingerprints are byte-identical (`shipped_presets_have_pinned_fingerprints` green) and
testnet-11 does not move. The one rule that changed is the held fence's own (Decision 5), which no
shipped preset arms.

### 8.1 What each Decision became

| Decision | where it lives | what pins it |
|---|---|---|
| **1** one unit, one verdict | `PalwLeafEvidenceV1` and `palw_one_move_verdict_v1` (`palw_shard_court_v1.rs`); `palw_shard_court_verdict_v1` is the shape check plus the same function; `palw_leaf_evidence_from_capture_v1` (`palw_leaf_evidence_v1.rs`) the one builder — the annex route, the whole-capture prover where the family refuses it, the artifact rows, the prompt carriage | `the_executors_leaf_evidence_is_the_one_move_object_by_either_route` (guilty convicts, honest clears, X1 equality, a decode leaf, one devnet carrier) |
| **2** the fast path | request index bit 29 (`palw_leaf_evidence_request_index_v1`), `leafIndex = 6` on `PalwIntervalOpeningRequestMessage`, the signed message tag 3 (`palw_fp_leaf_request_message_v1`); the resolver answers a leaf request before any interval arithmetic; the seat's `pursue_named_leaf_v1` | `the_panel_pursues_a_named_leaf_serves_its_evidence_and_answers_the_held_court`; the leaf-request signature round trip |
| **3** the demand, bounded | `PalwHeldMissingV1::StepLeaf` (appended); the binding half in `palw_held_da_check_accusation_v1` (in the step space, not fused); the chain half at acceptance (`palw_leaf_demand_is_the_seats_v1`: a seat of the bound panel, a leaf in its drawn interval — the seat's own geometry, `PalwSeatIntervalGeometryV1`, in both units) and in the fold (once per seat per claim: `held_leaf_demands`, delta tag 40, carriage tail `0xA4`, root block `held_leaf_demands`) | the two fold tests; `the_chains_interval_geometry_is_the_seats_on_both_units`; the delta tag pin |
| **4** the answer adjudicated | `MaterialDisclosedHeld` with `PalwHeldDisclosureV1::StepLeaf` — `ExecutorGuilty` voids `CourtFraud`, `FalseAccusation` closes refuted, anything else is `HeldDaRefused` | the fold tests |
| **5** the fence needs the court | `Params::validate_palw_v2`: `palw_held_context` refuses without `palw_da_court` and `palw_fp_da_pins` at or below it; `palw_held_context_mint_v1` arms both from genesis | the fence test's refusal cases |
| **6** both halves ship | the executor answers all four held units (`held_da_answer_v1` has no catch-all arm; the state chunk and the step range through `held_state_chunk_answer_v1` / `held_step_range_answer_v1`, every family); `PalwDaDutyV2.held_missing` tells it which | `the_executor_answers_every_held_unit_the_court_can_name` (held fold, every checkpoint, ranges across a block edge and at the widest, the court's own check); `the_floor_answers_a_held_chunk_and_range_from_its_dense_retention`; the no-catch-all pin |
| **7** the burden is the chain's | Decision 3's bound; the lane's rate and throttle for the fast path | as above |

### 8.2 What the live drill found (both fixed, both pinned)

`scripts/misaka-palw-shard-court-devnet-drill.sh` with `HELD=1 LEAF=fast` runs the executor with
`--palw-drill-answer-only` — it serves its claims' answer envelope and never the capture — so a
seat judges by intervals alone. Its first two runs found two defects older than this ADR, on the
path from a sampled fault to a named leaf:

1. **The first fault gate stopped a fault still addressed at a block.** The interval arm notes a
   `FaultInRange` as a block, asks for the block's leaves, and names the leaf from them on a later
   round (ADR-0086 Decision 6) — which the gate, stopping the verdict block the moment any fault was
   held, never let run. `de6fb0da`: a free-prompt claim whose fault is a block runs the interval arm
   again; the ledger never replaces a named leaf with a block, so the fall-through ends the round
   the leaf is named.
2. **The naming could never name a leaf in a lying interval.** It ran the close's replay, which on
   a fold puts the seat's own leaves under the served frontier and requires them to walk to the
   committed root — exactly what a lie inside the interval prevents ("this party's replay does not
   reproduce the step leg root under the served frontier", every round). `6dffa805`: the naming
   replays without the walk; the executor's leaves are bound to its commitment through the served
   block's digest, and what is named is adjudicated on the executor's own evidence, so a ghost can
   only clear. Pinned by `a_served_block_names_the_leaf_a_liar_committed`; restoring the walk
   reproduces the drill's error verbatim. **The same walk remains on ADR-0085 Decision 3's close
   from served intervals**, which therefore cannot close against a lying interval either; it is not
   on this ADR's path (the evidence is the executor's), and is recorded as its own task.

### 8.3 The drill runs

Three nodes on one host, the floor-only devnet minted with `--palw-held-context-devnet`, node-0
the liar (step leaf 0 of its canonical free-prompt claims tampered and re-committed) and running
`--palw-drill-answer-only`, so no seat holds a capture:

| run | binary | what happened |
|---|---|---|
| 1 | `be3c5101` | the seat verified interval 0 as a fault in block 0, asked for the block, held it — and nothing followed: 8.2 item 1 |
| 2 | `de6fb0da` | the block was read every round and named no leaf ("does not reproduce the step leg root under the served frontier"): 8.2 item 2 |
| 3, `LEAF=fast` | `6dffa805` | **PASS.** Claim `14b3ed4c…` committed 16:47:08; at 16:53:09 node-1 found the fault in block 0 of interval 0 and asked for the block (`0x80000000`); at 16:53:11 the block named leaf 0; at 16:53:13 it asked for the leaf's evidence (`0x20000000`) and node-0 served 25,029 bytes in 29 ms; at 16:53:15 it filed `ShardCourtAccused` on the executor's own evidence, and node-2 did the same ten seconds later. Block `a7591092…` carried it at 16:55:29; the claim read `voided court_fraud` on node-1 and node-2; bond 0's collateral fell 1,110,106,160 → 1,110,067,640 sompi |
| 4, `LEAF=slow` | `8856efc0` | **PASS.** node-0 also runs `--palw-drill-refuse-leaf-evidence`. Claim `c31aa3bd…` committed 17:00:55; at 17:07:02–04 both seats named leaf 0 and asked, and node-0 refused; at 17:12:24, `PALW_LEAF_EVIDENCE_FAST_PATH_DAA_V1` later, both demanded it on chain; block `0d6679a1…` carried one `DefaultAccusedHeld` at 17:13:17 (the other seat's demand was dropped with its block standing — one session a claim); at 17:13:18 node-0 answered "the held accusation of `StepLeaf { leaf: 0 }` from the retained capture — deadline DAA 124"; block `a9cfa992…` carried the `MaterialDisclosedHeld` at 17:15:34 and the fold's one-move verdict on it voided the claim `court_fraud`, on node-1 and node-2; bond 0's collateral fell by the same 38,520 sompi |

Invariant 5 holds on a live devnet: a seat that holds no capture convicts a tampered claim through
the fast path, and through the slow path when the executor refuses the fast one — and a refusal is
worth nothing to it, since the demand's answer is its own evidence and silence would have been
`ProducerWithholding`.

### 8.4 What remains before a network arms the held regime

* ADR-0085 Decision 3's close from served intervals against a liar (8.2 item 2).
* The verification-vector ADR (§6) — the evidence a context limit is armed on.
