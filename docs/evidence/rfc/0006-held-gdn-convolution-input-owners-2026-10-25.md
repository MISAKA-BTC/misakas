# RFC-0006 held-GDN convolution input owners — 2026-10-25

## Production roles

The actual held convolution builder independently allocated required-coordinate rows, copied
input tile rows, built per-row contiguous runs and sibling paths, cloned its binding, and built
a whole step-tree level for each range opening. The separately retained accused witness did not
cover these allocations. This continuation admits these additional convolution-construction
roles and carries their persistent owner alongside the separate witness owner through the
actual node court and signed-output handoff.

The allocating native `gdn_conv_window_required_v2` and the new request scan now share one
borrowed row visitor. It uses the existing native convolution source/range/checkpoint helpers;
there is no second interpretation of q/k/v widths, FullConv versus head gathering, cadence or
coordinate order. There are `3*min(checkpoint_positions, conv_kernel)` source rows. Each actual
head/source tile interval determines its count, and canonical consecutive indices determine its
actual run count. Borrowed tile lookup uses the same immutable material/first-index match as
the later clone; source absence or changed coordinate is a native Evidence refusal.

The persistent request includes the actual binding native heap, outer input-row slots, actual
preimage slots, selected borrowed tile native heaps, actual run-vector slots and sibling-vector
roles. Paths are bounded by two siblings per actual tree level, using typed push growth and
old/new overlap. Workspace includes outer/inner required-coordinate vectors, the peak current
row's run list, and the sequential range-opening tree peak `N+ceil(N/2)` Hash64 entries plus
the actual 256-byte Merkle-leaf Writer. Range trees are built sequentially, so this tree peak is
not multiplied by the number of runs. Requests use native type sizes, actual counts and heap
visitors, not serialized bytes multiplied into RAM or a physical allocator/RSS estimate.

Both persistent and workspace tickets are acquired before these native arrays/clones/openings.
After assembly, native output capacities and row/tile/run counts are checked while admission
remains borrowed and live. A fallible check does not consume its ticket before the output can
retire. The validated persistent ticket transfers infallibly into the existing witness-bearing
guard; temporary workspace retires. The actual node's existing consuming callback checks the
court and admits signed output before the input guards retire. The generic callback type alone
makes no retained-output guarantee; the concrete action owner fulfills that obligation.

## Coordinate-rank allocation discovered during review

The original proposed visitor called `canonical_step_leaf_index`, which internally constructed
private `PalwLeafShapeV1.body_kv/logits_kv` vectors before any complete request existed. That draft
was not connected as an allocation-free preflight. The implemented native cached-rank context
counts actual private `PalwKvTermV1` values through the same table/term visitor used by the
legacy shape constructor, admits a checked type-sized logical envelope, then uses fallible
`try_reserve_exact` for both arrays before shared filling. This is resource admission, not a
claim of minimal allocation cost, allocator overhead or RSS. The current rank request uses
`term_slots` counts; the convolution push-built roles separately retain conservative growth
and old/new overlap envelopes. These arrays drop before their owner. Rank queries use the
cached shape and allocate no vectors.

Historical and cached rank calls share the exact same rank body and table/position ordering;
the historical early-refusal path is preserved. The row visitor owns an admitted rank context
through coordinate traversal; the actual immutable Base0 clone source owns its own admitted
rank context through construction. These are separate real caches with separate tickets, not
shadow resource multipliers. They retire before the returned persistent input guard. Unrelated
canonical-index consumers retain their existing interface and scope.

## Verification status

The parent's actual Core build passed in 3m06s in
`/tmp/misaka-rfc-conv-rank-staged-core-build-20261025.log`. New cached-rank tests passed **2/2 in
0.07 seconds**, including the isolated quota child (0.00 seconds), in
`/tmp/misaka-rfc-cached-rank-core-tests-20261025.log`. Children are included in their parent counts.
The existing coordinate oracle passed **1/1 in 3.21 seconds** in
`/tmp/misaka-rfc-cached-rank-coordinates-regression-20261025.log`, and the existing job oracle
passed **1/1 in 6.63 seconds** in `/tmp/misaka-rfc-cached-rank-job-regression-20261025.log`.
The existing shape oracle also passed **1/1 in 48.75 seconds** in
`/tmp/misaka-rfc-cached-rank-shape-regression-20261025.log`, completing all three named legacy
regressions.

The initial actual node build passed in 5m27s, with pinned binary
`/tmp/misaka-rfc-conv-staged-filer-node-20261025`, SHA-256
`1b8863e2cff79ee453e7babd6b60a045632b36948b224a48fa2a0a00d1097ba4`.
Its new node filter was **1/2 PASS, 1 FAIL in 2.96 seconds** (failed child 1.48 seconds), logged
in `/tmp/misaka-rfc-conv-staged-node-conv-tests-20261025.log`. The assertion at test line 112
incorrectly assumed that this all-Fixed fused-attention fixture needed positive-sized KvTerm
arrays, and required Resource before borrowed reads. It actually has zero such terms, so the
borrowed scan correctly succeeded. Only the test was corrected to require that success and the
later nonzero construction-admission refusal; no artificial reservation or production relaxation
was introduced. The original failure is not counted as acceptance.

