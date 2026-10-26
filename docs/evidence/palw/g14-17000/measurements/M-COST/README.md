# M-COST — real-model long-context cost status

**NOT_MET as an arming requirement.** On 2026-10-26 the user confirmed that no 9B weights or 9B-8k `.palwtir` exist locally or on the checked hosts, and instructed measuring the available Dolphin 1.5B A16 artifact first. 9B-8k remains **not measured**. Codex did not access the public hosts.

## Pinned available input

[Local input inventory](dolphin-input-inventory-2026-10-26.json) records fresh byte counts and SHA-256 comparisons, not just the old manifests:

- Dolphin3.0-Qwen2.5-1.5B, source revision `60c996e28ebee1ea3256fa9d40c06e0b20ceca67`.
- Legacy A16 32k precise artifact: 1,811,942,348 B, SHA-256 `8d88abf0cf46d86bed52466328690dbfebf6b49efb2b278a9648f947c4843da6`; matches the existing conversion record.
- PALWART3 index: 5,433 B, SHA-256 `4c039d64aeb87802470522ef1904cc64ef1377c948d0b05bd3da45541f68b1b0`; segment: 1,826,164,766 B, SHA-256 `7c3d4b18919afc410b79dd8d36a9443df6d8d4d8f1161dfd8e3e08db31056223`; both match.
- Existing 32,768-token input, SHA-256 `41069932f1940b3d7a59b54647bff7fe81fc03de9582b999fe57c2e0bd119d2d`.

The index contains the `base0-a16` section. It is not a K2 program, and the canonical G14 harness currently registers `wide128_v1(7)`, a small K2 fixture. An A16 runtime result must not be relabeled as that kernel slice's native claim/court result. The artifact's recorded model-conversion diagnostics are not new model review or claim certification.

## Runtime preflight and long-context measurement

Test-only harness: `misaka/palw/base0/tests/g14_real_model_measure.rs`, built through the `misaka-palw-opv-meas` test target at source `3872a488c64e03abfcdac11624460dab89d11138`. The [build record](dolphin-build-record.json) pins binary SHA-256 `d709c5cbfebdfd3da7f64509f8d4856980fc68801853f298f4f01afb8c841ba1` and confirms unchanged non-documentation source during compilation. It validates the exact artifact hash, compiles the graph-v7 A16 profile against the actual artifact, and executes the existing token sequence in bounded 16-position chunks. Each chunk emits elapsed wall time, cache resident bytes, logical trace i32 bytes and a logits digest. Runtime uses 3 Rayon threads. N=0 measures setup separately. The runner records process CPU/RSS and load, and stops only its own process group if RSS exceeds 8 GiB or the 12-hour limit.

Both resource preflights exited 0. These are one process each on Mac13,1 (32 GiB, 10 CPUs), under load, with three Rayon threads; there is no repetition distribution or p95.

| Positions | Setup wall (s) | Forward wall (s) | Process real / user / sys (s) | Peak RSS (B) | Cache allocation (B) | Logical transient trace (B) | Load before → after |
|---|---:|---:|---|---:|---:|---:|---|
| 0 | 7.344532 | 0 | 12.99 / 12.26 / 0.39 | 1,887,535,104 | 0 | 0 | 5.943 → 5.500 |
| 32 | 7.252610 | 1.316720 | 13.91 / 15.80 / 0.43 | 2,085,158,912 | 1,835,008 | 279,636,992 | 5.500 → 5.173 |

[Raw N=0](raw/dolphin-preflight-n0-2026-10-26/raw.json) and [raw N=32](raw/dolphin-preflight-n32-2026-10-26/raw.json) retain commands, source/binary identity, CPU/RSS, load, setup/result JSON and stdout/stderr digests. Process real time above comes from `/usr/bin/time -l`; raw `wall_seconds` is the monitor's elapsed time, which includes up to five seconds of polling delay. Do not subtract that monitor value as compute time. Artifact and token hashes were checked before and after execution. Final N=32 logits SHA-256: `7b0ca035d1554cb91dff0507460586ebfe183129cc4026b078a9c16eb05c21f8`.

