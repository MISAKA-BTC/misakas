//! **Elastic Final capacity** (RFC-0016 §10). Pure arithmetic and data structures; the fold reads them through
//! `palw_state_v2::palw_rfc0016_fold_v1`.
//!
//! * [`palw_waterfill_capacity_v1`] — each ready verifier's per-span replay budget split across the classes it is ready for, in
//!   proportion to their demand: a verifier is counted ONCE (its allocations sum to at most its budget), and a class's capacity comes
//!   only from verifiers that hold it — no other class's demand is subtracted from a disjoint verifier set.
//! * [`palw_c7_derived_cap_v1`] — the long-window (C7, the 2M row) concurrency derived from measured capacity, under its measured ceiling.
//! * [`PalwFinalQueueV1`] — the bounded, deadline-preserving pending Final queue: one reservation per claim, a per-bond share, a TTL;
//!   the verification window starts when a claim LEAVES the queue (never Final before its checks).
//! * [`PalwAdmissionQuoteV1`] — the pre-lottery quote combining V+ rights, capacity and the quote margin `h`.
//! * [`palw_verifier_share_permille_v1`] — the M\*-49 verifier-share slope hook, within ADR-0132 C's 10–30 %.
//! * [`palw_opv_check_budget_v1`] — the OPV route's per-model check budget, scaled with the claim rate.

use std::collections::{BTreeMap, BTreeSet};

use crate::Hash64;
use crate::palw_state_v2::PalwBondKeyV2;

use super::rights::PalwRightsQuoteV1;
use super::values::{PALW_RFC0016_VERIFIER_SHARE_MAX_PERMILLE_V1, PALW_RFC0016_VERIFIER_SHARE_MIN_PERMILLE_V1, PalwRfc0016ValuesV1};

/// One ready verifier as the capacity account sees it: its per-span replay budget and the classes it is ready for (deduplicated).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwVerifierSupplyV1 {
    pub per_span: u128,
    pub classes: BTreeSet<Hash64>,
}

/// **Per-verifier water-filling.** For each verifier, its budget is split over ITS classes in proportion to their `demand` (equally,
/// floored, where none of them has demand). Returns each class's capacity per span. Every verifier's allocations sum to at most its
/// budget, so `Σ_class capacity ≤ Σ_verifier budget` — no verifier is counted twice across classes or models.
pub fn palw_waterfill_capacity_v1(verifiers: &[PalwVerifierSupplyV1], demand: &BTreeMap<Hash64, u128>) -> BTreeMap<Hash64, u128> {
    let mut capacity: BTreeMap<Hash64, u128> = BTreeMap::new();
    for v in verifiers {
        if v.classes.is_empty() || v.per_span == 0 {
            continue;
        }
        let total: u128 = v.classes.iter().map(|c| demand.get(c).copied().unwrap_or(0)).fold(0u128, u128::saturating_add);
        for class in &v.classes {
            let share = if total == 0 {
                v.per_span / v.classes.len() as u128
            } else {
                let d = demand.get(class).copied().unwrap_or(0);
                // ⌊per_span · d / total⌋ without overflow: split into whole and remainder parts.
                let whole = (v.per_span / total).saturating_mul(d);
                let part = (v.per_span % total).checked_mul(d).map(|x| x / total).unwrap_or_else(|| {
                    // Fall back to the coarser product (still ≤ the exact share).
                    ((v.per_span % total) / total.max(1)).saturating_mul(d)
                });
                whole.saturating_add(part).min(v.per_span)
            };
            let slot = capacity.entry(*class).or_insert(0);
            *slot = slot.saturating_add(share);
        }
    }
    capacity
}

/// **Does the class's demand (with the candidate claim) fit its water-filled capacity?** Capacity zero never admits.
pub fn palw_capacity_fits_v1(capacity_per_span: u128, demand_per_span: u128) -> bool {
    capacity_per_span > 0 && demand_per_span <= capacity_per_span
}

