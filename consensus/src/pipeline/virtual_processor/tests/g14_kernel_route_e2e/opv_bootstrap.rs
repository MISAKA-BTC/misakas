//! **OPV-BOOT on the real node: the startup cycle closed, from zero Finals** (`docs/design/palw/opv-beacon-bootstrap.md`).
//!
//! ```text
//! zero Finals ── bootstrap class B: 104 bind → kernel class → 106 (COMPLETE-CHECK policy) → 107 → 109 PostComplete
//!                   judged whole in the fold (every leaf re-rooted, the commitments recomputed, every input run): CONFORMANCE_PASSED
//!             ── B's OPV class is derived-eligible → registers → its claims reach OPV Final (PanelIndependent)
//!             ── candidate C (SAMPLED policy) commits: its sources are the eligible set (B) → the beacon locks on B's Finals
//!             ── C's evidence, window, CONFORMANCE_PASSED → C's OPV class is derived-eligible → registers → its claim reaches Final
//! ```
//!
//! The fences are test-armed through the harness's `Config` seam (validation still refuses them). **No eligibility hook**: every
//! class here is eligible only because `opv_eligibility_v1` derives it from the chain; the fixtures (seeds 31–45) are attested by no
//! test hook (`fixture()`'s seed 7 is). The drill's effective-bits floor is 0 (the interim sampled policy is 2 bits): what the floor
//! of 128 decides is asserted on the same chain state.

use super::super::g14_registration_e2e::{Tensors, layout_of};
use super::*;
use kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1;
use kaspa_consensus_core::palw_conformance_evidence_v1::{
    CheckOutcomeV1, ConformanceEvidenceActionV1, ConformanceEvidencePostV1, ConformanceScopeV1,
    PALW_CONFORMANCE_CHALLENGE_WINDOW_DAA_V1, PALW_CONFORMANCE_EVIDENCE_DEADLINE_DAA_V1, ResultV1, assemble_evidence_v1,
    derive_selection_v1, openings_root_v1, palw_onboarding_challenge_policy_v1, palw_onboarding_sealed_policy_v1,
    reference_leaf_result_v1,
};
use kaspa_consensus_core::palw_kernel_route_v1::{PALW_KERNEL_ROUTE_FIRST_AUX_TABLE_V1, PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1};
use kaspa_consensus_core::palw_onboarding_v1::{
    ArtifactBindingStateV1, AttemptBeaconV1, ConformanceAttemptEndV1, ConformanceAttemptRowV1,
};
use kaspa_consensus_core::palw_opv_bootstrap_v1::*;
use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_operands_v1;
use misaka_palw_challenge::{
    AttributedWorkV1, ConformanceCommitmentV1, FinalPathV1, OnboardingStateV1 as S, RootV1, SubjectKindV1, WorkBeaconStateV1,
    challenge_seed_v1, collect_attributed_work_beacon_v1,
};

const BOOT: usize = 1;
const CAND: usize = 3;
const OUTSIDER: usize = 7;
/// Producers of OPV claims, rotated (a producer holds at most three live OPV claims; the distinct rule wants distinct bonds).
const PRODUCERS: [usize; 5] = [0, 2, 4, 5, 6];

/// The onboarding network with the OPV fence (the given deny-list), armed WITHOUT its validation, on the DRILL terms: the floor at
/// `min_effective_bits` and a sampled conformance allowed to gate (GAP-70's switch, which validation refuses on any real network).
/// The bootstrap's mechanics — a sampled class becoming a source after the complete check — need both; what the release terms decide
/// is asserted where it differs ([`eligibility_release`]).
fn boot_config(min_effective_bits: u16, denied: Vec<Hash64>) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = kernel_config_onboarding();
    let mut params = config.params.clone();
    let mut fence = PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(1), denied);
    fence.min_effective_bits = min_effective_bits;
    fence.sampled_conformance_gates_reward = true;
    assert!(fence.validate_value().is_err(), "GAP-70's drill switch is refused by the value's own validation");
    params.palw_panel_free_v1 = Some(fence);
    assert!(params.validate_palw_v2().is_err(), "the real validation still refuses the fences");
    (Config::new(params), bundle, premine, floats)
}

fn fx_of(f: &OnbFixture) -> Fixture {
    Fixture { program: f.program.clone(), params: f.params.clone(), plan: f.plan.clone(), pc: f.pc.clone() }
}

fn ops_of(f: &OnbFixture) -> Vec<PalwArtifactOperandV1> {
    let tensors = f.params.tensors.iter().map(|(k, t)| (*k, t.to_le_bytes())).collect();
    palw_tir_inventory_operands_v1(&f.program, &Tensors(tensors)).expect("the inventory")
}

fn class_id(f: &OnbFixture, mode: VerificationModeV1) -> Digest {
    single_class_id_v1(k2_tir_v2_descriptor().digest(), &f.program.encode(), &f.plan, &f.pc, mode)
}

/// One onboarded class: its V2 fixture, the kernel class's weights (`kernel` — the V2 fixture's own, or another set of weights of
/// the same program for a FALSE binding), the registrant, and the ids.
struct Onb {
    f: OnbFixture,
    kernel: OnbFixture,
    card: usize,
    v2: Hash64,
    legacy: Digest,
    opv: Digest,
    complete: bool,
    /// Bound under the sealed-source (v3) policy instead of the sampled (v2) one.
    sealed: bool,
}

impl Onb {
    fn kfx(&self) -> Fixture {
        fx_of(&self.kernel)
    }

    fn facts(&self) -> OpvClassFactsV1 {
        OpvClassFactsV1::of_registration(
            k2_tir_v2_descriptor().digest(),
            &self.kernel.program.encode(),
            &self.kernel.plan,
            &self.kernel.pc,
        )
    }
}

struct Spec {
    v2: OnbFixture,
    kernel: OnbFixture,
    card: usize,
    complete: bool,
    sealed: bool,
}

impl Spec {
    fn honest(seed: u64, card: usize, complete: bool) -> Spec {
        Spec { v2: onb_fixture(seed), kernel: onb_fixture(seed), card, complete, sealed: false }
    }

    /// A candidate bound under the network's sealed-source (v3) policy.
    fn sealed(seed: u64, card: usize) -> Spec {
        Spec { sealed: true, ..Spec::honest(seed, card, false) }
    }
}

impl Net {
    fn kernel_bound_under(&mut self, card: usize, v2_class: Hash64, kernel_class: Hash64, policy: Hash64) -> Obj {
        let payload = borsh::to_vec(&(v2_class, kernel_class, policy)).unwrap();
        let signature = self.onboarding_signature(card, 106, &payload);
        Obj::KernelBoundV1 { v2_class, kernel_class, challenge_policy_id: policy, signer: self.bond(card), signature }
    }

    fn evidence(&mut self, card: usize, v2_class: Hash64, action: ConformanceEvidenceActionV1) -> Obj {
        let payload = borsh::to_vec(&(v2_class, &action)).unwrap();
        let signature = self.onboarding_signature(card, 109, &payload);
        Obj::ConformanceEvidenceV1 { v2_class, action: Box::new(action), signer: self.bond(card), signature }
    }

    fn attempt(&self, v2: Hash64) -> ConformanceAttemptRowV1 {
        self.api().expect("the route").conformance_attempt_v1(&v2).expect("a conformance record")
    }

    fn attributed_events(&self) -> Vec<AttributedWorkV1> {
        self.api()
            .expect("the route")
            .finals_read_v1()
            .expect("the rows rebuild")
            .into_iter()
            .filter_map(|f| f.event.map(|e| borsh::from_slice(&e).expect("an attributed beacon event")))
            .collect()
    }

    fn budget(&self) -> (u64, u32, u64) {
        self.api()
            .and_then(|r| r.aux.get(&(PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, Vec::new())).cloned())
            .map(|b| borsh::from_slice(&b).expect("a budget row"))
            .unwrap_or_default()
    }
}

