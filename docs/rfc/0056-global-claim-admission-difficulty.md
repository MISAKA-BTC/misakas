# RFC-0056 — 全claim共通の受付難易度とcourt負荷連動制御

> **2026-11-18 PALW-AN1追補（DRAFT_NOT_ACTIVATED）:** [RFC-0059](0059-palw-an1-input-binding-and-panel-court.md) / [ADR-0227](../adr/0227-palw-an1-input-binding-and-panel-court.md) に基づく別profileの受付契約は本書§17による。owner・nonceを実入力へ束縛し、検証はPanelの完全再実行＋Courtを維持する。AN1のTicketと旧CAD1/SWC1の未来R_ticketを同じcodecへ混在させない。以下の旧提案・採択履歴・試験結果を遡及変更しない。

> **2026-11-17 監査対応改訂:** [RFC-0058](0058-staged-work-commitments-and-consensus-eligibility.md) / [ADR-0226](../adr/0226-staged-work-commitments-and-consensus-eligibility.md) と組み合わせる新SWC1 profileでは、本書§16を適用する。旧CAD1案の経済TrialSlotだけでは同じ計算の再抽選防止やPoW費用下界は保証しない。以前の46件の抽象試験記録は改訂前snapshotの証拠として保持し、この変更の合格記録に読み替えない。

- 日付: 2026-11-17（Asia/Tokyo）
- 状態: PROPOSED_DESIGN_NOT_ACTIVATED。ユーザー依頼による具体化済み設計案。native実装・性能測定・公開activationの完了ではない。
- 判断記録: [ADR-0224](../adr/0224-global-claim-admission-difficulty.md)
- 実装調査の基準: MISAKA-BTC/misakas main@2d8212b634880b0967fbf24b00b41936c2322c14。
- 関連規範: [RFC-0055](0055-global-admission-court-calendar-and-one-bps.md)の資源上界・全工程予約・Node内部上限・総搬送1 BPS。
- 新policy: ClaimAdmissionDifficultyV1（CAD1）。新wire tag・activation高さ・署名IDを本書だけで既存番号へ割り当てない。
- 主題別20 RFC／50 ADRを維持する。0056/0224は統合後の変更記録である。

## 0. 目的と前RFCとの関係

ユーザーは、courtをNodeへ提出するときの資源制限とは別に、「全種類の新規claimが高コストな検証責任を作る前に、共通の受付難易度で候補量を調整する」設計を求めた。本書の主題はD_admitの意味、判定位置、観測、更新、非迂回性である。

RFC-0055はhardな容量制限・全合法継続の予約を所有する。本書は同RFC §6の受付Dを独立して詳細化する。採択される新profileでは本書のCAD1をその一つのDとして使い、UAI1とCAD1を二重に掛けない。§6と異なる点は、canonical観測の定義、負荷の分母、消費済み費用とstock peak、観測不完全時の扱い、実行機会と結果commitの一回性である。旧runtimeのWやreceipt targetは置換しない。

court/claimを一件・三件・五件に固定しない。モデルはcanonical整数計算で完全一致するという前提を維持し、数値誤差を理由とする設計変更を行わない。未知modelの個別benchmark、モデル審査、モデル本体をNodeへ持たせる構成、計算正当性の暗号学的証明を追加しない。

結論は、boundedな入口、共通D、RFC-0055の予約、既存のFinal/経済条件を接続することである。Dは期待候補量の調整器であり、資源上限・正しさ・権利の証明ではない。

AN1に適用する場合は§17の登録・実入力導出・Ticket・元claim固定の順序を使う。Dの更新則と容量予約は共用するが、旧Presealへ未生成の入力/出力/traceを仮置きしたり、AN1合格後に旧ticketを追加で要求したりしない。

## 1. 確認した現状と不足

添付main ZIPのcommentとGitHub mainは同じ2d8212b6である。docs・consensus/core・consensus・root AGENTSの3,582 blobをGitHub treeと照合し、一致を確認した。詳細は[ソース監査](../maintenance/revisions/2026-11-17-admission-difficulty/SOURCE_AUDIT.md)。

| 現行mainの経路 | 確認したこと | 新Dとの違い |
| --- | --- | --- |
| Attempt | stateless・署名・pins・stateful資格/資本/重複の後にclass/W ticketを判定。PAV3ではその前にnative material検査がある | 既存ticketは存在する。全route共通のcourt負荷連動Dではなく、最前段の全コスト除去でもない |
| FreePrompt新規 | 形式・署名・仕事量・bond・価格・class/Panel容量等を検査してoriginal/Panel予約へ進む | Final後のreceipt ticketを新規受付の前段gateに使っていない |
| TFP1/TFO1/TFO2 | Plain/Funded original、実資金、actor/role、service bookと同checkpoint installがある | 予約が何もないわけではない。これだけで全新規routeが共通Dへ接続されたことにはならない |
| FP receipt | claimのFinalを先に要求し、その後にreceipt ticket等を判定 | 新規Panel/court責任を作る前の流入制御ではない |
| court block budget | processorとfoldに費用上限がある | 一blockの上限と、新規originalの将来責任の全体調整は別 |
| court capacity policy | presetのNoneと、明示profileで推定policyをarmするhookが併存 | ソースの存在・有効化可能性・実測・公開稼働は別 |

このmainでTFO3という独立carrierの実装は確認していない。TFO2内部のwire version 3をTFO3と呼ばない。TFO3や将来routeは本書の適用対象として列挙するが、接続済みとは扱わない。

## 2. 四つの制御の境界

| 名称 | 入力と判定時点 | 保持する意味 |
| --- | --- | --- |
| D_admit | canonicalなclaim責任負荷と適格候補の観測。新originalの重い処理前 | 全モデル・全新規routeに一つの受付target |
| 既存W_work / class target | 既存の生成率・仕事/class条件 | 既存適用版の必要条件として維持する |
| FP receipt target / P_m / CCU / 生成権 | 既存のFinal・認証・資本・経済契約 | 新Dの合格だけで発行・消費できない |
| 全carrier合計1 BPS | RFC-0055の共通外側permitとschedule | 搬送の持続目標。Dから別のpermitを発行しない |

Dを通った候補も予約に失敗し得る。受理されたoriginalもFinal前は仕事報酬・creditを取得しない。Dを使って1秒一claimのFinal、一claim一block、modelごとの1 BPSを約束しない。

## 3. 入口と適用対象

### 3.1 新規責任の判定

送信者が付けたkind文字列ではなく、native状態遷移が新original、新しい検証責任、新たな資本試行枠を作るかで分類する。

