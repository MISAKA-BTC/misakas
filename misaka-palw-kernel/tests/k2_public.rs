//! **The 2026-10-07 amendments' measure on the kernel route**: an ordinary public bond that knows only a claim's published bytes
//! localizes a lie and convicts it; a court node that also knows only bytes agrees; withheld material ends in an objective
//! producer default, never a conviction; and no reward opens until RFC-0015 §1.1's criteria are evidenced for exactly that profile
//! with public material — for inference classes and for RFC-0004's evaluation claims alike.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::*;
use misaka_palw_kernel::assurance::AssuranceModeV1;
use misaka_palw_kernel::check::registration_outcome_v1;
use misaka_palw_kernel::descriptor::{k2_tir_v1_descriptor, k2_tir_v2_descriptor};
use misaka_palw_kernel::improve::{
    EpochKernelPolicyV1, EvaluationCompositionV1, EvaluationProsecutabilityV1, EvaluationResultV1, ImprovementRewardBlockV1,
    PromotionRuleV1, SubjectV1, improvement_reward_gate_v1, promotion_decision_v1,
};
use misaka_palw_kernel::lifecycle::{ClaimEventV1, ClaimLifecycleV1, ClaimStateV1, LifecyclePolicyV1};
use misaka_palw_kernel::outcome::{CoverageBucketV1, CoverageEvidenceV1};
use misaka_palw_kernel::public::{
    DemandOutcomeV1, FaultProofWireV1, FreshVerifierV1, MaterialDemandV1, MaterialKeyV1, ProfileMaterialV1, ProsecutionCriterionV1,
    ProsecutionGateV1, ReleaseMetricsV1, RewardBlockV1, TensorWireV1, reward_eligible_v1, settle_demand_v1,
};
use misaka_palw_kernel::verify::{DismissalV1, MaterialV1, ScopeV1, ScopeVerdictV1};
use misaka_palw_tir::{Prim, Tensor};

/// A peer serving a claim's values as **bytes** (whoever serves: the producer, a DA provider). Withheld keys are not served.
struct BytePeer {
    nodes: BTreeMap<(u32, u16, u16), Vec<u8>>,
    params: BTreeMap<(u16, Option<u16>), Vec<u8>>,
}

impl BytePeer {
    fn serving(c: &Claim, trace: &misaka_palw_kernel::trace::TraceV1, withhold: &[(u32, u16, u16)]) -> Self {
        let mut nodes = BTreeMap::new();
        for (p, pos) in trace.values.iter().enumerate() {
            for (s, occ) in pos.iter().enumerate() {
                for (n, t) in occ.iter().enumerate() {
                    let k = (p as u32, s as u16, n as u16);
                    if !withhold.contains(&k) {
                        nodes.insert(k, borsh::to_vec(&TensorWireV1::of(t)).unwrap());
                    }
                }
            }
        }
        let params = c.params.tensors.iter().map(|(k, t)| (*k, borsh::to_vec(&TensorWireV1::of(t)).unwrap())).collect();
        BytePeer { nodes, params }
    }

    fn wire(bytes: &[u8]) -> Option<Tensor> {
        borsh::from_slice::<TensorWireV1>(bytes).ok()?.decode().ok()
    }
}

impl MaterialV1 for BytePeer {
    fn node_value(&self, p: u32, s: u16, n: u16) -> Option<Tensor> {
        self.nodes.get(&(p, s, n)).and_then(|b| Self::wire(b))
    }
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        self.params.get(&(index, layer)).and_then(|b| Self::wire(b))
    }
}

fn known() -> Vec<misaka_palw_kernel::KernelDescriptorV1> {
    vec![k2_tir_v1_descriptor(), k2_tir_v2_descriptor()]
}

fn lie(c: &Claim) -> ((u32, u16, u16), misaka_palw_kernel::trace::TraceV1) {
    let at = c.find(3, |p| matches!(p, Prim::MatMul));
    let mut t = c.trace.clone();
    bump(&mut t.values[at.0 as usize][at.1 as usize][at.2 as usize], 1);
    (at, t)
}

#[test]
fn a_fresh_public_bond_convicts_from_published_bytes_and_a_byte_level_court_agrees() {
    let c = Claim::honest();
    let (at, lying) = lie(&c);
    // Published bytes, and a peer serving bytes. From here on nothing of the producer's is in scope.
    let record = c.publish(&lying);
    let peer = BytePeer::serving(&c, &lying, &[]);
    let header = c.header();
    drop(lying);

    let prosecutor = FreshVerifierV1::from_public_bytes(&record, &known(), header).unwrap();
    let ScopeVerdictV1::Fault(proof) = prosecutor.check(&peer, &ScopeV1::WholeClaim) else { panic!("the lie is found") };
    assert_eq!((proof.position, proof.occurrence, proof.node), at);
    let filing = FaultProofWireV1::of(&proof).to_bytes();

    // Another node, with the same public bytes and the filing's bytes alone.
    let court = FreshVerifierV1::from_public_bytes(&record, &known(), header).unwrap();
    let conviction = court.try_proof(&filing).unwrap();
    assert_eq!((conviction.position, conviction.occurrence, conviction.node), at);

    // The honest claim passes for a fresh verifier, and the same filing against it is dismissed.
    let honest = c.publish(&c.trace);
    let fresh = FreshVerifierV1::from_public_bytes(&honest, &known(), header).unwrap();
    assert!(matches!(fresh.check(&BytePeer::serving(&c, &c.trace, &[]), &ScopeV1::WholeClaim), ScopeVerdictV1::Pass { .. }));
    assert!(fresh.try_proof(&filing).is_err(), "a false accusation is dismissed");
    // Garbage filings are dismissed, never convicted.
    assert!(matches!(fresh.try_proof(&[1, 2, 3]), Err(DismissalV1::NotAuthentic(_))));
}

