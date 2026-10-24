//! Checked paid-trap lookup over the real native paid-claim fixture. The
//! fixture's full authority/index codec is used, without introducing new policy.
use super::*;
use crate::palw_mesh_v1::PalwTrapRevealedV1;
use crate::palw_state_v2::palw_paid_claim_trap_fold_v1::{check_fee_hold_until_checked_v1, on_paid_claim_convictions_v1};
use misaka_palw_kernel::{
    hash::Digest,
    ledger::LedgerEventV1,
    settle::{SettlementInstructionV1, SettlementKindV1},
};

fn armed(p: PalwStateParamsV2) -> PalwStateParamsV2 {
    p.with_paid_claim_trap_from(Some(0)).with_audit_mesh_from_daa(Some(0))
}
fn reveal(a: &PalwPaidClaimAuthorityV2) -> PalwTrapRevealedV1 {
    PalwTrapRevealedV1 {
        setter_bond: a.producer,
        claim: Hash64::from_bytes(a.enrollment.base_claim),
        fault_leaf: 0,
        tiles: 1,
        salt: [7; 32],
        signature: vec![],
    }
}
fn settlement(bond: Digest, kind: SettlementKindV1, claim: Digest) -> LedgerEventV1 {
    LedgerEventV1::Settlement(SettlementInstructionV1 { bond, amount: 1, kind, claim: Some(claim) })
}
fn fees(claim: Digest, poster: Digest, verifier: Digest) -> Vec<LedgerEventV1> {
    vec![
        settlement(poster, SettlementKindV1::PayJobEscrow, claim),
        settlement(poster, SettlementKindV1::ReleaseJobEscrow, claim),
        settlement(verifier, SettlementKindV1::FinalReward, claim),
        LedgerEventV1::CheckFeePaid { claim, verifier, amount: 1 },
    ]
}

#[test]
fn checked_paid_base_lookup_retains_exact_authority_and_native_fee_hold_without_ledger_rebuild() {
    let (mut state, p, extras, a) = setup();
    let p = armed(p);
    let claim = a.enrollment.base_claim;
    let got = state.kernel_route.as_ref().unwrap().paid_claim_authority_for_base_checked_v1(&claim).unwrap().unwrap();
    assert_eq!(*got, a);
    drop(got);
    let paid = state.paid_trap_target_checked_v1(&p, &Hash64::from_bytes(claim), 10).unwrap().unwrap();
    let b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    assert_eq!(check_fee_hold_until_checked_v1(&b, &claim, 10).unwrap(), Some(paid.reveal_end()));
    drop(b);
    // This structural API must not rebuild ledger rows while those same paid
    // rows are being projected. Removing unrelated ledger data here is only a
    // test of that property, never an accepted native state fixture.
    state.kernel_route.as_mut().unwrap().rows.clear();
    assert_eq!(*state.kernel_route.as_ref().unwrap().paid_claim_authority_for_base_checked_v1(&claim).unwrap().unwrap(), a);
}