| route/action | CAD1 |
| --- | --- |
| Attempt、FreePrompt、non-TIR、TFP1 Plain、TFO1/TFO2 Funded | 新profileの新originalは必須 |
| TFO3・将来の新規route | 同gateへの接続までdefault deny |
| 複数originalのbatch | 各originalに独立のTrialSlot/Preseal/判定と予約。batch一枚のticketで迂回不可 |
| evaluation・無報酬claim | 公開で新責任を作るなら必須。無報酬を免除理由にしない。無効化済み機能は開放しない |
| 既受理claimの適格notice・反証・応答・終端・保持 | 元の予約内で継続。新D・新seed・新ExecCreditは不要 |
| 既存権利のredeem、通常取引、BASE/control | 新originalを内包しない限り新Dの対象外。既存資格と独自上限を維持 |
| quote/preview | 高価な処理を無制限に呼べない。下記の入口予算と段階を適用 |

未知originalを救済と名乗るpacketは免除しない。既受理originalのmodel/program/shape/責任を拡大する操作をcontinuationへ偽装しない。旧署名の再生・IBDは当時の規則で検証し、現在のDを遡及適用しない。

### 3.2 判定順序

1. 固定長header、版、総bytes/件数/展開上界をallocation前に確認し、Node全体のingress leaseを取得。
2. 有界な認証・登録identity・資本参照・TrialSlot一回性・簡単なdescriptor規格を確認。
3. 事前に資金とcontrol容量を確保したPresealを参照し、凍結D・唯一のseedでticketを判定。
4. eligibleで、原commitに一致する資料を持つ候補だけ、予算内のintrinsic geometry/actor graph/plannerへ進む。
5. 現canonical prefixの実資金・全合法継続・flow/stock・期限をRFC-0055で予約。
6. original、資金、book、actor責任、一回性、cursorを同checkpointでcommitする。

Nodeに重い証拠・全graphを先にdecodeさせ、最後にDを確認する順序を禁止する。新wire/RPCはcompact Presealとeligible後のbounded original payloadを分離する。既存entryの末尾へif文を足しただけではこの順序を満たさない。

D前にも署名・hash・lookupの費用は存在する。その部分は共通の固定ingress/control予算で有界化する。D不合格者、未認証packet、mempool洪水、fork候補までDで消せるとは主張しない。

無料の簡易quoteはboundedな型/範囲確認までとする。全工程plannerを呼ぶ詳細quoteはeligibleな資金付きPresealと署名済み費用scopeへ束縛し、一度の入力/basisに対するcacheを用いる。basis変更後の再quoteも実費・回数相当の資源を消費する。quote結果自体は予約権ではない。

## 4. 合意状態と一回性

### 4.1 必須状態

| 状態 | 必須内容 |
| --- | --- |
| AdmissionPolicy | codec/domain、epoch/clock、D範囲・gain・閾値、capacity/checker/料金profile、meter/集計規則、上界 |
| TargetState[e] | 公表D、policy、収集期間、closed root、唯一のseed anchor/値、ready資料root、cursor、状態 |
| Preseal | TrialSlotKey、既存WorkId、producer/bond、model/program/weights、入力/生成条件、出力/trace commit、shape、target、資金/同意/署名期限 |
| TrialUse | 資本outpoint・既存実行機会/sequence、消費状態、費用/返還、終端と高水位 |
| CandidateDisposition | Ineligible、Accepted、NoCapacity、InvalidQualification、UnsupportedShape、InsufficientFunds、MissingMaterial、ExpiredUnvisited等 |
| EpochObservation | canonical cut、集合/seed/cursor、qualifying demand、congestion、coverage、消費flow、stock peak、現在future book |
| DifficultyState | 公表schedule、low_run、最後に処理したboundary、bootstrap/世代 |

表示用RPC counterを合意stateへ昇格しない。全fieldをnetwork/policy/target/generationへ束縛し、reorgでは同じjournalで戻す。

### 4.2 TrialSlotKeyと結果commitを分ける

既存WorkIdの意味を変更しない。その外側へ、既存の経済的実行機会と資本を一度だけ使用するTrialSlotKeyを置く。

~~~text
TrialSlotKey = H256(LP(domain_slot_text) || LP(network_id) ||
                   u64le(policy_generation) || NR("capital-outpoint", version, native_bytes) ||
                   u64le(canonical_opportunity_sequence))
PresealId   = H256(domain_preseal || CanonicalPresealFields)
~~~

domainと全fieldのencodingは§5による。canonical_opportunity_sequenceは自由なnonceではなく、native資本台帳が既存資格・費用・lockとともに一回だけ割り当てる実行機会である。クライアントが要求したsequenceを無条件で採用しない。別sequenceを取得するなら別trialの実費・資金・資本予算を満たす。

TrialUseの元キーである(network、既存資本outpoint、既存機会sequence)はpolicy世代をまたいで一回性を保持し、TrialSlotKeyのdomain更新で元の経済的機会を復活させない。資本の移転/rolloverも元の消費済み機会・残責任との認証関係を保ち、outpoint名の入替だけで同じ機会を再発行しない。TrialSlotKeyへoutput/trace/署名bytes/route名/model aliasを入れて別試行を作らない。最初のPresealがそのslotの入力・結果・model・targetを束縛し、後の差替えを拒否する。

同一slotの再署名、carrier交換、任意nonce、別route、別model名、偽output/traceへの変更は追加抽選にならない。旧CAD1案では同じ入力の別の有資金実行機会を別trialとしていた。**SWC1ではこれだけで別の採掘仕事にしない。** §16のComputationalWorkKeyを追加し、同じcanonical計算記述には別資本sequenceでも新ticket・新仕事信用を与えない。同じ利用者promptから新しい実jobが一意に導出される場合や、別のサービス支払いとは区別する。

別鍵・別modelを増やしても同じ資本を二重lockできず、全model共通の資本・入口・保持予算は増えない。異なる実資本で支払う別trialまでSybilとして同一視する身元審査は追加しない。

pruning後も資本sequence高水位や認証済みnullifier summaryを保持し、古いslotを再利用可能にしない。元の責任・保存・索引費用はRFC-0055に計上する。

## 5. ticketの正確な定義

H256はSHA-256。CAD1が新たに作るdigest・policy_hash・future_seedは32 bytesとする。既存のmodel/work/資本/trace等のIDは、それぞれのnative幅と意味を保持し、32 bytesへ切り詰めない。数値は指定幅little-endian、LP(bytes)=u32le(length)||bytesとする。epoch/sequence/policy_generationはu64、Dはu128容器の正整数で上限2^120−1。署名bytesはPresealIdから除き、署名subjectはPresealIdと料金/同意scopeを含む。重複fieldの任意並べ替え、無視される余剰field、非canonical整数を拒否する。実際のwire番号は別の登録で割り当てる。

既存の型はNR(tag, version, native_bytes)=LP(UTF8(tag))||u16le(version)||LP(native_bytes)で包む。tagは下表とTrialSlotKey式のcapital-outpointというASCII固定文字列、versionとinner codecは共通policyで許可した版だけとし、元のcanonical bytesを検査する。各LPの最大長、nested count/深さ、総展開量はprofile必須欄で、読み込み/allocation前に検査する。任意のJSONや表示名をcanonical bytesとして採用しない。

