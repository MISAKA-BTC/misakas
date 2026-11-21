# RFC-0062 — PALW-FOW1：推論関数の採掘出力と新規仕事の適格性

<!-- QF1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — QF1のQwen3具体profile

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0066](0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](../adr/0234-qwen3-fow1-execution-and-readout.md)／[適用・移行](../governance/qwen-fow1-profile-transition.md)。本RFCの抽象ReadoutをQwen3-8B向けに具体化するprofileはQF1。全36blockの第二残差後とfinal norm後の各4096 i16を、次tokenの各予測位置で読む。

QF1は36層Qwen3-8Bの整数A16実行、各予測位置の37 vector、HEX64実入力、固定Readout/四Root/Ticketを定義する。G-COLL/G-COST/G-USE、MAS1のUNKNOWN・BFT安全停止、Panel＋Courtを維持する。Qwen実機の品質・費用検査・native有効化は未完であり、旧claimやMOE2経済を変更しない。
<!-- QF1-ADOPTED-20261120-END -->


<!-- MAS1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MAS1の構造・実用性検査とBFT安全停止

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0065](0065-model-admission-screening-and-bft-stop.md)／[ADR0233](../adr/0233-model-admission-screening-and-bft-stop.md)／[MAS1適用・移行](../governance/model-admission-safety-transition.md)を、この新規採掘profileの追加正本とする。採掘資格の入口にkernel/graph解析・実用性G-USE・UNKNOWN保留・BFT停止状態を追加する。未確認のG-COSTは引き続き採掘不可である。

構造検査はPASS/FAIL/UNKNOWNを区別し、UNKNOWNは採掘不可のままBFT reviewへ接続する。G-EXEC/G-COLL/G-COSTを維持し、公開・共通の実用性G-USEと停止状態をAND条件へ追加する。BFTはモデル/ruleの停止と条件付き解除に限り、未検査の費用経路の承認・台帳finality・fork choice・reorgの権限を持たない。burnやowner変更で安全停止を解除しない。

旧「モデル品質・モデル単位の検査を一切しない」という文言は、MAS1の採掘資格に関する今回の限定検査へ置換する。一般登録とMOE2市場原子生成、通常取引、coreの経済配分、Panel＋Court、計算validity proof不使用、旧claimの元責任は維持する。旧採択ブロック・原文・実測はその版の履歴であり、今回の数学的証明・native実装・公開activationが完成したという意味ではない。
<!-- MAS1-ADOPTED-20261120-END -->

- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**（要求・責務・移行方針の採択。適合する本番モデル、反復費用検査、native実装、公開有効化の完成ではない）
- 系列: **PALW-FOW1**（Function Output and Work admissibility。論理名でありwire IDは未割当）
- 対応ADR: [ADR-0230](../adr/0230-palw-function-output-and-work-admissibility.md)
- 乱数正本: [RFC-0061 PALW-TB1](0061-palw-internal-threshold-beacon.md)／[ADR-0229](../adr/0229-palw-internal-threshold-beacon.md)
- 継承元: [RFC-0059 AN1](0059-palw-an1-input-binding-and-panel-court.md)、[RFC-0060 MOR1](0060-model-registration-and-attempt-output-binding.md)
- 現行担当: [入出力01](../rfc-v2/0001-inference-and-generative-jobs.md)、[TIR02](../rfc-v2/0002-tensor-ir-and-shared-query-data.md)、[登録03](../rfc-v2/0003-model-onboarding-and-artifacts.md)、[会計07](../rfc-v2/0007-claims-exec-and-answer-accounting.md)、[元claim16](../rfc-v2/0016-miner-execution-commit-and-material-supply.md)、[Final20](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)

## 0. 今回採択する変更

ユーザーは、監査で指摘された五項目の修正と、添付TB1方式への乱数生成の変更を明示した。本RFCは「LLM関数自体から得る採掘値」を保証対象にする。MOR1の回答・試行ID・識別値を組にした非重複性を、その代わりにしない。

FOW1を有効化する新規採掘では、登録された推論関数の採掘出力の非衝突性、複数試行の新規評価費用、全実行関係の検証可能性を**全て採掘適格性の必須条件**とする。単にreceiptを付ける登録、入力が違うだけのAN1登録、通常のモデル名による登録は、この条件を満たさない。

