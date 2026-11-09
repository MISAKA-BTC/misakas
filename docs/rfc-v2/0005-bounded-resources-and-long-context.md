# RFC-v2-0005 — 資源上限・長文脈メモリ・計測

## 2026-11-09公開資料・一人保証の追加境界

**適用順:** 本節と公開profileについての追加条項は、先行採択の同じ論点を更新する。公開profileの成功条件・atomic batchの規範はRFC20を正本とし、後段の2026-11-08／実行整合性改訂の説明は、矛盾しない資源・保存条件として継承する。旧署名・歴史付録の適用条件は変更しない。

[RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)／[ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md)で今回採択する公開profileでは、各claimの独立選出Panelに少なくとも一人の能動的な正直者がいることを前提とし、その人がモデル未保有の状態から資料取得・全体検証・反証提出へ進める予約を要求する。全体承認の最低件数は[20](0020-verification-gated-final-and-adversarial-models.md)の新profileで1件とするが、選出人数、再実行義務、資料取得量、全合法caseの処理量を1人分へ自動的に縮めない。安全性と、悪意多数の不署名・虚偽異議の下でも正常claimを期限内に完了できる条件を分ける。旧policyの75／41と4,096抽選位置／日は履歴・旧版に保持する。新profileの実供給能力、人数・資源数値・有効化は未確定であり、設計状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

## 2026-11-09新profileの追加境界

[採択RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)と[ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)により、[20](0020-verification-gated-final-and-adversarial-models.md)の実行整合性・一人の反証・検証未成立時の成功停止を適用する。一人の適格な検証者の資料取得・提出・裁定容量を悪意側の先着予約から守り、18/19/20の全分岐と総負荷を予約する。 詳細の規範所有者は16〜20であり、本追記で別の成功条件を定義しない。旧claim・歴史付録の意味とbytesは保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

原統合: 2026-10-29 / 採択改訂: 2026-11-08 / 原入力: `misakas-pre(5).zip`

[20本の索引](README.md) · [旧→新対応表](MAPPING.md) · [編集・移行規則](MIGRATION.md)

> **2026-10-29原統合の編集範囲:** 文書の統合・重複する責務の整理。技術仕様の採択、パラメーター変更、runtime有効化、試験成功の宣言ではない。
> **状態:** 採用済み・Draft・隔離実装・履歴を含む統合編集版。各条項の採択・置換関係を継承する。過去の「実装済み」「休眠」「未試験」等は、その記録時点の状態である。
> **番号:** 本書は`RFC-v2`。原条文中の`RFC-xxxx`と`§`は旧版を指す。旧番号・wire・署名・fence名は再採番しない。

## 2026-11-08採択による適用範囲

資源上限の共通規則を維持する。[19](0019-model-free-node-and-bounded-evidence.md)でNode一件・claim・block全体の上限を定め、[20](0020-verification-gated-final-and-adversarial-models.md)でcold取得・再実行・最後の合法negativeを元時計に収める。未測定の値を性能保証や受理可能なclassとして扱わない。

[RFC-0048](../rfc/0048-exec-credit-schedule-and-one-bps.md)・[ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md) の追加採択により、EXEC の二層設計と 1 BPS 目標に接続する資源契約、および court の待機・局所化・搬送による DAA 予算膨張を是正する完成条件を本書に加える。1 BPS は供給能力の実測値ではなく、court の資源上限や元の期限を緩和する根拠にもならない。

**優先関係:** 新profileに限り、本節・RFC16〜20・[ADR-0215](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md)・追加採択の [ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)・[公開裁定の方針v0.2](../design/palw/consensus/public-adjudication-policy-v0.2.md)を適用する。以下の基礎契約・旧profile記録と原付録は保持し、旧署名や旧Finalの意味を遡及変更しない。設計採択は実装・測定・有効化の完了を意味しない。

**原文収録元:** [旧0020](../rfc/0020-long-context-calibration-memory.md)。

**関連RFC:** [02 TIR・Kernel・共有Queryデータ](0002-tensor-ir-and-shared-query-data.md) / [03 モデル登録・変換・Artifact](0003-model-onboarding-and-artifacts.md) / [06 Panel参加・抽選・確率的検証・認証Receipt](0006-panels-and-probabilistic-verification.md) / [12 認証State・保存・Pruning・IBD](0012-authenticated-state-storage-and-ibd.md) / [14 Court統合契約](0014-court-master-contract.md) / [15 設計凍結・延期・不採用・未採択・変更管理](0015-freeze-deferred-rejected-and-governance.md)。

## 1. 対象

旧0020を中心に、既存計算意味を維持する較正・変換・忠実度計測・メモリ最適化を扱う。モデル改善protocolではなく、固定モデルを処理する道具と資源管理である。新しいcontextや演算を自動承認するRFCではない。

## 2. 意味を保ったメモリ削減

32K等の対象条件が持つ依存関係を保ち、同時に必要でないweight、activation、carry、作業領域を同時に保持しない。streaming、lazy読み込み、spill、再計算は、出力と検証関係の同一性を保った範囲で使う。

## 3. 全工程の資源計上

peak RAM、ディスク、一時領域、IO、CPU、保持byte期間を分ける。入口で予約した資源を、別経路で二重使用しない。異常終了、キャンセル、再起動時も未解放の予約やspillを追跡する。

TIRのadmission上限は02、登録ツールの制限は03、courtの全分岐予約は14、pruned nodeとIBDの保存契約は12を所有先とする。本RFCがそれらの数値を独自に緩和しない。

### 3.1 owner 専有待ちと資料の共有

新 profile は、資料 owner の排他的な席が順番に解放されるまで待つことを、公開取得・局所化・証拠提出の必須経路にしない。モデルを事前に持たない適格 outsider が、owner の選別や受領署名に依存せず、認証された共通資料と予約された公開経路を利用できることを要求する。通常 owner・Miner・モデル内 Panel がすべて結託する場合も、この要求を外さない。

canonical な blob／range と original root に対する認証を使い、同じ資料の配信・保存・先読みを共有できるようにする。資料を供給する worker は owner と別に割り当て可能とし、独立な取得・認証・局所証拠作成を並列化する。ただし、前の応答から次の争点が決まる処理、資料が到着するまで開始できない検査、依存境界の確定など、真の逐次依存は保持する。直列和を一律の `max()` に変えたり、worker 数だけで総帯域が増えたと扱ったりしてはならない。

