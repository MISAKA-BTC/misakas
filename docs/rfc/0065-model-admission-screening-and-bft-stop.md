# RFC-0065 — MAS1：モデル追加時の構造検査・実用性条件とBFT停止

<!-- QF1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — QF1のQwen3具体profile

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0066](0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](../adr/0234-qwen3-fow1-execution-and-readout.md)／[適用・移行](../governance/qwen-fow1-profile-transition.md)。QF1 descriptorでは36層・QK norm・StateMap・HEX64・整数scale・hash chunk checkerを検査する。QwenのG-COLLの定理が未確認、又はG-COSTの反復検査が未完ならUNKNOWNで採掘を保留する。

QF1は36層Qwen3-8Bの整数A16実行、各予測位置の37 vector、HEX64実入力、固定Readout/四Root/Ticketを定義する。G-COLL/G-COST/G-USE、MAS1のUNKNOWN・BFT安全停止、Panel＋Courtを維持する。Qwen実機の品質・費用検査・native有効化は未完であり、旧claimやMOE2経済を変更しない。
<!-- QF1-ADOPTED-20261120-END -->


- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。今回のユーザー指示による設計変更であり、検査器・BFT停止処理・Qwenの反復費用検査・公開有効化の完成ではない。
- 論理profile: **MAS1（Model Admission Safety）**。nativeのwire ID、fence、高さ、実committee又は本番parameterは未割当。
- 判断記録: [ADR-0233](../adr/0233-model-admission-screening-and-bft-stop.md)
- 基礎契約: [FOW1](0062-palw-function-output-and-work-admissibility.md)、[TB1](0061-palw-internal-threshold-beacon.md)、[追加モデルburn停止](0053-additional-model-burn-pause.md)、[MOE2](0064-reward-buy-and-permanent-liquidity.md)
- 適用・移行: [MAS1移行契約](../governance/model-admission-safety-transition.md)
- 機械可読の設計契約: [mas1-admission-policy.json](../design/palw/models/mas1-admission-policy.json)。実行可能なnative設定ではない。

## 0. 目的と採択範囲

ユーザーは、モデル追加経路において、実用性を持たず、又は採掘値を安価に予測・再利用できるモデルを事前に排除し、自動検査で安全性を確定できない場合をBFTによる停止へ接続するよう指示した。本RFCはこの入口と停止の契約を定める。

既存の「モデル品質審査をしない」という規則を、MAS1採掘資格に関する公開・共通の実用性条件の範囲で改定する。一般のartifact保存登録はpermissionlessのまま維持する。「BFT validatorを使わない」は台帳finality・fork choice・reorgの決定権について維持し、限定されたモデル安全停止権限だけを今回追加する。旧DNS-BFT、HF取得可否の多数決、モデル名allowlist、任意の品質委員会を復活させない。

MAS1はFOW1のG-EXEC・G-COLL・G-COSTを弱めない。構造検査のPASS、実用テストの高得点、BFTの署名数は、Qwen又は任意のプログラムの計算費用下界を証明しない。未対応・不明・期限切れをPASSとして扱わない。BFTに「未知のG-COSTを承認して採掘を許す」操作は存在しない。

## 1. 対象と二つの登録状態

MAS1を有効化したFOW1 profileへ、新たに採掘資格を得ようとする全モデルを対象とする。モデル名・owner・genesis/softforkという分類で安全条件を免除しない。ただし既受理の旧profileを遡及的にMAS1へ変更せず、切替は第14節に従う。

`ModelRegistered`は不変artifactと登録情報の保存である。ADDITIONALの登録はMOE2の一意市場作成と同じ原子遷移で成功する。MAS1の審査待ちを「市場未準備」と呼ばず、登録後にseed待ちの市場状態を作らない。

`MiningAdmissible`は、別に管理する採掘資格である。市場が存在する、Positionを買った、bondを預けた、既存モデルに似ているという事情から自動付与しない。未適格でも通常の推論提供・artifact保存・既存市場の通常取引は、それぞれの既存規則で可能である。UI/RPCは登録・市場・採掘資格を別項目で表示する。