#[test]
fn corrupt_paid_targets_refuse_actual_mesh_and_fee_consumers_before_any_write() {
    for corruption in 0..7 {
        let (mut state, p, extras, a) = setup();
        let p = armed(p);
        let base = a.enrollment.base_claim;
        let route = state.kernel_route.as_mut().unwrap();
        let key = (PAID_CLAIM_ENROLLMENTS_V2, paid_claim_index_key_v3(PAID_CLAIM_BASE_INDEX_PREFIX_V3, &base));
        let primary = (PAID_CLAIM_ENROLLMENTS_V2, a.context.id().to_vec());
        match corruption {
            0 => {
                route.aux.insert(key, vec![0xff]);
            }
            1 => {
                route.aux.remove(&primary);
            }
            2 => {
                route.aux.insert(primary, vec![0xff]);
            }
            3 => {
                let mut wrong = a.clone();
                wrong.version = 3;
                route.aux.insert(primary, borsh::to_vec(&wrong).unwrap());
            }
            4 => {
                let mut wrong = a.clone();
                wrong.enrollment.base_claim = [0xA5; 64];
                route.aux.insert(primary, borsh::to_vec(&wrong).unwrap());
            }
            5 => {
                route
                    .aux
                    .remove(&(PAID_CLAIM_ENROLLMENTS_V2, paid_claim_index_key_v3(PAID_CLAIM_ID_INDEX_PREFIX_V3, &a.enrollment.id())));
            }
            _ => {
                let mut index: PalwPaidClaimIndexV3 = borsh::from_slice(&route.aux[&key]).unwrap();
                index.owner = [0xA5; 64];
                route.aux.insert(key, borsh::to_vec(&index).unwrap());
            }
        }
        let r = reveal(&a);
        let root = state.state_root();
        let expected = state.paid_trap_target_checked_v1(&p, &r.claim, 10).unwrap_err().to_string();
        assert_eq!(state.trap_revealed_admissible_v3(&p, &r, 10), Err(expected), "acceptance cannot select legacy mesh");
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
        assert!(crate::palw_state_v2::palw_mesh_fold_v1::apply_trap_revealed_v1(&mut b, &ctx(10, 10, 10), &r).is_err());
        assert_eq!(b.state.state_root(), root);
        let batch = fees(base, a.enrollment.producer, [0x33; 64]);
        assert!(crate::palw_state_v2::palw_kernel_route_fold_v1::apply_settlements(&mut b, &batch, false, 10).is_err());
        assert_eq!(b.state.state_root(), root, "a corrupt target writes no held/settled fee or payout");
        let conviction =
            LedgerEventV1::Convicted { claim: base, accuser: [0x33; 64], slashed: 1, accuser_reward: 0, post_final: false };
        assert!(on_paid_claim_convictions_v1(&mut b, &[conviction], 10).is_err());
        assert_eq!(b.state.state_root(), root, "a corrupt target writes no conviction or signer charge");
    }
    // A later corrupt target must not leave an earlier valid paid conviction
    // journaled in this same batch before the caller receives the refusal.
    let (mut state, p, extras, a) = setup();
    let p = armed(p);
    let other = [0xF1; 64];
    state
        .kernel_route
        .as_mut()
        .unwrap()
        .aux
        .insert((PAID_CLAIM_ENROLLMENTS_V2, paid_claim_index_key_v3(PAID_CLAIM_BASE_INDEX_PREFIX_V3, &other)), vec![0xff]);
    let root = state.state_root();
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    let events: Vec<_> = [a.enrollment.base_claim, other]
        .into_iter()
        .map(|claim| LedgerEventV1::Convicted { claim, accuser: [0x33; 64], slashed: 1, accuser_reward: 0, post_final: false })
        .collect();
    assert!(on_paid_claim_convictions_v1(&mut b, &events, 10).is_err());
    assert_eq!(b.state.state_root(), root, "preflight prevents a partial earlier conviction write");
    let mut mixed_fees = fees(a.enrollment.base_claim, a.enrollment.producer, [0x33; 64]);
    mixed_fees.extend(fees(other, a.enrollment.producer, [0x33; 64]));
    assert!(crate::palw_state_v2::palw_kernel_route_fold_v1::apply_settlements(&mut b, &mixed_fees, false, 10).is_err());
    assert_eq!(b.state.state_root(), root, "preflight refuses the later corrupt fee target before the earlier valid target writes");
}

#[test]
fn genuine_absence_and_dormant_trap_keep_the_original_mesh_and_fee_paths() {
    let (mut state, p, extras, a) = setup();
    let r = reveal(&a);
    let route = state.kernel_route.as_mut().unwrap();
    route.aux.insert(
        (PAID_CLAIM_ENROLLMENTS_V2, paid_claim_index_key_v3(PAID_CLAIM_BASE_INDEX_PREFIX_V3, &a.enrollment.base_claim)),
        vec![0xff],
    );
    assert!(state.paid_trap_target_checked_v1(&p, &r.claim, 10).unwrap().is_none(), "dormant lookup must not inspect corrupt rows");
    assert_eq!(
        state.trap_revealed_admissible_v3(&p, &r, 10),
        state.mesh_trap_revealed_admissible_v2(&r, 10, p.audit_1004_active_at(10)).map_err(|e| e.to_string())
    );
    let p = armed(p);
    let ordinary = Hash64::from_bytes([0xEE; 64]);
    assert!(state.paid_trap_target_checked_v1(&p, &ordinary, 10).unwrap().is_none());
    let b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    assert_eq!(check_fee_hold_until_checked_v1(&b, &ordinary.as_bytes(), 10).unwrap(), None);
}

#[test]
fn native_capacity_refuses_paid_trap_reads_without_mutation_or_dormant_lookup() {
    const CHILD: &str = "MISAKA_PAID_TRAP_CHECKED_CAPACITY_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name = format!("{}::native_capacity_refuses_paid_trap_reads_without_mutation_or_dormant_lookup", module_path!());
        let name = name.split_once("::").unwrap().1;
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", name, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    let (state, p, extras, a) = setup();
    let r = reveal(&a);
    let root = state.state_root();
    let active = armed(p.clone());
    // Admit the fixture transition before constraining subsequent consumer
    // reads. Production transition construction keeps its own refusal boundary.
    let mut b = TransitionBuilder::new(&state, &active, false, false, false, false, &extras);
    use crate::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 }));
    assert!(state.paid_trap_target_checked_v1(&p, &r.claim, 10).unwrap().is_none());
    assert!(matches!(state.paid_trap_target_checked_v1(&active, &r.claim, 10), Err(PalwStateV2Error::NativeResource(_))));
    assert!(matches!(
        crate::palw_state_v2::palw_mesh_fold_v1::apply_trap_revealed_v1(&mut b, &ctx(10, 10, 10), &r),
        Err(PalwStateV2Error::NativeResource(_))
    ));
    assert_eq!(b.state.state_root(), root);
}
