//! **ADR-0132 — the end-to-end economics ledger: what a claim actually cost and what it was
//! actually paid, node-local, in shadow.**
//!
//! ADR-0131 prices a claim; this module follows the claim from its acceptance to the sompi its
//! `Final` named — or to the void that named nothing — and sums, per class, what the producers ran
//! (class draws × network draws × the draw's compute, ADR-0132 §1–2), what the panels replayed, and
//! what was paid to whom. The chain keeps a terminal claim only for its retention span, so a node
//! that wants a window longer than that writes rows as it sees them (`kaspad`'s recorder) and reads
//! the totals here. Nothing on the block path reads any of it; a row records a payout as the rule in
//! force at the claim's `Final` derived it, from the same functions the fold uses, so the ledger's
//! sompi are the coinbase's sompi for every claim the recorder observed before its `Final`.
//!
//! Every number is an integer; the rates are scaled by [`PALW_LEDGER_RATE_SCALE_V1`] (MSK-sompi per
//! 10⁹ MAC-equivalents), and `avg_*` fields are integer means.

use crate::palw_economic_compute_v1::{palw_attempted_compute_q32_per_claim_v1, palw_gap_permille_v1, palw_priced_reward_u128_v1};
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwVoidReasonV2};
use crate::{BlockHash, Hash64};
use borsh::{BorshDeserialize, BorshSerialize};

/// The row layout. A recorder that finds another version drops its rows and starts over.
pub const PALW_ECONOMICS_LEDGER_VERSION_V1: u32 = 2;

/// Rates are sompi per 10⁹ MAC-equivalents.
pub const PALW_LEDGER_RATE_SCALE_V1: u128 = 1_000_000_000;

/// **What the chain shows of one attempt-lane claim at a read** — the facts the ledger merges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwClaimLedgerObservationV1 {
    pub claim_id: Hash64,
    pub class_id: Hash64,
    pub producer_bond: PalwBondKeyV2,
    pub accepted_daa: u64,
    pub accepted_block: BlockHash,
    pub escrow_sompi: u64,
    pub pwu: u64,
    pub rebound_daa: Option<u64>,
    pub bound_daa: Option<u64>,
    pub licensed_daa: Option<u64>,
    pub final_daa: Option<u64>,
    pub voided_daa: Option<u64>,
    pub void_reason: Option<PalwVoidReasonV2>,
    /// The duty row's seats and how many were credited — present only while the row exists
    /// (bound past `palw_panel_economy`, until `Final`).
    pub seats: u16,
    pub credited_seats: u16,
    /// ADR-0132 Upgrade C: the economics the claim snapshotted at acceptance, while it is live and
    /// the payout fence wrote one.
    pub economics: Option<crate::palw_economic_payout_v1::PalwClaimEconomicsV1>,
}

/// Every attempt-lane claim in the state, as the ledger reads it. Free-prompt claims are the
/// receipt lane's and are not priced here.
pub fn palw_claim_ledger_observations_v1(state: &PalwChainStateV2) -> Vec<PalwClaimLedgerObservationV1> {
    state
        .claims_iter()
        .filter(|(_, claim)| matches!(claim.source, PalwClaimSourceV2::Attempt))
        .map(|(id, claim)| {
            let (bound_daa, licensed_daa, final_daa, voided_daa, void_reason) = match &claim.phase {
                PalwClaimPhaseV2::Provisional => (None, None, None, None, None),
                PalwClaimPhaseV2::PanelBound { bound_daa } => (Some(*bound_daa), None, None, None, None),
                PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => (None, Some(*licensed_daa), None, None, None),
                PalwClaimPhaseV2::Final { final_daa } => (None, None, Some(*final_daa), None, None),
                PalwClaimPhaseV2::Voided { voided_daa, reason } => (None, None, None, Some(*voided_daa), Some(*reason)),
                _ => (None, None, None, None, None),
            };
            let (seats, credited_seats) = state
                .panel_duty_row_of(id)
                .map(|row| (row.seats.len() as u16, row.seats.values().filter(|at| **at != 0).count() as u16))
                .unwrap_or((0, 0));
            PalwClaimLedgerObservationV1 {
                claim_id: *id,
                class_id: claim.class_id,
                producer_bond: claim.bond,
                accepted_daa: claim.accepted_daa,
                accepted_block: claim.accepted_block,
                escrow_sompi: claim.escrowed_reward,
                pwu: claim.pwu,
                rebound_daa: claim.rebound_daa,
                bound_daa,
                licensed_daa,
                final_daa,
                voided_daa,
                void_reason,
                seats,
                credited_seats,
                economics: state.claim_economics_of(id).copied(),
            }
        })
        .collect()
}

/// **One claim's row**, written by a recorder and kept past the chain's retention. First-seen
/// facts (the class's draws and compute at the time, the network draws of the accepted block) stay;
/// lifecycle marks fill in as they are observed; the payout is derived once, at the first read that
/// sees the `Final`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, serde::Serialize, serde::Deserialize)]
pub struct PalwClaimLedgerRowV1 {
    pub version: u32,
    pub claim_id: Hash64,
    pub class_id: Hash64,
    /// The producer's bond outpoint (`PalwBondKeyV2`'s inner outpoint, which the node's database
    /// can encode).
    pub producer_bond: crate::tx::TransactionOutpoint,
    pub accepted_daa: u64,
    pub accepted_block: BlockHash,
    pub escrow_sompi: u64,
    pub pwu: u64,
    /// The class draws a claim costs in expectation, Q32, at the class target when first seen.
    pub expected_attempts_q32: u128,
    /// The network draws a class win costs, Q32, from the accepted block's `bits`.
    pub network_expected_attempts_q32: u128,
    /// The economic compute of the job one draw runs (ADR-0131), at first sight.
    pub draw_compute: u128,
    /// The class's `pwu_per_inference` at first sight — today's price basis.
    pub leaves: u64,
    pub first_seen_daa: u64,
    pub last_seen_daa: u64,
    pub bound_daa: Option<u64>,
    pub rebound_daa: Option<u64>,
    pub licensed_daa: Option<u64>,
    pub final_daa: Option<u64>,
    pub voided_daa: Option<u64>,
    /// `receipt_timeout`, `bind_timeout`, `court_fraud`, `producer_withholding`, or empty.
    pub void_reason: String,
    /// The most seats a duty row showed, and the most credited.
    pub seats: u16,
    pub credited_seats: u16,
    /// What the `Final` named, by the rule in force then: producer, seats, reserve; and the
    /// escrow the price left unminted.
    pub producer_paid_sompi: u64,
    pub panel_paid_sompi: u64,
    pub reserve_sompi: u64,
    pub burned_sompi: u64,
    /// A claim that holds no escrow was paid its carve at acceptance (a merged block below the deep
    /// fence, ADR-0058 B-1) — nothing is named at its `Final`.
    pub paid_at_acceptance: bool,
    /// ADR-0132 Upgrade C: the claim snapshotted its economics at acceptance (the payout fence was
    /// active there); its `Final` is then priced at `rate` with the panel at `share`, and the three
    /// first-sight facts above are the snapshot's own.
    pub economic_snapshotted: bool,
    pub economic_rate_sompi_per_giga: u64,
    pub economic_panel_share_permille: u16,
}

