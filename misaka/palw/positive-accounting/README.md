# A5: Positive Court の信用・会計・依存関係

2026-10-31。A0 common contract **version 2 / revision 1** に対する先行実装。
`CreditGate` と `AtomicCourtStore` の接続口を実装した。後続のnode crateで、実A1検査terminalと既存escrow精算checkerへの接続まで検証した。
**native新claim受付・全consumer・永続化の接続完了ではない。**
native の信用認証と資金受付は、未接続の間は必ず拒否する。ネットワーク設定・wire tag・署名domain・既存IDの意味は追加・変更しない。

## 変更範囲

| ファイル | 内容 |
| --- | --- |
| `src/credit.rs` | 現viewに対する共通信用判定、全祖先の走査、循環・別branch・別result・不完全coverageの拒否 |
| `src/credit/consumers.rs` | 推論報酬・receipt・DAG weight・Rule E・決済・prefix・依存の全7用途の同一gate入口 |
| `src/fees.rs` | 有限なprivate会計参照実装。実原資を模した明示的初期残高、hold、適格サービスの一回精算、返金、契約burn、原子的受付とrollback |
| `src/credit/tests.rs` | 全7用途の同一判定、未検証祖先、独立claim、diamond、循環、binding差替え、上限 |
| `src/fees/tests.rs` | 支払・返金の重複、Panel交代、未検証終了、失敗の原子性、rollback、古い準備・別storeのtoken拒否 |
| `tests/` | 独立到達/循環oracle・独立算術oracle・consumer用途転送の全列挙試験 |
| `../positive-accounting-node/` | 実A1terminalを所有するviewと、実Transactionを既存escrow engineへ渡すnode adapter |
| `../positive-accounting-node/src/rows.rs` | 実coreのaux151 typed read/bundleへ接続したdata-only信用入力read。欠落・破損・link・署名失敗とnative owner未実装を区別 |
| `../../../scripts/ci/positive-accounting-ci.sh` | A0所有のworkspace登録を変更せず実行できる、root lockfileを使う検証入口 |

元checkoutの未コミット変更は移動・削除していない。A5 branch はA0の正式共通契約commit
`a85152edc` を基準にする。A0の正式commitを採用する統合先には、A5の実装commitだけを適用する。

## 信用判定の接続

`PositiveCreditGate<V>` は A0 の `CreditGate` を実装する。`RootedCreditView` が次を現在の実行済みstateから解決する。

1. 新署名resultに結び付いたclaim/case/result/output/dependenciesと、network/ruleset/branch。
2. A1が導出して受付時に固定したplan・必須relation数と、A1/A4によるローカル検査の完了結果。
3. 現Gate Cの抽選検証者承認、適格fraudの状態、既存の満期・責任・carrier・資金条件。
4. 署名された依存commitmentから認証した正規の親一覧。空一覧も認証する。再利用stateのexport identity/root/位置/model/tokenizer/layoutの照合は、このadapterの責任。

全祖先について同じ条件を評価する。未検証Aに依存するCはholdし、独立Bは進行できる。
scopeはclaim別の固定ancestry anchorとして `validate_scope` で検証し、current tipとの単純一致へ置換しない。
祖先の取消し・未解決告発・view変更は次の読取りへ直接反映し、子の保存済みready flagを使わない。
祖先数・edge数には明示的上限があり、超過時に走査を省略して許可することはない。

公開DTOの `PositivePassed`、任意のGate C boolean、旧claim IDの認証rowだけでは許可しない。
Gate Cの正直な割合の前提は既存規則のまま維持する。モデルの階層・登録モデルの審査をこの判定へ加えない。
`CreditAuthorization` はviewを借用する内部型であり、constructor・deserialize・native支払権限を持たない。
leafの `UnavailableNativeCredit` は、全用途を `MissingNativeResultBinding` として拒否する。
node crateの `TirCreditView` は実A1terminal・署名済み新result・A0 canonical selector/planを照合する。
そのviewに渡すGate C/資金/依存のrooted native解決は別の条件であり、単なるlocal matchでは埋めない。

A0 `327393e8e` のaux151 valueとcore `PositiveRowsV2::read/bundle`を使う `PositiveCreditRows` も接続した。
実stateのrow欠落、malformed value/key、cross-row link、Miner/payer/Witness署名の失敗を区別し、
成功時も `SignedCreditInputs` というdata-only結果に留める。`NativeRowCreditGate` は同じ境界を全7用途で読み、
後述の8 native ownerが未実装のため認可型は `Infallible` のまま拒否する。旧S/K/Vの認証へfallbackしない。
必要ownerはaccepted origin、accepted prefix/import世代、登録期待値、local replay、live funding/capital、
依存/export、現在Court/資源、実P draw Certifiedである。
A0 `069d54007` のP statement DTO認証は実抽選の`Certified`ではなく、`PublicCheckCertified`も代用しない。

