# M-CAP — court capacity status

**NOT_MET.** `PalwCourtMeasurementV1` requires every operation class's time, bytes and working set on the slowest supported node, the block budget, `rho_court` and session weight. The current Mac13,1 is not that node. No simultaneous-court node run, saturated/refilling coalition drill, reservation-floor check, or restart/reorg/pruned replay has been performed here.

The earlier `palw_court_op_timing_harness` run failed during its test-only configuration setup with `OneOutsiderCapacityIncomplete` / `ServiceOpportunityIncomplete` before measuring a court. The integrated `2119c943b` source now contains F03k fixture tests; their reported test results do not supply a capacity curve. A fixture-only single-operation timing result from M-C cannot be extrapolated to a protected court capacity under concurrent sessions.


## Route boundary found during measurement

The selected canonical kernel route uses non-interactive public fault filings. At source `2119c943b`, `misaka/palw/kernel/src/ledger.rs::BlockBudgetV1::charged_v1` charges a block-local adjudication count and combined work budget. Admissions cannot use the prosecution reserve; proof/refutation callers may. This reserve is not a per-claim TIR session reservation, nor a private allocation to each honest outsider. Another public filer can also consume it.

`canonical/f03k.rs` exercises two Sybil filings in a block, silence/refill and a later outsider filing, before/after Final. It does not produce a measured maximum simultaneous-court curve, worst admissible evidence cost, carrier backlog bound, or continuous adversarial service guarantee. A successful later filing cannot establish that an honest filing is included in a block where adversarial filings already exhausted the full budget.

Consequently the provisional TIR values (`cap=42`, `session_weight=13`, `rho_court=3125`) cannot be relabeled as measured capacity of kernel `FileProof`. Kernel measurements must record actual adjudications/work charged and rejected, admission/prosecution reserve use, complete carrier bytes, competing ordering, and per-block reset. TIR/shared-session measurements must separately exercise their actual reservation and queue consumers. No record requirement is waived by this distinction.
