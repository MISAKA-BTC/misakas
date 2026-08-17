//! ADR-0033: the credit gate as a pure function of chain state — the B14 wiring's
//! decision core.
//!
//! [`decide_credit_v1`] answers, for ONE commitment whose `challenge_close_daa` a chain
//! block has just reached, the ADR-0028 §1 predicate verbatim:
//!
//! ```text
//! credit(C) ⟺ W_challenge(C) closed
//!           ∧ ≥1 assigned attestation with an independently recomputed root equal to C's
//!           ∧ no accepted refutation against C
//! ```
//!
//! Everything here is arithmetic over facts the CALLER assembled from its own chain view
//! (the virtual processor's walk backward from the crediting block's selected parent —
//! the same shape as `compute_audit_fee_outputs`): the observed commitment, the
//! attestations and refutations filed against its root, the anchor chain block, and the
//! bonded candidate set at that anchor. No store handle, no clock, no oracle enters, so
//! construction and validation of a crediting block compute byte-identical answers, and a
//! reorg that changes the assembled facts changes the answer identically on every node
//! (ADR-0033 §5).
//!
//! What "yes" is worth is decided HERE too (§4): `base(C)` is the block subsidy scaled by
//! the registered leverage remedy's fraction — the B15 size lever is not advisory, it is
//! the mint arithmetic — and each paid attester earns `ρ_v · base(C)` by the registered
//! per-mille ratio. The emergency rollback needs no special case: a zero-ceiling
//! registration or a remedy that fails the §4e inequality at the crediting subsidy makes
//! every decision non-creditable through [`PalwCreditParamsV1::active_for`] itself
//! (ADR-0033 §6).
//!
//! Consensus-inert until a network carries `Params::palw_credit = Some(..)`; every shipped
//! network carries `None`.

use crate::palw_registry::PalwClassRegistrationV1;
use crate::palw_schedule::{
    PalwEconomicFactsV1, PalwPanelCandidateV1, job_schedule_v1, max_leverage_holds_v1, select_replay_panel_v1,
};
use kaspa_hashes::Hash64;

/// The ADR-0033 fence: the one registered class this network credits, and the chain facts
/// the §4e inequality reads. Carried on `Params` as an `Option` — `None` (every shipped
/// network) is the dormant wiring; `Some` is a deliberate, test- or stage-gated activation.
///
/// Stage-2 single-class wiring on purpose: an on-chain registration flow is a later
/// increment, and ADR-0033's preconditions forbid activating any of this before the §12
/// gate items hold — the fence exists so the wiring can be exercised without inventing
/// that flow early.
#[derive(Clone, Debug, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwCreditParamsV1 {
    /// The registered class (validated shape; its `leverage_remedy`, `windows`,
    /// `credited_ceiling_tokens` and `rho_v_permille` are what the gate reads).
    pub registration: PalwClassRegistrationV1,
    /// `S_eff` — the slashable bond reachable by refutation (sompi).
    pub s_eff_sompi: u64,
    /// The validator unbonding period (blocks), for the §4e inequality.
    pub unbonding_period_blocks: u64,
    /// No commitment accepted before this DAA score is ever evaluated (the wiring's own
    /// activation edge; distinct from any PoW activation).
    pub activation_daa: u64,
}

impl PalwCreditParamsV1 {
    /// ADR-0033 §6: the gate reads `class_active ∧ ¬class_frozen` before anything else.
    /// A frozen class is a zero-ceiling registration here, and a remedy that does not bound
    /// the aggregate mint at this block's subsidy refuses activation through the same door
    /// (a `credited_ceiling` of zero and a failed inequality both make `credit(C) = 0` with
    /// no special case).
    ///
    /// ADR-0039 1a joins the same door: a class whose kernel catalog is open cannot be
    /// convicted — the adjudicator answers `Unadjudicable` for an uncatalogued kernel and
    /// `settle_dispute_v3` then slashes nobody — so slash-bearing credit against it would mint
    /// against nothing. `adjudication_depth` is checked FIRST, ahead of even the activation
    /// edge, because it is the one condition no later fact can compensate for.
    pub fn active_for(&self, commit_accepted_daa: u64, block_subsidy_sompi: u64) -> bool {
        if self.registration.adjudication_depth != crate::palw_registry::PalwAdjudicationDepthV1::ArithmeticCatalogued {
            return false;
        }
        if commit_accepted_daa < self.activation_daa || self.registration.credited_ceiling_tokens == 0 {
            return false;
        }
        let facts = PalwEconomicFactsV1 {
            block_subsidy_sompi,
            s_eff_sompi: self.s_eff_sompi,
            unbonding_period_blocks: self.unbonding_period_blocks,
        };
        max_leverage_holds_v1(&self.registration.leverage_remedy, &facts, self.one_job_ceiling_sompi(block_subsidy_sompi))
    }

