# RFC-0014 — Panel多数派に依存しないPALW fraud prosecution: 1人の正直なverifierから有界の客観証拠へ

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


## 2026-10-09 登録不変性との整合

[ADR-0175](../adr/0175-registered-models-are-permanently-immutable.md)を適用する。モデルの配布者は登録済みweights・tokenizer・実行仕様・canonical artifact rootを変更する権限を持たない。MISAKA Torrent採用は後続改定で廃止する。peer、配布先、同じ内容の包装・取得経路はoff-chainの任意運用であり、合意上のavailability leaseは要求しない。新しいtransport bindingが必要な場合も、同じ固定model ID/rootへの対応を再検証し、違う内容なら別モデルとして登録する。

「新しいmodel/version」は独立した新モデルの登録を意味し、同じ登録IDへのversion追加・line/head置換を意味しない。配布不能だけで当該モデルのrewardability・weight・担保責任を変更しない。客観的計算不正への裁定は別に維持する。既存Position・AMMは改善版へ移動しない。§16の配布不介入・資本配分と、この永久不変bindingをそれぞれ検証する。

**トークンの表示名は Misaka、ticker は BILI。** `misaka` 系アドレスプレフィックスと既存のprotocol/CLI/API識別子は維持する（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


* **Status:** Revised Draft / implementation design, 2026-10-10後続改定。§16はモデル入手への合意不介入と拘束miner元本によるモデル別coinbase配分を定義する。旧Seeder/Torrent可用性/TRDC/FPR gateは撤回。倍率式・経済優位・実装・activationは未完了で、既存ネットワーク規則は変わらない。
* **Source baseline:** MISAKA-BTC/misakas public main `43f0bcb362d37cba79414940f3fdd368d40d3377`（2026-10-07確認）。コード上の現状と本RFCの提案を区別する。
* **Activation:** 協調したネットワーク別upgrade。提案fence名 `palw_dispute_complete_v1` は未割当であり、既存Paramsのfieldではない。DAA・object tag・serialization version・新しい署名domainはここでは割り当てない。
* **Scope:** class admission、第三者の証拠取得・局所化、seat/non-seat共通filer、court開始とFinalの競合、producer/seatへの客観的処罰。
* **Related:** RFC07 [S24]、RFC08 [S25]、RFC10 [S20]、RFC11 [S21]、RFC13 [S26]、ADR0171 [S22]、ADR0172 [S23]。
* **Deferred successor:** [RFC-0015: Panel=0への移行][R15]。本RFCのpublic non-seat prosecutionが完全に成立するまで、そのmodeは有効化禁止。
* **2026-10-07追加方針:** 旧モデル取得強制の設計は[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)で撤回した。以前の文書チェックは履歴であり、新coinbase配分の完成証拠ではない。
* **2026-10-10改定:** 旧モデル取得強制の設計は[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)で撤回した。以前の文書チェックは履歴であり、新coinbase配分の完成証拠ではない。
* **2026-10-10 FPR追加:** 旧モデル取得強制の設計は[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)で撤回した。以前の文書チェックは履歴であり、新coinbase配分の完成証拠ではない。
* **Normative language:** MUST / MUST NOTは有効化後に必要な規則、SHOULDは理由を記録して変更できる実装方針。以下の新しい型・module名は設計用の仮名で、現行Rust型の存在を意味しない。

## 0. 採用する設計

**後続改定のscope:** モデル自体の入手・公開は合意条件にしない。以下のG14/Dispute Completenessは、
verifierが正しい登録modelを保有/取得できた条件で、claim固有証拠から裁定する能力を指す。
モデル取得を目的とする要求は適格DA/courtとせず、全weights/反復rangeの強制公開へ拡張しない（§16、ADR177 D2/D7）。
閉鎖時の実効検出と経済安全性は別に未立証であり、旧無条件の全量取得保証を意味しない。

Panelは通常時の検査・receipt集約を担う。claimの真偽をPanel多数派の信頼だけで決めない。
不正を見つけた任意の適格な第三者が、Panelへの所属やgenesis operatorの協力を必要とせず、
認証された公開データから不正位置を絞り込み、有界のexact courtまたは既存の
`ExecutorRefuted`へ到達できる仕組みを作る。

**classを報酬・work weightの対象にする前に、第三者だけでこの処罰経路を完結できる
Dispute Completenessを合意上のadmission条件にする。**
ハッシュが開くこと、producer自身がcourt evidenceを作れること、成功するcourt kernelがあることは、
独立した第三者が証拠を取得できることの代わりにならない。

この条件を完全に満たした後の固定Panel撤去は[RFC-0015][R15]で別に定義する。
**Panel外の普通のpublic bondが、producerの秘密状態を使わず、public authenticated materialだけから
不正をlocalizeして客観的convictionまで進める状態に達するまで、Panel=0を有効にしてはならない。**
本RFCの文書作成・merge、operatorのみの成功、小geometryやliar instanceを使うcourtテストは、
この前提の達成ではない。Panel=0は検証の廃止ではなく、固定Panelからpermissionless verifierと
objective fraud proofへの移行であり、lifecycle・担保・Finalを含む別upgradeを要する。

目標は次の二つに分ける。

1. **検出後の保証:** 正直なverifierが、不正を示す検証可能な不一致を期限内に発見した場合、
   producerが必要なclaim固有証拠を開示すれば客観裁定へ、開示しなければ定義されたDA/default処理へ進む。
   不正claimをPanelの多数票だけで経済的Finalに逃がさない。
2. **検出の保証:** 完全な決定的検証なら、対象範囲について不正を検出する。
   ADR0171の確率的constraint検査なら、承認済みsuiteの条件付き見逃し確率を受け入れる。
   「1人オンラインなら全ての偽claimが必ず検出される」とは書かない。

「1人がいる」は、任意の小さなPC1台が無制限の全claimを裁ける意味ではない。
そのverifierは対象classの承認済み検査・局所化を実行でき、許容claim rateを処理する資源、
必要なbond/exposureとfee、期限内のネットワーク到達性を持つ必要がある。
計算資源、DA、chain inclusion、challenge randomnessへの仮定はPanel多数派の仮定と別に明示する。

## 1. 現行コードの到達点

以下はbaselineの確認結果であり、新仕様の実装完了の宣言ではない。

| 部分 | 現行の処理 | このRFCで閉じる不足 |
| --- | --- | --- |
| license後の客観処罰 | court conviction / ExecutorRefutedがReceiptLicensedをCourtFraudとしてvoidし、producerを処罰する [S01]–[S03] | 全対応classで第三者がその証拠を取得・作成できること |
| FalseValid | PanelFalseValidV2により署名範囲内の不正を責任追及する [S04] | 新しいconstraint scopeとterminal faultの対応 |
| court開始 | challenger bond、署名、期限などを検査する。Panel承認を条件にしない [S05] | non-seatの長い局所化をFinal前に完了させる有界の予約 |
| seat replay filer | mismatchからStepArithmetic/ExecutorRefuted、または名前付きStepLeaf要求へ進む [S06] | non-seat/operatorにも同じ証拠作成能力を提供する |
| operator自動監査 | replay mismatchを検出してDA告発する。整合したgarbage/borrowed traceはrow-0開示に答え得る [S07]–[S08] | errorログとDA告発で終わらず、計算不正の証明へ接続する |
| 8k held/fused | whole-capture cap、偽foldのprefix、committed fused tile取得にgapがある [S09]–[S10] | retained-level localizationとcommitted witness開示 |
| IR/pipeline DA | TirStepNode/TirRowNode/PipelineStepNode等の階層開示がある [S11] | 利用可能なものは再利用し、held/fusedの欠けた意味だけ拡張する |
| FinalとDA | 現行non-seat DA sessionはclaimをpauseしない [S12]。Final floor/horizonのhelperは既にある [S16] | 受理された不正追及とFinalの競合を合意規則で閉じる |
| court先取りとdirect proof | 現行ExecutorRefutedはopen courtがあるとClaimUnderSessionで拒否する [S03] | 別bondのsession存在だけで客観証拠を排除しない。共有進捗と決定的なsession清算 |

