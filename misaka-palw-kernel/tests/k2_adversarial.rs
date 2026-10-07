//! **RFC-0011 §15.7 / §16.5's adversarial and completion cases** not covered elsewhere: a permuted history window, a swapped
//! expert choice, a weight that is not the committed one, a cheap but unsound custom suite, a plan naming an unknown checker,
//! receipts reused across a reorg rebind, and verdicts that are identical across independent runs, scopes and a byte-only
//! verifier (node / SDK / IBD agreement).

mod common;

use common::*;
use misaka_palw_kernel::VerificationPlanV1;
use misaka_palw_kernel::beacon::{AnchorEventV1, AnchorTrackerV1, BeaconPolicyV1, ChainViewV1};
use misaka_palw_kernel::check::{check_plan_v1, registration_outcome_v1};
use misaka_palw_kernel::descriptor::{KernelScheduleV1, KernelStatusV1, k2_tir_v1_descriptor, k2_tir_v2_descriptor};
use misaka_palw_kernel::family::{CheckerIdV1, ConstraintFamilyV1};
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::public::FreshVerifierV1;
use misaka_palw_kernel::receipt::{
    ClaimFactsV1, ReceiptInputsV1, ReceiptRefusalV1, ReceiptSignatureVerifier, SignedConstraintReceiptV1, admit_receipt_v1,
    receipt_for_v1,
};
use misaka_palw_kernel::verify::{ClaimVerdictV1, FaultKindV1, MaterialV1, ScopeV1, TraceMaterialV1};
use misaka_palw_tir::{Prim, Tensor};

fn honest_material(c: &Claim) -> TraceMaterialV1<'_> {
    TraceMaterialV1 { trace: &c.trace, params: &c.params }
}

/// Mutate one committed value, verify, and require a fault of `kind` the public court convicts.
fn caught(c: &Claim, at: (u32, u16, u16), kind: FaultKindV1, mutate: impl Fn(&mut Tensor)) {
    let mut lie = c.trace.clone();
    let t = &mut lie.values[at.0 as usize][at.1 as usize][at.2 as usize];
    let before = t.clone();
    mutate(t);
    assert_ne!(*t, before, "the mutation must change the value");
    let (v, court) = c.verify_with(&lie, &TraceMaterialV1 { trace: &lie, params: &c.params });
    let ClaimVerdictV1::Fault(proof) = v else { panic!("{v:?}") };
    assert_eq!((proof.position, proof.occurrence, proof.node), at);
    assert_eq!(proof.kind, kind);
    court(&proof).unwrap();
}

#[test]
fn a_permuted_history_window_is_caught_at_its_append() {
    let c = Claim::honest();
    let at = c.find(3, |p| matches!(p, Prim::HistAppend { .. }));
    // The window is derived (rebuilt from the committed rows, never served): its wrong commitment is convicted from the rows alone.
    caught(&c, at, FaultKindV1::Misderived, |t| {
        // Swap the first two rows of the window (memory permutation, RFC-0011 §15.7).
        let row: usize = t.shape[1..].iter().product();
        let (a, b) = t.data.split_at_mut(row);
        a.swap_with_slice(&mut b[..row]);
    });
}

#[test]
fn a_swapped_expert_choice_is_caught_at_the_router() {
    let c = Claim::honest();
    let at = c.find(1, |p| matches!(p, Prim::TopK { .. }));
    // The same experts in another order (or a tie broken the other way) is another value of the TopK relation.
    caught(&c, at, FaultKindV1::Recompute, |t| t.data.reverse());
}

#[test]
fn a_weight_that_is_not_the_committed_one_is_unavailable_never_a_pass() {
    struct SwapWeights<'a>(TraceMaterialV1<'a>);
    impl MaterialV1 for SwapWeights<'_> {
        fn node_value(&self, p: u32, s: u16, n: u16) -> Option<Tensor> {
            self.0.node_value(p, s, n)
        }
        fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
            let mut t = self.0.param(index, layer)?;
            t.data[0] = if t.data[0] == 0 { 1 } else { 0 };
            Some(t)
        }
    }
    let c = Claim::honest();
    let (v, _) = c.verify_with(&c.trace, &SwapWeights(honest_material(&c)));
    assert!(matches!(&v, ClaimVerdictV1::Unavailable { what } if what.contains("not the committed")), "{v:?}");
}

#[test]
fn a_cheap_but_unsound_custom_suite_cannot_admit_a_class() {
    let fx = misaka_palw_tir_sketch::fixture::dense_moe_v1(7);
    let root = root_of(&fx.program);
    // One repetition: 126 − log2 R bits never reaches the 128-bit target.
    let mut one = k2_tir_v1_descriptor();
    one.soundness.repetitions = 1;
    let o = registration_outcome_v1(&active_for(&one), &one, &fx.program, root, MAX_POSITIONS, 0);
    assert_eq!(o.code(), "BOUNDS_EXCEEDED", "{o}");
    // "Check" the dense family by exact replay — the replay this route replaces — is not a descriptor of this line.
    let mut replay = k2_tir_v1_descriptor();
    replay.families.iter_mut().find(|f| f.family == ConstraintFamilyV1::DenseMatrix).unwrap().checker = CheckerIdV1::ExactRecompute;
    let o = registration_outcome_v1(&active_for(&replay), &replay, &fx.program, root, MAX_POSITIONS, 0);
    assert_eq!(o.code(), "PLAN_FORGED", "{o}");
    // A plan naming a checker id no binary implements does not even parse: unknown ids are never success.
    let d = k2_tir_v1_descriptor();
    let plan = misaka_palw_kernel::plan_for_tir_program_v1(&d, &fx.program, root, MAX_POSITIONS).unwrap();
    let mut bytes = borsh::to_vec(&plan).unwrap();
    let rel = borsh::to_vec(&plan.relations[0]).unwrap();
    let at = bytes.windows(rel.len()).position(|w| w == rel.as_slice()).unwrap();
    // `block u8, node u16, prim_tag u8, family u8, checker u8`: the checker byte is the sixth.
    bytes[at + 5] = 0xEE;
    assert!(borsh::from_slice::<VerificationPlanV1>(&bytes).is_err());
    // And a v2 plan is never accepted by the v1 checker (nor the reverse).
    let v2 = k2_tir_v2_descriptor();
    let p2 = misaka_palw_kernel::plan_for_tir_program_v1(&v2, &fx.program, root, MAX_POSITIONS).unwrap();
    let both = KernelScheduleV1::default()
        .with(d.digest(), KernelStatusV1::Active { since_daa: 0 })
        .with(v2.digest(), KernelStatusV1::Active { since_daa: 0 });
    assert_eq!(check_plan_v1(&both, &d, &fx.program, root, &p2, 0).unwrap_err().code(), "PLAN_FORGED");
}

