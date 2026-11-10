# PALW 公開資料と一人の正直なPanelを実装する方針

作成日: 2026-11-10。対象: 現行RFC-v2-0001〜0020と、追加採択RFC-0048〜0050。文書の位置づけ: **採択済み設計を実装するための作業計画**。新しい設計の採択、runtimeの完成、実測済み、有効化済みを意味しない。

## 1. 作業方針

**コードを入口から出口まで垂直に接続し終えてから、検証を通しで実行する。長時間の実LLM、cold取得、長文脈、最悪負荷の検証は、その最後にまとめる。**

実装の単位は「型だけ」「Panelだけ」「courtだけ」ではなく、一つのclaimが公開APIから入り、Minerのcommit、資料供給、独立Panelの全体検証、Nodeの裁定、Final、credit、決済、保存まで進む経路とする。最初の正常経路を通した後、不正、資料不足、悪意の異議、再起動・reorg等を同じ経路へ実装する。

本計画での「通す」は、実装工程では呼出し・型・保存・権限・状態遷移を接続してコードを完成させる意味である。動作を確認したという意味にはしない。検証工程を終えるまでは、進捗を「実装済み・未検証」と表示する。

| 工程 | 行うこと | 次へ進む条件 |
| --- | --- | --- |
| 契約と設計課題の具体化 | 新旧の版、全実行関係、資料case、成功gate、資源・期限、未確立の手順を整理 | 実装対象の意味と未決事項が明示され、担当間の契約が一致している |
| 通し実装 | 本番経路のコード、永続化、RPC、テストコード、fixture、runnerを作成 | 必須経路が接続され、成功を代行するstubや未処理分岐がない |
| 通し検証 | 固定した実装候補に対してbuild、短い試験、統合、復元を実行 | 必須の非長時間検査を通過し、重い測定の前提が成立している |
| 最終の計算・負荷検証 | 実LLM、cold唯一正直者、最大合法攻撃、長文脈、EXECとの混合負荷 | 対象profileの安全性・条件付き正常完了・資源上限を同一候補で確認できる |
| 独立レビューと修正 | 未接続、保証の抜け、試験の抜けを別担当が確認 | 未解決の必須問題がなく、結果が現在の候補に対応している |

実装途中には、`cargo check`、build、test、ベンチマークの合格待ちを設けない。fixtureと試験コードは実装と同時に書くが、実行は後段にまとめる。コード読解、差分確認、インターフェースのすり合わせ、設計上の論証は実装を具体化する作業として継続する。

検証を後ろへまとめる代わりに、共通契約の変更は全consumer・codec・保存形式・試験期待値へ同時に反映する。自動CIの結果が途中で発生した場合も捨てずに記録するが、実装順を部品ごとの合格待ちへ戻さない。

## 2. 実装する保証と維持する境界

目標は、**固定されたモデル・入力・規則に対する実行整合性を、独立Panelの全体検証と、一人でも提出できる局所反証によって守ること**である。Minerや各Panel本人が特定の機械で新規に実行した行為、非外注、実FLOPsを証明する目標にはしない。

採択済みの以下を実装全体の不変条件にする。

1. 各claimを担当する実際の独立選出Panelに、少なくとも一人の能動的な正直者がいる。その人がモデルを事前に持つとは仮定しない。
2. Minerは重み・構造・program、prompt・token列・tokenizer、数値規則、seed・RNG・初期KV等の依存、初回traceと認証情報をclaimへ束縛し、有限の公開供給・保持義務を負う。
3. global poolから結果固定後に選出する。モデル保有、Minerの推薦、取得成功者だけへの絞込み、同じclaimの再抽選を参加条件にしない。
4. Panelは全scopeを検証できる。任意の一人への委任やlayer分担だけで、未知の正直者の全体coverageを失わない。
5. 新profileの完全再実行承認は最低1件。ただし署名直後のFinal、全員ACK、隠れた多数決を成功条件にしない。Panel人数の新しい値を1に決める規則でもない。
6. 一人の適格な資料要求・計算noticeがnativeの未解決集合`Q`を作り、全肯定票に関係なく成功を保留する。一つの有効な元claim反証で無効化する。
7. Nodeはモデル全体、全推論、GPU、モデルrunnerを必要としない。認証と有界な局所検査を行い、逐次処理や別プロセス化によって全モデル再検査をNodeへ戻さない。
8. 資料不供給、検証未完了、計算不正、義務違反を区別する。未検証は成功不可であり、計算有罪を推定しない。
9. 原受理`a`、notice `a+3,120`、hard `a+10,000`を保持する。取得・receipt・交代で時計をresetしない。`hard+1`の排他的終了、保持期限と訴追権の違いも実装する。
10. 反証、Qによるhold、依存失効は、同じcanonical処理単位のFinal、credit発行・予約、schedule、permit消費、関連fee・取引効果より先に反映する。

旧署名、旧claim、旧75/41、旧`PositiveChecked`、旧Gate C、既存の二階層1.0／0.3は各版の意味を維持する。モデルの公開状況を登録資格審査やtier変更にしない。計算validity proofや新しい汎用proof VMを導入しない。規範は[PRINCIPLES §13–14](../../../PRINCIPLES.md)、[RFC20](../../../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)、[公開裁定v0.4](../consensus/public-adjudication-policy-v0.4.md)に置く。

## 3. 入力の基準と現在のコード

### 3.1 基準資料

主入力は添付の`misakas-main (1).zip`である。このコード側のdocsにはRFC-0050／ADR-0218と2026-11-09公開Panel改訂が含まれる。別添`misaka_docs_palw_execution_integrity_20261109(2).zip`は、その前のRFC-0049改訂までなので、後者で最新docsを上書きしない。

| 入力 | bytes | SHA-256 |
| --- | ---: | --- |
| `misakas-main (1).zip` | 101287368 | `148973f541f829bb81548607d48745d2f0d91d866eecf7d24afc68500411d39f` |
| `misaka_docs_palw_execution_integrity_20261109(2).zip` | 15306568 | `7b0c9874f2db35d9aea5c39fc262ba70df11c056d748b9eefb416adf47a80f35` |

本計画の現在地はソース読解に基づく。過去のPASS、コメント中の測定値、型や関数の存在を、この候補のbuild成功や新profileの動作確認へ読み替えない。

### 3.2 再利用する基盤と変更点

以下のパスはリポジトリrootからの相対パスである。

