# G14 実装部品 / X3 — release 試験記録 (2026-10-20)

計算・ネットワーク性能の測定ではなく、fixture の試験出力。全ログは stdout/stderr の内容を変更せず保存した。通常の compiler warnings も含む。武装・統合・配備なし。全て共有 heavy build lock 経由。

| ソース | コマンド / 対象 | 結果 |
| --- | --- | --- |
| `0336b6cea` | `cargo test --release -p kaspa-consensus-core --test palw_court_service_v1 --test palw_court_policy_v1 --test palw_the_release_did_not_move --test palw_activation_pool_is_t12_only --test g14_close_gates` | 32 passed、0 failed、0 ignored |
| `e27771b96` | release `kaspa_consensus-8369d24fd1f110cf lg14a_legacy_filer_e2e --nocapture` | 33 passed、0 failed、2 ignored（R-D2-NODE） |
| 同上・同じ binary | `t12_validation_clock_preserves_the_lead_cap_and_retry --nocapture` | 1 passed、0 failed |
| `e27771b96` | `cargo test --release -p kaspa-consensus-core --test palw_the_release_did_not_move --test palw_activation_pool_is_t12_only --test g14_close_gates` | 26 passed、0 failed、0 ignored |

`kaspa_consensus` は `cargo test --release -p kaspa-consensus --lib x3_req_5 -- --nocapture` で build した同一 test binary。最初の対象試験も 4 passed。上表の関連 suite にその 4 件を含むため、重複して成功件数を合計しない。

X3 reorg は同じ DAA / blue work の浅い tie における予約・session rollback と復元。pre-Final hold が Final branch に heartbeat だけで勝つ保証ではない。新規 service 予約部品は profile・block admission・codec・実供給・default 未接続で、T04/T05 の統合達成や door の性能保証ではない。

| ログ | SHA-256 |
| --- | --- |
| [core-service-release.log](core-service-release.log) | `14a8845358a861a9a4d7553f90f2e8bbb086259df7da748a1511030648ce03f3` |
| [x3-lg14a-release.log](x3-lg14a-release.log) | `5364231ecb9b767291d132d2652e6b598b862ba959f4a63d2c093f4138c160c8` |
| [x3-clock-cap-release.log](x3-clock-cap-release.log) | `bef99f7659f2e6bce3ff76df79c4315cbfa997903a97e78cbd5a5ee826f499dd` |
| [x3-identity-release.log](x3-identity-release.log) | `412a340bbaf399b040e713780a568bc1b8e6e14a16e12e503ef16aaba55dfe22` |

## 供給会計と共有 frontier の追加記録

- `0ea164766`: `cargo test --release -p kaspa-consensus-core --test palw_court_service_v1 --test palw_court_policy_v1 --test palw_the_release_did_not_move --test palw_activation_pool_is_t12_only --test g14_close_gates` → **36 passed / 0 failed / 0 ignored**。サービス部品は 7 件。他の 29 件は前の検証と重なる。
- `4a8d862f6`: `cargo test --release -p kaspa-consensus-core --test palw_court_shared_frontier_volume` → **2 passed / 0 failed**。geometry/codec の試験で、実モデル・実帯域を測定していない。

供給会計は state/fold/codec/snapshot 未接続。`MissingSupply` と quota 提供を区別し、default の全条件や Final 保留の実装完了とは表示しない。clone の rollback は node reorg の試験ではない。容量の条件付き導出は [調査](../../../design/palw/integration/rfc26-shared-frontier-volume-2026-10-20.md)を参照。

| ログ | SHA-256 |
| --- | --- |
| [core-supply-release.log](core-supply-release.log) | `5161fb3c7d49540203ef907285504e5acd7a562ad56f38f9ed08846b4f832efd` |
| [shared-frontier-volume-release.log](shared-frontier-volume-release.log) | `0bbefd1e44dd67974187bce382da47ea049a6e6410e5deb410d08eec71b0103d` |

## Rooted service deny-only adapter

`codex/rfc14-g14-completion`, base `6edf8243b`. No integration with `pre` or the separate transport branch, no arming/deployment. These are fixture correctness tests, not capacity/CPU/IO measurements.

