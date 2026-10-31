# A5 → A0: native信用rowと読取APIの最小提案

初版の基準: A0 native契約 `42a60b840`、[NATIVE-CONTRACT-R1.md](../../docs/design/palw/positive-court/NATIVE-CONTRACT-R1.md)。
2026-10-31更新。aux151型・`AcceptedOrigin`・core typed readerはA0 `327393e8e` で採用済み、
P statement DTO/署名scopeは `069d54007` で採用済みである。
現在のvalue正本は [rows.rs](../../misaka/palw/positive-contract/src/rows.rs)、P DTO正本は
[certification.rs](../../misaka/palw/positive-contract/src/certification.rs)。
以下のfield提案表は採用前の要求履歴として残す。private native owner、実P value/getter/fold、old/new handleは未完成であり、
署名済みdataのread成功をnative受付・実抽選`Certified`・信用・有効化の完成とは扱わない。
既に予約された `native::LIFECYCLE_OBJECT_TAG`、`KERNEL_AUX_TABLE`、`NativeRowKey`、
`CERTIFICATION_SUBJECT_PREFIX`、`certification_subject`、`ruleset_id`だけを使う。新しい番号・key kind・署名domain・hash domainは割り当てない。

### 採用済みFundingRowの三勘定を集計private bookへ潰さない

`rows::FundingRow` のvalue正本はfee/capital/surplusを別の `BalanceLeg` とする。
native constructorは `fee.initial = basis.required_fee`、`capital.initial = basis.required_capital`、
`surplus.initial = actual_output - fee.initial - capital.initial` をchecked arithmeticで導出する。
`fee.initial <= consent.max_fees` と全初期額 `initial_locked <= consent.max_locked` は別の上限である。
private `HoldTotals` や `PayerSignedFeeOnlyOutput.amount()` はこのrowへの代入済み値でもnative資金capabilityでもない。

説明用に `actual_output=100 / required_fee=27 / required_capital=0` とすると、正規分割は **27 / 0 / 73** になる。
source10・明示burn2を精算し、正規返還条件が成立した未検証終了の集計返金88は、
**fee.refunded=15 + surplus.refunded=73** であり、fee legへ88を記入してはいけない。
private bookのinitial100をfee.initialへコピーすることも不可。これは説明例で、fee値の採用ではない。
capitalありは現A5 fee-only adapterでは明示unsupportedであり、capital・surplusの解放条件はnative ownerが別々に解決する。

全予定source unitの料金合計は `Service.budget.fee`、全Courtサービスと契約burnはpayerの `max_fees` と
正規quote由来の必要feeで照合する。`Case.usage.fee` はMiner資料契約の消費量で、全Court支払の合計を代入しない。
実outputの余剰をfee capへ含めてnative fee額と混同しない。現private集計への保守的な拒否や署名preflightを、
余剰付きcanonical native rowの受理まで完了したと扱わない。

native接続では、全三勘定・現在outpoint・`last_settlement`・実provider/recipientを含む正規dues・
source receiptの一回払い参照・source/資源/責任予算・journal head・実UTXO差分を同じtransactionで更新し、
reorgで同じbefore-imageから戻す。単なる `BalanceLeg.validate()`、receiptの `Some(settlement)`、
保存されたjournal参照は、実eventの受理・現在lock・一回精算を認可しない。
新P attestationの署名成功も、この資金/精算ownerを代替しない。

## 1. 採用後にそのまま実装する境界

A0への要求の現在地は次のとおり。

1. aux151 value型は採用・data-only readまで実装済み。A6のbounded scanner/NativeHeap/restore検査への登録は残る。
2. P statement DTOは採用済み。§3のP subject value型・typed getterと、実抽選`Certified`だけを返す決定capabilityは残る。
3. §4のResult/Caseのaccepted origin・現在lineage・accepted-prefixを照合する読取session。
4. §5のold/newを区別した読取handleを、全consumerの新形式armへ渡すこと。

A5は採用された型を実core `PositiveRowsV2::read/bundle`で読むdata-only bridgeを実装済みである。
`PositiveCreditRows::resolve`は `SignedCreditInputs` を返し、Caseのpositive_witnessがあればWitness/Coverageも共通validatorで照合する。
`NativeRowCreditGate`は全7用途で同じ欠落/破損/link/署名失敗を返し、data確認後も8 native ownerをtyped missingとして拒否する。
origin/prefix、登録期待値、local replay、live funding/capital、依存/export、現在Court/資源、実P drawのcapabilityが揃えば、
ここからnative `RootedCreditView` / `CreditGate` へ接続する。
既存の公開`RootedCreditFacts`や任意`RootedCreditView`実装からnative支払権限を生成するadapterは作らない。
row decode、origin、local replay、資金、抽選のいずれかが未接続なら、対応する理由を返して拒否する。

