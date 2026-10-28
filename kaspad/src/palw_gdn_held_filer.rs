//! **RFC-0006 06-A: the automatic held-GDN checkpoint filer** (2026-10-18; dormant with `palw_memory_routing_court_v2`, which every
//! preset leaves off). A held (per-position) hybrid class serves no checkpoint chunks, so a forged recurrence checkpoint is reached
//! only through the producer's own bytes. Any bond outside the claim's producer runs this, from public material and its own replay:
//!
//! ```text
//! recompute   the accuser's OWN prefix walk finds the first committed checkpoint that is not the job's state
//!             (`base0_gdn_first_forged_checkpoint_retained_v3`); none → nothing to file, an honest producer is never demanded
//! demand      for one supported checkpoint terminal/head at a time, the exact `StateChunk` units that terminal reads
//!             (`base0_gdn_forged_checkpoint_demands_retained_v3`), each a signed `DefaultAccusedHeld` — one open at a time
//! answers     the producer's `MaterialDisclosedV2` / `MaterialDisclosedHeld` on chain, re-checked by the held DA court's own
//!             hash check (`palw_held_da_check_disclosure_v1`); nothing is taken on the carrier's word
//! accuse      once every unit is answered: transition or convolution-window evidence, held to the matching court terminal
//!             before anything is signed; a candidate whose authenticated answered
//!             bytes are honest moves on to the next supported terminal/head; a fault is filed as tag 244 `MemoryRoutingV2::Accused`
//!             (`GdnTransition` or `GdnConvWindow`), signed over `palw_memory_routing_session_id_v2`
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
    palw_held_da_accusation_message_v1,
};
use kaspa_consensus_core::palw_memory_routing_v2::{
    PALW_MEMORY_ROUTING_ACCUSATION_VERSION_V2, PALW_MEMORY_ROUTING_MLDSA87_CONTEXT_V2, PalwMemoryRoutingAccusationV2,
    PalwMemoryRoutingEvidenceV2, PalwMemoryRoutingObjectV2, palw_memory_routing_session_id_v2,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::palw_step::kernel_semantics_id_v1;
#[cfg(test)]
use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;
use kaspa_consensus_core::palw_step_refute::memory_v2::{
    PalwGdnCourtErrorV2, check_gdn_conv_window_refutation_v2, check_gdn_transition_refutation_v2,
};
#[cfg(test)]
use kaspa_consensus_core::palw_step_refute::memory_v2::{gdn_conv_window_demands_v2, gdn_transition_demands_v2};
use kaspa_consensus_core::palw_step_refute::{KDESC_Q36_GDN_STEP, KDESC_Q36_SSM_CONV};
use kaspa_hashes::Hash64;
use misaka_palw_base0::GdnInventoryOracleV2;
use misaka_palw_base0::fp_recompute::Base0FpRecomputeKernelsV1;
use misaka_palw_base0::gdn_evidence_v2::{
    Base0GdnCheckpointCourtV2, Base0GdnMaterialV2, Base0GdnOwnedMaterialV2,
    base0_gdn_conv_window_evidence_from_disclosures_retained_v4, base0_gdn_first_forged_checkpoint_retained_v3,
    base0_gdn_forged_checkpoint_demands_retained_v3, base0_gdn_transition_evidence_from_disclosures_retained_v4,
};
#[cfg(test)]
use misaka_palw_base0::gdn_evidence_v2::{
    base0_gdn_conv_window_evidence_from_disclosures_retained_v3, base0_gdn_transition_evidence_from_disclosures_retained_v3,
};

#[path = "palw_gdn_held_action_owner_v2.rs"]
mod action_owner;
pub use action_owner::{PalwGdnHeldErrorV2, PalwGdnHeldNextRetainedV2};
#[path = "palw_gdn_held_answer_owner_v3.rs"]
mod answer_owner;
#[path = "palw_gdn_held_geometry_owner_v4.rs"]
mod geometry_owner;
use kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1;

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
#[derive(Debug)]
pub struct PalwGdnHeldCaseV1 {
    pub claim: PalwGdnHeldClaimV1,
    /// The first forged checkpoint (the accuser's own recompute).
    pub index: u32,
    /// Recurrence transitions first, then Q36 convolution windows, in slot/head order.
    pub candidates: Vec<Base0GdnCheckpointCourtV2>,
    // Kept so the geometry owner drops with the case. Callers do not read it.
    #[allow(dead_code)]
    candidate_owner: geometry_owner::GeometryOwnerV4,
    /// The candidate under demand.
    pub at: usize,
    /// Its units and the answers found so far.
    pub demands: Vec<PalwHeldMissingV1>,
    demand_owner: Option<geometry_owner::GeometryOwnerV4>,
    pub answers: Vec<Option<NativeRetainedV1<PalwHeldDisclosureV1>>>,
    // Values drop before the reservation for their persistent Option slots.
    answer_slots: Option<answer_owner::AnswerSlotsV3>,
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
    /// Every candidate's authenticated terminal found no fault: nothing to file.
    Exhausted,
}

fn retained_leg_error_v3(error: kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1<String>) -> PalwGdnHeldErrorV2 {
    use kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1;
    match error {
        NativeDecodeErrorV1::Resource(why) => PalwGdnHeldErrorV2::Resource(why),
        NativeDecodeErrorV1::Decode(why) => PalwGdnHeldErrorV2::Operation(why),
    }
}

/// **Open a case**, or `None` when the accuser's own recompute finds every committed checkpoint honest.
pub fn palw_gdn_held_open_retained_v3<K: Base0FpRecomputeKernelsV1>(
    claim: PalwGdnHeldClaimV1,
    material: &Base0GdnMaterialV2<'_>,
    mut kernels: impl FnMut() -> K,
) -> Result<Option<PalwGdnHeldCaseV1>, PalwGdnHeldErrorV2> {
    if material.binding.committed_execution_root != claim.execution_root {
        return Err("the material's binding is not the claim's execution".into());
    }
    let Some(index) = base0_gdn_first_forged_checkpoint_retained_v3(material, kernels()).map_err(retained_leg_error_v3)? else {
        return Ok(None);
    };
    let (candidates, candidate_owner) = geometry_owner::candidates(material.binding)?;
    if candidates.is_empty() {
        return Err(format!("checkpoint {index} is forged but no supported recurrence or convolution terminal reads it").into());
    }
    let mut case = PalwGdnHeldCaseV1 {
        claim,
        index,
        candidates,
        candidate_owner,
        at: 0,
        demands: Vec::new(),
        demand_owner: None,
        answers: Vec::new(),
        answer_slots: None,
        demanded: None,
    };
    load_candidate_v1(&mut case, 0, material, &mut kernels)?;
    Ok(Some(case))
}

/// Compatibility error interface; the returned case still retains its answers.
pub fn palw_gdn_held_open_v1<K: Base0FpRecomputeKernelsV1>(
    claim: PalwGdnHeldClaimV1,
    material: &Base0GdnMaterialV2<'_>,
    kernels: impl FnMut() -> K,
) -> Result<Option<PalwGdnHeldCaseV1>, String> {
    palw_gdn_held_open_retained_v3(claim, material, kernels).map_err(|why| why.to_string())
}

fn load_candidate_v1<K: Base0FpRecomputeKernelsV1>(
    case: &mut PalwGdnHeldCaseV1,
    at: usize,
    material: &Base0GdnMaterialV2<'_>,
    kernels: &mut impl FnMut() -> K,
) -> Result<(), PalwGdnHeldErrorV2> {
    let candidate = *case.candidates.get(at).ok_or("GDN candidate outside the case")?;
    // Covers the pure helper's returned demands, selected-index and layer-list
    // arrays. Earlier Base0 leg/prefix work remains a separate input boundary.
    let admission = geometry_owner::DemandAdmissionV4::prepare(material.binding, candidate)?;
    let demands = base0_gdn_forged_checkpoint_demands_retained_v3(material, candidate, case.index, &mut *kernels)
        .map_err(retained_leg_error_v3)?;
    let owner = admission.finish(&demands, demands.capacity())?;
    let (answers, slots) = answer_owner::slots(demands.len())?;
    // Refused construction must not pair the next candidate with the old
    // candidate's answers on a retry.
    case.at = at;
    case.demands = demands;
    case.demand_owner = Some(owner);
    case.answers = answers;
    case.answer_slots = Some(slots);
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
) -> Result<Option<NativeRetainedV1<PalwHeldDisclosureV1>>, kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1> {
    use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1};
    use kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1;
    for object in objects {
        let carriage = match object {
            PalwConsensusObjectV2::MaterialDisclosedV2 {
                claim: c,
                unit: PalwDaUnitV1::Held(missing),
                answer: PalwDaAnswerV1::Held(carriage),
                ..
            } if *c == claim.claim && missing == unit => carriage.as_ref(),
            PalwConsensusObjectV2::MaterialDisclosedHeld { disclosure }
                if disclosure.claim == claim.claim && disclosure.missing == *unit =>
            {
                disclosure.as_ref()
            }
            _ => continue,
        };
        let checked = kaspa_consensus_core::palw_held_da_v1::palw_held_da_state_chunk_auth_request_v2(
            &claim.execution_root,
            unit,
            binding,
            &carriage.disclosure,
            max_leaves,
            form,
        )
        .and_then(|request| request.check_v2());
        match checked {
            Ok(()) => return NativeRetainedV1::try_from_clone_v1(&carriage.disclosure).map(Some),
            Err(NativeDecodeErrorV1::Decode(_)) => continue,
            Err(NativeDecodeErrorV1::Resource(capacity)) => return Err(capacity),
        }
    }
    Ok(None)
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

/// A refused terminal is not an authenticated honest candidate. The caller
/// retains the current candidate/answers so it can retry rather than settle it.
fn terminal_found_fault_v1<T>(verdict: Result<T, PalwGdnCourtErrorV2>) -> Result<bool, PalwGdnHeldErrorV2> {
    misaka_palw_base0::gdn_evidence_v2::base0_gdn_terminal_fault_v3(verdict).map_err(retained_leg_error_v3)
}

/// **One step of the case** against the chain's objects at DAA `now` (`disclose_window`: the held DA court's answer window).
pub fn palw_gdn_held_step_retained_v2<K: Base0FpRecomputeKernelsV1>(
    case: &mut PalwGdnHeldCaseV1,
    material: &Base0GdnMaterialV2<'_>,
    mut kernels: impl FnMut() -> K,
    objects: &[PalwConsensusObjectV2],
    now: u64,
    disclose_window: u64,
    accuser: &PalwGdnHeldAccuserV1<'_>,
) -> Result<PalwGdnHeldNextRetainedV2, PalwGdnHeldErrorV2> {
    if accuser.bond == case.claim.executor {
        return Err("the producer never accuses its own claim".into());
    }
    loop {
        note_answers_v1(case, material, objects)?;
        if let Some(i) = case.answers.iter().position(Option::is_none) {
            return Ok(match case.demanded {
                Some((open, at)) if open == i && now > at.saturating_add(disclose_window) => {
                    PalwGdnHeldNextRetainedV2::DefaultDue { unit: case.demands[i] }
                }
                Some((open, _)) if open == i => PalwGdnHeldNextRetainedV2::Wait,
                _ => {
                    let object = action_owner::demand(&case.claim, material, case.demands[i], accuser)?;
                    PalwGdnHeldNextRetainedV2::Demand { index: i, unit: case.demands[i], object }
                }
            });
        }
        // Every unit of this candidate is answered: the evidence, held to the court's own terminal before it is signed.
        let answers = answer_owner::complete_batch(&case.answers)?;
        let evidence = match case.candidates[case.at] {
            Base0GdnCheckpointCourtV2::Transition { node_slot, head } => base0_gdn_transition_evidence_from_disclosures_retained_v4(
                material,
                node_slot,
                head,
                case.index,
                &case.demands,
                &answers,
                &mut kernels,
            )
            .map_err(retained_leg_error_v3)?
            .map_v4(|refutation| PalwMemoryRoutingEvidenceV2::GdnTransition { refutation }),
            Base0GdnCheckpointCourtV2::ConvWindow { conv_slot, head } => base0_gdn_conv_window_evidence_from_disclosures_retained_v4(
                material,
                conv_slot,
                head,
                case.index,
                &case.demands,
                &answers,
            )
            .map_err(retained_leg_error_v3)?
            .map_v4(|refutation| PalwMemoryRoutingEvidenceV2::GdnConvWindow { refutation }),
        };
        // The accused-witness input guard spans both the actual native court
        // and signed-output preadmission. The output's independent ticket is
        // acquired before the callback returns and this input guard retires.
        let object = evidence.try_handoff_v4(|evidence| {
            let verdict = match &evidence {
                PalwMemoryRoutingEvidenceV2::GdnTransition { refutation } => check_gdn_transition_refutation_v2(
                    refutation,
                    case.claim.execution_root,
                    case.claim.shape_profile_id,
                    accuser.oracle,
                    material.max_leaves,
                ),
                PalwMemoryRoutingEvidenceV2::GdnConvWindow { refutation } => check_gdn_conv_window_refutation_v2(
                    refutation,
                    case.claim.execution_root,
                    case.claim.shape_profile_id,
                    material.max_leaves,
                ),
                _ => unreachable!("held GDN candidates use checkpoint terminals"),
            };
            if terminal_found_fault_v1(verdict)? {
                action_owner::accusation(&case.claim, evidence, accuser).map(Some)
            } else {
                Ok(None)
            }
        })?;
        if let Some(object) = object {
            return Ok(PalwGdnHeldNextRetainedV2::Accuse { object });
        }
        // Only an authenticated NoFaultFound advances. Missing material,
        // resource refusal or invalid evidence keeps the case available to retry.
        let next = case.at + 1;
        if next >= case.candidates.len() {
            return Ok(PalwGdnHeldNextRetainedV2::Exhausted);
        }
        load_candidate_v1(case, next, material, &mut kernels)?;
    }
}

// ---- the node's side: one book of cases, one tick -----------------------------------------------------------------------------

/// Legacy raw-object interface. Production uses the retained step and atomic
/// handoff below; callers extracting raw objects own their destination boundary.
pub fn palw_gdn_held_step_v1<K: Base0FpRecomputeKernelsV1>(
    case: &mut PalwGdnHeldCaseV1,
    material: &Base0GdnMaterialV2<'_>,
    kernels: impl FnMut() -> K,
    objects: &[PalwConsensusObjectV2],
    now: u64,
    disclose_window: u64,
    accuser: &PalwGdnHeldAccuserV1<'_>,
) -> Result<PalwGdnHeldNextV1, String> {
    use PalwGdnHeldNextRetainedV2 as N;
    Ok(
        match palw_gdn_held_step_retained_v2(case, material, kernels, objects, now, disclose_window, accuser)
            .map_err(|e| e.to_string())?
        {
            N::Demand { index, unit, object } => {
                case.demanded = Some((index, now));
                PalwGdnHeldNextV1::Demand { unit, object: object.into_parts_v1().0 }
            }
            N::Accuse { object } => PalwGdnHeldNextV1::Accuse { object: object.into_parts_v1().0 },
            N::Wait => PalwGdnHeldNextV1::Wait,
            N::DefaultDue { unit } => PalwGdnHeldNextV1::DefaultDue { unit },
            N::Exhausted => PalwGdnHeldNextV1::Exhausted,
        },
    )
}

/// **What the filer needs of the node** — a seam over the service, so the tick runs against a fixture in the tests.
pub trait PalwGdnHeldHostV1 {
    /// Held hybrid claims this node's bond may accuse (not its own, not final or voided), with their chain facts.
    fn accusable_claims(&self) -> Vec<PalwGdnHeldClaimV1>;
    /// The accuser's view of the claim's PUBLIC served material (never the producer's heap), or why there is none yet.
    fn material(
        &self,
        claim: &PalwGdnHeldClaimV1,
    ) -> Result<kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<Base0GdnOwnedMaterialV2>, String>;
    /// Fresh recompute kernels of the claim's class (the node's own resolve door).
    fn kernels(&self, claim: &PalwGdnHeldClaimV1) -> Result<Box<dyn Base0FpRecomputeKernelsV1 + '_>, String>;
    /// Every accepted object naming the claim (its demands' answers among them).
    fn objects(&self, claim: &PalwGdnHeldClaimV1) -> Vec<PalwConsensusObjectV2>;
    /// The class inventory's openings and the oracle over them.
    fn openings(&self, claim: &PalwGdnHeldClaimV1) -> Result<GdnInventoryOracleV2, String>;
    /// Complete accepted-history watermark. None holds moves while a page scan is incomplete.
    fn observed_through_daa(&self, now: u64) -> Option<u64> {
        Some(now)
    }
    fn disclose_window_daa(&self) -> u64;
    fn bond(&self) -> PalwBondKeyV2;
    fn network_domain(&self) -> Hash64;
    fn sign(&self, message: &[u8], context: &[u8]) -> Option<Vec<u8>>;
}

#[derive(Clone, Debug)]
pub(crate) struct PalwGdnHistoryWalkV1 {
    pub anchor: Hash64,
    pub anchor_daa: u64,
    pub floor: u64,
    pub next: Option<Hash64>,
    pub wanted: std::collections::HashSet<Hash64>,
}

/// The filer's memory across ticks: a case per claim, and the claims settled (honest, accused, defaulted or exhausted).
#[derive(Default)]
pub struct PalwGdnHeldBookV1 {
    pub(crate) history: Option<PalwGdnHistoryWalkV1>,
    pub(crate) history_failed_claims: std::collections::HashSet<Hash64>,
    pub cases: std::collections::BTreeMap<Hash64, PalwGdnHeldCaseV1>,
    pub settled: std::collections::BTreeMap<Hash64, &'static str>,
}

/// One object for the priority lane: `(claim, object)`.
pub type PalwGdnHeldOutV1 = (Hash64, PalwConsensusObjectV2);

fn note_answers_v1(
    case: &mut PalwGdnHeldCaseV1,
    material: &Base0GdnMaterialV2<'_>,
    objects: &[PalwConsensusObjectV2],
) -> Result<(), kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1> {
    for (i, unit) in case.demands.iter().enumerate() {
        if case.answers[i].is_none() {
            case.answers[i] =
                answer_on_chain_v1(objects, &case.claim, material.binding, unit, material.max_leaves, material.prompt_ids_form)?;
        }
    }
    Ok(())
}

/// The host's boxed kernels, as the base0 builders take them.
pub struct PalwGdnBoxedKernelsV1<'a>(pub Box<dyn Base0FpRecomputeKernelsV1 + 'a>);

