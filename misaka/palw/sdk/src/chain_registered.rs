//! Chain-registered resolution for a family this build's tables do not list.
//!
//! Dense and Qwen3.6 stay the compatibility arms inside [`crate::PalwClassSdk::resolve_chain_registered`]:
//! live source pins count those two applications, and an empty or custom `with_lineages` set still
//! resolves the same two containers. A further family implements [`ChainRegisteredFamilyV1`] and is
//! appended with `with_chain_registered_family`. The resolver does not grow a named branch for it.
//!
//! `NotThisFamily` is "this root is not mine". `Refused` is "it is mine, and this node cannot serve
//! it or the verification failed". A refusal is not offered to the next family.

use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_hashes::Hash64;

use crate::lineage::PalwLoadedArtifactV1;

pub enum ChainRegisteredAnswerV1 {
    NotThisFamily,
    Refused(String),
    Ready(Box<dyn PalwExecutionBackendV1>),
}

pub trait ChainRegisteredFamilyV1: Send + Sync {
    fn family_id(&self) -> &'static str;
    fn resolve_registered(
        &self,
        class_id: Hash64,
        artifact_root: Hash64,
        holdings: &[PalwLoadedArtifactV1],
        profile: &PalwShapeProfileV3,
        canonical: &PalwJobContextV2,
        network_id: &[u8],
        court: &PalwCourtParamsV2,
        prompt_ids_form: PalwPromptIdsFormV1,
    ) -> ChainRegisteredAnswerV1;
}

/// Registration uses one moment. The SDK's attempt rule and the moment's rule are the same value,
/// or the moment is refused. Block rules, claim-origin rules and node-local policy are different
/// moments and are not collapsed into the current tip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwRuleMomentKindV1 {
    ClassRegistration,
    BlockAcceptance { daa: u64 },
    ClaimOrigin { accepted_daa: u64 },
    LocalNodePolicy,
}

pub fn registration_moment_matches_sdk_v1(
    sdk_rules: kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1,
    moment_rules: kaspa_consensus_core::palw_attempt_rules_v1::PalwAttemptRulesV1,
    kind: PalwRuleMomentKindV1,
) -> Result<PalwRuleMomentKindV1, &'static str> {
    if kind != PalwRuleMomentKindV1::ClassRegistration {
        return Err("a registration moment is not a block, claim-origin or node-local moment");
    }
    if sdk_rules != moment_rules {
        return Err("the registration moment's attempt rule is not the rule this SDK resolves");
    }
    Ok(kind)
}
