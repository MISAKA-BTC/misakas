# RFC-0001: PALW 推論サーフェスの欠落機能 — 決定論的な生成制御・サービング・入力拡張の設計

| 項目 | 値 |
|---|---|
| Status | **Draft**(2026-09-27) |
| 対象 | testnet-12(R-core+)の free-prompt(FP)lane、`misaka-palw-gateway` / `misaka-palw-worker` / `misaka-palw-base0` / `misaka-palw-constraint`、Studio |
| 関連 | ADR-0082 D10/D11(decode の分子とサンプラー)、ADR-0096(OpenAI 互換サーフェス)、ADR-0144(使う推論に払う)、ADR-0145 §6(キャッシュは実行事実)、ADR-0077 D1(常駐 worker)、ADR-0153(2M 分割)、ADR-0160(容量再設計) |
| 実装ブランチ | `rcore/fp-sampler`(P0 のうち D10・D11・決定論 repeat_penalty) |

---

## 0. 要約

現在の FP lane は「宣言した長さまで、greedy で、1 リクエスト 1 エンジンで、テキストだけを」生成する。欠けているものは 3 系統に分かれる。

* **生成制御**(サンプリング、repeat_penalty、stop、logit_bias、n、自由な JSON Schema):出力 token を変えるので **consensus の規則**。job に commit し、panel が再実行で同じ token 列を得られ、court が「tile 2 枚の開示」で反証できる形でしか入れられない。
* **サービング**(会話をまたぐ KV キャッシュ、並行処理、embeddings、artifact の同梱):committed 計算を 1 bit も変えない限り **ノードだけの変更**で入る。報酬への反映(キャッシュ分を払わない)は ADR-0145 §6 がすでに規則として持つ。
* **入力拡張**(マルチモーダル、LoRA/adapter):class が「token id だけを読む、重み固定の実行グラフ」として定義されているので、**新しい class 設計**(ADR)が要る。

優先順位は、**P0** = decode 規則の一括導入(D10 + D11 + repeat_penalty + logit_bias + stop_token_ids、1 本の fence)、**P1** = ノードだけのサービング改善(KV prefix キャッシュ・並行処理・n の fan-out・embeddings・artifact sidecar)、**P2** = consensus の経済変更(prefix-state receipt、JSON Schema 拡張、n の job group)、**P3** = 新 class(adapter、マルチモーダル、embedding claim)とする。実用上いちばん効くのは P1 の KV prefix キャッシュ。

---

## 1. 前提 — PALW が守る 3 つの不変条件

以後のすべての設計は、次の 3 条件を満たすかで判定する。

| # | 不変条件 | 意味 |
|---|---|---|
| **I-1** | **出力を変えるものは job に commit され、再実行で再現する** | panel の seat は producer と同じ job を再実行し、同じ token 列を得る。乱数・時刻・ノード状態に依存する選択は入れられない。整数(Q24)カーネルなので、同じ入力なら bit 一致する。 |
| **I-2** | **反証は per-lane の「tile 2 枚の開示」で済む** | court は committed lane と、それを上回るはずの lane の logits tile 2 枚を開き、2 つの key を再計算して比べる(ADR-0049 Decision E)。行全体(Qwen 級の語彙で約 993 KB)を要する規則は `palw_close_budget` が拒否する。**したがって、lane j の key は「lane j の値」と「job・committed 履歴から公開で決まる量」だけの関数でなければならない。** softmax 正規化・top-p・min-p のような「行全体を見る」規則はこの形にならない。 |
| **I-3** | **キャッシュ・並列は実行上の事実であり、claim ではない** | ノードは committed 計算を変えない最適化を自由にしてよい。報酬は derived work(ADR-0145)で決まり、キャッシュで省いた計算は払わない(§6)。「キャッシュに当たった」という申告は入力にならず、検証者は新しい計算だけを再構成する。 |

### 1.1 分類表

