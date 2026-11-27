# RFC-0010 — モデル資料・共有供給・有限資金契約

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**DESIGN_SPECIFICATION**。採択済みの規則を統合する。提案に限る部分は該当節で明示する。
- 判断理由：[ADR-0010](../adr/0010-public-materials-and-funded-service.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. 静的モデルとclaim資料

モデルの重み/config/tokenizer等の静的artifactと、各claimの入力・trace・Readout・資料供給契約を分離する。取得場所はidentityを代替しない。HF等を利用しても正確なrevision、全ファイルdigest、model rootを照合する。

定期HF認定や匿名取得証明の成功を自動モデル採用へ接続しない。モデル追加はBFTが判断する。BFT承認済みでも必要資料がなければ個別claimの成功は保留又は未検証終了となる。

## 2. 供給計画

元claimへartifact、元実行commitment、対象unit/position、provider、consumer、元期限、collection/retention区間、料金、担保、容量を固定する。固定した上界を超える入力は受付前に拒否する。

必要資料を誰が保持・再供給するか、cold verifierの取得、最大同時要求、branch/reorg後の保持まで計画する。資料URLとACKだけで検査機会を保証したとみなさない。

## 3. Original・役割別同意・native予約

Originalにproducer、provider、verifierの役割別scopeと明示同意を結合する。他人のbondや予算を自己申告で拘束しない。資金tx、署名slot、対象source、予約量、消費・返金先をnativeで確認する。

予約と支払いは同じ予算行の有限残高を使う。一部消費、ゼロ額、再送、別carrierでも使用済みslotを復活させない。料金上限を超えた請求を黙ってclipして成功扱いにしない。

## 4. 共有query data

資料の識別子はownerに依存させず、認証された元root・position・codec・範囲で定める。複数claimが同じbytesを共有しても各参照の責任と期限を保存する。

第三者が同じ正規資料を供給でき、元ownerの無応答が排他的な拒否権にならないようにする。response eligibilityを選択親の認証履歴から復元し、再掲Originalで先行caseを隠さない。

## 5. 分割搬送と終端

Open/Part/Finish、native relay、ready-root opening、終端materialは元予約・sequence・総量・digestを検査する。重複partは冪等又は明示拒否とし、新しい役務報酬を作らない。

Finishが正しくても未取得資料や未裁定caseを成功にしない。collectionの完了とretention終了を区別し、閉鎖recordが金融責任を解消する条件も別に検査する。

## 6. 有限費用と保持

通常verifierのfee consentは予算・最大質問数・最大取得量・元期間を固定する。追加要求、fallback、再試行による無限の署名義務を作らない。資料共有でbytesを節約しても認証・索引・復元の費用をゼロ扱いしない。

local資源不足、供給者不在、偽資料、署名不一致、期限切れを異なる型で返す。供給機会を保証できない失敗から自動slashしない。

## 7. 検証項目

偽root、違うmodelの正しいtile、範囲外unit、別case、同意なし予約、同一slot再利用、再掲によるcase消失、供給競合、最遅応答、欠落body、走査上限、reorg、restart、pruned復元を確認する。

資料取得の成功、全量再実行の成功、Courtでの裁定、成功Finalをそれぞれ別の記録にする。


## 8. Positive assignmentと原資契約

本節は採択済みの限定契約と、その実装前に確定を要するwire候補を定める。`prover`は再実行・局所証拠の役務であり、暗号学的計算証明を生成しない。op・aux・domain・数値policyの候補は未割当状態を維持する。モデル採用はRFC-0007、全モデルの経済はRFC-0016、公開Court条件はRFC-0011へ結合する。

最初の経路は既存TIR登録、受理済みCanonicalTirAssignment、後続body0 Result、Panel、Node、P、poster-funded settlementとする。新Resultはprimary L1 workを代替せず、PAV2/PAV3、own-template job、class ticket、L1 tag、PoW、既存発行規則を変更しない。assignmentの発行・block/Final weight entitlementはB=F=R=0とし、実資金・役務費用・資源までゼロにしない。

### 8.2. native正本と権限の境界

assignmentの正本は、actual native transaction validatorと現在のaccepted-prefix foldが同じstate世代で受理した新rowだけとする。署名・DTO・hash一致・mempool表示・local journalは受理を代替しない。

最初のprivate profileは **poster=payer、worker=data-source、prover** の3役に限定する。3役とも実native bondであり、same accepted-prefixの登録key・仕事受付資格をnativeが検査する。posterは仕事と支払条件、workerは推論と有限資料供給、proverは再計算・record/evidence生成の有限契約へ明示同意する。workerに全証拠生成を移さない。兼任を許す場合もrole別署名と会計を省略しない。別payer・別providerへの委任は初期profileでは拒否し、後の別profile決定とする。無署名の第三者に仕事・債務を課さない。

初期modeは `CanonicalTirAssignmentV1` の一種類とし、任意prompt、任意seed、任意decode長、動的EOS、opaque外部処理を追加しない。TIR登録そのものは既存の登録規則で行い、assignment受付をモデル品質・公開性・保持者数の審査にしない。claimに必要な資料供給は、そのclaimの有限な署名契約として扱う。

`AssignmentPolicyV2`の正規preimageには、version/revision、mode、Positive policy ID、assignment/Case fee schedule ID、physical service profile ID、frame/program/layout caps、global/per-poster/per-worker/per-prover/per-class live caps、全role候補数・prover join数上限、全履歴/保持/復旧caps、accept/result/build/check/fraud-tail/refund各windowを必須fieldとして提案する。countはu32、bytes/work/金額/DAAはu64、IDはDigestとし、全演算をcheckedにする。実branch schedule・金額・単価は対応preimageから解決する。初期の代替提出を可能にする候補/joins上限は正かつ有限とし、ゼロ・最大整数・未定義IDを「無制限」「無料」「即時成功」の代用にしない。具体値は未採択であり、本節で公開値を与えない。

### 8.3. AssignmentTermsと独立したjob導出

`AssignmentTermsV2`は、次の順序のcanonical Borshを提案する。整数はlittle endian、Digest・native BlockHash・TransactionIdは各64 bytes、native bond outpointは64-byte TxIDとu32 indexである。opaqueな可変blobをtermsへ入れない。

| 順序 | field / 型 | 意味 |
| --- | --- | --- |
| 1 | version:u16, revision:u16 | 新assignment schema。2 / 1を提案 |
| 2 | network:Digest, ruleset:Digest, branch_anchor:NativeHash64 | native network/genesis、既発火forkと採択policy、独立した祖先anchor |
| 3 | mode:u8 | CanonicalTirAssignmentV1のみ。識別値0を提案 |
| 4 | registered_class:Digest | 既存native TIR class ID |
| 5 | poster:NativeOutpoint, worker:NativeOutpoint, prover:NativeOutpoint | 最初の3役の実native bond |
| 6 | payer:PrincipalId, refund_payload:Digest | payer同意者とnative P2PKH返金payload |
| 7 | poster_nonce:u64 | 同posterのmonotone nonce。使用済み値を永久に再利用しない |
| 8 | derivation_policy:Digest, source_policy:Digest, reserve_schedule:Digest | nativeが解決する正規preimageのID |
| 9 | worker_reward:u64 | poster原資からの推論成功対価。新発行ではない |
| 10 | max_assignment_fee:u64, max_assignment_capital:u64, max_assignment_locked:u64 | 事前勘定の署名上限 |
| 11 | accept_before_daa:u64, result_before_daa:u64, reserve_until_daa:u64 | 全てexclusive。policyの有限期間内でのみ受付 |

`AssignmentId = H_new_assignment_terms(canonical(full_terms))`とする。domainの提案名は `misaka/positive/assignment-terms/v2/r1`。実際の登録は中央レビューで行う。このIDに結果、署名、独自salt、同意ID、source catalogの派生ID、quoteの派生ID、自分の受理origin/hash、state root、actual lease IDを入れてはならない。`poster_nonce`以外の自由なidentity nonceを設けない。

nonceはnative poster bondごとのO(1) `next_nonce:u64, exhausted:bool` で管理する。初期はnext_nonce=0/exhausted=false、受理値はnext_nonceに厳密一致させる。成功時にchecked incrementし、u64最大値を受理した場合はexhausted=trueとして以後の新assignmentを拒否する。取消・期限切れ・資料回収・Case終了で戻さず、同じcanonical lineageで使用済み値を永久に再利用しない。再送はtermsだけでなく、全unsigned bundle・全attachmentsが元のaccepted preimageと一致し、署名・同じactual原資/lockの現在認可を再確認できる場合だけ無効果に扱う。異なるbundleや原資差替えをidempotent成功にしない。資料回収後の古いnonceはConsumed/MissingMaterialとして拒否する。counterは資料retentionと別で、snapshotに必須とする。正規reorgではその受理とcounter増分を一緒にundoし、世界共通のlocal spent fileを使わない。

新assignment fenceの完全policy preimageを、Params/fork identityのSome-only処理と実既発火fork resolverへ登録することを実装条件とする。r1の既存native ruleset recipeを別の意味へ読み替えず、assignmentを支配するpolicyがnative rule scopeへ認証されていない間は受付拒否する。単に外部JSONのpolicy IDをtermsへ記すだけではfork契約にならない。

nativeは登録class/artifact/program/layout/tokenizerを解決し、既存 `palw_tir_attempt_canonical_of_v1` でcanonical `(prefill,decode)` を導出する。初期modeは**full canonical pair**を使い、primary miningのprefill-draw短縮を暗黙適用しない。その上で既存 `palw_tir_attempt_job_for_anchor_of_v1(facts, AssignmentId, canonical, native_prompt_form)`を使う。prompt formはassignmentのnative rule scopeで固定し、held classの既存Merkle規則を維持する。

これは既存の式を新assignment anchorへ適用する**新job mode**であり、旧own-template anchorと同じjobだとは主張しない。program、数値型、registered max context、layout、算術、token選択規則は変更しない。canonical式が成立しないclassや有限予約を超えるclassは拒否し、contextを縮めて通さない。

期待値は次のように固定する。

| r1 field | native期待値 |
| --- | --- |
| version / revision | 既存r1定数の2 / 1。assignmentの別versionを転記しない |
| scope | accepted assignmentのnetwork/ruleset/branch anchor |
| job | AssignmentIdをanchorとして導出したcontext.job_id |
| execution | 同context.context_hash() |
| miner | worker bondの実登録keyから導出したPrincipalId |
| registered_class / artifact / program | native TIR登録metadata |
| input / generation_seed / prefill / decode | 同じ導出contextの値 |
| tokenizer | 同context.tokenizer_id。canonical TIR yardstickのゼロを維持 |
| max_context | 同context.max_context_tokens。layoutのposition上限は別途既存TIR規則で検査 |
| execution_rules | 導出context.cu_ruleset_id。canonical TIR yardstickのCUゼロを維持。scope.rulesetとは別 |
| selector | 既存共通greedy selector ID |
| temperature_q24 | 0 |
| dependencies | 初期modeはnativeが導出する認証済みempty preimage。任意依存を受け付けない |
| output | workerが署名固定する既存output commitment。assignmentは結果を持たず、Nodeが後続全関係検査のcandidate outputと一致を検査する |
| service_contract | literal r1 MinerServiceContract.id()。後続Cの署名上限・payer同意と全linkを検査し、A同意IDやassignment IDで代用しない |

registered tokenizerは別のnative metadataとしてclass IDにより認証する。r1 Resultのtokenizerを登録値へ黙って差し替えない。TIR contextのtoken budgetとlayoutのposition budgetの既存の違いも維持する。

### 8.4. 同意attachment、実原資、事前予約

terms確定後に、unsigned `AssignmentAdmissionBundleV2` を作る。内容はfull terms/AssignmentId、native導出job/quote/catalogの照合用digest、`AssignmentFundingAuthorizationV2`、`AssignmentSourceConsentV2`、`AssignmentProverConsentV2` のunsigned固定宣言とbounded preimage参照である。署名・同意ID・actual lease IDはbundleの署名対象へ自己参照させない。terms IDにはこのbundle IDを戻さない。nativeはbundleの照合用digestを権威として信用せず、同じ実登録/政策/clockから再導出する。

同bundleには将来C用の固定 `FutureCaseFundingBoundsV2` を含める。fieldはAssignmentId、payer:PrincipalId、refund_payload:Digest、worker:NativeOutpoint、max_case_fee/capital/locked:u64、case_admission_before_daa:u64、service_before_daa:u64とする。output/ResultId/Case funding scopeは含めない。3役はこれを含む同じbundleを認証し、Cの実r1 consentは後でoutputを含めて追加認証する。A専用capsとこのC用capsを混同しない。

poster/payer、worker/data-source、proverの各keyが、同じunsigned bundle digestへ**role別の新署名contextで一つずつ**署名する。poster署名はpayer funding宣言、worker署名はworker/source宣言、prover署名はprover宣言をそれぞれ明示的に認証する。別の旧purpose署名を流用せず、同じbundleの3役同意をactual native keysで検査する。初期op8は3つのrole署名を要し、現0..7の2署名上限を緩めない。外側transaction/script/lifecycleの署名が別にある場合は、その実検査・massも別に計上する。

funding authorizationは、AssignmentId、native導出quote ID、payer、funding_output_index:u32、refund_payload、fee/capital/reward/total cap、exclusive admission expiryを固定する。自分のcarrier TxIDは含めない。actual carrierのoutput・金額・script・TxIDはnative validatorから取得する。元入力の通常script認証、現在のcomposed UTXO、既存lock、same-block spendを省略しない。存在するoutputを予約済みとみなさず、coin lockとrowを同じcheckpointで作る。

実A初期額は、checked演算で `A_initial >= worker_reward + required_assignment_fee + required_assignment_capital` を満たし、payerの署名total capとterms.max_assignment_locked以下とする。fee/capitalも対応する署名上限以下でなければ拒否する。成功判定前の部分返金/支払後も、未払worker_reward・未払適格service due・未終了reservation/liabilityの必要floorを実残額が覆うことを再検査する。予想返金を先払い原資や追加の課金余力に数えない。

quote/catalogの要件形状は登録program・canonical countsと採択policyから導出する。AssignmentIdが確定してから計算するinput root・catalog ID・quote ID・実leaseは、accepted rowの派生資料として保存する。terms自身へ含めない。peerのquoteやpolicy digestだけで価格・capacityを認可しない。

source consentはAssignmentId、登録source root、nativeの正規unit種別・範囲、全Panel合計のrounds/unique/transmitted bytes/work/retention/fee上限、exclusive cutoff、fee-onlyか有限責任か、解除条件を認証する。モデル登録から無署名の供給義務を導出しない。旧withdrawn Artifact provider leaseを復活させない。canonical promptはNodeが再導出でき、不要なMiner要求を作らない。

初期worker/data-sourceが必要weightのsole holderである場合、nativeはそのclaimが読む**最悪全量**のunique data、既存opening、再送、IO/hash work、保持と復旧をquoteする。「Panelが多分持っている」を予算削減の根拠にしない。全量が有限上限と実供給/予約へ入らなければ拒否する。proverが既に資料を持つ場合は実要求をゼロにできるが、admissionの最悪条件を未署名の取得可能性仮定へ置換しない。

prover consentはAssignmentId、native job/context、profile/derivation/selector、全必須relation数・bound、再計算/record構築/分割提出のwork・RAM・bytes・保持・復旧上限、同意したfee/capital、build/submit cutoffを固定する。output依存の将来PlanId/WitnessIdを先に要求しない。Case後にnativeが導出するexact planと本人のWitness署名を別途検査する。prover同意は実role permissionの正本であり、Pの事後抽選承認ではない。登録keyの存在だけでこのpermissionを作らない。

**初期profileのA段では、実data要求/応答op3/4、witness生成・提出の合意サービス実行及びその支払を禁止する。** Aで提供済みとして精算できるのはassignment admission、固定metadata retention等の専用purposeだけである。workerの通常推論はsource追加処理/有料data応答ではない。上記data/prover同意は将来C用の要件・permissionを固定するものであり、実要求、全Panel合計source usage、proverサービス課金はCase受理後に初めて開始する。Aに同unitのusage/fee counterを作ってCでreset・再払いする未定義bridgeを持ち込まない。将来preCase data/proverサービスを認めるなら、認証済みunit/receipt/counter/feeのA→C移行を別の明示的仕様で採択する。

最初のproverは実native role契約で固定するが、永久の独占席ではない。本RFCに従い、代替提出者は同じAssignment/Caseの期限・全試行予算のまま、新purpose同意・実bond・必要fee/capital/容量をnativeで取得できるbounded join/交代を提案する。元proverのACKや裁量承認を必須にしない。joinを未実装のまま自由代替が完成したと称さず、単なる署名差替えで新しい予算を与えない。任意peerの未認証資料はrole取得・attempt消費・有罪の根拠にならない。

joinはtermsのinitial proverを書き換えず、同じAssignmentIdの追加role rowを作る。poster/payerのop8同意が許した全候補合計fee・capital・capacityの範囲だけを使う。初期joinで追加原資が要る場合は、新prover本人のjoin-purpose認証と実debitに限定し、第三者payerへの未署名課金やassignment payerの差替えは認めない。新prover本人の同意は必須で、旧proverへの新規task停止と既存dutyの残存を区別する。role rowをhashだけで参照して実署名・scope・clock・lease検査を省略しない。

op13のjoined proverも、op8と共通の有限取消条件と、既発生債権/capital/未完dutyを消さない規則へ明示署名する。取消・期限切れ・Case終了時はinitial proverだけでなく全live/retired join契約のactual native記録を走査上限内で照合する。initial 3役の同意だけで後発joinの権利を消してはならない。

事前assignment勘定を `A`、後続Case勘定を `C` とする。

```text
A_initial = A_qualified_service_paid + A_worker_success_paid
          + A_refunded + A_contractual_burn + A_remaining
C_initial = C_qualified_service_paid + C_refunded
          + C_contractual_burn + C_remaining
```

各式はchecked arithmeticで原資単位ごとに保存する。Aはactual admission/固定metadata retention等の専用サービス、worker成功対価、明示した返還可能capitalを区分する。Cはr1 Result確定後のdata/prover/Court/carrier等のfee・capital・surplusを区分する。Aで払った同じservice scopeをCから再支払しない。初期profileにはpreCase data/proverサービスのcounter移行を設けない。原則としてAとCに同じoutpointを二重lockしない。purpose資金の移替が必要なら実debit/creditとonce markerを同じwriterの一checkpointで行い、再送を新原資に数えない。

**事前Aに、output依存のr1 FundingConsentを使用しない。** Case Cでは既存r1 funding_scopeの二fieldだけをzero化する規則を維持し、完成したResult/Service/Consentの全linkを検査する。Result.outputや他fieldを追加で省略しない。Aの同意はCの同意・原資の代わりにならない。必要C原資を期限内に揃えられなければ、Case成功は成立しない。

既存fee/capital計算、actual UTXO qualification、resource reservation、settlement-once、native journal基盤を再利用する。ただし旧TFO1やKernelJob/Native39の目的・IDを読み替えず、新purposeの型付き契約を追加する。数値、schedule、profile、service supply、全branch費用が未定義ならtyped missingで受付拒否する。private finite policyの完全なpreimageと実native供給・予約を必須とし、常時拒否を成功に数えない。

### 8.5. 一writerによる受付、body0、状態遷移

`AcceptAssignment`は、actual current-prefix/state世代を独占する既存native budget writerが次をall-or-nothingで行う。

1. fence/version、frame/metadata caps、全署名、network/rule/ancestor scopeを検査する。
2. class登録metadata、worker/poster bond key・現在の仕事受付資格、nonce、clockを検査する。旧model審査を追加しない。
3. 正規job・source/catalog・finite quoteを導出し、全global/per-poster/per-worker/per-prover/per-class live枠と保持・復旧枠を予約する。
4. 実原資A、coin lock、worker/sourceとproverの実role契約、actual capacity leaseを同じprefixで確保する。
5. assignment/nonce index/派生資料参照/資金/予約/role rowを同じnative delta・journal・UTXO checkpointへ書く。

結果がないこの時点ではCaseRowやResultRowを作らず、ゼロResultIdも保存しない。accepted assignmentは `Open` となる。Minerの通常推論は行えるが、初期profileの合意data/proverサービスはまだ開始しない。off-chainの任意作業をaccepted data receipt、課金可能なwitnessサービス又は未採択assignmentの受理へ昇格しない。

NodeはResult受理から元の公開・異議期間、資料要求・回答、Court受付・裁定、支払・返金まで、合法な全branchの有限な資源と期限を予約する。result_before と最悪所要期間の和が reserve_until 以下であることをchecked演算で検査する。全branchの上界が決まらなければ受付を拒否する。旧profileの成立済みassignmentには元の抽選・失効・返金条件を適用する。

単に将来capacityや自己申告のP所要日数を整数rowへ書くだけでは足りない。op9はこの同じpolicy、残余期間、actual lease、P用data供給/保持cutoffを再検査し、reserved envelopeからCaseへ原子的に移す。actor local実行資源とnative Node全体資源を区別し、実供給・inclusionの仮定を無条件の進行保証としない。必要なowner/preimageが無ければ受付拒否する。

後続body0は、signed literal r1 Resultと既存Case funding bundleを保持し、**追加のaccepted assignment参照**を別operationで結合する。body0の旧意味を変更しない。新Resultの期待値をそのpayloadからコピーしてはならない。nativeが同じorigin lineageのaccepted assignmentを読み、§8.3の全期待値とworker署名を検査する。

具体的にはop9がAssignmentIdと既存body0 **payload**を一つの新versioned受付として原子的に検査する。op0で先にCaseを受理し後からassignmentを貼る二段階処理ではない。assignment-modeのResultを参照のない旧op0へ迂回させず、peerが申告したjob digestだけで割当済みとみなさない。

op9では **C.payer == Assignment.payer == native poster principal** とrefund_payloadの同一性をMUST検査する。worker recipient、exact AssignmentId/CaseId/ResultId、source/prover scope、signed fee/capital/total caps、expiry/deadlineと受理済みassignment条件の対応もMUST検査する。termsのA専用capをCへの支払許可として使わず、op8 unsigned bundleで固定した将来C用identity/上限と、output確定後の実r1 payer consentをともに満たす必要がある。違うpayer/refund先への差替えや片方だけのcap照合では受付しない。

最初のCase受付で `Open → Consumed(CaseId, ResultId)` とし、Case全試行予算・実C原資・全必須関係・有限source duties・依存empty資料・P用正本を同じcheckpointで作る。ここがdata/proverサービス及びPのcommit clockの開始点であり、Assignment受理時刻をPの先行commitに使わない。消費はResultのoutput/fee/Panel/relay/attempt/Case labelを変えても一度だけである。同じResultの再送も、全unsigned bundle/attachmentsの一致と現在の署名/原資認可を再確認して初めて元receipt又は無効果として扱い、新予約・新料金・新clockを生まない。異なるbundleをidempotent成功にせず、別Resultへの再束縛は禁止する。

assignmentのscopeは受理時の独立ancestorに固定する。後続bodyの現在tipへ置き換えない。「全branch一消費」は、**同じcurrent canonical executed view・同じorigin lineage内**で全Result/Caseラベルを横断して一消費という意味である。排他的forkの一時状態を世界共通のlocal spent fileで固定しない。reorgは消費・Case・資金・receipt・予約・支払を同時undoする。新lineageにoriginがなければ再利用不可とする。端末の実I/O・実計算済み時間はchain undoの対象ではなく、再実行やlocal journalは合意上のreceiptを捏造しない。

assignmentの計算/署名policyは受理時に固定し、後続forkの現在policyへ差し替えない。既存assignmentの期限・精算まで旧preimage/codecを保つ。将来forkで受付継続を止めたり既存契約を移行したりする場合は、別の明示的fork移行規則を要する。新Caseのactual原資・prefix・残余capacityの現在検査は、この不変な計算ruleの再解釈とは区別する。

### 8.6. 期限、取消、放棄、返金

Open assignmentの取消は、consumedでないことと、既に受理したsource/service duty・資本責任が消えないことをnativeで確認する。payerがworker/proverの実施済み契約対価を一方的に消せないよう、早期取消は**poster/payer・worker/data-source・proverの全3役**の署名、又は全3役がpolicy/bundleで予め署名した取消条件を必要とする。

初期op10はposter/payerとworkerの2つの新取消署名を用い、proverがop8 bundleで予め同意した取消条件（未提供業務の停止、適格receiptの対価・残責任の保存）をnativeで検査する方式に限定する。proverの既発生権利を変更する3者再交渉を、2署名の取消へ紛れ込ませない。条件外の取消は拒否し、通常の有限期限と残責任の精算へ進める。

op13で参加した全live/retired proverも同じ取消条件へ署名済みであることを確認し、その全既発生債権・capital・未完dutyをactual native history/rowsから検査する。initial同意だけを読んでjoin契約を消してはならない。initial A段のdata/prover実サービスは禁止であっても、join資本・固定metadata保持等の適格な別purpose権利を消してよいとはしない。

`result_before_daa`以降の新Resultは拒否する。Openの期限切れは `ExpiredUnconsumed` であり、推論成功報酬なし。Aの実提供サービスだけを正規receiptから精算し、未提供fee・worker成功対価・責任終了済みcapital・surplusを正規返金先へ返す。時計だけで未終了dutyや資本lockを解除しない。

Consumed後の取消はない。CaseがPositivePassed、ClaimRefuted、EndedUnverified等に進んでもassignmentをOpenへ戻さず、別Result/別Caseで予算を買い直さない。Case終了と、Miner/source応答完了・Panel完了・Node完了・残責任終了を別記録とする。Panelの放棄や偽証拠だけでMinerを有罪にしない。未検証終了ではAのworker成功対価を払わず、Cの実サービスと未消費返金を別精算する。原資・署名契約なしの罰金を作らない。

Courtの計算一致状態とPの認証状態を別に保持する。PositivePassedでもPがPending/NoPopulation/Expired/NotCertified等ならWorkerPaymentを認めず、計算一致を取り消したりMiner fraudとみなしたりしない。既存P policyが認証不成立を確定し成功対価の可能性が消えた場合、未払適格serviceとreservation/liability floorを残した安全な返金を行う。P不成立だけを理由に永久lockせず、pending-fraud等の残責任は署名契約/実policyに従って別に扱う。

### 8.7. 提案namespace、row、署名、保存形式

以下は**中央未割当の追加案**であり、現在のdecoderを表すものではない。global lifecycle tag176とaux151の既存用途を維持し、別global tagを無断採番しない。

| 提案local operation | 内容 |
| --- | --- |
| 8 AcceptCanonicalAssignment | terms、poster/payer・worker/data-source・proverの3役認証、bounded layout/consent attachments、実funding index |
| 9 AdmitAssignedResult | AssignmentId＋既存body0の全Result受付bundle。新modeの独立入口 |
| 10 CancelAssignment | AssignmentId＋明示取消認証 |
| 11 ExpireAssignment | AssignmentIdのみ。actual native clockで判定 |
| 12 SettleAssignment | AssignmentId＋accepted Case/receipt参照。金額・Passedはnative導出 |
| 13 JoinAssignmentProver | AssignmentId/既存Case、代替proverの新purpose同意、必要資金・actual予約。予算/期限をresetしない |

local 0..7を変更・再番号化・読み替えない。aux151 key/valueもBorsh `(2u16,1u16,variant)` の既存0..9を保持し、次をappendする案とする。

| 提案kind | key / 固定valueの要点 |
| --- | --- |
| 10 Assignment | AssignmentId → full fixed terms、accepted origin、worker key/Principal、derived job/input/context/registered metadata digest、status、clock、派生資料・予約・責任参照 |
| 11 AssignmentNonce | poster outpoint → permanent next_nonce/exhausted。資料回収・取消・期限切れで退役しない |
| 12 AssignmentUse | AssignmentId → immutable CaseId/ResultId/accepted origin。未消費時はrow無し |
| 13 AssignmentFunding | AssignmentId → payer認証、actual outpoint、purpose別初期額/支払/返金/burn/残額、actual lock・reservation参照 |
| 14 AssignmentSettlement | AssignmentId＋typed leg＋party＋ordinal → 実debit・recipient・amount・accepted receipt・once marker参照 |
| 15 AssignmentRole | AssignmentId＋role＋native bond → actual登録key/Principal、role別署名済みbundle、同意scope/上限/clock、原資・lease・accepted origin参照 |
| 16 AssignmentJournalHead | AssignmentId → next ordinal、last frame digest、actual accepted origin、保持clock |
| 17 AssignmentJournalFrame | AssignmentId＋ordinal → 固定typed frame metadataとbounded payloadのaccepted material参照 |

可変layout/source集合/履歴はbounded carrierとaccepted materialから解決する。固定rowの既存codec ceilingに収まるfield構成と署名保存を、中央schemaレビューで確定する。key/value version、順序、重複、未知kind、孤立Use、nonce不整合、rowリンク、署名、clock、履歴・残額を、bounded ingressとrestoreの両方で検査する。実resource ownerが全temporary/retained/replay費用を負い、DTOのbooleanや整数台帳をnative leaseへ昇格しない。

3つのkey/signatureを一つの16,384-byte rowへ詰め込まない。AssignmentRoleは**一役一row**で、その役のkey/signatureとunsigned bundleのaccepted material参照を持つ。全role rowが同じbundle、terms、派生quote/catalog、actual originへ結び付くことをrestoreでも検査する。主Assignment rowは3署名の無検査hash summaryではなく、これらの正規rowを必須linkとして解決する。

新op8には専用 `MAX_ROLE_SIGNATURES=3` と、3組のnative key/signature、3役宣言、layout/参照/metadataを含む新codec ceilingを提案する。keyを搬送する方式なら3組だけで21,657 bytes（3×(2,592+4,627)）あり、outer framing等を別加算する。実fixed field長・bounded attachments・temporary/retained heap・検査work・physical massを保守的にquoteし、private finite policyのframe capへ入らなければ拒否する。既存0..7を3署名対応として再decodeしない。fence未満は新operationを署名/資源/資金処理前に落とし、公開presetを開かない。

新ID/domainの提案対象は、AssignmentTerms、unsigned admission bundle、assignment-purpose funding/source/prover宣言、prover join、取消message、資金/予約/精算purposeである。初期3役のbundle署名contextは別にし、全署名がnetwork/rules/AssignmentId/役割を固定する。文字列・enum値は中央domain inventoryと全既存namespaceを照合してから登録する。r1 Result/Case/Witness/P subjectの既存ID/domainを変更しない。旧settlement legのsubject意味を変えず、必要legをappendして既存once engineへ接続する。

#### 8.7.1 Result前のjournalと実history reader

Result前に既存CaseJournalを使ってはならない。既存CaseJournalはCaseId/ResultIdを必須とするため、ゼロIDや架空Resultで流用しない。新 `AssignmentJournalV2` と上記head/frameはassignment-purposeの別namespace/domain追加案である。domain提案名は `misaka/positive/assignment-journal-frame/v2/r1`。header Notice・旧Kernel/Native39のhistoryを読み替えない。

frameの固定metadataはversion/revision、AssignmentId、ordinal:u64、prev_frame:Digest、event_kind:u8、actual AcceptedOrigin、payload_digest:Digest、payload_bytes:u32とする。payloadはbounded canonical typed eventであり、Open（full terms/bundle/3役認証/実funding・予約記録）、JoinProver、A専用purposeのadmission/固定metadata receipt、Cancel、Expire、BindResult、Settlementを区別する。preCase data/prover receiptは認めない。BindResultだけが実CaseId/ResultIdを持ち、native受付済みbody0へのexact参照を含む。frame digestはこの全metadataと正規payload digestにbindし、ordinal=0はprev=zero、それ以外は直前digestと連続ordinalを必須とする。新headの前提が現在headに一致しなければ、stale/ABAとして拒否する。

native writerはhead/frame/material retention、nonce、role、原資、lease、後続Case linkを同じcheckpointで進める。actual accepted eventをprivate constructorのreaderから読み、current prefix/lineageを再検査する。任意trait実装、保存row summary、policy+recordだけでhistory再生成功を宣言しない。payloadの実byteと署名、元のclock、nonce遷移、目的別資金・予約・全receiptの連鎖を、先に有限bytes/work/heap上限を予約したreplayで検査する。CaseJournalの最初の新境界にはAssignmentId・BindResult frame digest・accepted originをexact参照として結び、両journalを取り違えない。

前段materialの保持費用・全frames・復旧replayはassignment quoteへ含め、必要なCase/残責任/返金/rollback期間を覆う。historyがpruning等で失われ、適格なactual native資料から復元できなければ、source/session/admission/creditは `Unavailable/未検証` とする。local recordから補った成功を作らない。そのことだけでMiner/proverの有罪にせず、存在するcoin lockや責任を勝手に解除しない。permanent nonce counterと既存once消費証跡は、payload回収とは別に保存する。

### 8.8. Court、P、用途別entitlement、実送金

Panelが全再計算とrecord/evidenceを構築し、Nodeが全必須関係を独立検査する。資料・全Panel合計予算・期限・帰責は本RFCを維持する。Pのactual selected-verifier Certifiedは別の必要条件であり、旧S/K/V subjectやPublicCheckCertifiedを借りない。

P検査者の資料取得・保持も同じCaseのnative source contractと全体予算へ含め、別名のP要求としてMinerの上限や同unit feeをresetしない。caller permissionはactual P draw membership/current lineageから導出し、prover join権限やPanel署名をP承認へ昇格しない。最悪のP取得人数・再送・公開資料保持を既存policyからquoteできず、必要なsource cutoffへ入らなければ受理しない。

計算信用・推論成功対価・結果利用には共通Positive predicate **AND その用途の既存entitlement**を必要とする。実提供済みservice feeや安全なrefundは、actual receipt、未払due、reservation/liability floor、実原資、onceにより別判定し、未検証/P未認証だけで禁止しない。このassignment自身のnative発行・block/Final weight entitlementは **B=F=R=0**。これは実worker送金額やservice/capital/queue/UTXO費用がゼロという意味ではない。

| 用途 | 初期modeでの扱い |
| --- | --- |
| Aのworker成功対価 / WorkerPayment | exact assignment-consumed Result、PositivePassed、全coverage、P、満期・責任・fraud条件、実escrow、onceを全て満たしてposter原資から一回。旧Reward用途へ流用しない |
| 適格service fee・安全返金 | 署名契約・actual receipt・未払due・reservation/liability floor・原資・onceで別判定する。未検証/P不成立でも適格service対価と安全返金を永久停止しない |
| mining issuance / coinbase / model leg / DagWeight / RuleE / WorkReceipt mint-spend | assignment単体では拒否。新発行・採掘代替を作らない |
| settlement / dependency / prefix reuse | 消費先の既存用途が認める場合だけ、exact bindingと全共通gateを追加検査。Poster送金済みを別用途のwork権利とみなさない |

generic `PositiveCreditGate`だけで全用途をgrantしない。native adapter/consumerが用途別の出所と数量を認証する。A/Cとも実coin lockとdebit、recipient、schedule、once marker、payout queue容量を同じwriterで処理する。旧KernelJob/Native39のID、claim、certificate、escrow、quotaをコピーして充足扱いしない。

paid worker successには純APIのappend-only用途 `WorkerPayment` を提案する。旧 `Reward` はprimary issuance/coinbase/model/PWU等の既存用途として保持し、WorkerPaymentの許可をRewardへcast・別名化しない。WorkerPaymentはactual assignment原資・worker recipient・契約額とnative once legへ厳密に束縛し、他の7用途へ伝播させない。これは送金目的の分離であり、新たな発行権ではない。


### 8.9. 受入条件と有効化

r1全fieldと独立native期待値、元max context/CUゼロ、別worker/key/ancestor/rules/class/layout/inputの拒否、output依存funding cycle不在、nonce/bundleと全Result/Caseの一回消費、実UTXO/lock/prefix変更、payer/refund/caps、A/C二重払い、join/取消/未完duty、全原子的rollbackを検査する。

Panel/P不成立を無関係なMinerの責任へ転嫁しない。Panel不成立・資料供給期限違反とfinite bound欠落、P未完での計算credit/WorkerPayment拒否、適格service/refundの進行、WorkerPaymentからRewardへの誤用、reorg/restart/pruning/IBDの同値を検査する。

有限policy/scheduleのpreimage、実役務とnative block/globalのCPU/RAM/通信/保持/復旧、全consumer接続、独立reviewを有効化前に確認する。旧claimは元規則を維持し、未発効時の既存fingerprint/root/wireを変えない。設計採択だけで公開activationを行わない。

## 9. 限定funding relayとcustody契約

本節は採択済みの限定契約を定める。新operation/row/domain/suffixは中央namespaceレビューまで未割当とし、資料の存在や保存成功フラグだけで原資受付・支払いを成立させない。既存のnative署名・script・sighash・txidを変更しない。

### 9.4. 固定relay transaction template

最初のprofileは一種類、`NativeOp12NoCaseNoReceiptsV1`を提案する。

| field | 固定値／native導出値 |
| --- | --- |
| transaction version | 0 |
| locked input | 必ずinput 0。登録対象のexact native outpoint |
| populated entry | 登録で宣言するexpected amountと、native payer keyから導出する既存P2PKH script |
| input sequence / sigop count | 0 / 1 |
| lock time / gas | 0 / 0 |
| subnetwork | 既存`SUBNETWORK_ID_PALW_LIFECYCLE` |
| payload | 既存lifecycle version、object176、literal `(2,1,op12{assignment,case:None,receipts:[]})` |
| sighash type | 既存`SIG_HASH_NONE | SIG_HASH_ANY_ONE_CAN_PAY`、`0x82` |
| signature context | 既存transaction用`kaspa-pq-v2/tx/mldsa87` |

現在のraw wrapperではpayloadは77 bytes（outer 2+1、body 4+1+64+1+4）となる。実装時のcodec試験でexact bytesを固定し、他のwrapperや任意payloadを同一templateとして扱わない。template IDはこの正規導出規則と全固定fieldへbindする新purpose domainを中央登録する。

この署名で変えられるのは出力と他のfee入力である。現在入力のoutpoint、entry amount/script、sequence、sigop count、transaction version/lock time/subnetwork/gas、payload全文とhash typeは変えられない。署名hashに入らないentryのDAAを、future AcceptedOriginとして作ってはならない。

signerは、outputs空の**署名hash計算用template**を使ってよい。それを有効なbroadcast transactionや受付済みUTXOと扱わない。実spendは現在nativeが導出した全出力、必要なforeign fee inputs、通常のcanonical `<signature || 0x82> <payer key>` unlock scriptで構成し、Full script検査を通す。

このprofileのop12で`receipts:[]`は「carrierがreceipt subsetを選ばない」を意味する。nativeは実rooted historyから、当該accountについて現在適格なdue・refund・burn・必要continuationを全て導出する。空リストから「serviceなし」「P認証済み」「残責任なし」を導かない。`case:None`でも、Consumedならactual AssignmentUseとexact Case/Resultを解決する。input 0の現在lockからA/C/prover purposeを一意に決め、複数locked inputsを拒否する。既存の通常payer署名による非empty receipt op12をこのtemplateへ読み替えない。

### 9.5. 新しい登録operationの提案

#### 9.5.1 local op14

object176内のappend-only local `RegisterFundingRelayV2 = 14`を提案する。現8–13 decoderを黙って14対応にせず、独立したfence・bounded scanner・signature/work quoteを登録する。global object tagは追加しない。

bodyは`version:u16=2, revision:u16=1, operation:u8=14, terms:FundingRelayTermsV2, purpose_signature:[u8;4627], relay_signature:[u8;4627]`とする。keyはtermsのnative bondから解決する。外側transactionの全通常認証とmass/feeを別に検査・計上する。

`FundingRelayTermsV2`のfield順序を次の案とする。enumは有限なfixed variantsのみで、untrusted Vecは設けない。

| 順 | field |
| --- | --- |
| 1 | version:u16、revision:u16 |
| 2 | contract_network:Digest、contract_ruleset:Digest、assignment:AssignmentId、contract_anchor:NativeHash64 |
| 3 | relay_ruleset:Digest、relay_policy:Digest |
| 4 | payer_bond:NativeOutpoint、payer:PrincipalId、refund_payload:Digest |
| 5 | funding_purpose:RelayFundingPurposeV2、target:RelayTargetV2 |
| 6 | funding_outpoint:NativeOutpoint、expected_amount:u64、template:RelayTemplateV2 |
| 7 | register_before_daa:u64、funding_admission_before_daa:u64、custody_until_daa:u64 |
| 8 | metadata_fee_schedule:Digest、physical_custody_profile:Digest、max_metadata_fee:u64 |

purpose tag案は`PosterA=0 / CaseC=1 / ProverJoin=2`。targetは次の四種類を提案し、purposeとの組をnativeで限定する。

- `InitialAssignment { bundle:Digest, poster_nonce:u64 }`。
- `InitialCase { result:ResultId, funding_scope:FundingScopeId, consent:FundingConsentId }`。
- `InitialProver { prover:NativeOutpoint, join:Digest }`。
- `Continuation { original_funding:NativeOutpoint, predecessor:NativeOutpoint, parent_spend:NativeHash64, original_contract:Digest }`。purposeは元accountを維持する。`parent_spend == funding_outpoint.transaction`を必須とする。

terms IDとpurpose署名contextは新purposeとして別登録する。domain候補は`misaka/positive/funding-relay-terms/v2/r1`、purpose署名context候補は`misaka-positive/funding-relay-register/v2/r1`。これは既存transaction署名のcontextを変えない。terms IDは両署名、R.txid、accepted origin、post-state root、actual local leaseを含めない。literal R material digestは全body14（両署名を含む）の別domain hashとし、termsへ戻さない。

#### 9.5.2 Rの受付が確認するもの

1. native fence、network、relayの実既発火rule scope、signed contract anchorのactual ancestry、完全な有限policy/schedule/profile、frame/signature/heap/work/DB caps。
2. actual native bond/keyとpurpose認証。initial Aは現在のposter、initial Cはaccepted Assignmentのposter、initial joinはjoin本人のprover/payerへ限定する。initialは実仕事受付資格を確認する。continuationは元accountの保存key・契約権を確認し、退役した既存契約を新しい仕事受付と混同しない。
3. expected scriptはnative keyの既存P2PKHへ独立導出し、refund payloadとpayer Principalを検査する。expected amountは正でnative金額範囲・R profileの宣言上限内にあることを確認する。既に読めるAssignmentの将来C上限又はcontinuationの現在残額も越えないことを確認する。まだ搬送されていないFのconsent/basisを検査済みとは扱わず、実Fで全上限を再検査する。
4. 宣言されたexpected entryと固定templateから通常ML-DSA transaction sighashを再計算し、relay署名を実検証する。**このexpected entryは署名確認用DATAであり、将来UTXOの存在又は受理を認定しない。**
5. 固定target宣言と現在実際に読める契約をboundedに照合する。initial Aのbundle digestは将来Fとの一致用であり、まだないfull bundleの検査成功ではない。nonce参照だけでnative counterを増やさない。initial C/joinは現在accepted Assignmentとそのscope/capsを確認するが、未搬送の新r1 consent/joinを受理しない。continuationは現在の元account・predecessorを確認するが、未来parent spendの適格性や新出力の存在を認定しない。
6. actual R carrierの利用可能feeからmetadata chargeを一度割当し、実custody・DB write/replay容量を同じnative checkpointで確保する。資料、row、fee charge、pending countは一緒にcommit/undoする。

初期profileのinitial A登録は、宣言nonceがnativeの現在next nonceと等しいことだけ確認できる。counterがなければ既存規則のnext=0として読み、Rだけでcounter rowを作らない。同nonceで複数の将来Fを準備できる量はpending capsで制限し、最初のF成功後に他のRが新Assignment権を持つことはない。

Rのみでassignment/job/Case、実funding output、coin lock、通常推論receipt、役割契約、Assignment nonce消費、計算credit、WorkerPayment、発行・payoutを作らない。Rが登録したのは、料金を払った有限metadata custodyだけである。

同じexact outpoint/purposeの再送は、full terms・両署名・全materialが元のaccepted preimageと一致し、実custodyが維持されている場合だけ無効果に扱う。異なるterms、署名byte、target、custody期限への差替えをidempotent成功にしない。通常carrier feeと二回目のregistration chargeを区別する。初期profileにpayerによる早期R取消・material回収・同key上書きは設けない。

### 9.6. FとRの具体的な順序

初期profileでは、Rが**Fを検査するselected-parent canonical state**に既に存在することを要する。同じcarrierへ同居させず、同じblock内の未確定な先行処理へ依存しない。将来、同mergesetの先行Rを認める場合は、普通のUTXO walk前にも見えるactual prefix interleaveと共通mempool/template規則を別profileとして検証する。

1. payerはliteral op8/op9/op13のF、actual funding output、元の必要同意を完成し、Rに依存しないF.txidを確定する。
2. payerはそのexact F outpoint/amount/P2PKHに対する固定op12 relay署名と、新purposeのR terms署名を発行する。
3. Rを通常transactionで搬送する。nativeが署名、実metadata fee、caps、custodyを受理し、正規rowとraw materialを保存する。これが失敗してもFはまだ仕事・Case・原資契約として受理されていない。
4. Rのcanonical受理を確認後、Fを搬送する。F payloadにR digest/txidを追加しない。
5. F受付はactual own funding outputと現在composed UTXO、元の全署名・scope/caps、bond、capacity、nonce/onceを再検査する。exact outpoint＋purposeでRをnative lookupし、現在lineageのR origin・full material・署名・custody残期間・target一致を確認する。actual entryのamount/scriptでrelay署名をもう一度検証する。
6. Fの必要なA/C/join row、lock、役割・資源、実R bindingを一checkpointで確定する。R receiptだけで実原資やservice capacityを充足したと扱わない。

元のr1 FundingConsentとoutput-dependent scopeはそのまま検査する。C用R termsがResultIdを持つのは、完成したC carrierを準備した後であり、Result前のAに架空Resultを置かない。R purpose署名が元payer consentを置換することもない。

RだけがcanonicalでFが来なければmetadataサービスとその費用だけが成立する。Fが普通のtransactionとして受理されても、R等の前提を欠いてinitial native contractが拒否された場合は、そのpayer出力に契約を作らない。既存契約のlocked inputを消費する部分精算は別であり、後からnative処理だけをskipしてspendを残してはならない。

### 9.7. 部分精算とterminal精算

#### 9.7.1 全current cursorに残す不変条件

新profileの全live accountは、現在の実funding cursorと同額・同scriptのaccepted Rを持ち、そのRは少なくとも必要なterminal/rollback期間を実保持する。accountの元funding outpoint、budget、scope、once subjectはcontinuationで変えない。

最初のFでこの条件を作る。部分精算Sは、旧cursor用の既存Rでinputを認証できるが、新cursorを作るなら、その具体的なS.txid/output index/remaining amount向けR2をSの前に受理していなければならない。SはR2 hash/txidを参照せず、native lookupする。

Sのoutputsやfee inputsを変えるとS.txidが変わり、R2のtargetと一致しなくなる。relayが自由に後からそれらを変えてもpartialが成功するとは保証しない。payerはSの具体形を確認してR2を署名・登録する必要がある。

S受付は、旧current row、適格due、WorkerPayment条件、未払due/floor、実出力保存則、既存once、R2のactual accepted material/署名/custodyと元accountへのcontinuation linkを、同じcurrent prefixで再検査する。R2は新A/C/join、新予算、floor解放、義務終了、未来AcceptedOriginを作らない。

R2がなければ**Sのlocked-input spend自体を拒否**し、旧UTXO・row・R・nonce/onceを維持する。通常UTXO更新の後でmetadataだけを拒否する実装は不適合である。R2があれば、SのUTXO spend/new output、account debit/current cursor、R2 binding、once、settlement/journalを同checkpointで行う。

#### 9.7.2 terminal pathの正確な保証

actual native ownerが全サービス・P・fraud・duty・capital/reservation/liabilityの終了を確認し、未払適格dueを確定できたとき、現在cursorのRは次の一回全額精算を許す。

- WorkerPaymentが適格なら契約どおり一回払い、不適格が確定していれば成功対価を返金する。
- 適格service feeを実receipt/providerへ払い、署名済みの実burnがあれば一度だけ破棄する。
- 解放可能なfee/capital/surplusを正規payerへ返す。残額はzero、continuationなしとする。

これらの出力と全額保存則はnativeが拘束する。NONE署名を任意recipientへの送金権にしない。feeはforeign inputsが負担し、同じfeeをbond burn・relay charge等へ二重割当しない。

terminal時は新UTXOを再lockしないのでR2を要せず、追加payer署名も要らない。partialを拒否して旧cursorを維持した場合も、その旧cursor用Rが残る。部分精算を一度受理した後は、最初の消費済みRを再利用するのではなく、受理済みR2が次のcurrent Rとなる。

これはPを短縮する保証ではない。P Pending中にworker reserveを返したり、時計だけで未終了dutyをzeroにしたりしない。有限な全branch終了bound、実history/custody、relay供給・inclusionの前提がなければterminal完了を宣言しない。

#### 9.7.3 本RFCの残る条件付きpartial進行

本RFCはP未完／未検証でも適格service feeと安全返金を別判定し、部分返金floorを検査する。本仕様はその権利を消さない。payer署名付きSと事前R2が揃えば、Pを待たずにnativeが認めるpartialを進められる。

ただしpayerが新R2を拒めば、そのpartialを強制できない。本仕様は「全ての安全partialがpayerなしで必ず進む」と主張しない。この限界、partialが条件付きであること、およびfinite terminal fallbackを区別して採択・測定する必要がある。terminalだけを動かして本RFCの全partial要件が完成したと扱わない。

### 9.8. 有限metadata料金・custody owner

新`FundingRelayPolicyV2`と完全なfee/physical profile preimageを、Some-only Params fingerprintとactual fired-rule scopeへ登録する案とする。既存r1 ruleset recipeは変えない。contract scopeと新relay metadata serviceのscopeを区別し、後のfork policyで元契約を解釈し直さない。未定値には公開defaultを与えず、private finite preimageを必須にする。

最低限、global/per-poster/per-proverのpending registrations、同account/nonceの候補数、全continuation数、全stored rows、expected amount宣言上限、raw/decoded/frame bytes、署名数、temporary/retained heap、DB/batch bytes、replay reads/work、登録window、funding window、custody期間、rollback/pruning marginを正かつ有限にする。zero/MAX sentinelを無料・無制限の代用にしない。期限と料金の全加減乗算をcheckedにする。

登録料金はadmission＋署名検査＋全row/material bytes＋必要retained-byte×DAA＋worst replay/retirementの有限quoteを持つ。最初のprofileでは、**Rのactual available native transaction feeからquote額を一度burnし、既存のcollectible feeから差し引く**方式を提案する。元A/Cの将来refundを新予算とみなさず、原資不在のcustody報酬queue・mintを作らない。ordinary tipは残りの実feeだけである。失敗したRの通常feeと、成功したmetadata registration chargeを区別し、同じRの正規idempotent再送から二回目のregistration chargeを作らない。

purpose署名はこのnonrefundable metadata chargeの上限も認証する。Rの普通のfee inputsの所有者は通常scriptで支出を認証する。Fが来なくても、実登録・保管した有限metadataの料金をA/Cの架空債務へ付け替えない。

必要ownerは、actual current-prefix native budget writer、共通fee allocation owner、実RAM lease、実RocksDB hard-quota/WriteBatch、retention/replay/cleanup ownerである。local capacity failureは再試行するlocal resource errorであり、モデル計算の有罪や偽service receiptにしない。on-chainの有限resource bookとlocal physical ticketを区別し、lease IDをR termsへhash backしない。write成功のboolや自己申告capacityを実custodyに昇格しない。

Rが受理されたという事実だけで、FのP/source/fraud/reservation全branchがreadyとはしない。Fは元のactual ownerから最遅終了・資料保持・返金・rollback boundを解決し、Rのcustody残期間が覆うことを確認する。missing actual ownerは別の受付拒否であり、短いP TTLで補わない。

`register_before`と`funding_admission_before`は新R/新bindingの受付期限であり、既にboundとなったcurrent Rのterminal署名を無効にする期限ではない。現在UTXOが残っているのに、この時計だけで署名packageを消してはならない。Fの実capital/physical quoteは、current Rの完全な固定packageをlive accountのnative state slotとして保持する容量も含める。このslotが実際に保有するのは、少なくともexact terms/template/expected entry、payer key、purpose署名byte、relay transaction署名byte、actual accepted bindingと再認証に必要な資料であり、hashや保存済み成功flagだけではない。historical raw journalをpruneした後も、この完全なcurrent bound packageとnativeのaccepted provenanceを実readerが再生できなければならない。current packageは各current cursorに一つ、rollback用旧packageは元契約の有限continuation/retention上限内で予約し、固定expiryでlive署名byteを削除しない。

Rのpending custodyからFの実account slotへの移管、R2をcurrentとした際の旧packageのrollback枠を同writerで実予約する。これは新資金や追加請求権を作らず、同じaccountの署名capと全continuation上限内で行う。raw journalを回収できるのは、必要なaccepted資料が別の実native保持先から完全に再生できる場合か、account終了・rollback/pruning条件を満たした場合だけとする。

期間単価のquoteは実policyの有限義務・relay/refund inclusion windowと明示した供給/inclusion前提を含む。無期限censorshipまで料金で保証したと扱わず、前提外でもclockによる資金解放や署名資料の安全性を破る回収はしない。必要なcurrent state slot・延長保持を実ownerが確保できないprofileはFを受理できない。保存できるというinteger summaryだけをこのownerの代用にしない。

### 9.9. namespace・固定row・DB materialの提案

#### 9.9.1 aux151 append候補

既存0–18を保持し、次の19/20を**中央レビュー対象の候補**とする。番号・実byte長・domain文字列は採択とinventory更新後にのみ確定する。

| kind候補 | key / value案 |
| --- | --- |
| 19 FundingRelayAuthorization | key: exact funding outpoint＋purpose tag。value: full fixed terms、native payer key、一つのpurpose signature、actual R origin/material、metadata quote/charge参照 |
| 20 FundingRelayCustody | 同じkey。value: terms ID、一つのrelay transaction signature、actual custody/reservation/material参照、保持clock、optional exact binding（actual F/S origin、元account/contract/current predecessor） |

key案は既存Borsh `(2u16,1u16,kind)` 5 bytes＋NativeOutpoint 68＋purpose 1＝74 bytesであり、142-byte上限内にある。AssignmentIdとprover bondはvalueへ認証保存する。同じoutpointを異なるpurpose/Assignmentに二重登録・二重bindしない。有限三purposeの同outpoint範囲を照合し、hashだけの曖昧な重複検査にしない。

各rowは一つの署名と固定fieldのみとし、16,384-byte ceiling以下のexactサイズを試験する。full metadataをrow内の無制限Vecへ詰めない。19/20の両方、署名、full material、custody、fee、pending countが一checkpointに属する。20のbindingはactual F/S受付でのみ作り、R受付時に未来originを埋めない。

全pending countはrooted rowsから再現可能であり、有限global上限に収まるindex/range scan又は明示したtyped countersで管理する。counter追加が必要なら、そのkey/codec/undo/snapshotも中央schemaとして確定し、local singletonに依存しない。F成功時のpending→bound、unused R expiry、reorgの各増減を同じwriterで行う。

#### 9.9.2 raw DB material

新DB material候補は`DatabaseStorePrefixes::PalwStateV2Tip`内の**suffix 0x4c（未割当提案）**＋64-byte relay material digestとする。2026-11-03読取時点の対象storeでは0x49がnative identity、0x4aがCase journal、0x4bがAssignment journalであり、同じ対象内の0x4c使用は検索で見つからなかった。これは全repository・全pending branchの中央collision review完了を意味しない。global DB prefixを無断追加しない。

raw materialは正規body14の全byteとactual accepted framingを保存する。digest一致だけでorigin又はcustodyを認めない。現在canonical row19/20から選ぶprivate readerが、先に有限read/bytes/work/heap枠を確保し、実accepted R bytes、両署名、元scope、target、fee、custody、F/S bindingを再生する。world-wide local spent fileやpeer提供rowで成功を補わない。

DB stagingはordinary UTXO/PALW state、metadata burn、rows/counters、native reservationと同じWriteBatchに束ねる。orphan raw bytesは残ってよいが、row/root/lineageなしには権威を持たない。memory上のDB接続を保持したrestartだけでなく、全接続を閉じて再openする試験を要する。

#### 9.9.3 collision inventoryと互換性

| namespace | 現状／本仕様 |
| --- | --- |
| global lifecycle object | 176を維持。別global tagは提案しない |
| object176 local operation | 0–13を保持し14を提案。未採択時はpredecode前に拒否 |
| aux151 local key/value kind | 0–18を保持し19/20を提案 |
| aux140 settlement once | 既存legの意味を借りず、採択済みAssignment支払purposeとのexact debit/onceへ接続。Rだけで支払markerを作らない |
| DB material suffix | 同storeの0x49/0x4a/0x4bを保持し0x4cを提案。中央登録は未了 |
| domains/署名context | R terms/material/purpose/templateの新domainをinventoryで衝突確認。既存transaction contextは不変 |

None時は全shipped fingerprint/root/wireを不変にする。`CooperativeV1 / RequiresR_V2`というfunding relay profileの選択は、**元のAssignment intakeのactual fired scopeで一度だけ決定し、その認証済みpolicy/materialへ固定する**。後続C、全prover join、continuation、取消・期限切れ・精算まで、この元の選択を維持する。Fが後日のcurrent Paramsで処理されることを理由に、profileを再選択しない。元の選択を再生する資料がなければUnavailableであり、都合よくどちらかへfallbackしない。

後日のrelay activationは、既にacceptedの旧Assignment、その将来C/join、既存funding accountへRを遡及必須化しない。旧契約は元のcooperative profileとそのpayer署名依存・実liveness限界を維持する。旧署名・元r1 consentをrelay登録認証として再解釈せず、新保証付きへ自動昇格しない。逆に、元からRequiresRのAssignmentについても、後日のforkで原資料・codec・継続サービスを消したりRequiresRを外したりしない。満期・責任終了まで元profileを実行できるよう保持し、変更するなら別の明示的なfork移行規則を要する。

**既存accountの任意opt-in／移行は本仕様で定義・承認しない。** 既存契約への必要な合意、追加費用、資源、署名、支払権・返金権の保存を含め、別の設計判断とする。公開presetはNoneを維持する方針であり、公開網のlive Assignmentが存在しないと本節が実査・確認したわけではない。実装・移行・公開判断前に全presetと対象網のactual stateを確認する。live契約不在という未確認の仮定を、遡及適用の安全性の根拠にしてはならない。

### 9.10. reorg・復旧・pruning

- Rのcanonical受理をundoしたlineageでは、そのRを必要とするF/Sのnative受付も再検査される。raw bytesが残ってもRを復活させない。
- F又はSのundoは、funding cursor、R binding、pending count、資金/役割/nonce/Case、actual payment/once、journal、UTXOを一緒に戻す。R2がorphan又は未bindへ戻ったことから新予算を作らない。
- 現在cursorのRと、rollbackで必要になる過去cursorのRを、actual policyの必要保持期間まで残す。old Rを新R2で上書きしない。
- snapshot/IBDはcurrent accounts、row19/20、必要pending/binding証跡、native counts/reservations、raw materialを同じrootに照合する。通常sig検証、actual accepted ancestry、元のscope、feeとcustodyの再生を省略しない。
- unused Rはexclusive funding expiry後、実custody契約終了とrollback/pruning条件を満たしてから回収できる。bound Rはaccountがdrainedでもrollback horizonを確認する。時計だけで既存義務・coin lockを解除しない。
- 大きなraw資料を回収した後も、同じF/accountの再受付を防ぐ既存nonce/Use/join/once又は必要な小さなbinding証跡を維持する。retained registry数・DB quotaも有限にし、永久metadata増殖を無料にしない。
- 必要資料が失われた場合は`Unavailable`で停止し、署名・custody・支払済み状態を捏造しない。正規復旧なしにlocked valueを解放しない。実local I/Oはchain undoで未実行には戻らない。


### 9.12. 必須試験と有効化条件

必須試験は次を含む。

1. 固定77-byte payload、0x82、key/script/amount/outpoint/sequence/sigop/version/locktime/subnet/gas/contextの一致と一field変更拒否。追加fee入力・許可出力変更では署名が維持され、nativeは不適格recipient/fee流用を拒否すること。
2. F→署名→Rの構築で循環しないこと。FがR hashを参照しないこと。F payload/outputs変更で旧Rが無効になり、signature script変更ではtxidが変わらなくても通常Full検証を再実行すること。
3. R単体ではA/Case/funding/lock/role/nonce/credit/WorkerPaymentが増えないこと。料金・caps・expiry・actual bond/purpose/metadata custody不足を拒否し、同feeを二重burnしないこと。
4. actual F出力/現在UTXO/元payer consent/r1 scope/refund/capsがRの宣言と異なる場合の拒否。別Assignment/purpose/outpoint、孤立material、別lineage、R期限・custody不足の拒否。
5. A/C/op13全purposeでの先行R照合と一checkpoint rollback。Rを消すreorg、F/Sだけを消すreorg、競合nonce/Use/join、同outpoint複数purpose・別terms再送の拒否。
6. partial Sの前にR2が必要であること。R2なしでは旧UTXO/lock/Rが不変、R2ありでは正しいnew cursorとbindingへ一回だけ進むこと。Sのfee inputs/outputs変更で別txidになれば対応しないR2を拒否すること。
7. P PendingでWorkerPaymentを拒否しつつ、適格service/refundのpartialが正しいfloorとR2で進むこと。R2非協力時はpartialを強制できない限界を記録し、有限なactual終了後のterminal pathでは追加payer署名なしに全due/返金が進むこと。
8. P NoPopulation/Expired/NotCertified、pending-fraud、prover放棄、全live/retired join、未終了retention/dutyを含む実branch。P/fraud finite owner欠落を短縮TTLやsaved flagで通さないこと。
9. DB全接続を閉じたrestart、reorg/rollback、snapshot/pruned IBD、missing/corrupt/truncated/duplicate資料、hard-quota不足、NativeHeap/Wireの全field登録。raw orphanのみから成功しないこと。
10. 全shipped None fingerprint/root/wire、旧r1/TFO1/0–13/0–18互換性、元Assignment intake profileの固定（後日activationをまたぐ旧A→C/joinにRを遡及必須化しないこと）、historical raw pruning後のlive署名package保持、WorkerPayment→Reward誤用拒否、元account予算とonceの不変。Miner/Panel/Node/custody/relay別のCPU/RAM/通信/保持/復旧とnative全block/global資源を実測すること。

## 10. 後発検証者への供給

claim受付時に、静的artifactとclaim固有資料それぞれのroot・revision・範囲、供給者の署名義務、要求・応答期限、費用と最大bytesを確定する。後発者は選択履歴から要求を出し、第三者も同じrootへの正しいopeningを供給できる。応答の包含を確認してから再実行する。計算の矛盾と、署名済み供給義務の客観的不履行は個別に裁定する。帰責不能の取得失敗では成功を保留し、元期限で未検証終端とする。