現行court terminalが成立しても、ノードのRAM上限・ネットワーク上限・証拠取得不能で
filerがそこへ到達しないならDispute Completenessは満たさない。
`ServedView`で攻撃側の実行インスタンスを代役にした成功テストは、fresh verifierの証拠取得を
証明しない [S10]。

## 2. 安全性の定義と仮定

### 2.1 検出後の条件付き不変条件

claim cについて、以下が全て成立する場合を対象にする。

* cのclass/plan/kernelがDisputeAdmissibleで、対象の故障が承認済み検証関係に含まれる。
* 少なくとも1人の正直なverifierが開始期限までに不一致を発見し、正しいchallengeを提出する。
* chainが進行し、適格なcarrierは宣言されたinclusion bound以内に受理される。
* producerは定義されたdeadlineまでの開示義務を持ち、開示の有無を合意上判定できる。
* collision resistance、署名、commitment bindingと、適用suiteのchallenge/soundness仮定が成立する。
* planの最悪時CPU・RAM・I/O・bytes・roundsと担保条件を満たす。

このときMUST:

~~~text
accepted mismatch pursuit
    ├─ authenticated material is served
    │      └─ bounded localization → exact verdict
    │             ├─ proven claim fraud → void + producer liability
    │             └─ claim upheld / accusation dismissed → normal lifecycle
    └─ required material is not served
           └─ objectively established DA/default → defined obligation penalty

unresolved accepted pursuit → economic Final is blocked within its bounded deadline
~~~

有罪判定や適格なDA defaultに至ったclaimはFinalへ進めない。
正直なproducerへの不正なchallengeは、有界の時間と費用で棄却され、claimの時計を無期限に延長しない。

Panel全席が悪意でも、この処理にPanelの追加署名・投票を要求しない。
ただしPanel captureへの耐性は、BlockDAG合意・ネットワーク・carrier inclusionまで
攻撃者が支配した状態への耐性を意味しない。

### 2.2 確率的検査の残余リスク

ADR0171/RFC11の通常検査は条件付きの `epsilon_check` を持つ [S21]–[S22]。
完全検証能力を持つhonest verifierがいても、そのverifierが確率的検査だけを実行する場合、
未検出の不正がFinalする確率を0とはしない。

`Pr[undetected false claim]` の算定は、全constraintを対象にした承認済みsuite、
post-commit乱数、round transcript、adaptive/grinding/abortと繰返し回数を含めて行う。
`epsilon_check <= 2^-128` はRFC11の条件付き設計目標で、現行ネットワークの実測保証ではない。
Panel多数派が署名したという事実からこの暗号学的boundを導いてはならない。

legacyの少数step samplingは完全constraint検査ではない。
ADR0098の300-token / N=299 / k=4という過去の条件では、独立したhonest checkerが1人なら
単一点不正の検出は4/299、約1.34%にすぎない。3人の約3.96%、5人の約6.51%を、
悪意seatを含む署名数から読み替えてはならない [S19]。

### 2.3 公開性と検出責任

claim discoveryは全ての適格producerを対象にし、genesis operatorのclaimも除外しない。
seat/non-seatで参照できるroot、DA unit、terminal courtを変えてはならない。
verifierが何も提出しなかったことは、「不正なし」という積極的な証明ではない。

一人のhonest verifierが全claimを監視できるという運用仮定を置くなら、
classごとの最大claim rate、同時追及数、検査時間、通信量、必要担保を測定する。
新しいbondを大量に作る攻撃でこの資源仮定が崩れるなら、claim admission/rateに合意上の上限を設ける。
ノードのローカルqueue上限だけでネットワークの安全性を主張しない。

## 3. Dispute Completenessをclass admissionへ追加する

### 3.1 提案するimmutable descriptor

新形式classは、仮称 `PalwDisputePlanV1` のdigestをclass identityとclaim bindingに含める。
旧classの意味やIDを後から書き換えない。descriptorは有限で型付きの宣言データであり、
uploadされたnative code、WASM、script、任意VMを実行させない [S23]。

~~~text
PalwDisputePlanV1 (proposed)
    version, kernel_id/version, verification_suite_id/version, court_suite_id/version
    program/model/artifact/task/context/layout identities
    commitment_layout_id, locator_template_id, witness_schema_id
    input/state/weight/output/proof relation coverage
    approved localization transitions → exact terminal kernels
    claim-specific DA/retention policy + permitted unit/cumulative scope, canonical evidence encodings
    immutable model identity + optional distribution descriptor binding (section 16)
    max rounds, total bytes, carrier bytes, checker work, RAM/storage
    cold-fetch/verify/localize/disclose/court/inclusion deadline envelopes
    soundness profile, challenge_policy_id, challenge_binding_policy_id
    public commitment/source/lock/transcript reconstruction and retention schema
~~~

registrantが記載した「complete=true」や宣言soundnessを信頼しない。
合意checkerは、既存kernel templateへの適合、全relationのcoverage、全分岐のterminal reachability、
数値範囲・layoutと最悪時資源を検査する。許可されない演算・courtが必要なら
`KERNEL_EXTENSION_REQUIRED`として待つ。

`challenge_policy_id`は[RFC07 Part VI](0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)のimmutable policy digestである。`challenge_binding_policy_id`はそのpolicy内の承認済みsubject binding schemaを参照し、別のbeacon/seed方式を作らない。`PalwDisputePlanV1`がpublic-prosecution planを兼ねる。actual beacon・queries・transcriptは後続の認証evidenceであり、class identityを変えない。

### 3.2 admissionで必須とする五つの性質

1. **Binding:** job/input/output/weight/state/witnessと、その位置・寸法・kernel意味が
   一意のclaimに束縛される。別jobの本物のtraceの借用では成立しない。
2. **Independent access:** producerの秘密鍵、heap、debug state、攻撃注入instanceなしで、
   rootに認証された公開materialまたはその合意上の開示を得られる。
3. **Localization:** 承認済み検査の不一致を、宣言されたbytes/work/roundsでterminalへ落とせる。
   GKR/Freivaldsのfailureから不正leafが自動的に判明するとは仮定しない。
4. **Exact terminal:** terminalは有効化済みkernelの整数・量子化・順序規則で決定的に裁定される。
   giant model全体の緊急replayを隠れた依存にしない。
5. **Withholding closure:** 必須unitを回答しない場合も、適格なon-chain要求とdeadlineから
   DA/defaultへ進める。challengerのHTTP失敗だけでproducerをslashしない。

admissionは既存の `palw_class_admission_v2.rs` / `palw_tir_admission_v1.rs` の
資源・court検査 [S14]–[S15] と結合する。単発E2E試験だけで全入力の完全性を認定しない。
承認済みtemplateの構造的根拠と、最悪形状・境界・敵対条件の独立試験が必要である。

### 3.3 class lifecycle

提案する `DisputeAdmissible` はadmission capabilityであり、現行claim phaseではない。
不足があるclassは登録・研究用に保持できても、当該新経路で報酬またはwork weightを得ない。