| 現在のコード・symbol | 読み取れる範囲 | 今回の実装方針 |
| --- | --- | --- |
| `misaka/palw/positive-contract/src/messages.rs` — `MinerResultCommit`, `PanelWitnessCommit` | 旧Miner結果commitは意図的にtrace rootを持たず、execution recordはPanelの責任 | Miner初回traceを署名する新claim/domainを追加。旧Panel recordの不正をMiner有罪へ転用しない |
| `misaka/palw/remote/src/positive_miner_v2_inference.rs` — `infer_and_sign_tir_result_v2` | 認証済みパラメータから推論・結果署名するno-capture経路 | 同じ実行からcanonical trace、manifest、有限供給契約まで作る新版producerを接続 |
| `misaka/palw/tir/exec/src/node/positive_wide_material_v2/capture.rs` — `capture_one_pass_v2` | Panel向けone-pass captureと資源所有の部品 | 記録器を再利用し、Miner署名root、全関係、raw形式、分解規則への接続を追加 |
| `misaka/palw/remote/src/positive_panel_sources.rs` — `PanelSourceCacheR1` | 取得・opening再認証・local cache。現行source種別はartifact inventory/input | 初期state・RNG・依存・Miner traceを含む全manifestへ拡張。local受領をnative義務完了にしない |
| `consensus/core/src/palw_positive_source_v2.rs`、`kaspad/src/palw_positive_miner_source_v2.rs` | source ledger準備・replayとown-node adapter | actual session、duty、fee、予約、外部transport、daemonの継続処理までcallerを追って接続 |
| `consensus/core/src/palw_tir_court_v1.rs`、`palw_tir_objective_v1.rs`、`palw_tir_public_fold_v1.rs` | TIR cone反証、元claim解決、native裁定・反映の部品 | 元Miner初回rootと全関係familyに対応する有界checkerへ拡張 |
| `consensus/core/src/palw_court_shared_material_intake_v1.rs` | 共有資料の同一性認証 | 計算検証・供給完了・検査機会と分離したまま再利用 |
| `consensus/core/src/palw_public_evidence_package_v1.rs` — `PalwPublicExpectedOperationV1` | 現行の小型具体例は`Clamp` | 全LLM対応とみなさず、採用kernelの全関係に証拠抽出とcheckerを用意 |
| `consensus/core/src/palw_positive_check_v2/full_relation.rs`と`full_relation/wide_v2.rs` | local typed prototypeの全関係検査 | Panel側の関係照合等へ再利用。全frameのNode検査を新profileの通常経路にしない |
| `consensus/core/src/palw_security_certification_fold_v1.rs` — `commit_at_final_v1` | 旧Gate CはFinal時に認証subjectを固定 | 旧版を維持し、新claimへFinal先行の循環を持ち込まない |
| `consensus/core/src/palw_state_v2/positive_assignment_certification.rs` — `begin_positive_certification_v2` | こちらはResult固定から認証を開始。native draw/sessionがある | 開始位置の基盤を使う。旧完全Courtの`checked_terminal()`と新Panel全体承認を別の契約にする |
| `misaka/palw/positive-contract/src/certification.rs`、`consensus/core/src/palw_positive_certification_v2.rs` | `PositiveChecked`と旧honest-stake policy | 新しい全scope承認、選出者、元claim、規則版の束縛を別版で追加。旧kだけ1に変えない |
| `consensus/core/src/palw_execution_lane_v1.rs`、`palw_exec_v2*.rs`、`kaspad/src/palw_round_producer.rs` | round/snapshot/permitとTx/Sliceの基盤 | 期限付きExecCredit、一回発行、明示ScheduleId、現在slot生成、取消を接続 |
| `consensus/src/pipeline/virtual_processor/processor.rs`、`consensus/src/model/stores/palw_state_v2/` | live fold、archive、journal、保存の接続先 | 新original/Q/予約/認証/creditを同じdeltaと復元経路へ追加 |

既存のsource ledgerやadaptorは、その関数が存在するだけでpublic APIからの運用経路が完成したとは扱わない。caller、codec、actual native state、保存、再起動後の再開まで追う。

`palw_positive_court_v2/gate.rs`の失敗側prototypeだけを見て全native接続が無いとも断定しない。一方、別のAssignment接続があることを新PublicOneHonest実装済みとも扱わない。`palw_credit.rs`は廃止V1 overlayの互換encodingであり、新ExecCreditの主実装として復活させない。test-onlyの`consensus/src/processes/palw_state_v2_sync.rs`も新IBDの本経路にしない。

## 4. コード着手時に具体化する契約

### 4.1 全scopeの資料取得と検査機会

**最優先の未確立事項は、大容量の公開資料について、選択的秘匿と悪意の虚偽未着申告を、小さなNodeの根拠で区別して有限に処理するprofile別手順である。** 現行採択は`PROFILE PROCEDURE NOT ESTABLISHED`を明記している。これはテストを後ろへ回すだけでは解消しない設計課題である。[RFC18 §2.4](../../../rfc-v2/0018-interactive-negative-court-and-defaults.md)と[最新レビュー](../../../maintenance/revisions/2026-11-09-public-panel/REVIEW.md)を基準に、次の成果物を作る。

| 設計成果物 | 必ず具体化する内容 |
| --- | --- |
| 資料手続きの定義 | 全必須scope、manifest自体が未取得でも提出できる入口、有限要求、供給者の署名済み義務、公開搬送経路、取得権、反証・未履行の根拠 |
| nativeの判定述語 | `InspectionPrerequisitesEstablished`を作る認証可能な証拠、どの参加者・scope・時点に適用するか、根拠の保存先 |
| 終結の論証 | 正直なcold参加者をdefaultにしない条件、虚偽caseを有限に終える条件、根拠がない場合の成功不可・NoSlashHold・Abort |
| 資源と時限 | notice前の抽選・cold取得・全検査・初回包含と、不足時の早期要求からhard内の取得後検査・裁定を分けた予約。最大合法caseと防御応答を含む |

公開URL、provider署名、他Panelのreceipt、一部chunk、要求quota消尽、経過時間だけではこのrecordを作らない。全員ACKへの変更や、Nodeへ全モデルを順に流す方法でも埋めない。

具体手順が立つ前に、成功を可能にする仮の`available=true`や「後で確認済みにする」分岐を作らない。共通型・transport・保存・失敗側の配管は並行して実装できるが、この課題が未解決なら、対象profileの正常完了を含む実装完成・検証完了・有効化を宣言できない。全件拒否や全AbortをPALWの完成に数えない。

### 4.2 全実行関係と記録形式

