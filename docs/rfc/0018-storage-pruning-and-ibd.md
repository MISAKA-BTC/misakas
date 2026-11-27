# RFC-0018 — 状態保存・pruning・IBD

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**DESIGN_SPECIFICATION**。root不変のrow streamと保持規則を統合する。新authenticated root形式は提案のまま分離する。
- 判断理由：[ADR-0018](../adr/0018-bounded-storage-pruning-and-ibd.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. 保存するものと削除可能なもの

保存単位は台帳の正規状態、未消費権利、未解決責任、元claim/署名、必要資料、認証・停止・決済根拠、reorg範囲、曝露journalへ分ける。経過時間やdisk不足だけを根拠に検証に必要な状態を削除しない。

node-local cache、再取得可能な派生pack、終端済み資料、古いsnapshotは用途・参照・保持期限がない場合に削除できる。文書整理で旧RFCを削除することと、稼働台帳の履歴・責任・署名を削除することは別である。

## 2. pruning不変条件

hot state、cold state、snapshot、IBD staging、Court body、trace、delta、署名・曝露journalのlive setを明示する。claimがFinalでも公開異議期間、支払い、係争、依存、保持が終わるまでpinを外さない。

prune許可は認証されたfrontier、元責任終端、material_until、reorg保持、他recordからの参照で決める。未解決liabilityを保存先移動で消さない。pruned Nodeも同じconsensus判定を再現する。

## 3. 認証付き行アクセス【提案】

全表連結hashを置き換えるauthenticated state mapは独立したroot形式の提案とする。採用・移行前のrootを黙って変更しない。

提案する二分Patricia Merkle trieでは、生key byteを`1 + 8bit`、最後を`0`とするprefix-free・順序保存符号を使用する。leaf/branch/empty/tableごとにdomain分離し、table ID、schema、key/value上界、row countをcommitする。

存在・不在・範囲完全性・更新を、pinしたparent root/branch/table/schemaに対して検査する。範囲と交わる部分木をOpaqueで隠す証拠は不完全として拒否する。更新前の値を認証してから差分を適用する。

`Present`、`Absent`、`Unavailable(reason)`、`NotCovered`を分ける。取れない行を不在とみなさない。cacheは検証済みの存在/不在だけを保持し、取得失敗を不在cacheへ入れない。追い出しで回答の意味を変えない。

適格集合の抽選・全体上限・最大値等を行取得へ変えるときは、必要な完全性と結果同値性を別途確認する。勝者一行だけのproofで、より良い候補がなかったことを証明したとしない。

## 4. bounded row stream【採択済み設計】

既存root形式を保つIBDでは、最初にcollection rootのpreimageをwitness headerへ照合し、その後collectionごとにrowを有界spillへ流す。label、offset、row count、順序、projection、集合漏れを検査する。

collection終端の一致だけでは完了にせず、最後に全体importの内部整合性と全rootを再計算する。検査完了前の途中結果をactive stateへ昇格しない。

送信者のroot不一致・不正framingと、受信者のdisk/RAM予算拒否を区別する。前者はpeer不正、後者はlocal復旧であり、block無効判定にしない。新wireはprotocol交渉後だけ使用し、旧peerには対応形式を用いる。

## 5. 世代管理とcheckpoint

active、previous又はstagingの有限世代で保存する。完全検査後のVERIFIED markerと原子的切替journalを使い、途中crashで未検証世代へ起動しない。独立backup数は保存policyで上限を設ける。

snapshotにgenesis、network、合意設定digest、binary/schema、選択tip、DAA、PALW root、file checksum、pin集合を記録する。複製を作る前に実空きと共有blockを評価し、無制限な全台帳コピーを作らない。

## 6. 圧縮とGC

キー・値・canonical意味を変えない圧縮を許す。再open後の値、delta undo/redo、rootで同値を確認する。journalを毎block全件複製せず、必要な差分・世代・参照を保持する。

GCは候補mark、pin/依存再確認、削除、完了recordの順で進める。実行中reader、未完了write、reorg、曝露、係争のpinがある資料を消さない。必要資料を保持すると上限を超える場合は新規受付を制限し、必要状態を削って成功を捏造しない。

pruned Nodeの30GB等の資源目標は、対象状態量・保持期間・負荷・IBD一時量を指定して評価する。無限の未解決責任や無制限資料がある環境でも定数diskで動くとは主張しない。

## 7. 削除してはいけないjournal

TB1の公開済み秘密、署名済みVoteKey/PermitId、二重署名防止、元責任は通常のchain undoと寿命が異なる。rootに戻せたことだけを理由にこれらを未使用へ戻さない。同期資料から過去の秘密漏洩がなかったことを推定しない。

## 8. 受入条件

全保持/剪定ありの判定一致、cold IBD、偽row、不完全範囲、未検証staging、import途中crash、容量拒否、snapshot復元、live pin競合、未解決case、受賞・市場deltaのundo/redoを確認する。

diskは論理サイズと物理使用量、1block/1claim当たり増分、compaction時peak、snapshot共有、回収量を別に測る。圧縮率だけでpruning完了とは扱わない。

## 9. UTXO commitmentとPALW状態root

基盤UTXO commitmentの`LtHash32_1024` profileは、1024本の32-bit lane、4096-byte状態を用いる。canonical UTXO要素はoutpoint、金額、script、受理DAA、coinbase属性を含め、規定のBLAKE3 XOFからlane値を導く。加減算はlaneごとの規定moduloで行い、最終commitmentは規定のBLAKE2b-512で64 bytesへ固定する。旧16-bit laneの仕様を新profileの計算へ混ぜない。

このUTXO multiset commitment、PALW table root、headerのPALW state root、§3の提案authenticated mapは別形式である。幅が同じでも互換とみなさず、schema・domain・root種別を検査する。pruned/archival/IBDで同じUTXO集合とPALW状態を復元し、それぞれの正規rootを照合する。

保存形式の圧縮やrow streamはcanonical状態を変更しない。新rootへの移行には別の採用・発効・同期互換設計を要し、旧rootを新算法で再計算して既存headerの意味を変えない。
