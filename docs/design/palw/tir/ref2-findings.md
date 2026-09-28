# PALW-TIR v1 — findings of the independent second implementation (`misaka-palw-tir-ref2`)

> Scope: RFC-0002 freeze criterion 4 (`reference evaluator == independent second implementation ==
> every backend`) for `docs/spec/palw/04b-tensor-ir.md`: its evaluator, its admission
> (`tir_admit_v1`, §10.3 with §7 and §8), its demand evaluation (`eval_demanded`, §9.4) and now its
> H dissection (§9.5, PALW-TIR-37). This file records where the normative text is ambiguous, silent
> or wrong, and where the first implementation (`misaka-palw-tir` and, for §9.5, the dissection
> court in `kaspa-consensus-core`, observed only as black boxes through their public APIs) departs
> from it. Branch `tir/ref2`: revision 1 of 04b from `tir/core` b7492d601; revision 2 from a2e6ec35a
> (merged as 5f852c190); admission from ccd8f6aae (merged as 34c9d92d3); demand evaluation from
> `tir/phase-f` 23cf9235c (merged as ef0ad0984); the A1–A7 fix from `tir/core` b9d75a4e5 (merged as
> 564546f52); **§9.5 and the §9.4 decisions** from `tir/phase-f-f7` c0d826a1b (merged as afa519c0f).
> Written without reading `misaka-palw-tir/`, `consensus/` (tests included), the other files of this
> directory, or the legacy kernels; the text changes were read as `git diff` of 04b only.

## 概要(日本語)

- **H dissection(§9.5)を本文だけから独立実装**: `H` 上の縮約とサイト、`eval_range`(supplied と範囲)、
  root claim の finalize と要素閉包(1 index の probe を不動点まで)、受理手順 2〜5、cut・round 予算・部分和、
  厳密な fold、最下層の最初の不一致 index、義務 O-1〜O-3 と O-5 の `V`。**dissect vector 32/32 を再現**。
- **第 1 実装の court そのものと ×25 で突き合わせ**(公開 API のみ・ソース不読): 本物の step leg・binding・
  evidence store を組み、court の root claim 生成/判定・round 生成・phase の fold 判定・bottom 生成/判定を駆動。
  **6,787 局で不一致 0**: 各縮約への嘘 4,126 件はすべて最下層で**嘘の値の index ちょうど**で有罪、正直な
  executor 471 件は挑戦者敗北、root claim 拒否 1,045・round 拒否 1,145 件はどちらも同じ一手で拒否。
  phase と `eval_demanded_range` の層でも 68,590 局・192,840 手を比較。
- **H1(第 1 実装の逸脱、正直者が負ける)**: タイルが錐の縮約の一部しか読まないと、本文(手順 2 と 5)は
  その縮約に空の要素リストを要求するが、第 1 実装は空リストを拒否する(**自前の builder が作る claim を
  自前の checker が拒否**)。正直な executor は受理される root claim を出せず、沈黙で負ける。×25 で 99 件。
- **H2(admission の分岐)**: 第 1 実装の O-5 の `V` が本文の式より小さく、実際の閉包より小さい
  (例 2 対 5)。錐内の `MatMul` の第 1 オペランドが葉のとき、第 2 オペランドへ `d·K` を渡していない。
  上限 4096 付近で受理が割れ、正直な root claim が上限を超えうる。corpus と Qwen 形では一致。
- 他は editorial: H3 O-3 は §2.2(形に `H` は 1 つまで)から常に成立、H4 `h_tile` の 2 冪・タイル 4 lane 以上は
  Phase F の layout 規則で 04b にない、H5 supplied の不正に class 記述なし(両者 `Malformed`)、
  H6 O-5 の court window 条件は ADR-0082 Z4 参照で 04b だけでは実装不能。
- §9.4 の D1〜D5 は c0d826a1b で本文が決定(`state_after` の `[lo, hi]`、未保持 instance は `Malformed`、
  carried walk の terms、source の拒否は常に `Missing`)。ref2 を追従、**demand vector 697/697**、差分 0
  (第 1 実装の D4 も修正済み)。
- Phase F API の観察(§9.5 外): `palw_tir_canonical_context_v1` が `max_context_tokens = layout.max_context`
  を与えるのに、その context で `layout.max_context` 位置を使うジョブを `verify_tir_binding_v1` が
  "job context shape" で拒否(`positions + 1` なら通る)。arming 前に確認を推奨。

- **demand 評価(`eval_demanded`、§9.4)を本文だけから独立実装。** context `(p, o)`・2 種の target・
  要求の事前拒否(`Position`/`Malformed`)・5 つの source 質問・leaf(target は常に計算)・全 primitive の
  index map(表の順)・`Fixed` state の replay(writer の `p − 1`、誰も書かない instance は `p − q` を運ぶ)・
  work(要素 1 回・TopK は行ごと・`K`/`n`/運んだ position 数)を operand より先に課金・frame 上限・class。
  明示スタックで再帰なし(`history_bound − 1` からの 26 万 position の replay も両実装で成功)。
- **golden vector 672/672 を再現**(値・work・request 集合の grouping と並び・拒否 class)。
- **×25 の差分 776,251 件で不一致 0**: 乱数 program(range-safe と無保証)× 乱数 target × 乱数要素集合、
  replay window(一部 position だけ state を供給、0 に無い場合も)、敵対的 source(拒否・dtype 外・`[lo, hi]` 外・
  供給値への `Replay`・`token_bound` の token・不整合な値/極値、最大 3 か所)、work 上限ちょうどと 1 つ手前
  (171,418 組)。成功 510,277 件は値・work・request 集合がすべて一致(問い合わせの初出順も 97.4% で一致。順序は
  本文上 結果に含まれない)。失敗 265,974 件はすべて本文が許す class(複数要素が失敗しうる 150 件だけ class が
  異なり、どちらも許容集合内)。全 25 primitive と `state_after` が target として成功側で網羅。
- 本文の穴(D 系列): **D1** commit 済み writer の値の `[lo, hi]` 検査が replay にはあり `state_after` には無い
  (両実装とも字義どおり: 100 を state の値として返し、replay では `Operand`)。**D2** 値が見つからない carried
  walk の terms が未定義(両実装とも通過 position ごとに 1 課金 → 上限次第で `WorkLimit`/`Missing`)。
  **D3** どの block も参照しない instance の `state_after` が拒否されない。**D4 第 1 実装の逸脱(label のみ)**:
  source の拒否 class をそのまま返す(本文は `Missing`)。court では `InputSetNotCanonical` と
  `Unadjudicable` の取り違えになりうるが、どちらも close 拒否・slash なし。**D5** vector の「最重 case」は
  実際には「terms 最大の最初の case」。
- **admission の A1〜A7 は b9d75a4e5 で本文が確定。** 新本文だけから ref2 を更新(free/aligned/mixed の分類、
  1 group = `⌈aligned / G⌉` + free 全額、`C_j = 0` の拒否名、`max_checkpoint_interval ≥ 1`、拒否の順序)し、
  切替スイッチ(旧 A1 の読み)は削除。**admission.json 29/29 を再現**、**×25 の 487,500 件で不一致 0**
  (拒否は limit 名と値まで完全一致、受理は全派生量一致)。**本文が全 case を決めている。**
- 評価器: golden 250/250、両 merge(phase-f・A1 fix)後の ×25 差分(primitive 100 万・step・cone 312 万・変異ほか)も
  不一致 0。改訂 1・2 の F1〜F15・N1〜N4 は解消済み。

## H dissection (§9.5, PALW-TIR-37, at c0d826a1b)

### What was built

`misaka-palw-tir-ref2/src/dissect.rs` and `demand::eval_range`, from §9.5 alone: which nodes reduce
over `H` and a commit tile's site (reductions in node order, folds, §7 bounds, counts at the
context's `H`, `h`, `h_tile`); `eval_range` — §9.4 with supplied nodes that are leaves in the
target's context and a target that reduces over `[from, to)` only, at `to − from` terms per element,
with the stated `Malformed` refusals; the root claim's finalize (every reduction but `n` supplied from
the totals; for `n = r_m` the totals themselves) and element closure (the finalize's supplied reads,
then the probes `eval_range(ctx, r_i, [e], S_i, (0, 1))` to a fixpoint, an unclaimed read refused);
admission steps 2 to 5; the cut at arity `k`, the round budget, the tiles' positions; the partials
against the ROOT's totals; exact `Sum` (256-bit) and `Max` folds; the bottom's first differing value
(`i` major, then `e`); obligations O-1 to O-3 and O-5's `V` and round bytes. The chain carriage
(§9.5.9) is Phase F's and is not modelled; a source stands for a carriage's evidence.

