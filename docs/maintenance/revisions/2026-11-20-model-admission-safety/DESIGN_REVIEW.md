# MAS1設計レビューと実装時必須試験

## レビューの範囲

今回の設計文書に対する整合性レビューであり、独立した外部査読やnative試験の実施記録ではない。下表は将来の実装で満たすべき期待結果である。

| 試験入力・攻撃 | 必須の結果 |
| --- | --- |
| zが定数・s・H(s)でコア推論を使わない | 認証された規則違反ならFAIL |
| challengeがmask/truncationで消える | 登録規則との矛盾を認証してFAIL |
| yと無関係な巨大な採掘専用枝 | 費用の有用性条件違反を拒否。graph上の見せかけの依存でPASSにしない |
| dense/full-rankという自己申告 | 未認証の全weight条件ならUNKNOWN |
| 低rankだが実用的で、費用定理にも適合 | 低rankというだけでは詐欺にせず、全条件を評価 |
| 解析timeout・未知op・未検査MoE分岐 | UNKNOWNのまま採掘不可 |
| score高得点だがG-COST不明 | BFTが賛成しても採掘不可 |
| 新規UNKNOWNのBFT応答なし | 原期限でREJECTED_UNRESOLVED、算術有罪や仮承認にしない |
| 未適格モデルの評価実行 | 非報酬caseとしてのみ受理し、weight/credit/受賞権利を発行しない |
| 一つの評価traceを採掘claimとして再送 | 用途・適格性・一回会計で拒否 |
| 既PASSEDモデルへの未検証通報 | 受付だけで全ピアの永久停止にしない |
| 有効な局所反例 | BFTの署名収集を待たず機械的反証へ接続 |
| q未満・同じ署名者の重複 | StopCertificateとして拒否 |
| 別network/epoch/branch/generationの署名 | 現在scopeとの不一致で拒否 |
| TB1 share certificateを停止証明として提出 | purpose・権限・codecで拒否 |
| 同batchで停止とFinal/権利消費 | 停止を先に反映し、新しい成功を拒否 |
| 同batchで停止と解除 | 新停止を保持。解除は正確な旧集合だけを対象 |
| 一つのモデルのPASSでTheoremRule version全体を解除 | 個別解除は拒否。新rule採択と依存モデルの再検査を必要とする |
| 開いたrisk caseの世代をownerが更新 | 原期限内に署名対象を消せない。正規終結時だけ世代更新 |
| 旧PASS又は古い停止集合への解除証明 | 最新世代・期待旧recordの照合で拒否 |
| burn Resume又はowner/core変更 | burn理由だけが変化し、安全停止は残る |
| 重みを変えて同じIDのまま再開 | 不変性違反を拒否。新descriptorなら全検査をやり直す |
| sourceの同一artifact/profile別名 | 既知alias indexで停止と資格を共有 |
| 停止中の資料・Court・返還 | 元期限・予約で継続 |
| 停止証明から過去の確定残高を没収 | 権限外として拒否 |
| restart/reorg/IBDで古いPASSEDのみ復元 | 停止集合・rule依存・世代を欠く状態を拒否 |

## 選択した安全性境界

- 自動検査のsoundな否定と、網羅的な短絡検出能力を区別する。未認証のglobal predicateは閉じる。
- finite test、dataflow、entropy、roster署名をG-COSTの代用にしない。
- BFTは停止リスクを扱う権限であり、真実性oracle又は台帳finalityではない。停止前の窓と誤停止が残る。
- 公開モデルの正しい最適化は算術詐欺ではない。必要ならruleを停止・更新するが、罰金を費用の証明にしない。
- 実用性を守るため、強制された採掘専用処理と巨大traceの費用を有用推論へ混ぜない。

## 未完事項

具体的なQwen用G-COLL/G-COST、完全な自動解析器、有界な適合検査器、実用suiteと閾値、評価のnative経路、TB1追加purpose、実rosterと署名codec、保護容量、実機測定、ネットワーク/reorg試験、公開activationは今回完了していない。
