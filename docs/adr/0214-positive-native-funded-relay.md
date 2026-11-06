> **採択記録 — 2026-11-03（Asia/Tokyo）、ユーザー:** 「両案を採択し、限定実装を進める」。
> RFC-0046 / ADR-0214を同時採択。通常script検査を維持する先行relay資料、有限custody、新op14及びcurrent cursorのterminal資料保存を限定実装する。部分精算は追加資料が揃う場合に限る。
> 本番有効化、公開値、旧原資の移行、RFC-0045又は全体目標の完成を承認するものではない。以下の「未採択」「案」「提案」「採択された場合」は採択前の原文表現として保持する。namespaceは実装時に別途collision照合する。

# ADR-0214 — Positive原資へ先行relay資料・custodyを追加する判断案

- 状態: **Accepted — 2026-11-03、ユーザーの明示採択。限定実装・検証は未完了、公開有効化は未承認。**
- 日付: 2026-11-03。
- 対の仕様案: [Relay RFC draft](../rfc/0046-positive-native-funded-relay.md)。
- 前提: [RFC-0045](../rfc/0045-positive-court-canonical-assignment.md)、[ADR-0213](../adr/0213-positive-court-canonical-assignment.md)、[PRINCIPLES §11](../PRINCIPLES.md#design-freeze)。
- 本案のop14・custody ownerは、採択済み8–13の通常実装修正ではない。新規RFC/ADR双方の明示採択を求める**別の凍結例外案**である。採択済みRFC45をこのdraftで改定しない。

## 1. 判断を必要とする具体的な欠損

現在のnative money検査は、payerのP2PKH inputについて通常ML-DSA署名を要求する。原資のcoin lock、due、保存則、onceが正しくても、payerが将来署名しなければ実払い・返金は実行できない。payerが都度署名する試験が通ったことは、payer独立のlivenessを示さない。

既存`NONE | ANY_ONE_CAN_PAY`署名なら、将来の出力とrelayのfee入力を固定せず、現在の原資input・amount/scriptとexact payloadを認証できる。ただし、署名はfuture funding outpointのtxidを含む。その署名をfunding carrier Fのpayloadへ入れると、F.txidが自分の将来署名へ依存する。

現txid定義、scriptのpush-only/cleanstack/固定長、PQ input class規則を維持したまま、Fとは別の先行Rで署名を正規保存する仕組みが必要である。現在のop8–13、aux151、local block allocation ticketはこのcustodyを提供していない。これは実装済みownerに適切なconstructorを足すだけの問題ではない。

対のRFC §2は、実signhash、txid、script、wallet、body codecとDB storeの確認箇所を列挙する。既存の署名方式をデータ認証に使う案であり、モデル計算の正しさを暗号学的に証明する方式を導入しない。

## 2. 提案する決定

### D1 — 通常P2PKH/scriptを維持し、限定relay署名だけを使う

最初のprofileは、既存transaction version 0、input 0のexact funding outpoint、sequence 0、sigop count 1、lock time/gas 0、既存lifecycle subnetwork、固定`op12{assignment, case:None, receipts:[]}`、hash type`0x82`に限定する。transaction署名contextは既存のものを使う。

nativeはoutpoint、amount/script、全固定field、payload全文を照合し、実通常scriptを必ず実行する。NONE署名がoutputsを署名しないことを任意送金権へ変えない。nativeが実receipt・P・duty・floor・署名済み契約・onceから全金額とrecipientを導出する。

`case:None/receipts:[]`は、carrierがCaseやdue subsetを選ばず、actual lock/account/AssignmentUseからnativeが完全に解決するtemplateとする。Pやserviceの成功省略を意味しない。未知template、任意payload、非empty receiptへの差替えは拒否する。通常payer署名付き既存op12のwire/意味は保持する。

### D2 — 新local op14で、未来UTXOとは別の有料metadataを受理する

object176内の新local op14と、purpose別の`FundingRelayTermsV2`を提案する。新purpose署名はnetwork、元契約scope、relay policy、actual native payer bond/key、future outpoint、expected amount/native P2PKH、固定template、target、expiry/custody、metadata fee capを認証する。別途、既存transaction contextで作ったrelay署名を保持する。

Rは実native keyと独立に宣言したexpected entryでrelay署名を検証する。このentryは署名計算用DATAであり、未来funding UTXOの存在・受理・lock・予約ではない。未来のbundle/consent/join digestを、未搬送preimageの検査成功として扱わない。

Rだけでassignment/job/Case、通常推論・sourceサービス、funding account、coin lock、role permission、Assignment nonce、credit、WorkerPayment、payoutを作らない。current native keyが存在することを計算又はPの承認にしない。

R自身のactual feeから有限metadata chargeを一度burnし、実custodyに必要なnative RAM/DB/replay容量を同じcheckpointで確保する案とする。A/Cの将来refund又は未存在原資から費用を作らず、架空のcustody報酬queueやmintを追加しない。Fが来なくても、実提供した有限metadataサービスの費用だけが成立する。

### D3 — FはRを参照せず、nativeがexact outpointで先行Rを解決する

順序は「Rに依存しないFを完成 → F.txid確定 → relay署名 → Rの受理・custody → Fの受付」とする。F payloadにR hash/txidを追加しない。初期profileは、F検査時のselected-parent canonical stateでRを読めることを要求する。

Fはactual own outputと現在UTXO、元の全署名・scope/caps・refund/worker、nonce/once、必要なactual capacityを再検査する。同時にexact funding outpoint＋purposeからRを解決し、canonical lineage、全資料、署名、target、実custody残期間を確認する。actual F entryでrelay署名を再検証してから原資とbindingを一緒に受理する。

A/C/op13の既存payer consentは変更・代用しない。特にCでは、outputを含む完成したr1 Result/Service/FundingConsentと既存二field-zero scopeをそのまま照合する。Rの追加purposeがr1の別解釈を作ることはない。

### D4 — current cursorのterminal署名を失う部分精算を認めない

部分精算Sが新cursorを作る場合、その具体的なS.txid/output index/remaining amount向けR2を、Sの受付前に同じcanonical lineageで受理・保持していなければならない。SはR2 hashを参照しない。

R2登録は現在の元account/predecessorへbindするmetadataである。未来Sの適格性、出力の存在、floor解放、義務終了又は新AcceptedOriginを先に保存しない。S受付で全native条件とR2を再検査し、UTXO・current cursor・R2 binding・debit/once/journalを一checkpointで進める。予算・期限・元account identityはresetしない。

R2がなければSのlocked-input spendを拒否し、旧cursor・そのR・残責任を維持する。ordinary spendの後でPALW metadataだけをskipする実装は認めない。Sの出力やfee入力を変えてtxidが変われば、対応するR2が再び必要である。

### D5 — terminal保証と条件付きpartial進行を分けて採択する

actual P/source/fraud/duty/capital/reservation/liability ownerが有限に終了し、全dueと返金を確定できるとき、現在cursorのRは一回全額精算を追加payer署名なしで搬送できる。continuationがないのでR2を必要としない。WorkerPayment、適格service、署名済みburn、正規refundを別判定し、実出力保存則とonceを要求する。

この保証にもrelay/inclusionと必要な実資料・custody・finite ownerの前提が残る。P Pendingから勝手にterminalへ進めたり、pending-fraudを短縮したり、時計だけでcapitalを返したりしない。

一方、途中の新cursor用R2はpayerがその具体形を認証して発行する必要がある。従って本案は**全ての安全partialがpayerなしで必ず進むという保証を与えない**。R2を得られるpartialはP未完でも実native条件により進められるが、R2非協力時には旧cursorでterminal fallbackを待つ。

RFC45の部分返金floor、P未完・未検証時のservice/refundという要件を消す決定ではない。terminalだけの成功を全partial要件の完了証拠にしない。この限界が目的に合わない場合は、別の明示設計を要する。

### D6 — 実custody・有限caps・原子的復旧を新しいownerとして要求する

完全な`FundingRelayPolicyV2`とfee/physical preimageをSome-only fingerprintとactual fired scopeへbindする。global/per-poster/per-prover pending数、同nonce/account候補、continuation数、stored rows、bytes/heap/DB/batch/replay/work、expiry/custody/rollback期間を全て正かつ有限にする。公開値はこの案で決めない。

actual native budget/fee owner、physical RAM lease、RocksDB hard quota、retention/replay/cleanup ownerが全支出を担う。R row又は整数capacityだけでcustody成功にしない。local capacity不足は再試行するresource errorであり、偽receiptやMiner有罪にしない。

Fは元契約の最遅P/source/fraud/返金/rollback boundがR custodyで覆われることを別途確認する。実P/fraud ownerが未完成なら、RがあってもF又は精算は受付拒否のままである。本案はそのblockerを新しい短縮P policyで迂回しない。

新登録・funding受付のexpiryだけで、既にboundのcurrent relay署名を失効・回収しない。完全なcurrent packageはFの実capital/physical quoteに含むnative live-account slotで保つ。slot自身がexact terms/template/expected entry、payer key、purpose署名とrelay署名の実byte、actual accepted bindingと再認証資料を保有し、historical raw journalがprunedでも実native readerが再生できることを要求する。hash参照や保存Passだけのslotではない。current cursor当たり一つのpackageと有限continuation/rollback枠を実予約し、R2移行時の旧packageも必要なrollback期間保持する。固定expiryでliveの署名byteを削除しない。finite windowを超えるcensorshipまで無条件に保証したとはせず、前提外でも資金解放・偽receipt・安全性を壊す資料回収で進行を装わない。このactual保持ownerを作れないprofileは受付しない。

### D7 — append-onlyの新schemaとraw DB namespaceを提案する

対のRFCはaux151 kind19 `FundingRelayAuthorization`、kind20 `FundingRelayCustody`、同storeのraw material suffix`0x4c`を候補とする。exact funding outpoint＋purposeをkeyにし、元Assignment/contract、native payer key・一つずつの署名、actual R origin/custodyと実F/S bindingを固定rowで保存する。

keyは74 bytes案で142-byte ceiling内とし、各rowは16,384-byte ceiling内のexactサイズを検査する。materialを無制限Vecでrowへ詰めない。既存0–18、旧0–13、r1 ID/domain、0x49/0x4a/0x4b用途を変更しない。これらの番号・domain・suffixは**未割当**であり、全branchを含む中央collision review後にのみ登録する。

row19/20とraw bytes、fee、pending count、bindingを同じUTXO/PALW WriteBatchでcommitする。canonical rootのprivate readerが全byte・署名・origin・scope・履歴を有限予約下で再生する。orphan raw bytesや保存rowのboolだけで成功を復元しない。

reorgはR受理、F/S binding、funding cursor、nonce/Use/role/Case、資源、once/payment、journalとUTXOを同時にundoする。old/current Rを必要なrollback horizonまで保持する。全DB接続を閉じたrestart、snapshot/pruned IBD、資料回収・破損・不足を検証する。資料不足だけでcoin lockや責任を解放しない。

### D8 — 元Assignment intakeのprofileを固定し、旧契約へ遡及しない

`CooperativeV1 / RequiresR_V2`は、元のAssignment intake時のactual fired scopeと認証済みpolicy/materialで一度だけ選ぶ。後続C、全prover join、continuation、取消・期限切れ・精算まで固定し、後日のcurrent Paramsやrelay activationで再選択しない。元の選択資料が失われた場合はUnavailableとし、都合のよいprofileへfallbackしない。

既にacceptedの旧Assignmentとその後続C/join/accountは旧cooperative profileのままであり、payer署名依存と実liveness限界を明示する。後日のrelay activationを理由にRを遡及必須化したり、旧r1 consent・元署名をRの新purpose認証として再解釈したりしない。元RequiresR契約も後日forkで条件を外さず、その満期・責任終了まで元codec/preimage・必要継続サービスを維持する。変更には別の明示的なfork移行規則を要する。

旧資金の任意opt-in／移行は本案で定義・承認しない。必要な合意、追加費用・資源、既存支払/返金権の保存を含む別判断とする。公開presetのNone維持は方針であり、公開網のlive Assignment不在を本draftが実査したとは主張しない。全presetと対象網のactual stateを判断前に確認し、契約不在という未確認の仮定で遡及適用を正当化しない。

## 3. 凍結例外の正確な範囲

[PRINCIPLES §11](../PRINCIPLES.md#design-freeze)は次を定めている。

> 重大な問題が確認された場合は、新規 RFC と新規 ADR の両方、レビュー、明示的採択によって変更できる。

本案が求める例外は、新op14、relay purpose署名・template、先行metadata admission/custody owner、row19/20・DB material、RをF受付の必須前提とする規則、R2とterminal fallbackの新lifecycle、対応fee/resource/undo/restore契約である。これは採択済みRFC45の意味不変な実装修正ではない。

変更しない対象は、通常script/txid/sighash、r1 projection/ID/署名、元の資金caps/期限/nonce/once、全モデルのclaim検証とGate C、モデル二階層、旧TFO1/KernelJob/Native39、primary発行・weightである。Rはモデルの公開性や品質を審査する仕組みではない。

求める採択は**このRFC案とADR案の両方を採択して限定範囲の実装を進めること**である。数値、実測、独立review、全consumer完成、既存契約移行、公開activationを一括承認するものではない。両案の明示採択前にlive成功経路へ昇格させない。

## 4. 比較した代替

| 案 | 効果と未解決点 |
| --- | --- |
| cooperative payerのみ | 現在の会計安全性を検証できるが、payer停止後の支払・refund livenessは完成しない |
| terminal-only、partial廃止 | 事前署名は単純になるが、RFC45の途中service/refund要件を満たしたと扱えない |
| Fへの自己署名attachment | payloadを含むtxidと循環する |
| 新script／署名省略 | 通常P2PKH/script維持の契約を外すため本案では不採用 |
| local sidecar又はdetached block attachment | 前者はcanonical custodyにならず、後者は別wire/owner契約を必要とする |
| 本案の先行R/R2 | 既存署名・原資を維持できるが、先行登録のlatency・料金・DB/復旧負担と条件付きpartialを追加する |
| 現在の拒否状態を維持 | 新しい契約を採択せず安全境界を保つ。RFC45 liveness完了とは宣言しない |

## 5. 結果・検証・有効化

採択された場合でも、実装はsigner、bounded decoder、actual fee allocation/custody、共通block/mempool/template qualifier、F/R照合、R2 guard、実settlement/once、原子的DB、actual history reader/cleanupを接続して初めて成立する。pure hash一致、prepared signature、typed missingの拒否は成功証拠ではない。

対のRFC §12の試験を必須とする。特に、通常Full scriptを実行したrelay、F/R非循環、R単独の無効果、actual UTXO/caps/refund、P Pending/失敗branch、R2不足時の旧cursor保全、追加payer署名なしのterminal実送金、全DB閉鎖restart、reorg/pruning/IBDとNone互換性を、実native ownerで確認する。

公開ParamsはNoneを維持する。新owner、finite P/fraud bound、全branch費用・実容量、測定・review、対応全consumer auditが未完なら公開activationは拒否する。このADR案の作成を採択又はRFC45完成として報告しない。
