# BASE-0 ランタイム設計レビューの照合 — 2026-08-26

外部レビュー（「決定的整数 LLM ランタイム」の設計文書に対する評価）を、`PALW-BASE-0` の
**実装済みコードと突き合わせた**記録。レビューは設計文書だけを読んで書かれているため、
指摘の多くは既に本リポジトリで決着している。ここでは各指摘に対して
「済 / 部分 / 未」と**証拠となるファイル位置**を付け、残った本物のギャップだけを作業項目にする。

参照ブランチ: `main-safe`（t11 が実際に走っている線）。

## 要約

| # | レビューの指摘 | 判定 | 根拠 |
| --- | --- | --- | --- |
| 1 | HF Transformers との bit-exact 比較は原理的に不可能 | **済**（そもそも HF を正解にしていない） | ADR-0040、`misaka-palw-base0-ref2` |
| 2 | 整数なら `reduction_order = fixed` は不要・有害 | **済**（クラスの中心的性質として明文化） | ADR-0040 Decision E |
| 2b | overflow 境界を仕様に書き CI で assert | **済**（const_assert + 実行時 + shape 検証の三重） | `palw_base0.rs:100`, `palw_base0_ops.rs:114`, `artifact.rs:177` |
| 3 | 本当の敵は MatMul でなく非線形（RoPE/Softmax/RMSNorm/除算） | **済**（4 つとも整数化済み） | `rope.rs`, `palw_base0_ops.rs:264`, `RSQRT_ITERS`, `int_recip` |
| 4 | Validator フル再実行はスケールしない → bisection | **済**（court は二分探索で実装・実機 drill 済） | ADR-0027/0028/0030、`palw_step_refute.rs`, `palw_dispute.rs` |
| 5 | Tokenizer の実装差でチェーンが割れる | **済**（構造的に consensus 外へ出してある） | `palw_v2.rs:404`, `palw_freeprompt_v3.rs:192` |
| 6 | argmax tie-break / PRNG / offline calibration / EOS | **済** | `engine.rs:441`, `convert.rs`, `palw_freeprompt_v3.rs` |
| — | Phase 0: 仕様 + KAT ベクタ + 独立実装 | **部分** — 独立実装は 2 つあるが **KAT がリポジトリ外に出ていない** | 下記 G2 |
| — | コア crate から float を型レベルで排除（lint で deny） | **未** — 規律としては守られているが機械的強制がない | 下記 G1 |

つまり、**レビューの 6 指摘は全て既に決着済み**で、実装価値が残っているのは
レビューが本文でなく付随的に触れた 2 点（KAT の外部化、float の機械的排除）だけだった。

## 指摘ごとの照合

### 1. HF との bit-exact 比較 — 該当しない

レビューの指摘自体は正しい（float 実行と整数実行が bit-exact になることはない）。
ただし本リポジトリは HF を正解に置いていない。正解の三層は既に：

* **仕様**: `docs/adr/0040-palw-base-0-integer-arithmetic.md`（7 プリミティブ + 10 op の閉じた集合）
* **構造的独立の第 2 実装**: `misaka-palw-base0-ref2/src/primitives.rs`
  — 同一著者・別定式化。実際に**欠陥 3 件**を検出している（`SRDHM` の丸め方向、
  RSR の負値、`RoundingShiftRight64` の overflow panic）。
* **著者独立のオラクル**: `misaka-palw-base0-ref2/src/gemmlowp.rs`
  — Google の gemmlowp を byte-identical に vendor。`SRDHM` と `RoundingShiftRight` の
  2 つについては「このリポジトリが自己整合」ではなく「ADR-0040 が**正しい**」ことの証拠になる。

レビューが言う「暗号ライブラリと同じで、正解は参照実装ではなく仕様 + KAT」は、
この 3 層でほぼ実現している。**欠けているのは KAT がファイルとして外に出ていないこと**だけ（G2）。

### 2. reduction order — 既にクラスの中心的性質

ADR-0040 **Decision E** がレビューと同じ結論を、より強い形で述べている：

> 整数加算は正確に結合的かつ可換であり、丸めも飽和もない。**どの順序で積み上げても結果は変わらない**
> — スレッド数・SIMD 幅・タイル形状をまたいで。

