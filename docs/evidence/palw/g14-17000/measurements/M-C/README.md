# M-C — court operation measurement status

**NOT_MET for arming.** The test-only release harness `consensus/core/tests/palw_tir_court_verify_cpu.rs` built successfully at source commit `84d7186471454c050c85fd3170b1ba9dea0d9987` through the shared `buildslot.sh`; the pinned executable SHA-256 is `bc13a045eb79b795365c9f21050da122a2e3de479246c9e3beb2a02c9c1e4ac7`. The exact [build record](build-record.json) and [build log](build.log) give the command, source/test hashes, environment, exit 0 and binary path. The machine is Mac13,1 with 32 GiB RAM and 10 CPUs, **not** the minimum supported node. The observed 1-minute load before the build was 8.17 and during the build/queued run was above the instruction's 5 threshold. No unloaded sample has been collected in this branch yet. The 2026-10-22 table in `docs/evidence/palw/studio-values-2026-10-22/measurements.md` was measured at load 51–61 and is comparison history, not a fresh value or policy input.

The runner [run_court_op.py](../run_court_op.py) records the exact executable SHA-256, source/test hashes, command, environment, pre/post load, `/usr/bin/time -l` CPU and peak RSS, raw stdout/stderr and the harness's warm-up-separated median/p95/max over at least 20 operations. `MEASURE_N=0` is captured in separate processes as setup context. The [batch runner](../run_court_batch.py) has entered a shared build slot and is waiting for load ≤5 before launching the first operation. The per-operation runner's `--allow-under-load` mode is exploratory and marks the result `UNDER_LOAD`; arming evidence requires a quiet run and a repeat on the slowest supported node.

This preexisting harness still lacks the requested Panel=1 approval, post-commit challenge, constraint receipt, public fault proof, and outsider-ingress operation classes. Those must be added in test code and measured before a complete M-C table or `PALW_RFC0016_MEASURED_RECORDS_V1` / `PALW_RFC0608_MEASURED_RECORDS_V1` candidate can be claimed.

## Exploratory loaded sample — not an arming input

One operation completed on the pinned binary: `close_cone_forged_last_stage`, three fresh processes with `MEASURE_N=1000` and three separate `MEASURE_N=0` setup baselines. The first verification in each process is reported separately. All six processes had `under_load=true`; 1-minute load before/after ranged **8.07–15.72**. [Raw JSON and per-process stdout/stderr](raw/loaded-preliminary-close-cone-forged/raw.json) contain every command, exit 0, timestamp/load, CPU/RSS and harness line.
The raw JSON SHA-256 is `dbc1e9aa2c0baff470c9529f00d55572ede0ef5004a0767a4e2298b39c85efaf`; the build-record JSON SHA-256 is `d9e73954372ede293fec0185aa0b4067a6ebcb77ec23e6bbe306f784b4abdb6b`.

| Fixture operation | Median of three process medians | Largest process p95 | Largest item | Warm-up range | Exemplar body | Distinct fixtures | Median process peak RSS / N=0 RSS |
|---|---:|---:|---:|---:|---:|---:|---:|
| Forged close cone, last-stage mismatch | 383,708 ns | 3,165,917 ns | 15,553,792 ns | 219,125–223,542 ns | 10,753 B | 984 | 36,077,568 / 35,979,264 B |

The process medians were 389,875 / 383,708 / 373,709 ns; the p95 values were 3,165,917 / 2,137,917 / 1,191,792 ns. The median process RSS difference is 98,304 B, only a descriptive difference between independent processes, not an isolated verifier working set. The harness measured wall elapsed time on small fixtures and did not measure a real model or maximum profile. The high-load p95/max are scheduling-sensitive; no cost, capacity, deadline or fee policy may adopt them. The quiet batch remains pending.
