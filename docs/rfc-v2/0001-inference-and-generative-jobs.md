# RFC-v2-0001 — 推論・生成Job・入出力

## 2026-11-09公開資料・一人前提の追加境界

[採択RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)と[ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md)により、独立選出されたPanelの中の一人が、モデルを事前保有せずに公開資料を取得して全体検証できる新profileを採択する。公開promptの原文、canonical token列、tokenizer・前処理の対応を検査対象へ結ぶ。入力・初期依存を隠したclaimを成功へ進めない。 成功の規範所有者は[20](0020-verification-gated-final-and-adversarial-models.md)、公開方針は[v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)。以下の先行採択・基礎契約と衝突する箇所は、この新profileに限って本境界と16〜20を適用する。旧claim・歴史付録は保持し、状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

## 2026-11-09新profileの追加境界

[採択RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)と[ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)により、[20](0020-verification-gated-final-and-adversarial-models.md)の実行整合性・一人の反証・検証未成立時の成功停止を適用する。job、tokenizer、decode、seed、入力と出力の意味をMinerの固定実行へ束縛する。 詳細の規範所有者は16〜20であり、本追記で別の成功条件を定義しない。旧claim・歴史付録の意味とbytesは保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

原統合: 2026-10-29 / 採択改訂: 2026-11-08 / 原入力: `misakas-pre(5).zip`

[20本の索引](README.md) · [旧→新対応表](MAPPING.md) · [編集・移行規則](MIGRATION.md)

> **2026-10-29原統合の編集範囲:** 文書の統合・重複する責務の整理。技術仕様の採択、パラメーター変更、runtime有効化、試験成功の宣言ではない。
> **状態:** 採用済み・Draft・隔離実装・履歴を含む統合編集版。各条項の採択・置換関係を継承する。過去の「実装済み」「休眠」「未試験」等は、その記録時点の状態である。
> **番号:** 本書は`RFC-v2`。原条文中の`RFC-xxxx`と`§`は旧版を指す。旧番号・wire・署名・fence名は再採番しない。

## 2026-11-08採択による適用範囲

jobのモデル・program・入力・seed・出力・依存関係を、[16](0016-miner-execution-commit-and-material-supply.md)のMiner originalに束縛する。推論機能と検証成功を区別し、結果を出しただけで[20](0020-verification-gated-final-and-adversarial-models.md)の成功Finalへ進めない。

**優先関係:** 2026-11-08の先行採択はADR-0215と公開裁定v0.2を根拠とする。今回の公開資料・一人前提の新profileには冒頭の0050／0218・RFC16〜20・[公開裁定v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)を優先する。以下の基礎契約・旧profile記録と原付録は保持し、旧署名や旧Finalの意味を遡及変更しない。設計採択は実装・測定・有効化の完了を意味しない。

**原文収録元:** [旧0001](../rfc/0001-palw-inference-surface-gaps.md)、[旧0003](../rfc/0003-palw-generative-model-classes.md)。

**関連RFC:** [02 TIR・Kernel・共有Queryデータ](0002-tensor-ir-and-shared-query-data.md) / [03 モデル登録・変換・Artifact](0003-model-onboarding-and-artifacts.md) / [06 Panel参加・抽選・確率的検証・認証Receipt](0006-panels-and-probabilistic-verification.md) / [07 Claim・EXEC・仕事量・回答会計](0007-claims-exec-and-answer-accounting.md) / [09 Remote Client・SDK・非保管型操作](0009-remote-client-and-sdk.md) / [15 設計凍結・延期・不採用・未採択・変更管理](0015-freeze-deferred-rejected-and-governance.md)。

## 1. このRFCの責務

推論リクエストを、再現可能な入力・生成条件・出力へ変換する共通契約を扱う。旧0001の推論サーフェスと、旧0003の共通job／text profileを一つにする。TIRの命令意味は02、モデルの登録は03、計算の支払済み判定は07、裁判手続きは14が所有する。

## 2. 一つのJobとして固定するもの

モデル／classの不変identity、入力、生成条件、乱数の導出、出力形式、実行profileを拘束する。生成制御、stop条件、sampling、複数候補、構造化出力は、受理したversionが定義する範囲で決定論的に処理する。自由なprovider設定で合意対象の意味を変えない。

入力・出力・tensorのcanonical化は、同じbytesが同じ意味を持つことと、不正な形式を拒否することを分けて規定する。ハッシュや署名が一致することを、出力が正しい計算結果であることの証明とは呼ばない。

## 3. 共通層とprofileの分離

共通層はjob形、決定論的乱数、canonical tensor、出力、free-prompt carriageである。text profileを中心に読む。embedding、音声、vision入力等は、原文の個別profileと承認範囲を保持する。今回の統合で新たに提供を承認しない。

画像生成・動画生成は15の延期台帳に移す。旧0003全体を停止したとは解釈しない。一方、共有fenceを開くことで画像・動画を一緒に有効化してはならない。

## 4. 検証との接続

生成結果だけでなく、報酬対象となる入力、state、出力、job制約に検査関係を対応付ける。claimの認証は06／10、不一致の局所化・判決は14に従う。profileが増えたことだけで、既存の検査器が新しい演算まで被覆したと扱わない。

## 5. 受入確認

同一入力の再現、乱数domain、停止条件の境界、無効なschema／profileの拒否、textの回帰、延期profileの通常入口・代替入口での拒否を確認する。ここに列挙した試験は完了条件であり、今回実行した結果ではない。

---

## 原条文の統合収録

以下は内容を落とさないための原条文である。見出しの深さとリンク先だけを編集し、数式・コード・値・記録を再解釈していない。旧い方針・候補・試験記録も含むため、本文の責務・適用範囲と15の台帳を先に読む。原文内の相互参照は保存した旧ファイルへ戻る。新しい編集先は本書先頭の20本の索引・旧新対応表で確認する。

### 収録箇所

