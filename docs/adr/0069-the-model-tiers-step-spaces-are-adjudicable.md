# ADR-0069: The model tiers' step spaces are adjudicable — end to end, and proven by sweeping them

Status: ACCEPTED (implementation on `palw-step-space-e2e`; arming is a deployment decision, §7)
Date: 2026-09-01
Relates: ADR-0030 (the step space), ADR-0038 A4 (coverage), ADR-0049 (canonical IR, Decisions B/C/F/G),
ADR-0052 (the hybrid's integer arithmetic), ADR-0053 (one execution family), ADR-0067 (classes are
chain data), ADR-0068 (the LLM-primary economy)

## 1. The problem, stated the way the chain would have met it

ADR-0068 hands the two model tiers ≈98 % of blocks and issuance. On the day that table armed,
neither tier's claims could be prosecuted:

* **Neither model backend emitted a step space.** `execute` committed a bespoke composite
  (`keyed(context, shape_id, trace_root, output_root)`) in the `execution_root` slot — the qwen36
  module said it in so many words: *"here there is no step leg yet"*. `check_execution_root_binding`
  compares a close against `binding.committed_execution_root`, so every close on a model-tier claim
  died at `ExecutionRootMismatch` before any arithmetic was read. Both backends answered
  `supports_court() == false`; a dispute could not leave round 0 whichever party was honest.
* **The retained material discarded the evidence.** Logits rows and ids only — no binding, no
  tiles, no state chunks. A producer could not have answered a rung about its own execution.
* **The court's front door refused the tiled scheme.** `check_job_context_shape` admitted only the
  float event-tree scheme id, so every model-tier binding was `SchemeMismatch` before its first leaf.
* **A dozen arm/registration mismatches**, each structurally sufficient to make honest material
  `Unadjudicable` (the I10 freeze arm — a frozen class as the reward for being audited). §4 lists
  them; every one was found by the sweep, not by reading.

The registered v1 descriptors cannot close these gaps: the v1 A16 profile declares a one-byte state
map over an `i32` cache and omits a narrowing its engine performs; the v1 hybrid table carries a
phantom node no execution fills. Their ids are frozen chain facts. The corrected classes
(`qwen25_a16_profile_v2`, the qwen36 `graph-v3` row) are where court capability lands — which is
what ADR-0067 built them for.

## 2. Decision — the acceptance test IS the property

**A model tier is adjudicable when every leaf of a captured attempt adjudicates.** The landed
acceptance tests state it as code:

* `every_a16_leaf_adjudicates_and_a_tampered_one_convicts`: one captured A16 attempt; for EVERY
  leaf of its step space the backend's own prover assembles the refutation, the backend's own
  inventory answers the operands through real Merkle openings against its root, and the court
  finds no fault; then one tampered lane convicts at representative kernels — a decode call (the
  tiled pin) and a checkpoint-anchored attention step (the v2-map geometry) included.
* `every_qwen36_leaf_adjudicates_and_a_tampered_one_convicts` — the hybrid twin, over the RC's
  canonical job shape: ~1,900 leaves, every one `NoFaultFound`, five tampered leaves convicting
  (embed, a GDN recurrence head, a routed-expert tile, a decode call, the last leaf); plus
  `every_qwen3moe_leaf_adjudicates_and_a_tampered_routed_tile_convicts` for the all-attention
  flavor. Between them every arm of both layer kinds is exercised — recurrence, convolution,
  router, routed experts, combine, shared expert, gated attention.
* `a_real_bisection_converges_on_the_tampered_leaf_and_the_close_convicts`: the composition
  nothing had — a real ladder driven by the two parties' own materials converges on exactly the
  injected-fault leaf, the close passes the cost gate, proves operands against the class root,
  and convicts; the honest direction clears through the identical machinery.

Sampling was rejected deliberately: every defect in §4 was invisible at tile 0, leaf 0, or the
first position, which is where a sampled test looks.

## 3. The commitments a model-tier claim now carries

