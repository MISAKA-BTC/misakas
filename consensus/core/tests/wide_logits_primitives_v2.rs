//! Pure RFC-0043 data/selector primitives. No native admission, model preload,
//! Court integration, funding, actual Gate C draw or activation is exercised.
use kaspa_consensus_core::{
    Hash64,
    palw_decode_select_v2::{PalwDecodeSamplingV2, decode_lane_key_v2},
    palw_logits_q24_v2 as wide,
    palw_step_leg::{step_merkle_root_v1, step_opening_from_leaf_source_capped_v1},
    palw_step_refute::{PALW_LOGITS_TILE_LANES, tiled_logits_scheme_id_v1},
    palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_job_context_v1},
    palw_v2::PalwJobContextV2,
};

fn context() -> PalwJobContextV2 {
    let facts = PalwTirJobFactsV1 {
        class_id: Hash64::from_u64_word(7),
        max_context: 8192,
        token_bound: 4099,
        tiled: true,
        held: false,
    };
    let mut context = palw_tir_job_context_v1(&facts, (4, 2));
    // This is fixture data, not independently resolved native authority.
    context.trace_scheme_id = wide::scheme_id();
    context
}
fn rows() -> Vec<Vec<i64>> {
    vec![vec![1082i64 << 24; 4099], vec![-(1i64 << 34); 4099]]
}
fn event(
    ctx: &PalwJobContextV2,
    rows: &[Vec<i64>],
    ids: &[u32],
    row: u32,
    tile: u32,
) -> wide::WideLogitsEventV2 {
    let row_index = usize::try_from(row).unwrap();
    let context = ctx.context_hash();
    let roots: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(r, lanes)| wide::row_root(&context, u32::try_from(r).unwrap(), lanes).unwrap())
        .collect();
    let row_root = roots[row_index];
    let row_opening = step_opening_from_leaf_source_capped_v1(
        u64::try_from(roots.len()).unwrap(),
        u64::from(row),
        16,
        |index| roots[usize::try_from(index).unwrap()],
    )
    .unwrap();
    let chunks: Vec<_> = rows[row_index].chunks(PALW_LOGITS_TILE_LANES).collect();
    let leaves: Vec<_> = chunks
        .iter()
        .enumerate()
        .map(|(t, lanes)| wide::tile_leaf(&context, row, u32::try_from(t).unwrap(), lanes))
        .collect();
    let tile_opening = step_opening_from_leaf_source_capped_v1(
        u64::try_from(leaves.len()).unwrap(),
        u64::from(tile),
        16,
        |index| leaves[usize::try_from(index).unwrap()],
    )
    .unwrap();
    wide::WideLogitsEventV2 {
        row,
        generated: ids.to_vec(),
        row_root,
        row_opening,
        lanes: chunks[usize::try_from(tile).unwrap()].to_vec(),
        tile_opening,
    }
}
fn keyed(domain: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut hash = blake2b_simd::Params::new()
        .hash_length(64)
        .key(domain)
        .to_state();
    for part in parts {
        hash.update(&u64::try_from(part.len()).unwrap().to_le_bytes());
        hash.update(part);
    }
    Hash64::from_bytes(hash.finalize().as_bytes().try_into().unwrap())
}

#[test]
fn normative_domains_and_length_prefixed_bytes_stay_exact() {
    let name = b"tiled-logits-i64-q24-v2";
    assert_eq!(
        wide::scheme_id(),
        keyed(b"misaka-palw/tiled-logits/scheme/v2", &[name])
    );
    assert_ne!(wide::scheme_id(), tiled_logits_scheme_id_v1());
    let ctx = context();
    let context = ctx.context_hash();
    let lanes = [i64::MIN, 1082i64 << 24, i64::MAX];
    let bytes: Vec<_> = lanes.iter().flat_map(|lane| lane.to_le_bytes()).collect();
    let parts: &[&[u8]] = &[
        context.as_byte_slice(),
        &1u64.to_le_bytes(),
        &3u64.to_le_bytes(),
        &bytes,
    ];
    let tile = wide::tile_leaf(&context, 1, 3, &lanes);
    assert_eq!(tile, keyed(b"misaka-palw/tiled-logits/tile/v2", parts));
    assert_ne!(tile, keyed(b"misaka-palw/tiled-logits/tile/v1", parts));
    let ids = [0u32, u32::MAX];
    let ids_bytes: Vec<_> = ids.iter().flat_map(|id| id.to_le_bytes()).collect();
    assert_eq!(
        wide::outer_root(&ctx, 2, &tile, &ids),
        keyed(
            b"misaka-palw/tiled-logits/root/v2",
            &[
                context.as_byte_slice(),
                &4u64.to_le_bytes(),
                &2u64.to_le_bytes(),
                &2u64.to_le_bytes(),
                tile.as_byte_slice(),
                &ids_bytes,
            ]
        )
    );
}

