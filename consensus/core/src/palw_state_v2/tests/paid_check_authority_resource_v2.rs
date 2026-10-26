//! Private existing Program/authority fixture, actual native Receipt/FS folds.
//! The scheduling callback only retains a real concurrent-demand owner; neither
//! an approval nor an error result is injected. No signed native campaign claim.
use super::*;
use crate::palw_native_resource_v1::{
    NativeDemandLeaseV1, NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

// This is a synthetic logical ceiling, not a physical allocation or measured
// capacity. It must admit the fixture's preexisting conservative reservations;
// the callback then occupies precisely the remaining quota after first reads.
const LOGICAL_TEST_LIMIT: u64 = 1 << 60;

fn native(b: &mut TransitionBuilder<'_>, signer: &PalwBondKeyV2, o: &PaidClaimObjectV2) -> Result<(), PalwStateV2Error> {
    super::super::super::palw_claim_challenge_fold_v2::apply_paid_claim_v2(b, &ctx(6, 6, 6), signer, o)
}
fn lock_state(
    fs: bool,
) -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1, PalwPaidClaimAuthorityV2, PalwPaidClaimLockV2) {
    let (state, p, extras, a) = setup_mode(fs);
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    super::super::super::palw_claim_challenge_fold_v2::apply_paid_claim_v2(
        &mut b,
        &ctx(5, 5, 5),
        &a.producer,
        &PaidClaimObjectV2::Lock { enrollment: a.enrollment.id() },
    )
    .unwrap();
    let lock = (*b.state.kernel_route().unwrap().paid_claim_effective_lock_v6(&a).unwrap().unwrap()).clone();
    (b.state, p, extras, a, lock)
}
fn receipt(a: &PalwPaidClaimAuthorityV2, lock: &PalwPaidClaimLockV2, seat: &PalwBondKeyV2) -> PaidClaimReceiptV2 {
    PaidClaimReceiptV2 {
        version: 2,
        enrollment: a.enrollment.id(),
        verification_class: a.class.id(),
        assignment: lock.assignment,
        source_context: lock.context,
        sample_seed: lock.seed,
        evidence_root: a.enrollment.commit_root,
        transcript_root: a.enrollment.prior_transcript,
        signer: palw_kernel_bond_id_v1(seat),
        signed_daa: 6,
        complete: true,
        round: 0,
        message_root: [0; 64],
    }
}
fn occupy_remaining() -> Option<NativeDemandLeaseV1> {
    // Search by real admission, dropping every probe. No payload allocation is
    // simulated; the final lease is a real logical owner until the call ends.
    let mut low = 0;
    let mut high = LOGICAL_TEST_LIMIT;
    while low < high {
        let mid = low + (high - low + 1) / 2;
        match reserve_native_requests_v1(mid) {
            Ok(lease) => {
                drop(lease);
                low = mid;
            }
            Err(_) => high = mid - 1,
        }
    }
    let lease = reserve_native_requests_v1(low).unwrap();
    assert!(lease.as_ref().is_some_and(|owner| owner.memory_v1() > 0), "a real independent quota owner must be retained");
    assert!(reserve_native_requests_v1(1).is_err(), "the separate owner must actually occupy the remaining quota");
    lease
}

