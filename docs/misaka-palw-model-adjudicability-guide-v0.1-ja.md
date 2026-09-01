# MISAKA PALW モデル裁定可能化ガイド v0.1

Status: **v0.1 (2026-09-01)** — [ADR-0069](adr/0069-e2e-adjudicability-is-the-price-of-weight.md)
の実務手順書。対象は「新しいモデルを PALW の **weight を持つ LLM class**(cadence を稼ぐクラス)
として登録したい人」。関連: ADR-0039(catalog が閉じるまで weightless)、ADR-0049(裁定契約)、
ADR-0067(class は chain data、kernel は build)、ADR-0054/0056(share は生産に従う / permissionless
登録)。

---

## 0. 要旨 — なぜ「裁定可能化」が weight の前提なのか

PALW の唯一の主張は「ブロックの対価は**実際の LLM 推論**である」こと。この主張は、推論をやって
いない producer を**有罪判定できる**能力と同じ強さしか持たない。

登録(permissionless)と weight(cadence share)は別物だ:

- **誰でも**モデルを登録できる。ただし登録しただけのクラスは *liveness-admissible だが weightless*
  ——ブロックは作れるが fork-choice weight を持たず、cadence を稼がない(ADR-0039)。
- **weight を持つ**には、そのクラスが end-to-end で裁定可能——実 backend が実アンカーに対して
  争議を有罪判定まで最後まで回せること——を **certification** で証明する必要がある(ADR-0069)。

このガイドは、あなたのモデルを weightless から weight-bearing へ引き上げるための手順書だ。
唯一の完成参照は **BASE-0**(`misaka-palw-base0/src/backend.rs`)。以下、随所でその file:line を
写経対象として指す。

---

## 1. 満たすべき二つの adjudicability

| | 何を証明するか | どこで検査されるか | 誰の性質か |
|---|---|---|---|
| **静的 (static)** | profile が歩ける step space で、到達する全 kernel を adjudicator が再実行でき、全 node shape が servable | `check_lineage_v1`(SDK)+ `verify_catalog_coverage_v1` / `verify_profile_coverage_v1` → `court_catalog_root` | **class**(chain data) |
| **E2E** | 実 backend が実アンカーで争議を**有罪判定まで**回せる | Decision 3 の drill → `court_e2e_root`(ADR-0069 で新設) | **build**(kernel + court) |

登録と liveness に要るのは静的側だけ。**weight には両方**が要る。

静的側は既に整備・偽造不能(証明書は sealed constructor 経由でしか作れない)。**あなたが新規に
埋めるのは E2E 側**だ。以下はそのための道のり。

---

## 2. backend seam の地図 — `PalwExecutionBackendV1`

`consensus/core/src/palw_backend.rs`。メソッドは3群に分かれる。

**生産に必須(実装しないとそもそもクラスが動かない)**
- `model_id()` — クラスの文字列 id
- `job_for_anchor(anchor)` — アンカーが含意する canonical job と prompt(`:125`)。**producer が入力を
  選べてはならない** ——prompt はアンカーと vocab から導出する(BASE-0 は `base0_rc_job_v1`、A16 は
  `qwen25_a16_prompt_for_anchor`)。ここが自由だと「モデルを走らせる」と「都合のいい出力になる入力を
  探す」が同じ操作になる。
- `execute(job, prompt)` — 実推論。material と4つの root を返す(`:129`)
- `verify_material(material, claim)` — seat が署名前に回す自己整合チェック(`:166`)。**有罪判定では
  ない**——不一致は court の仕事で、seat は merits に署名しないだけ

**court に必須(これが無いと weight を持てない)**
- `bisect_prefix_state(material, index)` — ラダー各段でのそのパーティの prefix commitment
  (`:176`、既定 `None`)。**prefix 性が要**——index まで一致する2実行はここで一致し、その前で違う
  2実行はここで違う。これが「最初に食い違う index」=「最初に leaf が違う位置」を成立させる
- `refutation_for_index(material, index)` — 終端手の証拠。**原告・被告の両方が同じ呼び出しで作る**
  (`:205`、既定 `Err`)
- `supports_court()` — このクラスが court の手番を取れるか(`:193`、既定 `false`)。上2つを実装して
  初めて `true` にできる

**drill/補助**
- `execute_with_injected_fault(job, prompt, leaf)` — 既知の leaf に故障を注入した guilty material を
  作る(`:276`)。**これが certification vector の生成器**
- `operand_openings_for(...)`(`:257`)、`job_anchor_v1(...)`(`:230`)

QWEN36 / QWEN25-A16 は現在、**court に必須の3つが既定のまま**(`None`/`Err`/`false`)。だから
weight を持ってはならない、とコード自身が書いている(`misaka-palw-base0/src/qwen36_backend.rs:13-18`)。

---

## 3. 手順 — weightless から certified へ

