# RFC-v2-0019 — モデルを持たない Node と有界な局所証拠

2026-11-11の後続採択: [RFC-0053](../rfc/0053-additional-model-burn-pause.md)／[ADR-0221](../adr/0221-additional-model-burn-pause.md)がHF取得可否の認定をburn停止・解除へ置換する。追加登録モデルだけを対象に、誰でもburnでPAUSED、登録者だけがburnでACTIVEへ戻す。genesis・年次fork指定は対象外。第三者取得証明・定期可用性認定・取得不能の多数決を要求しない。HF固定版/hash照合はoff-chainの実行資料同一性確認として残す。claim固有資料、再実行承認、不正反証と責任・原期限は維持する。PAUSEDは新規受付・成功Final・新規報酬確定を止める。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

採択日: 2026-11-08 / 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。

[20本の索引](README.md) · [採択記録 ADR-0215](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) · [上位原則](../PRINCIPLES.md)

追加採択: [RFC-0048](../rfc/0048-exec-credit-schedule-and-one-bps.md)・[ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)。EXEC の 1 BPS 目標と court の全工程を共通資源で受理する場合の制約を加える。設計状態は `ADOPTED_DESIGN_NOT_ACTIVATED` のままとし、性能向上の実測や有効化を宣言しない。

2026-11-09追加採択: [RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)／[ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)。全実行関係の局所完全性、反証検査器の健全性、一人の適格な反証を多数承認で覆せない境界を本書と[18](0018-interactive-negative-court-and-defaults.md)・[20](0020-verification-gated-final-and-adversarial-models.md)で閉じる。本人の計算行為の証明、暗号学的validity proof、新しいproof VMを追加しない。

2026-11-09公開profileの追加採択: [RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)／[ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md)。独立選出されたPanelに少なくとも一人の能動的な正直者がいることを前提とし、その人がmodel未保有から公開資料を取得して全体検証・反証できる経路を要求する。Nodeは最低1件の全体承認と公開検査期間・Q等の成功条件を[20](0020-verification-gated-final-and-adversarial-models.md)に従って確認し、局所反証は自分で再計算する。承認1件は計算全体の直接証明ではなく、資料秘匿を成功へ変換しない。設計状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

## 1. 必須条件

**通常の合意 Node は、モデル本体の全重み・チェックポイントを取得または保持せず、モデル全体の推論を実行しない。**
この条件は、通常の承認確認、positive の局所検査、negative の裁定、再起動、reorg、履歴検証、IBD に適用する。
GPU、モデル用の推論 runtime、モデル配布先への接続、Panel worker の実行を Node の参加条件にしてはならない。

Miner・Panel・worker がモデルを扱う責務は [RFC-v2-0016](0016-miner-execution-commit-and-material-supply.md)／[0017](0017-noninteractive-positive-verification.md) に置く。
同じ運営者が Node と Panel を兼務することは可能だが、役割の兼務と合意 Node の必須機能を分ける。
Node の裁定から、モデルを取得して全推論する fallback を呼んではならない。別プロセスに移した全推論を Node の必須処理にすることも認めない。

本書は新しい claim／証拠 profile の設計採択であり、wire・fence・有効化高さ・新しい資源数値を割り当てない。
旧署名済み claim は旧版の検査器と契約で読む。実装済み・測定済み・稼働済みという状態は別に管理する。

## 2. Node が扱う情報

| 情報・処理 | 扱い |
| --- | --- |
| 全重み、モデル本体、全中間値、モデル用再実行キャッシュ | 保有・取得・生成を要求しない |
| claim と model／program／input／trace／output の root | 元の対象を認証するために保持・参照する |
| 登録された実行定義、規則版、typed primitive の意味 | 有界な metadata とプロトコルの検査コードとして扱う |
| 検証者の抽選・署名・承認範囲・期限・係争状態 | [RFC-v2-0020](0020-verification-gated-final-and-adversarial-models.md) の承認条件として検査する |
| 局所演算に必要な入力・出力・重み断片・opening | 上限内の認証付き証拠として読む |
| モデル URL の応答、ホストの現在状態、Miner の私的 RPC 応答 | native 裁定の権威にしない |

「モデルを持たない」は、対象モデルをロードして推論できる環境を要求しないという運用上の条件である。
重みに依存する局所計算では、その演算に必要な重みの値を証拠へ含める必要がある。
証拠中の重み断片を一値も Node に渡さない条件とは両立しない。局所証拠の公開・保持は、モデルの秘密保持を保証するものでもない。
登録 program を metadata と呼ぶことで、全重みや無制限の定数・計算グラフを Node へ押し込めてはならない。

## 3. 裁定の入力を閉じる

裁定関数は、次の二つから決定する。

