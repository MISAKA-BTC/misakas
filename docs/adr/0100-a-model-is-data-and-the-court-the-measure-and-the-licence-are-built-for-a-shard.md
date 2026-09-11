# ADR-0100 — A model is data: the one-move court, the held measurement and the licence per shard, and the boundary "permissionless" means

* Status: PROPOSED 2026-09-10. **Decisions 1, 2, 3, 4 and 6 IMPLEMENTED the same day,
  consensus-inert on every shipped preset** (§9): the one-move court is a consensus object behind
  the fence ADR-0099 declared, `None` everywhere, and the fence arms only over a genesis that
  commits to the court's signing context; `palw-class measure` and `verify` exist and were run on
  the real A16 artifact; licensing per shard is a consensus object behind its own fence
  (`Params::palw_shard_licensing`), with no state-version move (Decision 4, amended the same day).
  **Decisions 5, 7 and 8 are stated, not built**, each with the reason. Every number below is the generator's or
  the tool's, recorded in `docs/palw-shard-plan-2026-09-10.md` and §1.3.
* Builds on: [0099](0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md)
  (Decision 5's fence and "the four"; §6 steps 2–4; Decision 4 named and not built),
  [0098](0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md)
  (a seat that found a lie files nothing else — now it files the one thing; licensing fits one
  transaction up to eight shards), [0067](0067-classes-are-chain-data-kernels-are-the-build.md)
  (classes are chain data, kernels are the build; Decision 5's fenced interpreter and SA-4's
  "an unpaid panel is the cheapest thing on the chain to buy"; Decision 6's tiers and
  "registration and possession are different acts"), [0062](0062-data-availability-court.md)
  SA-1 (who may accuse, and at what price), [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md)
  / [0095](0095-a-position-is-a-membership-not-an-income.md) (a position is bought from the curve
  and sold back to it, transfers nothing and pays nobody), [0056](0056-palw-permissionless-class-admission-and-share-economy.md)
  (no allowlist, no vote, no identity at admission), [0075](0075-certification-is-a-consensus-object.md)
  (anyone certifies; the court grades), [0049](0049-palw-adjudication-contract.md)
  Decision G (the inventory is the layout an `artifact_root` commits to).
* Amends: ADR-0099 Decision 5 (built: the refusal "not a consensus object" is replaced by the
  ruleset-move condition), ADR-0099 §6 step 3 (`palw-class measure`, built), ADR-0099 Decision 4
  (licensing per shard: designed here, pure half built). Supersedes nothing.

## 0. The sentence this ADR is

**A model is data.** The two reviews this ADR answers (2026-09-10) said the same thing from two
sides: the chain's registration, certification, admission, draw, court and market are already
permissionless — nobody's permission is involved anywhere a validator can recompute the answer —
and what is NOT permissionless is that a new model FAMILY is still Rust: a profile builder, a
converter, an engine, a lineage, and a merge. This ADR does not remove that boundary; it states
it precisely, keeps every mechanism the reviews found sound, and builds three things ADR-0099 left
as words: the court a shard seat can open (a consensus object now), the measurement a holder of
an artifact takes (a tool now, run on the real A16 artifact and reproducing its genesis class id
and registered root), and the licence a panel of shard seats produces (a per-shard object and
quorum now, a fold next).

The definition this repository commits to:

> **Any user may introduce any model, without a maintainer's approval or a node software
> upgrade, provided the model can be deterministically lowered into the PALW model IR and kernel
> semantics the build already carries, and its artifact, class profile, work and execution can be
> independently verified.** The MODEL is permissionless; the EXECUTION SEMANTICS are a
> deterministic protocol; a new primitive is a protocol upgrade, never a registration.

And the settled boundary the operator restated with it: **a Position is a membership and never
money.** It is bought from the curve and sold back to it, it is not transferable between holders,
and it is never a means of payment or settlement — ADR-0087's rule, kept as it is and named here
as a standing invariant of this boundary (Decision 8).

## 1. What was measured

### 1.1 The one-move court is a consensus object

The four things `Params::palw_shard_court`'s refusal named (ADR-0099 §3 Decision 5) exist:

| the four | where |
|---|---|
| the object | `PalwConsensusObjectV2::ShardCourtAccused { accusation }` — appended last (the discriminant is positional; a rebase onto `main`'s enum fix keeps it last) |
| the acceptance rule | the virtual processor's arm: the FENCE, the accuser's bond key over the session id, the shape, the ruleset's close ceiling, the claim's executor and roots, the class, and the verdict DERIVED at the court's ladder — a fused site refused, an unadjudicable refutation refused |
| the fold | `palw_state_v2`'s arm: the fence again (the ladder rides in `PalwTransitionExtrasV1::shard_court_ladder`), the claim live, the accuser Active and above the floor and not the producer, one court at a time on a claim, the verdict derived again and applied — `CourtFraud` voids the claim and slashes `claim.reserved`; a refutation that proves none charges the accuser `min(reserved, floor)` |
| the signing context | `misaka-palw/shard-court/accuse/mldsa87/v1`, in `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V3` — V2 in V2's order, then this one — derived from the family's own domain list by the registry's test |
| who files one | the seat, from its own capture check: `fp_capture_samples_clear` now returns the refutation and the openings it built, and a seat that found a leaf that does not recompute files exactly that, signed under its bond key, once per claim, when the fence is in force; and `misaka palw shard-accuse` signs an accusation built elsewhere |

What the fence still refuses is ARMING over a bundle whose committed context set is not V3: a
network that wants the court states V3 at genesis through `palw_signature_contexts_v2`, and no
shipped preset does (§5 Invariant 8). No state record changed, so `PALW_STATE_V2_VERSION` did
not move.

### 1.2 The plan reads the inventory, and a shard's rows are the inventory's

ADR-0099's plan weighed layers by a formula. The inventory (ADR-0049 Decision G) is the layout
the class's root commits to, one row per tensor slice with its layer, so a HELD artifact measures
itself: every row lands by its layer, a graph-level row by the node tables that name it (the
embedding's on the first shard, the logits' on the last, a tied head's on both ends, a tensor a
layer node names without a placeholder on every shard, a `<tensor>.<derivation>` row — the A16
quantisation parameters — with its tensor), and a row nothing names is refused by name. The
plan's per-seat bytes and the rows' own bytes close a round trip (§5 Invariant 3), and a shard
is named by the class's `artifact_root` and a set of leaf indices under it — no new identity.

### 1.3 The held measurement, on the real artifact

`palw-class measure --network testnet-11 qwen25-1.5b-a16.palwart` (this machine, 2026-09-10; the
shipped A16 artifact, 1,795,427,276 bytes — the size `docs/model-requests.md` records):

| what | value |
|---|---|
| the geometry read off the file | 28 layers, the dense A16 lineage, the artifact's own `eps_q` carried into the manifest |
| the class at 512 | `4277d84f7d91…` — **the shipped `Qwen/Qwen2.5-1.5B/graph-v5@512` class, genesis-registered on testnet-11** |
| the inventory root | `1a7457f100d9…` — **the root the graph-v2, -v3 and -v5 rows register** |
| bytes measured from the inventory | 1,826,522,928 (the rows the commitment covers, a tied head's both views among them) |
| the file on disk | 1,795,427,276 |
| the family formula (ADR-0099) | 1,776,943,104 — a floor, as it said |
| fit | admitted at 512; refused at 32,768 and 131,072 by the ladder, the court window, the state chunks and the PublicDa payload; no profile at 1M |
| a self-reported 34 ms a position on this Mac | 2,117,400 positions inside the RC's receipt window, listed as self-reported |
| signed under a throwaway ML-DSA-87 seed | `verify --artifact` recomputes every field and the signature verifies; one byte added to the artifact's bytes → `artifact_bytes` MISMATCH and the signature does not verify, exit 1 |
| `verify` WITHOUT the artifact | the eight artifact-derived fields (bytes, basis, root, file size, four contexts' plans) are named as needing the artifact; everything else recomputed; exit 0 |
| the hybrid-container `qwen35-2b.palwq36` on this Mac (the Qwen3.5-2B rehearsal artifact) | **refused by name**: the inventory is built under the graph-v5 profile its manifest projects, and this artifact's embedding-lift table (`embed_lift.a16: 248320 triples serve neither one lane nor 2048`) is not a layout that inventory accepts — a measurement that cannot place a row refuses rather than estimates. **Answered by [ADR-0102](0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md)**: graph-v6 reads the lift per token, and the artifact measures (§1.4 there) |

### 1.4 Licensing per shard, priced

One part per shard at the shipped quorum is **14,393 bytes** (three receipts of 4,772, the claim,
two shard fields and the tags); it fits one standard transaction at every shard count, where the
whole-object form stops at eight (114,597 bytes; 128,913 at nine, ADR-0098 §1.2). A K3-class
claim at 23 shards is 23 carriers of 14,393 bytes — 331 KB of receipts across as many blocks as
it takes, against a block that cannot carry them together — which is why the progress a claim
keeps is a bitmap and the fold licenses on the last part (Decision 4).

## 2. The requirement

> **R-data — a model is data and its execution is protocol.** Anyone adds a model the build can
> lower; nobody adds a kernel by registering one. A seat holding a shard can convict; a holder of
> an artifact can measure it in a form any other holder recomputes and any node can check the
> recomputable half of; a panel of shard seats can license a claim in parts that each fit a
> carrier; and a Position stays a membership — never transferred, never a payment.

## 3. Decisions

**Decision 1 — the one-move court is built, and arming it is the ruleset move (ADR-0099 Decision
5, finished).** `ShardCourtAccused` rides signed and bounded by the close ceiling; the acceptance
arm and the fold both derive the verdict at the court's ladder (the fold's ladder rides in the
block's extras, so a node cannot admit what its fold refuses); the accuser is any Active bond at
or above the floor that is not the claim's own — priced, not privileged (ADR-0062 SA-1); a false
accusation costs `min(claim.reserved, floor)`; a fused site is refused, not tried (ADR-0093 is
still what a fused terminal needs). The shard fields left the object: the chain holds no plan,
and a leaf either recomputes or it does not, whoever names it; the seat's own rule — name only a
leaf your shard replayed — is a function beside the object. The signing context is in the V3
set, and `validate_palw_v2` refuses `palw_shard_court` over any bundle that does not commit to V3.

**Decision 2 — the inventory measures the artifact, and a shard's rows are the inventory's.**
`palw_artifact_bytes_from_inventory_v1` replaces the family formula for a held artifact (basis
"the artifact inventory's rows, byte for byte"); `palw_shard_inventory_rows_v1` is the shard
manifest — per shard, the inventory indices a seat holds and their bytes — derived, and equal to
the plan's own figure. `PalwArtifactBytesV1` gains `shared` (every shard's) and `ends` (both
ends') for the two graph-level classes an inventory can carry and a formula never does.

**Decision 3 — `palw-class measure` and `verify`.** The SDK's binary reads the geometry off the
artifact (the `eps_q` it carries now travels in the manifest, so the class named is the class the
artifact is), builds the inventory under the manifest's own profile, measures it, evaluates the
walls and the plans on the named network, writes the Measured Model Artifact, and signs its id
under the bond key when given one (`misaka-palw/measured-model/mldsa87/v1` — the convention's
name; NOT in any committed set, because no consensus rule verifies it: a registration is verified
by recomputation, ADR-0099 Decision 6). `verify` recomputes a document field by field; without
the artifact it names the artifact's fields as needing one rather than comparing them with a
formula (`PalwMeasuredCheckV1::NeedsTheArtifact`); it checks the signature and exits 1 on a
mismatch or a signature that does not verify.

**Decision 4 — licensing per shard, built, and it needs no flag day** (amended the same day; the
first text said the fold was a state-version flag day, and the precedent below says otherwise).
The pure half: `PalwShardReceiptPartV1 { claim, shard_count, shard_index, receipts }` is one
shard's licensing; `palw_shard_quorum_v1` is that shard's quorum over verified verdicts (its own
seats, once each, `Incapable` for neither side, a majority quorum so the two outcomes are
disjoint); `PalwShardLicensingProgressV1` is the bitmap a claim keeps. The consensus half:

* **Who declares the count: the class's registrant, once** (`ClassShardPlanDeclared`, signed
  under `misaka-palw/shard-plan/mldsa87/v1`). Not the line's owner, as this Decision first said:
  the panel is the class's, and a class carries many lines. A genesis class has no registrant and
  is never sharded; a plan is immutable, like the class's graph.
* **Who holds a shard: a bond, as a refinement of a class it declared** (`BondShardsDeclared`,
  signed under `misaka-palw/bond-shards/mldsa87/v1`; an empty list withdraws). **This corrects
  ADR-0099 Decision 3**, which said a shard capability id rides `capable_classes`: the fold
  refuses every entry of that set that is not a registered class id (`MissingClass`), so a derived
  id could never have been declared — and keeping the class entry is what keeps SA-1's bound,
  SA-2's exposure and SA-3's production proof on a shard seat.
* **The draw.** For a claim whose class has a plan — the fence read at the claim's ANCHOR, as
  ADR-0065 D1 and ADR-0071 SA-3 are, through one processor helper the binding and its validation
  share — the chain draws `derive_stratified_panel_v2`: the flat draw's eligibility
  (`palw_panel_eligible_bonds_v2`, now one spelling for both draws), each bond carrying its shards
  of the class, `derive_shard_panel_v1` per shard, stored flat and shard-major. A shard nobody
  holds refuses the draw by name and the claim voids at `BindTimeout`; a sharded class never falls
  back to a flat panel, which would ask shard seats to judge a whole model.
* **The licence.** A claim licenses by parts exactly when its bound panel has the stratified shape
  — `shard_count × seat_count` seats, told from a flat panel by its length alone, so a claim bound
  before the declaration keeps the whole licence it was drawn for (`palw_claim_licenses_by_parts_v1`).
  `ShardReceiptLicensed` carries one shard's receipts. At acceptance they meet the whole licence's
  own checks over that shard's seats (`receipt_quorum_over_seats_v2`, one spelling for both
  doors). In the fold: that shard's dissenting seats are charged as the whole licence charges
  them; the claim licenses in the block that lands its last shard (`license_claim`, one spelling
  for both licences); a shard whose quorum says Unavailable voids the claim where ADR-0065 D4 has
  not retired that door. `ReceiptLicensed` and `ProducerDefaulted` are refused for a claim that
  licenses by parts, and the collector assembles the next ready part.
* **No flag day.** The three new collections — plans, bonds' shards, per-claim progress — enter
  the state root in their own guarded block and the state carriage behind tail `0x99` only once
  written, which is the ADR-0087 / ADR-0088 / ADR-0095 precedent, and nothing can be written below
  `Params::palw_shard_licensing`. `PALW_STATE_V2_VERSION` stays 20, the golden vectors stay, and a
  dormant network roots byte-identically; the claim record gains no field (the progress lives in
  its own collection, dropped by `write_claim` on every way out of `PanelBound`). Arming is an
  ordinary scheduled activation, which `validate_palw_v2` refuses over a bundle that does not
  commit to the V3 contexts (the two new ones are in it) and without the one-move court armed at
  or below the same height — a panel of shard seats must be able to convict with what a shard
  seat holds (ADR-0098 Decision 5).

**Decision 5 — the economics come before the arming.** The stratified panel multiplies ADR-0067
SA-4's problem — a registrant funding a quorum of judges for the price of a few bonds — by the
shard count, and the one-move court is the remedy, not the cure: any bonded party can convict a
lie a bought panel licensed, but a bought panel still licences it first. `chain_classes` stays
sealed until seats are paid (SA-4) and ADR-0067 Decision 5's fuzz gate has run; a sharded class
becomes weight-bearing under the same two conditions and no earlier. Stated, so the order is not
reversed.

**Decision 6 — the boundary "permissionless" means, fixed.** Kept as the reviews found them:
admission without allowlist, vote or identity (ADR-0056); certification by anyone (ADR-0075);
the attempt drawn by the chain (ADR-0074); the court and the DA court open to any bonded party at
a price (ADR-0027, ADR-0062); a line founded by anyone, its versions its developer's, its roles
its owner's — ownership of a line is not permission over the chain (ADR-0088); a Position seeded,
bought and sold by anyone (ADR-0087, ADR-0094); the reward buying the pair with nobody's hand on
it (ADR-0091); a node syncing and verifying without a bond, production needing one (ADR-0061);
private prompts served to the claim's readers and to nobody else (the private-prompts design).
What is added: the definition in §0, and — for the membership half of the boundary — ADR-0101:
**the line controls the PRODUCT, providers control the SERVING, the chain proves the MEMBERSHIP**,
so a benefit a line declares (ADR-0095) is served by any provider a client can check, not only
by the line's origin.

**Decision 7 — what "a model is data" still needs, in order (the first review's list, adopted).**
1. A model descriptor and IR: the manifest of ADR-0099 Decision 1 is the descriptor's first form;
   the IR is the graph the profile already is, with a tensor-mapping spec beside it so a converter
   is data too. 2. A source adapter that fetches an immutable revision and parses DATA ONLY —
   never runs a repository's code; the worker never touches the repository. 3. A generic tensor
   mapping and artifact builder. 4. **The positive control: the two shipped families through the
   generic pipeline, reproducing their class ids, artifact roots and execution facts** — the day
   that passes, "new family = new Rust" ends for every architecture the kernel vocabulary covers.
5. `resolve_chain_registered` promoted to the ordinary path (after Decision 5). 6. An out-of-band
   content-addressed artifact store and resolver (local, repository, mirror, peer), fetching only
   what an executor chose — registration still carries kilobytes and no URL (ADR-0067 Decision
   6). 7. The shard manifest over the inventory (Decision 2) feeding a sharded execution backend
   behind `PalwExecutionBackendV1`, with placement an executor-side act and no scheduler on the
   chain (ADR-0074). 8. Seat advertisements as host facts, never chain truth (work stays in leaves
   and pwu). 9. A provider-side service descriptor — line, benefits, endpoint, model roots,
   expiry, provider key, signed — carried by a directory the chain does not hold. 10. `misaka
   model add` from a repository revision to a registered, certified, seeded line, over the
   primitives that exist.

**Decision 8 — a Position is a membership and never money.** Settled by the operator on
2026-09-10 and decided, with its three pins, as ADR-0101 Decision 5: bought from the curve, sold
back to it, transferable to nobody, never a means of payment or settlement — and nothing "a model
is data" adds (a provider, a directory, a sharded seat) receives, escrows or is paid in one.

## 4. What this costs

* **Chain:** nothing on any shipped preset — the object is refused wholesale while the fence is
  `None`, the fence is `None` everywhere, and the fingerprints are byte-identical. A network that
  arms it states V3 at genesis (a different `signature_contexts_root`, therefore a different
  ruleset id — the move ADR-0099 named).
* **Node:** one acceptance arm, one fold arm, one field on the extras, one module of pure
  functions for the licence; the seat keeps what it already computed.
* **An adder:** `palw-class measure` over the converted artifact, on the network's name, with the
  bond key — one command, one document.
* **A seat holding a shard:** its shard's inventory rows (Decision 2), and one carrier when it
  finds a lie.

## 5. Invariants the tests hold

```
1  The registry's committed set is derived from the families, V2 is the frozen nine's prefix,
   V3 is V2's prefix plus the one-move court's context, three sets have three roots, and the
   frozen root has not moved.
2  An inventory measures its artifact: every row lands by its layer or by the node that names
   it (a derived row with its tensor), a row nothing names is refused by name, and the total is
   the rows' own.
3  A shard's rows are the inventory's, every row on exactly the shards that hold it, and a
   shard's measured bytes are the plan's figure for it; a tied head lands on both ends and a
   shared tensor on every shard, counted once by a seat holding the whole.
4  An accusation's session id binds the network domain and every field but the signature; the
   shape rules refuse by name; the filer's rule names another shard's leaf.
5  A part fits one standard transaction at every shard count; the whole form does not at nine.
6  A shard's quorum counts only its own seats, once each, Incapable for neither side, and
   refuses a non-majority quorum.
7  The progress completes on the last shard, ignores a repeat, and names a foreign part.
8  The shard court fence is dormant on every preset, refused over the frozen set and over V2 by
   name, and assembles over a genesis that states V3.
9  A manifest of a shipped class names the shipped class (ADR-0099), and the held measurement
   of the real A16 artifact names the genesis class and its registered root (§1.3, run by hand
   — the artifact is not in the tree).
10 A document measured from an inventory verifies with the artifact, is refused by the tampered
   field with it, and without it names the artifact's fields as needing one.
11 A sharded claim licenses on its last part, holds a progress row only in between, and each
   part reverts and re-applies exactly.
12 The whole-object licence is refused for a claim drawn per shard and kept for one drawn flat;
   a part is refused for a flat claim.
13 A part is refused by name: dormant, another plan, a shard out of range, a seat of another
   shard, short of its quorum, already licensed; a shard whose quorum says Unavailable voids the
   claim, and past ADR-0065 D4 cannot.
14 A plan is declared once, by a class with a registrant, with a count in range; a bond's shards
   refine a class it declared, against the plan's count, and an empty list withdraws; a redraw
   drops the progress.
15 The stratified draw seats each shard from the bonds that hold it, one operator a shard; the
   binding validator recomputes it only when told the claim is sharded; a short shard refuses the
   draw by name; a part is validated over its own shard's seats and the whole door is shut.
16 The sharding collections enter the root and the carriage only once written; the licensing
   fence is dormant everywhere and arms only over V3 with the court no later; the new delta
   entries (36–38) and objects (39–41) keep their Borsh tags.
```

1 is `palw_mode_v2`'s tests; 2–3 `palw_shard_plan_v1`'s; 4 and 10
`consensus/core/tests/palw_adr0099_shard_plan.rs`; 5–7 `palw_shard_licensing_v1`'s; 8
`config::params`'s `the_shard_court_arms_only_over_a_bundle_that_commits_to_its_signing_context`;
11–14 and the root half of 16 `palw_state_v2`'s; 15 `palw_panel_v2`'s; the fence half of 16
`config::params`'s `the_shard_licensing_fence_is_dormant_and_arms_only_over_v3_with_the_court`, the
tags the two discriminant pins.

## 6. Order of work

1. Decisions 1, 2, 3 and 4 — **done** (§9); Decision 4's consensus half without a flag day.
2. A devnet drill of the one-move court: a genesis stating V3, a seat that files, a fold that
   convicts — the U-04 of ADR-0099, now with an object to file. **Done 2026-09-10** (§9, "the
   drill"): `scripts/misaka-palw-shard-court-devnet-drill.sh`, PASS on the second run.
3. A devnet drill of per-shard licensing: a class registered by a bond, a plan, bonds declaring
   shards, a stratified panel, parts landing — the seats holding the whole model until a sharded
   backend exists (Decision 7 step 7).
4. Decision 5's conditions: paid seats, the fuzz gate.
5. Decision 7, in its order; step 4 (the positive control) before any of 5–10.
6. The successor mint for a K3-class network (ADR-0097 Decision 5), priced by the tools.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0099 Decision 5 (the court declared, refused at assembly) | built; the refusal is now the ruleset-move condition (V3) |
| ADR-0099 Decision 4 (licensing past eight shards, named) | the part, the quorum and the progress built; the fold and the state version move stated as the next step |
| ADR-0099 §6 step 3 (`palw-class measure`) | built, with `verify`, run on the real artifact |
| ADR-0099 Decision 2 (the plan, derived from the formula) | the formula stays the estimate for a manifest; a held artifact measures itself from the inventory |
| ADR-0098 Decision 2 (a seat that found a lie files nothing else) | it files the one thing — the accusation — and nothing else |
| ADR-0087 / ADR-0095 (a Position is bought from the curve; a membership, not an income) | kept as they are; named as the standing invariant of Decision 8 |
| ADR-0067 SA-4 / Decision 5 (arming stays off; the fuzz gate) | kept; Decision 5 here says a sharded class inherits both conditions |

## 8. What is deliberately not decided

* **The provider directory's transport** (Decision 7 step 9): a signed descriptor, carried by
  something the chain does not hold.
* **A third family.** Still a converter and kernels first; Decision 7 step 4 is what changes that.
* **Any number of the successor mint.**

## 9. Number hygiene and implementation record

0100 is the next free number after ADR-0099 (whose README row says so). Claimed on
`feat/adr-0099-sharded-seat`, the same branch, on 2026-09-10; ADR-0101 follows it there. **The
next free number is 0102.** The branch's merge base with `main` (`7f4dded4`) predates `main`'s
Borsh enum fix (`891a1a14`, which moved `ModelLineBenefitsDeclared` to the end): the merge must
keep `ShardCourtAccused` LAST, after it — tag 38 on both trees, pinned by
`the_one_move_courts_borsh_tag_is_the_last_and_pinned` — and the branch's own copy of the pre-fix
order is `main`'s to overwrite.

* **2026-09-10** — written and implemented the same day:
  * `consensus/core/src/palw_shard_court_v1.rs` — Decision 1: the object as a consensus object's
    payload, the session id with the network domain, the ceiling, the charge, the verdict, the
    filer's rule.
  * `consensus/core/src/palw_state_v2.rs` — `ShardCourtAccused`, `shard_court_ladder` on the
    extras, seven refusals, the fold arm; `palw_lifecycle_objects_v2.rs` — the ride rule;
    `consensus/src/pipeline/virtual_processor/processor.rs` — the cached fence, the extras' ladder,
    the acceptance arm, the name table; `kaspad/src/palw_panel.rs` — the seat keeps and files
    its refutation; `kaspad/src/palw_producer.rs` — the fence reader; `misaka-cli/src/palw_shard_court.rs`
    — `palw shard-accuse`.
  * `consensus/core/src/palw_mode_v2.rs` — `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V3`, its root, the
    bundle check, the derivation test; `palw_derived_v1.rs` — the sweep; `config/params.rs` —
    the fence's new condition and its test.
  * `consensus/core/src/palw_shard_plan_v1.rs` — Decision 2: `shared` and `ends`, the inventory
    measurement, the shard rows, the derived-row rule.
  * `consensus/core/src/palw_measured_model_v1.rs` — Decision 3: the held artifact in the
    inputs and the document, `rms_eps_q` in the manifest, `NeedsTheArtifact`; the context renamed
    to the convention; `misaka-palw-sdk/src/bin/palw-class.rs` — `measure` and `verify`.
  * `consensus/core/src/palw_shard_licensing_v1.rs` — Decision 4's pure half.
  * **Decision 4's consensus half (same day):** `palw_shard_licensing_v1.rs` — the plan record,
    the two message functions and contexts, the extras parameters, the shard slice, the by-parts
    predicate; `palw_state_v2.rs` — three collections (rooted and carried behind `0x99` only once
    written), three delta entries, `ClassShardPlanDeclared` / `BondShardsDeclared` /
    `ShardReceiptLicensed` (tags 39–41), `PalwTransitionExtrasV1::shard_licensing`, fourteen
    refusals, the three fold arms, `license_claim` and the `write_claim` hook; `palw_panel_v2.rs`
    — `palw_panel_eligible_bonds_v2`, `derive_stratified_panel_v2`,
    `validate_panel_bound_v2_with_shards`, `receipt_quorum_over_seats_v2`,
    `validate_shard_receipt_part_v1`; `palw_lifecycle_objects_v2.rs` — three ride rules;
    `palw_mode_v2.rs` — V3 gains the two contexts; `config/params.rs`, `fork_id_v1.rs` —
    `Params::palw_shard_licensing` at every site, its two assembly rules and its test;
    `processor.rs` — the cached fence, the one-place stratified decision, the extras, the three
    acceptance arms, the stratified binding and its validation, the part assembler;
    `misaka-cli/src/palw_shard_licensing.rs` — `palw shard-plan` and `palw bond-shards`;
    `kaspad/src/palw_panel.rs` — the seat files only where the chain's refutation ladder can try
    the claim, and its source pin reads the new arm.
  * `docs/palw-shard-plan-2026-09-10.md` regenerated; this ADR; the README index.
* **2026-09-10, the drill (§6 step 2).** Three devnet nodes on this Mac, the floor class only,
  `--palw-shard-court-devnet=0` (the genesis states the COMPLETE_V3 set; the court armed from
  block one); node-0 produces canonical free-prompt claims whose capture is corrupted at step leaf
  0 (`--palw-drill-tamper-fp-leaf=0`, devnet/simnet only), the other two are its panel.
  * **Run 1 found a defect in the drill's own lie, not in the court.** The panel patched the
    attempt-lane liar's outcome into the honest free-prompt run, so the run's FACTS (the four leg
    roots the free-prompt execution root is recomputed from) still described the honest capture:
    `palw_fp_commitment_from_context_v3` refused the pair (`ContextDoesNotReproduceTheRoot`) and
    node-0 logged "the run does not assemble into a commitment" every interval — no claim, so no
    court. Fixed by a free-prompt verb of the drill fault
    (`PalwExecutionBackendV1::execute_free_prompt_with_injected_fault`; BASE-0 corrupts the
    capture BEFORE any fact is measured, `corrupt_capture_v1` shared with the attempt lane), the
    live failure pinned by `a_free_prompt_drill_fault_assembles_into_its_own_commitment_and_convicts`.
  * **Run 2, PASS.** 22:25:36 node-0 committed claim `d3766988f70d7a37…` (tx `8bd939f1…`), its
    material self-consistent; 22:29:19 both seats found leaf 0 of the served capture does not
    recompute and filed `ShardCourtAccused` (sessions `69e87283…`, `c797e2ef…`) — the claim was
    `panel_bound`; 22:30:21 block `16729a4e…` carried one accusation, every node folded it, and the
    three duplicates that followed were dropped with their blocks standing ("wrong phase"); the
    claim reads `voided court_fraud` on node-1 and node-2 over RPC; bond 0's collateral fell from
    1,110,106,160 to 1,110,067,640 sompi, −38,520 (`void_and_slash` debits the claim's `reserved`).
    No human step between the lie and the slash.
