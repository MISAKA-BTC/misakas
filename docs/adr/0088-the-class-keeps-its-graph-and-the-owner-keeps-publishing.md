# ADR-0088 — the class keeps its graph; a line keeps its owner, and the owner keeps publishing

**Status:** PROPOSED 2026-09-05, **revised the same day** — design only (no implementation yet).
The first draft of this number (commit `4f198f6e`, "the class keeps its graph, and the exam
names its weights") made the successor of a model's weights the verdict of an on-chain exam:
candidates proposed by anyone, questions drawn by the beacon from a permissionless pool of pure
programs, answers tried by the court, a chained index of accepted improvements. The operator
withdrew that direction on 2026-09-05: *strengthening a model is one developer's work, not a
protocol's* — "分散モデル学習ではなく分散モデル市場" (not distributed training, a distributed
market). A Qwen-class 27B model is trained by one team; what the chain should do is record every
version that team publishes, keep the old ones, count how each is used, and let the market price
the line. This text replaces the first draft under the same number, renamed to say what it now decides
(README §"Number hygiene"). The draft's reasoning is summarised in §9 so it can be found again.
**Requested by the operator:** one registered address is one model-developing entity (1 アドレス =
1 モデル開発主体); that address alone updates the model, V1 → V2 → V3, continuously; every version
is recorded in a Model Registry that is "a Git for models" (weights hash, runtime hash, dataset
commitment, training config, evaluations, inference statistics, position, price); users use the
model and PALW processes the use; the position is priced on the MISAKA EVM's Model AMM; a line
that the market rates highly has the strongest reason to keep improving; and the design should
be the one that makes strengthening *easiest* (より強化しやすい設計).
**Builds on:** ADR-0087 (positions on a curve), ADR-0056 Decisions 3, 5, 6 and 7 (a class IS its
graph; the registrant's bond; duplicates; what the chain does not judge), ADR-0067 (classes are
chain data, kernels are the build), ADR-0069 / ADR-0075 (certification is of kernels, never of
weights), ADR-0059 (carve, never mint), ADR-0065 (a bond is the chain's address-shaped identity
for a PALW actor).
**Amends:** ADR-0087 Decision 1 (the market is keyed by a *line*, of which the class's own is the
first — byte-identical for every class that has only its own), Decision 4 (the registrant leg is
the line's *owner's*, and the owner may share it with an adopted contributor), Decision 7 ("a new
version is a new class" is narrowed to a new *graph*; new weights are a new *version* of a line,
and the market stays); ADR-0056 Decision 6's admission clause ("an attempt whose `artifact_root`
differs is admission-rejected") becomes "differs from every root in force for the class".

## 0. The sentence this ADR is

A class is its graph and keeps it; it is the unit of work, share, certification and the court,
and none of that moves when weights do. A **line** is a model in the market's sense — a class,
an owner, a name — and the owner's developer publishes its **versions**, one signed object each,
as often as it likes; nothing is examined and nothing is voted. The chain keeps every version,
counts how each is used, records what the developer and anyone else *declare* about it, and
prices the line on ADR-0087's curve. Learning is one team's; research is open (a proposal is a
record, and adopting it pays); competition is between lines; the judge is the market.

## 1. What exists, and the wall this ADR goes through

