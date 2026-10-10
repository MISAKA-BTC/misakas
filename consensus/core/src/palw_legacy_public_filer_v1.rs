//! **Lane LG14-A (RFC-0014 §6–§8 on the legacy V2 Panel route): one fraud filer for every role, a non-seat reservation that holds a
//! claim's ends to the claim's own hard deadline, and direct proofs no open session pre-empts** — dormant behind
//! `Params::palw_legacy_public_filer_v1`, which no network can arm. Design: `docs/design/palw/legacy-route-g14-filer.md`.
//!
//! ```text
//! 154 DisputeReservedV1  (any Active bond at or above the floor, not the claim's executor)
//!                        a deposit (DA-6's exposure at the claim's stage, INTF's share) on the reserver's free half; one per bond per
//!                        claim over the claim's life. While ANY reservation on the claim is live the claim owes NO deadline: no Final,
//!                        no bind / receipt / redraw / unavailable / replay timeout, no retirement of a Final claim.
//!                        The claim's hard deadline is `trace_retention_daa − W_disclose` — a pure function of the CLAIM, so nobody's
//!                        reservation moves it. Past it every live reservation lapses.
//! 155 DisputeReleasedV1  the reserver ends its own reservation (its deposit is held until the claim resolves).
//!     DA demands         a demand by a bond whose reservation on the claim is live is a RESERVED session: admitted on the
//!                        reservation's own budget (34), never on DA-8's shared non-seat budget, and seat-like for DA-5.
//!     outcomes           conviction / DA default: every deposit refunded, the held ones too. A neutral void: live deposits refunded.
//!                        Release, lapse: the deposit is held (`dismissed_held`), refunded by a later conviction, burned at retirement.
//! ```
//!
//! Direct proofs (kinds 3, 4, 7): past the fence the adjudicators no longer refuse a proof because another bond's court is open
//! (`crate::palw_offence_attribution_v1::PalwSessionRuleV1`); the conviction's void closes every session in the same transition.
//!
//! This module holds the TYPES, constants, the message, the fence's `Params` half and the engine's pure half (the planner every
//! role's filer runs). The fold's arms live in `palw_legacy_public_filer_fold_v1`, a child module of `palw_state_v2`.

use std::collections::{BTreeMap, BTreeSet};

use borsh::{BorshDeserialize, BorshSerialize};

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::{PalwBondKeyV2, PalwClaimPhaseV2, PalwClaimStateV2, PalwConsensusObjectV2};

// ---- allocations (the Lead's registry: tags 154–156, deltas 200–204, tail 0xE2, RPC 204–206) -----------------------------------

/// Tag 154: a reservation ([`PalwConsensusObjectV2::DisputeReservedV1`]).
pub const PALW_DISPUTE_RESERVED_TAG_V1: u8 = 154;
/// Tag 155: a reserver's release ([`PalwConsensusObjectV2::DisputeReleasedV1`]).
pub const PALW_DISPUTE_RELEASED_TAG_V1: u8 = 155;
/// Tag 156: allocated to this lane and declared by no variant (reserved).
pub const PALW_DISPUTE_SPARE_TAG_V1: u8 = 156;
/// Delta 200: the one journaled writer of `legacy_disputes` (`PalwDeltaEntryV2::LegacyDispute`). 201–204 are reserved.
pub const PALW_DELTA_LEGACY_DISPUTE_V1: u8 = 200;
/// The carriage tail of `legacy_disputes`, written only when the table is non-empty.
pub const PALW_CARRIAGE_LEGACY_DISPUTES_TAIL_V1: u8 = 0xE2;

/// The reservation's wire version.
pub const PALW_DISPUTE_RESERVATION_VERSION_V1: u16 = 1;

/// The ML-DSA-87 context of every legacy-dispute object's signature (not in a live network's committed set: the Some-only fence covers
/// it, as DA16's own context is covered by its fence).
pub const PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/legacy-dispute/object/v1";
const PALW_LEGACY_DISPUTE_MESSAGE_DOMAIN_V1: &[u8] = b"misaka-palw/legacy-dispute/object-message/v1";
/// Every keyed domain and context this family uses, for the cross-family uniqueness sweep.
pub const PALW_LEGACY_DISPUTE_ALL_DOMAINS: &[&[u8]] = &[PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1, PALW_LEGACY_DISPUTE_MESSAGE_DOMAIN_V1];

// ---- the terms (consensus constants of the never-armed fence; POLICY values, design §5) --------------------------------------------

/// Live reservations one claim holds at once.
pub const PALW_DISPUTE_LIVE_RESERVATIONS_PER_CLAIM_V1: usize = 64;
/// Distinct reservers one claim admits over its life (live and closed).
pub const PALW_DISPUTE_RESERVERS_PER_CLAIM_TOTAL_V1: usize = 256;
/// Live reservations one bond holds at once, across claims.
pub const PALW_DISPUTE_LIVE_RESERVATIONS_PER_BOND_V1: usize = 64;
/// Reserved DA sessions one reservation may open on its claim (one open at a time: DA-1): the legacy localizer's worst case over the
/// widest step ladder (2^40 leaves, [`PalwLegacyBisectV1::max_demands`]) — the binding read, the halving to one range, the range, the
/// terminal. LG14-B's frontier descent needs `⌈40 / 9⌉ + 2`.
pub const PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1: u8 = 34;

// ---- the wire -------------------------------------------------------------------------------------------------------------------

/// **The reservation** (tag 154's payload): the claim, its committed roots as the reserver read them, and the reserver. The roots are
/// compared with the claim's record; the claim id alone already binds them, they are named so the object says what it disputes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwDisputeReservationV1 {
    pub version: u16,
    pub claim: Hash64,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    pub reserver: PalwBondKeyV2,
}

