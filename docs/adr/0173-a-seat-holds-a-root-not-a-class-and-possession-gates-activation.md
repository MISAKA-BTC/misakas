# ADR-0173 — A seat holds a root, not a class: possession is keyed `(bond, class, root)`, a new root is NotReady until its floor, the draw reads the claim's own root, and a model's activation waits for possession (validation and canary are the dormant skeleton)

**Status:** PROPOSED 2026-10-04 on `rcore/post5300-audit` (lane MU), the user's request after the DAA-5,300 release. Consensus change behind the
**same** dormant fence as the audit fixes, `Params::palw_audit_1004_v1` ("今回の監査修正で"): below it every path folds byte for byte as at
`0b1c11b87`, and on every shipped preset the fence is `None`. Amends **ADR-0088** (a version is in force from its publication — now: claimable from
its possession), **ADR-0135** Decision 4 (a readiness row is per class — now per root) and **RFC-0002 Part II Proposal A** (the seating floors are asked
of the claim's root). Touches **RFC-0004 §17.4.1** (a head moves only to an Active, seated class). Takes **no object tag**, **delta number 150**
(declared explicitly: `SeatRootReadiness`), **carriage tail `0xEB`**, no wRPC op and no DB prefix.

**Builds on:** ADR-0088 (model lines, versions, `PALW_VERSION_GRACE_DAA_V1` = 4,000), ADR-0133 §11.2 (the V2 possession multiproof), ADR-0135 (the
registry's readiness predicate), RFC-0002 Part II (`palw_class_seating`), RFC-0004 (candidates are new classes).

## 0. The sentence this ADR is

**A claim that names a root can be accepted only while `seat_count` operators besides its executor have shown they hold THAT root, and its panel is
drawn only from seats that have.**

## 1. The finding

ADR-0088 Decision 3 puts a published root in force from the accepting block: the founding root, or every Active line's current root, previews, and
superseded roots inside the grace. But a seat's readiness is keyed `(bond, class)` and the V2 possession proof is verified against the class's
**registered** root only. So when a line publishes R2 for a class whose seats proved R1:

* an R2 claim is accepted at once (the root is in force) and its panel is drawn from seats whose only proof is R1;
* those seats cannot replay R2, the claim cannot reach quorum, and it voids (`PanelUnavailable` / `NoCapablePanel`) — a claim the producer paid for, a seat
  blamed for a model it never held, and a liveness lever for anyone who may publish a version.

The invariant the chain must keep: **if an R2 claim can become valid, a required number of independent seats that actually hold R2 exist.**

## 2. The three conditions

| | meaning | enforced here |
|---|---|---|
| **Possession** | seats hold the root | **yes** — this ADR's rules (R1–R5) |
| **Validation** | the root is an allowed, safe artifact: data-only formats (TIR inventory, safetensors/GGUF lowered), a canonicalised root, and an attestation signed by independent operators over the format, the manifest and the behaviour-check results; the publisher alone cannot activate | **no** — `palw_root_activation_v1.rs` (types, signed message, counting rule; unwired) |
| **Activation** | `Candidate → Canary (claim-limited) → Active`; per-root immediate revoke; rollback to the previous root | **partly** — R5 gates activation on possession now; the canary / revoke stage machine is the unwired skeleton |

Possession is checkable from chain state alone (it is a Merkle opening); Validation is a judgement about content and needs independent signers;
Activation is a policy over both. Keeping them separate is what lets Possession ship now without deciding the other two.

## 3. The rules (all behind `palw_audit_1004_v1`)

**R1 — readiness is keyed `(bond, class, root)`.** A new table `seat_root_readiness: (bond, class, root) → PalwSeatReadinessRowV1` (delta 150, tail
`0xEB`, rooted only when non-empty). **The existing `(bond, class)` table IS the founding root's**, so the fence needs no migration: a legacy row reads
as the registered root (`PalwChainStateV2::seat_readiness_for_root`). The V2 proof object is unchanged — **no new tag**: the multiproof already
commits to one root, and the signature already covers the proof. The fold reconstructs the root the proof opens (`reconstruct_artifact_multiproof_root_v1`,
the verifier's walk without its final comparison) and writes: the registered root → the legacy row, as before; a root the class has in force
(`class_roots_in_force`, which includes previews and superseded-in-grace) → its own row; anything else → `ReadinessProofRefused`. A composite class's
possession root stays its adapter section; a composite takes no other root. A root row's replay rule (`ReadinessProofNotNewer`) and landing window are
the legacy row's. Writing a seat's row for a class also drops that seat's rows for roots that left force, so the table is bounded by roots in force.

**R2 — a new root is NotReady until its possession floor.** A claim naming a root other than the registered one is refused with `RootNotSeated
{ class, root, have, need }` until `|Ready(root) \ {operator(executor)}| ≥ seat_count` — the possession rule of `palw_class_seating_v1`, over the root's
rows (`check_root_possession_v1`). The same door runs for attempts (the attempt names its root) and free-prompt commitments (attributed to the
founding line's current root, as `note_claim_usage` does). Where `palw_class_seating` is armed the class-seating floors (possession and independence)
are asked over the root's seats as well (`check_class_seated_root_v1`), so a class whose founding seats have stopped re-proving R1 is not unseated for an R2
claim. Claims naming the founding root are asked what they were asked.

**R3 — the draw reads the claim's root.** `palw_bond_may_judge_class_v5(…, root, …)`: a seat is eligible for a claim only with a fresh row for the root in
`claim_roots[claim]` (the founding root when absent). The readiness policy carries `root_keyed`, set from the fence at the claim's anchor; the stake
census keys its verdicts and judge vectors by `(class, root)`. A seat holding R1 only is never drawn onto an R2 claim; an R1 claim keeps its R1 seats
through the grace.

**R4 — class-level readers count a seat that holds any root in force.** The lifecycle's ready count, the admission jury, the room and the registry read
judge a seat by `seat_readiness_class_v1`: the freshest of the legacy row and the rows of the roots in force. Without it a class whose founding root left
force after its grace would read as unseated while its current root is fully held. Below the fence it is the legacy row.

**R5 — possession gates activation.** `ModelVersionPublished` with `preview = false` and `ModelVersionPromoted` are refused with `RootNotSeated` until the
root's floor is met (no executor excluded). A new root therefore enters as a preview, seats prove it, and only then does it supersede the previous root;
the superseded root stays in force for the grace, so a claim is never without a root (**no gap**). A line whose new root never gathers its floor keeps its
current root: the failure is a stalled upgrade, not a void claim.

**R6 — RFC-0004: a head moves only to a class that is Active and seated.** A candidate is a new class and already goes `Candidate → Probation → Active`
with seating; the head switch (the epoch's winner) did not ask it. Past the fence `decide_epoch_v1` decides a winner that is not `Active` in the
registry's lifecycle, or not seated (possession floor with no executor excluded, plus the independence floor where armed), as
`NoChange { HeadNotReady }` (reason 6): the old head stays live. A rollback restores a previous head and is not gated.

## 4. Node side (producer / seat policy; no new trust)

* The registry read carries each class's roots in force and every seat's root rows, so a seat can see what it has not yet proved.
* **The readiness duty proves `(class, root)` for every root in force the seat holds the bundle of**: a held artifact resolves by its digest, the proof
  is the V2 multiproof over that root, and the superseded root keeps being proved during its grace (so R1 claims stay live). The duty's memo is keyed by
  slot `(class, root)`. Conformance, memory capacity and the proof lane are per `(class, root)` as they were per class.
* **Prefetch** of a new version's or a new candidate class's bundle goes through the node's existing artifact path (the holdings it loads and
  `--palw-chain-classes`, plus the improvement watcher's drop directory for candidate classes); the model transport (`bundle_commitment` /
  `ModelDistributionDeclared`, lane MN, ADR-0171) is **not in this branch**, so the fetch it would make is a named note —
  `ROOT_BUNDLE_MISSING` — until it merges, and a seat proves nothing for a root it does not hold. A seat never proves a root it cannot replay.
* Panel binary updates stay on fences; weights follow through the transport once it is in.

## 5. Tests

`palw_state_v2/tests/adr0135/root_possession_v1.rs`: a proof of a published root writes its own row, reverts by delta and round-trips the carriage
(migration: an R1 proof lands in the legacy table, which reads as the founding root); below the fence R2 and an unknown root are refused; R2 claims
are refused until `seat_count` operators besides the executor prove it and the executor's operator never counts; R1 claims stay admissible through the
grace and R1 leaves force when it ends; an R1-only seat is never drawn onto an R2 claim (and the unkeyed draw is what it was); a version is
activated only once possessed; a head moves only to an Active seated class. `palw_root_activation_v1`: the attestation's counting rule and the stage
machine. Below the fence every existing test is unchanged; `scripts/t12-repin.sh --drift-only` shows no drift (the fence stays `None`).

## 6. What this does not do

* It does not judge a root's **content** (§2 Validation). A possessed root can still be a bad model; possession proves the bytes are held, nothing more.
* It does not make the founding root's rows per-root *before* the fence: nothing is migrated, nothing is read differently below it.
* The class-level readiness reads (RPC `readySeatsNow`, the panel view) count by R4; a per-root count is a read-only follow-up.

## 7. Open questions for the user

1. **Canary behaviour checks** — what runs in `Canary`? Candidates: (a) the class's reference evaluator on a fixed probe set must match the seats' replay
   bit for bit (already what the court does, so free); (b) a quality gate against the previous root on the line's own suite (RFC-0004's evaluation jobs
   on a pinned subset); (c) a safety suite. (a) is the only one that needs no new machinery.
2. **Canary limit and duration** — the skeleton uses 10 % of the class's claim capacity for ≥ 2,000 DAA. Different numbers, or per-class?
3. **Who validates** — which operators may sign a validation attestation (any Active bond outside the publisher's operator? a registered validator set?),
   how many are required (the skeleton: 3 distinct operators, one independent failure blocks), and whether attestations are paid.
4. **Revoke authority** — per-root immediate revoke by whom: the line's owner only (rollback), the validators (a failed check), or the court (a
   conviction naming the root)? Claims already accepted keep the root they named in every design.
5. **A new root that never gathers its floor** — R5 leaves the line on its current root forever. Should a preview expire (withdraw automatically after N DAA)?
6. **R6 and a winner that is not yet ready** — the epoch is decided as no change (its fees and rewards follow the no-change path). Alternative: defer the
   decision until the winner is ready, which needs an epoch state the fold does not have; preferable only if candidates routinely win before they are seated.
