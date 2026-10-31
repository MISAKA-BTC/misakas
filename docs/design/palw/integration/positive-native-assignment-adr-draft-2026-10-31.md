# ADR番号未割当 — 最初のPositive native TIR経路を合意Assignmentとposter原資で分離する案

- 状態: **未採択・無効のintegration draft**。
- 日付: 2026-10-31。
- 対の仕様案: [RFC draft](positive-native-assignment-rfc-draft-2026-10-31.md)。
- 前提: [RFC-0044](../../../rfc/0044-positive-court-full-coverage.md)、[ADR-0212](../../../adr/0212-positive-court-without-model-prepossession.md)、[原則](../../../PRINCIPLES.md)。
- 新番号、wire/domain/row登録、実装完了、公開activationは承認されていない。この文書を実行指示に読み替えない。

## 1. 判断を求める問題

RFC44の役割分離をactual native TIR経路へ接続するには、Minerの結果とは独立したjob・input・workerの正本が要る。現generic envelopeはその値をコピーする形式であり、native job authorityではない。peerのResultやPanel recordから期待値を作ると、検査する問いを提出者自身に選ばせる。

primary own-template headerを最初の入口とする案では、Result transactionを含むMerkle rootからそのResultのjobを導出する循環がある。別の軽量post-PoW Noticeは循環を避け得るが、全literal Result/funding/Panel参照のidentity、旧抽選入力との一致、origin時点の経済責任・予約を追加決定しなければならない。旧PAV2/PAV3の意味をそのまま使える状態ではない。

既存kernelには、結果を含まないposted jobとposter-funded escrow、別途同意したworkerを固定する契約の実装がある。Native39にも、結果前の同意・実原資・quota/予約の段階がある。ただし両経路のclass/job ID、目的、証拠義務はnative TIR/r1 Resultと異なり、同じ長さのhashという理由で流用できない。既存KernelJobのpromptをcanonical TIR promptへ差し替えることも既存意味の変更である。

したがって、既存凍結仕様の内部補完だけでは次の新合意契約を埋められない。本ADR/RFCを新しい限定例外として採択するかの判断を求める。

## 2. 採択する場合の決定

### D1 — 適用範囲を分離する

RFC44の最初のactual native TIR経路を、**既存TIR登録→CanonicalTirAssignment→後続body0 Result→Panel→Node→P→poster-funded settlement**とする。新Resultはprimary L1 workを代替しない。PAV2/PAV3、own-template job、class ticket、L1tag、PoW、既存発行規則を変更しない。

assignment自身の発行・block/Final weight entitlementはB=F=R=0とする。実資金のhold・送金、service・capital・UTXO・queue容量までゼロと扱わない。これは全RFC44 consumer接続やprimary lightweight mining replacementの完了宣言ではない。

### D2 — 結果を含まないtermsを新authorityにする

最初のprivate profileはposter=payer、worker=data-source、proverの実native bondsとbounded role契約を固定する。3役がterms確定後の同じunsigned admission bundleへrole別の新contextでco-signし、native登録keyとactual prefixで検査する。別payer/providerは初期profileの対象外とする。無署名の割当を作らず、class/shape/max contextは登録のままとする。

AssignmentIdは全termsを新domainでhashし、結果・署名・source/quoteの派生ID・同意ID・自分のorigin/hash・lease IDを含めない。初期modeは `CanonicalTirAssignmentV1` のみ。AssignmentIdを新modeのanchorにして既存canonical TIR shape/prompt/seed関数を使い、full canonical pairを実行する。primary miningのprefill-draw短縮を黙って適用しない。これは新job authorityであり、旧own-template anchorの再利用ではない。

完全なassignment policyをSome-only Params/fork fingerprintと実既発火rule scopeへ結ぶことを実装条件とする。r1の既存ruleset recipeを再定義せず、外部policy IDだけで認証済みと扱わない。既存assignmentは受理時の計算policyを期限・精算まで保持し、将来forkでの停止/移行は別の明示規則を必要とする。

proverは再計算・record/evidence・分割提出に同意し、workerは通常推論と有限data応答だけを負う。workerがsole holderなら、全必要weightとopening/IO/再送/保持/復旧の最悪全量quoteを必須とする。proverが多分資料を持つという理由で受付の最悪予算を削らない。prover permissionは実role consentに由来し、Pの事後drawとは別である。

最初のprover固定を永久独占にしない。RFC44の代替提出に従い、新proverは同じAssignment/Caseの全予算・期限を維持したまま新purpose同意と実資金/予約でbounded joinできる案とする。元proverのACKや許可で交代を妨げない。未実装のjoinを完成済みと扱わず、交代で試行・資料・fee上限をresetしない。

