//! **RFC-0006 06-A: the automatic held-GDN checkpoint filer** (2026-10-18; dormant with `palw_memory_routing_court_v2`, which every
//! preset leaves off). A held (per-position) hybrid class serves no checkpoint chunks, so a forged recurrence checkpoint is reached
//! only through the producer's own bytes. Any bond outside the claim's producer runs this, from public material and its own replay:
//!
//! ```text
//! recompute   the accuser's OWN prefix walk finds the first committed checkpoint that is not the job's state
//!             (`base0_gdn_first_forged_checkpoint_v2`); none → nothing to file, an honest producer is never demanded
//! demand      for one recurrence (node slot, head) at a time, the exact `StateChunk` units its transition terminal reads
//!             (`base0_gdn_forged_checkpoint_demands_v2`), each a signed `DefaultAccusedHeld` — one open at a time
//! answers     the producer's `MaterialDisclosedV2` / `MaterialDisclosedHeld` on chain, re-checked by the held DA court's own
//!             hash check (`palw_held_da_check_disclosure_v1`); nothing is taken on the carrier's word
//! accuse      once every unit is answered: the transition evidence (`base0_gdn_transition_evidence_from_disclosures_v2`), held to
//!             the court's own terminal (`check_gdn_transition_refutation_v2`) before anything is signed; a candidate whose answered
//!             bytes are honest moves on to the next (node slot, head); a fault is filed as tag 244 `MemoryRoutingV2::Accused`
//!             (`GdnTransition`), signed over `palw_memory_routing_session_id_v2`
//! silence     an unanswered demand past the disclose window is the held DA court's DA-7 default — the fold's own sweep convicts;
//!             the filer files nothing more and reports it
//! ```
//!
//! Nothing here is a proof of computation (PRINCIPLES §8): the outputs authenticate data membership; the verdict is the court's
//! deterministic recomputation. The case is pure over its inputs (the chain's objects, the tip DAA, a signer), so the node's tick
//! and the tests drive the same state machine.
use kaspa_consensus_core::palw_artifact::{PalwArtifactOpeningV1, PalwProvenOperandsV1};
use kaspa_consensus_core::palw_held_da_v1::{
    PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT, PALW_HELD_DA_VERSION_V1, PalwHeldAccusationV1, PalwHeldDisclosureV1, PalwHeldMissingV1,
    palw_held_da_accusation_message_v1, palw_held_da_check_disclosure_v1,
};
use kaspa_consensus_core::palw_memory_routing_v2::{
    PALW_MEMORY_ROUTING_ACCUSATION_VERSION_V2, PALW_MEMORY_ROUTING_MLDSA87_CONTEXT_V2, PalwMemoryRoutingAccusationV2,
    PalwMemoryRoutingEvidenceV2, PalwMemoryRoutingObjectV2, palw_memory_routing_session_id_v2,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::palw_step::kernel_semantics_id_v1;
use kaspa_consensus_core::palw_step_refute::KDESC_Q36_GDN_STEP;
use kaspa_consensus_core::palw_step_refute::memory_v2::{check_gdn_transition_refutation_v2, gdn_transition_demands_v2};
use kaspa_hashes::Hash64;
use misaka_palw_base0::fp_recompute::Base0FpRecomputeKernelsV1;
use misaka_palw_base0::gdn_evidence_v2::{
    Base0GdnCheckpointCourtV2, Base0GdnMaterialV2, Base0GdnOwnedMaterialV2, base0_gdn_first_forged_checkpoint_v2, base0_gdn_forged_checkpoint_demands_v2,
    base0_gdn_transition_evidence_from_disclosures_v2,
};

/// The claim the case pursues, as the chain records it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwGdnHeldClaimV1 {
    pub claim: Hash64,
    pub executor: PalwBondKeyV2,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    /// The class's registered shape-profile id (the court binds the evidence to it).
    pub shape_profile_id: Hash64,
}

