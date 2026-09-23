//! **Kimi K3 mining foundation** — the class a chain can take through claim → Final without a
//! live Kimi runtime.
//!
//! There is no environment here that can run Kimi K3, so nothing in this module measures wall-clock
//! p99. Work, windows, carriage, slash and quanta are all derived: CanonicalWork, the global
//! lattice, the court-cost walk, panel VAR, execution-quantum minting. A class that still misses
//! those global windows is refused. No per-class deadline is invented to make it fit.
//!
//! Weight-bearing certification is the one thing a drill with weights must still supply. This
//! module does not mint a `PalwE2eCertificateV1` and does not add a family to
//! `palw_rc_certified_families_v1`. Share-zero registration is the path that is complete.

use crate::Hash64;
use crate::palw_canonical_work_v1::{
    PalwCanonicalClassDescriptorV1, PalwCanonicalExecutionFactsV1, PalwCanonicalWorkVectorV1, palw_canonical_work_v1,
};
use crate::palw_kimi_k3_profile::{
    KIMI_K3_CARD, KIMI_K3_RC_CANONICAL, kimi_k3_profile_v1, kimi_k3_runtime_working_set_v1, kimi_k3_tokenizer_id_card_v1,
};
use crate::palw_model_registry_v1::{PALW_REGISTRY_GLOBALS_V1, PalwDerivedProfileV1, PalwModelWorkV1, palw_derive_profile_v1};
use crate::palw_step::PalwStepError;

/// CanonicalWork of the registered job (8 prefill, 2 decode). Derived, not measured.
pub fn kimi_k3_canonical_work_v1() -> Result<PalwCanonicalWorkVectorV1, PalwStepError> {
    let profile = kimi_k3_profile_v1(KIMI_K3_CARD)?;
    let desc = PalwCanonicalClassDescriptorV1::of(&profile, kimi_k3_tokenizer_id_card_v1())
        .map_err(|_| PalwStepError::ProfileNotCanonical("Kimi K3 dtype is not a priced weight format"))?;
    palw_canonical_work_v1(&desc, &PalwCanonicalExecutionFactsV1::uncached(KIMI_K3_RC_CANONICAL.0, KIMI_K3_RC_CANONICAL.1))
        .map_err(|_| PalwStepError::ProfileNotCanonical("Kimi K3 canonical work does not derive"))
}

/// Registry work vector: verification compute is CanonicalWork, residency is the working set.
/// Artifact bytes a seat pages are the activated weights, not the 2.8 T file.
pub fn kimi_k3_derived_work_v1() -> Result<PalwModelWorkV1, PalwStepError> {
    let mac = kimi_k3_canonical_work_v1()?.arithmetic_mac_eq();
    let ws = kimi_k3_runtime_working_set_v1(KIMI_K3_CARD, KIMI_K3_CARD.n_ctx);
    Ok(PalwModelWorkV1 {
        verification_ccu: mac,
        economic_ccu_per_claim: mac,
        artifact_bytes: ws.activated_weight_bytes,
        working_set_bytes: ws.runtime_bytes(),
        ops_supported: true,
    })
}

pub fn kimi_k3_derived_profile_v1() -> Result<PalwDerivedProfileV1, PalwStepError> {
    // The pre-fence derivation: this is the foundation card's own arithmetic, quoted in ADR-0133
    // §7 and in `kimi_k3_ibd_identity_v1`'s hash, and it answers "what would this class cost" —
    // not "what does this chain gate on today". A height would make the card depend on where a
    // reader stands.
    Ok(palw_derive_profile_v1(&kimi_k3_derived_work_v1()?, &PALW_REGISTRY_GLOBALS_V1, false))
}

/// `verification_window_spans × span_daa ≤ window_receipt`. False means refuse the class, not
/// lengthen the network's receipt deadline.
pub fn kimi_k3_window_fits_global_receipt_v1(span_daa: u64, window_receipt_daa: u64) -> Result<bool, PalwStepError> {
    let spans = kimi_k3_derived_profile_v1()?.verification_window_spans as u64;
    Ok(spans.saturating_mul(span_daa.max(1)) <= window_receipt_daa)
}

