//! **ADR-0200 C1 / RFC-0033 §2: the D2 readiness digest/path disclosure court** (G14 item 6, lane r5-d2-c1). DORMANT.
//!
//! A seat's tag-230 root ([`super::readiness_root::SeatReadinessRootV1`]) is a Merkle commitment to one keyed digest per drawn
//! inventory coordinate. Root alone cannot be refuted from outside (ADR-0200 "問題"), so C1 adds the supply: any OTHER bond may
//! demand the authenticated digest path of one committed coordinate (an ordinal `< leaf_count` of the seat's own draw — only
//! draw-fixed coordinates are demandable, RFC-0033 §2), and the seat must answer with `(leaf_index, claimed_digest, siblings)`
//! — never model bytes — within the per-turn window.
//!
//! Outcomes (none of them a computation fault or a model review — ADR-0200 "無効pathと非供給の効果を…読み替えない"):
//! * a seat-signed answer whose path authenticates under the recorded root → `Disclosed` (the opening is shared by everyone; the
//!   existing [`super::readiness_root::SeatReadinessRootV1::refute`] then decides a wrong digest with the refuter's own bytes);
//! * a seat-signed answer whose path does NOT authenticate → `PathConvicted` (the seat signed a path its own root refutes);
//! * no answer by the inclusive deadline → `DisclosureDefaulted` ONLY when the F02 supply premise holds
//!   ([`palw_d2_disclosure_supply_premise_v1`] — the pinned court service profile); otherwise `NoSupplyHold` (no default, no
//!   slash: RFC-0033 §2 "実供給…がなければdefault/slashにせずhold").
//!
//! Typed-missing values — every one refuses, `None` is never success: the readiness draw cap
//! ([`super::readiness_root::PALW_D2_READINESS_DRAW_CAP_V1`]), the demand fee schedule ([`PALW_D2_DISCLOSURE_FEE_SCHEDULE_V1`]).
//! The turn is the adopted inclusive notice window ([`crate::palw_court_policy_v1::NOTICE_WINDOW_DAA`], RFC-0026 §2).
//!
//! State: one rooted map ([`PalwD2ReadinessRegistryV1`]) of roots and cases, journaled as delta 230 and carried in tail
//! [`PALW_CARRIAGE_D2_READINESS_TAIL_V1`] only when non-empty (`crate::palw_state_v2::d2_readiness`).
use super::readiness_root::{
    ReadinessDigestPathV1, SeatReadinessRootV1, palw_d2_readiness_root_admit_v1, palw_d2_readiness_root_key_v1,
};
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::{collections::BTreeMap, num::NonZeroU32};

/// Object tag of a disclosure demand (proposed in the D2 allocation, next to 230; see the lane notes).
pub const PALW_D2_DISCLOSURE_DEMAND_TAG_V1: u8 = 231;
/// Object tag of a disclosure answer.
pub const PALW_D2_DISCLOSURE_ANSWER_TAG_V1: u8 = 232;
/// **Carriage tail of the readiness registry.** `0xE5`: checked against all 74 `*_TAIL_V*: u8` values at base 64bc87dcd (0xE4 and
/// 0xE6 are taken, 0xE5 is not; 0xFB is the funded escrow's). Written only when the registry is non-empty.
pub const PALW_CARRIAGE_D2_READINESS_TAIL_V1: u8 = 0xE5;
/// ML-DSA-87 context of a challenger's demand signature.
pub const PALW_D2_DISCLOSURE_DEMAND_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/d2-readiness-demand/mldsa87/v1";
/// ML-DSA-87 context of the seat's answer signature (the literal seat authority).
pub const PALW_D2_DISCLOSURE_ANSWER_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/d2-readiness-answer/mldsa87/v1";
/// The per-turn answer window: the adopted inclusive RFC-0026 notice window (3,120 DAA), not a new value.
pub const PALW_D2_DISCLOSURE_TURN_DAA_V1: u64 = crate::palw_court_policy_v1::NOTICE_WINDOW_DAA;

const DEMAND_DOMAIN: &[u8] = b"MISAKA/PALW/D2/READINESS-DEMAND-MESSAGE/V1";
const ANSWER_DOMAIN: &[u8] = b"MISAKA/PALW/D2/READINESS-ANSWER-MESSAGE/V1";

/// **The demand fee schedule.** A production fee is a user decision (ADR-0200 採択境界: 本番料金は含まない).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwD2DisclosureFeeScheduleV1 {
    /// Sompi the challenger reserves per demand from its free collateral (refunded on `PathConvicted`/`DisclosureDefaulted`,
    /// kept on `Disclosed`; the money movement is the escrow lane's — see the lane notes).
    pub demand_fee_sompi: u64,
}