~~~text
judgment = judge(accepted_original_and_rules, authenticated_bounded_evidence)
~~~

accepted_original_and_rules は native の受理済み状態から取得する。提出者の packet に書かれた root を、それ自身の期待値として採用しない。
evidence は一括提出、または同じ canonical identity に認証された有界な分割資料である。Node の取得対象を、固定されたその資料集合に限定する。
許可されたチェーン資料の再取得と、外部のモデル本体の取得を混同しない。
提出者が渡した URL、任意コード、取得 callback、モデルロード用 descriptor を局所検査の入力にしない。

不足資料がある場合は、証拠を未完成として扱う。欠けた値をゼロ、既定値、Panel の推測、ローカル cache の別版で補わない。
Node のローカル clock、GPU の有無、モデル cache の有無、Miner の稼働状態によって同じ裁定の結果が変わってはならない。

## 4. 局所証拠の論理 schema

| 要素 | 認証・検査する内容 |
| --- | --- |
| domain | network、claim／evidence 版、work namespace、ruleset |
| original | 元の claim、受理済み model／program／input／execution／trace／output の root |
| location | canonical な演算・leaf・tensor・range・座標と leaf 数 |
| relation | WellFormed／Init／Step／Endのどの構成関係か、そのschema版、canonical関係ID、許可された分解規則 |
| operation | 演算種別、入力元、shape、dtype、丸め、出力先、必要な制御条件 |
| operands | 実際の演算が読む中間値・初期入力・状態境界・履歴依存値 |
| parameters | 実際に読む重み断片と、登録 artifact の該当座標への opening |
| recorded_output | original の trace／出力に結び付いた、Miner が主張した値 |
| authentication | 各値・演算定義・接続の認証経路、source の版、canonical codec |
| state／control | 必要な前後state root、read-from／認証更新、許可された選択・進行・乱数状態、終端と出力・仕事量の対応 |
| bounds | native が受理した work、bytes、heap、I/O、parts、保持の割当参照 |

これらの欄を証拠の自己申告だけから信用しない。値の所属と、検査する計算関係への結び付きを別々に確認する。
演算定義も固定された program に属さなければならない。任意の足し算・Clamp を提出して、元の別演算を検査したことにしない。
dynamic index、routing、mask、KV／prefix、履歴 state を読む profile は、その選択と依存先も認証し、必要な関係を局所検査の範囲へ含める。
Panel が再実行して作った opening は、元の Miner の root と一致する場合にだけ、その original の資料として使用できる。
Panel の署名付き witness の ID を、Miner の元の実行 root と同一視してはならない。

## 5. Node の検査順序

1. **外形と割当。** 版、長さ、分割数、割当参照、費用・権限を確認し、上限外の allocation を開始しない。
2. **元の対象。** accepted state から期待する claim と規則を解決し、packet の全 binding を照合する。
3. **演算。** 登録実行定義から対象位置の演算・型・形状・接続・数値規則を確認する。
4. **資料認証とrecord形式。** 上限内のraw記録と認証封筒を各original rootへ照合し、元recordのWellFormednessを検査してから数値をdecodeする。入力、重み、記録出力、状態境界、必要な初期条件の所属とschemaを分けて確認する。
5. **依存集合。** 必要な値が欠けず、禁止された値・重複・別の座標・非 canonical な余剰を含まないことを確認する。
6. **局所再計算。** その profile の決定論的な work 上限内で、対象の typed primitive または有界なInit／Step／Endの構成関係を実行する。接続・更新・停止・出力・仕事量も、対象関係に必要な範囲で検査する。
7. **比較。** 得られた値を Miner が固定した値と比較し、結果の対象・範囲・証拠 identity を返す。
8. **native 反映。** 正規に受理された case へ結果を結び付け、[RFC-v2-0018](0018-interactive-negative-court-and-defaults.md) の責任・決済を適用する。

検査器は申立人から、自由な program、任意の計算回路、全モデルを実行する手続きの実行権限を受け取らない。
登録された versioned kernel／typed primitive の許可された意味だけを使う。
Merkle opening と署名は、同一性と作成者の認証に使用する。計算全体の正しさを圧縮して証明する方式は導入しない。

### 5.1 同一性と計算関係を別に検査する