### ステップ 1: グラフを宣言し、エンジンと一致させる(ADR-0049 Decision F)

profile(`PalwShapeProfileV3`)に、エンジンが実行する**すべての narrowing** を宣言する。宣言と
実行が食い違うと、その食い違った step は永久に裁定不能になる。

- 参照: BASE-0 は `base0_check_graph_v1` でエンジンの op 列と宣言グラフの一致を強制する。
- **A16 の残件**: `A16Engine` に `plan()` が無く、`base0_check_graph_v1` 相当も無い。しかも `pre`
  テーブルで、宣言していない **requant**(embedding を A16 stream に載せる narrowing)を実行して
  いる(`legs.rs:113-119`)。A16 を certify する前に、この requant をグラフに宣言するか除去し、
  `plan()` と graph checker を書く必要がある。
- 落とし穴: 「走った」は「宣言どおり計算した」ではない。graph checker が無いと、この不一致は
  build 時ではなく**有罪判定の瞬間**に露見する。

### ステップ 2: step space を数えられるようにする

`canonical_step_coordinates`(`palw_step.rs`)、`step_leaf_count`、tile leaves が、あなたの profile
に対して有限に列挙・計数できること。ここまで来たら **SDK の静的バッテリを通す**:

```
check_lineage_v1(&your_lineage, &court)   // misaka-palw-sdk::conformance
```

これが緑になると、profile validate / 参照が厳密に後方 / 全 kernel catalogued / 全 node shape
servable / canonical job が worst case と n_ctx の内側 / court cost 導出可、が一括で保証される
(`misaka-palw-sdk/src/conformance.rs:29`)。**新クラスの最初のテストはこれにする。**

### ステップ 3: court の手番を実装する

- `bisect_prefix_state` — material から prefix state を計算して返す。BASE-0 は
  `base0_bisect_prefix_state_v1(&binding.job_context, &leaves, index)`(`backend.rs:239-242`)。
  **leaves を material から復元できることが前提**(→ ステップ4)。
- `refutation_for_index` — 終端の証拠を組む。BASE-0 は
  `base0_refutation_from_capture_v1(...)`(`backend.rs:276` 以降)。原告・被告で同一の呼び出し。
- 両方が実装できたら `supports_court()` を `true` に。

### ステップ 4(最重要の落とし穴): material が争議に必要なものを運ぶ

**ここが QWEN36 と A16 が今つまずいている場所だ。** 両者の retained material は logits と生成
トークンしか運んでいない:

- `qwen36_material_encode_v1`(`qwen36_backend.rs:269`)= `logits_rows` + `generated` のみ。
- `qwen25_a16_material_encode_v1`(`qwen25_a16_backend.rs:97`)= 同上。

logits だけからは prefix state を再構成できない。対して BASE-0 の material は
`(binding, tiles, logits_rows, generated, _)` を運ぶ(`base0_material_decode_v1`)ので、tiles から
`leaves_by_position` → prefix state を作れる。

二つの道のどちらかを取る:

1. **material に per-step tile と binding を載せる** — 単純だが、carriage の close 天井を超えては
   ならない。**flat には載らない**: Qwen 級 vocab は logits 1行 ≈ 993 KiB、close budget は 80 KiB。
   必ず **tiled** にする。
2. **checkpoint leg で dispute 時に再捕捉する** — material は軽いまま、争議に入ったら
   `Base0CheckpointCaptureV1::push_chunks` を `next_geometry` に対して回して leaves を作り直す
   (`legs.rs` の checkpoint leg。A16 の cache は `engine::KvCache` ではないのでこの経路)。

A16 用の橋渡しの半分は既にある: `a16_captured_rows_v1(&A16TraceV1) -> Vec<Base0CapturedRowV1>`
(`legs.rs:131`)が A16 trace を BASE-0 形の行へ変換する。残るのは checkpoint leg と上記 rung
2メソッド、そして material 形式の決定。

### ステップ 5: E2E drill を回す(= certification vector を作る)

covering leaf set `L` に対して:

1. `execute` → honest material。
2. 各 `ℓ ∈ L` で `execute_with_injected_fault(job, prompt, ℓ)` → guilty material。
3. honest / guilty の両方で `refutation_for_index(material, ℓ)` が `Ok`、`bisect_prefix_state` が
   真の prefix(`ℓ` で初めて食い違う: `i ≤ ℓ` で一致、`i = ℓ+1` で不一致)。
4. **実際の court** を回す: `adjudicate_court_close_v2` → `check_step_refutation_v1` が guilty を
   その leaf で有罪、honest を無罪にする。court を再実装しない——出荷される adjudicator を駆動する。
5. 4 が読むものがすべて手に入る(ステップ4を満たしていれば自動的に真)。

