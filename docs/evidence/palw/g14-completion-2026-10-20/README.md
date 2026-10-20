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
