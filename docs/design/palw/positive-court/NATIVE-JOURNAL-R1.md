# Positive native case journal r1 — A0接続契約

2026-10-31。RFC-0044 / ADR-0212の実装用契約。A6が実装する新しいnative履歴のidentityと再生境界を固定する。旧private archiveのcodec・head・初期record数を再利用しない。以下はwire/state owner間の契約であり、実DB・native履歴reader・再生・有効化の成功報告ではない。

## 正規eventとidentity

shared leafの新journal codecは、次の順序を持つ有界recordとする。

| field | 内容 |
| --- | --- |
| version / revision | u16 LEの2 / 1 |
| scope / case / result | 既存ChainScope / CaseId / ResultId |
| sequence / previous | u64 LE。初期sequence=1、previous=ZERO。後続は直前head |
| origin | 実accepted-prefix由来のAcceptedOrigin。peer値だけで受理を認めない |
| event kind | local u8。0はPositiveNativeBodyV2の完全canonical bytes、1は既存native Transactionの完全canonical bytes |
| payload | u32 LE長とbytes。kind0の操作番号0..7は既存bodyをそのまま使用 |
| auxiliary preimages | 有限個の正規参照とpreimage。exact referenced materialだけをnative ownerが選び、共有codecで順序・重複・長さを検査 |

headはcanonical full recordをkey `misaka/positive/case-journal-event/v2/r1` のBLAKE2b-512でhashする。`JournalHeadRow.history.digest`は**同じrecord**を別key `misaka/positive/case-journal-history/v2/r1` でhashする。両者は別identityであり、TxID・旧archive headとaliasしない。history.originはrecord.origin。recordに自分自身のheadやpost-state rootを含めず、previousだけを含める。

auxiliary entryは固定AcceptedMaterial（280 bytes）、u32 LE長、canonical preimage bytesの順とする。参照のBorsh 280 bytesのstrict lex順で並べ、同一参照の重複を拒否する。操作が実際に参照したpreimageだけを含め、未参照のnonceを認めない。native ownerが導出した期待参照一覧との完全一致と、typeごとのidentity・origin・canonical性を独立に照合する。shared codecのsyntax成功を授権にしない。payload/auxの件数・個別byte数・総byte数・decoded heap・hash/再生workを有限policyで先に制限し、checked arithmetic・exact EOFを検査してから所有bufferを確保する。

## inclusionと再送

case journalは、native accepted-prefix foldが受理した**Case状態またはCase費用を変えるevent**の完全な列である。正規化されたResult受付が最初のeventになる。

- 異なるauthored retry・検査失敗がCase費用やcandidateを変えた場合、失敗したraw bodyも保持する。`Err(Check)`だけを見て更新をrollbackしたり、履歴から除外したりしない。
- 現在の権限と署名を再確認した既知ExactFrame、同じcanonical RequestIdのSourceRequest、既受理UnitKeyに対する認証済みSourceResponseが完全なidempotent no-opなら、Case費用・history長・head・sequenceを増やさない。author・署名・carrierだけの違いを新しい資料義務にしない。responseのopening表現が異なっても、同じsource/unitへの認証が成立すればfirst receipt/provider/feeを保持する。同originの再配送も再適用しない。
- 将来位置のframeなど、現在のrelation受付条件を満たさない搬送はCase課金前に拒否する。コピーだけでMinerのCase予算や保存上限を消耗させない。通常TXの受理・fee・一般資源制限はnative層に残る。
- 独立に認可された新しい有料操作は別である。carrier TxIDの変更だけを新しい費用同意にしない。全Panelの資料dedupe・料金dedupeを維持する。

通常のaccepted TX列と、nativeに消費されたCase event列は異なる。欠落body/indexをemptyとしてskipするreaderから完全なCase履歴を作らない。完全性は実native event fold・保護された期待head・保持preimageから確認する。

## 精算・期限・復旧

kind1はA5のqualified settlementを再確認するための実取引資料である。保存したpaid flag・proposal・receiptだけで資格を再生成しない。実UTXO、入力授権、現在lock、正規料金・recipient、既精算marker、continuation lineageをnative ownerが確認する。source feeの一回性、Court料金のsettlementをまたぐ一回性、fee/capital/surplus保存を同じ更新に含める。

合成zero-TxIDのclock originを作らない。現在のnative時計による期限判定、既存body6のExpire、実精算carrierを区別する。期限を過ぎた未検証Caseは保存状態が古くてもcreditを得ない。Panel未完了をMiner資料不提出へ変換しない。

restartではraw eventを独立に再生してlocal checked factを作る。その後、現在のrooted Case/history/funding/resourceを確認するreadonly復旧portから、既にcommit済みのopaque admission receiptへ再bindする。live admission・prepare・funding commitを再呼出しせず、saved Passedからreceiptを作らない。

aux151のCourt/Source/Funding/JournalHeadは既存KernelRouteRowのold/new deltaを用い、実UTXO・acceptanceと既存per-block caller WriteBatchで確定する。追加のtyped material-storeもcaller-owned batchへstageし、DB commit前にcache・terminalを公開しない。A2のlocal spent I/O storeは別寿命であり、このchain undoへ含めない。

実装と検証の状態は [A0 checkpoint](../../../../handoff/positive-court/A0-integration-status.md) を参照する。
