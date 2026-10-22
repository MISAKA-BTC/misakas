# P1 実測 — この機械でできる §3 の計測(2026-10-22、lane values/p1-measure)

指示書: [mac-studio-values-and-measurements-2026-10-22.md](../../../design/palw/integration/mac-studio-values-and-measurements-2026-10-22.md) §3。値はここでは決めません(値の lane が使う入力です)。

**状態(2026-10-22 21:15): M-C・in-crate(material intake / post-Final duty)・R6(TIR frame)を計測済み、すべて UNDER_LOAD。M-X3・M-ENV・M-RHO・M-DA は、それらの実測と既存の t12 観測からの導出(§3〜§6)。court op harness は既存 fixture の検証失敗で NOT_RUN。** ユーザー指示(2026-10-22)により、値は目安として後で変えてよい前提で、負荷が下がるのを待たずに 1 回ずつ計測しました(各 ≥ 20 回)。

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
| 生データ | [raw/mac13-1-m1max-2026-10-22/](raw/mac13-1-m1max-2026-10-22/): `court_verify.json`・`in_crate.json`・`frame_measure.json`・`court_op_harness.json`(各 process の env・終了コード・peak RSS・開始時 load・`MEASURE` 行)、`frame/frame-NN.json`(TIR frame の正本 record 21 本)、`summary.json`(analyze.py の集計)。 |
| バッチ別の load(開始 / 終了、1 分平均) | M-C 58.4 / 60.8(18:41〜20:01)、in-crate 42.5 / 36.6(20:42〜20:49)、frame 51.6 / 51.8(21:11〜21:13)。court op harness 約 50(20:2x、失敗)。 |

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
- **court op harness**(`g14_court_capacity.rs` の `palw_court_op_timing_harness`、実 fold での DisputeReserved・OneMove 開廷・42 本同時・default sweep):**NOT_RUN。** base `34d68d7e2` で fixture の構築が params の検証で拒否されます(`g14_court_capacity.rs:120`、`Invalid("OneOutsiderCapacityIncomplete: … ServiceOpportunityIncomplete: …")`)。既存テスト側の問題で、product の修正はこの lane の範囲外です。生ログは `court_op_harness.json`。

### 1.4 material intake・post-Final duty(in-crate、実 fold、21 run、run 0 は warm-up)

material intake は history frame fixture の 2,756 target のうち、body の大きさ順に散らした 8 target(最大を含む、body 計 5,736 B、1 target 最大 839 B)を、Begin → Part → Finish の順に実際の fold(ML-DSA-87 検証、checkpoint、Finish での認証)へ通しました。署名は計時の外です。

| 操作 | state の bond 数 | 中央値 | p95 | 最大 | warm-up(最大) | 標本 |
|---|---|---|---|---|---|---|
| **material intake Begin** | 5 / 1,005 / 10,005 | **491.5 / 406.5 / 503.2 ms** | 622.9 / 595.6 / 600.9 ms | 702.7 / 694.4 / 712.6 ms | 593 / 609 / 658 ms | 160 |
| material intake Part(≤ 839 B) | 5 / 1,005 / 10,005 | 0.55 / 0.81 / 3.65 ms | 2.23 / 3.63 / 24.9 ms | 23.9 / 26.6 / 111.8 ms | 9.3 / 0.9 / 26.1 ms | 160 |
| **material intake Finish** | 5 / 1,005 / 10,005 | **429.7 / 378.8 / 451.6 ms** | 575.2 / 547.7 / 603.5 ms | 651.6 / 718.1 / 721.3 ms | 579 / 685 / 672 ms | 160 |

| 操作 | held fee 数(= open accusation 数) | 中央値 | p95 | 最大 |
|---|---|---|---|---|
| post-Final accusation(1 件) | 10 / 100 / 1,000 | 1.58 / 1.75 / 1.88 µs | 2.96 / 1.79 / 2.79 µs | 3.4 / 1.9 / 16.9 µs |
| post-Final answer(1 件) | 10 / 100 / 1,000 | 1.33 / 1.38 / 1.67 µs | 1.38 / 1.50 / 2.17 µs | 1.5 / 1.6 / 2.9 µs |
| post-Final observe(1 block、open 全件) | 10 / 100 / 1,000 | 7.2 / 51.6 / 290.1 µs | 8.8 / 52.1 / 300.8 µs | 17.5 / 52.2 / 301.0 µs |
| **post-Final default sweep(1 block、open 全件が既定)** | 10 / 100 / 1,000 | **0.080 / 1.58 / 51.7 ms** | 0.083 / 1.59 / 97.1 ms | 0.089 / 1.68 / 143.9 ms |

