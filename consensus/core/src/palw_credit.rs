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

use crate::palw_job_panel::{PalwPanelCandidateV3, select_job_panel_at_anchor_v3};
use crate::palw_registry::PalwClassRegistrationV1;
use crate::palw_schedule::{PalwEconomicFactsV1, PalwPanelCandidateV1, job_schedule_v1, max_leverage_holds_v1};
use kaspa_hashes::Hash64;
use std::collections::BTreeSet;

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
    /// ADR-0038 Decision D: this class's own DAA loop.
    ///
    /// NOT an `Option`. A network that registers a class registers its difficulty domain with it, and
    /// an `Option::None` here would be a silent absence for a value whose zero
    /// (`PalwClassDaaParamsV1::boot_target`) is the MAXIMUM block weight on the network.
    pub class_daa: crate::palw_class_daa::PalwClassDaaParamsV1,
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

/// The ADR-0028 §2 candidate set at ONE anchor, derived from the bond records a chain point
/// holds.
///
/// The rule lives here rather than at the call site because the call site got it wrong: it passed
/// the constant `bonded: true` for every record in its bond view, and a view holds EVERY record it
/// has ever been given, not the active ones (`ActiveBondView::records()` is the whole map). So a
/// `Slashed`, `Unbonding` or not-yet-`Active` bond took a panel seat and could be paid for
/// attesting — the eligibility predicate `select_replay_panel_v1`'s own doc says lives in the
/// function was being satisfied by a hardcoded answer from its caller.
///
/// `bonded` is a question about a POINT OF VIEW, and the point of view is the job's anchor:
/// `is_bond_active_at(record, anchor_daa)`, the same predicate the rest of the overlay judges bonds
/// with. `frozen` is `false` for every candidate on purpose — freezing is decided class-wide, once,
/// and fail-closed before this set is built, so a per-candidate copy could only disagree with it.
///
/// `executor_owner` closes self-attestation, which the lottery cannot: it excludes the executor by
/// `validator_id` alone, so an executor that funds a SECOND bond under a different validator key
/// draws itself onto its own panel and licenses its own work. The two keys are different, so no
/// rule the lottery can state catches it; the OWNER is the same, and only the bond records carry
/// that. Same-owner candidates are marked ineligible here.
///
/// This excludes an operator from attesting to ITS OWN executions, not from attesting at all — one
/// operator running several validators stays useful for every other executor's jobs. It also does
/// NOT close the one-signature-two-bonds hole: two bonds under one validator key may carry
/// different owners, which is what the lottery's own duplicate rejection is for.
///
/// `q` and the executor's `validator_id` exclusion remain the lottery's job.
pub fn panel_candidates_at_anchor_v1(
    bonds: &[crate::dns_finality::StakeBondRecord],
    runtime_class_id: Hash64,
    anchor_daa: u64,
    executor_owner: Hash64,
) -> Vec<PalwPanelCandidateV1> {
    bonds
        .iter()
        .map(|b| PalwPanelCandidateV1 {
            validator_id: b.validator_pubkey_hash,
            runtime_class_id,
            bonded: crate::dns_finality::is_bond_active_at(b, anchor_daa) && b.owner_pubkey_hash != executor_owner,
            frozen: false,
        })
        .collect()
}

