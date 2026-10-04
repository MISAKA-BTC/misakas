# PALW-TIR runtime residency — an IR class's weights are read within a budget, in tiers read off its program

> **Design record and implementation (lane M2, branch `tir/residency`, 2026-10-01).** ADR-0112 for
> every IR class: the generic executor (`misaka-palw-tir-exec`) gains a runtime residency, the SDK's
> IR lineage takes the node's residency policy, and `kaspad` reports and prices IR classes the way it
> reports and prices the Qwen3.6 mapping. **Node software throughout: no object, rule, fence,
> parameter or fingerprint moves.** A node holds an IR class's weights differently; it computes the
> same bytes (§6). Linked from RFC-0002 §7.

## 0. The sentence

**An IR class's weights are the same bytes on every machine that holds them; which of them are in
memory is the runtime's decision, made under a budget the operator states, never the kernel's. The
tiers are read off the class's PROGRAM — never off tensor names or model families: params every
forward reads whole are pinned, read once through the file descriptor and held; params a route
selects rows of (a mixture's experts) are held as rows under what the budget leaves, a route's rows
read together the moment its index is computed; params an input selects rows of (embeddings,
per-layer embeddings, n-gram and position tables) are read a row at a time and never held whole.
Nothing a kernel reads arrives through a page fault, because under a residency nothing is mapped.
The default budget is a fifth of the weights within what the host can spare; the floor — the pinned
set, one token's routed rows and one admission in flight — is the least it may be. A stated budget
below the floor is refused by name with its terms; a default below it leaves the class on the page
cache and says why.**

## 1. Why

ADR-0112 §1 measured the fleet: weights read through mapping faults arrive at 6–11 MB/s on the
fleet's virtio disks, against 845 MB/s for reads sized to a tensor — 12.8 GiB and three million major
faults a draw, twenty-minute draws — and even fully page-cached, owned buffers prefilled four times
faster. ADR-0112 fixed it for the hand-written Qwen3.6 runtime (`Qwen36ResidencyV1`). The generic path
did not have it: `TirArtifactV1` bound every param instance IN PLACE from a read-only mapping
(`node/mapped.rs`), `ParamData` borrowed slices of it, and RFC-0004's composites bound the parent's
params from the parent's mapping. That is fine for today's small IR classes and fatal for the
mixtures, per-layer embeddings and n-gram tables the generic frontend now expresses: Qwen4-Exp
(Level B on `tir/generic`) is 125 B params, 6 B active, plus a 51 B n-gram table — about 210 GiB
converted. Through faults one replay would read ~116 GiB at 11 MB/s, about three hours; through
reads sized to rows, about two and a half minutes (§7).

**A second, separate fix found on the way:** every `TirExecutor::new` recomputed `[min, max]` of every
param instance (`TirParams::range`, for the refined plan) — a pass over the whole artifact per job,
through faults on a mapping. The ranges are now memoised per instance in `TirParams` (and a residency
supplies them from its open pass), so a node reads its weights' ranges once per artifact, not once
per run.

## 2. The tiers, read off the dataflow (`misaka-palw-tir-exec/src/tiers.rs`)

`TirTiersV1::of(program, rules)` is a pure function of the program and the node's
`TirTierRulesV1 { pin_below_bytes }` (default 1 MiB). Per param:

* **Row-addressed** when every use a forward makes of it is a `Gather { axis: 0, batch_dims: 0 }` of
  the param itself, or of an uncommitted, uncarried `Reshape` chain of it (`tir_row_sites_v1`). Such a
  gather reads whole rows of the gathered view `[rows, …]`, and a row of a row-major view of a
  contiguous tensor is a contiguous run of the param's elements — `unit` elements at `row × unit`, one
  positional read. This is how the lowerer writes a mixture's expert stacks (`[E, out, in]`, gathered
  by the route) and their per-(expert, row) scales (a flat `[E·rows]` reshaped to `[E, rows]` and
  gathered by the same route).
* **Routed** when some gather's index is computed from the weights — tainted by a param through the
  dataflow (`weight_taint_v1`: a fixpoint over nodes, the carries between occurrences, and states; a
  param taints, the token, the position, a const and an `Iota` do not). A `TopK` over router logits is
  the case.
* **Gathered** when every index is a function of the inputs alone — the token, the position, a
  history of tokens (an n-gram id kept in a `Hist` state of tokens is not tainted).
* **Pinned** otherwise, with the reason (`TirPinnedWhyV1`): a dense use (a `MatMul` or elementwise
  operand, an index, a committed, carried or logits view, any view but a row-major reshape); an
  instance under `pin_below_bytes` (an activation table of 65,536 `i16` codes, a per-token scale
  vector: rows not worth a read each, and a budget cannot notice them); a forward that reads at least
  as many rows as there are; views of different row shapes.