/// **One pursuit of one forged checkpoint.**
#[derive(Clone, Debug)]
pub struct PalwGdnHeldCaseV1 {
    pub claim: PalwGdnHeldClaimV1,
    /// The first forged checkpoint (the accuser's own recompute).
    pub index: u32,
    /// Every recurrence `(node slot, head)` whose transition reads checkpoint `index`, in canonical order.
    pub candidates: Vec<(u32, u32)>,
    /// The candidate under demand.
    pub at: usize,
    /// Its units and the answers found so far.
    pub demands: Vec<PalwHeldMissingV1>,
    pub answers: Vec<Option<PalwHeldDisclosureV1>>,
    /// The unit last demanded and the DAA it was sent.
    pub demanded: Option<(usize, u64)>,
}

/// **What the case asks of the node now.**
#[derive(Clone, Debug)]
pub enum PalwGdnHeldNextV1 {
    /// Carry this signed demand (`DefaultAccusedHeld`).
    Demand { unit: PalwHeldMissingV1, object: PalwConsensusObjectV2 },
    /// A demand is open and inside its window.
    Wait,
    /// Carry this signed accusation (tag 244, `GdnTransition`); the court's own terminal finds the fault on it.
    Accuse { object: PalwConsensusObjectV2 },
    /// The open demand's window passed unanswered: the held DA court's DA-7 default convicts in the fold's sweep.
    DefaultDue { unit: PalwHeldMissingV1 },
    /// Every candidate's answered bytes are the honest ones (or none could be built): nothing to file.
    Exhausted,
}

/// Every recurrence node slot of the binding's profile, with each head its transition demands resolve for at `index`.
fn candidates_v1(binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2, index: u32) -> Vec<(u32, u32)> {
    let gdn = kernel_semantics_id_v1(KDESC_Q36_GDN_STEP);
    let profile = &binding.shape_profile;
    let mut out = Vec::new();
    let mut slot = 0u32;
    while let Some((node, layer)) = profile.resolve_node_slot(slot) {
        if layer.is_some() && node.kernel_semantics_id == gdn {
            let mut head = 0u32;
            while gdn_transition_demands_v2(binding, slot, head, index).is_ok_and(|d| !d.is_empty()) {
                out.push((slot, head));
                head += 1;
                if head > 1 << 12 {
                    break;
                }
            }
        }
        slot += 1;
    }
    out
}

/// **Open a case**, or `None` when the accuser's own recompute finds every committed checkpoint honest.
pub fn palw_gdn_held_open_v1<K: Base0FpRecomputeKernelsV1>(
    claim: PalwGdnHeldClaimV1,
    material: &Base0GdnMaterialV2<'_>,
    mut kernels: impl FnMut() -> K,
) -> Result<Option<PalwGdnHeldCaseV1>, String> {
    if material.binding.committed_execution_root != claim.execution_root {
        return Err("the material's binding is not the claim's execution".into());
    }
    let Some(index) = base0_gdn_first_forged_checkpoint_v2(material, kernels())? else { return Ok(None) };
    let candidates = candidates_v1(material.binding, index);
    if candidates.is_empty() {
        return Err(format!("checkpoint {index} is forged but no recurrence transition reads it (a convolution-window case)"));
    }
    let mut case = PalwGdnHeldCaseV1 { claim, index, candidates, at: 0, demands: Vec::new(), answers: Vec::new(), demanded: None };
    load_candidate_v1(&mut case, material, &mut kernels)?;
    Ok(Some(case))
}

fn load_candidate_v1<K: Base0FpRecomputeKernelsV1>(
    case: &mut PalwGdnHeldCaseV1,
    material: &Base0GdnMaterialV2<'_>,
    kernels: &mut impl FnMut() -> K,
) -> Result<(), String> {
    let (node_slot, head) = case.candidates[case.at];
    case.demands =
        base0_gdn_forged_checkpoint_demands_v2(material, Base0GdnCheckpointCourtV2::Transition { node_slot, head }, case.index, &mut *kernels)?;
    case.answers = vec![None; case.demands.len()];
    case.demanded = None;
    Ok(())
}