/// **The message a legacy-dispute object's signer signs**: `H(domain; network ‖ kind ‖ signer ‖ len ‖ payload)`, `payload` the
/// object's Borsh without its signature ([`palw_dispute_reserved_payload_v1`], [`palw_dispute_released_payload_v1`]).
pub fn palw_legacy_dispute_message_v1(network_domain: Hash64, kind: u8, signer: &PalwBondKeyV2, payload: &[u8]) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_LEGACY_DISPUTE_MESSAGE_DOMAIN_V1).to_state();
    s.update(network_domain.as_byte_slice());
    s.update(&[kind]);
    s.update(signer.0.transaction_id.as_byte_slice());
    s.update(&signer.0.index.to_le_bytes());
    s.update(&(payload.len() as u64).to_le_bytes());
    s.update(payload);
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// Tag 154's signed payload.
pub fn palw_dispute_reserved_payload_v1(reservation: &PalwDisputeReservationV1) -> Vec<u8> {
    borsh::to_vec(reservation).expect("a reservation serializes")
}

/// Tag 155's signed payload.
pub fn palw_dispute_released_payload_v1(claim: &Hash64) -> Vec<u8> {
    borsh::to_vec(claim).expect("a claim id serializes")
}

/// **Tag 154, built and signed** — what every role's filer submits. `sign` receives the message's bytes and returns the reserver's
/// ML-DSA-87 signature under [`PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1`].
pub fn palw_dispute_reserved_object_v1(
    network_domain: Hash64,
    reservation: PalwDisputeReservationV1,
    sign: impl FnOnce(&[u8]) -> Vec<u8>,
) -> PalwConsensusObjectV2 {
    let message = palw_legacy_dispute_message_v1(
        network_domain,
        PALW_DISPUTE_RESERVED_TAG_V1,
        &reservation.reserver,
        &palw_dispute_reserved_payload_v1(&reservation),
    );
    let signature = sign(message.as_byte_slice());
    PalwConsensusObjectV2::DisputeReservedV1 { reservation: Box::new(reservation), signature }
}

/// **Tag 155, built and signed.**
pub fn palw_dispute_released_object_v1(
    network_domain: Hash64,
    claim: Hash64,
    reserver: PalwBondKeyV2,
    sign: impl FnOnce(&[u8]) -> Vec<u8>,
) -> PalwConsensusObjectV2 {
    let message = palw_legacy_dispute_message_v1(
        network_domain,
        PALW_DISPUTE_RELEASED_TAG_V1,
        &reserver,
        &palw_dispute_released_payload_v1(&claim),
    );
    let signature = sign(message.as_byte_slice());
    PalwConsensusObjectV2::DisputeReleasedV1 { claim, reserver, signature }
}

/// **Is this object a legacy-dispute object (tags 154–155)** — a variant the live int-12 build cannot decode and skips (A-2)? Below
/// `Params::palw_legacy_public_filer_v1` the acceptance walk drops it by name before any slot, rent or budget is charged for it; the
/// fold refuses it as the second lock.
pub fn palw_object_is_legacy_dispute_v1(object: &PalwConsensusObjectV2) -> bool {
    matches!(object, PalwConsensusObjectV2::DisputeReservedV1 { .. } | PalwConsensusObjectV2::DisputeReleasedV1 { .. })
}

// ---- the state ------------------------------------------------------------------------------------------------------------------

/// **One live reservation**, keyed by its reserver inside its claim's record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwDisputeReservationRowV1 {
    /// The DAA the reservation was accepted at.
    pub reserved_daa: u64,
    /// What it holds on the reserver's free half (A-6), fixed at acceptance.
    pub deposit: u128,
    /// Reserved DA sessions it has opened on the claim, at most [`PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1`].
    pub sessions_opened: u8,
}

/// **One claim's dispute record** (`legacy_disputes`, keyed by claim): written with the claim's first reservation, deleted when nothing
/// is live and nothing is held — or with the claim at its retirement, burning what is held.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwDisputeClaimV1 {
    /// The DAA of the claim's first reservation.
    pub opened_daa: u64,
    /// `trace_retention_daa − W_disclose`, fixed at the first reservation ([`palw_dispute_hard_deadline_v1`]); never moves.
    pub hard_deadline_daa: u64,
    /// The live reservations, by reserver.
    pub live: BTreeMap<PalwBondKeyV2, PalwDisputeReservationRowV1>,
    /// Every reserver whose reservation on this claim ended (released, lapsed or settled) — the once-per-life rule, which is also the
    /// replay rule.
    pub closed: BTreeSet<PalwBondKeyV2>,
    /// Deposits of reservations that ended without an objective outcome, `(reserver, amount)` in closing order: refunded at a
    /// conviction, burned when the claim retires (DA-6's `refuted_held`, for the reservation).
    pub dismissed_held: Vec<(PalwBondKeyV2, u128)>,
}

impl PalwDisputeClaimV1 {
    /// Does this record hold its claim's ends?
    pub fn holds(&self) -> bool {
        !self.live.is_empty()
    }

    /// Reservers the claim has admitted over its life.
    pub fn reservers_total(&self) -> usize {
        self.live.len() + self.closed.len()
    }

    /// Has `bond` reserved this claim before (live or closed)?
    pub fn knows(&self, bond: &PalwBondKeyV2) -> bool {
        self.live.contains_key(bond) || self.closed.contains(bond)
    }

    /// What `bond` holds through this record: its live deposit and its dismissed ones.
    pub fn exposure_of(&self, bond: &PalwBondKeyV2) -> u128 {
        let live = self.live.get(bond).map(|row| row.deposit).unwrap_or(0);
        self.dismissed_held.iter().filter(|(held, _)| held == bond).fold(live, |sum, (_, amount)| sum.saturating_add(*amount))
    }
}

/// **The claim's hard deadline**: the last DAA at which its retention obligation still admits a DA session (`da_admission_v1` refuses
/// one whose deadline passes `trace_retention_daa`). A pure function of the claim — fixed by its acceptance, never by who reserves first.
pub fn palw_dispute_hard_deadline_v1(claim: &PalwClaimStateV2, w_disclose: u64) -> u64 {
    claim.trace_retention_daa.saturating_sub(w_disclose)
}

