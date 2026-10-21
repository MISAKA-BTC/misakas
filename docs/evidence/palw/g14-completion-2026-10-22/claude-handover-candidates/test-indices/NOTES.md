# Test-only independent actual Finish index — uncompiled

Current applied tree base. Repo untouched; no process/run intervention/build. Three test files only: numerical material, Prefix family, Selection family. Applycheck0. Production graph, direct Finish helper, all fixtures, native work/resource budgets/caps and small oldscan/direct equivalence test remain unchanged.

Heavy size-dependent assertions found:

* Numerical `for dependency -> cover.roots.any + graph.nodes.any`: O(D×R + D×N), where D includes every root/input/provider membership, N all source graph nodes. Exact equality on first found hash meant unrelated nodes were revisited for every dependency.
* Prefix `for dependency -> graph.nodes.any`: O(D×N), while every actual canonical ordinal was still checked separately.
* Selection `for dependency -> graph.nodes.find`, with `set.request_at()` for unrelated FinishUnit entries: O(D×N×inverse-query-work); overlapping StepRun ordinal decoding can be expensive despite only a few desired row-tile suppliers.
* Output tests use one graph filter/collect per fixture; Constraint uses one nodes count; these are linear passes, not repeated per dependency, and are left intact.

Each changed test now builds its own BTreeMap<actual logical ID, borrowed actual Finish node> in one graph traversal, asserting uniqueness. This does NOT call the new production keyed helper to validate itself. Per dependency lookup then independently checks actual Finish target/query, literal role, logical membership and absolute ID. Numerical additionally uses opaque ascending cover-root binary search instead of an O(R) root scan. Prefix still recomputes each actual canonical state leaf and compares its exact index. Selection still checks actual row/tile/native request inversion for the selected node only. Some assertions are stronger than old any-node membership. No assertion/cardinality/fixture is removed or replaced by production output alone.

Complexity becomes O(N + F log F + D log F) plus actual selected-request checks and fixed hashes, where F is actual Finish count. Index memory is only test provisioning (borrowed node refs plus keys), not admitted native heap or an extra production graph index. Existing native quota−1/hold/drop tests and resource assertions remain unchanged. This does not prove graph construction/real native material processing is cheap or claim measured runtime improvement; actual post-rebuild timings still required. Small realgraph oldfullscan/direct equivalence remains intentionally independent and unchanged.
