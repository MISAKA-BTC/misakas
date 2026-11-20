# RFC-0056／0057／0058 独立設計レビュー

日付: 2026-11-17  
対象: SWC1設計案と、CAD1／CPN1への版付き接続  
状態: **DESIGN_REVIEW_COMPLETE_WITH_ACTIVATION_GATES_OPEN**。今回列挙した文書上の指摘は修正を確認した。提案の検査であり、採択・native実装・性能・activation・PoW成立の認定ではない。

## 1. 対象と判断の境界

[RFC-0058](../../../rfc/0058-staged-work-commitments-and-consensus-eligibility.md)を、既存の[0056](../../../rfc/0056-global-claim-admission-difficulty.md)・[0057](../../../rfc/0057-court-nonce-and-panel-challenges.md)、RFC-v2-0016〜0020、RFC-0049／0050、root／docsのAGENTS、PRINCIPLES、DNS-BFT退役とPQ-onlyの履歴と照合した。ユーザーが添付した443行の監査対処案も入力に含む。

白書PDF自体は今回の入力にない。白書のページや定理を直接査読したとの主張はしない。現行sourceに関する結論は、別担当による[固定source監査](SOURCE_AUDIT.md)と区別する。本レビューはコードの実行、BLS／DKGの暗号監査、実LLM測定、DAG証明を行ったものではない。

主な判断は次のとおり。

- 完全Originalの固定と未知の後続乱数、一仕事一抽選、元rootへ束縛したCourt、成熟前の成功権利保留を組み合わせる方向は、提示された再抽選・差替え攻撃への具体的な状態契約になる。
- bindingから物理的な全推論実施や計算量下界を導かないこと、BFT委員会と一人の正直Panelを分けること、MatureEligibleと台帳settledを分けることを維持する。
- 以下に記録する本文の整合修正と、§5の有効化条件は別である。整合修正後も、実ネットワークで安全に有効化できるとの結論にはならない。

## 2. 重要な反例と本文への反映

