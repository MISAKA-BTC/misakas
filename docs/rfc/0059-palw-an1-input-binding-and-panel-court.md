# RFC-0059 — PALW-AN1：実入力への試行束縛とPanel＋Court

<!-- MAS1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MAS1の構造・実用性検査とBFT安全停止

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0065](0065-model-admission-screening-and-bft-stop.md)／[ADR0233](../adr/0233-model-admission-screening-and-bft-stop.md)／[MAS1適用・移行](../governance/model-admission-safety-transition.md)を、この新規採掘profileの追加正本とする。AN1の入力束縛・Panel＋Courtを維持し、新規MAS1採掘のmodel資格をRFC0065へ接続する。

構造検査はPASS/FAIL/UNKNOWNを区別し、UNKNOWNは採掘不可のままBFT reviewへ接続する。G-EXEC/G-COLL/G-COSTを維持し、公開・共通の実用性G-USEと停止状態をAND条件へ追加する。BFTはモデル/ruleの停止と条件付き解除に限り、未知の費用定理の承認・台帳finality・fork choice・reorgの権限を持たない。burnやowner変更で安全停止を解除しない。

旧「モデル品質・モデル単位の検査を一切しない」という文言は、MAS1の採掘資格に関する今回の限定検査へ置換する。一般登録とMOE2市場原子生成、通常取引、coreの経済配分、Panel＋Court、計算validity proof不使用、旧claimの元責任は維持する。旧採択ブロック・原文・実測はその版の履歴であり、今回の数学的証明・native実装・公開activationが完成したという意味ではない。
<!-- MAS1-ADOPTED-20261120-END -->

<!-- FOW1-TB1-20261120-BEGIN -->
## 現行の適用 — FOW1・TB1への限定的置換

2026-11-20のユーザー明示指示による新規採掘の設計正本は、[RFC-0062 PALW-FOW1](0062-palw-function-output-and-work-admissibility.md)／[ADR-0230](../adr/0230-palw-function-output-and-work-admissibility.md)と、乱数正本の[RFC-0061 PALW-TB1](0061-palw-internal-threshold-beacon.md)／[ADR-0229](../adr/0229-palw-internal-threshold-beacon.md)である。状態は**ADOPTED_DESIGN_NOT_ACTIVATED**。以下のAN1独立familyの原仕様・採択前記録は旧草案として保持する。

FOW1はAN1の実入力・完全trace・Panel＋Courtを継承するが、推論関数自身の採掘出力`z`、関数非衝突性と複数試行費用の必須gate、四Root Ticketを追加する。本書§3.2の「異なる出力を要求しない」は自然回答`y`及び旧AN1の保証範囲であり、FOW1の`z`へ適用しない。AN1適合だけでFOW1の新規採掘資格は得られない。

新profileの実行・Panel乱数は独立秘密を持つTB1 EXEC/PANELへ固定し、旧beacon・任意scheme・公開済みseedの再hashを代替にしない。0062のactivation A以後の全新規PALW採掘入口はFOW1＋TB1を必要とし、旧AN1受付をfallbackとして残さない。旧受理claimは受理時の版と固定されたdrain条件で処理し、過去の署名・元期限・責任を保存する。具体的な反復費用検査と適合モデルは未完成であり、文書変更は現在runtimeの停止・公開activationを意味しない。
<!-- FOW1-TB1-20261120-END -->

- 日付: 2026-11-18（Asia/Tokyo）
- 状態: **DRAFT_NOT_ACTIVATED**
- 対応ADR: [ADR-0227](../adr/0227-palw-an1-input-binding-and-panel-court.md)
- 接続: [RFC-0056](0056-global-claim-admission-difficulty.md) §17、[RFC-0057](0057-court-nonce-and-panel-challenges.md) §17、[RFC-0058](0058-staged-work-commitments-and-consensus-eligibility.md) §16
- 容量契約: [RFC-0055](0055-global-admission-court-calendar-and-one-bps.md)
- 改訂記録: [README](../maintenance/revisions/2026-11-18-an1/README.md)、[検証](../maintenance/revisions/2026-11-18-an1/VALIDATION.md)

