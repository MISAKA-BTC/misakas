# Source-only program normal-form profile — candidate, unbuilt

`source-program-profile.patch` applies to current shared tree (`git apply --check` passed). It changes three files: private ledger expansion helper+tests, crate-private reexport, and only Source.native_construction_requests' program-profile selection. No repository edits, cargo/rustc or execution performed.

## Boundary

Old generic `program_blob`, standalone Program/Byte owner and existing broader program_requests_v1 remain unchanged. New helper still obtains the opaque existing full scanner/metadata and independently scans actual unexpanded TirProgramV1 wire shape allocation-free. Unknown version/enum/bool/trailing bytes refuse; UTF8 validation remains actual Borsh. Native Source alone selects the codec+normal-form profile, because its exact constructor performs canonical decode + structural validation, not admission gate/range/cone/unrolled analyses. Numerical/parameter owners still retain their existing broader analysis budgets.

The detailed call/lifetime audit is `/tmp/g14-source-program-phase-audit/PHASE-AUDIT.md`. SourceID, constructors, limits, validation, values, resource demand mode and semantic errors are unchanged. This helper supplies no paid/complete/material availability authority.

## Conservative coexisting inventory

Let n=actual wire collection+byte elements, B/N/S=actual blocks/nodes/states. n is deliberately a LINEAR fallback bound on any malformed rank, shape or primitive perm vector, not an assumed already-valid MAX_RANK. All arithmetic is checked.

- 2×actual unexpanded wire heap: parsed program plus a full raw payload envelope covering validator's retained cloned constant shapes/data (other raw payloads need not actually clone; this is conservative). Inline parsed Program and canonical reencoding 2*max(bytes,1024) separately.
- declaration tree entries: n+S name tree nodes, n constant tuple tree nodes; covers still-live parameter names/seenconstants/state names.
- growing BlockInfo vector B.
- retained carry and one layer carry-out list: 2*MAX_CARRY TensorTypes, each conservatively up to n shape dimensions. Node ranks are not assumed validated here. All block carry counts are checked <=MAX_CARRY BEFORE these allocations.
- node inputs: MAX_NODE_INPUTS TensorTypes and that many rank-n owned shapes. Arity checked before allocating operands. Four additional growing shape Vecs sized 2*n+1 cover Gather's concatenated inferred rank, Select overlapping broadcasts, HistAppend's row/fixed/H vectors, and Transpose scratch bool Vec separately.
- written/appended sets: 2*S u16 tree entries.
- liveness: bool Vec N; DFS usize Vec N+n+MAX_CARRY+1. Each node's inputs expand only once after marking live; all pushes arise from roots or one actual input edge. This is a linear bound; no cone is constructed here.
- used map: three outer tree nodes, inner u16 trees plus one cloned inner set: 2*n tree envelopes.
- error writers: Dim Debug is at most `Fixed(4294967295)` plus delimiter (<20 bytes/element). A failed Gather inferred shape has <=2*n dimensions and declared shape <=n, giving <=60*n variable bytes. Transpose raw perm debug is <=5*n. Duplicate names are checked <=128 bytes before formatting. 1024 covers current fixed literals/kind/index widths. Four separate growing writer envelopes on 1024+60*n cover nested message/error Display copies while preserving existing public messages. This bound must be reviewed whenever validator diagnostics or Dim grammar change; it is not a general Rust Debug byte bound.

The inventory intentionally sums several mutually exclusive temporaries. This initial candidate prioritizes a justified conservative linear bound over minimizing every remaining byte. It never uses the observed 26GB as an allowed target or a measurement-derived multiplier.

## Tests / scope

Suggested filters: `source_program_profile_tests_v1`, existing `native_source_owner_tests_v1` (including isolated child workers), StepSpace inventory tests, Prefix native material, t12 identity and activation as parent requires.

New tests:
- two actual fixed/history executable fixtures: profile quota q-1 Resource before decode; q exact decode owner retained, second reservation refused while held, drop/re-reserve succeeds; byte-identical canonical program. q<generic requests is expected because Source excludes gate/cone/range, not a physical memory correctness assertion.
- valid-wire malformed shape rank4096 and duplicate pre carry indices8: local preallocation refusal at q-1 and actual normal-form Decode at q, with exact checker release. Native constructor is not called on an imaginary accepted registration; this explicitly tests its real decode subpath and not full Source admission.
- actual ProgramBuilder 128 Add nodes each referencing the same prior operand twice: successful real canonical decode exercises repeated liveness edges; adding a dead noncommit actual node yields native dead-node refusal and releases quota.

Existing real Source owner tests automatically use the new profile and test full Source identity/geometry and quota/lease refusal. New malformed tests do not replace those full constructor tests. The local test wrapper's q-derived capacity measures accounting behavior, not independent physical peak correctness or allocation instrumentation. No benchmark or door completion claim is made.

## Remaining review risks

- Whole failure-path writer inventory depends on current validate/Dim/Display implementation; independent read review requested before application.
- Broad scanner still runs allocation-free to preserve metadata authority; it computes its broad old quote without reserving it. Its historic io error classification is not globally repaired. New Source-local request overflows/reservations and wire OutOfMemory/TimedOut/Unsupported preserve Resource without string classification; malformed InvalidData remains Decode. The generic mapper is unchanged.
- Downstream registered numerical/parameter analysis genuinely has cone/range costs and must not adopt this Source-only profile just because a Source was constructed.
