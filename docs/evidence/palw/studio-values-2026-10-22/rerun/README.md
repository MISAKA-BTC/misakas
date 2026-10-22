# P1 実測の再実行手順(最低要件に近い機械向け、2026-10-22)

[measurements.md](../measurements.md) の M-C / M-X3・M-ENV・M-RHO・M-DA の入力 / R6 を、別の機械(例: 6 コア・11 GB の x86_64 Linux VPS)で同じ方法で取り直すための手順です。**MacBook(M1 Max)での値は最低要件ハードウェアの値ではありません。** VPS での再実行は MacBook 側(lead)が行います。

## 前提
- source: `values/p1-measure` branch(計測コードはテスト側だけ。product・consensus の変更なし)。
- Rust toolchain はリポジトリの `rust-toolchain` どおり。`/usr/bin/time`(Linux は GNU time: `apt install time`)、`python3`(3.8 以上、標準ライブラリだけ)。
- 稼働中の t12 ノードがある VPS では、build を `nice -n 19`・`-j3` など既存の運用規則に合わせて絞り、作成物は終了後に削除してください(MEMORY の VPS build 規則)。計測そのものは nice なしで走ります。
- ディスクは 30 GB 以上の空きを確保してください(release の test binary 4 本で数 GB)。

## 手順
```sh
git fetch origin values/p1-measure && git checkout values/p1-measure
export CARGO_TARGET_DIR=$PWD/target          # 任意
# 1. 全バッチ(build → load 待ち → 計測)。load(1 分平均)がコア数/2 以下になるまで最大 20 分待ち、
#    下がらなければ計測して under_load=true を記録します。
python3 docs/evidence/palw/studio-values-2026-10-22/rerun/measure.py \
    --out docs/evidence/palw/studio-values-2026-10-22/raw/<host-label> \
    --source-commit "$(git rev-parse HEAD)"
# 一部だけ: --batches court_verify,in_crate,court_op_harness,frame_measure
# 2. 集計(summary.json と Markdown 表)
python3 docs/evidence/palw/studio-values-2026-10-22/rerun/analyze.py docs/evidence/palw/studio-values-2026-10-22/raw/<host-label>
```

## 何を走らせるか
| batch | binary / test | 内容 | 回数 |
|---|---|---|---|
| `court_verify` | `kaspa-consensus-core --test palw_tir_court_verify_cpu`(`court_verify_cpu_measure`、`MEASURE_OP`/`MEASURE_N`/`MEASURE_SCALE`) | court 操作ごとの検証時間・証拠 bytes・peak RSS。1 process 1 operation | 各 op: N=0 ×3(RSS 基準)、N=10・100 ×1、N=1000 ×3。scale 付き op は scale 3 段階 × (N=0 ×1 + 計測 ×3、position は ×1) |
| `in_crate` | `kaspa-consensus-core --lib` の ignored test 2 本 | material intake(Begin/Part/Finish、追加 bond 0/1k/10k)、post-Final duty(held fee 10/100/1000) | `MEASURE_RUNS=21`(run 0 が warm-up) |
| `court_op_harness` | `--test g14_court_capacity` の `palw_court_op_timing_harness` | 実 fold(`apply_palw_transition_v7`)での DisputeReserved・OneMove 開廷・42 本同時・default sweep の block fold 時間 | `MEASURE_RUNS=21` |
| `frame_measure` | `misaka-palw-tir-exec --features node --test frame_measure` | corpus 11 class の frame 操作(execute・cold answer・step check・filing・frame bytes) | 21 process |

## 出力
- `<out>/court_verify.json` などのバッチ JSON: 各 process の環境変数・終了コード・peak RSS・user/sys・開始時の load・`MEASURE` 行。
- `<out>/frame/frame-NN.json`: `misaka-palw/tir-frame-measure/v1` の正本 record(1 run 1 文書)。
- `<out>/summary.json`: analyze.py の集計。
- `host`: macOS は `sysctl hw.model hw.memsize hw.ncpu`、Linux は `nproc`・`/proc/meminfo`・`/proc/cpuinfo`。

## 注意
- 時間は wall-clock(`Instant`)で、CPU 時間ではありません。
- fixture class(TIR の corpus model)の値であり、実モデル・最大 profile の値ではありません。
- peak RSS は process 全体(setup 込み)。N=0 の process との差を「検証分」として扱いますが、独立 process の差なので記述的な値です。
