# PALW `palw-only-v4` 監査レポート — 2026-08-17

対象: worktree `.claude/worktrees/palw-only-v4`、監査時 HEAD `f7c6864`、基準 `e4848d2`
（28 commits / +14,452 行 / PALW 関連 30,800 行）
※ 監査実行中に別セッションが `eb53ac9`（gemmlowp の vendor）を追加。§2.2 に反映済み。それ以外の指摘は不変。
手法: 8 次元の独立監査 → 次元ごとの敵対的反証（既定姿勢 REFUTED） → 完全性批評 + 統合。18 エージェント、782 tool calls。
加えて監査者自身による一次検算（gemmlowp 総当たり比較、rustc プロファイル実測、`origin/main` 差分照合）。

指摘 81 件 → 反証 4 件 → **生存 77 件**（CONFIRMED 74 / PLAUSIBLE 2 / UNVERIFIED 1）。
critical 1 / high 22 / medium 20 / low 34。live 17 / dormant 60。

ベースライン: `cargo test -p kaspa-consensus-core -p misaka-palw-base0 -p misaka-palw-base0-ref2 -p kaspa-pow`
→ **893 passed / 0 failed / 0 warnings**。

---

## 0. 判定

**このブランチについては NO-GO（mainnet 候補として）。devnet soak と land-stage 継続としては GO。**

ただしこの監査の最重要出力はブランチ判定ではない。

> **P0: `origin/main` に、任意のピアが 1 メッセージでノードを落とせる remote-crash が現存する。
> PALW とは無関係に成立し、hash-only の mainnet と稼働中の testnet-10 に当たる。**

`palw-only-v4` 自体は、意図的に consensus-inert な land-stage として見れば良質である。fence は 4 preset 全部 `None`
で、それを assert するテストもある（`params.rs:2397`）。dormant 60 件はその設計どおりの帰結で、「壊れている」のではなく
「まだ繋がっていない」。問題は繋ぐ順序であって、繋ぐこと自体ではない。

---

## 1. P0（live・本番系・PALW 非依存）

### P0-1. pruning-proof 経路が `algo_id` 検査より前に PoW を計算する → 1 メッセージで panic

**サイト:** `consensus/src/processes/pruning_proof/validate.rs:168`（他に `mod.rs:219`, `apply.rs:65`, `apply.rs:171`,
および `validate.rs:189`）
**`origin/main` にも同一に存在する。** ブランチ由来ではない。

**これは新規発見ではない。** 2026-08-16 の algo_id=4 総合監査が既に **H-1** として報告している
（「pruning-proof 経路だけが順序反転」「無権限ピアが proof 1 通で bootstrap/recovery 中のノードを落とせる」）。
**1 日経って未修正のまま、`origin/main` に乗っている。** 今回の監査が追加するのは 3 点:
(i) 起爆装置 (a) は **PALW を一切必要とせず**、`PALW_WORKER` を正しく持つノードでも落ちる、
(ii) `pow_algo_id` が u32→u8 で切り詰められる（`protocol/p2p/src/convert/header.rs:126`）ため未知 id の生成は自明、
(iii) 前回 high と評価されたが、(i) により **hash-only mainnet で PALW 無関係に成立する**ので critical。

`StateLayer0::new` は `hasher`/`matrix` を `pow_algo_id == POW_ALGO_ID_KHEAVYHASH` のときだけ `Some` にする:

```rust
// consensus/pow/src/lib.rs:214-218 (origin/main では 203-208)
let (hasher, matrix) = if header.pow_algo_id == POW_ALGO_ID_KHEAVYHASH {
    (Some(PowHash::new(l1_seed32, header.timestamp)), Some(Matrix::generate(l1_seed32)))
} else {
    (None, None)
};
```

`calculate_l1_tag` の `_` アームはその 2 つを `expect()` する:

```rust
// consensus/pow/src/lib.rs:283-284 (origin/main では 244-245)
let hasher = self.hasher.as_ref().expect("kHeavyHash StateLayer0 carries a PowHash");
let matrix = self.matrix.as_ref().expect("kHeavyHash StateLayer0 carries a Matrix");
```

コメントは安全条件を明示している — 「any other id is rejected up-stack by header validation before PoW runs」。
**pruning-proof 経路はこの条件を満たしていない。**

```rust
// validate.rs:168 — proof[0].last() は完全にピア制御下のヘッダ
let proof_pp_level = calc_block_level_layer0(&proof_pp_header, &ppm.network_id, ppm.max_block_level);
...
// validate.rs:189 — PoW が先
let (header_level, pow_passes) = calc_block_level_check_pow_layer0(header, ...);
// validate.rs:199 — check_algo_id は後
if kaspa_consensus_core::pow_layer0::check_algo_id(...).is_err() { return Err(...) }
```

**起爆装置は 2 つ:**

