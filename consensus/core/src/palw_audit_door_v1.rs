//! **ADR-0160 lane Q — the audit door** (testnet-12 only, behind the dormant
//! `Params::palw_capacity_audit_door`, F-Q; rcore/cap-s1 stage 2; ADR-0160 v3 §5.9, decision D-23).
//!
//! **The rule.** A **credited** claim — one whose escrow slot a ramp step's credit lowered below its
//! reward (`m_c < E`: the claim commits less than the reward it would be paid, so fraud on it would pay
//! unless something refuses it before payment) — reaches `Final` only once it holds audit receipts from
//! `k_aud` DISTINCT members of its audit pool: the operator bonds (lane A's list, testnet-12's eight
//! genesis cards) that are neither its producer nor a seat of its panel nor frozen nor excluded.
//! `k_aud` is 1 below ρ 250 and 2 from ρ 250 ([`palw_capacity_k_aud_v1`]). An unaudited credited claim
//! stays licensed (its weight J-1-capped) and **waits**: it is never voided, burned or charged for lack
//! of an audit (§5.9 (d)); its `Final` deadline is `max(L + short_challenge, audit_daa)`.
//!
//! **Detection is an on-chain observable** (the user's stage-2 gate). An audit is a receipt the
//! auditor signs and a block carries: `PalwConsensusObjectV2::AuditReceiptBatchV1 { auditor, entries:
//! [(claim, reproduced_root)], signature }`. The fold REFUSES the object (the block is invalid) when the
//! auditor is not an operator bond or is excluded, or when an entry's `reproduced_root` is not the
//! claim's committed execution root — a receipt can only say "this reproduces"; a pool member whose
//! replay does not reproduce posts nothing and files (a DA accusation, a refutation), and the claim is
//! convicted through the ordinary routes. The acceptance layer checks the signature against the
//! auditor's genesis-registered key (the key lane A reads). The fold SKIPS an entry (the block stands)
//! whose claim is gone, terminal, not credited, not licensed, already receipted by this auditor, or
//! whose pool does not hold the auditor. A kept entry appends `(auditor, daa)` to the rooted map
//! `audit_receipts` (hashed only when non-empty; an entry leaves with its claim's retirement).
//!
//! **Accountability** (§5.9 (g)): a conviction of a claim that holds any receipt inserts every
//! receipting auditor into the rooted set `excluded_auditors` (only a flag day removes one), so a pool
//! member that passed a fraud never audits again.
//!
//! **The backlog** (§5.9 (f)): admission of a credited claim is refused `AuditBacklogFull` while the
//! credited, licensed, non-terminal, unaudited claims number at least [`palw_capacity_audit_backlog_max_v1`]
//! — a queue bound, never the safety (an unaudited claim is never paid).

use std::collections::BTreeMap;

use crate::Hash64;
use crate::palw_state_v2::{PalwBondKeyV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwClaimStateV2, PalwStateParamsV2};

/// The domain of the pool's rank and of the receipt batch's signed message.
pub const PALW_AUDIT_DOOR_DOMAIN_V1: &[u8] = b"misaka-palw/capacity/audit-door/v1";

/// **ρ from which a credited claim needs two receipts** (§5.9 (c)): one compromised host is not enough
/// at the steps where one claim's credit is worth the most (T4).
pub const PALW_CAPACITY_AUDIT_K2_RHO_V1: u32 = 250;

/// **`W_aud`** — the audit latency the breaker's K6 measures against (§5.9 (g)); the node duty aims to
/// receipt within it.
pub const PALW_CAPACITY_AUDIT_W_AUD_DAA_V1: u64 = 60;

/// **`A_max`** — the most credited licensed claims that may wait for their audit before a new credited
/// claim is refused (§5.9 (f)). A release constant: ADR-0160 sets it from Stage 0's measured operator
/// audit rate × `W_aud` × ½; until that measurement it is the floor's ρ-100 issue rate over `W_aud` for
/// the eight genesis bonds (10 claims a DAA a 13k-equivalent unit is far above what they issue), 4,800.
/// A change is a new fence.
pub const PALW_CAPACITY_AUDIT_BACKLOG_MAX_V1: u64 = 4_800;

/// Most entries one receipt batch carries (a block-mass bound; the node batches per DAA).
pub const PALW_AUDIT_RECEIPT_BATCH_MAX_ENTRIES_V1: usize = 256;

/// **`k_aud(ρ)`**: 1 below ρ 250, 2 from ρ 250.
pub fn palw_capacity_k_aud_v1(rho: u32) -> usize {
    if rho >= PALW_CAPACITY_AUDIT_K2_RHO_V1 { 2 } else { 1 }
}

