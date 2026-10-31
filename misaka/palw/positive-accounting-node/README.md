# A5 node adapters

共通契約 v2/r1とA0 TIR annexを使い、A5のleafを実Node検査結果・既存精算checkerへ接続する。
このcrateはnode側だけに置く。依存方向は `node → core + exec + accounting leaf` で、coreがexecutorに依存する循環を作らない。
native新claimの受付、rooted row生成、公開網の有効化は別の未割当境界である。

## 実A1検査から信用判定

`tir::TirCreditView` はA1のprivate-field `CheckedTerminal` を所有する。外からPassed flagやrelation数を入れてterminalを作れない。
認証済みResultと現在のadmitted bindingに対して、scope/result/output/dependencies、A0のselector、
`coverage_plan_id(... exact_derivation_id(), mandatory_relations)`、全数を照合する。
Mismatchは信用にならず、現在のGate C・fraud・満期・責任・資金・全祖先はleafの同一predicateでさらに確認する。
全7用途の具体入口はleaf `CreditConsumers`。既存native readerの新Positive armはまだ存在しないため、その接続済みとは宣言しない。

A1正式source `ef8e4e87d` とoptional登録 `b3a61454c` を使う。
先行snapshotとの完全一致は `../../../handoff/positive-court/a1-formal-source-match.json` に記録した。
executor依存のfeatureは `positive-contract` であり、`node` だけでは共通adapterを有効にしない。

## 実Transactionから既存escrow検査

`native::check_existing_escrow_fee_spend` は次を確認し、既存 `palw_escrow_qualified_spend_v1` を直接呼ぶ。

- leafのprivate-field projectionが同じbook owner・現在世代・before totals・現在planから来たこと。
- actual Transactionのcached txidが現在の内容と一致すること。
- 認証済み新resultから導出したclaim/case、scope、funding consent、payerとexisting before-rowの一致。
- 実ML-DSA公開鍵からprincipal IDとnative address payloadをそれぞれ導出すること。IDをaddressにcastしない。
- 実出力のサービス料・契約burn・payer返金・continuationと、前後の全資金保存。
- 期限後のpayer returnが、提供済みservice feeを消していないこと。

返り値はprivateな検査済みproposalであり、native spend権限ではない。
`recheck_rooted_before` は現在のchain stateに同じescrow before-rowが実在するかを再確認する。
A4/A6はscript/UTXO検証、row/資源/case/責任/支払markerの同一transactionを別途完成させる必要がある。

試験のexisting rowとfunding consentはprivate fixtureで、旧FundingTermsに署名して新Result受付を通したものではない。
旧TFO1 funding bodyはproducerのtrace-root付きclaimへliteralに結び付いている。
旧FundingTerms.idを新serviceに埋めてそのclaimをResultから導出すると循環identityになる。
新funding body/参照方式をA0が割り当てるまで、これをnative原資受付の成功へ読み替えない。

## 検証

```sh
scripts/ci/positive-accounting-node-ci.sh
POSITIVE_ACCOUNTING_NODE_COMMAND=clippy scripts/ci/positive-accounting-node-ci.sh --all-targets --no-deps -- -D warnings
```

runnerは実Cargo manifestのdependency/feature/dev-dependencyを解決し、sourceを参照する一時workspaceを作る。
root lockfileをコピーし、ネットワークなし・debug情報なし・incrementalなしで検証する。
nodeだけの一時workspaceを実行しており、native node processの起動・合意処理の試験とは区別する。

実A1経路6試験とexisting escrow engine経路9試験が成功。実算術期待値は小TIRの `[8,14,3,14] → token 1` を別に固定する。
偽の出力、別result、plan差替え、再起動でflagだけ残すケース、reorg、旧codecへの流用、重複・支払順序、
不正payee、古いtxid/支払projection、rooted row不足、deadlineによるearned fee消失を検査する。
raw logとコマンドは `../positive-accounting/evidence/independent-phase2/` に保存する。

workspace/lockfile登録の提案差分は `workspace-registration.patch`。共有登録の所有者A0が統合時に適用する。
全native reader・FundingScope・Gate C対象の未割当要求は `../../../handoff/positive-court/A5-to-A0.md` に記載。
有効化設定は変更しない。