fn isolated_child(name: &str, flag: &str) {
    let qualified = format!("{}::{name}", module_path!());
    // libtest lists names without the crate prefix. A zero-test child is not
    // an exercised quota boundary, even when the process exits successfully.
    let exact = qualified.split_once("::").unwrap().1;
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", exact, "--nocapture", "--test-threads=1"])
        .env(flag, "1")
        .env("RUST_MIN_STACK", "67108864")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    print!("{stdout}");
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.lines().any(|line| line == "running 1 test"), "isolated child must actually execute exactly one test");
    assert!(output.status.success(), "isolated authority-read child failed");
}
#[test]
fn paid_check_authority_native_resource_after_first_reads_keeps_receipt_atomic() {
    const CHILD: &str = "MISAKA_PAID_AUTHORITY_SECOND_READ_CHILD_V2";
    if std::env::var_os(CHILD).is_none() {
        isolated_child("paid_check_authority_native_resource_after_first_reads_keeps_receipt_atomic", CHILD);
        return;
    }
    let (state, p, extras, a, lock) = lock_state(false);
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    let seat = lock.seats[0];
    let object = PaidClaimObjectV2::Receipt { receipt: receipt(&a, &lock, &seat) };
    let root = b.state.state_root();
    let journal_len = b.entries.len();
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LOGICAL_TEST_LIMIT, database_bytes: LOGICAL_TEST_LIMIT }));
    assert_eq!(crate::palw_native_resource_v1::native_declared_logical_limits_v1().unwrap().memory_bytes, LOGICAL_TEST_LIMIT);
    let reached = Rc::new(Cell::new(false));
    let blocker = Rc::new(RefCell::new(None));
    let (seen, held) = (reached.clone(), blocker.clone());
    let result = crate::palw_claim_challenge_v2::with_before_check_authority_v2(
        move |site| {
            assert_eq!(site, "ordinary-receipt");
            seen.set(true);
            *held.borrow_mut() = occupy_remaining();
        },
        || native(&mut b, &seat, &object),
    );
    assert!(reached.get(), "the first authority/ledger/lock reads must actually finish: {result:?}");
    assert!(
        matches!(result, Err(PalwStateV2Error::NativeResource(_))),
        "second-read local capacity must not become semantic refusal: {result:?}"
    );
    assert_eq!(b.state.state_root(), root);
    assert_eq!(b.entries.len(), journal_len);
    drop(blocker.borrow_mut().take());
    native(&mut b, &seat, &object).unwrap();
    assert_eq!(
        palw_paid_claim_read_v1(b.state.kernel_route().unwrap(), &a.enrollment.base_claim, 0).unwrap().unwrap().receipts.len(),
        1
    );
}
#[test]
fn paid_check_authority_native_positive_quorum_and_outsider_preserve_approval_rules() {
    let (state, p, extras, a, lock) = lock_state(false);
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    let root = b.state.state_root();
    let journal_len = b.entries.len();
    let outsider = bond_key(999);
    let invalid = PaidClaimObjectV2::Receipt { receipt: receipt(&a, &lock, &outsider) };
    assert!(matches!(native(&mut b, &outsider, &invalid), Err(PalwStateV2Error::KernelRouteRefused(_))));
    assert_eq!(b.state.state_root(), root);
    assert_eq!(b.entries.len(), journal_len);
    let first = lock.seats[0];
    native(&mut b, &first, &PaidClaimObjectV2::Receipt { receipt: receipt(&a, &lock, &first) }).unwrap();
    let partial = palw_paid_claim_read_v1(b.state.kernel_route().unwrap(), &a.enrollment.base_claim, 0).unwrap().unwrap();
    assert_eq!(partial.state, "round-locked");
    assert_eq!(partial.receipts.len(), 1);
    assert!(!b.state.kernel_route().unwrap().ledger_checked_v1().unwrap().paid_claim_consumers_v2[&a.enrollment.base_claim].complete);
    for seat in lock.seats.iter().skip(1) {
        native(&mut b, seat, &PaidClaimObjectV2::Receipt { receipt: receipt(&a, &lock, seat) }).unwrap();
    }
    assert_eq!(
        palw_paid_claim_read_v1(b.state.kernel_route().unwrap(), &a.enrollment.base_claim, 0).unwrap().unwrap().state,
        "round-complete"
    );
    assert!(b.state.kernel_route().unwrap().ledger_checked_v1().unwrap().paid_claim_consumers_v2[&a.enrollment.base_claim].complete);
}