joinでtermsのinitial proverを変えず、同じAssignmentIdに追加role rowを作る。元poster同意の全候補合計capを超えて課金せず、初期joinの追加原資は新prover本人のjoin-purpose認証・実debitに限定し、第三者payerやassignment payerの差替えを追加しない。主rowのsummaryだけでrole permissionを認めない。

job/context/input/seed/countsとMinerは、accepted assignmentの実native正本から再導出する。r1 Resultから期待値をコピーしない。r1 Result.tokenizerとexecution_rulesはそれぞれ導出context.tokenizer_id / cu_ruleset_idであり、canonical yardstickの両ゼロを保つ。execution_rulesをscope.rulesetで置換しない。selectorは既存共通greedy ID、temperature_q24は0である。registered tokenizerは別metadataとしてclass IDで認証し、r1 fieldを登録値へ差し替えない。対のRFCの全field表どおり、version/revisionは既存r1の2/1、outputはworkerの署名主張を後続Node検査で照合し、service_contractは実C用literal r1 Service.id()とする。

### D3 — 事前assignment原資と後続Case原資を分ける

事前A勘定は新assignment-purpose同意を使う。actual UTXO/coin lock、source consent、有限な資源・保持・復旧枠を一writerが同時予約する。output依存のr1 FundingConsentを事前Aへ無理に適用しない。

初期profileのA段では実data op3/4、witness生成/提出の合意サービス実行とその支払を禁止する。Aの提供済みサービスはassignment admission/固定metadata retention等の専用purposeだけとする。workerの通常推論をsource追加処理として請求しない。data/prover同意は将来C用であり、実要求・全Panel合計source usage・課金はCase受理後に初めて開始する。preCase unit/counter/feeをCでresetする未定義bridgeを作らず、将来必要なら別の明示仕様を要する。

事前bundleはnative導出quote/catalogとunsigned funding/source/prover宣言を含み、3役の署名を自己参照しない。Nodeの全branch・全受付時刻を覆う有限capacity envelopeを実native ownerが予約し、body0で残余window/leaseを再検査してCaseへ原子的に移す。必要なpolicy preimage・実供給・ownerが未定義なら拒否する。空のrole slot、実予約のない整数row、無限/ゼロ料金の仮置きで正本を埋めない。

このenvelopeは、Result admissionをcommit起点とする既存actual Gate C policyのanchor delay、draw queue/epoch容量、certify_by、attestation、decision/prosecution/pending-fraud条件、NoPopulation/Expired等のbranchと、資料供給・保持・支払・安全返金の最遅時刻を含む。新P policyや短縮TTLで代用せず、既存policyの有限な最悪boundを解決できなければ拒否する。

後続C勘定は、完成したliteral Result/Service/Consentを既存r1の二field-zero projectionで束縛する。追加fieldをhashから省略しない。A同意はCの支払許可ではなく、同じoutpoint/service/obligationを両勘定で二重計上しない。成功対価はAのposter原資から一回、Court等のサービスは各勘定の実receiptから一回だけ払う。

op8 bundleはoutputを含まないFutureCaseFundingBoundsも固定し、AssignmentId/payer/refund/worker、C用fee/capital/total capと受付/供給期限を明示する。Cの実r1同意でこれを追加認証する。P検査者のdata取得・保持も同じCase全体予算へ含め、別用途名でunit counterやMiner上限をresetしない。

実A初期額は `worker_reward + required_assignment_fee + required_assignment_capital` 以上かつ署名total cap以下とし、全てchecked演算で検査する。部分返金/支払後の実残額は、未払成功対価・未払適格service due・reservation/liability floorを満たす必要がある。op9ではC.payer==Assignment.payer/poster、refund_payload、worker recipient、signed scope/identity/caps/expiryの対応と、新しい実r1 payer consentをMUST検査する。A専用capだけでCを認可しない。

数値未定のfee/capital/schedule/profile/supplyはprivate finite policyの実preimageを必須とし、未定義なら拒否する。既存のtyped-missing resolverを成功へ変更したと称さない。旧KernelJob/Native39/TFO1のID・原資・義務を新purposeへ黙って移さない。

### D4 — 一つのaccepted assignmentを一度だけ消費する

actual validatorと既存native budget writerの一checkpointでterms、署名、nonce、clock、原資、source、予約、accepted rowを確定する。結果がない段階ではCase/Resultを作らず、ゼロResultIdで代用しない。

後続の新operationがaccepted AssignmentIdと既存body0 bundleを結び、native期待値を全照合して `Open→Consumed(CaseId,ResultId)` とする。ここが合意data/proverサービスとP commitの開始点であり、Assignment受理をPの先行commitにしない。output、fee、Panel、relay、attempt、Caseラベルを変えて二度目の消費を認めない。op8/op9の再送はtermsだけでなく全unsigned bundle/attachmentsがaccepted preimageと一致し、現在の署名・同じ原資/lock認可を再確認できる場合だけ無効果又は元receiptとし、料金・clock・予約を増やさない。違うbundleをidempotent成功にしない。

