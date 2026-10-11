# RFC06 cell resource consumers — 2026-10-11

This record extends the [four-RFC requirements index](rfc-0006-0007-0008-0012-design-audit-2026-10-10.md) and [SHARD implementation](shard-rfc6-10.md). It does not replace any RFC06 acceptance condition or approve a class, fee, soundness floor or activation.

## Concrete defects and implementation

A full-holder seat reserved host scratch and, when available, device memory. The shard-only seat and the non-seat public watcher started the same verifier without those reservations. Their work could overcommit a host while the producer, another seat or a court already held its budget. The shadow consumer also bypassed admission. The old seat released its admission immediately after checking cells, before building the exact court proof.

`kaspad/src/palw_panel/tir_shard_resource.rs` now owns the common admission. Full-holder seats, shard-only holders, public watchers and shadow runs use it. The CPU/device backend is constructed after host admission. If device memory cannot be reserved, CPU verification remains available under the host admission. A failed host reservation leaves the duty unanswered and retryable. The guard survives cell verification and accusation construction and releases on success, error or unwind. Scratch arithmetic is checked rather than saturated.

The common scratch envelope is the existing cell-seat node policy: three times the capture size plus 16 MiB. A decoded shard-only capture is counted without allocating another serialized copy. This is an admission mechanism and a bounded arithmetic expression; it is **not a verified worst-case RAM, work, elapsed-time or court-fee bound for every model**. Persistent fetched rows, material pools, model caches and exact prover workspace still need their own complete accounting and measurements.

A second defect defeated the intended shard memory scope. `TirExecutor::new_cell` initialized all `Fixed` state instances in the class, including every other layer. Restoring a segment repeated that full allocation. The executor now derives exactly the state instances touched by its immutable occurrence range, including read-only state. Other instances retain their indices with empty buffers. Checkpoint import/export respects that scope. An attempted wider cell, a whole-schedule step through a cell executor, or a checkpoint predicate naming another shard is refused before execution. Whole-schedule execution keeps its existing full state.

These changes are node execution/resource behavior. They allocate no consensus tag, delta, rooted field or new fence, and do not change live shard v1 pricing, draws, receipt rules, or public preset parameters.

## Evidence

`cargo test --locked -p misaka-palw-tir-exec --lib cell_resource_tests -- --nocapture` passed 2 tests with no failures or ignored cases, after a 3.12 s build; log `/tmp/rfc06-cell-memory-focused-final.log`. The tests verify scoped allocation after both restoration APIs, inclusion of read-only state, an actual cell step, unchanged full-state execution, and refusal of expanded scope or foreign-shard restoration. Initial test fixtures incorrectly used a noncanonical history bound and cross-role global state; those fixtures were corrected before this final run.

The common admission has additional tests for seat/watcher/holder contention, proof admission after release, checked overflow without mutation, error/unwind release, and refusal before backend construction. These require the centralized kaspad build and are not counted as passing before that build runs. Existing corpus cell verification, shard-only proof, non-seat conviction, device parity and node checks must also run against the integrated change.

## Full RFC06 closure conditions still tracked

| Requirement | Required remaining evidence or implementation |
| --- | --- |
| R6-CELL | Whole-claim coverage for every accepted versioned kernel/profile, including authenticated boundaries, memory and routing; sparse-error/algebraic soundness distinct from the first-divergence replay lemma; actual bounded public localization and terminal court for every covered relation. |
| R6-DRAW | Frozen population, retry/operator rules and one reservation have existing engine/fold tests. The historical stratified production adapter truncates overfull populations by collateral then bond id; the flat adapter instead returns ResourceLimit, which its historical fold maps to an empty population. The historical engine caps candidates at 4,096. A synthetic 100k ranking does not prove 100k eligibility or neutrality. The population policy, historical headroom, all actual consumers and transport require explicit closure. |
| R6-OUTSIDE | Authenticated public claim material, shard-only non-seat prosecution including dissected leaves, complete retention through the liability horizon, and pruned P2P recovery. Model acquisition remains off-chain under ADR177; unavailable models provide no positive detection floor. |
| R6-PRICE | Exact worst cell material/work/DA/court bounds and measured cost/capital terms, funded collectible fees, agreement between all lock/pay/readiness consumers, and real small/large/window-saturated results. Host readiness is node policy rather than an on-chain capacity proof. |
| R6-MIGRATION | Live t12 DAA 5,300 rules and dormant identities remain pinned. New profiles still require their own reviewed fence and activation evidence. Preserve historical open questions and distinguish existing v1 decisions from new-profile approval. |
| Resources/network | Release measurements at the actual intended 100k population and worst registered shape, with CPU/RSS/latency recorded; several actual processes exercising P2P/pruning/join/reorg. In-memory disclosure-pool and fold tests are not evidence of a pruned network join. |

