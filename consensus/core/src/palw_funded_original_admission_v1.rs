//! **G14 item 1 (lane r5-original-admission, 2026-10-22): the funded original's all-branch reservation** — pure, DORMANT.
//! Written, not compiled (lane rule).
//!
//! RFC-0026 §4 / RFC-0028 / RFC-0035 A §3, §7.1: at original admission the coordinator derives, from the NATIVE preparation only
//! (frame custody → liable signers, the complete native source-material graph → every hostile source branch), the complete set of
//! service obligations, and reserves them ALL in one candidate of the rooted court service book (all-or-nothing). Nothing here is
//! a caller flag, a profile hash promoted to policy or a self-signed manifest:
//!
//! * **Branches** ([`FundedBranchV1`]): (1) each liable signer × each response turn the service profile allows (exactly
//!   court-f02's canonical packets, same ids, so an ordinary F02 binding of the same claim is a prefix of this one); (2) each node
//!   of the complete native [`NativeSourceMaterialGraphV3`] (all query families × role slots: the hostile source branches), with
//!   its causal predecessors; (3) one retention/heap holding packet of the original.
//! * **Resources** ([`FundedResourceDemandV1`]): CPU work / IO / transfer bytes are the reserved packets' deterministic costs; heap
//!   and retention bytes are per-branch schedule values × branch count. The heap+retention total rides the retention packet, so it
//!   is held in the SAME rooted book (no new table, no new borsh layout).
//! * **Capital** ([`funded_required_amount_v1`]): required capital = checked Σ over branches; the escrow must cover
//!   `fee + (capital − producer deposit already held by the native work room)`, with the fee counted ONCE (RFC-0035 §7.1).
//! * **Liability** ([`funded_liability_resolved_v1`]): the exact predicate under which the escrow row's `liability_open` closes.
//!
//! Every numeric value comes from [`FundedAdmissionScheduleV1`], whose only constructor is `#[cfg(test)]` (typed-missing in
//! production, like the rest of the funded-original schedule; RFC-0035 §4).
use crate::{
    Hash64,
    palw_court_policy_v1::DISPUTE_WINDOW_DAA,
    palw_court_service_binding_v1::{CourtServiceBindingRefusalV1, CourtServiceProfileV1, court_service_binding_candidate_v1, court_service_packets_v1},
    palw_court_service_state_v2::CourtServiceStateV2,
    palw_court_service_v1::{ServiceAssessment, ServiceCost, ServicePacket},
    palw_court_source_material_graph_v3::NativeSourceMaterialGraphV3,
    palw_state_v2::PalwBondKeyV2,
};
use std::collections::BTreeSet;

/// Lane VAL estimates (not wired): `crate::palw_g14_values_v1` (V2).
/// **The unadopted admission values** (service profile, per-branch costs and capital). Private fields; the only constructor is
/// `#[cfg(test)]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FundedAdmissionScheduleV1 {
    service: CourtServiceProfileV1,
    source_packet_cost: ServiceCost,
    capital_per_turn_sompi: u64,
    capital_per_source_node_sompi: u64,
    heap_bytes_per_branch: u64,
    retention_bytes_per_branch: u64,
}

