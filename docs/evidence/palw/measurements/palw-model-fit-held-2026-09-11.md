# ADR-0097 — a model's fit is a lookup

Ruleset **testnet-11's lattice minted with the held regime (ADR-0103; NOT a shipped preset)**, fences read at DAA **18446744073709551614**: `palw_kary_court` armed, `palw_context_ladder` dormant, prompt ids **MerkleV1**. Ladder **2^48**, close ceiling **2250000 bytes** / **27 carriers**, turn deadline **42 DAA**, terminal rounds **2**, court window **3000 DAA**, standard transaction **120000 bytes**, free-prompt decode ceiling **1024 tokens**.

Where each ceiling lives, and therefore what it costs to move:

| wall | the ceiling lives in |
|---|---|
| geometry ceiling | `PalwShapeProfileV3::validate_geometry (PALW_STEP_MAX_ENUMERATION; gates ClassRegistered)` |
| ladder | `PalwCourtParamsV2::max_step_leaf_count (inside palw_ruleset_id_v2)` |
| close bytes | `PalwCourtParamsV2 cost ceilings (inside palw_ruleset_id_v2)` |
| close chunks | `PalwCourtParamsV2 cost ceilings (inside palw_ruleset_id_v2)` |
| terminal macs | `PalwCourtParamsV2 cost ceilings (inside palw_ruleset_id_v2)` |
| operand count | `PalwCourtParamsV2 cost ceilings (inside palw_ruleset_id_v2)` |
| court window | `PalwStateParamsV2::window_court and the court's turn deadline (inside palw_ruleset_id_v2)` |
| state chunks | `palw_step_leg::PALW_STEP_LEG_MAX_STATE_CHUNKS (the checkpoint leg's cap)` |
| public-da payload | `palw_mode_v2::PALW_STANDARD_TX_BYTES (the mirrored standard-transaction mass)` |

## 1. The rows this ruleset's genesis registers, and each family at its own width

First the rows the ruleset itself carries (`genesis_objects`, `ClassRegistered` with a carried profile) — the positive control: the chain admitted these when it was cut. Then each family at the width its geometry constant declares.

### genesis row `4277d84f7d91528cc04aa366d51ee1c2e4f7902c4f6b16a213dead1c7e227977db732f18ed6183db3d944d44726ebd3feff7b15c48f9dba11cd526684f35f1b7` (n_ctx 512, 28 layers)

- n_ctx **512**, 28 layers (28 attention, 0 recurrent), fused site: **true**; arity played **2**, arity this row would derive: **2**; prompt ids **MerkleV1** (472 bytes a close); one job answers at most **1024** tokens

| wall | need | have | unit | verdict | order | note |
|---|---|---|---|---|---|---|
| geometry ceiling | 14336 | 16777216 | positions × layers | admitted | **linear** | n_ctx 512 × layer_count 28 |
| ladder | 52778128 | 281474976710656 | leaves | admitted | **linear** | the whole context as prefill is 52778128 leaves; 2^26 would hold it |
| close bytes | 87743 | 2250000 | bytes | admitted | logarithmic | binding node attn[7] AttnFused (2100 opening + 85643 evidence) |
| close chunks | 2 | 27 | carriers | admitted | logarithmic | binding node attn[7] AttnFused (2100 opening + 85643 evidence) |
| terminal macs | 35840 | 16777216 | multiply-accumulates | admitted | constant |  |
| operand count | 4 | 8 | rows | admitted | constant |  |
| court window | 762 | 2999 | DAA | admitted | logarithmic | 13 moves × 42 DAA + 216 reserve at arity 2 over 512 history positions (no leaf ladder, ADR-0082 Z4) |
| state chunks | 1792 | 65536 | chunks | admitted | **linear** | 28 attention layers × 2 slices × ⌈512 / 16⌉ tiles |
| public-da payload | 2048 | 120000 | bytes | admitted | **linear** | the prompt ids ride the commitment under PublicDa; under PanelDa (ADR-0077 Decision 16) they do not |