### Results

| test | result |
| --- | --- |
| golden vectors (`dissect/*.json`) | **32/32**: every case's site, element lists, honest totals, finalize (= the committed tile), first cut at arity 2 (tiles, positions, partials) and bottom (positions, partials); on the way the honest root claim is admitted, every round folds and the bottom finds no fault |
| the first implementation's full court, ×25 (`dissect_court`): a real step leg from its fail-closed builder over this crate's run, a real binding and evidence store; its root-claim builder and checker, round builder, phase and bottom builder and grader | **6,787 games, 0 disagreements**: 471 honest executors (challenger defeated at the bottom); 4,126 executors that forge a tile and lie about one total, in each reduction in turn, convicted by both at exactly the lie's value index; 1,045 root claims (unforged lies) and 1,145 rounds (lies that do not fold) refused by both at the same move; its built honest claims and rounds equal this crate's |
| the phase and `eval_demanded_range`, ×25 (`dissect_differential`): long-`H` attention layers (1–3 heads, windows to `2^18`, 6–40 positions, a committed max, a fourth reduction, two layers) and 1,500 generated programs with a dissected tile; responders honest, lying consistently (the lie in the last, first, a random child or spread), not folding, unforged, out of bound, with short, long or unsorted lists; arity 2–64 | attention: 2,348 sites, 22,610 games, 82,171 moves, 0 disagreements; generated: 7,422 sites, 45,980 games, 110,669 moves, 99 disagreements — all H1 |
| cone functions, ×25: reductions, obligations O-1/O-2 (the named node), `V` at three tile lengths, round bytes | 127,928 commit points: identical but 13 `V` values (H2); the corpus and the Qwen2.5-1.5B-shaped decoder identical |
| range requests: the target supplied, an index that is no node, a range on a node that does not reduce over `H`, `from ≥ to`, `to > H`, the whole history, one index | identical (`Malformed` ×6, the same values and work ×2) |

Run: `cargo test --release -p misaka-palw-tir-ref2 --test dissect_golden --test dissect_differential --test dissect_court`
(`TIR_REF2_CASES=25`: ≈ 1 min; the first build of `kaspa-consensus-core` takes ≈ 3 min).

### Findings (H-series)

| id | § | severity | first impl | ref2 | text supports |
| --- | --- | --- | --- | --- | --- |
| H1 | 9.5.3 steps 2, 5 | **honest executor loses** | refuses a root claim with an empty element list — the one its own builder makes for a tile that reads none of some reduction | admits it (the lists are exactly the closure) | ref2 |
| H2 | 9.5.6 O-5 | **admission split near the cap** | `V` below the formula and below the real closure (2 vs 5): a `MatMul` of the cone whose first operand is a leaf passes less than `d · K` to its second operand | the formula; equals the real closure in every case | ref2 |
| H3 | 9.5.6 O-3 | editorial | never reports it | never reports it | O-3 cannot fail: one `H` per shape (§2.2) makes every reduction's output `H`-free |
| H4 | 9.5.1, 10.3 | editorial | a layout needs `h_tile` a power of two in `[1, 4096]` and tiles of `[4, 2^16]` lanes | any `h_tile ≥ 1`, `tile_len ≥ 1` | 04b states `h_tile ≥ 1`; the layout rules are Phase F's |
| H5 | 9.5.2 | editorial | `Malformed` | `Malformed` | no class for a supplied set holding the target or a non-node |
| H6 | 9.5.6 O-5 | not implementable from 04b | — | not modelled | "fits the court window … ADR-0082 Z4" |

#### H1 — an empty element list (§9.5.3) — the first implementation departs; an honest executor loses

> step 2: "each `L_i` strictly ascending with every element `< E_i`" · step 5: "the element closure
> computed with `T` supplied is **exactly `L`**: no claimed element goes unread"

A tile can read none of a reduction of its cone: two maxima over the history, broadcast and
concatenated — a tile of the first half reads only the first (`h1_empty_list_probe`,
`h1_on_the_court`); or any `Concat`, `Slice` or `Gather` that routes some rows around a reduction
(99 cases among the generated programs at ×25). Step 5 then forces `L_i = ∅`, and step 2 allows it.
The first implementation's claim check refuses any empty list ("a reduction's elements are
ascending, distinct and inside it") — its own `build_tir_root_claim_v1` produces `[[0], []]` and its
own `check_tir_root_claim_v1` refuses it. The executor owes the root claim within its window and no
admissible claim exists, so an honest executor loses the dissection by silence. The corpus's
batched-head attention never meets it; a lowering that concatenates per-head or per-branch
reductions does. Fix: admit an empty list (the carriage cap and the exact-closure check already
bound the claim); a round's children then carry empty lists too, and the bottom compares nothing
for that reduction.

#### H2 — the value bound `V` (§9.5.6 O-5) — admission split

> "each computed node in descending index order passes `d · K` to each operand of a `MatMul` (`K`
> its first operand's last extent) … `V` is the sum over the reductions of what arrives"

For `r2 = ReduceMax(MatMul(Const[a, K], X[K, H]))` with `X` an `H`-local chain from an earlier
reduction `r1`, the text passes `d · K` to `X`, and `V` counts `K` elements of `r1`; the real
closure reads exactly those. The first implementation's `palw_tir_dissect_value_bound_v1` returns
less — 2 against a real closure of 5, 3 against 4 (`value_bound_against_closures`: in all 13 cases
ref2's `V` equals the largest real closure and the first implementation's is below it). `V` is what
admission sizes the value cap, the round and the root claim against, so near the 4096 cap the two
implementations admit differently, and a class the first admits can require root claims past its
own cap (step 2 refuses them: the honest executor loses as in H1). The corpus programs and the
Qwen2.5-1.5B-shaped decoder have the same `V` in both. Fix: pass `d · K` to both operands of a
`MatMul`, `K` from the first operand.

#### H3 — O-3 cannot fail (§9.5.6) — editorial