| # | 入力 | 経路 | 影響ネットワーク |
|---|---|---|---|
| (a) | `pow_algo_id = 7`（未知 id なら何でも） | `_` アーム → `expect()` panic | **全ネットワーク**（mainnet 含む。PALW 一切不要） |
| (b) | `pow_algo_id = 4` を非 PALW ネットに | PALW アーム → `PalwUnavailable` → `lib.rs:135 panic!` | mainnet / testnet-10 |

`calc_block_level_check_pow_layer0` は `Err(_)` を `(0, false)` に落とすが、**`expect()` は `Err` ではなく panic** なので
捕まらない。

**前提条件（正直に）:** 攻撃者は被害ノードが IBD で選んだ syncer peer である必要がある。再起動直後・新規参加ノードは
必ず誰かを選ぶので、実務上は「ピアになれれば成立する」。

**最小修正（2 箇所）:**
1. `calculate_l1_tag` の `_` アームを `expect()` から `Err(PowLayer0Error::UnknownAlgoId(self.pow_algo_id))` に変える。
   `calculate_l1_tag` は既に `Result` を返しているので型は変わらない。これだけで (a) は塞がる。
2. proof 経路の **PoW 呼出より前**に `check_algo_id` を移す（`validate.rs` は 189 と 199 を入れ替えるだけ）。
   `mod.rs:219` / `apply.rs:65` / `apply.rs:171` には検査自体が無いので新設する。

なお `check_algo_id_known`（`pow_layer0.rs:482`）は「pruning-proof 経路で未知 id を弾く」と自分の doc に書いておきながら
**呼出元ゼロの dead code** である（low 指摘）。防御は設計されたが配線されなかった。

### P0-2. pruning-proof ヘッダが `check_palw_commitment_shape` を通らず、そのまま header store に永続化される

**サイト:** `consensus/src/processes/pruning_proof/validate.rs:200`, 書込は `apply.rs:172`

`palw_commitment` は非 PALW ヘッダでは **hash 不可視**（`hashing/header.rs` の digest gate が `is_palw_algo_id` で
ゲートされている）。main pipeline は `pre_ghostdag_validation.rs:82` で `check_palw_commitment_shape` を呼んで非空を
拒否するが、**proof 経路はこれを呼ばない**。

結果: ピアは正直な pruning proof の各ヘッダに任意バイト列を N KB 付けられる。`header.hash` は変わらず、PoW も親リンクも
通り、`validate_pruning_point_proof` は `Ok` を返し、`apply.rs:172` が junk ごと `headers_store` に書く。
さらに `PruningProofManager` は同じ store から proof を再構築する（`build.rs:185`, `build.rs:429`）ので、
**被害ノードが junk を他ノードへ再配布する**。無制限バイトの永続化＋伝播で、ストレージ増幅にもなる。

修正: proof のヘッダループにも `check_palw_commitment_shape` を入れる。`PALW_COMMITMENT_MAX_BYTES` は
「wire cap」と doc されているが実際は header validation の奥でしか見ていない（`protocol/p2p/src/convert/header.rs:145`）ので、
デコード境界に移すのが本筋。

### P0-3. ピア由来ヘッダ 1 本ごとにフルの LLM 推論 1 回、親チェックより前・グローバル mutex 直列

**サイト:** `consensus/src/pipeline/header_processor/pre_ghostdag_validation.rs:22`（PALW ネット = t10/t11/devnet で live）

`validate_header_in_isolation` は `check_pow_and_calc_block_level` を **`validate_parent_relations` より前**に走らせる
（`processor.rs:320-321`）。よって親が存在しない捏造ヘッダでも推論 1 回を買える。各推論は
`SPAWN_GATE`（`palw.rs:203`）を取るので**他のヘッダ検証は完全に直列化**する。
worker がハングすると `DEFAULT_TIMEOUT_SECS = 300`（`palw.rs:42`）× `run_worker_with_retry` 3 回（`palw.rs:227`）で
**1 ヘッダが最大 15 分グローバルゲートを保持**する。

blockrelay の blue-work スキップ発見的手法（`flow.rs:260-269`）は、その時点で未検証の `blue_work` を大きく申告すれば
迂回できる。

**併発する回帰（完全性批評が発見、どの次元も開かなかったファイル）:** `misaka-palw-worker/src/main.rs:432`
— B15 修正で v1 の model gate を撤去して v2 の always-recompute gate に統合した結果、
`--mode verify`（＝ブロック検証が呼ぶモード）の**最初の仕事が 1.28 GB の SHA-256 全読み**になった
（`main.rs:446-449` の `std::io::copy(&mut file, &mut hasher)`）。
HW 支援 SHA-256 で ~0.85 s/header、`sha2` ソフトフォールバックで ~5 s/header。
これも `SPAWN_GATE` の内側。mmap ベースの llama.cpp ロードでは触らなかった 1.28 GB の page-cache/disk トラフィックが
毎ヘッダ発生する。

