//! Protected mixed-court candidate statements. Explicit v7 class, scope, replay
//! and seal namespaces; shared bounded vector capital/catalog with prior routes.
use crate::palw_flat_model_vector_v3::*;
use crate::palw_model_vector_v2::{ModelVectorPostV2, decode_model_vector_post_v2};
use crate::{Hash64, palw_kernel_route_v1::PalwKernelRouteStateV1, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_court5::FlatSegContextV5,
    flat_court7::FlatContextV7,
    flat_replay7::FlatReplayEnvelopeV7,
    flat_scope7::FlatResourceScopeV7,
    hash::{Digest, object_id},
    ledger::flat_candidate7::FlatCandidateRecordV7,
    seg_ledger::SegClaimViewV1,
};
use std::io::{Cursor, Error, ErrorKind};
pub const FLAT_PROTECTED_TABLE_V7: u8 = 69;
pub const FLAT_SEAL_TABLE_V7: u8 = 70;
pub const FLAT_SEAL_ACTOR_TABLE_V7: u8 = 71;
pub const FLAT_SEAL_COUNT_TABLE_V7: u8 = 72;
pub const FLAT_NATIVE_SEAL_MATURITY_DAA_V7: u64 = 3;
pub const FLAT_NATIVE_SEAL_TTL_DAA_V7: u64 = 100;
pub const FLAT_NATIVE_SEALS_PER_BOND_V7: usize = 4;
pub const FLAT_NATIVE_SEALS_MAX_V7: u32 = 1024;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatLengthBindingV7 {
    pub template: Vec<u8>,
    pub admission: misaka_palw_kernel::flat_lengths7::FlatLengthAdmissionV7,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatModelVectorPostV7 {
    pub version: u16,
    pub profile: Digest,
    pub court_set: Digest,
    pub scope_root: Digest,
    pub replay_root: Digest,
    pub lengths: Option<FlatLengthBindingV7>,
    pub trace: ModelVectorPostV2,
}
impl FlatModelVectorPostV7 {
    pub fn trace_root(&self, class: Digest, binding: Digest) -> Digest {
        object_id(
            b"misaka-palw/flat-model-vector/protected-trace/v7",
            &(
                self.version,
                self.profile,
                self.court_set,
                self.scope_root,
                self.replay_root,
                &self.lengths,
                class,
                binding,
                self.trace.trace_root(class, binding),
                FLAT_NATIVE_SEAL_MATURITY_DAA_V7,
                FLAT_NATIVE_SEAL_TTL_DAA_V7,
                FLAT_NATIVE_SEALS_PER_BOND_V7 as u32,
                FLAT_NATIVE_SEALS_MAX_V7,
            ),
        )
    }
    pub fn id(&self, class: Digest, binding: Digest) -> Hash64 {
        vector_id(self.trace_root(class, binding))
    }
}
pub fn decode_flat_model_vector_post_v7(bytes: &[u8]) -> std::io::Result<FlatModelVectorPostV7> {
    if bytes.len() < 258 || bytes.len() > FLAT_MODEL_VECTOR_MAX_POST_BYTES_V3 || bytes[..2] != 7u16.to_le_bytes() {
        return Err(Error::new(ErrorKind::InvalidData, "invalid bounded mixed-court post7"));
    }
    let mut r = Cursor::new(bytes);
    let version = u16::deserialize_reader(&mut r)?;
    let profile = Digest::deserialize_reader(&mut r)?;
    let court_set = Digest::deserialize_reader(&mut r)?;
    let scope_root = Digest::deserialize_reader(&mut r)?;
    let replay_root = Digest::deserialize_reader(&mut r)?;
    let lengths = match u8::deserialize_reader(&mut r)? {
        0 => None,
        1 => {
            let n = u32::deserialize_reader(&mut r)? as usize;
            let start = r.position() as usize;
            if n > misaka_palw_kernel::flat_lengths7::FLAT_LENGTH_MAX_WIRE_BYTES_V7 || n > bytes.len().saturating_sub(start) {
                return Err(Error::new(ErrorKind::InvalidData, "bounded length template count"));
            }
            let template = bytes[start..start + n].to_vec();
            r.set_position((start + n) as u64);
            let admission = misaka_palw_kernel::flat_lengths7::FlatLengthAdmissionV7::deserialize_reader(&mut r)?;
            Some(FlatLengthBindingV7 { template, admission })
        }
        _ => return Err(Error::new(ErrorKind::InvalidData, "bounded length option")),
    };
    let trace = decode_model_vector_post_v2(&bytes[r.position() as usize..])?;
    Ok(FlatModelVectorPostV7 { version, profile, court_set, scope_root, replay_root, lengths, trace })
}
pub struct FlatModelVectorViewV7 {
    pub profile: Digest,
    pub scope: FlatResourceScopeV7,
    pub replay: FlatReplayEnvelopeV7,
    pub trace: SegClaimViewV1,
}
impl FlatModelVectorViewV7 {
    pub fn context(&self) -> FlatContextV7<'_> {
        FlatContextV7 { scope: &self.scope, base: FlatSegContextV5 { profile: self.profile, claim: self.trace.context() } }
    }
}
pub(crate) fn checked_flat_model_vector_v7(
    route: &PalwKernelRouteStateV1,
    class: Hash64,
    binding: Hash64,
    post: &FlatModelVectorPostV7,
) -> Result<FlatModelVectorViewV7, String> {
    use misaka_palw_kernel::{descriptor::*, evidence::*, seg::*, seg_ledger::*};
    let record = route.flat_candidate_record_v7(&class).ok_or("no explicit mixed-court candidate")?;
    let statement = route.model_artifact_binding_header_v2(&binding).ok_or("no mixed-court scoped model")?;
    if record.binding != binding.as_bytes()
        || record.plan.descriptor != statement.descriptor
        || record.plan.program_root != statement.program_root
        || record.param_commitments.root() != statement.kernel_param_root.as_bytes()
        || route.model_artifact_candidate_binding_v2(&class) != Some(binding)
    {
        return Err("mixed candidate is not this exact authenticated model scope".into());
    }
    let v = record.checked()?;
    let d = [k2_tir_v4_descriptor(), k2_tir_v5_descriptor()]
        .into_iter()
        .find(|d| d.digest() == record.plan.descriptor)
        .ok_or("unknown mixed-court job role")?;
    let header = EvidenceHeaderV1 {
        network_domain: route.header.policy.network_domain,
        ruleset_digest: route.header.policy.ruleset_digest,
        class_binding_id: class.as_bytes(),
        program_root: statement.program_root,
        artifact_root: statement.kernel_param_root.as_bytes(),
        plan_root: record.plan.root(),
    };
    let t = &post.trace;
    if post.version != 7
        || post.profile != v.scope.profile
        || post.court_set != v.scope.court_set
        || post.scope_root != v.scope.root()
        || post.replay_root != v.replay.root()
        || t.version != 2
        || t.evidence.header != header
        || t.evidence.suite != SuiteParamsV1::of(&d)
        || t.prompt.is_empty()
        || t.prompt.iter().chain(&t.generated).any(|x| *x >= v.program.token_bound)
    {
        return Err("mixed-court post identities, scope, replay or typed input differ".into());
    }
    t.evidence.check_structure(&t.segment_roots)?;
    let fed = if let Some(e) = v.encoder {
        if t.prompt.len() > e.l as usize || t.max_new_tokens != 0 || !t.generated.is_empty() || t.evidence.positions != 1 {
            return Err("malformed mixed-court encoder job".into());
        }
        &[][..]
    } else {
        if !misaka_palw_kernel::job::generation_length_matches_v1(t.max_new_tokens, t.generated.len()) {
            return Err("mixed-court delivery length differs".into());
        }
        let fed = &t.generated[..t.generated.len() - 1];
        if t.prompt.len() as u64 + fed.len() as u64 != t.evidence.positions as u64 {
            return Err("mixed-court position count differs".into());
        }
        fed
    };
    if let Some(lengths) = &post.lengths {
        // Shape-only specialization preserves canonical program byte length.
        // Refuse an unrelated large template before its program/range decode.
        if lengths.template.len() < 18
            || u32::from_le_bytes(lengths.template[14..18].try_into().unwrap()) as usize != record.program_bytes.len()
        {
            return Err("bounded length template is not this paid canonical byte extent".into());
        }
        let template = misaka_palw_kernel::flat_lengths7::FlatLengthTemplateV7::from_bytes(&lengths.template)?;
        let specialized = template.check_admission(&lengths.admission, v.encoder.is_some())?;
        if specialized.program != v.program
            || specialized.scope.root() != v.scope.root()
            || specialized.replay.root() != v.replay.root()
        {
            return Err("admitted length specialization differs from registered program/scope/replay".into());
        }
        if v.encoder.is_some() {
            lengths.admission.check_encoder_job(t.prompt.len(), t.evidence.positions)?;
        } else {
            lengths.admission.check_decoder_job(t.prompt.len(), t.max_new_tokens, t.evidence.positions)?;
        }
    }
    let prompt_root = prompt_root_of_ids_v1(&t.prompt);
    let root = post.trace_root(class.as_bytes(), binding.as_bytes());
    if t.evidence.positions > record.plan.max_positions
        || t.evidence.job_input_root != job_input_root_v2(t.prompt.len() as u32, &prompt_root, fed)
        || t.implementation_roots != [root; 3]
    {
        return Err("mixed-court job/context or independent implementation roots differ".into());
    }
    Ok(FlatModelVectorViewV7 {
        profile: post.profile,
        scope: v.scope,
        replay: v.replay,
        trace: SegClaimViewV1 {
            program: v.program,
            params: record.param_commitments,
            segment_roots: t.segment_roots.clone(),
            positions: t.evidence.positions,
            job: JobViewV1 {
                class_binding_id: class.as_bytes(),
                prompt_len: t.prompt.len() as u32,
                prompt_root,
                inline_prompt: Some(t.prompt.clone()),
                max_new_tokens: t.max_new_tokens,
                decode: misaka_palw_kernel::job::DecodeRuleV1::Greedy,
                complete: true,
            },
            generated: t.generated.clone(),
            encoder: v.encoder,
        },
    })
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatNativeSealRowV7 {
    pub seal: misaka_palw_kernel::flat_seal7::FlatSealRowV7,
    pub bond: PalwBondKeyV2,
    pub reserved: u64,
}
impl PalwKernelRouteStateV1 {
    pub fn flat_candidate_record_v7(&self, class: &Hash64) -> Option<FlatCandidateRecordV7> {
        let (_, r): (Digest, FlatCandidateRecordV7) = borsh::from_slice(
            self.rows.get(&(misaka_palw_kernel::rows::TABLE_FLAT_CONFORMANCE_CLASSES_V7, borsh::to_vec(&class.as_bytes()).ok()?))?,
        )
        .ok()?;
        (r.id() == class.as_bytes()).then_some(r)
    }
    pub fn flat_model_vector_protected_v7(&self, vector: &Hash64) -> bool {
        self.aux_row::<bool>(FLAT_PROTECTED_TABLE_V7, &borsh::to_vec(vector).unwrap()).unwrap_or(false)
    }
    pub fn flat_model_vector_view_v7(&self, id: &Hash64) -> Result<FlatModelVectorViewV7, String> {
        if !self.flat_model_vector_protected_v7(id) {
            return Err("not an explicit mixed-court vector7".into());
        }
        let row = self.flat_model_vector_row_v3(id).ok_or("no mixed-court vector")?;
        let h = &row.header;
        if row.post.len() != h.post_bytes_len as usize {
            return Err("mixed stored body length changed".into());
        }
        let p = decode_flat_model_vector_post_v7(&row.post).map_err(|e| e.to_string())?;
        if p.trace_root(h.class.as_bytes(), h.binding.as_bytes()) != h.trace_root
            || p.profile != h.profile
            || p.scope_root != h.scope_root
        {
            return Err("mixed stored identity changed".into());
        }
        let v = checked_flat_model_vector_v7(self, h.class, h.binding, &p)?;
        if v.scope.worst_court_bytes != h.max_filing_bytes
            || v.scope.worst_court_work != h.max_court_work
            || v.scope.worst_court_scratch_bytes != h.max_court_scratch_bytes
            || v.trace.program.encode().len() != h.program_bytes_len as usize
            || v.trace.params.by_instance.len() != h.pc_instances as usize
        {
            return Err("mixed stored resource ceilings changed".into());
        }
        Ok(v)
    }
    pub fn flat_model_seal_v7(&self, vector: &Hash64, actor: &Digest) -> Option<FlatNativeSealRowV7> {
        let r: FlatNativeSealRowV7 = self.aux_row(FLAT_SEAL_TABLE_V7, &borsh::to_vec(&(vector, actor)).ok()?)?;
        (r.seal.version == 7
            && r.seal.vector == vector.as_bytes()
            && r.seal.actor == *actor
            && crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&r.bond) == *actor)
            .then_some(r)
    }
    pub fn flat_model_seals_of_v7(&self, bond: &PalwBondKeyV2) -> Vec<Hash64> {
        let ids: Vec<Hash64> = self.aux_row(FLAT_SEAL_ACTOR_TABLE_V7, &borsh::to_vec(bond).unwrap()).unwrap_or_default();
        if ids.len() <= FLAT_NATIVE_SEALS_PER_BOND_V7 { ids } else { Vec::new() }
    }
    pub fn flat_model_seal_count_v7(&self) -> u32 {
        self.aux_row(FLAT_SEAL_COUNT_TABLE_V7, &[]).unwrap_or(0)
    }
    pub fn flat_model_seal_reserved_v7(&self, bond: &PalwBondKeyV2) -> u128 {
        let a = crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(bond);
        self.flat_model_seals_of_v7(bond).iter().filter_map(|v| self.flat_model_seal_v7(v, &a)).map(|r| r.reserved as u128).sum()
    }
}