Held, not carried (ADR-0103 Decision 8) — regime **HeldNetwork**:

| held term | need | unit | order | checked against | note |
|---|---|---|---|---|---|
| executor retention | 29362176 | bytes | **linear** | the executor's disk for the claim's life (claim_retirement); a host fact | the attention cache, the recurrence state and the ids, for the claim's life |
| seat fetch | 0 | bytes | constant | window_receipt × the seat's bandwidth (ADR-0103 Decision 7: the shard plan's fetch column) | a seat recomputes from the ids it holds and fetches nothing (ADR-0082 Decision 9) |
| seat replay | 512 | positions | **linear** | window_receipt at the family's replay rate over the drill's margin (ADR-0103 Decision 2; the certification drill) | interval 0 is the whole prefill, recomputed from the prompt (ADR-0086 §1) |

- verdict: **ADMITTED on every wall**

### Qwen2.5-1.5B A16 graph-v7 (dense; the held map)

- n_ctx **512**, 28 layers (28 attention, 0 recurrent), fused site: **true**; arity played **2**, arity this row would derive: **2**; prompt ids **MerkleV1** (472 bytes a close); one job answers at most **1024** tokens

| wall | need | have | unit | verdict | order | note |
|---|---|---|---|---|---|---|
| geometry ceiling | 677 | 65664 | nodes a position | admitted | constant | 677 nodes a position over 28 layers; the context multiplies nothing a node walks |
| ladder | 26 | 48 | levels (64 bytes of path each) | admitted | logarithmic | the whole context as prefill is 52778128 leaves; a path to one is 26 levels and no round is played |
| close bytes | 87743 | 2250000 | bytes | admitted | logarithmic | binding node attn[7] AttnFused (2100 opening + 85643 evidence) |
| close chunks | 2 | 27 | carriers | admitted | logarithmic | binding node attn[7] AttnFused (2100 opening + 85643 evidence) |
| terminal macs | 35840 | 16777216 | multiply-accumulates | admitted | constant |  |
| operand count | 4 | 8 | rows | admitted | constant |  |
| court window | 762 | 2999 | DAA | admitted | logarithmic | 13 moves × 42 DAA + 216 reserve at arity 2 over 512 history positions (no leaf ladder, ADR-0082 Z4) |
| state chunks | 12 | 48 | levels | admitted | logarithmic | ⌈512 / 16⌉ = 32 blocks a slice under 112 slices at most; a block appended moves no index |
| public-da payload | 0 | 120000 | bytes | admitted | constant | the widest job commits under PanelDa (ADR-0077 Decision 16): the ids are served under the root the chain names, never carried (ADR-0103 Decision 4) |

Held, not carried (ADR-0103 Decision 8) — regime **Held { panel_da: true }**:

| held term | need | unit | order | checked against | note |
|---|---|---|---|---|---|
| executor retention | 29362176 | bytes | **linear** | the executor's disk for the claim's life (claim_retirement); a host fact | the attention cache, the recurrence state and the ids, for the claim's life |
| seat fetch | 29360128 | bytes | **linear** | window_receipt × the seat's bandwidth (ADR-0103 Decision 7: the shard plan's fetch column) | the state at the last interval's start, one seat holding every layer — the Recompute route: 512 positions × 34 ms fit the seat's budget, so a seat recomputes the prefix and fetches nothing; resuming would fetch this |
| seat replay | 512 | positions | constant | window_receipt at the family's replay rate over the drill's margin (ADR-0103 Decision 2; the certification drill) | P, at zero fetch time, 34 ms a position and 600 DAA; a seat's bandwidth narrows it (Decision 7) |

- verdict: **ADMITTED on every wall**

### Qwen3.6-35B-A3B graph-v7 (hybrid; the held composition)

- n_ctx **512**, 40 layers (10 attention, 30 recurrent), fused site: **true**; arity played **2**, arity this row would derive: **2**; prompt ids **MerkleV1** (472 bytes a close); one job answers at most **1024** tokens