## 0. 適用範囲と方針

2026-11-18時点の草案は、whitepaperとRFCの改訂方式を **PALW-AN1、Panel＋Court** に統一する提案であった。現在の新規採掘規範は冒頭の0061/0062へ置換する。Groth16その他の計算validity proofを導入しない。Panelは元claimの実行全体を再現・比較し、Nodeは認証された局所関係と状態遷移を裁定する。[PRINCIPLES](../PRINCIPLES.md)の計算証明不使用、モデル非保有Node、単独反証、一般モデル登録・1.0/0.3を維持する。FOW1の共通数学的適合条件は一般モデル登録とは別の採掘条件である。

以下の「必須」「拒否する」「してはならない」は、旧**AN1 family**の提案内容と、0062が明示的に継承する共通契約を記録する。新規採掘の追加条件・置換・受付終了は0062 §9を優先する。旧CAD1/CPN1/SWC1のwire、既存署名、旧claim、過去検証、採択済み経済条件を遡及変更しない。文書の作成はnative実装・本番測定・activationではない。

AN1は次を目標とする。

1. owner・nonceを、実際にモデルが読む入力tokenへ結合する。
2. 一つの登録試行に、一つの正しい実行・Ticketを対応させる。
3. 元claim・Courtの別試行への直接付け替えを拒否する。
4. 一人の能動的な正直Panelに取得・完全再実行・反証の包含と裁定機会がある条件で、不正成功Finalを阻止する。

通常LLMと同額以上の実費、最適化・cache・外注の排除、全員悪意のPanelに対する無条件正当性、DAG安全性の完成は主張しない。

## 1. 入力資料と優先関係

設計根拠は、ユーザーが提示したAN1説明全8節、今回作成するAN1 whitepaper改訂稿、および添付RFC一式である。ユーザーがリンクしたAN1 HTML/ZIPは未取得であり、報告された11テスト群・7,226レコードを実測値として再掲しない。[独立参照実装](../maintenance/revisions/2026-11-18-an1/reference_impl/README.md)は別実装である。

従来のFULL1/VF1統合補完稿からは、正当な高速化と算術不正の区別、費用境界、前処理・共有計算の扱いを引き継ぐ。VF1のproof必須Final、setup、検証鍵、AoKによる不正Root排除はAN1へ採用しない。旧FULL1の固定入力による費用定理をAN1へ転用しない。

## 2. profileと登録の型

### 2.1 固定するprofile

FOW1で継承する際は、以下のbeacon項目をTB1 scheme/suite、role epoch、EXEC/PANEL RoundRef、roster root、公開scheduleへ具体化し、QualRootと閉鎖集合のFreezeRecordを別native recordとして保持する。0061のPREPARED、全Q、share certificate、曝露・no fallback条件を全consumerに適用する。

AN1 profileは、familyと版、model/program認証、実行意味論、token表、trace schema、全local checker、Merkle規則、署名・beacon、TimingPlan、受付target、会計・移行を一体として識別する。profile digestはこれらのcanonical bytesへ結合する。

必須の未設定項目、未対応opcode、曖昧な整数幅、未指定の端数処理、自由なpadding、無制限のlocal checkerを残したprofileを有効化してはならない。番号・native codec・本番パラメーターは本RFCの文書番号から自動割当しない。

### 2.2 登録文脈C

`C`は、少なくとも次の項目を順序・型・長さを固定して符号化したものとする。