impl FundedAdmissionScheduleV1 {
    /// **TEST ONLY.** No production constructor, deserializer or `Default`.
    #[cfg(test)]
    pub fn test_only_v1(
        service: CourtServiceProfileV1,
        source_packet_cost: ServiceCost,
        capital_per_turn_sompi: u64,
        capital_per_source_node_sompi: u64,
        heap_bytes_per_branch: u64,
        retention_bytes_per_branch: u64,
    ) -> Self {
        Self { service, source_packet_cost, capital_per_turn_sompi, capital_per_source_node_sompi, heap_bytes_per_branch, retention_bytes_per_branch }
    }
    /// **Production constructor (lane VAL, 2026-10-26)**: refuses an invalid service profile (zero budget, `controls != 1`, a packet
    /// larger than one DAA's budget, intervals reaching `a + 10,000`), a source packet larger than the budget, zero capital and
    /// holdings outside `1..=1 GiB`. Not read by any preset.
    pub fn new_v1(
        service: CourtServiceProfileV1,
        source_packet_cost: ServiceCost,
        capital_per_turn_sompi: u64,
        capital_per_source_node_sompi: u64,
        heap_bytes_per_branch: u64,
        retention_bytes_per_branch: u64,
    ) -> Result<Self, &'static str> {
        let b = service.budget;
        let fits = |c: ServiceCost| c.controls == 1 && c.bytes <= b.bytes && c.work <= b.work && c.io <= b.io;
        if service.version != 1 || b.bytes == 0 || b.work == 0 || b.io == 0 || b.controls == 0 || service.packets_per_signer == 0 {
            return Err("funded admission: invalid service profile");
        }
        if !fits(service.packet_cost) || !fits(source_packet_cost) {
            return Err("funded admission: a packet must fit one DAA's budget and carry one control");
        }
        let end = service.packets_per_signer.checked_mul(service.service_span_daa).and_then(|v| v.checked_add(1));
        if service.service_span_daa == 0 || end.is_none_or(|e| e > crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA) {
            return Err("funded admission: packet intervals must end before a + 10,000");
        }
        if capital_per_turn_sompi == 0 || capital_per_source_node_sompi == 0 {
            return Err("funded admission: capital must be positive");
        }
        const MAX_BRANCH_BYTES: u64 = 1 << 30;
        if !(1..=MAX_BRANCH_BYTES).contains(&heap_bytes_per_branch) || !(1..=MAX_BRANCH_BYTES).contains(&retention_bytes_per_branch) {
            return Err("funded admission: holdings per branch must be in 1..=1 GiB");
        }
        Ok(Self { service, source_packet_cost, capital_per_turn_sompi, capital_per_source_node_sompi, heap_bytes_per_branch, retention_bytes_per_branch })
    }
    pub fn service_profile(&self) -> CourtServiceProfileV1 {
        self.service
    }
}

/// One legal branch of the original the admission reserves for.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FundedBranchV1 {
    /// A liable signer's response turn (court-f02 canonical packet).
    SignerTurn { signer: PalwBondKeyV2, turn: u64 },
    /// A node of the complete native source-material graph (logical id; packet id is its absolute allocation packet id).
    SourceNode { logical: Hash64 },
    /// The original's retention / heap holding.
    Retention,
}

/// Deterministic resource demand reserved with the claim (never measured runtime).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FundedResourceDemandV1 {
    pub cpu_work: u64,
    pub heap_bytes: u64,
    pub io: u64,
    pub transfer_bytes: u64,
    pub retention_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FundedAdmissionRefusalV1 {
    Binding(CourtServiceBindingRefusalV1),
    SourceGraph(String),
    /// Heap/retention per branch must be positive (a holding packet with zero bytes is not a reservation).
    ScheduleInvalid,
    Overflow,
    /// The rooted book changed between the coordinator's check and the install (or an ordinary binding differs).
    BookMismatch,
    /// The derived packet universe and the reserved binding differ.
    BranchMismatch,
    /// A responsible actor has no current authority (bond not active).
    ActorWithoutAuthority,
}

/// The complete derivation. Packets are reserved by [`funded_admission_book_candidate_v1`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FundedAdmissionDerivationV1 {
    pub branches: Vec<(FundedBranchV1, [u8; 64])>,
    pub signer_packets: Vec<ServicePacket>,
    pub source_packets: Vec<ServicePacket>,
    pub retention_packet: ServicePacket,
    pub resources: FundedResourceDemandV1,
    /// Checked Σ of per-branch capital.
    pub capital_sompi: u64,
    pub hard_daa: u64,
}

impl FundedAdmissionDerivationV1 {
    pub fn branch_count(&self) -> u64 {
        self.branches.len() as u64
    }
    pub fn packet_ids(&self) -> BTreeSet<[u8; 64]> {
        self.branches.iter().map(|(_, id)| *id).collect()
    }
}