~~~text
RegisteredUnrewarded
    → semantic / coverage / dispute / resource admission
    → DisputeAdmissible
    → fixed candidate commitment + RFC07 Beacon Conformance / ConformancePassed
    → model/bond allocation snapshot + production budgets (§16; no acquisition gate)
    → ordinary readiness and licensing rules
    → reward-bearing / weight-bearing claims
~~~

登録・採掘のどちらでもmodelのdownload/seedを合意上義務付けない。classの裁定能力を検証する試験は、
検証者が正しいmodelを得られた条件で実施する。claim固有証拠のscopeとFinal/court条件は維持する。

現行の「held」は計算・保持形式にも使われるため、「安全審査待ち」の意味として流用しない。
既存reward classをこの文書だけで停止しない。移行判断は§11の別upgrade規則で行う。

### 3.4 G14に含めるpost-commit challenge再現性（2026-10-08）

新しいreward-bearing profileは、RFC07 Part VIのpolicyとRFC11 §17の三段階をbindする。
semantic/constraint/court/public-material/resource admission、Beacon Conformance、Active Eligibilityを分ける。
beacon samplingで未知演算を承認せず、ConformancePassedだけでG14や報酬資格を成立させない。

fresh non-Panel public bondは公開chain/materialから次の全経路を再現できなければならない。

```text
class / Kernel / verification / dispute / challenge policy resolve
-> canonical statement commitment and its position
-> future source eligibility / independent Final / settlement evidence
-> canonical ordered source set and branch-relative lock
-> RFC07 seed, identical queries/vectors and every interactive transcript round
-> authenticated mismatch localization -> exact court / appropriate DA default
```

producerの内部cache、非公開API、Panelのseed配布、operator署名を使わない。seed・epsilonをexporterから信頼せず再計算する。
original claim/sliceチェックの再現は元のsubject/seedを使い、`PUBLIC_PROSECUTION`で有利な別drawへ切り替えない。
追加検査はRFC07の明示的なsupplemental commitment/coverage規則に従う。有効な既存exact proofの提出に新beacon待ちを要求しない。

probabilistic mismatchはconvictionではない。statement/source/transcriptのbindingを確認し、有界の認証witnessへlocalizeし、
exact terminalから客観的convictionへ進む。未開示は適格なDA/default、honest claim/誤challengeは棄却で区別する。
同じseedを多数watcherが検査しても独立repetitionとしてepsilonを人数分掛け合わせない。

post-commit順序、qualifying future work、非循環source、grinding/retry/withholding/reorgのbounds、公開のquery/transcript再現と
exact escalationを、全許可profileについてG14 evidenceに追加する。cold verifier、restart/IBD/pruning、changed roots、偽source、
source不足、GKR後出しmessage、Final阻止・回収まで実nodeで試す。元の実装結果をこの追加gateの合格と扱わない。
source不足ではcandidate/verificationをpending/既定deadline扱いにし、heartbeat・BASE-0・取引・chainを止めない。
no BFT/DNS beacon、no seed timeout fallback、no新finality overlayを維持する。

## 4. 階層commitmentと独立した局所化

### 4.1 認証するtree

意味上はclaim → stage/segment → retained block → step/kernel transitionへ降りる。
入力・重み・entry/exit state・output/proof relationのrootもclaim bindingに含める。
既存bindingにあるrootは再利用し、無関係な第二のexecution rootを作らない。

IRの `TirStepNode` / `TirRowNode`、pipelineの `PipelineStepNode` / leaf開示を
使用できるfamilyでは再利用する [S11]。held foldには、そのretained levelで開示できる
認証frontier/nodeを追加する。leaf数・padding・level/index・child ordering・domain separationは
kernel/commitment versionで固定する。

### 4.2 比較と降下

challengerは自分の正しい計算・承認済み検査と、producerがcommitした認証nodeを比較する。
不一致nodeだけを降下し、名前付きrange/leaf/primitiveへ到達する。
served foldを「正しい計算で再生成してから照合」することを、偽foldの読み出し条件にしない。

原点のinput/entry stateと区間境界のstate continuityも対象にする。
偽checkpointから両者が同じ誤りを再現することを正しさとしない。
後段の計算が正しい場合は、最初の虚偽boundaryまたはそれを生成したrelationを裁く。

locatorがproducer全traceのdense展開や、retained treeにないflat prefixの再生成を必要とする場合、
そのplanは資源admissionを通さない。root/leafの比較だけなら階層的な降下が可能でも、
独立検査・witness生成の全コストが対数になるとは主張しない。

### 4.3 保持するmaterial

全dense traceを全verifierのRAMに保持することは要求しない。
ただしproducer/DA提供者は、宣言したretention期間、committed tile・認証boundary・court operandを
回答できなければならない。hash-only foldは必要なpreimageの可用性を代替しない。

disk-backed/streaming/chunked実装を許す。再計算でwitnessを復元する場合は、
commitした実データをdeadline内に再現できる必要がある。
攻撃側の「間違った実行方法」を知るfresh verifierによる再生成を期待しない。

## 5. held/fusedのDA gapを閉じる

### 5.1 retrievalとadjudicationを分離する

現行のfused `StepLeaf` は `DaUnitNeedsDissection` で拒否される [S09], [S12]。
これを単純に拒否解除して、巨大fused計算をone-stepで再実行させてはならない。

提案unit `CommittedKernelWitness` はまず「何をcommitしたか」を有界に開示する。
既存enumのtag・古いobjectの意味は保持し、追加variant/encodingは別fence以降に限定する。

~~~text
CommittedKernelWitness request (proposed)
    claim_id, dispute_plan_id, kernel_site, witness_part_index

CommittedKernelWitness answer (proposed)
    committed output tile / witness preimage
    membership opening to the claim's bound roots
    kernel coordinate, shape, layout and entry-state references
    authenticated query / state / operand references
    deterministic dissection root and bounded witness-part commitments
~~~

query/operand全体が大きい場合は、bounded partとそのroot、既存held dissectionへの参照に分ける。
単一tileの数KB〜数MBというサイズを一般保証しない。
全sessionのbytes・CPU・RAM・roundsと最大carrierを別々に検査する。

### 5.2 回答の処理

* root/位置/jobに一致しないbytesは適格回答にならない。未回答義務は所定deadlineまで残る。
* 正しいmembershipを持つ回答はDA義務を満たし、計算の正しさはexact courtへ渡す。
* bounded one-moveで裁けるものは既存の束縛された判定 [S13] を再利用する。
* fusedでdissectionが必要なら、開示済みcommitted tileと認証operandを起点に既存held courtへ進む。
* 回答がない場合は客観的なDA/defaultを処理する。計算fraudの証明として記録しない。

commitされた不正が問題のsiteにあり、正直なchallengerがそこへ到達した場合には、
producerは「偽tileを開示してexact裁定を受ける」か「必須tileを開示せずDA/defaultになる」。
正しいtileを回答した場合は棄却する。この二択を全siteが不正だという意味に拡張しない。

### 5.3 exact semantics

fused attentionの分解は、元kernelの丸め・saturation・整数範囲・accumulation順序を維持する。
非結合的な演算を分割したせいでhonest outputと違う値になるdissectionは禁止する。
input・weight・state opening、位置とshapeをclaim bindingまで検証する。
kernelが未対応、またはterminalが上限内に収まらないなら、そのplanはDisputeAdmissibleにしない。

## 6. seat/non-seat共通のfraud filer

### 6.1 仮称 palw_fraud_filer

node policyの共通engineを設け、Panel seat、genesis operator、通常bond holder、
外部watchdogが同じ証拠取得・局所化・提出builderを呼ぶ。