非衝突性には§4.2の定理、費用には§4.3の反復検査を要求する。既存Qwen又は任意のLLMについて、その定理が完成したと宣言するものではない。適合する具体的な関数系列・定理・検査器がない間はFOW1の採掘を有効化しない。適格性が空である状態を「実用的LLM-PoWが完成した」と表示しない。

モデル本体を持たないNode、Panelの完全再実行、認証された局所反証、単独反証優先、資料未充足時の成功停止を維持する。Groth16等の実行validity proof、新しいBFT finality、HF取得可否の審査委員会は導入しない。後続MAS1では公開suiteに基づく実用性G-USEと限定BFT安全停止を追加するが、未確認の費用条件を多数決で承認しない。

## 1. 四つの値と二種類の一回性

| 値 | 規範上の意味 | 代替にしてはならないもの |
| --- | --- | --- |
| `s` | TB1のEXEC値と事前登録文脈に結合した試行ID | 推論関数の採掘出力 |
| `y` | 利用者に返すcanonical回答token列又は登録された有用出力 | 識別用のtagを付けただけの採掘値 |
| `z` | 登録された**同一のコア推論関数**が生成するcanonical採掘出力 | `s`、`H(s)`、公開seedのecho、自由nonce、receipt、署名 |
| `rho` | 元実行を運搬・識別する補助receipt | `z`の非衝突性、実行正当性、費用の証明 |

FOW1の規範実行は、固定されたmodel/profileについて

$$\operatorname{CoreExec}_{M,P}(x)=(y,z,\tau),\qquad z=\operatorname{Readout}_{P}(\text{規定された推論状態})$$

とする。`Readout`の演算・入力位置・生成順・停止・出力長を登録programとtraceのEnd関係へ含める。`z`を含む実行を閉じた後に作る`R_z`、`R_tau`、Ticket、`rho`を、そのReadout又は先行コア実行の入力へ戻さない。元の推論を無視して試行IDを返す処理を、名前を変えてコア推論の採掘値にしてはならない。計算graph上の経路が存在するだけでも、§4の定理の代替にはならない。

`z`のcanonical符号化は、採用するsecurity profileで固定した32 byte以上の固定長とする。十分な長さは必要な形式条件であり、entropyや非衝突性の証明ではない。自然回答`y`が短い又は同じ内容でもよい。非衝突性を要求する正式な推論出力は`(y,z)`であり、`z`はその推論の計算対象に含まれる。`y`だけを表示するAPIも、認証された`z`とprofileへの参照を機械向け出力から失わない。

同じ試行をMinerとPanelが再実行すると、同じ`y,z,tau`を得なければならない。異なる試行では`z`の衝突を§4.2の明示的上界へ抑える。別のnonce・owner・epochを付けるだけで同じ実行を新しい仕事にすることはできない。同一試行の再提出に対する追加報酬の拒否は、非衝突性ではなく§7の原子的消費で実施する。

## 2. 不変な登録と採掘適格性

### 2.1 登録と適格性を別の状態にする

一般の`ModelRegistered`と、FOW1で仕事・報酬・合意資格を発生できる`FOW1Admissible`を分ける。前者はモデルの不変identity・形式・資源等に従うpermissionlessな登録である。後者は、全候補に同じ数学的・構造的規則を適用した結果であり、モデルの名前、人気、裁量的な品質評判、保有者数、HFの可用性、登録者の裁量で決めない。MAS1対象では別途、公開・共通の実用性G-USEと安全停止状態を確認する。

型・条件名は論理的状態でありnative enumを割当済みではない。新規の有償採掘・credit・weightを許す箇所はすべて後者を確認する。既存登録にflagを立てるだけで、既存のModel IDやprogramの意味を変更しない。

### 2.2 FunctionWorkDescriptor

以下を単一の不変なdescriptorに結合する。