profileごとに`WellFormed ∧ Init ∧ 全Step ∧ End`を満たすことと、固定された実行の整合性が一致する対応表を作る。各行に、TIR/kernel、数値意味、元trace座標、認証元root、witness抽出、native checker、最大bytes/heap/hash/parse/算術work/I/O、責任主体を記載する。

対象は算術だけでなく、read-from、状態の更新と非更新部分、index/shape、RNG、token選択、停止、出力、長さ、仕事会計まで含む。Minerの初回記録にない部分和を異議後に付け足して元claimの責任にしない。大きな演算は初回commitされた区間へ分け、整数幅、還元順序、overflow、丸め・再量子化を保つ。

既存のtrained artifact監査は旧all-node-i32 profileの制約を示しているが、新profileへの適合判定ではない。[監査記録](../../../../testing/positive-court-a7/test-compatible-llm-audit.md)の候補から開始しても、実際のmodel/program/artifact/tokenizerのdigestと新coverage表で再評価する。tiny fixtureだけで実LLM達成としない。許可kernelから構成可能かを確認するものであり、モデル品質やtierの審査は追加しない。

### 4.3 新旧契約、パラメーター、起動と経済

新claim/profile、署名domain、RPC/native codec、state namespace、fenceを別に定義する。実IDは既存registryと衝突しない形で実装時に割り当て、論理名`PublicOneHonest`を勝手に既存wire値へ対応させない。全presetへの公開activationはこの作業計画に含めない。

`m`、区間幅`B`、帯域、Node上限、batch上限、schedule lead/horizon/grace等の未決値を一覧にする。私的検証用の暫定値にはその印を付け、実測していない旧estimateを新profileの達成性能にしない。測定後に値を変更した場合も、規則・資源・署名へ影響する候補の更新として再検証する。

Final後の訴追経路を使うprofileは、新規申立期限、応答・裁定終端、資料保持、担保の各期限を受理前に固定する。保持期間が長いことだけから新しい訴追権を生成しない。必要な期限が未割当なら、そのprofileを有効にしない。

初回beaconの公開entropy profileと、既存の報酬配分pin／ADRの不整合は独立した有効化条件として扱う。`consensus/core/src/palw_beacon_bootstrap_v1.rs`の承認profile一覧が空であることを、固定seed、blockhash、local RNG、初回だけの認証免除で埋めない。報酬側は`docs/governance/parameter-register.md`、`palw_model_tier_v1.rs`、`palw_bond_budget_v1/allocation_v3.rs`を照合し、未確定の経済判断を実装者が選ばない。既存の採択を実装する範囲を超える変更が必要と判明したときだけ、具体的な案と影響を示して所定の変更手続きへ進む。

## 5. 垂直統合でコードを書く順序

以下は**コード作成の順序**であり、段階ごとの試験実行・合格待ちではない。保存とAPI、失敗時の扱い、試験コードを各変更と同時に書く。最初の正常経路にも、未成立条件を成功にするstubを入れない。

### I0 共通契約と実装台帳を固定する

第4節の成果物、新しい型・scope・版・domain、状態遷移、単一成功gate、各consumerの一覧を作る。対象実LLM、文脈長、prefill/decode、数値profile、最低Node、Panel/provider環境を検証対象表へ固定する。モデル名だけでなく実行するartifact/program/規則のidentityを持たせる。

この時点で必須試験の一覧と重い検証の投入条件を決める。長時間targetを通常suiteから明示的に分離し、V5/V6が必須のtest名・件数を指定して実行する契約を作る。未知の資源値や未確立手順を明示し、未解決箇所に仮の本番許可を与えない。実装ブランチ・担当ファイル・統合責任者を分け、共有codecと状態遷移には一人の統合責任者を置く。

### I1 単一の正常claimを受付から決済まで接続する

公開Job/RPC → Miner実行・初回trace/manifest署名 → native original受理・供給/保持予約 → 固定後draw → cold Panel取得・全再実行 → 一括承認 → Nodeの必須局所検査・成功gate → Final → 有限credit/支払資格 → 保存、という一往復をコードで完成させる。

`positive-contract`、`remote`、TIR exec、source adapter、`palw_state_v2/positive_assignment*`、RPC、native rows、決済consumerを同じ作業単位でつなぐ。Nodeには全frameを渡さず、root・承認・所定の有界局所資料を渡す。Panelが出力だけ一致していても、traceまたは全必須関係の照合が未完なら承認しない。

未完成の資料手順や未知kernelに到達した場合は、限定された未対応結果で停止する。この途中状態を正常経路完成とは報告せず、残件台帳に残す。

### I2 一人の反証を元claimの無効化と経済効果まで接続する

Panelの不一致検出 → 元Miner rootへの局所化・opening構築 → 公開notice/完成証拠 → native Q → 局所court → `Refuted` → 依存とcreditの取消 → 帰責に沿った費用・返還・slash → 保存、を同じ入口から接続する。

直接提出できる完成反証と、有界な対話局所化の両方を扱う。局所化の残区間または残手数が必ず減る規則を持たせる。Miner再署名、二人目の賛成、悪意Panelの協力を反証成立の条件にしない。

外側の不正packetはそのpacketだけ拒否し、既存case/Qを消さない。正規packetがMiner認証済みraw記録のschema違反を示す場合は`WellFormed`反証として扱う。この二つの責任の区別をparserからnative評決まで保つ。

### I3 モデル未保有者の資料要求と悪意caseの終結を接続する

取得不足 → 算術反証なしの適格`MaterialRequest` → 個別CaseId/Qと予約 → 認証付き供給 → `InspectionPending` → 全scopeの検査前提成立 → 検査・反証または正規terminal、までを実装する。要求と供給の公開transport、provider/cache/再配布の認証、実service観測、費用・保持を含める。

manifest、chunk索引、初期依存の対象一覧自体が未取得でも、受理済みoriginalに束縛されたmanifest rootとprofileの入口形式から有界要求を作れるようにする。要求時にモデル本体、Panelの完全再実行root、計算矛盾を必須にせず、必要資料を取得済みの人だけが要求できる循環を防ぐ。

予約は二段に分ける。原受理aから後続seed・draw、cold取得・内容認証、全体検査、初回noticeのcanonical包含までの最悪工程をnotice窓内に配置する。資料不足の場合は完全再実行を待たず適格MaterialRequestをnotice窓内に包含し、その同CaseIdの取得後検査・局所化・証拠・裁定をhard内へ予約する。mempoolへの送信だけを期限内の受理に数えない。遅いdrawや取得開始によってこの機会を消す計画は受理しない。