What the corpus says (`misaka-palw-tir-lower/tests/residency_tiers.rs` over every Hugging Face
fixture's lowered program, `tests/residency.rs`, `misaka-palw-sdk/tests/tir_residency.rs`): every
mixture routes its expert stacks and the scales gathered with them, and no dense model routes a stack;
every lowered model gathers its token embedding; Gemma-4's per-layer input table is gathered by the
token at every layer; the two-level RoPE tables are gathered by the position's
high and low bits; dense decoders and the recurrences route nothing; a tied unembedding makes the
embedding dense.

**The tiers move bytes, never values.** A served row is exactly the mapping's elements, and the
executor answers any read of a row-served instance that the tiers did not plan (a sink that asks
for every node's value) with the whole instance, read once and counted (`TirRowCountsV1::whole_reads`)
— so no committed byte depends on the classification being right. The tests assert the count stays
zero on every path the typed executor and the court take. (RFC-0004's evaluation executor runs the
reference interpreter, whose `ParamSource` asks for whole tensors: under a residency those are whole
reads, counted, of the instances the store serves by rows — §8.)

## 3. The budget and the floor

`TirResidencyArithmeticV1` (from the tiers alone, so a preflight prints it before a byte is
converted):

| term | definition |
| --- | --- |
| weights | every instance's bytes |
| pinned | every pinned instance |
| routed / token | Σ over routed instances of `min(rows, rows a forward reads) × row bytes` |
| in flight | the largest single admission — one route group's rows at one occurrence |
| **floor** | **pinned + routed / token + in flight** |
| fifth | ⌈weights / 5⌉ |
| routed capacity at budget `B` | `B − pinned − in flight` |

The policy is ADR-0112's (`TirResidencyPolicyV1`, mapped one-to-one from the SDK's
`PalwWeightResidencyV1`): `PageCache` (`--palw-class-resident-bytes 0`: mapped, as before, kept for
comparison), `Bytes(b)` (stated), `FifthOfTheWeights` (the default where the host cannot be read),
`FifthWithin(spare)` (the default where it can). **A stated budget below the floor is refused** with
the floor and its three terms in the message; **a default below the floor declines** to the page cache
with `TirResidencyDeclinedV1 { budget, floor, fifth, weights, routed }` — nobody asked for a number
that cannot run. A class that routes nothing has a floor near its size, so its fifth is always under
it and it stays on the page cache by default, exactly as before (`kaspad` says so in an info line); a
class that routes and is declined is the case ADR-0112 was written for, and `kaspad` warns. At the floor the routed cache holds exactly one token's routed rows, so a
forward never re-reads what it read earlier in the same forward; the in-flight term keeps a row a
gather is copying alive past an eviction inside the budget.

## 4. The store (`misaka-palw-tir-exec/src/node/residency.rs`)

* **Opening is one pass.** `TirWeightStoreV1::open` reads the pinned set in parallel (8 MiB reads at
  the device's queue depth) and then every byte of the file once, in inventory order, in chunks of
  whole leaves (32 MiB; the next chunk read while the last is hashed, a chunk's leaves hashed in
  parallel): the inventory root through the consensus leaf and frontier (equal to
  `palw_tir_inventory_root_v1`, tested), and each instance's `[min, max]`. `bytes_read` after open
  equals the weights' bytes exactly.
* **Routed rows** are an LRU keyed by `(param, layer, row)`, stamp-ordered (`O(log n)` a touch), under
  the routed capacity; an admission reads every missing row of a route group in parallel, off the lock.
  A row not held when its gather runs — evicted by another duty's forward — is read then ("late") and
  counted as a miss.
* **Gathered rows** are positional reads at the gather, never held.
* **Openings and trees** read pieces and chunks through the descriptor (`TirByteSourceV1`;
  `TirInventoryTreeV1::{open_with, operands_with, multiproof_with, from_leaves}`) — a court close never
  materialises an instance to open a 32 KiB leaf.
* **Nothing is mapped** under a residency (`TirArtifactV1::is_mapped() == false`): ADR-0112 I-4 holds
  by construction.
* **Composites** (RFC-0004): one store per parent root in a process (`tir_weight_store_for_root_v1`),
  shared by every candidate of that parent — `TirArtifactV1::open_composite_over(parent, …)` takes the
  held parent's store, `open_composite` finds it by root. A candidate's adapter is pinned; a parent
  param the candidate's own program reads whole (or in another row shape) that the store serves by
  rows is pinned for that candidate alone (`own_pinned_bytes`), so no candidate read ever falls back.

## 5. The executor and the node

* **Executor** (`params.rs`, `rows.rs`, `exec.rs`): `TirParams::serve_rows` installs a `TirRowSourceV1`
  for the unbound instances; the `Gather` arm takes the row path when its data is a row-served
  param's view at the site `TirPlan::rows` recorded (`tir_row_sites_v1`, structural, so a plan holds it
  for every artifact); the first gather of a route group admits the whole group (ADR-0112 Decision 4).
  Indices are checked exactly as the whole-instance kernel checks them, before anything is read. A
  fused region that would read a row-served param whole runs on the generic kernels.
* **SDK** (`lineages/tir.rs`): `load` passes the node's policy; the summary line names the budget,
  its terms and how many tokens of routed rows it holds (or why the page cache decides);
  `tir_residency_stats_of`, `tir_residency_owner_stats_of`, `tir_residency_spent_of`.
* **kaspad** (`palw_backends.rs`): a default measured once is spent across IR holdings too; the load
  line prints an IR class's arithmetic (a declined default: a warning for a class that routes, an info
line for one that routes nothing); a resident IR holding
  prices its replays at its budget and takes nothing more from `MemAvailable` (`holding_replay_bytes_v1`,
  `incremental_replay_bytes_v1`); the draw's storage line and a new per-replay line (panel seats) print
  each held residency's bytes read, misses of lookups, gathered rows, evictions and holdings.
  `--palw-class-resident-bytes` and `palw_class_residency_v1` apply to IR classes unchanged.