All these conditions remain part of the full RFC06 objective. The memory fixes above do not turn any UNKNOWN/FAIL economics or public soundness gate into PASS.


## Fetched rows: ownership, admission and eviction

A mirror fetch and a peer-row pursuit reserve the same host ledger before row lists or
weight tensors are constructed. The checked node-policy envelope is
`32 × operand bytes + 8192 × inventory leaves + 64 × encoded program bytes + 16 MiB`.
The operand and leaf counts are derived from authenticated class geometry and inventory
spans without enumerating every leaf. The multipliers cover the current i128 MapParams
construction, encoded/native copies and paths/collection metadata conservatively;
allocator/process RSS measurements and arbitrary-model workspace certificates remain open.
This is an explicit local estimate, not an approved consensus RAM bound or a price.

The pursuit owns its guard from admission through retries. Completion moves that guard
to the held rows without dropping and reserving again. The cached rows and guard share
one Arc: removing a cache entry cannot release memory while a verifier or court builder
still reads those rows. A mirror error, refused reconstruction, eviction of an unfinished
pursuit, or the last row owner's release returns the budget through RAII.

Every panel tick retains cache/fetch entries only for the actual union of this bond's
seat and enabled public-watcher duties through their receipt deadlines. Cache keys bind
`(class id, layer-shard count, shard)`, so a replan cannot reuse another partition's
parameters. This bounds stale cache lifetime; global material pools, whole-job run
pursuits, post-Final evidence retention and actual pruned P2P recovery need their own
acceptance evidence and are not closed by this cache change.

Validation added: a real tiny artifact fixture checks exact profile versus fetched
operand bytes/leaf count, shared-budget refusal, live watcher retention, incompatible
partition eviction, expired incomplete-pursuit release, and a verifier keeping its
reservation after cache eviction. Resource unit tests cover last-Arc release and all
three checked-overflow terms without large allocations. These daemon tests are pending
the parent integration build, not reported as passing here.


## Public watcher with partial rows

The actual watcher service now accepts a public capture without a full-class backend.
With `--palw-tir-shard-mirror` or `--palw-tir-shard-fetch`, it uses the same admitted row
acquisition/cache as a shard seat and runs the same cell verifier and held-row court.
The watch duty's class/root and shard partition must match both the public capture and
held rows. It files no seat receipt and does not send a seat-signed producer material
request. A rotating duty cursor prevents an unavailable early shard from monopolizing
the two verification/fetch attempts per tick; abstention leaves the duty retryable.

A planted lie with a one-move held-row close can accuse as the watcher's own bond.
A dissected leaf stays explicitly Unconvictable for this partial holder because the
interactive accuser playback still requires full execution. Without a public capture,
the shard-only watcher cannot yet assemble bounded cell runs directly from the native
service; the existing full-holder run pursuit remains available. Neither this connection
nor the fixture establishes an unconditional acquisition/detection floor, all-model
soundness, a worst-case court-cost certificate or real pruned P2P transport.

The public `cell_runs_v1` door also refuses malformed occurrence ranges before indexing
an upstream occurrence, reversed/outside positions and unaligned nonempty segment
starts. Empty terminal cells keep their no-demand meaning. A regression fixture checks
the previous out-of-bounds panic and refuses material a segment cannot restore.