fn keyed(key: &[u8], bytes: &[u8]) -> [u8; 64] {
    blake2b_simd::Params::new().hash_length(64).key(key).hash(bytes).as_bytes().try_into().expect("64-byte digest")
}

/// The retention holding packet's identity (claim, allocation, source).
pub fn funded_retention_packet_id_v1(claim: Hash64, allocation: Hash64, source: Hash64) -> [u8; 64] {
    keyed(b"misaka-palw/funded-original-retention/v1", &borsh::to_vec(&(claim, allocation, source)).expect("id serialization"))
}

fn add(a: u64, b: u64) -> Result<u64, FundedAdmissionRefusalV1> {
    a.checked_add(b).ok_or(FundedAdmissionRefusalV1::Overflow)
}
fn mul(a: u64, b: u64) -> Result<u64, FundedAdmissionRefusalV1> {
    a.checked_mul(b).ok_or(FundedAdmissionRefusalV1::Overflow)
}

/// **Derive every legal branch of the original and its costs.** Pure; never truncated (any overflow/invalid interval refuses).
pub fn derive_funded_admission_v1(
    claim: Hash64,
    allocation: Hash64,
    source: Hash64,
    accepted_daa: u64,
    signers: &BTreeSet<PalwBondKeyV2>,
    graph: &NativeSourceMaterialGraphV3,
    schedule: &FundedAdmissionScheduleV1,
) -> Result<FundedAdmissionDerivationV1, FundedAdmissionRefusalV1> {
    use FundedAdmissionRefusalV1 as R;
    if schedule.heap_bytes_per_branch == 0
        || schedule.retention_bytes_per_branch == 0
        || schedule.source_packet_cost.controls != 1
        || schedule.source_packet_cost.bytes == 0
        || schedule.source_packet_cost.work == 0
    {
        return Err(R::ScheduleInvalid);
    }
    graph.check_v3().map_err(R::SourceGraph)?;
    if graph.source_v3() != source {
        return Err(R::SourceGraph("source graph names another source".into()));
    }
    let hard = accepted_daa.checked_add(DISPUTE_WINDOW_DAA).ok_or(R::Overflow)?;
    let first = accepted_daa.checked_add(1).ok_or(R::Overflow)?;
    // (1) Liable signer × turn: court-f02's canonical packets (same order: signer canonical order, then turn).
    let signer_packets = court_service_packets_v1(claim, allocation, source, accepted_daa, signers, &schedule.service).map_err(R::Binding)?;
    let mut branches = Vec::new();
    {
        let mut it = signer_packets.iter();
        for signer in signers {
            for turn in 0..schedule.service.packets_per_signer {
                let p = it.next().ok_or(R::BranchMismatch)?;
                branches.push((FundedBranchV1::SignerTurn { signer: *signer, turn }, p.id));
            }
        }
        if it.next().is_some() {
            return Err(R::BranchMismatch);
        }
    }
    // (2) Every node of the complete native source graph (all hostile source branches).
    let mut source_packets = Vec::new();
    for node in graph.nodes_v3() {
        let id = graph.absolute_packet_id_v3(node.id_v3());
        source_packets.push(ServicePacket { id, release_daa: first, service_before: hard, cost: schedule.source_packet_cost });
        branches.push((FundedBranchV1::SourceNode { logical: node.id_v3() }, id));
    }
    let service_branches = branches.len() as u64;
    // Resources and capital, checked.
    let mut r = FundedResourceDemandV1::default();
    for p in signer_packets.iter().chain(source_packets.iter()) {
        r.cpu_work = add(r.cpu_work, p.cost.work)?;
        r.io = add(r.io, p.cost.io)?;
        r.transfer_bytes = add(r.transfer_bytes, p.cost.bytes)?;
    }
    r.heap_bytes = mul(service_branches, schedule.heap_bytes_per_branch)?;
    r.retention_bytes = mul(service_branches, schedule.retention_bytes_per_branch)?;
    // (3) The retention/heap holding, in the same book.
    let retention_packet = ServicePacket {
        id: funded_retention_packet_id_v1(claim, allocation, source),
        release_daa: first,
        service_before: hard,
        cost: ServiceCost { bytes: add(r.heap_bytes, r.retention_bytes)?, work: 1, io: 0, controls: 1 },
    };
    r.cpu_work = add(r.cpu_work, 1)?;
    branches.push((FundedBranchV1::Retention, retention_packet.id));
    let capital_sompi = add(
        mul(signer_packets.len() as u64, schedule.capital_per_turn_sompi)?,
        mul(source_packets.len() as u64, schedule.capital_per_source_node_sompi)?,
    )?;
    if branches.iter().map(|(_, id)| *id).collect::<BTreeSet<_>>().len() != branches.len() {
        return Err(R::BranchMismatch);
    }
    Ok(FundedAdmissionDerivationV1 { branches, signer_packets, source_packets, retention_packet, resources: r, capital_sompi, hard_daa: hard })
}

