//! Source regressions; added without executing Cargo/rustc. No allocator/RSS or
//! total NF CPU measurement is claimed. Parent global switches are not set;
//! the exact ignored helper changes only its isolated child process's switches.
use super::*;

#[test]
fn checked_work_precharge_is_atomic_and_never_refunds() {
    assert!(matches!(LocalScanWorkBudgetV1::new(0), Err(Error::ZeroLimit)));
    let (steps, bytes) = scan_upper(17).unwrap();
    assert_eq!(steps, (8 * (17 + 4) + 4096) + 3 * (8 * 17 + 4096) + 4);
    assert_eq!(bytes, 4 * 17 + 4);
    assert_eq!(scan_upper(u64::MAX), Err(Error::ArithmeticOverflow));
    let mut budget = LocalScanWorkBudgetV1::new(steps).unwrap();
    budget.charge(steps, bytes).unwrap();
    assert_eq!((budget.charged_steps(), budget.charged_reader_bytes(), budget.attempts()), (steps, bytes, 1));
    assert_eq!(budget.charge(1, 1), Err(Error::ScanSteps));
    assert_eq!((budget.charged_steps(), budget.charged_reader_bytes(), budget.attempts()), (steps, bytes, 1));
    // Companion-counter overflow must not partially update the step balance.
    budget.charged_reader_bytes = u64::MAX;
    assert_eq!(budget.charge(0, 1), Err(Error::ArithmeticOverflow));
    assert_eq!((budget.charged_steps(), budget.charged_reader_bytes(), budget.attempts()), (steps, u64::MAX, 1));
}

#[test]
fn unknown_profile_refuses_before_any_scanner_charge() {
    if supported_build() {
        return;
    }
    let mut work = LocalScanWorkBudgetV1::new(u64::MAX).unwrap();
    let result = scan_source_program_local_v1(
        &[],
        SourceProgramLocalLimitsV1 {
            max_program_bytes: u64::MAX,
            max_scan_steps: u64::MAX,
            max_nf_items: u64::MAX,
            max_native_requests: u64::MAX,
        },
        &mut work,
    );
    assert_eq!(result.err(), Some(Error::UnsupportedCoverage));
    assert_eq!((work.charged_steps(), work.observed_steps(), work.attempts()), (0, 0, 0));
    assert_eq!(bootstrap::scan_bootstrap_requests_v1(), Err(Error::UnsupportedCoverage));
    eprintln!("SOURCE_LOCAL_UNSUPPORTED_PROFILE: fixed refusal; no successful decode claimed");
}

/// Process-isolated flag changes: never mutate the parent test process's
/// OnceLocks, and never compare snapshots that other parallel tests can change.
#[test]
#[cfg(not(target_arch = "wasm32"))]
fn local_owner_is_independent_of_zero_global_limits_in_child() {
    use std::{
        process::{Child, Command, Stdio},
        thread,
        time::{Duration, Instant},
    };
    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            if !matches!(self.0.try_wait(), Ok(Some(_))) {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }
    let mut child = ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "palw_native_resource_v1::source_local::tests::global_flags_child_helper", "--ignored", "--nocapture"])
            .env("MISAKA_SOURCE_LOCAL_FLAGS_HELPER", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now().checked_add(Duration::from_secs(10)).unwrap();
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "isolated Source local flag helper failed");
            break;
        }
        assert!(Instant::now() < deadline, "isolated Source local flag helper exceeded polling deadline");
        thread::sleep(Duration::from_millis(5));
    }
    // The10s polling deadline does not promise bounded OS kill/wait scheduling,
    // process-group cleanup, physical RSS or model execution CPU coverage.
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
#[ignore = "exact helper for the process-isolated global flag regression"]
fn global_flags_child_helper() {
    use crate::palw_native_resource_v1::{
        NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, arm_native_resource_limits_v1,
        native_logical_admission_enabled_v1, native_resource_admission_enabled_v1,
    };
    assert_eq!(std::env::var_os("MISAKA_SOURCE_LOCAL_FLAGS_HELPER").as_deref(), Some(std::ffi::OsStr::new("1")));
    let zero = NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 };
    arm_native_resource_limits_v1(Some(zero));
    arm_native_logical_limits_v1(Some(zero));
    assert!(native_resource_admission_enabled_v1());
    assert!(native_logical_admission_enabled_v1());
    if !supported_build() {
        let mut work = LocalScanWorkBudgetV1::new(u64::MAX).unwrap();
        let cap = SourceProgramLocalLimitsV1 {
            max_program_bytes: u64::MAX,
            max_scan_steps: u64::MAX,
            max_nf_items: u64::MAX,
            max_native_requests: u64::MAX,
        };
        assert_eq!(scan_source_program_local_v1(&[], cap, &mut work).err(), Some(Error::UnsupportedCoverage));
        assert_eq!(work.attempts(), 0);
        eprintln!("SOURCE_LOCAL_UNSUPPORTED_PROFILE: child flags armed; fixed refusal, not successful decode");
        return;
    }
    #[cfg(all(
        misaka_source_local_audited_rust193_v1,
        target_pointer_width = "64",
        target_endian = "little",
        any(target_os = "linux", target_os = "macos")
    ))]
    audited::child_success_with_zero_global_limits();
}

