> **採択記録 — 2026-11-02（Asia/Tokyo）、ユーザー:** 「両案を採択し、実装を進める」。
> RFC-0045と[対の採択文書](../adr/0213-positive-court-canonical-assignment.md)を同時採択し、Canonical TIR Assignmentのjob/worker権限、事前purpose資金・予約、Resultの一消費binding、poster-funded成功対価を限定実装する。
> 採択は実装・検証の完了、公開パラメーター又は本番有効化の承認ではない。RFC-0044の全体目標・全consumer・primary lightweight miningは引き続き実装対象として残る。
> 下記の2026-10-31案の「未採択」「提案」「採択する場合」は採択前の履歴表現であり、今回採択した規範の出所として保持する。wire/domain/rowの登録は既存namespaceとの照合、未定値は有限policy、公開網の有効化は別承認を必要とする。

# RFC-0045 — Canonical TIR Assignmentを起点とするPositive Court native経路

- 状態: **Accepted — 2026-11-02、ユーザーの明示採択。実装・検証未完了、公開有効化は未承認。**
- 日付: 2026-10-31。
- 対の判断: [ADR-0213](../adr/0213-positive-court-canonical-assignment.md)。
- 基礎: [RFC-0044](../rfc/0044-positive-court-full-coverage.md)、[ADR-0212](../adr/0212-positive-court-without-model-prepossession.md)、[共通native契約](../design/palw/positive-court/NATIVE-CONTRACT-R1.md)、[原則](../PRINCIPLES.md)。
- 正式番号: RFC-0045 / ADR-0213。wire/domain/rowの中央登録照合と実装・検証は継続し、activation heightは未割当。

## 1. 問題、必要な追加決定、今回の到達範囲

RFC44はMinerの結果commitとPanelの証拠commitを分離する。一方、現native実装には、軽量なr1 Resultの期待job・workerを独立に固定するTIR assignment正本がない。一般のjob envelopeはsupervisor指定の値をコピーするだけであり、peerのResultから期待値を作ることも認められない。

primary own-template headerを最初の入口にすると、Resultを含むtransactionとそのMerkle rootからjobを導出する循環が生じる。Panel rootsを含む新header方式も、header identity、抽選、origin時点の責任・予約という別の追加契約を必要とする。旧PAV2/PAV3を軽量Resultへ読み替えて解決しない。

本案は、**既存native TIR登録 → 実受理された新versionのCanonicalTirAssignment → 後続body0 Result → Panel → Node → P → poster-funded settlement**を、RFC44の最初のnative TIR経路として提案する。新assignmentは計算結果を含まない。新Resultはprimary L1 workを代替しない。PAV2/PAV3、own-template job、class ticket、L1 tag、PoW、既存発行規則は変更しない。

採択しても、これはRFC44の全consumer接続やprimary lightweight mining replacementの完了ではない。対応外の用途は拒否したままとする。登録モデルの1.0/0.3、年次指定、モデル単位の審査不採用、claim単位Gate Cを維持する。計算の正しさを暗号学的に証明する方式を導入しない。

以下のMUST/MUST NOTは、この案が正式採択された場合の新versionの規則である。

## 2. native正本と権限の境界

assignmentの正本は、actual native transaction validatorと現在のaccepted-prefix foldが同じstate世代で受理した新rowだけとする。署名・DTO・hash一致・mempool表示・local journalは受理を代替しない。

最初のprivate profileは **poster=payer、worker=data-source、prover** の3役に限定する。3役とも実native bondであり、same accepted-prefixの登録key・仕事受付資格をnativeが検査する。posterは仕事と支払条件、workerは推論と有限資料供給、proverは再計算・record/evidence生成の有限契約へ明示同意する。workerに全証拠生成を移さない。兼任を許す場合もrole別署名と会計を省略しない。別payer・別providerへの委任は初期profileでは拒否し、後の別profile決定とする。無署名の第三者に仕事・債務を課さない。

初期modeは `CanonicalTirAssignmentV1` の一種類とし、任意prompt、任意seed、任意decode長、動的EOS、opaque外部処理を追加しない。TIR登録そのものは既存の登録規則で行い、assignment受付をモデル品質・公開性・保持者数の審査にしない。claimに必要な資料供給は、そのclaimの有限な署名契約として扱う。