~~~text
Discover → ResolvePlan → Check
    → Mismatch → ReservePursuit → Localize
    → RequestCommittedWitness → JudgeExact
    → SubmitExecutorRefuted / PlayBoundCourt
    → Convicted / Dismissed / DA default / Expired
~~~

Mismatchはローカル状態、ReservePursuit以降の受理・deadlineは合意状態である。
ローカルchecker failureやresource refusalをon-chain fraudと取り違えない。

engineはpublic claim/material/dispute read、capability/resource ledger、transport/carrier builder、
既存court/conviction builderへのadapterを受け取る。
roleごとに変えてよいのはclaim discovery・優先順位・ローカルbudgetであり、
証拠の有効性・terminalへ進める権限・root情報の公開性ではない。

### 6.2 現行moduleからの接続

1. `palw_filer_replay.rs`のclaim mismatch、名前付きleaf、capture候補、conviction doorをadapterに分離する。
2. `palw_operator_da.rs`のRefuted verdictを同じengineへ渡す。
   一貫した偽traceをrow-0 DA告発だけで処理済みにしない。
3. operator向けのproducer/signer filterを、engineの許可条件にしない。
   任意の適格producerのclaimをpublic readから追及できるようにする。
4. witness取得、binder/job認証、held/TIR/pipeline terminalのfamily adapterを共通契約へ接続する。
5. legacy DAの開示要求は独立機能として保持する。計算不正と結びつかないDA要求を、
   自動的にFalseValidのbasisへ変換しない。

fee払いcarrierの既存ObjectiveOffence提出が許される範囲は狭めない。
新しいbond/depositが必要なのは、相手とchainに開示・予約・対話資源を消費させる経路である。
direct proofとinteractive challengeのentry条件を明確に分ける。

### 6.3 node/RPCと再起動

public readはclass/plan identity、binding/root、retention、開始/Final deadline、
未処理要求、answer参照、pending court、既存convictionを返す。
秘密のoperator allowlistを知ることを証拠取得条件にしない。

nodeはclaim/site/plan別に進捗、material digest、提出carrier、deadlineを保存し、
再起動時はcanonical chainからsessionを再照合する。
reorgで無効になったopeningやreceiptを新しいrootの証拠として再利用しない。
同じ有効proofを再送してもproducerを二重徴収しない。退出・担保開放とconvictionの順序もstate foldで固定する。

## 7. non-seatの追及予約とFinal

### 7.1 現行との差分

現行R-core+のnon-seat DA sessionはclaimをpauseしない [S12]。
このままでは、外部verifierが局所化中にchallenge windowが終わり得る。
本RFCは、適格な外部追及についてbounded reservationを合意状態として追加する。
これはnode policy変更だけでは達成できない。

### 7.2 仮称 PalwDisputeReservationV1

開始条件はbond資格、deposit/exposure、claim/plan/siteのbinding、retention、
開始cutoff、session数と合計資源の上限、署名・feeを含む。
初期mismatchをまだexact proofにできないケースも対象にするため、開くこと自体を有罪認定にしない。

最初の受理でclaim単位のabsolute `dispute_hard_deadline` を固定する。
別bondの新session、再送、同unitの重複要求、席交代で時計を再開始しない。
同じsite/unitは共通回答を再利用し、answered unitへの新しい資源要求は既存同様に拒否する。
複数追及は同時数・総bytes/work・総延長に合意上のcapを設ける。

開始には次の余裕がMUST必要である。

~~~text
start + B_cold_fetch + B_check + B_localize + B_disclose
      + B_court + B_carrier + B_reorg_slack
    <= dispute_hard_deadline
~~~

最悪round数・返答時間と最小carrier処理能力からclass/plan別のDAA envelopeを導く。
秒のbenchmarkをDAAへ変換する際は、実際のnetwork clock/inclusion仮定を明記する。
「1人がwindow最終瞬間に見つけても必ず間に合う」とはしない。

producerが適格なunitを開示しない場合は、そのunitのdeadlineでDA/defaultへ進む。
challengerが義務を果たさなければ棄却・deposit処理を行う。
各期限を満たしたhonest pursuitが、新規spamによりcourtの順番を永遠に失わないqueue/resource規則も必要である。
bounded予約は無制限pauseを導入するための例外ではない。

### 7.3 Final条件

新経路のFinalは少なくとも次のAND条件を満たす。

~~~text
DisputeAdmissible class/plan/kernel
AND immutable model identity + network/model/producer-bond budgets satisfied (§16)
AND required positive verification evidence / coverage accepted
AND permitted claim-specific evidence / DA / court obligations satisfied
AND ordinary challenge + class verify horizon expired
AND no unresolved accepted pursuit / required court
AND plan-specific settlement and unique-work accounting satisfied
~~~

`palw_claim_final_floor_v1`などの既存helper [S16] を拡張し、
全Final writer、deadline sweep、vesting/work-rights readerが同じpredicateを見る。
UI上のFinalだけを遅らせ、reward/weightを先に確定させる実装は禁止する。

Panelの3票はReceiptLicensedへ進める条件の一部であって、客観的正しさの証明ではない。
required positive evidenceの存在を検査し、欠落をsilence=passにしない。
REAL rootのpre-Final weightとclaim-backed sliceの最大損失は改定RFC08 [S25] と整合させる。
EXEC slice自体はFinal前後ともfork-choice weight 0だが、pending reward・collateral・DA・court exposureは有界化を必要とする。
root単位の一回のFinal/精算とし、reservation/convictionで重複creditが発生しないようにする。

期限後の未検出不正について、新しい無期限rollbackを導入しない。
現行のpost-Final liabilityは保持期間・担保・適用fenceに従い、
本RFCのpre-Final予約とは別に扱う。

### 7.4 sessionの先取りで正直なverifierを排除しない

現行ExecutorRefuted adjudicatorはopen_courts_of > 0でClaimUnderSessionを返す [S03]。
既存の単一session規則をそのまま安全性の前提にしてはならない。
悪意challengerが先にcourtを開き、本物の不正証拠を受理不能にする競合を新fenceで閉じる。

新経路では、既に組み立てられた適格なdirect objective proofは、
別bondのcourt/sessionが開いているという理由だけで拒否しない。
gateとstate foldの両方を変更し、同じclaim/root/rulesetに対する証拠を完全検証した後、
conviction・未処理session終了・担保/exposure清算・deadline解除を一つの決定的transitionとして行う。
court中の発言だけで他sessionを消すことは許さない。

予約はclaimの排他的な所有権ではない。認証された開示は全参加者が再利用でき、
別の適格なverifierも必要なprogressまたはexact proofを提出できる。
同じsiteで最初にsessionを開いたbondだけが次の証拠を提出できる仕様を禁止する。
開示済みの正しいunitを使って別siteの不正を追う場合も、claim単位のabsolute deadlineは維持する。

session cap・deposit・feeだけでは、資金無制限のSybilによる資源独占を防いだとはいえない。
interactive枠の選択、共有progress、direct-proof優先処理と総work capを定義し、
先取り・全枠占有・別site誘導の試験を必須にする。
それでも必要carrierがinclusion bound内に受理されない状況は§2の仮定外であり、
「一人いるだけで安全」という無条件保証に読み替えない。


## 8. challengerの経済条件と処罰

* interactive開示・court・予約には、適格なpublic bondとsession deposit/exposureを要求する。
  genesis membershipやPanel seatであることを要求しない。