    /// `base(C)` at a given block subsidy: the registered fraction, floored — the size
    /// lever of the B15 remedy IS the mint amount.
    pub fn base_sompi(&self, block_subsidy_sompi: u64) -> u64 {
        ((block_subsidy_sompi as u128) * (self.registration.leverage_remedy.base_subsidy_permille as u128) / 1000) as u64
    }

    /// One paid attester's share: `ρ_v · base(C)` by the registered per-mille ratio.
    pub fn attester_share_sompi(&self, block_subsidy_sompi: u64) -> u64 {
        ((self.base_sompi(block_subsidy_sompi) as u128) * (self.registration.rho_v_permille as u128) / 1000) as u64
    }

    /// ONE job's full payout: `base(C)` plus its `q` attester shares — the per-block crediting
    /// ceiling, and the unit ADR-0033 §4e reasons in.
    ///
    /// Two independent things must hold for the §4e bound to mean anything, and only one of
    /// them was true when this doc was first written:
    ///
    /// 1. **At most one job's worth per block.** `max_leverage_holds_v1` bounds the
    ///    pre-unbonding gain as `payout × (unbonding / min_credit_interval + 1)`, which assumes
    ///    one credited job per interval — so a consumer that mints more than one job's worth in
    ///    a block makes the inequality vacuous. Consumers apply this value as that ceiling.
    /// 2. **The same `payout` on both sides.** The inequality used to derive its own unit as
    ///    `base(C)` alone while this ceiling paid `base(C) + q · ρ_v · base(C)`, so the gate
    ///    licensed `1 + q · ρ_v / 1000` times the mint it had measured. Both now delegate to
    ///    [`crate::palw_registry::PalwClassRegistrationV1::one_job_payout_sompi`], so the
    ///    agreement is structural rather than a comment asking for it.
    pub fn one_job_ceiling_sompi(&self, block_subsidy_sompi: u64) -> u64 {
        self.registration.one_job_payout_sompi(block_subsidy_sompi)
    }
}

/// One commitment as the crediting walk observed it (kind 0x01, accepted-DAA-stamped).
#[derive(Clone, Debug)]
pub struct PalwObservedCommitmentV1 {
    pub committed_root: Hash64,
    /// The v2 logits root attestations must independently recompute: the composite
    /// binding's `full_logits_trace_root`, or `committed_root` itself for a bare-v2 class.
    pub logits_root: Hash64,
    pub executor_id: Hash64,
    pub runtime_class_id: Hash64,
    pub accepted_daa: u64,
}

/// An attester that earned a share, and the bond that gets paid for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPaidAttesterV1 {
    /// The panel member (a `validator_pubkey_hash`; NOT unique on its own).
    pub validator_id: Hash64,
    /// The bond that filed the earning attestation — the payee.
    pub bond_outpoint: crate::tx::TransactionOutpoint,
}

/// One attestation filed against that commitment's root, as observed.
#[derive(Clone, Debug)]
pub struct PalwObservedAttestationV1 {
    pub attester_id: Hash64,
    /// The bond that FILED this attestation — the payee, and the only unique identity here.
    ///
    /// `attester_id` is a `validator_pubkey_hash`, which `dns_finality` states is not unique, so it
    /// cannot resolve a payout: with two bonds under one key the reward went to whichever the
    /// walk reached first (audit B5). The carriage has carried this outpoint all along; the
    /// consumer dropped it.
    pub bond_outpoint: crate::tx::TransactionOutpoint,
    pub attested_logits_root: Hash64,
    pub accepted_daa: u64,
}