## Complete population profile and native evidence candidate

`palw_permissionless_panel_complete_v1` is a separate dormant fence; every shipped preset
sets it to None, Never changes no identity, and every attempted activation is refused.
Its carriage version is 3. The historical version-2 policy, schema ordering and 4,096
behavior remain intact. An existing nonempty version-2 engine cannot be silently
reinterpreted as version 3: the fold refuses that migration. A reviewed historical
migration is still required before any activation.

The complete profile keeps every structurally eligible candidate at the pre-entropy
checkpoint, canonicalizes by `(operator, bond)`, and commits the whole frozen population.
Both actual flat and stratified FoldView adapters use its version and propagate capacity
refusal. Above the policy's candidate bound, the claim ends non-fraud with
`POPULATION_RESOURCE_LIMIT` before making a seal; no capital-ordered subset is drawn.
The readiness commitment hashes the same checkpoint root once per population rather
than once per bond. The native lifecycle producer gate resolves the new fence and
op220's real observation carries the carriage version, population-complete flag,
retained candidate count and configured limits.

The selector holds one operator group and the requested seat prefix, with exact existing
ticket/192-bit comparison and operator tie breaks. Every frozen eligible bond contributes
to its operator's weight, even if its current headroom excludes that bond; within an
operator the best eligible bond with headroom may sit. The used-operator set excludes
prior retries. This is O(seat prefix) selector memory; the frozen population and native
registry remain materialized and retained. It is not an O(seat prefix) whole-node bound.

Experimental consensus refusal constants:

| Resource | Complete-profile bound | Meaning / unresolved release condition |
| --- | ---: | --- |
| Candidates in one frozen population | 100,000 | An overfull population is refused whole; no truncation. |
| Candidates retained across claims | 400,000 | Four 100k populations exhaust it; shared population storage/expiry and long-running liveness remain unimplemented. |
| Historical seats across claims/retries | 65,536 | Retry history and reservations must validate below the bound. |
| Encoded engine import | 256 MiB | Bounds the engine only; native registry, class declarations, allocator RSS and prover memory are separate. |
| Admissions / assignments per block | 1 / 1 | Refuses unbounded parallel population work; it does not approve latency. |
| Pending / tracked claims | 128 / 1,024 | Historical larger profiles remain version 2. |

`cargo test --locked -p misaka-palw-panel` passed all 40 tests, including exact streaming
versus full operator races for fragmented collateral, roles, headroom fallback and
retries; unordered/duplicate/overflow input refusals; synthetic 100k seal/draw/retry/
import and 100001-candidate refusal. Final log:
`/tmp/rfc06-complete-leaf-suite-pass.log`. A 100k fixture engine after retry encoded to
34,104,279 bytes; the initial successful seal/draw/retry/import portion took 1.823 s on
the local machine. That is leaf-fixture evidence, not native throughput, RSS, key
independence or an approved release threshold. New core fence tests and native tests
are pending central compilation and are not counted as passing here.

The native `t12_complete_population_e2e` candidate builds real distinct ML-DSA-87 keys,
drops each unused private key immediately, installs actual BondRegistered objects and
funded collateral UTXOs in Config, recomputes the genesis UTXO commitment, and uses the
real processor writer. Its small test covers complete commitment, over-cap refusal,
actual op220 observation, carriage reload and block-feed replay. Its ignored resource
test targets exactly 100k eligible candidates, then 100001: it prints key/setup time,
native genesis time, draw time, native bond count, engine bytes and whole carriage bytes.
Run on a dedicated central runner, capturing `/usr/bin/time -v` (Linux) or `-l` (macOS),
free disk and CPU; no result exists yet. Block-feed replay is explicitly not pruned P2P.
The harness bypasses closed activation and uses reference entropy; it proves neither
source independence nor positive conviction probability.

