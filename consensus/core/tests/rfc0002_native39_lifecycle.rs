//! **RFC-0002 native v39 — the owning39 lifecycle through the REAL fold** (`apply_palw_transition_v7`).
//!
//! Every block is folded by the production transition and checked three ways (the delta re-applies to the parent and reverts to
//! it, the child's carriage reloads under its committed root). Nothing here calls a `cfg(test)` seam of the fold: the native39
//! actions are carried as `KernelRouteV1` objects exactly as a block carries them (the signature is the processor's check; the
//! fold reads the signer), and every audit/redemption value is derived by the production audit factories from proofs of the
//! folded state.
//!
//! **Arming.** Native39 is armed only through the EXISTING isolated calibration/test-genesis exception
//! (`local_validation_snapshot_v34` → `NativeCalibrationSnapshotV39::wrap` → `load_isolated_native_calibration_v39`): a distinct
//! devnet genesis whose params pass `validate_palw_v2`. No shipped preset is touched (`shipped_presets_have_pinned_fingerprints`
//! pins them). The Final environment's calibration record for the ordinary court profile is a FIXTURE INPUT of this isolated
//! genesis (`fixture_calibration`), not a measurement: no value here is reported as measured, and the snapshot purpose is
//! `Measurement`, which can never be A6/A8 or production evidence.
//!
//! **Processor mirror.** `kernel_route_extras` rebuilds `palw_kernel_route_extras_at` (consensus/src/.../processor/palw.rs) from
//! the same `Params` fields; the processor-only test seams (`attested_artifacts`, `test_admitted_classes`, `test_eligible`,
//! `max_adjudications_per_block`) are left empty / `None`, as in a non-test build.
//!
//! **No cryptographic proof of computation** (PRINCIPLES §8): the hashes, Merkle roots and signatures here are data identity
//! only; the Final is the escrow fold's independent-conditions Final, never a proof that the LLM computation is correct.
//!
//! Run: cargo test -p kaspa-consensus-core --test rfc0002_native39_lifecycle -- --nocapture

#[path = "native39_fixture/mod.rs"]
mod fx;
use fx::*;

// ---------------------------------------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------------------------------------

#[test]
fn rfc0002_native39_isolated_params_validate_and_shipped_presets_stay_dormant() {
    let p = native39_params();
    assert!(p.palw_native_execution_active_at_v39(0), "native39 is armed from the isolated genesis");
    assert!(p.palw_native_mining_v39.is_none(), "mining39 is not armed by the calibration path");
    assert!(p.has_isolated_native_calibration_v34());
    assert_ne!(p.genesis.hash, kaspa_consensus_core::config::params::devnet_shipped_params().genesis.hash, "a distinct genesis");
    for (name, shipped) in [
        ("devnet", kaspa_consensus_core::config::params::devnet_shipped_params()),
        ("t12-launch", kaspa_consensus_core::config::params::palw_t12_launch_params_v1()),
    ] {
        assert!(shipped.palw_native_execution_v39.is_none(), "{name}: native39 stays None");
        assert!(shipped.palw_native_mining_v39.is_none(), "{name}: mining39 stays None");
    }
}

/// FINDING (not fixed: a policy choice): the LOCAL validation preset copies the probe horizons, under which no native39 Fund is
/// admissible — the paged carriage's 88-packet floor in a 3-DAA window is ~60x the 2-DAA-period reserved service.
#[test]
fn rfc0002_native39_probe_horizons_admit_no_fund() {
    let mut c = N39::on(native39_params_with(false));
    let (a, _) = actors(&c.p);
    let source = fixture_source(32);
    register_source_and_class(&mut c, a.producer, &source, 2);
    let job = prepare_job(&c, &a, &source, 2, 0, 50, c.daa + 3);
    let r = job.contract.carriage().record();
    assert_eq!((r.answer_packets, r.answer_window), (88, 3), "the carriage floor and the probe's answer window");
    c.must("consent 43", &[stage(43, borsh::to_vec(&job.consent).unwrap(), a.producer)]);
    let ids: Vec<Digest> = borsh::from_slice(c.aux(TABLE, &key(19, &a.producer)).unwrap()).unwrap();
    let e = c.step(&[stage(44, borsh::to_vec(&NativeFundWireV39 { version: 39, consent: ids[0] }).unwrap(), a.requester)]).unwrap_err();
    eprintln!("[native39] probe horizons: Fund refused: {e}");
    assert!(e.contains("exceed shared physical service"), "{e}");
}