The corrected new node filter passed **2/2 in 2.61 seconds**, including its child (**1/1 in
1.32 seconds**), in `/tmp/misaka-rfc-conv-zero-cache-node-tests-retry1-20261025.log`. Retry build
passed in 2m52s in `/tmp/misaka-rfc-conv-zero-cache-node-build-retry1-20261025.log`. Pinned retry
binary: `/tmp/misaka-rfc-conv-zero-cache-node-retry1-20261025`, SHA-256
`5fcad12b3d57e2eac4828a8585b7a7c2b3deed039a0251da50e3a06e1bf9efe6`.
The correction snapshot is `/tmp/misaka-rfc-conv-zero-cache-test-correction-source-20261025`.
GDN production/Core/Base0 source was unchanged for this test-only correction; the retry also
contains separately tracked FIFO-cache (`f73f1d549`) and TIR (`d616c22c8`) work. This snapshot is
source identity, not a K3 input or campaign artifact.

The same initial pinned node binary passed the existing held-filer regressions **24/24 in
100.26 seconds** in `/tmp/misaka-rfc-conv-staged-node-held-regression-20261025.log`, with
`--skip conv_owner_tests` explicitly excluding the two new cases. This includes prior witness
and court controls updated to the real v4 construction prerequisites and confirms the new
production convolution route under those existing cases. It is not a full 26-case PASS and does
not replace the corrected new-filter run above. These two artifacts/runs are kept distinct; no
single-binary full 26-case result is claimed. Elapsed time is not a model cost measurement.

Pinned Core binary: `/tmp/misaka-rfc-conv-rank-staged-core-20261025`, SHA-256
`01387143005cc8461e3989f862163ff79375a9d8630496e64a1b6bb6915e8fdf`.
Build duration is not a model cost measurement. No Cargo invocation was started by this worker.
New leaves were formatted individually, existing modified files passed parser-only checks, and
scoped `git diff --check` passed. Docs checker passed with 43 baseline missing references and
zero new regressions, not a clean strict audit.

New Core filter: `palw_step::tests::cached_rank_tests_v3` (two parents, one isolated quota child
included). Cached rank is compared with the legacy call over the actual existing shipped-profile
corpus plus its synthetic edge profiles, table boundaries, first/last head tile indices, invalid
coordinates and context extremes. The admission child refuses before cache construction,
preserves the input, retries and checks array-owner lifetime. Existing closed-form rank/cost
regressions remain relevant; matching helper implementations alone are not model verification.

New node filter: `palw_gdn_held_filer::tests::conv_owner_tests` (two parents, one isolated quota
child included). An actual held-v5 producer fixture compares the shared visitor/native required
rows for first/last heads, checks cached/legacy indices and quote row/tile/run counts, and rejects
an out-of-range head. The child observes that the actual fused-attention held profile has only Fixed nodes, so its
zero-sized cached rank and borrowed scan legitimately succeed even with quota fully occupied.
It requires identical scan/count results, then refuses the nonzero complete-role admission
before body construction, preserves the case and distinguishes missing source from Resource.
Positive-sized rank-cache quota refusal is separately covered by the Core child. After retry it drives the real retained convolution builder, native court, signed
handoff and actual retained filer, observing independent persistent tickets through retirement.

Prior witness-owner exact-ticket checks now include the additional convolution input ticket.
The court Resource test requires actual v4 construction to succeed under its test budget before
claiming a subsequent native court refusal; an earlier new-construction refusal cannot count as
that target control. Runtime results and artifact identities above distinguish the initial failure, prior regression
run and corrected new-filter run without adding overlapping counts.

## Explicit remaining boundary

This closes the named convolution required/input/binding/range-opening construction roles.
The existing native demand-equality check before the full role request still constructs its own
demand vector/layout metadata and remains a separate admission boundary. The new path is not
claimed to be reserved before every allocation in the whole filer. Initial Base0 leg rebuild
and cached-rank admission have their own tickets; the accused witness has its own copy ticket.

Transition predecessor replay/witness construction, initial prefix state construction,
authentication scratch outside the quoted constructor, generic case-book/history maps and
PerDecodeCall served-chunk/frontier reconstruction remain open. No transition or arbitrary-prefix
bound is invented. Generic NativeHeap coverage errors remain typed Resource, and existing backend
String refusals are not classified by their text. This does not close all GDN/court/aux consumers.

Formal K3 specification/manifest/weights/signed inputs remain uncreated and fifteen campaign gates
remain UNKNOWN. Small native fixtures are not all-model costs or an accepted multi-process court
campaign. Public activation remains None, ordinary selected-source/V6 bootstrap refusal and model
tiers remain, and selected-verifier claim authentication is unchanged.