| wall | need | have | unit | verdict | order | note |
|---|---|---|---|---|---|---|
| geometry ceiling | 1875 | 65664 | nodes a position | admitted | constant | 1875 nodes a position over 40 layers; the context multiplies nothing a node walks |
| ladder | 28 | 48 | levels (64 bytes of path each) | admitted | logarithmic | the whole context as prefill is 165309072 leaves; a path to one is 28 levels and no round is played |
| close bytes | 283676 | 2250000 | bytes | admitted | logarithmic | binding node gdn[15] GatedDeltaNet (2116 opening + 281560 evidence) |
| close chunks | 4 | 27 | carriers | admitted | logarithmic | binding node gdn[15] GatedDeltaNet (2116 opening + 281560 evidence) |
| terminal macs | 1048576 | 16777216 | multiply-accumulates | admitted | constant |  |
| operand count | 6 | 8 | rows | admitted | constant |  |
| court window | 762 | 2999 | DAA | admitted | logarithmic | 13 moves × 42 DAA + 216 reserve at arity 2 over 512 history positions (no leaf ladder, ADR-0082 Z4) |
| state chunks | 12 | 48 | levels | admitted | logarithmic | ⌈512 / 16⌉ = 32 blocks a slice under 100 slices at most; a block appended moves no index |
| public-da payload | 0 | 120000 | bytes | admitted | constant | the widest job commits under PanelDa (ADR-0077 Decision 16): the ids are served under the root the chain names, never carried (ADR-0103 Decision 4) |

Held, not carried (ADR-0103 Decision 8) — regime **Held { panel_da: true }**:

| held term | need | unit | order | checked against | note |
|---|---|---|---|---|---|
| executor retention | 83888128 | bytes | **linear** | the executor's disk for the claim's life (claim_retirement); a host fact | the attention cache, the recurrence state and the ids, for the claim's life |
| seat fetch | 83886080 | bytes | **linear** | window_receipt × the seat's bandwidth (ADR-0103 Decision 7: the shard plan's fetch column) | the state at the last interval's start, one seat holding every layer — the Recompute route: 512 positions × 572 ms fit the seat's budget, so a seat recomputes the prefix and fetches nothing; resuming would fetch this |
| seat replay | 512 | positions | constant | window_receipt at the family's replay rate over the drill's margin (ADR-0103 Decision 2; the certification drill) | P, at zero fetch time, 572 ms a position and 600 DAA; a seat's bandwidth narrows it (Decision 7) |

- verdict: **ADMITTED on every wall**

## 2. The sweep — every wall at every context, every candidate

`REFUSED need / have`; a row the family cannot BUILD is refused at the geometry ceiling before any other wall can price it. `unpriced` on the four close walls is what a row the ladder refuses says of its close: the derivation's walk is capped at the ladder (audit D H-5), so the close of a row deeper than the ladder is not a number.

### Qwen2.5-1.5B A16 graph-v7 (dense; the held map)

| n_ctx | geometry ceiling | ladder | close bytes | close chunks | terminal macs | operand count | court window | state chunks | public-da payload | verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| 512 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 2048 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 8192 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 32768 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 131072 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 524288 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 1048576 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 2097152 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |

### Qwen3.6-35B-A3B graph-v7 (hybrid; the held composition)

| n_ctx | geometry ceiling | ladder | close bytes | close chunks | terminal macs | operand count | court window | state chunks | public-da payload | verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| 512 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 2048 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 8192 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 32768 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 131072 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 524288 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 1048576 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 2097152 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |

### Kimi K3 stand-in as graph-v7 (ADR-0097 §1.3 — NOT a class)

| n_ctx | geometry ceiling | ladder | close bytes | close chunks | terminal macs | operand count | court window | state chunks | public-da payload | verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| 512 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 2048 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 8192 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 32768 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 131072 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 524288 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 1048576 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |
| 2097152 | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | admitted | **admitted** |

## 3. The widest context each wall admits, per candidate

