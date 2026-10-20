# Isolated G14 integration candidate evidence — 2026-10-20

Source: X3 `0c3cb9f93`, selected main `0f808b81d`; candidate branch `codex/rfc14-g14-integration-candidate`. Authorized scope: ADR-0194 A implementation and testing only. No pre merge, production adoption, activation or deployment.

[Candidate scope and remaining work](../../../design/palw/integration/g14-isolated-integration-candidate-2026-10-20.md). Source-branch historical results are not added to joint-build counts. [Imported assets](imported-assets-comparison.json): 91 original files byte-identical to the main source, including the original user ZIP (SHA-256 `ec23d813ce1f6fcd8c5f7d78234f93ca48acb71df9306d8317711d10a96c0eb0`). [X3 source schema audit](x3-schema-prefix-comparison.json): key 0..11 and row 0..17 unchanged, service appended at 12/18. These comparisons are not network/capacity measurements.

## Intermediate failures, preserved

The first core compile found two imported test calls still passing owned frames into X3's borrowed API; corrected on the test side. The next compile ran after heap inventory regeneration but before the companion wire generator: nine missing scanner traits. Running both generators in order restored the complete inventory/scanner set. Private-owner scanner output then has no unrelated diff against X3. No production field was removed to bypass coverage.

The first actual native run (`cargo test --release -p kaspa-consensus-core --lib palw_tir_public_fold_v1`) completed with **25 passed / 1 failed / 0 ignored**. Its recovery test imported a main-only geometry assumption. Actual X3 synthetic fixture: acceptance 100, preparation end 4,493, owner interval 4,496..4,506, original hard 10,100. Reservation correctly succeeded. The test now exercises the X3 owner interval, separately retaining atomic rejection beyond original hard. This does not prove all-branch physical service or measured door feasibility. Production clocks, limits and identity pins did not change.

| Raw log | SHA-256 |
| --- | --- |
| [initial-api-compile-failure.log](initial-api-compile-failure.log) | `cb71f0169a37105d52697426cabc39e5043133d061cf528b743e30148b87ec61` |
| [intermediate-scanner-compile-failure.log](intermediate-scanner-compile-failure.log) | `54eb6d47f78a8c0e5f79e1cae03cef96c13737a42866b486e6343beec8b5ccc0` |
| [initial-joint-native-fixture-failure.log](initial-joint-native-fixture-failure.log) | `280f0e89badfffe48040a2883e63887bb8420c408da40ab1ffead693f5a6c59f` |

## Final selected joint-build verification

All cargo and release executable runs below used `/Users/wata/.cache/misaka-heavy-lock.sh` in this candidate worktree. Production sources, t12 identity expectations, clocks, caps and record-list adoption state were unchanged during the final runs. Only the imported branch-specific test premise was corrected. Final selected total: **93 passed / 0 failed / 0 ignored**. Intermediate passes/failures above are not added. R-D2-NODE's two ignored tests were not selected and remain unresolved; this total is not a full node-suite or T01–T15 acceptance count.

- `cargo test --release -p kaspa-consensus-core --lib palw_tir_public_fold_v1`: 26 passes, including holds, recovery in the X3 owner interval, native journal undo/replay, carriage reload, custody, streamed fraud and existing X3 ownership/transport/court regressions.
- `cargo test --release -p kaspa-consensus-core --test palw_court_policy_v1 --test palw_court_service_v1 --test palw_court_service_state_v2 --test palw_court_shared_frontier_volume --test g14_close_gates --test palw_activation_pool_is_t12_only --test palw_the_release_did_not_move`: 49 passes. This freshly builds service version/profile/tag/heap/scanner checks and the 26 release identity/gate/activation regressions.
- `cargo test --release -p kaspa-consensus-core --test palw_court_transport_v1 --test palw_tir_filing_clock_v2`: 3 passes. No profile adoption.
- The SAME freshly built core unit executable `target/release/deps/kaspa_consensus_core-cd85ed321316b157` ran `court_tir_shared_requests`, `palw_court_transport_v1` and `palw_tir_frame_cost_v1`: 2 + 2 + 5 passes.
- `cargo test --release -p kaspa-consensus --lib lg14a_a_newcomer_funds_and_relays`: 1 pass. Actual funded legacy carrier, virtual UTXOs, full selected-block import, equal root and a fresh authenticated read; not process restart, pruned import or TIR-original/service ingress.
- The SAME freshly built node unit executable `target/release/deps/kaspa_consensus-8369d24fd1f110cf` ran `x3_req_5` and `lg14a_a_non_seat_newcomer_convicts_a_computation_lie_before_final`: 4 + 1 passes. X3 shallow reorg, bounded clock and isolated genesis caveats remain. Node/core non-test dependencies compiled successfully. Warnings remain.

[Machine-readable counts and executable hashes](final-selected-results.json). Runtime output is evidence of test execution, not measured network throughput or economic sufficiency. Original admission, actual supply and full interactive duty graph remain incomplete; real fences stay dormant and production default authorization stays unavailable. The successful actual-signature relay tests do not replace availability for fresh adaptive answers.

