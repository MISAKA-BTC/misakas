# R7 — PALWQ361 の logit scale と f32 参照窓（目安・未実測、2026-10-22）

- **状態:** 目安（未実測）。承認は **ユーザー判断 2026-10-22（目安で可）**（35B MoE の f32 長系列実行が 32 GiB の機材に収まらないため）。実測の経路（`--palwq36-logit-scale` と 32K 全長の `--palwq36-reference-windows`）は変えておらず、こちらが優先。
- **branch:** `values/r7-estimate`（base `g14/integration-r7` 49d65113e）。worktree `/Users/wata/Downloads/MISAKA-r7`。
- **機材:** Mac Studio（Apple M1 Max、32 GiB）。**この lane では 35B の変換・参照・比較を一度も実行していない。** 下のメモリ量と時間は、コードを読んで出した見積りで、測定値ではない。
- **前提の禁止事項:** 計算の暗号学的証明は使わず、提案もしない（PRINCIPLES §8）。モデル単位の検証・審査は提案しない（§9）。ここでの fit は変換品質の検査で、合意上の検証ではない。

## 0. 決めた値

| 台帳キー | 値 | 意味 |
|---|---|---|
| `palwq361_logit_scale_estimate` | **2^-9 = 0.001953125** 物理 logit / 整数 code | `e_logits = 9` を仮定（較正 logit の absmax A が (16, 32] にある場合） |
| `palwq361_reference_prefix_tokens` | **2,048**（上限 4,096） | 縮小した f32 参照: held-out 列の先頭 L token。64 token の窓 32 個でちょうど敷き詰められる |
| `palwq361_estimate_scale_spread_log2_max` | **0.25**（倍率 2^±0.25 ≈ ×0.84〜×1.19） | 窓の実データから出した scale と目安が、これより離れていれば拒否 |
| `palwq361_estimate_kl_margin` | KL の閾値 **×0.5**（0.1 → 0.05、drift floor 0.01 → 0.005）、late/early 比 **≤ 1.25**（A16 の基準は 1.5） | prefix 参照で見えない 32K までの drift に対する余裕 |

## 1. logit scale の導出

### 1.1 PALWQ361 では scale は作りで決まる（入力は 1 つの整数）

- `qwen36-convert` は出力ヘッドを次のように書く（`misaka/palw/base0/src/bin/qwen36-convert.rs` の head 部分）。
  - `output.weight` は Q8_0 の GGUF を f32 に戻し、出力行ごとに 32 重みの group と 2 の冪の exponent で int8 に量子化する（`quantize_rows_grouped`。行の scale `s_c = 最大 group の peak / (127 · 2^20)`）。
  - `final_norm.a16 = norm_params(γ, e_final)`、`output.weight.a16 = projection_params(s_c, e_final, e_logits)`。`projection_params` の利得は `s_c · 2^(e_logits − e_final)`（`qwen36_calibrate.rs`）。
  - したがって整数 logit code は **`code = logit · 2^e_logits`**。物理的な logit scale は **`2^-e_logits`** で、近似ではなく定義どおりの値になる。
- `e_logits = site_exponent(A) = floor(log2(32767 / A)) − 1`（`A16_CODE_MAX = 32767`、`HEADROOM_BITS = 1`）。A は、変換器が持つ f32 参照の「logits」site の absmax で、8 token の較正 prompt（`9707, 11, 1879, 0, 3555, 374, 279, 6722`）の 8 位置 × 語彙 248,320 から取る。
- 分からないのは A がどの 2 の冪の区間にあるかだけ:

| A の区間 | e_logits | scale |
|---|---|---|
| (8, 16] | 10 | 2^-10 = 0.0009765625 |
| **(16, 32]** | **9** | **2^-9 = 0.001953125（目安）** |
| (32, 64] | 8 | 2^-8 = 0.00390625 |

### 1.2 A16 の値との関係

