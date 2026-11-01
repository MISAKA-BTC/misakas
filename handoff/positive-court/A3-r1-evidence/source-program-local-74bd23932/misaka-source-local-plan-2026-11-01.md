# Core local Source program scan/decode — design draft

2026-11-01. DESIGN ONLY. No repository API, source freeze, compilation, test result, native reservation or activation is established by this document. The preserved admission/evidence branches are outside this patch. Cargo, rustc, target and cache access remain on hold. Names below are proposed local Rust names, not wire/schema/domain assignments.

## Intended boundary

Add a Core-owned, explicit finite-limit, program-only scan/decode API. It reuses the actual existing TIR scanner and the Source normal-form allocation quote. It does not depend on process-global native switches and does not instantiate NativeRetainedV1, NativeDemandCheckerV1 or a native lease. Artifact must not acquire a dependency on consensus-core. The API returns data; canonical normal form is not registration, authenticated class/job assignment, computation correctness, model review or credit authority.

The scope is the original TirProgramV1 bytes and decode_canonical normal-form validation. It excludes range/cone/close sizing, StepSpace construction, full Source assembly, parameter/Merkle authentication, model execution and a guarantee about total validation CPU. Existing narrow/wide encoding and the Source quote's typed widths remain unchanged.

## Proposed exact public API

Proposed new Core module: `palw_tir_source_program_local_v1`.

```rust,ignore
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceProgramLocalLimitsV1 {
    pub max_program_bytes: u64,
    pub max_scan_steps: u64,
    pub max_nf_items: u64,
    pub max_native_requests: u64,
}

// Persistent owner-local DATA accounting, not a native lease/capability.
// No Clone/Copy/serialization, public counter fields or reset/refund operation.
pub struct LocalScanWorkBudgetV1 {
    limit: u64,
    charged_steps: u64,
    observed_steps: u64,
    charged_reader_bytes: u64,
    observed_reader_bytes: u64,
    attempts: u64,
}
impl LocalScanWorkBudgetV1 {
    pub fn new(max_total_scan_steps: u64) -> Result<Self, SourceProgramLocalErrorV1>;
    // Read-only getters; construction requires a finite, positive total.
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceProgramLocalQuoteV1 {
    program_bytes: u64,
    scan_steps_upper: u64,
    reader_bytes_upper: u64,
    nf_items: u64,
    source_native_requests: u64,
    diagnostic_requests: u64,
    owner_requests: u64,
    admitted_native_requests: u64,
}
// Read-only scalar getters for every quote field. No public constructor.

pub struct ScannedSourceProgramLocalV1<'a> {
    bytes: &'a [u8],
    quote: SourceProgramLocalQuoteV1,
    // Optional private NativeTirProgramShapeV1, only if the implementation
    // actually needs it after deriving the quote. Never export this as authority.
}

pub struct LocalSourceProgramOwnerV1 {
    program: misaka_palw_tir::TirProgramV1,
    quote: SourceProgramLocalQuoteV1,
}

pub fn scan_source_program_local_v1<'a>(
    bytes: &'a [u8],
    limits: SourceProgramLocalLimitsV1,
    work: &mut LocalScanWorkBudgetV1,
) -> Result<ScannedSourceProgramLocalV1<'a>, SourceProgramLocalErrorV1>;

impl ScannedSourceProgramLocalV1<'_> {
    pub fn quote(&self) -> &SourceProgramLocalQuoteV1;
    pub fn decode(self) -> Result<LocalSourceProgramOwnerV1, SourceProgramLocalErrorV1>;
}
impl LocalSourceProgramOwnerV1 {
    pub fn program(&self) -> &misaka_palw_tir::TirProgramV1;
    pub fn quote(&self) -> &SourceProgramLocalQuoteV1;
}
```

The scanner wrapper's private immutable borrow ties quote and consuming decode to the SAME bytes. Do not accept a caller-created numeric quote, an unrelated bytes argument in decode, a supplied ProgramInfo, callback, auth boolean or native capability. No DerefMut, public fields, unchecked from/into conversion, deep Clone implementation or automatic native-owner conversion. The immutable getter does not stop a caller allocating a separate clone; that clone is outside this local API's quote.

