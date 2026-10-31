# Phase8: source NoOp境界とA2計上済みdueの再検査

2026-10-31。A0の確定したlogical duplicate op3/op4 NoOp境界に合わせ、
DATAメーターを保持する算術とactual rooted auth/literal/opportunity判定を文書で明確に分けた。
論理重複の別carrierコピーでもCase usage/historyを増やさない判定はA2/A4 ownerが行う。
普通のtransaction受理/料金/historyはnative層で独立である。

最新A2のnext usageはfirst source dueを既に含む。`SourceDueUpdate` と
`recheck_source_due_update` はfeeだけold値へ投影して既存算術で独立再計算し、
全next usageと共通SourceFeeDeltaIntentの完全一致を要求する。
12のdueを24へ増やさず、provider/scope/amount/Noneへの差替えを拒否する。
既存 `prepare_source_due` はpre-due入力用として維持する。

新2試験は、全7usage fieldとreceiptが不変のNoOp DATA、およびA2既加算updateの再検査を確認する。
旧メーター増加fixtureはdistinct newly authenticated opportunityを想定した未認証DATAであり、
論理重複の課金を許す試験ではないと明記した。

実command/env/base/source SHA/exit/試験中source不変は各 `*-result.json`、原出力は同名 `.log` に保存する。

| 実行記録 | 結果 |
| --- | --- |
| `node-all-result.json` / `node-all.log` | no-default node全lib46件成功（dues9、新funds15、既存native15、rows7） |
| `node-clippy-result.json` / `node-clippy.log` | 同候補no-default全target、`-D warnings`、exit 0・警告なし |

独立read-onlyレビューとraw/source hashの照合は `independent-review.json` へ保存する。
本試験はA2/A4の実auth、cross-carrier NoOp、history/journal、one-shot opportunityを実行・認可しない。
native writer/funding/同一DB batchの完成を示すものではない。
