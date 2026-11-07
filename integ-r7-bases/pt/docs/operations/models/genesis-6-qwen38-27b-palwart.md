# genesis #6(Qwen3.8-27B)を PALWART3 にして genesis モデルにする手順

対象は genesis リスト(`scripts/genesis/genesis8_models.json`)の **#6** に固定されたモデルです。別の 27B(別の repo、別の revision、別の量子化)を変換しても genesis モデルにはなりません。genesis の 1.0 tier は genesis・年次フォークの catalog に載ったものだけで、catalog には registry に固定された入力から作った PALWART3 の root しか入りません。

| 項目 | 値 |
|---|---|
| 入力 repo | `huihui-ai/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-GGUF` |
| revision | `a0b17c71d9943529aefe88b83f494bf01c6b4d68` |
| ファイル | `Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-bf16.gguf`(54,657,734,080 B) |
| SHA-256 | `fcddc1ff6efa4932cf4a7eb18087be39527b740cdca0810bbd3a717312d61d53` |
| tokenizer(較正用) | `huihui-ai/Huihui-Qwen3.8-27B-abliterated` @ `739e3c5b89849f6c238ce1e5b70008612ae42cdd`(GGUF の語彙と id ごとに照合してから使う) |
| 経路 | TIR(GGUF `qwen35`、Gated DeltaNet hybrid)、family `qwen35-gdn-hybrid`、`--gdn-core-wide` |
| 提供 context | 32,768 tokens(較正も 32,768 tokens の 1 系列) |
| 出力 | PALWART3 1 個(約 29.5 GB、2 GiB の segment に分割) |
| 出力先 | `Misakachain/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-PALW` |
| license | Apache-2.0 |

`mmproj-model-bf16.gguf`(vision projector)は対象外で、取得しません(text 生成だけが対象)。

---

## 1. 機材の選び方

| 方式 | 必要な機材 | 時間の目安(未測定、±50%) | fit(品質検査) |
|---|---|---|---|
| **A. ローカル(推奨)** GGUF を手元に置いて変換 | RAM 64 GB 以上、空きディスク 100 GB 以上、コア数が多いほど速い | 較正 3〜19 h(64 コアで 3〜4 h)+ 変換 1.5〜3 h + fit | **実行できる** |
| B. stream GGUF を HTTPS の range 読みで流す | RAM 32 GB、空きディスク 約 4 GB + 余裕 30 GB | 15〜22 h(M1 Max 32 GB) | **実行できない**(GGUF が手元に残らないため) |

genesis にするには fit の合格記録が要るので、**A を使ってください。** B で作ったものは「fit 未測定」と記録され、そのままでは genesis の根拠になりません。

## 2. 準備

### 2.1 リポジトリ

```bash
git clone https://github.com/MISAKA-BTC/misakas.git
```

```bash
cd misakas && git checkout pre
```

### 2.2 バイナリ(release)

```bash
cargo build --release -p misaka-palw-tir-artifact --bin palwart
```

```bash
cargo build --release -p misaka-palw-tir-lower --features remote --bin palw-tir-convert --bin palw-tir-fidelity
```

`--features remote` は方式 B(HTTPS stream)のときだけ必要ですが、付けておいても問題ありません。バイナリは `target/release/` にできます。別の場所に置いた場合は、pipeline に `--bin-dir` で渡します。

### 2.3 Python(較正 token の作成用)

`tokenizers` が入った Python を用意します。

```bash
python3 -m venv ~/tir-venv && ~/tir-venv/bin/pip install tokenizers
```

pipeline には `--tokenize-python ~/tir-venv/bin/python` で渡します(既定値は開発機のパスです)。

### 2.4 Hugging Face

upload には、`Misakachain` に書き込める token での login が必要です。

```bash
pip install -U huggingface_hub
```

```bash
hf auth login --force
```

```bash
hf auth whoami
```

pipeline は token を読まず、表示もしません。`hf` を PATH に置くか、`--hf /path/to/hf` で渡します。download は匿名で行います。

## 3. 方式 A:ローカルで変換する(推奨)

### 3.1 GGUF を固定 revision で取得する

```bash
hf download huihui-ai/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-GGUF Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-bf16.gguf --revision a0b17c71d9943529aefe88b83f494bf01c6b4d68 --local-dir ~/genesis-src/06
```

```bash
shasum -a 256 ~/genesis-src/06/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-bf16.gguf
```

SHA-256 が上の表の値と一致することを確認します(pipeline も `--local-source` の各ファイルを固定値と照合し、違えば止まります)。

### 3.2 計画だけ表示する(何も実行しない)

```bash
GENESIS_WORK=~/genesis-work python3 scripts/genesis/genesis10_pipeline.py run --model 6 --local-source ~/genesis-src/06 --tokenize-python ~/tir-venv/bin/python --dry-run
```

実行されるコマンド(tokenizer の照合、較正 token の作成、`palw-tir-convert … --context 32768 --gdn-core-wide --palwart-*`、fit、upload、verify)がすべて表示されます。

### 3.3 試しに upload なしで実行する

```bash
GENESIS_WORK=~/genesis-work python3 scripts/genesis/genesis10_pipeline.py run --model 6 --local-source ~/genesis-src/06 --tokenize-python ~/tir-venv/bin/python --no-upload --keep --ram-accounting
```