#[test]
fn a_fresh_verifier_refuses_an_unknown_kernel_another_program_or_another_artifact() {
    let c = Claim::honest();
    let record = c.publish(&c.trace);
    assert!(FreshVerifierV1::from_public_bytes(&record, &[], c.header()).is_err(), "an unknown kernel is never success");
    let mut h = c.header();
    h.program_root = [1; 64];
    assert!(FreshVerifierV1::from_public_bytes(&record, &known(), h).is_err());
    let mut h = c.header();
    h.artifact_root = [1; 64];
    assert!(FreshVerifierV1::from_public_bytes(&record, &known(), h).is_err());
    assert!(FreshVerifierV1::from_public_bytes(&[0xFF; 9], &known(), c.header()).is_err());
}

#[test]
fn withheld_material_ends_in_an_objective_producer_default_never_a_conviction() {
    let c = Claim::honest();
    let (at, lying) = lie(&c);
    let record = c.publish(&lying);
    // The producer withholds exactly the value that would convict it.
    let peer = BytePeer::serving(&c, &lying, &[at]);
    let v = FreshVerifierV1::from_public_bytes(&record, &known(), c.header()).unwrap();
    assert!(matches!(v.check(&peer, &ScopeV1::WholeClaim), ScopeVerdictV1::Unavailable { .. }), "no pass, no conviction");

    let key = MaterialKeyV1::Node { position: at.0, occurrence: at.1, node: at.2 };
    let demand = MaterialDemandV1 { claim_id: [8; 64], key, demander_bond: [0xB0; 64], filed_daa: 100, deadline_daa: 150 };
    assert_eq!(settle_demand_v1(&v, &demand, None, 120), DemandOutcomeV1::Pending);
    // Serving a different (e.g. the honest) value is not serving the committed one.
    let other = TensorWireV1::of(&c.trace.values[at.0 as usize][at.1 as usize][at.2 as usize]);
    assert_eq!(settle_demand_v1(&v, &demand, Some(&other), 149), DemandOutcomeV1::Pending);
    assert_eq!(settle_demand_v1(&v, &demand, Some(&other), 150), DemandOutcomeV1::ProducerDefault);
    let ghost = MaterialDemandV1 { key: MaterialKeyV1::Node { position: 99, occurrence: 0, node: 0 }, ..demand };
    assert_eq!(settle_demand_v1(&v, &ghost, None, 150), DemandOutcomeV1::NoSuchValue);

    // The lifecycle records the default as an availability outcome.
    let mut l = ClaimLifecycleV1::new(LifecyclePolicyV1 { check_window_daa: 100, challenge_window_daa: 50 });
    l.apply(ClaimEventV1::BindChallenge { anchor_daa: 90 }).unwrap();
    l.apply(ClaimEventV1::StartChecking { daa: 95 }).unwrap();
    let s = l.apply(ClaimEventV1::MaterialUnavailable { daa: 150, producer_defaulted: true }).unwrap();
    assert_eq!(*s, ClaimStateV1::Unavailable { daa: 150, producer_defaulted: true });

    // Had the producer served the committed value in time, prosecution continues from it to a conviction.
    let committed = TensorWireV1::of(&lying.values[at.0 as usize][at.1 as usize][at.2 as usize]);
    let DemandOutcomeV1::Served(_) = settle_demand_v1(&v, &demand, Some(&committed), 140) else { panic!() };
    let ScopeVerdictV1::Fault(proof) = v.check(&BytePeer::serving(&c, &lying, &[]), &ScopeV1::WholeClaim) else { panic!() };
    v.try_proof(&FaultProofWireV1::of(&proof).to_bytes()).unwrap();
}

fn complete_gate(profile: [u8; 64]) -> ProsecutionGateV1 {
    ProsecutionGateV1 {
        profile,
        met: ProsecutionCriterionV1::ALL.into_iter().collect(),
        evidence_root: [0xD1; 64],
        prosecutor_was_seat_or_operator: false,
        used_private_material: false,
    }
}