coreのOption対象外・対象worker報酬Rの100% Miner帰属、追加モデルのMOE2 30%購入・20%永久LP・50%残余は変更しない。MAS1による停止は市場閉鎖、Position没収、永久LP解除又は別の報酬配分を発生させない。

## 2. 固定する登録入力

FOW1のFunctionWorkDescriptorへ、次の版付き情報を追加する。全情報は同じartifact・program・数値profile・Readoutへ結合する。

| 項目 | 内容 |
| --- | --- |
| `KernelAnalysisProfileId` | 対応TIR/kernel集合、shape/type、rewrite規則、解析上限、全分岐の扱い |
| `LoweringManifestRoot` | 元graphから規範TIRへの対応、演算・tensor・重み位置、compiler版と変換規則 |
| `UsefulStateMapRoot` | 回答yに必要な状態、zのReadout元、共有部分と採掘専用部分の対応 |
| `InputDomainId` | 許容job、challenge frame、長さ、sampling、cache origin、入力選別の範囲 |
| `PracticalSuiteId` | 利用用途、客観的score、正確な閾値、資源上限、test生成規則、基準profile |
| `WorkCostContractId` | 比較する経路・入力数・費用単位・閾値、前処理・共有・最適化・外注の会計 |
| `AdmissionPolicyId` | 判定規則、署名suite、BFT停止scope、期限、受付・再検査・異議容量 |
| `AdmissionGeneration` | 同じdescriptorに対する検査世代。旧判定・旧解除の再利用を拒否する単調な識別 |

全件coverage、入力依存、数値意味を維持するloweringを必須とする。許可されないcustom op、外部呼出、秘密の状態、未固定RNG、未bounded loop、未検査のMoE routing、未検査のcacheを「その他」として無視しない。未知のkernel・判定器・定理又は未認証の重み属性は`UNKNOWN`である。

浮動小数点の加算順序、overflow、量子化・丸め・飽和を保持しない代数変形を、安価な同値計算の証明に使わない。低rank等の構造証拠も、対象profileの正確な数値意味で検査する。

## 3. 自動検査の判定型

各判定器は`PASS`、`FAIL`、`UNKNOWN`のいずれかと、規則ID・認証した対象・coverage・資源消費・根拠を返す。

- `PASS`: その判定器が担当する、明記された必要条件を確認した。モデル全体のG-COSTが自動的に証明されたという意味ではない。
- `FAIL`: 認証付きの反例又は規則違反を確定した。その検査世代では採掘不可。
- `UNKNOWN`: 未対応・上限超過・不完全なcoverage・証拠不足・推定だけの危険又は期限切れ。新規採掘は保留し、BFT review caseへ接続する。

単なるtimeout、HTTP失敗、浮動小数点誤差、heuristicの警告を算術不正の確定に変えない。複数のPASSで一つのUNKNOWNを相殺しない。解析の打切り時点で発見がないこともPASSにしない。

### 3.1 自動で調べる構造と反例

| 検査対象 | 排除・保留する例 | 判定の境界 |
| --- | --- | --- |
| Initとchallenge | nonceがmetadataにしかない、mask/truncationで落ちる、定数へ置換される | 規定入力と矛盾する認証証拠はFAIL。形式的な依存線だけではG-COST PASSにしない |
| Readout | z=s、H(s)、定数、短いtable参照、回答を計算せず生成できる別枝 | 登録規則に反する短絡を確定すればFAIL。依存解析だけで全代替算法を否定しない |
| 有用な計算との結合 | yには不要な巨大な演算をz専用に追加し、LLMの有用仕事として価格付けする | 第4節の費用分離に反すればFAIL |
| 定数化・相殺 | 零/identity/対角/低rank、無効expert、打切り、飽和、既知の相殺 | 構造の存在だけを違反にしない。宣言下界に反する同値算法又は許可predicate違反を確定すればFAIL。未認証の全重み条件はUNKNOWN |
| cache・共有 | 同じprefixを毎回新規扱い、seedだけ替えて同じlogitsを使う、重み前処理を無視 | WorkCostContractに入らない共有があればFAIL又はUNKNOWN。正当なcache自体は不正ではない |
| 選別と条件分岐 | 簡単な入力だけ選択、benchmark検知、challenge依存の安いbranch、MoEの一部を非開示 | 許可入力全域・分岐の定理条件を確認できなければUNKNOWN。既知の違反はFAIL |
| traceとTicket | 自由salt、可変padding、偽のgraph位置、同じ仕事の複数ticket | canonicalな実行・一回会計に反すればFAIL |
| 実行の再現 | 未固定sampling、外部API、正規profile間で異なる丸め | 確定違反はFAIL、未実装検査器はUNKNOWN |

