# Actual-field StepSpace quote refinement candidate

Candidate: `step-space-precision.patch`; staging copies in `tree/`. Repo untouched, no compiler/test executed. `git apply --check` succeeds against current shared tree. Suggested filter: `native_step_space_inventory_tests_v1`, then Prefix native material tests and existing Source owner/quota tests.

## Exact scope and derivation

The old StepSpace additional quote used `structural_items = collection_entries + byte_entries` as block count, node count, state count AND schedule length. Inline constant bytes therefore also inflated both factors of its state-instance term. Candidate obtains actual field metadata in a second allocation-free wire scan, after the existing complete audited scanner has already accepted framing. Existing generated native scanners are used to skip every compound field, including node primitives; unknown variants, invalid bool and trailing data still refuse. String scanning only reads/skips the declared bytes; UTF-8 validation remains in the unchanged actual Borsh decoder, not this allocation-free metadata scan. The helper recognizes program version 1 only, rather than interpreting unknown-version bytes as this geometry. No opaque metadata constructor is exposed.

The second walk extracts actual parameter, block, total node, schedule layer and state kind/per-layer counts. It neither decodes a program nor builds a reference map/Vec. Parameter count is observational/test metadata; this patch DOES NOT refine the preexisting codec/validator/analysis quote.

Additional constructor allocations are bounded as follows:

- `blocks = Vec::with_capacity(program.blocks.len())`: actual B BlockSpace elements.
- Independently growing commits: every commit is a node, so total logical commits <= actual total N. Sum pinned grow envelopes <= grow-envelope(N) + B * grow-envelope(1), covering tiny independently allocated block vectors.
- Retained occurrences and the temporary second occurrences list inside `occurrence_slot_bases` each have exactly L+2 entries. Slot-base collected Vec uses the pinned growth envelope on L+2.
- Fixed state instances <= F_global + F_per_layer * L. Hist instances <= H_global + H_per_layer * L. Global states always instantiate once; per-layer states instantiate only at schedule entries whose block references that state. Counting all L for each is conservative without any reference-set allocation or semantic assumption about which valid block is scheduled.
- Each output Vec has its own minimum/growth envelope. At most one temporary `layers: Vec<Option<u16>>` coexists with both outputs. Its logical size <= max(L,1), including the zero-layer global `[None]` case.

ProgramInfo/validator/cones/interval/range scratch and decoded Program/layout remain covered by the unchanged existing `program_requests` and Source layout/input owner lines. No such cost is removed here. All new StepSpace arithmetic returns NativeCoverage Resource on overflow through the same existing native constructor API. Existing scanner IO error mapping is not reclassified by this patch; this is NOT a claim that every old scanner resource failure has been typed globally.

## Verification proposal and limits

Existing nonempty actual executable tiny fixture compares scanned metadata to decoded actual fields and additional quote to actual retained StepSpace capacities. Existing overflow/hostile framing tests remain, adapted to the new count helper. A new wire-only metadata test covers all four fixed/hist × global/per-layer categories and L=7; it explicitly does not claim the mutated schedule/state program is semantically executable.

Additional actual executable coverage now uses `palw_tir_public_fixed_history_frame_fixture_v1(Flat, committed_writer)` for both true and false. Each rebuilds the actual class through StepSpace::new, requires nonempty fixed and history state instances, checks occurrence/state bounds and compares the quote against actual retained Vec capacities. Ragged schedules and L=0 remain further coverage opportunities; the wire-only four-category test is kept separate. Code review derivation above is the conservative upper-bound basis, not a successful physical memory measurement. The existing actual nonempty fixture is not a proof of every allocator capacity behavior; pinned grow/min assumptions remain the same ones used by the current native inventory.

Crucially, separate `program_blob` still includes a collection_entries-squared cone term and actual-independent MAX_LAYERS unrolled workspace, multiplied by four coexisting analysis passes. This candidate preserves it. It may remain the dominant Source quote, so successful StepSpace refinement is not sufficient evidence of practical model capacity or door <=3,120. Capture printed Source/StepSpace/program subtotals after compile rather than asserting the 4GiB blocker is solved.