/// **The adopted demand fee: 1,300,000 sompi (0.013 MSK) per demand.** **Decided 2026-10-22 (lane P3, user delegation of value
/// decisions), awaiting independent verification** — `docs/evidence/palw/studio-values-2026-10-22/values-p3-slash-clock.md` V7:
/// twice the seat's cost of carrying one answer (a depth-6 path + ML-DSA-87 signature in one transaction, ≈ 12,672 B at the
/// 49.96 sompi/B fee observed in `docs/measurements/2026-10-22-devnet-payload-carrier.md`), so a demand never costs an honest seat
/// more than the challenger reserves. Today it is a free-collateral requirement only (the money movement is the escrow lane's).
/// `None` (a caller passing no schedule) still refuses every demand.
pub const PALW_D2_DISCLOSURE_FEE_SCHEDULE_V1: Option<PalwD2DisclosureFeeScheduleV1> =
    Some(PalwD2DisclosureFeeScheduleV1 { demand_fee_sompi: 1_300_000 });

/// **The decided slash of a seat on `PathConvicted` or `DisclosureDefaulted`: 300,000,000 sompi (3 MSK) per root.** Decided
/// 2026-10-22 (lane P3), awaiting independent verification (same record, V7): at least what the challengers spend demanding every
/// one of the 64 ordinals (fee + demand carriage ≈ 1.91 M sompi each, 122.5 M in all) is returned through the ≤ 490‰ finder share
/// (ADR-0032), i.e. ≥ 250 M sompi, rounded up. **No fold applies it yet**: the C1 court records standing only (lane r5-d2-c1 §4);
/// wiring the slash (and the fee movement) is an open dependency, and this constant must not be read as an active penalty.
pub const PALW_D2_READINESS_CONVICTION_SLASH_SOMPI_V1: Option<u64> = Some(300_000_000);

/// **The F02 supply premise for a disclosure default**: a pinned court service profile (RFC-0026 F02 / ADR-0192 A, lane court-f02).
/// Unpinned (today) every unanswered demand is a no-slash hold.
pub fn palw_d2_disclosure_supply_premise_v1() -> bool {
    crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_PROFILE_V1.is_some()
}

/// A challenger's demand: the root (by registry key), the committed ordinal, the fee it reserves, and a replay bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwD2DisclosureDemandV1 {
    pub root_key: Hash64,
    pub ordinal: u32,
    pub challenger: PalwBondKeyV2,
    pub fee_sompi: u64,
    /// The last DAA at which the signed demand may land (a replay never re-opens a closed case).
    pub valid_until_daa: u64,
}

/// The seat's answer: the authenticated digest path of the demanded ordinal. No model bytes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwD2DisclosureAnswerV1 {
    pub root_key: Hash64,
    pub ordinal: u32,
    pub path: ReadinessDigestPathV1,
}

const ORDINAL_DRAW_DOMAIN: &[u8] = b"MISAKA/PALW/D2/READINESS-DEMAND-ORDINAL/V1";

/// **Which committed coordinate a challenger demands** (the node filer's draw; RFC-0033 §2 "drawで固定された座標だけ"): a function
/// of the recorded root (its beacon-bound context, root and count) and the challenger's bond, so distinct challengers draw
/// independent coordinates. Advisory for the node; the fold admits any committed ordinal and refuses one outside the count.
pub fn palw_d2_disclosure_ordinal_draw_v1(root: &SeatReadinessRootV1, challenger: &PalwBondKeyV2) -> Option<u32> {
    if root.leaf_count == 0 {
        return None;
    }
    let seed = misaka_palw_challenge::hash::object_id(ORDINAL_DRAW_DOMAIN, &(root, challenger));
    let word = u64::from_le_bytes(seed[..8].try_into().ok()?);
    Some((word % u64::from(root.leaf_count)) as u32)
}

/// What the challenger's ML-DSA-87 key signs.
pub fn palw_d2_disclosure_demand_message_v1(network_domain: Hash64, demand: &PalwD2DisclosureDemandV1) -> Hash64 {
    Hash64::from_bytes(misaka_palw_challenge::hash::object_id(DEMAND_DOMAIN, &(network_domain, demand)))
}

/// What the seat's ML-DSA-87 key signs.
pub fn palw_d2_disclosure_answer_message_v1(network_domain: Hash64, answer: &PalwD2DisclosureAnswerV1) -> Hash64 {
    Hash64::from_bytes(misaka_palw_challenge::hash::object_id(ANSWER_DOMAIN, &(network_domain, answer)))
}

