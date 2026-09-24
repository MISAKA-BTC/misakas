//! **F1's no-false-positive sweep on the floor** (the Phase 1–2 review's probe, kept as a
//! regression): an HONEST floor attempt — the real floor backend's job for the anchor at the
//! prefill draw, run and committed by the producer — has no identity fault (J1–J7) and no output
//! fault (10), for 24 anchors under both prompt-id forms (48 runs). The same honest binding recorded
//! under another anchor is J1 (the check is live, not vacuous), and read as a MODEL class's claim —
//! the floor's graph under an identity that is not the base class — it would be held to the model
//! formula's job (`CoreV1`, `(n_ctx/8 − 1, 2)`), which the floor's 12-position context is too narrow
//! to have: `IdentityNotDerivable`, a refusal, never a conviction. (The reviewer's probe asserted
//! "J5 skipped" here, the F1 rule; F1-M's 3a holds model classes to J5.)

mod common;

use common::floor_backend;
use kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_canonical_v1;
use kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_offence_attribution_v1::{
    PalwClaimSourceKindV1, PalwIdentityFaultV1, PalwIdentityRulesV1, PalwOffenceTargetV1, palw_binding_identity_fault_v1,
    palw_output_fault_v1,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_step_refute::{PalwBase0DecodeTokensV1, PalwDecodeTokenPinV1};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;

#[test]
fn honest_floor_attempts_have_no_identity_or_output_fault() {
    let mut checked = 0;
    for form in [PalwPromptIdsFormV1::Flat, PalwPromptIdsFormV1::MerkleV1] {
        let backend = floor_backend(form);
        let profile = backend.profile().clone();
        let class_id = profile.shape_profile_id();
        let rules = PalwIdentityRulesV1 { prompt_ids_form: form, base_class_id: class_id, da_signer_liability: false };
        let as_model = PalwIdentityRulesV1 { prompt_ids_form: form, base_class_id: Hash64::from_u64_word(0xB45E), da_signer_liability: false };
        let model_formula = palw_attempt_canonical_v1(&profile, false);
        for n in 0u64..24 {
            let anchor = Hash64::from_u64_word(0x5EED_0000_0000 ^ n.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17));
            let (canonical, prompt) = backend.job_for_anchor(anchor).expect("a job");
            let job = palw_attempt_job_v1(canonical, true);
            let out = backend.execute(&job, &prompt).expect("the floor runs");
            let (binding, _tiles, rows, toks, ..) =
                misaka_palw_base0::produce::base0_material_decode_v1(&out.material).expect("the capture decodes");
            let target = PalwOffenceTargetV1 {
                claim_id: Hash64::from_u64_word(1),
                class_id,
                artifact_root: Hash64::default(),
                executor_bond: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_bytes([1; 64]), index: 0 }),
                execution_root: out.execution_root,
                lane: Some(PalwClaimSourceKindV1::Attempt),
                segment_count: None,
                phase: None,
                job_identity: anchor,
                trace_root: out.trace_root,
                output_root: out.output_root,
            };
            assert_eq!(palw_binding_identity_fault_v1(&target, &binding, rules, true), Ok(None), "{form:?} anchor {n}");
            let pin = PalwDecodeTokenPinV1::Base0V1(PalwBase0DecodeTokensV1 { logits_rows: rows, generated_token_ids: toks });
            assert_eq!(palw_output_fault_v1(&target, &binding, &pin), Ok(false), "{form:?} anchor {n}: the honest output");
            // The check is live: the same honest binding recorded under another anchor is J1.
            let other = PalwOffenceTargetV1 { job_identity: Hash64::from_u64_word(0xBAD0 + n), ..target.clone() };
            assert_eq!(
                palw_binding_identity_fault_v1(&other, &binding, rules, true),
                Ok(Some(PalwIdentityFaultV1::JobNotTheClaims)),
                "{form:?} anchor {n}: J1"
            );
            // Read as a model class's claim: the floor's context is too narrow for the formula.
            assert_eq!(model_formula, None, "n_ctx 12: no (n_ctx/8 - 1, 2) job exists");
            assert_eq!(
                palw_binding_identity_fault_v1(&target, &binding, as_model, true),
                Err(kaspa_consensus_core::palw_offence_v1::PalwOffenceVerifyError::IdentityNotDerivable),
                "{form:?} anchor {n}: as a model class, refused and never convicted"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 48, "24 anchors under both forms");
}