/// **Does `claim` have a pursuit a reservation may hold** — `Provisional`, `PanelBound`, `ReceiptLicensed`, or `Final` with its
/// vesting row unmatured (`final_row_open`, the post-`Final` DA stage)? `Voided` and `DefaultDisputed` (which no R-core+ chain holds)
/// never.
pub fn palw_dispute_phase_reservable_v1(phase: &PalwClaimPhaseV2, final_row_open: bool) -> bool {
    match phase {
        PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. } | PalwClaimPhaseV2::ReceiptLicensed { .. } => true,
        PalwClaimPhaseV2::Final { .. } => final_row_open,
        PalwClaimPhaseV2::Voided { .. } | PalwClaimPhaseV2::DefaultDisputed { .. } => false,
    }
}

// ---- the fence ------------------------------------------------------------------------------------------------------------------

impl Params {
    /// `palw_legacy_public_filer_v1`, resolved: `Some` only on a `ConsensusV2` network with a real height (`never()` is dormant).
    pub fn palw_legacy_public_filer_v1_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_legacy_public_filer_v1.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// **Is the legacy route's public filer in force at `daa_score`?** `false` on every preset.
    pub fn palw_legacy_public_filer_active_at(&self, daa_score: u64) -> bool {
        self.palw_legacy_public_filer_v1_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **The fence's mirror** on the V2 bundle's state params (`legacy_public_filer_from_daa`), which the fold reads. Written here
    /// and nothing else; `None` where the fence is not armed (or is `never()`).
    pub fn sync_palw_legacy_public_filer_v1(&mut self) {
        let from_daa = self.palw_legacy_public_filer_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_legacy_public_filer_from_daa(from_daa);
        }
    }

    /// **The fence's refusals**, asked by [`Params::validate_palw_v2`]: a V2 bundle whose mirror is not the fence's; arming on a
    /// ruleset that is not `ConsensusV2`; arming without `palw_rcore_plus` and `palw_offence_attribution` in force at or below it (the
    /// reservation rides R-core+'s DA court, the direct proofs are F2's adjudicators); and — until the full-activation release names
    /// its height — any arming at all. A `Some(never())` value is dormant and passes.
    pub fn validate_palw_legacy_public_filer_v1(&self) -> Result<(), PalwModeV2Error> {
        let armed = self.palw_legacy_public_filer_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.legacy_public_filer_from_daa(),
            _ => None,
        };
        if mirror != armed && matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid(
                "palw_legacy_public_filer_v1 disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_legacy_public_filer_v1 after the bundle is assembled",
            ));
        }
        let Some(at) = armed else {
            return Ok(());
        };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_legacy_public_filer_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        let below = |fence: Option<ForkActivation>| fence.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at);
        if !below(self.palw_rcore_plus) || !below(self.palw_offence_attribution) {
            return Err(PalwModeV2Error::Invalid(
                "palw_legacy_public_filer_v1 needs palw_rcore_plus and palw_offence_attribution in force at or below it: the reservation \
                 rides R-core+'s data-availability court and the direct proofs are F2's adjudicators",
            ));
        }
        Err(PalwModeV2Error::Invalid(
            "palw_legacy_public_filer_v1 cannot be armed yet: the legacy route's public filer changes when a V2 claim may end (RFC-0014 \
             §7) and its height is the full-activation release's to name (lane LG14-A)",
        ))
    }
}

// ---- the engine's pure half (RFC-0014 §6.1) -------------------------------------------------------------------------------------

/// **Who runs a filer.** The role changes discovery, priority and the local budget — never what counts as evidence, which terminal a
/// case may reach, or what the public reads return.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PalwFilerRoleV1 {
    /// A seat of the claim's panel.
    Seat,
    /// A genesis operator's bond.
    Operator,
    /// Any other bond holder.
    PublicBond,
    /// A watcher that holds no bond and files only fee-paid direct proofs.
    Watchdog,
}

/// **Where one case stands** (RFC-0014 §6.1's chain): `Mismatch` is local; from `Reserved` on, the phase is chain state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwFilerPhaseV1 {
    /// Seen, not yet checked.
    Discovered,
    /// The local check reproduced the claim's committed roots: nothing to file.
    Honest,
    /// The local check found a mismatch; nothing is on chain yet.
    Mismatch,
    /// The bond's reservation is live on chain.
    Reserved,
    /// A localizing demand is open (the unit named).
    Localizing,
    /// The first divergent leaf is located; its terminal is being filed or adjudicated.
    Judging,
    /// The claim was convicted (any route).
    Convicted,
    /// The claim was defaulted for withholding.
    DaDefault,
    /// The pursuit ended without an objective outcome (released, lapsed) — the claim stands.
    Dismissed,
    /// The claim ended before the pursuit could (a neutral void, retirement) or the pursuit ran past its deadline.
    Expired,
}

/// The widest `StepRange` a held demand may name (`crate::palw_held_da_v1::PALW_HELD_DA_MAX_RANGE_LEAVES_V1`): the bisection's last
/// probe discloses the whole remaining interval's leaf hashes at once.
pub const PALW_LEGACY_BISECT_FINAL_RANGE_V1: u64 = crate::palw_held_da_v1::PALW_HELD_DA_MAX_RANGE_LEAVES_V1 as u64;

/// **One probe of the legacy localizer**: a unit the engine demands through a reserved DA session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwLegacyProbeV1 {
    /// `DefaultAccused` of event `(row, tile)`: its answer carries the claim's binding (authenticated by the fold against the
    /// claim's `execution_root`), which every held demand must name.
    Binding { row: u32, tile: u8 },
    /// `DefaultAccusedHeld { StepRange { first, count } }`: the committed leaf hashes of `[first, first + count)`.
    Range { first: u64, count: u32 },
    /// `DefaultAccusedHeld { StepLeaf { leaf } }` at the located first divergent leaf: the producer's answer convicts itself (the
    /// one-move verdict reads the committed inputs), its silence defaults (DA-7).
    Terminal { leaf: u64 },
}

