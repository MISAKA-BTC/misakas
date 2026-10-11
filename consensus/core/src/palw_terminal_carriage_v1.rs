//! Reservation-owned transport of objective evidence. The signed manifest authenticates bytes,
//! not a verdict: completion must run the same claim-bound objective court as a direct filing.
use crate::{Hash64, palw_offence_v1::PalwOffenceKindV1, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeMap;

pub const PALW_TERMINAL_PART_BYTES_V1: u32 = 90_000;
pub const PALW_TERMINAL_MAX_PARTS_V1: usize = 32;
pub const PALW_TERMINAL_MAX_BYTES_V1: u64 = PALW_TERMINAL_PART_BYTES_V1 as u64 * PALW_TERMINAL_MAX_PARTS_V1 as u64;
pub const PALW_TERMINAL_CONTEXT_V1: &[u8] = b"misaka-palw/terminal-evidence/v1/mldsa87";
pub const PALW_TERMINAL_TAIL_V1: u8 = 0xF6;
const MANIFEST_DOMAIN: &[u8] = b"misaka-palw/terminal-evidence/v1/manifest";
const PART_DOMAIN: &[u8] = b"misaka-palw/terminal-evidence/v1/part";
pub type PalwTerminalKeyV1 = (Hash64, PalwBondKeyV2);

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTerminalDeclarationV1 {
    pub version: u16,
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    /// Pins this particular live reservation, excluding replay after reorg/replacement.
    pub reserved_daa: u64,
    pub kind: PalwOffenceKindV1,
    pub accused: PalwBondKeyV2,
    pub evidence_id: Hash64,
    /// Whole-prompt recomputation bought by this declaration.
    pub heavy_prompt_ids: u32,
    pub total_bytes: u64,
    pub part_bytes: u32,
    pub part_digests: Vec<Hash64>,
    pub signature: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTerminalPartV1 {
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub evidence_id: Hash64,
    pub index: u8,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPendingTerminalV1 {
    pub declaration: PalwTerminalDeclarationV1,
    /// Fixed at acceptance to the already existing reservation's hard deadline.
    pub deadline_daa: u64,
    pub parts: BTreeMap<u8, Vec<u8>>,
}
/// Accepted-tip progress copies only the small immutable manifest and a 32-bit bitmap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTerminalProgressV1 {
    pub now_daa: u64,
    pub court_maximum: u64,
    pub reserved_daa: u64,
    pub hard_deadline_daa: u64,
    pub declaration: Option<PalwTerminalDeclarationV1>,
    pub accepted_parts: u32,
}

/// Extracts the claim without changing the offence's original byte identity. Callers bound bytes first.
pub fn palw_terminal_claim_v1(kind: PalwOffenceKindV1, evidence: &[u8]) -> Result<Hash64, String> {
    use crate::palw_offence_attribution_v1::*;
    match kind {
        PalwOffenceKindV1::ExecutorRefuted => borsh::from_slice::<PalwExecutorRefutedEvidenceV1>(evidence).map(|p| p.claim_id),
        PalwOffenceKindV1::PanelFalseValidV2 => borsh::from_slice::<PalwPanelFalseValidEvidenceV2>(evidence).map(|p| p.claim_id),
        PalwOffenceKindV1::TirIdentityMismatch => borsh::from_slice::<PalwTirIdentityEvidenceV1>(evidence).map(|p| p.claim_id),
        _ => return Err("this offence is not reservation-owned terminal evidence".into()),
    }
    .map_err(|e| e.to_string())
}

impl PalwTerminalDeclarationV1 {
    pub fn key(&self) -> PalwTerminalKeyV1 {
        (self.claim, self.reserver)
    }
    pub fn check(&self, maximum: u64) -> Result<(), String> {
        if self.version != 1
            || !matches!(
                self.kind,
                PalwOffenceKindV1::ExecutorRefuted | PalwOffenceKindV1::PanelFalseValidV2 | PalwOffenceKindV1::TirIdentityMismatch
            )
            || u64::from(self.heavy_prompt_ids) > crate::palw_attempt_rules_v1::PALW_HEAVY_PROMPT_IDS_PER_BLOCK_V1
            || self.part_bytes == 0
            || self.part_bytes > PALW_TERMINAL_PART_BYTES_V1
            || self.total_bytes == 0
            || self.total_bytes > maximum.min(PALW_TERMINAL_MAX_BYTES_V1)
            || self.part_digests.is_empty()
            || self.part_digests.len() > PALW_TERMINAL_MAX_PARTS_V1
            || self.total_bytes.div_ceil(self.part_bytes as u64) != self.part_digests.len() as u64
            || self.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
        {
            return Err("terminal declaration exceeds its exact envelope/part/type bounds".into());
        }
        Ok(())
    }
    pub fn check_window(&self, now: u64, hard_deadline: u64) -> Result<(), String> {
        if now.saturating_add(crate::palw_state_v2::PALW_COURT_CLOSE_INCLUSION_MARGIN.saturating_mul(self.part_digests.len() as u64))
            >= hard_deadline
        {
            return Err("terminal parts cannot fit the remaining reservation window".into());
        }
        Ok(())
    }
}
pub fn palw_terminal_message_v1(network: Hash64, d: &PalwTerminalDeclarationV1) -> Result<Hash64, String> {
    // Never copy a hostile signature or arbitrarily long digest vector to hash an unchecked declaration.
    d.check(PALW_TERMINAL_MAX_BYTES_V1)?;
    let mut h = blake2b_simd::Params::new().hash_length(64).key(MANIFEST_DOMAIN).to_state();
    h.update(network.as_byte_slice());
    let mut unsigned = d.clone();
    unsigned.signature.clear();
    h.update(&borsh::to_vec(&unsigned).map_err(|e| e.to_string())?);
    Ok(Hash64::from_bytes(h.finalize().as_bytes().try_into().expect("64-byte hash")))
}
pub fn palw_terminal_part_digest_v1(index: u8, bytes: &[u8]) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PART_DOMAIN).to_state();
    h.update(&[index]);
    h.update(&(bytes.len() as u64).to_le_bytes());
    h.update(bytes);
    Hash64::from_bytes(h.finalize().as_bytes().try_into().expect("64-byte hash"))
}
/// Counts evidence before allocating any parts or asking the caller to sign. The owner and
/// original offence identity survive transport unchanged.
pub fn palw_terminal_declaration_v1(
    mut d: PalwTerminalDeclarationV1,
    evidence: &[u8],
    maximum: u64,
    sign: impl FnOnce(&PalwTerminalDeclarationV1) -> Result<Vec<u8>, String>,
) -> Result<PalwTerminalDeclarationV1, String> {
    d.total_bytes = evidence.len() as u64;
    if d.part_bytes == 0
        || d.part_bytes > PALW_TERMINAL_PART_BYTES_V1
        || evidence.is_empty()
        || evidence.len() as u64 > maximum.min(PALW_TERMINAL_MAX_BYTES_V1)
        || evidence.len().div_ceil(d.part_bytes as usize) > PALW_TERMINAL_MAX_PARTS_V1
    {
        return Err("objective evidence exceeds bounded terminal carriage".into());
    }
    if crate::palw_offence_v1::palw_offence_evidence_digest_v1(evidence) != d.evidence_id {
        return Err("objective evidence differs from its original offence digest".into());
    }
    d.heavy_prompt_ids = palw_terminal_prompt_ids_v1(d.kind, evidence);
    d.part_digests =
        evidence.chunks(d.part_bytes as usize).enumerate().map(|(i, p)| palw_terminal_part_digest_v1(i as u8, p)).collect();
    // Exact fixed-size placeholder permits declaration hashing before the real signature exists.
    d.signature = vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN];
    d.check(maximum)?;
    d.signature = sign(&d)?;
    d.check(maximum)?;
    Ok(d)
}
pub fn palw_terminal_split_v1(
    d: PalwTerminalDeclarationV1,
    evidence: &[u8],
    maximum: u64,
    sign: impl FnOnce(&PalwTerminalDeclarationV1) -> Result<Vec<u8>, String>,
) -> Result<(PalwTerminalDeclarationV1, Vec<PalwTerminalPartV1>), String> {
    let d = palw_terminal_declaration_v1(d, evidence, maximum, sign)?;
    let parts = evidence
        .chunks(d.part_bytes as usize)
        .enumerate()
        .map(|(i, p)| PalwTerminalPartV1 {
            claim: d.claim,
            reserver: d.reserver,
            evidence_id: d.evidence_id,
            index: i as u8,
            bytes: p.to_vec(),
        })
        .collect();
    Ok((d, parts))
}
impl PalwPendingTerminalV1 {
    pub fn check_part(&self, p: &PalwTerminalPartV1) -> Result<(), String> {
        let d = &self.declaration;
        d.check(PALW_TERMINAL_MAX_BYTES_V1)?;
        if (p.claim, p.reserver) != d.key() || p.evidence_id != d.evidence_id || self.parts.contains_key(&p.index) {
            return Err("terminal part has another owner/digest or repeats an accepted part".into());
        }
        let expected = d.part_digests.get(p.index as usize).ok_or("terminal part index exceeds declaration")?;
        let first = u64::from(p.index) * u64::from(d.part_bytes);
        let len = d.total_bytes.checked_sub(first).ok_or("terminal part begins beyond evidence")?.min(d.part_bytes as u64);
        if p.bytes.len() as u64 != len || palw_terminal_part_digest_v1(p.index, &p.bytes) != *expected {
            return Err("terminal part bytes differ from the authenticated declaration".into());
        }
        Ok(())
    }
    pub fn check(&self, maximum: u64) -> Result<(), String> {
        self.declaration.check(maximum)?;
        if self.parts.len() >= self.declaration.part_digests.len() {
            return Err("completed terminal assembly was retained".into());
        }
        // Validate without cloning the retained bytes.
        for (&i, bytes) in &self.parts {
            let d = &self.declaration;
            let first = u64::from(i) * u64::from(d.part_bytes);
            let len = d.total_bytes.checked_sub(first).ok_or("retained terminal part exceeds declaration")?.min(d.part_bytes as u64);
            if bytes.len() as u64 != len || d.part_digests.get(i as usize) != Some(&palw_terminal_part_digest_v1(i, bytes)) {
                return Err("retained terminal part differs from declaration".into());
            }
        }
        Ok(())
    }
    /// At most one complete evidence allocation. Intermediate parts are never parsed or judged.
    pub fn preview(&self, p: &PalwTerminalPartV1, maximum: u64) -> Result<Option<Vec<u8>>, String> {
        self.check(maximum)?;
        self.check_part(p)?;
        if self.parts.len() + 1 != self.declaration.part_digests.len() {
            return Ok(None);
        }
        let mut bytes = Vec::with_capacity(self.declaration.total_bytes as usize);
        for i in 0..self.declaration.part_digests.len() as u8 {
            bytes.extend_from_slice(if i == p.index {
                &p.bytes
            } else {
                self.parts.get(&i).ok_or("terminal assembly missing a declared part")?
            });
        }
        if bytes.len() as u64 != self.declaration.total_bytes
            || crate::palw_offence_v1::palw_offence_evidence_digest_v1(&bytes) != self.declaration.evidence_id
        {
            return Err("assembled terminal bytes do not open the original evidence digest".into());
        }
        Ok(Some(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0))
    }
    fn split(bytes: &[u8], part: u32, maximum: u64) -> (PalwTerminalDeclarationV1, Vec<PalwTerminalPartV1>) {
        palw_terminal_split_v1(
            PalwTerminalDeclarationV1 {
                version: 1,
                claim: Hash64::from_u64_word(4),
                reserver: bond(5),
                reserved_daa: 10,
                kind: PalwOffenceKindV1::ExecutorRefuted,
                accused: bond(6),
                evidence_id: crate::palw_offence_v1::palw_offence_evidence_digest_v1(bytes),
                heavy_prompt_ids: 0,
                total_bytes: 0,
                part_bytes: part,
                part_digests: vec![],
                signature: vec![],
            },
            bytes,
            maximum,
            |_| Ok(vec![9; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN]),
        )
        .unwrap()
    }
    #[test]
    fn exact_maximum_reverse_arrival_preserves_original_evidence_identity() {
        let bytes = vec![17; PALW_TERMINAL_MAX_BYTES_V1 as usize];
        let (d, parts) = split(&bytes, PALW_TERMINAL_PART_BYTES_V1, PALW_TERMINAL_MAX_BYTES_V1);
        assert_eq!(parts.len(), 32);
        assert!(borsh::object_length(&d).unwrap() < 100_000);
        let mut pending = PalwPendingTerminalV1 { declaration: d, deadline_daa: 1_000, parts: BTreeMap::new() };
        for p in parts.iter().rev() {
            assert!(borsh::object_length(p).unwrap() + 1 <= 100_000);
            if p.index == 0 {
                assert_eq!(pending.preview(p, PALW_TERMINAL_MAX_BYTES_V1).unwrap(), Some(bytes.clone()));
            } else {
                assert!(pending.preview(p, PALW_TERMINAL_MAX_BYTES_V1).unwrap().is_none());
                pending.parts.insert(p.index, p.bytes.clone());
            }
        }
        assert!(pending.preview(&parts[0], PALW_TERMINAL_MAX_BYTES_V1 - 1).is_err());
    }
    #[test]
    fn owner_digest_index_duplicate_and_corruption_cannot_complete() {
        let (d, parts) = split(&vec![13; 180_001], 90_000, PALW_TERMINAL_MAX_BYTES_V1);
        let mut row = PalwPendingTerminalV1 { declaration: d, deadline_daa: 100, parts: BTreeMap::new() };
        row.parts.insert(0, parts[0].bytes.clone());
        assert!(row.preview(&parts[0], PALW_TERMINAL_MAX_BYTES_V1).is_err());
        for mut p in
            [parts[1].clone(), parts[1].clone(), parts[1].clone(), parts[1].clone()].into_iter().enumerate().map(|(i, mut p)| {
                match i {
                    0 => p.reserver = bond(9),
                    1 => p.evidence_id = Hash64::default(),
                    2 => p.index = 31,
                    _ => p.bytes[0] ^= 1,
                }
                p
            })
        {
            assert!(row.preview(&p, PALW_TERMINAL_MAX_BYTES_V1).is_err());
            p.bytes.clear();
        }
        row.parts.insert(1, parts[1].bytes.clone());
        let saved = borsh::to_vec(&row).unwrap();
        let restarted: PalwPendingTerminalV1 = borsh::from_slice(&saved).unwrap();
        assert_eq!(restarted.preview(&parts[2], PALW_TERMINAL_MAX_BYTES_V1).unwrap(), Some(vec![13; 180_001]));
        row.parts.get_mut(&1).unwrap()[0] ^= 1;
        assert!(row.check(PALW_TERMINAL_MAX_BYTES_V1).is_err());
    }
    #[test]
    fn exact_reservation_window_has_no_extension_or_overflow() {
        let (d, _) = split(&vec![4; 180_001], 90_000, PALW_TERMINAL_MAX_BYTES_V1);
        let margin = crate::palw_state_v2::PALW_COURT_CLOSE_INCLUSION_MARGIN * 3;
        assert!(d.check_window(10, 10 + margin).is_err());
        assert!(d.check_window(10, 11 + margin).is_ok());
        assert!(d.check_window(u64::MAX - 1, u64::MAX).is_err());
    }
    #[test]
    fn signature_message_binds_network_reservation_accused_and_original_digest() {
        let (d, _) = split(&vec![4; 100_001], 90_000, PALW_TERMINAL_MAX_BYTES_V1);
        let net = Hash64::from_u64_word(1);
        let m = palw_terminal_message_v1(net, &d).unwrap();
        assert_ne!(m, palw_terminal_message_v1(Hash64::from_u64_word(2), &d).unwrap());
        for change in 0..6 {
            let mut other = d.clone();
            match change {
                0 => other.claim = Hash64::default(),
                1 => other.reserver = bond(8),
                2 => other.reserved_daa += 1,
                3 => other.accused = bond(8),
                4 => other.evidence_id = Hash64::default(),
                _ => other.kind = PalwOffenceKindV1::PanelFalseValidV2,
            };
            assert_ne!(m, palw_terminal_message_v1(net, &other).unwrap());
        }
        let mut malformed = d;
        malformed.part_digests.resize(33, Hash64::default());
        assert!(palw_terminal_message_v1(net, &malformed).is_err());
    }
    #[test]
    fn oversized_proof_is_refused_before_signing_or_parts_allocation() {
        let bytes = vec![4; 100_001];
        let (d, _) = split(&bytes, 90_000, PALW_TERMINAL_MAX_BYTES_V1);
        assert!(palw_terminal_split_v1(d.clone(), &bytes, 100_000, |_| panic!("must reject before signing")).is_err());
        let mut unsupported = d;
        unsupported.kind = PalwOffenceKindV1::DaDefault;
        assert!(
            palw_terminal_split_v1(unsupported, &bytes, PALW_TERMINAL_MAX_BYTES_V1, |_| panic!(
                "must reject unfileable kind before signing"
            ))
            .is_err()
        );
    }
}

