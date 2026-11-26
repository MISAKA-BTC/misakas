# RFC-0006 — TB1：用途別乱数・公開・復元

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**ADOPTED_DESIGN_NOT_ACTIVATED**。TB1の用途別秘密・suite・freeze・公開規則を維持する。
- 判断理由：[ADR-0006](../adr/0006-threshold-randomness-and-custody.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. 故障モデルと閾値

MUST、MUST NOTは本RFCを使用する新profileの要件である。

\[
n=3f+1,\qquad t=f+1,\qquad q=n-f=2f+1.
\]

| 記号 | 意味 |
| --- | --- |
| n | round前に固定した保持者slot数 |
| f | 悪意・秘密漏洩・停止を含む故障slotの総予算 |
| t | 正しいshareから秘密を私的に復元できる閾値 |
| q | 公開certificateに必要な、同一多項式上の相異なる署名付き点数 |
| Q | 完全VSSが準備完了したdealerの固定集合。`|Q| >= q` |

`t`と`q`を混同しない。q件の公開証拠が揃うより前に、f個の悪意shareと最初の一つの正直shareで秘密が復元され得る。fault budget fとは別に、無制限の正直offlineを許さない。同一運営者の複数slotは全て故障予算へ数える。

添付参照例は `n=7,f=2,t=3,q=5`。容量候補例 `n=64,f=21,t=22,q=43` は本番parameterの決定ではない。基礎論証は静的腐敗集合を対象とし、量子・適応/mobile侵害・並行session・nativeへの合成は別の検証義務とする。秘密保護期間の累積侵害を、瞬間的なf以下という観測で代替しない。

完全VSS、独立した正直entropy、PQ署名・秘匿channel、hash、native stable broadcast、期限内包含、rosterの故障上限を明示した前提として置く。正直なLLM Panelが一人いるという条件から、TB1保持者のf上限は導けない。

## 2. Purposeと全consumerの対応

論理purposeは `EXEC`、`PANEL`、`SCHEDULE`、`ROLE`、`ADMISSION`、任意評価の `MODEL_AUDIT` とする。未知purposeは拒否する。各purpose／roundは**独立した秘密・多項式・VSS session・公開gate**を持つ。公開済みseedのdomain再hashを、新しい未来未知性の代用にしない。各purposeのwire tagとsuite版は有効化時に一度だけ固定する。

| consumer | 乱数・処理 | 最初の正直share公開前に固定する対象 | 新profileでの扱い |
| --- | --- | --- | --- |
| FOW1の実入力 | TB1 `EXEC` の `sigma_E` | 完全C、owner、全nonce候補、model/job/profile、target、会計、上限・原期限、EXEC/PANEL参照 | 必須。sと実token入力へ結合 |
| 選出Panel | TB1 `PANEL` の `sigma_P` | 署名済みOriginal、受理集合、pool、重み、DrawPolicyId、担当義務・原期限 | 必須。EXECと別秘密・後続gate |
| PositiveNonce・担当別局所challenge | 正規PANEL値からdomain分離した決定論的派生 | 元Originalと固定担当規則 | 別の未来乱数を主張しない。各actor/義務へ束縛 |
| 決定論的生成sampling | EXEC/sと登録済みPRNG規則からの派生 | 入力・生成profile、PRNG・消費順・丸め | 追加beacon不要。生成用Rとtrace Rootを混同しない |
| 採掘Ticket | 登録された実行依存Ticket式 | 入力・実行・出力・trace・targetの同一文脈 | 追加のTB1 Ticket purposeを設けない |
| FOW1の候補処理順 | 正規Ticketとslotによる固定順 | ready集合、cursor、全資源契約 | 独立ADMISSION抽選や二重Dを追加しない |
| seed順位を使う別の新受付control profile | TB1 `ADMISSION` の `sigma_A` | 候補集合、事前D、順位規則、容量・対象epoch | 当該profileが明示採択された場合だけ使用。未設定なら無効 |
| EXEC生成権schedule | TB1 `SCHEDULE` の `sigma_S` | 適格credit集合、予約、割当規則、未来slot範囲 | 未認証仕事からseedや権利を生成しない |
| 次epoch保持者roster | 前epoch保持者によるTB1 `ROLE` の `sigma_R` | 次候補lease/担保lot集合、抽選規則、n/f、次suite/schedule | 自分のrosterを自分の未生成乱数で選ばない |
| negative、資料要求・応答、Court、取消・終端 | 新乱数を要求しない | 元claim、元予約、元期限 | TB1停止中も既受理救済を進める |
| 任意評価の実用評価入力 | 独立TB1 `MODEL_AUDIT` | 不変candidate/descriptor、suite、生成規則、予算、期限、test集合規則 | 非報酬用途。最初の正直share前に固定。採掘Ticketを生まない |
| 任意評価の再実行担当 | 別のTB1 `PANEL` | 非報酬評価の元提出物、pool、義務、原期限 | 評価提出後に固定。通常採掘PANELと別round |
| モデル安全停止 | 新乱数なし | 事前確定roster、MODEL_STOP権限、risk case | q署名の停止証明。TB1 share証明の代用や再利用を禁止 |

### 2.1 追加抽選を作らない境界

[RFC-0014 §6](0014-exec-schedule-and-capacity.md#6-受付難易度と資源制御)のFOW1受付順序 `(ticket, slot_id)` は維持する。RFC-0014の旧受付順位案のseed由来受付順位を、この同じFOW1経路へ追加してはならない。その旧順位機構を別の新control profileへ適用する場合だけ、`ADMISSION`としてTB1へ移す。これは採掘Ticketの第二抽選ではなく有限候補の処理順位であり、D合格・正当性・creditを単独で与えない。

旧SWC1の第三の `R_ticket`、Final後の旧receipt lottery、署名乱数・証明subsetの差からの再抽選は、新FOW1へ採用しない。AN1互換のTicketはRFC-0004の固定式、FOW1のTicketはRFC-0004の固定式に従う。`sigma_P`、`sigma_S`、`sigma_R`、`sigma_A`、MODEL_AUDITの値、停止証明又は可変proof bytesを、その式の追加saltへ入れてはならない。

### 2.2 Panel抽選policyとの分離

TB1は乱数源を定義する。Panel人数、stake加重、復元/非復元抽選、同一operator除外、予備担当は、freezeした `DrawPolicyId` が一意に決める。添付原稿の「重複なし抽選」を、既存の復元抽選を許す全profileへ暗黙適用しない。非復元方式を選ぶ新profileには、その母集団・重み・停止・人数条件を明記する。m枠をm人の独立主体とみなさず、一人の能動的な正直Panelという前提を別に評価する。

### 2.3 識別子と非循環性

```text
RoundRef = {
  chain_id, suite_id, role_epoch, purpose, logical_round,
  roster_root, schedule_id
}
VssSession = { RoundRef, dealer_slot }
Qualification = {
  RoundRef, ordered_dealer_slots, public_transcript_root,
  ready_records_root, resources_ref
}
FreezeRecord = {
  RoundRef, qual_root, stable_anchor, consumer_set_root,
  policy_roots, original_timing_plan
}
```

`consumer_set_root`は用途ごとに、登録集合、Original集合、credit集合、roster候補集合等を指す。poolや重み等の全policy rootを含め、別用途の同形recordへのすり替えを拒否する。Cには先に決まるRoundRefだけを入れ、Cをleafに含む閉鎖集合Root、QualRoot、将来seed、署名、slot結果を同じCへ戻す自己参照を作らない。

一つのRoundRefを一つの固定用途・一つのFreezeRecordに割り当てる。同じ秘密を複数のconsumer batchへ後付けしない。既知seedを見て新しいpool・target・候補を作る経路は拒否する。

## 3. suiteとcanonical値

| 要素 | 採択した設計要件 |
| --- | --- |
| 有限体 | `p=2^521−1`。66byte big-endian、`0 <= value < p` |
| 座標 | roster slotに対応する相異なる非0 field値。indexと鍵をnativeが固定 |
| 秘密・係数 | 理想モデルでは独立一様。実装では適切なentropyでseedしたCSPRNGとrejection sampling |
| VSS | Asharov–Lindell ECCC TR11-036 revision5 §5 Protocol5.6の完全手順 |
| 認証 | ML-DSA-65。鍵登録・所有証明・context・正規符号化を検査 |
| 私信 | ML-KEM-768、認証・鍵確認・用途別sessionを組み合わせたchannelとAES-256-GCM |
| context/commitment hash | SHA3-512、64byte |
| seed | SHAKE256、32byte |

ML-KEM単独を認証channel、ML-DSA単独をVSSとして扱わない。KDF、認証handshake、鍵確認、AEAD nonce/counterの永続化と再起動、送受信方向・sequence、標準訂正、wire vectors、資源上限を固定し実装検証するまで有効化しない。rejection samplingは剰余偏りを除くが低entropyを高entropyへ変えない。

TB1の64byte context digest、32byte seed、既存64byte native beaconは異なる型である。旧値の先頭32byteへの切詰め、暗黙padding、旧codecへの詰替えを禁止する。既存attempt IDの幅は当該実行profileに従い、新しいBeaconRefを含む正規codecとして版付けする。

固定長unsigned integer、長さ付きbyte列、tag付きrecord、規定順序vectorを使う。float、boolの整数扱い、数値文字列、重複/逆順index、範囲外値、末尾byte、無制限vectorを拒否する。`authenticated=true`、`stable=true`や渡されたroot属性を根拠にせず、実bytesとnative記録から検査する。

## 4. membership、ROLE、bootstrap

役務leaseにowner、担保lot、役務期限、容量、ML-DSA鍵、用途別通信鍵、所有証明を束縛する。同じ担保lotを複数ticketへ使用しない。アドレス分割が実主体の分散を証明するとは扱わない。PALW実績や担保量からf上限が自動的に証明されたとはしない。

新profileの次roster選出は、前epoch保持者が準備した**独立ROLE round**を使用する。次候補ticket集合・全順位規則・n/f・次suite/scheduleを、そのROLEの最初の正直share公開前にfreezeする。canonical順の候補とSHAKE streamから、固定policyに従う偏りを避けた抽選を行う。担保lot二重使用を拒否するが、実運営主体の集中は別に評価する。

添付TB1で許容した「前epoch PANEL値から次rosterを派生する方式」は、対象集合を同じ公開前に閉じた場合だけ成立する設計だった。本統合では用途と公開gateの混線を避けるため独立ROLEへ置換する。公開済みPANELからROLEの未来未知性を作るfallbackはない。この追加sessionのVSS・通信・保持・復元・容量・合成を評価する。

初回rosterは、明示したnative bootstrap設定で固定する。その集合の故障上限を外部前提として開示し、恒久的な特権validatorへ昇格させない。初回rosterを自身の未生成ROLEで選ばず、genesis定数をEXEC用の予測不能seedにしない。起動構成・fault仮定が未確定なら新TB1受理は無効のままとする。

辞退・不参加を見て別seedへ再選別しない。予備slotを設ける場合は全順位・義務・選択条件を同じfreezeに含め、別profileとして評価する。準備不成立は対応epochの新規受付停止へ接続する。未公開秘密を新rosterへ再共有して延命する方式は採らず、新roundに新秘密を準備する。

## 5. 完全VSSの準備

各dealerは各purpose/roundについて独立した秘密 `z_d` と次数f以下の多項式 `P_d` を用い、`P_d(0)=z_d`として完全BGW VSSを実施する。悪意dealerの乱数まで一様独立とは仮定しない。

全messageをRoundRef、dealer、sender、recipient、phase、sequenceへ束縛する。配布、cross-check、complaint、原典が定める公開・訂正、整合判定、outputを省略してはならない。Shamir＋暗号化＋多数ACKへ置換しない。private資料の欠落と値不一致を区別し、欠落を0で埋めたり、正直者の全row/columnを便宜公開して回復したりしない。

原典の同期private channel/broadcastをnative phaseへどう写すかを明示する。署名だけで一貫したbroadcastを実装したとみなさず、正直者が同じphase受付集合・期限・終了記録を検査できることを要求する。非canonical/過大record拒否を原典のどのinvalid/missing分岐へ写すかも検証する。任意timeoutを挿入した版へ原典のprivacy定理を自動適用しない。

VSS終了時、正直holderはRoundRef、dealer、VSS終了までの公開transcript root、終了phase anchorに束縛したReadyへ署名する。そのrootへReady自身、QualRoot、FreezeRecordを含めない。q件の同一context Readyと規定のVSS終了判定を満たしたdealerをqualifiedとする。Ready多数は、Nodeが全私信の正しさを直接証明したという意味ではない。

準備期限までにqualifiedとなった**全dealer**を固定slot順でQへ含める。都合のよいsubsetを選ばない。`|Q| >= q`、Q・資料の可用性・全容量の固定を満たして初めてPREPAREDとなる。FOW1のユーザー仕事はEXEC/PANEL双方のPREPAREDと予約完了後に受理する。SCHEDULE/ROLE/ADMISSION/MODEL_AUDITは、それぞれのconsumer契約に必要なroundが準備できるまで当該用途を受理しない。任意評価はMODEL_AUDITと結果提出後のPANELの双方の計画・容量を事前予約する。

## 6. Release gateと時間順序

### 6.1 EXECからPANELまで

| 順序 | nativeで確定する事実 |
| --- | --- |
| A | EXEC/PANEL双方のPREPARED、roster/Q、TimingPlan、容量と担保 |
| B | 完全Cと全nonce候補、model/job/profile、target、会計、原期限、両RoundRef、登録集合を閉鎖 |
| C | Bのanchorの所定native安定性。初めてEXECの正直share放出を許す |
| D | EXEC certificateを検査しsigma_Eを受理。Minerが規定実行と署名済みOriginalを生成 |
| E | Original/ready集合、元claimの予約・受理集合、pool、重み、DrawPolicy、担当義務・原期限を閉鎖 |
| F | Eのanchorの所定native安定性。初めてPANELの正直share放出を許す |
| G | PANEL certificateからsigma_Pを受理し、固定対象についてPanelを選出 |
| H | 完全再実行、資料case、Court、成功又は未成功終端 |

登録・Originalが0件でも、所定のcanonical空集合と原scheduleを使う。空batchを理由に予定された公開を恣意的に省略しない。独立ROLEの進行を仕事件数へ依存させない。前段やnativeが不成立なら成功を捏造せず、元の未成功終端と明示回復へ進む。

### 6.2 SCHEDULE、ROLE、ADMISSION

- **SCHEDULE:** 未失効・未消費・未予約の適格credit集合、予約ID、割当policy、未来slot範囲を閉じ、安定化後にsigma_Sを公開する。割当を見て集合・区間を選び直さない。現在生成中のscheduleのpermitを、そのseedを運ぶcarrierの前提にしない。
- **ROLE:** §4の次roster候補とpolicyを、前epochの独立ROLE公開より前に固定・安定化する。次rosterが準備するEXEC/PANELは選出後の別sessionである。
- **ADMISSION:** seed順位を使用する別control profileについて、対象候補集合、target/D、順位policy、容量と原期限を閉じ、安定化後にsigma_Aを公開する。登録前か実行後かというstageをTimingPlanで固定し、未来の未生成Rootを仮置きしない。FOW1の既存 `(ticket,slot_id)` 順へは追加しない。

各用途の依存DAGをTimingPlanに固定する。未来のconsumer、未発行credit、まだ決まらないroster、自己生成中のseedへ循環依存する計画は拒否する。purposeを増やしたこと自体を容量確保やliveな実装と扱わない。

任意評価のMODEL_AUDITは候補・suiteのfreeze後に独立して公開し、その結果で非報酬testを実行する。評価提出物のfreeze後に別PANELを公開する。評価が自分の採掘権利又はその権利で生成するblockに依存しないcontrol経路を使う。停止・資料・反証の処理にはこの新しいbeacon待機を追加しない。

### 6.3 共通の予測不能性境界

**最初の正直share放出が境界**である。公式発表・certificate完成・Nodeの受理時刻ではない。holderは自分で認証したnative状態からfreezeと安定性を検査し、外部boolean、mempool受付、未安定block、ローカル時計を代用にしない。

Original受付期限は、EXEC certificateの正直者への最遅許容配信位置に、実行・送信・包含の予約時間を加えた位置より後に置く。PANEL releaseはOriginal閉鎖anchorの安定化後とする。公開開始後の先行受信・計算開始差や選出後DoSを、この乱数だけで排除したと説明しない。native停止時の壁時計経過を成功や確実な返金と同一視しない。

## 7. 公開shareとcertificate

```text
ShareRelease = {
  RoundRef, qual_root, freeze_id, holder_slot,
  [(dealer_slot, share_value) for dealer_slot in Q],
  owner_signature
}
DealerCertificate = {
  dealer_slot, coefficients[0..f], supporting_share_release_refs
}
BeaconCertificate = {
  RoundRef, qual_root, freeze_id, certificates_for_every_dealer_in_Q
}
```

holderは自分が保持するshare値へ署名する。最初に提示された候補多項式へ投票を固定する方式を採らない。誰でも公開資料からcertificateを組み立てられる。署名bytesを乱数出力へ含めない。

Nodeは各dealerについて次の全条件を直接検査する。

1. 認証native記録から解決したRoundRef、Q、FreezeRecord、release gate、原deadlineとの一致。
2. roster内の相異なるq人以上の有効署名、固定されたindex、同一context。
3. 正規なf+1係数、次数f以下、全支持点が同じ多項式に乗ること。
4. 固定Qの全dealerを一回ずつ含むこと。
5. 各資源上限、資料可用性、固定consumer、使用状態、同値再送との整合。

復元は誤り・欠落を扱うReed–Solomon等を用い、最初のt点を無条件に正解としない。Nodeには候補多項式とq点を渡せばよく、全subset探索を必須にしない。certificateは全私信の検証証明でもLLM計算のvalidity proofでもない。

## 8. 一意な出力

全dealerの `z_d=P_d(0)` を固定順に含める。

```text
sigma = SHAKE256(
  Enc("PALW-TB1/beacon", RoundRef, QualRoot, FreezeId,
      [(dealer_slot, z_d) for dealer_slot in Q]),
  32
)
```

署名、提出者、support subset、到着順、transport nonce、コメント、可変metadataを含めない。無応答dealerの除外、途中の部分Qによる出力、別roundへの付替えは禁止する。同じ正しい秘密列を別の証明で搬送しても同じseedになる。

EXEC/PANEL/SCHEDULE/ROLE/ADMISSIONの秘密材料を共有しない。同じseedからの公開後のdomain派生は、§2で認めた固定consumerの処理に限り、独立した未来乱数とは表示しない。hashだけで漏洩、適応攻撃、選択的停止が解消したとはしない。

## 9. 失敗・reorg・一回性

通常状態は `PLANNED → PREPARING → PREPARED → FROZEN → RELEASING → READY → CONSUMED`。失敗は `PREPARE_FAILED`、`BEACON_UNAVAILABLE`、`EXPOSED_ABORTED` の未成功終端とする。論理状態名であり、native enum値を無断割当しない。

定数、block/header hash、旧work-fold、旧seal-reveal、BLS、別suite/key/roundへの自動fallbackはない。同じslot/attemptを新seedへ無料移送しない。明示回復では新材料、新round、新登録、元の資金・責任の決済を区別し、旧claimを復活させない。

公開済み秘密はDB undoで未公開にならない。holderは曝露したRoundRef、QualRoot、FreezeId、share contextを耐久journalへ記録し、再起動/reorg後も別freezeへ公開しない。深いreorgでfreezeの安定性が破れた未完了attemptは停止し、既知seedを別対象へ使わない。cold importだけで非canonical fork上の秘密漏洩が存在しないと証明できたとはしない。復旧の停止範囲・曝露記録伝播・再開native条件が未設定なら再開を無効とする。

batch共有は固定consumer/slot単位で使用を記録する。最初のclaimがseedを使った時点でbatch全体を消費済みにしない。対象の全consumerが成功又は失敗で終端し、所定の保持状態へ移った後にroundを閉じる。消費対象の後付けは不可。

不正shareの処分は同一context/indexへの二重署名、又は成立したq点certificateの多項式と署名付きshareの不一致等の公開証拠に限定する。最初のt点や未成立候補多項式との不一致だけでslashしない。私的早期漏洩を常に検知できるとはせず、金銭罰を秘匿性の代替にしない。

## 10. control容量と既受理救済

VSS私信・公開資料・cross-check・complaint・訂正・Ready・release・certificate・終端・保持・journalの全工程を事前予約する。全用途を同時に動かす場合は合算し、EXEC/PANEL二用途だけの添付容量例をSCHEDULE/ROLE等を含む総量として流用しない。

新規TB1準備は既受理negative、資料case、Court、取消・終端の容量を消費しない。これらの救済に新beacon、新しいTicket、D、未発行creditを要求しない。control carrierは現在生成中のseedや、そのseedを必要とする未完了仕事から独立に成立しなければならない。予約recordがあることと、nativeの実際の進行・期限内包含は別の要件である。

多数claimでroundを共有してよいが、全対象とpolicyを公開前に固定する。署名一個のbeaconと同じ通信量とは扱わない。private/publicの増幅、最大complaint、再送、復元CPU、保存複製、cold検証、包含遅延を測定するまで本番throughputや1 BPS達成を主張しない。

## 11. 条件付き安全性と保証限界

### 11.1 公開値の一意性

完全VSSが全正直holderの点を一つの次数f以下の真の多項式Pへ揃えると仮定する。偽多項式P'はPと高々f点でしか一致しない。正直支持は高々f、悪意支持も高々fなので、合計2fでq=2f+1に届かない。

\[
P'\ne P\quad\Longrightarrow\quad\operatorname{support}(P')\le 2f<q.
\]

全Qの値とcanonical出力は一意になる。これは完全VSSと署名・fault仮定の下での論証であり、単なるcertificate集合交差の議論ではない。

### 11.2 freeze前の予測不能性

Qはq人以上、故障はf以下なので正直dealerを含む。その独立秘密がf share以下と公開準備viewから隠れ、正直releaseがfreeze後だけなら、予測利得をVSS、通信、認証、hash、native、roster、entropy、合成の仮定に結び付けられる。ただし、未実装suiteの全失敗を無根拠に小さなepsilonへまとめた完成定理とはしない。

限定した古典idealモデルで、一つの未知座標が体上で一様、独立random oracleへの異なる照会がQ_H回という条件なら、未知座標を含む完全入力への先行照会項をQ_H/pで評価できる。実際のSHAKE256・PQ channel・nativeへ移すreduction、rushing、公開view、適応・量子・並行sessionを含む定量的評価は未完である。古典RO式をQROMへ流用しない。

### 11.3 公開拒否と活性

故障f以下でn−f正直holderのshareが元期限内に包含されれば、悪意dealer本人の協力なしに全Qを復元できる。固定Qを後から縮小して別の有効値を選ぶ自由は与えない。

native停止・検閲や閾値以上の共謀まで含め、公開された値だけを条件付けても無偏りであるとは主張しない。未成功終端は誤った成功を防ぐが、全選択的停止を暗号だけで除去しない。beaconの一意性は、LLM関数の出力非衝突性、GPU秒・電力・金銭費用、Panelの正直者存在を証明しない。

## 12. モデル採用との分離

モデル追加はBFT YES/NOだけである。MODEL_AUDITは評価を行う場合の非報酬用途であり、その実行・成功・timeoutを採用条件や自動採用の根拠にしない。MODEL_ADMISSION、MODEL_STOP、RETIRE、BurnResumeの署名権限とTB1 share公開を分離する。BFT採用票から新しい乱数を作らない。

## 13. 移行と検証

全consumerが同じcertificate、Qualification、FreezeRecord、roster、suite、purpose、原期限を確認する。既知seed・別round・旧beaconへfallbackしない。曝露journalを通常のDB undoで消さず、深いreorgや取得不能時は新規成功を保留する。公開済み秘密の未公開化をsnapshotだけで保証しない。

完全VSS、認証私信、悪意dealer、公開拒否、空batch、全用途の容量、native stable条件、再起動、曝露reorg、既受理Courtの継続を同じ構成で検証する。fixtureの補間成功を実ネットワークの全合成保証と扱わない。
