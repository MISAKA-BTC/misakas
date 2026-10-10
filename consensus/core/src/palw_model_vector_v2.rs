//! Publicly prosecutable conformance traces of unregistered scoped candidates.
//! A trace statement is a non-reward signal, never a complete conformance certificate.
//! Its roots use the existing segmented/typed grammar, not legacy flat vector digests.
use crate::{Hash64, palw_kernel_route_v1::PalwKernelRouteStateV1, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{hash::Digest, seg::SegmentedEvidenceV2};
use std::io::{Cursor, Error, ErrorKind};

pub const MODEL_VECTOR_TABLE_V2: u8 = 51;
pub const MODEL_VECTOR_BOND_INDEX_TABLE_V2: u8 = 52;
pub const MODEL_VECTORS_PER_BOND_V2: usize = 8;
pub const MODEL_VECTOR_MAX_POST_BYTES_V2: usize = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 - 134;
pub const MODEL_VECTOR_MAX_PROOF_BYTES_V2: usize = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 - 70;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModelVectorPostV2 {
    pub version: u16,
    pub prompt: Vec<u32>,
    pub max_new_tokens: u32,
    pub generated: Vec<u32>,
    pub evidence: SegmentedEvidenceV2,
    pub segment_roots: Vec<Digest>,
    /// Claimed reference, independent and backend results, each in the same typed grammar.
    /// Equality does not establish software identity or actual independent execution.
    pub implementation_roots: [Digest; 3],
}
impl ModelVectorPostV2 {
    pub fn trace_root(&self, class: Digest, binding: Digest) -> Digest {
        misaka_palw_kernel::hash::object_id(
            b"misaka-palw/model-vector/typed-trace/v2",
            &(class, binding, &self.prompt, self.max_new_tokens, &self.generated, &self.evidence, &self.segment_roots),
        )
    }
    pub fn id(&self, class: Digest, binding: Digest) -> Hash64 {
        vector_id(self.trace_root(class, binding))
    }
}
fn vector_id(trace_root: Digest) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"misaka-palw/model-vector/statement/v2", &trace_root))
}