読み取り:
- **material intake の Begin と Finish は 1 回 0.4〜0.5 s** で、Part(0.5〜3.7 ms)の約 100 倍。state の bond 数(5〜10,005)にはほぼ依存しないので、checkpoint の state clone ではなく、original 全体(この fixture では 2,756 target)に比例する導出(per-claim budget・target bound の再計算と推定。profiler の symbol が無く未確認)が支配的と見られます。Part だけは state の大きさに比例して増えます(bond 10,005 で 3.65 ms)。**1 件の Begin が node に 0.5 s の CPU を課すため、V5(保管料)と liveness(L-COURT)の両方の入力です。**
- post-Final default sweep は open 件数に対して線形より速く増えます(10→100 で 20 倍、100→1,000 で 33 倍)。observe は線形(10→1,000 で 40 倍)。
- 保持 bytes(8 target 分): body 5,736 B に対し、canonical row 9,068 B(1.58 倍)、block の delta 64 entry・53,173 B(9.3 倍、draft・part・ledger の行の journal を含む)。post-Final duty の row は 915 B。

## 2. R6 — TIR frame の費用 record の入力(`frame_measure.rs`、corpus 11 class × 21 process、run 0 は warm-up)

| class | leaves | execute 中央値 | cold answer 全件(中央値 / p95 / 最大) | court check 全 leaf の合計(中央値) | 1 leaf の court check 最大(中央値) | filing witness | capture bytes | prepared retained | owner turns(upper 6) | door DAA(upper 6) |
|---|---|---|---|---|---|---|---|---|---|---|
| corpus dense / dense-gqa-2layer | 118 | 0.66 / 0.75 ms | 23.2 / 58.0 / 67.3 ms・8.6 / 8.7 / 13.4 ms | 96.0 / 61.1 ms | 21.3 / 2.1 ms | 11,892 B | 15,178 B | 32,189 B | 11 | 2,192 |
| corpus gdn / gdn-k2-v4-grouped | 222 | 1.06 / 1.08 ms | 27.7 / 65.0 / 94.0 ms・22.2 / 56.3 / 57.8 ms | 276.9 / 269.6 ms | 31.9 / 26.7 ms | 20,403 B | 28,339 B | 59,494 B | 11 | 2,263 |
| corpus sliding / sliding-global | 166 | 0.95 / 0.95 ms | 24.6 / 51.2 / 62.9 ms・16.7 / 62.9 / 72.7 ms | 193.8 / 168.4 ms | 24.2 / 23.4 ms | 17,569 B | 23,447 B | 47,114 B | 11 | 2,203 |
| corpus mamba2 / mamba2 | 95 | 0.47 / 0.47 ms | 6.7 / 63.8 / 89.2 ms・6.7 / 13.5 / 30.1 ms | 51.2 / 49.0 ms | 6.8 / 6.2 ms | 15,076 B | 17,054 B | 30,937 B | 11 | 2,191 |
| corpus moe / moe-top2-shared | 52 | 0.47 / 0.48 ms | 3.3 / 31.6 / 39.9 ms・3.3 / 17.3 / 26.8 ms | 28.4 / 34.3 ms | 1.4 / 3.9 ms | 9,789 B | 9,993 B | 18,028 B | 11 | 2,145 |
| h-window-sum | 55 | 0.14 ms | 3.4 / 15.5 / 23.1 ms | 1.4 ms | 0.05 ms | 2,356 B | 4,380 B | 12,823 B | 11 | 2,145 |

- frame の full-frame encode + hash: upper 6(84,905 B)で **1.05 ns/B**(中央値、p95 2.53、最大 20.3)、upper 257(2,921,707 B)で 1.01 ns/B(p95 5.46)。既存の料率(1 byte 32 step)で 1 action に課金される parser step は upper 6 で 86,942,720、upper 257 で 2,991,827,968。
- `PalwTirFrameClockV1` の入力のうち、ここで得たもの: full-frame pass の parse 速度(上の ns/B)、ML-DSA-87 verify 1 回(§1、126 µs)、frame bytes。**得ていないもの:** 受理 control 1 件の fold 時間(`action_parser_steps` の当てはめ元。fold record が別に必要)、`control_bytes` の出荷版最大、`inclusion_margin_daa`・`parallel_bytes_per_daa`・`reorg_replay_moves`(ネットワーク観測が必要)。corpus class は小さく、実モデル・最大 profile の値ではありません。

