# Native graph memory preflight: concrete next seam (read-only, 2026-10-22)

Source: current G14 candidate worktree. No source edits/build/RPC, no new prices/profiles/caps. Existing caller geometry budgets are NOT native reservations and do not establish complete original admission.

## Source construction must be covered first

`palw_tir_public_source_v1.rs:188 new_v1` calls header.resolve_registered_layout (public_binding:193), clones registered program/layout, verifies binding, constructs decode view, then collects up to RUN entries in window_value_bytes (RUN is existing fixed format). `resolve_registered_layout` first checks program_bytes-derived layout lengths but clones program/layout before any native lease. `verify_tir_binding_v1` and analyze_ranges allocate their own indices/maps. A later graph quote cannot retroactively cover this peak.

Minimal seam: add a crate-private native source-construction owner taking borrowed current registered record/layout/header. Derive exact retained-program/layout/header clone requests via existing NativeHeap inventory (if inventory missing return NativeCoverage, no estimate fabricated), plus real verifier/index/decode-view scratch demand from metadata. Reserve before resolve_registered_layout. Keep lease through Source retained lifetime or transfer owner with NativeRetainedV1. Need inventory/constructor audit for verifier's scratch and its capacities, not only source's small final struct. Existing retained wire-native TIR decoder is in palw_native_resource_v1/tir.rs:13; reusable metadata-derived TirNativeRequestsV1 and btree_node_envelope_bytes_v1 are available. Resource failure remains typed NativeResource, distinct from invalid source.

## Source graph grouped preflight, without query universe expansion

`palw_court_tir_requests_v1.rs:174 source_data_upper` and private `visit_source_bounds:182` visit grouped native query families (levels + up to existing RUN count), not O(N*RUN+rows*tiles) queries. `signed_data_transport_upper:217` already computes checked conservative signed_controls including Q requests, Q*R responses, role streams, and one FullIds stream per role. Expose/reuse a crate-private grouped visitor (not a caller-supplied graph) to compute memory counts BEFORE NativeSourceMaterialGraphV3::derive_v3's maps.

Let Q=set.len, R=current exact maximum_custodians. For grouped non-IDs family count c, actual canonical maximum body b, k=ceil(b/PART):
- stream vertices += c*R*(k+2); stream edges += c*R*(k+2) (Begin has request parent).
- FullIds once if present: vertices += R*(k+2); edges += R*(k+1) (Begin has no request parent).
- Q request vertices; R*Q response vertices; response parent edges = Q*R*(R+1), because each response carries ALL role finishes plus its request. This R-squared edge memory must not be confused with signed wire bytes.
Bounds used to compute b in visit_source_bounds are conservative maxima, so totals are bounds until actual enumeration; after construction counters must be <= quote and exact graph count checked. All add/multiply/ceil/usize conversions checked. Existing request_body_bound/unit_bound need audit to confirm quote performs no wide allocations.

Retained memory requests: nodes BTreeMap envelope for V entries `(Hash64, SourceMaterialNodeV3)` plus E Hash64 predecessor Vec capacity envelopes, since Vec growth/one push may reserve beyond len. Temporary derive query_ids BTreeMap Q, full_ids_queries Vec up to IDs-query count, finishes Vec R, response parents Vec R+1, fixed hashing serialization buffers. Current streams Vec::push and responses to_vec+push use implicit capacities: use actual allocator capacity envelopes (TirNativeRequestsV1::vec_v1) or exact pre-reserve BEFORE inserts; do not charge only element len.

`SourceGraph::check_v3:304` adds ANOTHER indegree BTreeMap V, children BTreeMap at most V with aggregate E Hash64 child Vecs, ready Vec at most V. These peak alongside retained graph and must be included, or avoid redundant rebuild after construction with separately verified construction token. Native graph has no NativeHeap inventory currently; explicit concrete ownership requests are needed. BTreeMap node envelope helper is existing allocator-bound accounting; do not use arbitrary bytes per node.

`packet_ids_v3`, bind_projection_v3 etc add further sets/maps/causal packet clones. They are separate future scopes; source construction reservation does not cover these nor actual service slot search/CPU/fees.

## Numerical geometry: existing analytical count before paths

`palw_court_numerical_geometry_v3.rs:88 derive_v3` currently resolves/clones/verifies registered class then analyze_ranges BEFORE graph budgets. This preparation needs native retained/scratch coverage first. Existing `palw_tir_dissect_v1::tree_inventory_v1::numerical_tree_inventory_v1` is correct analytical seam: each real site gives disclosures+terminals, child_claims, path bytes, max_depth before root.paths allocation (lines135-154).

Use a first scan over original immutable leaf geometry to SUM checked root/path/edge/pathbyte counts, without retaining paths; reserve actual allocation requests, then second identical scan to materialize. This doubles metadata scan work (must account CPU separately, not pretend native memory reservation prices it). Per root retained `NumericalPathV3` Vec count P and path bytes B; retained root Vec count roots; DFS todo needs existing arity*depth sibling upper plus path bytes at those lengths, child range temporary Vec; current path.clone/push needs growth capacity accounting. Hold all aggregate leases through returned geometry's owner. Compare actual inventory at each root and final totals to quoted metadata. Keep caller budget as explicit local search bound only; reservation is separate opaque ownership.

## Remaining admission missing

Even a complete memory owner for source+numerical geometry remains SourceMaterialOnly + numerical coverage, not full original canonical grammar, protocol fees/CPU/IO/storage, selected native profiles, or escrow commit/settlement. It cannot activate default production diagnostics or produce funded success. Native current court/demand must come from actual accepting checkpoint/class registration rule; caller profile hash and budget are not authority.
