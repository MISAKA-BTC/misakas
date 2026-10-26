# M-C — court operation measurement status

**NOT_MET for arming.** The test-only release harness `consensus/core/tests/palw_tir_court_verify_cpu.rs` built successfully at source commit `84d7186471454c050c85fd3170b1ba9dea0d9987` through the shared `buildslot.sh`; the pinned executable SHA-256 is `bc13a045eb79b795365c9f21050da122a2e3de479246c9e3beb2a02c9c1e4ac7`. The exact [build record](build-record.json) and [build log](build.log) give the command, source/test hashes, environment, exit 0 and binary path. The machine is Mac13,1 with 32 GiB RAM and 10 CPUs, **not** the minimum supported node. The observed 1-minute load before the build was 8.17. No unloaded sample has been collected in this branch. The 2026-10-22 table in `docs/evidence/palw/studio-values-2026-10-22/measurements.md` was measured at load 51–61 and is comparison history, not a fresh value or policy input.

The runner [run_court_op.py](../run_court_op.py) records the exact executable SHA-256, source/test hashes, command, environment, pre/post load, `/usr/bin/time -l` CPU and peak RSS, raw stdout/stderr and the harness's warm-up-separated median/p95/max over at least 20 operations. `MEASURE_N=0` is captured in separate processes as setup context. The [batch runner](../run_court_batch.py) held one shared slot from **04:01:58–04:31:59 UTC** and checked 1-minute load 61 times. The minimum was **5.253**, median **8.814**, maximum **15.420**; no reading met the ≤5 gate. It exited 75 with `NOT_RUN_LOAD` and **zero operations executed**. The [complete wait log](raw/quiet-attempt-2026-10-26/batch.json) has SHA-256 `b2834cfbd37198b358c338be1e75e66c878a7c2e68326ff5dbe9f7df06112e1c`. The per-operation runner's `--allow-under-load` mode is exploratory and marks the result `UNDER_LOAD`; arming evidence requires a quiet run and a repeat on the slowest supported node.

This preexisting harness still lacks the requested Panel=1 approval, post-commit challenge, constraint receipt, public fault proof, and outsider-ingress operation classes. Those must be added in test code and measured before a complete M-C table or `PALW_RFC0016_MEASURED_RECORDS_V1` / `PALW_RFC0608_MEASURED_RECORDS_V1` candidate can be claimed.

## Exploratory loaded sample — not an arming input

One operation completed on the pinned binary: `close_cone_forged_last_stage`, three fresh processes with `MEASURE_N=1000` and three separate `MEASURE_N=0` setup baselines. The first verification in each process is reported separately. All six processes had `under_load=true`; 1-minute load before/after ranged **8.07–15.72**. [Raw JSON and per-process stdout/stderr](raw/loaded-preliminary-close-cone-forged/raw.json) contain every command, exit 0, timestamp/load, CPU/RSS and harness line.
The raw JSON SHA-256 is `dbc1e9aa2c0baff470c9529f00d55572ede0ef5004a0767a4e2298b39c85efaf`; the build-record JSON SHA-256 is `d9e73954372ede293fec0185aa0b4067a6ebcb77ec23e6bbe306f784b4abdb6b`.

| Fixture operation | Median of three process medians | Largest process p95 | Largest item | Warm-up range | Exemplar body | Distinct fixtures | Median process peak RSS / N=0 RSS |
|---|---:|---:|---:|---:|---:|---:|---:|
| Forged close cone, last-stage mismatch | 383,708 ns | 3,165,917 ns | 15,553,792 ns | 219,125–223,542 ns | 10,753 B | 984 | 36,077,568 / 35,979,264 B |

The process medians were 389,875 / 383,708 / 373,709 ns; the p95 values were 3,165,917 / 2,137,917 / 1,191,792 ns. The median process RSS difference is 98,304 B, only a descriptive difference between independent processes, not an isolated verifier working set. The harness measured wall elapsed time on small fixtures and did not measure a real model or maximum profile. The high-load p95/max are scheduling-sensitive; no cost, capacity, deadline or fee policy may adopt them. The quiet attempt above ended without a run.