| Raw log | Passed | SHA-256 |
| --- | --- | --- |
| [native-corrected-release.log](native-corrected-release.log) | 26 | `64399f363b89a0c155b896bba0329301b98eccf09f4ed6b0e344333b6f6a90a8` |
| [components-identities-release.log](components-identities-release.log) | 49 | `dc829bed4ec485f80a0830d72e857e388c75285d489d028214fa0ee3f94ee0ad` |
| [x3-profiles-release.log](x3-profiles-release.log) | 3 | `8d2140b78986cf55b09988c914dd07302e784a1b0b353d9c0c85d117cff3af17` |
| [funded-relay-node-release.log](funded-relay-node-release.log) | 1 | `099b15916917a8b150073d1eefa1ff0dbb2b446baaa049757e1037cf9bfbfba8` |
| [node-regressions-release.log](node-regressions-release.log) | 5 | `93c30afc3e4ef4e9162cf6002fa3d0dda2dbfa53ec5bcb0892ac5ce97caa2af9` |
| [core-companion-replay-release.log](core-companion-replay-release.log) | 9 | `c6f284df064dce5879a0ee666853d66eb92a76a9cc6a9c4b7ddb100ad0bf59f7` |

Documentation check remains FAIL on the same five imported missing references (43 baseline-known occurrences). No new authored reference defect was added; the baseline and raw imported material were not rewritten.

Candidate implementation checkpoint: `b9c6d0656`. Full imported diff whitespace inspection reports original raw asset/log formatting, including trailing blank lines. Those source bytes and digests are intentionally preserved. The authored consensus, ADR, design, maintenance and revised normative-document diff passes `git diff --check`; this is not a claim that raw imported material was normalized.

## Native logical requests and response subgraph

After `aef0d55e0`, source queries retain their IDs. A separate native logical-request set collapses the source ID chunks into the existing owned route's all-ID `Ids{0}` body, using `request_body_bound_v1`. Native response stage generation uses the existing part bound and body hash. The prior native producer-answer helper now sends the generated actions through the actual dispatcher, rooted chunk rows and Finish authentication.

Final shared-lock runs:

- `cargo test --release -p kaspa-consensus-core --lib court_tir_`: **5 passed** (includes the 2 existing source-query tests).
- The SAME freshly built `target/release/deps/kaspa_consensus_core-cd85ed321316b157 palw_tir_public_fold_v1`: **26 passed**. This is a release executable replay, not another compile.
- Fresh `cargo test --release -p kaspa-consensus-core --test g14_close_gates --test palw_activation_pool_is_t12_only --test palw_the_release_did_not_move`: **26 passed**; followed under the same lock by `cargo check --release -p kaspa-consensus`, success with warnings.

Selected **57 passed / 0 failed / 0 ignored** for this change; earlier checkpoint counts are not added. [Derivation and scope](../../../design/palw/integration/rfc26-native-response-graph-2026-10-20.md). Exact logical body bounds and fragment dependencies are not a complete interactive service graph, measured capacity, signer eligibility, adopted profile or node T05. Synthetic length/ID/maximum-integer checks are not model measurements. Existing clocks, caps, tags, source query IDs and identity expectations are unchanged. Documentation check has the same five raw imported reference defects.

| Raw log | Passed | SHA-256 |
| --- | --- | --- |
| [native-demand-graph-release.log](native-demand-graph-release.log) | 5 | `430405ede9b83348c8d0e8e68e4cb031b9542c7f54cc40b38ba8906a0933c377` |
| [native-demand-dispatch-replay.log](native-demand-dispatch-replay.log) | 26 | `797c5a55ad85d2420e1599c39f76d5b7f48827344f5501f6001ac143fa2190ad` |
| [native-demand-identities-check.log](native-demand-identities-check.log) | 26 | `696c1fd8faad63b16c82d255dde37ae75f08af3ca21919613c83cd764ce0e751` |

## Devnet observations now available

The operator's source `799a058fa` report and 1,180-line raw log are imported verbatim. [Reaggregation audit](../../../measurements/2026-10-19-devnet-payload-carrier-audit.md) separates source claims from directly reproduced observations, including duplicate stage-boundary block IDs, total-block versus floor-only bytes, first-mempool versus sender timestamps and header versus native accepting clocks. The reaggregation script was run twice with identical derived JSON; input blobs match the pinned source. This is not a new devnet run or capacity calibration and does not add node-test passes to any total.

## Actual native response-duty binding

After `bc4f3caa0`, the response duty reader derives literal Ready actor, original source/allocation, actual owner-scoped demand/turn, response clock, body stage/cursor and remaining native moves from native state. It preserves expired pending metadata as a record, without extending clocks or granting late-response authority. The native default guard for a service-bound acquisition now requires each purported unpaid actor to have this actual outstanding duty and a response deadline inside original hard, before reaching the existing denial-only service guard. Other routes and the missing complete profile are not bypassed.

Final shared-lock runs:

- `cargo test --release -p kaspa-consensus-core --lib palw_tir_public_fold_v1`: **27 passed**. Existing actual native response fixtures read each current duty before dispatching the generated action and check completion after Finish; the additional boundary/provenance test rejects missing original frame/preparation, owner-as-custodian substitution and invalid keys, and checks profile namespace separation, while expiry alone preserves owed identity.
- The SAME freshly built `target/release/deps/kaspa_consensus_core-cd85ed321316b157 court_tir_`: **5 passed**, executable replay.
- Fresh `cargo test --release -p kaspa-consensus-core --test g14_close_gates --test palw_activation_pool_is_t12_only --test palw_the_release_did_not_move`: **26 passed**, followed under the lock by `cargo check --release -p kaspa-consensus`, success with warnings.

