# M-SHADOW — paired claim comparison status

**NOT_MET.** The required experiment uses the **same claim** through the legacy Panel route and the Panel=1 kernel route, then compares the verdict, actual Final DAA, CPU, bytes and resource obligations for a correct claim and each of forged output, faked state, duplicate work and withheld randomness. No such paired run has been made on this source. Existing route-specific E2E tests do not establish equivalence, an empirical detection rate, or the costs of the attacks.

The SPINE checkpoint `ebb8b911f` selects card 1's genesis class and the single-prompt decode job from `canonical::claimed`. Both paths must accept semantically equivalent input; retain every mismatch as raw data. That checkpoint also reports that the kernel ledger can Final after a Panel pass and window without per-claim `SecurityCertified`. Until the selected-verifier approval is connected and measured, an apparent match of verdicts cannot be counted as safe Panel=1 equivalence. A Panel=1 test using a claim-derived interim lottery seed would likewise fail to establish unbiased post-commit selection; report the seed and selection assumptions separately.

## Source update inspected during continuation

Committed source `2119c943ba8872aadf7d5acd523bb4e595b21fb6` includes CERT `bc2c750f3`, adding the kernel certification row and held reward, later-anchor draw, producer release on certification and poster refund on failure. The lane reports two native integration tests passing. Thus the older checkpoint's "not implemented" limitation is no longer current for that source. This measurement branch has not built that source or run its same-claim twins; the required comparison remains **NOT_MET**. The fixed court timing executable remains on `84d718647`, which predates CERT.

Before selecting shadow parameters, resolve the [counter-unit finding](../approval-material.md): both draw consumers charge one network unit per claim, not per position. Do not run the comparison at an assumed 54-claim cap while describing its policy as 4,096 claim draws.

## Paired native intermediate harness

`canonical/measurement.rs` (test source through `94d08757d`) explicitly selects Panel=1 on both `Params` and the fold's state mirror. It asserts the resulting seat count. Separate fresh processes select 3 and 1 seats for identical fixture class/job/claim inputs. The runner compares all three IDs, verdict and actual Final DAA, retaining a mismatch as failure. N=0 constructs the same network/class/job/trace for a separate setup baseline. Measurements include native carrier admission, receipt construction/acceptance, public local check and proof ingress.

This is **interim kernel Panel=3 vs kernel Panel=1**, not the required legacy-model-route comparison. Receipts intentionally collude; their generation is not honest computation cost. Assignment uses the existing claim-derived interim seed. Gate C, post-commit randomness, and randomness withholding remain missing; unrevealed claim-seal expiry is reported as a separate limited case. Build and execution are pending at this checkpoint, so no equality or cost result is yet claimed.

## First native observations (retained failure evidence)

The native release build was verified at source `08a9cec0d`, binary SHA-256 `1836a9060680d6938a72dc581a71c6e6b30cb686617b04f435a8eb70cfc956ea`. [Build record](native-preflight-build-record.json).

1. [Parser preflight](raw/parser-preflight-2026-10-26/raw.json): the Rust N=0 test exited 0, but the collector failed to parse a JSON record on the same line as libtest's test-name prefix. The raw output is preserved; the collector was corrected and checked against this exact output.
2. [Different-schedule preflight](raw/different-schedule-preflight-2026-10-26/raw.json): all four fresh processes exited 0; actual seats were 3 and 1, both honest claims reached Final at DAA 54, and class/job IDs matched. **Claim IDs differed**, so the paired comparison stopped with **MISMATCH**, not PASS. Panel=3 claim begins `ead22b80`; Panel=1 begins `15bcb669`.

Source diagnosis: evidence headers bind `ClassRowV1.ruleset_digest` (`misaka/palw/kernel/src/ledger.rs`, `ClassRowV1::header`), and `KernelClaimV1::id` binds the evidence root. Changing the configured fork schedule changes that binding. A same-workload observation across two schedules is therefore not byte-identical claim evidence.

The revised experiment (`3872a488c`) uses one common fork schedule (Panel=1 from DAA 4), registers the same class before crossing, and admits the twins before/after the fence. It still checks class/job/claim identity. The intentionally shifted admission clocks are recorded: absolute Final DAA is retained for both, while the Final interval from receipt coverage must match. This does not erase the first mismatch, establish legacy-route equivalence, or add Gate C / post-commit beacon coverage. Its build/run is pending.

Per-phase `wall_ns` is elapsed time, not CPU time; `/usr/bin/time -l` supplies process CPU and RSS. `object_bytes` is Borsh PALW-object size for sends/receipts, inner prosecution bytes for proof ingress, or inner kernel object bytes for duplicate admission; it is not a complete transaction or transport-byte count. Zero in a phase without a supplied object count does not mean zero IO. The first single-process observations are under load and have no statistical p95 interpretation.
