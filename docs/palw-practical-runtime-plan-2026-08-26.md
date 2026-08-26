# 実用 LLM ランタイム計画 — 検証で一致する Qwen 実行系

目標: **llama.cpp のような実用ランタイムを、GPU を介しても検証でビット一致するように作る。**

llama.cpp が構造的にできないことを狙っている。float 実行では lane 分割・スレッド数・タイル形状・
FMA の縮約が全部答えを変えるので、二台のマシンが一致することを約束できない。だから float 系の
検証方式は reduction order とスレッド数と FMA ポリシーを全部 pin する羽目になる（Gensyn RepOps、
Thinking Machines の batch-invariant kernel は約 34〜61% のスループットを払っている）。
**整数実行はこのカテゴリごと消す** — ADR-0040 Decision E。

## 出発点（2026-08-26 実測、M4 Pro / 24 GB）

`cargo run --release -p misaka-palw-base0 --example base0-throughput -- 28 16 <tier>`

Qwen2.5-1.5B 実寸（28層・hidden 1536・ffn 8960・GQA 12/2・vocab 151,936、重み 1.65 GiB）:

| 段階 | ms/token | tok/s | GMAC/s | 備考 |
| --- | ---: | ---: | ---: | --- |
| W8A8 スカラー（着手前の BASE-0） | 458 | 2.18 | 3.35 | 単一スレッド |
| W8A16 スカラー（A16 tier 移植直後） | 439 | 2.28 | 3.52 | i64 累算は scalar bound では無料 |
| **W8A16 + NEON + rayon**（現状） | **33** | **29.9** | **46.1** | **13.5×**、ビット一致を assert 済み |

参考: メモリ帯域からの上限は CPU 実測ベースで ~70 tok/s、M4 Pro の unified 273 GB/s まで使えれば
~165 tok/s。**現状はまだ帯域の 1/6 しか使っていない**ので、GPU 以前に CPU 側にも伸び代がある。

## 現在の構成

| 層 | 実体 | 状態 |
| --- | --- | --- |
| 算術仕様 | ADR-0040（int8 tier）+ ADR-0047（A16 = W8A16 tier） | 凍結、KAT 17,881 ベクタで外部化済み |
| 参照実装 | `consensus/core/src/palw_base0{,_ops,_a16}.rs` | 裁定器が走らせるコード |
| 第 2 実装 | `misaka-palw-base0-ref2`（構造独立）+ vendored gemmlowp（著者独立） | 差分テスト済み |
| エンジン | `misaka-palw-base0/src/engine{,_a16}.rs` | A16 は本流へ移植済み |
| **高速カーネル** | `misaka-palw-base0/src/kernels.rs` | **NEON + rayon、本項で新設** |
| GPU | なし（Family M は pinned llama.cpp の Metal で、float ゆえ裁定不能） | **未着手** |

## 高速化が「安全である」根拠

`kernels.rs` が置き換えるのは 2 つの射影（`MatMulRequant` / `MatMulRescale`）だけで、
ビット一致は 3 段で assert している:

1. **ベクタ vs スカラー** — 16 要素ブロックと 512 要素チャンクの前後全アラインメント
2. **各射影 vs カタログ op** — 実ジオメトリの長さ（1536 / 8960 を含む）× コード両端
3. **前方伝播まるごと vs カタログ版エンジン** — 残差ストリームと KV cache を通しても一致

### 唯一のコスト: Decision E の前提

Decision E（順序自由）は**溢れないこと**が前提。参照は i64 で累算するので余裕があるが、
SIMD は i32 レーンで累算したい。`|w| ≤ 128`、`|x| ≤ 32767` なので 1 積が 4.19e6 に達し、
**i32 レーンは 128 項しか持てない**。よって縮約を 512 要素チャンク（4 レーン × 128 項、
4 倍のマージン）で切り、チャンク和を i64 に広げてから合算する。
これは参照の左畳み込みとは別の結合順序であり、**同じ数**になる。

チャンクを間違えても**リリースでは無言で wrap する**。`const _: () = assert!(...)` で
ビルド時に止め、上の差分テストが最後の砦。

## 次の段階

### 1. CPU の残り（帯域まで）
- `dot_i8_a16` は現状 `vmlal_s16`（1 命令 4 MAC）。`usdot`（FEAT_I8MM、Apple M2+）が使えれば
  x を上位/下位バイトに分解して 2 パスで **1 命令 16 MAC**。分解は代数的に厳密なので値は不変:
  `Σw·x = 256·Σw·hi + Σw·lo`、各パス上界 `8960×128×255 = 2.9e8 < 2^31`
- 重みのタイル化と prefetch（現状は素直な row-major 走査）
- attention アーム（`a16_attn_scores` / `a16_attn_values`）はまだスカラー

### 2. Metal バックエンド（決定済みの GPU 第一目標）
- Metal に `dp4a` 相当は無いので自前 MSL の i32 mad ループ。**整数なので bit-exact は保証される**
- threadgroup ごとの縮約も同じチャンク規則に従わせる（i32 レーンの 128 項制約は GPU でも同じ）
- 検証: CPU 参照との全前方一致を、GPU 実行の受領証について実行する

### 3. 実重み（品質）
- `qwen25-convert` の A16 PTQ 経路は `palw-mainnet-rc-integration` 側にあり、本流へは未移植
  （`convert.rs` の +464 行）。ここまでは**速度のみ**を測っており、品質は一切測っていない
- 現在の fixture はスケールを fan-in から導いただけの非較正値。実重みでの perplexity は未測定

### 4. court 側の reconcile（実行とは独立）
- `palw_step_refute`（+1147 行）と `palw_qwen25_profile`（+967 行）が両ブランチで動いており、
  A16 の裁定経路を本流に持ってくるのは別作業。**走らせるのに必要ではない**ので後回しにした

## 移植で踏んだ罠（再発防止）

1. **A16 ブランチの全マージは禁止。** `palw_admission_v2` の衝突が、b57adc83 で
   「走っているチェーンには出せない」として意図的に revert された mid-epoch budget 導出を
   引き戻す内容だった。40 commit を一括で入れると consensus が巻き戻る
2. **`a16_params` を上流のまま入れると全 class id が変わる。** 上流は他の optional field と同じく
   `None` に presence byte を吸わせている。digest は class id なので、既に登録済みのクラスが
   全部解決しなくなる。上流は再 mint できたが本流はできない。→ **不在なら 1 バイトも吸わない**形に変更し、
   フィールド追加前に実測した digest を pin して証明
3. **`rustfmt <lib.rs>` はモジュール木ごと整形する。** さらに `--edition 2021` を渡すと
   import 順が 2021 様式に書き換わり、無関係な 9 ファイルが差分に出た。
   lib.rs には掛けない・edition は 2024
4. **fixture の退化は差分テストを素通りする。** 全サイト同一ゲインの store は logit を全ゼロにしたが、
   高速版と参照版は「両方ゼロ」で一致するので差分テストは緑だった。
   捕まえたのは「別のトークンは別の行を出すか」という非退化テスト