| 原文 | 行 | 入口 |
| --- | --- | --- |
| 旧0001 | 1–431 | [RFC-0001: PALW 推論サーフェスの欠落機能 — 決定論的な生成制御・サービング・入力拡張の設計](#source-v1-0001-L00001) |
| 旧0003 | 1–1070 | [RFC-0003: PALW Generative Model Classes — one job, determinism and output layer, and the class ](#source-v1-0003-L00001) |
| 旧0003 | 1794–2252 | [II.2 Text generation](#source-v1-0003-L01794) |
| 旧0003 | 2290–2621 | [Rules, alternatives, security and activation](#source-v1-0003-L02290) |

---

<a id="source-v1-0001-L00001"></a>

### 旧RFC-0001 / 原文1–431行

出典: [0001-palw-inference-surface-gaps.md](../rfc/0001-palw-inference-surface-gaps.md)。配置理由: 主題別統合。

### RFC-0001: PALW 推論サーフェスの欠落機能 — 決定論的な生成制御・サービング・入力拡張の設計

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


| 項目 | 値 |
|---|---|
| Status | **§A(FP Job V4 リリース)= Implementation Frozen**(2026-09-27、G0)。§0〜§7 の P1〜P3 は Draft(次リリース以降) |
| 対象 | testnet-12(R-core+)の free-prompt(FP)lane、`misaka-palw-gateway` / `misaka-palw-worker` / `misaka-palw-base0` / `misaka-palw-constraint`、Studio |
| 関連 | ADR-0082 D10/D11(decode の分子とサンプラー)、ADR-0096(OpenAI 互換サーフェス)、ADR-0144(使う推論に払う)、ADR-0145 §6(キャッシュは実行事実)、ADR-0077 D1(常駐 worker)、ADR-0153(2M 分割)、ADR-0160(容量再設計) |
| リリース列車 | `rcore/fp-sampler`(§A の範囲だけ。bisect できる小さな commit を積み、設計・統合・検証・公開は 1 つのリリースとして扱う) |

---

#### 将来profileの検証境界（2026-10-06）

[ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)に従い、モデル拡張は
active Kernelの宣言的plan、またはversioned Kernelの合意更新で行う。モデル用VMは実装しない。
凍結済みdecode算法と既存claimの規則は変更しない。将来の大型モデルprofileでは
[RFC07 Part V](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md) / [RFC11 §15](../rfc/0011-permissionless-model-and-long-context-onboarding.md)
の小さい確率的constraint検査を通常経路とする。logitsからpenalty、bias、constraint、sampler、stop、
出力までstatementにbindし、未検査のdecode段階を残さない。生成用の決定論的乱数は検証用の
post-commit challengeではない。異常時のみ有界exact courtへ局所化する。reference replayとgolden
vectorsは適合確認として残す。この文書改訂で新profileを有効化しない。

#### §A. リリース「FP Job V4 — Deterministic Decode Pipeline」(Implementation Frozen)

**この節の算法・表現・順序は凍結されている。** 実装中に変える場合は「コードを仕様に合わせる」のではなく、本 RFC の変更として明示し、golden vectors も同時に改訂する。

##### A.1 範囲

含める:repeat_penalty、frequency / presence penalty、logit_bias、stop token sequences、ADR-0082 D10(decode leaf の分子)・D11(Gumbel-max サンプラー)、constraint を含む processor の順序。
含めない(次リリース以降):KV キャッシュ、並行処理、embeddings、マルチモーダル、LoRA/adapter、JSON Schema の subset 拡張。

##### A.2 `DecodeConfigV4`(規範)

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

##### A.3 processor の定義(規範)

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

##### A.4 Job V4(wire)

* `PalwFreePromptJobV4` = V3 の全フィールド + `decode: DecodeConfigV4`。`fp_job_id_v4` は V3 と別のドメイン分離タグで borsh 全体を hash する。
* **有効化の境界(G10)**:fence `palw_fp_decode_rules`(D10・D11 と同じ 1 本)の高さ H 以降、新しい FP job は V4 だけを受理する。H より前に受理された V3 の claim は、H 以降も **V3 の検証器**で最後まで検証する(V3 のコードは次の cleanup リリースまで残す)。
* **互換性(G7)**:no-op 正規形の V4 job は、同じ V3 job と**同じ token 列・同じ work**を出す。これが大量の入力で成り立たない限り flag day に進まない。

##### A.5 consensus vectors(仕様の実行可能版)

`consensus-vectors/fp-v4/` に置き、sampler(consensus-core)・worker(produce)・panel(replay)のテストが**すべて同じファイルを読む**。

```
repeat_penalty.json      frequency_penalty.json   presence_penalty.json
logit_bias.json          stop_sequences.json      processor_order.json
job_v4_encoding.json     // 入力フィールド → 期待される borsh バイト列 → 期待される job hash
v4_noop_equals_v3.json   // 同じ入力で V3 と V4 no-op の token 列が一致する例
```

##### A.6 関門(gate)と commit 計画

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

##### A.7 公開と有効化は分ける

コード公開 → tag・binary 公開 → ノード更新期間 → 更新率と動作の確認 → 有効化 fence。**リリースと有効化を同時にしない。**

有効化の必須条件(すべて):golden vectors 全通過、V4 no-op == V3、worker == panel、繰り返し実行で bit 一致、staging での flag day 予行の成功、新旧混在の境界テスト通過、release binary の再現性。

##### A.8 rollback

* 有効化前:V4 対応 binary → 旧 binary に戻せる(kit の `upgrade-rollback`)。
* 有効化後:V3 へは戻さない。問題が出たら、V4 の緊急パッチ(node)または、重大なら V5 の緊急 fence で直す。

##### A.9 公開物(実装作業に含める)

RFC-0001(本書)、必要な決定だけの ADR、Job V4 wire spec、consensus vectors、migration guide、release notes、有効化パラメータ、operator 向け更新手順(「何をいつまでに更新すればよいか」だけの 1 ページ)、rollback / 緊急手順。

---

#### 0. 要約

現在の FP lane は「宣言した長さまで、greedy で、1 リクエスト 1 エンジンで、テキストだけを」生成する。欠けているものは 3 系統に分かれる。

* **生成制御**(サンプリング、repeat_penalty、stop、logit_bias、n、自由な JSON Schema):出力 token を変えるので **consensus の規則**。job に commit し、panel が再実行で同じ token 列を得られ、court が「tile 2 枚の開示」で反証できる形でしか入れられない。
* **サービング**(会話をまたぐ KV キャッシュ、並行処理、embeddings、artifact の同梱):committed 計算を 1 bit も変えない限り **ノードだけの変更**で入る。報酬への反映(キャッシュ分を払わない)は ADR-0145 §6 がすでに規則として持つ。
* **入力拡張**(マルチモーダル、LoRA/adapter):class が「token id だけを読む、重み固定の実行グラフ」として定義されているので、**新しい class 設計**(ADR)が要る。

優先順位は、**P0** = decode 規則の一括導入(D10 + D11 + repeat_penalty + logit_bias + stop_token_ids、1 本の fence)、**P1** = ノードだけのサービング改善(KV prefix キャッシュ・並行処理・n の fan-out・embeddings・artifact sidecar)、**P2** = consensus の経済変更(prefix-state receipt、JSON Schema 拡張、n の job group)、**P3** = 新 class(adapter、マルチモーダル、embedding claim)とする。実用上いちばん効くのは P1 の KV prefix キャッシュ。

---

#### 1. 前提 — PALW が守る 3 つの不変条件

以後のすべての設計は、次の 3 条件を満たすかで判定する。

| # | 不変条件 | 意味 |
|---|---|---|
| **I-1** | **出力を変えるものは job に commit され、再実行で再現する** | panel の seat は producer と同じ job を再実行し、同じ token 列を得る。乱数・時刻・ノード状態に依存する選択は入れられない。整数(Q24)カーネルなので、同じ入力なら bit 一致する。 |
| **I-2** | **反証は per-lane の「tile 2 枚の開示」で済む** | court は committed lane と、それを上回るはずの lane の logits tile 2 枚を開き、2 つの key を再計算して比べる(ADR-0049 Decision E)。行全体(Qwen 級の語彙で約 993 KB)を要する規則は `palw_close_budget` が拒否する。**したがって、lane j の key は「lane j の値」と「job・committed 履歴から公開で決まる量」だけの関数でなければならない。** softmax 正規化・top-p・min-p のような「行全体を見る」規則はこの形にならない。 |
| **I-3** | **キャッシュ・並列は実行上の事実であり、claim ではない** | ノードは committed 計算を変えない最適化を自由にしてよい。報酬は derived work(ADR-0145)で決まり、キャッシュで省いた計算は払わない(§6)。「キャッシュに当たった」という申告は入力にならず、検証者は新しい計算だけを再構成する。 |

##### 1.1 分類表

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

#### 2. 項目別の設計

> **§2.1〜§2.3・§2.5 の生成制御は、§A(凍結仕様)が優先する。** 以下はその検討経緯で、食い違う箇所(プロンプトを window に含めるフラグ、stop 列の長さなど)は §A の値が正。

##### 2.0 MINING レーンと Chat パスの線引き(2026-09-27 ユーザー確認)

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

##### 2.1 決定論的 repeat_penalty(新規実装・P0)

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

##### 2.2 stop(停止条件・P0)

**現状.** ADR-0096 Decision 4 で拒否。宣言した budget まで生成し、利用者が切る。

**設計.** 文字列の stop は tokenizer の分割に依存するので consensus に入れない。代わりに **token id 列の stop**(`stop_token_ids: Vec<Vec<u32>>`、最大 4 本、各 8 token まで)を V4 に入れる。

* 規則:生成は `decode_token_limit` を上限とし、committed 列の末尾がいずれかの stop 列に一致した位置 k で止まる。以後の leaf はない。
* 反証:停止位置は公開の token 列から自明に検証できる(「一致していないのに止まった」「一致していたのに続けた」の 2 種類)。tile の開示は要らない。
* 報酬:D10 の分子は実行した decode leaf なので、早く止まった分は払わない(budget は上限、work は実測)。
* 文字列 stop:gateway が表示レイヤで扱う(該当文字列以降を返さない)。同時に、gateway は文字列を「確実に含む token 列」の候補へ変換して `stop_token_ids` に入れることもできるが、分割の揺れで止まり損ねうることを応答に明記する。

##### 2.3 logit_bias(P0)

**設計.** V4 に `logit_bias: Vec<(u32 token_id, i32 bias_q)>`(最大 300 件、提案値)を入れる。`bias_q` は OpenAI の `[-100, 100]` を Q24 に写し、`-100` は「禁止」フラグとして扱う(key を最小値に固定)。

* `v_j' = v_j + bias_j`(repeat 補正の前に適用)。`bias_j` は job から公開で決まるので、I-2 を満たす。
* 重複 id・範囲外は受理時に名指しで拒否する(ADR-0096 の原則)。

##### 2.4 n > 1(P1 → P2)

**原則.** 「1 推論 = 1 claim」は維持する。n 候補は n 本の claim。

* **P1(consensus 変更なし)**:gateway が同じプロンプトで n 本の job を出す。`sampling_seed_i = H(base_seed ‖ i)` とし、応答の `choices` にまとめる。§2.6 の prefix キャッシュで、prefill の実計算は 1 回で済む。
* **P2**:ADR-0145 §6 の prefix-state receipt が入れば、2 本目以降の prefill は「新しい計算なし」として払われない(二重払いなし)。親 job を共有する job group が必要かは P2 で判断する(必要なら ADR)。

##### 2.5 JSON Schema の拡張(P2)

**現状.** `misaka-palw-constraint::schema` の subset(type・properties・required・additionalProperties・items など)だけを受け、範囲外は名指しで拒否する。制約は decode の許可 token 集合として commit され、seat が再実行し、court が審理できる(ADR-0096 Decision 3)。

**拡張.** `enum`・`const`・判別できる `anyOf`/`oneOf`・`minItems`/`maxItems`・`minLength`/`maxLength`・数値範囲・非再帰の `$ref`・決定的な正規表現 subset の `pattern`(DFA に落とせるもの)を段階的に足す。

* 検証の形:制約は「状態 → 許可 token 集合」の決定的なオートマトン。committed token が許可集合に入っていることは token ごとに検証でき、「許可集合内の argmax」も、beating lane が許可集合内かを 1 bit 確かめれば 2 tile の反証で済む(I-2)。
* **近似はしない**方針を維持する。compile できない schema は従来どおり名指しで拒否する。
* DoS 対策:compile 後の状態数・遷移数・compile 時間に上限を置き、上限超えは拒否する。

##### 2.6 会話をまたぐ KV キャッシュの再利用(P1・最優先)

**現状.** `misaka-palw-gateway` の常駐 worker は artifact の mmap を 1 回で済ませる(ADR-0077 D1)が、「1 エンジン・1 KV キャッシュ・mutex 1 つ」で、リクエストごとに KV を使い捨てる。会話の各ターンが履歴全体を先頭から再計算するので、8k・2M class では会話が伸びるほど計算が増える。

**プロトコル側の土台はすでにある.** ADR-0145 §6 は、receipt に「入力 commitment・prefix-state commitment・新しい token 範囲・出力 commitment・class・実行モード(uncached / prefix-reused / KV-reused)」を持たせ、検証者は新しい計算だけを再構成し、キャッシュ分は払わないと定めている。**欠けているのはサービング側の実装。**

**設計(段階 1:ノードのみ、consensus 変更なし).**

* **prefix キャッシュ**:key = `(class_id, tokenizer_id, H(prefix token ids))`、value = その prefix までの KV 状態(K/V i16 codec)。LRU で、メモリ予算は `--kv-cache-budget` で指定する。任意でディスクへ退避する。
* **API**:OpenAI 互換のまま、会話履歴の先頭一致で自動的に当てる。`misaka.session_id` は任意のヒントとして受ける(無くても動く)。
* **決定性**:整数カーネルなので「キャッシュから再開」と「先頭から計算」は bit 一致しなければならない。**prefix-resume == fresh の golden テストを必須にする。** 一致しない場合はキャッシュを使わない(安全側に倒す)。
* **claim**:段階 1 では、claim は従来どおり全体を commit する(報酬も従来どおり)。高速化はノードの応答時間だけ。
* **メモリの目安**:8k は 1 セッションあたり数百 MiB 級で多数を保持できる。2M は K/V だけで約 11.6 GiB(kv-codec メモ)なので、事実上 1 セッション。

**段階 2(P2、consensus 変更).** ADR-0145 §6 の receipt 形状(prefix-state commitment)を FP の claim に実装し、panel は prefix を自前のキャッシュで持つか、prefix-state commitment に対して新しい範囲だけを再計算する。報酬は新しい計算の分だけになる。receipt 形状の変更なので fence が要る。

##### 2.7 並行処理(P1)

**設計.** 2 段で進める。

1. **複数 worker プロセス**:同じ artifact を読み取り専用で mmap 共有し(重みは OS のページキャッシュで共有)、KV は各プロセスが持つ。gateway は class ごとに worker プールを持ち、キュー・上限・公平性(接続元ごとの上限)で割り振る。実装が小さく、決定性に影響しない。
2. **プロセス内 scheduler(continuous batching)**:1 プロセスで複数シーケンスをまとめて計算する。**batch 化しても各シーケンスの計算が混ざらない(batch-invariant)こと**が条件で、整数カーネルは行単位の独立性を守れば bit 一致する。golden テスト(単独実行 == batch 内実行)を必須にする。

claim の形(1 推論 = 1 claim)は変わらない。各リクエストが自分の job を持つ。

##### 2.8 /v1/embeddings(P1、claim 化は P3)

* **定義**:class の最終層 hidden state を、固定の pooling(mean または last-token)でまとめた整数ベクトル(Q24)。正規化はクライアント側、または固定の整数 L2 で行う。
* **P1**:ノードのローカルサービスとして提供する(claim なし・報酬なし)。同じエンジン・同じ artifact・§2.6 のキャッシュを使う。
* **P3**:新しい job kind(出力 = ベクトルの commitment)として claim にする場合は、報酬と検証(ベクトル全体の再実行比較)の経済設計が要るので別 ADR。

##### 2.9 artifact の自己完結化(P1)

**現状.** tokenizer commitment が artifact 内で全ゼロ(既知の問題)、tokenizer.json は別リポジトリ、`generation_config.json` は参照用で実行時に読まれない、chat template は `wire` クレート側。

**設計(artifact v2 の sidecar セクション).**

* sidecar に `tokenizer.json`(バイト列と commitment)、chat template、generation config の既定値を入れ、manifest に各 digest を記録する。
* **tokenizer commitment を実値にする。** job はすでに `tokenizer_id: Hash64` を持つので、class の registry listing(「出品」)に tokenizer commitment を載せ、job の `tokenizer_id` との一致を要求する(この照合は fence で入れる P2)。
* Studio と gateway は sidecar から読む。chat template は sidecar のものを優先し、無ければ `wire` の既定を使う。generation config の既定値は gateway の既定値として使う(consensus には入れない)。
* 既存 artifact の digest・class id は変えない(sidecar は別 digest)。

##### 2.10 LoRA / adapter(P3、新 class)

* **派生 class**:`base_class_id` と `adapter_digest` から派生 class id を作り、registry に出品として登録する。
* **実行**:整数の LoRA(`W·x + B·(A·x)`、量子化した低ランク行列)を決定的に計算する。panel の seat は adapter ファイルも保持する(readiness / possession proof の対象を広げる)。
* **価格と反証**:base と同じ形で、adapter の計算量を work vector に足す。
* 別 ADR。重みの差し替えではなく「別の class」として扱うことで、既存 class の identity を壊さない。

##### 2.11 マルチモーダル(P3、新 class)

* **encoder class**:vision / audio encoder を独立した整数グラフとして登録し、出力の埋め込み列を LM class の入力に接続する「複合 job」にする。
* **最大の難所は前処理の決定性**(JPEG デコーダなどの実装差)。入力は「デコード済みの整数 tensor(例:RGB u8、固定解像度)」に限定し、その hash を job に commit する。デコードとリサイズはクライアント側の責任にする。
* 反証は同じ tile 開示の枠組みで、encoder の tile も開示対象にする。計算量が大きいので価格設計が要る。
* 別 ADR。

---

#### 3. 優先順位とフェーズ

| フェーズ | 内容 | consensus | 目安 |
|---|---|---|---|
| **P0** | decode 規則の一括導入:D10(decode leaf の分子)+ D11(Gumbel-max サンプラーを全エンジンへ)+ §2.1 repeat_penalty + §2.3 logit_bias + §2.2 stop_token_ids。job は V4。fence は `palw_fp_decode_rules` を「decode rules v2」として 1 本で arming | あり(fence 1 本、flag day) | D10・D11・repeat_penalty は `rcore/fp-sampler` で実装開始。logit_bias・stop は本 RFC の承認後に同じ lane へ |
| **P1** | §2.6 KV prefix キャッシュ(段階 1)、§2.7 並行処理(複数 worker)、§2.4 n の fan-out、§2.8 embeddings(ローカル)、§2.9 artifact sidecar(読み込み側) | なし | ノード更新だけで出せる。KV キャッシュを最優先 |
| **P2** | §2.6 段階 2(prefix-state receipt、キャッシュ分を払わない)、§2.5 JSON Schema 拡張、§2.9 の tokenizer 照合、§2.4 の job group(必要なら) | あり | ADR-0145 §6 の実装 |
| **P3** | §2.10 adapter、§2.11 マルチモーダル、§2.8 embedding claim | あり(新 class) | それぞれ別 ADR |

**P0 を 1 本の fence にまとめる理由.** どれも「lane key を公開の量で補正する」同じ形の規則で、反証の形(2 tile + 公開の履歴)を共有する。別々に arming すると、「サンプラーはあるが反復する」「分子はあるが greedy のまま」といった中途半端な状態を設定で作れてしまう(params.rs が D10 と D11 を 1 本にしている理由と同じ)。

---

#### 4. consensus・identity への影響

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

#### 5. リスクと緩和

| リスク | 緩和 |
|---|---|
| 生成側と検証側の計算が食い違い、正直な producer が反証される | produce と replay が consensus-core の同じ関数を呼ぶ。seed・温度・penalty を掃く bit 一致テスト。fence を跨ぐドリルで実チェーンを確認 |
| 整数丸めの不一致(負の値・オーバーフロー) | 丸め方向を −∞ に固定、i64 の中間値、範囲外は受理時に拒否、境界値テスト |
| DoS(logit_bias の長さ、schema の状態数、stop 列、キャッシュのメモリ、並行接続) | すべてに上限を置き、上限超えは名指しで拒否(ADR-0096 の原則) |
| キャッシュ再開と先頭計算の不一致 | golden テスト必須。不一致ならキャッシュを使わない |
| 容量再設計(ADR-0160)との相互作用 | claim 数・報酬の分子が変わるので、D10 の arming は容量の flag day と同時にしない。報酬の総額は ADR-0160 の排出設計に従う |
| 2M の検証コスト | 2M の同時 claim 上限(C7 = 1)は ADR-0153 まで据え置き。KV キャッシュも 2M は 1 セッション想定 |

---

#### 6. テスト・ドリル計画

1. **単体**:§2.1〜§2.3 の key 関数、丸め、境界値、identity 値での V3 一致。
2. **produce ⇔ replay の bit 一致**:seed × 温度 × penalty × bias × stop の組み合わせを、floor・8k class で掃く。
3. **反証**:規則を無視した token を commit した claim が、2 tile(+ 履歴)で反証され、正しい claim は反証されない。
4. **サービング**:prefix-resume == fresh、batch 内 == 単独、キャッシュの LRU とメモリ上限、並行接続の上限。
5. **ドリル**:salt 付きドリルで decode rules v2 の fence を低い高さで跨ぎ、seed・温度・repeat_penalty 付きの FP claim が bind → licence → Final まで進むこと、全ノードの状態が一致すること。
6. **公開チェーン**:flag day 後、fence 越えの FP claim の Final と、反証 0 件を監視する。

---

#### 7. 未決事項(判断が必要)

1. **repeat_penalty の範囲**:`p_q` の上限(提案:4.0)、`repeat_last_n` の上限(提案:256)、プロンプトを window に含めるかの既定(提案:含めない)。
2. **stop の扱い**:token id 列だけを consensus に入れ、文字列は gateway の表示レイヤで扱う案でよいか。
3. **logit_bias と stop を P0 に入れるか**:同じ fence にまとめる案(推奨)か、D10・D11・repeat_penalty だけで先に出すか。
4. **KV キャッシュの段階 2**(キャッシュ分を払わない報酬)の時期:容量再設計の ×10 flag day より前か後か。
5. **embeddings の claim 化**(P3)をするか、ローカルサービスのままにするか。
6. **マルチモーダルの入力形**:デコード済み整数 tensor に限定する案でよいか。

##### 7.1 採択記録(2026-10-03、lane U / `rfc1/serve`)

1〜3 は P0(§A、Job V4 / fence `palw_fp_decode_rules`)で実装済み。4〜6 を次のとおり採択し、実装した(いずれも休眠 fence、どの preset でも arm しない)。

> **2026-10-12 訂正:** その後、testnet-12 は int-11 flag day(`PALW_T12_INT11_FENCES_V1`)で `palw_fp_decode_rules`・`palw_fp_decode_constraint`(FP Job V6)・`palw_fp_prefix_state`・`palw_fp_prefix_inherit`・`palw_fp_job_v5` を DAA 5,300 で arm した。上の「どの preset でも arm しない」は採択時点の記述であり、稼働中の testnet-12 には当てはまらない([台帳](../design/palw/integration/mainnet-spec-register-2026-10-12.md) X4・T11)。

4. **KV キャッシュ段階 2 は今、専用の休眠 fence で**:`palw_fp_prefix_state`(FP Job version 11、`PalwFpPrefixStateV1` を carry)。段階 1(node のみ)は answer-only 経路で出荷済み。報酬側(credit)のみで、fold は全 prefill leaf を ctx 束縛 hash で畳むままなので、**キャッシュした prefix は commit 実行を速くしない**(段階 2b「継承 leaf」は残課題)。decode rules が有効な状況では D10 が既に prefill を 0 として credit するため、version 11 が価格を動かすのは「宣言された状態を seat が検証する」点であり、価格差ではない(e2e で等値を確認)。
5. **embeddings の claim 化は RFC-0003 の Embedding profile で**:ローカル `/v1/embeddings` は claim 無しのまま、claim 形は `PalwGenJobV1`(`Embedding` body)。gateway に job 構築器と `misaka.claim` の名前付き拒否を置いた(free-prompt レーンの worker しか持たない gateway は拒否する)。
6. **マルチモーダルはデコード済み整数 tensor のみ**:`palw_image_tensor` 部品(`u8` HWC RGB の hex)だけを受け、URL・エンコード済み画像は名前付きで拒否。画像は FP Job V5 の slot 参照(`input_root`)になる。encoder を持たない class の背後では実行時に名前付きで拒否する。

追加した休眠 fence:`palw_fp_prefix_state`(§2.6 段階 2)、`palw_fp_tokenizer_match`(§2.9)、`palw_fp_constraint_v2`(§2.5、前提の `palw_fp_decode_constraint` が未実装のため arm 不能)、`palw_adapter_class_v1`(§2.10、ADR-0163、object tag 94)。

##### 7.2 追記(2026-10-03、lane U 第 2 弾)

- **ADR-0096 D6-8(`palw_fp_decode_constraint`)を実装した**:FP job version 6(V3 job + constraint bytes)、producer と seat の replay が同じ mask(class の token table 経由)を通る、`validate_palw_v2` の arm 拒否を解除、drill 旗 `--palw-drill-fp-constraint-at`、gateway の `response_format` は fence 有効網で committed(version 6)になる。これで `palw_fp_constraint_v2` を drill で arm できる。**未了**:court の第 3 arm(`check_tiled_decode_token_refutation_v3` は純関数として既存)を wire 上の proof kind にすること、`render_answer_v2`(D6)、kaspad seat への token table の配布(table が無い seat は `Unverifiable` で棄権)。
- **(2026-10-03 夜 訂正: 段階 2b は実装済み・下の §7.3 を参照。以下は実装前の記述)** KV キャッシュ段階 2b(継承 leaf)は未実装:commit の leaf は job context 全体に束縛されるため、cache 済み prefix の leaf の継承には「prefix context」を全 leaf 束縛関数と court の binding に通す必要がある(consensus の再設計)。行(per-position rows)を cache する案は model サイズで 1 position あたり GB 級で不成立。段階 2(受領証)は credit 側のみで、decode rules 有効下では D10 が既に prefill を 0 とするため価格差は出ない。
- 画像・embedding claim は、tiny な in-repo vision class(toy VLM / toy vision encoder)で gateway → FP Job V5 / tensor job → worker → seat の in-process e2e を通した(`misaka/palw/gateway/src/tensor.rs` tests)。実 class(vision tower)は tir/generic の統合待ち。

##### 7.3 追記(2026-10-03 夜、lane U 第 3 弾)— 段階 2b と運用

- **段階 2b(FP job version 12、dormant fence `palw_fp_prefix_inherit`)**:prefix 位置の step leaf を job 非依存の prefix context(version-3 job context、`job_nullifier` = k、`assignment_id` = prefix state root、`step_tile_leaf_hash_ctx_v1`)で hash する。同じ prefix を共有する job の継承 leaf は byte 単位で一致し、再計算した root と等しく、継承範囲の嘘は先に同じ leaf を commit した claim で反証できる。V2・V11 の context と leaf は 1 bit も変わらない。fence は `palw_fp_prefix_state` を前提に持つ。
- **admission(必須の安全規則、acceptance で名指し拒否)**:version 12 の claim は、leaf が継承に安全と証明された class にだけ載る。条件は「held でない」「IR・生成 class でない」「fused-attention の court window を持たない」「shape profile が公開済みで整数 lane(`Int32`)」「KV aux series を持たない(`kv_chunk_calls == 0`)」。それ以外(fused attention、KV aux、未証明の kind)は walk が `an inherited-prefix claim (FP job version 12) over a class whose leaves are not inheritance-safe …` で skip する(`palw_fp_prefix_inherit_class_safe_v1`、`PalwChainStateV2::class_prefix_inherit_safe_v1`)。checkpoint の leaf は継承せず job ごとの context で hash する(整数 floor 上で seat の interval replay が Valid になることを e2e で確認)。
- **token table の自動読み込み**:seat は class artifact の隣の `<artifact>.palwtokens`(`palw-a16-fp-worker --emit-token-table` の出力)を起動時に登録する。table は自分の bytes から導いた root で登録され、制約付き claim は job が commit した `table_root` で table を選ぶので、root の合わない sidecar はその claim には使われない(seat は `Unverifiable` で棄権)。`--palw-token-table` は上書き・追加として残る。
- **drill 用 producer**:`misaka-palw-rfc1-drill-claim`(floor class 上で constraint / constraint2 / prefix / inherit の claim を実走して outbox に書く)+ `scripts/drills/misaka-palw-rfc1-drill-claims.sh`(rail で署名・submit)。adapter 一覧の producer は composite class が要るため floor 版はない。
- **残(最適化・凍結後)**:executor の計算削減、seat 側の prefix-leaf cache。

#### Mission alignment amendment — 2026-10-07

§Aと§2のdecode、artifact、KV reuse、adapter、multimodal拡張は、それぞれ公開の入力・state境界・version・outputと裁定証拠を拘束する。ローカルChatの非claim機能や将来profileのsource fidelityは、公開訴追可能性の代わりにならない。新しい報酬profileの受入にRFC14の外部public-bond試験を追加する。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)、[RFC15](../rfc/0015-panel-free-permissionless-verification.md)を参照する。

#### 改定 — 2026-10-12（W1・W2・X4・N4：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 詳細は[RFC/ADRレビューの整理（2026-10-12）](../audit/palw/design-reviews/rfc-adr-review-resolution-2026-10-12.md)。

- **W1:** Gatewayの整数seed変換を `le64(n+1)` に統一し、オーバーフローとなる入力を拒否する。gateway側の表示・入力変換規則であり、直ちにコンセンサス分岐が起こると断定しない。
- **W2:** FP V4自体のtext規則は変更しない。TextStream pipelineには追加規則を適用する。
- **X4:** FP Job V6は「実装済み・dormant。ただしcourtの一部に未完了事項あり」と記録する。
- **N4:** 通常の非報酬AI推論はプライバシーを維持して利用できる。公開訴追に必要な証拠を公開できない経路は、新規の報酬・weight対象から外す。

---

<a id="source-v1-0003-L00001"></a>

### 旧RFC-0003 / 原文1–1070行

出典: [0003-palw-generative-model-classes.md](../rfc/0003-palw-generative-model-classes.md)。配置理由: 主題別統合。

### RFC-0003: PALW Generative Model Classes — one job, determinism and output layer, and the class profiles on top of it

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **適用時期の現行方針（2026-10-17 文書化）:** 本 RFC の画像・動画経路は **mainnet 後へ延期・当面無効**。初期 mainnet の必須実装・提供・有効化に含めず、mainnet 開始による自動有効化もしない。共有のテキスト・固定登録・検証・会計は維持する。過去の Status・実装・試験・全面有効化計画はこの範囲の有効化承認ではない。[正式な範囲・無効化要求・再開条件](../governance/post-mainnet-scope.md)。今回 node コードは変更しておらず、runtime の無効化は未確認。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


| Field | Value |
| --- | --- |
| Status | Draft, 2026-09-28 — open questions 1–12 decided by the user on 2026-09-28 (see *Decision*); implementation of Part I and program version 2 started on `rfc3/impl` |
| Author(s) | MISAKA core (drafted with Claude) |
| Created | 2026-09-28 |
| Affects | spec/palw 03 (registry: pipeline classes), 04b (PALW-TIR: program version 2 — input tensors, output kinds, `post` effects), 05 (canonical work: attention over a token axis), 07/08/09 (claims, verification, court: derived inputs, stage edges, output digest), 11 (job lanes: a generative job family), 16 (fences) · all networks (dormant until armed) · `consensus/core` (R, the job family, pipeline admission, court), `misaka-palw-tir`, `misaka-palw-tir-lower`, `misaka-palw-sdk`, gateway |
| Branch | `rfc/0003-image-generation` (text only) |
| Related | RFC-0001 (§A FP Job V4, frozen; ADR-0082 D11 sampler), RFC-0002 (PALW-TIR; spec [04b](../spec/palw/04b-tensor-ir.md) on `tir/core`; Phase F integration design on `tir/phase-f`), ADR-0040 (integers only), ADR-0044 (F5/F6/F15: no randomness from executor-chosen fields), ADR-0053 (no tolerance), ADR-0057 (backends below the semantic boundary), ADR-0069 (weight needs e2e adjudicability), ADR-0072 (one inference, one ticket), ADR-0074 D1 (canonical prompt mode), ADR-0096 (refuse by name), ADR-0133 (resume), ADR-0135 D7 (a new op is a protocol upgrade), ADR-0144 (P1–P7), ADR-0145 §6 (a cache is an execution fact) |

#### Current extension and verification boundary — 2026-10-06

[ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) governs future
profiles: bounded declarative pipelines over active kernels; missing semantic/checker/court or task
relations need coordinated versioned kernel extensions, not BVM/GVM, arbitrary tests or uploaded
universal circuits. [RFC07 Part V](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md)
and [RFC11 §15](../rfc/0011-permissionless-model-and-long-context-onboarding.md) select small probabilistic
checks over encoded constraints, with exact bounded court only on disputes. Every stage, derived
input, decode rule, cross-stage edge and output belongs to the checked statement. Compose soundness
over the whole pipeline; a checked decoder alone is not verification of a multimodal model.
Generation randomness `R` is distinct from post-commit verifier challenges. The proposed challenge path uses only
[RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol), with a
class-bound policy and fully separated generation/challenge/Panel/ticket domains. Generation R is an input to the
deterministic output statement and is fixed with its task/seed/input commitments before checking; it is not a beacon
replacement or a means to reroll checks after seeing verification randomness. Whole media/pipeline state and relation
coverage remain mandatory; onboarding conformance follows RFC11 §17 and RFC13 without admitting unknown semantics.
Legacy replay below
remains a reference/old-profile rule, not a requirement for routine whole-stage/segment replay in
the new profile. This amendment changes no active implementation or fence.

#### 概要(日本語)

- **目的。** 画像生成モデル(Qwen-Image の 2.x 系、FLUX.2 [klein] 4B など)を、テキスト LLM と同じ形 ——
  **Job → 決定論的実行・constraint commitment → 小さい確率的検査 → Final（異常時のみexact court）** —— で permissionless に
  登録できるようにする。同じ形を埋め込み・音声・動画にも使う。モデルごとの差は小さな「class profile」に
  閉じ込め、共通層を先に作る。RFC は増やさず、この 1 本を **Part I(共通層)** と **Part II(class profile)** に分ける。
- **Part I-1 決定論的乱数 `R(seed, domain, step, position, lane)`。** 置き場所は **(a) job/consensus 層**。
  D11 と同じ keyed BLAKE2b-512 で、乱数 tensor は TIR program に **派生入力** として入り、court が job から
  再計算する(executor は commit しない)。**IR の primitive は増やさない**:R の入力はすべて job の事実と
  静的な座標で、計算された値には依存しないから。(b) 新 primitive は bit 演算・新しい `prim_set_id`・fence・
  全 backend での hash 実装が要るのに得るものがない。(c) library 合成は XOR なしの算術 PRNG を各 class が
  持つことになり、統計的な品質も「protocol が固定する」性質も失う(しかも seed を入れる入力 tensor は結局要る)。
- **R の鍵は job id ではなく job の seed(32 byte)。** D11 と同じで、同じ seed で同じ出力を再現でき、
  executor が選ぶ `job_nonce` に乱数の意味を持たせない(ADR-0044 F6 と同じ規律)。R は lottery には一切使わない。
- **RFC-0001 の D11 サンプラーは R の domain 0 として 1 byte も変えない**(key 文字列、`seed ‖ position ‖ lane`、
  digest の上位 13 bit、Gumbel 表)。新しい domain(画像の初期ノイズ・step ノイズ、音声、動画、class 定義用)は
  同じ文法で、1 回の hash から 16 bit の語を 32 個取る。
- **ビットから値へ。** 一様整数は語そのもの(任意範囲は ⌊w·n/2^b⌋)。Gaussian は **2^16 行の pinned 逆 CDF 表**
  (Q24、|z| ≤ 約 4.33、反対称で平均がちょうど 0)を引く「データ」で、演算ではない。protocol 側の入力種別
  `Normal` にするので区間が正確に決まり、class ごとに表を持たない。Gumbel は D11 の表のまま。
- **Part I-2 Canonical tensor と execution profile。** dtype・TF32・kernel 選択・GPU 数といった
  「execution profile」は MISAKA では不要。consensus の意味は整数 TIR program で、backend はバイト一致、
  fused kernel は identity の外、並列化は順序自由領域の中でしか起きず値を変えない。残る関心事は
  (1) 入力前処理(画像・音声のデコードとリサイズ)、(2) tokenization と固定テンプレート、(3) 出力の表示用
  エンコード、(4) fidelity 用 float 参照の固定、(5) ノードの性能設定 —— どれも profile ではない。
- **全 profile 共通の program 面。** `TirProgramV2` = V1 + 入力 tensor(job の値・上流 stage の出力・R の派生入力)
  + 出力種別(`Logits` / `Rows` / `Final`)+ `Rows`/`Final` の program では `post` が global Fixed state を
  書ける(1 step 1 writer は維持)。class は複数 program の **pipeline**(stage、構造だけの edge、1 本の step tree)。
  **primitive は 25 個のまま**で、足す強い理由は見つからなかった。
- **Part I-3 Canonical Output。** consensus は出力 node の整数 tensor を種類ごとの正準バイト列にしたもの
  (text = token id 列、image = raw u8 HWC、audio = PCM i16 LE、embedding = i32 Q 形式、video = u8 THWC)。
  digest は step tile に揃えた Merkle root で、step tile と食い違えば structural fault(executor の負け)。
  PNG・WAV・MP4・UTF-8 表示は presentation で consensus ではない。
- **Part II-1 画像生成(5 章)。** ImageJob(prompt/negative の token id、guidance は 1/16 格子、seed、
  image_index、sampler、step 数、解像度、出力 = RGB8。正準化は gateway の仕事で、受理時は検査と名指しの拒否だけ)。
  ノイズは R の `Normal` を latent の正準順 `[C, H, W]` で引き、pos = 0 で `Select`。denoise は **1 step = 1 position**、
  latent は `post` が書く Fixed state、scheduler 係数は `(step 数, pos)` で引く表、CFG は batch 軸、text encoder は
  別 program(stage)、VAE decode は解像度ごとの単一 position stage の鎖、画像 token 上の attention は 1 step 内の
  tensor 軸(H ではない)。
- **court。** bisection は既存の ladder で steps × tiles を narrowing する。K/V を head-major に commit すれば、
  attention の cone は約 15,600 token(head 次元 128)まで 16 Mi MAC・約 16.8 MB の上限に収まる(効く制約は MAC ではなく
  開示バイト)。VAE の mid-block attention や GroupNorm の統計のように収まらない exact reduction は、lowerer が
  **commit した部分和による 2 段 reduction** で解く(protocol 変更なし)。それを超える規模(2048² 級・動画)には、
  RFC-0002 の H dissection を **宣言された任意の reduction 軸へ一般化** することを提案する(H と違い、bottom の費用は
  admission が box demand で検査する)。PALW-TIR-33 はそのまま(不正な commit は executor の負け)。
- **fidelity と計算量(正直に)。** fidelity は同じノイズを入れた float 参照との latent 誤差・PSNR/LPIPS・CLIP 型スコア・
  FID/KID で測り、有用性の基準であって正当性の基準ではない。計算量は (P, N, S, CFG 係数) でパラメータ化した。
  **例 A**(P = 4×10^9、N = 4,096、S = 28、CFG 2)で約 1.3×10^15 MAC/枚、S = 4・CFG なしで約 1×10^14。
  汎用 CPU backend で数時間〜十数分、調整済み CPU で分単位、GPU 整数で秒単位。commit 量は例 A で約 1 TB/枚で、
  GPU では MAC より leaf の hash が重い。testnet の最初は ≤ 1B・少 step、4B 級は RFC-0002 Phase G(GPU 整数 backend)の後。
- **対象モデルについて確かなのは「text-to-image・拡散/flow transformer・VAE の latent 空間」だけ。** 構成は未検証なので、
  見積りは例として示し、image profile を凍結する前にモデルカードで確かめる checklist を付けた。
- **他の profile(各 1 段落)。** テキスト = RFC-0001 FP Job V4 をそのまま参照(再定義しない。D11 が domain 0)。
  埋め込み = `Rows`/`Final` + pooling stage、seed は 0 固定。マルチモーダル入力 = 正準 u8 画像を job が commit し、
  正規化・patchify(と固定比のリサイズ)を整数 stage で行う(デコードは consensus 外)。音声 = PCM i16 を正準形に、
  mel 前処理を整数 stage に、生成は codec token の LM か latent flow。動画 = 画像の一般化(latent に時間軸)で、
  token 数が大きいので一般化 dissection が必須、現実性は commit 量が決める。
- **決定(2026-09-28、ユーザー)。** 未決事項 1〜12 はすべて推奨どおり:鍵は seed、protocol 共通の 2^16 Gaussian 表、1 class 1 解像度、VAE は最初から on chain(最初は ≤ 512²)、profile ごとの上限、testnet は commit 量を受け入れる、画像の attention は `attention_prefill`、pipeline 登録は複数 carrier、最初の画像 class は小型(モデルカード確認後に選ぶ)、一般化 dissection は動画と一緒、音声の multi-codebook は後、画像 job の既定は `PanelDa`。
- **オフチェーンの対応物。** MISAKA Studio の diffusers sidecar(別 session で構築中)は consensus 外のローカル生成。
- **実装順序。** R と canonical output → TIR V2 → pipeline class と court 拡張(fence `palw_gen_v1`、休眠)→
  画像 profile(小型 class、埋め込みを smoke test に併用)→ ドリル → testnet → Phase G 後に 4B 級 → マルチモーダル入力
  → 音声 → 一般化 dissection → 動画。テキストは RFC-0001 の列車のまま。

#### Summary

Generative classes — images first, then embeddings, audio and video — become registrable the way
text classes are, with one loop for all of them: **a job fixes every input; execution is a
deterministic integer PALW-TIR pipeline; the output is a committed integer tensor with a canonical
byte form; a dispute ends with the court re-evaluating one cone.** Per-model difference is kept to a
class *profile* that reuses one common layer.

**Part I, the common layer, is built first.** (1) **Deterministic randomness:** `R(seed, domain,
step, position, lane)`, a counter-based keyed BLAKE2b-512 fixed by the protocol and independent of
execution order, parallelism, batching or device count. It lives at the job layer; random tensors
enter a program as *derived inputs* that the court recomputes; no primitive is added. RFC-0001's
D11 sampler is R's domain 0, byte for byte. (2) **Canonical tensors and the execution profile:**
the integer program already fixes everything an execution profile would, so no profile object is
needed. What remains is input preprocessing, tokenisation, presentation encoders and the float
reference for fidelity. The program surface every profile needs is small: input tensors, output
kinds, and state writes in `post` (`TirProgramV2`), plus pipeline classes (stages, structural edges,
one step tree). (3) **Canonical outputs:** one canonical byte form per output kind, a tile-Merkle
output digest, and presentation kept outside consensus.

**Part II, the class profiles:** Image generation in five chapters (ImageJob, deterministic noise,
the denoise scan, ImageOutput, verification and court), followed by fidelity, a parameterised
compute estimate and a checklist to verify before the profile is frozen. Then Text generation
(RFC-0001 unchanged), Embedding/encoder, Multimodal input, Audio and Video, each as small as it can
be. **The 25 primitives of PALW-TIR v1 stay; `prim_set_id` does not move.**

#### Motivation

1. **The target.** The user wants image models — the Qwen-Image line (2.x) and FLUX.2 [klein] 4B
   are the named examples — to be registrable as data. What is certain about them for this RFC is
   only that they are text-to-image diffusion/flow transformers working in a VAE latent space. Their
   exact geometry is unverified here, so every number below is parameterised, and the
   *Verification checklist* lists what must be confirmed first.
2. **Everything today is text-shaped.** A PALW-TIR v1 program maps `{token, pos}` and its states to
   logits (04b §3). Selection happens outside the program, in the FP decode rule (RFC-0001 §A, D11).
   The only committed output is a token sequence. There is no definition of a non-text job, of
   randomness other than the text sampler's, or of an output that is a tensor.
3. **Without a common layer, each modality repeats the per-family release problem.** RFC-0002
   removed "a release per architecture" by making the class a program. If images, audio and video
   each brought their own job wire, their own noise, their own output format and their own court arm,
   the same problem would come back as "a release per modality". The loop *job → deterministic
   execution → committed output → dispute → court replay* must have one shape. Profiles may differ
   only in their job body, their programs and their output kind.

#### Goals and non-goals

**Goals.**

- G1. One job envelope, one randomness function and one output rule, shared by every profile.
- G2. No new primitive. The court stays a PALW-TIR v1 interpreter that knows no model.
- G3. Randomness is a protocol function of job facts, reproducible bit for bit on any hardware, in
  any order, at any parallelism.
- G4. Consensus bytes and presentation are separated for every output kind.
- G5. Every profile is adjudicable by cones within the court ceilings, and where it is not, the RFC
  says so and names the fix.
- G6. Profiles are minimal: a job body, programs and an output kind. They add no semantics beyond Part I.
- G7. RFC-0001 §A (FP Job V4) is untouched.
- G8. Registration stays permissionless (P7) for multi-program classes.
- G9. An honest estimate of compute and commitment volume, and of what runs on testnet first.

**Non-goals.** Bit-identity with a float reference or with a framework's image for "the same seed"
(fidelity is measured, never required). Decoding user files (JPEG, PNG, MP3, MP4) inside consensus.
Content policy (no validity rule reads what a prompt or an output means, PALW-PR-1). Local,
non-consensus generation: MISAKA Studio's diffusers sidecar, being built separately, is the
off-chain counterpart, the image analogue of RFC-0001 §2.0's Chat path. In image v1: editing,
img2img, inpainting, reference images and adapters are refused by name (§II.1.1). A separate RFC per
modality. zk proofs. Production-scale video in v1 (only its path is drawn here).

---

### Part I — The common layer

#### I.0 One job shape

Every non-text profile uses one job family. Text keeps FP Job V4 (§II.2).

```
PalwGenJobV1 {
  version:  u16 = 1,
  envelope: PalwJobEnvelopeV1,   // the FP V3 envelope fields with their spec-11 meanings: network_domain,
                                 // class_id, executor_bond, executor_pubkey, operator_id, anchor_block,
                                 // anchor_daa, job_nonce, privacy_mode, prompt_mode
  seed:     [u8; 32],            // R's key (§I.1); all zeros for a class that draws no randomness
  body:     PalwGenBodyV1,       // Image | Embedding | Audio | Video — Part II
}
gen_job_id_v1 = H64(key "misaka-palw/gen-v1/job-id/v1", borsh(job))
```

- **One encoding per behaviour.** Acceptance verifies the canonical form and refuses anything else
  by name, as RFC-0001 §A.2 and ADR-0096 do. It never rewrites a job. Canonicalisation (text to ids,
  a typed guidance value to its grid, an integer seed to 32 bytes) is the gateway's work.
- **The body's kind MUST equal the class's profile** (`BodyKindMismatch`). The seed MUST be all zeros
  exactly when the class declares no random input: a seed on a deterministic class is `SeedNotUsed`, so
  a deterministic class never has two job ids for one computation, and an all-zero seed on a class that
  draws randomness is `SeedRequired` (the all-zero seed is the deterministic classes' encoding of "no
  seed"). The shipped gateway does not map integer seeds: it takes the 32-byte seed as 64 hex characters and refuses an
  integer by name (decided 2026-10-12, W1 — the `le64(n + 1) ‖ 0^24` suggestion is withdrawn); seed handling is the
  gateway's, not consensus's. *(Built 2026-10-01,
  04b §15.13; the biconditional is this bullet's own wording, and the integer mapping in §II.1.1 follows
  it.)*
- **The class binds the tokenizer.** A pipeline class's id commits to its tokenizer (Phase F D4), so
  the job carries none. This closes the gap that FP V3's `tokenizer_id` documents for text.
- **Canonical prompt mode** (ADR-0074 D1) carries over. The network's own job derives its prompt from
  its canonical anchor (computed over the envelope as `fp_canonical_anchor_v1` does), and its seed as
  `H64(key "misaka-palw/gen-v1/canonical-seed/v1", anchor)`, a pure function of the job.

#### I.1 Deterministic randomness

##### I.1.1 Where R lives — decision: (a), the job layer

**R is a consensus function at the job layer**, a keyed BLAKE2b-512 of job facts and static
coordinates (the discipline D11 already ships). **Random tensors enter a TIR program as derived
inputs.** Their elements are functions of the job alone. The executor never commits them, and the
court recomputes every element a cone reads. **No primitive is added**, and PALW-TIR v1's
`prim_set_id` is unchanged.

The argument:

1. **Every input of R is a job fact or a static coordinate; none is a computed value.** The seed is
   a job field. The domain comes from the class's declaration. The step, position and lane come from
   the scan position and the tensor's static shape. A value that depends only on the job is a job
   constant, like the prompt's token ids: it belongs where job constants are computed, and the IR only
   needs a way to receive it. Randomness whose *use* depends on data (comparing a uniform draw with a
   computed CDF, or rejection) is still a draw of job facts followed by `Compare`/`Select`, so it does
   not need R inside the IR either.
2. **(b), a new primitive, buys nothing and costs a protocol upgrade.** A `Hash`/`Philox` primitive
   needs either bitwise operations (XOR, rotate), which PALW-TIR has none of, or a monolithic hash
   primitive. Either way it brings:
   - a new prim-set descriptor, so a new `prim_set_id` and a fence;
   - a range transfer function that says "any value";
   - new cost formulas and golden vectors, and a second implementation of the hash;
   - bit-exact BLAKE2b or Philox in every backend, GPU included, inside the evaluator.

   It would also sit badly with RFC-0002's granularity rule, whose primitives are exact integer
   operations and named lossy sites of the arithmetic a model performs. A primitive would be needed
   only if randomness had to depend on *computed* values, and no inference family does that.
3. **(c), a library composite, is possible but strictly worse.** Without XOR, a counter-based
   generator must be purely arithmetic. For example, a multiply–add mixer whose 32-bit rotation of a
   64-bit word is a swap of halves, `(x mod 2^32)·2^32 + ⌊x / 2^32⌋`, is expressible with `Mul`,
   `Div(Floor)` and `Sub`, and its 64-bit squares need 32-bit limbs to stay inside `i128`. The costs:
   - 40–60 nodes per draw;
   - statistical quality that is unvetted in this form (Philox, Threefry, ChaCha and BLAKE all mix
     with XOR);
   - every class carrying its own copy, so "the protocol fixes R" becomes a convention: a registrant
     can ship a weak generator, and "seed 42" means different noise in two classes;
   - the seed still has to enter the program as an input tensor, so (c) needs (a)'s input facility
     *and* the mixer.
4. **(a) is what D11 already is.** The text sampler's Gumbel index is a keyed BLAKE2b that the court
   recomputes from the claim's job (`gumbel_index_v1`). This RFC states that once, for every domain.

##### I.1.2 The function

```
R(seed, domain d, step, position, lane)  →  an unsigned integer of b_d bits

  W_d   = words per digest: ⌊512 / b_d⌋ for a blocked domain, 1 for domain 0
  block = ⌊lane / W_d⌋,   word = lane mod W_d
  D     = BLAKE2b-512( key = K_d, message = enc_d(seed, step, position, block) )      (64 bytes)
  R     = the bits [word · b_d, (word + 1) · b_d) of D, read as a big-endian bit string
```

"Big-endian bit string" means bit 0 is the most significant bit of `D[0]`. For `b = 16`, word `k` is
`256·D[2k] + D[2k+1]`. For `b = 13` and `W = 1` it is `(256·D[0] + D[1]) >> 3`, which is exactly D11's
`u16::from_be_bytes([d[0], d[1]]) >> 3`. R is a pure function of its five arguments. Blocking packs
`W_d` consecutive lanes into one hash call, and a lane's value does not depend on which other lanes
anybody computes.

##### I.1.3 The canonical encoding of every input

| Input | Type | Encoding | Where the value comes from |
| --- | --- | --- | --- |
| `seed` | `[u8; 32]` | the 32 bytes as they are | the job's committed seed (`PalwGenJobV1.seed`; for text, `PalwFreePromptJobV3.sampling_seed`) |
| `domain` | registered id | selects the BLAKE2b **key** `K_d` (its ASCII bytes, no length prefix, no terminator, at most 64 bytes), the message layout `enc_d` and the width `b_d` | the domain id in the class's input declaration, looked up in §I.1.4 |
| `step` | `u32` | 4 bytes LE | for a per-step domain, the consuming stage's scan position `p`; otherwise 0 |
| `position` | `u32` | 4 bytes LE | the job's item index (`image_index`, `clip_index`); for domain 0, the committed logits row |
| `block` | `u64` | 8 bytes LE | `⌊lane / W_d⌋` |
| `lane` | `u64` | — | the element's row-major flat index in the declared input shape |

`enc_d` for every domain except 0 is `seed ‖ le32(step) ‖ le32(position) ‖ le64(block)` (48 bytes).
Domain 0 keeps D11's layout, `seed ‖ le32(position) ‖ le64(lane)` (44 bytes). Keys are distinct, so
the two layouts can never collide.

**Why the seed and not the job id.** The brief's first argument, `job_id`, is realised as the job's
seed:

- **D11 already keys on the seed.** One convention for every domain.
- **Reproducibility.** The same (class, prompt, parameters, seed) gives the same image in any job.
  That is what users mean by "seed". A job id also hashes the fee-bearing envelope, the bond and the
  anchor, so two identical requests would produce different images.
- **No randomness from executor-chosen fields.** The job id contains `job_nonce`, which the executor
  picks for uniqueness and which "carries no lottery meaning" (invariant F6). Keying R on the job id
  would let a free re-roll of the nonce choose among outputs. The seed is inside the job id anyway
  (FP V3 docs), so it cannot change after the fact.
- **Execution facts stay cacheable.** Identical computations have identical randomness (ADR-0145 §6).

Gateways take the seed as 32 bytes (64 hex characters) and refuse a user-typed integer seed by name, so the same seed
string is the same image on every gateway. *(Decided 2026-10-12, W1: the earlier `le64(n) ‖ 0^24` / `le64(n + 1) ‖ 0^24`
mappings are withdrawn.)* That is a presentation convention, not consensus.

**R is never a lottery input.** Eligibility still consumes a beacon that postdates the claim
(ADR-0044 F5/F6/F15). R decides only *what the execution computes*. A chosen seed buys a different
answer and never a better draw (the D11 argument). Grinding a seed costs a whole execution (ADR-0072).

##### I.1.4 Domains

| Id | Name | Key `K_d` | `b` | `W` | `step` | `position` | Lane order | Used by |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | `TEXT_GUMBEL_V1` | `misaka-palw/decode-select-v2/gumbel/v1` | 13 | 1 | — | committed logits row | vocabulary lane | RFC-0001 D11 (unchanged) |
| 1 | `IMAGE_INIT_NOISE_V1` | `misaka-palw/rand/image-init-noise/v1` | 16 | 32 | 0 | `image_index` | latent `[C, H, W]` row-major | §II.1 |
| 2 | `IMAGE_STEP_NOISE_V1` | `misaka-palw/rand/image-step-noise/v1` | 16 | 32 | denoise step `p` | `image_index` | latent `[C, H, W]` | stochastic samplers |
| 3 | `AUDIO_INIT_NOISE_V1` | `misaka-palw/rand/audio-init-noise/v1` | 16 | 32 | 0 | `clip_index` | latent `[C, T]` | §II.5 |
| 4 | `AUDIO_STEP_NOISE_V1` | `misaka-palw/rand/audio-step-noise/v1` | 16 | 32 | step `p` | `clip_index` | latent `[C, T]` | §II.5 |
| 5 | `VIDEO_INIT_NOISE_V1` | `misaka-palw/rand/video-init-noise/v1` | 16 | 32 | 0 | `clip_index` | latent `[C, F, H, W]` | §II.6 |
| 6 | `VIDEO_STEP_NOISE_V1` | `misaka-palw/rand/video-step-noise/v1` | 16 | 32 | step `p` | `clip_index` | latent `[C, F, H, W]` | §II.6 |
| 7 | `CLASS_UNIFORM_V1` | `misaka-palw/rand/class-uniform/v1` | 32 | 16 | 0 or `p` (declared) | item index | declared shape | any class-defined use, through its own table |

- A domain's key is never reused **for another domain**: each `K_d` keys one domain's words. D11's
  table pin is keyed with domain 0's own key over a different message (the 8,192 table entries, not
  an `enc_0` word message), so two messages of different meaning never share a key, and nothing
  collides (finding G11 of the independent second implementation).
- A new domain is a protocol change: a row here and a new `rand_set_id` (§Activation). Domain 7
  exists so that a new *use* of randomness in a class does not need one.
- A pipeline declares each domain **at most once** (PALW-RND-7). A stage that needs two streams sizes
  one input to hold both and splits it with `Slice`, since lanes are independent. This keeps every
  `(domain, step, position, lane)` unique within a job.
- The **lane order of the init-noise domains is canonical**: channel-major over the latent's
  `[C, H, W]` (`[C, T]`, `[C, F, H, W]`). Two classes with the same latent geometry therefore draw the
  same noise for one seed, and a program packs the noise into its own token layout with `Reshape` and
  `Transpose`. Admission enforces the rank. The meaning of the axes is the registrant's statement.

##### I.1.5 From bits to values

| Transform | Value `v` from word `w` | dtype, interval | Where it runs |
| --- | --- | --- | --- |
| `Uniform{b}`, `b` = the domain's word width: 16 (domains 1–6) or 32 (domain 7) | `v = w` | `idx`, `[0, 2^b − 1]` | job layer (input kind) |
| `Normal` (`b = 16`) | `v = PALW_GAUSS_Q24_V1[w]` | `i32` Q24, `[G[0], G[65535]]` | job layer (input kind) |
| `Gumbel` (domain 0 only) | `PALW_GUMBEL_Q24_V1[w]` (D11's table, `b = 13`) | `i32` Q24 | the FP decode rule, outside any program |
| uniform below `n` (recipe) | `⌊w · n / 2^b⌋`, bias `< n / 2^b` | in-program: `Mul`, `Div(Floor)` | program |
| Bernoulli(`p`) (recipe) | `Compare(w < ⌊p · 2^b⌋)` | in-program | program |
| class-defined distribution | `Gather(table, w)` over the class's own table | in-program | program |

**`b` is the domain's word width.** 04b NF-26 requires a `Uniform` input's `bits` to equal its domain's
word width, and the registered widths of the input domains are 16 and 32; so `b ∈ {1, 2, 4, 8}` never
occurs and nothing defines a read of fewer bits than a word. A program that wants fewer takes them
from the word in-program (the "uniform below `n`" recipe above). Domain 0's 13-bit word is the decode
rule's, not an input kind's (finding G6).

**The Gaussian table.** `PALW_GAUSS_Q24_V1[i] = RHAZ(Φ⁻¹((i + ½) / 2^16) · 2^24)` for
`i ∈ [0, 2^16)`, where RHAZ rounds half away from zero. It is evaluated at 60 or more significant
digits by a pinned generator script (`scripts/verify/palw-gauss-table.py`), in the manner of
`scripts/verify/palw-gumbel-table.py`, and pinned by
`BLAKE2b-512(key = "misaka-palw/rand/gauss-q24/v1", the 65,536 entries as LE i32)` =
`0b3c29bd5d81b9db…5285aa4` (in full: `consensus-vectors/rand-v1/domains.json`,
`misaka_palw_gen::rand::GAUSS_Q24_V1_DIGEST_HEX`). Neither the digit count nor the tie rule decides
any entry: the closest scaled entry to a rounding tie is `1.0·10^−5` from it (Gaussian) and `1.3·10^−4`
(D11's Gumbel table), so a double-double generator gives the same table, and RHAZ (here) against
round-half-even (D11's script) is a difference of wording only (finding G10). Properties:

- it is data rather than arithmetic, exactly as PALW-EX-5 treats "a transcendental evaluated at
  registration";
- it is strictly increasing;
- it is antisymmetric (`G[65535 − i] = −G[i]`), so the table's mean is exactly 0;
- every entry has probability `2^−16`;
- it reaches `|z| ≈ 4.33`;
- its centre spacing is `≈ 3.8·10^−5 σ`, far below a Q12 latent's resolution.

For comparison, D11's `2^13` resolution would stop at `≈ 3.84σ`. Resolution `2^16` puts about 4 values
of a 262,144-value latent at each end entry, against about 32 at `2^13`.

**Why `Normal` is an input kind and not an in-program `Gather` over a class-carried table.** The
lookup is the same "data, not arithmetic" either way. Doing it at the job layer has four advantages:

1. The interval is exact. A param takes its dtype's full range (PALW-TIR-9), so a class-carried table
   would need a never-firing clamp and would leave a wide interval.
2. No class carries 256 KiB of table, and no noise cone opens an artifact leaf.
3. "Seed `s`, domain `d`, shape `X`" is the same tensor in every class, as a protocol guarantee
   rather than a convention.
4. It is the shape D11's Gumbel lookup already has.

A class that wants another distribution declares `Uniform` and gathers its own table in its program.

##### I.1.6 RFC-0001's sampler is domain 0, unchanged

RFC-0001 §A is Implementation Frozen, and nothing here changes a byte of it.

- **The index.** `gumbel_index_v1(seed, position, lane)` *is* `R(seed, TEXT_GUMBEL_V1, —, position,
  lane)`:
  - `K_0 = "misaka-palw/decode-select-v2/gumbel/v1"`;
  - `enc_0 = seed ‖ le32(position) ‖ le64(lane)`;
  - `b_0 = 13`, `W_0 = 1`.
- **The value and its consumer.** The value is `PALW_GUMBEL_Q24_V1[w]`, pinned by
  `PALW_GUMBEL_Q24_V1_DIGEST_HEX`. Its consumer is the lane key `value·2^24 + T_q·G` in the §A
  pipeline's step 6 (RFC-0001 §A.3). `value` is the class's Q24 logit (§I.3.5), so `value·2^24` is
  Q48; `T_q` and `G` are Q24, so `T_q·G` is Q48 too, and the key is `2^48·(v + T·g)`: Gumbel-max, a
  draw from `softmax(v / T)`. **Corrected 2026-10-01 (finding G1 of the independent second
  implementation).** An earlier text of this section, and the shipped function, carried
  `((T_q·G) >> 24)`, which put the noise in Q24 beside a Q48 logit term and made the temperature inert
  (0 non-greedy draws in 3,600 at `T = 1` on fp-v4's rows). The function, RFC-0001 §A.3 and
  `consensus-vectors/fp-v4/` now carry the unshifted key; `T_q = 0` is unchanged byte for byte.
- **`position`** is the committed logits row: row 0 is the one the last prefill position produced.
- **Vectors.** D11's pinned table digest and the FP V4 golden vectors that exercise seeded selection
  (`consensus-vectors/fp-v4/`) stand as domain 0's vectors. Since the correction above they do exercise
  it: about half of the sampled selections in them are a lane the greedy rule would not choose (finding
  G2: before, every expected lane was the greedy one, so no vector could tell a working sampler from an
  inert one). `rand-v1` adds raw index vectors
  computed by the shipped `gumbel_index_v1`, so the two can never drift apart.

**Where domain 0 differs from the generic layout, and why it stays.**

- There is no `step` coordinate, because text has one scan.
- It uses one hash per lane. The court opens two lanes per dispute (I-2), so blocking would buy
  nothing there.
- Its word is 13 bits.

These are values of the family's parameters, not exceptions to its grammar.

**What this RFC takes from D11.**

- The keyed-BLAKE2b discipline.
- The rule that the court takes sampler inputs **from the claim, never from the challenger**
  (`PalwDecodeSamplingV2`).
- The pinned-table transform.
- The argument that a seed inside the job id cannot be changed after the fact.

**What stays outside programs.** Text selection (the lane key and the §A pipeline) remains a
job-layer rule that consumes an LM program's logits. RFC-0002's non-goal ("sampling stays in the logits
scheme and the FP job") is kept.

##### I.1.7 Derived inputs: how a program receives R and how the court recomputes it

A program declares `Random { domain, dist }` inputs (§I.2.3). At position `p`, element `e` of such an
input is `dist(R(seed, domain, step, position, e))`, where `step = p` for a per-step domain and 0
otherwise, and `position` is the job's item index.

- **The executor** computes the input with the consensus function. It never uses a framework RNG
  (torch, cuRAND).
- **The court's demand evaluator** (04b §9.4) gains a sixth question, `input(p, j, i)`. For a random
  input the *court answers it itself*, from the claim's job: it is never opened from a leaf and never
  taken from the challenger. The work is one hash per distinct block, charged as elements under the
  existing limits.
- **PALW-TIR-33 does not apply**: nothing was committed, so nothing can be malformed. An executor that
  used other noise is convicted at the first commit point whose cone reads the noise, because the
  court computes the true noise there.

##### I.1.8 Properties

- **Order-, partition- and device-independence.** Each element is a pure function of
  `(seed, domain, step, position, lane)`, and no generator state exists. Any split across threads,
  GPUs, batches or resumptions computes identical values. A framework generator's values depend on
  draw order, device and generator type; that dependence is gone.
- **Domain separation.** Distinct keys give independent PRFs, so one seed yields unrelated streams in
  two domains.
- **Quality.** BLAKE2b-512 as a keyed PRF: its outputs are indistinguishable from uniform, and so are
  the words inside a digest.
- **Cost.** A keyed call is two compressions (the key block, then the message). The key block's state
  is per-domain constant, so an implementation MAY precompute it. That is about one compression per 32
  Gaussian values: the 262,144 initial-noise values of a 1024² image take 8,192 calls, about a
  millisecond on one core.

#### I.2 Canonical tensors and the execution profile

##### I.2.1 One meaning, so no profile

In float ML stacks an *execution profile* exists because one model gives different bits under
different kernels. It covers dtype and autocast, TF32 on or off, cuBLAS/cuDNN algorithm choice,
attention backend, deterministic flags and device count. In MISAKA the consensus meaning of a class is
its integer program:

- **Types and rounding are part of the program.** Every value is an integer of a declared type. Loss
  happens only at named sites (PALW-TIR-2/3). No float accumulation or TF32 path exists to toggle
  (ADR-0040 A).
- **Kernel choice is outside the identity.** A fused kernel is correct iff it is byte-identical at
  every commit point, and the court runs the reference evaluator (F-1/F-3, PALW-TIR-17).
- **Parallelism cannot change a value.** Reordering is legal only inside order-free regions, whose
  every partial sum is proved not to overflow (PALW-TIR-4/24). R is per element (§I.1.8).

Every axis of a profile therefore either lives inside the program (dtype, quantisation, rounding, the
scheduler's tables) or cannot affect a bit (kernels, threads, devices, batching). Nodes still choose
kernels, but that is node configuration, never identity.

##### I.2.2 What remains of the concern

Five concerns survive. None of them is an execution profile.

| Concern | Rule |
| --- | --- |
| **Input preprocessing** — file decoding (JPEG/PNG/HEIC, MP3/AAC/Opus, MP4), orientation, colour management, resampling | Consensus starts at a **canonical integer tensor** (u8 HWC pixels, PCM i16). Decoders stay outside. Resize, normalise and patchify are either done by the gateway to a class-declared size (disclosed, non-consensus) or are an integer stage of the pipeline (§II.4) |
| **Tokenisation and templates** | Text to ids stays at the gateway; the job carries ids (the RFC-0001 stop-string precedent). A class's fixed prompt template (an encoder's system prefix, its pad token, its maximum length) is class data (§II.1.3.4) |
| **Presentation encoders** | PNG/WebP/JPEG, WAV/FLAC/Opus, MP4/WebM and UTF-8 rendering are never consensus (§I.3) |
| **The float reference for fidelity** | Pinned library versions, eager attention, fp32, and the *same noise* as the integer run. It decides usefulness, never validity (RFC-0002 criterion 5) |
| **Node performance configuration** | Backends, fused kernels, threads and batch sizes are node policy, held byte-identical by F-1 |

##### I.2.3 Canonical tensors at every boundary: the program surface every profile shares

A tensor crosses a boundary in three places: job to program (inputs), stage to stage (edges), and
program to user (outputs). PALW-TIR v1 has only the text case of each: `Input(0)` is a token and
`Input(1)` the position (04b §3.2); the output is a `logits` node consumed by the decode rule. §II.1.3.7
derives the gap from images. The minimal shared fix is below. The **primitive set is unchanged**:
program version 2 declares `PRIM_SET_ID_V1`.

```
TirProgramV2 = TirProgramV1 with
  version: 2
  inputs:  [InputDecl]                      // NEW — Ref::Input(2 + j) names inputs[j]; Input(0), Input(1) as in V1
  output:  Logits { node, scheme_id }       // replaces V1's `logits` + `logits_scheme_id`; V1's meaning exactly
         | Rows   { node }                  // the node's value at every position (an encoder's hidden rows)
         | Final  { node }                  // the node's value at the last position (a latent, an image)
InputDecl   { name: String, dtype, shape: [u32], source: InputSource }
InputSource = External { lo: i64, hi: i64 }                      // bound by the pipeline: a job value or an upstream output
            | Random   { domain: u16, dist: Uniform{bits: u8} | Normal }      // §I.1.7
```

- **Inputs.** At most 16; rank ≤ 4; at most `2^28` elements; each used; names unique. `External`
  inputs are `i8`/`i16`/`i32`/`idx` with `lo ≤ hi` inside the dtype. `Uniform` is `idx`, `Normal` is
  `i32`, and domain 0 is not allowed (text selection is not a program input).

  Range analysis (04b §7) gains the leaves `External → [lo, hi]`, `Uniform{b} → [0, 2^b − 1]` and
  `Normal → [G[0], G[65535]]`. Inputs cost nothing (§8), like params. They are constant over the scan,
  except per-step random domains, which are keyed by `p`.
- **Output kinds.** `Logits` keeps V1 and Phase F exactly: `post` runs only at positions whose logits
  are consumed, and it writes no state. `Rows` and `Final` need `post` at every position. For those
  two kinds **`post` MAY `StateWrite` a global `Fixed` state that `pre` does not write**. NF-19's
  one-writer-per-instance-per-step is kept, and `post` still appends to no history.

  In §9.4, the writer of a global state is occurrence 0's `StateWrite` or occurrence `L + 1`'s,
  whichever exists; NF-19 makes it unique. The output node is committed, has a committable dtype and
  has no `H` (as NF-6).
- **What V2 is not.** V2 has no control flow and no new primitive. No input is written by the
  program. A V1 program keeps being admitted as V1. A V2 program with no inputs and a `Logits` output
  computes what its V1 twin does, but under a different `graph_ir_root`.

**The pipeline class.** A class may be several programs run in sequence, each over its own scan:

```
TirPipelineV1 {
  version: 1,
  profile: u8,                               // Image | Embedding | Audio | Video (| Text, later)
  stages:  [StageDecl],                      // 1..=16, in execution order
  output:  { stage: u8, spec: OutputSpecV1 }, // §I.3
  offers:  ProfileOffers,                    // the job-parameter domains the class accepts (Part II)
}
StageDecl {
  name: String,
  program: [u8; 64],                         // graph_ir_root; the program bytes ride the registration (Phase F D3)
  layout:  PalwTirLayoutV1,                  // Phase F D5, per stage
  trip:    Fixed(u32) | JobSteps | TokenCount,   // the stage's T, a job fact known at acceptance
  tokens:  Option<TokenRule>,                // Input(0) at each position
  bind:    [Binding],                        // one per External input, in declaration order
}
TokenRule { prefix: [u32], source: Prompt | Negative, suffix: [u32], pad: Option<(u32 id, u32 to_len)> }
Binding = JobScalar(field) | JobTokens(TokenRule, u32 len)
        | StageRows { stage, drop: u32, pad_to: u32 } | StageFinal { stage } | StageRowCount { stage, drop: u32 }
tir_pipeline_class_id_v1 = H64(key "misaka-palw/tir/pipeline-class-id/v1",
                               le16(version) ‖ u8(profile) ‖ pipeline_root ‖ terms_digest ‖ artifact_root ‖ tokenizer_id)
pipeline_root = H64(key "misaka-palw/gen/pipeline-root/v1",
                    le32(|pipeline|) ‖ pipeline ‖ le16(#programs) ‖ graph_ir_root(program_0) ‖ … ‖ graph_ir_root(program_(P−1)))
terms_digest  = H64(key "misaka-palw/gen/class-terms/v1",
                    borsh(layouts) ‖ borsh(OutputSpecV1) ‖ borsh(offers))
```

The class id binds every fact a court or an admission reads (finding G16): the pipeline's bytes and every
program (through its `graph_ir_root`, 04b §3.6), every stage's layout (`PalwTirLayoutV1`: `max_context`,
`checkpoint_interval`, `h_tile`, `commit_tiles`, `state_tiles`), the output header, the offers (the step
counts, the job scalars' intervals, the prompt and negative-prompt maxima, the image slots, the source
maximum, the forced prefix, the source floor and, per profile, the sampler, guidance and steps scalars of an
image class or the pooling and widths of an embedding class), the profile and the version, the weights'
`artifact_root` and the tokenizer. `borsh` is the class record's own field-by-field encoding
(`PalwGenClassV1`, `palw_gen_class_v1.rs`; the grammar of each part, the profile offers included, is
04b §15.14). The class-id vector is `consensus-vectors/tir-v2/class-id.json` (eight classes and fifteen
single-field changes), generated by the independent second implementation
(`misaka-palw-gen-ref2/scripts/gen_proposed_vectors.sh`, branch `rfc3/ref2`) and held against the consensus
layer by `consensus/core/tests/palw_gen_vectors_v1.rs`.

- **Edges are structural.** Each input element maps to one committed element of an earlier stage's
  output, to a job value, or to a zero pad. There is no arithmetic in an edge, and an edge reads only
  earlier stages. Admission proves every edge's upstream proven interval (∪ {0} when padded) ⊆ the
  input's `[lo, hi]`, and the shapes agree.
- **One step tree** (Phase F D7, generalised). Stage-major, then per stage Phase F §2.5's order:
  commit points by slot, `Fixed` checkpoints every `C`. **There are no `Hist` tile leaves** (finding
  G14): a history row is the committed value of its `HistAppend` operand at an earlier position, and a
  cone that reduces over the history is dissected (RFC-0002 F7, §II.2.1) or must fit the court whole,
  so no `Hist` leaf is needed for a leaf to be adjudicated from leaves that precede it; `h_tile` is a
  layout field with no leaf of its own (it still enters the class id). A `Rows`/`Final` program's `post`
  commit points appear at every position. Stage outputs are commit points, so
  Phase F's invariant holds: *every leaf is adjudicated from leaves that precede it*. The first
  divergent leaf of the ladder is therefore always adjudicable.
- **Admission.** Admission checks:
  - every stage program under `tir_admit_v1` with the V2 rules;
  - every edge;
  - the job-parameter domains;
  - the output spec (§I.3.2);
  - the per-job cost and leaves under per-profile ceilings (§Security, *ceilings*);
  - the class id.

  A per-job trip count is bounded by `offers` (`JobSteps ≤` the largest offered step count;
  `TokenCount ≤` the class's maximum prompt length).
- **A pipeline is not a VM.** Stages run in declared order, always, each over a trip count fixed at
  acceptance. There is no condition and no loop between them (PALW-TIR-18 holds per stage).

#### I.3 Canonical outputs

##### I.3.1 The rule

**The consensus output of a job is the value of the class's output node, a committed integer tensor,
serialised in the canonical byte form of its kind. Its digest is part of the claim. Everything a user
sees beyond those bytes is presentation.**

The output node is already committed as step leaves. The digest adds no trust. It adds a *portable
identity* for the result: users, explorers and caches can name "the image with digest X" without
the step tree. It also adds a two-opening consistency check.

##### I.3.2 The digest

```
OutputSpecV1 { kind: u8, shape: [u32], meta: [u8] }        // meta per kind (§I.3.3)
canonical bytes B = the output node's elements, row-major, each in the kind's element encoding (below)
tile t      = the bytes of the node's lanes [t·tile_len, min((t+1)·tile_len, E)) — the node's step tiles
leaf_t      = H64(key "misaka-palw/output/tile/v1", le32(t) ‖ bytes of tile t)
node(l, r)  = H64(key "misaka-palw/output/node/v1", l ‖ r)
merkle_root = the root over leaf_0 … leaf_(n−1): level by level, neighbours paired with node(l, r);
              an odd last node of a level is promoted to the next level unchanged (never duplicated,
              never hashed with itself); one leaf is its own root, with no further hash
output_root = H64(key "misaka-palw/output/root/v1", borsh(OutputSpecV1) ‖ le32(tile_len) ‖ merkle_root)
```

- **The preimage binds the tiling.** `le32(tile_len)` is part of `output_root` (04b §15.7), so a root
  names its tiling: one output at two tile lengths has two roots (finding G3).
- **A tile's proof** is its sibling path, bottom-up, one hash for every level at which the tile's node
  has a sibling (a level at which it is the promoted odd node contributes none). Verification
  recomputes the leaf from `t` and the bytes, folds the path with `node`, binds `borsh(spec)` and
  `le32(tile_len)`, and compares with `output_root`; the tile's byte length must be the one the header
  and `tile_len` imply, and the path must be consumed exactly (finding G4; the proofs of
  `consensus-vectors/output-v1/digests.json` pin it).
- **A header is bounded** (finding G7; 04b §2.2's tensor caps, which the output node inherits).
  `tile_len ∈ [4, 65,536]` lanes; `kind ∈ 0 … 5` (§I.3.3's numbering); every dimension in `[1, 2^24]`
  and at most `2^28` elements; rank 1 (`Tokens`), 3 (`ImageRgb8`), 2 (`PcmI16`, `EmbeddingI32`), 4
  (`VideoRgb8`), 1 – 4 (`TensorLe`); `meta` as §I.3.3 gives it. **The element count is checked as it
  grows and saturates**: a header whose count passes `2^28` — a video `[2^24, 2^24, 2^24, 3]`, a rank-4
  `[2^24; 4]` — is a named `Shape` refusal at the class preflight, at the registration admission and at
  the preflight a node asks before it signs, never an arithmetic fault (finding G9: the first
  implementation multiplied the dimensions unchecked, and a registrant's header reached it).
  The refusal names are `UnknownKind`, `Shape`, `Meta`, `Count`, `Domain`, `TileLen`. Which of several
  applicable refusals a header with several faults gets is not specified (no verdict depends on it;
  accept and refuse never differ).
- **The value domain.** Admission proves that the output node's interval lies inside the kind's value
  domain (for example `[0, 255]` for pixels), so every honest lane has a canonical encoding.
- **A malformed lane.** A lane outside that domain is already a PALW-TIR-33 violation.
- **The fault.** An executor whose step tile and output tile disagree is convicted by the new
  structural fault **`TirOutputDigestMismatch { value_index }`** (discriminant 21), `value_index` the
  lane, within the opened tile, at which the output node's committed step tile and the output tile
  differ — the shape of Phase F's `TirLogitsTraceMismatch { value_index }`, and hashed into the
  evidence id (finding G17). A lane outside the output node's proven interval is
  `TirValueOutsideProvenInterval { value_index }` (PALW-TIR-33). It needs two openings and one move
  (the output close, `GenOutputTile`, court proof tag 16, 04b §15.13): the output tile under `output_root` and the output
  node's step tile of the same lanes under its stage's root; evidence that does not hold convicts
  nobody.
- **The execution root.** A tensor-output pipeline's execution root binds `output_root` where an LM
  binds `full_logits_trace_root`:
  `H64(key "misaka-palw/gen/tensor-execution-root/v1", gen_job_id_v1 ‖ class_id ‖ le64(step_leaf_count) ‖ step_root ‖ output_root)`.

##### I.3.3 Per output kind

| Kind | Consensus bytes (element encoding) | Header (`OutputSpecV1`) | Presentation, never consensus |
| --- | --- | --- | --- |
| **Text** (`Tokens`, kind 0) | the committed generated token ids, `u32` LE, cut at the committed stop or end of generation (RFC-0001 §A.3 step 7, ADR-0096 D6) — as the FP commitment already carries them; **a text claim commits no `output_root`** (its answer is its generated ids, bound in its execution root) | tokenizer (class), count; shape `[n]`, no `meta` | UTF-8 detokenisation, stop-string trimming, chat formatting |
| **Image** `ImageRgb8` (1) | raw `u8`, HWC row-major, RGB, top-left origin, no alpha, no padding; values as the program emits them (no gamma conversion, no ICC) | shape `[H, W, 3]`, no `meta` | PNG/WebP/JPEG, metadata, colour profiles, thumbnails |
| **Audio** `PcmI16` (2) | PCM `i16` LE, interleaved by channel, frames in time order | shape `[frames, channels]`, `meta = le32(sample_rate)`, `sample_rate ≥ 1` | WAV/FLAC/Opus/MP3 containers, resampling, loudness normalisation |
| **Embedding** `EmbeddingI32` (3) | `i32` LE `[n, d]` row-major in the class's fixed point (`q` fractional bits); normalisation, if any, done in the program | shape `[n, d]`, `meta = [q, normalised]`, `q ≤ 31`, `normalised ∈ {0, 1}` | float conversion (`v / 2^q`), similarity search, vector-DB formats |
| **Video** `VideoRgb8` (4) | raw `u8` THWC (frames, then image rows) | shape `[T, H, W, 3]`, `meta = le32(fps_num) ‖ le32(fps_den)`, both `≥ 1` | MP4/WebM/GIF |
| **Tensor** `TensorLe` (5) | the node's dtype (`i8`/`i16`/`i32`) LE row-major | shape of rank 1 – 4, `meta = [dtype, q]`, `dtype ∈ {0 (i8), 1 (i16), 2 (i32)}`, `q ≤ 62` | — (latent-only outputs, §II.1 alternatives) |

**Kind 0 and the digest.** `Tokens` has the same tile-Merkle `output_root` as every other kind (the
vector `tokens/7` of `consensus-vectors/output-v1/digests.json`). No RFC-0003 claim uses it: a text claim
answers with its generated ids, which its execution root binds. It exists for outputs that are id lists
which must be named by a portable digest, such as RFC-0004's finalized outputs (§II.2.3) (finding G8).

##### I.3.4 Why raw tensors, not a canonical PNG, WAV or MP4

A canonical PNG must fix each row's filter, the deflate stream (the compressor's exact choices,
which differ across implementations and versions), the chunk order and the metadata. Pinning a
compressor bit for bit is a consensus dependency on third-party code. It is the same class of problem
as trusting a JPEG decoder on the input side. The only trivially canonical PNG uses stored
(uncompressed) deflate blocks. That is the raw bytes plus a wrapper, and it adds nothing to them.
Lossy formats (JPEG, Opus, H.264) are non-canonical by nature.

So raw bytes are consensus. A gateway MAY serve a *reproducible PNG* (filter 0 on every row, stored
deflate) that anyone can derive from the raw bytes, without making it consensus. Output bytes never
ride the chain: the user's own node holds them (P1), and the court opens tiles.

##### I.3.5 The logits unit of a text class

**Decided 2026-10-01, on finding G1 of the independent second implementation.** A text class's
committed logits — the `Logits` output of a version-1 program, or of a pipeline's text stage (§II.2.1)
— are in **natural-log units × 2^24 (Q24)**: the committed `i32` `v` stands for the real logit
`v / 2^24`. RFC-0001 §A.3 measures every decode control that reads the logits' unit —
`temperature_q`, the frequency and presence penalties and the logit bias — in Q24, the legacy classes'
own fixed point, and D11's key (`value·2^24 + T_q·G`, §I.1.6) reads the committed integer as Q24.

- **A convention of the lowering, not a field of the class.** A program says no unit (`logits` and
  `logits_scheme_id`: spec 04b §15.12), and tir-lower's calibrated logit scale is a real number of its
  own choosing (5.3·10^-9 … 7.2·10^-8 on nine tiny fixtures: not even a power of two). The lowerer
  guarantees Q24 by ending a text program's `post` in the rescale to it, as a versioned lowering
  change. The chain cannot check a unit; a class whose lowerer does not guarantee it mis-scales only
  its own sampling. A class-declared unit was considered and not taken.
- **What is offered to whom.**
  - Every legacy class commits Q24: every control is offered.
  - An IR or generative text class registered **before** the later fence that opens the free-prompt
    lane to IR and pipeline classes is offered greedy selection, the repeat penalty, stop sequences
    and constraints only. A job asking it for the temperature, a frequency or presence penalty or a
    logit bias is refused by name (`DecodeControlNeedsQ24Logits`: the V4 walk skips it, V5 acceptance
    refuses it), dormant with `palw_fp_decode_rules`.
  - That later fence offers temperature, penalties and bias to a class **registered past it**, Q24 by
    construction of the lowering the fence names. A class registered earlier stays refused, the live
    SmolLM2 class among them: nothing on chain says what its unit is.
- **Checked off chain**: a runtime pack's verification compares the class's logit scale with the
  float reference.

#### I.4 The generative claim: a tensor job on the free-prompt lane

**Decided 2026-10-01 (coordinator: Option B).** A generative class that can register but never carry a
claim is not complete. A tensor job (`PalwGenJobV1`, §I.0: an image, an embedding) therefore reaches the
chain as a **free-prompt commitment of FP job version 10**, follows the lane's commitment → claim →
licence → panel → court flow, and — in this release — **earns nothing**: it is a *weightless claim*, as
RFC-0004's evaluation claim (FP job version 9, spec 17 §17.8.4) is. A tensor claim that reaches `Final`
says that a panel replayed the job and reproduced its step tree and its canonical output, and that no
court convicted the executor. This section fixes what is carried, what the chain checks and derives at
acceptance, what a claim reserves, what the panel and the court read, and which fence opens it. It is the
contract the build follows (spec 04b §15.15) and what a second implementation computes.

##### I.4.1 The job version and the wire

| FP job version | Job |
| --- | --- |
| 5 | V3 |
| 6 | ADR-0096 D8's constraint job (named, unbuilt) |
| 7 | V4 (RFC-0001 §A) |
| 8 | V5 — a V4 job with images or a source (§II.2.1, §II.2.2) |
| 9 | evaluation job (RFC-0004 §7.2) |
| **10** | **tensor job** (this section) |

Version 10 is provisional as 8 is (RFC-0001 may renumber at freeze); it binds nothing a network runs
until the fence arms. A version-10 commitment is the lane's commitment, in the lane's layout:
`PalwFreePromptCommitmentV3` over a job of the lane's one job type (`PalwFreePromptJobV3`) at
`version = 10`, in the lane's payload (`PalwFpCommitmentTxPayloadV3`), under the lane's signature (ML-DSA-87
over the claim id) — so the claim id is the lane's `fp_claim_id_v3` and every wrapper carries the job with
no layout change. The job's Borsh is the V3 fields, then the **tail**

```
PalwGenJobTailV1 { seed: [u8; 32], body: PalwGenBodyV1 }      // after the V3 fields; no decode config, no V5 tail
```

and nothing else. The V3 fields are the job's envelope (§I.0) where the envelope has them, and canonical
zeros where it has not — **one encoding per behaviour**, so a second spelling of one job is refused, never
read:

| lane field | tensor job |
| --- | --- |
| `network_domain`, `class_id`, `executor_bond`, `executor_pubkey`, `operator_id`, `anchor_block`, `anchor_daa`, `job_nonce`, `privacy_mode`, `prompt_mode` | the `PalwJobEnvelopeV1`'s, with the same meanings |
| `tokenizer_id`, `prompt_token_ids_hash`, `prompt_tokens`, `decode_token_limit`, `max_context_tokens`, `sampling_seed`, `temperature_q` | **zero** — the class binds the tokenizer (§I.0), the body carries the text commitments, the seed is the tail's; a non-zero value is refused (`ShellNotCanonical`) |
| `decode` | absent |

The job the chain reads is the lossless reconstruction `PalwGenJobV1 { version: 1, envelope, seed, body }`,
and **the job's id is `gen_job_id_v1` of it** (the lane's `fp_job_id_v3` of a version-10 job returns it), so
no field of the envelope, the seed or the body can change after the fact. The version word decides whether
the tail is read: no V3, V4 or V5 byte string decodes as a version-10 job, and every validator of those
versions refuses version 10 by name (`UnsupportedVersion`) — a build below the fence refuses a tensor claim
at its door exactly as it refuses version 9 or 8.

##### I.4.2 What a commitment carries

| commitment field | tensor claim |
| --- | --- |
| `trace_root` | the **step root**: the one step tree's root over the class's stages, stage-major (04b §15.14: `step_root`) |
| `output_root` | the **canonical output root** (§I.3.2) over the output node's canonical bytes at the output node's step tiles |
| `schedule_root` | zero |
| `execution_root` | `H64(key "misaka-palw/gen/tensor-execution-root/v1", job_id ‖ class_id ‖ le64 work_leaves ‖ trace_root ‖ output_root)` — **a function of the commitment's own fields**, which acceptance recomputes (`ExecutionRootNotItsParts`) |
| `decode_tokens_executed`, `stop_reason` | `0`, `ExactBudgetReached` (one encoding) |
| `work_leaves` | the step tree's leaf count — and the chain's own count of it (§I.4.5), never the executor's word |
| `trace_manifest_root`, `trace_chunk_count`, `trace_retention_daa` | zero, `1`, zero (as Phase F's pipeline claims: the units open step leaves and nodes under `trace_root`; the chain derives retention as `accepted_daa` plus the minimum) |

Because `execution_root` is recomputed from the roots beside it, a commitment cannot name a root its own
parts do not produce — the court's first test (the binding's parts produce the claim's execution root) can
fail for an executor only by lying about what it ran, and that lie is convicted at its first divergent leaf
(§II.1.5.3) or by the output close (§I.3.2, `TirOutputDigestMismatch`).

##### I.4.3 The payload and the material

The payload is the lane's. Under `PublicDa`, `prompt_token_ids` is the job's **prompt ids followed by its
negative ids** (`prompt_tokens + negative_tokens` of them; the body's two hashes are each over their own
list, in the network's form); under `PanelDa` the list is empty and the ids ride the capture served to the
panel (RFC-0003's decision 12: an image job defaults to `PanelDa`). **Images never ride the chain**: an
embedding's or an edit's image bytes (§II.4) travel to the worker and, with the capture, to the panel,
bound by the `ImageInputRefV1`'s `input_root` the body carries — as V5's do.

The panel's **material** for a tensor claim is its request frame — the job, its prompt and negative ids,
its images — served under the lane's material transport (opaque bytes pulled by claim id; the lane bounds
and deduplicates them and the consumer checks them). Nothing in the transport reads it: what binds it to
the claim is the job id inside the execution root, so a frame of another job reproduces another root and
convicts nobody. A seat holds the frame, replays the job over its held class
(`gen_tensor_seat_judge_v1`) and files `Valid` iff the replay's execution root is the claim's. **A seat that
finds a difference files nothing** (the lane's rule, ADR-0077 W10): a sampled verdict never slashes, and the
fault is the court's question.

##### I.4.4 The doors

Nothing opens below its fence — three gates, as every lane fence has (and as RFC-0004's version 9 has):

1. **The isolation door** (transaction validation, height-free): where the ruleset carries
   `palw_fp_job_v5`, a payload whose job is version 10 is checked by the tensor door
   (`validate_palw_fp_gen_commitment_tx_v1`): it decodes as a version-10 payload, its shell is canonical,
   its job is canonical in itself (`PalwGenJobV1::decode_canonical`'s rules, the network-free ones; the
   modes; the seed's shape), its commitment's fields are §I.4.2's, its `execution_root` is its parts', its
   ids (where carried) are the body's, its signature has the lane's length, and `work_leaves` is within the
   structural cap. Where the ruleset has no `palw_fp_job_v5` — every shipped preset — the lane's own door
   refuses the bytes by name.
2. **The header-context door** (the block's height): a version-10 payload at a containing block below
   `palw_fp_job_v5` is refused, so a build that schedules the fence and one that does not agree on every
   transaction below it (the one refuses it here, the other at its isolation door).
3. **The acceptance walk**: past the fence only, a version-10 payload that passes the walk's own checks —
   the same bounds an FP commitment meets at its class's ladder, and the signature under the key it
   carries — becomes a **`GenTensorCommitted`** object (the walk's product, as `FreePromptCommitted` is the
   lane's: never serialized to a peer, object tag 87 in spec 17 §17.0's allocation) whose tensor job the
   fold reads. Total over whatever was accepted: a payload that fails is skipped with its reason, never
   rejected, so a peer's payload cannot invalidate the block that carried it.

##### I.4.5 What the chain derives and refuses at acceptance (the fold)

The lane's arm checks the bond exactly as for any claim (it exists, the key is its own, it is not retiring,
frozen or below the producer floor) and hands a version-10 commitment to the tensor branch, which checks,
in this order — every refusal by name, none clamps or corrects:

1. **the fences**: `palw_gen_v1` and `palw_fp_job_v5` in force at the accepting block (the second lock);
2. **the class**: a registry row whose status is **`Active`** (not `Frozen`, not awaiting its activation
   height, not reclaimed); a `gen_classes` row of a **tensor profile** (Image or Embedding; a text class
   takes V4/V5, and a profile with no body is not built); and the class's verification-deadline admission
   (ADR-0152 §4-quater V2), as the lane asks it of any claim. **The model registry's lifecycle is not
   asked.** A pipeline's graph derives no priced work, so its lifecycle row opens as `Registered` and stays
   there — the registry's ramp is the attempt lane's probes, which a pipeline has none of — and a gate that
   asked the row would refuse every claim of every generative class (measured: `ClassNotAdmitting`, the
   first chain-level run of this carriage). The lane's own bounds do the gating instead: **readiness** (below),
   step 5 and §I.4.8. **A claim flows only once seats can replay it** (the coordinator's decision of 2026-10-01):
   at least a panel's worth (`seat_count`) of *distinct operators*, never the executor's, hold the class with a
   fresh readiness V2 possession proof — the registry's own five-clause predicate (active, above the floor, a
   fresh row, free collateral for the readiness multiple) — else `GenClassNotReady`. Without it the lane would
   accept a claim no panel can be drawn for, holding its executor's reservation and a slot of the class for the
   whole bind window. (The outsider seat of a bought class is drawn from the base population and must hold the
   class too to file `Valid`; the count above is the class's own operators.) **The floor is one shared
   function**: `class_seating_v1` (`palw_class_seating_v1`) returns, for a class and an executor, each ready
   operator once with its ready bonds, the executor's bond and operator excluded, and the panel's size. Lane
   F's staged-enablement proposal (RFC-0002 Part II §II.7.5 Proposal A, approved 2026-10-01) makes it the one
   door every class kind asks — IR, generative and composite — under a dormant fence `palw_class_seating`, and
   adds an independence floor over the same map (at least `⌊seat_count / 2⌋ + 1` base-population operators
   that are neither the registrant nor the executor); the floor above is its condition 1 and does not change;
3. **the job against the class**: `palw_gen_job_resolve_class_v1` — the body kind is the profile, the modes,
   the seed rule, every parameter in the class's offers, one image reference per slot at its size — and,
   where the ids are held, `palw_gen_job_ids_admitted_v1` (hashes, counts, the class's token bound);
4. **the work**: `work_leaves` equals the leaf count of the job's step space as the chain derives it from
   the class's pipeline, programs and layouts and the job's facts (the court's own count,
   `StepLeafCountNotCanonical`); a commitment whose count differs is refused, not corrected;
5. **the class's capacity**: the class's claims in flight on the lane are fewer than the fence's
   per-profile `max_inflight_claims` (§I.4.8), and the executor holds fewer than `⌈cap / 2⌉` of them not yet
   licensed (T-2(a)'s per-bond share, over the lane's cap: the registry's `c_class` is a lifecycle row's, which
   a pipeline class never has) — a generative class has no panel-room budget, so the cap and the share are
   the fence's;
6. **the work identity**: `work_id = H64(key "misaka-palw/gen/work-id/v1", class_id ‖ borsh(tail) ‖
   executor bond)`; a live claim of it refuses (`DuplicateWork`). One inference is one claim per bond: the
   same job under another nonce or anchor is the same work, and another bond's run of the job is another;
7. **the reservation and the exposure ceiling** (§I.4.6);
8. **the claim**: `source = FreePrompt { quanta: 0 }`, `pwu = 0`, no weight, no receipt rights, no escrow;
   the roots as committed; `trace_chunk_count = 1`, `trace_retention_daa = accepted_daa + the minimum
   retention`; the job's pin as the lane records it; the work identity; phase `Provisional`, the bind
   deadline armed. From here the claim is any claim of its class: panel at the bind, receipts, licence,
   `Final`, the court — and the lane's slash on a conviction.

##### I.4.6 Pricing: leaves are the work

The canonical-work rule of the lane (ADR-0074 Decision 5: "leaves are the work") carries over unchanged for
the unit: **a tensor claim's work is its exact step leaves**, counted by the chain (§I.4.5 step 4). That fixes
what a claim costs its executor and its panel:

- **reservation** — the lane's stage-1 rule on the claim's leaves at the class's registered
  `slash_value_per_pwu`: `⌈work_leaves × slash_value_per_pwu / ρ⌉` with `ρ = 1`, through the weight-cap
  reservation of the lane (`palw_claim_weight_reservation_of_v1`), held against the executor's committed
  ledger room under the lane's exposure ceiling. A false claim is slashed like any other.
- **no input is priced separately.** An image slot's stage is in the step tree and its leaves are in
  `work_leaves`; the slot's `token_equivalents` and the source's `source_token_floor` are the V5 text lane's
  (OQ13, OQ15 — **still pending user confirmation**, their recommended values the placeholders they are) and
  nothing in a tensor claim reads them. A tensor class declares `token_equivalents = 0`.
- **weight is a later fence.** A tensor claim has no quanta and no tickets: the lane's certification
  (ADR-0075's `fp_certified_classes`) guards weight only and no drilled family certificate covers a V2
  pipeline yet (Security and economic analysis: "Weight"), and the compute-priced era's work vector — OQ7's
  classification of activation-by-activation `MatMul`s as `attention_prefill`, decided 2026-09-28 —
  matters only where claims are priced in compute. When a later fence prices tensor claims, the unit is
  already fixed: `quanta = ⌊work_leaves / quantum⌋` against the class's own canonical job
  (`pwu_per_inference` at registration is the count of the class's most expensive offered job) and the panel
  economics of "Security and economic analysis" (`(1 + replicas) × MACs`) are its price. **The requester's
  payment to an executor is the gateway's, outside consensus**, in this release.

##### I.4.7 The court

**The closes are the generative ones, and they are built.** A tensor claim's `execution_root` is checked
against the binding a close carries (`PalwGenTensorBindingV1`), the class is the registry row the binding's
job names, and the closes are `GenCone` (10) at a divergent leaf, the root claim (tag 69) and dissection at a
dissected leaf (RFC-0002 F7 composed), and `GenOutputTile` (**16**: spec 17 §17.0 reserves 13–15 for
RFC-0004's evaluation proofs; the discriminant is explicit) for a canonical output that is not the step
tree's — under the generative court version the fence carries. The capture (`GenTensorCaptureV1`) is the
accused's data availability, served over the opening lane within the claim's retention.

**How a session is reached depends on the regime, and the first draft of this section missed it (finding
G23).** Where bisection is played, a challenger opens `CourtOpened`, the ladder narrows to a leaf, and the
closes above end the dispute; nothing more is needed. **Under the held regime (ADR-0103 Decision 1;
testnet-12 from genesis) `CourtOpened` is refused for every claim**: the court's entrance is a one-move
accusation, and the one that exists (`TirShardCourtAccused`, tag 62) names claims of IR classes only. A
tensor claim therefore had **no court at all** on testnet-12 — its reservation could never be slashed, and the
lane's one deterrent against a false execution would have been absent exactly where the lane runs. It is
closed before the fence can open:

**Decision 20 — `GenShardCourtAccused`, the generative one-move accusation** (`palw_gen_one_move_v1`; object
tag 88, approved 2026-10-01). Tag 62's twin with the registry turned to `gen_classes`:

```
PalwGenOneMoveAccusationV1 { version: u16 = 1, claim, execution_root, trace_root, executor_bond, accuser_bond,
                             verdict, proof: GenCone | GenOutputTile | GenDecodeToken, signature }
session_id = H64(key "misaka-palw/gen/one-move/session/v1", le32(|domain|) ‖ domain ‖ le16(version) ‖ claim
                 ‖ execution_root ‖ trace_root ‖ borsh(executor_bond) ‖ borsh(accuser_bond) ‖ borsh(verdict)
                 ‖ le64(|borsh(proof)|) ‖ borsh(proof))
signature  = ML-DSA-87 over session_id, context "misaka-palw/gen/one-move/accuse/mldsa87/v1"
```

- **Shape** (stateless, where it rides and again at acceptance): the version, a signature, an admissible
  proof (`GenDissection` is a session's bottom and has no session here), a proof whose binding commits the
  accusation's `execution_root`, an executor that is not the accuser.
- **Acceptance**: the fence; the shape; the accuser's registered key over the session id; a live claim of a
  generative class whose executor and roots are the accusation's; and the **verdict re-derived** by
  `adjudicate_close_proof_v2` — the court close's own function: its cost ceiling, the network's prompt form,
  the claim's `gen_classes` row, the binding's job and roots against the claim's — at the block's court and
  ladder, refused if it differs from the declared one in either direction.
- **Fold**: the legacy one-move court's gates in their order (a live claim, the executor and the roots, an
  accuser that is not the producer, the open-session rule, an Active accuser at or above the floor), then
  `ExecutorGuilty` convicts (the claim voids as `CourtFraud` and its executor is charged),
  `ChallengerDefeated` charges the accuser what a false accusation costs, and a cone accusation at a
  **dissected** leaf declares `ExecutorGuilty` and opens the dissection at that leaf — the responder's
  generative root claim (tag 69) is its first move.
- It is a court opening: it spends the block's adjudication slot (its evaluation is bounded by the court's
  limits, not its bytes), is dropped by name below `palw_gen_v1`, and rides at every height.

**What a one-move accusation can carry (finding G24; decision 22, approved 2026-10-01).** The accusation is
one object in one lifecycle carrier — about 100,000 bytes (`PALW_OBJECT_CHUNK_MAX_BYTES`) — and the chunk path
for larger objects is the certification lane's allow-list (`palw_chunked_object_kind_admitted_v1`:
`FamilyCertified` only); a close's own chunks (`CourtCloseDeclared`/`CourtCloseChunk`, up to 32 × 100,000 B,
PALW-TIR-38) are keyed to a session's two bonds, and a chain that plays no bisection opens a session only by
naming a dissected leaf (ADR-0103 Decision 5). So on a held-regime chain a lie is convictable in one move
**only at a leaf whose close fits one carrier**. This binds RFC-0002's IR classes equally, **including the live
ones on testnet-12, and is recorded as a known limitation there**: a lie at a leaf whose close exceeds one
carrier is **refused its licence but not slashed** (the seats replay it and file no receipt, so the claim never
reaches `Final` and its reservation is released when the receipt window lapses; the liar loses fees and time,
no collateral). Tag 62's semantics below the fence are not changed.

**Decision 22 — the held leaf challenge** (`HeldLeafChallengeDeclared`, object tag 90; fence
`palw_held_close_chunks_v1`; spec 04b §15.15.6 is normative). One generic object, for IR and pipeline classes
alike, **in place of a new chunk table keyed to (claim, accuser bond)**: a 60-site state surface (root, delta,
carriage, sweeps) is avoided by reusing the one the court already has. The accuser signs a named-leaf
challenge that carries the declaration of its close — the leaf, the claim and its roots, the two bonds, the
chunk count and the digests of each chunk and of the whole. The fold opens a session at `Terminal` on that
leaf exactly as ADR-0103 Decision 5 does for a dissected leaf (the session id derives from the claim, the
roots, the two parties and the space, so it is **keyed to (claim, accuser) by construction**) and writes the
**challenger-side close group** the existing `CourtCloseDeclared` would have written; the close then rides the
court's own `CourtCloseChunk` objects (up to the carried cap, `min(max_close_chunks, 32) × 100,000` B), the
completing chunk assembles it, the acceptance layer adjudicates it at the named leaf, and a conviction applies
through the `CourtClosed` arm. Tag 62 and tag 88 are untouched, and below the fence nothing changes.

*How it bounds griefing in the held regime.* **The accuser alone bears the assembly clock** (`4 · count` DAA,
at most 128, inside the session's backstop) **and the deposit** (`count` carriers' carriage, a charge against
its own bond when the session ends without the close it pinned); the object pays the declaration's grading
rent. **If it never completes the close** — or completes one that does not decode or does not adjudicate to
`ExecutorGuilty` — the deposit is forfeited, the accuser is charged what a lost held dissection costs, **the
claim is not convicted, voided or slashed**, and its licence anchor moves to the close so the seats keep a whole
challenge window (a decoy cannot carry a lie to `Final`). **Concurrency is capped per claim** (one challenge,
then one per panel seat, at most `1 + seat_count`; a decoy of the producer's holds off no seat), per network
(`palw_max_concurrent_court_sessions_v1`, each session holding the accuser's reservation) and per block (one
close completion graded). The executor is not put on a clock for a close the accuser has pinned: while the
challenger's declared close stands the session waits at `Terminal`.

**Until the fence is armed**, PALW-GEN-21 refuses registration of a pipeline class on a held-regime network
whose exact terminal close exceeds one carrier less the accusation's framing; once armed the bound is the carried
cap. The reduced image profile (§II.1.6) is far inside one carrier (a few KB a cone); an Example-C-sized class
(weights alone are `T · K` bytes a tile, §II.1.5.4a) needs the fence. The fence arms with the next flag day (the
generative, decode or improvement one); the live IR classes on testnet-12 are unchanged now.

**Evidence.** Through the real fold, over a real execution of the toy image class (its weights, worker, step
tree and output digest): the worker's binding is the commitment's execution root and the chain's own count of
the job's leaves; a planted output lie is convicted by the output close and a step-tile lie by the cone
close; an honest claim accused is acquitted and its accuser charged; and every refusal above is named.

##### I.4.8 Capacity and denial of service

A tensor claim costs a panel a full replay (§Security and economic analysis). Four limits bound what an
attacker can ask of the seats: the lane's reservation by leaves against the executor's collateral
(§I.4.6); **a per-class cap on claims in flight** — `max_inflight_claims` in the fence's per-profile
ceilings (`PalwGenProfileCeilingsV1`), hashed into the fence's value as every ceiling is — because a
generative class has no panel-room budget (its registry lifecycle row is `Registered` and never admits, and
the fold does not ask it); **a per-bond share of that cap** — one bond holds at most `⌈cap / 2⌉` of the
class's claims not yet licensed (T-2(a)'s rule over the lane's cap), so one executor cannot take every slot
and refuse the others; and the class's **status** — a claim is taken only while it is `Active`. The drill's
cap is 16 for every profile; **the network's value is provisional until the first profile's corpus is
measured** (OQ5's discipline) and is the fence's to set.

##### I.4.9 Which fence opens it

**`palw_fp_job_v5`, over `palw_gen_v1`.** The lane table (Activation plan) says it in one sentence: the
free-prompt lane opens for a pipeline class at its own later fence, and "for a pipeline class that lane is
FP Job V5's (`palw_fp_job_v5`)". A tensor job is a pipeline class's job, so it takes that lane's fence;
the node already names the pair (`palw_gen_lane_open_v1`: `palw_gen_v1` and `palw_fp_job_v5` in force).

- **One lane, one fence.** Every lane rule a pipeline class's claim depends on is shared between its text
  and its tensor jobs — the class registry row (`palw_gen_v1`), the commitment door, the seat's readiness
  and replay, the court — so a second fence would be a second flag day, a second drill and a second
  Some-only fingerprint entry for one door, and would add no safety: nothing opens below either.
- **The fence's prerequisites fit.** `validate_palw_fp_job_v5_v1` requires `palw_gen_v1` and
  `palw_fp_decode_rules` at or below it. A tensor job reads no decode rules, but arming the decode rules
  first is already the order, and the pair costs nothing.
- **Which job versions the walk admits is a property of the build, not of the height.** This release's
  walk admits version 10 past the fence and skips version 8 whatever the height; V5 joins it in the
  release that also confirms OQ13 and OQ15's price. The tensor lane therefore does not wait for the V5
  text lane's economics, and the fence's name stays the one the lane table gives it.
- **The fallback.** Should the user want tensor jobs open while V5 must stay shut *by a fence value*
  rather than by a release, a dedicated fence (`palw_gen_job_v1`, the same shape) replaces
  `palw_fp_job_v5` in exactly one function (`palw_gen_lane_open_v1`), one validator and one Params field;
  nothing else in the carriage moves.

##### I.4.10 What is not here

Weight and rewards for tensor claims (§I.4.6); sampled seat verification of a pipeline claim (a seat
replays the whole job, as §Security's panel economics assume — the lane's interval sampling is a later
optimisation for long classes); V5's walk (version 8 stays skipped, §I.4.9; a text pipeline claim's
one-move accusation is built but unreachable until it opens); several images in one claim (a job is one
image: one inference, one claim); audio and video bodies; the requester's payment; and a one-move
accusation larger than one carrier (G24: the held leaf challenge, §I.4.7, decision 22).

##### I.4.11 Allocations

Every number the carriage takes, in the spaces spec 17 §17.0 allocates for every branch:

| Space | Value | Note |
| --- | --- | --- |
| FP job version | **10** | the tensor job (§I.4.1) |
| consensus object tag | **87** `GenTensorCommitted` | the walk's product, never serialized; approved 2026-10-01 |
| consensus object tag | **88** `GenShardCourtAccused` | the generative one-move accusation (§I.4.7); approved 2026-10-01 |
| consensus object tag | **90** `HeldLeafChallengeDeclared` | the held leaf challenge, generic for IR and pipelines (§I.4.7, decision 22); approved 2026-10-01 (89 is RFC-0004's `CourtEvalRootClaimed`) |
| court proof tag | **16** `GenOutputTile` | explicit discriminant; 13–15 are RFC-0004's |
| fence | `palw_fp_job_v5` over `palw_gen_v1` | the per-profile `max_inflight_claims` is in `palw_gen_v1`'s value (§I.4.8) |
| fence | `palw_held_close_chunks_v1` | dormant, a bare height; requires `palw_tir_v1` and `palw_held_context` at or below it; arms with the next flag day (§I.4.7) |
| keys and contexts | `"misaka-palw/gen/work-id/v1"`, `"misaka-palw/gen/one-move/session/v1"`, `"misaka-palw/gen/one-move/accuse/mldsa87/v1"`, `"misaka-palw/held-close/challenge/v1"`, `"misaka-palw/held-close/challenge/mldsa87/v1"` | |

---

### Part II — Class profiles

Each profile states its job body, its canonical output, how it maps onto the TIR scan and states,
what (if anything) the IR lacks, and court feasibility. Image is the detailed one. The others reuse
it and Part I.

---

<a id="source-v1-0003-L01794"></a>

### 旧RFC-0003 / 原文1794–2252行

出典: [0003-palw-generative-model-classes.md](../rfc/0003-palw-generative-model-classes.md)。配置理由: 主題別統合。

#### II.2 Text generation

**Job fields.** FP Job V4, exactly as RFC-0001 §A froze it: `PalwFreePromptJobV3` fields at version
7, then `DecodeConfigV4`. That means the envelope, prompt ids, `decode_token_limit`,
`max_context_tokens`, `sampling_seed`, `temperature_q` and the decode controls. Nothing here redefines,
extends or reorders any of it.

**Canonical output.** The committed generated token ids, cut at the committed stop or end of
generation. UTF-8 is presentation (§I.3.3).

**Mapping onto the TIR scan.** An LM program is RFC-0002's `TirProgramV1` with output `Logits`, and
Phase F integrates it: one position per token, `Hist` for attention, `Fixed` for recurrences. RFC-0001
§A's pipeline selects the next token from the committed logits *outside* the program, with its
randomness as R's domain 0 (§I.1.6). A future TIR text class is a one-stage pipeline whose job is FP
Job V4. A vision-language class (§II.4) adds an image stage and needs a later FP job version that
embeds V4 unchanged plus image inputs.

**What the IR lacks.** Nothing.

**Court feasibility.** Phase F's arms (`TirCone`, H dissection, `TirLogits`,
`TirDecodeToken(Tiled)`) plus RFC-0001's per-lane two-tile refutation of the selection (I-2).

This RFC sits *under* RFC-0001 (its randomness is domain 0) and *beside* it (canonical outputs), and
changes nothing in it.

##### II.2.1 Text pipelines and the vision-language job (the VLM path)

Scheduled by the coordinator on 2026-09-29, after `JobImage` (§II.4). Everything here is dormant
under `palw_gen_v1`. Items that belong to RFC-0001's live text lane are marked **[RFC-0001]**; none
is built before that lane's owner agrees.

**The text stage.** A TIR text class is a pipeline whose **output stage is its language model**. That
stage is a `Logits` program, with RFC-0002's `TirProgramV1` meaning lifted as version 2
(`Logits { node, scheme_id }`), and its trip rule is **`TextStream`** (04b §15.6, `TripRule` tag 3):

- Its positions are the text job's stream, one position per id: the prompt ids, then the generated
  ids. `T = |prompt| + |generated| − 1`, because the last generated id is never fed back.
- Its `Input(0)` at position `p` is the stream's `p`-th id. It has no token rule; the stream is the
  job's.
- It reads earlier stages through the ordinary bindings (`StageFinal`, `StageRows`); a text-only
  class has no earlier stage.
- Its `max_trip` is the class's `max_context`, Phase F's `prefill + decode − 1` bound.
- Only the output stage may be `TextStream`, and it must be a `Logits` program. A `Logits` program
  may only be that stage (NF-P9′). Every other stage keeps PALW-GEN-1's trip count fixed at
  acceptance. The text stage's trip count is the claim's committed stream length, within the job's
  limits, exactly as a Phase F text class's is.

Nothing else about text changes (PALW-GEN-10 stands):

- the job of a class with no image slot is **FP Job V4** (RFC-0001 §A), unchanged;
- selection, the decode controls and stop are RFC-0001 §A.3's. They are applied outside the program
  to the committed logits, with `R`'s domain 0 (§I.1.6);
- the court over the text stage is Phase F's: `TirCone`, H dissection, `TirLogits` and
  `TirDecodeToken`;
- a job whose `prompt_tokens + decode_token_limit − 1` exceeds `max_context` is refused at acceptance,
  as Phase F refuses it.

**The vision-language job: FP Job V5 (image inputs) [RFC-0001].** A class with image slots takes this
job, which embeds V4 unchanged:

```
PalwFreePromptJobV5 = { v4: PalwFreePromptJobV4, images: Vec<ImageInputRefV1> }   // images: 1..=16, one per slot
wire                = le16(8) ‖ (borsh(v4) without its version word) ‖ borsh(images)
fp_job_id_v5        = H64(key "misaka-palw/fp-v5/job-id/v1", le64(|bytes|) ‖ bytes)   // the whole borsh
```

The embedded job *is* an FP Job V4 (version 7, its `DecodeConfigV4` present), and V4's own rules
admit it verbatim. On the wire only its version word says 8. No V3/V4 rule admits version 8, no V4
byte string decodes as V5, and V4's wire, ids, acceptance and fingerprints do not move. Built
dormant on `rfc3/fp-v5` (2026-09-29), behind the fence `palw_fp_job_v5`, which requires
`palw_gen_v1` and `palw_fp_decode_rules` at or below it.

- **One encoding per behaviour** (open question 14, decided 2026-09-29). A class with image slots takes
  V5 only, and a class without slots takes V4 only (`JobVersionNotOffered`). `images` is never empty,
  so a text job has exactly one encoding.
- `images` satisfies PALW-GEN-11: exactly one image per slot, at the slot's size.
- **Where images travel.** Image bytes never ride a transaction, whatever `privacy_mode` says about the
  prompt ids. They travel with the capture to the panel, and a dispute opens tiles against
  `input_root`. `PublicDa` makes a V5 job's prompt ids public, not its images.
- **Price [RFC-0001] — recommendation, pending user confirmation (open question 13).** RFC-0001's D10
  pays decode leaves only (prefill is 0). A V5 job's image stages are real work, so they are priced as
  **prefill-equivalent tokens**:
  - each image slot of the class declares `token_equivalents`, a count of prompt tokens, in its
    offer. The class id covers it;
  - admission floors it at `⌈admitted per-image work / per-token work⌉`. Per-image work is every
    non-text stage's admitted job work in MAC-equivalents (ADR-0131's table), split evenly over the
    images the stage depends on. Per-token work is the text stage's admitted work for one position.
    A non-text stage that depends on no image is refused, because it would be prefill no rule
    prices;
  - a V5 job is charged the sum of its slots' counts at the job's per-token price.

  Built as pure functions (`palw_fp_v5_image_tokens_v1`, `palw_fp_v5_image_charge_v1`). Nothing in the
  lane reads them until the user confirms.
- **Numbering.** The version number and the name are RFC-0001's to assign at freeze. RFC-0001 §A.8
  keeps "V5" for an emergency fence of V4; if that fence comes first, this job takes the next free
  version. Nothing here binds a name.

**Placement by placeholder ids (no job field).** This replaces §II.4's `start` job scalar.

- The image rows enter the text stage through one `StageFinal` edge from the vision stage: rows
  `[N_img, d_text]`, every slot's rows concatenated in slot order.
- The LM program places them itself. A `Fixed` state `cursor` counts the image rows placed so far.
  At a position whose token is the class's image placeholder id (a constant of the program) **while
  `cursor < N_img`**, the input embedding is `Gather(image_rows, cursor)` and the cursor advances.
  At any other position — including **a placeholder after the `N_img`-th, which is an ordinary token,
  embedded as a token** — the input embedding is the token's embedding.
- Placement is therefore a total, deterministic function of the prompt ids. It needs no job field
  and no acceptance rule, and a court replays it like any other state. A prompt with fewer
  placeholders than image rows uses fewer rows. A placeholder past the last row is embedded as a
  token, as HF embeds a generated placeholder, and as the lowering does (`rfc3/lower` 4c25416a5,
  hf-coverage §16). The two readings agree whenever a prompt has at most `N_img` placeholders, which
  chat templates guarantee; the class's chat template (the gateway's) emits exactly `N_img`.

**The step tree and the court.** One tree, stage-major (PALW-GEN-3): the image stages' leaves, then
the text stage's.

- A dispute over a text-stage leaf is Phase F's. Its cone may read image rows, which are the vision
  stage's committed output, carried under PALW-TIR-33 (a `StageFinal` edge). A dispute over a row is
  a dispute over the vision stage's leaf, adjudicated at the first divergent leaf.
- The vision stage's own cones read image lanes from tiles proven under `input_root`
  (PALW-TIR-48).
- **As built** (`rfc3/fp-v5`, 2026-09-29, dormant; library level):
  - the tree is structural, from the programs, the layouts and the trip counts. Per position: every
    commit point in slot order, cut into its commit tile. The text stage's logits are leaves only
    where the decode consumes them. After every `C`-th position, every `Fixed` state not written in
    `post`. Keyed leaves, a Merkle root per stage bound with its count, and one step root over the
    stage roots, as follows (finding G18; 04b §15.14 states it in full, and the vectors
    `consensus-vectors/tir-v2/step-tree.json` — generated by `misaka-palw-gen-ref2/scripts/gen_proposed_vectors.sh`
    and held against the consensus layer by `consensus/core/tests/palw_gen_vectors_v1.rs` — pin it):

    ```
    leaf       = H64(key "misaka-palw/gen/step-leaf/v1", borsh(coord) ‖ le32(value_count) ‖ lanes)
    coord      = u8 stage ‖ le32 pos ‖ kind ‖ le32 tile
    kind       = 0 ‖ le16 occurrence ‖ le16 node                       // a commit tile
               | 1 ‖ le16 state ‖ option                               // a Fixed-state checkpoint tile; option = 0 | 1 ‖ le16 layer
    lanes      = every value as four little-endian bytes (an `idx` as u32, every other dtype as i32; PALW-TIR-5)
    node       = H64(key "misaka-palw/gen/step-node/v1", left ‖ right)  // an odd last node is promoted unchanged
    stage_root = H64(key "misaka-palw/gen/stage-root/v1", u8 stage ‖ le64 leaf_count ‖ merkle_root)
    step_root  = H64(key "misaka-palw/gen/step-root/v1", le16 stage_count ‖ stage_root_0 ‖ … ‖ stage_root_(n−1))
    ```

    A leaf binds neither its dtype nor its `first_element` (both follow from the coordinate and the
    class's layout), and a stage root binds the stage index and the leaf count, so a stage's tree can be
    neither relabelled nor cut;
  - the worker runs the stages and then the text stage through FP Job V4's decoder. The panel
    re-executes and names a changed answer, a changed stage root or an unbound root;
  - the court adjudicates any leaf of any stage from the carried leaves before it (earlier stages
    entirely), the class's params, `R`, the job's facts, edges read under PALW-TIR-33 and image
    lanes from proven tiles. The decode door holds each generated id to the V4 selection over the
    committed logits row;
  - V5 rides the lane's job type as version 8, its images after the V4 tail (the same bytes as the
    wrapper). The commitment, the payload, the claim id and the signed message carry it with no
    layout change. The V4 validators refuse it, and the V5 validator applies V4's commitment rules
    to its V4 view past `palw_fp_job_v5`.

  - **the pipeline admission and the registry** (`palw_gen_admission_v1`, dormant under
    `palw_gen_v1`): the preflight; the one step tree counted exactly in closed form — the widest job
    within the court's ladder and `max_job_step_leaves`, and `pwu_per_inference` equal to the count
    of the class's most expensive offered job; every commit point's cone at its own tile length
    against the court (a cone that reduces over the history must fit whole — the generative court
    dissects no history before court version 3); the class id; and 0‰, since no attempt lane
    exists for pipelines. The registration is `ClassRegisteredGenV1`, object tag **68** (renumbered
    from 67 on 2026-09-29: RFC-0002's second IR fence takes 67 for `DefaultAccusedTirLeaf` and
    merges first). The fold writes the class's `gen_classes` row (rooted without the class's
    bytes, delta entry 90, carriage tail `0xC2`). A V5 claim's class is the row its job names: a
    text class, its tokenizer, its slots and its stream;
  - **the params against the artifact root** (`palw_gen_artifact_v1`): the pipeline inventory is
    Phase F's per program, in program order, in one tree, each leaf named `p<k>/<name>`. A close
    carries the param leaves its cone reads with their paths; the court reads a weight only from a
    leaf proven under the class's `artifact_root`, so an executor that ran other weights is convicted
    at its first divergent leaf. **Nodes load a class from a `PALWTIR2` file**
    (`misaka-palw-tir-artifact::v2`): the pipeline's and every program's canonical bytes, the class
    as declared, the tokenizer id, and one tensor table in exactly this inventory's order (program
    by program, each program's declared params in PALWTIR1's order), 64-byte aligned, so the root
    streams from the file front to back. It is not a consensus object; the node holds a class from it
    only when its pipeline, programs, class and tokenizer are the row's and its tensors hash to the
    row's `artifact_root`. PALWTIR1 readers are unchanged and refuse it by its magic;

  - **the closes as consensus objects** (`palw_gen_close_v1`): a V5 claim's execution root binds
    the job id, the class, the step leaf count, the step root and the generated ids; `GenCone` (tag
    10) opens the leaf the ladder narrowed to (the claim's one stage-major order), `GenDecodeToken`
    (tag 11) holds an id to its committed logits row. The prompt rides only when the disputed stage
    reads it. Below `palw_gen_v1` both are dropped by name;
  - **the history dissection, composed** (RFC-0002 F7, spec 04b §9.5): a text stage's attention
    cone is dissected, not closed whole. Admission asks F7's obligations, value bound, sizing and
    window of the stage's view verbatim; `CourtGenRootClaimed` (tag **69**, renumbered from 68 with
    the registration) opens F7's own phase, whose rounds and choices are F7's objects unchanged;
    `GenDissection` (tag 12) is the bottom. Every stage is sized under the block's box-demand rules:
    below `palw_tir_fence2` the DAA-2,000 release's, under which a dissected cone with a `TopK` is
    refused by name; past it ref2's H7 row (RFC-0002's second IR fence), under which `V` covers a
    TopK tile and such a cone is admitted dissected like any other. Admission targets: tir-lower's lowered tiny LLaVA, Qwen2-VL and Qwen2.5-VL (`rfc3/lower`
    4c25416a5), each admitted with its attention dissected; LLaVA's attention leaf is argued end to
    end (an honest responder acquitted, a lie in the totals convicted wherever it hides).

  Built since: the node's worker, seat, capture and court halves (`misaka-palw-base0::gen_worker`,
  re-exported by kaspad's `palw_gen_seat`), the source input and the forced prefix (§II.2.2), and
  `PALWTIR2`. The kaspad panel wiring and the V5 lane itself (the walk admitting version 8) wait
  for the V5 lane's fence, which the user decides, and open questions 13 and 15's prices.

**Order of implementation for this path.**

1. 04b: `TextStream` and NF-P9′, and the IR's text run (a selector supplies each generated id). IR
   only, dormant.
2. Consensus, dormant under `palw_gen_v1`:
   - the class profile `Text` (a text pipeline, with or without image slots). Its canonical output is
     the committed generated ids, with no output root (§I.3.3);
   - its preflight (the output stage is the text stage, and `max_context`);
   - the placement pattern's conformance vector.
3. **[RFC-0001]** FP Job V5's wire type, id and acceptance; the image-stage price; the worker and the
   panel executing a pipeline class; the court's composition. The V4 job type lives on
   `rcore/fp-sampler`, so V5 is built on top of it, after that lane's owner agrees.

##### II.2.2 Encoder–decoder text classes: the source input and the forced prefix

Scheduled by the coordinator on 2026-09-29, after the node's loops; dormant under `palw_gen_v1` and
`palw_fp_job_v5`, marked **[RFC-0001]** where it touches the lane. tir-lower lowers T5, BART, mBART,
Marian and Pegasus as two-stage pipelines (`rfc3/lower` 8addd5224, hf-coverage §17): an encoder stage
over the source text (`Fixed { n: 1 }`, the source template bound through `JobTokens` and
`JobTokenCount`), then the decoder as the text stage over `TextStream`, reading every layer's cross
keys and values through `StageFinal`. Two things the lowering had to borrow are defined here.

**(a) The source input.** A sequence-to-sequence job has two texts: the source the encoder reads and
the decoder's stream. The lowering carried the source in the job's `negative` list, the only second
list a `PipelineJob` has.

- **The binding.** `TokenSource::Source` (tag 2; `Prompt` 0 and `Negative` 1 unchanged) is the job's
  source ids. Any `TokenRule` reads it as it reads the prompt: a stage's token run, `JobTokens`,
  `JobTokenCount`. `PipelineJob` gains `source`. A pipeline that names no source encodes as before.
- **The class offer.** `max_source_tokens`: 0 exactly when no rule reads the source (a class that
  reads one offers one), and only a text class offers one (the V5 job carries it). Every rule that
  reads the source must hold the longest offered source and its template (the prompt's check). The
  class id covers it, and `source_token_floor` (below).
- **The V5 job field [RFC-0001].** `source: Option<{ token_ids_hash, tokens }>`, after `images`:
  `le16(8) ‖ borsh(v4)[2..] ‖ borsh(images) ‖ borsh(source)`. **The ids travel as an image's bytes
  do**, bound by `token_ids_hash` in the network's prompt-id form: to the worker and the panel, and in
  a close whose stage reads them, never on the chain, under either privacy mode (a `PublicDa` job
  publishes its prompt, not its source). A V5 payload's `prompt_token_ids` is therefore the prompt's
  alone, and V4's payload rules and the fold's prompt accounting read it unchanged. **One
  encoding per behaviour extends open question 14's rule:** a class that has image slots **or reads
  a source** takes V5 only, and a class with neither takes V4 only. A V5 job carries one image per
  slot (none for a class without slots) and its source exactly when the class reads one, never
  neither. Acceptance holds `tokens` to `[1, max_source_tokens]`.
- **The court.** A close carries the source ids whole exactly when the disputed stage reads them (the
  prompt's carriage rule, §II.2.1); a dispute elsewhere reveals nothing of the source.
- **Price — recommendation, pending user confirmation (open question 15, with open question 13).**
  Source tokens are priced as prompt tokens, one per id, at the job's per-token price, **and never
  below the class's `source_token_floor`**. The floor is there because the lowered encoders run one
  position over the padded source axis: their work is the whole width whatever the source's length,
  so a short source charged per id would carry most of the encoder free. The registrant declares the
  floor and the preflight holds it at or above `⌈admitted source work / per-token work⌉`, the image
  slot's rule (open question 13): every stage but the text stage is input work, split evenly over
  the inputs it depends on, images and the source alike, and a text-class stage that depends on
  neither is refused (prefill no rule prices). A job is charged `max(source.tokens,
  source_token_floor)` prompt tokens for its source. Built as a pure function; nothing in the lane
  reads it until the user confirms.

**(b) The forced decoder prefix, declared per class.** A sequence-to-sequence decoder starts from
fixed ids: `decoder_start_token_id`, and for mBART-50 the target language (`forced_bos_token_id`). The
lowering carried them as the job's prompt. They are the class's, not the job's choice, so **the class
declares them**: `forced_prompt_prefix` in its offers (the class id covers it). **They ride as the
prompt's head, and the class fixes what that head is.** This is the hf-coverage option of checking
the prompt's head, not a prefix inside `TextStream`, for one reason: the job is FP Job V4 underneath,
and V4 refuses an empty prompt (`EmptyPrompt`) and hashes, prices and discloses the prompt as it
stands. A seq2seq job's decoder prompt is otherwise empty, so a prefix the IR supplied would leave
the V4 envelope a prompt it refuses, and V4 must not move. With the prefix at the prompt's head the
stream is still `prompt ‖ generated`, and every V4 rule, the step tree and the court read it
unchanged. The class fixes the head:

- the preflight: the prefix only on a text class; every id below the text stage's `token_bound`; no
  longer than `max_prompt_tokens`;
- acceptance: where the prompt's ids ride (`PublicDa`), the prompt starts with the prefix
  (`palw_fp_v5_accept_payload_v1`: the stateless V5 rules, the class resolved, then the head);
- where they do not (`PanelDa`): the worker refuses a job whose prompt does not start with it, and a
  seat judges such a job as not the class's (it files nothing, so the claim is never licensed). The
  executor bears the consequence of running it.

**Order of implementation for this path.** The IR's `TokenSource::Source`; the class offer and the
preflight; the V5 field, its acceptance and the lane's version-8 tail; the prefix's checks; the
carriage of source ids in a close; the worker, seat and capture; a conformance test on the lowered
encoder–decoders once their vectors are exported. Built dormant on `rfc3/fp-v5` (2026-09-29) through
the worker, seat and capture, with the golden toy encoder–decoder
(`consensus-vectors/tir-v2/pipelines/toy-encdec.json`) as its vector; the conformance test on the
lowered models waits for their vectors.

##### II.2.3 Evaluation pipelines: the `Decode` stage and finalized outputs (RFC-0004 §7.2)

RFC-0004's open question 8 recommends that its two additions to the pipeline format live here. They
are built dormant (`rfc4/eval`, 2026-09-29), appended to the format so that every earlier pipeline
encodes as before:

- **The `Decode` stage kind** (`TripRule` tag 4). It is the text stage's stream and trip count
  (`prompt ‖ generated`, `T = |prompt| + max(|generated|, 1) − 1`) in a stage that is **not** the
  output. An evaluation pipeline's subject stage decodes (Generate), or is given the ids
  (teacher-forced: its `generated` list is the reference), and the scoring stages after it read:
  - **what it decoded**, through the token source `Generated` (tag 3), which only a stage after the
    `Decode` stage may name;
  - **its consumed logits rows**, through `StageRows` / `StageRowCount`: position `|prompt| − 1` on,
    row `r` the one `generated[r]` came from, which are exactly the stage's committed logits leaves.

  NF-P9′ becomes: the `TextStream` stage is the output, the `Decode` stage never is, and a pipeline
  has at most one of them. The court treats a `Decode` stage as the text stage: its consumed logits
  are its leaves, and the decode door holds each id it selected (in Generate mode).
- **`FinalizedOutput { claim, stage }`** (token source tag 5). It is the generated ids of the job's
  `claim`-th finalized claim (`claim < 8`), from that claim's stream stage `stage`. It is a token
  source rather than a new binding, so a template, its padding and its count are the existing
  `JobTokens` / `JobTokenCount` bindings'. A pairwise judge reads two. RFC05's former VM `Tests` kind
  is withdrawn; a future bounded non-VM test relation needs a separate active kernel/task profile.
  The job carries the ids and the chain holds their commitment. Where that commitment lives and how a
  close carries the ids is the evaluation job family's (RFC-0004 A6).
- **The key** (token source `Key`, tag 4): an evaluation item's key ids, which an exact-match stage
  compares an answer span with. RFC-0004 §7.2 names two additions and nothing else, but an exact
  match needs the key as a list, so this is a third. It is flagged there.

**Tags, widths and rules** (finding G12 of the independent second implementation; 04b §15.6 carries the
grammar once the first of these is built at a base):

> **[decision, 2026-10-01 — to be confirmed against `rfc4/eval`'s vectors, `pipelines/eval-*.json`, which
> are not at the `rfc3/fp-v5` base]**
> 1. **Tags.** `TripRule::Decode` is tag 4. `TokenSource::Generated` is tag 3, `TokenSource::Key` tag 4 and
>    `TokenSource::FinalizedOutput { claim: u8, stage: u8 }` tag 5 (`claim < 8`; `stage` a stage index,
>    as `StageRows`'s is).
> 2. **Edges from a `Decode` stage.** NF-P7 and NF-P8's "reads a `Rows` stage" cover a `Decode` stage
>    for its *consumed* rows: `StageRows { drop, pad_to }` reads rows `|prompt| − 1 + drop …` (the rows
>    `generated[r]` came from, the stage's committed logits leaves), and `StageRowCount { drop }` counts
>    `T − (|prompt| − 1) − drop`. NF-P8's declared bound `[0, max_trip − drop]` is loose by `|prompt| − 1`
>    for such a stage, which is harmless: it is a bound, not the count.
> 3. **NF-P9′.** A pipeline has at most one text-kind stage (`TextStream` or `Decode`) in all; the
>    `TextStream` stage is the output and the `Decode` stage never is.

The scoring stages themselves (ExactMatch, RefLogLik, Judge, Pairwise) are ordinary programs of the
scoring library (`misaka_palw_tir::scoring`, vectors `consensus-vectors/tir-v2/scoring/` and
`pipelines/eval-*.json`), adjudicated as any committed leaf (04b PALW-TIR-50).

#### II.3 Embedding / encoder

**Job fields.** The body is `EmbeddingBodyV1`:

- `input`: text ids (hash and count, carried as FP), or one canonical image (§II.4) or clip (§II.5);
- `pooling`, which MUST equal the class's (`last`, `mean` or `cls`);
- `dims`, one of the class's offered output widths (a truncatable class `Slice`s the vector);
- `output: EmbeddingI32`.

The seed is all zeros (`SeedNotUsed` otherwise).

**An embedding embeds at least one id** (finding G19, decided 2026-10-01): a text of 0 ids is refused
(`InputNotOffered`: an empty text embeds nothing), which §I.0's zero-length encoding does not cover; an image
job's empty prompt (the unconditional prompt, §II.1.1) is accepted. **The ids' bound** (finding G20) is the
least of the `token_bound` of the stages whose token run reads the list and the `hi + 1` of the external
inputs a `JobTokens` binding of that list feeds — an id the run would refuse is refused at acceptance; a
class that reads its prompt only through such a binding (an embedding) is bounded by the second rule. The
refusal names of an embedding job are `PoolingNotOffered`, `DimsNotOffered`, `InputNotOffered`,
`OutputSpecNotOffered` and `Image(…)` (04b §15.13, decisions 8–10).

**Canonical output.** `i32` LE `[n, d]` in the class's fixed point, L2-normalised in the program when
the model does it (the library's `L2Norm`).

**Mapping onto the TIR scan.**

- A causal encoder is an LM program with a `Rows` output.
- Last-token pooling is its `Final` output.
- Mean pooling is a second, single-position stage that reduces the `Rows` edge (masked `ReduceSum`, a
  `Div` by the row count, the optional normalisation).
- A bidirectional encoder is a single-position program over a padded token axis.

**What the IR lacks.** Nothing beyond Part I.

**Court feasibility.** LM cones as in Phase F. A bidirectional encoder's attention fits up to about
15,600 tokens per head. A pooling tile reads `T · tile` lanes, which fits for `T · tile ≤ 4·10^6`;
longer inputs use committed partial sums (§II.1.5.5).

This is the smallest consumer of Part I: output kinds, a pipeline and the output digest, without
randomness. That makes it the smoke-test class for the drills.

#### II.4 Multimodal input (vision canonicalisation)

**Job fields.** `images: [ImageInputRefV1 { input_root, h, w }]`, exactly one per image slot of the
class.

- **Image slots.** The class declares its image slots in its offers: slot `i` is `{ h, w, tile_len }`,
  at most 16 slots, each input tile `tile_len ∈ [4, 2^16]` bytes. The class id covers the slots. A
  job carries exactly one image per slot, at the slot's size (`ImageCount`, `ImageSizeNotOffered`).
  Every slot is read (NF-P10), so no image is optional; a class that takes fewer images is another
  class, as a class at another resolution is.
- An image input is `u8` HWC RGB at its slot's size. `input_root` is §I.3.2's construction applied to
  the image's bytes: header `ImageRgb8 [h, w, 3]`, the slot's `tile_len`, index-bound tile leaves,
  keyed nodes. An input root and an output root of the same bytes are the same commitment.
- Decoding, EXIF orientation, colour management, alpha and **arbitrary-size resampling** are outside
  consensus: the gateway resizes or letterboxes to a declared size and says so.
- Image bytes are far larger than a transaction. They travel like `PanelDa` prompt ids (with the
  capture to the panel), the chain carries `input_root`, and a dispute opens tiles.

**Canonical output.** Per the consuming profile: text through FP (§II.2), or an embedding (§II.3).

**Binding** (04b §15.6, NF-P10). A stage reads image `i` through `JobImage { index: i }` (binding tag 6),
into an `External` input `i16 [h, w, 3]` whose interval contains `[0, 255]`. `i16`, because program
inputs have no `u8` and every pixel is exact in it. One image is bound at one size wherever it is
bound, and the bound images are `0 … n − 1`.

**Admission and court** (04b §15.4, §15.9, PALW-TIR-48). Admission opens an image input at **1 byte a
lane**, and counts it as an operand of its own (it is opened by tiles with their paths). The court
never holds an image. It reads an image lane only from an input tile that the parties carry, after
verifying the tile against the job's `input_root`. A tile not proven under the root is refused
evidence and convicts nobody. A lane whose tile nobody carried fails the evaluation (`Missing`).

**Mapping onto the TIR scan.** A single-position **preprocessing stage** in integer TIR:

- per-channel normalisation (`Mul`/`Add`/`Div` by pinned constants);
- an optional fixed-ratio resize as two `MatMul`s with pinned resampling matrices (filter weights
  evaluated at registration, so data);
- crop as `Slice`;
- patchify as `Reshape`/`Transpose`.

A vision encoder over patches (attention over a `Fixed` patch axis) follows, with a `Rows`/`Final`
output. The LM stage (the text stage, §II.2.1) reads that output through a `StageFinal` edge. It
places patch embeddings where the prompt's image placeholder ids are, counted by a `Fixed` cursor
(§II.2.1), so no job scalar is needed. Dynamic-resolution encoders become one class per resolution
bucket, as images are.

**What the IR lacks.** Nothing beyond Part I.

**Court feasibility.** Vision-encoder cones are the image transformer's (§II.1.5.4): a few thousand
patches fit. A resampling tile costs `tile · s` MACs for source extent `s ≤ 4,096`. Input tiles are
opened against `input_root`.

#### II.5 Audio — generation and understanding

**Job fields.** For understanding: `AudioInputRefV1 { input_root, sample_rate, channels, frames }`,
PCM `i16` at the class's declared rate. Decoding and resampling stay outside consensus. For
generation: text ids (and optionally a reference clip as an input), `clip_index`, the steps or token
budget from the class's offers, and `output: PcmI16`.

**Canonical output.** PCM `i16` LE interleaved, with header (sample rate, channels, frames). The last
stage clamps to `[−32768, 32767]`.

**Mapping onto the TIR scan.**

- **Understanding.** A feature stage in integer TIR:
  - framing (`Gather` with a pinned index table);
  - the window and DFT as `MatMul`s with pinned tables;
  - power (`Mul`/`Add`) and the mel filterbank (`MatMul`);
  - the logarithm (`IntLn`) and normalisation.

  Then an encoder over frames (a `Fixed` axis) and, for speech-to-text, a text decoder that is an LM
  program reading the encoder rows through an `External` input. Cross-attention is over a `Fixed`
  axis.
- **Generation.** Either an LM over codec tokens, selected by RFC-0001 §A's rule with domain 0,
  followed by a codec-decoder stage; or latent diffusion or flow over a `[C, T]` latent
  (`AUDIO_*_NOISE_V1`, the image profile in one dimension), followed by a decoder or vocoder stage.
  Transposed convolutions are zero insertion (`Concat`) plus convolution. Periodic activations are
  range reduction plus a table.

**What the IR lacks.** Nothing beyond Part I. A codec LM with several codebooks per position emits
several logits rows per position, which needs a rows-per-position field in the FP logits scheme, a
later FP job version. That is an FP-side extension, not an IR one.

**Court feasibility.** One-dimensional convolution cones are small. Long activations (for example
30 s × 44.1 kHz × 64 channels ≈ 8.5·10^7 elements) fit the `2^28` cap. Attention over thousands of
frames fits up to about 15,600 per head, beyond which committed partials apply.

---

<a id="source-v1-0003-L02290"></a>

### 旧RFC-0003 / 原文2290–2621行

出典: [0003-palw-generative-model-classes.md](../rfc/0003-palw-generative-model-classes.md)。配置理由: 主題別統合。

### Rules, alternatives, security and activation

#### Proposed Spec text (sketch)

A new chapter `spec/palw/04c-generative-classes.md`, plus additions to 04b. Applies past `palw_gen_v1`.

**Randomness.**

- **PALW-RND-1 (the function).** Every producer, seat and court MUST compute `R` as §I.1.2–I.1.3
  define it. No other randomness MAY enter an execution.
- **PALW-RND-2 (inputs).** `seed` MUST be the job's committed seed. `step`, `position` and `lane` MUST
  be the declared coordinates. A court MUST take them from the claim's job, never from a challenger.
- **PALW-RND-3 (domains).** A domain MUST be registered with its key, layout and width. A key MUST NOT
  be reused. Domain 0 is RFC-0001 D11, unchanged.
- **PALW-RND-4 (transforms).** `Uniform` and `Normal` values MUST be the word and
  `PALW_GAUSS_Q24_V1[word]` respectively.
- **PALW-RND-5 (derived inputs).** A random input MUST NOT be committed. A court MUST recompute every
  element a cone reads.
- **PALW-RND-6 (not a lottery input).** No eligibility, ticket or beacon rule MAY read `R`.
- **PALW-RND-7 (one declaration per domain).** A pipeline MUST declare each domain at most once.

**Canonical outputs.**

- **PALW-OUT-1 (consensus bytes).** A tensor output's consensus value MUST be its output node's
  elements in the kind's canonical byte form (§I.3.3).
- **PALW-OUT-2 (domain).** Admission MUST prove the output node's interval inside the kind's value
  domain.
- **PALW-OUT-3 (digest).** A claim MUST carry `output_root` computed as §I.3.2 defines it, over tiles
  aligned with the output node's step tiles.
- **PALW-OUT-4 (consistency).** A step tile and its output tile that disagree MUST convict the
  executor (`TirOutputDigestMismatch`).
- **PALW-OUT-5 (presentation).** No encoding beyond the canonical bytes MAY be consensus.

**PALW-TIR program version 2.**

- **PALW-TIR-37 (inputs).** An input MUST be `External`, with a declared interval, or `Random`. Range
  analysis MUST use the declared interval, the word range or the table range.
- **PALW-TIR-38 (output kinds).** The output MUST be `Logits`, `Rows` or `Final`, and it MUST be a
  committed node with a committable dtype and no `H`.
- **PALW-TIR-39 (`post` effects).** For `Rows`/`Final` outputs, `post` MAY write a global `Fixed`
  state that `pre` does not write. NF-19's one-writer rule still holds, and `post` MUST NOT append to
  a history.
- **PALW-TIR-40 (replay).** The writer of a global state MUST be read from occurrence 0 or `L + 1`,
  whichever writes it.
- **PALW-TIR-41 (primitives).** Version 2 MUST declare `PRIM_SET_ID_V1`.

**Pipelines and jobs.**

- **PALW-GEN-1 (pipeline).** A pipeline class MUST run its stages in declared order, each over a trip
  count fixed at acceptance.
- **PALW-GEN-2 (edges).** An edge MUST be structural and MUST read only earlier stages. Admission MUST
  prove each edge's intervals and shapes.
- **PALW-GEN-3 (one tree).** All stages' leaves MUST form one step tree, stage-major.
- **PALW-GEN-4 (identity).** The class id MUST be `tir_pipeline_class_id_v1`.
- **PALW-GEN-5 (the job).** A generative job MUST be `PalwGenJobV1` in canonical form. Acceptance
  MUST refuse every other encoding by name and MUST NOT rewrite a job.
- **PALW-GEN-6 (seed).** The seed MUST be all zeros iff the class declares no random input.
- **PALW-GEN-7 (committed operands).** PALW-TIR-33 applies to every stage. A stage output is the
  executor's statement.
- **PALW-GEN-8 (ceilings).** Per-job MACs, leaves, peak memory and pipeline bytes MUST be within the
  network's ceilings for the profile.
- **PALW-GEN-9 (image job).** An image job MUST satisfy §II.1.1's table.
- **PALW-GEN-10 (text).** Text generation is RFC-0001 §A (FP Job V4). This chapter adds no text rule.
- **PALW-GEN-11 (job images).** A job MUST carry exactly one `ImageInputRefV1` per image slot of its
  class, at the slot's size. An image's `input_root` MUST be §I.3.2's construction over its bytes
  (`ImageRgb8 [h, w, 3]`, the slot's `tile_len`). A court MUST read image lanes only from tiles
  proven under `input_root` (PALW-TIR-48).
- **PALW-GEN-12 (the text stage).** A text pipeline's output stage MUST be a `Logits` program whose trip
  rule is `TextStream`, and no other stage may be either. Its positions MUST be the text job's stream.
  Selection, the decode controls and stop MUST be RFC-0001 §A's.
- **PALW-GEN-13 (the vision-language job).** A class with image slots MUST take FP Job V5 only, and a
  class without slots FP Job V4 only. V5 MUST embed every V4 field unchanged.
- **PALW-GEN-14 (the tensor job's carriage).** A tensor job (§I.0) MUST be carried as FP job version 10:
  the lane's V3 fields (the envelope's, and zeros for every text field the envelope has not), then
  `PalwGenJobTailV1 { seed, body }`, and nothing else. The job's id MUST be `gen_job_id_v1` of the
  reconstructed `PalwGenJobV1`. Any other spelling MUST be refused by name.
- **PALW-GEN-15 (the tensor commitment).** A version-10 commitment's `trace_root` MUST be the step root,
  `output_root` the canonical output root and `execution_root` the tensor execution root of its own
  fields; `decode_tokens_executed` MUST be 0, `stop_reason` `ExactBudgetReached`, `schedule_root` and
  `trace_manifest_root` zero, `trace_chunk_count` 1 and `trace_retention_daa` 0. Acceptance MUST recompute
  `execution_root`.
- **PALW-GEN-16 (the work).** A tensor claim's `work_leaves` MUST equal the chain's count of the job's step
  space over the class's pipeline, programs and layouts; a claim whose count differs MUST be refused, not
  corrected.
- **PALW-GEN-17 (weightless).** A tensor claim MUST carry no quanta, no pwu, no receipt rights, no weight
  and no escrow in this release; it MUST reserve `⌈work_leaves × slash_value_per_pwu⌉` as the lane's
  stage-1 rule prices it, and MUST hold the executor's collateral as any claim of that work does.
- **PALW-GEN-18 (identity and capacity).** A tensor claim's work identity MUST be `H64(key
  "misaka-palw/gen/work-id/v1", class_id ‖ borsh(tail) ‖ bond)`; a second live claim of it MUST be refused.
  A class's claims in flight on the lane MUST be fewer than the fence's `max_inflight_claims` for its
  profile.
- **PALW-GEN-19 (the fence).** Nothing in PALW-GEN-14…18 MAY be reachable below `palw_fp_job_v5` over
  `palw_gen_v1`: the isolation door MUST refuse a version-10 payload where the ruleset has no
  `palw_fp_job_v5`, the header-context door MUST refuse it below the fence, and the fold MUST refuse it as
  the second lock.

#### Alternatives

| Alternative | Why not |
| --- | --- |
| One RFC per modality or concern (randomness, outputs, image, audio, …) | The point is one common layer. Separate documents invite separate wires, noises and output formats, which is the per-family release problem at the modality level |
| R as a TIR primitive (b), or as a library composite (c) | §I.1.1. (b) is a protocol upgrade for nothing. (c) gives unvetted, per-class generators and still needs input tensors |
| R keyed by the job id | §I.1.3. It gives randomness meaning to the executor-chosen `job_nonce`, breaks reproducibility across jobs, and diverges from D11 |
| Framework-compatible noise (reproduce `torch.randn` for a seed) | Not bit-exact across devices and generator types, and float Box–Muller is transcendental |
| Gaussian by in-IR Box–Muller (`IntLn` and a sine table), or by a sum of uniforms | More nodes and less exact tails; the sum of uniforms is not Gaussian in the tails. A pinned table is data |
| A pre-scan block, or a second scan axis, for the text encoder | No control flow: every position would pay for both (§3.4) |
| The denoise update outside the IR, as a protocol sampler like D11 | Arbitrary solver math would become protocol code, and the IR expresses it exactly |
| One claim per denoise step | Breaks "one inference, one claim" and multiplies claim overhead |
| **Latent-only output** (the decoder off chain; `TensorLe` output) | Viable as an interim if Phase A shows the decoder's commitment cost is prohibitive. But the user's pixels would then be presentation, unverified |
| A canonical PNG, WAV or MP4 as consensus | A dependency on a compressor (§I.3.4) |
| Multi-resolution classes by padding and masking | Always computes the maximum size, and complicates positions. Deferred; one class per size |
| Autoregressive image-token models (VQ tokens from an LM, then a VQ decoder stage) | Expressible in PALW-TIR v1 today, needing only the pipeline and output kinds. A cheaper path to on-chain images, but not the targets. A variant of the image profile |
| Float pipelines with tolerances | ADR-0053 |
| Cryptographic proofs of model computation | Permanently prohibited by the user's 2026-10-15 decision, PRINCIPLES §8; not a future alternative |
| **Lazy commitment**: commit only carry-outs and required points, and let the responder commit a block's interior on demand during a dispute (multi-level bisection) | Cuts execution-time commitment volume about tenfold, which matters for GPU speed and video. It is a new court protocol, so later work, not v1 |
| An "execution profile" object inside the class id | §I.2.1: the program already is the profile |

#### Security and economic analysis

- **Randomness grinding.**
  - The executor controls no input of R.
  - A requester choosing a seed buys a different output, never a better draw.
  - A seed re-roll costs a whole execution (ADR-0072).
  - R feeds no eligibility rule (PALW-RND-6).
  - The court takes the seed from the claim (PALW-RND-2).
- **Forged noise, edges or outputs.** Noise is recomputed. An edge lie is a lie in an upstream commit
  point, caught at the first divergent leaf. Output bytes are tied to step tiles by
  `TirOutputDigestMismatch` and to their domain by PALW-TIR-33.
- **Unadjudicable pipelines (A4).** Every stage program decodes under v1 primitives. Every cone fits
  or is refused at admission, and generalised dissection is refused until its court version is armed.
  The residual risk is an interpreter defect, the RFC-0002 residual.
- **Mis-fusion on GPUs.** Online-softmax attention and float or TF32 paths are not byte-identical. A
  backend that uses them convicts itself, never the network (F-1, F-4). Integer Winograd or FFT
  convolutions are exact only if implemented exactly.
- **Ceilings and denial of service.** Images are 10^4–10^6 times an LM position. Per-profile,
  per-job ceilings (MACs, leaves, peak live bytes, pipeline bytes) belong in the fence value. Job
  parameters are bounded sets. Prompt lengths are capped. Admission stays linear in program bytes.
- **Weight (ADR-0069).** The primitive set adds nothing to certify. The V2 features (inputs, `post`
  effects, pipelines, the digest fault) need their own drilled family certificate before an image
  class can earn weight.
- **Canonical work.** Activation-by-activation `MatMul`s over a `Fixed` axis (image attention,
  cross-attention) are neither `dense_matmul` nor `attention_*` under Phase F §2.9's rules today. The
  proposal classifies them as `attention_prefill` (an open question), so they are not priced as
  elementwise work.
- **Content and privacy.** No validity rule reads a prompt's meaning or an output's content
  (PALW-PR-1). Policy is the gateway's. Because generation is reproducible, **anyone holding a job's
  prompt ids and seed can regenerate its image**. With `PublicDa` prompts the image is effectively
  public; `PanelDa` limits that to the seats (until a dispute). A user wanting unlinkable images
  should use fresh random seeds.
- **Panel economics.** Every seat that re-executes pays the full cost. The quanta and pricing of image
  claims must reflect `(1 + replicas) × MACs`.

#### Compatibility and migration

- **RFC-0001 §A and the FP lane are untouched.** D11's bytes are unchanged, and its vectors are
  domain 0's.
- **PALW-TIR v1 programs and Phase F classes are unchanged.** Version 2 is admitted only past
  `palw_gen_v1`.
- **Legacy classes are untouched.**
- **`PalwGenJobV1` and the pipeline registration are appended object variants.** They are dropped by
  name before the fence and skipped by older builds under the A-2 tolerance, as Phase F's
  `ClassRegisteredTirV1` is.
- **FP job version 10 is the lane's job type with a tail** (§I.4.1). No V3, V4 or V5 byte string decodes
  as it and no earlier validator admits it, so the lane's wire, ids and acceptance below the fence are what
  they were; a tensor claim's `FreePromptCommitted` carries its job in a Borsh-skipped field, so the
  object's encoding does not move either.

#### Open questions

1. **R's key:** the job's seed (recommended) or the job id.
2. **Gaussian:** a protocol-side `Normal` input kind with a `2^16`-entry table (recommended), or an
   in-program `Gather` over a class-carried table; and whether the resolution should be `2^16`.
3. **One resolution per class** (recommended for v1), or multi-resolution classes.
4. **The decoder on chain from the first class** (recommended), or latent-only output as an interim.
5. **Per-profile ceilings.** Per-job MACs and leaves for denoise and decoder stages, the per-position
   leaf cap, and peak memory.
6. **Commitment volume at GPU speed.** Accept it for testnet, and treat packed lanes for `i8`/`i16`
   commit points, a GPU-friendly leaf hash, or lazy commitment as separate work?
7. **Canonical work.** Classify activation-by-activation `MatMul`s as `attention_prefill`
   (recommended), or add a component to `PalwCanonicalWorkVectorV1`.
8. **Registration carriage.** A pipeline's programs can exceed one carrier (Phase F D3's ≈ 88,000
   bytes). Should the multi-carrier lane that Phase F deferred come with this fence?
9. **The first testnet image model:** an Example-C-sized class, and which family.
10. **Generalised dissection (§II.1.5.6):** with the first image fence, or with video.
11. **Multi-codebook audio tokens:** a later FP job version for several logits rows per position.
12. **Privacy of reproducible outputs:** is the `PublicDa` or `PanelDa` choice enough, or should image
    jobs default to `PanelDa`?
13. **The price of a V5 job's image stages** (§II.2.1) [RFC-0001]. Recommendation, **pending user
    confirmation**: prefill-equivalent tokens. Each slot declares a count, floored at
    `⌈admitted per-image work / per-token work⌉` and charged at the job's per-token price.
14. **May a class with image slots also take text-only V4 jobs?** Decided 2026-09-29: no. A class with
    slots takes V5 only, and the text-only use registers the text-only class (the same weights,
    another pipeline), so each behaviour keeps one encoding. Extended 2026-09-29 (§II.2.2): a class
    that reads a source takes V5 only as well.
15. **The price of a V5 job's source tokens** (§II.2.2) [RFC-0001]. Recommendation, **pending user
    confirmation** alongside 13: source tokens are priced as prompt tokens, at the job's per-token
    price, never below the class's `source_token_floor` (at or above `⌈admitted source work /
    per-token work⌉`: a padded encoder does its whole width for any source).
16. **Weight for tensor claims** (§I.4.6). Recommendation, decided as built 2026-10-01: none in the first
    release — a weightless claim, as RFC-0004's evaluation claim — and a later fence prices them by the lane's
    quanta over leaves once a drilled family certificate covers a V2 pipeline. Confirm that the first
    release carries no reward for executing a tensor job.
17. **The per-class cap on claims in flight** (§I.4.8). The drill uses 16 for every profile; the network's
    value is provisional until the first profile's corpus is measured.
18. **Sampled seat verification for long classes** (§I.4.10). A seat replays a whole tensor job. For a class
    whose replay is long, the lane's interval sampling (ADR-0077 Decision 8) over the one step tree is the
    optimisation: when, and with which sample counts.
19. **Which fence opens the tensor claim** (§I.4.9). Decided as the lane table implies: `palw_fp_job_v5` over
    `palw_gen_v1`. The fallback is a dedicated `palw_gen_job_v1` if the user wants tensor jobs open while V5
    must stay shut by a fence value rather than by a release.

#### Activation plan and order of implementation

The prerequisite is RFC-0002 Phase F: `palw_tir_v1` armed, with admission v10, the step space,
`TirCone` and H dissection. The new fence `palw_gen_v1` is Phase F D1's shape:

```
Option<PalwGenFenceV1 { activation, program_version: 2, court_version: 2, rand_set_id, output_set_id, ceilings }>
```

It is Some-only in both fingerprints, collapses to `never()`, and sits at an unused height.
Both ids are keyed digests of an ASCII descriptor line, so a fence value is a consensus number any
second implementation can compute from this text (finding G5):

```
rand_set_id   = H64(key "misaka-palw/rand-set-id/v1", rand_set_descriptor)
rand_set_descriptor =
  "palw-rand/v1/domains=" + join(",", [ "<id>:<NAME>:<key>:b<word bits>:<layout>:<step>"  for each §I.1.4 row, by id ])
  + "/gumbel-q24-v1=<hex>/gauss-q24-v1=<hex>"
  // <layout> ∈ {text, blocked}; <step> ∈ {none, zero, per-step, declared}; the two <hex> are the tables' BLAKE2b-512 pins

output_set_id = H64(key "misaka-palw/output-set-id/v1", output_set_descriptor)
output_set_descriptor =
  "palw-output/v1/kinds=" + join(",", [ "<tag>:<Name>:<element>:<shape>:<meta>" for each §I.3.3 kind, by tag ])
  + "/tile_len=[4,65536]/leaf=<key>(le32(t)|bytes)/node=<key>(left|right)/odd=promoted"
  + "/root=<key>(borsh(spec)|le32(tile_len)|merkle)"
```

The descriptors and both ids are pinned by `consensus-vectors/rand-v1/domains.json` and
`consensus-vectors/output-v1/kinds.json` (the table of kinds there is the grammar's example).

| Order | Work | Consensus change | Exit gate | Estimate |
| --- | --- | --- | --- | --- |
| 1 | **R**: `palw_rand_v1`, the domain table, the Gaussian table generator and pin, vectors `consensus-vectors/rand-v1/` (domain 0 cross-checked against `fp-v4/`) | none by itself | two implementations agree; D11 vectors unchanged | 1–2 weeks |
| 2 | **Canonical outputs**: encoders, `output_root`, vectors `consensus-vectors/output-v1/` | none by itself | vectors; round trip to reproducible PNG/WAV | 1 week |
| 3 | **PALW-TIR program version 2** in tir/core: inputs, output kinds, `post` effects, range leaves, the `input` question in §9.4, replay; golden vectors; the second implementation | none (crate) | reference = second implementation on V2 vectors | 3–4 weeks |
| 4 | **Pipeline class**: `TirPipelineV1`, edges, one step tree, class id, admission (per-stage v10, edges, offers, output spec, per-profile ceilings), carriage | dormant fence | first-divergent-leaf property over pipelines; mutation corpus refused by name | 3–4 weeks |
| 5 | **Job and court**: `PalwGenJobV1`, acceptance refusals, random inputs in the court, `External` edges, `TirOutputDigestMismatch`, the canonical-work rule | dormant fence | court battery on tiny pipelines: planted faults at every commit-point kind, forged noise, forged edges, forged digests, out-of-interval values | 3–4 weeks |
| 5b | **The claim's carriage** (§I.4): FP job version 10 on the free-prompt lane under `palw_fp_job_v5` — the doors, the walk, the fold's weightless claim (exact work, reservation, per-class cap), the node's worker, seat and court loops, and the executor's submit path | dormant fence | processor tests through the real extraction; every nothing-below-the-fence gate; a seat's replay and a court over a tensor claim on real nodes (step 8) | 3 weeks |
| 6 | **Image profile**: lowerers for one transformer family, its text encoder and its VAE (reduced fixtures first); committed-partial patterns; the fidelity harness with same-noise float reference; the §II.1 checklist completed | none | freeze criteria in the RFC-0002 manner: three-way bit identity, fidelity thresholds, every cone within ceilings | 6–8 weeks (overlaps 3–5) |
| 7 | **Embedding profile**, as the smoke-test class | none | same gates, trivial cost | 1–2 weeks |
| 8 | **Drills**: devnet, then a salted t12 chain; register tiny image and embedding classes; jobs; replay; disputes (a partial-sum cone included); crossing the fence on the shipping binary | — | RFC-0002 Phase F's D-F1…D-F4 pattern | 3 weeks |
| 9 | **testnet-12**: arm `palw_gen_v1` after `palw_tir_v1`; the first Example-C-sized image class | fence | registration → panel → `Final` | — |
| 10 | **RFC-0002 Phase G**: GPU integer backend, exact two-pass attention, GPU leaf hashing; then a 4B-class distilled image class | node releases | F-4 gates per backend; the throughput report | — |
| 11 | **Multimodal input** (§II.4), then image-editing bodies. Built on `rfc3/impl` (2026-09-29), dormant: `JobImage`, the slots, the image leaf, the court's tile reading. Then the **VLM path** (§II.2.1): the text stage in the IR and the `Text` profile, dormant; FP Job V5 in RFC-0001's lane after its owner agrees | fence value; **[RFC-0001]** for V5 | — | — |
| 12 | **Audio** (§II.5) | fence value | — | — |
| 13 | **Generalised dissection** (§II.1.5.6, court version 3), then **video** (§II.6), subject to open question 6 | fence | — | — |

**Which lanes each fence opens.** RFC-0002 Phase F's decision 12 keeps the free-prompt lane closed
to IR classes: testnet-12's DAA 2,000 (`palw_tir_v1`) opens registration, the attempt lane, panels
and the court for IR classes, and nothing of the free-prompt lane. `palw_gen_v1` opens registration
(the pipeline admission), panels and the court for generative classes; a pipeline has no attempt
lane (it registers at 0‰). **Opening the free-prompt lane to IR and pipeline classes is its own later fence.** For a pipeline class
that lane is `palw_fp_job_v5` over `palw_gen_v1` (§I.4.9). Past it the walk admits a **tensor job** — FP job
version 10, a weightless claim (§I.4) — and, in the release that also confirms open question 13's price, a V5
commitment (§II.2.1). Until then the walk skips every version-8 payload, whatever the height.

**Order of implementation, in one line:** R and canonical outputs, then TIR V2, then pipelines and
the court, then the image profile (with embedding as the smoke test), then drills and testnet, then
Phase G and the 4B class, then multimodal input, audio, and dissection with video. **Text stays on
RFC-0001's train throughout.** A TIR text class follows Phase F and depends on nothing here beyond
naming R's domain 0. Steps 1–8 take about 4–5 calendar months with two or three people, after Phase F.

#### Decision

**2026-09-28 (user): open questions 1–12 are decided as recommended.**

| # | Question | Decision |
| --- | --- | --- |
| 1 | R's key | the job's **seed** (§I.1.3), not the job id |
| 2 | Gaussian noise | the protocol-wide `Normal` input kind over the `2^16`-entry table `PALW_GAUSS_Q24_V1` (§I.1.5) |
| 3 | resolutions | **one resolution per class** in v1 |
| 4 | the decoder | the VAE decoder is **on chain from the start**; the first classes are **≤ 512²** |
| 5 | ceilings | **per-profile** ceilings (per-job MACs and leaves, the per-position leaf cap, peak memory) |
| 6 | commitment volume | **testnet accepts it**; packed lanes, a faster leaf hash and lazy commitment are separate later work |
| 7 | canonical work | activation-by-activation `MatMul`s (image attention) count as **`attention_prefill`** |
| 8 | registration carriage | **multi-carrier** pipeline registration comes with `palw_gen_v1` |
| 9 | first image class | a **small** class, chosen after the model-card checklist (§II.1) |
| 10 | generalised dissection | **with video** (§II.6), not with the first image fence |
| 11 | multi-codebook audio | **later** (a later FP job version) |
| 12 | privacy | image jobs **default to `PanelDa`**; `PublicDa` stays available when the user chooses it |
| 13 | V5 image price | **pending user confirmation** — recommendation: prefill-equivalent tokens per slot, floored at `⌈per-image work / per-token work⌉`, at the job's per-token price |
| 14 | V4 jobs on a class with image slots | **no** (2026-09-29): a class with slots takes V5 only; likewise a class that reads a source (§II.2.2) |
| 15 | V5 source-token price | **pending user confirmation** — recommendation: priced as prompt tokens, at the job's per-token price, never below the class's floor `⌈source work / per-token work⌉` |

**2026-10-01 (coordinator): a generative class must be able to carry a claim.** The carriage of a tensor job is decided as §I.4 states it:

| # | Question | Decision |
| --- | --- | --- |
| 16 | tensor claims' weight | **none in the first release** — a weightless claim (FP job version 10); the unit is the exact step leaves, the weight a later fence (§I.4.6) |
| 17 | class capacity | a per-class cap on claims in flight in the fence's per-profile ceilings (§I.4.8); the drill's value 16, the network's provisional |
| 18 | seat verification | the seat **replays the whole job**; interval sampling for long classes is later (§I.4.10) |
| 19 | the fence | **`palw_fp_job_v5` over `palw_gen_v1`** — the lane table's pipeline-class fence; a dedicated fence is the one-function fallback (§I.4.9) |
| 20 | a tensor claim's court under the held regime | **`GenShardCourtAccused`** (object tag 88, **approved 2026-10-01**): tag 62's twin for pipelines (§I.4.7, finding G23). The RFC was silent and a claim with no court is unsafe to open; additive and dormant |
| 21 | class gating of a tensor claim | the class's **status** (`Active`), **readiness** (a panel's worth of distinct operators, never the executor's, hold the class with a fresh possession proof — decided 2026-10-01; one shared function, `class_seating_v1`, which lane F's `palw_class_seating` extends with an independence floor for every class kind), the fence's cap on claims in flight and the per-bond share `⌈cap / 2⌉` — **not the registry lifecycle**, whose row for a pipeline never admits (§I.4.5, §I.4.8) |
| 22 | closes larger than one carrier under the held regime | **approved 2026-10-01, generic (IR and pipelines) (G24, §I.4.7)**: the held leaf challenge — object tag 90, fence `palw_held_close_chunks_v1` — opens a `Terminal` session at the named leaf and rides the court's own close group (keyed to claim and accuser by construction) up to the carried cap; the accuser bears the clock and the deposit, an abandoned close forfeits the deposit and leaves the claim unaffected, challenges are capped per claim, network and block; tag 62's live semantics are unchanged, PALW-GEN-21 refuses a pipeline class whose close exceeds one carrier until the fence is armed, and the live IR classes' limitation is recorded |

The rest of the RFC (Parts I and II, the program surface, the activation order) stands as written.

#### Mission alignment amendment — 2026-10-07

モデル種別、image/audio/video、MoE、recurrence、prompt/state/outputごとに、公開入力から局所化・terminal conviction・非開示裁定まで完結するprofileを必要とする。正直な実行のFinal、再現性や決定論的R、class-ready seatsだけでこのgateを代替しない。producer内部のheld captureを使う成功例や小さいfixtureを、大規模profileの公開訴追成功に外挿しない。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)、[RFC15](../rfc/0015-panel-free-permissionless-verification.md)を参照する。

#### Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

異なる生成モデル、context、token数でもproducer資本・期間に基づく総採掘上限を増やさない。claim容量の細分化は経済的権利の配分であり、定義済みの一job/一実行や同一workの二重報酬禁止を解除しない。
各profileの真の実行仕様・全constraint検証と取得・裁定gateを満たし、既存class予算は追加経済枠でなく別のadmission制限として扱う。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。

#### 改定 — 2026-10-12（challenge windowの再付与禁止、公開検証不能な経路の報酬除外：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §5と[RFC/ADRレビューの整理（2026-10-12）](../audit/palw/design-reviews/rfc-adr-review-resolution-2026-10-12.md)。

- **§I.4.7 / Decision 22の将来規則を置換:** challengeの放棄、失敗、close、Panel交代により、claim全体のchallenge windowを最初から付与し直してはならない。licence anchorを記録上更新する場合でも、challenge開始cutoff、absolute hard deadline、producer枠の回復時計は変更しない（RFC-0014 §7.2の2026-10-12改定と統一。旧claimは旧ruleset）。
- **N4/X2:** 画像bytesやsource IDsがPanelにだけ渡されるなど、公開訴追に必要な証拠を確保できない生成物・画像入力・source経路は、新規の報酬・weight対象から外す。通常の非報酬AI推論ではプライバシーを維持して利用できる。既存の実装記録は削除せず、将来の有効化対象を限定する。過去の有効claimは旧rulesetで処理し、遡及変更しない。
- **W2:** FP V4自体のtext規則は変更しない。TextStream pipelineには追加規則を適用する。
- **readiness:** 単一seatで全検証scopeを覆える経路だけをPanel=1暫定modeの対象にする（RFC-0010 2026-10-12改定）。
