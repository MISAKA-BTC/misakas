# Actual producer compile quote — source-only draft, 2026-11-02

This REPLACES the old always-Unsupported dependency stub. It is an actual checked
allocation-request implementation with successful quote/preparation/compilation
paths for the existing canonical narrow and wide programs. No model arithmetic,
model qualification, registration or computation-credit decision is performed.

Inspected base: 9b3216719b0b567a1c4aff08ccb62f9f0a866006 (the actual owned-parameter
component freeze). Only /tmp artifacts were written. Worktree/Core/compiler source
was not changed. No Cargo, rustc, model execution, compilation or test was run.
File-only rustfmt parsed/formatted the draft; git apply --check passed. These are
syntax/applicability checks, not type-check or execution evidence.

Deliverables:
- /tmp/a3-startup-plan-quote.patch
- /tmp/a3-startup-plan-quote.notes.md (this file)
- /tmp/a3-tir-plan-compile-quote-v2.rs (formatted new-file contents for review)

## Scope / dependencies

The patch adds only cfg(node) module+pubuse declarations next to the actual
producer in exec/src/plan.rs and new plan/compile_quote_v2.rs. The existing
TirPlan::compile body and every legacy method remain BYTE-UNCHANGED. No default
backend, native flag, wire/domain/ID, global checker, host lease, unsafe cast,
parameter table, parameter source, metadata normalization or public model policy
changes. There is no new Core dependency: Exec node already depends on Core;
Core does not depend on Exec or artifact. Artifact gains no Core dependency.

Requires the actual new Core LocalSourceProgramOwnerV1 API and public
 growing_vec_request_capacity_v1<T>(u64)->Option<u64> (9c010109d) plus public
btree_node_envelope_bytes_v1<K,V>(). Only an actual immutable Source owner is
accepted, never a caller-created quote, ProgramInfo, mutable TirPlan or decoded
program typed object with an unaudited provenance. That Source owner can only be
constructed after Core's private cfg+build-env supported-profile gate: audited
Rust1.93.0, Borsh/derive1.5.1, supported64-bit little-endian Linux/macOS targets.
No public boolean/callback can assert profile/authentication support here.

V1 byte encodings, original wide/narrow scheme and all program values stay literal.
The compile quote does not take, rewrite or manufacture a class/context/tokenizer.
The registered18/actual-job0 distinction is unaffected. This is resource/syntax
safety only; hashes/commitments in the surrounding system establish data identity,
not model computation correctness or consensus credit.

## API / startup order

Public at misaka_palw_tir_exec::plan (cfg node):

    tir_plan_compile_quote_visits_upper_v2(&LocalSourceProgramOwnerV1)
        -> Result<u64, TirPlanCompileQuoteErrorV2>
    quote_tir_plan_compile_v2(&LocalSourceProgramOwnerV1, TirPlanCompileLimitsV2)
        -> Result<TirPlanCompileQuoteV2, TirPlanCompileQuoteErrorV2>
    prepare_tir_plan_compile_v2(&LocalSourceProgramOwnerV1, TirPlanCompileLimitsV2)
        -> Result<PreparedTirPlanCompileV2<'_>, TirPlanCompileQuoteErrorV2>
    Prepared.quote() -> &TirPlanCompileQuoteV2
    Prepared.compile(self) -> Result<LocalCompiledTirPlanV2, fixed error>
    LocalCompiled.quote() -> &TirPlanCompileQuoteV2

The caller's cumulative startup budget charges the pure O(1) quote-visits upper
BEFORE prepare, including failed attempts/retries. Prepared holds an immutable
borrow of the exact Source owner and its privately produced scalar quote. Between
prepare and consuming compile, the Node startup owner obtains its actual host
reservation using that complete quote. compile then calls the unchanged producer
EXACTLY ONCE on the same borrowed source.program(). No replacement program/quote,
user decoder, policy callback or host-authority boolean is accepted.

