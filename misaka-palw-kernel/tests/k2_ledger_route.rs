//! **The consensus-embeddable ledger, attacked** (G14 R1–R5): signed route objects, the transactional per-object API, the block
//! budget, the canonical state root, settlement instructions, and a colluding producer + Panel against ordinary bonds.
//!
//! Every block in these tests goes through the mini consensus consumer of `common::chain` — it round-trips each object's strict
//! encoding, checks that a refusal leaves the state root byte-identical, applies every settlement instruction to a bond book and
//! checks the ledger's own bond view against it — so each scenario also proves those properties on its way.

mod common;

use common::chain::T;
use common::ledger_world::*;
use common::{MAX_POSITIONS, active_for, bump, root_of};
use misaka_palw_kernel::descriptor::{KernelStatusV1, k2_tir_v1_descriptor, k2_tir_v2_descriptor};
use misaka_palw_kernel::hash::{Digest, hex};
use misaka_palw_kernel::job::{DecodeRuleV1, KernelJobV1};
use misaka_palw_kernel::ledger::{
    AuthV1, DemandRowV1, KernelLedgerV1, KernelRefusalV1, KernelRouteObjectV1 as O, LedgerEventV1 as E, OutsiderFindingV1,
    ProsecutionV1, RefusalKindV1,
};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::route::{KERNEL_ROUTE_VERSION_V1, MAX_FILE_DEMAND_BYTES_V1, TAG_FILE_DEMAND_V1};
use misaka_palw_kernel::settle::{SettlementInstructionV1 as S, SettlementKindV1 as K};
use misaka_palw_kernel::state::{ClassRecordV1, StateRootPartsV1};
use misaka_palw_tir_sketch::fixture::dense_moe_v1;

fn auth(bond: Digest) -> AuthV1 {
    AuthV1 { signer_bond: bond }
}

/// Apply one object directly (not through the block harness): the receipts reach the consumer's book on success; a refusal must
/// leave the root unchanged.
fn direct(w: &mut World, o: &O, signer: Digest) -> Result<Vec<E>, KernelRefusalV1> {
    let before = w.l.root();
    match w.l.apply_object(o, &auth(signer)) {
        Ok(ev) => {
            w.consumer.book.apply_events(&ev).unwrap();
            Ok(ev)
        }
        Err(r) => {
            assert_eq!(w.l.root(), before, "a refusal changed the state: {r}");
            Err(r)
        }
    }
}

fn refused_with(w: &mut World, o: &O, signer: Digest, kind: RefusalKindV1) -> KernelRefusalV1 {
    let r = direct(w, o, signer).expect_err("must be refused");
    assert_eq!(r.kind, kind, "{r}");
    r
}

fn end(w: &mut World) -> Vec<E> {
    let ev = w.l.tick();
    w.consumer.book.apply_events(&ev).unwrap();
    ev
}

fn settlements(w: &World, from_daa: u64) -> Vec<S> {
    w.consumer.settlements.iter().filter(|(d, _)| *d >= from_daa).map(|(_, s)| *s).collect()
}

/// Seal `claim` in a block at the ledger's current DAA (a producer seals before it reveals).
fn seal_first(w: &mut World, claim: &misaka_palw_kernel::job::KernelClaimV1) {
    let seal = misaka_palw_kernel::ledger::claim_seal_v1(&claim.id());
    let daa = w.l.daa;
    let ev = w.block(daa, vec![T::SealClaim { producer: claim.producer_bond, job: claim.job_id, seal }]);
    assert!(ev.contains(&E::ClaimSealed { job: claim.job_id, producer: claim.producer_bond }), "{ev:?}");
}

fn ix(bond: Digest, amount: u64, kind: K, claim: Digest) -> S {
    S { bond, amount, kind, claim: Some(claim) }
}

// ── R2: the canonical state root ─────────────────────────────────────────────────────────────────────────────────────────

/// A small hand-built state (no fixture): two bonds one of them exiting, an attested artifact, a job, an open demand.
fn small_state() -> KernelLedgerV1 {
    let d = k2_tir_v1_descriptor();
    let mut l = KernelLedgerV1::genesis(policy(), active_for(&d), vec![d.clone()]).unwrap();
    l.begin_block(7).unwrap();
    l.sync_bond([1; 64], 5000);
    l.sync_bond([2; 64], 1000);
    l.attest_artifact([3; 64]);
    l.apply_object(&O::RequestExit { bond: [2; 64] }, &auth([2; 64])).unwrap();
    let job = KernelJobV1 {
        class_binding_id: [8; 64],
        prompt: vec![1, 2, 3],
        max_new_tokens: 2,
        decode: DecodeRuleV1::Greedy,
        nonce: [4; 64],
    };
    l.jobs.insert(job.id(), job);
    l.demands.insert(([4; 64], 0, 3), DemandRowV1 { demanders: vec![([2; 64], 10)], filed_daa: 5, deadline_daa: 25, last: Some(5) });
    l.burned = 11;
    l
}

fn part_names(a: &StateRootPartsV1, b: &StateRootPartsV1) -> Vec<&'static str> {
    let mut v = vec![];
    for (name, x, y) in [
        ("header", a.header, b.header),
        ("bonds", a.bonds, b.bonds),
        ("classes", a.classes, b.classes),
        ("pipeline_classes", a.pipeline_classes, b.pipeline_classes),
        ("jobs", a.jobs, b.jobs),
        ("pipeline_jobs", a.pipeline_jobs, b.pipeline_jobs),
        ("claims", a.claims, b.claims),
        ("demands", a.demands, b.demands),
        ("served", a.served, b.served),
        ("attested_artifacts", a.attested_artifacts, b.attested_artifacts),
    ] {
        if x != y {
            v.push(name);
        }
    }
    v
}

#[test]
fn the_state_root_is_versioned_canonical_and_pinned_by_a_golden_vector() {
    let l = small_state();
    let parts = l.root_parts();
    assert_eq!(parts.version, 1);
    let got: Vec<String> = [
        parts.header,
        parts.bonds,
        parts.classes,
        parts.pipeline_classes,
        parts.jobs,
        parts.pipeline_jobs,
        parts.claims,
        parts.demands,
        parts.served,
        parts.attested_artifacts,
    ]
    .iter()
    .map(|d| hex(&d[..8]))
    .collect();
    assert_eq!(got, GOLDEN_PARTS, "a change of the canonical encoding of any collection is a new root version");
    assert_eq!(hex(&l.root()), GOLDEN_ROOT);
    assert_eq!(l.root(), parts.root());
    // OPV-BOOT GAP-B1a: tables 25 and 26 are empty below `palw_panel_free_v1`, so they add nothing — this golden is the int-12-era root.
    assert!(l.claim_beacon_salts.is_empty() && l.forfeited_claim_seals.is_empty() && l.job_posters.is_empty());
    // Deterministic: the same fold twice, and a clone, agree.
    assert_eq!(small_state().root(), l.root());
    assert_eq!(l.clone().root(), l.root());
}

/// The first 8 bytes of each collection's root, for `small_state()`: header, bonds, classes, pipeline classes, jobs, pipeline jobs,
/// claims, demands, served, attested artifacts.
const GOLDEN_PARTS: [&str; 10] = [
    // The header moved with the G14-R4 fixes of GAP-5 (`LedgerPolicyV1::job_fee`, `job_escrow_ttl_daa`) and F-C4R3-05
    // (`prosecution_reserve_permille`), and OPV-BOOT's bonded seals (`seal_deposit`).
    "09043da9ac9f16cb",
    "801771b3f093df1f",
    "365486291ac886c2",
    "589b62088e37616b",
    "d96edea1b1a41437",
    "55df02bd4d504930",
    "b5c2d031b5d976db",
    "56b8aaefe844577a",
    "c46a4586dcf73c2d",
    "b7a5cb38db702693",
];
/// The root moved with the G14-R4 fixes of GAP-R7, GAP-5, F-C4R3-05, the bonded seals and the served-demand bonds (the accusers' proof seals and the posters' job escrows joined the root
/// as their own collections, and the policy in the header gained the job fee and the escrow TTL).
const GOLDEN_ROOT: &str =
    "f0dcae31c547757c6ef7329d6e382be012d4618fe82f39fe232510075d0d7bef4403616fd5f481f2430b2faf424d4f13050f35cca405dd325bf1b88984359fa5";