修正順: ①`validate_parent_relations` を PoW より前に出す ②verify モードのモデルハッシュを 1 回だけにする
（プロセス生存中メモ化、または mtime+size ではなく起動時 1 回）③`SPAWN_GATE` の粒度を見直す。

---

## 2. 算術層（BASE-0）— 監査者自身の一次検算

### 2.1 検算で確認できたこと（修正は正しい）

自作の gemmlowp 参照実装と総当たり比較（実装を見ずに仕様から再導出）:

| 対象 | 比較数 | 不一致 |
|---|---|---|
| `srdhm` vs 本物の gemmlowp `SaturatingRoundingDoublingHighMul` | 92,529 | **0** |
| `rounding_shift_right` vs round-half-away-from-zero | 128,736 | **0** |
| （参考）修正前の欠陥形 vs gemmlowp | 92,529 | 44,234（47.8%） |
| （参考）修正前の RSR vs 仕様 | 128,736 | 59,791（46.4%） |
| i32 溢れ | 両方 | **0** |

**先の 3 件の修正は正しい。** 報告されていた 50.1% / 3.2% の数字も再現した。

### 2.2 新規: ADR-0040 C1 の規範文が C2 の実装と矛盾する（**この監査で新規発見**）

ADR-0040 C1 は規範として宣言する:

> **C1. Rounding is round-half-away-from-zero, and it happens only in `RoundingShiftRight`.**
> "One rule, one site. Every other integer operation is exact, so this is the *only* place a value loses information"

しかし C2 の `SRDHM` も `/(1<<31)` で情報を落とし、**丸め規則が違う**。nudge が `1 - 2^30`（対称な half より
絶対値が 1 小さい）なので、負の積のちょうど half は **ゼロ方向**に丸まる。正の積は away に丸まる。
つまり実体は round-half-**up**（+∞方向）であって round-half-away ではない。

実測: 92,529 ペア中 **2,019 件（1.57%）**で「出荷実装」と「C1 の規則に忠実な SRDHM」が食い違う。

```
SRDHM(-2147483647, 1073741824)   出荷実装 = -1073741823
                                 C1 忠実  = -1073741824
```

half 条件は `|a·b| ≡ 2^30 (mod 2^31)` なので**任意に構成できる**。統計的レアケースではない。

**なぜ重大か。** 直したばかりの欠陥の**極性が反転しただけの同型**である。
前回は「コードが gemmlowp から外れた」、今回は「コードは gemmlowp に一致したが**仕様文が**コードから外れた」。
C2 の擬似コードから写した第三者は一致し、C1 の規範文から実装した第三者は 1.57% で不一致 →
ADR-0027 の法廷では rounding difference ではなく **conviction**。
そして `misaka-palw-base0-ref2` も同じ gemmlowp 解釈なので、**差分試験では原理的に見えない**。
これは既に記録済みの「構造的独立であって著者独立ではない」限界の、具体的で計測された実例である。

**`eb53ac9`（監査中に着地した gemmlowp vendor）は、この指摘を解消せず、むしろ決定的にする。**
vendor された upstream 原典 `misaka-palw-base0-ref2/vendor/gemmlowp/fixedpoint/fixedpoint.h` を直接読むと、
**同一 oracle の中で 2 つの primitive が別の丸め規則を使っている**ことが確認できる:

```cpp
// :346-348  SaturatingRoundingDoublingHighMul —— 非対称 nudge + 切り捨て除算 = round-half-UP
std::int32_t nudge = ab_64 >= 0 ? (1 << 30) : (1 - (1 << 30));
std::int32_t ab_x2_high32 = static_cast<std::int32_t>((ab_64 + nudge) / (1ll << 31));

// :375-378  RoundingDivideByPOT —— 負値だけ threshold を +1 = round-half-AWAY-from-zero
const IntegerType threshold = Add(ShiftRight(mask, 1), BitAnd(MaskIfLessThan(x, zero), one));
return Add(ShiftRight(x, exponent), BitAnd(MaskIfGreaterThan(remainder, threshold), one));
```

コミットメッセージはこの 2 行を並べて引用しながら、**両者が違う規則を実装していることには触れていない**。
`eb53ac9` は「~1.8M 入力で spec が upstream と厳密一致」を機械検査で固定したので、
コードは SHA-256 pin 付きで round-half-up に**確定した**。一方 ADR-0040 C1 の規範文は変更されておらず
（`git show eb53ac9 -- docs/adr/0040-*.md` は空）、今も round-half-away を単一規則として宣言している。
**乖離は縮まらず、機械検査で固定された。**

また upstream の `RoundingDivideByPOT` は `assert(exponent >= 0); assert(exponent <= 31);` で
**定義域を実行時 assert している**（:369-370）。ref2 も hard assert する。
定義域を `debug_assert!` でしか守っていないのは **repo の 32bit 版だけ**で、§2.3 の指摘はこれで一層強くなる。

