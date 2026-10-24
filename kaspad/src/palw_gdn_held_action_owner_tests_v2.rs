//! Actual held-filer/terminal controls. Earlier case/evidence allocations are
//! intentionally outside this signed-output ownership boundary.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::{NativeCoverageV1, NativeRetainedV1};

fn refused() -> NativeCoverageV1 {
    NativeCoverageV1::unsupported::<PalwConsensusObjectV2>("held GDN destination test", "destination capacity refused")
}
#[test]
fn retained_held_filer_commits_demand_and_accusation_only_after_destination_admission() {
    let w = world(0);
    let host = Host { w: &w, material: w.forged.clone(), chain: Default::default() };
    let claim = claim_of(&host.material, &w);
    let mut book = PalwGdnHeldBookV1::default();
    let mut rejected_demand = None;
    palw_gdn_held_filer_emit_v2(&host, &mut book, 100, |_, object| {
        assert!(matches!(&**object, PalwConsensusObjectV2::DefaultAccusedHeld { .. }));
        rejected_demand = Some(borsh::to_vec(&**object).unwrap());
        Err(refused())
    });
    assert!(rejected_demand.is_some());
    assert_eq!(book.cases[&claim.claim].demanded, None);
    assert!(!book.settled.contains_key(&claim.claim));
    let mut rejected_accusation = None;
    for now in 101..400 {
        palw_gdn_held_filer_emit_v2(&host, &mut book, now, |_, object| {
            if let PalwConsensusObjectV2::DefaultAccusedHeld { accusation } = &**object {
                if now == 101 {
                    assert_eq!(Some(borsh::to_vec(&**object).unwrap()), rejected_demand);
                }
                host.chain.borrow_mut().push(producer_answer(&w, &claim, accusation.missing));
                Ok(())
            } else {
                rejected_accusation = Some(borsh::to_vec(&**object).unwrap());
                Err(refused())
            }
        });
        if rejected_accusation.is_some() {
            break;
        }
    }
    assert!(rejected_accusation.is_some(), "actual answered checkpoint reaches the court accusation");
    assert!(!book.settled.contains_key(&claim.claim), "destination failure cannot mark an accusation published");
    let mut accepted: Option<NativeRetainedV1<PalwConsensusObjectV2>> = None;
    palw_gdn_held_filer_emit_v2(&host, &mut book, 400, |_, object| {
        assert_eq!(Some(borsh::to_vec(&**object).unwrap()), rejected_accusation);
        accepted = Some(NativeRetainedV1::try_from_clone_v1(&**object)?);
        Ok(())
    });
    assert_eq!(book.settled.get(&claim.claim), Some(&"accused"));
    let accepted = accepted.unwrap();
    let PalwConsensusObjectV2::MemoryRoutingV2 { object } = &*accepted else { panic!("an accusation") };
    let PalwMemoryRoutingObjectV2::Accused { accusation } = &**object else { panic!("one-move court") };
    let PalwMemoryRoutingEvidenceV2::GdnTransition { refutation } = &accusation.evidence else { panic!("transition court") };
    check_gdn_transition_refutation_v2(refutation, accusation.execution_root, w.shape_profile_id, &w.oracle, PALW_STEP_LEG_MAX_LEAVES)
        .unwrap();
}

#[test]
fn retained_held_action_quota_precedes_signer_and_owner_lives_until_drop() {
    const CHILD: &str = "MISAKA_HELD_GDN_SIGNED_ACTION_CHILD";
    let mode = std::env::var(CHILD).ok();
    if mode.is_none() {
        let name = format!("{}::retained_held_action_quota_precedes_signer_and_owner_lives_until_drop", module_path!());
        let name = name.split_once("::").unwrap().1;
        for mode in ["zero", "lifetime"] {
            assert!(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", name, "--nocapture", "--test-threads=1"])
                    .env(CHILD, mode)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        return;
    }
    let w = world(0);
    let claim = claim_of(&w.forged, &w);
    let kernels = || w.backend.gdn_recompute_kernels_v2().unwrap();
    let mut case = palw_gdn_held_open_v1(claim.clone(), &w.forged.view(), kernels).unwrap().unwrap();
    // Construct a real guilty terminal before quota arming; only the new signed
    // action's copies/retention are under test, not earlier refutation assembly.
    let (_, next, _) = run(&w, true);
    let PalwGdnHeldNextV1::Accuse { object } = next else { panic!("actual terminal accusation") };
    let PalwConsensusObjectV2::MemoryRoutingV2 { object } = object else { panic!("court object") };
    let PalwMemoryRoutingObjectV2::Accused { accusation } = *object else { panic!("court accusation") };
    let sign_count = std::cell::Cell::new(0);
    let signer = |m: &[u8], c: &[u8]| {
        sign_count.set(sign_count.get() + 1);
        sign(m, c)
    };
    let accuser = PalwGdnHeldAccuserV1 {
        bond: bond(2),
        network_domain: Hash64::from_u64_word(0xD0),
        openings: &w.openings,
        oracle: &w.oracle,
        sign: &signer,
    };
    use kaspa_consensus_core::palw_native_resource_v1::{
        NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
    };
    let ceiling = if mode.as_deref() == Some("zero") { 0 } else { 256 << 20 };
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
    if ceiling == 0 {
        assert!(matches!(
            palw_gdn_held_step_retained_v2(&mut case, &w.forged.view(), kernels, &[], 100, 20, &accuser),
            Err(PalwGdnHeldErrorV2::Resource(_))
        ));
        assert_eq!(case.demanded, None);
        assert!(matches!(
            super::super::action_owner::accusation(&claim, accusation.evidence, &accuser),
            Err(PalwGdnHeldErrorV2::Resource(_))
        ));
        assert_eq!(sign_count.get(), 0, "both output admission refusals precede their signer");
    } else {
        let demand = palw_gdn_held_step_retained_v2(&mut case, &w.forged.view(), kernels, &[], 100, 20, &accuser).unwrap();
        let PalwGdnHeldNextRetainedV2::Demand { object: demand, .. } = demand else { panic!("retained demand") };
        let accusation = super::super::action_owner::accusation(&claim, accusation.evidence, &accuser).unwrap();
        assert_eq!(sign_count.get(), 2);
        assert!(reserve_native_requests_v1(ceiling).is_err());
        drop(demand);
        assert!(reserve_native_requests_v1(ceiling).is_err(), "accusation has an independent retained ticket");
        drop(accusation);
        assert!(reserve_native_requests_v1(ceiling).is_ok(), "output admission is released only after the final owner drops");
    }
}
