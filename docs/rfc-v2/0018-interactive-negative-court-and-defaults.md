# RFC-v2-0018 — 対話型 negative court・公開異議・提供義務違反

2026-11-11の後続採択: [RFC-0053](../rfc/0053-additional-model-burn-pause.md)／[ADR-0221](../adr/0221-additional-model-burn-pause.md)がHF取得可否の認定をburn停止・解除へ置換する。追加登録モデルだけを対象に、誰でもburnでPAUSED、登録者だけがburnでACTIVEへ戻す。genesis・年次fork指定は対象外。第三者取得証明・定期可用性認定・取得不能の多数決を要求しない。HF固定版/hash照合はoff-chainの実行資料同一性確認として残す。claim固有資料、再実行承認、不正反証と責任・原期限は維持する。PAUSEDは新規受付・成功Final・新規報酬確定を止める。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

採択日: 2026-11-08 / 単独反証と状態遷移の改訂: 2026-11-09 / 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。

[20本の索引](README.md) · [採択記録 ADR-0215](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) · [上位原則](../PRINCIPLES.md)

追加採択: [RFC-0048](../rfc/0048-exec-credit-schedule-and-one-bps.md)・[ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)。EXEC 二層設計との資源競合、および court の DAA 予算膨張を是正する公開取得・予約・完成条件を追加する。設計状態は引き続き `ADOPTED_DESIGN_NOT_ACTIVATED` とする。

追加採択: [RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)・[ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)。一人の適格な検証者の反証を多数の肯定署名から独立に裁定し、未解決異議をnative状態として保持する。

公開資料とモデル未保有の単独正直者についての追加採択: [RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)・[ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md)。20の新profile `PublicOneHonest`では、初めてモデルを取得する選出者の資料要求・検査・反証を保護する。正常claimに対する悪意の不署名と有限の虚偽caseを処理する条件を追加し、資料到着、検査前提の成立、検査完了を区別する。論理名はwire IDの割当ではない。

## 1. 適用範囲と責務

本書は、新しい claim 版における不一致の局所化、局所 court、公開資料の要求、有限な提供義務違反を所有する。
正常時の非対話 positive は [RFC-v2-0017](0017-noninteractive-positive-verification.md)、元の Miner commitment と供給義務は [RFC-v2-0016](0016-miner-execution-commit-and-material-supply.md) に従う。
Node の検査上限は [RFC-v2-0019](0019-model-free-node-and-bounded-evidence.md)、成功 Final の条件は [RFC-v2-0020](0020-verification-gated-final-and-adversarial-models.md) が所有する。

採択は文書上の設計採択である。wire ID、codec tag、署名 domain、fence、有効化高さは未割当であり、本書は稼働を宣言しない。
旧署名済み claim、旧 court、旧責任期限は受理時の規則を維持する。旧番号や既存 receipt の意味を本書へ読み替えてはならない。
共通の会計保存則・役割別同意は [RFC-v2-0011](0011-bond-budgets-fees-and-payments.md)、旧経路の記録は [RFC-v2-0014](0014-court-master-contract.md) を参照する。

## 2. 脅威と開始条件

モデルが共有済みであること、モデル保有者に正直な者が含まれること、通常 Panel が独立であることを前提にしない。
同じ悪意ある主体がモデル登録者・Miner・モデル内 Panel を支配し、外部者にだけ必要資料を渡さない場合を扱う。
公開異議経路の存在から、正直な検証者の実在、資料取得の成功、期限内の参加を推定してはならない。

PublicOneHonestは、当該claimを担当する独立選出Panelに能動的な正直者が少なくとも一人いることを条件とする。その人がモデル・重み・traceを事前に保有することを要求しない。本人が必要資料を取得できない場合も、初回manifestと供給契約から適格な資料要求を開始できる。誰が正直かをnativeの観測可能なflagや多数票で決定する仕組みにはしない。

negative の対話は、固定済みの claim について不一致を発見した後、その場所を絞るために使う。
すでに認証された直接反証がそろう場合は、対話の完了や positive の作成失敗を待たずに提出できる。
資料不足だけの場合は、計算不正の探索を強制せず、提供義務と未検証終了を扱う。

| 観測 | 受付候補 | その観測だけから出せない結論 |
| --- | --- | --- |
| 元の実行条件で再現した trace／出力が不一致 | 局所化を開始する適格な notice | Miner の計算有罪 |
| 元の claim に結び付いた必須関係の矛盾 | 直接の局所反証 | 他 claim／他当事者への一括帰責 |
| 必要資料を取得できない | 有限な canonical request と未検証処理 | 計算有罪、提供義務違反の自動成立 |
| 既定の義務・要求・提出機会・期限と不履行がそろう | QualifiedDeliveryDefault の裁定 | モデルの登録取消や階層変更 |
| Panel の証言または署名だけがある | 当該発言者と対象の認証 | 算術の評決、成功 Final |

### 2.1 一人で開始・完了できる資格

選出検証者一人は、元claimのdraw記録に対する本人の資格、署名、既存のbond・費用同意、本人用の救済予約を用いてnoticeを提出できる。肯定票数、negativeの定足数、Minerの同意、Panel代表者の署名、他の検証者の応答を追加条件にしてはならない。重複する抽選positionが同じbondを指す場合は、その同一主体の予約を正規化し、票数を理由に別人の救済枠を奪わない。

公開第三者は選出名簿に属する必要はないが、元profileの公開bond下限、費用、対象範囲、期限、有限な公開予約の条件を満たす。正規に受理して予約を付けた第三者の権利を、後の多数票・owner交代・同じ資料への別要求で取り消さない。資格だけから無制限の新規要求枠が生じるとはしない。20の「一人」の条件付き保証が第三者へ適用されるのは、その人の公開経路が当該条件を満たす場合である。

初回申立ては次のいずれかである。

| 種別 | 受理時に確認するもの | 直後の効果 |
| --- | --- | --- |
| `MismatchNotice` | 元claim・原root・同じ実行contextに関する比較root、canonicalな争点区間、本人署名、資格、期限、資金・予約 | 未解決caseを作り成功を保留。計算有罪は未確定 |
| `MaterialRequest` | 元claimに必要なcanonical対象、検査scope、署名済み供給義務との対応、本人資格、取得後検査・反証を含む件数・費用・応答機会の予約 | 未解決検証caseを作り成功を保留。個別の供給義務をそのcaseへ結ぶ。未提供・算術不正は未確定 |
| `DirectRefutation` | 元claimの必須関係への完成済み局所証拠、19の認証・サイズ・work上限、資格、期限、有限の提出費用・予約 | noticeとterminalを同時受理できる。Node再計算後にだけ計算有罪を確定 |

