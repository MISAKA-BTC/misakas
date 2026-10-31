# A5 node adapters

共通契約 v2/r1とA0 TIR annexを使い、A5のleafを実Node検査結果・既存精算checkerへ接続する。
このcrateはnode側だけに置く。依存方向は `node → core + exec + accounting leaf` で、coreがexecutorに依存する循環を作らない。
namespaceはA0 `42a60b840` で予約済みだが、native新claimの受付、rooted row生成、公開網の有効化は未完成である。

## 新Positive資金とsource dueの採用案

`dues` は既存ServiceReceipt keyに対するfirst unique source料金の発生を計算する。
`Case.usage.fee`は資料サービスdueだけを数え、実支払やCourt全体の料金とは分ける。
`native_funds` は新FundingRowの三勘定、old/new/provisional共通lock分類、actual Transactionの出力・burnを検査する。
どちらもnode-local DATA提案で、state writerやnative権限を持たない。
actual accepted-prefix・canonical recipient/rate・live UTXO/lock・一回払い・責任・同一batchはnative ownerの接続が必要である。
具体APIと採用箇所は [A5-positive-funds-proposal.md](../../../handoff/positive-court/A5-positive-funds-proposal.md) を参照。
旧FundingTerms/escrowのID、global key/domain、公開presetは変更しない。

## 実coreのtyped rowsから信用入力を読む

`rows::PositiveCreditRows` はA0 `327393e8e` の固定value/既存keyを使い、実 `PalwChainStateV2` に対する
core `PositiveRowsV2::read/bundle`へ直接接続する。独自codecやnative setterは追加しない。
欠落route、欠落row、malformed key/value、signed bundleのlink不一致、Result/fundingとWitnessの署名失敗を区別する。
Caseにpositive_witnessがある場合は実Witness/Coverage rowも読み、共通 `validate_witness_bundle` で照合する。
core bundleの失敗時だけread-only probeで欠落/破損componentを特定し、署名失敗を単なる欠落に変えない。

成功結果 `SignedCreditInputs` は署名確認済み**data only**であり、native admissionやcreditではない。
公開DTOから生成するconstructorはなく、`rows::NativeRowCreditGate`の認可型は `Infallible`。
全7信用用途は同じread結果、または次の8 ownerの未実装をtyped denialとして返す。

- accepted origin
- accepted prefix / import世代
- registered expectations
- local replay
- live funding / capital
- dependencies / exports
- current Court / resources
- actual P draw Certified

署名bundle内のOrigin/Passed/資金残高、snapshot root、A0 `069d54007` のP statement DTO署名は、
これらownerのprivate capabilityを作らない。P DTO認証は実抽選の`Certified`ではなく、
`PublicCheckCertified`や旧S/K/V subjectも代用しない。旧認証rowへのfallbackは実装していない。

## 実A1検査から信用判定

`tir::TirCreditView` はA1のprivate-field `CheckedTerminal` を所有する。外からPassed flagやrelation数を入れてterminalを作れない。
認証済みResultと現在のadmitted bindingに対して、scope/result/output/dependencies、A0のselector、
`coverage_plan_id(... exact_derivation_id(), mandatory_relations)`、全数を照合する。
Mismatchは信用にならず、現在のGate C・fraud・満期・責任・資金・全祖先はleafの同一predicateでさらに確認する。
全7用途の具体入口はleaf `CreditConsumers`。既存native readerの新Positive armはまだ存在しないため、その接続済みとは宣言しない。
`validate_scope` は各claimの固定ancestry anchorを現在viewで検証する。親子のanchorが異なっても同じ有効lineageなら進め、
親originだけがreorgで消えた場合はその子孫をholdする。現在tipへ署名scopeを塗り替えない。

A1正式source `ef8e4e87d` とoptional登録 `b3a61454c` を使う。
先行snapshotとの完全一致は `../../../handoff/positive-court/a1-formal-source-match.json` に記録した。
executor依存のfeatureは `positive-contract` であり、`node` だけでは共通adapterを有効にしない。
A0/A1 `50473e607` の `check_authenticated_part` も使用し、不正署名・署名込みcap超過はchecker進捗を変えず、
受理したpacketの署名bytesとtransport実meterを検査する。

## 実payer署名とactual funding output

`native::inspect_signed_fee_only_funding` はA0 `AuthenticatedFunding` と認証済みMiner resultを使う。
新FundingScopeの非循環projectionから、payer署名、最終service/result、case、resource quote、全予定料金を照合する。
提供済みunitだけでなく全scheduleのsource feeをService上限へ、Court全体fee+burnをpayer/quote上限へ別々に制限する。
capital-bearing契約は明示拒否し、このprivate fee-only経路のholdへcapitalを混ぜない。

