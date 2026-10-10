# RFC-0015 — Panel=0: 固定Panelからpermissionless verifierとobjective fraud proofへ移行する

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**トークンの表示名は Misaka、ticker は BILI。** `misaka` 系アドレスプレフィックスと既存のprotocol/CLI/API識別子は維持する（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


* **Status:** Revised Draft / deferred implementation design, 2026-10-10後続改定。モデル入手への合意不介入、固定identity、条件付きG14、拘束miner元本によるcoinbase配分と個別bond上限を採用する。旧Seeder/TRDC/FPR gateは撤回。配分式・経済評価・実装・activationは未完了。
* **Source baseline:** MISAKA-BTC/misakas public main `43f0bcb362d37cba79414940f3fdd368d40d3377`（2026-10-07確認）。
* **Hard prerequisite:** [RFC-0014][R14]のDispute Completenessとpublic non-seat prosecutionが、対象ネットワークで本RFCの全reward-bearing class/profileについて完全に成立していること。RFC14の文書作成・mergeだけでは満たさない。
* **Activation:** 前提達成後の別の協調upgrade。提案fence名 `palw_panel_free_v1` は設計用の未割当名で、既存Paramsのfieldではない。activation DAA、wire tag、署名domainはここでは割り当てない。
* **Decision:** 固定Panelの選出・必須receipt quorum・seat依存のlicensingを新経路の合意要件から外し、public authenticated material、permissionless verifier、objective court、producer担保、challenge windowで検証・処罰を行う。§8.3のclass非依存Block Cap / Reward Cap / Final Weight Capと共通DAA拘束を新modeの経済規則とする。
* **Not activated by:** RFC14/RFC15のmerge、seat/quorum定数の変更、テスト用小modelの成功、operatorのみのdrill、代役captureを使うcourtテスト、将来のcompact proofの提案。
* **Normative language:** MUST / MUST NOTは新mode有効化後の規則。以下のphase/mode/descriptor名は設計用で、現行Rust型の存在や有効化を意味しない。

## 0. 日本語での決定と有効化禁止条件

**Panel=0は可能な設計である。ただし「検証をなくす」のではなく、
「固定Panelによる検証をなくして、permissionless verifier + objective fraud proofへ完全移行する」。**

Panel=0は、claimごとに事前選出したPanel seatを合意上要求しないという意味である。
full nodeの決定的validation、LLM計算の検査、DA、court、producer/public challengerのbond、
不正へのslashは残る。watchdogや監査サービスは任意のpublic参加者として継続できる。

**次の条件が完全に成立するまで、本RFCのPanel=0 modeを有効にしてはならない。**

> **Panel外の普通のpublic bondが、producerの秘密状態を使わず、
> public authenticated materialだけから不正をlocalizeして客観的convictionまで進める。**

「秘密状態」はproducerのheap、鍵、内部cache、debug endpoint、攻撃注入instanceや
genesis operator専用の非公開materialを含む。
producerが開示を拒否する場合は、正直なverifierが適格なon-chain要求とdeadlineから
客観的DA/default処理へ進める必要がある。materialがないのに計算不正のexact proofが
作れるとは主張しない。

前提の達成は§1のrelease gateで確認する。**現在のコードには未解決のgapがあるため、
この文書は現行MISAKAがPanel=0で安全に稼働できるという宣言ではない。**

## 1. 必須のactivation gate: Public Fraud Prosecution Complete

### 1.1 G14の内容

以下のG14は正しい登録modelを保有/取得できたverifierの条件付き検証能力である。
全保有者の配布拒否に対する取得保証は撤回する。claim固有の有限証拠開示とmodel取得は別scopeであり、
非公開modelの監視・経済安全性は別途未立証として評価する（ADR177 D2/D7）。

仮称 `G14_PUBLIC_FRAUD_PROSECUTION_COMPLETE` はrelease criterionであり、
単一の合意boolean、operatorの承認署名、Panel投票ではない。
次の全条件を満たす検証記録がMUST必要である。

1. **Ordinary public entry:** genesis operatorでも既存seatでもないpublic bondが、
   公開された同じ資格・maturity・担保・fee規則で参加できる。
2. **Fresh verifier:** claim公開後に起動した独立node/clientが、開始cutoffまでに
   class/plan/job/input/model materialを取得して検査できる。
3. **Authenticated material only:** 取得するbytesはclaim/program/artifact/rootへ束縛される。
   producerやoperatorの秘密状態・非公開API・liar instanceを使わない。
4. **Complete localization:** 対象planの故障を宣言されたbytes/work/rounds内で
   bounded exact terminalへ落とせる。whole dense captureや巨大モデル全体の緊急replayへ逃げない。
5. **Objective outcome:** 開示された不正にはexact conviction、未開示には適格なDA/default、
   honest claim/誤ったchallengeには棄却という、決定的な結果を得られる。
6. **Permissionless filing:** ExecutorRefuted、必要DA unit、interactive court、
   RFC14のnon-seat予約・共有progressを使える。別bondのcourt先取りで排除されない。
7. **Actual chain path:** RPC取得から署名・fee・mempool/carrier inclusion・state fold・
   conviction/slash・Final阻止まで、実node E2Eを完結する。
8. **Recovery and resources:** restart/IBD/reorg/重複proof/退出/担保保持/資源上限と
   worst-case deadlineを検証する。
9. **Post-commit challenge completeness:** [RFC07 Part VI](0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)のimmutable policyをbindし、statement/prover messageを各challenge前に固定する。sourceは独立に有効化・検証された将来のcanonical PALW workであり、fresh outsiderがqualification/Final/settlement、同じsource順序・seed・queries・各GKR roundを公開materialから再構成できる。grinding、abort/retry、withholding、reorg、資源・保持期限のboundsとexact escalationが全許可profileで成立する。
10. **Immutable identity / conditional prosecution / non-interference（2026-10-10後続改定）:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)とRFC14 §16を適用する。モデル入手・Seeder・Torrent/PoR/全量取得監査/lease/TRDC/FPRを合意条件にしない。claimは固定Model ID/root/specをbindし、正しいmodelを得た外部verifierがclaim固有証拠からlocalize/convictできることを試験する。モデル未取得だけでqualification/Final/weightを止めず、閉鎖時に実効検出pが0となる場合も経済評価へ含める。モデル全量をcourt要求で取得する経路を作らない。

この追加条件はG14の一部であり、別のPanel=0 activation shortcutではない。RegisteredDormant / ConformancePassedと
ActiveRewardableをRFC11 §17に従って分け、semantic/constraint/court/public-material/resource completenessをsamplingで代替しない。
source不足やtimeoutをheartbeat/BASE-0/EXEC hash、committee署名、local RNGで補わず、pending/既定deadlineとして扱う。
chain livenessは現行mainのまま進む。既存のsame-seed watcher非独立性、positive checks、DA/defaultとconvictionの区別を維持する。

このgateは本RFCで許可する**全reward-bearing class、kernel、layout、task/context profile**
に適用する。canonical 8k held/fusedの三gap [C12]、整合したgarbage trace、borrowed trace、
state/routing/checkpoint/output不正を含む。
小geometryやoperator-onlyの成功を、別の最大形状の成功として数えない。

future kernelを全て実装する義務ではないが、未対応profileをreward-bearingにしてはならない。
既存の広告・class envelopeをこっそり縮小してgateを通ったことにしない。
Candidate/研究用profileと、合意上報酬を得るprofileを明確に表示する。

