# Constraint potential terminal families — candidate, uncompiled

`constraint-family.patch` is incremental against the current repository containing reviewed Output/regular-stop/Selection. `tree/` stages all changed/new files. No repo edits, cargo or native execution by this agent. `build.py` regenerates the candidate; do not apply an earlier all-tail-rejected draft.

## Substantive prerequisite: exact structural Constraint tail coverage

Real constrained jobs carry `PalwFpJobTailV1::Constraint`; native SourceGraph previously refused every tail, so a Constraint family built on that owner would have no positive native path. This candidate adds exact Constraint-only codec/normal-form resource coverage before that native graph is constructed. Prefix/other tails remain typed Resource/out-of-coverage.

Allocation-free existing `NativeWireScanV1` preflight reads the actual automaton codec. The header chooses the EXISTING V1/V2 byte ceilings; no new bounds. Parsed frames/nodes/edges plus canonical Borsh writer growth are reserved alongside existing frame-validation scratch. After reservation, the typed Borsh decoder invokes existing `validate_form` and the same canonical-byte comparison used by `from_bytes_form`. IO resource errors stay Resource rather than crossing the old String-only malformed conversion. The parsed automaton and each sequential writer drop before graph materialization. Normal-form validation uses constant traversal state; it does not perform gate/cone/model computation. The extra writer covers twofold Borsh growth from 1024 bytes; decoded graph shape comes from the actual native scanner, not a byte multiplier.

This is STRUCTURAL data coverage for known forms, not activation/admission of V2 at an unarmed point. Existing native admission/config/clock gates remain unchanged. Old source graph/query/node IDs, limits, wire and legacy derive are unchanged. Actual native graph test compares old/new node and scope sets.

## Native potential grammar and actual connection

The existing tiled Constraint token constructor admits position 0..D−1, beat 0..V−1, witness None or Some(0..V−1). Thus the compact potential token shape has checked D×V×(V+1) entries. A separate final entry denotes the actual constrained original stop at D with the original stop reason; native count/reason legality is checked. This is the potential request shape, not a claim that every target can complete under every supplied prefix (existing replay can correctly reject an inadmissible earlier prefix).

`ConstraintUniverseV1` derives from actual immutable semantics/frame/native Source/native graph only. It owns one retained Output dependency owner and BORROWS that single all-role FullIds finish slice into projected token/stop duties. A token projection additionally derives exact all-role potential tiles at its actual row. Stop has no row dependencies. It is not regular-stop selection and does not invoke regular-stop helpers.

Actual semantic supply is NOT present in the seven-family SourceGraph namespace. The candidate exposes this honestly: unresolved EOG/potential-rendering table supply for token grammar; unresolved EOG-only supply for stop. The table root is actual immutable data identity, not a provider obligation/receipt. No fake all-role semantic supply vector or extension of the old SourcePledge is introduced. Protocol retention/funding/providers remain incomplete.

The projected-duty entry compares actual current graph/source/semantics and exact target to the EXISTING signed token/stop intent, then calls the existing retained table-material builder. It neither replaces signatures nor changes any signed domain/clock/fee. Runtime consumes the existing authenticated EOG/rendering owner plus actual Store IDs/tiles and rechecks normative replay. The existing returned evidence borrows its material owner and rechecks current native state; it is not a graph acceptance certificate or a future financial coordinator permit. Such a coordinator must separately retain its metadata/owners.

## Tests prepared, not executed

- `native_constraint_tail_graph_uses_exact_codec_quota_and_preserves_old_graph`: actual Constraint program/original; native SourceGraph nodes/IDs equal old graph; actual Prefix tail remains unsupported; q−1 refuses valid and valid-wire malformed-normalform inputs before decode; q success retains all allocations and drops; malformed start-frame decode releases lease; valid reconstruction succeeds afterward.
- Existing actual constrained token fixture becomes native-owned Source, exhaustively maps small actual D×V×(V+1) rank/coordinates, tests upper/lower boundaries, max legal witness, local graph-visit Resource, nonempty provider rows/FullIds, explicit unresolved semantic supply, duty→signed retained token builder result equality, wrong-family duty rejection.
- Existing four actual stop fixtures run through the same universe/projected Stop→retained builder; no row dependencies, EOG-only unresolved supply, exact FullIds role set, and token duty cannot serve Stop. Existing EOG-only rendering-zero/stop-phase/verdict/native negative checks remain.

Suggested filters: `native_constraint_tail_graph`, `shared_constraint_material_builder_reconstructs_real_native_original_and_rechecks`, `shared_constraint_stop_terminal_uses_full_ids_without_row_and_preserves_boundaries` plus existing SourceGraph tests. Neither V2 valid-frame reconstruction nor mined/funded original install is established by these unexecuted tests.

## Resource boundaries and remaining work

Universe reserves actual frame/hash scratch before constructor checks and nested Output owner reservation. Projection reserves inline duty and exact row Vec before allocating; zero/insufficient exploration limit stays Resource with no partial success. Source/nativegraph/retained table and returned evidence material ownership remain separate. Native Source rebuilding, current actor signature, parser/replay, intent cloning and evidence/transient proof work keep the existing caller-admitted contract; this is not a complete native resource-funded submission path. All-family closure, semantic supplier grammar/obligations, fee/capital/retention bodies and ordered funded install remain unresolved.

## Postclaim component versus prospective original admission seam

This concrete constructor deliberately requires equality with the actual currently retained state frame. It is a POSTCLAIM retained-state component; it cannot yet derive all branches during original's pre-reservation seam before that frame is installed. This local guard must not become a frozen architectural requirement that every future genuine native-prepared frame already has a rooted claim. The future coordinator must consume an opaque actual PreparedFrame from native preparation at `prepare_original_v1` BEFORE reservations/writes, with authoritative config and current acceptance context. A caller-decoded frame is not that authority. Supporting the former requires a separate checked adapter/owner, not removal of the latter authentication guard or a caller-supplied Complete flag.