CanonicalPresealFieldsは下記順序の連結とする。すべて必須で、省略・別順序・未知の末尾fieldを認めない。optionalなnative要素がある型は、そのnative版の唯一のabsence encodingを用いる。

| 順 | fieldの符号化 |
| --- | --- |
| 1〜4 | u16le(1)、LP(network_id)、policy_hash[32]、u64le(target_epoch) |
| 5 | TrialSlotKey[32] |
| 6〜8 | NR("work",...)、NR("producer",...)、NR("bond",...) |
| 9〜11 | NR("model",...)、NR("program",...)、NR("weights",...) |
| 12〜15 | NR("input",...)、NR("conditions",...)、NR("output",...)、NR("trace",...) |
| 16〜19 | NR("shape",...)、NR("funding",...)、NR("consent",...)、NR("deadline",...) |
| 20 | NR("route-semantics",...) |

fundingは実outpoint・料金版・金額/上限・署名済みscope、consentは必要な権限と同意commit、deadlineは元のclock/pin・signature期限・有効期間を束縛する。route-semanticsはpolicyへ登録された意味版で、未対応adapterを有効化するものではない。model等の参照は登録されたnative identityを使用する。相互の一致と資金有効性も別途検査する。

domain_slot、domain_preseal、domain_ticketはそれぞれ以下のUTF-8 bytesをLPしたものとする。

~~~text
MISAKA/PALW/CAD1/SLOT
MISAKA/PALW/CAD1/PRESEAL
MISAKA/PALW/CAD1/TICKET

ticket = uint256_be(H256(
    LP(domain_ticket_text) || LP(network_id) || policy_hash ||
    u64le(target_epoch) || PresealId || future_seed
))
T(D) = floor((2^256 - 1) / D)
eligible = ticket <= T(D)
rank = (ticket, PresealId)
~~~

network_idのbytesと長さ上界はactivationでpinする。seed/root/D/profileを提出者が選び直せない。policy_hashにはTrialSlot、観測、更新則と全codecを含める。異なる世代・networkのticketを再利用しない。

一様なticketという前提の下で、通過確率は正確には次である。

$$
p(D)=\frac{\lfloor(2^{256}-1)/D\rfloor+1}{2^{256}}\simeq\frac{1}{D}.
$$

model数、tier、P_m、CCUをこの式へ再乗算しない。既存routeのW等の資格条件は別に維持する。D合格は高価な推論を実行した証拠ではなく、虚偽commitもticketに通り得る。

SWC1では、上記旧CAD1 ticketを新profileの[0058 §5](0058-staged-work-commitments-and-consensus-eligibility.md)へ接続する。Dと更新式は共用し、一つの受付gateとして使う。WorkKey／OriginalId／freeze／未来R_ticketを束縛する新codecと旧codecを区別し、旧署名・旧targetのhashを遡及変更しない。

AN1では上記の旧ticket式を使用せず、§17.2のAN1式と厳密なtarget変換を使用する。既存のCAD1/SWC1 claimをAN1として解釈し直す規則ではない。

## 6. 120秒epochと未知seed

RateClock、origin、canonical close frontierはRFC-0055の版付きclock契約を使う。120秒を120 DAAや120個の受理blockで代用しない。

| 時点 | target eについて行うこと |
| --- | --- |
| Start(e−2)の収集開始処理より前 | 同boundaryの先行処理でD_eとpolicyを公表・固定 |
| epoch e−2 | bounded Presealを受理し、実費・資本・control/保持を確保 |
| Start(e−1) | 集合をfreeze。追加・削除・target移動を禁止 |
| epoch e−1のfreeze後、seed cutまで | 既存beaconの一意なanchorから未知seedを導出。eligible資料を有界に準備 |
| Start(e) | seedとcanonical資料readinessを閉じ、予約されたprefix処理を開始 |
| epoch e | rank順の有限cursorでintrinsic適合と実予約を判定 |
| Start(e+1) | 未処理を明示してepochを閉じ、観測を確定。D_(e+3)の更新に利用 |

原claimのDAA notice/hard期限は元の意味を維持する。最新資料readiness・検証・包含を期限へ収められなければ新規受理しない。

epoch 0/1の新規admissionは空。epoch 0でtarget 2の収集を始める前にD_2=D_initial、low_run=0を設定する。D_initialは範囲内のactivation必須欄で、未知値を自動補完しない。既受理courtとBASE/controlはbootstrap中も継続する。

Observation[0]とObservation[1]はbootstrap-unavailableかつincompleteとする。b=1/2ではD_initialを保持しlow_run=0とし、未開始の二targetを完全なidleとして緩和に使わない。

seedはfreeze後の指定beaconからnetwork/policy/集合root/targetへ束縛して得る。指定anchorの失敗時に「次の当たりseed」を選ばない。

この要件は、既存native beaconが必要な未知性・偏り耐性を満たすとの確認ではない。SWC1では0058 §6により、閉鎖前の敵対者による復元、当選済み実行からのheader variant、最後の寄与者のwithholdingを含めて検査する。R_job・R_ticket・R_panelは因果stageごとに未予測であることを要求し、既知seedのdomain分離だけで代用しない。

共通clock/control profileは、seed確定からeligible payloadの検査・伝播・canonical包含までのguard G_ready_msを定める。0<G_ready_ms<120000かつ予約された最大対象量を運べる場合だけ当該一epoch計画を有効とする。seed cut=Start(e)−G_ready_msとし、seedのcanonical確定はfreeze後かつcutより前でなければならない。guardに収まらないprofileは受付不能で、推測の短いguardを採用しない。

seed cutを満たさないtargetはunavailableとし、Start(e)直前に届いたseedで個別候補をMissingMaterial/責任違反にしない。新規受付をせず、元slotと費用/返還を有限に処分する。失敗targetを無料で次へ持ち越さない。

## 7. Presealの資金・処理容量・資料

Presealは無料・無制限のmempool登録ではない。canonical化の前に、有界なheader・認証、実資金出力と同意、既存資格、TrialSlot、共通shape規格を検査する。

署名済み入口実費を徴収し、claim原資の上界と必要資本を他trialから分離してlockする。名目bondを支出可能escrowと同一視しない。実費・lock量は共通の料金/責任profileと認証shapeから導き、modelごとの審査・登録試行料へ転用しない。

同時に、Presealの記録・凍結・hash/順位索引・ticket・候補訪問・精算・保持に必要な最悪control費用を予約する。候補集合を無制限に受け入れてからsortしない。各item・各訪問の最小費用を非ゼロとし、予約可能数は残資源から導く。未知seedを都合よく低いeligible数と予想してcontrol予約を削らない。

