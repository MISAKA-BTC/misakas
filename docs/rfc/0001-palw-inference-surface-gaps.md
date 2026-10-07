# RFC-0001: PALW 推論サーフェスの欠落機能 — 決定論的な生成制御・サービング・入力拡張の設計

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


| 項目 | 値 |
|---|---|
| Status | **§A(FP Job V4 リリース)= Implementation Frozen**(2026-09-27、G0)。§0〜§7 の P1〜P3 は Draft(次リリース以降) |
| 対象 | testnet-12(R-core+)の free-prompt(FP)lane、`misaka-palw-gateway` / `misaka-palw-worker` / `misaka-palw-base0` / `misaka-palw-constraint`、Studio |
| 関連 | ADR-0082 D10/D11(decode の分子とサンプラー)、ADR-0096(OpenAI 互換サーフェス)、ADR-0144(使う推論に払う)、ADR-0145 §6(キャッシュは実行事実)、ADR-0077 D1(常駐 worker)、ADR-0153(2M 分割)、ADR-0160(容量再設計) |
| リリース列車 | `rcore/fp-sampler`(§A の範囲だけ。bisect できる小さな commit を積み、設計・統合・検証・公開は 1 つのリリースとして扱う) |

---

## 将来profileの検証境界（2026-10-06）

[ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)に従い、モデル拡張は
active Kernelの宣言的plan、またはversioned Kernelの合意更新で行う。モデル用VMは実装しない。
凍結済みdecode算法と既存claimの規則は変更しない。将来の大型モデルprofileでは
[RFC07 Part V](0007-palw-verification-certificates-and-algebraic-checks.md) / [RFC11 §15](0011-permissionless-model-and-long-context-onboarding.md)
の小さい確率的constraint検査を通常経路とする。logitsからpenalty、bias、constraint、sampler、stop、
出力までstatementにbindし、未検査のdecode段階を残さない。生成用の決定論的乱数は検証用の
post-commit challengeではない。異常時のみ有界exact courtへ局所化する。reference replayとgolden
vectorsは適合確認として残す。この文書改訂で新profileを有効化しない。

## §A. リリース「FP Job V4 — Deterministic Decode Pipeline」(Implementation Frozen)

**この節の算法・表現・順序は凍結されている。** 実装中に変える場合は「コードを仕様に合わせる」のではなく、本 RFC の変更として明示し、golden vectors も同時に改訂する。

### A.1 範囲

含める:repeat_penalty、frequency / presence penalty、logit_bias、stop token sequences、ADR-0082 D10(decode leaf の分子)・D11(Gumbel-max サンプラー)、constraint を含む processor の順序。
含めない(次リリース以降):KV キャッシュ、並行処理、embeddings、マルチモーダル、LoRA/adapter、JSON Schema の subset 拡張。

### A.2 `DecodeConfigV4`(規範)

```
DecodeConfigV4 {
    repeat_penalty_q:   u32,              // Q16 の有理数 p/2^16。65536 = 1.0 = 無効。許容 [65536, 262144](最大 4.0)
    penalty_window:     u16,              // W。penalty がすべて無効なら 0、有効なら 1..=256
    frequency_penalty_q: i32,             // Q24(logit 単位)。許容 [-2·2^24, 2·2^24]
    presence_penalty_q:  i32,             // Q24(logit 単位)。許容 [-2·2^24, 2·2^24]
    logit_bias:         Vec<(u32, i32)>,  // (token_id, bias_q)。token_id の狭義昇順(重複なし)、最大 300 件
                                          // bias_q は Q24(logit 単位)、許容 [-100·2^24, 100·2^24]
                                          // bias_q == -100·2^24 は「禁止」(hard mask)。0 の項目は置かない
    stop_sequences:     Vec<Vec<u32>>,    // token id 列。最大 4 本、各 1..=16 token。重複なし、辞書順に整列
}
```

* **no-op の正規形はただ 1 つ**:`repeat_penalty_q = 65536`、`penalty_window = 0`、`frequency_penalty_q = 0`、`presence_penalty_q = 0`、`logit_bias = []`、`stop_sequences = []`。同じ意味に複数の符号化を許さない(同じ挙動の job が別の job id を持たないため)。**`penalty_window` は、3 つの penalty のいずれかが無効値でないときだけ 1..=256 で、そうでなければ 0。**
* 範囲外・並び順違反・重複・空の列・0 の bias 項目は、job の受理時に**名指しで拒否**する(ADR-0096 の原則)。