The 8,192-position run completed successfully; the 32,768-position run is now executing. Its completion is not claimed. Each 16-position chunk also computes its terminal logits, so the aggregate includes more output-head work than a single terminal-head invocation. N=32 must not be linearly extrapolated to 32k. Hashing the artifact warms filesystem cache; this is not a cold-transfer measurement. Logical trace bytes omit wire framing, openings and retention and are not serialized DA bytes/DAA. Court, cold retrieval, transfer, retention pricing, minimum-node results and 9B remain unmeasured.


## Completed 8k real-artifact run

The pinned binary above executed **8,192 positions in 512 chunks**, exit 0, with the same artifact/token hashes before and after. [Raw logs](raw/dolphin-long-n8192-2026-10-26/raw.json), [derived summary](raw/dolphin-long-n8192-2026-10-26/summary.json), [manifest](raw/dolphin-long-n8192-2026-10-26/manifest.json). The summary verifies log hashes, contiguous chunk ranges and exact sums against the harness result.

- Forward wall: **1,030.074612 s (17 min 10 s)**.
- Whole process real / user / sys: **1,044.36 / 2,751.09 / 29.36 s**. CPU time includes setup, hashing, trace handling and output; forward wall excludes hashing the final logits and printing each chunk.
- Maximum RSS: **2,772,631,552 B (2.582 GiB)**; maximum KV cache allocation: **469,762,048 B**.
- Observed 1-minute load: **5.112–13.085**. Status **UNDER_LOAD_RUNTIME_ONLY**; one full run, not a repeated-run p95 bound.
- Logical transient trace total: **71,587,069,952 B**. Traces are discarded after each chunk. This is neither retained disk usage nor serialized DA material; do not price it as DA bytes/DAA.
- Last logits SHA-256: `c5ba00975c75f02352cee724ec51cc1ece927bd8b01d1ec0093efede005bfc81`.

The first 1,024 positions took 53.018 s and the final 1,024 took 206.337 s. Both growing context and changing host load contribute; a constant per-token extrapolation to 32k is unsupported. Detailed 1,024-position windows preserve the observed growth, with within-run chunk quantiles labeled as descriptive. The 32k run uses the same binary/input and fresh process; results remain pending.

This is a real 1.5B artifact measurement, **not 9B-8k**, not K2 native claim validation, and not the full M-COST requirement. No claim monetary price or X3 deadline is derived from this CPU/RSS observation alone. Missing cold transfer, retained encoded material, court work and minimum-node observations stay unmeasured.

## 8k prefix consistency during the 32k run

A [captured comparison](raw/dolphin-prefix-8192-2026-10-26/comparison.json) at 09:27 UTC compared the finished 8k run against the first 8,192 positions of the still-running 32k process. All **512 chunk-terminal logits digests agree**, using identical artifact, token input, shape and binary identities. The byte-for-byte bounded stdout snapshots and their hashes are retained. This checks the emitted 16-position endpoints; it does not compare every position's logits or complete internal traces and does not certify a claim. Timing and allocation fields may differ. The remainder of 32k is still pending; the prefix result is not a completed 32k cost measurement.


## Completion-evidence validation

The collector now also checks that setup/result JSON agrees with the actual
stdout records, requested and executed position counts agree, every chunk has
exactly the requested width (except a final remainder), and the final logits
digest/cache allocation agree with the last chunk. Eight checks pass, including
negative cases with recomputed log hashes: missing chunks, mismatched final
records/digests/cache, wrong chunk width and a falsely enlarged requested scope
are rejected. The existing 8k summary regenerates byte-for-byte unchanged; N=0
remains a valid setup control. This strengthens evidence collection and adds no
new model execution or claim-certification result.
