# M-SHADOW — paired claim comparison status

**NOT_MET.** The required experiment uses the **same claim** through the legacy Panel route and the Panel=1 kernel route, then compares the verdict, actual Final DAA, CPU, bytes and resource obligations for a correct claim and each of forged output, faked state, duplicate work and withheld randomness. No such paired run has been made on this source. Existing route-specific E2E tests do not establish equivalence, an empirical detection rate, or the costs of the attacks.

The SPINE checkpoint `ebb8b911f` selects card 1's genesis class and the single-prompt decode job from `canonical::claimed`. Both paths must accept semantically equivalent input; retain every mismatch as raw data. That checkpoint also reports that the kernel ledger can Final after a Panel pass and window without per-claim `SecurityCertified`. Until the selected-verifier approval is connected and measured, an apparent match of verdicts cannot be counted as safe Panel=1 equivalence. A Panel=1 test using a claim-derived interim lottery seed would likewise fail to establish unbiased post-commit selection; report the seed and selection assumptions separately.

## Source update inspected during continuation

Committed source `2119c943ba8872aadf7d5acd523bb4e595b21fb6` includes CERT `bc2c750f3`, adding the kernel certification row and held reward, later-anchor draw, producer release on certification and poster refund on failure. The lane reports two native integration tests passing. Thus the older checkpoint's "not implemented" limitation is no longer current for that source. This measurement branch has now built the merged source and completed the limited kernel Panel=3/1 comparison below; the required legacy-route comparison remains **NOT_MET**. The fixed court timing executable remains on `84d718647`, which predates CERT.

Before selecting shadow parameters, resolve the [counter-unit finding](../approval-material.md): both draw consumers charge one network unit per claim, not per position. Do not run the comparison at an assumed 54-claim cap while describing its policy as 4,096 claim draws.

## Paired native intermediate harness

`canonical/measurement.rs` (test source through `94d08757d`) explicitly selects Panel=1 on both `Params` and the fold's state mirror. It asserts the resulting seat count. Separate fresh processes select 3 and 1 seats for identical fixture class/job/claim inputs. The runner compares all three IDs, verdict and actual Final DAA, retaining a mismatch as failure. N=0 constructs the same network/class/job/trace for a separate setup baseline. Measurements include native carrier admission, receipt construction/acceptance, public local check and proof ingress.

This is **interim kernel Panel=3 vs kernel Panel=1**, not the required legacy-model-route comparison. Receipts intentionally collude; their generation is not honest computation cost. Assignment uses the existing claim-derived interim seed. Gate C, post-commit randomness, and randomness withholding remain missing; unrevealed claim-seal expiry is reported as a separate limited case. Build and execution are pending at this checkpoint, so no equality or cost result is yet claimed.

## First native observations (retained failure evidence)

The native release build was verified at source `08a9cec0d`, binary SHA-256 `1836a9060680d6938a72dc581a71c6e6b30cb686617b04f435a8eb70cfc956ea`. [Build record](native-preflight-build-record.json).

1. [Parser preflight](raw/parser-preflight-2026-10-26/raw.json): the Rust N=0 test exited 0, but the collector failed to parse a JSON record on the same line as libtest's test-name prefix. The raw output is preserved; the collector was corrected and checked against this exact output.
2. [Different-schedule preflight](raw/different-schedule-preflight-2026-10-26/raw.json): all four fresh processes exited 0; actual seats were 3 and 1, both honest claims reached Final at DAA 54, and class/job IDs matched. **Claim IDs differed**, so the paired comparison stopped with **MISMATCH**, not PASS. Panel=3 claim begins `ead22b80`; Panel=1 begins `15bcb669`.

Source diagnosis: evidence headers bind `ClassRowV1.ruleset_digest` (`misaka/palw/kernel/src/ledger.rs`, `ClassRowV1::header`), and `KernelClaimV1::id` binds the evidence root. Changing the configured fork schedule changes that binding. A same-workload observation across two schedules is therefore not byte-identical claim evidence.

The revised experiment (`3872a488c`) uses one common fork schedule (Panel=1 from DAA 4), registers the same class before crossing, and admits the twins before/after the fence. It still checks class/job/claim identity. The intentionally shifted admission clocks are recorded: absolute Final DAA is retained for both, while the Final interval from receipt coverage must match. This does not erase the first mismatch, establish legacy-route equivalence, or add Gate C / post-commit beacon coverage. Its completed build/run is recorded below.

Per-phase `wall_ns` is elapsed time, not CPU time; `/usr/bin/time -l` supplies process CPU and RSS. `object_bytes` is Borsh PALW-object size for sends/receipts, inner prosecution bytes for proof ingress, or inner kernel object bytes for duplicate admission; it is not a complete transaction or transport-byte count. Zero in a phase without a supplied object count does not mean zero IO. The first single-process observations are under load and have no statistical p95 interpretation.


## Common-schedule native batch completed