/// **The long-window (C7) concurrency, derived** (replaces the static `max_inflight_claims` past the fence): `⌊σ_cap · capacity ·
/// window / claim_replay⌋`, at least one when there is any capacity, zero when there is none, and never above the MEASURED ceiling
/// (`c7_measured_concurrency`, `0` = no ceiling measured).
pub fn palw_c7_derived_cap_v1(capacity_per_span: u128, window_spans: u64, claim_replay: u128, values: &PalwRfc0016ValuesV1) -> u32 {
    if capacity_per_span == 0 || claim_replay == 0 {
        return 0;
    }
    let raw = capacity_per_span.saturating_mul(window_spans as u128) / claim_replay;
    let safe = raw.saturating_mul(values.capacity_safety_permille.min(1_000) as u128) / 1_000;
    let mut cap = safe.min(u32::MAX as u128) as u32;
    cap = cap.max(1);
    if values.c7_measured_concurrency > 0 {
        cap = cap.min(values.c7_measured_concurrency);
    }
    cap
}

/// **The capacity shortfall a model reports** (RFC-0016 §10, never a silent Final delay): the per-span replay missing for the demand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCapacityShortV1 {
    pub capacity_per_span: u128,
    pub demand_per_span: u128,
    pub shortfall_per_span: u128,
    /// Ready verifiers holding the class (each counted once).
    pub ready_verifiers: u32,
}

pub fn palw_capacity_short_v1(capacity_per_span: u128, demand_per_span: u128, ready_verifiers: u32) -> Option<PalwCapacityShortV1> {
    (demand_per_span > capacity_per_span || capacity_per_span == 0).then_some(PalwCapacityShortV1 {
        capacity_per_span,
        demand_per_span,
        shortfall_per_span: demand_per_span.saturating_sub(capacity_per_span),
        ready_verifiers,
    })
}

/// One pending claim in a class's Final queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwFinalQueueEntryV1 {
    pub claim: Hash64,
    pub bond: PalwBondKeyV2,
    pub queued_daa: u64,
    /// Block rights / budget reserved for this claim while it waits: at most ONE claim's worth (`1`), never more.
    pub reserved_rights: u64,
}

/// Why a claim does not enter the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwFinalQueueRefusalV1 {
    /// Capacity is zero: no claim is queued (nothing could ever serve it).
    CapacityZero,
    QueueFull,
    BondShareFull,
    Duplicate,
    /// A queued claim may reserve at most one claim's rights.
    OverReservation,
}

/// **The bounded, deadline-preserving pending Final queue of one class** (RFC-0016 §10). FIFO by `queued_daa`, then claim id; a
/// bond holds at most its capital share of the queue's length; an entry past `queue_ttl_daa` is voided without slash. A claim's
/// verification window and deadlines start when it LEAVES the queue ([`Self::dequeue_v1`] returns that DAA); while queued it is
/// never Final and earns nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwFinalQueueV1 {
    pub entries: Vec<PalwFinalQueueEntryV1>,
}

impl PalwFinalQueueV1 {
    /// Enqueue, or refuse. `bond_share_cap` is the bond's ceiling in entries (its capital share of `queue_len_per_class`, ≥ 1).
    pub fn enqueue_v1(
        &mut self,
        entry: PalwFinalQueueEntryV1,
        capacity_per_span: u128,
        bond_share_cap: u32,
        values: &PalwRfc0016ValuesV1,
    ) -> Result<u32, PalwFinalQueueRefusalV1> {
        if capacity_per_span == 0 {
            return Err(PalwFinalQueueRefusalV1::CapacityZero);
        }
        if entry.reserved_rights > 1 {
            return Err(PalwFinalQueueRefusalV1::OverReservation);
        }
        if self.entries.iter().any(|e| e.claim == entry.claim) {
            return Err(PalwFinalQueueRefusalV1::Duplicate);
        }
        if self.entries.len() as u64 >= values.queue_len_per_class as u64 {
            return Err(PalwFinalQueueRefusalV1::QueueFull);
        }
        let held = self.entries.iter().filter(|e| e.bond == entry.bond).count() as u64;
        if held >= bond_share_cap.max(1) as u64 {
            return Err(PalwFinalQueueRefusalV1::BondShareFull);
        }
        self.entries.push(entry);
        self.entries.sort_by(|a, b| (a.queued_daa, a.claim).cmp(&(b.queued_daa, b.claim)));
        Ok(self.entries.iter().position(|e| e.claim == entry.claim).unwrap_or(0) as u32)
    }