/// **Onboard every spec in parallel**: V2 registrations (one per block: `PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1`), artifact bindings
/// (104), the binding windows, legacy kernel classes over the bound roots, and 106 under each spec's policy (the complete check, or
/// the sampled policy).
async fn onboard_all(net: &mut Net, specs: Vec<Spec>) -> Vec<Onb> {
    let mut v2s = Vec::new();
    for s in &specs {
        let o = net.v2_registration(&s.v2, s.card, net.daa() + 30);
        let Obj::ClassRegisteredTirV1 { class_id, .. } = &o else { unreachable!() };
        let class_id = *class_id;
        v2s.push(class_id);
        net.send(vec![(s.card, o)]).await;
        assert!(net.chain.tip_state().1.class(&class_id).is_some(), "the V2 class registered");
    }
    let mut items = Vec::new();
    for (s, v2) in specs.iter().zip(&v2s) {
        items.push((s.card, net.artifact_bound(s.card, *v2, Hash64::from_bytes(s.kernel.pc.root()))));
    }
    net.send(items).await;
    let matures = specs
        .iter()
        .zip(&v2s)
        .map(|(s, v2)| net.api().unwrap().artifact_binding_v1(v2, &Hash64::from_bytes(s.kernel.pc.root())).unwrap().matures_daa)
        .max()
        .unwrap();
    net.beat_to(matures).await;
    let d = k2_tir_v2_descriptor();
    let mut items = Vec::new();
    for s in &specs {
        let register = K::RegisterClass {
            descriptor: d.digest(),
            program_bytes: s.kernel.program.encode(),
            plan: s.kernel.plan.clone(),
            param_commitments: s.kernel.pc.clone(),
        };
        items.push((s.card, net.route(s.card, &register)));
    }
    // A kernel registration spends one of the block's NON-proof adjudication runs: the test node's four, less the half reserved for
    // proofs (C4 F-C4R3-05) — two per block. A third in the same block is refused over budget, so they go two blocks at a time.
    for wave in items.chunks(2) {
        net.send(wave.to_vec()).await;
    }
    let (complete, sampled, sealed) = (
        palw_onboarding_complete_check_policy_v1().id(),
        palw_onboarding_challenge_policy_v1().id(),
        palw_onboarding_sealed_policy_v1().id(),
    );
    let mut items = Vec::new();
    let mut out = Vec::new();
    for (s, v2) in specs.into_iter().zip(v2s) {
        let legacy = class_id(&s.kernel, VerificationModeV1::PanelLicensed);
        let opv = class_id(&s.kernel, VerificationModeV1::OptimisticPublicVerification);
        assert!(net.ledger().classes.contains_key(&legacy), "the legacy kernel class registered over the matured binding");
        let policy = Hash64::from_bytes(if s.complete {
            complete
        } else if s.sealed {
            sealed
        } else {
            sampled
        });
        items.push((s.card, net.kernel_bound_under(s.card, v2, Hash64::from_bytes(legacy), policy)));
        out.push(Onb { f: s.v2, kernel: s.kernel, card: s.card, v2, legacy, opv, complete: s.complete, sealed: s.sealed });
    }
    net.send(items).await;
    for o in &out {
        assert!(net.api().unwrap().kernel_binding_v1(&o.v2).is_some(), "kernel-bound");
    }
    out
}

/// The committed sampled scope of the drill (as the OB-P0 tests): two vectors, two leaves, the densest fault — 2 bits.
fn scope() -> ConformanceScopeV1 {
    let mut s = ConformanceScopeV1::new(2, 3, 2, 2);
    (s.vector_fault_ppm, s.leaf_fault_ppm) = (1_000_000, 1_000_000);
    s
}

fn commitment(net: &Net, o: &Onb, implementation: u8) -> ConformanceCommitmentV1 {
    let route = net.api().expect("the route");
    let binding = route.kernel_binding_v1(&o.v2).expect("kernel-bound");
    ConformanceCommitmentV1 {
        version: 1,
        chain_genesis: net.config.params.genesis.hash.as_bytes(),
        ruleset_id: route.header.policy.ruleset_digest,
        subject_kind: SubjectKindV1::ModelConformance,
        candidate_id: o.v2.as_bytes(),
        kernel_descriptor_id: k2_tir_v2_descriptor().digest(),
        challenge_policy_id: binding.challenge_policy_id.as_bytes(),
        artifact_root: o.f.artifact_root.as_bytes(),
        program_root: program_root_v1(&o.f.program.encode()),
        source_root: RootV1::Absent,
        tokenizer_or_input_schema_root: RootV1::Absent,
        layout_root: [0x11; 64],
        verification_plan_root: o.f.plan.root(),
        constraint_root: RootV1::Absent,
        implementation_set_root: [implementation; 64],
        test_scope_root: if o.complete { [0x33; 64] } else { scope().root() },
        calibration_id: RootV1::Absent,
        input_and_state_binding_root: RootV1::Absent,
        resource_profile_id: [0x44; 64],
        commitment_object_id: None,
        canonical_commitment_position: None,
    }
}

async fn commit(net: &mut Net, o: &Onb, implementation: u8) {
    let c = commitment(net, o, implementation);
    let obj = net.conformance_committed(o.card, c.clone());
    net.send(vec![(o.card, obj)]).await;
    let a = net.attempt(o.v2);
    assert!(a.open() && a.commitment == c, "the commitment opened an attempt");
    assert_eq!(a.is_complete_check(), o.complete);
}

/// The honest complete check of `o`'s current attempt (every implementation agrees with the reference).
fn complete_post(net: &Net, o: &Onb) -> CompleteCheckPostV1 {
    let domain = palw_complete_check_domain_v1(&o.f.program, o.f.plan.max_positions).expect("a bootstrap class qualifies");
    complete_check_post_v1(&o.f.program, &domain, net.attempt(o.v2).commitment.statement_root(), ops_of(&o.f)).expect("the reference")
}

/// What a pack computes for `o`'s current SAMPLED attempt: the seed from the chain's own attributed Finals, every selected check run
/// (vectors by the bound kernel class's greedy run, leaves from the V2 artifact).
fn sampled_post(net: &Net, o: &Onb) -> ConformanceEvidencePostV1 {
    let attempt = net.attempt(o.v2);
    let policy = attempt.policy();
    let ctx = attempt.beacon_context(&policy);
    let AttemptBeaconV1::Locked(beacon) = net.api().expect("the route").attempt_beacon_v1(&attempt, net.daa()).unwrap() else {
        panic!("the beacon is locked")
    };
    let seed = challenge_seed_v1(&ctx, &attempt.commitment.subject(), &beacon).expect("a seed");
    let selection = derive_selection_v1(&seed, &policy, &scope(), &o.f.program).expect("a selection");
    let ledger = net.ledger();
    let ops = ops_of(&o.f);
    let mut outcomes = Vec::new();
    for v in &selection.vectors {
        let tokens = greedy(&o.kfx(), &ledger, &o.legacy, &v.prompt, v.decode as usize);
        let a = misaka_palw_kernel::hash::object_id(b"test/logits-digest-stand-in", &tokens);
        let mut a32 = [0u8; 32];
        a32.copy_from_slice(&a[..32]);
        let ran = ResultV1::Ran { a: a32, b: [7; 32], tokens, positions: v.prompt.len() as u32 + v.decode };
        outcomes.push(CheckOutcomeV1 {
            check_id: v.check_id.clone(),
            reference: ran.clone(),
            independent: ran.clone(),
            backend: ran,
            disagreement: None,
        });
    }
    for l in &selection.leaves {
        let bytes = &ops[l.leaf_index as usize].bytes;
        let a = reference_leaf_result_v1(&o.f.program, l.param, bytes).expect("the leaf decodes");
        let w = o.f.program.params[l.param as usize].dtype.width();
        let ran = ResultV1::Ran { a, b: [0; 32], tokens: vec![], positions: (bytes.len() / w) as u32 };
        outcomes.push(CheckOutcomeV1 {
            check_id: l.check_id.clone(),
            reference: ran.clone(),
            independent: ran.clone(),
            backend: ran,
            disagreement: None,
        });
    }
    let map = outcomes.iter().map(|x| (x.check_id.clone(), x.clone())).collect();
    let evidence =
        assemble_evidence_v1(&attempt.commitment, &policy, &scope(), &beacon, &seed, &selection, &openings_root_v1(&None), &map);
    ConformanceEvidencePostV1 { evidence, scope: scope(), outcomes }
}

/// The capture-proof chunk lane's target of `v2`'s conformance evidence at the tip (`palw_conformance_chunk_target_v1`).
fn chunk_target(net: &Net, v2: Hash64) -> Option<u64> {
    kaspa_consensus_core::palw_onboarding_v1::palw_conformance_chunk_target_v1(&net.api().expect("the route"), &v2, net.daa())
}

/// The derived eligibility of `o`'s OPV class at the tip (nothing denied, no hook) under the given terms.
fn eligibility_under(net: &Net, o: &Onb, floor: u16, sampled_gates_reward: bool) -> Result<OpvEligibleV1, OpvIneligibleV1> {
    let route = net.api().expect("the route");
    let ledger = route.ledger().expect("the rows rebuild");
    let policy = route.header.opv.expect("the network declares OPV");
    let view =
        OpvEligibilityViewV1 { policy: &policy, denied: &[], min_effective_bits: floor, sampled_gates_reward, test_eligible: &[] };
    route.opv_eligibility_v1(&ledger, &o.facts(), net.daa(), &view)
}

/// ... on the DRILL terms (floor 0, a sampled conformance may gate).
fn eligibility_drill(net: &Net, o: &Onb) -> Result<OpvEligibleV1, OpvIneligibleV1> {
    eligibility_under(net, o, 0, true)
}

/// ... on the RELEASE terms: the ruled floor of 128 effective bits, and only the complete check gates rewards (GAP-70).
fn eligibility_release(net: &Net, o: &Onb) -> Result<OpvEligibleV1, OpvIneligibleV1> {
    eligibility_under(net, o, 128, false)
}

