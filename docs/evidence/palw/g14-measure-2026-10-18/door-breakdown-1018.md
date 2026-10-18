# G14 door breakdown, Huihui-Qwen3.5-2B 32K class (2026-10-18)

Analysis record. No rule, constant, price or window was changed. Source: `misaka/palw/tir/exec/tests/frame_measure.rs`,
test `door_breakdown_on_a_real_class`. Raw output: `door-breakdown-1018.json`.

Reproduce:
```
MISAKA_TIR_FRAME_MEASURE_PALWTIR=<huihui2b.class.palwtir> MISAKA_DOOR_BREAKDOWN_OUT=<out.json> CARGO_INCREMENTAL=0 \
cargo test --release -j 6 -p misaka-palw-tir-exec --features node --test frame_measure door_breakdown_on_a_real_class -- --nocapture
```

Geometry: leaves 114,664; decode 2; prefill 4; tiles 61; vocab 248,320. Config: acquisition_daa 1, inclusion_margin 4,
collection_daa 8, reorg_replay_moves 1. Clock: control_bytes 14,320, parallel_bytes_per_daa 125,000.

## Door, custodian upper 6 (collect_until - accepted = 1,062,231)

Door = prep(16) + (acq 1 + margin 4) + max(path + queue, 1) + max(collection 8, filing 82).

| term | formula | DAA | share |
|---|---|---:|---:|
| cone runs | 448 x 2,346 | 1,051,008 | 98.94% |
| trace tiles of one row | 61 x 69 | 4,209 | 0.40% |
| output / stop-rule check | 1 x 2,346 | 2,346 | 0.22% |
| reorg restart | 1 x 2,346 | 2,346 | 0.22% |
| owner queue wait | ceil(41/8) x 256 | 1,536 | 0.14% |
| step-tree descent | 3 x 123 | 369 | 0.03% |
| rows + tile-tree descent | 2 x 51 + 2 x 53 | 208 | 0.02% |
| leaf / IDs | 55 + 51 | 106 | 0.01% |
| filing allowance | max(8, 82) | 82 | 0.01% |
| preparation | | 16 | |
| open window | 1 + 4 | 5 | |
| **total** | | **1,062,231** | |

Note on the code: the door runs from claim acceptance to `collect_until`. It is the whole pursuit allowance, not the wait
before an accusation can open (`open_until` = 21).

## The cone term, per turn (one StepRun of 256 leaves, production bound)

- Response body bound 2,122,389 B = 256 leaves x 8,192 B (the class's widest leaf value bound), i.e. 130 parts of 16,384 B.
- Response-side controls 133 (1 response + 2 + 130 parts); request side 2.
- Serial dependency per turn: 4 x (2 + 133) = 540 DAA of margin.
- Contention: all = slots x K_owner = 7 x 8 = 56 concurrent owner-custodian chains. Transport = ceil((56 x 133 x 14,320 + 56 x 2,122,389) / 125,000) = 1,805 DAA:
  about 853 DAA of control carriers and 951 DAA of response bytes.
- Span = 1 + 540 + 1,805 = 2,346 DAA. Matches `tir_public_turn_span_v1` (asserted in the test).

## Lever table (computed, not applied)

| lever | formula | door DAA | fits 3,120 |
|---|---|---:|:-:|
| baseline (upper 6) | as shipped | 1,062,231 | no |
| upper 257 | slots 258 | 30,273,252 | no |
| cone runs = 1 (not a derivable value here; lower bound) | cone 1 | 13,569 | yes |
| cone runs = 448 (production = cap) | cone 448 | 1,062,231 | no |
| body = real widest leaf x 256 | run body 2,097,152 B | 1,047,381 | no |
| body = 0 (lower bound) | run body 0 | 29,625 | yes |
| only demanded custodians answer | slots 2 | 480,184 | no |
| owner queue wait removed | queue 0 | 1,060,695 | no |
| K_owner = 4 | all = slots x 4 | 654,906 | no |
| K_owner = 1 | all = slots x 1 | 349,193 | no |

Upper 257 is not the shipped frame upper (6). The "cone = 1" and "body = 0" rows are bounds below anything the protocol can
produce; they show what each term costs, not a proposal.

## Findings

1. The door is the cone term: 448 sequential runs, each priced at the full 56-chain transport. The cone run count
   448 = ceil(114,664 / 256) is the cap in `cone_runs_bound_v1` (`min(stretch/run + 1 + history, whole)`), and the
   cap binds for this class. The uncapped derived value was not computed here.
2. Response bytes (951 DAA per turn) and control carriers (853 DAA) are about equal, and both scale with K_owner x slots.
3. The margin chain (540 per turn) scales with parts = run bytes / 16,384, so it is independent of run size in total.
4. Open items (not done here): deriving the uncapped cone bound from the demand structure; checking whether
   `maximum_window_lanes_v1` overestimates; shared authenticated material across owners; the maximum 32K profile; the real
   step_check and filing measurements.

## Armed-horizon regression

With `maximum_door_daa = 3,120` the same class and job are refused with `DoorBeyondLiabilityHorizon { required: 1,062,231,
allowed: 3,120 }` (asserted in the test). The unlimited setting used for measurement is not arming evidence.
