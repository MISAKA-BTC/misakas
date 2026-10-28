//! **R6-CELL round 4 (2026-10-15): the coverage closure** — the dormant fence `palw_cell_coverage_closure_v1`, `None` on every
//! preset and refused when armed by [`Params::validate_palw_cell_coverage_closure_v1`]. It pays and weights nothing, so it is NOT in
//! `PALW_N3_GATED_REWARD_FENCES_V1`.
//!
//! **What it closes.** The R6-CELL checker ([`crate::palw_cell_coverage_v1`]) found the legacy step court fail-OPEN on a state chunk
//! map it cannot enumerate: `palw_step_leg::checkpoint_fault` reads `palw_state_chunk_count_at_v1`'s `None` ("this crate cannot
//! enumerate that map") as NO FAULT, and a version-1/2 profile may register ANY map id (`validate_shape` pins only version 3's). An
//! unknown map is therefore a class whose checkpoints no court can judge and whose disputes end in the producer's favour — a G14
//! soundness break. It also named four uncovered kinds ([`PalwCellUncoveredKindV1`]): the hybrid v1/v2 recurrence half, the solo
//! recurrence maps, multi-router tables and the fenced Kimi router.
//!
//! **The rule past the fence** (one predicate, [`palw_cell_closure_legacy_refusal_v1`]: the legacy checker answers `Err`):
//!
//! ```text
//! REQ-CELL-1  a ClassRegistered (tag 3) carrying a profile, folded at a DAA >= the fence, whose legacy coverage has any gap
//!             (an unknown map id, or any uncovered kind) is refused at registration (`object_preconditions_v1`; keyed on the
//!             class's registration DAA = the block's)
//! REQ-CELL-1b a claim ACCEPTED at a DAA >= the fence of a class whose graph the fold holds (`fp_work_profile_of`) and whose
//!             coverage has a gap is refused at the claim gate (`check_class_admits_claim`, code CELL_UNCOVERABLE) on every lane
//!             that gate serves (attempt, free-prompt, evaluation)
//! REQ-CELL-2  where admission cannot see the map (a class registered before the fence, or a genesis class, whose graph the fold
//!             does not hold), a court close of a claim accepted >= the fence whose binding (pinned to the claim's execution root
//!             and to the registered class) is uncoverable adjudicates ExecutorGuilty — objective, from the binding alone, never
//!             NoFaultFound / ChallengerDefeated (`adjudicate_court_close_v4`); the pure step-leg checker's closure form
//!             (`check_step_refutation_closure_capped_v1`) answers ShapeProfileNotCanonical for such a binding and never reads
//!             an unenumerable map's `None` as no fault
//! REQ-CELL-3  below the fence (every shipped network: the mirror is None) every predicate here answers "old rule": the same
//!             verdicts, writes and roots; a claim accepted before the fence keeps its old ruleset to its end
//! ```
//!
//! **Every uncovered kind is REFUSED AT ADMISSION, none covered by a new court path in this round** — each needs a verifier the court
//! does not have (see each [`PalwCellUncoveredKindV1`] variant): a class that wants the recurrence covered registers the v3/v4/v5
//! hybrid composition (which the court enumerates); a multi-router or Kimi-routed table has no unique router slot for a routed
//! product. TIR classes are outside this fence: their coverage is per descriptor (`palw_tir_cell_coverage_v1`), and no shipped
//! fixture has a gap.

use crate::config::params::{ForkActivation, Params};
use crate::palw_cell_coverage_v1::{PalwCellCoverageGapV1, PalwCellUncoveredKindV1, palw_legacy_cell_coverage_v1};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::PalwStateParamsV2;
use crate::palw_step::PalwShapeProfileV3;

/// The refusal code the claim gate and the registration refusal report (`PalwStateV2Error::ClassNotRewardable` /
/// `PalwStateV2Error::CellCoverageRefused`).
pub const PALW_CELL_UNCOVERABLE_CODE_V1: &str = "CELL_UNCOVERABLE";

/// **Why a legacy class is not coverable** past the fence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCellClosureRefusalV1 {
    /// A boundary the checker enumerated and found no verifier for.
    Uncovered(PalwCellUncoveredKindV1),
    /// A profile kind the checker does not know — an unknown state chunk map id, an unknown profile version: fail closed.
    UnknownProfileKind,
}

