# A3 — one-capture typed-to-legacy/source adapter

Base: exact A0 `79c3f9bca`. This source-only prototype preserves the original
program, layout, dtypes, context, class identity, legacy roots and r1 APIs. It
does not assign a record, wire, domain, ID or native authority. Canonical
tokenizer correction `86932f2c7` and its evidence `618af71c8` were completed
separately; their historical counts are not added to this increment.

## Binding and A1 interface

`TypedCaptureAdapter::prepare(capture, TypedAdapterExpected, limits, stages)` borrows the
immutable capture and independently expected registration, artifact, job,
authenticated original Miner Result, rooted admission inputs and step space.
It joins both identity chains: expected class ID must equal the job/capture's
class ID, not only the Result's registration ID. It preserves the actual job
tokenizer (including zero) independently of registered tokenizer metadata.
Execution/input/seed/counts, original class/program/layout and rooted Result
checks remain required. Authenticated authorship is not native admission.

`expanded_claim(plan, index)` rederives an immutable A1 ExpandedRelation and
checks original position/occurrence/slot/node/dtype/resolved shape before slicing
exact native values. It accepts no peer-supplied relation or cursor authority.
`all_claims` exports the complete `Vec<Vec<i128>>` required by A1's existing local
freeze API; original native buffers remain live. The adapter does not clone the
ExpandedPlan. A1's borrowed metadata getters and `checker.preparation()` permit
short immutable source-building borrows followed by mutable `submit`.

`material(preparation, index, checked_frontier, side_store, rules, source_limits)`
checks the same expected Result/class/job and exact independently derived
frontier, then invokes A1's existing bounded source-only factory. The frontier
must be read from actual checker progress; building data is not authority to
advance it. The checker independently verifies its own next relation/frontier.

## Original legacy data, lazily reconstructed

| Original leaf kind | Exact retained source |
|---|---|
| Commit | Node at absolute position/occurrence/node/slot; original committed lane range |
| Fixed checkpoint | FixedAfter at that actual checkpoint/state/layer; original lane range |
| History tile | HistoryAppendRow at each absolute source position, oldest first; original sub-row lane range |

Only committable old dtypes enter the old four-byte encoder. Uncommitted i64 and
i128 values never become old leaves or authenticated operands. Each reconstructed
preimage uses the existing encoder/version/coordinate/count and must hash to
`capture.run().leaf_hashes[index]` under the original execution/class identity.
`legacy_opening` streams those existing hashes through the core's original
opening algorithm and verifies the old Merkle root. Two authenticated edge
paths produce existing range siblings. No second model run, full leaf-preimage
spool or full-tree copy is used. Test-only ordinary runs provide an oracle.

The auxiliary sorted index is O(records), with binary lookups. Streamed openings
use O(log N) auxiliary memory but O(N) saved-hash reads per opening; serving all
openings can therefore have quadratic CPU cost. Proof attempts, reads, hashes,
geometry and failed lookups are charged persistently. Builder moves and failed
deliveries do not reset them. This is not a production throughput claim.

`store(side_store)` supplies step preimages/openings/ranges only through this
hash-checked bridge. It delegates original parameter/prompt/decode-pin material
to the caller's existing bounded store. It never delegates step access or the
provider's begin/reset/replay hook. Hidden allocation/IO work of the side store
still requires its own owner reservation; this wrapper does not verify that a
caller-supplied reservation equals a physical allocation.

## Simultaneous backing and lifetime

The quote counts actual retained native buffer/shape capacities, original run
hash/row/generated capacities, index backing, **all** i128 target claims
(`16*target_lanes + sizeof(Vec<i128>)*relations`), two original binding clones,
generated IDs, input-authentication scratch, one tile and streamed path scratch.
It explicitly adds the caller's non-default preparation, source authentication,
evaluator and signature reservations. These are logical backing reservations,
not measured allocator/RSS or ML-DSA workspace guarantees.

`prepare` requires and persists all four external stage reservations, and checks
index + metadata + these stages before identity hashing or index allocation.
Later `all_claims` declarations can only increase each same owner's envelope;
independent owners must be pre-summed by the caller. Smaller declarations and
material Drop cannot release these persistent reservations. Every runtime check
uses the same additive stage inventory: all four persistent external components
+ live material's two body buffers + live source-auth storage + live evaluator
workspace. Even identically named external/live components may own independent
buffers, so they are added; no alias optimization is assumed. A1's evaluator
workspace already includes target indices/results, counted once within that
live component.

Before exporting all claims, the combined quote includes this entire live stage,
all future claim headers/lanes, prior cumulative exported claims, index, metadata,
tile and path scratch. Failure happens before reserving/charging the outer Vec.
All owned claim exports are cumulatively charged before allocation, including
outer vector headers. Every subsequent allocation includes this same persistent
and live stage inventory. `TypedAdapterMaterial` owns a single staging slot, with no Clone or
owned-buffer extraction. A second build while it is retained refuses. Drop (or
failed construction) releases only backing; spent work counters stay charged.
The slot is claimed before invoking external provider code, preventing recursive
material builds. Directly returned binding/preimage/path copies beyond one
active target/material, or explicit copies of borrowed fields, are caller-owned
retention and need additional caller reservations; this is not a global allocator.

No disk spool or IO benchmark is implemented. Dense target-lane count itself can
grow with history dimensions; this all-claims A1 prototype is not claimed to be
memory-efficient for large LLM contexts. Bounds may refuse an unchanged job;
the adapter does not reduce its context or rewrite the model to make it fit.

## Static range and native-acceptance boundary

The original sparse I32 CarryIn/Add capture fixture preserves its bytes and runs
numerically for its concrete inputs, but fails `analyze_ranges`' static sufficient
condition over the full declared dtype range. This is not evidence of actual
runtime overflow. Old V1 native admission itself requires that analysis; the
fixture is **not** described as a previously admitted/registered native class.
Ordinary learned lowering's I64→Clamp and I128→Clamp forms are not equated with
this hand-built fixture. No range guard is removed in this change.

A raw material builder may produce data for that declaration. The actual
source-authentication guard and A1 checker must refuse it as Delivery/unverified,
keep arithmetic coverage unchanged and never infer Miner fault or PanelMismatch
from the unsupported profile. A dedicated regression fixes that boundary.

A separate NEW range-admissible I8 carry/state/history class declaration is used
for local fullrelation integration, with genuine active i64/i128 intermediates.
Its original declaration is fixed before its class ID is computed; no existing
class is mutated to pass verification. Neither fixture has a native admission
receipt. Panel-only local signed claims are checked by the actual A1 checker;
the original Miner Result/signature are retained unchanged.

The original r1 PC2R1REC/Witness meaning is not used for extra typed records.
A1 test-only caller-local domains remain unpublished; no default/global domain
is added here. Adopted wide-final logits remain a separate later explicit
profile; this Root79/A1 prototype retains its i32-final limitation. Real LLM,
native body 176 acceptance, protected storage, funding/locks/fees, Gate C and
public activation remain outside these local tests. Frozen-candidate commands,
results and raw hashes will be attached separately.