`execute` for a court-capable class runs the captured walk (the A16 engine's traced/planned
forward; the hybrid's planned interpreter — one committed row per declared node), tiles every row
at the node's own `tile_len` into the canonical leaf space, and commits:

* `execution_root = binding.committed_execution_root` — the step binding's own composition
  (logits leg ‖ activation leg ‖ checkpoint leg ‖ step leg), exactly what the court pins.
* `trace_root` = the TILED logits root over the selecting rows (one per generated token).
* material = the family codec (`binding, tiles, logits rows, ids, checkpoint chunks`) — the one
  encoding the floor already used, so a seat rebuilds the legs and reproduces the roots.

Checkpoints: the A16 class captures under its registered four-byte map at the family interval.
The hybrid registers NO map (its recurrence is genesis-anchored by declaration); its canonical
cadence is the empty leg — `interval = n_ctx`, above every legal job's decode-call count, so
`decode_calls / interval` is zero and the sentinel map is never asked to chunk anything.

Capability follows the class, not the binary: the v1 A16 profile keeps the legacy composite and
says `supports_court() == false`; the hybrid's court capability rides exactly the constructor that
proves servability (`from_registered_profile` — the plan is the proof, ADR-0067).

## 4. The consensus changes, and why they are one version

`PALW_V2_TRACE_FORMAT_VERSION` moved 2 → 3. Every item below changes what a court concludes about
an object a v2 court also concludes about — two verdicts, one object, which is a fork — so they
ship as one version and two builds refuse each other at the handshake instead:

1. `PalwDecodeTokenPinV1::TiledV1` — a tiled class pins its generated ids through the rows-tree
   root (one selecting row at a model vocabulary is hundreds of KiB against the ~80 KiB close).
2. `verify_kv_anchor` derives the anchor geometry from the class's REGISTERED `state_chunk_map_id`
   (four-byte map included) and widens at the map's own element width, instead of the one-byte map
   unconditionally — which read a v2 class's chunks at quarter width and froze honest classes.
3. The matmul arms' parameter triples ride the `{name}.a16` suffix (the rule the conv triples,
   grouped exps and rope clamp already follow): the bare name is the codes', both are requested at
   byte offset zero, and `find_operand_v1` resolves `(name, layer, offset)` before length — one of
   the two requests was structurally unservable by any canonical inventory.
4. The registration-shape alignments: scores/values read the ONE registered triple and tile it (a
   per-`(head, position)` request has no tensor to land on — the count is the job's); the softmax
   reads its registered single byte; `Fixed` vs `KvScaled` out-width decides per-lane vs
   singleton-tiled for the stream narrowings; the seven position-0 sink seams resolve `.sink0` by
   the family's FIXED list (an artifact-dependent rule would let a challenger steer which table
   the court reads by withholding an opening).
5. `run_program` reports the slice offset for the lane- and head-sliced families (it reported 0,
   mis-slicing every tile but the first); `src_tile` resolves the `LayerIn` sentinel the way the
   canonical leaf set does.
6. `check_job_context_shape` admits the tiled scheme pin.
7. The routed-expert resolution (§5).

t11's fingerprint moved d7510c7a… → 923fe103… (devnet → 65eaa6e7…). The genesis is untouched;
deployment is whole-fleet-together (a Relaunch train, or one upgrade window).

## 5. The routed experts — the hybrid's one genuinely new court arm

A `.routed` matmul node computes the k CHOSEN experts' projections, and which experts those are is
decided by the execution itself — the ids exist only in the `RouterTopk` node's committed row.
The court read `node.weight_name` verbatim, a name no artifact tensor matches: every routed step
was `Unadjudicable` by construction.

Derive, never declare: the canonical input set for a `.routed` node APPENDS the table's unique
router-topk row at the same `(call, position)` — committed material, opened like any other input —
and the arm walks the challenged tile block by block, reading each block's expert id off that row
and resolving the codes/exps/triples under the PER-EXPERT tensors the artifact already stores
(`blk.N.ffn_expert.{e}_…`) at expert-local offsets. The expert identity is never the accuser's to
choose: it comes off the claim's own committed row, proven against the claim's own step root.

