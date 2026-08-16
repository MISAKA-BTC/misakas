# PALW algo_id=4 難易度と経済の実測 — DAAレプリカ + 隔離devnet実走 (2026-08-16)

公開テストネット Track A ゲート3。方法は二段構え:

1. **DAAの正確なレプリカ** (`scripts/misaka-palw-daa-sim.py`) を書き、**live実測の
   (timestamp, bits) を食わせて全ブロック bit-exact 一致**することを先に証明する
   — レプリカが信頼できて初めて、長時間挙動を simulation で外挿できる。
2. **隔離ローカルdevnet** (実推論、`--nodnsseed` + localhost bind + `--disable-upnp`、
   本番seederへの経路ゼロ) で trivial bits から実走: ramp・収束・2nd miner参加・
   emission を実測する。

## 実行系

- node: palw-unified kaspad (devnet, PALW algo 4 always, Metal worker ~0.97s/推論)
- miner: misaminer 逐次PALW grind (20s template refresh)。tooling patch 2点:
  `MISAMINER_LOG` env (attempt毎のdebug行) と mined行への bits/ts/blue_score/
  coinbase0 追記 — **miner.logだけで監査が自己完結**し、RPC client不要になる。
- 実測較正: trivial段階の attempts/block **1.90** (理論 E=2.0, p≈0.5)、
  推論 mean 968ms、ブロックあたり +約1.0s のnode検証オーバーヘッド。

## 機構の発見: genesisはDAA窓に入らない

当初レプリカは「445日古いdevnet genesisのtimestampが窓に入り、genesis退出
(block 265) まで clamp が続く」と予測したが、**live は #151 で調整開始**。
原因はレプリカ側の誤り: [window.rs:152] に

> Special case: Genesis does not enter the DAA window due to having a fixed timestamp

と明記されており、**窓 = 直近 min(h-1, 264) 個の採掘ブロック (genesis除外)**。
古いgenesis timestampによる難易度攪乱は設計済みの既知問題として封じられていた。
修正後のレプリカは **live全ブロック bit-exact (mismatch 0)** — 初調整 #151 の値
`0x20235df9` も含めて一致。

**帰結 (launch設計)**: 固定難易度のlaunch窓は正確に `min_difficulty_window_size`
= **150ブロック**。trivial bits では fleet attempt速度で走り抜ける
(devnet実測: 150ブロック×2.76s ≈ 7分; testnet T=120s推定: 150×約10-20s ≈
25-50分で 150×4445.62 ≈ **667k MSK が前倒し発行**)。パラメータコメントの
「~5h fixed-difficulty launch window」は target cadence 前提であり、trivial bits
実態とは異なる。ゲート5のpreset判断: 前倒し発行を許容して文書化するか、
genesis bits を予測収束値近傍 (TN11の `0x200ccccc` 前例) に置くか。

## 実測 — 624ブロック完走 (単独 #1–404 → 2-miner #405–624)

| 区間 | interval | 難易度 (対genesis) | 備考 |
|---|---|---|---|
| 1–150 (固定窓) | 2.76s | 1.00x | attempts/block 1.90 (理論2.0) |
| 151–265 (ramp) | 7.33s | 3.00x | 初調整#151で即~3.5xへ |
| 265–400 | 7.48s | 4.00x | 単独収束途上 |
| 405–505 (miner2参加) | **7.06s** | **4.95x** | interval低下→難易度加速 |
| 505–625 | 8.18s | **6.13x** | 2倍平衡(~8-10x)へ収束途上 |

- **レプリカ bit-exact: 線形区間405ブロック全数、mismatch 0** (初調整#151の
  `0x20235df9` 含む)。#405以降はminer2並走で並列ブロック25個が発生しDAG化 —
  DAG窓のbit-exact再現はレプリカのscope外 (挙動検証のみ: 難易度は滑らかに上昇、
  異常値なし)。
- 2-miner応答はsim予測と同構造 (sim: dip~5.5s→8.2x@620 / live: dip 7.06s→6.13x@620)。
  差分は実系の+1s/block検証オーバーヘッドとtemplate staleness (≤20s) で説明がつく。
- 振動・発散・stallは一度もなし。attempts/block全体 6.2 (両rig計3,878 attempts/624 blocks)。
- 総所要: 約75分 (放置可能なbackground実走)。

## 収束後の経済 — carveの実測

month-0 のフル block subsidy はテーブル毎秒値 3,704,683,450 × ttpb 10,000/1000 =
**37,046,834,500 sompi** (crescendo always → after表; month 0 は daa < 262,980)。
しかし観測された coinbase 出力0 は **22,969,037,390 sompi = ちょうど 62%** だった。

これは予期していなかったが、params と sompi 単位で一致する: ADR-0018 §F の
Stage-3 分割が devnet で genesis-active (`full_reward_split_daa_score = 0`) であり、
`fee_split` は **worker_base 6200bps + worker_inclusion 800bps + validator 3000bps**
(params.rs — 「validator subsidy share raised 25% → 30% (re-genesis 同便)」)。
miner に直接支払われるのは worker BASE share のみ。

実走では **観測した118 coinbase全てが 22,969,037,390 sompi と完全一致**。
並列ブロックをmergeしたブロック (56個) はcoinbase出力が2本 — kaspaのmergeset
報酬機構 (merge対象ブロック毎に1出力) で、各出力とも62% share値。

**帰結 (公開testnetのlaunch経済)**: validator が bond するまで、チェーンは
計画排出の **62% しか mint しない** (validator 30% は don't-mint で焼却相当、
inclusion 8% は §D pool 経路)。「4445.62 MSK/block」はフル表レートであり、
miner 実収は launch 直後 **2756.28 MSK/block** — 検証ノード誘致の経済説明では
この区別を明示すること。

- **testnet T=120s換算のフル表レート: 3,704,683,450 × 120 = 444,562,014,000 sompi
  = 4445.62 MSK/block** (コードの `YEAR1_PER_BLOCK_TWO_MINUTE` 定数と一致) —
  レート保存はコード定数から再導出できた。

## testnet外挿 (T=120s, 窓264/min150, fleet実測attempt時間)

| host速度 | 収束interval | attempts/block | 収束難易度 |
|---|---|---|---|
| 6.1s (B) | ~119s | ~19 | ~9.8x |
| 7.8s (A) | ~123s | ~15.5 | ~7.9x |
| 15.7s (ibm) | ~112s | ~7.0 | ~4.0x |

単独miner検証コスト (ゲート2実測 6-16s/header) は収束後 5-13%/block — 余裕あり。

## 再現

```bash
python3 scripts/misaka-palw-daa-sim.py            # generative trajectories
python3 scripts/misaka-palw-daa-sim.py replay <miner.log...>   # bit-exact verification
python3 scripts/misaka-palw-gate3-analyze.py <workdir>         # live-run analysis
```
