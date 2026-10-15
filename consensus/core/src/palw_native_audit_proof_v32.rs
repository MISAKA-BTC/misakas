//! Readonly native audit openings against independently pinned state roots.
//! The frozen state/aux hash formats are unchanged. New proof labels identify
//! their exact commitment positions; producer JSON never mints this view.
use crate::{
    Hash64,
    palw_state_proof_v1::{PalwCollectionOpeningV1, PalwFactProofV1, PalwStateOpeningV1},
    palw_state_v2::{PALW_STATE_V2_VERSION, PalwChainStateV2},
};
pub const NATIVE_AUX_LABEL_V32: &[u8] = b"kernel-aux-v32";
pub const NATIVE_PAYOUT_LABEL_V32: &[u8] = b"pending-payouts-v32";
pub const NATIVE_AUX_PROOF_MAX_BYTES_V32: usize = 64 << 20;
pub const NATIVE_AUX_PROOF_MAX_ROWS_V32: usize = 65_536;
fn checked_extent(proof: &PalwFactProofV1) -> Result<(), String> {
    if proof.state.preimage.len() > 4 << 20 || proof.collection.rows.len() > NATIVE_AUX_PROOF_MAX_ROWS_V32 {
        return Err("native audit32 bounded opening/row count differs".into());
    }
    let total = proof
        .collection
        .rows
        .iter()
        .try_fold(proof.state.preimage.len(), |n, (k, v)| n.checked_add(k.len())?.checked_add(v.len()))
        .ok_or("native audit32 proof extent overflow")?;
    if total > NATIVE_AUX_PROOF_MAX_BYTES_V32 {
        return Err("native audit32 flat opening exceeds bounded extent".into());
    }
    Ok(())
}
/// This raw root is used only with the explicit new proof label. Every encoded
/// key must be the exact Borsh `(table:u8,key:Vec<u8>)`; values are original raw
/// row bytes. They enter the SAME aux-root/v1 hash as the consensus store.
pub fn native_aux_opening_root_v32(rows: &[(Vec<u8>, Vec<u8>)]) -> Result<Hash64, String> {
    if rows.len() > NATIVE_AUX_PROOF_MAX_ROWS_V32 {
        return Err("native audit32 aux row count".into());
    }
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/kernel-route/aux-root/v1").to_state();
    h.update(&(rows.len() as u64).to_le_bytes());
    let mut previous: Option<(u8, &[u8])> = None;
    for (k, v) in rows {
        if k.len() < 5 || k.len() > 4101 || v.len() > 32 << 20 {
            return Err("native audit32 bounded exact aux row".into());
        }
        let n = u32::from_le_bytes(k[1..5].try_into().unwrap()) as usize;
        if n != k.len() - 5 || previous.is_some_and(|p| p >= (k[0], &k[5..])) {
            return Err("native audit32 duplicate/reordered/noncanonical aux key".into());
        }
        previous = Some((k[0], &k[5..]));
        h.update(k);
        h.update(&(v.len() as u32).to_le_bytes());
        h.update(v);
    }
    let mut root = [0; 64];
    root.copy_from_slice(h.finalize().as_bytes());
    Ok(Hash64::from_bytes(root))
}
pub fn prove_native_aux_v32(state: &PalwChainStateV2) -> Result<PalwFactProofV1, String> {
    let route = state.kernel_route().ok_or("native audit32 kernel route absent")?;
    if route.aux.len() > NATIVE_AUX_PROOF_MAX_ROWS_V32 {
        return Err("native audit32 aux row count bound".into());
    }
    // Preflight BEFORE cloning any full source/trace/court payload.
    let opening = PalwStateOpeningV1::of(state);
    let total = route
        .aux
        .iter()
        .try_fold(opening.preimage.len(), |n, ((_, k), v)| n.checked_add(5)?.checked_add(k.len())?.checked_add(v.len()))
        .ok_or("native audit32 original row extent overflow")?;
    if total > NATIVE_AUX_PROOF_MAX_BYTES_V32 {
        return Err("native audit32 use a bounded independent replay instance; full flat proof exceeds cap".into());
    }
    let rows =
        route.aux.iter().map(|((table, key), value)| (borsh::to_vec(&(*table, key)).expect("fixed aux key"), value.clone())).collect();
    Ok(PalwFactProofV1 { state: opening, collection: PalwCollectionOpeningV1 { label: NATIVE_AUX_LABEL_V32.to_vec(), rows } })
}
pub fn prove_native_payouts_v32(state: &PalwChainStateV2) -> Result<PalwFactProofV1, String> {
    let proof = PalwFactProofV1 {
        state: PalwStateOpeningV1::of(state),
        collection: PalwCollectionOpeningV1 {
            label: NATIVE_PAYOUT_LABEL_V32.to_vec(),
            rows: state
                .pending_payouts_iter()
                .map(|(k, v)| (borsh::to_vec(k).expect("fixed payout key"), borsh::to_vec(v).expect("fixed payout value")))
                .collect(),
        },
    };
    checked_extent(&proof)?;
    Ok(proof)
}
/// Nonwire, private mint. It proves original native rows at a pinned inner
/// state root; it does not prove PoW/fork choice, a Final or coin redemption.
pub struct VerifiedNativeAuxRowsV32<'a> {
    state: Hash64,
    root: Hash64,
    ledger: Hash64,
    rows: &'a [(Vec<u8>, Vec<u8>)],
}
impl<'a> VerifiedNativeAuxRowsV32<'a> {
    pub fn state_root(&self) -> Hash64 {
        self.state
    }
    pub fn root(&self) -> Hash64 {
        self.root
    }
    pub fn ledger_root(&self) -> Hash64 {
        self.ledger
    }
    pub fn rows(&self) -> &'a [(Vec<u8>, Vec<u8>)] {
        self.rows
    }
    pub fn row(&self, table: u8, key: &[u8]) -> Option<&'a [u8]> {
        let k = borsh::to_vec(&(table, key)).ok()?;
        self.rows.iter().find(|(j, _)| *j == k).map(|(_, v)| v.as_slice())
    }
}
pub fn verify_native_aux_v32<'a>(
    proof: &'a PalwFactProofV1,
    pinned_inner_state: Hash64,
) -> Result<VerifiedNativeAuxRowsV32<'a>, String> {
    checked_extent(proof)?;
    if proof.collection.label != NATIVE_AUX_LABEL_V32
        || proof.state.root() != pinned_inner_state
        || proof.state.preimage.get(..2) != Some(PALW_STATE_V2_VERSION.to_le_bytes().as_slice())
    {
        return Err("native audit32 exact label/version/pinned state mismatch".into());
    }
    let root = native_aux_opening_root_v32(&proof.collection.rows)?;
    let marker = b"kernel-route/v1";
    let mut positions = proof.state.preimage.windows(marker.len()).enumerate().filter_map(|(i, v)| (v == marker).then_some(i));
    let start = positions.next().ok_or("native audit32 original kernel commitment absent")? + marker.len();
    if positions.next().is_some() {
        return Err("native audit32 ambiguous original kernel commitment".into());
    }
    let commitment = proof.state.preimage.get(start..start + 128).ok_or("native audit32 truncated kernel commitment")?;
    if commitment[64..] != root.as_bytes() {
        return Err("native audit32 aux rows do not open the original kernel aux position".into());
    }
    Ok(VerifiedNativeAuxRowsV32 {
        state: pinned_inner_state,
        root,
        ledger: Hash64::from_bytes(commitment[..64].try_into().unwrap()),
        rows: &proof.collection.rows,
    })
}
pub fn verify_native_payouts_v32<'a>(
    proof: &'a PalwFactProofV1,
    pinned_inner_state: Hash64,
) -> Result<&'a [(Vec<u8>, Vec<u8>)], String> {
    checked_extent(proof)?;
    let raw = &proof.state.preimage;
    if proof.collection.label != NATIVE_PAYOUT_LABEL_V32
        || proof.state.root() != pinned_inner_state
        || raw.get(..2) != Some(PALW_STATE_V2_VERSION.to_le_bytes().as_slice())
    {
        return Err("native audit32 payout label/version/pinned state differs".into());
    }
    // ADR-0043's fixed seven leading collections, then the bounded optional
    // epoch budget and receipt/capability/claim roots. Never scan arbitrary
    // bytes for a purported payout root.
    let mut offset = 2 + 7 * 64;
    match raw.get(offset) {
        Some(0) => offset += 1,
        Some(1) => {
            let n = u32::from_le_bytes(
                raw.get(offset + 9..offset + 13).ok_or("native audit32 truncated epoch budget")?.try_into().unwrap(),
            ) as usize;
            offset = offset
                .checked_add(13)
                .and_then(|o| n.checked_mul(72).and_then(|n| o.checked_add(n)))
                .ok_or("native audit32 budget extent overflow")?;
        }
        _ => return Err("native audit32 epoch budget option differs".into()),
    }
    offset = offset.checked_add(3 * 64).ok_or("native audit32 payout offset overflow")?;
    let expected = raw.get(offset..offset + 64).ok_or("native audit32 payout position absent")?;
    let root = crate::palw_state_v2::palw_collection_root_of_entries_v1(
        b"pending_payouts",
        proof.collection.rows.len(),
        proof.collection.rows.iter().cloned(),
    );
    if expected != root.as_bytes() {
        return Err("native audit32 original payout rows mismatch".into());
    }
    if proof.collection.rows.windows(2).any(|w| w[0].0 >= w[1].0) {
        return Err("native audit32 duplicate or reordered payout key".into());
    }
    Ok(&proof.collection.rows)
}