### 1.2 gate未達の取り扱い

G14が未達、記録欠落、前提profileで失敗、または資源/期限未確定なら:

* 本RFCのfenceはdormantのままにする。activation heightを設定しない。
* 現行のPanel/receipt/担保規則を維持する。
* Panel=0の「暫定有効化」「検証省略での試験運用」「一部operatorを信頼する例外」を設けない。
* failing classを含むmodeを、別のclassでの成功やcompact proof構想で有効化しない。

G14がPASSになっても、自動的にPanel=0にはしない。
Panelを外す新lifecycle、担保・reward/weight、DAとwatcherの実運用、codec/fingerprint、
移行・client表示という本RFC固有のgateも別に満たす必要がある。

## 2. 検証する役割と安全性の前提

| 役割 | 新modeでの責任 |
| --- | --- |
| Producer | 有効classで計算し、固定Model ID/root/specをclaim/input/output/state/witnessへbindする。必要なclaim materialの開示・保持、担保/feeを負担する。モデルの常時Seeder義務はなく共有は任意 |
| Permissionless verifier / watchdog | 任意のpublic参加者として独立検査する。不一致をlocalizeし、必要materialを要求してproof/courtへ進む |
| Full node | object・binding・合意規則と受理された客観証拠を決定的に検証し、state・weight・報酬・slashを更新する |
| Optional off-chain model peer / claim-evidence provider | modelの保存・Torrent/mirror共有は任意で、chainのSeeder登録・lease・監査・default対象にしない。claim固有証拠を別に提供する場合は、その有限なscopeと規範的責任だけを扱う |
| Optional verification service | 監視・検査・material保持のサービスを提供する。契約や報告に固定Panelの合意権限を与えない |

「誰でも検査できる」と「実際に誰かが検査する」は異なる。
本RFCのoptimistic modeは、**各claimについて少なくとも1人のcapable honest verifierが
期限内に実際に検査・追及すること**を安全性の運用前提にする。

chainは形式上有効なrootや署名だけから、計算不正を自動的に察知するわけではない。
watcherが後から参加できること、bondが存在すること、bountyが高いことだけでは、
honestな監視の存在を数学的に保証しない。
モデル取得・検査の開始に間に合わない参加者は、そのclaimの安全性の前提に数えない。

Panel多数派の正直さを要求しない一方で、chain進行、bounded inclusion、
認証DA、commitment/署名の安全性、checkerのsoundness、資源・監視能力という仮定は残る。
full node全体が各巨大LLMをreplayする設計にはしない。

## 3. なぜ現行のseats/quorumを0にするだけでは成立しないか

baselineの既存codeには以下の依存がある。

| 現行依存 | 新modeで必要な変更 |
| --- | --- |
| PALW_V2_PANEL_SEATS=5 / QUORUM=3 [C01] | 旧定数をゼロにせず、別のsettlement modeとversionを定義 |
| Provisional → PanelBound → ReceiptLicensed → Final [C02] | 固定Panelを経ないChallengeable経路を追加 |
| seat membership・receipt quorum検証 [C03] | 新modeでは必須licensing条件から外す。旧objectでは維持 |
| Final/settlementにbasis_k下限がある [C04] | signer数を根拠にしない新しいeligibilityと担保計算 |
| Valid signer lock、served mask、Panel recordによるescrow release [C05]–[C06] | producer中心のreservation/escrowと独立したDA/監査経済 |
| court開始がReceiptLicensedを要求する [C07] | 新Challengeable状態のchallenge surfaceとdeadlineを認める |
| conviction funnelが既存phaseを処理する [C08] | 新modeを明示的に扱い、旧claimの意味は変更しない |
| operator filerの計算proof不足 [C11] | RFC14の共通filerで完全に閉じてからmodeを有効化 |
| operator-anchorによるPanel binding [C13] | 新modeにはPanel bindingを要求しない。producer/public bondの公開資格は別に満たす |

空receiptを「全員Valid」、quorum=0を「検査済み」、空Panelのserved maskを
「DA義務充足」と扱う実装は禁止する。
`basis_k=1`を偽装したり、既存の`max(basis_k,2)`で署名者不在の経済を正当化したりしない。

## 4. 選択する新modeとclaim lifecycle

### 4.1 OptimisticPublicVerification

本RFCの基本modeは仮称 `OptimisticPublicVerification` とする。
class/claim identityはmodeとversionをbindし、network policyが対象classをadmitする。
producerがmodel metadataだけで自由に「検査を減らすmode」を選ぶことはできない。

~~~text
Producer: claim + public bond + authenticated commitments/material
    ↓ public admission + producer reservation + fee
Challengeable
    ↓
permissionless verification / fixed challenge window
    ├─ accepted exact fraud proof → Convicted → void + producer slash
    ├─ accepted interactive pursuit → Disputed
    │      ├─ exact guilty → Convicted
    │      ├─ required material withheld → DA/default
    │      └─ upheld / dismissed → resume original bounded clock
    └─ window closed + no unresolved accepted dispute + DA/accounting satisfied
           ↓
         Final (assurance = optimistic public verification)
~~~

Challengeableやassurance fieldは提案概念であり、既存enumを変更済みという意味ではない。
固定PanelのPanelBound/ReceiptLicensedを偽造してこの経路へ入れない。

### 4.2 Challengeableのadmission

少なくとも以下を合意上検査する。

* RFC14のDisputeAdmissible class/plan/kernel、対象mode、固定artifact/job/layout/task/context identity。
* RFC14 §16.4の新`ModelRewardEligible`。semantic/constraint/court planと有効rulesetを検査し、modelの取得可否を入力にしない。
* producer authorization、public bond資格、最大同時claims/aggregate gain、必要な責任担保。
* model資本snapshot、network/model/個別bond予算、claim受理時の最大権利予約と共通DAA時計。
* claim固有input/state/trace/output/witnessの認証manifest、許可unit/累積scope、有限な開示・retention義務。
* court/localizer資源、challenge window・cutoff・hard deadline、feeと固有work/slice identity。

モデルのpeer数・download成功票・URL到達性・PoR/Full Fetch/leaseはadmission/Finalに要求しない。
claim固有証拠をhash告知だけで充足とせず、適格要求/defaultと算術的convictionを区別する。
model取得目的のweight-file/range・全量列挙/反復要求は受理しない。
許可witnessから実際にterminalへ到達する方法とmodel operandの認証を確定する（ADR177 D2）。
model提供拒否だけではcourt/void/Slash/weight停止を起動しない。旧FPR/CHALLENGED gateは撤回する。

### 4.3 正の検査receiptの位置づけ

新modeでは固定PanelのValid receiptをChallengeable/Finalの必須条件にしない。
任意verifierの検査報告やサービスreceiptを受け取れても、
その人数・多数決をclaimの真偽の最終判定に使わない。

これはRFC14 §7.3 / RFC11・ADR0171 [C14]–[C15] の
required positive evidence/receipt路線を全ネットワークで削除する変更ではない。
**新modeについてだけ、そのFinal条件をoptimistic admission・DA・window・court・担保へ
置き換える、明示的な別upgrade**である。

旧mode、旧claim、新しいnode-verifiable proofを必須にする将来modeには、
それぞれのpositive evidence条件を維持する。
RFC15が未有効の間は、RFC14の既存・提案条件を緩めない。

## 5. public materialと後から参加するverifier