#[test]
fn each_collection_has_its_own_root_and_the_root_covers_the_state_and_nothing_else() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = w.lying(&job, 3);
    let (id, trace) = (lie.claim.id(), lie.trace.clone());
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    w.block(11, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    let base = w.l.root_parts();

    // Each mutation changes exactly its own collection's root.
    let changed = |mutate: &dyn Fn(&mut KernelLedgerV1)| {
        let mut l = w.l.clone();
        mutate(&mut l);
        let after = l.root_parts();
        assert_ne!(l.root(), w.l.root());
        part_names(&base, &after)
    };
    assert_eq!(changed(&|l| l.sync_bond(PRODUCER, 4999)), ["bonds"]);
    assert_eq!(changed(&|l| l.attest_artifact([7; 64])), ["attested_artifacts"]);
    assert_eq!(changed(&|l| l.burned += 1), ["header"]);
    assert_eq!(changed(&|l| l.begin_block(12).unwrap()), ["header"]);
    assert_eq!(changed(&|l| l.claims.get_mut(&id).unwrap().committed_daa += 1), ["claims"]);
    assert_eq!(changed(&|l| l.demands.get_mut(&(id, 0, at.0)).unwrap().deadline_daa += 1), ["demands"]);
    assert_eq!(changed(&|l| l.classes.values_mut().next().unwrap().plan.declared_error_bits += 1), ["classes"]);
    assert_eq!(changed(&|l| l.policy.demand_bond += 1), ["header"]);
    let job_id = job.id();
    assert_eq!(changed(&|l| l.jobs.get_mut(&job_id).unwrap().max_new_tokens += 1), ["jobs"]);
    assert_eq!(
        changed(&|l| {
            let served = misaka_palw_kernel::public::ServedPositionV1 { values: vec![], inputs: vec![] };
            l.served.insert((id, 0, 0), served);
        }),
        ["served"]
    );

    // Receipts and the block budget are not state: spending budget (a refused object that did court work) and starting a block at
    // the same clock leave the root alone.
    let root = w.l.root();
    w.l.begin_block(11).unwrap();
    let junk = O::FileProof { accuser: SPAM1, claim: id, proof: ProsecutionV1::Kernel(vec![1, 2, 3]) };
    let ev = direct(&mut w, &junk, SPAM1).unwrap();
    assert!(matches!(&ev[..], [E::ProofDismissed { .. }, ..]));
    assert_eq!(w.l.budget_used().adjudications, 1, "a court ran");
    // (the dismissal charged the fee and burned it: that IS state)
    assert_ne!(w.l.root(), root);
    let root = w.l.root();
    w.l.begin_block(11).unwrap();
    assert_eq!(w.l.budget_used().adjudications, 0);
    assert_eq!(w.l.root(), root, "the budget is block-local scratch");

    // Rows are canonical Borsh: they round-trip.
    let claim_row = w.l.claims[&id].clone();
    assert_eq!(borsh::from_slice::<misaka_palw_kernel::ledger::ClaimRowV1>(&borsh::to_vec(&claim_row).unwrap()).unwrap(), claim_row);
    let bond_row = w.l.bonds[&OUTSIDER].clone();
    assert_eq!(borsh::from_slice::<misaka_palw_kernel::ledger::BondRowV1>(&borsh::to_vec(&bond_row).unwrap()).unwrap(), bond_row);
    let demand_row = w.l.demands[&(id, 0, at.0)].clone();
    assert_eq!(borsh::from_slice::<DemandRowV1>(&borsh::to_vec(&demand_row).unwrap()).unwrap(), demand_row);
    let record = w.l.classes[&w.class].record();
    assert_eq!(borsh::from_slice::<ClassRecordV1>(&borsh::to_vec(&record).unwrap()).unwrap(), record);
    assert_eq!(record.param_commitments, misaka_palw_kernel::trace::ParamCommitmentsV1::of(&w.params), "only commitments are stored");
    let served_row = {
        w.block(12, vec![T::Respond { claim: id, stage: 0, position: at.0, bytes: position(&trace, at.0, |_| {}) }]);
        w.l.served[&(id, 0, at.0)].clone()
    };
    assert_eq!(
        borsh::from_slice::<misaka_palw_kernel::public::ServedPositionV1>(&borsh::to_vec(&served_row).unwrap()).unwrap(),
        served_row
    );
}

#[test]
fn the_root_does_not_depend_on_the_order_the_binary_lists_its_kernels() {
    let (d1, d2) = (k2_tir_v1_descriptor(), k2_tir_v2_descriptor());
    let schedule = |order: &[(&misaka_palw_kernel::descriptor::KernelDescriptorV1, u64)]| {
        let mut s = misaka_palw_kernel::descriptor::KernelScheduleV1::default();
        for (d, since) in order {
            s = s.with(d.digest(), KernelStatusV1::Active { since_daa: *since });
        }
        s
    };
    let a = KernelLedgerV1::genesis(policy(), schedule(&[(&d1, 0), (&d2, 5)]), vec![d1.clone(), d2.clone()]).unwrap();
    let b = KernelLedgerV1::genesis(policy(), schedule(&[(&d2, 5), (&d1, 0)]), vec![d2.clone(), d1.clone()]).unwrap();
    assert_eq!(a.root(), b.root());
    let c = KernelLedgerV1::genesis(policy(), schedule(&[(&d1, 0), (&d2, 6)]), vec![d1.clone(), d2.clone()]).unwrap();
    assert_ne!(a.root(), c.root(), "nodes that disagree on the kernel schedule disagree on the root");
    let e = KernelLedgerV1::genesis(policy(), schedule(&[(&d1, 0), (&d2, 5)]), vec![d1.clone()]).unwrap();
    assert_ne!(a.root(), e.root());
}

// ── R3: who may sign what; malformed and oversized objects; a refusal never changes the state ───────────────────────────

#[test]
fn an_object_is_applied_only_if_signed_by_the_actor_it_names_and_a_refusal_changes_nothing() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = w.lying(&job, 3);
    let id = lie.claim.id();
    let (a, commit) = lie.tx.clone().signed(PRODUCER);
    assert_eq!(a, auth(PRODUCER));
    seal_first(&mut w, &lie.claim);
    // Nobody but the producer a claim names can commit it (the Panel's friends cannot commit a claim in its name).
    w.l.begin_block(10).unwrap();
    for imposter in [OUTSIDER, SPAM1, [0xEE; 64]] {
        refused_with(&mut w, &commit, imposter, RefusalKindV1::Unauthorized);
    }
    assert!(w.l.claims.is_empty());
    direct(&mut w, &commit, PRODUCER).unwrap();
    w.l.apply_panel_tally(&id, true).unwrap();
    end(&mut w);

    // A proof, a demand, an exit and a withdrawal are signed by the accuser / demander / bond they name.
    w.l.begin_block(11).unwrap();
    let proof = O::FileProof { accuser: OUTSIDER, claim: id, proof: ProsecutionV1::Kernel(vec![1]) };
    refused_with(&mut w, &proof, SPAM1, RefusalKindV1::Unauthorized);
    let demand = O::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 };
    refused_with(&mut w, &demand, SPAM1, RefusalKindV1::Unauthorized);
    refused_with(&mut w, &O::RequestExit { bond: PRODUCER }, OUTSIDER, RefusalKindV1::Unauthorized);
    refused_with(&mut w, &O::Withdraw { bond: PRODUCER }, OUTSIDER, RefusalKindV1::Unauthorized);
    // Objects naming no actor still need a bond the consumer synced.
    let junk_job =
        KernelJobV1 { class_binding_id: w.class, prompt: vec![1], max_new_tokens: 1, decode: DecodeRuleV1::Greedy, nonce: [9; 64] };
    refused_with(&mut w, &O::PostJob { job: junk_job.clone() }, [0xEE; 64], RefusalKindV1::Unauthorized);
    refused_with(&mut w, &O::Respond { claim: id, stage: 0, position: 0, bytes: vec![] }, [0xEE; 64], RefusalKindV1::Unauthorized);
    // An exiting bond adds no class or job (state that outlives it) but a producer can always answer.
    direct(&mut w, &O::RequestExit { bond: SPAM2 }, SPAM2).unwrap();
    refused_with(&mut w, &O::PostJob { job: junk_job }, SPAM2, RefusalKindV1::Unauthorized);
    direct(&mut w, &demand, OUTSIDER).unwrap();
    let r = refused_with(&mut w, &O::Respond { claim: id, stage: 0, position: 9, bytes: vec![] }, SPAM2, RefusalKindV1::Rule);
    assert_eq!(r.why, "no open demand for this position", "an exiting bond may respond: the refusal is the rule's, not the auth's");
    end(&mut w);
}