Selected **58 passed / 0 failed / 0 ignored**; historical overlapping totals are not added. [Scope, authority and unpriced additional validation work](../../../design/palw/integration/rfc26-native-response-duty-2026-10-20.md). This is not packet-to-duty reservation admission, service-supply recording, a complete interaction graph, a portable admission certificate, default permission or node T05/T08. The owner-scoped current duty ID must not be used as an owner-independent shared packet ID. Physical delivery, full dependency/end policy, all-branch graph and adopted numeric profiles remain open; clocks, caps, tags, empty real-network arming records and identity expectations remain unchanged.

| Raw log | Passed | SHA-256 |
| --- | --- | --- |
| [native-duty-dispatch-release.log](native-duty-dispatch-release.log) | 27 | `43a8af8d2d3d5f9e2661bb0f1e24ad62296213dff392fd6f32bb33cb5e577323` |
| [native-duty-graph-replay.log](native-duty-graph-replay.log) | 5 | `3844bc48445f4baedd96f1a3497d9841c9989eb5dc32496bf8d8a44284e30721` |
| [native-duty-identities-check.log](native-duty-identities-check.log) | 26 | `ba6bc2633cb5ad4a4fe2b60a5230cd4469efb0d8cb96d1d457f8cb62ad4e92c9` |

## Retained stage timing counterexample

Only a cfg(test) native fixture was added after `11a9b2b0e`; production Rust is unchanged. Early and late valid Begin/Part transitions yield equal retained acquisition rows and current duty views, but straddle the assigned service DAA. This demonstrates missing predecessor timing in those retained fields, not identical full node histories, impossibility of journal replay, measured throughput or a violation by the existing resource-only service book.

[ADR-0195](../../../adr/0195-rooted-court-response-eligibility-history.md) and [RFC-0028](../../../rfc/0028-rooted-court-response-eligibility-history.md) propose a separate rooted history. Runtime markers are **not implemented**, and the Proposed design does not authorize defaults, schema adoption or arming. Whole-path causality and original complete admission remain necessary.

Targeted fresh cargo build: **1 passed**, included again in the freshly compiled core executable's native **28 passed**. Prior checkpoint identity/gate/activation/release executables were replayed (**12 + 5 + 9 passed**), not freshly compiled now. Unique selected total: **54 passed / 0 failed / 0 ignored**; do not add the targeted 1 or historical checkpoints. Every release run used the shared heavy lock. [Commands and counts](stage-clock-counterexample-artifact.json).

| Raw log | Passed groups | SHA-256 |
| --- | --- | --- |

| [stage-clock-counterexample-release.log](stage-clock-counterexample-release.log) | 1 | `b88a0bfe17328b82c66a8ac7c677839d27b1b7b2b017e15f1cba86467321dc9c` |

| [stage-clock-counterexample-regressions.log](stage-clock-counterexample-regressions.log) | 28 + 12 + 5 + 9 | `56a2d8d4cb9b089787ca086da07b502a90b3b49ecd63b182a1c16ee4006d9385` |


Documentation check still reports the same five imported missing references plus 43 baseline-known occurrences; no baseline or raw imported evidence was rewritten. The current authored diff passes whitespace checks.

## Rooted native prerequisite history candidate

After `e82921ee1`, [RFC-0028](../../../rfc/0028-rooted-court-response-eligibility-history.md) has an isolated runtime candidate. `CourtDutyClockV1` payload 1 uses appended key/row tags 13/19, preserving service 12/18, X3 container 11/17 and old action/lifecycle tags. It records actual native context when a service-bound Demand/Reacquire/Begin/Part creates a different Ready duty. Late binding cannot backfill; voluntary owner answers create no Ready debt. WorkDebit, Finish and reads do not restamp history. Original service/profile/acquisition/actor scope is checked on writes and snapshot consistency. Markers are immutable; their binding cannot be silently removed/substituted.

Stage and marker writes share the existing `StagedCourtSeed` journal. A checkpoint restores both on failure at the bound verified-half boundary. The added checkpoint, source/duty reads, writes, scans, indices and retained bytes have uncalibrated costs; no free work or adopted profile is assumed. Original material TTL stays unchanged; unresolved service holds retain historical rows beyond it without extending any signed service promise.

Before the exclusive-hard-bound correction, selected **64 passed / 0 failed / 0 ignored** (source preserved in [the prior patch](duty-clock-pre-hard-boundary.patch)):

- Fresh targeted native build: **2 passed**, ALREADY contained in the same freshly built core unit executable's native replay **30 passed**. Do not add the targeted 2, intermediate 1 or historical checkpoints.
- Fresh marker codec/heap/scanner **2**, existing service-state **6**, identity/G14/activation/release **26**: **34 passed**, rebuilt against this candidate.
- `cargo check --release -p kaspa-consensus`: success, with warnings.
- Generator idempotence: success, including comparison with staged outputs. Its raw “committed” wording refers to the Git index at execution; those bytes are included here.