| # | 欠落機能 | 分類 | 何を変えるか |
|---|---|---|---|
| 1 | マルチモーダル入力 | **K** 新 class | class 定義(encoder グラフ)、job の入力形 |
| 2 | 会話をまたぐ KV キャッシュ | **N** ノード(報酬反映は **C**) | worker/gateway。receipt の prefix-state は ADR-0145 §6 |
| 3 | 並行処理 | **N** ノード | worker のプロセス構成、scheduler |
| 4 | `/v1/embeddings` | **N**(claim 化は **K**) | gateway の新エンドポイント |
| 5 | `stop` | **C** consensus | job に stop_token_ids、decode の停止規則 |
| 6 | `logit_bias` | **C** consensus | job に bias 表、lane key |
| 7 | JSON Schema の範囲 | **C** consensus | `misaka-palw-constraint` の subset と compile |
| 8 | `n` > 1 | **N**(job group は **C**) | gateway の fan-out |
| 9 | artifact の自己完結性 | **N**(registry 照合は **C**) | artifact v2 sidecar、Studio の読み込み |
| 10 | LoRA / adapter | **K** 新 class | 派生 class の登録と実行 |
| — | サンプリング(seed/temperature) | **C** | ADR-0082 D11(実装済みのサンプラーを engine に入れる) |
| — | repeat_penalty | **C** | 本 RFC §2.1(新規) |

---

## 2. 項目別の設計

### 2.1 決定論的 repeat_penalty(新規実装・P0)

**目的.** 小さなモデルは temperature 0 で少数の吸引子に崩れる(params.rs の実測:60 seed で min-entropy 約 3.1 bit)。D11 の Gumbel-max サンプラーだけでも改善するが、長い回答では同じ句の反復が残る。llama.cpp の `repeat_penalty` / `frequency_penalty` / `presence_penalty` に相当するものを、**乱数を使わない整数演算**で入れる。

**規則.** 位置 t で、committed 済みの直前 `W = repeat_last_n` 個の token(プロンプトの末尾を含むかは job のフラグで選ぶ)について、各 token id j の出現数 `c_j(t)` を数える。lane j の整数 logit `v_j`(Q24、エンジンがすでに出している値)を次で補正する。

```
repeat (乗算型、llama.cpp と同じ向き):
  c_j(t) > 0 かつ v_j > 0 :  v'_j = floor( v_j · 2^16 / p_q )
  c_j(t) > 0 かつ v_j ≤ 0 :  v'_j = floor( v_j · p_q / 2^16 )      // 負の値はより負に(−∞ 方向に切り捨て)
  c_j(t) = 0             :  v'_j = v_j
frequency / presence (加算型、OpenAI と同じ向き):
  v''_j = v'_j − c_j(t) · f_q − [c_j(t) > 0] · s_q                  // f_q, s_q は Q24
key:
  key_j = v''_j · T_ONE + T_q · G_j(seed, t, j)                       // D11 の key、ties は最小 index
```

* `p_q` は Q16(`65536` = 1.0 = 無効)、範囲 `[65536, 4·65536]`。`f_q`・`s_q` は Q24 の符号付き整数で、範囲は OpenAI の `[-2, 2]` を写した値に制限する。
* 丸めは**すべて −∞ 方向**に固定し、i64 で中間値を持ち、オーバーフローは job の受理時に範囲で排除する。
* `W` は `1..=256`(上限は DoS と court のコストで決める。§7 の未決事項)。

**I-2 を満たす理由.** `v''_j` は「lane j の値 `v_j`」と「`c_j(t)`」だけの関数で、`c_j(t)` は committed 済みの token 列(回答そのもの、または DA で公開される capture)から誰でも数えられる。court の反証は従来どおり 2 tile の開示に、該当 window の committed token 列(公開済み)を添えるだけで済む。**行全体は要らない。**

**job への載せ方.** `PalwFreePromptJobV3` は borsh 全体を `fp_job_id_v3` で hash するので、フィールドを足すと既存の job id の意味が変わる。したがって **`PalwFreePromptJobV4`** を新設し、`repeat_penalty_q: u32`・`repeat_last_n: u16`・`repeat_includes_prompt: bool`・`frequency_penalty_q: i32`・`presence_penalty_q: i32` を持たせる(§2.2・§2.3 のフィールドも同じ V4 に入れる)。V3 はそのまま有効で、V4 は decode 規則の fence(§3)以降だけ受理する。**全フィールドが identity 値の V4 は、同じ token 列を出すことをテストで保証する**(greedy との byte 一致、D11 の `T_q = 0` と同じ性質)。