しかも Decision E は**条件付きであること**まで書いてある。飽和加算は結合的でないため、
この性質は「accumulate で overflow しない」（C3）を前提とする。よって境界は安全上の配慮ではなく
**性質の前提**として扱われている。

境界の強制は三重：

```
consensus/core/src/palw_base0.rs:110   const_assert!(MAX_DOT_LEN * (-128)^2 <= i32::MAX)  … ビルドが止まる
consensus/core/src/palw_base0_ops.rs:114  dot_i8 が長さ超過を Err で返す               … 実行時
misaka-palw-base0/src/artifact.rs:177  shape 検証が d_head / d_ff / d_model を弾く      … 登録時
```

`MAX_DOT_LEN = 131_071`（= `i32::MAX / 16_384`）。レビューの Qwen2.5-1.5B の見積り
（`127 × 127 × 8960 ≈ 1.445×10⁸ < 2³¹`）は、本リポジトリがより保守的に `128² = 16_384` を
使っている点を除けば同じ結論。K が大きいクラスで `i64` へ切り替える判断も ADR に明記済み。

**レビューより repo の方が精密な点**: レビューの `saturation = "forbidden"` は本クラスと異なる。
BASE-0 は **accumulate では overflow 不能、narrowing では飽和**（C3）。narrowing の飽和は
値域を閉じるために必要で、禁止すると `i32 → int8` が定義できない。

`artifact.rs:170` 付近には、この境界を「あらゆる次元」に適用していた過去の誤りの記録もある
（`vocab` は reduction 長ではなく出力幅なので、131_071 で縛ると実在する語彙が全て弾かれる）。

### 3. 非線形 — 4 つとも決着済み

| レビューの懸念 | 実装 |
| --- | --- |
| RoPE で `sinf/cosf` を呼んだら終わり | `misaka-palw-base0/src/rope.rs` — 事前計算した固定小数点テーブル。ADR-0040 が catalog から `sinf/cosf` を**削除**し、ADR-0031（canonical transcendentals）を「持たないことで」supersede している |
| Softmax の online rescaling は結果を変える | `palw_base0_ops.rs:264` — max → exp → sum → reciprocal の 2-pass 固定。flash 系の逐次 rescale は存在しない |
| RMSNorm の rsqrt は反復回数を固定するか LUT | `palw_base0.rs:61` `RSQRT_ITERS: u32 = 3` — 固定反復 + 16 エントリの seed テーブル。収束判定で抜ける経路はない |
| 除算は全面禁止 | `int_recip` で置換済み。softmax も sigmoid も逆数経由（`palw_base0_ops.rs:279`, `:296`） |

整数 exp はレビューが挙げた I-BERT 系の 2 次多項式近似そのもの
（`POLY2_A/B/C`, `palw_base0.rs:50`）。

### 4. 検証フロー — bisection は実装済み

レビューの「`O(validator数 × フル推論)` はスケールしない → Verde 型 bisection」は、
ADR-0027/0028 の court として既に実装されている：

```
consensus/core/src/palw_step.rs         17 種の op 語彙とステップ空間
consensus/core/src/palw_step_leg.rs     leg（争点を絞る単位）
consensus/core/src/palw_step_refute.rs  1 ステップまで絞った後の反証
consensus/core/src/palw_dispute.rs      紛争の状態機械
consensus/core/src/palw_adversarial.rs  敵対ケース
```

trace 粒度についても、レビューが「Phase 5 で必ず設計に入れろ」と言っている
**checkpoint / leg の二段構え**が既にある（checkpointed full-dispute は実機 drill 済）。
TEE には依存していない（PQC を掲げる以上 TEE は筋が悪い、というレビューの判断とも一致）。

### 5. Tokenizer — 構造的に consensus の外

レビューの懸念（pretokenizer regex の実装差、BPE マージ順の tie-break、`tokenizers` crate の
バージョン更新でコンセンサスが割れる）は、**token 列を束縛して文字列を束縛しない**ことで消してある：

* `palw_v2.rs:404` — worker はこの経路でテキストを tokenize も normalize も template も**しない**
* `palw_freeprompt_v3.rs:195` — job が運ぶのは `prompt_token_ids_hash`
* `palw_freeprompt_v3.rs:192` — `tokenizer_id` はクラス行と一致必須。
  「同じバイト列を別の tokenizer で読めば別のプロンプト」だから交差確認として運ぶ

