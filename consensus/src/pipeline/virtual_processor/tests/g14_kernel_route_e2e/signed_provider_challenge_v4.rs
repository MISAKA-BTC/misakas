//! Actual same-claim private Lock and authenticated malformed-unit prosecution.
use super::*;
#[path = "../../../../../tests/fixtures/signed_provider_config_v4.rs"]
mod common;
pub(super) use common::config;
pub(super) fn enabled() -> bool {
    std::env::var("MISAKA_SIGNED_PROVIDER_NATIVE_V4").as_deref() == Ok("1")
}
#[test]
#[ignore = "requires explicitly pinned matching kaspad test executable; run with --ignored"]
fn rfc07_signed_socket_unit_failure_paid_checker_native_funded_challenge() {
    let bin = std::env::var_os("MISAKA_SIGNED_PROVIDER_CHECKER_BIN_V4").expect("explicit matching kaspad test binary required");
    assert!(std::path::Path::new(&bin).is_file());
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &child_name("typed_writer_child"), "--nocapture", "--test-threads=1"])
        .env("MISAKA_TYPED_WRITER_CHILD_V4", "1")
        .env("MISAKA_SIGNED_PROVIDER_NATIVE_V4", "1")
        .status()
        .unwrap();
    assert!(status.success(), "same-claim native writer failed: {status}");
}
pub(super) async fn paid_job(w: &mut World, job: &KernelJobV1) -> (Digest, Digest) {
    use misaka_palw_kernel::claim_paid_v2::PaidClaimObjectV2 as P;
    let route = w.net.api().unwrap();
    let object = misaka_palw_sdk::claim_paid_v2::paid_claim_class_object_v2(
        &route,
        &w.net.config.params.palw_claim_challenge_v2.unwrap().policy,
        w.net.kid(1),
        w.class,
        // Eight slots leave room for the actual separately carried Fund below.
        misaka_palw_challenge::paid_opt_in_source_policy_v5(2, 8, 20, 2, 1),
        &w.net.config.params.palw_beacon_source_opt_in_v1.unwrap().policy,
    )
    .unwrap();
    let K::PaidClaimV2 { object: P::RegisterClass { class } } = &object else { unreachable!() };
    let class = class.id();
    let object = w.net.route(1, &object);
    w.net.send(vec![(1, object)]).await;
    let object = misaka_palw_sdk::claim_paid_v2::paid_claim_job_object_v2(
        &w.net.api().unwrap(),
        class,
        job.id(),
        w.net.kid(1),
        [0xE6; 64],
        w.net.daa() + 200,
        1,
    )
    .unwrap();
    let K::PaidClaimV2 { object: P::EnrollJob { job } } = &object else { unreachable!() };
    let job = job.id();
    let object = w.net.route(1, &object);
    w.net.send(vec![(1, object)]).await;
    (class, job)
}
pub(super) fn paid_commit(
    w: &World,
    class: Digest,
    job: Digest,
    commit: &misaka_palw_kernel::route::SaltedCommitV1,
    salt: Digest,
    consent: Hash64,
) -> K {
    misaka_palw_sdk::claim_paid_v2::paid_claim_commit_object_derived_v2(
        &w.net.api().unwrap(),
        class,
        job,
        commit.clone(),
        salt,
        consent.as_bytes(),
        w.net.daa(),
    )
    .unwrap()
}
pub(super) async fn finish(
    mut w: World,
    db: std::sync::Arc<kaspa_database::prelude::DB>,
    dir: &std::path::Path,
    committing: &K,
    material: &PaidClaimMaterialWireV2,
    commit: misaka_palw_kernel::route::SaltedCommitV1,
    root: Hash64,
    consent: Hash64,
) {
    use kaspa_consensus_core::{
        palw_claim_challenge_v2::{PAID_CLAIM_LOCKS_V2, PalwPaidClaimLockV2},
        palw_monetary_material_v1::PalwMonetaryMaterialActionV1 as M,
    };
    use kaspa_database::prelude::ConnBuilder;
    use misaka_palw_kernel::claim_paid_v2::PaidClaimObjectV2 as P;
    let K::PaidClaimV2 { object: P::Commit { enrollment, .. } } = committing else { panic!("actual same-claim paid commit") };
    let enrollment = enrollment.id();
    assert!(!w.net.api().unwrap().paid_claim_approved_final_exists_v2());
    let funding = {
        use kaspa_consensus_core::palw_claim_challenge_v2::{PAID_CLAIM_ENROLLMENTS_V2, PalwPaidClaimAuthorityV2};
        let route = w.net.api().unwrap();
        let authority = route
            .aux
            .iter()
            .filter(|((table, key), _)| *table == PAID_CLAIM_ENROLLMENTS_V2 && key.len() == 64)
            .filter_map(|(_, bytes)| borsh::from_slice::<PalwPaidClaimAuthorityV2>(bytes).ok())
            .find(|row| row.enrollment.id() == enrollment)
            .expect("actual prospective paid enrollment");
        let policy = &w.net.config.params.palw_beacon_source_fee_v1.as_ref().unwrap().policy;
        let fee = policy.minimum_fee_for(&authority.source_target).unwrap();
        let count = policy.max_funded_sources.min(authority.source_target.terms.max_total);
        misaka_palw_sdk::claim_paid_v2::paid_claim_funding_object_v2(&route, enrollment, w.net.kid(1), fee, count, policy)
            .expect("explicit payer funds the actual future source before its seal window")
    };
    let funding = w.net.route(1, &funding);
    w.net.send(vec![(1, funding)]).await;
    assert!(w.net.api().unwrap().paid_claim_enrollment_v3(&enrollment).is_ok(), "native Fund froze source terms");
    let object = w.net.route(1, &K::PaidClaimV2 { object: P::Lock { enrollment } });
    w.net.send(vec![(1, object)]).await;
    let lock: PalwPaidClaimLockV2 =
        w.net.api().unwrap().aux_row(PAID_CLAIM_LOCKS_V2, &borsh::to_vec(&enrollment).unwrap()).expect("native Lock");
    assert_eq!(lock.anchor, misaka_palw_kernel::claim_paid_v2::paid_claim_bootstrap_anchor_v2());
    let provider = w.net.bond(2);
    let checker = *lock.seats.iter().find(|b| **b != provider).expect("distinct drawn checker");
    let checker_card = (0..8).find(|c| w.net.bond(*c) == checker).unwrap();
    // Make the checker's ordinary fee float a non-genesis, accepted change output.
    let mut extra = w.net.ledger().jobs.values().next().unwrap().clone();
    extra.nonce = [0xE9; 64];
    let object = w.net.route(checker_card, &K::PostJob { job: extra });
    w.net.send(vec![(checker_card, object)]).await;
    let route = w.net.api().unwrap();
    let ledger = route.ledger().unwrap();
    let parameters = material.params.iter().map(|((_, p, l), v)| ((*p, *l), v.clone())).collect();
    let misaka_palw_kernel::route::SaltedCommitV1::Claim { claim, .. } = &commit else { panic!("Program claim") };
    let job = ledger.jobs.get(&claim.job_id).expect("actual committed job");
    let resource_bound = misaka_palw_sdk::claim_paid_v2::producer_resources::bound_program_work_v3(
        &ledger.classes[&job.class_binding_id],
        job,
        &parameters,
    )
    .unwrap();
    let work = misaka_palw_sdk::claim_paid_v2::private_bootstrap::ProgramPaidBootstrapWorkV3 {
        version: 3,
        base_job: job.id(),
        commit,
        material: material.clone(),
        resource_bound,
    };
    std::fs::write(dir.join("checker-work"), borsh::to_vec(&work).unwrap()).unwrap();
    let input = common::CheckerHandoff {
        writer_pid: std::process::id(),
        checkpoint: w.net.chain.tip_state().0,
        daa: w.net.chain.ctx.consensus.get_virtual_daa_score(),
        root,
        enrollment: Hash64::from_bytes(enrollment),
        consent,
        checker,
        provider,
        checker_card: checker_card as u32,
        provider_card: 2,
        funding: w.net.funding[checker_card].0,
    };
    std::fs::write(dir.join("checker-handoff"), borsh::to_vec(&input).unwrap()).unwrap();
    let World { net, fx, class, jobs } = w;
    let Net { chain, config, bundle, premine, floats, funding, domain, rnd, mut _keep } = net;
    let simulated_time = chain.ctx.simulated_time;
    let nonce = chain.nonce_for_reopen();
    let db_path = db.path().to_path_buf();
    drop(ledger);
    drop(route);
    let weak = std::sync::Arc::downgrade(&db);
    drop(chain);
    drop(db);
    assert_eq!(weak.strong_count(), 0);
    let status = std::process::Command::new(std::env::var_os("MISAKA_SIGNED_PROVIDER_CHECKER_BIN_V4").unwrap())
        .args([
            "--exact",
            "private_integration::kernel_paid_checker_v4::signed_socket_native::checker_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("MISAKA_SIGNED_PROVIDER_HANDOFF_V4", dir)
        .env("MISAKA_SIGNED_PROVIDER_DB_V4", &db_path)
        .status()
        .unwrap();
    assert!(status.success(), "production checker child failed: {status}");
    let db = ConnBuilder::default().with_files_limit(10).with_db_path(db_path).with_create_if_missing(false).build().unwrap();
    let mut reopen = config.clone();
    reopen.process_genesis = false;
    let (tx, rx) = async_channel::unbounded();
    let chain = t12_reopened_chain(TestConsensus::with_db(db.clone(), &reopen, tx), &config, &bundle, simulated_time, nonce);
    _keep.push(Box::new(rx));
    w = World { net: Net { chain, config, bundle, premine, floats, funding, domain, rnd, _keep }, fx, class, jobs };
    assert_eq!(w.net.chain.tip_state().0, input.checkpoint);
    let object: Obj = borsh::from_slice(&std::fs::read(dir.join("checker-object")).unwrap()).unwrap();
    let tx: Transaction = borsh::from_slice(&std::fs::read(dir.join("checker-transaction")).unwrap()).unwrap();
    let Obj::MonetaryMaterialActionV1 { action, signer, .. } = &object else { panic!("real checker signed monetary action") };
    let M::ChallengeCurrentV4 { root: ar, consent: ac, provider: ap, dependency, .. } = &**action else {
        panic!("availability obligation, not conviction/receipt")
    };
    assert_eq!((*ar, *ac, *ap, *dependency, *signer), (root, consent, provider, root, checker));
    assert_eq!(tx.inputs.len(), 1);
    assert_eq!(tx.inputs[0].previous_outpoint, input.funding);
    let funding_entry = w.net.chain.ctx.consensus.get_virtual_utxo_entry(input.funding).expect("real checker funding");
    assert_eq!(tx.outputs.len(), 1);
    assert_eq!(tx.outputs[0].script_public_key, funding_entry.script_public_key);
    let fee = funding_entry.amount.checked_sub(tx.outputs[0].value).expect("authorized change never mints money");
    assert!(fee <= 100_000_000, "same explicit checker fee cap");
    assert!(
        kaspa_consensus_core::palw_material_challenge_checkpoint_v4::material_challenge_checkpoint_from_route_v4(
            &w.net.api().unwrap(),
            checker,
        )
        .unwrap()
        .is_none(),
        "rehearsal and WAL publication did not apply the obligation"
    );
    assert!(matches!(
        w.net.chain.ctx.consensus.palw_object_rehearsal_v1(&object),
        Some(kaspa_consensus_core::palw_producer_v2::PalwObjectRehearsalV1::Accepted)
    ));
    w.net.mempool(&tx).expect("exact paid-checker transaction admitted");
    let t = w.net.ttpb();
    let block = w.net.chain.heartbeat(t, vec![tx.clone()]).await;
    assert!(block.transactions.iter().any(|v| v.id() == tx.id()));
    w.net.chain.heartbeat(t, vec![]).await;
    assert!(w.net.chain.ctx.consensus.get_virtual_utxo_entry(input.funding).is_none());
    let change = TransactionOutpoint::new(tx.id(), 0);
    let change_entry = w.net.chain.ctx.consensus.get_virtual_utxo_entry(change).expect("actual accepted checker change");
    assert_eq!(change_entry.amount, tx.outputs[0].value);
    assert_eq!(change_entry.script_public_key, funding_entry.script_public_key);
    let route = w.net.api().unwrap();
    let checkpoint =
        kaspa_consensus_core::palw_material_challenge_checkpoint_v4::material_challenge_checkpoint_from_route_v4(&route, checker)
            .unwrap()
            .unwrap();
    assert!(checkpoint.matches(domain, checker, action).unwrap());
    assert!(!route.paid_claim_approved_final_exists_v2());
    eprintln!(
        "SIGNED_UNIT_NATIVE_V4 claim={root} enrollment={} provider={provider:?} consent={consent} checker={checker:?} tx={} accepted_checkpoint={}",
        input.enrollment,
        tx.id(),
        w.net.chain.tip_state().0
    );
    drop(route);
    let accepted = w.net.chain.tip_state().0;
    let time = w.net.chain.ctx.simulated_time;
    let nonce = w.net.chain.nonce_for_reopen();
    let config = w.net.config.clone();
    let bundle = w.net.bundle.clone();
    let path = db.path().to_path_buf();
    let weak = std::sync::Arc::downgrade(&db);
    drop(w);
    drop(db);
    assert_eq!(weak.strong_count(), 0, "accepted writer fully closed before cold reopen");
    let db = ConnBuilder::default().with_files_limit(10).with_db_path(path).with_create_if_missing(false).build().unwrap();
    let mut reopen = config.clone();
    reopen.process_genesis = false;
    let (notify, keep) = async_channel::unbounded();
    let chain = t12_reopened_chain(TestConsensus::with_db(db.clone(), &reopen, notify), &config, &bundle, time, nonce);
    assert_eq!(chain.tip_state().0, accepted);
    assert!(chain.ctx.consensus.get_virtual_utxo_entry(input.funding).is_none());
    assert_eq!(chain.ctx.consensus.get_virtual_utxo_entry(change).unwrap().amount, tx.outputs[0].value);
    let route = chain.ctx.consensus.palw_kernel_route_v1().expect("cold native route");
    let retained =
        kaspa_consensus_core::palw_material_challenge_checkpoint_v4::material_challenge_checkpoint_from_route_v4(&route, checker)
            .unwrap()
            .expect("cold accepted obligation");
    assert_eq!(retained, checkpoint);
    assert!(retained.matches(domain, checker, action).unwrap());
    assert!(!route.paid_claim_approved_final_exists_v2());
    eprintln!("SIGNED_UNIT_NATIVE_V4 cold_checkpoint={accepted} exact_fee={fee} obligation_sequence={}", retained.sequence);
    drop(route);
    drop(chain);
    drop(keep);
    drop(db);
}