G14を満たした共通fraud filer [R14] を、roleによらず利用する。
claim公開後、普通のpublic bondが同じpublic RPC/DAから:

~~~text
claim/plan解決
    → canonical input・model/artifact・root情報を取得
    → 独立した検査
    → mismatch
    → 階層的localization
    → committed witness取得 / on-chain開示要求
    → bounded exact court / ExecutorRefuted
    → canonical conviction
~~~

既存Panelのprivate cacheやoperatorの特別なreadがないと証拠を作れないclassは許可しない。
少なくとも、完全なgarbage、整合した偽trace、別jobのtrace流用、
単一stepとstate/checkpoint/boundary/出力の嘘を閉じる。

public discoveryはoperator producerも含む全対象claimを提供し、
Challengeable時刻、開始cutoff、Final floor、hard deadline、material/root、
未処理sessionと提出済みproofを返す。
active disputeの回答を他のverifierが再利用できるようにし、
sessionの先取りで別人の有効proofを拒否しない。

cold model/artifact取得、最大profileの検査、DA開示とcarrier inclusionを時間budgetに含める。
claim公開後に起動したfresh verifierが実際に間に合うことを実測する。
現行capを超えるwhole captureやliar instanceへの依存が残れば、gateはFAILである [C12]。
モデル自体は任意のoff-chain配布で取得し、chainが取得結果を資格判定へ使うことはない。
全peer停止や公開拒否だけでは新claim/reward/Final weightを止めない。モデル取得はG14試験の条件であり、
公開参加による資本増加と閉鎖時の検出率を別の経済評価へ含める。claim固有の有限witness/court責任は維持する。

## 6. challenge window、Final、censorshipとspam

### 6.1 時計

Challengeableの受理時点でbase windowとabsolute hard deadlineを固定する。
必須claim固有materialの依存で必要期間を満たせない場合はChallengeableへadmitしない、
または定義されたDA/defaultで終了する。足りない時間を「watcherが速ければよい」で補わない。

RFC14のnon-seat追及予約を利用し、適格なinteractive disputeだけが有界の延長を得る。
別bond、別session、再送、別site、重複DA要求でhard deadlineを再開始しない。
適格direct objective proofは、他bondのopen courtだけを理由に排除しない。

~~~text
start + B_cold_material + B_check + B_localize + B_disclose
      + B_court + B_carrier + B_reorg_slack
    <= dispute_hard_deadline
~~~

実装で一部作業が並行できる場合も、使用した計測条件とworst-case boundを記録する。
秒とDAAを無条件に同一視しない。

### 6.2 Final predicate

新modeのFinalは最低でも:

~~~text
class/plan/mode admitted under completed G14
AND producer commitment/authorization/reservation valid
AND permitted claim-specific evidence / court / retention obligations satisfied
AND network + model coinbase allocation + producer bond budgets satisfied (§8.5)
AND base challenge window + class verification horizon expired
AND no unresolved accepted pursuit / required court
AND unique work, accounting and bounded-liability conditions satisfied
AND class-independent bond claim / block / reward / Final-weight accounting valid (§8.3)
~~~

有罪または適格DA/defaultでvoidされたclaimはFinalへ進めない。
honest claimへの誤ったchallengeは、deadline・deposit規則により有界で棄却する。
「challengeがない」ことを「正しさが証明された」と表示しない。

### 6.3 inclusion仮定と資源独占

proofを必要期限までcanonical chainへ載せられることが安全性の前提である。
検閲耐性を新しいPanel/委員会の署名で代替しない。
carrier lane、fee、proof処理budget、複数producerへの配送と
inclusion/reorg slackを具体化し、honest proofが遅延する敵対的node試験を行う。

unlimited censorshipや無制限の敵対的資金でも必ず安全とは主張しない。
depositだけで全Sybilを防いだとはしない。
shared progress、direct-proofの扱い、session/claim aggregate capにより、
正当な追及を排除せず総負荷を有界にする条件を審査する。
chainがそのinclusion仮定を満たせないなら、Panel=0のrelease gateを通さない。

## 7. 通常時の検査、soundnessと監視市場

### 7.1 実際に監視する主体

public watcherはpublic bond/feeと資源条件を満たして参加し、
固定の選出・seat assignment・Valid quorumを必要としない。
professional watchdog、利用者、自分の利益を守るサービス、独立node等が監視できる。

ただし監視市場を開いただけでは各claimのhonest検査を保証しない。
全reward-bearing profileで検査能力・cold material取得・通常時費用、
attack-induced load、実際の稼働と離脱を測定する。
監視されないclaimがwindowを通過し得ることは、optimistic modeの明示的な限界である。

nodeは形式・root shape・authorizationを検査する。
提出されたfraud proof/court evidenceは全nodeが検証する。
大きなclaimを誰かが検査する前に、chain自身が「怪しい」と判別する機能を仮定しない。

### 7.2 確率的checker

通常検査にADR0171の承認済みconstraint suiteを使う場合、
honest watcherが仕様どおり実行しても未検出の残余確率がある [C15]。
未検出のclaimに、exact courtが存在するだけで有罪proofが自動発生するわけではない。

全claimの見逃しには、監視不在、suiteの条件付きcheck error、
challenge/binding/DA/inclusion/resource仮定の破綻を別々に考慮する。
`epsilon_check <= 2^-128`という検査器の目標を、
監視不在も含めたネットワーク全体の無条件安全性へ読み替えない。

複数watcherが同じseedやtranscriptを検査しただけで、
誤り確率を人数分のべき乗にしてはならない。
post-commit randomness・全constraint coverage・independent checksの条件を満たす必要がある。

### 7.3 検査サービスの報酬

bountyは不正検出の誘因だが、攻撃がない通常時の監視費用も扱う。
任意サービス契約、公開検査fee、DA保持fee等を設計できる。
合意上の固定Panelへの必須支払やサービス多数決を新しい名前で復活させない。

任意のaudit completion報告を「honestに全計算を検査した証明」とみなさない。
報酬条件は客観的に検証可能な仕事/証拠か、合意外の明示的なサービス契約にする。
サービス運営者の名前・内部ログ・主張をfraud裁定の信頼根拠にしない。

## 8. Panel担保を外した後の経済設計

### 8.1 新しいproducer reservation

現行のValid signer lockやbasis_k [C04]–[C06] は新modeで存在しない。
その損失吸収能力を除いたまま、既存のproducer reservationだけで安全と仮定しない。

新modeはclaimと同時claimsの最大fraud gain、unsettled work/weight、
報酬・escrow・state/取引への影響、court/DAコストを評価し、
**producer中心のlocked collateralと未払escrowでどこまで回収できるか**
を新しい経済規則として固定する。

* 検査前の全escrow解放・完全work creditを禁止する。
* 予約と担保退出のclockはhard deadline・retention/liability期間に整合させる。
* concurrent claimsで同じ担保を重複して損失吸収に使わない。
* nonexistent signer lockをkで割る計算を持ち込まない。
* nominal slashと実際のcollectedを区別する。
* fork-choice操作や外部の経済損失は、単一claimのrewardだけをgainとして隠さない。

bond比率・escrow解放時点・総未決済creditの値は、release前の分析と実測で決める。
これらが未定ならPanel=0を有効化しない。

### 8.2 convictionとbounty

既存conviction funnelとproducerのapplicable slash/rights失効 [C08]–[C09] を再利用し、
新phaseへの移行を明示する。計算fraudとDA/defaultを別basisで記録する。
watcherのローカル検査failureだけでslashしない。