#[test]
fn malformed_and_oversized_objects_are_refused_transactionally_through_the_strict_codec() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    seal_first(&mut w, &h.claim);
    let (a, commit) = h.tx.clone().signed(PRODUCER);
    let good = commit.encode();
    w.l.begin_block(3).unwrap();
    let before = w.l.root();
    let mut cases: Vec<(&str, Vec<u8>, RefusalKindV1)> = vec![];
    cases.push(("empty", vec![], RefusalKindV1::Malformed));
    cases.push(("truncated", good[..good.len() / 2].to_vec(), RefusalKindV1::Malformed));
    let mut trailing = good.clone();
    trailing.push(0);
    cases.push(("trailing byte", trailing, RefusalKindV1::Malformed));
    let mut version = good.clone();
    version[0] = KERNEL_ROUTE_VERSION_V1 + 1;
    cases.push(("another version", version, RefusalKindV1::Malformed));
    let mut tag = good.clone();
    tag[1] = 0xFE;
    cases.push(("unknown tag", tag, RefusalKindV1::Malformed));
    // A flipped byte inside the evidence either fails to parse or is another object entirely; never a panic, never applied.
    for i in [2usize, 70, 140, good.len() / 3, good.len() - 1] {
        let mut flipped = good.clone();
        flipped[i] ^= 0xFF;
        cases.push(("flipped byte", flipped, RefusalKindV1::Rule));
    }
    let mut demand = O::FileDemand { demander: OUTSIDER, claim: [0; 64], stage: 0, position: 0 }.encode();
    demand.resize(MAX_FILE_DEMAND_BYTES_V1 + 1, 0);
    cases.push(("oversized demand", demand, RefusalKindV1::Oversized));
    let mut lie = vec![KERNEL_ROUTE_VERSION_V1, 9];
    lie.extend([1u8; 64]);
    lie.extend([0u8, 0, 0, 0, 0]);
    lie.extend(u32::MAX.to_le_bytes());
    cases.push(("a length prefix larger than the bytes", lie, RefusalKindV1::Malformed));
    for (name, bytes, kind) in cases {
        let r = w.l.apply_encoded(&bytes, &a);
        match (name, r) {
            ("flipped byte", Err(r)) => {
                assert!(matches!(r.kind, RefusalKindV1::Malformed | RefusalKindV1::Rule | RefusalKindV1::Unauthorized), "{name}: {r}")
            }
            ("flipped byte", Ok(ev)) => panic!("a corrupted claim was applied: {ev:?}"),
            (_, Ok(ev)) => panic!("{name}: applied {ev:?}"),
            (_, Err(r)) => assert_eq!(r.kind, kind, "{name}: {r}"),
        }
        assert_eq!(w.l.root(), before, "{name}: a refused object changed the state");
    }
    // The good bytes apply, through the same entry point.
    let ev = w.l.apply_encoded(&good, &a).unwrap();
    assert!(ev.iter().any(|e| matches!(e, E::ClaimCommitted { .. })));
    assert_ne!(w.l.root(), before);
    // PanelCovered is not an object: no declared tag decodes to it, and its old shape is not a route object.
    for tag in 0u8..=40 {
        if !(1..=11).contains(&tag) {
            let mut b = vec![KERNEL_ROUTE_VERSION_V1, tag];
            b.extend([0u8; 64]);
            assert_eq!(w.l.apply_encoded(&b, &a).unwrap_err().kind, RefusalKindV1::Malformed, "tag {tag}");
        }
    }
    assert_eq!(TAG_FILE_DEMAND_V1, 8);
}

#[test]
fn a_class_registers_commitments_only_over_an_attested_artifact_and_never_twice() {
    let fx = dense_moe_v1(7);
    let d = k2_tir_v1_descriptor();
    let plan = plan_for_tir_program_v1(&d, &fx.program, root_of(&fx.program), MAX_POSITIONS).unwrap();
    let pc = misaka_palw_kernel::trace::ParamCommitmentsV1::of(&fx.params);
    let register = |pc: &misaka_palw_kernel::trace::ParamCommitmentsV1| O::RegisterClass {
        descriptor: d.digest(),
        program_bytes: fx.program.encode(),
        plan: plan.clone(),
        param_commitments: pc.clone(),
    };
    let genesis = KernelLedgerV1::genesis(policy(), active_for(&d), vec![d.clone()]).unwrap();
    let mut l = genesis.clone();
    l.begin_block(1).unwrap();
    l.sync_bond(PRODUCER, 5000);
    let err = |l: &mut KernelLedgerV1, o: &O| {
        let before = l.root();
        let r = l.apply_object(o, &auth(PRODUCER)).unwrap_err();
        assert_eq!(l.root(), before, "{r}");
        r.why
    };
    // The consumer's registry has not attested the artifact public: the registrant's say-so is not enough.
    assert!(err(&mut l, &register(&pc)).contains("not attested public"));
    l.attest_artifact(pc.root());
    // A commitment set that lacks a param a relation reads makes that relation unprovable: refused.
    let mut missing = pc.clone();
    let k = *missing.by_instance.keys().next().unwrap();
    missing.by_instance.remove(&k);
    l.attest_artifact(missing.root());
    let why = err(&mut l, &register(&missing));
    assert!(why.contains("lack param"), "{why}");
    // A commitment for an instance the program does not declare is junk: refused.
    let mut junk = pc.clone();
    junk.by_instance.insert((4000, None), [1; 64]);
    l.attest_artifact(junk.root());
    assert!(err(&mut l, &register(&junk)).contains("does not declare"));
    // The honest registration stores the commitments, not the artifact; the class id binds them.
    let ev = l.apply_object(&register(&pc), &auth(PRODUCER)).unwrap();
    let [E::ClassRegistered { class }] = &ev[..] else { panic!("{ev:?}") };
    let row = &l.classes[class];
    assert_eq!(row.param_commitments, pc);
    // Registering it again would overwrite the row every court reads: refused, and the row is untouched.
    let after = l.root();
    assert_eq!(err(&mut l, &register(&pc)), "the class is already registered");
    assert_eq!(l.root(), after);
    // An exiting registrant adds none.
    l.apply_object(&O::RequestExit { bond: PRODUCER }, &auth(PRODUCER)).unwrap();
    assert_eq!(l.apply_object(&register(&pc), &auth(PRODUCER)).unwrap_err().kind, RefusalKindV1::Unauthorized);
}

#[test]
fn an_outsider_authenticates_the_public_artifact_against_the_registered_commitments() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let (id, da) = (h.claim.id(), Da::publishing(&h.trace, &[]));
    w.block(10, vec![h.tx, T::PanelCovered { claim: id }]);
    let fresh = KernelLedgerV1::replay(&w.genesis, &w.blocks);
    let check = |artifact: &dyn misaka_palw_kernel::ledger::PublicArtifactV1| {
        misaka_palw_kernel::ledger::OutsiderV1 { ledger: &fresh, claim: id, material: &da, artifact, salt: [0x5A; 64] }.check()
    };
    assert_eq!(check(&w.params).unwrap(), OutsiderFindingV1::Clean);
    // A mirror serving a perturbed weight is unauthenticated: the outsider has no artifact, not a wrong one — it reports
    // unavailable, and never convicts the honest producer with a forged weight.
    let mut tampered = w.params.clone();
    let key = *tampered.tensors.keys().find(|k| k.0 == 8).unwrap();
    bump(tampered.tensors.get_mut(&key).unwrap(), 0);
    let r = check(&tampered);
    assert!(r.as_ref().is_err_and(|e| e.contains("unavailable")), "{r:?}");
    // A mirror serving nothing is unavailable too.
    let empty = misaka_palw_tir::MapParams { tensors: Default::default() };
    assert!(check(&empty).is_err());
}

// ── R4: the block budget and the cost of junk ────────────────────────────────────────────────────────────────────────────