/// `A_max` for a class (one value for every attributable class until the stage-0 measurement).
pub fn palw_capacity_audit_backlog_max_v1(_class_id: &Hash64) -> u64 {
    PALW_CAPACITY_AUDIT_BACKLOG_MAX_V1
}

/// **One entry of a receipt batch**: the claim and the root the auditor's replay reproduced.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwAuditEntryV1 {
    pub claim_id: Hash64,
    pub reproduced_root: Hash64,
}

/// **A claim's audit** (rooted in `PalwChainStateV2::audit_receipts`): each distinct auditor that
/// receipted it, with the DAA of the block that carried its receipt, in bond order.
#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwAuditStatusV1 {
    pub receipts: Vec<(PalwBondKeyV2, u64)>,
}

impl PalwAuditStatusV1 {
    /// Whether `auditor` already receipted the claim.
    pub fn has_v1(&self, auditor: &PalwBondKeyV2) -> bool {
        self.receipts.iter().any(|(bond, _)| bond == auditor)
    }

    /// The status with `auditor`'s receipt at `daa` added (kept in bond order).
    pub fn with_v1(&self, auditor: PalwBondKeyV2, daa: u64) -> Self {
        let mut receipts = self.receipts.clone();
        if !self.has_v1(&auditor) {
            receipts.push((auditor, daa));
            receipts.sort_unstable();
        }
        Self { receipts }
    }

    /// **The DAA at which the claim became audited under `k_aud`**: the `k_aud`-th earliest receipt's
    /// DAA, or `None` while fewer than `k_aud` distinct auditors receipted it.
    pub fn audited_at_v1(&self, k_aud: usize) -> Option<u64> {
        if k_aud == 0 {
            return Some(0);
        }
        let mut days: Vec<u64> = self.receipts.iter().map(|(_, daa)| *daa).collect();
        days.sort_unstable();
        days.get(k_aud - 1).copied()
    }
}

/// **Is `claim` credited?** (§5.3 as stage 2 reads it.) An ATTEMPT claim accepted at or past F-Q, with
/// a reward, of an attributable class (not on C7's list), whose escrow slot the step at its
/// `accepted_daa` credits — the escrow lane's own gate (`palw_escrow_credit_applies_v1`: the step's
/// credit reaches `q_seat`), so the audit door gates exactly the claims whose commitment was cut below
/// their reward. A pure function of the params and the claim record, so the fold, the deadline index
/// at rest and the load re-derivation read one answer (the price record of v3 D.2 is lane B's, when ρ
/// stops being a pure function of the params).
pub fn palw_capacity_claim_credited_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> bool {
    matches!(claim.source, PalwClaimSourceV2::Attempt)
        && palw_capacity_credited_at_v1(params, claim.accepted_daa, &claim.class_id, claim.escrowed_reward)
}

/// [`palw_capacity_claim_credited_v1`] on a claim's acceptance facts — the escrow term's reading, which
/// holds the DAA, the class and the reward but not the record (every claim it prices with a reward is an
/// attempt: a free prompt escrows none).
pub fn palw_capacity_credited_at_v1(params: &PalwStateParamsV2, accepted_daa: u64, class_id: &Hash64, escrowed_reward: u64) -> bool {
    params.capacity_audit_active_at(accepted_daa)
        && escrowed_reward > 0
        && crate::palw_escrow_funding_v2::palw_claim_class_attributable_v1(params, class_id)
        && params
            .capacity_escrow_credit_at_v1(accepted_daa)
            .is_some_and(|credit| crate::palw_escrow_funding_v2::palw_escrow_credit_applies_v1(&credit))
}

/// **The ramp step that prices `claim`'s seats** (v3 AS-1′, stage 2): its step where the claim is
/// credited (the duty and the lock divided by ρ, AS-1/AS-2), `None` otherwise — an uncredited claim, a C7
/// claim and a free prompt are priced exactly as today whatever the step.
pub fn palw_capacity_seat_step_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> Option<crate::palw_aggregate_liability_v1::PalwCapacityStepV1> {
    if palw_capacity_claim_credited_v1(params, claim) { params.capacity_step_at(claim.accepted_daa) } else { None }
}

/// **`k_aud` of a credited claim** — at the ρ of its own `accepted_daa` (the step's).
pub fn palw_capacity_claim_k_aud_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> usize {
    palw_capacity_k_aud_v1(crate::palw_weight_cap_v1::palw_capacity_rho_at_v1(params, claim.accepted_daa))
}

/// **Where the audit door holds a credited claim's `Final`**: `Some(None)` — credited and not yet
/// audited (the claim waits, no deadline); `Some(Some(audit_daa))` — credited and audited at
/// `audit_daa` (its deadline is at least that); `None` — not credited (today's rule).
pub fn palw_capacity_audit_gate_v1(
    params: &PalwStateParamsV2,
    claim: &PalwClaimStateV2,
    status: Option<&PalwAuditStatusV1>,
) -> Option<Option<u64>> {
    if !palw_capacity_claim_credited_v1(params, claim) {
        return None;
    }
    Some(status.and_then(|s| s.audited_at_v1(palw_capacity_claim_k_aud_v1(params, claim))))
}

