//! Bounded derivation of the immutable rendering facts for a prospective
//! tokenizer check. This is a read-only verifier, with no accepted state or
//! clock effect. EOG comes from the registered table, independently of JSON.

use crate::{
    Hash64,
    palw_class_semantics_v1::PalwClassSemanticsV1,
    palw_fp_constraint_job_v1::{
        PALW_TOKEN_TABLE_MAX_LEAVES_V1, PalwTokenTableEogOpeningV1, PalwTokenTableOpeningV1, palw_fp_constraint_table_root_v1,
        table_leaf_hash_v1,
    },
    palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    palw_staged_court_state_v1::{PalwStagedCourtKeyV1, PalwStagedCourtRowV1},
    palw_state_v2::{
        PalwChainStateV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1, palw_tir_public_original_quote_v1,
    },
    palw_step_leg::PalwStepPrefixStreamV1,
    palw_tir_container_v1::{PalwTirContainerRowV1, palw_tir_bound_container_v1},
    palw_tir_public_allocation_v1::PalwTirPublicAllocationPlanV1,
    palw_tir_public_source_v1::{PalwTirPublicAuthenticatedIdsV1, PalwTirPublicSourceV1},
    palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryCertificateV1,
    palw_tokenizer_rendering_v1::{
        PalwQwenRenderingMapV1, PalwTokenizerRenderingRuleV1, palw_tokenizer_raw_commitment_v1, palw_unique_json_v1,
        tokenizer_key_events_from_unique_json,
    },
};
use kaspa_core::memory_budget::{MemoryBudget, MemoryBudgetError, MemoryClass, global_memory_budget};
use std::sync::Arc;

/// The caller supplies the immutable tokenizer identity and registered table
/// context, independently of the submitted raw bytes and EOG opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTokenizerRenderingStatementV2 {
    pub tokenizer_id: Hash64,
    pub exact_raw_bytes: u64,
    pub table_root: Hash64,
    pub vocab: u32,
    pub rule: PalwTokenizerRenderingRuleV1,
}

