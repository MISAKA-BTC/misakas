//! **ADR-0118 — the held regime is armed at a height on a network minted before it.**
//!
//! testnet-11 was minted flat, with the V1 signing-context set and no held regime, and it takes
//! consensus changes by activation, never by a re-genesis. These tests pin what arming the regime
//! there means: the fence validates at a height of its own (Decision 6), a held class commits its
//! own Merkle prompt ids while every other class keeps the network's flat ones (Decision 3), the
//! fence commits the V4 contexts into the identity itself (Decision 1) — and, pinned as the
//! limitation it is, that testnet-11's frozen ladder and prompt cap bound what a held class can be
//! admitted at there, which a network minted with the regime is not bound by.

use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_RC_AUDIT_FENCE_DAA, PALW_RC_HELD_FENCE_DAA, Params, palw_arm_held_regime_at_v1, palw_rc_shipped_params,
};
use kaspa_consensus_core::palw_class_admission_v2::{
    PalwClassAdmissionError, PalwHeldAdmissionV1, palw_admission_shape_at_v1, palw_post_genesis_registration_capped_v1,
    verify_class_admission_v8,
};
use kaspa_consensus_core::palw_context_ladder::{palw_a16_context_row_profile_v5, palw_class_ladder_rules_for_court_v1};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2, PalwCourtParamsV2};
use kaspa_consensus_core::palw_model_fit_v1::palw_model_fit_v2;
use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1, palw_prompt_ids_form_of_class_v1};
use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, qwen25_a16_artifact_row_profile_v7};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

/// A height no testnet-11 schedule entry uses — the proposed deep-audit flag day's.
const HELD_AT: u64 = 7_000;

fn bundle(params: &Params) -> &PalwConsensusParamsV2 {
    match &params.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle,
        _ => panic!("testnet-11 ships a ConsensusV2 bundle; this test's premise is wrong, not its subject"),
    }
}

/// testnet-11 as it was before its held flag day was scheduled: the shipped preset with the
/// regime's three fences cleared.
fn t11_without_the_regime() -> Params {
    let mut params = palw_rc_shipped_params();
    params.palw_held_context = None;
    params.palw_shard_court = None;
    params.palw_fp_da_pins = None;
    params
}

/// testnet-11 with the held regime armed at `daa` by the one helper the preset uses.
fn t11_held_at(daa: u64) -> Params {
    let mut params = t11_without_the_regime();
    palw_arm_held_regime_at_v1(&mut params, daa);
    params
}

/// The dense lineage's held row (graph-v7) at `n_ctx`.
fn dense_v7(n_ctx: u32) -> PalwShapeProfileV3 {
    qwen25_a16_artifact_row_profile_v7(PalwQwen25GeometryV1 { n_ctx, ..QWEN25_1_5B }).expect("graph-v7 builds at every context")
}

/// The gate a `ClassRegistered` meets at `daa`, for a registration whose canonical job meets
/// ADR-0077 Decision 14's floor (`n_ctx / 8` positions) counted against the ruleset's ladder — the
/// ADR-0103 suite's spelling. `Err(Profile)` when the canonical job itself is past the ladder.
fn gate_at(params: &Params, profile: &PalwShapeProfileV3, daa: u64) -> Result<(), PalwClassAdmissionError> {
    let b = bundle(params);
    let shape = palw_admission_shape_at_v1(params, b, profile, daa).expect("the preset has an admission shape");
    let canonical = PalwJobContextV2 {
        version: kaspa_consensus_core::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: b"adr-0118".to_vec(),
        job_id: Hash64::from_u64_word(1),
        job_nullifier: Hash64::from_u64_word(2),
        assignment_id: Hash64::from_u64_word(3),
        execution_seed: [3; 32],
        model_profile_id: Hash64::from_u64_word(4),
        runtime_manifest_hash: Hash64::from_u64_word(5),
        runtime_class_id: Hash64::from_u64_word(6),
        shape_profile_id: profile.shape_profile_id(),
        trace_scheme_id: profile.logits_scheme_id,
        cu_ruleset_id: Hash64::from_u64_word(7),
        tokenizer_id: Hash64::from_u64_word(8),
        prompt_token_ids_hash: Hash64::from_u64_word(9),
        declared_prefill_tokens: (profile.n_ctx / 8).max(2),
        exact_decode_tokens: 2,
        max_context_tokens: profile.n_ctx,
    };
    let object = palw_post_genesis_registration_capped_v1(
        profile.clone(),
        canonical.clone(),
        Hash64::from_u64_word(0xA16),
        0,
        1,
        1,
        0,
        PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::default(), 0)),
        Vec::new(),
        b.court.max_step_leaf_count(),
    )
    .map_err(|e| PalwClassAdmissionError::Profile(format!("{e:?}")))?;
    verify_class_admission_v8(
        b,
        profile,
        &canonical,
        &object,
        &[],
        &[],
        shape.ladder,
        shape.court,
        false,
        shape.token_lift,
        shape.fused_dissectable,
        shape.held,
    )
    .map(|_| ())
}

