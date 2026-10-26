# M-CAP (lane ARM-LEGACY, 2026-10-27): one-move court op, court processing time, Panel bind+license on the legacy route

**Status: UNDER_LOAD, DEV-MACHINE ONLY. Not a production pass and not a measured record.**

- Hardware: Apple M1 Max, 10 cores, 32 GiB (`hw.memsize` 34359738368), macOS (Darwin 25.6.0). This is **not minimum-spec hardware**.
- Source: branch `claude/g14-arm-legacy`, commit `2cb9bf02b` (test-side timing only; the code under test is unchanged from `56c5ca067` + `claude/g14-fam-legacy2`).
- Binary: `target/debug/deps/kaspa_consensus-fdfaf54499c073c5` (dev profile with `opt-level = 2` for kaspa-consensus / -core / hashes / muhash, `CARGO_PROFILE_DEV_DEBUG=0`). The sha256 `55491bd2bf356c03b02953ccfe7cc4788d97e2d27efc72499f3f55c43c6a9575`.
- Command: `cargo test -p kaspa-consensus --lib -j 4 -- --test-threads=2 --nocapture g14_close_legacy::legacy_v2 g14_close_legacy::g14_arm_legacy_d` (through `buildslot.sh`).
- Load: `uptime` before the run was 10.97 / 11.30 / 8.01 and after it 11.73 / 12.76 / 11.95. Other lanes' builds were running. The no-load rerun (`run.sh`: wait for load < 5, then 10 runs of the built binary, no build) was queued behind 8 build slots with load at 45.7, and was stopped. It never ran. **Waiver.**
- Raw log: `build-run-under-load.log` (this directory).

## Results (2 processes × 31 samples)

| Quantity | court capacity fence | median | p95 (nearest rank) | min / max |
|---|---|---|---|---|
| one-move verdict (`palw_shard_court_verdict_at_v2`, floor class, real rows, 35,669 B object) | off | 133.875 µs | 141.667 µs | 132.625 / 161.625 µs |
| same | on | 132.083 µs | 140.292 µs | 131.542 / 167.375 µs |

| Quantity (one sample per process) | fence off | fence on |
|---|---|---|
| Panel bind + license, 5 seats (wall) | 474.1 ms | 538.4 ms |
| pursuit + one-move court + kind-3 slashes (wall, includes block building in the harness) | 2,478 ms over 20 DAA | 2,342 ms over 20 DAA |
| licensed DAA / Final floor / window_court | 23 / 143 / 3,000 | 23 / 143 / 3,000 |

## V11 estimate (for the value ledger; the coordinator pins any record)

- The one-move class costs about 0.14 ms (p95) here. The ledger's cap of 42 one-move verdicts per block therefore costs about 6.0 ms of court CPU per block. That is about 0.6% of a 1-second block on this machine, so the measurement **does not contradict** the estimate. **Keep V11 as estimated: cap 42, session weight 13, op_work [5,13,1,11], ρ_court 3,125 sompi/DAA/unit.**
- The opening, dissection-move and close classes are **not measured** (no harness drives them). The working set is not measured. The slowest supported node is not measured.
- Revisit when any of these happens: (1) a no-load or minimum-spec run gives a one-move p95 above 2.4 ms (the point where 42 per block passes 10% of a block); (2) the opening, dissection or close classes are measured; (3) the maximum class profile (largest class, maximum rows) is measured. Any of these replaces this estimate.

## Hard deadline, Panel capacity, court processing time (estimates)

- Court processing time: one full pursuit (binding read, descent, terminal, one-move verdict, slashes) ran in **20 DAA**. That is far inside the Final floor (licence + 120) and window_court (3,000). Estimated margin: 20 DAA used out of ≥ 120 DAA available before the floor.
- Panel capacity: bind + license for one claim took about 0.5 s of wall time for 5 seats in the harness. The number of concurrent claims per Panel was **not measured**. **Waiver.**
- Hard-deadline bound: `palw_claim_hard_deadline_v1` is not armed on this harness, so the hard deadline was **not measured**. **Waiver.** The 20 DAA pursuit is the only figure here.