/// **The legacy bisection** (the localizer for non-fused leaves, RFC-0014 §4.2 over the existing held units): the first committed
/// step leaf whose hash differs from the honest run's. Invariant: every leaf in `[0, lo)` matched the honest run, and some leaf in
/// `[lo, hi)` does not (`hi = n` at the start: the committed root differs, so some leaf does). The step order is topological for the
/// forward pass, so the first divergent leaf is a step whose inputs all agree with the honest run — the one the one-move verdict
/// convicts when it is disclosed. Width-1 ranges halve the interval until it fits one range of
/// [`PALW_LEGACY_BISECT_FINAL_RANGE_V1`] leaves, which locates the leaf. LG14-B's frontier descent replaces the halving where it lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwLegacyBisectV1 {
    pub lo: u64,
    pub hi: u64,
}

impl PalwLegacyBisectV1 {
    /// Over a claim of `leaves` committed step leaves.
    pub fn new(leaves: u64) -> Self {
        Self { lo: 0, hi: leaves }
    }

    /// The located leaf, once the interval is one leaf wide.
    pub fn located(&self) -> Option<u64> {
        (self.hi > self.lo && self.hi - self.lo == 1).then_some(self.lo)
    }

    /// The next range to demand: the whole interval once it fits one range, else the middle leaf. `None` once located (or empty).
    pub fn next_range(&self) -> Option<(u64, u32)> {
        if self.hi <= self.lo || self.located().is_some() {
            return None;
        }
        let width = self.hi - self.lo;
        if width <= PALW_LEGACY_BISECT_FINAL_RANGE_V1 {
            return Some((self.lo, width as u32));
        }
        // The middle at `(width − 1) / 2`: a mismatch leaves `⌈width / 2⌉`, a match `⌊width / 2⌋`, so `max_demands` is exact.
        Some((self.lo + (width - 1) / 2, 1))
    }

    /// Record what the chain disclosed for `[first, first + matches.len())`: whether each committed leaf hash is the honest run's.
    /// Read in leaf order: a match moves `lo` past it, the first mismatch closes `hi` on it.
    pub fn record_range(&mut self, first: u64, matches: &[bool]) {
        for (offset, matches_honest) in matches.iter().enumerate() {
            let leaf = first + offset as u64;
            if leaf < self.lo || leaf >= self.hi {
                continue;
            }
            if *matches_honest {
                self.lo = leaf + 1;
            } else {
                self.hi = leaf + 1;
                break;
            }
        }
    }

    /// Demands the localizer spends at most over `leaves`: the binding read, the halving, the final range, the terminal.
    pub fn max_demands(leaves: u64) -> u32 {
        let wide = leaves.div_ceil(PALW_LEGACY_BISECT_FINAL_RANGE_V1).max(1);
        let halvings = u64::BITS - wide.saturating_sub(1).leading_zeros();
        halvings + 3
    }
}

/// **What the chain says about one claim, from the public reads** (the tip state and the accepted blocks), for the engine's decision.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwFilerClaimFactsV1 {
    /// The claim is gone, or terminal with nothing left to pursue.
    pub ended: bool,
    /// The claim's terminal outcome, when it has one.
    pub outcome: Option<PalwFilerPhaseV1>,
    /// May a reservation be filed now (fence, phase, hard deadline, caps — the fold's own check, asked at the tip)?
    pub reservable: bool,
    /// This bond's reservation is live.
    pub reserved: bool,
    /// This bond has a DA session open on the claim (one at a time, DA-1).
    pub session_open: bool,
    /// May a DA session be opened on the claim now (a panel is bound, inside retention)?
    pub accusable: bool,
    /// The claim's binding was read off an authenticated answer on chain.
    pub binding_known: bool,
}

/// **The engine's next move for one case.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwFilerActionV1 {
    /// Nothing to do now (waiting for an answer, a panel, the tip).
    Wait,
    /// File tag 154.
    Reserve,
    /// Demand this unit (a reserved session when the reservation is live).
    Demand(PalwLegacyProbeV1),
    /// The located leaf is a fused site: its terminal is the held route's (LG14-B's committed kernel witness and dissection).
    HeldRoute { leaf: u64 },
    /// The case is over.
    Done(PalwFilerPhaseV1),
}

/// **The common filer's step** — the same for a seat, an operator, a public bond and a watchdog (a watchdog never reserves: it holds
/// no bond; a seat's sessions already pause the claim). `bisect` is the case's localizer state, `demandable` the class's `StepLeaf`
/// rule (a fused leaf is LG14-B's).
pub fn palw_fraud_filer_next_v1(
    role: PalwFilerRoleV1,
    facts: &PalwFilerClaimFactsV1,
    mismatch: bool,
    bisect: &PalwLegacyBisectV1,
    demandable: impl Fn(u64) -> bool,
) -> PalwFilerActionV1 {
    if let Some(outcome) = facts.outcome {
        return PalwFilerActionV1::Done(outcome);
    }
    if facts.ended {
        return PalwFilerActionV1::Done(PalwFilerPhaseV1::Expired);
    }
    if !mismatch {
        return PalwFilerActionV1::Done(PalwFilerPhaseV1::Honest);
    }
    if !facts.reserved && role != PalwFilerRoleV1::Watchdog && role != PalwFilerRoleV1::Seat {
        return if facts.reservable { PalwFilerActionV1::Reserve } else { PalwFilerActionV1::Wait };
    }
    if facts.session_open || !facts.accusable {
        return PalwFilerActionV1::Wait;
    }
    if !facts.binding_known {
        return PalwFilerActionV1::Demand(PalwLegacyProbeV1::Binding { row: 0, tile: 0 });
    }
    match bisect.located() {
        Some(leaf) if demandable(leaf) => PalwFilerActionV1::Demand(PalwLegacyProbeV1::Terminal { leaf }),
        Some(leaf) => PalwFilerActionV1::HeldRoute { leaf },
        None => match bisect.next_range() {
            Some((first, count)) => PalwFilerActionV1::Demand(PalwLegacyProbeV1::Range { first, count }),
            None => PalwFilerActionV1::Wait,
        },
    }
}

