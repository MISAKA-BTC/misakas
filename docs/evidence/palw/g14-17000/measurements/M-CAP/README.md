# M-CAP — court capacity status

**NOT_MET.** `PalwCourtMeasurementV1` requires every operation class's time, bytes and working set on the slowest supported node, the block budget, `rho_court` and session weight. The current Mac13,1 is not that node. No simultaneous-court node run, saturated/refilling coalition drill, reservation-floor check, or restart/reorg/pruned replay has been performed here.

The earlier `palw_court_op_timing_harness` run failed during its test-only configuration setup with `OneOutsiderCapacityIncomplete` / `ServiceOpportunityIncomplete` before measuring a court. The integrated `2119c943b` source now contains F03k fixture tests; their reported test results do not supply a capacity curve. A fixture-only single-operation timing result from M-C cannot be extrapolated to a protected court capacity under concurrent sessions.


## Route boundary found during measurement

The selected canonical kernel route uses non-interactive public fault filings. At source `2119c943b`, `misaka/palw/kernel/src/ledger.rs::BlockBudgetV1::charged_v1` charges a block-local adjudication count and combined work budget. Admissions cannot use the prosecution reserve; proof/refutation callers may. This reserve is not a per-claim TIR session reservation, nor a private allocation to each honest outsider. Another public filer can also consume it.

`canonical/f03k.rs` exercises two Sybil filings in a block, silence/refill and a later outsider filing, before/after Final. It does not produce a measured maximum simultaneous-court curve, worst admissible evidence cost, carrier backlog bound, or continuous adversarial service guarantee. A successful later filing cannot establish that an honest filing is included in a block where adversarial filings already exhausted the full budget.

Consequently the provisional TIR values (`cap=42`, `session_weight=13`, `rho_court=3125`) cannot be relabeled as measured capacity of kernel `FileProof`. Kernel measurements must record actual adjudications/work charged and rejected, admission/prosecution reserve use, complete carrier bytes, competing ordering, and per-block reset. TIR/shared-session measurements must separately exercise their actual reservation and queue consumers. No record requirement is waived by this distinction.

## Native reduced-budget preflight — 2026-10-26

[Captured preflight](raw/native-capacity-preflight-2026-10-26/summary.json) completed eight fresh processes, all exit 0: 6/64-position fixture profiles × 1/6 outstanding claims × N=0/1. Source is `9c93b6a130b724b0b8d9ea31ab8525c2af8ac1dc`, binary SHA-256 `bd7f7a365a1dadab35b9cffa91ddbb6e59fea5f5554302d673872d4da000e652`. This predates the later-anchor Panel=1 integration. The [build record](raw/native-capacity-preflight-2026-10-26/build-record.json), raw logs, commands, machine and log hashes are preserved; the manifest covers every captured file. Load 9.959–10.130 makes this **UNDER_LOAD_FIXTURE_ONLY**.

For each N=1 process, two separate blocks carry junk filings from distinct fixture bonds. Six filings yield four adjudications and two over-budget drops in each round, with dismissal fees only on the four adjudicated filings. Each charged filing consumes 1,224,336 declared work units. The later round has a new budget block, confirming reset. A fresh outsider subsequently constructs a valid fault filing and convicts in a later block. N=0 includes claim setup but no prosecution. It is not subtracted from phase timings.

The processor uses its explicit **cfg(test) adjudication cap 4**, reserve 2, and work cap 1,073,741,824. Neither the production cap 64 nor minimum-hardware throughput is measured. The six claims are established sequentially: in the 64-position case the first is already Final and five are in ProbabilisticPass. The logs retain each state. The planted fault is at position 1; the 64-position profile does not represent worst-position discovery cost.

For the single 64-position / six-claim N=1 preflight sample, filing phases took 19.072 ms and 21.896 ms. Each block contained 74,310 bytes of full local Borsh transactions for six filings (29,226 object bytes). The outsider's local check took 7.862 ms and its later filing 12.338 ms (13,758 transaction bytes). These are single descriptive samples, not latency bounds; transaction serialization excludes P2P framing. Seal-phase budget rows can refer to the previous charged block, identified by `budget_blue`.

The [summary validator](../summarize_capacity_measure.py) verifies raw log hashes, emitted records, paired N=0/N=1 conditions, charged work, charged versus dropped counts, and successive budget identities. The 20-repeat batch (320 processes) is queued separately. Latest-source runs are also separate. No continuous adversarial inclusion, reserved service for a particular honest outsider, Gate C, restart/reorg/pruning, real-model court, or production capacity claim follows from this preflight.

## Native reduced-budget batch — 320 processes completed

The [full batch](raw/native-capacity-batch-2026-10-26/summary.json) completed **320/320 processes with exit 0**, on the same pinned `9c93b6a13` source/binary as the preflight: two profiles × four outstanding-claim counts (1/2/4/6) × 20 repetitions × N=0/N=1. Every N=1 run verified both junk-filing rounds and the later outsider conviction. Log hashes, charged work, drop counts, budget resets and paired baselines were independently checked with the summary validator. [Raw records](raw/native-capacity-batch-2026-10-26/raw.json) and [manifest](raw/native-capacity-batch-2026-10-26/manifest.json) preserve all observations.

Load ranged **10.351–19.690**. The following wall times are median / nearest-rank p95 in milliseconds over 20 fresh processes per condition. They include native carrier processing and local blocks; they are not a CPU-only measurement or a population tail bound.

