# ADR-0117 — A draw is one forward

* Status: PROPOSED and IMPLEMENTED 2026-09-11 on `feat/adr-0103-held-context`, at the operator's
  instruction. The same day ADR-0112 §8 had recorded the one-forward ticket as not taken; the
  operator reversed that and asked for the change ("1 回の forward で抽選する … 実装を行う方針に変更する").
  Decision 2 (the engine) is consensus-neutral and in force wherever this build runs. Decision 1
  (the job) is behind `Params::palw_prefill_draw`, `None` on every shipped preset, so no fingerprint
  moves until a network arms it.
* **Scheduled on testnet-11 at DAA 4,000** (operator, 2026-09-11), on the pre-mainnet audit's flag day
  (`PALW_RC_AUDIT_FENCE_DAA`), so one rollout carries both; t11's fingerprint moves `09efd285…` →
  `4300409b…`. The shared height has one consequence the fork-id gate cannot see: a build carrying
  the audit fence without this one advertises the same fork id and parts silently at 4,000 (the
  constant's doc). Held context (ADR-0103) was asked for the same height and given its own flag day
  instead — its prerequisites on testnet-11 (signature contexts, Merkle prompt ids, the shard court)
  are not armable yet.
* Builds on: [0072](0072-the-ticket-is-the-execution.md) (the ticket is a function of the
  execution), [0084](0084-the-ids-ride-the-capture-stays-home.md) (Decision 7: the seat's verdict by
  execution), [0112](0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md)
  (the weights are read within a budget, and a draw's cost is what it reads).
* Reverses: ADR-0112 §8's "Decided 2026-09-11: not taken."

## 0. The sentence this ADR is

**A draw is decided by one pass over its prompt. The prompt is the one the anchor has always
derived, at the class's canonical prefill length, and the engine computes all of it one layer at a
time, so every weight is read once per draw. Past `Params::palw_prefill_draw` the draw's job has no
decode call: its one generated token is chosen from the last prefill position's logits, and the
ticket — a function of an attempt whose trace root commits that row and that token — is decided by
that pass. The class, its id, its certification, its price and the free-prompt lane are unchanged.
A seat holding a producer's material checks the whole job the block asked for, not only its id.**

## 1. What was known, and what was found writing it

* **The job.** Qwen3.6's canonical job is `(7, 2)`: seven prompt positions and two generated tokens.
  The first token comes from the last prefill position, so the committed path runs 7 + 1 = 8 forward
  passes (the step space's `P + D − 1`; only the old path with no compiled plan ran 9, and ADR-0112 §1
  and §8 said nine). Each forward was one position through every layer, so a draw read the weights
  eight times.
* **The order is not the protocol.** A layer's walk reads and writes only that layer's cache, and a
  position's walk through layer `l` reads only its own row out of layer `l − 1` and the positions
  before it in layer `l`. So the prompt's positions can be computed a layer at a time with every
  committed row bit-identical, and the weights of a layer — its tensors and the union of the experts
  its positions route to — read once for the whole prompt. That is the operator's "1 回の forward",
  and it needs no rule.
* **The decode call is a rule.** A generated token depends on the one before it, so the second token
  is a second pass over the weights whatever the engine does. Removing it changes the job.
* **The job is not in the class id.** `class_id = shape_profile_id()` hashes the graph profile alone
  (the virtual processor's own comment: "The canonical job is NOT covered by the class id"), so
  changing the draw's job needs no re-mint. ADR-0112 §8 said otherwise; it was wrong.
* **The job is enforced by seats, not at acceptance.** Nothing in block acceptance checks that the
  execution was the job the anchor names. Seats check it: a seat with no material replays the job and
  compares roots (ADR-0084 Decision 7); a seat holding the material verified it against the claim's
  roots and the anchor — and **only the job's id**. A producer could therefore answer the right
  question with a smaller job, skipping its decode calls or running a prompt of another length under
  the same id, and every seat that held its material vouched for it; only the replaying seats
  refused. That gap was found writing Decision 3 and is closed by it.
* **One token is not a prompt.** A one-token prompt maps the anchor to one of the vocabulary's
  248,320 ids, and the model's output for one token at position 0 is a fixed function of that id.
  Every draw would be a lookup into a table computed once, and the lottery would price nothing. The
  prompt stays at the class's canonical prefill: seven ids for Qwen3.6, 248,320^7 prompts.

## 2. The requirement

A draw costs one pass over the class's weights. The ticket stays a function of an execution a court
can adjudicate, position by position. Producers and seats agree on the job at every height, and a
seat that holds a material cannot vouch for a job its block did not ask for.

## 3. Decisions

**Decision 1 — past the fence, the attempt's job is the canonical job without its decode calls.**
`palw_attempt_v2::palw_attempt_job_v1(canonical, prefill_draw)` sets `exact_decode_tokens = 1` when
`Params::palw_prefill_draw_active_at` holds at the attempt's own block's DAA score, and changes
nothing else: the anchor, the prompt, the prefill and every identity field are the canonical job's.
The producer that runs the job (`produce_one`) and every seat path that replays or judges it (the
panel's `attempt_job_for_claim`: the challenger's re-execution, the court challenger's, and the
verdict by execution) call that one function at that one height. It is a fence because seats are
what enforce the job: a seat on the old rule replays the two-token job, finds a root it did not
compute, and voids an honest claim, so the network must switch at one height. A bare fence, `None`
on every preset, Some-only in the fingerprint, the schedule id and the fork id. Arming it is a flag
day like any other.

What does not move:

* **the class** — its id, its canonical job, its certification drill and its registration. A
  prefill-only job is a prefix of the canonical job's step space, which every court already
  adjudicates, and the drill that certified the class covered it;
* **the free-prompt lane** — an answer is decode tokens by definition, and it never calls the
  function;
* **the ticket's derivation** — `class_ticket_v3` over `execution_commitment_v3`. It is still the
  execution: the trace root commits the last prefill position's logits row and the token chosen from
  it, and the execution root commits every node's output at every prompt position.

**Decision 2 — the prefill is one pass over the weights.** `Qwen36Engine::forward_prefill_planned`
computes every prompt position through a layer before the next layer is read. The post table runs at
the last position only, the one whose rows the step space commits. The capture loop takes it whenever
the class wants no checkpoint inside the prefill (the shipped hybrid never does). A class whose
cadence wants one needs the cache as it stands after each position, which a layer-major pass never
holds, so it keeps the stepped pass. The rows, the last logits and the cache left behind are the
stepped pass's bits, so this is in force everywhere, fence or not: before the fence it takes Qwen3.6's
draw from eight passes over the weights to two (the prefill, then the decode call), and past it to
one. Under ADR-0112's budget the same order admits a layer's routed union while that layer runs. With
seven positions against forty layers, that union — at most 56 experts — sits inside even the floor's
320, so each expert a draw needs is read once.

**Decision 3 — a held material answers the whole job its block asked for.** `PalwClaimRootsV1` carries
`attempt_draw: Option<bool>`. It is `Some(prefill_draw)` for an attempt claim, read at the claim's
block's height, and `None` for a free-prompt claim, whose anchor is its job's own id, or for a caller
with no block. With `Some`, every family's `verify_material` (the floor, the dense tier, the hybrid)
requires the material's job context to equal `palw_attempt_job_v1(job_for_anchor(anchor),
prefill_draw)`, field for field, and says `Mismatch` otherwise. Before the fence that is the canonical
job, and the skipped-decode job the id check let through is refused. Past it, the canonical job is
the one refused. The legacy composite paths recompute under the same job.

**Decision 4 — the price stays the canonical job's.** `pwu_per_inference` is the class's canonical
step-leaf count, and item 6 of admission still derives an attempt's pwu from it. A prefill-only draw
executes the canonical job's prefill, so past the fence one pwu buys a fixed fraction of the leaves
it bought before — per class, the prefill share of the canonical job. Making the price the draw's own
leaf count needs each class's profile at admission. The genesis classes registered with no carriage
have only the catalog's canonical count on chain, and adding a per-class draw count to the ruleset
bundle would move every V2 network's fingerprint while the fence is dormant. Every pwu of one class
past the fence is the same fraction, and a faster engine already moves pwu per unit of compute the
same way (Decision 2 does, with no fence). So the price is unchanged, and the per-class fractions are
pinned by a test (§5 I-6) so they are a stated number rather than a surprise:

| shipped class | canonical job | leaves a one-forward draw executes | of the price |
|---|---|---|---|
| the floor | (8, 4) | 5,560 of 7,708 | 72.1 % |
| PALW-QWEN36 graph-v3 | (7, 2) | 2,326,264 of 2,685,360 | 86.6 % |
| the dense graph-v5@512 | (63, 2) | 6,508,520 of 6,630,544 | 98.1 % |

Today one pwu buys one leaf on every class. Past the fence it buys 0.72 of a leaf on the floor,
0.87 on the hybrid and 0.98 on the dense row. Measured against the dense row, the floor's weight per
leaf executed therefore rises by about 36 % and the hybrid's by about 13 %. That is the relative
weight this decision leaves standing. Re-pricing belongs to the next re-mint, which can carry the draw's count in the catalog.

## 4. What it costs, and what it buys

| Qwen3.6, per draw | before | Decision 2 only | Decisions 1 and 2 |
|---|---|---|---|
| passes over the weights | 8 | 2 | **1** |
| the always-set read (1.86 GiB), on a host with no residency | up to 8× | 2× | **1×** |
| routed experts read, under ADR-0112's budget | each token's, less the LRU's hits | the prompt's union once, plus the decode token's | **the prompt's union, once** |
| compute | 8 forwards | 8 forwards, fewer post tables | **7 positions, one post table** |

The post table is the unembedding, a 248,320 × 2,048 projection. The stepped prefill computed it at
every prompt position and discarded all but the last, and the one-pass prefill computes it once.

What it does not buy: on a host that cannot spare the floor (ADR-0112 §10.5), weights are still read
through the page cache's faults at 11 MB/s. The draw reads about 1.86 GiB of always-set plus the
prompt's experts once, instead of eight times, and it is still minutes. On such a host the residency
budget is the fix, and §8 of ADR-0112 names the node's own working set as what stands in its way.

## 5. Invariants the tests hold

1. **I-1, the one-pass prefill is the stepped one.** On the v2 and fused v5 graphs, from a fresh cache
   and from a prefix, every committed row, the last logits and the cache left behind are the same
   bits (`the_one_pass_prefill_is_the_position_by_position_one`).
2. **I-2, under a budget it is the owned store's rows and reads no more.** At the floor, the one-pass
   rows equal the owned store's, and its misses and bytes read are at most the stepped pass's
   (`the_one_pass_prefill_under_a_budget_is_the_owned_rows_and_reads_no_more`).
3. **I-3, the one-forward job is the canonical job without its decode calls.** It is idempotent, a
   different execution, and identical before the fence
   (`the_one_forward_job_is_the_canonical_job_without_its_decode_calls`).
4. **I-4, the fence is dormant everywhere**, named when armed, exact at its height, and absent
   outside `ConsensusV2` (`the_prefill_draw_fence_is_dormant_and_named_when_armed`).
5. **I-5, a held material answers the whole job.** Before the fence the canonical material matches
   and a skipped-decode material, or a longer prompt under the same id, is `Mismatch`; past it, the
   other way round; with no block the id check stands
   (`a_held_material_answers_the_whole_job_its_block_asked_for`; on the hybrid,
   `the_one_forward_draw_is_the_canonical_prompt_in_one_pass`, where the one-forward run's single
   token is the canonical run's first).
6. **I-6, the price's fraction is pinned per shipped class**
   (`the_one_forward_draw_is_priced_as_the_canonical_job_and_the_fraction_is_pinned`).

## 6. Supersession

| what | by |
|---|---|
| ADR-0112 §8: "Decided 2026-09-11: not taken" (the single-forward ticket) | Decisions 1 and 2 |
| ADR-0112 §8's claim that the job is in the class id, and §1's "nine forward passes" | §1 above: eight, and not in the id |
| every family's `verify_material` checking only the material's job id | Decision 3 |

## 7. What is deliberately not decided

* **The arming height.** A flag day, announced like DAA 3,500's, is the operator's call.
  testnet-11's schedule is unchanged by this ADR.
* **Re-pricing.** Decision 4's fraction stands until a re-mint can carry the draw's own count.
* ~~**The dense tier's one-pass prefill.**~~ **Built 2026-09-12** (§9.1): the dense engine walks
  the registered plan a layer at a time over runs of positions, and the capture loop and the
  interval replays take it.

## 8. Number hygiene

0117, beside 0116 (the same instruction's other half). **The next free number is 0118.**

## 9. Implementation record (2026-09-11)

| Decision | where | what pins it |
|---|---|---|
| **1** the job | `Params::palw_prefill_draw` (field, fence accessors, schedule id, fingerprint, fork-id probe); `palw_attempt_v2::palw_attempt_job_v1`; `palw_producer::produce_one`; `palw_panel::attempt_job_for_claim` and its three callers | I-3, I-4 |
| **2** one pass | `Qwen36Engine::forward_prefill_planned`; `qwen36_execute_streaming_v1` | I-1, I-2 |
| **3** the whole job | `PalwClaimRootsV1::attempt_draw`; `verify_material` in `backend.rs`, `qwen25_a16_backend.rs`, `qwen36_backend.rs`; `palw_panel::attempt_draw_for_claim` | I-5 |
| **4** the price | — (unchanged) | I-6 |

The suites on `feat/adr-0103-held-context` after the change: consensus core 2,123 passed (library
and integration), `misaka-palw-base0` 432, the SDK 25, `kaspad`'s library 87; `misaka-cli` and
`misaka-palw-job-replay` compile against the new `PalwClaimRootsV1`.

### 9.1 The dense tier's one pass (2026-09-12, at the operator's instruction)

`A16Engine::forward_prefill_planned` walks the registered plan a layer at a time over a run of
positions. Each projection runs once over the run — `kernels::a16_matmul_requant_batch`, the
weight row read once for every position, which the kernel tests hold bit-identical to the
single-row projection — with the sink position alone on its own parameters (ADR-0050). The fused
attention site reads one concatenation of the layer's history and hands each position its prefix,
where the stepped walk copied the whole history for every position. Every other node is the
stepped walk's own evaluation (`eval_node`, extracted from `walk_table` so the two walks are one
computation), on the pool one position a task. A cache write appends the run's rows in position
order and a read sees the series that ends at its own row; a plan whose layer reads the cache before
writing it keeps the stepped walk (`one_pass_prefill_supported`).

It runs where a prefill runs: the dense capture loop — the producer's draw (Decision 1's job) and
its free-prompt capture, in runs of `A16_PREFILL_RUN_POSITIONS` = 64, a checkpoint after a prefill
position taken after its run from the rows it reads by index — and, through
`Base0FpReplayForwardV1`, the interval replays a seat and an executor run (a held interval of
prompt positions, a shipped class's interval 0). The legacy v1 class, which commits every prompt
position's logits in its composite root, keeps the stepped walk.

| | stepped | runs of 32 | runs of 64 |
|---|---|---|---|
| 256 positions | 8.64 s (33.8 ms a position) | 3.94 s (15.4 ms), 2.20× | — |
| 508 positions (the 512 row's canonical prefill) | 18.30 s (36.0 ms) | 8.28 s (16.3 ms), 2.24× | 7.47 s (14.7 ms), 2.45× |

The real 1.5B artifact, testnet-11's graph-v5 row, one 12-core M-series host, the prompt the
env-gated `one_pass_prefill_on_the_real_dense_row` probe builds; every committed row, the cache and
the last position's logits the same bits in every run. Pinned on the fixture by
`the_one_pass_prefill_is_the_position_by_position_one` (the v2, v5 and v7 plans, the fast and
catalog engines, from the sink and from a stepped prefix, runs of every width), and end to end by
the context vectors, whose documents did not move. The 32,768-position vector's stages, the same
host, before and after (with ADR-0121's streamed replays beside it): produce 32.6 → 7.7 s, seat
91.6 → 34.7 s, court 180.7 → 44.2 s, availability 11.5 → 2.1 s — 318.6 s to 91.4 s, the document
`0b8b191d…` both times.