The quote is an admitted per-operation logical envelope, not a global concurrency reservation. Retaining the owner retains the provenance of the program/quote pair; it does not debit a global ledger. Simultaneous owners need a separate owner-local aggregate budget if the caller requires one. Retaining a preparation envelope after scratch has been freed is conservative and avoids pretending that a narrower retained-only bound has already been audited.

The budget is retained by the actual local owner across attempts and retries. `limits.max_scan_steps` limits one attempt's precharged envelope; the persistent budget also limits the sum of all attempted envelopes. Creating a new budget for each retry defeats that owner's cumulative policy and is not the prescribed calling flow. Its private counters are data accounting, not authenticated producer facts, a native role or a shared global capability.

Proposed public error is a fixed-size enum, without owned String or formatted diagnostics on the refusal path:

```rust,ignore
pub enum SourceProgramLocalErrorV1 {
    ZeroLimit,
    ProgramBytes,
    ScanSteps,
    NormalFormItems,
    NativeRequests,
    ArithmeticOverflow,
    UnsupportedCoverage,
    Encoding,
    NormalForm,
}
```

This error surface does NOT alone eliminate allocations inside existing scanner/validator errors. The diagnostic backend issue below must be resolved before promising before-allocation enforcement.

## Exact current scanner call graph and counters

Repository root for references: `/Users/wata/.codex/worktrees/positive-court-a1/MISAKA-testnet`.

`palw_native_resource_v1/ledger_expansion.rs:582` (`source_tir_program_shape_v1`) does:

1. `standalone_tir_program_shape_v1(bytes)` at line604. That function (558–577) creates a stack u32 length prefix plus `Cursor(prefix).chain(bytes)`. Its outer reader is framed at n+4, and `program_blob` invokes `nested_blob` (234–244), which creates a child reader framed at n over the original payload.
2. The standalone function then calls `step_space_shape_v1(bytes)` (460–528), creating a third reader over n bytes. This derives actual blocks/nodes/layers/state geometry without decoding a program.
3. Source then creates a fourth reader (605–606), rescanning the raw TirProgramV1 grammar to obtain the parsed-payload quote.

Thus there are FOUR reader instances on the successful current path: outer n+4, nested n, StepSpace n, raw Source n. They are not one shared budget. With the current intrinsic reader limit

```text
L(x) = checked(8*x + 4096)
scan_steps_upper = checked((L(n+4)+1) + 3*(L(n)+1))
                 = checked(L(n+4) + 3*L(n) + 4)
reader_bytes_upper = checked((n+4) + 3*n) = checked(4*n + 4)
```

`step()` increments its counter BEFORE checking whether it exceeds the limit (wire.rs60–64). A failed budget check can therefore leave a reader at L(x)+1. Charge that failing-step allowance for all four potential readers unless a stronger control-flow proof is separately established. The old increment/refusal behavior must stay unchanged.

Admit `scan_steps_upper <= max_scan_steps` BEFORE any scanner pass. Also atomically check and charge the full envelope against `work.limit - work.charged_steps` before the first scanner pass. Charge the companion reader-byte envelope and attempt count in the same checked update. This is a conservative sum of the four readers' possible successful-and-failing step counts; it is not a measurement of steps actually consumed. `NativeWireReaderV1::new` currently uses saturating arithmetic (wire.rs49–52); the new local precharge uses checked arithmetic and the format cap, without changing old constructor behavior. Re-audit this formula if a scanner pass, nesting or limit rule changes.

The charge is NEVER refunded on scan failure, nf-item/request refusal, canonical-decode failure or successful early termination. A later retry uses the same persistent budget and incurs its own envelope. Preserve already accumulated counters on any overflow/refusal; do not increment one counter and fail before committing the others. A precharge refused before scanner entry performs no scanner work; fixed metadata/error work remains subject to the separately declared diagnostic boundary.

In addition, observe each pass's actual reader counters and carry them into the persistent budget on BOTH success and failure. This needs an additive `NativeWireReaderV1::steps()` getter (and a consumed-byte observation from initial length minus remaining), plus a thin owner-local observed port of the existing four-reader call graph. Observing only the outer reader omits the child scanner and subsequent readers. Do not use `?` to escape before recording a pass result: retain `let result = ...`, save the counter delta, drop/finish the reader, then propagate result. The nested child must similarly report before dropping it and returning to its parent. An RAII observer is an alternative only if its ownership/drop order is audited to capture every error return without allocation or a global/thread-local callback.