`AssignmentPolicyV2`の正規preimageには、version/revision、mode、Positive policy ID、assignment/Case fee schedule ID、physical service profile ID、frame/program/layout caps、global/per-poster/per-worker/per-prover/per-class live caps、全role候補数・prover join数上限、全履歴/保持/復旧caps、accept/result/build/check/fraud-tail/refund各windowを必須fieldとして提案する。countはu32、bytes/work/金額/DAAはu64、IDはDigestとし、全演算をcheckedにする。実branch schedule・金額・単価は対応preimageから解決する。初期の代替提出を可能にする候補/joins上限は正かつ有限とし、ゼロ・最大整数・未定義IDを「無制限」「無料」「即時成功」の代用にしない。具体値は未採択であり、このdraftで公開値を与えない。

## 3. AssignmentTermsと独立したjob導出

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

## 4. 同意attachment、実原資、事前予約

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

最初のproverは実native role契約で固定するが、永久の独占席ではない。RFC44に従い、代替提出者は同じAssignment/Caseの期限・全試行予算のまま、新purpose同意・実bond・必要fee/capital/容量をnativeで取得できるbounded join/交代を提案する。元proverのACKや裁量承認を必須にしない。joinを未実装のまま自由代替が完成したと称さず、単なる署名差替えで新しい予算を与えない。任意peerの未認証資料はrole取得・attempt消費・有罪の根拠にならない。

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

## 5. 一writerによる受付、body0、状態遷移

`AcceptAssignment`は、actual current-prefix/state世代を独占する既存native budget writerが次をall-or-nothingで行う。

1. fence/version、frame/metadata caps、全署名、network/rule/ancestor scopeを検査する。
2. class登録metadata、worker/poster bond key・現在の仕事受付資格、nonce、clockを検査する。旧model審査を追加しない。
3. 正規job・source/catalog・finite quoteを導出し、全global/per-poster/per-worker/per-prover/per-class live枠と保持・復旧枠を予約する。
4. 実原資A、coin lock、worker/sourceとproverの実role契約、actual capacity leaseを同じprefixで確保する。
5. assignment/nonce index/派生資料参照/資金/予約/role rowを同じnative delta・journal・UTXO checkpointへ書く。

結果がないこの時点ではCaseRowやResultRowを作らず、ゼロResultIdも保存しない。accepted assignmentは `Open` となる。Minerの通常推論は行えるが、初期profileの合意data/proverサービスはまだ開始しない。off-chainの任意作業をaccepted data receipt、課金可能なwitnessサービス又は未採択assignmentの受理へ昇格しない。

Node側の全branch service/capacityは、policyの全合法branchと全Result受付時刻を覆う有限envelopeとして、実native ownerが予約する。prover build・Node check・fraud/liability・refundだけでなく、**Result受理をcommit起点とする既存actual Gate C policy**のanchor delay、draw queue/epoch容量、certify_by、attestation、decision/prosecution/pending-fraud条件とNoPopulation/Expired等の不成立branchを含める。P参加者のdata取得・保持・支払・安全返金の最遅時刻までを既存policyの最悪boundから導出し、`result_before + worst_existing_end_to_end_bound <= reserve_until` をchecked演算で必要とする。新しいP policyや短縮TTLで既存anchor/draw/decisionを省略しない。既存policyの全branchに有限な最悪boundを解決できなければ拒否する。

単に将来capacityや自己申告のP所要日数を整数rowへ書くだけでは足りない。op9はこの同じpolicy、残余期間、actual lease、P用data供給/保持cutoffを再検査し、reserved envelopeからCaseへ原子的に移す。actor local実行資源とnative Node全体資源を区別し、実供給・inclusionの仮定を無条件の進行保証としない。必要なowner/preimageが無ければ受付拒否する。

後続body0は、signed literal r1 Resultと既存Case funding bundleを保持し、**追加のaccepted assignment参照**を別operationで結合する。body0の旧意味を変更しない。新Resultの期待値をそのpayloadからコピーしてはならない。nativeが同じorigin lineageのaccepted assignmentを読み、§3の全期待値とworker署名を検査する。

具体的にはop9がAssignmentIdと既存body0 **payload**を一つの新versioned受付として原子的に検査する。op0で先にCaseを受理し後からassignmentを貼る二段階処理ではない。assignment-modeのResultを参照のない旧op0へ迂回させず、peerが申告したjob digestだけで割当済みとみなさない。