「dense」「full-rank」「パラメータが多い」「Qwenと同形」「出力が高entropyに見える」「多数のtestで衝突しなかった」だけをG-COLL/G-COSTの証明として受理しない。認識できる短絡の検出範囲を`DetectionCoverage`として公開する。任意の悪意あるモデルを完全に自動排除できるとは宣言しない。

### 3.2 下界と安いアルゴリズムの扱い

G-COSTは[RFC-0062 §4.3](0062-palw-function-output-and-work-admissibility.md#g-cost-review)の反復費用検査で判定する。再現された正規Ticket入力の費用が事前閾値を下回れば当該世代はFAIL。旧世代のPASSは新しいartifact・suiteへ引き継がない。

未証明の性能測定又は特定GPUでの高速化は、危険を報告する材料になるが、そのまま数学的な反例とはしない。モデルの用途を維持する正しい高速化をslashせず、仕事単価・適格性ruleの再検討と停止へ接続する。

## 4. 実用性と採掘専用計算の制限

`G-USE`を、G-EXEC・G-COLL・G-COSTと別の採掘必要条件として追加する。単なる人気投票ではなく、採択済みPracticalSuiteの同じ規則を全候補へ適用する。

PracticalSuiteは、少なくとも次を固定する。具体的suite・閾値・判定器が採択されていないprofileは有効化不可とし、登録者が閾値0又は空のtest集合を選べる既定値を置かない。

1. 用途と許容入力分布、test生成器、採点対象のcanonical出力、客観的score関数、非空の最小件数と合格閾値。
2. challenge adapter適用後の回答品質。元モデル名の評判や、adapterを外した結果で代用しない。
3. 数値profile、最大入力/出力長、tensor/trace資源上限、異常終了・無限反復・空出力の扱い。
4. useful inferenceと、Readout・commitment・解析・検証の費用を分けた報告。元から利用者に必要な計算を主な費用根拠にする。
5. 新規job、共通prefix、cache、batch、MoE routingを含む試験範囲と、適用できない用途。

`UsefulStateMap`は、yに必要な計算とzに使う状態の対応を明示する。独立した重いhash puzzle、無意味な巨大行列、不要な長文生成、使われないexpert又は全MAC途中値の書出しを追加し、その費用を「Qwenの通常の有用推論費用」として認定しない。必要な軽量Readout・資料供給・検証処理は許すが、費用を分離し、追加負担の上限と根拠をprofileに明記する。

安全な解析で不要枝と確定できる場合は排除する。意味的な必要性が確認できない場合はUNKNOWNであり、graph上で線をつないだだけの証明を認めない。source-levelの数値意味を保つ変換でも、G-COSTの別検査は必要である。

### 4.1 testの実行とNodeの責務

モデル・descriptor・suite・test予算・固定期限を登録者が変更できない形でfreezeした後、独立した`TB1-MODEL_AUDIT`でtest instanceを導出する。このpurposeは[TB1用途表](../design/palw/consensus/tb1-beacon-consumers.json)に追加する。EXEC/PANELの既知値をhashして未知のtest乱数としない。複数候補を事前登録して有利なものだけ選ぶ機会は受付予算・費用検査の候補数へ数える。

評価は`NON_REWARDING_MODEL_EVALUATION`として行う。未適格モデルを検査するための実行であり、FOW1採掘claim、work weight、EXEC credit、受賞権利、モデル帰属報酬又はMOE2買付を発生させない。評価の成功を同じtraceの採掘成功へ変換しない。資源は事前の登録/評価予算から予約する。

評価提出物を固定した後、別のTB1-PANEL roundで再実行担当を選ぶ。これは当該モデルの採掘資格・新規mint・未成立の受賞ブロックに依存しない公開control経路を使う。既存のPanel完全再実行・元提出物へのbinding・局所Court・PublicOneHonest条件をこの非報酬用途へ明示適用する。対象モデルからの自己選出を認めない。

Nodeは全モデルを実行しない。認証済み出力から有界に計算できるscore、評価caseの正規terminal、coverage、challenge、期限を検査する。画像・動画・自動モデル改善の延期は解除しない。定性的な「賢い」という投票、Nodeが検査できない外部LLM judge、未認証の評価値をPASSにしない。

実GPU時間・メモリの測定は装置・実装・条件を付した運用資料として扱う。NodeがRPCの自己申告を直接consensus判定にしない。再現可能な資源違反は規範上限で検査し、客観化できない性能上の懸念はBFT reviewへ送る。

test合格はそのsuiteについての実用性根拠に限定する。有限testから全入力の品質、未知のbackdoor不在、G-COLL又はG-COSTを結論しない。memorization、選別、分布外入力に対する残余リスクを記録する。

## 5. モデル非保有Nodeが合格を受理できる条件

自動解析workerはモデル全体を取得して解析できるが、その署名は解析結果の真実性を自動的に証明しない。Nodeは次を分ける。

| 根拠 | Nodeが扱える範囲 |
| --- | --- |
| boundedなgraph/type/shape/kernel predicate | 認証済みmanifestと対応位置を既存checkerで直接検査する |
| 局所的な規則違反・実行反証 | 元rootへのopening、位置、数値規則を検査してFAILを確定する |
| 実用testの結果 | 第4.1節の非報酬評価caseをPanel＋Court条件の下で終結し、有限scoreを計算する |
| 全weightが満たす大域的な性質 | 既存FOW1 TheoremRuleが認める認証付き適合検査がなければUNKNOWN |
| 未知の代替算法が存在しないという主張 | 有効なTheoremRuleと全適合条件が必要。worker署名やBFT署名で代替しない |

登録者の`.root`属性、JSONの`full_rank=true`、Merkle membershipの数件、解析したというACKだけから全重みの性質を認めない。有限個のopeningは、その位置の内容を確認するだけである。万能なプログラム困難性判定器又は新しいSNARKを暗黙に仮定しない。

## 6. 採掘資格と保留の状態機械

一般登録状態とは別に、検査世代ごとに`AdmissionState`を持つ。

| 状態 | 意味 | 新規採掘 |
| --- | --- | --- |
| `UNASSESSED` | 登録直後、資料・検査・評価が未完 | 不可 |
| `CHECKING` | 予約済み予算で検査中 | 不可 |
| `REVIEW_HOLD` | 必須項目がUNKNOWN、又は有効なrisk hold | 不可 |
| `REJECTED` | 違反確定、又は検査の原期限までに合格せず終端 | 不可 |
| `PASSED` | 全必須項目が現行ruleに適合 | 他のgateも満たす場合だけ可能 |

自動検査のFAILはBFTを待たずREJECTEDにする。UNKNOWNは直ちにREVIEW_HOLDにし、対象・理由・既確認項目・不足証拠・原期限を持つ`ModelRiskCase`をcanonicalに作る。BFTが止まっても、新規モデルを暫定PASSにしない。

期限満了はAdmissionStateをREJECTED、終端理由を`REJECTED_UNRESOLVED`とする。これは未適格終端であり、算術有罪・自動slashではない。待機caseは有限に閉じ、モデルのartifact保存と市場は維持する。再評価には新しい検査世代と予算を必要とし、旧UNKNOWN・停止・根拠を失わない。

既にPASSEDのモデルについて、誰かが「怪しい」と送信しただけでは全ピアの採掘を停止しない。必須の再検査失効・有界な認証反例は自動hold/FAILへ、未確定の懸念は第7節のBFT停止証明へ進める。未署名packetや公開caseの受付だけを永久vetoにしない。

有効な採掘述語を全consumerで共有する。

```text
EffectiveMiningAllowed =
    ProfileActivated
    AND AdmissionState == PASSED
    AND G_EXEC AND G_COLL AND G_COST AND G_USE
    AND CurrentRulesAndEvidence
    AND NoAdmissionHold
    AND NoStructuralFailure
    AND NoBFTSafetyStop
    AND NoApplicableBurnPause
    AND AllExistingClaimAndAccountingGates
```

## 7. BFT停止権限の構成

### 7.1 rosterと故障前提

停止担当は、対象caseを開く前にnativeで確定したTB1 holder rosterを基に、`MODEL_STOP`権限を明示付与された同epochの集合とする。モデル登録者が個別に選んだ集合や当該claimのPanelを代用しない。権限はTB1 shareから自動的に発生せず、activation recordと用途別署名鍵の登録を必要とする。

`n=3f+1`、`f>=1`、故障・共謀者は高々f、停止証明は相異なる`q=2f+1`の署名を必要とする。n/fの実数値、collateralの非重複、選出・交替、鍵保護、suite、署名の最大bytes、故障モデルを有効化前に固定する。5人のLLM Panelに一人の正直者がいることから、このBFT前提を導かない。

roleの候補集合・選出・bootstrapはTB1-ROLEの規則を継承し、候補モデル自身のworkや報酬を初回停止権限の根拠にしない。bootstrap未構築又はfault budget未設定ならMAS1は有効化しない。

### 7.2 一方向の停止証明とnative順序

`ModelSafetyStopCertificate`は、固定された一つのcase/payloadに対するq署名である。PBFT等による別台帳を作らず、既存nativeのcanonical順序で停止集合を更新する。証明をmempoolで見た時点や各ピアのwall clockでconsensus状態を変えない。

署名対象は少なくとも、domain/version、`action=STOP`、network/genesis、scope、artifact/program/profile/Readout digest、TheoremRule、AdmissionGeneration、CaseId、ReasonCode、EvidenceRoot、policy ID、roster epoch/root、caseのcanonical起点、受理期限を含む。すべて長さ・順序・codecを固定する。署名者の重複、別purpose、別network、旧roster、別世代、別branchの起点、期限切れを拒否する。停止と解除は別action・別message subtypeとし、相互に署名を流用できない。

scopeは`ARTIFACT_PROFILE`又は事前採択済みの`THEOREM_RULE_VERSION`に限定する。後者は同ruleに依存するモデルだけを対象とし、全ネットワーク停止、fork選択、既確定残高の変更又はclaimの真偽判断へ拡張しない。rule依存indexと全入口を同じnative状態へ結合する。

ScopeRefは識別付きunionとする。ARTIFACT_PROFILEはartifact/program/profile/ReadoutとAdmissionGenerationを、THEOREM_RULE_VERSIONは不変RuleId/versionとRuleControlGenerationを束縛する。無関係なfieldをwildcardにしない。rule停止は現在の依存モデルと、その後同じ停止versionへ登録されるモデルの両方へ効く。

ModelRiskCaseの受理後は、owner/APIによる世代更新で署名対象を無効化できない。原期限内の再検査はcaseに結合した評価IDで進め、世代変更はcaseの正規終結と同じnative遷移へ制限する。停止済みのscopeは世代をまたいで保持し、新しい検査世代を作るだけでは消えない。

停止集合への追加はidempotentであり、同じcertificateの再送で別停止・報酬・burnを生まない。適用済み停止は、署名epochや証明の受理期限が過ぎても自然解除しない。

ReasonCodeは`UNRESOLVED_ADMISSION`、`SHORTCUT_RISK`、`WORK_RULE_RISK`、`USEFULNESS_RISK`、`CHECKER_COVERAGE_RISK`を少なくとも区別する。EvidenceRootは取得可能なcase資料と結合し、空URL又は「検査済み」という署名だけで理由を満たさない。未証明の危険を停止理由にすることは許すが、それを計算不正の判決と表示しない。

### 7.3 誰が何を判断するか

Nodeは署名権限・threshold・scope・世代・資料契約・native状態を検証する。自動判定不能な危険の実体判断はBFT担当者の責任であり、署名が数学的真実性を証明したとは扱わない。正直な担当者も誤った危険判断をし得るため、誤停止の可用性リスクは残る。

BFTは未証明モデルを承認する役ではない。新規UNKNOWNはすでに採掘不可であり、certificateは停止理由と責任を共有状態へ確定する。稼働後に初めて発見された未知の短絡については、発見・分析・署名収集・canonical包含までの期間が残る。未知の全攻撃を事前又は瞬時に排除した保証はしない。

一人の正直者が危険を報告することと、q署名で停止を確定することは別である。公的な局所反例があれば自動反証経路を使えるが、数学的に未確定の懸念ではBFTの賛同・包含条件が必要になる。

## 8. 停止の効果、優先順位、既存責任

新しいMAS1 reducerは、同batchの有効な安全停止・認証反例・rule失効を、成功deadline sweep、Final、credit/weight/permit発行、受賞権利消費、新規報酬確定より前に反映する。旧実装のpre-object sweepをそのまま残して停止をすり抜けさせない。旧profileの再生順序は変更しない。

同batchで適格な停止と解除が競合すれば停止を優先する。解除が消去できるのは第9節で束縛した既存停止集合だけであり、後から追加されたstopを消せない。すでに前のcanonical位置で確定した効果を、後続certificateだけで遡及取消ししない。

停止後は次を止める: 新規採掘受付、成功Final、新規のwork/credit/weight/permitの認定、未消費の当選権利を使う新しい受賞ブロック、未確定のモデル報酬発生。元の未決claim・権利の期限を保持し、期限内に正規復帰しなければ既定の未成功・失効終端へ進む。停止で索引から脱落させず、無期限にRAM/slotを占有しない。

停止前に確定済みの残高・支払債務、受理済みブロック、消費済み権利、過去fork weightはBFT証明だけで取り消さない。既存の客観的なclaim不正・取消規則は独立して適用する。安い正当な算法の発見だけでは、過去の正しい実行を算術詐欺に変更しない。

停止中も資料供給、公開異議、negative Court、返還、責任精算、既存記録の取得を継続する。これらのcontrol処理は停止したモデルの採掘権利又は未成立のTB1-MODEL_AUDIT roundを待たない。経路・予約容量・手数料原資をactivation前に接続する。

## 9. 解除と迂回防止

安全停止とburn停止は別の理由集合である。ownerのResume burnは`BURN_PAUSE`だけを解除し、`ADMISSION_HOLD`、`STRUCTURAL_FAILURE`、`BFT_SAFETY_STOP`、`RULE_DISABLED`を解除しない。genesis/fork指定によるburn対象外化も安全停止を消さない。

同じdescriptorについての誤停止解除には、再検査世代の全必須項目PASS、有効なG-EXEC/G-COLL/G-COST/G-USE、未解決caseなしを必要とする。そのうえで、現在有効な停止担当rosterによるq署名の`ModelSafetyResolutionCertificate`を必要とする。

解除bodyは、専用subtypeと`action=RESOLVE`、network/genesis、対象descriptor、最新検査世代、最新PASS record digest、取り消すStopId集合とそのroot、期待する直前control record、policy、roster、起点、期限を束縛する。Nodeはこれらを現在のcanonical状態と照合する。署名だけの解除、旧PASS、古い停止集合、timeout、少数署名、owner変更からの解除を拒否する。q署名があってもG-COSTがUNKNOWNなら採掘不可である。

重み・kernel・Readout・入力規則を変更して危険を解消する場合は、新しい不変descriptorとして全検査をやり直す。旧Model IDをmutableにしない。停止したTheoremRuleを変名して同じrule versionとして通さない。

THEOREM_RULE_VERSION停止は、一つのモデルのPASSやModelSafetyResolutionCertificateでは解除しない。ruleの復帰には別途レビュー・採択した新しい不変rule versionと明示的な適用境界を必要とし、利用モデルは対応するdescriptorで全適合を再検査する。旧versionの停止記録は保持する。これにより、一つの良いモデルの再検査で同ruleの未検査モデルを一括復帰させない。

既知の同一artifact/program/profileのaliasはcanonicalな依存indexで停止を共有する。任意の異なるbytesについて意味的同値を完全判定できるとはしない。小変更モデルも新規検査・候補予算・費用ruleの対象であり、旧承認を継承しない。

## 10. 受付、期限、検閲、reorg

登録・自動解析・非報酬評価・risk caseごとに、最大bytes、演算/件数、元期限、最大同時case、累積予算、reserved control容量、料金・refund条件をpolicyで固定する。適格なrisk報告を単なる高額burnの支払競争へ置き換えない。新規登録の大量投入が既受理の反証・停止・責任精算を排除しない。

無効な報告は棄却し、同一caseの重複は統合して上限を管理する。証拠付き反証とBFT停止証明に保護容量を設ける。ただしこれだけで無条件の包含を証明したとはせず、nativeの包含・サービス・fault前提をactivation資料へ記載する。

BFTが停止した場合、新規UNKNOWN/失効済みモデルは閉じたまま期限終端へ進む。既PASSEDで未知の危険がまだ証明もcertificationもされていないモデルを、何もないのに全停止したとは扱わない。停止の活性には、必要な署名者の賛同と稼働、資料取得、期限内包含が必要である。

reorgはcontrol状態、admission、case、権利消費、会計をnative journalで原子的にundo/redoする。証明のキャッシュだけを別のconsensus状態として適用しない。旧起点がcanonicalでなくなった証明は再検査し、再提出には有効な起点と期限を必要とする。公開済みTB1乱数のfreshnessはundoで回復しない。DAGの分断・再合流・深いreorgをBFT停止証明だけで解決したとはしない。

pruning/snapshot/IBDでは、有効なAdmissionPolicy、descriptor、最新generation、PASS/hold、停止集合と解除根拠、rule依存index、roster/case起点を再検査可能な形で保持する。古いPASSEDだけをimportして停止を消す経路を禁止する。

## 11. 条件付きで示せる性質

### 11.1 未確認モデルの成功阻止

全consumerが第6節の同じ述語を検査し、初期状態がUNASSESSED、PASS以外からの成功遷移が存在しないなら、必須項目がUNKNOWN/FAILのモデルから新しい成功効果は発生しない。これは状態遷移の帰納法による。BFT timeoutにもPASSへの遷移がないため、BFT停止時にもこの性質を維持する。

### 11.2 少数の不正署名だけではBFT停止証明を作れない

署名が偽造困難で、n=3f+1の認証された集合の不正者が高々fなら、q=2f+1署名の証明は少なくともf+1人の正直な署名を含む。また二つのq集合の共通部分は少なくとも`2q-n=f+1`である。これはthresholdの集合論的な保証であり、停止理由の真実性、合意プロトコル全体、又はQwenの困難性の証明ではない。

### 11.3 burnと古い解除による迂回の排除

停止理由を別々に保持し、解除対象集合・期待旧record・検査世代を照合するなら、burn Resumeと古いResolutionCertificateは新しい安全停止を除去できない。同batch停止優先と全consumerへの反映を組み合わせ、新規成功のすり抜けを排除する。native実装とreorg処理の検証は別途必要である。

### 11.4 証明していないこと

全ての安い代替算法の自動発見、任意LLMのG-COST、実用testの未知入力への一般化、BFT担当者の完全な危険認識、期限内包含、停止前の被害ゼロ、DAG全体の安全性を本RFCの完成定理としない。これらを`epsilon=0`又は根拠のないnegligible項にしない。

## 12. 旧方針の限定的な置換

| 旧方針 | MAS1での扱い |
| --- | --- |
| RFC0062/ADR0230の品質審査をしない | 一般登録は維持。採掘に限り公開・共通のG-USEを追加。名前・人気・裁量的allowlistは不使用 |
| FOW1の三gate | 三gateを維持し、構造検査・G-USE・現在の停止状態をAND条件へ追加 |
| ADR0180のBFT不使用 | 台帳finality等の禁止を維持。モデル/ruleの安全停止と条件付き解除だけを後続の例外として追加 |
| RFC0053のburn Resume | burn理由だけを解除。安全停止は第9節の条件を必要とする |
| coreのburn対象外 | 経済的burn停止の対象外は維持。MAS1採掘の安全適格性までは免除しない |
| TB1の用途集合 | 独立MODEL_AUDITを追加。MODEL_STOPは新乱数ではなく用途別署名権限 |
| モデル単位検査不採用 | MAS1の採掘資格に関する限定検査へ明示改定。HF可用性認定や一般のモデル品質委員会は復活しない |

旧採択・原条文・測定・署名済みclaimは履歴として保持する。現在の限定的な改定を理由に、廃止されたDNS finality、延期されたメディア/モデル改善又は旧乱数を再開しない。

## 13. 有効化前の必須確認

| 分類 | 必須の根拠・否定試験 |
| --- | --- |
| 解析 | 定数Readout、challenge脱落、死んだ重い枝、既知の短絡、未対応op、MoE分岐、打切りUNKNOWNを区別できる |
| 有界性 | 全重みの自己申告、欠落coverage、偽manifest、解析上限超過をPASSにしない |
| 有用性 | 固定suite/閾値、nonce後の品質、非報酬評価、評価の採掘転用拒否、費用分離 |
| 数学 | G-COLLの具体TheoremRuleと全適合predicate。G-COSTは別途、固定したsuiteで反復検査 |
| BFT | roster権限・fault仮定・署名suite、重複署名、別purpose/branch/epoch、拒否・交替、証明搬送 |
| 遷移 | UNKNOWN待機中の採掘拒否、timeout終端、停止優先、burn/owner/alias迂回、stale解除、再検査 |
| 消費 | Final/credit/weight/permit/受賞権利/報酬の全入口、停止中の元期限と責任継続 |
| 回復 | 3ピア再生、restart、reorg、IBD、pruningでadmissionと停止集合が一致 |
| 運用 | 実suiteと実モデルの実測、trace/Panelを含む負担、保護容量、bootstrap、明示activation |

検査器又はsuiteが未完成であることを理由に「BFTがあるので暫定有効」としない。現在のQwenが合格済みであると宣言しない。

## 14. 移行

現在は設計採択のみで、公開設定・配備・支払を変更しない。将来のactivation recordで対象network、profile、適用境界、rule/suite/roster policy、旧pendingのdrain、検査予算、実装gateを固定する。

適用境界以後の新規MAS1/FOW1採掘資格は全条件を必須とする。既登録モデルにも同じ事前検査を行い、単に登録済みという理由でPASSEDを付けない。旧版で正規受理済みのclaimは元の規則・責任・期限でdrainし、新profileへ付け替えない。

有効なTheoremRule、実用suite、bounded checker、非報酬評価、TB1-MODEL_AUDIT、BFT controlと搬送がそろわなければactivationは成立しない。旧AN1/MOR1、旧beacon、burnだけの解除へfallbackしない。MAS1を将来有効化できることと、現在の稼働網をこの文書だけで止めることを区別する。

## 15. 理論上の参照と検証範囲

BFTの故障前提とquorumの背景は、Castro/Liskov, [Practical Byzantine Fault Tolerance](https://www.usenix.org/conference/osdi-99/practical-byzantine-fault-tolerance)を参照する。本RFCはPBFTの実装・全定理を継承したという主張ではなく、限定された署名証明と既存native順序の契約である。

本改訂の保存原本・変更一覧・文書検査は[改訂記録](../maintenance/revisions/2026-11-20-model-admission-safety/README.md)へ置く。検証結果は文書整合性と明示した抽象不変条件の範囲に限定し、native実装・一般LLMの計算困難性・実機性能の検証と区別する。
