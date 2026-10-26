# M-SHADOW — paired claim comparison status

**NOT_MET.** The required experiment uses the **same claim** through the legacy Panel route and the Panel=1 kernel route, then compares the verdict, actual Final DAA, CPU, bytes and resource obligations for a correct claim and each of forged output, faked state, duplicate work and withheld randomness. No such paired run has been made on this source. Existing route-specific E2E tests do not establish equivalence, an empirical detection rate, or the costs of the attacks.

The SPINE checkpoint `ebb8b911f` selects card 1's genesis class and the single-prompt decode job from `canonical::claimed`. Both paths must accept semantically equivalent input; retain every mismatch as raw data. That checkpoint also reports that the kernel ledger can Final after a Panel pass and window without per-claim `SecurityCertified`. Until the selected-verifier approval is connected and measured, an apparent match of verdicts cannot be counted as safe Panel=1 equivalence. A Panel=1 test using a claim-derived interim lottery seed would likewise fail to establish unbiased post-commit selection; report the seed and selection assumptions separately.

## Source update inspected during continuation

Committed source `2119c943ba8872aadf7d5acd523bb4e595b21fb6` includes CERT `bc2c750f3`, adding the kernel certification row and held reward, later-anchor draw, producer release on certification and poster refund on failure. The lane reports two native integration tests passing. Thus the older checkpoint's "not implemented" limitation is no longer current for that source. This measurement branch has not built that source or run its same-claim twins; the required comparison remains **NOT_MET**. The fixed court timing executable remains on `84d718647`, which predates CERT.

Before selecting shadow parameters, resolve the [counter-unit finding](../approval-material.md): both draw consumers charge one network unit per claim, not per position. Do not run the comparison at an assumed 54-claim cap while describing its policy as 4,096 claim draws.

## Paired native intermediate harness

`canonical/measurement.rs` (test source through `94d08757d`) explicitly selects Panel=1 on both `Params` and the fold's state mirror. It asserts the resulting seat count. Separate fresh processes select 3 and 1 seats for identical fixture class/job/claim inputs. The runner compares all three IDs, verdict and actual Final DAA, retaining a mismatch as failure. N=0 constructs the same network/class/job/trace for a separate setup baseline. Measurements include native carrier admission, receipt construction/acceptance, public local check and proof ingress.

This is **interim kernel Panel=3 vs kernel Panel=1**, not the required legacy-model-route comparison. Receipts intentionally collude; their generation is not honest computation cost. Assignment uses the existing claim-derived interim seed. Gate C, post-commit randomness, and randomness withholding remain missing; unrevealed claim-seal expiry is reported as a separate limited case. Build and execution are pending at this checkpoint, so no equality or cost result is yet claimed.