op9はassignment参照と既存body0 payloadを一つの受付で扱う。旧op0で先にCaseを作り後から参照を貼る方式や、参照なしの旧経路へassignment-modeを迂回させる方式は認めない。

posterごとのpermanent O(1) next_nonce/exhausted counterを採用する案とする。新受理はnext値だけ、成功時に増分、u64最大値受理後はexhaustedとし、取消・期限切れ・payload回収で使用済みnonceを再利用可能にしない。counterはsnapshotにも必須とし、正規lineageのreorgで受理と同時にundoする。回収済み古いnonceを「再送だから新受付」と扱わない。

「全branch一消費」は同じcurrent canonical executed view・同じorigin lineage内の全Result/Caseラベル横断の意味とする。排他的forkの状態を世界共通のlocal spent fileで凍結しない。assignmentのancestorを現在tipへ付け替えず、originが消えたlineageで再利用しない。reorgは消費・Case・資金・予約・支払を同時undoする。実行済みlocal I/Oを合意上の消費証跡にしたり、chain undoで未実行へ戻ったと偽装したりしない。

### D5 — 取消と未検証終了は成功・違反と分ける

Openの早期取消はposter/payer・worker/data-source・prover全3役の署名又は全3役が事前署名した有限条件を要する。Consumed後はassignmentをOpenへ戻さない。新Result受付期限、予約期間、source duties、Case期限は別clockとし、clockだけで未終了の責任を消さない。

初期取消operationはposter/workerの2署名と、proverがop8で予め認証した取消条件を用いる。既提供サービス・残責任を保護し、その条件外の3者再交渉を2署名で受け付けない。

joined proverもop13で同じ有限取消条件へ明示同意する。取消・期限切れ・Case終了では全live/retired join契約の既発生債権/capital/未完dutyをactual nativeから検査し、initial 3役の同意だけで後発joinの権利を消さない。

未消費期限切れ・Case未検証終了ではworker推論成功対価を払わない。実提供済みserviceの契約対価と、未提供fee・成功対価・解放可能capital・surplusの返金を分ける。Panel放棄・偽証拠・無署名ACKからMinerの計算有罪や追加債務を作らない。Minerに全trace・証拠構築を必須にしない。

Court.PositivePassedとP認証を別状態にする。PがPending/NoPopulation/Expired/NotCertified等でも、計算一致をMiner fraudへ変更しない。P不成立が既存policyで確定し成功対価の可能性が消えれば、未払due・reservation/liability floorを保った安全返金へ進める。P失敗だけで永久lockしない。

### D6 — 共通Positive gateを既存用途別entitlementへ追加する

Poster-funded成功対価には、exact assignment/Result、全coverage、PositivePassed、actual P-selected Certified、既存fraud・満期・責任・原資・once条件を全て要求する。旧S/K/V承認、PublicCheckCertified、一致local replayを代替にしない。

共通Positive AND既存entitlementは、計算信用・推論成功対価・結果利用に適用する。未検証でも実提供済みservice fee・安全refundは、actual receipt・未払due・reservation/liability floor・原資・onceで別判定する。generic PositiveCreditGateの成功だけでDagWeight、RuleE、WorkReceipt、coinbase/model leg、mining issuanceをgrantしない。初期assignment単体はこれらを拒否する。settlement/dependency/prefix等の結果利用も、消費先の既存用途が認める範囲で同じPositive predicateを追加する。正しいpaid計算という事実から新しい発行権を作らない。

paid worker success専用のappend-only純API用途WorkerPaymentを提案する。その許可を旧Reward（primary issuance/coinbase/model/PWU等）へ流用しない。actual assignment原資・worker recipient・契約額・onceへ束縛し、他用途へ伝播させない。B/F/Rがゼロでも実worker送金額がゼロという意味ではない。

### D7 — append-onlyの新namespaceと実ownerを必要とする

対のRFCはlocal operations 8–13、aux151 kinds 10–17、assignment-purpose ID/domain/context/settlement legの追加を**提案**する。既存local 0..7、aux151 0..9、r1 Result/Case/Witness/P subject、旧claimのencoding/意味は変更しない。番号・文字列は中央namespaceレビュー後にのみ登録する。

op8だけに3役署名と新codec ceilingを設ける案であり、旧0..7のMAX_SIGNATURES=2は維持する。3組のkey/signatureだけで21,657 bytesとなる方式では、全field/attachment/outer署名を追加した実byte・heap・work・mass capをpolicyで必須化する。fence別にpredecode/dropし、未採択profileを旧2署名codecの例外として通さない。