| ID | 反例・問題 | 必要な処理／確認対象 | 現在のレビュー状態 |
| --- | --- | --- | --- |
| R01 | 同じ実行について別の有資金TrialSlotを得れば追加抽選できる | 経済的TrialUseと、canonical計算記述のWorkUseを分ける。CAD1単独の旧§4.2をSWC1へ無条件流用しない | 0058 §2〜3、0056 §4.2／16の対応を確認 |
| R02 | WorkKeyにroot・署名・epoch・総policy hashを入れ、外装変更で別仕事にする | WorkKeyは実行意味に限定する。結果、証拠搬送／保存、旧wire包装はOriginal側へ束縛し、policy更新で元一回性を失わせない | 0058 §2.3で意味を変えないtrace木layout等を除外したことを確認 |
| R03 | 無資金の第三者が偽rootを先着させ、正当な仕事を永久に予約する | R_job前の有資金登録とowner認証を必須にする。期待WorkKeyはnative導出とし、任意hash予約を認めない。競合は固定登録順で解決する | 0058 §2.2／3.1に反映 |
| R04 | 登録ownerが偽rootを固定し、落選／反証／cancel後に同じ仕事を再投入する | 最初のOriginalと唯一のtargetを保持し、失敗でもWorkUseを未使用へ戻さない。新epoch／別署名／別routeを再抽選にしない | 0058 §3に反映 |
| R05 | seedは変わるが計算入力は同じで、外側hashだけが変わる | JobForSeedを先に固定し、seedを規範上の実入力／初期状態へ反映する。Nodeの導出を自己申告rootで代用しない | 0058 §2.3の有界導出・自己申告root禁止まで確認 |
| R06 | 公開前に攻撃者だけがthreshold shareから乱数を復元し、集合を追加する | 未知性の締切は一般公開時刻ではなく、敵対者が復元可能になる最初の時点。正直shareは有効なfreeze後にのみ出す | 0058 §4／6に反映 |
| R07 | 別key・別round・別集合・署名subset・fallbackから有利な乱数を選ぶ | descriptor／key／slotを固定し、一意出力とStageUseを要求する。障害では新規受付を止め、同じ仕事を次seedへ移さない | 0058 §6に反映 |
| R08 | 既知のR_ticketをhashし直してPanel用未知乱数とし、受理対象を選別する | Panel対象集合を閉じた後の別の未知R_panelを使う | 0058 §4／7、0057 §5／16と修正後W11で確認 |
| R09 | BFT委員会に正直者がいるので、別集合から選ぶPanelにも正直者がいるとみなす | BFT quorum／beacon thresholdと、Panel poolの敵対質量・能動的完全検証条件を分離する | 0058 §6.2／7.1に反映 |
| R10 | 他人の正常な局所openingをコピーすれば、Minerの別rootや全体誤りを正当化できる | 元Original／元rootを変更不可とし、共有bytesと本人の承認責任を分ける。単独反証／Qをpositive数で相殺しない | 0058 §7、0057の既存責務で維持 |
| R11 | 長期責任や未解決caseが残る仕事を早期にweight／EXECへ使い、後の反証を逃れる | 新profileの用途別成熟、同batchの反証優先、全consumerの現在状態再検査、一回消費を接続する | 0058 §9〜10に反映 |
| R12 | 古いWorkUseをpruneして、過去の同一計算を新epochへ再投入する | exactな歴史的一回性を認証済みstate／summaryへ残す。欠落を未使用と解釈しない | 0058 §3.2でexact summary、復元不能時hold、全履歴一定との混同禁止を確認 |
| R13 | 零重み・共通prefix・batch・cache等で、正しい完全rootを安価に構成する | 正しさの違反とはしない。計算費用下界／非償却性が未証明であることを残す | 0058 §0／11に明記 |
| R14 | 成熟やBFT乱数の成立を理由に、DAGのcommon-prefix／台帳確定まで成立したとする | MatureEligibleとsettled、乱数用合意と台帳決済を分ける。DAG定理を独立gateにする | 0058 §6.3／9／12〜13に明記 |
| R15 | 共通Court contextがR_panelを必須にし、beacon停止中の資料要求／negativeまで止める | positive専用contextとAction専用contextを分け、後者へ未来Panel seedを要求しない | 0057 §16のSWC1PositiveContext／SWC1ActionContextと0058 §7で修正確認 |
| R16 | JobSeed／FreezeIdのdomainや連結順が曖昧で、異なる実装が別bytesを採る | LP・固定domain・NR・順序を明記し、stage集合のrootもcanonicalにする | 0058 §2.3／6.1の具体式・stage値・集合codecを確認 |
| R17 | 登録hashが、その登録を含む未来freezeの完全IDを参照して循環する | 事前固定はkey世代／slot／選択規則。集合root→FreezeId→JobSeed／ticketの向きを守る | 0058 §2.2の自己参照禁止を確認 |
| R18 | 落選leafを別leafと再包装し、新batchの仕事として抽選し直す | 各execution leaf／rangeのWorkKeyと仕事量onceを保持し、未対応adapterをdefault denyにする | 0058 §3.3で確認 |
| R19 | positive contextはR_panelを束縛していても、nonceの外側beaconを別seedへ替える | PositiveNonceへ渡すbeacon参照とseedを、同じPANEL stageのものへ一致させる | 0057 §16で明記を確認 |
| R20 | 文書／実装試験のPASSだけで本番権利を解放、又は未証明下界を理由に旧経済を停止する | 実装証拠、本番合意の安全性判断、PoW費用下界の主張を分ける | 0058 §13のG1〜G5／G6／G7の区分を確認 |

### 2.1 指摘した本文内の明確な矛盾

最初の草稿のW11は「ticket判明後にPanel対象／pool／roundを選択」を不適合としていた。しかし正規順序は、R_ticketで候補を選別し、受理対象とpoolを固定し、その後にR_panelを得る。従って禁止すべきなのはR_panel判明後の対象選択、又は既知R_ticketをPanel乱数にも流用してから行う選択である。修正後のW11と0057 §5はこの順序に一致する。

また最初のSWC1CourtContextはpositiveとnegativeの双方へR_panelを要求し得る形だった。修正後は専用のAction contextが未来Panel seedを含まず、適格第三者と既受理caseの進行をbeacon待ちへ巻き込まない。selected Panel専用義務が正規assignment後に生じることは別に定義されている。

### 2.2 NodeのWorkKey導出

WorkUseの一回性が意味を持つには、受理したJobRegistrationとR_jobから期待WorkKeyを認証できなければならない。ownerが任意のactual-input rootを提出し、それを期待値として保存するだけでは、偽キーによる再抽選を防げない。

