# RFC-0041 — bond-free bootstrap verification (2026-10-31)

**Latest verified built candidate: C10 — PASS.** The earlier Prefix helper integration failure is resolved. The seven-package EVM-enabled build, RFC0041 core 3 tests, exact node test, and 11 related Prefix regressions passed.

| Candidate | Build | RFC0041 core | RFC0041 node | Related Prefix regressions |
| --- | --- | --- | --- | --- |
| C7 | 7 packages PASS | 3 PASS | 1 PASS | Not part of C7 selection |
| C10 | 7 packages PASS | 3 PASS | 1 PASS | 11 PASS |

[C10 results and scope](candidate-c10/README.md) / [C10 verification and SHA256](candidate-c10/verification.json).

The node path covers actual BASE payment, normally funded signed bonds and maturation, PALW/selected Panel, approved 75/41 authentication, reorg and same-DB restart. Funding comes from the owned genesis UTXO and real change; one BASE reward is not claimed to fund all eight bonds. The private harness limits and selected-verifier honest-stake premise remain explicit in the record.

The user requested continued concurrent editing with results retained per candidate. C10 uses preserved fixed executable copies and before/after source inventories. Its PASS does not extend to later edits. [C7 summary](candidate-c7-summary.md), [C7 verification](verification.json), its logs and hashes remain unchanged.
