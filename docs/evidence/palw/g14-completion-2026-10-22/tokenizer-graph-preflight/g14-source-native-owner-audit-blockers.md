# Native Source/geometry owner: exact audit result and non-fabricated boundary

Read-only source. No repository patch is proposed as a complete owner yet: existing program-codec lease is NOT a proof that Source construction's extra space/index/hash allocations are covered. This is an inventory gap, not a need to choose prices/caps/profiles and not a claim that an upper is mathematically impossible.

## Reusable correct lease seam

- `palw_native_resource_v1/demand.rs::reserve_native_requests_v1` returns `Option<NativeDemandLeaseV1>`; owned ticket grows/splits and releases outstanding local allocation demand on Drop. Off-mode None must not be relabelled as a paid/native complete admission.
- `NativeRetainedV1::from_owner_reserved_v1` retains this ticket with an actual returned value. Do not return a bare Source after dropping the constructor scratch lease if retained Source.header/window Vec still owns allocations.
- `TirNativeRequestsV1::{exact_v1,vec_v1,add_v1,product_v1}` and `native_value_requests_v1` use typed actual ownership/capacity bounds, not RSS or guessed multipliers.
- `ledger_expansion.rs:449 standalone_tir_program_requests_v1` is a borrowed framing scan over actual bytes before decode; it reserves codec/normalform/gate+cone+interval workspace through `program_blob:375`. It is valid for existing `decode_native_tir_program_v1`; it does NOT currently inventory PalwTirStepSpaceV1's extra fields.

## Exact extra allocations before Source is available

`Source::new_v1` -> `header.resolve_registered_layout_v1` -> `resolve_owned_class_v1` -> verify_tir_binding -> StepSpace::new, then Source invokes verify_tir_binding AGAIN. First verified result is temporary, second coexists with constructed Source.

1. resolve_registered_layout clones canonical program Vec and layout (commit_tiles/state_tiles Vecs). `class.layout_digest` serializes layout with pinned Borsh initial/growth capacity; class.class_id repeats it. Binding clones job_context.network_id. Existing NativeHeap inventory covers PalwTirLayoutV1 and public header; program raw Vec copy must be counted separately from Arc record clone.
2. `palw_tir_step_v1.rs:218 StepSpace::new` decodes program, validates (ProgramInfo), clones layout, then from_program:224 allocates private `Vec<BlockSpaceV1>` of B blocks; nested growing `Vec<CommitNodeV1>` aggregate up to N commits; `program.occurrences()` Vec O where O=layers+2; slot_bases Vec O; retained fixed/hist `Vec<PalwTirStateInstanceV1>` aggregate up to S*max(1,layers); transient per-state `Vec<Option<u16>>` up to max(1,layers). Invalid layout paths still allocate block/commit prefix before refusal.
3. Source's retained header clone and `window_value_bytes` collected over existing RUN=256 entries. Context/source hashes serialize actual job_context/header into Borsh buffers; reserve pinned real serializer capacity from `object_length`, not encoded length only. Source's window/cone bound functions scan existing metadata without wide query output.
4. Numerical derive additionally `Box<VerifiedBinding>`, analyze_ranges allocation, firstpass scan and its per-site dissect_site/reductions data, then retained roots/paths/DFS. Current graphbudget quote does not cover any of these.

## Concrete next patch shape (requires explicit audited inventory, no arbitrary constants)

A) Extend borrowed program scan to return an opaque crate-private structural quote containing existing program_requests + actual parsed collection cardinality (currently shape.collection_entries is discarded by standalone_tir_program_requests). A checked aggregate cardinality is a legitimate conservative bound on B,N,S,layers; it is not a caller budget. Do not expose an API accepting a peer-declared count and treating it as scanned.

B) In palw_tir_step_v1 (private types are visible there), add typed StepSpace request quote from this opaque parsed metadata: exact B BlockSpace, growing aggregate commit storage (per-block minimum capacity must account up to B empty/nonempty Vecs), exact/growing occurrence and slot-base vectors matching their actual constructors, both fixed/hist vectors' capacity envelopes, and simultaneous per-state layer scratch. Use current type sizes/capacity helper. Existing standalone program request includes ProgramInfo/intervals but caller must document where its retained vs scratch peaks occur; do not spend the same peak twice to excuse omitted StepSpace fields.

C) Source-native constructor takes real registered/layout/header, rejects check_program mismatch and primitive impossible metadata first; asks A+B+actual NativeHeap clone requests+actual serialized writer requests; `reserve_native_requests_v1` BEFORE program/layout clone or Source::new; then executes original unchanged Source::new and returns NativeRetainedV1<Source>. Class/header identities still checked by existing constructor. Keep capacity errors typed NativeResource, semantic errors distinct. Source sourceID/header/wire values remain identical.

D) Numerical owner needs its OWN firstpass inventory and lease, not a promise that Source lease covers it. Existing numerical tree_inventory provides exact paths/bytes/edges; add root vec + path Vec capacity + DFS frontier paths and site/reduction scratch. Only then call existing derive. Caller NumericalGeometryBudget remains a local scan budget, never a lease.

## Why no executable complete-owner patch is supplied now

Current framework exports no audited StepSpace allocation inventory or opaque borrowed parsed-count quote. Reusing the standalone codec bound without B) would silently omit real large Vecs. A wrapper that clones/decodes first to discover counts would violate pre-expansion admission. A wrapper that returns Unsupported always would be an empty stub, not useful completion. Inventing bytes-per-node factors or treating sourcegraph V/E budget as native lease would be incorrect. The source above identifies the smallest real unit A+B+C and exact functions/fields to audit before implementing; D is separately larger.

## Tests required for A+B+C

- Same real class/header Source ID/window bounds before/after; typed owner keeps lease alive until last owner drops.
- Hostile canonical vector count/truncation/collection overflow rejected by borrowed scan BEFORE clone/decode; layout lengths exceeding program metadata similarly reject.
- Native checker quota one byte below exact requests refuses before constructor; exact request succeeds for existing tiny/two-tile/history-GQA real fixtures. No new global quota/preset.
- Malformed semantic source versus insufficient local resource produce distinct errors, including decoder timeout/IO handling.
- More commit nodes/per-layer state occurrences increases quote before allocation; zero/minimum Vec capacities and invalid-layout prefix failures included.
