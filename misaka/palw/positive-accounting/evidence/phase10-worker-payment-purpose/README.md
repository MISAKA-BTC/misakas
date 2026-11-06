# Phase10: WorkerPaymentとprimary Rewardの用途分離

2026-10-31。A0の明示依頼により、pure `CreditUse` の末尾へWorkerPaymentを追加した。
旧7用途の順序を保持し、serialization/wire/domain/policyは追加しない。
`Reward` / `inference_reward` は既存primary issuance・coinbase・model/PWU専用、
`WorkerPayment` / `worker_payment` は既存契約によるworker成功支払専用である。
ALL_CREDIT_USES、gateのexhaustive分岐、private All fixture、Unavailable/NativeRows拒否は8用途を扱う。

paid-only WorkerPayment＋契約で認めたSettlementからReward/receipt/DAG/Rule Eを与えない。
独立8×8とconsumer route、実A1 terminalでも返るpurposeの分離・欠落・撤回を検査する。

提供済みsourceサービス料と安全返金はこの計算信用gateの対象外である。
新money反例ではALL8 computation creditが拒否中でも、genuine private bookがPendingとEndedUnverifiedで
提供済みsource料を一回ずつ払い、責任未解決時はrefundを拒否し、正規closure後だけ一回返金する。
全undoで初期bookへ戻し、P不足だけで資金を永久lockしないことと責任floorを区別する。
このprivate試験はactual native UTXO/receipt/lease/closureを成立させたものではない。

| 実行記録 | 結果 |
| --- | --- |
| `common-all-result.json` / `common-all.log` | 本A5候補の共通契約35件成功。A0側の別44件の候補とは区別する |
| `leaf-all-result.json` / `leaf-all.log` | leaf53件成功（unit42、routes1、独立graph/rights8、fees2） |
| `leaf-clippy-result.json` / `leaf-clippy.log` | leaf全target、`-D warnings`、exit 0・警告なし |
| `node-default-all-result.json` / `node-default-all.log` | default node全lib55件成功（dues9、新funds15、既存native15、rows7、実TIR9） |
| `node-default-clippy-result.json` / `node-default-clippy.log` | default node全target、`-D warnings`、exit 0・警告なし |

各JSONはcommand/env/base/25対象source SHA/実行中source不変を保存する。
独立read-only source/raw/hashレビューは `independent-review.json` を参照。
各scopeや過去候補の件数を合算してnative全体の成功件数にしない。

Generic Authorization型自体は既存のままである。actual native最終issuerは現在viewでpurpose==Reward、
worker payerはpurpose==WorkerPaymentを厳密に確認し、別用途の認可をそのまま渡してはならない。
actual origin/paid/mining契約・全consumer・actual money/refund floor・VP同一batchの接続は別に必要である。
shared enum追加以外のnative writer/activationは変更していない。
