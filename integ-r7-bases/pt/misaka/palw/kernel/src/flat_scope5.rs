//! All-position/all-leaf resource admission for the staged internal flat-v5
//! profile. No shipping class or route selects this profile. This certificate
//! prices terminal courts and material; it is not a complete verifier, DA,
//! network admission, model fidelity or Final certificate.
use crate::element::PreparedV1;
use crate::flat_court4::{self as common, FlatCourtBoundsV4};
use crate::flat_court5::{decode_bounds_program, flat_trace_profile_id_v5, malformed_bounds};
use crate::hash::{Digest, object_id};
use crate::merkle3::LayoutV3;
use crate::seg_encoder::EncoderBindingV1;
use crate::verify::DismissalV1;
use borsh::BorshSerialize;
use misaka_palw_tir::program::TirProgramV1;

pub const FLAT_SCOPE_DOMAIN_V5: &[u8] = b"misaka-palw/kernel/flat-resource-scope/v5";
pub const FLAT_SCOPE_PROGRAM_DOMAIN_V5: &[u8] = b"misaka-palw/kernel/flat-resource-program/v5";
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatRelationScopeV5 {
    pub block: u8,
    pub node: u16,
    pub occurrences: u32,
    pub filing_bytes: u64,
    pub work: u64,
    pub scratch_bytes: u64,
    pub operand_leaves: Vec<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatResourceScopeV5 {
    pub version: u16,
    pub profile: Digest,
    pub program: Digest,
    pub max_positions: u32,
    /// Empty for a decoder, otherwise the exact derived (first_input, L).
    pub encoder: Option<(u16, u32)>,
    /// One entry per reachable block node, not per layer/position/leaf.
    pub relations: Vec<FlatRelationScopeV5>,
    pub relation_instances_per_position: u64,
    pub relation_instances_per_claim: u128,
    pub worst_court_bytes: u64,
    pub worst_court_work: u64,
    pub worst_court_scratch_bytes: u64,
    pub worst_operand_leaves: u64,
    /// Conservative independent maxima multiplied by the maximum positions.
    /// These are computation/retention prices, never public clear-byte promises.
    pub trace_value_bytes_per_position: u128,
    pub trace_value_bytes_per_claim: u128,
    pub node_commitments_bytes_per_claim: u128,
    pub flat_tree_bytes_per_position: u128,
    pub flat_hash_input_bytes_per_claim: u128,
}
impl FlatResourceScopeV5 {
    pub fn root(&self) -> Digest {
        object_id(FLAT_SCOPE_DOMAIN_V5, self)
    }
}
fn na(s: &str) -> DismissalV1 {
    DismissalV1::NotAuthentic(s.into())
}
fn cap(mut b: FlatCourtBoundsV4) -> Result<FlatCourtBoundsV4, DismissalV1> {
    b.filing_bytes = b.filing_bytes.checked_add(3).ok_or_else(|| na("scope byte overflow"))?;
    if b.filing_bytes > common::FLAT_FAULT_MAX_BYTES_V4
        || b.work > common::FLAT_FAULT_MAX_WORK_V4
        || b.scratch_bytes > common::FLAT_FAULT_MAX_SCRATCH_V4
        || b.operand_leaves.iter().sum::<u64>() > common::FLAT_FAULT_MAX_LEAVES_V4 as u64
    {
        return Err(na("complete flat-v5 court scope exceeds resource caps"));
    }
    Ok(b)
}
/// Validate once and derive resource ceilings without allocating tensors,
/// parameter images, position arrays or any Merkle tree. Validated v1 shapes
/// have at most one monotone H. The shared envelope prices both node grammars,
/// every possible tail, previous-state writers and maximum reference lines.
pub fn flat_resource_scope_v5(
    program: &TirProgramV1,
    max_positions: u32,
    encoder: Option<&EncoderBindingV1>,
) -> Result<FlatResourceScopeV5, DismissalV1> {
    if max_positions == 0 || max_positions > program.history_bound || max_positions > misaka_palw_tir::program::HISTORY_BOUND_V1_HELD {
        return Err(na("invalid flat scope extent"));
    }
    let program_bytes = common::size_with_limit(program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64)
        .map_err(|_| na("oversized flat scope program"))?;
    if let Some(e) = encoder
        && (max_positions != 1 || crate::seg_encoder::encoder_binding_v1(program).ok().as_ref() != Some(e))
    {
        return Err(na("wrong flat scope encoder binding"));
    }
    let pr = PreparedV1::without_scope(program, encoder).map_err(DismissalV1::NotAuthentic)?;
    let p = max_positions - 1;
    let mut out = FlatResourceScopeV5 {
        version: 5,
        profile: flat_trace_profile_id_v5(),
        program: object_id(FLAT_SCOPE_PROGRAM_DOMAIN_V5, program),
        max_positions,
        encoder: encoder.map(|e| (e.first_input, e.l)),
        relations: Vec::new(),
        relation_instances_per_position: pr.node_count,
        relation_instances_per_claim: pr.node_count as u128 * max_positions as u128,
        worst_court_bytes: 0,
        worst_court_work: 0,
        worst_court_scratch_bytes: 0,
        worst_operand_leaves: 0,
        trace_value_bytes_per_position: 0,
        trace_value_bytes_per_claim: 0,
        node_commitments_bytes_per_claim: pr.node_count as u128 * max_positions as u128 * 64,
        flat_tree_bytes_per_position: 0,
        flat_hash_input_bytes_per_claim: 0,
    };
    let mut counts = vec![0u32; program.blocks.len()];
    let mut representative = vec![None; program.blocks.len()];
    for (s, (b, _)) in pr.w.occurrences.iter().enumerate() {
        counts[*b as usize] += 1;
        representative[*b as usize].get_or_insert(s as u16);
    }
    for (b, s) in representative.into_iter().enumerate() {
        let Some(s) = s else { continue };
        for (n, node) in program.blocks[b].nodes.iter().enumerate() {
            let n = n as u16;
            let sh = pr.shapes(p, s, n);
            let len = LayoutV3::of(&sh.out).len;
            let shape: Vec<_> = sh.out.iter().map(|d| *d as u64).collect();
            let material = if len >= 64 { crate::merkle5::flat_material_bounds_v5(node.out.dtype, &shape) } else { None };
            let output_bytes = material.map(|m| m.leaf_hash_bytes).unwrap_or(0);
            // 127 bounds every large tail; below 64 every whole result is
            // smaller. This also covers the pre-threshold small output regime.
            let count = if len >= 64 { 127 } else { len };
            let bound = cap(common::relation_bounds_program::<5>(
                program,
                max_positions,
                &pr,
                p,
                s,
                n,
                count,
                output_bytes,
                true,
                program_bytes,
            )?)?;
            maxima(&mut out, &bound);
            out.relations.push(FlatRelationScopeV5 {
                block: b as u8,
                node: n,
                occurrences: counts[b],
                filing_bytes: bound.filing_bytes,
                work: bound.work,
                scratch_bytes: bound.scratch_bytes,
                operand_leaves: bound.operand_leaves,
            });
            out.trace_value_bytes_per_position += len as u128 * node.out.dtype.width() as u128 * counts[b] as u128;
            if let Some(m) = material {
                out.flat_tree_bytes_per_position += m.retained_tree_bytes * counts[b] as u128;
                out.flat_hash_input_bytes_per_claim += m.hash_input_bytes * counts[b] as u128 * max_positions as u128;
            }
        }
    }
    // Logits cannot contain H in v1, so this exact bound is position invariant.
    maxima(&mut out, &cap(decode_bounds_program(program, &pr, p, (pr.w.occurrences.len() - 1) as u16, program.logits)?)?);
    maxima(&mut out, &cap(malformed_bounds(program, &pr)?)?);
    out.trace_value_bytes_per_claim = out.trace_value_bytes_per_position * max_positions as u128;
    Ok(out)
}
fn maxima(out: &mut FlatResourceScopeV5, b: &FlatCourtBoundsV4) {
    out.worst_court_bytes = out.worst_court_bytes.max(b.filing_bytes);
    out.worst_court_work = out.worst_court_work.max(b.work);
    out.worst_court_scratch_bytes = out.worst_court_scratch_bytes.max(b.scratch_bytes);
    out.worst_operand_leaves = out.worst_operand_leaves.max(b.operand_leaves.iter().sum());
}
/// An untrusted declaration is accepted only if every field is exactly the
/// independently derived scope. There is no registrant-selected omission,
/// smaller budget or profile/extent substitution.
pub fn check_flat_resource_scope_v5(
    program: &TirProgramV1,
    max_positions: u32,
    encoder: Option<&EncoderBindingV1>,
    declared: &FlatResourceScopeV5,
) -> Result<(), DismissalV1> {
    if declared.relations.len() > misaka_palw_tir::program::MAX_BLOCKS * misaka_palw_tir::program::MAX_NODES_PER_BLOCK
        || declared.relations.iter().any(|r| r.operand_leaves.len() > 9)
    {
        return Err(na("oversized flat resource declaration"));
    }
    if flat_resource_scope_v5(program, max_positions, encoder)? != *declared {
        return Err(na("flat resource declaration does not match derivation"));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::element::SegClaimContextV1;
    use crate::flat_court4::tests::{identity, primitive_fixture, state_fixture};
    use crate::flat_court5::{FlatSegContextV5, flat_leaf_court_bounds_v5, flat_whole_court_bounds_v5};
    use crate::job::DecodeRuleV1;
    use crate::trace::ParamCommitmentsV1;
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::{HISTORY_BOUND_V1_HELD, HISTORY_BOUND_V1_SMALL, Ref};
    use misaka_palw_tir::{DType, Dim, TensorType};
    fn assert_envelope(program: &TirProgramV1, positions: u32) -> FlatResourceScopeV5 {
        let report = flat_resource_scope_v5(program, positions, None).unwrap();
        let params = ParamCommitmentsV1::default();
        let roots = vec![[0; 64]; crate::seg::segment_count_v1(positions) as usize];
        let c = FlatSegContextV5 {
            profile: flat_trace_profile_id_v5(),
            claim: SegClaimContextV1 {
                program,
                params: &params,
                segment_roots: &roots,
                positions,
                prompt_len: positions,
                prompt_root: [0; 64],
                inline_prompt: None,
                generated: &[],
                decode: DecodeRuleV1::Greedy,
                encoder: None,
            },
        };
        let pr = common::prepare(&c).unwrap();
        for p in 0..positions {
            for (s, (block, _)) in pr.w.occurrences.iter().enumerate() {
                for n in 0..program.blocks[*block as usize].nodes.len() {
                    let bound = report.relations.iter().find(|r| r.block == *block && r.node == n as u16).unwrap();
                    let (_, shape) = pr.declared(s as u16, n as u16, p);
                    let len = LayoutV3::of(&shape).len;
                    let leaves = if len < 64 { 1 } else { len / 64 };
                    for leaf in 0..leaves {
                        let actual = if len < 64 {
                            flat_whole_court_bounds_v5(&c, p, s as u16, n as u16)
                        } else {
                            flat_leaf_court_bounds_v5(&c, p, s as u16, n as u16, leaf)
                        }
                        .unwrap();
                        assert!(
                            actual.filing_bytes + 3 <= bound.filing_bytes,
                            "bytes p={p} s={s} n={n} leaf={leaf}: {actual:?} > {bound:?}"
                        );
                        assert!(actual.work <= bound.work, "work p={p} s={s} n={n} leaf={leaf}");
                        assert!(actual.scratch_bytes <= bound.scratch_bytes, "scratch p={p} s={s} n={n} leaf={leaf}");
                        assert!(actual.operand_leaves.len() <= bound.operand_leaves.len());
                        for (a, b) in actual.operand_leaves.iter().zip(&bound.operand_leaves) {
                            assert!(a <= b, "operand p={p} s={s} n={n} leaf={leaf}: {a}>{b}");
                        }
                    }
                }
            }
        }
        report
    }
    pub(crate) fn history(row: u32, window: u32) -> TirProgramV1 {
        let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
        let state = pb.hist_state("history", DType::I16, &[row], window, false);
        let mut pre = pb.block("pre", vec![]);
        let x = pre.cast(Ref::Input(0), DType::I16);
        let x = pre.broadcast(x, &[Dim::Fixed(row)]);
        let h = pre.hist_append(state, x);
        let h = pre.transpose(h, &[1, 0]);
        let wide = pre.cast(h, DType::I128);
        let sum = pre.reduce_sum(wide, 1, DType::I128);
        let sum = pre.clamp(sum, -100, 100, DType::I32);
        let pre = pre.finish(&[sum]);
        let mut post = pb.block("post", vec![TensorType::fixed(DType::I32, &[row, 1])]);
        let y = post.cast(Ref::CarryIn(0), DType::I32);
        post.commit(y);
        let post = post.finish(&[]);
        pb.finish(pre, vec![], post, 0)
    }
    #[test]
    fn scope_v5_bounds_every_position_leaf_primitive_and_mixed_history_grammar() {
        for tag in 0..23 {
            let (p, _, _) = primitive_fixture(tag);
            assert_envelope(&p, 3);
        }
        let (p, _, _, _) = state_fixture();
        assert_envelope(&p, 5);
        for row in [1, 2, 3, 7, 31, 63, 64, 65, 127, 129] {
            for window in [1, 2, 65] {
                assert_envelope(&history(row, window), window + 2);
            }
        }
    }
    #[test]
    fn scope_v5_includes_earlier_small_row_paths_and_earlier_127_element_tails() {
        let p = history(1, 128);
        let scope = assert_envelope(&p, 130);
        // At the last position H=128: flat leaves have 64 elements. At H=127
        // the single tail has 127; before H=64 each [H,1] row was a v3 leaf.
        let cast = scope.relations.iter().find(|r| r.block == 0 && r.node == 3).unwrap();
        assert!(cast.operand_leaves[0] >= 63, "small row paths must survive final flat shape");
        assert!(cast.filing_bytes > 63 * 500);
    }
    #[test]
    fn scope_v5_rederives_all_declarations_and_binds_program_profile_extent_and_occurrences() {
        let (p, _) = identity(129);
        let scope = assert_envelope(&p, 3);
        check_flat_resource_scope_v5(&p, 3, None, &scope).unwrap();
        let mut variants = vec![];
        let mut x = scope.clone();
        x.version = 4;
        variants.push(x);
        let mut x = scope.clone();
        x.profile[0] ^= 1;
        variants.push(x);
        let mut x = scope.clone();
        x.program[0] ^= 1;
        variants.push(x);
        let mut x = scope.clone();
        x.max_positions = 2;
        variants.push(x);
        let mut x = scope.clone();
        x.relations.pop();
        variants.push(x);
        let mut x = scope.clone();
        x.relations[0].filing_bytes -= 1;
        variants.push(x);
        let mut x = scope.clone();
        x.relations[0].work -= 1;
        variants.push(x);
        let mut x = scope.clone();
        x.relations[0].scratch_bytes -= 1;
        variants.push(x);
        let mut x = scope.clone();
        x.relations[0].occurrences += 1;
        variants.push(x);
        let mut x = scope.clone();
        x.relations[0].operand_leaves.clear();
        variants.push(x);
        let mut x = scope.clone();
        x.trace_value_bytes_per_claim -= 1;
        variants.push(x);
        let mut x = scope.clone();
        x.worst_court_bytes -= 1;
        variants.push(x);
        let mut x = scope.clone();
        x.encoder = Some((0, 1));
        variants.push(x);
        for x in variants {
            assert!(check_flat_resource_scope_v5(&p, 3, None, &x).is_err());
            assert_ne!(x.root(), scope.root());
        }
        assert!(flat_resource_scope_v5(&p, 0, None).is_err());
        assert!(flat_resource_scope_v5(&p, p.history_bound + 1, None).is_err());
        let mut bad = p.clone();
        bad.blocks[0].nodes[0].out.shape[0] = Dim::Fixed(0);
        assert!(flat_resource_scope_v5(&bad, 1, None).is_err());
    }
    #[test]
    fn scope_v5_prices_huge_job_extents_and_public_parameter_shapes_without_images_or_position_arrays() {
        let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_HELD);
        let table = pb.param("external.table", DType::I8, &[1 << 24], false);
        let ids = pb.konst(DType::Idx, &[129], &vec![0; 129]);
        let mut pre = pb.block("pre", vec![]);
        let x = pre.gather(table, ids, 0, 0);
        let pre = pre.finish(&[x]);
        let mut post = pb.block("post", vec![TensorType::fixed(DType::I8, &[129])]);
        let y = post.cast(Ref::CarryIn(0), DType::I8);
        post.commit(y);
        let post = post.finish(&[]);
        let p = pb.finish(pre, vec![], post, 0);
        let a = flat_resource_scope_v5(&p, 1, None).unwrap();
        let b = flat_resource_scope_v5(&p, HISTORY_BOUND_V1_HELD, None).unwrap();
        assert_eq!(a.relations, b.relations);
        assert_eq!(b.relations.len(), 2);
        assert_eq!(b.relation_instances_per_claim, 2 * (HISTORY_BOUND_V1_HELD as u128));
        assert_eq!(b.trace_value_bytes_per_claim, a.trace_value_bytes_per_claim * HISTORY_BOUND_V1_HELD as u128);
        assert_ne!(a.root(), b.root());
        // No parameter storage or token/position/root array is passed to admission.
    }
    #[test]
    fn scope_v5_charges_constant_sort_reference_work_and_refuses_oversized_lines() {
        fn sort(n: u32) -> TirProgramV1 {
            let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
            let x = pb.konst(DType::I8, &[n], &vec![7; n as usize]);
            let mut pre = pb.block("pre", vec![]);
            let x = pre.topk(x, 0, 64);
            let pre = pre.finish(&[x]);
            let mut post = pb.block("post", vec![TensorType::fixed(DType::Idx, &[64])]);
            let y = post.cast(Ref::CarryIn(0), DType::I32);
            post.commit(y);
            let post = post.finish(&[]);
            pb.finish(pre, vec![], post, 0)
        }
        let small = sort(4096);
        let scope = assert_envelope(&small, 2);
        let sort_bound = scope.relations.iter().find(|r| r.block == 0).unwrap();
        assert!(sort_bound.operand_leaves.iter().all(|n| *n == 0));
        assert!(sort_bound.work >= 127 * 4096 * (64 + 12 * 64));
        let oversized = sort_program_large();
        assert!(misaka_palw_tir::validate::validate(&oversized).is_ok());
        assert!(flat_resource_scope_v5(&oversized, 1, None).is_err());
        let params = ParamCommitmentsV1::default();
        let roots = [[0; 64]];
        let c = FlatSegContextV5 {
            profile: flat_trace_profile_id_v5(),
            claim: SegClaimContextV1 {
                program: &oversized,
                params: &params,
                segment_roots: &roots,
                positions: 1,
                prompt_len: 1,
                prompt_root: [0; 64],
                inline_prompt: None,
                generated: &[],
                decode: DecodeRuleV1::Greedy,
                encoder: None,
            },
        };
        // The exact per-filing guard must refuse as well, before reference sorting.
        assert!(flat_leaf_court_bounds_v5(&c, 0, 0, 0, 0).is_err());
        fn sort_program_large() -> TirProgramV1 {
            sort(65536)
        }
    }
    #[test]
    fn scope_v5_deduplicates_layer_templates_but_prices_every_state_instance() {
        fn layered(layers: usize) -> TirProgramV1 {
            let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_HELD);
            let state = pb.fixed_state("accumulator", DType::I16, &[129], -10, 10, true);
            let hist = pb.hist_state("history", DType::I16, &[1], 65, true);
            let mut pre = pb.block("pre", vec![]);
            let x = pre.cast(Ref::Input(0), DType::I16);
            let x = pre.broadcast(x, &[Dim::Fixed(129)]);
            let pre = pre.finish(&[x]);
            let carry = TensorType::fixed(DType::I16, &[129]);
            let mut body = pb.block("layer", vec![carry.clone()]);
            let x = body.add(Ref::CarryIn(0), Ref::State(state), DType::I32);
            let x = body.state_write(state, x);
            let row = body.slice(x, 0, 0, 1);
            body.hist_append(hist, row);
            let body = body.finish(&[x]);
            let mut post = pb.block("post", vec![carry]);
            let y = post.cast(Ref::CarryIn(0), DType::I16);
            post.commit(y);
            let post = post.finish(&[]);
            pb.finish(pre, vec![body; layers], post, 0)
        }
        let p = layered(3);
        assert_envelope(&p, 5);
        let p = layered(1024);
        let scope = flat_resource_scope_v5(&p, HISTORY_BOUND_V1_HELD, None).unwrap();
        assert_eq!(scope.relations.len(), 7);
        assert!(scope.relations.iter().filter(|r| r.block == 1).all(|r| r.occurrences == 1024));
        assert_eq!(scope.relation_instances_per_position, 2 + 1024 * 4 + 1);
        assert_eq!(scope.relation_instances_per_claim, (2 + 1024 * 4 + 1) as u128 * HISTORY_BOUND_V1_HELD as u128);
    }
    #[test]
    fn scope_v5_encoder_scope_binds_job_mode_axis_and_single_position() {
        let mut pb = ProgramBuilder::new(5, HISTORY_BOUND_V1_SMALL);
        let ids = pb.param(crate::seg_encoder::ENCODER_IDS_PARAM_V1, DType::Idx, &[65], false);
        let count = pb.param(crate::seg_encoder::ENCODER_COUNT_PARAM_V1, DType::Idx, &[], false);
        let mut pre = pb.block("pre", vec![]);
        let x = pre.add(ids, count, DType::I32);
        let pre = pre.finish(&[x]);
        let mut post = pb.block("post", vec![TensorType::fixed(DType::I32, &[65])]);
        let y = post.cast(Ref::CarryIn(0), DType::I32);
        post.commit(y);
        let post = post.finish(&[]);
        let p = pb.finish(pre, vec![], post, 0);
        let binding = crate::seg_encoder::encoder_binding_v1(&p).unwrap();
        let scope = flat_resource_scope_v5(&p, 1, Some(&binding)).unwrap();
        assert_eq!(scope.encoder, Some((binding.first_input, 65)));
        assert!(scope.relations[0].operand_leaves.iter().all(|n| *n == 0));
        check_flat_resource_scope_v5(&p, 1, Some(&binding), &scope).unwrap();
        assert!(check_flat_resource_scope_v5(&p, 1, None, &scope).is_err());
        assert!(flat_resource_scope_v5(&p, 2, Some(&binding)).is_err());
        let mut wrong = binding;
        wrong.l = 66;
        assert!(flat_resource_scope_v5(&p, 1, Some(&wrong)).is_err());
        wrong = binding;
        wrong.first_input += 1;
        assert!(flat_resource_scope_v5(&p, 1, Some(&wrong)).is_err());
    }
}