参照: BASE-0 はこの往復を既にテストで持っている(`backend.rs:503-730` の honest/guilty
`refutation_for_index` と before/after `bisect_prefix_state`、`:730` の `execute_with_injected_fault`)。
**このテストの通過ベクタが、そのままクラスの回帰テスト兼 certification 証拠になる。**

`L` は **covering** であること: 宣言した全テーブル(`pre`/`gdn`/`attn`/`post`)に少なくとも1 leaf、
prefill と decode の両方に少なくとも1 position を含む。`L` が漏らした leaf は、有罪判定されずに
食い違える step なので、狭い `L` は弱い保証になる。

### ステップ 6: 提出して weight を得る

- build の **certified family 集合**(ADR-0069 の `court_e2e_root`)に、あなたの family descriptor を
  加える。証明書は drill を有罪判定まで通した時にのみ sealed constructor から作れる(coverage の
  `PalwReachableKernelSetV1` と同じ偽造不能パターン)。
- これで `verify_class_admission_v2` / `granted_share_table_v2` が、あなたの family
  (`PalwRegistrationTermsV2.family`)に weight-bearing な share を許すようになる。
- weight を持たせる有効化は **activation**——`court_e2e_root` が `court_catalog_root` と同じ形で
  動く——であって re-genesis ではない(方針)。

---

## 4. 監査由来の落とし穴チェックリスト

certification の前後で、これらを自分のコードに対して確認する(2026-09-01 監査で実際に見つかった形):

- [ ] **material が logits だけになっていないか。** なっていれば prefix state を作れず court に入れ
  ない(§3 ステップ4)。
- [ ] **close が flat に組まれていないか。** 80 KiB を超えると carriage に載らない。tiled にする。
- [ ] **集約値だけを検査していないか。** `exps.iter().max()` のように最大値だけ見て要素ごとの値域を
  見ないと、負の指数1バイトで `partial << -1` に到達し全ノードが panic する(監査 F15、
  `palw_qwen36_ops.rs:448-463`)。kernel の入口で**全要素**を検査する。
- [ ] **validate 前に geometry を演算していないか。** 攻撃者由来の `shape_profile` を
  `validate_shape()` の前に座標計算へ渡すと court close で全ノードが落ちる(監査 F25、
  `palw_court_v2.rs:448`)。演算の前に必ず shape を検証する。
- [ ] **`supports_court()` を実体より先に `true` にしていないか。** rung 2メソッドが既定のままなら
  `false` のままにする。嘘の `true` は「有罪判定できるクラス」を騙ることになる。

---

## 5. CI とチェックリスト(緑にすべきもの)

1. `check_lineage_v1(&lineage, &court)` — 静的バッテリ(§3 ステップ2)。
2. `check_sdk_v1(&sdk)` — 台帳全体で class id が衝突しないこと(`conformance.rs:96`)。
3. E2E drill テスト(§3 ステップ5)。BASE-0 の `backend.rs:503-730` を雛形に、covering `L` で
   honest 無罪 / guilty 有罪を assert。**difference で書く**: full `L` で通るクラスが、テーブルを
   1つ落とした `L` では落ちること。
4. graph check(§3 ステップ1)。エンジンの op 列 = 宣言グラフ。

---

## 6. 写経の順番(BASE-0 を最小完成例として読む)

1. `misaka-palw-base0/src/backend.rs:189` `execute_with_injected_fault` — drill の入口
2. 同 `:239` `bisect_prefix_state` — prefix commitment の最小形
3. 同 `:245-` `refutation_for_index` — 終端証拠の組み方
4. 同 `:503-730` テスト群 — E2E 往復の完成形(= certification vector)
5. `misaka-palw-base0/src/legs.rs` — `base0_bisect_prefix_state_v1` / `Base0StepCaptureV1` /
   checkpoint leg / `a16_captured_rows_v1`(A16 橋渡しの半分)
6. `misaka-palw-sdk/src/conformance.rs:29` `check_lineage_v1` — 静的バッテリの中身

BASE-0 が court を最後まで回せる唯一のクラスである理由は、この6ファイルに全部書いてある。
新しいモデルを weight-bearing にするとは、この6つを自分の family について再現することだ。

---

## 付録: 用語

- **family** — backend の実行系統(BASE-0 / QWEN36 mmap / QWEN25-A16 dense 等)。certification は
  family 単位。登録タームの `PalwRegistrationTermsV2.family` がチェーン上の判定材料。
- **weightless / weight-bearing** — cadence share が 0 か非0か。ADR-0069 の gate は weight にだけ
  かかり、登録・liveness にはかからない。
- **covering leaf set** — drill が故障を注入する leaf の集合。全テーブル + prefill/decode を覆う。
- **`court_catalog_root` / `court_e2e_root`** — build の裁定能力(前者=kernel 単位、後者=E2E)を
  1個の hash として ruleset root にコミットしたもの。2ノードが同一 build から同一値を計算する。