## 3. M-X3 — `B_*` 項(導出、fixture class)

t12 の既存観測(`docs/measurements/2026-10-19-t12-passive-daa-blocks.md`): 1 DAA の中央値 184 s(観測最大 240 s、6 間隔)、1 DAA 1 block、block の上限 125,000 B/DAA、観測した block の tx bytes は中央値 56,134 B・最大 111,949 B(空きは中央値 68,866 B・最悪 13,051 B)。court の turn deadline は 42 DAA(court cap 42 = `turn_deadline × PALW_COURT_CLOSE_MAX_PER_BLOCK`(= 1))。

| 項 | 実測した入力(この機械、負荷下) | DAA への換算 | 状態 |
|---|---|---|---|
| `B_cold_fetch` | corpus class の cold answer 全件: 最大 class で中央値 27.7 ms・p95 65.0 ms・最大 94.0 ms(§2) | 計算は 1 DAA(184 s)の 0.05% 未満 → **block 単位で 1 DAA** | fixture のみ。実モデルの取得(provider からの転送・disk)は未計測 |
| `B_check` | 全 leaf の court check の合計: 最大 class 276.9 ms、owner の refutation 構築の合計 最大 689.9 ms(§2)。1 件の court 操作は最大 1.46 ms(中央値、§1) | → **1 DAA** | fixture のみ。実モデルの check は 32 GiB では未計測(10-18 記録と同じ) |
| `B_localize` | localization の owner turn 数 = 11(upper 6、全 class)。court の dissection は最大 2 round(§1.2)+ position round + bottom | 1 turn = 次の block に入れば 1 DAA、相手が turn deadline まで遅らせれば 42 DAA → **11〜462 DAA**(court 経路は open・2 round・position・close で 8 move: 8〜336 DAA) | fixture の turn 数。実モデルの turn 数は step leaf 数で増える |
| `B_carrier` | 最大の court object 33,821 B(position close、署名 2 本込み)、close cone 10,753 B、root claim 16,571 B | 空きが中央値なら 1 DAA、観測最悪の空き 13,051 B なら ⌈33,821 / 13,051⌉ = **3 DAA**。既存の `PALW_COURT_CLOSE_INCLUSION_MARGIN` は 4 DAA / chunk | 混雑下の取り込み遅延は未観測 |
| `B_reorg_slack` | — | — | **NOT_RUN**(restart / reorg drill(V7)とネットワーク観測が必要) |
| `c_retain` | 保持 bytes: material intake で body の 1.58 倍の row(§1.4)、TIR frame の prepared retained 12.8〜59.5 KB / claim(§2) | 価格(sompi / DAA)は実測では出ない | **価格は NOT_RUN**(実際の保持費用・市場価格が必要) |

この fixture での Δ = `B_cold_fetch + B_check + B_localize + B_carrier + B_reorg_slack` は、`B_reorg_slack` を除いて **最小 1 + 1 + 11 + 3 = 16 DAA、相手が全 turn を期限まで遅らせる場合 1 + 1 + 462 + 3 = 467 DAA**。どの項も 0 ではありません。X3 の合成(`hard − accepted = 5,520 + Δ`、`palw_x3_composed_policy_v1`)に入れる値の選択は値の lane の判断です。

## 4. M-ENV — legacy filer の hard 期限 envelope と held DA

- envelope `start + B_cold_fetch + B_check + B_localize + B_disclose + B_court + B_carrier + B_reorg_slack ≤ trace_retention − W_disclose`: t12 の既存の窓は `B_disclose + B_court` = 1,200 + 3,000 = 4,200 DAA、producer の既存 retention は bind 600 + receipt 600 + challenge 1,200 + court 3,000 = 5,400 DAA。測った `B_*` は §3 の Δ(fixture で 16〜467 DAA、`B_reorg_slack` 無し)。**class ごとの最大 profile での envelope は NOT_RUN**(実モデルの最大 profile が必要。GAP-07)。
- legacy held DA の descent round 数・answer bytes・34 session の予約予算: **NOT_RUN。** この worktree に legacy held DA descent の計測 harness が無く、実ノード E2E(lg14b)が必要です。参考として、署名付き court object 1 件の bytes は §1 の表(4,779〜33,821 B)。

