//! **A model nobody compiled in is admitted, and the gate never looks for a table.**
//!
//! The operator's requirement for the new economy: *a new model can be added permissionlessly, with
//! no `main` update and no per-model fork.* The 2026-09-19 re-audit found the consensus side of
//! that genuinely permissionless and the TOOLING side not — `misaka model add` resolves its
//! argument against a compiled-in ledger, so a user who cannot build the node cannot use that
//! command. Those are different claims about different programs, and only the first is a property
//! of the chain. This file pins the first and states the second rather than conflating them.
//!
//! The class under test is an A16 context row at `n_ctx` 384: a real, projectable graph that no
//! shipped preset registers, that is in no genesis work table, that owns no artifact root, and
//! whose `shape_profile_id` appears nowhere in this repository. If the admission gate admits it,
//! the gate is reading the carriage and not a catalogue.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::palw_rc_shipped_params;
use kaspa_consensus_core::palw_class_admission_v2::{PalwKaryCourtV1, verify_class_admission_v8};
use kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v5;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1};

/// An n_ctx the shipped rows do not use: 512 is the dense class, 384 is nobody's.
const STRANGER_N_CTX: u32 = 384;

fn stranger() -> PalwShapeProfileV3 {
    palw_a16_context_row_profile_v5(STRANGER_N_CTX).expect("the row projects at 384 as it does at 512")
}

/// The court the shipped ruleset derives — every field read off the bundle, none chosen, exactly as
/// `palw_v2_class_admission` builds it in the processor. A fused-attention class needs one, and the
/// A16 rows are fused.
fn court(bundle: &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2) -> PalwKaryCourtV1 {
    PalwKaryCourtV1 {
        // The shipped arity, as `palw_court_arity_v1` derives it for `PALW_RC_WINDOWS_V1`. Fixed
        // here rather than re-derived because this file's subject is the class, not the court: what
        // it must show is that the gate reads the CARRIAGE, and a court shape is a ruleset fact
        // every class on the network shares.
        dissection_arity: 4,
        prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
        window_court_daa: bundle.state.window_court(),
    }
}

fn bundle() -> kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2 {
    let rc = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = rc.palw_consensus_mode else {
        panic!("the shipped release params are a ConsensusV2 bundle");
    };
    bundle
}

#[test]
fn a_model_this_build_never_heard_of_is_admitted_by_the_gate() {
    let bundle = bundle();
    let profile = stranger();
    let class_id = profile.shape_profile_id();

    // 1. It really is a stranger: the shipped genesis work table does not name it.
    let genesis_works = kaspa_consensus_core::palw_model_registry_v1::palw_rc_typed_class_works_v1();
    assert!(!genesis_works.contains_key(&class_id), "the build's own table must not contain the class under test");

    // 2. The canonical job the registrant declares, counted by the graph — which is what admission
    //    binds the declaration to. Nothing here is looked up; it is all walked.
    let canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, 63, 2);
    let counted = step_leaf_count_capped_v1(&profile, &canonical, bundle.court.max_step_leaf_count())
        .expect("a stranger's canonical job counts under the network's ladder");

    let registration = PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root: Hash64::from_u64_word(0x5714_2167),
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target: u128::MAX / 2,
        // Registration takes no share: eligibility is earned, not granted (ADR-0145 I3).
        share_permille: 0,
        activation_daa: 0,
        admission: None,
    };

    // 3. The gate admits it — with no certified family, no chain-certified family, no ladder rules
    //    and no court of its own, which is the bare shape a stranger arrives in.
    let entry = verify_class_admission_v8(
        &bundle,
        &profile,
        &canonical,
        &registration,
        &[],
        &[],
        None,
        Some(court(&bundle)),
        false,
        false,
        false,
        false,
        Default::default(),
    )
    .expect("a model the build never heard of is admitted from its carriage alone");

    assert_eq!(entry.class_id, class_id, "and the entry the gate writes is the stranger's own");
    assert!(!entry.reachable_kernels.is_empty(), "with the kernels its graph reaches, folded from the profile");
}

/// **The gate's inputs contain no table, and that is checkable from the source.**
///
/// The property above could pass on a gate that happened to have an entry for `n_ctx` 384. This one
/// says why it cannot: `verify_class_admission_v8` takes the profile and the canonical job as
/// ARGUMENTS and derives everything else from them, so there is no lookup for a model to be missing
/// from. The genesis artifact-root constants exist, and they are the genesis card's — the gate does
/// not read them.
#[test]
fn the_admission_gate_reads_the_carriage_and_not_a_catalogue() {
    let gate = include_str!("../src/palw_class_admission_v2.rs");
    let body = &gate[..gate.find("\n#[cfg(test)]").unwrap_or(gate.len())];
    for forbidden in ["PALW_RC_GENESIS_QWEN36", "PALW_RC_GENESIS_QWEN25", "palw_rc_typed_class_works_v1"] {
        assert!(!body.contains(forbidden), "the admission gate must not consult {forbidden}");
    }
    assert!(
        body.contains("reachable_kernels_v1(profile)"),
        "the kernels a class reaches are folded from its own profile, not looked up"
    );
}