残る影響は **UX のみ**（gateway が違えば同じ文字列が別 id 列になりうる）で、フォークにはならない。
チェーンが見るのは id 列のハッシュだけ。

### 6. 細かい点

* **argmax tie-break**: `engine.rs:441` `argmax_lowest` — 最小 token id。テストで固定（`:604`）
* **`temperature > 0`**: 存在しない。BASE-0 の decode は greedy のみ。よって PRNG の凍結は不要
* **量子化 calibration**: offline 確定。`convert.rs` が唯一の float 使用箇所で、
  これは PTQ パイプライン（登録前）であって推論経路ではない
* **EOS / budget**: job envelope に束縛（`palw_fp_execution_v3.rs:127`）

### 実装ベースの推薦について

レビューが挙げた土台のうち、既に採用済み：

* **gemmlowp** — vendor 済み（`misaka-palw-base0-ref2/src/gemmlowp.rs`）。
  レビューの「BASE-0 をゼロから定義するより gemmlowp を出発点に」は、
  `SRDHM` / `RoundingShiftRight` について**そのまま実行されている**
* **I-BERT 系の整数非線形** — 2 次多項式近似として実装済み
* **カーネル** — `backend.rs` / `optimized.rs` / `kernels.rs`

採用しない：**candle / tract / burn**。BASE-0 は 10 op の閉じた catalog で自己完結しており、
テンソルフレームワークを持ち込むと catalog の閉性（court の前提）が壊れる。
`ggml` / `llama.cpp` は GGUF レイアウトの参考として `misaka-palw-base0/src/mmap.rs` の
コンバータ側にある（`misaka-palw-metal` は ADR-0053 で削除。GGUF を*実行*する経路は無く、
読むだけの経路が残っている）。

## 残っている本物のギャップ

### G1 — float 排除が規律であって強制ではない

BASE-0 の consensus 経路は**実際に float-free**（全ファイルを走査して確認済み）。
`f32`/`f64` の出現は次だけで、いずれも doc コメントか `#[cfg(test)]` 内：

```
misaka-palw-base0/src/rope.rs:315      テスト（整数テーブルが float の三角関数と一致することの確認）
misaka-palw-base0/src/optimized.rs:204 テスト（float なら順序で結果が変わることの実演）
misaka-palw-base0/src/produce.rs:20    doc コメント
misaka-palw-base0/src/convert.rs       offline PTQ（意図的に float、推論経路ではない）
```

しかし**これを守らせる仕組みがない**。ADR-0040 Decision A は「libm をリンクするビルドは
適合実装ではない」と言い切っているが、それを検査するものが存在しない。
レビューの「lint で deny すれば事故の 9 割が消える」はここに当たる。

Rust には「float 型を禁止する」組み込み lint も clippy lint も無いので、
**crate 内のソースを走査するテスト**として実装する（下記）。

### G2 — KAT ベクタがリポジトリの外に出ていない

差分テスト（`misaka-palw-base0-ref2/tests/differential.rs`）は Rust コード同士の比較なので、
**第三者が Rust を読まずに適合実装を書く経路がない**。
ADR-0040 は「二つの独立実装が一致して初めてクラスが登録される」と定めているのに、
その一致を外部が確認する手段が repo の内部形式しかない。

必要なのは、仕様から機械可読な KAT を吐き、そのダイジェストをテストで固定すること。
これはレビューの Phase 0（「仕様 + KAT ベクタ + 独立実装」）で唯一未達の部分。

### G3 — 記録のみ（本作業の対象外）

* Qwen A16 レーン（`engine_a16.rs` / `reference.rs` / `replay.rs`）は
  `palw-mainnet-rc-integration` にあり **`main-safe` に未マージ**
* Q4_K family の adjudicator が無く、coverage gate が正しく拒否するため
  **BASE-0 が唯一 weight-bearing なクラス**

## 実装（このブランチ `palw-base0-runtime-hardening`）

どちらも**追加のみで consensus の挙動を変えない**。走っているチェーンの fingerprint に影響しない。

### G1 — `misaka-palw-base0/tests/float_free.rs`

ADR-0040 Decision A を**検査可能にした**。Rust には float 型を禁止する lint が
組み込みにも clippy にも無いので、crate 自身のソースを走査するテストとして実装した。