共有による削減を計上できるのは、同じ認証済み bytes が必要な時刻・利用者へ実際に供給され、保持と取得の契約が満たされる範囲だけである。root や URL が一致しただけでは転送・読出しをゼロにしない。blob を再利用しても、別 claim・座標・役割・規則版への proof binding、署名検査、提出機会、決済の検査は残す。共有資料の契約と各当事者の責任の分離は [18](0018-interactive-negative-court-and-defaults.md) に従う。

### 3.2 依存 DAG、資源総量、実行可能な予約

受理対象ごとに、取得開始から局所化・証拠作成・認証・収録・裁定・必要な復旧までを task の依存 DAG として表す。各 task は、資料が利用可能になる最早時点、前提 task、手番ごとの締切、実行場所、資源需要、共有可能な資料、再開時に必要な仕事を持つ。私的 owner が任意に応答するという期待を、保証された資料 release 時刻として使わない。

受理済み全 claim の集合を A、許可された要求上限内で同時に起こり得る継続を ω、処理対象の期限幅を D とする。少なくとも次の二条件が必要である。

~~~text
critical_path(A, ω) <= D
required_work(A, ω, resource, interval)
    <= guaranteed_service(resource, interval)
~~~

二つ目は、CPU、署名・hash・parse、disk I/O、転送 bytes、制御メッセージ、chain 包含などの資源ごとに、要求が release されてから各締切までの必要な区間を対象にする。既存予約を差し引いた容量を使い、全期間の平均だけで短い手番の不足を隠さない。保持メモリ・一時領域・ディスク容量は別に、各時点の同時占有量と保持終了までの契約を検査する。容量の単位を処理率に取り違えない。

これらは必要条件であり、単独でも二条件の組でも受理の十分条件にはしない。**既存予約、release 時刻、依存関係、各手番、元の期限を満たす実行可能な予約 schedule が必要である。** 未知の将来要求を先取りして選べる架空の schedule ではなく、実際に判明した要求に従って全合法継続へ対応できる割当規則と、その供給機構を持たせる。予約の取得・受付・資金の確保を同じ native 契約へ結び付け、受理後に初めて空き容量を探す設計にしない。

全分岐とは、一人の申立人が選ぶ最短の一経路に限らない。複数の攻撃者が異なる合法 range を求める場合、複数 claim の同時係争、重複・重なり・非重複資料、最後の notice と最遅応答、公開 fallback、許容された再開を含む。相互排他的と認証できる分岐だけが同じ予約を共用できる。同時に合法な分岐を「どれか一つ」と仮定して容量を再販売しない。有限な要求数・費用・資格と合わせ、claim 内および全 claim の累積予算を検査する。

### 3.3 original の時計と契約の整合

original 受理点 a、notice の `a + 3,120`、係争 hard の `a + 10,000`、最遅 notice 後の `6,880 DAA` を維持する。最遅 notice からの残り予算には、資料取得、局所化、合法な最遅応答、証拠提出・収集、包含、裁定、許容された reorg・再起動の回復を含める。準備・Open・保持・収集を別の時計として明示し、重ねられる区間だけを重ねる。保持義務を収集終了までの直列待ちにせず、保持延長によって notice／hard を延ばさない。

model data の最後の chunk、依存する opening、最後の合法署名が届く時刻まで計上する。計算・取得前に署名した有効期限・保持期限と、実際の original 受付時点で残る期間を照合し、古い見積時点の残期間を再利用しない。署名済み責務を受理後に無同意で延ばさず、必要なら未受理の段階で正規の再同意を得る。受理済み original の時計はこの処理で更新しない。

再取得、再送、reorg で取り消された手番の replay は別に数える。添付分析で述べられた `reorg_replay_moves` の値 `64` を「取得失敗の64回 retry」として扱わない。replay 対象の手番、再認証、再包含、復元する予約と、再利用可能な資料を当該版の規則から導出する。本改訂は replay 回数や新しい timeout の数値を割り当てない。

### 3.4 EXEC の 1 BPS 目標と court の共通資源

EXEC の production／credit schedule と court が同じ CPU・I/O・帯域・包含枠を使う場合、EXEC、通常検証、公開 court、BASE／Heartbeat の claim・court・anchor・schedule 処理、必要な復旧の需要を合算する。queue を分けるだけで物理容量が増えたとは扱わず、共通の予約台帳と上限を使う。EXEC の未確定な credit や目標 1 BPS から将来の court 供給を計上しない。

予約済みの公開異議を守る queue 分離・優先順位・backpressure を設け、未予約の EXEC／新規 claim の受付・発行を抑制できるようにする。EXEC が停止する場合や credit が不足する場合にも、BASE／Heartbeat が claim、court、anchor、schedule を運ぶ経路とその必要資源を維持する。仕事の信用付与を止めたことで、その仕事を検証・否定する経路まで停止する循環を作らない。EXEC 二層設計自体の所有先は [07](0007-claims-exec-and-answer-accounting.md)・[10](0010-dag-consensus-and-native-settlement.md) とする。

### 3.5 予約不供給と未解決責任

予約は帳簿上の残量だけでは足りず、指定時刻に資料・処理・包含を供給する仕組みと観測可能な機会を伴う必要がある。実供給が失敗した場合、DAA の経過だけから当事者の無応答 slash や成功 Final を作らない。元の時計内で許された回復を行い、条件を完成できなければ未検証終了とする。残る訴追・保持・依存・資金責任は未解決のまま記録し、hard 到達を一括返還・証拠削除・全責任解除の根拠にしない。

供給前提が崩れた場合の失敗処理と、供給前提下での期限内完了保証を分ける。「容量超過を拒否できる」ことだけから、対象実モデルを受理・裁定できるとは結論しない。

### 3.6 モデル未保有の選出者と正常claimの完了

[16](0016-miner-execution-commit-and-material-supply.md)の公開検証資料は、重み・構造・program、promptとcanonical token列・tokenizer、数値規則、初期KV等の依存・RNG、Minerの初回traceとopeningを含む。選出資格を当該modelの事前保有、Minerの推薦、既存cache、資料取得の成功者へ狭めない。選出者に必要な計算・通信能力と、モデルの事前保有を区別する。能力条件を別の選出poolへ変更する場合は、[20](0020-verification-gated-final-and-adversarial-models.md)のpolicy変更とし、そのpoolと各選出結果にも一人前提が必要である。

