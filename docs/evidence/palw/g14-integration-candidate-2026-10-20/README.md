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