資料応答が届いても検査caseは直ちに閉じない。notice終了後の応答も、既に受理した同CaseId・scope・原時計で検査と反証を継続する。`MaterialId`によるbytes共有と、申立人ごとのCaseId・残予算・次手・終了条件を分離する。

各選出bondの正規化された保護枠と、有限の第三者枠、防御応答の枠を確保する。正直者が誰か不明なので、共通の先着一枠だけを「正直者用」にしない。悪意多数の不署名は成功を妨げず、虚偽caseは正規terminalまたは客観的defaultで有界に終える。前提不足を正直な申立人のdefaultへ変換せず、回復または未検証Abortへ進める。

### I4 同じcanonical処理単位の優先順位とEXECを完成させる

`palw_state_v2/block_preparation.rs`、`transition_builder.rs`、`palw_kernel_route_fold_v1.rs`、`palw_exec_v2_fold.rs`と各経済writerを横断し、新profileの事前収集・裁定・状態反映を接続する。概念上の順序を次に統一する。

1. その単位の合法なnotice、資料要求、応答、完成証拠を認証して集める。
2. 反証、Qによるhold、依存失効を反映する。
3. 合法な応答を含めた状態でdefault、case終結、期限、返還を決める。
4. 更新後のstateから成功・既存権利の使用可否を再導出し、経済効果を原子的に反映する。

成功Finalから期限付きExecCreditを一度発行し、予約、消費、失効、期限切れを別状態にする。future scheduleの対象集合をseed前に固定し、明示ScheduleIdと現在slotのpermitへ接続する。通常producer loopに、推論・資料取得・過去round回収・次期schedule作成を混ぜない。

新規mintだけでなく、既発行creditの予約、schedule有効化、permit受理/消費、native settlement、関連feeも同じ現在の資格判定を読む。「既受理consumed」の保護は前のcanonical処理単位ですでに確定した分に限定する。同単位内のprovisional消費で反証を回避させない。過去に正規消費されたEXECの全取引を新しい規則で一括rollbackする変更は加えない。

### I5 永続化、復元、公開起動、既存機能への接続を完成させる

実装済みの各縦経路を、live virtual processor、native row、atomic delta、journal、pruning、restart、reorg、IBDへ同時に反映する。保存するのはoriginal、draw/seed、承認、Q/CaseId、供給義務、検査前提、原時計、予約と実service、未払・留保、credit/schedule、消費・失効、署名の一回性である。

rootに認証された不在と取得不能を分ける。欠けたQを空、raw journalを承認済み、別branchの評決を現branchの評決と扱わない。復元のためにNodeへモデルを取得させない。依存claimの取消は権利を伝播停止し、consumerを自動的に計算不正としてslashしない。

RFC13の公開bootstrapと初回PALW/creditの循環を接続し、未知seed等で開始不能ならその理由を明示する。RPC/SDK/CLIは「署名受理」「検証待ち」「Qで保留」「Final」「未検証Abort」「反証」「取消」を区別する。実際の公開送受信とnative包含まで接続し、private helperのみを完成扱いにしない。

### I6 必要な共有処理と最終実装候補をまとめる

同一model rootの公開cache、認証済みchunkの再利用、独立作業の並列化を、既に完成させた権利と責任を変えずに接続する。費用削減は実際に共有できたbytesと固定費だけに適用し、cold参加者の費用をcache hitで隠さない。

atomic batchは任意最適化であり、単一claim経路を完成させるための前提にしない。初回候補で有効にする場合は、同Miner/model/profileの有限leafを受理前・抽選前に固定し、一leaf反証で全体失敗、部分Final/creditなし、全leafの検証・仕事・価値・最悪case合算、一回会計を一式で実装する。含めない候補では入口を明示的に未対応にし、batchを検証済みと報告しない。後から既受理claimを吸収する方式を追加しない。

全RFC対応表、consumer一覧、fixture、実行runner、候補identityと結果schemaを仕上げる。第8節の実装出口を満たしてから、検証工程に入る。

runnerの段階分けを実装で強制する。長時間ケースは専用target・suite、または明示的な除外/選択規則でV1〜V4から外す。`ignored`を使う場合もV5/V6で名前を指定して必ず実行し、未実行・skipをPASSにしない。既存の無filter nextestへ長時間ケースを追加するだけでは、この順序を守ったことにならない。

## 6. 共通の状態と権限の設計

`ResultCommitted → VerificationPending → CertificationReady → Final`を新profileの正常状態遷移とする。Qと資料caseは承認票とは独立して保存する。資料供給完了は検査完了でも完全再実行承認でもない。

成功判定は、規範を所有するRFC20に合わせ、一つのnativeな派生判定へ集約する。実装上の型名は別途確定するが、意味は次を欠かさない。

```text
certification_ready =
    完全に定義された有界profileが受理済み
    AND original / draw / 署名domainが正しい
    AND 必須の局所positive検査が成立
    AND 同一subject全体への選出者の完全承認が1件以上

can_success_final =
    certification_ready
    AND notice窓終了・所定の公開検査機会成立
    AND Qが空
    AND 未解決の供給・実service不成立がない
    AND 依存・資金・責任・原期限の条件が成立
    AND 反証・取消・Abort・失効ではない
```

この値をpeer/RPC/workerから受け取ったboolで設定しない。nativeの認証済みstateから導出する。`CertificationReady`に達しただけでは報酬・creditを発生させない。Final後に適格なcaseが開けば、現在の起源権利の使用もholdする。

最終consumer台帳には、仕事報酬、escrow支払、worker/service支払、mint、safe frontier/weight、settlement depth、Rule E、settled anchor、EXEC credit予約、schedule、permit消費、依存export/useを列挙する。各consumerの既存の資格・fee同意・支払目的は保つ。成功仕事の資格と、実際に履行した資料供給の対価や安全な返金を一括して同じ条件に変えない。

## 7. RFC 20本と実装工程の対応

以下の番号はすべて**現行RFC-v2**である。I番号は第5節、V番号は第9節を指す。

