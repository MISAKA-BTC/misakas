# P1 実測 — この機械でできる §3 の計測(2026-10-22、lane values/p1-measure)

指示書: [mac-studio-values-and-measurements-2026-10-22.md](../../../design/palw/integration/mac-studio-values-and-measurements-2026-10-22.md) §3。値はここでは決めません(値の lane が使う入力です)。

**状態: M-C 完了(負荷下)。M-X3・M-ENV・M-RHO・M-DA・R6 は計測中 — この節は後で追記します。**

## 0. 環境と計測の衛生

| 項目 | 値 |
|---|---|
| 機材 | `hw.model` = **Mac13,1**(Mac Studio、Apple M1 Max)、`hw.ncpu` = 10(P 8 + E 2)、`hw.memsize` = 34,359,738,368(32 GiB)、macOS 26.7.1 |
| 最低要件か | **いいえ。** 最低要件ハードウェアの値ではありません。最低要件(例: 6 コア・11 GB の x86_64 Linux)では [rerun/](rerun/README.md) の手順で取り直してください。 |
| build | `cargo test --release`(`CARGO_TARGET_DIR` 共有、build は nice 10)。計測は nice なし。共有 target を他の agent の build が上書きするため、binary は private な場所へ写してから実行(`measure.py --bin-dir`)。 |
| source | M-C の binary: `4e6df6ad4`(`palw_tir_court_verify_cpu.rs`)。in-crate の binary: `80c8f5e1b`。product・consensus のコードは base `34d68d7e2`(`g14/integration-r7`)から変更なし。 |
| **負荷** | **すべてのバッチが負荷下です。** 同じ機械で 5 つ以上の agent が並列に build しており、1 分平均負荷は 51〜61(コア数 10、閾値 5)。measure.py は 20 分待っても下がらなかったため、そのまま計測して `under_load=true` を記録しました(M-C: 開始 18:41 load 58.40/51.32/43.70、終了 20:01 load 60.83/51.20/54.08、119 process 全部が負荷下)。 |
| 負荷の影響 | 中央値は 10-19 の静かな計測とほぼ一致(close cone 349 µs → 381 µs、root claim 1.36 → 1.46 ms、bottom 1.11 → 1.20 ms)。**p95・最大は preemption で膨らんでいます**(最大 127 ms など)。最大値を安全上限として使わないでください。 |
| 回数 | 1 process 1 operation。in-process で N = 10 / 100 / 1,000 件(N=1,000 は 3 process)。最初の 1 件(warm-up)は分けて記録し、中央値・p95・p99・最大は残りの件から。N=0(fixture 構築のみ)の process を 3 回走らせて RSS の基準にしました。 |
| 時間の意味 | `Instant` の wall-clock(CPU 時間ではありません)。fixture class(TIR の corpus model 5 つと state program 2 つ)であり、実モデル・最大 profile の値ではありません。 |
| 生データ | [raw/](raw/) の `court_verify.json`(各 process の env・終了コード・peak RSS・開始時 load・`MEASURE` 行)と `summary.json`。 |

## 1. M-C — court 操作クラスごとの検証時間・証拠 bytes・working set

### 1.1 結果(N = 1,000、1 件あたり)

| 操作 | 中央値 | p95 | 最大(負荷下) | warm-up | 証拠 bytes(先頭 / 最大) | peak RSS | RSS − N=0 | distinct |
|---|---|---|---|---|---|---|---|---|
| close cone acquit(NoFaultFound) | 380.8 µs | 3.48 ms | 127.7 ms | 0.23 ms | 10,753 | 34.7 MiB | 0.5 MiB | 984 |
| close cone forged(最後の再計算で有罪) | 386.9 µs | 3.20 ms | 46.4 ms | 0.22 ms | 10,753 | 34.7 MiB | 0.4 MiB | 984 |
| root claim admit | 1.463 ms | 9.88 ms | 75.4 ms | 4.88 ms | 16,571 | 16.0 MiB | 0.2 MiB | 96 |
| dissection round apply | 0.29 µs | 1.4 µs | 5.5 µs | 2.5 µs | 806 | 16.8 MiB | 3.1 MiB | 64 |
| dissection bottom honest | 1.201 ms | 8.36 ms | 133.5 ms | 2.00 ms | 14,834 / 23,225 | 15.9 MiB | 0.3 MiB | 96 |
| position close honest(tile 4) | 1.115 ms | 3.84 ms | 43.5 ms | 1.08 ms | 25,900 / 33,555 | 15.1 MiB | 0.2 MiB | 90 |
| position close forged(中央の嘘、有罪) | 1.124 ms | 2.88 ms | 24.2 ms | 1.08 ms | 26,752 / 33,363 | 15.0 MiB | −0.1 MiB | 64 |
| lookahead expand(深さ 2) | 0.63 µs | 5.4 µs | 106 µs | 7.2 µs | 814 / 1,214 | 13.8 MiB | 0.1 MiB | 64 |
| D2 demand open(draw 256) | 0.041 µs | 0.042 µs | 0.084 µs | 0.7 µs | 152(署名込み 4,779) | 12.3 MiB | 0.2 MiB | 256 |
| D2 answer disclosed(draw 256) | 3.08 µs | 3.21 µs | 5.9 µs | 7.5 µs | 652(署名込み 5,279) | 12.7 MiB | 0.3 MiB | 256 |
| D2 answer convicted(draw 256) | 3.04 µs | 3.25 µs | 4.8 µs | 7.7 µs | 652(署名込み 5,279) | 12.5 MiB | −0.1 MiB | 256 |
| ML-DSA-87 verify(1 署名) | 126.2 µs | 135.1 µs | 2.24 ms | 0.16 ms | 署名 4,627、公開鍵 2,592 | 13.2 MiB | 0.4 MiB | 64 |

