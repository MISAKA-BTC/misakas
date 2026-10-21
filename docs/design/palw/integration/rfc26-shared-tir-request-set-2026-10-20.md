# RFC-0026 F03 — owner-independent native TIR request universe

Status: Proposed / Review Required implementation component, 2026-10-20. [ADR-0192 A/A detailed contract](rfc26-service-shared-frontier-contract-2026-10-20.md) and [RFC-0026](../../../rfc/0026-court-policy-master-contract.md) remain governing. No wire tag, original terms, profile adoption, preset, limit, activation, deployment or pre integration changes.

## Source-derived closed query set

`palw_court_tir_requests_v1::TirSharedRequestSetV1` derives its geometry from the actual `PalwTirPublicSourceV1` and checks source/header identity against the supplied original frame, including its canonical original claim hash. A nonzero shared profile identity is required, but is not treated as an approval record or authentication of accepted state. Its caller still must resolve the actual accepted frame/profile.

The set is every request shape accepted by the CURRENT native source validator, with a deterministic family/coordinate order:

| Family | Complete coordinates |
| --- | --- |
| StepNode | Every interior level and every native width index, including non-bottom levels |
| StepLeaf | Every original step leaf |
| StepRun | Every legal count up to the existing RUN maximum and every valid start for that count; overlapping runs are retained |
| RowsNode | Every interior row-tree level/index when tiled trace is supported |
| TileNode | Every original decode row and every interior tile-tree level/index |
| TraceTile | Every original decode row and vocab tile |
| Ids | Every canonical existing chunk offset, including the final partial chunk |

The native source's own tiled/Flat/Float availability restrictions are retained, not a new model exclusion or a statement that all G14 routes are covered. G14 still has to cover the broader claim universe through its actual routes. The shape set is not an adopted restriction on a future public request codec; changes to legal requests require updating the versioned universe and its evidence.

## Cardinality and access

For N leaves, all runs contribute `sum(N - c + 1)` for every permitted c. Tree levels use the actual native tree-width function, so odd, singleton and non-power-of-two geometries use the same widths as validation. Rows multiply tile-node counts by exact decode count. All sums and products are checked; overflow refuses instead of truncating or raising a cap.

Counting stores seven family counts and scalar geometry. `request_at(ordinal)` selects a family, level or run-count without materializing a vector of all requests. It validates the resulting request with the real native validator as a final check. Out-of-set ordinals refuse. This does not imply that a book storing every eventual packet has constant memory or that all legal requests fit the hard deadline.

Shared request identity hashes version, original claim, native allocation instance identity (including its immutable accepting clocks), native rules identity, proposed shared profile, source identity and canonical request bytes. It has no owner, queue, relayer, current wall clock or participation input. A different shared profile gets a different namespace. Hashes establish scope/data identity only, not computation correctness.

## Capacity and native integration still required

This is a query identity, NOT a complete service packet or duty identity. Signer roles, response eligibility, turn, fragment, original hard/followup/retention and full codec/work/IO/memory/control costs must still be derived from the actual adopted profile and bound to the native service book. It has no reservation or default-permission caller and cannot arm the existing fail-closed gates.

Enumerating all legal queries is conservative with respect to a protocol that restricts reachable choices by phase. It must not be confused with proof of the complete interactive obligation graph. Conversely, any narrower reachable set needs a deterministic derivation from the actual protocol, including every hostile choice; a sampled localizer path is insufficient. Reusing authenticated data between overlapping queries may save transmission, but the transformation, openings, signer work and state/retention costs must be explicit. The cardinality is not a lower bound requiring every overlapping representation to be separately carried on chain.

Original atomic admission, actual accepting-block supply, fair inclusion assumptions, owner-independent public request/response ingress, all-owner collusion, recovery/end effects and full T05/T14 remain open. No unknown rate or cost is replaced with zero. Existing 3,120 / 10,000 and wire/rules identities are not changed to make the set fit.

## Test scope

The small native frame/source fixture enumerates a deliberately larger rectangular candidate space, filters it with the actual `check_request_v1`, and compares the full canonical serialized set to ordinal enumeration. It checks uniqueness, profile namespace separation, zero-profile refusal and terminal ordinal refusal. Separate arithmetic-only geometries test odd boundaries, partial ID chunks, disabled trace families, large synthetic N and checked overflow. Those synthetic cases are not measured models or network capacity, and this component test does not constitute node T05.

## Native logical-request distinction — integration candidate

The set above remains the source-query set. On the existing owned route, only `Ids { offset: 0 }` is a legal logical Demand; it carries the concatenation of every source ID chunk. The candidate now derives a separate native logical-request set and response subgraph, preserving these query IDs. See [native response derivation and limits](rfc26-native-response-graph-2026-10-20.md). Source-query cardinality must not be substituted for funded native obligation count or first-chunk bytes for the all-ID body ceiling.