#[test]
fn an_over_budget_object_is_refused_without_a_fee_and_a_later_block_still_convicts() {
    let mut p = policy();
    p.max_adjudications_per_block = 4;
    let mut w = World::with(p);
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };

    // A bonded spammer fills the block's court budget with junk proofs; each costs the fee, and the real proof behind them is
    // dropped (refused, not fatal, no fee).
    let junk = |i: u8| T::FileProof { accuser: SPAM1, claim: id, proof: ProsecutionV1::Kernel(vec![i, 2, 3]) };
    let mut txs: Vec<T> = (0..4).map(junk).collect();
    txs.push(T::FileProof { accuser: OUTSIDER, claim: id, proof: proof.clone() });
    let ev = w.block(20, txs);
    assert_eq!(ev.iter().filter(|e| matches!(e, E::ProofDismissed { .. })).count(), 4, "{ev:?}");
    let why = refused(&ev).unwrap();
    assert!(why.starts_with("the block's adjudication budget is spent"), "{why}");
    assert!(convicted(&ev).is_none());
    assert_eq!(w.l.bonds[&SPAM1].collateral, 1000 - 4 * 5, "the spammer paid for every court run; the outsider paid nothing");
    assert_eq!(w.l.bonds[&OUTSIDER].collateral, 1000);
    assert_eq!(w.l.burned, 20 + 2, "four fees (and the job's non-refundable posting fee: GAP-5)");
    // The budget is per block: the same proof, in the next block, convicts. The spam bought one block of delay at 4 fees.
    let ev = w.block(21, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)), "{ev:?}");

    // The kind of the refusal, on the per-object API: OverBudget, the state untouched.
    let mut p = policy();
    p.max_adjudications_per_block = 1;
    let mut w = World::with(p);
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    w.block(10, vec![h.tx, T::PanelCovered { claim: id }]);
    w.l.begin_block(11).unwrap();
    let junk = O::FileProof { accuser: SPAM1, claim: id, proof: ProsecutionV1::Kernel(vec![1, 2, 3]) };
    direct(&mut w, &junk, SPAM1).unwrap();
    assert_eq!(w.l.budget_used().adjudications, 1);
    let r = refused_with(&mut w, &junk, SPAM1, RefusalKindV1::OverBudget);
    assert!(r.why.contains("budget"));
    // The next block starts with a fresh budget.
    w.l.begin_block(12).unwrap();
    assert!(direct(&mut w, &junk, SPAM1).is_ok());
}

#[test]
fn the_court_work_budget_bounds_what_one_block_can_make_the_network_run() {
    // One block may run a court of the class's worst cost exactly once.
    let probe = World::new();
    let work = probe.l.classes[&probe.class].bounds.max_court_work;
    assert!(work > 0);
    let mut p = policy();
    p.max_court_work_per_block = work;
    p.prosecution_reserve_permille = 0; // This test isolates the full-block court ceiling; reserve liveness is tested below.
    let mut w = World::with(p);
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    w.block(10, vec![h.tx, T::PanelCovered { claim: id }]);
    let junk = |i: u8| T::FileProof { accuser: SPAM1, claim: id, proof: ProsecutionV1::Kernel(vec![i, 2, 3]) };
    let ev = w.block(11, vec![junk(1), junk(2), junk(3)]);
    assert_eq!(ev.iter().filter(|e| matches!(e, E::ProofDismissed { .. })).count(), 1, "{ev:?}");
    assert_eq!(ev.iter().filter(|e| matches!(e, E::Refused { why, .. } if why.contains("budget"))).count(), 2);

    // A class whose worst court cannot fit one block's budget never registers: nobody could prosecute it.
    let mut p = policy();
    p.max_court_work_per_block = work - 1;
    let w = World::with(p);
    assert!(w.l.classes.is_empty(), "{:?}", w.events);
    assert!(refused(&w.events).unwrap().contains("court budget"));
}

#[test]
fn invalid_challenge_spam_is_priced_by_free_collateral_and_bounded_by_the_block_budget() {
    let mut p = policy();
    p.max_adjudications_per_block = 8;
    let mut w = World::with(p);
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    w.block(10, vec![h.tx, T::PanelCovered { claim: id }]);
    // A spammer with 1000 collateral and a fee of 5 can buy 200 court runs in all, 8 a block: a bounded drain, never a stall.
    let (mut dismissed, mut priced_out) = (0usize, 0usize);
    for b in 0..40u64 {
        let txs: Vec<T> =
            (0..8u8).map(|i| T::FileProof { accuser: SPAM1, claim: id, proof: ProsecutionV1::Kernel(vec![b as u8, i, 9]) }).collect();
        let ev = w.block(11 + b, txs);
        dismissed += ev.iter().filter(|e| matches!(e, E::ProofDismissed { .. })).count();
        priced_out += ev.iter().filter(|e| matches!(e, E::Refused { why, .. } if why.contains("free collateral"))).count();
        assert!(!w.l.claims[&id].convicted, "whatever the spam, an honest claim is not convicted");
        assert!(w.l.budget_used().adjudications <= 8);
    }
    assert_eq!(dismissed, 200, "1000 / 5");
    assert_eq!(priced_out, 40 * 8 - 200, "once the free collateral cannot cover the fee a filing is refused outright: no court run");
    assert_eq!(w.l.bonds[&SPAM1].collateral, 0);
    assert_eq!(w.l.burned, 1000 + 2, "every fee (and the job's posting fee: GAP-5)");
    assert_eq!(w.consumer.book.burned, 1000 + 2, "the whole fee was burned, by explicit instructions");
}

// ── simultaneous public challengers, duplicates, restarts ────────────────────────────────────────────────────────────────

#[test]
fn simultaneous_public_challengers_convict_once_and_the_second_is_a_duplicate_with_no_second_slash() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    // Two ordinary bonds found the same fault independently (each with its own salt) and file in the same block.
    let OutsiderFindingV1::Prosecute(p1) = outsider(&w, id, &da) else { panic!() };
    let fresh = KernelLedgerV1::replay(&w.genesis, &w.blocks);
    let OutsiderFindingV1::Prosecute(p2) =
        (misaka_palw_kernel::ledger::OutsiderV1 { ledger: &fresh, claim: id, material: &da, artifact: &w.params, salt: [0xC3; 64] })
            .check()
            .unwrap()
    else {
        panic!()
    };
    let collateral = w.l.bonds[&PRODUCER].collateral;
    let ev = w.block(
        20,
        vec![
            T::FileProof { accuser: OUTSIDER, claim: id, proof: p1 },
            T::FileProof { accuser: SPAM1, claim: id, proof: p2 },
            T::FileProof { accuser: SPAM2, claim: id, proof: ProsecutionV1::Kernel(vec![9]) },
        ],
    );
    assert_eq!(convicted(&ev), Some((1000, 500, false)), "{ev:?}");
    assert_eq!(ev.iter().filter(|e| matches!(e, E::Convicted { .. })).count(), 1);
    assert_eq!(ev.iter().filter(|e| matches!(e, E::Duplicate { .. })).count(), 2, "the second finder and even a junk filing: {ev:?}");
    assert_eq!(w.l.bonds[&PRODUCER].collateral, collateral - 1000, "slashed once");
    assert_eq!(w.consumer.paid(&OUTSIDER), 500);
    assert_eq!(w.consumer.paid(&SPAM1), 0, "no reward for the duplicate");
    assert_eq!(w.l.bonds[&SPAM1].collateral, 1000, "and no fee: a duplicate is not a dismissal");
    assert_eq!(w.consumer.book.slashed, 1000 + 2, "one slash (and the job's posting fee: GAP-5)");
    // The exact instructions of the conviction.
    assert_eq!(
        settlements(&w, 20),
        vec![ix(PRODUCER, 1000, K::SlashFraud, id), ix(OUTSIDER, 500, K::AccuserReward, id), ix(PRODUCER, 500, K::Burn, id),]
    );
}