## 料金・原子性の接続

`PrivateAccountingStore<A>` は A0 の `AtomicCourtStore` を実装する。
これは有限なprivate参照bookであり、既存money engineを置換する実装ではない。
`AccountingAuthority` のnative実装では、A0/A4/A6が同じrooted viewで以下を解決する必要がある。

- `admission`: 署名済みpayer consent、実資金、料金上限・全schedule・返金/burn条件、全資源quote、intentとの完全一致。
- `service`: Node確認済みの正規source unit・回答carrier・Positive完成scopeと、その実recipient。Panelの受領ACKは根拠にしない。
- `claim_status`: 完成feeを払う時点の現在のclaim結果。
- `closed_case`: sourceの残義務・料金を含めて終了可能であること。Positive完了だけで未応答source義務を消さない。

未実装の既定methodは拒否する。`UnassignedAccountingAuthority` のnative受付に成功経路はない。
private初期残高は明示的なテスト/統合入力であり、UTXO検査やpayer署名の代わりではない。

受付は読取専用prepare後に、残高debit・case・資源hold・undoを一体で確定する。
同一caseの同一契約へのretryは追加holdを作らず、同じcaseへのresult/契約の差替えは拒否する。
精算keyにpayee・Panel試行・relay・consumer・nonceを入れず、同じscopeへの別入口の支払を防ぐ。
未知のunit/未提供service/不一致recipientには払わない。正常完成feeは現在も `PositivePassed` のscopeだけに一回払う。
source料金・carrier料金は、claimが未検証終了しても提供済みなら契約内で精算できる。
終了時に未精算の適格serviceを先に確定して残額を返し、返金の再実行は何も移転しない。

```text
hold_initial = paid_unique_service + refunded + remaining_hold + contractual_burn
initial_private_supply = balances + remaining_holds + burned
```

両方を更新前に検査する。複数recipientの途中でoverflowや容量不足があっても、先行支払・返金・資源解放は残らない。
undoは残高・hold・支払dedupe・burn・資源・caseを一体で戻す。単調epochでrollback後の古いprepareを拒否し、
owner tokenで別storeへの流用を拒否する。会計側はMinerの計算有罪・slash額を導出しない。

参照bookは有限な全体candidate/before-imageをcopyするため、費用は概ね `accounts + cases × services` に比例する。
nativeのA6実装は既存の資源計上とmoney engineを使い、変更rowを同じjournalへ保存する必要がある。
このbookのundoをそのままIBD/snapshot形式として採用しない。実Nodeでの性能・永続化は未検証。

既存money engineとの接続用に `settlement_projection` を追加した。private-field projectionには現在世代と実支払順序を保持し、
遅れて小さい番号のunitを払っても、既存escrow scheduleが消費済みのprefixを書き換えない。
node bridgeは `validate_projection` でowner/世代/現在planを再検査し、実Transactionのcached IDと出力・返金・burn・継続escrowを
既存 `palw_escrow_qualified_spend_v1` で検査する。新funding consentとnative rowの生成権限は付与しない。
新FundingScopeはA0 `42a60b840` の実payer署名とactual own-output preflightへ接続した。
全予定source料金とCourt全体料金を別capで制限し、capital-bearingをfee-only bookへ混入させない。
旧FundingTerms ID/claimを新namespaceへ転用せず、policy/schedule resolverとnative lockは未完成として拒否する。

## 全consumerの調査表

以下は既存入口の一覧であり、**新Positive経路の接続済みカバレッジ表ではない**。
namespace・固定row value・data-only typed readerはA0で確定済みだが、live decoderとnative authority ownerは未完成なので、
この変更では既存native consumerのreaderを変更していない。
各入口を新経路へ接続する際は、次の用途で同じ `CreditGate` を読み、旧claimの挙動を変えないこと。
基準 `a5800d2dd` のcore/consensus内をGate C・certification predicateで検索すると、コメント/設定を含め116箇所あった。