`charged_steps` is the safety bound and `observed_steps` is evidence. Observations do not refund or replace the precharge. Do not charge both full envelope and actual steps to the same remaining balance. Actual counters must not exceed their charged envelope; an impossible excess becomes a fail-closed coverage error and preserves the full charge. A getter alone does not implement this accounting: nested reader creation and all early returns in the local thin port still require explicit observation. Legacy nonobserved scanner behavior/pricing/error ordering stays unchanged.

`reader_bytes_upper` counts bytes traversed at NativeWireReader boundaries, including the nested payload once in the child and once in its parent. Observed reader bytes are ONLY `initial_remaining - current_remaining`, summed for the actual readers, on success or failure. In particular, a refused `skip(k)` whose request exceeds remaining does not consume or debit k observed bytes; it leaves remaining unchanged. Charged reader bytes remain the worst-case 4*n+4 and are never refunded on a failure. These quantities are NOT unique input bytes, filesystem IO, hardware reads or total CPU. `skip` has its own stack-buffer read loops (wire.rs67–83), and `read` itself does not call `step` (101–109). Public `max_scan_steps` therefore bounds the existing scanner step model only. Since `max_program_bytes` is finite and checked first, byte traversal is independently bounded by the above formula; expose it as a distinct quote field. If A0 requires a separately selectable read-work ceiling, that is an additional explicitly reviewed limit, not an undocumented interpretation of `max_scan_steps`.

`nf_items = shape.structural_items_v1()` (ledger_expansion.rs573–576) is exactly the actual scanner's `collection_entries + byte_entries`. Byte vectors, including schedule layers, participate. Strings currently have allocation accounting but do not add their payload length to byte_entries (wire.rs205–212); do not describe nf_items as every encoded byte or every validator operation. It is a finite structural cap checked before canonical decode, not a total normal-form CPU guarantee. Map ordering/comparison, constant comparison, repeated shape clones and diagnostic formatting require separate CPU instrumentation or a proved operation model before any stronger claim.

Normal-form validation CPU and subsequent plan compilation/startup CPU remain explicitly UNRESOLVED. Neither scanner precharge, nf-items, max_native_requests nor byte-format limits is a measured/certified CPU bound for those phases. This draft must not be described as bounding all compile work. No speculative universal validator-CPU formula is assigned here.

## Limits flow before decode

1. Reject every zero public limit with a fixed error. Convert bytes.len to u64 with checked conversion; require n <= min(max_program_bytes, MAX_PROGRAM_BYTES) and n representable in the scanner's u32 frame. Do not hash, clone, format or decode first.
2. Calculate the complete four-reader scan-step and byte-read bounds using checked scalar arithmetic; reject an over-limit scan step quote before scanning.
3. Resolve/admit a concrete diagnostic and owner framing envelope BEFORE calling a scanner backend that can allocate errors. This is presently an implementation blocker, detailed below. The quote must name its scope; no codec-byte-to-RAM scaling is allowed.
4. Call the existing complete Source scan/quote engine. It has no global native-switch dependency itself. Do not add an alternate hand-picked grammar or reprice the existing ledger path. Error classes must remain distinct in the new API even if public diagnostic detail is intentionally bounded.
5. Read nf_items and require nf_items <= max_nf_items before decode. Obtaining it currently also computes Source q; that successful quote arithmetic uses scalars and fixed loops, not a program/map allocation. If earlier rejection is desired, split an additive private facade around the same scanner output rather than alter legacy error ordering.
6. Compute `admitted_native_requests = checked(source_q + diagnostic_extra + owner_extra)` under the chosen exact diagnostic policy; reject values above max_native_requests before decode. Avoid double-counting diagnostic terms already in source_q. Do not silently substitute the much broader standalone program/gate request value for source_q, or remove its preliminary scanner/error behavior without review.
7. Return only the private same-bytes scanned wrapper. `decode(self)` invokes exactly TirProgramV1::decode_canonical(self.bytes); it must not take a replacement payload, limits or arbitrary user decoder. No fallback to an unbounded decoder.
8. Map canonical-decode failures through the audited bounded diagnostic path. Success moves the original program and quote into an immutable local owner. Original bytes are borrowed only through decode; retaining an additional raw bytes copy is not included unless separately quoted.