/// Card `card` registers `o`'s kernel class under OPV (tag 13).
async fn register_opv(net: &mut Net, card: usize, o: &Onb) -> bool {
    let register = K::RegisterClassV2 {
        mode: VerificationModeV1::OptimisticPublicVerification,
        descriptor: k2_tir_v2_descriptor().digest(),
        program_bytes: o.kernel.program.encode(),
        plan: o.kernel.plan.clone(),
        param_commitments: o.kernel.pc.clone(),
    };
    let obj = net.route(card, &register);
    net.send(vec![(card, obj)]).await;
    net.ledger().opv.classes.contains(&o.opv)
}

/// `n` jobs on `class`, posted by card `card`.
async fn post_jobs(net: &mut Net, card: usize, class: Digest, n: u8, nonce: u8) -> Vec<KernelJobV1> {
    let jobs: Vec<KernelJobV1> = (0..n)
        .map(|i| KernelJobV1 {
            class_binding_id: class,
            prompt: vec![3, 17, 9],
            max_new_tokens: 3,
            decode: DecodeRuleV1::Greedy,
            nonce: [nonce.wrapping_add(i); 64],
        })
        .collect();
    let posts: Vec<(usize, Obj)> = jobs.iter().map(|j| (card, net.route(card, &K::PostJob { job: j.clone() }))).collect();
    net.send(posts).await;
    for j in &jobs {
        assert!(net.ledger().jobs.contains_key(&j.id()), "the job posted");
    }
    jobs
}

/// Honest claims of `jobs` of `class` (weights `fx`) by `producers` (one each): sealed in one block, revealed in the next. Returns the
/// claim ids (committed or not — the caller asserts).
async fn claims(net: &mut Net, fx: &Fixture, class: Digest, jobs: &[KernelJobV1], producers: &[usize]) -> Vec<Digest> {
    let ledger = net.ledger();
    let (mut seals, mut reveals, mut ids) = (Vec::new(), Vec::new(), Vec::new());
    for (job, producer) in jobs.iter().zip(producers) {
        let generated = greedy(fx, &ledger, &class, &job.prompt, job.max_new_tokens as usize);
        let produced = produce(fx, &ledger, &class, job, net.kid(*producer), generated, |_| {});
        let id = produced.claim.id();
        // Past `palw_panel_free_v1` a claim opens only its SALTED seal (G14-R4's claim seal v2, inner kind 20).
        let (seal, reveal) = seal_and_reveal(&ledger, net.kid(*producer), &produced.object);
        seals.push((*producer, net.route(*producer, &seal)));
        reveals.push((*producer, reveal));
        ids.push(id);
    }
    net.send(seals).await;
    let reveals: Vec<(usize, Obj)> = reveals.into_iter().map(|(p, o)| (p, net.route(p, &o))).collect();
    net.send(reveals).await;
    ids
}

/// Beat until `o`'s sampled attempt's beacon is locked at the sink; returns the lock position.
async fn until_locked(net: &mut Net, o: &Onb) -> u64 {
    let ttpb = net.ttpb();
    let policy = palw_onboarding_challenge_policy_v1();
    for _ in 0..400 {
        let state = collect_attributed_work_beacon_v1(&net.attempt(o.v2).beacon_context(&policy), &net.attributed_events(), net.daa());
        if let Ok(WorkBeaconStateV1::Locked(b)) = state {
            return b.lock_position;
        }
        assert!(!matches!(state, Ok(WorkBeaconStateV1::Unavailable { .. })), "the beacon must not run out of window here: {state:?}");
        net.chain.heartbeat(ttpb, Vec::new()).await;
    }
    panic!("the beacon never locked")
}

async fn until_final(net: &mut Net, claim: &Digest) -> u64 {
    let ttpb = net.ttpb();
    for _ in 0..200 {
        if let ClaimStateV1::Final { final_daa } = net.claim_state(claim) {
            return final_daa;
        }
        net.chain.heartbeat(ttpb, Vec::new()).await;
    }
    panic!("the claim never reached Final: {:?}", net.claim_state(claim))
}

// ---- the proof ---------------------------------------------------------------------------------------------------------------

/// **The whole bootstrap, from zero Finals, with no hook.** Nothing is eligible at the start and an OPV registration is refused.
/// The bootstrap class B passes a COMPLETE check judged in the fold (no seed, no beacon, no window) and its OPV class becomes eligible
/// by the derived rule (under the drill floor AND under 128: a complete check meets any floor); it registers and its claims reach OPV
/// Final. The candidate C commits under the SAMPLED policy: its frozen sources are exactly B's OPV class; B's Finals (distinct
/// producers) lock the beacon; C's evidence passes its window; C's OPV class becomes eligible by the derived rule (under the drill
/// floor — and NOT under the interim 128, which only a complete check can meet with a 2-bit scope); it registers and its own claim
/// reaches OPV Final, a `PanelIndependent` beacon fact attributed to its producer. A second node replays to every root.
#[tokio::test]
async fn g14_opv_bootstrap_from_zero_finals_a_complete_check_seeds_the_beacon_and_a_sampled_class_becomes_eligible() {
    kaspa_core::log::try_init_logger("warn");
    let mut net = Net::over_cfg(boot_config(0, Vec::new()), TestConsensus::new);
    net.beat_to(1).await;
    let mut onbs = onboard_all(&mut net, vec![Spec::honest(31, BOOT, true), Spec::honest(32, CAND, false)]).await;
    let c = onbs.pop().unwrap();
    let b = onbs.pop().unwrap();
    assert!(net.api().unwrap().finals_read_v1().unwrap().is_empty(), "zero Finals");
    assert_eq!(eligibility_drill(&net, &b), Err(OpvIneligibleV1::ConformanceNotPassed), "nothing is eligible before a conformance");
    assert!(!register_opv(&mut net, BOOT, &b).await, "an OPV registration before eligibility is refused (no registrant choice)");

    // ---- B: the complete check, judged in the fold ----
    commit(&mut net, &b, 0x22).await;
    let attempt = net.attempt(b.v2);
    assert!(attempt.eligible_profiles.is_empty(), "a complete check freezes no sources: it draws no beacon");
    assert_eq!(net.chain.ctx.consensus.palw_conformance_evidence_v1(b.v2).unwrap().beacon, "COMPLETE_CHECK");
    assert_eq!(chunk_target(&net, b.v2), None, "a complete check rides one carrier: no chunk lane target");
    let collateral = net.collateral(BOOT);
    let post = complete_post(&net, &b);
    let obj = net.evidence(BOOT, b.v2, ConformanceEvidenceActionV1::PostComplete(Box::new(post.clone())));
    net.send(vec![(BOOT, obj)]).await;
    let a = net.attempt(b.v2);
    assert_eq!(a.record.state, S::G14Eligible, "CONFORMANCE_PASSED at once, then the public-prosecution step");
    assert_eq!(a.record.conformance_evidence_id, Some(post.id()));
    assert_eq!(net.collateral(BOOT), collateral - PALW_COMPLETE_CHECK_FEE_SOMPI_V1, "the complete check's fee is burned");
    let domain = palw_complete_check_domain_v1(&b.f.program, b.f.plan.max_positions).unwrap();
    assert!(net.budget().2 >= domain.work, "its work was charged to the block's adjudication budget");
    assert_eq!(eligibility_drill(&net, &b), Ok(OpvEligibleV1::Derived { v2_class: b.v2 }));
    assert_eq!(
        eligibility_release(&net, &b),
        Ok(OpvEligibleV1::Derived { v2_class: b.v2 }),
        "a complete check meets the release terms: any floor, and it is the one conformance that gates rewards (GAP-70)"
    );
    assert!(register_opv(&mut net, BOOT, &b).await, "B's OPV class registers: derived-eligible");

    // ---- C: the sampled commitment freezes the eligible set as its sources ----
    commit(&mut net, &c, 0x22).await;
    let attempt = net.attempt(c.v2);
    assert_eq!(attempt.eligible_profiles, vec![Hash64::from_bytes(b.opv)], "the only eligible class: the bootstrap's");
    for id in [c.v2, Hash64::from_bytes(c.legacy), Hash64::from_bytes(c.opv)] {
        assert!(attempt.excluded_profiles.contains(&id), "the candidate under every mode is excluded");
    }
    assert_eq!(eligibility_drill(&net, &c), Err(OpvIneligibleV1::ConformanceNotPassed));
    let p = palw_onboarding_challenge_policy_v1();
    assert_eq!(
        chunk_target(&net, c.v2),
        Some(
            attempt.committed_daa + p.anchor_delay_slots + p.beacon_window_slots - 1
                + p.settlement_depth_d
                + PALW_CONFORMANCE_EVIDENCE_DEADLINE_DAA_V1
        ),
        "before the lock: the latest lock the window allows, plus the evidence deadline"
    );

    // ---- B's OPV claims reach Final with no Panel and lock C's beacon ----
    let jobs = post_jobs(&mut net, BOOT, b.opv, 2, 0x10).await;
    let ids = claims(&mut net, &b.kfx(), b.opv, &jobs, &PRODUCERS[..2]).await;
    for id in &ids {
        assert!(net.ledger().claims.contains_key(id), "an eligible OPV class's claim commits");
    }
    let lock = until_locked(&mut net, &c).await;
    let sources: Vec<AttributedWorkV1> = net.attributed_events();
    assert_eq!(sources.len(), 2, "the beacon's two sources: the bootstrap's Finals");
    assert!(sources.iter().all(|w| w.event.source_profile_id == b.opv && w.event.final_path == FinalPathV1::PanelIndependent));
    assert_ne!(sources[0].attribution.producer_id, sources[1].attribution.producer_id, "distinct producers (the distinct rule)");
    assert!(lock > attempt.committed_daa);
    assert_eq!(chunk_target(&net, c.v2), Some(lock + PALW_CONFORMANCE_EVIDENCE_DEADLINE_DAA_V1), "a Post: lock + deadline");

    // ---- C's evidence, window, pass ----
    let post = sampled_post(&net, &c);
    let obj = net.evidence(CAND, c.v2, ConformanceEvidenceActionV1::Post(Box::new(post)));
    net.send(vec![(CAND, obj)]).await;
    let posted = net.attempt(c.v2).evidence.expect("the fold accepted the evidence");
    assert_eq!(posted.window_end_daa, posted.posted_daa + PALW_CONFORMANCE_CHALLENGE_WINDOW_DAA_V1);
    assert_eq!(chunk_target(&net, c.v2), Some(posted.window_end_daa - 1), "a Refute: the window's last DAA");
    net.beat_to(posted.window_end_daa + 1).await;
    assert_eq!(chunk_target(&net, c.v2), None, "a closed attempt takes no part");
    assert_eq!(net.attempt(c.v2).record.state, S::G14Eligible, "C passed with the bootstrap's beacon");

    // ---- C is eligible by the derived rule (the drill floor), not under the ruled 128 ----
    assert_eq!(eligibility_drill(&net, &c), Ok(OpvEligibleV1::Derived { v2_class: c.v2 }));
    assert!(
        matches!(eligibility_release(&net, &c), Err(OpvIneligibleV1::PolicyNotVerified(_))),
        "a 2-bit sampled scope against the last contributor's grinding is 0 effective bits: below the ruled floor"
    );
    assert_eq!(
        attempt_effective_bits_v1(&net.api().unwrap(), &c.v2, &net.attempt(c.v2)),
        misaka_palw_challenge::EffectiveBitsV1::Bits(0)
    );
    assert!(register_opv(&mut net, CAND, &c).await, "C's OPV class registers: derived-eligible");

    // ---- C's own claim reaches OPV Final ----
    let jobs = post_jobs(&mut net, CAND, c.opv, 1, 0x40).await;
    let id = claims(&mut net, &c.kfx(), c.opv, &jobs, &PRODUCERS[2..3]).await[0];
    assert!(net.ledger().claims.contains_key(&id), "C's claim commits");
    until_final(&mut net, &id).await;
    let mine: Vec<AttributedWorkV1> = net.attributed_events().into_iter().filter(|w| w.event.source_profile_id == c.opv).collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].event.final_path, FinalPathV1::PanelIndependent);
    assert_eq!(mine[0].attribution.producer_id, net.kid(PRODUCERS[2]));
    // Every class with an onboarding passes the derived rule now, and the network's eligible set says so.
    let route = net.api().unwrap();
    let ledger = route.ledger().unwrap();
    let policy = route.header.opv.unwrap();
    let view =
        OpvEligibilityViewV1 { policy: &policy, denied: &[], min_effective_bits: 0, sampled_gates_reward: true, test_eligible: &[] };
    let mut want = vec![Hash64::from_bytes(b.opv), Hash64::from_bytes(c.opv)];
    want.sort();
    assert_eq!(route.opv_eligible_set_v1(&ledger, net.daa(), &view), want);
    let z = net.replay().await;
    net.assert_same(&z, "replay");
}

