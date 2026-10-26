# M-CAP — court capacity status

**NOT_MET.** `PalwCourtMeasurementV1` requires every operation class's time, bytes and working set on the slowest supported node, the block budget, `rho_court` and session weight. The current Mac13,1 is not that node. No simultaneous-court node run, saturated/refilling coalition drill, reservation-floor check, or restart/reorg/pruned replay has been performed here.

The earlier `palw_court_op_timing_harness` run failed during its test-only configuration setup with `OneOutsiderCapacityIncomplete` / `ServiceOpportunityIncomplete` before measuring a court. The F02/F03 integration lanes are still in progress. A fixture-only single-operation timing result from M-C cannot be extrapolated to a protected court capacity under concurrent sessions.