原rootと比較rootの相違はnoticeの理由であり、正しさの判決ではない。未署名の私的苦情、任意の別モデル、自己申告の資格、同じ要求の無制限な複製は適格noticeにならない。受理判定の期待値はnativeのoriginal・draw・policyから導出し、packetの`qualified=true`を信用しない。

`MaterialRequest`の開始に、計算矛盾、Panel自身の完全な再実行root、モデルの事前保有を要求しない。元claim・manifest root・必要な対象IDとcanonical range・検査scope・申立人・供給義務者・各期限・資金・個別予約を束縛する。要求すべき対象を識別する入口情報自体が欠ける場合も、16の認証付きmanifestに対する有界な要求として扱える形式をprofileに固定する。必要情報を取得済みの者だけが合法要求を作れる循環を認めない。

### 2.2 未解決集合と個別caseを保存する

claim Cの未解決集合を`Q(C)`とする。各要素は`CaseId`、元subjectと版、申立人、受理点、検査scope、現在の争点と手番、対象義務、残予算、個別期限、証拠identity、状態を持つ。`CaseId`は、domain・元claim・正規化した申立人のbond/principal・申立て種別・canonicalな争点identityから導出する。選出者はnative drawのbond、第三者は受理済み資格に対応するprincipalを使い、packetの別名をそのまま採らない。争点identityは初回の比較root対または要求資料・検査scopeを含み、継続時にCaseIdを作り直さない。relay・queue・再送nonce・同一bondの重複drawで別予算にしない。

共有する`MaterialId`／要求対象identityは、内容root・canonical範囲・規則版から導出し、申立人名を含めない。**共有資料のidentityと、個々の申立人のCaseIdは別である。** 同じ資料への要求を先に出した悪意者のcaseへ、別の正直な申立人のcase・予約・提出権を吸収してはならない。同じbytesへの応答を再利用しても、各caseの取得後検査と終了条件を別々に評価する。一人の撤回・default・棄却で別人のcaseを閉じない。

適格noticeまたは要求を受理した時点で、要素の保存、予約確保、成功保留を同じnative遷移にする。`Q(C)`が空でない間は20の成功条件が成立しない。多数の肯定署名、局所positive、別caseの棄却、Panelの「解決済み」receiptは、この集合を消去する権限を持たない。

無効な署名・opening・scopeのpacketを他人のCaseIdに向けても、その人のQ、手番、完了flag、保護された救済予算を更新しない。無効packetの受信・検査の上限と提出者への料金・spam制限は入口で処理し、被害者の正規の一手として数えない。元の応答義務者が適格な応答をしないまま期限に達した場合の責任は、別途§8の条件から判定する。

状態は少なくとも`Open`、`AwaitingMaterial`、`InspectionPending`、`Localizing`、`TerminalSubmitted`、`Adjudicated`、`ClosedRejected`、`ServiceInterrupted`を区別する。論理名であって既存wire tagではない。集合から除けるのは、その要素に対応する決定論的な終端をnativeが適用したときだけである。未取得のstate行を空集合とみなさず、rootに認証された不在と`Unavailable`を区別する。

### 2.3 資料の履行と取得後検査を分ける

`MaterialRequest`の適格応答は、その対象の供給義務を履行済みにできるが、それだけでは検証caseを解決しない。必要な資料がそろったcaseを`InspectionPending`へ進め、`Q(C)`と申立人の残予約を維持する。他人の受領receipt、providerの完了署名、別caseのpositiveは、本人の検査caseの終了ではない。

初回受理時に、対象資料とその資料を必要とする元claimの検査scope、取得完了期限、検査・局所化・証拠作成・包含・裁定の各終端、最大手数・bytes・work・費用を固定する。到着時刻から新しい期間を開始せず、元の時計・有限予算内で検査する。応答が遅く残りの最悪検査・救済工程を収容できない場合、単なる履行扱いで成功へ戻さず、帰責条件を分けた回復または未検証Abortにする。

期限内に始まったこのcaseでは、取得後に発見した元scope内の不一致を、同じ`CaseId`の`Localizing`または`DirectRefutation`へ進められる。初回notice窓が閉じた後でも、これは受理済みcaseの継続であり、新規notice・別claim・別検査scope・再抽選・予算更新ではない。元scopeから外れる要求はその継続権を得ない。

検査caseを閉じられるのは、当該申立人が対象資料と検査scopeを明示して署名した検査完了／許可された撤回、固定争点の認証済み正規terminal、客観的な`ChallengerDefault`、または成功不可を維持した期限終了である。本人の完了申告はそのcaseの権利行使を終了するものであり、17の完全再実行承認を自動生成しない。無回答defaultには本人の必要資料・所定の残検査時間・提出機会が実際に成立したことが必要であり、遅い応答やservice不足を本人のdefaultへ転嫁しない。

### 2.4 検査前提の成立と資料秘匿・虚偽苦情の境界

検査caseの前提確認を、論理状態`InspectionPrerequisitesEstablished`として記録する。これは既存wire tagや自己申告boolではなく、元scopeに必要な資料へのアクセス・内容認証・残検査時間・当人の提出包含機会を、profileが定めた有界な客観的根拠からnativeが確認したrecordである。根拠への参照、対象scope、成立時点、残手数・期限を保存する。単にあるchunkが適格応答されたことと、全検査scopeの前提成立は同一ではない。

許される遷移は次のとおりとする。

| 状況 | caseの遷移と効果 |
| --- | --- |
| 一部の要求資料だけが到着 | 履行済みの対象だけを記録し、残るAwaitingMaterial／InspectionPendingとQを維持 |
| 元scopeの全検査前提が客観的に成立 | InspectionPendingに前提成立recordを付け、元の予約済み検査・提出期限を適用。到着時刻から時計は再開しない |
| 前提成立後、本人が署名して検査完了・許可された撤回を提出 | 当該caseを閉じる。17の完全検証承認は別にscopeと署名を検査 |
| 前提成立後、元scope内の不一致・直接反証を提出 | 同じCaseIdと残予算でLocalizing／TerminalSubmittedへ継続 |
| 全前提と正規の次手・提出機会が成立したのに、本人が検査期限・次手を放棄 | その本人のChallengerDefaultで当該caseを終了。無関係な他caseは維持 |
| 前提を客観的に確定できず、元期限内の回復もできない | ServiceInterrupted／NoSlashHold、または帰責可能な供給義務違反を別途処理し、成功不可のまま未検証Abort |

