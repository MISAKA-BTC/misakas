# RFC0006 memory and routing court consumer — source work in progress

This is a remaining-work record, not acceptance evidence. Code implementation precedes consolidated validation. No build, tests, benchmark or node execution has run for this work.

## Confirmed missing consumers

- `gdn_core_anchored_replay_v1` has no production caller. The execution-step verifier authenticates only KV anchors and explicitly rejects an attached KV anchor for a node reading no KV cache. A bounded recurrence replay helper alone does not establish an authenticated recurrence court.
- Solo GDN v1/v2 have real executor geometry/serialization, but the core state-count dispatcher cannot enumerate them. Solo v3 also requires its exact graph-derived key-head gather.
- Hybrid v1/v2 have map IDs naming both halves but `fp_recompute` explicitly refuses their absent composed serializer. Their historical roots must not silently gain recurrence chunks. A new versioned composed map/capture/restore consumer is required.
- Routed readers append the unique q36 router of their whole layer table. They do not authenticate a per-reader router selection. Multiple routers and Kimi routing cannot be made covered by choosing an arbitrary router or changing the refusal enum.

## Source added

`palw_gdn_state_map_v2` derives a borrowed, allocation-free recurrence geometry from the actual signed profile. It enumerates the existing full-window v1, per-head v2 and key-head v3 layouts, using the existing chunk byte/count limits. It spells delta/conv chunk order, exact row dimensions and region-major convolution channel mapping. It does not change old IDs, root formation, accepted-profile semantics or court dispatch. It is not by itself a connected court or a coverage completion.

## Code still required

1. Add an explicit version/fence for the new consumer, default dormant with no public preset change. Keep old profile/root/codec interpretation intact.
2. Implement new anchor evidence that binds the existing signed execution binding, exact checkpoint index/cadence, checkpoint leaf and state-chunk membership. Geometry must be derived from the admitted profile, never supplied as allocation or state authority by a challenger.
3. Connect verified delta/conv state to deterministic bounded GDN replay, including initial-zero state, partial final chunks, grouped key-head aliases and the actual fused versus integer kernel semantics. Add exact output-slice comparison and objective mismatch classification.
4. Implement a new hybrid composed serializer/capture/restore under its explicit new map version. Authenticate both attention and recurrence halves; do not reinterpret hybrid v1/v2 historical roots.
5. Introduce an immutable per-reader router binding under an explicit new profile/consumer version. Derive the selected router's score universe and width, authenticate its own committed row, and use it for expert gather/combine. q36 and Kimi arithmetic keep their actual separate semantics. Missing/ambiguous bindings remain unadjudicable until this code exists.
6. Connect registration/cell inventory, SDK/backend evidence assembly, actual court dispatch and typed before-allocation admission. Only then update coverage results for the boundaries actually reached by those consumers.
7. Prepare source controls for changed/foreign checkpoint roots, wrong index/cadence/layer/head, truncated/duplicated chunks, cross-head alias disagreement, wrong selected router, multiple independent routers, Kimi expert universe, honest long-versus-anchored replay parity and flags-off historical bytes. Run them only in the consolidated validation phase.

All evidence above authenticates data identity, membership and the selected reader binding. It is not a cryptographic proof that model computation is correct. The verdict comes from deterministic recomputation over the authenticated inputs. No ZK, succinct validity argument, polynomial-commitment computation proof, recursive proof or proof aggregation is adopted or left as a future remedy. Resource/fee calibration, complete native writer closure, authenticated original-identity migration and 100k/pruned-P2P acceptance remain separate open requirements.