#[test]
fn rfc0002_native39_job_audit_payout_status_and_redemption_through_the_real_fold() {
    let mut c = N39::new();
    let bonds = genesis_bonds(&c.p);
    let pairs = vec![
        Actors { producer: bonds[0].0, requester: bonds[1].0, finalizer: bonds[2].0 },
        Actors { producer: bonds[3].0, requester: bonds[4].0, finalizer: bonds[2].0 },
    ];
    let source = fixture_source(32);
    let reward = 50;
    let life = run_jobs(&mut c, pairs, &source, reward);
    let (final_daa, s_final) = (life.after_final.0, life.after_final.1.clone());
    // Every single credit is written once and pending in the Final block's queue.
    for r in &life.runs {
        assert!(s_final.kernel_route().unwrap().aux.contains_key(&(TABLE, key(23, &r.id))), "credit row 23");
    }
    assert_eq!(s_final.pending_payouts_iter().count(), life.runs.len(), "one pending row per credit");
    // A second Final on a settled job is refused (the escrow settles once).
    assert!(c.try_fold(c.daa + 1, &[stage(51, final_body(&life.runs[0].id, 0), life.runs[0].a.finalizer)]).is_err(), "re-Final refused");
    // The paying block P: the fold drains the queue prefix (step 1b) at the next block.
    let paying_daa = final_daa + 1;
    c.must("paying block", &[]);
    let s_paid = c.s.clone();
    assert_eq!(s_paid.pending_payouts_iter().count(), 0, "the paying block drained every pending credit");

    // Headers: F (Final block) → P (paying block, commits S_final, carries the coinbase) → Q (commits S_paid).
    let p = c.p.clone();
    let (fund_daa, s_fund) = (life.after_fund.0, life.after_fund.1.clone());
    let (commit_daa, s_commit) = (life.after_commit.0, life.after_commit.1.clone());
    let h_fund = header(&p, fund_daa, None, None, &[]);
    let h_commit_child = header(&p, fund_daa + 1, Some(&h_fund), Some(&s_fund), &[]);
    let h_commit = header(&p, commit_daa, None, None, &[]);
    let h_after_commit = header(&p, commit_daa + 1, Some(&h_commit), Some(&s_commit), &[]);
    let h_final = header(&p, final_daa, None, None, &[]);
    let drain = drain_prefix(&s_final);
    let leading = [(1_000u64, Hash64::from_u64_word(0x5EED))];
    let cb = coinbase(paying_daa, &leading, &drain);
    let txs = vec![cb.clone()];
    let h_paying = header(&p, paying_daa, Some(&h_final), Some(&s_final), &txs);
    let h_after = header(&p, paying_daa + 1, Some(&h_paying), Some(&s_paid), &[]);

    let (ra, rb) = (&life.runs[0], &life.runs[1]);
    // ---- the job audit factory re-derives everything from the authenticated stream ----
    let fin = audit(&p, &h_paying, &h_final, &s_final, ra.id, None).expect("the Final-state job audit");
    assert_eq!(fin.terminal(), kaspa_consensus_core::palw_native_job_v39::NativeJobTerminalV39::Final);
    assert_eq!(fin.original().reward, reward);
    assert_eq!(fin.original().consent, ra.consent, "the original consent");
    assert_eq!(fin.original().producer, ra.a.producer);
    assert_eq!(fin.original().requester, ra.a.requester);
    assert_eq!(fin.maximum().root(), ra.job.maximum.root(), "the original maximum");
    assert_eq!(fin.original().maximum_contract, ra.job.contract.record().root(), "the complete maximum contract");
    assert_eq!(fin.claim().map(|c| c.claim), Some(ra.commit.claim.root()), "the committed claim");
    let st = fin.settlement().expect("the settlement");
    assert_eq!((st.producer_credit, st.requester_credit, st.actual_slash), (reward, 0, 0), "full producer credit, nothing slashed");
    let credit = fin.credit().expect("the single credit").clone();
    assert_eq!((credit.amount, credit.job, credit.established), (reward, ra.id, final_daa));
    assert_eq!(credit.recipient, ra.a.producer);
    assert_eq!(s_final.pending_payout(&credit.queue).map(|r| (r.amount, r.payload)), Some((reward, credit.payout)), "the queue row");
    assert_eq!((fin.master(), fin.stage()), ([0; 64], None), "a standalone job has no Master binding");
    eprintln!("[native39] audit: job {} Final, credit {} sompi to the producer, queue row pending", hex8(&ra.id), credit.amount);

    // A tampered or truncated stream, or a stream of another state under this anchor, is refused.
    let book_key = borsh::to_vec(&(TABLE, key(17, &ra.id))).unwrap();
    assert!(stream(&s_final, |rows| rows.iter_mut().find(|r| r.0 == book_key).unwrap().1[0] ^= 1).is_err(), "a tampered book row");
    assert!(stream(&s_final, |rows| rows.retain(|r| r.0 != book_key)).is_err(), "a withheld book row");
    assert!(stream(&s_final, |rows| rows.swap(0, 1)).is_err(), "reordered rows");
    {
        let other = stream(&s_paid, |_| {}).unwrap();
        let a = anchor(&p, &h_paying, &h_final, &s_final).unwrap();
        assert!(PalwChainStateV2::native_job_audit_from_stream_v39(&other, &a, &p, ra.id, None).is_err(), "stream/anchor mismatch");
    }
    assert!(anchor(&p, &h_paying, &h_final, &s_paid).is_err(), "a header commits one state only");

    // ---- payout status: NotSettled → Pending → ConsumedDeliveryWitnessRequired ----
    let status_at = |child: &Header, parent: &Header, s: &PalwChainStateV2| {
        let job = audit(&p, child, parent, s, ra.id, None).expect("job audit");
        let proof = prove_native_payouts_v32(s).unwrap();
        let rows = verify_native_payout_rows_v32(&proof, PalwStateOpeningV1::of(s).root()).unwrap();
        native_payout_status_v39(&job, &rows).unwrap()
    };
    let s1 = status_at(&h_after_commit, &h_commit, &s_commit);
    let s2 = status_at(&h_paying, &h_final, &s_final);
    let s3 = status_at(&h_after, &h_paying, &s_paid);
    eprintln!("[native39] payout status: committed {s1:?} → Final {s2:?} → paid {s3:?}");
    assert_eq!(s1, NativePayoutStatusV39::NotSettled);
    assert!(matches!(s2, NativePayoutStatusV39::Pending { amount, .. } if amount == reward));
    assert_eq!(s3, NativePayoutStatusV39::ConsumedDeliveryWitnessRequired, "absence is never read as delivery");
    let _ = h_commit_child; // the Fund-state anchor is exercised by the status of a funded job below
    assert_eq!(status_at(&h_commit_child, &h_fund, &s_fund), NativePayoutStatusV39::NotSettled);

    // ---- redemption: only with the bracketing witness of the paying block ----
    let proof = prove_native_coinbase_v33(&h_paying, h_paying.hash, &p, txs.iter()).unwrap();
    let paid = verify_native_coinbase_v33(&proof, &h_paying, h_paying.hash, &p).unwrap();
    let before_proof = prove_native_payouts_v32(&s_final).unwrap();
    let before = verify_native_payout_rows_v32(&before_proof, PalwStateOpeningV1::of(&s_final).root()).unwrap();
    let after_proof = prove_native_payouts_v32(&s_paid).unwrap();
    let after = verify_native_payout_rows_v32(&after_proof, PalwStateOpeningV1::of(&s_paid).root()).unwrap();
    let rank_a = s_final.pending_payouts_iter().position(|(k, _)| *k == credit.queue).unwrap();
    let index_a = (leading.len() + rank_a) as u32;
    let red = derive_native_redemption_v39(&fin, &fin, &before, &after, &paid, index_a).expect("the bracketed delivery redeems");
    assert_eq!((red.amount(), red.job(), red.credit()), (reward, ra.id, credit.root()));
    assert_eq!(red.outpoint(), kaspa_consensus_core::tx::TransactionOutpoint::new(cb.id(), index_a));
    assert_eq!((red.before_state(), red.after_state()), (before.state_root(), after.state_root()));
    eprintln!("[native39] redemption: outpoint {:?} amount {}", red.outpoint(), red.amount());
    // The second job's credit redeems from the same coinbase at its own rank.
    let fin_b = audit(&p, &h_paying, &h_final, &s_final, rb.id, None).unwrap();
    let rank_b = s_final.pending_payouts_iter().position(|(k, _)| *k == fin_b.credit().unwrap().queue).unwrap();
    assert_ne!(rank_a, rank_b);
    assert!(derive_native_redemption_v39(&fin_b, &fin_b, &before, &after, &paid, (leading.len() + rank_b) as u32).is_ok());

    // Negatives.
    // (1) Wrong witness: the after state (credit already consumed) as the "before" state.
    let after_audit = audit(&p, &h_after, &h_paying, &s_paid, ra.id, None).unwrap();
    assert!(derive_native_redemption_v39(&fin, &after_audit, &after, &after, &paid, index_a).is_err(), "post-payment state as witness");
    // (1b) A witness from before the credit existed.
    let committed = audit(&p, &h_after_commit, &h_commit, &s_commit, ra.id, None).unwrap();
    let cp = prove_native_payouts_v32(&s_commit).unwrap();
    let crows = verify_native_payout_rows_v32(&cp, PalwStateOpeningV1::of(&s_commit).root()).unwrap();
    assert!(derive_native_redemption_v39(&fin, &committed, &crows, &after, &paid, index_a).is_err(), "pre-Final witness");
    // (1c) Another job's audit as the witness.
    assert!(derive_native_redemption_v39(&fin, &fin_b, &before, &after, &paid, index_a).is_err(), "another job's credit");
    // (2) Not consumed: the after-state still holds the row.
    assert!(derive_native_redemption_v39(&fin, &fin, &before, &before, &paid, index_a).is_err(), "credit not consumed");
    // (3) A reordered drain prefix.
    let mut swapped = drain.clone();
    swapped.swap(0, 1);
    let cb_swapped = coinbase(paying_daa, &leading, &swapped);
    let h_sw = header(&p, paying_daa, Some(&h_final), Some(&s_final), std::slice::from_ref(&cb_swapped));
    let pr_sw = prove_native_coinbase_v33(&h_sw, h_sw.hash, &p, std::iter::once(&cb_swapped)).unwrap();
    let paid_sw = verify_native_coinbase_v33(&pr_sw, &h_sw, h_sw.hash, &p).unwrap();
    assert!(derive_native_redemption_v39(&fin, &fin, &before, &after, &paid_sw, index_a).is_err(), "reordered drain prefix");
    // (4) A wrong script, and (5) a wrong value, at the credit's own output.
    for (what, out) in [
        ("script", (drain[rank_a].0, Hash64::from_u64_word(0xBAD))),
        ("value", (drain[rank_a].0 + 1, drain[rank_a].1)),
    ] {
        let mut d = drain.clone();
        d[rank_a] = out;
        let cbx = coinbase(paying_daa, &leading, &d);
        let hx = header(&p, paying_daa, Some(&h_final), Some(&s_final), std::slice::from_ref(&cbx));
        let px = prove_native_coinbase_v33(&hx, hx.hash, &p, std::iter::once(&cbx)).unwrap();
        let vx = verify_native_coinbase_v33(&px, &hx, hx.hash, &p).unwrap();
        assert!(derive_native_redemption_v39(&fin, &fin, &before, &after, &vx, index_a).is_err(), "wrong {what}");
    }
    // (6) A wrong output index (the prefix starts elsewhere) and an output outside the coinbase.
    assert!(derive_native_redemption_v39(&fin, &fin, &before, &after, &paid, index_a + 7).is_err(), "wrong output index");
    // (7) A coinbase not in the pinned block.
    assert!(verify_native_coinbase_v33(&proof, &h_sw, h_sw.hash, &p).is_err(), "the coinbase leaf binds its block");
    // (8) Zero amount: a zero reward never reaches a credit — the producer's consent is refused by the fold.
    let zero = prepare_job(&c, &Actors { producer: bonds[5].0, requester: bonds[6].0, finalizer: bonds[2].0 }, &source, 2, 0, 0, c.daa + 3);
    let e = c.step(&[stage(43, borsh::to_vec(&zero.consent).unwrap(), bonds[5].0)]).unwrap_err();
    assert!(e.contains("requester/max/reward"), "a zero-reward consent is refused: {e}");
}