/// **Whether a credited claim counts against the backlog** (§5.9 (f)): licensed (any licence), not
/// terminal, not yet audited. (`DefaultDisputed` — an open accusation — is out: its session decides.)
pub fn palw_capacity_awaits_audit_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2, status: Option<&PalwAuditStatusV1>) -> bool {
    matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. })
        && matches!(palw_capacity_audit_gate_v1(params, claim, status), Some(None))
}

/// **The pool's rank** — `H(domain ‖ claim ‖ bond)` (lifted from the node's `palw_operator_da_order_v1`
/// shape), the turn order the node duty reads; the fold uses the pool as a set.
pub fn palw_audit_rank_v1(claim_id: &Hash64, bond: &PalwBondKeyV2) -> Hash64 {
    let mut hasher = blake2b_simd::Params::new().hash_length(64).key(PALW_AUDIT_DOOR_DOMAIN_V1).to_state();
    hasher.update(b"rank");
    hasher.update(claim_id.as_bytes().as_slice());
    hasher.update(&borsh::to_vec(bond).expect("a bond key encodes"));
    Hash64::from_bytes(hasher.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// The ML-DSA-87 context an auditor signs its batch under (the acceptance layer's check).
pub const PALW_AUDIT_RECEIPT_V1_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/audit-receipt/mldsa87/v1";

/// **The message an auditor signs** for a batch: `H(domain ‖ "batch" ‖ network ‖ auditor ‖ entries)` —
/// the network domain binds it to one chain (a receipt is never replayable across networks sharing a
/// card, the premine-replay rule).
pub fn palw_audit_receipt_batch_message_v1(network_domain: Hash64, auditor: &PalwBondKeyV2, entries: &[PalwAuditEntryV1]) -> Hash64 {
    let mut hasher = blake2b_simd::Params::new().hash_length(64).key(PALW_AUDIT_DOOR_DOMAIN_V1).to_state();
    hasher.update(b"batch");
    hasher.update(network_domain.as_bytes().as_slice());
    hasher.update(&borsh::to_vec(auditor).expect("a bond key encodes"));
    hasher.update(&borsh::to_vec(entries).expect("entries encode"));
    Hash64::from_bytes(hasher.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// **The audit pool of a claim, ranked** (§5.9 (a)): the operator bonds that are neither its producer
/// nor a seat of `seats`, nor frozen (`frozen`), nor excluded, ordered by [`palw_audit_rank_v1`].
pub fn palw_audit_pool_v1<'a>(
    operators: impl Iterator<Item = &'a PalwBondKeyV2>,
    claim_id: &Hash64,
    producer: &PalwBondKeyV2,
    seats: &[PalwBondKeyV2],
    frozen: impl Fn(&PalwBondKeyV2) -> bool,
    excluded: impl Fn(&PalwBondKeyV2) -> bool,
) -> Vec<PalwBondKeyV2> {
    let mut ranked: BTreeMap<Hash64, PalwBondKeyV2> = BTreeMap::new();
    for bond in operators {
        if bond == producer || seats.contains(bond) || frozen(bond) || excluded(bond) {
            continue;
        }
        ranked.insert(palw_audit_rank_v1(claim_id, bond), *bond);
    }
    ranked.into_values().collect()
}

/// **The audit pool of `claim_id` on `state`** — [`palw_audit_pool_v1`] over the params' operator
/// bonds, the claim's producer, its current panel's seats, the freezes and the exclusions.
pub fn palw_audit_pool_of_claim_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &PalwStateParamsV2,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
) -> Vec<PalwBondKeyV2> {
    let seats: Vec<PalwBondKeyV2> = state.panel(claim_id).map(|panel| panel.seats.iter().map(|seat| seat.bond).collect()).unwrap_or_default();
    palw_audit_pool_v1(
        params.capacity_audit_operators().iter(),
        claim_id,
        &claim.bond,
        &seats,
        |bond| crate::palw_aggregate_liability_v1::palw_bond_is_frozen_v1(state, bond),
        |bond| state.auditor_excluded_v1(bond),
    )
}

/// **`credited_unaudited`** (§5.9 (f)) — the credited licensed claims on `state` still awaiting their
/// audit. Counted on demand (only an admission of a CREDITED claim past F-Q asks it).
pub fn palw_capacity_audit_backlog_v1(state: &crate::palw_state_v2::PalwChainStateV2, params: &PalwStateParamsV2) -> u64 {
    state.claims_iter().filter(|(id, claim)| palw_capacity_awaits_audit_v1(params, claim, state.audit_status_of_v1(id))).count() as u64
}

/// **What makes a receipt batch INVALID** (the fold refuses the block; the acceptance layer drops the
/// object first): the auditor is not an operator bond or is excluded; the batch is empty, longer than
/// [`PALW_AUDIT_RECEIPT_BATCH_MAX_ENTRIES_V1`], or not in strictly increasing claim order; or an entry
/// names a held claim whose committed execution root is not the entry's `reproduced_root`. Everything
/// else about an entry (a claim gone, terminal, uncredited, unlicensed, already receipted by this
/// auditor, or whose pool lacks it) is a skip, not a refusal.
pub fn palw_audit_receipt_batch_refusal_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &PalwStateParamsV2,
    auditor: &PalwBondKeyV2,
    entries: &[PalwAuditEntryV1],
) -> Option<&'static str> {
    if !params.capacity_audit_operators().contains(auditor) {
        return Some("the auditor is not an operator bond");
    }
    if state.auditor_excluded_v1(auditor) {
        return Some("the auditor is excluded (a claim it receipted was convicted)");
    }
    if entries.is_empty() || entries.len() > PALW_AUDIT_RECEIPT_BATCH_MAX_ENTRIES_V1 {
        return Some("a batch holds between one and 256 entries");
    }
    if !entries.windows(2).all(|pair| pair[0].claim_id < pair[1].claim_id) {
        return Some("a batch's entries are in strictly increasing claim order");
    }
    for entry in entries {
        if let Some(claim) = state.claim(&entry.claim_id)
            && claim.execution_root != entry.reproduced_root
        {
            return Some("an entry's reproduced root is not its claim's committed execution root");
        }
    }
    None
}