| 項目 | 固定する内容 |
| --- | --- |
| artifact | model root、全weights/config、program、tokenizer、adapter、数値意味の正確な版 |
| 入力 | AN1の実token規則、許容jobと入力分布、長さ、position/mask、初期state・cache由来 |
| コア関数 | kernel系列、全演算、生成・停止、`y`と`z`の役割、Readout・canonical符号化 |
| trace | Init/全Step/Readout/End、四Root、canonical木、全局所checkerと資源上限 |
| 数学的根拠 | TheoremRuleId、定理本文digest、証明・仮定、許容parameter、全適合predicate |
| 費用 | 計算モデル、計数単位、前処理・共有・外注の会計、比較経路、反復試行、基準実装と閾値 |
| 乱数・時間 | TB1 suite、EXEC/PANELのRoundRef型と選択規則、role epoch制約、TimingPlan規則ID、freeze・公開gate |
| consensus | 新Ticket domain、元claim署名、仕事キー、会計版、用途、移行・停止条件 |

未知の実beacon、`s`、`y,z`、実行Root、receipt、閉鎖集合Root、将来のPanel結果をdescriptor自身のdigestへ循環的に入れない。descriptorが認める規則に従った具体的なepoch・EXEC/PANEL RoundRef・原期限・TimingPlan instanceは試行文脈`C`又はそこから一意に認証されるnative recordへ固定し、epochが変わるだけで重みのModel IDを再定義しない。先に確定するRoundRefと後から確定するcertificate／FreezeRecordも別型にする。

### 2.3 G-COLLのTheoremRuleと決定論的適合

`TheoremRuleId`は、合意が有効とする具体的なモデル／kernel系列、数学的定理、前提の分類と適合検査器の不変recordを指す。任意の文書URL又は登録者の「証明済み」という署名を参照するIDではない。新しい定理系列の採択・rule更新は版を付けたプロトコル変更であり、実行ごとのSNARKを導入する手続きではない。

前提を二種類に分ける。hash/RO・PQ primitive、攻撃者の計算モデル、TB1の故障上限、正直Panel・包含・native安定性等は、rule採択時に明示する**外部安全性仮定**であり、Nodeが登録時に数学的真偽又は将来の成立を実測したとは扱わない。保証はこれらに条件付く。他方、個々の登録に関するmodel/program/kernel/weight・入力出力規則・parameter等の**適合前提**は全て認証付きで検査しなければならない。検査できないartifactの属性を外部仮定へ移して合格させない。

同じruleの適用範囲に入る候補は、モデル名のallowlistや個別の人手承認なしに提出できる。採掘適格化には次を全て必要とする。

1. 当該ruleが現在のfamily/security profileで有効である。
2. 実artifact identity、program、shape、kernel、数値規則、入力・出力規則が定理の対象に一致する。
3. 定理が各登録artifactに要求する**全て**の適合前提を、認証された値を用いる登録済みの有界な適合検査器で確認できる。
4. 第4節の三つのgateが満たされ、費用検査の範囲・累積予算、および衝突・暗号・検証に用いる失敗確率が指定される。
5. 必要なchecker、資源、TB1契約、claim consumer、移行が実装・検証され、当該familyの有効化条件が満たされる。

Nodeは全重みを取得せず、既存の認証されたregistration/program identityと有界な適合情報を扱う。しかし、Nodeが読まない全重みの性質を、未認証のheader、`.root`属性、graphの見た目、単独Panelの署名から推定してはならない。全重みや未知の意味論に関する仮定を持ち、その成立をこの方式で確認できないruleは**未適格**とする。未知の汎用proof checker、モデル取得又は新しい登録委員会で補うことを黙認しない。

このため、任意のQwen重みへchallenge prefixを追加しただけでFOW1Admissibleにはならない。具体的なruleに適合しない登録は、一般モデルとして存在できても、FOW1の新規採掘・credit・weightには使えない。非対応を旧AN1/MOR1へfallbackして同じ資格を与えない。

### 2.4 MAS1の解析・実用性・停止状態

[RFC0065](0065-model-admission-screening-and-bft-stop.md)のKernelAnalysisProfile、LoweringManifest、UsefulStateMap、PracticalSuite、WorkCostContract、AdmissionPolicy、AdmissionGenerationをdescriptorへ結合する。ModelRegisteredからFOW1Admissibleへの遷移には、全必須検査PASS、G-USE及び安全停止なしを加える。

