//! Real direct Memory reader → the same failure selector used by the node.
//! No transport failure is injected in the malformed-unit controls.
use super::*;
use crate::claim_paid_v2::provider_actions_v4::provider_material_challenge_unit_v4;
fn located(
    fixture: &MemoryProvider,
    limits: ProviderReaderLimitsV4,
    fetch: impl FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4>,
) -> Result<ProviderCheckedResultV4, ProviderReaderFailureV4> {
    let mut ledger = route(&fixture.ledger).ledger_checked_v1().unwrap();
    check_provider_material_located_v4(
        &mut ledger,
        fixture.claim,
        PaidClaimTargetV2::TypedSpec,
        &fixture.work.material,
        &fixture.check,
        limits,
        fetch,
    )
}
fn fetched(fixture: &MemoryProvider, s: u8, p: u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4> {
    NativeRetainedV1::try_from_clone_v1(fixture.units.get(&(s, p)).unwrap()).map_err(capacity)
}
#[test]
fn malformed_successful_reads_select_the_exact_entry_position_and_parameter_units() {
    let fixture = MemoryProvider::new();
    let root = fixture.ledger.root();
    let mut observed = std::collections::BTreeSet::new();
    let clean = located(&fixture, MemoryProvider::limits(), |s, p| {
        observed.insert((s, p));
        fetched(&fixture, s, p)
    })
    .unwrap();
    assert!(matches!(clean.finding(), OutsiderFindingV1::Clean));
    assert!(clean.result.complete.is_some());
    let targets = [
        *observed.iter().find(|(s, _)| *s == misaka_palw_kernel::spec::MEMORY_PRE_STATE_STAGE_V1).unwrap(),
        *observed.iter().find(|(s, _)| *s == 0).unwrap(),
        *observed.iter().find(|(s, _)| *s == PALW_KERNEL_PARAMETER_STAGE_V4).unwrap(),
    ];
    for target in targets {
        let mut transport_failures = 0;
        let outcome = located(&fixture, MemoryProvider::limits(), |s, p| {
            if (s, p) == target {
                NativeRetainedV1::retain_existing_v1(vec![0]).map_err(|e| {
                    transport_failures += 1;
                    capacity(e)
                })
            } else {
                fetched(&fixture, s, p)
            }
        });
        let failure = match outcome {
            Err(failure) => failure,
            Ok(_) => panic!("malformed provider material cannot complete a check"),
        };
        assert_eq!(transport_failures, 0, "successful transport is the original missing branch");
        assert!(matches!(failure.cause(), ProviderReaderErrorV4::InvalidMaterial(_)));
        assert_eq!(provider_material_challenge_unit_v4(&failure), Some(target));
        assert_eq!(fixture.ledger.root(), root, "a failure selector grants no native conviction/receipt");
    }
    assert!(located(&fixture, MemoryProvider::limits(), |s, p| fetched(&fixture, s, p)).unwrap().result.complete.is_some());
}
#[test]
fn capacity_opaque_and_transport_failures_do_not_invent_invalid_unit_metadata() {
    let fixture = MemoryProvider::new();
    for mode in 0..4 {
        let mut calls = 0;
        let outcome = located(&fixture, MemoryProvider::limits(), |_, _| {
            calls += 1;
            Err(match mode {
                0 => ProviderReaderErrorV4::LocalCapacity("own quota".into()),
                1 => ProviderReaderErrorV4::Unknown("unclassified cause".into()),
                2 => ProviderReaderErrorV4::Unavailable("transport unavailable".into()),
                _ => ProviderReaderErrorV4::InvalidMaterial("transport authentication".into()),
            })
        });
        let failure = match outcome {
            Err(e) => e,
            Ok(_) => panic!("a failed read cannot complete"),
        };
        assert_eq!(calls, 1, "sticky failure prevents a later query overwriting provenance");
        if mode < 3 {
            assert_eq!(failure.invalid_unit(), None);
            assert_eq!(provider_material_challenge_unit_v4(&failure), None);
        }
        // The final mode has the exact callback unit, and the node also retains
        // its existing transport query. Both identify the same current unit.
    }
    let mut limits = MemoryProvider::limits();
    limits.max_workspace_bytes = 1;
    // The callback returns its separately retained raw owner first. The
    // reader's workspace limit is then checked against that actual frame and
    // codec quote, before authentication/typed decoding. It is not a promise
    // that no network request occurs when only one workspace byte remains.
    let mut calls = 0;
    let root = fixture.ledger.root();
    let failure = match located(&fixture, limits, |s, p| {
        calls += 1;
        fetched(&fixture, s, p)
    }) {
        Err(e) => e,
        Ok(_) => panic!("own budget refusal cannot complete"),
    };
    assert_eq!(calls, 1, "actual frame quota refusal stops subsequent acquisitions");
    assert!(matches!(failure.cause(), ProviderReaderErrorV4::LocalCapacity(_)));
    assert_eq!(failure.invalid_unit(), None);
    assert_eq!(provider_material_challenge_unit_v4(&failure), None);
    assert_eq!(fixture.ledger.root(), root);
}

#[test]
fn full_parameter_reconstruction_failure_never_blames_the_last_authentic_row() {
    use crate::provider_reader_fixture::ledger_world::World;
    use kaspa_consensus_core::palw_kernel_parameter_material_v4::kernel_parameter_at_v4;
    let mut world = World::new();
    let job = world.post_job(2, &[3, 17], 1, 0x72);
    let made = world.honest(&job, 1);
    let claim = made.claim.id();
    world.block(4, vec![made.tx]);
    let class = world.l.claims[&claim].class_binding_id;
    // Isolated reconstruction counterexample: all row paths are authentic
    // under this mixed row/column commitment, yet no full tensor reproduces it.
    // This does not claim the modified class is a newly admitted native class.
    assert_eq!(world.l.classes[&class].descriptor.commitment_suite_id, 2);
    let key = *world
        .params
        .tensors
        .iter()
        .find(|(_, t)| t.shape.len() >= 2 && t.shape[..t.shape.len() - 1].iter().product::<usize>() > 1)
        .unwrap()
        .0;
    let tensor = &world.params.tensors[&key];
    let mut first = misaka_palw_kernel::merkle::TensorOpeningV1::row(tensor, 0).unwrap();
    first.other_root = [91; 64];
    let mixed = first.recomputed_commitment().unwrap();
    world.l.classes.get_mut(&class).unwrap().param_commitments.by_instance.insert(key, mixed);
    let invalid_unit = Cell::new(None);
    let calls = Cell::new(0);
    let result = fetch_parameter_observed_v4(
        &world.l,
        &claim,
        0,
        key.0,
        key.1,
        MemoryProvider::limits(),
        |stage, ordinal| {
            assert_eq!(stage, PALW_KERNEL_PARAMETER_STAGE_V4);
            let (spec, row) = kernel_parameter_at_v4(&world.l, &claim, ordinal).unwrap();
            assert_eq!((spec.parameter, spec.layer), key);
            let mut opening = misaka_palw_kernel::merkle::TensorOpeningV1::row(tensor, row).unwrap();
            opening.other_root = [91; 64];
            assert!(opening.authenticates(&mixed));
            let unit = PalwKernelParameterRowV4 {
                program: 0,
                parameter: key.0,
                layer: key.1,
                row,
                opening: PalwKernelParameterOpeningV4::RowV2(opening),
            };
            calls.set(calls.get() + 1);
            NativeRetainedV1::retain_existing_v1(borsh::to_vec(&unit).unwrap()).map_err(capacity)
        },
        |s, p| invalid_unit.set(Some((s, p))),
    );
    let cause = match result {
        Err(e) => e,
        Ok(_) => panic!("mixed full commitment must refuse"),
    };
    assert!(matches!(&cause, ProviderReaderErrorV4::InvalidMaterial(why)
        if why == "full fetched parameter does not reproduce registered tensor commitment"));
    assert!(calls.get() > 1);
    assert_eq!(invalid_unit.get(), None, "the last authentic row is not the cross-row fault");
    let failure = ProviderReaderFailureV4::from(cause);
    assert_eq!(provider_material_challenge_unit_v4(&failure), None);
}