interactive追及はpublic bondのdeposit/exposureを要する。
有効な客観proofにはdefined bounty、正当なcourtにはdeposit返却、
誤ったchallenge/義務不履行にはdefined費用を適用する。
同じproofの重複、複数session、relayでproducerを二重処罰しない。

bountyの財源は徴収できた担保・明示的fee pool等に限定し、
架空のnominal額や未発行の報酬から支払ったことにしない。
producerとchallengerの自己共謀によるfake fraud/bounty循環、
同じworkの再利用・proof front-runningを経済試験に含める。

Panel=0ではPanelFalseValidの対象seatは新claimに存在しない。
producerの処罰を、存在しないseatへのS4処罰完了に依存させない。
旧claimの署名者責任は旧規則の期間・scopeで維持する。
任意検査サービスへの契約違反は、別の定義がない限りPanelFalseValidとして処理しない。

### 8.3 Class-Independent Bond Production, Reward and Final Weight Caps — 2026-10-10追加決定

#### 8.3.1 目的・claim容量・三つの総量上限

**一定額のbondが一定DAA期間に得られる報酬対象ブロック数と報酬総額の上限を、
モデルや計算量とは独立に固定し、Final後の確定weightにも同じ資本・期間による総上限を課す。**
経済的な発行能力の単位は個別claimの計算速度ではなく、
拘束されたbond資本と合意上の共通DAA期間である。
同じclass・仕事量のclaimだけを比較する規則ではなく、異なるclass間にも共通の上限を適用する。

同一rulesetの下で、producer bondを`b`、固定された適格担保額を`C`、
合意上の集計期間を`I`、その長さを`W` DAAとして、次をMUST満たす。

~~~text
N_claims(b, I) <= Q_max(C, W, rho) // 共通倍率に対応するclaim発行上限
N_blocks(b, I) <= N_max(C, W)     // Block Cap: 報酬対象ブロック数
R_total(b, I)  <= R_max(C, W)     // Reward Cap: 帰属し得る報酬総額
FinalWeight(b,I) <= F_max(C, W)   // Final Weight Cap: 確定consensus credit
~~~

`N_max`、`R_max`、`F_max`は共通rulesetの下で担保額と期間だけから導出する。
`Q_max`は共通のclaim容量倍率`rho`にも対応するが、倍率増加で三つの総量上限を増やさない。
同額bond・同期間・同倍率なら、正直なminerと高速に偽造するminerの発行上限は同じである。
Model ID、class ID、PWU、実行時間、推論token数、主張した計算量、計算速度で
claim容量とこの三つの総量上限を増やしてはならない（MUST NOT）。
class/仕事量は実行仕様・検証手順・検証資源・必要な損失担保を決めるために使用する。
個別classのadmission、chain全体の予算、未解決責任によって実際に使える機会が減ることはあり得るが、
それらをbondの共通上限の加算枠にしてはならない。

Block Capだけでは高額rewardの経路を選ぶ迂回を防げず、Reward Capだけでは
大量のclaim/blockによる検証負荷やfork-choice操作を制限できない。
三つの総量上限に加え、未決済claim数・weight・最大責任額の別capをMUST維持する。
無報酬claimによるspamをBlock Capだけで防いだと扱わない。

これは同額bondの**最大獲得機会の上限**を共通にする規則である。
実際の報酬額、上限まで採掘できること、同じ実終了時刻を保証しない。
報酬は必要な計算・適用modeの検証・Final条件を満たしたclaimにのみ発生し、
計算を省略した者へ同額の報酬を無条件に支払う規則ではない。

#### 8.3.2 全報酬・確定weight経路の予約と清算

Block Cap / Reward Cap / Final Weight Capは全てのproducer報酬対象経路にMUST一貫して適用する。
Attempt、Free Prompt、claim-backed block、receipt block、対象となるwork slice/rider/batch、
未払escrow、将来の報酬請求権を、単一claimの発行数だけで代用してはならない。
一つのclaimから複数の報酬対象blockや支払が派生する場合も、全てを同じbond会計へ束縛する。
root/sliceの同じ仕事へ別rewardを新設せず、既存の発行・escrow資金制約も維持する。

1. claim受理時に、帰属するproducer bond、共通ruleset、受理DAA `d`、
   claim発行枠、派生blockの最大数、最大報酬責任とFinal weight配分を合意状態へ予約する（MUST）。
   将来の権利行使数・報酬額を有界に予約できないclaimを報酬対象へadmitしてはならない。
2. block生成・権利行使・報酬付与/成熟・Final weight付与の各経路でも同じ上限と予約残高をMUST再検査する。
   claimだけを制限して、後続receiptや別支払経路で上限を超えることを禁止する。
3. 未払escrowと将来請求権は潜在的な報酬としてMUST予約に含める。
   支払時には同じ予約を消費済みへ移し、予約と支払を別々の新規rewardとして二重計上しない。
   payoutを後の期間へ遅らせて古い権利と新しい発行枠を無制限に合算してはならない。
4. claim・block・reward・Final weightの帰属、集計期間、権利の発行/行使/支払時点の対応をMUST固定する。
   rolling/epoch方式、期間境界、繰越、担保増減の扱いは§14で定めるが、
   どの方式でも同じ資本を同時に重複計上して共通上限を迂回してはならない。

これらは個別bondへの追加制約であり、総subsidyを増やさない。model別配分は§8.5/ADR177で改定するが、
可変配分から個別capを超える報酬や未予約の権利を発生させない。
Tx feeその他の支払を含む`R_total`の構成と帰属もrelease前に列挙し、
producerの計算報酬を別の支払名目へ移して除外することを禁止する。

#### 8.3.3 共通DAA拘束と責任担保の分離

受理DAA `d`で消費した経済的発行枠の通常の最早回復時刻を、
class/仕事量によらない共通期間`W`を用いて`reuse_not_before = d + W`に固定する（MUST）。
秒・日への換算を合意時計と同一視しない。

早期の検証成功、任意audit/Valid報告、早いFinal、claim取消、void/conviction、
再送、別session、同じ仕事の分割によって、`d + W`より早く発行枠を返してはならない（MUST NOT）。
失効した報酬の予約を新claimのReward Capとして即時再利用することも禁止する。
量産した不正claimは同じ経済的発行枠を消費し、計算省略によって枠の回復を早めない。

**発行枠の回復と、slash可能な担保・未払報酬の解放は別の条件である。**
challenge、court、DA、retention/liabilityの責任が残る限り、必要な担保と未払報酬をMUST保持する。
`d + W`到達だけで既存のslash責任を消滅させず、同じ担保を新旧claimの損失吸収へ重複使用しない。
発行枠が回復しても、実際の新claim受理には残存責任控除後のheadroomを要求する。
slashされた資本を元の担保額`C`のまま再利用可能として数えてはならない。

§6のChallengeable開始時刻・base window・hard deadlineは公開反証の時計であり、
`d + W`の発行枠回復時計と別に検査する。`W`経過だけではFinal・報酬成熟を認めない。
claim固有の適格裁定/証拠義務が未解決なら、Final/担保解放は当該規則で処理する。
model取得不能だけではFinal/defaultを起動せず、実効検出不足を経済評価の未解決リスクとして扱う。
本RFCの「事後反証」はclaim受理後の公開反証を指し、不可逆な経済Finalと
責任担保の解放後に無条件で取り消せることを意味しない。