JobForSeedの共通profileは、既存入力commit、seedを反映する入力構造、必要な認証経路、数値規則を固定し、有界なNode処理で期待commitを導出・検査できる必要がある。Nodeへ巨大なtokenize、全モデル取得、全推論を追加する設計にはしない。実際の数値意味との対応は、元OriginalのInit／Step／EndとPanel／courtで検査する。この手順と費用上界が未実装なら、自己申告boolで補わず有効化gateに残す。

## 3. 採択済み境界との整合

### 3.1 DNS-BFTとPQ-only

PRINCIPLESのDNS-BFT退役、RFC-v2-0010の追加決済authorityを設けない境界、0017のPanel選出に別BFT委員会を導入しない規則を保持する。RFC58はthreshold BLS参照構成を**DISABLED**とし、現行native beaconがすでに同じ性質を満たすと記載していない。

既存のML-DSA認証と、候補BLS beaconの乱数安全性は別である。外側にML-DSA署名を追加しても、BLS公開鍵から乱数を予測できる攻撃への耐性は変わらない。新方式の明示的な方針判断と完全なfault modelを要し、このレビューだけで例外採択を行わない。

参考となる一次資料は[drand Cryptography](https://docs.drand.love/docs/cryptography/)、[Security Model](https://docs.drand.love/docs/security-model/)、[Timelock Security Assumptions](https://docs.drand.love/docs/timelock-encryption/)。固定key／messageに対する一意性、早期取得、停止、threshold侵害、非耐量子の制約を区別して読む。drandの運営集合や設置手順を、そのままMISAKAのpermissionless保証とみなさない。

### 3.2 モデルとPanel

モデル別の品質・性能・保有者・公開性審査、登録tier変更を新設しない。WorkKeyのcontent正規化は、同じcanonical内容のaliasを識別する規則であり、意味的に等価な全programを認定する審査ではない。

PublicOneHonestの最低完全承認は1件であるが、全選出者の完全検証責務、必要資料、正直な一人の包含・反証機会は残る。個別sampleを提出したことから、全体を本人が実行したとは結論しない。5人という数を安全性の保証や固定court件数へ変換しない。

### 3.3 既存認証と新成熟gate

RFC-v2-0020は既に、検証前Finalの禁止、未解決Qによるhold、同batchの反証優先、現在のcredit資格再検査を要求している。今回の追加は、新stage／WorkKeyと新profileの成功権利を用途別成熟へ接続する差分として扱う。旧Final enumや旧署名、前のcanonical batchで消費済みの権利を、新しい状態名だけで遡及変更しない。

## 4. 有限資源と再起動・reorg

JobRegistration／Original presealの有限なcontrol費用と、当選後の全court責任予約を分ける必要がある。全presealへ将来courtを無条件予約すると、Dで選別する前にその容量を占有できる。一方、後続ticket／Panelを理由に受理済みclaimの原時計をresetしてはならない。初回受理a、各stageの固定cut／expiry、資料・異議・責任窓の関係を版付きで定める。

一回性の保持は、全履歴をRAMへ置くこととは別である。live rows、membership／absence検査、更新、IBD、journal、認証済みsummaryを0055のstock／flowへ含める。履歴rootがあるだけで全文の取得が保証されるわけではなく、必要状態を認証できない場合に未使用へ降格しない。Bloom filterや単なるrolling cacheはexactな再使用拒否の代替にならない。

全参加者の総履歴bytesが永久一定になるとの保証はしない。closed epochを理由に一回性記録まで削除する最適化には、過去登録の再入不能、canonical job写像、世代間正規化、正規checkpointとreorgの範囲を別に示す必要がある。現在のローカルDBだけで全fork横断の一回性や不可逆性を主張しない。

120秒は受付Dの制御epochであり、3段階乱数と全検証の完了時間ではない。複数modelの処理は共通予算内で並行できるが、1 BPSは全carrierの目標であり、1秒Finalや1秒settledを意味しない。

## 5. 修正後にも残す有効化条件

| 境界 | この文書レビューでは満たせないもの |
| --- | --- |
| Beaconとstable freeze | 適合する実方式、DKG／鍵／committee管理、早期復元耐性、停止／復旧、非循環なanchor、方針適合 |
| Native全経路 | Studioから登録・Original・Panel・court・権利消費までの同版実装、全header／DAA／weight consumerの非迂回性 |
| 資料と正常完了 | 選択的秘匿と虚偽未着を有界に区別・終結する実手順、cold Panel、保護された実供給と包含 |
| Node容量 | 最大合法case、1万〜数万model、control費用、全stageのstock／flow、回復、実測1 BPS |
| 計算費用 | 最適化・償却を許した実費下界、規範workに対する普遍的比例、本人の物理実行 |
| DAG合意 | 認証遅延・反証・成熟・分断・依存・既消費creditを含むcommon-prefix、収束、活性 |

hashのbinding、段階乱数の未知性・一意性、canonicalな状態遷移を仮定すれば、一つの承認されたWorkKeyから複数の認められた新規仕事受付ticketを作らないという条件付き主張はできる。既存receipt quantum等の後続用途は、その有限量会計を維持する。一仕事一抽選を、既存の全用途を一つの処理へ統合する意味にはしない。それ以上の計算費用や合意安全性を、hash入力へのroot追加、nonce、threshold署名、文書テストから導かない。

## 6. 最終見直し

0056 §4.2／5／6／16、0057 §5／16、0058の全文と追補、ADR-0226を照合した。今回報告した文書上の矛盾と曖昧さは、上表のとおり本文へ修正された。WorkKey意味・二層受付・exact履歴・seed待ちを作らないAction・batchの粒度・canonicalなJobSeed／freeze・自己参照防止が一つの提案として接続している。

採択すべき具体的なprofile値、native adapter、beacon方式、実供給手順、実機上限、経済・DAG安全性の根拠は未完了である。これらを文書の整合確認でPASSにしない。特にG1〜G5の接続証拠だけでは本番合意weight／権利を解放せず、実際の敵対者・経済前提とG7の安全性判断を要する。G6はPoW費用下界の成立を主張する根拠として別に残る。

抽象検査・文書検査の実行結果は別の検証記録が所有する。本レビューではその件数や実行PASSを先に認定しない。最終パッケージのhashと入力との差分は改訂manifestが所有する。

## 7. 統合toy modelの追加レビュー

[check_work_model.py](check_work_model.py)と[model-results.json](model-results.json)を追加で読み取り確認した。対象は抽象的な状態契約であり、ここで発見した不備をMISAKA nativeコードの脆弱性とみなさない。SHA-256やopeningの計算が実際に行われることと、native codec・署名・storage・LLM・DA・合意が実装されることは別である。

最初のモデルでは、次の3群・4経路がテスト群の外に残っていた。モデル担当が修正した後、当該4経路だけを独立に再実行して拒否を確認した。大規模な追加探索、実機試験、native試験は行っていない。

| ID | toy model内の反例 | 修正後の独立再確認 |
| --- | --- | --- |
| M01a | 承認0件のclaimをnotice→resolve_no_fault→mintへ進めると、ケース終結だけで成熟できる | `not-mature-eligible`で拒否。承認数0、`VerificationPending`を維持 |
| M01b | `AbortedUnverified`へ進めたclaimにnoticeを出し、ケース終結から復活させる | `notice-state`で拒否。`AbortedUnverified`を維持 |
| M02 | GC後、同じcapital／sequenceのjob_slotだけを更新して再登録し、経済的機会を再利用する | 永続する`trial_highwater`により`trial-sequence-reused`で拒否 |
| M03 | checkpointのrecordから登録への参照だけを改変し、同じ認証digestで復元する | recordの`reg`参照をdigestへ追加し、`checkpoint-authentication`で拒否 |

独立再確認したスクリプトのSHA-256は`2653679f7938bc3349022395792d64ec83535faa1cddb7fa167623a360f71060`。読み取った結果ファイルも同じscript hashを記録し、`scope = abstract-model-only`、12件中12件PASSであった。その時点の56／57／58のsource hashも結果ファイルと一致した。結果ファイルは本レビューでは書き換えていない。

モデルはBLS／DKG、native署名・DB・LLM、network DA、DAG定理、PoW費用下界、非償却性、Studio native経路、実1 BPS、1万model負荷を明示的に未検証としている。既知seedのdomain分離、正しい零重みモデルの安価なtrace構成、局所sampleと完全再実行の違い、仕事の成熟と台帳settledの違いを、成立しない主張として記録している。これらの否定例を安全性定理のPASSへ変えている記述は確認していない。

12件のPASSは列挙したfixtureケースの結果であり、全入力・全状態遷移・敵対的ネットワークを網羅した保証ではない。特に信頼済みcheckpoint、経済台帳、beaconの知識／公開時刻、完全資料の取得はfixture前提のまま残る。
