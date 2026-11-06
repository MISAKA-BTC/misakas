//! A separately declared finite stateful program, not a registered/native model.
//! Values come from whole reference execution/cones; Node receives only real
//! inventory and step-tree openings, never the test read-set discovery source.
use super::*;
use crate::palw_tir_court_v1::source_only::wide_v2::test_support::{ReadSet, read_set};
use misaka_palw_tir::{ConeEnv, StateKind};
use std::collections::BTreeMap;

type Relation = crate::palw_positive_check_v2::expanded_plan::ExpandedRelation;
struct Mixed {
    base: Fixture,
    prompt: Vec<u32>,
    values: BTreeMap<(u32, u16, u16), Tensor>,
    leaves: Vec<PalwTirLeafV1>,
    leaf_values: Vec<Vec<i128>>,
}
fn mixed(window: u32) -> Mixed {
    let mut pb = ProgramBuilder::new(4, 1 << 18);
    let embed = pb.param("stateful_embedding", DType::I8, &[4, 4], false);
    let fixed = pb.fixed_state("accumulator", DType::I8, &[4], -128, 127, false);
    let hist = pb.hist_state("rows", DType::I8, &[4], window, false);
    let factor = pb.scalar(DType::I64, 1i128 << 40);
    let (pre, appended) = {
        let mut b = pb.block("stateful_pre", vec![]);
        let x = b.gather(embed, Ref::Input(0), 0, 0);
        b.commit(x);
        let rows = b.hist_append(hist, x);
        let sum = b.reduce_sum(rows, 0, DType::I32);
        let sum = b.reshape_fixed(sum, &[4]);
        let acc = b.add(Ref::State(fixed), sum, DType::I32);
        let bounded = b.clamp(acc, -128, 127, DType::I8);
        b.state_write(fixed, bounded);
        let Ref::Node(xi) = x else { unreachable!() };
        (b.finish(&[bounded]), xi)
    };
    let (post, logits) = {
        let mut b = pb.block("wide_post", vec![TensorType::fixed(DType::I8, &[4])]);
        let x = b.cast(Ref::CarryIn(0), DType::I64);
        let y = b.mul(x, factor, DType::I64);
        b.commit(y);
        let Ref::Node(yi) = y else { unreachable!() };
        (b.finish(&[]), yi)
    };
    let mut program = pb.finish(pre, vec![], post, logits);
    program.logits_scheme_id = misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2;
    misaka_palw_tir::interval::analyze_ranges(&program).unwrap();
    let committed = program.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).count();
    let class = PalwTirClassV1 {
        version: 1,
        program: program.encode(),
        tokenizer_id: Hash64::from_u64_word(718),
        layout: PalwTirLayoutV1 {
            version: 1,
            max_context: 64,
            checkpoint_interval: 2,
            h_tile: 2,
            commit_tiles: vec![4; committed],
            state_tiles: vec![4, 4],
        },
    };
    let mut params = MapParams::default();
    params
        .tensors
        .insert((0, None), Tensor::new(DType::I8, vec![4, 4], vec![1, 2, 3, 4, 2, 4, 1, 3, 3, 1, 4, 2, 4, 3, 2, 1]).unwrap());
    let operands = palw_tir_inventory_operands_v1(&program, &Src(&params)).unwrap();
    let artifact = artifact_root_v1(&operands.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
    let (job, prompt) = crate::palw_tir_profile_resolver_v2::ResolvedTirJobProfileV2::new(&class, artifact, 1 << 18)
        .unwrap()
        .job_for_anchor(&Hash64::from_u64_word(731), PalwPromptIdsFormV1::Flat)
        .unwrap();
    assert_eq!((job.declared_prefill_tokens, job.exact_decode_tokens), (7, 2));
    let positions = job.declared_prefill_tokens + job.exact_decode_tokens - 1;
    let interp = Interpreter::new(&program).unwrap();
    let mut state = RunState::default();
    let (mut rows, mut generated, mut tokens, mut commits, mut after, mut raw_hist, mut carries) =
        (vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
    let mut values = BTreeMap::new();
    for pos in 0..positions {
        let token = if pos < job.declared_prefill_tokens { prompt[pos as usize] } else { *generated.last().unwrap() };
        let before = state.clone();
        let step = interp.step(&params, &mut state, token).unwrap();
        let carry_node = program.blocks[pre as usize].carry_out[0];
        let carry = step.commits.iter().find(|c| c.block == pre && c.node == carry_node).unwrap().value.clone();
        carries.push(carry.data.clone());
        raw_hist.push(step.commits.iter().find(|c| c.block == pre && c.node == appended).unwrap().value.data.clone());
        // Even window=1 deliberately loses prior rows in RunState. The separate
        // raw append archive constructs historical tiles, not a fake state snapshot.
        assert_eq!(state.hist[&(hist, None)].len(), (window as usize - 1).min(pos as usize + 1));
        for (occ, (block, layer)) in program.occurrences().into_iter().enumerate() {
            if block == post && pos + 1 < job.declared_prefill_tokens {
                continue;
            }
            let env =
                ConeEnv {
                    token: Some(token),
                    pos,
                    carry_in: if occ == 0 { BTreeMap::new() } else { BTreeMap::from([(0, carry.clone())]) },
                    fixed: program
                        .states
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| matches!(s.kind, StateKind::Fixed { .. }))
                        .map(|(i, s)| {
                            (
                                i as u16,
                                before.fixed.get(&(i as u16, layer)).cloned().unwrap_or_else(|| {
                                    Tensor::zeros(s.dtype, &s.shape.iter().map(|n| *n as usize).collect::<Vec<_>>())
                                }),
                            )
                        })
                        .collect(),
                    hist_prior: program
                        .states
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| matches!(s.kind, StateKind::Hist { .. }))
                        .map(|(i, _)| {
                            (i as u16, before.hist.get(&(i as u16, layer)).map(|r| r.iter().cloned().collect()).unwrap_or_default())
                        })
                        .collect(),
                    supplied: BTreeMap::new(),
                };
            for node in 0..program.blocks[block as usize].nodes.len() {
                values.insert((pos, occ as u16, node as u16), interp.eval_cone(block, layer, node as u16, &params, &env).unwrap());
            }
        }
        if pos + 1 >= job.declared_prefill_tokens {
            let row = step.logits.data.iter().map(|v| i64::try_from(*v).unwrap()).collect::<Vec<_>>();
            let selected = row.iter().enumerate().reduce(|a, b| if b.1 > a.1 { b } else { a }).unwrap().0 as u32;
            rows.push(row);
            generated.push(selected);
        }
        tokens.push(token);
        commits.push(step.commits);
        after.push(state.clone());
    }
    let space = PalwTirStepSpaceV2::new(&class).unwrap();
    let mut builder = PalwTirStepLegBuilderV2::new(&space, &job, class.class_id(&artifact), 1000).unwrap();
    let (mut preimages, mut hashes, mut leaves, mut leaf_values) = (vec![], vec![], vec![], vec![]);
    while let Some(leaf) = builder.next_leaf() {
        let data = match leaf.kind {
            PalwTirLeafKindV1::Commit { block, layer, node, first_element, .. } => {
                let v = &commits[leaf.position as usize]
                    .iter()
                    .find(|c| c.block == block && c.layer == layer && c.node == node)
                    .unwrap()
                    .value
                    .data;
                v[first_element as usize..first_element as usize + leaf.value_count as usize].to_vec()
            }
            PalwTirLeafKindV1::State { state, layer, first_element, .. } => {
                let v = &after[leaf.position as usize].fixed[&(state, layer)].data;
                v[first_element as usize..first_element as usize + leaf.value_count as usize].to_vec()
            }
            PalwTirLeafKindV1::HistTile { state, layer, first_lane, row_lanes, first_position, .. } => {
                assert_eq!((state, layer), (hist, None));
                (first_position..=leaf.position)
                    .flat_map(|p| raw_hist[p as usize][first_lane as usize..first_lane as usize + row_lanes as usize].iter().copied())
                    .collect()
            }
        };
        let image = palw_tir_leaf_preimage_v2(&space, &job, &leaf, &data).unwrap();
        hashes.push(step_tile_leaf_hash_v1(&job.context_hash(), &class.class_id(&artifact), &image));
        preimages.push(image);
        leaves.push(leaf);
        leaf_values.push(data.clone());
        builder.push(&data).unwrap();
    }
    let (count, root) = builder.finish().unwrap();
    let trace = crate::palw_logits_q24_v2::trace_root(&job, &rows, &generated).unwrap();
    let binding = PalwTirStepBindingV1 {
        version: 1,
        committed_execution_root: crate::palw_tir_step_v1::palw_tir_execution_root_v1(
            &job.context_hash(),
            &trace,
            &class.class_id(&artifact),
            count,
            &root,
        ),
        job_context: job,
        class,
        artifact_root: artifact,
        full_logits_trace_root: trace,
        step_leaf_count: count,
        step_merkle_root: root,
    };
    verify_tir_binding_v2(&binding, 1000).unwrap();
    Mixed {
        base: Fixture { binding, preimages, hashes, operands, tokens, carries, rows, generated },
        prompt,
        values,
        leaves,
        leaf_values,
    }
}
fn mixed_limits() -> FullRelationLimits {
    let mut l = limits();
    l.plan.max_relations = 1000;
    l.plan.max_legacy_leaves = 1000;
    l
}
fn mixed_checker(m: &Mixed, s: &Signed, alter_checkpoint: bool) -> FullRelationCheckerV2 {
    let prep = s.prep(&m.base, mixed_limits());
    let claims = prep
        .plan()
        .iter()
        .map(|r| match r.kind {
            ExpandedRelationKind::Legacy(l) => {
                let mut values = m.leaf_values[l.index as usize].clone();
                if alter_checkpoint && matches!(l.kind, PalwTirLeafKindV1::State { .. }) {
                    values[0] -= 1;
                }
                values
            }
            ExpandedRelationKind::RecomputeNode { ctx, node, first_lane, lane_count, .. } => {
                m.values[&(ctx.pos, ctx.occurrence, node)].data[first_lane as usize..first_lane as usize + lane_count as usize]
                    .to_vec()
            }
        })
        .collect();
    let panel = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([93; 32]);
    let record = prep
        .freeze_record(m.base.binding.clone(), m.base.generated.clone(), principal(panel.verification_key.as_ref()), claims)
        .unwrap();
    let signature =
        *libcrux_ml_dsa::ml_dsa_87::sign(&panel.signing_key, &record.signing_digest().0, record.signature_context(), [0; 32])
            .unwrap()
            .as_ref();
    prep.begin(record, panel.verification_key.as_ref(), &signature).unwrap()
}
fn closure(m: &Mixed, r: &Relation) -> ReadSet {
    let l = mixed_limits();
    read_set(
        &m.base.binding,
        r,
        &m.base.preimages,
        &m.base.operands,
        &m.prompt,
        &m.base.generated,
        &PalwTirCourtRulesV1 {
            max_step_leaf_count: l.plan.max_legacy_leaves,
            prompt_form: PalwPromptIdsFormV1::Flat,
            limits: DemandLimits { max_elements: l.plan.max_elements_per_relation, max_terms: l.plan.max_terms_per_relation },
        },
        l.source,
    )
}
fn operands(m: &Mixed, indices: &[u64]) -> PalwStepInputRowV1 {
    let mut siblings = vec![];
    let mut k = 0;
    while k < indices.len() {
        let mut count = 1;
        while k + count < indices.len() && indices[k + count] == indices[k] + count as u64 {
            count += 1;
        }
        siblings.push(step_merkle_range_siblings_v1(&m.base.hashes, indices[k] as usize, count).unwrap());
        k += count;
    }
    PalwStepInputRowV1 { preimages: indices.iter().map(|i| m.base.preimages[*i as usize].clone()).collect(), run_siblings: siblings }
}
fn mixed_material(m: &Mixed, read: &ReadSet) -> Material {
    Material {
        operands: operands(m, &read.steps),
        params: if read.params.is_empty() {
            PalwTirParamOpeningV1::None
        } else {
            let openings = read.params.iter().map(|i| open_artifact_leaf_v1(&m.base.operands, *i).unwrap()).collect::<Vec<_>>();
            PalwTirParamOpeningV1::Single(palw_artifact_multiproof_from_openings_v1(&openings).unwrap())
        },
        prompt: if read.prompt { m.prompt.clone() } else { vec![] },
        pin: read.decode.then(|| WideGeneratedIdsPinV2 {
            rows_root: crate::palw_logits_q24_v2::rows_root(&m.base.binding.job_context, &m.base.rows).unwrap(),
            generated_token_ids: m.base.generated.clone(),
        }),
    }
}
fn mixed_target(m: &Mixed, r: &Relation) -> Option<Target> {
    let ExpandedRelationKind::Legacy(leaf) = r.kind else {
        return None;
    };
    let trace = if leaf.dtype == DType::I64 {
        let row = leaf.position + 1 - m.base.binding.job_context.declared_prefill_tokens;
        let context = m.base.binding.job_context.context_hash();
        let roots = m
            .base
            .rows
            .iter()
            .enumerate()
            .map(|(i, l)| crate::palw_logits_q24_v2::row_root(&context, i as u32, l).unwrap())
            .collect::<Vec<_>>();
        let tile = crate::palw_logits_q24_v2::tile_leaf(&context, row, 0, &m.base.rows[row as usize]);
        Some(WideLogitsEventV2 {
            row,
            generated: m.base.generated.clone(),
            row_root: roots[row as usize],
            row_opening: step_opening_v1(&roots, u64::from(row)).unwrap(),
            lanes: m.base.rows[row as usize].clone(),
            tile_opening: step_opening_v1(&[tile], 0).unwrap(),
        })
    } else {
        None
    };
    Some(Target {
        opening: step_opening_v1(&m.base.hashes, leaf.index).unwrap(),
        preimage: m.base.preimages[leaf.index as usize].clone(),
        trace,
    })
}
fn next(m: &Mixed, c: &mut FullRelationCheckerV2) -> Result<(), FullRelationError> {
    let r = c.preparation().plan().relation_at(c.progress().next_relation).unwrap();
    let material = mixed_material(m, &closure(m, &r));
    let target = mixed_target(m, &r);
    c.submit(&material.view(), target.as_ref().map(Target::view))
}
#[test]
fn wide_stateful_windows_verify_fixed_history_and_partial_history_read_closure() {
    for window in [1, 3] {
        let m = mixed(window);
        let s = Signed::new(&m.base, false);
        let mut c = mixed_checker(&m, &s, false);
        let (mut fixed_reads, mut history_reads, mut partial_reads) = (0, 0, 0);
        let program = m.base.binding.class.decode_program().unwrap();
        assert!(m.leaves.iter().any(|l| matches!(l.kind, PalwTirLeafKindV1::State { .. })));
        assert!(m.leaves.iter().any(|l| matches!(l.kind, PalwTirLeafKindV1::HistTile { .. })));
        while c.progress().next_relation < c.preparation().plan().summary().relations {
            let r = c.preparation().plan().relation_at(c.progress().next_relation).unwrap();
            let read = closure(&m, &r);
            let position = match r.kind {
                ExpandedRelationKind::Legacy(l) => l.position,
                ExpandedRelationKind::RecomputeNode { ctx, .. } => ctx.pos,
            };
            let history_target = matches!(r.kind, ExpandedRelationKind::RecomputeNode { ctx, node, .. }
                if ctx.occurrence == 0 && matches!(program.blocks[program.schedule.pre as usize].nodes[node as usize].prim,
                    misaka_palw_tir::Prim::HistAppend { .. }));
            for i in &read.steps {
                let leaf = m.leaves[*i as usize];
                assert!(leaf.index < r.legacy_frontier);
                assert_ne!(leaf.dtype, DType::I64);
                match leaf.kind {
                    PalwTirLeafKindV1::State { .. } => fixed_reads += 1,
                    PalwTirLeafKindV1::HistTile { .. } => history_reads += 1,
                    PalwTirLeafKindV1::Commit { block, node, .. }
                        if history_target
                            && !position.is_multiple_of(2)
                            && leaf.position + 1 == position
                            && block == program.schedule.pre
                            && node == 0 =>
                    {
                        partial_reads += 1
                    }
                    _ => {}
                }
            }
            let material = mixed_material(&m, &read);
            let target = mixed_target(&m, &r);
            c.submit(&material.view(), target.as_ref().map(Target::view)).unwrap();
        }
        assert!(fixed_reads > 0);
        if window == 3 {
            assert!(history_reads > 0);
            assert!(partial_reads > 0);
        }
        assert_eq!(c.progress().checked_rows, 2);
        assert_eq!(c.finish().unwrap(), CheckOutcome::Matched);
    }
}
#[test]
fn dropped_corrupt_and_authenticated_stale_wide_checkpoint_cannot_advance_or_convict() {
    let m = mixed(3);
    let s = Signed::new(&m.base, false);
    let mut c = mixed_checker(&m, &s, false);
    let (r, read, current, stale) = loop {
        let r = c.preparation().plan().relation_at(c.progress().next_relation).unwrap();
        let read = closure(&m, &r);
        let pair=read.steps.iter().find_map(|i| {
            let current=m.leaves[*i as usize];
            let PalwTirLeafKindV1::State{state,layer,first_element,..}=current.kind else{return None};
            m.leaves.iter().find(|older| older.position<current.position && older.index<r.legacy_frontier && !read.steps.contains(&older.index)
                && matches!(older.kind,PalwTirLeafKindV1::State{state:s,layer:l,first_element:f,..} if (s,l,f)==(state,layer,first_element)))
                .map(|older|(*i,older.index))
        });
        if let Some((current, stale)) = pair {
            break (r, read, current, stale);
        }
        next(&m, &mut c).unwrap();
    };
    let target = mixed_target(&m, &r);
    assert_ne!(m.leaf_values[current as usize], m.leaf_values[stale as usize]);
    let before = c.progress();
    for attack in 0..3 {
        let mut material = mixed_material(&m, &read);
        if attack == 1 {
            let offset = read.steps.iter().position(|i| *i == current).unwrap();
            material.operands.preimages[offset].values_le[0] ^= 1;
        } else {
            let mut indices = read.steps.iter().copied().filter(|i| *i != current).collect::<Vec<_>>();
            if attack == 2 {
                indices.push(stale);
                indices.sort_unstable();
            }
            // For stale, every carried leaf/proof is authentic but the required
            // current checkpoint is absent. The older position cannot substitute.
            material.operands = operands(&m, &indices);
        }
        assert_eq!(c.submit(&material.view(), target.as_ref().map(Target::view)), Err(FullRelationError::Delivery));
        let after = c.progress();
        assert_eq!(after.next_relation, before.next_relation);
        assert_eq!(after.legacy_frontier, before.legacy_frontier);
        assert_eq!(after.outcome, CheckOutcome::Checking);
        assert!(after.charged_material_bytes > before.charged_material_bytes);
    }
    while c.progress().next_relation < c.preparation().plan().summary().relations {
        next(&m, &mut c).unwrap();
    }
    assert_eq!(c.finish().unwrap(), CheckOutcome::Matched);
}
#[test]
fn signed_wrong_wide_checkpoint_is_panel_witness_rejection_only() {
    let m = mixed(3);
    let s = Signed::new(&m.base, false);
    let result = s.result.id();
    let mut c = mixed_checker(&m, &s, true);
    loop {
        let r = c.preparation().plan().relation_at(c.progress().next_relation).unwrap();
        let checkpoint = matches!(r.kind, ExpandedRelationKind::Legacy(PalwTirLeafV1 { kind: PalwTirLeafKindV1::State { .. }, .. }));
        let progress = c.progress();
        let outcome = next(&m, &mut c);
        if checkpoint {
            assert_eq!(outcome, Err(FullRelationError::PanelMismatch));
            assert_eq!(c.progress().outcome, CheckOutcome::WitnessRejected);
            assert_eq!(c.progress().next_relation, progress.next_relation);
            break;
        }
        outcome.unwrap();
    }
    assert_eq!(s.result.id(), result);
    s.result.authenticate(&s.key, &s.signature).unwrap();
}