**修正:** C1 の見出しを事実に合わせる。案:
> C1. 情報が失われるのは `RoundingShiftRight` と `SRDHM` の 2 箇所だけであり、
> 前者は round-half-away-from-zero、後者は gemmlowp 互換の round-half-up である。
> 2 つの規則が違うのは意図的で、`SRDHM` は既存実装との bit 一致が選定理由だからである。

`palw_base0.rs:82-83` のモジュール doc（「the ONE site ... Every other operation here is exact」）も同時に直す。
なお `rescale_q` の doc は「`requantize` rounds twice (inside SRDHM at bit 31, then again at `shift`)」と
正しく書いており、同じツリー内で自己矛盾している。

### 2.3 `requantize` の shift が未検証で、refutation 経路から生バイトが入る

**サイト:** `consensus/core/src/palw_base0.rs:102`（監査エージェントも独立に発見、medium）
**到達経路（監査者が追加で特定）:** `consensus/core/src/palw_step_refute.rs:272`

```rust
// palw_step_refute.rs:267-273 — weight oracle 行から生バイト
let params: Vec<ops::QuantParams> = row.chunks_exact(5)
    .map(|c| ops::QuantParams {
        multiplier: i32::from_le_bytes([c[0], c[1], c[2], c[3]]),
        shift: c[4],                     // ← 0..=255 のまま、検証ゼロ
    }).collect();
```

→ `requantize_row` → `requantize` → `rounding_shift_right(_, s)`。防御は `debug_assert!(s <= 31)` **だけ**。
`rescale_q` は `shift.min(RESCALE_MAX_SHIFT)` で無条件 clamp しているのに（`palw_base0.rs:214`）、こちらには無い。

**本番プロファイルでの実測**（`Cargo.toml:372` が `[profile.release] overflow-checks = true` なので、
`-O -C debug-assertions=off -C overflow-checks=on` が本番の条件）:

```
shift=  8 -> 3906      shift= 32 -> 0       shift= 63 -> 0
shift= 31 -> 0         shift= 62 -> 0       shift= 64 以上 -> PANIC
```

- `s ∈ 32..=63`: ADR-0040 C1 の定義域 `0..=31` の外で、黙って 0 を返す。定義域外を拒否する第三者実装とは結果が割れる。
- `s ≥ 64`: **release でも panic**。公開の総関数における panic は、`palw_base0_ops` が構造的に排除しているはずの
  remote halt そのもの。`rounding_shift_right_64` の overflow panic を i128 で直したのと**同じ欠陥が 32bit 版に残り、
  そちらが refutation 経路側**である。

**今日は到達しない**: live な weight oracle は `NoStepWeights`（`processor.rs:7939` が常に `None`）なので、
全ての step conviction は `Unadjudicable` になる。Track-D で本物の oracle を入れた瞬間に起爆する地雷。

**修正:** `rounding_shift_right` に `rescale_q` と同じ無条件 clamp か、`QuantParams` を検証付きコンストラクタ経由に
限定する（フィールドを private に）。

### 2.4 エージェントが見つけた算術欠陥（`overflow-checks = true` により全て release panic）

| 深刻度 | 内容 | サイト |
|---|---|---|
| low→**要注意** | `int_recip(v) = (int_rsqrt(v) * int_rsqrt(v)) >> K` が **1 ≤ v ≤ 511 の全てで i64 溢れ**。ADR-0040 の `IntRecip` は総関数ではなく、宣言された定義域の 511 入力で spec と ref2 が別の値を返す | `palw_base0.rs:290` |
| medium | `rms_norm` の Qk narrowing が `as i32` で**飽和せず wrap**。row 幅 ≥ 16385 で最大活性の符号が反転する。ADR-0040 C3 は "nothing wraps anywhere" と明言 | `palw_base0_ops.rs:194` |
| medium | `rope_table` の sin 項が i32::MIN 四隅で i64 溢れ（cos 項は溢れないことを導出済み） | `palw_base0_ops.rs:226` |
| low | RoPE 生成器が未検証の `ln_theta_gen_q` で i128 溢れ、`from_parts` が無制限 vocab で usize 溢れ。`Base0ShapeV1::validate` は非ゼロしか見ていない | `misaka-palw-base0/src/rope.rs:128` |
| low | `RopeTableV1::digest_bytes` が `cos_q.iter().zip(sin_q.iter())` で長さ不一致を不可視化し、entry 数の接頭辞も無い。`row()` は長さ検査なしで slice | `rope.rs:234` |

class digest については **偽造方向には injective であることが検証された**（engine が読む全フィールド＝Decision H の
2 つの `ScaleParams` を含めて digest に入っている）。逆方向は non-injective で、engine が読まない `requant[4]` を
吸っているため同一計算に 2^40 通りの class id が付く（`artifact.rs:325`、実害は低いがテストが「必須」として固定している）。