| RFC | 実装へ反映する項目 | 主工程 | 検証で確かめる出口 |
| --- | --- | --- | --- |
| [01 推論・生成Job](../../../rfc-v2/0001-inference-and-generative-jobs.md) | prompt/tokenizer/token列、seed、decode、停止、入出力をoriginalへ束縛 | I0–I1 | V2/V3/V5: 別入力・別tokenizer・偽停止を受理しない |
| [02 TIR・Kernel](../../../rfc-v2/0002-tensor-ir-and-shared-query-data.md) | 数値意味、全関係、初回区間trace、query identity | I0–I2 | V2/V5: 採用全関係の正例と偽の元recordを裁く |
| [03 登録・Artifact](../../../rfc-v2/0003-model-onboarding-and-artifacts.md) | 不変artifact/program/layoutと公開chunkの認証 | I1/I3 | V2/V5: すり替え拒否、初取得・再配布から検証可能 |
| [04 二階層・年次カタログ](../../../rfc-v2/0004-model-tiers-and-annual-catalog.md) | 1.0/0.3、係数一回、全tierのclaim検証 | I4–I5 | V4: tierを検証免除にせず既存会計を保つ |
| [05 資源・長文脈](../../../rfc-v2/0005-bounded-resources-and-long-context.md) | cold取得、全再実行、trace保持、全case・包含予約 | I0/I3–I6 | V5/V6: 最悪合法工程とEXEC共用負荷を収容 |
| [06 Panel・抽選](../../../rfc-v2/0006-panels-and-probabilistic-verification.md) | commit後global抽選、成熟bond、再抽選禁止、モデル未保有者 | I1/I3 | V2/V3: 独立選出、非選出署名・旧署名を数えない |
| [07 Claim・EXEC会計](../../../rfc-v2/0007-claims-exec-and-answer-accounting.md) | 有限ExecCredit、一回mint、reserved/consumed/revoked/expired | I1/I4 | V3/V4: 二重仕事・同単位消費逃れ・期限切れを拒否 |
| [08 依存・責任](../../../rfc-v2/0008-claim-dependencies-and-liability.md) | KV/prefix/source由来、依存期限、失効伝播 | I1–I5 | V2/V4: 別源を拒否し、取消と責任を分離 |
| [09 Remote・SDK](../../../rfc-v2/0009-remote-client-and-sdk.md) | 公開commit/要求/応答/証拠/承認/状態API | I1–I5 | V3: 実transportとcanonical包含から終端へ到達 |
| [10 DAG・Native決済](../../../rfc-v2/0010-dag-consensus-and-native-settlement.md) | pre-Final認証、反証優先、ScheduleId、現在slot、settlement | I1/I4–I5 | V4/V6: 単一gate、取消、1 BPSとの共存条件 |
| [11 Bond・Fee](../../../rfc-v2/0011-bond-budgets-fees-and-payments.md) | 予算、同意、役割別帰責、返金、留保、会計保存則 | I1–I5 | V2/V4: 未検証と有罪を分離し、支払目的を保つ |
| [12 State・IBD](../../../rfc-v2/0012-authenticated-state-storage-and-ibd.md) | 新全状態、delta、journal、pruning、IBD/reorg | 各IとI5 | V4/V6: 同state再現、Q/責任を失わずNodeモデル不要 |
| [13 Bootstrap・Release](../../../rfc-v2/0013-permissionless-bootstrap-and-release.md) | 公開参加、BASE回復、登録・成熟、初回PALW/credit | I0/I5 | V4: 起動循環と認証免除を作らず、未決gateを報告 |
| [14 Court統合契約](../../../rfc-v2/0014-court-master-contract.md) | 受付、duties、資金、時計、default、残存責任 | I2–I5 | V2/V3/V6: 全合法分岐の有限終結と帰責 |
| [15 凍結・延期・管理](../../../rfc-v2/0015-freeze-deferred-rejected-and-governance.md) | 版分離、延期入口、未採択案、activationの分離 | 全工程 | V1/V4: 旧意味保持、延期機能の迂回有効化なし |
| [16 Miner commit・供給](../../../rfc-v2/0016-miner-execution-commit-and-material-supply.md) | Miner初回trace、全manifest、有限供給/保持 | I0–I3 | V2/V5: 元claimへの帰責とcold取得 |
| [17 Positive](../../../rfc-v2/0017-noninteractive-positive-verification.md) | 全scope再実行、一括承認、最低1件、必須局所検査 | I1 | V2/V3/V5: 部分承認で全体成功にしない |
| [18 Negative・Default](../../../rfc-v2/0018-interactive-negative-court-and-defaults.md) | 一人要求、Q/CaseId、取得後検査、直接反証、客観default | I2–I3 | V2/V3/V5/V6: 秘匿を成功にせず虚偽caseも有界終結 |
| [19 小さなNode](../../../rfc-v2/0019-model-free-node-and-bounded-evidence.md) | 全関係の局所証拠、native上限、全モデルfetch禁止 | I0–I3/I5 | V2/V5/V6: bytes/hash/parse/heap/IO込みで上限内 |
| [20 成功gate](../../../rfc-v2/0020-verification-gated-final-and-adversarial-models.md) | 状態遷移、公開検査機会、Q、Final/Abort/Revoked、全consumer | 全工程 | V3–V6: 一人反証優先と正常claimの条件付き完了 |

延期対象の「旧RFC-0003の画像・動画」「旧RFC-0004のモデル改善」と、現行v2-03の登録・v2-04の二階層を混同しない。画像・動画・改善protocolは`DEFERRED_POST_MAINNET / DISABLED`を維持し、共有入口からの迂回を検査する。Panel=0、未採択のAccount Bondや新報酬案、provider custody移管案を新profileへ混入させない。[RFC15](../../../rfc-v2/0015-freeze-deferred-rejected-and-governance.md)を区分の正本にする。

## 8. 通し実装の出口

次をコードレビューと変更台帳で確認してから、検証の実行へ切り替える。この確認自体は動作PASSではない。

- 新profileがpublic RPC → consensus受理 → Panel/資料 → native court → Final/credit/決済まで接続され、別fixtureだけで成功する経路になっていない。
- 正常、反証、資料不足、遅延、虚偽case、依存失効、再起動/reorg/IBDのhandler、保存、回復、外向け状態が実装されている。
- 全consumerが同じ現在の権利判定を使い、旧certificate、ローカルbool、raw資料、Final一覧からの別発行経路がない。
- 採用kernelの全関係対応表とchecker/証拠抽出の対応が揃う。未知演算のgeneric runner fallbackがない。
- 第4節のprofile手順が具体化されている。未確立のままなら残件として保持し、正常完了を含む候補の完成扱いにしない。
- 旧版と新profile、延期・未採択機能のfenceが区別される。元の履歴・測定・署名の意味を変更していない。
- 必須試験のID、入力、期待するnative行と経済効果、実行する実関数/入口、必要機材、timeout、出力先がrunnerへ登録されている。
- `TODO`やstubが残っても成功側へ到達できないこと、必須作業の未実装を「未対応だから安全」として完了に数えていないことを確認する。

## 9. 実装後の通し検証

### 9.1 候補を固定する