**実装箇所.** 生成(`misaka-palw-base0` の produce)と再実行(panel の replay・`misaka-palw-reexecutor`)の**両方が同じ関数**(consensus-core の `palw_decode_select_v2` に `decode_lane_key_v3` を追加)を呼ぶ。エンジン側で独自に書かない。

**テスト.** (a) produce と replay で、seed・温度・penalty の組み合わせを掃いて token 列が bit 一致、(b) identity 値で V3 と一致、(c) 反証:penalty を無視した token を commit した claim が 2 tile + 履歴で反証できる、(d) 境界値(`p_q` の上下限、負の logit、`W` の上限、プロンプト跨ぎ)。

### 2.2 stop(停止条件・P0)

**現状.** ADR-0096 Decision 4 で拒否。宣言した budget まで生成し、利用者が切る。

**設計.** 文字列の stop は tokenizer の分割に依存するので consensus に入れない。代わりに **token id 列の stop**(`stop_token_ids: Vec<Vec<u32>>`、最大 4 本、各 8 token まで)を V4 に入れる。

* 規則:生成は `decode_token_limit` を上限とし、committed 列の末尾がいずれかの stop 列に一致した位置 k で止まる。以後の leaf はない。
* 反証:停止位置は公開の token 列から自明に検証できる(「一致していないのに止まった」「一致していたのに続けた」の 2 種類)。tile の開示は要らない。
* 報酬:D10 の分子は実行した decode leaf なので、早く止まった分は払わない(budget は上限、work は実測)。
* 文字列 stop:gateway が表示レイヤで扱う(該当文字列以降を返さない)。同時に、gateway は文字列を「確実に含む token 列」の候補へ変換して `stop_token_ids` に入れることもできるが、分割の揺れで止まり損ねうることを応答に明記する。

### 2.3 logit_bias(P0)

**設計.** V4 に `logit_bias: Vec<(u32 token_id, i32 bias_q)>`(最大 300 件、提案値)を入れる。`bias_q` は OpenAI の `[-100, 100]` を Q24 に写し、`-100` は「禁止」フラグとして扱う(key を最小値に固定)。

* `v_j' = v_j + bias_j`(repeat 補正の前に適用)。`bias_j` は job から公開で決まるので、I-2 を満たす。
* 重複 id・範囲外は受理時に名指しで拒否する(ADR-0096 の原則)。

### 2.4 n > 1(P1 → P2)

**原則.** 「1 推論 = 1 claim」は維持する。n 候補は n 本の claim。

* **P1(consensus 変更なし)**:gateway が同じプロンプトで n 本の job を出す。`sampling_seed_i = H(base_seed ‖ i)` とし、応答の `choices` にまとめる。§2.6 の prefix キャッシュで、prefill の実計算は 1 回で済む。
* **P2**:ADR-0145 §6 の prefix-state receipt が入れば、2 本目以降の prefill は「新しい計算なし」として払われない(二重払いなし)。親 job を共有する job group が必要かは P2 で判断する(必要なら ADR)。

### 2.5 JSON Schema の拡張(P2)

**現状.** `misaka-palw-constraint::schema` の subset(type・properties・required・additionalProperties・items など)だけを受け、範囲外は名指しで拒否する。制約は decode の許可 token 集合として commit され、seat が再実行し、court が審理できる(ADR-0096 Decision 3)。

**拡張.** `enum`・`const`・判別できる `anyOf`/`oneOf`・`minItems`/`maxItems`・`minLength`/`maxLength`・数値範囲・非再帰の `$ref`・決定的な正規表現 subset の `pattern`(DFA に落とせるもの)を段階的に足す。

* 検証の形:制約は「状態 → 許可 token 集合」の決定的なオートマトン。committed token が許可集合に入っていることは token ごとに検証でき、「許可集合内の argmax」も、beating lane が許可集合内かを 1 bit 確かめれば 2 tile の反証で済む(I-2)。
* **近似はしない**方針を維持する。compile できない schema は従来どおり名指しで拒否する。
* DoS 対策:compile 後の状態数・遷移数・compile 時間に上限を置き、上限超えは拒否する。