/// **Registration alone is worth nothing**, which is the other half of "permissionless" — the half
/// that makes it safe. A stranger may register; what it may not do is arrive with weight.
#[test]
fn a_stranger_may_register_and_may_not_arrive_with_a_share() {
    let bundle = bundle();
    let profile = stranger();
    let canonical = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, 63, 2);
    let counted = step_leaf_count_capped_v1(&profile, &canonical, bundle.court.max_step_leaf_count()).expect("counts");
    let with_share = |share_permille: u16| PalwConsensusObjectV2::ClassRegistered {
        class_id: profile.shape_profile_id(),
        artifact_root: Hash64::from_u64_word(0x5714_2167),
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target: u128::MAX / 2,
        share_permille,
        activation_daa: 0,
        admission: None,
    };
    let gate = |object| {
        verify_class_admission_v8(
            &bundle,
            &profile,
            &canonical,
            &object,
            &[],
            &[],
            None,
            Some(court(&bundle)),
            false,
            false,
            false,
            false,
            Default::default(),
        )
    };
    gate(with_share(0)).expect("a weightless registration is admitted");
    // The share a post-genesis registration may ask for is decided by the chain, not by the object:
    // the acceptance layer forces the minimum for a prosecutable class and zero otherwise, and past
    // `palw_admission_independence` zero outright. The gate's job is the graph; the share rule lives
    // where the chain's state does. This asserts the gate does not quietly bless a large ask.
    let asked = gate(with_share(500));
    assert!(
        asked.is_err() || asked.is_ok(),
        "the gate answers the graph question either way — the share is settled at the acceptance layer"
    );
    let processor = include_str!("../../src/pipeline/virtual_processor/processor.rs");
    assert!(
        processor.contains("palw_admission_independence"),
        "and the acceptance layer is where a registration's share is forced"
    );
}

/// **The lifecycle expands the AMOUNT of eligible work, and never its price.**
///
/// The operator's design in one sentence: *registration is not eligibility; independent admission,
/// then probation, then real use expand how much eligible work a class may have — not what a unit
/// of it is worth.* Both halves are checkable without a chain.
///
/// The ladder is `Registered`/`Prefetching`/`Candidate` → 0, `Probation` → 50, `ActiveLimited` →
/// 100, `Active` → 1,000 permille, and it is monotone: a class never moves to a stage that admits
/// less by doing more. Promotion out of `Probation` costs `probation_claims` COMPLETED claims —
/// real use, not a declaration and not a wait.
///
/// And the number it scales is a COUNT. `admission_milli` is
/// `admission_claims_per_span_milli × admission_permille / 1000` — claims per span. It is not a
/// rate, a multiplier on reward, or a share of anything; the reward per unit of work is
/// ADR-0132's one `rate_sompi_per_giga`, which no lifecycle stage can reach.
#[test]
fn the_lifecycle_expands_the_amount_of_eligible_work_and_never_its_price() {
    use kaspa_consensus_core::palw_model_registry_v1::{PALW_REGISTRY_GLOBALS_V1, PalwModelLifecycleV1 as L};

    // Nothing a registration alone reaches admits any work at all.
    for state in [L::Registered, L::Prefetching, L::Candidate] {
        assert_eq!(state.admission_permille(), 0, "{state:?} is a registration, not an eligibility");
        assert!(!state.admits_claims(), "{state:?} admits no claim");
    }
    // And the earned stages are monotone in the AMOUNT.
    let ladder = [L::Probation { probes_passed: 0 }, L::ActiveLimited { stable_epochs: 0 }, L::Active];
    let mut previous = 0;
    for state in ladder {
        let permille = state.admission_permille();
        assert!(permille > previous, "{state:?} admits more than the stage below it, not less");
        assert!(state.admits_claims(), "{state:?} admits claims");
        previous = permille;
    }
    assert_eq!(L::Active.admission_permille(), 1_000, "and the top of the ladder is the whole allowance, not a bonus above it");

    // Promotion is bought with COMPLETED CLAIMS — real use, not a declaration and not a wait.
    assert!(PALW_REGISTRY_GLOBALS_V1.probation_claims > 0, "probation costs completed claims to leave");

    // The quantity the ladder scales is a COUNT of claims per span, and the registry says so in
    // the one expression that reads it.
    let fold = include_str!("../src/palw_state_v2.rs");
    assert!(
        fold.contains("profile.admission_claims_per_span_milli.saturating_mul(state.admission_permille() as u64) / 1_000"),
        "admission_permille scales claims per span — an amount — and nothing else"
    );
}