## 2. aux151 value: A5読取りに必要な具体field

採用後の全valueは `(version, revision, body)`。正確なfield順序・上限はA0 `rows.rs` が唯一の正本である。
下表の順序・仮称は採用前のA5要求履歴であり、codec仕様として使わない。
Result/Case/Coverageに以下のA5必須fieldを持たせ、A4/A6が必要な残りの進捗・予算を同じ型へ統合する。
この表だけでCourt全状態schemaが完成したとは扱わない。

| 既存 `NativeRowKey` | value候補の必須field | native readerが再計算・照合する条件 |
| --- | --- | --- |
| `Result(result_id)` | literal `MinerResultCommit`、Miner公開鍵、Miner署名、`accepted_origin`、採用Positive policy digest、受付時fired-fork digest、`FundingScopeId` | literalからResultId/ClaimId/CaseIdを再導出。署名principal・登録class/artifact/program/input/tokenizer/selectorのnative期待値を照合。`scope.ruleset == native::ruleset_id(fired_fork, policy)`。branchはcaseの固定anchor |
| `ClaimIndex(claim_id)` | `result: ResultId`、`case: CaseId` | Result rowから導出したclaim/caseと完全一致。欠落・孤立・誤indexでは旧claimへfallbackしない |
| `Case(case_id)` | `result: ResultId`、literal `MinerServiceContract`、A1導出`CoverageDescriptor`、現在`ClaimStatus`、現在Miner資料/Witness進捗、現attempt、`positive_witness: Option<WitnessId>`、資源lease参照、責任/成熟のnative参照 | Result.service_contractとliteral Service.id、Service.case、最終funding consentを照合。planは登録programから再導出。PositivePassed時はpositive_witness必須。残source義務・未解決fraudを別に読む |
| `Witness(witness_id)` | literal `PanelWitnessCommit`、Panel公開鍵、署名、`accepted_origin` | 実作者principal・drawn/permissionless提出適格性・case/result/plan/attempt・provenanceを照合。Panel記録をMiner署名traceと見なさない |
| `Coverage(witness_id)` | `case: CaseId`、`result: ResultId`、`plan: PlanId`、必須relation数、受理cursor、累積frame/signature/work予算、最終受理event参照 | cursorと終端状態は復元用cache。これらだけではlocal-checked capabilityを作らない。caseのpositive_witnessと、実A1 terminalのwitness/result/descriptorを一致させる |
| `Funding(case_id)` | `FundingScopeId`、literal `FundingConsent`とpayer鍵/署名、literal `FundingPlanBasis`または認証済み正規参照、実funding outpoint、現在lock outpoint/額、fee部分の初期額/支払/返金/burn/残額、capital予約と残責任の参照、一回精算のnative参照 | 実UTXO/同mergeset暫定lock/qualified spend/現在rowをnative ownerが検証。feeとcapitalを分離し、資料feeをService.budget.fee以内、全feeをConsent.max_fees以内にする。単なるfunded/paid boolを読まない |
| `ServiceReceipt { case, kind, source, unit }` | `result: ResultId`、成立済み有限duty参照、Nodeが受理した既存G14 response carrier参照、実provider、受理時刻、消費bytes/work、料金精算参照 | 原資料root/opening・正規unit・供給機会・受付をnativeで再検査。Panel ACKでは完了しない。keyにPanel/relay/attemptを入れない |
| `JournalHead(case_id)` | A6の現在journal位置/世代・前位置・原子的deltaへの正規参照 | native checkpoint・accepted prefixとの一致を確認。peerの値からsession世代を生成しない |

`SourceUnion`と`PacketReceipt`はA4/A6の正規union/署名付きpacket受理履歴を使う。
推論成功報酬やcoverage済みを示す独立flagをそこへ追加しない。
完成feeをsource unitに見せかけたり、未割当の新key kindで一回払いmarkerを作ったりしない。
完成scope/carrierの精算はA0採用のFunding/既存money engineの正規schedule・journal参照へ結び付ける。

### accepted originの候補

`accepted_origin`は少なくとも次をnativeから固定する。

