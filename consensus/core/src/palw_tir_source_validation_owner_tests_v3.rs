// Included in the existing real signed Source-three fixture module.
thread_local! {static CONTINUATION_VALIDATION_FREE_V3:std::cell::Cell<u64>=const{std::cell::Cell::new(0)};}
fn continuation_validation_requests_v3(
    case: &ContinuationSourceCaseV2,
    frame: &crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1,
) -> u64 {
    use crate::palw_tir_public_state_v1::source_validation_v3 as v;
    let key = match &case.control.target {
        T::Preparation { .. } => frame.preparation_key_v1(),
        T::Acquisition { owner, reserved_daa } => owner_key(frame, *owner, *reserved_daa).unwrap(),
        _ => panic!("real Preparation/Owner fixture"),
    };
    match case.state.staged_court_row_v1(&key).unwrap() {
        Row::TirPreparation(row) => v::preparation_requests_v3(row).unwrap(),
        Row::TirAcquisition(row) => v::acquisition_requests_v3(row).unwrap(),
        _ => panic!("actual accepted source row"),
    }
}
#[test]
fn source_validation_streamed_identity_matches_canonical_borsh_and_raw_errors() {
    use crate::palw_tir_public_state_v1::source_validation_v3::hash_v3;
    use borsh::BorshSerialize;
    fn check<T: BorshSerialize>(domain: &[u8], value: &T) {
        let bytes = borsh::to_vec(value).unwrap();
        let raw =
            Hash64::from_bytes(blake2b_simd::Params::new().hash_length(64).key(domain).hash(&bytes).as_bytes().try_into().unwrap());
        assert_eq!(hash_v3(domain, value).unwrap(), raw);
    }
    let f = source_three_fixture();
    let frame = &f.fx.frame;
    let cases = continuation_source_cases_v2(&f);
    check(b"misaka-palw/tir-source-header/v1", frame.header_v1());
    for case in &cases {
        let key = match &case.control.target {
            T::Preparation { .. } => frame.preparation_key_v1(),
            T::Acquisition { owner, reserved_daa } => owner_key(frame, *owner, *reserved_daa).unwrap(),
            _ => unreachable!(),
        };
        match case.state.staged_court_row_v1(&key).unwrap() {
            Row::TirPreparation(row) => check(b"source-validation-test", row),
            Row::TirAcquisition(row) => check(b"source-validation-test", row),
            _ => unreachable!(),
        }
    }
    check(b"bulk-borsh-test", &vec![11u8; 70_000]);
    let custody = frame.custody_v1();
    let old = custody.liable_signers_v1();
    for actor in std::iter::once(custody.pledge_v1().producer)
        .chain(custody.drawn_panel_v1().iter().copied())
        .chain(std::iter::once(outsider()))
    {
        assert_eq!(
            old.contains(&actor),
            actor == custody.pledge_v1().producer || frame.original_custody_v1().grant_v1(&actor).is_some()
        );
    }
    struct Refuses;
    impl BorshSerialize for Refuses {
        fn serialize<W: std::io::Write>(&self, _: &mut W) -> std::io::Result<()> {
            Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "canonical refusal"))
        }
    }
    assert_eq!(hash_v3(b"error", &Refuses).unwrap_err(), borsh::to_vec(&Refuses).unwrap_err().to_string());
}
#[test]
fn source_validation_actual_preparation_and_pending_owner_admit_before_hash_and_hold_to_last_borrow() {
    const CHILD: &str = "MISAKA_SOURCE_VALIDATION_CHILD_V3";
    if std::env::var_os(CHILD).is_none() {
        let full = format!(
            "{}::source_validation_actual_preparation_and_pending_owner_admit_before_hash_and_hold_to_last_borrow",
            module_path!()
        );
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", full.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    use crate::palw_native_resource_v1::*;
    use crate::palw_tir_public_state_v1::*;
    let f = source_three_fixture();
    let frame = &f.fx.frame;
    let mut cases = continuation_source_cases_v2(&f);
    let network = f.extras.tir_public_network_domain_v1.unwrap();
    // The Owner fixture must have a real pending native demand, not an invented
    // demand DTO; Preparation already has its actual signed capture receipt.
    let case = &cases[2];
    let mut b = TransitionBuilder::try_new(&case.state, &f.params, false, false, false, false, &f.extras).unwrap();
    apply_control_v1(&mut b, &at(frame, case.offset), network, &case.control).unwrap();
    cases[2].state = b.state.clone();
    let source_q = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::native_construction_requests_v1(
        &f.fx.record,
        &f.fx.layout,
        frame.header_v1(),
    )
    .unwrap();
    const LIMIT: u64 = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    thread_local! {static SEEN:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
    fn last() {
        assert!(
            reserve_native_requests_v1(1).is_err(),
            "both admitted validation scratch and Source remain live at the actual final validation borrow"
        );
        SEEN.with(|v| v.set(true));
    }
    for case in [&cases[1], &cases[2]] {
        let validation = continuation_validation_requests_v3(case, frame);
        assert!(validation > 0);
        let key = match &case.control.target {
            T::Preparation { .. } => frame.preparation_key_v1(),
            T::Acquisition { owner, reserved_daa } => owner_key(frame, *owner, *reserved_daa).unwrap(),
            _ => unreachable!(),
        };
        let get = || match case.state.staged_court_row_v1(&key).unwrap() {
            Row::TirPreparation(row) => {
                palw_tir_public_source_for_preparation_row_native_v2(&case.state, row, frame.descriptor_v1().maximum_leaves)
            }
            Row::TirAcquisition(row) => {
                palw_tir_public_source_for_row_native_v2(&case.state, row, frame.descriptor_v1().maximum_leaves)
            }
            _ => unreachable!(),
        };
        let before = case.state.state_root();
        let blocker = reserve_native_requests_v1(LIMIT - (validation - 1)).unwrap();
        reset_continuation_source_stages_v2();
        assert!(matches!(get(), Err(PalwTirPublicSourceErrorV2::Resource(_))));
        assert_eq!(continuation_source_stages_v2(), [0, 0, 0], "actual validation target refuses before Source construction");
        drop(blocker);
        let blocker = reserve_native_requests_v1(LIMIT - validation).unwrap();
        reset_continuation_source_stages_v2();
        assert!(matches!(get(), Err(PalwTirPublicSourceErrorV2::Resource(_))));
        assert_eq!(continuation_source_stages_v2(), [1, 0, 0], "Q admits actual validation, but no room for the separate Source");
        drop(blocker);
        let peak = source_q + validation;
        let blocker = reserve_native_requests_v1(LIMIT - peak).unwrap();
        source_validation_v3::observe_last_v3(Some(last));
        SEEN.with(|v| v.set(false));
        let source = get().unwrap();
        source_validation_v3::observe_last_v3(None);
        assert!(SEEN.with(std::cell::Cell::get));
        assert_eq!(source.source_id_v1(), frame.plan_v1().source_id);
        assert!(
            reserve_native_requests_v1(validation).is_ok(),
            "validation workspace retires before returning the independently retained Source"
        );
        assert!(reserve_native_requests_v1(validation + 1).is_err());
        drop(source);
        assert!(reserve_native_requests_v1(peak).is_ok());
        drop(blocker);
        assert_eq!(case.state.state_root(), before);
        let raw = match case.state.staged_court_row_v1(&key).unwrap() {
            Row::TirPreparation(row) => {
                palw_tir_public_source_for_preparation_row_v1(&case.state, row, frame.descriptor_v1().maximum_leaves).unwrap()
            }
            Row::TirAcquisition(row) => {
                palw_tir_public_source_for_row_v1(&case.state, row, frame.descriptor_v1().maximum_leaves).unwrap()
            }
            _ => unreachable!(),
        };
        assert_eq!(raw.source_id_v1(), get().unwrap().source_id_v1());
        match case.state.staged_court_row_v1(&key).unwrap() {
            Row::TirPreparation(row) => {
                let mut wrong = row.as_ref().clone();
                wrong.descriptor.source_id = Hash64::default();
                assert!(matches!(
                    palw_tir_public_source_for_preparation_row_native_v2(&case.state, &wrong, frame.descriptor_v1().maximum_leaves),
                    Err(PalwTirPublicSourceErrorV2::Decode(_))
                ));
            }
            Row::TirAcquisition(row) => {
                let mut wrong = row.as_ref().clone();
                wrong.descriptor.source_id = Hash64::default();
                assert!(matches!(
                    palw_tir_public_source_for_row_native_v2(&case.state, &wrong, frame.descriptor_v1().maximum_leaves),
                    Err(PalwTirPublicSourceErrorV2::Decode(_))
                ));
            }
            _ => unreachable!(),
        }
        assert!(reserve_native_requests_v1(LIMIT).is_ok(), "semantic refusal drops validation and Source owners");
    }
    assert!(reserve_native_requests_v1(LIMIT).is_ok());
}

#[test]
fn source_identity_writer_borsh_layout_crosses_power_of_two_without_underquote() {
    use crate::palw_native_resource_v1::NativeDecodeErrorV1;
    use crate::palw_tir_public_source_v1::PalwTirPublicSourceV1 as Source;
    // A real layout's actual Borsh field sequence; this standalone writer
    // geometry control does not register this expanded layout as a new class.
    let f = source_three_fixture();
    let mut layout = f.fx.layout.clone();
    layout.commit_tiles.clear();
    layout.state_tiles.clear();
    while borsh::object_length(&layout).unwrap() <= 1024 {
        layout.commit_tiles.push(4);
    }
    struct Growth {
        bytes: Vec<u8>,
        peak_envelope: usize,
    }
    impl std::io::Write for Growth {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let old = self.bytes.capacity();
            self.bytes.extend_from_slice(bytes);
            let now = self.bytes.capacity();
            self.peak_envelope = self.peak_envelope.max(if old != now { old + now } else { now });
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let span = borsh::object_length(&layout).unwrap() as u64;
    let mut writer = Growth { bytes: Vec::with_capacity(1024), peak_envelope: 1024 };
    borsh::BorshSerialize::serialize(&layout, &mut writer).unwrap();
    assert_eq!(writer.bytes, borsh::to_vec(&layout).unwrap());
    assert!(span > 1024 && span < 1100);
    assert!(span * 2 < writer.peak_envelope as u64, "old logical-span quote misses actual Borsh growth transition");
    let corrected = Source::native_identity_writer_requests_v3(span).unwrap();
    assert!(corrected >= writer.peak_envelope as u64);
    assert_eq!(Source::native_identity_writer_requests_v3(1024).unwrap(), 2048);
    assert_eq!(Source::native_identity_writer_requests_v3(1025).unwrap(), 4096);
    assert_eq!(Source::native_identity_writer_requests_v3(2048).unwrap(), 4096);
    assert_eq!(Source::native_identity_writer_requests_v3(2049).unwrap(), 8192);
    assert!(matches!(Source::native_identity_writer_requests_v3(u64::MAX), Err(NativeDecodeErrorV1::Resource(_))));
}
