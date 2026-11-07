# 初期 mainnet の対象範囲 — 画像・動画・モデル改善は延期

**Decision: Accepted — 追加のユーザー指示を記録。整理日 2026-10-17。**  
**Policy status: `DEFERRED_POST_MAINNET` / `DISABLED`（方針上）。**  
[ADR-0182](../adr/0182-defer-media-and-model-improvement-until-post-mainnet.md) / [RFC-0022](../rfc/0022-post-mainnet-media-and-model-improvement-scope.md) / [PRINCIPLES §12](../PRINCIPLES.md#post-mainnet-scope)。

> **RFC-0003 の画像・動画の経路と RFC-0004 のモデル改善は、mainnet 稼働開始後に取り組む機能として延期し、当面は無効にする。初期 mainnet の必須実装・提供・有効化対象には含めない。mainnet 開始をもって自動的に有効にはしない。**

これは実装・運用に要求する方針です。入力は docs のみであり、今回 node の Params・fence・RPC を変更したわけではありません。**実際の runtime の無効化は未確認**で、full checkout の実装照合と拒否試験を必要とします。

## 対象

| 領域 | 当面の方針 | 具体的な範囲 |
| --- | --- | --- |
| RFC-0003 の画像経路 | mainnet 後へ延期・無効 | 画像生成 profile、画像 job / pipeline の提供・新規受理・実行・claim・当該経路の経済/合意 credit |
| RFC-0003 の動画経路 | mainnet 後へ延期・無効 | 動画生成 profile、動画 job / pipeline の提供・新規受理・実行・claim・当該経路の経済/合意 credit |
| RFC-0004 のモデル改善 | mainnet 後へ延期・無効 | 改善 protocol の epoch、学習・蒸留等と接続する改善用 dataset/material・候補・評価 job、候補選択、改善固有の報酬・pool・自動化 |

改善版が独立した不変モデルであるという ADR-0175 の原則は維持します。旧 head / root / AMM の上書き方式を、延期した設計として復活させません。

## 無効化の要求

当面の mainnet 候補 release・公開提供では対象機能を enable / arm / 自動展開しません。CLI、SDK、gateway、RPC、登録・job admission、fold・claim・報酬経路等の別入口から迂回して開かないことを確認します。未提供機能として明示的に拒否し、成功・利用可能であるかのように表示しません。

RFC-0004 の `palw_improvement_v1` は当該 release で有効化しない方針です。RFC-0003 の `palw_gen_v1` や `palw_fp_job_v5` は共通部分を持つため、名前だけで全体を一律停止してテキスト経路を壊しません。共通機能を使う場合も、画像・動画 profile と改善経路が閉じていることを別途確認します。分離できないまま共通 fence を開いて対象機能を一緒に有効化してはいけません。新たな fence 名・高さ・wire ID はこの文書で作りません。

過去の wire ID、decoder、証拠、既存履歴を安全に読む責任は保持します。旧ネットワークで既に生じた claim・bond・報酬・裁定責任を、この延期を理由に遡及無効化しません。過去の「armed」「integrated」「次回 flag day」の記録は、今後の有効化の承認ではありません。

## 残す機能・広げない範囲

テキスト生成、必要な TIR / kernel / 検証の共通部分、claim ごとの Gate C / court、bond 予算、固定モデル登録と二階層、ADR-0175 の不変性は撤回しません。`palw_model_immutable_v1` の不変性ルールまで改善と一緒に停止しません。

RFC-0019 の既存手順に従う年次カタログ追加と、RFC-0020 の既存モデルの較正・変換・忠実度計測・メモリ最適化を、RFC-0004 の改善 protocol と混同しません。既定の承認条件・固定 identity は維持します。

本指示を RFC-0003 全体の廃止と読み替えず、音声・embedding・その他 modality を自動承認もしません。初期提供は既存の承認済み範囲に従います。一般のオフチェーン研究・外部の学習行為を禁止する規程ではありませんが、延期機能の提供・実装完了を当面の必須作業にはしません。

## 初期 mainnet の受入

対象機能の肯定的 E2E 完成や性能達成を、初期 mainnet の release blocker にしません。過去の「全 RFC の実装完了」条件は、この延期範囲を除いて読みます。

一方で、**対象経路が閉じていること**と、共通実装の不具合がテキスト・登録・検証・会計を壊していないことの確認は必要です。共有コードの合意・会計・回復上の問題を、延期を理由に免除しません。

最低限、画像/動画 job の通常入口・代替入口からの拒否、改善 epoch / 評価 / 候補選択 / 固有報酬が進まないこと、共通経路での迂回不能、再起動・再同期での一貫性、テキストと claim 認証の回帰を確認します。これは試験要件であり、今回 PASS したとの記録ではありません。

## mainnet 後の再開

mainnet が開始しても延期状態のままです。機能別の再開計画、責任者、対象 release、影響・セキュリティレビュー、実装・測定・拒否/肯定試験・回復試験、必要な互換性・移行条件を確認し、設計責任者の明示的な提供・有効化承認で再開します。期日や activation height は未定です。

採用済み設計のまま再開することはリリース判断です。構造・責任・不変条件を変える場合は [凍結規程](design-freeze.md) の重大問題時の新規 RFC + 新規 ADR を必要とします。