impl PalwCellClosureRefusalV1 {
    /// The refusal, by name (stable text; the fold's error carries it).
    pub fn why(self) -> &'static str {
        use PalwCellUncoveredKindV1 as K;
        match self {
            Self::UnknownProfileKind => "a state chunk map id or profile kind the court does not know: no court can enumerate it",
            Self::Uncovered(K::HybridRecurrenceUnenumerated) => {
                "a hybrid v1/v2 map: its recurrence half is not enumerated by the court (register the v3/v4/v5 composition)"
            }
            Self::Uncovered(K::SoloRecurrenceMap) => "a standalone recurrence map: its geometry is the executor's, not the court's",
            Self::Uncovered(K::MapSectionNotSpelled) => "the registered map does not name a memory section this profile needs",
            Self::Uncovered(K::MapNotEnumerable) => {
                "the registered map's chunk count is not derivable at a checkpoint of this profile"
            }
            Self::Uncovered(K::MultiRouterTable) => "more than one router in a layer table: routed products are unadjudicable",
            Self::Uncovered(K::KimiRouter) => "a Kimi router: its routed products resolve no router slot and are unadjudicable",
            Self::Uncovered(K::TirFamilyUnchecked | K::TirCarryNotCommitted | K::TirStateUnchecked) => {
                "an IR boundary without a verifier"
            }
        }
    }
}

/// **The one predicate**: `None` exactly when [`palw_legacy_cell_coverage_v1`] covers every boundary of `profile`; otherwise the
/// refusal — an unknown kind first (fail closed by name), else the first uncovered boundary's kind (the checker's deterministic
/// order: layers ascending, then memory sections).
pub fn palw_cell_closure_legacy_refusal_v1(profile: &PalwShapeProfileV3) -> Option<PalwCellClosureRefusalV1> {
    let gaps = palw_legacy_cell_coverage_v1(profile).err()?;
    if gaps.iter().any(|g| matches!(g, PalwCellCoverageGapV1::UnknownProfileKind { .. })) {
        return Some(PalwCellClosureRefusalV1::UnknownProfileKind);
    }
    gaps.iter()
        .find_map(|g| match g {
            PalwCellCoverageGapV1::Uncovered { kind, .. } => Some(PalwCellClosureRefusalV1::Uncovered(*kind)),
            PalwCellCoverageGapV1::UnknownProfileKind { .. } => None,
        })
        // An `Err` with no gap is not a shape the checker produces; fail closed rather than read it as covered.
        .or(Some(PalwCellClosureRefusalV1::UnknownProfileKind))
}

/// **The one predicate under RFC-0006's consumer v2** (`palw_memory_routing_court_v2`): [`palw_cell_closure_legacy_refusal_v1`]
/// over [`crate::palw_cell_coverage_v1::palw_legacy_cell_coverage_v2`] — the solo gdn v1/v2 recurrence, the composed hybrid maps and
/// (with a covering binding, or bindable at registration) the multi-router q36 tables are covered; every other gap — a Kimi router
/// included (G-16: Kimi is out of scope) — refuses as in round 4.
pub fn palw_cell_closure_refusal_v2(
    profile: &PalwShapeProfileV3,
    consumers: crate::palw_cell_coverage_v1::PalwCellConsumersV2<'_>,
) -> Option<PalwCellClosureRefusalV1> {
    let gaps = crate::palw_cell_coverage_v1::palw_legacy_cell_coverage_v2(profile, consumers).err()?;
    if gaps.iter().any(|g| matches!(g, PalwCellCoverageGapV1::UnknownProfileKind { .. })) {
        return Some(PalwCellClosureRefusalV1::UnknownProfileKind);
    }
    gaps.iter()
        .find_map(|g| match g {
            PalwCellCoverageGapV1::Uncovered { kind, .. } => Some(PalwCellClosureRefusalV1::Uncovered(*kind)),
            PalwCellCoverageGapV1::UnknownProfileKind { .. } => None,
        })
        .or(Some(PalwCellClosureRefusalV1::UnknownProfileKind))
}

