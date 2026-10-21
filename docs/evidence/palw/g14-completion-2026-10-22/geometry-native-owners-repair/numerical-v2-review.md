# Numerical owner typed materializer v2 review

Read-only; no repo edits/builds. Reviewed typed-materializer.patch and cumulative numerical-owner-v2.patch.

Previous cause-classification finding is resolved: two actual try_reserve helpers retain NativeCoverage Resource, and native wrapper maps the private enum through native_v3. String/&str semantic errors remain Decode. The path usize conversion overflow is Resource in the new native API. Old public derive_v3 uses legacy_v3 and retains exact strings: numerical root allocation failed / numerical path allocation failed / numerical path length overflow, alongside unchanged semantic strings.

The shared materializer avoids duplicated traversal. Root/path materialization, cap checks and DFS order are unchanged. Resource errors release the reservation by existing RAII. Deterministic usize::MAX/u64::MAX tests exercise real capacity-overflow refusal without intentionally exhausting physical memory or classifying strings. This does not imply every implicit Rust allocation/OOM becomes a catchable typed error; the repair addresses the actual try_reserve failures that the code already captures.

No additional blocking discrepancy found. The prior successful-path coexisting-heap review still applies.

Provenance caveat: existing /tmp/g14-numerical-native-owner/AUDIT.json describes v1 candidate/source hash and old_derive_v3_byte_identical=true. For v2 the public method is now a wrapper, so literal body identity no longer holds. Keep that JSON explicitly v1-only or update it to describe shared-materializer semantic/string preservation with the new hash; do not reuse its byte-identity claim for v2.