/// Action 53 collection, the GC'd consent and the archive: after collection the audit needs the archived original consent, which
/// it authenticates against the immutable collection record; arbitrary bytes grant nothing.
#[test]
fn rfc0002_native39_collection_gc_and_archive_audit_through_the_real_fold() {
    let mut c = N39::new();
    let bonds = genesis_bonds(&c.p);
    let pairs = vec![Actors { producer: bonds[0].0, requester: bonds[1].0, finalizer: bonds[2].0 }];
    let source = fixture_source(32);
    let life = run_jobs(&mut c, pairs, &source, 50);
    let r = &life.runs[0];
    let p = c.p.clone();
    // Archive the original bodies while they are live.
    let route = c.s.kernel_route().unwrap();
    let mut archive = MemArchive::default();
    archive.consents.insert(r.id, route.aux.get(&(TABLE, key(27, &r.id))).expect("live consent row 27").clone());
    archive.sources.insert(source.root(), route.aux.get(&(TABLE, key(1, &source.root()))).expect("source body").clone());
    archive.classes.insert(life.class, route.aux.get(&(TABLE, key(3, &life.class))).expect("class body").clone());
    let h = c.aux(TABLE, &key(16, &r.id)).unwrap().clone();
    // Collection before the complete retention horizon is refused; past it, any Active bond collects once.
    let deadline = r.job.contract.record().temporal.total + life.after_fund.0;
    assert!(c.try_fold(c.daa + 1, &[stage(53, job_body(&r.id), bonds[2].0)]).is_err(), "collection inside the retention");
    // The quota subject also closes only once its admitted interval has expired.
    let next_interval = (life.after_fund.0 / TEST_INTERVAL + 1) * TEST_INTERVAL;
    assert!(c.try_fold(deadline + 1, &[stage(53, job_body(&r.id), bonds[2].0)]).is_err(), "collection inside the admitted interval");
    c.step_at(next_interval.max(deadline + 1), &[stage(53, job_body(&r.id), bonds[2].0)])
        .unwrap_or_else(|e| panic!("collect 53 refused: {e}"));
    let s_gc = c.s.clone();
    let gc_daa = c.daa;
    assert!(c.try_fold(c.daa + 1, &[stage(53, job_body(&r.id), bonds[2].0)]).is_err(), "a second collection is refused");
    let route = s_gc.kernel_route().unwrap();
    assert!(!route.aux.contains_key(&(TABLE, key(27, &r.id))), "the consent body row is collected");
    for kind in [16u8, 17, 23, 28] {
        assert!(route.aux.contains_key(&(TABLE, key(kind, &r.id))), "kind {kind} stays");
    }
    assert_eq!(c.aux(TABLE, &key(16, &r.id)).unwrap(), &h, "the immutable funded header is kept");
    // The audit at the collected state.
    let hp = header(&p, gc_daa, None, None, &[]);
    let hc = header(&p, gc_daa + 1, Some(&hp), Some(&s_gc), &[]);
    let without = audit(&p, &hc, &hp, &s_gc, r.id, None);
    eprintln!("[native39] after collection, audit without archive: {:?}", without.as_ref().err());
    assert!(without.is_err(), "a collected consent is a named failure without its archive, never success");
    let with = audit(&p, &hc, &hp, &s_gc, r.id, Some(&archive)).expect("the archived original authenticates");
    assert_eq!(with.credit().map(|c| c.amount), Some(50));
    assert_eq!(with.original().consent, r.consent);
    // Arbitrary bytes: a changed consent body, or another job's consent, grant no authority.
    let mut bad = archive.clone();
    bad.consents.get_mut(&r.id).unwrap()[10] ^= 0xFF;
    assert!(audit(&p, &hc, &hp, &s_gc, r.id, Some(&bad)).is_err(), "changed archived bytes");
    let mut bad = archive.clone();
    bad.consents.insert(r.id, vec![0u8; 512]);
    assert!(audit(&p, &hc, &hp, &s_gc, r.id, Some(&bad)).is_err(), "arbitrary archived bytes");
    let mut bad = archive.clone();
    let body = bad.classes.get_mut(&life.class).unwrap();
    body[3] ^= 1;
    let class_live = route.aux.contains_key(&(TABLE, key(3, &life.class)));
    let res = audit(&p, &hc, &hp, &s_gc, r.id, Some(&bad));
    eprintln!("[native39] class body live after collection: {class_live}; changed archived class → {:?}", res.as_ref().err());
    if !class_live {
        assert!(res.is_err(), "a changed archived class body");
    }
}

