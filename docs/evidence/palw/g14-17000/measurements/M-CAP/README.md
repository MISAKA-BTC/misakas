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