/// Local work limits for this single pass. A native caller must also budget
/// the JSON parser's transient allocations and reserve its processing slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize)]
pub struct PalwTokenizerRenderingBudgetV2 {
    pub raw_bytes: u64,
    pub vocab: u32,
    pub key_events: u64,
    pub eog_ids: u32,
    pub total_rendering_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[non_exhaustive]
pub struct PalwDerivedTokenizerRenderingV2 {
    pub statement: PalwTokenizerRenderingStatementV2,
    pub key_events: u64,
    pub maximum_rendering_bytes: u64,
    pub total_rendering_bytes: u64,
    pub eog_leaf: Hash64,
    pub eog_count: u32,
    pub lowest_eog_id: u32,
}

/// Rebuild all token leaves in ID order against the original promote-odd
/// tree. The authenticated last (EOG) opening lets the checker retain only
/// a Merkle frontier, without storing the table or allocating for sparse IDs.
pub fn derive_tokenizer_rendering_v2(
    statement: PalwTokenizerRenderingStatementV2,
    raw: &[u8],
    eog: &PalwTokenTableEogOpeningV1,
    budget: PalwTokenizerRenderingBudgetV2,
) -> Result<PalwDerivedTokenizerRenderingV2, String> {
    check_operands(statement, raw, eog, budget)?;
    let leaf_count = u64::from(statement.vocab) + 1;
    let lowest_eog_id = eog.lowest().ok_or("tokenizer table has no EOG")?;
    if eog.ids.iter().any(|id| *id >= statement.vocab) || !eog.verifies(&statement.table_root, statement.vocab) {
        return Err("tokenizer EOG does not open the registered table".into());
    }
    if palw_tokenizer_raw_commitment_v1(raw) != statement.tokenizer_id {
        return Err("tokenizer does not open the exact raw commitment".into());
    }

    let json = palw_unique_json_v1(raw)?;
    let key_events = tokenizer_key_events_from_unique_json(&json)?;
    if key_events > budget.key_events {
        return Err("tokenizer exceeds its key event limit".into());
    }
    let rendering = match statement.rule {
        PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1 => PalwQwenRenderingMapV1::from_unique_json(&json)?,
    };
    drop(json);
    if rendering.highest_id() >= statement.vocab {
        return Err("tokenizer names a token outside the registered vocabulary".into());
    }

    let mut tree = PalwStepPrefixStreamV1::new_capped_v1(leaf_count, &eog.opening, &[], PALW_TOKEN_TABLE_MAX_LEAVES_V1)
        .map_err(|err| err.to_string())?;
    let mut maximum_rendering_bytes = 0;
    let mut total_rendering_bytes = 0u64;
    for id in 0..statement.vocab {
        let entry = rendering.rendering(id);
        let len = entry.as_ref().map_or(0, |bytes| bytes.len() as u64);
        if len > u64::from(u32::MAX) {
            return Err("tokenizer rendering exceeds the original table format".into());
        }
        maximum_rendering_bytes = maximum_rendering_bytes.max(len);
        total_rendering_bytes = total_rendering_bytes
            .checked_add(len)
            .filter(|total| *total <= budget.total_rendering_bytes)
            .ok_or("tokenizer exceeds its total rendering byte limit")?;
        tree.push(table_leaf_hash_v1(id, entry.as_deref())).map_err(|err| err.to_string())?;
    }
    let (root, _) = tree.finish().map_err(|err| err.to_string())?;
    if root != statement.table_root {
        return Err("derived token renderings differ from the registered table".into());
    }
    Ok(PalwDerivedTokenizerRenderingV2 {
        statement,
        key_events,
        maximum_rendering_bytes,
        total_rendering_bytes,
        eog_leaf: eog.opening.leaf_hash,
        eog_count: eog.ids.len() as u32,
        lowest_eog_id,
    })
}

fn check_operands(
    statement: PalwTokenizerRenderingStatementV2,
    raw: &[u8],
    eog: &PalwTokenTableEogOpeningV1,
    budget: PalwTokenizerRenderingBudgetV2,
) -> Result<(), String> {
    let leaf_count = u64::from(statement.vocab) + 1;
    if statement.exact_raw_bytes == 0
        || statement.exact_raw_bytes != raw.len() as u64
        || statement.exact_raw_bytes > budget.raw_bytes
        || statement.vocab == 0
        || statement.vocab > budget.vocab
        || leaf_count > PALW_TOKEN_TABLE_MAX_LEAVES_V1
        || eog.ids.len() > budget.eog_ids as usize
        || eog.ids.len() > statement.vocab as usize
    {
        return Err("tokenizer rendering operands exceed their exact length or work limits".into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize)]
struct PreparedTokenizerIdentityV2 {
    semantics: Hash64,
    artifact_root: Hash64,
    graph_ir_root: Hash64,
    layout_digest: Hash64,
    eof: PalwTokenizerArtifactInventoryCertificateV1,
    declared_key_events: u64,
    statement: PalwTokenizerRenderingStatementV2,
}

impl PreparedTokenizerIdentityV2 {
    fn resolve(
        state: &PalwChainStateV2,
        class_id: &Hash64,
        artifact_root: &Hash64,
        semantics: &PalwClassSemanticsV1,
        applicable_daa: u64,
    ) -> Option<Self> {
        let record = state.tir_class_v1(class_id)?;
        let eof = state.staged_tokenizer_inventory_v1(semantics.tokenizer_id)?;
        if !semantics.well_formed_for(class_id, artifact_root)
            || semantics.bound_from_daa > applicable_daa
            || record.facts.class_id != *class_id
            || record.tokenizer_id != semantics.tokenizer_id
            || eof.check_v1().is_err()
            || eof.artifact_hash != semantics.tokenizer_id
            || eof.completed_daa > applicable_daa
            || semantics.raw_tokenizer_bytes? != eof.actual_bytes
        {
            return None;
        }
        Some(Self {
            semantics: semantics.commitment(),
            artifact_root: *artifact_root,
            graph_ir_root: record.graph_ir_root,
            layout_digest: record.layout_digest,
            eof: eof.clone(),
            declared_key_events: semantics.tokenizer_key_events?,
            statement: PalwTokenizerRenderingStatementV2 {
                tokenizer_id: record.tokenizer_id,
                exact_raw_bytes: eof.actual_bytes,
                table_root: semantics.token_table_root?,
                vocab: record.logits_vocab,
                rule: semantics.rendering_rule?,
            },
        })
    }
}

/// Operator pins for the exact native registrations being prepared. The
/// inventory id is the signed offer's id, read back after native EOF acceptance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTfp1TokenizerIntakePinsV2 {
    pub class_id: Hash64,
    pub container_root: Hash64,
    pub semantics_id: Hash64,
    pub inventory_id: Hash64,
}

/// Local intake refusal and native quote errors stay distinct. A cold cache
/// or changed local pin must never become a consensus-invalid verdict.
#[derive(Debug, thiserror::Error)]
pub enum PalwTfp1TokenizerIntakeQuoteErrorV2 {
    #[error("local tokenizer intake unavailable: {0}")]
    Local(&'static str),
    #[error(transparent)]
    Native(#[from] PalwStateV2Error),
}

fn tfp1_tokenizer_intake_active(extras: &PalwTransitionExtrasV1) -> bool {
    extras.class_semantics_active
        && extras.tir_container_identity_active
        && extras.tir_public_conviction_active
        && extras.staged_public_court.is_some()
}

const VERIFIED_TOKENIZER_REGISTRATION_VERSION_V1: u16 = 1;
const VERIFIED_TOKENIZER_REGISTRATION_DOMAIN_V1: &[u8] = b"misaka-palw/verified-tokenizer-registration/v1";

#[derive(Debug, thiserror::Error)]
pub enum PalwVerifiedTokenizerRegistrationErrorV1 {
    #[error("tokenizer registration does not match the accepted parent: {0}")]
    Parent(&'static str),
    #[error("tokenizer registration evidence refused: {0}")]
    Evidence(String),
    #[error("local tokenizer registration capacity: {0}")]
    Capacity(#[from] MemoryBudgetError),
}

/// Fixed-size candidate value for a future rooted tokenizer record. Created
/// only by re-executing the parser and verifying the full table/EOG. Encoding
/// provides a reproducible identity; it cannot confer verified provenance.
/// V1 fixes the Qwen V1 rendering interpreter and its JSON/EOF semantics.
#[derive(Debug, PartialEq, Eq, borsh::BorshSerialize)]
pub struct PalwVerifiedTokenizerRegistrationV1 {
    version: u16,
    pins: PalwTfp1TokenizerIntakePinsV2,
    identity: PreparedTokenizerIdentityV2,
    container: PalwTirContainerRowV1,
    limits: PalwTokenizerRenderingBudgetV2,
    facts: PalwDerivedTokenizerRenderingV2,
}

impl PalwVerifiedTokenizerRegistrationV1 {
    /// Rebuild on the accepted parent independently of any producer cache.
    /// The caller supplies its effective work limits and bounded retained
    /// input; the additional parser work is reserved before any input hashing.
    /// A future native writer must fix `limits` in its ruleset and acquire the
    /// authenticated operands through its own funded, replayable input path.
    #[allow(clippy::too_many_arguments)]
    pub fn derive_v1(
        parent: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        now_daa: u64,
        pins: PalwTfp1TokenizerIntakePinsV2,
        raw: &[u8],
        eog: &PalwTokenTableEogOpeningV1,
        limits: PalwTokenizerRenderingBudgetV2,
    ) -> Result<Option<Self>, PalwVerifiedTokenizerRegistrationErrorV1> {
        if !tfp1_tokenizer_intake_active(extras) {
            return Ok(None);
        }
        let (identity, container) = PalwTokenizerRenderingCacheV2::tfp1_identity(parent, now_daa, &pins)
            .map_err(PalwVerifiedTokenizerRegistrationErrorV1::Parent)?;
        Self::from_operands(identity, container, pins, raw, eog, limits).map(Some)
    }

    fn from_operands(
        identity: PreparedTokenizerIdentityV2,
        container: &PalwTirContainerRowV1,
        pins: PalwTfp1TokenizerIntakePinsV2,
        raw: &[u8],
        eog: &PalwTokenTableEogOpeningV1,
        limits: PalwTokenizerRenderingBudgetV2,
    ) -> Result<Self, PalwVerifiedTokenizerRegistrationErrorV1> {
        let facts = derive_with_native_scratch(&identity, raw, eog, limits, global_memory_budget())?;
        Ok(Self { version: VERIFIED_TOKENIZER_REGISTRATION_VERSION_V1, pins, identity, container: container.clone(), limits, facts })
    }

    /// The expected digest of a persisted value can be checked after a cold
    /// re-derivation. There is intentionally no deserialize-as-verified API.
    pub fn commitment_v1(&self) -> Hash64 {
        verified_tokenizer_commitment_v1(self)
    }

    /// Use only in-process verified values on a current parent. Exact
    /// registrations can survive reorgs; missing/replaced registrations or
    /// different work limits must go through a fresh native derivation.
    pub fn applies_to_parent_v1(
        &self,
        parent: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        now_daa: u64,
        limits: PalwTokenizerRenderingBudgetV2,
    ) -> bool {
        tfp1_tokenizer_intake_active(extras)
            && self.limits == limits
            && PalwTokenizerRenderingCacheV2::tfp1_identity(parent, now_daa, &self.pins)
                .is_ok_and(|(identity, container)| self.identity == identity && &self.container == container)
    }

    pub fn facts_v1(&self) -> &PalwDerivedTokenizerRenderingV2 {
        &self.facts
    }
}

fn verified_tokenizer_commitment_v1(value: &impl borsh::BorshSerialize) -> Hash64 {
    let mut digest = blake2b_simd::Params::new().hash_length(64).key(VERIFIED_TOKENIZER_REGISTRATION_DOMAIN_V1).to_state();
    value.serialize(&mut digest).expect("fixed tokenizer registration encodes into hash state");
    Hash64::from_bytes(digest.finalize().as_bytes().try_into().expect("64-byte registration digest"))
}

fn derive_with_native_scratch(
    identity: &PreparedTokenizerIdentityV2,
    raw: &[u8],
    eog: &PalwTokenTableEogOpeningV1,
    mut budget: PalwTokenizerRenderingBudgetV2,
    memory: &Arc<MemoryBudget>,
) -> Result<PalwDerivedTokenizerRenderingV2, PalwVerifiedTokenizerRegistrationErrorV1> {
    use PalwVerifiedTokenizerRegistrationErrorV1::Evidence;
    let statement = identity.statement;
    budget.key_events = budget.key_events.min(identity.declared_key_events);
    check_operands(statement, raw, eog, budget).map_err(Evidence)?;
    // Same conservative parser multiplier as the existing Node tokenizer
    // check. Includes separate EOG-sort scratch and bounded tree overhead.
    let scratch = statement
        .exact_raw_bytes
        .checked_mul(128)
        .and_then(|bytes| bytes.checked_add(4 * eog.ids.len() as u64 + 16_384))
        .ok_or_else(|| Evidence("tokenizer scratch estimate overflow".into()))?;
    let _lease = memory.try_reserve(MemoryClass::Native, scratch)?;
    derive_tokenizer_rendering_v2(statement, raw, eog, budget).map_err(Evidence)
}

struct PreparedTokenizerRenderingV2 {
    identity: PreparedTokenizerIdentityV2,
    facts: PalwDerivedTokenizerRenderingV2,
    registration: Option<PalwVerifiedTokenizerRegistrationV1>,
}

/// One process-local entry per instance for prospective TIR checks. Empty on restart.
/// Callers supply their current candidate-scoped state at each use. A reorg
/// can reuse the entry only if that state still resolves the exact same
/// registration and accepted EOF. No raw data or full table is retained.
/// This cache has no consensus, admission, scheduling or fee authority.
#[derive(Default)]
pub struct PalwTokenizerRenderingCacheV2 {
    prepared: Option<PreparedTokenizerRenderingV2>,
}

impl PalwTokenizerRenderingCacheV2 {
    fn class_identity(parent: &PalwChainStateV2, class_id: &Hash64, candidate_daa: u64) -> Option<PreparedTokenizerIdentityV2> {
        PreparedTokenizerIdentityV2::resolve(
            parent,
            class_id,
            &parent.class(class_id)?.artifact_root,
            parent.class_semantics_v1(class_id)?,
            candidate_daa,
        )
    }

    /// Check the current parent before acquiring the original material.
    pub fn class_is_prepared_v2(&self, parent: &PalwChainStateV2, class_id: &Hash64, candidate_daa: u64) -> bool {
        self.prepared.as_ref().is_some_and(|entry| {
            Self::class_identity(parent, class_id, candidate_daa).is_some_and(|identity| entry.identity == identity)
        })
    }

    /// Prepare a class before a prospective claim. On a matching cache hit,
    /// `raw` and `eog` are unused. Otherwise they are locally bounded inputs,
    /// and the checker reserves the extra parser scratch before hashing or
    /// parsing. The caller owns the budgets for acquisition and scheduling.
    pub fn prepare_class_v2(
        &mut self,
        parent: &PalwChainStateV2,
        class_id: &Hash64,
        candidate_daa: u64,
        raw: &[u8],
        eog: &PalwTokenTableEogOpeningV1,
        budget: PalwTokenizerRenderingBudgetV2,
    ) -> Result<(), String> {
        let identity = Self::class_identity(parent, class_id, candidate_daa)
            .ok_or("parent state has no applicable tokenizer registration and exact EOF")?;
        self.prepare(identity, raw, eog, budget, global_memory_budget())
    }

    fn prepare(
        &mut self,
        identity: PreparedTokenizerIdentityV2,
        raw: &[u8],
        eog: &PalwTokenTableEogOpeningV1,
        budget: PalwTokenizerRenderingBudgetV2,
        memory: &Arc<MemoryBudget>,
    ) -> Result<(), String> {
        if self.prepared.as_ref().is_some_and(|entry| entry.identity == identity) {
            return Ok(());
        }
        let facts = derive_with_native_scratch(&identity, raw, eog, budget, memory).map_err(|why| why.to_string())?;
        self.prepared = Some(PreparedTokenizerRenderingV2 { identity, facts, registration: None });
        Ok(())
    }

    fn tfp1_identity<'a>(
        parent: &'a PalwChainStateV2,
        now_daa: u64,
        pins: &PalwTfp1TokenizerIntakePinsV2,
    ) -> Result<(PreparedTokenizerIdentityV2, &'a PalwTirContainerRowV1), &'static str> {
        let point = parent.last_point().ok_or("tokenizer intake has no accepted parent point")?;
        if now_daa < point.daa_score {
            return Err("tokenizer intake candidate precedes the accepted parent");
        }
        let identity = Self::class_identity(parent, &pins.class_id, now_daa)
            .ok_or("tokenizer intake requires prospective class semantics and native accepted EOF")?;
        let container = palw_tir_bound_container_v1(parent, pins.class_id).ok_or("tokenizer intake requires a bound PALWART3")?;
        if identity.semantics != pins.semantics_id
            || identity.eof.inventory_id != pins.inventory_id
            || identity.eof.completed_daa > point.daa_score
            || container.container_root != pins.container_root
            || container.bound_daa > point.daa_score
        {
            return Err("tokenizer intake substitutes the accepted registration or signed EOF identity");
        }
        Ok((identity, container))
    }

    /// Read-only preclaim intake on the accepted parent. `Ok(false)` means
    /// dormant. The native fold has already verified the offer and paid
    /// inventory; this call rechecks the exact raw file and registered table
    /// under a local scratch lease before memoizing. It admits no claim.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_tfp1_intake_v2(
        &mut self,
        parent: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        now_daa: u64,
        pins: &PalwTfp1TokenizerIntakePinsV2,
        raw: &[u8],
        eog: &PalwTokenTableEogOpeningV1,
        budget: PalwTokenizerRenderingBudgetV2,
    ) -> Result<bool, String> {
        if !tfp1_tokenizer_intake_active(extras) {
            return Ok(false);
        }
        let (identity, container) = Self::tfp1_identity(parent, now_daa, pins)?;
        if self.prepared.as_ref().and_then(|entry| entry.registration.as_ref()).is_some_and(|record| {
            record.pins == *pins && record.identity == identity && &record.container == container && record.limits == budget
        }) {
            return Ok(true);
        }
        let registration = PalwVerifiedTokenizerRegistrationV1::from_operands(identity, container, *pins, raw, eog, budget)
            .map_err(|why| why.to_string())?;
        self.prepared = Some(PreparedTokenizerRenderingV2 {
            identity: registration.identity.clone(),
            facts: registration.facts.clone(),
            registration: Some(registration),
        });
        Ok(true)
    }

    /// Opt-in native quote boundary. The prepared entry is only a local
    /// prerequisite under the effective fences; the full native quote still
    /// owns all pricing, the admission horizon and the original's validation.
    #[allow(clippy::too_many_arguments)]
    pub fn tfp1_intake_quote_v2(
        &self,
        parent: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        extras: &PalwTransitionExtrasV1,
        now_daa: u64,
        pins: &PalwTfp1TokenizerIntakePinsV2,
        original: &PalwFreePromptCommitmentV3,
        header: &PalwPublicTirBindingHeaderV1,
    ) -> Result<Option<PalwTirPublicAllocationPlanV1>, PalwTfp1TokenizerIntakeQuoteErrorV2> {
        if tfp1_tokenizer_intake_active(extras) {
            use PalwTfp1TokenizerIntakeQuoteErrorV2::Local;
            let (identity, container) = Self::tfp1_identity(parent, now_daa, pins).map_err(Local)?;
            let semantics = parent.class_semantics_v1(&pins.class_id).ok_or(Local("tokenizer intake has no registered semantics"))?;
            if original.job.class_id != pins.class_id
                || original.job.tokenizer_id != identity.statement.tokenizer_id
                || !semantics.well_formed_for(&pins.class_id, &header.artifact_root)
                || !semantics.matches_context(&header.job_context)
                || palw_fp_constraint_table_root_v1(&original.job).is_some_and(|root| root != identity.statement.table_root)
            {
                return Err(Local("tokenizer intake quote changes the registered job or artifact"));
            }
            if !self
                .prepared
                .as_ref()
                .and_then(|entry| entry.registration.as_ref())
                .is_some_and(|record| record.pins == *pins && record.identity == identity && &record.container == container)
            {
                return Err(Local("tokenizer intake quote requires a current local preparation"));
            }
        }
        Ok(palw_tir_public_original_quote_v1(parent, params, extras, now_daa, original, header)?)
    }

    fn for_claim(&self, state: &PalwChainStateV2, claim_id: &Hash64) -> Option<&PreparedTokenizerRenderingV2> {
        let prepared = self.prepared.as_ref()?;
        let claim = state.claim(claim_id)?;
        if let Some(registration) = &prepared.registration {
            let container = palw_tir_bound_container_v1(state, claim.class_id)?;
            if registration.container != *container || container.bound_daa >= claim.accepted_daa {
                return None;
            }
        }
        let semantics = state.claim_semantics_v1(claim_id)?;
        let PalwStagedCourtRowV1::TirClaim(frame) = state.staged_court_row_v1(&PalwStagedCourtKeyV1::TirClaim(*claim_id))? else {
            return None;
        };
        let original = frame.original_v1();
        let header = frame.header_v1();
        if frame.plan_v1().original_claim != *claim_id
            || frame.plan_v1().terms.accepted_daa != claim.accepted_daa
            || frame.descriptor_v1().layout_digest != prepared.identity.layout_digest
            || original.execution_root != claim.execution_root
            || original.output_root != claim.output_root
            || !semantics.matches_context(&header.job_context)
            || palw_fp_constraint_table_root_v1(&original.job).is_some_and(|root| Some(root) != semantics.token_table_root)
        {
            return None;
        }
        let identity =
            PreparedTokenizerIdentityV2::resolve(state, &claim.class_id, &header.artifact_root, semantics, claim.accepted_daa)?;
        // The EOF row has a DAA but no intra-DAA ordering pin. Do not
        // retroactively qualify claims accepted at that same DAA.
        (identity.eof.completed_daa < claim.accepted_daa && prepared.identity == identity).then_some(prepared)
    }

    /// `None` means no applicable local result. The caller must derive
    /// `expected_id` from the authenticated claim trace and still check the
    /// claim's constraint and output bindings.
    pub fn verify_claim_token_v2(
        &self,
        state: &PalwChainStateV2,
        claim_id: &Hash64,
        expected_id: u32,
        opening: &PalwTokenTableOpeningV1,
    ) -> Option<bool> {
        Some(self.for_claim(state, claim_id)?.facts.verifies_rendering(expected_id, opening))
    }

    /// Read the authenticated EOG fallback for this claim without processing
    /// the JSON or opening the EOG list again.
    pub fn lowest_eog_for_claim_v2(&self, state: &PalwChainStateV2, claim_id: &Hash64) -> Option<u32> {
        Some(self.for_claim(state, claim_id)?.facts.lowest_eog_id)
    }
}

impl PalwDerivedTokenizerRenderingV2 {
    fn verifies_rendering(&self, expected_id: u32, opening: &PalwTokenTableOpeningV1) -> bool {
        opening.id == expected_id
            && opening.rendering.as_ref().map_or(0, |bytes| bytes.len() as u64) <= self.maximum_rendering_bytes
            && opening.verifies(&self.statement.table_root, self.statement.vocab)
    }

    fn verified_eog<'material>(&self, opening: &'material PalwTokenTableEogOpeningV1) -> Option<&'material [u32]> {
        (opening.ids.len() == self.eog_count as usize
            && opening.lowest() == Some(self.lowest_eog_id)
            && opening.opening.leaf_hash == self.eog_leaf
            && opening.verifies(&self.statement.table_root, self.statement.vocab))
        .then_some(opening.ids.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_fp_constraint_job_v1::PalwTokenTableV1;

    const RAW: &[u8] = br#"{"added_tokens":[{"id":2,"content":"long"}],"model":{"vocab":{"A":0,"b":2},"merges":[]}}"#;
    const BUDGET: PalwTokenizerRenderingBudgetV2 =
        PalwTokenizerRenderingBudgetV2 { raw_bytes: 1024, vocab: 8, key_events: 32, eog_ids: 2, total_rendering_bytes: 8 };

    fn fixture() -> (PalwTokenizerRenderingStatementV2, PalwTokenTableV1) {
        let table = PalwTokenTableV1 {
            entries: vec![Some(b"A".to_vec()), Some(vec![]), Some(b"long".to_vec()), None],
            eog_token_ids: vec![2],
            tokenizer_id: palw_tokenizer_raw_commitment_v1(RAW),
        };
        let statement = PalwTokenizerRenderingStatementV2 {
            tokenizer_id: table.tokenizer_id,
            exact_raw_bytes: RAW.len() as u64,
            table_root: table.root(),
            vocab: 4,
            rule: PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1,
        };
        (statement, table)
    }

    #[test]
    fn derives_full_table_bounds_with_override_hole_and_padding() {
        let (statement, table) = fixture();
        let eog = table.eog_opening().unwrap();
        let facts = derive_tokenizer_rendering_v2(statement, RAW, &eog, BUDGET).unwrap();
        assert_eq!(facts.statement, statement);
        assert_eq!(facts.key_events, 11);
        assert_eq!(facts.maximum_rendering_bytes, 4);
        assert_eq!(facts.total_rendering_bytes, 5);
        assert_eq!(facts.eog_leaf, eog.opening.leaf_hash);
        assert_eq!(facts.eog_count, 1);
        assert_eq!(facts.lowest_eog_id, 2);
        let registration = Hash64::from_u64_word(1);
        let bounds = PalwRootedTokenizerOpeningBoundsV1::from_facts(registration, &facts);
        assert_eq!(bounds.registration_id, registration);
        assert_eq!(bounds.eog_opening_bytes, borsh::object_length(&eog).unwrap() as u64);
        let openings = (0..statement.vocab).map(|id| table.opening_of(id).unwrap());
        let actual = openings.map(|opening| borsh::object_length(&opening).unwrap() as u64).max().unwrap();
        assert_eq!(bounds.maximum_rendering_opening_bytes, actual);
    }

    #[test]
    fn rooted_facts_require_exact_id_and_every_eog_member() {
        let (mut statement, mut table) = fixture();
        table.eog_token_ids = vec![1, 2];
        statement.table_root = table.root();
        let eog = table.eog_opening().unwrap();
        let facts = derive_tokenizer_rendering_v2(statement, RAW, &eog, BUDGET).unwrap();
        assert_eq!(facts.verified_eog(&eog), Some([1, 2].as_slice()));
        let mut shortened = eog.clone();
        shortened.ids.pop();
        assert_eq!(facts.verified_eog(&shortened), None);
        let mut changed = eog;
        changed.ids[1] = 3;
        assert_eq!(facts.verified_eog(&changed), None);
        let opening = table.opening_of(0).unwrap();
        assert!(facts.verifies_rendering(0, &opening));
        assert!(!facts.verifies_rendering(1, &opening));
        let mut oversized = opening;
        oversized.rendering = Some(b"unregistered".to_vec());
        assert!(!facts.verifies_rendering(0, &oversized));
    }

    #[test]
    fn rejects_wrong_table_even_with_its_own_valid_eog_opening() {
        let (mut statement, mut table) = fixture();
        table.entries[2] = Some(b"x".to_vec());
        let wrong = table.eog_opening().unwrap();
        assert!(derive_tokenizer_rendering_v2(statement, RAW, &wrong, BUDGET).is_err());
        statement.table_root = table.root();
        assert!(wrong.verifies(&statement.table_root, statement.vocab));
        assert!(derive_tokenizer_rendering_v2(statement, RAW, &wrong, BUDGET).is_err());
        let mut wrong_eog = wrong;
        wrong_eog.ids[0] = 1;
        assert!(derive_tokenizer_rendering_v2(statement, RAW, &wrong_eog, BUDGET).is_err());
    }

    #[test]
    fn refuses_authenticated_invalid_json_ids_and_premature_eof() {
        let (statement, table) = fixture();
        let eog = table.eog_opening().unwrap();
        for raw in [
            br#"{"model":{"vocab":{"a":0,"\u0061":1},"merges":[]}}"#.as_slice(),
            br#"{"model":{"vocab":{"a":0,"b":0},"merges":[]}}"#,
            br#"{"model":{"vocab":{},"merges":[]},"added_tokens":[{"id":0,"content":"a"},{"id":0,"content":"b"}]}"#,
            br#"{"model":{"vocab":{"a":4294967295},"merges":[]}}"#,
            &RAW[..RAW.len() - 1],
        ] {
            let input = PalwTokenizerRenderingStatementV2 {
                tokenizer_id: palw_tokenizer_raw_commitment_v1(raw),
                exact_raw_bytes: raw.len() as u64,
                ..statement
            };
            assert!(derive_tokenizer_rendering_v2(input, raw, &eog, BUDGET).is_err());
        }
        assert!(derive_tokenizer_rendering_v2(statement, &RAW[..RAW.len() - 1], &eog, BUDGET).is_err());
    }

    #[test]
    fn refuses_limits_below_derived_work() {
        let (statement, table) = fixture();
        let eog = table.eog_opening().unwrap();
        for budget in [
            PalwTokenizerRenderingBudgetV2 { raw_bytes: RAW.len() as u64 - 1, ..BUDGET },
            PalwTokenizerRenderingBudgetV2 { vocab: 3, ..BUDGET },
            PalwTokenizerRenderingBudgetV2 { key_events: 10, ..BUDGET },
            PalwTokenizerRenderingBudgetV2 { eog_ids: 0, ..BUDGET },
            PalwTokenizerRenderingBudgetV2 { total_rendering_bytes: 4, ..BUDGET },
        ] {
            assert!(derive_tokenizer_rendering_v2(statement, RAW, &eog, budget).is_err());
        }
    }

    fn identity_fixture(statement: PalwTokenizerRenderingStatementV2) -> PreparedTokenizerIdentityV2 {
        use crate::palw_state_v2::PalwBondKeyV2;

        PreparedTokenizerIdentityV2 {
            semantics: statement.table_root,
            artifact_root: statement.table_root,
            graph_ir_root: statement.table_root,
            layout_digest: statement.table_root,
            eof: PalwTokenizerArtifactInventoryCertificateV1 {
                artifact_hash: statement.tokenizer_id,
                actual_bytes: statement.exact_raw_bytes,
                inventory_id: statement.table_root,
                opener: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(1), 0)),
                opened_daa: 1,
                completed_daa: 2,
            },
            declared_key_events: BUDGET.key_events,
            statement,
        }
    }

    #[test]
    fn cold_prepare_reserves_scratch_and_only_memoizes_success() {
        let (statement, table) = fixture();
        let eog = table.eog_opening().unwrap();
        let identity = || identity_fixture(statement);
        let small = MemoryBudget::new(1, [1; 4]);
        let adequate = MemoryBudget::new(32_768, [32_768; 4]);
        let mut cache = PalwTokenizerRenderingCacheV2::default();
        assert!(cache.prepare(identity(), RAW, &eog, BUDGET, &small).is_err());
        assert!(cache.prepared.is_none());
        let mut wrong_raw = RAW.to_vec();
        wrong_raw[0] = b'x';
        assert!(cache.prepare(identity(), &wrong_raw, &eog, BUDGET, &adequate).is_err());
        assert!(cache.prepared.is_none());
        assert_eq!(adequate.snapshot().reserved_bytes, 0);
        cache.prepare(identity(), RAW, &eog, BUDGET, &adequate).unwrap();
        assert_eq!(adequate.snapshot().reserved_bytes, 0);
        cache.prepare(identity(), &[], &eog, BUDGET, &small).unwrap();
        let mut replacement = identity();
        replacement.declared_key_events = 1;
        assert!(cache.prepare(replacement, RAW, &eog, BUDGET, &adequate).is_err());
        assert_eq!(adequate.snapshot().reserved_bytes, 0);
        let verified = cache.prepared.unwrap();
        assert!(verified.facts.verifies_rendering(0, &table.opening_of(0).unwrap()));
        assert!(!verified.facts.verifies_rendering(1, &table.opening_of(0).unwrap()));
        let mut wrong = table.opening_of(3).unwrap();
        assert!(verified.facts.verifies_rendering(3, &wrong));
        wrong.rendering = Some(vec![]);
        assert!(!verified.facts.verifies_rendering(3, &wrong));
    }

    #[test]
    fn tfp1_intake_is_dormant_without_all_fences_and_requires_a_parent() {
        let state = PalwChainStateV2::genesis();
        let (statement, table) = fixture();
        let eog = table.eog_opening().unwrap();
        let pins = PalwTfp1TokenizerIntakePinsV2 {
            class_id: statement.table_root,
            container_root: statement.table_root,
            semantics_id: statement.table_root,
            inventory_id: statement.table_root,
        };
        let mut cache = PalwTokenizerRenderingCacheV2::default();
        let mut extras = PalwTransitionExtrasV1 {
            class_semantics_active: true,
            tir_container_identity_active: true,
            staged_public_court: Some(crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1()),
            ..Default::default()
        };
        assert_eq!(cache.prepare_tfp1_intake_v2(&state, &extras, 0, &pins, &[], &eog, BUDGET), Ok(false));
        extras.tir_public_conviction_active = true;
        assert!(cache.prepare_tfp1_intake_v2(&state, &extras, 0, &pins, RAW, &eog, BUDGET).is_err());
        assert!(cache.prepared.is_none());
    }

    #[test]
    fn registration_bytes_pin_full_eog_and_effective_limits() {
        let (statement, table) = fixture();
        let eog = table.eog_opening().unwrap();
        let denied = MemoryBudget::new(1, [1; 4]);
        let adequate = MemoryBudget::new(32_768, [32_768; 4]);
        assert!(matches!(
            derive_with_native_scratch(&identity_fixture(statement), RAW, &eog, BUDGET, &denied),
            Err(PalwVerifiedTokenizerRegistrationErrorV1::Capacity(_))
        ));
        let build = |limits| {
            let identity = identity_fixture(statement);
            let facts = derive_with_native_scratch(&identity, RAW, &eog, limits, &adequate).unwrap();
            PalwVerifiedTokenizerRegistrationV1 {
                version: VERIFIED_TOKENIZER_REGISTRATION_VERSION_V1,
                pins: PalwTfp1TokenizerIntakePinsV2 {
                    class_id: statement.table_root,
                    container_root: statement.table_root,
                    semantics_id: identity.semantics,
                    inventory_id: identity.eof.inventory_id,
                },
                identity,
                container: PalwTirContainerRowV1 {
                    container_root: statement.table_root,
                    manifest_digest: statement.tokenizer_id,
                    weights_section: 0,
                    inner_leaf_count: 1,
                    bound_daa: 1,
                },
                limits,
                facts,
            }
        };
        let record = build(BUDGET);
        assert_eq!(record.commitment_v1(), build(BUDGET).commitment_v1());
        assert_eq!(record.facts_v1().eog_leaf, eog.opening.leaf_hash);
        assert_eq!(record.facts_v1().eog_count, 1);
        assert_ne!(record.commitment_v1(), build(PalwTokenizerRenderingBudgetV2 { eog_ids: 3, ..BUDGET }).commitment_v1());
        let mut altered = build(BUDGET);
        altered.facts.eog_leaf = statement.tokenizer_id;
        assert_ne!(record.commitment_v1(), altered.commitment_v1());
        assert_eq!(borsh::to_vec(&record).unwrap().len(), 1298);
        assert_eq!(
            record.commitment_v1().to_string(),
            "f20fc0ff7018849e3eeff3932bb3b6db91e2b617396e1908615621a22affcbcb09a6ddf3b2a125b1cb1a2013715166f5e381d934225588325f600e5b5b3669a3"
        );
        let semantics = PalwClassSemanticsV1 {
            version: crate::palw_class_semantics_v1::PALW_CLASS_SEMANTICS_VERSION_V1,
            class_id: record.pins.class_id,
            artifact_root: record.identity.artifact_root,
            bound_from_daa: 1,
            model_profile_id: Hash64::default(),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(),
            tokenizer_id: statement.tokenizer_id,
            cu_ruleset_id: Hash64::default(),
            token_table_root: Some(statement.table_root),
            rendering_rule: Some(statement.rule),
            raw_tokenizer_bytes: Some(statement.exact_raw_bytes),
            tokenizer_key_events: Some(BUDGET.key_events),
            prefix_state_relation: None,
        };
        let parent = crate::palw_state_v2::PalwBlockContextV2 {
            block: crate::BlockHash::from_u64_word(1),
            daa_score: 2,
            blue_score: 2,
            subsidy: 0,
        };
        let mut persisted = PalwRootedTokenizerRegistrationRowV1 {
            version: VERIFIED_TOKENIZER_REGISTRATION_VERSION_V1,
            pins: PalwTfp1TokenizerIntakePinsV2 { semantics_id: semantics.commitment(), ..record.pins },
            semantics,
            parent,
            point: crate::palw_state_v2::PalwBlockContextV2 {
                block: crate::BlockHash::from_u64_word(2),
                daa_score: 3,
                blue_score: 3,
                subsidy: 0,
            },
            record_id: record.commitment_v1(),
            facts: record.facts.clone(),
        };
        // Structural decoding cannot manufacture native installation provenance.
        let encoded = borsh::to_vec(&persisted).unwrap();
        assert!(encoded.len() < 1536);
        let shape = crate::palw_native_resource_v1::preflight_native_value_v1::<PalwRootedTokenizerRegistrationRowV1>(
            encoded.as_slice(),
            encoded.len() as u64,
        )
        .unwrap();
        assert_eq!(shape.canonical_bytes_v1(), encoded.len() as u64);
        assert_eq!(borsh::from_slice::<PalwRootedTokenizerRegistrationRowV1>(&encoded).unwrap(), persisted);
        assert!(persisted.check(&persisted.key()).is_ok());
        assert!(persisted.check_context(&PalwChainStateV2::genesis()).is_err());
        assert!(borsh::from_slice::<PalwRootedTokenizerRegistrationRowV1>(&encoded[..encoded.len() - 1]).is_err());
        persisted.facts.eog_count = ROOTED_TOKENIZER_LIMITS_V1.eog_ids + 1;
        assert!(persisted.check(&persisted.key()).is_err());
        assert_eq!(adequate.snapshot().reserved_bytes, 0);
    }
}

// Fixed limits for the private native adapter. They are part of the record
// identity and may only change under a new registration ruleset/version.
pub(crate) const ROOTED_TOKENIZER_LIMITS_V1: PalwTokenizerRenderingBudgetV2 = PalwTokenizerRenderingBudgetV2 {
    raw_bytes: 8 * 1024 * 1024,
    vocab: 262_144,
    key_events: 4 * 1024 * 1024,
    eog_ids: 4096,
    total_rendering_bytes: 8 * 1024 * 1024,
};

/// Persisted compact native row. Decoding it is structural only. The sole
/// fold writer starts from raw operands, and replay/load must start from a
/// validated journal or an authenticated chain-state root.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwRootedTokenizerRegistrationRowV1 {
    version: u16,
    pins: PalwTfp1TokenizerIntakePinsV2,
    semantics: PalwClassSemanticsV1,
    parent: crate::palw_state_v2::PalwBlockContextV2,
    point: crate::palw_state_v2::PalwBlockContextV2,
    record_id: Hash64,
    facts: PalwDerivedTokenizerRenderingV2,
}

impl PalwRootedTokenizerRegistrationRowV1 {
    pub(crate) fn derive(
        parent: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        point: crate::palw_state_v2::PalwBlockContextV2,
        pins: PalwTfp1TokenizerIntakePinsV2,
        raw: &[u8],
        eog: &PalwTokenTableEogOpeningV1,
    ) -> Result<Self, PalwVerifiedTokenizerRegistrationErrorV1> {
        use PalwVerifiedTokenizerRegistrationErrorV1::Parent;
        let prior = *parent.last_point().ok_or(Parent("tokenizer registration lacks an accepted parent"))?;
        if prior.block == point.block || prior.blue_score >= point.blue_score || prior.daa_score > point.daa_score {
            return Err(Parent("tokenizer registration point does not follow its parent"));
        }
        let record = PalwVerifiedTokenizerRegistrationV1::derive_v1(
            parent,
            extras,
            point.daa_score,
            pins,
            raw,
            eog,
            ROOTED_TOKENIZER_LIMITS_V1,
        )?
        .ok_or(Parent("tokenizer registration prerequisites are dormant"))?;
        let semantics = parent.class_semantics_v1(&pins.class_id).ok_or(Parent("tokenizer registration lost its semantics"))?;
        Ok(Self {
            version: VERIFIED_TOKENIZER_REGISTRATION_VERSION_V1,
            pins,
            semantics: semantics.clone(),
            parent: prior,
            point,
            record_id: record.commitment_v1(),
            facts: record.facts,
        })
    }