検証開始時に、source treeまたはcommit、dirty差分、lockfile、toolchain、feature、binary、profile/config、fixture/model/artifact/tokenizer/program、kernel/数値規則、機材、シナリオ一覧のdigestを記録する。原入力ZIPのhashは出発点の識別であり、変更後binaryの識別には使わない。

結果は候補ごとの新しい出力ディレクトリに保存し、過去の採択・レビュー・試験ログを上書きしない。runnerは終了コードだけでなく、実行したtest名と件数、必須ケースの充足、skip/ignored/timeout、各native終端と経済効果を記録する。空のsuiteや`--no-run`をPASSにしない。

### 9.2 実行順序

| 段階 | 実行する検証 | 次段へ渡す結果 |
| --- | --- | --- |
| V0 候補と環境 | identity、機材、必要artifact、runner登録、出力先、未決gateを確認 | 実行可能な候補とケース一覧。欠けた資材はNOT_RUNとして区別 |
| V1 静的検査とbuild | format、lint、check/build、codec/domain/fence、docs、必要featureの登録 | compile可能な候補。既存失敗と新規回帰を分ける |
| V2 短い単体・状態機械 | 元root認証、全関係、raw/packet、Q/default/時計、抽選、会計保存則、権限 | 個別関係とnative遷移の健全性 |
| V3 小規模の実経路E2E | 公開transport、canonical包含、Miner/Panel/Node分離、正常/反証/資料case/Final/credit/支払 | stubではない入口から出口の接続 |
| V4 復元・互換・経済 | restart、reorg、IBD、pruning、依存取消、同batch全順序、旧署名、bootstrap、全consumer | 保存と経済効果の一貫性。未知の起動/報酬条件はgateとして残す |
| V5 最終計算群 | 実LLM、モデル未保有Panel、正常/不正/秘匿/遅延/悪意不署名、長文脈vector | 対象実モデルでの全経路、測定範囲と残前提 |
| V6 最終負荷群 | 最大合法case、最遅応答、混合claim/EXEC/court、最低Node、長時間・障害回復・保持 | 予約に対する実service、期限内終結、性能・容量の分母付き記録 |

V1〜V4で、型不整合、認証bypass、誤slash、Q消去、二重支払など重い試験を無意味にする失敗が見つかった場合は、同じ段階の独立な検査結果を集めたうえで修正する。未修正の候補へV5/V6を投入しない。これは実装途中の小分け検証ではなく、通し実装後の検証キャンペーン内の依存順である。

同じ試験をpackage別実行と全suiteで無意識に重複させない。変更対象の必須coverageと既存CIで必要な回帰を一つの実行台帳に割り当てる。既存gateで不足する新ケースを追加し、過去のprivate suiteのPASSを新profileの代わりにしない。

### 9.3 必須のケースと判定

| ID | シナリオ | 必須の判定 |
| --- | --- | --- |
| T01 | coldな正直Panel一人、他は不署名、正常Miner | 完全承認1件と他の条件で正常Final。全員ACKを要求しない |
| T02 | 他全員が承認、元traceに一つの誤り | 唯一正直者の反証でRefuted。Final/報酬/creditを与えない |
| T03 | 出力は一致するが途中trace、read-from、KVまたはstateが不正 | 出力一致だけで承認せず、該当必須関係の反証が成立 |
| T04 | 偽の停止、長さ、仕事量、RNG/選択規則 | End/control/accountingまで元claimへ結び付けて裁定 |
| T05 | 正直Panelだけに資料、manifest、chunk索引、初期依存一覧を隠す | モデル・完全再実行root・算術証拠なしで有界要求とQ。多数承認でも成功不可。責任別defaultまたは未検証Abort |
| T06 | 遅いdraw/取得開始、notice直前の要求・包含、notice後の最終資料到着と矛盾発見 | notice前と後の二段予約、同CaseIdの検査・反証権を維持。供給だけでQを消さず、送信だけを包含としない |
| T07 | URL/他Panel receipt/一部chunk/quota消尽を全取得と主張 | 検査前提を作らず、正直者のdefaultにしない |
| T08 | 正常Minerに最大合法数の虚偽case・最遅応答 | 全てを客観的に有限終結し、前提成立時は原期限内に正常Final |
| T09 | 検査前提と当人の提出機会成立後に申立人が次手を放棄 | 当該caseのみChallengerDefault。別case・権利・供給義務を消さない |
| T10 | 不正packet、偽opening、別claim/型/座標/モデルの証拠 | packet拒否のみ。既存Qは維持、Minerを有罪にしない |
| T11 | 正規packetで認証されたMiner raw record自体が不正 | WellFormedの反証。外側packet不正と同じ処理にしない |
| T12 | 共有資料を悪意者が先に要求し、自分のcaseをdefault | bytes共有を維持しつつ、正直者のCaseId/予算/期限を保持 |
| T13 | 同canonical単位に反証/Q holdとFinal・旧credit消費・fee | オブジェクト順序に関係なく取消/hold優先。二重支払・消費逃れなし |
| T14 | notice境界、hard当日の合法応答、hard+1、再送 | 端点の規則とeffect identityを守り、時計reset・成功復活なし |
| T15 | 依存取消、restart/reorg/IBD/pruning、欠損state | 同state/経済効果を再現。取得不能をQ空へ変えず、モデル全量fetchなし |
| T16 | 無料permit、旧certificate、Final一覧、slice等から権利を生成 | 単一発行gateを迂回できず、TxとSliceの意味を保持 |
| T17 | 未知kernel、上限外証拠、過剰decode/hash/heap | allocation前に有界受付。別kernelや全モデルrunnerで救済しない |
| T18 | 旧claim/署名、既存tier、延期media/改善入口 | 旧意味と回帰を保持し、新profileで未採択経路を開かない |
| T19 | 初回bootstrap、creditなし、seed未成立 | EXEC先在や認証免除の循環なし。未承認entropyなら未対応を明示し、有効化しない |
| T20 | atomic batchを採用する候補 | 全leaf事前固定、一leafで全体反証、部分creditなし、再送・分割二重計上なし |

T02〜T04/T11は、採用する全relation familyへ展開する。単一のClampや一layerで全coverageを代表させない。小規模では独立した期待値と故障注入で各関係を検査し、V5/V6では実LLMの同じnative経路で対応する攻撃を確認する。

## 10. 最後にまとめる計算と負荷の測定

V5/V6は、軽い試験を通した候補の同一source・binary・profile/configに対する一つの計画として実行する。Miner、Panel、provider、Nodeは分離し、Nodeにモデルファイルと推論runtimeを配置せず、裁定から外部model endpointへ接続していないことも観測する。

