# A0 — native接続契約 v2/r1

2026-10-31。[共通契約](COMMON-CONTRACT.md)、[TIR annex](TIR-INTERCHANGE-R1.md) のnative接続先。これはcodec/fold/実資金/全consumerの完成宣言ではない。以下の番号・署名scope・原子的境界をA0が所有し、A4/A5/A6が実装する。

## 1. 名前空間

`positive-contract/src/native.rs`が定数とkeyの正本。

- lifecycle object **176** を `PositiveCourtEventV2` 用に中央 `PALW_A2_TAG_ALLOCATIONS_V1` へ予約。既存170/171/175/178、旧Miner-trace形式を再利用しない。現時点では予約のみで、176のlive decoder・kind・fenceはまだない。
- kernel-route aux table **151** を使用。150（court capital）、161（rejected response）と別。既存aux root、`KernelRouteRow` delta 160、carriage tail `0xEC`を使い、空の新top-level root blockを追加しない。
- keyはBorsh `(2u16,1u16,NativeRowKey)`。kind 0 Result、1 ClaimIndex、2 Case、3 Witness、4 Coverage、5 SourceUnion、6 ServiceReceipt、7 Funding、8 PacketReceipt、9 JournalHead。keyのidentity型と順序はcodeを参照。
- 新claimのGate C subject prefixは **P（0x50）**、IDは `native::certification_subject(full_result)`。旧S/K/V subjectや任意旧claim IDの承認を借りない。全LLMで実際のclaim抽選承認の`Certified`が必要で、`PublicCheckCertified`、Positive単体、一致したlocal replayを代替にしない。

namespace予約だけでaux151を任意bytesとして受理しない。A6は`wire_kernel_aux.rs`へ正規key/value型のbounded scanner・NativeHeap・復元整合性を接続する。未知key、孤立Witness/Case、誤ID、別schema、上限超過を拒否する。

## 2. 安定したscope

`ChainScope.branch`は、そのcaseに割り当てた**祖先anchor**であり、刻々変わる現在tipではない。native viewは現在のlineageにanchorが存在すること、branch固有のResult/Case rowが存在すること、同じaccepted-prefix/state世代であることを確認する。reorgで消えたcaseのsaved Passを移植しない。

新規受付のrulesetは、nativeが解決する既発火fork digestとPositive policy digestから `native::ruleset_id` で導出する。将来のactivation予定全体を含むparams fingerprintを署名rulesetにしない。caseは受付時のscope/policyを固定し、後続forkでの既存case処理は明示的移行規則で決める。Minerのoptional flagで旧形式へ戻せない。

## 3. Resultから始めるfunding scope

旧 `FundingTermsV1.execution_root` は旧Miner署名traceの意味を保持する。Result IDや、後から作るPanel rootをそこへ詰めない。旧TFP1/TFO1 installerに新形式を偽装しない。

`funding.rs`の`FundingScopeId`は、次の**別domainでの正規projection**：

```text
MinerResultCommit 全field（service_contractだけ0）
+ MinerServiceContract 全field（funding_consentだけ0）
+ FundingPlanBasis 全field
```

それ以外のoutput、input、class/program/artifact/tokenizer、scope、Miner、依存、provider委任、資料予算、fee/保持期限等はすべて含める。これはMiner結果署名のfield省略ではない。最終ResultとServiceは元の全fieldでhash/署名し、nativeは`Result.service_contract == Service.id()`、`Service.funding_consent == FundingConsent.id()`、projection scope、CaseIdを再計算する。

この順なら、case/job/結果core → basis → payer consent → Service ID → Result IDを一度で構成でき、hash固定点は不要。Basisのquote/catalogは**最終Result ID、Service ID、Scope ID、consent ID、carrier TxIDを参照してはならない**。`coverage_plan_id(result,...)`もResult IDを含むのでBasisへ直接入れない。Basisのderivation/countを登録programから再導出し、最終Resultが確定後に対応するPlanIdを導出する。

`Basis.policy/schedule/resource_quote`はpeerの任意digestではない。A4/A5は同じrooted resolverでpolicyと料金表の正規preimage（source unit/completion/carrier、recipient、額、burn/返金条件）を解決し、digestとderived quoteを再計算する。raw DTOやpayer署名だけで別料金表を差し込まない。resolver tokenは現在viewに結び付け、commit時にも再検査する。未割当なら拒否する。

`FundingConsent`はpayer principal、CaseId、ScopeId、最大fee/lock額、実carrierのoutput index、返金P2PKH address payload、排他的な最終受付時刻を署名する。**自分を含むcarrier TxIDを署名payloadへ入れない**。nativeが実carrierとoutput indexからoutpointを求める。期限は受付期限であり、自動返金・既存義務の消滅期限ではない。