/// Authenticated readonly payout table. An observation is not a redeemed
/// transaction or an unspent-output proof.
pub struct VerifiedNativePayoutRowsV32<'a> {
    state: Hash64,
    rows: &'a [(Vec<u8>, Vec<u8>)],
}
impl<'a> VerifiedNativePayoutRowsV32<'a> {
    pub fn state_root(&self) -> Hash64 {
        self.state
    }
    pub fn rows(&self) -> &'a [(Vec<u8>, Vec<u8>)] {
        self.rows
    }
}
pub fn verify_native_payout_rows_v32<'a>(
    proof: &'a PalwFactProofV1,
    pinned_inner_state: Hash64,
) -> Result<VerifiedNativePayoutRowsV32<'a>, String> {
    Ok(VerifiedNativePayoutRowsV32 { state: pinned_inner_state, rows: verify_native_payouts_v32(proof, pinned_inner_state)? })
}
/// Re-derive every audit field from proven original rows. No serialized audit
/// DTO, producer truth flag, or caller-made private view is a minting input.
/// The caller pins its own chain/header/selected-parent and ruleset first;
/// SDK's anchored wrapper authenticates this DAA against that parent header.
pub fn derive_native_execution_audit_from_proofs_v32(
    auxiliary: &VerifiedNativeAuxRowsV32<'_>,
    payouts: &VerifiedNativePayoutRowsV32<'_>,
    parameters: &crate::config::params::Params,
    daa: u64,
    job: [u8; 64],
) -> Result<crate::palw_native_audit_v27::CheckedNativeExecutionAuditViewV27, String> {
    if auxiliary.state != payouts.state
        || !parameters.palw_native_final_active_at_v27(daa)
        || !parameters.palw_funded_execution_active_at_v11(daa)
    {
        return Err("native audit32 original proof state or independent owning fences differ".into());
    }
    let mut rows = std::collections::BTreeMap::new();
    for (k, v) in auxiliary.rows {
        // Keys were bounded and authenticated BEFORE any native row decode.
        let (table, key): (u8, Vec<u8>) = borsh::from_slice(k).map_err(|_| "native audit32 exact original auxiliary key")?;
        if rows.insert((table, key), v.clone()).is_some() {
            return Err("native audit32 duplicate original row".into());
        }
    }
    let mut queue = std::collections::BTreeMap::new();
    for (k, v) in payouts.rows {
        if k.len() != 64 || v.len() != 72 {
            return Err("native audit32 fixed payout row extent".into());
        }
        let id: Hash64 = borsh::from_slice(k).map_err(|_| "native audit32 payout key grammar")?;
        let p: crate::palw_state_v2::PalwPayoutV2 = borsh::from_slice(v).map_err(|_| "native audit32 payout value grammar")?;
        if queue.insert(id, p).is_some() {
            return Err("native audit32 duplicate original payout".into());
        }
    }
    let domain =
        crate::palw_attempt_v2::palw_network_domain_v2_for(parameters.net.to_string().as_bytes(), Some(parameters.genesis.hash))
            .as_bytes();
    let view = PalwChainStateV2::native_execution_audit_from_authenticated_rows_v27(
        &rows,
        domain,
        daa,
        auxiliary.state.as_bytes(),
        auxiliary.ledger.as_bytes(),
        auxiliary.root.as_bytes(),
        &queue,
        job,
    )?;
    if view.record().original_final_policy
        != parameters.palw_native_final_v27.as_ref().ok_or("native audit32 original trusted Final policy absent")?.policy.root()
    {
        return Err("native audit32 original Final consent differs from independently pinned Params".into());
    }
    Ok(view)
}

