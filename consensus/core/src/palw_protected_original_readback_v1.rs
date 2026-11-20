//! Snapshot-local identity readback, using the same admission-link check as
//! Final and receipt spending. This is a node observation, not a state proof.
use crate::{Hash64, palw_state_v2::PalwChainStateV2};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectedOriginalReadbackV1 {
    pub version: u16,
    pub claim: Hash64,
    pub execution_root: Hash64,
    pub original: Hash64,
    pub carrier_transaction: Hash64,
    pub carrier_block: Hash64,
    pub accepting_block: Hash64,
    pub accepted_tx_ordinal: u32,
    pub event_ordinal: u32,
    pub accepted_daa: u64,
    pub accepted_blue_score: u64,
    pub observed_tip: Hash64,
    pub observed_tip_daa: u64,
    pub full_replay_approval_recorded: bool,
    pub public_refutation_recorded: bool,
}

/// Immutable identity of the exact admitted original checked by a Panel.
/// The observed tip and approval flags are deliberately not replay identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtectedReplayBindingV1 {
    pub claim: Hash64,
    pub execution_root: Hash64,
    pub original: Hash64,
    pub carrier_transaction: Hash64,
    pub carrier_block: Hash64,
    pub accepting_block: Hash64,
    pub accepted_tx_ordinal: u32,
    pub event_ordinal: u32,
    pub accepted_daa: u64,
    pub accepted_blue_score: u64,
}

impl ProtectedOriginalReadbackV1 {
    pub fn replay_binding_v1(&self) -> ProtectedReplayBindingV1 {
        ProtectedReplayBindingV1 {
            claim: self.claim,
            execution_root: self.execution_root,
            original: self.original,
            carrier_transaction: self.carrier_transaction,
            carrier_block: self.carrier_block,
            accepting_block: self.accepting_block,
            accepted_tx_ordinal: self.accepted_tx_ordinal,
            event_ordinal: self.event_ordinal,
            accepted_daa: self.accepted_daa,
            accepted_blue_score: self.accepted_blue_score,
        }
    }

    pub fn check_carrier_v1(&self, claim: Hash64, txid: Hash64, execution_root: Hash64) -> Result<(), String> {
        if self.version != 1 || self.claim != claim || self.carrier_transaction != txid || self.execution_root != execution_root {
            return Err("protected admission readback names another carrier or execution".into());
        }
        if self.accepting_block == Hash64::default()
            || self.carrier_block == Hash64::default()
            || self.observed_tip == Hash64::default()
            || self.observed_tip_daa < self.accepted_daa
        {
            return Err("protected admission readback has no current inclusion point".into());
        }
        Ok(())
    }
}

/// None only when this snapshot has no claim or no protected frame. Corrupt or
/// mismatched stored links are errors, never a successful inclusion readback.
pub fn protected_original_readback_v1(state: &PalwChainStateV2, id: Hash64) -> Result<Option<ProtectedOriginalReadbackV1>, String> {
    let Some(claim) = state.claim(&id) else { return Ok(None) };
    let Some(frame) = state.public_tfp1_frame_v1(id).map_err(|e| format!("protected frame: {e:?}"))? else { return Ok(None) };
    crate::palw_state_v2::palw_protected_admission_reads_back_v1(state, &id, claim).map_err(|e| e.to_string())?;
    let tip = state.last_point().ok_or("protected admission has no committed tip")?;
    let origin = frame.origin_v1();
    let readback = ProtectedOriginalReadbackV1 {
        version: 1,
        claim: id,
        execution_root: claim.execution_root,
        original: Hash64::from_bytes(frame.original_id_v1().0.0),
        carrier_transaction: Hash64::from_bytes(origin.carrier_transaction.0),
        carrier_block: Hash64::from_bytes(origin.carrier_block.0),
        accepting_block: Hash64::from_bytes(origin.accepting_block.0),
        accepted_tx_ordinal: origin.accepted_tx_ordinal,
        event_ordinal: origin.event_ordinal,
        accepted_daa: origin.accepted_daa,
        accepted_blue_score: origin.accepted_blue_score,
        observed_tip: tip.block,
        observed_tip_daa: tip.daa_score,
        full_replay_approval_recorded: crate::palw_state_v2::included_replay::protected_replay_approval_recorded_v1(state, id),
        public_refutation_recorded: crate::palw_state_v2::palw_protected_refutation_recorded_v1(state, &id),
    };
    readback.check_carrier_v1(id, readback.carrier_transaction, claim.execution_root)?;
    Ok(Some(readback))
}

/// Read the protected original's own TIR header for a public verifier. This
/// frame has its own admission and service booking, independently of the
/// legacy staged ClaimBundle. Reading the header opens no court or authority.
#[allow(clippy::type_complexity)]
pub fn protected_filer_tir_view_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    maximum_leaves: u64,
) -> Result<
    Option<(crate::palw_decode_view_v1::PalwDecodeViewV1, Box<crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1>)>,
    String,
> {
    let Some(frame) = state.public_tfp1_frame_v1(claim).map_err(|e| format!("protected frame: {e:?}"))? else { return Ok(None) };
    let accepted = state.claim(&claim).ok_or("protected frame has no accepted claim")?;
    crate::palw_state_v2::palw_protected_admission_reads_back_v1(state, &claim, accepted).map_err(|e| e.to_string())?;
    let header = frame.header_v1();
    let record = state.tir_class_v1(&accepted.class_id).ok_or("protected TIR class record is absent")?;
    let view = crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(header, record, maximum_leaves)?;
    if view.committed_execution_root != accepted.execution_root
        || view.full_logits_trace_root != accepted.trace_root
        || view.step_leaf_count != accepted.work_leaves
        || view.class_id != accepted.class_id
    {
        return Err("protected TIR view differs from the accepted claim".into());
    }
    Ok(Some((view, Box::new(header.clone()))))
}