/// The assembled proof still observes the existing constraint fence; a chunk cannot bypass it.
pub fn palw_terminal_is_constraint_v1(kind: PalwOffenceKindV1, evidence: &[u8]) -> bool {
    use crate::palw_offence_attribution_v1::{PalwExecutorRefutedEvidenceV1, PalwPanelFalseValidEvidenceV2};
    use crate::palw_offence_v1::PalwPanelContradictionV1 as C;
    let c = match kind {
        PalwOffenceKindV1::ExecutorRefuted => {
            borsh::from_slice::<PalwExecutorRefutedEvidenceV1>(evidence).ok().map(|p| p.contradiction)
        }
        PalwOffenceKindV1::PanelFalseValidV2 => {
            borsh::from_slice::<PalwPanelFalseValidEvidenceV2>(evidence).ok().map(|p| p.contradiction)
        }
        _ => None,
    };
    matches!(c, Some(C::ConstrainedDecode { .. } | C::ConstrainedDecodeFlat { .. }))
}

/// Runs the ordinary court for an already authenticated assembly. The manifest's claim is
/// compared to the adjudicator's target rather than trusting an opaque bytes hash as attribution.
#[allow(clippy::too_many_arguments)]
pub fn palw_terminal_adjudicate_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    d: &PalwTerminalDeclarationV1,
    evidence: &[u8],
    court_maximum: u64,
    fp_decode_rules: bool,
    rules: crate::palw_offence_attribution_v1::PalwIdentityRulesV1,
    sessions: crate::palw_offence_attribution_v1::PalwSessionRuleV1,
    sig: Option<crate::palw_offence_attribution_v1::PalwFalseValidSigCheckV1<'_>>,
) -> Result<Hash64, String> {
    use crate::palw_offence_attribution_v1 as a;
    d.check(court_maximum)?;
    if evidence.len() as u64 != d.total_bytes || crate::palw_offence_v1::palw_offence_evidence_digest_v1(evidence) != d.evidence_id {
        return Err("terminal proof does not open its original evidence identity".into());
    }
    let budget = a::PalwObjectiveEvidenceBudgetV1::ReservedTerminal { court_maximum };
    if let Some(charge) =
        a::palw_offence_heavy_prompt_charge_at_v2_with_budget_v1(state, &d.accused, d.kind, evidence, rules, true, sessions, budget)
    {
        if charge.prompt_ids > u64::from(d.heavy_prompt_ids) {
            return Err("terminal declaration did not buy its whole-prompt recomputation".into());
        }
    }

    let (claim, offence) = match d.kind {
        PalwOffenceKindV1::ExecutorRefuted => {
            let f = a::palw_check_executor_refuted_at_v1_with_budget_v1(
                state,
                &d.accused,
                evidence,
                fp_decode_rules,
                false,
                rules,
                sessions,
                budget,
            )
            .map_err(|e| e.to_string())?;
            (f.target.claim_id, a::palw_executor_refuted_offence_id_v1(&d.accused.0, &f.target.claim_id))
        }
        PalwOffenceKindV1::PanelFalseValidV2 => {
            let f = a::palw_check_panel_false_valid_at_v2_with_budget_v1(
                state,
                &d.accused,
                evidence,
                fp_decode_rules,
                false,
                rules,
                sig,
                sessions,
                budget,
            )
            .map_err(|e| e.to_string())?;
            (f.target.claim_id, a::palw_false_valid_offence_id_v2(&d.accused.0, &f.target.claim_id))
        }
        PalwOffenceKindV1::TirIdentityMismatch => {
            let f =
                a::palw_check_tir_identity_mismatch_at_v1_with_budget_v1(state, &d.accused, evidence, false, rules, sessions, budget)
                    .map_err(|e| e.to_string())?;
            (f.target.claim_id, a::palw_tir_identity_offence_id_v1(&d.accused.0, &f.target.claim_id))
        }
        _ => return Err("terminal offence kind cannot be filed".into()),
    };
    if claim != d.claim {
        return Err("adjudicated terminal target is another claim".into());
    }
    if state.consumed_offence(&offence).is_some() {
        return Err("terminal offence was already consumed".into());
    }
    Ok(offence)
}

fn palw_terminal_prompt_ids_v1(kind: PalwOffenceKindV1, evidence: &[u8]) -> u32 {
    use crate::palw_offence_attribution_v1::{PalwExecutorRefutedEvidenceV1, PalwPanelFalseValidEvidenceV2};
    use crate::palw_offence_v1::{PalwPanelContradictionV1 as C, PalwPromptProofV1 as P};
    let c = match kind {
        PalwOffenceKindV1::ExecutorRefuted => {
            borsh::from_slice::<PalwExecutorRefutedEvidenceV1>(evidence).ok().map(|p| p.contradiction)
        }
        PalwOffenceKindV1::PanelFalseValidV2 => {
            borsh::from_slice::<PalwPanelFalseValidEvidenceV2>(evidence).ok().map(|p| p.contradiction)
        }
        _ => None,
    };
    match c {
        Some(C::PromptNotAnchored { binding, proof: P::Whole | P::FpCanonicalWhole { .. } }) => {
            binding.job_context.declared_prefill_tokens
        }
        _ => 0,
    }
}