UNKNOWNは採掘不可のREVIEW_HOLDとしてBFTへ接続し、FAILは拒否する。自動検査やBFT署名をG-COLL/G-COSTの証明としない。全重みの未認証属性は引き続き不適格であり、BFTはそれを認定する委員会ではない。評価実行は非報酬とし、採掘資格を前提にしない独立control経路で検査する。

## 3. TB1を使う入力・実行・公開の順序

本familyの新しい未来乱数源は[RFC-0061](0061-palw-internal-threshold-beacon.md)のPALW-TB1に固定する。完全BGW VSS、PQ認証私信、公開share certificateを用い、EXECとPANELは別秘密・別session・別gateとする。既知のEXEC値をhashし直してPANELの未知性を作らない。

`n=3f+1`、私的な秘密復元`t=f+1`、公開certificate`q=2f+1`を区別する。freezeはcertificate完成や公式発表ではなく、**最初の正直share放出前**にTB1のnative安定条件を満たす必要がある。正直Panelの存在からTB1のfault budgetを導かない。

1. 有効なFOW1 descriptor、全`C`/nonce候補、owner/beneficiary、target、原期限、EXEC/PANEL RoundRefを登録する。両roundのPREPAREDと必要予約を先に確認する。
2. TB1 EXEC用の登録集合をnativeにfreezeし、同じ集合・roundの公開gateが成立するまでshareを公開しない。
3. 正規EXEC certificateから得た`\sigma_E`を使い、`s=H_FOW1-attempt(Enc(C,\sigma_E))`を導出する。AN1の固定byte-token frameを実入力に用いる。QF1を明示したprofileに限り、RFC0066 §3のHEX64 chat frameへ置換する。
4. `CoreExec`から`y,z,tau`と四Root・唯一のTicketを生成し、署名済み元claimを固定する。
5. 元claim集合、global Panel pool、抽選規則をTB1 PANELへfreezeし、その別秘密の正規certificateから`\sigma_P`を得る。
6. 選出Panelが元claimを独立に完全再実行する。一致すれば通常の承認を提出する。不一致や資料不足があれば、元期限・元予約で次節の手続きを使う。

無効certificate、予算不足、公開拒否、深いreorgによる安定性前提の破れはTB1の失敗規則へ進める。同一attemptを別round、旧beacon、block hash又はローカル乱数へ移して再抽選しない。公開済みroundの曝露journalをreorgで未公開へ戻さない。

新しいADMISSION抽選をFOW1へ重ねない。AN1と同じく、適格候補の既定順位は固定Ticketとslot IDの決定論的順序を使う。別controlで受付順位用TB1-ADMISSIONを使う場合の規範は0061の用途表に従い、FOW1の採掘機会を増やさない。EXEC scheduleと将来role選出はそれぞれ独立用途であり、採掘Ticketの代替ではない。

生成用PRNGは`\sigma_E`を含む固定試行文脈から決定論的に導出し、draw順・sampling・丸め・停止を固定する。Court nonceは認証済みPANEL値と元claim・担当者・責任に束縛して派生させる。既受理negative、資料要求・応答、終端へ新しいTB1待機を追加しない。

<a id="fow1-panel-dialogue"></a>

### 3.1 Panelの非対話検証と紛争時の任意対話（2026-11-21 採択）

選出された各Panel検証者は、固定済みの入力・モデル・実行規則と必要な資料を取得し、独立に完全再実行する。元claimの回答、採掘値 `z`、正規traceを含む四Root、Ticketを照合した後に承認する。正常時はMinerへの追加質問や応答待ちを挟まず、所定の承認・異議期限・未解決caseの確認を経てFinalを判定する。credit・決済には所定のGate C認証も必要である。

- **不一致**　検証者は、元claimに結び付く局所反証を相手の協力なしに構成し、直接Courtへ提出できる。Nodeは有界な証拠を決定論的に裁定する。反証の構成中でも、元期限内の適格な不一致noticeを受理し、caseが未解決の間は成功を保留する。
- **資料不足**　既存の資料caseで取得・検査の機会を確保する。元期限までに検証を完了できなければ未検証で終了し、成功Finalとcreditを与えない。root差や資料不足だけで計算不正を認定しない。