#[test]
fn no_reward_opens_without_a_complete_public_prosecution_of_exactly_that_profile() {
    let c = Claim::honest();
    let d = k2_tir_v1_descriptor();
    let armed = registration_outcome_v1(&active(), &d, &c.program, root_of(&c.program), MAX_POSITIONS, 0);
    let profile = c.header().class_binding_id;
    let public = ProfileMaterialV1::kernel_route(true);
    assert_eq!(reward_eligible_v1(&profile, &armed, &public, None), Err(RewardBlockV1::NoDrill));
    let mut seat = complete_gate(profile);
    seat.prosecutor_was_seat_or_operator = true;
    assert!(matches!(
        reward_eligible_v1(&profile, &armed, &public, Some(&seat)),
        Err(RewardBlockV1::Incomplete(m)) if m == vec![ProsecutionCriterionV1::OrdinaryPublicEntry]
    ));
    let mut injected = complete_gate(profile);
    injected.used_private_material = true;
    assert!(matches!(reward_eligible_v1(&profile, &armed, &public, Some(&injected)), Err(RewardBlockV1::Incomplete(_))));
    let mut no_chain = complete_gate(profile);
    no_chain.met = ProsecutionCriterionV1::ALL[..6].iter().copied().collect::<BTreeSet<_>>();
    assert!(matches!(reward_eligible_v1(&profile, &armed, &public, Some(&no_chain)), Err(RewardBlockV1::Incomplete(_))));
    assert_eq!(reward_eligible_v1(&profile, &armed, &public, Some(&complete_gate([0; 64]))), Err(RewardBlockV1::OtherProfile));
    let private = ProfileMaterialV1 { needs_fold_prefix: true, ..public };
    assert_eq!(reward_eligible_v1(&profile, &armed, &private, Some(&complete_gate(profile))), Err(RewardBlockV1::PrivateMaterial));
    let shipped = registration_outcome_v1(&misaka_palw_kernel::builtin_schedule_v1(), &d, &c.program, root_of(&c.program), 64, 0);
    assert!(matches!(
        reward_eligible_v1(&profile, &shipped, &public, Some(&complete_gate(profile))),
        Err(RewardBlockV1::NotEligible(_))
    ));
    reward_eligible_v1(&profile, &armed, &public, Some(&complete_gate(profile))).unwrap();

    // The metrics stay apart: eligibility with ≥ 128 bits is not coverage, and coverage is not the gate.
    let m = ReleaseMetricsV1::of(
        &armed,
        CoverageEvidenceV1 { onchain_registration: true, public_prosecution_measured: false },
        true,
        None,
    );
    assert_eq!(m.coverage, CoverageBucketV1::Untested);
    assert!(m.error_bits.unwrap() >= 128);
    assert_eq!(m.prosecution_missing.len(), 7);
}

#[test]
fn a_promotion_opens_a_reward_only_when_every_evaluation_claim_is_publicly_prosecutable() {
    let d = k2_tir_v1_descriptor();
    let comp = EvaluationCompositionV1 {
        task_root: [1; 64],
        tokenizer_or_input_schema_root: [2; 64],
        task_output_schema: [3; 64],
        score_definition_root: [4; 64],
    };
    let policy = EpochKernelPolicyV1 {
        line: [5; 64],
        epoch: 1,
        opened_daa: 0,
        permitted_descriptors: vec![d.digest()],
        composition: comp,
        cross_kernel_pairs: vec![],
    };
    let rule = PromotionRuleV1 { n_min: 10, delta_num: 1, delta_den: 10, alpha_num: 1, alpha_den: 20, candidates: 1 };
    let cand = [0xCA; 64];
    let mode = AssuranceModeV1::Probabilistic { descriptor: d.digest(), error_bits: 200 };
    let mut results = Vec::new();
    for item in 0..12u32 {
        let id = |s: u8| {
            let mut x = [s; 64];
            x[0] = item as u8;
            x
        };
        results.push(EvaluationResultV1 { claim_id: id(1), item, subject: SubjectV1::Parent, score: 0, mode });
        results.push(EvaluationResultV1 { claim_id: id(2), item, subject: SubjectV1::Candidate(cand), score: 1, mode });
    }
    let items: Vec<u32> = (0..12).collect();
    let decision = promotion_decision_v1(&policy, &rule, cand, &items, &results).unwrap();
    assert!(decision.eligible, "{:?}", decision.why_not);
    let profile = [0x77; 64];
    let all_public: Vec<_> = results
        .iter()
        .map(|r| EvaluationProsecutabilityV1 { claim_id: r.claim_id, profile, inputs_public_after_draw: true })
        .collect();
    // Quality passed; computation must be prosecutable too.
    assert!(matches!(
        improvement_reward_gate_v1(&decision, &results, &all_public, &[]),
        Err(ImprovementRewardBlockV1::NotProsecutable(..))
    ));
    improvement_reward_gate_v1(&decision, &results, &all_public, &[complete_gate(profile)]).unwrap();
    let mut secret = all_public.clone();
    secret[3].inputs_public_after_draw = false;
    assert!(matches!(
        improvement_reward_gate_v1(&decision, &results, &secret, &[complete_gate(profile)]),
        Err(ImprovementRewardBlockV1::NotProsecutable(..))
    ));
}