eligible後のoriginal資料はPresealIdのcommitに一致するbounded payloadとしてcanonicalに記録し、Start(e)でready集合を閉じる。各Nodeの私的な受信時刻をreadiness判定へ使わない。資料受付・保持・伝播費も予約対象とする。資料がない候補はMissingMaterialとして有限に処分し、他候補を無期限に待たせない。pre-Final資料供給・provider/Panel準備はRFC-0055の責任契約も満たす。

本段階の入口/control予算と、§9のD観測に使うclaim責任予算を区別する。未認証・不正・不合格packetが消費した入口費用は消えないが、それを「適格なcourt需要」としてDへ直接注入しない。

## 8. 決定的な候補処理

closed root・D・seed・ready rootを固定し、rankで一巡する。処理は保存cursorからの次prefixに限る。候補の先取り・再順序付け・同epoch内の再抽選を禁止する。

eligibilityは有界なhash判定である。高コストなintrinsic検査・plannerを実行する候補はeligibleなものだけに限定する。全候補のhash/索引/最悪訪問費と保存は§7で既に有界でなければならない。

各候補を次に分類する。

| 結果 | 必要条件 | Dの混雑観測 |
| --- | --- | --- |
| Accepted | ticket、資金・資格・shape・元期限、全工程予約が成立 | qualified_demand=true |
| NoCapacity | 資金・資格・shapeが有効で、固定の空claim-bookなら同じprofile/期限で全工程を収容でき、既受理予約との競合で不成立 | qualified_demand=true、known_congestion=true |
| InvalidQualification / UnsupportedShape / InsufficientFunds | 内在的な不適合・資金不足等 | demand/congestionへ加えない |
| MissingMaterial / intrinsic deadline failure | 必要資料がない、空bookでも期限/依存が不成立等 | NoCapacityへ偽装しない |
| Ineligible | ticket不合格 | 重いplanner・Panel・court義務を作らない |
| ExpiredUnvisited | 有限cursorで未処理のままtarget終了 | 観測不完全。需要ゼロとして緩和しない |

NoCapacityの空book比較は、固定system予約を除去しない。同じcandidate geometry・readiness・clock・資金を使い、資源競合と内在的不能を区別する。planner自体の限界・未処理・実装エラーをNoCapacityと報告しない。

大きい候補が不成立でも、後続の小さい候補を検査できる。結果とcursor更新を同checkpointで保存し、例外時はそのbatchをrollbackする。再送は同じ結果を返し、支払や抽選を重複させない。

### 8.1 進行の省略と観測不足

§7のcontrol予約は、slot/区間ごとに処理すべき次prefixと最大訪問workを決める。指定serviceを含む正規carrierのnative foldは、予約量内のAdmissionAdvanceを実行する。producerが候補を恣意的に選ぶ自由枠にはしない。close処理・cursor証跡も予算化する。

carrier欠落・clock飛越し・必要な進行の欠落があれば、未観測epochや未処理尾部を空需要と合成しない。観測はincompleteとし、§10でlowを禁止する。全資料が揃い、全control予約が守られる条件でだけ完了を主張する。

canonical化されなかったRPC提出の検閲は履歴から観測できない。Dや次prefix規則だけで無条件の包含・公平性を保証したとはしない。

## 9. Dへ入力する負荷

### 9.1 分母

資源rと区間Iについて、共通qualification済みのCから、固定system、最低通常取引、ingress/control、復旧の先行枠S_fixedを引く。

$$
C_A(I,r)=C(I,r)-S_{\rm fixed}(I,r).
$$

これはclaim責任へ割り当てられる供給であり、hostの空きRAMや瞬間CPU使用率ではない。負のC_Aを許さない。必要次元のC_Aがゼロなら、そのprofileは新規claimを収容できず、Dの緩和で解決しない。

S_fixedは基礎control/ingress等の先行枠であり、claim固有のnotice・court・終端・保持はR_claim側で計上する。同じ費用を両方へ重ねて引かない。preseal/controlの費用とclaim責任flowの費用区分もpolicyへ固定する。

外側permitを1本releaseする費用と、同じcarrier内でco-packできるclaim payload/検証容量は別次元にする。外側1 BPSを全量控除した同じ件数次元へclaimを再加算して、常に容量ゼロにしない。搬送の実可否はRFC-0055のpermit・service bookも別途確認する。

### 9.2 三つのcanonical指標

全比率はceil(1024*numerator/denominator)、checked整数で求める。正の容量超過は帳簿/サービス不整合として新規停止し、wrapやclipで正当化しない。分母・分子ともゼロの次元は比率ゼロ。正の需要に供給ゼロならunavailableとして扱う。

~~~text
U_future = 現在のbookに残る全claim責任について、
           有限horizonの各120秒区間flowとstock占有期間の最大比率
U_spent  = 直前完了epochにcanonical cost meterで消費済みとなった
           claim責任flowの各次元の比率
U_stock  = 直前完了epochのcanonical claim関連live-state/保持stockの最大比率
U        = max(U_future, U_spent, U_stock)
~~~

U_futureには未開始の全合法継続・保持・終端も含む。U_spentは実CPU秒ではなく、規定のcost debitを使う。U_stockは認証stateから導く占有上界で、local workerの終了時刻やRSSを使わない。観測区間と共通capacity generationを固定する。

三つを足して同じ仕事を二重計上しない。未実行の未来予約の過去の一瞬のpeakを無条件には加えない。解放済み予約は現在bookから外れるが、実費・live-state peak・発生済みNoCapacityは履歴に残る。cancel後の残責任はbookから消さない。

これにより、epoch内で重い処理を完了・解放したあと、境界の残予約だけを見て低負荷と誤判定することを防ぐ。固定1 BPS維持費や不正packetの費用だけでDを上げない。

### 9.3 需要・混雑・完全性

qualified_demandは、凍結集合のeligible候補をnativeに訪問し、identity/一回性、既存route資格、必要資金、対応shape、内在的な全工程/期限適合を確認したものが一件以上あること。算術結果全体の正しさが既に証明された意味ではない。

known_congestionは、その候補に§8の正規NoCapacityが一件以上記録されたこと。受付成功数だけで算出せず、全拒否でも正しいNoCapacityなら混雑とする。

observation_completeは、canonical cutと対象集合・seed・ready rootが確定し、全recordが検査済みのterminal dispositionになり、必要なcontrol進行・費用集計・stock peakが認証され、未処理の穴がないこと。完全な空集合と、候補はあるがeligibleゼロはcompleteになり得る。

unavailableは、profile未適合/retired、共通service不足がcanonicalに確定、必要clock/close根拠不成立、seed不成立等による新規停止である。各Nodeだけの復元失敗・I/O遅延・CPU/RAM不足を合意上のunavailableへ投票しない。そのNodeは検証・同期・新規受付を停止/保留し、架空の観測を発行しない。

この停止状態はactive policyと認証済み共通stateから導く。提出者がunknown/disabled profileや供給ゼロのshapeを要求しただけでは全体のunavailableを立てず、その候補を規格/資格不適合として拒否する。正の残責任に供給ゼロという§9.2の異常も、自己申告需要ではなく認証bookと有効profileの不整合を指す。