    pub(crate) fn key(&self) -> PalwStagedCourtKeyV1 {
        PalwStagedCourtKeyV1::VerifiedTokenizerRegistration(self.pins.class_id, self.pins.semantics_id)
    }

    pub fn record_id_v1(&self) -> Hash64 {
        self.record_id
    }

    pub fn facts_v1(&self) -> &PalwDerivedTokenizerRenderingV2 {
        &self.facts
    }

    pub(crate) fn check(&self, key: &PalwStagedCourtKeyV1) -> Result<(), String> {
        let f = &self.facts;
        let s = &f.statement;
        let cap = ROOTED_TOKENIZER_LIMITS_V1;
        if self.version != VERIFIED_TOKENIZER_REGISTRATION_VERSION_V1
            || self.key() != *key
            || !self.semantics.well_formed_for(&self.pins.class_id, &self.semantics.artifact_root)
            || self.semantics.commitment() != self.pins.semantics_id
            || self.parent.block == self.point.block
            || self.parent.blue_score >= self.point.blue_score
            || self.parent.daa_score > self.point.daa_score
            || self.semantics.bound_from_daa > self.parent.daa_score
            || s.tokenizer_id != self.semantics.tokenizer_id
            || Some(s.table_root) != self.semantics.token_table_root
            || Some(s.rule) != self.semantics.rendering_rule
            || Some(s.exact_raw_bytes) != self.semantics.raw_tokenizer_bytes
            || s.exact_raw_bytes == 0
            || s.exact_raw_bytes > cap.raw_bytes
            || s.vocab == 0
            || s.vocab > cap.vocab
            || self.semantics.tokenizer_key_events.is_none_or(|n| f.key_events > n)
            || f.key_events > cap.key_events
            || f.maximum_rendering_bytes > f.total_rendering_bytes
            || f.total_rendering_bytes > cap.total_rendering_bytes
            || f.eog_count == 0
            || f.eog_count > cap.eog_ids
            || f.eog_count > s.vocab
            || f.lowest_eog_id >= s.vocab
            || f.eog_leaf == Hash64::default()
            || self.record_id == Hash64::default()
        {
            return Err("invalid fixed verified tokenizer registration row".into());
        }
        Ok(())
    }