/// **Decision 6: testnet-11 takes the regime at a height of its own — 7,000, the operator's.** The
/// shipped preset IS the helper at that height (one spelling), the ruleset validates with the
/// network's prompt ids still flat, the schedule gains the height the fork-id gate refuses a stale
/// node at, and the identity moves from the build before it. From genesis on the same bundle it is
/// still refused: a mint that takes the regime states V4 and Merkle itself.
#[test]
fn testnet_11_takes_the_held_regime_at_a_height_of_its_own() {
    assert_eq!(PALW_RC_HELD_FENCE_DAA, Some(HELD_AT), "the operator's height (2026-09-12), shared with the deep-audit fence");
    let shipped = palw_rc_shipped_params();
    let armed = t11_held_at(HELD_AT);
    assert_eq!(
        (shipped.palw_held_context, shipped.palw_shard_court, shipped.palw_fp_da_pins),
        (armed.palw_held_context, armed.palw_shard_court, armed.palw_fp_da_pins),
        "the preset arms exactly what the helper does"
    );
    assert_eq!(shipped.consensus_params_id(), armed.consensus_params_id());
    let before = t11_without_the_regime();
    shipped.validate_palw_v2().unwrap_or_else(|e| panic!("testnet-11 takes the held regime at {HELD_AT}: {e:?}"));
    assert_eq!(shipped.palw_prompt_ids_form_at(u64::MAX - 1), PalwPromptIdsFormV1::Flat, "the network's genesis form is kept");
    assert!(!before.fence_schedule_v1().contains(&HELD_AT) && shipped.fence_schedule_v1().contains(&HELD_AT));
    assert_ne!(shipped.consensus_params_id(), before.consensus_params_id(), "a node without the regime is another network");

    let from_genesis = t11_held_at(0);
    let err = format!("{:?}", from_genesis.validate_palw_v2().expect_err("a flat V1 genesis never took the regime"));
    assert!(err.contains("COMPLETE_V3") || err.contains("from genesis"), "{err}");
}

/// **Why the height must be fresh, or its other fences released together.** `fork_id_v1` digests
/// the FIRED HEIGHTS: arming the regime at the audit's 4,000 adds no schedule entry, so a node
/// carrying 4,000's other fences and not this one would advertise the same fork id and diverge
/// silently past it. The same regime at its own height adds the entry the gate refuses a stale
/// node at.
#[test]
fn a_held_fence_at_a_scheduled_height_is_invisible_to_the_fork_id_gate() {
    let before = t11_without_the_regime().fence_schedule_v1();
    assert!(before.contains(&PALW_RC_AUDIT_FENCE_DAA));
    assert_eq!(t11_held_at(PALW_RC_AUDIT_FENCE_DAA).fence_schedule_v1(), before, "no entry the gate could see");
    assert_eq!(t11_held_at(HELD_AT).fence_schedule_v1().len(), before.len() + 1);
}

/// **Decision 3: a held class commits its own Merkle ids; every other class keeps the network's.**
/// On testnet-11 armed at a height, the court the gate is handed reads the network's flat form; the
/// held dense row is priced at the tiled root anyway (the ladder rules' cost shape and the fit's
/// report both say Merkle) and admitted, where before ADR-0118 it was refused `LinearInTheContext`
/// on the close. The shipped graph-v5 row keeps the flat form, byte for byte.
#[test]
fn on_a_network_minted_flat_a_held_class_is_priced_at_its_own_merkle_form_and_admitted() {
    let params = t11_held_at(HELD_AT);
    let b = bundle(&params);
    let held = dense_v7(512);
    let shipped = palw_a16_context_row_profile_v5(512).expect("graph-v5 at 512");
    for (profile, expected) in [(&held, PalwPromptIdsFormV1::MerkleV1), (&shipped, PalwPromptIdsFormV1::Flat)] {
        assert_eq!(palw_prompt_ids_form_of_class_v1(PalwPromptIdsFormV1::Flat, profile), expected);
        assert_eq!(palw_prompt_ids_form_of_class_v1(PalwPromptIdsFormV1::MerkleV1, profile), PalwPromptIdsFormV1::MerkleV1);
        let shape = palw_admission_shape_at_v1(&params, b, profile, HELD_AT).expect("a shape");
        let court = shape.court.expect("the k-ary court is armed from genesis");
        assert_eq!(court.prompt_ids_form, PalwPromptIdsFormV1::Flat, "the court reads the network's form");
        let rules = palw_class_ladder_rules_for_court_v1(profile, Some(court), b.court.max_step_leaf_count()).expect("mapped");
        assert_eq!(rules.cost_shape.prompt_ids_form, expected, "the close is priced at the class's own form");
        let regime = kaspa_consensus_core::palw_model_fit_v1::palw_fit_regime_for_v1(shape.held, profile);
        assert_eq!(palw_model_fit_v2(profile, b, Some(court), PalwPromptIdsFormV1::Flat, regime).prompt_ids_form, expected);
    }
    let shape = palw_admission_shape_at_v1(&params, b, &held, HELD_AT).expect("a shape");
    assert_eq!(shape.held, PalwHeldAdmissionV1 { armed: true, panel_da: true });
    gate_at(&params, &held, HELD_AT).unwrap_or_else(|e| panic!("the held row at 512 is admitted past the fence: {e}"));
    assert_eq!(gate_at(&params, &held, HELD_AT - 1), Err(PalwClassAdmissionError::HeldMapNeedsItsFence), "and not before it");
    gate_at(&params, &shipped, HELD_AT).unwrap_or_else(|e| panic!("the shipped row is admitted as before: {e}"));
}