/// Audit-time Master32 composite Final (lead default 2026-10-15, pending user review).
/// NOT a consensus object: a deterministic function of the verified original aux rows,
/// the owning ruleset's Stage28 policy and public tokens/outputs. Every component must
/// already be an individually genuine settled Stage28 Final in those rows.
pub struct CheckedNativeMasterCompositeV32 {
    pub master: [u8; 64],
    pub composite: [u8; 64],
    pub components: Vec<[u8; 64]>,
    pub state: Hash64,
    pub aux_root: Hash64,
}
pub fn derive_native_master_composite_v32(
    auxiliary: &VerifiedNativeAuxRowsV32<'_>,
    parameters: &crate::config::params::Params,
    daa: u64,
    master: [u8; 64],
    jobs: &[[u8; 64]],
    values: &[Vec<misaka_palw_tir::Tensor>],
    generated: &[u32],
) -> Result<CheckedNativeMasterCompositeV32, String> {
    let stage = parameters
        .palw_stage_execution_v28
        .as_ref()
        .filter(|f| f.activation.is_active(daa))
        .ok_or("master32 composite owning Stage28 fence inactive")?;
    let mut rows = std::collections::BTreeMap::new();
    for (k, v) in auxiliary.rows {
        let (table, key): (u8, Vec<u8>) = borsh::from_slice(k).map_err(|_| "master32 composite exact original auxiliary key")?;
        if rows.insert((table, key), v.clone()).is_some() {
            return Err("master32 composite duplicate original row".into());
        }
    }
    let (composite, components) =
        PalwChainStateV2::native_master_composite_from_rows_v32(&rows, &stage.policies, master, jobs, values, generated)?;
    Ok(CheckedNativeMasterCompositeV32 { master, composite, components, state: auxiliary.state, aux_root: auxiliary.root })
}
/// Bounded public transport of component outputs: `u32 stages (<=64)`, then per stage
/// `u32 tensors (<=65536)`, then per tensor `u8 dtype tag, u8 rank (<=8), rank*u32 dims,
/// u32 byte length, little-endian bytes`. Every count is bounded by the remaining bytes
/// BEFORE allocation; the kernel then authenticates each tensor against its trace opening.
pub const NATIVE_MASTER_OUTPUTS_MAX_BYTES_V32: usize = 256 << 20;
pub fn decode_native_master_outputs_v32(raw: &[u8]) -> Result<Vec<Vec<misaka_palw_tir::Tensor>>, String> {
    fn take<'a>(raw: &'a [u8], at: &mut usize, n: usize) -> Result<&'a [u8], String> {
        let end = at.checked_add(n).ok_or("master32 outputs offset overflow")?;
        let v = raw.get(*at..end).ok_or("master32 outputs truncated")?;
        *at = end;
        Ok(v)
    }
    fn u32_at(raw: &[u8], at: &mut usize) -> Result<usize, String> {
        Ok(u32::from_le_bytes(take(raw, at, 4)?.try_into().unwrap()) as usize)
    }
    if raw.len() > NATIVE_MASTER_OUTPUTS_MAX_BYTES_V32 {
        return Err("master32 outputs byte cap".into());
    }
    let mut at = 0;
    let stages = u32_at(raw, &mut at)?;
    if stages == 0 || stages > 64 {
        return Err("master32 outputs stage count".into());
    }
    let mut out = Vec::with_capacity(stages);
    for _ in 0..stages {
        let n = u32_at(raw, &mut at)?;
        if n > 65536 || n > raw.len().saturating_sub(at) / 6 {
            return Err("master32 outputs tensor count".into());
        }
        let mut tensors = Vec::with_capacity(n);
        for _ in 0..n {
            let tag = take(raw, &mut at, 1)?[0];
            let dtype = *misaka_palw_tir::DType::ALL.get(tag as usize).ok_or("master32 outputs dtype tag")?;
            let rank = take(raw, &mut at, 1)?[0] as usize;
            if rank > 8 {
                return Err("master32 outputs rank".into());
            }
            let mut shape = Vec::with_capacity(rank);
            for _ in 0..rank {
                shape.push(u32_at(raw, &mut at)?);
            }
            let bytes = u32_at(raw, &mut at)?;
            let body = take(raw, &mut at, bytes)?;
            tensors.push(misaka_palw_tir::Tensor::from_le_bytes(dtype, &shape, body).map_err(|e| e.to_string())?);
        }
        out.push(tensors);
    }
    if at != raw.len() {
        return Err("master32 outputs trailing bytes".into());
    }
    Ok(out)
}