/// A recorded root's verdict. Only C1 outcomes; no ready credit is ever written here (ADR-0200: no root-only ready door).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwD2ReadinessStandingV1 {
    Committed,
    /// The seat signed a path its own root refutes, at this ordinal.
    PathConvicted {
        ordinal: u32,
        at_daa: u64,
    },
    /// The seat did not answer a supplied demand by the deadline (DA default, no computation fault).
    DisclosureDefaulted {
        ordinal: u32,
        at_daa: u64,
    },
}

/// A recorded tag-230 root.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwD2ReadinessRowV1 {
    pub root: SeatReadinessRootV1,
    pub recorded_daa: u64,
    pub standing: PalwD2ReadinessStandingV1,
}

/// A case's state.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwD2DisclosureOutcomeV1 {
    Open,
    /// The authenticated opening, shared by every later refuter.
    Disclosed {
        path: ReadinessDigestPathV1,
        at_daa: u64,
    },
    PathConvicted {
        at_daa: u64,
    },
    DisclosureDefaulted {
        at_daa: u64,
    },
    /// Deadline passed without the supply premise: held, no default, no slash.
    NoSupplyHold {
        at_daa: u64,
    },
}

/// One disclosure case per `(root, ordinal)`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwD2DisclosureCaseV1 {
    pub demand: PalwD2DisclosureDemandV1,
    pub opened_daa: u64,
    /// Inclusive: an answer landing AT this DAA is in time.
    pub deadline_daa: u64,
    pub outcome: PalwD2DisclosureOutcomeV1,
}

/// Registry key: a root, or a case of a root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum PalwD2ReadinessKeyV1 {
    Root(Hash64),
    Case(Hash64, u32),
}

/// Registry entry; its variant matches its key's ([`palw_d2_readiness_entry_consistent_v1`]).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwD2ReadinessEntryV1 {
    Root(PalwD2ReadinessRowV1),
    Case(PalwD2DisclosureCaseV1),
}

/// The rooted registry (state field `d2_readiness`).
pub type PalwD2ReadinessRegistryV1 = BTreeMap<PalwD2ReadinessKeyV1, PalwD2ReadinessEntryV1>;

/// Carriage invariant: a root row sits under its own registry key, a case under its demand's `(root, ordinal)`, and a case's root
/// is recorded with the ordinal inside its count.
pub fn palw_d2_readiness_entry_consistent_v1(
    registry: &PalwD2ReadinessRegistryV1,
    key: &PalwD2ReadinessKeyV1,
    entry: &PalwD2ReadinessEntryV1,
) -> bool {
    match (key, entry) {
        (PalwD2ReadinessKeyV1::Root(k), PalwD2ReadinessEntryV1::Root(row)) => {
            *k == palw_d2_readiness_root_key_v1(&row.root.context()) && row.root.leaf_count != 0
        }
        (PalwD2ReadinessKeyV1::Case(k, o), PalwD2ReadinessEntryV1::Case(case)) => {
            case.demand.root_key == *k
                && case.demand.ordinal == *o
                && case.deadline_daa == case.opened_daa.saturating_add(PALW_D2_DISCLOSURE_TURN_DAA_V1)
                && matches!(registry.get(&PalwD2ReadinessKeyV1::Root(*k)), Some(PalwD2ReadinessEntryV1::Root(r)) if *o < r.root.leaf_count)
        }
        _ => false,
    }
}

/// Typed refusals of the court.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwD2DisclosureRefusalV1 {
    DrawCapUnadopted,
    RootShape(&'static str),
    RootAlreadyRecorded,
    FeeScheduleUnadopted,
    FeeMismatch,
    UnknownRoot,
    RootNotCommitted,
    OrdinalOutsideDraw,
    SelfDemand,
    DemandExpired,
    CaseExists,
    NoOpenCase,
    AnswerLate,
    AnswerSubstitutes,
}

impl PalwD2DisclosureRefusalV1 {
    /// A static reason for the fold's typed error.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DrawCapUnadopted => "readiness draw cap is unadopted/unknown",
            Self::RootShape(why) => why,
            Self::RootAlreadyRecorded => "a readiness root for this seat and draw is already recorded",
            Self::FeeScheduleUnadopted => "the readiness disclosure fee schedule is unadopted",
            Self::FeeMismatch => "the demand's fee is not the schedule's",
            Self::UnknownRoot => "the demand names no recorded readiness root",
            Self::RootNotCommitted => "the readiness root is already convicted or defaulted",
            Self::OrdinalOutsideDraw => "the demanded ordinal is outside the root's committed draw",
            Self::SelfDemand => "a seat cannot demand its own readiness path",
            Self::DemandExpired => "the demand landed after its signed validity",
            Self::CaseExists => "this readiness coordinate already has a disclosure case",
            Self::NoOpenCase => "no open disclosure case for this coordinate",
            Self::AnswerLate => "the disclosure answer landed after its inclusive deadline",
            Self::AnswerSubstitutes => "the disclosure answer substitutes its coordinate",
        }
    }
}