// ---- negatives -----------------------------------------------------------------------------------------------------------------

/// **No bootstrap class: the beacon never comes, and the chain lives.** A sampled candidate alone commits: its frozen source set is
/// empty, the collection window closes BEACON_UNAVAILABLE (counted), a second commitment again freezes nothing, its OPV class is
/// never eligible and its registration is refused — while blocks keep coming and the state keeps moving.
#[tokio::test]
async fn g14_opv_bootstrap_without_a_complete_check_class_the_beacon_never_comes_and_the_chain_lives() {
    kaspa_core::log::try_init_logger("warn");
    let mut net = Net::over_cfg(boot_config(0, Vec::new()), TestConsensus::new);
    net.beat_to(1).await;
    let c = onboard_all(&mut net, vec![Spec::honest(34, CAND, false)]).await.pop().unwrap();
    commit(&mut net, &c, 0x22).await;
    let attempt = net.attempt(c.v2);
    assert!(attempt.eligible_profiles.is_empty(), "no class is eligible: no source can ever qualify");
    let start = net.daa();
    let root = net.chain.tip_state().1.state_root();
    let policy = palw_onboarding_challenge_policy_v1();
    net.beat_to(attempt.committed_daa + policy.anchor_delay_slots + policy.beacon_window_slots + 1).await;
    let a = net.attempt(c.v2);
    assert_eq!((a.record.state, a.record.attempts()), (S::RegisteredDormant, 1), "BEACON_UNAVAILABLE, counted");
    assert!(matches!(a.last_end, Some((ConformanceAttemptEndV1::BeaconUnavailable, _))));
    commit(&mut net, &c, 0x23).await;
    assert!(net.attempt(c.v2).eligible_profiles.is_empty(), "and again: the fixed point from genesis is empty");
    assert_eq!(eligibility_drill(&net, &c), Err(OpvIneligibleV1::ConformanceNotPassed));
    assert!(!register_opv(&mut net, CAND, &c).await, "never eligible: the OPV registration is refused");
    // The main chain lives: blocks keep coming, the PALW state keeps moving, and no Final exists anywhere.
    assert!(net.daa() > start + policy.beacon_window_slots);
    assert_ne!(net.chain.tip_state().1.state_root(), root);
    assert!(net.api().unwrap().finals_read_v1().unwrap().is_empty());
    let z = net.replay().await;
    net.assert_same(&z, "replay");
}

/// A wide128-shaped class whose vocabulary is too large to check whole (1,100 single-token inputs > 1,024): stateless, but its
/// domain is past the complete check's bound — its conformance would need sampling, so it needs the beacon.
fn wide_vocab_fixture(v: u32, seed: u64) -> OnbFixture {
    use kaspa_consensus_core::palw_artifact::{artifact_leaf_v1, artifact_root_v1};
    use kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1;
    use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_TOKEN, Ref};
    use misaka_palw_tir::{DType, Rounding, TensorType};
    let d = 16u32;
    let mut pb = ProgramBuilder::new(v, HISTORY_BOUND_V1_SMALL);
    let tok = pb.param("tok_embd", DType::I8, &[v, d], false);
    let w64 = pb.param("wide.w64", DType::I64, &[d, d], true);
    let lm = pb.param("output.w", DType::I8, &[v, d], false);
    let carry = vec![TensorType::fixed(DType::I32, &[d])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let row = b.gather(tok, Ref::Input(INPUT_TOKEN), 0, 0);
        let x = b.cast(row, DType::I32);
        b.finish(&[x])
    };
    let wide = {
        let mut b = pb.block("wide", carry.clone());
        let xc = b.reshape_fixed(Ref::CarryIn(0), &[d, 1]);
        let acc = b.matmul(w64, xc, DType::I128);
        let sh = b.shr(acc, 70, Rounding::HalfAwayFromZero, DType::I128);
        let c = b.clamp(sh, -(1 << 20), 1 << 20, DType::I32);
        let c = b.reshape_fixed(c, &[d]);
        b.finish(&[c])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let h = b.clamp(Ref::CarryIn(0), -32767, 32767, DType::I16);
        let hc = b.reshape_fixed(h, &[d, 1]);
        let l = b.matmul(lm, hc, DType::I32);
        let l = b.reshape_fixed(l, &[v]);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    let mut program = pb.finish(pre, vec![wide], post, logits);
    program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut next = |lo: i128, hi: i128| {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        lo + (x as i128).rem_euclid(hi - lo + 1)
    };
    let mut tensors = BTreeMap::new();
    for (j, decl) in program.params.iter().enumerate() {
        let (lo, hi) = if decl.dtype == DType::I8 { (-128, 127) } else { (-(1i128 << 62), 1i128 << 62) };
        let n: usize = decl.shape.iter().map(|s| *s as usize).product();
        let layer = if decl.per_layer { Some(0u16) } else { None };
        let data = (0..n).map(|_| next(lo, hi)).collect();
        tensors.insert((j as u16, layer), Tensor::new(decl.dtype, decl.shape.iter().map(|s| *s as usize).collect(), data).unwrap());
    }
    let params = MapParams { tensors };
    let dsc = k2_tir_v2_descriptor();
    let plan = plan_for_tir_program_v1(&dsc, &program, program_root_v1(&program.encode()), MAX_POSITIONS).expect("a K2-TIR-v2 plan");
    let pc = ParamCommitmentsV1::of(&params);
    let mut class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1 {
            version: kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1,
            max_context: MAX_POSITIONS,
            checkpoint_interval: 2,
            h_tile: 2,
            commit_tiles: Vec::new(),
            state_tiles: Vec::new(),
        },
        tokenizer_id: Hash64::from_bytes([0x70; 64]),
    };
    class.layout = layout_of(&class, MAX_POSITIONS);
    let ops = palw_tir_inventory_operands_v1(&program, &Tensors(params.tensors.iter().map(|(k, t)| (*k, t.to_le_bytes())).collect()))
        .expect("the inventory");
    let artifact_root = artifact_root_v1(&ops.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).expect("a root");
    OnbFixture { program, params, plan, pc, class, artifact_root }
}

