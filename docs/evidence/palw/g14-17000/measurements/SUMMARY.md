# G14 Panel=1 / DAA 17,000 measurement status (2026-10-26)

Existing core timing executable source: `84d7186471454c050c85fd3170b1ba9dea0d9987` (`codex/g14-panel1-measure` based on `claude/g14-panel1-17000`). This folder is measurement work and decision material, not policy adoption, arming record, full-activation pass or deployment. The measured Mac13,1 (32 GiB, 10 CPUs) is **not** the slowest supported node.

| Priority / ID | Actual result | Remaining work and time condition |
|---|---|---|
| P0 M-X3 | **NOT_MET**. Existing passive t12 184 s/DAA and 111,949 B/block are conversion inputs only. | SPINE fixed card 1 / single-prompt decode; still need its maximum profile, cold fetch, check, every court round, inclusion/reorg and real `c_retain` on minimum hardware. Duration cannot be estimated before artifact and source are fixed. [Detail](M-X3/README.md). |
| P0 M-C | **NOT_MET**. Source-pinned release harness built successfully (binary SHA-256 `bc13a045…`). The continuation adds five conditions (signature, root admission, D2 sweep 16/256/4096), 30 processes and 15,000 timed calls at load 7.870–12.180. All expected outcomes pass; the batch is UNDER_LOAD_FIXTURE_ONLY. The earlier forged-close operation ran with N=1000 ×3 and N=0 ×3 at load 8.07–15.72: median of process medians 383,708 ns, largest process p95 3,165,917 ns. It is **UNDER_LOAD**, excluded from arming. A 30-minute quiet batch held a build slot but ran zero operations because load never fell below 5 (minimum 5.253). Earlier load 51–61 run is also excluded. | A later native Gate C batch completed 120 processes (5/3, 75/41, 90/50; 20 N=0 and 20 N=1 each), all exit 0, under load 11.376–12.466. Complete the remaining kernel classes and run under load≤5; repeat on minimum hardware. The prior 14-operation batch took about 80 minutes under severe load, which is history rather than a quiet-run ETA. [Detail](M-C/README.md). |
| P0 M-CAP | **NOT_MET**. Eight native reduced-budget preflight processes pass under load 9.959–10.130. Six filings exercise cap 4, two over-budget drops, block reset and a later outsider conviction. Full 320-process batch also completed, all exit 0, under load 10.351–19.690; 20 repetitions per profile/count/baseline condition. No production-cap or simultaneous-court node run. | Complete F02/F03 fixture integration, M-C, minimum-node saturation/refill and restart/reorg/pruned drills. ETA unknown until those prerequisites finish. [Detail](M-CAP/README.md). |
| P0 M-SHADOW | **NOT_MET**. Limited kernel Panel=3/1 batch completed: 60 fresh processes, 15 pairs; claim IDs/verdicts and relative Final windows agree. Absolute Final is 54/57 after shifted admission; all differences retained. Under load 9.409–9.880; legacy-route, Gate C and randomness-withholding comparison remain absent. | Reviewed source `2119c943b` now includes kernel certification (`bc2c750f3`); its lane reports passing integration tests, not a paired measurement by this branch. Run honest + four attack twins on both routes after that gate, retaining every verdict/Final/cost mismatch. [Detail](M-SHADOW/README.md). |
| P0 M-COST | **NOT_MET**. Dolphin 1.5B 32k A16 artifact and 32,768-token input are now pinned; all existing artifact hashes match fresh reads. Pinned runtime preflight passed at N=0 and N=32 under load: 32-position forward 1.316720 s, peak RSS 2,085,158,912 B. The real 1.5B 8k run completed: forward 1,030.075 s, peak RSS 2.582 GiB, load 5.112–13.085. The 32k run is executing. 9B-8k has no artifact (user confirmed). | Complete 32k A16 execution; keep it separate from the K2 fixture slice. Court/transfer/DA economics remain separate measurements. [Detail](M-COST/README.md). |
| P1 M-P15 | **NOT_RUN**. | Original admission curves on isolated local devnet, including restart/rejection and simultaneous load; after P0. |
| P1 M-L | **NOT_RUN**. | Rebuild and run court/carrier rejection and 1k–1M row clone harnesses after P0. |
| P1 M-DA | **NOT_RUN**. | Per-class material and retained bytes for fixture plus authenticated real artifact. |
| P1 M-STO | **NOT_RUN**. | M1–M9 isolated devnet runs, subject to free disk ≥30 GiB. |
| P2 M-OPS | **PUBLIC READ COMPLETE; economics NOT_MET**. | A second public wRPC read at 06:03:33 UTC reported t12 DAA 15,904 and a one-transaction sink block. Two snapshots give 165.4605 s/net DAA advance; this is not a latency bound. No operational cost, net income or capital cost can be inferred. Local participation awaits user approval. [Detail](M-OPS/README.md). |