- `cargo test --release -p kaspa-consensus-core --lib rooted_service_guard_denies_default_and_native_undo_restores_legacy`: **1 passed**. The existing verified-half native TIR dispatcher refuses default before state/economic writes for a bound claim with missing supply; even supplied quota does not replace unadopted inclusion assumptions. An unbound legacy claim retains its existing default. Borsh row restoration and real native delta-200 undo/forward replay preserve roots. Carrier signatures and original setup are synthetic; no node ingress, full snapshot import or pruned IBD is claimed.
- `cargo test --release -p kaspa-consensus-core --test palw_court_service_state_v1 --test palw_court_service_v1 --test palw_court_policy_v1 --test palw_the_release_did_not_move --test palw_activation_pool_is_t12_only --test g14_close_gates`: **39 passed / 0 failed / 0 ignored** (3 new persisted codec/resource/binding tests, 36 existing component/gate/release regressions, including pinned t12 identity).
- `cargo check --release -p kaspa-consensus`: **success**, existing warnings remain.

All commands used the shared heavy build lock. An initial integration-test compile used a private module path; it was corrected to the existing public re-exports before the final successful test command. No resource limit, golden value, imported audit asset or baseline was changed. Production original-admission/block-supply writers, exact owner-independent duties, Final/bond/evidence/pruning recovery, physical budget closure and full node lifecycle tests remain open. The new internal row schema is Proposed, not adopted.

| Raw log | SHA-256 |
| --- | --- |
| [rooted-service-native-release.log](rooted-service-native-release.log) | `4fb7df751d829cd0611ed7cd295ff41a2861dcf3a67441b9e9787e5d0701425d` |
| [rooted-service-regressions-release.log](rooted-service-regressions-release.log) | `62330a3f3d432c59ea2f84be811766ef366093b3112f41268895fc4d6f1c5b18` |
| [rooted-service-consensus-check.log](rooted-service-consensus-check.log) | `3a6877fc7b0c7e74b74510fdbba941d9db30d0532ebb9468a3c7f74fee7ac73e` |

## Unresolved service recovery holds

Base `960729572`, dormant candidate on the same branch. No schema/preset/deadline-value change, no integration/arming/deployment. The binding remains unresolved until a reviewed terminal/recovery contract exists; this is not a complete recovery or capacity guarantee.

- `cargo test --release -p kaspa-consensus-core --lib rooted_service_`: **3 passed**. Deny-only default guard, Final/retirement/material/exit holds past original hard/TTL, original term values preserved, native delta undo/replay, full `PalwStateCarriageV2` restore with an accepting point AFTER hard/TTL, and streamed fraud adjudication while service-held. Existing synthetic verified-half carriers are used; this is not real network ingress, live restart or pruned IBD.
- `cargo test --release -p kaspa-consensus-core --test palw_court_service_state_v1 --test palw_court_service_v1 --test palw_court_policy_v1 --test palw_the_release_did_not_move --test palw_activation_pool_is_t12_only --test g14_close_gates`: **39 passed / 0 failed / 0 ignored**, including pinned t12 identity and unchanged release. These overlap the previous turn's regression targets.
- The same release lib-test executable `target/release/deps/kaspa_consensus_core-cd85ed321316b157 g14_staged_witness_assembles_in_order_and_convicts_once`: **1 passed**, preserving the unbound legacy witness consumption behavior.
- `cargo check --release -p kaspa-consensus`: **success** on the latest source; warnings remain.

All used the shared heavy build lock. An intermediate two-test native run passed before late-point snapshot and staged-witness coverage were added; it is not substituted for the final three-test log below. Review found and fixed a hold/deletion interaction that would have prevented a streamed witness from reaching fraud adjudication. A service-held claim retains the existing incomplete prefix while assembled evidence is adjudicated; it does not create a completed V1 witness row. Prefix retention is not archival storage of the complete evidence. Clearing/rearchiving that prefix belongs to the still-open terminal/recovery contract.

These checks do not prove expiry/pruning correctness across all terminal reasons or both monetary clocks, all resource bounds, ordinary/court saturation, complete supplied-duty eligibility, successful recovery/rescheduling, actual original admission, outsider access under collusion or node lifecycle closure. Those remain required. No expected identity or resource ceiling was repinned or raised.