URL、providerの保存署名、別Panelの受領証・完全検証承認、小さなchunk数個の応答、要求quotaの消尽、DAAの進行だけでは全前提を成立させない。資料を隠された正直な人に「予算を使い切ったから受領済み」と扱ってdefaultを課してはならない。一方、前提が客観的に成立した後に申立人が単に「まだ検証していない」と申告しても、署名済みの次手・期限・scopeを無期限に延長できない。

正常完了を保証するprofileは、許す全ての資料要求と取得後検査caseについて、この前提成立または正規terminalへ至る有界な実装上の対応を示す必要がある。正直に公開された大きな資料と虚偽の未着申告を、現在の小さなnative資料だけでは区別できない場合、その対応は未確立である。承認最低件数1、bond、希望的な配信予定を、その不足の解決とみなさない。Nodeが全モデルを取得・走査するfallbackを暗黙に追加せず、19・20の有効化gateを満たすまで当該profileを正常完了可能として有効化しない。

## 3. 受理時に固定する対象

negative case は、元の受理済み claim の identity と受理 DAA を参照する。申立人が提出した別の claim を期待値として使わない。
比較する両者の commitment は、同じ model・program・input・実行規則・seed・job context に関するものでなければならない。
最終出力が一致していても、元の trace root と再現した trace root が異なる場合、正常時の root 一致として扱わない。

| 論理欄 | 必須の結び付き |
| --- | --- |
| scope | network、work namespace、claim 版、規則版 |
| original | 元 claim、Miner の署名済み実行 root、出力、受理点 a |
| challenger | 申立人、bond、本人が主張した比較用 root または直接反証 |
| location | canonical な区間・leaf・演算座標、現在の争点 |
| material | 登録物・入力・元 trace への認証付き資料の参照 |
| authority | 役割、応答義務、必要な費用同意、予約、提出権限 |
| clock | notice 窓、hard、collect_until、material_until、手番の締切 |
| budget | 元 claim に帰属する累積 work・bytes・要求数・同時処理の上限 |

この表は論理契約であり、既存 wire 型の追加フィールドや新しい数値 ID を割り当てるものではない。
新形式では旧署名と domain を分離し、同じ bytes を別の役割・版の同意として再利用できないようにする。

## 4. 対話による局所化

1. **受付。** native owner が original、適格な申立人、notice 窓、bond、資金、最悪経路の予約を確認する。
2. **固定。** 両者の root と、比較対象の canonical な trace 構造を固定する。対話中の root の差し替えを認めない。
3. **絞り込み。** 認証付きの区間 commitment を用い、二分探索等の当該 profile が定義する手順で相違箇所を縮める。
4. **境界。** 共通の直前境界または元の claim に認証された依存値から、争点の演算に必要な値を特定する。
5. **資料完成。** 演算定義、入力・出力、必要な重み断片、依存値、各 opening を局所証拠へまとめる。
6. **裁定。** Node が上限内の資料を独立に認証し、一つの局所関係を決定論的に再計算する。
7. **反映。** native owner が、受付済み case、実際の証拠 identity、当事者の責任に結び付けて状態・資金を一度だけ更新する。

比較用 trace は申立人の主張である。Panel が自分で作った誤った値を、元の Miner の値として帰責してはならない。
Miner 側の証拠に用いる値は、Miner が署名した original root に対して開かなければならない。
双方の root が異なること、Panel が多数であること、探索が終了したことは、いずれも算術有罪の代わりにならない。
任意の一箇所を正しく計算したという PredicateHolds も、claim 全体の正常性を確定しない。

### 4.1 局所化の進捗とfalse complaintの終了

profileは、両rootの固定方法、区間の規格化、各手番が開く境界、次の区間の一意な選択、terminalとなる条件、最大手数、各応答期限をoriginalで固定する。合法な継続ごとに、未解決区間または残手数が厳密に減る順位を持たせる。応答者が同じ区間を繰り返す、別rootへ移る、再開名を変えることで進捗を無限に戻してはならない。供給義務への無応答は§8の条件に従い、申立人の次手・terminalの放棄は本節および§2.4の前提を満たす場合だけChallengerDefaultとする。当該役割の客観的defaultを算術有罪へ変換しない。

対話のterminalが元claimに認証された全必要入力・演算・対象出力を持つことを19で確認する。比較側の値だけでMinerを裁かない。初期境界、読み出し、状態接続、命令位置、停止・最終出力の不整合も16・19の各必須関係へ到達できなければならず、算術命令だけを候補にして探索を終えない。

`LocalPredicateHolds`でcaseを閉じられるのは、そのcaseの固定争点を決着する正規terminalであり、対話の認証履歴とendpoint条件がそろう場合だけである。Minerが無関係な正しい一箇所を提出した場合は、そのcaseの終端にならない。反証が成立しなければ、そのcaseだけを`ClosedRejected`へ進め、20がすべての成功条件を再評価する。claim全体の正しさを新たに認定したとは扱わない。

申立人が必要な次手またはterminalを提出しない場合、本人の署名済み義務・前提資料・個別の提出包含機会・期限を確認して`ChallengerDefault`を確定できる。これはそのcaseを不成立で閉じる根拠であり、他caseや資料義務を閉じる根拠ではない。責任のない通信・service不足ではこのdefaultを作らず、§7・20の回復／未検証終了に進める。無効packetの拒否だけでは、すでに受理した別の有効caseを閉じない。

正しいMinerが元rootに一致する応答を適格に提出する条件の下では、悪意の比較root・虚偽の不一致・手番放棄を、正規terminalの棄却または申立人defaultへ有限に進められる局所化規則を要求する。Miner以外の供給者や正直なPanelが同じ元rootへの認証済み資料を代理提出できる範囲は、元契約で明示する。代理者の新しいtrace・rootで元Minerの主張を置換しない。残りの悪意Panelの協力、追加署名、代表者の許可を、正しい応答者の進行条件にしない。

