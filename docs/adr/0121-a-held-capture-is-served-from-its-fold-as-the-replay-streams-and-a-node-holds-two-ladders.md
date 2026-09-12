# ADR-0121 — A held capture is served from its fold as the replay streams, and a node holds two ladders

* Status: PROPOSED and IMPLEMENTED 2026-09-12 on `feat/adr-0103-held-context`, at the operator's
  instruction ("node 側の held 経路（ADR-0120、数日規模）に進む" — the number moved to 0121 when the
  model-seed session claimed 0120 the same day, §8). The node half of
  [ADR-0119](0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md),
  whose §7 mapped it. **Consensus-inert:** no consensus object, rule, parameter or fingerprint
  moves; every object a node now builds by streaming is byte for byte the object the whole-capture
  path built, and every class that is not held is served, checked and named exactly as before. Rides
  testnet-11's held build (DAA 7,000) because that is the build in which a held class first exists.
* Builds on: [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (the regime; Decision 2's intervals of `P` positions), [0119](0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md)
  (a held class's ladder is the regime's `2^40`), [0086](0086-the-opening-carries-the-fold-not-the-leaves.md)
  (the V4 opening is the fold's digests; a seat names the leaf from a served block),
  [0085](0085-the-close-is-assembled-from-what-was-served.md) (the close annex and the close from
  served intervals), [0111](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md)
  (a leaf's evidence), [0082](0082-the-close-is-flat-in-the-context.md) Decision 7 (the fold).
* Amends: ADR-0086 Decision 6's block index, for a held class only (§3 D3); ADR-0082 Decision 7's
  "the retain level is the ruleset's ladder's", for a held class only (§3 D3). Closes ADR-0119 §7's
  first three items; §7 below keeps the rest.

## 0. The sentence this ADR is

**A node serves, checks, names and prosecutes a held capture from its fold as the replay streams —
the retained digests and the two edge blocks for an opening, one block for a block answer, the
step's blocks for a leaf's evidence, the disputed leaves' tiles for an annex — and never holds a
vector the size of the job or of the interval; it walks a held class at the class's ladder and lays
a capture out whole only below the network's; and a held class folds at level 12, with a block
request that counts from its interval.**

## 1. What was found

ADR-0119 moved a held class's ladder to the regime's `2^40` on the chain and left the node where it
was: every backend was built at the network's ladder
(`with_step_ladder_cap(court.max_step_leaf_count())`), so a held producer refused any job past
`2^26` leaves — about 650 positions of the dense row — and a claim in that gap was a job its seats
signed `Unavailable` for. Raising the number
the node passes was not the fix, because the survey of 2026-09-12 (ADR-0119 §7) found that number
doing two jobs, and the routes it guarded not streaming:

* **One knob, two jobs.** The backend's ladder priced and refused a job, and it was the only guard
  in front of the whole-capture paths that allocate a vector of `step_leaf_count` hashes from a
  count that arrived in a gossiped blob (the dense re-execution of a fold, `leaves_by_position`, a
  dense material's check). At `2^40` a few hundred bytes could ask for `2^46`.
* **Replays that did not stream.** Every interval replay — the executor's opening, the seat's V4
  verdict, the block answer, the naming, a leaf's evidence, the annex — sized a dense capture to the
  WHOLE step space for a window of an interval or two, and kept every tile of the window beside it:
  27 GB of leaf vector at 4,096 positions of the 1.5B dense row, a process abort at the held ladder;
  the seat's verdict alone walked 845 MB of range for one interval of the 512-position class, 13.5
  GB at `2^21` positions; the naming held six megabytes of tiles a position to compare one block.
* **A level derived where it had to be pinned.** Producer and seat each derived the fold's retain
  level from their ladder (`max(⌈log2 ladder⌉ − 20, 12)`): 20 at `2^40`, a block of 64 MiB against a
  4 MiB interval lane, and a held interval's width `P` is derived from one digest per `2^12` leaves.
* **A block index that runs out.** A block-leaves request carries sixteen bits of GLOBAL block index;
  at level 12 that reaches leaf `2^28`, which a held job of the dense row passes before its 3,000th
  position.

## 2. The requirement

A node answering or judging a held claim holds, at most, the fold's retained tree (`leaf_count / 64`
bytes at level 12), the two blocks an opening's range cuts, the blocks one step reads, and the
disputed leaves' tiles — never the job's leaves or the interval's; a count out of a blob allocates
nothing past the network's ladder; and every object it builds is the byte string the whole-capture
path built, so neither the chain nor a seat can tell which path made it.

## 3. Decisions

### D1 — Two ladders: the class's walks, the network's materializes

Each family's backend keeps the number its caller passes — the network's `max_step_leaf_count`,
unchanged at every construction site (the SDK, the lineages, the workers) — as its
**materialization cap** (`materialize_cap()`), and derives the **class's ladder** from its own
profile (`step_ladder_cap()` = `palw_class_step_ladder_v1(network, profile)`: the regime's `2^40`
for a held class, the network's for every other). The class's ladder prices a job, refuses a capture
above it and walks every Merkle path and streamed replay; the materialization cap bounds every site
that lays a capture out whole. For a class that is not held the two are one number, so nothing
changes there.

The sites, classified in each of the three families:

* **The class's ladder:** the producer's fold (`execute_free_prompt_streaming`,
  `execute_for_verdict`), pricing (`fp_context_work_leaves_v1`), the interval count, the fold's
  interval opening, resume and block answer, the held DA court's step-range answer from a fold,
  the seat's V4 verdict and naming, the close from served intervals, a leaf's evidence from a
  fold, the operand openings' adjudicator run, and the fused-site dissection's site derivation.
* **The materialization cap:** the attempt lane's dense capture (`execute`, the drill's injected
  fault), the whole-capture prover (`refutation_with_prompt`), the bisection's prefix state, a dense
  material's check (`verify_material`), a dense retention's opening, block and step-range answer,
  the dense re-execution of a fold (`dense_capture_from_fold_v1`) and the fused site's honest
  re-execution (`attn_rerun_v1`). BASE-0 retains every capture dense, so every site of that family
  that reads a capture reads this.

A capture between the two ladders is refused by the whole-capture prover **by name**
(`base0_whole_capture_refusal_v1`: "past this node's materialization cap … judged by the streamed
routes … never laid out whole"), before a leaf vector is built from its count.

The panel reads the class's ladder where it judges a claim itself: the accused's filed output tile
is walked at the ladder of the class its binding names, and the capture sampler grades at
`class_step_ladder(class_id)` (resolved from the class's profile through the same two sources as
its prompt form, ADR-0118). The conformance battery counts a lineage's canonical job at the class's
ladder, as the gate does.

### D2 — Every held route streams

* **The replay** (`d532f89a`): a family's one required kernel verb is `replay_interval_into` — every
  tile of the window handed to a sink at its canonical leaf index, in leaf order, the index a cursor
  walked by the fold's own slot walk (`base0_position_tiles_v1`, shared by the capture and the
  replay, so the two cannot enumerate differently).
* **The seat's V4 verdict** (`9b447b55`): `Base0FoldRangeCheckV1` folds each whole block as it
  completes and compares it with the served digest, keeping only the edge blocks (and the first
  differing block's leaves); the root is walked from `[left] ‖ digests ‖ [right]` by
  `base0_fold_range_root_v1`, the consensus walk's own rule.
* **The executor's opening** (`17300012`, `37c9c7ca`): the span replay streams; each completed block
  is checked against the node the executor retained for it; only the two edge blocks and the seed row
  are kept, the siblings come from them and the retained nodes (`range_siblings_from_edges_v1`), and
  the opening is walked to the committed root before it is served. A block answer keeps its block.
* **The naming** (`ddb150e1`): the seat keeps the served block's own tiles and drops the rest.
* **A leaf's evidence** (`b2af4c94`): the family verb `fp_leaf_refutation_v1`, whose default is
  ADR-0085's annex path, is overridden for a fold by `base0_fp_leaf_refutation_from_fold_v1`: only
  the blocks the step's inputs and output live in are replayed, from the interval's anchor, each
  checked against its retained digest; the openings are walked from the retained tree's edges.
* **The close annex** (this ADR's last commit): the annex reads one tile per disputed leaf and
  nothing else, so a fold replays from the interval's anchor to the last disputed leaf and keeps
  those tiles (`base0_fp_disputed_tiles_from_fold_v1`), where it kept the interval's.

### D3 — A held class folds at 12, and a block request counts from its interval

`palw_base0_sparse_retain_level_for_class_v1(profile, ladder)` is 12 for a class under the held
regime whatever its ladder, and the ladder's own level for every other class, unchanged. The
producer's fold and the dense route's V4 tree take it; a seat never derives the level — it reads it
off the opening it verified, the panel's block request included. The retained set this costs is
`leaf_count / 64` bytes: 6 MiB for a 4,096-position job of the dense row, about 3 GiB at its `2^21`
positions, beside the hundred gigabytes of cache that job holds anyway.

A block-leaves request's sixteen bits are, **for a held class**, the block's distance from the block
holding the interval's first leaf (`base0_fp_block_request_number_v1`; the executor reads it back
against the opening it serves, `base0_fp_block_of_request_number_v1`). A held interval's width is
derived so that its digests fit the lane, which is at most `2^16` blocks, so the number always fits.
The served block still names its own first leaf. Every other class keeps ADR-0086 Decision 6's
global index, so no request a shipped node sends changes meaning.

### D4 — What is refused rather than built

Past the materialization cap, the whole-capture arms refuse by name instead of attempting the
allocation: the capture sampler's prover, the bisection's prefix state, the fold's dense
re-execution and the fused site's honest re-execution. Two consequences are this ADR's to state:
a held claim past the network's ladder is judged on the interval lane (the held route) and not by
the whole-capture sampler; and its fused-site dissection evidence is not built on this node (§7).

## 4. What it costs

Time is what it was: a replay of the interval from its anchor, which every route already paid; the
annex's now stops at the last disputed leaf. Memory is §2's bound. The wire is unchanged for every
class that is not held; for a held class the one new meaning is the block request's number (D3), in
the build that first registers one.

## 5. Invariants the tests hold

* `the_fold_range_root_is_the_consensus_root` — the segmented walk equals the consensus walk over
  every leaf, on trees, levels and ranges swept; a short or long path is refused as the consensus
  walk refuses it.
* `the_edge_blocks_give_the_spans_siblings` — the siblings from the two edge blocks and the retained
  nodes are the siblings from the whole span.
* `a_tree_deeper_than_the_default_leg_serves_its_honest_openings` — a tree past the default leg's
  depth opens at its own.
* `the_executors_own_close_is_the_capture_close` — a leaf's evidence from the fold is the capture
  close byte for byte, at every main-step leaf of a multi-interval job, with the tree kept at three
  small retain levels so a step's leaves and paths cross block edges.
* `a_held_executor_serves_blocks_by_their_interval_number_and_the_disputed_tiles_alone` — on the
  held row at the regime's ladder: the fold is at 12; every whole block of every interval is served
  by its number, past the first block included, and folds to its digest; the disputed tiles are the
  interval's own and nothing outside it; the shipped row's number is the block.
* `a_held_class_folds_at_twelve_and_every_other_class_at_its_ladders_level` — at `2^22…2^40`.
* `a_held_class_walks_at_its_ladder_and_materializes_at_the_networks` — built at `2^26`, the held
  row walks at `2^40` and materializes at `2^26`, the shipped row at `2^26` for both; the refusal is
  named between the two and the old refusal outside the class's ladder.
* `the_close_from_served_intervals_holds_and_asks_for_the_disputed_leafs_block` — the close's
  gathering names the block by its class's number.

## 6. Supersession

ADR-0086 Decision 6 and ADR-0082 Decision 7 stand for every class that is not held. ADR-0119 §7's
first three items (the two knobs, the replays, the retain level) are closed here.

## 7. What is deliberately not decided yet

* **The resume opening's transport.** A resume carries the whole state at the interval's start over
  the same 4 MiB lane, and a class of `n_ctx` `2^21` always takes the Resume route; it needs its own
  transport, or chunks. The arithmetic, for the dense 1.5B row: the cache is 56 KiB a position
  (2 KV heads × 128 lanes × 4 bytes, K and V, 28 layers), so one 4 MiB answer holds the state of 73
  positions; cut into parts, the lane's per-peer serve budget (48 MiB a minute) moves about 880
  positions of state a minute from one executor. A seat resuming deep in a long job is therefore a
  question of a state-sync lane of its own and of which layers a seat holds (ADR-0099's shards),
  not of a larger cap on this one.
* **A fused site's dissection evidence past the network's ladder.** It is built from the honest
  re-execution's dense rows and the whole K/V history; a windowed builder (the rows the site reads,
  from the interval's anchor) is its own piece of work.
* **The worker frame and the gateway's prompt limit** (256 KiB, about 60,000 ids; 64 KiB of text),
  which bind only past the network's ladder.
* **A measured run at scale.** Every bound above is the code's own arithmetic and the fixtures'; a
  4,096-position held job of the 1.5B row, produced, opened, verified and named end to end on real
  hardware, has not been run.

## 8. Number hygiene

Written as 0120 and renumbered the same day: the model-seed session claimed 0120 first (ADR-0119 §8
and the index say so). The next free number is 0122.
