//! Exact streaming re-execution of the dormant flat-v5 profile. Public material
//! contains only node commitments and hash paths. Values used in a filing come
//! from the verifier's independently acquired registered model and public job.
//! This linear node scan is not a sampled checker or a deadline/liveness proof.
use crate::element::{OperandSourceV1, PreparedV1, operand_sources};
use crate::flat_court4 as common;
use crate::flat_court5::*;
use crate::merkle3::{LeafOpeningV3, TreesV3};
use crate::merkle5::{FlatDescentV5, FlatLeafHashV5, FlatLeafValueV5, FlatTreeV5};
use crate::seg::{NodeOpeningV1, PROMPT_TILE_IDS_V1, PromptTileOpeningV1};
use crate::trace::SourceV1;
use misaka_palw_tir::{DType, Tensor, TirError, TirErrorKind};
use std::collections::BTreeSet;

/// Coordinates map directly to native candidate-vector point DA units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlatReadV5 {
    pub position: u32,
    pub occurrence: u16,
    pub node: u16,
    pub leaf: Option<u64>,
}
impl FlatReadV5 {
    fn node(p: u32, s: u16, n: u16) -> Self {
        Self { position: p, occurrence: s, node: n, leaf: None }
    }
}
/// Implementations may serve cached authenticated chain responses or an
/// off-chain stream. The verifier authenticates each response independently.
pub trait FlatPublicReadsV5 {
    fn node(&self, unit: FlatReadV5) -> Option<NodeOpeningV1>;
    fn hash(&self, unit: FlatReadV5) -> Option<FlatLeafHashV5>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlatFindingV5 {
    /// Exact replay, all node commitments and delivered decode ids agree.
    Clean,
    Fault(FlatFaultV5),
    /// Absent, malformed or borrowed material never establishes correctness.
    Demand(FlatReadV5),
    /// Wrong/missing model, job, reference operands or an inadmissible relation.
    Inconsistent(String),
}
fn error(e: impl std::fmt::Debug) -> FlatFindingV5 {
    FlatFindingV5::Inconsistent(format!("{e:?}"))
}
fn read_node(
    c: &FlatSegContextV5<'_>,
    pr: &PreparedV1<'_>,
    public: &dyn FlatPublicReadsV5,
    unit: FlatReadV5,
) -> Result<NodeOpeningV1, FlatFindingV5> {
    public
        .node(unit)
        .filter(|o| common::authenticate_node(c, pr, o, (unit.position, unit.occurrence, unit.node)))
        .ok_or(FlatFindingV5::Demand(unit))
}
fn value_proof(t: &Tensor, node: Option<NodeOpeningV1>, reads: &[(u64, u64)]) -> Result<FlatOperandV5, FlatFindingV5> {
    let mut out = FlatOperandV5 { node, flat: Vec::new(), legacy: Vec::new() };
    if out.node.is_some() && t.data.len() >= 64 {
        let tree = FlatTreeV5::of(t).map_err(error)?;
        for &(i, _) in reads {
            out.flat.push(FlatLeafValueV5::of(t, &tree, i).ok_or_else(|| error("missing own flat leaf"))?);
        }
    } else {
        let tree = TreesV3::of(t);
        for &(row, tile) in reads {
            out.legacy.push(LeafOpeningV3::of_trees(t, &tree, 0, row, tile).ok_or_else(|| error("missing own v3 leaf"))?);
        }
    }
    Ok(out)
}
fn checked_fault(c: &FlatSegContextV5<'_>, fault: FlatFaultV5) -> FlatFindingV5 {
    match fault.to_bytes().and_then(|bytes| FlatFaultV5::from_bytes(&bytes)) {
        Err(e) => error(e),
        Ok(decoded) => match verify_flat_fault_v5(c, &decoded) {
            Ok(_) => FlatFindingV5::Fault(decoded),
            Err(e) => error(e),
        },
    }
}
fn relation(
    c: &FlatSegContextV5<'_>,
    pr: &PreparedV1<'_>,
    public: &dyn FlatPublicReadsV5,
    unit: FlatReadV5,
    expected: &Tensor,
    own: &dyn Fn() -> Result<Vec<Tensor>, String>,
    token: &dyn Fn() -> Option<PromptTileOpeningV1>,
) -> FlatFindingV5 {
    let output = match read_node(c, pr, public, unit) {
        Ok(o) => o,
        Err(f) => return f,
    };
    let (p, s, n) = (unit.position, unit.occurrence, unit.node);
    let (dtype, shape) = pr.declared(s, n, p);
    if expected.dtype != dtype || expected.shape != shape {
        return error("reference output metadata differs from the program");
    }
    let root = match flat_profile_commitment_v5(expected) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    if root == output.commitment {
        return FlatFindingV5::Clean;
    }
    let leaf = if expected.data.len() >= 64 {
        let tree = match FlatTreeV5::of(expected) {
            Ok(t) => t,
            Err(e) => return error(e),
        };
        let max = tree.layout().bounds_for::<5>(dtype).expect("validated tree bounds").localization_replies;
        let mut next = 0;
        let mut selected = None;
        for _ in 0..max {
            let read = FlatReadV5 { leaf: Some(next), ..unit };
            let Some(hash) = public.hash(read).filter(|h| h.index == next) else { return FlatFindingV5::Demand(read) };
            match hash.descend(&output.commitment, &tree) {
                Ok(FlatDescentV5::Probe(i)) => next = i,
                Ok(FlatDescentV5::DivergentLeaf(_)) => {
                    selected = Some(hash);
                    break;
                }
                _ => return FlatFindingV5::Demand(read),
            }
        }
        let Some(hash) = selected else { return error("flat localization exceeded its bound") };
        Some(hash)
    } else {
        None
    };
    let own = match own() {
        Ok(t) => t,
        Err(e) => return error(e),
    };
    let deps = match &leaf {
        Some(h) => flat_leaf_dependencies_v5(c, p, s, n, h.index, &own),
        None => flat_whole_dependencies_v5(c, p, s, n, &own),
    };
    let deps = match deps {
        Ok(d) => d,
        Err(e) => return error(e),
    };
    let sources = match operand_sources(pr, p, s, n) {
        Ok(v) => v,
        Err(e) => return error(e),
    };
    let mut inputs = Vec::with_capacity(sources.len());
    for ((source, t), reads) in sources.iter().zip(&own).zip(&deps.leaves) {
        let node = match source {
            OperandSourceV1::Source(SourceV1::Node { position, occurrence, node }) => {
                let o = match read_node(c, pr, public, FlatReadV5::node(*position, *occurrence, *node)) {
                    Ok(o) => o,
                    Err(f) => return f,
                };
                if flat_profile_commitment_v5(t).ok() != Some(o.commitment) {
                    return error("a prior operand differs: this is not the first faulty relation");
                }
                Some(o)
            }
            OperandSourceV1::Source(SourceV1::Param { index, layer }) => {
                if c.claim.params.by_instance.get(&(*index, *layer)) != Some(&crate::merkle3::tensor_commitment_v3(t)) {
                    return error("own parameter is not the registered model's");
                }
                None
            }
            _ => {
                inputs.push(FlatOperandV5::default());
                continue;
            }
        };
        match value_proof(t, node, reads) {
            Ok(o) => inputs.push(o),
            Err(f) => return f,
        }
    }
    let token = token();
    let fault = match leaf {
        Some(leaf) => FlatFaultV5::Leaf(FlatLeafFaultV5 { version: 5, output, leaf, inputs, token }),
        None => FlatFaultV5::Whole(FlatWholeFaultV5 { output, inputs, token }),
    };
    checked_fault(c, fault)
}

/// Re-execute from authenticated job ids and independently acquired parameters.
/// Retains current/previous position values, not the complete trace. Public
/// reads have no tensor-value API. Stop at the first fault or DA request. A
/// retry replays the prefix; callers should cache replies, and must account for
/// replay/hash/transport costs separately from the bounded terminal court.
pub fn check_flat_claim_v5(
    c: &FlatSegContextV5<'_>,
    public: &dyn FlatPublicReadsV5,
    artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
    tokens: &[u32],
) -> FlatFindingV5 {
    let pr = match common::prepare(c) {
        Ok(p) => p,
        Err(e) => return error(e),
    };
    let mut previous: Vec<Vec<Tensor>> = Vec::new();
    let mut stopped = None;
    let mut visit = |p: u32, values: &[Vec<Tensor>]| {
        for (s, occurrence) in values.iter().enumerate() {
            for (n, expected) in occurrence.iter().enumerate() {
                let (s, n) = (s as u16, n as u16);
                let own = || {
                    operand_sources(&pr, p, s, n)?
                        .iter()
                        .map(|src| {
                            let t = match src {
                                OperandSourceV1::Token => {
                                    Tensor::scalar(DType::Idx, *tokens.get(p as usize).ok_or("missing fed id")? as i128).ok()
                                }
                                OperandSourceV1::Position => Tensor::scalar(DType::Idx, p as i128).ok(),
                                OperandSourceV1::Source(SourceV1::Node { position, occurrence, node }) => {
                                    let pos = if *position == p {
                                        values
                                    } else if position.checked_add(1) == Some(p) {
                                        &previous
                                    } else {
                                        return Err("source older than the previous position".into());
                                    };
                                    pos.get(*occurrence as usize).and_then(|v| v.get(*node as usize)).cloned()
                                }
                                OperandSourceV1::Source(SourceV1::Param { index, layer }) => artifact(*index, *layer),
                                OperandSourceV1::Source(SourceV1::Const(j)) => crate::trace::const_tensor(c.claim.program, *j).ok(),
                                OperandSourceV1::Source(SourceV1::Zeros { dtype, shape }) => Some(Tensor::zeros(*dtype, shape)),
                                OperandSourceV1::Source(SourceV1::Public(v)) => Tensor::scalar(DType::Idx, *v as i128).ok(),
                                OperandSourceV1::Source(SourceV1::Input { k, .. }) => {
                                    let e = c.claim.encoder.as_ref().ok_or("unbound encoder input")?;
                                    match k {
                                        0 => e.ids(&tokens[..c.claim.prompt_len as usize]).ok(),
                                        1 => e.count(c.claim.prompt_len).ok(),
                                        _ => None,
                                    }
                                }
                            };
                            t.ok_or_else(|| "missing own operand".into())
                        })
                        .collect::<Result<Vec<_>, String>>()
                };
                let token = || {
                    if c.claim.inline_prompt.is_none() && (p < c.claim.prompt_len || c.claim.encoder.is_some()) {
                        let tile = if c.claim.encoder.is_some() { 0 } else { p / PROMPT_TILE_IDS_V1 as u32 };
                        PromptTileOpeningV1::of(&tokens[..c.claim.prompt_len as usize], tile)
                    } else {
                        None
                    }
                };
                let finding = relation(c, &pr, public, FlatReadV5::node(p, s, n), expected, &own, &token);
                if finding != FlatFindingV5::Clean {
                    stopped = Some(finding);
                    return Err(TirError::new(TirErrorKind::Missing, "flat verifier stopped"));
                }
            }
        }
        if c.claim.encoder.is_none() && p + 1 >= c.claim.prompt_len {
            let index = p + 1 - c.claim.prompt_len;
            let s = (values.len() - 1) as u16;
            let n = c.claim.program.logits;
            let t = &values[s as usize][n as usize];
            let delivered = c.claim.generated[index as usize];
            if c.claim.decode.select(t) != Some(delivered) {
                let finding = (|| {
                    let node = read_node(c, &pr, public, FlatReadV5::node(p, s, n))?;
                    let rival = c.claim.decode.select(t).ok_or_else(|| error("empty logits"))?;
                    let reads = if delivered as usize >= t.data.len() {
                        Vec::new()
                    } else {
                        [delivered, rival]
                            .iter()
                            .map(|e| {
                                if t.data.len() >= 64 {
                                    (FlatTreeV5::of(t).expect("validated logits").layout().leaf_of(*e as u64).unwrap(), 0)
                                } else {
                                    let (row, tile, _) = crate::merkle3::LayoutV3::of(&t.shape).row_leaf_of(*e as u64).unwrap();
                                    (row, tile)
                                }
                            })
                            .collect::<BTreeSet<_>>()
                            .into_iter()
                            .collect()
                    };
                    Ok(checked_fault(
                        c,
                        FlatFaultV5::Decode(FlatDecodeFaultV5 { index, rival, logits: value_proof(t, Some(node), &reads)? }),
                    ))
                })()
                .unwrap_or_else(|f| f);
                stopped = Some(finding);
                return Err(TirError::new(TirErrorKind::Missing, "flat decode stopped"));
            }
        }
        previous = values.to_vec();
        Ok(())
    };
    let replay = crate::seg_detect::replay_values_v1(&c.claim, artifact, tokens, None, &mut visit);
    if let Some(f) = stopped {
        f
    } else {
        match replay {
            Ok(()) => FlatFindingV5::Clean,
            Err(e) => error(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::SegClaimContextV1;
    use crate::flat_court4::tests::{primitive_fixture, state_fixture};
    use crate::seg::{SegmentedCommitmentsV1, prompt_root_of_ids_v1};
    use crate::trace::{ParamCommitmentsV1, trace_v1};
    use misaka_palw_tir::{MapParams, program::TirProgramV1};
    use std::cell::Cell;

    // Private producer values are inaccessible through the public provider API.
    struct Server {
        c: SegmentedCommitmentsV1,
        values: Vec<Vec<Vec<Tensor>>>,
        missing: Option<FlatReadV5>,
        borrowed: bool,
        hashes: Cell<u64>,
    }
    impl FlatPublicReadsV5 for Server {
        fn node(&self, u: FlatReadV5) -> Option<NodeOpeningV1> {
            if self.missing == Some(u) {
                return None;
            }
            let mut o = self.c.node_opening(u.position, u.occurrence, u.node)?;
            if self.borrowed {
                o.position = o.position.wrapping_add(1);
            }
            // Wire roundtrip separates server storage from received data.
            borsh::from_slice(&borsh::to_vec(&o).unwrap()).ok()
        }
        fn hash(&self, u: FlatReadV5) -> Option<FlatLeafHashV5> {
            self.hashes.set(self.hashes.get() + 1);
            if self.missing == Some(u) {
                return None;
            }
            let t = &self.values[u.position as usize][u.occurrence as usize][u.node as usize];
            let h = FlatTreeV5::of(t).ok()?.opening(u.leaf?)?;
            crate::merkle5::decode_flat_hash_v5(&borsh::to_vec(&h).unwrap()).ok()
        }
    }
    struct Trial {
        p: TirProgramV1,
        m: MapParams,
        pc: ParamCommitmentsV1,
        prompt: Vec<u32>,
        generated: Vec<u32>,
        roots: Vec<crate::hash::Digest>,
        server: Server,
    }
    impl Trial {
        fn of(mut p: TirProgramV1, m: MapParams, prompt: Vec<u32>, wrong: Option<(u32, u16, u16, usize)>) -> Self {
            // Fixtures have 129 logits. A fresh job must permit their selected id.
            p.token_bound = 256;
            let mut trace = trace_v1(&p, &m, &prompt).unwrap();
            let generated = vec![
                crate::job::DecodeRuleV1::Greedy.select(&trace.values.last().unwrap().last().unwrap()[p.logits as usize]).unwrap(),
            ];
            if let Some((q, s, n, e)) = wrong {
                trace.values[q as usize][s as usize][n as usize].data[e] += 1;
            }
            let c = SegmentedCommitmentsV1::new(
                trace
                    .values
                    .iter()
                    .map(|pos| pos.iter().map(|occ| occ.iter().map(|t| flat_profile_commitment_v5(t).unwrap()).collect()).collect())
                    .collect(),
            );
            let roots = c.segment_roots();
            let pc = ParamCommitmentsV1::of_v3(&m);
            Self {
                p,
                m,
                pc,
                prompt,
                generated,
                roots,
                server: Server { c, values: trace.values, missing: None, borrowed: false, hashes: Cell::new(0) },
            }
        }
        fn context(&self, inline: bool) -> FlatSegContextV5<'_> {
            FlatSegContextV5 {
                profile: flat_trace_profile_id_v5(),
                claim: SegClaimContextV1 {
                    program: &self.p,
                    params: &self.pc,
                    segment_roots: &self.roots,
                    positions: self.server.c.positions(),
                    prompt_len: self.prompt.len() as u32,
                    prompt_root: prompt_root_of_ids_v1(&self.prompt),
                    inline_prompt: inline.then_some(&self.prompt),
                    generated: &self.generated,
                    decode: crate::job::DecodeRuleV1::Greedy,
                    encoder: None,
                },
            }
        }
        fn check(&self, inline: bool) -> FlatFindingV5 {
            let mut fed = self.prompt.clone();
            fed.extend_from_slice(&self.generated[..self.generated.len() - 1]);
            check_flat_claim_v5(&self.context(inline), &self.server, &|i, l| self.m.tensors.get(&(i, l)).cloned(), &fed)
        }
        fn assert_fault(&self, inline: bool, at: (u32, u16, u16)) -> FlatFaultV5 {
            let FlatFindingV5::Fault(f) = self.check(inline) else {
                panic!("no fault: {:?}", self.check(inline));
            };
            let v = verify_flat_fault_v5(&self.context(inline), &f).unwrap();
            assert_eq!((v.position, v.occurrence, v.node), at);
            let bound = flat_fault_court_bounds_v5(&self.context(inline), &f).unwrap();
            assert!(f.to_bytes().unwrap().len() as u64 <= bound.filing_bytes);
            f
        }
    }
    #[test]
    fn flat_stream_verifier_all_25_primitives_localizes_without_output_values() {
        let mut tags = BTreeSet::new();
        for tag in 0..23 {
            let (p, m, n) = primitive_fixture(tag);
            assert_eq!(p.blocks[0].nodes[n as usize].prim.tag(), tag);
            tags.insert(tag);
            let good = Trial::of(p.clone(), m.clone(), vec![1, 2], None);
            assert_eq!(good.check(true), FlatFindingV5::Clean, "tag {tag}");
            assert_eq!(good.server.hashes.get(), 0, "matching nodes need no leaf reads");
            let last = good.server.values[1][0][n as usize].data.len() - 1;
            let bad = Trial::of(p, m, vec![1, 2], Some((1, 0, n, last)));
            bad.assert_fault(true, (1, 0, n));
            assert!(bad.server.hashes.get() <= 3, "129-element target has two leaves");
        }
        let (p, m, wn, hn) = state_fixture();
        for (tag, n) in [(23, wn), (24, hn)] {
            assert_eq!(p.blocks[0].nodes[n as usize].prim.tag(), tag);
            tags.insert(tag);
            for position in 0..4 {
                let good = Trial::of(p.clone(), m.clone(), vec![1, 2, 3, 1], None);
                assert_eq!(good.check(false), FlatFindingV5::Clean);
                let last = good.server.values[position][0][n as usize].data.len() - 1;
                let bad = Trial::of(p.clone(), m.clone(), vec![1, 2, 3, 1], Some((position as u32, 0, n, last)));
                bad.assert_fault(false, (position as u32, 0, n));
            }
        }
        assert_eq!(tags, (0..25).collect());
    }
    #[test]
    fn flat_stream_verifier_small_nodes_decode_ties_and_wrong_registered_model() {
        let (p, m, _, _) = state_fixture();
        let bad = Trial::of(p.clone(), m.clone(), vec![1, 2, 3], Some((1, 0, 0, 0)));
        assert!(matches!(bad.assert_fault(false, (1, 0, 0)), FlatFaultV5::Whole(_)));
        let mut decode = Trial::of(p, m, vec![1, 2, 3], None);
        decode.generated[0] = 1; // All logits tie: lowest index wins.
        assert!(matches!(decode.assert_fault(true, (2, 1, 0)), FlatFaultV5::Decode(_)));
        decode.generated[0] = 200; // Outside the logits, within the job id bound.
        assert!(matches!(decode.assert_fault(true, (2, 1, 0)), FlatFaultV5::Decode(_)));
        let (p, m, _) = primitive_fixture(11);
        let good = Trial::of(p, m, vec![1], None);
        let wrong = |i, l| {
            let mut t = good.m.tensors.get(&(i, l))?.clone();
            t.data[0] += 1;
            Some(t)
        };
        assert!(matches!(
            check_flat_claim_v5(&good.context(true), &good.server, &wrong, &good.prompt),
            FlatFindingV5::Inconsistent(_)
        ));
        assert!(matches!(
            check_flat_claim_v5(&good.context(true), &good.server, &|_, _| None, &good.prompt),
            FlatFindingV5::Inconsistent(_)
        ));
    }
    #[test]
    fn flat_stream_verifier_requires_public_material_and_authenticates_coordinates() {
        let (p, m, n) = primitive_fixture(6);
        let mut bad = Trial::of(p, m, vec![1], Some((0, 0, n, 128)));
        let unit = FlatReadV5::node(0, 0, 0);
        bad.server.missing = Some(unit);
        assert_eq!(bad.check(true), FlatFindingV5::Demand(unit));
        bad.server.missing = None;
        bad.server.borrowed = true;
        assert_eq!(bad.check(true), FlatFindingV5::Demand(unit));
        bad.server.borrowed = false;
        let hash = FlatReadV5 { node: n, leaf: Some(0), ..unit };
        bad.server.missing = Some(hash);
        assert_eq!(bad.check(true), FlatFindingV5::Demand(hash));
        bad.server.missing = None;
        bad.assert_fault(true, (0, 0, n));
        let mut tokens = bad.prompt.clone();
        tokens[0] += 1;
        assert!(matches!(
            check_flat_claim_v5(&bad.context(true), &bad.server, &|i, l| bad.m.tensors.get(&(i, l)).cloned(), &tokens),
            FlatFindingV5::Inconsistent(_)
        ));
    }

    #[test]
    fn flat_stream_verifier_wide_internal_and_small_downstream_relations() {
        use misaka_palw_tir::{
            builder::ProgramBuilder,
            program::{HISTORY_BOUND_V1_SMALL, Ref},
        };
        let mut pb = ProgramBuilder::new(256, HISTORY_BOUND_V1_SMALL);
        let w = pb.param("source", DType::I64, &[129], false);
        let mut pre = pb.block("pre", vec![]);
        let wide = pre.cast(w, DType::I128);
        let small = pre.slice(wide, 0, 123, 6);
        let out = pre.clamp(small, -100, 100, DType::I32);
        let ty = pre.ty(out);
        let pre = pre.finish(&[out]);
        let mut post = pb.block("post", vec![ty]);
        let out = post.cast(Ref::CarryIn(0), DType::I32);
        post.commit(out);
        let post = post.finish(&[]);
        let p = pb.finish(pre, vec![], post, 0);
        let mut m = MapParams::default();
        m.tensors.insert((0, None), Tensor::new(DType::I64, vec![129], (0..129).map(|i| i - 64).collect()).unwrap());
        assert_eq!(Trial::of(p.clone(), m.clone(), vec![1], None).check(true), FlatFindingV5::Clean);
        let bad = Trial::of(p.clone(), m.clone(), vec![1], Some((0, 0, 0, 128)));
        assert!(matches!(bad.assert_fault(true, (0, 0, 0)), FlatFaultV5::Leaf(_)));
        let bad = Trial::of(p, m, vec![1], Some((0, 0, 1, 5)));
        let FlatFaultV5::Whole(f) = bad.assert_fault(true, (0, 0, 1)) else { panic!("small relation") };
        assert_eq!(f.inputs[0].flat[0].opening.dtype, DType::I128.tag());
    }

    #[test]
    fn flat_stream_verifier_generated_ids_feed_later_state_and_history() {
        let (p, m, wn, hn) = state_fixture();
        for node in [wn, hn] {
            for wrong in [None, Some((3, 0, node, 128))] {
                let mut t = Trial::of(p.clone(), m.clone(), vec![1, 0, 0, 0], wrong);
                t.prompt = vec![1];
                t.generated = vec![0; 4];
                if wrong.is_none() {
                    assert_eq!(t.check(false), FlatFindingV5::Clean);
                } else {
                    t.assert_fault(false, (3, 0, node));
                }
            }
        }
    }

    #[test]
    fn flat_stream_verifier_encoder_uses_job_ids_padding_and_count() {
        use crate::seg_encoder::*;
        use misaka_palw_tir::{
            builder::ProgramBuilder,
            program::{HISTORY_BOUND_V1_SMALL, Ref},
        };
        let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
        let w = pb.param("embedding", DType::I16, &[4, 2], false);
        let ids = pb.param(ENCODER_IDS_PARAM_V1, DType::Idx, &[129], false);
        let count = pb.param(ENCODER_COUNT_PARAM_V1, DType::Idx, &[], false);
        let mut pre = pb.block("pre", vec![]);
        let x = pre.gather(w, ids, 0, 0);
        let count = pre.cast(count, DType::I16);
        let y = pre.add(x, count, DType::I32);
        let ty = pre.ty(y);
        let pre = pre.finish(&[y]);
        let mut post = pb.block("post", vec![ty]);
        let y = post.cast(Ref::CarryIn(0), DType::I32);
        post.commit(y);
        let post = post.finish(&[]);
        let p = pb.finish(pre, vec![], post, 0);
        let e = encoder_binding_v1(&p).unwrap();
        let mut m = MapParams::default();
        m.tensors.insert((0, None), Tensor::new(DType::I16, vec![4, 2], (0..8).collect()).unwrap());
        let pc = ParamCommitmentsV1::of_v3(&m);
        let prompt = vec![1, 3];
        for wrong in [None, Some((0usize, 257usize)), Some((1, 0)), Some((2, 257))] {
            let mut trace = trace_encoder_v1(&p, &m, &e, &prompt).unwrap();
            if let Some((n, i)) = wrong {
                trace.values[0][0][n].data[i] += 1;
            }
            let c = SegmentedCommitmentsV1::new(
                trace
                    .values
                    .iter()
                    .map(|pos| pos.iter().map(|occ| occ.iter().map(|t| flat_profile_commitment_v5(t).unwrap()).collect()).collect())
                    .collect(),
            );
            let roots = c.segment_roots();
            let server = Server { c, values: trace.values, missing: None, borrowed: false, hashes: Cell::new(0) };
            for inline in [true, false] {
                let ctx = FlatSegContextV5 {
                    profile: flat_trace_profile_id_v5(),
                    claim: SegClaimContextV1 {
                        program: &p,
                        params: &pc,
                        segment_roots: &roots,
                        positions: 1,
                        prompt_len: 2,
                        prompt_root: prompt_root_of_ids_v1(&prompt),
                        inline_prompt: inline.then_some(&prompt),
                        generated: &[],
                        decode: crate::job::DecodeRuleV1::Greedy,
                        encoder: Some(e),
                    },
                };
                let finding = check_flat_claim_v5(&ctx, &server, &|i, l| m.tensors.get(&(i, l)).cloned(), &prompt);
                match wrong {
                    None => assert_eq!(finding, FlatFindingV5::Clean),
                    Some((n, _)) => {
                        let FlatFindingV5::Fault(f) = finding else { panic!("encoder not convicted: {finding:?}") };
                        let v = verify_flat_fault_v5(&ctx, &f).unwrap();
                        assert_eq!((v.position, v.occurrence, v.node), (0, 0, n as u16));
                    }
                }
            }
        }
    }
}