    fn check_registration(&self, state: &PalwChainStateV2) -> Result<(), String> {
        self.check(&self.key())?;
        let class = state.class(&self.pins.class_id).ok_or("verified tokenizer class absent")?;
        let current = state.class_semantics_v1(&self.pins.class_id).ok_or("verified tokenizer semantics absent")?;
        let mut compatible = self.semantics.clone();
        // Native semantics may fill the previously empty, unrelated prefix
        // relation. Old rows remain historical and the current lookup below
        // uses only the exact current semantics key.
        if compatible.prefix_state_relation.is_none() {
            compatible.prefix_state_relation = current.prefix_state_relation;
        }
        if &compatible != current {
            return Err("verified tokenizer semantics changed on this branch".into());
        }
        let identity = PreparedTokenizerIdentityV2::resolve(
            state,
            &self.pins.class_id,
            &class.artifact_root,
            &self.semantics,
            self.point.daa_score,
        )
        .ok_or("verified tokenizer class or EOF context absent")?;
        let container = palw_tir_bound_container_v1(state, self.pins.class_id).ok_or("verified tokenizer PALWART3 absent")?;
        if identity.eof.completed_daa > self.parent.daa_score
            || identity.eof.inventory_id != self.pins.inventory_id
            || container.bound_daa > self.parent.daa_score
            || container.container_root != self.pins.container_root
            || identity.statement != self.facts.statement
        {
            return Err("verified tokenizer signed EOF, table or PALWART3 changed on this branch".into());
        }
        // This checks persisted commitment consistency, not parser provenance.
        let encoded_fields = (self.version, self.pins, identity, container, ROOTED_TOKENIZER_LIMITS_V1, &self.facts);
        if verified_tokenizer_commitment_v1(&encoded_fields) != self.record_id {
            return Err("verified tokenizer registration commitment mismatch".into());
        }
        Ok(())
    }