既存row ceilingを破る3署名inline rowを作らず、一役一AssignmentRole rowと同じunsigned bundleのaccepted preimageで保存・復元する。policyは候補/join、actor/global live枠、byte/work/保持/復旧、全window、fee/capital/service scheduleの実preimageを必須とし、未定値を公開ゼロ・無限・即時成功にしない。

Result前のhistoryには新assignment-purposeのtyped bounded journal/head/frame/material retentionを用い、CaseId/ResultId必須の既存CaseJournalへゼロIDを挿入しない。actual AcceptedOrigin、連続ordinal、prev digest、native writerのhead一致、literal署名・原資・leaseをactual accepted readerで再生する。Case後はexactAssignment/BindResult frameを参照して連結する。policy+recordだけでhistory検査成功にせず、原資料欠落はUnavailable/未検証とする。前段保持・再生費をquoteし、資金lockや永久nonce/once証跡を資料回収で消さない。

bounded framing、typed scanner、restore整合性、NativeHeap、全履歴再生、same-prefix資金・予約・Caseの原子性を既存native一writer基盤へ接続する。公開DTOのbool、独立整数台帳、保存Pass、独自成功stubをnative能力にしない。モデル審査や暗号学的な計算正当性証明を導入しない。

## 3. 凍結例外の正確な範囲

求める例外は、新CanonicalTirAssignment authority、terms ID起点の独立job導出、worker同意と事前purpose資金・予約、後続Resultへの一消費binding、新operation/row/purpose、poster-funded native精算である。これを旧jobのバグ修正又は既存未割当fieldの補完と呼ばない。

変更しない対象は、既存登録metadata・最大文脈長・算術・旧job/claim ID・PAV2/PAV3・primary L1 pricing・発行・モデル階層・年次指定・Gate C・既存claim履歴である。モデル公開性の審査を行わず、有限供給義務は署名したassignment/claimに限定する。

この設計例外は**新RFC＋ADRの明示採択**なしに実装済みの成功経路へ昇格させない。採択後も、公開数値、private測定、実装完成、独立review、公開網別activationは別判断である。

## 4. 比較した代替

| 案 | 利点 | 採らない場合・残る契約 |
| --- | --- | --- |
| 本案の新canonical assignment | primary header/抽選と結果commitを分離し、結果前に期待job/workerを固定できる | 新job authorityと事前資金・一消費契約を採択する必要がある |
| primary lightweight Noticeを先行 | primary mining自体を軽量Resultへ接続する目標に直接対応 | header identity、抽選projection、Panel roots責任・origin資本、PAV3との関係が未解決 |
| 既存KernelJobのexact promptを維持した新Result | 既存posted job/payer契約を利用できる | native TIR classとの正当なbridge、worker同意、旧/新claimのjob二重消費防止が要る。promptやIDの置換は禁止 |
| 既存Native39をそのまま利用 | 結果前の同意/原資/予約の実基盤がある | class/source/job/evidence/Final目的が異なる。IDコピーでTIR/r1適合と扱えない |
| 拒否を維持して追加設計しない | 凍結仕様と公開Noneを保持できる | RFC44のactual native light Result経路は未完成のまま |

本案は以前検討したown-template方針を黙って変更するものではない。最初のnative経路の範囲を明示的に変更する追加判断であり、primary mining置換は独立した未完了項目として残す。

## 5. 必須検証と有効化条件

受入試験には、r1全field/CUゼロと正規jobの独立導出、実登録max context維持、別worker/key/class/layout/input/rule/ancestor拒否、同nonce別terms/同terms別bundle拒否、全Result/Case一消費、署名/原資再確認付き再送、A/C循環・二重払い不在、A必要額/返金floorとC payer/refund/caps、preCase data/proverサービス拒否、実native prefix/UTXO/lock変更、全原子的rollback、全joined proverの取消/期限/未完duty、Panel/P不成立のMiner責任分離、実Gate C全時刻/NoPopulation/Expiredとbound欠落拒否、P未完・未検証の計算信用/WorkerPayment拒否とservice/refund進行、WorkerPayment→Reward誤用拒否、用途別B/F/Rゼロ、reorg/restart/pruning/IBDを含める。

private finite policyの正規preimageを使った実native試験、実LLMでのMiner/Panel/Node別CPU/RAM/通信/保持・復旧測定、全case/block/epoch資源、実原資保存、全consumer auditを要求する。小型fixtureの成功を測定済みLLM profileと扱わない。数値未定・未測定・owner不在・復元未検証なら受付/公開armingは拒否する。

公開Paramsは全released presetでNoneを維持し、None時の旧fingerprint/root/wireを変えない。設計採択は有効化許可ではなく、公開網ごとの別承認と実装・検証証跡を必要とする。現時点の結果はレビュー可能な未採択案2本であり、native経路完成や発行・信用の解放ではない。
