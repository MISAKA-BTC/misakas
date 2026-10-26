# M-SHADOW — paired claim comparison status

**NOT_MET.** The required experiment uses the **same claim** through the legacy Panel route and the Panel=1 kernel route, then compares the verdict, actual Final DAA, CPU, bytes and resource obligations for a correct claim and each of forged output, faked state, duplicate work and withheld randomness. No such paired run has been made on this source. Existing route-specific E2E tests do not establish equivalence, an empirical detection rate, or the costs of the attacks.

The SPINE lane must freeze the one class/job shape, and both paths must accept semantically equivalent input. Retain every mismatch as raw data. A Panel=1 test using a claim-derived interim lottery seed would also fail to establish unbiased post-commit selection; report the seed and selection assumptions separately.
