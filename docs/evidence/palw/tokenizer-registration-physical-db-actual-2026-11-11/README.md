# Actual tokenizer RocksDB storage and GC

2026-11-11。`9d9f88348` の隔離ノードに、実 tokenizer **2,104,556 bytes / 33 Part**を通常 UTXO で署名・送信する fixture が通過した。測定用 class は shipped SmolLM2 1.7B program/layout を用い、raw/table の同一性を検査した。公開対象 class の登録・TFP1 の受付ではない。

| 観測点 | 登録 DB の全ファイル長 | 空取引 DB の全ファイル長 |
| --- | ---: | ---: |
| 共通の前提状態、flush 後 | 4,725,482 | 4,725,475 |
| Finish、flush 後 | 22,695,446 | 5,418,212 |
| raw GC、compaction 後 | 14,074,322 | 6,388,433 |
| DB close/reopen 後 | 14,109,067 | 6,427,373 |

DAA 185 の GC で33 Part・session・予約 book が消え、検査済み登録は残った。元の35 source body と35 fold journal の保存サイズは GC・compaction・reopen 後も一致した。GC 自身の undo/redo journal は2,281,877 bytes。raw GC は通常履歴の削除ではない。

測定途中の最大観測ファイル長は42,710,900 bytes、WALは33,149,026 bytesだった。サンプル間の瞬間最大値ではない。macOS opt-level 0、試験全体43.88秒、process peak RSS 340,901,888 bytes。OS cache は落としておらず、最低 Linux 機材の性能や最大同時負荷への適格性を示さない。測定用の登録 charge 589,719,313 sompi を公開料金に採用していない。

[source・入力 hash・全観測](summary.json)と[実行ログ](measurement.log)を保存する。64 KiB合成入力の[先行試験](../tokenizer-registration-physical-db-functional-2026-11-11.json)は別記録として保持する。
