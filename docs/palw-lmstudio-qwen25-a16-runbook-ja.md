# LM Studio から Qwen2.5-1.5B-Instruct · W8A16 static PTQ でブロック生成まで — 運用手順

**これは何か。** LM Studio でダウンロードした Qwen2.5-1.5B-Instruct を、PALW の dense クラス
（W8A16 static PTQ、`.palwart`）に変換し、**ブロック生成まで**持っていく手順。従来の
`qwen25-convert` は HF チェックポイント（`model.safetensors` + `config.json` +
`tokenizer.json`）だけを読んだが、LM Studio が保存するのは **GGUF**
（`~/.lmstudio/models/<publisher>/<repo>/<file>.gguf`）であり、その間に道がなかった。
本ブランチで `qwen25-convert` は GGUF を直接読む：ファイルを一度読み、HF チェックポイントを
合成し（`misaka-palw-base0/src/lmstudio.rs`）、以降は従来と**同一のコード**が走る。

**先に結論の表。** どの入手形態で何ができるかは、キャリア（重みの運搬形式）だけで決まる：

| LM Studio での入手 | 変換 | チャット | ブロック生成 drill / 自前 genesis / 新規登録 | **testnet-11 の genesis QWEN25-A16 で produce** |
|---|:-:|:-:|:-:|:-:|
| GGUF `Q4_K_M` / `Q5_K_M` / `Q6_K` / `Q8_0`（通常のカタログ） | ✓ | ✓ | ✓ | **✗（原理的に不可）** |
| GGUF `F16` / `fp16` | ✓ | ✓ | ✓ | ほぼ確実に ✗（丸め再発生。digest で判定） |
| GGUF `BF16`（あれば） | ✓ | ✓ | ✓ | ✓ 見込み — **digest 一致が唯一の判定** |
| MLX の bf16 ディレクトリ（`model.safetensors` 同梱） | ✓（従来のディレクトリ経路） | ✓ | ✓ | 同上 — digest 一致なら ✓ |
| （LM Studio 外）HF の元チェックポイント | ✓ | ✓ | ✓ | ✓ — pin はこの変換から作られた |

理由は一行で言える：**クラスの同一性は重みのバイト列から導かれる。** 量子化された GGUF は
「同じモデル」ではなく「近い別の重み」であり、変換すれば動く・忠実・裁定可能なクラスになるが、
その `artifact_root` は genesis が pin した
`c00faa480f2344d4a737e5b2e87ab6064d8d6e42c1ffeb6aa0a14ed62134299a7c9dc08f15342cefca1e29390810e6d2c5879f4c3853ebe43a9e2d47ed57ba17`
（`PALW_RC_GENESIS_QWEN25_A16_ARTIFACT_ROOT`）には**決して**一致しない。ツールは変換のたびに
どちら側に居るかを印字するので、digest の不一致が最初の症状になることはない。

対応キャリア：`F32` / `F16` / `BF16` / `Q8_0` / `Q4_K` / `Q5_K` / `Q6_K`
（`Q4_K_M`・`Q5_K_M`・`Q6_K`・`Q8_0` という配布名はこれらの組合せ）。それ以外
（`Q3_K`、`Q2_K`、i-quant、MLX の 4bit/8bit パック形式）は**名前を挙げて拒否**される —
黙って読み違えるより、Q4_K_M 以上で取り直してもらう方が安い。

---

## 0. 前提

* このリポジトリのビルド環境（Rust）。モデルの実行に GPU は不要 — エンジンは整数 CPU 実装。
* LM Studio で **Qwen2.5 1.5B Instruct** をダウンロード済み（既定の場所は
  `~/.lmstudio/models/`。旧版は `~/.cache/lm-studio/models/`。別の場所に置いた場合は
  `LMSTUDIO_MODELS=/path/to/models` で教える）。
* アーキテクチャ検査は GGUF メタデータの `general.architecture == "qwen2"`。Coder 系
  （Qwen2.5-Coder-1.5B-Instruct）も同じ経路で変換できるが、genesis の pin はあくまで
  素の Instruct のものである。

## 1. 変換 — W8A16 static PTQ