/// **The escrow amount the payer must have signed for** (RFC-0035 §7.1 one fee allocation). `capital` = max(schedule floor,
/// derived Σ branch capital); the producer's full deposit (already held by the native work room at install, `install_original_v1`)
/// covers part of it; the fee is counted once and only from the escrow (the escrow output is output-side of the carrier, so it is
/// never also part of the carrier's fee `F`).
pub fn funded_required_amount_v1(fee: u64, capital_floor: u64, derived_capital: u64, bond_reserved: u64) -> Option<u64> {
    let capital = capital_floor.max(derived_capital);
    fee.checked_add(capital.saturating_sub(bond_reserved))
}

/// **Reserve every branch in one candidate book** (all-or-nothing on a clone). Returns `(after_signer_step, full)`: the first is
/// exactly what court-f02's ordinary binding would install for the same claim, so the install can accept a book where that
/// ordinary binding was just written and still extend it deterministically.
#[allow(clippy::too_many_arguments)]
pub fn funded_admission_book_candidate_v1(
    existing: Option<&CourtServiceStateV2>,
    claim: Hash64,
    allocation: Hash64,
    source: Hash64,
    accepted_daa: u64,
    signers: &BTreeSet<PalwBondKeyV2>,
    graph: &NativeSourceMaterialGraphV3,
    derivation: &FundedAdmissionDerivationV1,
    schedule: &FundedAdmissionScheduleV1,
) -> Result<(CourtServiceStateV2, CourtServiceStateV2), FundedAdmissionRefusalV1> {
    use FundedAdmissionRefusalV1 as R;
    let after_signer =
        court_service_binding_candidate_v1(existing, claim, allocation, source, accepted_daa, signers, &schedule.service).map_err(R::Binding)?;
    let mut full = after_signer.clone();
    let projection = graph.bind_projection_v3(derivation.source_packets.clone()).map_err(R::SourceGraph)?;
    projection.reserve_v3(&mut full.book, derivation.hard_daa).map_err(R::SourceGraph)?;
    full.book
        .reserve(std::slice::from_ref(&derivation.retention_packet), derivation.hard_daa)
        .map_err(|e| R::Binding(CourtServiceBindingRefusalV1::Reservation(e)))?;
    let binding = full.claims.get_mut(&claim).ok_or(R::BranchMismatch)?;
    binding.packet_ids.extend(derivation.source_packets.iter().map(|p| p.id));
    binding.packet_ids.insert(derivation.retention_packet.id);
    if binding.packet_ids != derivation.packet_ids() || binding.packet_ids.len() as u64 != derivation.branch_count() {
        return Err(R::BranchMismatch);
    }
    full.check_live_v1().map_err(R::SourceGraph)?;
    Ok((after_signer, full))
}