op9では **C.payer == Assignment.payer == native poster principal** とrefund_payloadの同一性をMUST検査する。worker recipient、exact AssignmentId/CaseId/ResultId、source/prover scope、signed fee/capital/total caps、expiry/deadlineと受理済みassignment条件の対応もMUST検査する。termsのA専用capをCへの支払許可として使わず、op8 unsigned bundleで固定した将来C用identity/上限と、output確定後の実r1 payer consentをともに満たす必要がある。違うpayer/refund先への差替えや片方だけのcap照合では受付しない。

最初のCase受付で `Open → Consumed(CaseId, ResultId)` とし、Case全試行予算・実C原資・全必須関係・有限source duties・依存empty資料・P用正本を同じcheckpointで作る。ここがdata/proverサービス及びPのcommit clockの開始点であり、Assignment受理時刻をPの先行commitに使わない。消費はResultのoutput/fee/Panel/relay/attempt/Case labelを変えても一度だけである。同じResultの再送も、全unsigned bundle/attachmentsの一致と現在の署名/原資認可を再確認して初めて元receipt又は無効果として扱い、新予約・新料金・新clockを生まない。異なるbundleをidempotent成功にせず、別Resultへの再束縛は禁止する。

assignmentのscopeは受理時の独立ancestorに固定する。後続bodyの現在tipへ置き換えない。「全branch一消費」は、**同じcurrent canonical executed view・同じorigin lineage内**で全Result/Caseラベルを横断して一消費という意味である。排他的forkの一時状態を世界共通のlocal spent fileで固定しない。reorgは消費・Case・資金・receipt・予約・支払を同時undoする。新lineageにoriginがなければ再利用不可とする。端末の実I/O・実計算済み時間はchain undoの対象ではなく、再実行やlocal journalは合意上のreceiptを捏造しない。

assignmentの計算/署名policyは受理時に固定し、後続forkの現在policyへ差し替えない。既存assignmentの期限・精算まで旧preimage/codecを保つ。将来forkで受付継続を止めたり既存契約を移行したりする場合は、別の明示的fork移行規則を要する。新Caseのactual原資・prefix・残余capacityの現在検査は、この不変な計算ruleの再解釈とは区別する。

## 6. 期限、取消、放棄、返金

Open assignmentの取消は、consumedでないことと、既に受理したsource/service duty・資本責任が消えないことをnativeで確認する。payerがworker/proverの実施済み契約対価を一方的に消せないよう、早期取消は**poster/payer・worker/data-source・proverの全3役**の署名、又は全3役がpolicy/bundleで予め署名した取消条件を必要とする。

初期op10はposter/payerとworkerの2つの新取消署名を用い、proverがop8 bundleで予め同意した取消条件（未提供業務の停止、適格receiptの対価・残責任の保存）をnativeで検査する方式に限定する。proverの既発生権利を変更する3者再交渉を、2署名の取消へ紛れ込ませない。条件外の取消は拒否し、通常の有限期限と残責任の精算へ進める。

op13で参加した全live/retired proverも同じ取消条件へ署名済みであることを確認し、その全既発生債権・capital・未完dutyをactual native history/rowsから検査する。initial同意だけを読んでjoin契約を消してはならない。initial A段のdata/prover実サービスは禁止であっても、join資本・固定metadata保持等の適格な別purpose権利を消してよいとはしない。

`result_before_daa`以降の新Resultは拒否する。Openの期限切れは `ExpiredUnconsumed` であり、推論成功報酬なし。Aの実提供サービスだけを正規receiptから精算し、未提供fee・worker成功対価・責任終了済みcapital・surplusを正規返金先へ返す。時計だけで未終了dutyや資本lockを解除しない。

Consumed後の取消はない。CaseがPositivePassed、ClaimRefuted、EndedUnverified等に進んでもassignmentをOpenへ戻さず、別Result/別Caseで予算を買い直さない。Case終了と、Miner/source応答完了・Panel完了・Node完了・残責任終了を別記録とする。Panelの放棄や偽証拠だけでMinerを有罪にしない。未検証終了ではAのworker成功対価を払わず、Cの実サービスと未消費返金を別精算する。原資・署名契約なしの罰金を作らない。