impl kaspa_utils::mem_size::MemSizeEstimator for PalwClaimLedgerRowV1 {}

/// The class facts a row snapshots at first sight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwLedgerClassFactsV1 {
    pub expected_attempts_q32: u128,
    pub network_expected_attempts_q32: u128,
    pub draw_compute: u128,
    pub leaves: u64,
}

/// **The payout rule in force at a claim's `Final`**, as the fold applies it (ADR-0124 Decisions 1,
/// 2 and 6): whether the work price applies to this class, its leaves against the unit, and whether
/// a duty row splits the reward with the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwLedgerPayoutRuleV1 {
    /// `palw_work_priced_reward` active at the `Final`, and the class is a priced (model) class.
    pub work_priced: bool,
    pub leaves: u64,
    pub unit_leaves: u64,
    /// `palw_panel_economy` active when the panel bound: a duty row exists and the split applies.
    pub panel_economy: bool,
    /// ADR-0132 Upgrade C: the claim snapshotted its economics — priced `min(escrow, attempted ×
    /// rate)` with the panel at the snapshot's share, in place of the work price and the fifth.
    pub economic: Option<PalwLedgerEconomicRuleV1>,
}

/// The rate rule a snapshotted claim's `Final` applies (ADR-0132 Upgrade C).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwLedgerEconomicRuleV1 {
    pub attempted_ccu: u128,
    pub rate_sompi_per_giga: u64,
    pub panel_share_permille: u16,
}

/// What a `Final` named.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwLedgerPayoutV1 {
    pub producer_sompi: u64,
    pub panel_sompi: u64,
    pub reserve_sompi: u64,
    pub burned_sompi: u64,
}

impl PalwLedgerPayoutV1 {
    /// `producer + panel + reserve + burned` — the escrow, always.
    pub fn total(&self) -> u64 {
        self.producer_sompi + self.panel_sompi + self.reserve_sompi + self.burned_sompi
    }
}

/// [`PalwLedgerPayoutRuleV1`] applied to one `Final`'s escrow, with the same functions the fold uses
/// (`palw_work_priced_reward_v1`'s rule over `u128`, `palw_panel_split_v1`). ADR-0091's buyback is
/// not modelled: no line on testnet-11 has a pair, and where one does the slice is a fraction of the
/// producer's share, not of the emission.
pub fn palw_ledger_payout_v1(escrow_sompi: u64, rule: PalwLedgerPayoutRuleV1, seats: u16, credited: u16) -> PalwLedgerPayoutV1 {
    let reward = match rule.economic {
        Some(economic) => palw_rate_priced_reward_v1(escrow_sompi, economic.attempted_ccu, economic.rate_sompi_per_giga as u128),
        None if rule.work_priced => palw_priced_reward_u128_v1(escrow_sompi, rule.leaves as u128, rule.unit_leaves as u128),
        None => escrow_sompi,
    };
    let burned_sompi = escrow_sompi - reward;
    if rule.panel_economy && seats > 0 {
        let pool_permille = rule
            .economic
            .map(|economic| economic.panel_share_permille)
            .unwrap_or(crate::palw_panel_economy_v1::PALW_PANEL_POOL_PERMILLE_V1 as u16);
        let split =
            crate::palw_panel_economy_v1::palw_panel_split_permille_v1(reward, pool_permille, seats as usize, credited as usize);
        PalwLedgerPayoutV1 { producer_sompi: split.producer, panel_sompi: split.paid, reserve_sompi: split.reserve, burned_sompi }
    } else {
        PalwLedgerPayoutV1 { producer_sompi: reward, panel_sompi: 0, reserve_sompi: 0, burned_sompi }
    }
}