| 群 | 必須項目 |
| --- | --- |
| chain | chain_id、family/version、epoch、登録anchor、schedule_id |
| 主体 | canonical owner、beneficiary、登録権限 |
| 仕事 | model_root、program/profile digest、job_root、決定論的生成条件、入力・出力・trace上限 |
| 試行 | 規定幅nonce、候補集合識別、beacon scheme/key/round、選出予定 |
| 当選 | difficulty_snapshot_id、唯一のtarget、受付開始・終了 |
| 会計・検証 | accounting_version、用途、資料義務、完全検査scope、資源予算、移行規則 |

ownerの表示文字列、tokenizerの未固定正規化、可変長の曖昧な連結を識別子として使わない。登録時に未知の実beacon、trace/output root、Panel結果をCへ入れない。実input rootもCの後に定まるため、Cへ循環的に要求しない。

候補集合識別は事前割当の非循環IDとする。C/RegKeyをleafに含むfreeze集合Rootはnativeの閉鎖記録に別途保持し、そのRootを同じCへ逆流させない。

### 2.3 slot・試行・元claim・有用仕事の分離

`RegKey = H_reg(Enc(C))`をnativeの正規登録キーとし、一つのRegKeyに一つの`slot_id`だけを原子的に割り当てる。資金transaction、carrier、再送、再署名を変更して同一Cに別slotを発行してはならない。割当後のslot_idをCへ戻さず、Cと資金・slotの管理envelopeを分離する。slotはCに自由な乱数を追加するための欄ではない。

登録後の実beaconを`σ`として、

$$s=H_{\mathrm{attempt}}(\operatorname{Enc}(C,\sigma)).$$

を試行IDとする。`RegKey → slot_id → s`の対応をnativeに保持する。成功消費にはslotだけでなく`AttemptUse[s]`と実計算キーの一回性も含め、import/再登録/別経済封筒での別名化を拒否する。

`OriginalId`は署名前の正規な元claim本文を識別する。署名bytes、Panel nonce、証拠bytes、コメントはOriginalId/Ticket/消費キーを変える自由度にしない。

`BaseWorkKey`は元の需要・model/job/計算意味論を識別し、採掘試行IDと分離する。有用成果の単発creditを要求する用途では、BaseWorkKeyと用途の消費も同じ原子遷移へ含める。別nonce・owner・epochだけで新しい有用需要とみなさない。意味的に等価な任意programを識別できるとまでは主張せず、登録済みcanonical表現の範囲を規定する。

## 3. 実入力の導出

### 3.1 byteからtokenへの単射

profileは256種類のbyteを、256個の相異なる有効token IDへ写す表`ByteToken[0..255]`と、BOS/POW/SEPのIDを固定する。三つの特殊tokenは互いに異なり、byte tokenと重複しない。modelの実embeddingが対応する有効indexを持つことを要求する。

$$x=[\mathrm{BOS},\mathrm{POW}]\Vert[\mathrm{ByteToken}[s_0],\ldots,\mathrm{ByteToken}[s_{31}]]\Vert[\mathrm{SEP}]\Vert\mathrm{JobTokens}_P(J).$$

長さは`35 + len(job_tokens)`。challenge文字列の再token化、UNKへの縮約、自動切詰め、challengeを消すchat template、実行時だけの別token列を禁止する。context上限超過は登録拒否または規定済み未成功終端とし、同じ試行のままchallengeを落として続行しない。

job token化・template・attention mask・position・生成条件を固定し、modelが実際に読んだtokenをtraceの入力関係へ結合する。メタデータ上のnonceだけではAN1を満たさない。

### 3.2 意味論と最適化

旧AN1では、異なるsは異なる正規token列を与えるが、異なる出力、異なる全内部値、通常推論と同じ実費は要求・保証しなかった。**新FOW1ではこの限界を採掘資格として受け入れず、コア採掘出力zの非衝突性と複数試行費用を0062 §4の必須gateにする。** 自然回答yや全中間値が必ず違うことは要求しない。challenge prefixによって元job単独の場合と回答が変わり得るため、サービス契約と利用者への表示は実際の実行対象を記す。