At the first attempted P0 window the 1-minute load was approximately 12–21, above the instruction's 5 threshold; the shared buildslot was 4/4 occupied. The subsequent quiet attempt was 30 minutes at load 5.253–15.420 and produced no timing. No loaded court timing is treated as a policy value. The separate public observation includes its own UTC timestamps and pre/post load. Results that were not run are explicitly **NOT_MET/NOT_RUN**, never PASS.

The updated [decision material](approval-material.md) finds a **counter-unit mismatch**: source `2119c943b` charges 1 per claim draw, while VAL divides the 4,096 cap by 75 as though it counted positions. At 75/41 the implemented cap gives 0.059702 false certifications per 365 epochs under the stated model, above 0.001. Recommendation: retain 75/41 with **54 claim draws/epoch** (0.00078709); keeping 4,096 claim draws requires at least 90/50 under all three recorded criteria. The 89/50 alternative misses the 85% response criterion. These are arithmetic recommendations, not product changes or measured detection rates.

The 4-DAA inclusion estimate cannot establish timely delivery by dividing packet bytes by residual block space: a whole 34,000-byte carrier cannot fit in a 13,051-byte residual. Reserved priority or legal fragmentation and backlog must be demonstrated. The current slice list **includes claim hard deadline**, so M-X3 is still a prerequisite despite VAL's report calling it optional. V11's full-window modeled jam cost is 170.625 MSK; the older ≥300 claim is stale. No `k/D`, fee, bond, deadline or activation value is pinned by this measurement branch. [Record candidates](../records-candidates.md) retain empty evidence digests for unmet requirements. No public node/VPS was changed or accessed over SSH; no source or record was pinned in product code.

## Continuation validation

The measurement-runner classification checks pass **4/4** (high-load refusal, explicit loaded status, child failure, and quiet fixture never becoming arming evidence). The parameter audit's exact rational tail checks and exhaustive minimal-seat searches pass. `python3 docs/maintenance/check_docs.py` passes with zero new issues; 43 existing baseline references remain. Neither arithmetic nor these runner checks are a real-node campaign.

Raw Rust stdout logs retain their emitted trailing blank line byte-for-byte (covered by the manifest). `git diff --check` flags those raw EOF blanks; the check passes with only those `.stdout.log` files excluded. No raw log was normalized to silence that warning.

## Native and real-model continuation

Central source `2119c943b` is now merged into this branch at `99a5a7adc`. Test-only additions through `94d08757d` add: (1) an explicitly mirrored Panel=1 fence compared with interim kernel Panel=3 on identical class/job/claim IDs, and (2) real Dolphin 1.5B A16 graph-v7 execution from the existing 32k artifact. The initial native preflight revealed different claim IDs under different fork schedules; all logs are preserved and that pair is rejected as a same-claim comparison. The common-schedule, fork-crossing revision completed all 60 processes; identical claims/verdicts, 50-DAA Final windows, absolute Final 54/57. It remains a loaded kernel-only comparison. The real-model release binary is built from `3872a488c`, with successful N=0/N=32 resource preflights; 8k completed under load; 32k is in progress. The old core binary and its timings above remain identified by their own old source.