---

## 3. dormant high — 根本原因ごとにまとめる

60 件が dormant。ただし「fence を開けた日に何が起きるか」で読むべきもので、根本原因は 4 つに集約される。

### 3.1 認証の不在 — 署名を検証する経路が weight/credit 側に一つも無い

- **receipt の署名がどこでも検証されない**（`palw_facts.rs:342`）。偽造 receipt で捏造フォークを full safe weight まで
  熟成できる。ADR-0038 の `Final` 前提（「private fork は self-finalize できない」= commit 04c2650）が
  **認証されていない receipt の上に乗っている**ので、private-fork 攻撃が復活する。
- **credit 経路と weight 経路は署名ゼロ**（B1 は slash 2 経路のみ PARTIAL CLOSED、`processor.rs:4864`）。
- **形式が整うだけの conviction carriage**（署名なし・裁定なし）がブロックの PALW weight を恒久的に無効化する
  （`palw_facts.rs:350`）。B9 と同型。
- **署名なし・bond なしの `BisectMove::Open` 1 本**で任意ブロックを永久に Provisional に固定できる
  （`palw_facts.rs:409`）。admission は shape check しかしない（`palw_carriage.rs:1176-1182`）。

### 3.2 順序と純粋性 — 同じ DAG から違う weight が出る

- **`dispute_is_open_v1` が bisection ladder を生スライス順で replay する**（`palw_facts.rs:432`）。
  sort key が無く、turn 違いの move を `let _ =` で捨てる（`palw_bisect.rs:295`, `298-300`)。
  前向きに歩けば Terminal → Final、後ろ向きなら AwaitVerdict → Provisional。
  唯一の防御は「caller の walk order に従う」という**強制されていない doc コメント**（`palw_facts.rs:430-431`）で、
  canonical carriage order はこのブランチのどこにも定義されていない。
  しかも最も近い既存の前例 `compute_palw_credit_outputs` は **newest-first** で歩く（`processor.rs:4886`）。
  隣の `crossing.sort_by_key(|(tx_id,_,accepted)| (*accepted,*tx_id))`（`processor.rs:4922`、
  コメントは "in one pinned order (construction == validation)"）が正解の形を既に示している。
  検証者の指摘どおり `resolve_block_facts_v1`/`chain_weights_v1`/`compare_tips_v1` は**非テスト呼出がまだゼロ**なので
  今日は分断しない。**次にその caller を書く人のための地雷**である。
- **pwu が class の現在 DAA target から導出され、per-block の target 履歴が無い**（`palw_facts.rs:124`）。
  retarget のたびに既に成熟した履歴の safe weight が書き換わる。
- **`pwu_per_inference` に store も view も lookup も無い** — ブロック weight の支配項が、
  resolver が block 自身の `execution_class_id` と突き合わせもしない bare な caller 供給 `Option` である
  （`palw_facts.rs:316`、完全性批評）。

### 3.3 誤有罪 — 正直な実行者が slash される経路

- **正直な executor が公開した attestation だけで bond を切れる**: 「authorship half」が
  refute 対象の execution commitment に一度も束縛されない（`palw_carriage.rs:887`）。
- **RmsNorm が `eps_q = 1` ハードコードで裁定される**（`palw_step_refute.rs:218`）。
  class の実 epsilon を運ぶ registration フィールドが無いので、それ以外の epsilon を持つ正直な producer は有罪になる。
- **slash 裁定が署名メッセージを証明書自身の `network_id` に束縛する**（`palw_slash.rs:327`, `palw_carriage.rs:882`）。
  devnet/testnet の証明書が mainnet の bond を切る。evidence の dedup も無い。
- **step adjudicator が job context の宣言しない shape profile で再計算する** —
  `job_context.shape_profile_id` が一度も比較されない（`palw_step_leg.rs:818`）。
- **`MatMulQuant` が oracle に 1 出力行分の重みしか要求しない**ので 1 要素出力しか再計算できず、
  しかもその失敗が `Unadjudicable` ではなく**チャレンジャーの過失**として報告される（`palw_step_refute.rs:250`）。
- **「閉じた」BASE-0 catalog が op 9 `Rescale` を欠く**（`palw_step_refute.rs:105`）。
  ADR-0040 Decision H が「これ無しでは他の 9 つは計算できない」と言っている当の op である。
  つまり 2.2 と同根で、**catalog が閉じたという主張が裁定側にはまだ届いていない**。

### 3.4 mint 経路 — 前回 9 blocker の回帰