**対話による不一致箇所の絞り込みは任意とする。** 提供する場合も対話を待たずに直接反証を受理し、追加質問で元期限・予約・責任を変更しない。任意の問い合わせに答えなかったことを、受理済みの資料供給義務の不履行として扱わない。対話を提供する実装も、そこで見つけた不一致を同じ直接反証の形式で提出できる。対話を実装しないNodeは、元claimと提出された証拠だけで同じ裁定を再現できなければならない。

FOW1の実装・受入試験・公開有効化に対話機能を要求しない。有効化前には、対話なしの構成で必要資料の取得、完全再実行、単独での有界な直接反証の構成・搬送・裁定、資料未充足時の成功停止が元期限内に機能することを確認する。新しいTicketの生成費用は§4.3の反復検査で独立に評価する。

## 4. 採掘適格性の三つの独立gate

### 4.1 G-EXEC: 全コア実行の正当性と局所反証

登録された初期入力、program、全weight index、状態更新、全kernel、Readout、停止、`y,z`への結合をtraceが覆う。全局所関係が真なら規範実行に一致すること、不正な規範記録に少なくとも一つの認証された有界反証があることをruleごとに示す。単なるtrace長又はMerkle openingの成功をこの完全性の証明にしない。

Panelは元claimの全実行を再現し、`y,z`と四Rootを照合する。Nodeは元claimから期待される局所関係を検査する。全体の安全性は、能動的な正直Panel、資料・検査機会、適格反証の包含と処理、正しいnative gateに依存する。今回、全Panel悪意でも無条件安全という方式へ変更しない。

### 4.2 G-COLL: 関数の採掘出力zの非衝突性

ruleは、固定されたmodel/kernel系列と全許容入力・登録選別に対し、異なる有効試行`i != j`のコア出力について、明示した攻撃者・乱数モデルの下で

$$\Pr[\exists i\ne j: z_i=z_j]\le\varepsilon_z(Q,\lambda)$$

を証明する。`Q`は提出された成功だけでなく、事前処理、協力者、破棄候補、候補内・複数epoch・複数モデルの適応的選別を含む総予算である。許容する`Q_max`、期間、security parameter、許容失敗確率をprofileに固定し、未設定なら有効化しない。

一つのTB1 EXEC公開値を複数候補が共有する場合も対象にする。各試行が独立一様な乱数を持つと仮定して、共有seedからそのまま独立性を結論してはならない。モデル・入力・ownerを敵対者が選ぶ範囲を定理から省かない。

MOR1の`rho`、`s`のhash、後付けnonceの非衝突性は本gateを満たさない。`z=H(s)`、`z=s`、`z=H(s||constant)`を既存回答に付けるだけの構成も、推論関数由来という名称だけで適格にならない。自然回答が違ったという有限テスト、ゼロ衝突の観測、token入力の単射性は定理の代替ではない。

具体hashを使う関数系列は、hashの仮定と推論状態の分布・評価困難性を分ける。古典ROでのbirthday boundを量子攻撃・QROMへ無条件に移さない。既存PQ方針に適合する主張と、残る量子・適応・合成上の前提をsecurity profileで特定する。

<a id="g-cost-review"></a>

### 4.3 G-COST — 新しい正しいTicketを安く作れる経路の反復検査（2026-11-21改訂）

新規採掘の対象instanceで、初回Ticket照会に必要な正規入力を基準実行より安く作れる経路を複数回検査する。PASSの主張は、指定した条件と予算で調べた経路の範囲に限る。全アルゴリズムに対する数学的費用下界は採掘の要件から外す。G-COLLは§4.2に従う。反復検査には新しい費用policy版を割り当て、過去に固定したrule・署名・claimの意味を変更しない。