[Build record](native-crossing-build-record.json): source `f665796d1aa759e0b3c4879c7deb39755e144fb1`, binary SHA-256 `942cb1484a6ee2648b554b2b3165e8b0688a1c3dc295258bc28e7665dc16f2b0`. Non-documentation source was unchanged during compilation. [Raw logs](raw/native-crossing-2026-10-26/raw.json), [descriptive summary](raw/native-crossing-2026-10-26/summary.json), [digest manifest](raw/native-crossing-2026-10-26/manifest.json).

All **60 fresh processes exited 0**: five cases × three repetitions × two seat policies × N=0/N=1. All 15 pairs have identical class/job/claim IDs and verdicts. Load ranged **9.409–9.880**, so the status is **UNDER_LOAD_FIXTURE_ONLY**, not an arming pass. Three observations per case/seat are descriptive and do not establish a p95 bound or attack detection probability.

| Case | Both routes' result | Panel=3 / Panel=1 Final DAA | Final minus coverage DAA |
|---|---|---|---|
| Honest | FINAL | 54 / 57 | 50 / 50 |
| Forged output | CONVICTED, no producer payout | none / none | not applicable |
| Fake state | CONVICTED, no producer payout | none / none | not applicable |
| Duplicate work | COPY_REJECTED_ORIGINAL_FINAL | 54 / 57 | 50 / 50 |
| Withheld claim reveal | UNREVEALED_SEAL_FORFEITED | none / none | not applicable |

Absolute Final times **differ**. Panel=3 admission/coverage occurs before/at the common fence, while Panel=1 admission/coverage occurs after it (coverage DAA 4 versus 7); this is an intentionally shifted clock, not evidence that absolute Final is equal. Every raw comparison retains that mismatch. The withheld reveal is a claim-seal test, **not randomness withholding**.

| Native phase, median of three fresh processes | Panel=3 wall ms | Panel=1 wall ms | Counted body bytes, Panel=3 / Panel=1 |
|---|---:|---:|---|
| Honest receipt construction/signing | 1.265208 | 0.944875 | 16,341 / 5,447 |
| Honest receipt acceptance | 6.855042 | 4.386041 | 16,341 / 5,447 |
| Honest outsider local check | 2.400041 | 2.297542 | uncounted |
| Forged-output proof seal + ingress | 9.103583 | 9.161209 | 150 / 150 inner prosecution |
| Fake-state proof seal + ingress | 9.644833 | 10.551959 | 1,410 / 1,410 inner prosecution |

These native phase timers include signing where named, carrier/mempool/template/fold work where invoked, and local simulated blocks; they are not isolated verifier CPU. The full summary preserves every phase, N=0 process RSS/CPU and maxima. The comparison excludes Gate C and real artifact execution. It does not establish network propagation, legacy-route equivalence, minimum-hardware capacity, or production policy safety.

## Post-commit-anchor preflight failed — 2026-10-26

The release build at `8e96ab0538e0be5de90b8096accf66edca61feb1` completed with unchanged source and binary SHA-256 `2f16ff06103eeed9cd78da888143093a000489a46d4e4807986dccdab479b9a7`. [Four-process preflight](raw/postcommit-preflight-failure-2026-10-26/raw.json), [build record](raw/postcommit-preflight-failure-2026-10-26/build-record.json), [manifest](raw/postcommit-preflight-failure-2026-10-26/manifest.json).

Panel=3 N=0/N=1 and Panel=1 N=0 exited 0. Panel=1 N=1 exited 101: `canonical/seed.rs::until_seated` exhausted 32 attempt blocks and panicked `no post-commit anchor seated the claim`. It emitted no completed measurement row. The collector correctly stopped with **FAILED**; no equality or timing result is claimed for the new pair. The dependent full comparison/Gate C/capacity sequence did not run. These failure logs remain intact even if a later run succeeds.

Diagnosis is in progress: run the integration source's own two `lane_seed_` tests on this fixed binary, and record per-attempt anchor, span, admitted-attempt presence and kernel claim state in the next test build. The test's 32-attempt limit and acceptance predicates are unchanged. Neither cause nor a product regression is established by the preflight alone.

### Integration source's seed tests also fail, at an earlier stage

The same pinned binary ran the existing `canonical::seed::lane_seed_` filter: **0 passed, 2 failed**, process exit 101. Both tests fail during `World::on`, at `g14_kernel_route_e2e.rs:764`, with `the class registered through the real path`; neither reaches its seed assertions. [Complete captured run](raw/seed-source-diagnostics-2026-10-26/raw.json), [stdout](raw/seed-source-diagnostics-2026-10-26/stdout.log), [stderr](raw/seed-source-diagnostics-2026-10-26/stderr.log). This is distinct from the measurement preflight's later failure to obtain an assignment. It establishes that the two source tests are not passing on this built source; it does not yet establish a common cause or an on-network failure.

The next test build adds failure-only per-attempt diagnostic detail without changing the 32-attempt limit, the draw predicate or production code. Separate shadow, Gate C and exact-maximum-trace capacity preflights will retain their individual outcomes instead of letting a shadow failure prevent collection of the other diagnostic results.