/// Which earlier lifecycle stage an independent auditor anchors (real-checkpoint path).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeLifecycleStageV32 {
    /// The class is registered against a mature, unrefuted original source binding.
    Register,
    /// A job is funded with the producer's original signed consent (the "seat" of the
    /// Panel-free native lane: the consenting producer bond holding the model).
    Seat,
}
/// Fields re-derived from authenticated original rows only. `job`/`funding`/`consent`
/// are zero for `Register`. Never a producer/node assertion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeLifecycleAnchorV32 {
    pub namespace: u16,
    pub stage: NativeLifecycleStageV32,
    pub class: [u8; 64],
    pub source: [u8; 64],
    pub program: [u8; 64],
    pub maximum_scope: [u8; 64],
    pub maximum_resource: [u8; 64],
    pub job: [u8; 64],
    pub funding: [u8; 64],
    pub consent: [u8; 64],
    pub requester_operator: [u8; 64],
    pub producer_operator: [u8; 64],
    pub terminal: String,
    pub anchored_daa: u64,
}
pub struct CheckedNativeLifecycleAnchorV32 {
    anchor: NativeLifecycleAnchorV32,
    state: Hash64,
    aux_root: Hash64,
}
impl CheckedNativeLifecycleAnchorV32 {
    pub fn anchor(&self) -> &NativeLifecycleAnchorV32 {
        &self.anchor
    }
    pub fn state_root(&self) -> Hash64 {
        self.state
    }
    pub fn aux_root(&self) -> Hash64 {
        self.aux_root
    }
}
pub fn derive_native_lifecycle_anchor_v32(
    auxiliary: &VerifiedNativeAuxRowsV32<'_>,
    parameters: &crate::config::params::Params,
    daa: u64,
    stage: NativeLifecycleStageV32,
    id: [u8; 64],
) -> Result<CheckedNativeLifecycleAnchorV32, String> {
    let mut rows = std::collections::BTreeMap::new();
    for (k, v) in auxiliary.rows {
        let (table, key): (u8, Vec<u8>) = borsh::from_slice(k).map_err(|_| "lifecycle32 exact original auxiliary key")?;
        if rows.insert((table, key), v.clone()).is_some() {
            return Err("lifecycle32 duplicate original row".into());
        }
    }
    let text_key = |kind: u8| borsh::to_vec(&(29u8, kind, id)).expect("fixed key");
    let stage_key = |kind: u8| borsh::to_vec(&(28u8, kind, id)).expect("fixed key");
    let (text_table, stage_table, text_kind, stage_kind) = match stage {
        NativeLifecycleStageV32::Register => {
            (crate::palw_flat_execution_v11::EXECUTION_CLASS_TABLE_V11, crate::palw_flat_stage_v14::STAGE_TABLE_V14, 0u8, 2u8)
        }
        NativeLifecycleStageV32::Seat => {
            (crate::palw_flat_execution_v11::EXECUTION_JOB_TABLE_V11, crate::palw_flat_stage_v14::STAGE_TABLE_V14, 0u8, 5u8)
        }
    };
    let anchor = if rows.contains_key(&(text_table, text_key(text_kind))) {
        if !parameters.palw_native_final_active_at_v27(daa) || !parameters.palw_funded_execution_active_at_v11(daa) {
            return Err("lifecycle32 Text29 owning fences inactive".into());
        }
        PalwChainStateV2::native_text_lifecycle_from_rows_v29(&rows, stage, id)?
    } else if rows.contains_key(&(stage_table, stage_key(stage_kind))) {
        let fence = parameters
            .palw_stage_execution_v28
            .as_ref()
            .filter(|f| f.activation.is_active(daa))
            .ok_or("lifecycle32 Stage28 owning fence inactive")?;
        PalwChainStateV2::native_stage_lifecycle_from_rows_v28(&rows, &fence.policies, stage, id)?
    } else {
        return Err("lifecycle32 no original Text29/Stage28 row for this id".into());
    };
    Ok(CheckedNativeLifecycleAnchorV32 { anchor, state: auxiliary.state, aux_root: auxiliary.root })
}

