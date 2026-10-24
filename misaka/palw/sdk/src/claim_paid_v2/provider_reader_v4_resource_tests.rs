//! Real Memory provider/checker regressions for acquisition failure provenance.
//! No selected native receipt, K3 artifact or public activation is installed.
use super::*;

fn run(
    fixture: &MemoryProvider,
    limits: ProviderReaderLimitsV4,
    fetch: impl FnMut(u8, u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4>,
) -> Result<ProviderCheckedResultV4, ProviderReaderErrorV4> {
    let mut ledger = route(&fixture.ledger).ledger_checked_v1().unwrap();
    check_provider_material_classified_v4(
        &mut ledger,
        fixture.claim,
        PaidClaimTargetV2::TypedSpec,
        &fixture.work.material,
        &fixture.check,
        limits,
        fetch,
    )
}
fn fetched(fixture: &MemoryProvider, stage: u8, position: u32) -> Result<NativeRetainedV1<Vec<u8>>, ProviderReaderErrorV4> {
    NativeRetainedV1::try_from_clone_v1(
        fixture.units.get(&(stage, position)).ok_or_else(|| ProviderReaderErrorV4::Unavailable("actual unit absent".into()))?,
    )
    .map_err(capacity)
}

#[test]
fn provider_workspace_refusal_is_typed_and_a_fresh_retry_completes_real_memory_steps() {
    let fixture = MemoryProvider::new();
    let original = fixture.ledger.root();
    let mut limits = MemoryProvider::limits();
    limits.max_workspace_bytes = 1;
    assert!(matches!(run(&fixture, limits, |s, p| fetched(&fixture, s, p)), Err(ProviderReaderErrorV4::LocalCapacity(_))));
    assert_eq!(fixture.ledger.root(), original, "read refusal does not mutate accepted claim/receipt state");
    let checked = run(&fixture, MemoryProvider::limits(), |s, p| fetched(&fixture, s, p)).unwrap();
    assert_eq!(checked.finding(), &OutsiderFindingV1::Clean);
    assert!(checked.result.complete.is_some(), "a fresh actual checker retry completes every memory step");
    assert_eq!(fixture.ledger.root(), original);
}

#[test]
fn provider_transport_causes_survive_sticky_optional_readers_without_any_complete_result() {
    let fixture = MemoryProvider::new();
    for expected in 0..4 {
        let mut calls = 0;
        let outcome = run(&fixture, MemoryProvider::limits(), |_, _| {
            calls += 1;
            Err(match expected {
                0 => ProviderReaderErrorV4::LocalCapacity("remote-reader local quota".into()),
                1 => ProviderReaderErrorV4::Unavailable("remote transport unavailable".into()),
                2 => ProviderReaderErrorV4::InvalidMaterial("remote authentication failed".into()),
                _ => ProviderReaderErrorV4::Unknown("opaque local capacity text is not a typed cause".into()),
            })
        });
        let error = match outcome {
            Ok(_) => panic!("failed provider acquisition cannot return Clean or a selected-seat result"),
            Err(error) => error,
        };
        assert!(matches!(
            (expected, error),
            (0, ProviderReaderErrorV4::LocalCapacity(_))
                | (1, ProviderReaderErrorV4::Unavailable(_))
                | (2, ProviderReaderErrorV4::InvalidMaterial(_))
                | (3, ProviderReaderErrorV4::Unknown(_))
        ));
        assert_eq!(calls, 1, "sticky failure stops subsequent provider queries");
    }
}

#[test]
fn malformed_actual_provider_units_are_invalid_and_legacy_string_errors_stay_unknown() {
    // Exercise the actual native scanner's checked arithmetic, rather than
    // inventing a capacity-looking error string or allocating hostile lengths.
    let overflow = kaspa_consensus_core::palw_native_resource_v1::NativeWireShapeV1::default().allocation(u64::MAX, 16).unwrap_err();
    assert_eq!(overflow.kind(), std::io::ErrorKind::OutOfMemory);
    assert!(matches!(shape_error(overflow), ProviderReaderErrorV4::LocalCapacity(_)));
    let fixture = MemoryProvider::new();
    let outcome = run(&fixture, MemoryProvider::limits(), |_, _| NativeRetainedV1::retain_existing_v1(vec![0]).map_err(capacity));
    assert!(matches!(outcome, Err(ProviderReaderErrorV4::InvalidMaterial(_))));
    let mut ledger = route(&fixture.ledger).ledger().unwrap();
    let legacy = check_provider_material_v4(
        &mut ledger,
        fixture.claim,
        PaidClaimTargetV2::TypedSpec,
        &fixture.work.material,
        &fixture.check,
        MemoryProvider::limits(),
        |_, _| Err("local material capacity: arbitrary remote text".into()),
    );
    let error = match legacy {
        Ok(_) => panic!("opaque legacy failure cannot complete verification"),
        Err(error) => error,
    };
    assert_eq!(error.code, "PROVIDER_READER_V4");
    assert!(error.detail.starts_with("material acquisition unclassified:"));
}

#[test]
fn native_quota_refusal_remains_capacity_in_parameter_authentication_and_actual_checker() {
    const CHILD: &str = "MISAKA_PROVIDER_READER_RESOURCE_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name = format!("{}::native_quota_refusal_remains_capacity_in_parameter_authentication_and_actual_checker", module_path!());
        // module_path includes the crate name, while libtest names start at its
        // first module. Strip exactly that known prefix, not a display error.
        let name = name.split_once("::").unwrap().1;
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let fixture = MemoryProvider::new();
    let mut ledger = route(&fixture.ledger).ledger_checked_v1().unwrap();
    let (_, parameter) = fixture.units.iter().find(|((stage, _), _)| *stage == PALW_KERNEL_PARAMETER_STAGE_V4).unwrap();
    use kaspa_consensus_core::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 }));
    assert!(matches!(
        auth_requests(PALW_KERNEL_PARAMETER_STAGE_V4, parameter, &fixture.ledger, &fixture.claim),
        Err(ProviderReaderErrorV4::LocalCapacity(_))
    ));
    assert!(matches!(
        check_provider_material_classified_v4(
            &mut ledger,
            fixture.claim,
            PaidClaimTargetV2::TypedSpec,
            &fixture.work.material,
            &fixture.check,
            MemoryProvider::limits(),
            |_, _| panic!("selected material admission must refuse before provider fetch"),
        ),
        Err(ProviderReaderErrorV4::LocalCapacity(_))
    ));
}