Each cell is the largest `n_ctx` at which THAT wall alone admits the row (bisection; every wall's `need` is non-decreasing in the context). The row's fit is the minimum of its cells. The four close cells cannot exceed the ladder's: past it the close is unpriced, not admitted.

| candidate | geometry ceiling | ladder | close bytes | close chunks | terminal macs | operand count | court window | state chunks | public-da payload | fit |
|---|---|---|---|---|---|---|---|---|---|---|
| Qwen2.5-1.5B A16 graph-v7 | 67108864 | 67108864 | 67108864 | 67108864 | 67108864 | 67108864 | 67108864 | 67108864 | 67108864 | **67108864** |
| Qwen3.6-35B-A3B graph-v7 | 67108864 | 67108864 | 67108864 | 67108864 | 67108864 | 67108864 | 67108864 | 67108864 | 67108864 | **67108864** |
| Kimi K3 stand-in as graph-v7 | 67108864 | 43821980 | 43821980 | 43821980 | 43821980 | 43821980 | 67108864 | 67108864 | 67108864 | **43821980** |

## 4. Does a 2M context fit? The geometry ceiling, for every depth

Under the held regime a held row meets no `n_ctx × layer_count` ceiling: `validate_geometry` reads the per-position budget instead (`PALW_STEP_MAX_NODES_PER_POSITION` = **65664** nodes a position; ADR-0103 Decision 6) and the context is bounded by the ladder's depth. The table is the shipped ceiling, which every class that registers no held map still meets on this network.

`PALW_STEP_MAX_ENUMERATION` bounds `n_ctx × layer_count` at **16777216** (`PalwShapeProfileV3::validate_geometry`). At 2^21 positions the fewest layers refused is **9**; at 2^20, **17**.

| layers | widest context the ceiling admits | 2^21 (2M) | 2^20 (1M) | 2^17 (128K) |
|---|---|---|---|---|
| 1 | 16777216 | admitted | admitted | admitted |
| 8 | 2097152 | admitted | admitted | admitted |
| 9 | 1864135 | **REFUSED** | admitted | admitted |
| 16 | 1048576 | **REFUSED** | admitted | admitted |
| 28 | 599186 | **REFUSED** | **REFUSED** | admitted |
| 40 | 419430 | **REFUSED** | **REFUSED** | admitted |
| 64 | 262144 | **REFUSED** | **REFUSED** | admitted |
| 92 | 182361 | **REFUSED** | **REFUSED** | admitted |
| 93 | 180400 | **REFUSED** | **REFUSED** | admitted |
| 128 | 131072 | **REFUSED** | **REFUSED** | admitted |
| 256 | 65536 | **REFUSED** | **REFUSED** | **REFUSED** |
| 1024 | 16384 | **REFUSED** | **REFUSED** | **REFUSED** |

## 5. What a seat must hold to replay one job

From the geometry and the state map's own row widths (i32 cache: `kv_heads × head_dim × 4` a position a layer). No verdict: no ruleset states what a host has. The artifact is the converter's number and is not here; for a model whose artifact this tree does not hold, the parameter count is printed as a lower bound at one byte a weight.

| candidate | n_ctx | attention cache | recurrent state | prompt ids | artifact (lower bound) |
|---|---|---|---|---|---|
| Qwen2.5-1.5B A16 graph-v7 | 512 | 28.0 MiB | 0 B | 2.0 KiB | the converter's |
| Qwen2.5-1.5B A16 graph-v7 | 32768 | 1.8 GiB | 0 B | 128.0 KiB | the converter's |
| Qwen2.5-1.5B A16 graph-v7 | 131072 | 7.0 GiB | 0 B | 512.0 KiB | the converter's |
| Qwen2.5-1.5B A16 graph-v7 | 1048576 | 56.0 GiB | 0 B | 4.0 MiB | the converter's |
| Qwen2.5-1.5B A16 graph-v7 | 2097152 | 112.0 GiB | 0 B | 8.0 MiB | the converter's |
| Qwen3.6-35B-A3B graph-v7 | 512 | 20.0 MiB | 60.0 MiB | 2.0 KiB | the converter's |
| Qwen3.6-35B-A3B graph-v7 | 32768 | 1.2 GiB | 60.0 MiB | 128.0 KiB | the converter's |
| Qwen3.6-35B-A3B graph-v7 | 131072 | 5.0 GiB | 60.0 MiB | 512.0 KiB | the converter's |
| Qwen3.6-35B-A3B graph-v7 | 1048576 | 40.0 GiB | 60.0 MiB | 4.0 MiB | the converter's |
| Qwen3.6-35B-A3B graph-v7 | 2097152 | 80.0 GiB | 60.0 MiB | 8.0 MiB | the converter's |
| Kimi K3 stand-in as graph-v7 | 512 | 69.0 MiB | 241.5 MiB | 2.0 KiB | 2.5 TiB |
| Kimi K3 stand-in as graph-v7 | 32768 | 4.3 GiB | 241.5 MiB | 128.0 KiB | 2.5 TiB |
| Kimi K3 stand-in as graph-v7 | 131072 | 17.2 GiB | 241.5 MiB | 512.0 KiB | 2.5 TiB |
| Kimi K3 stand-in as graph-v7 | 1048576 | 138.0 GiB | 241.5 MiB | 4.0 MiB | 2.5 TiB |
| Kimi K3 stand-in as graph-v7 | 2097152 | 276.0 GiB | 241.5 MiB | 8.0 MiB | 2.5 TiB |

## 6. Every wall's order, and what is held (ADR-0103 Decision 8)

Each cell is the order of the wall's `need` in the context, read by the row's own predicate at the context and six doublings of it (a wider row priced under a ladder that holds it). `**linear**` on a chain wall is what the held regime refuses a class for (`LinearInTheContext`); the held terms are linear by design and are bounded by the budget each names, never by a ruleset number.

| candidate | n_ctx | geometry ceiling | ladder | close bytes | close chunks | terminal macs | operand count | court window | state chunks | public-da payload | executor retention (held) | seat fetch (held) | seat replay (held) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Qwen2.5-1.5B A16 graph-v7 | 512 | constant | logarithmic | logarithmic | logarithmic | constant | constant | logarithmic | logarithmic | constant | **linear** | **linear** | constant |
| Qwen2.5-1.5B A16 graph-v7 | 32768 | constant | logarithmic | logarithmic | logarithmic | constant | constant | logarithmic | logarithmic | constant | **linear** | **linear** | constant |
| Qwen2.5-1.5B A16 graph-v7 | 2097152 | constant | logarithmic | logarithmic | logarithmic | constant | constant | logarithmic | logarithmic | constant | **linear** | **linear** | constant |
| Qwen3.6-35B-A3B graph-v7 | 512 | constant | logarithmic | logarithmic | logarithmic | constant | constant | logarithmic | logarithmic | constant | **linear** | **linear** | constant |
| Qwen3.6-35B-A3B graph-v7 | 32768 | constant | logarithmic | logarithmic | logarithmic | constant | constant | logarithmic | logarithmic | constant | **linear** | **linear** | constant |
| Qwen3.6-35B-A3B graph-v7 | 2097152 | constant | logarithmic | logarithmic | logarithmic | constant | constant | logarithmic | logarithmic | constant | **linear** | **linear** | constant |
| Kimi K3 stand-in as graph-v7 | 512 | constant | logarithmic | logarithmic | logarithmic | constant | constant | logarithmic | logarithmic | constant | **linear** | **linear** | constant |
| Kimi K3 stand-in as graph-v7 | 32768 | constant | logarithmic | logarithmic | logarithmic | constant | constant | logarithmic | logarithmic | constant | **linear** | **linear** | constant |
| Kimi K3 stand-in as graph-v7 | 2097152 | constant | logarithmic | logarithmic | logarithmic | constant | constant | logarithmic | logarithmic | constant | **linear** | **linear** | constant |