受理計画は、coldの選出者が最後の必要chunkを取得し、全必須関係を検査し、比較・局所化・証拠作成・包含・裁定へ進める範囲を含む。cache hitを保証できない対象はcoldとして予約する。同じrootの認証済みchunkを独立provider、cache、他の検証者から再利用できるが、モデル所有者の追加許可・席・署名を再配布や検証の条件にしない。streamingはpeak RAMを減らせても、必要な総bytes・再実行・trace処理を省略しない。

正直な人が誰かはnativeには分からない。抽選位置の上限から各正規化bond／principalへの有限救済枠を予約し、復元抽選の重複位置や別relayで増幅しない。全参加者が合法な最大限の要求・最遅応答を行っても、一人の全体検証と反証の枠を他者へ移さない。反証caseだけでなく、正常claimに対する虚偽の計算case・適格資料要求を、正規terminal・棄却・客観的defaultで終結する資源も予約する。

正常claimの完了前提には、正しいMinerの供給・応答、少なくとも一人の正しい全体検証、実際のサービス・包含と復旧能力が含まれる。最後の適格noticeから全合法手番・裁定・回復が元hardまでに収まり、到着後のInspectionPendingを正直な要求者のdefaultにしないことを証明する。最後の資料を遅く出して検査時間を奪う計画を受理しない。供給失敗時の安全なAbortを、前提下の正常完了の代用にしない。

### 3.7 受理前に封印するatomic batch

抽選・モデル取得・ロード・署名搬送の固定費を共有する初期batchは、[20](0020-verification-gated-final-and-adversarial-models.md)の**atomic batch**だけを対象とする。同じMiner・model・profileの有限execution leaf群を、入力・出力・trace・仕事ID・canonical順序・leaf数とともに受理前に封印し、一つの新originalとして受理する。全leafを固定した後の未知seedでPanelを選ぶ。既に受理済みの別originalを吸収しない。抽選後のleaf追加・差替え・既知Panelへの新claim継ぎ足しは禁止する。versioned envelope、各leafのmembership、選出subject、累積予約、決済の接続が未実装・未確定なら、batch経路を有効化しない。

各選出者の検証義務は、batchの全leafと各leafの全実行関係を覆う。少なくとも一人が全対象を期限内に検証できなければならない。対象を各Panelへ分担し、正直者が検査しないleafを残す方式を、この一人保証へ含めない。署名集約はscopeを小さくする許可ではなく、全leafの結果・trace・承認意味への結び付きを検査する。

original、受理原点、notice／hard、責任、Final、成功creditはbatchに一つである。全leafの検証承認と全Qの解消が必要であり、一leafの適格な計算反証はbatch全体をRefutedにする。部分成功・部分creditは発行しない。供給・検証未完了もbatch全体の成功を止める。これは初期方式の失敗範囲であり、大きくまとめるほど一つの問題で失う未確定仕事も増える。execution IDを別batchへ再送・分割して二重creditを得る経路を拒否する。

CaseIdはoriginal内でも申立人・争点を含めて独立に保持し、別leafのcase棄却や別人の完了でQを消さない。共有MaterialIdによるbyte再利用と個別の取得・検査・反証権を分離する。全体がRefutedになった後も、別の保持義務・費用・未解決責任を一括消去しない。原時計を再発行せず、予約の利用・解放は当該originalの正規terminalと残責任に従う。

batchの最大leaf数・総再実行仕事・総資料量・保持・総価値・最大case数を別々に上限化する。B件をまとめてもB件の推論検証は残る。旧抽選位置枠をbatch数だけに読み替えて、仕事量・供給量・価値・累積抽選リスクの上限を迂回してはならない。全leafのcold取得と全合法caseを合算し、相互排他的でない分岐へ予算を二重に割り当てない。

### 3.8 登録モデル数と継続活動量を分ける容量policy

旧policyの4,096抽選位置／日と75位置／claimでは、上限は `floor(4096/75) = 54 claim/日` である。これは新しい公開profileの処理量を測った値ではない。新profileは次の資源vectorと各期限区間で受付を判定し、旧上限の暗黙継承も、無制限への暗黙解除も行わない。

| 資源 | 受理前に固定する上限・測定対象 |
| --- | --- |
| 選出・認証 | draw位置、正規化参加主体、署名・membership・状態更新のwork |
| Panel全体検証 | profile／shape／長さ別の再実行・全trace比較・hash・証拠構築 |
| 配信 | coldの重み・入力・依存・trace・openingと再送を含むbytes、providerと共用リンクの帯域 |
| 保持 | model／trace／局所証拠／未完成資料の同時占有とbyte期間 |
| Node局所裁定 | 全合法caseのCPU、parse、hash、heap、I/O、proof wrapperを含むbytes |
| 包含・復旧 | canonical制御message、各手番、EXECとの合算、restart／reorg／IBDの再処理 |

需要の参考式は `V = Σ λ_i r_i C_i`（検証計算量／秒）、`D ≈ Σ λ_i r_i h_i S_i`（モデル資料の新規取得量／秒）とする。λはclaim到着率、rは再実行を担当する異なる主体数、Cは1claimの検証費用、hはcache miss比、Sは必要model bytesである。入力・trace・opening・再取得・係争は別に加算する。抽選位置数は異なる主体数と同じとは限らず、肯定閾値1からr=1は導けない。

50,000モデル×1claim／日なら平均約0.579 claim／秒、同じモデル数×1claim／時なら約13.89 claim／秒である。この算術は活動量の例であり、採択処理能力・実測・SLAではない。モデル数、到着分布・burst、仕事長、同時claim、cold比、参加主体、最大合法係争を同時に指定して測定する。対応済みkernelの組合せの再利用はモデルごとの新checker作成を減らせるが、model registry・bond・期限索引・状態復元の費用は[12](0012-authenticated-state-storage-and-ibd.md)で別に検査する。

Panelと公開配信の供給を増やしても、共通のcourtを全Nodeが実行する場合の一Node当たり上限は増えない。Node台数やqueue数をcourt処理能力の倍率にしない。新しい受付policyの人数・資源数値・最大価値は、[パラメーター台帳](../governance/parameter-register.md)に根拠と測定・有効化状態を記録する。数値が未確定のprofileを稼働可能としない。

## 4. 計測結果の扱い