- 証拠 bytes は borsh 直列化。position close は close object 全体(responder と challenger の ML-DSA-87 署名 2 本 = 9,254 B を含む)。D2 は object 本体、括弧内は署名 4,627 B を足した値。
- 署名付きの court object は、表の検証時間に加えて validator が ML-DSA-87 verify(中央値 126 µs)を 1 回払います。
- peak RSS は process 全体(setup 込み)。差(RSS − N=0)はどの操作も 3.1 MiB 以下で、検証そのものの working set は小さく、ピークは fixture 構築が支配的です(独立 process の差なので記述的な値)。

### 1.2 成長の次数(N・scale を 3 段階)

| 操作 | 段階 | 中央値 | p95 | 読み取り |
|---|---|---|---|---|
| close cone acquit | N 10 / 100 / 1,000 | 290 / 297 / 381 µs | 0.59 / 0.77 / 3.48 ms | 1 件の時間は N に依存しない(N を増やすと preemption を拾う確率が上がり p95 が増える) |
| root claim admit | N 10 / 100 / 1,000 | 1.32 / 1.48 / 1.46 ms | — | 同上 |
| dissection bottom | N 10 / 100 / 1,000 | 0.98 / 1.19 / 1.20 ms | — | 同上 |
| position close honest | tile 4 / 8 / 16 | 1.115 / 1.113 / 1.124 ms | — | **この fixture では tile 幅を上げても terminal の幅が変わらない**(distinct 90 / 80 / 80、最大 bytes 33,555 / 33,629 / 33,629)。幅による増え方は測れていない |
| D2 answer | draw 16 / 256 / 4,096 | 1.88 / 3.08 / 4.25 µs | — | path 長(log₂ draw)に比例。bytes は 396 / 652 / 908(1 段 64 B 増) |
| D2 demand open | draw 16 / 256 / 4,096 | 41 ns(3 段とも) | — | 定数 |
| **D2 deadline sweep(1 block)** | open case 16 / 256 / 4,096 | **0.71 / 8.67 / 206.8 µs** | 0.88 / 9.25 / 217.8 µs | **registry 全体に線形以上**(16→256 で 12×、256→4,096 で 24×)。毎 block、全 registry を走査するため、open case 数の上限が block あたりの費用を決める |

- dissection の honest descent は、この fixture では最大 2 round で terminal tile に達しました(`max_rounds_to_terminal=2`)。実モデルの round 数は step leaf 数で決まり、ここからは出ません。

### 1.3 測れなかったもの(M-C)

- **shared bottom v2 の native fold**(`check_shared_tir_position_bottom_v2`):武装した fixture がありません。既存の in-crate テストは `palw_tir_bounded_door_v1` 未満で拒否されることだけを確かめています。部品(position bottom の check = position close の表、ML-DSA-87 verify)で代用し、fold 固有の state 参照の費用は含みません。
- **forged dissection bottom**:fixture 上で偽の lane が terminal tile の cone に入る case が作れず、op から外しました。有罪経路は close cone forged と position close forged で測っています。
- (a) one-move 判定、履歴 terminal:10-19 の記録と同じく未実施です。
- material intake の part / finish、post-Final duty の accusation / answer は in-crate のバッチで計測中です(§2 に追記します)。

## 2. 以降の計測(計測中)

court op harness(実 fold での OneMove 開廷・42 本同時・default sweep)、material intake、post-Final duty、TIR frame(R6)、および M-X3 / M-ENV / M-RHO / M-DA の導出は、このあと追記します。

## 3. NOT_RUN(この機械では実施しない)

| ID | 理由 |
|---|---|
| M-Q36 | 35B MoE を f32 で長系列実行する必要があり、32 GiB の RAM では足りません。実 artifact もこの worktree にありません。 |
| M-MEM | RFC-0020 の 8K / 16K / 32K 計測には実 artifact が必要です。 |
| M-STO | 30 GB ストレージの M1〜M9 には devnet と実 artifact が必要です。この機械のディスクは他 lane と共有で 30 GB 以上の空きを保つ規則があります。 |
| M-UPG | RFC-0019 の R7 / R8 には実 artifact が必要です。 |
| M-L | 指示の P1 範囲外(liveness branch の計測)。 |