1. **検査前に固定するもの。** descriptor・artifact・数値profile、検査世代、基準実装、機材・費用単位、試す経路、異なる入力を導く試行予定と回数、予算、安価と判定する基準費用比をsuiteへ固定する。比較基準を結果に合わせて動かさない。新規の候補を調べる場合は新世代に記録する。
2. **反復と比較。** 各経路について別々に導出した複数の入力で独立した完全実行と比較する。確認対象は回答だけでなく、その試行の四Rootを含む正規Ticket入力である。共通prefix/KV、前処理、batchや複数候補への費用配分を含め、準備と実行の総費用、生成した候補数、適用範囲を残す。測定誤差で境界を決められない結果は未確定とする。
3. **判定。** 正しいTicket入力を事前閾値未満で生成する再現可能な結果があればFAIL。修正し、新たな世代で確認する。全予定試行・全経路の比較が完了し該当例がなければ、その範囲でPASS。資料不足、未設定の閾値、試行不足、旧世代の記録だけならUNKNOWN。正しい高速化それ自体をclaimの算術不正として裁定しない。
4. **追跡。** 入力・基準出力・計測記録・実装版・再現手順と検査外の経路を残す。各Nodeは同じ認証済み計測記録を整数単位で判定し、Node自身の時計や機材の速さを合意入力にしない。後から安価な経路が判明した場合はMAS1の停止・再検査を適用する。検査のPASSから全入力の費用、予算当たりのTicket数、未知の経路の発見確率を推定しない。

### 4.4 現在の完成状態

本改訂で、既存Qwen系列のG-COLLの適合定理又はG-COSTの採択済み実機検査結果を追加したとはしない。旧FULL1の部分和生成量、MOR1のreceipt衝突、TB1の乱数保証、Panelの正当性保証を結合して、その不足を解消したとは主張しない。

現在採択するのは三gateを必須とする設計である。未証明のruleを`verified=true`として登録する経路、テスト合格だけで適格化する経路、正しい局所Courtの数で代替する経路を禁止する。

### 4.5 MAS1対象の追加条件

本節の三gateに、RFC0065のG-USE、構造検査、最新検査世代・rule・安全停止状態を追加する。有限の実用testは用途別の品質根拠であり、各gate固有の根拠を代替しない。採掘専用の無用な重い枝、巨大trace生成又は高価な証明を、コアLLMの有用計算費用として認定しない。

## 5. 四Root、Ticket、補助receipt

`R_x`、`R_y`、`R_z`、`R_tau`は別domainのcanonical commitmentとする。`z`の固定長全bytesを認証する`R_z`と、traceのReadout/Endが同じ`z`を指すことを必須関係にする。単にownerが宣言した`R_z`を、コア出力とみなしてはならない。

FOW1のTicketを次へ一意に固定する。

$$T=H_{\rm FOW1-ticket}\!\left(\operatorname{Enc}_{P}(s,R_x,R_y,R_z,R_\tau)\right).$$

既存AN1の三Root Ticketとは別family・別domainであり、既存の署名codecを意味変更しない。`s`は実入力frameに用いる32byte（QF1ではRFC0066のHEX64 frame）、`T`は32byteの値を規定順序で読む256bit unsigned integerとする。FOW1のhash suite、domain bytes、field型、順序、長さ、空値、padding、整数境界をprofileで完全指定する。既存SHA-256等の名前だけで新wire値を推定しない。二つの正しい符号化を同じ試行に認めない。

唯一のdifficulty snapshotから得た`theta`に対する`T < theta`を一度だけ判定する。境界は`0 <= theta <= 2^256`とし、`theta=2^256`を扱える広い整数型を使う。旧D境界を引き継ぐ場合はAN1と同じ`theta=floor((2^256-1)/D)+1`である。Panel seed、Court nonce、署名乱数、receipt、表示文字列、carrier、自由saltをTicketへ入れない。旧SWC1の未来`R_ticket`、旧Final後receipt抽選、旧複数量子drawをFOW1へ重ねない。

補助receiptを用いる場合は、別domainで完全`C`、認証済みEXECの値、`s`、四Rootをhashする。正式なenvelopeは`(y,z,s,rho)`とする。`rho`やTicketを作る後処理を、その入力になっている`R_tau`又は`R_z`へ循環的に含めない。元claimの署名又はOriginalIdをrhoへ戻さない。

## 6. 何が条件付きで導けるか

### 6.1 一試行一Ticket

固定context、正規TB1 EXEC、決定的コア関数、canonical四Root、固定Ticket encodingがあると、同じ試行の正しいTicket入力は一つである。再署名、別envelope、再送、holderのshare subset、Panel交替によって別の正しいTicketを作れない。