| consumer | 既存入口（repo相対） | `CreditUse` | 新Positiveの接続 |
| --- | --- | --- | --- |
| legacy発行・worker/user成功報酬・coinbase/model/PWU | `consensus/core/src/palw_state_v2/transition_builder.rs`、`palw_security_certification_fold_v1.rs`、`palw_state_v2.rs` | Reward | 未接続 |
| kernel escrow成功報酬・held reward release | `consensus/core/src/palw_security_certification_fold_v1.rs`、`palw_kernel_route_fold_v1.rs` | Reward | 未接続 |
| native mining reward/receipt生成 | `consensus/core/src/palw_native_mining_fold_v39.rs` | Reward / WorkReceipt | 未接続 |
| receipt mint/spend・carrier資格 | `consensus/core/src/palw_rfc0018_v1.rs`、`palw_native_receipt_v39.rs` | WorkReceipt | 未接続 |
| safe/live/provisional weight、復算・retire・取消 | `consensus/core/src/palw_rfc0018_v1.rs`、`palw_weight_cap_v1.rs`、`palw_state_v2/chain_state.rs`、`palw_state_v2/transition_builder.rs` | DagWeight | 未接続 |
| Rule E参加 | `consensus/core/src/palw_fork_choice_rule_e_v1.rs`、`palw_fork_choice_rule_e_leaf_v3.rs` | RuleE | 未接続 |
| safe frontier・settlement depth・settled anchor | `consensus/core/src/palw_security_certification_v1.rs`、`palw_settlement_v1.rs`、`palw_native_settlement_v1.rs` | Settlement | 未接続 |
| EVM settlement/replay/export | `consensus/core/src/palw_state_v2/evm_replay_settlements_owned_v2.rs`、`palw_claim_local_v1.rs` | Settlement | 未接続 |
| prefix/state再利用・子孫信用 | `consensus/core/src/palw_tir_prefix_state_v1.rs`、`palw_prefix_family_state_v1.rs`、`palw_claim_local_v1.rs`、`misaka/palw/sdk/src/lineage.rs` | PrefixReuse / Dependency | 未接続 |
| SDK/RPC表示 | `consensus/core/src/palw_user_job_views_v1.rs`、`palw_user_status_v1.rs`、`palw_user_views_v1.rs` | 対象credit用途と同じ判定 | 未接続 |

精算接続先は既存 `palw_settlement_once_v1.rs` / `palw_settlement_once_fold_v1.rs` と
`palw_security_certification_fold_v1.rs` のhold/releaseである。既存leg IDを新しい意味に変更せず、
必要な新規割当はA0に戻す。資金受付は既存funded-original coordinatorの原子的境界と合わせる。

## 検証と引継ぎ

```sh
scripts/ci/positive-accounting-ci.sh
```

scriptは一時manifestを作り、repoのlockfileと同じA0契約を使ってA5を検証する。独立integration testsも自動登録する。
一時manifestは終了時に削除する。leafはA0 `4dede1aff` でworkspace登録済みなので、
通常の `cargo test -p misaka-palw-positive-accounting --locked` でも実行できる。
元の `workspace-registration.patch` は適用済みの提案履歴。nodeもA0 `1eecf492a` / `6aeb4c42c` で登録済み。

初期実行済み: A5の29単体試験とA0共通契約の13試験、全成功。
raw logは `evidence/a5-unit-tests.log` と `evidence/a0-contract-tests.log`、
対象sourceのSHA-256・base commit・コマンド・終了codeは `evidence/validation.json` に保存した。
認証済みnative資金、実Court裁定、旧G14全回帰、実process、
再起動/reorg/IBD、実測性能は未検証。単体試験内の権威adapterはsynthetic fixtureであり、
実Courtへ接続できたという評価に加算しない。仮のnative成功処理はない。

並列実装段階ではleaf単体32件・独立oracle/consumer試験5件、実A1terminal経路6件・既存escrow engine経路9件が成功。
後続の同一candidate検証ログは `evidence/independent-phase2/` に追加し、初期ログを上書きしない。
新funding consentを旧FundingTermsへ直接結ぶとresult/service/funding/claimの循環identityになるため、
実UTXO受付は未接続として維持する。共有API/ID・全native readerの具体要求は
`../../../handoff/positive-court/A5-to-A0.md` にまとめた。
その後、非循環FundingScopeは共有契約として確定し、native API提案・固定anchor検証・実署名/actual output検査へ進んだ。
最新の候補別ログは `evidence/independent-phase3*`、row/readerの具体提案は
`../../../handoff/positive-court/A5-native-credit-api.md` を参照する。初期・phase2の原記録は変更していない。
後続の `evidence/phase4-native-rows/README.md` は、A0採用rowを実coreから読む7試験とlib clippy成功、
共通row試験の対象commitの違い、data-only境界を記録する。phase3の旧候補の結果と合算してnative全体完成とはしない。

| 引継ぎ先 | 必要な変更 |
| --- | --- |
| A0 | namespace/workspace/aux151型/P statement DTOは確定。P value/getter・実draw/native owner・live decoder・有効化条件・必要なmoney legを完成 |
| A1/A4 | `checked_coverage` と全必須planのbinding、実サービス受理・実証拠作者・現在status・終了可能性を接続 |
| A5（統合段階） | 上表の全native consumerと既存money engineに同じpredicate/精算計画を接続 |
| A6 | 同一transaction内のnative資金・資源・case・責任・支払markerをjournal化し、復元時にローカル計算履歴を再検査 |
| A7 | 同一candidateで全readerの未検証=0、サービス料分離、二重支払・失敗注入・再起動・reorg・IBDを独立試験 |

有効化設定: **変更なし・無効のまま**。