The native fixtures check actual Demand provenance, absent old-stage history on late binding, early/late accepting contexts, same-block/same-DAA responses, latest legal Part leaving no eligible future Finish unit, unchanged deadline, denied default, history retention after Finish/TTL, immutable-marker refusal with stage/journal rollback, binding/profile substitution refusal, journal undo/replay and full in-memory carriage reload. The conflicting-history failure deliberately injects contradictory provenance as a writer-failure fixture, not a claimed valid accepted node history. Signatures/original admission use the existing verified-half harness. Actual node ingress, signer outage, process restart, pruned import, full fork campaign, flood and all legal WorkDebit/restart paths are not established here.

Initial compilation failed because the generated layout resolved the wire field's short `Key` alias to an unrelated local alias, also traversing an already owner-local type and producing duplicate trait implementations. Using the canonical field type fixes the graph. Regeneration reports zero unresolved types; no hand deletion of generated visitors was used as the fix. The intermediate single native case predates the final binding/atomicity/boundary additions. These logs are retained and excluded from the final total. The first metadata-save attempt used `hashlib.file_digest`, unavailable in system Python; portable streaming SHA-256 completed the artifact without changing source or measurement data.

[Commands, source/executable hashes, counts and scope](duty-clock-artifact.json).

| Raw log | Scope / passed groups | SHA-256 |
| --- | --- | --- |
| [duty-clock-first-release.log](duty-clock-first-release.log) | initial compile failure: [] | `9a4898fda620d19ca6c596fb83ecc492e12182b300069aa38f339b89db74594a` |
| [duty-clock-native-release.log](duty-clock-native-release.log) | intermediate prototype: [1] | `4b78ba7d98a3cb70f843d568230e0d49cd4b8d9c282a75aa5dcb71ddbcb65c6b` |
| [duty-clock-final-native-release.log](duty-clock-final-native-release.log) | final targeted native: [2] | `1acad35704e0b377a296508c2636dc8ecdb7967626a4c82f383eb630ca0a04ff` |
| [duty-clock-components-check-release.log](duty-clock-components-check-release.log) | final components and non-test check: [12, 5, 2, 6, 9] | `64ba0c865b0d220cfda50fbcd88cd16b5168c67f4dacfc806bd4b7dc1fcae6db` |
| [duty-clock-full-native-replay.log](duty-clock-full-native-replay.log) | final native replay: [30] | `60bedf8033097ddeab48abbbb662adc281db5c0f41ef53d5d68cd0388eb2a2da` |
| [duty-clock-wire-idempotent.log](duty-clock-wire-idempotent.log) | generator idempotence: [] | `f37ab49c54bf5b109caa581b2e6b1535ca0dc30de9442d0b7df178ef441dfb02` |

Timing history is necessary input, not slash permission. Delay of earlier stages is not automatically a defense, and aggregate reserved capacity is not blame. Full causal graph/original all-branch reservation, owner-independent shared ingress, actual supplies, physical calibration/delivery assumptions, signer/dependency/end policy and T01–T15 remain open. No cap, clock, identity expectation, adopted numeric profile, real-network arming record, pre integration or deployment changed.

## Exclusive hard-boundary verification

Final review found that the auxiliary timing predicate included the original hard DAA while the existing service packet interval excludes its `service_before` upper bound. The candidate now uses `first_eligible <= unit < min(response_deadline, original_hard)`, with an explicit equal-hard negative fixture. This changes no native response deadline or admission rule and grants no default authority. The unit-cost book in `bind_clock_service` is a synthetic metadata fixture, not measured/adopted capacity or fees.

After that correction, fresh targeted native **2 passed**, fresh integrations **34 passed** (marker 2, service 6, identity/gates 26), non-test consensus check succeeded, and the SAME freshly built unit executable replayed the full native module **30 passed**. Unique selected **64 passed / 0 failed / 0 ignored**; the targeted 2 are contained in 30. Do not add the earlier 64 or historical totals. Source fields did not change; a repeated generator check also succeeded against the staged outputs. [Final commands, hashes, source patch, counts and scope](duty-clock-hard-bound-artifact.json). The prior artifact/source patch and all raw logs are retained.

| Final raw log | Passed groups | SHA-256 |
| --- | --- | --- |
| [duty-clock-hard-bound-native-release.log](duty-clock-hard-bound-native-release.log) | [2] | `d16f6b5fe993ab74d73ba20f8d3592b1839633d8118378857eaeae5c12ad2c58` |
| [duty-clock-hard-bound-components-replay.log](duty-clock-hard-bound-components-replay.log) | [12, 5, 2, 6, 9, 30] | `9b00dee4efb1eaa09fd6ddb0db460e45f1240c2eef7e899afa759e9f3ce3b6a3` |

The scope and all incomplete contracts in the preceding section remain unchanged. Documentation check still reports the same five imported missing references and 43 baseline-known occurrences; no baseline/raw material was rewritten.

## Owner-independent shared source data candidate

After `705b27616`, [RFC-0029](../../../rfc/0029-owner-independent-shared-tir-query-data.md) / [ADR-0196](../../../adr/0196-owner-independent-shared-tir-query-data.md) add an isolated **in-memory** shared-data primitive. It has no native lifecycle tag, rooted frontier/admission, response-duty completion, fee exemption, credit/default/activation effect. The original frame/source/profile query view remains a caller responsibility, not a producer DTO certificate.