    pub(crate) fn check_adoption(
        &self,
        state: &PalwChainStateV2,
        point: &crate::palw_state_v2::PalwBlockContextV2,
    ) -> Result<(), String> {
        if &self.point != point
            || state.last_point() != Some(&self.parent)
            || state.class_semantics_v1(&self.pins.class_id) != Some(&self.semantics)
        {
            return Err("verified tokenizer journal changes the exact accepting block or parent".into());
        }
        self.check_registration(state)
    }

    pub(crate) fn check_child(
        &self,
        state: &PalwChainStateV2,
        point: &crate::palw_state_v2::PalwBlockContextV2,
    ) -> Result<(), String> {
        if &self.point != point || state.last_point() != Some(point) {
            return Err("verified tokenizer journal does not leave its exact accepting block".into());
        }
        self.check_registration(state)
    }

    pub(crate) fn check_context(&self, state: &PalwChainStateV2) -> Result<(), String> {
        let now = state.last_point().ok_or("verified tokenizer registration has no accepted point")?;
        if self.point.daa_score > now.daa_score || self.point.blue_score > now.blue_score {
            return Err("verified tokenizer registration lies after the snapshot".into());
        }
        self.check_registration(state)
    }
}

/// Read from an authenticated accepted native parent. A standalone decoded
/// row or an untrusted state snapshot cannot authorize reuse or a lower quote.
pub(crate) fn palw_rooted_tokenizer_registration_v1<'a>(
    state: &'a PalwChainStateV2,
    extras: &PalwTransitionExtrasV1,
    class_id: Hash64,
    candidate_daa: u64,
) -> Option<&'a PalwRootedTokenizerRegistrationRowV1> {
    if !tfp1_tokenizer_intake_active(extras) || state.last_point()?.daa_score > candidate_daa {
        return None;
    }
    let semantics = state.class_semantics_v1(&class_id)?;
    let key = PalwStagedCourtKeyV1::VerifiedTokenizerRegistration(class_id, semantics.commitment());
    let PalwStagedCourtRowV1::VerifiedTokenizerRegistration(row) = state.staged_court_row_v1(&key)? else { return None };
    (row.point.daa_score < candidate_daa && row.key() == key && row.check_context(state).is_ok()).then_some(row.as_ref())
}

