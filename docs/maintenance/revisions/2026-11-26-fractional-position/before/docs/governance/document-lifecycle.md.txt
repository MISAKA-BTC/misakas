# 文書の役割と状態

<!-- MOE2-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MOE2の30%購入・20%永久流動性・50%残余

**ADOPTED_DESIGN_NOT_ACTIVATED**。新経済profileの正本は[RFC0064](../rfc/0064-reward-buy-and-permanent-liquidity.md)／[ADR0232](../adr/0232-reward-buy-and-permanent-liquidity.md)／[移行契約](model-option-economics-transition.md)。ADDITIONALの対象worker報酬Rを `B=floor(300R/1000)`、`A=floor(200R/1000)`、`D=R-B-A` に分け、Bで購入したPositionの必要分をAと同じ市場へ追加し、余剰だけをburnする。全工程とDの支払を同じ原子的決済に結合する。

永久LPはnativeの非償還記録であり、withdraw・transfer・owner/管理者引出し・LP手数料請求を持たない。実資産は通常swapに使い、Aを旧locked seed下限へ加算しない。登録時の一意市場生成、coreの市場なし・Miner=R、通常AMM5%/3%・直接移転、TB1/FOW1は維持する。永久LP記録済みmodel IDのcore指定は禁止する。

旧MOE1の「50%を全額購入し、購入Positionを全retireする」は旧cohortの規則であり、MOE2の新規経路へ適用しない。以下の旧採択・原条文・測定はその版の履歴として保存する。
<!-- MOE2-ADOPTED-20261120-END -->


<!-- MOE1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 採択 — 追加モデル限定MisakaOption経済MOE1

**ADOPTED_DESIGN_NOT_ACTIVATED**。新経済の正本をRFC0063/ADR0231とRFC-v2-0011へ追加する。旧sourceの率・非譲渡・core売却継続は新経済profileの選択肢ではない。

追加モデル登録と一意な市場の作成を同じ原子遷移とし、通常の登録済み・市場未作成状態やseed待ちを設けない。genesis・有効softfork指定モデルは市場を作らず、価格付け・共通上限処理後の対象worker報酬RをMinerへ100%配分する。追加モデルのみ50%買上げ、通常AMMはburn5%・owner総額3%・net残差、直接送受信はAMM手数料なしとする。

[RFC0063](../rfc/0063-additional-model-option-economics.md) / [ADR0231](../adr/0231-additional-model-option-economics.md) / [適用と移行](model-option-economics-transition.md)。新経済profileではこの後続採択を優先する。旧率・旧署名・原文引用・過去の実装試験はその版の記録として保持し、新規経路へfallbackしない。TB1/FOW1の正当性・乱数・Court・適格性条件を免除しない。
<!-- MOE1-ADOPTED-20261120-END -->

<!-- TB1-FOW1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 採択 — TB1乱数とFOW1コア採掘値

**ADOPTED_DESIGN_NOT_ACTIVATED**。今回のユーザー指示により、新規PALW採掘の方式をFOW1＋TB1へ統一する。コア推論の採掘値zと補助receiptを区別し、登録系列の非衝突定理・複数試行費用条件・完全検査対応を採掘適格性に要求する。未完成のQwen定理や参照fixtureを本番合格としない。

新しい採掘profileの規範入口をRFC0061/0062へ変更する。旧文書のAccepted/Draft表示や旧beaconの語は旧版の状態を表し、新profileの選択肢ではない。

[TB1の正本](../rfc/0061-palw-internal-threshold-beacon.md) / [FOW1の正本](../rfc/0062-palw-function-output-and-work-admissibility.md) / [用途・置換・移行](palw-tb1-fow1-transition.md) / [全旧記述の適用台帳](../maintenance/revisions/2026-11-20-tb1-fow1/BEACON_AND_POLICY_COVERAGE.md)。新profileでは本節とこれら後続規範を優先する。以下の旧方式・旧Status・原文収録は、その版の履歴又は本改定で明示的に継承する範囲であり、新規受付のfallbackではない。旧受理claimの署名・責任・原期限は保持する。設計採択、native実装、定理完成、公開activationを区別する。
<!-- TB1-FOW1-ADOPTED-20261120-END -->

| 種類 | 答える問い | 内容 |
| --- | --- | --- |
| getting-started / operations | どう使うか | network / release を明示した手順 |
| architecture | どう構成されるか | 全体像と責任分界。稼働済みと混同しない |
| spec | 何に従うか | 対象版・適用条件付きの規範 |
| ADR | なぜ決めたか | 背景・決定・比較案・帰結・置換関係 |
| RFC | 何を検討したか | 技術設計・代替案・未決事項・採択への参照 |
| design | 実装まで何を詰めるか | 詳細設計・統合・計測計画 |
| testing | どう確かめるか | 試験方法と受入条件 |
| audit / evidence | 何を確かめたか | commit・機材・条件・日時付きの観測 |
| history | 当時どうだったか | 旧版・撤回部分・判断経緯。保存は採用継続/撤回の自動判断ではない |

## 状態を分離する

Decision（Proposed / Accepted / Mixed / Superseded / Withdrawn / Unknown）、実装（有無・休眠）、検証（対象付き・未実施）、有効化（network・高さ・未確認）、リリース範囲（当面提供 / Deferred）を分ける。

設計採用・コードの存在・試験 PASS・有効化は別。特に RFC-0003 の画像・動画と RFC-0004 の改善は、過去に実装や試験の記録があっても [当面無効](post-mainnet-scope.md)。資料の Status は記録時の範囲に限定する。

## 原文・番号・更新

主 ADR / RFC の番号・パスを保持し、番号衝突は完全パスで識別する。新規番号は索引と統合先の予約を照合する。旧主張を消して当時の意味を変えず、置換する節・後継文書を明示する。

README を全履歴の保管場所にせず、短い入口・完全索引・原文を分ける。原文のコマンド・ログ・digest・数値を書き換えない。実測や既存ネットワークは文書整理だけで再承認しない。

採用済み設計の変更は [凍結規程](design-freeze.md)、数値調整は [台帳](parameter-register.md) に従う。コードと文書の不一致は推測で埋めず記録する。
