//! Actual source coordinate selection shared by Court and dependency geometry.
//! No values, state writer, wire, resource/custody or complete-admission claim.
use super::*;
#[derive(Clone, Copy, Debug)]
pub(crate) enum SupplyGeometryV1 {
    Zero,
    Replay,
    Step { coord: PalwStepCoordinateV1, dtype: DType, lane: usize },
    Node { ctx: DemandContext, node: u16, index: usize },
}
pub(crate) fn reserved(
    space: &PalwTirStepSpaceV1,
    job: PalwTirJobShapeV1,
    pos: u32,
    reserved: u32,
    tile: u64,
) -> PalwStepCoordinateV1 {
    let (call_index, position) = job.call_position(pos);
    PalwStepCoordinateV1 { call_index, node_slot: space.program_slots() + reserved, position, tile_index: tile as u32 }
}
pub(crate) fn commit(
    space: &PalwTirStepSpaceV1,
    job: PalwTirJobShapeV1,
    ctx: DemandContext,
    node: u16,
    tile: u64,
) -> TirResult<PalwStepCoordinateV1> {
    let Some(block) = occurrence_block(space, ctx.occurrence) else { return tir_err(TirErrorKind::Malformed, "no such occurrence") };
    let (Some(slot), Some(_)) = (space.node_slot(usize::from(ctx.occurrence), node), space.commit_tile_len(block, node)) else {
        return tir_err(TirErrorKind::Malformed, format!("node {node} of block {block} is not a commit point"));
    };
    let (call_index, position) = job.call_position(ctx.pos);
    Ok(PalwStepCoordinateV1 { call_index, node_slot: slot, position, tile_index: tile as u32 })
}
pub(crate) fn fixed(
    space: &PalwTirStepSpaceV1,
    job: PalwTirJobShapeV1,
    pos: u32,
    state: u16,
    layer: Option<u16>,
    index: usize,
) -> TirResult<SupplyGeometryV1> {
    if pos == 0 {
        return Ok(SupplyGeometryV1::Zero);
    }
    if !pos.is_multiple_of(space.layout.checkpoint_interval) {
        return Ok(SupplyGeometryV1::Replay);
    }
    let Some(k) = space.fixed_instance_index(state, layer) else {
        return tir_err(TirErrorKind::Malformed, format!("state {state} layer {layer:?} has no checkpoint"));
    };
    let inst = space.fixed_instances()[k];
    let tile = index as u64 / u64::from(inst.tile_lanes);
    Ok(SupplyGeometryV1::Step {
        coord: reserved(space, job, pos - 1, k as u32, tile),
        dtype: inst.dtype,
        lane: index % inst.tile_lanes as usize,
    })
}
pub(crate) fn history(
    space: &PalwTirStepSpaceV1,
    job: PalwTirJobShapeV1,
    pos: u32,
    state: u16,
    layer: Option<u16>,
    row_pos: u32,
    index: usize,
) -> TirResult<SupplyGeometryV1> {
    let h_tile = space.layout.h_tile;
    let tile_start = row_pos - row_pos % h_tile;
    let tile_end = tile_start + (h_tile - 1);
    if tile_end < pos {
        let Some(k) = space.hist_instance_index(state, layer) else {
            return tir_err(TirErrorKind::Malformed, format!("history {state} layer {layer:?} has no tiles"));
        };
        let inst = space.hist_instances()[k];
        let sub = index as u64 / u64::from(inst.tile_lanes);
        let first_lane = sub * u64::from(inst.tile_lanes);
        let row_lanes = (inst.elements - first_lane).min(u64::from(inst.tile_lanes));
        let coord = reserved(space, job, tile_end, (space.fixed_instances().len() + k) as u32, sub);
        let lane = (row_pos - tile_start) as u64 * row_lanes + (index as u64 - first_lane);
        Ok(SupplyGeometryV1::Step { coord, dtype: inst.dtype, lane: lane as usize })
    } else {
        let Some((ctx, node)) = hist_row_node_v1(&space.program, state, layer, row_pos) else {
            return tir_err(TirErrorKind::Malformed, format!("history {state} layer {layer:?} appends nothing"));
        };
        Ok(SupplyGeometryV1::Node { ctx, node, index })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_source_geometry_preserves_checkpoint_history_and_incomplete_row_values() {
        let cases = vec![
            crate::palw_tir_court_v1::test_support::tiny_execution(None),
            crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1().execution,
        ];
        let (mut checkpoints, mut histories, mut incomplete) = (0, 0, 0);
        for x in cases {
            let v = crate::palw_tir_step_v1::verify_tir_binding_v1(&x.binding, 1 << 40).unwrap();
            let inventory = PalwTirInventoryIndexV1::new(&v.space.program).unwrap();
            let rules = PalwTirCourtRulesV1 {
                max_step_leaf_count: 1 << 40,
                prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
                limits: misaka_palw_tir::demand::DemandLimits::UNLIMITED,
            };
            let mut source = store_source(&v, &x.binding.job_context, &inventory, &x, x.binding.step_leaf_count, &rules);
            for pos in 0..v.job.positions {
                for inst in v.space.fixed_instances() {
                    for index in [0, inst.elements as usize - 1] {
                        let expected = fixed(&v.space, v.job, pos, inst.state, inst.layer, index).unwrap();
                        let actual = DemandSource::state(&mut source, pos, inst.state, inst.layer, index).unwrap();
                        match expected {
                            SupplyGeometryV1::Zero => assert!(matches!(actual, StateSupply::Value(0))),
                            SupplyGeometryV1::Replay => assert!(matches!(actual, StateSupply::Replay)),
                            SupplyGeometryV1::Step { coord, dtype, lane: element } => {
                                checkpoints += 1;
                                let leaf = v.space.leaf_index(&x.binding.job_context, &coord).unwrap();
                                assert!(matches!(
                                    v.space.leaf_at(&x.binding.job_context, leaf).unwrap().kind,
                                    PalwTirLeafKindV1::State { .. }
                                ));
                                let preimage = x.step_leaf(leaf).unwrap();
                                let value = super::super::lane(dtype, &preimage.values_le, element).unwrap();
                                assert!(matches!(actual,StateSupply::Value(v) if v==value));
                            }
                            _ => panic!("unexpected fixed node geometry"),
                        }
                    }
                }
                for inst in v.space.hist_instances() {
                    let occ = misaka_palw_tir::demand::state_occurrence_v1(&v.space.program, inst.state, inst.layer).unwrap();
                    let block = occurrence_block(&v.space, occ).unwrap();
                    let h = misaka_palw_tir::demand::history_length_v1(&v.space.info, block, pos).unwrap();
                    for row in (pos + 1 - h as u32)..pos {
                        for index in [0, inst.elements as usize - 1] {
                            let expected = history(&v.space, v.job, pos, inst.state, inst.layer, row, index).unwrap();
                            let actual = DemandSource::hist_row(&mut source, pos, inst.state, inst.layer, row, index).unwrap();
                            match expected {
                                SupplyGeometryV1::Step { coord, dtype, lane: element } => {
                                    histories += 1;
                                    let leaf = v.space.leaf_index(&x.binding.job_context, &coord).unwrap();
                                    assert!(matches!(
                                        v.space.leaf_at(&x.binding.job_context, leaf).unwrap().kind,
                                        PalwTirLeafKindV1::HistTile { .. }
                                    ));
                                    let preimage = x.step_leaf(leaf).unwrap();
                                    assert_eq!(actual, super::super::lane(dtype, &preimage.values_le, element).unwrap());
                                }
                                SupplyGeometryV1::Node { ctx, node, index } => {
                                    incomplete += 1;
                                    assert_eq!(actual, DemandSource::node(&mut source, ctx, node, index).unwrap());
                                }
                                _ => panic!("unexpected history geometry"),
                            }
                        }
                    }
                }
            }
        }
        assert!(checkpoints > 0 && histories > 0 && incomplete > 0);
    }
}