/// **Merge one observation into a row.** A first sight snapshots the class facts; a later one keeps
/// them. Lifecycle marks are set the first time they are seen and never cleared; seats and credited
/// seats keep their maximum; the payout is derived exactly once, when the `Final` is first seen,
/// from `rule_at_final` — a claim seen only after its `Final` is still priced (the rule is a
/// function of the height), but a claim the recorder never saw before its `Final` has no duty row
/// to read its credited seats from, and `seats` then stays what the recorder last saw (zero for
/// one it never saw bound). `paid_at_acceptance` is the escrow's absence.
pub fn palw_ledger_merge_v1(
    existing: Option<&PalwClaimLedgerRowV1>,
    obs: &PalwClaimLedgerObservationV1,
    seen_daa: u64,
    facts: PalwLedgerClassFactsV1,
    rule_at_final: impl FnOnce(u64) -> PalwLedgerPayoutRuleV1,
) -> PalwClaimLedgerRowV1 {
    let mut row = match existing {
        Some(row) if row.claim_id == obs.claim_id => row.clone(),
        _ => PalwClaimLedgerRowV1 {
            version: PALW_ECONOMICS_LEDGER_VERSION_V1,
            claim_id: obs.claim_id,
            class_id: obs.class_id,
            producer_bond: obs.producer_bond.0,
            accepted_daa: obs.accepted_daa,
            accepted_block: obs.accepted_block,
            escrow_sompi: obs.escrow_sompi,
            pwu: obs.pwu,
            // ADR-0132 Upgrade C: where the chain snapshotted the claim's economics, the row keeps
            // the chain's numbers, not the recorder's reading of the class now.
            expected_attempts_q32: obs.economics.map(|e| e.expected_attempts_q32).unwrap_or(facts.expected_attempts_q32),
            network_expected_attempts_q32: obs
                .economics
                .map(|e| e.network_expected_attempts_q32)
                .unwrap_or(facts.network_expected_attempts_q32),
            draw_compute: obs.economics.map(|e| e.draw_ccu).unwrap_or(facts.draw_compute),
            leaves: facts.leaves,
            first_seen_daa: seen_daa,
            last_seen_daa: seen_daa,
            bound_daa: None,
            rebound_daa: None,
            licensed_daa: None,
            final_daa: None,
            voided_daa: None,
            void_reason: String::new(),
            seats: 0,
            credited_seats: 0,
            producer_paid_sompi: 0,
            panel_paid_sompi: 0,
            reserve_sompi: 0,
            burned_sompi: 0,
            paid_at_acceptance: obs.escrow_sompi == 0,
            economic_snapshotted: obs.economics.is_some(),
            economic_rate_sompi_per_giga: obs.economics.map(|e| e.rate_sompi_per_giga).unwrap_or(0),
            economic_panel_share_permille: obs.economics.map(|e| e.panel_share_permille).unwrap_or(0),
        },
    };
    // A snapshot seen later than the first sight (a recorder that started after the acceptance)
    // still names the chain's numbers.
    if !row.economic_snapshotted
        && let Some(economics) = obs.economics
    {
        row.economic_snapshotted = true;
        row.economic_rate_sompi_per_giga = economics.rate_sompi_per_giga;
        row.economic_panel_share_permille = economics.panel_share_permille;
        row.expected_attempts_q32 = economics.expected_attempts_q32;
        row.network_expected_attempts_q32 = economics.network_expected_attempts_q32;
        row.draw_compute = economics.draw_ccu;
    }
    row.last_seen_daa = row.last_seen_daa.max(seen_daa);
    if row.bound_daa.is_none() {
        row.bound_daa = obs.bound_daa;
    }
    if row.rebound_daa.is_none() {
        row.rebound_daa = obs.rebound_daa;
    }
    if row.licensed_daa.is_none() {
        row.licensed_daa = obs.licensed_daa;
    }
    if row.voided_daa.is_none() {
        row.voided_daa = obs.voided_daa;
        if let Some(reason) = &obs.void_reason {
            row.void_reason = palw_void_reason_name_v1(reason).to_string();
        }
    }
    row.seats = row.seats.max(obs.seats);
    row.credited_seats = row.credited_seats.max(obs.credited_seats);
    if row.final_daa.is_none()
        && let Some(final_daa) = obs.final_daa
    {
        row.final_daa = Some(final_daa);
        if row.escrow_sompi > 0 {
            let mut rule = rule_at_final(final_daa);
            // ADR-0132 Upgrade C: a snapshotted claim's `Final` is priced by its snapshot — the
            // fold's rule — whatever the fences say at the `Final`'s height.
            if row.economic_snapshotted {
                rule.economic = Some(PalwLedgerEconomicRuleV1 {
                    attempted_ccu: palw_ledger_row_attempted_compute_v1(&row),
                    rate_sompi_per_giga: row.economic_rate_sompi_per_giga,
                    panel_share_permille: row.economic_panel_share_permille,
                });
            }
            let paid = palw_ledger_payout_v1(row.escrow_sompi, rule, row.seats, row.credited_seats);
            row.producer_paid_sompi = paid.producer_sompi;
            row.panel_paid_sompi = paid.panel_sompi;
            row.reserve_sompi = paid.reserve_sompi;
            row.burned_sompi = paid.burned_sompi;
        }
    }
    row
}

/// The name a void reason prints under — the `getPalwClaims` spelling.
pub fn palw_void_reason_name_v1(reason: &PalwVoidReasonV2) -> &'static str {
    match reason {
        PalwVoidReasonV2::BindTimeout => "bind_timeout",
        PalwVoidReasonV2::ReceiptTimeout => "receipt_timeout",
        PalwVoidReasonV2::CourtFraud => "court_fraud",
        PalwVoidReasonV2::ProducerWithholding => "producer_withholding",
        PalwVoidReasonV2::NoCapablePanel => "no_capable_panel",
        // ADR-0152 v22 skeleton: declared, written by nobody yet.
        PalwVoidReasonV2::UnavailableQuorum => "unavailable_quorum",
        PalwVoidReasonV2::NotReplayBacked => "not_replay_backed",
    }
}

/// The compute a row's producer ran in expectation: class draws × network draws × one draw's job.
pub fn palw_ledger_row_attempted_compute_v1(row: &PalwClaimLedgerRowV1) -> u128 {
    palw_attempted_compute_q32_per_claim_v1(
        row.network_expected_attempts_q32,
        palw_attempted_compute_q32_per_claim_v1(row.expected_attempts_q32, row.draw_compute),
    )
}

/// **One class's totals over the rows the recorder holds.**
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwClassLedgerTotalsV1 {
    pub class_id: Hash64,
    pub claims: u64,
    /// Ever bound to a panel; ever licensed (a `Final` was licensed first); `Final`; voided; redrawn.
    pub bound: u64,
    pub licensed: u64,
    pub finals: u64,
    pub voided: u64,
    pub redrawn: u64,
    pub paid_at_acceptance: u64,
    pub escrow_final_sompi: u128,
    pub producer_paid_sompi: u128,
    pub panel_paid_sompi: u128,
    pub reserve_sompi: u128,
    pub burned_sompi: u128,
    /// Σ over every claim of what its producer ran in expectation.
    pub attempted_compute: u128,
    /// Σ over the `Final` claims of one draw's job — the compute each `Final` certified.
    pub final_compute: u128,
    /// Σ over the claims that bound of `seats × one draw's job` — what a panel replays today
    /// (every live seat runs the whole job, ADR-0132 §1.6).
    pub verification_compute: u128,
    pub bind_wait_daa_sum: u64,
    pub bind_wait_n: u64,
    pub licence_wait_daa_sum: u64,
    pub licence_wait_n: u64,
    pub final_wait_daa_sum: u64,
    pub final_wait_n: u64,
    pub void_wait_daa_sum: u64,
    pub void_wait_n: u64,
    pub expected_attempts_q32_sum: u128,
    pub network_expected_attempts_q32_sum: u128,
    pub first_accepted_daa: u64,
    pub last_accepted_daa: u64,
}

