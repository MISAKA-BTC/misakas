# Numerical owner audit versions

- `AUDIT.json` applies ONLY to initial `palw_court_numerical_geometry_v3.rs` and `numerical-owner.patch` (v1). Its `old_derive_v3_byte_identical:true` is correct only for that initial candidate. It does not audit v2.
- `palw_court_numerical_geometry_v3.typed.rs`, `typed-materializer.patch` and `numerical-owner-v2.patch` are v2. V2 extracts the old public derive body into a shared private typed materializer; public derive now maps typed errors to the old strings, native derives preserve NativeCoverage allocation failures. Therefore the old public derive function is NOT byte-identical in v2. Caps, graph/path semantics and old string behavior are intended preserved, pending actual build/tests.
- Both versions are /tmp proposals. No cargo, native execution or repository mutation was performed here. Local `git apply --check` validates patch applicability only.