### 2.6 会話をまたぐ KV キャッシュの再利用(P1・最優先)

**現状.** `misaka-palw-gateway` の常駐 worker は artifact の mmap を 1 回で済ませる(ADR-0077 D1)が、「1 エンジン・1 KV キャッシュ・mutex 1 つ」で、リクエストごとに KV を使い捨てる。会話の各ターンが履歴全体を先頭から再計算するので、8k・2M class では会話が伸びるほど計算が増える。

**プロトコル側の土台はすでにある.** ADR-0145 §6 は、receipt に「入力 commitment・prefix-state commitment・新しい token 範囲・出力 commitment・class・実行モード(uncached / prefix-reused / KV-reused)」を持たせ、検証者は新しい計算だけを再構成し、キャッシュ分は払わないと定めている。**欠けているのはサービング側の実装。**

**設計(段階 1:ノードのみ、consensus 変更なし).**

* **prefix キャッシュ**:key = `(class_id, tokenizer_id, H(prefix token ids))`、value = その prefix までの KV 状態(K/V i16 codec)。LRU で、メモリ予算は `--kv-cache-budget` で指定する。任意でディスクへ退避する。
* **API**:OpenAI 互換のまま、会話履歴の先頭一致で自動的に当てる。`misaka.session_id` は任意のヒントとして受ける(無くても動く)。
* **決定性**:整数カーネルなので「キャッシュから再開」と「先頭から計算」は bit 一致しなければならない。**prefix-resume == fresh の golden テストを必須にする。** 一致しない場合はキャッシュを使わない(安全側に倒す)。
* **claim**:段階 1 では、claim は従来どおり全体を commit する(報酬も従来どおり)。高速化はノードの応答時間だけ。
* **メモリの目安**:8k は 1 セッションあたり数百 MiB 級で多数を保持できる。2M は K/V だけで約 11.6 GiB(kv-codec メモ)なので、事実上 1 セッション。

**段階 2(P2、consensus 変更).** ADR-0145 §6 の receipt 形状(prefix-state commitment)を FP の claim に実装し、panel は prefix を自前のキャッシュで持つか、prefix-state commitment に対して新しい範囲だけを再計算する。報酬は新しい計算の分だけになる。receipt 形状の変更なので fence が要る。

### 2.7 並行処理(P1)

**設計.** 2 段で進める。

1. **複数 worker プロセス**:同じ artifact を読み取り専用で mmap 共有し(重みは OS のページキャッシュで共有)、KV は各プロセスが持つ。gateway は class ごとに worker プールを持ち、キュー・上限・公平性(接続元ごとの上限)で割り振る。実装が小さく、決定性に影響しない。
2. **プロセス内 scheduler(continuous batching)**:1 プロセスで複数シーケンスをまとめて計算する。**batch 化しても各シーケンスの計算が混ざらない(batch-invariant)こと**が条件で、整数カーネルは行単位の独立性を守れば bit 一致する。golden テスト(単独実行 == batch 内実行)を必須にする。

claim の形(1 推論 = 1 claim)は変わらない。各リクエストが自分の job を持つ。

### 2.8 /v1/embeddings(P1、claim 化は P3)

* **定義**:class の最終層 hidden state を、固定の pooling(mean または last-token)でまとめた整数ベクトル(Q24)。正規化はクライアント側、または固定の整数 L2 で行う。
* **P1**:ノードのローカルサービスとして提供する(claim なし・報酬なし)。同じエンジン・同じ artifact・§2.6 のキャッシュを使う。
* **P3**:新しい job kind(出力 = ベクトルの commitment)として claim にする場合は、報酬と検証(ベクトル全体の再実行比較)の経済設計が要るので別 ADR。

### 2.9 artifact の自己完結化(P1)

**現状.** tokenizer commitment が artifact 内で全ゼロ(既知の問題)、tokenizer.json は別リポジトリ、`generation_config.json` は参照用で実行時に読まれない、chat template は `wire` クレート側。