/// **Which closure a structural step check runs under** — the caller's decision for the claim at hand. `Off` and `Legacy` are the
/// round-4 `bool` (`From<bool>` keeps every existing caller's meaning); `MemoryRoutingV2` is the closure with consumer v2 in force
/// for the claim, where a profile consumer v2 reaches is canonical and a solo / composed checkpoint's count is derivable.
///
/// **RFC06-4: one predicate on every blame path.** `MemoryRoutingV2` carries the claim's class router binding row as it stood at the
/// claim's acceptance (`None`: none declared by then), so the structural step check — and through it every offence-attribution
/// path (the Panel false-valid V1/V2 verifiers, the fault site, the panel contradiction, the fold) — judges a routed table exactly
/// as the claim gate (REQ-CELL-1b, [`palw_cell_closure_claim_refusal_v2`]) and the court (REQ-CELL-2,
/// [`palw_cell_closure_court_convicts_v2`]) do: covered only under a binding declared at or before the claim that names every routed
/// reader. Build it with [`Self::for_claim`] or [`Self::for_mirrors`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwCellClosureModeV1<'a> {
    Off,
    Legacy,
    MemoryRoutingV2 { router_binding: Option<&'a crate::palw_router_binding_v2::PalwRouterBindingRowV2> },
}

impl From<bool> for PalwCellClosureModeV1<'_> {
    fn from(closure: bool) -> Self {
        if closure { Self::Legacy } else { Self::Off }
    }
}

impl<'a> PalwCellClosureModeV1<'a> {
    pub fn in_force(&self) -> bool {
        !matches!(self, Self::Off)
    }

    /// Consumer v2 governs the claim.
    pub fn is_memory_routing_v2(&self) -> bool {
        matches!(self, Self::MemoryRoutingV2 { .. })
    }

    /// Is `profile` uncoverable under this closure (a shape fault past it)?
    pub fn refuses_profile(&self, profile: &PalwShapeProfileV3) -> bool {
        match self {
            Self::Off => false,
            Self::Legacy => palw_cell_closure_legacy_refusal_v1(profile).is_some(),
            Self::MemoryRoutingV2 { router_binding } => palw_cell_closure_refusal_v2(
                profile,
                crate::palw_cell_coverage_v1::PalwCellConsumersV2 { router_binding: *router_binding, registration: false },
            )
            .is_some(),
        }
    }

    /// The mode for a claim accepted at `accepted_daa` under the two mirrors, with the class's binding row (aux 139) if the chain
    /// holds one; a row declared after the claim is not the claim's.
    pub fn for_claim(
        params: &PalwStateParamsV2,
        accepted_daa: u64,
        router_binding: Option<&'a crate::palw_router_binding_v2::PalwRouterBindingRowV2>,
    ) -> Self {
        Self::for_mirrors(
            params.cell_coverage_closure_from_daa(),
            params.memory_routing_court_from_daa(),
            accepted_daa,
            router_binding,
        )
    }

    /// [`Self::for_claim`] from the two mirrored heights (`PalwStateParamsV2::cell_coverage_closure_from_daa` and
    /// `memory_routing_court_from_daa`) — for the verifiers that are handed heights rather than the params. `(None, _)`: `Off`;
    /// `(Some, None)`: the round-4 `Legacy` closure, byte for byte the old `bool`.
    pub fn for_mirrors(
        cell_closure_from_daa: Option<u64>,
        memory_routing_from_daa: Option<u64>,
        accepted_daa: u64,
        router_binding: Option<&'a crate::palw_router_binding_v2::PalwRouterBindingRowV2>,
    ) -> Self {
        if cell_closure_from_daa.is_none_or(|from| accepted_daa < from) {
            Self::Off
        } else if memory_routing_from_daa.is_some_and(|from| accepted_daa >= from) {
            Self::MemoryRoutingV2 { router_binding: router_binding.filter(|row| row.bound_from_daa <= accepted_daa) }
        } else {
            Self::Legacy
        }
    }