#### 8.3.4 同額資本の分割・再登録・別経路による迂回禁止

同額の拘束資本を複数bondへ分割しても、Block Cap / Reward Cap / Final Weight Capの合計を
一つのbondへ集約した場合より増やしてはならない（MUST NOT）。
最小枠・丸め・burstを含む導出式についてこの性質を検証する。
bond退出・再登録・鍵変更・担保移転によって既存の消費履歴、`reuse_not_before`、
未解決責任を消して満額の初期枠を再付与してはならない。
追加の独立した資本には規則上の追加枠を認め得るが、同じ資本の再表示を追加資本と数えない。

複数class、Free Prompt、receipt権利の譲渡/償還、slice分割、複数の支払経路でも
同じproducer帰属と集約上限をMUST維持する。
reorg/restart/IBDではcanonicalな予約・消費・回復時計・責任を決定的に復元し、
同じcanonical workの二重支払や時計の初期化を禁止する。
未検証claimの不可逆なfork-choice利益、報酬、EVM/state決済利益は§9で別途制限し、
これらのcapだけで防止済みとは扱わない。

#### 8.3.5 現行issuance slotsとの関係

現行[`palw_issuance_slots_v1`](../../consensus/core/src/palw_issuance_slots_v1.rs)は
bond額からslot/token容量を導出する既存のqueue/DoS制限であり、経済安全性全体の証明ではない。
新規Attemptが主対象で、counted `ReceiptLicensed`等でslotを早期解放し、
この機能単独では全報酬経路のBlock Cap / Reward Cap / Final Weight Capを保証しない。
tokenのDAA補充と、phase依存のslot解放を同じ時計と呼ばない。

2026-10-10の[pre/t12コード確認](../design/palw/t12-bond-reuse-audit-2026-10-10.md)では、
`palw_t12_shipped_params()`のF-SはDAA 1,700から有効となる設定である。
過去のlaunch/dormant fixtureやsource冒頭の休眠説明を、現在の組立設定と同一視しない。
これは稼働fleetのbinary/tipを確認した記録ではない。
同確認記録のclass/work依存の拘束期間という提案は、本節のclass非依存の共通`W`へ改定する。
既存のslot/bucketを利用する場合も、本節の全経路・共通拘束・報酬会計を実装し、
§13の試験と別の協調upgradeを経てから新modeへ適用する。文書追記で旧claimの規則を変更しない。

#### 8.3.6 経済安全性の判定と未確定値

不正反証はPanelの承認を要さず、適格な独立public verifierが公開証拠で開始できなければならない。
期限内の有効な客観証拠には§8.2の裁定・void・slash・報酬失効を適用する。
計算省略で同額bondのブロック数・報酬総額の上限が増えないことを先に確認し、
その上で正直な計算の期待利益が不正を上回ることを経済分析する。
claim/戦略ごとの単純化した比較条件は次である。

~~~text
p * (R_risk + L_collectible_net) > C_saved
~~~

`p`は責任期限内の実効検出・裁定・徴収確率、`R_risk`は検出時に実際に失効させられる報酬、
`L_collectible_net`は実際に回収できる担保損失から攻撃者側への還流を控除した額、
`C_saved`は偽造により節約できる計算費用である。
producerとreporterが共謀する場合は、現行PALW reporter bountyの49%規則を引き継ぐ場合等の実際の還流を
控除し、nominal/gross Slashをそのまま攻撃者の損失と数えない。
同時claimが同じbondを共有する場合も、同じ担保損失をclaimごとに重複して計上しない。

Reward Capは報酬経路の最大不正利得を有界にするが、`p > 0`や十分な期待損失を保証しない。
fork-choice/外部市場等の追加利得が残る場合は上式だけで判定せず、その利得も評価する。
Block Cap、Reward Cap、`W`、責任期間、検出率、徴収可能額は現行規則の読み取り、
実測、敵対的経済分析から固定する。10,000 BILIや100件/7日を既定値にしない。
現行t12のproducer最小bond 13,000 BILIを、この追記で変更しない。

#### 8.3.7 予約額引下げと公開検証・責任担保・共通DAA拘束の一体設計

**Panel=0でAttemptの予約額を引き下げる場合は、倍率`rho`だけでなく、escrowの予約規則、
公開検証能力、実際に徴収可能な担保、§8.3.3の共通DAA拘束を一体で設計する（MUST）。**
現行のモデルclassは[`palw_capacity_class_creditable_v1`](../../consensus/core/src/palw_audit_door_v1.rs)の
escrow割引対象に含まれず、計算量の予約部分だけを`rho`で割っても予約総額は同じ比率では下がらない。
`rho = 1,000`から`10,000`への変更だけで、約3,500 BILIが約350 BILIになると仮定してはならない。

新modeのescrow予約を減らすには、対象となる全報酬profileで、modelを得た検証者の計算検査・claim固有証拠からの客観裁定が
責任期限内に完結する能力と、同時claimの最大損失を回収できる担保・未払報酬をMUST評価する。
Panel signer lockがない状態で、旧modeのaudit/credit条件をそのまま安全性の根拠にしない。
予約引下げ後も共通`d + W`までの早期回復禁止、未解決責任の保持、担保重複使用禁止、
class非依存Block Cap / Reward Cap / Final Weight Capを維持し、§8.3.6の期待利益条件を再評価する。
拘束期間を延ばすだけで、公開検証能力や徴収可能額の不足を満たしたことにしない。

担保予約としてのescrow termと実際の未払報酬額を区別し、予約引下げを報酬削減や発行量増加と
同一視しない。具体的な予約額・割引条件・共通拘束期間は実測と敵対的経済分析で固定する。
約350 BILIや`rho = 10,000`は検討例であり、本RFCでの採用値・有効化を意味しない。

### 8.4 Claim容量の倍率とbond当たり総影響の保存

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を共通前提として適用する。
Bondが採掘速度と報酬総額の上限を決め、モデル計算はその枠を使用するために必要な正当な仕事となる。
不正をしても枠は増えず、公開反証・客観裁定で有罪になれば報酬失効と適用規則上の担保損失を負う。

claim容量を`m`倍にする場合、一件の経済的権利を細分化するか同じ予算から配分することで、
同額bond・同期間のブロック権利・報酬・確定weightの合計を維持する（MUST）。
同じ大きさの権利なら`N → mN`に対して一件の配分は`A → A/m`、合計は`N × A`のままである。
これは説明用の関係であり、全claimの報酬額を同じにする、実行量/PWUを偽装する、
物理blockを小数個発行する、上限までの実採掘を保証する規則ではない。
block権利の端数は合意上の整数/fixed-point台帳で集約し、実際のblock発生時にも総上限を検査する。
端数切上げ・最小credit・一claimからの複数支払・期間境界で総量が増えることを禁止する。

Final前の合計weight上限だけでは本原則を満たさない。Final時の付与、retiredへの移動、
conviction/reversal、fork-choice/DAAとsettlementの全writer/readerで、同じ予約済み配分と上限をMUST使う。
現行の`palw_weight_final_safe_v1`が原則full contributionを加算する経路は、新規則へ移行する際に改定する。
旧claimは旧rulesetのままとし、旧責任を残したupgradeで新しい満額枠を重複取得させない。
EXEC_TX/EXEC_SLICEのweight/DAAは0を維持し、この原則だけで時計や乱数sourceを増やさない。