cache、batch、正当な簡約、外注による正しい最適化自体をslash違反にしない。FOW1の採掘資格では、これらの共有・償却を反復費用検査の範囲に含め、未検査の経路を明示する。

## 4. 時系列と二つのfreeze

以下の順序はFOW1にも継承するが、実行用はTB1 EXEC、選出用は別秘密のTB1 PANELに限定する。双方を事前にPREPAREDとし、freezeは各roundの最初の正直share放出前にnative安定条件を満たす。certificate完成を未知性の境界にしない。旧64byte native beaconをTB1 seedへ暗黙変換しない。

1. 資金付き登録と全nonce候補・model/job/profile/target/accountingを、実行用beaconへの攻撃者の最初のアクセスより前に閉じる。
2. 登録した鍵・scheme・roundのσを認証し、sとxを導出する。早期漏洩・偏り・停止・別roundへの再試行を別のbeacon契約で扱う。
3. Minerは規定実行から旧AN1のy/trace/三Root/Ticketを作る。FOW1では0062のy/z/trace/四Root/Ticketへ置換する。候補を多数事前登録し一部だけ計算する行動は残る。
4. ownerの署名済み元claimを一回だけ固定し、元claim集合とPanel選出poolを閉じる。
5. その閉鎖後の選出イベントでPanelを選ぶ。Panel再実行を見てから元Rootを置換・補完してはならない。
6. 元期限・元予約で完全検査、資料case、Court、成熟、成功または非成功終端を処理する。

単に同じ既知seedを別domainでhashしただけでは、二つ目のfreezeに必要な未知性を得ない。別候補を含む全ての適応的選別を対象に、抽選pool・seed・偏り・同一登録の再試行を評価する。

## 5. Ticketと旧受付方式の分離

以下は**旧AN1の三Root Ticket**である。FOW1は0062 §5の`R_z`を加えた四Rootと別domainを使う。同じwire/署名を暗黙に新しい式へ読み替えない。

$$T=H_{\mathrm{ticket}}(\operatorname{Enc}(s,R_x,R_y,R_\tau)).$$

Ticket入力へproof/署名の乱数、Panelの選出結果、Court nonce、任意metadataを追加しない。旧SWC1の未来`R_ticket`をAN1へ重ねず、既存CAD1のcodecを意味変更しない。

nativeはCから固定したD snapshotを使う。CAD1の受理条件が`T <= floor((2^256−1)/D)`であるとき、AN1は

$$\theta=\left\lfloor(2^{256}-1)/D\right\rfloor+1,\qquad T<\theta$$

として同じ境界を得る。`D=1`では`θ=2^256`を広い整数で表す。targetを後のepochへ持ち越し、当選結果を見て易しいものへ選び直さない。共通controllerを一回適用し、モデル別の恣意的Dや追加の独立抽選を作らない。

正しいTicket入力は固定試行に一つであるが、偽Rootへの安いhash試行は可能である。Dだけで検証前DoSを防いだと扱わず、登録・候補数・資金・bytes・予約を別に適用する。候補順位と受付集合のtie-breakは0056§17に従う。

## 6. 実行意味論・trace・木

### 6.1 全関係

実行を`Exec_(M,P)(x)=(y,τ)`とし、命令、重み配置、整数幅・符号、量子化、丸め/飽和/overflow、非線形処理、attention、生成長・EOS・tie-break、RNGを固定する。環境依存の未指定kernel結果を許さない。

traceは初期入力から最終出力までの全必要関係を持つ。recordの命令位置、型、全参照辺、token読出し、weight index、状態遷移、Init/End、output結合を含める。算術の正しさだけでは別の入力・重み・命令の選択を正当化しない。

全関係と境界が成立すれば、命令順の帰納法で正規実行に一致する。不一致なら最初の相違点が局所反証になるためには、profile/compilerの全関係展開が完全であることが必要である。局所checkerのない処理を成功対象へ入れない。

### 6.2 有界なNode checker