// ---- the engine's shared reads and builders (RFC-0014 §6.1–§6.3): the node's filer and the real-node suite run THESE ------------

impl PalwLegacyProbeV1 {
    /// The DA unit this probe demands — the unit the fold records as answered once the producer discloses it.
    pub fn unit(&self) -> crate::palw_da_rcore_v1::PalwDaUnitV1 {
        use crate::palw_da_rcore_v1::PalwDaUnitV1;
        use crate::palw_held_da_v1::PalwHeldMissingV1;
        match *self {
            Self::Binding { row, tile } => PalwDaUnitV1::Event { row, tile },
            Self::Range { first, count } => PalwDaUnitV1::Held(PalwHeldMissingV1::StepRange { first, count }),
            Self::Terminal { leaf } => PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { leaf }),
        }
    }
}

/// **The engine's facts for `me` off one claim's public view** (RPC 204's [`crate::palw_state_v2::PalwLegacyDisputeViewV1`]; `None`:
/// the state no longer holds the claim) and the fold's own admission check of `me`'s reservation (`reservable`). A recorded
/// conviction or default decides first, then the claim's phase; a reservation of `me` that ended without one (released, lapsed) is
/// `Dismissed` — `me`'s pursuit is over, whoever else's is not.
pub fn palw_fraud_filer_facts_v1(
    view: Option<&crate::palw_state_v2::PalwLegacyDisputeViewV1>,
    me: &PalwBondKeyV2,
    reservable: bool,
    binding_known: bool,
) -> PalwFilerClaimFactsV1 {
    use crate::palw_state_v2::PalwVoidReasonV2;
    let Some(view) = view else { return PalwFilerClaimFactsV1 { ended: true, ..Default::default() } };
    let outcome = if view.executor_refuted || view.court_convicted {
        Some(PalwFilerPhaseV1::Convicted)
    } else if view.da_defaulted {
        Some(PalwFilerPhaseV1::DaDefault)
    } else {
        match &view.phase {
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. } => Some(PalwFilerPhaseV1::DaDefault),
            PalwClaimPhaseV2::Voided {
                reason: PalwVoidReasonV2::CourtFraud | PalwVoidReasonV2::CourtHeldVerdict | PalwVoidReasonV2::CourtDefault,
                ..
            } => Some(PalwFilerPhaseV1::Convicted),
            PalwClaimPhaseV2::Voided { .. } => Some(PalwFilerPhaseV1::Expired),
            _ => None,
        }
    };
    let live = view.record.as_ref().is_some_and(|record| record.live.contains_key(me));
    let closed = view.record.as_ref().is_some_and(|record| record.closed.contains(me));
    PalwFilerClaimFactsV1 {
        ended: false,
        outcome: outcome.or((closed && !live).then_some(PalwFilerPhaseV1::Dismissed)),
        reservable,
        reserved: live,
        session_open: view.sessions.iter().any(|(accuser, _)| accuser == me),
        accusable: !matches!(view.phase, PalwClaimPhaseV2::Provisional),
        binding_known,
    }
}

/// **The reservation `me` files on the claim of `view`**, over the claim's committed roots as the view reads them.
pub fn palw_fraud_filer_reservation_v1(
    view: &crate::palw_state_v2::PalwLegacyDisputeViewV1,
    me: PalwBondKeyV2,
) -> PalwDisputeReservationV1 {
    PalwDisputeReservationV1 {
        version: PALW_DISPUTE_RESERVATION_VERSION_V1,
        claim: view.claim_id,
        execution_root: view.execution_root,
        trace_root: view.trace_root,
        reserver: me,
    }
}

