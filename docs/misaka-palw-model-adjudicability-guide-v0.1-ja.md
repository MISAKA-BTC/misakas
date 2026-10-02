# MISAKA PALW モデル裁定可能化ガイド v0.1

Status: **v0.1 (2026-09-01)** — [ADR-0069](adr/0069-e2e-adjudicability-is-the-price-of-weight.md)
の実務手順書。対象は「新しいモデルを PALW の **weight を持つ LLM class**(cadence を稼ぐクラス)
として登録したい人」。関連: ADR-0039(catalog が閉じるまで weightless)、ADR-0049(裁定契約)、
ADR-0067(class は chain data、kernel は build)、ADR-0054/0056(share は生産に従う / permissionless
登録)。

> **改訂注(2026-10)**: 本書は 2026-09-01 時点の手順書。その後 [ADR-0075](adr/0075-certification-is-a-consensus-object.md)
> で certified family 集合は build 固定ではなく **genesis ∪ チェーン状態**(`FamilyCertified` /
> `ClassLaneCertified` オブジェクト)になり、QWEN36 / QWEN25-A16 も court 手番を実装済み。本書の
> 対象は **新しい family(新アーキテクチャ)の backend を build に実装する開発者**に絞る。既存
> family がカバーするモデルの追加・認証は [palw-add-a-model-runbook.md](palw-add-a-model-runbook.md) /
> [palw-certify-a-new-model.md](palw-certify-a-new-model.md) / [wiki/Adding-a-Model-JA.md](wiki/Adding-a-Model-JA.md)、
> SDK 側(lineage / checkpoint の追加)は [palw-model-onboarding-sdk.md](palw-model-onboarding-sdk.md) を参照。

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
| **E2E** | 実 backend が実アンカーで争議を**有罪判定まで**回せる | Decision 3 の drill → certified family 集合(ADR-0069 の `court_e2e_root`、ADR-0075 以降は genesis ∪ チェーン状態) | **build**(kernel + court) |

登録と liveness に要るのは静的側だけ。**weight には両方**が要る。

静的側は既に整備・偽造不能(証明書は sealed constructor 経由でしか作れない)。**あなたが新規に
埋めるのは E2E 側**だ。以下はそのための道のり。

---

## 2. backend seam の地図 — `PalwExecutionBackendV1`

`consensus/core/src/palw_backend.rs`。メソッドは3群に分かれる。

**生産に必須(実装しないとそもそもクラスが動かない)**
- `model_id()` — クラスの文字列 id(`:242`)
- `job_for_anchor(anchor)` — アンカーが含意する canonical job と prompt(`:248`)。**producer が入力を
  選べてはならない** ——prompt はアンカーと vocab から導出する(BASE-0 は `base0_rc_job_v1`、A16 は
  `qwen25_a16_prompt_for_anchor`)。ここが自由だと「モデルを走らせる」と「都合のいい出力になる入力を
  探す」が同じ操作になる。
- `execute(job, prompt)` — 実推論。material と4つの root を返す(`:277`)
- `verify_material(material, claim)` — seat が署名前に回す自己整合チェック(`:394`)。**有罪判定では
  ない**——不一致は court の仕事で、seat は merits に署名しないだけ

**court に必須(これが無いと weight を持てない)**
- `bisect_prefix_state(material, index)` — ラダー各段でのそのパーティの prefix commitment
  (`:425`、既定 `None`)。**prefix 性が要**——index まで一致する2実行はここで一致し、その前で違う
  2実行はここで違う。これが「最初に食い違う index」=「最初に leaf が違う位置」を成立させる
- `refutation_for_index(material, index)` — 終端手の証拠。**原告・被告の両方が同じ呼び出しで作る**
  (`:607`、既定 `Err`)
- `supports_court()` — このクラスが court の手番を取れるか(`:442`、既定 `false`)。上2つを実装して
  初めて `true` にできる

**drill/補助**
- `execute_with_injected_fault(job, prompt, leaf)` — 既知の leaf に故障を注入した guilty material を
  作る(`:1230`、既定 `Err`)。**これが certification vector の生成器**
- `operand_openings_for(...)`(`:710`)、`job_anchor_v1(...)`(`:682`)

2026-09-01 時点では QWEN36 / QWEN25-A16 の court 3メソッドが既定のままだったが、現在は両者とも
実装済み。ただし `supports_court()` は条件付きで、登録グラフと plan を持つ backend だけが `true`
を返す(`misaka-palw-base0/src/qwen36_backend.rs:1922`、`qwen25_a16_backend.rs:2713`)。持たない
backend は trait の既定(`None`/`Err`)のまま、と明記されている(`qwen36_backend.rs:9-16`)。

---

## 3. 手順 — weightless から certified へ

### ステップ 1: グラフを宣言し、エンジンと一致させる(ADR-0049 Decision F)

profile(`PalwShapeProfileV3`)に、エンジンが実行する**すべての narrowing** を宣言する。宣言と
実行が食い違うと、その食い違った step は永久に裁定不能になる。

