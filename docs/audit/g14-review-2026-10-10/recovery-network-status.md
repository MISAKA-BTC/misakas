# Automatic producer and public verifier recovery — implementation checkpoint

The new `palw_service_recovery_e2e.rs` module uses the existing ordinary service fixture with an opt-in recovery hook. Existing service cases keep their original path. The automatic path launches a separate producer `PalwPanelService`, using its real signing key, funding/state directory and retained capture produced by actual model execution. The outside verifier receives no capture and all manual answer injection in the fixture is disabled for this path.

Two tests are implemented, **not yet compiled or run** at this checkpoint:

- `automatic_producer_multipart_and_fresh_verifier_resume_after_partial_transport`: the existing ordinarily admitted 112,291-byte constrained job must be published by the automatic producer answer worker as one manifest and two unique parts. Both producer and verifier restart while the accepted native bitmap records a partial assembly. The restarted workers use the same configuration and persisted funding, and the verifier must independently replay all three roots Honest and release its real reservation.
- `restarted_public_verifier_defaults_when_automatic_producer_holds_no_material`: a live outsider restarts while a public binding demand is open; an automatic producer with no retained job/capture must not fabricate an answer. The existing consensus DA-default and producer-sanction/release assertions remain required.

`PalwPanelConfig` now derives `Clone` so a restart can use precisely the same configuration. No consensus fence, admission predicate, active profile, collateral, deadline or shipping activation guard is weakened.

Formatting and `git diff --check` passed. A coordinated integrated node build and both service runs are pending; this source checkpoint is not PASS evidence. The fixture shares a native public consensus/mempool interface and uses ordinary PQ carrier admission and mining templates. It is not a socket RPC/P2P or pruned-IBD proof. Actual selected-chain reorg, remote authenticated reads, maximum-profile budgets and full G14 activation remain unproven.

## Constrained job-only recovery fix (verification pending)

The production DA material loader previously replayed a retained constrained FP descriptor without opening its authenticated constraint mask. A host with the correct registered model and public table would therefore refuse recovery after its capture was lost/corrupt even though its retained `FPM1` job/input was sufficient. Recovery now opens the scope from that descriptor and the host's matching table, restores the thread scope afterward, and retains/responds only if the remade capture reproduces the claim's recorded roots and job pin.

The new real-model regression `constraint_job_only_recovery_restores_the_mask_and_never_answers_from_missing_tables_or_wrong_roots` covers descriptor-only and corrupt-capture recovery, identical authenticated Event answers, exact retained capture bytes, unavailable/different tokenizer tables and wrong recorded roots. It asserts the unscoped execution fails, making the pre-fix failure mechanism explicit. This test is implemented but not yet compiled/run. Formatting and diff checks passed; no PASS or full G14 guarantee is claimed.