命令のfan-in、整数幅、参照数、opening長、最大証拠bytes、checkerの最悪処理量を公開上限へ収める。大きなGEMMや全attentionを無制限の一関係として隠さない。必要な基本関係へ展開する。

Nodeは全モデルを保持しないが、正規登録・program/profile・元claim・型規則・期待Rootを認証する。具体的な局所witnessは、全モデルの暗号学的validity proofではない。

### 6.3 canonical commitment

入力・出力・traceの各木は異なるdomainを持ち、全leaf・padding・内部nodeへs、木種別、論理長、位置または高さ・範囲を結合する。左右の順序、record順、値/座標/長さの符号化、空領域を一意にする。

本参照shapeはinput/output/traceの論理長Dを1以上とし、空出力を受理しない。空shapeを追加する場合はcanonical empty rootと終端規則を別版で定義する。

AN1参照形は`L=2^ceil(log2 D)`へのpaddingを使い、各paddingの期待bytesを固定して個別にhashする。任意paddingで別の正しいRootを作れない。既存FULL1のpaddingなしexact-D木とは別profileである。実装が別の木を選ぶならprofile・Root・checker・費用計数・vectorを一体で版更新する。

native受付とCourtは、論理長・件数・index・整数値の同じ厳密な型/range規則を使う。数値が等しいからといって`6153.0`を整数6153として受理したり、boolを整数へ暗黙変換したりしてはならない。受付は通るが全openingが型不一致で拒否されるclaimを作らない。

model/program rootは試行前のcommitmentなので未知のsを入れない。Court contextが期待する固定Rootとsを結合する。入力Rootを登録Cへ逆流させない。

## 7. 元claimと認証済み資料

元claimはslot_id、RegKey、s、model/profile/jobの識別、三Root、論理長、生成条件、TimingPlan、責任・帰属・会計版を結合する。nativeが再導出できる値をpayloadから自由に採用しない。

Nodeが期待するinput rootは登録C・σ・正規job tokenから導出する。長いjobの全tokenをNodeが毎回保持・hashできないprofileでは、有限の入力結合関係・認証されたjob opening・完全Panel再実行・資料caseにより同じ意味を保証する別のadapterを規定しなければならない。本改訂の具体参照profileはNodeが直接再計算できる予算内の入力長に限定する。未実装adapterを理由にpayloadのRxを正規input rootとして採用してはならない。未実装の小入力用処理を任意長入力へ拡大しない。

Panelと資料loaderは、供給されたmodel/program/traceの実bytesと正規形からRootを再計算する。任意オブジェクトの`.root`属性、relayの`verified`flag、元claimと無関係なcacheを信用しない。正規model rootの重みと実際に実行した重みが一致する必要がある。

正規実行Rootと元claim Rootが異なることだけで、モデル非保有Nodeへ全体の不正を証明したとは扱わない。Minerがcommitした元traceの認証済みrecordを取得し局所反証を作る。取得できなければ資料caseを通じて成功を止める。資料の欠落を架空の算術反証で埋めない。

## 8. Courtと結果の分類

### 8.1 nativeからの期待値

Courtはclaimを自由に選ぶRootを持ち込む入口ではない。native元claimから期待s/owner/profile/Rx/Ry/Rtraceを取得し、認証済み命令・token・座標から使用する重みindexと過去値位置をNodeが導出する。

| 証拠成分 | 確認先 |
| --- | --- |
| 命令・型・参照 | 固定program rootと期待命令位置 |
| 入力token | 正規input root・登録job・sの入力導出 |
| model値 | 登録model rootの**期待index** |
| 過去値・問題の記録 | 元trace rootの期待位置 |
| 出力 | 元output rootと認証されたEnd関係 |
| 提出責務 | 元claim、actor、role、CPN action context、原期限 |

Miner nonceは実入力を変える。Panel nonceは同じ元実行に対する担当者・義務・再送防止を結合する。この二つを混ぜて、Panelごとに元実行を変えてはならない。