- 参照: BASE-0 は `base0_check_graph_v1`(`misaka-palw-base0/src/plan.rs:802`)でエンジンの op 列と
  宣言グラフの一致を強制する。dense / hybrid は profile から plan を組む
  (`A16Engine::plan_from_profile`、`engine_a16.rs:1772` / `qwen36_plan.rs:293`)。
- 落とし穴: 「走った」は「宣言どおり計算した」ではない。graph checker が無いと、この不一致は
  build 時ではなく**有罪判定の瞬間**に露見する。

### ステップ 2: step space を数えられるようにする

`canonical_step_coordinates`(`consensus/core/src/palw_step.rs:1659`)、`step_leaf_count`、tile leaves が、あなたの profile
に対して有限に列挙・計数できること。ここまで来たら **SDK の静的バッテリを通す**:

```
check_lineage_v1(&your_lineage, &court)   // misaka-palw-sdk::conformance
```

これが緑になると、profile validate / 参照が厳密に後方 / 全 kernel catalogued / 全 node shape
servable / canonical job が worst case と n_ctx の内側 / court cost 導出可、が一括で保証される
(`misaka-palw-sdk/src/conformance.rs:29`)。**新クラスの最初のテストはこれにする。** SDK 側の作業は
[palw-model-onboarding-sdk.md](palw-model-onboarding-sdk.md) を参照。

### ステップ 3: court の手番を実装する

- `bisect_prefix_state` — material から prefix state を計算して返す。BASE-0 は
  `base0_bisect_prefix_state_v1(&binding.job_context, &leaves, index)`(`backend.rs:701-711`)。
  **leaves を material から復元できることが前提**(→ ステップ4)。
- `refutation_for_index` — 終端の証拠を組む。BASE-0 は
  `refutation_for_index`(`backend.rs:713`)→ `refutation_with_prompt` →
  `legs::base0_refutation_from_capture_capped_v1`(`backend.rs:202`)。原告・被告で同一の呼び出し。
- 両方が実装できたら `supports_court()` を `true` に。

### ステップ 4(最重要の落とし穴): material が争議に必要なものを運ぶ

**2026-09-01 時点で QWEN36 と A16 がつまずいていた場所。** 当時の retained material は logits と
生成トークンしか運んでいなかった(`qwen36_material_encode_v1` / `qwen25_a16_material_encode_v1`
はこの形のまま残っている: `qwen36_backend.rs:936`、`qwen25_a16_backend.rs:769`)。両者の court
経路は現在 `base0_material_decode_any_v1` で binding を復元する。

logits だけからは prefix state を再構成できない。BASE-0 の material は
`(binding, tiles, logits_rows, generated, _)` を運ぶ(`base0_material_decode_v1`、
`produce.rs:474`)ので、tiles から leaves → prefix state を作れる。

二つの道のどちらかを取る:

1. **material に per-step tile と binding を載せる** — 単純だが、carriage の close 天井を超えては
   ならない。**flat には載らない**: vocab 248,320 で flat 1行 ≈ 993 KB、carrier は約 80 KiB
   (`consensus/core/src/palw_qwen36_profile.rs:1444-1445`。A16 は 607,744 B 対 81,920 B、
   `qwen25_a16_backend.rs:14-16`)。必ず **tiled** にする。
2. **checkpoint leg で dispute 時に再捕捉する** — material は軽いまま、争議に入ったら
   `Base0CheckpointCaptureV1::push_chunks`(`legs.rs:795` / `:1248`)を `next_geometry` に対して
   回して leaves を作り直す。

BASE-0 以外の engine trace を BASE-0 形の行へ変換する例: `a16_captured_rows_v1`(`legs.rs:138`)。

### ステップ 5: E2E drill を回す(= certification vector を作る)

covering leaf set `L` に対して:

1. `execute` → honest material。
2. 各 `ℓ ∈ L` で `execute_with_injected_fault(job, prompt, ℓ)` → guilty material。
3. honest / guilty の両方で `refutation_for_index(material, ℓ)` が `Ok`、`bisect_prefix_state` が
   真の prefix(`ℓ` で初めて食い違う: `i ≤ ℓ` で一致、`i = ℓ+1` で不一致)。
4. **実際の court** を回す: `adjudicate_court_close_v2` → `check_step_refutation_v1` が guilty を
   その leaf で有罪、honest を無罪にする。court を再実装しない——出荷される adjudicator を駆動する。
5. 4 が読むものがすべて手に入る(ステップ4を満たしていれば自動的に真)。

参照: BASE-0 の drill テスト(`misaka-palw-base0/src/backend.rs` の test module、例:
`the_drill_fault_is_self_consistent_and_only_a_re_execution_finds_it`、`:1701`)。
**このテストの通過ベクタが、そのままクラスの回帰テスト兼 certification 証拠になる。**

