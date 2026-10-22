# T3 — liveness 計測（L-COURT-01 / L-LC-01/02）、Mac Studio、2026-10-22

計測だけで、product code は変更していない。追加したのは `#[ignore]` 付きの計測テスト 3 本と build/run script だけ。

## 0. 何を、どこで計測したか
- branch `studio/t3-liveness` = `origin/g14/integration-r7` `193c2c65b` + merge `origin/liveness/court` `f74ca87d3` + merge `origin/liveness/lifecycle` `7abbaaed3`（衝突なし）+ 計測 commit `5fd3ba53a`。raw log は `bf8a225ce`。
- rustc 1.93.0 (254b59607 2026-01-19)、`--release`、timed run は `nice` なし、`--test-threads=1`。binary は共有 target から退避して直接実行した。
- ハードウェア: `hw.model: Mac13,1`（Mac Studio M1 Max）、`hw.memsize: 34359738368`、`hw.ncpu: 10`。Mac Studio は最低要件ハードウェアではない。
- 負荷: **全 batch が UNDER_LOAD。** run 1 は 20 分待っても load1 が 5 を下回らず（開始時 load1 ≈ 27）、run 2 も同じ（≈ 45）。他 agent の build が同時に走っていた。
- binary の sha256: lib `26f2dfa6…15f9`、court_close_floor `c16c3e11…a0f5`、held_close_chunks `c62fe30d…71c8`、court_close_replay `68a2f070…e682`、court_fences `e4ba57a3…39ae`（全桁は raw log の各ファイルにある）。
- 計測テスト:
  - `consensus/core/tests/studio_t3_court_close_floor.rs` — floor を 7 サイズ、段ごとと合計で。warm-up 3、本計測 30。
  - `consensus/core/tests/palw_held_close_chunks.rs::studio_t3_court_adjudication_cost` — 実 Gen held-close fixture の上で `adjudicate_court_close_v3`（→ v5）。warm-up 3、本計測 30。
  - `consensus/core/src/palw_state_v2/tests/liveness_lifecycle_v1.rs::studio_t3_lifecycle_rehearsal_clone_fold_vs_claim_rows` — warm-up 2、本計測 20（1M だけ 10）。
  - 既存の監査テスト 3 本もそのまま実行した。
- fence（int-12 出荷形、DAA 13,850）: `palw_audit_2026_09_11`=0、`palw_held_close_chunks_v1`/`palw_gen_v1`/`palw_improvement_v1`=5300、`palw_tir_v1`=2000 が LIVE。`palw_court_capacity_v1` と `palw_court_transport_v1` は None。つまり t12 では court 予算が効かず、枠は `PALW_COURT_CLOSE_MAX_PER_BLOCK` だけで、しかも適用された move しか消費しない（AC-SLOT）。LIVE は 135 件。

## 1. L-COURT-01 floor（µs、median / p95 / max；r1 load≈27、r2 load≈45）
| bytes | chunks | r1 | r2 | ns/B（median） | digest（r2 median） |
|---:|---:|---:|---:|---:|---:|
| 1,071 | 2 | 4.5/7.2/14.0 | 4.5/8.0/16.1 | 4.2 | 3.3 |
| 16,071 | 2 | 17.9/20.8/22.0 | 43.9/155.0/1341.5 | 1.11/2.73 | 39.7 |
| 128,071 | 3 | 140.1/141.3/148.8 | 142.6/146.1/151.2 | 1.09/1.11 | 130.3 |
| 512,071 | 7 | 556.8/566.5/602.4 | 556.5/645.7/745.3 | 1.09 | 516.2 |
| 1,000,071 | 12 | 1117/1132/1168 | 1110/1143/1149 | 1.11 | 1007.7 |
| 2,000,071 | 22 | 2281/2386/2600 | 2269/2335/2342 | 1.13 | 2018.8 |
| 2,700,007 | 29 | 3129/14662/77155 | 3156/3192/3194 | 1.16 | 2720.6 |
- 64 KB 以上は ≈1.1 ns/B で線形。そのうち約 90% が keyed digest。組立 ≈0.05、decode ≈0.07、再 encode ≈0.03 ns/B（以前の「組立 0.72 ns/B」は digest を含まない値と思われる）。
- 既存の監査テスト（20 回平均）: 100 KB 112–114 µs、1 MB 1.13–1.59 ms、2.7 MB 3.46–3.82 ms。
- r1 の 2.7 MB の p95/max と r2 の 16 KB の外れ値は負荷による preemption。median は 2 run で一致している。

## 2. L-COURT-01 裁定（実 fixture、Gen held-close、3 chunk、µs、median / p95 / max）
| ケース | bytes | 結果 | floor | adjudicate r1 | r2 |
|---|---:|---|---:|---:|---:|
| 名指しした leaf での有罪 close | 3,989 | Ok(ExecutorGuilty) | 5.6/5.7/6.2 | 236.5/237.9/243.0 | 239.9/246.2/248.3 |
| 別 leaf の close（拒否） | 3,925 | Err(leaf 91 ≠ 90) | 5.5/6.7/6.8 | 229.7/237.9/238.7 | 231.8/240.2/240.2 |
| 正直な tile を guilty と宣言（不一致で拒否） | 3,989 | ChallengerDefeated ≠ 宣言 | 5.6/5.7/6.2 | 235.7/244.8/245.3 | 236.1/245.5/245.6 |
- 拒否される close でも、裁定は全経路と同じだけかかる。4 KB では floor の約 40 倍。
- **未計測:** 100 KB〜2.7 MB の close の裁定時間（fixture が 4 KB しかない）。裁定は byte に比例しないので外挿しない。`max_close_bytes` は 16,819,992 で、carriage の天井 2.7 MB より大きい。

