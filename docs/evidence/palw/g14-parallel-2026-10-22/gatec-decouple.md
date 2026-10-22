# Lane gatec-decouple (U05) — status: written, not compiled, not run

Branch `g14p/gatec-decouple` (base 8b036ed38).

## Findings
- The Gate C → `palw_model_eligibility_v1` dependency existed ONLY in `Params::validate_palw_rfc0018_v1`
  (`consensus/core/src/palw_rfc0018_v1.rs`). The runtime path already works without RFC-0017:
  - processor `palw_kernel_route_object_is_signed` (`consensus/src/pipeline/virtual_processor/processor/palw.rs` ~6842) admits an
    inner-75 attestation that names an existing certification row whenever Gate C is in force, bypassing the eligibility A-2 gate;
  - the fold (`palw_kernel_route_fold_v1.rs` ~1574) routes inner 75 to `try_attest_claim_certification_v1` (keyed on Gate C's mirror)
    before RFC-0017's handler.
  - Gate C's predicate never reads a model tier (tiers are applied once in A_m's c_m by `palw_model_tier_v1`, ADR-0179).
- `PALW_RFC0018_ARMABLE_V1 = false` gated nothing but arming (only read in `validate_palw_rfc0018_v1`); its doc condition was
  "RFC's user decisions made and rules built and verified" (RFC-0018 §12 U-1..U-8, §10 T-0..T-5) — a pure approval/measurement pin.

## Implemented (`consensus/core/src/palw_rfc0018_v1.rs`)
- `validate_palw_rfc0018_v1`: Gate C now needs `palw_probabilistic_constraints_v1` (aux 136) and `palw_model_tier_v1` (frozen
  two-tier rule) at or below it; `palw_model_eligibility_v1` is checked (at or below) only where set (non-never). No per-model exam.
- Bool replaced by a record gate mirroring `palw_g14_gates_v1`: `PalwRfc0018GatedFenceV1` (name/armed/requirements/no_record_refusal),
  `PalwRfc0018ArmingRecordV1`, `PALW_RFC0018_ARMING_RECORDS_V1 = &[]` (EMPTY → every armed fence still refused, as before),
  `palw_rfc0018_arming_record_admits_v1`, `Params::validate_palw_rfc0018_records_in_v1`, `Params::validate_palw_rfc0018_with_records_v1`
  (validate_palw_rfc0018_v1 delegates with the pinned list). No isolated-genesis exemption (the bool had none).
- No wire/borsh/params-field change; presets untouched → identity unchanged.

## Tests (kaspa-consensus-core lib, module `palw_rfc0018_v1::tests`)
- new: `gatec_decouple_gate_c_validates_without_model_eligibility_past_the_fence`,
  `gatec_decouple_gate_c_needs_the_tier_fence_and_aux_136_and_a_set_eligibility_fence_below_it`,
  `gatec_decouple_rfc18_record_gate_is_exact_and_well_formed`, `gatec_decouple_shipped_presets_identity_and_validation_unchanged`
- edited: `rfc18_rule_e_v2_needs_rule_e_at_or_below_it_and_the_mirror_must_agree` (prereqs now tier+aux136, not eligibility)
- existing identity tests to rerun: `rfc18_fences_are_dormant_on_every_preset_and_named_in_the_exhaustive_list`,
  `rfc18_never_collapses_and_fingerprints_are_some_only`, `palw_model_eligibility_v1::tests` (unchanged file).

## Fences needed (central arming, t12 @16000)
`palw_independently_work_certified_v1`, `palw_model_tier_v1`, `palw_probabilistic_constraints_v1` at the same height (tier
validation itself requires Gate C and `palw_model_bond_allocation_v1` at or below it). Each additionally needs: an RFC-0018 record
pinned in `PALW_RFC0018_ARMING_RECORDS_V1` (Gate C) and `palw_model_tier_v1`'s own unconditional refusal lifted (RFC-0019 §10,
not this lane). `palw_model_eligibility_v1` must stay unset.

## Open / uncertain
- RFC-0018 §12 U-1 text still says "Gate C ≥ RFC-0017 eligibility"; superseded by PRINCIPLES §9 (2026-10-15). RFC text not edited.
- Requiring the tier fence is a new ordering rule for Gate C (it already was symmetric from the tier side); tests that arm Gate C on
  node test params without the tier fence (e.g. `rfc23_stage1_node.rs`, `owning39/master.rs`, `rule_e_remote_v2.rs`) were already
  refused by the old bool if they validated; verify centrally.
- Retiring `palw_model_eligibility_fold_v1.rs` (survey suggestion) not done: inner 75's wire type and the attestation row types are
  shared with Gate C; out of scope for a decoupling-only change.
- Record requirement lists are my reading of §10/§12; adjust on review.