/// The producer's answer to `unit` on chain, re-checked by the held DA court's own hash check against the claim's binding.
fn answer_on_chain_v1(
    objects: &[PalwConsensusObjectV2],
    claim: &PalwGdnHeldClaimV1,
    binding: &kaspa_consensus_core::palw_step_leg::PalwStepBindingV2,
    unit: &PalwHeldMissingV1,
    max_leaves: u64,
    form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Option<PalwHeldDisclosureV1> {
    use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1};
    objects.iter().find_map(|object| {
        let carriage = match object {
            PalwConsensusObjectV2::MaterialDisclosedV2 {
                claim: c,
                unit: PalwDaUnitV1::Held(missing),
                answer: PalwDaAnswerV1::Held(carriage),
                ..
            } if *c == claim.claim && missing == unit => carriage.as_ref(),
            PalwConsensusObjectV2::MaterialDisclosedHeld { disclosure } if disclosure.claim == claim.claim && disclosure.missing == *unit => {
                disclosure.as_ref()
            }
            _ => return None,
        };
        palw_held_da_check_disclosure_v1(&claim.execution_root, unit, binding, &carriage.disclosure, max_leaves, form)
            .ok()
            .map(|_| carriage.disclosure.clone())
    })
}

/// The signing seam: `(message, context) → signature` under the accuser's bond key.
pub type PalwGdnHeldSignV1<'a> = &'a dyn Fn(&[u8], &[u8]) -> Option<Vec<u8>>;

/// The accuser's standing inputs for a step.
pub struct PalwGdnHeldAccuserV1<'a> {
    pub bond: PalwBondKeyV2,
    pub network_domain: Hash64,
    /// The class inventory's operand openings the terminal reads (carried in the accusation) and the oracle over them.
    pub openings: &'a [PalwArtifactOpeningV1],
    pub oracle: &'a PalwProvenOperandsV1,
    pub sign: PalwGdnHeldSignV1<'a>,
}