## 6. Invariants and where they are held

| | invariant | tests |
| --- | --- | --- |
| I-1 | identity: a budgeted store computes the mapped one's committed rows, outputs, inventory root, leaves, openings, readiness material and court closes — at the floor, at a fifth, holding everything; and the reference evaluator's values on 6,000 random programs, the corpus and the goldens with every row-addressed param served | `residency_node.rs::a_budgeted_artifact_computes_what_the_mapped_one_does_at_the_floor_at_a_fifth_and_whole`, `residency.rs::the_row_path_computes_the_reference_on_*`, SDK `tir_residency.rs`, `improve_composite_node.rs` |
| I-2 | the routed rows held never pass the capacity between admissions; at the floor, one token's | `…evicts_reads_again_and_never_holds_more_than_it_leaves` |
| I-3 | a stated budget below the floor is refused with the floor and its terms; the floor opens | `a_stated_budget_below_the_floor_is_refused_by_name` |
| I-4 | nothing mapped under a residency; no served instance read whole on a node path | `is_mapped()`, `whole_reads == 0` in every identity test |
| I-5 | the policy's arithmetic: a fifth rounds up, `0` is the page cache | `the_residency_policy_arithmetic` |
| I-6 | the seam: the lineage takes the policy; composites share one store per parent root | `a_composite_candidate_shares_its_parents_store_*`, SDK composite test |
| I-7 | a default takes a fifth within what is spare and declines below the floor, never refused | `a_default_budget_is_a_fifth_within_what_is_spare_*` |

## 7. Numbers

**Measured (debug build, the 64-expert test mixture: 4 layers, 2 of 64 experts a token, 448 bytes an
expert across its four stacks; a 15-position job):**

| budget | routed capacity | lookups | hits | misses | evictions | routed bytes read |
| --- | --- | --- | --- | --- | --- | --- |
| the floor (14,976 B) | 3,584 B = one token | 480 | 32 | 448 | 416 | 50,176 |
| a fifth (25,191 B) | 13,799 B ≈ 3.9 tokens | 480 | 116 | 364 | 241 | 40,768 |
| everything | all experts | second run: all hits | | 0 new | 0 | 0 new |