#[test]
fn a_proof_resubmitted_after_a_restart_or_an_ibd_replay_is_a_duplicate() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let file = |who| T::FileProof { accuser: who, claim: id, proof: proof.clone() };
    w.block(20, vec![file(OUTSIDER)]);
    assert!(w.l.claims[&id].convicted);
    let root = w.l.root();

    // IBD from genesis: the fresh node reaches the same state, and the same proof resubmitted is a duplicate with no effect.
    let mut ibd = KernelLedgerV1::replay(&w.genesis, &w.blocks);
    assert_eq!(ibd.root(), root);
    let ev = ibd.apply_block(&misaka_palw_kernel::ledger::LedgerBlockV1 { daa: 21, txs: file(SPAM1).into_txs(PRODUCER) });
    assert_eq!(ev.iter().filter(|e| matches!(e, E::Duplicate { .. })).count(), 1, "{ev:?}");
    assert!(!ev.iter().any(|e| matches!(e, E::Convicted { .. })));
    assert_eq!(ibd.bonds[&PRODUCER].collateral, w.l.bonds[&PRODUCER].collateral);

    // A restart half-way, then the rest, then the same proof again.
    let half = w.blocks.len() / 2;
    let mut restarted = KernelLedgerV1::replay(&w.genesis, &w.blocks[..half]);
    for b in &w.blocks[half..] {
        restarted.apply_block(b);
    }
    assert_eq!(restarted.root(), root);
    let ev = restarted.apply_block(&misaka_palw_kernel::ledger::LedgerBlockV1 { daa: 22, txs: file(OUTSIDER).into_txs(PRODUCER) });
    assert!(ev.iter().any(|e| matches!(e, E::Duplicate { .. })) && !ev.iter().any(|e| matches!(e, E::Convicted { .. })), "{ev:?}");
}

// ── R1 against a colluding Panel: the court is never pre-empted by the grace or by open sessions ─────────────────────────

#[test]
fn a_direct_proof_inside_the_grace_pre_empts_final_whether_sessions_are_settled_or_still_open() {
    // (1) The producer's friends demand two positions in the window's last block and the producer answers both at the deadline
    // (79): the window is closed, no session is open, and Final waits for the grace. The outsider's direct proof lands inside it.
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    let trace = lie.trace.clone();
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]); // window end 60
    w.block(
        59,
        vec![
            T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 0 },
            T::FileDemand { demander: SPAM2, claim: id, stage: 0, position: 2 },
        ],
    );
    let serve = |p: u32| T::Respond { claim: id, stage: 0, position: p, bytes: position(&trace, p, |_| {}) };
    w.block(79, vec![serve(0), serve(2)]);
    assert!(matches!(w.state(&id), ClaimStateV1::WindowClosed { .. }), "no Final inside the grace: {:?}", w.state(&id));
    assert!(w.l.demands.is_empty());
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let ev = w.block(88, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)), "pre-Final, last block of the grace: {ev:?}");
    assert!(!ev.iter().any(|e| matches!(e, E::Final { .. })));
    w.block(300, vec![]);
    assert!(!w.l.claims[&id].rewarded && matches!(w.state(&id), ClaimStateV1::Convicted { .. }));

    // (2) A session is still open when the direct proof lands: the proof is adjudicated in its own block, the open session
    // settles moot, every bond returns, and the producer never earns the reward.
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    let trace = lie.trace.clone();
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    w.block(
        59,
        vec![
            T::FileDemand { demander: SPAM1, claim: id, stage: 0, position: 0 },
            T::FileDemand { demander: SPAM2, claim: id, stage: 0, position: 2 },
        ],
    );
    w.block(70, vec![T::Respond { claim: id, stage: 0, position: 0, bytes: position(&trace, 0, |_| {}) }]);
    assert!(matches!(w.state(&id), ClaimStateV1::Disputed { open: 1, .. }));
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    let ev = w.block(71, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert_eq!(convicted(&ev), Some((1000, 500, false)), "{ev:?}");
    assert!(ev.contains(&E::DemandsMoot { claim: id, refunded: 1 }));
    assert!(w.l.demands.is_empty());
    assert_eq!(w.l.bonds[&SPAM2].reserved, 0);
    w.block(300, vec![]);
    assert!(!w.l.claims[&id].rewarded);
}

// ── settlements are explicit ─────────────────────────────────────────────────────────────────────────────────────────────

/// **OPV-BOOT's sealed-source beacon (v3) primitives.** A claim seal is BONDED: `seal_deposit` of the producer's free collateral is
/// reserved when it seals (a re-seal keeps the one deposit and restarts the clock), returned when the claim commits over it, and
/// FORFEITED (slashed, burned) if it expires unrevealed — withholding a sealed reveal is never free. The claim row keeps the seal's DAA
/// (`sealed_daa`), so the beacon can order sources by their seal position after the reveal.
#[test]
fn a_claim_seal_is_bonded_kept_on_the_claim_row_and_forfeited_when_withheld() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let (id, seal) = (h.claim.id(), misaka_palw_kernel::ledger::claim_seal_v1(&h.claim.id()));
    let ev = w.block(5, vec![T::SealClaim { producer: PRODUCER, job: job.id(), seal }]);
    assert!(ev.contains(&E::ClaimSealed { job: job.id(), producer: PRODUCER }), "{ev:?}");
    assert_eq!(settlements(&w, 5), vec![S { bond: PRODUCER, amount: 1, kind: K::ReserveSealDeposit, claim: None }]);
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 1);
    // A re-seal keeps the one deposit; its clock restarts (the harness re-seals at the ledger's clock before the reveal).
    w.block(6, vec![T::SealClaim { producer: PRODUCER, job: job.id(), seal }]);
    assert_eq!((w.l.bonds[&PRODUCER].reserved, w.l.seals[&(job.id(), PRODUCER)].daa), (1, 6));
    w.block(8, vec![h.tx, T::PanelCovered { claim: id }]);
    assert_eq!(w.l.claims[&id].sealed_daa, 6, "the seal's position survives the reveal, on the claim row");
    assert!(settlements(&w, 8).contains(&ix(PRODUCER, 1, K::ReleaseSealDeposit, id)), "the deposit returns at the reveal");
    assert_eq!(w.l.bonds[&PRODUCER].reserved, 1000, "the claim's reservation alone");
    // A seal nobody reveals expires and forfeits its deposit.
    let job2 = w.post_job(20, &[3, 17, 9], 3, 2);
    w.block(21, vec![T::SealClaim { producer: PRODUCER, job: job2.id(), seal: [0x5E; 64] }]);
    let (collateral, burned) = (w.l.bonds[&PRODUCER].collateral, w.l.burned);
    let ev = w.block(21 + 100 + 1, vec![]);
    assert!(ev.contains(&E::SealForfeited { job: job2.id(), producer: PRODUCER, forfeited: 1 }), "{ev:?}");
    assert_eq!((w.l.bonds[&PRODUCER].collateral, w.l.burned), (collateral - 1, burned + 1));
    assert!(w.l.seals.is_empty());
    assert!(w.l.forfeited_claim_seals.is_empty(), "below palw_panel_free_v1 a forfeited seal leaves no row (the historical root)");
    // A producer with no free collateral for the deposit cannot seal.
    w.block(130, vec![T::RegisterBond { bond: SPAM2, collateral: 0 }]);
    let ev = w.block(131, vec![T::SealClaim { producer: SPAM2, job: job2.id(), seal: [0x5F; 64] }]);
    assert!(refused(&ev).unwrap().contains("seal deposit"), "{ev:?}");
}

/// **A seal that lost its job to another claim is forfeited too — deliberately.** One claim per job: once another producer's claim
/// holds the job, a rival's live seal of it can never be revealed, and it expires like a withheld one. Refunding it would make
/// grinding free: N bonds each seal the same job (one honest output, N claim ids) and the last to act reveals the one whose id suits
/// the sealed-source beacon — the others would come back. The forfeit prices that choice at one deposit per discarded seal; an honest
/// producer reads the seals already on chain before it seals a job.
#[test]
fn a_seal_on_a_job_another_claim_took_is_forfeited_at_its_expiry() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    w.block(4, vec![T::SealClaim { producer: OUTSIDER, job: job.id(), seal: [0x5A; 64] }]);
    assert_eq!(w.l.bonds[&OUTSIDER].reserved, 1, "the rival's seal holds its deposit");
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    w.block(8, vec![h.tx, T::PanelCovered { claim: id }]);
    assert!(w.l.claims.contains_key(&id), "the producer's claim holds the job");
    let ev = w.block(9, vec![T::SealClaim { producer: OUTSIDER, job: job.id(), seal: [0x5B; 64] }]);
    assert!(refused(&ev).unwrap().contains("another claim already holds the job"), "{ev:?}");
    assert_eq!(w.l.seals[&(job.id(), OUTSIDER)].deposit, 1, "the live seal stays, unrevealable");
    w.block(100, vec![]);
    let (collateral, burned) = (w.l.bonds[&OUTSIDER].collateral, w.l.burned);
    let ev = w.block(4 + 100 + 1, vec![]);
    assert!(ev.contains(&E::SealForfeited { job: job.id(), producer: OUTSIDER, forfeited: 1 }), "{ev:?}");
    assert_eq!((w.l.bonds[&OUTSIDER].collateral, w.l.bonds[&OUTSIDER].reserved, w.l.burned), (collateral - 1, 0, burned + 1));
}

