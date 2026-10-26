//! **Lane g14-fam-panelda: PanelDa text prompts get a G14 route by mandatory disclosure (ADR-0206 proposed, DORMANT).**
//!
//! Fixture-only fence heights (`FENCE`, below) — test values, not a pin.

use kaspa_consensus_core::config::params::{ForkActivation, TESTNET_PARAMS};
use kaspa_consensus_core::palw_freeprompt_v3::{PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_V3_VERSION, PalwFreePromptJobV3};
use kaspa_consensus_core::palw_g14_gates_v1::{
    PalwG14RouteFamilyV1 as F, PalwG14RouteMirrorV1, palw_g14_fp_route_families_at_v1, palw_g14_fp_route_families_v1,
    palw_g14_route_family_fences_v1,
};
use kaspa_consensus_core::palw_g14_values_v1::PALW_G14_PANEL_DA_DISCLOSE_BY_DAA_V1;
use kaspa_consensus_core::palw_panel_da_disclosure_v1::*;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64 as H;

/// Test-only fence height.
const FENCE: u64 = 1_000;

fn prompt() -> Vec<u32> {
    vec![11, 12, 13, 14, 15]
}

fn job(privacy_mode: u8, version: u16) -> PalwFreePromptJobV3 {
    PalwFreePromptJobV3 {
        version,
        network_domain: H::from_u64_word(0x4E),
        class_id: H::from_u64_word(1),
        executor_bond: TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 },
        executor_pubkey: vec![7; 4],
        operator_id: H::from_u64_word(90),
        anchor_block: H::from_u64_word(0xA0),
        anchor_daa: 100,
        job_nonce: [3; 32],
        tokenizer_id: H::from_u64_word(0x70),
        prompt_token_ids_hash: kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&prompt()),
        prompt_tokens: prompt().len() as u32,
        decode_token_limit: 16,
        max_context_tokens: 64,
        privacy_mode,
        prompt_mode: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: None,
        tail: None,
    }
}

/// Every public FP route and the disclosure fence in force from `FENCE` (fixture).
fn mirror() -> PalwG14RouteMirrorV1 {
    PalwG14RouteMirrorV1 {
        from_daa: 0,
        legacy_filer_from: Some(0),
        staged_court_from: Some(0),
        tir_conviction_from: Some(0),
        panel_da_disclosure_from: Some(FENCE),
        ..Default::default()
    }
}

fn routed(m: &PalwG14RouteMirrorV1, j: &PalwFreePromptJobV3, d: u64) -> bool {
    palw_g14_fp_route_families_at_v1(j, m, d).iter().all(|f| m.route_armed(*f, d, true))
}

#[test]
fn panel_da_text_routes_past_the_fence_and_matches_public_da_shape() {
    let m = mirror();
    let pd = job(PALW_FP_PRIVACY_PANEL_DA, PALW_FP_V3_VERSION);
    let fams = palw_g14_fp_route_families_at_v1(&pd, &m, FENCE);
    assert_eq!(fams, vec![F::FpPanelDaDisclosure, F::FpSelection, F::FpTokenizer]);
    // After disclosure the claim is judged by exactly the public families a PublicDa job of the same shape needs (③〜⑥).
    let public = palw_g14_fp_route_families_v1(&job(PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_V3_VERSION));
    assert_eq!(&fams[1..], &public[..]);
    assert!(routed(&m, &pd, FENCE));
}

#[test]
fn fence_boundary_is_byte_identical_below() {
    let m = mirror();
    let pd = job(PALW_FP_PRIVACY_PANEL_DA, PALW_FP_V3_VERSION);
    assert_eq!(palw_g14_fp_route_families_at_v1(&pd, &m, FENCE - 1), vec![F::NonPublicEvidence]);
    assert_eq!(palw_g14_fp_route_families_at_v1(&pd, &m, FENCE - 1), palw_g14_fp_route_families_v1(&pd));
    assert!(!routed(&m, &pd, FENCE - 1));
    let unarmed = PalwG14RouteMirrorV1 { panel_da_disclosure_from: None, ..m };
    assert_eq!(palw_g14_fp_route_families_at_v1(&pd, &unarmed, 1 << 40), vec![F::NonPublicEvidence]);
}

#[test]
fn v5_images_stay_non_public_evidence() {
    let m = mirror();
    let v5 = job(PALW_FP_PRIVACY_PANEL_DA, kaspa_consensus_core::palw_fp_job_v5::PALW_FP_V5_VERSION);
    assert!(!palw_panel_da_disclosable_job_v1(&v5));
    assert_eq!(palw_g14_fp_route_families_at_v1(&v5, &m, FENCE + 10), vec![F::NonPublicEvidence]);
    assert!(!routed(&m, &v5, FENCE + 10));
}

