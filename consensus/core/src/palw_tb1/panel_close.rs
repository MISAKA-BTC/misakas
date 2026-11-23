//! Canonical QF1 PANEL close commitment. Population eligibility and material
//! reservations belong to the owning policy; this reader binds its inputs to
//! the admitted Originals and the prepared round on the selected branch.
use super::{freeze::Tb1MiningClosure, qf1_original_carrier::qf1_marker};
use crate::{
    Hash64,
    block::Block,
    palw_state_v2::{PalwChainStateV2, Qf1PanelOriginal},
};
use sha3::{Digest, Sha3_512};
use std::io;

fn invalid(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}

/// The historical local PANEL encoding, shared by the holder and native
/// validation. The declared row count and slot order are part of the hash.
pub struct PanelConsumerCommitment {
    hash: Sha3_512,
    remaining: u32,
    last_slot: Option<u64>,
}

impl PanelConsumerCommitment {
    pub fn new(exec_inputs: &[u8; 64], close: Hash64, certificate: Hash64, seed: &[u8; 32], rows: usize) -> io::Result<Self> {
        let remaining = u32::try_from(rows).map_err(|_| invalid("PANEL submissions count"))?;
        let hash = Sha3_512::new()
            .chain_update(b"MISAKA/private/QF1/local-panel-inputs/v1")
            .chain_update(exec_inputs)
            .chain_update(close.as_bytes())
            .chain_update(certificate.as_bytes())
            .chain_update(seed)
            .chain_update(remaining.to_be_bytes());
        Ok(Self { hash, remaining, last_slot: None })
    }

    pub fn push(&mut self, slot: u64, original: Hash64, row: &Qf1PanelOriginal, material: Hash64) -> io::Result<()> {
        if self.remaining == 0 || self.last_slot.is_some_and(|prior| prior >= slot) {
            return Err(invalid("PANEL submission outside the declared slot order"));
        }
        self.hash.update(slot.to_be_bytes());
        for field in [row.registration, original, row.original_block, row.funding_ref, material] {
            self.hash.update(field.as_bytes());
        }
        self.hash.update(row.deadline.to_be_bytes());
        self.remaining -= 1;
        self.last_slot = Some(slot);
        Ok(())
    }

    pub fn finish(mut self, candidates: &[(Hash64, u64)]) -> io::Result<[u8; 64]> {
        if self.remaining != 0 {
            return Err(invalid("incomplete PANEL submission set"));
        }
        self.hash.update(u32::try_from(candidates.len()).map_err(|_| invalid("PANEL pool count"))?.to_be_bytes());
        let mut prior = None;
        for &(actor, weight) in candidates {
            if weight == 0 || prior.is_some_and(|id| id >= actor) {
                return Err(invalid("invalid local PANEL pool order or weight"));
            }
            self.hash.update(actor.as_bytes());
            self.hash.update(weight.to_be_bytes());
            prior = Some(actor);
        }
        Ok(self.hash.finalize().into())
    }
}

/// Inputs resolved by the PANEL policy at the closing parent. Material
/// reservations are keyed by Original in the same order as the indexed intake.
pub struct PanelCloseInputs<'a> {
    pub materials: &'a [(Hash64, Hash64)],
    pub candidates: &'a [(Hash64, u64)],
    pub draw_policy: Hash64,
}

/// Check a proposed close against the complete indexed intake, the accepted
/// EXEC freeze, and the owner-resolved PANEL policy. The caller supplies the
/// candidate's actual selected parent.
pub fn verify_indexed_panel_close(
    parent: Hash64,
    state: &PalwChainStateV2,
    candidate: &Block,
    exec_close: Hash64,
    exec_closure: &Tb1MiningClosure,
    inputs: PanelCloseInputs<'_>,
) -> io::Result<Tb1MiningClosure> {
    let accepted = state.tb1_exec_close_receipt(exec_close).ok_or_else(|| invalid("missing PANEL cut's EXEC close"))?;
    let certificate = state.tb1_exec_certificate(exec_close).ok_or_else(|| invalid("missing PANEL cut's EXEC certificate"))?;
    let (exec, panel) =
        state.tb1_exec_panel_preparation(accepted.preparation, parent).ok_or_else(|| invalid("missing PANEL cut's preparation"))?;
    if exec_closure.commitment(accepted.selected_parent) != accepted.freeze_id
        || exec_closure.round.id() != exec.round().id()
        || exec_closure.qual_root != exec.qualification().root()
    {
        return Err(invalid("PANEL cut differs from the accepted EXEC freeze"));
    }
    let round = panel.round();
    if !candidate.header.direct_parents().contains(&parent)
        || candidate.header.blue_score != round.close_blue_score
        || candidate.header.daa_score >= round.release_from_daa
    {
        return Err(invalid("PANEL close outside its prepared schedule"));
    }
    let mut intake = state.qf1_panel_intake(exec_close, parent).ok_or_else(|| invalid("missing indexed PANEL intake"))?;
    let mut commitment = PanelConsumerCommitment::new(
        &exec_closure.consumer_set_root,
        exec_close,
        certificate.certificate(),
        &certificate.seed,
        inputs.materials.len(),
    )?;
    for &(expected_original, material) in inputs.materials {
        let (slot, original, row) = intake
            .next()
            .ok_or_else(|| invalid("extra PANEL submission"))?
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if original != expected_original {
            return Err(invalid("PANEL material reservation names a different Original"));
        }
        commitment.push(slot, original, &row, material)?;
    }
    if intake.next().is_some() {
        return Err(invalid("PANEL close omitted an admitted Original"));
    }
    let closure = Tb1MiningClosure {
        round,
        qual_root: panel.qualification().root(),
        consumer_set_root: commitment.finish(inputs.candidates)?,
        policy_root: inputs.draw_policy.as_bytes(),
    };
    if qf1_marker(candidate) != Some(closure.commitment(parent)) {
        return Err(invalid("PANEL close differs from the branch intake and policy"));
    }
    Ok(closure)
}
