# Pruned public input recovery — 2026-10-21

Source base: `c0137d340`. Candidate only; no activation, deployment or new capacity guarantee.

The isolated baseline [raw log](pruning-before.log) reproduces the real worker Honest assertion failure: 0 pass / 1 fail in 109.98 s. The initial [zero-match invocation](pruning-zero-match-attempt.log) ran zero tests and is not a pass.

The generic harness advanced 50 actual native blocks per 100 ms idle pass after a demand. Production workers tick every two seconds. After pruning at DAA 2,118 the harness could expire the original reservation before the restarted workers drained sixteen carrier/answer pairs. Only after actual pruning in `PrunedRound`, the harness now advances one ordinary native block per pass. Production workers, native signatures/admission/template/fold, original hard deadline, session/lane limits, the 360 s wall deadline and all strict assertions are unchanged. This is a test-clock pacing correction, not a production throughput repair or measured door bound.

[Targeted run](pruning-paced-targeted.log): 1 pass / 0 fail, 118.49 s. It actually prunes an accepted PromptIds chunk-zero body and acceptance row, restarts an empty verifier, drains sixteen distinct StepNode lanes, accepts exactly one signed numbered reacquisition with the original reservation, receives the automatic producer response, independently judges all three roots Honest, and carries signed release at DAA 2,347.

[Six-case actual-service selection](recovery-services.log): 6 pass / 0 fail / 0 ignored, 275.94 s. Covers reserved DA default, ordinary FP identity conviction, original-history pruning, partial transport restart, real selected-chain reorg and pruned-input reacquisition. Targeted1 overlaps the six and is not added. [Build, source and executable pins](pruning-build.json); [exact two-file patch](pruning-pacing.patch).

This closes this specific previously failing recovery case. It does not establish all-family G14, RFC-0026 T01–T15, colluding-owner shared ingress, pruned-network IBD, calibrated costs or real-model door ≤3,120.