一件の権利・予約を小さくし、正直なminerが数十万BILIの担保なしに実用的なclaim数を扱える方向を選ぶ。
必要な計算・公開検証・court/DA費用・徴収可能な損失担保は維持し、§8.3.6–8.3.7の実測と期待利益評価を要求する。
共通上限は確率的検査・外部監視・exact courtの代用品ではなく、全条件のANDでPALWの成立を評価する。

### 8.5 モデル別bond連動coinbase allocationと入手への不介入

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)をPanel=0でも適用する。
model配布・Seeder・取得可否はchainが管理せず、旧availability/TRDC/FPR gateを撤回する。
固定Model ID/root/specとclaim固有の有限証拠/court責任は維持する。

#### 8.5.1 集計と配分

modelへ予約/拘束されたminer元本を、受理済みsnapshotで重複なく集計する。

~~~text
sum_m C_{b,m}(t) <= C_b_effective_locked(t)
S_m(t) = sum_b C_{b,m}(t)
A_m(t) = f(S_m(t)), f >= 0, f(0) = 0
R_m(t) = R_PALW(t) * A_m(t) / sum_j A_j(t)  [positive denominator]
sum_m R_m(t) <= R_PALW(t)
~~~

claim件数/予約額、rho、root/slice/rider、鍵分割、model複製で同じ元本を増幅しない。
市場seed/AMM/Position/Panel/配布契約担保は自動算入しない。責任控除・slash/退出・model移動、
snapshot/積分・期間境界を規範化し、短期借入や同時多model割当による二重配分を防ぐ。
7日は説明例であり、配分epochと共通`W`の対応/具体値は未決である。

総coinbase/subsidy・DAA発行予算を維持し、分母0ならmodel配分0とする。
整数精度/丸め残余、未配分/未使用枠、model内のproducer/owner等のlegを確定する。
資本預託だけで支払わず、有効な計算claim・Final・固有workを要する。

#### 8.5.2 個別bond capとの同時制約

受理時にmodel、配分snapshotと最大権利を固定し、network/model/bondの各予算を同時に予約・検査する。
coinbase、未払escrow、成熟、Final、retired/reversal、reader/undoで同じ帰属を使う。
外部資本でmodel配分が増えても既存bondのQ/B/R/F capと最早回復`d+W`は増やさない。
model倍率変動や移動で枠を早く回復せず、過去claimへ無予約の追加権利を遡及付与しない。
未使用model枠の再配分も個別cap以下とし、同じsubsidyを複数legで二重支払しない。
coinbase配分からblock頻度/ticket/DAA/難易度を自動変更せず、変更には別の明示的upgradeを要する。

#### 8.5.3 公開対閉鎖の経済評価

公開により第三者の参加資本を集めた方が、閉鎖自己資本より圧倒的に有利となることを設計目標にする。
同額`S_m`なら自己/外部資本は同じ配分であり、ウォレット/IP数を所有者の独立性証明に使わない。
線形`f`だけで既存minerの利益増を保証せず、superlinearな曲線の集中・Sybil/自己増資・予算飽和を審査する。
他model競争、model内取り分、既存bond cap、資本/配布/検証費、閉鎖による`p`低下・49%還流も含める。
全資本自前の閉鎖攻撃者と同額自己増資も比較し、優位の数値閾値/範囲を事前定義する。
正しいmodelを得られないverifierの`p`は0にもなり、capや資本増加だけで不正の期待損失を証明しない。
達成できない条件と未立証事項を報告し、未確定の倍率式を安全性保証として使わない。

**2026-10-10改定（bond集約優位）:** ADR-0177の改定により、評価対象は「公開対閉鎖の優位」から「bond集約による配分優位」へ変わる。
`A_m = S_m^α`（`α > 1`）を候補とする。個別bond上限・総予算・共通期間は不変である。大口資本の優位は残余リスクとし、`p=0`は独立gateとする。

#### 8.5.4 completion / activation

同一chain/claim/proofでpeer/取得結果を変えても合意結果が変わらない試験、claim固有court scope、
資本重複排除、整数/zero-denominator/未使用枠、三層予算、同額bondの正直/偽造機会、
公開対閉鎖の経済比較、reorg/restart/IBD/移行を独立に検証する（ADR177 §3）。
現行subsidy carve/rider/emission budgetのPASSは本配分式の実装証拠ではない。
倍率式・会計/裁定実装・条件付きG14・経済評価と別の協調activationまで新modeを有効化しない。

## 9. work weight、PALW blockとsettlement

Panel=0はLLM計算の正しさへの処罰経路を置き換える設計である。
それだけでfake claimを防ぐfork choice、時計、DAA、native EVM決算が完成したとはしない。

RFC08のclaim-backed work/slice [C17] に適用する場合、
root/slice/jobの一意性、最大未決済credit、依存sliceの失効、
reorgと各writer/readerのFinal条件を新modeに揃える。
同じ実行を複数のclaim/blockで重複報酬にしない。

unresolved claimからirreversibleな報酬/state・外部releaseを許可しない。
早期のblock inclusionと経済的なsafe settlementは区別する。
RPC/bridgeは新modeのFinal、pending dispute、assuranceを読めなければsafeと表示しない。

RFC14 §16.10の規則E接続を継承する。適用modeのFinal/court条件を満たさないworkを確定weightへ昇格させず、
provisional weightのcap・用途・失効・undoを全fork-choice/settlement readerで証明する。
escrowだけでは不可逆なchain選択を防げない。EXEC_TX/EXEC_SLICEのweight/DAAは0のままである。
全peer停止でもmodel入手性だけで合意結果を変えないこと、閉鎖時の検出率、
private maturation・全Panel共謀、partition/reorg/restartを区別して検証する。

RFC12のDNS finality撤去・native EVM変更 [C18] は別のupgradeである。
固定PALW Panelの撤去とDNS validatorの撤去を同一変更と呼ばない。
本RFCからDNS、BlockDAG、署名、hash、PoW/clockの既存規則を黙って削除しない。

## 10. 将来のnode-verifiable compact proofとの関係

producerが全nodeの承認済みkernel checkerで安価に検証できるcompact proofを
提出する方式も、固定Panelなしで設計できる。
ただし本RFCの基本modeはoptimistic public verificationであり、
compact proofがすでに実装・有効という意味ではない。

将来その別modeを追加するなら、semantic/constraint coverage、post-commit challenge、
whole-claim soundness、worst-case node資源、公開witness、exact courtを別途審査し、
新scheme/version/assuranceをbindする。positive proofが必須のmodeでsilence=passにしない。

**compact proofを導入する場合でも、本RFCのG14を省略・代替してPanel=0を有効化しない。**
普通のpublic bondからの独立した不正追及が完結することを、この提案の共通前提とする。

ADR0172のversioned kernel方針 [C16] に従い、VM fallback、TEE validity trust、
BFT運営委員会、trusted committeeによる真偽判定を追加しない。

## 11. 移行・wire互換・公開表示

### 11.1 段階

~~~text
current Panel rules
    → RFC14 public fraud prosecution complete (G14 PASS)
    → panel-free lifecycle/economics/DA/Final implementation
    → adversarial shadow / real-node E2E / independent review
    → separately scheduled network upgrade
    → Panel=0 for admitted new-mode claims
~~~

5/3から1/1を経由するdrillは任意であり、本RFCの必須中間段階ではない。
1/1にも別の担保・lifecycle検証が必要で、既存basis_k=2依存を定数変更だけで迂回しない。
1/1成功は0へのgate達成を意味しない。