    /// Void every entry whose wait passed `queue_ttl_daa` (no slash; the issuance capacity it consumed is NOT returned early).
    pub fn expire_v1(&mut self, now_daa: u64, values: &PalwRfc0016ValuesV1) -> Vec<Hash64> {
        let ttl = values.queue_ttl_daa;
        let (gone, kept): (Vec<_>, Vec<_>) = self.entries.iter().copied().partition(|e| e.queued_daa.saturating_add(ttl) < now_daa);
        self.entries = kept;
        gone.into_iter().map(|e| e.claim).collect()
    }

    /// Take up to `n` entries in order; each one's verification window starts at `now_daa` (deadline-preserving).
    pub fn dequeue_v1(&mut self, n: usize, now_daa: u64) -> Vec<(Hash64, u64)> {
        let take = n.min(self.entries.len());
        self.entries.drain(..take).map(|e| (e.claim, now_daa)).collect()
    }

    /// The rights all queued claims reserve together — at most one each.
    pub fn reserved_rights_v1(&self) -> u64 {
        self.entries.iter().map(|e| e.reserved_rights.min(1)).sum()
    }

    /// Queue wait of each entry at `now_daa` (telemetry).
    pub fn waits_v1(&self, now_daa: u64) -> Vec<u64> {
        self.entries.iter().map(|e| now_daa.saturating_sub(e.queued_daa)).collect()
    }
}

/// A bond's ceiling in queue entries: `⌈L · C_b / ΣC⌉` over the bonds active in the class's queue (the asking bond included), at least 1.
pub fn palw_queue_bond_share_cap_v1(capital: u128, active_capital: u128, values: &PalwRfc0016ValuesV1) -> u32 {
    let len = values.queue_len_per_class as u128;
    if active_capital == 0 {
        return values.queue_len_per_class.max(1);
    }
    (len.saturating_mul(capital).div_ceil(active_capital)).clamp(1, u32::MAX as u128) as u32
}

/// The verdict of the pre-lottery quote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwQuoteVerdictV1 {
    /// Rights and capacity hold with the margin `h` to spare: mining now is admitted unless more than `h` concurrent winners race.
    Admit,
    /// Admitted now, but within the margin: a concurrent winner may take it (the race is absorbed by the queue or reported).
    AdmitWithinMargin,
    /// Capacity is short: the claim would wait in the queue (its window starts when it leaves).
    Queue,
    /// Refused before any forward is spent (the code names why: rights, capacity zero, queue full).
    Refuse(&'static str),
}

/// **The combined pre-lottery admission quote** (RFC-0016 §8.1 step 7, §10): read BEFORE an Attempt's forward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwAdmissionQuoteV1 {
    pub rights: Option<PalwRightsQuoteV1>,
    pub capacity_per_span: Option<u128>,
    pub demand_per_span: Option<u128>,
    pub short: Option<PalwCapacityShortV1>,
    pub queue_len: u32,
    pub margin_h: u32,
    pub verdict: PalwQuoteVerdictV1,
}

/// Combine the parts. `rights` is `None` below the V+ fence; `capacity` is `None` below the capacity fence.
pub fn palw_admission_quote_v1(
    rights: Option<PalwRightsQuoteV1>,
    capacity: Option<(u128, u128, u32)>,
    queue_len: u32,
    values: &PalwRfc0016ValuesV1,
) -> PalwAdmissionQuoteV1 {
    let h = values.quote_margin_h;
    let mut verdict = PalwQuoteVerdictV1::Admit;
    if let Some(q) = rights {
        match q.verdict {
            Err(refusal) => verdict = PalwQuoteVerdictV1::Refuse(refusal.code()),
            Ok(_) => {
                let room = q.remaining_own.max(q.reissue_room).min(q.epoch_total.saturating_sub(q.epoch_used));
                if room <= h as u64 {
                    verdict = PalwQuoteVerdictV1::AdmitWithinMargin;
                }
            }
        }
    }
    let (capacity_per_span, demand_per_span, short) = match capacity {
        Some((cap, demand, ready)) => (Some(cap), Some(demand), palw_capacity_short_v1(cap, demand, ready)),
        None => (None, None, None),
    };
    if !matches!(verdict, PalwQuoteVerdictV1::Refuse(_))
        && let Some(s) = short
    {
        verdict = if s.capacity_per_span == 0 {
            PalwQuoteVerdictV1::Refuse("RFC0016_CAPACITY_ZERO")
        } else if queue_len.saturating_add(h) >= values.queue_len_per_class {
            PalwQuoteVerdictV1::Refuse("RFC0016_QUEUE_FULL")
        } else {
            PalwQuoteVerdictV1::Queue
        };
    }
    PalwAdmissionQuoteV1 { rights, capacity_per_span, demand_per_span, short, queue_len, margin_h: h, verdict }
}