    /// **The mode one offence-attribution (blame) path runs a claim under** (RFC06-4). Past consumer v2 (both mirrors govern the
    /// claim) EVERY path gets the same [`Self::MemoryRoutingV2`] — the claim gate's and the court's predicate. Below it each path
    /// keeps exactly its historical answer: `round4` says whether the path applied round 4's closure before consumer v2 existed
    /// (the Panel false-valid V1/V2 verifiers, the fault site and the V1 panel contradiction did; `ExecutorRefuted` and the terminal
    /// carriage did not, and stay `Off`).
    pub fn for_blame_path(
        mirrors: PalwClosureMirrorsV1,
        accepted_daa: u64,
        router_binding: Option<&'a crate::palw_router_binding_v2::PalwRouterBindingRowV2>,
        round4: bool,
    ) -> Self {
        match Self::for_mirrors(mirrors.cell_closure_from_daa, mirrors.memory_routing_from_daa, accepted_daa, router_binding) {
            mode @ Self::MemoryRoutingV2 { .. } => mode,
            mode if round4 => mode,
            _ => Self::Off,
        }
    }

    /// The router a routed step's arithmetic reads under this mode: the bound router's global slot for the reader at `out_slot`
    /// under consumer v2 with a binding, `None` (the legacy unique-q36 rule) otherwise — the same row tag 244's court reads.
    pub fn bound_router_slot(&self, profile: &PalwShapeProfileV3, out_slot: u32) -> Option<u32> {
        match self {
            Self::MemoryRoutingV2 { router_binding: Some(row) } => {
                crate::palw_router_binding_v2::palw_bound_router_slot_v2(profile, row, out_slot)
            }
            _ => None,
        }
    }
}

/// **The two heights a blame path reads** — `PalwStateParamsV2::cell_coverage_closure_from_daa` and
/// `memory_routing_court_from_daa`, both `None` on every shipped network. `Default` is both absent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwClosureMirrorsV1 {
    pub cell_closure_from_daa: Option<u64>,
    pub memory_routing_from_daa: Option<u64>,
}

impl PalwClosureMirrorsV1 {
    pub fn of(params: &PalwStateParamsV2) -> Self {
        Self {
            cell_closure_from_daa: params.cell_coverage_closure_from_daa(),
            memory_routing_from_daa: params.memory_routing_court_from_daa(),
        }
    }

    /// Does consumer v2 govern something accepted at `accepted_daa` (the only case a blame path reads the binding row)?
    pub fn memory_routing_governs(&self, accepted_daa: u64) -> bool {
        self.cell_closure_from_daa.is_some_and(|from| accepted_daa >= from)
            && self.memory_routing_from_daa.is_some_and(|from| accepted_daa >= from)
    }
}

/// **The blame-path mode for the live claim `claim_id`** (`Off` for a target that is no longer a live claim — a liability or vesting
/// row — exactly as before). The binding row is read only when consumer v2 governs the claim, so below the fence nothing new is
/// read. Returns the row beside the mode's inputs; the caller builds the mode with [`PalwCellClosureModeV1::for_blame_path`] over
/// a borrow of it.
pub fn palw_blame_closure_inputs_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    claim_id: &crate::Hash64,
    mirrors: PalwClosureMirrorsV1,
) -> Option<(u64, Option<crate::palw_router_binding_v2::PalwRouterBindingRowV2>)> {
    let claim = state.claim(claim_id)?;
    let row = if mirrors.memory_routing_governs(claim.accepted_daa) {
        crate::palw_state_v2::palw_router_binding_row_of_v2(state, &claim.class_id)
    } else {
        None
    };
    Some((claim.accepted_daa, row))
}

/// **Is the closure in force for something dated `daa`** (a registration's block, a claim's acceptance)? `false` on every shipped
/// network (the mirror is `None`).
pub fn palw_cell_closure_governs_v1(params: &PalwStateParamsV2, daa: u64) -> bool {
    params.cell_coverage_closure_from_daa().is_some_and(|from| daa >= from)
}

/// **REQ-CELL-1: may a carried profile register at `daa_score`?** `None` below the fence, or for a covered profile.
pub fn palw_cell_closure_registration_refusal_v1(
    params: &PalwStateParamsV2,
    daa_score: u64,
    profile: &PalwShapeProfileV3,
) -> Option<PalwCellClosureRefusalV1> {
    if !palw_cell_closure_governs_v1(params, daa_score) {
        return None;
    }
    // RFC-0006 consumer v2: past its fence a registration is judged by what that consumer reaches (a routed table admitted when it
    // is bindable; its claims wait for the binding at the claim gate).
    if params.memory_routing_court_active_at(daa_score) {
        return palw_cell_closure_refusal_v2(
            profile,
            crate::palw_cell_coverage_v1::PalwCellConsumersV2 { router_binding: None, registration: true },
        );
    }
    palw_cell_closure_legacy_refusal_v1(profile)
}

