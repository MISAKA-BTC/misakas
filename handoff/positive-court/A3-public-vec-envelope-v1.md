# A3 — unchanged growing-Vec request policy, public local helper

Source `9c010109d`, branch `codex/a3-public-vec-envelope-v1`. Exactly two Core
files change; two regression tests are source-only and **not compiled/run**.
Root's current build and previous tested candidates are unchanged.

`palw_native_resource_v1::growing_vec_request_capacity_v1<T>(count: u64)` is a
public, nonallocating `const fn` returning `Option<u64>`. It extracts the existing
private `TirNativeRequestsV1::vec_v1` policy: zero returns0; otherwise raise to
minimum8 for one-byte elements,4 for other widths at most1024, or1 for larger
elements, then checked doubling. The return value is a logical element-count
envelope. The caller must separately check multiplication by `size_of::<T>()`,
native index widths and all simultaneous local byte limits before construction.

The old private method keeps its zero early return and uses the same helper
before unchanged exact-byte and aggregate addition. Capacity, byte and aggregate
overflow ordering, component messages/types and unchanged counters on failure
are preserved. Zero-sized types retain the old checked count overflow even
though successful count multiplication by their width gives zero bytes.
No native flags, authority, price, wire ID, credit or runtime route changes.

This function supplies neither an actual Vec capacity promise nor a physical
allocator/RSS bound, host lease, NF workspace quote or complete startup permit.
Node startup remains responsible for its typed plan/params/source/diagnostic
ownership and separate reservation. A1's program-only local owner does not gain
native authority by consuming this helper.

[The source-only manifest](A3-r1-evidence/public-vec-envelope-source-9c010109d/manifest.json)
pins source/tree/files, the preserved contract and independent A1/panel_sources
reviews. Proposed tests cover const/public access, zero/minimum transitions,
width1024/1025, large counts/ZST overflow and all three private failure stages.
No successful test count or binary hash is supplied before the authorized run.