### A.3 processor の定義(規範)

位置 t の decode ステップで、エンジンが出す lane j の整数 logit を `v_j`(`i32`、クラスの固定小数点 Q24)とする。**生成済み token 列**(プロンプトは含めない)のうち直近 `W = penalty_window` 個の中の token j の出現数を `c_j(t)` とする。演算はすべて `i64` で行う。

```
1. repeat(乗算型、同じ token が window に複数回あっても 1 回だけ適用):
     c_j(t) > 0 かつ v_j > 0 :  a_j = floor( v_j · 65536 / p_q )
     c_j(t) > 0 かつ v_j ≤ 0 :  a_j = floor( v_j · p_q / 65536 )           // −∞ 方向(div_euclid)
     c_j(t) = 0              :  a_j = v_j
2. frequency / presence(同じ window):
     b_j = a_j − c_j(t) · f_q − [c_j(t) > 0] · s_q
3. logit_bias(禁止項目を除く):
     d_j = b_j + bias_j                                                   // 項目がなければ bias_j = 0
4. 飽和:
     v''_j = clamp(d_j, i32::MIN + 1, i32::MAX)
5. constraint(最終の hard mask):
     許可集合 A(t) = { j : constraint(response_format のオートマトン)が許す } ∖ { logit_bias で禁止の j }
     A(t) が空なら、そのステップで生成は終わる(理由を記録)
6. selection:
     committed_t = argmax_{j ∈ A(t)} decode_lane_key_v2(v''_j, seed, t, j, temperature_q)   // ties は最小 index
7. stop:
     committed 列の末尾がいずれかの stop 列に一致したら、そのステップで生成を終える(stop 列の token は回答に含む。
     表示で切るのは gateway)。一致しなければ decode_token_limit まで続ける
```

* **frequency / presence の window は repeat と同じ W**(本 RFC の判断。OpenAI は全生成 token を数えるが、court の数え上げを W に抑えるため)。
* **stop 文字列**は consensus に入れない。gateway が各文字列をその tokenizer 単独のエンコードで token 列に変換して `stop_sequences` に入れ、変換できない文字列は拒否する。文脈によって別の分割で生成された場合は一致しない(止まらない)ことを応答に明記する。
* **反証(I-2)**:1〜4 の補正は「lane j の値」と「job・生成済み列から公開で決まる量」だけの関数なので、court は committed lane と beating lane の 2 tile、window 分の生成済み token 列、bias・mask の該当項目を開示すれば足りる。
* **D10**:free-prompt claim の quanta は実行した decode leaf で数える(prefill は 0)。stop で早く止まった分は払わない。
* **D11**:`temperature_q = 0` の V4 はサンプラーの雑音項が消え、greedy と同じ選択になる。
* **D11 の key(2026-10-01 訂正。独立第二実装の発見 G1)**:`decode_lane_key_v2(v, seed, t, j, T_q) = v · 2^24 + T_q · G_j`。`v` は Q24 の logit、`T_q` は Q24 の温度、`G_j` は Q24 の Gumbel 変数で、**両項とも Q48**。右シフトは無い。実装は一時期 `v · 2^24 + ((T_q · G_j) >> 24)` と書かれており(雑音が Q24、logit 項が Q48 で 2^24 倍小さい)、温度が実質無効だった(fp-v4 の 24 lane の行 12 本・T = 1.0・3,600 抽選で非 greedy 0 件。訂正後は約 65 %)。訂正後の key は `argmax_j (v_j + T·g_j) = argmax_j (v_j / T + g_j)` で、実数の logit・温度に対する Gumbel-max、つまり `softmax(v / T)` からの抽選である(統計検定: `palw_decode_select_v2` の test、T ∈ {0.5, 1, 2})。`T_q = 0` の key は `v · 2^24` のままで、greedy の選択は 1 bit も動かない。`palw_fp_decode_rules` は全 preset で休眠なので、どの網の挙動も変わらない。key は logit の単位が Q24 であることを前提にする(上の冒頭: `v_j` は「クラスの固定小数点 Q24」)。**IR・生成 text クラスの logit の単位(2026-10-01 決定、RFC-0003 §I.3.5、spec 04b §15.12)**:text program の `logits` は自然対数単位 × 2^24(Q24)で commit する、というのが lowering の保証であり、class の宣言する field ではない(tir-lower が較正した logit scale は既存の IR クラスで 5.3·10^-9 … 7.2·10^-8 の任意の実数で、チェーンは単位を検査できない)。temperature・frequency/presence penalty・logit_bias は単位に依存するので、IR・生成クラスのうち、free-prompt lane を IR・pipeline クラスに開く後続の fence **より前**に登録されたものには提供しない(job は名指しで拒否される: V4 は walk が skip、V5 は受理で拒否。`PalwFpV3Error::DecodeControlNeedsQ24Logits`。`palw_fp_decode_rules` と同じく休眠)。greedy・repeat penalty(Q16 の比で単位に依存しない)・stop・constraint は全クラスに提供する。後続の fence は、**それ以降**に登録された IR・生成クラス(lowering が Q24 を保証する)にだけ全 control を提供する。それ以前に登録された class(稼働中の SmolLM2 など)は拒否のまま。