/// **Record a tag-230 root** (the fold's write, after the validator checked the seat's signature). Cap `None` refuses.
pub fn palw_d2_readiness_record_v1(
    registry: &PalwD2ReadinessRegistryV1,
    root: &SeatReadinessRootV1,
    cap: Option<NonZeroU32>,
    now_daa: u64,
) -> Result<(PalwD2ReadinessKeyV1, PalwD2ReadinessEntryV1), PalwD2DisclosureRefusalV1> {
    if cap.is_none() {
        return Err(PalwD2DisclosureRefusalV1::DrawCapUnadopted);
    }
    palw_d2_readiness_root_admit_v1(root, cap).map_err(PalwD2DisclosureRefusalV1::RootShape)?;
    let key = PalwD2ReadinessKeyV1::Root(palw_d2_readiness_root_key_v1(&root.context()));
    if registry.contains_key(&key) {
        return Err(PalwD2DisclosureRefusalV1::RootAlreadyRecorded);
    }
    Ok((
        key,
        PalwD2ReadinessEntryV1::Root(PalwD2ReadinessRowV1 {
            root: root.clone(),
            recorded_daa: now_daa,
            standing: PalwD2ReadinessStandingV1::Committed,
        }),
    ))
}

fn root_row(registry: &PalwD2ReadinessRegistryV1, root_key: Hash64) -> Result<&PalwD2ReadinessRowV1, PalwD2DisclosureRefusalV1> {
    match registry.get(&PalwD2ReadinessKeyV1::Root(root_key)) {
        Some(PalwD2ReadinessEntryV1::Root(row)) => Ok(row),
        _ => Err(PalwD2DisclosureRefusalV1::UnknownRoot),
    }
}

/// **Open a demand** (signature checked by the validator against `demand.challenger`'s bond key). Refuses with the unadopted cap or
/// fee schedule; the ordinal must be one the seat's own draw committed; the seat cannot demand itself; one case per coordinate.
pub fn palw_d2_disclosure_open_v1(
    registry: &PalwD2ReadinessRegistryV1,
    demand: &PalwD2DisclosureDemandV1,
    cap: Option<NonZeroU32>,
    fees: Option<PalwD2DisclosureFeeScheduleV1>,
    now_daa: u64,
) -> Result<(PalwD2ReadinessKeyV1, PalwD2ReadinessEntryV1), PalwD2DisclosureRefusalV1> {
    cap.ok_or(PalwD2DisclosureRefusalV1::DrawCapUnadopted)?;
    let fees = fees.ok_or(PalwD2DisclosureRefusalV1::FeeScheduleUnadopted)?;
    if demand.fee_sompi != fees.demand_fee_sompi {
        return Err(PalwD2DisclosureRefusalV1::FeeMismatch);
    }
    if now_daa > demand.valid_until_daa {
        return Err(PalwD2DisclosureRefusalV1::DemandExpired);
    }
    let row = root_row(registry, demand.root_key)?;
    if row.standing != PalwD2ReadinessStandingV1::Committed {
        return Err(PalwD2DisclosureRefusalV1::RootNotCommitted);
    }
    if demand.ordinal >= row.root.leaf_count {
        return Err(PalwD2DisclosureRefusalV1::OrdinalOutsideDraw);
    }
    if demand.challenger == row.root.seat {
        return Err(PalwD2DisclosureRefusalV1::SelfDemand);
    }
    let key = PalwD2ReadinessKeyV1::Case(demand.root_key, demand.ordinal);
    if registry.contains_key(&key) {
        return Err(PalwD2DisclosureRefusalV1::CaseExists);
    }
    let deadline_daa = now_daa.checked_add(PALW_D2_DISCLOSURE_TURN_DAA_V1).ok_or(PalwD2DisclosureRefusalV1::DemandExpired)?;
    Ok((
        key,
        PalwD2ReadinessEntryV1::Case(PalwD2DisclosureCaseV1 {
            demand: *demand,
            opened_daa: now_daa,
            deadline_daa,
            outcome: PalwD2DisclosureOutcomeV1::Open,
        }),
    ))
}

