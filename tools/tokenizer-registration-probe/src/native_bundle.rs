// Private measurement file format shared with the opt-in core fixture.
use kaspa_consensus_core::{
    Hash64,
    palw_mode_v2::PalwCourtParamsV2,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1,
    palw_state_v2::{PalwBlockContextV2, PalwStateParamsV2, PalwTransitionExtrasV1},
    palw_tir_frame_cost_v1::PalwTirFrameClockV1,
    palw_tir_public_allocation_v1::PalwTirPublicAllocationRulesV1,
    palw_tokenizer_registration_v1::TokenizerRegistrationPolicyV1,
};

pub const VERSION: u16 = 1;
pub const MAX_ACTIONS: usize = 2 * 128 + 3;
pub const MANIFEST_FILE: &str = "manifest.borsh";
pub const STATE_FILE: &str = "initial-state.borsh";

pub fn action_file(index: usize) -> String {
    format!("action-{index:03}.tkr1")
}

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct AllocationRules {
    maximum_leaves: u64,
    maximum_custodians: u64,
    court: PalwCourtParamsV2,
    dissection: bool,
    prompt_form: u8,
    config: PalwStagedPublicCourtConfigV1,
    clock: PalwTirFrameClockV1,
}

impl From<PalwTirPublicAllocationRulesV1> for AllocationRules {
    fn from(rules: PalwTirPublicAllocationRulesV1) -> Self {
        Self {
            maximum_leaves: rules.maximum_leaves,
            maximum_custodians: rules.maximum_custodians,
            court: rules.court,
            dissection: rules.dissection,
            prompt_form: match rules.prompt_form {
                PalwPromptIdsFormV1::Flat => 0,
                PalwPromptIdsFormV1::MerkleV1 => 1,
            },
            config: rules.config,
            clock: rules.clock,
        }
    }
}

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Action {
    pub at: PalwBlockContextV2,
    pub txid: Hash64,
    pub paid: u64,
    pub burn: u64,
    pub collectible: u64,
    pub charge_id: Option<Hash64>,
    pub parent_root: Hash64,
    pub next_root: Hash64,
    pub delta_bytes: u64,
}

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Manifest {
    pub version: u16,
    pub case: String,
    pub params: PalwStateParamsV2,
    pub policy: TokenizerRegistrationPolicyV1,
    pub network: Hash64,
    pub allocation_network: Hash64,
    pub allocation: AllocationRules,
    pub initial: PalwBlockContextV2,
    pub initial_root: Hash64,
    pub open_ids: [Hash64; 2],
    pub snapshot_before: u32,
    pub snapshot_bytes: u64,
    pub actions: Vec<Action>,
}

impl Manifest {
    /// The complete nondefault subset of the seeded fixture's extras.
    pub fn extras(&self) -> PalwTransitionExtrasV1 {
        let a = &self.allocation;
        let prompt_form = match a.prompt_form {
            0 => PalwPromptIdsFormV1::Flat,
            1 => PalwPromptIdsFormV1::MerkleV1,
            _ => panic!("unknown measurement prompt form"),
        };
        PalwTransitionExtrasV1 {
            class_semantics_active: true,
            tir_container_identity_active: true,
            tir_public_conviction_active: true,
            staged_public_court: Some(a.config),
            tir_public_allocation_rules_v1: Some(PalwTirPublicAllocationRulesV1 {
                maximum_leaves: a.maximum_leaves,
                maximum_custodians: a.maximum_custodians,
                court: a.court,
                dissection: a.dissection,
                prompt_form,
                config: a.config,
                clock: a.clock,
            }),
            tir_public_network_domain_v1: Some(self.allocation_network),
            tokenizer_registration_policy_v1: Some(self.policy),
            tokenizer_registration_network_domain_v1: Some(self.network),
            ..Default::default()
        }
    }
}