/// **REQ-CELL-1b: the claim gate's question** for a claim accepted at `accepted_daa` of a class whose graph the fold holds
/// (`held_profile`, the fold's `fp_work_profile_of`). `None` below the fence, for a covered class, and — where the fold does not
/// hold the graph — `None` here: that claim is the court's (REQ-CELL-2), which convicts rather than acquits.
pub fn palw_cell_closure_claim_refusal_v1(
    params: &PalwStateParamsV2,
    accepted_daa: u64,
    held_profile: Option<&PalwShapeProfileV3>,
) -> Option<PalwCellClosureRefusalV1> {
    if !palw_cell_closure_governs_v1(params, accepted_daa) {
        return None;
    }
    palw_cell_closure_legacy_refusal_v1(held_profile?)
}

/// **REQ-CELL-1b under consumer v2**: [`palw_cell_closure_claim_refusal_v1`] with the class's router binding row in hand (aux 139).
/// Past `palw_memory_routing_court_v2` a routed table needs a binding declared AT OR BEFORE the claim's acceptance that covers every
/// routed reader; the recurrence maps consumer v2 reaches are covered. Below that fence: byte for byte the v1 answer.
pub fn palw_cell_closure_claim_refusal_v2(
    params: &PalwStateParamsV2,
    accepted_daa: u64,
    held_profile: Option<&PalwShapeProfileV3>,
    router_binding: Option<&crate::palw_router_binding_v2::PalwRouterBindingRowV2>,
) -> Option<PalwCellClosureRefusalV1> {
    if !palw_cell_closure_governs_v1(params, accepted_daa) {
        return None;
    }
    let profile = held_profile?;
    if !params.memory_routing_court_active_at(accepted_daa) {
        return palw_cell_closure_legacy_refusal_v1(profile);
    }
    let binding = router_binding.filter(|row| row.bound_from_daa <= accepted_daa);
    palw_cell_closure_refusal_v2(
        profile,
        crate::palw_cell_coverage_v1::PalwCellConsumersV2 { router_binding: binding, registration: false },
    )
}

/// **REQ-CELL-2: does the court convict on the closure alone?** For a claim accepted at `accepted_daa` under a closure in force from
/// `cell_closure_from_daa`, whose binding's (registered) profile is `profile`: `true` exactly when the profile is uncoverable. The
/// caller has pinned the binding to the claim's execution root and to the registered class first.
pub fn palw_cell_closure_court_convicts_v1(
    cell_closure_from_daa: Option<u64>,
    accepted_daa: u64,
    profile: &PalwShapeProfileV3,
) -> bool {
    cell_closure_from_daa.is_some_and(|from| accepted_daa >= from) && palw_cell_closure_legacy_refusal_v1(profile).is_some()
}

/// **REQ-CELL-2 under consumer v2**: [`palw_cell_closure_court_convicts_v1`] where `memory_routing_from_daa` (the consumer's mirror)
/// governs the claim too — the class's router binding row (declared at or before the claim's acceptance) in hand. `None` for the
/// mirror: byte for byte the v1 answer.
pub fn palw_cell_closure_court_convicts_v2(
    cell_closure_from_daa: Option<u64>,
    memory_routing_from_daa: Option<u64>,
    accepted_daa: u64,
    profile: &PalwShapeProfileV3,
    router_binding: Option<&crate::palw_router_binding_v2::PalwRouterBindingRowV2>,
) -> bool {
    if memory_routing_from_daa.is_none_or(|from| accepted_daa < from) {
        return palw_cell_closure_court_convicts_v1(cell_closure_from_daa, accepted_daa, profile);
    }
    let binding = router_binding.filter(|row| row.bound_from_daa <= accepted_daa);
    cell_closure_from_daa.is_some_and(|from| accepted_daa >= from)
        && palw_cell_closure_refusal_v2(
            profile,
            crate::palw_cell_coverage_v1::PalwCellConsumersV2 { router_binding: binding, registration: false },
        )
        .is_some()
}