モデル、context、batch、profile、kernel、機材、commit、build条件を記録する。平均値・小さいfixture・推定値を、最大ケースや全モデルの保証に読み替えない。未測定は未測定として残す。

### 4.1 記録が必要な測定条件

| 区分 | 必須の記録と判定範囲 |
| --- | --- |
| 対象と実装 | artifact／program／規則の版、commit、build、実際の prefill・decode・context・batch、演算と witness の最大条件。class の最大 context 名を実 job の実行長に読み替えない |
| 環境 | 最低要件の CPU・RAM・ディスク・帯域、Node／worker 数と役割、cache の cold／warm、owner の結託・停止、同時 claim と要求分布、EXEC と court の競合負荷 |
| 計算と資料量 | 決定論的 work、実 CPU 時間、署名・hash・parse、資料と proof wrapper の bytes、重複排除前後の量、I/O、peak メモリ、保持容量と期間を分ける |
| 経過時間 | original 受付、資料の最後の取得、署名、notice、各応答、最終収集、canonical 包含、裁定までの wall clock と DAA 座標を記録する。局所 checker の時間だけで全経路を代用しない |
| DAA 換算 | 観測した DAA 進行と秒の対応、対象窓・進行率の範囲・採用した保守的境界・再編条件を記録する。`1 DAA = 1秒 = 1 EXEC` を仮定しない。秒の費用を DAA に換算する場合、締切が速く消費される側の境界も必要である |
| 統計と未完了 | 投入総数、受理数、chain 収録確認数、裁定完了数、失敗・timeout・観測終了時未完了数を同じ分母で示す。分位点の対象母集団・観測窓・未完了の扱いを明示し、完了例だけの中央値を全件の上限にしない |
| 供給と復旧 | 予約量に対する実処理・実包含、各手番の提出機会、backpressure、release 時刻の遅延、reorg・restart・pruned IBD 後の再開と残責任を確認する |

総仕事量の算術、仮定した最低帯域による換算、測定された wall clock、実際に保証できる service は別欄にする。測定された分位点だけで決定論的な全合法経路の上限を確定せず、profile の資源上限・予約機構・測定の適用範囲を合わせて判定する。

### 4.2 添付分析と既存記録の位置付け

今回の添付分析は、owner 待機だけでなく、局所化の直列取得・重複搬送・proof 上限・保持期限のずれ・未完了の搬送が別々の制約になり得る、という改訂理由として扱う。そこに示された過去の計算値・実測値・条件付き換算は、今回再測定した証拠、現在の実装状態、全モデルの保証値として使用しない。過去の原票と矛盾する判定ラベルがあれば、矛盾を報告し、有利なラベルだけを保証へ採用しない。

debug の短い経路、局所 court の単体 PASS、RPC への投入成功、一経路だけの短い DAA 値をもって、DAA 予算膨張が解消済みとは記録しない。本書の改訂では性能測定を実行していない。

## 5. 受入確認

最適化前後の計算一致、メモリ上限、streaming途中の失敗、容量不足、spill破損、反復実行での残留、検証時の追加peakを確認する。長文脈を処理できることと、そのclaimを期限内に裁定できることは別に評価する。

**受付の安全性 gate** は、全合法継続を収容できない需要・期限・署名契約を、受理前に拒否できること、受理後の供給失敗を成功や不当な slash に変換しないことである。**有用性・完了 gate** は、目的とする実モデル・実 job・文脈長を実際に受理でき、明示した供給・配送・復旧前提と最低要件の環境の下で、cold outsider の資料取得から適正な局所裁定までを原期限内に完了できることである。両 gate の結果・未完了条件を別に記録する。全件拒否による安全性の PASS を、有用性の PASS に使わない。

| ID | 受入ケース | 必要な結果 |
| --- | --- | --- |
| C05-01 | owner の全席を結託者が占有し、outsider は資料を持たない | owner の席の解放待ちを必須にせず、公開の取得・反証経路または規定の不供給処理へ進める |
| C05-02 | worker を増やし critical path は短くなるが、共通帯域または署名 CPU の総需要が超過 | 総需要・実供給・schedule の検査で拒否。並列数で不足を隠さない |
| C05-03 | critical path と全期間総量は収まるが、資料 release の遅延により途中の手番に割当不能 | 期限ごとの実行可能 schedule がないため受理しない |
| C05-04 | 複数攻撃者が異なる合法資料を要求し、複数 claim が同時係争 | 同時成立する全分岐と全 claim の累積予約を維持する |
| C05-05 | 同じ blob を別 claim・座標・proof で再利用 | bytes の認証済み再利用と各 binding の検査費用を区別し、別対象の認証を省かない |
| C05-06 | 最遅 notice、最遅応答、最後の model data、署名期限、reorg replay が重なる | 元 a からの予約と契約を照合。原時計を更新せず、越える計画を受理しない |
| C05-07 | 超過 profile は拒否できるが目的の実モデルは未受理 | 安全性と有用性の結果を分け、問題解消済みとしない |
| C05-08 | EXEC 目標負荷と court が共通 CPU・I/O・包含を使い、途中で credit が不足 | 合算予約と backpressure を適用。BASE／Heartbeat の救済・anchor・schedule 経路を維持する |
| C05-09 | RPC 受理の一部だけが chain で確認され、観測終了時に未完了が残る | 分母と未完了を報告し、完了例だけから期限内完了率や上限を確定しない |
| C05-10 | 唯一の正直な選出者が初めて扱うmodelでcacheを持たない | cold取得から全体検証・承認または反証まで、同じbinary・policy・元期限内で完了する |
| C05-11 | 正常claimに対して他の全Panelが不署名・合法な最大虚偽case・最遅応答を選ぶ | 一人の全体承認を残し、全caseを客観的に終結して正常Finalへ進む。正常系の全件Abortを合格としない |
| C05-12 | 最後のchunkとnoticeが最遅になり、検査時間が足りない | 計画を受理しないか、供給前提破れとして成功不可。InspectionPendingを早期終了しない |
| C05-13 | batch固定後のleaf追加、既知Panelへの継ぎ足し、部分leafだけの検証 | subject／全体coverage不一致として拒否。元時計・署名scopeを維持 |
| C05-14 | atomic batchの一leaf反証、資料不足、部分credit要求、既受理originalの吸収 | 一leaf反証でbatch全体Refuted、資料不足では全体成功不可。部分credit・既受理original吸収・時計再発行を拒否 |
| C05-15 | 承認閾値1への変更を理由に、75位置を1人分の取得・裁定予約へ縮小する | 実際の選出主体・全合法分岐に対する不足として拒否 |
| C05-16 | 50,000登録・1claim／日と1claim／時、burst、cold取得が混在 | 各活動量を別負荷として測定し、受理・完了・未完了・費用・最大資源を同じ分母で記録 |
| C05-17 | Nodeを増やすだけで共通courtの供給を増やした見積 | 一Node当たりのworst-case検査と合算予約で拒否 |
| C05-18 | cache再配布者へ所有者の追加許可を要求する | 取得・再配布・検証の公開経路を満たさず、新profileの完了条件にしない |