* 適格なfraud verdictではdepositを返し、既存reporter/court rewardの原則に従ってbountyを処理する。
* false accusation、challenger default、未認証materialだけの申告は、それぞれ定義された費用を課す。
* malformed objectをdecoderで拒否することと、受理済みsessionのdepositをburnすることを区別する。
* 代数的検査のローカルfailureだけではproducerをslashしない。
  objective exact verdict、証明された証拠/義務違反、または定義されたDA/defaultが必要である。
* producer処罰には現行conviction funnelを再利用する。reservation・escrow/rights・S2/S3等の
  applicable tierを維持し、新しい別経路で`claim.reserved`だけの処罰へ弱めない [S17]。
* bounty/徴収ledgerはnominal amountと実際のcollectedを区別し、未回収額を支払財源にしない。
* classの最大gain、同時claims/予約とlock lifetimeを結び付ける。
  proof受理前に担保が開放される、攻撃がdepositより大きな無償検査を強制する、といった経路を試験する。

DA/defaultは可用性または義務違反の処理であり、それだけで全Valid signerの計算fraudを証明しない。
court default、held verdict、execution-proving fraudのbasisは現行の区別を維持する。
producer convictionに全seatの別件処罰が完了することを要求しない。

## 9. Valid receiptの検証範囲

現行 `PanelFalseValidV2` のscope原則 [S04] を、新constraint receiptにも適用する。

~~~text
receipt binding (new scheme; conceptual)
    claim_id, class/plan/kernel/suite version
    challenge and transcript commitment
    attested relation ids, segment/cell/boundary scope
    input/output/state roots and verification assurance
~~~

receipt scheme/versionごとのcoverageは合意checkerが導く。
registrant/seatの任意文字列「全計算を検査済み」を信頼しない。

FalseValidの裁定は、faultがそのreceiptの実際のattestationと矛盾するかを検証する。
partial seatは受け取った認証boundaryからの区間計算に責任を負うが、
別区間が生成したboundaryそのものの正しさを署名していなければ、その嘘で処罰しない。
arithmetic faultでは必要なoutput/input/siteがscope内にあることを確かめる。
whole-output/identity/constraint-proof faultには、それぞれ対応するscope規則を定義する。

確率的検査が仕様どおり実行され、許された残余確率で通過したという事実だけでは、
故意の嘘や検査義務違反を証明しない。
新schemeは「関係そのものを保証する署名」と「特定検査transcriptの正しい実行を保証する署名」を区別し、
どの客観証拠でslash可能かを明文化する。
legacyのfull/partial receiptを新しい保証へ読み替えない。

## 10. ADR0171との接続

通常時の巨大モデル検証はADR0171のencoded/algebraic constraint方針を使う。
Panelが毎回whole inferenceまたはwhole segmentをreplayする方式へ戻さない [S21]–[S23]。

MatMul、量子化、nonlinear/range/lookup、routing、memory/state、境界と出力を
承認済みkernel relationで覆う。登録されたplanの一部relationだけを検査して全claimのboundを名乗らない。
check failureからexact courtへ移るlocalizerも同じplanのadmission対象にする。

~~~text
approved probabilistic checks
    → authenticated relation disagreement
    → bounded localization / witness disclosure
    → exact terminal verdict or defined obligation default
    → applicable conviction funnel
~~~

field mismatchだけを計算fraudとしない。未認証のwitness、非canonical整数/field変換、
faulty verifierなどを切り分ける。
証明protocolの違反を処罰する場合も、定義された客観的な違反証拠を必要とする。

VM fallback、TEEの正しさへの信頼、BFT運営委員会の裁定、trusted committee beaconを追加しない。
未知のrelationはversioned kernelのreview/activationを待つ [S23]。
Panel選択乱数とalgebraic proof challenge乱数は別domainで、RFC10の公開性だけから
後者のunpredictabilityを推論しない。

## 11. wire・合意移行

node-only変更で既存証拠の提出能力を共有する段階と、
新DA unit・admission・reservation/Final条件を変える合意段階を分ける。

合意段階ではMUST:

1. descriptor・class/claim identity、request/answer、receipt、courtのversionと署名domainを固定する。
2. 新unitのcanonical encoding、field bounds、chunk/root bindingと最大object/transaction bytesを定義する。
3. state carriage/root、delta apply/revert、IBD、pruning、RPC/SDK/manifest/fingerprintへ反映する。
4. claimのbind/admission時点のrulesetを固定し、in-flight旧claimは旧規則で完了させる。
5. 未対応nodeのunknown-version successを禁止し、協調upgrade前にshadow検証する。
6. 既存classの新descriptorは新identityとして再admitする。
   現行にgapがあるclassの新規報酬claimを止める必要がある場合は、別途公開した移行policyとfenceで実施する。
7. post-Final liability/担保保持の長さをhard deadlineと整合させ、途中でreleaseして逃げられないことを確認する。
8. §16の固定model identity、distinct miner元本、配分snapshotとnetwork/model/bond予算を
   state/root/carriageへ含める。claim固有courtの許可unit/累積scope、Finalと責任解放を規範化し、
   同じ認証済みobjectから同じ結果を得ることを確認する。model取得・peer状態は合意入力にしない。

RFC10のoperator-anchor撤去はこのRFCから暗黙に有効化しない [S20]。
公開fraud filerを先に作れても、permissionlessなPanel seedの安全性は別のactivation gateである。
新classの公開registration/報酬適格性を広げる前に、Dispute Completenessを満たす必要がある。

[RFC-0015][R15]のoptimistic panel-free modeは、将来の別fenceでpositive Panel receiptを必須条件から外す。
そのmodeが全activation gateを満たして有効化されるまでは、§7.3のpositive evidence/Final条件を維持する。
旧claim・旧modeの条件はRFC15によって遡及的に緩めない。

## 12. 実装対象と段階ごとの出口条件

| 段階 | 主な変更先・仮称 | 完了条件 |
| --- | --- | --- |
| P0: 契約・test固定 | dispute plan/template、既存T54g/T46・resource vectors | served captureの代役とfresh outsiderの差を再現。未対応familyを明示 |
| P1: held証拠の開示 | palw_da_rcore_v1、palw_held_da_v1、palw_shard_court_v1、palw_state_v2 | fused committed tileを認証取得し、返答/未返答の両方を合意処理 |
| P2: bounded局所化 | held retained tree、既存IR/pipeline node disclosure、palw_filer_held | canonical 8k条件でdense cap/prefix/tileの三gapを閉じる。小geometryだけで完了扱いしない |
| P3: 共通filer | proposed kaspad/src/palw_fraud_filer.rs、palw_filer_replay、palw_operator_da、RPC/SDK | seat・non-seat・operatorで同じgarbage/borrowed traceをexact proofへ変換 |
| P4: admission・Final | class/TIR admission、state deadlines/locks/sweep、proposed reservation | 完全性未達classは新reward対象外。外部追及でFinalを有界にblockし、spamで無限延長しない |
| P5: 大型constraint suite | RFC07/RFC11/ADR0171のkernel/checker/receipt/localizer | 全constraint、条件付きsoundness、独立witness取得、最大court負荷が測定・審査済み |
| P6: permissionless rollout | RFC10 beacon/binding、network schedules/fingerprint | genesis operatorなしの新参加者が登録・claim・外部追及・settlementを完結 |
| P-A: モデル資本・coinbase配分 / 不介入 | §16、distinct capital snapshot、versioned allocation/accounting、RPC/SDK、claim-specific court scope | network/model/bond予算保存、取得結果からの独立性、公開対閉鎖の経済評価とrecovery/移行を独立に検証。旧Transport統合・Seeder/取得gateは廃止 |

RFC15のPanel=0実装・有効化は本RFCの完了条件に含めない。依存はRFC14のpublic prosecution完成から
RFC15へ一方向とし、RFC14完成だけでPanel数・quorum・既存lifecycleを変更しない。