### 11.2 互換性

mode/phase/object/class identity、署名domain、codec、state root/carriage、Params mirror、
mempool gate、SDK/RPC/manifest、fingerprintとactivation scheduleを一緒に固定する。

* 旧claimは旧Panel/receipt/担保・deadline規則で完了させる。
* 旧receiptを新modeのevidenceに読み替えない。
* 新modeではPanelBound/ReceiptLicensedが必須でなくなることを全writer/readerへ反映する。
* 不明versionを成功扱いせず、非対応nodeはupgradeなしに新履歴へ参加しない。
* class identity/plan/rulesetをbind/admission時に固定し、途中upgradeでcourt意味を変えない。
* restart/IBD/reorg/pruningでpublic prosecutionと担保を再現する。

旧networkの規則、まだadmitしていないprofile、mode間の違いを保持する。
G14未達のprofileを旧modeから新modeへ自動移行しない。

### 11.3 利用者に示す情報

最低限、claimのsettlement mode、window/deadline、pending court/DA、Final assuranceを公開する。
新modeのFinalは「optimistic public verificationの条件を満たした」と表示し、
「全nodeがLLM全計算の正しさを証明済み」「監査なしでも必ず安全」と表示しない。

監査サービスがoptionalであることと、honestな実監視が安全性の前提であることを併記する。
watcher台数やreceipt件数を、independent ownershipやhonestyの証明として表示しない。

## 12. 実装変更の対応表

| 面 | 変更候補 | 必須確認 |
| --- | --- | --- |
| mode/claim lifecycle | palw_state_v2、palw_mode_v2、versioned claim/object encoding | 空quorumによる旧licenceではなく、新Challengeableとruleset |
| public admission | class/TIR admission、RFC14 dispute plan、public bond reads | 全reward profileでG14、非operator資格とmaterial取得 |
| court surface | palw_court_v2、objective offence gate/fold、RFC14予約 | licenseのない新claimを正当に追及でき、先取りで排除しない |
| shared filer | RFC14 palw_fraud_filer、operator/seat/SDK adapters | fresh ordinary bondがpublic dataだけでproof/courtへ到達 |
| Final/DA/locks | deadline sweep、Final helper、vesting/rights、DA sessions | admitted DA、hard deadline、unresolved dispute、producer担保 |
| economics | producer reservation、aggregate gain、bounty/fees、全経路の発行/報酬会計 | signer lock依存を除いたcost/回収可能額・退出条件、§8.3のclass非依存総量cap・共通DAA拘束 |
| block/settlement | processor/mining/work-slice、RPC/native EVM readers | stage別weight、unique work、safe/Finalの一致 |
| network upgrade | Params/bundle mirror、fingerprint、codec/IBD | gate未達でactivation不可。旧履歴は不変 |
| UI/docs | explorer/options/SDKのmode・assurance表示 | no-challengeと正しさの証明を区別 |

上記は実装箇所の設計であり、このRFCの作成でRustコードを変更したことを意味しない。

## 13. 必須試験とactivation checklist

### 13.1 前提gateの否定試験

次のいずれかを故意に残したbuildで、Panel=0のactivationが拒否されることを確認する。

* operatorだけがproofを作れる、public bondの資格・APIが不足する。
* producer heap/drill instance/private endpointが必要になる。
* canonical held/fusedのcap/prefix/committed tile gapが残る。
* garbage/borrowed traceがDAに答えると計算不正の追及が終わる。
* non-seat pursuit中にFinalできる、open court先取りでdirect proofが拒否される。
* 最大profileやcold material時間、resource/inclusion boundsが未検証。
* class/planの完成記録を欠く、またはstaged upgradeのnetwork identityが一致しない。
* Block Cap / Reward Cap / Final Weight Cap / 共通DAA拘束が未実装・未確定、または報酬経路の一部が集約会計を迂回する。

### 13.2 Panel=0 node E2E

baselineのfold unit testだけでは新modeの証明にならない。
以下をreal node、普通のpublic bond、public authenticated materialで実行する。

| ケース | 必須結果 |
| --- | --- |
| 正しいclaim、固定Panelなし | 新Challengeable → window/DA完了 → Final。空receiptで旧licenceを偽造しない |
| 偽claim、producer/operator共謀 | 後から起動したfresh public verifierがlocalize → exact conviction → slash/Final阻止 |
| material withholding / rootは開く偽trace / borrowed trace | それぞれDA/default・exact fraud・identity/output faultへ到達 |
| 有効proofがあるが別courtが先行 | objectively valid proofを受理し、session・露出・担保を決定的に清算 |
| malformed proof / honest producer / faulty checker | 不正なslashなし、bounded棄却 |
| watcherの退出・復帰・全watcher不在 | 前二者の時計と追及を検証。不在ケースの安全性をPASSとして報告しない |
| spam、全interactive枠占有、carrier遅延 | cap/hard deadlineとinclusion仮定の範囲内で正当追及が進行 |
| 同時claims、担保退出、fake fraud bounty | gain/予約二重使用・自己共謀利益・架空collected支払なし |
| 同額bond、異なるmodel/class/PWU/token数、正直な計算対省略偽造 | Block Cap / Reward Cap / Final Weight Capの共通上限が不変。巨大計算の主張や高速偽造で拡大しない |
| 早期検証/Final、取消、void/conviction、再送・別session | 経済的発行枠は受理DAA `d`からの`d + W`より早く回復しない。報酬予約の即時再利用なし |
| Attempt / Free Prompt / 一claimから複数receipt・block / slice・rider・batch | 受理予約とblock/報酬付与の両方で集約capを検査。未払・将来請求権を含め二重払い・別経路の迂回なし |
| 期間境界、遅延payout、bond分割・退出/再登録・鍵変更・担保移転 | 同額資本の総上限が増えず、消費履歴・時計・残存責任を引き継ぐ。丸め/burst/繰越の境界を検証 |
| `d + W`到達、未解決court/DA/retention、slash後の新claim | 枠回復と担保解放を分離。残存責任と徴収済み資本を控除し、担保の重複使用なし |
| Attempt/escrow予約引下げ、倍率変更、同時claim増加 | §8.3.7の公開検証能力・徴収可能額・共通DAA拘束を一体評価。総量cap・残存責任・期待利益条件を維持 |
| 容量×1/×100/×1000、Final/retired/reversal、丸め・最小credit | §8.4のブロック権利・報酬・確定weight総量が不変。全writer/readerで同じ配分を使用 |
| restart/IBD/reorg/duplicate proof | 同じcanonical stateを復元。二重reward/slashや時計再開始なし |
| 最大admitted class/context/layout | canonical規模で各budget内。小fixtureで置換しない |
| work-sliceとnative settlementを対象にする場合 | weight/escrow/state/bridgeのFinal条件と不正失効が一致 |
| 旧claimをまたぐupgrade | 旧Panel duty/receipt/lockは旧規則のまま完了 |

検査方式のsoundness review、冷温両条件の資源計測、監視市場の運用条件、
producer-only担保の経済評価を別々に記録する。
有限のE2E件数だけから全入力の完全性やhonest watcherの永久存在を結論しない。

### 13.3 全て満たすまで有効化しない

