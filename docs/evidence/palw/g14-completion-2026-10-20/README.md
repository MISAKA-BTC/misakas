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