**設計(artifact v2 の sidecar セクション).**

* sidecar に `tokenizer.json`(バイト列と commitment)、chat template、generation config の既定値を入れ、manifest に各 digest を記録する。
* **tokenizer commitment を実値にする。** job はすでに `tokenizer_id: Hash64` を持つので、class の registry listing(「出品」)に tokenizer commitment を載せ、job の `tokenizer_id` との一致を要求する(この照合は fence で入れる P2)。
* Studio と gateway は sidecar から読む。chat template は sidecar のものを優先し、無ければ `wire` の既定を使う。generation config の既定値は gateway の既定値として使う(consensus には入れない)。
* 既存 artifact の digest・class id は変えない(sidecar は別 digest)。

### 2.10 LoRA / adapter(P3、新 class)

* **派生 class**:`base_class_id` と `adapter_digest` から派生 class id を作り、registry に出品として登録する。
* **実行**:整数の LoRA(`W·x + B·(A·x)`、量子化した低ランク行列)を決定的に計算する。panel の seat は adapter ファイルも保持する(readiness / possession proof の対象を広げる)。
* **価格と反証**:base と同じ形で、adapter の計算量を work vector に足す。
* 別 ADR。重みの差し替えではなく「別の class」として扱うことで、既存 class の identity を壊さない。

### 2.11 マルチモーダル(P3、新 class)

* **encoder class**:vision / audio encoder を独立した整数グラフとして登録し、出力の埋め込み列を LM class の入力に接続する「複合 job」にする。
* **最大の難所は前処理の決定性**(JPEG デコーダなどの実装差)。入力は「デコード済みの整数 tensor(例:RGB u8、固定解像度)」に限定し、その hash を job に commit する。デコードとリサイズはクライアント側の責任にする。
* 反証は同じ tile 開示の枠組みで、encoder の tile も開示対象にする。計算量が大きいので価格設計が要る。
* 別 ADR。

---

## 3. 優先順位とフェーズ

| フェーズ | 内容 | consensus | 目安 |
|---|---|---|---|
| **P0** | decode 規則の一括導入:D10(decode leaf の分子)+ D11(Gumbel-max サンプラーを全エンジンへ)+ §2.1 repeat_penalty + §2.3 logit_bias + §2.2 stop_token_ids。job は V4。fence は `palw_fp_decode_rules` を「decode rules v2」として 1 本で arming | あり(fence 1 本、flag day) | D10・D11・repeat_penalty は `rcore/fp-sampler` で実装開始。logit_bias・stop は本 RFC の承認後に同じ lane へ |
| **P1** | §2.6 KV prefix キャッシュ(段階 1)、§2.7 並行処理(複数 worker)、§2.4 n の fan-out、§2.8 embeddings(ローカル)、§2.9 artifact sidecar(読み込み側) | なし | ノード更新だけで出せる。KV キャッシュを最優先 |
| **P2** | §2.6 段階 2(prefix-state receipt、キャッシュ分を払わない)、§2.5 JSON Schema 拡張、§2.9 の tokenizer 照合、§2.4 の job group(必要なら) | あり | ADR-0145 §6 の実装 |
| **P3** | §2.10 adapter、§2.11 マルチモーダル、§2.8 embedding claim | あり(新 class) | それぞれ別 ADR |

**P0 を 1 本の fence にまとめる理由.** どれも「lane key を公開の量で補正する」同じ形の規則で、反証の形(2 tile + 公開の履歴)を共有する。別々に arming すると、「サンプラーはあるが反復する」「分子はあるが greedy のまま」といった中途半端な状態を設定で作れてしまう(params.rs が D10 と D11 を 1 本にしている理由と同じ)。

---

## 4. consensus・identity への影響

