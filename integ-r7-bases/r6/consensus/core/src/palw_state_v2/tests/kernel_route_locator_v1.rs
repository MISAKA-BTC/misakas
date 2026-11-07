//! `PalwChainStateV2::kernel_route_offset_of_preimage_v1` (the op-242 client's structural locator) agrees with the writer's own
//! offset (`state_root_preimage_located_v1`) and is not moved by a label occurring anywhere the grammar does not begin a block.
use super::*;

fn with_kernel(mut state: PalwChainStateV2) -> PalwChainStateV2 {
    state.kernel_route = Some(crate::palw_kernel_route_v1::PalwKernelRouteStateV1::new(
        crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(h64(1), h64(2)),
        None,
        misaka_palw_kernel::rows::LedgerScalarsV1 { daa: 1, burned: 0 },
    ));
    state
}

fn located(state: &PalwChainStateV2) -> (Vec<u8>, Option<usize>) {
    let (preimage, offset) = state.state_root_preimage_located_v1();
    assert_eq!(preimage, state.state_root_preimage(), "the located writer is the preimage writer, byte for byte");
    (preimage, offset)
}

#[test]
fn kernel_route_locator_matches_the_writer_on_real_states() {
    let floor = crate::palw_real_share_v1::PalwFloorStateV1 {
        mode: crate::palw_real_share_v1::PalwFloorModeV1::Probe { until: 77 },
        last_probe_end: Some(5),
    };
    let mut floored = with_kernel(m02_populated_state());
    floored.floor_state = Some(floor);
    let mut floored_without_kernel = m02_populated_state();
    floored_without_kernel.floor_state = Some(floor);
    for (name, state, expect_some) in [
        ("genesis", PalwChainStateV2::genesis(), false),
        ("genesis + kernel", with_kernel(PalwChainStateV2::genesis()), true),
        ("populated", m02_populated_state(), false),
        ("populated + kernel", with_kernel(m02_populated_state()), true),
        ("populated + floor + kernel", floored, true),
        ("populated + floor", floored_without_kernel, false),
    ] {
        let (preimage, written) = located(&state);
        assert_eq!(written.is_some(), expect_some, "{name}");
        assert_eq!(PalwChainStateV2::kernel_route_offset_of_preimage_v1(&preimage), Ok(written), "{name}");
        if let (Some(at), Some(route)) = (written, state.kernel_route.as_ref()) {
            assert_eq!(&preimage[at..at + 15], b"kernel-route/v1");
            assert_eq!(&preimage[at + 15 + 64..at + 15 + 128], route.aux_root().as_byte_slice(), "{name}");
        }
    }
}

/// A label planted after the real block (two windows: the old search refused it as ambiguous) or with no real block at all (one
/// window: the old search would have accepted it) never moves the structural locator.
#[test]
fn kernel_route_locator_ignores_labels_outside_the_grammar() {
    let (mut preimage, written) = located(&with_kernel(m02_populated_state()));
    preimage.extend_from_slice(b"kernel-route/v1");
    preimage.extend_from_slice(&[0xAB; 128]);
    assert_eq!(PalwChainStateV2::kernel_route_offset_of_preimage_v1(&preimage), Ok(written));
    let (mut absent, none) = located(&PalwChainStateV2::genesis());
    assert_eq!(none, None);
    // Not on a 64-byte step of the unlabeled run: the walk never inspects it.
    absent.push(0);
    absent.extend_from_slice(b"kernel-route/v1");
    absent.extend_from_slice(&[0xCD; 128]);
    assert_eq!(PalwChainStateV2::kernel_route_offset_of_preimage_v1(&absent), Ok(None));
}

/// Every optional block the grammar allows before the kernel route, in every combination, built synthetically (the unlabeled roots'
/// bytes never begin with a label).
#[test]
fn kernel_route_locator_walks_every_optional_block_before_the_kernel_route() {
    let root = |n: usize| [(n as u8) | 0x80; 64];
    let floor = borsh::to_vec(&crate::palw_real_share_v1::PalwFloorStateV1 {
        mode: crate::palw_real_share_v1::PalwFloorModeV1::Idle,
        last_probe_end: Some(9),
    })
    .unwrap();
    let budgets =
        borsh::to_vec(&PalwEpochBudgetsV2 { epoch_index: 3, budget_blocks: [(h64(4), 5u64)].into_iter().collect() }).unwrap();
    for with_budgets in [false, true] {
        for leading in 0..=17usize {
            for (tir, floor_on) in [(false, false), (true, false), (false, true), (true, true)] {
                for trailing in 0..=2usize {
                    let mut p = PALW_STATE_V2_VERSION.to_le_bytes().to_vec();
                    (0..7).for_each(|i| p.extend_from_slice(&root(i)));
                    if with_budgets {
                        p.push(1);
                        p.extend_from_slice(&budgets);
                    } else {
                        p.push(0);
                    }
                    (0..14 + leading).for_each(|i| p.extend_from_slice(&root(i)));
                    if tir {
                        p.extend_from_slice(b"tir_shard/v1");
                        p.extend_from_slice(&root(1));
                        p.extend_from_slice(&root(2));
                    }
                    if floor_on {
                        p.extend_from_slice(b"floor_state_v1");
                        p.extend_from_slice(&floor);
                    }
                    (0..trailing).for_each(|i| p.extend_from_slice(&root(i)));
                    let at = p.len();
                    p.extend_from_slice(b"kernel-route/v1");
                    p.extend_from_slice(&[7; 128]);
                    p.extend_from_slice(b"staged_public_courts/v1");
                    assert_eq!(
                        PalwChainStateV2::kernel_route_offset_of_preimage_v1(&p),
                        Ok(Some(at)),
                        "budgets {with_budgets} leading {leading} tir {tir} floor {floor_on} trailing {trailing}"
                    );
                }
            }
        }
    }
    // A run longer than the grammar allows is not walked into.
    let mut p = PALW_STATE_V2_VERSION.to_le_bytes().to_vec();
    (0..7).for_each(|i| p.extend_from_slice(&root(i)));
    p.push(0);
    (0..14 + 20).for_each(|i| p.extend_from_slice(&root(i)));
    p.extend_from_slice(b"kernel-route/v1");
    p.extend_from_slice(&[7; 128]);
    assert_eq!(PalwChainStateV2::kernel_route_offset_of_preimage_v1(&p), Ok(None));
    // Another layout version is refused, never searched.
    let mut other = p.clone();
    other[0] ^= 1;
    assert!(PalwChainStateV2::kernel_route_offset_of_preimage_v1(&other).is_err());
}