これは決定性と符号化から導ける。別の試行に要する費用は§4.3の対象として調べる。

### 6.2 Ticket数と当選確率

古典的Ticket ROと事前固定targetを仮定する。返値を見る前に正しい入力であることが定まる相異なる初回照会を高々 `k` 件に限れば、1件以上当たる確率の上界は `1 − (1 − theta/2^256)^k` である。§4.3の実測から、任意の計算予算で作れるTicket数 `k` の上限は導けない。実測最小値を普遍的な `c_min` として当選確率や合意weightに使わない。

## 7. 再利用・仕事キー・経済責任

| 対象 | FOW1での扱い |
| --- | --- |
| 同じモデル重み・固定programの保存/配布 | 内容認証を維持して共有できる。費用比較はその事前取得・前処理を含める |
| 同じ試行の再送・Panel再実行 | 同じ`y,z`とRootを使う。追加Ticket・仕事creditなし |
| 別試行でのprefix/KV/cache/batch | 規範意味とoriginを満たし、G-COSTがその共有・償却を含む場合に許す |
| 別ownerによる成果のコピー | 入力・`z`・trace・帰属を新試行へ付け替えられない。同じ実仕事の二重会計も拒否する |
| コア計算を省いて識別値だけ生成 | 正しい規範実行ではないか、G-COSTを破る系列である。単なるreceipt一致で受理しない |
| 正しい出力を求める新しい高速アルゴリズム | 自動的に算術詐欺とはしない。定理の前提と安全性を再評価し、必要なrule停止・移行を公開手続きで行う |

`RegKey`、native slot、`AttemptUse[s]`、canonicalな`ComputationalWorkKey`、用途別`BaseWorkKey`の消費をSuccessFinal・mint・依存index・journalと同じ原子遷移へ含める。新しいfamily名、Model IDの別名、owner、資金tx、round又はreceiptの変更で同じ仕事を未消費へ戻さない。旧新profileをまたぐ対応表とcanonical work identityを移行recordへ保存する。

一つの試行に一つの採掘Ticketを認めることは、回答tokenや仕事量を複数unitに価格付けすることと区別する。価格付けのunit数を、独立した追加当選drawとして再利用しない。新nonceだけで新しい有用需要を認定しない。

不正な計算、署名偽造、二重消費、署名した提供義務違反は既存の証拠に従って扱う。安価な正しい計算、理論の反例の発見、正しいcache利用を、それだけでslashしない。金銭罰又は高価な証明生成をLLM評価下界の代替にしない。rule停止が必要になっても、過去の正当な署名・決済を文書変更だけで遡及没収しない。

## 8. 全consumerの単一gate

FOW1の成功条件はAN1の検証先行Finalへ、現在有効な`FOW1Admissible`、G-COLLの対応TheoremRuleと現世代の費用検査、TB1 certificate/FreezeRecord、正規`z`と四Root、唯一のTicket、一回消費を加える。MAS1対象ではさらにRFC0065のEffectiveMiningAllowedを必要とし、同batchの安全停止を成功sweep・発行・未消費受賞権利の消費より先に反映する。モデル登録画面、miner、Panel、Node、SDK、RPC、import/repair、snapshot/IBD、reorg、reward/credit/weightの全consumerが同じ型・版・gateを使う。

同batchでは適格notice/資料case、反証、失効、依存取消を成功・発行・消費より先に適用する。現在の適格性を過去snapshotの合格表示で代替しない。証拠不足、未知rule、未対応Readout、TB1不成立、元期限切れは成功にしない。

公開済みseedのfreshnessはnativeの通常undoだけでは回復しない。TB1の曝露recordとFOW1の試行消費recordを区別して復元する。旧codecや旧三Root入力を新四Root入力へ自動変換して成功させない。

## 9. 置換表と移行

### 9.1 新profileに適用する優先規則