#[test]
fn every_tested_i32_lane_key_and_seeded_selection_matches_legacy() {
    let mut all = vec![
        vec![i32::MIN, 0, i32::MAX],
        vec![1, 1, -1],
        vec![-7, -4, -1],
    ];
    let mut random = 0x5a1cu32;
    for _ in 0..16 {
        all.push(
            (0..97)
                .map(|_| {
                    random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                    i32::from_le_bytes(random.to_le_bytes())
                })
                .collect(),
        );
    }
    for seed in [[0; 32], [7; 32], [255; 32]] {
        for temperature_q in [0, 1, 1 << 24, u32::MAX] {
            let sampling = PalwDecodeSamplingV2 {
                seed,
                temperature_q,
            };
            for position in [0, 1, 13, 1023] {
                for row in &all {
                    let values: Vec<_> = row.iter().map(|value| i64::from(*value)).collect();
                    for (lane, value) in row.iter().enumerate() {
                        assert_eq!(
                            wide::lane_key(i64::from(*value), &sampling, position, lane),
                            i128::from(decode_lane_key_v2(
                                *value,
                                &seed,
                                position,
                                lane,
                                temperature_q
                            ))
                        );
                    }
                    assert_eq!(
                        wide::select(&values, &sampling, position),
                        Some(sampling.select(row, position))
                    );
                }
            }
        }
    }
    let sampling = PalwDecodeSamplingV2 {
        seed: [1; 32],
        temperature_q: u32::MAX,
    };
    assert_eq!(
        wide::select(&[i64::MIN, i64::MAX, 0], &sampling, 1),
        Some(1)
    );
    assert_eq!(
        wide::lane_key(1082i64 << 24, &PalwDecodeSamplingV2::GREEDY, 0, 0),
        1082i128 << 48
    );
    assert_eq!(
        wide::select(&[5, 5, 4], &PalwDecodeSamplingV2::GREEDY, 0),
        Some(0)
    );
    assert_eq!(wide::select(&[], &sampling, 0), None);
}

#[test]
fn root_builders_reject_empty_ragged_count_and_old_scheme_shapes() {
    let ctx = context();
    let data = rows();
    assert_eq!(wide::row_root(&ctx.context_hash(), 0, &[]), None);
    assert_eq!(wide::rows_root(&ctx, &[]), None);
    assert_eq!(wide::rows_root(&ctx, &data[..1]), None);
    assert_eq!(wide::trace_root(&ctx, &data, &[1]), None);
    let mut bad = data.clone();
    bad[1].pop();
    assert_eq!(wide::rows_root(&ctx, &bad), None);
    let mut bad = ctx.clone();
    bad.trace_scheme_id = tiled_logits_scheme_id_v1();
    assert_eq!(wide::rows_root(&bad, &data), None);
    let mut bad = ctx;
    bad.exact_decode_tokens = 0;
    assert_eq!(wide::rows_root(&bad, &[]), None);
    assert_eq!(wide::trace_root(&bad, &[], &[]), None);
}

#[test]
fn authenticate_exact_rows_tiles_and_caps_without_narrowing() {
    let ctx = context();
    let rows = rows();
    let ids = [4098, 1];
    let trace = wide::trace_root(&ctx, &rows, &ids).unwrap();
    for row in 0..2 {
        for tile in 0..2 {
            let event = event(&ctx, &rows, &ids, row, tile);
            assert_eq!(event.authenticate(&ctx, &trace, 4099, 2), Ok(()));
            assert!(event.authenticate(&ctx, &trace, 4099, 1).is_err());
            assert!(event.authenticate(&ctx, &trace, 0, 2).is_err());
            assert!(event.authenticate(&ctx, &trace, 8193, 2).is_err());
            let mut old = ctx.clone();
            old.trace_scheme_id = tiled_logits_scheme_id_v1();
            assert!(event.authenticate(&old, &trace, 4099, 2).is_err());
            for attack in 0..10 {
                let mut bad = event.clone();
                match attack {
                    0 => bad.row = 1 - row,
                    1 => bad.row_opening.leaf_index = 1 - u64::from(row),
                    2 => bad.generated[0] ^= 1,
                    3 => bad.lanes[0] ^= 1,
                    4 => bad.lanes.pop().map(|_| ()).unwrap(),
                    5 => bad.tile_opening.leaf_index = u64::MAX,
                    6 => bad.row_opening.siblings.clear(),
                    7 => bad.tile_opening.siblings.push(Hash64::from_u64_word(77)),
                    8 => bad.tile_opening.leaf_index ^= 1,
                    _ => bad.generated.pop().map(|_| ()).unwrap(),
                }
                assert!(
                    bad.authenticate(&ctx, &trace, 4099, 2).is_err(),
                    "row {row} tile {tile} attack {attack}"
                );
            }
        }
    }
    let mut bad = ctx.clone();
    bad.exact_decode_tokens = 3;
    assert!(
        event(&ctx, &rows, &ids, 0, 0)
            .authenticate(&bad, &trace, 4099, 2)
            .is_err()
    );
}

#[test]
fn coherently_committed_bad_token_is_data_not_computation_authority() {
    let ctx = context();
    let rows = rows();
    let ids = [4099, 1]; // First token is outside the independently supplied vocabulary.
    let trace = wide::trace_root(&ctx, &rows, &ids).unwrap();
    let disclosure = event(&ctx, &rows, &ids, 0, 0);
    assert_eq!(disclosure.authenticate(&ctx, &trace, 4099, 2), Ok(()));
    // This explicit ordinary semantic predicate fails. No full Court/native
    // integration or computation-pass capability is claimed by this test.
    assert!(usize::try_from(disclosure.generated[0]).unwrap() >= 4099);
    assert_ne!(
        wide::select(&rows[0], &PalwDecodeSamplingV2::GREEDY, 0),
        Some(4099)
    );
    let roots: Vec<_> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            wide::row_root(&ctx.context_hash(), u32::try_from(i).unwrap(), row).unwrap()
        })
        .collect();
    let root = step_merkle_root_v1(&roots).unwrap();
    assert_eq!(wide::outer_root(&ctx, 2, &root, &ids), trace);
    assert_ne!(wide::outer_root(&ctx, 3, &root, &ids), trace);
}
