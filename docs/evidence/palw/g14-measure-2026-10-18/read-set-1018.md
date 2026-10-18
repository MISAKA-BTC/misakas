# G14 read set of one disputed step, Huihui-Qwen3.5-2B 32K class (2026-10-18)

Measurement record. No rule, constant, fence, window or price changed. Recorder: `read_set_on_a_real_class` in
`misaka/palw/tir/exec/tests/frame_measure.rs` (test only). Raw: `raw/read-set-prefill4-decode2-1018.json`,
`raw/read-set-door-options-d2-1018.json`, partial decode-30 log `raw/read-set-decode30-partial-1018.log`.

## Status: partial

- DONE: the real-class recorder (uses the production builder `backend.cone_refutation` over the dense capture, reads the carried
  operands/params/prompt/decode of the refutation). Prefill 4 / decode 2, 6 positions, 114,664 step leaves, 11 commit kinds.
- NOT DONE: state and history kinds. This class has checkpoint interval 164 and h_tile 32, and the 6-position job contains no
  State or HistTile leaf. A decode-30 run (33 positions, first history tile complete) was started and killed after 7 of 11 kinds
  (too slow: one kind ran more than 30 minutes). Its partial numbers are in the raw log and are NOT a result. It already shows the
  read set grows with length (Reshape: up to 28 runs / 114 leaves at 33 positions, against 8 runs / 29 at 6 positions).
- NOT DONE: a checkpoint (State) leaf needs position >= 164, so it is out of reach for this recorder without much more memory.
- NOT DONE: a structural (params-free) analyser, dedup/signature/control/retention accounting, sparse fetch of model parts,
  the non-cone door terms per option, and the 32K maximum profile.
- Sampling: 4 disputed leaves per kind plus the kind's last leaf (44 trials). The maxima below are OBSERVED over these samples.
  They are not a safe upper bound for any kind.

## Model, class, artifact, hardware, trials

- Class: `Misakachain/Huihui-Qwen3.5-2B-abliterated-PALW` (PALWART3, 32K), artifact
  `huihui2b.class.palwtir` (2,449,908,224 bytes, scratchpad). Class id `tir-frame-measure-huihui2b.json`.
- Host: Apple M1 Max, 10 cores, 32 GiB. Release build, `CARGO_INCREMENTAL=0`, `-j 6`. Peak RSS of the decode-2 run: 3.14 GB.
- Trials: 44 cone refutations (decode 2). Runtime 13 s for the recorder, 15 s wall including the run.

## Observed read set, decode 2 (observed maximum over the sampled leaves of each kind)

| kind (block:prim) | kind leaves | sampled | max runs (256) | max leaves | max value bytes | max whole-run bytes | max wire bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| attn+mlp#1:Clamp | 17,040 | 5 | 3 | 97 | 24,832 | - | 490,349 |
| attn+mlp#1:Gather | 3,840 | 5 | 1 | 1 | 256 | - | 103,217 |
| attn+mlp#1:Reshape | 2,160 | 5 | 8 | 29 | 7,424 | - | 56,193 |
| gdn+mlp:Clamp | 51,840 | 5 | 3 | 97 | 24,832 | - | 490,789 |
| gdn+mlp:Concat | 8,640 | 5 | 1 | 1 | 256 | - | 36,449 |
| gdn+mlp:Div | 90 | 5 | 1 | 32 | 8,192 | - | 83,439 |
| gdn+mlp:Gather | 20,160 | 5 | 1 | 1 | 256 | - | 168,037 |
| gdn+mlp:Reshape | 8,640 | 5 | 8 | 27 | 4,992 | - | 55,644 |
| gdn+mlp:Select | 90 | 5 | 1 | 32 | 8,192 | - | 84,661 |
| post:Clamp | 2,004 | 5 | 2 | 32 | 8,192 | - | 1,169,221 |
| pre:Clamp | 160 | 5 | 0 | 0 | 0 | - | 107,496 |

Maximum over all sampled kinds: 8 runs, 97 leaves, 24,832 value bytes, 524,288 whole-run bytes (8 touched runs), 1,169,221 wire
bytes. The "max whole-run bytes" column is per kind and is only filled in the JSON. The whole-run number is the byte size of the
touched 256-leaf runs, and it is what a run-granular fetch would move.

Note: the kinds above are the 11 groups of commit points that exist in this program. The prim name is the commit node's
primitive, so most "Clamp" leaves are the output of a quantised matmul followed by a clamp (the MatMul node itself is not a commit
point).

## Door under each option (observed inputs, decode 2; `raw/read-set-door-options-d2-1018.json`)

Door = 103 fixed + 11,120 non-cone terms (trace tiles 4,209, output check 2,346, reorg restart 2,346, owner queue 1,536, descent
369, rows/tile tree 208, IDs 106) + cone term. Cone term = turns x turn span. Turn span = 1 + margin + transport, with
transport = ceil(56 chains x (ctrl x 14,320 + body) / 125,000), margin = 4 x (2 + ctrl), ctrl = 3 + parts (16,384 B parts).
This check reproduces 1,062,231 for the shipped 448 serial turns with the 2,122,389 B body.

| option (cone term only changed; non-cone terms held at shipped values) | door DAA | <= 3,120 |
|---|---:|:-:|
| current: 448 serial turns, body bound 2,122,389 | 1,062,231 | no |
| A. serial, measured max runs (8), bound body | 29,991 | no |
| B. serial, 8 runs, body = measured whole-run bytes (524,288) | 16,095 | no |
| C. one round, sparse leaves, body = measured max wire (1,169,221) | 12,537 | no |
| D. one round, sparse leaves, body = measured max value bytes (24,832) | 11,296 | no |
| E. A with material shared across the 7 custodians (not 56 chains) | 17,359 | no |
| F. C with material shared across custodians | 11,658 | no |
| G. A with all 8 runs in parallel (margin paid once, bytes shared) | 26,197 | no |
| H. C in parallel (one round, same as C here) | 12,537 | no |
| floor: cone term = 0 | 11,223 | no |

Reading: no option reaches 3,120, and not even the zero-cone floor does (11,223). The non-cone terms are 11,120 of that. The
cone-term options bring the door down from 1,062,231 to about 11,300 to 30,000. Reaching 3,120 needs the non-cone terms
(trace tiles 4,209, output/reorg 2,346 each, owner queue 1,536) to shrink too, and each of those is a functional change, not a
coefficient change. Those non-cone options were not computed here.

Assumptions and limits of the table:
- The 8 runs and 97 leaves are observed over 44 samples at 6 positions. The job at 33 positions already reached 28 runs, so the
  true maximum is larger. The table is not a bound.
- Bytes come from the observed refutation (decode 2). The 125,000 B/DAA ceiling and the control price are the shipped constants,
  used for sensitivity only.
- The per-run body in options B/C/D is an observed number, not the protocol body bound. Adopting it would be a rule change.

## Reproduction

```
MISAKA_TIR_FRAME_MEASURE_PALWTIR=<huihui2b.class.palwtir> MISAKA_READ_SET_OUT=<out.json> MISAKA_READ_SET_PER_KIND=4 \
MISAKA_READ_SET_DECODE=2 CARGO_INCREMENTAL=0 cargo test --release -j 6 -p misaka-palw-tir-exec --features node \
  --test frame_measure read_set_on_a_real_class -- --nocapture
python3 -I <scratchpad>/door/door.py <out.json>     # door options (the script is not in the repo; the formulas are above)
```
Decode-30 (partial, killed): same command with `MISAKA_READ_SET_DECODE=30`; about 4 GB RSS; slow.