canonical cutはStart(b)でepoch b−1の費用/terminalを確定した直後、bの新規処理を始める前とする。観測の根拠root・disposition・counterを保存し、後からcancel/再送でそのclose結果を消さない。

## 10. 整数difficulty controller

Dは1〜2^120−1、中間演算はchecked uint256。Uの基数は1024、high閾値768、low閾値256、増加ceil(9D/8)、緩和floor(8D/9)、連続low回数3をRFC-0055から継承する。これは制御規範の初期値で、機器性能の測定値ではない。

epoch b開始時、b>=1でObservation[b−1]を使い、直前公表のD_(b+1)からD_(b+2)を計算する。b/b+1のDは変更しない。boundaryとtargetの二重処理・逆順・欠けた履歴の偽装を拒否する。

~~~text
high = qualified_demand AND (U >= 768 OR known_congestion)
low  = observation_complete AND U <= 256
       AND NOT known_congestion AND NOT unavailable

if unavailable:
    next_D = previous_published_D
    low_run = 0
elif high:
    next_D = min(D_max, ceil(9 * previous_published_D / 8))
    low_run = 0
elif low:
    low_run = min(3, low_run + 1)
    if low_run == 3:
        next_D = max(1, floor(8 * previous_published_D / 9))
        low_run = 0
    else:
        next_D = previous_published_D
else:
    next_D = previous_published_D
    low_run = 0
publish D_(b+2) = next_D
~~~

incompleteでも確定済みの適格high根拠があれば増加できる。incompleteをlowにしてはいけない。seed不成立等のunavailableはhighより優先して保持する。

| 観測 | 挙動 |
| --- | --- |
| 適格需要があり予約/消費/stockがhigh | Dを上げる |
| 規格内・有資金候補が既存予約でNoCapacity | 受理数ゼロでもDを上げる |
| 完全な低負荷epochが連続 | 三回目でDを下げる |
| 完全な空集合・eligibleゼロ、かつ低負荷 | 意図したidle decayとして緩和可能 |
| eligible未処理尾部、観測穴 | low禁止。有効highがなければ保持しlow_run reset |
| 全拒否が不正・資金/資格不足・unsupportedのみ | 混雑として数えない |
| 供給不能・seed不成立 | D保持、low_run reset。既受理救済を継続 |
| 私的な大量RPC・host負荷 | D入力にはしない |

D=1のceil(9D/8)は2となる。小さい整数Dを含め「実倍率は常に12.5%以内」と表現しない。比率・閾値の変更は新policyで事前公表し、凍結済みtargetへ遡及適用しない。

epoch k中の負荷がD_(k+3)へ効くまで、RateClock上で約240〜360秒の遅れがある。継続した倍率更新の収束も即時ではない。急増時のNode保護は、Dの追従を待たずRFC-0055のhard budgetとbounded ingressで行う。閉ループ安定性や目標処理量を式だけで実証したとはしない。

## 11. 費用・失効・キャンセル

| 処分 | 実費・lock・一回性 |
| --- | --- |
| 同じPreseal再送 | canonical debitは一回。追加抽選なし。再受信のlocal入口費はboundedに処理 |
| Ineligible | 既に行った受付/凍結/hash等の実費は負担。未使用資金は有限の精算予約で返還。元slotは再抽選不可 |
| NoCapacity / MissingMaterial / ExpiredUnvisited | 正規理由を保存し、実費を精算。未使用lockは契約どおり有限解放。同targetで順序を変えて再挑戦しない |
| seed不成立 | 新規originalを作らず、使用済み実費以外を契約に従い返還。別seedへの無料carryなし |
| Accepted | 資金・TrialSlot・原commitをoriginalとRFC-0055の全工程bookへ原子的に引き継ぐ |
| cancel | freeze済み集合・既消費費用・dispositionを削除しない。未受理を終結させるだけ。受理後の残責任は0055で維持 |
| reorg | 集合/seed/D/cursor/費用/slot/予約を同じjournalでundo/replay。正規にundoされた消費は新canonical履歴で一回性を再判定。新世代へ旧worker結果を混ぜない |

receipt失敗やNoCapacity、seed障害を算術的不正としてslashしない。費用請求と有罪判定を分ける。返還・終了・保持削除にも容量を予約し、大量失効で無制限の後処理を作らない。

結果が虚偽でも形式・資格・資金を満たした候補を、この入口だけでは排除できない。有料の負荷操作は残り得る。実費・同時資本lock・有限trial/control予算・全工程責任・後続Panel/courtによって費用と被害を制限する。無資金の送信件数をそのまま難化票にはしない。

通常のcancel/再送/pruningでは一回性を復活させないが、正規reorgによるundoとは区別する。forkをまたいで絶対に別seedを引けない保証をCAD1だけで主張せず、既存beacon・fork choice・finalityの前提へ従う。全forkの受信/検査費用はRFC-0055のlocal上限でも守る。

## 12. 多数modelとNode保護の帰結

1万/5万modelでもD stateは共通で、model数そのものをdifficulty入力にしない。global ingress、TrialSlot/資本、Preseal保持、claim予約の予算をmodelごとに増設しない。

未知modelのshapeは共通checker/kernel/cost規則で判定する。Dや共通形式への適合をモデル品質・配布状況・保有者数の審査に使わない。Nodeは全modelの重みや全推論を要求されない。

registry・索引・cost counter・closed集合・nullifierはstockを消費する。各epoch/各blockで全modelを走査せず、認証索引・差分counter・有限calendarを使う。U_future/stock peakの集計、freeze、順位付け、snapshot/IBD/GC自体も予算内にする。

D_maxでも、全ticketが偶然合格しても、予約不可のoriginalは受理しない。全claimの入口保護は、D判定の位置、bounded codec、実費/資本、予約、実装のleaseを合わせて成立する。CPU/OS/ディスク等の任意障害まで無停止とは主張しない。

## 13. 接続・移行・実装完了条件

共通gateはroute adapterの上位に置く。OriginalAdmissionV1のFunded分岐だけへ追加しない。Attempt header、plain/non-TIR FP、TFP1、TFO1/2、evaluation、batch、quote、将来TFO3を別々に追跡する。

新規の型・cost meter・観測root・journal/snapshotを未有効profileで実装し、以下を満たしてからactivationを判定する。