/// Claim-local operand bounds after reuse of a verified registration. These
/// sizes exclude the carrier, authenticated ID source and terminal execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwRootedTokenizerOpeningBoundsV1 {
    pub registration_id: Hash64,
    pub maximum_rendering_opening_bytes: u64,
    pub eog_opening_bytes: u64,
}

impl PalwRootedTokenizerOpeningBoundsV1 {
    fn from_facts(registration_id: Hash64, facts: &PalwDerivedTokenizerRenderingV2) -> Self {
        let vocab = u64::from(facts.statement.vocab);
        let leaves = vocab + 1;
        let rendering_siblings = crate::palw_step_leg::step_leg_max_opening_siblings_v1(leaves) as u64;
        let eog_siblings = crate::palw_step_leg::step_range_sibling_count_v1(leaves, vocab, 1);
        // PalwStepOpeningV1: u64 index, 64-byte hash, Vec length and sibling hashes.
        let step_opening_bytes = |siblings| 8 + 64 + 4 + 64 * siblings;
        Self {
            registration_id,
            // ID, Some discriminant, Vec length, rendering bytes and opening.
            maximum_rendering_opening_bytes: 4 + 1 + 4 + facts.maximum_rendering_bytes + step_opening_bytes(rendering_siblings),
            eog_opening_bytes: 4 + 4 * u64::from(facts.eog_count) + step_opening_bytes(eog_siblings),
        }
    }
}

