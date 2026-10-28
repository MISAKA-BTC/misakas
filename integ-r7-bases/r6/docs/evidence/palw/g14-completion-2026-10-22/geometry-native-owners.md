# Native owners for numerical topology and source graph — 2026-10-22

Candidate based on `796090c8e`; release build, 37 unique parent tests and 2 isolated quota-worker invocations passed.

## Numerical geometry

The explicit native constructor borrows an existing retained Source. It reserves preparation, interval/site/cone/value and analytical tree-inventory scratch before the first actual leaf scan. Only checked counters escape that scan; all prepared data is dropped before a second materialization pass. The same lease grows to cover roots, each path Vec capacity, DFS paths/frontier and child ranges, then stays with the returned geometry. Actual counts/depth must match the first pass. Existing local limits, arity and path semantics remain unchanged.

A shared private typed materializer preserves explicit root/path allocation refusal as Resource on the native API and returns the original exact strings on the legacy API. No string comparison classifies errors. Existing infallible allocations and prelude String errors are not claimed to be universally classified. The initial v1 byte-identity audit is v1-only; v2 changes the old API to a wrapper and the body to a typed shared materializer.

## Source graph

The explicit owner first reserves actual frame-check clone/hash-writer scope, then grouped metadata derives full V/E/Q/R bounds before expansion. Node/query maps, per-vector minimum capacities, all predecessor edges, shared FullIds ordinals, role finishes and check_v3 indegree/children/ready scratch are reserved. The old graph constructor and relative/absolute IDs are preserved. Unknown carried-tail parser inventory is explicitly Unsupported only on this new owner; old derive is unchanged. Packet-set/projection allocations remain separate.

Both owners retain a conservative construction peak through returned lifetime. They are logical allocation ownership, not CPU/IO prices, RSS measurements, paid receipts, native policy authentication, complete canonical original or full F02/F03/escrow integration.

## Evidence

[Final frozen source/commands/executable hashes](geometry-native-owners-final/verification.json), [numerical independent review](geometry-native-owners-repair/numerical-independent-review.md), [typed revision review](geometry-native-owners-repair/numerical-v2-review.md), [source graph review](geometry-native-owners-repair/source-graph-independent-review.md).

The first build failed E0369 because an extra `>` in a nested generic call parsed as comparison. [Original failed source and reason](geometry-native-owners/failure-reason.md) are preserved; the repair removes one character. All builds/native execution use the shared heavy lock and frozen Rust/Cargo. No pre push, arming or deployment.

The second build failed E0382 because the minimum-capacity accumulator was consumed twice in a loop. [Second failed source and reason](geometry-native-owners-repair/failure-reason.md) are preserved. Final repair caches its byte scalar once before the loop, without changing counts or limits.

## Result

Numerical geometry5, source graph7, retained Source2, shared requests9, t12 identity9 and activation5 passed (37 unique parent tests); two isolated Source quota workers also passed. [Exact test names](geometry-native-owners-final/selected-test-summary.json). Nonempty history geometry is compared across all existing legal arities; local checker tests cover first reservation/growth refusal, zero outstanding after failure, held ticket and Drop release. Native allocator-capacity overflow preserves Resource while old error spelling remains. Frozen source hashes stayed unchanged. This is component resource/identity evidence, not a funded-node or whole G14 campaign. [Remaining closure review](geometry-native-owners-final/remaining-full-original-closure-audit.md).