- A16 の scale は `a16_logit_scale_v1 = 2^(ceil(log2(A/32767)) + 1)`（`misaka/palw/base0/src/convert.rs`）。これは変換器の較正統計（`final_logit_absmax`）から計算する値で、窓で測った値ではない。区間の境界を除けば PALWQ361 と同じ式。
- **A16 の実測値はリポジトリに無い。** `docs/` と `scripts/` を `a16-source-calibration`、`final_logit_absmax`、`logit_scale`、`qwen25-window-compare` で検索しても、実行した記録は見つからない（handover 2026-10-18 には「実行する」という予定があるだけ）。そのため A16 から借りられるのは「scale は較正の absmax から 2 の冪として決まる」という導出の形で、数値は借りられない。
- Qwen3.6 の logit の大きさの記録もリポジトリに無い。`docs/evidence/hf-onboarding/20261010-huihui-qwen35-9b-fit-diagnosis` の site 統計は相対誤差で、大きさではない。
- **目安の根拠:** Qwen 系の次 token logit は、短い英文 prompt では最大でおおよそ 15〜35 になることが多い（一般知識であり、この lane では測っていない）。最も起こりやすい区間 (16, 32] を取って、e_logits = 9 とする。

### 1.3 保守側にする仕組み（目安が外れても fit は通らない）

- 外れた場合はどの方向でも 2 倍の誤差になる。KL(p ‖ softmax(k·z)) は k について凸なので、温度を倍にしても半分にしても、正しい head の KL は大きくなる。ただし、量子化 noise で KL が最小になる k が 1 より小さい場合、scale を小さめに外すと、ずれたモデルの KL が小さく出ることがある。KL だけでは保守側にならない。
- そこで **`qwen36-window-compare` が窓ごとに `logit_scale_spread` を出す**ようにした。`sqrt(Σ(r−r̄)² / Σ(q−q̄)²)` で、行ごとに中心化し、窓内でまとめて計算する。物理 logit の広がりと整数 code の広がりの比なので、変換が正しければ `2^-e_logits` 付近になる。読み手（`palwq36_estimate_fit`）は、全窓で `|log2(spread / 目安)| ≤ 0.25` を満たさなければ拒否する。
  - 目安が 2 の冪ひとつ分外れている場合 → 拒否（fit は止まり、upload されない）。誤って拒否する方向にしか外れない。
  - head が 2 の冪ひとつ分ずれている場合（例: e_final の off-by-one）→ spread が動くので拒否。
- **spread に合わせて scale を選び直す（snap する）ことはしない。** そうすると、head の 2 の冪のずれを scale が吸収して見えなくなる。spread は報告するだけで、KL は常に与えた scale で計算する。
- 拒否されたときに、spread が示す別の 2 の冪で走り直すのは新しい目安の決定であり、記録と承認が要る。その前に、§4 の「変換器の e_logits」で独立に確かめること。

## 2. f32 参照窓（縮小参照）

### 2.1 32 GiB で可能かどうか

- f32 の重み 140 GB を持つ必要はない。`qwen36_reference` は layer 順に流す実装で、ピークは最大の 1 tensor（`ffn_down_exps` 1.07 GiB）と活性値だけ（モジュール冒頭のコメント）。
- 32K で問題になるのは logit の行を保持すること。`Qwen36CalibrationV1.logits` は全位置の行を持ち（32,768 × 248,320 × 4 B ≈ 32.5 GB）、`--reference-logits` の書き出し buffer がもう 1 組ある。合わせて約 65 GB で、32 GiB では足りない。
- **prefix L なら収まる:** 行 1 本 ≈ 0.99 MB なので 2 × L × 0.99 MB。L = 2,048 で約 4.1 GB、L = 4,096 で約 8.1 GB。これに URL 窓の cache（24 × 64 MiB = 1.5 GiB）、1 tensor（1.07 GiB）、活性値と KV（L × 数 KiB）が加わる。**見積りで、測定ではない。** 計算時間（scalar f32、MoE は token あたり約 3B の積和）も測っていない。
- このため estimate mode の参照は prefix にする。既存の `qwen36-convert --reference-only --prompt <先頭 L 個の held-out token> --reference-logits FILE`（変換器の f32 参照そのもの。測定 pass と同じ GGUF 読み取りと検査）を使い、`write_reference_windows_from_logits` が `misaka-palw/qwen36-reference-windows/v1` に切り出す（行は複製するだけで、再計算も丸めもしない）。そのうえで `qwen36-window-compare` が整数 engine を同じ L token で進めて比べる。窓は `a16_window_starts(L, 32)`。L = 2,048 では 0, 64, …, 1,984 となり、全位置を比べる。

### 2.2 余裕（margin）