/// **GAP-5 (the user's ruling: user-pays escrow): a Final reward is paid out of the job's escrow, once — nothing is issued.** Every
/// posted job reserves `claim_reward` of its poster's free collateral (`ReserveJobEscrow`) and burns `job_fee` (`JobFee` + `Burn`).
/// The job's first Final debits the escrow (`PayJobEscrow`) and pays exactly that (`FinalReward`). A post-Final conviction frees the
/// job but the next claim of it finalizes with no reward; an escrow no claim can use goes back to its poster after
/// `job_escrow_ttl_daa`; a poster that cannot cover escrow + fee posts nothing; a producer answering its own job is paid its own
/// escrow back and is down the fee. The money identity holds throughout: every payout is routed out of a debit of a real bond in the
/// same batch (the test consumer's book refuses a `FinalReward` with no spent escrow behind it, in every ledger test).
#[test]
fn a_final_reward_is_paid_once_out_of_the_posters_escrow_and_nothing_is_ever_issued() {
    use common::chain::{POSTER, POSTER_COLLATERAL};
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    assert_eq!(
        settlements(&w, 2),
        vec![
            S { bond: POSTER, amount: 7, kind: K::ReserveJobEscrow, claim: None },
            S { bond: POSTER, amount: 2, kind: K::JobFee, claim: None },
            S { bond: POSTER, amount: 2, kind: K::Burn, claim: None },
        ]
    );
    assert_eq!(w.l.job_escrows[&job.id()].amount, 7);
    assert_eq!((w.l.bonds[&POSTER].collateral, w.l.bonds[&POSTER].reserved), (POSTER_COLLATERAL - 2, 7));
    // A lie finalizes unprosecuted: it is paid out of the escrow, which is spent.
    let (_, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[]));
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    assert_eq!(w.block(60, vec![]), vec![E::Final { claim: id, reward: 7 }]);
    assert_eq!(
        settlements(&w, 60),
        vec![ix(POSTER, 7, K::PayJobEscrow, id), ix(PRODUCER, 7, K::FinalReward, id)],
        "the poster pays exactly what the producer is paid"
    );
    assert!(w.l.job_escrows.is_empty(), "an escrow pays once");
    assert_eq!((w.l.bonds[&POSTER].collateral, w.l.bonds[&POSTER].reserved), (POSTER_COLLATERAL - 2 - 7, 0));
    // Convicted inside its liability horizon: the job is free again (the paid reward is not clawed back) — and a second claim of the
    // same job finalizes with NO reward: one escrow, one reward.
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!() };
    w.block(61, vec![T::FileProof { accuser: OUTSIDER, claim: id, proof }]);
    assert!(w.l.claims[&id].convicted);
    let h = w.honest(&job, 3);
    let hid = h.claim.id();
    w.block(70, vec![h.tx, T::PanelCovered { claim: hid }]);
    assert_eq!(w.block(120, vec![]), vec![E::Final { claim: hid, reward: 0 }]);
    assert!(!w.l.claims[&hid].rewarded);
    assert_eq!((w.consumer.book.final_rewards, w.consumer.book.escrow_spent), (7, 7));
    // An escrow no claim can use goes back after its TTL (posted 130 + 300); while a producer's seal of the job is live it waits.
    let idle = w.post_job(130, &[3, 17, 9], 3, 2);
    w.block(400, vec![T::SealClaim { producer: PRODUCER, job: idle.id(), seal: [0x11; 64] }]);
    w.block(431, vec![]);
    assert!(w.l.job_escrows.contains_key(&idle.id()), "past its TTL, but a live seal of the job keeps its escrow (until 500)");
    w.block(501, vec![]);
    assert!(!w.l.job_escrows.contains_key(&idle.id()), "returned once the seal expired");
    assert_eq!(w.l.bonds[&POSTER].reserved, 0);
    assert!(
        w.events
            .iter()
            .any(|e| matches!(e, E::JobEscrowReturned { job, poster, amount: 7 } if *job == idle.id() && *poster == POSTER))
    );
    // A poster that cannot cover escrow + fee posts nothing (and pays nothing).
    w.block(510, vec![T::RegisterBond { bond: POSTER, collateral: 8 }]);
    let ev = w.block(511, vec![T::PostJob { job: KernelJobV1 { nonce: [9; 64], ..job.clone() } }]);
    assert!(refused(&ev).unwrap().contains("escrow and fee"), "{ev:?}");
    assert_eq!(w.l.bonds[&POSTER].collateral, 8);
    // A self-posted job: the producer posts and answers its own job — paid its own escrow back, it is down the fee: never a gain.
    let own = KernelJobV1 { nonce: [8; 64], ..job.clone() };
    let before = w.l.bonds[&PRODUCER].collateral + w.consumer.paid(&PRODUCER);
    direct(&mut w, &O::PostJob { job: own.clone() }, PRODUCER).unwrap();
    let h = w.honest(&own, 3);
    let hid = h.claim.id();
    w.block(520, vec![h.tx, T::PanelCovered { claim: hid }]);
    assert_eq!(w.block(570, vec![]), vec![E::Final { claim: hid, reward: 7 }]);
    let after = w.l.bonds[&PRODUCER].collateral + w.consumer.paid(&PRODUCER);
    assert_eq!(before - after, 2, "a self-posted job costs exactly its non-refundable fee");
    // The money identity over the whole run: every payout came out of a debit, none was issued.
    let b = &w.consumer.book;
    let paid: u64 = b.paid.values().sum();
    assert_eq!(b.slashed, b.routed, "every debit is routed: paid out or burned");
    assert!(paid + b.burned <= b.slashed, "Σ payouts ≤ Σ fees + slashes + spent escrows (no source counted twice)");
    assert_eq!(b.final_rewards, b.escrow_spent, "every Final reward is a spent escrow");
}