pub(crate) fn palw_rooted_tokenizer_opening_bounds_v1(
    state: &PalwChainStateV2,
    extras: &PalwTransitionExtrasV1,
    candidate_daa: u64,
    original: &PalwFreePromptCommitmentV3,
    header: &PalwPublicTirBindingHeaderV1,
) -> Option<PalwRootedTokenizerOpeningBoundsV1> {
    let class = original.job.class_id;
    let row = palw_rooted_tokenizer_registration_v1(state, extras, class, candidate_daa)?;
    let record = state.tir_class_v1(&class)?;
    let facts = row.facts_v1();
    if !row.semantics.well_formed_for(&class, &header.artifact_root)
        || !row.semantics.matches_context(&header.job_context)
        || original.job.tokenizer_id != facts.statement.tokenizer_id
        || original.execution_root != header.committed_execution_root
        || original.decode_tokens_executed != header.job_context.exact_decode_tokens
        || record.logits_vocab != facts.statement.vocab
        || palw_fp_constraint_table_root_v1(&original.job).is_some_and(|root| root != facts.statement.table_root)
    {
        return None;
    }
    Some(PalwRootedTokenizerOpeningBoundsV1::from_facts(row.record_id_v1(), facts))
}

/// Borrowed facts for one accepted original and its source-authenticated IDs.
/// This grants no verdict, claim eligibility, credit or material availability.
pub(crate) struct PalwRootedClaimTokenizerV1<'a> {
    row: &'a PalwRootedTokenizerRegistrationRowV1,
    ids: &'a PalwTirPublicAuthenticatedIdsV1,
}