Courtの計算一致状態とPの認証状態を別に保持する。PositivePassedでもPがPending/NoPopulation/Expired/NotCertified等ならWorkerPaymentを認めず、計算一致を取り消したりMiner fraudとみなしたりしない。既存P policyが認証不成立を確定し成功対価の可能性が消えた場合、未払適格serviceとreservation/liability floorを残した安全な返金を行う。P不成立だけを理由に永久lockせず、pending-fraud等の残責任は署名契約/実policyに従って別に扱う。

## 7. 提案namespace、row、署名、保存形式

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
| 18 AssignmentProverFunding | AssignmentId＋prover native bond → op13参加者自身の署名済みjoin consent・別の実outpoint・fee/capital/surplus・未払due/責任floor・予約/精算参照 |

2026-11-02実装schema確定: kind18は採択済みop13が要求する参加者自身の原資を、poster専用のkind13へ混在させず保存する従属accountである。0–17の番号・wire・意味を変更せず、新しいResult mode/IDを導入しない。keyはBorsh prefix 5＋AssignmentId 64＋native bond 68＝137 bytes。join consent digestを加えると201 bytesとなり既定142-byte上限を破るためkeyへ加えず、literal consentをvalueへ保存する。同一assignment・同一bondには一つのfunded joinだけを許し、consentの差替えや再入金でCaseの全体予算・期限・onceをresetしない。valueは一つのpayer key/signatureだけを持つ固定rowであり、元のkind13 account、後続Caseのkind7 accountとは別purpose/別実coin lockである。 kind14 settlementのvalueには`Poster / Prover(native bond)`の固定account識別子を含め、対応する原資だけの残額・due・責任floorへ適用する。keyの142-byte上限を維持するためaccountはkeyへ追加せず、ordinalは同一AssignmentId・leg・partyについて全accountを横断して一意とする。異なる原資accountで同じkeyを再利用して精算rowを上書きしない。


可変layout/source集合/履歴はbounded carrierとaccepted materialから解決する。固定rowの既存codec ceilingに収まるfield構成と署名保存を、中央schemaレビューで確定する。key/value version、順序、重複、未知kind、孤立Use、nonce不整合、rowリンク、署名、clock、履歴・残額を、bounded ingressとrestoreの両方で検査する。実resource ownerが全temporary/retained/replay費用を負い、DTOのbooleanや整数台帳をnative leaseへ昇格しない。

3つのkey/signatureを一つの16,384-byte rowへ詰め込まない。AssignmentRoleは**一役一row**で、その役のkey/signatureとunsigned bundleのaccepted material参照を持つ。全role rowが同じbundle、terms、派生quote/catalog、actual originへ結び付くことをrestoreでも検査する。主Assignment rowは3署名の無検査hash summaryではなく、これらの正規rowを必須linkとして解決する。

新op8には専用 `MAX_ROLE_SIGNATURES=3` と、3組のnative key/signature、3役宣言、layout/参照/metadataを含む新codec ceilingを提案する。keyを搬送する方式なら3組だけで21,657 bytes（3×(2,592+4,627)）あり、outer framing等を別加算する。実fixed field長・bounded attachments・temporary/retained heap・検査work・physical massを保守的にquoteし、private finite policyのframe capへ入らなければ拒否する。既存0..7を3署名対応として再decodeしない。fence未満は新operationを署名/資源/資金処理前に落とし、公開presetを開かない。

新ID/domainの提案対象は、AssignmentTerms、unsigned admission bundle、assignment-purpose funding/source/prover宣言、prover join、取消message、資金/予約/精算purposeである。初期3役のbundle署名contextは別にし、全署名がnetwork/rules/AssignmentId/役割を固定する。文字列・enum値は中央domain inventoryと全既存namespaceを照合してから登録する。r1 Result/Case/Witness/P subjectの既存ID/domainを変更しない。旧settlement legのsubject意味を変えず、必要legをappendして既存once engineへ接続する。

### 7.1 Result前のjournalと実history reader

Result前に既存CaseJournalを使ってはならない。既存CaseJournalはCaseId/ResultIdを必須とするため、ゼロIDや架空Resultで流用しない。新 `AssignmentJournalV2` と上記head/frameはassignment-purposeの別namespace/domain追加案である。domain提案名は `misaka/positive/assignment-journal-frame/v2/r1`。header Notice・旧Kernel/Native39のhistoryを読み替えない。

