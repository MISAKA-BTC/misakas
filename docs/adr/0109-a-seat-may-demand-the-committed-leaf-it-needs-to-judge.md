# ADR-0109 — A seat may demand the committed leaf it needs to judge

* Status: PROPOSED 2026-09-11 on `feat/adr-0103-held-context` (continuing ADR-0103 at `36f115c4`),
  written from the operator's decision on ADR-0103 §10.5. Nothing is armed: the new unit rides the
  held regime (`Params::palw_held_context`, `None` on every shipped preset) and ADR-0062's court, so
  no shipped fingerprint, identity, schedule or fork id moves. Testnet-11 does not move.
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
anchor, the annex for that one leaf, the refutation assembled from it — so an answer costs one
interval's replay, never the job's. It is served only to a key the lane already authorises for the
claim, once per `(bond, claim, leaf)`. The seat runs Decision 1's verdict at its own artifact root:
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

This ADR was first committed as 0108 (`e251c751`, 15:19:45 +0900) — 102 seconds after
`f608a066` (15:18:03 +0900) committed ADR-0108, "an extension is a manifest the verifier
recomputes", on `feat/adr-0108-extension-envelope`, a concurrent session's branch. A concurrent
claimant renumbers the later writer, and this is the later writer: it is 0109, and every reference
on this branch — code comments, log lines, the drill's patterns — was renumbered with it. The
verification-vector ADR takes its number when it is written, from what is resident on every branch
then, rather than reserving one here.
