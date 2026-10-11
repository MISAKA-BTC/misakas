//! Signed prospective staged court moves. No unsigned byte may poison an owner's legacy leaf
//! continuation: intermediate chunks authenticate the owner even before the full leaf completes.
use crate::Hash64;
use crate::palw_constraint_stream_court_v1::{PalwConstraintStreamCourtV1, PalwConstraintStreamPhaseV1};
use crate::palw_freeprompt_v3::PalwFreePromptJobV3;
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_step_leg::{PalwStepBindingV2, PalwStepOpeningV1};
use crate::palw_step_refute::PalwTiledDecodePinV1;
use borsh::{BorshDeserialize, BorshSerialize};
pub const PALW_CONSTRAINT_STREAM_CONTEXT_V1: &[u8] = b"misaka-palw/constraint-stream/v1/mldsa87";
pub const PALW_CONSTRAINT_STREAM_TAIL_V1: u8 = 0xF4;
/// Seed transport is independent of a legacy direct objective offence cap. It is still bounded
/// by the admitted court carrier ceiling, never a limit chosen by the challenger.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwConstraintStreamSeedV1 {
    pub version: u16,
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub binding: PalwStepBindingV2,
    pub job: PalwFreePromptJobV3,
    pub pin: PalwTiledDecodePinV1,
    pub witness: Option<u32>,
    pub eog_count: u32,
    pub eog_opening: PalwStepOpeningV1,
    pub signature: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwConstraintStreamLeafV1 {
    pub version: u16,
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub seed_id: Hash64,
    pub id: u32,
    pub present: bool,
    pub length: u32,
    pub opening: PalwStepOpeningV1,
    pub signature: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwConstraintStreamActionV1 {
    EogPart { offset: u32, ids: Vec<u32> },
    EogFinish,
    RenderingPart { offset: u32, bytes: Vec<u8> },
    RenderingFinish,
    EogTokenFinish,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwConstraintStreamMoveV1 {
    pub version: u16,
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub seed_id: Hash64,
    /// Explicit phase prevents replaying an equal-offset chunk for another token occurrence.
    pub phase: PalwConstraintStreamPhaseV1,
    pub action: PalwConstraintStreamActionV1,
    pub signature: Vec<u8>,
}
fn message(label: &[u8], fields: &[&[u8]]) -> Vec<u8> {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/constraint-stream/move/v1").to_state();
    h.update(&(label.len() as u32).to_le_bytes());
    h.update(label);
    for field in fields {
        h.update(&(field.len() as u64).to_le_bytes());
        h.update(field);
    }
    h.finalize().as_bytes().to_vec()
}
fn encode<T: BorshSerialize>(t: &T) -> Vec<u8> {
    borsh::to_vec(t).expect("stream types serialize")
}
fn check_signature_shape(version: u16, signature: &[u8]) -> Result<(), String> {
    if version != 1 || signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
        return Err("stream move version/signature is outside fixed owner format".into());
    }
    Ok(())
}
impl PalwConstraintStreamSeedV1 {
    pub fn signing_message(&self) -> Vec<u8> {
        message(
            b"seed",
            &[
                &encode(&self.version),
                &encode(&self.claim),
                &encode(&self.reserver),
                &encode(&self.reserved_daa),
                &encode(&self.binding),
                &encode(&self.job),
                &encode(&self.pin),
                &encode(&self.witness),
                &encode(&self.eog_count),
                &encode(&self.eog_opening),
            ],
        )
    }
    pub fn check(&self, carrier_maximum: u64) -> Result<(), String> {
        check_signature_shape(self.version, &self.signature)?;
        struct Counter(u64);
        impl std::io::Write for Counter {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0 = self.0.checked_add(b.len() as u64).ok_or(std::io::ErrorKind::InvalidData)?;
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut n = Counter(0);
        self.serialize(&mut n).map_err(|e| e.to_string())?;
        if n.0 > carrier_maximum {
            return Err("stream seed exceeds admitted bounded carrier".into());
        }
        Ok(())
    }
    pub fn court(&self, expected_claim_root: Hash64, ladder: u64) -> Result<PalwConstraintStreamCourtV1, String> {
        PalwConstraintStreamCourtV1::new(
            self.claim,
            self.reserver,
            self.reserved_daa,
            expected_claim_root,
            &self.binding,
            &self.job,
            &self.pin,
            self.witness,
            self.eog_count,
            &self.eog_opening,
            ladder,
        )
    }
}
impl PalwConstraintStreamLeafV1 {
    pub fn signing_message(&self) -> Vec<u8> {
        message(
            b"leaf",
            &[
                &encode(&self.version),
                &encode(&self.claim),
                &encode(&self.reserver),
                &encode(&self.reserved_daa),
                &encode(&self.seed_id),
                &encode(&self.id),
                &encode(&self.present),
                &encode(&self.length),
                &encode(&self.opening),
            ],
        )
    }
    pub fn check(&self) -> Result<(), String> {
        check_signature_shape(self.version, &self.signature)?;
        if self.opening.siblings.len() > 23 {
            return Err("stream leaf opening exceeds global table proof depth".into());
        }
        Ok(())
    }
}
impl PalwConstraintStreamMoveV1 {
    pub fn signing_message(&self) -> Vec<u8> {
        message(
            b"move",
            &[
                &encode(&self.version),
                &encode(&self.claim),
                &encode(&self.reserver),
                &encode(&self.reserved_daa),
                &encode(&self.seed_id),
                &encode(&self.phase),
                &encode(&self.action),
            ],
        )
    }
    pub fn check(&self) -> Result<(), String> {
        check_signature_shape(self.version, &self.signature)?;
        match &self.action {
            PalwConstraintStreamActionV1::EogPart { ids, .. }
                if ids.is_empty() || ids.len() > crate::palw_eog_stream_v1::PALW_EOG_STREAM_PART_IDS_V1 =>
            {
                Err("stream EOG move exceeds fixed bounded work".into())
            }
            PalwConstraintStreamActionV1::RenderingPart { bytes, .. }
                if bytes.is_empty() || bytes.len() > crate::palw_rendering_stream_v1::PALW_RENDERING_STREAM_PART_BYTES_V1 =>
            {
                Err("stream rendering move exceeds fixed bounded work".into())
            }
            _ => Ok(()),
        }
    }
    /// Acceptance checks the owner signature AND actual rooted claim/reservation BEFORE this.
    pub fn apply_to(&self, court: &mut PalwConstraintStreamCourtV1) -> Result<(), String> {
        self.check()?;
        if self.seed_id != court.seed_id() || self.phase != court.phase() {
            return Err("stream move changes rooted seed/token phase".into());
        }
        match &self.action {
            PalwConstraintStreamActionV1::EogPart { offset, ids } => court.push_eog(self.seed_id, *offset, ids),
            PalwConstraintStreamActionV1::EogFinish => court.finish_eog(),
            PalwConstraintStreamActionV1::RenderingPart { offset, bytes } => court.push_rendering(self.seed_id, *offset, bytes),
            PalwConstraintStreamActionV1::RenderingFinish => court.finish_rendering(),
            PalwConstraintStreamActionV1::EogTokenFinish => court.finish_eog_token(self.seed_id),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{TransactionId, TransactionOutpoint};
    fn movement() -> PalwConstraintStreamMoveV1 {
        PalwConstraintStreamMoveV1 {
            version: 1,
            claim: Hash64::from_u64_word(1),
            reserver: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(2), index: 3 }),
            reserved_daa: 40,
            seed_id: Hash64::from_u64_word(5),
            phase: PalwConstraintStreamPhaseV1::Prefix(0),
            action: PalwConstraintStreamActionV1::RenderingPart { offset: 0, bytes: vec![b'a'] },
            signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        }
    }
    #[test]
    fn signed_move_binds_literal_owner_claim_reservation_phase_offset_and_bytes() {
        let m = movement();
        m.check().unwrap();
        let signed = m.signing_message();
        let mut mutations = Vec::new();
        let mut x = m.clone();
        x.claim = Hash64::default();
        mutations.push(x);
        let mut x = m.clone();
        x.reserver.0.index += 1;
        mutations.push(x);
        let mut x = m.clone();
        x.reserved_daa += 1;
        mutations.push(x);
        let mut x = m.clone();
        x.phase = PalwConstraintStreamPhaseV1::Prefix(1);
        mutations.push(x);
        let mut x = m.clone();
        x.action = PalwConstraintStreamActionV1::RenderingPart { offset: 1, bytes: vec![b'a'] };
        mutations.push(x);
        let mut x = m.clone();
        x.action = PalwConstraintStreamActionV1::RenderingPart { offset: 0, bytes: vec![b'x'] };
        mutations.push(x);
        for changed in mutations {
            assert_ne!(changed.signing_message(), signed);
        }
        let mut wrong = m.clone();
        wrong.signature.pop();
        assert!(wrong.check().is_err());
        wrong = m;
        wrong.action = PalwConstraintStreamActionV1::RenderingPart { offset: 0, bytes: vec![0; 65537] };
        assert!(wrong.check().is_err());
    }
}