### 8.2 判定

| 判定 | 意味と効果 |
| --- | --- |
| InvalidEvidence | 誤ったRoot/位置/path/actor等で認証不成立。Minerの不正として扱わない |
| LocalHolds | 認証された当該局所関係に矛盾なし。全体の成功を意味しない |
| ComputationRefuted | 元claimへ認証された関係・型・range・padding・Endの違反。成功を阻止する |
| 資料case未解決 | 必要資料の取得・応答が未充足。規定の終了処理まで成功を止める |

別modelまたは同model別indexの値をCourtへ出すことは、期待位置に対する証拠でない。元traceに認証された不正な命令・index recordを示す場合と区別する。

recordがCourtのbyte/型/範囲上限を超えて局所判定できない場合は、反証成立と誤分類せず、元claim・期待indexに結び付く有限の資料/構造caseを開始・維持する。規定上限内の正しい応答がない間は成功しない。過大payloadをInvalidEvidenceとして捨てたことだけでholdを解消してはならない。Nodeが無制限のrecordを受け取って裁定する迂回も禁止する。

既受理のnegative、資料要求、応答、終端に新D、新seed、新Ticketを課さない。自由な未認証spamを無条件に保留理由へするのでもなく、適格性、scope、有限case数、資金、期限、予約を適用する。未応答の制裁には責任を帰属できる規定済み根拠が必要であり、可用性不足を常に計算詐欺としない。

## 9. Panel責務と一人正直条件

Panelは、元claimの正規入力・model・全trace・outputを取得し、規定された全実行を再現・比較する。少数のpositive Court、他Panelの結論、DA ACK、同じ署名の再包装を完全検査の代替にしない。共有資料や正しい計算の再利用は許されるが、本人の物理労働を証明したとは扱わない。

一人正直の定理は、能動的な正直Panelが少なくとも一人存在し、その者が元claimを知り、必要資料を取得して全体検査できるか、取得不能を成功停止へ結合でき、有効反証・資料caseがFinal前に包含・裁定される条件で成立する。一人の有効反証は何件のpositiveよりも優先する。

選出人数を一人へ固定する提案ではない。最低完全承認、検査scope、資料義務、異議機会はprofileへ固定し、他のselected participantに与える実効的な異議時間を省略しない。全員悪意、正直者の永続検閲、資料caseの包含不能では、本定理の前提が満たされない。

## 10. Finalと全consumer

AN1の論理gateは、現在の登録・profile・帰属・資金・期限・依存、固定targetの当選、所定の完全承認、必要資料と異議機会、未解決適格caseなし、有効反証/失効なし、未消費を全て要求する。これらはnative記録から導出する。

原子的commitの順序を次で固定する。

1. 同batchの適格notice・資料caseによるholdを設置し、有効反証、失効、依存取消を適用する。
2. 元claim、profile/accounting版、期限、資料・検査・異議状態を現在stateで再検査する。
3. slot、s、実ComputationalWorkKey、用途に応じたBaseWorkKeyの未消費を確認する。
4. SuccessFinal、全消費印、報酬/credit発行、依存index、journalを同じ原子遷移で更新する。

後続のweight利用、EXEC mint/reserve/consume、DAA work標本、clock寄与、別支払APIも用途別の認証・成熟・取消条件を確認する。単なる受付済み候補や、古い承認cacheを利用可能な権利にしない。

timeout、court混雑、Panel未応答、資料不足、legacy fallback、repair、snapshot/importが、未検証を成功へ変える経路になってはならない。reorgは消費・発行・予約・依存を同じjournalで整合的にundoする。成熟とsettlement、単一履歴の一回性とDAG安全性は別に扱う。

## 11. 費用・確率・会計

### 11.1 正しい照会数による上界

独立なTicket RO、固定当選率p、現在の返値を見る前に決まる正しさ、全関係者の相異なる正しい初回Ticket照会数の**事前確定上限q**のもとで、