The native intermediate comparison is not the complete legacy-model-versus-kernel M-SHADOW: it excludes Gate C certification, post-commit beacon selection and randomness withholding. Its `withheld_reveal` case tests an unrevealed claim seal and is explicitly not a beacon-withholding result. Current interim challenge seeds are claim-derived. Runtime and script provenance accompany future results. [Dolphin inputs](M-COST/dolphin-input-inventory-2026-10-26.json) are verified now; the user instructed leaving 9B-8k unmeasured.


## Latest integration and open scope decision

Source `648544c46` was merged after the old measurement binaries were pinned. It adds delayed Panel=1 assignment from a later round anchor, kernel court funding/holding work, and policy approval constructors. New test harnesses now explicitly wait for the post-commit seat before accepting receipts; their release build from `8e96ab053` completed. The honest Panel=1 preflight failed to obtain an assignment after 32 attempts; its failure evidence is preserved and diagnosis is in progress. The dependent full latest-source sequence stopped. Old timing/equality results remain attached to their original sources.

The [latest X3 scope audit](M-X3/README.md#latest-source-scope-finding--648544c46) confirms that X3's native claim helper does not govern kernel ledger claims. Kernel settlement uses the signed paid lifetime. Retaining the provisional Δ=2,016 does not reconcile the kernel fence list with that contract; the slice owner must resolve scope before arming. The 75/41 approval constructor also retains the 4,096 claim-counter mismatch. No product policy, activation height or arming digest was changed by the measurement work.

## Full-cap and anchor-prerequisite follow-up

Harness commit `969afa31f` adds an explicit cfg(test) cap of 64 and an optional early activation of the already-shipped anchor-retention prerequisite in both fixture parameter mirrors. The approved external-gain bound is asserted at 500 MSK. Source remains frozen while its new build waits for a shared slot. The queued workflow first preserves a no-retention control, then runs independent shadow, Gate C and maximum-trace preflights. Only a passing preflight advances its own repeated batch. Six filings with cap 64 are not a saturation test; no production capacity or arming result is claimed.

The running real-model 32k process agrees with the finished 8k process at all 512 emitted chunk endpoints in their common prefix; [comparison and bounded logs](M-COST/raw/dolphin-prefix-8192-2026-10-26/comparison.json) are captured. The complete 32k result and the low-load court rerun remain pending.


## Shared native build checkpoint

The actual legacy/backend comparison (`d2982b9d0`) and independent-carrier capacity
experiment (`dfd2ccf6a`) are now merged into this measurement branch at
`c89034ce3`. The only merge conflict was the test module list; both modules remain.
Python syntax, documentation links and diff checks pass. Rust compilation and
runtime results are still pending.

Three unstarted native build jobs were consolidated into one `native-fullcap`
build. Their waiting processes, absent build records and FIFO tickets were checked
before replacing them; the first job's original FIFO timestamp was preserved.
The full-cap workflow and the new legacy/capacity runners consume the same pinned
binary after that build succeeds. No running model or other lane was stopped.
This avoids two duplicate consensus builds without removing any queued test scope.

The legacy preflight compares actual TIR backend work with a kernel Panel=1 job,
retaining separate route identities and Final DAAs. The capacity experiment mines
independent carrier UTXOs before offering up to 72 junk proofs and an outsider's
valid prosecution to the unchanged native template. It records observed
saturation rather than inferring it from the offered count. Neither prepared
harness establishes a passing measurement, continuous-refill service, honest
per-seat verification cost or Gate C. All five P0 arming requirements remain
NOT_MET.


Public deadline context at 19:42 JST: [read-only wRPC](M-OPS/README.md#evening-observation--104245-utc-1942-jst)
reports DAA 16,003, leaving 997 DAA to 17,000. The recent two-point average is
169.2198 seconds per net DAA advance; it is not an inclusion or wall-clock
activation guarantee. This observation does not complete any P0 requirement.
