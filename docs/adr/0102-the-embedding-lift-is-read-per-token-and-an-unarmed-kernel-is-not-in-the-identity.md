# ADR-0102 — The embedding lift is read per token, and a kernel a network has not armed is not in its identity

* Status: PROPOSED 2026-09-10 on `feat/adr-0099-sharded-seat`; **Decisions 1–4 IMPLEMENTED the same
  day, consensus-inert on every shipped preset** (§9): the graph, the kernel, its fence, the
  admission refusal, the registration root, the manifest variant and the measurement. The fence
  `Params::palw_token_lift` is `None` everywhere and the kernel is outside `court_catalog_root`, so
  testnet-11's and devnet's fingerprints do not move (`shipped_presets_have_pinned_fingerprints`).
  Decision 5 (the table row, the drill family, the arming) is stated, not built.
* Builds on: [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
  (the held measurement, whose §1.3 refused this artifact by name),
  [0082](0082-the-close-is-flat-in-the-context.md) (graph-v5's fused attention, which graph-v6
  keeps), [0070](0070-the-model-tiers-step-spaces-are-adjudicable.md) §7(b) (an inventory refuses a
  store it cannot serve rather than inventing one), [0042](0042-palw-mainnet-candidate-ruleset.md)
  Decision 11 and [0052](0052-palw-qwen36-hybrid-class.md) (`court_catalog_root` is part of every V2
  ruleset id), [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) /
  [0075](0075-certification-is-a-consensus-object.md) (weight needs a certified family),
  [0067](0067-classes-are-chain-data-kernels-are-the-build.md) Decision 5 (the chain-registered arm),
  [0093](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) (the fused
  site's responder, which no family implements yet).
* Amends: ADR-0100 §1.3 (its last row now has an answer). Supersedes nothing.

## 0. The sentence this ADR is

**The hybrid engine lifts each token's embedding row by that token's calibrated triple, and every
hybrid graph declared the lift per lane — so no calibrated hybrid artifact had an inventory, a
measurement or a court. Graph-v6 declares what the engine does.** Its kernel is new, and a kernel
appended to the court's catalog moves every V2 network's identity the day the binary ships; so the
kernel lives in a fenced table outside that identity, and a network admits it by arming
`Params::palw_token_lift` — the fence, not the build, is what enters the identity, and only there.

## 1. What was measured

### 1.1 The refusal

`palw-class measure --network testnet-11 qwen35-2b.palwq36` (ADR-0100 §1.3, 2026-09-10): the
inventory built under the graph-v5 profile the manifest projects refused the artifact —
`embed_lift.a16: 248320 triples serve neither one lane nor 2048`. A measurement that cannot place a
row refuses rather than estimates; that was correct, and it was the whole answer.

### 1.2 Why: three readings of one store

| who | reads `embed_lift.a16` as |
|---|---|
| `qwen36-convert` | writes ONE calibrated triple per vocabulary row (248,320 for the 2B) |
| the engine (`qwen36.rs`) | a one-row store lifts every token by that row; a longer one is indexed by the token (`lift.get(token_id)`) |
| graph-v1 / v3 / v5 (`KDESC_A16_REQUANTIZE`, lane-sliced over `Hidden`) | one triple tiled across the lanes, or one per lane (2,048) |

The only store all three read alike is the one-row broadcast — which only fixtures carry. Every
converted hybrid artifact fell outside the graph: the inventory refused it (so no measurement and no
registered inventory root), and the court, had it been asked, would have read per-lane triples the
store does not have. The latent half: a store of exactly `hidden_dim` rows is one the lane-sliced
inventory accepts and the engine indexes by token — the two disagree on it. No converter writes one,
and graph-v6 removes the disagreement for the lift rather than guarding it.

### 1.3 The flag day this does not take

The first build put the new kernel in the adjudication table beside the others. The fingerprint pin
went red on two presets:

| preset | pinned | with the kernel in the catalog |
|---|---|---|
| testnet-11 | `060e3597…` | `1c42e99b…` |
| devnet | `b40976b2…` | `14a2ba69…` |

`court_catalog_root` hashes `catalogued_kernel_ids_v1()` into every V2 bundle, so a kernel nobody's
class reaches still changes what peers compare at the handshake — and the 2026-09-09/10 partitions
came from identity-moving changes that shipped unannounced. Decision 2 is the answer; with it the
pin is green and unchanged.

### 1.4 The held measurement under graph-v6, on the real artifact

`palw-class measure --network testnet-11 --name Qwen/Qwen3.5-2B qwen35-2b.palwq36` (this Mac):

| what | value |
|---|---|
| the file | 2,484,508,160 bytes, lineage `qwen36-mmap-v1`, 24 layers |
| bytes measured from the inventory | 2,484,279,940 (the family formula says 2,388,705,280 — a floor, as ADR-0099 said) |
| the inventory root (what a graph-v6 registration pins) | `189c78352da5…` |
| the root computed over the mapping (what the v1/v3 rows register) | `d6d79a213180…` |
| the class at 512 | `3d57104c65a5…` — **admitted** |
| at 32,768 | `91bbf9b05370…` — refused by the ladder, the court window, the PublicDa payload |
| at 131,072 | `ac0bb11d257e…` — refused by those and the state chunks |
| at 1,048,576 | no profile — the geometry ceiling (ADR-0097) |
| `verify --artifact` | every field recomputed equal; the deterministic half agrees; exit 0 |
| `preflight --model-id …/graph-v6` on testnet-11 (run while a graph-v6 table row existed; Decision 5 removed it) | **refused by name**: "the class reaches the per-token lift kernel and this network has not armed palw_token_lift" |
| the document | id `eb277d48bc06…`, unsigned; measured twice — before and after the §9 recovery — byte-identical |
| cost of one measurement | cold: 303 s wall (20.8 s user — a read-bound pass over the file), 1.68 GB resident; with the file in the page cache: 18.8 s, 5.82 GB resident (the mapping's pages count, and the inventory copies every row) |

A temporary graph-v6 row at the table's own geometry (`n_ctx` 8) paired to the same inventory root
as the measurement at 512: the root is a function of the weights and the node tables, not of the
context.

### 1.5 The court over graph-v6

On the hybrid fixture with a non-uniform per-token lift (`a_graph_v6_hybrid_adjudicates_a_per_token_lift_and_a_tampered_lift_convicts`):
the graph-v5 inventory refuses the store by name; the graph-v6 inventory commits one leaf per
vocabulary row (and tiles a one-row store across the vocabulary, the engine's other reading); every
leaf of an honest capture the one-step court can try clears (the fused attention sites are the
dissection's, ADR-0082); a lane corrupted at a lift leaf convicts at a prompt position and at a
decode call — whose token is a GENERATED one — and the prover's opening is exactly one 17-byte
triple.

## 2. The requirement

A converted hybrid model — the family the K3 stand-in is written in — has to be measurable,
registrable under a root a close can prove against, and adjudicable at its lift, without moving the
identity of any network that does not want it.

## 3. Decisions

**Decision 1 — Graph-v6 is graph-v5 with the lift read per token.** One node differs: the lift after
the gather names `KDESC_A16_REQUANTIZE_BY_TOKEN` (`a16/requantize/token-row/i128-mul-rshift-sat16/v1`):
the same `a16_requant` arithmetic, with the ONE triple at the position's token's row — the token read
from the carried ids exactly as the gather reads it (the prompt's at call 0, the generated one at
call c ≥ 1). The inventory commits the store one row per token (a calibrated store verbatim, a
one-row store tiled across the vocabulary); the plan compiles the node to the op the engine already
runs. A class is its graph: a new class id over the same artifact, and every earlier class stays the
chain fact it is.

**Decision 2 — A kernel a network has not armed is not in its identity.** The adjudicator gains a
second table, `KERNEL_CATALOG_FENCED_V1`: resolved like any other kernel, excluded from
`catalogued_kernel_ids_v1()` and therefore from `court_catalog_root`. The admission gate
(`verify_class_admission_v7`, and v6 is v7 with the fence down) refuses a class that reaches a fenced
kernel unless `Params::palw_token_lift` is active at the block — by name,
`TokenLiftNeedsItsFence`, before any coverage walk — and judges the rest of the graph against the
identity catalog as before. `validate_palw_v2` refuses a genesis that registers such a class unless
the fence is armed from genesis (genesis rows do not pass through the gate; the ADR-0082 fused-row
precedent). The fence is Some-only in the fingerprint, the schedule id and the fork id.

Why two builds agree: on a network where the fence is dormant no class can reach the kernel — this
build refuses the registration by name and a build without the kernel refuses it as a coverage gap —
and no existing class reaches it, so every verdict and every root is the same on both. Where a network
arms it, the fence's height is in its fingerprint and the fork-id gate keeps an older build off.

**Decision 3 — A graph-v6 registration pins the operand-inventory root.**
`qwen36_registers_inventory_root_v1` is the one spelling: a hybrid graph that reads the lift per
token registers the root a close's openings prove against; every earlier hybrid row keeps the root
computed over the mapping, as the chain registered them. The SDK derives the inventory root once per
graph per holding and memoizes it (the derivation copies every row — 2.5 GB for the 2B); the
chain-registered arm finds a holding by its computed root (possession, as the dense lineage's digest
is) or by that inventory root under the same graph.

**Decision 4 — The manifest names the graph.** `PalwModelFamilyV1::HybridQwen36TokenLift`
(`"hybrid-qwen36-token-lift"`) projects graph-v6; appended last, so every earlier document's Borsh
bytes — its id, its signature — are what they were, and a `hybrid-qwen36` manifest keeps naming the
graph-v5 class it always named. `palw-class measure` states it for every held hybrid artifact.

**Decision 5 — Not the canonical table yet, and why that is right.** No graph-v6 row joins
`qwen36_canonical_classes_v1`. Two invariants this tree pins refuse one, and both are correct: the
SDK conformance battery (every row's kernels in the identity catalog) and "every catalog class is
certifiable by a committed family without a code change" (ADR-0075's mainnet route). No committed
family covers a fenced kernel, and widening one would move `court_e2e_root` — the identity again, and
a certificate for an adjudication nobody performed (ADR-0069 Decision 5). The row, a drill family
that convicts through the per-token lift (the §1.5 test is its unit-level form), and the arming go
together, as one ruleset move for the network that chooses it.

**Decision 6 — `supports_court()` is not narrowed.** It answers two turns — disclosure and
arithmetic — and the producer reads it for the first (the DA court's responder), which every hybrid
row has. The arithmetic half over a calibrated artifact under a v1/v3 row is refused where it is
asked (`operand_openings_for` names the store), and a seat reads that as a sample it could not
clear, never as a conviction.

## 4. What this costs

One node table, one kernel arm, one fence at the `Params` sites every fence has, one admission
refusal, one inventory branch, one manifest variant. A graph-v6 root costs one pass that copies the
artifact's rows: fine for the 2B (a 2.5 GB copy), 33 GiB for the 35B-A3B — a streaming inventory is
the prerequisite for measuring and serving K3-class artifacts this way (§8).

## 5. Invariants the tests hold

1. The fenced kernel resolves, is disjoint from the identity catalog, and is not in `KDESC_ALL` —
   `a_fenced_kernel_resolves_and_stays_out_of_the_identity`.
2. Shipping it moves no fingerprint — `shipped_presets_have_pinned_fingerprints`, unchanged.
3. The fence is dormant on every preset, visible in the fingerprint when armed, `never()` is
   absence, and a genesis registering a graph-v6 row needs it from genesis —
   `the_token_lift_fence_is_dormant_visible_when_armed_and_its_kernel_is_outside_the_catalog_root`.
4. Dormant: graph-v6 refused by name and graph-v5 untouched; armed: both admitted at the 2B geometry
   at 512 under the RC-shaped court, the v6 entry recording the fenced kernel —
   `the_per_token_lift_is_admitted_by_its_fence_alone`.
5. The court: honest leaves clear, a tampered lift lane convicts at a prompt position and a decode
   call, one 17-byte opening; the v5 inventory refuses the store, the v6 one serves it and a
   singleton — `a_graph_v6_hybrid_adjudicates_a_per_token_lift_and_a_tampered_lift_convicts`.
6. The registration root: the inventory root under v6, the computed root under v2, resolved twice
   (the second from the memo), possession by either — `a_chain_registered_graph_v6_class_resolves_by_its_inventory_root`.
7. The manifest: v6 is its own class over one geometry and one artifact, JSON names it, Borsh tags
   0/1/2 — `a_token_lift_manifest_names_graph_v6_and_moves_no_earlier_document`.

## 6. Order of work

1. Decisions 1–4 — **done** (§9).
2. A drill family for the per-token lift (the ADR-0069 route): a fixture drilled to a conviction
   through the lift, its kernel set, and the question of how a network commits it without a re-mint
   (ADR-0075's chain certification is the candidate).
3. The graph-v6 table row and a devnet flag (`--palw-token-lift-devnet`), then a devnet drill:
   register the 2B under graph-v6, produce, sample, convict at a lift leaf.
4. A streaming inventory, so a K3-class artifact's root is a pass, not a copy.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0100 §1.3, the 2B artifact "refused by name" | measured under graph-v6 (§1.4) |
| graph-v5 as the hybrid measurement's graph | kept for `hybrid-qwen36` manifests; a held artifact measures under graph-v6 |
| "a kernel the court adjudicates is in `court_catalog_root`" (ADR-0052) | kept for the identity table; a fenced kernel is adjudicated and admitted by its fence |

## 8. What is deliberately not decided

* **Arming** on any network, and when.
* **How a network commits a drill family for the fenced kernel** without a re-mint.
* **The streaming inventory's shape.**

## 9. Number hygiene and implementation record

0102 is the next free number after ADR-0101 (whose §9 says so). Claimed on
`feat/adr-0099-sharded-seat` on 2026-09-10. **The next free number is 0103.**

* **2026-09-10** — written and implemented the same day:
  * `consensus/core/src/palw_step_refute.rs` — `KDESC_A16_REQUANTIZE_BY_TOKEN`, `Qwen36Op::RequantizeByToken`
    and its arm, `KERNEL_CATALOG_FENCED_V1`, `fenced_kernel_ids_v1`, the resolver over both tables.
  * `consensus/core/src/palw_qwen36_profile.rs` — `QWEN36_PRE_IR_V6`, `qwen36_profile_v6`,
    `qwen36_artifact_row_profile_v6`.
  * `consensus/core/src/config/params.rs`, `fork_id_v1.rs` — `Params::palw_token_lift` at every
    site, its genesis rule, its test; `palw_class_admission_v2.rs` — `TokenLiftNeedsItsFence`,
    `verify_class_admission_v7`, `PalwAdmissionShapeV1::token_lift`,
    `palw_genesis_reaches_fenced_kernel_v1`; `consensus/src/pipeline/virtual_processor/processor.rs` —
    the cached fence and the acceptance call through v7.
  * `consensus/core/src/palw_measured_model_v1.rs` — `HybridQwen36TokenLift`; the manifest test.
  * `misaka-palw-base0/src/inventory.rs` — the per-token branch, `qwen36_registers_inventory_root_v1`;
    `qwen36_plan.rs` — the lift accepts the kernel; `qwen36_backend.rs` — the court test.
  * `misaka-palw-sdk` — the memoized root (`registered_root_of`), the chain-registered lookup by
    either root, the preflight through v7, the test; `palw-class measure` under graph-v6.
  * The working tree was lost to a scratchpad wipe at the session's restart (2026-09-11) before it
    was committed, and rebuilt by replaying this session's recorded edits onto `a9c9abe4`; the
    replay is what §5's tests and §1.4's measurement ran against.