/// **Whether a batch entry is KEPT** (appends the auditor's receipt) on `state`: the claim is held, not
/// terminal, credited, licensed, not yet receipted by this auditor, and its pool holds the auditor.
pub fn palw_audit_entry_kept_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &PalwStateParamsV2,
    auditor: &PalwBondKeyV2,
    entry: &PalwAuditEntryV1,
) -> bool {
    let Some(claim) = state.claim(&entry.claim_id) else { return false };
    matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. })
        && palw_capacity_claim_credited_v1(params, claim)
        && !state.audit_status_of_v1(&entry.claim_id).is_some_and(|status| status.has_v1(auditor))
        && palw_audit_pool_of_claim_v1(state, params, &entry.claim_id, claim).contains(auditor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bond(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_bytes([n; 64]), 0))
    }

    #[test]
    fn k_aud_is_one_below_rho_250_and_two_from_it() {
        assert_eq!([1, 10, 100, 249].map(palw_capacity_k_aud_v1), [1, 1, 1, 1]);
        assert_eq!([250, 500, 1_000].map(palw_capacity_k_aud_v1), [2, 2, 2]);
    }

    #[test]
    fn a_claim_is_audited_at_its_k_th_distinct_receipt() {
        let s = PalwAuditStatusV1::default();
        assert_eq!(s.audited_at_v1(1), None);
        let s = s.with_v1(bond(3), 110);
        assert_eq!(s.audited_at_v1(1), Some(110));
        assert_eq!(s.audited_at_v1(2), None, "one auditor is not two");
        let again = s.with_v1(bond(3), 120);
        assert_eq!(again, s, "the same auditor counts once");
        let s = s.with_v1(bond(1), 130);
        assert_eq!(s.audited_at_v1(2), Some(130), "the second distinct receipt");
        assert_eq!(s.receipts, vec![(bond(1), 130), (bond(3), 110)], "kept in bond order");
    }

    #[test]
    fn the_pool_leaves_out_the_producer_the_seats_the_frozen_and_the_excluded() {
        let operators: Vec<PalwBondKeyV2> = (1..=8).map(bond).collect();
        let claim = Hash64::from_bytes([7; 64]);
        let seats = [bond(1), bond(2), bond(3), bond(4), bond(5)];
        let pool = palw_audit_pool_v1(operators.iter(), &claim, &bond(6), &seats, |b| *b == bond(9), |b| *b == bond(8));
        assert_eq!(pool.len(), 1, "8 cards − producer − 5 seats − 1 excluded");
        assert_eq!(pool, vec![bond(7)]);
        let open = palw_audit_pool_v1(operators.iter(), &claim, &bond(9), &seats, |_| false, |_| false);
        assert_eq!(open.len(), 3, "a non-card producer: the three cards off the panel (t12's |pool| = 3)");
        let mut sorted = open.clone();
        sorted.sort_by_key(|b| palw_audit_rank_v1(&claim, b));
        assert_eq!(open, sorted, "ranked by H(domain ‖ claim ‖ bond)");
    }
}
