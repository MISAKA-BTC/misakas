# A3 bounded PALWTIR1 header — source only, not compiled

Base code: Hello2 `1b14d1ecc`; source-review evidence `9c4e21f56`. The reviewed header patch is applied on separate branch `codex/a3-wide-artifact-startup-v2`. No Cargo/type check/test execution has occurred. Earlier Hello2 sources and Root validation candidate remain separate. The process-owner patch and LoadedWideArtifact sketch below remain unapplied drafts.

## Patch scope

The patch adds artifact::bounded_v1 and one module declaration only. Existing PALWTIR1 bytes, tags, IDs, container/default loader, compiler and runtime semantics are unchanged. It is an opt-in metadata parser, NOT a replacement artifact loader, verified source, program decoder, model eligibility decision, host/RSS lease, or generic process implementation. Core is not added as an artifact dependency.

- read_header_bounded_v1 reads the existing 14-byte prefix and header using the caller's already-open descriptor and its file length. It caps file/header/native-root/parser work/joint operation backing BEFORE reserving the raw header frame.
- decode_header_bounded_v1 reads ALL existing header fields directly: version u16, program Vec<u8> (u32 length), layout Vec<u8>, tokenizer [u8;64], tensor table Vec<Entry> (entry param u16, layer Option<u16>, offset/bytes u64), meta UTF-8 String (u32 length). No generic Borsh deserialize is called anywhere in production draft code.
- Every owned Vec is exact-reserved after checked copied-element bytes, cumulative native bytes and joint [frame + native +8192 + external] checks. Header root includes every inline Vec/String descriptor. Layout remains raw retained bytes; PalwTirLayoutBytesV1 is an allocation-free borrowed view of existing version1 scalar fields and bounded u32 word arrays, NOT a new serializable DTO. Empty layout remains None; no defaults are invented. Scalar values and tokenizer bytes are never rewritten.
- validate_with_program borrows the caller's ALREADY canonical-decoded, normal-form-validated TirProgramV1 and compares its canonical serialized bytes to the literal header through a comparison writer. It does not re-decode or clone it. It preflights finite metadata/P*L*reference scanning work, then checks exactly the old param_instances_v1 inventory order (including one instance for each global parameter), declared byte lengths, alignment, non-overlap and file bounds without expected/got Vecs or a BTreeMap. PalwTirCheckedHeaderV1 is a borrowed table view with binary-search locate(). The Core program owner/lease must outlive it. This function does not independently establish full program validity.

Program/header/layout format caps are existing MAX_PROGRAM_BYTES, MAX_BLOCKS*MAX_NODES_PER_BLOCK, MAX_STATES, MAX_PARAMS*MAX_LAYERS plus explicit local limits. Provenance is preserved as UTF-8, capped locally, and has no identity role. Parser work is symbolic 128*(header+14+1), not CPU instructions. max_copy_bytes covers retained Vec element storage (including tensor headers); raw frame is counted separately in max_header_bytes/operation peak.

## Source-only regression draft (not run)

Included tests: exact native/copy/joint-peak and each one below; hostile program/header lengths (file cap before prefix and header cap before body allocation/read) and truncation; empty tensor table checked against separately decoded no-param program; literal tokenizer/provenance and empty-layout preservation; borrowed layout word arrays plus hostile word count; original program mutation refused. Tests may need API/type-level fixes after an explicitly authorized compile. They do not establish allocation measurements or whole-model readiness.

## API/ownership follow-up: LoadedWideArtifactV2 (NOT implemented by this patch)

Proposed node-layer shape (names below are dependencies/proposals, not claimed existing APIs):

    struct WideArtifactStartupLimitsV2 {
        header: PalwTirHeaderLimitsV1,
        max_plan_bytes: u64, max_params_metadata_bytes: u64,
        max_owned_param_bytes: u64, max_source_read_bytes: u64,
        max_inventory_leaves: u64, max_startup_work: u64,
        max_startup_peak_bytes: u64, external_retained_bytes: u64,
    }
    struct LoadedWideArtifactV2 {
        // Drop order: params before backing; all tickets until last dependent owner.
        // Exact fields depend on existing artifact integration; do not recreate unsafe static borrows.
        artifact: TirArtifactV1,
        expected: PalwTirStepBindingV1,
        verified: CoreRetainedVerifiedWideBindingV2, // DEPENDENCY: explicit local Core API pending A1.
        startup_ticket: StartupReservation, // DEPENDENCY: owner-supplied logical/host reservation contract.
    }

    fn quote_startup_from_header_v2(header, core_program_quote, actual_counts, limits) -> StartupQuote;
    fn open_expected_v2(file_descriptor_or_path, independently_rooted_expected,
                        expected_execution_hash, limits, reservation_provider) -> LoadedWideArtifactV2;
    fn with_prepared_v2<R>(&self, io_limits,
        body: impl for<'a> FnOnce(PreparedWideCellV2<'a>) -> Result<R, String>) -> Result<R, String>;
    fn check_source_unmodified_v2(&self) -> Result<(), String>;

Do not apply this sketch until Core API is exact. A1/A0 is drafting explicit-local bounded source-program quote/retained decode; existing Core source_tir_program_shape_v1 is crate-private. The current public decode_native_tir_program_v1 is conditional on global native/logical admission and is not the desired explicit-local startup contract. The program owner must be retained, or its value AND lease transferred together via NativeRetainedV1::into_parts_v1.