## What Source q already covers

`source_tir_program_shape_v1` (ledger_expansion.rs618–663) quotes actual nested parsed-program allocation, a second raw envelope for validation constant clones, inline TirProgramV1, canonical re-encoding, declaration and constant-key trees, BlockInfo, carry clones before output-shape validation, operand/inference shape temporaries, state-effect sets, liveness stack/maps, used maps/sets and four growing diagnostic writer envelopes. In particular it uses actual nf_items for malformed rank/perm lengths, rather than assuming MAX_RANK before those shapes are checked.

Typed request arithmetic is `TirNativeRequestsV1` (tir.rs28–63), with pinned Vec capacity growth/minimums. `BTreeNodeEnvelopeV1` (typed.rs192–204) includes map parent/metadata/slots/edges and alignment padding under the pinned Rust profile. It is not physical allocator arena overhead, RSS or admission by the system allocator.

New owner framing must be derived from its actual representation. The proposed owner keeps program and quote inline and adds no Box/Arc/Vec; inline `size_of::<LocalSourceProgramOwnerV1>()` is stack/return storage, not a new heap allocation. Source q already includes `size_of::<TirProgramV1>()`; if the public quote counts inline owner storage too, add only the separately named checked representation delta and state that convention. If an implementation changes to Box/Arc or a ticket/map, quote its actual allocation, refcount/alignment header and nested ownership BEFORE allocating it. Do not reuse a guessed NativeRetained header for a distinct local owner.

Full Source assembly is not covered by Source program q. Existing `palw_tir_public_source_v1::native_construction_requests_v1` (247–299) separately adds StepSpace, raw program ownership, two layout copies, three header/context owners, RUN output, retained owner and identity-writer requests. Layout, header, source maps, parameter inventory, Merkle scratch, downstream decode/cone/range analyses and concurrent owners remain outside this proposed API.

A3's Node startup owner must explicitly supply its own private typed coexistence quotes/arguments for retained Arc/program owners, plan compilation and retained plans, source/parameter/index maps, loaded weights and the Loaded owner. It must atomically check the real overlap of these owners with program decoding/scratch BEFORE the corresponding allocations. No public from-counts quote or caller-supplied scalar is authority for those allocations, and no new artifact-to-Core dependency is introduced. This Core program quote is one named component of startup; it is not all startup RAM, all compile CPU or permission to materialize the Loaded owner. The exact A3 Node port and aggregate lifetime accounting are outside this draft and remain required integration work.

## Diagnostic floor: unresolved before-allocation blocker

The current successful scanner path allocates no program/maps, but its errors are not allocation-free:

- wire.rs35–39 and63 create `io::Error::new` custom payloads from scanner messages.
- generated enum errors use wire_bad with the same custom IO-error path; the complete generated TIR grammar is wire_generated.rs12577–12640.
- ledger_expansion.rs590–600 converts IO errors with `to_string` or allocates NativeCoverageV1 error strings. TirNativeRequests overflow also builds NativeCoverageV1 strings (tir.rs33–55; typed.rs12 onward).
- decode_canonical and normal-form errors can allocate diagnostic strings after q is admitted. Existing Source q's four writer envelopes (ledger_expansion.rs654–661) are part of that successful-preflight preparation envelope, not a retroactive reservation for scanner errors occurring before q exists.

Do NOT invent a 4KiB/16KiB diagnostic floor or assert `size_of::<io::Error>() + message.len()` bounds IO-error heap ownership. `io::Error::new` may own a private boxed custom error; its representation and intermediate String/Box owners require a pinned implementation audit. Adding a blanket input-bytes multiplier is also not a valid replacement.

Recommended minimal port before implementation approval: a fixed-error `SliceReader` for the NEW local scanner path, paired with a legacy `Read` adapter for the same TIR grammar declarations. The slice reader borrows the original immutable bytes, holds scalar cursor/remaining/steps state, performs checked slice reads and reports a fixed-size error enum; it must not construct an `io::Error::new` custom box, NativeCoverage Strings or `.to_string()` result on that path. Preserve the old reader's increment-then-limit check, exact byte widths and consumed-byte observations. The legacy Read adapter must preserve current io::Error messages/order and native pricing.