| Raw log | SHA-256 |
| --- | --- |
| [service-holds-native-release.log](service-holds-native-release.log) | `cf1dc5635e149b95770c1632a1ec15360d8530be09e7cd862a63f057aa974645` |
| [service-holds-regressions-release.log](service-holds-regressions-release.log) | `1bd56ce29765736dcfaa54c3de51aa96a8ac6da827f009f82cfcfa04cb838aa1` |
| [service-holds-legacy-witness-release.log](service-holds-legacy-witness-release.log) | `34a430772bd841b5895c5fe4ec248e0b4ca271982137fd1151367855643424ab` |
| [service-holds-consensus-check.log](service-holds-consensus-check.log) | `3b921f7a57d99a455c20c0fa1bb35bcbf80c76d87794fd0fa6558f93da5597cb` |

## Missing-supply repair candidate

Base `d4fbc4073`, same dormant branch. No production caller or admission/supply adapter, no arming/integration/deployment. This is bounded quota repair, not a successful owner prosecution profile.

- `cargo test --release -p kaspa-consensus-core --test palw_court_service_state_v1 --test palw_court_service_v1 --test palw_court_policy_v1 --test palw_the_release_did_not_move --test palw_activation_pool_is_t12_only --test g14_close_gates`: **45 passed / 0 failed / 0 ignored**. Includes per-claim atomic repair failure, shared-slot dimensional refunds, no movement of supplied/future reservations, original interval/hard preservation, canonical multi-claim recovery, and the unchanged t12/release regressions.
- Final `cargo test --release -p kaspa-consensus-core --lib rooted_service_ -- --nocapture`: **4 passed / 0 failed / 0 ignored**. Native delta-200 repair, stale-clock refusal against native accepting context, no hold/default permission release, repeat without another write, undo/replay and full carriage reconstruction. Existing three native hold/fraud cases overlap the previous record. All signatures/original worlds remain synthetic verified-half fixtures; no live node ingress/IBD or performance measurement is claimed.
- `cargo check --release -p kaspa-consensus`: **success**, existing warnings remain. Non-test code did not change after that check and the 45-target run; the later correction was to the native test's prefix/time setup.

The initial native owner-based case **failed: 3 passed / 1 failed**, at reservation admission with `InvalidInterval`. It used the real legacy fixture's post-preparation owner interval outside original `+10,000`. This failure is retained below; it is NOT counted as a pass or hidden by widening hard or modifying model/resource limits. The final test explicitly asserts the same rejection, then tests the journal on the SAME original frame during preparation, AFTER the fixture's licence. This narrower journal check does not substitute for owner-path completion.

Final emitted geometry (deterministically derived fixture clocks, NOT CPU/network or actual-model measurements): accepted `100`, preparation-until `220000`, owner interval `220003..220013`, hard `10100`, refusal `InvalidInterval`. The static legacy-network quote bound and its scope are in the [clock-origin investigation](../../../design/palw/integration/rfc26-shared-frontier-volume-2026-10-20.md). The current owned route remains incompatible with the target clock; a versioned real profile, actual response/deadline mapping, source horizons, all allowed branches and node lifecycle tests remain required.

All commands used the shared heavy build lock. Two uncompleted compiler runs were explicitly interrupted when adding accepting-clock validation and correcting the fixture's accepting point to follow its licence; neither is counted as successful verification. Supply history is never erased by repair. Current assignment moves only after missing supply; retry history is in old/new native journal rows, not a new history field in the current snapshot. Retention/pruned-import closure remains open.

| Raw log | SHA-256 |
| --- | --- |
| [service-repair-owned-window-failed.log](service-repair-owned-window-failed.log) | `fecd06d5ba998895355468458a7add9fd8d46a1c5982f5c9f12e58c6338f0514` |
| [service-repair-native-release.log](service-repair-native-release.log) | `ed1437863e9a0c2b1da3cfd61326555e7ac6c63112f33e45c6e6164030c5fb12` |
| [service-repair-regressions-release.log](service-repair-regressions-release.log) | `623b2363031bb166b4c4fe0e5c1c34d4f0f9ea107311a87a61da2f977b17a028` |
| [service-repair-consensus-check.log](service-repair-consensus-check.log) | `6bf16608cc7c1e666d804403b637fb6e190ef25056954f9e644245e9a061b5a8` |

## Dependency horizon covers material retention