#[cfg(all(
    misaka_source_local_audited_rust193_v1,
    target_pointer_width = "64",
    target_endian = "little",
    any(target_os = "linux", target_os = "macos")
))]
mod audited {
    use super::*;
    use crate::palw_native_resource_v1::ledger_expansion::source_tir_program_shape_v1;
    use misaka_palw_tir::{
        DType, Dim, Ref, TensorType,
        builder::ProgramBuilder,
        program::{HISTORY_BOUND_V1_HELD, LOGITS_I64_Q24_SCHEME_V2, MAX_CARRY},
    };
    macro_rules! require_audited {
        () => {
            if !supported_build() {
                eprintln!("SOURCE_LOCAL_UNSUPPORTED_PROFILE: cfg alone cannot authorize decode; audited success test not run");
                return;
            }
        };
    }

    fn program(wide: bool) -> TirProgramV1 {
        let mut builder = ProgramBuilder::new(256, HISTORY_BOUND_V1_HELD);
        let scores = builder.param("scores", DType::I32, &[4], false);
        let mut pre = builder.block("pre", vec![]);
        let scores = pre.reshape_fixed(scores, &[4]);
        pre.commit(scores);
        let pre = pre.finish(&[scores]);
        let mut post = builder.block("post", vec![TensorType::fixed(DType::I32, &[4])]);
        let output = post.cast(Ref::CarryIn(0), if wide { DType::I64 } else { DType::I32 });
        post.commit(output);
        let Ref::Node(logits) = output else { unreachable!() };
        let post = post.finish(&[]);
        let mut program = builder.finish(pre, vec![], post, logits);
        if wide {
            program.logits_scheme_id = LOGITS_I64_Q24_SCHEME_V2;
        }
        program
    }
    fn limits() -> SourceProgramLocalLimitsV1 {
        SourceProgramLocalLimitsV1 {
            max_program_bytes: MAX_PROGRAM_BYTES as u64,
            max_scan_steps: 1 << 32,
            max_nf_items: 1 << 32,
            max_native_requests: 1 << 40,
        }
    }
    fn work() -> LocalScanWorkBudgetV1 {
        LocalScanWorkBudgetV1::new(1 << 40).unwrap()
    }
    fn quote(bytes: &[u8]) -> SourceProgramLocalQuoteV1 {
        *scan_source_program_local_v1(bytes, limits(), &mut work()).unwrap().quote()
    }
    fn scalar_state(work: &LocalScanWorkBudgetV1) -> (u64, u64, u64, u64, u64) {
        (work.charged_steps(), work.observed_steps(), work.charged_reader_bytes(), work.observed_reader_bytes(), work.attempts())
    }
    fn public_kind(kind: TirErrorKind) -> Error {
        match kind {
            TirErrorKind::Encoding => Error::Encoding,
            TirErrorKind::NormalForm => Error::NormalForm,
            TirErrorKind::Shape => Error::Shape,
            _ => Error::UnsupportedCoverage,
        }
    }