### A.4 Job V4(wire)

* `PalwFreePromptJobV4` = V3 の全フィールド + `decode: DecodeConfigV4`。`fp_job_id_v4` は V3 と別のドメイン分離タグで borsh 全体を hash する。
* **有効化の境界(G10)**:fence `palw_fp_decode_rules`(D10・D11 と同じ 1 本)の高さ H 以降、新しい FP job は V4 だけを受理する。H より前に受理された V3 の claim は、H 以降も **V3 の検証器**で最後まで検証する(V3 のコードは次の cleanup リリースまで残す)。
* **互換性(G7)**:no-op 正規形の V4 job は、同じ V3 job と**同じ token 列・同じ work**を出す。これが大量の入力で成り立たない限り flag day に進まない。

### A.5 consensus vectors(仕様の実行可能版)

`consensus-vectors/fp-v4/` に置き、sampler(consensus-core)・worker(produce)・panel(replay)のテストが**すべて同じファイルを読む**。

```
repeat_penalty.json      frequency_penalty.json   presence_penalty.json
logit_bias.json          stop_sequences.json      processor_order.json
job_v4_encoding.json     // 入力フィールド → 期待される borsh バイト列 → 期待される job hash
v4_noop_equals_v3.json   // 同じ入力で V3 と V4 no-op の token 列が一致する例
```

### A.6 関門(gate)と commit 計画

| Gate | 内容 | 通過条件 |
|---|---|---|
| G0 | RFC freeze | §A に未決事項なし(本節) |
| G1 | sampler | 純粋・決定的な processor の実装(consensus-core) |
| G2 | vectors | golden vectors 全通過 |
| G3 | wire | Job V4 / borsh / hash |
| G4 | execution | worker が V4 を実行 |
| G5 | verification | panel が同じ claim を再現 |
| G6 | gateway | API → 正規形 V4 への変換 |
| G7 | compatibility | V4 no-op == V3 |
| G8 | rehearsal | staging(salt 付きドリル)で flag day を再現、新旧 worker・新旧 job の境界 |
| G9 | release | tag・binary・文書の公開 |
| G10 | activation | V3 → V4 の fence を越える |

commit(bisect できる粒度):`01 spec: freeze DecodeConfigV4` → `02 sampler: canonical penalty representation` → `03 repeat penalty` → `04 frequency/presence` → `05 logit bias` → `06 stop matcher` → `07 processor total ordering` → `08 test: consensus golden vectors` → `09 wire: FP Job V4` → `10 worker: execute V4` → `11 panel: verify V4` → `12 gateway: normalize to V4` → `13 test: V4-noop == V3` → `14 docs: migration / release / activation`。

### A.7 公開と有効化は分ける

コード公開 → tag・binary 公開 → ノード更新期間 → 更新率と動作の確認 → 有効化 fence。**リリースと有効化を同時にしない。**

有効化の必須条件(すべて):golden vectors 全通過、V4 no-op == V3、worker == panel、繰り返し実行で bit 一致、staging での flag day 予行の成功、新旧混在の境界テスト通過、release binary の再現性。

### A.8 rollback

* 有効化前:V4 対応 binary → 旧 binary に戻せる(kit の `upgrade-rollback`)。
* 有効化後:V3 へは戻さない。問題が出たら、V4 の緊急パッチ(node)または、重大なら V5 の緊急 fence で直す。

### A.9 公開物(実装作業に含める)

RFC-0001(本書)、必要な決定だけの ADR、Job V4 wire spec、consensus vectors、migration guide、release notes、有効化パラメータ、operator 向け更新手順(「何をいつまでに更新すればよいか」だけの 1 ページ)、rollback / 緊急手順。

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