A shape holds at most one `H` (§2.2). A `ReduceSum`/`ReduceMax` over `H` keeps its axis as 1 and
has no other `H`; a `MatMul` contracting `H` has `H` only in `K` (both operands' only `H`), so its
batch, `M` and `N` are `H`-free. Every reduction over `H` therefore has an `H`-free output, and
neither implementation ever reported O-3 in 127,928 commit points. Keep it as a guard, or say that
it follows from §2.2.

#### H4 — `h_tile` and tile lengths (§9.5.1, §10.3) — editorial

04b gives `h_tile ≥ 1` and `1 ≤ tile_len ≤ 2^16`; the first implementation's commitment layout
refuses an `h_tile` that is not a power of two in `[1, 4096]` and commit or state tiles of fewer than
4 lanes. The dissection's arithmetic does not need either rule; they are Phase F's layout rules and
04b should cite them where it states the ranges.

#### H5 — a malformed supplied set (§9.5.2) — editorial

"`supplied` is a set of node indices of the target's block, none equal to `target`" states a
precondition without a class; both refuse the target supplied, or an index that is no node,
`Malformed` (as §9.2's environment malformations are). One clause would pin it.

#### H6 — the court window (§9.5.6 O-5) — not implementable from 04b

"the whole exchange over `⌈max_context / h_tile⌉` tiles at arity `k` fits the court window, by the
rule that sizes the network's arity (ADR-0082 Z4 …)" refers outside 04b; the rest of O-5 (`V ≤ 4096`,
the round and root-claim bytes) is implemented here, and `V` and the round bytes are compared.

#### Observation outside §9.5 — the binding's job context (Phase F API)

To drive the court, ref2 builds a job context with `palw_tir_canonical_context_v1(class, class_id,
(positions, 1))`, which sets `max_context_tokens = layout.max_context`. For a job that touches
exactly `layout.max_context` positions (`prefill + decode − 1`, the layout's own definition),
`PalwTirStepSpaceV1::job_shape` accepts it but `verify_tir_binding_v1` refuses the binding ("job
context shape") unless `max_context_tokens = positions + 1` (found by probing: `prefill + decode ≤
max_context_tokens`). Two functions of the first implementation disagree by one about whether the
longest job fits; worth settling before `palw_tir_v1` is armed.

## Demand evaluation (`eval_demanded`, 04b §9.4 at 23cf9235c)

### What was built

`misaka-palw-tir-ref2/src/demand.rs`, from §9.4 (and the rest of 04b) alone: contexts `(p, o)` with
`H = min(p + 1, W)` per block; the two targets; the request refusals before anything is read
(`Position` for `p ≥ history_bound`; `Malformed` for an occurrence, node, element, state or instance
that does not exist — every applicable class kept for comparison); the five source questions behind
a trait, with a recorder for the request set; leaves (commit points, except the target of a `node`
request in its context); every primitive's index map in the table's order (`Select` reads only the
chosen operand, `Gather` reads the data only after the `Index` check, `MatMul` interleaves `a` and
`b` per `t`, `TopK` evaluates a whole row once, `HistAppend` reads the input or `hist_row`); the
values of §6 element by element (the order-free sums, the exact-result rule, `Divisor`, the
transcendentals); `Fixed`-state replay (a supplied value checked against dtype and `[lo, hi]`;
`Replay` at 0 is `Missing`; the writer at `p − 1` as a leaf or computed and checked against
`[lo, hi]`; an instance nothing writes carried from the largest `q ≤ p` with a value); work — one
element per computed element (a `TopK` row once), `K` / `n` terms, and `p − q` per distinct need of a
carried value — charged when an element is first scanned, before its operands, stopping as soon as a
count exceeds its limit; and the frame cap `6 · max_terms + 24 · max_elements`. It runs on an explicit
stack of frames (no native recursion). `demand_outcomes` evaluates everything the request can reach
without stopping and returns the set of failures the text allows (every failing element's class,
plus `WorkLimit` when the work exceeds the limits) — the yardstick for comparing refusal classes.

The differential calls `misaka_palw_tir::demand::eval_demanded` through `validate` and a
`DemandSource` implemented over the same answer model (`tests/common/demsrc.rs`), so both see
exactly the same answers, and records the questions each one asks.

### Results

| test | result |
| --- | --- |
| golden vectors (`demand/*.json`, 7 files) | **672/672**: values, work (`elements`, `terms`), the grouped request set in the stated order, and every `expect_error` |
| random range-safe programs × targets × element sets, ×25 | 330,431 cases (205,939 both succeed), 0 disagreements |
| random programs without the range guarantee, ×25 | 180,040 cases (105,853 both succeed; `Overflow` 304, `Index` 53, `Divisor` 17 agreed), 0 disagreements |
| replay windows (states supplied every `k`, at random, at 0 only, or never at 0; 6–20 positions), ×25 | 261,322 cases (196,325 both succeed, 159,419 of them `state_after`), 0 disagreements |
| history rows through a committed node and through a carry-in (per layer), windows 1–`2^18` | 3,960 cases, 0 disagreements |
| edges (482 cases) and the text-gap probes (16): a replay from `history_bound − 1` down to 0 (1,048,576 elements through uncommitted writers; 524,286 carried terms), 1,500-position replays through committed and uncommitted writers, zero limits on zero work, 5,000 repeated elements at exact limits, the empty request, `Replay` at 0, 15 request refusals alone and together | 0 disagreements |
| hostile sources (a refusal, a value outside the dtype or `[lo, hi]`, `Replay` for a supplied value, a token at `token_bound`, an inconsistent or extreme value; 1–3 at once) | 124,068 of the cases above, 0 disagreements |
| work limits at the boundary: exactly the work succeeds, one element or one term short fails `WorkLimit` | 171,418 triples, 0 disagreements |
| totals ×25 | **776,251 cases, 0 disagreements**; 510,277 successes identical in values, work and request set; 265,974 failures each with a class the text allows (150 with the other of two allowed classes); every primitive covered as a successful target |

On "the ordered leaf requests": §9.4 makes the requests a **set** ("the order it asks in is not
part of the result"), and the golden vectors print it grouped and sorted; the sets, compared in that
sorted form, agree in every success. The order in which the questions are first asked also coincides in
497,007 of the 510,277 successes (97.4%); where it differs, it differs as the text allows.

The frame cap claim ("the cap is never reached while the work is within the limits") holds for
both: zero limits on a zero-work request (a supplied carried value, a committed writer's leaf)
succeed, and so do 5,000 repeats of two elements at exactly the work of two.

Run: `cargo test --release -p misaka-palw-tir-ref2 --test demand_golden --test demand_differential`;
`TIR_REF2_CASES=25` for the scale above (≈ 20 s).

### Findings (D-series) — decided by the text at c0d826a1b

The §9.4 text now decides all five (merged as afa519c0f): a source's refusal fails `Missing`
whatever reason it gives (D4 — the first implementation now reports `Missing` too), a `state_after`
leaf must lie in `[lo, hi]` (D1), a carried walk is charged one term per position it passes and `p`
when it finds no value (D2), an instance no run holds is `Malformed` (D3), and the vectors' boundary
case is "the first case with the most terms" (D5). ref2 follows; the 697 updated vectors (with the
`withhold` cases) reproduce, and the demand differential has 0 disagreements. The table below is the
record of what the text said at 23cf9235c.


| id | § | severity | first impl | ref2 | text supports |
| --- | --- | --- | --- | --- | --- |
| D1 | 9.4 replay / `state_after` | editorial | a committed writer's value in its dtype but outside `[lo, hi]` is returned as `state_after` and refused (`Operand`) when replayed | same | the letter, which checks `[lo, hi]` only in the replay |
| D2 | 9.4 work | editorial | a carried walk charges one term per position it passes, also when it never finds a value | same | undefined: terms are `p − q` for a `q` that may not exist |
| D3 | 9.4 request | editorial | `state_after` of an instance its `per_layer` has but no block references is asked of the source | same | the letter (no refusal); no run defines its value except completeness's zeros |
| D4 | 9.4 source | label only (court: which refusal) | a source's refusal fails the evaluation with **the source's own class** (`Operand`, `Position`, `Malformed` pass through) | `Missing` | ref2: "the evaluation then fails (class `Missing`)" |
| D5 | 9.4 golden vectors | editorial | the boundary cases use the first case with the most terms, not the heaviest | — | "the heaviest case at exactly its work" |

#### D1 — `[lo, hi]` for a committed writer's value (§9.4)

> replay: "its element `i` in context `(p − 1, that occurrence)`, a leaf if the writer is a commit
> point and computed otherwise, and in `[lo, hi]` (else `Operand`)" / "A `state_after(p, j, l)`
> target's element `i` is the writer's element `i` in context `(p, the writer's occurrence)` — a
> leaf if the writer is committed"

With a committed `StateWrite` of a state in `[-5, 5]` answering 100 at position 2 (in `i16`), both
implementations return 100 for `state_after(2, w)` ("what a checkpoint at 2 holds") and refuse the
replay at position 3 that reads the same leaf (`Operand`); a node reading that leaf as a `Node`
operand also gets 100 (the leaf rule checks only the dtype). In the court PALW-TIR-33 convicts such a
value before any evaluation (the `StateWrite`'s interval is inside `[lo, hi]`), so no verdict turns
on it; standalone, the evaluator can report a `Fixed` value outside its declared range. Fix: check
`[lo, hi]` for the `state_after` leaf too, or say that the replay's check is the only one.

#### D2 — the terms of a carried walk that finds no value (§9.4 "Work", "Charging")

> "the number of positions the value is carried across: `p − q` for the largest `q ≤ p` at which the
> source answers a value"

When the source answers `Replay` all the way down to position 0, there is no `q`; the evaluation
fails (`Missing`), but how many terms it has spent — and so whether a small `max_terms` makes it
`WorkLimit` first — is not defined. Both implementations charge one term per position as the walk
passes it: `state_after(9, u)` with nothing supplied is `WorkLimit` under `max_terms = 3` and
`Missing` under 9, in both. Success is unaffected (either way it fails). Fix: "a carried value is
charged one term for each position its walk passes".

#### D3 — an instance no block references (§9.4 "The request")

> "`state_after` names a state that is not `Fixed` or an instance its `per_layer` does not have (a
> layer for a global state; none, or a layer `≥ L`, for a per-layer one)"

A per-layer state read and written only by the block of layer 0 still "has" an instance at layer 1,
so `state_after(p, v, 1)` is not refused: nothing writes it, and its value is asked of the source
(both implementations: `Missing` from a source that knows nothing of it, the source's value
otherwise). No run holds that instance (§9.1's completeness would read it as zeros), the golden
vectors' `states` list only referenced instances, and the court's checkpoints never contain it.
Fix: refuse (`Malformed`) an instance no occurrence's block references, or define its value.

#### D4 — the class of a source refusal (§9.4 "The source") — the first implementation departs

> "Any answer may be a refusal; the evaluation then fails (class `Missing`) and never substitutes a
> value."

The first implementation's `DemandSource` returns `TirResult`, and `eval_demanded` passes the
error's class through: a source refusing with `Operand`, `Position` or `Malformed` makes the
evaluation fail with that class (probe `text_gap_probes`, D4). ref2 reports `Missing` for any
refusal, as written. Success versus failure is the same, so no verdict splits; but §9.3's court
rule maps `Missing` to `InputSetNotCanonical` and `Operand`/`Position` to `Unadjudicable`, so a court
source that refuses with another class changes which refusal the close gets (nobody is slashed
either way). Fix: map every source error to `Missing` in `eval_demanded`, or say in §9.4 that a
refusal carries the source's class.

#### D5 — "the heaviest case" of the golden vectors (§9.4 "Golden vectors")

The boundary cases (exactly the work, one element short, one term short) are built on the first
case with the most terms, which is not the heaviest by elements in five of seven files (e.g.
`gdn-k2-v4-grouped`: position 1's commit, 6,380 elements, while position 3's has 6,390; both 1,080
terms). The vectors are right; the sentence describing them is loose. Fix: "the first case with the
most terms".

## Admission (`tir_admit_v1`, 04b at ccd8f6aae; A1–A7 resolved by b9d75a4e5)

### After the fix (b9d75a4e5, merged as 564546f52)

The new §10.3 states the split rule whole, and ref2 now follows it from the text alone: every node of
the union of the closure's update cones is classified free / aligned / mixed in ascending order (a
closure state is aligned; a commit point — another member's committed `StateWrite` included — a
param, const, input or carry-in is free); the replay splits when every member has the same first
dimension `G > 1` and no node is mixed; one group pays `⌈aligned / G⌉` plus every free node whole,
each summed cone by cone. A `C_j` of 0 names `max_tile_macs` (if one group's MACs pass it) or else
`max_tile_transcendentals`, with that component's value; `max_checkpoint_interval ≥ 1` is an input
rule; refusals follow the stated order, so the differential now requires the same limit AND value
(no more "another broken ceiling"). The `Readings` switch that reproduced the old A1 behaviour is
removed: there is one reading.

| id | text now | status | evidence |
| --- | --- | --- | --- |
| A1 | the split rule stated whole (free / aligned / mixed; free nodes paid whole by every group) | **resolved** | `admission.json` `a1-*` cases reproduce; ×25: 32,746 split replays among 95,897 range-safe replays, all identical |
| A2 | refusals name the ceiling by field, in a stated order, so name and value are determined | **resolved** | every ×25 refusal (54,243) has the same limit and value in both |
| A3 | `C_j = 0` names the component past its cap; `max_checkpoint_interval ≥ 1` among the inputs | **resolved** | `a3-*` vectors; the zero cap is an input refusal in both |
| A4 | a committed `StateWrite`'s cone counts in both cone-work terms | **resolved** | `cone_work` identical in every admitted case |
| A5 | a cone's `whole` cost is defined (informative) | **resolved** | `whole` identical in every cone |
| A6 | a failed range obligation has the class of the rule it stands for (§9.3 row) | **resolved** | `a6-overflow`, `a6-index`, `a6-divisor` vectors |
| A7 | only written `Fixed` states have a `C_j`; `C` = the cap if none is written | **resolved** | `a1-a-member-of-another-width-a7-a-state-nobody-writes` |

| test | result |
| --- | --- |
| `admission.json` (29 cases) | **29/29**: every admitted case's checkpoint interval, cone work, per-position quantities, states (closure, groups, per-position cost, interval), cones (nodes, leaves in order, whole, tiles, tile, opened bytes, operands, `H` reductions, chunk) and every node's cost and interval; every refusal's kind, limit, value and cap, or class |
| random programs, legacy ceilings, ×25 | 37,500: 0 disagreements |
| random programs, random inputs, ×25 | 150,000: 22,336 admitted, 12,021 refused past a ceiling (all same limit and value), 0 disagreements |
| range-safe random programs, ×25 | 150,000: 102,882 admitted with every quantity identical, 37,270 refused past a ceiling (all same limit and value), 0 disagreements |
| mutations, ×25 | 150,000: 0 disagreements (program refusals: same class, or another class of a rule the program breaks) |

The findings below are the record of what the text said at ccd8f6aae.


### What was built

`misaka-palw-tir-ref2/src/admit.rs`, from §10.3 in its stated order: the inputs' ranges (`tile_len`,
`h_chunk`); decoding, normal form and types (§4.4, §5, §6); the §7 intervals, exact in 256-bit
integers and checked against each obligation; the §8 node costs at `H = W` and the per-position
cost (over the occurrences), state bytes (by instance), peak live bytes, commit lanes and step
leaves; every commit point's cone (§10.2) with its leaves, whole cost, tiles, a tile's box demand
(descending pass, demands capped, the `HistAppend` prior-row demand, committed leaves at 4 bytes a
lane), its operands and — for a cone that reduces over `H` — the chunk at `H = min(h_chunk, W)`;
the cone work over every commit point's cone and every `StateWrite`'s update cone, its cap checked
before a cone is costed; the replay closures (a fixpoint over the states the update cones read), the
alignment rules as a free / aligned / not-aligned lattice, `G`, one group's cost `⌈c / G⌉`, `C_j`
and `C`. `graph_ir_root` (§3.6) with the BLAKE2b written for `prim_set_id`.

### Results

| test | result |
| --- | --- |
| golden vectors | 250/250: 138 primitive (incl. `error_arity_*`), 87 program (34 steps, 15 cones, 38 refusals), 18 encoding, 7 `graph_ir_root` (and the one printed in §3.6) |
| corpus programs, legacy ceilings | dense GQA, sliding + global, GDN, Mamba-2, MoE admitted by both with every exposed quantity identical; `fixed-state-saturation` and `hist-window` refused by both (`Overflow`, §7) |
| Qwen2.5-1.5B-shaped decoder (this crate's builder, 3,374 bytes) | admitted by both, identical (10 cones, cone work 307, `C` = cap); under 8 settings of `tile_len`/`h_chunk` and a variant with few commit points, the same verdicts, limits and values (e.g. both refuse `tile_len` 4096 at 100,663,296 tile MACs) |
| one ceiling at a time (10 ceilings × caps 0, 1, 1000 × 6 programs) | identical verdict, limit and value |
| random programs, ×25 | 37,500 under the legacy ceilings and 150,000 under random inputs: 0 disagreements beyond A1/A3 |
| range-safe random programs, ×25 (the generator keeps only nodes whose §7 obligations hold) | 150,000: 103,491 admitted by both with every quantity compared, 42,050 refused past a ceiling (same limit and value, or A3), 4,459 input refusals; 0 disagreements beyond A1/A3 |
| mutations of range-safe programs, ×25 | 150,000: 0 disagreements beyond A1/A3; a program refusal's class is always the class of a rule the program breaks |
| coverage of the admitted programs (×25) | 1.23 M cones, 40,346 dissected, 87,901 with a history leaf, 95,897 `Fixed` replays, 7,196 closures of ≥ 2 states, 6,699 replays split into groups, 3,456 `C_j` below the cap |
| probes | the group rule on hand-built update cones; a node shared by two update cones (costed once per cone by both — the literal text); a state written by two layer blocks (both report the smaller `C_j`'s replay); the replay refusal's value |

Run: `cargo test --release -p misaka-palw-tir-ref2 --test admit_differential`; `TIR_REF2_CASES=25`
for the scale above (≈ 30 s).

### Findings

| id | § | severity | first impl | ref2 | text supports |
| --- | --- | --- | --- | --- | --- |
| A1 | 10.3 groups | would-split-consensus | a replay splits only if every member's `StateWrite` is aligned (never free), following a commit point that is itself a node of the closure's update cones | free nodes never block a split, wherever they sit; a commit point is always a free leaf | ref2 (uniform); the first implementation's rule is not in the text |
| A2 | 10.3 refusals | editorial | "tile MACs", "position MACs", … "one position's state replay (MACs or transcendentals)" | the ceiling names (`max_tile_macs`, …), `state_replay` | neither — the text names no limit |
| A3 | 10.3 `C_j = 0` | editorial | reports the replay's MACs whenever there are any (also against a transcendental cap); `max_checkpoint_interval = 0` accepted as an input | the component past its cap (0 for a zero interval cap); same input rule | ref2 for the value ("names … the value and the cap"); neither for the zero cap |
| A4 | 10.3 cone work | editorial (resolved) | a committed `StateWrite` counted twice (commit cone + update cone) | first read "counted once", then aligned to the literal Σ | the first implementation (literal) |
| A5 | 10.3 outputs | editorial | `ConeV1.whole` = Σ §8 node costs of the cone at `H = W` | same | undefined |
| A6 | 7, 10.3 step 2 | editorial | a broken obligation → `Overflow` (⊆ dtype), `Index` (Gather), `Divisor` (Div) | same | unstated (§9.3's classes of the rules the obligations stand for) |
| A7 | 10.3 step 5 | editorial | a `Fixed` state no block writes has no `C_j` and is not listed | same | "derives every `Fixed` state's checkpoint interval" |

#### A1 — which replays split into groups (§10.3) — would-split-consensus

> "The replay is **split into `G` groups** when every member's shape has the same first dimension
> `G > 1` and every node of the closure's update cones is *aligned* — its output's axis 0 has extent
> `G`, and element `[g, …]` depends on the closure's states only through their elements `[g, …]` —
> by these rules, with a leaf other than a closure state *free*: a node whose operands are all free
> is free; …"

The rules classify nodes as free, aligned or not aligned, and then require "every node … is
aligned". Both implementations accept free nodes inside an update cone (a replay of
`S ← S + Clamp(P)` splits in both). They part when a whole update is free: `S ← Clamp(P)`, or a
member `m` of the closure whose update reads only commit points. **ref2** reads the rule uniformly —
a free node is aligned in the text's own definition (its element `[g, …]` depends on no closure
state) and never blocks, wherever it sits — and a commit point is always a free leaf ("a leaf other
than a closure state"). **The first implementation**, observed exactly (a switch in ref2,
`Readings { free_update_splits: false }`, reproduces every one of its answers at ×25): every
member's `StateWrite` must be aligned, not free, and a commit point that is itself a node of the
closure's update cones (another member's `StateWrite`) is followed as that node. Its rule is also
internally uneven: the same free update cone splits as part of one state's closure and not in its
own. Consequence: `groups`, the per-position replay cost `⌈c / G⌉` and `C_j` differ (30,319 cases
at ×25) and so do `C` and the class's checkpoint layout; where `C_j` drops to 0 under one reading
and not the other, one implementation admits what the other refuses (22 cases). Fix: state the
rule for free nodes and for commit points inside a closure in one sentence — for example "a free
node, a free `StateWrite` included, counts as aligned; a commit point is a free leaf even when it
is a member's `StateWrite`" (ref2's reading), or the first implementation's two conditions.

#### A2 — refusal names (§10.3) — editorial

> "A refusal past a ceiling names the limit, where, the value and the cap"

No names are given; the implementations use different strings for every ceiling (the differential
maps them). If a refusal is ever recorded or compared across implementations (a registration
receipt, an RPC error), the names should be fixed in the text, e.g. the ceilings' own names.

#### A3 — a `C_j` of 0 (§10.3) — editorial

> "Over the blocks that write `j`, the smallest `C_j` counts; a `C_j` of 0 is a refusal."

Which value such a refusal reports is not said. When a replay's transcendentals break the tile
ceiling and its MACs do not, the first implementation reports the MACs against the transcendental
cap; ref2 reports the component past its cap (47 cases at ×25 differ only in this value). And a
`max_checkpoint_interval` of 0 is not listed as out of range, yet makes every program with a
written `Fixed` state a refusal (both accept it as an input and refuse the program). Fix: define the
value (the component past its cap) and require `max_checkpoint_interval ≥ 1` among the inputs.

#### A4 — the cone work of a committed `StateWrite` (§10.3) — editorial, resolved

> "the **cone work** — `Σ`, over every commit point's cone (§10.2) and every `StateWrite`'s update
> cone (below), …" / "the cone (§10.2) of its `StateWrite` node (counted once in the cone work)"

A committed `StateWrite` is both a commit point and a `StateWrite`, and its two cones are the same
set of nodes. ref2 first read "counted once" as "once in all"; the literal Σ over the two families
counts it twice, which is what the first implementation does, and ref2 now does too. Fix: "a
committed `StateWrite`'s cone counts in both terms".

#### A5 — `ConeV1.whole` (§10.3) — editorial

The public result carries a cone's whole cost, which the text does not define. Both compute Σ of
§8's node costs over the cone's nodes at `H = W`. Fix: one sentence, or drop it from the result.

#### A6 — the class of a range refusal (§7, §10.3 step 2) — editorial

§10.3 step 1 names the §9.3 classes; step 2 names none. Both refuse a broken "⊆ out dtype" as
`Overflow`, a `Gather` index obligation as `Index` and a divisor obligation as `Divisor` — the
classes of the evaluation rules the obligations stand for. Fix: add a row to §9.3's table.

#### A7 — a `Fixed` state no block writes (§10.3 step 5) — editorial

> "derives every `Fixed` state's checkpoint interval (below)"

A state that is read but never written needs no replay; both leave it out of the result and out of
`C = min_j C_j`. Fix: "every written `Fixed` state".

### Revision 2's open items N1–N4: resolved

| id | text now | evidence |
| --- | --- | --- |
| N1 | §9.2: unread carry-in, `Fixed` and history entries are ignored, whatever their key, kind or type | `probes::p12` asserts both ignore all seven cases |
| N2 | §9.3: outside a program, an arity error is `Shape` | the new `error_arity_*` primitive vectors reproduce (138/138) |
| N3 | §9.2: every row checks values | `probes::p10` |
| N4 | §3.6: `graph_ir_root = BLAKE2b-512(key "misaka-palw/tir/graph-ir-root/v1", encode(program))` | computed from the definition; all 7 vector roots and the value printed in §3.6 reproduce |

## Revision 2 (a2e6ec35a): status of F1–F15

| id | revision 2's text | status | evidence (both implementations, ×25 run unless noted) |
| --- | --- | --- | --- |
| F1 | §9.2 table: an absent `Fixed` value is `Missing`, "never implied" | **resolved** | vectors `refusals[]` "state j's value is absent"; B3 "drop a needed Fixed value" 446/446 `Missing`; `probes::p03`, `p09` |
| F2 | §9.2: an environment that supplies `target` is `Malformed`; the target is always evaluated | **resolved** | vectors "the target is supplied"; B3 4,250/4,250 `Malformed` |
| F3 | §9.2 table: an absent history is `Missing` even when zero rows are needed | **resolved** | vectors "history absent at position 0"; B3 304/304 (0 rows) and 294/294 (≥ 1 row) `Missing` |
| F4 | §9.2: an entry at an index that is no node is `Malformed`; unreached entries are ignored | **resolved** | vectors "a supplied index that is no node"; B3 4,206/4,206 `Malformed`, 3,441/3,441 unreached wrong-typed entries ignored |
| F5 | NF-19: no two nodes of one step write one state instance; `post` writes no state | **resolved** | `encoding.json` NF-19 cases; `limits` "pre and post write one global state" `NormalForm`; B2 4,873/4,873 programs with a post write refused (`NormalForm`) |
| F6 | same rule | **resolved** | `limits` "a global Hist appended by pre and post", `probes::p02` (windows 4, 2, 1) refused by both |
| F7 | §9.2: `Position` before evaluation; the token checked when the closure reads it | **resolved** | B3 `pos = history_bound` 4,195/4,195 `Position`; token absent 2,028/2,028 `Missing`, at `token_bound` 996/996 `Operand`, absent and unread 6,400/6,400 ok |
| F8 | §9.2 table: carry-ins checked (dtype, shape, values) | **resolved** | B3 "a carry-in of the wrong shape" 663/663 `Operand`; `probes::p03`, `p10` |
| F9 | §6.5 *Type* clause | **resolved** | text matches both implementations; `probes::p06` |
| F10 | §9.1(1): "needed" = some node of the program reads `Input(0)`; checked before any node | **resolved** | B: every failing step now reports the same class (82,487/82,487; under revision 1, 1,743 differed) |
| F11 | §9.1 "Run-state completeness" | **resolved** | ref2 now reads an absent instance as zeros / empty; `probes::p05`, `p09` (an omitted history at pos 2 is `Position` in both) |
| F12 | §2.2 caps sentence rewritten | **resolved** | `probes::p07`, `limits` |
| F13 | NF-2 `2 ≤ \|blocks\|` | **resolved** | `encoding.json` `one_block`; `limits` |
| F14 | §6.0 defines `PRIM_SET_ID_V1` (BLAKE2b-512 keyed by `misaka-palw/tir-prim-set-id/v1` over the rev2 descriptor); NF-1 requires it | **resolved** | ref2 computes it with its own BLAKE2b (RFC 7693 and KAT vectors pass) and obtains exactly the printed constant; `limits`: 0xFF, zero, one flipped bit and the rev1 descriptor's id are refused by both |
| F15 | §9.3 table: one class per rule; several broken rules → the class of one of them | **resolved** | golden vectors' classes all reproduced; differential: the first implementation's class is always the class of a rule the input breaks (checked against ref2's violation sets, 0 exceptions); classes differ only on multi-rule inputs (C 3,636, D 3,620 of 360,154 / 178,519 failures) |

## New after revision 2 (resolved by ccd8f6aae, see "Revision 2's open items N1–N4" above)

All four are places where the two implementations agree but the text is silent or inconsistent.

### N1 — environment entries the closure never reads (§9.2) — editorial

> "Entries for nodes of `block` that the closure does not reach are ignored, whatever their type."

The sentence covers `supplied` only. An environment may also carry `fixed`, `hist_prior` and
`carry_in` entries the closure does not read — for a state or carry the block has, for an index that
names no state or no carry-in, or keyed by a state of the other kind (a `Fixed` entry for a `Hist`
state). Both implementations ignore all of them (`probes::p12`, seven cases, and a wrong-length
history under a supplied node). Since revision 2 refuses an index that is no node (`Malformed`), a
reader could expect the same of a `fixed` key that is no state. Fix: "Entries of `carry_in`, `fixed`
and `hist_prior` that the closure does not read are ignored, whatever their key, kind or type."

### N2 — the class of an arity error outside a program (§9.3, §12) — editorial

§9.3 files arity under NF-14 (`NormalForm`), a rule about a node of a program. The primitive vectors
(§12) evaluate a primitive alone; a `Concat` of 9 or an `Add` of 1 there is refused `Shape` by both
implementations (`probes::p12`, `p06`). No vector pins it. Fix: one sentence in §12 — "outside a
program, a wrong number of inputs is a type-rule failure (`Shape`)" — or the reverse.

### N3 — "values" in the §9.2 table (§9.2, §9.3) — editorial

§9.2's table checks a supplied node and a param for "declared dtype and shape", a carry-in for
"dtype, shape and values"; §9.3 files "a param, carry-in, `Fixed` value …, history row or supplied
node of the wrong dtype, shape or values" as `Operand`. Both implementations check values (inside the
dtype) for all of them (`probes::p10`: a param `i8` holding 200, a supplied node or a carry-in
outside `i32`, a supplied node with too few elements — all `Operand`). Fix: "values" in every row.

### N4 — the identity hash (§3.6) — ambiguity; not implementable from 04b

> "`graph_ir_root = H(encode(program))` for the network's keyed hash."

Revision 2 names the hash and key of `prim_set_id` (F14) but not of `graph_ir_root`: neither the key
nor the domain string is in 04b, so a second implementation cannot compute a class's IR root from the
text. It is not needed to evaluate a program, but it is the identity the class id commits to. Fix:
state it as §6.0 does for `prim_set_id` (algorithm, output length, key bytes).

## Revision 2: what was built and how it was tested

From the revised text only: the §9.2 cone in its stated order (request and environment → `Malformed`;
then `Position`; then the token; then every value the closure reads, checked before any node is
evaluated), run-state completeness in steps, NF-1's `PRIM_SET_ID_V1` (own BLAKE2b), NF-2, NF-19,
the class `Malformed` and the §9.3 class of every rule. To test §9.3's "the class of one of them"
exactly, normal form now reports **every** rule a program breaks (`normal_form::violations`), and a
step every rule it breaks (`eval::step_violations`: every node whose operands can be computed, a
failure poisoning its consumers); the differential requires the first implementation's class to be
in that set.

| test | revision 2 result |
| --- | --- |
| golden | **239/239**: primitive 134/134, program 87/87 (34 steps, 15 cones, 38 refusals), encoding 18/18 — classes included |
| A | 1,000,000 primitive cases: 737,689 both ok and identical, 262,311 both fail with the same class; 0 disagreements |
| A2 | 19 maximal accumulations: ref2 = text = first |
| B | 37,500 programs, 149,913 steps (67,426 both ok, 82,487 both fail — same class in all), run states identical, 1,558,860 court-env + 1,558,860 random-subset cones identical and equal to the step's value; 0 disagreements |
| B2 | 7,500 programs generated with state writes in `post`: 4,873 refused by both (`NormalForm`), the other 2,627 run (10,582 steps); 0 disagreements |
| B3 | 18 single defects of an honest court environment, 36,025 cones: ref2 = text = first on every one |
| C | 398,373 byte mutations: 0 disagreements; every class of the first implementation is the class of a rule the bytes break |
| D | 225,000 structural mutations + 139,443 steps of the mutants both accept: 0 disagreements, same class condition |
| limits | 105 limit cases + 8 UTF-8 cases + the 262,144-byte cap: ref2 = text = first; classes within the broken rules |
| probes | 12 probes, each asserting revision 2's verdict and class on both |

---

## Revision 1 (history)

The findings below were filed against revision 1 (b7492d601) and are all resolved by revision 2 (see
the status table above); they are kept as filed.

### Revision 1: what was built and how it was tested

`misaka-palw-tir-ref2` (test/verification only, never a consensus dependency) implements from 04b
alone: the §2 types and `H`, §2.3 broadcasting, the §4 encoding (a hand-written reader and writer —
no derive — with the size cap, strict tags/bools/UTF-8, no trailing byte and the re-encoding
identity), NF-1..NF-22, the §6 type rules symbolic in `H`, all 25 primitives, `StateWrite` and
`HistAppend` with the window, the §9.1 step and run, and §9.2 cones. Structural choices differ from
the obvious ones on purpose: every exact result is computed as a 256-bit signed integer and only then
range-checked (§6.1); every `>>` of §6.5 is floor division written from truncating division plus a
correction (no shift operator); the §6.5 constants are re-declared, not imported; index maps unravel
each output position into its multi-index by the §0 definition; a step is a pure function of the run
state, effects applied to a copy only after every node succeeded. §7 (ranges), §8 (costs) and §10
(court feasibility) are Gate 2 analyses and were not implemented.

| test | what | result |
| --- | --- | --- |
| `tests/golden.rs` | every vector of `consensus-vectors/tir-v1/` | 134/134 primitive cases, 49/49 program steps and cones, 12/12 encoding cases; no error-class disagreement |
| `tests/differential.rs` A | random single primitives, range extremes, exact halves, IntExp bucket edges, ties, perturbed output types | 1,000,000 cases (×25 run): 737,689 both ok and identical, 262,311 both fail, **0 disagreements**, classes identical |
| A2 | maximal accumulations: `ReduceSum`/`MatMul` exactly at and one past each dtype's ends (contractions up to K = 2^17, totals that fit while a partial sum does not, `idx` outputs) | 19 cases, ref2 = text on all, first = ref2 on all |
| B | random well-formed programs from ref2's own generator (histories of windows 1, 2, 3, 5, `history_bound`; attention with a `MatMul` contracting `H`; per-layer and global states and params) | 37,500 programs, 149,693 steps (72,196 both ok, 77,497 both fail), run states identical after every step, 1,690,652 court-env cones + 1,690,652 random-subset cones identical and equal to the step's value; **0 disagreements** |
| B2 | as B with a global state written by pre and post | 7,500 programs, 29,774 steps, 342,013 cones; 0 disagreements (see F5) |
| B3 | honest court envs with one perturbation each | table under F1–F4 |
| C | random byte mutations of valid encodings | 398,269 cases, 0 disagreements on accept/reject; the 48,940 accepted by both re-encode to the input in both |
| D | structural mutations of every field class, re-encoded; steps of mutants both accept | 225,000 decodes + 131,664 steps, 0 disagreements |
| `tests/limits.rs` | every §4.4/§5 limit at and past its edge, UTF-8 edges | 98 + 8 cases and the 262,144/262,145-byte cap: ref2 = text on all, first = ref2 on all |
| `tests/probes.rs` | the silent corners, one by one; the largest run-time `H` (window `history_bound` at pos `history_bound − 1`) | evidence of F1–F8, F11; the largest `H` agrees |
| `tests/claims.rs` | the numeric statements of §6.5, exhaustively | all hold (see "Verified claims") |

Run: `CARGO_TARGET_DIR=… cargo test --release -p misaka-palw-tir-ref2`; `TIR_REF2_CASES=25` scales the
differential (≈2 min on this machine); `--ignored` adds the exhaustive §6.5 sweeps (≈17 s).

### Revision 1 findings

Severity: **would-split-consensus** — two implementations that each follow a defensible reading
give different success/failure or values on the same input; **ambiguity** — the text allows two
readings (the implementations happen to agree, or differ only on inputs no conforming caller
builds); **editorial** — the intent is clear but the text is incomplete or inconsistent.

| id | § | severity | first impl | ref2 |
| --- | --- | --- | --- | --- |
| F1 | 9.2 | would-split-consensus (first vs text) | a `Fixed` value missing from the env is taken as zeros | `Missing` |
| F2 | 9.2 | would-split-consensus | a supplied target is returned as supplied | the target is recomputed |
| F3 | 9.2 | would-split-consensus | an absent history that needs 0 rows is empty | `Missing` |
| F4 | 9.2 | ambiguity | a supplied index that is no node is ignored | refused (`Operand`) |
| F5 | 3.4, NF-19 | ambiguity | two `StateWrite`s of one global state in a step: post wins | same |
| F6 | 3.4, 6.7, NF-19 | ambiguity (admission gap) | two `HistAppend`s of one global history per step admitted; runs break | same |
| F7 | 9.2 | editorial | cones enforce `pos < history_bound`, `token < token_bound` | same |
| F8 | 9.2 | editorial | cones check carry-in dtype/shape | same |
| F9 | 6.5 | editorial | transcendentals: `out.shape = x.shape`, any out dtype | same |
| F10 | 9.1(1) | editorial | "the token is needed" = checked when a node reads it | checked up front if any node reads it |
| F11 | 9.1 | editorial | run-state instances materialised lazily | materialised at the initial state |
| F12 | 2.2, NF-8 | editorial | per-tensor caps as ref2 | — |
| F13 | NF-2, NF-3 | editorial | — | — |
| F14 | 3.1, 3.6, 6.0 | ambiguity | any `prim_set_id` admitted | same; cannot compute it |
| F15 | 0, 5, 12 | editorial | class mapping differs from ref2 | — |

#### B3 under revision 1: honest court environments with one perturbation (×10 run)

| perturbation | 04b | cases | both ok | both fail | ref2 ok / first fails | ref2 fails / first ok |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| drop a needed carry-in | fails (`Missing`) | 526 | 0 | 526 | 0 | 0 |
| drop a needed `Fixed` value | fails (`Missing`) | 297 | 0 | 0 | 0 | **297** |
| drop a needed history of 0 rows (pos 0 or window 1) | fails (`Missing`) | 157 | 0 | 0 | 0 | **157** |
| drop a needed history of ≥ 1 row | fails (`Missing`) | 146 | 0 | 146 | 0 | 0 |
| supply the target with a wrong value | silent | 3,262 | 0 (values differ in all) | 0 | **3,262 value differs** | 0 |
| supply an index that is no node | silent | 3,438 | 0 | 0 | 0 | **3,438** |
| a wrong-shaped supplied node inside the closure | fails | 1,903 | 0 | 1,903 | 0 | 0 |
| a wrong-shaped supplied node outside the closure | silent | 2,843 | 2,843 | 0 | 0 | 0 |
| `pos = history_bound` | silent | 3,239 | 0 | 3,239 | 0 | 0 |
| one history row too many | fails | 346 | 0 | 346 | 0 | 0 |
| a `Fixed` value outside `[lo, hi]` | fails (§9.1(2)) | 251 | 0 | 251 | 0 | 0 |

#### F1 — a `Fixed` value missing from a cone's env (§9.2) — would-split-consensus

> "`env` gives: the token and `pos`; the carry-in values; the `Fixed` values at the start of the
> position; … A value the closure needs and nobody supplied fails (class `Missing`)."

The text is explicit. Readings: (a) the closure reads `State(j)` and `env.fixed` has no entry → fail
`Missing`; (b) an absent entry means the initial value (all zeros). **The first implementation takes
(b)** at every position: `probes::p09` at pos 4 returns `[8, −8]` where the honest value (state
`[3, −3]`) is `[11, −11]`; B3 shows 297/297 such cones succeed with zeros. **ref2 takes (a)**, as the
text says. Consequence: a court (or any caller) that omits a state value recomputes the disputed tile
with zeros and convicts the honest party instead of failing; two implementations disagree on
success/failure. The same leniency applies to a step's run state (`probes::p05`, "fixed missing"),
where it is harmless from the initial state (F11). Fix: the first implementation, or — if the zero
default is wanted — the text: "a `Fixed` value absent from `env` is an error (class `Missing`); the
initial all-zero value is never implied".

#### F2 — the target itself supplied (§9.2) — would-split-consensus

> "The evaluator computes the backward closure of `target` that stops at every supplied node (a
> supplied node's value is taken, not recomputed, …)"

Readings: (a) the closure of `target` starts at `target`; if `target` is supplied it is a supplied
node, so its value is taken and returned; (b) `eval_cone` "evaluates one node" — the target is always
recomputed and only nodes strictly before it can be supplied. **The first implementation takes (a)**
(`probes::p03` "target supplied" returns the supplied `[99, 99]`; B3: 3,262/3,262 wrong supplied
targets echoed). **ref2 takes (b)**: the court's purpose is to recompute the disputed value, and a
court that builds `supplied` from all committed values of the occurrence (the disputed one included)
would, under (a), "recompute" exactly the claim under dispute. Fix: "`target` is always evaluated;
an entry of `supplied` for `target` itself is ignored" (or "is an error").

#### F3 — an absent history that needs zero rows (§9.2) — would-split-consensus

> "for each `Hist` state the prior rows (exactly `min(pos, window − 1)`, oldest first)"

When `min(pos, window − 1) = 0` (pos 0, or window 1 at any pos), is an env without an entry for the
state a supply of zero rows or a missing value? **The first implementation treats an absent entry
as the empty list** (157/157 in B3; with ≥ 1 row needed both fail); **ref2 requires the entry**
(`Missing`), reading "env gives … the prior rows" literally. A court that omits empty histories
splits the two. Fix (either, written down): "an absent entry is the empty list; the number of rows
MUST be exactly `min(pos, window − 1)`" (recommended: it removes a pointless failure mode), or "an
entry MUST be present for every history the closure appends to, possibly empty".

#### F4 — a supplied index that is no node of the block (§9.2) — ambiguity

> "and **supplied** values for any nodes of the occurrence."

An entry keyed by an index ≥ |nodes| is outside that domain. **First: ignored** (3,438/3,438);
**ref2: refused** (`Operand`), on the reading that a malformed env is an error. No conforming court
builds such an entry, so this splits only on malformed inputs. Fix: say which ("entries for indices
that are not nodes of the block, or that the closure does not reach, are ignored" is the lenient
form; both implementations already ignore unreached entries, even wrong-shaped ones — 2,843/2,843).

#### F5 — one global `Fixed` state written by pre and by post (§3.4, NF-19) — ambiguity

> "At most one `StateWrite` per state per block; its output … becomes the state's value for the next
> position."

NF-19 is per block, so pre and post may both write the same global state; the text does not say
which write becomes the next value. Both implementations admit it and **the later occurrence (post)
wins**, and both read the start-of-position value in post (`probes::p01`; B2: 29,774 steps, run
states identical). A backend applying effects in another order would split. Fix: either forbid it
("a state is written by at most one block": for a global state, by pre or post, not both) or state
"effects apply in occurrence order; the last write wins".

#### F6 — one global `Hist` appended by pre and by post (§3.4, §6.7, NF-19) — ambiguity (admission gap)

> "Exactly the `HistAppend` node of the block appends to it (at most one per state per block); its
> output is the last `H = min(pos + 1, window)` rows, oldest first, this position's row last."

With two appenders there are two rows per position, and "the rows appended at the previous
`min(pos, window − 1)` positions" (§6.7) is undefined. Both implementations admit such a program
(`limits`: "a global Hist appended by pre and post"), and at run time (`probes::p02`): window ≥ 3 —
every step at pos ≥ 1 fails in both (the stored rows no longer match `H`); window 2 — runs, and pre's
row is silently dropped (post's row is the only prior row); window 1 — harmless. An admitted program
that cannot pass position 1 whatever its inputs is a normal-form gap. Fix: add to NF-19 "and each
state is written or appended by at most one block" (per layer instance this already holds).

#### F7 — position and token bounds in cones (§9.2) — editorial

§9.2 cites "§6 and §9.1(2)", not §9.1(1). Both implementations nevertheless refuse a cone at
`pos ≥ history_bound` (`Position`, 3,239/3,239) and a token `≥ token_bound` that the closure reads
(`Operand`). The first implementation's env also allows *no* token (`Option`), which is fine when
the closure does not read `Input(0)` and `Missing` when it does. Fix: "§9.1(1) applies to a cone,
with 'needed' meaning read by the closure".

#### F8 — carry-in values in cones (§9.2) — editorial

Only supplied node values are said to be checked. Both implementations check carry-ins (dtype, shape,
values in the dtype) and fail `Operand` (`probes::p03`, `p10`). Fix: "every carry-in, `Fixed` value
and history row the closure reads is checked against its declaration as in §9.1(2)".

#### F9 — the type rule of `IntExp`, `IntRsqrt`, `IntLn` (§6.5) — editorial

> "All three read and write **Q24**; their inputs MUST NOT be `i128` …; their outputs obey the
> exact-result rule."

No shape rule and no output-dtype rule are stated (every other primitive has a *Type:* clause).
Both implementations use `out.shape = x.shape`, any output dtype, input any dtype but `i128`
(`probes::p06`: a changed shape is refused by both). Fix: add "*Type:* 1 input, not `i128`;
`out.shape = x.shape`; any `out.dtype`".

#### F10 — "if the token is needed" (§9.1(1)) — editorial

Undefined. The first implementation checks the token when a node reads `Input(0)` (so a step whose
earlier node overflows reports `Overflow`, although the text's order — step 1 before step 2 — implies
`Operand`); ref2 checks up front whenever any node of the program reads it. Success versus failure is identical, because a step evaluates every node (1,743 steps of
B differ only in the reported class). Fix: "if some node of the program reads `Input(0)`".

#### F11 — completeness of the run state (§9.1) — editorial

> "Input: the program, the params, the run state `(pos, Fixed values, Hist rows)`, and `token`."

The text's run state holds every instance from the initial state on. The first implementation's
API accepts a run state without instances and treats absence as the initial value (zeros / empty);
ref2 materialises every instance at pos 0 and treats absence as `Missing`. From the initial state
the runs are identical (B: 72,196 run states equal after every step); they differ only on a
hand-made incomplete state (`probes::p05`, `p09`). Fix: say whether an implementation's run state
may omit never-written instances (absent = initial value) — and that this convenience does not carry
over to a cone's env, where the value is opened from a commitment (F1).

#### F12 — the element caps (§2.2, NF-8) — editorial

> "Every tensor's element count at the worst case MUST be `≤ 2^28`." / "consts and states have
> `≤ 2^28` elements, params `≤ 2^40`"

"Every tensor" contradicts NF-8 and §14 deviation 7 for params; and for a `Hist` state it is not said
whether "state" means the row or the window. Both implementations cap the row by NF-8 and the window
through the `HistAppend` output `[W] ++ row` under §2.2 (row 1,024 × window 2^18 accepted, 1,025
refused, `probes::p07`). Fix: "every node output …"; "a `Hist` row has ≤ 2^28 elements; its window
is capped by its `HistAppend` output".

#### F13 — the block count (NF-2, NF-3) — editorial

`1 ≤ |blocks|` (NF-2) is unreachable: NF-3 requires `pre ≠ post`. Fix: `2 ≤ |blocks| ≤ 16`.

#### F14 — `prim_set_id` (§3.1, §3.6, §6.0) — ambiguity; not implementable from the text

> "`prim_set_id`: … the network's hash of the prim-set descriptor (§6.0)"; "`graph_ir_root =
> H(encode(program))` for the network's keyed hash."

04b gives the descriptor string but not the hash function (nor its key), so a second implementation
cannot compute `prim_set_id` from 04b; and no rule of §4.4 or §5 compares `prim_set_id` with anything.
Both implementations admit any 64 bytes (`limits`: "prim_set_id all 0xFF"). A program can therefore
claim any prim set and still be admitted, and its `graph_ir_root` varies with a field that carries
no semantics. Fix: name the hash and key; add "`prim_set_id` = the network's value" to NF-1 or state
that `tir_admit_v1` checks it.

#### F15 — error classes (§0, §5, §12) — editorial

Only success versus failure is normative (§0, PALW-TIR-34), yet `encoding.json` pins classes and
no table maps each rule to a class. Where the implementations report different classes (always both
failing): an input added to or removed from a node — first `Shape`, ref2 `NormalForm` (NF-14 arity);
a node whose declared type was altered (dtype, a dimension, an added `H`, the element cap) — often
first `NormalForm`, ref2 `Shape` (the order in which the NF-16 type check and the NF-17/NF-22 checks
run); an out-of-range token — first the class of whichever node fails first, ref2 `Operand`.
In the ×25 run: 11,343 + 3,490 (D), 331 + 3,648 (C), 1,743 (B) class differences, none on success
versus failure. Fix: a rule-to-class table, or a sentence in §12 that the classes of the vectors are
informative.

### Verified claims (no finding)

`tests/claims.rs` (`--ignored`, exhaustive): `IntExp` has maximum `IntExp(0) = 16,781,800` over
`[−31·LN2_Q − 1000, 1000]` and is non-decreasing inside every range-reduction bucket; `IntRsqrt`'s
Newton value lies in `[8,388,608, 16,777,215] ⊂ [1, ONE]` for every mantissa in `[2^24, 2^26)` (the
§7 premise), outputs sampled over every exponent stay in `[0, 68,719,472,640]` with the maximum at
`IntRsqrt(1)`; `IntLn`'s series part lies in `[0, 11,629,070]` for every mantissa in `[2^24, 2^25)`,
so `IntLn(x) ∈ [s·LN2_Q, (s+1)·LN2_Q)` holds — with a margin of only 10 units below `LN2_Q`
(informative: a change of constants could break §7's `IntLn` interval).

Everything else in §2–§6 and §9 was implementable from the text alone, and the golden vectors
reproduced on the first run.