/// **One step of the case** against the chain's objects at DAA `now` (`disclose_window`: the held DA court's answer window).
pub fn palw_gdn_held_step_v1<K: Base0FpRecomputeKernelsV1>(
    case: &mut PalwGdnHeldCaseV1,
    material: &Base0GdnMaterialV2<'_>,
    mut kernels: impl FnMut() -> K,
    objects: &[PalwConsensusObjectV2],
    now: u64,
    disclose_window: u64,
    accuser: &PalwGdnHeldAccuserV1<'_>,
) -> Result<PalwGdnHeldNextV1, String> {
    if accuser.bond == case.claim.executor {
        return Err("the producer never accuses its own claim".into());
    }
    loop {
        for (i, unit) in case.demands.iter().enumerate() {
            if case.answers[i].is_none() {
                case.answers[i] =
                    answer_on_chain_v1(objects, &case.claim, material.binding, unit, material.max_leaves, material.prompt_ids_form);
            }
        }
        if let Some(i) = case.answers.iter().position(Option::is_none) {
            return Ok(match case.demanded {
                Some((open, at)) if open == i && now > at.saturating_add(disclose_window) => {
                    PalwGdnHeldNextV1::DefaultDue { unit: case.demands[i] }
                }
                Some((open, _)) if open == i => PalwGdnHeldNextV1::Wait,
                _ => {
                    let mut demand = PalwHeldAccusationV1 {
                        version: PALW_HELD_DA_VERSION_V1,
                        claim: case.claim.claim,
                        missing: case.demands[i],
                        accuser: accuser.bond,
                        binding: material.binding.clone(),
                        signature: Vec::new(),
                    };
                    let message = palw_held_da_accusation_message_v1(accuser.network_domain.as_byte_slice(), &demand);
                    demand.signature = (accuser.sign)(message.as_byte_slice(), PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT)
                        .ok_or("no signing key for a held demand")?;
                    case.demanded = Some((i, now));
                    PalwGdnHeldNextV1::Demand {
                        unit: case.demands[i],
                        object: PalwConsensusObjectV2::DefaultAccusedHeld { accusation: Box::new(demand) },
                    }
                }
            });
        }
        // Every unit of this candidate is answered: the evidence, held to the court's own terminal before it is signed.
        let (node_slot, head) = case.candidates[case.at];
        let answers: Vec<PalwHeldDisclosureV1> = case.answers.iter().map(|a| a.clone().expect("all answered")).collect();
        let refutation = base0_gdn_transition_evidence_from_disclosures_v2(
            material,
            node_slot,
            head,
            case.index,
            &case.demands,
            &answers,
            &mut kernels,
        );
        if let Ok(refutation) = refutation
            && check_gdn_transition_refutation_v2(
                &refutation,
                case.claim.execution_root,
                case.claim.shape_profile_id,
                accuser.oracle,
                material.max_leaves,
            )
            .is_ok()
        {
            let mut accusation = PalwMemoryRoutingAccusationV2 {
                version: PALW_MEMORY_ROUTING_ACCUSATION_VERSION_V2,
                claim: case.claim.claim,
                execution_root: case.claim.execution_root,
                trace_root: case.claim.trace_root,
                executor_bond: case.claim.executor,
                accuser_bond: accuser.bond,
                evidence: PalwMemoryRoutingEvidenceV2::GdnTransition { refutation },
                artifact_openings: accuser.openings.to_vec(),
                signature: Vec::new(),
            };
            let session = palw_memory_routing_session_id_v2(accuser.network_domain, &accusation);
            accusation.signature = (accuser.sign)(session.as_byte_slice(), PALW_MEMORY_ROUTING_MLDSA87_CONTEXT_V2)
                .ok_or("no signing key for a GDN accusation")?;
            return Ok(PalwGdnHeldNextV1::Accuse {
                object: PalwConsensusObjectV2::MemoryRoutingV2 {
                    object: Box::new(PalwMemoryRoutingObjectV2::Accused { accusation }),
                },
            });
        }
        // This candidate's answered bytes are honest (the terminal finds no fault): the next recurrence (node slot, head).
        case.at += 1;
        if case.at >= case.candidates.len() {
            return Ok(PalwGdnHeldNextV1::Exhausted);
        }
        load_candidate_v1(case, material, &mut kernels)?;
    }
}

// ---- the node's side: one book of cases, one tick -----------------------------------------------------------------------------

/// **What the filer needs of the node** — a seam over the service, so the tick runs against a fixture in the tests.
pub trait PalwGdnHeldHostV1 {
    /// Held hybrid claims this node's bond may accuse (not its own, not final or voided), with their chain facts.
    fn accusable_claims(&self) -> Vec<PalwGdnHeldClaimV1>;
    /// The accuser's view of the claim's PUBLIC served material (never the producer's heap), or why there is none yet.
    fn material(&self, claim: &PalwGdnHeldClaimV1) -> Result<Base0GdnOwnedMaterialV2, String>;
    /// Fresh recompute kernels of the claim's class (the node's own resolve door).
    fn kernels(&self, claim: &PalwGdnHeldClaimV1) -> Result<Box<dyn Base0FpRecomputeKernelsV1 + '_>, String>;
    /// Every accepted object naming the claim (its demands' answers among them).
    fn objects(&self, claim: &PalwGdnHeldClaimV1) -> Vec<PalwConsensusObjectV2>;
    /// The class inventory's openings and the oracle over them.
    fn openings(&self, claim: &PalwGdnHeldClaimV1) -> Result<(Vec<PalwArtifactOpeningV1>, PalwProvenOperandsV1), String>;
    fn disclose_window_daa(&self) -> u64;
    fn bond(&self) -> PalwBondKeyV2;
    fn network_domain(&self) -> Hash64;
    fn sign(&self, message: &[u8], context: &[u8]) -> Option<Vec<u8>>;
}