元rootへのmembershipだけでは、開いた値が対象演算の正しいoperandであるとは限らない。Nodeは、[16 §4](0016-miner-execution-commit-and-material-supply.md#4-canonical-traceとrootの条件)の関係schemaから必要なoperandとread-fromを導出し、packetが指定した任意のsource集合で代用しない。演算定義がprogram内に存在するだけでなく、その位置・型・入力辺・出力辺・選択条件が一致しなければならない。

state更新では、直前rootに対する読み出し・書込みのopeningを確認し、許可された更新だけから次rootを導出する。変更対象外のsubtreeを保持することも認証経路で確認する。複数の要素更新の順序、alias、同一位置への重複writeはschemaで一意にする。前後rootの個別membershipだけでstateの連続性を保証したことにしない。

例として、固定整数規則の関係が`z = s + w*x`で、元claimに属する値が`s=8,w=3,x=5,z=26`であれば、型・位置・演算・read-fromを認証した後にNodeが`23 != 26`を確認して反証できる。別のprogramの足し算を持ち込んだ場合や、`s`の出どころが異なる場合は、同じ数値でもその算術例をMiner有罪の根拠にはしない。

### 5.2 反証検査器の健全性と局所完全性

本節の対象は、[16 §4.1](0016-miner-execution-commit-and-material-supply.md#41-全実行関係を有限の局所関係へ分解する)の`ValidTrace(C,T)`である。各profileは以下を満たさなければならない。

1. **健全性。** Nodeが`ComputationRefuted`を返した場合、固定された`C`に対するMinerの認証済みrecordの少なくとも一つの必須関係が偽である。署名・hashの安全性、型とcodecの一意性、native checkerの正しさを前提とする。Panelの比較用traceの誤りだけで成立してはならない。
2. **局所完全性。** `C`の認証可能な有界raw記録`T`が必要範囲まで取得でき、`ValidTrace(C,T)`が偽なら、`WellFormed`を含む少なくとも一つの必須関係の違反について、Nodeの固定上限内で判定できる元claimに束縛した証拠を構成できる。
3. **全実行との対応。** `WellFormed`、`Init`、全`Step`、`End`が真であるなら、その記録は固定model／program／input／rulesに従う実行であり、宣言output・長さ・仕事会計も規則に一致する。本人のGPU利用や、各Panelが独立に実行した履歴をこの結論へ含めない。
4. **構成可能性。** 完全再実行した正直な検証者が、最悪の必要資料取得・比較・局所化・opening構築を[18](0018-interactive-negative-court-and-defaults.md)の原期限と予約内で実施できる。存在だけの証拠や、無制限探索を必要とする証拠では足りない。

**証明スケッチ。** まず認証済みraw記録のWellFormedが偽なら、そのschema違反を反証する。WellFormedが真ならInitが最初の境界を固定された入力・モデル・依存へ結ぶ。各Stepの正しいread-fromと決定論的演算、許可されたstate更新・進行から、帰納的に次の境界が正しくなる。Endが正規の停止・宣言出力・長さ・仕事量を結ぶので、全関係が真なら宣言された実行結果が正しい。対偶により、誤った実行記録にはWellFormed、Init、いずれかのStep、またはEndの偽の構成関係がある。全構成関係が元rootから開示でき、各関係のcheckerと証拠が有界であることから、その一つをNodeで裁ける。

この帰納は、TIR／kernelの意味と局所schemaの対応を証明する設計上の議論であり、Nodeへ全計算の圧縮証明を渡すプロトコルではない。単一の正しい局所関係、少数のランダム検査、root一致だけで帰納の全前提が検査済みになったとは扱わない。

特に、任意のcheckpoint間に未分解の巨大な計算が残る場合、read-fromを検査しない場合、Endを欠く場合には、この証明は成立しない。当該profileは受理しない。存在しないtraceのhashを提出して資料を隠す場合は、局所完全性の取得前提を満たさないため、算術有罪を捏造せず18／20の資料義務・成功停止・未検証終了に従う。

### 5.3 profileを受理するための閉じた対応表

許可kernelごとに、プロトコルが採用した次の対応を登録し、元claimがその版を参照する。提案者の自己申告フラグや、外部モデルrunnerをこの対応の代わりにしない。

| 要素 | 必須内容 |
| --- | --- |
| 意味と関係schema | 固定TIRの意味、WellFormed／Init／Step／Endの全構成関係、canonicalな位置と接続の導出規則 |
| 分解と対応証明 | 巨大演算・融合演算の分解が元の数値規則と同値であること。省略・重複・余分なwriteがないこと |
| 証拠抽出 | Minerの初回rootから必要recordとopeningを取得・組立てる方法。欠落時の適格要求先 |
| native checker | 各関係の決定論的検査器と結果型。演算、接続、初期・終端を含む |
| 上限 | 各関係の最大record数・bytes・経路長・decode／hash／算術work・heap・I/O・分割数と局所化の全工程予算 |
| 適用範囲 | 許可kernel・shape・長さ・layoutと版。範囲外のclaimを拒否する決定論的規則 |

許可された構成規則に従うprogramの組合せだけを受理する。すべてのmodelを列挙して審査する方式や、Minerが任意のcheckerを提出する方式にはしない。未知kernel・未知schema・上限未確定・対応未成立は`UnsupportedProfile`であり、別kernelのcheckerやモデル全体の再実行で代用しない。

この表の論理要件を満たすこと、実装が要件を満たすこと、対象実モデルの最悪資源が上限内にあることは別のgateである。本書の証明スケッチだけで、未実装のcheckerや未測定profileを有効にしない。

### 5.4 不正な証拠と不正な元recordの区別

外側のpacketのcodec・長さ・権限・opening・元claimとのbindingが不正なら`EvidenceRejected`であり、元Minerの計算有罪でも既存caseの解決でもない。上限内の正規packetが元Miner rootに属するraw recordを認証し、そのrecordの規範schema違反を示す場合は`WellFormed`の反証として`ComputationRefuted`を返す。この結果名は算術だけでなく元claimの必須構成関係の破れを含む。

unknown dtypeや非canonical整数等の内側recordを先にdecodeして申立人の不正と決め付けない。16のparseと独立なraw認証を先に行い、位置に対応する規範schemaをnative stateから解決して判定する。Nodeが上限内で認証できない巨大record、欠落したopening、未開示recordからは反証を導かず、既存caseと供給義務を維持して18／20の有限終了へ進める。

### 5.5 固定長の整数計算区間と実LLMのcoverage

巨大な行列積の一出力要素でも、全内積を一度に読むとNodeの上限を超え得る。具体化の一案は、profileで固定した最大B項の区間ごとに、入力範囲と前後の累積値をMinerの初回traceへcommitすることである。数学的な加算に相当する場合の形は次になる。

~~~text
s[j+1] = s[j] + sum(w[k] * x[k], k = j*B .. min((j+1)*B, K)-1)
~~~

これは無限精度の式をそのまま検査器にする指示ではない。実際の関係は、固定した整数幅、乗算の拡張、還元順序、overflow／saturation、丸め・再量子化の位置を持つcanonicalなfoldとして定義し、元kernelとの同値性を示す。未使用padding、最終区間の長さ、初期累積値、前区間へのread-from、最終出力への変換も必須関係である。Nodeは当該区間の最大B組の値と境界・接続・openingを読み、その範囲だけを再計算する。Bは最低要件Nodeで全wrapper・hash・parse・heap・I/Oを含めて測定し、protocolが採用する版付き上限として固定する。本採択ではBの数値を割り当てない。

checkpointだけを置いて、その間にNode上限を超える未分解演算を残してはならない。Panelが後から作った部分和や、異議後に変更した区間配置を元Minerのtraceとして認めない。区間を大きくすれば局所裁定費が増え、小さくすればMiner／Panel／providerのtrace・hash・I/O・保持費が増える。[05](0005-bounded-resources-and-long-context.md)の全工程予算で両方を測る。

実LLMの採用profileでは、embedding／index／shape、行列積・加算・再量子化、normalization／activation、attention・mask・位置情報、KVのread／write、必要なRNG・token選択、停止・出力・長さ・仕事会計の全関係について、§5.3の対応表を完成させる。実際の許可kernelの集合に応じて列挙し、不要な演算を新たに必須にするものではない。LUTや整数近似を使う場合も、表と定義域・丸め規則を固定programへ結び付け、その近似を元の別数値規則と同じ計算だと偽らない。

正直なPanelは出力だけでなくcanonical trace rootまたは全必須関係まで照合する。同じ出力でも誤った途中trace、古いstateの参照、偽の停止長・仕事量を承認しない。対応済みkernelから構成できるmodelへ仕組みを再利用できる一方、未知kernel・未分解shape・未確定上限を自動対応扱いにしない。少なくとも一つの対象実LLMで、正常Final・元trace不正・read-from不正・状態改変・終端不正・不供給Abortを同じnative実装で完了することが有効化gateである。

## 6. 結果と保証の範囲

| 結果 | 意味 | 禁止する読み替え |
| --- | --- | --- |
| LocalPredicateHolds | 検査した局所関係は一致した | 推論全体の正しさ、無条件の Final |
| ComputationRefuted | 認証された original の必須関係に矛盾がある | 無関係な当事者・claim への一括 slash |
| EvidenceIncomplete | 必須資料をそろえて検査できない | 暫定 success、モデル取得による再実行 |
| EvidenceRejected | 対象・認証・形式・上限が不正である | それだけによる Miner の算術有罪 |
| UnsupportedProfile | Node が合意上受理可能な検査器を持たない | 汎用モデル runtime での代用 |

複数の小さな局所 positive の合格も、それだけで検査していない関係を正しいと確定しない。
claim 全体の独立検証は Panel 側、選出者の承認条件は RFC-v2-0020 に置く。
モデル内の Miner・Panel が寡占されている可能性を除外しない。全体の抽選検証者まで悪意ある主体が支配する場合に、小さな証拠だけで全体の正しさを無条件保証するとは主張しない。
この残る前提と、Node による局所反証の客観性は別に記録する。

`ComputationRefuted`の判定は、対象claimに対して一人が提出した適格な証拠だけで成立する。反証者の数、肯定署名数、Panelの投票結果を局所関係の真偽に使用しない。多数の肯定承認があっても評決を上書きできない。notice受付・資源保護・同一block内の優先順・Final／creditへの効果は[18](0018-interactive-negative-court-and-defaults.md)と[20](0020-verification-gated-final-and-adversarial-models.md)に従う。

## 7. 一件の資源上限

同じ profile の受理と検査は、同じ有界な資源契約を使用する。提案者の自己申告の小さい見積へ置き換えない。

| 資源 | 規範 |
| --- | --- |
| proof／material bytes | 本体・wrapper・認証経路・分割 metadata を含める。超過 packet を切り詰めない |
| input／parameter elements | tensor の長さだけでなく、実際に展開・参照する要素数を制限する |
| paths／siblings／nesting | Merkle 経路、重複、深さ、再帰・展開の仕事を制限する |
| deterministic work | 演算・hash・parse・依存探索等を、決定論的な単位で計量する |
| heap／temporary bytes | 同時に生存するコピー、復号後データ、scratch、経路集合を含める |
| I/O bytes | cold path の認証資料読出しと、保存・再取得で必要な量を含める |
| parts／reassembly | 分割数、順序、未完成集合、重複受信、再構成中の容量を制限する |

CPU の実時間・peak RSS・帯域は、対象機器と敵対的入力を明示して測定する。
ホスト固有の経過秒数を native の算術評決に使わない。実時間の目標と、合意上の決定論的 work 上限を分ける。
OS timeout やローカル資源不足を、Miner の計算不正という評決へ変えてはならない。
整数 overflow、長さの掛け算、圧縮後と展開後の容量、型変換は allocation 前に検査する。
ハッシュ認証後なら無制限に展開してよい、という例外を設けない。

局所算術の CPU が小さくても、必要 witness、wrapper、opening、分割・再構成、搬送・収録の bytes が上限を超えるなら受理可能とはしない。一つの proof を分割できることだけでは、全体の包含量・再構成メモリ・最終提出期限を満たしたことにならない。末尾の資料が届いてから行う認証と裁定も計上する。

同じ認証済み blob を複数証拠で読む場合、共通 bytes の I/O・保存を実際に省ける範囲だけを共有費用とする。proof ごとの original、座標、role、ruleset、署名、接続の検査を省かない。同じ hash から別 claim の評決を流用せず、重複 packet と異なる必要 binding の集合を区別する。どの work を一度だけ実行できるかを検査器の版と cache key に明示し、Node の手元にない資料を見積上だけ cache 済みにしない。

## 8. Node 全体の資源上限

一つの court が小さいだけでは、低性能 Node の条件を満たさない。次を別途 native の受付予算へ含める。

- ブロック当たりの総検査 work、総受信 bytes、演算定義の新規取り込み量。
- 同時 claim／court 数、未完成 packet の合計、pending の保持量。
- 保存する局所証拠、終了記録、依存資料の累積容量と所定期間。
- 再起動・reorg の最大再検査量、cold IBD の一時領域と read amplification。
- 通常 positive、negative、公開回復に必要な各予約と、重複排除後の総量。

warm cache のある一台だけが上限内に収まることを、cold Node の受理根拠にしない。
Node が保持していない資料を、見積上だけ既存 cache として差し引かない。
多数の署名・多数の小さな proof に分割しても、元 claim とブロック全体の上限を再生成できないようにする。
通常 Node の最低要件に GPU・モデル常駐を足して、受理済み profile の超過を解決しない。

### 8.1 依存と実供給を含む受付

Node 側の上限は、[05](0005-bounded-resources-and-long-context.md) の task DAG、資源別総需要／実保証供給、実行可能な予約 schedule の三条件へ接続する。Node に届く前の資料取得・局所化・証拠作成をゼロと見積もらず、それを担当する Panel／worker／provider の予約と、Node の認証・包含・裁定の予約を結ぶ。Node の一件の検査器だけが速くても、cold outsider の取得から裁定までの計画が成立しなければ、その profile の完了保証にはならない。

複数 claim と全合法同時分岐に対して、CPU、署名・hash・parse、I/O、転送、制御・包含、メモリ、保持を計上する。少数の worker を増やしただけで共通リンクの供給が増えるとは扱わない。資料 release 時刻・各手番の締切・元の hard・既存予約を満たす schedule がない場合、critical path と全期間の総量だけが上限内でも受理しない。

### 8.2 EXEC と court の合算予約

EXEC block の検証、仕事 credit の状態更新、通常 positive、negative、公開取得・回復、BASE／Heartbeat の claim・court・anchor・schedule が同じ物理資源を使う場合、その需要を共通の台帳に加算する。queue を分けるだけで別の CPU・I/O・帯域・包含能力が存在するとは扱わない。court の予約を保持したまま、予約外の EXEC／claim 需要へ backpressure を適用できるようにする。

EXEC の 1 BPS は目標であり、未測定の最低搬送量や署名検証率を保証しない。過去の RPC 投入率、平均 block 間隔、未確定 credit から、court が期限内に使用できる枠を導出しない。供給能力は、最低要件の Node、競合負荷、実際の canonical 包含、各手番の機会を含む測定と合意上の予約機構に対応させる。

EXEC が止まり、または利用可能な credit がない状態でも、BASE／Heartbeat の claim・court・anchor・schedule 経路と必要資源を維持する。未検証な仕事から次の EXEC を作れないために、その仕事の検証資料・異議・終端を処理できない循環を禁止する。CPU・帯域の不足を、court の原期限の延長、Node の全モデル取得、肯定承認の省略で解決してはならない。

### 8.3 cold Panelと悪意多数の係争を含む上限

モデル未保有の唯一の正直者が資料取得から全体検証を行うことはPanel側の責務であり、Nodeのmodel fetchや全再実行へ移さない。公開providerのURL、cacheの宣言、受領署名だけを取得完了の客観証明としない。資料要求のnative処理、必要な有界応答、InspectionPending、計算caseへの継続は[18](0018-interactive-negative-court-and-defaults.md)に従う。全資料を小packetへ分割してNode経由で配り切るfallbackを、取得可能性や正常完了の保証にしてはならない。

一人保証でも誰が正直かをNodeが識別する前提はないため、選出された各正規化主体の有限の取得・提出・裁定枠と、他主体が合法に使える最大caseを[05](0005-bounded-resources-and-long-context.md)で合算する。悪意の不署名で最低1件の正規承認を無効にせず、悪意の虚偽caseは有界な局所裁定・正規terminal・客観的defaultで終結する。無効packetの拒否だけで既存caseを閉じること、別人の終了で正直な人のQを消すことは認めない。

全合法caseを元hardまでに終結できる供給・包含が成立する条件と、実サービス不供給により安全にAbortする条件を別に測る。bondを増やしたことだけでCPU・帯域・包含枠が増えたと扱わない。全Nodeが同じ裁定を処理する場合、Node台数の増加を一Node当たりcourt容量の倍率にしない。atomic batchは[20](0020-verification-gated-final-and-adversarial-models.md)の単一originalとして全leafの仕事・資料・最大case・価値を予約し、部分creditやbatch内のcase権利の併合を認めない。

## 9. cold Node・再起動・履歴検証・IBD

受理済み claim と規則版、抽選・承認、実際の局所証拠、裁定・帰責・決済の根拠を、認証された保存経路で復元できるようにする。
局所証拠が複数部分に分かれる場合、完成済み範囲と未完成範囲を保存し、再起動で未検査部分を検査済みにしない。
保存された Passed=true、Panel の署名、資料 root だけを、必要な局所算術の再検査資料の代わりにしてはならない。

Node が新しいモデルを知らなくても、固定された小さな演算定義と証拠から同じ評決を再計算できなければならない。
元の Miner、当時の Panel、モデル配布サイトが停止していても、所定の保持経路が供給する局所資料で再検査できることを確認する。
必要資料をローカルに取得できない場合、その Node は検証・同期を完了扱いにしない。
この取得失敗だけを、過去の claim の算術有罪や既存 block の無効性という新しい合意事実にしてはならない。

pruning は [RFC-v2-0012](0012-authenticated-state-storage-and-ibd.md) と [0018](0018-interactive-negative-court-and-defaults.md) の保持契約に従う。
残る訴追・依存・reorg の責任に必要な bytes を、単なるディスク目標のために削除しない。
保持期間と証拠の新規受付期間を分け、過去の資料を保存しただけで新しい訴追権を作らない。

## 10. 受付と失敗時の処理

受理前に、その claim の正常検査と、最悪の適格な異議・公開回復を Node の上限内で処理できることを確認する。
これは claim／実行 profile の処理可能性の条件であり、モデルの公開状況・保持者数を審査して階層を変える制度ではない。
小さな局所化へ到達できない profile は、モデル本体を Node が取得する fallback を前提に受理しない。
一つの巨大な行列演算を「一演算」と呼ぶだけでは、資源上限を満たしたことにならない。

受理後の資料不足・サービス障害は、規定の保留・回復・Abort を使用する。timeout や上限超過を成功 Final に変換しない。
不完全な反証が届いたことだけで、有効な既存 claim を無条件に永久停止させない。適格 notice と費用・期限の扱いは RFC-v2-0018 に従う。
当事者の提供義務違反は、Node の局所取得失敗とは分け、正規の義務・提出機会・期限・不履行から裁定する。

予約不供給や DAA の進行だけで応答機会の成立を推定しない。`NoCapacity`／`DeadlineExhausted` を含む未完了は元時計の未検証終了と残責任の保存へ接続し、成功 Final、自動 slash、証拠の全削除に変換しない。係争 hard と資料保持・残責任の期限は独立に検査する。

資料を持たない outsider は、必要なモデル断片・実行資料を自らの worker と公開供給経路で取得して局所証拠を作る。合意 Node が outsider の全再実行を肩代わりする経路は設けない。全 owner が私的配布を拒んでも認証済み共通資料から裁定できる経路を確認し、実際の資料がどこにも供給されない場合は、反証を捏造せず [18](0018-interactive-negative-court-and-defaults.md)・[20](0020-verification-gated-final-and-adversarial-models.md) の未検証・有限義務違反の条件を適用する。

## 11. 現行実装との接点

既存の PalwTirClassRecordV1 は登録 program の bytes を保持し、局所裁定では palw_tir_binding_with_program_v1 がそれを補う。
本書は、この有界な実行 metadata の参照を認める。全重み保有とは区別し、metadata 自体にも受付・メモリ・検査仕事の上限を課す。
全計算グラフの保有まで不要にする追加 profile では、演算定義・座標・接続を登録 program commitment へ認証する局所形式が必要である。root だけを渡して演算内容を信用しない。

既存の palw_tir_court_v1 は、必要な入力・parameter opening 等を運ぶ局所検査の接点である。
palw_public_evidence_package_v1 は現時点の参照コードでは scalar Clamp の限定形式であり、全演算を扱える汎用裁定の完成を示さない。
その限定形式を、任意の演算定義がすでに元 program へ認証されることの証拠として使ってはならない。
旧 full-coverage Positive Court の「Node が全関係を直接検査する」という保証を、小さな局所検査と承認署名へ無言で読み替えない。
新しい checked scope・証拠形式・claim 版を分離し、実装接続と資源測定を新規に確認する。

## 12. 必須の受入確認

| ID | 条件・攻撃 | 必要な結果 |
| --- | --- | --- |
| L01 | モデル cache 空、GPU なし、モデル配布先への通信禁止 | 正常な承認確認と有界な局所検査が完了する |
| L02 | Node のモデル読出し API／全推論 API を呼出し禁止にする | positive・negative・復旧の全経路で呼ばれない |
| L03 | 演算種別・dtype・shape・丸め・接続を差し替える | 元 program への認証・意味検査で拒否 |
| L04 | 別 claim／別 model／別位置の値・opening を混ぜる | binding 不成立として拒否 |
| L05 | 重み・入力・状態境界が一つ欠ける | EvidenceIncomplete。補完 fetch・既定値・success なし |
| L06 | 長さ overflow、巨大展開、深い経路、多数 sibling | allocation と実行を上限内で拒否できる |
| L07 | 小 packet の大量分割、複数 case、再送で累積枠を回避 | claim・ブロック・Node 全体の上限を維持する |
| L08 | cold Node と warm Node、異なる CPU の決定論的実行 | 同じ入力資料から同じ評決・証拠 identity を得る |
| L09 | 原 Miner／Panel／モデル配布先を停止して履歴再検査 | 所定の保持済み局所資料で完了し、モデル取得しない |
| L10 | 分割証拠の途中で停止・再起動・reorg | 未検査範囲を成功へ変換せず、一回性と予約を復元 |
| L11 | 一つの LocalPredicateHolds と多数の結託署名 | 全計算の直接証明と表示せず、独立承認条件を別に読む |
| L12 | 登録 program が許可上限を超える | claim の処理可能性として拒否し、Node 要件を拡大しない |
| L13 | IBD 中に局所証拠を取得できない | 同期未完了として扱い、過去の算術有罪を捏造しない |
| L14 | 局所算術は速いが wrapper・opening を含む witness や分割包含量が上限外 | allocation／受付で拒否。CPU の短さを全工程の受理根拠にしない |
| L15 | 同じ blob を別 claim・座標・規則版・役割の proof に使い、重複費用削減を主張 | 認証済み bytes の再利用だけを許可し、各 binding と累積 work を検査する |
| L16 | 全 owner が共謀し、cold outsider が公開資料から反証を作る | worker 側の取得から Node の局所裁定までを確認。Node のモデル読出し・全推論 API は呼ばない |
| L17 | EXEC が 1 BPS の目標負荷を発生させ、公開 court と CPU・I/O・帯域・包含が競合 | 合算予約・queue 分離・backpressure で受理済み救済を守り、未供給を見かけの容量に算入しない |
| L18 | EXEC 停止／credit 枯渇と、notice・応答・anchor・schedule 更新が重なる | BASE／Heartbeat の必要資源を維持し、次の EXEC や未検証仕事の信用付与を前提にしない |
| L19 | 複数 claim の非重複要求と restart／reorg／pruned IBD の再検査が重なる | 全需要・同時占有・各期限を予約へ反映し、単一 claim の測定で受理しない |
| L20 | 予約はあるが service が供給されず、model data の末尾も未取得で期限に達する | 未検証終了と残責任の保存。自動 slash・成功 Final・全モデル fallback なし |
| L21 | Init・各Step・Endのそれぞれを一箇所だけ改変し、残りの局所演算を整合させる | 各種類について元claimの有界な反証を構成・裁定できる |
| L22 | 正しいmembershipを持つ別位置・別producer・古いstateをoperandにする | read-from／接続検査で検出し、値のmembershipだけでpassにしない |
| L23 | 書込み値は正しいが変更対象外のstate rootを差し替える | 認証更新から導出した次rootと一致せず、局所反証できる |
| L24 | 未実行のsuffixを切り落とし、出力root・生成長・workを改変する | End／進行／会計関係で検出する。全演算の一部が正しいことを全体passへ変換しない |
| L25 | 許可された各kernelの融合・非融合表現で丸めが異なる | 数値意味との対応不成立としてprofileを拒否する |
| L26 | 反証に必要な一関係だけbytes／work上限を超える | 局所完全性と資源条件がそろわないため、元claimのprofileを受理しない |
| L27 | 一人の有効反証に対して残り全Panelが肯定署名する | NodeのComputationRefutedを維持し、18／20により無効化する |
| L28 | 大多数の局所positive、trace root一致、自己申告completeだけがある | 全関係の検査済み証明とせず、profile対応・独立承認・異議条件を別に検査する |
| L29 | 正規の証拠封筒・openingが不正codecの元Miner recordを認証する | 申立人の形式エラーとせず、WellFormedの局所反証としてComputationRefuted |
| L30 | packet自体の不正と、開示不能／上限外の元record | 架空の構成違反を作らず、packet拒否または資料未完了を返す。受理済みcaseを勝手に閉じない |
| L31 | 唯一の正直なPanelだけがmodel未保有で、他の全Panelが承認 | 公開取得から全体検証・小さな元claim反証まで完了し、Nodeは全modelを取得しない |
| L32 | 唯一の正直なPanelへ選択的に資料を渡さず、他Panelが承認 | 算術反証を要求せず適格資料caseを処理し、未解決中は成功不可。反証を捏造しない |
| L33 | 正常claimへ悪意多数が不署名・最大の合法虚偽caseを提出 | 正しい供給・包含前提でcaseを有界に終結し、一人の承認から正常Final。Abortだけを合格にしない |
| L34 | 固定B区間の境界値、最終区間長、overflow、丸めを個別改変 | 元kernelと同じ規則の有界反証を作れる。後出し部分和は拒否 |
| L35 | 出力は一致するが中間trace・read-from・停止長・workが不正 | 出力一致を全体承認へ使わず、該当必須関係を反証する |
| L36 | 資料不足を大量の小packetへ分割しNodeに全modelを搬送させる | per-proof／claim／block／Node累積上限と役割境界で拒否し、全model fallbackなし |
| L37 | atomic batchの一leafの局所反証・資料不足・部分credit要求 | 全originalのRefutedまたは成功holdへ接続し、部分creditなし。個別CaseIdの責任は維持 |
| L38 | 一つの実LLMで対応表に未検査kernel／未知shapeが残る | UnsupportedProfileとして有効化しない。別演算の成功や多数署名でcoverageを補わない |

この表は受入要件であり、本書の作成によって試験済みにはならない。承認済みの数値 profile、最悪資源測定、native 接続、restart／reorg／IBD の証拠をそろえてから、有効化を別に判断する。

測定記録は [05](0005-bounded-resources-and-long-context.md) の条件に従い、計算 work・bytes・実 CPU／wall clock・DAA 座標と換算範囲・分位点・未完了を含む分母を分離する。受付の安全性と目的の実モデルの有用性・完了を別に判定し、debug の局所 PASS、投入受付だけの成功、超過拒否だけで court の DAA 問題を解消済みとしない。
