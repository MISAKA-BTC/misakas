# Fully owned artifact params — source-only opt-in component draft

Inspected base f95887209. Only this /tmp note and /tmp/a3-owned-artifact-params.patch
are written. No actual worktree/Cargo/compile/test/commit. Rustfmt stdin syntax
parsing and git apply --check are the only applicability checks. No measured
allocation, RSS, execution or root-authentication test evidence is claimed.

## What the patch contains

1. A cfg(node) producer-owned Params table quote next to params.rs::new/insert.
2. A read-only checked-header layout() accessor forwarding to the existing
   bounded_v1 metadata parser; no new grammar, defaults or Core dependency in
   the artifact crate.
3. Node owned_artifact_params_v2 and explicit opt-in module declaration. This is
   no backend/registry/device/default dispatch or native capability activation.
4. Four UNRUN source regressions, including real small temporary PALWTIR1 files.

Hard dependency: A0/A1-reviewed public growing_vec_request_capacity_v1 from
Core9c010109dc56f51808c51dded67c4b93df10fa2c. The current inspected branch does not
supply it, so this draft is NOT independently type-checkable until that API lands.
It uses the exact shared logical collection envelope, not a copied private rule
or encoded-size*64/guessed scratch constant. The public BTree request envelope is
explicitly restricted to64-bit here; unsupported targets fail before admission.
The separate NF-workspace and TirPlan::compile quote gaps remain untouched.

## Caller-owned/admitted inputs and identity boundary

OwnedParamInputsV2 borrows a bounded checked header/program view, an UNMODIFIED
TirPlan::compile result, independently rooted/admitted expected binding and
opaque strict V2 verified space, and its original context. These owners AND their
existing source/parse/compile/host tickets stay externally reserved. The caller
must already establish that expected/verified/context refer to the same original
binding. The component does not re-decode, validate, compile, construct another
StepSpace, authenticate signatures, or turn mutable public compiled fields into
a trusted plan. In particular this function is not a new context-hash/class-ID
admission boundary: the independent association is an explicit precondition.

Loader compares original context literally to expected.job_context; original
header program to registered expected program; checked program to borrowed V2
program and plan.program; expected layout to V2 layout/job geometry; and header
registration tokenizer to expected.CLASS tokenizer. Registration18 and actual
job tokenizer0 stay separate; no equality is forced or field rewritten.

Header Some(layout) must match every original expected scalar and word literally.
Header None is legitimate undeclared lowerer metadata: only the independently
admitted original expected layout supplies that pin. No default, max-context
choice, file serialization, or original byte rewrite is invented. Returned
layout_origin distinguishes DeclaredInFile from IndependentlyExpectedOnly.
The header remains unchanged, including an empty layout, tokenizer and provenance.

R: Read+Seek is the SAME already-open descriptor used to check header/file extent.
The caller owns descriptor identity, bounded custom provider internals, file stamp
and mutation checks. No path is reopened. The component itself reads only the
checked extents. A different descriptor cannot be made the same merely by this
Rust signature; callers may not claim that stronger association without their
source owner. Concurrent in-place mutation has no immutable-file guarantee.

The returned OWNED native values, not file bytes alone, are re-encoded into one
bounded piece buffer and authenticated by the authoritative Core visitor plus
artifact_leaf_parts_v1 and PalwArtifactMerkleFrontierV1. Root must equal independent
expected.artifact_root. This is G14 data identity, never model arithmetic truth,
model qualification, a Court outcome, Gate C, SecurityCertified or credit.
The owner keeps all borrowed pins alive and exposes params only by shared borrow.
Once owned, future source-file mutation cannot alter its native parameter Vecs.

## Exact native reading and retained ownership

