# Independent review: authoritative rebased typed Input candidate

Reviewed `input-boundaries-rebased-typed.patch` (NOT the superseded unrebased patch) against actual demand::fetch/carry_in_node, TIR-14 validation, native court token/commit_leaf mapping, StepSpace job/leaf geometry, source authentication and Store constructors. Read-only; no repo edit/build/RPC.

No concrete blocking correctness/ownership defect found in the candidate. All new tests remain unrun; this review is not execution evidence.

## Actual coordinate origin
- Input(0) is recorded at actual Walk::fetch after its existing edge and scalar element bounds. The conservative Select/Gather walk still follows all allowed potential branches, rather than one honest read set. Input(1) remains native position scalar; constants remain authenticated canonical-program material.
- Fixed-state noncheckpoint replay already calls set_context(pos-1, writer_occ), queues the writer coordinate and restores caller context. Later pending processing re-establishes that coordinate before fetching inputs. Collector therefore uses replay position/occurrence, not the original root's position. No separate imitation of Fixed replay was added.
- Carry uses EXACT existing demand::carry_in_node_v1 (same position, previous occurrence), actual committed writer, commit_tile_len, node_slot and job.call_position -> space.leaf_index. Native court TirSource::node/commit_leaf uses the same tile/index mapping. TIR-14 requires all carry_out committed; candidate checks this again and refuses unsupported/unavailable mapping. Preceding leaf check is consistent with actual execution ordering.
- New token_source helper is the old court split exactly; helper itself intentionally does NOT validate arbitrary position. Collector separately requires pos<actual job.positions. No new modulo/off-by-one or generated-0 shift. Valid last generated input is exact_decode_tokens-2, because positions=prefill+decode-1.

## Legacy and typed refusal preservation
Legacy parameter path calls typed tracked helper with None: no boundary nodes/Vec payload are allocated, no new carry/token geometry checks run, all existing parameter data/order are unchanged. Independent returned parameter equality tests remain useful.

Rebased new named_with_inputs/named_tracked/prepared_tracked all return CoverErrorV1. Native root-input precheck is Local.native; tracked result maps CoverError::native directly. There is NO Local -> String -> E::Decode path in the new owner. New carry/token shape/missing/position errors convert through existing From<&str>/String as Semantic. Imported existing decoder/verifier errors retain their prior scope; do not claim every possible failure is now classified.

## New owner allocation
New boundary insertion is inside one existing edge-charged fetch. Total unique boundary memberships across roots <=global max_total_edges. root Vec<=actual original leaves; all per-root Vec minimums + total lengths are counted; temporary boundary BTree<=edges and its iterator coexist with output lengths/prior roots. Parameter quote independently covers parameter Walk/index/ranges/verified binding. Extra context/source references add no heap. job_shape/carry mapping/node_slot/leaf_index are scalar/borrowed scans and add no unquoted retained payload. New combined owner grows before sink allocations and retains ticket; failure drops partial roots/sets and lease, returns no partial successful cover.

Store check itself borrows immutable opaque cover, produces no escaping material/authority, and does not clone inputs. Existing Store constructor still reconstructs a bare Source and requires independent admission per existing docs; this change does NOT make the whole Store/units/material pipeline resource-complete. Test's fixture units/IDs/rows and copies are ordinary test allocations, not evidence of native acquisition ownership.

## Existing authenticators, not presence-only DTO trust
Root cover has private outer constructor/fields and no decoder/Clone; source_id cannot be caller-swapped through accessors. Store verifies source affinity. Source-produced units are opaque complete authenticated units; carry requires exact StepLeaf query/response. IDs are source-bound complete authenticated IDs. Prompt form is actual retained plan rule; Flat constructor checks exact count+commitment, Merkle constructor checks sorted unique tile indexes plus native count/index/path/last-tile authentication. Collector pos range plus authenticated tile shape makes tile presence sufficient for that coordinate. No caller form override.

## Actual fixture and remaining scope
New Input(0) fixture inserts an uncommitted Select BEFORE canonical program encoding/registration/execution, shifts old node refs/carry_out/logits and recomputes class/layout/roots normally. Both branches are the same actual previous operand, so original numeric values are preserved while actual demanded condition reads Input0. It is not a fake root/site/post-admission plan edit. Actual positive tests require nonempty PromptPosition AND GeneratedId for both preselected Flat/Merkle forms; Store tests exercise supplied/missing/wrong form/wrong bytes/IDs/carry/source.

A dedicated Input(0)-through-Fixed-replay positive is not included; correctness follows current same Walk context mechanism but has not been newly demonstrated by this candidate's GQA fixture. This is a useful later regression, not justification to fabricate a site or weaken validators.

Scope stays input/carry-coordinate relation only. Other committed Node, checkpoint/Hist state, parameter acquisition, semantic/terminal branches, public raw delivery, fee/escrow/custody/native paid install are NOT completed. check_root_input_cover is a borrowed material sufficiency check for this narrow relation, not full closure/financial/conviction capability.