The paired TIR grammar should be emitted from one field/variant declaration source (including the complete primitive/ref/type/program variants). Do not write a separate hand-maintained partial parser. This may be a reader/error-parameterized shared grammar engine, or paired generated implementations from the same schema; the minimal choice requires A0 review of the current generated scanner/macro port. Shared allocation quote arithmetic also needs a fixed-error local adapter before its overflow path can be called allocation-free. Existing legacy wrappers continue using their original error adapters, logical widths and evaluation order. The current public enum alone does not implement this port, and no repository patch exists in this draft.

Alternative, if A0 explicitly keeps current allocating errors: audit the finite reachable scanner error sites and pinned std/Borsh formatting/box representation, compute a checked maximum simultaneous diagnostic allocation using actual type/header sizes, static message lengths, Vec/String growth and live temporary count, then admit that concrete fixed floor before scanning. Source q diagnostics can cover later formatting only after demonstrating containment. No defensible numeric floor has been established in this source-only audit, so that alternative remains blocked; it cannot be implemented by choosing a magic constant.

Even a fixed-error SliceReader and paired static scanner do NOT make Borsh or normal-form errors static. `TirProgramV1::decode_canonical` still runs Borsh decode, re-encoding and validate with their existing error owners. Reuse the existing Source diagnostic quote only after auditing containment of the exact live diagnostic owners produced by ALL of those phases, including malformed input paths and formatting conversions. That decode/re-encode/NF diagnostic containment is a SEPARATE unresolved audit blocker if not proved. Static scanner success does not resolve it. This is logical request accounting; it does not guarantee recovery from process allocator OOM.

## Exact proposed patch ports (not applied)

1. New `consensus/core/src/palw_tir_source_program_local_v1.rs`: the four-field limits, fixed local error enum, private quote/borrowed scan/immutable owner and additive scan/decode functions. No NativeRetained/global checker calls.
2. `consensus/core/src/lib.rs`: additive module export only, after the diagnostic backend is resolved.
3. `consensus/core/src/palw_native_resource_v1/ledger_expansion.rs`: reuse the Source scan/quote entry and private shape getters, preserving current native/ledger wrappers byte-for-byte. If a static error backend is selected, extract shared scanner/quote mechanics behind distinct adapters; retain current wrappers and ordering as the reference oracle. Do not replace Source q with program_blob's broader four-analysis-pass profile.
4. `consensus/core/src/palw_native_resource_v1/wire.rs` plus generated TIR wire grammar template/tool owner: add the reviewed fixed-error SliceReader/legacy Read adapter paired grammar port and a `steps()` getter. Existing NativeWireReaderV1::new, wire format, legacy default step limits, failing-step behavior and error text remain unchanged. Precharge L(n+4)+3*L(n)+4; a local observed port must still capture actual counters for all four readers, including failed nested passes, before early-return propagation. Count only consumed bytes in observations; refused skip requests are not consumption. Do not expose a global budget toggle.
5. `consensus/core/src/palw_native_resource_v1/tir.rs` / typed quote primitives: no native ledger/rate changes. If static quote overflow errors need extraction, share arithmetic and typed capacities with a new local error adapter; do not alter existing NativeCoverage error order or widths.
6. New focused source tests colocated with the new module after implementation authorization: exact and one-below byte/step/item/request limits; zero/overflow; malformed trailing/version/tag/rank/constant/name data; immutable same-bytes scan/decode; wide/narrow original program equality; global-switch independence; refusal before decoder callback/allocator events with a controlled test hook; nested first/second/third/fourth-pass errors recorded before propagation; step-limit failure observes L+1 and fits the +4 allowance; refused oversized skip observes zero additional consumed bytes; scan/decode failures keep charged counters; retries spend the same persistent budget; independent observed counters never refund/double-debit the admitted envelope; paired fixed-error/legacy TIR grammar acceptance/count equivalence without changing old error order. Native authority/registration tests are out of scope. No builds have run for this draft.

No change to artifact Cargo dependencies, native activation, ledger/currency rates, legacy request pricing, class IDs, wire widths or model tiers is proposed. No repository file has been changed to prepare this document.