After `4f55a837d`, `CourtClockBounds` now requires the minimum signed dependency horizon to cover both hard + followup and hard + retention. The earlier component accepted 10,120 even when material lasted through 10,150. Other claims' same-bond liabilities still affect collateral release independently. No production caller, wire, preset or existing signed terms changed.

Shared-lock command: `cargo test --release -p kaspa-consensus-core --test palw_court_policy_v1 --test palw_the_release_did_not_move --test palw_activation_pool_is_t12_only --test g14_close_gates`. Result: **29 passed / 0 failed / 0 ignored** (policy 3 including twelve horizon boundary cases, release 9, activation 5, gates 12 including t12 identity). This is not T09 node acceptance.

[Raw release log](dependency-retention-release.log), SHA-256 `7b173688d0dd5fcc7ad7189d594955f14cc8f517100c3c482f3802fae923a087`. [Source audit and remaining admission contract](../../../design/palw/integration/rfc26-dependency-horizon-audit-2026-10-20.md).

## Minimum signed custody for rooted service binding

After `6f99abc74`, the proposed service context guard checks the main accepted custody pledge against original hard. When the frame has a semantic component it also requires the actual accepted bundle and matching history pledge scope/horizon. Missing dependency is not absence of a dependency. No signature, signed horizon, preset, identity or old unbound claim is changed.

Shared-lock commands:

```sh
cargo test --release -p kaspa-consensus-core --lib rooted_service_
cargo test --release -p kaspa-consensus-core --test palw_court_service_state_v1 --test palw_court_service_v1 --test palw_court_policy_v1 --test palw_the_release_did_not_move --test palw_activation_pool_is_t12_only --test g14_close_gates
cargo check --release -p kaspa-consensus
```

Final native 4 + regressions 45 = **49 passed / 0 failed / 0 ignored**. Non-test consensus release check succeeded, warnings remain. The native main-custody boundary uses the existing synthetic verified-half original/frame fixture (not real signature ingress): actual pledge field minus one and equality pass, plus one refuses. Existing native cases still cover default refusal, Final/collateral/material holds, streamed fraud adjudication, recovery and native journal/carriage replay. This count overlaps previous suites; do not add them as new unique tests.

Dedicated semantic missing/wrong-scope/short-horizon admission tests, full signed-dependency enumeration, profile followup/retention, atomic original admission and T09 remain incomplete. This minimum guard is not G14 completion or a node restart/pruned IBD result. Document validation still fails on the same five imported-asset references; baseline and original package bytes remain unchanged.

| Raw log | SHA-256 |
| --- | --- |
| [service-custody-native-release.log](service-custody-native-release.log) | `633cc971761c97cff1e4431877e0a814d4ae63097c306b42073b62532f6f52ba` |
| [service-custody-regressions-release.log](service-custody-regressions-release.log) | `ceaa60e8ba1496da9194aeb4a2695a3ae11e5e85bcc2d58346c5d20b5b39468e` |
| [service-custody-consensus-check.log](service-custody-consensus-check.log) | `b210e0482a6ed0a8aa70b983e5cc8ae40115a348f445bba401c4bae300535f48` |

## Semantic custody guard branch checks

After `f286cd394`, test-only changes add one explicit synthetic semantic guard case. Production code and parameters are unchanged. The main native frame fixture is extended through an explicitly named guard-only helper; its composite semantic frame/bundle is not claimed to pass native signature, payment, credit, serialization or original admission. The semantic family resource plan uses the existing derivation.

The new case refuses absent bundle, required horizon minus one, and each substituted claim/resource/execution/job/producer scope. Equality and plus one pass. This does not let a long main pledge replace a short semantic pledge.

Shared-lock commands: `cargo test --release -p kaspa-consensus-core --lib rooted_service_` (5 passed: new guard 1 + existing native 4) and `cargo test --release -p kaspa-consensus-core --test g14_close_gates --test palw_the_release_did_not_move --test palw_activation_pool_is_t12_only` (26 passed including t12 identity). Final selected total **31 passed / 0 failed / 0 ignored**. These overlap prior suites and are not a G14/T09 closure count. Native semantic original admission, complete dependency enumeration, node restart/pruned IBD and actual costs remain unproven.