/// **The demand for `probe`, signed by `accuser`** — P2-6's event builder for the binding read, DA-3's held builder for a range or
/// the terminal leaf (each runs the fold's stateless checks before it signs: a fused terminal is refused as LG14-B's dissection).
#[allow(clippy::too_many_arguments)]
pub fn palw_fraud_filer_demand_object_v1(
    network_domain: &Hash64,
    claim: Hash64,
    claim_execution_root: &Hash64,
    probe: PalwLegacyProbeV1,
    binding: Option<&crate::palw_step_leg::PalwStepBindingV2>,
    accuser: PalwBondKeyV2,
    form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<PalwConsensusObjectV2, String> {
    use crate::palw_held_da_v1::PalwHeldMissingV1;
    let missing = match probe {
        PalwLegacyProbeV1::Binding { .. } => {
            return crate::palw_da_rcore_v1::palw_da_accusation_object_v1(network_domain, claim, probe.unit(), accuser, sign)
                .map_err(|e| e.to_string());
        }
        PalwLegacyProbeV1::Range { first, count } => PalwHeldMissingV1::StepRange { first, count },
        PalwLegacyProbeV1::Terminal { leaf } => PalwHeldMissingV1::StepLeaf { leaf },
    };
    let binding = binding.ok_or_else(|| "no binding read off the chain yet".to_string())?.clone();
    crate::palw_da_rcore_v1::palw_da_held_accusation_object_v1(
        network_domain,
        claim,
        claim_execution_root,
        missing,
        binding,
        accuser,
        form,
        sign,
    )
    .map_err(|e| e.to_string())
}

/// **Read one authenticated answer into the case**: the binding (checked against the claim's committed `execution_root`, the root
/// the fold authenticated it against) restarts the bisection over its step leaves; a range's committed leaf hashes are compared with
/// the filer's own (`own_range(first, count)`, from its own run — never the producer's material). A terminal's answer is the fold's
/// to adjudicate: nothing to read.
pub fn palw_fraud_filer_learn_v1(
    probe: PalwLegacyProbeV1,
    answer: &crate::palw_da_rcore_v1::PalwDaAnswerV1,
    claim_execution_root: &Hash64,
    binding: &mut Option<crate::palw_step_leg::PalwStepBindingV2>,
    bisect: &mut PalwLegacyBisectV1,
    own_range: impl FnOnce(u64, u32) -> Result<Vec<Hash64>, String>,
) -> Result<(), String> {
    use crate::palw_da_rcore_v1::PalwDaAnswerV1;
    use crate::palw_held_da_v1::PalwHeldDisclosureV1;
    match (probe, answer) {
        (PalwLegacyProbeV1::Binding { .. }, PalwDaAnswerV1::Event(disclosure)) => {
            let read = disclosure.binding().clone();
            if read.committed_execution_root != *claim_execution_root {
                return Err("the disclosed binding is not the claim's".into());
            }
            *bisect = PalwLegacyBisectV1::new(read.step_leaf_count);
            *binding = Some(read);
            Ok(())
        }
        (PalwLegacyProbeV1::Range { first, count }, PalwDaAnswerV1::Held(carriage)) => {
            let PalwHeldDisclosureV1::StepRange { opening } = &carriage.disclosure else {
                return Err("a range is answered by a range".into());
            };
            let mine = own_range(first, count)?;
            if opening.leaf_hashes.len() != count as usize || mine.len() != count as usize {
                return Err(format!(
                    "a range of {count} leaves read {} committed and {} own hashes",
                    opening.leaf_hashes.len(),
                    mine.len()
                ));
            }
            let matches: Vec<bool> = opening.leaf_hashes.iter().zip(mine.iter()).map(|(committed, own)| committed == own).collect();
            bisect.record_range(first, &matches);
            Ok(())
        }
        (probe, _) => Err(format!("{probe:?} reads nothing from this answer")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(n), 0))
    }

    /// The localizer finds the first divergent leaf of every lie within `max_demands`, whatever the claim's size.
    #[test]
    fn the_bisection_locates_the_first_divergent_leaf() {
        for n in [1u64, 2, 3, 7, 1_024, 1_025, 4_097, 300_000, 1 << 27] {
            for lie in [0, n / 3, n / 2, n - 1] {
                let mut bisect = PalwLegacyBisectV1::new(n);
                let mut demands = 1; // the binding read
                while bisect.located().is_none() {
                    let (first, count) = bisect.next_range().expect("a range inside");
                    let matches: Vec<bool> = (first..first + u64::from(count)).map(|leaf| leaf < lie).collect();
                    bisect.record_range(first, &matches);
                    demands += 1;
                }
                demands += 1; // the terminal
                assert_eq!(bisect.located(), Some(lie), "n {n}, lie {lie}");
                assert!(demands <= PalwLegacyBisectV1::max_demands(n), "n {n}, lie {lie}: {demands} demands");
            }
        }
        assert!(
            PalwLegacyBisectV1::max_demands(1u64 << 40) <= u32::from(PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1),
            "the widest ladder fits"
        );
    }

    /// The engine's step: every role reaches the same terminal from the same facts; only a bond reserves; the binding is read first;
    /// a fused located leaf goes to the held route.
    #[test]
    fn every_role_takes_the_same_road() {
        let located = PalwLegacyBisectV1 { lo: 41, hi: 42 };
        let facts = PalwFilerClaimFactsV1 { reservable: true, accusable: true, binding_known: true, ..Default::default() };
        let terminal = PalwFilerActionV1::Demand(PalwLegacyProbeV1::Terminal { leaf: 41 });
        assert_eq!(
            palw_fraud_filer_next_v1(PalwFilerRoleV1::PublicBond, &facts, true, &located, |_| true),
            PalwFilerActionV1::Reserve
        );
        assert_eq!(palw_fraud_filer_next_v1(PalwFilerRoleV1::Operator, &facts, true, &located, |_| true), PalwFilerActionV1::Reserve);
        assert_eq!(palw_fraud_filer_next_v1(PalwFilerRoleV1::Seat, &facts, true, &located, |_| true), terminal);
        let reserved = PalwFilerClaimFactsV1 { reserved: true, ..facts.clone() };
        for role in [PalwFilerRoleV1::PublicBond, PalwFilerRoleV1::Operator, PalwFilerRoleV1::Seat] {
            assert_eq!(palw_fraud_filer_next_v1(role, &reserved, true, &located, |_| true), terminal);
        }
        assert_eq!(
            palw_fraud_filer_next_v1(PalwFilerRoleV1::PublicBond, &reserved, true, &located, |_| false),
            PalwFilerActionV1::HeldRoute { leaf: 41 }
        );
        let unbound = PalwFilerClaimFactsV1 { binding_known: false, ..reserved.clone() };
        assert_eq!(
            palw_fraud_filer_next_v1(PalwFilerRoleV1::PublicBond, &unbound, true, &located, |_| true),
            PalwFilerActionV1::Demand(PalwLegacyProbeV1::Binding { row: 0, tile: 0 })
        );
        let wide = PalwLegacyBisectV1::new(1 << 20);
        assert_eq!(
            palw_fraud_filer_next_v1(PalwFilerRoleV1::PublicBond, &reserved, true, &wide, |_| true),
            PalwFilerActionV1::Demand(PalwLegacyProbeV1::Range { first: (1 << 19) - 1, count: 1 })
        );
        assert_eq!(
            palw_fraud_filer_next_v1(PalwFilerRoleV1::PublicBond, &reserved, false, &located, |_| true),
            PalwFilerActionV1::Done(PalwFilerPhaseV1::Honest)
        );
        let open = PalwFilerClaimFactsV1 { session_open: true, ..reserved.clone() };
        assert_eq!(palw_fraud_filer_next_v1(PalwFilerRoleV1::PublicBond, &open, true, &located, |_| true), PalwFilerActionV1::Wait);
        let convicted = PalwFilerClaimFactsV1 { outcome: Some(PalwFilerPhaseV1::Convicted), ..reserved };
        assert_eq!(
            palw_fraud_filer_next_v1(PalwFilerRoleV1::PublicBond, &convicted, true, &located, |_| true),
            PalwFilerActionV1::Done(PalwFilerPhaseV1::Convicted)
        );
    }

    /// The record's bookkeeping: once per life, exposure is live plus held.
    #[test]
    fn the_record_counts_reservers_and_exposure() {
        let mut record = PalwDisputeClaimV1 { opened_daa: 5, hard_deadline_daa: 900, ..Default::default() };
        record.live.insert(bond(1), PalwDisputeReservationRowV1 { reserved_daa: 5, deposit: 70, sessions_opened: 0 });
        record.closed.insert(bond(2));
        record.dismissed_held.push((bond(2), 30));
        record.dismissed_held.push((bond(1), 11));
        assert!(record.holds());
        assert_eq!(record.reservers_total(), 2);
        assert!(record.knows(&bond(1)) && record.knows(&bond(2)) && !record.knows(&bond(3)));
        assert_eq!(record.exposure_of(&bond(1)), 81);
        assert_eq!(record.exposure_of(&bond(2)), 30);
        assert_eq!(record.exposure_of(&bond(3)), 0);
    }

    /// The message binds the network, the kind, the signer and every payload byte.
    #[test]
    fn the_message_binds_every_field() {
        let net = Hash64::from_u64_word(7);
        let base = palw_legacy_dispute_message_v1(net, 154, &bond(1), b"payload");
        assert_ne!(base, palw_legacy_dispute_message_v1(Hash64::from_u64_word(8), 154, &bond(1), b"payload"));
        assert_ne!(base, palw_legacy_dispute_message_v1(net, 155, &bond(1), b"payload"));
        assert_ne!(base, palw_legacy_dispute_message_v1(net, 154, &bond(2), b"payload"));
        assert_ne!(base, palw_legacy_dispute_message_v1(net, 154, &bond(1), b"payloae"));
        assert_ne!(PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1, PALW_LEGACY_DISPUTE_MESSAGE_DOMAIN_V1);
    }

    /// **The fence is dormant everywhere**: `None` on every preset, inactive at every height, and refused whenever armed.
    #[test]
    fn the_fence_is_dormant_on_every_preset_and_refused_when_armed() {
        use crate::config::params::{
            devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_launch_params_v1, palw_t12_shipped_params,
        };
        for (name, p) in [
            ("testnet-12", palw_t12_shipped_params()),
            ("testnet-12 launch", palw_t12_launch_params_v1()),
            ("testnet-11", palw_rc_shipped_params()),
            ("devnet", devnet_shipped_params()),
            ("mainnet", mainnet_shipped_params()),
        ] {
            assert_eq!(p.palw_legacy_public_filer_v1, None, "{name}: dormant");
            assert!(!p.palw_legacy_public_filer_active_at(u64::MAX), "{name}: never in force");
            p.validate_palw_legacy_public_filer_v1().expect("dormant passes");
            assert!(
                p.palw_fences_v1().iter().any(|(fence, at)| *fence == "palw_legacy_public_filer_v1" && at.is_none()),
                "{name}: listed"
            );
        }
        let mut armed = palw_t12_shipped_params();
        armed.palw_legacy_public_filer_v1 = Some(ForkActivation::new(9_000_000));
        assert!(
            matches!(armed.validate_palw_legacy_public_filer_v1(), Err(PalwModeV2Error::Invalid(why)) if why.contains("mirror")),
            "an unsynced mirror is refused by name"
        );
        armed.sync_palw_legacy_public_filer_v1();
        assert!(armed.palw_legacy_public_filer_active_at(9_000_000) && !armed.palw_legacy_public_filer_active_at(8_999_999));
        if let PalwConsensusMode::ConsensusV2(bundle) = &armed.palw_consensus_mode {
            assert!(bundle.state.legacy_public_filer_active_at(9_000_000) && !bundle.state.legacy_public_filer_active_at(8_999_999));
        }
        assert!(
            matches!(armed.validate_palw_legacy_public_filer_v1(), Err(PalwModeV2Error::Invalid(why)) if why.contains("cannot be armed yet"))
        );
        let mut never = palw_t12_shipped_params();
        never.palw_legacy_public_filer_v1 = Some(ForkActivation::never());
        assert!(!never.palw_legacy_public_filer_active_at(u64::MAX));
        never.validate_palw_legacy_public_filer_v1().expect("never() is dormant");
        let mut early = palw_t12_shipped_params();
        early.palw_legacy_public_filer_v1 = Some(ForkActivation::new(0));
        early.palw_rcore_plus = None;
        early.sync_palw_legacy_public_filer_v1();
        assert!(
            matches!(early.validate_palw_legacy_public_filer_v1(), Err(PalwModeV2Error::Invalid(why)) if why.contains("palw_rcore_plus"))
        );
    }

    /// **The shared facts read**: a recorded outcome decides first, then the phase; `me`'s ended reservation is `Dismissed`; a claim the
    /// state no longer holds has ended; only `me`'s session and reservation count as `me`'s.
    #[test]
    fn the_facts_read_off_the_public_view() {
        use crate::palw_da_rcore_v1::PalwDaUnitV1;
        use crate::palw_state_v2::{PalwLegacyDisputeViewV1, PalwVoidReasonV2};
        let (me, other) = (bond(1), bond(2));
        let mut view = PalwLegacyDisputeViewV1 {
            claim_id: Hash64::from_u64_word(9),
            class_id: Hash64::default(),
            producer: bond(3),
            phase: PalwClaimPhaseV2::Provisional,
            execution_root: Hash64::from_u64_word(4),
            trace_root: Hash64::from_u64_word(5),
            accepted_daa: 10,
            trace_retention_daa: 5_000,
            job_identity: Hash64::default(),
            work_leaves: 0,
            deadline_daa: None,
            hard_deadline_daa: 3_800,
            record: None,
            sessions: Vec::new(),
            answered: Vec::new(),
            open_courts: 0,
            executor_refuted: false,
            court_convicted: false,
            da_defaulted: false,
        };
        let facts = palw_fraud_filer_facts_v1(Some(&view), &me, true, false);
        assert_eq!(facts, PalwFilerClaimFactsV1 { reservable: true, ..Default::default() }, "Provisional: no session may open yet");
        view.phase = PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 20 };
        let mut record = PalwDisputeClaimV1::default();
        record.live.insert(other, PalwDisputeReservationRowV1 { reserved_daa: 21, deposit: 7, sessions_opened: 0 });
        record.closed.insert(me);
        view.record = Some(record);
        let facts = palw_fraud_filer_facts_v1(Some(&view), &me, false, true);
        assert_eq!(facts.outcome, Some(PalwFilerPhaseV1::Dismissed), "me's pursuit ended without an outcome; another's goes on");
        assert!(facts.accusable && !facts.reserved && facts.binding_known);
        view.record.as_mut().unwrap().closed.clear();
        view.record
            .as_mut()
            .unwrap()
            .live
            .insert(me, PalwDisputeReservationRowV1 { reserved_daa: 22, deposit: 7, sessions_opened: 1 });
        let session = crate::palw_da_rcore_v1::PalwDaSessionV1 {
            opened_daa: 23,
            deadline_daa: 100,
            accuser_is_seat: true,
            exposure: 7,
            units: vec![PalwDaUnitV1::Event { row: 0, tile: 0 }],
            stage: crate::palw_da_rcore_v1::PalwDaStageV1::Licensed,
        };
        view.sessions = vec![(other, session.clone())];
        let facts = palw_fraud_filer_facts_v1(Some(&view), &me, false, true);
        assert!(facts.reserved && !facts.session_open && facts.outcome.is_none(), "another's session is not me's");
        view.sessions.push((me, session));
        assert!(palw_fraud_filer_facts_v1(Some(&view), &me, false, true).session_open);
        view.da_defaulted = true;
        assert_eq!(palw_fraud_filer_facts_v1(Some(&view), &me, false, true).outcome, Some(PalwFilerPhaseV1::DaDefault));
        view.court_convicted = true;
        assert_eq!(palw_fraud_filer_facts_v1(Some(&view), &me, false, true).outcome, Some(PalwFilerPhaseV1::Convicted));
        view.court_convicted = false;
        view.da_defaulted = false;
        view.phase = PalwClaimPhaseV2::Voided { voided_daa: 30, reason: PalwVoidReasonV2::ReceiptTimeout };
        assert_eq!(palw_fraud_filer_facts_v1(Some(&view), &me, false, true).outcome, Some(PalwFilerPhaseV1::Expired));
        assert!(palw_fraud_filer_facts_v1(None, &me, false, true).ended, "a retired claim has ended");
        let reservation = palw_fraud_filer_reservation_v1(&view, me);
        assert_eq!(
            (reservation.claim, reservation.execution_root, reservation.trace_root, reservation.reserver),
            (view.claim_id, view.execution_root, view.trace_root, me)
        );
    }

    /// **The shared demand builder**: the binding read is P2-6's signed `DefaultAccused` of event `(0, 0)`; a held unit needs the
    /// binding read first; an unsigned demand is refused; each probe names the unit the fold records as answered.
    #[test]
    fn the_demand_builder_names_the_probes_unit() {
        use crate::palw_da_rcore_v1::PalwDaUnitV1;
        use crate::palw_held_da_v1::PalwHeldMissingV1;
        let (domain, claim, root, me) = (Hash64::from_u64_word(1), Hash64::from_u64_word(2), Hash64::from_u64_word(3), bond(4));
        let form = crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat;
        let binding_read = PalwLegacyProbeV1::Binding { row: 0, tile: 0 };
        let object = palw_fraud_filer_demand_object_v1(&domain, claim, &root, binding_read, None, me, form, |_, _| Some(vec![7; 8]))
            .expect("the binding read needs no binding");
        assert!(
            matches!(object, PalwConsensusObjectV2::DefaultAccused { claim: c, missing_event_index: 0, accuser, .. } if c == claim && accuser == me)
        );
        assert!(palw_fraud_filer_demand_object_v1(&domain, claim, &root, binding_read, None, me, form, |_, _| None).is_err());
        let range = PalwLegacyProbeV1::Range { first: 5, count: 1 };
        let refusal = palw_fraud_filer_demand_object_v1(&domain, claim, &root, range, None, me, form, |_, _| Some(vec![7; 8]));
        assert!(refusal.is_err_and(|why| why.contains("no binding")));
        assert_eq!(binding_read.unit(), PalwDaUnitV1::Event { row: 0, tile: 0 });
        assert_eq!(range.unit(), PalwDaUnitV1::Held(PalwHeldMissingV1::StepRange { first: 5, count: 1 }));
        assert_eq!(PalwLegacyProbeV1::Terminal { leaf: 9 }.unit(), PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { leaf: 9 }));
    }

    /// **The fence moves the params and schedule ids only when armed** (Some-only), and `Some(never())` is absence.
    #[test]
    fn the_fence_is_hashed_some_only() {
        use crate::config::params::palw_t12_shipped_params;
        let t12 = palw_t12_shipped_params();
        let ids = |p: &Params| (p.consensus_params_id(), p.consensus_identity_id(), p.consensus_schedule_id());
        let mut never = t12.clone();
        never.palw_legacy_public_filer_v1 = Some(ForkActivation::never());
        never.validate_palw_v2().expect("never() is dormant and the ruleset validates");
        // `Some(never())` collapses for the identity and is not hashed into the params id; the schedule id (a report, not a
        // gate) writes the `never()` sentinel like every other Some-only fence's `for_each_fence` arm.
        assert_eq!(never.consensus_identity_id(), t12.consensus_identity_id(), "Some(never()) collapses for the identity");
        assert_eq!(never.consensus_params_id(), t12.consensus_params_id(), "and is not hashed into the params id");
        let mut armed = t12.clone();
        armed.palw_legacy_public_filer_v1 = Some(ForkActivation::new(9_000_000));
        armed.sync_palw_legacy_public_filer_v1();
        assert!(armed.validate_palw_v2().is_err(), "validate_palw_v2 refuses it");
        let ((p0, i0, s0), (p1, i1, s1)) = (ids(&t12), ids(&armed));
        assert_ne!(p0, p1, "the params id names the armed fence");
        assert_eq!(i0, i1, "a future height is not yet a rule for the identity");
        assert_ne!(s0, s1, "the schedule names it");
    }
}