/// **Fold the seat's answer** (signature checked by the validator against the ROOT's seat bond). Returns the new case and, on a
/// wrong path, the new root row (`PathConvicted`). A late answer or one naming another coordinate is refused (no effect).
pub fn palw_d2_disclosure_answer_v1(
    registry: &PalwD2ReadinessRegistryV1,
    answer: &PalwD2DisclosureAnswerV1,
    now_daa: u64,
) -> Result<Vec<(PalwD2ReadinessKeyV1, PalwD2ReadinessEntryV1)>, PalwD2DisclosureRefusalV1> {
    let key = PalwD2ReadinessKeyV1::Case(answer.root_key, answer.ordinal);
    let Some(PalwD2ReadinessEntryV1::Case(case)) = registry.get(&key) else {
        return Err(PalwD2DisclosureRefusalV1::NoOpenCase);
    };
    if case.outcome != PalwD2DisclosureOutcomeV1::Open {
        return Err(PalwD2DisclosureRefusalV1::NoOpenCase);
    }
    if now_daa > case.deadline_daa {
        return Err(PalwD2DisclosureRefusalV1::AnswerLate);
    }
    let row = root_row(registry, answer.root_key)?;
    let mut case = case.clone();
    match row.root.check_path_at_ordinal_v1(answer.ordinal, &answer.path) {
        Ok(()) => {
            case.outcome = PalwD2DisclosureOutcomeV1::Disclosed { path: answer.path.clone(), at_daa: now_daa };
            Ok(vec![(key, PalwD2ReadinessEntryV1::Case(case))])
        }
        Err(_) => {
            case.outcome = PalwD2DisclosureOutcomeV1::PathConvicted { at_daa: now_daa };
            let mut convicted = row.clone();
            if convicted.standing == PalwD2ReadinessStandingV1::Committed {
                convicted.standing = PalwD2ReadinessStandingV1::PathConvicted { ordinal: answer.ordinal, at_daa: now_daa };
            }
            Ok(vec![
                (key, PalwD2ReadinessEntryV1::Case(case)),
                (PalwD2ReadinessKeyV1::Root(answer.root_key), PalwD2ReadinessEntryV1::Root(convicted)),
            ])
        }
    }
}

/// **The deadline sweep** at `now_daa`: every `Open` case whose inclusive deadline is strictly behind becomes `DisclosureDefaulted`
/// (and its root `DisclosureDefaulted`) under the supply premise, else `NoSupplyHold`. Returns the writes in key order.
pub fn palw_d2_disclosure_sweep_v1(
    registry: &PalwD2ReadinessRegistryV1,
    now_daa: u64,
    supply_premise: bool,
) -> Vec<(PalwD2ReadinessKeyV1, PalwD2ReadinessEntryV1)> {
    palw_d2_disclosure_sweep_with_supply_v1(registry, now_daa, &|_| supply_premise)
}

