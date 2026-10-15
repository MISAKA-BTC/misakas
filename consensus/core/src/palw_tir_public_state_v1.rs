//! Dormant rooted storage adapter for paid TIR acquisition. There is deliberately
//! no Open, carrier, fee, reserve or admission entry here. Existing ClaimBundle
//! funding is NOT reinterpreted as payment for this additional source plan.
//!
//! Before activation a versioned native quote must fund the complete SUM and
//! establish separate exposure/quota, source holds, material TTL, due indexes and
//! recovery. These rows must not enter the legacy Owner accounting/sweep path.
//! Generic delta210 can carry the returned bounded changes; delta211 cannot.
use crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1;
use crate::palw_staged_court_state_v1::{PalwSharedCourtV1, PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use crate::palw_tir_public_owned_v1::{
    PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1, PalwTirPublicAcceptedBodyRefV1, PalwTirPublicChunkRecordV1, PalwTirPublicContractV1,
    PalwTirPublicOwnedActionV1, PalwTirPublicOwnedEffectV1, PalwTirPublicOwnedV1,
};
use crate::palw_tir_public_source_v1::{PalwTirPublicAuthenticatedIdsV1, PalwTirPublicSourceV1};
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeMap;

fn hash<T: BorshSerialize>(domain: &[u8], value: &T) -> Result<Hash64, String> {
    let bytes = borsh::to_vec(value).map_err(|e| e.to_string())?;
    Ok(Hash64::from_bytes(blake2b_simd::Params::new().hash_length(64).key(domain).hash(&bytes).as_bytes().try_into().expect("64")))
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicPreparationRowV1 {
    pub descriptor: PalwTirPublicSourceDescriptorV1,
    pub allocation_id: Hash64,
    pub preparation: PalwSharedCourtV1<crate::palw_tir_public_owned_v1::PalwTirPublicPreparationV1>,
}
impl PalwTirPublicPreparationRowV1 {
    pub fn key_v1(&self) -> Key {
        Key::TirPreparation(self.preparation.contract_v1().claim, self.allocation_id)
    }
    pub fn check_structure_v1(&self, key: &Key) -> Result<(), String> {
        self.preparation.check_v1()?;
        let c = self.preparation.contract_v1();
        if *key != self.key_v1()
            || self.allocation_id != c.allocation_id
            || self.descriptor.source_id != c.source_id
            || self.descriptor.maximum_leaves == 0
            || self.descriptor.native_custodian_upper != c.plan.maximum_custodians
            || self.descriptor.work_rules != Some(self.preparation.work_v1().allocation.rules)
        {
            return Err("TIR preparation row changes original allocation/source/native rules".into());
        }
        Ok(())
    }
}

/// Authenticated ORIGINAL context reconstruction only. Native new original-full-
/// allocation/grant admission remains separate. The retained old ClaimBundle
/// is a context fallback, never a substitute raw-custody pledge or new payment.
pub fn palw_tir_public_source_for_preparation_row_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    row: &PalwTirPublicPreparationRowV1,
    maximum_leaves: u64,
) -> Result<PalwTirPublicSourceV1, String> {
    row.check_structure_v1(&row.key_v1())?;
    let c = row.preparation.contract_v1();
    if let Some(frame) = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, c.claim) {
        let plan = frame.plan_v1();
        let custody = frame.custody_v1();
        if row.key_v1() != frame.preparation_key_v1()
            || row.descriptor != *frame.descriptor_v1()
            || c.original_accepted_daa != plan.terms.accepted_daa
            || c.preparation_until_daa != plan.preparation_until_daa
            || c.collect_until_daa != plan.terms.collect_until_daa
            || c.material_until_daa != plan.terms.material_until_daa
            || c.producer != custody.pledge_v1().producer
            || &c.drawn_panel != custody.drawn_panel_v1()
            || !c.custodians.is_subset(&custody.liable_signers_v1())
            || c.plan != plan.preparation
            || row.preparation.work_v1().allocation != plan.preparation_work
        {
            return Err("TIR preparation substitutes original full allocation or qualified frozen source scope".into());
        }
        let source = source_for_frame(state, frame, maximum_leaves)?;
        row.preparation.check_source_v1(&source, &plan.acquisition)?;
        return Ok(source);
    }
    // Old dormant rows may still reconstruct their original context. This does
    // not fund a new full allocation, create a raw grant or authorize admission.
    let bundle = state.staged_bundle_contract_v1(c.claim).ok_or("TIR preparation original full frame/context is not connected")?;
    let crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Tir(header) = &bundle.original_binding else {
        return Err("TIR preparation cannot reinterpret a legacy original binding".into());
    };
    let original = state.staged_original_job_v1(c.claim)?.ok_or("TIR preparation original public commitment is missing")?;
    let record = state.tir_class_v1(&header.job_context.shape_profile_id).ok_or("TIR preparation registered class is missing")?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, record.facts.class_id)
        .ok_or("TIR preparation registered layout is missing")?;
    let d = &row.descriptor;
    if c.original_accepted_daa != bundle.claim.terms.accepted_daa
        || c.producer != bundle.claim.producer
        || c.drawn_panel != bundle.claim.panel
        || c.material_until_daa > bundle.claim.terms.material_until_daa
        || crate::palw_freeprompt_v3::fp_claim_id_v3(&original) != c.claim
        || d.class_id != record.facts.class_id
        || d.layout_digest != record.layout_digest
        || d.header_digest != hash(b"misaka-palw/tir-source-header/v1", header)?
        || d.maximum_leaves != maximum_leaves
        || d.native_config != bundle.config
        || d.native_custodian_upper != bundle.history_custody.maximum_custodians
        || d.work_rules.is_none_or(|rules| rules.max_step_leaf_count != maximum_leaves)
        || crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&header.job_context)
            != bundle.history_custody.pledge.job_identity
    {
        return Err("TIR preparation substitutes original accepted clock/class/header/source context".into());
    }
    let source = PalwTirPublicSourceV1::new_v1(record, layout, header, original.execution_root, original.trace_root, maximum_leaves)?;
    let acquisition =
        crate::palw_tir_public_owned_v1::PalwTirPublicOwnedPlanV1::derive_v1(&source, d.native_custodian_upper, d.native_config)?;
    let allocation = crate::palw_tir_public_owned_v1::derive_tir_public_work_allocation_v1(
        record,
        layout,
        header,
        &source,
        &acquisition,
        d.work_rules.ok_or("TIR preparation has no frozen native work rules")?,
    )?;
    if source.source_id_v1() != d.source_id || allocation != row.preparation.work_v1().allocation {
        return Err("TIR preparation work differs from original registered native quote".into());
    }
    row.preparation.check_source_v1(&source, &acquisition)?;
    Ok(source)
}

pub struct PalwTirPublicPreparationSnapshotV1 {
    pub snapshot_block: crate::BlockHash,
    pub now_daa: u64,
    pub row: PalwSharedCourtV1<PalwTirPublicPreparationRowV1>,
    pub source: PalwTirPublicSourceV1,
}
pub fn palw_tir_public_preparation_snapshot_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: Key,
    maximum_leaves: u64,
) -> Result<Option<PalwTirPublicPreparationSnapshotV1>, String> {
    let point = state.last_point().ok_or("TIR preparation has no accepted original clock")?;
    let Some(Row::TirPreparation(row)) = state.staged_court_row_v1(&key) else { return Ok(None) };
    row.check_structure_v1(&key)?;
    let c = row.preparation.contract_v1();
    if point.daa_score < c.original_accepted_daa || point.daa_score >= c.material_until_daa {
        return Err("TIR preparation snapshot is outside original material window".into());
    }
    let source = palw_tir_public_source_for_preparation_row_v1(state, row, maximum_leaves)?;
    Ok(Some(PalwTirPublicPreparationSnapshotV1 { snapshot_block: point.block, now_daa: point.daa_score, row: row.clone(), source }))
}
pub fn palw_tir_public_preparation_transition_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: Key,
    expected_source: Hash64,
    expected_turn: u64,
    signer: PalwBondKeyV2,
    now: u64,
    action: &PalwTirPublicOwnedActionV1,
    maximum_leaves: u64,
) -> Result<PalwTirPublicTransitionV1, String> {
    let Some(Row::TirPreparation(old)) = state.staged_court_row_v1(&key) else {
        return Err("TIR preparation has no original funded row".into());
    };
    let source = palw_tir_public_source_for_preparation_row_v1(state, old, maximum_leaves)?;
    if expected_source != source.source_id_v1() || expected_turn != 0 {
        return Err("TIR preparation changes original source/turn domain".into());
    }
    let mut next = old.clone();
    let effect = match action {
        PalwTirPublicOwnedActionV1::WorkDebit(debit) => {
            PalwTirPublicOwnedEffectV1::WorkDebited(next.preparation.capture_debit_v1(&source, signer, now, debit)?)
        }
        PalwTirPublicOwnedActionV1::PreparationReady { descriptor } => {
            next.preparation.mark_ready_v1(&source, signer, now, *descriptor)?;
            PalwTirPublicOwnedEffectV1::PreparationReady(signer)
        }
        PalwTirPublicOwnedActionV1::PreparationCollect { actor } => {
            PalwTirPublicOwnedEffectV1::PreparationDefault(next.preparation.collect_v1(signer, *actor, now)?)
        }
        _ => return Err("TIR preparation root cannot accept owner DA moves".into()),
    };
    next.check_structure_v1(&key)?;
    Ok(PalwTirPublicTransitionV1 {
        changes: vec![PalwTirPublicRowChangeV1 {
            key,
            old: Some(Row::TirPreparation(old.clone())),
            new: Some(Row::TirPreparation(next)),
        }],
        effect,
    })
}

