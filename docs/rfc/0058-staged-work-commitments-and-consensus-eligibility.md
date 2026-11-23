# RFC-0058 — 実行commit・段階別抽選・仕事の一回性と合意資格

- 日付: 2026-11-17（Asia/Tokyo）
- 状態: **PROPOSED_DESIGN_NOT_ACTIVATED**。設計の修正案。native実装、実機試験、PoW計算量下界、DAG安全性の証明、公開activationは未完了。
- 判断記録: [ADR-0226](../adr/0226-staged-work-commitments-and-consensus-eligibility.md)
- 提案profile名: StagedWorkCommitmentV1（SWC1）。wire ID・fence・activation高さは未割当て。
- 依存: [RFC-0055](0055-global-admission-court-calendar-and-one-bps.md)、[RFC-0056](0056-global-claim-admission-difficulty.md)、[RFC-0057](0057-court-nonce-and-panel-challenges.md)。55は資源、56は受付D、57はCourtの義務・nonce、58は仕事・抽選・合意資格の接続を所有する。
- 読取りsource: MISAKA-BTC/misakas main@2d8212b634880b0967fbf24b00b41936c2322c14。添付docs(3).zipは別の文書入力であり、その全体をこのcommitの内容と同一視しない。
- 監査入力: ユーザーが提示した白書監査文と添付テキスト。15ページの白書PDF自体は今回の添付に存在せず、原PDFの式・ページ・証明を直接照合したとの主張はしない。