The native writer's duplicate public-key/operator walks were O(N²) for a registration
batch. A lazy per-fold transient index removes that product: only hash buckets are
stored for public keys and collisions still compare exact key bytes; the earliest
operator holder remains in bond order. `write_bond` keeps it coherent and `restore`
invalidates it. It enters no root, schema, delta or carriage. The candidate test compares
walks through registration, collision, removal/replacement, rollback, journal apply/
revert and carriage reload; the historical operator gate test remains intact.

## Public court, DA and pricing: exact existing bounds and open costs

The class admission path already enumerates every committed cone and stores
`court_cost { max_close_bytes, max_terminal_macs, max_operand_count }` in the class
catalogue. Admission step 9 calls `palw_tir_carried_closes_admit_v1` on the deepest legal
job, sizing the terminal close as it is carried, including multiproofs and dissected
root claims. Existing `palw_tir_close_size` and `palw_tir_close_range` tests compare those
bounds against actual closes and the exact faster range driver. These are size/work
bounds for the admitted carried form, not elapsed time, host RSS, watcher income or a
whole-claim probabilistic soundness certificate.

Current default court policy has 16,777,216 terminal MACs, a 2,250,000-byte close ceiling
from 27 chunks of 100,000 bytes with framing, and derived kernel element/term limits of
four times the terminal MAC ceiling. A public TirStepRun has at most 256 leaves. Existing
DA session policy permits at most three concurrent non-seat opens, sixteen lifetime
non-seat opens and four lifetime seat opens per claim. These existing limits are not
newly increased or financially approved by RFC06. Repeated sessions, interactive depth,
whole-job capture construction, material retention, source fees, first acquisition and
stake capital must be included when pricing a worst public prosecution. The partial-row
watcher still refuses a dissected close it cannot build and cannot yet assemble missing
captures from bounded cell-run service responses.

All-model cell evidence remains the node-feature corpus suite: golden primitives and
small dense/sliding/GDN/Mamba/MoE/history programs across layer partitions, aligned
position boundaries, honest pipeline equivalence, planted leaves, consistent upstream
boundary lies, generated-token faults and unavailable inputs. R6-CELL still requires
full accepted profile/version and routing/memory relation coverage, public localization
and exact terminal courts. Exact replay checks do not prove a sparse constraint check's
whole-claim detection floor. No unknown or zero floor is promoted here.

Existing price consumers are `palw_tir_shard_fold_v1`'s bound-seat pay weights, shard-part
lock weights, and `palw_tir_shard_v1::palw_tir_shard_room_v1`; the dormant segment readiness
consumer is `kaspad/src/palw_panel/tir_shard.rs::capacity_admits`. Lock and pay use a claim's
accepted DAA and existing `max(work share, resident share)` policy. Node readiness is a
local declared capacity. Release closure still requires every consumer/epoch boundary
test, the real worst cell workspace/public-court/DA envelope, funded collectible fees,
watcher capital and measured small/large/cap/window profitability. These memory admission
patches do not supply those economics values.

## Pruned P2P conflict remains concrete

Actual server `protocol/flows/src/v8/request_pruning_point_snapshots.rs` and requester
`protocol/flows/src/ibd/flow.rs` share `PALW_PRUNING_STATE_MAX_BYTES = 384 MiB` for the
whole native PALW carriage plus class declarations. The server materializes the full
single message and refuses an overfull response. A 256 MiB engine import limit alone
cannot guarantee that a 100k native registry plus several frozen populations fits this
transport. The resource drill records whole-carriage bytes precisely to expose this
conflict; it must not be called a pruned join PASS.

[The existing chunked transport design](pruned-ibd-palw-state-chunked-transport.md)
requires new protocol messages, fixed pruning-point/root across chunks, spill-file
back-pressure/streaming decode, resumable authenticated chunks, class declaration
coverage and actual multi-process reorg/drop/resume/pruned-join tests. It is still a
release condition. The single-message cap is not raised without resource evidence,
and no VM/Torrent/Seeder/acquisition consensus gate is restored.
