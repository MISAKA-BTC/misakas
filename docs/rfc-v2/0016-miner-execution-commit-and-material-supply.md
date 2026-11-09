# RFC-v2-0016 — Minerの実行commitとclaim固有の資料供給

| 欄 | 内容 |
| --- | --- |
| 状態 | **ADOPTED_DESIGN_NOT_ACTIVATED** |
| 採択日 | 2026-11-08 |
| 採択根拠 | ユーザーによる、モデルを持たないNode・Minerの資料供給・非対話positive・対話negativeの採用指示 |
| 決定記録 | [ADR-0215](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) |
| 適用 | 新しいclaim／commit形式を明示的に有効化した経路のみ |
| 実装・検証・稼働 | 本文の採択では完了・成功・有効化を宣言しない |
| wire・署名domain・fence・高さ | 本文では数値・識別子を割り当てない。旧形式を再利用しない |

[20本の索引](README.md) / [17 positive](0017-noninteractive-positive-verification.md) / [18 negative](0018-interactive-negative-court-and-defaults.md) / [19 Node](0019-model-free-node-and-bounded-evidence.md) / [20 Finalと脅威モデル](0020-verification-gated-final-and-adversarial-models.md)

2026-11-09追加採択: [RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)／[ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)。本書は、Minerに帰属する全実行関係と、その局所反証可能性を定義する。新profileの「計算証明」は固定されたモデル・入力・規則に対するexecution integrityと客観的な局所反証を指す。Miner本人の機器利用、Panel本人の独立実行、全検証者共謀下の無条件positiveを証明する意味ではない。設計状態は **ADOPTED_DESIGN_NOT_ACTIVATED** を維持する。

同日の公開資料・単独正直者profileの追加採択: [RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)／[ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md)。各claimを担当する独立選出Panelに少なくとも一人の能動的な正直者がいることを条件とし、その人のモデル事前保有を要求しない。プロンプトと検証に必要な全資料を、選出後に取得・認証・再配布できる供給契約へ具体化する。17・20の最低1件の完全検証承認は、この供給・検査・単独反証経路の代用ではない。

## 1. 目的と適用境界

Minerが結果だけを出し、Panelが以前からモデルを持つことに検証を依存させる構成を改定する。
Minerは元の実行に対する主張を署名固定し、そのclaimの独立検証と局所裁定に必要な資料を供給する責任を持つ。
Panelは資料を取得して検証し、Nodeはモデル本体を取得せず、承認条件と上限付きの局所証拠を検査する。

選出前にモデルが共有済みであること、モデル保有者の中に正直な者がいることを前提にしない。新profileの正直な一人は当該claimを担当する独立選出Panelの中にいるという前提であり、公開URLや抽選記録からNodeがその人を識別する保証ではない。
登録者・Miner・モデル保有者に偏った他の参加者が同一の悪意ある主体であり、正直な選出者にだけ資料を隠す場合を扱う。秘匿を新profileの正常な供給形態として認めず、適格要求・成功停止・客観的な義務違反または未検証終了へ接続する。
全選出者が共謀する場合、正直な者の取得・検査・提出経路が成立しない場合の無条件な保証は与えない。安全性・正常完了・包含の各前提と限界は20が所有する。

本書が定義するのはclaimごとの供給・検証契約であり、モデル登録の公開性審査ではない。
モデル階層1.0／0.3、登録時の無審査、年次指定、既存の発行・bond上限を変更しない。
資料を出さないことからモデル自体を失格・降格・再審査にする経路を追加してはならない。

## 2. 責務とデータの行き先

| 役割 | 必須の責務 | 完了したと扱ってはならないもの |
| --- | --- | --- |
| Miner | 元の結果と実行traceの固定、必要資料の生成・供給、署名済みの有限義務 | rootやURLの提出だけによる検証完了 |
| 資料提供者・keeper | 明示的に同意した範囲の取得・搬送・保持・再供給 | custody署名による計算承認 |
| 選出Panel | 認証済み資料による独立検証、17の承認または18の異議 | 資料受領だけによるpositive |
| 適格な第三者 | 公開要求と、元のclaimに結び付いた異議の提出 | Panelの許可を必要とする参加 |
| Node | 受理済み状態、署名、root、義務、上限付き証拠の検査 | モデル本体の取得・全推論による救済 |