| Raw log | SHA-256 |
| --- | --- |
| [semantic-service-guard-release.log](semantic-service-guard-release.log) | `66cd5cc9f1b96630311b2c1cef8cdcd23038afa3b6bcd1a3b0e0a0827034bf6a` |
| [semantic-service-identities-release.log](semantic-service-identities-release.log) | `e1aacfd03d558290396031c59e28497dde66b8f1780195c0376769d8e7979f9c` |

## Real-signature presigned TIR response relay

After `ed1723fc1`, a test-only native case uses real ML-DSA signatures for Begin/Part/Finish, with all three responses signed before the key is dropped. Two distinct illustrative carrier inputs extract the exact same signed control and reach identical native roots. Wrong-domain signatures refuse without mutation, and relayed Finish authenticates the original committed leaf. Original/custody setup remains verified-half and the illustrative outer carriers are unfunded: no UTXO/mempool/block fee acceptance or node T08 completion is claimed.

Shared-lock command `cargo test --release -p kaspa-consensus-core --lib court_relay_presigned`: **1 passed**. The prior release test executables were rerun under the same lock (production code has not changed): `target/release/deps/g14_close_gates-07a58aeba402f266`, `target/release/deps/palw_activation_pool_is_t12_only-19bd1d2a52f9c5c6`, `target/release/deps/palw_the_release_did_not_move-c8b47338cdf2801d`: **26 passed**, including pinned t12 identity. Total selected **27 passed / 0 failed / 0 ignored**. The identity replay is explicitly not a fresh cargo compilation of those targets. All code changes in this step are cfg(test).

[Signer/relay source audit and remaining T08 work](../../../design/palw/integration/rfc26-court-signer-relay-audit-2026-10-20.md). Availability for fresh adaptive responses still requires the authorized signer. No delegation, terminal unsigned evidence rule or funding-key transfer is introduced.

| Raw log | SHA-256 |
| --- | --- |
| [court-relay-native-release.log](court-relay-native-release.log) | `b024579ce3a3f6bf25e142e6c5e912cf4a908dd006c7ce7f8a7bdb07998a8d5e` |
| [court-relay-identities-release.log](court-relay-identities-release.log) | `e225f397aa925dc5a4c6febe8333ee2ae2ba8f77292008627b61d3a60d201685` |

## Funded third-party relay through legacy node acceptance

After `0af29566b`, the isolated LG14-A harness separates signed producer response preparation from carrier submission. The existing producer-paid wrapper is preserved. The new test creates an actual honest legacy claim, binds/licenses it, registers the newcomer, opens a real signed reservation/demand, prepares the producer's signed disclosure, and has the newcomer fund/sign the outer carrier. Mempool, node template inclusion, carrying block acceptance and subsequent native fold all succeed. Exact signed payload bytes are carried unchanged; the filer's read learns the authenticated binding and the node's producer duty disappears. Producer collateral and the harness's producer funding entry are unchanged. Direct virtual-UTXO retention checks, process shutdown and restart/IBD are not asserted by this new test.

Shared-lock command: `cargo test --release -p kaspa-consensus --lib lg14a_a_newcomer_funds_and_relays` — **1 passed / 0 failed / 0 ignored**. Existing real-node pursuit regression on the SAME freshly built executable `target/release/deps/kaspa_consensus-b46a428bb295fa5b lg14a_a_non_seat_newcomer_convicts_a_computation_lie_before_final` — **1 passed**. Previously compiled identity executables `g14_close_gates-07a58aeba402f266`, `palw_activation_pool_is_t12_only-19bd1d2a52f9c5c6`, `palw_the_release_did_not_move-c8b47338cdf2801d` rerun under the lock — **26 passed**. Production code has not changed. Final selected total **28 passed / 0 failed / 0 ignored**. This is not a performance measurement or all-G14 count.

The first regression driver named the separate X3 worktree's executable instead of this build's new filename and exited 127 before any test. The corrected driver used the `Running unittests` path from the build log; no build or node test was restarted because of an observation timeout.

Harness differences remain exactly those documented in LG14-A: isolated re-keyed genesis and funded test actors, skipped PoW and a directly configured dormant legacy fence bypassing real-network activation validation. No live network, preset, deployment or arming changed. This is legacy MaterialDisclosedV2, not TIR original/TFP1 acceptance, all legal branches or complete T08. Fresh adaptive answers still require the authorized court signer. [Detailed scope](../../../design/palw/integration/rfc26-court-signer-relay-audit-2026-10-20.md).