P1/P2のnode testとconsensus testは同時に設計し、P4のgateがない状態で
新証拠形式に依存するclassをreward-bearingにしない。
既存objectだけで成立するP3のoperator→proof接続は、合意変更と独立に進められるが、
全class対応や新Final保証の達成とは呼ばない。

## 13. 必須の敵対的試験とrelease gate

### 13.1 一人のfresh outsider

producerと3/4/5 malicious seatsが偽claimをlicenseする。
正直なverifierはPanel外の通常public bondのみで、producerのheap/鍵/drill instanceを持たない。
正しい固定modelを保有/任意配布から取得した条件で、public claimと認証されたclaim固有materialから処罰まで進む。
全保有者拒否時のmodel取得はこの試験の保証に含めない。

少なくとも以下を実行する。

| ケース | 必須結果 |
| --- | --- |
| junk root・materialなし | 適格なDA要求 → default、producerの該当義務を処罰 |
| 自己整合したgarbage trace | row-0が開いても処理を終了せず、局所化 → exact fraud verdict |
| 別jobの本物trace | job/input/binding不一致を客観証明。貸し元のhonest executionを誤って没収しない |
| 単一step/token、境界、checkpoint、routing/stateの改ざん | planに定義したfaultへ到達。sampling漏れは検出成功と数えない |
| canonical 8k held/fused | fresh verifierで三gapを閉じる。ServedView代役を禁止 |
| verifierが巨大captureをRAM展開できない | 宣言されたstreaming/retained-level経路で完結 |
| producerが異なるchunk/root/shape、偽operandを回答 | 認証拒否または定義された客観違反。未認証bytesでhonest producerを有罪にしない |
| 正しいproducerとmalicious challenger | 棄却・deadline/費用処理後にFinal可。不要なarithmetic slashなし |
| partial seatと他segmentの嘘 | scope外seatを処罰しない |
| 無応答・偽回答・court turn default | 各basisを区別し、producer/seatを誤帰属しない |
| non-seat予約spam、複数Sybil、既回答unit | 同時数/総資源/hard deadlineを守り、正当追及の進行も確認 |
| 悪意bondが先にcourtを開き、枠を占有 | direct objective proofを排除せず、別verifierの共有progressとsession終了を確認 |
| restart/reorg/重複proof/Final境界/担保退出 | canonical sessionを復元し、二重slash/bounty/weightと時計再開始なし |

### 13.2 検証レベル

合意unit/fold試験だけでは完了にしない。
署名/object gate → mempool/fee/carrier inclusion → acceptance walk → state fold →
deadline/Final/reward/weight/担保 → restart/IBD/reorgまで実行するnode E2Eが必要である。

source/model/layout、kernel/plan/suite、network/fence、hardware、class/job/claim、
artifact digests、bytes/RAM/storage/work/DAA、実際のcarrierと裁定を記録する。
reduced fixtureの成功をcanonical rowの成功と呼ばない。
暗号学的soundnessと全入力の到達性はreview/templateの根拠で確認し、有限のtest件数だけから保証を導かない。

### 13.3 activationを拒否する条件

次のいずれかが残れば新経路を有効化しない。

* producer内部instanceを使わないと証拠が作れない。
* DAに答えられる偽traceを、対応planの外部filerが有罪証拠へ変換できない。
* localizerが全dense capture、無制限replay、未認証boundary、未有効kernelを必要とする。
* non-seat追及中にFinal/報酬が確定する、またはchallenge spamが時計を無限に延長する。
* bytes/work/rounds/inclusion/deposit/retentionのworst-case boundが未定または実測条件で不成立。
* 固定model/root照合、distinct元本・model配分・三層予算が不成立、または取得成否を資格/weight判定へ持ち込む。
* claim固有証拠/court scope・認証/default・Final/責任解放が未定、または公開対閉鎖の経済条件が未評価。
* legacy claim、旧receipt、state root/fingerprint、IBD/reorgの意味が変わる。
* 大型constraint suiteのcoverage/soundness/localizationが未審査なのに大型対応済みと表記する。

## 14. baseline確認記録と未決パラメータ

2026-10-07の前段コード確認では、baselineで次を実行し45件成功した。

~~~sh
cargo test -p kaspa-consensus-core --locked --offline --no-default-features \
  --test palw_operator_da_candidates --test rcore_m3_da_court
# 3 + 21 passed

cargo test -p kaspa-consensus-core --locked --offline --no-default-features \
  --test palw_tir_court --test palw_court_decode_close_door \
  --test rcore_s4_conviction_funnel
# 11 + 1 + 9 passed
~~~

これは既存機能の確認で、新しいDispute Completeness、新DA unit、共通filer、予約規則の
実装成功ではない。T46 [S18] はsource確認のみで、この記録では実行していない。
T54gのfresh-seat gapもsource確認である。
本RFC作成中に稼働ネットワークへclaim/challengeを送信したり、upgradeを適用したりしていない。

activation前に別途固定する項目:

* commitment/locator/witnessのcanonical wire schema、domain、tag、version。
* familyごとのmax material/carrier/total bytes、rounds、checker workとRAM/storage。
* class rate・同時session・duplicate/cache範囲とscheduler fairness。
* bond/deposit/exposure、実費とbounty、lock/retention/hard deadlineとnetwork inclusion envelope。
* encoded constraint suiteのparameterと全claim soundness accounting。
* independent implementation、template review、実network E2Eとmigration/fingerprint evidence。
* §16のimmutable identity、重複のないmodel元本、coinbase配分/残余、claim固有court scope、
  個別bond予算との接続、公開/閉鎖の経済評価と条件付き実効検出率。

これらが未定の間は設計・実装計画であり、「1 honest verifierで任意モデルが安全」の
稼働保証として利用しない。

## 15. Pinned source references

現行コードへの参照は全てsource baselineへ固定する。
手元の古いcheckoutに同名fileがない場合でも、別版のコードを参照して設計を解釈しない。