/// The filer's memory across ticks: a case per claim, and the claims settled (honest, accused, defaulted or exhausted).
#[derive(Default)]
pub struct PalwGdnHeldBookV1 {
    pub cases: std::collections::BTreeMap<Hash64, PalwGdnHeldCaseV1>,
    pub settled: std::collections::BTreeMap<Hash64, &'static str>,
}

/// One object for the priority lane: `(claim, object)`.
pub type PalwGdnHeldOutV1 = (Hash64, PalwConsensusObjectV2);

/// The host's boxed kernels, as the base0 builders take them.
pub struct PalwGdnBoxedKernelsV1<'a>(pub Box<dyn Base0FpRecomputeKernelsV1 + 'a>);

impl Base0FpRecomputeKernelsV1 for PalwGdnBoxedKernelsV1<'_> {
    fn forward_no_capture(&mut self, token: usize, position: usize) -> Result<(), misaka_palw_base0::fp_recompute::Base0FpRecomputeError> {
        self.0.forward_no_capture(token, position)
    }
    fn state_chunks(
        &self,
        profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        positions: u32,
    ) -> Result<Vec<Vec<u8>>, misaka_palw_base0::fp_recompute::Base0FpRecomputeError> {
        self.0.state_chunks(profile, positions)
    }
}

/// **One tick**: open a case for every accusable claim not yet seen (the accuser's own recompute decides), step every open case,
/// and return the objects to carry. A claim whose recompute finds every checkpoint honest is settled `honest` and never demanded.
pub fn palw_gdn_held_filer_tick_v1<H: PalwGdnHeldHostV1>(host: &H, book: &mut PalwGdnHeldBookV1, now: u64) -> Vec<PalwGdnHeldOutV1> {
    let mut out = Vec::new();
    let claims = host.accusable_claims();
    let live: std::collections::BTreeSet<Hash64> = claims.iter().map(|c| c.claim).collect();
    book.cases.retain(|claim, _| live.contains(claim));
    for claim in claims {
        if book.settled.contains_key(&claim.claim) || claim.executor == host.bond() {
            continue;
        }
        let Ok(owned) = host.material(&claim) else { continue };
        let view = owned.view();
        if host.kernels(&claim).is_err() {
            continue;
        }
        let kernels = || PalwGdnBoxedKernelsV1(host.kernels(&claim).expect("the class resolved above"));
        if !book.cases.contains_key(&claim.claim) {
            match palw_gdn_held_open_v1(claim.clone(), &view, kernels) {
                Ok(Some(case)) => {
                    book.cases.insert(claim.claim, case);
                }
                Ok(None) => {
                    book.settled.insert(claim.claim, "honest");
                    continue;
                }
                Err(_) => continue,
            }
        }
        let Ok((openings, oracle)) = host.openings(&claim) else { continue };
        let sign = |m: &[u8], c: &[u8]| host.sign(m, c);
        let accuser = PalwGdnHeldAccuserV1 { bond: host.bond(), network_domain: host.network_domain(), openings: &openings, oracle: &oracle, sign: &sign };
        let objects = host.objects(&claim);
        let case = book.cases.get_mut(&claim.claim).expect("opened above");
        match palw_gdn_held_step_v1(case, &view, kernels, &objects, now, host.disclose_window_daa(), &accuser) {
            Ok(PalwGdnHeldNextV1::Demand { object, .. }) => out.push((claim.claim, object)),
            Ok(PalwGdnHeldNextV1::Accuse { object }) => {
                out.push((claim.claim, object));
                book.settled.insert(claim.claim, "accused");
            }
            Ok(PalwGdnHeldNextV1::DefaultDue { .. }) => {
                book.settled.insert(claim.claim, "defaulted");
            }
            Ok(PalwGdnHeldNextV1::Exhausted) => {
                book.settled.insert(claim.claim, "exhausted");
            }
            Ok(PalwGdnHeldNextV1::Wait) | Err(_) => {}
        }
    }
    out
}

#[cfg(test)]
#[path = "palw_gdn_held_filer_tests.rs"]
pub(crate) mod tests;