| # | 前回（`palw-credit-gate-b14` @ `9c0b914`） | この lineage での状態 | サイト |
|---|---|---|---|
| B1 | PALW 署名が consensus のどこでも未検証 | **PARTIAL** — slash 2 経路のみ検証。credit / weight はゼロ | `processor.rs:4864` |
| B2 | `committed_root` の dedup が無い | **STILL OPEN** | `processor.rs:4901` |
| B3 | credit output が予算・上限なしで coinbase に append | **STILL OPEN** | `processor.rs:7173` |
| B4 | `min_credit_interval_daa` が consensus で未強制 | **STILL OPEN** — §4e の leverage 不等式は vacuous | `palw_schedule.rs:360` |
| B5 | payee が非一意 `validator_pubkey_hash` × HashMap 順 | **STILL OPEN** — `.find()` で解決し、commitment 自身の `bond_outpoint` は無視 | `processor.rs:4982` |
| B6 | gate 入力が純関数でない（pruned = 空 Vec = fail-open） | **STILL OPEN、かつ悪化** — ADR-0032 audit-bond spend gate が sink 相対の virtual store を読むブロック妥当性規則になった | `processor.rs:2611`, `utxo_validation.rs:1553` |
| B7 | algo-4 排他 PoW・hash floor 無し・runtime 不在で panic | **DECIDED but NOT IMPLEMENTED** — panic は残る（意図的、transient は retry が吸収）。ADR-0036 D4 が mainnet の hard precondition とした恒久 hash floor は**未実装**（ADR-0036 は Proposed） | `consensus/pow/src/lib.rs:135` |
| B8 | class identity が libm を pin していない | **PARTIAL** — manifest は libm を運ぶが、credit gate は `runtime_class_id`（タグ文字列のハッシュ）しか照合しない | `palw_credit.rs:149` |
| B9 | 形式だけの refutation が bond も裁定もなくクレジットを消す | **STILL OPEN** | `palw_carriage.rs:505` |

**9 件中 CLOSED は 0 件。** PARTIAL 3、STILL OPEN 5、DECIDED-not-implemented 1。
architecture が ADR-0038 で反転したにもかかわらず、mint 経路の consumer 層は `palw-credit-gate-b14` 当時のまま
持ち越されている。

**「bonded / not frozen」は live 呼出サイトで今もハードコード true**（`processor.rs:4930-4938`、監査者が一次確認）:

```rust
let candidates: Vec<PalwPanelCandidateV1> = bonds.iter().map(|b| PalwPanelCandidateV1 {
    validator_id: b.validator_pubkey_hash,   // 非一意キー。正解の bond_outpoint は同じ b にあって捨てられる
    runtime_class_id: credit.registration.runtime_class_id,
    bonded: true,                            // ハードコード
    frozen: false,                           // ハードコード ＝ 緊急停止が効かない
}).collect();
```

一方、これを直した `select_job_panel_v3`（`bond_outpoint` と operator で dedup し `class_frozen` を尊重、
`palw_job_panel.rs:58`）は **非テスト呼出がゼロ**（呼出 144/219/230 は全て `#[cfg(test)]` 内、テストは 123 行目から）。
**landed but not wired** の典型で、前回監査が `Wired?` 列を作った理由そのものが再現している。
なお V3 自身にも欠陥がある: bond outpoint で dedup しながら seat の ticket と解決は `validator_id`
（非一意と明記されている）で行う（`palw_job_panel.rs:116`）。

**緊急停止は依然として動かない。** `class_frozen` は型としては存在するが、live path で参照する箇所は無く、
class store は Frozen 状態を表現できない（`palw_dispute.rs:219`）。

---

## 4. ADR と実装の乖離（doc-vs-code）

前回監査で ledger を訂正した経緯があるので、今回も明示する。

1. **ADR-0038 の中心的性質「admission は hash 検査であってモデル実行ではない」が到達不能**
   （`pow_layer0.rs:426`）。`consensus/pow` は `palw_commitment` を一切読まず、shape 規則は非空を全て拒否する
   （`check_palw_commitment_shape`、`pow_layer0.rs:426-428`）。commit f3455ff の「the header carries its work's claim,
   end to end」は**配管のみの着地**である。malleability は本当に閉じている（`pre_ghostdag_validation.rs:82` に live caller
   あり、監査者が一次確認）が、それは同時にフィールドが完全に inert だということでもある。
2. **ADR-0038「full nodes stop re-running inference, ever」も未実装**（`pow_layer0.rs:447`）。algo-4 は今も排他 PoW。
3. **fork-choice fence を開けても fork choice は何も変わらない**（`processor.rs:6259`, `header_processor/processor.rs:428`）。
   seam 呼出サイト 2 箇所とも `palw` を `None` ハードコードしており、weight derivation を呼ぶものが無い。
   一方 fence は params fingerprint には入る（`params.rs:733`）ので、**開けると P2P fingerprint だけが割れる**。
4. **`params.rs:434-437` は「`validate_palw_v1` は現在 `Some` を拒否する」と書くが、コードは拒否しない**
   （`params.rs:550-558`：`palw_credit` が `None` のときだけ拒否）。
   `palw_tip_order_v1` の doc（`params.rs:571`）も「fence は設定できないので常に BlueWorkOnly」と書くが、
   実際の保証は「どの preset も設定していない」だけ（`params.rs:2397` のテストが assert しているのはそちら）。
   構造的保証と設定値は別物である。