/// **A "bootstrap" whose activation would need the beacon is refused.** A stateless class with 1,100 single-token inputs is past the
/// complete check's bound: tag 106 under the complete-check policy is refused (rows untouched) — it must take the sampled policy,
/// which it can (and which then waits for a beacon). A PostComplete for a sampled attempt is refused too: no class skips the beacon
/// by calling its sampled conformance complete.
#[tokio::test]
async fn g14_opv_bootstrap_a_class_that_cannot_be_checked_whole_is_refused_the_complete_check() {
    kaspa_core::log::try_init_logger("warn");
    let big = wide_vocab_fixture(1_100, 35);
    assert!(palw_complete_check_domain_v1(&big.program, big.plan.max_positions).unwrap_err().contains("inputs"));
    let mut net = Net::over_cfg(boot_config(0, Vec::new()), TestConsensus::new);
    net.beat_to(1).await;
    // `onboard_all` asserts the binding; 106 is done by hand here.
    let o = net.v2_registration(&big, CAND, net.daa() + 30);
    let Obj::ClassRegisteredTirV1 { class_id: v2, .. } = &o else { unreachable!() };
    let v2 = *v2;
    net.send(vec![(CAND, o)]).await;
    let root = Hash64::from_bytes(big.pc.root());
    let o = net.artifact_bound(CAND, v2, root);
    net.send(vec![(CAND, o)]).await;
    net.beat_to(net.api().unwrap().artifact_binding_v1(&v2, &root).unwrap().matures_daa).await;
    let register = K::RegisterClass {
        descriptor: k2_tir_v2_descriptor().digest(),
        program_bytes: big.program.encode(),
        plan: big.plan.clone(),
        param_commitments: big.pc.clone(),
    };
    let o = net.route(CAND, &register);
    net.send(vec![(CAND, o)]).await;
    let legacy = class_id(&big, VerificationModeV1::PanelLicensed);
    assert!(net.ledger().classes.contains_key(&legacy), "the kernel class registered");
    let rows = net.api().unwrap().aux.clone();
    let o = net.kernel_bound_under(
        CAND,
        v2,
        Hash64::from_bytes(legacy),
        Hash64::from_bytes(palw_onboarding_complete_check_policy_v1().id()),
    );
    net.send(vec![(CAND, o)]).await;
    assert_eq!(net.api().unwrap().aux, rows, "the complete-check policy is refused: the class cannot be checked whole");
    let o =
        net.kernel_bound_under(CAND, v2, Hash64::from_bytes(legacy), Hash64::from_bytes(palw_onboarding_challenge_policy_v1().id()));
    net.send(vec![(CAND, o)]).await;
    assert!(net.api().unwrap().kernel_binding_v1(&v2).is_some(), "the sampled policy is accepted: it will need the beacon");
    let onb = Onb {
        f: big.clone_onb(),
        kernel: big.clone_onb(),
        card: CAND,
        v2,
        legacy,
        opv: class_id(&big, VerificationModeV1::OptimisticPublicVerification),
        complete: false,
        sealed: false,
    };
    commit(&mut net, &onb, 0x22).await;
    let rows = net.api().unwrap().aux.clone();
    let fake = CompleteCheckPostV1 {
        version: 1,
        commitment_root: net.attempt(v2).commitment.statement_root(),
        operands: Vec::new(),
        inputs: CompleteRootsV1 { reference: [0; 64], independent: [0; 64], backend: [0; 64] },
        leaves: CompleteRootsV1 { reference: [0; 64], independent: [0; 64], backend: [0; 64] },
    };
    let o = net.evidence(CAND, v2, ConformanceEvidenceActionV1::PostComplete(Box::new(fake)));
    net.send(vec![(CAND, o)]).await;
    assert_eq!(net.api().unwrap().aux, rows, "a PostComplete for a sampled attempt is dropped: nothing written, nothing charged");
    assert!(net.attempt(v2).open() && net.attempt(v2).evidence.is_none());
}

impl OnbFixture {
    fn clone_onb(&self) -> OnbFixture {
        OnbFixture {
            program: self.program.clone(),
            params: self.params.clone(),
            plan: self.plan.clone(),
            pc: self.pc.clone(),
            class: self.class.clone(),
            artifact_root: self.artifact_root,
        }
    }
}

/// **A block filled with hostile complete checks.** Three bootstrap classes have open complete-check attempts; in ONE block their
/// registrants post a junk result list (fails late, after the whole forward work), a junk inventory (fails early) and an honest check,
/// and an outsider posts a check for a class it does not register. The outsider's is dropped free; the block judges exactly
/// [`PALW_COMPLETE_CHECKS_PER_BLOCK_V1`] = 2 — each charged its class's full work before reading the post and each burning the fee —
/// and the third waits, uncharged, for a later block. Nothing panics; the failures are counted attempts; the waiting check passes in
/// the next block; a re-commitment after a failure passes honestly; a second node replays to every root.
#[tokio::test]
async fn g14_opv_bootstrap_a_block_of_hostile_complete_checks_spends_budget_and_never_stops_the_chain() {
    kaspa_core::log::try_init_logger("warn");
    let mut net = Net::over_cfg(boot_config(0, Vec::new()), TestConsensus::new);
    net.beat_to(1).await;
    let onbs = onboard_all(&mut net, vec![Spec::honest(41, 1, true), Spec::honest(42, 3, true), Spec::honest(43, 5, true)]).await;
    for o in &onbs {
        commit(&mut net, o, 0x22).await;
    }
    let collateral: Vec<u64> = onbs.iter().map(|o| net.collateral(o.card)).collect();
    let late_fail = {
        let mut p = complete_post(&net, &onbs[0]);
        p.leaves.backend[0] ^= 1; // every input right, one implementation's leaf root wrong: fails after the whole forward work
        p
    };
    let early_fail = {
        let mut p = complete_post(&net, &onbs[1]);
        p.operands.truncate(3); // a junk inventory: fails at the first check
        p
    };
    let honest = complete_post(&net, &onbs[2]);
    let stranger = complete_post(&net, &onbs[2]);
    let items = vec![
        (onbs[0].card, net.evidence(onbs[0].card, onbs[0].v2, ConformanceEvidenceActionV1::PostComplete(Box::new(late_fail)))),
        (onbs[1].card, net.evidence(onbs[1].card, onbs[1].v2, ConformanceEvidenceActionV1::PostComplete(Box::new(early_fail)))),
        (onbs[2].card, net.evidence(onbs[2].card, onbs[2].v2, ConformanceEvidenceActionV1::PostComplete(Box::new(honest.clone())))),
        (OUTSIDER, net.evidence(OUTSIDER, onbs[2].v2, ConformanceEvidenceActionV1::PostComplete(Box::new(stranger)))),
    ];
    net.send(items).await;
    let judged: Vec<bool> = onbs.iter().map(|o| net.attempt(o.v2).evidence.is_some()).collect();
    assert_eq!(judged.iter().filter(|j| **j).count(), PALW_COMPLETE_CHECKS_PER_BLOCK_V1 as usize, "the block's cap: {judged:?}");
    let (_, adjudications, court_work) = net.budget();
    let works: Vec<u64> =
        onbs.iter().map(|o| palw_complete_check_domain_v1(&o.f.program, o.f.plan.max_positions).unwrap().work).collect();
    let charged: u64 = works.iter().zip(&judged).filter(|(_, j)| **j).map(|(w, _)| *w).sum();
    assert_eq!((adjudications, court_work), (2, charged), "each judged check charged its whole work; the waiting one nothing");
    for (i, o) in onbs.iter().enumerate() {
        let fee = if judged[i] { PALW_COMPLETE_CHECK_FEE_SOMPI_V1 } else { 0 };
        assert_eq!(net.collateral(o.card), collateral[i] - fee, "the fee only for a judged check");
    }
    for (i, o) in onbs.iter().enumerate().take(2) {
        if judged[i] {
            let a = net.attempt(o.v2);
            assert_eq!((a.record.state, a.record.attempts()), (S::RegisteredDormant, 1), "a junk check is a counted failure");
            assert!(matches!(a.last_end, Some((ConformanceAttemptEndV1::EvidenceFailed, _))));
        }
    }
    // The waiting one goes in the next block (its registrant re-sends it).
    let waiting = judged.iter().position(|j| !j).expect("one waits");
    let resend = match waiting {
        0 => {
            let mut p = complete_post(&net, &onbs[0]);
            p.leaves.backend[0] ^= 1;
            p
        }
        1 => {
            let mut p = complete_post(&net, &onbs[1]);
            p.operands.truncate(3);
            p
        }
        _ => honest,
    };
    let o = net.evidence(onbs[waiting].card, onbs[waiting].v2, ConformanceEvidenceActionV1::PostComplete(Box::new(resend)));
    net.send(vec![(onbs[waiting].card, o)]).await;
    assert!(net.attempt(onbs[waiting].v2).evidence.is_some(), "judged in a later block");
    assert_eq!(net.attempt(onbs[2].v2).record.state, S::G14Eligible, "the honest check passed through the junk");
    // A failed class re-commits (attempt 2) and passes honestly.
    commit(&mut net, &onbs[0], 0x23).await;
    let p = complete_post(&net, &onbs[0]);
    let o = net.evidence(onbs[0].card, onbs[0].v2, ConformanceEvidenceActionV1::PostComplete(Box::new(p)));
    net.send(vec![(onbs[0].card, o)]).await;
    assert_eq!(net.attempt(onbs[0].v2).record.state, S::G14Eligible);
    assert_eq!(net.attempt(onbs[0].v2).record.attempts(), 1, "one counted failure, then a pass");
    let z = net.replay().await;
    net.assert_same(&z, "replay");
}