impl Params {
    /// `palw_cell_coverage_closure_v1`'s height, `Some` only for a real one (`never()` is `None`).
    pub fn palw_cell_coverage_closure_v1_from_daa(&self) -> Option<u64> {
        self.palw_cell_coverage_closure_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
    }

    /// **The closure's mirror** on the V2 bundle's state params (`cell_coverage_closure_from_daa`), which the fold and the court
    /// read. Written here and nothing else; `None` where the fence is not armed (or is `never()`).
    pub fn sync_palw_cell_coverage_closure_v1(&mut self) {
        let from = self.palw_cell_coverage_closure_v1_from_daa();
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_cell_coverage_closure_from_daa(from);
        }
    }

    /// **Refused when armed at a real height until an arm-readiness record is pinned** (lane rfcgap-a, 2026-10-22: the rule is built
    /// — REQ-CELL-1 at registration, REQ-CELL-1b at the claim gate, REQ-CELL-2 in `adjudicate_court_close_v4` and the step-leg
    /// closure form — so the refusal is by record, never by name); `None` and `Some(never())` pass. The mirror must agree with the
    /// fence first. The binary's list ([`PALW_CELL_COVERAGE_CLOSURE_ARMING_RECORDS_V1`]) is EMPTY, so every real network is refused.
    pub fn validate_palw_cell_coverage_closure_v1(&self) -> Result<(), PalwModeV2Error> {
        self.validate_palw_cell_coverage_closure_in_v1(PALW_CELL_COVERAGE_CLOSURE_ARMING_RECORDS_V1)
    }

    /// [`Self::validate_palw_cell_coverage_closure_v1`] against `records` in place of the binary's pinned list (tests and arm-readiness
    /// tooling; the node validates with the pinned list). An isolated G14 test genesis ([`Params::has_isolated_g14_genesis_v1`]) skips
    /// the record pin, as the G14 gates do; no shipped preset carries one.
    pub fn validate_palw_cell_coverage_closure_in_v1(&self, records: &[PalwCellClosureArmingRecordV1]) -> Result<(), PalwModeV2Error> {
        if let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
            && bundle.state.cell_coverage_closure_from_daa() != self.palw_cell_coverage_closure_v1_from_daa()
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_cell_coverage_closure_v1 disagrees with the V2 bundle's mirror: Params::sync_palw_cell_coverage_closure_v1",
            ));
        }
        let Some(at) = self.palw_cell_coverage_closure_v1_from_daa() else { return Ok(()) };
        if self.has_isolated_g14_genesis_v1() {
            return Ok(());
        }
        match palw_cell_closure_arming_record_admits_v1(self.genesis.hash, at, records) {
            Ok(()) => Ok(()),
            Err(crate::palw_g14_gates_v1::PalwG14ArmingRefusalV1::NoRecord) => Err(PalwModeV2Error::Invalid(
                "palw_cell_coverage_closure_v1 cannot be armed: the closure rule (RFC-0006 R6-CELL round 4) is built, but no arm-readiness \
                 record (REQ-CELL fold suite, node E2E, the blame-path review, fence-crossing drill) is pinned for this network and height \
                 (PALW_CELL_COVERAGE_CLOSURE_ARMING_RECORDS_V1 is empty; the arming itself is a user decision)",
            )),
            Err(other) => Err(PalwModeV2Error::Invalid(other.reason())),
        }
    }
}

// =================================================================================================================================
// The arm-readiness record (lane rfcgap-a, 2026-10-22; survey row U11)
// =================================================================================================================================