* **S01** — [court verdict → conviction funnel][S01] — `consensus/core/src/palw_state_v2.rs`.
* **S02** — [licensed/Final claimへの処理][S02] — `consensus/core/src/palw_state_v2.rs`.
* **S03** — [ExecutorRefutedの許可証拠と客観判定][S03] — `consensus/core/src/palw_offence_attribution_v1.rs`.
* **S04** — [PanelFalseValidV2と署名範囲][S04] — `consensus/core/src/palw_state_v2.rs`.
* **S05** — [court開始条件][S05] — `consensus/core/src/palw_court_v2.rs`.
* **S06** — [seat replay filerと8k制約][S06] — `kaspad/src/palw_filer_replay.rs`.
* **S07** — [外部operatorの検出とDA告発の境界][S07] — `kaspad/src/palw_operator_da.rs`.
* **S08** — [整合した偽traceがFinalへ進み得るログ][S08] — `kaspad/src/palw_operator_da.rs`.
* **S09** — [held/fusedの三つの証拠取得gap][S09] — `kaspad/src/palw_filer_held.rs`.
* **S10** — [独立seatのgapとServedView代役][S10] — `kaspad/src/palw_filer_held_e2e.rs`.
* **S11** — [DA unit: IR/pipelineのleaf/node開示][S11] — `consensus/core/src/palw_da_rcore_v1.rs`.
* **S12** — [non-seat DAは現行ではclaimをpauseしない][S12] — `consensus/core/src/palw_state_v2.rs`.
* **S13** — [束縛されたone-move判定][S13] — `consensus/core/src/palw_shard_court_v1.rs`.
* **S14** — [class admissionとcourt資源][S14] — `consensus/core/src/palw_class_admission_v2.rs`.
* **S15** — [TIR admissionの資源・court規則][S15] — `consensus/core/src/palw_tir_admission_v1.rs`.
* **S16** — [現行Final floorと検証horizon][S16] — `consensus/core/src/palw_state_v2.rs`.
* **S17** — [没収・追加ペナルティ・slash額][S17] — `consensus/core/src/palw_state_v2.rs`.
* **S18** — [license済みclaimのstep proofによるconvictionテスト][S18] — `consensus/src/pipeline/virtual_processor/tests/t46_false_valid_real_claim.rs`.
* **S19** — [過去のsampling条件と検出確率][S19] — `docs/adr/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md`.
* **S20** — [permissionless bindingの未有効化とoperator依存][S20] — `docs/rfc/0010-permissionless-palw-panel-and-claim-completion.md`.
* **S21** — [確率的検査・exact court・Final条件][S21] — `docs/rfc/0011-permissionless-model-and-long-context-onboarding.md`.
* **S22** — [大型モデルの検査設計、未実装・未有効化][S22] — `docs/adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md`.
* **S23** — [versioned kernel、VM/TEE/BFT代替依存の禁止][S23] — `docs/adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md`.
* **S24** — [constraint receipt、coverage、代数的検査][S24] — `docs/rfc/0007-palw-verification-certificates-and-algebraic-checks.md`.
* **S25** — [claim-backed work slicesとsettlement][S25] — `docs/rfc/0008-palw-claim-backed-consensus-blocks.md`.
* **S26** — [再現可能なlayoutと独立検査][S26] — `docs/rfc/0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md`.
* **S27** — [番号予約: next free RFC-0014][S27] — `docs/rfc/README.md`.

[S01]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L21619
[S02]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L23107
[S03]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_offence_attribution_v1.rs#L1996
[S04]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L22731
[S05]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_court_v2.rs#L333
[S06]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_filer_replay.rs#L5
[S07]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_operator_da.rs#L27
[S08]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_operator_da.rs#L1052
[S09]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_filer_held.rs#L5
[S10]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_filer_held_e2e.rs#L12
[S11]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_da_rcore_v1.rs#L98
[S12]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L29613
[S13]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_shard_court_v1.rs#L341
[S14]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_class_admission_v2.rs#L424
[S15]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_tir_admission_v1.rs#L91
[S16]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L4845
[S17]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L26728
[S18]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/src/pipeline/virtual_processor/tests/t46_false_valid_real_claim.rs#L4450
[S19]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/adr/0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md#L54
[S20]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0010-permissionless-palw-panel-and-claim-completion.md#L3
[S21]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0011-permissionless-model-and-long-context-onboarding.md#L363
[S22]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md#L3
[S23]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md#L9
[S24]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0007-palw-verification-certificates-and-algebraic-checks.md#L1
[S25]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0008-palw-claim-backed-consensus-blocks.md#L1
[S26]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md#L1
[S27]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/README.md#L1
[R15]: 0015-panel-free-permissionless-verification.md

## 16. モデル入手への不介入と拘束bond連動coinbase配分（2026-10-10後続改定）

### 16.1 採択方針と旧方針の撤回

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を正とする。
**チェーンはモデルを入手できるかどうかに一切干渉しない。**
MISAKA Torrentの採用・統合、専用Bonded SeederとSeeder報酬を廃止する。
必須Torrent、独立Bonded Seeder群、PoR、Full Fetch、funded availability lease、TRDC、FPR、
モデル全量の強制公開費用と取得不履行に基づくreward/weight停止を撤回する。
旧7/5コピー、Seeder 15%、監査3-of-5、符号化4-of-7も採択しない。
旧文書チェックのPASSは旧設計の整合記録であり、この後続設計の経済・実装証拠ではない。

### 16.2 配布と合意の境界

一般的なP2P配布・DHT/PEX・mirror・保存/共有契約はoff-chainの任意運用である。
MISAKA固有のTorrent採用や専用Seeder制度・報酬は設けない。
モデル保有者、miner、Panel、一般peerへ公開・Seeder義務を課さず、chainはpeer数・独立性・速度を追跡しない。
全peer停止、公開拒否、ローカル取得失敗だけではclaim受理、報酬、Final weightや担保責任を変えない。
配布の成功/失敗を証言する新委員会・availability oracleを導入しない。

### 16.3 固定モデルとの同一性

登録時にModel ID、Canonical Artifact Root、canonical manifest、全weights、tokenizer/config/実行仕様、
artifact sizeを固定し、変更は別Model IDとして登録する。path/order/encoding/file扱い等のcanonical化を定義する。
claimは固定Model ID/root/specをbindする。取得者はpiece/shardを認証し、全量取得/復元後のrootを登録rootと照合する。
任意のTorrent descriptor/infohashを登録するなら、その固定rootとの対応をimmutable bindingにする。
infohashとartifact rootは別domainであり、infohashの必須登録・Torrent接続を資格条件にしない。
異なるデータを正規モデルとして使用せず、root一致だけでminerの実計算を証明したと扱わない。

### 16.4 モデル資格とclaim証拠のscope

旧六項`ModelRewardEligible`の可用性gateは撤回する。新routeの設計predicateは次とする。
型名は仮称で、合意snapshotから導き、swarmやノードの取得結果を入力にしない。

~~~text
ModelRewardEligible(model, epoch)
    = ImmutableIdentityBindingValid
      AND SemanticConstraintCourtPlanAdmitted
      AND ApplicableKernelModeAndRulesetActive

ClaimRewardEligible(claim)
    = ModelRewardEligible(claim.model, claim.accepted_epoch)
      AND RootBoundClaimCommitmentAndAuthorizationValid
      AND NetworkModelAndBondBudgetsReserved
      AND ClaimSpecificEvidenceCourtAndFinalConditionsSatisfied
      AND UniqueWorkAndNoApplicableConviction
~~~

型付きのclaim固有state/trace/output/witnessのcommitment・保持・客観裁定は維持する。
モデル取得目的のweight-file/range、全量列挙/反復要求をこのcourtに持ち込まない。
許可unit・累積scopeと必要model operandの認証をADR177 D2に従って確定する。
取得できないという申告だけでは適格なcourt開始、void、Slash、反証期間延長にならない。

**実装（lane DA16、2026-10-10、休眠 `palw_provider_court_v1`）:** 全 route の demand／opening／challenge unit を
`consensus-core palw_court_scope_v1` の inventory（`palw_court_unit_scope_v1`）として claim 固有（input/trace/state/output/witness hash）と
model bytes（weights／その複写・関数／file range）に分類し、compel できる unit は claim 固有のものだけとする（`palw_court_demand_allowed_v1`）。
累積 scope は、モデル bytes 開示が要求回数によらず claim・model ごとに 0、claim 固有 unit は claim ごとに 1 回、(claim, 要求 operator) ごとに
16 unit まで（他の要求者の枠を消費しないため G14 の starvation-freedom を保つ）。kernel route では fence 以上で snapshot slice と登録 `M0` の
pre-state を拒否し、retrieval claim 自身の entry（stage `0xC0 + s`）を claim 固有の demand unit とした（malformed／junk leaf に対しても
court が全域で裁定する、G14C GAP-52）。verifier が自分の copy から出す model operand は登録 root（kernel class の `ParamCommitmentsV1` root、
V2 `artifact_root`）に対して `palw_verify_verifier_model_operand_v1` で認証する。node 単位の model-bytes mask（K2S が v4/v5 の owed set で
呼ぶ）、活性値から関係を復元しうる残余（Level 2 は利用者判断）、t12 で武装済みのモデル bytes を compel する route（held `StepLeaf`、
dissection の root claim、readiness）と新しい休眠 fence の提案、各 unit class の G14 到達性は
[DA16 design](../design/palw/da16-transport-and-provider-court.md) §7。状態は implemented（armable ではない）。

