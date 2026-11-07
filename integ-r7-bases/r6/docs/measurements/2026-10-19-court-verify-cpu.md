# 1,000 件の court 証拠を node が検証する CPU・メモリ(fixture class、release)

測定日: 2026-10-19(JST)。**測定対象は fixture class(TIR の 5 コーパス model と 2 state program、既存 test fixture)であり実モデルではない。** 安全上限ではない。観測最大は 1 試行の観測値。

## 条件
- source: g14/door-design `2a6ea1a4f`(計測 test の commit)。base は `16458bbb1`。
- build: `cargo test --release -p kaspa-consensus-core --test palw_tir_court_verify_cpu`(`--ignored`)。`cargo` は `/Users/wata/.cache/misaka-heavy-lock.sh` 経由。
- 機材: Apple M1 Max、この Mac だけ。稼働中の t12 や外部 host は使っていない。並列実行はしていない(1 プロセス 1 コアで逐次)。
- 試行: 操作ごとに N = 0 / 1 / 10 / 100 / 1,000 を各 1 プロセス。N = 0 は fixture 構築のみで、その RSS が setup の基準。
- 各 run の時間は 1 件ごとに `Instant` で計測(build・証拠構築は計測外)。cases は distinct 上限 1,000 で構築し、N が distinct を超えたら循環(該当表の distinct 列を参照)。
- ピーク RSS は `/usr/bin/time -l` の maximum resident set size。setup 込みの値。
- 証拠 bytes は borsh 直列化のサイズ。**ケースごとに変わり得るが、ここでは最初の 1 ケースの値を記録。**

## 結果(1 件あたり、N = 1,000 の run)

| 操作 | 中央値 | p99 | 観測最大 | 1,000 件の合計 CPU | ピーク RSS(N=1000) | RSS(N=0 基準) | 証拠 bytes(先頭ケース) | 期待どおり |
|---|---|---|---|---|---|---|---|---|
| (c) close: honest leaf, full cone recomputation (NoFaultFound) | 349.4 µs | 3.254 ms | 3.521 ms | 0.553 s | 34.0 MiB | 34.0 MiB | 10,753 | 1000/1000 |
| (d) close: forged lane, openings pass, last recomputation convicts (ComputationMismatch) | 353.7 µs | 3.262 ms | 3.384 ms | 0.551 s | 33.8 MiB | 34.4 MiB | 10,753 | 1000/1000 |
| (c') root claim admission on a dissected leaf | 1.364 ms | 1.496 ms | 1.551 ms | 1.342 s | 15.6 MiB | 15.3 MiB | 16,571 | 1000/1000 |
| (b) one dissection round applied to a phase | 0.3 µs | 1.7 µs | 4.8 µs | 0.000 s | 15.9 MiB | 13.0 MiB | 806 | 1000/1000 |
| (c'') dissection bottom, honest (NoFaultFound) | 1.112 ms | 1.224 ms | 1.301 ms | 1.082 s | 15.1 MiB | 15.0 MiB | 14,834 | 1000/1000 |

## N 別の中央値・p99(ns)

### close_cone_acquit

| N | distinct | 中央値 | p99 | 観測最大 | 合計 CPU(ns) | 期待どおり |
|---|---|---|---|---|---|---|
| 1 | 984 | 212625 | 212625 | 212625 | 212625 | 1 |
| 10 | 984 | 288500 | 587459 | 587459 | 3168960 | 10 |
| 100 | 984 | 294625 | 765000 | 773459 | 43011174 | 100 |
| 1000 | 984 | 349417 | 3254291 | 3521000 | 552736573 | 1000 |

### close_cone_forged_last_stage

| N | distinct | 中央値 | p99 | 観測最大 | 合計 CPU(ns) | 期待どおり |
|---|---|---|---|---|---|---|
| 1 | 984 | 210375 | 210375 | 210375 | 210375 | 1 |
| 10 | 984 | 291125 | 589875 | 589875 | 3206419 | 10 |
| 100 | 984 | 295625 | 769000 | 771666 | 42446580 | 100 |
| 1000 | 984 | 353708 | 3262333 | 3383875 | 550885334 | 1000 |

### root_claim_admit

| N | distinct | 中央値 | p99 | 観測最大 | 合計 CPU(ns) | 期待どおり |
|---|---|---|---|---|---|---|
| 1 | 96 | 1371833 | 1371833 | 1371833 | 1371833 | 1 |
| 10 | 96 | 1281916 | 1344167 | 1344167 | 12850667 | 10 |
| 100 | 96 | 1383709 | 1482417 | 1484750 | 135605417 | 100 |
| 1000 | 96 | 1363666 | 1496375 | 1551125 | 1341824502 | 1000 |

### dissect_round_apply

| N | distinct | 中央値 | p99 | 観測最大 | 合計 CPU(ns) | 期待どおり |
|---|---|---|---|---|---|---|
| 1 | 64 | 1500 | 1500 | 1500 | 1500 | 1 |
| 10 | 64 | 292 | 1542 | 1542 | 4416 | 10 |
| 100 | 64 | 333 | 1833 | 10250 | 49418 | 100 |
| 1000 | 64 | 333 | 1666 | 4750 | 412250 | 1000 |

### dissect_bottom_honest

| N | distinct | 中央値 | p99 | 観測最大 | 合計 CPU(ns) | 期待どおり |
|---|---|---|---|---|---|---|
| 1 | 96 | 976083 | 976083 | 976083 | 976083 | 1 |
| 10 | 96 | 939084 | 1005833 | 1005833 | 9519958 | 10 |
| 100 | 96 | 1116500 | 1142792 | 1143958 | 106931372 | 100 |
| 1000 | 96 | 1112417 | 1223917 | 1301209 | 1081771290 | 1000 |

## 読み取れること
- close(cone)の 1 件検証は中央値 0.2〜0.35 ms、p99 は約 3.2 ms(N=1,000)。N=1,000 の合計 CPU は約 0.55 s。honest と forged で差はない(最後の再計算まで走るため)。
- root claim は中央値約 1.3 ms、dissection bottom は約 1.1 ms。1,000 件の合計で約 1.3 s と約 1.1 s。
- dissection round 1 件は中央値 0.3 µs 程度(phase の状態遷移と fold 検査のみ)。
- RSS: N=0(setup)と N=1000 のピークはほぼ同じ(差は 0〜3 MiB)。検証そのものでの増加は小さく、ピークは fixture 構築が支配的。
- 1,000 件を同時に来たと仮定した直列の合計は、close 0.55 s + root 1.34 s + bottom 1.08 s 程度。これは 1 コアの逐次値で、並列・実ノードの I/O やキャッシュ状態は含まない。

## 測れなかったもの・注意
- (a) one-move 判定 `palw_tir_one_move_verdict_v1`:chain state(`PalwChainStateV2`・claim)の組み立てが必要で、この計測では未実施。close(cone)の核となる検証で代用しているが、one-move 固有の state 検査コストは含まない。
- 履歴の terminal(`adjudicate_terminal_v1`、attention の history tile)は未実施。
- dissection bottom の『不正な証拠』版(最後の reduction で失敗)は未実施。honest のみ。
- 証拠 bytes は先頭ケースのみ。ケース間の bytes 分布は未記録。
- 並列実行は未実施。
- dissect_round_apply は前回実行で、単一 tile の range(開始時点で terminal)を含めたため `AlreadyTerminal` で拒否される誤った setup があった。除外して再測定した結果のみを記録している。
- 計測 test の出力は `MEASURE` 行(stderr)。JSON は `2026-10-19-court-verify-cpu.json` に同名で置いた。
