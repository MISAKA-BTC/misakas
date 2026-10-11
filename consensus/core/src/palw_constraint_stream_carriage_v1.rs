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
pub const PALW_CONSTRAINT_STREAM_TAIL_V1: u8 = 0xF8;
/// Seed transport is independent of a legacy direct objective offence cap. It is still bounded
/// by the admitted court carrier ceiling, never a limit chosen by the challenger.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwAuxiliaryPublicCourtSeedV1 {
    /// The original registered table leaf; its complete bytes authenticate only at EOF.
    Tokenizer {
        id: u32,
        present: bool,
        length: u32,
        opening: PalwStepOpeningV1,
        raw_tokenizer_bytes: u64,
        tokenizer_key_events: u64,
        /// Verifier-held operand length; authenticated only by the original flat hash at EOF.
        operand_tokenizer_bytes: u64,
    },
    /// Must equal the outer rule-owned class step ladder; no prover-selected weakening.
    Prefix {
        maximum_leaves: u64,
        /// Fee metadata must equal the literal accepted prefix job and immutable relation.
        prefix_tokens: u32,
        relation: crate::palw_class_semantics_v1::PalwPrefixStateRelationV1,
    },
}
/// Content-addressed profile already retained by the claim's class certification. The byte
/// quote prices hydration; it cannot substitute a different graph for that historic class.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwRetainedCourtProfileV1 {
    pub class_id: Hash64,
    pub original_bytes: u64,
}
pub fn palw_retained_profile_skeleton_v1(profile: &crate::palw_step::PalwShapeProfileV3) -> crate::palw_step::PalwShapeProfileV3 {
    let mut result = profile.clone();
    for table in [&mut result.pre_nodes, &mut result.gdn_nodes, &mut result.attn_nodes, &mut result.post_nodes] {
        for node in table {
            node.weight_name.clear();
            node.weight_dtypes.clear();
        }
    }
    result.transcendental_bindings.clear();
    result.contraction_facts.clear();
    result
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwConstraintStreamSeedV1 {
    pub version: u16,
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub binding: PalwStepBindingV2,
    /// None reads the literal retained claim-rooted job; Some must equal that exact job.
    pub job: Option<Box<PalwFreePromptJobV3>>,
    pub pin: PalwTiledDecodePinV1,
    pub witness: Option<u32>,
    pub eog_count: u32,
    pub eog_opening: PalwStepOpeningV1,
    /// Prospective typed court selection. None preserves constrained-selection behavior.
    pub auxiliary: Option<PalwAuxiliaryPublicCourtSeedV1>,
    pub signature: Vec<u8>,
    pub retained_profile: Option<PalwRetainedCourtProfileV1>,
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
    TokenizerPacket(crate::palw_tokenizer_table_stream_v1::PalwTokenizerStreamPacketV1),
    TokenizerSeal,
    TokenizerRenderingPart { offset: u32, bytes: Vec<u8> },
    Prefix(crate::palw_prefix_family_court_v1::PalwPrefixCourtActionV1),
    PrefixPublicDemand(crate::palw_prefix_public_source_v1::PalwPrefixPublicDemandV1),
    PrefixPublicAnswer(crate::palw_prefix_public_source_v1::PalwPrefixPublicAnswerV1),
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
                &encode(&self.auxiliary),
                &encode(&self.retained_profile),
            ],
        )
    }
    /// Prepare a bounded reference before signing. Hydration still authenticates the literal
    /// retained chain profile and accepted claim independently; this helper grants no authority.
    pub fn use_retained_profile_v1(&mut self) -> Result<(), String> {
        if self.retained_profile.is_some() {
            return Err("seed already references an immutable profile".into());
        }
        let profile = &self.binding.shape_profile;
        let class_id = profile.shape_profile_id();
        if self.binding.job_context.shape_profile_id != class_id {
            return Err("seed profile is not the signed binding's declared class".into());
        }
        let original_bytes = borsh::to_vec(profile).map_err(|e| e.to_string())?.len() as u64;
        self.binding.shape_profile = palw_retained_profile_skeleton_v1(profile);
        self.retained_profile = Some(PalwRetainedCourtProfileV1 { class_id, original_bytes });
        Ok(())
    }
    pub fn resolved_binding_v1(
        &self,
        actual_class: Hash64,
        profile: Option<&crate::palw_step::PalwShapeProfileV3>,
    ) -> Result<PalwStepBindingV2, String> {
        let Some(reference) = &self.retained_profile else { return Ok(self.binding.clone()) };
        let profile = profile.ok_or("staged reference has no retained authenticated class profile")?;
        if reference.class_id != actual_class
            || self.binding.job_context.shape_profile_id != actual_class
            || profile.shape_profile_id() != actual_class
            || borsh::to_vec(profile).map_err(|e| e.to_string())?.len() as u64 != reference.original_bytes
            || self.binding.shape_profile != palw_retained_profile_skeleton_v1(profile)
        {
            return Err("staged profile reference changes accepted class identity, bytes or exact skeleton".into());
        }
        let mut result = self.binding.clone();
        result.shape_profile = profile.clone();
        Ok(result)
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
        let job = self.job.as_deref().ok_or("unrooted seed cannot resolve its public job reference")?;
        self.court_with_rooted_job(expected_claim_root, ladder, job)
    }
    pub fn court_with_rooted_job(
        &self,
        expected_claim_root: Hash64,
        ladder: u64,
        actual_job: &PalwFreePromptJobV3,
    ) -> Result<PalwConstraintStreamCourtV1, String> {
        if self.retained_profile.is_some() {
            return Err("unrooted helper cannot hydrate retained profile".into());
        }
        self.court_with_resolved_binding_v1(expected_claim_root, ladder, actual_job, &self.binding)
    }
    pub fn court_with_resolved_binding_v1(
        &self,
        expected_claim_root: Hash64,
        ladder: u64,
        actual_job: &PalwFreePromptJobV3,
        binding: &PalwStepBindingV2,
    ) -> Result<PalwConstraintStreamCourtV1, String> {
        let resolved = self.resolved_binding_v1(actual_job.class_id, Some(&binding.shape_profile))?;
        if &resolved != binding {
            return Err("hydration changes signed non-profile binding fields".into());
        }
        if self.job.as_deref().is_some_and(|j| j != actual_job) {
            return Err("stream seed changes literal retained job".into());
        }
        if self.auxiliary.is_some() {
            return Err("auxiliary seed is not a constrained-selection court".into());
        }
        PalwConstraintStreamCourtV1::new_with_seed_context_v1(
            self.claim,
            self.reserver,
            self.reserved_daa,
            expected_claim_root,
            binding,
            actual_job,
            &self.pin,
            self.witness,
            self.eog_count,
            &self.eog_opening,
            ladder,
            self.retained_profile.as_ref().map(|_| Hash64::from_bytes(self.signing_message().try_into().expect("seed digest width"))),
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
    /// Only this typed Answer changes signature authority. Every historic/owned action
    /// remains owner-signed, including public-source Demand and local prefix continuation.
    pub fn authorization_bond_v1(&self) -> PalwBondKeyV2 {
        match &self.action {
            PalwConstraintStreamActionV1::PrefixPublicAnswer(answer) => answer.answerer,
            _ => self.reserver,
        }
    }
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
        if let PalwConstraintStreamActionV1::PrefixPublicAnswer(answer) = &self.action {
            answer.check()?;
        }
        if matches!(
            &self.action,
            PalwConstraintStreamActionV1::PrefixPublicDemand(_) | PalwConstraintStreamActionV1::PrefixPublicAnswer(_)
        ) && self.phase != PalwConstraintStreamPhaseV1::PrefixCourt
        {
            return Err("public prefix acquisition changes typed prefix phase".into());
        }
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
            PalwConstraintStreamActionV1::TokenizerPacket(packet)
                if packet.bytes.len() > 65536
                    || packet.proofs.len() > 64
                    || borsh::to_vec(packet).map_err(|e| e.to_string())?.len() > 90000 =>
            {
                Err("tokenizer packet exceeds admitted bounded carrier".into())
            }
            PalwConstraintStreamActionV1::TokenizerRenderingPart { bytes, .. } if bytes.is_empty() || bytes.len() > 65536 => {
                Err("tokenizer rendering part exceeds bounded work".into())
            }
            PalwConstraintStreamActionV1::Prefix(action) if borsh::to_vec(action).map_err(|e| e.to_string())?.len() > 90000 => {
                Err("prefix action exceeds bounded staged carrier".into())
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
            PalwConstraintStreamActionV1::Prefix(_)
            | PalwConstraintStreamActionV1::PrefixPublicDemand(_)
            | PalwConstraintStreamActionV1::PrefixPublicAnswer(_)
            | PalwConstraintStreamActionV1::TokenizerPacket(_)
            | PalwConstraintStreamActionV1::TokenizerSeal
            | PalwConstraintStreamActionV1::TokenizerRenderingPart { .. } => {
                Err("typed auxiliary move is not a constrained court action".into())
            }
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
    fn public_prefix_answer_signer_is_typed_and_every_literal_identity_is_signed() {
        use crate::palw_prefix_public_source_v1::{PalwPrefixPublicAnswerActionV1, PalwPrefixPublicAnswerV1};
        let mut answer = movement();
        let signer = PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(99), index: 3 });
        answer.phase = PalwConstraintStreamPhaseV1::PrefixCourt;
        answer.action = PalwConstraintStreamActionV1::PrefixPublicAnswer(PalwPrefixPublicAnswerV1 {
            answerer: signer,
            cursor_id: Hash64::from_u64_word(11),
            demand_id: Hash64::from_u64_word(12),
            action: PalwPrefixPublicAnswerActionV1::Finish,
        });
        assert_eq!(answer.authorization_bond_v1(), signer);
        answer.check().unwrap();
        let signed = answer.signing_message();
        for mutation in 0..7 {
            let mut changed = answer.clone();
            match mutation {
                0 => changed.reserver = signer,
                1 => changed.claim = Hash64::from_u64_word(100),
                2 => changed.reserved_daa += 1,
                3 => changed.seed_id = Hash64::from_u64_word(100),
                4 => changed.phase = PalwConstraintStreamPhaseV1::Prefix(0),
                _ => {
                    let PalwConstraintStreamActionV1::PrefixPublicAnswer(a) = &mut changed.action else { unreachable!() };
                    if mutation == 5 { a.answerer = answer.reserver } else { a.demand_id = Hash64::from_u64_word(100) }
                }
            }
            assert_ne!(changed.signing_message(), signed);
        }
        let owner_move = movement();
        assert_eq!(owner_move.authorization_bond_v1(), owner_move.reserver);
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
    #[test]
    fn prefix_typed_carrier_binds_family_transition_and_bounds_hostile_body() {
        use crate::palw_prefix_family_court_v1::PalwPrefixCourtActionV1 as A;
        let mut m = movement();
        m.phase = PalwConstraintStreamPhaseV1::PrefixCourt;
        m.action = PalwConstraintStreamActionV1::Prefix(A::TilePart { offset: 0, bytes: vec![1, 0, 0, 0] });
        m.check().unwrap();
        let message = m.signing_message();
        let encoded = borsh::to_vec(&m).unwrap();
        let decoded: PalwConstraintStreamMoveV1 = borsh::from_slice(&encoded).unwrap();
        assert_eq!(decoded, m);
        let mut foreign = m.clone();
        foreign.phase = PalwConstraintStreamPhaseV1::Tokenizer;
        assert_ne!(message, foreign.signing_message());
        foreign = m.clone();
        foreign.action = PalwConstraintStreamActionV1::Prefix(A::FinishTile);
        assert_ne!(message, foreign.signing_message());
        foreign = m.clone();
        foreign.action = PalwConstraintStreamActionV1::Prefix(A::TilePart { offset: 4, bytes: vec![1, 0, 0, 0] });
        assert_ne!(message, foreign.signing_message());
        m.action = PalwConstraintStreamActionV1::Prefix(A::TilePart { offset: 0, bytes: vec![0; 90_001] });
        assert!(m.check().is_err());
    }
    #[test]
    fn registered_graph_variable_metadata_can_exceed_a_single_seed_carrier() {
        let (_, mut binding, _, _) = crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(false, false);
        let base = borsh::to_vec(&binding.shape_profile).unwrap().len();
        binding.shape_profile.transcendental_bindings = vec![
            crate::palw_step::PalwTranscendentalBindingV1 {
                site: crate::palw_step::PalwTranscendentalSiteV1::VectorExpPolynomial,
                algorithm_id: Hash64::from_u64_word(31),
            };
            1500
        ];
        // Registration's shape gate has no count ceiling here; the extra bindings do not
        // change node count/width or reachable kernels. This is a carrier regression witness,
        // not a claim that this synthetic registration has run through an actual service.
        binding.shape_profile.validate_shape().unwrap();
        let profile_bytes = borsh::to_vec(&binding.shape_profile).unwrap().len();
        assert_eq!(profile_bytes - base, 1500 * 65);
        let (_, a) = crate::palw_decode_constraint_v1::tests::public_constrained_case_for_tests_v1(false);
        let mut seed = PalwConstraintStreamSeedV1 {
            version: 1,
            claim: Hash64::from_u64_word(32),
            reserver: movement().reserver,
            reserved_daa: 40,
            binding,
            job: None,
            pin: a.pin,
            witness: None,
            eog_count: 0,
            eog_opening: a.eog.opening,
            auxiliary: Some(PalwAuxiliaryPublicCourtSeedV1::Prefix {
                maximum_leaves: 1 << 32,
                prefix_tokens: 2,
                relation: crate::palw_class_semantics_v1::PalwPrefixStateRelationV1::Base0Int8KvV1,
            }),
            signature: vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            retained_profile: None,
        };
        let seed_bytes = borsh::to_vec(&seed).unwrap().len();
        eprintln!("G14 full profile={profile_bytes} seed(job=None)={seed_bytes} carrier=90000");
        assert!(profile_bytes > 90_000);
        assert!(seed_bytes > 90_000);
        assert!(seed.check(90_000).is_err());
        let full = seed.binding.shape_profile.clone();
        let class = full.shape_profile_id();
        seed.binding.job_context.shape_profile_id = class;
        seed.binding.shape_profile = palw_retained_profile_skeleton_v1(&full);
        seed.retained_profile = Some(PalwRetainedCourtProfileV1 { class_id: class, original_bytes: profile_bytes as u64 });
        let compact_size = borsh::to_vec(&seed).unwrap().len();
        eprintln!("G14 referenced profile seed={compact_size} original_profile={profile_bytes}");
        assert!(compact_size < 90_000);
        seed.check(90_000).unwrap();
        let identity = seed.signing_message();
        let resolved = seed.resolved_binding_v1(class, Some(&full)).unwrap();
        assert_eq!(resolved.shape_profile, full);
        assert_eq!(seed.signing_message(), identity);
        assert!(seed.resolved_binding_v1(Hash64::default(), Some(&full)).is_err());
        assert!(seed.resolved_binding_v1(class, None).is_err());
        let mut false_quote = seed.clone();
        false_quote.retained_profile.as_mut().unwrap().original_bytes -= 1;
        assert!(false_quote.resolved_binding_v1(class, Some(&full)).is_err());
        assert_ne!(false_quote.signing_message(), identity);
        let mut false_stub = seed.clone();
        false_stub.binding.shape_profile.hidden_dim += 1;
        assert!(false_stub.resolved_binding_v1(class, Some(&full)).is_err());
        let mut later_profile = full.clone();
        later_profile.base0_rms_eps_q += 1;
        assert!(seed.resolved_binding_v1(class, Some(&later_profile)).is_err());
    }
}
