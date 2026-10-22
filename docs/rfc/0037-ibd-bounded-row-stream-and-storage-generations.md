# RFC-0037 — IBD の有界 authenticated row stream と世代番号付き保存

Status: **Proposed / Review Required** (2026-10-22)。[ADR-0204](../adr/0204-ibd-bounded-row-stream-and-storage-generations.md)。
G14 storage(pruned node ≤ 30 GB)の IBD lane。**consensus は変えない**(state root の形式・header・fork choice・抽選は不変)。
変わるのは P2P に見える転送形式だけで、新 message は登録しておらず休眠。採択・protocol version 割当・配備は未承認。
設計の詳細と現状分析は [ibd-generations.md](../design/palw/storage/ibd-generations.md)。

## 1. 問題

pruning point の PALW state は今、carriage 全体(encoded ≤ 384 MiB、decoded ≤ 512 MiB)を spill に受け取り、
全量をメモリに decode してから header の root と照合する。chunk hash は送信者が選んだ stream root への転送完全性で、
consensus root との照合は最後の 1 回だけ。偽の state は全量の受信・decode の後でしか分からず、
受信側は「どこまでが正しいか」を途中で知る手段がない。state が大きくなるほど、受信・展開の一時領域とメモリが
管理領域の上限(IBD spill 384 MiB × 2)を圧迫する。

## 2. 提案: preimage 先行・collection 単位で検証する row stream

root は「各 collection の 64 byte root を並べた preimage の hash」なので、root の形式を変えずに:

1. 送信者は最初に preimage(数 KiB)を送る。受信者は witness header(pruning point の selected-chain 子)の
   `palw_state_root` と照合する。**row を受け取る前に**不一致なら切断・peer 責。
2. 続いて collection ごとに `CollectionStart{label, row_count, preimage_offset}`、`Row{key, value}`×n、
   `CollectionEnd`。受信者は row を collection root の hash に逐次流しつつ capped spill へ書き、
   `CollectionEnd` で preimage[offset..offset+64] と照合。不一致はその collection で停止・peer 責。
3. offset は範囲内・重複なし、label 重複なし、宣言行数の過不足は peer 責。
4. root が record の projection にかかる collection(`gen_classes`、`tir_classes`)は全 record を送り、受信者が
   decode → projection を hash する(collection ごとの codec 表が必要。§5)。非標準 root の collection は既存の
   light-client 関数(`PalwCollectionOpeningV1::root`)で照合する。
5. 全 collection の後、受信者は spill から typed carriage を組み立て、**既存の import ゲート**
   (`restore_native_carriage_v1(expected root)` = carriage 検査・index 再構築・`assert_internal_consistency*`・全 root
   再計算)を通す。名指しされなかった collection はここで落ちる。この段を通過して初めて「検証済み」。

受信側の検証器は `consensus/core/src/palw_row_stream_v1.rs` に実装済み(wire には未接続)。

### 2.1 何を保証し、何を保証しないか

- 保証: 受け入れた collection は header が commit した state のそれである。偽 row は当該 collection の終わりで検出され、
  何も昇格しない。spill は 1 file ≤ cap、同時 2 file。
- 保証しない: stream 単体の完全性(全 collection を名指ししたか)と全体整合性。これは第 2 段(既存ゲート)が担う。
  **root、または root に一致する stream を受け取っただけでは検証済みにしない。**
- 計算の正しさの暗号学的証明は使わない(PRINCIPLES §8)。使うのはハッシュによるデータ同一性だけ。

### 2.2 失敗の帰責

| 事象 | 帰責 |
| --- | --- |
| preimage ≠ root、collection 不一致、行数・offset・label の違反、projection の decode 失敗 | peer(切断・別 peer で再試行) |
| spill cap・byte 予算・容量予約の拒否、local I/O | **local**(停止・再試行)。peer を罰しない、block を無効にしない |
| 供給者がいない | sync の停止。「取得できない」≠「存在しない」≠「無効」 |

## 3. 互換性と休眠

- 新しい request/response(例: `RequestPruningPointPalwRows` / `PruningPointPalwRows`)は**新 protocol version** の
  交渉でのみ使う。両端が新 version のときだけ送る。旧 peer には従来の 106/107 carriage 転送を使い続ける。
  旧 103 leader が未知 request で接続を切る問題(`request_pruning_point_palw_state` のコメント参照)を避けるため、
  version 交渉前に新 message を送らない。
- 本 RFC の時点では message を proto に追加していない。flag(node 設定)も既定 off とし、採択まで有効化しない。
- consensus fingerprint・params id に入る値は変えない。

## 4. 世代番号付き保存(node-local、参考)

受信した state は staging 世代に書き、第 2 段を通過した証拠からのみ `VERIFIED` marker を作り、journal の原子的
rename で active に切り替える。世代は同時に最大 2(active + previous か staging)。これは node-local で P2P にも
consensus にも見えないため本 RFC の決定対象ではないが、§2 の「何も昇格しない」の実体なので併記する
(`database/src/storage_generations/`、設計 §3.1)。

## 5. 未決事項

1. collection ごとの codec 表(label → key/value 型、projection 関数、非標準 root)。serve 側はこれに従って row を出す。
   表の網羅性は「全 root を再計算する第 2 段」が最終的に保証するが、表の欠落は IBD 不能として現れるので
   golden test で ADR-0043 §2 の一覧と突き合わせる。
2. 第 2 段のメモリ: phase 1 は現行同様 O(state)。phase 2 は state lane の authenticated map(別 RFC)を前提に、
   全体検査を世代 DB 上の bounded-memory 集計へ移す。各 `assert_*` の分類は設計 §3.2。
3. message の大きさ・row の chunking・再開(既存 106 の resume と同様に preimage + collection index で再開可能)。
4. protocol version 番号と、既存 106/107 からの移行期間。

## 6. 検証

- 実装済みテスト: `consensus/core/tests/ibd_row_stream_generations.rs`(stream root のバイト同一性、genesis preimage、
  途中不一致での停止と非昇格、poison、cap 超過、whole-state 拒否、検証〜切替間の crash、projection)。
- 採択後に必要: 実 PALW state の全 collection を serve → 受信 → 組立 → 既存ゲート、が現行 carriage import と同一の
  `PalwChainStateV2` を作ることの等価性テスト(t12 の pruning snapshot で)。