/// **The M\*-49 verifier-share slope hook**: `clamp(base + γ · (util − u*), 10 %, 30 %)` in permille — a popular (high-utilization)
/// model's verifiers take more of the same claim's pay; the total is unchanged.
pub fn palw_verifier_share_permille_v1(utilization_permille: u32, values: &PalwRfc0016ValuesV1) -> u32 {
    let delta = (utilization_permille as i64 - values.verifier_util_target_permille as i64)
        * values.verifier_slope_gamma_permille as i64
        / 1_000;
    (values.verifier_share_base_permille as i64 + delta)
        .clamp(PALW_RFC0016_VERIFIER_SHARE_MIN_PERMILLE_V1 as i64, PALW_RFC0016_VERIFIER_SHARE_MAX_PERMILLE_V1 as i64) as u32
}

/// **The OPV route's per-model check budget** for an epoch with `claims` claims: `⌈claims · checks_per_claim⌉`, so the checked share
/// does not thin as a model becomes popular (replaces the fixed `N = 1` of the interim M\*-49 terms).
pub fn palw_opv_check_budget_v1(claims: u64, values: &PalwRfc0016ValuesV1) -> u64 {
    ((claims as u128).saturating_mul(values.opv_checks_per_claim_permille as u128)).div_ceil(1_000).min(u64::MAX as u128) as u64
}