```text
accepting_block       // foldが実際にこのeventを採用したblock
carrier_block         // mergeset由来ならaccepting_blockと異なり得る
carrier_transaction
accepted_tx_ordinal  // 当該accepted-prefix内の正規順序
event_ordinal        // carrier body内の正規event位置
accepted_daa
accepted_blue_score
```

現在のstate root・import epoch・accepted-prefix世代は読取sessionのnative capabilityへ持たせる。
row自身のpost-state rootをrowへ入れる自己参照hashは作らない。
必要なbefore-rootは既存A6 journalのbefore-imageから照合できる。
peerが宣言したclock、処理順序、selected parentや世代を受け入れない。

## 3. P subject: 実抽選Certifiedだけを許す

subjectは必ずliteral Result rowから `native::certification_subject(result)` を再計算する。
旧S/K/Vのsubject関数や任意claim IDを引数にしたlookupは代用しない。

P-row storageについては、既存 `PALW_SECURITY_CERTIFICATION_TABLE_V1` の予約P namespaceへ置く
typed getterをA0が確定する案。`NativeRowKey`にはcertification kindがないため、
aux151の正規key列へ裸のP keyを混ぜない。配置とkey helperはA0の採用事項であり、A5は推測して実装しない。
既存tableのIDを変更・再利用する提案ではない。

P valueに最低限必要なbindingは次のとおり。

```text
version / revision
subject
claim / result / case
certification_policy             // nativeで解決した採用済みpolicyの正規参照
commit_origin / final_origin
anchor                          // draw用の実beacon/span/execution identity
drawn_positions                 // 実抽選の順序・重複位置を保持
drawn_at / deadline
accepted_positive_attestations  // 新P subjectに対するversion付き承認の受理参照
outcome / decided_at
```

`accepted_positive_attestations`はA0の新P subject bodyを実認証した参照である。
旧`Reproduced`や旧trace transcriptの意味を黙って「公開Positive検査済み」へ変更しない。
新attestation body/署名scopeはA0 `069d54007` で採用済み。
`PositiveCourtAttestationV2::authenticate`は期待されたliteral statementと署名を認証するだけであり、
実draw membership、現在origin、期限、担保、承認位置、`Certified` outcomeを生成しない。
これらnative ownerが未実装の間、P決定capabilityも未実装のままとする。

native P-readerはnative certification foldから返るprivate-fieldの `DrawCertified` associated capabilityを要求する。
これには現在sessionのResult/Case/subject、実draw、採用policy、承認位置、異議/責任windowを結び付ける。
保存されたoutcomeだけをpeerから渡してconstructorを呼べるAPIにしない。
判定は**`outcome == Certified`の明示比較**とする。`PublicCheckCertified`はモデル階層・BASE-0を含めP subjectでは拒否する。
抽選承認の正直な割合の前提は既存規則のまま維持し、Positive計算検査と混同しない。

重要な既存差分:

- `PalwCertificationOutcomeV1::certifies()` は `Certified | PublicCheckCertified` を許すため、新P readには使えない。
- `palw_certification_verdict_v1` はpublic-check classの早期分岐を持つ。そのままPへ呼び出さず、
  新P typed bindingに対し実抽選を必須にした別入口をA0/core ownerが接続する。
- 既存draw/seed計算部品は利用できるが、P subjectの実受理・新attestation・policy確認を省略しない。

## 4. 読取session・local検査・lineage

公開APIの形は以下を提案する。型名は仮称で、shared wire IDではない。

```rust
// 各型はownerだけが作れるprivate fieldsを持つ。Clone/Deserialize/公開setterなし。
NativeExecutedView<'v>          // current PALW state + real accepted-prefix + import世代 + lineage
NativePositiveClaim<'v>         // checked ClaimIndex/Result/Case/Witness references
NativeLocalCheck<'v>            // real A1 CheckedTerminal、またはA6再実行からのみ生成
NativeFundingReadiness<'v>      // live lock/qualified-spend/capital/duty/資源ownerが生成
NativeDrawCertified<'v>         // §3のP-subject決定ownerが生成
NativeCreditAuthorization<'v>  // 以上を同一viewで照合した読取り許可。money移転権限とは別

trait NativePositiveCreditReader {
    type View;
    type RootedClaim<'v> where Self: 'v;
    type Authorization<'v> where Self: 'v;
    type Denial;

    fn resolve<'v>(&'v self, view: &'v Self::View, claim: ClaimId)
        -> Result<Self::RootedClaim<'v>, Self::Denial>;
    fn authorize<'v>(&'v self, claim: &Self::RootedClaim<'v>, usage: CreditUse)
        -> Result<Self::Authorization<'v>, Self::Denial>;
}
```