### 4.2 完成済み反証を多数決から独立に処理する

`DirectRefutation`は、19の検査に必要な最小資料がそろえば、一人からの提出だけで評価する。Minerの追加署名、positive bundleの完成、比較用trace全体、対話の全手番、negativeの票数を要求しない。ただし元claimへのbinding、適法な提出窓、資源上限は省略しない。

有効な`ComputationRefuted`は20の`Refuted`または`Revoked`へ接続する。どのcaseの証拠であっても、元claimの同じ必須関係の破れを確定した後は、別caseの`LocalPredicateHolds`や後着のpositiveが元claimを復活させない。適用する`EvidenceId`と`OffenceId`をcanonicalな証拠・元claim・対象役割へ結び付け、同じ事実の再送・別relay・別申立人からの提出で二重裁定効果・二重徴収を生まない。

## 5. オフチェーン処理と公開経路

通常の探索は認証済みの P2P/RPC を使ってよい。私的な対話履歴は高速化の補助資料であり、それだけから native の slash、義務履行、成功 Final を作らない。
当事者が停止した場合、許された canonical request・response・直接反証を同じチェーンの予約された経路へ提出できなければならない。
外部 DA chain、別の裁定チェーン、Panel の期限証明を新しい権威にしない。

選出 Panel の所属を、公開異議の提出資格にしてはならない。既存の条件を満たす public bonded outsider は、同じ original に対する資料取得と反証を行える。
Miner が選出 Panel だけへ資料を渡したことを、公開の請求への履行として扱わない。
必要な対象は登録済み実行定義・元 claim・canonical range から導出し、モデルを持たない申立人にも、許された資料要求を識別するための情報を与える。

資料の canonical request identity は、申立人名、relay、キュー名を変えて再生成しない。重複・重なりは canonical な対象へまとめる。
ただし、資料を共有することと、各当事者の notice、費用同意、手番、提出機会、履行事実を共有することは別である。
認証済み資料が到着しただけでは、特定当事者の有限な提供義務が完了したことにはならない。
代替 provider の応答は、元の義務が許可し、その義務に対する適格応答として受理された場合に履行へ算入する。

### 5.1 owner の専有待ちを公開経路の前提にしない

新 profile の公開取得・局所化・提出は、資料 owner が排他的な席を保持する期間や、その席の先行待ち行列が解消することを必須の前提にしない。owner の全席を攻撃者が占有しても、適格 outsider の認証済み資料要求、資料の利用、反証提出の予約を失わせない。owner、Miner、モデル内 Panel の協力者だけを provider の候補に限定しない。

claim が供給責務を負う資料を、canonical blob／range／認証経路に分け、許可された共通経路へ結び付ける。所有者と異なる worker・provider が、認証できる同じ bytes を供給し、複数の請求人が利用できるようにする。認証済み資料の共有・先読み・一括取得と独立 worker の並列化を認める。対話の前の応答が次の争点を決める場合など、真に逐次である辺は [05](0005-bounded-resources-and-long-context.md) の依存 DAG に残す。

供給者の署名や保存宣言だけでは、outsider に資料が届いたことにならない。すべての元保有者が隠匿し、適格な供給経路にも必要 bytes がない場合、算術反証の完成を捏造しない。所定の肯定検証を成立させず、提供義務違反を客観的に裁ける範囲と未検証終了を分ける。private owner の任意の応答を、保証された公開取得の実績に置き換えてはならない。

### 5.2 共有する bytes と共有できない責任

同じ blob の複数要求は canonical identity により共有できるが、異なる合法 range、別 root、別 ruleset、別 claim の証拠 binding を同一扱いにしない。重複した bytes の保存・搬送を減らした場合も、必要な各 opening・署名・座標・型・役割への検査を残す。実際に再利用できない cold fetch を cache hit として割り引かない。

資料の到着が遅いことによる複数人の再要求を無限に予約しない一方、異なる適格な要求を重複扱いして破棄しない。要求者・relay・queue・case を変えても、同じ original に帰属する累積予算は同じとする。許可された別 original の要求は全 claim の容量台帳へ加算する。本人の費用同意、応答義務、手番、包含機会、返還・slash の条件は、資料の重複排除で他人のものに置き換えない。

### 5.3 小さな公開応答と全資料の配信を混同しない

通常の重み・入力・traceの配信はMiner／Panel／provider側で行う。係争に用いるcanonical responseは、固定した対象とrangeへの認証、提出機会、期限、履行効果を19の上限内で判定できる形式に限る。Nodeが外部URLへ接続して未着を主観判定する、全chunkを順次読み込めば最後にはモデル全体を検査できるという理由で上限を迂回する、未知の配信者のreceiptを新しい裁定権威にする、という経路を認めない。

一つの小さな応答はその対象の履行だけを示し、全資料の利用可能性や全体検証の完了を示さない。複数応答の合計bytes・hash・CPU・包含・保持も05・19の全claim予算へ計上する。必要な全検査前提をこの予算と元期限内に確定できないときは、§2.4に従って成功を止める。cache・mirrorは供給の実手段であり、それらが存在するという宣言だけで前提成立recordを作らない。

## 6. 時計は元の claim から動かさない

本版の公開異議は [旧 RFC-0047](../rfc/0047-batch-court-time-contract.md) の原点と境界を保持する。

| 時点・量 | 規則 |
| --- | --- |
| a | 元の claim の受理 DAA。新しい receipt の受理点へ移さない |
| notice | a から a + 3,120 までに、適格な最初の notice を受理する |
| hard | 係争中 claim の終端は a + 10,000 |
| 最遅 notice 後の予算 | 6,880 DAA。局所化・提出・収集・回復を含む |
| collect_until | hard 以下。保持期限で置き換えない |
| material_until | 別の保持期限。元の署名済み義務・依存・reorg 条件に従う |
| 排他終端 | clock-v2 の hard + 1 と collect_until <= hard を混同しない |