impl PalwClassLedgerTotalsV1 {
    fn rate(paid: u128, compute: u128) -> u128 {
        if compute == 0 { 0 } else { paid.saturating_mul(PALW_LEDGER_RATE_SCALE_V1) / compute }
    }
    /// `producer_actual_msk / producer_attempted_ccu` (sompi per 10⁹ MAC-eq).
    pub fn producer_per_attempted_compute(&self) -> u128 {
        Self::rate(self.producer_paid_sompi, self.attempted_compute)
    }
    /// `panel_actual_msk / panel_verification_ccu`.
    pub fn panel_per_verification_compute(&self) -> u128 {
        Self::rate(self.panel_paid_sompi, self.verification_compute)
    }
    /// `total_actual_msk / total_attempted_ccu` — the producer's and the panel's pay over what both ran.
    pub fn total_per_attempted_compute(&self) -> u128 {
        Self::rate(self.producer_paid_sompi + self.panel_paid_sompi, self.attempted_compute + self.verification_compute)
    }
    pub fn total_per_final_compute(&self) -> u128 {
        Self::rate(self.producer_paid_sompi + self.panel_paid_sompi, self.final_compute)
    }
    /// `accepted → licensed`, in permille of the claims seen.
    pub fn licence_rate_permille(&self) -> u32 {
        if self.claims == 0 { 0 } else { (self.licensed as u128 * 1000 / self.claims as u128) as u32 }
    }
    /// `licensed → Final`, in permille of the claims that licensed.
    pub fn final_of_licensed_permille(&self) -> u32 {
        if self.licensed == 0 { 0 } else { (self.finals as u128 * 1000 / self.licensed as u128) as u32 }
    }
    /// `Final` among the terminal claims, in permille.
    pub fn final_rate_permille(&self) -> u32 {
        let terminal = self.finals + self.voided;
        if terminal == 0 { 0 } else { (self.finals as u128 * 1000 / terminal as u128) as u32 }
    }
    fn avg(sum: u64, n: u64) -> u64 {
        if n == 0 { 0 } else { sum / n }
    }
    pub fn avg_bind_wait_daa(&self) -> u64 {
        Self::avg(self.bind_wait_daa_sum, self.bind_wait_n)
    }
    pub fn avg_licence_wait_daa(&self) -> u64 {
        Self::avg(self.licence_wait_daa_sum, self.licence_wait_n)
    }
    pub fn avg_final_wait_daa(&self) -> u64 {
        Self::avg(self.final_wait_daa_sum, self.final_wait_n)
    }
    pub fn avg_void_wait_daa(&self) -> u64 {
        Self::avg(self.void_wait_daa_sum, self.void_wait_n)
    }
    pub fn avg_expected_attempts_q32(&self) -> u128 {
        if self.claims == 0 { 0 } else { self.expected_attempts_q32_sum / self.claims as u128 }
    }
    pub fn avg_network_expected_attempts_q32(&self) -> u128 {
        if self.claims == 0 { 0 } else { self.network_expected_attempts_q32_sum / self.claims as u128 }
    }
    /// `producer + panel + reserve + burned == Σ escrow of the Finals` — the identity every window
    /// must satisfy (ADR-0124 Decision 8: nothing minted, nothing lost).
    pub fn emission_identity_holds(&self) -> bool {
        self.producer_paid_sompi + self.panel_paid_sompi + self.reserve_sompi + self.burned_sompi == self.escrow_final_sompi
    }
}

/// [`PalwClassLedgerTotalsV1`] of `class_id` over `rows`.
pub fn palw_class_ledger_totals_v1<'a>(
    rows: impl IntoIterator<Item = &'a PalwClaimLedgerRowV1>,
    class_id: Hash64,
) -> PalwClassLedgerTotalsV1 {
    let mut t = PalwClassLedgerTotalsV1 { class_id, first_accepted_daa: u64::MAX, ..Default::default() };
    for row in rows.into_iter().filter(|r| r.class_id == class_id) {
        t.claims += 1;
        t.first_accepted_daa = t.first_accepted_daa.min(row.accepted_daa);
        t.last_accepted_daa = t.last_accepted_daa.max(row.accepted_daa);
        t.expected_attempts_q32_sum = t.expected_attempts_q32_sum.saturating_add(row.expected_attempts_q32);
        t.network_expected_attempts_q32_sum = t.network_expected_attempts_q32_sum.saturating_add(row.network_expected_attempts_q32);
        t.attempted_compute = t.attempted_compute.saturating_add(palw_ledger_row_attempted_compute_v1(row));
        if row.paid_at_acceptance {
            t.paid_at_acceptance += 1;
        }
        if row.rebound_daa.is_some() {
            t.redrawn += 1;
        }
        let ever_bound = row.bound_daa.is_some() || row.licensed_daa.is_some() || row.final_daa.is_some() || row.seats > 0;
        if ever_bound {
            t.bound += 1;
            t.verification_compute = t.verification_compute.saturating_add(row.draw_compute.saturating_mul(row.seats.max(1) as u128));
            if let Some(bound) = row.bound_daa {
                t.bind_wait_daa_sum += bound.saturating_sub(row.accepted_daa);
                t.bind_wait_n += 1;
            }
        }
        if row.licensed_daa.is_some() || row.final_daa.is_some() {
            t.licensed += 1;
            if let (Some(bound), Some(licensed)) = (row.bound_daa, row.licensed_daa) {
                t.licence_wait_daa_sum += licensed.saturating_sub(bound);
                t.licence_wait_n += 1;
            }
        }
        if let Some(final_daa) = row.final_daa {
            t.finals += 1;
            t.final_compute = t.final_compute.saturating_add(row.draw_compute);
            t.final_wait_daa_sum += final_daa.saturating_sub(row.accepted_daa);
            t.final_wait_n += 1;
            t.escrow_final_sompi += row.escrow_sompi as u128;
            t.producer_paid_sompi += row.producer_paid_sompi as u128;
            t.panel_paid_sompi += row.panel_paid_sompi as u128;
            t.reserve_sompi += row.reserve_sompi as u128;
            t.burned_sompi += row.burned_sompi as u128;
        }
        if let Some(voided) = row.voided_daa {
            t.voided += 1;
            t.void_wait_daa_sum += voided.saturating_sub(row.accepted_daa);
            t.void_wait_n += 1;
        }
    }
    if t.claims == 0 {
        t.first_accepted_daa = 0;
    }
    t
}

/// **Proposal C of ADR-0132, as a shadow price**: a claim is paid `min(escrow, attempted_ccu × rate)`
/// with `rate` in sompi per 10⁹ MAC-eq — a constant, not a class. A model heavier than `escrow /
/// rate` is paid its escrow whole; adding it changes no other class's pay.
pub fn palw_rate_priced_reward_v1(escrow_sompi: u64, attempted_compute: u128, rate_sompi_per_giga: u128) -> u64 {
    let priced = attempted_compute.saturating_mul(rate_sompi_per_giga) / PALW_LEDGER_RATE_SCALE_V1;
    priced.min(escrow_sompi as u128) as u64
}

