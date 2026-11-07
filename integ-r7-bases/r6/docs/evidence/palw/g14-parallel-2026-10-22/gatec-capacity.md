# G14 lane gatec-capacity (2026-10-22)

Status: **written, not compiled, not run** (lane rules: no cargo).

## Finding first: most of the brief was already implemented on this base (d9f91de69)

The brief described an older tree. On this branch base:

### Task A (RFC-0018 Gate C) — already unified on ONE predicate
`palw_rfc0018_v1::palw_fp_receipt_clock_v1` is already read by every site the brief lists:
- receipt admission: `palw_fp_admission_v3::receipt_items_1_to_5` (V3 and RFC-0009 V4 via `palw_receipt_v4.rs:476`); the processor passes `Some(state_params)` (`processor/palw.rs` ~10100–10128).
- fold quantum consumption: `palw_state_v2.rs` ReceiptSpend edge (~19559) refuses with `ReceiptUncertified` BEFORE the quantum range / `spent` insert / settlement-once marker — an uncertified spend is refused, not "weighed 0 and consumed" (the brief's described bug no longer exists; existing test `rfc18_a_receipts_wait_for_security_certification_and_count_from_it` asserts "must refuse (not weigh 0 and pay)").
- RPC/template exposure: spendable-receipts listing `processor/palw.rs` ~3598 skips claims where the clock errs; `kaspad/src/palw_service_tir_frame_e2e.rs:898` same.
- coinbase: the receipt-block worker share is paid only to blocks the processor pre-check (`processor/palw.rs` ~10040, parent state) entitles; that pre-check reads the same clock. `consensus/src/processes/coinbase.rs` itself only splits payouts of already-entitled blocks (no own predicate to change).
- revocation: conviction voids the claim -> `palw_work_certified_from_row_v1` returns false -> status `revoked`; siblings/reorgs read their own chain state at the candidate point.

**Gap closed by this lane:** the predicate was `Final ∧ SecurityCertified` only. Certification decides with nothing pending, but an accusation opened AFTER certification did not suspend the lane. Now:
- `consensus/core/src/palw_rfc0018_v1.rs::palw_fp_receipt_clock_v1`: governed + certified + `chain.palw_accusation_pending_v1(claim_id)` -> `Err(status "accusation-pending")`. Because every site above calls this one function, admission, fold, RPC listing and coinbase entitlement change together. Acquittal (sessions closed) re-opens; conviction voids (`revoked`).
- Fence: the existing Gate C fence (`palw_independently_work_certified_v1`, read via the RFC-0018 mirror, `palw_gate_c_governs_v1`); not governed -> untouched released rule. `PALW_RFC0018_ARMABLE_V1` stays `false` (not changed by this lane).
- test helper `PalwChainStateV2::set_open_courts_for_tests` (`palw_state_v2/chain_state.rs`, `#[cfg(test)]`).

### DAG width schedule — consumers exist; the "stub, reader無し" note is stale
`palw_dag_width_schedule_v1` is consumed: `GhostdagManager::with_dag_width` (`consensus/src/processes/ghostdag/protocol.rs:225`, per selected-parent DAA k via `width_at_parent`), wired in `consensus/src/consensus/services.rs:148,292` and pruning proof build/validate (`pruning_proof/build.rs:604`, `validate.rs:217`); `max_block_parents_history` / `mergeset_size_limit_history` in header and virtual processors; `merge_depth_history` in `block_depth.rs` via services.rs:130; mergeset history services.rs:201,305. `ghostdag_k_history()` itself has no caller outside validation/tests because ghostdag reads the fence struct directly. The only stale "stub" wording found is in a historical evidence copy (`docs/evidence/palw/g14-completion-2026-10-21/funded-original-stage1-sources/.../lib.rs:577`), which is an evidence snapshot and was left unedited. Arming stays refused by `validate_palw_rfc0018_v1`.

### Task B (RFC-0016 capacity) — already fixed on this base
`palw_capacity_supply_from_attestations_v1` already returns `PalwVerifierResourceSupplyV1::conservative_per_span()` = `min_m claims_per_span(m) × c_m` (audit 16-B fix), not the bare compute. Production admission past `palw_elastic_final_capacity_v1` (`rfc0016_capacity_active_at`) already reads attested capacity: `palw_rfc0016_fold_v1.rs::rfc0016_capacity_read_v1` -> `palw_rfc0016_attested_capacity_v1` -> `palw_waterfill_resources_v1` (exact multi-resource split, which keeps DA/court whole); below the fence the fixed reference-seat path is unchanged. This lane only added the requested regression test.

## Tests added (crate `kaspa-consensus-core`, lib tests)
- `palw_fp_admission_v3::tests::gatec_an_accusation_pending_after_certification_suspends_receipts`
- `palw_rfc0016_v1::capacity::tests::g14_supply_carries_the_binding_minimum_compute100_da5_court3` (compute 100 / DA 5 / court 3 -> supply 3·c, allocation 3 claims)

Run: `cargo test -p kaspa-consensus-core gatec_an_accusation_pending g14_supply_carries` (two invocations).

## Fences needed armed (centrally, t12 DAA 16000)
- RFC-0018 Gate C (`palw_independently_work_certified_v1`) — requires `PALW_RFC0018_ARMABLE_V1 = true` first (user decision, RFC-0018 §12).
- RFC-0016 `palw_elastic_final_capacity_v1` (+ its mirror's measured `claim_load` records; unmeasured class credits nothing).

## Risks / uncertain
- `palw_accusation_pending_v1` includes DA sessions and legacy dispute reservations; a griefing accuser can suspend a certified claim's receipt lane for the session's bounded life (bounded by existing DA/court caps and accuser bonds). The use window counts from certification, so a long suspension could let the beacon window lapse — honest-claim payment loss under griefing is possible; not changed here (would need a clock-pause design, record as open question).
- Design scope: RFC-0018 §7 / 18-A (adopted 2026-10-15) defines Gate C as the one receipt predicate; adding "not accusation-pending" matches the brief's definition and the certification verdict's own `accusation_pending` fact (`palw_security_certification_v1.rs:410`).
