# G14 cone runs: why 448, and what is a mis-count (2026-10-18)

Analysis record. No rule, constant, price, fence or window was changed; the door stays 1,062,231 DAA (`door-breakdown-1018.md`).
Huihui-Qwen3.5-2B 32K class, measured job prefill 4 / decode 2, 114,664 step leaves, 61 tiles, vocab 248,320.

## 1. Why cone_runs_bound_v1 reaches 448

`consensus/core/src/palw_tir_step_v1.rs`, `cone_runs_bound_v1(job, leaf_count, run)`:

- `per_position` = every occurrence's commits at the widest H (`block_leaves_at(block, last)`) + one checkpoint's fixed-state
  tiles + one history tile's leaves. This is the most leaves ANY single position holds.
- `stretch = (checkpoint_interval + h_tile + 1) x per_position` leaves, read as one contiguous run-count `ceil(stretch / 256) + 1`.
- history: `tiles = (window + C) / h_tile + 1` completed tiles, each `ceil(hist_leaves_per_tile / 256) + 1` runs.
- capped by `whole = ceil(leaf_count / 256)`.

For this job the stretch is wider than the job: `(C + h_tile + 1)` positions is at least the 6 positions the job has, so the
stretch covers every leaf of every position before the disputed one. Its run count therefore reaches the cap
`whole = ceil(114,664 / 256) = 448`. The cap binds. The uncapped value is not a derived fact for this job; it is
`stretch` in leaves, which is at least the whole job.

So 448 is a SUPERSET bound: "every leaf of every earlier position in the window might be read". It is safe, but it is not
the read set of one disputed step.

## 2. What the court actually reads (code)

`consensus/core/src/palw_tir_court_v1.rs`, `TirSource` (the court's evaluation source):

- `state()`: pos 0 is zero; a checkpoint position reads one checkpoint leaf; any other position REPLAYS (`StateSupply::Replay`),
  so the chain is the writer's nodes back to the last checkpoint.
- `hist_row()`: a row in a completed tile reads that tile's leaves at `tile_end` (one leaf per sub-lane chunk of each history
  instance); a row in the current tile reads its own commit node.
- `token()`: prompt or generated ids, not step leaves.
- Evaluation is `eval_demanded` element by element: it pulls only the operands its demanded elements need.

So the court reads sparse operands: per position, the nodes its demanded elements reach, not every leaf of that position.
Whether that sparse set is much smaller than the stretch cannot be decided from the bound alone. The real set is the
refutation's `operands` (`PalwTirConeRefutationV1::operands`, "every step leaf the evaluation reads, ascending leaf index").

Caveat on the measurement: `frame_measure.rs` runs the court half only for fixture classes with in-memory params (`MapParams`,
i128 per element). A 2B-parameter class needs far more than 32 GB for that. The step-leaf request set should not depend on
parameter values (the requests depend on the program's node structure and positions; values only feed arithmetic and
embedding-row choice, which are inventory leaves, not step leaves). That is an argument, not yet a measurement. A recorder on
`TirSource` (or a zero-valued DemandSource over the real class) would give the real read set; it was not built here.

## 3. Checked items

| item | finding | effect on door |
|---|---|---|
| stretch not clipped to `job.positions` | real over-count for jobs shorter than `C + h_tile + 1`; for this job the cap binds anyway | none for this class (448 either way) |
| completed-tile term `ceil(hist/256) + 1` per tile | a tile's sub-leaves are contiguous, so the term is a superset; minor | none here (no completed tile in a 6-position job) |
| `maximum_window_lanes_v1` | per run = `count x widest leaf` (2,048 lanes = 8,192 B) for all 256 leaves, or `spans x position lanes`, whichever is smaller; the widest-leaf-everywhere term overestimates when a run's leaves are smaller | not measured; body bound 2,122,389 B is near 256 x 8,192 |
| serial runs | `tir_public_localization_path_v1` adds the cone as `cone_runs` SERIAL turns, each at a full span | see section 4 |
| same material per custodian x owner | each of 56 chains (slots 7 x K_owner 8) carries its own full body | see section 4 |

## 4. Where the door comes from (computed here, from the constants above)

- Per cone turn: transport = `ceil(56 x (133 x 14,320 + 2,122,389) / 125,000)` = 854 (control) + 951 (bytes) = 1,805 DAA, plus
  540 serial margin.
- 448 serial turns: margin 448 x 540 = 241,920 DAA; transport 448 x 1,805 = 808,640 DAA; the door's cone term is the sum (1,051,008).
- Batching all 448 runs into one round removes the serial margin (about 241,920 DAA) but NOT the bytes: 448 x 1,805 = 808,640 DAA
  remains. Batching alone cannot reach 3,120.
- The byte wall is the run granularity. One leaf is 9,383 B, which is 4.2 DAA over 56 chains; a run is 2.12 MB, which is 951 DAA
  per turn over 56 chains. The door falls in proportion to the bytes of the set actually fetched.

## 5. Conclusion

- No provable mis-count large enough to change the door was found from code alone. The 448 is a safe superset; the reduction to
  the true read set is the lever that matters, and it must be measured on the real class before any rule is written.
- Real mis-count found (small, not applied): the stretch ignores `job.positions`. It is a fix for short jobs only; it needs a dormant
  fence and a drill, and was not implemented in this session.
- Not a mis-count, but the largest lever: fetch the read set as leaves (or sparse runs) instead of full 256-leaf runs, batched into
  one round. The door then scales with bytes actually needed (about 4.2 DAA per 9.4 KB leaf over 56 chains) plus one round's margin.
  This needs the measured read set (section 2) and an ADR.

## 6. Not done (needs the user or a measured read set)

- The real read set per disputed leaf (recorder on the real class, or a zero-valued DemandSource). Blocked on memory for the
  court's i128 params; a recorder that avoids params is the next step.
- The ADR with options (share authenticated material across owners and custodians; batch independent requests; parallel
  turns; sparse leaf fetch). Each has a door to compute once the read set is measured. Writing an ADR with unmeasured doors would
  be an unverified rule.
- The maximum 32K profile (prefill 4 / decode 2 is the measured job). Not measured here.