> **§2.1〜§2.3・§2.5 の生成制御は、§A(凍結仕様)が優先する。** 以下はその検討経緯で、食い違う箇所(プロンプトを window に含めるフラグ、stop 列の長さなど)は §A の値が正。

### 2.0 MINING レーンと Chat パスの線引き(2026-09-27 ユーザー確認)

以下の各項目を「chain の規則が要るか」で読むときの線は、**MINING レーン**か**非 MINING の Chat パス**かで引く。

| | MINING レーン | Chat パス(非 MINING) |
|---|---|---|
| 実体 | `misaka-palw-gateway` の FP レーン:job を commit し、claim を出し、panel が再実行し、court が審理する | 同じ worker・artifact でローカルに答えるだけ:job も claim も報酬もない |
| 出力を変える制御(stop・logit_bias・sampling・penalty) | **chain の規則**(§A:FP Job V4、fence `palw_fp_decode_rules`、seat の再実行、court) | ノードの実装だけ(chain 変更なし)。§A と同じ関数を使ってよいが、何も commit しない |
| 会話をまたぐ KV キャッシュ再利用 | **chain 変更は不要**:`palw_canonical_work_v1.rs` がすでに `reused_prefix_tokens` を価格に入れ、chain は払済みの prefix を自分の記録から読む(ADR-0145 §6、§2.6)。ノードの高速化は committed 計算を 1 bit も変えない限り自由 | 自由(ノードのみ) |
| 並行処理(§2.7) | ノードのみ(1 推論 = 1 claim のまま) | ノードのみ |
| embeddings(§2.8) | claim 化するなら新 job 種別(P3、別 ADR) | ローカルサービスとして自由(chain 変更なし) |
| artifact の自己記述(§2.9) | 新しい artifact 形式と、**通常の permissionless なクラス登録**(新しい artifact は新しいクラス。既存クラスの identity は変えない) | 同左の artifact を読むだけ |

つまり、**chain の規則として入れるのは「MINING レーンで committed token を変えるもの」だけ**で、それは §A(FP Job V4)で完結している。KV キャッシュ再利用・並行処理・embeddings・Chat での stop / logit_bias / sampling・artifact の自己記述は、どれも chain 変更なしで入る(artifact 形式の追加は既存のクラス登録の手続きで扱う)。

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

### 7.1 採択記録(2026-10-03、lane U / `rfc1/serve`)

1〜3 は P0(§A、Job V4 / fence `palw_fp_decode_rules`)で実装済み。4〜6 を次のとおり採択し、実装した(いずれも休眠 fence、どの preset でも arm しない)。

4. **KV キャッシュ段階 2 は今、専用の休眠 fence で**:`palw_fp_prefix_state`(FP Job version 11、`PalwFpPrefixStateV1` を carry)。段階 1(node のみ)は answer-only 経路で出荷済み。報酬側(credit)のみで、fold は全 prefill leaf を ctx 束縛 hash で畳むままなので、**キャッシュした prefix は commit 実行を速くしない**(段階 2b「継承 leaf」は残課題)。decode rules が有効な状況では D10 が既に prefill を 0 として credit するため、version 11 が価格を動かすのは「宣言された状態を seat が検証する」点であり、価格差ではない(e2e で等値を確認)。
5. **embeddings の claim 化は RFC-0003 の Embedding profile で**:ローカル `/v1/embeddings` は claim 無しのまま、claim 形は `PalwGenJobV1`(`Embedding` body)。gateway に job 構築器と `misaka.claim` の名前付き拒否を置いた(free-prompt レーンの worker しか持たない gateway は拒否する)。
6. **マルチモーダルはデコード済み整数 tensor のみ**:`palw_image_tensor` 部品(`u8` HWC RGB の hex)だけを受け、URL・エンコード済み画像は名前付きで拒否。画像は FP Job V5 の slot 参照(`input_root`)になる。encoder を持たない class の背後では実行時に名前付きで拒否する。

追加した休眠 fence:`palw_fp_prefix_state`(§2.6 段階 2)、`palw_fp_tokenizer_match`(§2.9)、`palw_fp_constraint_v2`(§2.5、前提の `palw_fp_decode_constraint` が未実装のため arm 不能)、`palw_adapter_class_v1`(§2.10、ADR-0163、object tag 94)。

### 7.2 追記(2026-10-03、lane U 第 2 弾)

