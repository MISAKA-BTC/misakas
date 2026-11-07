# Is an explicit selected-stage guard necessary?

Read-only during frozen build. No source edits or execution.

## Normal native provenance already implies the stage

`PalwTirPublicClaimV1::prepare_native_v1` calls `check_tir_public_allocation_native_v1`. That function calls the same `derive_tir_public_allocation_v1` over actual original/header/record/layout/semantics/inventory/native rules and requires complete equality of the ENTIRE carried plan with its recomputed expected plan. A caller cannot remove a semantic stage and preserve that check.

In derive_tir_public_allocation_v1, a constrained job with no semantics is Incomplete(NativeSemanticStatement). With matching registered semantics, an is_constraint job adds the Constraint family unconditionally. `finish_preclaim_quote_v1` adds the Tokenizer family whenever raw_tokenizer_bytes is Some and a matching exact accepted EOF inventory is supplied; a mismatched Some/None pair errors rather than silently omitting the family. Both create the actual semantic bundle/resource identity. The ordinary original preparation path takes semantics/inventory from state, not from a caller bool, and installs the same signed history dependency (`frame_semantic_bundle_v1`).

The new Tokenizer adapter requires the claim-keyed snapshot, not the current class row: `palw_offence_target_v1(state,&claim)` obtains `state.claim_semantics_v1(claim)`. `palw_tokenizer_statement_view_v1` requires all four prospective table/root/rendering/raw/key-bound fields. A snapshot with historical None cannot enter. The Constraint adapter likewise reads claim_semantics directly and requires matching job table root; its native Court constructor requires an actually constrained original job.

## Later declarations: bound_from alone is NOT the entire guarantee

`ClassSemanticsDeclaredV1` in objects.rs intentionally preserves a previous class row's bound_from_daa when filling previously absent prospective fields. Therefore a later declaration may have bound_from <= an old claim: the adapter's numeric bound comparison alone would not reject that declaration if someone incorrectly copied the entire current class row into the old claim snapshot.

But the actual native transition does not perform that copy. `write_claim` stamps the then-current row only on claim CREATION (`!self.state.claims.contains_key`), under the independent semantic fence. Later declarations update only class_semantics; already accepted claim_semantics stays unchanged. The existing test around palw_state_v2/tests.rs12688 explicitly checks class table/rendering fields become Some while the old claim snapshot remains None, and an initially historical claim gets no snapshot during migration. Thus the later prospective declaration is rejected by missing claim snapshot/prospective fields even when bound_from is retained, not by the bound number alone.

## Counterexample status / recommendation

No counterexample using ordinary accepted native transitions was found. A direct privileged state mutation/test fixture could stamp a later class row onto an old nonsemantic frame; the carriage's immutable-semantic checks permit a claim snapshot's optional fields to be a subset of the class row, so merely validating a manufactured state shape is not a proof of historical provenance. An extra plan-stage guard could reject that fabricated combination, but that is not a reachable native-state attack and should not be presented as one.

The current Store API explicitly trusts rooted actual state/frame provenance; it is not an installer for caller-supplied partial graphs or fabricated snapshots. Under that contract the additional stage/resource Some guard is redundant. I recommend not adding it solely for defensive duplication. If a future coordinator accepts externally assembled state or decoded frames without re-running native plan equality, fix that boundary first; a selected-stage check would not establish full dependency/funding/history completeness.

The genuine remaining limits are unchanged: actual native policy accepting-point provenance remains a caller duty, public raw/proof availability is absent, and private material construction is not paid original admission or complete branch closure.