このnative実装はsealedな具体backendに限定する。任意外部crateの`RootedCreditView`実装や、
`local_checked/funding_ready/gate_c/anchor_present: bool`を渡すconstructorをnative入口に置かない。
native acceptor/VP・A1・A6・money ownerがそれぞれ実処理から作ったassociated private型を合成する。
sessionは実state root、import epoch、selected-parent/accepted-prefix位置をnativeから固定し、
pending apply/commit直前にも同じsessionを再検査する。古い認可をjournal rollback後に再使用しない。

### scopeはclaimごとの固定anchor

`ChainScope.branch`はcurrent tipではない。Result Aと、それを依存先にするResult Cのanchorは異なり得る。
各claimの署名scopeをそのまま保ち、現在lineage上にそのaccepted originとanchorが有効であることを確認する。
同じ現在tip文字列へscopeを塗り替えない。anchor membershipだけでなくbranch固有rowとaccepted-prefixも照合する。
rulesetも受付時fired-fork/policy由来で固定する。別のrulesetやnetworkへの差替えを許さず、
後続forkを跨ぐ既存case処理はA0の明示的移行規則に従う。

A5 leafには次の後方互換portを実装済みである。

```rust
fn validate_scope(&self, binding: &CreditBinding) -> Result<(), CreditDenial>;
```

既定実装は従来の `binding.scope == view.scope()` を要求するprivate単一anchor view用。
native overrideは `NativePositiveClaim` の実origin/lineage/policyを検証して結果を返す。
全祖先のgateと、node `TirCreditView` のterminal登録/coverage読取りは同じmethodを使う。
`TirCreditView`も検証をnative viewへdelegateする。terminal、署名result、expected dependencyのbinding完全一致は緩和しない。
現在のlineageから親originが消えた場合は子もholdし、無関係なclaimは進行できる。
単にcallerが渡したanchor一覧をnative authorityへ昇格させる実装にはしない。

### local-checked capability

保存されたCoverage cursor/Case.PositivePassedはauthorityではない。
`NativeLocalCheck`は、A1の実`CheckedTerminal`のresult/witness/descriptor/outcomeと、
現在Caseのpositive_witness・A0正規planを照合してのみ作る。
fresh node/IBDでは、A6が認証済みcarrier・署名付きframeを同じ規則で再実行するまで生成しない。
cacheは現在state/import世代/accepted-prefix、case/result/witness/plan/scopeへ結び付ける。
不足資料・別branch・pruned履歴で自己検査が未完了ならcreditを保留する。

依存一覧の正規preimageはResult valueに添付するか、その認証済みcarrier履歴を参照する案。
新しいNativeRowKey kindは作らない。Result.dependenciesとのdigest照合、export identity/state root/位置/
model/tokenizer/layout、親の正規claim/result/CaseIdをnativeで検証する。
空一覧も認証し、未解決を空一覧として返さない。全祖先を現在viewで検査する。

## 5. 全consumerに渡すold/new handle

nativeが解決した非公開constructorの `ResolvedCreditSubject<'v>` を用意し、
`LegacyTrace(OldClaimHandle<'v>)` と `PositiveResult(NativePositiveClaim<'v>)` を明示的に区別する案。
判別は実受付形式/rulesetとrooted rowから行い、`ClaimIndexがないならLegacy`という推定はしない。
Positive解決失敗から旧S/K/V rowや旧pre-fenceのtrueへfallbackしない。
旧handleは従来のGate C/fence/報酬規則へそのまま渡し、旧claimのbyte/会計を変えない。

| `CreditUse` | Positive armの挿入箇所 | 拒否時の結果 |
| --- | --- | --- |
| Reward | Final/held reward release・issuance/coinbase/model/PWU・worker/user成功escrowの支払前 | 成功報酬なし。既に提供されたサービス料金は別のqualified fee処理 |
| WorkReceipt | mint時と各spend時、native receipt資格の検査時 | mint/spend/自己carrier資格なし |
| DagWeight | live/provisional/safe加算、復算、retire/取消し | 一貫して0。以前の加算は同じjournalで取消し |
| RuleE | 参加資本とleaf/復算 | 参加資本0 |
| Settlement | safe frontier/depth/settled anchor/EVM settlementの根拠claim読取 | 進行しない。既存のsource/depth条件も維持 |
| PrefixReuse | export/stateをtrusted inputへ再利用する前 | 再利用creditなし |
| Dependency | 子claimの信用を進める前 | 当該子孫だけhold |