/// Consensus identity two nodes recompute from the same objects, with no weights present.
pub fn kimi_k3_ibd_identity_v1() -> Result<Hash64, PalwStepError> {
    let profile = kimi_k3_profile_v1(KIMI_K3_CARD)?;
    let kernels = crate::palw_class_admission_v2::reachable_kernels_v1(&profile);
    let work = kimi_k3_derived_work_v1()?;
    // Fence-free on purpose: this hash is an identity two nodes at different heights must agree
    // on, so the seat gate's height must not enter it.
    let derived = palw_derive_profile_v1(&work, &PALW_REGISTRY_GLOBALS_V1, false);
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/kimi-k3-ibd-identity/v1").to_state();
    h.update(profile.shape_profile_id().as_byte_slice());
    h.update(kimi_k3_tokenizer_id_card_v1().as_byte_slice());
    h.update(&(kernels.len() as u32).to_le_bytes());
    for id in &kernels {
        h.update(id.as_byte_slice());
    }
    h.update(&work.verification_ccu.to_le_bytes());
    h.update(&derived.verification_window_spans.to_le_bytes());
    h.update(&derived.required_ready_seats.to_le_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Ok(Hash64::from_bytes(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_class_admission_v2::{
        PalwClassAdmissionError, PalwHeldAdmissionV1, PalwKaryCourtV1, derive_court_cost_shaped_v1,
        palw_genesis_reaches_kimi_kernel_v1, verify_class_admission_v8, verify_class_admission_v9,
    };
    use crate::palw_e2e_adjudicability::{
        family_certified_for_weight_v1, family_certified_for_weight_v2, palw_e2e_family_id_v1, palw_rc_certified_families_v1,
        palw_rc_court_e2e_root_v1,
    };
    use crate::palw_execution_lane_v1::PalwExecFinalV1;
    use crate::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_v1, palw_execution_quantum_count_v1};
    use crate::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1;
    use crate::palw_kimi_k3_artifact_v1::kimi_k3_convert_artifact_v1;
    use crate::palw_kimi_k3_profile::{kimi_k3_class_id_v1, kimi_k3_reachable_kernels_v1};
    use crate::palw_kimi_k3_tokenizer_v1::{KimiK3TokenizerSpecV1, kimi_k3_tokenize_v1};
    use crate::palw_mode_v2::{
        DEFAULT_MAX_CLOSE_BYTES, DEFAULT_MAX_OPERAND_COUNT, DEFAULT_MAX_TERMINAL_MACS, PalwCourtParamsV2, tests::conforming_bundle,
    };
    use crate::palw_model_registry_v1::{
        PalwLifecycleObservationV1, PalwManifestVerdictV1Flag, PalwModelLifecycleV1, palw_lifecycle_step_v1,
    };
    use crate::palw_panel_var_v1::{
        PalwClaimFraudFactsV1, palw_drawn_panel_covers_v1, palw_max_fraud_gain_v1, palw_panel_seat_required_v1,
    };
    use crate::palw_state_v2::{PALW_BOND_KEY_V2_MIN, PalwConsensusObjectV2, PalwPwuRuleV2};
    use crate::palw_step_refute::{catalogued_kernel_ids_v1, kernel_can_serve_node_v1, kimi_fenced_kernel_ids_v1};

    fn rc_court() -> PalwKaryCourtV1 {
        PalwKaryCourtV1 {
            dissection_arity: 4,
            prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            window_court_daa: PALW_RC_WINDOWS_V1.window_court,
        }
    }

    fn rc_deadline() -> u64 {
        crate::palw_context_ladder::palw_court_turn_deadline_for_history_v1(
            PALW_RC_WINDOWS_V1.window_court,
            crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
            crate::palw_context_ladder::PALW_CONTEXT_LADDER_TERMINAL_MOVES,
            crate::palw_mode_v2::DEFAULT_MAX_CLOSE_CHUNKS,
            KIMI_K3_CARD.n_ctx as u64,
            crate::palw_state_chunk_map::PALW_ATTN_HISTORY_TILE_V4,
        )
        .expect("the RC window holds a 10-position dispute")
        .1
    }

    fn rc_bundle() -> crate::palw_mode_v2::PalwConsensusParamsV2 {
        let mut bundle = conforming_bundle();
        bundle.court = PalwCourtParamsV2::with_cost_ceilings(
            crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
            rc_deadline(),
            2,
            DEFAULT_MAX_CLOSE_BYTES,
            DEFAULT_MAX_TERMINAL_MACS,
            DEFAULT_MAX_OPERAND_COUNT,
        )
        .expect("legal");
        bundle
    }

    fn weightless(profile: &crate::palw_step::PalwShapeProfileV3) -> PalwConsensusObjectV2 {
        let canonical = crate::palw_base0_profile::rc_job_context(profile, KIMI_K3_RC_CANONICAL.0, KIMI_K3_RC_CANONICAL.1);
        let counted = crate::palw_step::step_leaf_count_capped_v1(
            profile,
            &canonical,
            crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
        )
        .expect("counts");
        PalwConsensusObjectV2::ClassRegistered {
            class_id: profile.shape_profile_id(),
            artifact_root: Hash64::from_u64_word(0x4B33),
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
            initial_target: 1,
            share_permille: 0,
            activation_daa: 0,
            admission: None,
        }
    }

    fn admit(kimi_family: bool, share: u16) -> Result<crate::palw_mode_v2::PalwClassCatalogEntryV2, PalwClassAdmissionError> {
        let profile = kimi_k3_profile_v1(KIMI_K3_CARD).expect("projects");
        let court = rc_court();
        let rules = crate::palw_context_ladder::palw_class_ladder_rules_for_court_v1(
            &profile,
            Some(court),
            crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
        );
        let canonical = crate::palw_base0_profile::rc_job_context(&profile, KIMI_K3_RC_CANONICAL.0, KIMI_K3_RC_CANONICAL.1);
        let mut registration = weightless(&profile);
        if let PalwConsensusObjectV2::ClassRegistered { share_permille, .. } = &mut registration {
            *share_permille = share;
        }
        verify_class_admission_v9(
            &rc_bundle(),
            &profile,
            &canonical,
            &registration,
            &palw_rc_certified_families_v1(),
            &[],
            rules,
            Some(court),
            false,
            false,
            false,
            false,
            PalwHeldAdmissionV1::default(),
            kimi_family,
            // The Kimi foundation drill predates the 2026-09-23 fence: the pre-fence gate.
            false,
        )
    }

    #[test]
    fn every_reachable_kernel_is_adjudicable() {
        let profile = kimi_k3_profile_v1(KIMI_K3_CARD).expect("projects");
        crate::palw_catalog_coverage::verify_profile_coverage_v1(&profile).expect("every node's shape is servable");
        for (table, nodes, pre) in [
            ("pre", &profile.pre_nodes, true),
            ("gdn", &profile.gdn_nodes, false),
            ("attn", &profile.attn_nodes, false),
            ("post", &profile.post_nodes, false),
        ] {
            for (slot, node) in nodes.iter().enumerate() {
                kernel_can_serve_node_v1(node, pre).unwrap_or_else(|e| panic!("{table}[{slot}]: {e}"));
            }
        }
        let ids = kimi_k3_reachable_kernels_v1(KIMI_K3_CARD).expect("projects");
        let fenced = kimi_fenced_kernel_ids_v1();
        assert!(!ids.is_disjoint(&fenced));
        assert!(fenced.is_disjoint(&catalogued_kernel_ids_v1()));
        let identity: std::collections::BTreeSet<_> = ids.difference(&fenced).copied().collect();
        crate::palw_catalog_coverage::verify_catalog_coverage_v1(&crate::palw_catalog_coverage::PalwReachableKernelSetV1 {
            execution_class_id: profile.shape_profile_id(),
            kernel_ids: identity,
        })
        .expect("non-Kimi kernels sit in the identity catalog");
    }

    #[test]
    fn rc_certified_families_do_not_cover_kimi_and_no_fake_family_is_pinned() {
        let kernels = kimi_k3_reachable_kernels_v1(KIMI_K3_CARD).expect("projects");
        let rc = palw_rc_certified_families_v1();
        assert!(
            family_certified_for_weight_v1(palw_rc_court_e2e_root_v1(), &rc, &kernels).expect("root matches").is_none(),
            "Kimi is not a genesis-certified family; a drill with weights still has to post one"
        );
        assert!(!rc.iter().any(|f| f.family_id == palw_e2e_family_id_v1("PALW-KIMI-K3")));
        let chain = crate::palw_e2e_adjudicability::PalwE2eFamilyV1 {
            family_id: palw_e2e_family_id_v1("PALW-KIMI-K3"),
            drilled_class_id: kimi_k3_class_id_v1(),
            kernel_ids: kernels.clone(),
            covering: crate::palw_e2e_adjudicability::PalwE2eCoveringV1 {
                pre: true,
                gdn: true,
                attn: true,
                post: true,
                prefill: true,
                decode: true,
                convicted_leaves: kernels.len() as u32,
                malformed_refused: true,
                drilled_kernel_ids: kernels.clone(),
            },
        };
        assert!(
            family_certified_for_weight_v2(palw_rc_court_e2e_root_v1(), &rc, std::slice::from_ref(&chain), &kernels)
                .expect("genesis root still matches")
                .is_some(),
            "once a real FamilyCertified lands, set inclusion is the weight path — this is not that certificate"
        );
    }

    #[test]
    fn the_fence_admits_share_zero_and_refuses_weight_without_a_family() {
        let profile = kimi_k3_profile_v1(KIMI_K3_CARD).expect("projects");
        let canonical = crate::palw_base0_profile::rc_job_context(&profile, KIMI_K3_RC_CANONICAL.0, KIMI_K3_RC_CANONICAL.1);
        let registration = weightless(&profile);
        let court = rc_court();
        let rules = crate::palw_context_ladder::palw_class_ladder_rules_for_court_v1(
            &profile,
            Some(court),
            crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
        );
        let dormant = verify_class_admission_v8(
            &rc_bundle(),
            &profile,
            &canonical,
            &registration,
            &[],
            &[],
            rules,
            Some(court),
            false,
            false,
            false,
            false,
            PalwHeldAdmissionV1::default(),
        )
        .expect_err("dormant");
        assert_eq!(dormant, PalwClassAdmissionError::KimiFamilyNeedsItsFence);

        let entry = admit(true, 0).expect("armed, share-zero Kimi registers");
        assert_eq!(entry.class_id, profile.shape_profile_id());
        assert!(!entry.reachable_kernels.is_disjoint(&kimi_fenced_kernel_ids_v1()));

        let err = admit(true, 1).expect_err("weight needs a certified family a drill has not produced");
        assert!(matches!(err, PalwClassAdmissionError::NotEndToEndCertified { share: 1 }));
    }

    #[test]
    fn derived_verification_fits_the_global_receipt_window() {
        let span_daa = crate::palw_verification_profile_v1::PALW_SPAN_ANCHORS_V1;
        assert!(
            kimi_k3_window_fits_global_receipt_v1(span_daa, PALW_RC_WINDOWS_V1.window_receipt).expect("derives"),
            "the registered job's derived window must fit window_receipt; if it does not, refuse, do not extend"
        );
        let profile = kimi_k3_derived_profile_v1().expect("derives");
        assert!(profile.verification_window_spans >= 1);
        assert!(
            kimi_k3_window_fits_global_receipt_v1(span_daa, 0).expect("derives") == false,
            "a zero receipt window refuses the class rather than inventing a per-class deadline"
        );
    }

    #[test]
    fn no_per_class_lifecycle_profile_is_invented() {
        assert!(
            kimi_k3_window_fits_global_receipt_v1(
                crate::palw_verification_profile_v1::PALW_SPAN_ANCHORS_V1,
                PALW_RC_WINDOWS_V1.window_receipt
            )
            .expect("derives"),
            "global windows hold the registered job, so there is no Kimi-specific receipt deadline"
        );
    }

    #[test]
    fn carriage_and_court_cost_fit_the_rc_close_ceiling() {
        let profile = kimi_k3_profile_v1(KIMI_K3_CARD).expect("projects");
        let cost = derive_court_cost_shaped_v1(
            &profile,
            crate::palw_context_ladder::palw_class_ladder_rules_for_court_v1(
                &profile,
                Some(rc_court()),
                crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
            )
            .expect("the hybrid map prices a checkpoint-anchored court")
            .cost_shape,
        )
        .expect("prices");
        assert!(
            cost.max_close_bytes <= DEFAULT_MAX_CLOSE_BYTES,
            "close {} must fit the RC ceiling {}",
            cost.max_close_bytes,
            DEFAULT_MAX_CLOSE_BYTES
        );
        assert!(cost.max_terminal_macs <= DEFAULT_MAX_TERMINAL_MACS);
        assert!(cost.max_operand_count <= DEFAULT_MAX_OPERAND_COUNT);
        let chunks = crate::palw_mode_v2::palw_close_chunks_for_bytes_v1(cost.max_close_bytes);
        assert!(chunks <= crate::palw_mode_v2::DEFAULT_MAX_CLOSE_CHUNKS);
        assert!(
            !crate::palw_state_chunk_map::palw_profile_is_held_v4(&profile),
            "the registered job is not a held map: prompt ids stay on chain, Panel DA is not required"
        );
    }

    #[test]
    fn collateral_covers_panel_false_valid() {
        let work = kimi_k3_canonical_work_v1().expect("derives");
        let pwu = work.arithmetic_mac_eq().min(u128::from(u64::MAX)) as u64;
        let facts = PalwClaimFraudFactsV1 {
            reserved: u128::from(pwu),
            escrowed_reward: pwu.saturating_div(10),
            exposure_pwu: pwu,
            slash_value_per_pwu: 1,
            extra_economic_rights_sompi: 0,
        };
        let required = palw_panel_seat_required_v1(&facts);
        let gain = palw_max_fraud_gain_v1(&facts);
        assert!(required > 0, "a Kimi claim locks more than zero");
        let seats = [required, required, required, required, required];
        assert!(palw_drawn_panel_covers_v1(&seats, &facts), "three seats at required exceed max fraud gain {gain}");
        let short = [required.saturating_sub(1), required.saturating_sub(1), required.saturating_sub(1), required, required];
        assert!(!palw_drawn_panel_covers_v1(&short, &facts), "a quorum posted under required does not cover");
    }

    #[test]
    fn execution_quanta_follow_work_not_copies() {
        let mac = kimi_k3_canonical_work_v1().expect("derives").arithmetic_mac_eq();
        assert!(mac > PALW_EXECUTION_QUANTUM_V1 as u128, "the registered job is more than one quantum of work");
        let seed = Hash64::from_u64_word(7);
        let one = Hash64::from_u64_word(1);
        let two = Hash64::from_u64_word(2);
        // Counts are of CanonicalWork units, not of a minted million-ticket vector.
        let n_one = palw_execution_quantum_count_v1(mac, PALW_EXECUTION_QUANTUM_V1 as u128, seed, one);
        let n_two = palw_execution_quantum_count_v1(mac.saturating_mul(2), PALW_EXECUTION_QUANTUM_V1 as u128, seed, two);
        assert!(n_one >= 1);
        assert!(n_two > n_one, "twice the CanonicalWork mints more quanta");
        assert_eq!(
            palw_execution_quantum_count_v1(mac, PALW_EXECUTION_QUANTUM_V1 as u128, seed, one),
            n_one,
            "repeating the same facts does not inflate"
        );
        let bond = PALW_BOND_KEY_V2_MIN;
        let a = PalwExecFinalV1 {
            domain: Hash64::from_u64_word(9),
            bond,
            operator_id: Hash64::from_u64_word(8),
            claim_id: one,
            execution_root: Hash64::from_u64_word(0x4B33),
            credit: 250_000,
        };
        let copy = PalwExecFinalV1 { claim_id: two, ..a };
        let minted = palw_execution_mint_quanta_v1(&[a, copy], seed, PALW_EXECUTION_QUANTUM_V1 as u128, 1);
        let once = palw_execution_mint_quanta_v1(&[a], seed, PALW_EXECUTION_QUANTUM_V1 as u128, 1);
        assert_eq!(minted.len(), once.len(), "two Finals of one execution_root mint one ticket set");
        assert!(minted.len() >= 2, "250k credit is at least two quanta");
    }

    #[test]
    fn ibd_pruning_reorg_and_timeout_see_one_identity() {
        let a = kimi_k3_ibd_identity_v1().expect("a");
        let b = kimi_k3_ibd_identity_v1().expect("b");
        assert_eq!(a, b, "two nodes holding no weights recompute the same identity");
        let prompt = "same prompt on every host";
        assert_eq!(
            kimi_k3_tokenize_v1(&KimiK3TokenizerSpecV1::CARD, prompt.as_bytes(), true).expect("a"),
            kimi_k3_tokenize_v1(&KimiK3TokenizerSpecV1::CARD, prompt.as_bytes(), true).expect("b")
        );
        let tensors = vec![("blk.0.kda_q.weight".into(), vec![1, 2, 3]), ("token_embd.weight".into(), vec![4, 5])];
        assert_eq!(
            kimi_k3_convert_artifact_v1(KIMI_K3_CARD, &KimiK3TokenizerSpecV1::CARD, tensors.clone()).expect("a"),
            kimi_k3_convert_artifact_v1(KIMI_K3_CARD, &KimiK3TokenizerSpecV1::CARD, tensors).expect("b")
        );
        let mut bundle = conforming_bundle();
        assert!(!palw_genesis_reaches_kimi_kernel_v1(&bundle), "shipped genesis does not register Kimi");
        bundle.genesis_objects.clear();
        let timeout_fits = kimi_k3_window_fits_global_receipt_v1(
            crate::palw_verification_profile_v1::PALW_SPAN_ANCHORS_V1,
            PALW_RC_WINDOWS_V1.window_receipt,
        )
        .expect("derives");
        assert!(timeout_fits, "receipt timeout is the global window; Kimi does not get its own");
    }

    #[test]
    fn candidate_prefetching_probation_walks_on_derived_facts() {
        let g = PALW_REGISTRY_GLOBALS_V1;
        let profile = kimi_k3_derived_profile_v1().expect("derives");
        let ready = profile.required_ready_seats;
        let calm = |seats: u32, jury: bool| PalwLifecycleObservationV1 {
            manifest: PalwManifestVerdictV1Flag::Valid,
            ready_seats: seats,
            probes_passed_this_span: 0,
            probes_failed_this_span: 0,
            utilization_permille: 300,
            collateral_ok: true,
            cap_ok: true,
            window_fits_receipt: true,
            span_stable: true,
            admission_jury_seated: jury,
        };
        let mut s = PalwModelLifecycleV1::Candidate;
        s = palw_lifecycle_step_v1(s, &calm(ready, false), &profile, &g);
        assert_eq!(s, PalwModelLifecycleV1::Candidate, "the registrant cannot seat the jury alone");
        s = palw_lifecycle_step_v1(s, &calm(0, true), &profile, &g);
        assert_eq!(s, PalwModelLifecycleV1::Prefetching);
        s = palw_lifecycle_step_v1(s, &calm(ready.saturating_sub(1).max(0), true), &profile, &g);
        assert_eq!(s, PalwModelLifecycleV1::Prefetching, "short of the derived ready-seat floor");
        s = palw_lifecycle_step_v1(s, &calm(ready, true), &profile, &g);
        assert_eq!(s, PalwModelLifecycleV1::Probation { probes_passed: 0 });
        assert_eq!(s.admission_permille(), 50);
        assert!(s.admits_claims());
    }
}