1. 全新規routeとquoteが、重い処理前の同一D gateを通る。未接続routeと旧carrier downgradeは新規責任を作れない。
2. 初期trace・TrialSlot・Preseal・資金・原子的予約が一致し、失敗でpartial claim/escrow/bookを残さない。
3. 必要な共通capacity/checker/料金/clock/beacon profileが存在し、各上界が測定・構造検査と整合する。
4. 因果順序・最遅資料・元期限・実包含と全actor救済がRFC-0055で成立する。
5. 高D・seed停止・新規受付停止中にも、既受理notice/反証/応答/終端が継続する。
6. 実LLMの同一結果からStudio送信、受付、Panel全体検証、正常Final、不正反証、精算・既存権利消費まで接続する。
7. 大量の未認証・不合格・有資金候補を混ぜ、DB/保持/復旧と共存する最低NodeでCPU/RAM/queueが上界内にある。
8. W、receipt、D、carrierという別loopを同時に動かし、未完了分母、backlog、空slot、待ち時間を含め安定性と利用率を評価する。

移行時は旧original責任を元期限で保護する。旧版targetをdrainし、policy世代をまたぐPreseal・seedの付け替えを禁止する。既に公表したDを緩く再初期化する無料抽選を作らず、新しいcontrollerの初期値・低負荷counter reset・開始targetをactivation recordへ明記する。旧WorkId、署名、Final、P_m、tier、既存責任を遡及変更しない。

## 14. 反例と必須受入試験

| ID | 入力・反例 | 必須結果 |
| --- | --- | --- |
| D01 | 大量の未認証RPC、不正署名、未資金preseal | Dのdemand/congestionを増やさず、入口予算内で拒否 |
| D02 | Uがhighだがeligible候補は資金/資格/shape不備 | 安価な偽demandでDを上げない |
| D03 | 有資格・有資金候補がNoCapacityで全拒否 | 受理数ゼロを低需要と誤判定せずhigh |
| D04 | epoch内に重い処理完了、境界の残予約はゼロ | 消費済みflow/stock peakにより偽lowを防ぐ |
| D05 | 一瞬の未来予約を合法に解放 | 未来の過去peakだけで難化しない。実費/NoCapacity/残責任は保持 |
| D06 | 固定BASE/外側1 BPSだけ稼働 | claim負荷のhighへ自動変換しない。co-pack容量を二重控除しない |
| D07 | 同じslotの再署名/route/model alias/結果差替え | 追加ticketを作れない |
| D08 | 別資本sequenceで同じcanonical計算を再提出 | 旧CAD1の経済trialと区別。SWC1ではWorkUseにより追加の仕事ticket・credit不可 |
| D09 | seed後追加、target移動、別seed、freeze後取消 | 集合/一回性を変更できない |
| D10 | eligible尾部未処理、batch省略、clock飛越し | completeなidleと扱わずlow禁止 |
| D11 | 低負荷の完全空集合、eligibleゼロ | 連続lowによる意図した緩和 |
| D12 | bootstrap、D=1/D_max、threshold直上/直下、整数丸め | 一意の結果、既公表target不変、overflowなし |
| D13 | 同じclock/service cut・外部state変化の位置を保つ入力順/合法batch分割 | 同じrank/prefixと結果。異なる時刻や予約解放を挟む分割まで同結果とはしない |
| D14 | 大候補不成立と小候補の後続 | 固定件数/one-winnerにせず残vectorへfit |
| D15 | 将来route/評価/Plain/複数original batchで迂回 | 新規責任はすべて共通gate |
| D16 | D最大・seed不成立・新規停止 | 既受理救済は元予約で進む。未知original偽装は拒否 |
| D17 | cancel/失効/reorg/再送 | 費用保存・返還一回。通常処分ではslot復活不可、正規reorgは同checkpointをundoし新canonicalで再判定 |
| D18 | model IDを1万/5万へ増加 | D/容量/同一資本枠は増えない。実registry性能は別に測定 |
| D19 | quote/previewや不合格payloadを大量投入 | 重いplanner・decodeへの無制限経路なし |
| D20 | snapshot欠損とhost遅延 | ローカル保留/復旧。架空の共通混雑/空需要を発行しない |

## 15. レビュー・検証・パラメーター

120,000ms epoch、D範囲、1024基数、768/256閾値、9/8と8/9、連続low3、b+2適用を本設計案で使用する。court件数、model数、最低CPU/RAMを固定値に変換しない。共通の有限profile欄とD_initialが未設定なら新規有効化不可である。

[ADR-0224](../adr/0224-global-claim-admission-difficulty.md) / [実装接続表](../design/palw/integration/claim-admission-difficulty-implementation-plan-2026-11-17.md) / [改訂と検証](../maintenance/revisions/2026-11-17-admission-difficulty/README.md) / [レビュー](../maintenance/revisions/2026-11-17-admission-difficulty/REVIEW.md)。

抽象モデルはticketから観測・Dまでを接続して反例を検査する。前RFC-0055の30試験を、この新経路の合格証拠に流用しない。文書整合・抽象モデル・native・実機・activationを別々に記録する。

## 16. SWC1への監査対応改訂

本節は新SWC1 profileに限る優先規則である。既存CAD1署名、旧claim、改訂前の試験結果を変更しない。設計の追加判断はADR-0226、完全な定義はRFC-0058が所有する。

| 論点 | 変更後の契約 |
| --- | --- |
| 経済TrialSlotと仕事 | 資本機会の一回性に加え、実行意味から導くComputationalWorkKeyの一回性を検査。別資本・別alias・別policy包装で同じ計算を新しい仕事にしない |
| 元結果 | owner／beneficiaryをR_job前に固定。全OriginalをR_ticket前に一回だけ固定し、任意の偽rootを後から正しいrootへ差し替えられない |
| seed | JOB登録閉鎖→R_job→実job→Original閉鎖→R_ticket→受理集合閉鎖→R_panel。各stageに唯一のanchor・期限・失敗終端を割当てる |
| 時計 | presealの入口／保持／精算予約と、ticket後のfull court受理を分離。native admission aは全工程book install時に一回確定し、Panel選出や遅い資料到着で取り直さない |
| D | 120秒epoch、共通D、観測・整数更新、既受理救済の免除を維持。新profileで二重Dを掛けない |
| 合意効果 | 偽rootがticketに通っても、その未検証仕事をDAA work標本・safe weight・成功報酬・EXEC creditにしない。成熟と用途別現在状態を確認 |
| 失敗・reorg | 通常の失効・反証・cancelでWorkUseを復活させない。canonical undoは全関連stateを同時復元。fork間の無料再抽選防止にはstable freezeの別の根拠が必要 |
| 証明 | ticket非増幅と固定後抽選までの契約。本人の全推論実施や非償却性・DAG全体安全性は別の未証明課題 |

既存コードのC_attempt_zeroにはtrace／output／execution rootが含まれる。「rootが欠落しているため位置情報だけで先読み可能」との監査解釈は、現在mainへの確認済み攻撃として採用しない。一方、安価な偽root、未admit headerの効果、beaconの候補選別は0058の別監査項目にする。

この改訂の検査は[2026-11-17-work-binding](../maintenance/revisions/2026-11-17-work-binding/README.md)で記録する。改訂前46件の結果やsource hashを上書きしない。

