# 隔離 devnet での 17,408 B payload tx の取り込み(2026-10-19)

> **t12 の実網ではない。** この Mac(Apple M1 Max)の上だけの隔離 devnet。127.0.0.1 のみ、外部 peer なし。

- **日時:** 2026-10-19 16:25〜16:47 JST(unix 1792394700–1792396000 前後)。
- **ネットワーク:** `scripts/drills/misaka-palw-round-lane-devnet-drill.sh` の 4 node 構成(`--devnet --palw-devnet-floor-only --palw-produce --palw-panel --palw-round-lane`、bond 0〜3)。node-0 は rothschild 用に gRPC 127.0.0.1:26610 と wRPC JSON 127.0.0.1:28110 を開けて再起動した(drill は `--nogrpc`)。
- **build:** `release/t12-16000` 72ad80463(kaspad・rothschild・misaminer を `--release`)。
- **送信側:** ML-DSA-87 化した rothschild(67bc4d4ca)、`--payload-size 17408 --tps 1`。資金は、この devnet だけの公開 premine から計測用の鍵へ 1,000 MSK を送ったもの(t12 や他網の鍵・資金は使っていない)。
- **計測:** `measure.py`(依存なしの wRPC JSON client)で 1 秒ごとに DAA と、payload 17,408 B の計測 tx の mempool 滞留を記録。新しい sink ごとに、ブロックの tx bytes と計測 tx の取り込み DAA を txid で突き合わせた。
- **段階:** 送信と停止を shell で繰り返した。
  - duty005:3 秒送って 57 秒止める(≈ 0.05 tx/s の意図)
  - duty020:12 秒送って 48 秒止める(≈ 0.2 tx/s の意図)
  - sat100:連続(1 tx/s の意図)
  - 各段は DAA が 10 進むまで。
- **生ログ:** [2026-10-19-devnet-payload-carrier.jsonl](2026-10-19-devnet-payload-carrier.jsonl)(3 段階を連結、1,180 行)。
- **試行回数:** 各段 1 回。

## 結果

| 段階 | DAA | 経過 | 送れた計測 tx | 取り込みまで(DAA) | 取り込みまで(秒) | mempool 滞留の最大 |
|---|---|---|---:|---|---|---:|
| duty005 | 16 → 27 | 361 s | 1 | 0 | 8 | 1 |
| duty020 | 27 → 38 | 300 s | 2 | 0, 1 | 12, 13 | 2 |
| sat100 | 38 → 48 | 480 s | 4 | 0, 0, 0, 0 | 3, 2, 1, 2 | 2 |

| 項目 | 値 |
|---|---|
| 観測したブロック | 35(すべて `LEGACY_FLOOR`) |
| 1 ブロックの tx bytes | 中央値 49,752、観測最大 111,927 |
| 1 DAA の秒数 | 中央値 28、観測最大 124(31 間隔) |
| 計測 tx の取り込み | 7 本すべて、送信と同じ DAA か次の DAA |

## 読み方と限界

- **意図した負荷はかかっていない。** 3 段階を通して、送れた計測 tx は合計 7 本だけだった。1 tx/s を 8 分続けたはずの sat100 でも 4 本。rothschild の出力を捨てていたため原因は特定できていない。推定は、資金の UTXO が 1 つで、rothschild が前の tx の確認(非 coinbase は 10 DAA)を待つことと、段階ごとに process を起動し直していたこと。**したがって、混雑・飽和の下での取り込みは測れていない。**
- 測れたのは、**静かな devnet で、17,408 B の payload を持つ ML-DSA-87 の tx が mempool と block に受理され、0〜1 DAA で取り込まれた**ことだけ(7 本)。改修した rothschild が devnet の consensus に受理されることの確認にはなる。
- 静かな devnet でも、producer の floor tx がブロックあたり最大 111,927 B を使っていた(t12 の受動的な観測の 111,949 B とほぼ同じ)。court の証拠が通常の tx と同じ容量を奪い合えば、混雑時に court が使える容量は 125,000 B/DAA よりかなり小さい。
- 記録 script は sink のブロックしか読まないので、ブロックごとの bytes は取りこぼしがあり得る(下限)。取り込みの DAA はブロック header の DAA で、chain block として受理された時点ではない。
- tx 1 本の serialized bytes と mass は記録していない。
- **観測値であり、安全上限でも保証値でもない。** 試行は各段 1 回。p99 は出していない。

## 次に必要なこと

- 計測用の UTXO をあらかじめ多数に分けておき、rothschild を 1 process で連続して走らせ、出力を残す。
- そのうえで、ブロック容量(125,000 B/DAA)を超える負荷をかけ、取り込みまでの DAA の分布を測る。
