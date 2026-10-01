# RFC-0003 PALW-GEN-20 — the gate's close price against the worker's measured closes

*2026-10-02, lane D. Evidence, not a decision. The class is the reduced SD3 pipeline as re-lowered with `CONV_DENSE_V2`
(class `6a88a17d003f…`, 13 stages, 168 commit points — 166 cone-closable and 2 dissected — 20,922 canonical step leaves,
`tile_len` 16, artifact 1,141,016 B).*

## What is compared

* **priced** — `palw_gen_worst_closes_of_class_v1` (`consensus/core/src/palw_gen_close_price_v1.rs`): what
  `verify_gen_class_admission_v1` holds against the carrier and what the catalog's `court_cost.max_close_bytes` records. The PALW-TIR-38 twin
  over every stage's version-1 view, the generative court's reading of inputs (edges are leaves of the upstream stage under its own root),
  of a `post`-written state (the committed write of the previous position, NF-29) and of checkpoint leaves; every leaf opened with its own
  path at its own stage's depth, every param as a whole `PalwArtifactOpeningV1` at the class inventory's depth (`p<k>/…` names), the frame
  the close object serialized at its widest binding (an ML-DSA-87 executor key, every stage root, the ids the stage reads).
* **measured** — `PalwGenEvidenceV1::cone_close` at a leaf of the widest job: exactly the units the court's evaluation read, serialized
  as the one-move accusation carries them (`borsh(PalwCourtVerdictProofV2::GenCone)`), the job's executor key at its on-chain length.

**Sample.** 1,231 leaves of the widest job (4 steps, a 3-id prompt): every 40th leaf of the claim's one order (every stage, every position),
the first, middle and last leaf of every commit point, and the first leaf of every position of every commit point; 1,215 are cone
closes (16 are leaves of the two dissected points, held to their root claims and bottoms below). The structural argument for every
position and job is the twin's (an abstract evaluator over element sets, a superset where the court reads by a value); the sweep is its
check on this class. Also held: every non-dissected commit leaf of the golden toy image, embedding and VLM classes
(`consensus/core/tests/palw_gen_close_price.rs`), and the committed write of the previous position (below).

## Result

