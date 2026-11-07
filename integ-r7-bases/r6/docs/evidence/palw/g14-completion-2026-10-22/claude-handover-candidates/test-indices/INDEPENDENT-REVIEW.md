# Independent read review — test-only Finish indices

2026-10-22. Reviewed patch/NOTES against actual current three test files and opaque cover constructor. No repository edit, build or native/process intervention.

No concrete blocking issue found by read review. Patch changes only Numerical/Prefix/Selection assertions. It does not touch production lookup/graph, native reservation code, fixture construction, caps, exact-work/minus-one tests, quota hold/drop tests, signed finalizer tests or the small independent old-fullscan/direct-lookup equivalence test.

Each fixture traverses actual opaque graph nodes once, filters literal Finish variants and builds a test-owned logical-ID-to-borrowed-node BTreeMap, rejecting duplicate keys. Per-dependency lookup uses actual stored nodes rather than the new production helper. Logical membership is retained by key lookup, absolute identity remains checked, and literal role/material/request-shape checks strengthen the previous Numerical/Prefix any-node assertions. Prefix still derives and checks the actual canonical leaf for every ordinal. Selection still requires exact selected row/tile and all-role membership/cardinality; checked u32 conversion improves the old unchecked tile cast.

Numerical root binary search is supported by actual OriginalRootInputCover construction: ascending for leaf in 0..original_leaves, at most one roots.push per eligible leaf. No external DTO can reorder this opaque cover. Indexing removes repeated unrelated StepRun inverse work; selected StepLeaf/TraceTile request_at roundtrips remain independent checks. Existing all-role/count/resource assertions are retained.

The borrowed test index adds test-provisioning memory only; no claim is made that it is admitted native heap. Complexity improves by removing repeated full scans, but actual timing and runtime/type success remain unverified until parent lock-controlled rebuild. A faster assertion is not evidence that production construction, proof authentication, complete-original admission or financial settlement succeeds.