/// References to immutable native material, never a deserializable admission
/// marker. No program, profile, leaf preimage or decoded private unit is copied.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicSourceDescriptorV1 {
    pub class_id: Hash64,
    pub layout_digest: Hash64,
    pub header_digest: Hash64,
    pub source_id: Hash64,
    pub maximum_leaves: u64,
    pub native_config: PalwStagedPublicCourtConfigV1,
    pub native_custodian_upper: u64,
    pub work_rules: Option<crate::palw_tir_public_owned_v1::PalwTirPublicWorkRulesV1>,
}
impl PalwTirPublicSourceDescriptorV1 {
    pub fn allocation_id_v1(&self, contract: &PalwTirPublicContractV1) -> Result<Hash64, String> {
        hash(b"misaka-palw/tir-acquisition-allocation/v1", &(self, contract))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicAcquisitionRowV1 {
    pub descriptor: PalwTirPublicSourceDescriptorV1,
    pub owned: PalwSharedCourtV1<PalwTirPublicOwnedV1>,
}
impl PalwTirPublicAcquisitionRowV1 {
    pub fn key_v1(&self) -> Result<Key, String> {
        let c = self.owned.contract_v1();
        Ok(Key::TirAcquisition(c.claim, c.owner, self.descriptor.allocation_id_v1(c)?))
    }
    /// Local storage consistency only. Full source/config/upper verification is
    /// state-aware and must run after all rows of an imported snapshot are read.
    pub fn check_structure_v1(&self, key: &Key) -> Result<(), String> {
        let c = self.owned.contract_v1();
        if self.key_v1()? != *key
            || c.claim == Hash64::default()
            || c.bundle_id == Hash64::default()
            || self.descriptor.source_id == Hash64::default()
            || c.source_id != self.descriptor.source_id
            || c.plan.source_id != c.source_id
            || c.plan.config != self.descriptor.native_config
            || c.plan.maximum_custodians != self.descriptor.native_custodian_upper
            || self.descriptor.maximum_leaves == 0
            || self.descriptor.work_rules != self.owned.work_v1().map(|w| w.allocation.rules)
            || c.reserved_daa < c.terms.accepted_daa
            || c.reserved_daa >= c.terms.open_until_daa
            || c.terms.stream_until_daa >= c.terms.collect_until_daa
            || c.terms.collect_until_daa > c.terms.material_until_daa
            || c.terms.material_until_daa > c.terms.bond_exit_after_daa
        {
            return Err("TIR sidecar key/source/config/immutable clock mismatch".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicChunkRowV1 {
    pub acquisition: Key,
    pub source_id: Hash64,
    pub material_until_daa: u64,
    pub record: PalwTirPublicChunkRecordV1,
}
impl PalwTirPublicChunkRowV1 {
    pub fn check_structure_v1(&self, head: Hash64) -> Result<(), String> {
        if !matches!(self.acquisition, Key::TirAcquisition(..))
            || self.source_id == Hash64::default()
            || self.material_until_daa == 0
            || self.record.demand_id == Hash64::default()
            || self.record.bytes.is_empty()
            || self.record.bytes.len() > PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1
            || self.record.offset.checked_add(self.record.bytes.len() as u64).is_none()
            || self.record.head_v1()? != head
        {
            return Err("TIR immutable chunk key/scope/length mismatch".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPublicAcceptedRowV1 {
    pub acquisition: Key,
    pub source_id: Hash64,
    pub material_until_daa: u64,
    pub history_index: u64,
    pub reference: PalwTirPublicAcceptedBodyRefV1,
}
impl PalwTirPublicAcceptedRowV1 {
    pub fn check_structure_v1(&self, id: Hash64) -> Result<(), String> {
        if !matches!(self.acquisition, Key::TirAcquisition(..))
            || self.source_id == Hash64::default()
            || self.reference.source_id != self.source_id
            || self.reference.total_bytes == 0
            || self.reference.chunk_head == Hash64::default()
            || self.reference.opened_daa >= self.reference.respond_until_daa
            || self.reference.respond_until_daa >= self.material_until_daa
            || self.reference.hash_v1()? != id
        {
            return Err("TIR accepted body reference key/scope/clock mismatch".into());
        }
        Ok(())
    }
}

/// Reconstruct against the *retained accepted* context. `maximum_leaves` must be
/// resolved by a future native admission from its frozen rules, not chosen by a
/// carrier. Equality here is only a defensive cross-check; no new payment or
/// custody signatures are established by this function. In particular the
/// Panel subset check below is NOT qualified literal Valid evidence: future
/// admission must bind the exact independently verified grants. No consumer of
/// the returned collection identities is activated by this storage adapter.
pub fn palw_tir_public_source_for_row_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    row: &PalwTirPublicAcquisitionRowV1,
    maximum_leaves: u64,
) -> Result<PalwTirPublicSourceV1, String> {
    row.check_structure_v1(&row.key_v1()?)?;
    let c = row.owned.contract_v1();
    if let Some(frame) = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, c.claim) {
        let plan = frame.plan_v1();
        let custody = frame.custody_v1();
        if row.descriptor != *frame.descriptor_v1()
            || c.bundle_id != plan.allocation_id
            || c.plan != plan.acquisition
            || c.terms != plan.owner_terms_v1()?
            || c.producer != custody.pledge_v1().producer
            || custody.drawn_panel_v1().contains(&c.owner)
            || !c.custodians.is_subset(&custody.liable_signers_v1())
            || row.owned.work_v1().is_none_or(|work| work.allocation != plan.owner_work)
        {
            return Err("TIR owner substitutes original full allocation or frozen qualified source scope".into());
        }
        // Exact membership at Open is a native admission duty. Later qualified
        // grants cannot retroactively enlarge or erase this already-paid row's
        // protected slots. Membership here is never inferred from drawn Panel.
        let source = source_for_frame(state, frame, maximum_leaves)?;
        check_source(row, &source)?;
        return Ok(source);
    }
    let bundle = state.staged_bundle_contract_v1(c.claim).ok_or("TIR sidecar has no retained original ClaimBundle")?;
    let crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Tir(header) = &bundle.original_binding else {
        return Err("TIR sidecar cannot reinterpret a legacy binding".into());
    };
    let original = state.staged_original_job_v1(c.claim)?.ok_or("TIR sidecar lost retained original public commitment")?;
    let record = state.tir_class_v1(&header.job_context.shape_profile_id).ok_or("TIR sidecar registered class absent")?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, record.facts.class_id)
        .ok_or("TIR sidecar original class layout absent")?;
    let d = &row.descriptor;
    if d.class_id != record.facts.class_id
        || d.layout_digest != record.layout_digest
        || d.header_digest != hash(b"misaka-palw/tir-source-header/v1", header)?
        || d.maximum_leaves != maximum_leaves
        || d.native_config != bundle.config
        || d.native_custodian_upper != bundle.history_custody.maximum_custodians
        || c.bundle_id != bundle.plan.id_v1(c.claim)
        || c.producer != bundle.claim.producer
        || c.terms.accepted_daa != bundle.claim.terms.accepted_daa
        || c.terms.material_until_daa > bundle.claim.terms.material_until_daa
        || !c.custodians.iter().all(|bond| *bond == c.producer || bundle.claim.panel.contains(bond))
        || bundle.claim.panel.contains(&c.owner)
        || crate::palw_freeprompt_v3::fp_claim_id_v3(&original) != c.claim
        || crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&header.job_context)
            != bundle.history_custody.pledge.job_identity
    {
        return Err("TIR sidecar substitutes accepted class/layout/header/native geometry or source hold".into());
    }
    let source = PalwTirPublicSourceV1::new_v1(record, layout, header, original.execution_root, original.trace_root, maximum_leaves)?;
    check_source(row, &source)?;
    if let Some(work) = row.owned.work_v1() {
        let rules = row.descriptor.work_rules.ok_or("TIR work allocation lost frozen native rules")?;
        if rules.max_step_leaf_count != maximum_leaves
            || work.allocation
                != crate::palw_tir_public_owned_v1::derive_tir_public_work_allocation_v1(
                    record, layout, header, &source, &c.plan, rules,
                )?
        {
            return Err("TIR rooted work allocation differs from original registered full-source quote".into());
        }
    }
    Ok(source)
}

fn source_for_frame(
    state: &crate::palw_state_v2::PalwChainStateV2,
    frame: &crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1,
    maximum_leaves: u64,
) -> Result<PalwTirPublicSourceV1, String> {
    let record = state.tir_class_v1(&frame.descriptor_v1().class_id).ok_or("original TIR full frame lost registered class")?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, record.facts.class_id)
        .ok_or("original TIR full frame lost authenticated layout")?;
    frame.source_v1(record, layout, maximum_leaves)
}

fn check_source(row: &PalwTirPublicAcquisitionRowV1, source: &PalwTirPublicSourceV1) -> Result<(), String> {
    row.check_structure_v1(&row.key_v1()?)?;
    let d = &row.descriptor;
    if source.source_id_v1() != d.source_id
        || source.header_v1().job_context.shape_profile_id != d.class_id
        || hash(b"misaka-palw/tir-source-header/v1", source.header_v1())? != d.header_digest
    {
        return Err("TIR sidecar source differs from immutable descriptor".into());
    }
    row.owned.contract_v1().check_native_config_v1(source, d.native_config, d.native_custodian_upper)?;
    row.owned.check_v1(source)
}

/// A current authenticated snapshot view. The Arc shares small metadata only;
/// bulk body rows are fetched by exact rooted head, never copied by this getter.
pub struct PalwTirPublicSnapshotV1 {
    pub snapshot_block: crate::BlockHash,
    pub now_daa: u64,
    pub row: PalwSharedCourtV1<PalwTirPublicAcquisitionRowV1>,
    pub source: PalwTirPublicSourceV1,
    pub preparation: Option<PalwSharedCourtV1<PalwTirPublicPreparationRowV1>>,
}
pub fn palw_tir_public_snapshot_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: Key,
    maximum_leaves: u64,
) -> Result<Option<PalwTirPublicSnapshotV1>, String> {
    let point = state.last_point().ok_or("TIR acquisition snapshot has no accepted clock")?;
    let Some(Row::TirAcquisition(row)) = state.staged_court_row_v1(&key) else { return Ok(None) };
    if point.daa_score < row.owned.contract_v1().reserved_daa || point.daa_score >= row.owned.contract_v1().terms.material_until_daa {
        return Err("TIR acquisition snapshot is outside original material clock".into());
    }
    row.check_structure_v1(&key)?;
    let source = palw_tir_public_source_for_row_v1(state, row, maximum_leaves)?;
    let preparation = if let Some(reference) = row.owned.preparation_v1() {
        let Some(Row::TirPreparation(prep)) =
            state.staged_court_row_v1(&Key::TirPreparation(reference.claim, reference.allocation_id))
        else {
            return Err("TIR owner snapshot lost original claim-wide preparation".into());
        };
        palw_tir_public_source_for_preparation_row_v1(state, prep, maximum_leaves)?;
        row.owned.check_preparation_v1(&prep.preparation)?;
        Some(prep.clone())
    } else {
        None
    };
    Ok(Some(PalwTirPublicSnapshotV1 { snapshot_block: point.block, now_daa: point.daa_score, row: row.clone(), source, preparation }))
}

/// Single-process claim book. No Clone/Deserialize implementation: an ordinary
/// snapshot or a deserialized receipt cannot recreate a work permit. A restart
/// burns all pre-existing grants and must buy a NEW finite debit. This does not
/// make off-chain execution atomic with consensus or prove that work happened.
/// Canonical spends revert on reorg; orphaned physical work does not. Across a
/// restart that also forgets the local high-water mark, this is NOT a bound on
/// orphan work. A durable orphan-attempt journal or an explicit bounded-reorg
/// assumption remains necessary before claiming restart/resource completion.
pub struct PalwTirPublicWorkBookV1 {
    key: Key,
    actor: PalwBondKeyV2,
    baseline_nonce: u64,
    highest_prepared: Option<u64>,
    restart_required: bool,
    pending: Option<PendingWork>,
}
struct PendingWork {
    debit: crate::palw_tir_public_owned_v1::PalwTirPublicWorkDebitV1,
    actor_spent: crate::palw_tir_public_owned_v1::PalwTirPublicWorkV1,
    actor_grants: u64,
}
impl PalwTirPublicWorkBookV1 {
    fn prepare_preparation_v1(
        &mut self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        scope: crate::palw_tir_public_owned_v1::PalwTirPublicWorkScopeV1,
        work: crate::palw_tir_public_owned_v1::PalwTirPublicWorkV1,
        history: Option<crate::palw_step_leg::PalwStepOpeningV1>,
        maximum_leaves: u64,
    ) -> Result<PalwTirPublicOwnedActionV1, String> {
        if self.pending.is_some() || history.is_some() {
            return Err("TIR preparation has an outstanding debit or unrelated history input".into());
        }
        let snapshot = palw_tir_public_preparation_snapshot_v1(state, self.key, maximum_leaves)?
            .ok_or("TIR original preparation row disappeared")?;
        let paid = snapshot.row.preparation.work_v1();
        let actor = paid.actors.get(&self.actor).cloned().unwrap_or_default();
        if scope.actor != self.actor || actor.grants < self.baseline_nonce || self.highest_prepared.is_some_and(|n| actor.grants <= n)
        {
            return Err("TIR preparation book refuses an old actor nonce after replay/restart".into());
        }
        let debit = crate::palw_tir_public_owned_v1::PalwTirPublicWorkDebitV1 {
            expected_nonce: actor.grants,
            previous_head: actor.last.as_ref().map(|r| r.hash_v1()).transpose()?.unwrap_or_default(),
            scope,
            work,
            restart: self.restart_required,
            history: None,
        };
        let mut preview = snapshot.row.preparation.as_ref().clone();
        preview.capture_debit_v1(&snapshot.source, self.actor, snapshot.now_daa, &debit)?;
        self.highest_prepared = Some(actor.grants);
        self.pending = Some(PendingWork { debit: debit.clone(), actor_spent: actor.spent, actor_grants: actor.grants });
        Ok(PalwTirPublicOwnedActionV1::WorkDebit(debit))
    }
    fn confirm_preparation_v1(
        &mut self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        maximum_leaves: u64,
    ) -> Result<Option<PalwTirPublicWorkPermitV1>, String> {
        let pending = self.pending.as_ref().ok_or("TIR preparation has no fresh process-local debit intent")?;
        let snapshot = palw_tir_public_preparation_snapshot_v1(state, self.key, maximum_leaves)?
            .ok_or("TIR original preparation row disappeared")?;
        let c = snapshot.row.preparation.contract_v1();
        if snapshot.now_daa >= c.preparation_until_daa
            || snapshot.row.preparation.ready_v1(&self.actor).is_some()
            || snapshot.row.preparation.defaulted_v1(&self.actor)
        {
            return Err("TIR preparation permit missed its original Ready deadline or duty already ended".into());
        }
        let work = snapshot.row.preparation.work_v1();
        let Some(actor) = work.actors.get(&self.actor) else { return Ok(None) };
        if actor.grants <= pending.debit.expected_nonce {
            return Ok(None);
        }
        let receipt = actor.last.as_ref().ok_or("TIR preparation paid lane has no native receipt")?;
        if receipt.debit != pending.debit
            || actor.grants != pending.actor_grants.checked_add(1).ok_or("TIR prep actor nonce overflow")?
            || actor.spent != pending.actor_spent.checked_add_v1(pending.debit.work)?
            || !work.spent.covers_v1(&receipt.cumulative)
        {
            return Err("TIR preparation confirmation changes native actor receipt/spent".into());
        }
        let permit = PalwTirPublicWorkPermitV1 {
            scope: pending.debit.scope,
            work: pending.debit.work,
            receipt_id: receipt.hash_v1()?,
            snapshot_block: snapshot.snapshot_block,
            preparation: true,
        };
        self.pending = None;
        self.restart_required = false;
        Ok(Some(permit))
    }
}

/// Minted only by confirming this process's fresh debit against current native
/// state. Receipt/host-ticket/unsigned-DTO deserialization cannot construct it.
pub struct PalwTirPublicWorkPermitV1 {
    scope: crate::palw_tir_public_owned_v1::PalwTirPublicWorkScopeV1,
    work: crate::palw_tir_public_owned_v1::PalwTirPublicWorkV1,
    receipt_id: Hash64,
    snapshot_block: crate::BlockHash,
    preparation: bool,
}
impl PalwTirPublicWorkPermitV1 {
    pub fn is_preparation_v1(&self) -> bool {
        self.preparation
    }
    pub fn scope_v1(&self) -> &crate::palw_tir_public_owned_v1::PalwTirPublicWorkScopeV1 {
        &self.scope
    }
    pub fn work_v1(&self) -> crate::palw_tir_public_owned_v1::PalwTirPublicWorkV1 {
        self.work
    }
    pub fn receipt_id_v1(&self) -> Hash64 {
        self.receipt_id
    }
    pub fn snapshot_block_v1(&self) -> crate::BlockHash {
        self.snapshot_block
    }
}

pub fn palw_tir_public_work_book_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: Key,
    actor: PalwBondKeyV2,
    maximum_leaves: u64,
) -> Result<PalwTirPublicWorkBookV1, String> {
    if matches!(key, Key::TirPreparation(..)) {
        let snapshot =
            palw_tir_public_preparation_snapshot_v1(state, key, maximum_leaves)?.ok_or("TIR original preparation row is absent")?;
        let c = snapshot.row.preparation.contract_v1();
        let work = snapshot.row.preparation.work_v1();
        if (actor != c.producer && !c.drawn_panel.contains(&actor))
            || snapshot.row.preparation.ready_v1(&actor).is_some()
            || snapshot.row.preparation.defaulted_v1(&actor)
            || snapshot.now_daa >= c.preparation_until_daa
        {
            return Err("TIR preparation book has no pending literal original duty".into());
        }
        return Ok(PalwTirPublicWorkBookV1 {
            key,
            actor,
            baseline_nonce: work.actors.get(&actor).map_or(0, |a| a.grants),
            highest_prepared: None,
            restart_required: work.actors.get(&actor).is_some_and(|a| a.grants != 0),
            pending: None,
        });
    }
    let snapshot = palw_tir_public_snapshot_v1(state, key, maximum_leaves)?.ok_or("TIR work book has no accepted sidecar")?;
    let c = snapshot.row.owned.contract_v1();
    if actor != c.owner && !c.custodians.contains(&actor) {
        return Err("TIR work book actor has no protected literal lane".into());
    }
    let work = snapshot.row.owned.work_v1().ok_or("TIR work book has no original full paid work allocation")?;
    Ok(PalwTirPublicWorkBookV1 {
        key,
        actor,
        baseline_nonce: work.actors.get(&actor).map_or(0, |a| a.grants),
        highest_prepared: None,
        restart_required: work.actors.get(&actor).is_some_and(|a| a.grants != 0),
        pending: None,
    })
}
impl PalwTirPublicWorkBookV1 {
    pub fn pending_action_v1(&self) -> Option<PalwTirPublicOwnedActionV1> {
        self.pending.as_ref().map(|p| PalwTirPublicOwnedActionV1::WorkDebit(p.debit.clone()))
    }

    pub fn prepare_v1(
        &mut self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        scope: crate::palw_tir_public_owned_v1::PalwTirPublicWorkScopeV1,
        work: crate::palw_tir_public_owned_v1::PalwTirPublicWorkV1,
        history: Option<crate::palw_step_leg::PalwStepOpeningV1>,
        maximum_leaves: u64,
    ) -> Result<PalwTirPublicOwnedActionV1, String> {
        if matches!(self.key, Key::TirPreparation(..)) {
            return self.prepare_preparation_v1(state, scope, work, history, maximum_leaves);
        }
        if self.pending.is_some() {
            return Err("TIR work book already has an unconfirmed debit; retry its exact action".into());
        }
        let snapshot = palw_tir_public_snapshot_v1(state, self.key, maximum_leaves)?.ok_or("TIR work sidecar disappeared")?;
        let paid = snapshot.row.owned.work_v1().ok_or("TIR work allocation disappeared")?;
        let actor = paid.actors.get(&self.actor).cloned().unwrap_or_default();
        if scope.actor != self.actor
            || actor.grants < self.baseline_nonce
            || self.highest_prepared.is_some_and(|issued| actor.grants <= issued)
        {
            return Err("TIR work book refuses an old actor/nonce after replay or rollback".into());
        }
        let debit = crate::palw_tir_public_owned_v1::PalwTirPublicWorkDebitV1 {
            expected_nonce: actor.grants,
            previous_head: actor.last.as_ref().map(|r| r.hash_v1()).transpose()?.unwrap_or_default(),
            scope,
            work,
            restart: self.restart_required,
            history,
        };
        check_work_input(&snapshot.row, &snapshot.source, &debit, &|k| state.staged_court_row_v1(k))?;
        let mut preview = snapshot.row.owned.as_ref().clone();
        preview.work_debit_v1(&snapshot.source, self.actor, snapshot.now_daa, &debit)?;
        self.highest_prepared = Some(actor.grants);
        self.pending = Some(PendingWork { debit: debit.clone(), actor_spent: actor.spent, actor_grants: actor.grants });
        Ok(PalwTirPublicOwnedActionV1::WorkDebit(debit))
    }

    /// None means the exact debit is not yet accepted. No permit is minted by
    /// preparing/signing/submitting a request or by trusting a returned receipt.
    pub fn confirm_v1(
        &mut self,
        state: &crate::palw_state_v2::PalwChainStateV2,
        maximum_leaves: u64,
    ) -> Result<Option<PalwTirPublicWorkPermitV1>, String> {
        if matches!(self.key, Key::TirPreparation(..)) {
            return self.confirm_preparation_v1(state, maximum_leaves);
        }
        let pending = self.pending.as_ref().ok_or("TIR work book has no fresh process-local debit intent")?;
        let snapshot = palw_tir_public_snapshot_v1(state, self.key, maximum_leaves)?.ok_or("TIR work sidecar disappeared")?;
        let owned = &snapshot.row.owned;
        let work = owned.work_v1().ok_or("TIR work allocation disappeared")?;
        let Some(actor) = work.actors.get(&self.actor) else { return Ok(None) };
        if actor.grants <= pending.debit.expected_nonce {
            return Ok(None);
        }
        let receipt = actor.last.as_ref().ok_or("TIR accepted actor work has no rooted receipt")?;
        if receipt.debit != pending.debit
            || actor.grants != pending.actor_grants.checked_add(1).ok_or("TIR actor grant overflow")?
            || actor.spent != pending.actor_spent.checked_add_v1(pending.debit.work)?
            || !work.spent.covers_v1(&receipt.cumulative)
            || snapshot.now_daa >= owned.contract_v1().terms.stream_until_daa
            || pending.debit.scope.turn != owned.active_v1().map_or(owned.spent_v1().turns, |p| p.turn)
        {
            return Err("TIR work confirmation changes current accepted debit/actor/spent/original turn/clock".into());
        }
        check_work_input(&snapshot.row, &snapshot.source, &pending.debit, &|k| state.staged_court_row_v1(k))?;
        let permit = PalwTirPublicWorkPermitV1 {
            scope: pending.debit.scope,
            work: pending.debit.work,
            receipt_id: receipt.hash_v1()?,
            snapshot_block: snapshot.snapshot_block,
            preparation: false,
        };
        self.pending = None;
        self.restart_required = false;
        Ok(Some(permit))
    }
}

fn material_scope<'a>(
    state: &'a crate::palw_state_v2::PalwChainStateV2,
    key: Key,
    source_id: Hash64,
    until: u64,
) -> Result<&'a PalwTirPublicAcquisitionRowV1, String> {
    let Some(Row::TirAcquisition(owner)) = state.staged_court_row_v1(&key) else {
        return Err("TIR material lost its original acquisition metadata".into());
    };
    owner.check_structure_v1(&key)?;
    let c = owner.owned.contract_v1();
    let now = state.last_point().ok_or("TIR material has no accepted snapshot")?.daa_score;
    if source_id != c.source_id || until != c.terms.material_until_daa || now < c.reserved_daa || now >= until {
        return Err("TIR material substitutes original scope or retention clock".into());
    }
    Ok(owner)
}

/// Borrowed exact-head public material. This does not turn an arbitrary chunk
/// into an answer: Finish still authenticates the entire original source unit.
pub fn palw_tir_public_chunk_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    head: Hash64,
) -> Result<Option<&PalwTirPublicChunkRowV1>, String> {
    let Some(Row::TirRawChunk(row)) = state.staged_court_row_v1(&Key::TirRawChunk(head)) else { return Ok(None) };
    row.check_structure_v1(head)?;
    material_scope(state, row.acquisition, row.source_id, row.material_until_daa)?;
    Ok(Some(row))
}