これらは設計上の完成条件であり、測定・実装・activation の完了記録ではない。資源数値と供給前提が未確認の profile は、有用性・完了 gate を未完了として残す。

---

## 原条文の統合収録

以下は内容を落とさないための原条文である。見出しの深さとリンク先だけを編集し、数式・コード・値・記録を再解釈していない。旧い方針・候補・試験記録も含むため、本文の責務・適用範囲と15の台帳を先に読む。原文内の相互参照は保存した旧ファイルへ戻る。新しい編集先は本書先頭の20本の索引・旧新対応表で確認する。

### 収録箇所

| 原文 | 行 | 入口 |
| --- | --- | --- |
| 旧0020 | 1–187 | [RFC-0020 — 長文脈の較正・変換・忠実度計測のメモリ: 32K の依存構造を保ち、同時に持たなくてよいものを持たない](#source-v1-0020-L00001) |

---

<a id="source-v1-0020-L00001"></a>

### 旧RFC-0020 / 原文1–187行

出典: [0020-long-context-calibration-memory.md](../rfc/0020-long-context-calibration-memory.md)。配置理由: 主題別統合。

### RFC-0020 — 長文脈の較正・変換・忠実度計測のメモリ: 32K の依存構造を保ち、同時に持たなくてよいものを持たない

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **設計前提(ADR・RFC より上位)— [MISAKAの不可侵原則](../PRINCIPLES.md):** 確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。本RFCはこの前提の下にあり、衝突する場合は前提が優先する。
> **暗号学的な計算証明を使わない — [PRINCIPLES §8](../PRINCIPLES.md)(恒久):** 本RFCは合意の外側の道具(較正・変換・忠実度計測)のメモリ設計であり、計算の正しさの証明を何も導入しない。使うハッシュは成果物と統計の同一性(digest)だけである。
> **genesis 前提 — 2026-10-15/16 ユーザー決定:** genesis は 8 モデル(#1 2 3 4 5 6 8 10)、**テキスト生成のみ**、**全モデルを 32K(32,768 token)の文脈で提供する**(2026-10-16)。再帰(Gated DeltaNet)クラスは提供文脈と同じ長さの系列 1 本で較正する(freeze-v1 §5.2、変換器が強制)。

* **Status:** Draft — 2026-10-16。設計(§1–§8)。実装(**implemented, unverified** — 本 lane は cargo build/test を実行していない。どの項目も PASS ではない): RAM 計上モード(§2)、carry の二重保持の除去(§3)、lazy 重みの既定値と警告(§4)、全系列スコア経路の監査(§5.1)、carry のディスク退避(§6.2、opt-in)、長文脈ドリフト検査(§7)。設計のみ・未実装: タイル化した厳密 attention と chunk-aware observer(§5.2–§5.4。監査の結果、genesis のテキスト経路には全行列経路が存在しない)、KV 履歴のディスク退避(§6.3)。
* **Decision:**
  1. **目標は 32K の依存構造を完全に保ったまま、同時に持つ必要のないデータを RAM に置かないこと。** 系列を分割しない。attention の範囲を縮めない。`Op::Attention` の `chunk` や小さな `max_window` をメモリ対策に流用しない(どちらも query が見てよい key を制限し、意味を変える)。
  2. **計測を先にする。** 段階(較正・変換・忠実度・推論)と種類(重み・carry・KV 履歴・GDN 状態・attention 一時領域・統計・logits)ごとに live と peak を出す RAM 計上モードを置き、OOM の原因を推測でなく帰属させる(§2)。
  3. **ビット同一性を崩さない。** 実装したメモリ削減(§3、§6.2)と速度改善(位置の run 評価、スレッド化カーネル)は、同じ入力に対して統計 digest・logits・GDN 状態がビット単位で一致することを試験で固定する。一致しない高速経路は明示的にラベルを付け、成果物の同一性を主張しない(§5.3)。
* **Activation:** **なし。** 合意・fence・wire・RPC には触れない。変換器(`palw-tir-convert`)と忠実度計測(`palw-tir-fidelity`)の CLI フラグと、genesis パイプライン(`scripts/genesis/genesis10_pipeline.py`)の引数だけである。
* **Baseline:** `integ/genesis10`(本RFCの commit)。
* **Not activated by:** 本RFCの merge、試験 source の存在、見積り表(§9)。
* **Normative language:** MUST / MUST NOT は本RFCの道具が守る不変条件である。

#### ユーザー方針 2026-10-16(本RFCを拘束する)

1. 32K の依存構造は完全に保つ。不要なデータを同時に RAM に置かない。系列分割・attention 範囲の縮小はしない。
2. まず計測: 段階別・種類別に RAM を分け、計上モードで OOM を帰属させる。
3. carry の二重保持を除く(ユーザー提供パッチ `reduce_calibration_carries`)。期待される削減は 1 系列分の carry(例: 32K × 4096 × FP32 ≈ 512 MiB)。アロケータが解放済みメモリを保持し得るので、総 RSS については何も言わない。
4. lazy 重み(`--calib-lazy-mib` / `--calib-lazy-cache-mib`)は重みの常駐を制限するだけで、KV や attention には効かない。行範囲を読めるソースが必要で、事前量子化 param は対象外。
5. タイル化した厳密 attention は、全行列経路が実在する場所に限る。正規化は**見えるすべての key** に対して行い、絶対位置の因果マスク(key_pos ≤ query_pos)、絶対 RoPE 位置、GQA の対応を保つ。**key chunk ごとに softmax して足し合わせてはならない。**
6. 統計の互換: 参照と同一の多パス経路(a)を先に、オンライン softmax の高速経路(b)は下位ビットが変わり得るのでラベル付けし成果物同一性を主張しない。chunk を意識した observer を置く。
7. 32K を超える文脈(64K/128K)は KV と carry をディスクへ予算付きで退避する(RAM とディスク I/O の交換。計算量は全 attention で O(N²) のまま)。
8. 実装順: (1) 計測、(2) carry パッチと lazy 重み、(3) 全行列経路が見つかった場合のみタイル化 attention、(4) より長い文脈のための KV/carry 退避。

#### 0. 要約

| 対象 | 32K で何が RAM を占めるか | 本RFCの扱い |
| --- | --- | --- |
| 重み | 実行中の occurrence 1 つ分(層、または embedding / head)。#6 の f32 embedding/head が最大 5.1 GB | 既定のまま(layer-major)。lazy 重みは MoE の expert stack 向けで、密な模型では既定 off(§4) |
| carry | 系列全体の隠れ状態 1 本(#2 256 MiB、#3 512 MiB、#6 640 MiB)。**旧実装は入力と出力の 2 本を同時に持っていた** | 消費した入力行をその場で退役させ 1 本に(§3)。超長文脈はディスク退避(§6.2) |
| KV 履歴 | attention 層 1 つ分の session の key/value 行(#2 128 MiB、#3/#6 256 MiB) | そのまま。layer-major では同時に 1 層分だけ(§6.3) |
| GDN 状態 | 層あたり数 MiB | そのまま |
| attention 一時領域 | **1 query 位置 × 全 head のスコア行**(#6 で 24 × 32,768 × 8 B ≈ 6 MiB) | 全行列は存在しない(§5.1)。タイル化は全行列経路が実在する所だけ(§5.2) |
| 統計 | site 数 × channel 数。数十 MiB 以下 | そのまま |
| logits | 較正では作った端から捨てる。忠実度計測の `--exec` 経路は 1 位置ずつ比較 | `--exec` なしで 32K を評価すると両側の logits が ~97 GB になるため拒否(§7) |

#### 1. 範囲

対象は genesis のテキストデコーダ(token id → 次 token の logits)の 4 段階である。

| 段階 | 道具 | 長文脈で効くもの |
| --- | --- | --- |
| 較正 | `palw-tir-convert`(`fidelity::calibrate` → `run_layer_major`) | carry、KV 履歴、GDN 状態、attention 一時領域、統計 |
| 変換 | `palw-tir-convert`(`materialise_stream` → PALWART3) | 重み(テンソル単位)。文脈長には依存しない |
| 忠実度 | `palw-tir-fidelity --exec`(`run_to_post` + 型付き整数実行器) | carry(post 入力)、整数 KV、logits(1 位置ずつ) |
| 推論 | `misaka-palw-tir-exec`(1 token ずつの step) | 整数 KV |

合意(court、Gate C、re-execution)は 1 位置の step を標本検証するので、本RFCの対象外である。

#### 2. 計測を先にする — RAM 計上モード(実装済み・未検証)

`misaka/palw/tir/lower/src/ram_account.rs`。`--ram-accounting FILE`(`palw-tir-convert`、`palw-tir-fidelity`)、パイプラインでは `run --ram-accounting`(`<model dir>/ram-<step>.jsonl`)。

* **種類:** `weights`(実行中 occurrence の常駐 param と lazy cache)、`carries`(RAM 上の carry)、`spilled_carries_on_disk`(退避分、RAM ではない)、`kv_history`、`recurrent_state`、`attention_temporaries`(1 回の attention 呼び出しの最大一時領域)、`stats`、`logits`。
* **段階:** `open`、`calibration`、`conversion`、`materialise`、`fidelity.float`、`fidelity.int`。段階に入るたびに peak を live 値から数え直す。
* **出力:** 1 行 1 JSON。occurrence(層)ごと、整数実行 1,024 位置ごと、段階の境で snapshot(live・peak・RAM の live 合計・OS の RSS)。最後に段階ごとの peak の要約行。
* **注意:** live は評価器のデータ構造を長さで数えた値(`f32` 行 × 4 B)であり、アロケータの見方ではない。アロケータは解放済みメモリを保持し得るので、RSS は live 合計より大きいままになり得る。両方を記録するのはそのためである。無効時は相対 atomic の読み 1 回だけで、結果は変わらない。

#### 3. carry の二重保持を除く(実装済み・未検証)

旧実装の `run_layer_major` / `run_to_post` は、occurrence ごとに入力の carry 系列(`carries`)を読みながら出力系列(`outs`)を別に作り、occurrence の終わりで差し替えていた。その間、系列 2 本分を同時に持つ。

**変更(ユーザー提供パッチを、位置の run 評価に合わせて適用):** 入力行は読んだ時点で取り出し(RAM 上の行は空にして解放)、出力は次の occurrence の carry として書く。post block では入力行を退役させ、求められた logits だけを残す。session・絶対位置・評価順・site の観測順・`SiteStat` の merge 順は変わらない。

* **期待される削減:** 系列 1 本分の carry。32K × hidden × 4 B = #2 256 MiB、#3 512 MiB、#6 640 MiB(#3 はユーザー分析の例 ≈512 MiB に一致)。run の長さ分(既定 32 位置)の出力は一時的に重なる。
* **総 RSS については何も言わない**(アロケータ)。§2 の計上で live の減少を確認する。
* **ビット同一性:** `misaka/palw/tir/lower/tests/position_blocks.rs::consumed_and_spilled_carries_are_the_two_sequence_run_bit_for_bit` が、旧アルゴリズム(2 系列保持・1 位置ずつ・系列順 merge)をそのまま写した参照と、統計の digest(`stats_digest_hex`)・正準 JSON・全 logits のビットを 7 つのアーキテクチャの toy checkpoint で比較する。

#### 4. lazy 重み(既存機能の既定値と案内)

`--calib-lazy-mib N` は、`N` MiB 以上の param を occurrence と一緒に常駐させず、演算が求めた時に読む(expert stack は router が選んだ expert の行だけ)。`--calib-lazy-cache-mib` は読んだテンソルを保持する予算である。

* **効くもの:** 重みの常駐だけ。KV 履歴・carry・attention には効かない。
* **前提:** 行範囲を読めるソース(safetensors の `Checkpoint`、遅延 shard、リモート GGUF)。事前量子化 param は対象外(整数をまとめて読む)。
* **罠(本 lane のパイプラインで実際に既定にしていた):** `--calib-lazy-mib 64` と 512 MiB の cache では、cache に収まらない**密な**テンソル(MLP 行列、embedding、head)を**位置ごとに丸ごと読み直す**(`ParamStore::view`)。#6 のストリーム GGUF では位置 × 層ごとに約 1 GB の再ダウンロードになる。
* **既定:** off(`palw-tir-convert` は元から 0、パイプラインも 0 に修正済み)。layer-major はもともと 1 occurrence 分しか常駐させず、genesis 全モデルで 32 GB に収まる。
* **案内:** 使うのは MoE の expert stack を常駐させたくない時だけ。`--calib-lazy-mib` を密な行列より大きくして expert stack だけを lazy にするか、`--calib-lazy-cache-mib` を層の lazy な密テンソルの合計より大きくする。
* **警告(実装済み):** cache に収まらないテンソルを丸ごと読み直した最初の 1 回に、プロセスごとに一度だけ警告を出す。

#### 5. attention のメモリ

##### 5.1 監査: 全系列のスコア行列はどこにあるか(実施済み)

| 経路 | attention の形 | 32K での一時領域 |
| --- | --- | --- |
| 較正・忠実度の float 参照(`float_ref::attention`) | 1 query 位置 × その位置までの全履歴、head ごと(2 パス softmax) | #6: 24 head × 32,768 × (dot・score・prob の f64 + 観測用 f32)≈ 25 MiB |
| 位置の run 評価(`Session::eval_occurrence_run`) | 同上。run 内の各位置は**絶対位置で見える行だけ**を読む(§5.5) | 同上 |
| 整数の参照評価器・型付き実行器(`tir`、`misaka-palw-tir-exec`) | 1 token ずつの step。スコアは履歴長の行 | 同程度 |
| court・re-execution | 1 位置の step | 同程度 |
| A16 / PALWQ361 エンジン(base0) | 1 token ずつ | 同程度 |
| 全行列(N × N)を作る経路 | 双方向エンコーダ(`lower/bidir.rs`)、vision(`lower/vision.rs`)、diffusion(`diffusion/attn.rs`) | **genesis の範囲外**(テキスト生成のみ) |
| 全系列の **logits**(スコアではない) | `palw-tir-fidelity` を `--exec` なしで使う時、両側の logits を全位置分保持 | 32K × 248,320 × 12 B ≈ 97 GB → **8 GiB 超は拒否するよう変更**(§7) |

結論: genesis のテキスト経路(較正・変換・忠実度 `--exec`・実行器・court)に 32K × 32K のスコア行列は存在しない。§5.2–§5.4 のタイル化は**実装しない**(ユーザー方針 5: 全行列経路が実在する所だけ)。全行列経路(双方向・vision・diffusion)を将来 genesis に入れる場合の仕様として残す。

##### 5.2 タイル化した厳密 attention(仕様、未実装)

全行列の大きさは `B × H × N² × bytes`。例: 24 head × 32,768² × 4 B = **96 GiB**。

| 方式 | 一時領域(24 head、FP32) |
| --- | --- |
| 全行列 | 96 GiB |
| query だけタイル化(Q = 128) | 24 × 128 × 32,768 × 4 B = **384 MiB** |
| query と key をタイル化(Q = 128、K = 1,024) | 24 × 128 × 1,024 × 4 B = **12 MiB** |

不変条件(MUST):

1. softmax の正規化は、その query から**見えるすべての key** に対して 1 回だけ行う。**key chunk ごとに softmax して結果を足し合わせてはならない**(確率の和が 1 にならず、意味が変わる)。
2. 因果マスクは絶対位置で判定する(`key_pos ≤ query_pos`)。タイル内の相対位置で判定しない。sliding window・sink も絶対位置で同じ規則を適用する。
3. RoPE は絶対位置で回す。タイルの先頭を 0 にしない。
4. GQA の対応(query head `h` → kv head `h / (heads / kv_heads)`)をタイル化で変えない。
5. 系列を分割しない。前のタイルの key を落とさない。

##### 5.3 統計との互換

較正は attention の `scores` と `probs` の sub-site を観測し、`calib.rs` は IEEE のビットパターンとその digest を保存する。よって 2 つの経路を分ける。

* **(a) 参照と同一の経路(先に実装する):** 多パス。(1) 見える全 key のスコアの最大値、(2) 分母(参照と同じ加算順)、(3) 確率・出力・統計。浮動小数の演算順を参照と揃え、統計 digest・logits が参照とビット一致する。
* **(b) 高速経路:** オンライン softmax(走りながら最大値と分母を更新)と、統計用の別パス。下位ビットが変わり得る。**ラベルを付け、成果物の同一性(artifact root、統計 digest)を主張してはならない。**

##### 5.4 chunk を意識した observer(仕様、未実装)

`SiteStat::observe()` はベクトルの長さから ragged か channel 配置かを推定する。chunk ごとに観測すると、1 行が長さの違う断片として届き、channel の扱いと位置 0 の判定が崩れる。chunk を意識した observer は (論理行の長さ、chunk の offset、絶対位置) を受け取り、1 行を連結して観測した場合と同じ `absmax`・`pos0_absmax`・`rest_absmax`・`chan_absmax`・`ragged` を作る。`sum_sq` は (a) の経路では参照と同じ順で加える。

##### 5.5 位置の run 評価(実装済み・未検証)は chunk attention ではない

32K 較正の速度のため、block の各 node を run 内の全位置について評価してから次の node へ進む(`Session::eval_occurrence_run`、既定 32 位置、`PALW_FLOAT_POSITION_BLOCK` で変更、1 で従来どおり)。行列は run ごとに 1 回読まれる。attention は run 内でも**1 query 位置ずつ**、その位置までに位置ごとの実行が append したはずの行(`key_pos ≤ query_pos`)だけを読む。系列分割・範囲縮小・`chunk`/`max_window` の流用は一切ない。admit する block は、履歴が append 1 つと attention の読みだけ、固定状態が 1 node だけ(GDN、causal conv1d)のものに限り、それ以外は位置ごとに戻る。統計・logits がビット一致することを `tests/position_blocks.rs` が 7 アーキテクチャで固定する。

#### 6. 32K を超える文脈(64K / 128K)

##### 6.1 線形に増えるもの

| | 32K | 64K | 128K |
| --- | --- | --- | --- |
| carry(#6、5,120 × 4 B) | 640 MiB | 1.25 GiB | 2.5 GiB |
| KV 履歴(#6、attention 層 1 つ、key+value 2,048 × 4 B) | 256 MiB | 512 MiB | 1 GiB |
| attention 一時領域(#6、1 query 位置) | ~25 MiB | ~50 MiB | ~100 MiB |
| 全 attention の演算量 | N² に比例 | ×4 | ×16 |

##### 6.2 carry のディスク退避(実装済み・未検証、opt-in)

`--spill-carries-dir DIR [--spill-carries-budget-mib N]`(両道具、パイプラインも同名)。ある occurrence の出力が系列全体で予算を超える見込み(最初の行のバイト × 位置数 > 予算、0 なら常に)なら、出力をファイルへ位置順に書き、次の occurrence は run ごとに読み戻す。行は同じ `f32` のビット(リトルエンディアン)なので、in-RAM の実行とビット一致する(`tests/position_blocks.rs` が予算 0 で検証)。RAM をディスク I/O と交換する。32K では不要(§9)。

##### 6.3 KV 履歴の退避(仕様、未実装)

手順: 層ごとに重みを読む → 隠れ状態をブロック単位でディスクから読む → GDN 状態・conv 履歴は RAM に保つ → attention は KV をブロック単位で読む → 統計を更新 → 出力をディスクへ書く → バッファを解放。演算量は全 attention で O(N²) のまま。

未実装の理由: layer-major では同時に存在する KV 履歴は attention 層 1 つ・系列 1 本分で、genesis では 128K でも #6 で 1 GiB に収まる。さらに 1 query 位置ずつ KV をディスクから読むと I/O が O(N²) になるため、KV 退避は query ブロックでまとめて読む構造(§5.2 の query タイル化)を前提とする。必要になった時点で §5.2(a) と同時に実装する。

#### 7. 検証

* **ビット同一性(試験 source、未実行):** `misaka/palw/tir/lower/tests/position_blocks.rs`(run 評価、消費型 carry、退避 carry を旧アルゴリズムと統計 digest・正準 JSON・全 logits で比較。7 アーキテクチャ)、`misaka/palw/tir/lower/src/float_ref/tests.rs`(スレッド化した `linear_raw` / `linear_raw_many` / `attention` / `gated_delta_traced` を旧カーネルの写しとビット比較。GDN 状態を含む)。
* **長文脈ドリフト検査(実装済み・未検証):** `palw-tir-fidelity --exec --drift-windows N` は、評価の全位置を作った端から比較し(logits 行を保持しない)、評価を N 等分した窓ごとの平均 KL と最大 KL を出す。パイプラインの `fit` は短い held-out(2 × 64 token)に加えて、較正に使った最初の 32K token の**次の** 32K token(RFC-0002、held-out)で 32 窓を測り、平均 KL ≤ 0.1、top-1 ≥ 0.8、各窓の平均 KL ≤ 0.1、後半/前半 ≤ 1.5(後半の KL ≤ 0.01 なら免除)を満たさなければ index を upload しない。
* **未計測:** 8K / 16K / 32K の peak RAM と時間。§2 の計上モードで測る(§9 のコマンド)。

#### 8. 実装順と状態

| 順 | 内容 | 状態 |
| --- | --- | --- |
| 1 | 計測(RAM 計上モード) | 実装済み・未検証 |
| 2 | carry パッチ、lazy 重みの既定値と警告 | 実装済み・未検証 |
| 3 | タイル化した統計対応 attention((a) を先) | **不要と判断**(§5.1: genesis の経路に全行列なし)。仕様のみ |
| 4 | KV/carry のディスク退避 | carry は実装済み・未検証(opt-in)。KV は仕様のみ(§6.3) |

#### 9. 32K の見積り(未計測)

前提: M1 Max(P 8 + E 2、32 GB)。旧カーネルの実測(9B 較正 8,192 位置 18,360 s ≈ 3.5 GMAC/s、#2 の 7 層 × 8,192 位置 ≈ 1,600 s ≈ 2.1 GMAC/s)を基準に、スレッド化カーネルと run 評価で線形部 12–22 GMAC/s、attention 12–18 GMAC/s と置いた。誤差は ±50% 程度。

| | 線形 MAC/位置 | attention MAC(32K 全体) | 較正 32K | 変換 | 長文脈 fit | peak RAM(較正) |
| --- | --- | --- | --- | --- | --- | --- |
| #2(2B、GDN 18 + attention 6) | 1.9 G | 13 T | 1.5–2 h | ~10 分 | float 1.5–2 h + 整数 ~0.5 h | 2.5–3.5 GB(fit 時 5–6 GB) |
| #3(9B、GDN 24 + attention 8) | 7.9 G | 35 T | 4.5–7 h | ~30 分 | float 4.5–7 h + 整数 ~1 h | 5–6 GB(fit 時 ~16 GB) |
| #6(27B、GDN 48 + attention 16、ストリーム GGUF) | 25.6 G | 106 T | 13–19 h | 1.5–3 h(HTTPS 再読込と upload 込み) | この Mac では不可(整数 program 29.5 GB、ソースはストリーム) | 8–11 GB |

**#6:** この Mac での合計は 15–22 時間の見込みで、24 時間をわずかに下回る程度で不確かである。最速で正しい経路は**較正専用マシン**: 64 コア級・64 GB 以上の機械に GGUF を置き、`palw-tir-fidelity <gguf> --calib calib-tokens.json --calibrate-only --stats-out calib-stats.json --gdn-core-wide`(run 評価で行列の再読込が減り、コア数に比例する。見込み 3–4 時間)、この Mac で `--calib-stats-in calib-stats.json` を付けて変換だけを行う(1.5–3 時間)。統計は別プラットフォームの libm の下位ビットを含み得るが、成果物は運ぶ統計に対して決定的であり、manifest が測定プラットフォームを記録する。同じ機械で長文脈 fit も測れる。

#### 10. 本RFCで決めないこと

* 32K 以外の提供文脈。
* §5.2 の (b) 高速経路の採否。
* §6.3 の KV 退避の実装時期(必要が計測で示された時)。
* 32K の court・re-execution の資源上限(合意側の別作業)。