/// **ADR-0119 lifts the limitation ADR-0118 pinned here: a held class's ladder is the regime's.**
/// testnet-11's ladder stays the `2^26` its genesis froze — a bisection opened before the fence must
/// stay playable inside the court window, and a bundle that took a deeper one is refused below — but
/// a held class exists only past the fence, where no bisection opens, so it is walked, priced and
/// recounted at the regime's `2^40`: the dense held row that ADR-0118 measured refused at 1,024 and
/// at 2^21 is admitted at both. A row of the same geometry without the held map keeps the network's
/// ladder, and before the fence none of them registers.
#[test]
fn a_held_class_on_testnet_11_is_admitted_to_2_21_at_its_own_ladder() {
    let params = t11_held_at(HELD_AT);
    let b = bundle(&params);
    assert_eq!(b.court.max_step_leaf_count(), 1 << 26, "testnet-11's ladder, frozen at genesis, is unchanged");
    for n_ctx in [512u32, 1_024, 32_768, 1 << 21] {
        gate_at(&params, &dense_v7(n_ctx), HELD_AT).unwrap_or_else(|e| panic!("the held row at {n_ctx} is admitted: {e}"));
    }
    assert_eq!(gate_at(&params, &dense_v7(1 << 21), HELD_AT - 1), Err(PalwClassAdmissionError::HeldMapNeedsItsFence));
    let shipped_wide = palw_a16_context_row_profile_v5(1_024).expect("graph-v5 at 1,024");
    assert!(gate_at(&params, &shipped_wide, HELD_AT).is_err(), "a row without the held map keeps the network's 2^26");

    let mut deep = params.clone();
    let PalwConsensusMode::ConsensusV2(deep_bundle) = &mut deep.palw_consensus_mode else { unreachable!() };
    let court = deep_bundle.court;
    deep_bundle.court = PalwCourtParamsV2::with_cost_ceilings(
        kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1,
        court.turn_deadline_daa(),
        court.terminal_rounds(),
        court.max_close_bytes(),
        court.max_terminal_macs(),
        court.max_operand_count(),
    )
    .and_then(|c| c.with_dissection_arity(court.dissection_arity()))
    .expect("a legal court");
    let err = format!("{:?}", deep.validate_palw_v2().expect_err("a live chain's bundle cannot take the regime's ladder"));
    assert!(err.contains("window_court"), "{err}");
}

/// **Decision 1: the fence commits the V4 contexts into the identity itself.** Past genesis the
/// bundle still names the set the network was minted with, so the M-8 rule — two builds that spell
/// a context differently cannot share an identity — is kept by the fence's own write, beside its
/// height, wherever the bundle does not state V4 (a mint that does commits it through the ruleset
/// id, and its identity does not move — the ADR-0110 512 vector's pinned document is the witness).
/// Pinned in the source because no value-level test can separate the root from the height.
#[test]
fn the_held_fence_writes_the_v4_contexts_beside_its_height() {
    let source = include_str!("../src/config/params.rs");
    // In `consensus_params_id` — the ruleset's identity — not the schedule id, which names heights.
    let id = source.find("pub fn consensus_params_id(&self)").expect("the identity");
    let at = id + source[id..].find("h.write(b\"palw_held_context\");").expect("the fence's identity write");
    let next = &source[at..at + source[at..].find("\n        }\n").expect("the fence's block")];
    assert!(next.contains("h.write(activation.daa_score().to_le_bytes());"), "{next}");
    assert!(
        next.contains("palw_v2_signature_contexts_root_v4()") && next.contains("h.write(v4.as_bytes());"),
        "the V4 root rides the fence: {next}"
    );
    assert!(ForkActivation::new(HELD_AT).is_active(HELD_AT) && !ForkActivation::new(HELD_AT).is_active(0));
}
