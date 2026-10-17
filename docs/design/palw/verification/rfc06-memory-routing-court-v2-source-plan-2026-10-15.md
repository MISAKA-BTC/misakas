# RFC0006 memory and routing court consumer — source work in progress

This is a remaining-work record, not acceptance evidence. Code implementation precedes consolidated validation. No build, tests, benchmark or node execution has run for this work.

## Confirmed missing consumers

- `gdn_core_anchored_replay_v1` has no production caller. The execution-step verifier authenticates only KV anchors and explicitly rejects an attached KV anchor for a node reading no KV cache. A bounded recurrence replay helper alone does not establish an authenticated recurrence court.
- Solo GDN v1/v2 have real executor geometry/serialization, but the core state-count dispatcher cannot enumerate them. Solo v3 also requires its exact graph-derived key-head gather.
- Hybrid v1/v2 have map IDs naming both halves but `fp_recompute` explicitly refuses their absent composed serializer. Their historical roots must not silently gain recurrence chunks. A new versioned composed map/capture/restore consumer is required.
- Routed readers append the unique q36 router of their whole layer table. They do not authenticate a per-reader router selection. Multiple routers and Kimi routing cannot be made covered by choosing an arbitrary router or changing the refusal enum.

## Source added

`palw_gdn_state_map_v2` derives a borrowed, allocation-free recurrence geometry from the actual signed profile. It enumerates the existing full-window v1, per-head v2 and key-head v3 layouts. Composed head maps require one complete chunk per head; held maps use the existing path-depth cap, flat maps the existing chunk-count cap. It spells delta/conv order, direct selected-layer/head indexing, exact row dimensions and region-major convolution channel mapping. It does not change old IDs, root formation or accepted-profile semantics.

`palw_step_refute::memory_v2` adds a separate version-2 data-membership evidence and deterministic terminal API. The exact predecessor recurrence checkpoint is derived from the registered cadence; it is deliberately different from KV's checkpoint after the current cache write. Selected delta chunks are authenticated at their exact layer/head indices against the existing flat or held state root and the claim-bound checkpoint root. Canonical step inputs are constructed directly from the authenticated suffix; the code does not allocate a full prefix and then trim it. The terminal calls the existing float anchored replay or Q36 integer arithmetic with its authenticated initial state, and compares the committed output tile exactly. Genesis uses zero state only where no predecessor exists. Legacy hybrid v1/v2 without an actual composed serializer and solo key-head v3 without its production capture remain explicit refusals.

The terminal retains a typed logical temporary reservation through authentication, input assembly, state decode and deterministic replay. It counts actual fixed source widths, per-vector minimum/growth capacity, source membership paths, per-head state and actual Q36 parameter/extraction buffers. Its scanner/heap visitors are generated from the new exact types. This does not establish physical allocator/RSS closure, calibrated fees or complete native runtime admission. Source controls cover changed roots/cadence/head/chunks and genesis legacy arithmetic agreement, but have not run.

The API is not yet a native carrier, fence, registration/cell coverage or SDK capture/restore consumer. Existing V1 evidence fields/bytes are unchanged; its canonical helper passes no suffix override and Q36 receives no initial state. A standalone court API is not a completed consumer integration.

## Code still required

1. Add an explicit version/fence for the new consumer, default dormant with no public preset change. Keep old profile/root/codec interpretation intact.
2. Connect the new exact anchor evidence and terminal to actual native court acceptance/folding and provider evidence assembly. Geometry must remain derived from the admitted profile, never supplied as allocation or state authority by a challenger.
3. Finish conv-state consumers and end-to-end anchored replay controls, including partial final chunks, grouped key-head aliases and the actual fused versus integer kernel semantics. Delta-state authentication and the standalone float/Q36 terminal now exist as unexecuted source; this is not full model coverage.
4. Implement a new hybrid composed serializer/capture/restore under its explicit new map version. Authenticate both attention and recurrence halves; do not reinterpret hybrid v1/v2 historical roots.
5. Introduce an immutable per-reader router binding under an explicit new profile/consumer version. Derive the selected router's score universe and width, authenticate its own committed row, and use it for expert gather/combine. q36 and Kimi arithmetic keep their actual separate semantics. Missing/ambiguous bindings remain unadjudicable until this code exists.
6. Connect registration/cell inventory, SDK/backend evidence assembly, actual court dispatch and typed before-allocation admission. Only then update coverage results for the boundaries actually reached by those consumers.
7. Prepare source controls for changed/foreign checkpoint roots, wrong index/cadence/layer/head, truncated/duplicated chunks, cross-head alias disagreement, wrong selected router, multiple independent routers, Kimi expert universe, honest long-versus-anchored replay parity and flags-off historical bytes. Run them only in the consolidated validation phase.

All evidence above authenticates data identity, membership and the selected reader binding. It is not a cryptographic proof that model computation is correct. The local verdict comes from deterministic recomputation over authenticated inputs. Consensus credit still requires randomly selected validator approval under an explicit honest-fraction premise; optimistic Final and matching local replay do not replace it. No prohibited computation-proof mechanism is adopted or left as a future remedy. Resource/fee calibration, complete native writer closure, authenticated original-identity migration and 100k/pruned-P2P acceptance remain separate open requirements.