モデル本体・大きなtraceはMiner／Panel／資料提供者側で扱う。
チェーンにはcommit、契約、必要な認証情報、承認、局所証拠を規定の上限内で載せる。
公開提出経路を別の鎖や外部DAの合意へ移す設計にはしない。
通常の認証付きファイル配信・キャッシュを使っても、取得可能性をホストの名称から推定しない。

### 2.1 公開検証パッケージと取得権

新profileのclaimは、次の内容を元のcommit・manifestへ結び付け、選出Panelと適格な公開第三者が取得できることを供給義務に含める。

| 公開対象 | 固定する内容 |
| --- | --- |
| モデル | 当該計算に必要な全重み・adapter・構造・登録program、各rootと座標 |
| プロンプトと入力 | system／user等の実際のプロンプト全体、正規の入力token列、tokenizer・前処理の版と変換規則 |
| 数値意味 | 整数幅、符号、scale、量子化・再量子化、丸め、飽和・overflow、還元順序。整数であることだけを完全一致の根拠にしない |
| 開始状態と生成 | seed、RNGと消費順、sampling・停止規則、再利用prefix、初期KVその他の必要依存状態 |
| 元の実行主張 | 初回commitされたtrace・checkpoint・境界値、必要なopening、出力・生成長・仕事会計 |
| 取得・保持 | 内容rootとchunk索引、上限、取得先、非独占の代替供給、費用、期限、元claimに対する保持義務 |

プロンプト原文とcanonical token列の対応を保証するprofileでは、tokenizer・前処理もInitの検査対象にする。入力token列だけを認証して、公開された別の原文まで検査済みと主張しない。必要な状態・重み・openingを「モデル秘密」「内部API」として除外するprofileは、この公開検証profileの成功対象にしない。

Minerは、特定のモデル保有者名簿、私的な許可、受領者への個別推薦を取得・検査・再配布の条件にしない。同じ認証済みbytesを独立provider、cache、他の参加者が供給できる。公開の資料を再配布する権限と、第三者が新たな要求をcourtへ持ち込む有限の資格・費用・予約は区別する。

公開は全Nodeへの全量配布を意味しない。Panelは認証済みchunkを順次取得・検査し、許されたcacheを再利用できる。密なモデルの全体検証に必要な総読出し量、cold取得、全実行の検査範囲は残る。資料の一部を最初から持つ者だけで成立する供給計画を、モデル未保有の選出者への計画に流用しない。

## 3. Minerが署名する元の実行記録

以下は新しい署名対象の**論理schema**である。実wire名・tag・codecの割当を意味しない。
全欄はcanonicalな型・長さ・順序で符号化し、署名はチェーンと新しい形式にdomain分離する。

| 欄群 | 署名・認証する内容 |
| --- | --- |
| 形式と範囲 | chain scope、claim版、実行profile、規則版 |
| 当事者と仕事 | Minerのprincipal・bond・operatorの対応、元のjobとexecution identity、caseを導出する固定値 |
| モデル | 登録済みartifact／inventory root、program root、layout、tokenizer、使用するadapter等のroot |
| 入力 | 元の入力commit、prompt形式、入力長、再利用prefix・KV等の依存root |
| 実行規則 | dtype、丸め、演算規則、生成・停止規則、seed、selector、context／decode上限 |
| 出力 | 元の出力root、生成ID列のcommit、規則に必要な長さ |
| trace | trace layoutと版、canonical leaf数、step tree root、logits等の構成root |
| 全実行関係 | 許可されたkernel／local-checker版、関係schema／分解規則のdigest、初期・遷移・終了の記録への参照、宣言仕事単位とその導出規則 |
| 実行root | 上記の元contextとtrace構成を束縛するoriginal execution root |
| 依存関係 | 依存claim、元の生成範囲、creditと責任の条件 |
| 資料 | manifest root、codec、canonical対象空間、供給契約への参照 |
| 有限契約 | 義務者、対象範囲、取得・公開提出・保持期限、予約資源、費用・返還条件 |

