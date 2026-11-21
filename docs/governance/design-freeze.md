# 設計凍結と変更手続き

<!-- MAS1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MAS1の構造・実用性検査とBFT安全停止

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0065](../rfc/0065-model-admission-screening-and-bft-stop.md)／[ADR0233](../adr/0233-model-admission-screening-and-bft-stop.md)／[MAS1適用・移行](model-admission-safety-transition.md)を、この新規採掘profileの追加正本とする。ユーザーは今回のモデル検査・BFT停止への接続を明示指示した。既承認の文書改訂について再確認を要求せず、停止権限の範囲と有効化条件を別に固定する。

構造検査はPASS/FAIL/UNKNOWNを区別し、UNKNOWNは採掘不可のままBFT reviewへ接続する。G-EXEC/G-COLL/G-COSTを維持し、公開・共通の実用性G-USEと停止状態をAND条件へ追加する。BFTはモデル/ruleの停止と条件付き解除に限り、未知の費用定理の承認・台帳finality・fork choice・reorgの権限を持たない。burnやowner変更で安全停止を解除しない。

旧「モデル品質・モデル単位の検査を一切しない」という文言は、MAS1の採掘資格に関する今回の限定検査へ置換する。一般登録とMOE2市場原子生成、通常取引、coreの経済配分、Panel＋Court、計算validity proof不使用、旧claimの元責任は維持する。旧採択ブロック・原文・実測はその版の履歴であり、今回の数学的証明・native実装・公開activationが完成したという意味ではない。
<!-- MAS1-ADOPTED-20261120-END -->

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

**Decision: Accepted — 今回のユーザー指示を記録。整理日 2026-10-17。**  
[PRINCIPLES §11](../PRINCIPLES.md#design-freeze) / [ADR-0181](../adr/0181-design-freeze-and-controlled-change.md) / [RFC-0021](../rfc/0021-design-freeze-and-documentation-governance.md)。

> **設計の変数・パラメーター以外は原則凍結し、通常は変更禁止とする。ただし、重大な問題が確認された場合には、新規 RFC と新規 ADR の両方により、レビューと明示的な採択を経て変更できる。**

## 凍結対象と継続できる作業

採用済みの原則、合意・状態遷移、責任分界、モデル制度、認証・裁定、予算会計、データ・wire の意味、不変条件を通常変更しません。凍結は完成・検証・有効化を意味せず、未採択 Draft 全文を採用したり旧案を復活させたりしません。

既存設計への実装、意味不変の不具合修正・性能改善、試験・監査・文書整理は継続できます。「修正」「最適化」と呼んでも、挙動・保証・責任を変える場合は設計変更として扱います。

## パラメーター調整

明示された調整変数は、採用済みの構造と不変条件を維持した範囲で調整できます。名前・単位・対象・旧新値・調整範囲・導出・測定・影響・試験・承認者・適用条件を [台帳](parameter-register.md) に記録します。未承認値を本番値に読み替えず、範囲が未定なら推測で埋めません。既存の固定値を再調整する場合も設計責任者の明示的な再承認を要します。

数値・フラグの変更でも、認証数や待機をゼロにして検証を外す、二階層・超線形配分・登録総枠を実質廃止する、責任保持を短絡する、Model ID / root / wire の意味を変える場合は通常調整ではありません。重大問題時の設計変更手続きへ移します。

## 重大な問題

合意安全性の破綻、継続的停止・回復不能、資金・担保・報酬の重大な損失や二重計上、認証・裁定が成立しない再現可能な重大欠陥などです。影響・根拠・再現方法、既存設計内の修正やパラメーター調整では解決できない理由を示します。

「別方式が好み」「流行」「実装しやすい」「文書を短くしたい」は単独では例外理由になりません。懸念の報告・調査は可能ですが、それだけで凍結を解除しません。

## 例外変更の手順

1. 問題・深刻度・対象 network / release / commit / 仕様節と、再現または検証可能な証拠を記録する。
2. **新規 RFC** に、問題、代替案、最小変更、影響、不変条件、互換性・移行、試験、失敗時対応を書く。
3. **新規 ADR** に、RFC のレビュー、採用理由、不採用案、承認者、残余リスク、旧 ADR / RFC の置換節を書く。相互参照し、両方の明示的採択が揃うまで現行設計を維持する。
4. Spec、PRINCIPLES の該当節、台帳、状態・置換関係を整合させる。PRINCIPLES 自体の改定も設計責任者（現行のユーザー承認主体）の明示的承認を要する。
5. 実装・試験・独立レビュー・移行・リリースを確認し、必要な **有効化を別途承認**する。文書やコードの merge だけで fence・高さ・wire・fingerprint・稼働設定を変えない。

緊急の停止・ロールバックは既存の承認済み運用手順内で行い、恒久的設計変更の抜け道にしません。承認権限の移譲は今回行いません。

## 適用時期と履歴

明示的に指示された [画像・動画・モデル改善の延期](post-mainnet-scope.md) は、リリース対象と有効化の管理であり、技術方式の差し替えではありません。重大欠陥の存在を仮定した決定ではありません。後日の再開でも凍結された設計を無断で変更できません。

過去の「再審しない」「恒久」「変更不可」は通常の変更禁止として維持し、重大問題時の例外手続きは本規程に統一します。既存の禁止方式や技術的決定を今回撤回しません。番号・日付の大きい文書が自動的に上位になるわけではなく、PRINCIPLES と明示的な採択・置換関係に従います。コードとの違いは不整合として記録します。