## Continuation batch — five conditions, 2026-10-26

**UNDER_LOAD_FIXTURE_ONLY; NOT_MET for arming.** The same pinned release binary ran under one shared build slot from 15:10:24 to 15:13:16 JST. Five conditions each have three N=0 processes and three N=1000 processes (30 processes; 15,000 timed calls including warm-ups). Every expected outcome passed. Observed pre/post 1-minute load was **7.870–12.180**, always above 5. This is Mac13,1 fixture timing, not minimum hardware or the newly integrated CERT implementation.

[Raw per-process data and batch](raw/continuation-2026-10-26/batch.json), [summary](raw/continuation-2026-10-26/summary.json), [SHA-256 manifest](raw/continuation-2026-10-26/manifest.json). The batch command used `--allow-under-load --n 1000 --case mldsa87_verify --case root_claim_admit --case d2_sweep:16 --case d2_sweep:256 --case d2_sweep:4096`; each child records its full command and environment. Runner change: `79f64c05c`.

| Operation / scale | Median of process medians (µs) | Largest process p95 (µs) | Largest item (µs) | Body bytes | Median process RSS / N=0 RSS (bytes) |
|---|---:|---:|---:|---:|---:|
| mldsa87_verify | 126.250 | 134.333 | 218.292 | 4,627 | 13,729,792 / 13,631,488 |
| root_claim_admit | 1437.791 | 1523.291 | 2916.458 | 16,571 | 16,433,152 / 16,285,696 |
| d2_sweep / 16 | 0.750 | 0.792 | 0.958 | 0 | 12,566,528 / 12,500,992 |
| d2_sweep / 256 | 9.333 | 9.500 | 64.459 | 0 | 12,713,984 / 12,943,360 |
| d2_sweep / 4096 | 216.416 | 237.667 | 417.958 | 0 | 19,283,968 / 15,695,872 |

The signature rows cycle 64 fixtures, root admission cycles 96, and each D2 scale repeats a sweep of one unchanged registry. N=1000 includes one separately reported warm-up per process, leaving 999 items for its distribution. The D2 body-byte value 0 means this internal sweep has no incoming exemplar body; it does not mean zero computation, writes or retention cost. Signature bytes alone exclude public key and carrier envelope. These are wall elapsed times, not isolated CPU time. Process CPU/RSS includes setup; the negative RSS difference at scale 256 demonstrates why independent-process subtraction is not an isolated working-set measurement.

The three D2 scales provide actual observations of growth, but scheduling load, cache reuse and fixture-only state prevent treating their ratios as an arming weight or a node saturation result. The old core binary does not cover the five requested native classes. The later native-path observations below cover part of that gap, with different timing boundaries and only three repetitions.


## Later native-path observations

The [kernel Panel=3/1 batch](../M-SHADOW/README.md#common-schedule-native-batch-completed) now measures receipt construction/acceptance, outsider local checking, and public-fault seal/ingress through the native processor at source `f665796d1`. It retains N=0 setup processes and all per-phase timings, but has only three repetitions per case/seat, load 9.409–9.880 and a small fixture. These composite native phases are not interchangeable with the old core harness's isolated verifier operations. No complete quiet M-C table is claimed.

A further test-only harness at source `209ea76ee` measures Panel=1 plus Gate C with 5/3, 75/41 and 90/50 draw/approval thresholds, N=0 uncertified-Final baselines, and native later-anchor/draw, attestation and settlement phases. Its policies are unapproved fixture configurations. The eight harness bonds cannot establish independent responding operators; synthetic signed attestations do not measure honest verifier model recomputation. The binary is building; the runner is scheduled for 20 fresh N=0 and N=1 processes at each scale. No result is claimed until it executes.