## 5. M-RHO — court 占有の経済性(導出)

court 操作 4 クラスに §1 の実測を当て、署名付き object は ML-DSA-87 verify(中央値 126.2 µs、p95 135.1 µs)を加えます。対応付けは近似です(OneMove ≈ close cone の再計算、Opening ≈ root claim admit、DissectionMove ≈ round apply、Close ≈ dissection bottom / position close の大きい方、position close は署名 2 本)。

| クラス | 中央値ベース | p95 ベース | bytes(最大) |
|---|---|---|---|
| OneMove | 381 + 126 = 507 µs | 3,482 + 135 = 3,617 µs | 10,753 |
| Opening | 1,463 + 126 = 1,589 µs | 9,884 + 135 = 10,019 µs | 16,571 |
| DissectionMove | 0.3 + 126 = 126.5 µs | 1.4 + 135 = 136.5 µs | 806(+ 署名 4,627) |
| Close | max(1,201 + 126, 1,124 + 252) = 1,376 µs | max(8,361 + 135, 3,838 + 270) = 8,496 µs | 33,821 |

- `PalwCourtMeasurementV1::policy()` と同じ規則(単位 = 最も安いクラスの時間、各クラス = ⌈時間 / 単位⌉、session weight = 最も重いクラス)で計算すると、**op_work = [5, 13, 1, 11]、session weight = 13**(中央値ベース、単位 126.5 µs)。p95 ベースでは [27, 74, 1, 63]、weight 74(負荷下の p95 なので過大側)。
- 1 session の占有: court 経路の move 数(この fixture で 8)× 1 move あたり最大 turn deadline 42 DAA → **最大 336 DAA**。全 42 slot を T DAA 占有する攻撃者の費用は `ρ_court × 42 × T × weight`(weight 13 なら 546 · ρ_court · T)。ρ_court の値は値の lane の判断です。
- node 側の負荷: block 1 個を court object だけで埋めても(125,000 B / 10,753 B ≈ 11 件 × 1.6 ms)約 18 ms の検証で、184 s の block 間隔に対しては小さい。**律速は CPU ではなく block の bytes です。** ただし material intake の Begin / Finish(1 件 0.4〜0.5 s)と post-Final default sweep(open 1,000 件で 52 ms / block)は、件数の上限が無ければ node の時間を大きく使います(§1.4)。

## 6. M-DA — class ごとの DA 取得量と保持 bytes

| class(corpus) | material(capture)/ claim | cold answer の request 数 / answer 1 件の最大 | prepared retained / claim | frame bytes(upper 6) |
|---|---|---|---|---|
| dense | 15,178 B | 121 / 647 B | 32,189 B | 84,905 B |
| gdn | 28,339 B | 225 / 711 B | 59,494 B | 84,905 B |
| sliding | 23,447 B | 169 / 711 B | 47,114 B | 84,905 B |
| mamba2 | 17,054 B | 98 / 647 B | 30,937 B | 84,905 B |
| moe | 9,993 B | 55 / 583 B | 18,028 B | 84,905 B |
| h-window-sum | 4,380 B | 58 / 523 B | 12,823 B | 84,905 B |

- 保持 bytes / DAA は「claim 数 / DAA × 上の bytes」です。chain に載る量の上限は block の 125,000 B/DAA(1 DAA 1 block)で、これを全部使っても 1 日(≈ 470 DAA)で約 59 MB。
- material intake を通した chain 上の保持は、body 1 B あたり row 1.58 B、journal(delta)9.3 B(§1.4)。
- 実モデル artifact での DA 量は NOT_RUN(手元に実 artifact がありません)。

## 7. NOT_RUN(この機械では実施しない)

| ID | 理由 |
|---|---|
| M-Q36 | 35B MoE を f32 で長系列実行する必要があり、32 GiB の RAM では足りません。実 artifact もこの worktree にありません。 |
| M-MEM | RFC-0020 の 8K / 16K / 32K 計測には実 artifact が必要です。 |
| M-STO | 30 GB ストレージの M1〜M9 には devnet と実 artifact が必要です。この機械のディスクは他 lane と共有で 30 GB 以上の空きを保つ規則があります。 |
| M-UPG | RFC-0019 の R7 / R8 には実 artifact が必要です。 |
| M-L | 指示の P1 範囲外(liveness branch の計測)。 |