- prefix 参照の判定基準は A16 の基準（`genesis8_models.json` の `fit_tolerance`）より厳しくした。KL の 3 つの閾値は ×0.5、late/early 比は 1.25、top-1 は 0.8 のまま（`palwq36_estimate_tolerance`。どの項目も元の基準より緩くはならない）。
- 窓が 32K 全長の参照（別の機材が作ったもの）で scale だけが目安の場合は、A16 の基準と spread 検査を使う。

## 3. 保証しないこと

1. **logit scale は測っていない。** A が (16, 32] の外にあれば spread 検査で拒否される。誤って通るのではなく、fit が止まる。
2. **残る危険:** head が 2 の冪ひとつ分ずれていて、目安もちょうど同じ向きに同じだけ外れている場合は、両方の検査を通る。目安を spread に合わせて変えない限り起こらない（§1.3）。完全に閉じるには §4 の変換器の e_logits が要る。
3. **prefix より後ろ（2,048〜32,768 位置）は比べていない。** GDN 状態 exponent の horizon 補正（`horizon_growth_bits`）、長距離 attention、32K までの drift は検査の外にある。prefix の中では合っていて後ろでずれるモデルは、estimate mode では通るが、実測なら落ちる。これが R7（参照窓を目安で決める）を受け入れる代償で、§2.2 の余裕で小さくしているが、なくなってはいない。
4. f32 参照は Q8_0 の GGUF を f32 に戻したもので、元の bf16 の重みではない（実測の経路と同じ）。
5. メモリと時間は見積りで、この lane では一度も走らせていない。
6. upload gate（2026-10-18 ユーザー決定「genesis upload には実測 fit が要る」）は、この R7 承認に限って例外を持つ。`estimated: True` で、class が PALWQ361、approval 文字列が上の承認と一致し、spread が整合し、短い fit と長い fit の両方が基準内のときだけ通る（`palwq36_estimate_refusal`）。fit は `measured: False` として記録され、card にも「ESTIMATE, not a measurement」と書かれる。

## 4. 実測に置き換えるには

- **scale（追加の計算は要らない）:** 変換の測定 pass（`qwen36-convert --reference-only --measured-out`）は、この A と e_logits をすでに計算している（`let e_logits = cal::site_exponent(site(&calibration, "logits"))`）。A16 の `--calibration-report` と同じように `logits_absmax`、`e_logits`、`logit_scale = 2^-e_logits` を書き出せば、scale は目安ではなく、A16 と同じ地位の「変換器の較正から導いた値」になり、§3-2 も閉じる。この lane では変換器に手を入れていない（未実装）。
- **32K 参照:** 64 GB 以上の機材で `--palwq36-reference-windows` を作る。または、窓の位置の行だけを保持する参照 writer を作る（ピークは活性値 32K × 2,048 × 4 B ≈ 256 MiB と 10 層分の KV なので、32 GiB に収まる見込み。計算は O(n²) の attention を含み、日の単位の見積り。未実装）。

## 5. 変更と試験

- `misaka/palw/base0/src/bin/qwen36-window-compare.rs`: 窓ごとの `logit_scale_spread`（`logit_scale_spread_v1`）を追加。KL と既存の出力は変えていない。試験 `qwen36_logit_spread_names_the_power_of_two_and_refuses_a_flat_row`。
- `scripts/genesis/genesis10_pipeline.py`:
  - 定数 `R7_*` と `PALWQ361_*` を追加。
  - `palwq36_estimate_tolerance`、`write_reference_windows_from_logits`、`palwq36_estimate_fit`、`palwq36_estimate_refusal`、`Pipeline.fit_palwq36_estimate`、`palwq36_prefix_tokens`、`q36_source_args` を追加。
  - flag `--palwq36-estimated-scale [S]`（値を省くと台帳の値 2^-9）と `--palwq36-reference-prefix L` を追加。
  - `fit_plan` を estimate mode に対応させた。`--palwq36-logit-scale` との併用は拒否する。
  - `upload_refusal` で R7 の estimate だけを受け付ける。card には「ESTIMATE」と書く。`palwart-record.json` には fit の `estimate` ブロックが入る。
  - 実測の経路（`palwq36_window_fit`、`fit_palwq36`）は変えていない。
- 試験: `scripts/genesis/tests/test_genesis10_pipeline.py::Palwq36EstimateR7`（10 件）。
- 台帳: `docs/governance/parameter-register.md` と `parameters.json` に上の 4 キーを登録。状態は「目安（未実測）」。
