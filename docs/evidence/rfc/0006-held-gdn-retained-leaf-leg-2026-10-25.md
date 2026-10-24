# RFC-0006 retained held-GDN leaf-leg consumer — 2026-10-25

## Production connection

The actual held filer used `base0_gdn_leg_v2` during initial forged-checkpoint discovery,
candidate demand loading, and transition/convolution evidence assembly. That raw helper
reconstructed checkpoint arrays on every call without owning their admission. Earlier capture,
candidate/demand, answer and signed-action tickets did not cover these independently allocated
arrays.

A new retained PerPosition API pre-admits the existing native leaf-leg constructor. Its borrowed
request includes the actual context/profile native heap visitors (including nested graph refs
and network bytes), the two push-built leaf/hash vectors with checked native growth/overlap,
and Merkle scratch of `N + ceil(N/2)` Hash64 entries for the first pair of levels. Subsequent
levels are smaller. Constructor hash-writer scratch is also counted: CanonicalWriter starts
at 512 bytes and checkpoint/profile/Merkle-leaf Writers at 256. The actual field-write order
and bulk network-id extension drive checked Vec growth; a network append is not assumed to
behave like byte-at-a-time push. These are explicit native roles, not an encoded-byte/RAM
multiplier or a physical allocator/RSS guarantee. Hash64 and seed write lengths are read from
their actual typed byte slices rather than assumed struct layout.

Both persistent and workspace requests are acquired before the existing `from_leaves_v1`
constructor clones context/profile or any leaf. The native chain/cadence/hash checks are reused,
then binding root/count and returned capacity are checked. The immutable owner retains the
persistent arrays through caller use and drops the arrays before its ticket. It exposes no
raw extraction, Clone or mutable dereference. Temporary context/profile/Merkle/writer admission
retires after construction.

The actual held filer now selects retained counterparts for all four named callers. Shared
borrowed-leg implementations preserve their original checking behavior without a second leg
rebuild. The owner stays live through initial prefix traversal, candidate confirmation, and
transition/convolution evidence assembly. Constructor Resource propagates as
`NativeDecodeErrorV1::Resource` to `PalwGdnHeldErrorV2::Resource`; it never becomes an honest None,
an exhausted/settled candidate, an inferred timeout default or a signature. A refusal during
next-candidate construction preserves the old cursor/demands/answers/clock. Legacy raw/String
APIs retain their historical interface for callers outside this upgraded path.

This retained API supports only actual PerPosition cadence with no served chunks. Another
cadence or an incompatible served-chunk input explicitly refuses with typed Resource; there
is no raw helper fallback. The actual held path is PerPosition. Historical/per-decode-call
served-byte builders keep their existing implementation and remain outside this ownership
closure; their functionality is not represented as reserved by these new APIs.

## Verification status

The parent's actual node build passed in 2m36s. The new node filter passed **3/3 in
3.79 seconds**, including its isolated quota child (**1/1 in 1.69 seconds**, included in its
parent), in `/tmp/misaka-rfc-gdn-retained-leaf-leg-tests-20261025.log`. The complete held-filer
group passed **21/21 in 60.11 seconds**, including these same three new cases and the earlier
eighteen, in `/tmp/misaka-rfc-gdn-leaf-leg-full-held-tests-20261025.log`. These overlapping runs
are not summed as distinct cases. Build duration is not a model cost measurement.

Pinned actual node binary: `/tmp/misaka-rfc-gdn-leaf-leg-tir-owner-node-20261025`, SHA-256
`1270ec0a59854cfc532872a470ce3aadd12ae3af0be59b027a5b93e36f2c947f`.
The separate actual BASE-0 writer-growth test passed **1/1 in 0.00 seconds**, with its build
finishing in 2m37s, in `/tmp/misaka-rfc-gdn-leaf-leg-writer-growth-tests-20261025.log`.

The final source inventory is
`/tmp/misaka-rfc-gdn-leg-tir-owner-pruning-run3-source-20261025.json` (4318 entries), SHA-256
`24a27b5c8c7c016c730fbe8427acd92a679d8f3b3c60c28f54807aee19a18f4b`.
This identifies the source generation; it is not a K3 manifest or campaign acceptance artifact.

No Cargo invocation was started by this worker. New leaves were formatted individually;
existing modified files passed parser-only checks and scoped `git diff --check`. Docs checker
passed with 43 existing missing references and zero new regressions, not a clean strict audit.

Exact new node filter: `palw_gdn_held_filer::tests::leaf_leg_tests` (three parent cases, one
isolated quota child included). Actual tiny producer material checks retained/native leg root,
leaf/hash equivalence and corrupt-chain refusal. A constructor-only alternate context with a
3000-byte network ID drives bulk writer growth and native chain/root reconstruction; this is
not a registered/accepted claim or another model execution. An actual retained held-open
control requires typed refusal for PerDecodeCall even with otherwise honest material.

The isolated quota case admits its old real case/answers before arming. Initial honest open
and direct candidate demand loading refuse before any prefix forward. After releasing quota,
a wrapped actual recompute observes that the leg ticket remains live during its prefix use and
retires on return. A second exact budget permits the complete-answer batch but leaves no room
for the next leg: the real step refuses before accusation/signing and retains its old state.
Releasing the blocker permits a real terminal-checked accusation. Independent retained-leg
lifetime is checked after temporary actions retire. Children are not additional distinct cases.

Exact additional BASE-0 unit filter:
`gdn_evidence_v2::leaf_leg_owner::tests::bulk_hash_writer_growth_matches_actual_extend_reservations`.
It compares the typed Writer request against real Vec `extend_from_slice` capacity/old-new
peaks for empty, boundary and long bulk network appends. Full existing held-filer and native
GDN groups remain relevant regression filters.

## Remaining scope

Replay/prefix state construction, prompt/chunk authentication scratch, nested refutation
rows/tiles/paths, the refutation's own allocation, generic case-book/history maps and the
PerDecodeCall served-chunk/frontier constructor still require their own admission. The
borrowed-leg helpers retain existing downstream String boundaries for recompute/refutation
errors; this patch preserves the new leg admission Resource specifically and does not claim
all downstream errors have been retyped. Every such refusal still withholds rather than
becoming an authenticated NoFaultFound. Actual capture/answer/output tickets do not cover
those independent roles retroactively.

Formal K3 specification/manifest/weights/signed inputs remain uncreated. These are small
mechanism controls, not all-model costs, an accepted multi-process court campaign, or any of
the fifteen UNKNOWN gates. Activation remains None, ordinary selected-source/V6 bootstrap
refusal and model tiers remain, and selected-verifier claim authentication is unchanged.