impl Base0FpRecomputeKernelsV1 for PalwGdnBoxedKernelsV1<'_> {
    fn forward_no_capture(
        &mut self,
        token: usize,
        position: usize,
    ) -> Result<(), misaka_palw_base0::fp_recompute::Base0FpRecomputeError> {
        self.0.forward_no_capture(token, position)
    }
    fn state_chunks(
        &self,
        profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        positions: u32,
    ) -> Result<Vec<Vec<u8>>, misaka_palw_base0::fp_recompute::Base0FpRecomputeError> {
        self.0.state_chunks(profile, positions)
    }
    fn state_chunks_snapshot_retained_v4(
        &self,
        profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        positions: u32,
    ) -> Result<Vec<Vec<u8>>, kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
        self.0.state_chunks_snapshot_retained_v4(profile, positions)
    }
}

/// **One tick**: open a case for every accusable claim not yet seen (the accuser's own recompute decides), step every open case,
/// and return the objects to carry. A claim whose recompute finds every checkpoint honest is settled `honest` and never demanded.
pub fn palw_gdn_held_filer_emit_v2<H: PalwGdnHeldHostV1>(
    host: &H,
    book: &mut PalwGdnHeldBookV1,
    now: u64,
    mut emit: impl FnMut(
        Hash64,
        &kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1<PalwConsensusObjectV2>,
    ) -> Result<(), kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1>,
) {
    let claims = host.accusable_claims();
    let live: std::collections::BTreeSet<Hash64> = claims.iter().map(|c| c.claim).collect();
    if let Some(history) = &book.history {
        for claim in &history.wanted {
            if !live.contains(claim) && !book.settled.contains_key(claim) {
                book.history_failed_claims.insert(*claim);
            }
        }
    }
    book.cases.retain(|claim, _| live.contains(claim));
    for claim in claims {
        if book.settled.contains_key(&claim.claim) || claim.executor == host.bond() {
            continue;
        }
        let Ok(owned) = host.material(&claim) else {
            if book.history.is_some() {
                book.history_failed_claims.insert(claim.claim);
            }
            continue;
        };
        let view = owned.view();
        if host.kernels(&claim).is_err() {
            if book.history.is_some() {
                book.history_failed_claims.insert(claim.claim);
            }
            continue;
        }
        let kernels = || PalwGdnBoxedKernelsV1(host.kernels(&claim).expect("the class resolved above"));
        if let std::collections::btree_map::Entry::Vacant(e) = book.cases.entry(claim.claim) {
            match palw_gdn_held_open_retained_v3(claim.clone(), &view, kernels) {
                Ok(Some(case)) => {
                    e.insert(case);
                }
                Ok(None) => {
                    book.settled.insert(claim.claim, "honest");
                    continue;
                }
                Err(_) => {
                    if book.history.is_some() {
                        book.history_failed_claims.insert(claim.claim);
                    }
                    continue;
                }
            }
        }
        let objects = host.objects(&claim);
        let case = book.cases.get_mut(&claim.claim).expect("opened above");
        let Some(observed_daa) = host.observed_through_daa(now).filter(|_| !book.history_failed_claims.contains(&claim.claim)) else {
            if let Err(why) = note_answers_v1(case, &view, &objects) {
                kaspa_core::debug!("held GDN answer storage pending: {why}");
            }
            continue;
        };
        let Ok(input) = host.openings(&claim) else { continue };
        let sign = |m: &[u8], c: &[u8]| host.sign(m, c);
        let accuser = PalwGdnHeldAccuserV1 {
            bond: host.bond(),
            network_domain: host.network_domain(),
            openings: input.openings(),
            oracle: input.oracle(),
            sign: &sign,
        };
        match palw_gdn_held_step_retained_v2(case, &view, kernels, &objects, observed_daa, host.disclose_window_daa(), &accuser) {
            Ok(PalwGdnHeldNextRetainedV2::Demand { index, object, .. }) => {
                if emit(claim.claim, &object).is_err() {
                    continue;
                }
                // A backfilled observation is older than this submission. Its
                // clock must not shorten a newly queued demand's answer window.
                case.demanded = Some((index, now.max(observed_daa)));
            }
            Ok(PalwGdnHeldNextRetainedV2::Accuse { object }) => {
                if emit(claim.claim, &object).is_err() {
                    continue;
                }
                book.settled.insert(claim.claim, "accused");
            }
            Ok(PalwGdnHeldNextRetainedV2::DefaultDue { .. }) => {
                book.settled.insert(claim.claim, "defaulted");
            }
            Ok(PalwGdnHeldNextRetainedV2::Exhausted) => {
                book.settled.insert(claim.claim, "exhausted");
            }
            Ok(PalwGdnHeldNextRetainedV2::Wait) | Err(_) => {}
        }
    }
}

/// Compatibility raw collection for historical fixtures. The actual service
/// retains action ownership and supplies a fallible destination admission.
pub fn palw_gdn_held_filer_tick_v1<H: PalwGdnHeldHostV1>(host: &H, book: &mut PalwGdnHeldBookV1, now: u64) -> Vec<PalwGdnHeldOutV1> {
    let mut out = Vec::new();
    palw_gdn_held_filer_emit_v2(host, book, now, |claim, object| {
        out.push((claim, (**object).clone()));
        Ok(())
    });
    out
}

#[cfg(test)]
#[path = "palw_gdn_held_filer_tests.rs"]
pub(crate) mod tests;