| 旧条項・方式 | FOW1の扱い |
| --- | --- |
| RFC59 §3.2の「異なる出力を要求しない」 | 自然回答`y`には維持するが、コア採掘出力`z`にはG-COLLを必須化 |
| RFC60 §4/§8の`O=(y,s,rho)`とreceipt衝突定理 | 補助的な識別保証として保持。コア関数の適格性を代替しない |
| RFC59/60の三Root Ticket | 本RFC §5の四Root・新domainへ置換 |
| 未指定の実行/Panel beacon、旧seal-reveal/anchor fold、BLS候補 | TB1の用途別RoundRefとcertificateへ置換。旧値へのfallbackなし |
| ADR0044 F1のprompt-only規則 | 新profileは実challenge入力を必須とする。旧署名済みfree-promptには旧意味を保持 |
| ADR0044/0074等のFinal後receipt抽選、旧quantum複数draw | 新規FOW1には適用せず、一試行一Ticketを使用 |
| 旧sampled approval | FOW1の完全Panel再実行と全checker coverageへ置換。旧claim検査用は保持 |
| 無条件に再利用を認めた費用解釈 | G-COSTが許容・計数する範囲を必要条件に追加。意味を保つ高速化自体は不正としない |
| 一般モデル登録・旧MOR1適合 | FOW1採掘適格性を自動付与しない |

### 9.2 activationとlegacy drain

採択されたactivation recordで、新規受付の切替点`A`、対象network、family、rule集合、codec、旧pendingの対象cut、drain期限と終端を固定する。文書日付やファイル番号をactivation点にしない。

`A`以後に新たなPALW採掘・報酬・credit・weightを要求する全入口はFOW1＋TB1を必要とし、旧AN1/MOR1/free-prompt/旧Ticketによる**新規受付を閉じる**。別API、旧SDK、import、relay、model再登録で旧入口を再開しない。一般モデルの保存登録や通常の推論利用を、旧採掘資格へ接続しない。

`A`前に正規受理された旧claimは、事前固定したlegacy drain規則と元期限・元責任で処理する。旧署名の再解釈、過去のRootの書換え、二重credit、pendingの新規試行化をしない。過去履歴検査のための旧beacon readerは保持できるが、新しい対象へ乱数を供給する入口にしてはならない。

FOW1の有効なruleが一件もない、又はTB1のbootstrap・容量・安定性が成立しない場合は、`A`のactivation gateを満たさない。文書上の採択だけで既存networkを停止したり、代替の旧採掘経路を暗黙に有効化したりしない。

## 10. 有効化前の必須確認

| 群 | 必須の根拠又は否定試験 |
| --- | --- |
| 対話経路を実装しない構成 | 正常claimは完全再実行から既定の承認へ進む。不正なtraceは協力不要の直接反証で裁定でき、資料欠落や未解決caseのまま成功しない |
| コア出力 | 同一試行の`y,z,tau`再現、別試行のz衝突上界、Readout/Endの完全な局所関係 |
| 費用 | 前処理・shared seed・cache・batch・零/低rank・特殊入力・外注を含むG-COSTの反復測定、再現と独立review |
| 代替拒否 | `z=s`、`H(s)`、回答へのtag、receiptのrename、重い証明だけの仕事量認定を合格にしない |
| 登録 | 偽TheoremRule、未確認のweights predicate、未知kernel、自己申告属性、同名旧profileを拒否 |
| TB1 | EXEC/PANEL別秘密、最初の正直share前freeze、全Q、独立RoundRef、公開拒否、曝露reorg、no fallback |
| Ticket | canonical四Root、固定target、再署名・share subset・Panel nonce・自由paddingからの再抽選を拒否 |
| 会計 | 同一仕事の旧新alias、別slot、別model名、再送、import/reorg、全consumerでの一回消費 |
| Court | z/Readout不正、入力/weight index不正、未充足資料、単独反証、悪意多数のpositiveと保護容量 |
| 移行 | A以後の旧新規受付拒否、既受理旧claimのdrain、legacy乱数の新対象利用拒否 |
| 実用 | 一つ以上の実LLMにおける全gate、品質、cold取得、最悪資源、native復旧・合意接続 |

これらは未完了の条件を含む。TB1参照モデルの試験やMOR1 receiptの参照試験を、FOW1コア関数・費用・実モデルの完成試験として合算しない。本RFCの採択後も、証明・実装・実測・公開有効化は各々の証拠で追跡する。
