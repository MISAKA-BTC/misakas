# ADR-0088 — the class keeps its graph, and the exam names its weights

**Status:** PROPOSED 2026-09-05, design only (no implementation yet). Requested by the operator
on 2026-09-05, on top of ADR-0087: a model is to be replaced every few weeks by a strengthened
one (数週間ごとに強化されたものに更新); how much stronger each replacement is becomes a benchmark
(どれくらい強化されたかをベンチマークに); the benchmark is the indicator a position is bought and
sold on (売り買いの指標); and the design must let many parties strengthen a model together,
decentralised, with the improvement reaching the position's price (分散されつつみんなでモデルを
強化し合い、ポジションの価格上昇のベンチマークに繋げる).
**Builds on:** ADR-0087 (a position is bought from the curve and sold back to it), ADR-0056
Decisions 5, 6 and 7 (a class IS its graph; duplicates are priced; what the chain does not
judge), ADR-0067 (classes are chain data, kernels are the build), ADR-0069 / ADR-0070 / ADR-0075
(certification is of kernels, on a lane, by object — never of weights), ADR-0074 Decision 2 (the
beacon is the chain's own), ADR-0073 SA-1 (`k ≥ 3`), ADR-0077 (a prompt a person would type is a
claim the court can try), ADR-0078 Decision 10 (an integer metric anyone recomputes), ADR-0084 /
ADR-0086 (the answer's ids ride; the opening carries the fold), ADR-0059 (carve, never mint).
**Amends:** ADR-0087 Decision 4 (the registrant leg gains a second payee) and Decision 7 ("a new
version is a new class" is narrowed to a new *graph*; new *weights* on the same graph succeed
inside the class, and the market stays); ADR-0056 Decision 6's admission clause ("an attempt
whose `artifact_root` differs is admission-rejected") becomes "differs from a root in force".
**Does not amend:** ADR-0056 Decision 7 — see §2, which is the argument that it need not.

## 0. The sentence this ADR is

A class is its graph and keeps it. Its weights are its versions. Which version is in force is
decided by an exam nobody has seen before the candidates were frozen, written by everyone, drawn
by the chain, answered by claims the court can try, and graded by a byte comparison every node
makes. The position of ADR-0087 is on the class, so it is on the line of versions — and the
chain of accepted improvements is an index the market reads.

## 1. What exists, and the wall this ADR has to go through

* **A class IS its graph, and the weights are a separate field written once.**
  `class_id == profile.shape_profile_id()` (ADR-0056 §"What already holds"; ADR-0067: "the borsh
  of the whole profile, node tables included"). `PalwClassStateV2 { artifact_root, … }` carries
  the weights' root beside it — "what artifact openings prove against; an attempt whose
  `artifact_root` differs is admission-rejected" — written at the `ClassRegistered` arm and
  mutated by no path. Same graph with new weights is the same `class_id`, and re-registering it
  is refused `DuplicateClass`. ADR-0053 recorded the dead end this makes for a repaired worker:
  "No configuration, rebuild or restart closes that loop. Only a re-mint does." ADR-0056
  Decision 6 decided the other direction on purpose ("the same `artifact_root` under a different
  profile is a different class … the gate does not deduplicate by weights, and must not") and
  left this one where it was.
* **So ADR-0087 Decision 7 is right for a graph and impossible for weights.** "A new version is a
  new class with a new market" is what a wider context or a new architecture gets, and should.
  Continued training, a fine-tune, a better quantisation of the same graph — the thing the
  operator means by 強化 every few weeks — has no registration path at all today, and if it did
  (a different name in the profile), it would open an empty market beside a full one and
  Decision 7 would tell the holders to sell one story and buy the next through 12 % of fees.
* **Certification is of kernels, never of weights.** ADR-0069 Decision 2, restated by ADR-0075
  §4: "a fixture drill certifies kernels, not weights". `pwu_per_inference` is the canonical
  job's counted step leaves — a property of the graph. Share, budget, target seed (ADR-0076),
  exposure (ADR-0076 §6), the court's ceilings: all functions of the graph. Nothing consensus
  holds about a class changes when its weights do. That is the fact this ADR is built on.
* **The chain judges no quality.** ADR-0056 Decision 7, kept deliberately: "the gate checks
  *adjudicability*, never *quality* … Confusing the two would put a benchmark inside consensus,
  and a benchmark is an oracle." ADR-0087 §1 lists what the chain does not know: "any human
  preference score". What precedent does admit is a **deterministic integer measurement every
  node recomputes from committed material**: ADR-0075 Decision 2's transition-side grader ("the
  court grades; nothing else vouches", bounded per block), ADR-0078 Decision 10's metric ("the
  metric is in the artifact. 'Better' is the difference of two derivations' metrics, and anyone
  can recompute both"), ADR-0070's leaf-by-leaf adjudication.
* **Consensus holds token ids, not text.** The profile names a `tokenizer_id` that "the envelope
  does not carry" (`palw_carriage.rs`); no tokenizer runs in consensus; a free-prompt claim in
  `User` mode carries its prompt ids, and the worker manifest publishes the class's `n_ctx`,
  `special_tokens` and `eog_token_ids` so a gateway "builds a segment-wise prompt from names and
  never from ids it guessed" (`PalwFpWorkerManifestV1`).
* **A prompt is already a claim the court can try**, and its answer's ids already ride: ADR-0077
  (rows, checkpoint-priced court), ADR-0084 (`FPA1` — "the answer's ids, bound to the claim's
  `output_root` by the ADR-0078 X6 recompute"), ADR-0086 (the seat replays and supplies its own
  leaves). The court needs the artifact by root and nothing the chain does not have.
* **The chain has one source of randomness it may use**: `derive_beacon_fact_v3`, the fold of
  the first `k ≥ 3` attempt blocks at or after a slot (`PALW_BEACON_FOLD_MIN_K_V1 = 3`), under
  ADR-0074 Decision 2's law — "MUST NOT read … any value one party can set".
* **The market of ADR-0087 is per class**, state v21, `palw_model_market: Option<ForkActivation>`,
  6 % of every MSK leg leaving the trade (5 % burned, 1 % to the class's registrant), the
  arithmetic in `palw_model_market_v1.rs`. At `74c34476`, the tip this ADR is written on, the
  fold is not yet committed; it is being written on the same branch the same day, and where
  this ADR names ADR-0087's shape it names the implemented one: a market opens at its first
  buy rather than at registration, and the state root does not bump for it.

## 2. The requirement, and why the exam is not the oracle ADR-0056 Decision 7 refuses

The operator asks for four things at once: (i) the weights of a model change every few weeks,
by whoever makes them stronger; (ii) how much stronger is a number on the chain; (iii) the
number is what a position is priced against; (iv) many hands can do the strengthening, and none
of them needs anyone's permission.

Decision 7 refuses a benchmark *inside the admission gate*: a party supplying the thing it will
be judged against (ADR-0055's three-way mistake), a maintainer's taste as a consensus fact, a
float, a clock. This ADR does not touch admission — a class is admitted by adjudicability, at
zero or the floor share, exactly as before — and the exam it adds has no term a party asserts:

| term of an exam | who chooses it | what pins it |
|---|---|---|
| the candidates | anyone, by a signed, bonded, rent-paying object | frozen before the questions exist |
| the questions | the chain, from the beacon, through public pure programs | regenerated by every node from state |
| the reference answer | computed by the same program, in integers | never asserted by anyone |
| a model's answer | a claim the court can try (ADR-0077) | replay; a false one is convicted and slashed |
| the score | a prefix comparison of token ids | recomputed at apply by every node |
| the verdict | two integer inequalities on counts | in the fold, at the window's close |

What remains chosen by people is the **syllabus** — the pool of programs the questions are drawn
from. It is chosen by everyone (permissionless, rent-priced), in public, a whole window before it
can be drawn, and it is equally visible to every candidate and to the incumbent. An exam whose
syllabus is public and whose questions are not is what an exam is. That is the whole of the
human input, and §5 prices the ways it can be bent.

## 3. Decisions

**Decision 1 — A class keeps its graph; its weights are versions; at most one root is in force,
two during a grace.** `PalwClassStateV2.artifact_root` stays what it is: the FOUNDING root,
inside the signed registration preimage, never rewritten. Beside it the class gains
`PalwArtifactHeadV1 { root, author: Option<PalwBondKeyV2>, since_daa, previous: Option<Hash64>,
grace_until_daa }` and a bounded history `roots: Vec<(Hash64, since_daa)>` (the last
`PALW_SUCCESSION_HISTORY_V1 = 8`). At founding the head is the founding root with `author =
None`. The admission rule of ADR-0056 Decision 6 item 5 becomes: a claim is admitted iff the root
it names is in force at its block — the head, or the head's `previous` while `daa <
grace_until_daa`. Every claim under this fence NAMES its root (attempt claims already do; the
free-prompt job gains `artifact_root` in the version this fence arms), so the court replays a
claim against the root the claim named, whichever the head is by the time the dispute runs, and
a retired root's material is served for the retention window of the claims against it, as the
head's is. `DuplicateClass` stands unchanged. Nothing in the share table, the budget, the target
seed, the exposure formula, the certified sets or the court's ceilings reads the head — they are
all the graph's — and ADR-0087's market is keyed by `class_id` exactly as written, so a position
is a position in the class and therefore in every version the class will have.

**Decision 2 — A successor is proposed by a bond, for a window, at a price.**
`ArtifactCandidateV1 { class_id, root, window, parent_root: Option<Hash64>, author:
PalwBondKeyV2 }`, signed by the author bond's ML-DSA-87 key under the network domain (the
registration's own discipline), carried on the lifecycle band. Refused unless: the class is
`Active` and has an artifact (the floor has none — Decision 10); the object lands in the window's proposal
phase; `root ≠ head.root` (`CandidateIsHead`); the author bond is Active; fewer than
`PALW_EXAM_MAX_CANDIDATES_V1 = 8` candidates are accepted for `(class, window)` in chain order
(`CandidateSlotsFull` — the rest wait a window). An accepted proposal pays
`PALW_EXAM_CANDIDATE_RENT_V1` by the ADR-0075 SA-2 mechanism — withheld from the accepting
block's reward by don't-mint, capped at the carrier's fee, so it is destroyed and not recovered
by a miner; a refused proposal pays carriage only. The rent is the price of a positive,
falsifiable claim ("this root will sit the exam and score"), sized against what sitting the exam
costs the incumbent's defenders (§4), so a proposal that never answers costs its author more
than it costs the class. `parent_root` is a free field and therefore decides nothing: it is
displayed by the explorer as the author's own attribution and is never paid. A root that was
head before may be proposed again — a rollback is a succession like any other.

**Decision 3 — The calendar is the chain's, and every class shares it.** Window
`w = ⌊(daa − activation_daa) / PALW_EXAM_WINDOW_DAA_V1⌋`, one length for the network, the
operator's number (§4 works the example at three weeks). Inside a window, in DAA:

| phase | from | to | what is accepted |
|---|---|---|---|
| proposals | `0` | `P = 0.30 W` | `ArtifactCandidateV1` for this window |
| freeze | `P` | — | the candidate set and the head root are fixed; the syllabus snapshot is taken |
| draw | first fold of `k ≥ 3` attempt blocks at or after `P` | — | the seed exists; every node generates the items |
| exam | draw | `E = 0.80 W` | `ExamAnswerV1` for this window |
| dispute tail | `E` | `W` | no new answers; the last answers' court windows run out |
| close | `W` | — | scores, the succession, the index — in the transition slot after ADR-0056 Decision 5's reclamation (slot 2e) |

Two constraints are checked at bundle assembly and refuse a bundle that breaks them, as
ADR-0055 Decision 3's window rule is: `W − E ≥` the free-prompt dispute window (an answer filed
at the last exam block must be refutable before it is counted), and
`PALW_SUCCESSION_GRACE_DAA_V1 ≤ P` (Decision 7's grace ends before the next freeze, so a
window's head is one root). A class with no accepted candidate at the freeze holds no exam that
window: nothing is drawn for it, nothing is scored, its index does not move.

**Decision 4 — The syllabus is a pool of templates, and a template is a pure integer program
that emits token ids.** `ExamTemplateV1 { tokenizer_id: Hash64, program: Vec<u8> (≤
PALW_EXAM_TEMPLATE_MAX_BYTES = 2,048), max_prompt_ids: u16, max_decode: u8 (≤
PALW_EXAM_MAX_DECODE_V1 = 16) }`, permissionless, unsigned, keyed by `template_id = H(bytes)`.
Run under `PALW_EXAM_VM_V1` — a purpose-built stack machine of at most 32 opcodes over `u64`
with a per-run gas ceiling `PALW_EXAM_GAS_V1 = 65,536` steps, a PRNG that is
`BLAKE2b(seed ‖ item_index ‖ counter)` and nothing else, and two outputs: `prompt_ids: Vec<u32>`
and `reference_ids: Vec<u32>`. No floats, no clock, no host input, no memory beyond a bounded
stack and a bounded scratch: ADR-0079's "a pure function needs no permissions", and the reason
the in-tree EVM was not taken (an optional, non-default feature, and larger than this one job).
Templates are written **per tokenizer**: a template for `tokenizer_id` `T` emits the whole prefill
— control tokens (`<|im_start|>` …) by their ids, the question, the instruction that shapes a
short answer — for the classes whose profile names `T`. Consensus never tokenises: fairness
needs identical inputs, not canonical ones, and every candidate of a class shares the class's
tokenizer because it shares the class's graph. A template emitting nonsense hurts every
candidate of that tokenizer equally, which is to say it hurts no one and is pruned (below).

Registration is priced and bounded: `PALW_EXAM_TEMPLATE_RENT_V1` burned by the SA-2 mechanism,
at most `PALW_EXAM_TEMPLATE_POOL_V1 = 512` templates per `tokenizer_id` (state cost ≤ 1 MiB per
tokenizer at the byte cap — the bytes must be state, because every node runs them; ADR-0078
Decision 1's "the thing never rides" is about artifacts the chain never runs, and this is the
opposite case), refused when the pool is full. At registration the transition runs the program
on `PALW_EXAM_TEMPLATE_PROBE_V1 = 16` fixed probe seeds and refuses a template whose sixteen
prompts are not sixteen distinct id sequences within its declared bounds, whose reference is
empty or longer than `max_decode`, or whose run exceeds the gas ceiling (`TemplateDegenerate`).
A template is drawable only in windows after the one it was registered in (`registered_window
< w`), so nobody registers a question for a candidate they can already see. **The syllabus is
pruned by discordance:** each drawn template accumulates, per class it was drawn for, the count
of items on which the compared roots disagreed; a template drawn in
`PALW_EXAM_TEMPLATE_PRUNE_WINDOWS_V1 = 4` contested windows with zero discordance in all of
them is evicted at the fourth close — an item every model gets right, or every model gets
wrong, measures nothing, and a pool squatted with junk empties itself at the squatter's rent.
Renewal: a template is evicted `PALW_EXAM_TEMPLATE_TTL_WINDOWS_V1 = 8` windows after
registration unless re-registered (same bytes, a fresh rent, the discordance record kept).

**Decision 5 — The draw is the beacon's, and the items are never state.** At the freeze the fold
records, per contested class, `PalwExamWindowV1 { window, class_id, head_root, candidates,
pool_digest: H(the template ids drawable for this tokenizer, sorted), seed: None }`. The seed is
the ADR-0073 SA-1 fold of the first `k ≥ 3` attempt blocks at or after `P`, written when the
chain has produced it (`NoBeaconYet` until then, and the exam phase does not open before it).
Item `i ∈ 0..PALW_EXAM_ITEMS_V1` is generated by every node as: `t = H(seed ‖ i) mod |pool|`
over the snapshot's sorted ids, skipping — deterministically, advancing `i`'s counter — a
template whose `max_prompt_ids + max_decode` exceeds the class's registered `n_ctx`; the program
runs with `(seed, i)`; the item is `(prompt_ids, reference_ids, max_decode)`. Items live in a
per-window cache and never in the state root: seed and snapshot digest are state, and the items
are their function. `PALW_EXAM_ITEMS_V1` is the operator's number; §4 works 1,024.

**Decision 6 — An answer is a free-prompt claim in everything the court can see, and nothing
else.** `ExamAnswerV1 { window, class_id, root, executor: PalwBondKeyV2, first_item: u16,
items: Vec<ExamItemAnswerV1> }` with at most `PALW_EXAM_BATCH_V1 = 64` items per object, each
`ExamItemAnswerV1 { answer_ids: Vec<u32> (≤ max_decode), output_root, trace_root,
execution_root, work_leaves, … }` — the fields a `PalwFreePromptCommitmentV3` carries for a job
whose prompt ids are the item's and whose decode is greedy at the item's `max_decode`,
`stop_reason` free (an EOG before the budget is an answer; the execution still runs the declared
budget, as every free-prompt job does). The object is signed by the executor bond's key. At
apply the fold: regenerates the items; checks `answer_ids` against `output_root` by the X6
recompute; reserves the executor's exposure for each item as a free-prompt claim of that length
would (ADR-0076 §6's target-free quantity); grades each item — **correct iff `answer_ids` begins
with `reference_ids`** — and writes `PalwExamBatchV1 { key: (window, class, root, executor,
first_item), digest, answered_bits, correct_bits, void_bits, dispute_deadline_daa }` (≈ 300
bytes; the batch's bytes stay in the carrier for a challenger to read). An item is disputable
exactly as a free-prompt claim is: a court against `(batch, item)` under ADR-0077's ladder, the
material served under ADR-0084/0086; a conviction sets `void_bits[item]`, releases nothing to the
executor and slashes as a false claim slashes. One answer per `(window, class, root, item,
executor)`; a second executor may answer the same item, and two unvoided answers that DISAGREE
on an item after their deadlines leave that item **unanswered** for that root — the fold cannot
tell which execution was false without the court, and it will not guess. Any Active bond may
answer for any root in the contest, the head included: sitting the exam is not the author's
duty but the class's, and whoever holds the class's positions has the reason to do it.

An exam answer draws no ticket, earns no reward, adds no blue work, spends no budget and moves
no share: ADR-0078 X5 verbatim ("no weight, no payment and no exposure" — except the exposure
the executor stakes on its own truthfulness), because anything a party can produce without
certification bearing on it must weigh nothing (README principle 3).

**Decision 7 — The score, the improvement, and the succession.** At the close, for each
contested class, over the `N` drawn items, counting only unvoided answers past their deadline:

* `score(r) = correct(r) / N` in permille, unanswered counting as wrong;
* for each candidate `c` against the head `h`: `wins = #{i : c right, h wrong}`,
  `losses = #{i : h right, c wrong}`, `Δ = (wins − losses) · 1000 / N` (identically
  `score(c) − score(h)`);
* `c` **passes** iff `answered(c) ≥ PALW_EXAM_MIN_ANSWERED_V1` (§4: `0.9 N`), and
  `Δ ≥ PALW_EXAM_MIN_IMPROVEMENT_PERMILLE_V1` (the magnitude the operator calls "how much
  stronger"), and `(wins − losses)² ≥ PALW_EXAM_SIGNIFICANCE_V1 · (wins + losses)` with the
  constant `9` (three standard deviations of a sign test on the discordant items, in integers;
  a candidate that beats the head by luck on a coin-flip set of items does not pass);
* the successor is the passing candidate with the greatest `score`, ties to the incumbent
  first and then to the earlier proposal in chain order; no passing candidate → the head stays,
  and the record says by how much each candidate fell short.

The head is not exempt from silence: a head no executor answers for during a whole exam phase
scores what it answered, which is nothing. This is the one checkable form of silence the doctrine
allows (README principle 2: "no object on this chain within W", with a `W` a majority would have
to sustain — here weeks, and any bond may break it), and what it charges is not a bond but an
incumbency whose defenders chose not to defend it. A class whose head is `Frozen` or `Dormant`
holds no exam (Decision 10). A class without a head root — the floor — holds none either.

On succession the fold writes `head = { root: c, author: c's bond, since_daa: close, previous:
h, grace_until_daa: close + PALW_SUCCESSION_GRACE_DAA_V1 }`, appends to `roots`, and records
`PalwExamRecordV1 { window, head_root, successor: Option<Hash64>, n, head_score, best_score,
wins, losses, candidates: Vec<(root, score, passed)> }` — the last record only; the explorer
keeps the history. From the close both roots are admissible; after the grace only the new one.
Nothing in the share table moves: it is the same class.

**Decision 8 — The index is the chain of measured improvements.** `index_permille: u32` is
`1000` at founding and gains `Δ` at every succession — a chained index, because each window's
items are new and only differences measured on the same items in the same window are
comparable, and the sum of those differences along the line of heads is the one number that
says how far the class has come since it was founded. An unmeasured head change — a
re-registration after `Dormant` (ADR-0056 Decision 5), which founds again with the
re-registrant's root — leaves the index where it was and sets `index_break_daa`, so the explorer
draws the break. The index moves at a close and nowhere else; it is a fold fact in the state
root; it is what the operator asked to have as ベンチマーク, and what the market reads.

**Decision 9 — Who is paid, and only from what was paid in.** ADR-0087 Decision 4's registrant
leg (1 % of every MSK leg) becomes two payees: `PALW_MODEL_AUTHOR_PERMILLE_OF_LEG_V1` of the leg
to the head's `author` bond's `payout_payload`, the rest to the class's `registrant_bond` as
before; when `author = None` (the founding root is in force) the whole leg is the registrant's,
so ADR-0087's arithmetic is unchanged until the first succession. A payee that has no bond (a
genesis class's registrant; an author bond that has since unbonded) is burned, as ADR-0087
already burns a genesis class's leg. The stream is the reward for strengthening: it is paid to
whoever's weights are in force, for as long as they are, by the class's own trading — king of
the hill, and the hill is public, because every head's artifact is served like the founding one
was, so the next author starts from the current best. That is the decentralised half of
みんなで強化し合う: cumulative, permissionless, and paid.

Optionally, a **bounty** the operator may arm: `PALW_MODEL_BOUNTY_PERMILLE_V1` of every gross
leg accrues to the class market's `bounty_reserve_sompi` (a new field in the market row, an
accounting entry like the reserve), and at a succession the fold spends it as an ordinary BUY on
the class's own curve — burn, registrant/author and net legs applied exactly as a buy's are —
crediting the units to the author's holder (its payout payload, the same identity ADR-0087's
positions use). The author therefore holds a position in the class it just improved, sells it
only back to the curve, and the buy lifts the price at the moment the index moves — the one
mechanical link between improvement and price that keeps ADR-0087 M2, because real MSK enters
the reserve. Default `0`: the operator names the number and where it comes from (a carve of the
5 % burn, or a seventh percent on the leg — §4 gives both).

**Decision 10 — What holds no exam.** The floor (`is_base_class`, no artifact: ADR-0068's
"artifact-less KAT class") never has a head and never an exam. A `Frozen` class (a contradiction
certificate is about the graph, not the weights) holds none, and its market is closed to buys
under ADR-0087 Decision 7 as before. A class that becomes `Dormant` drops its pending candidates
and batches at the reclamation; the returning re-registration founds again (Decision 8).

**Decision 11 — A consensus rule armed by activation, never by regenesis.**
`palw_model_succession: Option<ForkActivation>` on the params, top level, bare; refused by
`validate_palw_v2` unless `palw_model_market` is armed at or before it (an author with no leg to
be paid from is a design that has not been armed). Below the fence the four objects are refused
and the head is the founding root. The fingerprint moves only where the flag is set. **The
state root does not bump.** The head, history, index, window rows, batch rows, template pool and
bounty reserve are state-root collections that enter the root only when non-empty — the rule
ADR-0087's implementation settled on the day this was drafted: the root rides in the header, so
a chain on which the fence is `None`, or on which nothing has happened, keeps its root byte for
byte, and the carriage gains a tagged tail only after the first object. A class whose head is
its founding root and which holds no contest contributes nothing to the root.
`PALW_STATE_V2_VERSION` stays where ADR-0087's landing leaves it. The free-prompt job's root
field is the version the fence arms (Decision 1).

**Decision 12 — What a participant reads.** RPC `getPalwClassHead(class_id)` (head root, author,
since, grace, index, break, last record), `getPalwExamWindow(class_id, window)` (phase, seed,
candidates and their running answered/correct counts, deadlines in DAA),
`getPalwExamTemplates(tokenizer_id)` (ids, bounds, discordance record, TTL);
`getPalwModelMarket` gains `bounty_reserve_sompi` and the author payee. CLI: `misaka palw
succession propose|answer|status`, `misaka palw exam template register|probe`. The explorer's
Model Market page draws the index and the price on one axis — the operator's 指標 — and the
window's calendar beside them, because a contest is a dated event the market prices before its
close.

## 4. What this costs, stated before it is measured

Worked with `W = 3 weeks ≈ 30,000 DAA at one block a minute` (illustrative; the DAA count follows
the network's block rate), `N = 1,024`, `PALW_EXAM_MIN_IMPROVEMENT_PERMILLE_V1 = 50`,
`PALW_EXAM_MIN_ANSWERED_V1 = 922 (0.9 N)`, decode ≤ 16 ids, prompts ≤ 192 ids.

* **Sitting the exam.** 1,024 inferences per root. At the hybrid's ~9 s per canonical inference
  (ADR-0076 §1) with short prompts and 16 decode tokens, ≈ 2.6 machine-hours per root; head
  plus eight candidates ≈ 24 machine-hours per class per window, spread over the exam phase's
  two weeks. Exposure per executor: `items × pwu` of a short free-prompt claim, released as
  deadlines pass — one bond answering the whole exam at once needs the exposure of 1,024 open
  claims, which is why several bonds may share a root's exam.
* **Every node.** At the draw, 1,024 template runs at ≤ 65,536 VM steps each ≈ 6.7 × 10⁷ integer
  steps, under a second, once per contested class per window. Per answer object: ≤ 64 X6
  recomputes and 64 prefix comparisons. At the close: `(candidates + 1) × N` bit operations.
  The court runs only on a dispute, off the block path, as today. No node executes a model.
* **State.** Per class: head + history ≈ 8 × 80 B + record ≈ 400 B ≈ 1 KB. Per contested
  window per class: ≤ 9 roots × 16 batches × 300 B ≈ 43 KB, dropped at the close. Template pool:
  ≤ 512 × 2 KiB = 1 MiB per tokenizer id, the one standing cost; two tokenizers today.
* **Rents (operator's numbers; the sizes below are the derivation).** A candidate's rent should
  approximate what the head's defenders spend to answer: 2.6 machine-hours — the operator sets
  the MSK; the record carries `10 MSK` as the example. A template's rent should make squatting
  the pool cost more than the class's index is worth to bend: at `1 MSK` per template a full
  pool of 512 costs 512 MSK per 8 windows AND is pruned by discordance within 4; the operator
  scales it with the largest market.
* **The verdict's noise.** A candidate 5 points better than the head with 20 % discordant items
  (205): expected `wins − losses ≈ 51`, `51² = 2,601 ≥ 9 × 205 = 1,845` — passes. Two roots of
  equal strength on the same 205 discordant items: `P(wins − losses ≥ 51) ≈ P(Z ≥ 3.56) ≈
  2 × 10⁻⁴` per candidate per window — at eight candidates, one accidental succession in ≈ 600
  windows. Halving `N` doubles the variance; the operator trades exam cost against that.
* **The fee split, per gross MSK leg `m`, once a succession has happened**
  (`PALW_MODEL_AUTHOR_PERMILLE_OF_LEG_V1 = 500` as the example — half the registrant leg):

  | leg | burn | registrant | author | bounty | net |
  |---|---|---|---|---|---|
  | ADR-0087 as written | 5 % | 1 % | — | — | 94 % |
  | this ADR, bounty off | 5 % | 0.5 % | 0.5 % | 0 | 94 % |
  | bounty carved from the burn (1 %) | 4 % | 0.5 % | 0.5 % | 1 % | 94 % |
  | bounty added (1 %) | 5 % | 0.5 % | 0.5 % | 1 % | 93 % |

  The round trip stays 12 % in the first three rows and becomes 14 % in the last. Supply moves
  only by the burn (ADR-0059; ADR-0087 M2), in every row.

## 5. Security — the four principles, checked before it is built

Read against README §"Security amendments": *a free field is a free draw; silence is not a
verdict; weight is what certification buys; the chain never takes the host's word.*

| # | attack | what stops it, and the residual |
|---|---|---|
| A1 | **Craft the syllabus**: register templates the challenger answers and the head does not. | A template is drawable only from the window after its registration, so its author cannot see this window's candidates; but the author can BE a candidate who trains on its own templates. Cost: the share of the pool the crafted templates take, at rent, against a draw the author cannot steer; and every other candidate — and the head's next author — trains on the same public syllabus. Residual: a well-funded author who holds most of a tokenizer's pool can bias one window; the discordance prune and the per-tokenizer cap bound how long, not whether. Recorded, not closed. |
| A2 | **Propose junk every window** to make the head's defenders spend 2.6 machine-hours. | The rent (Decision 2) is burned and sized at that cost; a junk candidate that never answers pays it and scores nothing. |
| A3 | **Post a false answer for the head** so the head looks weak. | An answer is a claim: a false execution is convicted and slashed at the executor's exposure; a conflicting honest answer from any bond leaves the item unanswered rather than wrong. Residual: the defenders must litigate per item (checkpoint-priced, ADR-0077); the attacker risks collateral per item. The asymmetry is the court's, not this ADR's. |
| A4 | **Withhold the beacon** (a producer re-rolls the draw by withholding an attempt block). | ADR-0073 SA-1's `k ≥ 3` fold: bias `p → p³`; the seed is written by the fold from blocks every node holds (ADR-0074 Decision 2). |
| A5 | **Win, then withhold the artifact** so the class cannot produce on its new head. | The grace keeps the previous root admissible while producers fetch; a head nobody serves earns nothing (no claims → no share growth, ADR-0054 decays it) and is rolled back by the next exam by anyone who serves the old root. Residual: one window of a class producing on its previous root. |
| A6 | **Front-run your own upgrade**: the author buys positions, then proposes. | Not an attack on the rule — it is the market pricing a belief the author holds first, and the curve's 12 % round trip and slippage bound the short game. Stated as a consequence; the bounty (Decision 9) makes it the *intended* alignment. |
| A7 | **Squat the pool** with degenerate templates so nothing discriminates and no succession can pass. | Degenerate at registration is refused (the probe); degenerate in use is pruned by discordance within four contested windows; the cap is per tokenizer, so one tokenizer's squat does not touch another's; the rent is burned each TTL. Residual: four windows of a bent syllabus at the squatter's rent. |
| A8 | **Order the chain** to win a tie or a candidate slot. | Ties go to the incumbent first; slot order among proposals decides only who waits a window; a miner can delay a proposal, not forge one. |
| A9 | **Feed a non-canonical tokenisation** through a template. | Every candidate and the head receive the same ids; nothing is checked for canonicity because nothing needs to be. |
| A10 | **Move the price by rule** (burn unsold curve units, raise `V`) at a succession. | Rejected by arithmetic: with `(R+V)·U = K`, removing `u` unsold units without MSK makes the reserve after all holders sell `−V·u·(S−U)/(U·(S−u)) < 0` — insolvent; raising `V` likewise. The only price move that keeps M2 is a buy with MSK that was paid in, which is what the bounty is. |
| A11 | **A candidate whose artifact does not fit the graph** (wrong tensor shapes). | The chain never sees the artifact; its answers cannot be executed honestly, so they are convicted or never filed; it scores nothing and paid its rent. |

The one thing this ADR asks the chain to trust that it did not before is a **program written by a
stranger and run by every node** — bounded by gas, bytes, a pure VM and a probe. That is the
same shape as ADR-0075 Decision 2's grader and ADR-0078's transformers, and the VM's determinism
is drilled like a kernel before the fence is armed (§7).

## 6. Invariants the tests must hold

* **S1 (one head).** For every class and every DAA, exactly one root is in force outside a
  grace and exactly two inside one; `head.previous` is the only second root; the founding
  `artifact_root` is never rewritten.
* **S2 (the items are a function).** Two nodes with the same state derive byte-identical items
  for `(window, class)`; a template registered after the freeze changes nothing; the snapshot
  digest pins the pool.
* **S3 (no free field).** Every field of `ExamAnswerV1` is pinned — window and class by
  equality, root by the contest, items by regeneration, `answer_ids` by the X6 recompute,
  the execution by the court; a property test over the object kinds finds no unpinned byte.
* **S4 (weightless).** An exam answer changes no share, no budget, no payout, no blue work, no
  ticket; a chain with and without the answers reaches the same fork choice.
* **S5 (the verdict).** Both inequalities are necessary; ties go to the incumbent; a candidate
  below `MIN_ANSWERED` never passes; `Δ` equals `score(c) − score(h)` exactly; voided items count
  as unanswered; disagreeing unvoided answers count as unanswered.
* **S6 (ADR-0087's M1–M8 hold across a succession)**, including a bounty buy: `Σ msk_in =
  reserve + Σ payouts + burned + registrant_paid + author_paid + bounty_reserve`, always.
* **S7 (bounded work).** Per block: ≤ the object caps × 64 recomputes; per draw: ≤ `N × gas`;
  per close: `O((C+1)·N)`; the template pool never exceeds its cap; a template over gas is
  refused at registration and cannot exist in the pool.
* **S8 (replay).** A claim against a retired root replays and is adjudicated against that root
  after the grace has ended and after a later succession.
* **S9 (silence).** A contested window in which the head answers nothing records
  `head_score = 0` and passes the best candidate that clears the bars; an uncontested window
  records nothing.
* **S10 (the fence).** The fingerprint is unchanged where the flag is `None`; the state root of
  a chain with founding heads and no contest is unchanged byte for byte; the objects are
  refused below it; arming it without `palw_model_market` is refused at validation.
* **S11 (the index).** `index = 1000 + Σ Δ` over successions since founding; a re-registration
  after `Dormant` sets the break and leaves the value.

## 7. Order of work

1. `PALW_EXAM_VM_V1`: the opcode table, the PRNG, the gas meter, the probe; a differential
   drill of the VM across the fleet's targets (the same discipline ADR-0069 applies to kernels)
   before anything else, because a VM two hosts disagree on is a fork.
2. State: head/history/index, window and batch rows, the template pool; the objects; the
   grader; the close; S1–S11.
3. Params: the fence; `validate_palw_v2`'s ordering rule; the fingerprint pin for `None`.
4. Admission: "a root in force" in the attempt and free-prompt paths; the job version.
5. The fee split's second payee; the bounty reserve and its buy.
6. RPC, CLI, the explorer page.
7. Devnet drill: found a class, register templates, propose a root, sit the exam from two
   bonds, dispute one item, close, read the index; then testnet-11 by activation.

## 8. Implementation record

(none — design only)

## 9. What is deliberately not decided

* Every constant named `_V1` above: the window, `N`, the improvement floor, the answered floor,
  the caps, the rents, the author's permille, the bounty and its source. The operator sets them
  when the flag is armed; §4 carries the examples the tests will use.
* Whether template authors are paid — for instance a slice of the bounty in proportion to the
  discordance their templates produced in the winning window. Not needed for the exam to work;
  it would make A1 dearer to attempt and A7 dearer to sustain, and it needs its own audit.
* A synthetic, tokenizer-free template kind (arithmetic, string tasks) rendered through a
  chain-committed vocabulary table. It would remove the per-tokenizer pool but needs the
  vocabulary in state; left for the version after the first exam has been sat.
* An exam for a class whose weights nobody proposes to replace (a standing score). Cost with
  no verdict attached; the index answers the operator's question without it.
* Succession across graphs — a lineage of classes with one market. ADR-0087 Decision 7 stands
  for a new graph; if a class's holders are to follow a new graph, that is a market re-keying
  with its own ADR.
* Whether a head's author owes any exposure beyond the rent. A bad model is not a fault; a
  false execution is the executor's; nothing was found for an author to be slashed for.

## 10. Number hygiene

This is ADR-0088. The README's next free number was 0088 after 0087's row; it becomes 0089 with
this row. The ADR is written on `docs/adr-0088-succession-exam`, branched from
`palw-adr0084-served-answer` at `74c34476` — the tip that holds ADR-0087 — and lands beside it.