/// The gate's answer for one commitment at its crediting block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCreditDecisionV1 {
    /// The §1 predicate.
    pub creditable: bool,
    /// `base(C)` (sompi) — meaningful only when `creditable`.
    pub base_sompi: u64,
    /// Each paid attester's share (sompi).
    pub attester_share_sompi: u64,
    /// The on-time, root-matched panel members, deduplicated, in panel order — the §4a
    /// `q · ρ_v · base(C)` recipients.
    ///
    /// Each carries the bond that filed the earning attestation: the consumer pays
    /// `bond_outpoint`, never a lookup by `validator_id` (audit B5).
    pub paid_attesters: Vec<PalwPaidAttesterV1>,
}

impl PalwCreditDecisionV1 {
    fn nothing() -> Self {
        Self { creditable: false, base_sompi: 0, attester_share_sompi: 0, paid_attesters: Vec::new() }
    }
}

/// The ADR-0028 §1 predicate for one commitment, evaluated at its crediting block.
///
/// The caller assembled every input from its own chain view; this function only decides.
/// `anchor` is the first chain block at or past `accepted_daa + Δ_bind` on the caller's
/// chain (ADR-0028 §2 keeps it settled), `candidates` the bonded set the caller derived at
/// that anchor, `refutation_accepted_daas` the accepted-DAA stamps of every accepted
/// refutation against this root — a refutation accepted AFTER the window still convicts,
/// but does not revoke credit (the deliberate §3 asymmetry; the ledger counts that tail
/// separately).
pub fn decide_credit_v1(
    params: &PalwCreditParamsV1,
    commitment: &PalwObservedCommitmentV1,
    anchor: &Hash64,
    candidates: &[PalwPanelCandidateV1],
    attestations: &[PalwObservedAttestationV1],
    refutation_accepted_daas: &[u64],
    block_subsidy_sompi: u64,
) -> PalwCreditDecisionV1 {
    // §6 first: class active, not frozen, remedy bounding — then the class match itself.
    if !params.active_for(commitment.accepted_daa, block_subsidy_sompi)
        || commitment.runtime_class_id != params.registration.runtime_class_id
    {
        return PalwCreditDecisionV1::nothing();
    }
    let Ok(schedule) = job_schedule_v1(&params.registration.windows, commitment.accepted_daa) else {
        return PalwCreditDecisionV1::nothing();
    };
    // A refutation accepted anywhere inside the window voids credit regardless of
    // attestation count (§3 of the predicate).
    if refutation_accepted_daas.iter().any(|&daa| daa <= schedule.challenge_close_daa) {
        return PalwCreditDecisionV1::nothing();
    }
    // The panel is derived, never stored — the rule cannot drift from its output (§2).
    let panel = select_replay_panel_v1(
        &commitment.committed_root,
        &commitment.executor_id,
        anchor,
        &commitment.runtime_class_id,
        candidates,
        params.registration.windows.q as usize,
    );
    // ≥1 assigned attestation, on time, with the independently recomputed root equal to
    // C's. Panel order (not arrival order) fixes the payout order; one share per assignee.
    let mut paid = Vec::new();
    for member in &panel {
        // The EARNING attestation, not merely the fact that one exists: its bond outpoint is the
        // payee. Ties inside one panel member (two bonds under one validator key both filing) are
        // broken by the outpoint so the choice is a function of the records, never of walk order.
        let mut earning: Vec<&PalwObservedAttestationV1> = attestations
            .iter()
            .filter(|a| {
                a.attester_id == *member
                    && a.attested_logits_root == commitment.logits_root
                    && a.accepted_daa <= schedule.replay_deadline_daa
            })
            .collect();
        earning.sort_by_key(|a| (a.bond_outpoint.transaction_id, a.bond_outpoint.index));
        if let Some(a) = earning.first() {
            paid.push(PalwPaidAttesterV1 { validator_id: *member, bond_outpoint: a.bond_outpoint });
        }
    }
    if paid.is_empty() {
        // Zero attestations ⇒ credit 0; the panel is never shrunk to make a job creditable.
        return PalwCreditDecisionV1::nothing();
    }
    PalwCreditDecisionV1 {
        creditable: true,
        base_sompi: params.base_sompi(block_subsidy_sompi),
        attester_share_sompi: params.attester_share_sompi(block_subsidy_sompi),
        paid_attesters: paid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_schedule::PalwLeverageRemedyV1;

    fn h64(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    /// The registry's BASE-0 registration (fractional remedy, two-minute windows), wired into a
    /// fence with the live bond and unbonding facts.
    ///
    /// BASE-0 and not the float fleet class: ADR-0039 1a makes `active_for` decline any class
    /// whose catalog is open, and the float class is honestly structural-only, so a fence over it
    /// credits nothing. Every measured number is the same — the registration is the fleet row
    /// with BASE-0 kernel ids and tag.
    fn fence() -> PalwCreditParamsV1 {
        PalwCreditParamsV1 {
            registration: crate::palw_registry::tests::base0_registration(),
            s_eff_sompi: 20_000 * 100_000_000,
            unbonding_period_blocks: 10_083,
            activation_daa: 0,
        }
    }

    const SUBSIDY: u64 = 370_468_345 * 1_200; // the 120 s net's rate-preserved subsidy

    fn candidate(id: Hash64, class: Hash64) -> PalwPanelCandidateV1 {
        PalwPanelCandidateV1 { validator_id: id, runtime_class_id: class, bonded: true, frozen: false }
    }

    /// Executor 0xE0 plus four bonded same-class candidates; q = 2 draws two of them.
    fn scene() -> (PalwCreditParamsV1, PalwObservedCommitmentV1, Hash64, Vec<PalwPanelCandidateV1>, Vec<Hash64>) {
        let params = fence();
        let class = params.registration.runtime_class_id;
        let commitment = PalwObservedCommitmentV1 {
            committed_root: h64(0xC0),
            logits_root: h64(0x1A),
            executor_id: h64(0xE0),
            runtime_class_id: class,
            accepted_daa: 1_000,
        };
        let anchor = h64(0xAA);
        let candidates: Vec<_> =
            [h64(0xE0), h64(0x01), h64(0x02), h64(0x03), h64(0x04)].iter().map(|id| candidate(*id, class)).collect();
        let panel = select_replay_panel_v1(
            &commitment.committed_root,
            &commitment.executor_id,
            &anchor,
            &class,
            &candidates,
            params.registration.windows.q as usize,
        );
        assert_eq!(panel.len(), 2, "q=2 out of four non-executor candidates");
        (params, commitment, anchor, candidates, panel)
    }

    /// A bond outpoint for a validator id — one per attester, so a payee is identifiable.
    fn bond_of(attester: Hash64) -> crate::tx::TransactionOutpoint {
        crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes(attester.as_bytes()), index: 0 }
    }

    fn on_time(attester: Hash64, logits: Hash64) -> PalwObservedAttestationV1 {
        // anchor = 1 000 + Δ_bind(10); replay deadline = anchor + w_replay(30) = 1 040.
        PalwObservedAttestationV1 {
            attester_id: attester,
            bond_outpoint: bond_of(attester),
            attested_logits_root: logits,
            accepted_daa: 1_035,
        }
    }

    /// The paid ids, for assertions that are about panel ORDER rather than about the payee.
    fn paid_ids(d: &PalwCreditDecisionV1) -> Vec<Hash64> {
        d.paid_attesters.iter().map(|a| a.validator_id).collect()
    }

    #[test]
    fn the_predicate_credits_and_pays_panel_members_in_panel_order() {
        let (params, commitment, anchor, candidates, panel) = scene();
        let attestations = vec![on_time(panel[1], commitment.logits_root), on_time(panel[0], commitment.logits_root)];
        let d = decide_credit_v1(&params, &commitment, &anchor, &candidates, &attestations, &[], SUBSIDY);
        assert!(d.creditable);
        assert_eq!(paid_ids(&d), panel, "payout order is panel order, not arrival order");
        // base = subsidy · 1‰ (floored); share = ρ_v(1.0) · base.
        assert_eq!(d.base_sompi, SUBSIDY / 1000);
        assert_eq!(d.attester_share_sompi, d.base_sompi);
        // And the ceiling that bounds a block's whole credit is the unit §4e was checked
        // against: base + q · share, with q = 2. The two arithmetics agreeing is the point.
        assert_eq!(params.one_job_ceiling_sompi(SUBSIDY), d.base_sompi * 3);
        assert_eq!(params.one_job_ceiling_sompi(SUBSIDY), params.registration.one_job_payout_sompi(SUBSIDY));
    }

    #[test]
    fn one_on_time_match_is_enough_and_only_that_member_is_paid() {
        let (params, commitment, anchor, candidates, panel) = scene();
        let attestations = vec![on_time(panel[0], commitment.logits_root)];
        let d = decide_credit_v1(&params, &commitment, &anchor, &candidates, &attestations, &[], SUBSIDY);
        assert!(d.creditable);
        assert_eq!(paid_ids(&d), vec![panel[0]]);
    }

    #[test]
    fn zero_attestations_mean_zero_credit_never_a_shrunk_panel() {
        let (params, commitment, anchor, candidates, _) = scene();
        let d = decide_credit_v1(&params, &commitment, &anchor, &candidates, &[], &[], SUBSIDY);
        assert_eq!(d, PalwCreditDecisionV1::nothing());
    }

    /// **Audit B3/B4: one job's payout is the per-block ceiling ADR-0033 §4e reasons in.**
    ///
    /// `max_leverage_holds_v1` bounds the pre-unbonding gain as `base(C) × jobs` where
    /// `jobs = unbonding / min_credit_interval + 1` — one credited job per interval. A consumer
    /// minting more than one job's worth per block makes that inequality vacuous, so the ceiling
    /// has to be exactly one job: base plus its `q` shares.
    #[test]
    fn the_one_job_ceiling_is_base_plus_q_shares() {
        let (params, ..) = scene();
        let base = params.base_sompi(SUBSIDY);
        let share = params.attester_share_sompi(SUBSIDY);
        let q = params.registration.windows.q as u64;
        assert!(base > 0 && share > 0 && q > 0, "the fixture must exercise real values");
        assert_eq!(params.one_job_ceiling_sompi(SUBSIDY), base + share * q);

        // It is strictly more than one payout and strictly less than two jobs' worth, so it bounds
        // a block to a single job without ever truncating that job's own attesters.
        assert!(params.one_job_ceiling_sompi(SUBSIDY) > base, "a job's attesters must fit under it");
        assert!(params.one_job_ceiling_sompi(SUBSIDY) < 2 * (base + share * q), "two jobs must not fit");

        // Saturating, not wrapping: an absurd subsidy cannot produce a small ceiling.
        assert!(params.one_job_ceiling_sompi(u64::MAX) >= params.one_job_ceiling_sompi(SUBSIDY));
    }

    /// **Audit B5: the payee is the bond that FILED, not a lookup by validator key.**
    ///
    /// `attester_id` is a `validator_pubkey_hash` and is not unique, so two bonds under one key
    /// both look like the same panel member. The decision must name the outpoint that earned the
    /// share, and it must pick it from the records rather than from arrival order — otherwise the
    /// consumer's `bonds.iter().find(|b| b.validator_pubkey_hash == id)` paid whichever bond the
    /// walk reached first.
    #[test]
    fn the_paid_attester_names_the_bond_that_filed() {
        let (params, commitment, anchor, candidates, panel) = scene();

        // One attester, one bond: the payee is that bond.
        let single = vec![on_time(panel[0], commitment.logits_root)];
        let d = decide_credit_v1(&params, &commitment, &anchor, &candidates, &single, &[], SUBSIDY);
        assert_eq!(d.paid_attesters.len(), 1);
        assert_eq!(d.paid_attesters[0].validator_id, panel[0]);
        assert_eq!(d.paid_attesters[0].bond_outpoint, bond_of(panel[0]), "the payee is the filing bond");

        // TWO bonds under ONE validator key both file. The panel member earns ONE share, and the
        // payee is chosen by outpoint order — a function of the records, not of arrival order.
        let low = crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes([0x01; 64]), index: 0 };
        let high = crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes([0xFE; 64]), index: 0 };
        let mut first = on_time(panel[0], commitment.logits_root);
        first.bond_outpoint = high;
        let mut second = on_time(panel[0], commitment.logits_root);
        second.bond_outpoint = low;

        let forward = decide_credit_v1(&params, &commitment, &anchor, &candidates, &[first.clone(), second.clone()], &[], SUBSIDY);
        let backward = decide_credit_v1(&params, &commitment, &anchor, &candidates, &[second, first], &[], SUBSIDY);
        assert_eq!(forward.paid_attesters.len(), 1, "one panel member earns one share");
        assert_eq!(forward.paid_attesters[0].bond_outpoint, low, "the lower outpoint wins, deterministically");
        assert_eq!(forward, backward, "the payee must not depend on the order the walk collected them");
    }

    #[test]
    fn late_wrong_root_off_panel_and_duplicate_attestations_do_not_pay() {
        let (params, commitment, anchor, candidates, panel) = scene();
        let late = PalwObservedAttestationV1 {
            attester_id: panel[0],
            bond_outpoint: bond_of(panel[0]),
            attested_logits_root: commitment.logits_root,
            accepted_daa: 1_041, // one past the replay deadline
        };
        let wrong_root = on_time(panel[1], h64(0xBB));
        let off_panel = on_time(h64(0xE0), commitment.logits_root); // the executor is never on its own panel
        let d = decide_credit_v1(&params, &commitment, &anchor, &candidates, &[late, wrong_root, off_panel], &[], SUBSIDY);
        assert_eq!(d, PalwCreditDecisionV1::nothing(), "late, non-matching and unassigned attestations earn nothing");

        // A duplicate on-time match pays its assignee once.
        let dup = vec![on_time(panel[0], commitment.logits_root), on_time(panel[0], commitment.logits_root)];
        let d = decide_credit_v1(&params, &commitment, &anchor, &candidates, &dup, &[], SUBSIDY);
        assert_eq!(paid_ids(&d), vec![panel[0]]);
    }

    #[test]
    fn a_refutation_inside_the_window_voids_credit_and_one_after_does_not() {
        let (params, commitment, anchor, candidates, panel) = scene();
        let attestations = vec![on_time(panel[0], commitment.logits_root)];
        // challenge_close = 1 000 + 720 = 1 720.
        let inside = decide_credit_v1(&params, &commitment, &anchor, &candidates, &attestations, &[1_720], SUBSIDY);
        assert_eq!(inside, PalwCreditDecisionV1::nothing(), "an accepted refutation inside the window voids credit");
        let after = decide_credit_v1(&params, &commitment, &anchor, &candidates, &attestations, &[1_721], SUBSIDY);
        assert!(after.creditable, "conviction after the window is slash material, never a credit revocation (§3 asymmetry)");
    }

    #[test]
    fn a_foreign_class_a_zero_ceiling_and_a_failed_inequality_all_credit_nothing() {
        let (params, commitment, anchor, candidates, panel) = scene();
        let attestations = vec![on_time(panel[0], commitment.logits_root)];

        let mut foreign = commitment.clone();
        foreign.runtime_class_id = h64(0x77);
        let d = decide_credit_v1(&params, &foreign, &anchor, &candidates, &attestations, &[], SUBSIDY);
        assert_eq!(d, PalwCreditDecisionV1::nothing());

        let mut frozen = params.clone();
        frozen.registration = frozen.registration.to_zero_credit();
        let d = decide_credit_v1(&frozen, &commitment, &anchor, &candidates, &attestations, &[], SUBSIDY);
        assert_eq!(d, PalwCreditDecisionV1::nothing(), "the rollback is inside the gate, not beside it");

        let mut unbounded = params.clone();
        unbounded.registration.leverage_remedy = PalwLeverageRemedyV1 { min_credit_interval_daa: 1, base_subsidy_permille: 1_000 };
        let d = decide_credit_v1(&unbounded, &commitment, &anchor, &candidates, &attestations, &[], SUBSIDY);
        assert_eq!(d, PalwCreditDecisionV1::nothing(), "an unbounded remedy refuses activation through the same door");

        // ADR-0039 1a through the same door: a class whose catalog is open credits nothing.
        // The gate is on the DEPTH, so this is the float fleet class's actual behaviour — the
        // only change needed to reach it is the honest one.
        let mut open_catalog = params.clone();
        open_catalog.registration.adjudication_depth = crate::palw_registry::PalwAdjudicationDepthV1::StructuralOnly;
        assert!(!open_catalog.active_for(commitment.accepted_daa, SUBSIDY));
        let d = decide_credit_v1(&open_catalog, &commitment, &anchor, &candidates, &attestations, &[], SUBSIDY);
        assert_eq!(d, PalwCreditDecisionV1::nothing(), "a class the court cannot convict earns no slash-bearing credit");
        // And it is not a fixture artifact: the shipped float class IS that class.
        let mut float_class = params.clone();
        float_class.registration = crate::palw_registry::tests::fleet_registration();
        assert!(!float_class.active_for(commitment.accepted_daa, SUBSIDY));

        let mut before_activation = params.clone();
        before_activation.activation_daa = 5_000;
        let d = decide_credit_v1(&before_activation, &commitment, &anchor, &candidates, &attestations, &[], SUBSIDY);
        assert_eq!(d, PalwCreditDecisionV1::nothing());
    }
}