* **A class IS its graph; the weights are a separate field written once.** `class_id ==
  profile.shape_profile_id()` — the borsh of the geometry, node tables included (ADR-0056,
  ADR-0067). `PalwClassStateV2.artifact_root` is written at the `ClassRegistered` arm and mutated
  by no path; the same graph with new weights is the same class and is refused `DuplicateClass`
  (ADR-0053's Family M dead end: "only a re-mint does"). So the operator's V1 → V2 → V3 on one
  model has no path today, and ADR-0087 Decision 7's "a new version is a new class" cannot be
  taken for weights at all.
* **Whose class it is, is already a chain fact.** `PalwClassStateV2.registrant_bond:
  Option<PalwBondKeyV2>` — "whose bond paid for this class to exist" (ADR-0056 Decision 3),
  `None` exactly for the classes a genesis registered. A bond is the chain's address-shaped
  identity: an ML-DSA-87 key, a collateral outpoint, a `payout_payload` the coinbase pays
  (ADR-0065). ADR-0087 already pays the registrant leg to that payload. This is the "registered
  address" the operator means, and it is post-quantum; an EVM address is its window (ADR-0089),
  never the owner.
* **Certification is of kernels, never of weights** (ADR-0069 Decision 2; ADR-0075 §4: "a
  fixture drill certifies kernels, not weights"). `pwu_per_inference`, share, budget, target
  seed, exposure, the court's ceilings: all functions of the graph. A new set of weights on a
  certified graph runs on the certified kernels and needs no drill, no binding and no code change.
* **The chain judges no quality** — ADR-0056 Decision 7, kept and now fully respected: "the gate
  checks *adjudicability*, never *quality* … a benchmark is an oracle." What the chain can count
  is *paid work*: every accepted claim names a class, an executor and a root, and the fold can
  attribute it to a version.
* **Where a claim's root lives.** An attempt claim carries `artifact_root` and admission compares
  it with the class's (`consensus/core/src/palw_admission_v2.rs:252`); a free-prompt claim
  carries none (`consensus/core/src/palw_fp_admission_v3.rs` never reads it) — the class has one
  root, so it never had to; the court's operand openings prove against `class.artifact_root`
  (`consensus/core/src/palw_state_v2.rs:9467`).
* **The market of ADR-0087 is per class, implemented** (`f8d54d9b`): `model_markets` and
  `model_positions` keyed by `class_id`, the rows in the state root only when non-empty, the
  carriage's tagged tail, `ModelBuy`/`ModelSell`, `palw_model_market: Option<ForkActivation>`,
  `None` everywhere.
* **The class's name is off the chain.** The worker manifest's `model_id` (`Qwen/Qwen2.5-1.5B/
  graph-v2`) is a catalog string; the profile carries no name and no tokenizer identity. A
  line's name is therefore a new chain fact, and the line's identity must not depend on a name
  alone (a name is squattable; an owner is not).

## 2. The requirement, and the shape that answers it

The operator's list, in this ADR's words:

| the operator's step | the chain's object |
|---|---|
| ① one address = one developing entity | a **line** has one **owner** bond; the founding line of a class is its registrant's |
| ② the developer strengthens V1 → V2 → V3 | `ModelVersionPublished`, signed by the line's **developer** bond, as often as it likes |
| ③ every version is recorded, none deleted | `PalwModelVersionV1` rows — root, parent, declared metadata, chain-counted usage — a bounded window in state, the whole history in the explorer |
| ④ users use the model; ⑤ PALW processes the use | claims name their root; the fold attributes each accepted claim to `(line, version)` and counts it |
| ⑥ the position is priced; ⑦ the AMM discovers the price | ADR-0087's market, keyed by line; ADR-0089's EVM face |
| ⑧ a highly-rated line has the strongest reason to keep going | the registrant leg is the owner's, per trade, for as long as the line trades — and an adopted contributor's share of it, per version |

And what makes strengthening *easy*, which the operator asked for beyond the list:

* **No gate but a signature.** A version is one object under one fence; no exam, no drill, no
  re-binding, no re-registration, no code change, no rent beyond carriage. The graph is
  certified; the weights are the developer's.
* **A preview before a promotion.** A developer can publish a version as a *preview*: its root
  is in force (executors may run it, users may ask for it, the chain counts it) while the line's
  *current* version stays what it was. Real usage of V(n+1) is measured before V(n+1) is made
  the default — A/B on the chain's own counters, then `ModelVersionPromoted`.
* **Rollback is a publish.** Re-publishing an older root as a new version is legal and cheap;
  the history says so.
* **A grace, so producers never fall off.** After a promotion the previous current root stays
  admissible for `PALW_VERSION_GRACE_DAA_V1`, so a producer that has not fetched the new
  artifact keeps producing.
* **Open research with attribution and pay.** Anyone with a bond may attach a **proposal** (a
  root and a note) to a line; the developer evaluates it off the chain, and if the next version
  adopts it, the version says so and the owner's chosen share of the registrant leg goes to the
  proposer for as long as that version is current. "学習は中央化、研究はオープン化" with a
  ledger.
* **Cold owner, hot developer.** The owner bond names a developer bond (and a maintainer); the
  key that publishes daily is not the key that can transfer the line. A stolen developer key can
  publish junk, which the market sees and the owner revokes; it cannot take the line.
* **Competition on one architecture.** More than one line may live on one class — Developer B
  founds `QWEN-B` on the same graph as Developer A's `QWEN-A` — and the market compares them.
  The class's share and cadence are the class's; a block is a block whichever line's root it ran.

## 3. Decisions

**Decision 1 — A line is `(class, owner, name)`; the class's own line is the first, and its id
is the class id.** `PalwModelLineV1 { line_id, class_id, owner: Option<PalwBondKeyV2>, developer:
Option<PalwBondKeyV2>, maintainer: Option<PalwBondKeyV2>, name: Vec<u8> (≤ 64), founded_daa,
current: u32, previews: Vec<u32> (≤ PALW_MODEL_PREVIEWS_V1 = 2), versions_published: u32,
contributor_permille_of_leg: u16, status: Active | Retired }`. Every class has one **founding
line** — `line_id = class_id`, `owner = registrant_bond`, `name` = the class's catalog name as
the registrant wrote it, version 1 = the registration's `artifact_root` — which exists
implicitly: no row is written until something about it changes (a version, a role, a retire), so
a chain where nothing happened has no line rows and ADR-0087's per-class market is, byte for
byte, the founding line's. A genesis class (`registrant_bond = None`) has an unowned founding
line: one version, no developer, no way to publish — the genesis card may name an owner bond
for a genesis class at the next re-mint (ADR-0075 §7's card gains `genesis_line_owners`), and
mainnet, born floor-only with every model registered later (ADR-0075 Decision 8), never has an
unowned model line. A further line on a class is founded by `ModelLineFounded { class_id, name,
founder: PalwBondKeyV2, root, signature }`: `line_id = H("misaka-palw/model-line/id/v1" ‖ class_id
‖ founder ‖ name)` — the owner is in the id, so a name is never squatted, only shared; at most
`PALW_MODEL_LINES_PER_CLASS_V1 = 64` lines per class; the founder becomes owner, developer and
maintainer; version 1 is `root`. The floor (`is_base_class`, no artifact) has no line.

**Decision 2 — A version is one signed object, and the developer signs it.**
`ModelVersionPublished { line_id, version, root, parent: Option<u32>, adopted_from:
Option<Hash64>, runtime_hash: Option<Hash64>, dataset_commitment: Option<Hash64>,
training_config_hash: Option<Hash64>, notes_hash: Option<Hash64>, preview: bool, signature }`,
signed by the line's developer bond over `palw_model_version_message_v1(network_domain, line_id,
version, root, parent, adopted_from, the four hashes, preview)` under
`PALW_MODEL_VERSION_MLDSA87_CONTEXT`. Refused unless: the line exists (or is a class's implicit
founding line with an owner) and is Active; `version == versions_published + 1` (versions are
dense and monotone — the "commit number"); the developer bond is Active; `parent` names an
existing version or is `None`; `adopted_from` names a proposal of this line; `root` is not the
current root (a no-op version is refused; re-publishing an *older* root is allowed — that is a
rollback); a preview is refused when the line already holds `PALW_MODEL_PREVIEWS_V1` previews.
The fold writes `PalwModelVersionV1 { root, parent, adopted_from, runtime_hash,
dataset_commitment, training_config_hash, notes_hash, published_daa, published_by, status:
Current | Preview | Superseded { until_daa } | Withdrawn, usage: PalwVersionUsageV1 }`. When
`preview` is false the version becomes **current** at once and the previous current becomes
`Superseded { until_daa: published_daa + PALW_VERSION_GRACE_DAA_V1 }`. `ModelVersionPromoted {
line_id, version, signature }` (developer) turns a preview into the current the same way;
`ModelVersionWithdrawn { line_id, version, signature }` (developer) takes a preview out of force
at once, or a superseded version out of force before its grace ends — a current version cannot
be withdrawn, only succeeded. The declared hashes are **declarations**: the chain checks their
length and nothing else, records who signed them, and labels them so in every surface (README
principle 4: the chain never takes the host's word — here it does not pretend to).

**Decision 3 — The roots in force for a class are the union of its lines'.** For a class at
`daa`: the founding `artifact_root` if the class has no line rows; otherwise, over every Active
line of the class, its current version's root, its previews' roots, and the roots of superseded
versions whose `until_daa > daa`. An attempt claim is admitted iff the root it names is in that
set (ADR-0056 Decision 6 item 5, restated); the set is bounded by lines × (1 + previews + 1). The
fold records, for every claim admitted while the class has line rows, the root it named
(`claim_roots: claim → root`, a state-root collection, removed with the claim), and the court's
operand openings prove against `claim_roots[claim]` when present and `class.artifact_root`
otherwise (`consensus/core/src/palw_state_v2.rs:9467`, amended). **A free-prompt claim names no root today**; under
this fence the free-prompt job gains `artifact_root` in its next version (v6), and until that
version ships a free-prompt claim is attributed to the class's founding line's current root and
replayed against it — a recorded gap, not a rule (§8).

**Decision 4 — Usage is counted by the fold, per version, and it is the one measurement the
chain makes.** `PalwVersionUsageV1 { attempt_claims: u64, fp_claims: u64, work_leaves: u128,
first_used_daa: Option<u64>, last_used_daa: Option<u64> }`, incremented at every accepted claim
that names (or is attributed to) the version's root, in the same arm that admits it. A voided
claim (court fraud) is subtracted at the voiding. These are paid inferences — ADR-0072's "the
ticket is the execution" — and nothing else; they say what was *used*, never how *good* it was,
which keeps ADR-0056 Decision 7 whole.

**Decision 5 — Evaluations are declarations, from anyone, and say who declared them.**
`ModelEvaluationPosted { line_id, version, evaluator_id: Hash64, score_permille: u32,
report_hash: Hash64, by: PalwBondKeyV2, signature }`, signed by `by`. At most
`PALW_MODEL_EVALUATIONS_PER_VERSION_V1 = 16` per version, first come; one per bond per version.
The fold stores `(evaluator_id, score_permille, report_hash, by, posted_daa)` and marks whether
`by` is the line's developer or maintainer (the line's own word) or a stranger's. No rule reads a
score. The explorer shows them beside the usage counters and the price, which together are the
"benchmark" the operator asked for as an indicator: what the developer says, what strangers say,
what was used, what it trades at — four columns, none of them a consensus verdict.

**Decision 6 — Roles: owner, developer, maintainer; and the owner may hand the line over.**
`ModelLineRolesSet { line_id, developer: Option<PalwBondKeyV2>, maintainer:
Option<PalwBondKeyV2>, contributor_permille_of_leg: u16, signature }` (owner) — `None` means
"the owner"; `ModelLineOwnerTransferred { line_id, new_owner: PalwBondKeyV2, signature }` (owner;
the new owner's bond must be Active; developer and maintainer are reset to the new owner);
`ModelLineRetired { line_id, signature }` (owner): the line leaves Active, its market is
`closed_to_buys` (ADR-0087 Decision 7's rule, now per line), its roots leave force after the
grace, its history stays. The registration exposure of ADR-0056 Decision 3 stays with the
registrant's bond whatever the owner does: it is the registration's rent, not the line's
title. Nothing here moves a position: ADR-0087 Decision 5 stands, and the operator's own rule —
development rights may transfer, positions may not — is exactly that.

**Decision 7 — Proposals: open research, recorded, and paid when adopted.** `ModelProposalPosted
{ line_id, root, note_hash: Hash64, by: PalwBondKeyV2, signature }` (any Active bond, at most
`PALW_MODEL_PROPOSALS_PER_LINE_V1 = 32` open per line, first come; the developer may
`ModelProposalClosed { line_id, proposal_id, signature }` to make room): `proposal_id = H(line
‖ root ‖ by)`. When a version's `adopted_from` names a proposal, the fold records the adoption
on both rows and, for as long as that version is current, pays `contributor_permille_of_leg` of
the line's registrant leg to the proposer's payout payload (Decision 8). The developer evaluates
proposals off the chain, in its own environment, on its own data — which is where a 27B model's
evaluation happens anyway; the chain records the candidates, the choice and the credit.

**Decision 8 — The registrant leg is the owner's, by line, and shared when the owner says so.**
ADR-0087 Decision 4's 1 % leg on every MSK leg of a line's market goes to the line's **owner**
bond's payout payload — the registrant's, for a founding line whose ownership never moved, so
ADR-0087's arithmetic is unchanged; burned when the line has no owner (a genesis line), as
ADR-0087 already burns it. When the current version was adopted from a proposal and the owner
has set `contributor_permille_of_leg > 0`, that share of the leg goes to the proposer's payload
and the rest to the owner's. The market row gains `owner_paid_sompi` and `contributor_paid_sompi`
beside `registrant_paid_sompi`'s meaning (the field is kept and now means the owner's total).
Nothing else in the split moves: 5 % burned, 1 % the leg, 94 % net, 12 % round trip.

**Decision 9 — The market is keyed by line, and the founding line's key is the class id.**
ADR-0087's `model_markets` and `model_positions` are keyed by `line_id`; `ModelBuy`/`ModelSell`'s
`class_id` field is renamed `line_id` (the fence is `None` everywhere; no chain carries the
objects). For a class with only its founding line every value is identical to ADR-0087 as
written. A buy on a non-founding line requires the line to exist and be Active; a sell never
requires anything but the units. Each line gets its own facade address on the EVM (ADR-0089).

**Decision 10 — State, and no root bump.** New collections: `model_lines: BTreeMap<Hash64,
PalwModelLineV1>`, `model_versions: BTreeMap<(Hash64, u32), PalwModelVersionV1>` (the last
`PALW_MODEL_VERSION_HISTORY_V1 = 64` per line stay in state; older rows are evicted at the 65th
publish and live in the explorer — `versions_published` on the line keeps the count monotone),
`model_proposals: BTreeMap<Hash64, PalwModelProposalV1>`, `model_evaluations: BTreeMap<(Hash64,
u32, PalwBondKeyV2), PalwModelEvaluationV1>`, `claim_roots: BTreeMap<Hash64, Hash64>`. Each
enters the state root only when non-empty and the carriage carries them in a second tagged tail
(`0x88`) after ADR-0087's, so a chain with no line row commits the root and the bytes it commits
today (ADR-0087's implementation rule; the root rides in the header and may not move).
`PALW_STATE_V2_VERSION` stays at 20.

**Decision 11 — A consensus rule armed by activation, never by regenesis.** `palw_model_lines:
Option<ForkActivation>` on the params, top level, bare; below it the ten objects are refused by
name at acceptance and the class has one root; past it the founding lines exist and the objects
apply. Independent of `palw_model_market`: a registry without a market is useful, and a market
without lines is ADR-0087. The fingerprint moves only where the flag is set. Rents: a line
founding, a proposal and an evaluation pay `PALW_MODEL_OBJECT_RENT_SOMPI_V1` (§4) burned by
ADR-0075 SA-2's don't-mint mechanism, so the bounded tables cannot be filled for free; a version,
a promotion, a withdrawal, a roles change, a transfer and a retire pay carriage only — the
developer's daily objects are not taxed, which is the point.

**Decision 12 — What a participant reads and does.** RPC `getPalwModelLine(line_id)` (the row,
the current and preview versions, the roots in force), `getPalwModelVersion(line_id, version)`
(the row with usage and evaluations), `getPalwModelLines(class_id)`, `getPalwModelProposals
(line_id)`; `getPalwModelMarket` and `getPalwModelPositions` keyed by line. CLI: `misaka palw
line found|show|log|roles|transfer|retire`, `misaka palw version publish|promote|withdraw|
show`, `misaka palw proposal post|close`, `misaka palw evaluate`. The explorer's Model page is
the operator's picture: `QWEN-27B-001 · V1 ── V2 ── V3 ── V4 ← CURRENT`, each commit with its
hashes, its declared and stranger evaluations, its usage, and the line's curve beside it. The
class SDK's `add-model` flow gains `--line` and `--version`: a developer registers a class once,
then publishes.

## 4. What this costs, stated before it is measured

* **Objects:** a version ≈ 64 (line) + 4 + 64 (root) + 4 × 65 (hashes) + 4,627 (signature) ≈
  5.1 KB; a founding ≈ the same plus the name; a proposal/evaluation ≈ 4.9 KB. All under the
  standard mass; no chunking.
* **Fold:** O(1) per object; O(lines × 4) per attempt admission for the roots-in-force set
  (≤ 256 hashes per class at the caps); one map write per accepted claim for the usage counter.
* **State:** a line row ≈ 300 B; a version row ≈ 400 B (64 kept per line ⇒ ≤ 26 KB per line);
  a proposal ≈ 200 B (≤ 32 open); an evaluation ≈ 200 B (≤ 16 per kept version); `claim_roots`
  one 128 B row per live claim of a class with lines. A network with 20 lines and full histories
  holds ≈ 1 MB, the size of one ADR-0075 drill.
* **Rents (operator's numbers):** `PALW_MODEL_OBJECT_RENT_SOMPI_V1 = 1 MSK` for a founding, a
  proposal, an evaluation — the tables they fill are bounded and the rent is what makes the
  bounds cost something to reach; a version pays carriage only.
* **Latency:** a version is in force in the block that accepts it; a promotion the same; the
  grace for the superseded root `PALW_VERSION_GRACE_DAA_V1 = 4,000` (≈ 2.8 days at one block a
  minute) is how long a producer has to fetch the new artifact.
* **The fee table is ADR-0087's** — 5 % burned, 1 % the leg (owner, or owner + contributor),
  94 % net.

## 5. Security — the four principles, checked before it is built

*A free field is a free draw; silence is not a verdict; weight is what certification buys; the
chain never takes the host's word* (README §"Security amendments").

| # | attack | what stops it, and the residual |
|---|---|---|
| A1 | **A developer publishes broken or malicious weights as current.** | Producers keep the previous root through the grace; a preview lets them (and users) try first; the market and the usage counters are the judgment; the owner can withdraw a preview and succeed a current. A bad model costs its line, not the chain: weight and share are the class's and unchanged (principle 3). |
| A2 | **A stolen developer key.** | It can publish and promote; it cannot transfer, retire, set roles or the contributor share — those are the owner's, which the design keeps cold. The owner re-sets the developer; the junk versions stay in the history as a record of the theft. Residual: a stolen *owner* key is a stolen line, as a stolen key is a stolen anything. |
| A3 | **Name squatting / impersonation** (`QWEN-27B-001` founded by a stranger). | The line id includes the founder; two lines may share a name and the explorer shows the owner beside it; nothing routes by name. |
| A4 | **Filling the bounded tables** (lines, proposals, evaluations). | Per-class / per-line / per-version caps, first come, and a burned rent on the filling objects; the developer can close proposals. Residual: a well-funded party can hold a line's 32 proposal slots for 32 MSK until closed — the developer's slots to clear. |
| A5 | **Declared hashes that lie** (dataset, config, evaluation). | They are declarations, labelled as such, signed by a bond that can be named; no rule reads them; they are the explorer's and the market's to weigh (principle 4). |
| A6 | **Redirecting the fee leg.** | It follows the owner, and the owner moves only by the owner's signature; positions never move (ADR-0087 D5). |
| A7 | **A claim naming a root that is not in force.** | Refused at admission, as a wrong `artifact_root` is today; the set is bounded and read from the fold. |
| A8 | **Attributing free-prompt usage to the wrong version** (no root on the fp claim). | Recorded gap (Decision 3): counted on the founding line's current until job v6 ships; the court's replay for such a claim uses that root — an executor that ran a preview on the fp lane before v6 is one whose claim the court may misjudge, so the SDK refuses fp jobs on non-current roots until v6. |
| A9 | **A version that does not fit the graph** (wrong tensor shapes). | Executors cannot run it: no claims, no usage; the developer's problem, visible in the counters. |
| A10 | **Two lines racing on one class for the class's cadence.** | There is no race: the class's share and budget are the class's; every accepted block adds to it whichever line's root ran. Lines compete for users and for the market, which is what they are for. |

## 6. Invariants the tests must hold

* **L1 (one owner).** A line has exactly one owner; the founding line's is the registrant; a
  transfer is signed by the current owner; positions are untouched by any role object.
* **L2 (dense versions).** `versions_published` increases by one per publish; version `n` is
  refused unless `n == published + 1`; the history window holds the last 64 and the count never
  decreases.
* **L3 (roots in force).** `class_roots_in_force(class, daa)` equals the union of Decision 3
  over the class's Active lines; an attempt naming any other root is refused; a superseded root
  leaves the set at `until_daa`; a withdrawn preview leaves it at once.
* **L4 (attribution).** Every accepted claim that names a root increments exactly one version's
  usage; a voided claim decrements it; `claim_roots` has a row for every live claim of a class
  with line rows and none for a class without.
* **L5 (the leg).** ADR-0087 M1–M8 hold with the key renamed; the owner's and the contributor's
  legs sum to the registrant leg; a genesis line burns it; a transfer moves it from the next
  block.
* **L6 (declarations are inert).** No transition reads a score, a dataset commitment or a
  runtime hash; a property test over the fold with random declarations finds the same state
  root for the same roots and usage.
* **L7 (bounds).** Lines per class, previews per line, proposals per line, evaluations per
  version, versions kept per line — each refused at the cap.
* **L8 (replay and revert).** The deltas replay to the same root and revert; the carriage of a
  chain without line rows is byte-identical to ADR-0087's; with rows it carries the `0x88` tail
  and a legacy reader refuses it rather than misreading it.
* **L9 (the fence).** The fingerprint is unchanged where `None`; the ten objects are refused
  below it; the founding line answers the same values below and above it for a class whose
  registrant never published.
* **L10 (court).** A court against a claim of a superseded or preview root replays against that
  root after the grace and after later versions.

## 7. Order of work

1. State: lines, versions, proposals, evaluations, `claim_roots`; the founding line's implicit
   row; the roots-in-force reader; the usage counters at admission and voiding; the tail; L2–L8.
2. Objects and arms: the ten objects, their messages and contexts, acceptance-layer signature
   checks (the bond's stored key), rents; L1, L7.
3. Admission: `palw_admission_v2` against the set; the court's operand root from `claim_roots`;
   L3, L10.
4. Market: the key rename, the owner/contributor legs; L5.
5. Params: the fence, the fingerprint pin; L9.
6. RPC, CLI, the SDK's `--line/--version`, the explorer page.
7. The free-prompt job v6 (`artifact_root` on the job) — its own change, closing A8.
8. Devnet drill: register a class, publish V2 as preview, produce on both roots, promote,
   post a proposal, adopt it in V3 with a contributor share, transfer the line, retire it; then
   testnet-11 by activation.

## 8. Implementation record (2026-09-05, `palw-adr0088-0089-impl`)

**Landed — §7 items 1–6 (state, objects and arms, admission, market, params, RPC and CLI);
item 7 (the free-prompt job v6) and item 8 (the devnet drill) are not, nor the SDK's
`--line/--version` and the explorer page.** The fold half is commit `bf4ed1d8`; the rest is the
branch's closing commit.

| where | what |
|---|---|
| `consensus/core/src/palw_model_lines_v1.rs` | The constants (64 lines a class, 2 previews a line, 64 versions of history, 32 proposals a line, 16 evaluations a version, a grace of 4,000 DAA, a name of at most 64 bytes, a rent of 1 MSK on a founding, a proposal and an evaluation); the rows (`PalwModelLineV1`, `PalwModelVersionV1`, `PalwModelProposalV1`, `PalwModelEvaluationV1`); `founding_line_v1` — the class's own row synthesised, never written: its id the class id, its owner the registrant, its version 1 the genesis root; `model_line_id_v1` (class, founder, name — so a name is shared, never squatted) and `model_proposal_id_v1`; `split_owner_leg_v1` (Decision 8); the ten messages under ONE ML-DSA-87 context, `misaka-palw-model-line-v1`. |
| `palw_state_v2.rs` | `model_lines`, `model_versions`, `model_proposals`, `model_evaluations`, `claim_roots` on the state, **in the root only when non-empty** (Decision 10 — the same shape ADR-0087 §7 taught); the carriage tail `0x88`; delta variants with replay and revert; the ten objects appended after `ModelSell`; the fold arms — a founding pays rent, needs an active bond and a name unused on the class; a publish is the dense next version with its parent in the history, at most two previews, an adopted proposal marked; a promotion opens the grace on the root it supersedes; a withdrawal refuses the current; roles, transfer and retirement are the owner's; proposals and evaluations are any active bond's; `class_roots_in_force(class, daa)` and `model_version_of_root`; usage counted on the version a claim named (`note_claim_usage`) and uncounted when the claim is voided; `apply_palw_transition_v7` taking `PalwTransitionExtrasV1 { model_lines_active, .. }` (`v6` stays for callers that carry no extras). |
| `palw_admission_v2.rs` | Decision 3: an attempt's `artifact_root` must be one of the class's roots in force at the block's DAA, else `ArtifactRootMismatch`. |
| `palw_lifecycle_objects_v2.rs` | Ride gates for the ten objects; a buy's sink binding keyed by line. |
| `palw_model_market_v1.rs` | `contributor_paid_sompi` beside `registrant_paid_sompi`; the 1 % leg split owner/contributor by the line's permille. |
| `config/params.rs` | `palw_model_lines: Option<ForkActivation>` requiring `palw_model_market`; `palw_model_lines_fence` / `palw_model_lines_active_at`; pinned in the fingerprint; `None` on every preset. |
| `virtual_processor/processor.rs`, `utxo_validation.rs`, `body_validation_in_isolation.rs` | The ten objects refused by name below the fence at the block's DAA; each signature verified at acceptance against the pubkey the bond registry stores for the bond the object names, and the role checked against the line's row (the owner for roles/transfer/retire, the developer for a publish or a move, any active bond for a proposal or an evaluation); carrier fees collected once the fence is armed. |
| `kaspad/src/palw_panel.rs` | The ten names in the panel's transcript. |
| RPC | `getPalwModelLine(lineId)` (the row, its roots in force, its current root), `getPalwModelVersion(lineId, version)` (the row with its evaluations), `getPalwModelLines(classId)`, `getPalwModelProposals(lineId)` — ops 173–176 through core, service, gRPC and wRPC; `getPalwModelMarket` and `getPalwModelPositions` re-keyed to `lineId`; the integration test's round trips. |
| CLI | `misaka palw line-show`, `line-log`, `line-list`, `proposals` (readers, `--json`); `line-found`, `version-publish [--preview]`, `version-promote`, `version-withdraw`, `line-roles`, `line-transfer`, `line-retire`, `proposal-post`, `proposal-close`, `evaluate` — each signs the fold's own message with the bond's key, checks the key owns the bond the row names, and sends nothing without `--yes`. |

**Tests.** `palw_model_lines_v1` (6): a line id carries its founder; the founding line is the
registrant's with the roles defaulting to the owner (L1); a version is in force by its status and
the grace (L3); usage counts paid work and a voiding takes it back (L4); the leg is the owner's
unless the owner shares it (L5); every message binds every field. `palw_state_v2::tests::model_lines`
(8): the founding line is the registrant's and a publish moves the root with a grace (L1, L2, L3);
a preview is in force but not current until promoted and a withdrawal takes it out (L3); roles are
the owner's to set, a transfer resets them, a genesis line has nobody (L1); a proposal is adopted by
a version and the owner shares the leg (L5, L7); usage is counted on the version a claim named and
admission reads the roots in force (L4, L3); a founded line shares the class and the roots are the
union (L3, L7); evaluations are declarations from anyone and say whose they are (L6); the carriage
stays legacy until a line row exists and the deltas replay and revert (L8). The params test pins L9
(fingerprint unchanged where `None`, the ten refused below the fence). L10 is the court reading
`claim_roots`, covered by reading. The consensus-core suite is green at 1,868.

**What the implementation taught.** (1) The founding line is best *synthesised* rather than
written at activation: a class registered before the fence has a line the moment the fence is
armed, with no genesis edge and no object. (2) A free-prompt claim names no root today (the job
carries the class only), so its usage is attributed to the founding line's current version until
job v6 carries `artifact_root` — recorded as the open item it is, not hidden in a default.
(3) One signing context for all ten objects is enough because every message spells its own kind
first; ten contexts would have been ten places to get a domain wrong. (4) The rent is charged on
the three objects anyone may post (founding, proposal, evaluation) and on nothing an owner posts
about its own line: the owner already paid to own it.

**Not landed.** The free-prompt job v6 (§7 item 7); the SDK's `--line/--version`; the explorer's
line and version pages; the devnet drill (§7 item 8); arming on testnet-11. As with ADR-0087, the
acceptance-layer signature checks are covered by reading — a processor-level fixture for a signed
lifecycle object still does not exist.

## 9. What the first draft decided, and why it was withdrawn

The draft (commit `4f198f6e`) chose weights by an exam: candidates proposed by any bond with a
burned rent, frozen before the questions existed; questions drawn by the ADR-0073 SA-1 beacon
from a permissionless pool of pure integer programs emitting token ids; answers as court-triable
claims graded by an id-prefix comparison; succession by two integer inequalities on wins and
losses; a chained index of accepted improvements as the benchmark; the registrant leg split with
the head's author; an optional bounty. It answered "who may strengthen a model" with "anyone who
wins", and it was consistent — but it put the choice of a 27B model's next weights in a
consensus verdict over a syllabus somebody had to write, with a residual (its A1) that a
well-funded author could bend a window's exam. The operator's reframing removes the verdict:
the developer chooses, the chain records, the market judges. What the draft got right is kept —
the class keeps its graph; roots in force with a grace; claims name their root and the court
replays it; the fence pattern; rows in the root only when non-empty; a position never moves —
and its exam machinery (the VM, the pool, the draw, the verdict, the index, the author split,
the bounty) is withdrawn entire.

## 10. What is deliberately not decided

* Every `_V1` constant: previews per line, lines per class, proposals, evaluations, versions
  kept, the grace, the rent. The operator sets them when the flag is armed; §4 carries the
  examples.
* The genesis card's `genesis_line_owners` — whether the RC's genesis model classes get owners at
  the next re-mint or are re-registered by their maintainers as post-genesis classes.
* Whether a maintainer should also be able to *withdraw* a preview (an operational safety
  valve) — today only the developer can.
* A per-line *service* fee (a share of a claim's escrowed reward to the owner, for the use of
  the model) — a different ADR: it touches ADR-0042 Decision 10's carve and the coinbase, not
  the market.
* Whether a line may move to a *new graph* (a wider context, a new architecture) and keep its
  market — ADR-0087 Decision 7 stands for a new graph; a line-level graph migration is a market
  re-keying with its own ADR.

## 11. Number hygiene

This is ADR-0088, revised in place on 2026-09-05 before it landed anywhere but its own branch.
The README's next free number was 0088 after 0087's row; it became 0089 with this row and 0090
with ADR-0089's. The text lives on `palw-adr0088-0089-impl` (branched from
`palw-adr0084-served-answer` at `c303b3b9`, the tip that holds ADR-0087's implementation); the
draft's branch `docs/adr-0088-succession-exam` is superseded and should not be merged.