No unsafe, mmap, Tensor, MapParams, lazy row source or full i128 parameter mirror
is used in production. Each declared I8/I16/I32/I64/Idx tensor reserves one exact,
fallible native Vec. A single exact bounded byte chunk is filled from the checked
extent, decoded using primitive from_le_bytes, and pushed into that Vec. Signed
endpoints and all u32 Idx bits survive. I128 params refuse. Buf moves its native
Vec into ParamData::Cow::Owned and then an empty, preallocated Params slot; there
is no payload clone. The canonical checked table forbids duplicate inserts.
read/seek/alloc/identity errors are fixed enums, with no prequote String payload.

Chunk is sized to the largest ACTUAL inventory piece or declared element width,
not an unconditional maximum. It must fit max_chunk_bytes; a policy smaller than
an actual piece refuses rather than silently allocating32KiB. The same buffer
is reused to serialize a piece from the OWNED typed slices for Core leaf hashing.
Every tensor must fit Core's existing u32 byte-offset coordinates before calling
the visitor. The authoritative visitor remains the only actual leaf ordering;
closed-form prequote leaf counts admit work/buffers but never mint another root
mapping/domain. A latched callback error skips later hashing while the visitor
finishes its already admitted bounded metadata/coordinate walk (API is infallible).

OwnedArtifactParamsV2 holds params before its optional memory lease, and borrows
source/pins owners for the entire returned lifetime. The owner has no Clone/move-out extraction. Borrowed TirParams deep Clone and exports are separately caller-reserved; its guard does not follow copies.
The existing optional host reserver receives original verified class/context IDs
and new_owner_peak_bytes; its own bookkeeping is an independently admitted
provider contract. If absent, no guard is fabricated. Its lease stays attached
until owned params drop, retaining the scratch-inclusive reservation conservatively.
No global memory policy/flag is changed. Core local owner is NOT a host lease.

## Typed quote and coexistence inventory

Producer quote follows exactly TirParams::new's current no-row-source path:
- sizeof(TirParams) root, one outer Vec<Vec<Option<ParamData>>> over P;
- per-declaration exact vec![None; width] Option<ParamData> slots, width=max(L,1)
  for per-layer and1 global, including unread layer slots;
- per_layer Vec<bool>(P), ranges outer Vec<Vec<OnceLock<Option<Interval>>>> and
  each inner range-slot collection. Public shared growing envelope is used for
  the actual collect/growing paths. rows=None adds no TirServed heap.
Successful insert moves owned data and replaces an empty OnceLock, no payload
allocation. Startup compile/refine/range scanning is not silently covered.

Core inventory visitor invokes its instance projection twice sequentially: count
then visit. One peak envelope covers exact readers Vec<BTreeSet<u8>>(P), actual
Ref::Param reference count times public BTree node envelope, outer instance Vec,
exact global [None], and per-layer Option<u16> collections bounded by L. Duplicate
references overcount BTree memberships conservatively. Each projection requires
work admission; no unquoted Core count/index constructor is called first.

Frontier has sizeof(public frontier) plus shared growing envelope of depth
floor(log2(leaves))+1 Option<Hash64> peaks; no dense leaf/hash tree is allocated.
Fixed semantic hash objects are typed sizeofs for leaf hasher, node hash State,
finalized blake2b Hash, four Hash64 values (incoming/carry/sibling/result envelope)
and current row coordinate. These are logical semantic object requests, NOT a
claim about every compiler-optimized/native crypto/OS call-stack frame or RSS.
No unexplained fixed scratch constant absorbs unknown heap allocations.

Retained current native target = sizeof(private owner, containing Params root)
+ Params heap metadata + TOTAL target owned payload. The current tensor is a
subset of that total, not another full-payload summand. Peak adds one chunk,
visitor metadata, frontier and named hash objects. Original source/program/header/
class/verified/plan/context owners remain external; all previous returned owners
are cumulative retained claims. max_native_bytes applies to the whole current
new-owner peak, max_operation_bytes adds external and prior claims. Metadata
limit includes Params root+heap plus visitor metadata (overlapping retained fields
are informational, not added to retained twice).