    #[test]
    fn same_bytes_narrow_wide_old_quote_and_four_reader_observations() {
        require_audited!();
        for wide in [false, true] {
            let bytes = program(wide).encode();
            let old = TirProgramV1::decode_canonical(&bytes).unwrap();
            let (shape, old_requests) = source_tir_program_shape_v1(&bytes).unwrap();
            let mut observed = SourceProgramScanObservationV1::default();
            let (_, observed_requests) = source_tir_program_shape_observed_v1(&bytes, &mut observed).unwrap();
            let (actual_steps, actual_bytes, readers) = observed.totals_v1().unwrap();
            assert!(observed.complete_v1());
            assert_eq!(readers, 4);
            assert_eq!(actual_bytes, 4 * bytes.len() as u64 + 4);
            assert_eq!(observed_requests, old_requests);
            let mut work = work();
            let scanned = scan_source_program_local_v1(&bytes, limits(), &mut work).unwrap();
            let q = *scanned.quote();
            assert_eq!(q.source_native_requests(), old_requests);
            assert_eq!(q.nf_items(), shape.structural_items_v1());
            assert_eq!(q.program_bytes(), bytes.len() as u64);
            assert_eq!(work.observed_steps(), actual_steps);
            assert_eq!(work.observed_reader_bytes(), actual_bytes);
            assert!(actual_steps <= q.scan_steps_upper());
            assert_eq!(work.charged_steps(), q.scan_steps_upper());
            assert_eq!(work.charged_reader_bytes(), q.reader_bytes_upper());
            let owner = scanned.decode().unwrap();
            assert_eq!(owner.program(), &old);
            assert_eq!(owner.program().encode(), bytes);
            assert_eq!(owner.quote(), &q);
            assert_eq!(owner.program().wide_logits_q24(), wide);
            let decode_phase = old_requests + q.diagnostic_requests() + q.owner_requests();
            assert_eq!(q.admitted_native_requests(), bootstrap::scan_bootstrap_requests_v1().unwrap().max(decode_phase));
            assert_eq!(q.owner_requests(), (size_of::<LocalSourceProgramOwnerV1>() - size_of::<TirProgramV1>()) as u64);
        }
    }

    #[test]
    fn exact_limits_and_each_one_below_precede_canonical_decode() {
        require_audited!();
        let bytes = program(true).encode();
        let q = quote(&bytes);
        let exact = SourceProgramLocalLimitsV1 {
            max_program_bytes: q.program_bytes(),
            max_scan_steps: q.scan_steps_upper(),
            max_nf_items: q.nf_items(),
            max_native_requests: q.admitted_native_requests(),
        };
        let mut exact_work = LocalScanWorkBudgetV1::new(q.scan_steps_upper()).unwrap();
        let owner = scan_source_program_local_v1(&bytes, exact, &mut exact_work).unwrap().decode().unwrap();
        assert_eq!(owner.program().encode(), bytes);
        for (field, want, charged) in [
            (0, Error::ProgramBytes, false),
            (1, Error::ScanSteps, false),
            (2, Error::NormalFormItems, true),
            (3, Error::NativeRequests, true),
        ] {
            let mut cap = exact;
            match field {
                0 => cap.max_program_bytes -= 1,
                1 => cap.max_scan_steps -= 1,
                2 => cap.max_nf_items -= 1,
                _ => cap.max_native_requests -= 1,
            }
            let mut work = work();
            assert_eq!(scan_source_program_local_v1(&bytes, cap, &mut work).err(), Some(want));
            assert_eq!(work.attempts(), u64::from(charged));
            assert_eq!(work.charged_steps(), if charged { q.scan_steps_upper() } else { 0 });
        }
        for field in 0..4 {
            let mut zero = exact;
            match field {
                0 => zero.max_program_bytes = 0,
                1 => zero.max_scan_steps = 0,
                2 => zero.max_nf_items = 0,
                _ => zero.max_native_requests = 0,
            }
            let mut work = work();
            assert_eq!(scan_source_program_local_v1(&bytes, zero, &mut work).err(), Some(Error::ZeroLimit));
            assert_eq!(scalar_state(&work), (0, 0, 0, 0, 0));
        }
        let mut cap = exact;
        cap.max_native_requests = bootstrap::scan_bootstrap_requests_v1().unwrap() - 1;
        let mut work = work();
        assert_eq!(scan_source_program_local_v1(&bytes, cap, &mut work).err(), Some(Error::NativeRequests));
        assert_eq!(scalar_state(&work), (0, 0, 0, 0, 0));
    }