### 16.5 重複しないモデル別miner元本

受理済み配分snapshotにおいて、各bondのmodel別適格拘束元本を`C_{b,m}(t)`とする。

~~~text
sum_m C_{b,m}(t) <= C_b_effective_locked(t)
S_m(t) = sum_b C_{b,m}(t)
~~~

claim予約額や件数を単純合算しない。rho・再送・root/slice/rider・key分割で同じ元本を増幅しない。
owner seed/AMM/Position/Panel担保も自動算入しない。slash、退出、責任控除、model移動、期間境界の
資本帰属とsnapshot/積分方式を固定し、短期借入と二重配分を評価する。7日間は未採用の説明例である。

### 16.6 モデル別coinbase allocation

採択済み総PALW発行予算の中で、非負・単調、`f(0)=0`の配分関数を用いる。

~~~text
A_m(t) = f(S_m(t))
R_m(t) = R_PALW(t) * A_m(t) / sum_j A_j(t)   [positive denominator]
sum_m R_m(t) <= R_PALW(t)
~~~

他モデル資本を固定した場合、model資本の増加は配分を非減少にする。
全modelが同時に増資する場合の絶対増額や既存minerの収入増は保証しない。
分母0ならmodel配分0。丸め・未配分/未使用残高、model内leg、old/new会計の処理を規範化し、追加mintをしない。
元本を預けただけでは支払わず、当該modelの有効な計算claimとFinal・予算条件を要する。

### 16.7 個別bond予算との同時制約

ネットワークcoinbase予算、model配分、ADR176の個別Q/B/R/F capを同時に検査する。
公開参加者の資本追加で集合の配分が増えても、既存minerの最大枠と共通`d + W`時計を増やさない。
受理snapshotと最大配分権を固定し、未払escrow・成熟・Final・reversal/readerで同じ帰属を使う。
可変model配分から過去claimへ未予約の追加creditを発生させない。配分だけでblock数・DAA・難易度を変えない。

### 16.8 独立受入matrix

ADR177 §3のnon-interference、identity/court scope、distinct capital、allocation conservation、
same-bond opportunity、open-versus-closed economics、recovery/activationを独立に試験する。
同一chain/proofでpeer停止とlocal取得結果を変えても合意結果が同じになることを確認する。
モデルのavailability監査をactivation gateへ戻さず、実効監視・経済評価の仮定を明示する。

### 16.9 資料と記録の位置づけ

Torrent/PoRの外部仕様は任意transportの資料であり、報酬資格・全量公開・外部所有者識別の証明ではない。
現行subsidy carveは`palw_reward_v2.rs`、DAA emission budgetはADR167を参照する。
本章の`S_m`/`f`配分は未実装で、従来のt12・Torrent/FPR文書PASSを新配分の証拠に流用しない。

### 16.10 規則Eとfork-choiceへの接続

claim固有の適格court/Final条件、暫定creditと不可逆settlementの区別、ADR176のFinal weight上限を維持する。
root/sliceと全writer/reader/undo、partition/reorg/restart/IBDをversioned規則で検証する。
モデル配布不能だけで新たなweight停止を起動せず、閉鎖時の未検出不正を無条件に防げるとは主張しない。
escrow・個別cap・model配分だけでRule E統合が完了したとしない。EXEC_TX/EXEC_SLICEのweight/DAAは0を維持する。

### 16.11 ECON・MEAS: 公開参加の経済優位

公開による外部参加資本の増加が、閉鎖自己資本より圧倒的に有利となる倍率曲線を目標にする。
同額の`S_m`では自己資本と外部資本は同じ配分であり、ウォレット数を独立資本認定に使わない。
`f(S)=S`だけで既存minerの利益増を保証せず、superlinearな曲線の集中/Sybil/予算飽和も評価する。
既存miner/ownerのcap、配布/資本/計算/検証費、競合model、自己増資、借入、全資本自前、
閉鎖による検出率低下、49% reporter還流、共有担保と外部利得を含めて比較する。
優位の数値基準・適用市場/資本範囲を事前定義し、成立しない条件と未立証事項を報告する。
正しいモデルを得られないverifierの実効`p`は0にもなり、`p(R_risk+L_net)>C_saved`を無条件に成立としない。

**2026-10-10改定（bond集約優位）:** 本節の目標はADR-0177の改定に従い、「公開参加の経済優位」から「実拘束bondを多く集めたモデルの優遇」へ変わる。
「同額自己増資より公開が常に有利」の証明は要件から外す。候補`A_m = S_m^α`（`α > 1`、比較`α = 2`）を、個別cap・費用・モデル内配分・資本集中を含めて評価する。
大口資本の優位は残余リスクとして受容する。`p=0`の計算省略は独立の安全性gateとして残す。

### 16.12 未決パラメータとactivation

`f`の形・上限/逓増範囲、model内配分、適格元本、配分時計/snapshot、ADR176の共通`W`との接続、
整数精度・残余、claim固有court scope、検出/包含/徴収仮定、経済優位の閾値、versioned移行を固定する。
wire/fingerprint・全実装・独立試験・別の協調activationまで新modeを有効化しない。
model配分で採掘機会まで変更する追加設計は本coinbase改定から自動導入しない。

<a id="forced-public-retrieval"></a>
### 16.13 Forced Public Retrievalの撤回と検証保証の変更

旧FPR三段階、全量公開義務、モデル配布用重大Slash/上限付きdefault、代替Seeder・公開費予約と
availability起因CHALLENGED weight制約を撤回する。modelを公開しないだけでminer bondを失わせない。
既存claim固有の客観的計算反証・適格court/defaultはそれぞれのrulesetで維持する。
**全所有者が拒否してもpublic verifierが全modelを取得できるという保証は撤回する。**
G14の検証は正しい固定modelを保有/取得できた条件付きとなる。モデル未取得だけではchainは介入しない。
旧TRDC/FPRは未実装・未有効化だった設計であり、現在のG14/activation要件として引用しない。
過去の文書チェックと撤回sectionはADR177の整合記録に履歴として保持する。

## Mission alignment amendment — 2026-10-07

本RFCはMISAKAの中核目標を実現する主実装設計である。各completion gateは普通のpublic bond・fresh verifier・公開transportと認証material・producer秘密状態なしを共通条件とし、kernel単体やliar-internal ServedViewだけの試験で完了扱いにしない。新しいclass/profileの報酬有効化は、この全経路の成立を必要条件とする。RFC15のPanel=0は本RFCの未完成部分が一つでも残る間は有効化しない。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](0014-panel-independent-fraud-prosecution.md)、[RFC15](0015-panel-free-permissionless-verification.md)を参照する。

## Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](0015-panel-free-permissionless-verification.md)を適用する。

確率的検出とpublic non-seat prosecutionに、producer bondが時間内のclaim機会とblock/reward/Final weightの総量を制限する前提を追加する。計算省略で枠を増やさず、客観裁定後は報酬失効と徴収可能担保への責任を執行する。
public challengerの別責任・資源枠をproducer採掘capと同一視しない。正当な追及を包含できる能力、実効検出/徴収率、共有担保枯渇と自己共謀還流を評価し、capだけでG14/Rule E/ECON/MEASを満たしたと扱わない。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。