/// A Master through the fold: registration (65); its component composed by `compose_and_check_component_v39` and bound by the
/// first authentic Fund84; the independent Final; the aggregate (66 → 83); and the complete Master Final audit, which matches the
/// aggregate and the component's own job audit under ONE anchor. Single-stage: a multi-stage pipeline must route every earlier
/// stage's output into a later stage (`validate_pipeline_v39`), which needs the upstream material path (actions 61–64) —
/// listed as remaining in the report, not exercised here.
#[test]
fn rfc0002_native39_master_component_aggregate_and_master_final_audit_through_the_real_fold() {
    let mut c = N39::new();
    let bonds = genesis_bonds(&c.p);
    let (owner, producer, finalizer) = (bonds[1].0, bonds[0].0, bonds[2].0);
    let p = c.policies();
    // Two independent stages without an edge are not a valid pipeline (every stage must feed a later one or be the output).
    {
        let s = [fixture_source_v(32, 5), fixture_source_v(32, 6)];
        let programs: Vec<_> = s.iter().map(|s| s.program().clone()).collect();
        let stages = (0..2)
            .map(|i| StageDecl { name: format!("s{i}"), program: i, trip: TripRule::Fixed { n: 32 }, max_trip: 32, tokens: None, bind: vec![] })
            .collect();
        assert!(PipelineV39 { version: 39, stages, output_stage: 1 }.check(&programs).is_err(), "an orphan stage is refused");
    }
    let source = fixture_source_v(32, 5);
    c.must("source 41", &[stage(41, source.wire().to_bytes(), producer)]);
    let class = NativeClassWireV39::derive(&source, &p, 32).unwrap();
    c.must("class 42", &[stage(42, borsh::to_vec(&class).unwrap(), producer)]);
    let register_daa = c.daa + 1;
    {
        let pol = &c.s.kernel_route().unwrap().header.policy;
        eprintln!(
            "[native39-debug] route policy: max_adjudications {} max_court_work {} admission_limit {} reserve {}",
            pol.max_adjudications_per_block,
            pol.max_court_work_per_block,
            pol.admission_work_limit_v1(),
            pol.prosecution_reserve_permille
        );
    }
    let deadline = register_daa + TEST_HORIZON;
    let plan = plan_master(&c, owner, &[producer], std::slice::from_ref(&source), 40, 0, 0, deadline);
    // A deadline past horizon x stages is refused; a class registered short of its whole context cannot enter a Master.
    let late = plan_master(&c, owner, &[producer], std::slice::from_ref(&source), 40, 0, 0, deadline + 1);
    match c.try_fold(register_daa, &[stage(65, borsh::to_vec(&late.wire).unwrap(), owner)]) {
        Err(e) => eprintln!("[native39] late Master refused: {e}"),
        Ok((s, _, _)) => {
            let wrote = s.kernel_route().unwrap().aux.contains_key(&(TABLE, key(80, &late.id)));
            eprintln!("[native39-debug] late Master raw {} bytes", borsh::to_vec(&late.wire).unwrap().len());
            panic!("late Master not refused (record written: {wrote})");
        }
    }
    c.must("Master 65", &[stage(65, borsh::to_vec(&plan.wire).unwrap(), owner)]);
    assert!(c.aux(TABLE, &key(80, &plan.id)).is_some(), "the Master record 80");
    assert!(c.try_fold(c.daa + 1, &[stage(65, borsh::to_vec(&plan.wire).unwrap(), owner)]).is_err(), "Master replay refused");
    // Closing before the component is Final is refused while the Master deadline has not expired.
    assert!(c.try_fold(c.daa + 1, &[stage(66, plan.id.to_vec(), bonds[4].0)]).is_err(), "incomplete Master cannot close early");

    let s0 = run_component(&mut c, &plan, 0, &[], finalizer, false);
    // The Fund84 binding is written at the first authentic Fund and names the exact original job.
    let (m84, f84): (Digest, kaspa_consensus_core::palw_native_master_v39::NativeMasterFundedComponentV39) = row_of(&c.s, 84, &s0.job);
    assert_eq!((m84, f84.stage, f84.job, f84.admitted), (plan.id, 0, s0.job, s0.funded_daa));
    assert_eq!(s0.maximum.input_manifest().master, plan.id, "the composed manifest names its Master");
    // A second job for the already bound stage is refused (the binding is never replaced).
    {
        let (m0, _) = compose_and_check_component_v39(plan.id, &plan.request, &plan.pipeline, &source, 0, &[], &[]).unwrap();
        let contract = contract_for(&c, &source, &plan.classes[0], &m0);
        let consent = NativeConsentWireV39 {
            version: 39,
            class: plan.class_ids[0],
            requester: owner,
            requester_operator: operator(&c, &owner),
            producer_operator: operator(&c, &producer),
            reward: 40,
            expires: c.daa + 3,
            maximum_contract: contract.record().root(),
            maximum: request_bytes(&m0),
            material: manifest_bytes(&m0),
        };
        let e = c.try_fold(c.daa + 1, &[stage(43, borsh::to_vec(&consent).unwrap(), producer)]).unwrap_err();
        eprintln!("[native39] a second job for a bound stage: {e}");
    }
    // A composition for a stage the Master does not have, or for another Master identity, is refused by the composer.
    assert!(compose_component_request_v39(plan.id, &plan.request, &plan.pipeline, &source, 1, &[]).is_err());
    assert!(compose_component_request_v39([0; 64], &plan.request, &plan.pipeline, &source, 0, &[]).is_err());
    // Final, then the aggregate.
    c.step_at(s0.final_daa, &[stage(51, final_body(&s0.job, 0), finalizer)]).unwrap_or_else(|e| panic!("component Final: {e}"));
    let before_close = (c.daa, c.s.clone());
    c.must("Master close 66", &[stage(66, plan.id.to_vec(), bonds[4].0)]);
    let closed = (c.daa, c.s.clone());
    assert!(c.aux(TABLE, &key(83, &plan.id)).is_some(), "the aggregate Final row 83");
    assert!(c.try_fold(c.daa + 1, &[stage(66, plan.id.to_vec(), bonds[4].0)]).is_err(), "the aggregate is recorded once");
    eprintln!("[native39] Master {}: job {} Final {}, aggregate at {}", hex8(&plan.id), hex8(&s0.job), s0.final_daa, closed.0);

    // ---- the complete Master Final audit under one anchor ----
    let pp = c.p.clone();
    let hp = header(&pp, closed.0, None, None, &[]);
    let hc = header(&pp, closed.0 + 1, Some(&hp), Some(&closed.1), &[]);
    let rows = stream(&closed.1, |_| {}).unwrap();
    let a = anchor(&pp, &hc, &hp, &closed.1).unwrap();
    let audit_m = PalwChainStateV2::native_master_final_audit_from_stream_v39(&rows, &a, &pp, plan.id, None)
        .expect("the Master Final audit")
        .expect("the aggregate exists");
    let rec = audit_m.record();
    assert_eq!((rec.master, rec.components.len()), (plan.id, 1));
    let j = PalwChainStateV2::native_job_audit_from_stream_v39(&rows, &a, &pp, s0.job, None).unwrap();
    assert_eq!((j.master(), j.stage()), (plan.id, Some(0)), "the component audit names its Master stage");
    let comp = &rec.components[0];
    assert_eq!(comp.original.job, s0.job);
    assert_eq!(comp.single_credit, j.credit().unwrap().root());
    assert_eq!(comp.claim, s0.commit.claim.root());
    assert_eq!((comp.amount, comp.finalized), (40, s0.final_daa));
    assert_eq!(j.maximum().root(), s0.maximum.root(), "the composed maximum is the funded original");
    assert_eq!(audit_m.source().record().id, plan.id);
    assert_eq!(audit_m.daa(), closed.0, "one anchor: the audit's DAA");
    // Before the aggregate exists the audit is None, never a success.
    let hb = header(&pp, before_close.0, None, None, &[]);
    let hbc = header(&pp, before_close.0 + 1, Some(&hb), Some(&before_close.1), &[]);
    let rows_b = stream(&before_close.1, |_| {}).unwrap();
    let ab = anchor(&pp, &hbc, &hb, &before_close.1).unwrap();
    assert!(PalwChainStateV2::native_master_final_audit_from_stream_v39(&rows_b, &ab, &pp, plan.id, None).unwrap().is_none());
    // Another state's stream under this anchor, or a tampered aggregate row, is refused.
    assert!(PalwChainStateV2::native_master_final_audit_from_stream_v39(&rows_b, &a, &pp, plan.id, None).is_err(), "stream/anchor");
    let agg_key = borsh::to_vec(&(TABLE, key(83, &plan.id))).unwrap();
    assert!(stream(&closed.1, |r| r.iter_mut().find(|x| x.0 == agg_key).unwrap().1[70] ^= 1).is_err(), "tampered aggregate");
}