受理原点DAAや実際の抽選recordはチェーンが決める。Minerの自己申告をその代わりにしない。
署名済みのcommitとチェーンの受理recordは、同じclaimに相互に結び付けて保存する。

**job-contextのdigestと、計算後のtraceを束縛するexecution rootは別物である。**
既存欄の名称がexecutionであるという理由だけで、中間値を固定済みと扱ってはならない。
同様に、Panelが後から作ったwitness rootを、Minerが署名したrootとして補完してはならない。

モデルrootは要求された登録物と一致する必要がある。結果を見た後のモデル・入力・規則の差替えを拒否する。
必要欄のzero値、未定義のroot、自己申告の成功flagを、認証済みの値へ読み替えない。

## 4. Canonical traceとrootの条件

同じ元job・モデル・入力・実行規則から、同じtraceを再構成できなければならない。
trace形式は少なくとも次を一意に定める。

- 記録対象の演算・checkpointと、leafのcanonicalな順序・総数。
- token位置、layer／node、tensor、range、shape、dtypeと値の符号化。
- 入力元・出力先・重み座標・履歴・状態遷移との接続。
- 境界値、非線形演算、丸め、logits、生成ID、停止・長さの扱い。
- dependencyから読み込む範囲と、creditに算入する仕事との対応。

各rootは元のjob context、登録program・artifact、layout、位置と型に束縛する。
同じ生の値でも、別claim・別位置・別dtypeのleafを流用できてはならない。
木のhashと、当事者の署名付き封筒のIDを分離し、Panelの署名者IDを元の計算contextへ差し替えない。
ハッシュとopeningが確認するのは同一性であり、rootだけでは計算の正しさや資料の取得可能性を確認できない。

入力から出力への接続を認証できない形式、必要な計算関係がtraceから脱落する形式は受理しない。
rootの一致を使う正常経路と、不一致を絞るnegative経路で、同じ元trace形式を使う。
異議を出された後に、別のcheckpoint配置・leaf数・表現へ変更してはならない。

### 4.1 全実行関係を有限の局所関係へ分解する

受理したoriginalを`C`、Minerのrootに認証可能な有界のraw記録集合を`T`とする。まず`WellFormed(C,T)`でcodec・型・長さ・構造を検査し、成立する場合に`T`を数学的に`S_0,...,S_n`という状態境界と、その間の局所記録として解釈する。これは既存のtyped TIR／bounded scan／versioned kernelの意味を記述するための記法であり、新しいVM、任意ISA、アップロードされた検証プログラムを導入するものではない。

`C`のモデル・program・入力・初期依存・数値規則・生成規則に対して、次の関係集合を一意に導出する。

```text
ValidTrace(C, T) = WellFormed(C, T)
                  AND Init(C, S_0)
                  AND every Step(C, i, S_i, S_(i+1)) for 0 <= i < n
                  AND End(C, S_n, declared_output, declared_length, declared_work)
```

`WellFormed`が偽なら`ValidTrace`は偽であり、存在しないdecoded stateの算術を評価しない。`WellFormed`、`Init`、`Step`、`End`は必要に応じてさらに有界な構成関係へ分解する。「every」は検査すべき関係の完全な意味定義であり、Nodeが全関係を日常的に再計算する指示ではない。全関係を調べる責務は[17](0017-noninteractive-positive-verification.md)、そのうち一つの違反を裁く責務は[19](0019-model-free-node-and-bounded-evidence.md)に置く。