frameの固定metadataはversion/revision、AssignmentId、ordinal:u64、prev_frame:Digest、event_kind:u8、actual AcceptedOrigin、payload_digest:Digest、payload_bytes:u32とする。payloadはbounded canonical typed eventであり、Open（full terms/bundle/3役認証/実funding・予約記録）、JoinProver、A専用purposeのadmission/固定metadata receipt、Cancel、Expire、BindResult、Settlementを区別する。preCase data/prover receiptは認めない。BindResultだけが実CaseId/ResultIdを持ち、native受付済みbody0へのexact参照を含む。frame digestはこの全metadataと正規payload digestにbindし、ordinal=0はprev=zero、それ以外は直前digestと連続ordinalを必須とする。新headの前提が現在headに一致しなければ、stale/ABAとして拒否する。

native writerはhead/frame/material retention、nonce、role、原資、lease、後続Case linkを同じcheckpointで進める。actual accepted eventをprivate constructorのreaderから読み、current prefix/lineageを再検査する。任意trait実装、保存row summary、policy+recordだけでhistory再生成功を宣言しない。payloadの実byteと署名、元のclock、nonce遷移、目的別資金・予約・全receiptの連鎖を、先に有限bytes/work/heap上限を予約したreplayで検査する。CaseJournalの最初の新境界にはAssignmentId・BindResult frame digest・accepted originをexact参照として結び、両journalを取り違えない。

前段materialの保持費用・全frames・復旧replayはassignment quoteへ含め、必要なCase/残責任/返金/rollback期間を覆う。historyがpruning等で失われ、適格なactual native資料から復元できなければ、source/session/admission/creditは `Unavailable/未検証` とする。local recordから補った成功を作らない。そのことだけでMiner/proverの有罪にせず、存在するcoin lockや責任を勝手に解除しない。permanent nonce counterと既存once消費証跡は、payload回収とは別に保存する。

## 8. Court、P、用途別entitlement、実送金

Panelが全再計算とrecord/evidenceを構築し、Nodeが全必須関係を独立検査する。資料・全Panel合計予算・期限・帰責はRFC44を維持する。Pのactual selected-verifier Certifiedは別の必要条件であり、旧S/K/V subjectやPublicCheckCertifiedを借りない。

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

## 9. 凍結例外、代替、検証、有効化

本案が求める限定例外は、新job mode/authority、terms ID起点のjob導出、新worker同意・assignment-purpose実予約、新規local operations/rows/purposes、後続r1 Resultとの一消費binding、poster-funded成功対価のnative精算である。既存の式・登録・claim・PAV2/PAV3・原資・発行・モデル階層の意味は変更しない。これらの新合意規則は既存の未割当箇所をコードで補っただけとは扱わず、**新RFC＋ADRの明示採択**を必要とする。

代替は、primary lightweight headerの契約を先に確定する案、又は既存KernelJobのexact promptを維持して新Resultを結ぶ案である。後者にもnative TIR classとの正当なbridgeが必要で、ID同一視では解決しない。現在の拒否状態を保つ選択も残る。own-template方針を黙って差し替えない。

必須試験は、r1全field/独立期待jobと実LLM classの元max context/CUゼロ維持、別worker/key/ancestor/rules/class/layout/input拒否、output依存funding cycle不在、同nonce別terms/同terms別bundle・全Result/Case再束縛拒否、actual UTXO/lock/accepted-prefix変更、payer/refund/caps照合、A必要額/部分返金floor、preCase data/proverサービス拒否、A/C二重払い、全原子的rollback、全joined proverの取消/期限/未完duty、Panel/P不成立のMiner責任転嫁防止、実Gate C全時刻/NoPopulation/Expiredとfinite bound欠落拒否、P未完/未検証の計算信用・WorkerPayment拒否と適格service/refundの進行、WorkerPayment→Reward誤用拒否、用途別B/F/Rゼロ、reorg/restart/pruning/IBDの同一結果を含む。

未定値は有限なprivate policy/scheduleの正規preimageで必須化し、実LLMでMiner/Panel/Node別のCPU/RAM/通信/保持/復旧とnative全case/block/epoch資源を測る。測定・独立review・実native一writer試験・全consumer auditが未完ならpublic activationは拒否する。公開Paramsは全てNoneのままとし、None時の既存fingerprint/root/wireを維持する。設計採択、実装、private試験、実測、公開網別有効化は、それぞれ別の判断・承認とする。
