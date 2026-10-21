# G14 lane native-postfinal — native v39 mining claim post-Final accusation → revocation

Status: **written, not compiled, not run** (lane rule: no cargo).

## Finding: the mechanism already exists on this base
The gap described in the lane brief (native branch of the certification sweep with `accusation_pending: false`) was already
closed on the integration base by commit `2805df00a` ("G-N2, dormant"). This lane verified the wiring by reading and added
the two missing test cases (honest-unaffected, below-fence). No new consensus object, wire tag or fence was added.

Existing implementation (read-verified):
- Accusation path: stage action 80 `ACCUSE_MINING_ACTION_V39` → `consensus/core/src/palw_native_job_fold_v39.rs::apply`
  → `palw_native_mining_court_fold_v39.rs::apply` (kinds 0 filing / 1 demand / 2 answer / 3 default), reusing the job court's
  `check_flat_filing_v39`, `check_flat_demand_v39`, `check_flat_reply_v39` evidence formats on retained component material
  (holds via `MINING_JOB_HOLD_KIND_V39`, `job_held`, liability horizon `palw_native_mining_liability_until_v39`).
- Pending: `palw_native_receipt_v39.rs::native_mining_accusation_pending_v39` (open demand count) and
  `native_mining_convicted_v39`; consumed in `palw_security_certification_fold_v1.rs::sweep_security_certification_v1`
  native branch (`still_final` excludes convicted, `accusation_pending` reads the reservation).
- Conviction: `palw_native_mining_court_fold_v39.rs::convict` — slashes producer's reserved exposure (never refused for short
  collateral), finder ≤490‰ (ADR-0032), rest burned; receipt removed + granted safe weight revoked; claim closed without
  reward; certification row withdrawn; Reproduced attestors' G-N2 locks charged (`charge_native_attestor_locks_v39`).
- Veto stays 0 (RFC-0018 U-6 not adopted). No computation proofs (PRINCIPLES §8).

## Tests (crate `misaka-consensus-core`, module `palw_native_job_fold_v39::mining_court::tests`)
Existing: `g14n2_a_post_final_conviction_slashes_revokes_the_receipt_weight_and_certification_and_charges_attestors`,
`g14n2_a_mining_conviction_is_not_refused_for_short_collateral`, `g14n2_an_open_demand_is_gate_c_pending_and_a_hold_keeps_the_component_material`,
`g14n2_the_liability_horizon_spans_certification_receipt_and_one_prosecution_window`.
Added: `g14n2_an_unaccused_honest_claim_is_untouched`, `g14n2_below_the_mining_fence_action_80_is_refused_without_effect`.
Run: `cargo test -p misaka-consensus-core g14n2_`

## Fences needed armed
Route extras `native_mining39` (`PalwNativeMiningFenceV39`), RFC-0018 Gate C (`gate_c_at`), ADR-0176 bond budget mirror —
the same prerequisites as the owning39 mining fold. No new fence.

## Open / uncertain
- No end-to-end v39 job fixture exists; filing/demand/answer verification is covered only by the job court's own tests, the
  verdict by fold-level tests that write rows directly. A full sweep→pending→convict test through `sweep_security_certification_v1`
  is not present (the pending fact and convicted fact are tested at the predicate level).
- Below-fence test asserts refusal + unchanged aux/weight/bonds; it relies on `PalwKernelRouteExtrasV1::default()` leaving
  `native_mining39 = None`.
