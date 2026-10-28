# G14 derived upper of the cone read set, Huihui-Qwen3.5-2B 32K class (2026-10-18)

Analysis and test record. No rule, constant, fence, window, price or consensus behaviour was changed. The only production-crate
change is a read-only accessor `hist_leaves_per_tile_v1()` in `consensus/core/src/palw_tir_step_v1.rs`.
Raw: `derived-upper-1018.json` (door rows and window), `derived-window-raw-1018.json` (test output),
`containment-fixture-1018.json` (containment record).

## Status: partial. Element-level derivation and State/HistTile measurement on the real class are NOT done.

## 1. The window rule (value-free, params-free)

Let `p` be the disputed step leaf's position, `C` the checkpoint interval, `h` the history tile. The rule:

- every commit or state operand `q` satisfies `q <= p <= q + C + h + 1`;
- every HistTile operand satisfies `q < p`.

Why it holds (code): `TirSource::state` replays from `p` to the last checkpoint (at most `C` positions back); `hist_row` reads the
current tile's commit points (at most `h` back) or one HistTile leaf per completed tile (any older position, so it is covered
by the second clause); `node`/`param`/`token` read commit nodes at the same position. Values change only WHICH element of those
nodes is read (Gather index, Select branch, TopK selection), never which position, so the rule is value-free.

Containment test (`read_set_containment_on_fixture_classes`, `frame_measure.rs`, release): all 11 fixture classes, every step
leaf as disputed leaf, 10-position jobs with checkpoint interval 2 and history tile 2. Result: 2,583 disputed leaves, 14,382
operands, 4,132 State operands, 1,411 HistTile operands, 0 violations. The same run checked that each completed history tile is
one contiguous range of step indices (20 tiles, all contiguous). Record: `containment-fixture-1018.json`.

## 2. Derived counts for the 32K class (params-free, `structural_read_window_on_a_real_class`)

Per-position leaf counts are the production `leaves_at_position` (prefill 1, so the post occurrence is included at every
position: an upper over all prefills). Cross-check: the measured job (prefill 4, 5 positions) gives 114,664 leaves and the shipped
448 runs, matching the record.

| quantity | value |
|---|---:|
| C, h, max_context | 164, 32, 32,768 |
| hist leaves per completed tile | 96 |
| total step leaves at 32,768 positions | 786,963,904 |
| max window leaves over all disputed p (argmax p = 32,639) | 4,915,476 |
| derived runs, history leaves counted per leaf (no contiguity assumption) | **116,166** |
| derived runs, history tile as one contiguous range (conditional, section 4) | 20,850 |
| shipped `cone_runs_bound_v1` at 32,768 positions | 95,309 |
| shipped at 6 / 33 / 165 / 197 / 1,024 positions | 552 / 3,035 / 15,479 / 18,421 / 95,309 |

Formula (sound superset, per-leaf history): `derived_leaves(p) = Σ_{q=p−C−h−1}^{p} L(q) + ⌊(p−C−h−1)/h⌋ · H`, with `L(q)`
the leaves at position q and `H = 96`; runs `≤ min(⌈window/256⌉ + 1 + ⌊lo/h⌋·H, ⌈total/256⌉)`. Evaluated at every p < 32,768.

Proof sketch: the rule (section 1) plus `Σ` counts every leaf at those positions; a run covers at most 256 consecutive indices.
Old history leaves are counted one run each (no assumption on their placement), so the bound holds without contiguity.

## 3. Comparison with observed_max and the 448

- observed_max (6-position decode-2 sampling, 8 runs / 97 leaves) is an observation, not a bound. Not used as a bound here.
- 448 is the measured job's own cap (`whole` = ceil(114,664/256)), not a property of a 32K job.
- At 32K the shipped formula gives 95,309, which is BELOW the per-leaf window superset (116,166). So the shipped value is a
  bound only under the per-tile contiguity it assumes (`per_tile = ceil(hist/256)+1`). That contiguity is verified on fixture
  classes, not proven for every legal layout. This is a finding about the shipped bound, not a rule change.

## 4. Uncovered conditions (explicit)

1. Element precision: the rule is a superset over positions, not over elements. Real reads are sparse; the set is not tightened.
2. Model operands (inventory/param leaves): NOT in the step-leaf bound. Gather over a parameter table reads a row chosen by a
   token-dependent index, so its reads range over the whole table. Not bounded or measured here.
3. Token/prompt/decode units: separate authenticated units, not step leaves, not bounded here.
4. Contiguity of completed history tiles: verified on fixture classes only (20 tiles). The 20,850 figure is conditional on it.
5. Boundary coverage: the containment test uses C = 2, h = 2. The rule is generic in C and h (proof sketch), but the test does not
   prove it for every (C, h) of the 32K class.
6. State and HistTile kinds on the REAL class: NOT measured. Checkpoint interval 164 and tile 32 need positions beyond 164; the
   decode-30 run was too slow (section 6 of read-set-1018). So no observed State/HistTile value exists for the real class.
7. Adversarial tests (collusion, default on missing answers, restart/reorg) are not part of this record.

## 5. Recomputed door (cone term only changed; non-cone terms held at shipped values)

door(runs) = 11,223 + runs × 2,346 (11,223 = the shipped door with the cone at zero; 448 × 2,346 + 11,223 = 1,062,231 reproduces it).

| cone runs | door DAA | <= 3,120 | basis |
|---:|---:|:-:|---|
| 448 | 1,062,231 | no | measured 5-position job (shipped) |
| 95,309 | 223,606,137 | no | shipped formula at 32,768 positions |
| 116,166 | 272,536,659 | no | derived upper, per-leaf history (sound by section 2) |
| 20,850 | 48,925,323 | no | conditional on history-tile contiguity |

Nothing here reaches 3,120. Whether the door check applies per claim job or per class maximum is not settled in this record; if
it applies per claim, every long job is far above the limit under the shipped formula.

## 6. Reproduction

```
CARGO_INCREMENTAL=0 cargo test --release -j 3 -p misaka-palw-tir-exec --features node --test frame_measure read_set_containment_on_fixture_classes -- --nocapture
MISAKA_TIR_FRAME_MEASURE_PALWTIR=<huihui2b.class.palwtir> MISAKA_WINDOW_OUT=<out.json> CARGO_INCREMENTAL=0 cargo test --release -j 3 \
  -p misaka-palw-tir-exec --features node --test frame_measure structural_read_window_on_a_real_class -- --ignored --nocapture
```
Builds and runs go through `/Users/wata/.cache/misaka-heavy-lock.sh` (coordinator, 10-18); the runs above were made before that
instruction and are recorded as such.