```bash
cargo build --release -p misaka-palw-base0 --bin qwen25-convert

# LM Studio のモデルディレクトリから自動発見（最高忠実度のキャリアを選ぶ）：
./target/release/qwen25-convert --lmstudio --a16 --out qwen25-1.5b-a16.palwart

# あるいはファイル/ディレクトリを直接指す：
./target/release/qwen25-convert ~/.lmstudio/models/lmstudio-community/Qwen2.5-1.5B-Instruct-GGUF/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf \
    --a16 --out qwen25-1.5b-a16.palwart
```

`--lmstudio` は引数に検索語も取れる（`--lmstudio "qwen2.5 coder 1.5b"`）。既定は
`qwen2.5 1.5b instruct`。候補を全部印字した上で、キャリアの忠実度順
（bf16 → f16 → q8_0 → q6_k → q5 → q4）で先頭を選ぶ。

変換は二相：まず **float 参照実装が較正プロンプト（実文 57 トークン）を前方実行して各サイトの
振幅を測り**（これが static calibration）、次に測った範囲を整数トリプル `(m, shift, zero)` に
凍結して int8 重み＋A16 パラメータ表を書く（これが PTQ）。出力の読み方：

```
gguf carrier  F32×114 Q4_K×170 Q6_K×28 — QUANTIZED: different weights …   ← キャリアの申告
…
a16 top-1 agree      NN/57      ← 忠実度ゲート（float 参照との一致）
a16 FAITHFUL         true       ← これが false の変換は使わない
a16 artifact  <128 hex>         ← このクラスの identity
a16 pin       MATCHES … / does not match …   ← testnet-11 の genesis で produce できるか
a16 written   qwen25-1.5b-a16.palwart
```

書き出し直後にファイルを読み戻して digest を再検証するので、「書けたが読めない」は
その場で落ちる。参考：HF チェックポイント（2.9 GiB 読み）からの変換は参照実行込みで
リファレンス機（M4 Pro）で数秒〜十数秒、出力 1.7 GiB。GGUF 経路は先頭にファイル読みと
チェックポイント合成（+数 GiB の RAM）が乗る。

## 2. 変換物を先に信用しない — チャットで確かめる

```bash
cargo build --release -p misaka-palw-base0 --bin base0-chat

# LM Studio 経路には tokenizer.json が無い。語彙とマージ表は GGUF ヘッダ内に居るので、
# 変換に使ったのと同じファイルを渡す：
./target/release/base0-chat --artifact qwen25-1.5b-a16.palwart \
    --gguf ~/.lmstudio/models/…/Qwen2.5-1.5B-Instruct-Q4_K_M.gguf \
    --prompt "What is the capital of France? Answer in one sentence."
```

`tokenizer.json` を持っているなら従来どおり `--tokenizer` でもよい。二つは同じ語彙の
二つの運搬形式である。

## 3. ブロック生成 — drill（コンセンサスが受理するところまで）

チェーンを立てる前に、**本物のコンセンサス検査一式**を通してブロックが受理されることを
確認できる。genesis はこの artifact の root を名指しし、canonical job（14 prefill + 2 decode）を
`A16Engine` が実行し、tiled logits でコミットし、クラスの宝くじを引き、ML-DSA-87 で署名し、
UTXO tip に入る：

```bash
MISAKA_QWEN25_A16_ARTIFACT=/path/to/qwen25-1.5b-a16.palwart \
  cargo test --release -p kaspa-consensus real_qwen25_a16 -- --ignored --nocapture
# → "dense drill: block <hash> accepted" が成功の一行
```

これは量子化キャリア由来（pin 不一致）の artifact でも通る。**drill の genesis は手元の
artifact root を登録するからで、これが「pin と一致しなくてもクラスとしては本物」の意味**である。

## 4. testnet-11 で produce する

§1 の出力が **`a16 pin MATCHES …`** だった場合のみ、公開網の genesis QWEN25-A16 クラスで
produce できる。bond・鍵・支払先は [testnet11-join-mining.md](testnet11-join-mining.md) §1–§4 の
とおり整えた上で：

```bash
kaspad --testnet --netsuffix=11 \
  --palw-produce --palw-panel \
  --palw-class-artifact=/path/to/qwen25-1.5b-a16.palwart \
  --palw-producer-class=<QWEN25-A16 の 128-hex class id> \
  …（bond・鍵・支払関連のフラグは join-mining §4 と同一）
```

class id は `kaspad --testnet --netsuffix=11 --palw-dump-classes` が印字する（chain が
登録した表そのものが正）。ノードは起動時に artifact の root を**計算**し、chain の登録と
照合し、不一致は root を挙げて拒否する — pin 不一致の artifact をここに渡しても、黙って
別クラスの仕事をすることはない。