impl<'a> PalwRootedClaimTokenizerV1<'a> {
    /// Check against an ID supplied by the native ordered court cursor.
    pub(crate) fn verifies_rendering(&self, expected_id: u32, opening: &PalwTokenTableOpeningV1) -> bool {
        self.row.facts_v1().verifies_rendering(expected_id, opening)
    }

    /// Resolve the ID from the original authenticated sequence. The outer
    /// `None` means invalid or out of range, distinct from an absent leaf.
    pub(crate) fn committed_rendering_at<'material>(
        &self,
        position: u32,
        opening: &'material PalwTokenTableOpeningV1,
    ) -> Option<Option<&'material [u8]>> {
        let id = *self.ids.ids_v1().get(position as usize)?;
        self.verifies_rendering(id, opening).then_some(opening.rendering.as_deref())
    }

    /// Constraint verification needs every authenticated EOG member. The
    /// lowest ID by itself is only a fallback, never a membership answer.
    pub(crate) fn verified_eog<'material>(&self, opening: &'material PalwTokenTableEogOpeningV1) -> Option<&'material [u32]> {
        self.row.facts_v1().verified_eog(opening)
    }
}

/// Historical claims and claims without a matching registration keep the
/// ordinary route. A present prospective row with broken context fails closed.
pub(crate) fn palw_rooted_claim_tokenizer_v1<'a>(
    state: &'a PalwChainStateV2,
    claim_id: Hash64,
    source: &PalwTirPublicSourceV1,
    ids: &'a PalwTirPublicAuthenticatedIdsV1,
) -> Result<Option<PalwRootedClaimTokenizerV1<'a>>, String> {
    let Some(claim) = state.claim(&claim_id) else { return Ok(None) };
    let Some(semantics) = state.claim_semantics_v1(&claim_id) else { return Ok(None) };
    let key = PalwStagedCourtKeyV1::VerifiedTokenizerRegistration(claim.class_id, semantics.commitment());
    let Some(staged) = state.staged_court_row_v1(&key) else { return Ok(None) };
    let PalwStagedCourtRowV1::VerifiedTokenizerRegistration(row) = staged else {
        return Err("rooted claim tokenizer key has another row type".into());
    };
    // The record clock cannot order writes within one DAA. Only strictly
    // later claims can depend on this registration.
    if row.point.daa_score >= claim.accepted_daa || row.point.blue_score >= claim.accepted_blue_score {
        return Ok(None);
    }
    let accepted = state.last_point().ok_or("rooted claim tokenizer has no accepted snapshot")?;
    if row.key() != key || claim.accepted_daa > accepted.daa_score || claim.accepted_blue_score > accepted.blue_score {
        return Err("rooted claim tokenizer record or acceptance point changed".into());
    }
    row.check_context(state)?;
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim_id)
        .ok_or("rooted claim tokenizer original frame absent")?;
    let original = frame.original_v1();
    let header = frame.header_v1();
    let plan = frame.plan_v1();
    let descriptor = frame.descriptor_v1();
    let record = state.tir_class_v1(&claim.class_id).ok_or("rooted claim tokenizer TIR class absent")?;
    let facts = row.facts_v1();
    if &row.semantics != semantics
        || plan.original_claim != claim_id
        || plan.terms.accepted_daa != claim.accepted_daa
        || plan.source_id != source.source_id_v1()
        || descriptor.source_id != source.source_id_v1()
        || descriptor.class_id != claim.class_id
        || descriptor.layout_digest != record.layout_digest
        || header != source.header_v1()
        || header.artifact_root != semantics.artifact_root
        || !semantics.matches_context(&header.job_context)
        || original.job.class_id != claim.class_id
        || original.job.tokenizer_id != facts.statement.tokenizer_id
        || original.execution_root != claim.execution_root
        || crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&header.job_context) != claim.job_identity
        || original.decode_tokens_executed != source.decode_count_v1()
        || original.trace_root != claim.trace_root
        || original.output_root != claim.output_root
        || palw_fp_constraint_table_root_v1(&original.job).is_some_and(|root| root != facts.statement.table_root)
        || source.vocab_v1() != facts.statement.vocab
        || ids.source_id_v1() != source.source_id_v1()
        || ids.ids_v1().len() != source.decode_count_v1() as usize
    {
        return Err("rooted claim tokenizer changes the authenticated original/source/IDs".into());
    }
    Ok(Some(PalwRootedClaimTokenizerV1 { row, ids }))
}

include!("palw_native_resource_v1/private/palw_tokenizer_rendering_v2.rs");