LocalCompiledTirPlanV2 owns a PRIVATE plan and quote and keeps NO Source borrow:
Loaded can therefore own Source + compiled plan as independent fields, with no
self-reference. It is neither Clone nor Deref and has no raw public plan accessor
or into_plan conversion. The raw plan() accessor is pub(crate) for the same-Exec
Loaded/Prepared integration only. This matters: TirPlan exposes its fused OnceLock
and fused_regions() can allocate through &TirPlan. A public shared-plan accessor
would still allow interior payload injection/lazy allocations. Trusted same-crate
bridges must not mutate the plan or initialize later fusion stages without their
own separately admitted contract. The existing legacy TirPlan public API remains
unchanged; this wrapper does not try to restrict independent legacy callers.

LocalCompiled is logical data, not a NativeRetained or host lease. The actual
Loaded owner must attach and retain its actual aggregate guard until plan, source,
header, parameter payload and other guarded owners have dropped. This quote alone
does not implement that aggregate owner, source binding or native registration.

Limits: max_quote_visits, max_retained_bytes, max_scratch_bytes,
max_operation_bytes, external_retained_bytes. First four must be positive; all
arithmetic is checked, fixed enum on refusal. external is pre-summed independent
header/File/class/context/verified/prior-export/previous-plan ownership, EXCLUDING
the current Source owner because its complete admitted envelope is included
explicitly below. The header/read/source stages remain separately admitted.

## Logical coexistence formula

S = source.quote().admitted_native_requests(), with immutable producer provenance.
R = sizeof(LocalCompiledTirPlanV2) + all listed retained compiler heaps.
V = ONE ADDITIONAL S for the actual unchanged validate(program) call in compile.
O = second occurrences Vec root+element request.
G = maximum whole-constant Gather i128 slab + usize-shape root/backing.
T = one occurrence's state-touch tree root/backing.
W = maximum block's row-site temporary roots/backing.
F = actual Prepared borrow coordinate and Result representation deltas.
X = independent caller external_retained_bytes.

    scratch = V + O + G + T + W + F
    operation_peak = S + R + scratch + X

This deliberately SUMS named temporary phases even when they do not coexist. The
second S contains the same known Source preparation profile (including parsing
and diagnostic/owner terms not all needed by validate-only); it is a named
conservative revalidation envelope, NOT a fake second parse, private quote policy
copy, encoded-size multiplier or omission of the second validator. Original S
remains separate while the compiler result is built. ProgramInfo retained heap is
also explicitly inventoried in R even though the full V overbounds it again.

Source q's existing four diagnostic byte writers are not newly reinvented here;
V reuses the official successful Source owner envelope including the new typed std
error extras. A repeated validator with the SAME immutable normal-form program
cannot generate a semantic error in current code, but its successful NF allocations
and original fixed-context diagnostic Strings STILL execute and are funded.

R includes the actual private plan+quote root, not sizeof(TirPlan) alone. Future
Loaded wrapper/header/guard bookkeeping is NOT hidden in this root and must be
added by that actual producer. Temporary scalar Rust return/move optimization and
realloc-internal/allocator/OS/compiler stack frames are not an RSS guarantee.
Independent Rust-owned Vec/String/tree allocations are explicitly inventoried.
There is no OOM recovery promise: unchanged compile uses ordinary allocators.

## Line-by-line producer allocation audit

All repository references below are in the inspected worktree:
/Users/wata/.codex/worktrees/a3-panel-positive-court/MISAKA-testnet.
Notation E<T>(n)=checked n*sizeof(T), G<T>(n)=the actual public9c growing-capacity
request times sizeof(T), Tree<K,V>(n)=public pinned BTree node envelope times n.
G is conservatively used for exact clone/collect paths too. These clones copy
visible elements rather than the caller's spare capacity. No program encoding,
private NF quote reconstruction, generated tree or input-side metadata clone
occurs during the quote itself.