$$\Pr[\mathrm{correct\ win}]\le 1-(1-p)^q\le\min(1,pq).$$

未照会で提出した候補をNodeが初めてhashする場合と、協力者の照会も含める。最終的な当選集合だけを見たq、単なる期待予算、都合のよい候補だけを数えたqにこの式を流用しない。

### 11.2 未取得の費用定理

この節の未証明条件は、新FOW1で任意の将来研究ではなく**採掘適格性とactivationの必須条件**となる。TheoremRuleと全適合predicateを0062 §2/§4に従って確認できないモデルはFOW1採掘を行えない。

さらに、各実行の総予算Bと、前処理/cache/batch/外注/特殊model/入力選別を許した下界があって、正しい初回照会までに`q <= floor(B/c_min)`と示せる場合だけ、成功Final確率へ費用上界を接続できる。cryptoとPanel/gateの失敗は全期間・全候補の別項とする。

本RFCはc_minを通常LLMのGPU費用・電力・金銭費用として与えていない。AN1は実入力がσに依存するので、旧FULL1の固定A/Bに対する`bC >= KN`、18N、rank-1の値生成・費用比を流用してはならない。正しいFinalからTicket以前の実費・本人労働・非償却性を直接推論しない。

hash木の呼出数は木ごとに計数するが、具体hashの衝突困難性だけで古典RO抽出やGPU費用下界を得たと扱わない。tree/word算術量の加算には共通単位と操作重複の確認が必要である。

### 11.3 正当最適化・会計版

同じ正しい観測記録を作る高速化をslashしない。再署名・再送の二重発行と、正しいtraceを別手順で生成することを分離する。検証費用を意図的に重くすることをLLM評価下界の代替にしない。

認定単位、報酬/credit/weightの対応、BaseWorkKey用途、版、有効化・停止・pending移行を事前固定する。既存経済係数・総上限を黙って変更せず、その政策上の係数を実費定理と呼ばない。未証明のFLOPsや通常LLM料金を「証明済み仕事量」として認定しない。

## 12. 資源・時計・サービス

0055の全model共通容量、元caseの保護、全分岐予約を維持する。登録/元claimの薄い前段と、Panel完全再実行・資料サービス・courtを区別して予約・課金する。既受理の救済を新規登録spamや実行待ちで排除しない。

120秒はdifficulty制御epoch。全carrier合計1 BPSは外側搬送目標。AN1の全工程が120秒で完了、1秒でsettlement、全modelで一件だけ、courtを固定五件等へ変更しない。

Panelのcold資料取得、model root照合、全再実行、trace保持、証拠作成、包含・裁定を含めて最悪費用と期間を測定する。正常claimの完了には実資源が必要であり、nonceや署名の追加だけでは活性は得られない。

## 13. 移行

現在の新規採掘への移行は0062 §9を優先する。指定されたactivation A以後は旧AN1の新規受付を閉じる。以下は旧受理claim・署名の互換処理と、継承する不変条件として扱う。

AN1のfamily/codec/domainを新規に識別し、旧claimの意味・署名・元期限・過去の認証を保存する。旧profileの記録をimportするとき、AN1のfreezeや実入力結合が済んだと読み替えない。

profile更新・停止のpending処理は登録時に固定した移行規則に従う。停止後の新成功を許すか、検証継続か、非成功終端かを明記し、後から安易な旧target/旧codecへ逃がさない。正当な既発行権利を文書差替えだけで遡及没収しない。

本提案は登録モデルの品質審査やHF取得認定を復活させない。モデル資料とclaim資料の実認証、既存burn停止・帰属・無審査1.0/0.3をそれぞれ既存の意味で扱う。

## 14. 受入条件

以下は**実装の受入要件**であり、全てを本番実証したという記録ではない。