/// **The closure's arm-readiness requirements**, one evidence digest each, in this order (RFC-0006 R6-CELL "Remaining"). The closure
/// only REFUSES (registration, claim gate) or convicts from the binding alone; it pays and weights nothing, so no economic value is
/// measured. The arming decision itself stays the user's: a record is pinned only by a reviewed change.
pub const PALW_CELL_COVERAGE_CLOSURE_ARMING_REQUIREMENTS_V1: &[&str] = &[
    "V1 build, regressions and boundary decode: the REQ-CELL suite (palw_cell_coverage_closure_v1 tests), palw_cell_coverage_v1 \
     round4_every_uncovered_kind_is_enumerated_with_its_disposition, palw_step_leg req_cell_2_an_unknown_map_is_never_no_fault_under_the_closure \
     and the adjudicate_court_close_v4 closure fixture (palw_court_v2)",
    "node E2E: past the fence an uncoverable legacy class is refused at registration (REQ-CELL-1) and at the claim gate \
     (REQ-CELL-1b, CELL_UNCOVERABLE), and a court close of a claim accepted past it over a pre-fence uncoverable class convicts \
     ExecutorGuilty from the binding alone (REQ-CELL-2); the block stands",
    "review: the Panel false-valid structural routes' closure mode (palw_offence_v1 / palw_offence_attribution_v1, \
     PalwCellClosureModeV1::for_blame_path) — the open R6-CELL question, decided and recorded",
    "drill: the shipped binary crosses the fence; restart / IBD / reorg replay identical across it; claims accepted before it keep \
     the old rules (REQ-CELL-3)",
];

/// The record format's version.
pub const PALW_CELL_COVERAGE_CLOSURE_ARMING_RECORD_VERSION_V1: u16 = 1;

/// **An arm-readiness record** for `palw_cell_coverage_closure_v1` — the same fields and form checks as
/// [`crate::palw_g14_gates_v1::PalwG14ArmingRecordV1`] (a separate type because the closure is not a G14 conviction fence: adding it to
/// `PalwG14GatedFenceV1::ALL` would put it under the G14-from-genesis set and the court-policy completeness gate, which do not apply).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCellClosureArmingRecordV1 {
    pub version: u16,
    pub genesis: crate::Hash64,
    pub activation_daa: u64,
    pub source_commit: [u8; 20],
    pub evidence: &'static [[u8; 32]],
    pub review: [u8; 32],
}

/// **The pinned records — EMPTY.** The closure armed at a real height on a real network is refused.
pub const PALW_CELL_COVERAGE_CLOSURE_ARMING_RECORDS_V1: &[PalwCellClosureArmingRecordV1] = &[];

/// A record's own form (not whether it is pinned): version, a non-zero source commit, one non-zero distinct evidence digest per
/// requirement (none equal to the review), a non-zero review.
pub fn palw_cell_closure_arming_record_form_v1(
    record: &PalwCellClosureArmingRecordV1,
) -> Result<(), crate::palw_g14_gates_v1::PalwG14ArmingRefusalV1> {
    use crate::palw_g14_gates_v1::PalwG14ArmingRefusalV1 as R;
    if record.version != PALW_CELL_COVERAGE_CLOSURE_ARMING_RECORD_VERSION_V1 {
        return Err(R::Version);
    }
    if record.source_commit == [0; 20] {
        return Err(R::SourceCommit);
    }
    let expected = PALW_CELL_COVERAGE_CLOSURE_ARMING_REQUIREMENTS_V1.len();
    if record.evidence.len() != expected {
        return Err(R::EvidenceCount { expected, got: record.evidence.len() });
    }
    if record.review == [0; 32] {
        return Err(R::Review);
    }
    for (i, digest) in record.evidence.iter().enumerate() {
        if *digest == [0; 32] || *digest == record.review || record.evidence[..i].contains(digest) {
            return Err(R::Evidence(i));
        }
    }
    Ok(())
}

/// **Does `records` authorize arming the closure at `activation_daa` on the network whose genesis is `genesis`?** The first record
/// naming exactly this (network, height) decides; none is `NoRecord`.
pub fn palw_cell_closure_arming_record_admits_v1(
    genesis: crate::Hash64,
    activation_daa: u64,
    records: &[PalwCellClosureArmingRecordV1],
) -> Result<(), crate::palw_g14_gates_v1::PalwG14ArmingRefusalV1> {
    let Some(first) = records.iter().find(|r| r.genesis == genesis && r.activation_daa == activation_daa) else {
        return Err(crate::palw_g14_gates_v1::PalwG14ArmingRefusalV1::NoRecord);
    };
    palw_cell_closure_arming_record_form_v1(first)
}

#[cfg(test)]
mod tests;
