# M-COST — real-model long-context cost status

**NOT_MET as an arming requirement.** On 2026-10-26 the user confirmed that no 9B weights or 9B-8k `.palwtir` exist locally or on the checked hosts, and instructed measuring the available Dolphin 1.5B A16 artifact first. 9B-8k remains **not measured**. Codex did not access the public hosts.

## Pinned available input

[Local input inventory](dolphin-input-inventory-2026-10-26.json) records fresh byte counts and SHA-256 comparisons, not just the old manifests:

- Dolphin3.0-Qwen2.5-1.5B, source revision `60c996e28ebee1ea3256fa9d40c06e0b20ceca67`.
- Legacy A16 32k precise artifact: 1,811,942,348 B, SHA-256 `8d88abf0cf46d86bed52466328690dbfebf6b49efb2b278a9648f947c4843da6`; matches the existing conversion record.
- PALWART3 index: 5,433 B, SHA-256 `4c039d64aeb87802470522ef1904cc64ef1377c948d0b05bd3da45541f68b1b0`; segment: 1,826,164,766 B, SHA-256 `7c3d4b18919afc410b79dd8d36a9443df6d8d4d8f1161dfd8e3e08db31056223`; both match.
- Existing 32,768-token input, SHA-256 `41069932f1940b3d7a59b54647bff7fe81fc03de9582b999fe57c2e0bd119d2d`.

The index contains the `base0-a16` section. It is not a K2 program, and the canonical G14 harness currently registers `wide128_v1(7)`, a small K2 fixture. An A16 runtime result must not be relabeled as that kernel slice's native claim/court result. The artifact's recorded model-conversion diagnostics are not new model review or claim certification.

## Measurement in progress

Test-only harness: `misaka/palw/base0/tests/g14_real_model_measure.rs` (source `94d08757d`). It validates the exact artifact hash, compiles the graph-v7 A16 profile against the actual artifact, and executes the existing token sequence in bounded 16-position chunks. Each chunk emits elapsed wall time, cache resident bytes, logical trace i32 bytes and a logits digest. Runtime uses 3 Rayon threads. N=0 measures setup separately. The runner records process CPU/RSS and load, and stops only its own process group if RSS exceeds 8 GiB or the 12-hour limit.

First run a short segment, then 8k and 32k when resource observations permit. At this checkpoint the binary is still building and **no new model execution result exists**. Hashing the artifact warms filesystem cache; this is not a cold-transfer measurement. Logical trace bytes omit wire framing, openings and retention and are not serialized DA bytes/DAA. Court, cold retrieval, transfer, retention pricing, minimum-node results and 9B remain unmeasured.
