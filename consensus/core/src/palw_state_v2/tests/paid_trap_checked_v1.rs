//! Checked paid-trap lookup over the real native paid-claim fixture. The
//! fixture's full authority/index codec is used, without introducing new policy.
#![allow(warnings)]
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

// Actual sweep mechanics over the full native paid-authority fixture. The
// terminal and held/receipt rows are explicit local-state fixture setup, not an
// accepted court campaign or a new public arming configuration.
fn sweep_fixture() -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1, PalwPaidClaimAuthorityV2) {
    use crate::palw_paid_claim_trap_v1::*;
    let (mut state, p, extras, a) = setup();
    let route = state.kernel_route.as_mut().unwrap();
    let base = a.enrollment.base_claim;
    let mut kernel = route.kernel_claim_row_v1(&Hash64::from_bytes(base)).unwrap();
    kernel.convicted = true;
    kernel.life.state = misaka_palw_kernel::lifecycle::ClaimStateV1::Convicted { daa: 9 };
    route.rows.insert((misaka_palw_kernel::rows::TABLE_CLAIMS_V1, base.to_vec()), borsh::to_vec(&kernel).unwrap());
    for (slot, candidate) in a.checker_candidates.iter().take(2).enumerate() {
        // The original fixture shares a payout payload; separate these local
        // test recipients so a forfeited fee cannot hide in another's total.
        state.bonds.get_mut(&candidate.bond).unwrap().payout_payload = h64(900 + slot as u64);
        let verifier = palw_kernel_bond_id_v1(&candidate.bond);
        let receipt = PaidClaimReceiptV2 {
            version: 2,
            enrollment: a.enrollment.id(),
            verification_class: a.class.id(),
            assignment: [0; 64],
            source_context: a.context.id(),
            sample_seed: [0; 64],
            evidence_root: [0; 64],
            transcript_root: [0; 64],
            signer: verifier,
            signed_daa: 8,
            complete: slot == 0,
            round: 0,
            message_root: [0; 64],
        };
        let receipt_id = object_id(b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2", &(receipt.enrollment, verifier));
        route.aux.insert((PAID_CLAIM_RECEIPTS_V2, receipt_id.to_vec()), borsh::to_vec(&receipt).unwrap());
        let held = PalwHeldCheckFeeRowV1 {
            version: PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
            claim: base,
            verifier,
            slot: slot as u64,
            poster: a.enrollment.producer,
            fee: 1_000,
            release_after_daa: 10,
        };
        route.aux.insert(
            (PALW_PAID_CLAIM_TRAP_TABLE_V1, palw_paid_trap_held_fee_key_v1(&base, &verifier, slot as u64)),
            borsh::to_vec(&held).unwrap(),
        );
    }
    (state, armed(p), extras, a)
}

#[test]
fn actual_held_fee_sweep_uses_full_paid_authority_and_forfeits_clean_fee_once() {
    use crate::palw_paid_claim_trap_v1::*;
    let (state, p, extras, a) = sweep_fixture();
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    crate::palw_state_v2::palw_paid_claim_trap_fold_v1::release_held_check_fees_v1(&mut b, 11).unwrap();
    let (_, kept) = palw_paid_trap_forfeit_v1(1_000);
    for (slot, candidate) in a.checker_candidates.iter().take(2).enumerate() {
        let verifier = palw_kernel_bond_id_v1(&candidate.bond);
        let key = (PALW_PAID_CLAIM_TRAP_TABLE_V1, palw_paid_trap_held_fee_key_v1(&a.enrollment.base_claim, &verifier, slot as u64));
        assert!(!b.state.kernel_route.as_ref().unwrap().aux.contains_key(&key));
        let payload = b.state.bonds[&candidate.bond].payout_payload;
        let amount = b.state.pending_payouts.get(&palw_kernel_payout_key_v1(&payload)).map_or(0, |r| r.amount);
        assert_eq!(amount, if slot == 0 { kept } else { 1_000 }, "actual sweep derives Clean status rather than a test boolean");
    }
    let after = b.state.state_root();
    crate::palw_state_v2::palw_paid_claim_trap_fold_v1::release_held_check_fees_v1(&mut b, 12).unwrap();
    assert_eq!(b.state.state_root(), after, "the repeated actual sweep releases nothing twice");
}

#[test]
fn actual_held_fee_sweep_missing_base_or_later_corruption_keeps_all_rows_and_payouts() {
    use crate::palw_paid_claim_trap_v1::*;
    for later_corruption in [false, true] {
        let (mut state, p, extras, a) = sweep_fixture();
        let route = state.kernel_route.as_mut().unwrap();
        if later_corruption {
            // The first paid claim is valid and due. A later damaged due row
            // must still be discovered before the first payout/deletion.
            let other = [0xff; 64];
            assert!(a.enrollment.base_claim < other, "the corrupt due key must sort after the valid claim");
            let kernel = route.rows[&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, a.enrollment.base_claim.to_vec())].clone();
            route.rows.insert((misaka_palw_kernel::rows::TABLE_CLAIMS_V1, other.to_vec()), kernel);
            route
                .aux
                .insert((PAID_CLAIM_ENROLLMENTS_V2, paid_claim_index_key_v3(PAID_CLAIM_BASE_INDEX_PREFIX_V3, &other)), vec![0xff]);
            let verifier = palw_kernel_bond_id_v1(&a.checker_candidates[0].bond);
            let held = PalwHeldCheckFeeRowV1 {
                version: PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
                claim: other,
                verifier,
                slot: 0,
                poster: a.enrollment.producer,
                fee: 1_000,
                release_after_daa: 10,
            };
            route.aux.insert(
                (PALW_PAID_CLAIM_TRAP_TABLE_V1, palw_paid_trap_held_fee_key_v1(&other, &verifier, 0)),
                borsh::to_vec(&held).unwrap(),
            );
        } else {
            route.aux.remove(&(
                PAID_CLAIM_ENROLLMENTS_V2,
                paid_claim_index_key_v3(PAID_CLAIM_BASE_INDEX_PREFIX_V3, &a.enrollment.base_claim),
            ));
        }
        let root = state.state_root();
        let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
        assert!(crate::palw_state_v2::palw_paid_claim_trap_fold_v1::release_held_check_fees_v1(&mut b, 11).is_err());
        assert_eq!(b.state.state_root(), root, "no held row, payout or settlement is changed before all due reads succeed");
    }
}

#[test]
fn actual_held_fee_sweep_quota_refusal_retains_due_fees_without_false_payout() {
    const CHILD: &str = "MISAKA_PAID_TRAP_SWEEP_QUOTA_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name = format!("{}::actual_held_fee_sweep_quota_refusal_retains_due_fees_without_false_payout", module_path!());
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
    let (state, p, extras, _) = sweep_fixture();
    let root = state.state_root();
    let mut b = TransitionBuilder::new(&state, &p, false, false, false, false, &extras);
    use crate::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 }));
    assert!(matches!(
        crate::palw_state_v2::palw_paid_claim_trap_fold_v1::release_held_check_fees_v1(&mut b, 11),
        Err(PalwStateV2Error::NativeResource(_))
    ));
    assert_eq!(b.state.state_root(), root);
}