RPC/SDK表示も同じtyped handleと判定理由を使う。PositivePassed、DrawCertified、CreditReadyを区別する。
`NativeCreditAuthorization`を得ても、実支払はA4/A5/A6の同一fold checkpoint/UTXO batchを通す。

## 6. 既存公開APIの再利用と足りないtyped入口

| 既存API/位置 | 利用範囲・必要な差分 |
| --- | --- |
| `PalwChainStateV2::{kernel_route_state_v1,state_root,last_point}` (`palw_state_v2/chain_state.rs`) | 同じ現stateからrowとpointを読む。import/accepted-prefix/ancestryのcapabilityはこれだけでは得られない |
| `PalwKernelRouteStateV1::aux_row<T>` (`palw_kernel_route_v1.rs`) | decode失敗と欠落を共にNoneへ落とす旧API。新aux151は実装済み `PositiveRowsV2::read` のtyped Resultを使い、欠落と破損を分離する。A6 scanner/restore/NativeHeapへの登録は残る |
| `preflight_native_value_v1` / `NativeWireScanV1` / NativeHeap | key/valueをallocation前に検査し、所有rowのleaseを取る。A6がaux151とP-rowをscanner/restoreへ接続 |
| `reachability_service.try_is_chain_ancestor_of`、selected-chain store (`virtual_processor/processor.rs`) | VPで実anchor lineageを解決。store/importの不明・エラーはUnavailable。callerのcanonical boolを入口へ流さない |
| VPのpruning/import statusとepoch照合 (`processor/routed_material_v2.rs`) | 同じimport世代でのrooted read/refreshの既存パターン。Positive用accepted-prefix検査と結合 |
| `native::{ruleset_id,certification_subject}` | literal/受付時policy/fired-forkから再導出。現tipやfuture全体fingerprintへ置換しない |
| `palw_certification_draw_v1` / `palw_certification_seed_v1` | 抽選の既存部品。P用の実受理・policy・新attestation・明示Certified判定は別途必要 |
| `palw_*security_certified_v1/v39` と `PalwCertificationOutcomeV1::certifies()` | 既存S/K/V専用のまま保存。Pへ流用するとPublicCheckCertifiedを通すため禁止 |
| A1 `CheckedTerminal` / `TirNodeChecker::check_authenticated_part` | 実算術terminalと署名込み予算を使用。DTOの自己申告では代用しない |
| `PalwValidatedClaimCheckpointV1::{dependencies,credit_eligible,affected_by}` (`palw_claim_local_v1.rs`) | 認証済みclaim-local依存検査の再利用候補。新Result/anchor/export bindingと新P approvalが別途必要 |
| `funded_escrows_v1` / `palw_escrow_qualified_spend_v1` | 旧funded row・実Transactionの保存則/出力検査の部品。Positive lock versionをA0/native ownerが接続するまで旧rowの意味を変えない |

## 7. 型採用直後の最小実装手順

1. value型はA0採用済み。P value/getter・old/new handleの採用と、A6のtyped scanner/restore登録が残る。
2. **DATA read部分は実装済み**。node `rows::PositiveCreditRows` が実coreのClaimIndex→Result/Case/Funding bundleと
   Witness/Coverageを読み、欠落・malformed・link・auth失敗を区別する。data成功からnative ownerを作らず、
   全7用途は `NativeRowCreditGate` で同じmissingを返す。実P decision/current-view/readinessとの合成は残る。
   writerはA4/A6既存one-writerに残し、shared module登録はA0が行う。
3. node側で実A1 terminal/A6 replayとnative sessionを合成し、`validate_scope`と既存leaf gateへ接続。
4. 各consumerにtyped old/new dispatchを追加。Positive armだけがこのgateを呼び、旧armの回帰を維持。
5. 同一candidateで、別固定anchor依存の成功、親origin除去、P以外のrow借用、PublicCheckCertified、
   保存PassだけのIBD、funding解除/reorg、署名packet拒否、全7用途の拒否と成熟後の許可を検証。

この順序ならA0採用後に必要なAPIやfieldを再発明せず最小adapterを実装できる。
現在の `UnavailableNativeCredit` を根拠なく成功へ変えない。本番presetの有効化は変更しない。