~~~text
ACTIVATION_ALLOWED
    = G14_PUBLIC_FRAUD_PROSECUTION_COMPLETE
      AND all rewarded profile dispute/resource gates
      AND new lifecycle/court/Final implementation complete
      AND panel-free collateral/accounting review complete
      AND class-independent bond claim / block / reward / Final-weight caps + common DAA hold complete (§8.3–8.4)
      AND public material/watcher/inclusion operating assumptions supported
      AND ADR177 non-interference / capital / allocation / open-vs-closed ECON-MEAS gates complete
      AND RFC14 claim-specific prosecution / Rule E gates complete
      AND independent adversarial node E2E + recovery/migration PASS
      AND separately coordinated network schedule
~~~

これはreleaseのAND契約であり、operatorの口頭承認だけでtrueになるfieldではない。
前提の一部だけの成功、RFCのmerge、0 seats設定、将来proof方式を根拠に有効化してはならない。
RFC14 §16.8/ADR177 §3のnon-interference、資本重複排除、model/network/bond予算保存、
公開対閉鎖の敵対的経済評価をPanel=0でも実施する。model未取得だけの停止・Slashを起動しない。
正しいmodelを得たfresh verifierによるclaim固有courtと計算反証を実証する。
旧7/5/15%・監査/lease・FPR全量公開/代替Seeder/公開費gateは撤回する。
G14 PASSでも自動有効化しない。モデル非公開時の経済優位は未立証であり、旧claimと発行上限を保持する。

**2026-10-10追加（ユーザー指示）:** 本RFCの有効化には、[PALW確率・経済安全性ゲート](../design/palw/probabilistic-economic-security-gate.md)（四条件、T1–T6、A–F、三つの反例、経済的抑止と独立の被害有界化）のPASSを追加で要求する。PASS / FAIL / UNKNOWNのうちUNKNOWNはPASSにしない。

## 14. 本RFCで決めない値と現状

activation DAA、object tag・signature domain、bond/exposure比率、bounty/費用、
最大未決済work/gain、challenge/cold-fetch/court/inclusion時間、session cap、
DA保持期間、node資源、check error parameterは別途固定する。
§8.3の`N_max(C, W)` / `R_max(C, W)`の導出式と具体値、共通DAA期間`W`、
`Q_max(C, W, rho)` / `F_max(C, W)`、倍率と一claim配分・block権利端数の関係、
rolling/epoch集計と境界/繰越、block・rewardの帰属時計、担保増減・分割/移転時の履歴、
支払構成、責任期間、旧modeとの移行会計もrelease前に固定する。
§8.3.7のAttempt/escrow予約額と割引条件も、公開検証能力・徴収可能担保・共通DAA拘束の評価後に固定する。
§8.5の`f`/配分時計/実効元本、model内配分、残余/zero denominator、claim固有court scope、
公開優位の数値基準・市場仮定・閉鎖時の実効検出率も固定する。
方針は個別cap/共通拘束とモデル入手への不介入に決定するが、未確定値を任意の件数/日数/倍率で埋めない。
それらが未確定の間はDraftであり、稼働保証ではない。

baselineには固定Panelとsigner担保・receipt/licensingへの依存があり [C01]–[C07]、
外部proof/fresh held証拠にも未完部分がある [C10]–[C12]。
**したがって本RFCの必須前提は、現在のbaselineでは達成済みとしない。**

本RFCの作成でPanel定数、合意コード、bond、ネットワーク設定、稼働serverは変更しない。
RFC14の既存機能に対する45 test PASSを、Panel=0や新gateの実装成功として流用しない。
新modeの実node試験・経済評価・upgradeは未実施である。

## 15. 参照

**R14** — [RFC-0014: Panel多数派に依存しないfraud prosecution][R14]。
特に§2（条件付き保証）、§3（Dispute Completeness）、§5（fused DA）、
§6（共通filer）、§7（予約/Final/先取り）、§13（fresh outsider E2E）。

以下の現行sourceは全てbaselineへ固定する。

* **C01** — [現行5 seats / quorum 3][C01] — `consensus/core/src/palw_fp_devnet_v3.rs`.
* **C02** — [現行claim phase][C02] — `consensus/core/src/palw_state_v2.rs`.
* **C03** — [receipt quorum validation][C03] — `consensus/core/src/palw_panel_v2.rs`.
* **C04** — [Final basis_kの既存下限][C04] — `consensus/core/src/palw_state_v2.rs`.
* **C05** — [Valid signer lockの経済計算][C05] — `consensus/core/src/palw_state_v2.rs`.
* **C06** — [Panel/served mask/basis_kによるescrow release][C06] — `consensus/core/src/palw_state_v2.rs`.
* **C07** — [ReceiptLicensedを対象とするcourt開始][C07] — `consensus/core/src/palw_court_v2.rs`.
* **C08** — [客観convictionからclaimを処理する現行funnel][C08] — `consensus/core/src/palw_state_v2.rs`.
* **C09** — [producerの没収・slash][C09] — `consensus/core/src/palw_state_v2.rs`.
* **C10** — [seat proof filerとheld gap][C10] — `kaspad/src/palw_filer_replay.rs`.
* **C11** — [外部operatorのDA-only不足][C11] — `kaspad/src/palw_operator_da.rs`.
* **C12** — [fresh verifierと8k held/fusedの三gap][C12] — `kaspad/src/palw_filer_held_e2e.rs`.
* **C13** — [operator-anchorの現行trust境界][C13] — `docs/rfc/0010-permissionless-palw-panel-and-claim-completion.md`.
* **C14** — [required positive evidenceとexact court][C14] — `docs/rfc/0011-permissionless-model-and-long-context-onboarding.md`.
* **C15** — [確率的constraint検査の未有効化][C15] — `docs/adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md`.
* **C16** — [versioned kernelと禁止する代替信頼][C16] — `docs/adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md`.
* **C17** — [claim-backed work、weightとsettlement][C17] — `docs/rfc/0008-palw-claim-backed-consensus-blocks.md`.
* **C18** — [PALW-only合意とnative EVMの別upgrade][C18] — `docs/rfc/0012-palw-only-consensus-and-native-evm-settlement.md`.

[R14]: 0014-panel-independent-fraud-prosecution.md
[C01]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_fp_devnet_v3.rs#L780
[C02]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L5830
[C03]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_panel_v2.rs#L2981
[C04]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L1003
[C05]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L4602
[C06]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L4700
[C07]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_court_v2.rs#L333
[C08]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L23107
[C09]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L26728
[C10]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_filer_replay.rs#L5
[C11]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_operator_da.rs#L27
[C12]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_filer_held_e2e.rs#L12
[C13]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0010-permissionless-palw-panel-and-claim-completion.md#L33
[C14]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0011-permissionless-model-and-long-context-onboarding.md#L363
[C15]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md#L3
[C16]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md#L9
[C17]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0008-palw-claim-backed-consensus-blocks.md#L1
[C18]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0012-palw-only-consensus-and-native-evm-settlement.md#L1

## Mission alignment amendment — 2026-10-07

本RFCのPanel=0は、固定Panelの担当検証をpermissionless verifier + objective fraud proofへ移す別modeである。前提であるRFC14の全completion gatesが、公開materialだけを使う普通の非Panel bondで完全に成立するまでdeferred/inactiveを維持する。前提成立だけで自動有効化せず、本RFCのpositive verification、pending/maturity、DA、exposure、monitoring economics、migrationと明示的activationも必要とする。空quorum、無receiptのsilent acceptance、単なるpanel_seats=0では成立しない。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](0014-panel-independent-fraud-prosecution.md)、[RFC15](0015-panel-free-permissionless-verification.md)を参照する。

## MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。