/// Fixed header precedes variable post bytes: reservation and court pricing never copy a trace.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModelVectorHeaderV2 {
    pub trace_root: Digest,
    pub class: Hash64,
    pub binding: Hash64,
    pub program_bytes_len: u32,
    pub max_filing_bytes: u64,
    pub max_court_work: u64,
    pub poster: PalwBondKeyV2,
    pub posted_daa: u64,
    pub liability_until: u64,
    pub reserved: u64,
    pub refuted: bool,
}
impl ModelVectorHeaderV2 {
    pub fn id(&self) -> Hash64 {
        vector_id(self.trace_root)
    }
    pub fn reserved_at(&self, daa: u64) -> u64 {
        if self.refuted || daa >= self.liability_until { 0 } else { self.reserved }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModelVectorRowV2 {
    pub header: ModelVectorHeaderV2,
    pub post: Vec<u8>,
}
impl PalwKernelRouteStateV1 {
    pub fn model_vector_header_v2(&self, id: &Hash64) -> Option<ModelVectorHeaderV2> {
        let bytes = self.aux.get(&(MODEL_VECTOR_TABLE_V2, borsh::to_vec(id).ok()?))?;
        let h = ModelVectorHeaderV2::deserialize_reader(&mut Cursor::new(bytes)).ok()?;
        (h.id() == *id).then_some(h)
    }
    pub fn model_vector_v2(&self, id: &Hash64) -> Option<ModelVectorRowV2> {
        let row: ModelVectorRowV2 = self.aux_row(MODEL_VECTOR_TABLE_V2, &borsh::to_vec(id).ok()?)?;
        (row.header.id() == *id).then_some(row)
    }
    pub fn model_vectors_of_v2(&self, bond: &PalwBondKeyV2) -> Vec<Hash64> {
        let ids: Vec<Hash64> = self.aux_row(MODEL_VECTOR_BOND_INDEX_TABLE_V2, &borsh::to_vec(bond).unwrap()).unwrap_or_default();
        if ids.len() > MODEL_VECTORS_PER_BOND_V2 { Vec::new() } else { ids }
    }
    pub fn model_vector_reserved_at_v2(&self, bond: &PalwBondKeyV2, daa: u64) -> u128 {
        self.model_vectors_of_v2(bond)
            .iter()
            .filter_map(|id| self.model_vector_header_v2(id))
            .filter(|h| h.poster == *bond)
            .map(|h| h.reserved_at(daa) as u128)
            .sum()
    }
    /// Rebuild the exact public court context. Neither a kernel job, execution permission
    /// nor a Final claim is needed; the candidate's admitted program/plan is authoritative.
    pub fn model_vector_view_v2(&self, id: &Hash64) -> Result<misaka_palw_kernel::seg_ledger::SegClaimViewV1, String> {
        let row = self.model_vector_v2(id).ok_or("no model vector")?;
        let post = decode_model_vector_post_v2(&row.post).map_err(|e| e.to_string())?;
        if post.trace_root(row.header.class.as_bytes(), row.header.binding.as_bytes()) != row.header.trace_root {
            return Err("model vector body differs from its statement".into());
        }
        checked_vector_view(self, row.header.class, row.header.binding, &post)
    }
}

pub(crate) fn checked_vector_view(
    route: &PalwKernelRouteStateV1,
    class: Hash64,
    binding: Hash64,
    post: &ModelVectorPostV2,
) -> Result<misaka_palw_kernel::seg_ledger::SegClaimViewV1, String> {
    use misaka_palw_kernel::{descriptor::*, evidence::*, seg::*, seg_ledger::*};
    let record = route.kernel_class_record_v1(&class).ok_or("no admitted vector candidate")?;
    let statement = route.model_artifact_binding_header_v2(&binding).ok_or("no scoped model statement")?;
    if record.descriptor != statement.descriptor
        || misaka_palw_kernel::public::program_root_v1(&record.program_bytes) != statement.program_root
        || record.param_commitments.root() != statement.kernel_param_root.as_bytes()
    {
        return Err("vector candidate is not the model statement's scope".into());
    }
    let d = [k2_tir_v4_descriptor(), k2_tir_v5_descriptor()]
        .into_iter()
        .find(|d| d.digest() == record.descriptor)
        .ok_or("unknown vector descriptor")?;
    let program = misaka_palw_tir::TirProgramV1::decode_canonical(&record.program_bytes).map_err(|e| e.to_string())?;
    let header = EvidenceHeaderV1 {
        network_domain: route.header.policy.network_domain,
        ruleset_digest: route.header.policy.ruleset_digest,
        class_binding_id: class.as_bytes(),
        program_root: statement.program_root,
        artifact_root: statement.kernel_param_root.as_bytes(),
        plan_root: record.plan.root(),
    };
    if post.version != 2
        || post.evidence.header != header
        || post.evidence.suite != SuiteParamsV1::of(&d)
        || post.prompt.is_empty()
        || post.prompt.iter().chain(&post.generated).any(|t| *t >= program.token_bound)
    {
        return Err("vector identity, suite or input is malformed".into());
    }
    post.evidence.check_structure(&post.segment_roots)?;
    let encoder = if d.digest() == k2_tir_v5_descriptor().digest() {
        Some(misaka_palw_kernel::seg_encoder::encoder_binding_v1(&program)?)
    } else {
        None
    };
    let fed = if let Some(e) = encoder {
        if post.prompt.len() > e.l as usize || post.max_new_tokens != 0 || !post.generated.is_empty() || post.evidence.positions != 1 {
            return Err("malformed encoder vector".into());
        }
        &[][..]
    } else {
        if !misaka_palw_kernel::job::generation_length_matches_v1(post.max_new_tokens, post.generated.len()) {
            return Err("vector has another delivery length".into());
        }
        let fed = &post.generated[..post.generated.len() - 1];
        if post.prompt.len() as u64 + fed.len() as u64 != post.evidence.positions as u64 {
            return Err("vector has another position count".into());
        }
        fed
    };
    let prompt_root = prompt_root_of_ids_v1(&post.prompt);
    if post.evidence.positions > record.plan.max_positions
        || post.evidence.job_input_root != job_input_root_v2(post.prompt.len() as u32, &prompt_root, fed)
        || post.implementation_roots != [post.trace_root(class.as_bytes(), binding.as_bytes()); 3]
    {
        return Err("vector input, context or implementation roots differ".into());
    }
    Ok(SegClaimViewV1 {
        program,
        params: record.param_commitments,
        segment_roots: post.segment_roots.clone(),
        positions: post.evidence.positions,
        job: JobViewV1 {
            class_binding_id: class.as_bytes(),
            prompt_len: post.prompt.len() as u32,
            prompt_root,
            inline_prompt: Some(post.prompt.clone()),
            max_new_tokens: post.max_new_tokens,
            decode: misaka_palw_kernel::job::DecodeRuleV1::Greedy,
            complete: true,
        },
        generated: post.generated.clone(),
        encoder,
    })
}

fn invalid() -> Error {
    Error::new(ErrorKind::InvalidData, "model vector counts exceed the bounded wire")
}
fn count(r: &mut Cursor<&[u8]>, max: usize, width: usize) -> std::io::Result<usize> {
    let n = u32::deserialize_reader(r)? as usize;
    if n > max || n > r.get_ref().len().saturating_sub(r.position() as usize) / width {
        return Err(invalid());
    }
    Ok(n)
}
fn ids(r: &mut Cursor<&[u8]>) -> std::io::Result<Vec<u32>> {
    let n = count(r, 1 << 21, 4)?;
    (0..n).map(|_| u32::deserialize_reader(r)).collect()
}
pub fn decode_model_vector_post_v2(data: &[u8]) -> std::io::Result<ModelVectorPostV2> {
    if data.len() > MODEL_VECTOR_MAX_POST_BYTES_V2 {
        return Err(invalid());
    }
    let mut r = Cursor::new(data);
    let version = u16::deserialize_reader(&mut r)?;
    if version != 2 {
        return Err(invalid());
    }
    let prompt = ids(&mut r)?;
    let max_new_tokens = u32::deserialize_reader(&mut r)?;
    let generated = ids(&mut r)?;
    let evidence = SegmentedEvidenceV2::deserialize_reader(&mut r)?;
    let n = count(&mut r, misaka_palw_kernel::seg::SEG_MAX_SEGMENTS_V4, 64)?;
    let segment_roots = (0..n).map(|_| Digest::deserialize_reader(&mut r)).collect::<std::io::Result<_>>()?;
    let implementation_roots = <[Digest; 3]>::deserialize_reader(&mut r)?;
    if r.position() as usize != data.len() {
        return Err(invalid());
    }
    Ok(ModelVectorPostV2 { version, prompt, max_new_tokens, generated, evidence, segment_roots, implementation_roots })
}

/// Only bounded element/type/decode courts are admitted here. WholeValue's optional
/// tensors are deliberately excluded; they need their separate admission/resource contract.
pub fn decode_model_vector_fault_v2(data: &[u8]) -> std::io::Result<misaka_palw_kernel::element::SegFaultV1> {
    use misaka_palw_kernel::{element::*, seg::*};
    fn node(r: &mut Cursor<&[u8]>) -> std::io::Result<NodeOpeningV1> {
        let position = u32::deserialize_reader(r)?;
        let occurrence = u16::deserialize_reader(r)?;
        let node = u16::deserialize_reader(r)?;
        let commitment = Digest::deserialize_reader(r)?;
        let n = count(r, 64, 64)?;
        let node_siblings = (0..n).map(|_| Digest::deserialize_reader(r)).collect::<std::io::Result<_>>()?;
        let position_root = Digest::deserialize_reader(r)?;
        let n = count(r, 10, 64)?;
        let position_siblings = (0..n).map(|_| Digest::deserialize_reader(r)).collect::<std::io::Result<_>>()?;
        Ok(NodeOpeningV1 { position, occurrence, node, commitment, node_siblings, position_root, position_siblings })
    }
    fn operand(r: &mut Cursor<&[u8]>) -> std::io::Result<OperandOpeningV1> {
        let node = match u8::deserialize_reader(r)? {
            0 => None,
            1 => Some(node(r)?),
            _ => return Err(invalid()),
        };
        let n = count(r, MODEL_VECTOR_MAX_PROOF_BYTES_V2 / 70, 70)?;
        let leaves = (0..n)
            .map(|_| crate::palw_onboarding_v1::ArtifactTileOpeningV3::deserialize_reader(r).map(|t| t.leaf().clone()))
            .collect::<std::io::Result<_>>()?;
        Ok(OperandOpeningV1 { node, leaves })
    }
    fn token(r: &mut Cursor<&[u8]>) -> std::io::Result<Option<PromptTileOpeningV1>> {
        match u8::deserialize_reader(r)? {
            0 => Ok(None),
            1 => {
                let index = u32::deserialize_reader(r)?;
                let n = count(r, PROMPT_TILE_IDS_V1, 4)?;
                let ids = (0..n).map(|_| u32::deserialize_reader(r)).collect::<std::io::Result<_>>()?;
                let n = count(r, 64, 64)?;
                let siblings = (0..n).map(|_| Digest::deserialize_reader(r)).collect::<std::io::Result<_>>()?;
                Ok(Some(PromptTileOpeningV1 { index, ids, siblings }))
            }
            _ => Err(invalid()),
        }
    }
    if data.len() > MODEL_VECTOR_MAX_PROOF_BYTES_V2 {
        return Err(invalid());
    }
    let mut r = Cursor::new(data);
    let fault = match u8::deserialize_reader(&mut r)? {
        0 => {
            let position = u32::deserialize_reader(&mut r)?;
            let occurrence = u16::deserialize_reader(&mut r)?;
            let node = u16::deserialize_reader(&mut r)?;
            let element = u64::deserialize_reader(&mut r)?;
            let output = operand(&mut r)?;
            let n = count(&mut r, MODEL_VECTOR_MAX_PROOF_BYTES_V2 / 5, 5)?;
            let inputs = (0..n).map(|_| operand(&mut r)).collect::<std::io::Result<_>>()?;
            SegFaultV1::Element(ElementFaultV1 { position, occurrence, node, element, output, inputs, token: token(&mut r)? })
        }
        1 => {
            let opening = node(&mut r)?;
            let dtype = u8::deserialize_reader(&mut r)?;
            let n = count(&mut r, MODEL_VECTOR_MAX_PROOF_BYTES_V2 / 8, 8)?;
            let shape = (0..n).map(|_| u64::deserialize_reader(&mut r)).collect::<std::io::Result<_>>()?;
            SegFaultV1::Malformed(MalformedFaultV1 {
                opening,
                dtype,
                shape,
                row_root: Digest::deserialize_reader(&mut r)?,
                col_root: Digest::deserialize_reader(&mut r)?,
            })
        }
        2 => SegFaultV1::Decode(SegDecodeFaultV1 {
            index: u32::deserialize_reader(&mut r)?,
            rival: u32::deserialize_reader(&mut r)?,
            logits: operand(&mut r)?,
        }),
        _ => return Err(invalid()),
    };
    if r.position() as usize != data.len() {
        return Err(invalid());
    }
    Ok(fault)
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_kernel::{element::*, seg::*};
    fn post() -> ModelVectorPostV2 {
        let d = misaka_palw_kernel::descriptor::k2_tir_v4_descriptor();
        ModelVectorPostV2 {
            version: 2,
            prompt: vec![0, 1],
            max_new_tokens: 1,
            generated: vec![1],
            evidence: SegmentedEvidenceV2 {
                version: 2,
                header: misaka_palw_kernel::evidence::EvidenceHeaderV1 {
                    network_domain: [1; 64],
                    ruleset_digest: [2; 64],
                    class_binding_id: [3; 64],
                    program_root: [4; 64],
                    artifact_root: [5; 64],
                    plan_root: [6; 64],
                },
                job_input_root: [7; 64],
                positions: 2,
                segment_len: SEG_LEN_V4,
                claim_root: [8; 64],
                suite: misaka_palw_kernel::evidence::SuiteParamsV1::of(&d),
            },
            segment_roots: vec![[9; 64]],
            implementation_roots: [[10; 64]; 3],
        }
    }
    fn opening() -> NodeOpeningV1 {
        NodeOpeningV1 {
            position: 0,
            occurrence: 0,
            node: 0,
            commitment: [1; 64],
            node_siblings: vec![[2; 64]],
            position_root: [3; 64],
            position_siblings: Vec::new(),
        }
    }
    #[test]
    fn model_vector_post_wire_is_bounded_before_allocation_and_preserves_exact_borsh() {
        let p = post();
        let bytes = borsh::to_vec(&p).unwrap();
        assert_eq!(decode_model_vector_post_v2(&bytes).unwrap(), p);
        for (offset, value) in [(0, 3u16.to_le_bytes().to_vec()), (2, u32::MAX.to_le_bytes().to_vec())] {
            let mut bad = bytes.clone();
            bad[offset..offset + value.len()].copy_from_slice(&value);
            assert_eq!(decode_model_vector_post_v2(&bad).unwrap_err().kind(), ErrorKind::InvalidData);
        }
        let mut bad = bytes;
        bad.push(0);
        assert!(decode_model_vector_post_v2(&bad).is_err());
        assert!(decode_model_vector_post_v2(&vec![0; MODEL_VECTOR_MAX_POST_BYTES_V2 + 1]).is_err());
    }
    #[test]
    fn model_vector_fault_wire_bounds_every_nested_count_and_keeps_valid_court_grammars() {
        let t = misaka_palw_tir::Tensor::new(misaka_palw_tir::DType::I8, vec![1], vec![3]).unwrap();
        let operand = OperandOpeningV1 {
            node: Some(opening()),
            leaves: vec![misaka_palw_kernel::merkle3::LeafOpeningV3::of(&t, 0, 0, 0).unwrap()],
        };
        let f = ElementFaultV1 {
            position: 0,
            occurrence: 0,
            node: 0,
            element: 0,
            output: operand.clone(),
            inputs: vec![OperandOpeningV1::default(); 32],
            token: Some(PromptTileOpeningV1::of(&[0, 1], 0).unwrap()),
        };
        let faults = [
            SegFaultV1::Element(f),
            SegFaultV1::Malformed(MalformedFaultV1 {
                opening: opening(),
                dtype: 0,
                shape: vec![1; 5],
                row_root: [2; 64],
                col_root: [3; 64],
            }),
            SegFaultV1::Decode(SegDecodeFaultV1 { index: 0, rival: 1, logits: operand }),
        ];
        for f in faults {
            let bytes = f.to_bytes();
            assert_eq!(decode_model_vector_fault_v2(&bytes).unwrap(), f);
            let mut bad = bytes;
            bad.push(0);
            assert!(decode_model_vector_fault_v2(&bad).is_err());
        }
        // A type proof's node sibling count; refusal precedes any missing sibling body.
        let mut bad = vec![1];
        bad.extend(0u32.to_le_bytes());
        bad.extend([0; 4]);
        bad.extend([1; 64]);
        bad.extend(65u32.to_le_bytes());
        assert_eq!(decode_model_vector_fault_v2(&bad).unwrap_err().kind(), ErrorKind::InvalidData);
        assert_eq!(decode_model_vector_fault_v2(&[3]).unwrap_err().kind(), ErrorKind::InvalidData);
    }
    #[test]
    fn model_vector_statement_binds_exact_scope_input_delivery_and_clocked_capital() {
        let p = post();
        let id = p.id([3; 64], [4; 64]);
        assert_ne!(id, p.id([5; 64], [4; 64]));
        assert_ne!(id, p.id([3; 64], [5; 64]));
        for which in 0..5 {
            let mut q = p.clone();
            match which {
                0 => q.prompt[0] = 1,
                1 => q.generated[0] = 0,
                2 => q.evidence.header.plan_root[0] ^= 1,
                3 => q.segment_roots[0][0] ^= 1,
                _ => q.max_new_tokens += 1,
            }
            assert_ne!(id, q.id([3; 64], [4; 64]));
        }
        let h = ModelVectorHeaderV2 {
            trace_root: p.trace_root([3; 64], [4; 64]),
            class: Hash64::from_bytes([3; 64]),
            binding: Hash64::from_bytes([4; 64]),
            program_bytes_len: 123,
            max_filing_bytes: 456,
            max_court_work: 789,
            poster: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(9), 0)),
            posted_daa: 1,
            liability_until: 201,
            reserved: 100,
            refuted: false,
        };
        assert_eq!(h.id(), id);
        assert_eq!(h.reserved_at(200), 100);
        assert_eq!(h.reserved_at(201), 0);
        let row = ModelVectorRowV2 { header: h.clone(), post: borsh::to_vec(&p).unwrap() };
        assert_eq!(ModelVectorHeaderV2::deserialize_reader(&mut Cursor::new(borsh::to_vec(&row).unwrap())).unwrap(), h);
        assert_eq!(ModelVectorHeaderV2 { refuted: true, ..h }.reserved_at(2), 0);
    }
    #[test]
    fn model_vector_tables_are_disjoint_from_provider_and_scoped_model_allocations() {
        let tables = [43, 44, 45, 46, 47, 48, 49, 50, MODEL_VECTOR_TABLE_V2, MODEL_VECTOR_BOND_INDEX_TABLE_V2];
        assert_eq!(tables.into_iter().collect::<std::collections::BTreeSet<_>>().len(), 10);
    }
}