struct Chain {
    tip: u64,
    fork: u8,
}

impl ChainViewV1 for Chain {
    fn selected_from(&self, from: u64, n: usize) -> Vec<(u64, Digest)> {
        (from..=self.tip)
            .take(n)
            .map(|d| (d, misaka_palw_kernel::hash::id(b"test/block", &[&d.to_le_bytes()[..], &[self.fork]].concat())))
            .collect()
    }
    fn tip_daa(&self) -> u64 {
        self.tip
    }
}

struct Toy;

impl ReceiptSignatureVerifier for Toy {
    fn verify(&self, bond: &Digest, message: &Digest, signature: &[u8]) -> bool {
        signature.len() == 128 && signature[..64] == bond[..] && signature[64..] == message[..]
    }
}

#[test]
fn receipts_under_a_stale_anchor_are_refused_after_a_reorg_rebind() {
    let c = Claim::honest();
    let ev = c.evidence_of(&c.trace);
    let d = k2_tir_v1_descriptor();
    let policy = BeaconPolicyV1 { delay_daa: 5, span: 3, finality_daa: 10, inclusion_cutoff_daa: 100, max_rebinds: 3 };
    let mut tracker = AnchorTrackerV1::new(policy, 0, 10);
    let AnchorEventV1::Bound { anchor: first, .. } = tracker.observe(&Chain { tip: 100, fork: 0 }).unwrap() else { panic!() };
    let assignments = vec![([1; 64], ScopeV1::WholeClaim)];
    let (verdict, _) = c.verify_with(&c.trace, &honest_material(&c));
    let receipt = receipt_for_v1(
        &ReceiptInputsV1 {
            descriptor: &d,
            evidence: &ev,
            claim_id: [8; 64],
            assignment_root: [5; 64],
            challenge_anchor: first,
            sample_seed: [0x43; 64],
            seat_bond: [1; 64],
            seat_operator: [1; 64],
            signed_daa: 50,
        },
        &ScopeV1::WholeClaim,
        &verdict,
    )
    .unwrap();
    let mut sig = receipt.seat_bond.to_vec();
    sig.extend_from_slice(&receipt.signing_message());
    let signed = SignedConstraintReceiptV1 { receipt, signature: sig };
    let facts = |anchor| ClaimFactsV1 {
        descriptor: &d,
        evidence: &ev,
        claim_id: [8; 64],
        assignment_root: [5; 64],
        challenge_anchor: anchor,
        sample_seed: [0x43; 64],
        assignments: &assignments,
        deadline_daa: 1_000,
    };
    admit_receipt_v1(&signed, &facts(first), &Toy).unwrap();
    let AnchorEventV1::Rebound { stale, anchor, .. } = tracker.observe(&Chain { tip: 100, fork: 1 }).unwrap() else { panic!() };
    assert_eq!(stale, first);
    assert_eq!(admit_receipt_v1(&signed, &facts(anchor), &Toy), Err(ReceiptRefusalV1::WrongChallenge), "reorg reuse refused");
    assert_eq!(tracker.attempts(), 2);
}

#[test]
fn verdicts_are_identical_across_runs_scopes_and_a_byte_only_verifier() {
    let c = Claim::honest();
    let (a, _) = c.verify_with(&c.trace, &honest_material(&c));
    let (b, _) = c.verify_with(&c.trace, &honest_material(&c));
    assert_eq!(a, b, "two runs (a live node and an IBD replay) agree byte for byte");
    let ClaimVerdictV1::Pass { scope_root, probabilistic_checks, .. } = a else { panic!("{a:?}") };
    // A verifier that knows only the published bytes reaches the same verdict.
    let fresh = FreshVerifierV1::from_public_bytes(&c.publish(&c.trace), &[k2_tir_v1_descriptor()], c.header()).unwrap();
    let ClaimVerdictV1::Pass { scope_root: r2, probabilistic_checks: n2, .. } =
        fresh.check(&honest_material(&c), &ScopeV1::WholeClaim)
    else {
        panic!()
    };
    assert_eq!((scope_root, probabilistic_checks), (r2, n2));
    // Every segment scope passes and together they cover what the whole claim covers.
    let ev = c.evidence_of(&c.trace);
    let mut covered = 0;
    for i in 0..ev.segments.len() as u32 {
        let (v, _) = c.verify_scope(&c.trace, &honest_material(&c), &ScopeV1::Segments(vec![i]));
        let ClaimVerdictV1::Pass { positions, .. } = v else { panic!("{v:?}") };
        covered += positions;
    }
    assert_eq!(covered, TOKENS.len() as u32);
}