## 17. PALW-AN1の実入力束縛と受付契約

本節は[RFC-0059](0059-palw-an1-input-binding-and-panel-court.md)のAN1 familyを対象とする**DRAFT_NOT_ACTIVATED**の追補である。旧CAD1/SWC1の§§4〜8、16と異なる登録・Ticket順序は本節の範囲で明示的に分岐させる。Panel＋Court、共通D、全工程予約、120秒difficulty epoch、全carrier合計1 BPSを維持する。暗号学的な計算validity proofを新設しない。

### 17.1 二つの固定時点と実入力

AN1は次の順序を固定する。

| 段階 | 固定・導出する内容 | 追加してはいけない処理 |
| --- | --- | --- |
| 登録・候補集合の閉鎖 | nativeのslot_id、owner/報酬帰属、nonce、model root、job root、実行profile、chain/epoch、beacon scheme/key/round、当該slotのtarget snapshot・期限・資金/容量 | 実行beacon取得後のnonce/model/job/profile選び直し、無資金での無制限候補登録 |
| 実行beacon確定 | 登録した唯一の認証済みsigmaからsを導出 | 別round、別seed、署名乱数、任意fallbackによる再抽選 |
| 入力・実行 | x=InputDerive(s,J)、正規実行の出力yと全trace、canonical R_x/R_y/R_tauを生成 | tokenizerやchat templateでchallengeを落とす、自由paddingや未拘束metadataでRootを変える |
| 元claim固定・受付 | Minerの署名済み元claim、s、三Root、target、slot、責任scopeを一回だけ固定。AN1 Ticketと全工程予約を検査 | 後のPanelの計算結果からRootを埋める、同slotのRoot差替え |
| Panel選出 | 固定済み元claim・受理対象集合・pool・選出規則を、後続の選出イベントへ束縛 | Panelを見てclaimを選び直す、実行beaconを単にhashし直して未来の未知性とする |
| 検証・Final | 完全再実行、必要資料、全scopeの比較、適格Court・Qの解消、原期限、現在の資格と一回性 | Ticket合格や局所一致だけで成功Final/weight/creditを作る |

登録で固定するJは元jobのcanonical token列又はその認証可能なcommitmentであり、まだ存在しないxのRootをCへ循環的に要求しない。sは登録文脈Cとsigmaから導出し、R_xはその後のInputDeriveから決まる。実入力のtoken表、BOS/POW/SEP、256個の相異なるbyte token ID、順序、長さ上限、生成長、停止・出力選択、整数演算意味をprofileへ固定する。challengeを先頭の実tokenへ入れ、文字列化後の正規化・切詰め・UNK置換を認めない。

候補の凍結時刻は一般公開時刻だけを基準にせず、攻撃者が当該実行beaconへ先にアクセスする場合も含む。凍結前漏洩・選別・withholdingの扱いは実beaconの安全性条件として残す。登録済み候補の一部だけを実行・提出することまで禁止した、又はその費用を証明したとは記載しない。

slotはnative資本機会へ結合する。Cの同一性と登録済みbeacon参照が同じ試行を、別資本・別route・model aliasだけで複数slotへ複製しない。モデル名のaliasは登録済みのcanonical model identityへ正規化する。異なるowner/nonceが実入力を変える別の登録候補である場合は、各候補が既存資本・料金・global control予算を負担する。身元審査やモデル品質審査は追加しない。

### 17.2 TicketとDの一回適用

AN1の意味上の式は次である。正確なdomain、型、長さ、native IDの幅、認証済みsigmaのcodecはRFC-0059の固定版に従う。

~~~text
s        = H_attempt(Enc(C, sigma))
x        = InputDerive(s, J)
(y, tau) = Exec_M,P(x)
ticket   = uint256_be(H_ticket(Enc(s, R_x, R_y, R_tau)))
theta(D_snapshot) = floor((2^256 - 1) / D_snapshot) + 1
eligible = ticket < theta(D_snapshot)
~~~

これは旧§5のticket <= T(D)と同じ整数の通過境界を、AN1のstrict比較へ写したものである。D=1ではtheta=2^256となり、全ての256bit ticketが通る。thetaと中間演算は2^256を正確に扱える容器又は同値の境界処理を用い、256bitの0へwrapしない。Dとcontrollerの共通値は維持し、UAI1、CAD1、AN1を独立な三回の抽選として掛け合わせない。

target snapshotと受付期間はnonce候補登録前に公表され、各登録slotへ一意に結合される。Nodeは登録stateから期待targetを導き、payload、現在のより易しいD、他epochのtargetを採用しない。期限切れ、落選、NoCapacity、資料不足を理由として同じs/slotを新targetへ無料で持ち越さない。

署名bytes、Panelの署名・nonce・人数、Court内容、relay、carrier、コメント、輸送chunkの再包装をAN1 Ticketへ含めない。Panel用の後続乱数は選出とCourt義務に使い、正しいAN1 Ticketを作り直す材料にしない。旧SWC1のR_ticketはAN1の追加salt又は必須抽選として使用しない。

AN1の候補処理順は、正規なticket整数とnative slot_idのcanonical bytesによる `(ticket, slot_id)` の辞書順とする。同一slotの複数候補を順位へ入れない。ready集合の閉鎖と有限cursorは§8の規則を継承し、旧PresealId又は署名bytesを新しいtie-breakerとして選ばせない。

固定C、sigma、決定論的profileに対応する**正しい**三RootとTicketは一つである。ただし、任意の偽Rootを使う安価なhash照会まで禁止できるわけではない。D合格を「通常LLM費用を支払った」「実行が正しい」「検証済み」と扱わない。

### 17.3 受付とPanel/Court容量の予約

AN1登録のcompact recordと、実行後の署名済み元claimは別の版付きrecordとする。旧Presealの固定fieldへ未生成Rootのゼロ値を入れて意味を変えない。登録時には資金付きの候補/control/保持/精算容量、元claim受理時には完全再実行・資料取得・局所化・全合法Court・反証包含・終端・保持の全工程を予約する。

Nodeの順序は、長さ/版/個数上限→登録と認証beacon/slot/期待入力→署名と元claim一回性→固定targetのTicket→有界なdescriptor/planner→実資金と全工程予約→原子的受付、とする。重い資料展開・全graph構成を先に行わない。正規入力の構築・Root検査・token索引・登録照合も費用ゼロとは扱わず、共通profileの有限予算へ含める。

元claimのtrace/output個数、位置、期限、target等の整数fieldは、native受付とCourtで同じcanonicalな型・幅・範囲を使う。整数と数値比較が等しい浮動小数点表現、boolean、暗黙cast、範囲外値を拒否する。例えば整数のtrace個数と同値に比較される`6153.0`をnativeが受け付け、Courtだけが整数型として拒否する不整合を許さない。canonical性は元claim固定・Ticket評価の前に検査し、Finalでも現在の登録/claimとの整合を再検査する。