#[test]
fn every_money_decision_is_an_explicit_settlement_instruction_and_nothing_is_minted_internally() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let (at, lie) = w.lying(&job, 3);
    let (id, da) = (lie.claim.id(), Da::publishing(&lie.trace, &[at]));
    w.block(10, vec![lie.tx, T::PanelCovered { claim: id }]);
    // (the claim's seal deposit returns at its reveal: OPV-BOOT's bonded seals)
    assert_eq!(settlements(&w, 10), vec![ix(PRODUCER, 1, K::ReleaseSealDeposit, id), ix(PRODUCER, 1000, K::ReserveClaim, id)]);
    // A demand that defaults: bond reserved, then default slash, the demander's share of it and the burned rest (a default is split
    // like a slash, C4 F-C4R3-02), the bond back. The rest of the reservation is HELD through the default's liability horizon.
    let OutsiderFindingV1::Demand(_) = outsider(&w, id, &da) else { panic!() };
    w.block(11, vec![T::FileDemand { demander: OUTSIDER, claim: id, stage: 0, position: at.0 }]);
    assert_eq!(settlements(&w, 11), vec![ix(OUTSIDER, 10, K::ReserveDemand, id)]);
    w.block(31, vec![]);
    assert_eq!(
        settlements(&w, 31),
        vec![
            ix(PRODUCER, 100, K::SlashDefault, id),
            ix(OUTSIDER, 50, K::DemanderShare, id),
            ix(PRODUCER, 50, K::Burn, id),
            ix(OUTSIDER, 10, K::ReleaseDemand, id),
        ]
    );
    // An honest claim: Final pays the reward by instruction, and the liability horizon's end releases the reservation.
    let job = w.post_job(100, &[3, 17, 9], 3, 2);
    let h = w.honest(&job, 3);
    let hid = h.claim.id();
    w.block(101, vec![h.tx, T::PanelCovered { claim: hid }]);
    w.block(151, vec![]);
    w.block(352, vec![]);
    assert_eq!(
        settlements(&w, 101).into_iter().filter(|s| s.claim == Some(hid)).collect::<Vec<_>>(),
        vec![
            ix(PRODUCER, 1, K::ReleaseSealDeposit, hid),
            ix(PRODUCER, 1000, K::ReserveClaim, hid),
            // GAP-5: the Final reward is the poster's escrow, debited and routed to the producer — never issued.
            ix(common::chain::POSTER, 7, K::PayJobEscrow, hid),
            ix(PRODUCER, 7, K::FinalReward, hid),
            ix(PRODUCER, 1000, K::ReleaseClaim, hid)
        ]
    );
    // The defaulted claim's horizon (31 + 200) ended with no valid proof: its reservation is released, never slashed for withholding.
    assert!(settlements(&w, 352).contains(&ix(PRODUCER, 900, K::ReleaseClaim, id)));
    // A dismissed filing's fee is slashed from the accuser's free collateral and burned.
    let job = w.post_job(400, &[3, 17, 9], 3, 3);
    let h = w.honest(&job, 3);
    let hid = h.claim.id();
    w.block(401, vec![h.tx, T::PanelCovered { claim: hid }]);
    w.block(402, vec![T::FileProof { accuser: SPAM1, claim: hid, proof: ProsecutionV1::Kernel(vec![1]) }]);
    assert_eq!(settlements(&w, 402), vec![ix(SPAM1, 5, K::SlashFiling, hid), ix(SPAM1, 5, K::Burn, hid)]);
    // A withdrawal returns the collateral by instruction.
    w.block(500, vec![T::RequestExit { bond: SPAM2 }]);
    w.block(530, vec![T::Withdraw { bond: SPAM2 }]);
    assert_eq!(settlements(&w, 530), vec![S { bond: SPAM2, amount: 1000, kind: K::Withdraw, claim: None }]);
    assert!(!w.l.bonds.contains_key(&SPAM2));
}

#[test]
fn the_panels_coverage_is_derived_by_the_consumer_never_submitted_and_silence_never_passes() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    w.block(10, vec![h.tx]);
    // An unknown claim has no tally; a "not covered" tally changes nothing and the claim times out, never passing.
    w.l.begin_block(11).unwrap();
    assert!(w.l.apply_panel_tally(&[7; 64], true).is_err());
    let root = w.l.root();
    assert!(w.l.apply_panel_tally(&id, false).unwrap().is_empty());
    assert_eq!(w.l.root(), root);
    w.block(120, vec![]);
    assert!(matches!(w.state(&id), ClaimStateV1::TimedOut { .. }), "{:?}", w.state(&id));
    assert!(!w.l.claims[&id].rewarded);
    // A block older than the ledger's clock is refused and nothing changes.
    let root = w.l.root();
    let r = w.l.begin_block(5).unwrap_err();
    assert_eq!(r.kind, RefusalKindV1::Stale);
    assert_eq!(w.l.root(), root);
    assert_eq!(KERNEL_ROUTE_VERSION_V1, 1);
}

// ── R5: the claim's challenge subject comes from the single contract ─────────────────────────────────────────────────────

#[test]
fn a_claims_challenge_subject_feeds_the_single_contracts_seed_and_the_ledger_stores_no_beacon() {
    use misaka_palw_challenge::seed::SeedRefusalV1;
    use misaka_palw_challenge::{
        BeaconContextV1, RootV1, SubjectKindV1, WorkBeaconStateV1, WorkFinalEventV1, WorkSourceKindV1, challenge_seed_v1,
        collect_work_beacon_v1, reference_policy_v1,
    };

    let contract = reference_policy_v1(3, 2, 40, 5, 4);
    let mut p = policy();
    p.challenge_policy_id = contract.id();
    let mut w = World::with(p);
    let job1 = w.post_job(2, &[3, 17, 9], 3, 1);
    let job2 = w.post_job(3, &[3, 17, 9], 3, 2);
    let (h1, h2) = (w.honest(&job1, 3), w.honest(&job2, 3));
    let (id1, id2) = (h1.claim.id(), h2.claim.id());
    w.block(10, vec![h1.tx, h2.tx]);

    let s1 = w.l.claim_challenge_subject(&id1).unwrap();
    let s2 = w.l.claim_challenge_subject(&id2).unwrap();
    assert!(w.l.claim_challenge_subject(&[7; 64]).is_none());
    assert_eq!(s1.subject_kind, SubjectKindV1::ClaimVerification);
    assert_eq!((s1.chain_genesis, s1.ruleset_id, s1.challenge_policy_id), ([9; 64], [3; 64], contract.id()));
    assert_eq!(s1.subject_id, id1);
    // The roots the claim's class binds are present and equal; the claim's own roots differ; what does not apply is typed absent.
    let class = &w.l.classes[&w.class];
    let evidence = |id: &Digest| match &w.l.claims[id].body {
        misaka_palw_kernel::ledger::ClaimBodyV1::Program { evidence, .. } => evidence.clone(),
        _ => panic!(),
    };
    assert_eq!(s1.kernel_id, RootV1::Present(class.descriptor.digest()));
    assert_eq!(s1.verification_plan_root, RootV1::Present(class.plan.root()));
    assert_eq!(s1.program_root, RootV1::Present(misaka_palw_kernel::public::program_root_v1(&class.program_bytes)));
    assert_eq!(s1.artifact_root, RootV1::Present(class.param_commitments.root()));
    assert_eq!(s1.input_root, RootV1::Present(evidence(&id1).job_input_root));
    assert_eq!(
        s1.commitment_root,
        misaka_palw_kernel::claim_subject::claim_commitment_root_v1(&id1, &evidence(&id1).root()),
        "the claim and the evidence object the producer fixed before any challenge existed"
    );
    // The two claims commit byte-identical evidence (same prompt, same honest trace) under different jobs: their commitment roots
    // still differ, so no beacon context is shared between claims.
    assert_eq!(evidence(&id1).root(), evidence(&id2).root());
    assert!(matches!(s1.state_root, RootV1::Present(_)));
    assert_eq!((s1.tokenizer_or_schema_root, s1.layout_root, s1.constraint_root), (RootV1::Absent, RootV1::Absent, RootV1::Absent));
    assert_eq!(
        (s1.kernel_id, s1.verification_plan_root, s1.program_root, s1.artifact_root),
        (s2.kernel_id, s2.verification_plan_root, s2.program_root, s2.artifact_root)
    );
    assert_ne!(s1.subject_id, s2.subject_id);
    assert_ne!(s1.commitment_root, s2.commitment_root);
    assert_ne!(s1.id(), s2.id());
    // A pure function of ledger state: a fresh node derives the same subject.
    let fresh = KernelLedgerV1::replay(&w.genesis, &w.blocks);
    assert_eq!(fresh.claim_challenge_subject(&id1).unwrap(), s1);

    // The subject drives the contract's seed under a locked PALW Work Beacon of its own commitment; it is refused for another claim's
    // beacon context and for another policy. The ledger itself stores no beacon at all.
    let ctx = |s: &misaka_palw_challenge::ChallengeSubjectV1| BeaconContextV1 {
        chain_genesis: s.chain_genesis,
        ruleset_id: s.ruleset_id,
        policy: contract.clone(),
        subject_kind: s.subject_kind,
        commitment_root: s.commitment_root,
        commitment_position: 100,
        challenge_epoch: 7,
        eligible_profiles: [[0xA1; 64], [0xA2; 64]].into_iter().collect(),
        excluded_profiles: [[0xCA; 64]].into_iter().collect(),
        candidate_profile_id: misaka_palw_challenge::RootV1::Absent,
    };
    let work = |n: u8, profile: Digest, accepted: u64, settled: u64| WorkFinalEventV1 {
        kind: WorkSourceKindV1::RealUsefulWork,
        source_profile_id: profile,
        canonical_work_id: [n; 64],
        execution_commitment: [n ^ 0xFF; 64],
        accepted_position: accepted,
        settlement_position: settled,
        occurrence_index: 0,
        claim_final: true,
        da_satisfied: true,
        validity_independent: true,
        depends_on_profiles: vec![],
        final_path: misaka_palw_challenge::FinalPathV1::PanelLicensed { panel_seed_id: [0x5E; 64], panel_epoch: 1 },
    };
    let events = [work(1, [0xA1; 64], 103, 110), work(2, [0xA2; 64], 104, 111), work(3, [0xA1; 64], 105, 112)];
    let locked = |c: &BeaconContextV1| match collect_work_beacon_v1(c, &events, 117).unwrap() {
        WorkBeaconStateV1::Locked(b) => b,
        other => panic!("{other:?}"),
    };
    let (c1, c2) = (ctx(&s1), ctx(&s2));
    let (b1, b2) = (locked(&c1), locked(&c2));
    let seed1 = challenge_seed_v1(&c1, &s1, &b1).unwrap();
    let seed2 = challenge_seed_v1(&c2, &s2, &b2).unwrap();
    assert_ne!(seed1, seed2, "two claims never share a seed");
    assert_eq!(challenge_seed_v1(&c1, &s2, &b1), Err(SeedRefusalV1::SubjectMismatch), "another claim's subject under this beacon");
    let mut other_policy = s1.clone();
    other_policy.challenge_policy_id = [0x77; 64];
    assert_eq!(challenge_seed_v1(&c1, &other_policy, &b1), Err(SeedRefusalV1::PolicyMismatch));
    // The legacy per-claim beacon is gone: the public record the ledger assembles carries none, and an outsider's checks use its
    // own salt (the faults it finds are convictable whatever vectors found them).
    assert_eq!(w.l.public_record(&id1).unwrap().0.beacon, [0; 64]);
}