| ID | 必須の反例・確認 |
| --- | --- |
| AN1-01 | 同一Cを別資金/別slotで再登録しても新しい一回性を得ない |
| AN1-02 | owner/nonce変更が実tokenに反映され、古いclaim/openingの直接転用は拒否 |
| AN1-03 | token collision、UNK、切詰め、templateによるchallenge削除を拒否 |
| AN1-04 | beacon後のC変更、Panel選出後の元Root変更、既知seed再hashによる未来性偽装を拒否 |
| AN1-05 | 重みRoot属性だけの偽装、別model/別indexの証拠を拒否 |
| AN1-06 | 認証された算術/型/range/入力/End/padding違反を反証、偽pathをInvalidEvidenceに分離 |
| AN1-07 | 少数の正しいlocal Courtだけで全実行を承認しない |
| AN1-08 | 多数の不正positiveがあっても一人の有効反証が優先 |
| AN1-09 | 資料不足・未解決異議・期限切れ・停止をtimeout成功へ変えない |
| AN1-10 | D=1/最大値/境界直前直後で旧≤とAN1<が一致し、難易度を二重適用しない |
| AN1-11 | slot/s/実仕事/用途別BaseWorkKeyの原子消費、同batch反証先行、現在state再検査 |
| AN1-12 | import/repair/別API/legacy、reorg/取消/依存を通じた二重発行・未検証発行を拒否 |
| AN1-13 | 正しいcache・簡約・外注を算術不正としてslashしない |
| AN1-14 | 本番modelの全local関係、cold取得、最悪負荷、包含・裁定、全carrier経路を実測 |
| AN1-15 | 論理長/件数/indexのfloat・bool等を受付で拒否し、Courtと同じcanonical型にする |
| AN1-16 | 局所証拠上限を超えるrecordを資料/構造holdへ送り、InvalidEvidenceの破棄だけで成功にしない |

## 15. 有効化gateと証拠

新規FOW1には0062のG-EXEC/G-COLL/G-COSTと0061の完全TB1のgateを追加する。旧AN1又はMOR1の参照試験だけでこのgateを満たさない。新規familyの条件が未完なら、旧AN1をfallbackとして有効化しない。

有効化には、仕様の採択、native登録/freeze/署名/beacon、全実行意味論・trace/local checkerの完全性、実効的なPanel＋Court/資料契約、全consumer、移行、資源実測、合意との接続を要求する。採掘費用を合意安全性の根拠として主張するなら、独立な費用定理とその適用範囲も別gateにする。

独立参照実装は、入力・重み・trace・Courtと小さな状態モデルを検査する。学習済み本番LLM、実署名・beacon、ネットワークDA、native DB、全消費API、DAG定理、LLM評価費用の下界を証明しない。失敗がないテスト結果をこれらの代替にしない。

通常閲覧先の20 RFCと50 ADRは維持する。新0059/0227は統合後の新しい設計草案であり、担当冊へ適用範囲付きの参照を追加する。過去ログ・digest・旧検査baselineを変更せず、新規回帰を別に記録する。

<!-- MOR1-DRAFT-20261120-BEGIN -->
## 2026-11-20 MOR1追加草案の担当範囲

以下はMOR1初版の追加記録である。現在の関数由来出力・費用・TB1・新規受付の規範は0061/0062を優先する。

**DRAFT_NOT_ACTIVATED**。AN1への追加草案として、モデル登録時の認証出力構成をMOR1に定義する。AN1の完全C、s、実token入力、三Root、Ticket式、元claim、Panel＋Court、一回会計を維持する。MOR1のreceiptでTicketを置換したり、自由な再抽選を追加したりしない。

[RFC-0060](../rfc/0060-model-registration-and-attempt-output-binding.md)／[ADR-0228](../adr/0228-model-registration-and-attempt-output-binding.md)／[変更・検証](../maintenance/revisions/2026-11-20-model-output-binding/README.md)。この参照追記は既存の採択・履歴原文・runtime・activationを変更しない。
<!-- MOR1-DRAFT-20261120-END -->
