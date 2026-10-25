//! Phase-1 projection/ownership controls. These directly assembled
//! rows test historical codec/read compatibility, never model approval.
use super::*;
use crate::palw_native_resource_v1::*;
fn hash(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}
fn fixture() -> PalwChainStateV2 {
    let mut s = PalwChainStateV2::genesis();
    s.classes.insert(
        hash(2),
        PalwClassStateV2 {
            artifact_root: hash(3),
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(10),
            status: PalwClassStatusV2::Active,
            registered_daa: 7,
            registrant_bond: None,
            fused_attention: false,
        },
    );
    s.model_positions.insert((hash(2), hash(4)), 11);
    // Historical read rows populate every owned dynamic view collection. They
    // are not accepted model-review, registration, payment or claim evidence.
    use crate::palw_model_lines_v1::*;
    let version = s.model_version(&hash(2), 1).unwrap();
    for line in [6, 7] {
        s.model_lines.insert(hash(line), founding_line_v1(hash(2), None, vec![line; 17], 7));
        s.model_versions.insert((hash(line), 1), version.clone());
    }
    let by = PalwBondKeyV2(TransactionOutpoint::new(crate::tx::TransactionId::from_bytes([0x55; 64]), 0));
    for (id, line) in [(8, 6), (9, 6), (10, 7)] {
        s.model_proposals.insert(
            hash(id),
            PalwModelProposalV1 { line_id: hash(line), root: hash(3), note_hash: hash(4), by, posted_daa: 7, adopted_in: None },
        );
        s.model_evaluations.insert(
            (hash(line), 1, PalwBondKeyV2(TransactionOutpoint::new(crate::tx::TransactionId::from_bytes([id; 64]), 0))),
            PalwModelEvaluationV1 {
                evaluator_id: hash(id),
                score_permille: 1,
                report_hash: hash(4),
                posted_daa: 7,
                is_lines_own: false,
            },
        );
    }
    s.model_markets.insert(hash(6), crate::palw_model_market_v1::PalwModelMarketV1::pledge_v1(7, 100, hash(4)));
    s
}
fn params() -> PalwStateParamsV2 {
    PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, hash(1), 4, 1000, 100, 1000, 0).unwrap()
}
fn lifecycle(state: crate::palw_model_registry_v1::PalwModelLifecycleV1) -> crate::palw_model_registry_v1::PalwModelLifecycleRowV1 {
    use crate::palw_model_registry_v1::*;
    PalwModelLifecycleRowV1 {
        state,
        work: PalwModelWorkV1::default(),
        profile: PalwDerivedProfileV1::default(),
        since_span: 0,
        probes_passed: 0,
        probes_failed: 0,
        probes_passed_this_span: 0,
        probes_failed_this_span: 0,
        ready_seats: 0,
        inflight_claims: 0,
        utilization_permille: 0,
        admission_milli: 0,
        cap_utilization_permille: 0,
        priced_share_permille: 0,
    }
}
#[test]
fn historical_evm_view_draft_payload_matches_existing_projection() {
    let s = fixture();
    let raw = s.evm_view_v1(crate::evm::EVM_CHAIN_ID, hash(1));
    let before = s.evm_view_quote_v2().unwrap();
    let source = NativeRetainedV1::retain_existing_v1(s).unwrap();
    let owned = source.evm_view_owned_v2(crate::evm::EVM_CHAIN_ID, hash(1), NativeEvmMarketGateInputsV2::default()).unwrap();
    assert_eq!(&**owned, &raw);
    let mut larger = fixture();
    larger.model_positions.insert((hash(2), hash(5)), 1);
    assert!(larger.evm_view_quote_v2().unwrap().persistent_bytes > before.persistent_bytes);
}
#[test]
fn historical_evm_view_draft_reason_matches_legacy_all_modes_and_lifecycles() {
    use crate::palw_model_registry_v1::*;
    let mut s = fixture();
    let p = params();
    let states = [
        None,
        Some(PalwModelLifecycleV1::Registered),
        Some(PalwModelLifecycleV1::Prefetching),
        Some(PalwModelLifecycleV1::Probation { probes_passed: 1 }),
        Some(PalwModelLifecycleV1::ActiveLimited { stable_epochs: 1 }),
        Some(PalwModelLifecycleV1::Active),
        Some(PalwModelLifecycleV1::Held),
        Some(PalwModelLifecycleV1::Candidate),
        Some(PalwModelLifecycleV1::Capped { since_daa: 1 }),
    ];
    for status in [
        PalwClassStatusV2::Active,
        PalwClassStatusV2::Frozen { since_daa: 1 },
        PalwClassStatusV2::Registered { activation_daa: 9, pending_share_permille: 0 },
        PalwClassStatusV2::Dormant { since_daa: 1 },
    ] {
        s.classes.get_mut(&hash(2)).unwrap().status = status;
        for row in states.iter().cloned() {
            s.model_lifecycles.clear();
            if let Some(row) = row {
                s.model_lifecycles.insert(hash(2), lifecycle(row));
            }
            for mask in 0u8..16 {
                let g = NativeEvmMarketGateInputsV2 {
                    audit_active: mask & 1 != 0,
                    virtual_active: mask & 2 != 0,
                    registry_present: mask & 4 != 0,
                    work_target_active: mask & 8 != 0,
                };
                let extras = PalwTransitionExtrasV1 {
                    audit_2026_09_23_active: g.audit_active,
                    model_virtual_v1: g.virtual_active.then_some(0),
                    work_target_active: g.work_target_active,
                    model_registry: g.registry_present.then(|| PalwModelRegistryFoldV1 {
                        globals: PALW_REGISTRY_GLOBALS_V1,
                        span_daa: 1,
                        genesis_works: Default::default(),
                        grace_until_daa: 0,
                        admission_audit_period_daa: None,
                        readiness_v2_active: false,
                        bond_maturity: None,
                    }),
                    ..Default::default()
                };
                assert_eq!(NativeEvmMarketGateInputsV2::from_extras_v2(&extras), g);
                for class in [hash(1), hash(2), hash(9)] {
                    let old = palw_model_market_admits_v1(&s, &p, &extras, &class);
                    let new = s.evm_market_refusal_v2(p.base_class_id(), g, &class).map(|reason| reason.into_legacy_error_v2(class));
                    assert_eq!(format!("{:?}", old.err()), format!("{new:?}"));
                }
            }
        }
    }
}
#[test]
fn historical_evm_view_draft_quota_before_build_and_final_arc_drop() {
    const CHILD: &str = "MISAKA_HISTORICAL_EVM_VIEW_DRAFT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::historical_evm_view_draft_quota_before_build_and_final_arc_drop", module_path!());
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
    let s = fixture();
    let q = s.evm_view_quote_v2().unwrap();
    let total = q.persistent_bytes + q.scratch_bytes;
    let source = s.native_heap_v1("phase1 source").bytes().unwrap()
        + (size_of::<NativeRetainedV1<PalwChainStateV2>>() + size_of::<[usize; 3]>()) as u64;
    const LIMIT: u64 = 32 << 20;
    assert!(source + total < LIMIT);
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let state = NativeRetainedV1::retain_existing_v1(s).unwrap();
    let blocker = reserve_native_requests_v1(LIMIT - source - (total - 1)).unwrap();
    BUILT.with(|n| n.set(0));
    assert!(state.evm_view_owned_v2(crate::evm::EVM_CHAIN_ID, hash(1), Default::default()).is_err());
    assert_eq!(BUILT.with(std::cell::Cell::get), 0);
    drop(blocker);
    let blocker = reserve_native_requests_v1(LIMIT - source - total).unwrap();
    let owner = state.evm_view_owned_v2(crate::evm::EVM_CHAIN_ID, hash(1), Default::default()).unwrap();
    assert_eq!(BUILT.with(std::cell::Cell::get), 1);
    let remaining_scratch = reserve_native_requests_v1(q.scratch_bytes).unwrap();
    assert!(reserve_native_requests_v1(1).is_err());
    let handler = Arc::clone(&owner);
    drop(owner);
    assert_eq!(handler.positions.get(&(hash(2), hash(4))), Some(&11));
    assert!(reserve_native_requests_v1(1).is_err(), "last handler Arc still owns the persistent ticket");
    drop(handler);
    assert!(reserve_native_requests_v1(1).is_ok());
    drop(remaining_scratch);
    drop(blocker);
    drop(state);
    assert!(reserve_native_requests_v1(LIMIT).is_ok(), "all source/view/temporary reservations return");
}