Building the arm convicted five more court-side descriptions of this graph, each of which would
have refuted an honest producer at a real dispute: the router arm read `k` from a stored triple
the engine never reads and emitted an interleaved row (the engine commits ids then weights, and
`k` is the declared width halved); the combine passed the whole routing row where the kernel takes
its weight lanes, and assumed disjoint covering runs; `MulWide` needed the four name-dispatched
resolutions the engine performs (plain, routed-fused, shared-fused, scalar broadcast); the decay
read a declared name no store holds instead of its two calibration rows plus the bias; the
convolution's decode window was one position short, and its requant name carried a `.weight` the
store does not. The `.sink0` position-0 convention is now scoped to classes that declare the dense
family's own projection kernel — it was firing on the hybrid's like-named seams.

## 6. Registration obligations — what a court-capable class pins

* **`artifact_root` is an OPENABLE inventory root.** ADR-0049 Decision G said it about the floor;
  it now binds the tiers: a close's operand openings verify against the registered root, and
  nothing can be opened against a flat digest. `a16_inventory_v1` / `qwen36_inventory_v1` are the
  canonical layouts (each normalising its artifact's parameter store to the shapes the arms
  request — singleton expansion included), and `qwen25_a16_registration_v2` states the obligation
  on its signature. The SDK's dense lineage resolves the court-capable row by the row's own root
  derivation — one mapping, `CanonicalClassV1::artifact_root`, decides on both sides.
* **The corrected A16 class's head names the engine's own view** (`output.weight`,
  `QWEN25_A16_HEAD_TENSOR_V2`): the tied spelling put the gather's rows and the head matmul's
  tiles under one inventory name at colliding offsets, making one view structurally unservable.
  Tying stays a fact about bytes. This moves the corrected class's id — which was never
  chain-registered, and a class is its graph.
* The corrected classes pass the FULL admission gate at their real geometries
  (`verify_class_admission_v2`: shape, both coverage gates, the ladder bound, all three
  court-cost ceilings, the PWU recount) — asserted by tests, not claimed by comments.

## 7. What this ADR does NOT decide

* **When the fleet moves to fingerprint 923fe103…** — a deployment train (whole-fleet, since the
  ruleset id moves), the operator's and ADR-0068's to schedule.
* **When the corrected classes register on chain and when producers switch to them.** The v1
  classes remain registered, remain liveness-admissible, and remain unprosecutable — ADR-0039's
  words already cover them ("admissible for liveness and must not carry weight"); moving the
  weight-bearing share onto the corrected rows is the operator half of this ADR's promise.
* The hybrid's checkpoint-anchored recurrence (a registered state chunk map for the GDN state) —
  deliberately deferred exactly as the profile's own comment defers it; `n_ctx 8` is the
  genesis-replay budget until then, and raising the context is gated on that map.
* **Two findings the sweep left standing rather than papering over.** (a) The hybrid's compiled
  `forward_token_probed` walk records ~17 named sites per layer against the 46–48 nodes the v2/v3
  tables declare, so it cannot fill the declared step space: BOTH authorities capture by compiling
  the plan from the profile they hold (bit-identical to the compiled engine by the standing
  differentials), and a backend whose id names no ledger row stays legacy and court-incapable.
  (b) A per-token `embed_lift.a16` store — which the real converter may write for the 33 GiB
  artifact — is not servable per lane; the inventory refuses it rather than guessing, and the
  Requantize arm would need the gather's per-token resolution before that leaf adjudicates on real
  weights. Neither blocks a fixture-verified class; both block claiming the real artifact is
  covered, which is why they are written here.
* The hybrid's checkpoint-anchored recurrence, as above; and the `MulElem` cost arm's 9-byte
  triple pricing (A16 triples are 17), an under-pricing the already-admitted dense class shares.