/// **Eligibility is lost when its condition stops holding (DA lapse), and with it the mode.** C binds its V2 class to the commitments
/// of OTHER weights of its program (a false binding nobody refutes at first), passes a sampled conformance with the bootstrap's beacon,
/// becomes eligible and registers its OPV class. Then an outsider refutes the binding with two disagreeing openings: C is
/// `BindingNotStanding`, its next claim is dropped at the door, no new commitment takes it as a source — and the bootstrap class, whose complete
/// check PROVED its binding, keeps its eligibility (its binding cannot be refuted). Another plan of C's program is another class:
/// never eligible without its own conformance.
#[tokio::test]
async fn g14_opv_bootstrap_eligibility_is_lost_when_the_artifact_binding_is_refuted() {
    kaspa_core::log::try_init_logger("warn");
    let mut net = Net::over_cfg(boot_config(0, Vec::new()), TestConsensus::new);
    net.beat_to(1).await;
    let b = onboard_all(&mut net, vec![Spec::honest(36, BOOT, true)]).await.pop().unwrap();
    commit(&mut net, &b, 0x22).await;
    let p = complete_post(&net, &b);
    let o = net.evidence(BOOT, b.v2, ConformanceEvidenceActionV1::PostComplete(Box::new(p)));
    net.send(vec![(BOOT, o)]).await;
    assert!(register_opv(&mut net, BOOT, &b).await);
    // The bootstrap's jobs wait posted (a job is no source; its claim, after C's start, is).
    let jobs = post_jobs(&mut net, BOOT, b.opv, 2, 0x50).await;
    // A false binding: the V2 artifact is seed 37's weights, the kernel class runs seed 38's (the same program).
    let c =
        onboard_all(&mut net, vec![Spec { v2: onb_fixture(37), kernel: onb_fixture(38), card: CAND, complete: false, sealed: false }])
            .await
            .pop()
            .unwrap();
    commit(&mut net, &c, 0x22).await;
    assert_eq!(net.attempt(c.v2).eligible_profiles, vec![Hash64::from_bytes(b.opv)]);
    claims(&mut net, &b.kfx(), b.opv, &jobs, &PRODUCERS[..2]).await;
    until_locked(&mut net, &c).await;
    let post = sampled_post(&net, &c);
    let o = net.evidence(CAND, c.v2, ConformanceEvidenceActionV1::Post(Box::new(post)));
    net.send(vec![(CAND, o)]).await;
    let posted = net.attempt(c.v2).evidence.expect("posted");
    net.beat_to(posted.window_end_daa + 1).await;
    assert_eq!(net.attempt(c.v2).record.state, S::G14Eligible);
    assert_eq!(eligibility_drill(&net, &c), Ok(OpvEligibleV1::Derived { v2_class: c.v2 }), "eligible while the binding stands");
    let kernel_root = Hash64::from_bytes(c.kernel.pc.root());
    let binding = net.api().unwrap().artifact_binding_v1(&c.v2, &kernel_root).unwrap();
    assert!(net.daa() + 12 < binding.final_daa, "the refutation horizon is still open (margin {})", binding.final_daa - net.daa());
    assert!(register_opv(&mut net, CAND, &c).await, "C's OPV class registers while eligible");
    // ---- DA lapse: the binding is refuted ----
    let proof = row_mismatch_proof(&c.f, &c.kernel);
    let o = net.artifact_challenged(OUTSIDER, c.v2, kernel_root, proof);
    net.send(vec![(OUTSIDER, o)]).await;
    assert!(net.api().unwrap().artifact_binding_v1(&c.v2, &kernel_root).unwrap().refuted, "refuted by two disagreeing openings");
    assert_eq!(eligibility_drill(&net, &c), Err(OpvIneligibleV1::BindingNotStanding), "eligibility lost");
    assert_eq!(eligibility_drill(&net, &b), Ok(OpvEligibleV1::Derived { v2_class: b.v2 }), "the bootstrap's proven binding stands");
    let jobs = post_jobs(&mut net, CAND, c.opv, 1, 0x60).await;
    let id = claims(&mut net, &c.kfx(), c.opv, &jobs, &PRODUCERS[2..3]).await[0];
    assert!(!net.ledger().claims.contains_key(&id), "a claim of a class that lost eligibility is dropped at the door");
    // Another plan of the same program (and the same artifact) is another class: no conformance, never eligible.
    let d = k2_tir_v2_descriptor();
    let plan48 = plan_for_tir_program_v1(&d, &b.kernel.program, program_root_v1(&b.kernel.program.encode()), 48).unwrap();
    let facts = OpvClassFactsV1::of_registration(d.digest(), &b.kernel.program.encode(), &plan48, &b.kernel.pc);
    let route = net.api().unwrap();
    let ledger = route.ledger().unwrap();
    let policy = route.header.opv.unwrap();
    let view =
        OpvEligibilityViewV1 { policy: &policy, denied: &[], min_effective_bits: 0, sampled_gates_reward: true, test_eligible: &[] };
    assert_eq!(route.opv_eligibility_v1(&ledger, &facts, net.daa(), &view), Err(OpvIneligibleV1::NotOnboarded));
    assert_eq!(route.opv_eligible_set_v1(&ledger, net.daa(), &view), vec![Hash64::from_bytes(b.opv)], "only the bootstrap stays");
    let z = net.replay().await;
    net.assert_same(&z, "replay");
}

/// **The deny-list only takes away**: a bootstrap class that passed its complete check is not eligible while the network's fence
/// denies it, and its OPV registration is refused.
#[tokio::test]
async fn g14_opv_bootstrap_the_deny_list_takes_eligibility_away_and_never_grants_it() {
    kaspa_core::log::try_init_logger("warn");
    let denied = class_id(&onb_fixture(39), VerificationModeV1::OptimisticPublicVerification);
    let mut net = Net::over_cfg(boot_config(0, vec![Hash64::from_bytes(denied)]), TestConsensus::new);
    net.beat_to(1).await;
    let b = onboard_all(&mut net, vec![Spec::honest(39, BOOT, true)]).await.pop().unwrap();
    assert_eq!(b.opv, denied);
    commit(&mut net, &b, 0x22).await;
    let p = complete_post(&net, &b);
    let o = net.evidence(BOOT, b.v2, ConformanceEvidenceActionV1::PostComplete(Box::new(p)));
    net.send(vec![(BOOT, o)]).await;
    assert_eq!(net.attempt(b.v2).record.state, S::G14Eligible, "the complete check passed");
    assert!(!register_opv(&mut net, BOOT, &b).await, "denied: the OPV registration is refused");
    let route = net.api().unwrap();
    let ledger = route.ledger().unwrap();
    let policy = route.header.opv.unwrap();
    let denied_ids = [Hash64::from_bytes(denied)];
    let view = OpvEligibilityViewV1 {
        policy: &policy,
        denied: &denied_ids,
        min_effective_bits: 0,
        sampled_gates_reward: true,
        test_eligible: &[],
    };
    assert_eq!(route.opv_eligibility_v1(&ledger, &b.facts(), net.daa(), &view), Err(OpvIneligibleV1::Denied));
    assert!(route.opv_eligible_set_v1(&ledger, net.daa(), &view).is_empty());
}

