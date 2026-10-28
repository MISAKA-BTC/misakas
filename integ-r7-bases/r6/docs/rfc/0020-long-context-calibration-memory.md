# RFC-0020 — 長文脈の較正・変換・忠実度計測のメモリ: 32K の依存構造を保ち、同時に持たなくてよいものを持たない

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **設計前提(ADR・RFC より上位)— [MISAKAの不可侵原則](../PRINCIPLES.md):** 確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。本RFCはこの前提の下にあり、衝突する場合は前提が優先する。
> **暗号学的な計算証明を使わない — [PRINCIPLES §8](../PRINCIPLES.md)(恒久):** 本RFCは合意の外側の道具(較正・変換・忠実度計測)のメモリ設計であり、計算の正しさの証明を何も導入しない。使うハッシュは成果物と統計の同一性(digest)だけである。
> **genesis 前提 — 2026-10-15/16 ユーザー決定:** genesis は 8 モデル(#1 2 3 4 5 6 8 10)、**テキスト生成のみ**、**全モデルを 32K(32,768 token)の文脈で提供する**(2026-10-16)。再帰(Gated DeltaNet)クラスは提供文脈と同じ長さの系列 1 本で較正する(freeze-v1 §5.2、変換器が強制)。

* **Status:** Draft — 2026-10-16。設計(§1–§8)。実装(**implemented, unverified** — 本 lane は cargo build/test を実行していない。どの項目も PASS ではない): RAM 計上モード(§2)、carry の二重保持の除去(§3)、lazy 重みの既定値と警告(§4)、全系列スコア経路の監査(§5.1)、carry のディスク退避(§6.2、opt-in)、長文脈ドリフト検査(§7)。設計のみ・未実装: タイル化した厳密 attention と chunk-aware observer(§5.2–§5.4。監査の結果、genesis のテキスト経路には全行列経路が存在しない)、KV 履歴のディスク退避(§6.3)。
* **Decision:**
  1. **目標は 32K の依存構造を完全に保ったまま、同時に持つ必要のないデータを RAM に置かないこと。** 系列を分割しない。attention の範囲を縮めない。`Op::Attention` の `chunk` や小さな `max_window` をメモリ対策に流用しない(どちらも query が見てよい key を制限し、意味を変える)。
  2. **計測を先にする。** 段階(較正・変換・忠実度・推論)と種類(重み・carry・KV 履歴・GDN 状態・attention 一時領域・統計・logits)ごとに live と peak を出す RAM 計上モードを置き、OOM の原因を推測でなく帰属させる(§2)。
  3. **ビット同一性を崩さない。** 実装したメモリ削減(§3、§6.2)と速度改善(位置の run 評価、スレッド化カーネル)は、同じ入力に対して統計 digest・logits・GDN 状態がビット単位で一致することを試験で固定する。一致しない高速経路は明示的にラベルを付け、成果物の同一性を主張しない(§5.3)。
* **Activation:** **なし。** 合意・fence・wire・RPC には触れない。変換器(`palw-tir-convert`)と忠実度計測(`palw-tir-fidelity`)の CLI フラグと、genesis パイプライン(`scripts/genesis/genesis10_pipeline.py`)の引数だけである。
* **Baseline:** `integ/genesis10`(本RFCの commit)。
* **Not activated by:** 本RFCの merge、試験 source の存在、見積り表(§9)。
* **Normative language:** MUST / MUST NOT は本RFCの道具が守る不変条件である。

## ユーザー方針 2026-10-16(本RFCを拘束する)

1. 32K の依存構造は完全に保つ。不要なデータを同時に RAM に置かない。系列分割・attention 範囲の縮小はしない。
2. まず計測: 段階別・種類別に RAM を分け、計上モードで OOM を帰属させる。
3. carry の二重保持を除く(ユーザー提供パッチ `reduce_calibration_carries`)。期待される削減は 1 系列分の carry(例: 32K × 4096 × FP32 ≈ 512 MiB)。アロケータが解放済みメモリを保持し得るので、総 RSS については何も言わない。
4. lazy 重み(`--calib-lazy-mib` / `--calib-lazy-cache-mib`)は重みの常駐を制限するだけで、KV や attention には効かない。行範囲を読めるソースが必要で、事前量子化 param は対象外。
5. タイル化した厳密 attention は、全行列経路が実在する場所に限る。正規化は**見えるすべての key** に対して行い、絶対位置の因果マスク(key_pos ≤ query_pos)、絶対 RoPE 位置、GQA の対応を保つ。**key chunk ごとに softmax して足し合わせてはならない。**
6. 統計の互換: 参照と同一の多パス経路(a)を先に、オンライン softmax の高速経路(b)は下位ビットが変わり得るのでラベル付けし成果物同一性を主張しない。chunk を意識した observer を置く。
7. 32K を超える文脈(64K/128K)は KV と carry をディスクへ予算付きで退避する(RAM とディスク I/O の交換。計算量は全 attention で O(N²) のまま)。
8. 実装順: (1) 計測、(2) carry パッチと lazy 重み、(3) 全行列経路が見つかった場合のみタイル化 attention、(4) より長い文脈のための KV/carry 退避。

## 0. 要約

| 対象 | 32K で何が RAM を占めるか | 本RFCの扱い |
| --- | --- | --- |
| 重み | 実行中の occurrence 1 つ分(層、または embedding / head)。#6 の f32 embedding/head が最大 5.1 GB | 既定のまま(layer-major)。lazy 重みは MoE の expert stack 向けで、密な模型では既定 off(§4) |
| carry | 系列全体の隠れ状態 1 本(#2 256 MiB、#3 512 MiB、#6 640 MiB)。**旧実装は入力と出力の 2 本を同時に持っていた** | 消費した入力行をその場で退役させ 1 本に(§3)。超長文脈はディスク退避(§6.2) |
| KV 履歴 | attention 層 1 つ分の session の key/value 行(#2 128 MiB、#3/#6 256 MiB) | そのまま。layer-major では同時に 1 層分だけ(§6.3) |
| GDN 状態 | 層あたり数 MiB | そのまま |
| attention 一時領域 | **1 query 位置 × 全 head のスコア行**(#6 で 24 × 32,768 × 8 B ≈ 6 MiB) | 全行列は存在しない(§5.1)。タイル化は全行列経路が実在する所だけ(§5.2) |
| 統計 | site 数 × channel 数。数十 MiB 以下 | そのまま |
| logits | 較正では作った端から捨てる。忠実度計測の `--exec` 経路は 1 位置ずつ比較 | `--exec` なしで 32K を評価すると両側の logits が ~97 GB になるため拒否(§7) |

## 1. 範囲

対象は genesis のテキストデコーダ(token id → 次 token の logits)の 4 段階である。

| 段階 | 道具 | 長文脈で効くもの |
| --- | --- | --- |
| 較正 | `palw-tir-convert`(`fidelity::calibrate` → `run_layer_major`) | carry、KV 履歴、GDN 状態、attention 一時領域、統計 |
| 変換 | `palw-tir-convert`(`materialise_stream` → PALWART3) | 重み(テンソル単位)。文脈長には依存しない |
| 忠実度 | `palw-tir-fidelity --exec`(`run_to_post` + 型付き整数実行器) | carry(post 入力)、整数 KV、logits(1 位置ずつ) |
| 推論 | `misaka-palw-tir-exec`(1 token ずつの step) | 整数 KV |

合意(court、Gate C、re-execution)は 1 位置の step を標本検証するので、本RFCの対象外である。

## 2. 計測を先にする — RAM 計上モード(実装済み・未検証)

`misaka/palw/tir/lower/src/ram_account.rs`。`--ram-accounting FILE`(`palw-tir-convert`、`palw-tir-fidelity`)、パイプラインでは `run --ram-accounting`(`<model dir>/ram-<step>.jsonl`)。

* **種類:** `weights`(実行中 occurrence の常駐 param と lazy cache)、`carries`(RAM 上の carry)、`spilled_carries_on_disk`(退避分、RAM ではない)、`kv_history`、`recurrent_state`、`attention_temporaries`(1 回の attention 呼び出しの最大一時領域)、`stats`、`logits`。
* **段階:** `open`、`calibration`、`conversion`、`materialise`、`fidelity.float`、`fidelity.int`。段階に入るたびに peak を live 値から数え直す。
* **出力:** 1 行 1 JSON。occurrence(層)ごと、整数実行 1,024 位置ごと、段階の境で snapshot(live・peak・RAM の live 合計・OS の RSS)。最後に段階ごとの peak の要約行。
* **注意:** live は評価器のデータ構造を長さで数えた値(`f32` 行 × 4 B)であり、アロケータの見方ではない。アロケータは解放済みメモリを保持し得るので、RSS は live 合計より大きいままになり得る。両方を記録するのはそのためである。無効時は相対 atomic の読み 1 回だけで、結果は変わらない。

## 3. carry の二重保持を除く(実装済み・未検証)

旧実装の `run_layer_major` / `run_to_post` は、occurrence ごとに入力の carry 系列(`carries`)を読みながら出力系列(`outs`)を別に作り、occurrence の終わりで差し替えていた。その間、系列 2 本分を同時に持つ。

**変更(ユーザー提供パッチを、位置の run 評価に合わせて適用):** 入力行は読んだ時点で取り出し(RAM 上の行は空にして解放)、出力は次の occurrence の carry として書く。post block では入力行を退役させ、求められた logits だけを残す。session・絶対位置・評価順・site の観測順・`SiteStat` の merge 順は変わらない。

* **期待される削減:** 系列 1 本分の carry。32K × hidden × 4 B = #2 256 MiB、#3 512 MiB、#6 640 MiB(#3 はユーザー分析の例 ≈512 MiB に一致)。run の長さ分(既定 32 位置)の出力は一時的に重なる。
* **総 RSS については何も言わない**(アロケータ)。§2 の計上で live の減少を確認する。
* **ビット同一性:** `misaka/palw/tir/lower/tests/position_blocks.rs::consumed_and_spilled_carries_are_the_two_sequence_run_bit_for_bit` が、旧アルゴリズム(2 系列保持・1 位置ずつ・系列順 merge)をそのまま写した参照と、統計の digest(`stats_digest_hex`)・正準 JSON・全 logits のビットを 7 つのアーキテクチャの toy checkpoint で比較する。

## 4. lazy 重み(既存機能の既定値と案内)

`--calib-lazy-mib N` は、`N` MiB 以上の param を occurrence と一緒に常駐させず、演算が求めた時に読む(expert stack は router が選んだ expert の行だけ)。`--calib-lazy-cache-mib` は読んだテンソルを保持する予算である。

* **効くもの:** 重みの常駐だけ。KV 履歴・carry・attention には効かない。
* **前提:** 行範囲を読めるソース(safetensors の `Checkpoint`、遅延 shard、リモート GGUF)。事前量子化 param は対象外(整数をまとめて読む)。
* **罠(本 lane のパイプラインで実際に既定にしていた):** `--calib-lazy-mib 64` と 512 MiB の cache では、cache に収まらない**密な**テンソル(MLP 行列、embedding、head)を**位置ごとに丸ごと読み直す**(`ParamStore::view`)。#6 のストリーム GGUF では位置 × 層ごとに約 1 GB の再ダウンロードになる。
* **既定:** off(`palw-tir-convert` は元から 0、パイプラインも 0 に修正済み)。layer-major はもともと 1 occurrence 分しか常駐させず、genesis 全モデルで 32 GB に収まる。
* **案内:** 使うのは MoE の expert stack を常駐させたくない時だけ。`--calib-lazy-mib` を密な行列より大きくして expert stack だけを lazy にするか、`--calib-lazy-cache-mib` を層の lazy な密テンソルの合計より大きくする。
* **警告(実装済み):** cache に収まらないテンソルを丸ごと読み直した最初の 1 回に、プロセスごとに一度だけ警告を出す。

## 5. attention のメモリ

### 5.1 監査: 全系列のスコア行列はどこにあるか(実施済み)

| 経路 | attention の形 | 32K での一時領域 |
| --- | --- | --- |
| 較正・忠実度の float 参照(`float_ref::attention`) | 1 query 位置 × その位置までの全履歴、head ごと(2 パス softmax) | #6: 24 head × 32,768 × (dot・score・prob の f64 + 観測用 f32)≈ 25 MiB |
| 位置の run 評価(`Session::eval_occurrence_run`) | 同上。run 内の各位置は**絶対位置で見える行だけ**を読む(§5.5) | 同上 |
| 整数の参照評価器・型付き実行器(`tir`、`misaka-palw-tir-exec`) | 1 token ずつの step。スコアは履歴長の行 | 同程度 |
| court・re-execution | 1 位置の step | 同程度 |
| A16 / PALWQ361 エンジン(base0) | 1 token ずつ | 同程度 |
| 全行列(N × N)を作る経路 | 双方向エンコーダ(`lower/bidir.rs`)、vision(`lower/vision.rs`)、diffusion(`diffusion/attn.rs`) | **genesis の範囲外**(テキスト生成のみ) |
| 全系列の **logits**(スコアではない) | `palw-tir-fidelity` を `--exec` なしで使う時、両側の logits を全位置分保持 | 32K × 248,320 × 12 B ≈ 97 GB → **8 GiB 超は拒否するよう変更**(§7) |

結論: genesis のテキスト経路(較正・変換・忠実度 `--exec`・実行器・court)に 32K × 32K のスコア行列は存在しない。§5.2–§5.4 のタイル化は**実装しない**(ユーザー方針 5: 全行列経路が実在する所だけ)。全行列経路(双方向・vision・diffusion)を将来 genesis に入れる場合の仕様として残す。

### 5.2 タイル化した厳密 attention(仕様、未実装)

全行列の大きさは `B × H × N² × bytes`。例: 24 head × 32,768² × 4 B = **96 GiB**。

| 方式 | 一時領域(24 head、FP32) |
| --- | --- |
| 全行列 | 96 GiB |
| query だけタイル化(Q = 128) | 24 × 128 × 32,768 × 4 B = **384 MiB** |
| query と key をタイル化(Q = 128、K = 1,024) | 24 × 128 × 1,024 × 4 B = **12 MiB** |

不変条件(MUST):

1. softmax の正規化は、その query から**見えるすべての key** に対して 1 回だけ行う。**key chunk ごとに softmax して結果を足し合わせてはならない**(確率の和が 1 にならず、意味が変わる)。
2. 因果マスクは絶対位置で判定する(`key_pos ≤ query_pos`)。タイル内の相対位置で判定しない。sliding window・sink も絶対位置で同じ規則を適用する。
3. RoPE は絶対位置で回す。タイルの先頭を 0 にしない。
4. GQA の対応(query head `h` → kv head `h / (heads / kv_heads)`)をタイル化で変えない。
5. 系列を分割しない。前のタイルの key を落とさない。

### 5.3 統計との互換

較正は attention の `scores` と `probs` の sub-site を観測し、`calib.rs` は IEEE のビットパターンとその digest を保存する。よって 2 つの経路を分ける。

* **(a) 参照と同一の経路(先に実装する):** 多パス。(1) 見える全 key のスコアの最大値、(2) 分母(参照と同じ加算順)、(3) 確率・出力・統計。浮動小数の演算順を参照と揃え、統計 digest・logits が参照とビット一致する。
* **(b) 高速経路:** オンライン softmax(走りながら最大値と分母を更新)と、統計用の別パス。下位ビットが変わり得る。**ラベルを付け、成果物の同一性(artifact root、統計 digest)を主張してはならない。**

### 5.4 chunk を意識した observer(仕様、未実装)

`SiteStat::observe()` はベクトルの長さから ragged か channel 配置かを推定する。chunk ごとに観測すると、1 行が長さの違う断片として届き、channel の扱いと位置 0 の判定が崩れる。chunk を意識した observer は (論理行の長さ、chunk の offset、絶対位置) を受け取り、1 行を連結して観測した場合と同じ `absmax`・`pos0_absmax`・`rest_absmax`・`chan_absmax`・`ragged` を作る。`sum_sq` は (a) の経路では参照と同じ順で加える。

### 5.5 位置の run 評価(実装済み・未検証)は chunk attention ではない

32K 較正の速度のため、block の各 node を run 内の全位置について評価してから次の node へ進む(`Session::eval_occurrence_run`、既定 32 位置、`PALW_FLOAT_POSITION_BLOCK` で変更、1 で従来どおり)。行列は run ごとに 1 回読まれる。attention は run 内でも**1 query 位置ずつ**、その位置までに位置ごとの実行が append したはずの行(`key_pos ≤ query_pos`)だけを読む。系列分割・範囲縮小・`chunk`/`max_window` の流用は一切ない。admit する block は、履歴が append 1 つと attention の読みだけ、固定状態が 1 node だけ(GDN、causal conv1d)のものに限り、それ以外は位置ごとに戻る。統計・logits がビット一致することを `tests/position_blocks.rs` が 7 アーキテクチャで固定する。

## 6. 32K を超える文脈(64K / 128K)

### 6.1 線形に増えるもの

| | 32K | 64K | 128K |
| --- | --- | --- | --- |
| carry(#6、5,120 × 4 B) | 640 MiB | 1.25 GiB | 2.5 GiB |
| KV 履歴(#6、attention 層 1 つ、key+value 2,048 × 4 B) | 256 MiB | 512 MiB | 1 GiB |
| attention 一時領域(#6、1 query 位置) | ~25 MiB | ~50 MiB | ~100 MiB |
| 全 attention の演算量 | N² に比例 | ×4 | ×16 |

### 6.2 carry のディスク退避(実装済み・未検証、opt-in)

`--spill-carries-dir DIR [--spill-carries-budget-mib N]`(両道具、パイプラインも同名)。ある occurrence の出力が系列全体で予算を超える見込み(最初の行のバイト × 位置数 > 予算、0 なら常に)なら、出力をファイルへ位置順に書き、次の occurrence は run ごとに読み戻す。行は同じ `f32` のビット(リトルエンディアン)なので、in-RAM の実行とビット一致する(`tests/position_blocks.rs` が予算 0 で検証)。RAM をディスク I/O と交換する。32K では不要(§9)。

### 6.3 KV 履歴の退避(仕様、未実装)

手順: 層ごとに重みを読む → 隠れ状態をブロック単位でディスクから読む → GDN 状態・conv 履歴は RAM に保つ → attention は KV をブロック単位で読む → 統計を更新 → 出力をディスクへ書く → バッファを解放。演算量は全 attention で O(N²) のまま。

未実装の理由: layer-major では同時に存在する KV 履歴は attention 層 1 つ・系列 1 本分で、genesis では 128K でも #6 で 1 GiB に収まる。さらに 1 query 位置ずつ KV をディスクから読むと I/O が O(N²) になるため、KV 退避は query ブロックでまとめて読む構造(§5.2 の query タイル化)を前提とする。必要になった時点で §5.2(a) と同時に実装する。

## 7. 検証

* **ビット同一性(試験 source、未実行):** `misaka/palw/tir/lower/tests/position_blocks.rs`(run 評価、消費型 carry、退避 carry を旧アルゴリズムと統計 digest・正準 JSON・全 logits で比較。7 アーキテクチャ)、`misaka/palw/tir/lower/src/float_ref/tests.rs`(スレッド化した `linear_raw` / `linear_raw_many` / `attention` / `gated_delta_traced` を旧カーネルの写しとビット比較。GDN 状態を含む)。
* **長文脈ドリフト検査(実装済み・未検証):** `palw-tir-fidelity --exec --drift-windows N` は、評価の全位置を作った端から比較し(logits 行を保持しない)、評価を N 等分した窓ごとの平均 KL と最大 KL を出す。パイプラインの `fit` は短い held-out(2 × 64 token)に加えて、較正に使った最初の 32K token の**次の** 32K token(RFC-0002、held-out)で 32 窓を測り、平均 KL ≤ 0.1、top-1 ≥ 0.8、各窓の平均 KL ≤ 0.1、後半/前半 ≤ 1.5(後半の KL ≤ 0.01 なら免除)を満たさなければ index を upload しない。
* **未計測:** 8K / 16K / 32K の peak RAM と時間。§2 の計上モードで測る(§9 のコマンド)。

## 8. 実装順と状態

| 順 | 内容 | 状態 |
| --- | --- | --- |
| 1 | 計測(RAM 計上モード) | 実装済み・未検証 |
| 2 | carry パッチ、lazy 重みの既定値と警告 | 実装済み・未検証 |
| 3 | タイル化した統計対応 attention((a) を先) | **不要と判断**(§5.1: genesis の経路に全行列なし)。仕様のみ |
| 4 | KV/carry のディスク退避 | carry は実装済み・未検証(opt-in)。KV は仕様のみ(§6.3) |

## 9. 32K の見積り(未計測)

前提: M1 Max(P 8 + E 2、32 GB)。旧カーネルの実測(9B 較正 8,192 位置 18,360 s ≈ 3.5 GMAC/s、#2 の 7 層 × 8,192 位置 ≈ 1,600 s ≈ 2.1 GMAC/s)を基準に、スレッド化カーネルと run 評価で線形部 12–22 GMAC/s、attention 12–18 GMAC/s と置いた。誤差は ±50% 程度。

| | 線形 MAC/位置 | attention MAC(32K 全体) | 較正 32K | 変換 | 長文脈 fit | peak RAM(較正) |
| --- | --- | --- | --- | --- | --- | --- |
| #2(2B、GDN 18 + attention 6) | 1.9 G | 13 T | 1.5–2 h | ~10 分 | float 1.5–2 h + 整数 ~0.5 h | 2.5–3.5 GB(fit 時 5–6 GB) |
| #3(9B、GDN 24 + attention 8) | 7.9 G | 35 T | 4.5–7 h | ~30 分 | float 4.5–7 h + 整数 ~1 h | 5–6 GB(fit 時 ~16 GB) |
| #6(27B、GDN 48 + attention 16、ストリーム GGUF) | 25.6 G | 106 T | 13–19 h | 1.5–3 h(HTTPS 再読込と upload 込み) | この Mac では不可(整数 program 29.5 GB、ソースはストリーム) | 8–11 GB |

**#6:** この Mac での合計は 15–22 時間の見込みで、24 時間をわずかに下回る程度で不確かである。最速で正しい経路は**較正専用マシン**: 64 コア級・64 GB 以上の機械に GGUF を置き、`palw-tir-fidelity <gguf> --calib calib-tokens.json --calibrate-only --stats-out calib-stats.json --gdn-core-wide`(run 評価で行列の再読込が減り、コア数に比例する。見込み 3–4 時間)、この Mac で `--calib-stats-in calib-stats.json` を付けて変換だけを行う(1.5–3 時間)。統計は別プラットフォームの libm の下位ビットを含み得るが、成果物は運ぶ統計に対して決定的であり、manifest が測定プラットフォームを記録する。同じ機械で長文脈 fit も測れる。

## 10. 本RFCで決めないこと

* 32K 以外の提供文脈。
* §5.2 の (b) 高速経路の採否。
* §6.3 の KV 退避の実装時期(必要が計測で示された時)。
* 32K の court・re-execution の資源上限(合意側の別作業)。
