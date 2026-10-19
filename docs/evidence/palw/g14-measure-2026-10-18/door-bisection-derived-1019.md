# G14 door with ADR-0191 history-tile bisection: derived upper (2026-10-19)

**derived_upper (formula, not measured).** Analysis record. No rule, constant, price, window or fence was changed; nothing is armed.

Test: `bounded_door_derived_upper_32k_and_2m_prints_every_term` in `consensus/core/src/palw_tir_frame_cost_v1.rs`
(run with `cargo test -p kaspa-consensus-core --lib bounded_door_derived_upper -- --nocapture`). It calls only the existing
`palw_tir_dispute_door_fit_v1`, `palw_tir_history_rounds_v1` and the declared ADR-0191 constants. It asserts arithmetic identities
only; it does not assert the door is at or below 3,120.

## Assumptions

- Shape: Huihui-Qwen3.5-2B class. Attention tile, GDN state block and matmul block of 65,536 / 65,536 / 131,072 B, one-leaf plus
  one-logits-tile floor of 16,384 B (ADR-0191 declared shape; not derived from the program).
- `b = 16`, `M = 1` (one adjudication slot per claim), `K_owner = 8`, `L = 2` extra levels (step block 17,408 B, ADR-0191 final terms).
- Span fit `span(B) = 44 + ceil(B / 920)` from ADR-0190's two one-round points. A fit, not re-measured on the shipped binary.
- Rounds: `R = R_pos + R_hist + R_op + R_tile`. `R_pos = ceil(log16 N)`, `R_op = ceil(log16 1024) = 3`, `R_tile = ceil(log16 61) = 2`
  (vocabulary 248,320 / 4,096 lanes). `R_hist = palw_tir_history_rounds_v1(tiles)`: 2,048 tiles → 3, 65,536 tiles → 4.
- Terminal history read ≈ 135,600 B: the value given for this step, **not recomputed here**. It is priced as its own adjudication
  carrier (own + one restart), added on top of the 17,408 B step block (door_B). door_A excludes it.
- Non-cone carry-over (declared, from `door-breakdown-1018.md`): IDs/leaf 106, filing allowance 82, open window 5. Replaced and
  not added: shipped trace tiles 4,209, output 2,346, reorg restart 2,346, owner queue 1,536 (the dispute formula's
  `(K+1)·span` carries the queue, own adjudication and restart).
- Bandwidth sensitivity: 125,000 B/DAA is the shipped constant and a theoretical ceiling only. The 1/10 case (12,500 B/DAA) is
  modelled by multiplying every byte term (blocks, floor, terminal) by 10, which is exactly `ceil(10B / 920)` in the span fit. The
  44 DAA intercept and the 46 DAA round are not scaled. The carry-over constants are not scaled either (small).

## Term table (DAA)

### 125,000 B/DAA (sensitivity baseline)

| term | 32K (N = 32,768, 2,048 tiles) | 2M (N = 2,097,152, 65,536 tiles) |
|---|---:|---:|
| rounds R (R_pos + R_hist + R_op + R_tile) | 4 + 3 + 3 + 2 = 12 | 6 + 4 + 3 + 2 = 15 |
| prep (`PALW_TIR_DISPUTE_PREP_DAA_V1`) | 16 | 16 |
| bisection (R + 2) × 46 | 14 × 46 = 644 | 17 × 46 = 782 |
| output check | 53 | 53 |
| step block B_adj = 16,384 + 256 + 256 + 512 | 17,408 B | 17,408 B |
| span(B_adj) | 63 | 63 |
| admission queue + own + restart, (K + 1) × span | 9 × 63 = 567 | 9 × 63 = 567 |
| **core door** (`palw_tir_dispute_door_fit_v1`) | **1,280** | **1,418** |
| carry-over IDs / leaf | 106 | 106 |
| carry-over filing allowance | 82 | 82 |
| carry-over open window | 5 | 5 |
| **door_A** (no terminal) | **1,473** | **1,611** |
| terminal span(135,600 B) = 44 + ceil(B / 920) | 192 | 192 |
| terminal own + restart | 384 | 384 |
| **door_B** (with terminal) | **1,857** | **1,995** |

### 12,500 B/DAA (1/10 sensitivity, bytes × 10)

| term | 32K | 2M |
|---|---:|---:|
| rounds R | 12 | 15 |
| bisection (R + 2) × 46 | 644 | 782 |
| step block B_adj | 174,080 B | 174,080 B |
| span(B_adj) | 234 | 234 |
| admission queue + own + restart, 9 × span | 2,106 | 2,106 |
| **core door** | **2,819** | **2,957** |
| carry-over (106 + 82 + 5) | 193 | 193 |
| **door_A** | **3,012** | **3,150** |
| terminal span(1,356,000 B) | 1,518 | 1,518 |
| terminal own + restart | 3,036 | 3,036 |
| **door_B** | **6,048** | **6,186** |

## Reading the numbers (not a pass or fail claim)

- At the 125,000 B/DAA sensitivity, door_A is 1,473 (32K) and 1,611 (2M); door_B is 1,857 and 1,995. Both are under 3,120 under
  these assumptions.
- At 1/10 bandwidth, door_A is 3,012 (32K) and 3,150 (2M): the 2M value is 30 DAA above 3,120. door_B is above 3,120 at both sizes.
- The dominant terms are the `(K + 1) × span(B_adj)` carrier count (567 at full bandwidth, 2,106 at 1/10) and the bisection rounds
  (644 / 782). The step block size is the lever: it is the term the 1/10 case is most sensitive to.
- Round count changes from the ADR-0191 table: 2M had R = 16 with `R_attn = 5` at 131,072 tiles; this record uses 65,536 tiles
  (`R_hist = 4`), so R = 15. The tile count is taken as given; the tile size that yields it (h = 32 at 2M) is not re-derived here.

## Unmet conditions

1. **No release-binary measured span.** Every span is the fit `44 + ceil(B / 920)`; the 17 KB and 135.6 KB points are outside the
   two fit points (0.8 MB, 2.1 MB), so the intercept dominates and the result depends on it.
2. **No live-node E2E.** The terminal 135.6 KB value is not recomputed from the step program; the carry-over constants come from
   `door-breakdown-1018`, a measurement of the shipped 32K class, not of the bisection path.
3. Shape constants (kv row, state block, matmul block) of the 2M program are declared, not derived.
4. The M = 1 admission rule is the dormant `palw_tir_adjudication_slot_v1`; its wait is `(K − 1) × span` by construction.
5. Whether the terminal carrier counts in addition to, or in place of, the 17,408 B step block is a modelling choice. door_A and
   door_B bracket it.

No fence is armed. The derived upper is not a bound on the shipped door.