cold条件は、「初めてのモデル」という表示だけでなく、そのPanelのmodel/cacheが空であり、計測開始後に取得したbytesと認証処理を記録する。共有cacheがある場合は別条件として測る。fixture生成時に先読みしたモデルや、warmなOS/file cacheを無記録でcold結果に含めない。

| 測定対象 | 記録するもの |
| --- | --- |
| Miner | 基準推論、初回capture、hash、trace生成、保存、供給の時間・CPU/GPU・メモリ・bytes。測定範囲をそろえた追加費用 |
| Panel | cold取得、認証、全体再実行、比較、局所化、opening構築、署名・提出を含む全工程 |
| Node | packet/wrapper/署名/openingを含むbytes、parse/hash/算術work、CPU、peak heap/RSS、I/O、同時case合計 |
| serviceと期限 | notice前の初回要求/反証のcanonical包含とhard内の取得後検査・裁定を分け、予約、実供給、最遅手番、全合法分岐、回復、残余、未完了・Abort件数を記録 |
| 保存 | trace/chunk/証拠/native stateの保存量、保持中の増加、pruning後の責任・復元可能性 |
| 容量 | model数×claim頻度、cold/warm比、Panel全再実行量、抽選・署名、配信、court、包含の別々の需要 |
| EXEC | 有効credit量、予定slot、実canonical block、空slot/欠落の理由、TPS・受理遅延・決済遅延の別々の値 |

EXEC 1 BPSはglobal EXEC_TXの条件付き目標であり、claim/s、court/s、1秒finality、DAA/秒の値に変換しない。旧4,096抽選位置／75枠の容量計算も、新profileの測定値にしない。同じNode資源を使うEXECとcourtを合算し、Node台数を増やしただけで一Nodeの上限が増えたと扱わない。

最大合法な要求・虚偽caseと、正しいMinerの防御応答を同時に投入する。各選出bondの枠、第三者枠、包含枠を他者が先取りしても、唯一正直者の予約が残ることを確認する。成功例だけの中央値ではなく、全受理件数、全終了状態、p95/p99/最大、timeout、資源不足、分母を保存する。性能数値は測った構成だけに適用し、未知のモデル規模やkernelへ外挿して有効化しない。

資源超過を拒否できることに加え、少なくとも一つの対象実LLMが正常Finalと局所反証・不供給終了まで動くことを必須にする。全拒否・全Abortで安全性だけ成立しても、有用なprofileの完成ではない。

## 11. 検証runnerと既存コマンドの扱い

次は後続の検証工程のためのコマンドであり、本方針作成時の実行結果ではない。新profile用の統合runner・ケースは実装工程で追加する。新しいファイル名、CLI、test名はその実装で登録するまで「予定」と表示する。

### 11.1 共通検査

既存入口は[scripts/ci-gates.sh](../../../../scripts/ci-gates.sh)。検証manifestに実行対象を固定し、同じgateを重ねて実行しない。

```sh
bash scripts/ci-gates.sh --list
bash scripts/ci-gates.sh --group fast
bash scripts/ci-gates.sh fmt clippy pq-guard check
NEXTEST_PROFILE=ci bash scripts/ci-gates.sh nextest derive-suite doctest
```

必要featureの`build-devnet-prealloc`、`check-evm-send`、hash/doc/example等の既存CI gateも、変更範囲とrelease要件に従って台帳へ割り当てる。単に上の四行だけを実行して全CIと表現しない。

現行`nextest` gateは無filterの通常suiteなので、新しい長時間ケースをそのまま混ぜない。第5節I0/I6のtarget分離を実装し、V0で通常群とV5/V6群の列挙を照合する。通常群から除外した必須ケースは、最終群で実行名・件数を確認できるまで未検証のままとする。

`NEXTEST_PROFILE=ci`は既存の`fail-fast = false`とJUnit出力を使い、一つの失敗だけで独立な検査結果を失わないために指定する。ほかのrunnerも、失敗・未実行の内訳を残し、集約終了コードで個別失敗を隠さない。

`rust-toolchain.toml`のpinは`1.93.0`。Cargo default-membersは外部pinned llama.cppを要する`misaka-palw-worker`を除いているので、無条件の`--workspace`やrootの`./test`へ切り替えない。workerを対象経路で使う場合だけ、依存runtimeとbinaryを固定してその検証を別に計上する。Nodeのbuild/run要件へモデルruntimeを戻さない。

### 11.2 既存部品の回帰

```sh
bash scripts/ci/a3-panel-ci.sh
bash scripts/ci/positive-court-a4-ci.sh
bash scripts/ci/positive-accounting-node-ci.sh
```

これらは旧private EXACT_STREAM、Positive Court、accounting adapter等の回帰に使う。新PublicOneHonestの公開native E2E、cold配信、唯一正直者、全LLMの証明には代用しない。

既存の`testing/positive-court-a7/run.py`、`real_llm_process.py`、`process_control.py`からcandidate固定、process管理、timeout、manifestの仕組みを再利用できる。ただし旧runnerの`cold_claimed: false`や未検証scopeを消すだけで新しい保証を作らず、実際の公開資料・native carrier・状態・決済を接続したrunnerを用意する。

### 11.3 文書と長時間群

```sh
python3 -B docs/maintenance/check_docs.py
python3 -B docs/rfc-v2/check_integrity.py
python3 -B docs/adr-v2/check_integrity.py
```

採択snapshotを照合するときは、変更を許可された箇所と歴史保全の範囲を分ける。既存のリンク欠損やsnapshot不一致を隠すためにbaselineを一括更新しない。過去の有限モデルrunnerが既存JSONを上書きする場合は、検証用コピーと新しい出力先で実行する。抽象モデルのPASSはnative runtimeのPASSではない。

長文脈の既存入口は次であり、V5/V6へ置く。

```sh
bash scripts/ci-gates.sh context-vectors
```

実LLMとcold/負荷のrunnerは、第9節の必須ケース、candidate、timeout、全資源capをmanifestから受け取り、成功・失敗・未実行を区別して出力するよう実装する。正確な既存test名とコマンドは、コード統合後にmanifestへ固定する。ゼロ件実行や旧profileだけの合格で必要coverageを満たしたことにしない。

## 12. 不合格時の修正と独立レビュー

通し実装後に、執筆・実装の主担当とは別の担当が次の三方向を確認する。