5. **ADR-0039 Decision 5 の epoch weight budget「enforced at admission, by rejection」は存在しない**
   （`palw_class_daa.rs:49`）。
6. **ADR-0039「coverage complete before a class carries weight」に強制点が無い**（`palw_catalog_coverage.rs:128`）。
7. **ADR-0039 Decision 4 が言う「landed challenge は class と bond outpoint を束縛する」に対し、
   live な algo-4 seed はどちらも束縛しない**（`pow_layer0.rs:692`）。記述されている 6 成分構成は到達不能な方である。
8. **`PALW_RUNTIME_MANIFEST_VERSION_V2` は「新しい manifest が名乗ってはいけない版」と doc されるが、
   manifest の `version` フィールドを検証するコードが無い**（`palw_v2.rs:83`）。
9. **`libm_identity` は「診断専用・load-bearing ではない」と doc されながら manifest にハッシュされている**
   （`palw_v2.rs:1023`）。`expf`/`logf` の算術が同一な glibc の点リリースでも class が割れ、
   正直な worker が正直な job envelope で `die()` する。B8 の修正が作った新しい面。

---

## 5. その他 live（medium/low）

- **`LATEST_DB_VERSION` が 7 のまま**なのに Header が bincode シリアライズのフィールドを得た（`factory.rs:66`）。
  in-place アップグレードが「古い DB を綺麗に拒否」ではなく **panic** する。
- **main pipeline は `PalwUnavailable`/`PalwWorkerFailed` を `RuleError::InvalidPoW` に変換する**
  （`pre_ghostdag_validation.rs:157`）。panic を入れた理由（「静かな全件拒否を避ける」）と正面から矛盾する経路が
  同じツリーに共存している。
- **class calibration probe の一時的失敗がプロセス生存中 `PalwUnavailable` として恒久メモ化**される（`palw.rs:371`）。
- **algo-5 の fixture 分岐が model-pin/class 検査より前に走る**（`palw.rs:159`）。algo-4 が
  「calibration を先に置く」ことで閉じた穴が algo-5 に開いている。
- **`run_worker` が stdin 書込・wait エラー経路で子プロセスをリーク**する（`palw.rs:272`）。
- **`pow_algo_id` が p2p / gRPC 境界で u32 → u8 に黙って切り詰められる**（`protocol/p2p/src/convert/header.rs:126`）。
- **新しい PALW carriage subnetwork id 4 種（0x46–0x49）が全 preset で activation fence 無しに admit される**
  （`tx_validation_in_isolation.rs:297`）。

---

## 6. しっかりしている所（公平に）

- **block identity の malleability は本当に閉じている。** digest は三重ゲート（algo / digest path / 空判定）で、
  `check_palw_commitment_shape` は live な caller を持つ（`pre_ghostdag_validation.rs:82`）。
  「PALW soak net は再創世するから」という当初の主張が偽だったことを自ら発見して訂正した記録も残っている。
  正しい形の自己訂正である。
- **BASE-0 の 3 件の算術修正は、独立総当たり 22 万件で完全一致。** 直したという主張は正しい。
- **class digest は偽造方向に injective。** engine が読む全フィールド（Decision H の 2 つの `ScaleParams` を含む）が
  digest に入っていることが独立検証された。
- **dormancy は本物で、テストで固定されている。** `palw_credit`/`palw_fork_choice` は 4 preset 全て `None`、
  `params.rs:2397` が「どの preset も fence を入れない」を assert し、fence は fingerprint にも入っている。
- **`NoStepWeights`（`processor.rs:7939`）が正しい方向に倒れている。** oracle を持たないノードは
  `Unadjudicable` を返し、誰も有罪にしない。「再計算できないノードは、その step が間違っていることを立証していない」
  という判断は正しい。
- **RoPE テーブルを float 経由にしなかった判断は正しい。** 生成器が整数なら第三者が shape からバイト一致で再導出でき、
  争点が「公開 blob を信じろ」ではなく短い整数プログラムになる。
- **`crossing.sort_by_key(...)`（`processor.rs:4922`）は正解の形を既に持っている。** 3.2 の修正は
  新概念ではなく、隣にある形の横展開で済む。
- **893 passed / 0 failed / 0 warnings。**

---

## 7. 最短経路

### 即時（本番系。ブランチとは独立に、今日）

1. **P0-1 を塞ぐ** — `calculate_l1_tag` の `_` アームを `Err(UnknownAlgoId)` に変え、proof 経路の PoW 呼出前に
   `check_algo_id` を移す（`validate.rs` は 189/199 の入替）。`mod.rs:219` / `apply.rs:65,171` には新設。
   `origin/main` にも当てる。**これは他のどの項目とも直列依存が無く、最優先。**