#[test]
fn aggregate_claim_work_stops_at_the_proof_reserve_and_a_fresh_outsider_still_convicts() {
    let probe = World::new();
    let row = &probe.l.classes[&probe.class];
    let cost = row.admission_work_v1().unwrap();
    let court = row.bounds.max_court_work;
    let mut p = policy();
    p.max_court_work_per_block = 2 * cost + court - 1;
    p.prosecution_reserve_permille = (court as u128 * 1000).div_ceil(p.max_court_work_per_block as u128) as u16;
    assert!(
        p.admission_work_limit_v1() >= cost && p.admission_work_limit_v1() < 2 * cost,
        "the test must reach work exhaustion before run exhaustion: cost {cost}, court {court}"
    );
    let mut w = World::with(p);
    let job_a = w.post_job(2, &[3, 17, 9], 3, 1);
    let job_b = w.post_job(2, &[3, 17, 9], 3, 2);
    let (_, lie) = w.lying(&job_a, 3);
    let id = lie.claim.id();
    let da = Da::publishing(&lie.trace, &[]);
    let honest = w.honest(&job_b, 3);
    let retry = honest.tx.clone();
    let ev = w.block(10, vec![lie.tx, honest.tx, T::PanelCovered { claim: id }]);
    assert!(w.l.claims.contains_key(&id));
    assert_eq!(ev.iter().filter(|e| matches!(e, E::ClaimCommitted { .. })).count(), 1);
    assert!(refused(&ev).is_some_and(|why| why.contains("budget")), "{ev:?}");
    assert_eq!(w.l.budget_used().court_work, cost);
    let used = w.l.budget_used();
    let (_, obj) = retry.clone().signed(PRODUCER);
    refused_with(&mut w, &obj, PRODUCER, RefusalKindV1::OverBudget);
    assert_eq!(w.l.budget_used(), used);
    let OutsiderFindingV1::Prosecute(proof) = outsider(&w, id, &da) else { panic!("no proof") };
    let ev = direct(&mut w, &O::FileProof { accuser: OUTSIDER, claim: id, proof }, OUTSIDER).unwrap();
    assert!(convicted(&ev).is_some(), "{ev:?}");
    assert_eq!(w.l.budget_used().court_work, cost + court);
    // The honest sealed claim retries next block; rejection did not consume its seal or collateral.
    let ev = w.block(11, vec![retry]);
    assert!(ev.iter().any(|e| matches!(e, E::ClaimCommitted { .. })), "{ev:?}");
}

#[test]
fn malformed_claims_spend_admission_work_but_class_oversize_is_refused_before_trace_copy() {
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    seal_first(&mut w, &h.claim);
    w.l.begin_block(3).unwrap();
    let (_, mut obj) = h.tx.signed(PRODUCER);
    let O::CommitClaim { commitments, .. } = &mut obj else { panic!() };
    commitments[0][0][0][0] ^= 1;
    let cost = w.l.classes[&w.class].admission_work_v1().unwrap();
    let r = refused_with(&mut w, &obj, PRODUCER, RefusalKindV1::Rule);
    assert!(r.why.contains("trace root"));
    assert_eq!(w.l.budget_used().court_work, cost);
    w.l.begin_block(4).unwrap();
    let cap = w.l.classes[&w.class].bounds.max_commit_bytes + misaka_palw_kernel::gate::CLAIM_CARRIER_OVERHEAD_BYTES_V1;
    let O::CommitClaim { commitments, .. } = &mut obj else { panic!() };
    *commitments = vec![vec![vec![[0; 64]; (cap / 64 + 1) as usize]]];
    assert!(obj.encoded_len() <= obj.max_encoded_bytes(), "test must reach the class ceiling");
    let r = refused_with(&mut w, &obj, PRODUCER, RefusalKindV1::Oversized);
    assert!(r.why.contains("class bound"));
    assert_eq!(w.l.budget_used(), Default::default());
}

#[test]
fn registration_refuses_an_unusable_admission_budget_and_work_addition_never_wraps() {
    let w = World::new();
    let row = &w.l.classes[&w.class];
    let cost = row.admission_work_v1().unwrap();
    let mut impossible = row.bounds;
    impossible.max_commit_bytes = u128::MAX;
    assert!(misaka_palw_kernel::gate::claim_admission_work_v1(&impossible, 0).is_err());
    impossible.max_commit_bytes = u64::MAX as u128;
    assert!(misaka_palw_kernel::gate::claim_admission_work_v1(&impossible, 0).is_err());
    let mut p = policy();
    p.max_court_work_per_block = row.bounds.max_court_work + cost;
    p.prosecution_reserve_permille = 999;
    assert!(p.admission_work_limit_v1() < cost, "fixture must require more than 0.1% of the block");
    let d = k2_tir_v1_descriptor();
    let mut l = KernelLedgerV1::genesis(p, active_for(&d), vec![d]).unwrap();
    l.sync_bond(PRODUCER, 5000);
    l.attest_artifact(row.param_commitments.root());
    let (_, obj) = w.register().signed(PRODUCER);
    let root = l.root();
    let r = l.apply_object(&obj, &auth(PRODUCER)).unwrap_err();
    assert!(r.why.contains("non-proof work budget"), "{r}");
    assert_eq!(l.root(), root);
    assert!(l.classes.is_empty());
    let mut p = policy();
    p.max_court_work_per_block = 2 * row.bounds.max_court_work.max(cost);
    p.prosecution_reserve_permille = 1;
    assert!(cost <= p.admission_work_limit_v1());
    let d = k2_tir_v1_descriptor();
    let mut l = KernelLedgerV1::genesis(p, active_for(&d), vec![d]).unwrap();
    l.sync_bond(PRODUCER, 5000);
    l.attest_artifact(row.param_commitments.root());
    let root = l.root();
    let r = l.apply_object(&obj, &auth(PRODUCER)).unwrap_err();
    assert!(r.why.contains("guaranteed proof work"), "{r}");
    assert_eq!(l.root(), root);
    assert!(l.classes.is_empty());
    // Reuse a live claim and exhaust u64 exactly; a positive court cannot fit via saturation.
    let mut w = World::new();
    let job = w.post_job(2, &[3, 17, 9], 3, 1);
    let h = w.honest(&job, 3);
    let id = h.claim.id();
    w.block(10, vec![h.tx]);
    w.l.restore_budget(misaka_palw_kernel::ledger::BlockBudgetV1 { adjudications: 0, court_work: u64::MAX });
    let used = w.l.budget_used();
    refused_with(
        &mut w,
        &O::FileProof { accuser: SPAM1, claim: id, proof: ProsecutionV1::Kernel(vec![1, 2, 3]) },
        SPAM1,
        RefusalKindV1::OverBudget,
    );
    assert_eq!(w.l.budget_used(), used);
    assert_eq!(w.l.policy.admission_work_limit_v1(), u64::MAX - u64::MAX / 2);
}