取得・検証・異議受付は同じ原点から進行する。positive 完了、negative 開始、担当交代、再送、再起動、case の枝分けで時計をリセットしない。
異議のない claim を一律に hard まで待たせる規則ではない。成功の最早時点は RFC-v2-0020 が決める。
Panel が「対話完了」と署名した時点から新しい 10,000 DAA を始めてはならない。
上限内に収まらない経路を、時間見積の切り詰めや旧長期経路への切替によって受理可能にしてはならない。
資料保持期間の延長は、新規 notice・追加裁定の権利を自動的に延長しない。
Final 後の証拠は、その claim 版に元々定められた訴追・責任の窓に限り処理し、期限を遡及変更しない。

### 6.1 初回notice・terminal・Finalの境界

`n = a + 3,120`、`h = a + 10,000`とする。初回noticeのcanonical受理は`a <= now <= n`で許される。完成済みの直接反証がこの窓内にある場合は、初回noticeとterminalを原子的に受理する。`now > n`の新規初回noticeを、単に完成済み証拠という名前で受付可能にしない。

受理済みcaseのterminalは、そのcaseの`collect_until <= h`と各手番の境界に従う。`collect_until = h`が許されたcaseでは`now = h`の合法なterminalを終端処理より先に評価する。clock-v2の排他終端`h+1`を、新しい証拠受付期限と読み替えない。

成功Finalの最早時点は`now > n`である。端点における異議、適格応答、直接反証、裁定、失効、Finalの同一canonical処理バッチ内の優先関係は20が所有する。実装はFinalを先に適用して同じバッチの正規noticeを遅着扱いにせず、期限に間に合う最終証拠をAbortの先行で無効化しない。

Final後の訴追はoriginalで固定した有限の受付・責任窓と保持義務に限る。profileはその窓と閉鎖規則を空欄のまま有効化できない。後日証拠が見つかったことだけで、新たなnotice権・時計・役割同意を生成しない。期限内に受理された事実と、後日に証拠を復元できることを別に保存する。

### 6.2 全公開経路の期限と契約を閉じる

original 受付時に、[05](0005-bounded-resources-and-long-context.md) の critical path・資源別総需要／実供給・実行可能な予約 schedule を照合する。最遅 notice の `a + 3,120` から、cold fetch、局所化、最遅応答、最後の proof、収集・包含・裁定、許容された復旧を `a + 10,000` の内側へ割り当てる。最も短い一経路や完成済み proof の提出だけでは、残り `6,880 DAA` の公開救済を満たしたことにしない。

末尾の model data、対応する opening、署名・role consent が利用可能になる時点と、各手番の締切を明記する。計算前の時刻に署名された有効期限・保持期限から、取得・計算・搬送・受付までに消費した時間を差し引いて検査する。受付時の残期間が不足する場合、期限の書換えや後続 receipt を新 original にする方法で救済しない。未受理なら必要な再同意・再予約を正規の手順で行い、受理済みなら元契約と未検証処理に従う。

reorg による handoff／手番の replay、同じ packet の再送、未取得資料の再取得を分ける。添付分析にある `reorg_replay_moves = 64` は過去の分析対象の設定であり、本版の値の採択でも「取得失敗64回」の意味でもない。対象となる最大手番の認証・搬送・再包含・予約復元と、再利用可能な資料の範囲を導出し、回復費用を隠さず同じ original に計上する。

Frame／Batch、public fallback、relay の別入口、再起動・reorg 後の再開にも同じ受理・予約・時計・署名契約を適用する。保持期間は取得済み資料をいつまで残すかの義務であり、保持期間中ずっと一つの要求が処理枠を専有することや、収集期限を延ばすことを自動的には意味しない。

PublicOneHonestでは、coldな選出者の取得・全体検証・初回noticeの最悪経路をnotice窓内へ、最後の合法noticeから資料回復・検査・局所化・最遅応答・terminal・包含・裁定の最悪経路をhard内へ配置する。検査開始点を全必要資料の最遅利用可能時刻に合わせて評価し、最後に資料を渡したことで検査時間が消える計画を受理しない。caseごとの検査締切・応答締切は原点aから固定し、悪意者が期限端点を合法に使っても終了できる余裕を予約する。

## 7. 結果の型と成功条件

以下は native の結果を分離する論理名であり、既存 enum の tag を再利用する指定ではない。

| 結果 | 必要な根拠 | claim と責任への効果 |
| --- | --- | --- |
| ComputationRefuted | 元 claim に属する必須の計算関係が、認証済み値から再計算して破れている | claim 無効化と、確定した計算責任の処理 |
| LocalPredicateHolds | 当該caseの正規terminalにおいて指定局所関係が一致する | 当該反証だけを棄却。無関係な一箇所のpassではcaseを閉じず、全体の承認・Finalは別途判定 |
| EvidenceRejected | 認証、対象、形式、範囲、資源上限の不成立 | 当該証拠を拒否。Miner の計算有罪を導かない |
| QualifiedDeliveryDefault | §8 の有限義務違反が成立する | その役割が署名した delivery 責任のみを処理 |
| ChallengerDefault | §4.1の申立人の次手・terminal義務違反が客観的に成立する | そのcaseのみ不成立で閉鎖。Minerの全計算を肯定せず、費用・責任は署名範囲内で処理 |
| Unverified | 資料不足・検証未完了で所定の承認条件が成立しない | 成功 Final・当該仕事の credit を与えない |
| ServiceInterrupted | 必要な提出・包含機会が成立しない | NoSlashHold とし、有界の回復または Abort へ進める |
| Abort | 元の期限内に必要条件を完成できなかった | 成功 Final と区別して終了し、残資金を規則どおり決済 |

同じ反証の棄却で、別の受理済み異議、依存関係、供給義務を消してはならない。
棄却後に成功候補へ戻す場合も、肯定承認・期限・依存・未解決事項を RFC-v2-0020 に従って再評価する。
期限到達を成功として処理しない。反証不成立を、検査していない範囲の肯定証明に変換しない。

`Q(C)`に未解決要素が残ったまま成功可能な期限を終えた場合は20の`AbortedUnverified`とする。成功Finalへ自動昇格させず、Minerへの計算slashを推定しない。Abort後も元版が受理済み責任の処理を認める範囲ではcase・証拠を保持するが、後の閉鎖で当該失効claimを成功へ復活させない。

## 8. QualifiedDeliveryDefault の全条件

提供義務違反を処罰するには、次のすべてを native の記録から確認する。

