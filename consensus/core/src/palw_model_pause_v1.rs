//! Pure model burn-pause planning for an exact on-chain class registration.
//! A plan has no signature, funded-carrier or native write authority.

use std::num::NonZeroU64;

use crate::{
    Hash64,
    constants::MAX_SOMPI,
    palw_artifact_identity_v1::palw_catalog_container_root_of_class_v1,
    palw_mode_v2::PalwConsensusParamsV2,
    palw_model_tier_v1::palw_model_tier_of_v1,
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModelPauseModeV1 {
    Active,
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModelPauseActionV1 {
    Pause,
    Resume,
}

/// Scope and compare-and-set generation to bind in any future carrier.
/// The action also fixes the expected mode: Pause requires Active, and
/// Resume requires Paused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ModelPauseIntentV1 {
    pub class: Hash64,
    pub artifact_root: Hash64,
    pub expected_generation: u64,
    pub action: ModelPauseActionV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ModelPauseStateV1 {
    class: Hash64,
    artifact_root: Hash64,
    generation: u64,
    mode: ModelPauseModeV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ModelPauseBurnPolicyV1 {
    pause: NonZeroU64,
    resume: NonZeroU64,
}

impl ModelPauseBurnPolicyV1 {
    pub(crate) fn new(pause: NonZeroU64, resume: NonZeroU64) -> Result<Self, ModelPauseRefusalV1> {
        if pause.get() > MAX_SOMPI || resume.get() > MAX_SOMPI {
            return Err(ModelPauseRefusalV1::BurnExceedsMoneyBound);
        }
        Ok(Self { pause, resume })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModelPauseRefusalV1 {
    UnknownClass,
    ClassNotYetRegistered,
    ExemptModel,
    MissingRegistrant,
    WrongArtifactRoot,
    WrongStateScope,
    StaleGeneration,
    AlreadyInRequestedMode,
    GenerationOverflow,
    BurnExceedsMoneyBound,
}

/// Additional authorization above the ordinary funded transaction checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModelPauseAuthorizationV1 {
    AnyPayer,
    Registrant(PalwBondKeyV2),
}

/// An unsigned candidate with explicit requirements. The accepting path must
/// recompute this plan at its canonical position, authenticate Resume against
/// the registrant's native key, and atomically write the state and allocate
/// the burn from the same validated carrier's remaining fee. Refused or
/// duplicate controls must not take the additional burn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UnsignedModelPausePlanV1 {
    pub next: ModelPauseStateV1,
    pub authorization: ModelPauseAuthorizationV1,
    pub required_burn: NonZeroU64,
}

fn registered_model_v1(
    state: &PalwChainStateV2,
    params: &PalwConsensusParamsV2,
    class: Hash64,
    daa: u64,
) -> Result<(Hash64, PalwBondKeyV2), ModelPauseRefusalV1> {
    let record = state.class(&class).ok_or(ModelPauseRefusalV1::UnknownClass)?;
    if record.registered_daa > daa {
        return Err(ModelPauseRefusalV1::ClassNotYetRegistered);
    }
    // The tier convenience function treats an absent registrant as
    // GenesisAssembly. This exemption needs the actual genesis object or a
    // matching entry in the native approved catalog.
    use PalwConsensusObjectV2::{ClassRegistered, ClassRegisteredGenV1, ClassRegisteredTirV1};
    let genesis = params.genesis_objects.iter().any(|object| match object {
        ClassRegistered { class_id, artifact_root, .. }
        | ClassRegisteredGenV1 { class_id, artifact_root, .. }
        | ClassRegisteredTirV1 { class_id, artifact_root, .. } => (*class_id, *artifact_root) == (class, record.artifact_root),
        _ => false,
    });
    let mirror = params.state.model_tier_v1();
    if palw_model_tier_of_v1(mirror, &class, &record.artifact_root, genesis, daa).is_approved()
        || palw_catalog_container_root_of_class_v1(state, &class, daa)
            .is_some_and(|container| palw_model_tier_of_v1(mirror, &class, &container.hash(), false, daa).is_approved())
    {
        return Err(ModelPauseRefusalV1::ExemptModel);
    }
    let registrant = record.registrant_bond.ok_or(ModelPauseRefusalV1::MissingRegistrant)?;
    Ok((record.artifact_root, registrant))
}

fn current_model_pause_v1(
    class: Hash64,
    artifact_root: Hash64,
    row: Option<&ModelPauseStateV1>,
) -> Result<(u64, ModelPauseModeV1), ModelPauseRefusalV1> {
    match row {
        None => Ok((0, ModelPauseModeV1::Active)),
        Some(row) if (row.class, row.artifact_root) == (class, artifact_root) => Ok((row.generation, row.mode)),
        Some(_) => Err(ModelPauseRefusalV1::WrongStateScope),
    }
}

/// `None` means the burn-pause gate is inapplicable to an approved model.
/// The caller must separately enforce the class's ordinary status and claim
/// rules. A previously paused class stops being subject to this gate when
/// its annual designation becomes effective.
pub(crate) fn effective_model_pause_v1(
    state: &PalwChainStateV2,
    params: &PalwConsensusParamsV2,
    class: Hash64,
    daa: u64,
    row: Option<&ModelPauseStateV1>,
) -> Result<Option<ModelPauseModeV1>, ModelPauseRefusalV1> {
    match registered_model_v1(state, params, class, daa) {
        Ok((root, _)) => Ok(Some(current_model_pause_v1(class, root, row)?.1)),
        Err(ModelPauseRefusalV1::ExemptModel) => Ok(None),
        Err(error) => Err(error),
    }
}

pub(crate) fn plan_model_pause_v1(
    state: &PalwChainStateV2,
    params: &PalwConsensusParamsV2,
    daa: u64,
    row: Option<&ModelPauseStateV1>,
    intent: &ModelPauseIntentV1,
    policy: ModelPauseBurnPolicyV1,
) -> Result<UnsignedModelPausePlanV1, ModelPauseRefusalV1> {
    use ModelPauseAuthorizationV1::{AnyPayer, Registrant};
    use ModelPauseModeV1::{Active, Paused};
    let (artifact_root, registrant) = registered_model_v1(state, params, intent.class, daa)?;
    if intent.artifact_root != artifact_root {
        return Err(ModelPauseRefusalV1::WrongArtifactRoot);
    }
    let (generation, mode) = current_model_pause_v1(intent.class, artifact_root, row)?;
    if generation != intent.expected_generation {
        return Err(ModelPauseRefusalV1::StaleGeneration);
    }
    let (next_mode, authorization, required_burn) = match intent.action {
        ModelPauseActionV1::Pause => (Paused, AnyPayer, policy.pause),
        ModelPauseActionV1::Resume => (Active, Registrant(registrant), policy.resume),
    };
    if mode == next_mode {
        return Err(ModelPauseRefusalV1::AlreadyInRequestedMode);
    }
    let generation = generation.checked_add(1).ok_or(ModelPauseRefusalV1::GenerationOverflow)?;
    // Retain the generation on Resume as well, so a stale Pause cannot bind
    // to a fresh-looking Active state after a complete cycle.
    let next = ModelPauseStateV1 { class: intent.class, artifact_root, generation, mode: next_mode };
    Ok(UnsignedModelPausePlanV1 { next, authorization, required_burn })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::params::{ForkActivation, Params},
        network::{NetworkId, NetworkType},
        palw_annual_catalog_v1::{ApprovedModelV1, PalwAnnualCatalogChainV1},
        palw_mode_v2::PalwConsensusMode,
        palw_model_tier_v1::PalwModelTierFenceV1,
        palw_state_v2::{PalwClassStateV2, PalwClassStatusV2, PalwPwuRuleV2},
        tx::TransactionOutpoint,
    };

    fn h(word: u64) -> Hash64 {
        Hash64::from_u64_word(word)
    }

    fn world() -> (PalwChainStateV2, PalwConsensusParamsV2, ModelPauseIntentV1, PalwBondKeyV2) {
        let params = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
        let PalwConsensusMode::ConsensusV2(bundle) = params.palw_consensus_mode else { panic!("T12 is V2") };
        let registrant = PalwBondKeyV2(TransactionOutpoint::new(h(3), 0));
        let intent =
            ModelPauseIntentV1 { class: h(1), artifact_root: h(2), expected_generation: 0, action: ModelPauseActionV1::Pause };
        let mut state = PalwChainStateV2::genesis();
        state.set_class_for_tests(
            intent.class,
            PalwClassStateV2 {
                artifact_root: intent.artifact_root,
                slash_value_per_pwu: 1,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1),
                status: PalwClassStatusV2::Frozen { since_daa: 12 },
                registered_daa: 11,
                registrant_bond: Some(registrant),
                fused_attention: false,
            },
        );
        (state, bundle, intent, registrant)
    }

    fn burn() -> ModelPauseBurnPolicyV1 {
        ModelPauseBurnPolicyV1::new(NonZeroU64::new(10).unwrap(), NonZeroU64::new(20).unwrap()).unwrap()
    }

    #[test]
    fn model_pause_v1_derives_distinct_authorization_and_blocks_stale_replay() {
        use ModelPauseActionV1::{Pause, Resume};
        use ModelPauseAuthorizationV1::{AnyPayer, Registrant};
        use ModelPauseModeV1::{Active, Paused};
        use ModelPauseRefusalV1::{AlreadyInRequestedMode, StaleGeneration};
        let (state, params, mut intent, registrant) = world();
        let class = intent.class;
        let class_before = state.class(&class).unwrap().clone();
        let plan = |row, intent: &ModelPauseIntentV1| plan_model_pause_v1(&state, &params, 20, row, intent, burn());
        let mode = |row| effective_model_pause_v1(&state, &params, class, 20, row);
        assert_eq!(mode(None), Ok(Some(Active)));
        assert_eq!(plan(None, &ModelPauseIntentV1 { action: Resume, ..intent }), Err(AlreadyInRequestedMode));
        let pause = plan(None, &intent).unwrap();
        assert_eq!((pause.authorization, pause.required_burn.get(), pause.next.generation), (AnyPayer, 10, 1));
        assert_eq!(mode(Some(&pause.next)), Ok(Some(Paused)));
        assert_eq!(plan(Some(&pause.next), &intent), Err(StaleGeneration));
        intent.expected_generation = 1;
        assert_eq!(plan(Some(&pause.next), &intent), Err(AlreadyInRequestedMode));
        intent.action = Resume;
        let resume = plan(Some(&pause.next), &intent).unwrap();
        assert_eq!((resume.authorization, resume.required_burn.get(), resume.next.generation), (Registrant(registrant), 20, 2));
        assert_eq!(mode(Some(&resume.next)), Ok(Some(Active)));
        intent.action = Pause;
        intent.expected_generation = 0;
        assert_eq!(plan(Some(&resume.next), &intent), Err(StaleGeneration));
        assert_eq!(state.class(&class), Some(&class_before));
        assert!(matches!(class_before.status, PalwClassStatusV2::Frozen { .. }));
    }

    #[test]
    fn model_pause_v1_checks_exact_scope_and_exempts_catalog_designations() {
        use ModelPauseRefusalV1::{
            ClassNotYetRegistered, ExemptModel, GenerationOverflow, MissingRegistrant, UnknownClass, WrongArtifactRoot,
            WrongStateScope,
        };
        let (mut state, params, intent, _) = world();
        let plan = |daa, row, intent: &ModelPauseIntentV1| plan_model_pause_v1(&state, &params, daa, row, intent, burn());
        assert_eq!(plan(20, None, &ModelPauseIntentV1 { class: h(9), ..intent }), Err(UnknownClass));
        assert_eq!(plan(10, None, &intent), Err(ClassNotYetRegistered));
        assert_eq!(plan(20, None, &ModelPauseIntentV1 { artifact_root: h(9), ..intent }), Err(WrongArtifactRoot));
        let pause = plan(20, None, &intent).unwrap();
        let foreign = ModelPauseStateV1 { class: h(9), ..pause.next };
        assert_eq!(plan(20, Some(&foreign), &intent), Err(WrongStateScope));
        let exhausted = ModelPauseStateV1 { generation: u64::MAX, ..pause.next };
        assert_eq!(
            plan(
                20,
                Some(&exhausted),
                &ModelPauseIntentV1 { expected_generation: u64::MAX, action: ModelPauseActionV1::Resume, ..intent }
            ),
            Err(GenerationOverflow)
        );
        let approved = ApprovedModelV1::of(intent.class, intent.artifact_root);
        let mut tier = PalwModelTierFenceV1::at(ForkActivation::new(12));
        tier.catalogs = PalwAnnualCatalogChainV1::genesis_only(vec![]).with_edition(30, vec![approved.clone()]);
        let mut tiered = params.clone();
        tiered.state = params.state.clone().with_model_tier_v1(Some(tier.mirror()));
        assert_eq!(effective_model_pause_v1(&state, &tiered, intent.class, 29, Some(&pause.next)), Ok(Some(ModelPauseModeV1::Paused)));
        assert_eq!(effective_model_pause_v1(&state, &tiered, intent.class, 30, Some(&pause.next)), Ok(None));
        let resume = ModelPauseIntentV1 { expected_generation: 1, action: ModelPauseActionV1::Resume, ..intent };
        assert_eq!(plan_model_pause_v1(&state, &tiered, 30, Some(&pause.next), &resume, burn()), Err(ExemptModel));

        tier.catalogs = PalwAnnualCatalogChainV1::genesis_only(vec![approved]);
        let mut genesis_list = params.clone();
        genesis_list.state = params.state.clone().with_model_tier_v1(Some(tier.mirror()));
        assert_eq!(plan_model_pause_v1(&state, &genesis_list, 20, None, &intent, burn()), Err(ExemptModel));
        let record = PalwClassStateV2 { registrant_bond: None, ..state.class(&intent.class).unwrap().clone() };
        state.set_class_for_tests(intent.class, record.clone());
        assert_eq!(plan_model_pause_v1(&state, &params, 20, None, &intent, burn()), Err(MissingRegistrant));
        assert_eq!(effective_model_pause_v1(&state, &params, intent.class, 20, None), Err(MissingRegistrant));
        assert_eq!(plan_model_pause_v1(&state, &tiered, 30, None, &intent, burn()), Err(ExemptModel));

        let (class, artifact_root) = params
            .genesis_objects
            .iter()
            .find_map(|object| match object {
                PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } => Some((*class_id, *artifact_root)),
                _ => None,
            })
            .expect("testnet-12 has a genesis class");
        let genesis_intent = ModelPauseIntentV1 { class, artifact_root, ..intent };
        state.set_class_for_tests(class, PalwClassStateV2 { artifact_root, registered_daa: 0, ..record.clone() });
        assert_eq!(plan_model_pause_v1(&state, &params, 20, None, &genesis_intent, burn()), Err(ExemptModel));
        assert_eq!(effective_model_pause_v1(&state, &params, class, 20, None), Ok(None));
        state.set_class_for_tests(class, PalwClassStateV2 { artifact_root: h(9), registered_daa: 0, ..record });
        assert_eq!(effective_model_pause_v1(&state, &params, class, 20, None), Err(MissingRegistrant));
        assert_eq!(ModelPauseBurnPolicyV1::new(NonZeroU64::MAX, burn().resume), Err(ModelPauseRefusalV1::BurnExceedsMoneyBound));
    }
}