- 各段階は `~/genesis-work/state/6.json` に記録され、中断しても再実行すれば続きから進みます。
- `--ram-accounting` は、段階ごとの RAM の使用量を `<model dir>/ram-<step>.jsonl` に書きます(RFC-0020 の測定になります)。
- 空きディスクが 30 GB(`--min-free-gb`)を切ると止まります。

### 3.4 本番:upload まで行う

fit に合格したら、upload・verify・cleanup を行います。

```bash
GENESIS_WORK=~/genesis-work python3 scripts/genesis/genesis10_pipeline.py run --model 6 --local-source ~/genesis-src/06 --tokenize-python ~/tir-venv/bin/python --steps upload,verify,cleanup
```

最初から通しで行う場合は、3.3 のコマンドから `--no-upload --keep` を外します。`--local-source` の GGUF は link して使うだけで、cleanup でも削除されません。

## 4. 大きな機械がない場合

較正だけを別の機械で測る方法もあります(`palw-tir-fidelity --calibrate-only --stats-out` で統計を測り、変換側で `--calib-stats-in` で読む)。ただし、較正 token のファイルは pipeline の convert 段階が tokenizer を GGUF の語彙と照合したうえで作るものなので、手作業で作ると記録と一致しなくなる恐れがあります。**64 GB 以上の機械で方式 A を通しで実行するのが、最も確実です。** 分担する場合は、事前に相談してください。

## 5. 合格条件(fit)

pipeline の `fit` 段階が、次の 2 つを自動で判定します。外れたら index・manifest・card は upload されません。

1. **短い検査**:固定の held-out テキスト(README.md、docs/PRINCIPLES.md、2 × 64 tokens)で、平均 KL ≤ 0.1、top-1 一致 ≥ 0.8。
2. **長い検査**:RFC-0002 の、較正に使った最初の 32K の続きの 32K tokens。生成しながら全位置を比べ(`--drift-windows 32`)、次をすべて満たすこと。
   - 平均 KL ≤ 0.1、top-1 ≥ 0.8
   - どの window の平均 KL も ≤ 0.1
   - 後半と前半の比 ≤ 1.5(後半の window の KL が ≤ 0.01 なら免除)

報告は `fit-report.json` と `fit-long-report.json` に出ます。

## 6. できあがるものと、渡してほしいもの

- Hugging Face:`Misakachain/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-PALW` に index、2 GiB の segment、README(model card)、manifest、license/notice。verify 段階が、remote の SHA-256 / git blob を手元の記録と照合します。
- 手元の記録:`~/genesis-work/genesis10_results.json` の #6 の行と `~/genesis-work/state/6.json`。内容は次のとおりです。
  - container root(PALWART3 の root。catalog の `artifact_root` になる値)
  - index の SHA-256、各 segment の bytes と SHA-256
  - inner roots(TIR の inventory root など)
- そのほか:`convert-report.json`、`fit-report.json`、`fit-long-report.json`、`ram-*.jsonl`、`manifest.json`。

**これらのファイルを Claude に渡してください**(または `docs/evidence/` 配下に commit)。genesis 化の残りはこちらで行います。

## 7. genesis モデルにする(こちらで行う部分)

1. **記録の照合**:`palwart verify` / `palwart root` と `misaka_palw_sdk::palwart_deep` で、container root と inner roots を consensus の関数で導き直して一致を確かめます。
2. **class の宣言**:次のネットワークで #6 の class を宣言し、class id(128 hex)を得ます。
3. **edition 0 の catalog**:class id と記録の root から、RFC-0019 の edition 0 を作ります。

```bash
python3 scripts/genesis/genesis10_pipeline.py catalog --classes classes.json --fence-daa <次のネットワークの tier fence> --out edition0.json
```

   root か class id が欠けている model があると拒否します(仮の hash は入れません)。
4. **release manifest**:`scripts/release/misaka-release-manifest.py` で edition 0 を root 化し、genesis・年次フォークの catalog として network identity に入れます。これで #6 は 1.0 tier になります。

**まだ未完の前提(genesis 化を止めるもの)**
- **登録が PALWART3 の container root を束ねること。** いまの TIR class は inventory root を直接登録しています。container root → inner inventory root → operand → 裁定の認証の連鎖は、RFC02 の lane で実装中です。
- **#6 の fit 記録。** 方式 A の fit 合格が要ります。

genesis に入っても、claim ごとの検証(Gate C / SecurityCertified、court)は免除されません(PRINCIPLES §9)。

## 8. 注意

- revision、ファイル(bf16)、context(32,768)、`--gdn-core-wide` を変えないでください。変えると root が変わり、genesis の記録と一致しなくなります。`--context` は実験用です。
- fit が許容外なら、upload せずに止まります。値を緩めて通すことはしません。
- 方式 B は 1 回の変換で GGUF を 2.5〜3 倍ほど(約 140〜160 GB)ダウンロードします。
- 長時間の実行になるので、スリープを止めてください(macOS なら `caffeinate -dimsu` の下で実行)。
- Python がメモリを大きく使ったときに備えて、RSS を監視してください(`--ram-accounting`、`top -o mem`)。