1. 当人が、対象・役割・範囲・上限・期限・責任・必要な費用に署名している。
2. 要求が、その original と義務に対して許可された canonical request である。
3. 当人が応答するための前提資料と、応答権限が成立している。
4. その当人・その手番に、予約された適格な提出・包含機会が成立している。
5. 同じ義務への適格な直接応答または許可された代替応答で、履行済みになっていない。
6. 規則が定める締切に達し、未履行の範囲を確定できる。

義務者自身が供給・保持を約した original 資料の隠匿、破棄、未保存は、条件3の外部前提の未成立による免責に含めない。第三者 provider が他の義務者から受領する資料への依存は、署名済み責務の依存関係に従って別途帰責する。

私的 RPC の timeout、Panel の「受け取っていない」という証言、上限外・重複・無効要求への無応答は、この全条件の代わりにならない。
別人・別手番にあった提出機会を、その当人の機会として流用しない。Panel が receipt を出さなくても native が受理した適格応答を消さない。
機会の前提を立証できない場合、算術有罪や PartyDefault へ変換しない。成功条件が未成立であることは別に維持する。
受理済み Miner 応答を、その後の Panel の失敗だけで遡及的に未提供へ変更しない。

## 9. 妨害・再試行・資源枯渇への制約

適格 notice だけが、その対象 claim の成功を保留できる。無制限の「取得できない」という申告に永久 veto を与えない。
要求資格、bond、費用、範囲、件数、同時 case 数、重複排除、終了条件を受理時に確認する。
不適格な packet は受理せず、受理した反証にも元の期限内での終了を要求する。

positive の通常処理が、必要な negative・公開回復の予約を使い切ってはならない。
予約には、最後の合法 notice、最遅の合法応答、公開提出、再起動時の回復までを含める。
単一要求だけでなく、全 Panel・全 outsider・全 relay を合わせた元 claim 全期間の累積費用を制限する。
case、鍵、nonce、provider、探索分割を変えて同じ検証予算を更新できないようにする。
取得失敗を理由に、都合のよい Panel を引くまで検証者を再抽選する経路を設けない。
別 claim の大量購入に対する既存の bond・発行・容量の上限も残し、一 claim 一回だけで反復攻撃全体が解決したと扱わない。

### 9.1 全分岐の予約と EXEC からの隔離

受理する要求の有限な集合について、複数の攻撃者が別々の合法資料を要求する分岐と、複数 claim の同時係争を含めて予約する。一経路だけの測定や、互いに排他的でない分岐への同じ枠の使い回しでは足りない。細分化・重複 proof・異なる binding に必要な CPU、I/O、帯域、制御メッセージ、包含、保持を [05](0005-bounded-resources-and-long-context.md)・[19](0019-model-free-node-and-bounded-evidence.md) の合算予算で扱う。

公開 court と EXEC の queue は、予約済みの異議・応答機会が EXEC の負荷で飢餓状態にならないよう分け、実際に共用する物理資源を同じ台帳で予約する。予約外の新規需要には backpressure を適用する。EXEC の 1 BPS 目標や未確定 credit を、測定されていない court 容量の供給証拠にしない。

EXEC が止まり、または仕事の credit が不足しても、BASE／Heartbeat による claim・court・anchor・schedule の経路は必要資源とともに維持する。公開 notice、適格応答、反証、終端処理を次の成功 EXEC 待ちにしてはならない。通常の肯定検証と受理済み救済の双方を含む需要が供給を超える場合、既存の権利を奪って見かけの 1 BPS を達成するのではなく、新規受付・発行側へ制約を適用する。

### 9.2 一人分の救済を他のPanelの需要から守る

original admissionは、drawで生じ得る各独立選出bondの一つの適格な救済経路と、profileが許す有限の公開第三者経路を予算化する。draw確定後に本人の予約へ結び付け、他の選出者の合法要求、positive、owner席、EXEC負荷で本人の最初の適格noticeとその最悪継続の予約を消費できないようにする。同じ主体の重複draw位置・重複要求には追加予算を与えない。各主体の費用同意と責任を、共有される資料bytesとは別に保持する。

予約は個別の受付・応答・terminal・包含・再計算・許容回復までを含み、受理済み一人のケースが後から`NoCapacity`で捨てられる計画を受理しない。共有する物理資源については05の全合法同時継続の検査を残す。「個別枠」と書くだけで供給容量が増えたとは扱わない。新規claimと未予約の追加要求へbackpressureをかけ、既存の救済予約を先に維持する。

未知の第三者全員に無制限の枠を保証しない。公開経路の有限上限、資格、費用、競合する新規要求の決定論的な受付順をoriginalのprofileに固定し、受理後のcaseについては同じ非収奪条件を適用する。20の単独反証保証に使う一人について、その受付・予約・包含の前提を明記する。

一人の正直者が誰かは不明なので、先着者が使い切れる共通一枠だけを「正直者用」と呼んではならない。各選出者の有限権利に加え、同時に許す第三者caseの上限を合算する。最低承認数が1でも、その変更を根拠に他の選出者の救済予約や全体検証義務を取り除かない。

### 9.3 包含前提と観測可能な障害

私的送信、mempool到着、relayの受領署名、Nodeのローカル時計だけをcanonical受理時刻として使わない。包含の保証は、元profileに定めたchainの進行・配送・予約サービスの前提に依存する。全block producerによる任意の検閲や、nativeに一切現れない私的な未着を、この仕組みが常に検出できるとはしない。

nativeに認証された未解決case、未履行要求、予約serviceの不供給があれば、成功を停止し、元時計内の回復または未検証終了へ進める。前提が崩れて有効な証拠がcanonicalに到達しない場合は、単独反証による無効化を実証済みとせず、20の保証範囲外の障害として記録する。記録可能な失敗と、観測できない検閲を同じPASS判定へまとめない。

### 9.4 悪意多数の不署名・虚偽caseと条件付き正常完了

17の承認最低件数1により、正直な一人が完全検証・承認を終えた正常claimを、他の選出者の不署名だけでは止められない。さらに正常Finalへ到達するには、正しいMinerが元rootに沿って供給・応答し、全合法caseについて§2.4の検査前提・§4.1の有限局所化・§7のChallengerDefault／LocalPredicateHolds等の客観的終結が成立し、必要な実サービスが供給されることを要求する。