`L` は **covering** であること: 宣言した全テーブル(`pre`/`gdn`/`attn`/`post`)に少なくとも1 leaf、
prefill と decode の両方に少なくとも1 position を含む。`L` が漏らした leaf は、有罪判定されずに
食い違える step なので、狭い `L` は弱い保証になる。

### ステップ 6: 提出して weight を得る

ADR-0075 以降、build の集合に descriptor を加える(ADR-0069 当初の方式)のではなく、drill を
**チェーン上のオブジェクト**として提出する(`FamilyCertified` → `ClassLaneCertified`)。手順・
コマンド(`palw-certify drill` / `bind`、`misaka palw submit-object`)とチェーンの検査内容は
[palw-certify-a-new-model.md](palw-certify-a-new-model.md) を参照。

新 family の場合の前提: `palw-certify drill --family` がその family を drill できる build であること
(現在の対象は同ドキュメント参照)。family descriptor は `PalwE2eFamilyV1`
(`consensus/core/src/palw_e2e_adjudicability.rs:174`)で、class は自分の到達 kernel 集合が
family の `kernel_ids` に含まれるとき weight を持てる。チェーン上の family は
`PalwRegistrationTermsV2.chain_certified_families`(`palw_state_v2.rs:6128`)として
admission / share 判定に渡る。

---

## 4. 監査由来の落とし穴チェックリスト

certification の前後で、これらを自分のコードに対して確認する(2026-09-01 監査で実際に見つかった形):

- [ ] **material が logits だけになっていないか。** なっていれば prefix state を作れず court に入れ
  ない(§3 ステップ4)。
- [ ] **close が flat に組まれていないか。** 80 KiB を超えると carriage に載らない。tiled にする。
- [ ] **集約値だけを検査していないか。** `exps.iter().max()` のように最大値だけ見て要素ごとの値域を
  見ないと、負の指数1バイトで `partial << -1` に到達し全ノードが panic する(監査 F15。修正後の検査は
  `q36_check_group_exponents_v1`、`consensus/core/src/palw_qwen36_ops.rs:467`)。kernel の入口で**全要素**を検査する。
- [ ] **validate 前に geometry を演算していないか。** 攻撃者由来の `shape_profile` を
  `validate_shape()` の前に座標計算へ渡すと court close で全ノードが落ちる(監査 F25)。演算の前に必ず shape を検証する。
- [ ] **`supports_court()` を実体より先に `true` にしていないか。** rung 2メソッドが既定のままなら
  `false` のままにする。嘘の `true` は「有罪判定できるクラス」を騙ることになる。

---

## 5. CI とチェックリスト(緑にすべきもの)

1. `check_lineage_v1(&lineage, &court)` — 静的バッテリ(§3 ステップ2)。
2. `check_sdk_v1(&sdk)` — 台帳全体で class id が衝突しないこと(`conformance.rs:187`)。
3. E2E drill テスト(§3 ステップ5)。BASE-0 の `backend.rs` drill テストを雛形に、covering `L` で
   honest 無罪 / guilty 有罪を assert。**difference で書く**: full `L` で通るクラスが、テーブルを
   1つ落とした `L` では落ちること。
4. graph check(§3 ステップ1)。エンジンの op 列 = 宣言グラフ。

---

## 6. 写経の順番(BASE-0 を最小完成例として読む)

1. `misaka-palw-base0/src/backend.rs:649` `execute_with_injected_fault` — drill の入口
2. 同 `:701` `bisect_prefix_state` — prefix commitment の最小形
3. 同 `:713` `refutation_for_index` — 終端証拠の組み方
4. 同 test module(`:1248` 以降)— E2E 往復の完成形(= certification vector)
5. `misaka-palw-base0/src/legs.rs` — `base0_bisect_prefix_state_v1` / `Base0StepCaptureV1` /
   checkpoint leg / `a16_captured_rows_v1`(他 engine の trace 変換例)
6. `misaka-palw-sdk/src/conformance.rs:29` `check_lineage_v1` — 静的バッテリの中身

BASE-0 が court を最後まで回せる最小の参照実装である理由は、この6箇所に全部書いてある。
新しいモデルを weight-bearing にするとは、この6つを自分の family について再現することだ。

---

## 付録: 用語

- **family** — backend の実行系統(BASE-0 / QWEN36 mmap / QWEN25-A16 dense 等)。certification は
  family 単位(`PalwE2eFamilyV1`)。class は到達 kernel 集合の包含で family に紐づく。
- **weightless / weight-bearing** — cadence share が 0 か非0か。ADR-0069 の gate は weight にだけ
  かかり、登録・liveness にはかからない。
- **covering leaf set** — drill が故障を注入する leaf の集合。全テーブル + prefill/decode を覆う。
- **`court_catalog_root` / `court_e2e_root`** — build の裁定能力(前者=kernel 単位、後者=E2E)を
  1個の hash として ruleset root にコミットしたもの。ADR-0075 以降、weight gate が読む certified
  集合は genesis ∪ チェーン状態。