#[test]
fn paid_check_authority_fs_second_read_resource_is_not_a_source_approval() {
    const CHILD: &str = "MISAKA_PAID_AUTHORITY_FS_SECOND_READ_CHILD_V2";
    if std::env::var_os(CHILD).is_none() {
        isolated_child("paid_check_authority_fs_second_read_resource_is_not_a_source_approval", CHILD);
        return;
    }
    // Storage-path negative only, like the existing FS projection fixture.
    // An interactive class cannot obtain this bootstrap lock through Lock; do
    // not turn the row's source_valid flag into a valid source/approval claim.
    let (mut state, p, extras, a) = setup_mode(true);
    let lock = PalwPaidClaimLockV2 {
        enrollment: a.enrollment.id(),
        context: a.context.id(),
        anchor: paid_claim_bootstrap_anchor_v2(),
        seed: [20; 64],
        locked_daa: 5,
        assignment: [21; 64],
        seats: a.checker_candidates.iter().map(|c| c.bond).collect(),
        quorum: 3,
        source_valid: true,
    };
    state.kernel_route.as_mut().unwrap().aux.insert((PAID_CLAIM_LOCKS_V2, a.enrollment.id().to_vec()), borsh::to_vec(&lock).unwrap());
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    let message =
        PaidClaimObjectV2::StageMessage { enrollment: a.enrollment.id(), round: 0, prior_transcript: [0; 64], message_root: [22; 64] };
    let root = b.state.state_root();
    let journal_len = b.entries.len();
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LOGICAL_TEST_LIMIT, database_bytes: LOGICAL_TEST_LIMIT }));
    assert_eq!(crate::palw_native_resource_v1::native_declared_logical_limits_v1().unwrap().memory_bytes, LOGICAL_TEST_LIMIT);
    let reached = Rc::new(Cell::new(false));
    let blocker = Rc::new(RefCell::new(None));
    let (seen, held) = (reached.clone(), blocker.clone());
    let result = crate::palw_claim_challenge_v2::with_before_check_authority_v2(
        move |site| {
            assert_eq!(site, "fs-initial-source");
            seen.set(true);
            *held.borrow_mut() = occupy_remaining();
        },
        || native(&mut b, &a.producer, &message),
    );
    assert!(reached.get(), "FS preliminary identity/clock/message checks must pass before the second read: {result:?}");
    assert!(
        matches!(result, Err(PalwStateV2Error::NativeResource(_))),
        "FS local capacity must not become invalid-source refusal: {result:?}"
    );
    assert_eq!(b.state.state_root(), root);
    assert_eq!(b.entries.len(), journal_len);
    drop(blocker.borrow_mut().take());
    // Release of capacity does not authenticate this projected source. The
    // existing no-interactive-bootstrap guard still refuses the same message.
    assert!(matches!(native(&mut b, &a.producer, &message), Err(PalwStateV2Error::KernelRouteRefused(_))));
    assert_eq!(b.state.state_root(), root);
    assert_eq!(b.entries.len(), journal_len);
}

#[test]
fn paid_check_authority_present_corruption_and_true_absence_are_decode_errors() {
    use crate::palw_native_resource_v1::NativeDecodeErrorV1 as E;
    let (mut state, _, _, a, _) = lock_state(false);
    let route = state.kernel_route.as_mut().unwrap();
    let saved = route.aux.remove(&(PAID_CLAIM_ENROLLMENTS_V2, a.context.id().to_vec())).unwrap();
    assert!(matches!(route.paid_claim_check_authority_checked_v2(&a.context.id(), 6), Err(E::Decode(_))));
    route.aux.insert((PAID_CLAIM_ENROLLMENTS_V2, a.context.id().to_vec()), vec![255]);
    assert!(matches!(route.paid_claim_check_authority_checked_v2(&a.context.id(), 6), Err(E::Decode(_))));
    route.aux.insert((PAID_CLAIM_ENROLLMENTS_V2, a.context.id().to_vec()), saved);
    assert!(route.paid_claim_check_authority_checked_v2(&a.context.id(), 6).is_ok());
    assert!(
        route.paid_claim_check_authority_v2(&a.context.id(), 6).is_ok(),
        "String SDK compatibility wrapper keeps the same positive authority"
    );
}

#[test]
fn paid_normal_source_initial_authority_keeps_primary_identity_and_corruption_class() {
    use crate::palw_native_resource_v1::NativeDecodeErrorV1 as E;
    let (mut state, _, _, a) = setup();
    let route = state.kernel_route.as_mut().unwrap();
    route.aux.insert(
        (crate::palw_beacon_source_opt_in_v1::PALW_BEACON_SOURCE_TABLE_TARGETS_V4, a.context.id().to_vec()),
        borsh::to_vec(&a.source_target).unwrap(),
    );
    let source = route.rfc07_normal_source_authority_checked_v3(&a.context.id()).unwrap();
    assert_eq!(source.authority(), &a);
    assert_eq!(source.context(), &a.context);
    assert_eq!(source.target(), &a.source_target);
    drop(source);
    route.aux.insert((PAID_CLAIM_ENROLLMENTS_V2, a.context.id().to_vec()), vec![255]);
    assert!(matches!(route.rfc07_normal_source_authority_checked_v3(&a.context.id()), Err(E::Decode(_))));
}