| Raw log | SHA-256 |
| --- | --- |
| [funded-relay-node-release.log](funded-relay-node-release.log) | `6f90c51aa9b15b12a2c463d2e4694b1dea99e2f2f5e9ed5586b804ccaa73bc2b` |
| [funded-relay-regressions-release.log](funded-relay-regressions-release.log) | `6fd1af4381035e8bf31d2bd48aa78154aa9429f0851f13896aff9797c73f09b5` |
| [funded-relay-driver-before-binary-correction.log](funded-relay-driver-before-binary-correction.log) | `33a8179ed448676a3685accfe8e50285174817073aec4a408ceb7211826906d2` |

## Source-derived owner-independent TIR request universe

After `e7b357485`, the dormant component counts and indexes all seven current native request families without materializing the complete set. Scope binds original claim, native allocation instance, native rules, source and shared profile, without owner/relayer/queue. This is not a service-packet identity, adoption record, admission, funded obligation graph or physical capacity guarantee.

Final shared-lock runs:

```sh
cargo test --release -p kaspa-consensus-core --lib court_tir_shared_requests
cargo test --release -p kaspa-consensus-core --test g14_close_gates --test palw_activation_pool_is_t12_only --test palw_the_release_did_not_move
cargo check --release -p kaspa-consensus
```

Final component **2 passed**, fresh identity/gate/activation/release **26 passed**, selected total **28 passed / 0 failed / 0 ignored**. Non-test consensus release check succeeded, warnings remain. An earlier intermediate component run passed before final scope and boundary cases were added; only the final verified log below is counted.

The actual tiny native source validator is the oracle over a larger candidate space, checking set equality and uniqueness. Additional cases check wrong-source scope, shared-profile namespace, invalid ordinals, every family boundary, largest allowed run, closed-form run count, odd/partial geometry and overflow. Large synthetic arithmetic geometries are not measured models or network throughput. [Scope and unresolved integration](../../../design/palw/integration/rfc26-shared-tir-request-set-2026-10-20.md). Full hostile interaction graph, signer/turn/fragment, actual cost and paid admission/supply and node T05 remain open. No fence or limit was changed.

| Raw log | SHA-256 |
| --- | --- |
| [shared-requests-native-release.log](shared-requests-native-release.log) | `8aa995c84c1276cb0d5835b54816fbaa5f9a17e0034ba5dd7010e85506c7e6aa` |
| [shared-requests-identities-release.log](shared-requests-identities-release.log) | `d00bacfe53d03e7837078e2740e66792acc2e3e38fd2f7beab79d577f01c91b8` |
| [shared-requests-consensus-check.log](shared-requests-consensus-check.log) | `3297e70c637923af56c5ff111c4e6018e385c4a0451ead404e6fa3efd922a9f4` |

## Funded relay: actual UTXOs and full block replay

The extended legacy node test checks producer funding remains unspent, the newcomer's carrier input is spent, and the actual change UTXO has the expected amount and script. A fresh in-process consensus instance imports the full selected block history; its state root, UTXOs, cleared producer duty and independently read authenticated disclosure agree. This is full block replay, not process restart, pruned import or TIR-original/service-ledger coverage.

Shared-lock `cargo test --release -p kaspa-consensus --lib lg14a_a_newcomer_funds_and_relays`: **1 passed / 0 failed / 0 ignored** (67.81 s test runtime, not a throughput measurement). The three previously compiled release identity executables named in the preceding funded-relay section were replayed under the lock: **26 passed**. Selected total **27 passed**; no production code changed. Earlier narrower scope statements describe the previous test version.

| Raw log | SHA-256 |
| --- | --- |
| [funded-relay-utxo-ibd-node-release.log](funded-relay-utxo-ibd-node-release.log) | `4e5560b0d24e6aa49d1ef4650ca30ba8b616beaff57f2bee3e3d3801b053aac7` |
| [funded-relay-utxo-ibd-identities-release.log](funded-relay-utxo-ibd-identities-release.log) | `19a0c8c386d18544393661dd4578949f23c252b8d17c19e795bc38b8b43bb58f` |