/// **The OPV route's capacity limit**: a new OPV claim of a model is admitted only while its open claims stay within the checks the
/// model's verifiers can perform this epoch (`checks_available`, measured), so a popular model is not finalized unchecked.
pub fn palw_opv_capacity_admits_v1(open_claims: u64, checks_available: u64, values: &PalwRfc0016ValuesV1) -> bool {
    palw_opv_check_budget_v1(open_claims.saturating_add(1), values) <= checks_available
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::TransactionOutpoint;

    fn h(i: u64) -> Hash64 {
        Hash64::from_u64_word(i)
    }

    fn bond(i: u32) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Default::default(), i))
    }

    fn sup(per_span: u128, classes: &[u64]) -> PalwVerifierSupplyV1 {
        PalwVerifierSupplyV1 { per_span, classes: classes.iter().map(|c| h(*c)).collect() }
    }

    fn vals() -> PalwRfc0016ValuesV1 {
        PalwRfc0016ValuesV1 {
            capacity_safety_permille: 1_000,
            queue_len_per_class: 4,
            queue_ttl_daa: 10,
            quote_margin_h: 1,
            verifier_share_base_permille: 150,
            verifier_slope_gamma_permille: 200,
            verifier_util_target_permille: 500,
            opv_checks_per_claim_permille: 500,
            ..Default::default()
        }
    }

    /// N4: a verifier ready for two models is counted once; disjoint sets do not subtract each other (T5: more verifiers, more capacity).
    #[test]
    fn a_verifier_is_counted_once_and_disjoint_sets_do_not_interfere() {
        let demand: BTreeMap<_, _> = [(h(1), 300u128), (h(2), 100u128)].into_iter().collect();
        let cap = palw_waterfill_capacity_v1(&[sup(100, &[1, 2])], &demand);
        assert_eq!(cap[&h(1)] + cap[&h(2)], 100, "one verifier's budget, once");
        let disjoint = palw_waterfill_capacity_v1(&[sup(100, &[1]), sup(100, &[2])], &demand);
        assert_eq!((disjoint[&h(1)], disjoint[&h(2)]), (100, 100), "no cross-model subtraction");
        let more = palw_waterfill_capacity_v1(&[sup(100, &[1]), sup(100, &[2]), sup(100, &[1])], &demand);
        assert_eq!(more[&h(1)], 200, "adding a verifier of model 1 raises model 1's capacity");
    }

    /// N3: capacity zero admits nothing and queues nothing.
    #[test]
    fn capacity_zero_admits_nothing() {
        assert!(!palw_capacity_fits_v1(0, 0));
        let mut q = PalwFinalQueueV1::default();
        let e = PalwFinalQueueEntryV1 { claim: h(1), bond: bond(1), queued_daa: 1, reserved_rights: 1 };
        assert_eq!(q.enqueue_v1(e, 0, 4, &vals()), Err(PalwFinalQueueRefusalV1::CapacityZero));
        assert_eq!(palw_c7_derived_cap_v1(0, 2_799, 10, &vals()), 0);
    }

    /// N5: a queued claim reserves at most one claim's rights; the queue is bounded and per-bond fair; TTL voids without early return.
    #[test]
    fn the_queue_is_bounded_fair_and_reserves_at_most_one_each() {
        let v = vals();
        let mut q = PalwFinalQueueV1::default();
        let e =
            |c: u64, b: u32, at: u64, r: u64| PalwFinalQueueEntryV1 { claim: h(c), bond: bond(b), queued_daa: at, reserved_rights: r };
        assert_eq!(q.enqueue_v1(e(1, 1, 1, 2), 10, 4, &v), Err(PalwFinalQueueRefusalV1::OverReservation));
        assert_eq!(q.enqueue_v1(e(1, 1, 1, 1), 10, 2, &v), Ok(0));
        assert_eq!(q.enqueue_v1(e(1, 1, 1, 1), 10, 2, &v), Err(PalwFinalQueueRefusalV1::Duplicate));
        assert_eq!(q.enqueue_v1(e(2, 1, 2, 1), 10, 2, &v), Ok(1));
        assert_eq!(q.enqueue_v1(e(3, 1, 3, 1), 10, 2, &v), Err(PalwFinalQueueRefusalV1::BondShareFull));
        assert_eq!(q.enqueue_v1(e(4, 2, 3, 1), 10, 2, &v), Ok(2));
        assert_eq!(q.enqueue_v1(e(5, 3, 3, 1), 10, 2, &v), Ok(3));
        assert_eq!(q.enqueue_v1(e(6, 4, 3, 1), 10, 2, &v), Err(PalwFinalQueueRefusalV1::QueueFull));
        assert_eq!(q.reserved_rights_v1(), 4);
        assert_eq!(q.expire_v1(12, &v), vec![h(1)], "queued at 1, TTL 10: void past 11");
        assert_eq!(q.dequeue_v1(1, 20), vec![(h(2), 20)], "the window starts when the claim leaves the queue");
    }

    #[test]
    fn the_c7_cap_is_derived_and_bounded_by_the_measured_ceiling() {
        let mut v = vals();
        assert_eq!(palw_c7_derived_cap_v1(10, 100, 100, &v), 10);
        assert_eq!(palw_c7_derived_cap_v1(1, 1, 1_000, &v), 1, "any capacity admits one");
        v.c7_measured_concurrency = 3;
        assert_eq!(palw_c7_derived_cap_v1(10, 100, 100, &v), 3);
    }

    /// N6: the quote refuses before the lottery and flags the margin.
    #[test]
    fn the_quote_refuses_before_the_lottery() {
        let v = vals();
        assert_eq!(palw_admission_quote_v1(None, Some((0, 1, 0)), 0, &v).verdict, PalwQuoteVerdictV1::Refuse("RFC0016_CAPACITY_ZERO"));
        assert_eq!(palw_admission_quote_v1(None, Some((10, 20, 1)), 0, &v).verdict, PalwQuoteVerdictV1::Queue);
        assert_eq!(palw_admission_quote_v1(None, Some((10, 20, 1)), 3, &v).verdict, PalwQuoteVerdictV1::Refuse("RFC0016_QUEUE_FULL"));
        assert_eq!(palw_admission_quote_v1(None, Some((10, 5, 1)), 0, &v).verdict, PalwQuoteVerdictV1::Admit);
    }

    #[test]
    fn the_verifier_share_moves_only_within_ten_to_thirty_percent() {
        let v = vals();
        assert_eq!(palw_verifier_share_permille_v1(500, &v), 150);
        assert_eq!(palw_verifier_share_permille_v1(1_000, &v), 250);
        assert_eq!(palw_verifier_share_permille_v1(10_000, &v), 300);
        assert_eq!(palw_verifier_share_permille_v1(0, &v), 100);
    }

    #[test]
    fn the_opv_check_budget_scales_with_claims() {
        let v = vals();
        assert_eq!(palw_opv_check_budget_v1(10, &v), 5);
        assert!(palw_opv_capacity_admits_v1(9, 5, &v));
        assert!(!palw_opv_capacity_admits_v1(10, 5, &v));
    }
}
