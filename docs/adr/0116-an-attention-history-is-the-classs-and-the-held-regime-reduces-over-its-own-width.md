# ADR-0116 — An attention history is the class's, and the held regime reduces over its own width

* Status: PROPOSED and IMPLEMENTED 2026-09-11 on `feat/adr-0103-held-context`, at the operator's
  instruction. The same day ADR-0103 §10.7 recorded a decision to keep the bound; the operator then
  reversed it and asked for the change ("A16 の履歴上限 2^18 … の実装を行う方針に変更する").
  Consensus-inert on every shipped network: the bound a class reads is fixed by the held map
  registered in its class id, and the gate admits a held class only while `Params::palw_held_context`
  is armed. No preset's fingerprint moves.
* Builds on: [0040](0040-palw-base-0-integer-arithmetic.md) (Decision E: exact integer accumulation within
  a proven bound, which the A16 tier inherits), [0082](0082-the-close-is-flat-in-the-context.md)
  (the fused attention site and its tile route), [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (the held regime, whose widest context is 2M), [0110](0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md)
  (the context vectors that found the wall).
* Reverses: ADR-0103 §10.7's "Decided 2026-09-11: the bound stays."

## 0. The sentence this ADR is

**The longest attention history an A16 op reduces over is a property of the class, not of the
projection tier: a class under the held regime reduces over the regime's width, `2^21` positions,
and every other class over the `2^18` it has always had. The number is read from the class's
registered profile by every engine, seat and court that runs it, so no two nodes can read it
differently, and a held class whose context is past its bound is refused at the gate by name.**

## 1. What was found

ADR-0103 §10.7 — "the eighth wall". The A16 tier bounds every reduction at `A16_MAX_DOT_LEN = 2^18`,
a number chosen for projections ("one power of two above the largest real reduction in the family").
W10 (`a16_attn_values`) and the fused site reused that constant as the bound on the attention
HISTORY, so no A16-family class could execute, replay or adjudicate past position 262,143, and the
2M context the held regime exists for failed at its 262,145th row. The shipped networks never met
the wall, because a free prompt there is at most 4,096 ids. The held regime did: `0110-dense-v7-2m`
was refused before produce, by name, and ADR-0103's claim that every wall admits at 2M was wrong
about one wall that was not a chain wall.

Three facts about the bound decided how it could move:

* **It is in no hashed descriptor.** The kernel descriptors do not encode it, `kernel_semantics_id`
  is a hash of the descriptor string, and the court catalog root is a hash of the ids. Changing the
  constant moves no id, root or fingerprint, so changing it alone would be a silent divergence
  between builds wherever a history past `2^18` is reachable.
* **It is shared with the matmuls.** The fast kernels' `sdot` path skips chunking under a
  compile-time assert that fails above 263,191. The projection bound cannot move with it.
* **A history past `2^18` is reachable only under the held regime.** Every other class's context is
  bounded by the enumeration ceiling (`n_ctx × layer_count`) and the ladder. Only a class that
  registered a held map has its context bounded by the ladder's depth, which admits 2M.

## 2. The requirement

A class under the held regime runs, replays and is adjudicated at every position its registered
context admits, up to ADR-0103's 2M, with the arithmetic exact under ADR-0040 Decision E at that
width. Every class that exists today computes and refuses exactly what it did before, and no node
can apply a different bound from another to the same class.

## 3. Decisions

**Decision 1 — the bound is read off the class.** `palw_state_chunk_map::palw_attn_history_bound_v1`
returns the held regime's `A16_MAX_ATTN_HISTORY_HELD_V1 = 2^21` for a profile whose state chunk map
is held, and `A16_MAX_ATTN_HISTORY_V1 = 2^18` for every other profile. The second is the old bound
byte for byte. It takes the profile alone because every engine, seat and court that runs a class
holds its profile and none of them holds a ruleset. The held map is inside the class id
(`shape_profile_id`), and the admission gate refuses a held class unless `Params::palw_held_context`
is armed, so the bound is the network's decision without being a value anyone could read
differently. No new fence: the held regime is what admits contexts past `2^18`, and its fence already
decides when that is true.

**Decision 2 — the ops take the bound, and the old names keep the old one.** `a16_attn_values_within`
and `a16_attn_fused_reference_within_v1` in consensus core, and `a16_attn_values_fast_within` and
`a16_attn_fused_uniform_fast_within` in the kernels, refuse a history past `min(max_history, 2^21)`,
so no caller widens past the held bound whatever it passes. The names without `_within` delegate at
`2^18`. The projection bound `A16_MAX_DOT_LEN` is untouched, and so is the tile route's per-tile
bound: a tile is a court parameter, not a history, and a whole history is folded from tiles.

**Decision 3 — every consumer reads the same bound.** The court's `AttnFused` and `AttnValues` arms
(`palw_step_refute::qwen36_row`) read the bound from the profile they already hold. The dense and
hybrid plans (`A16ProfilePlanV1`, `Qwen36ProfilePlanV1`) record it when they are compiled from the
registered profile, and every planned walk passes it to the attention kernels. The engines' own
unplanned paths, which exist only for the shipped canonical rows, keep `2^18`.

**Decision 4 — a held context past the bound is refused at the gate.** The held map lifted the context
ceiling; the ops still refuse past `2^21`. So `verify_class_admission_v8` refuses a held class that has
attention layers and a context above its bound, with both numbers and this ADR in the message. That
is better than a producer, a seat or a court discovering the wall at the class's first position past
it. The refusal is reachable only with the fence armed.

**Decision 5 — the vectors' wall is their class's.** `palw_context_vector_blocked_v1` reads the bound
of the held row every shipped vector runs. `0110-dense-v7-2m` is now inside it: its last position
attends to 2,097,151 rows. A job one row past the held bound is still refused before produce, by
name.

## 4. What it costs

* **Exactness: nothing.** A value product is below `2^30`, so `2^21` rows sum below `2^51`. A row of
  `2^21` Q24 exponents sums below `2^45`. Both are far inside `i64`, and a test runs the widest
  history at the extreme codes against the exact `i128` sum.
* **Precision, which is the model's arithmetic, not consensus.** At `2^21` rows, a near-uniform row's
  Q24 probabilities are about `8 / 2^24`, three bits each. Every node computes the same three bits, so
  nothing diverges; what a long, flat attention row loses is resolution. A probability format wider
  than Q24 would be new kernel semantics (a new descriptor, a new id), and it is not decided here (§7).
* **Memory:** the fused kernel's scratch rows are the history's own length, about 200 MB for twelve
  heads at `2^21`, for a class that asked for that context.
* **Time, which this ADR does not buy.** The seat's replay grows with the square of the width past the
  knee (ADR-0103 §10.6). The 131,072-position vector took 44.7 minutes, 819 s of it the seat's, on a
  12-core host (ADR-0110 §9.5). The bound moving makes 2M executable; it does not make it cheap.
  Running the 2M vector end to end is an external run of days, not a CI gate.

## 5. Invariants the tests hold

1. **I-1, the shipped bound is unchanged.** `a16_attn_values` still refuses `2^18 + 1` rows, and
   `A16_MAX_ATTN_HISTORY_V1 == A16_MAX_DOT_LEN`
   (`an_attention_history_past_the_bound_is_refused_and_that_is_the_eighth_wall`).
2. **I-2, the held bound is `2^21`, and exact there.** `2^18 + 1` rows and `2^21` rows are reduced,
   `2^21 + 1` rows are refused, no caller widens past it, and the accumulator equals the exact sum at
   the extreme codes (`the_held_history_bound_is_2_21_and_exact_there`).
3. **I-3, the fast kernels are still the catalog past the old wall**
   (`past_the_eighth_wall_the_held_kernels_are_the_catalog`).
4. **I-4, the bound is the class's.** The held dense and hybrid rows read `2^21`; the shipped dense
   row and the hybrid's graph-v6 read `2^18` (`the_attention_history_bound_is_the_classs`).
5. **I-5, the gate refuses a held context past the bound**
   (`a_held_context_past_the_attention_history_bound_is_refused_by_name`), and admits `2^21`.
6. **I-6, the 2M vector is inside the bound**, and a job one row past it is refused before produce
   (`the_2m_vector_is_inside_the_held_bound_and_one_row_past_it_is_refused_by_name`).

## 6. Supersession

| what | by |
|---|---|
| ADR-0103 §10.7: "Decided 2026-09-11: the bound stays" | Decisions 1–5 |
| ADR-0110's `0110-dense-v7-2m`, "refused before produce, by name, until that wall moves" | Decision 5: inside the held bound |

## 7. What is deliberately not decided

* **A wider probability format.** Three-bit probabilities at `2^21` rows are exact and identical on
  every node, and they are coarse. A class that needs more resolution at that length needs a softmax
  and a requantization in a wider fixed point. That is a new kernel with its own descriptor, in the
  fenced table the way ADR-0102's lift is.
* **Arming the held regime anywhere.** This ADR changes what a held class can run; ADR-0103's fence
  still decides whether any network has held classes, and every shipped preset leaves it dormant.
* **Running the 2M vector end to end.** It is now possible and it takes days. Its first reproduction
  belongs with the 128K run's (ADR-0110 §9.5), as an external pin.

## 8. Number hygiene

Written as 0116 on `feat/adr-0103-held-context`, after `origin/main` (0114, 0115) and every local
branch were listed. 0113 was left unused by the writers of 0114 and 0115 and stays skipped. 0117 is
this line's other ADR of the same instruction. **The next free number is 0118.**

## 9. Implementation record (2026-09-11)

| Decision | where | what pins it |
|---|---|---|
| **1** the bound | `palw_state_chunk_map::palw_attn_history_bound_v1`; `palw_base0_a16::{A16_MAX_ATTN_HISTORY_V1, A16_MAX_ATTN_HISTORY_HELD_V1}` | I-4 |
| **2** the ops | `a16_attn_values_within`, `a16_attn_fused_reference_within_v1`; `kernels::{a16_attn_values_fast_within, a16_attn_fused_uniform_fast_within}` | I-1, I-2, I-3 |
| **3** the consumers | `palw_step_refute::qwen36_row` (`AttnFused`, `AttnValues`); `A16ProfilePlanV1::attn_history`, `Qwen36ProfilePlanV1::attn_history` | I-3, the plans' differentials |
| **4** the gate | `palw_class_admission_v2` (the held arm) | I-5 |
| **5** the vectors | `context_vector::palw_context_vector_blocked_v1` | I-6 |

The suites: consensus core (the A16 op, held, step-refute, chunk-map and admission modules) 109
passed; the held integration suite 11 passed; base0's kernels, engines, plans and vectors 40 passed.