/// The gap between two classes' rates, `max / min − 1` in permille; `None` where either is zero.
pub fn palw_ledger_gap_permille_v1(a: u128, b: u128) -> Option<u128> {
    palw_gap_permille_v1(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_economic_compute_v1::{
        PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, palw_expected_attempts_q32_v1, palw_network_expected_attempts_q32_v1,
    };
    use crate::palw_panel_economy_v1::palw_panel_split_v1;
    use crate::tx::TransactionOutpoint;

    const ESCROW_PRE: u64 = 275_628_448_680;
    const ESCROW_6001: u64 = 320_084_650_080;
    const DENSE_DRAW: u128 = 83_102_171_136;
    const HYBRID_DRAW: u128 = 18_055_200_736;
    const DENSE_LEAVES: u64 = 6_630_544;
    const HYBRID_LEAVES: u64 = 2_685_360;
    const UNIT_27B: u64 = 9_000_776;
    const ONE: u128 = PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(n), 0))
    }
    fn obs(claim: u64, class: u64, accepted: u64, escrow: u64) -> PalwClaimLedgerObservationV1 {
        PalwClaimLedgerObservationV1 {
            claim_id: h(claim),
            class_id: h(class),
            producer_bond: bond(1),
            accepted_daa: accepted,
            accepted_block: h(1_000 + claim),
            escrow_sompi: escrow,
            pwu: 1,
            rebound_daa: None,
            bound_daa: None,
            licensed_daa: None,
            final_daa: None,
            voided_daa: None,
            void_reason: None,
            seats: 0,
            credited_seats: 0,
            economics: None,
        }
    }
    fn dense_facts(net_bits: u32) -> PalwLedgerClassFactsV1 {
        PalwLedgerClassFactsV1 {
            expected_attempts_q32: palw_expected_attempts_q32_v1(227_593_162_155_002_462_664_823_364_924_468_699_761),
            network_expected_attempts_q32: palw_network_expected_attempts_q32_v1(net_bits),
            draw_compute: DENSE_DRAW,
            leaves: DENSE_LEAVES,
        }
    }
    fn hybrid_facts(net_bits: u32) -> PalwLedgerClassFactsV1 {
        PalwLedgerClassFactsV1 {
            expected_attempts_q32: palw_expected_attempts_q32_v1(339_585_066_988_723_425_628_408_675_859_162_129_054),
            network_expected_attempts_q32: palw_network_expected_attempts_q32_v1(net_bits),
            draw_compute: HYBRID_DRAW,
            leaves: HYBRID_LEAVES,
        }
    }
    fn rule_pre() -> PalwLedgerPayoutRuleV1 {
        PalwLedgerPayoutRuleV1 { work_priced: false, leaves: 0, unit_leaves: 0, panel_economy: false, economic: None }
    }
    fn rule_6001(leaves: u64) -> PalwLedgerPayoutRuleV1 {
        PalwLedgerPayoutRuleV1 { work_priced: true, leaves, unit_leaves: UNIT_27B, panel_economy: true, economic: None }
    }

    /// A window of `n` claims for one class: `finals` reach `Final` (bound at +20, licensed at
    /// +40, Final at +2,000), `voided` time out at +1,242 after a redraw, the rest stay bound.
    #[allow(clippy::too_many_arguments)]
    fn window(
        class: u64,
        first_claim: u64,
        n: u64,
        finals: u64,
        voided: u64,
        escrow: u64,
        facts: PalwLedgerClassFactsV1,
        rule: PalwLedgerPayoutRuleV1,
        seats: u16,
        credited: u16,
    ) -> Vec<PalwClaimLedgerRowV1> {
        (0..n)
            .map(|i| {
                let accepted = 4_000 + i;
                let mut o = obs(first_claim + i, class, accepted, escrow);
                o.bound_daa = Some(accepted + 20);
                o.seats = seats;
                o.credited_seats = credited;
                let bound = palw_ledger_merge_v1(None, &o, accepted + 21, facts, |_| rule);
                let mut later = o.clone();
                if i < finals {
                    later.bound_daa = None;
                    later.licensed_daa = Some(accepted + 40);
                    let licensed = palw_ledger_merge_v1(Some(&bound), &later, accepted + 41, facts, |_| rule);
                    let mut fin = later.clone();
                    fin.licensed_daa = None;
                    fin.final_daa = Some(accepted + 2_000);
                    fin.seats = 0;
                    fin.credited_seats = 0;
                    palw_ledger_merge_v1(Some(&licensed), &fin, accepted + 2_001, facts, |_| rule)
                } else if i < finals + voided {
                    later.bound_daa = None;
                    later.rebound_daa = Some(accepted + 621);
                    later.voided_daa = Some(accepted + 1_242);
                    later.void_reason = Some(PalwVoidReasonV2::ReceiptTimeout);
                    later.seats = 0;
                    palw_ledger_merge_v1(Some(&bound), &later, accepted + 1_243, facts, |_| rule)
                } else {
                    bound
                }
            })
            .collect()
    }

    /// **The payout is the rule in force at the `Final`.** Below the fences the producer is named
    /// the escrow whole and no seat is paid; past them the class's leaves against the unit price the
    /// reward, the pool is a fifth of it, each credited seat a fifth of the pool, the rest the
    /// reserve's, and the priced-away escrow is burned. `producer + panel + reserve + burned ==
    /// escrow` either way; the floor (unpriced) keeps its escrow whole past the fence too.
    #[test]
    fn adr0132_the_payout_is_the_rule_in_force_at_the_final() {
        let pre = palw_ledger_payout_v1(ESCROW_PRE, rule_pre(), 5, 3);
        assert_eq!(pre, PalwLedgerPayoutV1 { producer_sompi: ESCROW_PRE, ..Default::default() });
        let dense = palw_ledger_payout_v1(ESCROW_6001, rule_6001(DENSE_LEAVES), 5, 3);
        let reward = palw_priced_reward_u128_v1(ESCROW_6001, DENSE_LEAVES as u128, UNIT_27B as u128);
        let split = palw_panel_split_v1(reward, 5, 3);
        assert_eq!(dense.producer_sompi, split.producer);
        assert_eq!(dense.panel_sompi, split.per_seat * 3);
        assert_eq!(dense.reserve_sompi, split.per_seat * 2 + (reward / 5 - split.per_seat * 5));
        assert_eq!(dense.burned_sompi, ESCROW_6001 - reward);
        assert_eq!(dense.total(), ESCROW_6001);
        assert!(dense.producer_sompi * 100 / ESCROW_6001 >= 58 && dense.producer_sompi * 100 / ESCROW_6001 <= 59, "73.7 % × 80 %");
        let floor = palw_ledger_payout_v1(
            ESCROW_6001,
            PalwLedgerPayoutRuleV1 { work_priced: false, panel_economy: true, ..rule_6001(7_708) },
            5,
            5,
        );
        assert_eq!(floor.burned_sompi, 0);
        assert_eq!(floor.total(), ESCROW_6001);
        assert_eq!(floor.reserve_sompi, ESCROW_6001 / 5 - (ESCROW_6001 / 5 / 5) * 5, "five credited seats: only the dust is reserve");
    }

    /// **A row keeps what it first saw and fills what it later sees, once.** The class facts and
    /// the accepted block are the first sight's; a bind, a redraw, a licence and a Final each set
    /// their mark the first time; the payout is derived at the first sight of the `Final` and a
    /// later read (where the duty row is gone) does not re-derive it; an escrow of zero is a
    /// claim paid at acceptance.
    #[test]
    fn adr0132_a_row_keeps_first_sight_and_fills_marks_once() {
        let facts = dense_facts(0x207f_ffff);
        let mut o = obs(7, 1, 4_500, ESCROW_6001);
        let row = palw_ledger_merge_v1(None, &o, 4_501, facts, |_| rule_6001(DENSE_LEAVES));
        assert_eq!((row.first_seen_daa, row.bound_daa, row.producer_paid_sompi), (4_501, None, 0));
        o.bound_daa = Some(4_520);
        o.seats = 5;
        o.credited_seats = 2;
        let other_facts = PalwLedgerClassFactsV1 { draw_compute: 1, ..facts };
        let row = palw_ledger_merge_v1(Some(&row), &o, 4_530, other_facts, |_| rule_6001(DENSE_LEAVES));
        assert_eq!(row.draw_compute, DENSE_DRAW, "first sight's facts stay");
        assert_eq!(row.bound_daa, Some(4_520));
        o.bound_daa = None;
        o.licensed_daa = Some(4_560);
        o.credited_seats = 3;
        let row = palw_ledger_merge_v1(Some(&row), &o, 4_561, facts, |_| rule_6001(DENSE_LEAVES));
        assert_eq!((row.bound_daa, row.licensed_daa, row.credited_seats), (Some(4_520), Some(4_560), 3));
        o.licensed_daa = None;
        o.final_daa = Some(6_500);
        o.seats = 0;
        o.credited_seats = 0;
        let row = palw_ledger_merge_v1(Some(&row), &o, 6_501, facts, |at| {
            assert_eq!(at, 6_500, "the rule is read at the Final's height");
            rule_6001(DENSE_LEAVES)
        });
        let expected = palw_ledger_payout_v1(ESCROW_6001, rule_6001(DENSE_LEAVES), 5, 3);
        assert_eq!((row.producer_paid_sompi, row.panel_paid_sompi), (expected.producer_sompi, expected.panel_sompi));
        assert_eq!(row.seats, 5, "the duty row's size survives the Final that dropped it");
        let again = palw_ledger_merge_v1(Some(&row), &o, 6_600, facts, |_| panic!("derived once"));
        assert_eq!(again.producer_paid_sompi, row.producer_paid_sompi);
        assert_eq!(again.last_seen_daa, 6_600);
        let merged = palw_ledger_merge_v1(None, &obs(8, 1, 4_500, 0), 4_501, facts, |_| rule_pre());
        assert!(merged.paid_at_acceptance);
        let mut voided = obs(9, 1, 4_500, ESCROW_6001);
        voided.voided_daa = Some(5_742);
        voided.void_reason = Some(PalwVoidReasonV2::ReceiptTimeout);
        voided.rebound_daa = Some(5_121);
        let row = palw_ledger_merge_v1(None, &voided, 5_743, facts, |_| rule_pre());
        assert_eq!((row.voided_daa, row.rebound_daa, row.void_reason.as_str()), (Some(5_742), Some(5_121), "receipt_timeout"));
    }

    /// **The actual rates are a property of each class, not of the mix, and the emission identity
    /// holds in every mix.** Dense 100/0, 0/100, 50/50, 90/10 and 10/90 against the hybrid at the
    /// live Final rates (13 % of terminal claims; 0 for the hybrid) under the 6,001 rule: the dense
    /// tier's `producer / attempted` is the same number in every mix, the hybrid's is zero in every
    /// mix, and `producer + panel + reserve + burned == Σ escrow of the Finals` for each class.
    #[test]
    fn adr0132_the_actual_rates_are_the_classes_not_the_mixes() {
        let bits = 0x207f_ffff;
        let mut dense_rate = None;
        for (n25, n36) in [(1_000u64, 0u64), (0, 1_000), (500, 500), (900, 100), (100, 900)] {
            let dense =
                window(1, 1, n25, n25 * 13 / 100, n25 * 87 / 100, ESCROW_6001, dense_facts(bits), rule_6001(DENSE_LEAVES), 5, 3);
            let hybrid = window(2, 10_000, n36, 0, 0, ESCROW_6001, hybrid_facts(bits), rule_6001(HYBRID_LEAVES), 5, 0);
            let rows: Vec<_> = dense.into_iter().chain(hybrid).collect();
            let t25 = palw_class_ledger_totals_v1(&rows, h(1));
            let t36 = palw_class_ledger_totals_v1(&rows, h(2));
            assert!(t25.emission_identity_holds() && t36.emission_identity_holds());
            assert_eq!(t36.producer_per_attempted_compute(), 0, "no Final, nothing paid, whatever the mix");
            assert_eq!(t36.panel_per_verification_compute(), 0);
            if n25 > 0 {
                let r = t25.producer_per_attempted_compute();
                assert!(r > 0);
                match dense_rate {
                    None => dense_rate = Some(r),
                    Some(prev) => assert!(r.abs_diff(prev) * 1_000 <= prev, "{r} vs {prev}"),
                }
                assert_eq!(t25.final_rate_permille(), 130);
                assert_eq!(t25.licence_rate_permille(), 130, "in this window only the Finals licensed");
                assert_eq!(t25.avg_bind_wait_daa(), 20);
                assert_eq!(t25.avg_final_wait_daa(), 2_000);
                assert_eq!(t25.avg_void_wait_daa(), 1_242);
                assert_eq!(t25.redrawn, n25 * 87 / 100);
                assert!(t25.verification_compute >= t25.attempted_compute * 5 / 8, "five seats replay every bound claim's job");
            }
        }
        // The rate itself: 13 % × 73.7 % × 80 % of the escrow, over 1.495 × 2.0 × 83.1 G.
        let paid_per_claim = 130 * (ESCROW_6001 as u128 * DENSE_LEAVES as u128 / UNIT_27B as u128) * 8 / 10 / 1000;
        let attempted = palw_ledger_row_attempted_compute_v1(
            &window(1, 1, 1, 0, 0, ESCROW_6001, dense_facts(bits), rule_6001(DENSE_LEAVES), 5, 3)[0],
        );
        let expected = paid_per_claim * PALW_LEDGER_RATE_SCALE_V1 / attempted;
        let got = dense_rate.unwrap();
        assert!(got.abs_diff(expected) * 100 <= expected, "≈ {expected} sompi per 10⁹ MAC-eq, got {got}");
    }

    /// **A Final-rate gap and a licence-rate gap move the actual rates and nothing in the price.**
    /// Same price, same compute: the class that finalizes twice as often is paid twice as much per
    /// forward; a class whose licences never finalize is paid nothing; the licence and Final rates
    /// read the difference where it is.
    #[test]
    fn adr0132_a_final_rate_gap_is_read_as_pay_not_price() {
        let facts = dense_facts(0x207f_ffff);
        let a = window(1, 1, 200, 26, 174, ESCROW_6001, facts, rule_6001(DENSE_LEAVES), 5, 3);
        let b = window(2, 1_000, 200, 52, 148, ESCROW_6001, facts, rule_6001(DENSE_LEAVES), 5, 3);
        let (ta, tb) = (palw_class_ledger_totals_v1(&a, h(1)), palw_class_ledger_totals_v1(&b, h(2)));
        assert_eq!(ta.attempted_compute, tb.attempted_compute, "the same forwards were run");
        assert_eq!(tb.producer_paid_sompi, 2 * ta.producer_paid_sompi);
        assert_eq!(palw_ledger_gap_permille_v1(ta.producer_per_attempted_compute(), tb.producer_per_attempted_compute()), Some(1_000));
        assert_eq!((ta.final_rate_permille(), tb.final_rate_permille()), (130, 260));
        // Licensed but never Final: a licence rate without a Final rate, and no pay.
        let mut licensed_only = window(3, 2_000, 100, 0, 0, ESCROW_6001, facts, rule_6001(DENSE_LEAVES), 5, 3);
        for row in licensed_only.iter_mut().take(60) {
            row.licensed_daa = Some(row.accepted_daa + 40);
        }
        let tc = palw_class_ledger_totals_v1(&licensed_only, h(3));
        assert_eq!((tc.licence_rate_permille(), tc.final_of_licensed_permille(), tc.producer_paid_sompi), (600, 0, 0));
    }

    /// **The class target and the network's `bits` move attempted compute, never pay.** The dense
    /// tier at its live target (1.495 draws) against the same class at MAX (1.0): the same pay,
    /// 1.495 × the attempted compute, so the rate drops by that factor; the difficulty floor
    /// (2.0 network draws) against a target four times tighter: the same again.
    #[test]
    fn adr0132_targets_and_bits_move_attempted_compute_not_pay() {
        let at_max = PalwLedgerClassFactsV1 { expected_attempts_q32: ONE, ..dense_facts(0x207f_ffff) };
        let a = window(1, 1, 100, 13, 87, ESCROW_6001, dense_facts(0x207f_ffff), rule_6001(DENSE_LEAVES), 5, 3);
        let b = window(1, 1_000, 100, 13, 87, ESCROW_6001, at_max, rule_6001(DENSE_LEAVES), 5, 3);
        let (ta, tb) = (palw_class_ledger_totals_v1(&a, h(1)), palw_class_ledger_totals_v1(&b, h(1)));
        assert_eq!(ta.producer_paid_sompi, tb.producer_paid_sompi);
        assert!(
            ta.attempted_compute * 1000 / tb.attempted_compute >= 1_494 && ta.attempted_compute * 1000 / tb.attempted_compute <= 1_496
        );
        assert!(tb.producer_per_attempted_compute() > ta.producer_per_attempted_compute());
        assert_eq!(ta.avg_expected_attempts_q32() * 1000 / ONE, 1_495);
        let tight =
            window(1, 2_000, 100, 13, 87, ESCROW_6001, dense_facts(0x207f_ffff / 4 * 4 - 0x0100_0000), rule_6001(DENSE_LEAVES), 5, 3);
        let tt = palw_class_ledger_totals_v1(&tight, h(1));
        assert_eq!(tt.producer_paid_sompi, ta.producer_paid_sompi);
        assert!(tt.attempted_compute > ta.attempted_compute, "a tighter network target costs more forwards");
        assert!(tt.avg_network_expected_attempts_q32() > ta.avg_network_expected_attempts_q32());
    }

    /// **A class no panel can run is paid nothing and verifies nothing** — and an operator outage
    /// reads as the licence rate it costs, not as a price. Rows that never bind (no drawable panel)
    /// hold no verification compute; rows bound to seats that never credit (every seat `Incapable`
    /// or silent) hold five replays' worth and no licence; one operator down among five seats is a
    /// licence rate of three-fifths against two, which is the number the ledger shows.
    #[test]
    fn adr0132_no_capable_panel_pays_nothing_and_an_outage_is_a_licence_rate() {
        let facts = hybrid_facts(0x207f_ffff);
        let never_bound: Vec<_> = (0..50)
            .map(|i| palw_ledger_merge_v1(None, &obs(i, 2, 4_000 + i, ESCROW_6001), 4_001 + i, facts, |_| rule_pre()))
            .collect();
        let t = palw_class_ledger_totals_v1(&never_bound, h(2));
        assert_eq!((t.bound, t.verification_compute, t.producer_paid_sompi, t.licence_rate_permille()), (0, 0, 0, 0));
        assert!(t.attempted_compute > 0, "the producer still ran the forwards");
        let bound_incapable = window(2, 100, 50, 0, 0, ESCROW_6001, facts, rule_6001(HYBRID_LEAVES), 5, 0);
        let t = palw_class_ledger_totals_v1(&bound_incapable, h(2));
        assert_eq!(t.verification_compute, 50 * 5 * HYBRID_DRAW, "five seats each replay — if they can");
        assert_eq!((t.licensed, t.producer_paid_sompi), (0, 0));
        let healthy = window(1, 200, 100, 60, 40, ESCROW_6001, dense_facts(0x207f_ffff), rule_6001(DENSE_LEAVES), 5, 3);
        let outage = window(1, 400, 100, 36, 64, ESCROW_6001, dense_facts(0x207f_ffff), rule_6001(DENSE_LEAVES), 5, 2);
        let (th, to) = (palw_class_ledger_totals_v1(&healthy, h(1)), palw_class_ledger_totals_v1(&outage, h(1)));
        assert_eq!((th.licence_rate_permille(), to.licence_rate_permille()), (600, 360));
        assert!(to.producer_per_attempted_compute() * 1000 / th.producer_per_attempted_compute() == 600);
        assert!(to.panel_paid_sompi < th.panel_paid_sompi, "two credited seats of five are paid, three go to reserve");
        assert!(to.emission_identity_holds());
    }

    /// **A heavier model moves the unit, never a rate** (ADR-0132 proposals C against the unit
    /// rule). Under today's unit the 27B's registration lowers the dense tier's pay from the whole
    /// escrow to 73.7 %; under `min(escrow, attempted × rate)` the dense tier is paid the same
    /// before and after, and the heavy model is capped at its escrow — it signals a re-pin, it does
    /// not tax its neighbours. A model lighter than both is priced proportionally under either.
    #[test]
    fn adr0132_a_heavier_model_moves_the_unit_but_not_a_rate() {
        let dense_unit_before = palw_priced_reward_u128_v1(ESCROW_6001, DENSE_LEAVES as u128, DENSE_LEAVES as u128);
        let dense_unit_after = palw_priced_reward_u128_v1(ESCROW_6001, DENSE_LEAVES as u128, UNIT_27B as u128);
        assert_eq!(dense_unit_before, ESCROW_6001);
        assert!(dense_unit_after * 1000 / ESCROW_6001 == 736, "the 1 ‰ registration cut the dense tier to 73.7 %");
        let dense_attempted = palw_ledger_row_attempted_compute_v1(
            &window(1, 1, 1, 0, 0, ESCROW_6001, dense_facts(0x207f_ffff), rule_6001(DENSE_LEAVES), 5, 3)[0],
        );
        let rate = ESCROW_6001 as u128 * PALW_LEDGER_RATE_SCALE_V1 / dense_attempted;
        let dense_rate_before = palw_rate_priced_reward_v1(ESCROW_6001, dense_attempted, rate);
        let heavy_attempted = dense_attempted * 7;
        let heavy = palw_rate_priced_reward_v1(ESCROW_6001, heavy_attempted, rate);
        let dense_rate_after = palw_rate_priced_reward_v1(ESCROW_6001, dense_attempted, rate);
        assert_eq!(dense_rate_before, dense_rate_after, "a rate is a constant: the neighbour's pay does not move");
        assert_eq!(heavy, ESCROW_6001, "the heavy model is capped at the escrow, paid whole");
        // The rate is floored to a sompi per 10⁹ MAC-eq, so the priced escrow is short by at most
        // `attempted / 10⁹` sompi — some 250 on a 2.5 × 10¹¹ MAC-eq claim.
        assert!(dense_rate_before >= ESCROW_6001 - (dense_attempted / PALW_LEDGER_RATE_SCALE_V1) as u64 - 1, "{dense_rate_before}");
        let light = palw_rate_priced_reward_v1(ESCROW_6001, dense_attempted / 4, rate);
        assert!(
            light * 1000 / ESCROW_6001 >= 249 && light * 1000 / ESCROW_6001 <= 250,
            "a quarter of the compute, a quarter of the escrow"
        );
        assert_eq!(palw_rate_priced_reward_v1(ESCROW_6001, 0, rate), 0);
    }

    /// The void reasons print under the `getPalwClaims` spelling.
    #[test]
    fn adr0132_void_reasons_print_under_the_claims_spelling() {
        assert_eq!(palw_void_reason_name_v1(&PalwVoidReasonV2::BindTimeout), "bind_timeout");
        assert_eq!(palw_void_reason_name_v1(&PalwVoidReasonV2::ReceiptTimeout), "receipt_timeout");
        assert_eq!(palw_void_reason_name_v1(&PalwVoidReasonV2::CourtFraud), "court_fraud");
        assert_eq!(palw_void_reason_name_v1(&PalwVoidReasonV2::ProducerWithholding), "producer_withholding");
    }

    /// **ADR-0132 Upgrade C: a snapshotted claim's `Final` is priced by its snapshot, whatever the
    /// fences say at the `Final`'s height** — the fold's rule, mirrored — and the row keeps the
    /// chain's numbers over the recorder's reading of the class.
    #[test]
    fn adr0132_a_snapshotted_final_is_priced_by_its_snapshot_whatever_the_fences_say() {
        use crate::palw_economic_payout_v1::PalwClaimEconomicsV1;
        let snapshot = PalwClaimEconomicsV1 {
            draw_ccu: 800_000,
            verification_ccu: 1_000_000,
            seat_count: 5,
            expected_attempts_q32: 2 * ONE,
            network_expected_attempts_q32: ONE,
            rate_sompi_per_giga: 100_000_000,
            panel_share_permille: 300,
        };
        let mut o = obs(1, 2, 4_000, 620_000);
        o.economics = Some(snapshot);
        o.seats = 1;
        o.credited_seats = 1;
        let facts = PalwLedgerClassFactsV1 {
            expected_attempts_q32: 7 * ONE,
            network_expected_attempts_q32: 3 * ONE,
            draw_compute: 1,
            leaves: 5,
        };
        let row = palw_ledger_merge_v1(None, &o, 4_001, facts, |_| rule_6001(5));
        assert!(row.economic_snapshotted);
        assert_eq!((row.expected_attempts_q32, row.network_expected_attempts_q32, row.draw_compute), (2 * ONE, ONE, 800_000));
        assert_eq!(palw_ledger_row_attempted_compute_v1(&row), 1_600_000);
        // The Final, seen after the chain dropped the snapshot: 160 000 at a 30 % panel share, while
        // the rule at the height says the work price and the fifth.
        let mut fin = obs(1, 2, 4_000, 620_000);
        fin.final_daa = Some(4_100);
        fin.seats = 1;
        fin.credited_seats = 1;
        let paid = palw_ledger_merge_v1(Some(&row), &fin, 4_101, facts, |_| rule_6001(5));
        assert_eq!(
            (paid.producer_paid_sompi, paid.panel_paid_sompi, paid.reserve_sompi, paid.burned_sompi),
            (112_000, 48_000, 0, 460_000),
            "min(620 000, 1.6 M × 0.1) = 160 000: 112 000 to the producer, 48 000 to the one seat, 460 000 never named"
        );
        // A row that never saw a snapshot is priced by the rule at the height, as before.
        let plain = palw_ledger_merge_v1(None, &fin, 4_101, facts, |_| rule_6001(5));
        assert!(!plain.economic_snapshotted);
        let expected = palw_ledger_payout_v1(620_000, rule_6001(5), 1, 1);
        assert_eq!((plain.producer_paid_sompi, plain.panel_paid_sompi), (expected.producer_sompi, expected.panel_sompi));
        // The rule itself, over a five-seat panel with three credited: the pool is 48 000, a seat 9 600.
        let rule = PalwLedgerPayoutRuleV1 {
            economic: Some(PalwLedgerEconomicRuleV1 {
                attempted_ccu: 1_600_000,
                rate_sompi_per_giga: 100_000_000,
                panel_share_permille: 300,
            }),
            ..rule_6001(5)
        };
        let out = palw_ledger_payout_v1(620_000, rule, 5, 3);
        assert_eq!((out.producer_sompi, out.panel_sompi, out.reserve_sompi, out.burned_sompi), (112_000, 28_800, 19_200, 460_000));
        assert_eq!(out.total(), 620_000);
    }
}