/// Complete-model identity/receipt projected only from the privately verified
/// original aux opening. A producer DTO cannot mint this checked wrapper.
pub struct CheckedNativeMasterExecutionV32 {
    record:crate::palw_flat_stage_v28::NativeMasterExecutionRecordV32,
    state:Hash64,auxiliary:Hash64,
}
impl CheckedNativeMasterExecutionV32 {
    pub fn record(&self)->&crate::palw_flat_stage_v28::NativeMasterExecutionRecordV32{&self.record}
    pub fn state(&self)->Hash64{self.state}
    pub fn auxiliary_root(&self)->Hash64{self.auxiliary}
}
pub fn derive_native_master_execution_record_v32(auxiliary:&VerifiedNativeAuxRowsV32<'_>,parameters:&crate::config::params::Params,daa:u64,master:[u8;64],jobs:&[[u8;64]],outputs:&[Vec<misaka_palw_tir::Tensor>],generated:&[u32])->Result<CheckedNativeMasterExecutionV32,String>{
    let p=&parameters.palw_stage_execution_v28.as_ref().filter(|f|f.activation.is_active(daa)).ok_or("master32 receipt original Stage28 fence inactive")?.policies;
    let mut rows=std::collections::BTreeMap::new();
    for(k,v)in auxiliary.rows{let key:(u8,Vec<u8>)=borsh::from_slice(k).map_err(|_|"master32 receipt exact original aux key")?;if rows.insert(key,v.clone()).is_some(){return Err("master32 receipt duplicate original aux row".into());}}
    let record=PalwChainStateV2::native_master_execution_record_from_rows_v32(&rows,p,master,jobs,outputs,generated)?;
    Ok(CheckedNativeMasterExecutionV32{record,state:auxiliary.state,auxiliary:auxiliary.root})
}