/// **G14 lane r6-d1-release: the causal edges of the funded source branches** — recorded with the all-branch book (in its
/// `CourtServiceRowV3` form) so the canonical recovery sweep never reschedules a source packet at or after one that depends on it.
pub fn funded_admission_causal_edges_v1(
    graph: &NativeSourceMaterialGraphV3,
    derivation: &FundedAdmissionDerivationV1,
) -> Result<std::collections::BTreeMap<[u8; 64], Vec<[u8; 64]>>, FundedAdmissionRefusalV1> {
    let projection = graph.bind_projection_v3(derivation.source_packets.clone()).map_err(FundedAdmissionRefusalV1::SourceGraph)?;
    Ok(projection.causal_edges_v3())
}

/// What the liability predicate reads for one funded row (all from rooted state at `daa`).
#[derive(Clone, Debug)]
pub struct FundedLiabilityViewV1 {
    pub daa: u64,
    pub hard_deadline_daa: u64,
    /// The original frame's material horizon; `None` once the frame was pruned past it.
    pub material_until_daa: Option<u64>,
    /// Every native duty clock of the claim: `(response_deadline_daa, material_until_daa)`.
    pub clocks: Vec<(u64, u64)>,
    /// The book's assessment of every packet of the claim's binding (empty when no binding exists).
    pub packets: Vec<ServiceAssessment>,
}

/// **The exact predicate that clears `liability_open`.** True iff ALL of:
/// 1. `daa >= hard_deadline_daa` (X3 hard: no court branch can open after it);
/// 2. retention discharged: `daa >= material_until_daa` of the frame (if the frame still exists);
/// 3. every duty clock of the claim is past both its response deadline and its material horizon;
/// 4. every reserved packet of the claim's binding is decided by the accepting chain: `OpportunityProvided` (served) or
///    `MissingSupply` (RFC-0026 / ADR-0192 A: no-slash — nobody can be charged for it after hard). `Pending` or
///    `UnknownObligation` keeps the liability open.
pub fn funded_liability_resolved_v1(v: &FundedLiabilityViewV1) -> bool {
    v.daa >= v.hard_deadline_daa
        && v.material_until_daa.is_none_or(|m| v.daa >= m)
        && v.clocks.iter().all(|(respond, material)| v.daa >= *respond && v.daa >= *material)
        && v.packets.iter().all(|a| matches!(a, ServiceAssessment::OpportunityProvided | ServiceAssessment::MissingSupply))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(daa: u64) -> FundedLiabilityViewV1 {
        FundedLiabilityViewV1 {
            daa,
            hard_deadline_daa: 10_100,
            material_until_daa: Some(10_200),
            clocks: vec![(10_050, 10_150)],
            packets: vec![ServiceAssessment::OpportunityProvided, ServiceAssessment::MissingSupply],
        }
    }

    #[test]
    fn r5_liability_predicate_needs_every_term() {
        assert!(!funded_liability_resolved_v1(&view(10_199)), "retention horizon not reached");
        assert!(funded_liability_resolved_v1(&view(10_200)));
        let mut v = view(10_200);
        v.hard_deadline_daa = 10_201;
        assert!(!funded_liability_resolved_v1(&v), "before hard");
        let mut v = view(10_200);
        v.clocks.push((10_201, 0));
        assert!(!funded_liability_resolved_v1(&v), "open duty clock");
        let mut v = view(10_200);
        v.packets.push(ServiceAssessment::Pending);
        assert!(!funded_liability_resolved_v1(&v), "pending packet");
        let mut v = view(10_200);
        v.packets.push(ServiceAssessment::UnknownObligation);
        assert!(!funded_liability_resolved_v1(&v), "unknown obligation");
        let mut v = view(10_200);
        v.material_until_daa = None;
        assert!(funded_liability_resolved_v1(&v), "pruned frame: its horizon already passed");
    }

    #[test]
    fn r5_required_amount_counts_fee_once_and_credits_held_deposit() {
        assert_eq!(funded_required_amount_v1(700, 300, 500, 0), Some(1_200));
        assert_eq!(funded_required_amount_v1(700, 300, 500, 200), Some(1_000));
        assert_eq!(funded_required_amount_v1(700, 300, 100, 1_000), Some(700), "deposit never refunds the fee");
        assert_eq!(funded_required_amount_v1(u64::MAX, 1, 1, 0), None);
    }
}