Every run's capture, roots and leaves equal the mapped artifact's; `bytes_read − open = routed bytes
read + gathered bytes` on every run (every read is counted). The fleet measurement's tool is
`tir-exec-bench --container <path> --resident-bytes <n>` (the mapping's run without the flag); this
lane runs nothing on a fleet host.

**Real checkpoints, from their configs alone** (`misaka-palw-tir-lower/tests/residency_tiers.rs`:
each program lowered at its real shapes, no weight read; the node's rule; GiB; "replay" is a
4,097-forward canonical job — a 4,096-position context — at the cold expected union of its routes
plus its gathered rows, ESTIMATED at 845 MB/s):

| config | weights | pinned | routed (a token) | floor | a fifth | replay |
| --- | --- | --- | --- | --- | --- | --- |
| qwen3-30b-a3b | 28.96 | 1.28 | 27.09 (1.69) | 3.01 | 5.79, holds it | 27.1 GiB, 34 s |
| qwen3-next-80b-a3b | 75.20 | 2.01 | 72.61 (1.42) | 3.45 | 15.04, holds it | 72.6 GiB, 92 s |
| gpt-oss-20b | 20.15 | 1.28 | 17.80 (2.23) | 3.59 | 4.03, holds it | 17.8 GiB, 23 s |
| deepseek-v2-lite | 14.93 | 1.10 | 13.43 (1.26) | 2.41 | 2.99, holds it | 13.4 GiB, 17 s |
| qwen1.5-moe-a2.7b | 13.72 | 1.54 | 11.60 (0.77) | 2.35 | 2.74, holds it | 11.6 GiB, 15 s |
| olmoe-1b-7b | 6.59 | 0.38 | 6.02 (0.75) | 1.18 | 1.32, holds it | 6.0 GiB, 8 s |
| mixtral-8x7b | 43.72 | 1.48 | 42.00 (10.50) | 12.30 | 8.74, SHORT | 42.0 GiB, 53 s |
| granite-3.1-3b-a800m | 3.19 | 0.37 | 2.81 (0.56) | 0.95 | 0.64, SHORT | 2.8 GiB, 4 s |
| qwen2.5-7b (dense) | 7.72 | 6.70 | 0 | 6.70 | 1.54, SHORT | 0.03 GiB |

The ratio is a property of the mixture (ADR-0112 §8): routing few of many experts (8 of 128, 10 of
512) holds the floor inside a fifth; routing two of eight (Mixtral) or eight of forty (Granite) does
not, and those stay on the page cache by default unless a budget at least the floor is stated; a
dense model's floor is its size. At a long canonical job a replay touches nearly every expert once,
so it reads about the routed stacks: 34 s for Qwen3-30B-A3B at 845 MB/s, against about 44 minutes
at the fault rate (11 MB/s). Qwen4-Exp (Level B on `tir/generic`, ~210 GiB converted) is the same
arithmetic at its size: ~116 GiB a replay is ~2.5 min at 845 MB/s, ~4.1 min at 500 MB/s, against ~3 h
through faults. The registration preflight (`tir/residency-preflight`) prints these terms for any
model from its headers.

## 8. Candidates of one parent, weight-stationary (`misaka-palw-tir-exec/src/lockstep.rs`)

**The API.** `TirLockstepV1` steps several executors — a parent's composite candidates, over the one
store they share — a position at a time and an OCCURRENCE at a time: every member's layer `L`
before any member's layer `L + 1`. A layer's parent weights then serve the whole batch while they
are at hand: its pinned weights pass through the CPU's caches once instead of once per candidate, and
its routed rows are admitted once — the first member's admission reads them, the rest find them
held. Activations, states and adapters stay each member's own. The executor's step is cut at
occurrence boundaries (`begin_step`, `run_occurrences`, `end_step`: `step_opt` is exactly the three in
sequence), so each member computes what it computes stepped alone; `tir_lockstep_batch_v1` bounds a
batch by the routed capacity over one admission (every member's admission of a layer held at once)
and by each member's working memory within what the host spares.

**Measured** (debug, `residency_node.rs`): three candidates of the 64-expert test mixture over the
parent's store at its floor, ten positions on the same tokens — every member's commits and logits
equal to the member run alone; **102,144 bytes of routed rows read one candidate after another,
34,048 in lockstep**: a third, the batch size (one after another re-reads every token's experts once
per candidate; the batch reads them once). The candidates share the store's rows because a
candidate's tiers are read under its parent store's rules; each pins only its adapter and the
embedding its tied head reads whole.

**Why the RFC-0004 duty does not call it yet** (measured against the code, not guessed):

* the evaluation executor runs the **reference interpreter** (`palw_eval_run_v1` →
  `palw_gen_execute_v1` → `InterpreterV2`, params through `ParamSource` as whole `i128` tensors), whose
  cost is the interpreter's arithmetic, not the weights' reads — batching it would not move it;
* evaluation duties do not co-occur **in time**: the loop runs one job at a time
  (`PALW_IMPROVE_MAX_RUNNING_V1 = 1`); they do co-occur in the **plan** — `palw_improve_duties_v1`
  orders an epoch's tasks item first, subject second, so one item's jobs over every held candidate of
  a parent are adjacent and share their inputs. When the evaluation executor moves to the typed
  executor (byte-identical to the reference, held by the differential gates), those adjacent tasks
  are a lockstep batch as they stand: same parent store, same prompt, one step per position.

So the executor API and its identity tests ship; the duty keeps its executor.

## 9. Not done here

* **Reading and computing a layer overlapped** (ADR-0112 §8's last bullet): an admission reads a
  group, then the gathers compute it.
* **Promotion**: a row hot enough to be worth pinning is still an LRU entry.
* **The evaluation executor on the typed backend**, which would make RFC-0004's adjacent same-item
  tasks a lockstep batch (§8).