/// The same set as SEATS, for the V3 draw: one candidate per bond, keyed on the bond outpoint.
///
/// Everything the v1 form decides is decided here identically — `bond_status` from
/// `effective_bond_status` at the anchor, the executor's own operator excluded — with two
/// differences that only the seat form can express:
///
/// * the bond outpoint travels, so two bonds under ONE validator key are two distinct seats
///   instead of one ambiguous id the v1 lottery has to drop entirely;
/// * `operator_root` carries the owner, so the draw's own per-operator dedup applies. That dedup is
///   about ONE voice per operator on a panel; the `executor_owner` exclusion below is a different
///   rule (no voice at all for the executor's own operator) and both are needed.
///
/// `class_frozen` is `false` for every seat for the same reason `frozen` is in the v1 form:
/// freezing is decided class-wide, once, and fail-closed before this set is built.
///
/// A bond the executor owns is marked `Slashed` rather than dropped from the vector. That is
/// deliberate: the V3 draw's eligibility rule is a status check, so encoding "not eligible" as the
/// status it already refuses keeps ONE rejection path instead of two, and a caller cannot
/// accidentally reinstate the seat by re-filtering. It never becomes a slash — nothing reads this
/// status as a bond record.
/// **`bond_status` here is an ELIGIBILITY VERDICT, not the bond's actual status**, and the two
/// panel assemblers in this tree now disagree about that field on purpose. Read this before
/// treating a `Slashed` here as a fact about a bond.
///
/// `palw_job_panel::palw_panel_candidates_v1` — the consensus-side assembler — reports
/// `effective_bond_status`, the truth. This one reports `Active` iff the bond may take a seat and
/// `Slashed` otherwise, because it must express **an exclusion the candidate type cannot hold**:
/// every bond of the executor's OWNER is barred, and `eligible_seats_v3` is only given the
/// executor's validator id. Encoding that as a status is how the exclusion survives the call.
///
/// The cost is that a merely `Unbonding` or not-yet-`Active` bond reads as `Slashed` downstream, so
/// nothing may take a slash decision, a telemetry count, or an operator-facing message from this
/// field. The draw is the only correct consumer; it asks one question of it and that question is
/// answered correctly.
///
/// Stated rather than repaired because repairing it means an eligibility field on
/// `PalwPanelCandidateV3` and a migration of both assemblers, and this path is audited and paying:
/// a speculative refactor of a live mint path is the more expensive mistake. `the_credit_panel_
/// encodes_eligibility_not_status` pins the encoding so a future edit that "fixes" the status
/// breaks loudly instead of silently widening the panel to the executor's own operator.
pub fn panel_seats_at_anchor_v3(
    bonds: &[crate::dns_finality::StakeBondRecord],
    runtime_class_id: Hash64,
    anchor_daa: u64,
    executor_owner: Hash64,
) -> Vec<PalwPanelCandidateV3> {
    bonds
        .iter()
        .map(|b| {
            let eligible = crate::dns_finality::is_bond_active_at(b, anchor_daa) && b.owner_pubkey_hash != executor_owner;
            PalwPanelCandidateV3 {
                validator_id: b.validator_pubkey_hash,
                bond_outpoint: b.bond_outpoint,
                runtime_class_id,
                bond_status: if eligible { crate::dns_finality::BondStatus::Active } else { crate::dns_finality::BondStatus::Slashed },
                class_frozen: false,
                operator_root: Some(b.owner_pubkey_hash),
            }
        })
        .collect()
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
/// chain (ADR-0028 §2 keeps it settled) and `anchor_daa` is that block's own DAA score,
/// `candidates` the bonded set the caller derived at that anchor, `refutation_accepted_daas`
/// the accepted-DAA stamps of every accepted refutation against this root — a refutation
/// accepted AFTER the window still convicts, but does not revoke credit (the deliberate §3
/// asymmetry; the ledger counts that tail separately).
///
/// `job_id` must come from an AUTHENTICATED envelope: the commitment digest covers the envelope
/// hash, and the caller verifies that signature before calling. It is miner-chosen, and the honest
/// reason that is safe is NOT that the anchor is unpredictable — `PalwScheduleParamsV1::validate`
/// only requires `delta_bind != 0`, so with a small Δ_bind a miner-executor can mine the anchor
/// itself and grind its hash. It is safe because nothing here relies on the panel being
/// unpredictable: replays are full and refutation is permissionless (ADR-0028 §2). A Δ_bind floor
/// is the change to make if unpredictability ever becomes load-bearing.
#[allow(clippy::too_many_arguments)]
pub fn decide_credit_v1(
    params: &PalwCreditParamsV1,
    commitment: &PalwObservedCommitmentV1,
    network_id: &[u8],
    job_id: Hash64,
    anchor: &Hash64,
    anchor_daa: u64,
    candidates: &[PalwPanelCandidateV3],
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
    // The panel is derived, never stored — the rule cannot drift from its output (§2) — and it is
    // drawn as SEATS, keyed on the bond outpoint.
    //
    // The id-keyed draw could not express two bonds under one validator key: their tickets and
    // their tie-break were the same value, so it had to drop the id entirely (fail-closed, but both
    // bonds then undrawable forever). A seat is `(validator_id, bond_outpoint)` and the outpoint is
    // unique, so each bond enters under its own identity.
    let panel = select_job_panel_at_anchor_v3(
        network_id,
        job_id,
        commitment.committed_root,
        *anchor,
        anchor_daa,
        &commitment.executor_id,
        &commitment.runtime_class_id,
        candidates,
        params.registration.windows.q as usize,
    );
    // ≥1 assigned attestation, on time, with the independently recomputed root equal to
    // C's. Panel order (not arrival order) fixes the payout order; one share per SEAT.
    //
    // A seat is matched by an attestation FILED BY THAT SEAT'S BOND — no tie-break is needed any
    // more, because the assignee and the payee are one value. The old loop matched on the
    // non-unique validator id and then chose among the filings by outpoint, so which bond got paid
    // was a property of the filings rather than of the draw.
    let mut paid: Vec<PalwPaidAttesterV1> = Vec::new();
    // ONE SIGNATURE FUNDS AT MOST ONE SHARE. The signed attestation message does not cover the bond
    // outpoint, so a validator holding two bonds can file the SAME signature under each of them and
    // collect two shares for one replay — and with a seat-keyed draw both of its bonds are now
    // genuinely drawable, which is exactly what makes the replay reachable. `q` is meant to buy `q`
    // independent checks; paying one party twice buys one check and pays for two. Keyed on the
    // signed content — `(attester_id, attested_logits_root)` — because that is what one signature
    // is, and the first seat in panel order keeps the share so the choice is the draw's, not the
    // filer's. The structural fix is for the message to name the bond; this floor holds regardless.
    let mut signature_used: BTreeSet<(Hash64, Hash64)> = BTreeSet::new();
    for seat in &panel {
        let earning = attestations.iter().find(|a| {
            a.bond_outpoint == seat.bond_outpoint
                && a.attester_id == seat.validator_id
                && a.attested_logits_root == commitment.logits_root
                && a.accepted_daa <= schedule.replay_deadline_daa
        });
        if let Some(a) = earning
            && signature_used.insert((a.attester_id, a.attested_logits_root))
        {
            paid.push(PalwPaidAttesterV1 { validator_id: seat.validator_id, bond_outpoint: seat.bond_outpoint });
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

    /// The credit panel's `bond_status` is an ELIGIBILITY verdict, and this pins that encoding.
    ///
    /// The exclusion it carries cannot be expressed any other way: every bond of the executor's
    /// OWNER is barred, while `eligible_seats_v3` is only told the executor's validator id. A
    /// future edit that "corrects" this field to report the real status would silently widen the
    /// panel to include the executor's own second bond — which is the executor attesting to its own
    /// work and collecting the attester share for it.
    #[test]
    fn the_credit_panel_encodes_eligibility_not_status() {
        fn rec(seed: u8) -> crate::dns_finality::StakeBondRecord {
            crate::dns_finality::StakeBondRecord {
                version: 1,
                bond_outpoint: crate::tx::TransactionOutpoint::new(Hash64::from_bytes([seed; 64]), 0),
                owner_pubkey_hash: Hash64::from_u64_word(seed as u64),
                validator_pubkey_hash: Hash64::from_u64_word(seed as u64),
                validator_pubkey: vec![seed; 32],
                amount: 20_000,
                activation_daa_score: 0,
                created_daa_score: 0,
                unbonding_period_blocks: 100,
                owner_reward_spk_payload: [0u8; 64],
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                status: crate::dns_finality::BondStatus::Active,
            }
        }
        let owner = Hash64::from_u64_word(0xE0);
        let mut executor_second_bond = rec(2);
        executor_second_bond.owner_pubkey_hash = owner; // same owner as the executor
        let mut outsider = rec(3);
        outsider.owner_pubkey_hash = Hash64::from_u64_word(0xF0);
        let mut unbonding = rec(4);
        unbonding.owner_pubkey_hash = Hash64::from_u64_word(0xF1);
        unbonding.unbond_request_daa_score = Some(500);

        let seats = panel_seats_at_anchor_v3(
            &[executor_second_bond.clone(), outsider.clone(), unbonding.clone()],
            Hash64::from_u64_word(7),
            1_000,
            owner,
        );
        let status_of = |op| seats.iter().find(|s| s.bond_outpoint == op).unwrap().bond_status;

        assert_eq!(
            status_of(executor_second_bond.bond_outpoint),
            crate::dns_finality::BondStatus::Slashed,
            "the executor's own operator must be barred — this seat is the self-attestation hole"
        );
        assert_eq!(status_of(outsider.bond_outpoint), crate::dns_finality::BondStatus::Active);
        assert_eq!(
            status_of(unbonding.bond_outpoint),
            crate::dns_finality::BondStatus::Slashed,
            "an ineligible bond reads Slashed whatever it actually is — nothing may take a slash decision from this field"
        );
    }
    use super::*;
    // The id-keyed lottery, kept in the tests only: the v1 candidate form and its draw are still
    // the contrast these assertions are about (what the seat form can express and it cannot).
    use crate::palw_schedule::{PalwLeverageRemedyV1, select_replay_panel_v1};

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
            class_daa: crate::palw_class_daa::PalwClassDaaParamsV1::stage1_defaults(),
        }
    }

    const SUBSIDY: u64 = 370_468_345 * 1_200; // the 120 s net's rate-preserved subsidy

    /// The chain identity the V3 seed binds. A fixture value; the live caller passes the genesis
    /// hash, so a panel drawn on one network is not the panel on another.
    const NET: &[u8] = b"misaka-credit-test";
    /// The authenticated envelope's job id — miner-chosen but signature-covered.
    fn job_id() -> Hash64 {
        Hash64::from_u64_word(0x5A)
    }
    /// The anchor BLOCK's own DAA score, which the snapshot root binds.
    const ANCHOR_DAA: u64 = 1_010;

    fn candidate(id: Hash64, class: Hash64) -> PalwPanelCandidateV3 {
        PalwPanelCandidateV3 {
            validator_id: id,
            bond_outpoint: bond_of(id),
            runtime_class_id: class,
            bond_status: crate::dns_finality::BondStatus::Active,
            class_frozen: false,
            operator_root: None,
        }
    }

    /// Executor 0xE0 plus four bonded same-class candidates; q = 2 draws two SEATS of them.
    fn scene()
    -> (PalwCreditParamsV1, PalwObservedCommitmentV1, Hash64, Vec<PalwPanelCandidateV3>, Vec<crate::palw_job_panel::PalwPanelSeatV3>)
    {
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
        let panel = select_job_panel_at_anchor_v3(
            NET,
            job_id(),
            commitment.committed_root,
            anchor,
            ANCHOR_DAA,
            &commitment.executor_id,
            &class,
            &candidates,
            params.registration.windows.q as usize,
        );
        assert_eq!(panel.len(), 2, "q=2 out of four non-executor candidates");
        (params, commitment, anchor, candidates, panel)
    }

    /// The decision, with every argument the tests hold constant filled in.
    fn decide(
        params: &PalwCreditParamsV1,
        commitment: &PalwObservedCommitmentV1,
        anchor: &Hash64,
        candidates: &[PalwPanelCandidateV3],
        attestations: &[PalwObservedAttestationV1],
        refutations: &[u64],
    ) -> PalwCreditDecisionV1 {
        decide_credit_v1(params, commitment, NET, job_id(), anchor, ANCHOR_DAA, candidates, attestations, refutations, SUBSIDY)
    }

    /// A bond outpoint for a validator id — one per attester, so a payee is identifiable.
    fn bond_of(attester: Hash64) -> crate::tx::TransactionOutpoint {
        crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes(attester.as_bytes()), index: 0 }
    }

    /// An on-time attestation filed BY THAT SEAT'S BOND — the only filing a seat can be matched by.
    fn on_time(seat: &crate::palw_job_panel::PalwPanelSeatV3, logits: Hash64) -> PalwObservedAttestationV1 {
        // anchor = 1 000 + Δ_bind(10); replay deadline = anchor + w_replay(30) = 1 040.
        PalwObservedAttestationV1 {
            attester_id: seat.validator_id,
            bond_outpoint: seat.bond_outpoint,
            attested_logits_root: logits,
            accepted_daa: 1_035,
        }
    }

    /// What the decision must pay for a given set of seats, in the given order.
    fn expect_paid(seats: &[&crate::palw_job_panel::PalwPanelSeatV3]) -> Vec<PalwPaidAttesterV1> {
        seats.iter().map(|s| PalwPaidAttesterV1 { validator_id: s.validator_id, bond_outpoint: s.bond_outpoint }).collect()
    }

    /// The candidate set is derived from bond STATUS at the anchor, not asserted by the caller.
    ///
    /// This was `bonded: true` for every record the bond view held, and a view holds every record
    /// it has ever been given — so a slashed bond took a panel seat and could be paid for
    /// attesting to the work of the executor that had just been slashed alongside it.
    #[test]
    fn only_bonds_active_at_the_anchor_become_candidates() {
        use crate::dns_finality::{BondStatus, StakeBondRecord, effective_bond_status};
        let class = h64(0xC1);
        let rec = |seed: u8, activation: u64, unbond: Option<u64>, slashed: Option<u64>| StakeBondRecord {
            version: 1,
            bond_outpoint: crate::tx::TransactionOutpoint {
                transaction_id: crate::tx::TransactionId::from_bytes([seed; 64]),
                index: 0,
            },
            owner_pubkey_hash: h64(seed),
            validator_pubkey_hash: h64(seed),
            validator_pubkey: vec![seed; 32],
            amount: 20_000 * 100_000_000,
            activation_daa_score: activation,
            created_daa_score: 0,
            unbonding_period_blocks: 10_083,
            owner_reward_spk_payload: [0u8; 64],
            unbond_request_daa_score: unbond,
            slashed_at_daa_score: slashed,
            // The raw field is vestigial — `effective_bond_status` recomputes from the stamps —
            // so it is deliberately set to the value a naive reader would trust, to show that the
            // derivation and not this byte is what decides eligibility.
            status: BondStatus::Active,
        };
        // One of each status AT anchor 1 000: active, pending (activates later), unbonding, slashed.
        let bonds = vec![
            rec(0x01, 500, None, None),
            rec(0x02, 5_000, None, None),
            rec(0x03, 500, Some(900), None),
            rec(0x04, 500, None, Some(800)),
        ];
        let anchor_daa = 1_000;
        let candidates = panel_candidates_at_anchor_v1(&bonds, class, anchor_daa, h64(0xE0));
        assert_eq!(candidates.len(), bonds.len(), "every record is a candidate; only `bonded` differs");
        for (c, b) in candidates.iter().zip(&bonds) {
            assert_eq!(c.runtime_class_id, class);
            assert!(!c.frozen, "freezing is class-wide and decided before this set is built");
            assert_eq!(
                c.bonded,
                effective_bond_status(b, anchor_daa) == BondStatus::Active,
                "`bonded` must be the anchor's own answer for {:?}",
                b.validator_pubkey_hash
            );
        }
        assert_eq!(candidates.iter().filter(|c| c.bonded).count(), 1, "exactly the one active bond is eligible");
        assert!(candidates[0].bonded, "the active bond");
        for i in 1..4 {
            assert!(!candidates[i].bonded, "pending/unbonding/slashed must not be eligible (index {i})");
        }

        // And the point of view moves the answer: at a later anchor the pending bond is active and
        // the unbonding one is still out.
        let later = panel_candidates_at_anchor_v1(&bonds, class, 6_000, h64(0xE0));
        assert!(later[1].bonded, "the pending bond activated by 6 000");
        assert!(!later[2].bonded && !later[3].bonded);

        // The lottery then applies the flags rather than re-deciding them.
        let panel = select_replay_panel_v1(&h64(0x71), &h64(0xE0), &h64(0x04), &class, &candidates, 4);
        assert_eq!(panel, vec![h64(0x01)], "only the active bond can be drawn");

        // Self-attestation: the executor funds a SECOND bond under a different validator key but
        // its own owner. The lottery cannot see this — the validator keys genuinely differ — so a
        // panel drawn on validator id alone seats the executor's own sibling and lets it license
        // the executor's work.
        let mut sibling = rec(0x09, 500, None, None);
        sibling.owner_pubkey_hash = h64(0xE0); // the executor's operator
        let with_sibling = vec![bonds[0].clone(), sibling.clone()];
        let drawn = panel_candidates_at_anchor_v1(&with_sibling, class, anchor_daa, h64(0xE0));
        assert!(drawn[0].bonded, "an unrelated active bond is untouched");
        assert!(!drawn[1].bonded, "the executor's own second bond must not be eligible for its own job");
        assert_eq!(
            select_replay_panel_v1(&h64(0x71), &h64(0xE0), &h64(0x04), &class, &drawn, 4),
            vec![h64(0x01)],
            "the sibling is out of the draw entirely"
        );
        // It is excluded from THIS executor's panel, not from attesting at all: for another
        // executor's job the same bond is a perfectly good verifier.
        let other_job = panel_candidates_at_anchor_v1(&with_sibling, class, anchor_daa, h64(0xEE));
        assert!(other_job[1].bonded, "an operator stays useful for every other executor's jobs");
    }

    #[test]
    fn the_predicate_credits_and_pays_panel_members_in_panel_order() {
        let (params, commitment, anchor, candidates, panel) = scene();
        let attestations = vec![on_time(&panel[1], commitment.logits_root), on_time(&panel[0], commitment.logits_root)];
        let d = decide(&params, &commitment, &anchor, &candidates, &attestations, &[]);
        assert!(d.creditable);
        assert_eq!(d.paid_attesters, expect_paid(&[&panel[0], &panel[1]]), "payout order is panel order, not arrival order");
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
        let attestations = vec![on_time(&panel[0], commitment.logits_root)];
        let d = decide(&params, &commitment, &anchor, &candidates, &attestations, &[]);
        assert!(d.creditable);
        assert_eq!(d.paid_attesters, expect_paid(&[&panel[0]]));
    }

    #[test]
    fn zero_attestations_mean_zero_credit_never_a_shrunk_panel() {
        let (params, commitment, anchor, candidates, _) = scene();
        let d = decide(&params, &commitment, &anchor, &candidates, &[], &[]);
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
    fn the_paid_attester_names_the_seat_that_filed() {
        let (params, commitment, anchor, candidates, panel) = scene();

        // The payee is the SEAT'S bond, and it is the draw's choice rather than the filings'.
        let single = vec![on_time(&panel[0], commitment.logits_root)];
        let d = decide(&params, &commitment, &anchor, &candidates, &single, &[]);
        assert_eq!(d.paid_attesters, expect_paid(&[&panel[0]]), "the payee is the seat's own bond");

        // A filing that names the seat's validator id but a DIFFERENT bond earns nothing. Under the
        // id-keyed matching this was the ambiguity that had to be broken by sorting the filings;
        // now the seat names its bond, so there is nothing to break.
        let mut foreign_bond = on_time(&panel[0], commitment.logits_root);
        foreign_bond.bond_outpoint =
            crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes([0xFE; 64]), index: 0 };
        let d = decide(&params, &commitment, &anchor, &candidates, &[foreign_bond], &[]);
        assert_eq!(d, PalwCreditDecisionV1::nothing(), "a filing from an unseated bond is not an assigned attestation");

        // Order-invariance, still pinned: the answer is a function of the records.
        let both = vec![on_time(&panel[1], commitment.logits_root), on_time(&panel[0], commitment.logits_root)];
        let mut reversed = both.clone();
        reversed.reverse();
        assert_eq!(
            decide(&params, &commitment, &anchor, &candidates, &both, &[]),
            decide(&params, &commitment, &anchor, &candidates, &reversed, &[])
        );
    }

    /// ONE SIGNATURE FUNDS ONE SHARE, even when its signer holds two seated bonds.
    ///
    /// The signed attestation message covers `(network, executor, job_context, logits_root,
    /// committed_root)` — NOT the bond outpoint. So a validator with two bonds can file the same
    /// signature under each, and the seat-keyed draw now genuinely seats both, which is what makes
    /// the replay reachable. `q` is meant to buy `q` independent checks; paying one party twice buys
    /// one check and pays for two.
    #[test]
    fn one_signature_cannot_fund_two_seats() {
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
        // Two bonds under ONE validator key, plus one unrelated validator.
        let twin_key = h64(0x01);
        let mut twin_a = candidate(twin_key, class);
        twin_a.bond_outpoint =
            crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes([0x11; 64]), index: 0 };
        let mut twin_b = candidate(twin_key, class);
        twin_b.bond_outpoint =
            crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes([0x22; 64]), index: 0 };
        let draw = |cands: &[PalwPanelCandidateV3]| {
            select_job_panel_at_anchor_v3(
                NET,
                job_id(),
                commitment.committed_root,
                anchor,
                ANCHOR_DAA,
                &commitment.executor_id,
                &class,
                cands,
                params.registration.windows.q as usize,
            )
        };

        // Both of one key's bonds seated: exactly the situation the seat rewrite created, and the
        // one the id-keyed draw could not reach.
        let twins = vec![twin_a.clone(), twin_b.clone()];
        let panel = draw(&twins);
        assert_eq!(panel.len(), 2, "q=2 seats both twin bonds: {panel:?}");
        assert!(panel.iter().all(|s| s.validator_id == twin_key));

        // The same signature — same attester id, same attested root — filed under each bond.
        let filings: Vec<PalwObservedAttestationV1> = panel.iter().map(|s| on_time(s, commitment.logits_root)).collect();
        let d = decide(&params, &commitment, &anchor, &twins, &filings, &[]);
        assert!(d.creditable, "the work was done once and is creditable once");
        assert_eq!(d.paid_attesters.len(), 1, "one signature, one share: {:?}", d.paid_attesters);
        assert_eq!(d.paid_attesters[0].bond_outpoint, panel[0].bond_outpoint, "the first seat in PANEL order keeps it");

        // Two DISTINCT signers each earn their own share — the dedup is per signature, not a cap on
        // shares, so it must cost an honest second verifier nothing.
        let mixed_cands = vec![twin_a, candidate(h64(0x02), class)];
        let mixed_panel = draw(&mixed_cands);
        assert_eq!(mixed_panel.len(), 2);
        let mixed: Vec<PalwObservedAttestationV1> = mixed_panel.iter().map(|s| on_time(s, commitment.logits_root)).collect();
        let d = decide(&params, &commitment, &anchor, &mixed_cands, &mixed, &[]);
        assert_eq!(d.paid_attesters.len(), 2, "two independent verifiers earn two shares");
    }

    #[test]
    fn late_wrong_root_off_panel_and_duplicate_attestations_do_not_pay() {
        let (params, commitment, anchor, candidates, panel) = scene();
        let late = PalwObservedAttestationV1 { accepted_daa: 1_041, ..on_time(&panel[0], commitment.logits_root) };
        let wrong_root = on_time(&panel[1], h64(0xBB));
        // The executor is never on its own panel, so its filing names no seat.
        let off_panel = PalwObservedAttestationV1 {
            attester_id: h64(0xE0),
            bond_outpoint: bond_of(h64(0xE0)),
            attested_logits_root: commitment.logits_root,
            accepted_daa: 1_035,
        };
        let d = decide(&params, &commitment, &anchor, &candidates, &[late, wrong_root, off_panel], &[]);
        assert_eq!(d, PalwCreditDecisionV1::nothing(), "late, non-matching and unassigned attestations earn nothing");

        // A duplicate on-time match pays its seat once.
        let dup = vec![on_time(&panel[0], commitment.logits_root), on_time(&panel[0], commitment.logits_root)];
        let d = decide(&params, &commitment, &anchor, &candidates, &dup, &[]);
        assert_eq!(d.paid_attesters, expect_paid(&[&panel[0]]));
    }

    #[test]
    fn a_refutation_inside_the_window_voids_credit_and_one_after_does_not() {
        let (params, commitment, anchor, candidates, panel) = scene();
        let attestations = vec![on_time(&panel[0], commitment.logits_root)];
        // challenge_close = 1 000 + 720 = 1 720.
        let inside = decide(&params, &commitment, &anchor, &candidates, &attestations, &[1_720]);
        assert_eq!(inside, PalwCreditDecisionV1::nothing(), "an accepted refutation inside the window voids credit");
        let after = decide(&params, &commitment, &anchor, &candidates, &attestations, &[1_721]);
        assert!(after.creditable, "conviction after the window is slash material, never a credit revocation (§3 asymmetry)");
    }

    #[test]
    fn a_foreign_class_a_zero_ceiling_and_a_failed_inequality_all_credit_nothing() {
        let (params, commitment, anchor, candidates, panel) = scene();
        let attestations = vec![on_time(&panel[0], commitment.logits_root)];

        let mut foreign = commitment.clone();
        foreign.runtime_class_id = h64(0x77);
        let d = decide(&params, &foreign, &anchor, &candidates, &attestations, &[]);
        assert_eq!(d, PalwCreditDecisionV1::nothing());

        let mut frozen = params.clone();
        frozen.registration = frozen.registration.to_zero_credit();
        let d = decide(&frozen, &commitment, &anchor, &candidates, &attestations, &[]);
        assert_eq!(d, PalwCreditDecisionV1::nothing(), "the rollback is inside the gate, not beside it");

        let mut unbounded = params.clone();
        unbounded.registration.leverage_remedy = PalwLeverageRemedyV1 { min_credit_interval_daa: 1, base_subsidy_permille: 1_000 };
        let d = decide(&unbounded, &commitment, &anchor, &candidates, &attestations, &[]);
        assert_eq!(d, PalwCreditDecisionV1::nothing(), "an unbounded remedy refuses activation through the same door");

        // ADR-0039 1a through the same door: a class whose catalog is open credits nothing.
        // The gate is on the DEPTH, so this is the float fleet class's actual behaviour — the
        // only change needed to reach it is the honest one.
        let mut open_catalog = params.clone();
        open_catalog.registration.adjudication_depth = crate::palw_registry::PalwAdjudicationDepthV1::StructuralOnly;
        assert!(!open_catalog.active_for(commitment.accepted_daa, SUBSIDY));
        let d = decide(&open_catalog, &commitment, &anchor, &candidates, &attestations, &[]);
        assert_eq!(d, PalwCreditDecisionV1::nothing(), "a class the court cannot convict earns no slash-bearing credit");
        // And it is not a fixture artifact: the shipped float class IS that class.
        let mut float_class = params.clone();
        float_class.registration = crate::palw_registry::tests::fleet_registration();
        assert!(!float_class.active_for(commitment.accepted_daa, SUBSIDY));

        let mut before_activation = params.clone();
        before_activation.activation_daa = 5_000;
        let d = decide(&before_activation, &commitment, &anchor, &candidates, &attestations, &[]);
        assert_eq!(d, PalwCreditDecisionV1::nothing());
    }
}
