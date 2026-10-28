# 状態と適用範囲

**対象は添付 `docs(1).zip` と今回の明示的指示です。** 設計の採択、実装の存在、検証、稼働ネットワークへの有効化、今回のリリース範囲を区別します。node・Params・RPC の実態は今回照会・変更・試験していません。

## 初期 mainnet の範囲

| 機能 | 現行方針 | 実際の runtime | 再開 |
| --- | --- | --- | --- |
| RFC-0003 画像経路 | **mainnet 後へ延期・当面無効** | 無効化実装・拒否試験の照合が必要。今回未確認 | 機能別レビューと明示的承認 |
| RFC-0003 動画経路 | **mainnet 後へ延期・当面無効** | 同上 | mainnet 開始で自動有効化しない |
| RFC-0004 モデル改善 | **mainnet 後へ延期・当面無効** | 同上。共通の不変登録まで停止しない | 機能別レビューと明示的承認 |

[正式な延期・無効化方針](governance/post-mainnet-scope.md)。延期機能の肯定的完成を初期 mainnet の条件から外しますが、閉鎖確認と共通機能の安全性・回帰は免除しません。音声・embedding 等を新規承認する表ではありません。

## 設計・実装・検証・有効化

| 項目 | 文書上の設計・実装状態 | 有効化の扱い | 根拠 |
| --- | --- | --- | --- |
| PALW のみの合意 | BFT validator 不使用の設計を採用 | 実ネットワークの退役は今回未確認 | [ADR-0180](adr/0180-palw-only-consensus-no-bft-validators.md) |
| V+ | 採用設計・測定待ち・休眠実装、未検証という記載 | 今回有効化なし | [RFC-0016](rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) |
| モデル二階層 | ADR-0179 採用。旧モデル審査は撤回 | 今回有効化なし | [RFC-0017](rfc/0017-model-eligibility-examination-and-renewal.md) |
| Gate C / DAG | RFC全体はDraft、個別採択あり・休眠実装 | 今回有効化なし | [RFC-0018](rfc/0018-dag-parallel-multi-model-consensus.md) |
| 年次カタログ | Draft / 個別決定と試験記録 | 予定高さを稼働済みとしない | [RFC-0019](rfc/0019-annual-model-upgrade.md) |
| 長文脈メモリ | Draft / 部分実装・未検証の記載 | 合意有効化の対象外という記載 | [RFC-0020](rfc/0020-long-context-calibration-memory.md) |

## 稼働版を確認するとき

network、release tag / commit、実際の Params / fence / fingerprint、有効化高さ、node と chain の証拠を照合します。資料の current / today / live / PASS は当時の対象だけに有効です。コードと資料の不一致は隠さず記録します。

今回、`α=1.5`、30日平均、0.3/1.0 の階層、登録総枠の原文上の設計値、bond 予算・claim 認証は変えていません。拘束の後続決定は Panel=0 7日相当 / Panel=1 1日相当です。[台帳](governance/parameter-register.md)

元資料の detailed readiness は [readiness matrix](design/palw/integration/activation-readiness-matrix.md)。その過去の状態を現在の稼働証拠にしていません。
