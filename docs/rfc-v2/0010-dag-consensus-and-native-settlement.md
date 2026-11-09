# RFC-v2-0010 — DAG合意・EXECスケジュール・Native EVM決済

## 2026-11-09公開資料・一人前提の追加境界

[採択RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)と[ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md)により、独立選出されたPanelの中の一人が、モデルを事前保有せずに公開資料を取得して全体検証できる新profileを採択する。一人の完全承認と保護された全検査・異議機会を20の成功gateへ接続し、同batchの反証・Q holdを全credit consumerより先に反映する。EXEC 1 BPSをclaim認証件数やcourt容量の保証にしない。 成功の規範所有者は[20](0020-verification-gated-final-and-adversarial-models.md)、公開方針は[v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)。以下の先行採択・基礎契約と衝突する箇所は、この新profileに限って本境界と16〜20を適用する。旧claim・歴史付録は保持し、状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

## 2026-11-09新profileの追加境界

既発行ExecCredit・schedule・permitにも[20 §5.2](0020-verification-gated-final-and-adversarial-models.md#52-同じcanonical処理単位での反証優先と終端)の優先順を適用する。同じBASE／native batchの反証・Qによるhold・依存失効を反映してから、候補EXECのcanonical受理と`reserved -> consumed`を判定する。「既受理consumed」を保持する対象はそのbatchより前のcanonical消費であり、候補の署名・受信・provisional実行・batch内の前方配置ではない。無効となった同batch候補から取引効果やfeeを先に外へ出さない。既受理取引の一括rollback、slotの再抽選、元expiryの延長は追加しない。

[採択RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)と[ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)により、[20](0020-verification-gated-final-and-adversarial-models.md)の実行整合性・一人の反証・検証未成立時の成功停止を適用する。同blockの異議・反証・失効を成功と経済効果より先に評価し、credit取消を既存取引の一括rollbackへ読み替えない。EXECの1 BPS契約は維持する。 詳細の規範所有者は16〜20であり、本追記で別の成功条件を定義しない。旧claim・歴史付録の意味とbytesは保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

原統合: 2026-10-29 / 採択改訂: 2026-11-08 / 原入力: `misakas-pre(5).zip`

[20本の索引](README.md) · [旧→新対応表](MAPPING.md) · [編集・移行規則](MIGRATION.md)

> **2026-10-29原統合の編集範囲:** 文書の統合・重複する責務の整理。技術仕様の採択、パラメーター変更、runtime有効化、試験成功の宣言ではない。
> **状態:** 採用済み・Draft・隔離実装・履歴を含む統合編集版。各条項の採択・置換関係を継承する。過去の「実装済み」「休眠」「未試験」等は、その記録時点の状態である。
> **番号:** 本書は`RFC-v2`。原条文中の`RFC-xxxx`と`§`は旧版を指す。旧番号・wire・署名・fence名は再採番しない。

## 2026-11-08採択による適用範囲

新profileは[20](0020-verification-gated-final-and-adversarial-models.md)のResultCommitted後にcommit後乱数でglobal verifierを選び、Finalを要求しない事前認証predicateからCertificationReadyへ進む。必要条件成立後に成功Finalを与え、その後だけcreditを消費する。旧Gate CのFinal後認証と別版にし、循環待ちを作らない。

**優先関係:** 2026-11-08の先行採択はADR-0215と公開裁定v0.2を根拠とする。今回の公開資料・一人前提の新profileには冒頭の0050／0218・RFC16〜20・[公開裁定v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)を優先する。以下の基礎契約・旧profile記録と原付録は保持し、旧署名や旧Finalの意味を遡及変更しない。設計採択は実装・測定・有効化の完了を意味しない。

**EXEC継続と1 BPSの追加採択:** [RFC-0048](../rfc/0048-exec-credit-schedule-and-one-bps.md) / [ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)に基づき、認証済みworkから有限期間のcreditを作り、事前公開した未来slotのscheduleを`ScheduleId`で直接参照する。正常時の目標は**全モデル共通のEXEC_TXについて1秒slot当たり1 canonical block、1 BPS**とする。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。これは実測値、1秒settlement、preset変更、既存wireの再解釈を意味しない。

creditの発行・予約・消費・失効は[07](0007-claims-exec-and-answer-accounting.md)、scheduleとslot・canonical EXEC順序は本書が所有する。新規RFCを現行の21冊目にせず、閲覧入口は20冊を維持する。以下の新schedule規則は新profileだけに適用し、原条文付録のbytes・過去の採択状態は保持する。

**原文収録元:** [旧0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md)、[旧0018](../rfc/0018-dag-parallel-multi-model-consensus.md)。

**関連RFC:** [04 モデル二階層・年次カタログ](0004-model-tiers-and-annual-catalog.md) / [06 Panel参加・抽選・確率的検証・認証Receipt](0006-panels-and-probabilistic-verification.md) / [07 Claim・EXEC・仕事量・回答会計](0007-claims-exec-and-answer-accounting.md) / [08 Claim依存・Prefix生成元・未解決責任](0008-claim-dependencies-and-liability.md) / [11 Bond予算・配分・Fee同意・決済会計](0011-bond-budgets-fees-and-payments.md) / [12 認証State・保存・Pruning・IBD](0012-authenticated-state-storage-and-ibd.md) / [13 公開Bootstrap・Genesis・有効化とRelease接続](0013-permissionless-bootstrap-and-release.md) / [14 Court統合契約](0014-court-master-contract.md) / [15 設計凍結・延期・不採用・未採択・変更管理](0015-freeze-deferred-rejected-and-governance.md)。

## 1. 一つの合意Authority

旧0012のPALW-only consensus／native EVM settlementと旧0018のDAG並行生成／Gate Cを継承する。PALWの承認・workからfork choiceと決済を進め、EXECを同じ鎖の取引実行laneとして残す。別chain、BFT委員会、DNS validator overlay、独立の最終承認者を追加しない。退役・履歴は[15](0015-freeze-deferred-rejected-and-governance.md)を参照する。

| 役割 | 本書の境界 |
| --- | --- |
| PALWの基礎となるcanonical履歴 | scheduleのauthority、claim認証、native台帳、EXECの受理順序・決済を決める |
| EXEC_TX | 正規permitで取引を運ぶ。生成自体でPALW weight・DAA・settlement workを追加しない |
| EXEC_SLICE | 適用版が許すwork資料を運ぶ。sliceだけでEXEC_TX permitや新たな二重creditを得ない |
| Heartbeat / BASE経路 | 適用版の独立したliveness・object搬送を維持する。tickだけを認証workに換算しない |

Nodeはモデル本体、全推論、GPU、外部モデルURLを必要としない。承認、native状態、署名、slot規則、有界なcourt資料、通常の取引実行を検査する。[19](0019-model-free-node-and-bounded-evidence.md)の資源上限はEXEC負荷下でも維持する。

## 2. 認証と裁定を分ける

新claim版は[20](0020-verification-gated-final-and-adversarial-models.md)の事前認証、公開異議、依存、資金条件を満たして成功Finalへ進む。その後、[07](0007-claims-exec-and-answer-accounting.md)の適格性・成熟・有効期限を満たしたcreditだけをschedule候補にできる。`CertificationReady`、局所positive、モデルtier、local replay、単なるFinalという名前からcreditを作らない。

global poolから選ばれた検証者の正直さという前提は残る。未共有モデル、モデル内Miner／Panelの全結託を想定し、資料不足を成功認証へ変換しない。計算不正の有罪判定は[14](0014-court-master-contract.md)・[18](0018-interactive-negative-court-and-defaults.md)の公開証拠と決定論的courtに従う。認証は全計算の暗号学的証明でも、将来の取消がない保証でもない。

旧profileのGate C・公開裁定v0.1〜v0.3は旧履歴として保持し、今回の公開資料・一人前提の新claim版には[v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)を適用する。暗号学的計算証明の禁止、二階層、公開検証・原時計・責任上限は変更しない。

## 3. DAGと並行進行

複数モデルの生成、sibling attribution、Panel anchorの公開参加、Rule E、DAG幅は、それぞれ独立した条件・fenceを持つ。G14限定集合が動いたことを、全DAG機能の有効化へ読み替えない。

**1 BPSはEXEC_TXの正常時目標**である。slot幅は1秒、正規枠は全モデルを通じてglobalに一つとする。モデルごと1 BPS、claimごと1 BPS、REAL・Heartbeat・EXEC_SLICEを含めた総ブロック数の目標に読み替えない。複数候補が流通しても同じglobal slotに複数のcanonical EXEC_TXを受理しない。

credit不足、未確定seed、schedule未公開、割当先停止、ネットワーク分断、資源不足では空slotがあり得る。slotを飛ばし、過去slotの一括生成、無根拠permit、Heartbeatのcredit化で埋めない。operator集中制約など既存の安全条件がslotを空にする場合も、その条件を1 BPSのために迂回しない。

## 4. Native EVM決済

nativeの受理済み仕事から正規の仕事量、anchor、成熟、operator／集中度、取消条件を導出する。仕事namespaceごとのmappingはsource identityと実際の受理点を保持する。EXEC候補の独立した到着順や署名数を新しいfork choiceにしない。

| 取引状態 | 意味と必要条件 |
| --- | --- |
| `EXEC included` | 署名されたEXEC候補に取引が含まれる。Nodeの検証・canonical採用・決済の完了を意味しない |
| `provisional` | 対象schedule・permit・取引を検査し、指定した暫定state上で実行した候補。競合、後着、取消、reorgで採用されない場合がある |
| `BASE accepted` | canonical BASEの共通順序によってEXEC・取引が受理され、permit消費とUTXO/EVM更新を同一native foldへ反映した状態 |
| `settled` | BASE acceptedの連続prefixについて、既存の版付きsettlement policyが必要work・成熟・depth・依存・責任条件を満たすと認証した状態 |

RPC・walletは四状態と対象block/slot/prefix/policy/generationを区別する。単なる時刻経過、EXECの署名、空slot、1 BPS達成から`safe`・`finalized`・`settled`を導かない。後日の認証撤回とcanonical reorgも別事象として表示・記録する。

1秒slotは高速な取引搬送の周期であり、1秒の不可逆finalityを約束しない。settlement policyの既存要件を弱めず、保証範囲とglobal検証者の前提を維持する。

## 5. 互換性と受入

旧ネットワークの履歴、既存credit、permit、bond権利、accepted prefixを消さず、overlayの停止・返還・移行は既存方針に従う。新scheduleのschema・署名domain・clock profile・fence・activation高さは未割当とする。旧slotや旧署名を新しい`ScheduleId`形式として再解釈しない。

新規則が既存の「最新parent stateからscheduleを探す」経路と異なる部分は、writer・reader・undo・snapshot・IBD・RPCを同じ版で切り替える。既存型にフィールドを足すだけで意味が保たれるとは扱わない。以下の概念名は実装済みAPIや割当済みwire IDの宣言ではない。

## 6. ScheduleIdが指す固定subject

新profileのEXECは、直接の`ScheduleId`と自身のglobal slotを必ず指定する。NodeはそのIDが指すcanonicalなschedule commitを取得・認証し、現在のtipから「最新schedule」を推測して代用してはならない。

| subject | 固定する内容 |
| --- | --- |
| network / profile | genesis・network identity、schema、active fence、allocation / clock / inclusion policyの識別子 |
| epoch / authority | globalな対象epoch、このproposalの予約・schedule commitを含まない先行canonical cutのBASE anchor identityとstate root、候補集合の固定記録 |
| eligible集合 | canonicalなcredit unit ID、元work・operator・bond・有効期限を含む認証済み集合rootとcanonical順序 |
| seed | 集合固定後に確定するseed anchorのidentity、seed値と導出規則 |
| assignment | ScheduleIdに依存しないleaf `(global_slot, permit_position, producer, credit_unit_id)` のcanonical集合を認証するallocation root |
| 時間境界 | 絶対的な`start_slot`、排他的な`end_slot`、包含期限・slot閉鎖規則、clock profile |

seed前の固定参照を`ScheduleProposalId`とし、network・profile・epoch・authority・eligible集合・未来slot区間・各policyをcanonical encodingでhashする。`ScheduleId`はdomain separationの下で、このProposalId、seed anchor/value、allocation rootを束縛する。hashは資料の同一性を認証し、work計算全体の正当性を証明するものではない。

authorityの入力は予約前の既知canonical cutであり、proposalやscheduleを収録するtransaction／block自身のhash、その適用後state rootを自分のID材料に入れない。予約とschedule commitの実際の収録block・適用後rootは、native受理recordと包含証拠としてIDの外で保持する。使用時には入力cutのcanonical性だけでなく、proposalの正規予約とschedule commitが現枝で受理済みであることも検査する。

allocation leafには、これから導出するScheduleIdやPermitIdを含めない。依存順序は`先行cut → ScheduleProposalId → 後続seedとallocation root → ScheduleId → PermitId`とする。Nodeはleafのopeningを検査し、確定したScheduleIdとleafからPermitIdを再導出する。IDを含む木からそのIDを作る循環hashを許さない。

`ScheduleId`を変えて同じglobal slotを増設できない。native状態はepochとglobal slot範囲の一意性を検査し、同じcanonical枝に重複する有効scheduleを受理しない。scheduleの件数・slot数・unit数・証拠bytesは有限のprofile上限内に置く。Nodeはcommit時に決定論的割当・一意性を有界な処理で検査する。単独のopeningだけで集合全体の非重複が証明されたとは扱わない。

`PermitId`はnetwork/profile、ScheduleId、global slot、固定されたpermit位置、割当producer、credit unitから一意に導出する。二重署名の衝突単位とdurable journalのkeyはこのPermitIdであり、payload digestやattachmentをkeyへ加えて別permitにしない。署名メッセージにはそれらを含め、同じPermitIdの異なる署名対象を検出できるようにする。

## 7. 固定・抽選・公開・開始の順序

1. **Eligible集合を固定する。** 合意で指定したcutの認証済みnative状態から、未失効・未消費・他schedule未予約のcredit unitを取り出す。[07](0007-claims-exec-and-answer-accounting.md)のMintRecordと数量保存に従い、同じunitを`ScheduleProposalId`へ予約する。
2. **後続seedを確定する。** 集合固定時には未知だったcanonical seed anchorを、版付きの決定論的規則で選ぶ。header nonce、任意timestamp、producerが選べる最新tip、ローカル乱数を再抽選源にしない。seed供給者のgrinding・公開拒否リスクと利用する乱数前提は評価・公開する。
3. **一意に一回割り当てる。** 固定集合・seed・policyから全割当を再現する。各permitは一つのslot・producer・credit unitへ束縛し、各unitは同時に一つの有効割当だけを持つ。割当を見て集合・slot区間・epochを選び直さない。
4. **canonicalにcommitし公開する。** native受理によってProposalIdとScheduleIdの対応、allocation root、原資予約、時間境界を確定する。必要なscheduleデータ・openingを開始前に公開し、Node・割当producerが取得して検査できる状態にする。
5. **未来のstart_slotから使用する。** commit・公開の必要先行期間を満たす未来slotだけを許可する。遅れたcommitで過去slotへ遡らず、開始条件を満たさないepochは使用しない。

候補予約とcommitの前後で適格性を再確認する。固定後の失効・取消は対応unitを無効化し、残りの割当のseedや位置を引き直さない。未選択unitの返却、失敗したproposalの解放、再試行上限は[07](0007-claims-exec-and-answer-accounting.md)に従い、同じunit ID・元のexpiry・累積attemptを維持する。

seed遅延・schedule欠落はcredit発行や公開異議の時計をresetしない。空slotや公開不足を理由に、Miner／Panelの追加署名だけで臨時permitを発行してはならない。

## 8. DAA・秒slot・包含期限を分ける

| 時計・区間 | 用途 | 禁止する混同 |
| --- | --- | --- |
| DAAのcollection / freeze / seed / commit境界 | canonical状態の順序、credit予約、原時計と責任期限 | DAAを単純に秒数へ置き換えない |
| 絶対秒slot `[start_slot, end_slot)` | producerへ事前割当した1秒ごとの生成機会 | 毎slot、新たなBASE blockやDAA更新を待たない |
| inclusion / closure window | 遅延候補の収集・canonical収録とslotの閉鎖 | 生成期間の終了だけで、期限内の遅延候補を一律拒否しない |
| credit expiry / liability retention | 発行済み権利の寿命、資料保持と責任追跡 | schedule作成・遅延収録で元の寿命やcourt期限を延長しない |

合意でcommitしたepoch originとclock profileから、`slot = floor((consensus_time - origin) / 1秒)`の絶対slot区間を定義する。ここで`consensus_time`の入力、DAA境界との対応、許容timestamp、未来時刻の扱い、epoch間の重複禁止、停止・再開時の境界は版付き合意規則で固定する。DAAの進み方を1秒と仮定しない。

producerはcommit済み時計を基準にlocal timerで起床し、現在slotを選ぶ。**local wall clockは合意の正本ではない。** validatorの恒久的なaccept/rejectはcanonicalなtime context、blockの認証済み対象slot、包含・閉鎖境界から再現し、検査したPCの現在時刻で過去ブロックのvalidityを変えない。初期受信時の未来時刻保留は、永続的な不正認定と区別する。

信用の期限にDAAと時刻の両方を使うprofileでは、適用条件と比較順を明示し、暗黙の換算や都合のよい片方を使わない。slot終了後に遅延包含を許す場合、その許可・収録上限・元expiryとの関係を予約時から固定する。slot使用期間、包含猶予、資料保持の数値は本採択で捏造せず、activation gateで実測・承認する。

## 9. Work anchor・producer・attachmentの分離

**work anchor**はcreditの元となる認証済みworkを特定する。**schedule authority anchor**は集合固定とschedule commitのcanonicalな根拠である。**EXEC attachment**は取引候補を実際に収録するBASE側の依存・parent・mergeset関係である。三つのidentityと有効性を別フィールド・別predicateで検査し、attachmentを変えて元の仕事や割当を選び直さない。

slot producerはscheduleに記録した署名者であり、その場の推論完了やwork anchorを収録したBASE producerの応答を待たない。BASE producerとの一致を要求せず、creditに固定したowner／bond／operatorに対する資格を個別認証する。[07](0007-claims-exec-and-answer-accounting.md)の非譲渡性を維持し、schedule作成を別ownerへの移転や任意のproducer付替えに使わない。許された署名鍵管理の範囲も所有権移転と区別する。

通常のtip伸長ではScheduleId、seed、assignmentを維持する。使用時にはcanonical membership、元creditの失効・取消、permit未消費、attachmentの適法性を最新の適用stateで再確認する。authority anchorがcanonical枝から外れるreorgでは、その枝のscheduleを無条件に流用せず、新しいcanonical状態で有効性と台帳をundo/replayする。

## 10. 現在slotの生成と、遅延候補の検証

producerは現在slotに自分の有効permitがある場合だけ生成し、遅れたslotはskipする。起動時の未処理slotを走査してまとめて採掘しない。未来slotのpayloadを準備しても、未来slotの候補を今のslotとして公開・消費しない。

概念上の`exec_opportunity(...)`はcanonical state generationと対象ScheduleIdから、現在slotのpermit・expiry・包含境界、または機会なしの理由を返す。`build_exec(...)`はそのScheduleId・slot・permitを直接指定し、署名前に最新stateで再検査する。modelの推論や新しいBASE blockの到着を、毎回の生成準備に追加しない。API名は新しい実装契約の候補であり、現行APIの存在を宣言しない。

署名対象にはnetwork/profile、ScheduleId、global slot、permit、producer、attachmentと取引payload digestを束縛する。署名前にdurable journalへ`PermitId → 署名対象digest`を記録する。同一permitを別payloadで再署名してはならず、再起動・reorgでchain上の消費がundoされても、このローカル記録を削除しない。同一bytesの再送は包含条件内で許可する。

validatorは受信時の現在slotと候補slotが一致することだけを要求しない。候補の対象slotに正しい割当があり、署名・取引・canonical収録時の失効状態・包含期限・slot閉鎖条件を満たすかを判定する。履歴検証も同じ規則と保存証拠で完結する。

**署名と自己申告timestampだけでは、実際に旧slotで生成されたことを証明できない。** 正直なproducerの過去slot生成禁止と、validatorが客観的に検知できる境界を区別する。包含猶予内のbackdatingを完全検出できるとは主張せず、その可能範囲・遅延・二重署名の扱いをclock profileの残余として評価する。

## 11. 共通順序・競合・accepted prefix

同じScheduleId/slot/permitの異なるpayloadへの二重署名は、両署名と同じ固定subjectを認証した客観的equivocationの候補である。異なる枝の正規schedule、異なるpermit、競合UTXO/EVM取引があるだけで、同じ不正を証明したことにしない。slashの対象と条件は採択済みの責任規則に従う。

BASEは版付きの**共通順序関数**を用い、canonicalに収集したEXEC候補をglobal slot、permitと決定論的tie-breakにより処理する。ローカル受信順やprivate cacheを順序根拠にしない。同一slotの候補競合は一つのcanonical候補に解決し、取引の二重spend・nonce競合はその共通順序上のnative実行規則で解決する。候補の単体validityと、canonical受理・取引成功は別である。

native状態はaccepted EXEC prefixとslotのclose frontierを持つ。合意上の包含・閉鎖条件を満たしたslotを順番に閉じ、候補のないslotは有限期限後に空として閉じる。閉鎖前の収集、閉鎖時の候補確定、common order、permit消費・EVM適用の境界を同じpolicyで定義する。より後のslotを先に確定して未閉鎖の穴を残したり、閉鎖済みslotへの後着をprefix途中へ挿入したりしない。

同じcanonical枝の通常伸長では受理済みprefixはappend-onlyである。正規BASE受理、permitの`reserved → consumed`、関連するUTXO/EVM効果・feeは同じnative foldへ原子的に反映する。単なる生成・署名・伝送・provisional実行はcredit消費にしない。fork選択が変わる場合は別のcanonical reorgとして、同じ版のundo/replayで台帳とexecution prefixを一緒に復元する。

未使用slotの予約は、包含期限・必要な閉鎖処理を終えcanonicalに未消費と確定するまで解放しない。解放や後続利用は[07](0007-claims-exec-and-answer-accounting.md)の同じunit ID・元expiry・累積attempt上限を守る。消費undoを理由に、durable journalに反する別payloadを同じpermitで作らない。

## 12. 失効・取消と取引権利の境界

| 事象 | nativeの扱い |
| --- | --- |
| 使用前のcredit失効・取消 | 対応unitとそのpermitを使用不可にする。他unitのassignmentを再抽選・移動しない |
| provisional候補の原資・scheduleが失効 | canonical受理を保留または拒否し理由を記録する。既存accepted prefixへ勝手に効果を加えない |
| BASE accepted後に元workの反証・credit取消が成立 | 消費済みunitを再発行せず、provenance・責任・現在の認証状態を更新する。未使用permitには取消を反映する |
| 取消がPALW fork choiceに影響してcanonical枝が変化 | 正規reorgとしてEXEC消費・実行効果をundo/replayする。別の巻戻しauthorityを追加しない |
| settled表示の根拠が後日失われる | 既存settlement policyに従って現在の認証と過去の公表記録を区別し、withdrawal/conflictを記録する |

creditの単なる期限満了は、適法に受理・消費した取引や既得権の一括取消命令ではない。元workの不正でも、**そのcreditで運ばれた全取引の自動rollback、全受領者への返還義務、無条件のsettlement保証を本改定で追加しない**。保持する権利、未確定利益、責任帰属、native取消効果は既存の採択済み規則と[07](0007-claims-exec-and-answer-accounting.md)に従う。未確定の接続はactivationを止め、暗黙のrollback仕様で埋めない。

## 13. EXEC creditに循環依存しない進行

claim提出、認証資料、positive／negative court、資料要求・応答、schedule proposal/commit、Heartbeat／BASE anchorの搬送に、これから作るEXEC creditや当該scheduleの成功を必須条件としてはならない。既存の独立したBASE/object経路とその有限資源・料金規則を維持する。

creditがゼロでも次のworkを提出・認証でき、scheduleが欠けてもcourtを終結でき、EXECが止まってもBASEのlivenessを進められることを確認する。独立経路を無制限・無償・予約不要な迂回路にはしない。

1 BPSのEXEC負荷は共有の搬送・CPU・メモリ・state I/O・保存予算を消費する。公開negative、資料供給、局所courtの予約容量をEXECへ流用せず、原受理からのnotice・hard期限をslotやschedule更新でresetしない。モデル非保有Nodeの制限内で、混合負荷時にも公開裁定の予算が残る必要がある。

## 14. 1 BPSの測定項目とactivation gate

| KPI | 分けて報告する内容 |
| --- | --- |
| generated BPS | 生成したdistinct EXEC_TX候補数／観測秒。再送を除き、空payload・equivocation・過去slot生成を別計上 |
| valid BPS | 完全なschedule・署名・取引検査を通ったdistinct候補数／観測秒。競合候補をcanonical数に数えない |
| BASE accepted BPS | canonical native foldへ新たに受理したEXEC_TX数／観測秒。reorg前の総数、取消数、正味数を区別 |
| settled BPS | 版付きsettlement条件を新たに満たしたEXEC_TX数／観測秒。後日の認証撤回を別記し不可逆性を仮定しない |
| slot fill | 全global slotを分母にした割当率・生成率・受理率。credit不足・集中制約・停止slotも分母に残す |
| latency | work受付→認証→credit、freeze→公開→開始、生成→受理→settledの分布と待ち行列。観測終端の未完了件数・timeout・取消を分母とともに報告 |
| 持続供給 | 成熟eligible creditの純供給率、予約・消費・失効率、残高、BASE収録能力、増加backlog、schedule切替時の空白 |
| 混合負荷 | EXEC_TX・claim・positive・公開negative・資料応答の同時負荷、予約court容量、deadline違反、CPU/RAM/I/O/帯域と保存量 |

同じ観測区間・負荷条件を使い、完了例だけの平均やp95/p99で未解決件を隠さない。初期creditストックで短時間1 BPSを示す試験と、期限付きcreditの継続供給で1 BPSを維持する試験を分ける。stockが減り続ける、未受理backlogが増え続ける、courtが期限を逃す状態を持続達成と判定しない。settled BPSの立上り・遅延は生成BPSと別に記録する。

activation前に、clock/epoch mapping、timestampと未来時刻規則、公開先行期間、credit寿命、slot生成区間、包含猶予、閉鎖frontier、seed条件、schedule上限、再試行・返却、BASE受理容量、共通順序・取消・reorg・settlementの接続を版付きで固定する。具体的な期間・猶予・閾値は実測と明示承認を必要とし、本書で任意の数値を置かない。1秒slot以外の既存presetをこの文書だけで変更しない。

## 15. 実装接続・移行と受入ケース

添付source snapshotの`consensus/core/src/palw_execution_lane_v1.rs`には、秒round、DAA span、集合snapshot後のseed、permit、credit・forfeitの足場がある。この参照は最新mainの動作、有効化、1 BPSの実測、新しいScheduleId直接参照・clock/包含規則の完成を示さない。`exec_opportunity`と`build_exec`、schedule ID検索、native consumption、RPC四状態、durable journal、pruned IBDの接続を新profileとして確認する。

| ID | 受入ケース | 必須結果 |
| --- | --- | --- |
| S01 | seed判明後の集合・epoch・slot区間変更 | 拒否。同一freezeから引き直さない |
| S02 | 同unitの重複予約、異なるScheduleIdのslot重複 | canonical状態で一意性を拒否。global枠を増やさない |
| S03 | commitが開始slotに間に合わない／資料非公開 | 当該scheduleを開始せず、過去slotを埋めない |
| S04 | BASE tipが数slot更新されないが有効scheduleあり | 毎slotの新BASE待ちなしで現在slotの機会を導出。期限・資源条件は維持 |
| S05 | 普通のtip伸長／元credit一部取消 | assignment不変。取消unit由来permitのみ無効化し、使用時stateで検査 |
| S06 | authority anchorがreorgで枝から外れる | schedule・予約・消費・execution stateをcanonical枝でundo/replay |
| S07 | 再起動直後に多数の過去slotあり | catch-up生成なし。現在slotだけ生成 |
| S08 | 同permitで異なるpayload署名を要求 | durable journalが拒否。reorgによるconsume undo後も拒否 |
| S09 | 対象slot後に候補が届く／包含期限を過ぎる | 同一のcanonical境界から前者の適否を判定し、期限超過・閉鎖後の挿入は拒否 |
| S10 | 正しい署名で旧timestampを自己申告 | 署名だけで実生成時刻を証明したと扱わず、包含規則で制限 |
| S11 | 同slot競合・二重spend・受信順逆転 | 共通順序で同じ受理結果。equivocationと取引競合を区別 |
| S12 | 後着の古いslot、空slot、並行候補 | close frontierを守り、通常伸長のaccepted prefixを途中変更しない |
| S13 | BASE受理直後のcredit取消／settled根拠の撤回 | 自動全TX rollbackやcredit再発行をせず、規定責任・認証更新・正規reorgを区別 |
| S14 | creditゼロ、seed停止、EXEC飽和 | claim・court・schedule commit・Heartbeat／BASEの独立進行を維持 |
| S15 | 長時間1 BPSと公開negativeの混合負荷 | 4種BPS、未解決分母、stock減少、backlog、court予約・期限を報告 |
| S16 | cold Node・IBD・再起動・異なる受信順 | モデルなしでschedule・prefix・台帳・四状態が一致し、旧履歴を読み替えない |

旧版と新版の混在epoch、fence前後のundo、未消費予約、未決court・依存、過去のsettled公表履歴を含む移行表を完成させる。文書リンク検査や少数の局所試験を、これらの合意接続・持続性能・activationの完了証拠にしない。

---

## 原条文の統合収録

以下は内容を落とさないための原条文である。見出しの深さとリンク先だけを編集し、数式・コード・値・記録を再解釈していない。旧い方針・候補・試験記録も含むため、本文の責務・適用範囲と15の台帳を先に読む。原文内の相互参照は保存した旧ファイルへ戻る。新しい編集先は本書先頭の20本の索引・旧新対応表で確認する。

### 収録箇所

| 原文 | 行 | 入口 |
| --- | --- | --- |
| 旧0012 | 1–607 | [RFC-0012 — PALW-only consensus and native EVM settlement: retire DNS validators and their reorg](#source-v1-0012-L00001) |
| 旧0018 | 1–430 | [RFC-0018 — DAGを最大限に活用する設計変更: 多モデル並行生成、特権なきPanel anchor、SecurityCertified(Gate C)とDNS廃止後の収束](#source-v1-0018-L00001) |

---

<a id="source-v1-0012-L00001"></a>

### 旧RFC-0012 / 原文1–607行

出典: [0012-palw-only-consensus-and-native-evm-settlement.md](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md)。配置理由: 主題別統合。

### RFC-0012 — PALW-only consensus and native EVM settlement: retire DNS validators and their reorg veto

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


* Status: **Draft — dormant implementation; no activation or production settlement parameters assigned.**
* Spec closures, 2026-10-15: [R12-* requirement groups](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md#spec-closure-2026-10-15) — normative rules and named parameters; no PASS or activation implied.
* Revision, 2026-10-10後続改定: §2.1 withdraws model-availability consensus and adds model-bond coinbase allocation under unchanged total issuance and individual ceilings. Rule E and claim-specific adjudication remain separate gates; the dormant DNS-retirement work does not implement this amendment.
* Implementation record: [unactivated fence, native heads, monetary transition and remaining release gates](../design/palw/integration/rfc-0012-dormant-implementation.md).
* Source baseline: `3b09a814ed4b2a2f90009c05c1219efc9be46b5f` on `misakas/main`.
* Requested direction: remove DNS validators, DNS finality, DNS attestations, precommits,
  DNS-final and the DNS/stake reorg veto; let PALW block production and settlement carry the EVM bridge.
* Activation: a new coordinated, network-specific consensus fence; **no height assigned**.
  This is not a claim that DAA 5,300 already removed these dependencies.
* Would supersede, at activation only: the retention of DNS validators and their reward pool in
  [ADR-0126](../adr/0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md),
  the DNS BFT authority of
  [ADR-0128](../adr/0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md),
  and the DNS-based EVM `safe` tag / optional pause in
  [ADR-0109](../adr/0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md).
  Historical validation and pre-fence entitlements remain governed by their original rules.
* Builds on: [RFC-0008](../rfc/0008-palw-claim-backed-consensus-blocks.md),
  [RFC-0010](../rfc/0010-permissionless-palw-panel-and-claim-completion.md),
  [RFC-0007](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md),
  [RFC-0011](../rfc/0011-permissionless-model-and-long-context-onboarding.md),
  [ADR-0127](../adr/0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md),
  [ADR-0129](../adr/0129-a-double-spend-needs-the-anchors-not-the-blocks.md),
  [ADR-0020](../adr/0020-selected-parent-evm-lane.md), and
  [ADR-0089](../adr/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md).

#### 0. Decision

**PALW is the sole live consensus authority. No DNS validator set, bonded-stake vote,
attestation quorum, precommit lock, DNS-final anchor or stake reorg veto may approve, delay,
override or block post-fence PALW chain selection or native UTXO/EVM settlement.**

The normal path is:

`LLM execution → committed claim / uniquely accounted work slices → PALW blocks →`
`Panel verification + challenge/court lifecycle → PALW settlement → native UTXO/EVM state`.

Block inclusion and optimistic EVM execution can precede claim settlement; the arrows do not
require a producer to wait for `Final` before carrying an EVM payload. Safe settlement is the
later reader-visible condition. No separate validator certificate is required at either stage.

“Remove validators” means **remove the DNS finality-validator role**. Deterministic full-node validation, PALW producers, signed computation evidence, courts, public bonds and objective slashing remain. PALW Panel seats remain in the transitional mode; their honest majority is not the arithmetic trust root. A later Panel-free mode is separately deferred under RFC15 until RFC14 and that mode's own gates pass. DNS here is the finality overlay,
not domain-name resolution or DNS peer discovery. Those unrelated facilities remain.

The bridge in this RFC is the **native MISAKA UTXO ↔ MISAKA EVM accounting transition on the
same L1**, not an Ethereum/other-chain bridge. An external chain cannot inherit PALW settlement
without its own proof verification, destination rules and security analysis.

#### 1. What must actually change in today's code

| Existing dependency, verified at the source baseline | Required replacement |
| --- | --- |
| `processor.rs::dns_reorg_outcome` calls `dns_bft_gate_refusal` **before** PALW candidate comparison. `dns_bft.rs` rejects abandonment of the DNS-confirmed anchor until its staleness rule releases it. | No DNS veto on post-fence selection, including extension, sibling race, deep reorg and sink search. Keep the applicable PALW comparator and its independently reviewed safety checks. |
| `palw_fork_choice.rs::compare_palw_candidates_v1` orders frontier, safe weight, bounded live total and deterministic tie-break; pipeline fences add further PALW protections. | Reuse the full PALW selection path, not a replacement “highest DAA” or raw header-work selector. Audit every selection/IBD/pruning entry point. |
| `processor.rs::update_evm_canonical_heads` takes `safe` from `last_dns_confirmed_anchor`, otherwise falls back to `sink`; `finalized` uses a pruning result, with an initial sink fallback. | PALW-derived heads with explicit unavailable/bootstrap state; neither missing DNS nor missing history may promote an unconfirmed sink to `safe`/`finalized`. |
| `bridge_finality_is_fresh` reads DNS state. ADR-0109 defaults to `Label`, but retains the `Pause` policy. | No DNS freshness gate in template assembly, claim RPC, wallet or relay. Retire `Pause` as a DNS-dependent policy on the new network rules. |
| `validate_evm_deposit_claims` and `apply_evm_bridge_effects` already enforce lock consumption and withdrawal UTXO effects. | Preserve and test this same-chain accounting, executed against one canonical state generation; do not add a bridge signer committee. |
| ADR-0126's t12 split allocates 20% subsidy to DNS validators and 72% to PALW claim escrow, with 8% inclusion. | Retire future DNS allocations through one reconciled reward/fee schedule; clear historical liabilities without re-minting them (§6). |
| `dns_finality` also houses shared bond, payout, snapshot and historical validation types; PALW uses key-loading utilities under validator-named crates. | Extract shared functions/types before deleting services. A module name is not proof that everything in it is obsolete. |

Primary source paths:

* [Virtual processor](../../consensus/src/pipeline/virtual_processor/processor.rs), especially
  `dns_reorg_outcome`, `update_evm_canonical_heads`, `bridge_finality_is_fresh`, snapshot import
  and template construction.
* [DNS BFT gate](../../consensus/src/pipeline/virtual_processor/dns_bft.rs),
  [PALW comparator](../../consensus/core/src/palw_fork_choice.rs),
  [shared/historical overlay types](../../consensus/core/src/dns_finality.rs).
* [Native EVM accounting](../../consensus/src/processes/evm/mod.rs), especially
  `validate_one_deposit_claim`, `validate_evm_deposit_claims`, `apply_evm_bridge_effects`.

Stopping validator daemons, hiding their explorer tab or setting `Label` alone does **not** implement
this RFC. All live dependencies above must disappear together; historical readers may remain.

#### 2. One authority, without an unadvertised replacement committee

1. At the new fence, DNS bond weight, voting snapshots, inactivity leak, attestations, precommit
   locks and `last_dns_confirmed_anchor` contribute **zero authority** to live fork choice.
   Do not retain an emergency `StakeScore` fallback or reintroduce DNS through pruning/IBD.
2. Preserve PALW validity, accepted-transaction provenance, maturity, bounded immature weight,
   strict-win/tie rules and missing-data fail-closed behavior as resolved by the active schedule.
   A candidate lacking evidence is unresolved, not a reason to switch to stake or raw blue work.
3. Header-only work ordering remains a download hint, not a second canonical-chain authority.
   Virtual state, UTXO acceptance, EVM heads, sync and pruning must agree on one validated history.
4. PALW Panel receipts certify computation, not a BFT vote for a preferred chain. No new
   `PALW precommit`, committee checkpoint or “Panel-final reorg veto” may replace the deleted DNS
   gate under a new name. Existing PALW fork-choice protections are not abolished by this rule.
   [RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)'s
   proposed verification randomness is a fact derived from qualifying future canonical PALW work,
   not validator voting, DNS finality, committee/threshold signatures or a new settlement authority.
   Its branch-relative BeaconLocked predicate neither forbids a valid PALW reorg nor adds a finality overlay.
5. RFC08 multiple blocks per claim must bind `(claim, slice, work budget)` and prevent replay,
   duplicated weight or multiple rewards for the same work. More carrier blocks do not imply
   more independent verified work, independent challenges or settlement confidence.
   Under revised RFC08, both EXEC_TX and EXEC_SLICE have zero fork-choice/DAA contribution;
   slice work settles once at root Final and cannot count as extra chain confirmations.
6. Heartbeat/floor are bounded recovery facilities under their active validity rules. They may
   carry state transitions where permitted and advance timeout machinery, but are not substitutes
   for useful-work evidence in the bridge confirmation policy below. This RFC does not silently
   activate RFC08 or claim that today's floor-heavy production is already useful-work dominant.

##### 2.1 Model acquisition non-interference, allocation and Rule E — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。chainはmodel取得可否・Seeder登録/独立性・配布量/速度を管理せず、
旧必須Torrent、独立Bonded Seeder、PoR/Full Fetch/lease/TRDC/FPRと取得不履行に基づく資格/weight停止を撤回する。
model配布は任意のTorrent/mirror/共有契約で行い、取得失敗・非公開だけでSlash/Final延長を起動しない。

固定Model ID/root、weights/tokenizer/config/specは不変とし、変更は新Model IDとして登録する。
取得bytesは認証と全量復元rootで照合し、claimも同じroot/specをbindする。
任意のinfohash対応を登録する場合はimmutable bindingとし、必須取得経路にしない。
claim固有state/trace/output/witnessの有限な証拠責任は維持するが、modelの全量/反復range公開へ転用しない。
必要operandの認証と許可unit/累積scopeをADR177 D2で確定し、modelを得たfresh non-seat verifierの
計算反証/localization/exact courtを試験する。全model保有者拒否時の取得保証は撤回する。

重複しないminer拘束元本`S_m=sum_b C_{b,m}`からmodelのcoinbase予算を`f(S_m)`で配分する。
同じ元本をclaim数/rho/複数modelで増幅せず、総発行予算と個別bondのQ/B/R/F cap・共通DAA拘束を維持する。
資本預託だけでは支払わない。model別配分はblock頻度・DAA・fork choiceを自動変更しない。
公開参加資本が閉鎖自己資本より圧倒的に有利になることは倍率式・経済評価の未立証目標であり、
同額資本の所有者独立性をchainが識別できるとはしない。既存minerの実収入増も無条件保証しない。
non-interference、distinct-capital、三層予算、公開/閉鎖比較、回復/移行のgateはADR177 §3を用いる。
既存実装/測定・旧claim/activation記録は保持し、新配分・Panel=0は未実装/未有効化である。

EXEC_TX/EXEC_SLICEのweight/DAA=0、unique-work、root単位一回精算を維持する。
適用modeのclaim固有Final/court条件・暫定creditとsafe settlement・規則Eの全writer/reader/undoを検証する。
model配布不能だけの停止を起動せず、閉鎖時の未検出workを必ず排除できるとは主張しない。

#### 3. What “settled” means after DNS-final is gone

Three different facts MUST remain distinct:

* **Execution validity:** a claim passed the active verification and dispute rules and reached
  `Final`. Under RFC07/RFC11 this can have quantified probabilistic soundness, conditional on
  coverage, commitments, randomness, evidence availability and the stated adversary assumptions.
* **Canonical acceptance:** a block and its UTXO/EVM effects are accepted on the PALW-selected
  history currently verified by a node. A BLUE label or `Final` claim alone is not irreversibility.
* **Settlement confidence:** the relevant accepted effects have sufficient PALW-backed history
  under a published confirmation policy. There is no DNS/BFT certificate after retirement.

Define a versioned, deterministic `PalwSettlementPolicyV1` (proposed type, not an existing API)
and its derivation from a specified canonical state snapshot. Parameters include required
settled-anchor depth `D`, additional **unique eligible matured useful-work** `W`, work-concentration
limits and the applicable challenge/DA lifecycle. Values and their security justification are
activation blockers; this RFC assigns neither arbitrary confirmation counts nor a time promise.
*(2026-10-15: the parameters, their invariants, the 2026-10-08/10-11 drill-provisional rulings and the
open-court/EXEC-horizon/maturity rules are specified in [Spec closures — 2026-10-15](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md#spec-closure-2026-10-15), R12-SETTLE.
The drill values remain USER DECISION REQUIRED for production.)*

For an executed EVM result at carrier `B`, its safe eligibility requires:

1. `B` and its execution/input provenance belong to the validated selected history; the EVM result
   and corresponding UTXO effects are available and root-verified.
2. `B` is covered by the PALW settlement frontier, with the active provenance/claim-lifecycle rules
   satisfied. Missing data or a pending dispute that prevents settlement cannot be treated as success.
3. Qualifying settled anchors and unique matured work at/after the accepted effect meet `D` and
   `W`. Count shared-claim slices using their bounded work budget, not as independent security
   trials. DNS votes, heartbeat ticks and floor-only work add nothing to these useful-work metrics.
4. Derive a **prefix**, not disconnected safe blocks: if an earlier required dependency is
   unresolved, a later large claim cannot skip it. The mapping from the PALW frontier to eligible
   EVM results must be explicit and identical in live processing, restart and snapshot import.

This settlement policy labels risk; it MUST NOT introduce another tip-selection veto. Its depth
and weight bounds require analysis of the actual frontier-first comparator, not a copied PoW
confirmation formula. A private fork may have colluding Panel seats: “a private fork cannot get
receipts” is not a security proof. Test private maturation, concentrated stake/work, compromised
seats, withheld evidence, partitions and adaptive corruption without relying on DNS to stop them.

Execution-check error, Panel compromise, randomness grinding, DA failure and chain reorganization
are separate failure events. A direct randomized checker's conditional miss rate does not
bound total chain-finality risk. Consensus credit authenticates selected verifier approvals
under an explicit honest-fraction assumption (PRINCIPLES §8); cryptographic model-computation
proofs are prohibited. Publish checking scope, participation, availability, randomness and
reorganization assumptions before choosing the policy parameters.

##### 3.1 Publication history is separate from the current certificate

A recomputed `finalized` may retreat to `null` when evidence no longer certifies it. That MUST NOT erase
which head the node previously published as finalized. Retain a versioned local publication record,
including its ruleset, policy, retirement fence and generation, in the same durable batch as the native
snapshot and EVM heads. Advance its conflict anchor only to an equal or descendant finalized head.
This record is incident history: it cannot certify a head, supply useful work, or veto fork choice.

Every post-retirement evaluation MUST check the selected sink against the retained publication, including
after an ordinary withdrawal, restart, or rollback across the retirement fence. Abandoning that head
produces sticky `FinalizedConflict`, with safe/finalized absent. A canonical withdrawal is named by
`readiness.finalized.withdrawnFrom` after restart; a conflict has its own alarm. Current labels remain
subject to root, provenance, generation and ordered-prefix checks, independently of this history.

Only a validated pruning-point execution-state import/resync may clear both publication history and the
sticky snapshot, atomically with imported execution state and absent safe/finalized heads. Invalid imports
preserve them. The readiness memo MUST be invalidated even when the imported generation is unchanged.
Corrupt or incompatible history is preserved and makes readers fail closed. An older positive snapshot
can bootstrap a missing publication row; an older negative snapshot cannot distinguish no publication
from a forgotten withdrawal and requires validated recovery. An upgrade must not silently assume that
such a node never published a finalized head. The dormant public fence provides no deployed migration
evidence; local store tests and in-process fork tests do not establish crash/P2P/pruned-IBD guarantees.

#### 4. Native EVM bridge: one ledger transition, no validator approval

##### 4.1 Deposit / withdrawal invariants

Retain ADR-0020's selected-parent execution order. A carrier and the child that executes its payload
must not be conflated by the UI, receipts or confirmation calculation.

* Deposit: an accepted `EVM_DEPOSIT_LOCK` outpoint is found by the reconstructible lock index;
  producers prepare its system operation against the same state generation as their template.
  Validate destination, amount, tip, unspent status and timeout; consume the outpoint once and
  credit the EVM account once. No DNS attestation, second depositor signature or bridge signer is
  required for automatic claiming of an already valid lock.
* Preserve the existing claim/refund boundary: claim acceptance requires DAA **less than** timeout;
  at/after timeout the refund route owns the lock. Preserve the same-block visibility restriction.
  The two paths must be exclusive even across merges, delayed execution and reorgs.
* Withdrawal: deterministic EVM execution debits the account and materializes the protocol-defined
  UTXO output exactly once, with existing destination, scaling, precision and amount checks. It is
  not a request that a validator multisig must honor.
* Commit UTXO diffs, EVM state/results, consumed-lock and withdrawal identities, market settlement
  effects and canonical head/index changes atomically or through a crash-safe transaction protocol
  with equivalent recovery guarantees. Reorgs undo/reapply **both** accounting domains together.
* Preserve EVM nonce/gas/replay protection and ADR-0089 order → fold → child settlement ordering.
  Removing DNS is not permission to spend unsatisfied market orders or change membership pricing.

Conservation tests must cover native units and EVM scaling: deposits/withdrawals transfer backing,
not create subsidy. Include locked funds, credits, fees/tips, refunds, escrow, sinks and burned units
in the ledger reconciliation. A retry, restart, duplicate carrier or alternative branch cannot
increase the combined spendable supply.

##### 4.2 Progress and reader-visible heads

| Surface | New meaning |
| --- | --- |
| `latest` | Latest fully executed, canonical EVM result under PALW; optimistic/reorgable. Never just an unexecuted advertised tip. |
| `safe` | Deepest canonical EVM result in the prefix satisfying §3's published PALW policy. No DNS lookup; no fallback to an unsafe sink. |
| `finalized` | Conservative PALW-settled prefix also satisfying validated pruning/checkpoint safety. A pruning snapshot by itself is not evidence of settlement. This is protocol/economic finality under stated assumptions, not a validator certificate or unconditional mathematical finality. |
| Native bridge status | Lock seen → accepted → claim included → EVM executed / withdrawal materialized → PALW safe → finalized policy satisfied; also refund, reorged and failed states. Include carrier/result hashes and precise unmet conditions. |

Where a tag has no qualifying EVM result, return an explicitly documented unavailable result/error
for that RPC method and expose the reason; do not substitute `latest`. Extend internal head types
to represent absence. Whenever present, `finalized` is an ancestor of `safe`, which is an ancestor
of `latest`, in **executed EVM-result order**.

If useful work or verification stops, optimistic inclusion can continue under the permitted
recovery rules, but useful-work settlement waits and reports why. No DNS quorum can unpause it and
no number of heartbeats fabricates safe work. Timeouts may still advance for refunds/courts.
Wallets must distinguish an optimistic spendable balance from a safe balance, without promising
that low block latency equals external-payment finality.

Recompute `safe` on legal reorgs and emit changed/removed logs and transaction status. Do not pin
it by an RPC-local checkpoint. A conflict below `finalized`/pruned history requires the documented
resync/safety-failure procedure and an explicit alarm, never a silent relabel or DNS veto revival.

##### 4.3 Pruning import authority and repeatable recovery

An EVM header and account snapshot reproducing a block's commitments establish execution data for that
block; they do not authorize resetting settlement incidents at an arbitrary block. EVM pruning import
MUST target the node's established pruning point, installed by the existing validated headers-proof or
catch-up path. Check that target before expensive validation, then recheck and hold it through the import
batch. An unreadable target/header is an explicit error, not a panic or an assumed empty state. These
checks do not replace proof validation or change branch selection.

Retrying a committed import, including after restart before IBD's stable flag was set, MUST succeed for
identical canonical execution headers, snapshots and checkpoints. Complete missing rows independently;
do not treat an existing row as proof of success or overwrite contradictory/unreadable data. Verify all
existing rows before staging flat-state/head changes or clearing publication history. A checkpoint's
block, EVM number, root, payload and checksum must agree with the validated input. Current snapshot
canonicality and code/hash checks remain required. Storage corruption requires validated recovery;
ignoring a read error or deleting a conflicting row merely to make a retry succeed is forbidden.

The pruning-history context import must also accept identical existing samples. Validate nonempty history,
network genesis, uniqueness, monotonic blue score, immutable sample consistency and required header shapes
before changing history. Keep its target/retention update coherent under the context lock. The durable EVM
import batch and explicit IBD stable flag still form a recovery protocol; this subsection does not claim one
transaction spanning all UTXO/PALW/EVM sidecars. Real transport, interrupted imports, proof adoption and
cross-ledger recovery remain acceptance requirements under §§4.1 and 8.

A checkpoint-only chain row has no delta evidence and MUST NOT be cached as an ordinary row keyed only
by block hash. Evict previously ordinary evidence when that block becomes the current checkpoint;
read its root-verified execution separately. If context later moves below it, require its own delta
again and stop at missing history rather than inherit an empty contribution from the former boundary.
Readiness memo reuse requires both sink and pruning point to match. A successful import still resets
the memo after commit; context freshness is not permission to clear publication history or incidents.

The RPC response combines committed settlement and a current advisory evaluation. Equality of sink
alone does not establish agreement if pruning context moved between publications. Attach readiness only
when its version, policy and shared certificate statements (generation, safe, finalized and stop) agree
with the returned snapshot. Otherwise omit the explanation until a coherent publication exists; do not
substitute optimistic heads or invent a certificate. This coherence filter verifies agreement between
already validated local reads, not cryptographic soundness or a transaction across all sidecars.

#### 5. Retire the overlay without erasing history or locking users' bonds

Introduce a proposed `palw_dns_retirement_v1` fence, scheduled only for a compatible PALW network.
Its consensus parameters, reward schedule, transaction retirement matrix and snapshot version are
part of the fingerprint/schedule commitment. Changing a CLI flag is not activation.

Before scheduling, specify the fence predicate at **each** call site (2026-10-15: the resolved call sites are listed in
[Spec closures — 2026-10-15](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md#spec-closure-2026-10-15), R12-AUTHORITY item 2; EVM execution and snapshot boundary remain open): earning block, paying block,
accepting transaction view, incumbent chain, candidate chain, EVM execution and snapshot boundary.
No candidate may choose its own activation state to bypass a live rule. Crossing/forking around
the fence must converge independently of arrival order; the exact transition algorithm and tests
are release prerequisites, not left to a generic `if daa >= H` patch.

| Operation/state | At and after the fence |
| --- | --- |
| New DNS validator bonds, activations, attestations and precommits | Retired; no duties, admission to new live participation, vote weight or new reward accrual. Define explicit mempool rejection and consensus treatment for each legacy transaction kind, including merged transactions and crossing-boundary carriers. |
| Pre-fence DNS votes, anchors, snapshots and historical blocks | Retain sufficient versioned data/readers to replay and verify old history. They confer no post-fence veto or RPC safety label. |
| Legacy validator bond exits | Keep a bounded, deterministic exit/withdrawal path. Retirement never confiscates principal or demands a now-impossible fresh attestation/quorum to exit. Preserve existing liabilities and applicable cooldowns. |
| Pre-fence misconduct | Preserve a finite evidence/contest/settlement horizon with the original offense rules. Historical slashing cannot become a new live finality vote. State exactly when residual exposure ends. |
| Shared PALW bonds / keys / services | Separate by explicit role and ownership. Do not unlock a PALW producer/seat exposure merely because it shares a key, crate or record type with a DNS validator. |
| Remaining DNS reward/quality reserves | Snapshot and settle lawful pre-fence entitlements through deterministic accounting (§6), without requiring post-retirement participation. |

Stopping the DNS-only service is safe only after the fence and its duties are retired. Extract shared
ML-DSA key loaders and bond/payout helpers first. Keep historical codecs and replay as long as supported
networks need them; delete live voter/aggregator scheduling, not consensus history indiscriminately.

#### 6. Monetary policy: no DNS reward, no extra issuance

*(2026-10-15: this split is **not adopted**. The dormant code commits it only inside the unarmed fence's policy values;
production split and fee schedule are USER DECISION REQUIRED — see [Spec closures — 2026-10-15](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md#spec-closure-2026-10-15), R12-MONEY.)*

The proposed normal **eligible PALW block** split for the current 72/8/20 t12 baseline is:
**92% PALW claim escrow, 8% inclusion, 0% DNS validator subsidy**. The removed 20 percentage points
join the existing claim escrow; its producer/Panel allocation and work-price/market rules remain
those of the active PALW economy unless separately amended. This is an explicit proposal, not a
statement of the currently shipped split.

The released share MUST NOT become an immediately spendable unconditional producer reward.
It follows verified work, escrow release at the proper lifecycle stage, and void/burn rules.
Heartbeat/floor do not acquire a 92% useful-work escrow by default: preserve their separately bounded
recovery issuance and do not mint the redirected share where no eligible claim can back it. Publish
the complete block-kind matrix before activation, including treatment of any unallocated amount.

Resolve all subsidy, ordinary/finality-fee, inclusion bounty, deferred quality, reserve-drip and
slashing allocations together. No fresh fee stream may accrue to a retired validator pool. Redirect
new DNS fee shares into the versioned PALW/inclusion fee schedule, distinct from the subsidy split;
exact integer rounding and destinations must be frozen before release. Pre-fence balances are not
fresh fees: settle their recorded liabilities once, then handle residual reserves under an explicit
auditable rule. Do not silently sweep users' old claims into the new budget.

For cross-fence merged/attempt rewards, preserve the invariant `escrow <= actually withheld base`
at the payer. ADR-0126's paired earning/paying-score rule is a required regression case, not a license
to retrospectively raise an old claim's escrow from 72% to 92%. Test every score pairing, delayed
release, slash, void, replay and rounding remainder. The emission ceiling never increases.

#### 7. RPC, tooling and operator migration

* Add a versioned settlement-status response with ruleset/policy id, canonical generation, executed
  head, PALW frontier, safe/finalized results or absence, unique work/depth and unmet conditions.
  This is derived data, not an oracle. Cache and index state must be rebuildable from verified history.
* Retire `getPrecommitDuty` and DNS attestation duties explicitly (`retired`, fence and reason).
  Keep historical queries distinct from live capabilities. Old clients must not treat empty duties
  as node failure or interpret legacy DNS-final data as current safety.
* Remove DNS validator onboarding/current quorum panels from SDK, CLI, explorer and wallet normal
  flows. Keep historical bond exit/reward/evidence tools through their migration horizon. Use
  “PALW Panel” for compute verifiers and name the remaining trust assumptions plainly.
* After the fence, reject or clearly retire DNS-dependent `Pause` configuration rather than leaving
  an EVM lane stuck waiting for nonexistent votes. No operator override may change block validity.
* Snapshot import/restart must derive or verify heads, settlement summaries, legacy liabilities and
  EVM backing against committed history. Never initialize `safe` or `finalized` from the sink merely
  because a field is missing. Old snapshot/peer versions require explicit compatibility negotiation.

#### 8. Implementation sequence and release gates

1. **Dependency and security inventory.** Enumerate live DNS reads/writes, stake fallback paths,
   transaction kinds, payouts and snapshot formats. Analyze PALW without the veto: private maturation,
   adversarial Panel concentration, bootstrap randomness, DA/court censorship, long-range attacks and
   partitions. State the genesis/trusted-sync assumptions; no unproved “PALW is equally safe” claim.
2. **Separation with legacy equivalence.** Move shared helpers, freeze old-history golden vectors,
   add settlement policy types and reconstructible summaries. Below-fence roots, accepted sets,
   fees, payouts and EVM results must remain byte-identical.
3. **Dormant coordinated implementation.** Implement retirement, bond/reward wind-down, all fork-choice
   entry points, bridge heads and client capability handling behind the same reviewed schedule.
   Do not deploy a fork-choice-only patch and leave accounting or RPC semantics inconsistent.
4. **Adversarial multi-node drill.** Run full replay, pruning/IBD and crash recovery with zero DNS
   validators. Publish artifacts/results and settle policy parameters and resource bounds.
5. **Schedule only after the gates pass.** Publish fingerprint, height, compatibility matrix,
   bond exit procedure and release notes. Old nodes diverge after the fork and must not participate
   as compatible peers. After activation, reverting binaries is not a safe rollback; any corrective
   protocol transition needs a coordinated plan preserving all accepted financial effects.

Mandatory acceptance matrix:

| Test | Required result |
| --- | --- |
| Zero DNS validators, healthy PALW work/verification | Claims settle, canonical chain progresses, native deposit/withdrawal and market orders complete without DNS services or votes. |
| DNS-final anchor opposed to valid PALW candidate; absent/all-equivocating DNS votes | Post-fence PALW selection/heads are unchanged by DNS state; historical evidence can affect only legacy liabilities. |
| Sibling races, deep forks, strict-win/tie rules, private mature claims, partitions | Identical eligible histories converge under the specified PALW rules; security failures cannot be masked by reinstating stake veto. |
| One claim copied into many blocks; repeated work slices | No duplicate useful-work budget, reward or artificial independent settlement depth. |
| Heartbeat/floor only, withheld receipts, missing DA, open court | Permitted recovery proceeds; `safe` does not advance without its eligible PALW requirements. No fabricated latest-as-safe fallback. |
| Reorg before/after deposit claim, refund boundary, withdrawal and market settlement | Root/state equality after undo/replay; no double credit, double refund, missing debit, repeated withdrawal or supply inflation. |
| Child executes parent payload; pending/noncanonical payload | Correct execution provenance; unexecuted or detached effects never labeled safe. |
| Old/new earn-pay score pairs; unbond/slash/reward backlog | Coinbases agree with construction; no locked-forever principal, lost lawful entitlement or unbacked escrow. |
| Restart, crash between store writes, pruned sync and legacy snapshot migration | Same canonical state, backing and head tags as full replay, or explicit unavailable/resync status—not silent promotion. |
| Removal audit | No live DNS finality dependency in PALW selection, EVM bridge or wallet safety; allowlisted legacy readers and shared utilities are justified individually. |

#### 9. Completion and non-claims

RFC12 is implemented only when DNS validators are unnecessary **and incapable of exercising a live
veto**, pre-fence funds/history remain correct, and PALW alone supplies the documented native EVM
settlement path across replay, reorg and pruning. A renamed API or a stopped daemon is insufficient.

This document does not prove PALW common-prefix security, set confirmation parameters, remove the
need for independent computation verification, make arbitrary external bridges trustless, or promise
safe finality while useful work is absent. Those limits are not grounds to restore DNS secretly;
they are measurable security and liveness conditions that must be addressed before activation.

#### Mission alignment amendment — 2026-10-07

DNS authorityの撤廃とPALW computation accountabilityは維持する。PALW Panelを残す記述は移行中の通常処理を指し、正直な多数派を安全性前提にしたり永久の固定Panelを義務づけたりしない。計算の客観的裁定はDNS vote、Panel vote、EVM executionで代替しない。RFC14全gateとRFC15固有のsettlement/monitoring/migration条件が揃うまではPanel=0を有効化しない。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)、[RFC15](../rfc/0015-panel-free-permissionless-verification.md)を参照する。

#### Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

PALW-only/native EVMの将来経路にも同じbond/DAAのblock/reward/Final weight予算を適用する。Finalというphase名だけを未予約full creditやbridge safeの根拠にしない。
会計上の配分、保留dispute、残存責任、経済Finalを全head/writer/readerで一致させる。旧DNS/clockの撤去は別upgradeのままで、本原則から新weight/DAAを暗黙に有効化しない。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。

#### MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。

#### 改定 — 2026-10-12（モデル配分とブロック生成機会の接続：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §3A.5。

ADR-0177 2026-10-12改定のモデル配分（Miner＋Panelの適格claim bondと1.3〜2倍の係数、提案）をブロック生成機会へ接続する場合、coinbase配分だけの変更として済ませず、ブロック機会の帰属・上限・移行を本RFCで明示する。物理blockの総頻度、DAA/難易度、fork choice、Final weightを同時に変更するとは決定しておらず、特にFinal weightへ係数を自動乗算しない。必要な変更は別のversioned review・試験・activationを要する。

**X5:** `palw_dns_retirement_v1` は設計上のfence識別子、`Params::palw_dns_retirement` はRustの内部フィールド名であり、異なる階層の名前である。

<a id="spec-closure-2026-10-15"></a>

#### Spec closures — 2026-10-15

**Scope and reading rule.** This section closes specification gaps that the
[design audit's requirement index](../audit/palw/design-reviews/rfc-0006-0007-0008-0012-design-audit-2026-10-10.md#完全解消を判定する要件の索引)
lists for R12-AUTHORITY, R12-SETTLE, R12-BRIDGE, R12-HEADS, R12-MONEY and R12-OPS. Where this RFC's body is silent
or contradicts a later design record, ADR or user ruling, this section governs; superseded body text is kept
and annotated, not deleted. [PRINCIPLES](../PRINCIPLES.md) and ADR-0173/0176/0177/0178 rank above it. Nothing
here is an implementation PASS, an activation, a fence height or a production parameter assignment; every
public preset keeps `palw_dns_retirement_v1` absent. "USER DECISION REQUIRED" marks a named parameter whose
value needs the user's decision; a recommendation appears only where an existing record justifies one.
Mechanics results and private-process acceptance are not independent review or economic evidence.

##### R12-AUTHORITY — every selection entry point, ordering and assumptions

1. **Rule E ordering (ADR-0178 §4.6).** A ruleset MUST NOT arm `palw_dns_retirement_v1` unless
   `palw_fork_choice_rule_e_v1` is armed at or below the same height. The public validator continues to refuse both
   until their gates close.
2. **Fence predicate per call site (closes §5's "specify at each call site").** Fork choice resolves retirement at
   the **incumbent** sink (the candidate cannot select its own rule). Transaction admission and template selection
   resolve it at the **accepting** DAA. Claim escrow uses the **lower** of the earning and paying DAA. Historical
   equivocation evidence is admissible only within the accepting-DAA horizon and only when both signed targets lie
   strictly before retirement. The EVM-execution and snapshot-boundary predicates MUST be specified in the same
   form before scheduling; until then this item remains open (it is not left to a generic `daa >= H` test).
3. **No replacement authority.** After retirement no DNS state, Panel receipt, committee/threshold signature,
   RFC07 `BeaconLocked` predicate, settlement label, publication-history row or operator override may veto or
   reorder PALW selection. `safe`/`finalized` labels are outputs of selection, never inputs to it.
4. **Removal inventory as a release artifact.** The release MUST carry an inventory of every live DNS read/write,
   stake fallback, transaction kind, payout and snapshot path, each marked removed or allow-listed with a reason
   (§8 removal audit). A renamed module is not evidence of removal.
5. **Trusted-sync and genesis assumptions.** The release MUST publish a versioned statement of what a new node
   trusts (genesis identity, headers-proof/pruning-point import, any checkpoint) and of the adversary assumptions
   under which PALW-only selection is analysed (private maturation, Panel concentration, bootstrap randomness,
   DA/court censorship, long-range and partition attacks). This RFC does not supply that analysis; absence of the
   statement blocks scheduling.
6. **Model availability is not an input** (ADR-0177, ADR-0178 §10): selection, settlement and readiness read no
   model distribution, peer reachability, acquisition result or lease.
7. Remaining (observations, not verified): multi-process zero-DNS fork/partition/long-range drill, attack tests of
   the certified effect, the EVM/snapshot predicates in item 2.

##### R12-SETTLE — `PalwSettlementPolicyV1` parameters and invariants

1. **Open dispute inventory.** Evidence filtering, lifecycle closure and readiness MUST read one shared inventory of
   open DA sessions **and** open computation courts (`native_open_sessions_v1`). An expired retention clock never
   bypasses an open court. A claim with an open dispute contributes no D or W.
2. **Unique work.** W counts each `(canonical work id, quantum index)` once, read from the PALW delta of the block
   that carried the transition (not from rows the sink still holds). EXEC slice work counts once, under the root's
   identity, at root native Final. Carrier copies, heartbeat, floor, execution Round and unspent FP grants add zero.
   A later `Final → Voided` conviction retracts what the `Final` counted.
3. **EXEC consumer horizon.** The Final journal, the native facts and the node lifecycle/readiness MUST carry the
   EXEC consumer horizon of every counted root, so a dependency still inside its liability window is not mature.
4. **Maturity invariant.** A counted fact is mature no earlier than
   `max(accepted + M_maturity, Final + claim_retirement, trace_retention)` and, additionally, no earlier than the
   last applicable court/DA/liability endpoint of that claim (including any G14/OPV/EXEC consumer path). A longer
   new G14 path therefore lengthens maturity automatically (2026-10-11 ruling).
5. **Prefix, not islands.** §3 item 4 stands; an unresolved earlier dependency stops the safe prefix.
6. **Named parameters** (all committed in the fence's policy and fingerprint; all **value: USER DECISION REQUIRED**):

   | Parameter | Invariant | Recommendation and source |
   | --- | --- | --- |
   | `settled_anchor_depth` D | integer ≥ 1; counts distinct qualifying grant/attempt anchors, never slices | recommended **3** — 2026-10-11 drill-provisional ruling; with a 400 ‰ operator cap three operators is the minimum that can satisfy it ([policy proposal](../design/palw/execution/rfc-0012-policy-proposal.md)) |
   | `unique_mature_work` W (pwu) | > 0; distinct from ADR-0176's budget window | recommended **6,331,093,440** as a drill floor (= 3 × smallest REAL claim); production value from a census `⌈k × matured REAL work per window⌉`, which does not exist yet |
   | `operator_concentration_cap_permille` | 1..1000 | recommended **400** (2026-10-11 drill ruling) |
   | `class_concentration_cap_permille` | 1..1000 | recommended **1000** for now; re-evaluate **600** once ≥ 3 REAL classes have mature work (2026-10-11 ruling) |
   | `M_maturity` (DAA) | ≥ accepted+bind+receipt+challenge+court of the counted rules | recommended **5,400** (DERIVED in the policy proposal); per the 2026-10-08 ruling it is neither shortened nor treated as final without the formula derivation, the 5,400/3,000/1,000/600 comparison and the attack tests |
   | `legacy_evidence_horizon_daa` | ≥ the DNS `evidence_window_blocks` it preserves | recommended **10,080** (DERIVED from `evidence_window_blocks`) |

   The 2026-10-11 values were adopted for the **drill only**; they are not production values and do not arm anything.

   **Status (2026-10-15, lead record): closed-as-provisional.** The specification of R12-SETTLE is closed with these
   values marked **drill-provisional (user ruling 10-11), production values pending census/measurement**:
   D = 3; W = 6,331,093,440 pwu (revisit with the census); operator concentration 400 ‰; class concentration
   1000 ‰ for now, re-evaluated at 600 ‰ once at least three REAL classes have mature work;
   maturity = `max(accepted + 5,400 DAA, the later retirement/liability deadline)`, so any longer G14 deadline
   lengthens maturity; legacy DNS evidence horizon 10,080 DAA. No preset carries them and no fence is armed.
7. Remaining: census of operators/classes/pwu per window, native court-extension and pruned-provenance paths,
   private-fork attack evidence (validation-only).

##### R12-BRIDGE — native accounting

The §4.1 invariants are complete as specification. Added rule: claim/refund exclusivity is evaluated at the
accepting DAA of the executing block, so a merge, delayed execution or reorg cannot make both paths succeed; market
settlement effects belong to the same state generation as the UTXO/EVM effects and are undone with them. Remaining
items are validation-only: reorg/refund boundary rollback, actual market fills, and equality of both ledger roots
after undo/replay.

##### R12-HEADS — durability and reader coherence

1. Every import session/recovery marker, each verified sidecar install, the marker preceding virtual UTXO
   replacement and the final stable publication MUST pass a synchronous WAL barrier before the next step. Intrusive
   target resets use the same barrier.
2. Native-retirement virtual commits MUST persist publication history synchronously with their UTXO/selected-chain
   generation before releasing reader guards (extends §3.1).
3. Validated snapshot declarations MUST be installed with the PALW tip, the servable pruning snapshot and the
   incomplete marker in one batch; whole-set validation precedes staging. Genesis PALW delta, initial tip, UTXO-valid
   rows and accepted class-carriage index rows are staged in one batch after the transition is fully derived.
4. An incomplete intermediate virtual copy MUST NOT publish canonical ETH or native tags.
5. Remaining (observations): power-loss durability, atomic initialization of every header/body/virtual store, crash
   between generations under real pruning, finalized-conflict external evidence.

##### R12-MONEY — split, fees and retained liabilities

1. **The §6 split is not adopted.** `92 / 8 / 0` is a proposal. The dormant implementation commits it inside the
   unarmed fence's policy values only so the mechanics can be tested; that is not adoption. The same holds for the
   dormant rule that normal and native-settlement transaction fees go 100% to the carrier.
   Named parameters `retirement_subsidy_split` (claim-escrow ‰, inclusion ‰, DNS ‰ = 0; sum = 1000) and
   `retirement_fee_schedule` (per-recipient fee shares; sum = 1000; no DNS/service share): **value: USER DECISION REQUIRED**. No recommendation is made here, because the
   model-bond allocation of the claim escrow under ADR-0177 D4 (and its 2026-10-12 proposal) is itself undecided.
2. Invariants that hold for any value: the emission ceiling never increases; escrow ≤ actually withheld base at the
   payer; the retired DNS share is not minted where no eligible claim backs it; old claims keep their recorded
   entitlement (no retroactive 72%→92%); pre-fence liabilities are settled once and residual reserves follow an
   explicit auditable rule.
3. **2026-10-11 rulings applied.** A fee-only Round is not counted in `B_max`; it is limited by a separate,
   capital-proportional `ExecutionCap` charged at allocation. In v1 the market fee is not in `R_max`; expected fee
   income enters the economic-safety computation.
4. Remaining: reconciliation of snapshot generations, old/new earn-pay and legacy horizons on every path
   (validation-only).

##### R12-OPS — capabilities, retired kinds, release procedure

1. New DNS bonds (`0x10`), attestation shards (`0x11`) and precommits (`0x19`) are rejected after retirement in
   header context, populated UTXO context and template selection. Bond unbond/exit stays available.
2. Clients MUST check reported durable import status before selecting funds or signing. Under native retirement a
   client requires the import-status capability; a missing capability is accepted only for dormant legacy rules.
   Validator status reports an unknown finality authority when both the settlement read and DNS authority fail,
   never a fallback "active DNS validator" headline.
3. wRPC keeps the exact v1 encoding when the optional native fields are absent and uses response v2 when present.
4. **Release observation bounds.** Real-process drills require a separately supplied
   `rfc12-real-process-observation-bounds` v1 document (`maximumOwnedProcessRssKiB`,
   `maximumOwnedDatabaseLogicalBytes`, `maximumProbeMs`, `samplePeriodMs` ∈ [100, 5000],
   `maximumFixtureOutputBytes`) and a fixed `workflow_timeout_s`. Values: **USER DECISION REQUIRED**; none is
   invented here. A >100,000-block processor latency bound for the release: **USER DECISION REQUIRED**.
5. Before scheduling, the bond exit procedure, compatibility matrix and fingerprint MUST be published (§8 step 5).
6. Remaining: third-party wallets/SDKs/EVM tooling migration, pruned P2P, 100k release latency (validation-only).
7. **External wallet path (2026-10-15, round 3; source only, no PASS).** Items 1–3 are now available to third-party
   code as one versioned surface: `kaspa-wallet-core::misaka_preflight` (capability schema `misaka.wallet.capability.v1`,
   a single `preflight` call before signing, WASM `misakaPreflightV1`). It refuses on genesis mismatch, a missing or
   incomplete import status, an unknown finality authority, and retired kinds once retirement is active or scheduled
   within 1,000 DAA of the sink. The 1,000 DAA margin is a proposal: **USER REVIEW**. Integrator note:
   [rfc12-external-wallet-integration-2026-10-15](../design/palw/execution/rfc12-external-wallet-integration-2026-10-15.md).
   Third-party migration itself remains EXTERNAL.

##### ALL-MODEL

RFC6's 2026-10-15 section holds the shared ALL-MODEL rules. For this RFC: no settlement, head or fork-choice rule
reads model availability; immutable model identity binds claims; the 2026-10-12 proposal to connect model-bond
allocation to block opportunities does not change D, W, fork choice or Final weight.

#### ユーザー方針 2026-10-15 による改定

* DNS退役後のsettlementの`safe`/`settled`は、**認証済み(`SecurityCertified`)のPALW creditだけ**を意味する。native settlementはfence以後、`Final`遷移ではなく認証遷移を成熟workの証拠とする。未認証claimだけのchainはPALW settlement finalityなしと報告する(RPCは`uncertifiedFinalAnchors`と前提の文言を返す)。
* DNS-BFTの物理除去は別作業(`integ/rfc12-dns-removal`)であり、本改定はそれに依存しない。

**ユーザー方針(原文):**「この攻撃を塞ぐ方針で進める。DNS-BFTはRFCにある通り廃止したので使用できない。」— ここでの攻撃は、閉鎖モデルまたは自前Panelのモデルの未認証claimが`Final`に達し、safe frontier・safe weight・live total・settlement depth・rule E participation・settled anchorを進めて、不正な合意またはsettlement finalityを得ること。補足判断(同日):「抽選した検証者の承認を認証の柱にしている。これは承認にするということは前提にするのでそのままでいい。」

* **`Final`と`SecurityCertified`は別状態である。** `Final`は手続きの完了(利用者払いescrowの経済的settlementは既存規則どおり)。合意credit(safe frontier、safe weight、bounded immature/live total、settlement depth、rule E participation、settled anchor、発行由来の報酬leg)は`SecurityCertified`のclaimだけが生む([RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md) §7、Gate C)。
* **認証は前提に基づく。** claim固定後の乱数でglobal poolから抽選した検証者のm-of-k肯定承認と、G14公開告発windowの異議なし経過で成立する。「抽選されたverifier stakeのうち正直な割合が閾値以上」を明示的な前提とし、無条件の数学的証明とは扱わない。暗号学的な計算証明は使わない(PRINCIPLES §8)。認証済みでもG14で反証されたclaimはvoidされ、合意creditは取り消される。
* **DNS-BFTは廃止済みであり、安全性にも収束にも使わない。**
* **genesisモデルにも認証の免除はない。70Bモデルは、独立した検証(抽選検証者による再実行)が成立するまで合意creditを持たない。**
* 本改定は追記であり、上の本文は履歴として残す。実装は休眠(fence `palw_independently_work_certified_v1`、全presetで`None`、有効化は別途のユーザー判断)。

---

<a id="source-v1-0018-L00001"></a>

### 旧RFC-0018 / 原文1–429行

出典: [0018-dag-parallel-multi-model-consensus.md](../rfc/0018-dag-parallel-multi-model-consensus.md)。配置理由: 主題別統合。

### RFC-0018 — DAGを最大限に活用する設計変更: 多モデル並行生成、特権なきPanel anchor、SecurityCertified(Gate C)とDNS廃止後の収束

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **設計前提(ADR・RFC より上位)— [MISAKAの不可侵原則](../PRINCIPLES.md):** 確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。本RFCはこの前提の下にあり、衝突する場合は前提が優先する。
> **暗号学的な計算証明を使わない — [PRINCIPLES §8](../PRINCIPLES.md)(2026-10-15 ユーザー決定、恒久):** 本RFCはZK・succinct argument・validity proof 等を、必須条件・代替経路・将来・研究のいずれとしても挙げない。使う暗号はハッシュ、署名、Merkle commitment/opening、乱数(beacon)だけであり、計算の正しさは決定論的再計算・サンプリング再実行・代数的スポットチェック・抽選検証者の承認(明示的前提の下)で扱う。
> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)のbond当たりQ/B/R/F上限と共通DAA拘束を適用する。[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)のモデル入手不介入は維持するが、§7.9のとおり **合意creditについては改定** する(ユーザー方針 2026-10-15)。

**トークンの表示名は Misaka、ticker は BILI。** `misaka` 系アドレスプレフィックスと既存のprotocol/CLI/API識別子は維持する（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

* **Status:** Draft — 2026-10-15。レビュー検証(§1)、設計(§4–§9)、休眠fenceの下の実装(**implemented, dormant, unverified**、§13)。build/test/実node試験は未実施で、どの項目もPASSではない。
  * **実装・検証の後続記録 (2026-10-31):** 上記は初版時点の状態。現在は DAG 幅 reader も接続されている。モデル別並行生成、遅延到着、再起動・replay と driver の現行 RPC 対応は [検証記録](../evidence/rfc/0018-parallel-dag-2026-10-31.md) を参照する。設計採用、有効化、実プロセス受入の完了を意味しない。
* **Decision:**
  1. 同じ親から同時に生成された異なるモデルのsibling blockは全て保持・mergeされ、各claimはBLUE/REDに関わらず独立に記録・検証・帰属される。共有上限で競合するときの採否は、mergeset集合の関数(色・merge順に依存しない)とする(§4)。
  2. Panel anchorのgenesis operator特権を除去し、bond済み・成熟済みの任意の採用attemptの集約seedで置き換える(§5)。
  3. **`Final`と`SecurityCertified`を別状態にする(Gate C、ユーザー方針で採用)。** 合意creditは`SecurityCertified`のclaimだけが生む。認証は「claim固定後の乱数でglobal poolから抽選した検証者のm-of-k肯定承認」+「G14公開告発windowの異議なし経過」であり、**前提に基づく認証** である(§7)。
  4. DNS-BFTは廃止済み(RFC-0012)であり、本RFCは安全性にも収束にも使わない。DNS廃止後の収束はGate Cとrule E v2(資本加重・分割耐性・認証済みのみ)で行う(§6)。
  5. DAG幅(k/BPS/parents/mergeset)は休眠scheduleの仕様とstubだけを置き、値はユーザー判断とする(§8)。
* **Activation:** 5つの新fence(§13.1)。全presetで`None`、flag-dayリストに含めない、fingerprintはSome-only、`Some(never())`は`None`へ畳む、armすると`validate_palw_rfc0018_v1`が拒否する。A-2一様: fence未満では全readerが従来どおりに答え、Gate Cのfence未満で受理されたclaimは旧規則のまま完結する。**有効化は別途のユーザー判断。**
* **Baseline:** 検証の基準は`pre` = `5aa52b886`、live testnet-12 = `rcore/int-12` `0b1c11b87`、実装は`integ/rfc02-rfc06-12-g14` `6b1a01acd`から分岐した`integ/rfc0018-dag`。本文のcode行番号は`integ/rfc0018-dag`のもの(§1の検証表はHEAD `6b1a01acd`時点の行)。
* **Not activated by:** 本RFCのmerge、設計値表(§11)、単体試験sourceの存在、driverの出力。
* **Normative language:** MUST / MUST NOTは各fence有効化後の規則。設計値は名前付きの未承認値であり本番承認ではない。

#### ユーザー方針 2026-10-15(本RFCを拘束する)

1. 「この攻撃を塞ぐ方針で進める。DNS-BFTはRFCにある通り廃止したので使用できない。」— ここでの攻撃は、閉鎖モデルまたは自前Panelのモデルの未認証claimが`Final`に達し、safe frontier・safe weight・live total・settlement depth・rule E participation・settled anchorを進めて、不正な合意またはsettlement finalityを得ること。
2. 認証の柱は抽選検証者の承認とする。これは前提として置く(「正直な抽選verifier stakeの割合が閾値以上」)。暗号学的な計算証明を必須条件に加えない。f = 1/3、f = 0.6、最悪ケースの評価は前提が崩れた場合の影響の報告であり、設計を変える条件ではない。G14の公開告発で反証されたclaimは認証済みでもvoidされ、合意creditは取り消される。
3. MISAKAは暗号学的な計算証明を一切使用しない(PRINCIPLES §8、恒久)。

#### 0. 要約と目標状態

**目標状態:** 「多数のモデルごとにminerとPanelが独立して同時稼働し、それぞれの計算がDAGに取り込まれ、特定のPoS側の生成者選出や確定制御に妨げられず、全体として一つの合意に収束する」。

DAG(GHOSTDAG、複数親、REDの取引受理)は作り直さない。現状は (a) 複数親・複数tip・RED受理がすでに機能し、(b) PALWの採否は単一proposer選出ではなく計算由来のticketとbond予算で決まり、(c) RED merged workもPALWでは数える。残る障害は四つである。

| 障害 | 現状 | 本RFC |
| --- | --- | --- |
| sibling間の順序依存 | 共有上限(DAA当たり発行予算、network room、per-bond)で競合したとき、consensus order(blue workとhash)の先頭が勝つ。ADR-0170のseed anchorは「最後に採用されたattempt」で決まる | canonical sibling order(モデル間round robin、§4) |
| Panel anchorの特権 | testnet-12はDAA 750からgenesisの8 operator bondのattemptだけがanchor(lane A)。headerだけで判定し、merged workで拒否されたattemptもanchorになり得る | 許可不要の集約anchor(§5) |
| 未認証claimの合意credit | `Final`のclaimはsourceを問わずfrontierを進め、settlementはattemptの`Final`を数え、rule Eはvoidされたattemptも数える。閉鎖モデルの自作自演でもfinalityが進む。testnet-12の現在の保護はDNS-BFT gateに依存しており、DNS-BFTは廃止済み | Gate C: `SecurityCertified`だけがcreditを生む(§7)、rule E v2(§6) |
| DAG幅 | 120 s / k = 1 / parents 10 / mergeset 180。長時間推論(p50 340 s)はk = 1でほぼ全てRED | 休眠schedule仕様とstub(§8) |

#### 1. レビュー主張の検証(STEP 1)

判定: **CONFIRMED** / **PARTLY** / **WRONG**。行は`6b1a01acd`。liveは`0b1c11b87`、preは`5aa52b886`。

| # | 主張 | 判定 | 根拠(file:line) | live t12 |
| --- | --- | --- | --- | --- |
| a | `ghostdag(parents)`は複数親を扱う | CONFIRMED | `consensus/src/processes/ghostdag/protocol.rs:303`(selected parent `:244-271`、mergeset順`:317`、k-cluster着色`:334-364`、blue score/work `:366-386`)。round blockは常にRED(`:335`)、ADR-0105のheartbeat透過(`:112-147`) | live(同一。差分はtest整形のみ) |
| b | `pick_virtual_parents`は複数tipを選びtemplateへ渡す | CONFIRMED | `processor.rs:5033-5139`(候補`max_block_parents×3 = 30`、`:4794-4798`、重い半分5+無作為、`:5055-5073`、parents ≤ 10かつmergeset ≤ 180、`:5105`)。template `processor/template.rs:853`。PALW固有の除外: heartbeat幅 ≤ 4(`:5085-5120`)、round lane枠(`:5102`)、merge depth(`:5222-5240`)、lead cap(`processor/palw.rs:10212`)。bond・attemptの性質による除外はない | live(`processor.rs`内の同等code) |
| c | `palw_v2_merged_works`はBLUE/REDのPALW workを集めて検証する。REDはPALWで数えGHOSTDAG blue workには足さない。重複・予算超過は拒否 | PARTLY | 収集は`processor/palw.rs:9197-9251`、BLUE/REDを分けず`acceptance_ordered_mergeset_without_selected_parent`の一列(`:9215`、blue work昇順、DAA 750以降はparents-first)。blue workは`mergeset_blues`だけ(`protocol.rs:368-384`)— CONFIRMED。重複はfoldで拒否(`palw_state_v2.rs` `DuplicateClaim`/`DuplicateExecution`/`DuplicateWork`)、予算超過は**skip**(block不適格ではない、step 4b)。**PALWにRED専用の期限はない**(GHOSTDAGのmerge depthとmergeset上限だけ)。REDの報酬は自分のminerへ(`coinbase.rs:313-351`) | live |
| d | miner資格は計算由来ticket(`class_ticket_v3`)+bond/予算で、単一PoS proposer選出ではない。Panel選出`select_job_panel_v3`は別で、classを検証できるbondから抽選 | PARTLY | ticket: `palw_attempt_v2.rs:581-588`(execution commitmentのhash、`ticket ≤ target`、`palw_admission_v2.rs:925-934`)— CONFIRMED。**`select_job_panel_v3`(`palw_job_panel.rs:137`)にはlive呼出元がない**。liveのPanelは`derive_panel_v2_with_policy_judged_v1`(`palw_panel_v2.rs:1415`)、binder `processor/palw.rs:9472-9517`/`9574-9707` | ticket: live。`select_job_panel_v3`: 未使用 |
| e | DNS stake reorg gateが存在し最初に問われる(`dns_reorg_outcome`) | CONFIRMED(補足付き) | `processor/dns.rs:132`。呼出: `processor.rs:4833`(dead-branch escape)、`:4931`(sink search)、`palw_rule_e.rs:323`。順序: heapはblue work順(`:4848-4851`)→finality→UTXO検証→`dns_reorg_outcome`。内部ではDNS未廃止なら**BFT gate `dns_bft_gate_refusal`が最初**(`dns.rs:150`)、V2の非延長はstrict economic win(`:229-237`)。旧work×stake規則はV2では延長にしか届かない | live(`dns_bft_gate`はDAA 0、strict-winはDAA 750) |
| f | testnet-12ではPanel bindにgenesis 8 operator bondの特権anchorが関与し、RED merged blockで届いても認識される | CONFIRMED | `palw_operator_anchor_v1.rs:102-104`(全genesis bond)、`processor/palw.rs:6543-6562`(`blues ∪ genuine_reds`、selected parent除く)、seedはDAGで最古のoperator attempt(`:6576-6595`)。**追加所見F-6:** `operator_of_v1`(`:169-176`)はheaderだけを見るので、merged workでskipされたoperator attemptもanchorになり得る | live(DAA 750〜、live版は`mergeset_reds`にround blockを含む) |
| g | partition再合流が収束しないことがある。rule E(ADR-0178)とDNS退役(RFC-0012)は実装済み・休眠 | CONFIRMED | rule E `palw_fork_choice_rule_e_v1`は全preset `None`、`PALW_FORK_CHOICE_RULE_E_ARMABLE_V1 = false`(`:99`)。DNS退役`palw_dns_retirement`は全preset `None`かつrule Eなしでは拒否(`:121-129`)。live `0b1c11b87`には両moduleとも存在しない | live: 未実装。HEAD: 休眠 |
| h | 120 s、k = 1、parents 10、mergeset 180 | PARTLY | `params.rs:228-229`(`new_two_minute_bps` = `new_seconds_per_block(120, 1)`)、`:243`(180)、`:254`(120,000 ms)、`:258`(10)。**finality depthは式の360ではなく`window_challenge/2` = 600、pruning depthは74,920**(`with_palw_v2_depths`、`params.rs:4197-4201`)。merge depth 30 block、coinbase maturity 1、DAA窓264。k/BPSを高さで変える仕組みはない(`ForkedParam`はcoinbaseのbps履歴だけ、`:5675-5689`) | live |
| i | `palw_mergeset_view.rs`に同一親sibling BLUE/REDの試験がある | PARTLY | `consensus/src/pipeline/virtual_processor/tests/palw_mergeset_view.rs:9`の**1件だけ**(3 siblingで真のREDとmerged BLUEを作り、round blockが会計を変えないことを確認)。複数モデル・claim帰属は試験していない。**live `0b1c11b87`には存在しない**(`acb7dd644`で追加) | pre/HEAD |

##### 1.1 レビューが扱っていなかった点

| 項目 | 所見 |
| --- | --- |
| 多モデルのticketと難易度(共通W) | `palw_work_target`以降、class targetは`MAX·min(1, CCU_m/W)`(`palw_work_target_v1.rs:116-125`)。Wはnetwork共通でepochごとに`model_blocks/expected_blocks`で再調整(`:98-111`)。single lotteryではclass ticketがlottery全体で、attempt blockのblue workはモデルに依らず一定(`protocol.rs:374-382`)。**モデルAのminerはモデルBとticketを競わない**。相互作用は共通Wの再調整、k = 1の着色、共有上限、ADR-0165の`FloorNotIdle`だけ |
| 異なるモデルの同時siblingの採否 | claimは`attempt_id_v2`と`execution_key`で鍵付けされ、モデルが違えば衝突しない。round・parent単位の単一性制約はない。帰属: bondはattemptの`executor_bond`、escrowは`carrying_block`、Panelは`claim_id`(`palw.rs:9502`)。**順序依存はstep 4bの共有上限だけ**: DAA当たり16 carve(`EmissionBudgetExhausted`)、network room、同一bondのissuance/exposure。負けたsiblingのcarveはcoinbaseで差し引かれた後、escrowされずに焼却される(`palw_v2_merged_escrow_withheld`の超集合性) |
| ADR-0170のseed anchor窓とRED | DAA 5,300以降(live)、採用されたmerged attemptはBLUE/REDを問わずseed anchorを記録(`palw_state_v2.rs:14543-14555`)。slotはlast-write-winsなので**merging blockのconsensus orderがseedを決める**(module doc `palw_anchor_window_v1.rs:31-32`が認める)。4bでskipされたREDはanchorにならない。効くのは次のspanから(`palw_anchor_window_v1.rs:83-85`) |
| partition再合流(経済比較器 vs blue work) | 比較器`compare_palw_candidates_v1`(`palw_fork_choice.rs:72-78`)のkeyは(safe frontier, safe weight, live total, hash)。strict-winで2 DAAより深いtieは現職維持(`palw_fork_authority_v2.rs:221`)。heartbeatだけの分割では両側0で恒久分裂し、finality depth(600 blue score)で封印される(record: 約4–6 h〜7–10 h)。ADR-0178 §5も同じ |
| IBD candidates RwLock再入deadlock | `protocol/flows/src/flow_context.rs:622`(parking_lotは非再入)。修正は`ibd/flow.rs:860`の単一read(`:898`)。commit `57235a5b3`(親`0b1c11b87`)と双子`537b8b000`は**HEADに含まれ、live `rcore/int-12`には含まれない**(hotfixは一括release待ち) |
| safe frontier(比較器の第1key) | `palw_state_v2.rs:14660-14688`(本branchでは`:14692-14712`): 最古の未解決claimより下で、**phaseが`Final`の最深claim**。weightを読まず、sourceも見ない。weight 0のFinal(uncertified-weightless、RFC-0017 probation、budgetで0に切られたclaim、free-promptのFinal)も進める。後退しない(`reverse_convicted_final`はweightだけ引く) |
| rule Eの二つの未解決問題(ADR-0178 §10/§11) | (1) participationは**資本ではなく異なるbondの数**(分割すれば増える)。(2) **voidされたattemptも数える**: `PalwRuleESideAccV1::push`がbondを集合に入れるのは`Voided`の判定より前(`palw_fork_choice_rule_e_v1.rs:463-476`、ADR-0178 §10「no void moves participation」)。 |
| testnet-12の現在の保護 | V2の深いreorgはBFT gate(DNS-BFT)とstrict-winに依存する。**DNS-BFTはRFC-0012で廃止済み**(物理除去はWIP、`origin/codex/rfc12-remove-dns-bft` `344a1b882`、pre-reorgで未build、本branchには取り込まない)。したがって閉鎖モデル攻撃は新規則の中で塞がなければならない(§7) |

#### 2. 主要な所見

* **F-1(sibling順序依存):** 異なるモデルのsibling claimはいずれも受理され得るが、共有上限で競合するとGHOSTDAG順の先頭が勝ち、負けたsiblingのcarveは焼却される。決定的だがモデル間で公平ではなく、merge順の偶然が帰属を決める。
* **F-2(ADR-0170のlast-write-wins):** 同一blockの複数REDのうちseed anchorになるのはconsensus orderの最後。
* **F-3(PALWのRED期限なし):** REDのPALW workはGHOSTDAGのmerge depth(30 block ≈ 1 h)以内にmergeされれば数えられる。それより遅いREDは`ViolatingBoundedMergeDepth`で失われる。
* **F-4(未認証claimの合意credit):** `Final`はPanel licenceとDAA sweepだけで到達し、safe frontier・safe weight・settlement depth・settled anchor・rule E participationを進める。Panelを自分で満たせる閉鎖モデルでは、これが外部の誰にも検査されないまま合意creditになる。
* **F-5(frontierとsettlementの不一致):** frontierは`Final`のclaimをsourceを問わず採り、settlementはattemptの`Final`だけを数える。free-promptの`Final`だけが存在すると、frontierは進むのにsettlement depthは0になる。
* **F-6(operator anchorのheader判定):** merged workで拒否されたoperator attemptもanchorになり得る。
* **F-7(rule Eの分割とvoid):** §1.1のとおり。
* **F-8(DNS-BFT依存):** live testnet-12の深いreorg保護は廃止済みの機構に依存している。
* **F-9(長時間推論とk = 1):** 推論p50 340 sではλ = 2·340/120 = 5.7となり、k = 1のRED率は推定97.7 %(§8)。live実測(2026-10-03)は72 attemptのうち71がRED。

#### 3. liveと休眠の対応

| 機構 | live testnet-12(`0b1c11b87`) | pre / HEAD | 本RFC |
| --- | --- | --- | --- |
| GHOSTDAG・複数親・RED取引受理 | live | 同じ | 変更なし |
| merged work(BLUE/RED)のPALW受理 | live | 同じ | §4のcanonical順序(休眠) |
| lane A operator anchor | live(DAA 750) | 同じ | §5で置換(休眠) |
| ADR-0170 anchor窓 | live(DAA 5,300) | 同じ | §4でcanonical化(休眠) |
| DNS-BFT gate | live(DAA 0)| 同じ。RFC-0012で廃止済み、物理除去はWIP | 依存しない |
| strict economic win | live(DAA 750) | 同じ | Gate Cがkeyの中身を認証済みに限る(休眠) |
| rule E v1 | なし | 休眠・arm不可 | rule E v2(休眠) |
| DNS退役(RFC-0012) | なし | 休眠・rule Eなしでは拒否 | Gate C・rule E v2と組で収束(休眠) |
| `SecurityCertified` | なし | なし | §7(休眠) |
| IBD deadlock hotfix | なし | あり | — |

#### 4. 設計変更1: 多モデル同時生成とRED取り込み(`palw_dag_sibling_attribution_v1`)

##### 4.1 不変条件

* **I-S1(保持):** 有効なsibling blockは色に関わらず全て保持され、merge depth以内に後続blockのmergesetへ入る(GHOSTDAGの既存性質)。MUST NOT: PALWの規則でsiblingをparent候補から外す。
* **I-S2(独立記録):** 各claimは自分のbond・class・carrying blockで記録される(`attempt_id_v2`、`execution_key`、`accepted_block`)。Panelは`claim_id`で鍵付けされ、seedはclaim idを含む(`palw_panel_draw_seed_v1`)。siblingのPanel・escrow・報酬を混同しない(MUST)。
* **I-S3(順序非依存):** 一つのaccepting blockが採用するmerged attemptの集合は、mergesetの**集合**の関数でなければならない(MUST)。色・blue work・merge順・到着順に依存しない。
* **I-S4(モデル間公平):** 共有上限で競合したとき、各モデルのsiblingが1件ずつ採用されてから、どのモデルの2件目も採用される(round robin)。
* **I-S5(seed anchor):** ADR-0170のseed anchorは、同じspanで採用されたattempt(自身・merged、BLUE/RED)のうち`H(domain ‖ execution key)`が最小のもの。last-write-winsではない(`record_round_seed_anchor_of`、`transition_builder.rs`)。
* **I-S6(selected parentのsibling):** siblingのうちGHOSTDAGがselected parentに選んだものは、自分のblockの遷移で先に採用される(merger のmergesetに入らない)。どれが選ばれるかはGHOSTDAGの決定的な規則(blue work、次にblock hash)で決まり、到着順にもmerge順にも依存しない。block hashを動かすには勝ちticket(推論1回)がもう一度要る。共有上限で負けるのは、このselected parentを除いたmergeset内のsiblingで、それはcanonical順で決まる。

##### 4.2 規則(fence以後、accepting blockのDAAで判定)

step 4bの前に、mergesetのattempt workだけを並べ替える。非attempt work(receipt spend等)は元の位置に残す。順序は、モデル(class)ごとに`H(domain ‖ execution key)`の昇順、モデル同士は各モデル先頭の優先度順、1巡につき1モデル1件のround robin(`palw_sibling_canonical_order_v1`、`consensus/core/src/palw_rfc0018_v1.rs:283`、fold `palw_state_v2.rs:14481-14496`)。優先度はsibling自身の推論で決まり、accepting minerは動かせない。siblingのminerが動かすには別の勝ちticket(推論1回)が要る。

##### 4.3 mergeset上限とREDの期限

* mergeset 180、parents 10、heartbeat ≤ 4/mergesetは変えない。round robinはこの上限の内側で働く。
* REDのPALW workの期限はGHOSTDAGのmerge depth(testnet-12で30 block)である。これを超えるREDは`ViolatingBoundedMergeDepth`で失われる。推論時間 + 伝播はmerge depthの時間(約60分)より十分短くなければならない(SHOULD、§8で計測)。
* coinbaseのcarve差し引きは採用集合の超集合のまま(`palw_v2_merged_escrow_withheld`)。負けたsiblingのcarveは焼却。本RFCは「どれが負けるか」をmerge順の偶然からround robinへ変えるだけで、焼却の規則は変えない。

##### 4.4 計測と受入試験

| ID | 試験 | 合格条件(記録する値) | 実装 |
| --- | --- | --- | --- |
| T-1a | 3モデル・3 miner・別Panel、同じ親からsibling(in-process、k = 1) | 3 blockともmergerの過去にある(selected parent/BLUE/RED)。3 claimとも自分のbond・class・carrying blockで記録 | `consensus/src/pipeline/virtual_processor/tests/rfc0018_dag_siblings.rs` |
| T-1b | 同じblock群を別nodeへ、3 siblingの到着順を全6通り | sink、selected parent、blues、reds、PALW state root(採用claim・消費した上限・escrow)、safe frontier、ADR-0170 seed anchorが一致 | 同上 |
| T-1c | sibling fence armで同じ2試験 | 同上 | 同上 |
| T-1d | canonical順は集合の関数、round robin、非attempt slot不変 | 回転・逆順で同じ列。3モデルの先頭3件は3モデル | `palw_rfc0018_v1/tests.rs` |
| T-1e | 実process: 3 producer nodeを分離して同じbaseから3モデルを同時採掘、再接続 | 各siblingの色、claim記録、帰属、Panelがclaimごとに別 | `scripts/drills/rfc0018_dag_drill.py siblings` |

#### 5. 設計変更2: Panel anchorの特権除去(`palw_panel_anchor_permissionless_v1`)

##### 5.1 候補の評価

| 候補 | 許可不要 | grinding耐性 | RED認識 | 評価 |
| --- | --- | --- | --- | --- |
| lane A(現状、operatorのattempt) | ✗(genesis 8 bond) | operatorを信頼 | ✓ | 置換対象 |
| ADR-0130 `round_seed_anchor`(execution laneの勝利推論) | ✓ | 1回の再抽選 = 1回の勝ち推論 | ADR-0170以降✓ | 単一sourceの最後の貢献者が選べる |
| ADR-0170窓(span単位の最新anchor) | ✓ | 同上 | ✓ | last-write-wins(F-2) |
| merged-attemptの集約(採用) | ✓ | 下の上界 | ✓ | 正直な貢献を1件でも含めば予測不能 |
| beacon(RFC07 Part VI/N3) | ✓ | 設計上最良 | — | bootstrapが未成立(N3)。成立後の置換先として残す |

##### 5.2 規則(fence以後、anchor blockのDAAで判定。lane Aより優先)

* **貢献(確定したfold出力):** anchor block Xのselected parentの状態にある**attempt claim**(foldが採用したもの)で、Xより下のchain blockが受理したもの。carrying blockはBLUEでもREDでもよい。phaseに依らない(採用されたものは後でvoidされても採用された)。認識は採用の1 chain block後になる(`palw_permissionless_contributions_from_state_v1`、`palw_rfc0018_v1.rs`)。headerだけのoperator判定(F-6)はなくなる。
* **bondごとに1件:** 各bondの最早`(DAA, block)`の貢献だけを使う。同じbondの後の勝ちは再抽選にもweightにもならない。
* **集約seedとproducer除外:** slot以降の、**claimのproducer以外のbond**の貢献をbond順に並べ、`H(domain ‖ slot ‖ Σ(bond, execution))`(`palw_permissionless_anchor_seed_v1`)。そのようなbondが1つもなければ、Xはそのclaimをanchorしない(`palw_permissionless_anchors_claim_v1`)。この述語を、processorのwalk(`palw_v2_anchor_fact_of_candidate_for_v1`)、binding導出、foldのstep 4c(`palw_void_claims_unbound_in_their_anchor_block_v1`)の三者が同じ確定集合に対して問う。anchorされないclaimはXでbindもvoidもされず、自分のanchorを待つ。
* **到達範囲:** 最新の貢献のDAA(anchor自身のDAAで上限)。draw時刻は最早の貢献のDAA(lane Aと同じ扱い)。
* **旧claim:** anchor blockのDAAがfence未満なら従来(lane AまたはF1)。

##### 5.3 grinding上界

攻撃者が窓内の`b`個のbondを支配し、bond `i`が窓内に`q_i`件の採用attemptを持てるとき、攻撃者が選べるseedの数は高々`Π(1 + q_i)`(各bondで「出さない」か自分の勝ちのどれか)。正直な貢献の実行commitmentは公開前に予測できず、hashの内側にある。`q_i`はADR-0176のbond当たりclaim予算で上限があり、1件ごとに勝ち推論1回の費用がかかる。Panel捕獲確率はこの因数倍を超えない(報告値、§10 T-2b)。

##### 5.4 未解決

* GAP-A1(header近似)とGAP-A2(2 bond最小での代替)は2026-10-15に解消した(確定fold出力からの貢献、claimごとのproducer除外)。残るのは、認識が採用より1 chain block遅れることだけである。

#### 6. 設計変更3: DNS廃止後の収束 — rule E v2(`palw_rule_e_participation_v2`)

##### 6.1 規則(fence以後、現職のDAAで判定。rule E v1を同じ高さ以下で要する)

* **participation v2** = 共通過去`F`の登録で、排他過去に**voidされておらずGate Cで認証済み**のattemptを1件以上持つ異なるbondの、`F`での担保の合計(`PalwRuleEParticipationV2Acc`、`palw_rfc0018_v1.rs:219`、accumulator `palw_fork_choice_rule_e_v1.rs:520`)。
* **順序の修正:** bondは、Voided/未認証の判定を通った後でだけparticipation集合に入る(v2)。Gate Cの下では、未認証のattemptはrecord段階で`attempt = false`になるので、v1の集合にも入らない(`palw_fork_choice_rule_e_v1.rs:357-383`)。
* **even split v2** = 共通資本の1/3(切上げ、最小1)。
* **順序:** participationが数える間(`W_p`以上)は資本、その後に経済key(frontier, safe, live)。それぞれ認証済みのみ(Gate C)。
* **creditなし同士(§6.4):** 両側とも認証済みcreditが一切ない(資本0、frontier 0、safe 0、live 0)ならGHOSTDAG順で決める(`palw_rule_e_decide_v2`、`:804`、processor `palw_rule_e.rs:218-226`)。IBDはtieで現職を保つ(従来どおり)。

##### 6.2 性質

* **分割耐性:** 資本に線形で、bond当たりの上限も下限もないので、資本を`n`個のbondへ分けても合計は分けない場合を超えない(全片が認証済みattemptを持つとき等しい)。23,076分割の検算はT-0。
* **bond式v2との整合:** creditは`F`での担保`C`で、bond式v2の`u·C`と予算engineの`F_max(C, W)`が読むのと同じ`C`。予約や`F`以後の入金は数えない。
* **void・未認証の除外:** voidされた(G14でconvictされたものを含む)attemptと未認証のattemptは数えない。
* **leaf v2:** clientのleafは資本を運ばない(GAP-R4)。v2を宣言していないnetworkではnodeも資本を0として扱い、clientのpairと一致する。

##### 6.3 Gate Cとの組

DNS退役後の収束は**Gate Cとrule E v2の組**で行う。fork choiceの全key(participation、frontier、safe、live)は認証済みclaimだけから作られ、未認証claimは誰に対してもcreditを持たない。

##### 6.4 creditなし区間の扱い

両側がcreditを持たない区間(heartbeatだけ、または未認証claimだけ)は、PALWのfinalityが一切報告されていない区間である(§7.8)。GHOSTDAG順で決めても、報告済みのfinalityは覆らない。これでheartbeatだけの分割(ADR-0178 §5)も収束する。finality depthを超えた分割は従来どおり封印される。

##### 6.5 受入試験

| ID | 試験 | 合格条件 | 実装 |
| --- | --- | --- | --- |
| T-3a | v2のorderはv2 fence未満でv1と同一、反対称 | 全組合せ | `palw_rfc0018_v1/tests.rs` |
| T-3b | 分割中立・bondは1回・void/未認証/非共通は0 | 資本の等式 | 同上 |
| T-3c | 実process: partition、再接続、新規IBD node。linkごとに配送遅延を変え到着順を変える | sink、sinkのmerge読み(selected parent/blues/reds)、採用claim集合、PALW経済状態、fork choiceの立場が全nodeで一致 | `rfc0018_dag_drill.py partition` |
| T-3d | DNS退役+rule E v2+Gate Cのlocal calibration devnetで同じ | 同上+未認証claimのcredit 0 | `rfc0018_dag_drill.py retired` |

#### 7. Gate C — `SecurityCertified`(`palw_independently_work_certified_v1`、ユーザー方針により採用)

##### 7.1 二つの状態

| 状態 | 意味 | 何が起きるか |
| --- | --- | --- |
| `Final` | claimの手続きが完了 | 利用者払いescrowの経済的settlementは既存規則どおり |
| `SecurityCertified` | 合意creditを持ってよい | safe frontier、safe weight、bounded immature/live total、settlement depth、rule E participation、settled anchor、発行由来(coinbase)報酬legに寄与 |

不変条件(fence以後、claimの受理DAAで判定):

* **I-C1:** `SecurityCertified`でないclaimは、上の7つのどれにも寄与しない(MUST NOT)。
* **I-C2:** `Final`、Panelのlicence、Panelの過半、モデル保有者の集合、登録者が満たせる集合のいずれも、単独で認証にならない(MUST NOT)。
* **I-C3:** 認証済みclaimが後でG14の公開告発で反証されたら、voidされ、合意creditは取り消される(MUST)。safe weightは`reverse_convicted_final`で引かれ、participation・settlement depthから消える。
* **I-C4:** DNS-BFTおよびDNS finality anchorに安全性・収束を依存しない(MUST NOT)。
* **I-C5:** 何も認証されない間も、DAG、通常取引、heartbeat/BASE-0の時計、DAAは進む。止まるのはfinality creditだけ(MUST)。

##### 7.2 認証述語(前提に基づく認証)

1. **commit:** 支配されたclaimの`Final`で、claim・producer・母集団の切断点を固定する(`commit_at_final_v1`、`consensus/core/src/palw_security_certification_fold_v1.rs:45`)。
2. **draw:** commitより厳密に後のexecution laneのseed anchor(ADR-0130)から作るseedで、**global pool**(Active、commitの`verifier_maturity`前に登録、最小担保以上、producerのoperatorを除く)から`m`位置を担保比例・復元抽選する(`palw_certification_pool_admits_v1`、`palw_security_certification_v1.rs:280`、RFC-0017/R7-ECONの抽選を再利用)。**モデル保有の有無で母集団を絞らない**。
3. **attest:** 抽選されたbondは期限内に、そのclaimを再実行して判定を署名する(RFC-0017のinner 75を、exam idの代わりにcertification idで使う、`try_attest_claim_certification_v1`、fold `:111`)。
4. **decide:** `k`位置以上が`Reproduced`(mismatchなし)で、claimがまだ`Final`、未解決の告発がなく、**G14公開告発window**(`prosecution_window_daa`)が`Final`から経過したら`Certified`(`palw_certification_verdict_v1`、`:320`)。期限までに`k`未満なら`NotCertified`、母集団不足は`NoPopulation`、`certify_by`までに抽選できなければ`Expired`、`Final`でなくなれば`Withdrawn`。**1 claimにつき1回、再試行なし、classごとに数える**(attemptの有界化と計数)。network全体のepoch当たり抽選数にも上限がある。
5. **公開check経路:** 決定論的な公開検査(G14完了classの公開material)が固定されたclassは、告発window経過で`PublicCheckCertified`。このbinaryでは固定classは0件(`PALW_SECURITY_PUBLIC_CHECK_CLASSES_PINNED_V1 = &[]`)。

**前提(明示):** 認証は抽選検証者の署名付き承認に依る。**「抽選されたverifier stakeのうち正直な割合が閾値(`k/m`)以上である」ことを前提とする。** 署名は再計算の証明ではない。これは無条件の数学的証明ではなく、前提に基づく認証である。RPCはこれを`securityCertificationPremise`として返す(§7.8)。

閉鎖モデルは、正直な抽選検証者が入手も再実行もできないので、正直な位置から肯定承認を得られない。攻撃者が`m`位置中`k`以上を引いたときだけ認証される。

##### 7.3 前提が崩れた場合の影響(報告値、設計の変更条件ではない)

攻撃者のverifier stake比`f`(抽選される母集団に占める割合)ごとの、閉鎖モデルの1 claimが認証される確率`P(X ≥ k)`、`X ~ Bin(m, f)`(`palw_certification_false_pass_f64`):

| `f` | 7-of-5 | 47-of-32(設計値) | 23,076 claimでの期待認証数(47/32) |
| --- | --- | --- | --- |
| 0.03 | 4.85e-7 | 8.9e-38 | ≈ 0 |
| 0.25 | 1.29e-2 | 6.4e-10 | 1.5e-5 |
| 1/3 | 4.53e-2 | 1.19e-6 | 0.027 |
| 0.6 | 0.420 | 0.163 | ≈ 3,762 |
| 1(最悪) | 1 | 1 | 23,076 |

* 7-of-5では`f = 1/3`で23,076 claimのうち約1,045件が認証される。claim数に比例して前提失敗の影響が積み上がるので、設計値は`m = 47, k = 32`とした(未承認、U-5)。費用は認証されるclaimごとに`m`回の再実行である。
* `f = 0.6`や最悪ケースでは、本設計は合意creditを守れない。これは前提の外側であり、設計を変える条件ではない(ユーザー方針2)。
* veto(否定`v`位置で拒否)は追加案U-6であり、採用設計ではない(設計値`v = 0`)。

##### 7.4 合意creditの加算点(一箇所)

認証が決まったとき、sweep(`sweep_security_certification_v1`、fold `:234`、kernel routeの終了tick `palw_kernel_route_fold_v1.rs:1671`)が、再導出と同じ式でcreditを加える(`credit_certified_v1`、`:196`): attemptはFinalのsafe weight(canonical基準、F-Wの上限、予算の付与)とsettled anchor 1件、free-promptはそれまでのspendのweight。発行由来の報酬legは`Final`で保留され(`hold_issuance_reward_at_final_v1`、`:91`)、認証時に支払われ、失敗時は名付けられない(焼却、新規発行なし)。

##### 7.5 frontierとsettlementの単一規則

fence以後、frontierとsettlement depthの両方で、合意creditを生むのは**認証済みattemptの`Final`だけ**とする(`palw_consensus_credit_anchor_v1`、`palw_security_certification_v1.rs:367`)。free-promptの合意creditは認証後のspendのweightだけで、frontierもsettlement depthも動かさない。「free-promptの`Final`だけが存在する」場合は、frontierなし・depth 0(F-5の解消、T-0c)。fence未満は従来(frontierは任意source、settlementはattempt)。

##### 7.5a frontierの巻き戻し(conviction後の再計算)

fence以後、safe frontierは毎block**再計算**する: 解決済みprefix内の、frontier資格のある`Final`の最深claim(認証済みattempt。fence未満で受理されたclaimは従来規則)、ただし**floor**を下回らない(`palw_gate_c_frontier_v1`)。

* **floor:** fenceが効いた最初のblockで、そのときのfrontierで初期化する(有効化でfrontierが戻らない)。資格のある`Final`のclaimが**退役**したとき、その位置まで上げる。退役したclaimはもうconvictされない(`reverse_convicted_final`は退役済みを扱わない)。kernel-route aux 136の`'F'` row(`note_retirement_frontier_floor_v1`、`ensure_frontier_floor_v1`)。
* **巻き戻し:** 認証済みclaimが後でconvict(void)されると、そのclaimはfrontierでなくなり、frontierは残る資格claimの最深、またはfloorへ戻る。deltaの`Frontier`エントリが任意の移動を記録するので、reorgは正確に戻し、IBDはcarriageでfloor rowを受け取る。
* **pruning:** pruning上限は、進行の上限(§7.7)と**巻き戻し上限**(convictionで再びfrontierになり得る最も浅い位置 = stateに残る資格`Final`の最小受理blue score、`palw_frontier_rollback_bound_v1`)の小さい方を超えない。再びfrontierになり得る点をpruneすることはない。資格claimは退役で必ずstateから消えるので、上限は有界に進む。

##### 7.6 寿命

認証rowはclaimの退役まで残り、退役後にsweepで消える。`claim_retirement_daa ≥ lifetime_daa()`(`certify_by + attest window`と告発windowの大きい方)でなければならない(MUST、activation checklist)。満たさないと`Final`のclaimが未認証のまま退役する(creditなしで整合はするが、正直なclaimも認証されない)。

##### 7.7 pruning progress

fence以後、frontierは認証済みclaimでしか動かないので、pruningはfrontierを待たない。上限は、最古の未解決claimと、認証が未決の最古の`Final` claimの、受理blue scoreの小さい方の1つ下(`palw_pruning_progress_ceiling_v1`、`:379`、processor `palw.rs:719-750`)。open claim・court・未決の認証が読む履歴は消さない(MUST)。未決の認証は`lifetime_daa`で必ず決まるので、pruningの停止は有界である。

##### 7.8 settlementの意味とRPC

DNS-BFTがないので、settlementの`safe`/`settled`はPALWの認証済みcreditだけを意味する。未認証claimだけのchainは**PALW settlement finalityなし**(`settled = false`、`depth = 0`)と報告し、`uncertifiedFinalAnchors`(認証されていない`Final` attemptの数)と`securityCertificationPremise`(前提の文言)を返す(`GetPalwSettlementResponse`、wire version 4、gRPC field 13/14)。claimごとの認証状態は`GetPalwClaims`のrow(`securityCertification`、wire version 6、gRPC field 39)で返す。

**leaf-v2 client経路:** rule Eのrecord(`PalwRuleEClaimRecordV1`)は真のphaseと**認証状態**(`certification`: NotGoverned / Certified / Uncertified)を持ち、leaf-v2 windowにcommitされる。Gate Cの適用(未認証はparticipationなし・frontierなし、FPの`Final`はfrontierなし)はnodeとclientが共有するaccumulatorで行う。clientは`palw_rule_e_certification_summary_v1`で、未認証`Final`の数と前提の文言を得る(`palw_fork_choice_rule_e_leaf_v2.rs`)。

**header commitment(GAP-R4、2026-10-15に解消):** rule E かつ(rule E v2 または Gate C)が state の点で効くとき、header は **rule-E leaf v3** を commit する(`palw_fork_choice_rule_e_leaf_v3.rs`): window の record を認証状態込みで、registry を `(bond, collateral)` で、registry の資本合計を leaf に。どの文法かは読み手自身の ruleset が点で決める(`palw_rule_e_commit_form_v1`: flat / v1 / rule-E v2 / rule-E v3)。node は `palw_committed_state_root_v2` を commit し、v39 anchor・L2 client(`verify_committed_opening_v2`)・carriage RPC は同じ `palw_verify_committed_leaf_v2` で開く。v3 の fence 未満では v1/v2 の envelope は byte 単位で不変(leaf v2 の window leaf は認証状態の field を含まない)。native settlement の roots chain は、rule E 下では各 row の post-state から commit root を計算する(rule-E leaf は delta から逆算できないため)。RFC-0012のnative settlementも、fence以後は`Final`遷移ではなく認証遷移を成熟workの証拠とする(`palw_native_settlement_v1.rs:161`、`:338`、`:404`)。

##### 7.9 ADR-0177の変更(明示)

ADR-0177は「モデルの入手不能だけでは報酬もweightも止めない」とした。**合意creditについてこれを変更する:** モデルを入手できない検証者は再実行できないので、そのモデルのclaimは認証されず、合意creditを持たない。すでに認証されたworkの報酬は取り消さない(告発による反証を除く)。入手不能を理由にslashしない点は変わらない。

##### 7.10 genesisモデルと70B

genesis登録は認める。**genesisモデルにも認証の免除はない**。70Bモデルは、独立した検証(抽選検証者の再実行)が成立するまで合意creditを持たない。

##### 7.11 受入試験 — 必須の最上位試験「300M閉鎖モデル/自前Panel攻撃」(T-0)

**合格条件: 不正な合意finalityが一切ない。** 6つの量を**別々に**報告する。

| ID | ケース | safe frontier移動 | safe weight | settlement depth | rule E participation比 | settled anchor | 報酬流出 | 実装 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| T-0a | 300Mを13,000 BILIの最小bond 23,076個に分割、閉鎖モデル、自前Panel、未決/veto/expire | 0 | 0(FP分を除き) | 0 | 0 | 0 | 0 | `consensus/core/src/palw_rfc0018_v1/attack_tests.rs` |
| T-0b | 1/7のattemptをvoid | 同上 | 同上 | 同上 | 0(順序修正) | 同上 | 同上 | 同上 |
| T-0c | free-promptの`Final`だけ(認証済みでも) | 0 | spend分のみ | 0 | — | 0 | — | 同上、`tests.rs` |
| T-0d | reorg(両方向)とIBDのclaim-set経路 | 正直側が現職でも挑戦者でも勝つ | | | | | | `attack_tests.rs` |
| T-0e | 分割の中立性(仮に認証されても23,076片 = 1 bond) | | | | 等しい | | | 同上 |
| T-0f | 実process: 定常、reorg(分離と再接続)、restart、pruning progress、新規IBD | 各phaseで上の6量が0、premiseがRPCに出る、pruning pointが進む | | | | | | `scripts/drills/rfc0018_dag_drill.py attack` |
| T-0g | 前提失敗時の影響(f = 1/3、0.6、1)を報告 | 報告のみ | | | | | | `attack_tests.rs` |

その他のGate C試験: identity(fence未満・出荷network)、`Final ≠ SecurityCertified`、Panel licenceは認証ではない、失敗outcomeは全て0、voidは常に未認証、provisionalは0で一定、verdict(m-of-k、告発window、未解決告発、期限、expire、withdraw)、母集団がglobalでproducerを除く(`palw_rfc0018_v1/tests.rs`、`palw_security_certification_v1.rs`内の単体試験)。

#### 8. DAG幅(`palw_dag_width_schedule_v1`、仕様とstub)

##### 8.1 現状

120 s、k = 1、parents 10、mergeset 180、merge depth 30 block、finality 600、heartbeat ≤ 4/mergeset。heartbeatはADR-0105で透過(bonded blockの着色を邪魔しない)。

##### 8.2 RED率の推定(Poisson、λ = 2·d/T、`palw_dag_width_red_permille_v1`)

| 実効遅延 d | k = 1 | k = 2 | k = 3 | k = 4 | k = 8 |
| --- | --- | --- | --- | --- | --- |
| 2 s(伝播) | 0.1 % | 0.0 % | 0.0 % | 0.0 % | 0.0 % |
| 30 s | 9.0 % | 1.4 % | 0.2 % | 0.0 % | 0.0 % |
| 120 s | 59.4 % | 32.3 % | 14.3 % | 5.3 % | 0.0 % |
| 340 s(8k p50) | 97.7 % | 92.1 % | 81.6 % | 66.8 % | 12.0 % |
| 418 s(8k p95) | 99.2 % | 97.0 % | 91.6 % | 82.4 % | 26.7 % |

T = 60 sでは340 sの推論はk = 8でも79.6 %がRED。推論が長いworkは、kを広げてもBLUEにはならない。

##### 8.3 分析

* **PALWの取り込みはREDでも成立する**(§1 c)。RED率が問題になるのは (i) GHOSTDAGのblue workに入らないこと(PALWはblue workをfork choiceの主keyにしない)、(ii) merge depth内にmergeされる必要、(iii) ADR-0165のfloor状態機械がREDでNormalを延長しない点である。
* **throughput:** chain block 1つでmergeset 180、parents 10。多モデルの同時tipが10を超えると、複数blockにわたってmergeされる。merged attemptの採用は、DAA当たり16 carveの発行予算が先に効く(§4のround robinで公平化)。
* **安全余裕:** kを上げるとblue集合が広がり、私的に作ったblockもBLUEになりやすい。PALWではfork choiceの主keyは認証済みcredit(Gate C)なので、kの拡大がfinalityを直接与えることはない。それでもGHOSTDAGの順序の安全性(DAA、merge depth、pruning)はkとBPSに依存する。
* **heartbeat:** BPSを変えるとheartbeatの間隔、DAA窓、PALWの全DAA単位の窓(bind、receipt、challenge、court、retirement)が変わる。DAAで表した値の協調した再導出が要る。

##### 8.4 stubの理由

GHOSTDAGの`k`、parentsの上限、mergesetの上限は、GHOSTDAG managerとheader pipelineのconstructorのscalarである。block時間はPALWの全DAA窓を伸縮させる。高さで切り替えるには、selected parentのDAAで鍵付けしたblock単位のreader(crescendoの`ForkedParam`の形)を、header処理、GHOSTDAG、difficulty、pruningの全経路に通す必要があり、すべてlive経路に触れる。本RFCはfence(`PalwDagWidthScheduleFenceV1`、`palw_rfc0018_v1.rs:439`)、構造検査、id(fingerprint)、RED推定関数だけを置き、readerは置かない。validationはarmを拒否する。

#### 9. 他RFC・ADRとの関係

* **RFC-0016 V+:** モデルのblock権利の消費は、同じmergesetのsibling間で**merge順に依存してはならない**(MUST)。§4のcanonical順は、権利の消費をstep 4bの採用順と同じ集合関数にする。V+の権利は合意creditではなく生成機会であり、Gate Cとは独立(権利を消費しても、認証されるまでcreditはない)。
* ~~**RFC-0017(U-2、P0):** Probation の claim を `SecurityCertified` にしない bar~~ — **2026-10-15 第 5 回で削除した。** モデル単位の審査はすべてのモデルで廃止され([PRINCIPLES §9](../PRINCIPLES.md)、[ADR-0179](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md))、bar の対象が存在しない。Gate C の claim 単位認証が、genesis・年次指定・登録のすべての階層で唯一の安全性 gate である(Gate C 自体は不変)。
* **RFC-0017(2026-10-15 第 5 回で改定):** モデル階層(genesis・年次フォーク 1.0、登録 0.3)は経済上の権利だけを決める。モデル単位の審査はない。claim 単位の合意 credit は Gate C だけが決め、どの階層も免除しない。`palw_model_eligibility_v1` は inner 74/75 の A-2 gate として残り、tier fence `palw_model_tier_v1` が同じ高さ以下にあるときだけ arm できる。
* **ADR-0176:** participation v2のcreditは担保`C`、予算engineの`F_max(C, W)`と同じ`C`。Gate Cは予算の予約を変えない(予約は受理時、creditは認証時)。
* **ADR-0178:** rule E v2が§10/§11の二つの未解決問題を閉じる。v1の意味は変えない。
* **RFC-0012:** DNS退役後のsettlementは認証遷移を証拠とする(§7.8)。

#### 10. 受入試験の一覧

T-0(§7.11、必須の最上位)、T-1(§4.4)、T-2: permissionless anchor(seedはclaimのproducer以外のbondを要し、未採用を無視し、貢献は確定fold出力(REDのcarrierを含む)で、bondごとの最初の貢献の関数で、順序に依存しない — `palw_rfc0018_v1/tests.rs`。T-2b: grinding上界`Π(1+q_i)`の実測報告 — 未作成)、T-3(§6.5)、T-4: DAG幅推定の単調性とschedule構造検査、T-5: DNS退役calibration devnet(`retired`)。

#### 11. 本RFCで決めない値

| 名前 | 設計値(未承認) | 意味 |
| --- | --- | --- |
| `PALW_CERT_M_DESIGN_V1` / `_K_` | 47 / 32 | 抽選位置数と必要肯定数 |
| `PALW_CERT_VETO_DESIGN_V1` | 0(なし) | veto案(U-6) |
| `PALW_CERT_ANCHOR_DELAY_SPANS_DESIGN_V1` | 1 | commit後のseed anchor遅延 |
| `PALW_CERT_ATTEST_WINDOW_DAYS_DESIGN_V1` | 2日 | 承認期限 |
| `PALW_CERT_PROSECUTION_WINDOW_DAYS_DESIGN_V1` | 7日 | G14公開告発window |
| `PALW_CERT_CERTIFY_BY_DAYS_DESIGN_V1` | 3日 | 抽選の期限 |
| `PALW_CERT_VERIFIER_MATURITY_DAYS_DESIGN_V1` | 7日 | 母集団の成熟 |
| `PALW_CERT_DRAWS_PER_EPOCH_DESIGN_V1` / `_EPOCH_DAYS_` | 4,096 / 1日 | network全体の抽選上限 |
| `PALW_RFC0018_ANCHOR_MIN_DISTINCT_BONDS_DESIGN_V1` | 2 | 集約anchorの最小bond数 |
| `PalwDagWidthScheduleV1` | なし(probeはk 3、120 s、16、360で構造試験用) | DAG幅 |
| `W_p`(rule E) | 20 DAA(現行) | participationが数え始める深さ |

#### 12. ユーザー判断が必要な事項

* **U-1** 各fenceの有効化と順序(Gate C ≥ RFC-0017 eligibility・kernel route、rule E v2 ≥ rule E、DNS退役 ≥ rule E)。
* **U-2** 発行由来報酬legの保留を、producerへの1 legで支払うか(本実装)、Panel share・buyback・vestingへ分けるか(GAP-C4)。
* **U-3** free-promptの認証前spendのcoinbase carve(GAP-C3、本実装はweightだけ止める)。
* **U-4** DAG幅scheduleの値と、block単位readerを実装するか。
* **U-5** `m`/`k`と、認証1件あたり`m`回の再実行費用の負担。
* **U-6** veto案の採否。
* **U-7** creditなし同士のtieをGHOSTDAG順にする規則(§6.4)の承認。
* **U-8** 公開check経路に固定するclassと、その計測記録。

#### 13. 実装(休眠、未build、未試験)

##### 13.1 fenceと割り当て

| 種類 | 割り当て | 確認 |
| --- | --- | --- |
| fence | `palw_dag_sibling_attribution_v1`、`palw_panel_anchor_permissionless_v1`、`palw_rule_e_participation_v2`、`palw_independently_work_certified_v1`(`Option<ForkActivation>`)、`palw_dag_width_schedule_v1`(`Option<PalwDagWidthScheduleFenceV1>`)— `config/params.rs:3186-3206` | 全preset `None`(4 preset literal)、`identity.rs`: collapse・pin list・fingerprint 2箇所Some-only・visit・destructure 3箇所、`fork_id_v1.rs`のprobe、`validation.rs`、mirror `PalwStateParamsV2::rfc0018_v1`(`#[borsh(skip)]`、`state_params.rs:689-697`)、sync `Params::sync_palw_rfc0018_v1` |
| kernel-route aux table | **136**(65-byte key: `'S'`認証row、`'C'`class計数、`'N'`network epoch) | 使用中〜135をgrep。scanner armは`gen_rfc06_native_wire.py`と`gen_rfc06_native_inventory.py`へ追加し、生成器を再実行してrustfmt(`wire_kernel_aux.rs`、`wire_generated.rs`、`generated.rs`) |
| kernel inner kind | 新規なし。RFC-0017のinner 75を、certification idを名指す承認として再利用 | — |
| RPC | 新opなし。`GetPalwSettlementResponse` wire version 4(`uncertifiedFinalAnchors`、`securityCertificationPremise`)、`RpcPalwClaimRow` wire version 6(`securityCertification`)。gRPCはmessage内のfield番号(settlement 13/14、claim row 39)を追加 | RPC op番号 ≤ 244/255、RpcRequest/Response oneofの番号 ≤ 1307は不変 |
| object tag / p2p / funded action | 割り当てなし | — |

##### 13.2 module

* `consensus/core/src/palw_rfc0018_v1.rs` — fence、mirror、Gate Cの述語と各site helper、rule E v2のaccumulator、canonical sibling順、permissionless anchor、DAG幅stub、validation。単体試験`palw_rfc0018_v1/tests.rs`、最上位試験`palw_rfc0018_v1/attack_tests.rs`。
* `consensus/core/src/palw_security_certification_v1.rs` — 認証のpolicy、row、鍵、母集団、抽選、verdict、frontier/settlementの単一規則、pruning progress、前提失敗の影響関数、RPCの前提文言。
* `consensus/core/src/palw_security_certification_fold_v1.rs` — `palw_state_v2`の子module: commit、報酬保留、承認、sweep、credit加算。

##### 13.3 Gate Cの読み取り箇所(全て、fence未満は従来どおり)

| 合意credit | 箇所 |
| --- | --- |
| participation(rule E v1/v2) | `palw_fork_choice_rule_e_v1.rs:357-383`(record: 未認証は`attempt = false`、未認証`Final`は`Voided`扱い、認証済みFPの`Final`はfrontierなし)、accumulator `:520`、v2 `palw_rfc0018_v1.rs:219` |
| provisional weight(bounded immature/live total) | `transition_builder.rs:8286`(reserve)、`:8311`(release)、`:9311`(void hold)、`chain_state.rs:3367`(整合性の再導出)、`palw_weight_cap_v1.rs:250`(F-W staged)、`:503`(`palw_bounded_immature_v2`)、rule E record `palw_fork_choice_rule_e_v1.rs:420`、予算engine `:296` |
| final weight | `transition_builder.rs:8934`(`finalize_claim`)、`:5060`(`reverse_convicted_final`)、`:9456`(`retire_claim`)、`chain_state.rs:3318`(整合性)、`palw_state_v2.rs:18974`(receipt spend)、`chain_state.rs:3334`(FPの整合性)、認証時の加算 fold `:196` |
| safe frontier | `palw_state_v2.rs:14692-14712`(`palw_gate_c_advances_frontier_v1`) |
| settlement depth | `palw_settlement_v1.rs:89-104`、native settlement `palw_native_settlement_v1.rs:161`、`:338`、`:404` |
| settled anchor | `transition_builder.rs:7545`、`:7744`、`:8630`、`:9194`(fence以後はどれも数えない)、認証時 fold `:196` |
| coinbase(発行由来報酬leg) | `transition_builder.rs:9000`(`Final`で保留)、認証時の支払い fold `:196` |
| commit / 承認 / sweep | `transition_builder.rs:8895`、`palw_kernel_route_fold_v1.rs:1079`、`:1671` |
| pruning | `processor/palw.rs:719-750` |
| RPC | `rpc/service/src/service.rs:2120-2128` |

##### 13.4 driver

`scripts/drills/rfc0018_dag_drill.py`: `siblings`(T-1e)、`partition`(T-3c)、`retired`(T-3d)、`attack`(T-0f)。plan固定、node・app dirは各自専有、P2Pはdriver所有のloopback link(方向ごとの配送遅延で到着順を変える)、全判定は安定したsinkでの公開wRPC。driverは何もarmしない。fenceを出さないbinaryではBLOCKED(出荷presetでは常にこれ)。statusは`OBSERVED_CONSISTENT`/`BLOCKED`/`FAIL`。

##### 13.5 compile上のリスク(未build)

* 新fieldを持つstruct literal(`PalwRuleESideV1`、`PalwRuleEPairV1`、`PalwSettlementV1`、`GetPalwSettlementResponse`、`NativeDeltaEvidenceV1`、`SkippedEvidenceV1`、`PalwChainAnchorV1`)の取りこぼし。grepで既知の構築箇所は更新済み。
* `palw_security_certification_fold_v1`の即時呼出closure(`try_attest_claim_certification_v1`)の借用、`commit_at_final_v1`での`ensure_route_header`呼出(合成した`PalwBlockContextV2`)。
* processorのanchor walkの新しい変数と、`palw_rule_e.rs`のclosure型注釈。
* `PalwRfc0018MirrorV1`の`Default`(内部policyの`Default`に依存)。
* T-1のE2E試験は2M/8k行を`Active`にplantしてattemptを作る。heldのclassではproducer factsが拒否する可能性がある。

#### 14. 有効化gate

~~~text
ACTIVATION_ALLOWED(各fence)
    = ユーザーの有効化判断(U-1)と §11 の値の承認
      AND 単体試験・最上位試験 T-0(attack_tests.rs)を validation が実行して合格
      AND T-0f(attack)を local calibration devnet で実行し、全phaseで 6 量が 0
      AND T-1e / T-3c / T-3d を real node で実行し、収束を記録
      AND claim_retirement_daa ≥ certification lifetime_daa(§7.6)
      AND Gate C ≥ palw_model_eligibility_v1・palw_probabilistic_constraints_v1、rule E v2 ≥ rule E、DNS退役 ≥ rule E
      AND A-2 一様試験(fence 遥か上で全block同一)
      AND 前提失敗の影響(§7.3)をユーザーへ報告済み
~~~

#### 15. 参照

* [PRINCIPLES](../PRINCIPLES.md)(§0、§8)、[ADR-0058](../adr/README.md)、[ADR-0105](../adr/README.md)、[ADR-0130](../adr/README.md)、[ADR-0170](../adr/0170-the-seed-anchor-is-a-window-not-a-span.md)、[ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)、[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)、[ADR-0178](../adr/0178-fork-choice-heals-a-partition-on-bonded-participation.md)
* [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md)、[RFC-0016](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md)、[RFC-0017](../rfc/0017-model-eligibility-examination-and-renewal.md)
* [ユーザー判断 2026-10-15 §6](../history/decisions/2026-10/user-decisions-2026-10-15-adr-rfc16-rfc17.md)
* [実装状況](../design/palw/integration/rfc-0018-implementation-status-2026-10-15.md)

#### 改訂(2026-10-17)— 共有 frontier と独立な進行

共有 safe frontier の依存を「本当に依存している責任」に限る方針は [RFC-0023](../rfc/0023-unresolved-claim-liability-and-independent-progress.md) / [ADR-0186](../adr/0186-hold-unresolved-liability-advance-independent-claims.md) に記録した:未解決 claim の責任を保持し、それに依存しない claim を進める。