<!-- AN1-REVISION-BEGIN -->
> **2026-11-18追補 — PALW-AN1:** 最新のユーザー指示により、アドレス・登録nonceを実入力tokenへ束縛し、Panelの完全再実行とCourtを維持する案を[§16](#16-palw-an1の入力束縛と段階契約2026-11-18追補)へ追加した。対応は[RFC-0059](0059-palw-an1-input-binding-and-panel-court.md)／[ADR-0227](../adr/0227-palw-an1-input-binding-and-panel-court.md)。AN1の状態は **DRAFT_NOT_ACTIVATED**。§0〜15のSWC1提案・旧wire・既存採択は履歴と適用対象を維持する。AN1のTicketは実行Rootから導出し、SWC1の未来R_ticket抽選と混在させない。計算の正しさを暗号学的に証明する方式は追加しない。
<!-- AN1-REVISION-END -->

## 0. 判断

以下の§0〜15はSWC1の提案を記録する。SWC1では完全なOriginalを将来の一意な乱数より先に固定し、同じ計算・同じ資本機会を再抽選できなくする。Panel抽選も別の固定集合より後の未知乱数で行う。未検証のrootを持つ候補には成功報酬・新規合意資格・EXEC creditを与えず、既存の認証・成熟・取消・現在状態の再検査へ接続する。AN1には§16とRFC-0059の明示した順序・識別子を適用する。

これにより規定するのは、候補の差替え、無料の再抽選、署名・nonce・aliasによる増幅、未検証候補の権利獲得を防ぐ**状態契約**である。「rootをcommitしたので本人が全推論を実施済み」「未知乱数を入れたので非償却性が証明済み」とは結論しない。

任意の登録モデルについて、規範演算量と最適化後の実費が常に比例することは示されていない。整数完全一致でも、零重み・冗長演算・共通prefix・batch・cache・別の正確なアルゴリズムという反例候補は残る。モデル審査を追加せず、この不足を隠さない。PALWをBitcoin型PoWの代替として成立済みと呼ぶには、仕事量に関する追加の仮定・定理とDAG合意の定理が必要である。

## 1. 監査指摘の補正と維持する問題

### 1.1 実装のticketには既にrootが入る

現行attemptのexecution_commitment_v3は、unsigned attempt全体を複製してchallenge欄だけをゼロにし、canonical Borshへ直列化する。artifact_root、trace_root、output_root、execution_rootはその中にある。対応する記法は次である。

~~~text
C_attempt_zero = CanonicalBorsh(C_attempt with challenge := zero)
E = H_native(domain_exec, execution_anchor, length(C_attempt_zero), C_attempt_zero)
native_class_ticket = LE128(first_16_bytes(H_native(domain_ticket, E)))
~~~

従って「位置用challengeだけからticketを算出する」という解釈を、現行mainに対する確認済み攻撃にはしない。白書ではC_attempt_zeroの全fieldと正規化を明記すべきである。FP receiptのBE128等、別の既存wireとのbyte orderを統一して変更しない。

一方、以下は別々の課題であり、rootがhash入力に存在するだけでは解決しない。

| 命題 | この改訂での扱い |
| --- | --- |
| 固定された有効な実行・正規bytesからticketは一意 | 既存対策を保持する |
| 任意の偽rootをhashして候補を安く作れない | 一般には作れる。事前の資金・有限集合・一回性で機会を制限し、検証前の資格を与えない |
| ticketを見てからOriginalを変更できない | SWC1の因果順序・固定集合・bindingで条件付きに防ぐ |
| 正しいrootの構成には規範量以上の計算が必須 | 未証明。bindingから導かない |
| 未検証headerがclock・DAA・weightへ影響しない | fence別の実装接続と試験を要求する |
| 公開済みblock hashをfoldすれば偏りのない乱数になる | 一般には導けない。候補header選別・最後の寄与者・withholdingを評価する |

FP receiptには既にFinal／認証後の未来slot beaconとquantum一回性があり、attempt、FP、EXECにも各WorkIdの重複拒否がある。本書はそれらを削除して新制度に見せるものではない。[SOURCE_AUDIT](../maintenance/revisions/2026-11-17-work-binding/SOURCE_AUDIT.md)に実在箇所、fenceによる違い、実行確認していない攻撃候補を記録する。

### 1.2 変えない前提

整数のbit単位完全一致、モデル非保有Node、有界な局所checker、完全再実行するPanel、単独の適格反証、元Miner rootへのopeningを維持する。計算正当性の暗号学的証明は導入しない。

モデルの1.0／0.3、登録時の無審査、係数の一回適用、登録モデル全体の発行上限、追加モデルの現行burn停止契約を維持する。モデル別の性能・品質・保有者・取得認定を、WorkKeyやprofile適合検査の名で復活させない。

DNS-BFT廃止と既存のPQ-only方針は本RFCの提出だけで変更しない。§6のthreshold beaconは、既存native beaconに安全性を仮定で上乗せした実装事実ではなく、追加の判断を要する無効な候補である。

## 2. 四つの識別子

### 2.1 共通codec

新digestはH256=SHA-256、LP(x)=u32le(len(x))||x、NR(tag,v,x)=LP(UTF8(tag))||u16le(v)||LP(x)。整数は明記した幅のlittle-endian、ticketの比較値だけuint256_beとする。各LP、参照、要素数、木の高さ、総展開量は共通profileでallocation前に制限する。未知field・別のabsence encoding・重複field・overflowを拒否する。native ID・64-byte seedはNR内で全bytesを保存する。

### 2.2 JobRegistrationとOpportunityId

最初にnative資本台帳がTrialSlotを割り当て、owner、beneficiary、実資金、元job要求、model/program/content、実行規則、資源上界、全stageの時刻・乱数anchorを固定する。登録署名を検証してから索引へ入れる。

ここで事前固定するanchorは、key世代・slot・一意な選択規則である。まだできていないJobFreezeIdをRegistrationIdの中へ、OriginalFreezeIdをその集合のOriginalIdの中へ直接入れない。固定集合rootからFreezeIdを作る向きと、その後にJobSeed／ticketを作る向きを守り、hashの自己参照を作らない。

~~~text
RegistrationId = H256(
  LP("MISAKA/PALW/SWC1/REGISTRATION") || LP(network_id) ||
  policy_hash || TrialSlotKey || NR("owner",...) || NR("beneficiary",...) ||
  NR("funding-consent",...) || NR("requested-job",...) ||
  NR("model-content",...) || NR("program",...) || NR("execution-rules",...) ||
  NR("stage-schedule",...) || NR("resource-plan",...)
)
OpportunityId = H256(
  LP("MISAKA/PALW/SWC1/OPPORTUNITY") || LP(network_id) ||
  TrialSlotKey || RegistrationId
)
~~~

署名bytes・relay・transport nonceを識別子に含めない。署名はRegistrationIdと資金・責任scopeを認証する。owner／beneficiaryは結果公開後に差し替えられない。後から資料をコピーした者には送信者であることだけを理由とする採掘権を与えない。

資金、料金、control容量、固定集合処理、失敗時の精算・保持を登録時に予約する。Nodeが無制限の候補を保持してから料金を取る構成にしない。登録はモデル登録審査ではなく、そのclaim機会の有限な契約である。

### 2.3 ComputationalWorkKey

登録集合が閉じた後のR_jobを用い、共通profileのJobForSeedが実行すべきJ_execを一意に作る。次のJobSeedの全bytesを、検証対象となる入力・生成seed・初期状態の規定欄へ反映する。hashの外側にseedを足すだけ、短い位置番号だけに丸める、実行規則が参照しないdecorative fieldに置く方式は不適合である。

~~~text
JobSeed = H256(
  LP("MISAKA/PALW/SWC1/JOB-SEED") || LP(network_id) ||
  RegistrationId || JobFreezeId ||
  NR("job-beacon-anchor",...) || NR("job-seed",...)
)
~~~

JobFreezeIdは§6のJOB descriptorから導出し、JobSeed自体をpreimageへ入れない。

JobForSeedは、Nodeがモデル全量の取得・推論・巨大tokenizeをせずに、元入力rootと固定seed挿入schemaから期待する実入力commitを有界に導出・検査できる必要がある。認証済み変換のindex・最大更新経路・費用を共通profileと0055の計画へ含める。ownerが自己申告したactual-input rootをそのまま期待WorkKeyへ採用しない。

~~~text
ComputationalWorkKey = H256(
  LP("MISAKA/PALW/SWC1/WORK") || LP(network_id) ||
  NR("canonical-model-content",...) || NR("canonical-program",...) ||
  NR("arithmetic-rules",...) || NR("actual-input",...) ||
  NR("initial-state-and-dependencies",...) ||
  NR("generation-and-stop-rules",...) || NR("execution-range",...)
)
~~~

実行規則にはdtype、overflow、丸め、演算順序、tokenizer、生成seed、停止、長さを含める。actual-input等は全データそのものでも、共通codecで認証する一意のrootと型でもよい。Nodeは有界な認証参照を扱い、Panelは元の全scopeを検証する。

WorkKeyに使うのは実行意味の正規化であり、総policy_hashではない。難易度、Panel数、署名版、料金、保持期限、証拠chunking、意味を変えないtrace木のlayout更新はOriginal側へ束縛し、WorkKeyを作り直さない。既存artifactのrootが表示class名や包装まで含む場合、内容同一性を正規化するadapterが必要であり、名前だけを削除して完了としない。

WorkKeyには自由なclaim番号、owner署名、beneficiary、結果root、料金、保存期限、receipt nonce、carrier、表示モデル名、抽選epochを直接混ぜない。job生成に実際に使われたJobSeedはJ_execの検証対象なので含む。nativeモデルIDは登録・資格照合のため別に保存し、同じprogram・重みの単なるaliasでWorkKeyを増やさない。

これは**同じcanonical計算記述**の再信用を防ぐキーである。意味が同じ別program、代数的に等価な別モデル、近い入力の共通計算までは同値判定しない。「キーが違うなら費用も比例して増える」という命題は主張しない。

### 2.4 OriginalId

ownerは割当て済みの一つのWorkKeyについて、実行後に元Originalを一回だけ固定する。

~~~text
OriginalId = H256(
  LP("MISAKA/PALW/SWC1/ORIGINAL") || LP(network_id) ||
  policy_hash || OpportunityId || ComputationalWorkKey ||
  NR("native-original",...) || NR("artifact-root",...) ||
  NR("trace-root",...) || NR("output-root",...) || NR("execution-root",...) ||
  NR("trace-manifest-and-counts",...) || NR("work-accounting",...) ||
  NR("beneficiary-and-liability",...) || NR("material-supply-plan",...)
)
~~~

Originalの署名subjectはOriginalIdと元の認可scopeである。root名だけで全実行を覆ったと扱わない。Init、全演算とtensor接続、メモリ／KV、非線形処理、生成・停止、End／出力、宣言仕事量を規範関係と認証indexへ対応させる。各関係が有界なNode反証へ分解できないprofileは受付不可である。

Originalの中に旧nativeの位置用challenge等がある場合、正規化はadapterの固定codecへ明記する。後から無視するfieldを増やしてrootや責任を差し替えない。native WorkIdと新キーは認証された対応表に保存し、旧WorkIdの意味や幅を変更しない。

## 3. 一回性、先取り、保存

### 3.1 ownerを結果の先着で決めない

任意のhashを予約するAPIは禁止する。NodeがJobRegistrationとR_jobから期待WorkKeyを導出し、その登録ownerだけがOriginalを固定できる。不正署名・未資金・別登録の自己申告キーは、TrialUse、WorkUse、正直者の予約を一切変更しない。

同じ利用者promptでも、正規の新登録から別のJobSeedが実際のJ_execへ作用すれば別の計算記述となる。複数の正当登録が同じWorkKeyへ写った場合は、R_job前に固定した登録順で一つのownerを決め、他はDuplicateWorkとして精算し、追加ticketを作らない。Originalの先着や結果公開後の署名速度で決めない。

自分の正規登録へ偽rootを固定したownerは、その機会を消費する。落選、反証、資料不提出、自己都合cancel後にWorkKeyを解放して再抽選できない。別のownerの仕事を無資金で永久に封鎖する経路と、自分の有資金機会を失敗させることを区別する。

### 3.2 必須台帳

| 台帳 | 原子的に保存するもの |
| --- | --- |
| TrialUse | 既存資本outpoint／sequence、登録、費用、使用済み状態 |
| WorkUse | WorkKey、固定owner、OpportunityId、OriginalId、唯一のtarget、terminal理由 |
| StageUse | stage名、key世代、slot、固定集合、唯一のbeacon anchor、結果またはUnavailable |
| OriginalState | 元root、資料責任、Panel割当、Q／case、認証、成熟、取消世代 |
| RightsUse | 元仕事から発行する各native権利、量、用途、予約／消費／失効とprovenance |

署名変更・別bondへの資金移転・alias・route・batch・epoch・policy版変更だけでは台帳を初期化しない。同じ元仕事を複数用途へ数える場合も、既存の量とquantum分割規則の合計内に限り、新しい総仕事量を生成しない。

「一仕事一抽選」は本書の新規仕事受付機会を指す。既存の有限なreceipt quantumや搬送slotの抽選まで同じticketに統合する意味ではない。それらは元仕事から既定量を消費する後続用途であり、受付で落選した仕事を復活させたり、同じquantumを新しい仕事として再発行したりできない。

WorkUseの終了recordを消して同じrootを新epochへ持ち越すことを禁止する。pruningは、期限切れjobの再登録拒否と認証済みsummary／nullifierを合わせて行う。期限を越えたJobSeedは新機会へ再使用できない。固定jobを反復する別サービス支払いは可能だが、SWC1の新しい採掘仕事とはしない。

nullifierの保持も無料ではない。新profileは最大live登録数、更新／membership検査量、summaryの保持、IBD／reorg journal、GCをRFC-0055のstock・flow予算へ含める。有限なretentionで古いキーが復活する実装は不適合。無限履歴を全NodeがRAMへ持つ設計にもしてはならない。

summaryはexactな認証状態であり、Bloom filterや期限付きcacheだけで代用しない。使用済み／未使用の認証付き復元ができない場合は未使用と推定せず新規受付をholdする。hot stateと一回の検査量を有界化することを、ネットワーク全体の総履歴bytesが永久一定という主張にしない。旧seedの再入不能・跨profile正規化・checkpoint境界を証明できないうちは、epoch終了だけでtombstoneを捨てない。

### 3.3 batchと範囲の再包装

batch envelopeのrootだけを新WorkKeyにしない。各canonical execution leaf／rangeのWorkKey、元の仕事量・quantumの一回性を保持する。落選leafへ小さな別leafを足す、順序や境界を変える、別batchへ移す操作で元仕事を再抽選・再信用しない。batch rootは固定した順序付きleaf集合と元責務を束縛する外側のcommitである。

0056の複数original batchは各originalにticket・資金・予約を要求する。既存のatomic claim-batchと接続する場合も、leaf／rangeの再利用と重複仕事をnativeに正規化し、全leafの責務を有限に予約するadapterを必要とする。そのadapterが未接続の経路は新SWC1 originalをdefault denyとし、既存batchの署名や時計を読み替えない。

## 4. 因果順序

~~~mermaid
flowchart TD
  A["資金・登録集合を固定"] --> B["R_jobから実jobを導出"]
  B --> C["Original集合を固定"]
  C --> D["R_ticketで受付を選別"]
  D --> E["受理対象とpoolを固定"]
  E --> F["R_panelで検証者を選出"]
  F --> G["検査・court・成熟"]
  G --> H["用途別資格と権利消費"]
~~~

| stage | seedより先に固定する対象 | 後から許さない変更 |
| --- | --- | --- |
| JOB | 登録、model/program/rules、owner／beneficiary、資金、実行job写像、全stage予定 | 当たるseedを見て登録を追加する |
| TICKET | 全Original、WorkKey、target、D、資源・費用版、集合の閉鎖root | root、出力、仕事量、保持条件、targetの差替え |
| PANEL | 受理済みoriginal集合、各元root、pool snapshot、抽選規則、責任／期間 | 都合のよい集合・pool・roundだけを選ぶ |

「一般公開されていない」だけでは未知ではない。攻撃者が当該集合の閉鎖前にthreshold shareを復元する場合も予測として扱う。各seedの未知性は、そのstageの前提として証明・測定対象にする。

R_jobからdomainを変えてR_ticketやR_panelを作るだけでは、先行公開後の未知性はない。同様に既に公開したR_ticketをhashし直してPanel抽選用の未知乱数とはしない。Panel選出と、その選出後のCPN1個人challengeは、PANEL stageの同じ未知seedをdomain分離して使えるが、無条件の統計的独立性は主張しない。

### 4.1 120秒制御との接続

CAD1の120,000ms epochとD更新式は維持する。target eのDは既存のStart(e−2)より前に固定される。

- JOB登録の閉鎖とR_jobはOriginal収集開始より前に終える。
- target eのOriginal／Presealはepoch e−2に固定し、Start(e−1)で閉じる。
- R_ticketはその閉鎖後、Start(e)−G_readyより前に認証し、eligible payloadの準備時間を確保する。
- epoch eのcanonical prefixで資金・全工程予約を確定する。
- その受理集合とpoolを閉じた後にR_panelを得る。以後の取得・完全検査・courtは予約済みの元時計で進める。

JOBの前置区間、R_panel待ち、完全検査、訴追／成熟は追加のlatencyである。「一つのclaimが120秒で必ずFinal」「30秒一件」「120秒全体で一件」の規則へ読み替えない。複数modelの適格claimは同じglobal vector予算内で並行できる。

各stageのcut、伝播余裕、最遅合法caseと精算までの上界を受理前に同一scheduleへ固定する。一epochのguardへ収まらない処理を暗黙に押し込まない。必要な長さを持つprofileがなければ新規受付を停止する。既受理救済は元予約で継続する。

### 4.2 presealとclaim受理の時計

JobRegistration／Original presealでは、有限な入口・stage・保持・精算だけを予約する。全候補のfull court責任をD判定前に受理しない。canonicalなclaim admission aは、ticket合格後に全工程bookとnative originalを同checkpointへinstallした時点で一回だけ定まる。

aの導出、各untilへの変換、R_panel待ちの最悪時間を同じprofileへ固定し、予約時に残りの取得・完全検査・court時間を確認する。後のPanel抽選や資料到着でaを取り直さない。preseal自身にも固定target／expiryがあり、claim未受理の失敗を次epochへ無料carryしない。旧claimは元のaと時計を維持する。

## 5. CAD1 ticketとの統合

SWC1ではCAD1のDを一度だけ用い、§4で固定したOriginalに対する次のticketを入口にする。旧CAD1 wireのhashを無言で変更するのでなく、新profileのadapterとして識別する。

~~~text
WorkTicket = uint256_be(H256(
  LP("MISAKA/PALW/SWC1/WORK-TICKET") || LP(network_id) || policy_hash ||
  u64le(target_epoch) || OpportunityId || ComputationalWorkKey || OriginalId ||
  OriginalFreezeId || NR("ticket-beacon-anchor",...) || NR("ticket-seed",...)
))
T(D) = floor((2^256 - 1) / D)
AdmissionEligible = WorkTicket <= T(D)
Rank = (WorkTicket, OriginalId)
~~~

一様性を仮定したこのgate単体の通過確率は(T(D)+1)/2^256。native class／W等の残る必要条件を含む最終受理確率まで同じとはしない。D、W、FP receipt、外側1 BPSの状態・意味は分ける。複数の新Dを掛けて難易度を二重計上しない。

固定header、サイズ上界、ingress lease、資金付き既存登録参照、一回性、ticketの後に、boundedなdecodeと重いmaterial／plannerを進める。署名・lookup等の前段費用はRFC-0055のlocal／control上限で抑える。未知packetや無資金の送信件数をDの需要へ加えない。

D更新は0056の適格需要、残予約、消費flow、stock peak、完全性を使う。R_job／R_ticket不成立はUnavailableでありlow需要ではない。R_panel障害に伴う受理済み責任は実予約として残し、精算までの負荷を観測する。偽のarrivalをそのままdifficultyへ入力しない。

rootが虚偽でもticketに当選する場合はある。ticket合格は正当性認証ではない。偽Originalは予約されたPanel／courtで失敗する必要があり、その前には§10の効果を与えない。全候補が当選してもhard予算を超えるoriginalは受理しない。

## 6. Beacon能力契約と未採択の候補

### 6.1 必須の能力

各stageに一つの次のrecordを固定する。

~~~text
CanonicalStageDescriptor =
  u16le(1) || LP(network_id) || policy_hash ||
  u16le(stage) || u64le(key_epoch) || u64le(stage_slot) ||
  NR("parent-stage",...) || NR("stable-control-anchor",...) ||
  frozen_set_root[32] || u64le(frozen_count) ||
  NR("stage-schedule",...) || NR("randomness-scheme",...)
FreezeId = H256(
  LP("MISAKA/PALW/SWC1/FREEZE") || CanonicalStageDescriptor
)
BeaconMessage = H256(
  LP("MISAKA/PALW/SWC1/BEACON-MESSAGE") || FreezeId
)
~~~

stageはこの新codec内部のJOB=1、TICKET=2、PANEL=3。他の値は拒否し、既存wire番号は再割当てしない。parent-stageは直前のFreezeIdの認証参照であり、JOBの開始参照／absenceは共通profileで一意に固定する。randomness-schemeには方式版、認可されたkeyとその世代を含め、署名や生成済み乱数を含めない。stage-scheduleは元のclock版、close／ready／expiry cut、伝播・包含の余裕を含める。

集合の要素はcanonicalな登録／受理順の固定ordinalで並べる。JOB leafはRegistrationId、TICKET leafはOpportunityId||WorkKey||OriginalId、PANEL leafはOriginalId||NR("admission-cut",...)||NR("pool-snapshot",...)||NR("draw-policy",...)。署名bytesや受信順は使わない。leaf_i=H256(LP("MISAKA/PALW/SWC1/SET-LEAF")||u16le(stage)||u64le(i)||LP(item_i))、padding_i=H256(LP("MISAKA/PALW/SWC1/SET-PAD")||u16le(stage)||u64le(i))、parent=H256(LP("MISAKA/PALW/SWC1/SET-NODE")||left||right)とする。非空集合は最小の2冪へpaddingし、空集合はH256(LP("MISAKA/PALW/SWC1/SET-EMPTY")||u16le(stage))とする。countと最大木高を事前に検査する。

tree rootとcountを認証する。FreezeIdへ証明の署名bytes、集約subset、leader、relay、block header nonceを含めない。closed setを署名し直して別乱数messageにしない。JOB／TICKET／PANEL各集合の順序・index生成も登録済みcontrol予算内で行い、未予約の全候補sortを作らない。

能力契約は次の全てを要求する。

1. 同じstage_slot／key_epochに矛盾する二つの集合を閉鎖しない。
2. 閉鎖済みdescriptorに対する乱数値は一意。正規署名者subsetの選択で値が変わらない。
3. 閉鎖より前に敵対者が乱数を予測・復元できない。鍵登録、DKG結果、slot選択も固定対象。
4. 無効なshare、最後の寄与者の保留、早期取得、committee侵害を脅威モデルへ含める。
5. 障害で次slot、別key、別root、block hash、local乱数へ無料fallbackしない。
6. 乱数の検証・保持・配信・障害終端は有界。Nodeごとの受信時刻で合意上の期限や結果を分岐させない。
7. 未検証の当該PALW仕事でその仕事のfreeze／beacon／異議包含を支える循環を作らない。

乱数値が一意でも公開が必ず成功するとは限らない。release拒否時は新規仕事を有限にAbortUnverified／Unavailableへ進め、実費以外を元契約どおり精算する。同じOpportunity／WorkKeyを別のseedへ移さない。障害者への制裁は客観的な署名責務違反が立証できるscopeだけに限る。

### 6.2 threshold BLS参照候補 — DISABLED

固定public keyと固定messageに対する一意なthreshold BLSは、share subsetに依存しないsignatureから乱数を得る候補である。仮にcommitteeをn=3f+1、freeze certificateを2f+1、beacon thresholdを2f+1とする参照構成では、少なくともf+1の正直shareが必要となるため、正直shareを有効なfreeze前に出さない規則が重要になる。

これはMISAKAで採択済みの委員会数・fault boundではない。運営者・stake・鍵を区別したcommitteeの選出、初期key、DKG qualification、rotation、失敗時の停止、adaptive corruption、network同期／部分同期の前提を別の完全な採択案で固定するまで**DISABLED**である。BFTの安全性からPanelに能動的な正直者が一人いることは導かない。

drandの一次資料は、threshold BLSのsubset非依存性を説明する一方、早期取得、DoS停止、threshold侵害後の将来beacon予測を記述している。drandの既存運営groupをそのままpermissionlessなMISAKAの保証へ移植しない。

またBLSを利用する候補は古典暗号の前提を追加する。外側にML-DSA署名を付けてもBLS乱数の予測不能性が耐量子化するわけではない。既存PQ-only方針への明示的な例外採択、又は同能力を満たす別のレビュー済み構成がなければ有効化不可である。

既存DNS-BFTを復活させない。乱数用合意を台帳決済のcheckpoint合意へ拡大しない。新BFT finalityへ移るなら別RFC／ADRが必要であり、本書の範囲ではない。

### 6.3 native beaconとの互換性とfork

既存のwork-derived beaconがあるという事実だけで§6.1の適合とはしない。当選済みの一実行からheader nonce／timestamp／再署名を変えて複数block hashを作り、次のreceipt beaconを選別できる候補経路を、fence別に検査する。k個のhashをfoldしただけで「操作には全kの支配が必要」とはしない。

stable_control_anchorの安全性をPALW自身の未検証weightへ循環依存させない。canonical reorgの可能性があるfreezeを不可逆と呼ばない。anchorが後に非canonicalとなった場合は元stageを成功へ再使用せず、権利・予約を元journalでundoする。既知beaconへ新しい集合を結合することも、都合のよい別stageで同じWorkKeyを再抽選することも禁止する。

全forkを通じた一回性・stable anchorの共通性は、ローカル台帳だけでは保証できない。新profileが要求するこの境界を満たす合意上の根拠が未確立なら、新SWC1による合意資格の有効化は行わない。

## 7. Panel抽選とRFC-0057の修正

受理集合、各OriginalId、WorkKey、pool snapshot、抽選policy、全actor責務をPANEL stageの前に固定する。drawはR_panelと固定したclaim識別子からnative規則で一意に求める。producerのoperator除外、成熟bond、役務上界、同principalの重複位置、支払一回性は既存規則のまま扱う。

CPN1のContextHashを旧bytesのまま拡張しない。新SWC1 adapterのpositive contextは、元CPN1 contextにOpportunityId、WorkKey、OriginalId、PanelFreezeId、R_panelの全native参照を結合する。Action contextは元のnative case／duty、OpportunityId、WorkKey、OriginalIdへ結合し、未来のPanel freeze／seedを要求しない。正確な分岐は0057 §16による。ObligationKeyとnative Reward／OffenceOnceの一回性を増やさない。

positiveはPanel本人の元scope・元root・割当・個別challengeへ署名する。ここでいうPanelのCourt nonceを、元Minerの計算traceへ混ぜない。§16のAN1登録nonceは実入力を決める別の値であり、Panel選出後のCourt nonceとは区別する。共有する正しいopeningは再利用できるが、他者の署名封筒は本人の承認にならない。対象の重複を独立coverageや追加仕事量へ数えない。

negative、資料要求、応答、終端は、元のActionNonceとnative義務を使い、新D・新beacon・新EXEC creditを待たせない。個別sample外の有効反証、適格第三者の反証、肯定後の訂正を受け入れる。単独の有効反証／未解決Qを、多数のpositiveで相殺しない。

### 7.1 一人の正直者と確率

全選出者に完全検証責務を課し、PublicOneHonestの最低完全承認1件を維持する。「各自のsampleだけ計算すればよい」「全員の新challenge提出が必須」へ変更しない。最低承認数とPanelの人口数は別である。

独立の復元抽出、敵対的抽選質量f、m枠という理想条件なら、全枠敵対確率はf^m。最大N回の機会に対するunion boundはmin(1,N f^m)である。実際のpoolの相関、重複principal、adaptive corruption、取得／実働を含めた成功確率ではない。

例としてf=1/3、m=32、N=1,000,000なら、上界は約5.40×10^-10、百分率で約0.0000000540%。m=5の単回値は1/243、約0.4115%であり、「五人なら一人は必ず正直」の根拠にはならない。これらを新しい採択mにはしない。

失敗・破棄を含む全ての有資金機会を数える。乱数が未知でも、攻撃者は大量の支払済み登録から有利なものだけ公開し得る。その機会数、資本、料金、未公開時の責任と終了を追跡する。独立なNを仮定せずunion boundを使う場合も、各条件付き試行確率の根拠を示す。

## 8. 資料とcourtの活性

元Minerが資料を固定したこと、署名ACKがあること、URLがあること、他人が取得したことは、その正直なPanelの全scope取得・完全検査・期限内包含の証拠ではない。

RFC-0050およびRFC-v2-0018のInspectionPrerequisitesEstablishedを維持する。native手順が、その申立人の元scope、内容認証、残りの検査時間、提出・包含機会を有界な客観的根拠で確認する必要がある。これは自己申告boolでも、既存wireの実装完了という意味でもない。

資料の虚偽未着と選択的秘匿を、有限資源の手順で区別・終結できないprofileは**PROFILE PROCEDURE NOT ESTABLISHED**。新beaconやBFT署名はこの不足を埋めない。

RFC-0055の全合法actor・全段階予約へ、JOB／Original／Panelの各stage、個別challenge、最遅資料要求、局所化、反証、応答、終端、保持と返還を含める。敵対者が全合法要求枠を使っても、既受理の正直者の手続き用予約を新規claimへ流用しない。

資料到着が遅く残検査時間が不足する場合、時計をリセットして成功にしない。元契約の有限なservice hold／帰責可能な供給違反／未検証Abortへ分ける。全拒否・全Abortしかできない実装を活性の達成と数えない。正常供給、冷状態のPanel、最遅合法反証が同じprofileで成立する実機証拠を要する。

## 9. 検証と成熟の状態契約

以下は新profileに対する導出viewであり、旧wireのFinal enumを改名しない。

| 状態 | 必要条件 | まだ与えないもの |
| --- | --- | --- |
| Registered | owner／資金／job規則／予定を固定 | 成功報酬、work資格 |
| Committed | native WorkKeyに唯一のOriginalを固定 | 正しさの認証 |
| TicketEligible | 固定DとR_ticketに合格 | Final、合意weight、EXEC credit |
| VerificationPending | 全工程予約とPanelを固定し検査中 | 未解決Qを無視した成功 |
| VerifiedPendingMaturity | 成功検証・認証条件を満たすが責任／成熟が残る | SWC1由来の成功権利の解放 |
| MatureEligible | 当該用途の成熟、依存、取消、資本、一回性が全て有効 | 自動の台帳settled |
| Refuted／AbortedUnverified | 反証／未検証終了と元の責任処分 | 失敗仕事からの新しい成功権利 |

MatureEligibleは台帳のsettledではない。EXECのMinted／Reserved／Consumedと、取引のprovisional／BASE accepted／settledは別の状態機械である。仕事の成熟だけで取引順序の不可逆確定を宣言しない。

既存sourceにもretention、Final後のretirement、quantum、依存責任を合成するmaturityがある。今回の追加は、その規則と新WorkKey・stage・全consumerの接続を検証し、新profileの成功権利を同じversionの成熟境界まで解放しないことである。現行mainに成熟検査が存在しないとの主張はしない。

### 9.1 期限

各accepted Originalは同一policyでnotice_until、material_until、inspect_until、response_until、adjudicate_until、liability_until、release_after、用途別mature_afterを固定する。

受付untilはinclusive、解除afterはstrictly greaterとする。untilで合法に受理したcaseは、以後の有限な応答・裁定終端まで予約を保持する。mature_afterは全合法caseの最大終端、依存責任、認証／retention／quantumの既存成熟を下回らず、必要ならchecked_add(1)でinclusive境界の翌tickへ進める。同じclock値のbatch順序とcutをpolicyで固定する。

時刻を過ぎたことだけで未処理caseをResolvedにしない。元手順に従う裁定／客観的default／未検証Abortが必要である。元の合法caseが完了できないなら成熟しない。

不可逆なretired floorと無期限の遡及取消を同時に約束しない。新profileは、責任窓内の取消、窓後の新規使用停止／別の責任処分、通常reorg、既存settlement権を受理前のpolicyで区別する。旧claimの訴追権を短縮しない。期限後の新証拠が過去fork-choiceへ与える意味はDAG定理の仮定と一致させる。

### 9.2 同batchの処理順序

1. 合法なnotice・資料・応答・証拠を元予約へ記録する。
2. 反証、Q、依存取消、credit失効を適用する。
3. finite case終端、期限、返還、成熟候補を計算する。
4. 更新後の状態から資格を導出し、mint／reserve／permit／consumeを検査する。
5. 取引効果、費用、once、provenanceを同checkpointにcommitする。

候補bodyの前方にconsumeを置いて反証より先に既消費へ昇格させない。前のcanonical batchで消費済みのものは元のprovenanceを保持し、claim取消だけで再発行もしない。branchの正規reorgでは、関連する台帳・case・予約・消費を一体でundo／replayする。

## 10. 未検証候補から許す効果

この表は新profileの目標契約であり、現行全fenceで実装済みという意味ではない。

| 効果 | Registered〜VerificationPending | VerifiedPendingMaturity | MatureEligible |
| --- | --- | --- | --- |
| 有界な候補保管／relay | control／ingress予算内のみ | 同左 | 同左 |
| 受付実費、lock、供給等の既存役務精算 | 元の有限契約どおり | 同左 | 同左 |
| 新PALW仕事としてのDAA標本／work clock寄与 | 不可 | 不可 | 用途版が許す確定標本だけ |
| safe weight／frontier／settlement work | 不可 | 不可 | 現在の認証・成熟・依存状態が許す範囲 |
| SWC1由来の仕事成功報酬／EXEC credit発行 | 不可 | 不可 | 資本・予算・一回性を再検査して可能 |
| 既受理courtの進行／BASE・control | 独立予約で継続 | 継続 | 継続 |
| 台帳settled | この状態からは導けない | 同左 | 既存DAGの別条件が必要 |

既存のnetwork clockやBASEの進行を全停止する表ではない。当該PALW候補自身が未認証のwork寄与を得て、自己の抽選・認証・異議期限を進めることを禁止する。どのclockが独立で、どのsourceをDAAへ数えるかをfenceごとの接続表にする。

EligibilitySnapshotはnative状態から次の内容を導出する。

~~~text
{ policy_id, source_work_key, source_original, canonical_state_cut,
  dependency_root, case_root, maturity_cut, original_expiry,
  revocation_generation, purpose }
~~~

自己申告snapshotを信用しない。過去snapshotで適格でも、各consumerは同batch反証優先後の現在状態、用途、量、未消費、expiryを再検査する。hold／取消をactor署名・新nonce・新carrierで回避できない。高価な全モデル走査を毎slot行わず、認証索引・差分更新を予算内で使う。

## 11. 経済と計算量について証明する範囲

### 11.1 条件付きに主張できること

hashのbinding、正しいcanonical codec、stageの一意性と閉鎖前未知性、台帳の原子的遷移を仮定する。一つの許可済みOpportunity／WorkKeyには一つのOriginalと一つの新規仕事受付ticketしかなく、結果を知ってからroot／外装を変更して新しい試行を得られない。既存receipt quantum等の後続用途は§3.2の有限量会計に従う。

この仮定の下で、固定した一つの正当な候補がDのgateを通る確率は(T(D)+1)/2^256である。十分な独立性等を別に仮定した複数試行の費用評価にのみ、期待試行数約Dを使う。固定集合全体の選択、候補破棄、偏った乱数、関連jobの計算共有を無視した費用定理にはしない。

### 11.2 主張できないこと

既知の正しいrootを安価に構成できる者、外注先、同じ計算をcacheする者は、本人がフル実行していなくてもcommitできる。これはbinding破りではない。未知seedが実入力へ作用しても、モデルがその入力に実質依存しない、共通計算が大きい、batch化が有利な場合、各キーの費用は大きくならない。

従って、検証可能な結果の正当性と本人の物理労働の証明を分ける。正しい最適化traceを「手抜き」としてslashしない。FullReplayという本人の責任付き承認に対する客観的な誤りはRFC-0057の有限責任で扱うが、正しい結果を共有した事実から不労を立証しない。

### 11.3 必要な費用評価

共通profileについて、元実行、完全trace／root構成、cold／warm、共通prefix、batch、KV再利用、定数／疎／低ランク／冗長program、別の正確な実装、資料共有を含めた最適化側の費用を測る。結果はモデルの昇格・失効・審査に使わず、プロトコルが主張可能な安全性の評価に使う。

規範workをwとした普遍的な実費下界C_adv≥αwは未証明であり、有限benchmarkだけでは全登録モデルに証明できない。全モデルを無審査で扱う前提のまま、どの追加仮定で経済安全性が成立するかを明示する必要がある。

既存のbond／発行上限で経済的損害を有界化することは、LLM仕事の下界を証明することとは別である。既存の採択済み報酬条件を本書の未証明項目だけで遡及無効化しない。ただし、新profileを「PoWとして成立済み」「Bitcoin型PoWを置換できる」と公表する根拠にはしない。

## 12. DAGの独立した証明義務

ticket、局所checker、one-time会計が正しくても、fork-choice関数のcommon-prefix、収束、活性は自動的に導けない。

新WorkKey・stage delay・認証・用途別maturity・取消・participation・retired floorを入力とする実際のfork-choiceを固定し、少なくとも次を扱う。

- 認証待ちの多数model、並行DAG、ネットワーク分断と再合流。
- 一時的に誤認証されたweight、後日の反証、依存取消、pruning。
- 最遅合法caseと、異議を運ぶcontrol／court容量。
- beacon停止・選別・reorg時のstageと権利の復元。
- 既消費creditのprovenanceと、通常のcanonical undo／replay。
- adversaryのwork費用、stake／Panelの仮定、network遅延を混同しないこと。

全モデルの搬送合計1 BPSはRFC-0055の目標であり、同時に多数のclaimが検証中でもよい。1 BPSを1秒Final／1秒settled／一model一blockへ読み替えない。処理待ちを隠して観測分母から落とさない。

## 13. 移行と受入条件

旧claim・旧署名・旧ticket・旧WorkIdは元版で再生する。SWC1を明示した新originalだけに新順序を適用し、対象profileの旧route downgradeで新責任を作れないようにする。途中のCAD1 target、CPN1 obligation、未消費権利を新seedへ付け替えない。

| gate | 必須の成果 | 本改訂時点 |
| --- | --- | --- |
| G1 文書・canonical contract | 56／57／58の責務、codec、状態、期限、一回性が整合 | 提案として記述・レビュー対象 |
| G2 beacon／stable freeze | §6の能力、fault model、非循環anchor、方式の方針適合 | 未成立。BLS候補はDISABLED |
| G3 native全経路 | Studio→登録／Original→Panel→court→権利、header／DAAの全consumer | 未接続・未実行検証 |
| G4 正常／敵対的活性 | cold Panel、資料秘匿、虚偽未着、合法court洪水、正常成功 | native実機証拠が必要 |
| G5 負荷・回復 | 1万〜数万model、資源上界、snapshot／IBD／reorg、実1 BPS | 実機測定が必要 |
| G6 PoWとしての主張 | adversarial cost／非償却性の仮定・下界 | 未証明 |
| G7 DAG安全性の主張 | common-prefix／収束／活性、取消・成熟・分断込み | 未証明 |

G6を実装試験でPASSへ変えない。G7をthreshold beaconや状態遷移の小さなモデルでPASSへ変えない。既存ネットワークの稼働状態を本RFCだけで変更しない。

G1〜G5は設計・実装・運用の接続証拠であり、それだけで新SWC1の本番合意weight／権利を解放しない。実際に採用する経済・敵対者前提とG7の安全性判断が必要である。G6はPoW費用下界の成立を主張する根拠であり、未証明だからと旧経済を遡及停止する規則ではない。隔離された実装試験、本番資格の有効化、PoW成立の公表を別の判断とする。

実装・検証の具体的な作業は[接続計画](../design/palw/integration/staged-work-implementation-plan-2026-11-17.md)、攻撃別の判定は[脅威表](../maintenance/revisions/2026-11-17-work-binding/THREAT_MATRIX.md)、検査結果は[改訂記録](../maintenance/revisions/2026-11-17-work-binding/README.md)に分ける。

## 14. 必須の反例と試験

| ID | 入力・攻撃 | 必須の結果 |
| --- | --- | --- |
| W01 | 元attemptのchallenge以外のrootを変更 | 既存commitmentが変わることをsource／nativeで確認。欠落root攻撃と混同しない |
| W02 | 同じ有効実行からnonce／署名／relay／aliasを変更 | 新SWC1の仕事・ticket・権利は増えない |
| W03 | 偽rootで安価な候補を固定し当選 | 当選は可能でも未検証weight／信用なし。有効反証で成功を阻止 |
| W04 | R_ticket後のroot差替え／追加／別epoch再試行 | freeze／WorkUse／TrialUse不一致で拒否 |
| W05 | 無資金・不正署名が正直者のWorkKeyを先取り | owner予約・一回性・資金を消費しない |
| W06 | 自分の正規登録を失敗させて再挑戦 | 元機会は終端。新seedへの無料carryなし |
| W07 | seedをouter hashだけへ追加／既知seedをdomain分離 | 実job写像／stage未知性要件に不適合 |
| W08 | 正しいrootをcache／定数モデルで安く構成 | 算術不正にはしない。計算費用下界の反例として記録 |
| W09 | 集合・鍵・slot・share subset・header variantを選別 | 固定descriptorと一意性を要求。旧beaconの候補経路は実機試験 |
| W10 | freeze前のthreshold復元／beacon保留／DKG再選択 | 能力契約違反又はUnavailable。別seed fallbackなし |
| W11 | R_panel判明後にPanel対象／pool／roundを選択、又は既知R_ticketを未知Panel seedに流用 | PANEL閉鎖と未来R_panelの順序に不適合 |
| W12 | 他Panel封筒のコピー／自分の対象の共有opening | 前者拒否、後者は正しい局所資料として扱う。独立労働証明にはしない |
| W13 | 多数肯定＋一人のsample外反証／資料Q | 反証／Q優先。新D／新seed不要 |
| W14 | 同batchの反証とmint／reserve／consumeを並べ替え | 更新後stateで一意に拒否、既消費の先取り不可 |
| W15 | liability最終tick／未終結case／直後の成熟 | 境界とfinite terminalに従い、早期資格なし |
| W16 | Final／MatureEligibleだけでsettledを宣言 | 別のDAG条件が不足し拒否 |
| W17 | cancel／prune／policy変更／正規reorg | 通常処分では復活不可。正規undoは全関連stateを一致させる |
| W18 | 部分資料／虚偽ACK／正直者だけ秘匿／遅着 | 検査前提を捏造せず、元期限で終端。正常成功も別に必須 |
| W19 | 1万以上のmodelと大量登録／当選／合法case | 共通vector／ingress／stock維持、保護救済を新規へ貸さない |
| W20 | 実LLMの同一claimをStudioから最後まで送る | 正常と反証、権利消費、再起動／reorgを同版nativeで確認 |

抽象モデルでは閉鎖順序、hash／一回性、偽root当選後の阻止、共有証拠と責任、成熟／同batch優先の契約を調べる。LLMの実費、BLS／DKG、network同期、実署名、資料可用性、DAG定理をfixtureで代用して証明済みにしない。

## 15. 参考

- [固定sourceの監査](../maintenance/revisions/2026-11-17-work-binding/SOURCE_AUDIT.md)
- [既存の検証先行Final](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0020-verification-gated-final-and-adversarial-models.md)
- [既存のDAG／native settlement](https://github.com/MISAKA-BTC/misakas/blob/7ad72a9e0795586d50e0d1b9cbb99a222c500a33/docs/rfc-v2/0010-dag-consensus-and-native-settlement.md)
- [既存の公開資料・一人の正直Panel](0050-public-materials-and-one-honest-panel.md)
- [drand Cryptography](https://docs.drand.love/docs/cryptography/) — threshold署名の一意性等の参照。MISAKA採用の根拠ではない。
- [drand Security Model](https://docs.drand.love/docs/security-model/) — DKG、早期取得、停止、threshold侵害の区別。
- [drand Timelock Security Assumptions](https://docs.drand.love/docs/timelock-encryption/) — 同系暗号の非耐量子という制約の明記。

外部の推論証明製品名・倍率・論文の性能値は、本RFCの設計成立の根拠に採用していない。

## 16. PALW-AN1の入力束縛と段階契約（2026-11-18追補）

### 16.1 適用範囲と方式の分岐

本節は[RFC-0059](0059-palw-an1-input-binding-and-panel-court.md)のAN1に対する、仕事・固定時点・一回性・合意資格の接続を定める。状態は **DRAFT_NOT_ACTIVATED**。旧SWC1・CAD1・native attemptのhash inputや署名subjectを読み替えず、AN1の新規生成・受理には別family、codec、profile版、会計版、移行cutを必要とする。未接続routeはAN1をdefault denyとする。

ユーザーが指定した検証方式はPanel＋Courtである。Groth16、VF1の全関係proof gate、succinct validity proofを採用候補・必須条件へ追加しない。AN1の正当性は、完全な局所関係、能動的な正直Panel、資料と反証の実手続き、およびnative Final gateを条件として論じる。

| 項目 | §0〜15のSWC1提案 | AN1の提案 |
| --- | --- | --- |
| 実行の束縛 | R_jobを実job写像へ作用させる | 事前登録Cと認証済みsigmaからsを作り、s全32byteを実tokenへ作用させる |
| Ticket | Original集合閉鎖後の未来R_ticketを使用 | 固定sと正規入力・出力・trace Rootから導出 |
| 元claimの固定 | R_ticketより前 | 必ずPanel選出イベントより前。Ticketを作ってから提出可能 |
| Panel | 受理集合・pool閉鎖後のR_panel | 同じ未知性・独立した後続選出条件を維持 |
| 計算検証 | Panel完全再実行と有界なCourt | 同じ方式を維持。最低承認数と正直者の存在を分ける |
| 物理計算の下界 | 未証明 | 未証明。入力が違うことから通常LLM費用を導かない |

AN1にはSWC1のR_ticketによる追加抽選を重ねない。0056のglobal D/controllerを一回だけ使い、AN1の固定thresholdへ変換する。別の抽選を追加する場合は別profileと解析を必要とする。

### 16.2 登録、試行、仕事、元claimの識別

登録文脈Cはchain／network、AN1 family・実行profileと版、epoch、実行beaconのscheme・鍵世代・round、owner、model root、program／実行規則、job root、固定長の登録nonceを含む。資金、beneficiary、唯一のtarget、会計版、全期限・資源計画は同じ登録slotへ認証して束縛する。具体的なfield順序・幅・codecはRFC-0059と当該profileに固定し、minerが自由なfieldや任意saltを付加できない。

~~~text
slot_id  = native登録時の一意な有資金試行slot
s        = H_attempt(Enc(C) || sigma)
x        = InputDerive(s, J)
(y, tau) = Exec(M, P, x)
T_AN1    = H_ticket(Enc(s, R_x, R_y, R_tau))
~~~

`slot_id`はsigmaより先にnativeで割り当て、同じ登録の再署名・再送・再包装で作り直さない。native登録は同じcanonical Cを高々一つのslotへ対応させる。別資金や別carrierで同じCを追加登録しても新しい試行を得ない。sは認証済みsigmaを得てから計算し、AttemptUseにsと登録slotの一意な対応を保存する。Cまたはslot_idの事前定義へ、未来のs、実入力Root、出力／trace Root、未来のPanel集合を循環して入れない。登録されたjob rootはchallenge挿入前のJへ束縛し、sigma後の入力・出力・traceの木はsへ束縛する。

AN1の実計算を表すComputationalWorkKeyには、canonicalなmodel/program、演算・停止規則、実入力x、依存、実行範囲を用いる。ownerやnonceが実際にxを変えることを認証して初めて別計算として扱う。モデル表示名、署名、relay、料金、説明文だけで計算を増やさない。native WorkIdとの対応は元の幅と意味を保持した認証adapterとして保存する。

BaseWorkKeyは元の依頼・モデル・J・計算意味を正規化した有用成果の会計キー、slot_id／sはAN1採掘試行のキーとする。新nonceでxが変わっても、元依頼を納品したことや新しい有用需要を満たしたことにはならない。有用成果の単発creditを設定した用途では、元のBaseWorkKey消費規則を別に適用する。反復サービスの有償契約、採掘機会、useful creditを同じ量として二重計上しない。任意programの意味的同値性をこのキーだけで判定できるとは主張しない。

元claimはslot_id、s、登録文脈への参照、R_x／R_y／R_tau、trace schemaと件数、元native claim、責任・供給契約に一意に結合する。OriginalIdと署名subjectは別々に定義し、署名bytesをOriginalIdやTicketへ入れない。Panelは固定済みの元claimを検査し、自分の再実行Rootを新しい基準Rootとして登録しない。

### 16.3 実tokenへの単射な写像

AN1の入力は、profileで固定したtoken ID列として次を構成する。

~~~text
[BOS, POW] || ByteToken(s[0]) || ... || ByteToken(s[31])
           || [SEP] || job_tokens
~~~

ByteTokenは256個のbyte値に対する単射な表であり、特別tokenと衝突しない。試行IDを文字列へ変換して再tokenizeする、複数byteをUNKへ潰す、chat templateでchallengeを落とす、自動切詰めで先頭を除くことを禁止する。job_tokensのroot・長さ・順序と挿入規則を認証する。総入力長が上限を超える場合は、登録時の規定に従って当該試行を受理しない。都合のよい短縮jobへ置換しない。

この写像の単射性は、異なるsが異なる正規入力になることを示す。出力yが異なること、モデルがchallengeへ高感度であること、全計算が独立であることは示さない。challengeは実入力を変えるため、元prompt単独の回答と同じ内容・品質になるとも主張しない。AN1対応サービスはこの変換と利用者向け出力の意味を明示する。

長い共通promptより前にchallengeを置く。これで直接共有できる同一prefixの範囲は限定されるが、異なる32byte値が長い共通prefixを持つこと、重み・KV・batch・特殊モデル・別アルゴリズムの共有は残る。cache使用自体は不正にしない。

### 16.4 二つの固定時点と元時計

1. **実行beacon前の登録freeze。** owner、登録nonce、model、J、profile、target、会計・責任・資源・期限を固定し、全候補集合を閉じる。一般公開前だけでなく、攻撃者が当該sigmaへ最初にアクセスする前でなければならない。
2. 登録された一つのroundのsigmaを認証し、sと実入力を導出する。別round・key・fallback seedへ変更しない。未取得はUnavailableとして元の有限終端へ進める。
3. 正規実行・全trace・Root・Ticketを構成する。提出を選別しても、登録済みで破棄した候補・落選候補・協力者の生成を費用分析から除外しない。
4. **Panel前のOriginal freeze。** Minerが署名した元claimを一回だけ固定し、受理集合、pool snapshot、抽選規則、元root、責任期間を閉じる。
5. それより後の未知の選出イベントでPanelを定め、実資料取得、全体の再実行・比較、局所化、Courtを進める。既知sigmaのdomain変更だけで未来R_panelの未知性を代替しない。
6. 全検証条件、未解決case、依存、元期限、用途別成熟と一回性を現在状態で再確認して権利を発行する。

120秒は0056のdifficulty controllerのepochとして維持する。登録freeze、実行、Original freeze、未来Panel、資料取得、全検査、反証の包含・裁定、成熟までの総時間が120秒であるとはしない。各区間の最遅合法処理を受理時に予約し、後のPanel選出・資料到着でclaim admission時刻や期限を取り直さない。

実行前の登録は有界なcontrol・保持・精算を予約し、Ticket適格のoriginal受付では0055の全工程容量を原子的に確保する。初期の署名・lookup・入力commit検査にも入口上限を設ける。偽Rootを安くhashしてTicketを選ぶこと自体は残り、DだけでそのDoS経路が閉じたとは扱わない。

### 16.5 canonical traceと局所反証の完全性

profileはInit、全命令、型・範囲、入力読出し、重みindex、参照する過去値、メモリ／KV更新、分岐、生成・停止、Endと出力結合を規定する。全ての依存を先行位置へ向け、各局所関係の最大fan-in・opening数・decode量・再計算量を有界にする。大きな演算を一つの「局所」命令としてNodeに渡し、全モデル保持や無上限の全推論へ退化させない。

Nodeはnativeの登録profileと元claimから期待するs、R_x／R_y／R_tau、program root、model rootを取り出す。命令・座標・認証済みtoken等から必要な重みindexと先行trace indexを導出し、その位置のopeningを照合する。同じmodelの別位置の重みが存在するだけでは足りない。

形式の整ったtraceが正規実行と異なるときは、最初に異なる位置の局所関係が反証点となる。これは全ての必要な境界と関係を列挙したprofileについての帰納法であり、数箇所の正しいCourtから全実行の正しさを導く規則ではない。誤った命令・record種別・範囲・位置・padding・Endが元Rootへ認証されていれば構造／計算の反証とする。元Rootに認証されない値・path・重みを提出した場合はInvalidEvidenceであり、それだけで元claimを不正と断定しない。

入力・出力・traceの木にはkind、s、要素数、位置、node高さ・位置などをcanonicalに束縛し、木形とpadding値を固定する。受付で必要な件数・長さを制限する。§6.1のSWC1 stage集合木をAN1実行trace木の仕様として流用しない。現在取得した資料bytesをrootへ再計算し、オブジェクトの自己申告root属性だけで認証済みとしない。

### 16.6 一人の正直PanelとFinal・一回消費

正直Panelが一人いるという条件は、能動的な担当、必要資料の取得、完全再実行、差分局所化、有効反証のFinal前の包含・裁定を含む。選出された鍵が一つ正直であるだけでは足りない。資料不足や取得前提が未成立の場合を、時間経過・ACK・多数positiveで成功へ変えない。各資料caseを客観的かつ有限に終結する手順が未確立なら、新profileはその状態を成功へ昇格させず未成立事項として残す。

最低一件の完全承認、全ての合法な検査・異議機会、case解消、依存・元期限等の既存Final条件を維持する。一人の署名だけで即Finalとせず、単独の有効反証を他の承認数・共有Court数で相殺しない。negative・資料要求・応答・終端は新D、未来seed、新EXEC creditを待たない。

SuccessFinal、slot_idおよび対応するs／ComputationalWorkKeyの未消費確認と消費、用途別credit／報酬発行、必要なBaseWorkKey消費を一つのnative atomic transitionへ接続する。slotの登録・Original固定・失敗終端の一回性と、成功権利発行の消費を別に記録する。別slotに同じ正しい実行を再包装しても追加creditを与えない。失敗後に同slotを別nonceや別seedで再開しない。通常のcancel／expiry／pruningは消費や失敗の履歴を消さない。

§9.2の同batch反証優先、§10の現在のEligibilitySnapshot再検査、用途別成熟・依存・取消をAN1の全consumerに適用する。成功Finalを経ない仮credit、旧route、repair、batch、snapshot/importからAN1の報酬・EXEC・weightを作れない。数学的に正しい過去のreplay結果があっても、現在の未解決case・expiry・失効・消費を代替しない。

正規reorgではslot、case、予約、nullifier、credit、provenanceを同じjournalでundo／replayする。一つの安定したnative履歴での原子的一回性を、全fork間の二重支払防止、外部決済の確定、DAG common-prefixの証明へ拡張しない。

### 16.7 費用定理と有用性を未証明のまま区別する

AN1で一意なのは、固定Cとsigmaに対する**正しい**実入力・実行・Ticketである。任意の偽Rootのhash照会、正しい値のcache再構成、外注、特殊入力の高速化を一意性命題から排除しない。正しい高速化をslashせず、客観的な関係違反・二重発行・署名責務違反だけを既存規則に従って裁く。

固定された当選確率pと、独立なTicket random oracleについて、相異なる正しいTicket入力の初回照会数が各実行でq以下なら、成功確率は1−(1−p)^q以下である。qは全関係者・未提出・破棄した候補と、Minerが未照会で提出した候補に対するNodeの初回照会を含む。事後の当選件数や公開した候補だけをqに使わない。適応的な選択には、現在のoracle返値を見る前の判定と、固定上限で停止させる比較実験が必要である。

前処理・cache・batch・外注・入力選別・待機等を含む同一費用モデルでq≤floor(B/c_min)を独立に示せた場合に限り、費用Bから当選・成功Finalの確率へ接続する。cryptoの失敗、Panel／資料／包含／gateの失敗、費用仮定の失敗を対象期間・全試行について定義し、根拠なく小さい誤差項と置かない。平均予算だけから実行ごとのfloor上限を導かない。

AN1では実入力がsigmaへ依存する。従って、以前のFULL1の「mask情報取得前にA/Bが固定される」構成の値生成下界bC≥KN、18Nの算術上界、有限体の補正式をAN1へ転用しない。完全Merkle木のhash照会下界を論じる場合も、AN1の正しいcanonical木・scope分離・初回Ticket以前の実照会履歴・古典ROM抽出を別に定義する。仮想extractorの照会をMinerの実費へ加えない。

通常LLMのGPU時間・電力・金銭費用に等しいc_minは未証明である。有限の実測、nonceの長さ、登録料、Court署名の数をその代わりにしない。既存の採択済み経済を遡及変更せず、新AN1の会計・consensus資格の有効化と、PoW成立の対外主張は明示した追加条件の判断へ分ける。

### 16.8 移行と受入試験

旧SWC1／CAD1／CPN1／native claimは元のfamily・codec・時計で再生する。AN1の未確定field、activation cut、pending claim移行、共有資本と跨profile重複排除を決めるまで、新nativeルートを有効化しない。過去試験のPASSをAN1のPASSへ読み替えない。

| ID | AN1で追加する受入条件 |
| --- | --- |
| AN1-W01 | owner／nonceを変えた古いclaim・openingを拒否し、同じsの再署名は新Ticketにしない |
| AN1-W02 | tokenizer正規化、UNK、challenge切詰め、特殊token衝突、上限長超過を拒否 |
| AN1-W03 | sigma取得後の登録追加・nonce変更、Panel選出後の元Root差替えを拒否 |
| AN1-W04 | 期待program／token／座標と異なる重みindexのopeningをInvalidEvidenceとして拒否 |
| AN1-W05 | 認証された最初の誤計算、構造違反、自由padding、End／出力不一致を反証として受理 |
| AN1-W06 | 四人のpositiveと一人の有効反証でも成功不可。資料不足・未解決caseのtimeout成功なし |
| AN1-W07 | 全consumerで同batch反証優先、現在適格性、slotとBaseWorkKeyの一回性を確認 |
| AN1-W08 | 正常claimの成功、最遅合法反証、expiry境界、再起動・reorg・pruningの整合を確認 |
| AN1-W09 | 正しいcache・特殊モデルの高速化を算術不正としない。費用下界の未証明状態を維持 |

小さな決定論的モデルの参照実装は、入出力・重み・trace・Court・一回性の接続を検査する資料である。学習済み本番LLM、全native route、ネットワークのDA／包含、最低Nodeの上限負荷、実1 BPS、PoW費用定理、DAG安全性を検証した結果とは区別する。