| 必須関係 | 元のclaimに結び付けて閉じる内容 |
| --- | --- |
| WellFormed | 元rootに認証されたrecordのcanonical codec、位置に対応するrecord種別、dtype、shape、長さ・値域、padding・木構造・参照の規則。外側の証拠packetの妥当性とは分離する |
| Init | 登録model／adapter／program、prompt・入力長、tokenizer等の規定された入力変換、seedと乱数初期状態、prefix／KV／外部依存の生成元・版・受理条件、初期tensor・stateの対応 |
| Stepの定義 | canonicalな位置、許可kernel、shape、dtype、読み書き先、還元順序、丸め・飽和・overflow・FMAの扱い。算術値だけでなく、当該位置でその演算が要求されること |
| Stepの読み出し | 各operandが固定input／weight／正規の先行producerまたは直前stateのどの位置を読むか。別の過去state、別token、別layer、別claimへの差替えを認めない |
| Stepの書き込み | 変更対象とその値、変更前後のstate commitment、変更対象外の保存。許可されていないstateを同時に変更していないこと |
| 選択と進行 | bounded scanの位置、固定graph内の次の関係、routing／index／mask／branchに相当する許可された選択、attention範囲、KV読書き、乱数counterとsamplingの進行 |
| End | 正規の停止条件・終了位置、生成ID列と宣言output rootの一致、長さ、出力符号化、未実行suffixの不存在、仕事単位と既存prefix／依存再利用の二重計上禁止 |

初期・終端・接続・state更新を「算術ではない」として対象外にしてはならない。すべての乗算が正しくても、違う重みを読んだ、途中のlayerを飛ばした、出力列を差し替えた、仕事量を増量したclaimは`ValidTrace`を満たさない。

関係の対象空間、必要なrecord、長さ、木の形、位置は、許可されたprogramと固定jobから導出する。宣言された`n`やleaf数もその規則で検査し、任意の短いprefixを完了traceにしない。データ依存の合法な停止は`End`で停止規則・生成長・出力を検査する。trace rootを固定した事実自体は、これらの関係の成立を意味しない。

### 4.2 原claimから局所証拠を取り出せること

各関係について、必要なrecordと認証経路を取り出す決定論的な方法をprofileへ固定する。証拠は、元Minerの署名root、固定programの演算定義、値の型と位置、読み書きの接続を[19](0019-model-free-node-and-bounded-evidence.md)の上限内で認証できなければならない。

trace layoutは、内側の数値recordのdecodeに成功することを前提とせず、有界なraw bytesとその位置・長さ・kindの認証封筒を元rootへ照合できる形式にする。hashのcontextは元job・program・layoutに固定し、後から生成されるtrace root自身を循環参照しない。外側の証拠封筒・認証経路は正規で、内側の元Miner recordだけが型・codec・構造規則に違反すると確認できた場合、それは`WellFormed`の局所反証である。Nodeがdecodeエラーを単に申立人の無効packetとして捨ててはならない。

ただしraw recordの外形すら上限内で認証できない、openingを得られない、内容が未開示である場合には、この区分を捏造しない。原claimの宣言サイズ・schemaを受理時に検査し、後から開示できない部分は適格要求・提供義務・成功停止／未検証終了へ進める。上限外bytesを読んでから判定する例外は置かない。

immutableな重み・入力は元のartifact／input rootを読み、mutableなstateは、その位置の直前state rootへのopeningと許可された更新から次state rootを認証する。更新された要素だけを示して残りのstateを自由に差し替える形式は不可である。固定DAGのtensorを使う形式では、各readをcanonicalな先行producer出力へ結び付ける。同じ数値がどこかのleafに含まれるだけでは、正しいread-from関係を示さない。

大きな初期stateや出力列をNodeが全量走査する形式にしない。固定input・artifactへの参照、初期化の分解記録、または許可された認証更新で初期境界を局所検査可能にする。生成列・work accumulatorは各stepで正規に更新し、終端で宣言root・長さ・仕事単位と結ぶ。rootの更新則を検査せず、提出者の新rootを採用してはならない。

正直な検証者が必要資料を取得でき、`T`が認証可能な記録として開示される場合、誤った`T`には[19](0019-model-free-node-and-bounded-evidence.md)の局所完全性条件に従う反証を構成できる。単なるhash値だけが提出され、対応recordが開示されない場合は、存在しないrecordから算術反証を作ったことにしない。[18](0018-interactive-negative-court-and-defaults.md)の適格要求・提供義務と[20](0020-verification-gated-final-and-adversarial-models.md)の成功停止／未検証終了へ接続する。