## 3. L-LC-01/02（µs、median / p95 / max）
| claim 行 | per-object r1 | per-object r2 | clone r1 | empty fold r1 |
|---:|---:|---:|---:|---:|
| 262（t12） | 16.7/17.2/30.0 | 16.7/17.0/30.9 | 14.8/15.2/63.1 | 20.8/21.3/23.3 |
| 1k | 58.3/63.9/67.4 | 59.5/59.8/60.0 | 56.1/56.6/58.2 | 64.7/65.7/74.1 |
| 10k | 1051/1126/1923 | 1206/1520/1536 | 1072/1174/1180 | 1111/1166/1186 |
| 50k | 8052/8493/8566 | 8185/33830/103302 | 8304/28123/100980 | 11114/41325/58142 |
| 200k | 44476/96089/133462 | 70281/130057/158974 | 29342/71566/119535 | 52783/109412/129598 |
| 1M | 302708/348285/348285 | 348504/425976/425976 | 295278/351447/351447 | 352088/390548/390548 |
- 成長: 1k〜1M の log-log 傾きは per-object 1.24/1.27、clone 1.22/1.27、fold 1.26/1.28。本質は O(N) の全 state 複製（rehearsal ≈ clone）で、1 を超える分は cache の効果。1 行あたり 1k で ≈0.06 µs、1M で ≈0.30–0.35 µs。
- 1M 行で最大 RSS 3.28 GB。
- probe の world は claim 行だけなので、t12 の実 state（bond・class なども含む）に対しては下限。
- 既存の監査テストも再現した（1k 61 µs、10k 1.03 ms、50k 8.15 ms、50 倍で ×133）。

## 4. t12 規模の見積り（前提: 1 tx に object 1 個、carrier 約 7.5 KB の ML-DSA-87 入力 1 個、transient mass 4×bytes、block mass 500k → 約 16 tx/block。計測ではない）
| 経路 | 1 tx | block k=16 | k=100 |
|---|---:|---:|---:|
| L-LC、262 行 | ≈17 µs | ≈0.27 ms | ≈1.7 ms |
| L-LC、200k 行 | 44–70 ms | 0.7–1.1 s | 4.4–7 s |
| L-LC、1M 行 | 0.30–0.35 s | 4.8–5.6 s | 30–35 s |
| L-COURT、2.7 MB group（floor 3.16 ms + 裁定 0.24 ms 以上） | ≥3.4 ms | ≥54 ms | ≥340 ms |
| L-COURT、4 KB group | ≈0.25 ms | ≈4 ms | ≈25 ms |

## 5. 攻撃者の料金と node 作業
- L-COURT（mempool）: 攻撃者が払うのは group 1 個の宣言料と 27 chunk の carriage（chunk 1 個 1,250,000 sompi、27 個で ≈0.34 MSK、未配達なら同額の deposit を失う。code の定数から算出）。その後 4×27 = 108 DAA の間は再送が無料・無制限で、受信した各 node が 1 本あたり ≥3.4 ms を払う。1 MB/s の peer 1 本で 1 コアの約 45%。拒否 tx は relay されない。ban・rate limit の有無は未確認。
- L-COURT（block）: 攻撃者が miner なら同じ completion を k 本入れる。object は捨てられるが block は残り、料金は自分に戻る。全 node が k × 3.4 ms を払う。
- L-LC: t12 の今の規模では無害。ただし費用は state の行数に比例し、料金は比例しない。mempool の H-1 gate と template の再検査でも同じ複製が起きる。
- 修正の方向（拒否された completion で group を消費する・料金を取る、object ごとの複製をやめる、など）は設計判断なので、ここでは提案にとどめる。

## 6. 再計測（6 コア / 11 GB の x86_64 Linux VPS 用）
```bash
git fetch origin "+refs/heads/studio/t3-liveness:refs/remotes/origin/studio/t3-liveness"
git checkout -b t3-rerun origin/studio/t3-liveness
bash docs/evidence/palw/studio-t3-liveness-2026-10-22/build.sh
bash docs/evidence/palw/studio-t3-liveness-2026-10-22/run.sh docs/evidence/palw/studio-t3-liveness-2026-10-22/raw/build-Linux-<ts>.log VPS-x86_64
```
- 環境変数で LOAD_MAX、WAIT_MAX_S、STUDIO_T3_RUNS、STUDIO_T3_SIZES を変えられる。
- run.sh は batch ごとに uptime・hardware・sha256 を前後に記録し、最初に load1 < 5 を最大 20 分待つ。下回らなければ UNDER_LOAD と記録して続行する。
- 1M 行は RSS ≈3.3 GB。RSS 行には GNU time が必要（`apt install time`）。

## 7. raw log
`raw/`: 6 種類 × 2 run（`*-MacStudio-M1Max-2014*`、`*-run2-2035*`）と `build-Darwin-20261022T192112.log.gz`。