    #[test]
    fn failed_scans_observe_consumed_bytes_and_retries_spend_full_envelopes() {
        require_audited!();
        let good = program(false).encode();
        let mut truncated = good.clone();
        truncated.pop();
        let mut unknown_version = good.clone();
        unknown_version[..2].copy_from_slice(&u16::MAX.to_le_bytes());
        for bytes in [&truncated, &unknown_version] {
            let mut observation = SourceProgramScanObservationV1::default();
            assert!(source_tir_program_shape_observed_v1(bytes, &mut observation).is_err());
            let (actual_steps, actual_bytes, readers) = observation.totals_v1().unwrap();
            assert!(readers > 0 && readers < 4);
            let (steps, reader_bytes) = scan_upper(bytes.len() as u64).unwrap();
            let mut work = LocalScanWorkBudgetV1::new(2 * steps).unwrap();
            for attempt in 1..=2 {
                assert_eq!(scan_source_program_local_v1(bytes, limits(), &mut work).err(), Some(Error::Encoding));
                assert_eq!(work.attempts(), attempt);
                assert_eq!(work.charged_steps(), attempt * steps);
                assert_eq!(work.charged_reader_bytes(), attempt * reader_bytes);
                assert_eq!(work.observed_steps(), attempt * actual_steps);
                assert_eq!(work.observed_reader_bytes(), attempt * actual_bytes);
            }
            let before = scalar_state(&work);
            assert_eq!(scan_source_program_local_v1(bytes, limits(), &mut work).err(), Some(Error::ScanSteps));
            assert_eq!(scalar_state(&work), before);
        }
        // A claimed String length beyond the remaining frame is a refused skip,
        // not consumption of that malicious length. Fixed original V1 prefix:
        // version2 + primset64 + token/history8 + params-count4 + name-len4.
        let mut hostile = good.clone();
        hostile[78..82].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut observation = SourceProgramScanObservationV1::default();
        assert!(source_tir_program_shape_observed_v1(&hostile, &mut observation).is_err());
        let (_, actual_bytes, _) = observation.totals_v1().unwrap();
        assert!(actual_bytes < 4 * hostile.len() as u64 + 4);
        let mut work = work();
        assert_eq!(scan_source_program_local_v1(&hostile, limits(), &mut work).err(), Some(Error::Encoding));
        assert_eq!(work.observed_reader_bytes(), actual_bytes);
    }

    #[test]
    fn canonical_error_kinds_utf8_and_unvalidated_ranks_are_preserved() {
        require_audited!();
        let mut nf = program(false);
        nf.history_bound = 16;
        let mut shape = program(false);
        let post = shape.schedule.post as usize;
        shape.blocks[post].nodes[0].out.shape = vec![Dim::Fixed(3)];
        let mut rank = program(false);
        let pre = rank.schedule.pre as usize;
        rank.blocks[pre].nodes[0].out.shape = vec![Dim::Fixed(1); 4096];
        rank.blocks[pre].carry_out = vec![0; MAX_CARRY];
        let mut utf8 = program(false).encode();
        assert_eq!(&utf8[82..88], b"scores");
        utf8[82] = 255;
        for bytes in [nf.encode(), shape.encode(), rank.encode(), utf8] {
            let old_error = TirProgramV1::decode_canonical(&bytes).err().unwrap();
            let expected = public_kind(old_error.kind);
            let (_, source_requests) = source_tir_program_shape_v1(&bytes).unwrap();
            let mut work = work();
            let scanned = scan_source_program_local_v1(&bytes, limits(), &mut work).unwrap();
            assert_eq!(scanned.quote().source_native_requests(), source_requests);
            assert_eq!(scanned.decode().err(), Some(expected));
            assert_eq!(work.attempts(), 1); // decode does not charge scanners twice.
        }
    }

    #[test]
    fn bootstrap_branch_types_and_nf_text_are_finite_without_recharging_writers() {
        require_audited!();
        assert!(bootstrap::scan_bootstrap_requests_v1().unwrap() > 0);
        assert!(bootstrap::decode_diagnostic_extra_v1().unwrap() > 0);
        for items in [0, 1, 4096, MAX_PROGRAM_BYTES as u64] {
            bootstrap::check_nf_text_envelope_v1(items, MAX_PROGRAM_BYTES as u64).unwrap();
        }
        assert_eq!(bootstrap::check_nf_text_envelope_v1(u64::MAX, 1), Err(Error::ArithmeticOverflow));
    }

    pub(super) fn child_success_with_zero_global_limits() {
        let bytes = program(false).encode();
        let mut work = work();
        let owner = scan_source_program_local_v1(&bytes, limits(), &mut work).unwrap().decode().unwrap();
        assert_eq!(owner.program().encode(), bytes);
        drop(owner);
        eprintln!("SOURCE_LOCAL_AUDITED_PROFILE: successful local decode with both global limits zero; no native lease");
    }
}