受付計画は、悪意者が許された最大数の異なる要求を最後の合法時刻に出し、全手番の締切まで待つ場合を含む。正常Minerの応答側にも全caseを期限内に処理する予約を与え、申立人側の枠だけを確保して「正直者が一人だから終わる」としない。m、第三者上限、case数、最大手数、証拠上限、同時性を確定し、全合法継続が収まらなければ受理しない。

この条件が満たされる正常系では、虚偽caseをそれぞれ客観的に終了し、20が更新後のQと1件以上の完全検証承認からFinalを再評価する。caseを単に時間切れで無視してFinalへ進める規則ではない。前提未確立・実サービス不足のケースは引き続き安全に未検証終了するが、そのAbortを「悪意の係争を解決して正常仕事を完了した」という活性のPASSに含めない。

## 10. 責任・決済・保存

責任は、元の Miner、比較用 trace の署名者、資料 provider、relay、費用の支払者ごとに分離する。
Panel が誤った比較結果を出しただけで全員を算術有罪にしない。虚偽署名・二重署名・提供義務違反は、各々の客観的条件で扱う。
計算有罪、提供義務違反、反証失敗の費用、消費済み service fee、未使用予約額の返還、未終了の別責任を一つの slash にまとめない。
同一証拠・同一 offence の replay で二重徴収、二重報酬、二重返還を起こしてはならない。

元 claim、当事者の署名、固定 root、notice、機会、提出された bytes、裁定、資金変化を再起動・reorg 後も復元できるようにする。
Abort は記録消去ではない。別の責任や許された後続訴追に必要な資料は、署名済みの保持条件に従って残す。
依存 claim への影響は [RFC-v2-0008](0008-claim-dependencies-and-liability.md) に従い、独立 claim を巻き込まない。
保存した「棄却済み」「完了済み」という bool だけで、当時の裁定や義務の根拠を置き換えてはならない。

`Q(C)`、caseの受理順序と版、canonical anchor、残る個別予約・期限、各証拠の検査結果、終端理由、資金のonce markerを認証付きstateと局所資料から復元する。restart／IBDでcase行が未取得なら未解決として扱い、空集合へ降格しない。reorgでは原subject・draw・受理点に対するnativeのundo／replayを行い、孤立枝のreceiptだけで現在枝を承認・棄却しない。元時計・累積予算の復元は新たな猶予や予算の発行ではない。

`ComputationRefuted`によるsource取消は、20・07・08・10の単一経路で、未消費credit、予約permit、依存する未払利益へ一貫して反映する。消費済みcreditを再発行せず、消費provenanceと責任を残す。裁定そのものを、既に受理された全取引の独立した一括rollback命令にはしない。canonical forkが変わる場合だけ、既存の正規reorgで取引と消費をundo／replayする。

予約の不供給、`NoCapacity`、`DeadlineExhausted` は、供給機会や算術有罪の証拠ではない。元の期限内に許された回復を行い、必要条件を完成できなければ未検証で終了する。DAA が進んだだけで、受信・処理・包含の機会が供給されたと判断しない。未解決の delivery、保持、依存、資金責任とその根拠は残し、正常 Final・全額返還・全証拠削除へ自動変換しない。残責任の保存は、新規 notice や original の時計の延長を意味しない。

## 11. 実装接点と互換性

既存の局所 TIR checker、shared material intake、service opportunity、public fold、Batch v5 の時間契約を再利用候補とする。
参考となる実装名は palw_tir_objective_v1、palw_tir_public_fold_v1、palw_court_shared_material_intake_v1、palw_court_service_binding_v1、palw_batch_public_budget_v1 である。
これらの部品の存在は、新しい署名形式・native 受付・終了効果・資金処理の接続完了を意味しない。
旧 RFC-0044 の Panel 生成 witness を、後付けで Miner が署名した original trace に変えてはならない。
新 claim 版から導入し、旧 claim の時間・義務・責任は受理時の版で再検査する。

## 12. 必須の受入確認