### 4.3 巨大演算の分解を初回commitに含める

内積や行列積の一要素であっても、その依存範囲がNodeの上限を超える場合は、許可された還元順序で積・部分和・境界の記録を初回commitに含める。例えば`p_j = w_j * x_j`と`s_j = add_round(s_(j-1), p_j)`を使うprofileは、両演算のdtype、幅、overflow、丸め、最終変換を規則に従って固定する。浮動小数のFMAを、丸めの異なる乗算と加算へ勝手に置き換えない。

softmax・正規化・argmax・attention・MoE等も、対象kernelの全関係を同様に分解し、非線形関数や全体還元を未検査のoracleとして残さない。分解は[02](0002-tensor-ir-and-shared-query-data.md)の許可済み意味と同値でなければならない。上限内に分解できないkernel／shape／長さのclaimは受理しない。

Minerが巨大演算の入出力だけを固定した後で、Panelが新しい部分和や内部traceを作り、元Minerの署名済みrecordとして使うことを禁止する。後から追加したsubtraceに対するPanelの署名・Minerの別署名も、元claimの初回commitに代わらない。trace生成・hash・保持の増加はMiner／provider／Panel側の資源計画に計上し、Nodeの軽さから全体費用の小ささを推定しない。

整数内積を上限付き区間へ分ける場合、例えば最大B項の区間ごとの前後の部分和を初回commitに含める。B、各演算の幅・overflow・丸め、重みと入力の座標、境界の接続をprofileへ固定し、区間内の全関係を19の上限内で再計算できなければならない。途中で丸め・飽和する元の演算を無限精度の総和へ置き換えない。Bを大きくすればNodeの一件の負荷、小さくすればtrace境界・hash・I/Oが増えるため、最低要件Nodeでの測定を経て具体値を採用する。checkpoint間に巨大な未分解演算を残すことは、この区間方式の代用にならない。

### 4.4 構成規則と採用対象

全関係の局所裁定可能性は、Minerやprofile作者の`complete=true`という申告で認めない。プロトコルが許可したkernel版、canonical関係schema、分解・組立規則、局所checker、局所化手順、対象範囲と最悪上限の組合せを使う。任意の登録programについては、これらの構成規則に適合する範囲だけが本profileのclaimを出せる。

許可schemaから外れる演算や、関係の抜けを残す融合kernelは、別のversioned kernelと検査器の採用・実装・測定・有効化まで`UnsupportedProfile`とする。これはclaim表現の裁定可能性の検査であり、モデルの品質、公開状況、保有者数を審査してtierを変更する制度ではない。

「すべての関係が局所的に裁ける」と「Nodeがすべてを検査済み」は別である。前者は本節のprofile条件、後者の全体検証は17のPanel責務である。サンプリングした関係集合を、`ValidTrace`の全関係集合へ読み替えてはならない。

## 5. 一回の推論から資料を生成する

新profileのMinerは、正規の実行規則に従う元の推論中に必要値を記録し、traceを構成する。
記録済みの値からhash・opening・要求対象の索引を作れる構成を基準とする。
positiveのためだけに追加の全推論を必須にする契約にはしない。
ただし記録、hash、Merkle tree、I/O、保持、局所資料組立ての費用がゼロになるとは扱わない。

root固定後に値を修復したり、別の数値規則で得た結果を無断で正規traceへ変換したりしてはならない。
元の実行と供給するtraceが一致しないときは、新しい署名で旧claimを上書きせず、失敗・終了規則に従う。
正しいキャッシュや再利用を使える範囲は、既存のjob・dependency・仕事量規則で決める。
この記録は、Miner本人のGPU利用履歴、電力消費、実消費FLOPsを証明するものではない。

## 6. 資料manifestと内容認証

manifestは、claimで必要になり得る資料の位置・形式・供給契約を示す認証付き索引とする。
必要範囲は登録programと固定されたjobから導出し、Minerが列挙した都合のよい資料だけに限定しない。

