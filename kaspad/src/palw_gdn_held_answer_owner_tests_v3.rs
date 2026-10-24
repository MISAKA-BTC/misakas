//! Real producer disclosure -> retained case answers -> complete refutation
//! input batch. Quota controls do not claim earlier authentication/workspace.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
};
fn opened(w: &World) -> PalwGdnHeldCaseV1 {
    palw_gdn_held_open_v1(claim_of(&w.forged, w), &w.forged.view(), || w.backend.gdn_recompute_kernels_v2().unwrap()).unwrap().unwrap()
}
fn chain(w: &World, case: &PalwGdnHeldCaseV1) -> Vec<PalwConsensusObjectV2> {
    case.demands.iter().map(|unit| producer_answer(w, &case.claim, *unit)).collect()
}

#[test]
fn real_held_disclosure_reader_skips_foreign_and_invalid_units_and_retains_authenticated_answer() {
    let w = world(0);
    let case = opened(&w);
    let unit = case.demands[0];
    let good = producer_answer(&w, &case.claim, unit);
    let mut foreign = good.clone();
    let PalwConsensusObjectV2::MaterialDisclosedHeld { disclosure } = &mut foreign else { panic!("held carriage") };
    disclosure.claim = Hash64::from_u64_word(0xBAD);
    let mut invalid = good.clone();
    let PalwConsensusObjectV2::MaterialDisclosedHeld { disclosure } = &mut invalid else { panic!("held carriage") };
    let PalwHeldDisclosureV1::StateChunk { chunk, .. } = &mut disclosure.disclosure else { panic!("state chunk") };
    chunk.chunk_bytes[0] ^= 1;
    let read = |objects: &[PalwConsensusObjectV2]| {
        answer_on_chain_v1(objects, &case.claim, &w.forged.binding, &unit, w.forged.max_leaves, w.forged.prompt_ids_form)
    };
    assert!(read(&[foreign.clone(), invalid.clone()]).unwrap().is_none());
    let owned = read(&[foreign, invalid, good.clone()]).unwrap().unwrap();
    let PalwConsensusObjectV2::MaterialDisclosedHeld { disclosure } = good else { panic!("held carriage") };
    assert_eq!(*owned, disclosure.disclosure);
}

#[test]
fn actual_held_answer_capacity_refusal_retries_instead_of_default_and_owns_case_bytes() {
    const CHILD: &str = "MISAKA_HELD_GDN_ANSWER_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name = format!("{}::actual_held_answer_capacity_refusal_retries_instead_of_default_and_owns_case_bytes", module_path!());
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
    let w = world(0);
    let mut case = opened(&w);
    let objects = chain(&w, &case);
    case.demanded = Some((0, 100));
    let prior = (case.at, case.demanded);
    let calls = std::cell::Cell::new(0);
    let signer = |m: &[u8], c: &[u8]| {
        calls.set(calls.get() + 1);
        sign(m, c)
    };
    let accuser = PalwGdnHeldAccuserV1 {
        bond: bond(2),
        network_domain: Hash64::from_u64_word(0xD0),
        openings: &w.openings,
        oracle: &w.oracle,
        sign: &signer,
    };
    let ceiling = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(ceiling).unwrap();
    let kernels = || w.backend.gdn_recompute_kernels_v2().unwrap();
    assert!(
        matches!(
            palw_gdn_held_step_retained_v2(&mut case, &w.forged.view(), kernels, &objects, 10_000, 20, &accuser),
            Err(PalwGdnHeldErrorV2::Resource(_))
        ),
        "published valid answers with local copy refusal are not DefaultDue or honest"
    );
    assert_eq!(calls.get(), 0);
    assert_eq!((case.at, case.demanded), prior);
    assert!(case.answers.iter().all(Option::is_none));
    drop(blocker);
    let retry = palw_gdn_held_step_retained_v2(&mut case, &w.forged.view(), kernels, &objects, 10_000, 20, &accuser).unwrap();
    assert!(
        matches!(&retry, PalwGdnHeldNextRetainedV2::Accuse { .. }),
        "a fresh retry uses the answers even after the old demand timeout"
    );
    assert!(case.answers.iter().all(Option::is_some));
    assert_eq!(calls.get(), 1);
    drop(retry);
    assert!(
        reserve_native_requests_v1(ceiling).is_err(),
        "case answers retain independent native tickets after the signed action drops"
    );
    drop(case);
    assert!(reserve_native_requests_v1(ceiling).is_ok(), "case retirement releases the stored disclosures");
}

#[test]
fn complete_answer_batch_and_slot_reservations_precede_allocation_and_survive_copy_lifetime() {
    const CHILD: &str = "MISAKA_HELD_GDN_ANSWER_BATCH_CHILD";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name =
            format!("{}::complete_answer_batch_and_slot_reservations_precede_allocation_and_survive_copy_lifetime", module_path!());
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
    let w = world(0);
    let mut case = opened(&w);
    let objects = chain(&w, &case);
    note_answers_v1(&mut case, &w.forged.view(), &objects).unwrap();
    let before = borsh::to_vec(&case.answers).unwrap();
    let ceiling = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(ceiling).unwrap();
    assert!(matches!(super::super::answer_owner::complete_batch(&case.answers), Err(PalwGdnHeldErrorV2::Resource(_))));
    assert!(super::super::answer_owner::slots(case.demands.len()).is_err());
    assert_eq!(borsh::to_vec(&case.answers).unwrap(), before);
    drop(blocker);
    let batch = super::super::answer_owner::complete_batch(&case.answers).unwrap();
    for (copied, original) in batch.iter().zip(&case.answers) {
        assert_eq!(copied, &**original.as_ref().unwrap());
    }
    assert!(reserve_native_requests_v1(ceiling).is_err(), "independent complete answer copy retains its lease");
    drop(batch);
    let (slots, owner) = super::super::answer_owner::slots(case.demands.len()).unwrap();
    assert_eq!(slots.len(), case.demands.len());
    assert!(slots.iter().all(Option::is_none));
    assert!(reserve_native_requests_v1(ceiling).is_err(), "persistent Option slots have their own lifetime admission");
    drop(slots);
    drop(owner);
    assert!(reserve_native_requests_v1(ceiling).is_ok());
}
