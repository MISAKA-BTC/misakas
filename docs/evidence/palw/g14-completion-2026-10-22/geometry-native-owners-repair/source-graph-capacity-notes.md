# Scope and inventory notes

The new native source graph owner borrows a NativeRetained Source; it never treats an arbitrary graph budget or caller profile hash as native authority. No production caller or activation added. Old derive body is byte-identical in this candidate (AUDIT.json).

Two phases: frame validation/hash writer ticket BEFORE structural check; grouped native universe/extent under that ticket, then grow same ticket for actual graph/index construction. Frame plan clone is conservatively bounded by actual full frame NativeHeap; actual Borsh full frame or actual scope tuple length, pinned writer capacity/growth, bounds sequential subobject hash writers. Control/relative/node hashes already stream into the hash sink and need no per-node Vec writer.

Counts cover V graph BTree nodes plus predecessor Vec E and separate per-node Vec minimum, Q query-id BTree, FullIds ordinal Vec <= Q, role finishes R, check_v3 indegree V, children BTree <=V and children Vec E with per-Vec minimum, ready <=V, and the owner frame. Request descriptors and runtime guards contain only fixed fields; their inline size is inside SourceMaterialNodeV3's BTree envelope. sort_unstable is in-place. Query/FullIds/finishes scratch remains conservatively reserved alongside check scratch. Packet-id sets and bind_projection/service reservations are NOT covered by this construction owner.

Explicit limitation: carried job.tail parsers have not been inventoried; native owner returns NativeCoverage Unsupported for them. Old derive behavior is unchanged. Native original TIR preparation currently disallows such tails, but the resource refusal does not pretend this closes every future semantic grammar/version.

BTreeMap insert and some legacy Vec growth APIs are infallible allocator interfaces; native leases account trusted typed logical requests and local quota refusal, NOT a guarantee against physical OOM or a claim that every legacy allocator abort is now typed Resource. This candidate adds no string-based allocation classification.

Tests are proposals only: real tiny+two-tile all-family nodes/relative/absolute scopes equality, independent NativeDemandChecker prelude-1 and total-1 refusal with complete ticket cleanup, exact total retained until owner drop, old local budget refusal, checked counter overflow. No new prices/limits or protocol profile is selected.