| 対象 | 取得時に確認すること |
| --- | --- |
| モデルの重み・adapter | 登録artifactと正規の座標・rangeへのopening |
| プログラム・演算定義 | program root、規則版、型・形状・接続の一致 |
| 入力・prefix | 元のinput／dependency root、形式、範囲、生成元の一致 |
| trace・checkpoint | Minerのoriginal execution rootへのopeningとcanonicalな位置 |
| 出力・生成ID | Minerのoutput／traceとjob自身の生成・停止規則への結び付き |

manifestには対象ID、chunk境界、canonical codec、認証経路、サイズ上限、供給者と保持契約を含める。
manifest自体が大きい場合は認証付きの索引を使い、Nodeに全索引の常駐を要求しない。
圧縮資料は展開後の長さ・CPU・メモリも計上し、不正な長さや展開量を拒否する。
URL、provider署名、root、サイズ申告のいずれも、内容取得・認証の代わりにはならない。

manifestと必要範囲の導出情報は、モデルを未保有の選出者にも読める入口を持つ。重みを既に知っていなければ資料IDや合法rangeを特定できない循環を作らない。モデルroot単位の不変資料と、claimごとに変わる入力・依存状態・traceを区別し、cacheの再利用で別claimの検証完了や仕事creditを再利用しない。

## 7. 有限の資料供給義務

Minerは、少なくとも次の事項を元claimの契約に固定する。
providerに実作業を委ねる場合も、Minerのclaimが検証不能なら成功できない点は変わらない。
個々の金融責任は、実際に署名し受理された役割別の契約から判定する。

1. 義務者と対象claim、応答対象、利用できる代理・relay・keeperの権限。
2. 正規要求の対象空間、件数・byte・work・課金の累積上限。
3. 取得、公開要求、提出、保持、再取得のそれぞれの期限。
4. 正規の提出先、認証条件、完了条件、重複要求のまとめ方。
5. 予約された提出機会と、その機会が得られない場合の終了・返還規則。

時計は元の受理点を起点とし、再送、別Panel、別provider、caseの作り直しで再開しない。
同じ対象の認証済み内容は共有・再利用できる。資料を提出した者に排他的な所有権を与えない。
共有化によって、当事者ごとのnotice、本人の提出機会、負担者、未解決責任まで消してはならない。
要求の実体と課金対象はcanonicalなIDで重複排除し、key・relay・queueの変更で予算を再生させない。

## 8. 正常配信と公開要求

正常時はMiner／providerが固定資料を先に配信し、Panelは追加の計算内容の問答なしに17を実行できる。
認証済みcacheの再利用や固定ファイルの取得は許す。通信を伴うことと、対話型の計算裁定を区別する。
初めからモデルを持つPanelだけに仕事を回すこと、取得できない選出者を無料で再抽選することを認めない。

選出前の事前保有を不要にするため、受理計画は「未保有の選出者が入口manifestを得る→必要資料を認証取得する→全体を検査する→承認または適格反証を提出する」の全工程を含む。並列取得・先読み・既存cacheは実際に成立する範囲の最適化とし、未保証のcache hitをcold取得時間から控除しない。

適格なpublic bonded verifierにも、同じ元claimの資料を要求・取得・検査・公開提出する経路を残す。
Minerや選出Panelの許可、秘密のモデル所有者名簿、専用APIの私的認証だけに依存させない。
選択的に味方へだけ配信することを、公開の供給義務の履行として扱わない。
正規の公開要求は有限の対象・費用・bond・期限・予約枠に従い、無制限な取得申告に永久vetoを与えない。

資料が届かない場合、NodeがモデルURLへ接続して代行取得する経路を設けない。
Nodeが読むのは、受理された契約、適格要求、提出機会、公開された認証付き応答と終了事実である。
private RPCのtimeoutだけを、Minerの計算不正または供給defaultの確定証拠にしてはならない。
qualified defaultの成立要件は18、未検証終了とFinalの条件は20に従う。