Required construction sequence:
1. Open one descriptor, obtain file length/stamp; bounded header read. Keep raw frame/decoded header/Core input owners in the joint startup inventory while alive.
2. Core explicit-local scanner/quote admits nested decode + normal-form validation before decoding; retain ticket. Use validate_with_program for exact literal header-program/table binding. Copy the borrowed existing layout into the Core layout only after quote (or keep borrowed fields for comparison); preserve all values.
3. Before TirPlan::compile / TirParams::new / container index / residency construction, derive checked native/copy/work envelopes from actual decoded counts, including params*layers tables, duplicated program/constant/type metadata, BTreeSet/BTreeMap nodes, range/refinement scratch, and any typed parameter copy on unsupported endian/alignment. Existing executor workspace W is NOT this startup quote. Initially permit only mapped or fully resident bound parameters; lazy row routes remain explicit refusal.
4. Reuse the existing artifact owner and its safe borrowed accessors/drop order. This draft's checked-header view is not sufficient yet to build TirArtifactV1: integration needs an artifact-layer constructor that consumes prechecked metadata + the independently owned program/ticket, or a callback builder preserving the existing owning artifact's private safety boundary. Existing PalwTirContainerV1::index is private; do not bypass it with public field construction or repeat unchecked generic loader calls.
5. Stream actual G14 inventory root against the independently expected artifact root under explicit total-read/work/chunk/frontier caps; compare the original registered class (program/layout/registration tokenizer) literally and context.shape_profile_id/class-id/execution hash to the independent original pins. Class registration tokenizer may be18 while canonical context tokenizer0: never replace context or force equality. Hashes authenticate bytes, not computation correctness.
6. Construct or consume a bounded strict V2 verified space without a hidden second generic decode. DEPENDENCY: A1 retained/borrowed-space binding constructor. Retain the program/space/expected/plan/params startup owners and all corresponding tickets while serving. with_prepared_v2 creates a stack borrow after the owner is complete, avoiding a self-referential struct or escaping artifact's unsafe static internal views.
7. Preserve artifact.check_weights_unmodified() before Open and actual Step. Current Prepared holds only borrowed plan/params, so use an owner-backed session hook/facade. C-07 stamp checks follow existing file contract; they are not a promise against arbitrary concurrent in-place mutation. Never reopen a renamed path to authenticate one inode and execute another; keep descriptor-backed identity through parse/root/map or recheck exact source identity.
8. Add a separate V2 process owner: Child + WideCellClientV2, Drop kill/wait, stream deadline policy, bounded native→declared-LE Open builder. Reuse only V1 process lifetime pattern, not V1 Tensor/MapParams decode or CPU mirror. Session counters persist across Open/Close; actual executor guard covers W only, startup ticket and 2F+2D IO caps stay distinct.

Out of this patch: tensor reads/mapping/residency/startup plan quote, actual root/signature admission, new Core APIs, mutation hooks in session, generic process binary, lazy row provider, PALWTIR2/component/composite/PALWART3 loaders, FP/V4/GPU/native route. These remain required follow-up work, not completed by prepared toy IPC or this header draft.

## Review status before source application

- git apply --check /tmp/a3-bounded-artifact-header.patch PASS against frozen 1b14d1ecc (no apply).
- Rustfmt source parsing succeeds. No rustc/Cargo/type check or test execution is authorized/performed; testsource remains UNRUN.
- Header query/validation rechecks tighter per-field local limits as well as native/copy/operation limits; it cannot rely only on the initial parse policy.
- Split validation intentionally borrows an already bounded/normal-form-validated Core program; rejecting arbitrary forged typed programs semantically is not this API's role. Checked indices/products and strict serialized-byte equality prevent table access from trusting header indices.

## Parent review corrections

- New bounded_v1 declaration is inserted BEFORE the existing PALWTIR2 documentation, so the original documentation still attaches to pub mod v2.
- The empty-table test uses ProgramBuilder::new(4, 1 << 18), matching the actual normal-form history-bound profile. The earlier draft's 16 would fail decode_canonical validation; it is corrected without a Cargo/test run.

## Additional table regression source (UNRUN)

The nonempty-table test uses an original-normal-form ProgramBuilder (history bound2^18), a global I8[3] used only in pre, a per-layer I16[2] read twice by one block repeated at layers0/1, and a global head I16[64]. The independently hand-written table is [(0,None,4096,3),(1,Some(0),4160,4),(1,Some(1),4224,4),(2,None,4288,128)], file length4480. No expected-instance list or offsets are generated with the implementation's helper. It verifies borrowed program identity, literal metadata and positive/negative locate results.

Eleven mutations cover omission of the pre-only global None, omission of repeated layer1, extra undeclared entry, reordered layers, a forbidden global layer, wrong per-layer index, wrong byte size, aligned overlap, unaligned offset, largest-aligned-u64 + original128-byte overflow, and aligned past-file extent. Parser preserves the literal malformed table; split validation rejects it. These are source-only assertions: no typecheck/test execution or allocation measurement has occurred. Rustfmt parsing and git apply --check are the only validation performed.

## Normal-form regression correction

validate.rs:459 requires every block to be nonempty and :640-660 forbids genuinely unused declarations. Therefore the positive empty-table program now has a live committed token cast in pre; its no-parameter table remains empty. The nonempty positive program uses global index0 only in pre, proving it still needs exactly one None instance even though the repeated layer/post blocks never read it. A separately cloned packet with a truly unused global is refused by canonical decode; it is deliberately NOT passed to validate_with_program, whose caller contract requires the independently validated Core owner. Both positive fixtures preserve original normal-form history bound2^18. This correction was source-reviewed against the validator, not compiled or executed.