* **priced ≥ measured at every one of the 1,215 closes** (166 commit points): priced/measured in **[1.001, 1.570]**, median 1.01;
  the smallest margin 68 B (a leaf path one level shorter than the class's inventory depth, and a list length), the median 80 B. The
  points above 1.1× are points whose sample did not reach the worst tile; the twin sizes every tile.
* **largest priced close 72,916 B**, largest measured 72,832 B (the CLIP MLP activation: a `Gather` over a two-piece table, stages
  `text_rows` and `text_pool`), against the one-move carrier of **95,037 B** — a margin of 22,121 B, and the class is admitted by the gate
  where a close can only ride one carrier (`the_sd3_class_registers_where_a_close_can_only_ride_one_carrier`).
* **dissected** (the CLIP fused-attention outputs): root claim measured 15,863 B, priced 15,883 B; bottom measured 17,459 B
  (`text_rows`) and 13,615 B (`text_pool`), priced 24,497 B.
* **checkpoint leaves**: none in this class (no `Fixed` state outside `post`); the toy VLM's seven are priced per state (7,245 B) against
  a measured largest of 4,327 B.
* **sizing work**: 66,871,506 steps for the class (denoiser 14.9 M, the ten VAE stages 51.7 M, the text stages 0.2 M), 4 s on an
  idle machine; the cap is 2^27.

## What the first run of the sound price found, and what changed

The gate's previous check priced a close by the operand bytes at element granularity plus a 16 KiB frame (a necessary condition) and
the twin's first generalisation priced below the measurement at 136 of 166 points. Priced soundly, the class as first lowered was
**not** convictable in one move:

| point | priced | cause | change |
| --- | ---: | --- | --- |
| CLIP MLP activation (`text_rows`, `text_pool`) | 140,458 B | a table of four 32 KiB pieces; a tile's lookups can land in all four | `LowerOpts::table_shift = 1`: two pieces, 72,916 B |
| VAE convolutions (`vae.up1.*`, `vae.out`) | up to 236,335 B | im2col by an index table param: a gather's data element is at a value's index, so the worst case is one input leaf per tap | `CONV_DENSE_V2`: static `Reshape`/`Slice`/`Concat` maps — priced exactly, no table |
| the nearest upsample, the unpatchify | (same) | pinned `Gather` tables | static `Broadcast` / rank-4 `Reshape`/`Transpose` |

The integers are unchanged (the static columns equal the table's gather element for element on 9 geometries; fidelity vs diffusers
43.8–45.7 dB, as before; the conviction battery: 61 kinds convicted in one lying run, 32 independent lies). The artifact is half the
size (the tables are gone: 2,301,248 B → 1,141,016 B).

## The committed write of the previous position

The latent is written in `post` (NF-29), so the court reads it at `p` as the committed write at `p − 1`. The price counts that leaf for
every element a cone reads (`the_price_counts_the_committed_write_of_the_previous_position_for_every_element_a_cone_reads`): for each
sampled denoiser leaf at positions ≥ 1 whose measured close carries such a leaf, every one of them is in the twin's read set of the same
cone (by coordinate) and the point's price is at least the measured close.

## The table

| commit point (stage / block / primitive) | block,node | leaves measured | measured B | priced B | margin B | priced / measured |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| text_rows / pre / Clamp | 0,19 | 9 | 10962 | 11045 | 83 | 1.01 |
| text_rows / attn.nope+mlp / Clamp | 1,32 | 9 | 10250 | 10335 | 85 | 1.01 |
| text_rows / attn.nope+mlp / Clamp | 1,51 | 10 | 40050 | 40135 | 85 | 1.00 |
| text_rows / attn.nope+mlp / Clamp | 1,70 | 9 | 40050 | 40135 | 85 | 1.00 |
| text_rows / attn.nope+mlp / Clamp | 1,89 | 10 | 40050 | 40135 | 85 | 1.00 |
| text_rows / attn.nope+mlp / Clamp | 1,145 | 10 | 40754 | 40840 | 86 | 1.00 |
| text_rows / attn.nope+mlp / Clamp | 1,178 | 9 | 10250 | 10335 | 85 | 1.01 |
| text_rows / attn.nope+mlp / Clamp | 1,197 | 10 | 40626 | 40711 | 85 | 1.00 |
| text_rows / attn.nope+mlp / Gather | 1,200 | 10 | 72832 | 72916 | 84 | 1.00 |
| text_rows / attn.nope+mlp / Clamp | 1,212 | 10 | 26327 | 26415 | 88 | 1.00 |
| text_rows / post / Clamp | 2,32 | 9 | 10250 | 10335 | 85 | 1.01 |
| text_pool / pre / Clamp | 0,19 | 7 | 10962 | 11045 | 83 | 1.01 |
| text_pool / attn.nope+mlp / Clamp | 1,32 | 7 | 10250 | 10335 | 85 | 1.01 |
| text_pool / attn.nope+mlp / Clamp | 1,51 | 7 | 40050 | 40135 | 85 | 1.00 |
| text_pool / attn.nope+mlp / Clamp | 1,70 | 7 | 40050 | 40135 | 85 | 1.00 |
| text_pool / attn.nope+mlp / Clamp | 1,89 | 7 | 40050 | 40135 | 85 | 1.00 |
| text_pool / attn.nope+mlp / Clamp | 1,145 | 7 | 40754 | 40840 | 86 | 1.00 |
| text_pool / attn.nope+mlp / Clamp | 1,178 | 7 | 10250 | 10335 | 85 | 1.01 |
| text_pool / attn.nope+mlp / Clamp | 1,197 | 8 | 40626 | 40711 | 85 | 1.00 |
| text_pool / attn.nope+mlp / Gather | 1,200 | 7 | 72832 | 72916 | 84 | 1.00 |
| text_pool / attn.nope+mlp / Clamp | 1,212 | 8 | 26327 | 26415 | 88 | 1.00 |
| text_pool / post / Clamp | 2,32 | 7 | 10250 | 10335 | 85 | 1.01 |
| text_pool / post / Clamp | 2,49 | 7 | 38996 | 39081 | 85 | 1.00 |
| text_pool / post / Reshape | 2,50 | 7 | 5546 | 5626 | 80 | 1.01 |
| denoise / pre / Clamp | 0,23 | 11 | 28242 | 28317 | 75 | 1.00 |
| denoise / pre / Clamp | 0,38 | 5 | 29748 | 29819 | 71 | 1.00 |
| denoise / pre / Clamp | 0,63 | 5 | 6174 | 6242 | 68 | 1.01 |
| denoise / pre / Clamp | 0,72 | 5 | 28294 | 28369 | 75 | 1.00 |
| denoise / pre / Clamp | 0,86 | 5 | 24590 | 25175 | 585 | 1.02 |
| denoise / pre / Clamp | 0,111 | 5 | 6174 | 6242 | 68 | 1.01 |
| denoise / pre / Clamp | 0,120 | 5 | 28218 | 28293 | 75 | 1.00 |
| denoise / pre / Clamp | 0,147 | 5 | 7156 | 7225 | 69 | 1.01 |
| denoise / pre / Clamp | 0,161 | 7 | 24798 | 24871 | 73 | 1.00 |
| denoise / transformer_blocks.0.attn / Clamp | 1,8 | 5 | 30508 | 30583 | 75 | 1.00 |
| denoise / transformer_blocks.0.attn / Clamp | 1,17 | 7 | 30660 | 30735 | 75 | 1.00 |
| denoise / transformer_blocks.0.attn / Clamp | 1,79 | 11 | 11084 | 11157 | 73 | 1.01 |
| denoise / transformer_blocks.0.attn / Clamp | 1,140 | 7 | 11084 | 11157 | 73 | 1.01 |
| denoise / transformer_blocks.0.attn / Transpose | 1,161 | 13 | 28294 | 28369 | 75 | 1.00 |
| denoise / transformer_blocks.0.attn / Transpose | 1,182 | 15 | 28294 | 28369 | 75 | 1.00 |
| denoise / transformer_blocks.0.attn / Transpose | 1,203 | 15 | 28294 | 28369 | 75 | 1.00 |
| denoise / transformer_blocks.0.attn / Clamp | 1,210 | 19 | 22868 | 22953 | 85 | 1.00 |
| denoise / transformer_blocks.0.attn / ReduceMax | 1,211 | 5 | 28760 | 28851 | 91 | 1.00 |
| denoise / transformer_blocks.0.attn / Clamp | 1,221 | 7 | 29742 | 29834 | 92 | 1.00 |
| denoise / transformer_blocks.0.attn / Clamp | 1,230 | 13 | 32688 | 32783 | 95 | 1.00 |
| denoise / transformer_blocks.0.attn / Clamp | 1,260 | 11 | 30220 | 30297 | 77 | 1.00 |
| denoise / transformer_blocks.0.attn / Clamp | 1,269 | 7 | 30258 | 30335 | 77 | 1.00 |
| denoise / transformer_blocks.0.attn / Reshape | 1,270 | 5 | 6174 | 6242 | 68 | 1.01 |
| denoise / transformer_blocks.0.mlp / Clamp | 2,8 | 5 | 30489 | 30564 | 75 | 1.00 |
| denoise / transformer_blocks.0.mlp / Clamp | 2,70 | 11 | 11084 | 11157 | 73 | 1.01 |
| denoise / transformer_blocks.0.mlp / Clamp | 2,79 | 31 | 31520 | 31595 | 75 | 1.00 |
| denoise / transformer_blocks.0.mlp / Clamp | 2,118 | 31 | 6174 | 6242 | 68 | 1.01 |
| denoise / transformer_blocks.0.mlp / Clamp | 2,135 | 11 | 44981 | 45070 | 89 | 1.00 |
| denoise / transformer_blocks.0.mlp / Clamp | 2,144 | 7 | 30641 | 30716 | 75 | 1.00 |
| denoise / transformer_blocks.0.mlp / Clamp | 2,206 | 7 | 11084 | 11157 | 73 | 1.01 |
| denoise / transformer_blocks.0.mlp / Clamp | 2,215 | 19 | 31672 | 31747 | 75 | 1.00 |
| denoise / transformer_blocks.0.mlp / Clamp | 2,254 | 17 | 6174 | 6242 | 68 | 1.01 |
| denoise / transformer_blocks.0.mlp / Clamp | 2,271 | 9 | 45133 | 45222 | 89 | 1.00 |
| denoise / transformer_blocks.0.mlp / Reshape | 2,272 | 5 | 6174 | 6242 | 68 | 1.01 |
| denoise / transformer_blocks.1.attn / Clamp | 3,8 | 5 | 30508 | 30583 | 75 | 1.00 |
| denoise / transformer_blocks.1.attn / Clamp | 3,17 | 5 | 29572 | 29647 | 75 | 1.00 |
| denoise / transformer_blocks.1.attn / Clamp | 3,79 | 11 | 11084 | 11157 | 73 | 1.01 |
| denoise / transformer_blocks.1.attn / Clamp | 3,140 | 9 | 11084 | 11157 | 73 | 1.01 |
| denoise / transformer_blocks.1.attn / Transpose | 3,161 | 13 | 28294 | 28369 | 75 | 1.00 |
| denoise / transformer_blocks.1.attn / Transpose | 3,182 | 13 | 28294 | 28369 | 75 | 1.00 |
| denoise / transformer_blocks.1.attn / Transpose | 3,203 | 15 | 28294 | 28369 | 75 | 1.00 |
| denoise / transformer_blocks.1.attn / Clamp | 3,210 | 19 | 22868 | 22953 | 85 | 1.00 |
| denoise / transformer_blocks.1.attn / ReduceMax | 3,211 | 7 | 28760 | 28851 | 91 | 1.00 |
| denoise / transformer_blocks.1.attn / Clamp | 3,221 | 5 | 29742 | 29834 | 92 | 1.00 |
| denoise / transformer_blocks.1.attn / Clamp | 3,230 | 15 | 32688 | 32783 | 95 | 1.00 |
| denoise / transformer_blocks.1.attn / Clamp | 3,250 | 11 | 30220 | 30297 | 77 | 1.00 |
| denoise / transformer_blocks.1.attn / Reshape | 3,251 | 7 | 6174 | 6242 | 68 | 1.01 |
| denoise / transformer_blocks.1.attn / Reshape | 3,252 | 5 | 6174 | 6242 | 68 | 1.01 |
| denoise / transformer_blocks.1.mlp / Clamp | 4,8 | 5 | 30489 | 30564 | 75 | 1.00 |
| denoise / transformer_blocks.1.mlp / Clamp | 4,70 | 11 | 11084 | 11157 | 73 | 1.01 |
| denoise / transformer_blocks.1.mlp / Clamp | 4,79 | 29 | 31520 | 31595 | 75 | 1.00 |
| denoise / transformer_blocks.1.mlp / Clamp | 4,118 | 31 | 6174 | 6242 | 68 | 1.01 |
| denoise / transformer_blocks.1.mlp / Clamp | 4,135 | 11 | 44981 | 45070 | 89 | 1.00 |
| denoise / transformer_blocks.1.mlp / Reshape | 4,136 | 9 | 6174 | 6242 | 68 | 1.01 |
| denoise / transformer_blocks.1.mlp / Reshape | 4,137 | 5 | 6174 | 6242 | 68 | 1.01 |
| denoise / post / Clamp | 5,8 | 5 | 28983 | 29058 | 75 | 1.00 |
| denoise / post / Clamp | 5,69 | 11 | 11084 | 11157 | 73 | 1.01 |
| denoise / post / Clamp | 5,78 | 5 | 26946 | 27021 | 75 | 1.00 |
| denoise / post / StateWrite | 5,100 | 5 | 11903 | 11979 | 76 | 1.01 |
| vae.in / pre / Reshape | 0,0 | 3 | 5470 | 5794 | 324 | 1.06 |
| vae.in / post / Reshape | 1,54 | 5 | 12832 | 15051 | 2219 | 1.17 |
| vae.mid.r0 / pre / Reshape | 0,0 | 4 | 5470 | 5538 | 68 | 1.01 |
| vae.mid.r0 / post / Concat | 1,13 | 3 | 16552 | 22451 | 5899 | 1.36 |
| vae.mid.r0 / post / Clamp | 1,71 | 4 | 12720 | 12798 | 78 | 1.01 |
| vae.mid.r0 / post / Clamp | 1,96 | 5 | 5662 | 5730 | 68 | 1.01 |
| vae.mid.r0 / post / Reshape | 1,144 | 4 | 43780 | 43899 | 119 | 1.00 |
| vae.mid.r0 / post / Concat | 1,158 | 4 | 16552 | 22451 | 5899 | 1.36 |
| vae.mid.r0 / post / Clamp | 1,216 | 4 | 12720 | 12798 | 78 | 1.01 |
| vae.mid.r0 / post / Clamp | 1,241 | 5 | 5662 | 5730 | 68 | 1.01 |
| vae.mid.r0 / post / Reshape | 1,289 | 4 | 32164 | 43899 | 11735 | 1.36 |
| vae.mid.r0 / post / Clamp | 1,301 | 5 | 6388 | 6457 | 69 | 1.01 |
| vae.mid.at / pre / Reshape | 0,0 | 5 | 5662 | 5730 | 68 | 1.01 |
| vae.mid.at / post / Concat | 1,13 | 3 | 16552 | 22451 | 5899 | 1.36 |
| vae.mid.at / post / Clamp | 1,71 | 4 | 12744 | 12822 | 78 | 1.01 |
| vae.mid.at / post / Clamp | 1,82 | 5 | 33355 | 34210 | 855 | 1.03 |
| vae.mid.at / post / Clamp | 1,91 | 5 | 32907 | 34210 | 1303 | 1.04 |
| vae.mid.at / post / Clamp | 1,100 | 4 | 32907 | 34210 | 1303 | 1.04 |
| vae.mid.at / post / Clamp | 1,107 | 10 | 17278 | 17362 | 84 | 1.00 |
| vae.mid.at / post / ReduceMax | 1,108 | 3 | 51400 | 51531 | 131 | 1.00 |
| vae.mid.at / post / Clamp | 1,118 | 3 | 52126 | 52258 | 132 | 1.00 |
| vae.mid.at / post / Clamp | 1,127 | 4 | 55756 | 55893 | 137 | 1.00 |
| vae.mid.at / post / Clamp | 1,136 | 5 | 22093 | 23381 | 1288 | 1.06 |
| vae.mid.at / post / Clamp | 1,150 | 5 | 17278 | 17362 | 84 | 1.00 |
| vae.mid.r1 / pre / Reshape | 0,0 | 4 | 5662 | 5730 | 68 | 1.01 |
| vae.mid.r1 / post / Concat | 1,13 | 3 | 16552 | 22451 | 5899 | 1.36 |
| vae.mid.r1 / post / Clamp | 1,71 | 5 | 12528 | 12798 | 270 | 1.02 |
| vae.mid.r1 / post / Clamp | 1,96 | 4 | 5662 | 5730 | 68 | 1.01 |
| vae.mid.r1 / post / Reshape | 1,144 | 4 | 31908 | 43899 | 11991 | 1.38 |
| vae.mid.r1 / post / Concat | 1,158 | 3 | 16552 | 22451 | 5899 | 1.36 |
| vae.mid.r1 / post / Clamp | 1,216 | 5 | 12528 | 12798 | 270 | 1.02 |
| vae.mid.r1 / post / Clamp | 1,241 | 4 | 5662 | 5730 | 68 | 1.01 |
| vae.mid.r1 / post / Reshape | 1,289 | 5 | 43524 | 43899 | 375 | 1.01 |
| vae.mid.r1 / post / Clamp | 1,301 | 5 | 6388 | 6457 | 69 | 1.01 |
| vae.up0.r0 / pre / Reshape | 0,0 | 4 | 5662 | 5730 | 68 | 1.01 |
| vae.up0.r0 / post / Concat | 1,13 | 3 | 16552 | 22451 | 5899 | 1.36 |
| vae.up0.r0 / post / Clamp | 1,71 | 5 | 12534 | 12804 | 270 | 1.02 |
| vae.up0.r0 / post / Clamp | 1,96 | 5 | 5662 | 5730 | 68 | 1.01 |
| vae.up0.r0 / post / Reshape | 1,144 | 4 | 31916 | 43907 | 11991 | 1.38 |
| vae.up0.r0 / post / Concat | 1,158 | 3 | 16552 | 22451 | 5899 | 1.36 |
| vae.up0.r0 / post / Clamp | 1,216 | 5 | 12534 | 12804 | 270 | 1.02 |
| vae.up0.r0 / post / Clamp | 1,241 | 5 | 5662 | 5730 | 68 | 1.01 |
| vae.up0.r0 / post / Reshape | 1,289 | 4 | 43532 | 43907 | 375 | 1.01 |
| vae.up0.r0 / post / Clamp | 1,301 | 5 | 6388 | 6457 | 69 | 1.01 |
| vae.up0.r1 / pre / Reshape | 0,0 | 4 | 5662 | 5730 | 68 | 1.01 |
| vae.up0.r1 / post / Concat | 1,13 | 3 | 16552 | 22451 | 5899 | 1.36 |
| vae.up0.r1 / post / Clamp | 1,71 | 4 | 12534 | 12804 | 270 | 1.02 |
| vae.up0.r1 / post / Clamp | 1,96 | 5 | 5662 | 5730 | 68 | 1.01 |
| vae.up0.r1 / post / Reshape | 1,144 | 3 | 31916 | 43907 | 11991 | 1.38 |
| vae.up0.r1 / post / Concat | 1,158 | 3 | 16552 | 22451 | 5899 | 1.36 |
| vae.up0.r1 / post / Clamp | 1,216 | 5 | 12534 | 12804 | 270 | 1.02 |
| vae.up0.r1 / post / Clamp | 1,241 | 5 | 5662 | 5730 | 68 | 1.01 |
| vae.up0.r1 / post / Reshape | 1,289 | 4 | 43532 | 43907 | 375 | 1.01 |
| vae.up0.r1 / post / Clamp | 1,301 | 5 | 6388 | 6457 | 69 | 1.01 |
| vae.up0.us / pre / Reshape | 0,0 | 4 | 5598 | 5666 | 68 | 1.01 |
| vae.up0.us / post / Reshape | 1,50 | 10 | 19220 | 30171 | 10951 | 1.57 |
| vae.up1.r0 / pre / Reshape | 0,0 | 9 | 5662 | 5730 | 68 | 1.01 |
| vae.up1.r0 / post / Concat | 1,13 | 4 | 23960 | 24051 | 91 | 1.00 |
| vae.up1.r0 / post / Clamp | 1,71 | 9 | 17725 | 18065 | 340 | 1.02 |
| vae.up1.r0 / post / Clamp | 1,96 | 9 | 5790 | 5858 | 68 | 1.01 |
| vae.up1.r0 / post / Reshape | 1,144 | 7 | 46280 | 46911 | 631 | 1.01 |
| vae.up1.r0 / post / Concat | 1,158 | 3 | 14480 | 14559 | 79 | 1.01 |
| vae.up1.r0 / post / Clamp | 1,216 | 6 | 17597 | 18065 | 468 | 1.03 |
| vae.up1.r0 / post / Clamp | 1,241 | 6 | 5790 | 5858 | 68 | 1.01 |
| vae.up1.r0 / post / Reshape | 1,289 | 6 | 27248 | 27855 | 607 | 1.02 |
| vae.up1.r0 / post / Reshape | 1,302 | 7 | 20904 | 21503 | 599 | 1.03 |
| vae.up1.r0 / post / Clamp | 1,314 | 6 | 6580 | 6649 | 69 | 1.01 |
| vae.up1.r1 / pre / Reshape | 0,0 | 6 | 5790 | 5858 | 68 | 1.01 |
| vae.up1.r1 / post / Concat | 1,13 | 3 | 14480 | 14559 | 79 | 1.01 |
| vae.up1.r1 / post / Clamp | 1,71 | 7 | 17597 | 18065 | 468 | 1.03 |
| vae.up1.r1 / post / Clamp | 1,96 | 6 | 5790 | 5858 | 68 | 1.01 |
| vae.up1.r1 / post / Reshape | 1,144 | 6 | 27248 | 27855 | 607 | 1.02 |
| vae.up1.r1 / post / Concat | 1,158 | 3 | 14480 | 14559 | 79 | 1.01 |
| vae.up1.r1 / post / Clamp | 1,216 | 6 | 17533 | 18065 | 532 | 1.03 |
| vae.up1.r1 / post / Clamp | 1,241 | 7 | 5790 | 5858 | 68 | 1.01 |
| vae.up1.r1 / post / Reshape | 1,289 | 6 | 26992 | 27855 | 863 | 1.03 |
| vae.up1.r1 / post / Clamp | 1,301 | 6 | 6580 | 6649 | 69 | 1.01 |
| vae.out / pre / Reshape | 0,0 | 6 | 5662 | 5730 | 68 | 1.01 |
| vae.out / post / Concat | 1,13 | 4 | 12816 | 12895 | 79 | 1.01 |
| vae.out / post / Clamp | 1,71 | 6 | 15571 | 16231 | 660 | 1.04 |
| vae.out / post / Clamp | 1,96 | 6 | 5534 | 5602 | 68 | 1.01 |
| vae.out / post / Reshape | 1,144 | 4 | 23311 | 24494 | 1183 | 1.05 |
| vae.out / post / Transpose | 1,154 | 4 | 6858 | 6928 | 70 | 1.01 |