実carrierのown outputについてcurrent txid、index、P2PKH、金額、署名済みrefund payloadを既存inspectorで確認する。
現在時刻はpayer consentとAdmissionIntentの**両方の排他的期限**を満たす必要がある。
返る `PayerSignedFeeOnlyOutput` はpolicy/scheduleの正規preimage、UTXO存在/入力権限、coin lockを認証せず、
`require_native_admission()` は未割当のnative成功を必ず拒否する。actual outputの存在とcoin lockを混同しない。

## 実Transactionから既存escrow検査

`native::check_existing_escrow_fee_spend` は次を確認し、既存 `palw_escrow_qualified_spend_v1` を直接呼ぶ。

- leafのprivate-field projectionが同じbook owner・現在世代・before totals・現在planから来たこと。
- actual Transactionのcached txidが現在の内容と一致すること。
- 認証済み新resultとprojectionのcase/result/scope一致。旧namespaceの入力はliteral `FundingTermsV1` と旧rowを別に照合する。
- 実ML-DSA公開鍵からprincipal IDとnative address payloadをそれぞれ導出すること。IDをaddressにcastしない。
- 実出力のサービス料・契約burn・payer返金・continuationと、前後の全資金保存。
- 期限後のpayer returnが、提供済みservice feeを消していないこと。

返り値はprivateな算術検査proposalであり、新Positiveが旧escrowをspendする権限ではない。
旧FundingTerms IDを新Consent IDで上書きするfixtureと、旧rowのrooted一致を新受付成功に見せるmethodは廃止した。
A4/A6はversionedな新rowとscript/UTXO検証、row/資源/case/責任/支払markerの同一transactionを別途完成させる必要がある。

既存engineのrow/旧termsはprivate calculator fixtureで、旧FundingTermsに署名して新Result受付を通したものではない。
旧TFO1 funding bodyはproducerのtrace-root付きclaimへliteralに結び付いている。
旧FundingTerms.idを新serviceに埋めてそのclaimをResultから導出すると循環identityになる。
新funding bodyはA0が別namespaceで定義済みで、上記の実payer署名/actual output preflightへ接続した。
native row生成・実lock・rooted fee/policy resolverは別のまま維持する。

## 検証

```sh
scripts/ci/positive-accounting-node-ci.sh
POSITIVE_ACCOUNTING_NODE_COMMAND=clippy scripts/ci/positive-accounting-node-ci.sh --all-targets --no-deps -- -D warnings
cargo test --offline --locked -p misaka-palw-positive-accounting-node
```

runnerは実Cargo manifestのdependency/feature/dev-dependencyを解決し、sourceを参照する一時workspaceを作る。
root lockfileをコピーし、ネットワークなし・debug情報なし・incrementalなしで検証する。
nodeだけの一時workspaceを実行しており、native node processの起動・合意処理の試験とは区別する。

実A1/authpacket経路7試験、実signed fundingと旧engine calculator経路15試験を用意した。
実算術期待値は小TIRの `[8,14,3,14] → token 1` を別に固定する。
偽の出力、別result、plan差替え、再起動でflagだけ残すケース、reorg、旧codecへの流用、重複・支払順序、
不正payee、古いtxid/支払projection、旧terms namespaceの借用、deadlineによるearned fee消失、
payer署名の差替え、source/Court cap、capital、actual output、狭いintent期限の反例を検査する。
原ログは `../positive-accounting/evidence/independent-phase2/` と後続 `independent-phase3*` に保存し、
候補rev1の容量不足とrev2のsource修正を別に記録する。
typed rows境界の後続7試験とlib clippyは `../positive-accounting/evidence/phase4-native-rows/README.md` を参照する。
その実state fixtureは既存public carriageによるローカルDTO snapshotであり、native Originの成立を示す試験ではない。
phase4ではnative受付/coin lock、全consumer、実process、再起動/reorg/IBDの完成を主張しない。

workspace/lockfile登録はA0 `1eecf492a` / `6aeb4c42c` で適用済み。元の `workspace-registration.patch` は提案履歴として保持する。
全native reader・Gate C対象の具体提案は `../../../handoff/positive-court/A5-native-credit-api.md`、
共有接続の調整記録は `../../../handoff/positive-court/A5-to-A0.md` に記載。
有効化設定は変更しない。