// ---- pure checks over the fixtures -------------------------------------------------------------------------------------------

/// The qualification is a function of the program: the stateless, position-free fixture qualifies with its token bound as its
/// domain; a class with state (history) never does; the bounds refuse what the fold could not afford.
#[test]
fn opv_bootstrap_a_stateless_small_class_qualifies_and_a_class_with_history_never_does() {
    let f = onb_fixture(31);
    let d = palw_complete_check_domain_v1(&f.program, f.plan.max_positions).unwrap();
    assert_eq!((d.token_bound, d.positions, d.inputs), (32, 1, 32), "position-free: one input per token");
    assert!(d.work <= PALW_COMPLETE_CHECK_MAX_WORK_V1 && d.artifact_bytes <= PALW_COMPLETE_CHECK_MAX_ARTIFACT_BYTES_V1);
    let moe = misaka_palw_tir_sketch::fixture::dense_moe_v1(5).program;
    assert!(palw_complete_check_domain_v1(&moe, 64).unwrap_err().contains("state"), "history is never enumerable");
}

/// The complete check passes the truth and fails — never panics — on a moved, ragged or missing leaf, other bytes, a binding to
/// other commitments, and an implementation whose results differ anywhere; the post rides one carrier.
#[test]
fn opv_bootstrap_the_complete_check_passes_the_truth_and_fails_every_lie() {
    use kaspa_consensus_core::palw_artifact::{artifact_leaf_v1, artifact_root_v1};
    let f = onb_fixture(31);
    let d = palw_complete_check_domain_v1(&f.program, f.plan.max_positions).unwrap();
    let ops = ops_of(&f);
    let root = artifact_root_v1(&ops.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
    assert_eq!(root, f.artifact_root);
    let kroot = Hash64::from_bytes(f.pc.root());
    let post = complete_check_post_v1(&f.program, &d, [7; 64], ops.clone()).unwrap();
    assert!(borsh::to_vec(&post).unwrap().len() < PALW_COMPLETE_CHECK_MAX_POST_BYTES_V1, "one carrier");
    assert_eq!(judge_complete_check_v1(&f.program, &d, root, kroot, &post), CompleteVerdictV1::Pass);
    let code = |p: &CompleteCheckPostV1, kroot: Hash64| match judge_complete_check_v1(&f.program, &d, root, kroot, p) {
        CompleteVerdictV1::Fail(x) => x.code,
        CompleteVerdictV1::Pass => "PASS",
    };
    let mut moved = post.clone();
    moved.operands[3].row_start += 1;
    assert_eq!(code(&moved, kroot), "INVENTORY_SHAPE");
    let mut ragged = post.clone();
    ragged.operands[3].bytes.push(0);
    assert_eq!(code(&ragged, kroot), "INVENTORY_SHAPE");
    let mut short = post.clone();
    short.operands.pop();
    assert_eq!(code(&short, kroot), "INVENTORY_SHAPE");
    let mut other_bytes = post.clone();
    other_bytes.operands[0].bytes[0] ^= 1;
    assert_eq!(code(&other_bytes, kroot), "ARTIFACT_ROOT");
    assert_eq!(code(&post, Hash64::from_bytes(onb_fixture(38).pc.root())), "BINDING_NOT_EQUAL", "a false binding is caught");
    let mut liar = post.clone();
    liar.inputs.backend[0] ^= 1;
    assert_eq!(code(&liar, kroot), "RESULTS_DIFFER");
    let mut leaf_liar = post.clone();
    leaf_liar.leaves.independent[0] ^= 1;
    assert_eq!(code(&leaf_liar, kroot), "RESULTS_DIFFER");
    // The reference is the kernel's own greedy run.
    let ledger_free_next = {
        let post_occ = f.program.occurrences().len() - 1;
        let trace = trace_v1(&f.program, &f.params, &[9]).unwrap();
        DecodeRuleV1::Greedy.select(&trace.values[0][post_occ][f.program.logits as usize]).unwrap()
    };
    assert!(ledger_free_next < f.program.token_bound);
}

/// **The test seam is test-only.** The extras field the processor fills from `kernel_route_test_opv_eligible_v1` is `Vec::new()`
/// outside `cfg(test)`, and the hook and its list exist only under `cfg(test)`: no build that can run a network can name a class
/// eligible except through the derived rule.
#[test]
fn opv_test_eligibility_hook_is_test_only() {
    let src = include_str!("../../processor.rs");
    assert!(
        src.contains("#[cfg(test)]\n                    test_eligible: kernel_route_test_opv_eligible_list_v1(),\n                    #[cfg(not(test))]\n                    test_eligible: Vec::new(),"),
        "the extras' test_eligible is the hook only under cfg(test), empty otherwise"
    );
    for item in [
        "static KERNEL_ROUTE_TEST_OPV_ELIGIBLE_V1",
        "pub(crate) fn kernel_route_test_opv_eligible_v1",
        "fn kernel_route_test_opv_eligible_list_v1",
    ] {
        let at = src.find(item).unwrap_or_else(|| panic!("{item} exists"));
        assert!(src[..at].trim_end().ends_with("#[cfg(test)]"), "{item} is compiled only under cfg(test)");
        assert_eq!(src.matches(item).count(), 1, "{item} is defined once");
    }
    assert_eq!(src.matches("test_eligible:").count(), 2, "filled in exactly the two cfg branches");
}

// ---- the sealed-source beacon v3 on the node ---------------------------------------------------------------------------------

/// Card `p`'s honest claims of `jobs` (of `class`, weights `fx`), SEALED now — salted past the fence — and their reveals kept for later.
async fn seal_now(net: &mut Net, fx: &Fixture, class: Digest, jobs: &[KernelJobV1], producers: &[usize]) -> Vec<(usize, K, Digest)> {
    let ledger = net.ledger();
    let (mut seals, mut out) = (Vec::new(), Vec::new());
    for (job, p) in jobs.iter().zip(producers) {
        let generated = greedy(fx, &ledger, &class, &job.prompt, job.max_new_tokens as usize);
        let produced = produce(fx, &ledger, &class, job, net.kid(*p), generated, |_| {});
        let id = produced.claim.id();
        let (seal, reveal) = seal_and_reveal(&ledger, net.kid(*p), &produced.object);
        assert!(matches!(reveal, K::CommitClaimSalted { .. }), "past the fence the reveal carries its salt");
        seals.push((*p, net.route(*p, &seal)));
        out.push((*p, reveal, id));
    }
    net.send(seals).await;
    out
}

async fn reveal_now(net: &mut Net, reveals: &[(usize, K, Digest)]) {
    let items: Vec<(usize, Obj)> = reveals.iter().map(|(p, r, _)| (*p, net.route(*p, r))).collect();
    net.send(items).await;
}

/// **G14 condition 9 for v3 (RFC-0014 §3.4): the fresh non-Panel verifier**, from public reads alone — op 231's rows and the class's
/// program, op 212's Finals, and every page of op 211 (the route rebuilt and checked against the served roots, its seal facts derived
/// by the chain's own function) — through the SDK's path (`misaka model onboard verify`).
fn fresh_v3(net: &Net, o: &Onb) -> misaka_palw_sdk::onboarding_chain::FreshReportV1 {
    use misaka_palw_sdk::onboarding_chain::{
        PublicConformanceReadsV1, fresh_verify_from_reads_v1, sealed_sources_from_kernel_rows_v1,
    };
    let api = net.api().expect("the route");
    let (mut rows, mut after) = (Vec::new(), None);
    loop {
        // A small page, so the reader really gathers several (op 211's cursor).
        let page = api.rows_page_v1(after.take(), 4_096);
        rows.extend(page.rows);
        match page.next {
            Some(cursor) => after = Some(cursor),
            None => break,
        }
    }
    assert!(rows.len() as u64 == api.rows_page_v1(None, usize::MAX).total_rows, "every row gathered");
    let header = borsh::to_vec(&api.header).expect("the header serializes");
    let sealed_sources = sealed_sources_from_kernel_rows_v1(&header, rows.clone(), &api.ledger_root(), &api.aux_root())
        .expect("the served rows are the chain's");
    // One row changed: the reader's copy no longer roots to the served root, and it is refused.
    let mut forged = rows;
    if let Some(r) = forged.iter_mut().find(|(table, _, _)| *table < PALW_KERNEL_ROUTE_FIRST_AUX_TABLE_V1) {
        r.2.push(0);
    }
    assert!(
        sealed_sources_from_kernel_rows_v1(&header, forged, &api.ledger_root(), &api.aux_root()).is_err(),
        "a forged row is refused"
    );
    let read = net.chain.ctx.consensus.palw_conformance_evidence_v1(o.v2).expect("the class is known");
    let events = api
        .finals_read_v1()
        .expect("the rows rebuild")
        .into_iter()
        .filter_map(|f| f.event.map(|e| borsh::from_slice(&e).expect("a beacon event")))
        .collect();
    let reads = PublicConformanceReadsV1 {
        attempt_row: read.attempt_row.expect("the attempt row"),
        evidence_row: read.evidence_row,
        events,
        sealed_sources,
        tip_daa: net.daa(),
        program: read.program.expect("the class's program"),
    };
    fresh_verify_from_reads_v1(&reads, None).expect("the SDK verifier runs a v3 attempt")
}

fn beacon_of(net: &Net, o: &Onb) -> AttemptBeaconV1 {
    net.api().expect("the route").attempt_beacon_v1(&net.attempt(o.v2), net.daa()).expect("a beacon read")
}

/// The bootstrap B (complete check, eligible, registered under OPV) and a candidate C bound under the SEALED-SOURCE policy, committed.
async fn sealed_world(seed: u64) -> (Net, Onb, Onb) {
    let mut net = Net::over_cfg(boot_config(0, Vec::new()), TestConsensus::new);
    net.beat_to(1).await;
    let mut onbs = onboard_all(&mut net, vec![Spec::honest(seed, BOOT, true), Spec::sealed(seed + 1, CAND)]).await;
    let c = onbs.pop().unwrap();
    let b = onbs.pop().unwrap();
    commit(&mut net, &b, 0x22).await;
    let p = complete_post(&net, &b);
    let o = net.evidence(BOOT, b.v2, ConformanceEvidenceActionV1::PostComplete(Box::new(p)));
    net.send(vec![(BOOT, o)]).await;
    assert!(register_opv(&mut net, BOOT, &b).await, "the bootstrap registers under OPV");
    commit(&mut net, &c, 0x22).await;
    let a = net.attempt(c.v2);
    assert!(a.is_sealed_source() && a.policy() == palw_onboarding_sealed_policy_v1());
    assert_eq!(a.eligible_profiles, vec![Hash64::from_bytes(b.opv)], "the sources: the derived eligible set");
    (net, b, c)
}

/// **SG-01 closed on the node: the sealed-source beacon v3 locks on SALTED seals.** B's two producers seal claims inside C's seal
/// window `[S, S + W)` (salted: G14-R4's claim seal v2), and reveal them only once it closes, inside `[S + W, S + 2W)`. The beacon is
/// SEALING, then REVEALING, then SETTLING until both reach OPV Final, then LOCKED over both salts; C's sampled evidence under that
/// seed passes its window; C is derived-eligible (drill floor) with v3's accounting (`G = F`, and `ε_src` a second term). Replay.
#[tokio::test]
async fn g14_opv_bootstrap_a_sealed_source_v3_beacon_locks_on_salted_seals_and_the_class_passes() {
    kaspa_core::log::try_init_logger("warn");
    let (mut net, b, c) = sealed_world(51).await;
    let policy = palw_onboarding_sealed_policy_v1();
    let start = net.attempt(c.v2).committed_daa + policy.anchor_delay_slots;
    let w = policy.beacon_window_slots;
    let jobs = post_jobs(&mut net, BOOT, b.opv, 2, 0x70).await;
    net.beat_to(start).await;
    let reveals = seal_now(&mut net, &b.kfx(), b.opv, &jobs, &PRODUCERS[..2]).await;
    assert!(net.daa() < start + w, "both seals inside the seal window");
    assert!(matches!(beacon_of(&net, &c), AttemptBeaconV1::Waiting { state: "SEALING", have: 2, .. }), "{:?}", beacon_of(&net, &c));
    net.beat_to(start + w).await;
    reveal_now(&mut net, &reveals).await;
    for (_, _, id) in &reveals {
        assert!(net.ledger().claims.contains_key(id), "the salted reveal committed");
        assert!(net.ledger().claim_beacon_salt(id).is_some(), "its salt is kept (table 25)");
    }
    assert!(net.daa() < start + 2 * w, "both reveals inside the reveal window");
    let ttpb = net.ttpb();
    let mut locked = None;
    for _ in 0..400 {
        match beacon_of(&net, &c) {
            AttemptBeaconV1::Locked(bk) => {
                locked = Some(bk);
                break;
            }
            AttemptBeaconV1::Waiting { .. } => {
                net.chain.heartbeat(ttpb, Vec::new()).await;
            }
            other => panic!("the v3 beacon must lock here: {other:?}"),
        }
    }
    let locked = locked.expect("the v3 beacon locked");
    assert_eq!(locked.sources.len(), 2, "both salted sources mixed");
    let post = sampled_post(&net, &c);
    let o = net.evidence(CAND, c.v2, ConformanceEvidenceActionV1::Post(Box::new(post)));
    net.send(vec![(CAND, o)]).await;
    let posted = net.attempt(c.v2).evidence.expect("the fold accepted the evidence under the v3 seed");
    assert_eq!(posted.beacon_output.as_bytes(), locked.output);
    // G14 condition 9: a fresh non-Panel verifier re-derives the same v3 beacon, the same seed and a passing verdict from public reads.
    let fresh = fresh_v3(&net, &c);
    assert_eq!(
        fresh.verdict.beacon_output,
        Some(locked.output),
        "the fresh verifier's v3 beacon is the chain's: {}",
        fresh.verdict.beacon
    );
    assert_eq!(fresh.verdict.seed, Some(posted.seed.as_bytes()), "and its seed");
    assert_eq!(fresh.verdict.posted, Some(Ok(())), "and the evidence, bound and passing");
    assert!(fresh.agrees, "an open window: the verifier agrees with the chain ({})", fresh.why);
    net.beat_to(posted.window_end_daa + 1).await;
    let fresh = fresh_v3(&net, &c);
    assert!(
        fresh.chain_says_passed && fresh.agrees,
        "after the window: the chain passed it and the fresh verifier agrees ({})",
        fresh.why
    );
    // Passed: G14_ELIGIBLE, or already ACTIVE_REWARDABLE — the v3 beacon needs `2W` DAA of seal and reveal windows plus the sources'
    // OPV Finals, long enough for C's artifact binding to pass its whole refutation horizon, after which `activate_due_classes` may
    // activate C. ACTIVE_REWARDABLE is legitimate only past that horizon (the binding Final), never because of the beacon.
    let state = net.attempt(c.v2).record.state;
    assert!(matches!(state, S::G14Eligible | S::ActiveRewardable), "C passed with the v3 beacon: {state:?}");
    if state == S::ActiveRewardable {
        let binding = net.api().unwrap().artifact_binding_v1(&c.v2, &Hash64::from_bytes(c.kernel.pc.root())).expect("C's binding");
        assert_eq!(binding.state_at(net.daa()), ArtifactBindingStateV1::Final, "activated only past the binding's horizon");
    }
    assert_eq!(eligibility_drill(&net, &c), Ok(OpvEligibleV1::Derived { v2_class: c.v2 }));
    // v3's accounting: a 2-bit scope is still 0 effective bits (a drill), whatever the beacon; never above ε_src − 1.
    assert_eq!(
        attempt_effective_bits_v1(&net.api().unwrap(), &c.v2, &net.attempt(c.v2)),
        misaka_palw_challenge::EffectiveBitsV1::Bits(0)
    );
    let z = net.replay().await;
    net.assert_same(&z, "replay");
}

/// **A withheld v3 seal vetoes, counted — it is never an exclusion.** Three producers seal inside the window; one never reveals. At
/// the reveal window's close the attempt ends BEACON_VETOED (a counted retry), whatever the other two salts say: the adversary's only
/// post-reveal move is {lock, veto}. Replay.
#[tokio::test]
async fn g14_opv_bootstrap_a_withheld_v3_seal_vetoes_the_attempt_and_is_counted() {
    kaspa_core::log::try_init_logger("warn");
    let (mut net, b, c) = sealed_world(53).await;
    let policy = palw_onboarding_sealed_policy_v1();
    let start = net.attempt(c.v2).committed_daa + policy.anchor_delay_slots;
    let w = policy.beacon_window_slots;
    let jobs = post_jobs(&mut net, BOOT, b.opv, 3, 0x78).await;
    net.beat_to(start).await;
    let reveals = seal_now(&mut net, &b.kfx(), b.opv, &jobs, &PRODUCERS[..3]).await;
    net.beat_to(start + w).await;
    reveal_now(&mut net, &reveals[..2]).await;
    assert!(matches!(beacon_of(&net, &c), AttemptBeaconV1::Waiting { state: "REVEALING", have: 2, .. }), "{:?}", beacon_of(&net, &c));
    net.beat_to(start + 2 * w + 1).await;
    let a = net.attempt(c.v2);
    assert_eq!((a.record.state, a.record.attempts()), (S::RegisteredDormant, 1), "vetoed: a counted retry");
    assert!(matches!(a.last_end, Some((ConformanceAttemptEndV1::BeaconVetoed, _))), "{:?}", a.last_end);
    let z = net.replay().await;
    net.assert_same(&z, "replay");
}