* 走査対象: `misaka-palw-base0` の実行経路 11 モジュール + KAT を述べる 2 ファイル +
  `consensus/core/src/palw_base0.rs` / `palw_base0_ops.rs`（プリミティブ本体）
* 検出: `f32`/`f64` の語（`Hash64` や `buf32` は誤検出しない）、
  libm メソッド呼び出し（`.sqrt()` `.exp()` `.sin()` …）、float リテラル（`1..=4` や `x.0` は除外）
* 除外: offline PTQ（`convert.rs` ほか）と `#[cfg(test)]` 以降。
  テストでの float は**意図的に許す** — `rope.rs` は整数テーブルを `f64` の三角関数と照合し、
  `optimized.rs` は `f32` の総和がブロック幅で変わることを実演している。
  ここで float を禁止すると「なぜ整数クラスなのか」の証拠を消すことになる
* `every_source_file_is_classified` — `src/` に増えたファイルが
  「実行する / 変換する」のどちらにも分類されていなければ落ちる。
  実際、この作業中に `kat.rs` を追加した時点で発火した

**発火することを変異テストで確認済み**: `engine.rs` に `-> f32` / `.sqrt()` / `1.5` の
3 種を注入し、3 件とも行番号つきで検出されることを確認してから復元した。

### G2 — `misaka-palw-base0/src/kat.rs` + `bin/base0-kat` + `misaka-palw-base0-ref2/tests/kat.rs`

9 プリミティブ（`RoundingShiftRight`, `RoundingShiftRight64`, `SRDHM`, `Requantize`,
`RequantizeWithZero`, `Rescale`, `IntExp`, `IntRsqrt`, `IntRecip`）の
**17,881 ベクタ**を機械可読形式で出力する。

```bash
cargo run --release -p misaka-palw-base0 --bin base0-kat > palw-base0-kat-v1.json
```

* **乱数を使わない**。境界表（型限界・2 冪とその隣・両符号）、
  アルゴリズムが挙動を変える領域の全数列挙（`IntExp` の range-reduction バケット、
  `IntRsqrt` の seed basin、`IntRecip` が `1..=511` で `i64` を溢れさせていた小さい `v`）、
  そして**実際に欠陥が見つかった入力**（`REGRESSIONS`）だけ。
  seed `0x5EED_0006` は理由にならず、第三者は意図したベクタと偶然のベクタを区別できない
* **digest を pin**: `KAT_DIGEST = d136224b…` を JSON ではなく正準バイナリ符号化に対して計算。
  ベクタや答えを 1 つでも書き換えると落ちる。
  「新しい実装に合わせて答えを直す」ができないことが KAT の存在意義
* **第 2 実装で全ベクタを再生**（`misaka-palw-base0-ref2/tests/kat.rs`）。
  これが無ければ、配るファイルは「クラスについての言明」ではなく
  「実装 #1 が出力したもの」に過ぎない。
  `SRDHM` と `RoundingShiftRight` の 2 群は vendored gemmlowp でも再生している
  （著者独立なので、ADR-0040 が**正しい**ことの証拠になる唯一の 2 群）

`RequantizeWithZero` だけは ref2 に対応関数が無いので、
ref2 自身のプリミティブから `Saturate8(RSR(SRDHM(acc, mult), shift) + zero)` を組んで検証している。
`ref2_requantize` を使い回すと clamp が二重にかかり、
**飽和しないベクタ全てで黙って一致してしまう** — それは zero point が存在する理由そのもの。

### 検証

```
cargo test -p misaka-palw-base0 -p misaka-palw-base0-ref2   # 全 green
cargo clippy -p misaka-palw-base0 -p misaka-palw-base0-ref2 --all-targets   # 警告なし
```

digest は debug / release で一致（整数演算なので当然だが、確認した）。

## 次にやるなら

* KAT JSON をリリース成果物として公開する運用（現在はエクスポータのみコミット、
  JSON 本体は非コミット。digest が凍結の実体なので二重管理を避けた）
* 演算子単位でなく**層単位**の KAT（`rms_norm` / `softmax` / `rope_table` / 1 層の forward）。
  プリミティブが合っていても合成順序で割れる余地は残っている
* G3: Qwen A16 レーンを `main-safe` へマージするか、しない理由を記録する