| Producer site | Typed retained or scratch coverage |
|---|---|
| plan.rs295-296 validate | V=one full actual Source admitted envelope, no changed validator/order; info retained below. |
| validate.rs449-450 and468 | R: G<BlockInfo>(blocks), G<TensorType>(pre.carry_out.len), each referenced original out.shape G<Dim>. Original shapes/ranks read, no MAX_RANK substitution. ProgramInfo root is inside plan root. |
| plan.rs298 / program.rs183-191 | R: G<(u8,Option<u16>)>(L+2) original schedule occurrences. |
| plan.rs299 / program.rs196-204 | R: G<u32>(L+2) slot bases; O: second Vec<(u8,Option<u16>)> root+G(L+2), while first occurrence Vec survives. |
| plan.rs300-304 / elem.rs235-244 | R: G<Buf>(consts) plus per-const typed G<i8/i16/i32/i64/i128/u32>(elements). Result collection adds no second payload vector. Width/element division checked; successful canonical const data cannot fail Buf construction. |
| plan.rs305-307 | R: G<BlockPlan>(blocks), each G<NodePlan>(nodes), copied carry-in G<TensorType> + G<Dim> shapes, carry-out G<u16>. |
| plan.rs191-209 | R: per NodePlan G<TensorType>(inputs), G<Interval>(inputs), and actual rank G<Dim> for each original Node/CarryIn/Param/Const/State ref; Input rank0. No borrowed/refined/mutable plan data is trusted. |
| plan.rs210 / ranges.rs117-235 | node_facts is Copy scalar intervals/arrays, no heap container; all NodePlan Facts/Work/Acc/check/store fields are inline in NodePlan's typed element request. |
| plan.rs213-216 / plan.rs166-181 | G: per actual Const-first-input Gather, usize shape Vec root+G(rank) and whole Buf.to_i128s Vec root+G<i128>(ALL constant lanes), maximum across calls. Does not quote only selected/output rows. All previously built blocks/native consts/current operands survive in R. |
| plan.rs224-226 | R: NodePlan cloned Prim (exhaustive enum: only Transpose has G<u8> perm), cloned G<Ref> inputs, cloned output G<Dim> shape. No output lane tensor exists during plan compilation. |
| plan.rs309-313 | reads_token scans refs only, no collection. |
| plan.rs315-317 | R: instance_of G<Vec<Option<u32>>>(states), exact E<Option<u32>>(max(L,1) per-layer or1 global) for EVERY declaration slot, including unread layers. |
| plan.rs318-350 | R: G<StateInstance>(sum declared slots), per-state-slot G<usize>(shape rank), Tree<(u16,Option<u16>),()>(sum parameter slots). Full declared slots safely overbound sparse actual touched instances; no dynamic state value tensor is allocated. T: one BTreeSet<u16> root + Tree<u16,()>(states). StateWrite/HistAppend effects are included; no outside-state source is read. |
| plan.rs355-367 | Full program.clone heap: G<ParamDecl/ConstDecl/StateDecl/Block/Node>, all declaration/block String bytes, static shape Vec<u32>, constant byte Vec<u8>, every carry TensorType+Dim shape, every node Prim/Ref/out shape, every carry_out Vec<u16>, schedule layer Vec<u8>. Program inline root is already in opaque plan root. Native const payloads and this clone coexist with the original Source program. |
| plan.rs366 | fused OnceLock starts EMPTY, root in TirPlan; no future Region Vec is charged or allowed to appear under this quote. |
| tiers.rs217-221 | R: row.gathers G<Vec<Option<TirRowGatherV1>>>(blocks), exact per-node Option arrays; row.groups G<Vec<TirRowGroupV1>>(blocks); dense exact Vec<bool>(params), ordinary Rust bool elements. |
| tiers.rs245-262 | R: per-block group G<TirRowGroupV1>(candidate axis0/no-batch Gather nodes). Transparent parameter origin is NOT guessed/accepted as authority: counting all candidates is conservative. Group member heap uses explicit partition bound below. |
| tiers.rs226-239 | W: view Vec<Option<u16>> root+exact per-node array; carried BTreeSet<u16> root+Tree(carry_out.len); its FromIterator input Vec<u16> root+G(carry_out.len); by_index BTreeMap<Ref,usize> root+Tree(candidates). All prior rows/program/blocks survive in R. |
| tiers.rs165-183,205-212,245-249 | W: Reshape fixed_dims Vec<u64> root+G(output rank); Gather data/index TensorType roots + respective G<Dim> ranks + data fixed_dims Vec<u64> root+G(data rank), conservatively all summed for one candidate. Handles Option-collect partial failure before an H, not just fixed successful shapes. |