A new universe scope ID binds original claim/allocation/native rules/profile/source/cardinality; existing query IDs are unchanged. Real ML-DSA-87 request intents resolve literal bond keys from state, coalesce duplicate queries across requesters and preserve different legal choices. Public reads have no requester/owner credential. Typed source-authenticated units may be shared; raw ID chunks stay provisional until the complete outer-root-authenticated sequence exists. Public data identity never certifies model computation or a designated signer's duty completion. Intent decoding checks the fixed signature length before allocation; scope/key/network/context/ordinal substitution changes no cache state.

The new source-data bound sums every legal query's existing body/parser/hash/resident-unit-RAM upper components with O(tree height + existing run limit) work. It does not enumerate the universe in memory. Tiny native-unit enumeration, ragged frontier domination and overflow tests check the aggregation. These are partial derived uppers, not physical rate/fee/admission guarantees or a lower bound excluding optimized reuse. Full-ID EOF/dependencies, signatures/carriage/fragments/IO/new metadata/retention and the complete interactive graph remain to price and reserve. No missing item becomes zero.

Final selected **67 passed / 0 failed / 0 ignored**: fresh source/query/frontier **11**, fresh identity/gate/activation/release **26**, SAME freshly built core unit executable native-module replay **30**. Non-test `cargo check --release -p kaspa-consensus` succeeded with warnings. Existing wire/inventory generator output remains idempotent; this does NOT establish native wire coverage for an unassigned new lifecycle. Do not add intermediate runs or the historical overlapping 64/58/57/93 counts.

The initial compile failed because new test code used private state fields; tests switched to the existing `set_bond_for_tests` and public row reader without widening production visibility. The next run was **9 passed / 2 failed**: the existing four-slice verifier API received message/signature in reverse order. The actual ML-DSA positive cases caught that error; corrected code preserves real crypto checks and tests wrong network/key/context/coordinate/version/signature plus a foreign-source authenticated unit even on an already-filled query. Those logs and the intermediate source patch are retained, never counted as final passes. An intermediate identity/native check predates that correction and is also excluded.

[Final commands, hashes, exact scopes and source patches](shared-frontier-artifact.json). [Read-only tag-245 source audit](shared-frontier-legacy-audit.json) pins `g14/door-design` and the digest-only writer, lifecycle shape arm and four dormant preset values. No third-branch merge or tag 230/245 reinterpretation occurred.

| Raw log | Scope / successful groups | SHA-256 |
| --- | --- | --- |
| [shared-frontier-first-release.log](shared-frontier-first-release.log) | initial compile error: private test fields: [] | `78da3ae0bfe375991f2d1d2045db2064c0e88a860adb2ad8e6770c10eff78674` |
| [shared-frontier-release.log](shared-frontier-release.log) | intermediate: signature verifier arguments reversed: [] | `95e81ca40e84f515cb733557a7ce9b2db85d26a19ba5bb8467b8508cdabbd0df` |
| [shared-frontier-identities-native-check.log](shared-frontier-identities-native-check.log) | intermediate regression build, before signature fix: [12, 5, 9, 30] | `553bcb092df9a3bcbca91acc98e06460538ae9b9e20ac0de50d28dd98216de45` |
| [shared-frontier-corrected-release.log](shared-frontier-corrected-release.log) | final source query/frontier cases: [11] | `f9f9c85802fcaf619ec89af1ad87b463d0953bee0c07b4e5f056e29b4d530e30` |
| [shared-frontier-final-regressions-check.log](shared-frontier-final-regressions-check.log) | final fresh identities, non-test check, native replay: [12, 5, 9, 30] | `a5c4c9abd1d9221cf7124d078f1b64c6228697b641302776306ff0e43732c2b4` |

This is NOT an actual node all-owner-coalition/refilling T05, fresh native original/payment/UTXO/profile admission, real reserved supply, signer outage, physical/economic measurement, restart/reorg/pruned IBD or G14 completion. Tests use tiny original execution and injected bond registration (zero collateral as an identity fixture, never an adopted funding floor). All native clocks/caps/t12 identity values and real-network arming records remain unchanged. Original complete reservation/public ingress, full causal/default/dependency/end policy, R-D2-NODE, tag-230 decision, actual door <= 3,120 and T01–T15 remain open.

## Rooted shared-data native adapter candidate

