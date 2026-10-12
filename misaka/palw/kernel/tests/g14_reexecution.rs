//! A verifier localizes without a fault hint or producer trace. Proofs are accepted by the ordinary signed-object ledger path.
mod common;

use common::chain::T;
use common::ledger_world::{OUTSIDER, PRODUCER, World};
use common::{Claim, bump};
use misaka_palw_kernel::ledger::{LedgerEventV1, OutsiderFindingV1, OutsiderV1, ProsecutionV1, PublicSourceV1};
use misaka_palw_kernel::public::{FaultProofWireV1, FreshVerifierV1};
use misaka_palw_kernel::verify::{DismissalV1, MaterialV1, ReexecutionV1};
use misaka_palw_tir::{MapParams, Tensor};

struct NoTrace;
impl PublicSourceV1 for NoTrace {
    fn node(&self, _: u8, _: u32, _: u16, _: u16) -> Option<Tensor> {
        panic!("no producer trace may be read")
    }
}
struct Artifact<'a>(&'a MapParams);
impl MaterialV1 for Artifact<'_> {
    fn node_value(&self, _: u32, _: u16, _: u16) -> Option<Tensor> {
        panic!("no producer values")
    }
    fn param(&self, i: u16, l: Option<u16>) -> Option<Tensor> {
        self.0.tensors.get(&(i, l)).cloned()
    }
}

#[test]
fn an_unannounced_fault_past_sixteen_positions_is_convicted_with_zero_demands_and_zero_disclosures() {
    let mut w = World::new();
    let prompt = (0..63).map(|p| (p * 7 + 3) % 32).collect::<Vec<_>>();
    let job = w.post_job(2, &prompt, 1, 60);
    let at = w.matmul_at(62);
    let generated = w.greedy(&w.params, &prompt, 1);
    let lie = w.produce(&job, PRODUCER, generated, &w.params, |trace| {
        bump(&mut trace.values[at.0 as usize][at.1 as usize][at.2 as usize], 0)
    });
    let claim = lie.claim.id();
    w.block(10, vec![lie.tx, T::PanelCovered { claim }]);
    let restored = misaka_palw_kernel::ledger::KernelLedgerV1::from_rows(&w.genesis, w.l.scalars(), &w.l.to_rows()).unwrap();
    let verifier = OutsiderV1 { ledger: &restored, claim, material: &NoTrace, artifact: &w.params, salt: [0; 64] };
    let OutsiderFindingV1::Prosecute(proof) = verifier.check_computation().unwrap() else { panic!("the false commitment") };
    let ProsecutionV1::Kernel(bytes) = &proof else { panic!("an arithmetic court") };
    let wire: FaultProofWireV1 = borsh::from_slice(bytes).unwrap();
    assert_eq!((wire.position, wire.occurrence, wire.node), at);
    assert!(bytes.len() as u64 <= w.l.bounds_of(&w.class).unwrap().max_filing_bytes);
    assert!(w.l.demands.is_empty() && w.l.served.is_empty());
    let ev = w.block(11, vec![T::FileProof { accuser: OUTSIDER, claim, proof }]);
    assert!(ev.iter().any(|e| matches!(e, LedgerEventV1::Convicted { claim: c, .. } if *c == claim)));
    assert!(w.l.claims[&claim].convicted);
}

#[test]
fn local_artifact_aliases_and_false_commitment_filings_cannot_convict() {
    let c = Claim::honest();
    let fresh = FreshVerifierV1::from_public_bytes(&c.publish(&c.trace), &[c.descriptor.clone()], c.header()).unwrap();
    let mut alias = c.params.clone();
    let t = alias.tensors.values_mut().find(|t| t.dtype.width() == 1).unwrap();
    t.data[0] += 256; // same truncated wire bytes, different integer arithmetic
    assert_eq!(misaka_palw_kernel::trace::ParamCommitmentsV1::of(&alias).root(), c.pc.root());
    assert!(fresh.reexecute(&Artifact(&alias)).is_err(), "wire aliases are invalid local artifact tensors");
    let mut lie = c.trace.clone();
    bump(&mut lie.values[1][0][0], 0);
    let other = FreshVerifierV1::from_public_bytes(&c.publish(&lie), &[c.descriptor.clone()], c.header()).unwrap();
    let ReexecutionV1::Fault(mut proof) = other.reexecute(&Artifact(&c.params)).unwrap() else { panic!("fault") };
    assert_eq!(fresh.try_proof(&FaultProofWireV1::of(&proof).to_bytes()), Err(DismissalV1::NoFault));
    proof.inputs[0].data[0] ^= 1;
    assert!(matches!(other.try_proof(&FaultProofWireV1::of(&proof).to_bytes()), Err(DismissalV1::NotAuthentic(_))));
}

#[test]
fn admission_prices_exact_localization_even_when_the_probabilistic_plan_fits() {
    use misaka_palw_kernel::descriptor::{KernelScheduleV1, KernelStatusV1};
    use misaka_palw_kernel::gate::{ProsecutionGapV1, ProsecutionPolicyV1, public_prosecution_complete_v1};
    use misaka_palw_kernel::public::{ProfileMaterialV1, program_root_v1};
    let c = Claim::honest();
    let exact = misaka_palw_kernel::plan::reexecution_bounds_v1(&c.program, &c.plan).0;
    let mut d = c.descriptor.clone();
    d.limits.max_claim_verifier_work = exact - 1;
    let plan =
        misaka_palw_kernel::plan::plan_for_tir_program_v1(&d, &c.program, program_root_v1(&c.program.encode()), c.plan.max_positions)
            .unwrap();
    let schedule = KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 });
    misaka_palw_kernel::check::check_plan_v1(&schedule, &d, &c.program, plan.program_root, &plan, 0)
        .expect("the earlier probabilistic verifier budget fits");
    let policy = ProsecutionPolicyV1 {
        court_deadline_daa: 20,
        max_sessions_per_claim: 1024,
        max_public_bytes: 1 << 50,
        max_verifier_ram: 1 << 50,
        max_retained_state: 1 << 50,
    };
    let gaps = public_prosecution_complete_v1(&d, &c.program, &plan, &ProfileMaterialV1::kernel_route(true), &policy).unwrap_err();
    assert!(gaps.iter().any(|g| matches!(g, ProsecutionGapV1::Unbounded { what: "exact localization work", .. })), "{gaps:?}");
}