- **ADR-0096 D6-8(`palw_fp_decode_constraint`)を実装した**:FP job version 6(V3 job + constraint bytes)、producer と seat の replay が同じ mask(class の token table 経由)を通る、`validate_palw_v2` の arm 拒否を解除、drill 旗 `--palw-drill-fp-constraint-at`、gateway の `response_format` は fence 有効網で committed(version 6)になる。これで `palw_fp_constraint_v2` を drill で arm できる。**未了**:court の第 3 arm(`check_tiled_decode_token_refutation_v3` は純関数として既存)を wire 上の proof kind にすること、`render_answer_v2`(D6)、kaspad seat への token table の配布(table が無い seat は `Unverifiable` で棄権)。
- **(2026-10-03 夜 訂正: 段階 2b は実装済み・下の §7.3 を参照。以下は実装前の記述)** KV キャッシュ段階 2b(継承 leaf)は未実装:commit の leaf は job context 全体に束縛されるため、cache 済み prefix の leaf の継承には「prefix context」を全 leaf 束縛関数と court の binding に通す必要がある(consensus の再設計)。行(per-position rows)を cache する案は model サイズで 1 position あたり GB 級で不成立。段階 2(受領証)は credit 側のみで、decode rules 有効下では D10 が既に prefill を 0 とするため価格差は出ない。
- 画像・embedding claim は、tiny な in-repo vision class(toy VLM / toy vision encoder)で gateway → FP Job V5 / tensor job → worker → seat の in-process e2e を通した(`misaka-palw-gateway/src/tensor.rs` tests)。実 class(vision tower)は tir/generic の統合待ち。

### 7.3 追記(2026-10-03 夜、lane U 第 3 弾)— 段階 2b と運用

- **段階 2b(FP job version 12、dormant fence `palw_fp_prefix_inherit`)**:prefix 位置の step leaf を job 非依存の prefix context(version-3 job context、`job_nullifier` = k、`assignment_id` = prefix state root、`step_tile_leaf_hash_ctx_v1`)で hash する。同じ prefix を共有する job の継承 leaf は byte 単位で一致し、再計算した root と等しく、継承範囲の嘘は先に同じ leaf を commit した claim で反証できる。V2・V11 の context と leaf は 1 bit も変わらない。fence は `palw_fp_prefix_state` を前提に持つ。
- **admission(必須の安全規則、acceptance で名指し拒否)**:version 12 の claim は、leaf が継承に安全と証明された class にだけ載る。条件は「held でない」「IR・生成 class でない」「fused-attention の court window を持たない」「shape profile が公開済みで整数 lane(`Int32`)」「KV aux series を持たない(`kv_chunk_calls == 0`)」。それ以外(fused attention、KV aux、未証明の kind)は walk が `an inherited-prefix claim (FP job version 12) over a class whose leaves are not inheritance-safe …` で skip する(`palw_fp_prefix_inherit_class_safe_v1`、`PalwChainStateV2::class_prefix_inherit_safe_v1`)。checkpoint の leaf は継承せず job ごとの context で hash する(整数 floor 上で seat の interval replay が Valid になることを e2e で確認)。
- **token table の自動読み込み**:seat は class artifact の隣の `<artifact>.palwtokens`(`palw-a16-fp-worker --emit-token-table` の出力)を起動時に登録する。table は自分の bytes から導いた root で登録され、制約付き claim は job が commit した `table_root` で table を選ぶので、root の合わない sidecar はその claim には使われない(seat は `Unverifiable` で棄権)。`--palw-token-table` は上書き・追加として残る。
- **drill 用 producer**:`misaka-palw-rfc1-drill-claim`(floor class 上で constraint / constraint2 / prefix / inherit の claim を実走して outbox に書く)+ `scripts/misaka-palw-rfc1-drill-claims.sh`(rail で署名・submit)。adapter 一覧の producer は composite class が要るため floor 版はない。
- **残(最適化・凍結後)**:executor の計算削減、seat 側の prefix-leaf cache。

## Mission alignment amendment — 2026-10-07

§Aと§2のdecode、artifact、KV reuse、adapter、multimodal拡張は、それぞれ公開の入力・state境界・version・outputと裁定証拠を拘束する。ローカルChatの非claim機能や将来profileのsource fidelityは、公開訴追可能性の代わりにならない。新しい報酬profileの受入にRFC14の外部public-bond試験を追加する。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](0014-panel-independent-fraud-prosecution.md)、[RFC15](0015-panel-free-permissionless-verification.md)を参照する。