/// Fixed old outcomes, independent of the newly shared reason function. These
/// historical reader rows do not authorize model examination or promotion.
#[test]
fn historical_evm_view_legacy_golden_refusal_order_and_reason() {
    use crate::palw_model_registry_v1::*;
    let mut s = fixture();
    let p = params();
    let registry = || PalwModelRegistryFoldV1 {
        globals: PALW_REGISTRY_GLOBALS_V1,
        span_daa: 1,
        genesis_works: Default::default(),
        grace_until_daa: 0,
        admission_audit_period_daa: None,
        readiness_v2_active: false,
        bond_maturity: None,
    };
    let mut extras = PalwTransitionExtrasV1 {
        audit_2026_09_23_active: true,
        model_virtual_v1: Some(1),
        model_registry: Some(registry()),
        work_target_active: true,
        ..Default::default()
    };
    assert!(matches!(palw_model_market_admits_v1(&s,&p,&extras,&hash(9)),Err(PalwStateV2Error::MissingClass(id)) if id==hash(9)));
    s.classes.get_mut(&hash(2)).unwrap().status = PalwClassStatusV2::Frozen { since_daa: 1 };
    assert!(
        matches!(palw_model_market_admits_v1(&s,&p,&extras,&hash(2)),Err(PalwStateV2Error::ModelClassNotActive(id)) if id==hash(2))
    );
    s.classes.get_mut(&hash(2)).unwrap().status = PalwClassStatusV2::Active;
    s.model_lifecycles.insert(hash(2), lifecycle(PalwModelLifecycleV1::Probation { probes_passed: 1 }));
    assert!(matches!(palw_model_market_admits_v1(&s,&p,&extras,&hash(2)),Err(PalwStateV2Error::ModelClassNotTrading{class,state})
        if class==hash(2)&&state=="Probation { probes_passed: 1 }"));
    extras.model_virtual_v1 = None;
    s.model_lifecycles.insert(hash(2), lifecycle(PalwModelLifecycleV1::Held));
    assert!(matches!(palw_model_market_admits_v1(&s,&p,&extras,&hash(2)),Err(PalwStateV2Error::ModelClassNotEligible{class,state})
        if class==hash(2)&&state=="Held"));
    s.model_lifecycles.clear();
    assert!(matches!(palw_model_market_admits_v1(&s,&p,&extras,&hash(2)),Err(PalwStateV2Error::ModelClassNotEligible{class,state})
        if class==hash(2)&&state=="no row"));
    extras.audit_2026_09_23_active = false;
    assert!(palw_model_market_admits_v1(&s, &p, &extras, &hash(2)).is_ok());
    extras.model_virtual_v1 = Some(1);
    extras.model_registry = None;
    assert!(palw_model_market_admits_v1(&s, &p, &extras, &hash(2)).is_ok());
}