/// Current finished reference, or a historical reference with a CURRENT rooted
/// history proof. A deserialized accepted-ref row alone is not an authority.
pub fn palw_tir_public_accepted_body_v1<'a>(
    state: &'a crate::palw_state_v2::PalwChainStateV2,
    id: Hash64,
    proof: Option<&crate::palw_step_leg::PalwStepOpeningV1>,
    maximum_leaves: u64,
) -> Result<Option<&'a PalwTirPublicAcceptedRowV1>, String> {
    let Some(Row::TirAcceptedBody(row)) = state.staged_court_row_v1(&Key::TirAcceptedBody(id)) else { return Ok(None) };
    row.check_structure_v1(id)?;
    let owner = material_scope(state, row.acquisition, row.source_id, row.material_until_daa)?;
    let source = palw_tir_public_source_for_row_v1(state, owner, maximum_leaves)?;
    check_reference(owner, &source, row, proof)?;
    Ok(Some(row))
}

fn check_reference(
    owner: &PalwTirPublicAcquisitionRowV1,
    source: &PalwTirPublicSourceV1,
    row: &PalwTirPublicAcceptedRowV1,
    proof: Option<&crate::palw_step_leg::PalwStepOpeningV1>,
) -> Result<(), String> {
    let c = owner.owned.contract_v1();
    let r = &row.reference;
    let id = r.hash_v1()?;
    row.check_structure_v1(id)?;
    let (count, root) = owner.owned.history_v1();
    if row.acquisition != owner.key_v1()?
        || row.source_id != c.source_id
        || row.material_until_daa != c.terms.material_until_daa
        || row.history_index >= count
        || r.turn >= owner.owned.spent_v1().turns
        || (r.answerer != c.owner && !c.custodians.contains(&r.answerer))
        || source.demand_id_v1(
            &crate::palw_tir_public_source_v1::PalwTirPublicDemandScopeV1 {
                claim: c.claim,
                owner: c.owner,
                reserved_daa: c.reserved_daa,
                bundle_id: c.bundle_id,
                turn: r.turn,
                opened_daa: r.opened_daa,
                deadline_daa: r.respond_until_daa,
            },
            &r.request,
        )? != r.demand_id
        || r.total_bytes > crate::palw_tir_public_owned_v1::request_body_bound_v1(source, r.request)?.0
    {
        return Err("TIR accepted reference changes original authenticated demand scope".into());
    }
    if owner.owned.slot_v1(&r.answerer).is_some_and(|slot| slot.finished.as_ref() == Some(r)) {
        return Ok(());
    }
    let proof = proof.ok_or("historical TIR reference requires current history membership")?;
    if proof.leaf_index != row.history_index
        || proof.leaf_hash != id
        || proof.siblings.len() > 64
        || Some(crate::palw_step_leg::step_opening_root_capped_v1(count, proof, u64::MAX).map_err(|e| e.to_string())?) != root
    {
        return Err("TIR accepted reference does not belong to current rooted history".into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirPublicRowChangeV1 {
    pub key: Key,
    pub old: Option<Row>,
    pub new: Option<Row>,
}
pub struct PalwTirPublicTransitionV1 {
    changes: Vec<PalwTirPublicRowChangeV1>,
    pub effect: PalwTirPublicOwnedEffectV1,
}
impl PalwTirPublicTransitionV1 {
    pub fn changes_v1(&self) -> &[PalwTirPublicRowChangeV1] {
        &self.changes
    }
    pub fn deltas_v1(&self) -> Vec<crate::palw_state_v2::PalwDeltaEntryV2> {
        self.changes
            .iter()
            .map(|c| crate::palw_state_v2::PalwDeltaEntryV2::StagedCourtSeed { key: c.key, old: c.old.clone(), new: c.new.clone() })
            .collect()
    }
    /// Pure map/journal regression helper, not the live chain's index/TTL fold.
    /// Check every precondition before touching any row, also on reverse replay.
    pub fn apply_rows_v1(&self, rows: &mut BTreeMap<Key, Row>, forward: bool) -> Result<(), String> {
        for (i, change) in self.changes.iter().enumerate() {
            if self.changes[..i].iter().any(|c| c.key == change.key) {
                return Err("TIR atomic batch repeats a rooted key".into());
            }
            let (expected, next) = if forward { (&change.old, &change.new) } else { (&change.new, &change.old) };
            if rows.get(&change.key) != expected.as_ref() {
                return Err("TIR atomic batch stale expected row".into());
            }
            if let Some(next) = next {
                next.check(&change.key)?;
            }
        }
        let mut write = |c: &PalwTirPublicRowChangeV1| {
            let next = if forward { &c.new } else { &c.old };
            if let Some(row) = next {
                rows.insert(c.key, row.clone());
            } else {
                rows.remove(&c.key);
            }
        };
        if forward {
            self.changes.iter().for_each(&mut write);
        } else {
            self.changes.iter().rev().for_each(&mut write);
        }
        Ok(())
    }
}

/// Existing metadata only: no missing-row implicit admission, reserve renewal,
/// or source/clock substitution. The future signed carrier must include both
/// expected fields verbatim. Native validation MUST authenticate signer, resolve
/// maximum_leaves from frozen accepted rules, and supply `now` from the accepting
/// block's native context, NOT an incoming wire field or local wall clock. This
/// helper never manufactures accepted authority from a caller's DTO.
pub fn palw_tir_public_transition_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    key: Key,
    expected_source: Hash64,
    expected_turn: u64,
    signer: PalwBondKeyV2,
    now: u64,
    action: &PalwTirPublicOwnedActionV1,
    maximum_leaves: u64,
    ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
) -> Result<PalwTirPublicTransitionV1, String> {
    let Some(Row::TirAcquisition(row)) = state.staged_court_row_v1(&key) else {
        return Err("TIR acquisition has no preadmitted metadata row".into());
    };
    row.check_structure_v1(&key)?;
    let source = palw_tir_public_source_for_row_v1(state, row, maximum_leaves)?;
    transition(row, &source, expected_source, expected_turn, signer, now, action, |key| state.staged_court_row_v1(key), ids)
}

fn transition<'a>(
    old: &PalwSharedCourtV1<PalwTirPublicAcquisitionRowV1>,
    source: &PalwTirPublicSourceV1,
    expected_source: Hash64,
    expected_turn: u64,
    signer: PalwBondKeyV2,
    now: u64,
    action: &PalwTirPublicOwnedActionV1,
    lookup: impl Fn(&Key) -> Option<&'a Row>,
    ids: Option<&PalwTirPublicAuthenticatedIdsV1>,
) -> Result<PalwTirPublicTransitionV1, String> {
    check_source(old, source)?;
    let contract = old.owned.contract_v1();
    let turn = if let PalwTirPublicOwnedActionV1::WorkDebit(debit) = action {
        let turn = old.owned.active_v1().map_or(old.owned.spent_v1().turns, |p| p.turn);
        if debit.scope.turn != turn {
            return Err("TIR work action changes native turn".into());
        }
        turn
    } else if matches!(action, PalwTirPublicOwnedActionV1::Demand { .. } | PalwTirPublicOwnedActionV1::Reacquire { .. }) {
        old.owned.spent_v1().turns
    } else {
        old.owned.active_v1().ok_or("TIR action has no current accepted demand")?.turn
    };
    if expected_source != contract.source_id
        || expected_turn != turn
        || now < contract.reserved_daa
        || now >= contract.terms.material_until_daa
    {
        return Err("TIR signed action has stale source/turn/original clock".into());
    }
    let key = old.key_v1()?;
    if let Some(reference) = old.owned.preparation_v1() {
        let Some(Row::TirPreparation(preparation)) = lookup(&Key::TirPreparation(reference.claim, reference.allocation_id)) else {
            return Err("TIR owner lost its original rooted claim-wide preparation".into());
        };
        preparation.check_structure_v1(&preparation.key_v1())?;
        old.owned.check_preparation_v1(&preparation.preparation)?;
        let c = preparation.preparation.contract_v1();
        if c.source_id != reference.source_id
            || c.original_accepted_daa != reference.original_accepted_daa
            || c.preparation_until_daa != reference.preparation_until_daa
            || c.material_until_daa != reference.material_until_daa
        {
            return Err("TIR owner preparation reference changes original native scope/clock".into());
        }
        let response_actor = match action {
            PalwTirPublicOwnedActionV1::Begin { answerer, .. }
            | PalwTirPublicOwnedActionV1::Part { answerer, .. }
            | PalwTirPublicOwnedActionV1::Finish { answerer, .. } => Some(*answerer),
            PalwTirPublicOwnedActionV1::WorkDebit(d)
                if matches!(
                    d.scope.phase,
                    crate::palw_tir_public_owned_v1::PalwTirPublicWorkPhaseV1::Response
                        | crate::palw_tir_public_owned_v1::PalwTirPublicWorkPhaseV1::Ids
                ) =>
            {
                Some(d.scope.actor)
            }
            _ => None,
        };
        if response_actor.is_some_and(|a| a != contract.owner && preparation.preparation.ready_v1(&a).is_none()) {
            return Err("TIR response requires the literal signer's original claim-wide Ready promise".into());
        }
    }
    if let PalwTirPublicOwnedActionV1::WorkDebit(debit) = action {
        check_work_input(old, source, debit, &lookup)?;
    }
    if let PalwTirPublicOwnedActionV1::Reacquire { previous, .. } = action {
        let hash = previous.hash_v1()?;
        let Some(Row::TirAcceptedBody(accepted)) = lookup(&Key::TirAcceptedBody(hash)) else {
            return Err("TIR reacquire has no retained native accepted body reference".into());
        };
        accepted.check_structure_v1(hash)?;
        if accepted.acquisition != key
            || accepted.source_id != contract.source_id
            || accepted.material_until_daa != contract.terms.material_until_daa
            || accepted.reference != *previous
        {
            return Err("TIR reacquire substitutes another acquisition's accepted body".into());
        }
    }
    let mut records = Vec::new();
    if let PalwTirPublicOwnedActionV1::Finish { demand_id, answerer } = action {
        if signer != *answerer {
            return Err("TIR Finish signer differs from literal answer slot".into());
        }
        // Refuse stale/expired/cross-signer work before loading any raw body.
        old.owned.response_clock(signer, *demand_id, now)?;
        let slot = old.owned.slot_v1(answerer).ok_or("TIR Finish has no rooted accepted Begin")?;
        if slot.accepted_offset != slot.total_bytes || slot.finished.is_some() {
            return Err("TIR Finish is not unconsumed accepted EOF".into());
        }
        let limit = slot.total_bytes.div_ceil(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64);
        records
            .try_reserve_exact(usize::try_from(limit).map_err(|_| "TIR chunk count exceeds native usize")?)
            .map_err(|_| "TIR chunk metadata allocation failed")?;
        let mut head = slot.head;
        let mut offset = slot.total_bytes;
        for _ in 0..limit {
            let Some(Row::TirRawChunk(chunk)) = lookup(&Key::TirRawChunk(head)) else {
                return Err("TIR Finish lost an authenticated rooted chunk".into());
            };
            chunk.check_structure_v1(head)?;
            if chunk.acquisition != key
                || chunk.source_id != contract.source_id
                || chunk.material_until_daa != contract.terms.material_until_daa
                || chunk.record.offset.checked_add(chunk.record.bytes.len() as u64) != Some(offset)
            {
                return Err("TIR Finish chunk changes literal acquisition/source/retention/offset".into());
            }
            offset = chunk.record.offset;
            head = chunk.record.previous;
            records.push(chunk.record.clone());
        }
        if head != Hash64::default() || offset != 0 {
            return Err("TIR Finish rooted chunk chain has wrong length or cycle".into());
        }
    }
    let mut next = old.clone();
    let effect = if let PalwTirPublicOwnedActionV1::Finish { demand_id, .. } = action {
        let rows_anchor = if let Some(rows) = old.owned.accepted_rows_v1() {
            let hash = rows.body.hash_v1()?;
            let Some(Row::TirAcceptedBody(accepted)) = lookup(&Key::TirAcceptedBody(hash)) else {
                return Err("TIR rows anchor lost its rooted original all-ID body".into());
            };
            accepted.check_structure_v1(hash)?;
            if accepted.acquisition != key
                || accepted.source_id != contract.source_id
                || accepted.material_until_daa != contract.terms.material_until_daa
                || accepted.reference != rows.body
                || accepted.history_index >= old.owned.history_v1().0
                || rows.body.request != (crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::Ids { offset: 0 })
            {
                return Err("TIR rows anchor substitutes acquisition, source, TTL or accepted history".into());
            }
            // Only this native accepted pointer and rooted immutable body row
            // authorize the O(1) anchor. Wire/checkpoint values alone do not.
            Some(crate::palw_tir_public_source_v1::PalwTirPublicRowsAnchorV1::from_native_v1(source, rows.rows_root))
        } else { None };
        PalwTirPublicOwnedEffectV1::Finished(next.owned.finish_with_rows_v1(
            source, signer, *demand_id, now, &records, ids, rows_anchor.as_ref(),
        )?)
    } else {
        next.owned.apply_v1(source, signer, now, action, &records, ids)?
    };
    check_source(&next, source)?;
    if next.descriptor != old.descriptor || next.owned.contract_v1() != contract {
        return Err("TIR transition changes immutable descriptor or paid contract".into());
    }
    let mut changes = Vec::with_capacity(2);
    let extra = match &effect {
        PalwTirPublicOwnedEffectV1::Chunk(record) => {
            let head = record.head_v1()?;
            Some((
                Key::TirRawChunk(head),
                Row::TirRawChunk(
                    Box::new(PalwTirPublicChunkRowV1 {
                        acquisition: key,
                        source_id: contract.source_id,
                        material_until_daa: contract.terms.material_until_daa,
                        record: record.clone(),
                    })
                    .into(),
                ),
            ))
        }
        PalwTirPublicOwnedEffectV1::Finished(finish) => Some((
            Key::TirAcceptedBody(finish.body.hash_v1()?),
            Row::TirAcceptedBody(
                Box::new(PalwTirPublicAcceptedRowV1 {
                    acquisition: key,
                    source_id: contract.source_id,
                    material_until_daa: contract.terms.material_until_daa,
                    history_index: finish.history_index,
                    reference: finish.body.clone(),
                })
                .into(),
            ),
        )),
        _ => None,
    };
    if let Some((extra_key, row)) = extra {
        row.check(&extra_key)?;
        let previous = lookup(&extra_key);
        if previous.is_some_and(|previous| previous != &row) {
            return Err("TIR immutable rooted body/chunk key already has different material".into());
        }
        if previous.is_none() {
            changes.push(PalwTirPublicRowChangeV1 { key: extra_key, old: None, new: Some(row) });
        }
    }
    changes.push(PalwTirPublicRowChangeV1 { key, old: Some(Row::TirAcquisition(old.clone())), new: Some(Row::TirAcquisition(next)) });
    Ok(PalwTirPublicTransitionV1 { changes, effect })
}

fn check_work_input<'a>(
    owner: &PalwTirPublicAcquisitionRowV1,
    source: &PalwTirPublicSourceV1,
    debit: &crate::palw_tir_public_owned_v1::PalwTirPublicWorkDebitV1,
    lookup: &impl Fn(&Key) -> Option<&'a Row>,
) -> Result<(), String> {
    use crate::palw_tir_public_owned_v1::PalwTirPublicWorkPhaseV1 as P;
    let scope = &debit.scope;
    let key = owner.key_v1()?;
    match scope.phase {
        P::BodyRead | P::BodyAuthenticate => {
            let Some(Row::TirAcceptedBody(body)) = lookup(&Key::TirAcceptedBody(scope.input_head)) else {
                return Err("TIR body work has no exact native accepted reference".into());
            };
            check_reference(owner, source, body, debit.history.as_ref())?;
            if scope.request != body.reference.request || scope.input_offset > body.reference.total_bytes {
                return Err("TIR body work changes original accepted request/offset".into());
            }
            if scope.phase == P::BodyRead && scope.input_offset != 0 {
                let Some(Row::TirRawChunk(chunk)) = lookup(&Key::TirRawChunk(scope.cursor_id)) else {
                    return Err("TIR body work has no exact rooted chunk cursor".into());
                };
                chunk.check_structure_v1(scope.cursor_id)?;
                if chunk.acquisition != key
                    || chunk.source_id != source.source_id_v1()
                    || chunk.record.demand_id != body.reference.demand_id
                    || chunk.record.answerer != body.reference.answerer
                    || chunk.record.offset.checked_add(chunk.record.bytes.len() as u64) != Some(scope.input_offset)
                {
                    return Err("TIR body work substitutes another source/signer/turn/chunk offset".into());
                }
            }
            if scope.phase == P::BodyAuthenticate
                && (scope.cursor_id != body.reference.chunk_head || scope.input_offset != body.reference.total_bytes)
            {
                return Err("TIR body authentication changes original full-body head/size".into());
            }
        }
        P::ConeProbe => {
            let (count, root) = owner.owned.history_v1();
            if scope.input_head != root.unwrap_or_default() || scope.input_offset != count {
                return Err("TIR cone work changes current authenticated material revision".into());
            }
        }
        P::Capture | P::Response | P::Ids => {
            let p = owner.owned.active_v1().ok_or("TIR response preparation has no accepted demand")?;
            let request = match scope.request {
                crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::Ids { .. } => {
                    crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::Ids { offset: 0 }
                }
                other => other,
            };
            if p.request != request || scope.input_head != source.source_id_v1() {
                return Err("TIR response preparation changes current original request/source".into());
            }
        }
        P::Planning | P::OwnReplay => {
            if scope.input_head != source.source_id_v1() || scope.actor != owner.owned.contract_v1().owner {
                return Err("TIR independent replay/planning is not the outside owner's original source".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1;
    use crate::palw_step_leg::{step_merkle_root_v1, step_opening_v1};
    use crate::palw_tir_public_owned_v1::{PalwTirPublicOwnedPlanV1, palw_tir_public_body_hash_v1};
    use crate::palw_tir_public_source_v1::{PalwTirPublicRequestV1, PalwTirPublicResponseV1};
    use std::collections::BTreeSet;

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0))
    }
    // Preadmitted kernel fixture only: it is NOT a native paid-claim acceptance
    // test. Full SUM, literal grants, admission and holds are still unconnected.
    fn fixture() -> (PalwTirPublicSourceV1, BTreeMap<Key, Row>, Key, Vec<u8>) {
        let x = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&x.binding.class, &x.binding.artifact_root).unwrap();
        let mut header = crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1::of_binding_v1(&x.binding);
        let mut leaf = x.preimages[0].clone();
        leaf.values_le = vec![0xA5; 16_384];
        let mut hashes = x.hashes.clone();
        hashes[0] = crate::palw_step_leg::step_tile_leaf_hash_v1(&header.job_context.context_hash(), &record.facts.class_id, &leaf);
        header.step_merkle_root = step_merkle_root_v1(&hashes).unwrap();
        header.committed_execution_root = crate::palw_tir_step_v1::palw_tir_execution_root_v1(
            &header.job_context.context_hash(),
            &header.full_logits_trace_root,
            &record.facts.class_id,
            header.step_leaf_count,
            &header.step_merkle_root,
        );
        let maximum_leaves = 1 << 40;
        let source = PalwTirPublicSourceV1::new_v1(
            &record,
            &x.binding.class.layout,
            &header,
            header.committed_execution_root,
            header.full_logits_trace_root,
            maximum_leaves,
        )
        .unwrap();
        let config = PalwStagedPublicCourtConfigV1::network_v1();
        let plan = PalwTirPublicOwnedPlanV1::derive_v1(&source, 3, config).unwrap();
        let terms = PalwRenderingClaimTermsV1::derive(
            10,
            plan.envelope,
            config.acquisition_daa,
            config.collection_daa,
            config.reorg_retention_daa,
        )
        .unwrap();
        let contract = PalwTirPublicContractV1 {
            claim: Hash64::from_u64_word(31),
            owner: bond(7),
            reserved_daa: 10,
            bundle_id: Hash64::from_u64_word(32),
            source_id: source.source_id_v1(),
            terms,
            plan,
            producer: bond(1),
            custodians: BTreeSet::from([bond(1)]),
        };
        let capital = terms.plan.rent_sompi.checked_add(terms.plan.deposit_sompi).unwrap();
        let owned =
            PalwTirPublicOwnedV1::admit_native_v1(contract, &source, &BTreeSet::from([bond(2), bond(3)]), config, 3, 11, capital)
                .unwrap();
        let row = PalwTirPublicAcquisitionRowV1 {
            descriptor: PalwTirPublicSourceDescriptorV1 {
                class_id: record.facts.class_id,
                layout_digest: record.layout_digest,
                header_digest: hash(b"misaka-palw/tir-source-header/v1", &header).unwrap(),
                source_id: source.source_id_v1(),
                maximum_leaves,
                native_config: config,
                native_custodian_upper: 3,
                work_rules: None,
            },
            owned: Box::new(owned).into(),
        };
        let key = row.key_v1().unwrap();
        let body = borsh::to_vec(&PalwTirPublicResponseV1::StepLeaf { preimage: leaf, opening: step_opening_v1(&hashes, 0).unwrap() })
            .unwrap();
        (source, BTreeMap::from([(key, Row::TirAcquisition(Box::new(row).into()))]), key, body)
    }
    fn owned(rows: &BTreeMap<Key, Row>, key: Key) -> &PalwSharedCourtV1<PalwTirPublicAcquisitionRowV1> {
        let Row::TirAcquisition(row) = rows.get(&key).unwrap() else { panic!("metadata") };
        row
    }
    fn act(
        source: &PalwTirPublicSourceV1,
        rows: &BTreeMap<Key, Row>,
        key: Key,
        signer: PalwBondKeyV2,
        now: u64,
        action: &PalwTirPublicOwnedActionV1,
    ) -> Result<PalwTirPublicTransitionV1, String> {
        let row = owned(rows, key);
        let turn = if matches!(action, PalwTirPublicOwnedActionV1::Demand { .. } | PalwTirPublicOwnedActionV1::Reacquire { .. }) {
            row.owned.spent_v1().turns
        } else {
            row.owned.active_v1().unwrap().turn
        };
        transition(row, source, source.source_id_v1(), turn, signer, now, action, |k| rows.get(k), None)
    }
    fn commit(
        source: &PalwTirPublicSourceV1,
        rows: &mut BTreeMap<Key, Row>,
        key: Key,
        signer: PalwBondKeyV2,
        action: &PalwTirPublicOwnedActionV1,
    ) -> PalwTirPublicTransitionV1 {
        let change = act(source, rows, key, signer, 12, action).unwrap();
        change.apply_rows_v1(rows, true).unwrap();
        change
    }
    fn begin(source: &PalwTirPublicSourceV1, rows: &mut BTreeMap<Key, Row>, key: Key, body: &[u8]) -> Hash64 {
        commit(
            source,
            rows,
            key,
            bond(7),
            &PalwTirPublicOwnedActionV1::Demand { request: PalwTirPublicRequestV1::StepLeaf { index: 0 } },
        );
        let demand_id = owned(rows, key).owned.active_v1().unwrap().demand_id;
        commit(
            source,
            rows,
            key,
            bond(1),
            &PalwTirPublicOwnedActionV1::Begin {
                demand_id,
                answerer: bond(1),
                total_bytes: body.len() as u64,
                body_hash: palw_tir_public_body_hash_v1(body),
            },
        );
        demand_id
    }
    fn upload(source: &PalwTirPublicSourceV1, rows: &mut BTreeMap<Key, Row>, key: Key, body: &[u8]) -> Hash64 {
        let demand_id = begin(source, rows, key, body);
        for (i, bytes) in body.chunks(PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1).enumerate() {
            commit(
                source,
                rows,
                key,
                bond(1),
                &PalwTirPublicOwnedActionV1::Part {
                    demand_id,
                    answerer: bond(1),
                    offset: (i * PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1) as u64,
                    bytes: bytes.to_vec(),
                },
            );
        }
        demand_id
    }

    #[test]
    fn appended_keys_rows_and_delta210_preserve_legacy_ordinals() {
        let h = Hash64::from_u64_word(1);
        for (tag, key) in [Key::Claim(h), Key::Owner(h, bond(1)), Key::InventoryOpen(h, bond(1)), Key::Inventory(h), Key::TirLayout(h)]
            .into_iter()
            .enumerate()
        {
            let mut expected = vec![tag as u8];
            expected.extend(borsh::to_vec(&h).unwrap());
            if tag == 1 || tag == 2 {
                expected.extend(borsh::to_vec(&bond(1)).unwrap());
            }
            assert_eq!(borsh::to_vec(&key).unwrap(), expected);
        }
        let x = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let layout = Row::TirLayout(Box::new(x.binding.class.layout.clone()));
        let mut expected = vec![10];
        expected.extend(borsh::to_vec(&x.binding.class.layout).unwrap());
        assert_eq!(borsh::to_vec(&layout).unwrap(), expected);
        let (source, mut rows, key, body) = fixture();
        assert_eq!(borsh::to_vec(&key).unwrap()[0], 5);
        assert_eq!(borsh::to_vec(rows.get(&key).unwrap()).unwrap()[0], 11);
        assert!(rows.get(&key).unwrap().active_row_v1().is_none());
        let demand = begin(&source, &mut rows, key, &body);
        let part = act(
            &source,
            &rows,
            key,
            bond(1),
            12,
            &PalwTirPublicOwnedActionV1::Part { demand_id: demand, answerer: bond(1), offset: 0, bytes: body[..16_384].to_vec() },
        )
        .unwrap();
        assert_eq!(part.changes_v1().len(), 2);
        assert_eq!(borsh::to_vec(&part.changes_v1()[0].key).unwrap()[0], 6);
        assert_eq!(borsh::to_vec(part.changes_v1()[0].new.as_ref().unwrap()).unwrap()[0], 12);
        for delta in part.deltas_v1() {
            assert_eq!(borsh::to_vec(&delta).unwrap()[0], 210);
        }
    }

    #[test]
    fn part_metadata_is_atomic_reversible_and_body_shared() {
        let (source, mut rows, key, body) = fixture();
        let demand = begin(&source, &mut rows, key, &body);
        let before = borsh::to_vec(&rows).unwrap();
        let step = act(
            &source,
            &rows,
            key,
            bond(1),
            12,
            &PalwTirPublicOwnedActionV1::Part { demand_id: demand, answerer: bond(1), offset: 0, bytes: body[..16_384].to_vec() },
        )
        .unwrap();
        let duplicate = PalwTirPublicTransitionV1 {
            changes: vec![step.changes_v1()[0].clone(), step.changes_v1()[0].clone()],
            effect: PalwTirPublicOwnedEffectV1::Begun,
        };
        assert!(duplicate.apply_rows_v1(&mut rows, true).is_err());
        assert_eq!(borsh::to_vec(&rows).unwrap(), before);
        step.apply_rows_v1(&mut rows, true).unwrap();
        let after = borsh::to_vec(&rows).unwrap();
        assert_ne!(before, after);
        assert!(borsh::to_vec(&owned(&rows, key).owned).unwrap().len() < 16_384);
        let parent = rows.clone();
        let Row::TirRawChunk(a) = rows.get(&step.changes_v1()[0].key).unwrap() else { panic!("chunk") };
        let Row::TirRawChunk(b) = parent.get(&step.changes_v1()[0].key).unwrap() else { panic!("chunk") };
        assert!(std::ptr::eq(a.as_ref(), b.as_ref()));
        assert!(step.apply_rows_v1(&mut rows, true).is_err());
        assert_eq!(borsh::to_vec(&rows).unwrap(), after);
        step.apply_rows_v1(&mut rows, false).unwrap();
        assert_eq!(borsh::to_vec(&rows).unwrap(), before);
        step.apply_rows_v1(&mut rows, true).unwrap();
        assert_eq!(borsh::to_vec(&rows).unwrap(), after);
        // A stale metadata precondition must not insert the earlier chunk.
        let mut stale = parent;
        stale.remove(&step.changes_v1()[0].key);
        let saved = borsh::to_vec(&stale).unwrap();
        assert!(step.apply_rows_v1(&mut stale, true).is_err());
        assert_eq!(borsh::to_vec(&stale).unwrap(), saved);
    }

    #[test]
    fn finish_uses_rooted_scope_and_references_current_history() {
        let (source, mut rows, key, body) = fixture();
        let demand = upload(&source, &mut rows, key, &body);
        let finish = PalwTirPublicOwnedActionV1::Finish { demand_id: demand, answerer: bond(1) };
        let head = owned(&rows, key).owned.slot_v1(&bond(1)).unwrap().head;
        let saved_chunk = rows.remove(&Key::TirRawChunk(head)).unwrap();
        let before = borsh::to_vec(&rows).unwrap();
        assert!(act(&source, &rows, key, bond(1), 12, &finish).is_err());
        assert_eq!(borsh::to_vec(&rows).unwrap(), before);
        rows.insert(Key::TirRawChunk(head), saved_chunk.clone());
        let Row::TirRawChunk(mut foreign) = saved_chunk else { panic!("chunk") };
        foreign.source_id = Hash64::from_u64_word(99);
        rows.insert(Key::TirRawChunk(head), Row::TirRawChunk(foreign));
        assert!(act(&source, &rows, key, bond(1), 12, &finish).is_err());
        let chunk = rows.get_mut(&Key::TirRawChunk(head)).unwrap();
        let Row::TirRawChunk(chunk) = chunk else { panic!("chunk") };
        chunk.source_id = source.source_id_v1();
        let before = borsh::to_vec(&rows).unwrap();
        let step = commit(&source, &mut rows, key, bond(1), &finish);
        let after = borsh::to_vec(&rows).unwrap();
        assert_eq!(step.changes_v1().len(), 2);
        let Row::TirAcceptedBody(accepted) = step.changes_v1()[0].new.as_ref().unwrap() else { panic!("reference") };
        assert_eq!(borsh::to_vec(&step.changes_v1()[0].key).unwrap()[0], 7);
        assert_eq!(borsh::to_vec(step.changes_v1()[0].new.as_ref().unwrap()).unwrap()[0], 13);
        check_reference(owned(&rows, key), &source, accepted, None).unwrap();
        assert!(act(&source, &rows, key, bond(1), 12, &finish).is_err());
        step.apply_rows_v1(&mut rows, false).unwrap();
        assert_eq!(borsh::to_vec(&rows).unwrap(), before);
        step.apply_rows_v1(&mut rows, true).unwrap();
        assert_eq!(borsh::to_vec(&rows).unwrap(), after);
        let reference = accepted.reference.clone();
        let proof = step_opening_v1(&[reference.hash_v1().unwrap()], 0).unwrap();
        let reacquire = PalwTirPublicOwnedActionV1::Reacquire { previous: reference.clone(), proof: proof.clone(), nonce: 0 };
        commit(&source, &mut rows, key, bond(7), &reacquire);
        assert!(check_reference(owned(&rows, key), &source, accepted, None).is_err());
        check_reference(owned(&rows, key), &source, accepted, Some(&proof)).unwrap();
        assert!(act(&source, &rows, key, bond(7), 12, &reacquire).is_err());
        let accepted_key = Key::TirAcceptedBody(reference.hash_v1().unwrap());
        rows.remove(&accepted_key);
        let mut next = reacquire;
        if let PalwTirPublicOwnedActionV1::Reacquire { nonce, .. } = &mut next {
            *nonce = 1;
        }
        assert!(act(&source, &rows, key, bond(7), 12, &next).is_err());
    }

    #[test]
    fn literal_turn_source_signer_clock_reject_before_chunk_read() {
        let (source, mut rows, key, body) = fixture();
        let demand = upload(&source, &mut rows, key, &body);
        let row = owned(&rows, key);
        let turn = row.owned.active_v1().unwrap().turn;
        let now = row.owned.active_v1().unwrap().respond_until_daa;
        let action = PalwTirPublicOwnedActionV1::Finish { demand_id: demand, answerer: bond(1) };
        let no_read = |_: &Key| -> Option<&Row> { panic!("unauthorized Finish must not load raw body") };
        for (expected_source, expected_turn, signer, at, action) in [
            (Hash64::from_u64_word(9), turn, bond(1), 12, action.clone()),
            (source.source_id_v1(), turn + 1, bond(1), 12, action.clone()),
            (source.source_id_v1(), turn, bond(2), 12, action.clone()),
            (source.source_id_v1(), turn, bond(1), now, action.clone()),
            (
                source.source_id_v1(),
                turn,
                bond(1),
                12,
                PalwTirPublicOwnedActionV1::Finish { demand_id: Hash64::from_u64_word(9), answerer: bond(1) },
            ),
        ] {
            assert!(transition(row, &source, expected_source, expected_turn, signer, at, &action, no_read, None).is_err());
        }
        let before = borsh::to_vec(row).unwrap();
        let mut changed = row.clone();
        changed.descriptor.header_digest = Hash64::from_u64_word(9);
        assert!(check_source(&changed, &source).is_err());
        assert!(changed.check_structure_v1(&key).is_err());
        let mut changed = row.clone();
        changed.descriptor.native_custodian_upper += 1;
        assert!(check_source(&changed, &source).is_err());
        assert_eq!(borsh::to_vec(row).unwrap(), before);
    }

    #[test]
    fn rooted_chunks_remain_bounded_before_allocation_and_no_semantic_authority() {
        let (_, rows, key, _) = fixture();
        let acquisition = owned(&rows, key);
        let chunk = PalwTirPublicChunkRowV1 {
            acquisition: key,
            source_id: acquisition.descriptor.source_id,
            material_until_daa: acquisition.owned.contract_v1().terms.material_until_daa,
            record: PalwTirPublicChunkRecordV1 {
                demand_id: Hash64::from_u64_word(1),
                answerer: bond(1),
                previous: Hash64::default(),
                offset: 0,
                bytes: vec![1; 16_384],
            },
        };
        let head = chunk.record.head_v1().unwrap();
        let row = Row::TirRawChunk(Box::new(chunk).into());
        row.check(&Key::TirRawChunk(head)).unwrap();
        assert!(row.active_row_v1().is_none());
        assert!(row.owner_accounting_v1().is_none());
        let bytes = borsh::to_vec(&row).unwrap();
        assert_eq!(borsh::from_slice::<Row>(&bytes).unwrap(), row);
        let mut bomb = borsh::to_vec(&(Hash64::default(), bond(1), Hash64::default(), 0u64)).unwrap();
        bomb.extend(u32::MAX.to_le_bytes());
        assert!(borsh::from_slice::<PalwTirPublicChunkRecordV1>(&bomb).is_err());
    }
}