/// Honest: disclosed in time → the hold lifts (Final / reward may proceed under the public routes).
#[test]
fn honest_disclosure_releases_the_hold() {
    let (pd, a) = (job(PALW_FP_PRIVACY_PANEL_DA, PALW_FP_V3_VERSION), FENCE + 5);
    let at = a + 10;
    palw_panel_da_disclosure_admit_v1(&pd, &prompt(), PalwPromptIdsFormV1::Flat, a, at).expect("ids bind");
    let s = palw_panel_da_disclosure_status_v1(a, Some(at), at + 1);
    assert_eq!(s, PalwPanelDaDisclosureStatusV1::Disclosed { at });
    assert_eq!(palw_panel_da_settlement_hold_v1(s), None);
}

/// A lie with every owner colluding (nobody disputes): the deadline forces the material public, so an outsider holds the ids that
/// the hash binds — which is all the public FP routes need — with the notice window still open.
#[test]
fn collusion_cannot_keep_the_material_from_the_outsider() {
    let (pd, a) = (job(PALW_FP_PRIVACY_PANEL_DA, PALW_FP_V3_VERSION), FENCE);
    let deadline = palw_panel_da_disclosure_deadline_v1(a).unwrap();
    assert_eq!(deadline, a + PALW_G14_PANEL_DA_DISCLOSE_BY_DAA_V1);
    // Colluders cannot substitute other ids: the hash commitment fixes them.
    assert_eq!(
        palw_panel_da_disclosure_admit_v1(&pd, &[1, 2, 3, 4, 5], PalwPromptIdsFormV1::Flat, a, deadline),
        Err(PalwPanelDaDisclosureRefusalV1::IdsDoNotBind)
    );
    palw_panel_da_disclosure_admit_v1(&pd, &prompt(), PalwPromptIdsFormV1::Flat, a, deadline).expect("last-moment disclosure");
    let notice_left = kaspa_consensus_core::palw_court_policy_v1::NOTICE_WINDOW_DAA - (deadline - a);
    assert!(notice_left >= kaspa_consensus_core::palw_g14_values_v1::PALW_G14_PANEL_DA_OUTSIDER_MIN_WINDOW_DAA_V1);
    // Withholding instead → held, then default: no reward is ever released to a colluding producer.
    assert!(palw_panel_da_settlement_hold_v1(palw_panel_da_disclosure_status_v1(a, None, deadline)).is_some());
}

/// No disclosure: the responsible party defaults — claim void, deposit forfeited, no computation conviction, no slash.
#[test]
fn no_disclosure_is_default_not_conviction() {
    let (pd, a) = (job(PALW_FP_PRIVACY_PANEL_DA, PALW_FP_V3_VERSION), FENCE);
    let deadline = palw_panel_da_disclosure_deadline_v1(a).unwrap();
    let s = palw_panel_da_disclosure_status_v1(a, None, deadline + 1);
    assert_eq!(s, PalwPanelDaDisclosureStatusV1::Defaulted { deadline });
    assert!(palw_panel_da_settlement_hold_v1(s).is_some());
    assert_eq!(
        palw_panel_da_disclosure_admit_v1(&pd, &prompt(), PalwPromptIdsFormV1::Flat, a, deadline + 1),
        Err(PalwPanelDaDisclosureRefusalV1::AfterDeadline { deadline })
    );
    let o = PALW_PANEL_DA_DEFAULT_OUTCOME_V1;
    assert!(o.claim_void && !o.computation_conviction && o.computation_bond_slash_sompi == 0 && o.deposit_forfeit_sompi > 0);
}

#[test]
fn arming_mechanism_dormant_and_refused_until_wired() {
    assert_eq!(
        palw_g14_route_family_fences_v1(F::FpPanelDaDisclosure),
        &["palw_panel_da", "palw_court_clock_v2", "palw_panel_da_disclosure_v1", "palw_g14_route_required_v1"]
    );
    let mut p = TESTNET_PARAMS.clone();
    assert_eq!(p.palw_panel_da_disclosure_v1, None);
    p.validate_palw_panel_da_disclosure_v1().unwrap();
    p.palw_panel_da_disclosure_v1 = Some(ForkActivation::never());
    p.validate_palw_panel_da_disclosure_v1().expect("never() is dormant");
    p.palw_panel_da_disclosure_v1 = Some(ForkActivation::new(17_000));
    let why = format!("{:?}", p.validate_palw_panel_da_disclosure_v1());
    assert!(why.contains("at or below") || why.contains("not wired"), "{why}");
}