| Job token total (trace positions = total − 1) | Outstanding claims | Filing round 0 ms | Filing round 1 ms | Outsider local check ms | Outsider filing ms | Median full-process RSS |
|---|---:|---|---|---|---|---:|
| 6 | 1 | 4.988 / 5.855 | 4.641 / 5.264 | 1.464 / 1.482 | 5.103 / 5.874 | 56,942,592 B |
| 6 | 2 | 6.707 / 7.315 | 6.228 / 7.482 | 1.469 / 1.501 | 5.436 / 5.826 | 59,097,088 B |
| 6 | 4 | 9.966 / 31.227 | 10.671 / 35.868 | 1.567 / 6.063 | 6.827 / 28.226 | 63,315,968 B |
| 6 | 6 | 33.706 / 66.285 | 33.792 / 117.318 | 1.660 / 4.555 | 19.084 / 36.356 | 68,132,864 B |
| 64 | 1 | 21.035 / 36.083 | 18.646 / 27.814 | 10.671 / 19.447 | 15.376 / 28.619 | 64,569,344 B |
| 64 | 2 | 10.251 / 32.099 | 10.473 / 37.445 | 7.146 / 12.254 | 9.327 / 29.268 | 72,318,976 B |
| 64 | 4 | 38.332 / 53.514 | 35.778 / 79.409 | 7.598 / 9.124 | 35.163 / 71.811 | 93,110,272 B |
| 64 | 6 | 45.477 / 62.309 | 39.334 / 130.653 | 7.954 / 12.243 | 37.914 / 58.135 | 114,638,848 B |

The non-monotone filing medians (for example 64 positions at one versus two claims) coincide with changing machine load and cannot establish a production scaling law. Across both rounds the 1/2/4-claim cases charge all filings; every 6-claim case charges four and drops two without a dismissal fee. The reserve is available to any eligible prosecutor, including the junk coalition; an outsider's later successful filing does not establish priority under continuous same-block saturation.

This completes the reduced-budget fixture curve, **not M-CAP's arming requirement**. The production-cap node, minimum hardware, worst admissible proof, full TIR session reservations, continuous refill/service and restart/reorg/pruned replay remain unmeasured. Results after the post-commit anchor change will be kept under their own source identity.

### Exact meaning of the profile label

The raw `positions` field in these two batches is **prompt tokens plus requested generated tokens**, not the evidence trace length. Source `9c93b6a13` uses a three-token prompt and `max_new_tokens = positions - 3`; `produce` feeds all generated tokens except the final output back into the trace. Thus the 6/64 conditions exercise **5/63 trace positions**. The actual class limit is 64 trace positions, so a maximum legal trace requires a 65-token total with 62 generated tokens. [Pinned source derivation](profile-interpretation-2026-10-26.json) retains the relevant blobs and lines. No completed batch here reaches that exact maximum, and the planted fault at trace position 1 also leaves worst-position localization unmeasured. Raw labels and records are preserved without alteration.

A test-only follow-up now constructs the 65-token-total job, asserts its actual trace length is 64, and plants the fault at trace position 63. Schema 2 records actual trace length, fault position, public tensor count and serialized tensor bytes; artifact tensor serialization and program size are separate fields. These values will describe the fixture's available material, not a cold network transfer or a retention price. The new build and preflights are pending, so no maximum-trace result is claimed yet.

## New-source admission and the approved external-gain bound

Source `8e96ab053` includes the F03k2 Panel=1 class-admission relation and the approved external-gain bound 500 MSK. The processor's cfg(test) cap remains four. For the measured fixture's declared worst work 1,224,336, the source formula gives a one-share dismissal fee 0.1 MSK and 350 DAA exposure: **4 × 0.1 × 350 = 140 MSK**, below the **5 + 500 = 505 MSK** maximum gain. The class-registration branch refuses a failed relation. This source finding is consistent with the existing seed tests failing before a class appears. It does not explain the separate crossing preflight's missing assignment, since that class registered before the Panel=1 fence.

At the production cap 64, the same formula gives **2,240 MSK > 505 MSK**. This is a conditional arithmetic comparison, not a measured attack cost, inclusion guarantee, or successful production-node registration. [Inputs, source hashes and exact integer calculation](reduced-cap-gain-relation-2026-10-26.json) are preserved. The older reduced-budget results predate this combination of checks and retain their original source identity. The approved gain is not reduced merely to make a new fixture pass. A positive production-policy measurement must retain that gain and exercise a compatible full policy; a separately scaled mechanical fixture must identify its different assumptions explicitly.

## Explicit full-cap measurement configuration

The next isolated-process harness supports `--adjudication-cap 64`, selecting 64 only through a cfg(test) hook. Ordinary tests retain the reduced default 4, and the hook is absent from shipped binaries. The harness asserts that the external-gain bound remains 500 MSK. Six filings under this setting exercise a full-cap policy but do **not** saturate its 64-adjudication limit; outputs therefore continue to report `production_cap_measured=false`. Material at the exact 64-position trace maximum is measured separately from claim-count saturation.

`--early-anchor-window` explicitly moves only the measurement fixture's anchor-retention activation to DAA 0 in both parameter mirrors. Public t12 already activates that prerequisite at DAA 5,300, before the intended slice height; the short fixture would otherwise start below it. Source inspection shows that span boundaries clear an anchor before kernel tick when retention is inactive, while a new attempt anchor is written after that tick. This is a hypothesis for the missing deferred assignment, to be tested against the separately preserved unmodified-clock diagnostic. Neither this fixture clock nor its shortened Gate C windows is a production policy adoption. Runtime success is pending.