取得不能の選出者は、算術反証を先に完成させることなく18のMaterialRequestを開始できる。必要資料の到着後もInspectionPendingとして同じcaseを継続し、原時計内の全体検査・反証の機会を残す。Minerまたは味方のPanelによる「公開済み」「受領済み」という署名だけで、その人の未解決caseを閉じてはならない。供給の最遅時刻から必要検査・証拠作成・包含・裁定を収容できなければ、成功を許さない。

## 9. 資料の状態と成功条件

| 観測された状態 | 許される処理 |
| --- | --- |
| manifest／rootだけがある | 固定内容の識別。取得・算術検証・Finalは未成立 |
| 必要資料が届き、認証が成立 | Panelが検査へ進む。これだけで肯定承認は出さない |
| 所定の独立検証が成立 | 17の承認候補を作る。成功Finalは20が判定する |
| 必要資料が不足、取得不能、期限超過 | 肯定承認を出さず、規定の未検証・Abort経路へ進む |
| Minerの元traceに認証済み矛盾がある | 18のnegative／局所裁定へ進む |
| 有限の供給義務に帰責可能な不履行がある | 計算不正と区別して18で裁定する |

「資料がないため不正の反証を作れない」ことを、成功を許す理由にしてはならない。
もっとも、全選出検証者の共謀をNodeが署名から検出できるとはしない。20の認証前提を省略しない。
供給契約への同意や資料の受領証を、独立再実行の承認と合算してはならない。

## 10. 資源・保持・費用

受付前に、coldな選出Panelと第三者による取得・検査・最遅の異議・公開搬送・回復を見積もる。
一部の正直な参加者が既にモデルを持つ場合だけの費用を、必要資源の上限として使わない。
密なモデルの完全再実行で全重みが必要なら、その総転送量とPanel側の保存・実行費用を計上する。
Nodeの局所計算が小さいことから、モデル転送・Panelの全体検証・trace保持も小さいとは推定しない。

保持対象にはnegativeの開示と、Nodeが後から裁定を再検査するための局所資料を含める。
保持期限は供給・収集期限と区別し、収集窓を延長することで代用しない。
費用の原資・上限・部分払い・返還・徴収可能な責任は[11](0011-bond-budgets-fees-and-payments.md)と18／20へ接続する。
無制限の新規保管義務、モデル単位のSeeder資格・報酬、未同意のprovider課金を追加しない。

全モデルの資料保管、Panelの全体検証、Nodeの局所裁定の容量は別々に計上する。20の任意の有限batchを使う場合も、batch固定前の全leafの入力・出力・traceを束縛し、選出後にその全範囲を検査できる取得・計算・保持費を負担する。モデルの再取得・loadを共有したことを、異なる入力の再実行や反証資料生成が不要になった根拠にしない。

## 11. 互換性と実装接続

旧[RFC-0044](../rfc/0044-positive-court-full-coverage.md)のR2は、Minerの結果commitとPanelのwitness commitを分けた。
本書は新profileに限り、Minerがoriginal traceを署名固定する責任を追加する。
旧R2の「Minerに全trace構築を課さない」という条項を、既存claimへ遡って変更しない。
旧署名、旧PositiveChecked、旧の結果commitを、新しい意味で有効とみなしてはならない。

実装時にはMinerResultCommitのjob-context identityと、実際のtraceを束縛するrootを別欄として認証する。
capture_one_pass_v2等の記録処理や既存のparameter／trace openingを再利用しても、本書のnative接続が完成したとは扱わない。
新版のcodec・署名・入出力binding・保持・供給default・再起動の接続を確認してから、有効化判断へ進む。
公開裁定の元時計と認証開始点は20で一体に定義し、本書だけで期限やfenceを設定しない。

## 12. 必須の受入確認

以下は必要な検査項目であり、今回その実行成功を宣言する一覧ではない。