After `b318ea946`, [RFC-0029 §7](../../../rfc/0029-owner-independent-shared-tir-query-data.md#7-rooted-native-data-candidate-after-b318ea946) adds local key/row 14/20 (context), 15/21 (query), 16/22 (full IDs), payload versions 1. Old tags, lifecycle 230/245, existing clocks/caps, t12 identity and empty real arming records are unchanged. No public lifecycle is assigned, no production coordinator installs the context, and no object dispatcher calls these functions. Context is NOT a complete paid original, timely court notice, normal service opportunity or response-duty completion certificate.

The private native fold reconstructs current original/class/layout/source/profile, resolves an intent against that actual closed universe, verifies real requester ML-DSA and the existing Active/above-floor rule, and stores actual accepting block/DAA. Duplicate requests do not write; different coordinates coexist. Requester identity never owns shared data. Raw bodies/full IDs are source-authenticated on write/read/carriage consistency rather than trusted from a stored authenticated flag. Completed data/context/opening identity is immutable. Needed rows remain held past ordinary TTL while the service binding is unresolved, without extending signed custody promises; binding/profile removal/substitution cannot strand them. Actual raw/ID scratch allocation owners are reserved before allocation and returned runtime capabilities keep leases attached. Extra source construction, repeated authentication, metadata, copies, IO/retention still require full profile pricing/calibration; the existing source upper is not a complete new-service bound.

Final selected **69 passed / 0 failed / 0 ignored**:

- Fresh targeted native **1 passed**, ALREADY INCLUDED in the SAME freshly built unit executable's native replay **31 passed**. The initial honest-data version also passed 1 before the adversarial case was strengthened; neither targeted/intermediate 1 is added.
- SAME fresh unit executable query/frontier replay **11 passed**.
- Fresh new-schema codec/heap/scanner **1** and identity/G14/activation/release **26**, total **27 passed**.
- Non-test `cargo check --release -p kaspa-consensus`: success with warnings. Earlier core non-test check also succeeded; no tests are counted for it.
- Regenerated field/wire inventories are idempotent against staged outputs, with no unresolved native types. As elsewhere, the script's raw “committed” comparison is the Git index at execution; those generated bytes are included here.

The final native case fills all eight existing owner slots through the existing verified-half Open path and verifies the outsider's owner Open is denied. An independent shared request is then accepted with real requester signatures, a colluder's first identical request is coalesced, another choice remains, and owner count stays eight. It stores a genuinely committed arithmetic lie's opened leaf as public data while the entire claim remains unchanged: source identity does not certify computation/credit. It exercises invalid body/EOF refusal with unchanged state/journal, exact full-ID sealing, public reads, native journal undo/replay and full in-memory carriage reload. A tampered body with valid codec and recomputed storage digest is rejected on reload against the original source; tampered raw IDs also fail original outer-root validation even with their expected carriage root supplied. Historical requester absence does not invalidate the query's context check. Needed data survives ordinary TTL and cannot be generically deleted; new data/query operations fail at the unchanged original hard bound.

These are native fold/source and in-memory carriage fixtures, NOT real transaction admission/payment, a timely first notice, full original all-branch funding, a continuous coalition/refilling/latest-answer campaign, fresh custodian response signatures, actual reserved supply, calibrated physical/economic throughput, process restart/full/pruned node IBD or node T05. No data function changes a signer duty, credit, bond, refund or default verdict. Original admission, public funded/streaming ingress, full causal/dependency/end policy, actual supply, R-D2-NODE, tag230, real door <= 3,120 and T01–T15 remain open.

[Commands, exact results/overlap, hashes and source patch](rooted-shared-artifact.json).

| Raw log | Scope / selected passes | SHA-256 |
| --- | --- | --- |
| [rooted-shared-first-check.log](rooted-shared-first-check.log) | early non-test core check: [] | `79348d8604168b07c3b6c51d8eedd637b6540a8b91e0f6247c6b65622c202d61` |
| [rooted-shared-native-release.log](rooted-shared-native-release.log) | intermediate honest-data fixture: [1] | `44c8cd91292fc496bc63964a18ee038c4dcbe8c863599a27f8df144dfdee9d60` |
| [rooted-shared-authenticity-release.log](rooted-shared-authenticity-release.log) | final false-committed-data / valid-codec tamper fixture: [1] | `3480103e9f4a29383d2f09527b7253bb2ef53bb334fefb3eef6819ec9cfe3ccc` |
| [rooted-shared-final-regressions-check.log](rooted-shared-final-regressions-check.log) | final codec/identities, non-test consensus and native/query replay: [12, 5, 1, 9, 31, 11] | `001794ab66679ac32f58152dd674b8d5ac794b26eff120127a11f4c129eb4bd2` |
| [rooted-shared-wire.log](rooted-shared-wire.log) | initial schema generation: [] | `cfe33da42547d4fd9d6e08c511bb94436527f6f3d92158b0b7ac6f78505b2b49` |

## Signed bounded shared-stream envelope candidate

Parent `b980f5788`. [Source patch](shared-stream-runtime.patch) and [machine-readable commands, hashes and result groups](shared-stream-artifact.json). This checkpoint implements only a separate signed Begin/Part/Finish envelope, fixed actor/target slot identity and checked canonical part geometry. Existing 16,384-byte part and ML-DSA-87 signature limits remain. All network/claim/scope/target/signer/action fields are signed, including part bytes/offset/stream identity. FullIds is one target per scope rather than one full sequence per canonical ID chunk. A declaration change does not allocate a new actor slot.

Final selected **73 passed / 0 failed / 0 ignored**:

- [Final query/frontier release log](shared-stream-final-query-tests.log): 15, including 4 new stream tests. Real signatures cover all variant fields, wrong signer/network/context, and substitutions. Codec tests reject oversized payload/signature prefixes before payload reads, plus malformed versions, truncation/trailing bytes. Geometry covers full/final/ragged/empty/overrun/offset cases and checked u64 end.
- [Codec/identities and non-test consensus check](shared-stream-identities-check-native.log): first four groups 12 + 5 + 1 + 9 = 27; non-test check exit 0. Its earlier 31-native group is excluded.
- [Native replay after the final query build](shared-stream-final-native-replay.log): 31, using the same freshly rebuilt core unit executable as final query tests.

[Initial 4-test release log](shared-stream-envelope-tests.log) is intermediate and excluded: tests were subsequently strengthened to mutate every action field and check exact decode error kinds. These 4 are already within final 15. Do not add previous 69/67/64/93 totals. [Wire/heap generator consistency](shared-stream-wire-idempotent.log) succeeds with unchanged generated output. All cargo/release runs used the shared heavy lock.

No lifecycle tag or rooted stream/chunk row was introduced. Bond-key signature validity is not current Ready eligibility, original source/profile/cursor authentication, paid admission, EOF completion or a signer-duty effect. The whole-body adapter remains separate. Linked rooted chunks/typed leases, actual accepting clocks, EOF source authentication/atomic rollback, reload/reorg/pruning, full fee/capacity accounting and original all-branch reservation remain to connect. Part bytes are not total carrier bytes; signature/fixed fields and transaction overhead need separate accounting. Quiet-devnet 17,408-byte payload observations do not validate this envelope under load. No production fence/preset, cap, deadline, old wire meaning or identity expectation changed. This is not T05 or full G14 completion.

[Documentation check](shared-stream-docs-check.log) retains the same five imported missing references and 43 baseline-known occurrences; no baseline or original evidence was rewritten.

## Rooted signed shared-stream native adapter

Parent `33738ea26`. [Draft RFC-0029 §9](../../../rfc/0029-owner-independent-shared-tir-query-data.md#9-rooted-signed-prefix-candidate-after-33738ea26) documents the actual private adapter and remaining activation conditions. [Final source patch](rooted-stream-aligned-runtime.patch) and [schema prefix comparison](rooted-stream-final-schema-prefix.json) retain key tags 0..16 and row tags 0..22 unchanged; stream/chunk append 17/23 and 18/24. No lifecycle allocation, original production installer, fee exemption or actual reserved-supply path was added.

Ingress reconstructs the current original/source/profile/universe and original preparation, checks literal Ready eligibility and current Active/above-floor bond, and verifies real ML-DSA controls. Unit targets require an accepted request; IDs use one complete-sequence target. Begin is immutable and retries do not redate it. Parts link canonical bounded chunks with actual block/DAA. Prefixes are not authenticated public data. EOF checks the native body checksum and original source/whole-ID root, publishing data and EOF metadata under one outer checkpoint. Failure preserves prefix/root/journal. Data availability has no computation-credit, duty/default or automatic bond/material-release effect.

Chunk payloads use the existing shared ownership wrapper: full-state checkpoints share their allocations, and only an affected bounded chunk detaches on mutation. This avoids deep-copying retained prefix bytes per control. Full state/map/index costs and other existing row copies remain to be accounted; this is not a constant-time whole-transition or complete memory-profile claim. New typed leases cover assembly, provisional IDs and decoder workspace; the persistent writer handles map/journal/index owners.

The native fixtures retain all eight occupied owner slots and the outsider's rejected owner Open, then exercise the independent shared data path. Narrow source and existing 4096-wide test execution are both retained. The wide case reconstructs its actual original job context and crosses the existing Part boundary without changing bounds or deadlines. Tests cover non-Ready/forged controls, immutable/retried declarations, malformed/incomplete EOF, full/final parts, no public data before EOF, IDs EOF, wrong-source raw data, chunk corruption/missing rows, checkpoint payload sharing, journal undo/replay, native carriage reload, hard cutoff and unresolved deletion holds. These are in-memory native fixtures: original registration/Ready/signatures and payment/context installation are verified-half fixtures with the existing synthetic unit-cost service book. Newly submitted controls use actual ML-DSA bond keys; the key replacement is fixture setup, not a real rotation transaction. This is not a node carrier/funding/restart/pruned-IBD campaign or full T05/G14 acceptance.

Intermediate evidence is preserved and excluded from final pass totals:

- [Initial generator refusal](rooted-stream-wire-generation.log) required explicit review of the new handwritten bounded chunk codec. [Initial non-test check](rooted-stream-first-check.log) then reported nine missing old scanner traits because generation had stopped. Field-order/bounded-byte review was pinned, [regeneration](rooted-stream-reviewed-wire-generation.log) and [core check](rooted-stream-reviewed-check.log) succeeded. No scanner coverage was deleted to bypass the check.
- [Initial native failure](rooted-stream-native-release.log), with [matching source snapshot](rooted-stream-first-runtime.patch): the tiny source's real response bound is below the test's assumed 16,385 bytes. Bounds/deadlines were preserved. The narrow malformed input now stays within its real bound; the wider execution supplies the separate actual multipart success case. [Intermediate regression sequence](rooted-stream-regressions-check.log) stops on the earlier native executable's same failure; its final selector, containing a path typo, never executes. Its passes are not added.
- [Narrow/wide intermediate success](rooted-stream-narrow-wide-release.log), [Box-based source snapshot](rooted-stream-box-wide-runtime.patch), and [shared-ownership targeted success](rooted-stream-final-native-release.log) are intermediate, not additional distinct passes. The latter includes the checkpoint sharing assertion.
- [Pre-alignment final-attempt failure](rooted-stream-final-regressions-check.log) caught a real accounting discrepancy in the small chunk codec: shared/standard Arc wire scans omitted the conservative alignment allowance already used by typed heap ownership. [Source before correction](rooted-stream-before-alignment-runtime.patch) is retained. Both wire wrappers now match unchanged typed accounting using checked arithmetic; no hard cap or Borsh/hash identity changed. Scalar and zero-byte-payload tests expose the difference without Vec slack. [Corrected wire tests](rooted-stream-alignment-wire-tests.log) cover these and existing snapshot/manual-codec behavior.

The original graph's complete funding/fee, legal retry/restart/fault branches, fresh signer availability, original timely notice, actual reserved supply, physical carrier capacity, all T01–T15 and real-model door remain open. Old tags 230/245 are not reinterpreted, original promises/caps/clock are not extended, and real-network arming lists remain empty. Recovered quiet-devnet observations are not capacity calibration for this signed envelope.

Final selected result: **86 passed / 0 failed / 0 ignored**, non-test consensus check exit 0 and [final generator consistency](rooted-stream-aligned-wire-idempotent.log) success. [Exact commands, hashes, result groups and exclusions](rooted-stream-artifact.json): [aligned final regression log](rooted-stream-aligned-final-regressions-check.log) has 12 G14 + 5 activation + 2 codec + 9 release identity + 32 native + 15 query/frontier = 75; [wire/manual/snapshot checks](rooted-stream-alignment-wire-tests.log) add 11. Native/query/wire use the same final core unit executable. The targeted two are within the final 32 and are not added again. Historical 73/69/67/64/93 and every intermediate result above are excluded.

[Documentation check](rooted-stream-docs-check.log) retains the five imported missing references and 43 baseline-known occurrences. No new authored link defect or baseline rewrite is introduced. This is a dormant internal candidate checkpoint, not production integration/arming/deployment or full G14 completion.

## Explicit fresh data-response native candidate

Parent `fa1104eed`; [Draft RFC-0029 §10](../../../rfc/0029-owner-independent-shared-tir-query-data.md#10-explicit-fresh-data-response-candidate-after-fa1104eed), [source patch](fresh-response-runtime.patch) and [old tag prefix comparison](fresh-response-schema-prefix.json). Candidate key/row19/25 appends a receipt for `(request_id, literal signer)`; old tags/payloads and current presets/caps/clocks remain unchanged. No lifecycle or production installer was added.

A signed stream EOF or an authenticated body uploaded by a helper is not automatically a data reply by the designated signer. The separate bounded ML-DSA message binds network, original scope, request, original query block/DAA, signer and the authenticated current data fingerprint. The private native adapter reconstructs the actual original/preparation/Ready and current Active bond, validates data and signature, and immutably records the actual accepting context. Receipt retries do not redate it. A receipt does not authorize fault/default, certify computation or release credit/bond/material holds.

The query's block hash must already be known; the response cannot be in that same opening block. Later blocks may have the same DAA, and existing native controls receive no new delay. The stage reader exposes actual query/Ready/data facts and a receipt separately. Resource eligibility is unknown while data is absent, otherwise `max(query, Ready, data availability DAA) + 1` under the existing first-block accounting convention. This is not a per-turn deadline, a new funded duty or actual supply/inclusion proof. Late/missing data does not exonerate another earlier upload/preparation breach.

The narrow and existing 4096-wide native fixtures retain all eight owner slots, real request/stream/reply signatures and actual rooted public reads. New checks reject missing data, non-Ready/forged replies, wrong fingerprints, a reply in the query opening block and stale signatures after reaccepting the same query/data under another accepting context. Re-signing that new context succeeds even when data was published by the separate whole-body helper. Signed upload EOF alone leaves the reply absent; duplicate replies preserve root/journal/timestamp; hard-boundary/deletion guards and altered raw receipt metadata are rejected. Existing journal undo/replay and carriage reload now include receipts. The original/Ready/payment/context remain verified-half fixtures using the unchanged synthetic service book; alternative contexts are not a full node reorg, real fee/UTXO path or pruned IBD. Semantic court roles and physical signer availability remain separate.

Final selected **89 passed / 0 failed / 0 ignored** in the [final regression/check log](fresh-response-final-regressions-check.log): 12 G14 + 5 activation + 3 codec + 9 release identity + same final unit executable native32 + query/frontier/signatures17 + wire/manual11. This includes the [targeted two native fixtures](fresh-response-native-release.log); do not add them or historical86/73/69/67/64/93. New isolated signature tests bind every field/context and reject malformed signature lengths before allocation; their fixed envelope byte count is derived codec structure, not capacity calibration. Non-test consensus check and [generator consistency](fresh-response-wire-idempotent.log) succeed. [Exact commands, source/log/executable hashes and scopes](fresh-response-artifact.json).

[Documentation check](fresh-response-docs-check.log) retains the same five imported missing references and 43 baseline-known occurrences. No baseline or original evidence rewrite. Complete original fees/reservation, actual supply, all semantic duty/deadline/restart/fault branches, honest/delivery/signature assumptions, public carrier/funding, T01–T15, R-D2/tag230 and real-model door remain open. Real-network arming, pre merge and deployment remain absent.