Budget starts each attempt BEFORE nested metadata scans, with a checked closed
form reference/tree/param×layer visit envelope using actual cardinalities and
existing structural maxima. Work currency then adds source bytes, native decoded
elements, re-encoded piece/header bytes and conservative internal hash bytes.
It is not CPU instructions, elapsed time, pricing or model work. Total attempts,
read bytes, hash leaves, work and retained claims never reset/refund. A rejected
admission is atomic; already accepted earlier metadata charges remain after later
quote/guard/read/root refusal. Guard and native allocations follow both admissions.
A refused guard therefore leaves conservative retained/read/work claims, by design.

## Four source tests (UNRUN)

A small real temporary PALWTIR1 file uses independently hand-written signed I8,
high-bit Idx and I64 endpoint bytes. Expected artifact root is built from literal
raw rows using Core leaf/frontier; loader hashes the actual newly owned values.
No model executes. Synthetic coherent execution-root fields establish only the
pre-admitted metadata association for the test; no registration/credit is claimed.
All trusted test decode/compile/space creation and source copies are outside this
component and within an explicit conservative test external inventory.

Tests cover declared and undeclared layout, actual job tokenizer0 vs class18,
owned native endpoint values/root, local optional guard held then released on
owner Drop, cumulative attempt refusal without refund/more guard calls; file-root
mismatch and physical truncation release the guard with no partial owner; exact
payload/tensor/chunk/metadata/native/leaf/read/work/operation caps and each one
below. Before cap refusal the descriptor seek position must remain unchanged.
No global memory-reserver OnceLock is modified; tests pass a local existing-type
reserver to avoid cross-test interference. Tests are not allocator measurements.

## Not completed by this component

No startup program admission/decoder, complete compiler quote, NF scratch getter,
compiled plan authentication, source mutation hook integration, child startup
loader, generic model process protocol, lazy-row provider, native registration or
backend dispatch is implemented. No false completion claim is made. Before any
integration: A0/A1 policy API review, producer quote/visitor/Frontier audit,
independent source review, then explicitly authorized isolated typecheck/tests.

## Getter and admitted-association review clarification

The params getter is now `&TirParams<'_>` tied to self borrow. The private owner
itself has no Clone or move-out getter; that does NOT prohibit public TirParams
Clone or slice-derived exports. Every arbitrary deep copy/export belongs to the
caller's separate reservation/external inventory. This owner's guard follows
only its own retained original payload, never automatically a clone.

check_pins is explicitly only literal consistency checking of independently
admitted expected/verified/original-context association. No unquoted fresh
CanonicalWriter hash/class-ID computation was added. A future Loaded entry must
establish that entire association through one retained admitted owner before
this subset is used; this patch does not replace that missing startup authority.

## Fourth source regression: native I16/I32, per-layer, multiple G14 pieces

Added a separately declared normal-form wide fixture with one repeated layer
block and I16 per-layer [2,16385], I32 global [8193], and I64 endpoint logits.
Each I16 row is32770 bytes (32768+2); two rows and two layer instances yield8
leaves. The global I32 vector is32772 bytes (32768+4), yielding2 leaves; I64 adds1.
Expected Core-authoritative root and11 leaves are computed BEFORE writing the
candidate from independent native typed source literals, never from candidate
header/file bytes. No Tensor or full i128 mirror is used. Manual nonzero values
straddle each I16 piece/row boundary and the I32 ragged piece. The test checks
entire owned native slices, endpoints, root,11 leaves,32768-byte reused chunk,
163868 total read/payload bytes, largest tensor65540 bytes, producer Params
metadata quote and2+1+1 layer slots, cumulative work, guard lifetime and original
max_context16/class-tokenizer18 versus actual-context-tokenizer0. Original layout
tiles32 permit a finite1<<16 step cap; no registered program/model is rewritten.
All four test sources remain UNRUN; formatting/applicability is not type-check
or allocation/process/model evidence.