| ID | ケース | 必須結果 |
| --- | --- | --- |
| M16-01 | モデル未保有のPanelが選出される | 認証付き資料を取得して検証できる。事前保有者への再抽選をしない |
| M16-02 | model／input／job／traceのいずれかを差し替える | 元claimへのbindingで拒否する |
| M16-03 | 同じ値を別の位置・型・claimで使う | 認証またはcanonical形式の検査で拒否する |
| M16-04 | job-context IDだけでtrace固定済みと主張する | original trace未固定として拒否する |
| M16-05 | rootはあるが必要な資料がない | positive・成功Finalを成立させない |
| M16-06 | 味方のPanelにだけモデルを渡す | 適格な公開要求を閉じず、未履行の処理を維持する |
| M16-07 | Panelが停止／受領証を握り潰す | 非独占の正規提出と本人の提出機会を区別して処理する |
| M16-08 | private RPC timeout／収録機会なし | 自動的に計算有罪・供給defaultにしない |
| M16-09 | 同一要求を別key・relayで増殖させる | 元の累積資源・費用上限を超えない |
| M16-10 | 圧縮資料、巨大manifest、過大chunk | 展開後を含む上限で拒否し、Nodeのモデル取得へ移らない |
| M16-11 | restart／reorg／供給者交代 | 元のcommit・義務・期限・未解決責任を復元する |
| M16-12 | 旧署名・旧形式を新claimへ持ち込む | 旧規則でのみ読むか拒否し、新規の義務同意に転用しない |
| M16-13 | 全算術は整合するが、初期input／prefix／seedの一つが異なる | Init違反を元claimに結び付けて局所反証できる |
| M16-14 | 同じ値を含む別の重み座標、過去state、別layerの出力を読む | membershipだけで通さずread-from／接続違反を裁定する |
| M16-15 | 更新対象の値だけ正しくし、他のstateを同時に改変 | 認証更新で不変領域の保存を検査し、改変を反証する |
| M16-16 | 正しいprefixでtraceを切り、停止・出力・長さ・仕事量を偽る | Endまたは進行関係に違反し、完了claimとして成功させない |
| M16-17 | 巨大GEMMを入出力だけcommitし、異議後にPanelが部分和を追加 | 初回commitの不足として当該profileを受理しない。後付けtraceをMiner反証に使わない |
| M16-18 | fused kernelの丸め、RNG counter、mask、routingを一つ変更 | 許可kernel／固定選択・進行規則への違反を局所化できる |
| M16-19 | 関係schemaからEnd／state更新／仕事会計を除きcompleteを自己申告 | profileの構成規則不成立として受理しない |
| M16-20 | 宣言rootに対応するrecordを全供給者が隠す | 架空の算術反証を作らず、適格要求と成功停止／有限終了へ進む |
| M16-21 | 上限内で元rootに認証されたrecordだけが不正codec／dtype／長さを持つ | 証拠封筒の不正と分け、WellFormed関係の反証として裁ける |
| M16-22 | プロンプトだけ公開し、必要重み・tokenizer・初期KV・RNG規則の一つを秘匿 | 公開検証パッケージ未成立。適格要求を受け付け、成功を止める |
| M16-23 | 唯一の正直な選出者がモデルを一度も保有していない | 元のmanifestから対象を特定し、cold取得・全体検査・承認または反証まで元期限内で進める |
| M16-24 | Minerが味方だけに配信し、正直な選出者は算術反証をまだ作れない | 算術反証を資料要求の前提にせず、本人のQと予約を維持する |
| M16-25 | 独立providerが同じ認証済みchunkを再配布し、Minerが個別許可を拒否 | 内容と元契約に適合する供給を認め、所有者の裁量的な許可を待たない |
| M16-26 | 原文promptと実際のtoken列が異なるが両方を公開 | 固定tokenizer・前処理のInit関係で対応を確認し、公開だけで正当化しない |
| M16-27 | cache missと最遅供給が重なり、全体検査の時間が残らない | cache hitを仮定して成功させず、帰責を分けた回復または未検証終了 |
| M16-28 | 区間traceのBを大きくしてNode上限を超える、または部分和を後付けする | profileを受理せず、上限と初回commitを維持する |

未測定のbyte・CPU・RAM・保持費・wall timeはUNKNOWNとして残す。
数値未確定のprofileを「実装で適当に補う」ことは認めず、資源・契約・移行の具体化を有効化前に完了させる。
