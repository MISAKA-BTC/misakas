# RFC-0015 — Panel=0: 固定Panelからpermissionless verifierとobjective fraud proofへ移行する

**トークンの表示名は Misaka、ticker は BILI。** `misaka` 系アドレスプレフィックスと既存のprotocol/CLI/API識別子は維持する（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


* **Status:** Draft / deferred implementation design, 2026-10-07。合意規則・wire形式・activationは未実装・未有効化。
* **Source baseline:** MISAKA-BTC/misakas public main `43f0bcb362d37cba79414940f3fdd368d40d3377`（2026-10-07確認）。
* **Hard prerequisite:** [RFC-0014][R14]のDispute Completenessとpublic non-seat prosecutionが、対象ネットワークで本RFCの全reward-bearing class/profileについて完全に成立していること。RFC14の文書作成・mergeだけでは満たさない。
* **Activation:** 前提達成後の別の協調upgrade。提案fence名 `palw_panel_free_v1` は設計用の未割当名で、既存Paramsのfieldではない。activation DAA、wire tag、署名domainはここでは割り当てない。
* **Decision:** 固定Panelの選出・必須receipt quorum・seat依存のlicensingを新経路の合意要件から外し、public authenticated material、permissionless verifier、objective court、producer担保、challenge windowで検証・処罰を行う。
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
| Producer | 有効classで計算し、claim/input/output/state/witnessをcommitする。materialを公開・保持し、担保とfeeを負担する |
| Permissionless verifier / watchdog | 任意のpublic参加者として独立検査する。不一致をlocalizeし、必要materialを要求してproof/courtへ進む |
| Full node | object・binding・合意規則と受理された客観証拠を決定的に検証し、state・weight・報酬・slashを更新する |
| Public DA provider | rootに認証されるbytesを公開・保持する。新しい固定検証委員会にはしない |
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

* RFC14のDisputeAdmissible class/plan/kernel、対象mode、artifact/job/layout/task/contextのidentity。
* producer authorization、public bond資格、最大同時claims/aggregate gainと必要担保。
* public authenticated material manifest、既存rootとのbinding、取得/開示unitとretention義務。
* DA・court・localizerのbounds、challenge window・開始cutoffとhard deadline。
* fee、重複しないwork/slice identityと既存履歴での利用状況。

「URLを公表した」「witness_rootをcommitした」だけではmaterialが入手可能とは認定しない。
どのbytesを事前公開し、どのbounded unitをon-chain要求で開示させ、
未開示を何で客観判定するかをplanとDA規則で固定する。

rootの正しさやmaterialの意味を未検証のまま、重み・報酬を全量確定しない。
DA admissionは計算の正しさの証明ではない。

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

## 6. challenge window、Final、censorshipとspam

### 6.1 時計

Challengeableの受理時点でbase windowとabsolute hard deadlineを固定する。
未取得materialの依存で必要期間を満たせない場合はChallengeableへadmitしない、
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
AND required public DA/retention obligations satisfied
AND base challenge window + class verification horizon expired
AND no unresolved accepted pursuit / required court
AND unique work, accounting and bounded-liability conditions satisfied
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
| economics | producer reservation、aggregate gain、bounty/fees | signer lock依存を除いたcost/回収可能額・退出条件 |
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
      AND public material/watcher/inclusion operating assumptions supported
      AND independent adversarial node E2E + recovery/migration PASS
      AND separately coordinated network schedule
~~~

これはreleaseのAND契約であり、operatorの口頭承認だけでtrueになるfieldではない。
前提の一部だけの成功、RFCのmerge、0 seats設定、将来proof方式を根拠に有効化してはならない。

## 14. 本RFCで決めない値と現状

activation DAA、object tag・signature domain、bond/exposure比率、bounty/費用、
最大未決済work/gain、challenge/cold-fetch/court/inclusion時間、session cap、
DA保持期間、node資源、check error parameterは別途固定する。
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
