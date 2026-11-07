# t12 の DAA の進みとブロックの bytes(受動的な観測、2026-10-19)

- **日時:** 2026-10-19 07:14〜07:34 (CEST、ホスト時刻) / unix 1792386847–1792388047、1,200 秒。
- **対象:** 稼働中の testnet-12。観測点は misaka-ibm(vmi3450148)の node、wRPC JSON 127.0.0.1:26314。release 0b1c11b87906 の fleet。
- **方法:** 読み取りだけ。1 秒ごとに `getBlockDagInfo`、sink が変わるたびに `getBlock(includeTransactions)`。script は scratchpad の `dagsample.py`(依存なしの `wrpc.py` を使用)。負荷は流していない。試行 1 回。
- **tx bytes の数え方:** payload + 出力 script(+8 B)+ 入力の signatureScript(+40 B)の推定。ブロック全体の serialized size ではない。
- **生ログ:** [2026-10-19-t12-passive-daa-blocks.jsonl](2026-10-19-t12-passive-daa-blocks.jsonl)(1,188 行、エラー 0)。

## 結果

| 項目 | 値 |
|---|---|
| DAA の進み | 12,318 → 12,325(1,200 秒で 7) |
| DAA が変わった間隔(秒) | 213, 150, 240, 160, 208, 144 — 中央値 184、観測最大 240 |
| blockCount / DAA | どちらも 12,318 → 12,325(この区間は 1 DAA に 1 chain block) |
| 観測した sink の数 | 15(すべて `LEGACY_HEARTBEAT`) |
| 1 ブロックの tx 数 | 1〜7 |
| 1 ブロックの tx bytes | 中央値 56,134、観測最大 111,949 |

サンプル数が 6 間隔・15 ブロックと少ないため、p99 は出していない。

## 読み方
- t12 では、1 DAA がおよそ 2.5〜4 分。node 間の回線が数 MB/s あれば、**ネットワークの帯域は door の律速にならない**。律速は、ブロック容量(`max_block_mass 500,000 / 4 = 125,000 B/DAA`、1 DAA 1 ブロック時)のうち court の証拠が使える割合。
- 静穏時でも、ブロックの tx bytes は最大 111,949 B と、125,000 B に近いことがある。heartbeat が運ぶ tx(PALW の claim・carrier 等と推定、中身は未分類)が既に容量を使っている。court の証拠が通常の tx と同じ容量を奪い合うなら、混雑時の実効は 125,000 B/DAA より低くなる。
- **これは観測値であり、安全上限でも保証値でもない。**

## 測れていないこと
- court の証拠 tx が混雑下で何 DAA で取り込まれるか(負荷を流す測定は、隔離 devnet で実施中)。
- ブロックの tx の中身の分類(claim・carrier・heartbeat の内訳)。
- 95.111.236.186 の node は DAA 3,629 で止まっていた(fork id が合わず peer との handshake が拒否される)ため、観測点に使えなかった。
