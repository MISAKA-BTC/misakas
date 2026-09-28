# PALW-TIR v1 — findings of the independent second implementation (`misaka-palw-tir-ref2`)

> Scope: RFC-0002 freeze criterion 4 (`reference evaluator == independent second implementation ==
> every backend`) for `docs/spec/palw/04b-tensor-ir.md`. This file records where the normative text
> is ambiguous, silent or wrong, and where the first implementation (`misaka-palw-tir`, observed only
> as a black box through its public API) departs from the text. Branch `tir/ref2`: revision 1 of 04b
> from `tir/core` b7492d601; **revision 2** from `tir/core` a2e6ec35a (merged as 5f852c190), which
> applied F1–F15. Written without reading `misaka-palw-tir/`, the other files of this directory, or
> the legacy kernels; the text changes were read as `git diff` of 04b only.

## 概要(日本語)

- **改訂 2(a2e6ec35a)で F1〜F15 はすべて解消。** ref2 を改訂後の 04b だけから更新した(cone env の規則、
  NF-1 の `PRIM_SET_ID_V1`、NF-2、NF-19、§9.3 の class 表、run-state の省略規則)。`PRIM_SET_ID_V1` は
  RFC 7693 から書いた BLAKE2b で本文の定義どおりに計算し、本文に印字された値と一致した。
- golden vector は **239/239**(primitive 134、program の step 34・cone 15・refusal 38、encoding 18)を
  class まで一致して再現。
- 前回と同じ規模(×25)の差分で **成功/失敗と値の不一致は 0 件**。class は「規則を 1 つだけ破る入力では
  同じ、複数破る入力では破った規則の class のどれか」(§9.3)を、ref2 側で「破っている規則の集合」を
  計算して検査し、違反 0 件。class が分かれたのは複数規則を破る入力(例: carry-out node の宣言 dtype を
  変える → NF-16 と NF-5)だけ。cone の欠陥 18 種 36,025 件は本文 = ref2 = 第 1 実装。
- 改訂 2 が残す点(いずれも両実装は一致): N1 cone env のうち closure が読まない `fixed`/`hist_prior`/
  `carry_in` の扱い(両方無視、本文は supplied についてのみ明記)、N2 単独 primitive の arity 誤りの
  class(両方 `Shape`、§9.3 の表は program の NF-14 だけ)、N3 §9.2 の表と §9.3 で「値」の検査の書き方が
  不一致、N4 `graph_ir_root` の keyed hash(鍵・domain)がまだ 04b に無い(04b だけでは計算できない)。

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

## New after revision 2 (open)

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