| 担当 | 主に確認するもの |
| --- | --- |
| 計算・反証レビュー | 全関係のcoverage、Miner初回commit、証拠抽出、checker健全性、局所資源上限 |
| 状態・敵対者レビュー | cold唯一正直者、資料case、検査前提、全合法虚偽case、時計、Q、反証優先 |
| 統合・会計レビュー | 公開入口、live native fold、全consumer、二重計上、依存取消、復元、bootstrap、旧版互換 |

レビューで見つかった問題は、規範、再現条件、影響する経路、修正、再検証対象、候補identityを台帳に残す。合格のために前提や必須ケースを緩めず、private fixtureへ逃げたり、失敗ケースをskipにして完了扱いにしない。

修正後は、まず原因に対応する短い検査と影響する縦経路を再実行する。共有型、署名、数値意味、state/会計、資源値が変われば影響する範囲を広げる。必要な修正をまとめた次の候補で、長時間群を再度まとめて実行する。無関係な重い試験を修正ごとに反射的に全再実行しない。

binary/profile/configが変わった場合、旧候補の長時間測定を新候補のPASSへ付け替えない。最終候補に必要な長時間・統合結果をそろえ、独立レビューの必須問題が解消するまで反復する。設計手順が未確立、実資材や機材がない、資源が収まらない場合は、その問題を未解決として示す。試験の無限再実行や全拒否で解決したことにしない。

## 13. 完了の判定と進捗の表示

| 判定 | 必要な証拠 |
| --- | --- |
| 設計の具体化完了 | 全scope資料手順、全関係対応、状態・資源・責任・版が具体的に定義されている |
| 通し実装完了・未検証 | 必須の公開入口から経済出口と復元までコードがつながり、fixtures/runnerも存在する |
| 基本検証完了 | 固定候補のV1〜V4と必要回帰が成立する |
| 対象profileの実LLM検証完了 | 同じ候補でV5/V6、正常Final、一人反証、秘匿/遅延/悪意case、Node上限を確認する |
| 有効化を判断できる | 未解決の設計・実装・測定・bootstrap・経済gateがなく、独立レビューと移行資料が揃う |
| 公開有効化済み | 別途、既存の承認手続きとfence/高さの適用を完了している |

実装担当の進捗報告は「RFC何冊を編集したか」「関数を何個追加したか」ではなく、正常/反証/資料不足/復元/creditという経路ごとの接続状態と、残る阻害点を中心にする。計画時点ではbuild・native runtime・実LLMの検証結果を付与しない。

採択済みの`ADOPTED_DESIGN_NOT_ACTIVATED`は、計画を書いたことやコードがcompileしたことだけで変更しない。新profileの保証は一人の能動的な正直者、取得・提出・包含、native規則の実施という条件に基づく。全員共謀や永久検閲への無条件保証に拡張しない。

## 14. 最初の実装作業へ渡す成果物

この計画から実装を始める際は、次の順で作業を起票する。

1. **共通契約パック** — 新original/trace/manifest/full-replay承認、Q/CaseId、期限・予約、単一成功gate、consumer一覧、全関係coverage表。
2. **資料手続きの具体化** — 全scope取得・検査機会のnative根拠と有限終結。第4.1節の未確立gateを隠さず解決対象にする。
3. **一つの正常claimの縦断実装** — 第5節I1の実コード接続と対応fixture。旧契約の意味を保つ。
4. **反証・資料不足・復元・EXECの全分岐実装** — I2〜I6を統合し、同じcandidateへ収束させる。
5. **実装候補固定後の通し検証キャンペーン** — V0〜V6と独立レビュー。長時間計算はV5/V6に集約する。

主担当は共通契約とnative状態・成功gateを所有し、Miner/Panel・資料/court・EXEC/保存の担当を並行化できる。ただし各担当の完了は部品の存在ではなく、合意した入口・出口への接続で判断する。重い測定は共有機材の取り合いで結果を歪めないよう、最後に実行枠をまとめて割り当てる。

### 方針書の独立レビュー

2026-11-10に、本文の作成を担当していない2系統が、計算・反証と、実行工程・既存コード対応の観点からレビューした。manifest未取得時の要求入口、notice前とhard内の二段予約、長時間targetの分離、nextestの通し実行profileを補強し、両系統が修正後を再確認した。**最終判定は、実装方針として2系統ともPASS。計画に対する未解決指摘はない。**

この判定は第4節の未確立手順を解決した認定でも、runtime・実LLMの試験結果でもない。設計の具体化、コード接続、検証、測定、有効化は第13節の別々の条件を満たす必要がある。

### 根拠となる文書とコード

- [上位原則](../../../PRINCIPLES.md)、[現行RFC索引](../../../rfc-v2/README.md)、[RFC-0050](../../../rfc/0050-public-materials-and-one-honest-panel.md)、[ADR-0218](../../../adr/0218-public-materials-and-one-honest-panel.md)。
- [最新改訂のREADME](../../../maintenance/revisions/2026-11-09-public-panel/README.md)、[独立レビュー](../../../maintenance/revisions/2026-11-09-public-panel/REVIEW.md)、[検査記録](../../../maintenance/revisions/2026-11-09-public-panel/VALIDATION.md)。過去の条件付き規範PASSと有限モデル結果は、新コードの検証結果ではない。
- [Miner/Panel旧署名契約](../../../../misaka/palw/positive-contract/src/messages.rs)、[旧PositiveChecked契約](../../../../misaka/palw/positive-contract/src/certification.rs)、[Result固定後の既存native認証](../../../../consensus/core/src/palw_state_v2/positive_assignment_certification.rs)、[旧Final後の認証](../../../../consensus/core/src/palw_security_certification_fold_v1.rs)。
- [共有資料intake](../../../../consensus/core/src/palw_court_shared_material_intake_v1.rs)、[小型局所証拠](../../../../consensus/core/src/palw_public_evidence_package_v1.rs)、[live processor](../../../../consensus/src/pipeline/virtual_processor/processor.rs)、[native settlement](../../../../consensus/src/pipeline/virtual_processor/native_settlement.rs)。
- [実LLM既存profile監査](../../../../testing/positive-court-a7/test-compatible-llm-audit.md)、[旧A7検証の範囲](../../../../testing/positive-court-a7/README.md)、[既存CI runner](../../../../scripts/ci-gates.sh)、[Cargo workspace](../../../../Cargo.toml)、[toolchain pin](../../../../rust-toolchain.toml)。
- [パラメーター台帳](../../../governance/parameter-register.md)、[公開beacon bootstrap](../../../../consensus/core/src/palw_beacon_bootstrap_v1.rs)。既存の不整合や未承認profileを、計画内の任意値で解消した扱いにしない。