| 項目 | job | 新 fence | testnet-12 fp | 反証の形 | 検証コスト |
|---|---|---|---|---|---|
| D10 分子 | V3/V4 | `palw_fp_decode_rules` | arming 時に変わる | — | 変化なし |
| D11 サンプラー | V3 の seed/温度 | 同上 | 同上 | 2 tile | 1 lane あたり table 参照 1 回 |
| repeat_penalty | V4 | 同上 | 同上 | 2 tile + 履歴 window | lane ごとに出現数の参照 |
| logit_bias | V4 | 同上 | 同上 | 2 tile | 変化なし |
| stop_token_ids | V4 | 同上 | 同上 | token 列の照合のみ | 減る(早く止まる) |
| KV キャッシュ段階 1 | なし | なし | 変わらない | — | 変わらない |
| KV キャッシュ段階 2 | receipt 形状 | 新 fence | 変わる | ADR-0145 §6 | 新しい範囲だけ |
| JSON Schema 拡張 | constraint | 新 fence | 変わる | 2 tile + 許可集合 | オートマトンの遷移 |
| adapter / マルチモーダル | 新 class | 新 fence | 変わる | 同じ枠組み | class 次第 |

**rule manifest.** decode 規則は canonical な計算の一部なので、P0 の arming で rule manifest の digest が変わる可能性がある。変わる場合は identity(`consensus_identity_id`)に影響するので、fence の高さより前は旧規則、以後は新規則になるよう、manifest を fence で切り替える(DAA 750・1,300 の flag day と同じ手順で再 pin する)。

---

## 5. リスクと緩和

| リスク | 緩和 |
|---|---|
| 生成側と検証側の計算が食い違い、正直な producer が反証される | produce と replay が consensus-core の同じ関数を呼ぶ。seed・温度・penalty を掃く bit 一致テスト。fence を跨ぐドリルで実チェーンを確認 |
| 整数丸めの不一致(負の値・オーバーフロー) | 丸め方向を −∞ に固定、i64 の中間値、範囲外は受理時に拒否、境界値テスト |
| DoS(logit_bias の長さ、schema の状態数、stop 列、キャッシュのメモリ、並行接続) | すべてに上限を置き、上限超えは名指しで拒否(ADR-0096 の原則) |
| キャッシュ再開と先頭計算の不一致 | golden テスト必須。不一致ならキャッシュを使わない |
| 容量再設計(ADR-0160)との相互作用 | claim 数・報酬の分子が変わるので、D10 の arming は容量の flag day と同時にしない。報酬の総額は ADR-0160 の排出設計に従う |
| 2M の検証コスト | 2M の同時 claim 上限(C7 = 1)は ADR-0153 まで据え置き。KV キャッシュも 2M は 1 セッション想定 |

---

## 6. テスト・ドリル計画

1. **単体**:§2.1〜§2.3 の key 関数、丸め、境界値、identity 値での V3 一致。
2. **produce ⇔ replay の bit 一致**:seed × 温度 × penalty × bias × stop の組み合わせを、floor・8k class で掃く。
3. **反証**:規則を無視した token を commit した claim が、2 tile(+ 履歴)で反証され、正しい claim は反証されない。
4. **サービング**:prefix-resume == fresh、batch 内 == 単独、キャッシュの LRU とメモリ上限、並行接続の上限。
5. **ドリル**:salt 付きドリルで decode rules v2 の fence を低い高さで跨ぎ、seed・温度・repeat_penalty 付きの FP claim が bind → licence → Final まで進むこと、全ノードの状態が一致すること。
6. **公開チェーン**:flag day 後、fence 越えの FP claim の Final と、反証 0 件を監視する。

---

## 7. 未決事項(判断が必要)

1. **repeat_penalty の範囲**:`p_q` の上限(提案:4.0)、`repeat_last_n` の上限(提案:256)、プロンプトを window に含めるかの既定(提案:含めない)。
2. **stop の扱い**:token id 列だけを consensus に入れ、文字列は gateway の表示レイヤで扱う案でよいか。
3. **logit_bias と stop を P0 に入れるか**:同じ fence にまとめる案(推奨)か、D10・D11・repeat_penalty だけで先に出すか。
4. **KV キャッシュの段階 2**(キャッシュ分を払わない報酬)の時期:容量再設計の ×10 flag day より前か後か。
5. **embeddings の claim 化**(P3)をするか、ローカルサービスのままにするか。
6. **マルチモーダルの入力形**:デコード済み整数 tensor に限定する案でよいか。