2. **P0-2 を塞ぐ** — proof のヘッダループに `check_palw_commitment_shape` を追加。1 と同じファイルなので同時に。
3. **P0-3 を緩和** — `validate_parent_relations` を PoW より前に出す（安価な検査を先に）。
   `misaka-palw-worker` の verify モードのモデルハッシュを毎ヘッダから外す。t10/t11 の実測で効果を確認。

1–3 は並列に書けるが、1 と 2 は同一ファイルなので 1 変更セットにまとめるのが良い。

### 短期（ブランチ、並列可能）

4. **ADR-0040 C1 の規範文を訂正**（2.2）。`palw_base0.rs:82-83` のモジュール doc も同時に。
   放置すると第三者実装が誠実に書くほど有罪になる。安価で、リリース判断を最も変える。
5. **算術の総関数化** — `rounding_shift_right` の clamp、`int_recip` の v ≤ 511、`rms_norm` の saturating narrowing、
   `rope_table` の sin 項、RoPE 生成器の入力検証。`overflow-checks = true` なので全て release panic であり、
   全て「公開の総関数」契約違反である。ref2 側に対応する定数複製と定義域テストを同時に入れる。
6. **doc-vs-code の 9 件を訂正**（§4）。前回 ledger を訂正したのと同じ作業。

### 中期（直列。ここが本当のクリティカルパス）

7. **catalog を裁定側に届かせる** — op 9 `Rescale` を `palw_step_refute` の catalog に入れ、
   `MatMulQuant` の oracle 要求を出力行数に合わせ、`eps_q` と `shape_profile_id` を registration が運ぶようにする。
   これができるまで「catalog が閉じた」は実行側だけの主張である。
8. **weight 導出を chain state の純関数にする** — bisect replay の canonical order（`(accepted_daa, tx_id)`）、
   per-block DAA target 履歴、`pwu_per_inference` の store と class 照合。
   3.2 の 3 件は同一の根で、**1 つ欠けると別が fail-open する**ので分割不可。
9. **認証を weight/credit 経路に通す** — receipt 署名、conviction の署名と裁定、`BisectMove::Open` の bond 要求、
   証明書の network_id をチェーン側から取る。3.1 の 4 件も分割不可。
10. **mint 経路を 1 変更セットで健全化** — B2/B3/B4/B5/B6/B9 と `select_job_panel_v3` の配線。前回と同じ結論で、
    **1 つ欠けると別が fail-open する**。ここで `bonded`/`frozen` のハードコードを消し、
    動く緊急停止（`class_frozen` を live path が読む）を作る。
11. **~~hash floor の実装~~ → PALW-BASE-0 の登録と常時 Active 化**（B7）— ADR-0036 D4 は
    ADR-0039 W6′ に**置き換えられた**。mainnet にハッシュ floor は積まない。ブロック生成は
    全ネットワークで PALW work であり、liveness floor は可搬な整数専用クラス `PALW-BASE-0`
    （catalog が閉じるので任意の CPU で監査・有罪判定できる）。PALW 全断は**意図された大声の停止**で、
    ハッシュ順序への劣化ではない — 常にブロックを出せるハッシュ lane は、work ではなく lane を
    掘る恒久的インセンティブになるから。成果物は floor の実装ではなく、BASE-0 の artifacts・
    第 2 実装・difficulty-domain share。
    mainnet identity と束ねるので、新 network identity の決定と同時。

7 → 8 → 9 → 10 は直列で、**月単位**。fence を開けるのはその後。
今日 fence を開けると fork choice は何も変わらず P2P fingerprint だけが割れる（§4.3）ので、
「開けてみる」という中間ステップは存在しない。

---

## 付録: 反証された 4 件（反証が正しいと確認したもの）

1. `sink_search` の `blue_work` take_while が BlueWorkOnly 順序でしか正しくない（`processor.rs:6323`）
2. 64bit shift の定義域外挙動が ref2 と種類として違う（`palw_base0.rs:160`）
   — 唯一の in-tree caller `rescale_q` が `shift.min(RESCALE_MAX_SHIFT)` で**無条件 clamp** している（`palw_base0.rs:215`）。
   正しい反証。ただし **32bit 版（2.3）には clamp が無く、そちらは refutation 経路から到達する** ので別件。
3. filed refutation が job を永久ロックする（`palw_job_state.rs:127`）
4. `check_pwu_claim_v1` に非テスト呼出が無い（`palw_block_commitment.rs:204`）

---

監査方式の限界: 構造的独立であって著者独立ではない。仕様の読み違いは両側に再現される（2.2 がその実例）。
`SRDHM`/`RoundingShiftRight` については gemmlowp が直接 vendor 可能で、そこが third-party の食い違いが
最も出た箇所なので、次の一手はそこ。