pin が一致しなかった場合の選択肢は二つ：

1. **pin を再現できる素材に替える。** 確実なのは HF の元チェックポイント
   （`model.safetensors` / `config.json` / `tokenizer.json` のディレクトリを
   `qwen25-convert <dir> --a16`）。LM Studio 内で完結させたいなら BF16 GGUF か MLX の
   bf16 ディレクトリが候補だが、**判定は §1 の digest 行だけ**である（f16 は bf16 より
   指数域が狭く、再丸めが起き得るため「ほぼ確実に不一致」側）。
2. **その artifact を自分のクラスとして登録する。** post-genesis 登録
   （`--palw-register-class`、bond 署名と手数料 outpoint が要る）は最小 share（1‰）で入り、
   ADR-0054 の成長規則で産出に応じて伸びる。量子化キャリア由来のクラスも court で裁定可能な
   一級市民である — ただし genesis が資金した 200‰ の席はあくまで pin されたクラスのもの。

## 5. モデル無しでレーン全体を素振りする（開発者向け）

1.5B のダウンロード前に、GGUF → 変換 → drill の配管だけを数秒で通せる：

```bash
# Qwen2.5 の構造（GQA・バイアス・二つのノルム）を持つ dev fixture を GGUF で書く
cargo run -p misaka-palw-base0 --example qwen25-gguf-fixture -- /tmp/qwen25-dev-Q8_0.gguf q8_0

./target/release/qwen25-convert /tmp/qwen25-dev-Q8_0.gguf --a16 --out /tmp/qwen25-dev-a16.palwart
#   → 語彙が較正プロンプトを覆わないので "folding token ids (dev-scale checkpoint)" と注記される

MISAKA_QWEN25_A16_ARTIFACT=/tmp/qwen25-dev-a16.palwart \
  cargo test --release -p kaspa-consensus real_qwen25_a16 -- --ignored --nocapture
```

CI はこの経路を常時走らせている：

* `misaka-palw-base0` の `the_gguf_carrier_does_not_move_the_class` — 同一重みを
  HF safetensors / BF16 GGUF / F32 GGUF / （値を正確に運ぶ）Q8_0 GGUF の四形式で与え、
  合成チェックポイントのバイト一致と、W8A16 static PTQ の artifact digest 一致を主張する。
  「キャリアはクラスを動かさない」の実測形。
* `kaspa-consensus` の `palw_rc_an_lmstudio_gguf_conversion_produces_a_block_the_chain_accepts` —
  GGUF（あえて lossy な Q8_0）から静的 PTQ を経て、実 consensus がブロックを受理するまで。
  身代わりは**重みだけ**で、リーダ・合成・較正・変換・エンジン・コミットメント・全検査は
  本物（qwen36 の dev-fixture テストと同じ規律）。

## 6. つまずきの言語化

| 症状 | 意味 | 出口 |
|---|---|---|
| `general.architecture is "llama" …` | Qwen2 の GGUF ではない | Qwen2.5-1.5B-Instruct の GGUF を取り直す |
| `tensor blk.0.attn_q.weight has ggml type 11 …` | 未対応 quant（例: Q3_K） | Q4_K_M 以上で取り直す |
| `the file carries output.weight …` | lm_head が untied な別モデル | 1.5B（tied）を使う。7B などは別クラス設計が要る |
| `rope.freq_base … has no pinned rotary table` | θ が 10000 / 1000000 のどちらでもない | そのモデルは現行の rotary 表では表現できない |
| `folding token ids (dev-scale checkpoint)` | 語彙が較正プロンプトより小さい（fixture 等） | dev スケールでは正常。実モデルで出たら入力が壊れている |
| `a16 pin does not match …` | 量子化キャリア由来（想定どおり） | §4 の二択 |

## 7. この手順書が言っていないこと

* 量子化 GGUF 由来のクラスの**実モデルでの**忠実度スコアは、このリポジトリではまだ測っていない
  （fixture では FAITHFUL、実測は §1 が変換のたびに印字する）。ゲートを落とす変換を登録する
  理由はどの道ない。
* BF16 GGUF / MLX bf16 が pin を再現するかは、**digest 一致でだけ**主張できる。§1 の出力が
  唯一の判定であり、この文書はそれを先回りして約束しない。
