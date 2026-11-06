# Phase9: Positive必要条件 AND 用途別の既存entitlement

2026-10-31。PositivePassed/全coverage/P承認/資金Readyから新しい経済権利を作らないためのhardening。
`RootedCreditView::authorize_entitlement(binding, usage)` は未実装なら全用途をtyped拒否する。
共通Positive条件に加え、rootは要求用途、全祖先はDependencyの既存権利を確認する。
edgeごとのexport認証は従来どおり必要である。
`TirCreditView` はtrusted viewへそのまま委譲する。

private fixtureはAllまたは用途集合を明示する。native向けのAll implementationは追加しない。
settlement-only、reuse-only、権利情報欠落、用途単独付与の7×7、祖先Dependency撤回を検査した。
実A1 terminalを使うnode試験でも、現在の用途権利の拒否/撤回をlocal Matchで置き換えない。

| 実行記録 | 結果 |
| --- | --- |
| `leaf-all-result.json` / `leaf-all.log` | leaf全51件（unit41、consumer1、独立graph/entitlement7、料金2）成功 |
| `leaf-clippy-result.json` / `leaf-clippy.log` | leaf全target、`-D warnings`、exit 0・警告なし |
| `node-default-all-result.json` / `node-default-all.log` | default featureのnode全lib55件（dues9、新funds15、既存native15、rows7、実TIR9）成功 |
| `node-default-clippy-result.json` / `node-default-clippy.log` | default node全target、`-D warnings`、exit 0・警告なし |

各JSONは実command/env/base/25対象source SHA/実行中source不変を保存する。
原ログと独立sourceレビュー/hash照合は `independent-review.json` を参照。
この2つのscopeの件数や過去候補の結果を合算してnative全体の成功数にはしない。

Kernel/Native39のposter-funded paid FinalがB/F/R=0である既存条件を維持する。
actual origin/paid/mining契約から権利を解決するnative adapterは未実行・未完成で、missing portを拒否する。
actual VP/全consumer/複数process/復旧の完成や、モデルpolicyの変更を主張しない。
新wire/domain/policyは追加していない。
A0側のA7 `PrivateEconomicFixture` はownerによる明示entitlementへの移行が別途必要であり、
この試験結果へ含めていない。paid起源のfixtureへ自動Allを割り当ててはならない。