Group members: for groups with sizes m_i>=1 and M=sum m_i, the shared policy gives
SUM G<Pair>(m_i) <= G<Pair>(M) + groups*G<Pair>(1). Both groups and M are at most
candidate Gather count. Thus the quote pays G<Pair>(candidates) +
candidates*G<Pair>(1). A single flat G(total_members) alone could miss many tiny
per-group minimum-capacity requests. This algebra uses the exact public policy,
not a guessed byte factor or a separate private growth rule.

BTreeSet::from_iter's hidden input Vec is a real additional standard allocation.
Pinned1.93 set.rs1410-1420 collects then stable-sorts input. Canonical MAX_CARRY=8
(program.rs41) is below the stable sort's <=20 insertion-sort path; no heap sort
buffer occurs for this call. Root bulk construction uses node allocations already
covered by Tree; input Vec and tree may coexist and BOTH are counted. This is a
source-pinned std fact, not a general BTreeSet allocation claim.
[official Rust1.93 set source](https://github.com/rust-lang/rust/blob/1.93.0/library/alloc/src/collections/btree/set.rs#L1410),
[official small stable-sort path](https://github.com/rust-lang/rust/blob/1.93.0/library/core/src/slice/sort/stable/mod.rs#L70).

## Work / CPU limits

The O(1) prequote returns8*(nf_items+1), checked. Its8 named linear metadata
families are explicit in code. Source nf_items bounds collection/byte entries;
quote only reads lengths/ranks/indices and does not traverse arbitrary String/data
contents. This is a logical visit currency for the allocation-free quote walk,
NOT the compiler's total operations or CPU instructions/time.

Additional returned partial dimensions: occurrence_reference_visits upper
(nf_items+1)*(L+2); const_interval_lanes sums each actual const Ref's entire lane
scan in ranges::leaf_interval; const_slab_lanes sums each Const Gather's complete
slab lane count. Slab widening and the following traversal are distinct actual
loops over this same named lane dimension. A caller may charge an explicitly
stated multiple/operation model if it wants both visits, not label it a calibrated
CPU cost. Tree/string comparison costs and all NF CPU remain NOT certified by
these fields. No all-NF, all-compile CPU/time, kill/wait, RSS or allocator success
claim is made. The surrounding startup owner must honestly retain that limitation.

## Three new source tests — UNRUN

1. Actual prepared.compile path on narrow/wide x repeated-layer/no-layer normal
   form. Handwritten constant [-3,4,5,6] and Idx [0,3], two transparent parameter
   gathers sharing one group, transpose, Const Gather whole4-lane slab, global
   fixed/history writes and repeated-layer state. Asserts actual compiled native
   constants, exact row group count1/members2, exact state instance count2/4,
   program equality, empty fused OnceLock and attached quote. No model executes.
2. Exact limits then each one-below quote-visits/retained/scratch/operation cap;
   refusals occur in prepare before invoking unchanged compile. Tests logical caps,
   not physical allocator measurements or a fabricated host guard.
3. Shared Vec/tree typed checked overflow, zero-count behavior, no String error.

Unknown Source-profile refusal prints an explicit UNSUPPORTED label; success tests
then do not claim compile/quote success. Root must distinguish this branch from
actual supported-profile execution. No tests/compile have been run in this task.

## Remaining integration / not completed

This patch is NOT generic artifact startup completion. LoadedWideArtifact must
pre-admit framing/header/Source phases, literal class/context/root association,
same-descriptor source stamps/mutation checks, parameter tables/owned payload
read and authoritative G14 authentication, all co-live stage reservations,
cumulative work/read/retry budgets and actual host guards. A query to discover
Core Source-profile support before early header framing is a separate Core-owned
draft dependency; this quote itself requires an already constructed Source owner.

A future Loaded wrapper may own File+bounded header+Source+LocalCompiled+params,
borrow original admitted expected/verified/context and offer a carefully bounded
Prepared closure. It must retain no self-referential plan borrow, expose no raw
mutable/interior-mutable plan to external callers, and keep params/plan before its
actual guard on Drop. Caller-owned clones/exports and later fusion/refine/executor
workspaces need separate reservations. None of those are silently funded by this
compile quote or its inert logical owner. No old API/flags/IDs/pricing is changed.