/// **audit B-08 (fix-b): the sweep with a PER-CASE supply fact.** `supplied(case)` answers whether this case's own response window
/// (`opened_daa ..= deadline_daa`) had a recorded court service opportunity; only such a case defaults, every other one is a
/// no-supply hold. (The global premise flag alone defaulted every case once a profile was pinned, with no per-case observation.)
pub fn palw_d2_disclosure_sweep_with_supply_v1(
    registry: &PalwD2ReadinessRegistryV1,
    now_daa: u64,
    supplied: &dyn Fn(&PalwD2DisclosureCaseV1) -> bool,
) -> Vec<(PalwD2ReadinessKeyV1, PalwD2ReadinessEntryV1)> {
    let mut writes: Vec<(PalwD2ReadinessKeyV1, PalwD2ReadinessEntryV1)> = Vec::new();
    let mut roots: BTreeMap<Hash64, PalwD2ReadinessRowV1> = BTreeMap::new();
    for (key, entry) in registry {
        let (PalwD2ReadinessKeyV1::Case(root_key, ordinal), PalwD2ReadinessEntryV1::Case(case)) = (key, entry) else { continue };
        if case.outcome != PalwD2DisclosureOutcomeV1::Open || now_daa <= case.deadline_daa {
            continue;
        }
        let mut closed = case.clone();
        if supplied(case) {
            closed.outcome = PalwD2DisclosureOutcomeV1::DisclosureDefaulted { at_daa: now_daa };
            if let Ok(row) = root_row(registry, *root_key) {
                let row = roots.entry(*root_key).or_insert_with(|| row.clone());
                if row.standing == PalwD2ReadinessStandingV1::Committed {
                    row.standing = PalwD2ReadinessStandingV1::DisclosureDefaulted { ordinal: *ordinal, at_daa: now_daa };
                }
            }
        } else {
            closed.outcome = PalwD2DisclosureOutcomeV1::NoSupplyHold { at_daa: now_daa };
        }
        writes.push((*key, PalwD2ReadinessEntryV1::Case(closed)));
    }
    for (root_key, row) in roots {
        writes.push((PalwD2ReadinessKeyV1::Root(root_key), PalwD2ReadinessEntryV1::Root(row)));
    }
    writes
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(n: u32) -> Hash64 {
        let mut x = [0; 64];
        x[..4].copy_from_slice(&n.to_le_bytes());
        Hash64::from_bytes(x)
    }
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(n), index: 0 })
    }
    pub(crate) fn fixture(count: u32) -> (SeatReadinessRootV1, Vec<(u32, Hash64)>) {
        let ctx = super::super::readiness_root::ReadinessContextV1 { beacon: h(1), class_id: h(2), seat: bond(3), span: 4 };
        let ds: Vec<_> = (0..count).map(|i| (i * 3, h(500 + i))).collect();
        (SeatReadinessRootV1::from_digests(ctx, &ds, NonZeroU32::new(count)).unwrap(), ds)
    }
    const FEES: Option<PalwD2DisclosureFeeScheduleV1> = Some(PalwD2DisclosureFeeScheduleV1 { demand_fee_sompi: 7 });

    fn recorded(count: u32) -> (PalwD2ReadinessRegistryV1, Hash64, SeatReadinessRootV1, Vec<(u32, Hash64)>) {
        let (root, ds) = fixture(count);
        let mut reg = PalwD2ReadinessRegistryV1::new();
        let (k, e) = palw_d2_readiness_record_v1(&reg, &root, NonZeroU32::new(count), 10).unwrap();
        reg.insert(k, e);
        let PalwD2ReadinessKeyV1::Root(rk) = k else { unreachable!() };
        (reg, rk, root, ds)
    }
    fn demand(rk: Hash64, ordinal: u32) -> PalwD2DisclosureDemandV1 {
        PalwD2DisclosureDemandV1 { root_key: rk, ordinal, challenger: bond(9), fee_sompi: 7, valid_until_daa: 1_000 }
    }

    /// No cap or no fee schedule passed: recording and demanding refuse; `None` is never success.
    #[test]
    fn d2c1_unadopted_cap_and_fee_refuse_record_and_demand() {
        let (root, _) = fixture(3);
        let reg = PalwD2ReadinessRegistryV1::new();
        assert_eq!(palw_d2_readiness_record_v1(&reg, &root, None, 1), Err(PalwD2DisclosureRefusalV1::DrawCapUnadopted));
        let (reg, rk, _, _) = recorded(3);
        assert_eq!(palw_d2_disclosure_open_v1(&reg, &demand(rk, 0), None, FEES, 20), Err(PalwD2DisclosureRefusalV1::DrawCapUnadopted));
        assert_eq!(
            palw_d2_disclosure_open_v1(&reg, &demand(rk, 0), NonZeroU32::new(3), None, 20),
            Err(PalwD2DisclosureRefusalV1::FeeScheduleUnadopted)
        );
        assert!(palw_d2_disclosure_supply_premise_v1(), "the adopted court service profile supplies the default premise");
    }

    /// V7 (decided 2026-10-22, lane P3): the shipped cap and fee admit a record and a demand that pays exactly the adopted fee; a
    /// demand naming any other fee is refused; the slash bound covers 64 demands through the 490‰ finder share.
    #[test]
    fn p3_d2c1_adopted_cap_and_fee_record_and_demand() {
        let cap = super::super::readiness_root::PALW_D2_READINESS_DRAW_CAP_V1;
        let fees = PALW_D2_DISCLOSURE_FEE_SCHEDULE_V1;
        assert_eq!(fees.map(|f| f.demand_fee_sompi), Some(1_300_000));
        let (root, _) = fixture(3);
        let mut reg = PalwD2ReadinessRegistryV1::new();
        let (k, e) = palw_d2_readiness_record_v1(&reg, &root, cap, 10).expect("the adopted cap records a 3-leaf root");
        reg.insert(k, e);
        let PalwD2ReadinessKeyV1::Root(rk) = k else { unreachable!() };
        let mut d = demand(rk, 1);
        d.fee_sompi = 1_300_000;
        assert!(palw_d2_disclosure_open_v1(&reg, &d, cap, fees, 20).is_ok());
        d.fee_sompi = 1_299_999;
        assert_eq!(palw_d2_disclosure_open_v1(&reg, &d, cap, fees, 20), Err(PalwD2DisclosureRefusalV1::FeeMismatch));
        let slash = PALW_D2_READINESS_CONVICTION_SLASH_SOMPI_V1.expect("decided");
        let all_demands = 64 * (1_300_000 + 614_000u64);
        assert!(slash * 490 / 1_000 >= all_demands, "the finder share repays a challenger who demanded every ordinal");
    }

    /// Demand admission boundaries: fee, replay bound, draw ordinal, self-demand, one case per coordinate, re-record refused.
    #[test]
    fn d2c1_demand_admission_boundaries() {
        let (reg, rk, root, _) = recorded(4);
        let cap = NonZeroU32::new(4);
        assert_eq!(palw_d2_readiness_record_v1(&reg, &root, cap, 11), Err(PalwD2DisclosureRefusalV1::RootAlreadyRecorded));
        let mut d = demand(rk, 3);
        d.fee_sompi = 6;
        assert_eq!(palw_d2_disclosure_open_v1(&reg, &d, cap, FEES, 20), Err(PalwD2DisclosureRefusalV1::FeeMismatch));
        assert_eq!(
            palw_d2_disclosure_open_v1(&reg, &demand(rk, 4), cap, FEES, 20),
            Err(PalwD2DisclosureRefusalV1::OrdinalOutsideDraw)
        );
        assert_eq!(palw_d2_disclosure_open_v1(&reg, &demand(rk, 0), cap, FEES, 1_001), Err(PalwD2DisclosureRefusalV1::DemandExpired));
        assert!(palw_d2_disclosure_open_v1(&reg, &demand(rk, 0), cap, FEES, 1_000).is_ok(), "valid_until is inclusive");
        let mut own = demand(rk, 0);
        own.challenger = root.seat;
        assert_eq!(palw_d2_disclosure_open_v1(&reg, &own, cap, FEES, 20), Err(PalwD2DisclosureRefusalV1::SelfDemand));
        assert_eq!(palw_d2_disclosure_open_v1(&reg, &demand(h(77), 0), cap, FEES, 20), Err(PalwD2DisclosureRefusalV1::UnknownRoot));
        let mut reg = reg;
        let (k, e) = palw_d2_disclosure_open_v1(&reg, &demand(rk, 2), cap, FEES, 20).unwrap();
        let PalwD2ReadinessEntryV1::Case(c) = &e else { unreachable!() };
        assert_eq!(c.deadline_daa, 20 + 3_120);
        reg.insert(k, e);
        assert!(reg.iter().all(|(k, e)| palw_d2_readiness_entry_consistent_v1(&reg, k, e)));
        assert_eq!(palw_d2_disclosure_open_v1(&reg, &demand(rk, 2), cap, FEES, 21), Err(PalwD2DisclosureRefusalV1::CaseExists));
    }

    fn honest_path(root: &SeatReadinessRootV1, ds: &[(u32, Hash64)], ordinal: usize) -> ReadinessDigestPathV1 {
        super::super::readiness_root::palw_d2_readiness_path_v1(&root.context(), ds, ordinal)
    }

    /// Answer: an authenticated path discloses (deadline inclusive); a wrong path convicts the seat (root standing moves);
    /// a late answer and a second answer are refused.
    #[test]
    fn d2c1_answer_discloses_or_convicts_and_is_inclusive_at_the_deadline() {
        let cap = NonZeroU32::new(5);
        for wrong in [false, true] {
            let (mut reg, rk, root, ds) = recorded(5);
            let (k, e) = palw_d2_disclosure_open_v1(&reg, &demand(rk, 1), cap, FEES, 20).unwrap();
            reg.insert(k, e);
            let mut path = honest_path(&root, &ds, 1);
            if wrong {
                path.claimed_digest = h(999);
            }
            let answer = PalwD2DisclosureAnswerV1 { root_key: rk, ordinal: 1, path: path.clone() };
            assert_eq!(palw_d2_disclosure_answer_v1(&reg, &answer, 20 + 3_121), Err(PalwD2DisclosureRefusalV1::AnswerLate));
            let writes = palw_d2_disclosure_answer_v1(&reg, &answer, 20 + 3_120).unwrap();
            for (k, e) in writes {
                reg.insert(k, e);
            }
            let PalwD2ReadinessEntryV1::Case(case) = &reg[&PalwD2ReadinessKeyV1::Case(rk, 1)] else { unreachable!() };
            let PalwD2ReadinessEntryV1::Root(row) = &reg[&PalwD2ReadinessKeyV1::Root(rk)] else { unreachable!() };
            if wrong {
                assert_eq!(case.outcome, PalwD2DisclosureOutcomeV1::PathConvicted { at_daa: 3_140 });
                assert_eq!(row.standing, PalwD2ReadinessStandingV1::PathConvicted { ordinal: 1, at_daa: 3_140 });
                // Signed validity is checked before the already-convicted root standing.
                assert_eq!(
                    palw_d2_disclosure_open_v1(&reg, &demand(rk, 2), cap, FEES, 3_141),
                    Err(PalwD2DisclosureRefusalV1::DemandExpired)
                );
                let mut next = demand(rk, 2);
                next.valid_until_daa = 3_141;
                assert_eq!(
                    palw_d2_disclosure_open_v1(&reg, &next, cap, FEES, 3_141),
                    Err(PalwD2DisclosureRefusalV1::RootNotCommitted)
                );
            } else {
                assert_eq!(case.outcome, PalwD2DisclosureOutcomeV1::Disclosed { path, at_daa: 3_140 });
                assert_eq!(row.standing, PalwD2ReadinessStandingV1::Committed);
            }
            assert_eq!(palw_d2_disclosure_answer_v1(&reg, &answer, 3_140), Err(PalwD2DisclosureRefusalV1::NoOpenCase));
            let other = PalwD2DisclosureAnswerV1 { root_key: rk, ordinal: 0, path: honest_path(&root, &ds, 0) };
            assert_eq!(palw_d2_disclosure_answer_v1(&reg, &other, 30), Err(PalwD2DisclosureRefusalV1::NoOpenCase));
        }
    }

    /// Sweep: nothing at the deadline itself; past it, no supply premise → `NoSupplyHold` (root untouched); with it → DA default.
    #[test]
    fn d2c1_unanswered_demand_defaults_only_under_the_supply_premise() {
        let cap = NonZeroU32::new(3);
        let (mut reg, rk, _, _) = recorded(3);
        let (k, e) = palw_d2_disclosure_open_v1(&reg, &demand(rk, 0), cap, FEES, 20).unwrap();
        reg.insert(k, e);
        assert!(palw_d2_disclosure_sweep_v1(&reg, 3_140, true).is_empty(), "the deadline DAA is still in time");
        let held = palw_d2_disclosure_sweep_v1(&reg, 3_141, false);
        assert_eq!(held.len(), 1);
        assert!(
            matches!(&held[0].1, PalwD2ReadinessEntryV1::Case(c) if c.outcome == PalwD2DisclosureOutcomeV1::NoSupplyHold { at_daa: 3_141 })
        );
        let defaulted = palw_d2_disclosure_sweep_v1(&reg, 3_141, true);
        assert_eq!(defaulted.len(), 2);
        assert!(matches!(&defaulted[1].1, PalwD2ReadinessEntryV1::Root(r)
            if r.standing == PalwD2ReadinessStandingV1::DisclosureDefaulted { ordinal: 0, at_daa: 3_141 }));
    }

    /// The node's ordinal draw: inside the count, deterministic, challenger-dependent across a population.
    #[test]
    fn d2c1_ordinal_draw_is_inside_the_count_and_per_challenger() {
        let (root, _) = fixture(16);
        let draws: std::collections::BTreeSet<u32> =
            (0..64).map(|n| palw_d2_disclosure_ordinal_draw_v1(&root, &bond(100 + n)).unwrap()).collect();
        assert!(draws.iter().all(|o| *o < 16));
        assert!(draws.len() > 1, "challengers draw independently");
        assert_eq!(palw_d2_disclosure_ordinal_draw_v1(&root, &bond(100)), palw_d2_disclosure_ordinal_draw_v1(&root, &bond(100)));
        let mut empty = root.clone();
        empty.leaf_count = 0;
        assert_eq!(palw_d2_disclosure_ordinal_draw_v1(&empty, &bond(100)), None);
    }

    /// Wire: demand/answer borsh round-trip; the signed messages bind network and every field.
    #[test]
    fn d2c1_messages_bind_network_and_fields() {
        let (_, rk, root, ds) = recorded(2);
        let d = demand(rk, 1);
        assert_eq!(borsh::from_slice::<PalwD2DisclosureDemandV1>(&borsh::to_vec(&d).unwrap()).unwrap(), d);
        let a = PalwD2DisclosureAnswerV1 { root_key: rk, ordinal: 1, path: honest_path(&root, &ds, 1) };
        assert_eq!(borsh::from_slice::<PalwD2DisclosureAnswerV1>(&borsh::to_vec(&a).unwrap()).unwrap(), a);
        let m = palw_d2_disclosure_demand_message_v1(h(1), &d);
        assert_ne!(m, palw_d2_disclosure_demand_message_v1(h(2), &d));
        let mut d2 = d;
        d2.ordinal = 0;
        assert_ne!(m, palw_d2_disclosure_demand_message_v1(h(1), &d2));
        let am = palw_d2_disclosure_answer_message_v1(h(1), &a);
        let mut a2 = a.clone();
        a2.path.claimed_digest = h(3);
        assert_ne!(am, palw_d2_disclosure_answer_message_v1(h(1), &a2));
        assert_ne!(PALW_D2_DISCLOSURE_DEMAND_MLDSA87_CONTEXT_V1, PALW_D2_DISCLOSURE_ANSWER_MLDSA87_CONTEXT_V1);
    }
}