元claimの署名と固定はPanel選出より先であり、accepted set/poolも後続選出イベントより先に閉じる。元claimの到着後に自由なPanel roundを指定できるAPI、未固定RootをPanelへ計算させて埋めるAPI、署名だけ先に受け付け後からRootを書き換えるAPIを認めない。

偽Root候補はTicketへ通り得るため、global ingress、候補数の有限control予約、資金・原資、失敗時費用、同slot一回固定、全工程容量のhard上限が必要である。TicketだけをDoS防御の証明にしない。InvalidEvidenceや不正packetの私的到着件数を、適格なclaim需要の追加票へ変換しない。既存のcanonical U_future/U_spent/U_stock、qualified_demand、NoCapacity、観測不完全の規則を維持する。

正直なPanelのcold取得、全再実行、比較、局所反証作成、期限前の包含・裁定までの余裕を計画する。少数の局所sampleが一致しただけで全再実行費用を予約から外さない。資料不足・構造不正・未解決Qは成功を止める。新規受付が停止しても、既受理の適格notice/反証/応答/終端は新D、新しいAN1実行、新beacon、新ExecCreditを要求せず元予約で進める。

120秒はDの制御epochであり、登録からFinalまでの上限ではない。元claimのnotice/hard期限は全工程book install時のnative起点から一回だけ導き、Panel選出・再送・遅い資料到着で再開しない。全carrier合計1 BPSは外側搬送の目標で、AN1の一試行一秒Finalやモデル別枠を意味しない。

AN1の共通TimingPlanは登録閉鎖、実行beaconの最遅確定、元claim/ready集合の閉鎖、予約・受理集合の閉鎖、Panel選出の最早/最遅時点、検査/異議/包含の余裕、元hard期限とのClockBridgeを固定する。登録閉鎖→実行beacon→元claim閉鎖→Panel選出の順序を守り、全段階が原期限へ収まることを確認する。§6の旧CAD1のepoch表をそのままAN1の実行時間保証へ流用しない。TimingPlanが未設定又は資源上限内で実行不能なら、そのAN1 profileの新規受付を有効化しない。

### 17.4 Final・credit・再送の一回性

AN1では、登録したslot_idと同一登録試行のindexを、元claimの固定、AcceptedOrigin、Final資格、支払いへ一貫して結合する。受付時の元claim固定と、成功時のslot消費を別の状態遷移として扱う。元claim固定後に再送・cancel・pruning・policy世代変更で新しい元claimを作らない。

成功Finalには、当該元claimについてPanel完全承認の規定条件、全体検査/異議機会、必要資料、未解決Qなし、有効反証なし、原期限、現在のnative資格・依存・資金・成熟条件が必要である。多数のpositiveやTicket合格で、一人の有効な反証を無効化しない。最初のpositiveを受け取った直後に残る検査・異議機会を打ち切らない。

同一canonical commitでは、適格notice/反証・取消を先に反映し、その後に現在状態と未消費slotを再検査して、SuccessFinal、slot消費、報酬/credit/weight、必要なonce markerとjournalを原子的に更新する。並行workerが同じ未消費状態を見ても、両方をcommitしない。再送は既存receiptを返し、追加権利を作らない。単に同じ生成テキストが得られたという理由で、別の正規試行を同じslotと扱わない。

通常の失効やcancelで消費・元claim固定を復活させない。正規reorgは関連stateと発行/消費を同じjournalでundo/replayし、別branchの古いworker結果を再利用しない。単一の安定したnative履歴の一回性を、forkをまたぐ不可逆決済の定理に拡張しない。

### 17.5 費用と認定量の境界

AN1はowner/nonceから別の実入力とcommitmentを要求するが、正しい出力・traceを作る最安費用を証明しない。正しいcache再構成、前処理、batch、専用アルゴリズム、外注を、単に安いという理由でslash対象にしない。全観測記録が同じ分布なら、その記録だけに基づく支払い/slashでは生成方法の費用差を除去できない。

Ticketの独立ROモデルで正しい初回照会数qの上限から当選確率を評価できても、予算Bからq<=floor(B/c_min)を得るには別の費用定理が必要である。AN1の入力はs/sigmaに依存するため、入力をmask前に固定するFULL1のbC>=KNをそのままAN1へ流用しない。完全Merkle木のhash下界を主張する場合も、実際のtree encoding、scope分離、抽出時点、前処理/協力者を含む総照会、例外確率を別途証明する。

共通Dは容量制御であり、通常LLM FLOPs、GPU秒、電力又は金銭費用の認証器ではない。認定量・単位・報酬/credit/weightへの写像・会計版・適用範囲は登録前に固定し、未取得の費用下界を既存報酬式の安全性根拠として追加しない。既存の採択済み経済値を本追補だけで遡及変更しない。

### 17.6 移行と追加受入条件

AN1は新profile/codec/domainとして登録する。旧CAD1/CPN1/SWC1の署名済みclaimと継続責任は元規則・元期限でdrainする。新AN1 claimを旧codec/別routeへdowngradeして入力束縛やPanel/Court gateを迂回できないよう、全carrier・batch・repair・import経路を追跡する。未設定の実beacon、InputDerive、token表、program/trace schema、資源上界、native adapter、accounting/移行は有効化を阻止する。

| ID | 反例・境界 | 必須結果 |
| --- | --- | --- |
| A56-01 | 実行beacon取得後にnonce/model/job/profileを変更 | 登録context不一致として拒否 |
| A56-02 | 同じ登録試行を別slot/資本/routeへ再包装 | 追加の元claim・Ticket・creditを作らない |
| A56-03 | D=1、旧target直上/直下、より易しい後続D | theta=2^256を正確に扱い、固定snapshot以外を拒否 |
| A56-04 | AN1 Ticketへ旧R_ticket、署名又はPanel nonceを追加 | 別codecとして拒否し、二重抽選にしない |
| A56-05 | 正規input rootを含む偽traceがTicketに合格 | 入口の残余リスクとして有界処理。未検証仕事をweight/creditにしない |
| A56-06 | 元claim未固定のままPanel計算を開始し、後からRootを補完 | 新AN1の順序違反として拒否 |
| A56-07 | 反証とFinal、又は同slotの二重Finalが競合 | 反証を先に反映し、発行/消費は原子的に最大一回 |
| A56-08 | 正しい最適化trace、資料不足、seed停止 | 正しい最適化は算術不正にしない。未検証・不足を成功や自動有罪へ変えない |
| A56-09 | trace/output個数等を数値が等しいfloat又はbooleanへ変更 | 元claim受付前に拒否。nativeとCourtの整数型・幅・範囲を一致させる |

今回の内容・検証範囲は[2026-11-18 AN1改訂記録](../maintenance/revisions/2026-11-18-an1/README.md)へ分離する。既存46件、SWC1の12件、ユーザーが報告するAN1試験数を、当該版の独立実行結果と混同しない。