| ID | 敵対的・境界ケース | 必要な結果 |
| --- | --- | --- |
| N01 | Miner と通常 Panel が全員結託し、外部にだけ資料を渡さない | outsider の canonical 経路を維持。未履行・未検証を成功へ変換しない |
| N02 | Panel が別 job／別モデルの trace を Miner のものとして出す | binding で拒否し、Miner を処罰しない |
| N03 | 元の両 root を固定した後、途中の区間だけを差し替える | opening／座標／規則の不一致で拒否 |
| N04 | root は不一致だが Panel の再実行側が誤っている | root 不一致だけで Miner を有罪にしない |
| N05 | 最終出力は一致するが trace は異なる | 正常 positive へ流さず、適格な不一致として扱う |
| N06 | 直接反証がそろっている | 不要な対話・positive 完了待ちを要求せず裁定可能 |
| N07 | 局所 PredicateHolds を全体承認として提出する | 全体の承認・Final を与えない |
| N08 | Panel が受領 receipt を出さない／別人の機会を流用する | native の適格応答を維持し、誤った default を拒否 |
| N09 | 正規機会のない通信断・包含妨害 | NoSlashHold／Abort。算術有罪を捏造しない |
| N10 | 最遅 notice、最遅応答、public fallback、再起動が重なる | 同じ a と hard の内側で完了、または規定の未検証終了 |
| N11 | case・鍵・nonce・重複要求で予算や時計を再生成する | 原 original に集計し、更新を拒否 |
| N12 | positive 処理で negative の枠を使い切る | 予約段階で拒否し、既存の公開異議の権利を守る |
| N13 | 同一 offence／決済を再送し、直後に reorg する | 二重効果なし。根拠と残責任を復元 |
| N14 | 元の期限後に新規 notice を出す | 保持期間と受付権を混同せず、その版の既存権利だけを適用 |
| N15 | 全 owner の席を共謀者が占有し、cold outsider が初めて資料を取得する | 元 owner の枠の解放を待たず、認証された共通経路から局所裁定まで進める。実 bytes が全経路で不供給なら成功を止め、立証できる default と未検証終了を分ける |
| N16 | 先読み・並列 worker により一部の取得を重ね、別の箇所では前の応答が必要 | 独立部分だけを重ね、真の逐次依存を期限計算に残す |
| N17 | 複数の攻撃者が非重複の合法 range を要求し、別 claim も同時に係争する | 全合法同時分岐と全 claim の予約を維持し、単一経路の予算で受理しない |
| N18 | 同一 blob への要求を共有するが proof binding・応答者・手番が異なる | bytes の再利用と個別の認証・機会・責任を分離し、他人の履行で誤った default を作らない |
| N19 | 最後の model data・opening・署名が遅れ、計算前に署名した保持契約の残期間が不足 | 受付時の実残期間を検査。旧見積を流用せず、受理済み a と hard を更新しない |
| N20 | reorg replay moves を取得 retry と誤読して予算を省略または二重加算する | 版の手番・再包含・再取得を区別し、根拠のある回復予算と予約だけを使用する |
| N21 | EXEC が停止／credit 不足となり、court の合法応答時刻が到来する | BASE／Heartbeat の予約経路で claim・court・anchor・schedule を進め、次の EXEC を待たない |
| N22 | 予約は存在するが包含・CPU・資料の実供給がなく DAA だけ進む | 自動 slash・成功 Final なし。元時計で未検証終了し、未解決責任と証拠を保持する |
| N23 | 新入口を通さず旧 Frame／Batch・relay・restart fallback を使う | 新 profile の同じ全工程予算・時計・署名責務を検査し、旧長期経路への迂回を拒否する |
| N24 | 悪意Panelが全肯定閾値を占め、一人だけが適格noticeを出す | 定足数やMinerの同意を要求せず、個別caseを`Q(C)`へ保存して成功を止める |
| N25 | 有効な一人の完成済み反証と全員のpositiveが同じcanonical処理バッチにある | 反証を先に裁き、元claimをRefuted／Revokedにする。positiveは覆せない |
| N26 | `now=n`の初回notice、`now=n+1`のFinal、`now=h`の合法terminalが競合 | inclusive受付と20の処理順に従い、最後の合法証拠を失効で捨てない |
| N27 | 攻撃者が他人の個別予約・公開予約を合法要求で消費しようとする | 一人の受理済み救済枠を非収奪にし、全合法同時需要が収まらないoriginalを拒否する |
| N28 | 間違った申立人が無関係な正しい局所passを終端として提出 | 固定争点の正規terminalではないためcaseを閉じず、原手番・予算の範囲で進める |
| N29 | 一つのfalse complaintを棄却し、別の有効caseが残っている | 対象caseだけを閉じ、`Q(C)`を全消去しない |
| N30 | 申立人が客観的な提出機会の下で次手を放棄する | その役割のChallengerDefaultで有限終了。Minerの全計算の肯定にはしない |
| N31 | pruning／restart／IBDでQ(C)の行が取得不能 | 空集合とみなさず成功を停止。認証された元状態から復元する |
| N32 | 同一証拠を別申立人・relay・nonceで再送する | evidence／offenceの一回性を保ち、計算判決と徴収・報酬を重複適用しない |
| N33 | 適法なnoticeを含めない全producer検閲がnativeに現れない | 無条件の検閲検出・単独反証成功を宣言しない。包含前提の破れとして保証を限定する |
| N34 | 誤claimの取消時、未消費permitと既に受理・消費したEXECが混在 | 未消費権利を取消、consumed provenanceを保持。再発行・全取引の自動rollbackなし |
| N35 | 有限のpost-Final訴追窓・閉鎖規則をprofileで未定義にする | activation不可。無期限権利も暗黙の窓ゼロも採用しない |
| N36 | 正当な既存caseへ誤署名／無効opening／別scopeのpacketを送る | packetだけ拒否し、Q・争点・手番を閉じない |
| N37 | n直前のMaterialRequestへn後に資料が届き、その後に矛盾を発見する | 同CaseIdのInspectionPendingと反証権を維持。元scope・予算・期限内で継続し、応答到着だけでFinal不可 |
| N38 | 資料は届くが残りの最悪検査・裁定時間を確保できない | 検査未完了を本人defaultや成功にせず、元時計で回復／未検証終了 |
| N39 | 悪意者が同じ資料要求を先着し、後の正直な請求人が参加する | MaterialIdは共有してもCaseIdと予約・終了権は各人別。悪意者defaultで正直なcaseは消えない |
| N40 | 唯一の正直な選出者がモデル未保有で、他者だけが資料を得ている | 入口manifestから適格要求を開始し、算術反証・比較rootの先行提出を要求しない |
| N41 | 少数のchunkだけを応答し、要求quota消尽を全資料受領と扱おうとする | 全scopeの検査前提成立にしない。正直な申立人のdefaultを作らず、成功を保留 |
| N42 | 全検査前提と本人の提出機会が客観的に成立した後、悪意申立人が何も提出しない | 署名済み期限と次手に従ってそのcaseだけをChallengerDefaultで終了 |
| N43 | URL・provider receipt・別Panelの承認だけで検査前提成立recordを作る | 根拠不足として拒否し、Q・InspectionPendingを維持 |
| N44 | 正常Minerに対し悪意全員が最大の合法caseと遅延を使い、正直な一人だけが承認 | 全caseの客観的終結と実サービスの前提の下で、元hard内に解決して正常Final。Abortだけを活性PASSにしない |
| N45 | cold取得に失敗した正直者を、モデル保有者の味方へ置換する | 再抽選・資格変更を拒否し、元の予約と資料要求を維持 |
| N46 | 小さな公開応答を反復し、合計でNodeへ全モデルを流して供給を救済する | claim全体のbytes・work上限とモデル非保有境界を適用し、迂回を拒否 |
| N47 | 大資料の公開と虚偽未着を有界に客観終結する機構が未確立 | 正常完了profileとしての有効化不可。最低1承認で不足を隠さない |
| N48 | batchの一leafへの反証が成立し、他leafのpositiveだけでbatchを成功させる | 20のatomic batch全体の成功を止める。別の独立claimのcase・権利は巻き込まない |

この表は必要な検査項目であり、PASS の実測記録ではない。新しい native 経路の実装・資源測定・互換性・回復試験と、有効化判断は別に記録する。

DAA 予算膨張の解消判定には、[05](0005-bounded-resources-and-long-context.md) の受付安全性 gate と有用性・完了 gate を両方使用する。cold outsider の資料取得前から開始し、対象実モデル、全合法分岐、共用資源、未完了の分母まで記録する。debug の局所 PASS や、超過した全 claim の拒否だけを、実モデルの公開裁定が完了した証拠にしない。
