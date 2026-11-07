# Independent read review — canonical Finish lookup

2026-10-22. Reviewed patch and NOTES against actual current source; no repository modification, compilation or native execution.

No concrete blocking issue found by read review. Geometry::new orders StepNode count then exactly n StepLeaf requests; native_demands changes only family 6. Thus checked families[0]+index is the actual StepLeaf inverse, and its request_at roundtrip executes family 1 without enumerating overlapping StepRun requests. Out-of-range and checked overflow stay rejected.

Graph insert and new lookup both hash the identical (scope, Finish{target,role}) with the same existing streaming domain. BTreeMap lookup returns existing immutable nodes, checks kind, and introduces no index allocation or alternate identity. Logical/absolute packet identity, native frame/source/profile checks, complete target-by-role membership and output dependency order remain unchanged. Requiring each exact expected Finish is stronger than trusting group cardinality alone.

Numerical and Prefix joins charge checked local work before every target-role lookup and preserve typed Resource on exhaustion. Removing unrelated graph scans changes this private exploration counter meaning, explicitly documented, and supplies no calibrated CPU, protocol price, adopted budget or acceptance authority. Existing output cap, preallocation quota and retained owner are unchanged. Streaming hash/map operations have existing transient implementation costs; this patch adds no bulk owned allocation.

The small actual graph test compares keyed lookup against the previous scan including kind/logical/absolute identity, rather than merely calling the new helper on both sides. Native leaf/request/role boundary negatives and exact lookup-count versus count-minus-one tests exercise refusal. Existing nonempty Input0/Fixed/history and Prefix fixtures remain. Tests are uncompiled here: runtime/type/performance success remains for the parent lock-controlled verification. This does not establish complete original closure, funded admission or runtime selected-path minimality.