feeの範囲は区別する：`Service.budget.fee`はMinerへの有限資料サービス契約の料金上限。`FundingConsent.max_fees`はそのpayerが同意したCourt全体のfee上限、`Basis.required_fee`はnativeの全分岐quoteから導出するCourt全体必要fee。Panel計算料を資料料へ混入させない。private fee bookの初期額はfee部分であり、capital-bearing native lock全額と同一視しない。capitalの予約・返還・他claim責任はnative ownerの別勘定として同じ原子的更新に含める。A4/A5は資料料金合計をService上限以内、全feeをpayer上限以内に制限する。`required_fee + required_capital`はchecked addで、実output額が必要額以上・`max_locked`以下であることをnativeで確認する。

返金payloadはnativeのML-DSA P2PKH address payloadで、unkeyed principal/key IDを流用しない。`AuthenticatedFunding`はpayer署名確認のみ。実UTXO・支払能力・coin lock・admission・native支払権限を示さない。[独立vector](funding-vectors-r1.json)は合成単体fixtureであり、公開feeパラメーターではない。

## 4. 実資金と原子的受付

native acceptor（`palw_state_v2` / VirtualProcessor）だけが、次を保持する非公開・単回消費の準備capabilityを作る。

1. 実ValidatedTransaction、composed UTXO entries、現在selected parent/state root/accepted-prefix位置・世代・DAA。
2. 登録class/artifact/program/tokenizer、正規job/input/Miner割当、実A1から導出した全計画と原資料catalog。
3. 全分岐・全Panel・全試行の資源leaseと有限duty。署名/両framing/保持/復旧費も含む。
4. 新payer consentの署名・terms・実funding output、必要fee/capital、返金/recipient条件。
5. 元と次のjournal head、行・資金・責任のundo。

commit時にも現在view・時刻・funds・capacityを再確認し、case/予約/coin lock/一回払いmarker/責任/journalを同じfold checkpoint・PALW delta・UTXO `WriteBatch`で確定する。kernel route headerがなければ明示的に拒否し、writerのno-opを受付成功にしない。公開setterを別crateへ作ってone-writerを迂回しない。

既存のfunding preflightやpayer自身へのP2PKH出力だけではlockにならない。新Positive funded rowを既存escrow lock view・mempool/blockの同一拒否predicate・same-mergeset暫定lock・qualified spendへversion付きで接続する。aux FundingRefだけ設置して実coinを再利用可能にしない。旧`PalwFundedEscrowRowV1.contract_id`の意味は変更しない。

現行G14には料金・capital・retentionの推定値と承認済みpaid-ingress estimateがある。一方、production funded-original resolver、rooted receiptからのdues、全役割/供給/保持/復旧の契約適用は未接続。全料金未定義と扱わず、推定値をPositiveの実測済みprofileとも扱わない。既存の常時拒否を仮の`Ok`にしない。

## 5. Native eventとrowの責務

176のversioned bounded bodyはResult受付、Witness commit、署名済みExactFrame、資料要求、既存G14資料応答、Finish、Expireの操作を区別する。Finish/Expireは参照だけを提出し、Passedやclockをpeerから権限として受け取らない。A4のbody schema提案とA6のscannerが揃ってからlive decoderをA0で登録する。

Result rowはliteral署名commitとnativeで解決した役割/登録/受理origin/funding scopeを保存。Case rowは単一Result、全計画・source契約・予約・試行・三進捗を保持。Witness rowは実作者、署名、attempt、固定時刻とそのrecordを保持。Coverage rowやsnapshotのcursorはauthorityではなく、復元は認証済みcarrier履歴と既存検査を用いる。SourceUnionとunit receiptは全Panelで共有し、Panel/relay/nonceをdedupe keyに入れない。

署名済みframeの認証は[TIR annex](TIR-INTERCHANGE-R1.md)に従う。Record header等の単なるopeningが壊れている場合も、第三者の搬送だけで正直なcandidateを消費・有罪にしない。全費用の上限はnative wrapperと署名を含める。

## 6. 同一candidateの最低native試験

既存`T12Chain`／mempool→block template→heartbeat foldを使う。全node-commitの小さいTIRを既存実登録経路で登録し、実ML-DSA入力・実UTXOのResult受付とcoin lockを確認。warm/cold Panel→全証拠→model未保有Nodeの独立検査、source義務とPanel責任の分離、actual P-subjectの抽選承認・成熟後の全consumer、一回だけの適格サービス精算を試す。

さらに別Nodeのblock replay、途中DB reopen、reorg、IBD/pruningで、root・資金・予算・dedupe・cursor・責任が一致することを確認する。既存の「不完全な受付を拒否しfallback出力を保つ」fixtureは維持し、それを新形式の受付成功の証拠に数えない。公開presetはNoneのまま、検証用の有限private policyだけを明示選択する。
