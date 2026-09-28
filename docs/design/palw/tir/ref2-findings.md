# PALW-TIR v1 — findings of the independent second implementation (`misaka-palw-tir-ref2`)

> Scope: RFC-0002 freeze criterion 4 (`reference evaluator == independent second implementation ==
> every backend`) for `docs/spec/palw/04b-tensor-ir.md`, and now its admission (`tir_admit_v1`,
> §10.3 with §7 and §8). This file records where the normative text is ambiguous, silent or wrong,
> and where the first implementation (`misaka-palw-tir`, observed only as a black box through its
> public API) departs from the text. Branch `tir/ref2`: revision 1 of 04b from `tir/core`
> b7492d601; revision 2 from a2e6ec35a (merged as 5f852c190); **admission** from ccd8f6aae
> (merged as 34c9d92d3). Written without reading `misaka-palw-tir/` (its `tests/admit.rs`
> included), the other files of this directory, or the legacy kernels; the text changes were read
> as `git diff` of 04b only.

## 概要(日本語)

- **admission(`tir_admit_v1`)を 04b §10.3・§7・§8 だけから独立実装。** 区間解析、§8 の node cost と
  1 position の量、cone と leaf、tile の box demand(history の prior rows、commit 済み leaf は 4 byte/lane)、
  `H` 縮約の cone の `h_chunk` 分解、cone work(上限は costing 前に判定)、replay closure、group `G`、`C_j`。
- corpus 5 本と自前の **Qwen2.5-1.5B 形 decoder**(28 層・`d = 1536`・語彙 151,936・window `2^18`)で、
  公開 API が出す全ての派生量(区間・node cost・position・cone の nodes/leaves/tile/chunk/opened bytes/
  operands・closure・groups・`C_j`・`C`・cone work)が第 1 実装と完全一致。
- ×25 の差分(乱数 program・range-safe な乱数 program・変異・乱数 ceiling、計 487,500 件)で、
  **一致しないのは 1 つの読み方(A1)だけ**: `Fixed` state の replay を `G` 個の group に分けてよいかの規則。
  本文は「closure の update cone の全 node が aligned」とだけ書き、free な node の扱いを root とそれ以外で
  区別しない。ref2 は一様に読み(free は妨げない)、第 1 実装は「全 member の `StateWrite` が aligned
  (free 不可)、かつ closure の update cone に含まれる commit 点は leaf でなく node として辿る」。この規則を
  スイッチで再現すると残りは 0 件。A1 は 30,319 件で groups・replay cost・`C_j` を変え、**22 件で受理/拒否が
  逆になる**(consensus 分岐になりうる)。
- 他に editorial: A2 拒否の limit 名が本文に無い(第 1 実装 "tile MACs"、ref2 はしきい値名)、A3 `C_j = 0`
  拒否の値と `max_checkpoint_interval = 0` の扱い、A4 commit 済み `StateWrite` の cone work 二重計上の文言、
  A5 `ConeV1.whole` が未定義、A6 §7 の義務違反の class、A7 誰も書かない `Fixed` state。
- 改訂 2 の残り N1〜N4 は本文で解消済み(未読 entry は無視・単独 primitive の arity は `Shape`・値の検査・
  `graph_ir_root` の鍵)。`graph_ir_root` は本文の定義から計算し、全 vector と §3.6 の印字値に一致。

## Admission (`tir_admit_v1`, 04b at ccd8f6aae)

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
