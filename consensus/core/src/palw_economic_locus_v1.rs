//! **The economic locus census — what one answer costs when it is ONE claim and when it is N**
//! (ADR-0080 Decision 3, narrowed by measurement).
//!
//! ADR-0080 proposed cutting one long inference into N claims and asserted, as its Decision 3,
//! that "reward, ticket, weight and quanta are invariant under segmentation … splitting is
//! weight-neutral by construction". The MECHANISM was refuted before a line of it was
//! implemented (see the status block of `docs/adr/0080-the-answer-is-long-the-verified-unit-is-short.md`),
//! but the invariant survives as the REQUIREMENT any successor must satisfy — including a
//! successor that chunks the reductions in step space rather than the close, which is a different
//! design that meets the same bill.
//!
//! This module is that requirement made executable. It changes no rule, reads no state and is
//! called by no consensus path: it is a doc comment with a test suite attached, so that a designer
//! reaching for "just use more claims" pays for the discovery with one `cargo test` instead of two
//! ADRs.
//!
//! # What the census found, and where Decision 3 was too generous
//!
//! Decision 3 said the per-leaf quantities are invariant. They are — **inside a band**. Below the
//! band a segment rounds to zero work and is REFUSED; above it a claim saturates, so segmentation
//! *manufactures* quanta the single claim was denied. Neither edge was in the ADR, and the upper
//! one is the direction that matters: it is the "free money" Decision 3 was written to forbid,
//! reachable not by a bug but by the shipped cap.
//!
//! ## Per-LEAF (invariant under restructuring — inside `[quantum, cap · quantum]`)
//!
//! | quantity | where it is derived | why it is per-leaf |
//! |---|---|---|
//! | the quantum | [`crate::palw_freeprompt_v3::fp_class_quantum_leaves_v1`] (`palw_freeprompt_v3.rs:379`) | `max(1, pwu_per_inference / quanta_per_canonical_job)` — a fraction of the CLASS's canonical job, so it names no claim and no segment |
//! | the quanta | [`crate::palw_freeprompt_v3::fp_quanta_v3`] (`palw_freeprompt_v3.rs:392`) | `min(⌊work_leaves / quantum⌋, cap)` — leaves in, draws out |
//! | the lottery entries | [`crate::palw_freeprompt_v3::fp_quantum_ticket_v3`] (`palw_freeprompt_v3.rs:427`) | one draw per `(claim, quantum_index)`, so the COUNT of draws is the count of quanta |
//! | the weight a spend adds | `apply_receipt_spend`'s `per_quantum = claim.pwu / quanta` (`palw_state_v2.rs:6921`) | `pwu = quanta × quantum` exactly, so the claim's whole weight is its leaves, floored to the quantum |
//!
//! ## Per-CLAIM or per-BLOCK (NOT invariant — this is the load-bearing half)
//!
//! | quantity | value | where | what N segments cost |
//! |---|---|---|---|
//! | attempt-lane exposure | flat `pwu_per_inference × slash_value_per_pwu` | [`crate::palw_state_v2::palw_exposure_pwu_v1`] (`palw_state_v2.rs:1311`), spent at `apply_attempt` (`:6987`) | N × one reservation; the function has no leaf argument at all |
//! | free-prompt exposure | `quanta × quantum × slash_value_per_pwu` | `apply_free_prompt`'s `reserved` (`palw_state_v2.rs:6831`) | per-leaf **until the cap**, then flat — so above `cap · quantum` leaves, N segments reserve N × the ceiling |
//! | **challenger** exposure to open a court | the claim's own `reserved`, again | `apply_court_opened` (`palw_state_v2.rs:6674`) | N reservations to PROSECUTE one answer. *"the ceiling it already lives under does the counting"* — so a bond that could challenge M claims can challenge only M/N segmented answers, and segmentation raises the price of policing as fast as it raises the price of producing |
//! | panel seats | [`crate::palw_fp_devnet_v3::PALW_V2_PANEL_SEATS`] = 5 | `palw_fp_devnet_v3.rs:459` | N panels drawn, N × 5 seats put on duty |
//! | receipts to quorum | [`crate::palw_fp_devnet_v3::PALW_V2_PANEL_QUORUM`] = 3 | `palw_fp_devnet_v3.rs:460`, enforced by [`crate::palw_panel_v2::validate_receipt_quorum_v2`] | N × 3 ML-DSA-87 signatures on chain, at [`crate::dns_finality::STAKE_ATTESTATION_SIG_LEN`] = 4,627 bytes each |
//! | seat replay work | [`crate::palw_fp_interval_v1::PALW_FP_SEAT_INTERVAL_SAMPLES_V1`] = 4 intervals **per seat per claim** | `palw_fp_interval_v1.rs:28` | N × 5 × 4 interval replays for one answer. The draw is short-circuited only when `interval_count <= k`, so a short segment is checked WHOLE — segmentation buys denser sampling and pays the panel's CPU for it |
//! | derivations | [`crate::palw_derived_v1::PALW_DERIVED_MAX_PER_CLAIM`] = 4 | `palw_derived_v1.rs:36` | the ceiling is per claim, but a `PalwDerivedArtifactV1` names a SINGULAR `claim_id`/`output_root` — N × 4 slots that no derivation can span |
//! | abandon hold | `fp_abandon_hold` = 600 DAA (RC) | `palw_fp_devnet_v3.rs:100`, read by [`crate::palw_state_v2::palw_claim_is_on_abandon_hold_v2`] (`:1070`) | N reservations held past the void, not one |
//! | exposure lifetime | `max_claim_exposure_daa` = 7,200 DAA (RC) | [`crate::palw_fp_devnet_v3::PalwLatticeWindowsV1::max_claim_exposure_daa`] (`palw_fp_devnet_v3.rs:82`) | the segments of one answer are CONCURRENT, so the bond must carry N reservations at once against one `fp_max_exposure_ratio_permille` ceiling |
//! | payout queue rows | one `pending_payouts` row per finalized claim | [`crate::palw_state_v2::PalwChainStateV2`] `pending_payouts` (`palw_state_v2.rs:2866`), written by `write_payout` (`:4325`) | N rows, and unlike the four rebuildable indices this one IS hashed into `state_root` (`:3213`) |
//! | payout drain rate | [`crate::palw_state_v2::PALW_V2_MAX_PAYOUTS_PER_BLOCK`] = 8 per block | `palw_state_v2.rs:233`, drained at `palw_state_v2.rs:4616` | **the row segmentation actually breaks.** The constant is sized by an explicit premise — *"Eight against at most one new claim per block: a backlog drains eight times faster than it can be created, so this bounds latency, not throughput"* — and N claims per answer is exactly the assumption's negation. At N ≥ 8 the queue stops draining faster than it fills |
//! | epoch budget | `budget_blocks`, denominated in **blocks** | [`crate::palw_state_v2::PalwEpochBudgetsV2`] (`:1713`), derived by [`crate::palw_state_v2::derive_epoch_budgets_v2`] (`:5159`) | not a leaf quantity in either direction: the derivation is `⌊tol‰ · E · s_c / (1000 · denom_c)⌋` and takes no pwu, no leaves and no claim count — *"Blocks, never pwu"* |
//! | court close bytes | ≤ [`crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES`] (80 KiB) per dispute | [`crate::palw_class_admission_v2::derive_court_cost_v1`] (`palw_class_admission_v2.rs:255`) | **NOT 1/N.** ADR-0080 §1 measured the binding node as `attn[23] ffn_down`, whose operand width is the MODEL's `ffn_dim` and reads no history — a segment's close is the same size as the whole claim's |
//! | claim state rows, HASHED | `claims` (`:2841`), `panels` (`:2842`), `court_sessions` (`:2843`), `pending_payouts` (`:2866`), `derived_artifacts` (`:2878`) | the `state_root` preimage, `palw_state_v2.rs:3190` | N rows in each, and each one changes the state root every node compares |
//! | claim state rows, REBUILDABLE | `deadlines`, `unresolved`, `work_ids`, `open_courts_by_claim`, `court_deadlines` | `palw_state_v2.rs:2899` — the block marked *"indices: rebuildable, never serialized, never hashed"* | N rows in each, but **outside** the state root: memory and sweep cost, not consensus bytes |
//!
//! Two entries deliberately record a ZERO rather than being left out, because a reader checking
//! whether segmentation is free needs to know they were looked at: a free-prompt commitment's
//! `immature_contribution` and `escrowed_reward` are both **0** (`palw_state_v2.rs:6874`, `:6881`)
//! — "a commitment riding a transaction is not a block's work". So segmentation neither earns nor
//! costs on those two, and no successor should expect relief from them.
//!
//! # The line numbers will drift; the names will not
//!
//! Every citation names the ITEM as well as the line, so a moved line is still findable. Every row
//! that has a NUMBER — the panel, the quorum, `k`, the derivation cap, the signature length, the
//! abandon hold, the exposure lifetime, the epoch budget, the receipt carriage, and both edges of
//! the quanta band — is re-asserted below against the value the code actually holds, so a moved
//! value makes the suite red. Three rows are cited but not value-asserted, and they are named here
//! rather than left to be discovered: the **court close bytes** (whose per-claim-ness rests on
//! ADR-0080 §1's measurement, not on a constant this module can read), the **claim state rows**
//! (structural — the fields are private to `palw_state_v2`, so this module cannot enumerate them;
//! what it CAN do is pin [`crate::palw_state_v2::PALW_STATE_V2_VERSION`], because the root
//! preimage's own doc requires a new version constant for any change to its body — so the
//! hashed/rebuildable split above cannot silently stop being true), and the **singular
//! `claim_id`/`output_root`** of `PalwDerivedArtifactV1` (structural — a type, not a number).
//!
//! # One claim per block is a premise this codebase leans on in three places
//!
//! It is not a stylistic assumption. `PALW_V2_MAX_PAYOUTS_PER_BLOCK`'s sizing argument states it
//! at the constant (`palw_state_v2.rs:232`) and again at the drain (`:4614`), and `apply_attempt`
//! relies on it a third time to make the worker carve's "never exceeds the subsidy" bound
//! *structural rather than arithmetic* (`:7012`: "the claim IS this block, so the block's carve
//! funds exactly one claim"). A design that turns one answer into N claims negates the premise
//! all three rest on, and owes each of them an argument.
//!
//! # Nothing here is armed
//!
//! No `ForkActivation`, no state field, no wire type. `palw_segmentation_cost_v1` is arithmetic
//! over constants that already exist; a consensus path that called it would be a bug.

use crate::dns_finality::STAKE_ATTESTATION_SIG_LEN;
use crate::palw_derived_v1::PALW_DERIVED_MAX_PER_CLAIM;
use crate::palw_fp_devnet_v3::{PALW_V2_PANEL_QUORUM, PALW_V2_PANEL_SEATS};
use crate::palw_fp_interval_v1::PALW_FP_SEAT_INTERVAL_SAMPLES_V1;
use crate::palw_state_v2::PALW_V2_MAX_PAYOUTS_PER_BLOCK;

/// Where a quantity is denominated — the only question this module asks about anything.
///
/// The distinction is not decorative: it is exactly the predicate that decides whether a design
/// may restructure one answer into several units without changing what the chain pays or weighs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwEconomicLocusV1 {
    /// Priced in step leaves. Invariant under restructuring — a job's leaves do not change when
    /// it is cut up.
    PerLeaf,
    /// Priced in step leaves **only inside a band**, because the derivation floors below it and
    /// saturates above it. Invariant where the band holds and nowhere else, which is the
    /// narrowing this census exists to record.
    PerLeafInBand,
    /// Priced per claim. N claims for one answer is N of these, whatever the leaves.
    PerClaim,
    /// Priced per block (or per epoch of blocks). Not a leaf quantity in either direction.
    PerBlock,
}

impl PalwEconomicLocusV1 {
    /// Whether restructuring one answer into several claims leaves this quantity alone.
    ///
    /// [`Self::PerLeafInBand`] answers `false`, deliberately: an invariant with an escape hatch is
    /// not an invariant, and the escape hatch here is reachable at the SHIPPED cap
    /// (`MAX_QUANTA_PER_RECEIPT = 64`), not at some pathological parameter.
    pub const fn is_invariant_under_segmentation(&self) -> bool {
        matches!(self, Self::PerLeaf)
    }
}

/// One row of the census: a quantity, where it lives, and where to read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEconomicQuantityV1 {
    /// The quantity, in the words the code uses for it.
    pub name: &'static str,
    pub locus: PalwEconomicLocusV1,
    /// `file:line` at the commit that wrote this census, plus the item name — the name is the
    /// durable half.
    pub cited_at: &'static str,
    /// What N segments of one answer cost on this row.
    pub under_n_segments: &'static str,
}

/// **The census.** Every quantity that pays, weighs or gates on the free-prompt path, with its
/// locus.
///
/// A successor design's job is to read this table and say, for each `PerClaim` row, either "my
/// unit is still one claim" or "here is why N of these is acceptable". A design that says neither
/// is the one ADR-0080 was.
pub const PALW_ECONOMIC_LOCUS_CENSUS_V1: &[PalwEconomicQuantityV1] = &[
    PalwEconomicQuantityV1 {
        name: "quantum leaves (fp_class_quantum_leaves_v1)",
        locus: PalwEconomicLocusV1::PerLeaf,
        cited_at: "palw_freeprompt_v3.rs:379 fp_class_quantum_leaves_v1",
        under_n_segments: "unchanged — a property of the class, not of the claim",
    },
    PalwEconomicQuantityV1 {
        name: "quanta (fp_quanta_v3)",
        locus: PalwEconomicLocusV1::PerLeafInBand,
        cited_at: "palw_freeprompt_v3.rs:392 fp_quanta_v3",
        under_n_segments: "equal inside [quantum, cap*quantum]; LOST to the floor below it; MANUFACTURED above it",
    },
    PalwEconomicQuantityV1 {
        name: "lottery entries (fp_quantum_ticket_v3)",
        locus: PalwEconomicLocusV1::PerLeafInBand,
        cited_at: "palw_freeprompt_v3.rs:427 fp_quantum_ticket_v3",
        under_n_segments: "one draw per quantum, so it tracks the quanta exactly — and inherits their band",
    },
    PalwEconomicQuantityV1 {
        name: "safe weight per spend (per_quantum)",
        locus: PalwEconomicLocusV1::PerLeafInBand,
        cited_at: "palw_state_v2.rs:6921 apply_receipt_spend",
        under_n_segments: "claim.pwu / quanta == quantum exactly, so weight is leaves — inside the same band",
    },
    PalwEconomicQuantityV1 {
        name: "attempt-lane exposure (palw_exposure_pwu_v1)",
        locus: PalwEconomicLocusV1::PerClaim,
        cited_at: "palw_state_v2.rs:1311 palw_exposure_pwu_v1",
        under_n_segments: "N flat reservations; the function takes no leaf count",
    },
    PalwEconomicQuantityV1 {
        name: "free-prompt exposure (apply_free_prompt reserved)",
        locus: PalwEconomicLocusV1::PerLeafInBand,
        cited_at: "palw_state_v2.rs:6831 apply_free_prompt",
        under_n_segments: "pwu * slash_value, so per-leaf until the quanta cap saturates it — then N * the ceiling",
    },
    PalwEconomicQuantityV1 {
        name: "challenger exposure to open a court",
        locus: PalwEconomicLocusV1::PerClaim,
        cited_at: "palw_state_v2.rs:6674 apply_court_opened",
        under_n_segments: "N reservations on the CHALLENGER's bond — policing gets N times dearer too",
    },
    PalwEconomicQuantityV1 {
        name: "panel seats (PALW_V2_PANEL_SEATS)",
        locus: PalwEconomicLocusV1::PerClaim,
        cited_at: "palw_fp_devnet_v3.rs:459 PALW_V2_PANEL_SEATS",
        under_n_segments: "N panels, N*5 seats on duty",
    },
    PalwEconomicQuantityV1 {
        name: "quorum receipts (PALW_V2_PANEL_QUORUM)",
        locus: PalwEconomicLocusV1::PerClaim,
        cited_at: "palw_fp_devnet_v3.rs:460 PALW_V2_PANEL_QUORUM",
        under_n_segments: "N*3 ML-DSA-87 signatures on chain",
    },
    PalwEconomicQuantityV1 {
        name: "seat interval replays (PALW_FP_SEAT_INTERVAL_SAMPLES_V1)",
        locus: PalwEconomicLocusV1::PerClaim,
        cited_at: "palw_fp_interval_v1.rs:28 PALW_FP_SEAT_INTERVAL_SAMPLES_V1",
        under_n_segments: "N*seats*k replays — the honest verification CPU for one answer, N-fold",
    },
    PalwEconomicQuantityV1 {
        name: "derivation slots (PALW_DERIVED_MAX_PER_CLAIM)",
        locus: PalwEconomicLocusV1::PerClaim,
        cited_at: "palw_derived_v1.rs:36 PALW_DERIVED_MAX_PER_CLAIM",
        under_n_segments: "N*4 slots, none of which a singular claim_id/output_root can span",
    },
    PalwEconomicQuantityV1 {
        name: "abandon hold (fp_abandon_hold_daa)",
        locus: PalwEconomicLocusV1::PerClaim,
        cited_at: "palw_state_v2.rs:1070 palw_claim_is_on_abandon_hold_v2",
        under_n_segments: "N held reservations past the void",
    },
    PalwEconomicQuantityV1 {
        name: "exposure lifetime (max_claim_exposure_daa)",
        locus: PalwEconomicLocusV1::PerClaim,
        cited_at: "palw_fp_devnet_v3.rs:82 PalwLatticeWindowsV1::max_claim_exposure_daa",
        under_n_segments: "N concurrent holds against one bond's ceiling",
    },
    PalwEconomicQuantityV1 {
        name: "court close bytes (derive_court_cost_v1)",
        locus: PalwEconomicLocusV1::PerClaim,
        cited_at: "palw_class_admission_v2.rs:255 derive_court_cost_v1",
        under_n_segments: "N full-size closes — the binding node's operand width is the model's, not the interval's",
    },
    PalwEconomicQuantityV1 {
        name: "claim state rows, hashed (claims/panels/court_sessions/pending_payouts/derived_artifacts)",
        locus: PalwEconomicLocusV1::PerClaim,
        cited_at: "palw_state_v2.rs:3190 PalwChainStateV2::state_root",
        under_n_segments: "N rows in each, and every one of them moves the state root",
    },
    PalwEconomicQuantityV1 {
        name: "claim state indices, rebuildable (deadlines/unresolved/work_ids/open_courts_by_claim/court_deadlines)",
        locus: PalwEconomicLocusV1::PerClaim,
        cited_at: "palw_state_v2.rs:2899 'indices: rebuildable, never serialized, never hashed'",
        under_n_segments: "N rows in each, but OUTSIDE the state root — sweep and memory cost, not consensus bytes",
    },
    PalwEconomicQuantityV1 {
        name: "payout queue rows (pending_payouts)",
        locus: PalwEconomicLocusV1::PerClaim,
        cited_at: "palw_state_v2.rs:2866 PalwChainStateV2::pending_payouts",
        under_n_segments: "N rows, hashed into state_root — this one is NOT a rebuildable index",
    },
    PalwEconomicQuantityV1 {
        name: "payout drain rate (PALW_V2_MAX_PAYOUTS_PER_BLOCK)",
        locus: PalwEconomicLocusV1::PerBlock,
        cited_at: "palw_state_v2.rs:233 PALW_V2_MAX_PAYOUTS_PER_BLOCK",
        under_n_segments: "fixed at 8/block while the queue fills N per answer — the constant's own \
                           sizing premise is 'at most one new claim per block', which is what \
                           segmentation negates",
    },
    PalwEconomicQuantityV1 {
        name: "producer escrow (escrowed_reward)",
        locus: PalwEconomicLocusV1::PerBlock,
        cited_at: "palw_state_v2.rs:6881 apply_free_prompt (zero); :7015 apply_attempt (one carve)",
        under_n_segments: "0 on the free-prompt lane either way; on the attempt lane exactly ONE \
                           carve per block, because only the block's own attempt escrows \
                           (:7012 'the claim IS this block') — per block, never per claim",
    },
    PalwEconomicQuantityV1 {
        name: "immature contribution (beta * pwu)",
        locus: PalwEconomicLocusV1::PerBlock,
        cited_at: "palw_state_v2.rs:6874 apply_free_prompt (zero on a commitment)",
        under_n_segments: "0 either way — commitment-stuffing must not pump live weight",
    },
    PalwEconomicQuantityV1 {
        name: "epoch production budget (budget_blocks)",
        locus: PalwEconomicLocusV1::PerBlock,
        cited_at: "palw_state_v2.rs:5159 derive_epoch_budgets_v2",
        under_n_segments: "denominated in blocks; no claim term and no leaf term appear in it",
    },
];

/// **What N claims cost for one answer that could have been one claim.**
///
/// Every field is a count or a byte total this chain already charges; nothing here is a new price.
/// The point of gathering them is that each one alone reads as small.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSegmentationCostV1 {
    pub segments: u64,
    /// One panel drawn per claim.
    pub panels: u64,
    /// The MINIMUM receipt set that reaches quorum, per claim, summed.
    pub quorum_receipts: u64,
    /// `quorum_receipts × STAKE_ATTESTATION_SIG_LEN` — the signature bytes alone.
    pub receipt_signature_bytes: u64,
    /// The whole `ReceiptLicensed` carriage, envelopes included. Strictly larger than the
    /// signature total, which is why the signature total alone understates the bill.
    pub licensed_object_bytes: u64,
    /// `segments × seats × k` — the panel's honest replay work for one answer.
    pub seat_interval_replays: u64,
    /// `segments × per_claim_exposure_sompi` — held concurrently, because the segments of one
    /// answer are concurrent.
    pub exposure_sompi: u128,
    /// `segments × PALW_DERIVED_MAX_PER_CLAIM`.
    pub derivation_slots: u64,
    /// One `pending_payouts` row per finalized claim — hashed into the state root, not a
    /// rebuildable index.
    pub payout_rows: u64,
    /// **Blocks the payout queue needs to drain this answer**, at
    /// [`PALW_V2_MAX_PAYOUTS_PER_BLOCK`] per block: `⌈payout_rows / 8⌉`. One for an unsegmented
    /// answer, and the number that shows why the constant's sizing premise does not survive
    /// segmentation.
    pub payout_drain_blocks: u64,
}

/// The borsh wire size of one `PalwSeatReceiptV2` carrying a `Valid` verdict and a full-length
/// ML-DSA-87 signature.
///
/// DERIVED from the struct's own fields rather than typed in, and the test below re-derives it a
/// second way — by serialising a real receipt — so the two can never quietly disagree:
/// `claim` (`Hash64`, 64) + verdict tag (1) + `seat_bond` (a `TransactionOutpoint`, and
/// `TransactionId` is [`kaspa_hashes::Hash64`] on this chain per ADR-0008, so 64 + 4) +
/// `signed_daa` (8) + the `Vec<u8>` length prefix (4) + the signature (4,627).
///
/// The 32-byte reading of an outpoint's transaction id is upstream Kaspa's and is wrong here;
/// `the_seat_receipt_wire_size_is_what_the_constant_says` is what caught it.
pub const PALW_SEAT_RECEIPT_VALID_WIRE_BYTES_V1: u64 = 64 + 1 + (64 + 4) + 8 + 4 + STAKE_ATTESTATION_SIG_LEN as u64;

/// The borsh wire size of a `PalwConsensusObjectV2::ReceiptLicensed` carrying `receipts` receipts:
/// the object's own enum tag (1) + `claim` (64) + the vector's length prefix (4) + the receipts.
pub const fn palw_receipt_licensed_wire_bytes_v1(receipts: u64) -> u64 {
    1 + 64 + 4 + receipts * PALW_SEAT_RECEIPT_VALID_WIRE_BYTES_V1
}

/// Add up what a `segments`-way split costs, at this network's own panel constants.
///
/// `per_claim_exposure_sompi` is the caller's, because it is the one input that depends on the
/// class: it is `pwu × slash_value_per_pwu` on the free-prompt lane and the flat
/// `pwu_per_inference × slash_value_per_pwu` on the attempt lane.
pub fn palw_segmentation_cost_v1(segments: u64, per_claim_exposure_sompi: u128) -> PalwSegmentationCostV1 {
    let quorum_receipts = segments.saturating_mul(PALW_V2_PANEL_QUORUM as u64);
    PalwSegmentationCostV1 {
        segments,
        panels: segments,
        quorum_receipts,
        receipt_signature_bytes: quorum_receipts.saturating_mul(STAKE_ATTESTATION_SIG_LEN as u64),
        licensed_object_bytes: segments.saturating_mul(palw_receipt_licensed_wire_bytes_v1(PALW_V2_PANEL_QUORUM as u64)),
        seat_interval_replays: segments
            .saturating_mul(PALW_V2_PANEL_SEATS as u64)
            .saturating_mul(PALW_FP_SEAT_INTERVAL_SAMPLES_V1 as u64),
        exposure_sompi: (segments as u128).saturating_mul(per_claim_exposure_sompi),
        derivation_slots: segments.saturating_mul(PALW_DERIVED_MAX_PER_CLAIM as u64),
        payout_rows: segments,
        payout_drain_blocks: segments.div_ceil(PALW_V2_MAX_PAYOUTS_PER_BLOCK as u64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_freeprompt_v3::{fp_class_quantum_leaves_v1, fp_quanta_v3, fp_quantum_ticket_v3};
    use crate::palw_state_v2::{PalwClassStateV2, PalwClassStatusV2, PalwPwuRuleV2, palw_exposure_pwu_v1};
    use kaspa_hashes::Hash64;

    /// testnet-11's shipped free-prompt price, read from `palw_fp_devnet_v3`'s own constants
    /// through the bundle rather than retyped: a quantum is an eighth of the class's canonical
    /// job and a receipt draws at most 64 of them.
    const QUANTA_PER_CANONICAL_JOB: u32 = 8;
    const MAX_QUANTA_PER_RECEIPT: u32 = 64;

    /// A stand-in class: the only two fields the exposure and pricing paths read are the pwu rule
    /// and the slash value.
    fn class(pwu_per_inference: u64, slash_value_per_pwu: u64) -> PalwClassStateV2 {
        PalwClassStateV2 {
            artifact_root: Hash64::from_u64_word(0xA57),
            slash_value_per_pwu,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
            status: PalwClassStatusV2::Active,
            registered_daa: 0,
            registrant_bond: None,
        }
    }

    /// The free-prompt lane's own pricing, lifted out of `apply_free_prompt`
    /// (`palw_state_v2.rs:6808-6814`) so a test can ask what a claim of `work_leaves` costs
    /// without building a whole transition. Byte-identical arithmetic; if it ever diverges,
    /// `the_price_here_is_the_transitions_price` goes red.
    fn price(work_leaves: u64, pwu_per_inference: u64) -> (u32, u64, u64) {
        let quantum = fp_class_quantum_leaves_v1(pwu_per_inference, QUANTA_PER_CANONICAL_JOB);
        let quanta = fp_quanta_v3(work_leaves, quantum, MAX_QUANTA_PER_RECEIPT);
        (quanta, quantum, quanta as u64 * quantum)
    }

    // -----------------------------------------------------------------------------------------
    // The census itself
    // -----------------------------------------------------------------------------------------

    /// Every constant the census quotes is the constant the code holds. This is the row that goes
    /// red when someone changes a number and not the table.
    #[test]
    fn the_census_quotes_the_values_the_code_actually_holds() {
        assert_eq!(PALW_V2_PANEL_SEATS, 5, "the panel is five seats (palw_fp_devnet_v3.rs:459)");
        assert_eq!(PALW_V2_PANEL_QUORUM, 3, "quorum is three (palw_fp_devnet_v3.rs:460)");
        assert_eq!(PALW_FP_SEAT_INTERVAL_SAMPLES_V1, 4, "a seat opens k=4 intervals per claim");
        assert_eq!(PALW_DERIVED_MAX_PER_CLAIM, 4, "four derivations per claim");
        assert_eq!(STAKE_ATTESTATION_SIG_LEN, 4_627, "one ML-DSA-87 signature");
        assert_eq!(PALW_V2_MAX_PAYOUTS_PER_BLOCK, 8, "the payout queue drains eight claims per block");
        assert_eq!(crate::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1.fp_abandon_hold, 600, "the RC abandon hold, in DAA");
        assert_eq!(
            crate::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1.max_claim_exposure_daa(),
            7_200,
            "2*(600+600) + 1200 + 3000 + 600 — how long ONE claim can hold its bond's collateral"
        );
    }

    /// The census is a partition, not a wish list: every row is one of the four loci, and only
    /// `PerLeaf` claims invariance. Guards against a later edit that marks a `PerClaim` row
    /// invariant to make a design fit.
    #[test]
    fn only_per_leaf_rows_claim_invariance() {
        assert_eq!(
            PALW_ECONOMIC_LOCUS_CENSUS_V1.len(),
            21,
            "the census is complete or it is nothing — a row added or dropped is a deliberate edit here"
        );
        let mut names: Vec<&str> = PALW_ECONOMIC_LOCUS_CENSUS_V1.iter().map(|row| row.name).collect();
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique, "one quantity, one row");
        let invariant: Vec<&str> = PALW_ECONOMIC_LOCUS_CENSUS_V1
            .iter()
            .filter(|row| row.locus.is_invariant_under_segmentation())
            .map(|row| row.name)
            .collect();
        assert_eq!(
            invariant,
            vec!["quantum leaves (fp_class_quantum_leaves_v1)"],
            "exactly ONE quantity is unconditionally invariant under restructuring: the quantum, \
             which is a property of the class. Everything ADR-0080 Decision 3 called invariant is \
             invariant only inside the quanta band."
        );
        assert!(!PalwEconomicLocusV1::PerLeafInBand.is_invariant_under_segmentation());
        assert!(!PalwEconomicLocusV1::PerClaim.is_invariant_under_segmentation());
        assert!(!PalwEconomicLocusV1::PerBlock.is_invariant_under_segmentation());
    }

    // -----------------------------------------------------------------------------------------
    // The per-LEAF half — Decision 3's claim, verified, and then bounded
    // -----------------------------------------------------------------------------------------

    /// The quantum names no claim, no segment and no network constant — only the class's own
    /// canonical job. This is the one row that is invariant without qualification.
    #[test]
    fn the_quantum_is_a_fraction_of_the_class_job_and_names_no_claim() {
        assert_eq!(fp_class_quantum_leaves_v1(1_600, 8), 200);
        assert_eq!(fp_class_quantum_leaves_v1(160, 8), 20, "a small class gets a small quantum, not the same one");
        assert_eq!(fp_class_quantum_leaves_v1(3, 8), 1, "max(1, ...) — never zero, so a tiny class still prices");
    }

    /// **Decision 3, verified where it holds.** One job of 1,600 leaves cut into four exact
    /// 400-leaf segments yields the same quanta, the same pwu and therefore the same weight.
    #[test]
    fn quanta_and_weight_are_invariant_under_an_exact_split() {
        let pwu_per_inference = 1_600;
        let (whole_quanta, quantum, whole_pwu) = price(1_600, pwu_per_inference);
        assert_eq!((whole_quanta, quantum, whole_pwu), (8, 200, 1_600));

        for segments in [2u64, 4, 8] {
            let per_segment = 1_600 / segments;
            let (q, _, pwu) = price(per_segment, pwu_per_inference);
            assert_eq!(
                (q as u64 * segments, pwu * segments),
                (whole_quanta as u64, whole_pwu),
                "{segments} exact segments of {per_segment} leaves must weigh what one 1,600-leaf claim weighs"
            );
        }
    }

    /// The lottery entries follow the quanta exactly: one draw per `(claim, quantum_index)`, so
    /// the COUNT of draws for one answer is the count of quanta however the answer is cut. The
    /// draw VALUES differ — they are keyed on the claim id — and that is the property that keeps
    /// segmentation from being a re-roll of a losing ticket rather than a re-entry.
    #[test]
    fn the_draw_count_is_the_quantum_count_and_the_values_are_claim_keyed() {
        let (network, beacon) = (Hash64::from_u64_word(0x11), Hash64::from_u64_word(0x22));
        let whole = Hash64::from_u64_word(0xC1);
        let (segment_a, segment_b) = (Hash64::from_u64_word(0xC2), Hash64::from_u64_word(0xC3));

        let (whole_quanta, _, _) = price(1_600, 1_600);
        let (segment_quanta, _, _) = price(800, 1_600);
        assert_eq!(whole_quanta as u64, 2 * segment_quanta as u64, "8 draws either way");

        let one: Vec<u128> = (0..whole_quanta).map(|q| fp_quantum_ticket_v3(network, beacon, whole, q)).collect();
        let two: Vec<u128> = [segment_a, segment_b]
            .iter()
            .flat_map(|claim| (0..segment_quanta).map(move |q| fp_quantum_ticket_v3(network, beacon, *claim, q)))
            .collect();
        assert_eq!(one.len(), two.len(), "the same number of entries in the lottery");
        assert!(one.iter().all(|t| !two.contains(t)), "different claim ids draw different tickets");
    }

    /// **The floor edge Decision 3 did not name.** A segment shorter than one quantum prices at
    /// zero quanta, and the transition refuses it (`PalwStateV2Error::ZeroQuanta`,
    /// `palw_state_v2.rs:6810`). So a fine enough split does not merely round down — it makes the
    /// work unrepresentable, and the remainder of a coarser split is uncompensated.
    #[test]
    fn splitting_below_one_quantum_destroys_the_work_entirely() {
        let pwu_per_inference = 1_600; // quantum = 200
        assert_eq!(price(199, pwu_per_inference).0, 0, "a 199-leaf segment earns nothing and is refused");
        assert_eq!(price(1_599, pwu_per_inference).0, 7, "and the whole claim's own remainder certifies but never draws");

        // 1,600 leaves in 9 segments: each 177 leaves (plus a 7-leaf tail), each under the quantum.
        let per_segment = 1_600 / 9;
        assert_eq!(price(per_segment, pwu_per_inference).0, 0, "nine ways is nine refusals for work that priced at 8 quanta whole");
    }

    /// **The cap edge, which is the direction that matters.** `fp_quanta_v3` saturates at
    /// `MAX_QUANTA_PER_RECEIPT`, so a claim above `cap × quantum` leaves earns nothing more —
    /// and splitting it recovers exactly the weight the cap removed. This is the "free money"
    /// Decision 3 forbade, reachable at the SHIPPED cap rather than at a pathological parameter.
    ///
    /// The code says so in its own words: *"more work is more claims, each paying its own fee"*
    /// (`palw_freeprompt_v3.rs:388`). The census's job is to say what that fee actually is —
    /// see `a_thirty_seven_way_split_costs_half_a_megabyte_of_signatures`.
    #[test]
    fn splitting_above_the_receipt_cap_manufactures_quanta() {
        let pwu_per_inference = 1_600; // quantum = 200, cap = 64 quanta = 12,800 leaves
        let saturating = 12_800;
        assert_eq!(price(saturating, pwu_per_inference).0, MAX_QUANTA_PER_RECEIPT, "exactly at the cap");
        assert_eq!(price(saturating * 4, pwu_per_inference).0, MAX_QUANTA_PER_RECEIPT, "four times the work, the same 64 draws");

        let whole = price(saturating * 4, pwu_per_inference);
        let split: u64 = (0..4).map(|_| price(saturating, pwu_per_inference).2).sum();
        assert_eq!(whole.2, 12_800, "one claim's pwu saturates at cap x quantum");
        assert_eq!(split, 51_200, "four claims for the same leaves weigh four times as much");
        assert_eq!(split, 4 * whole.2, "segmentation past the cap is a 4x weight multiplier, not a neutral restructuring");
    }

    // -----------------------------------------------------------------------------------------
    // The per-CLAIM half — the load-bearing one
    // -----------------------------------------------------------------------------------------

    /// **Exposure on the attempt lane does not scale with leaves and cannot be made to.**
    /// `palw_exposure_pwu_v1` takes the class and the claimed pwu and, under `DerivedV1`,
    /// returns `pwu_per_inference` — a registered constant. There is no leaf argument, so no
    /// restructuring can change what one claim reserves.
    #[test]
    fn attempt_exposure_is_flat_per_claim_whatever_the_claim_did() {
        let c = class(1_600, 5);
        for claimed_pwu in [1u64, 1_600, 100_000, u64::MAX] {
            assert_eq!(
                palw_exposure_pwu_v1(&c, claimed_pwu),
                1_600,
                "the reservation is one canonical inference, whatever the attempt claims"
            );
        }
        // What the transition multiplies it by (`palw_state_v2.rs:6987`).
        assert_eq!(palw_exposure_pwu_v1(&c, 1_600) as u128 * c.slash_value_per_pwu as u128, 8_000);
        assert_eq!(
            palw_segmentation_cost_v1(37, 8_000).exposure_sompi,
            296_000,
            "37 segments hold 37 flat reservations, concurrently, against one bond's ceiling"
        );
    }

    /// **Free-prompt exposure is per-leaf until the cap and flat after it**, which is the same
    /// narrowing the quanta showed and the reason the census marks it `PerLeafInBand` rather than
    /// agreeing with ADR-0080's blanket "exposure reserves a flat `pwu_per_inference` per claim".
    /// That sentence is true of the ATTEMPT lane (above) and false of the lane a long free-prompt
    /// answer would actually travel — until the cap bites, at which point it becomes true again
    /// and N segments reserve N ceilings.
    #[test]
    fn free_prompt_exposure_scales_with_leaves_until_the_cap_and_then_stops() {
        let (pwu_per_inference, slash) = (1_600u64, 5u64);
        let reserved = |leaves: u64| price(leaves, pwu_per_inference).2 as u128 * slash as u128;

        assert_eq!(reserved(200), 1_000, "one quantum");
        assert_eq!(reserved(1_600), 8_000, "eight quanta — eight times the reservation, so it DOES scale");
        assert_eq!(reserved(12_800), 64_000, "at the cap");
        assert_eq!(reserved(51_200), 64_000, "four times the leaves, the same reservation — saturated");
        assert_eq!(
            4 * reserved(12_800),
            256_000,
            "and four claims for those same leaves reserve four ceilings: 4x the collateral for one answer"
        );
    }

    /// The panel is drawn per claim and its quorum is signed per claim. Neither constant has a
    /// leaf term, so N segments is N panels and 3N signatures — nothing amortizes.
    #[test]
    fn the_panel_and_its_quorum_are_per_claim() {
        let one = palw_segmentation_cost_v1(1, 0);
        let many = palw_segmentation_cost_v1(37, 0);
        assert_eq!((one.panels, one.quorum_receipts), (1, 3));
        assert_eq!((many.panels, many.quorum_receipts), (37, 111));
        assert_eq!(many.quorum_receipts, 37 * one.quorum_receipts, "linear in segments, constant in leaves");
    }

    /// **The panel's honest CPU is per claim too, and a short segment is checked WHOLE.**
    /// `palw_fp_interval_draw_v1` returns every interval when `interval_count <= k`, so cutting a
    /// job into segments short enough to have four intervals each makes every seat replay every
    /// interval of every segment. N segments therefore buy denser sampling — and charge the panel
    /// N times the replay work for one answer. That trade may be worth making; the census's point
    /// is that it IS a trade, not a neutral restructuring.
    #[test]
    fn the_seat_interval_draw_is_per_claim_and_a_short_segment_is_checked_whole() {
        let k = PALW_FP_SEAT_INTERVAL_SAMPLES_V1;
        let (network, beacon, claim) = (Hash64::from_u64_word(1), Hash64::from_u64_word(2), Hash64::from_u64_word(3));

        let long = crate::palw_fp_interval_v1::palw_fp_interval_draw_v1(&network, &beacon, &claim, 0, k, 128);
        assert_eq!(long.len(), k as usize, "a long claim is sampled");

        let short = crate::palw_fp_interval_v1::palw_fp_interval_draw_v1(&network, &beacon, &claim, 0, k, 4);
        assert_eq!(short, vec![0, 1, 2, 3], "a segment with k intervals is replayed end to end");

        assert_eq!(
            palw_segmentation_cost_v1(37, 0).seat_interval_replays,
            740,
            "37 segments x 5 seats x 4 intervals — for ONE answer that as a single claim costs 20"
        );
    }

    /// The derivation table is bounded per claim (`palw_state_v2.rs:4426`: "the table is bounded
    /// per claim precisely so that it is bounded overall"), and a `PalwDerivedArtifactV1` names a
    /// singular `claim_id`. So N segments is N × 4 slots that no single derivation can span — the
    /// finding that refuted ADR-0080's mechanism, expressed as a cost.
    #[test]
    fn derivation_slots_are_per_claim_and_no_derivation_spans_two() {
        assert_eq!(palw_segmentation_cost_v1(1, 0).derivation_slots, 4);
        assert_eq!(palw_segmentation_cost_v1(37, 0).derivation_slots, 148);
    }

    /// The abandon hold is a per-claim DAA cost on the executor's collateral, and it is inside the
    /// span that sizes every bond on the network: dropping it shortens `max_claim_exposure_daa` by
    /// exactly its own length. N abandoned segments hold N reservations for that long.
    #[test]
    fn the_abandon_hold_is_per_claim_collateral_time() {
        let rc = crate::palw_fp_devnet_v3::PALW_RC_WINDOWS_V1;
        let mut without = rc;
        without.fp_abandon_hold = 0;
        assert_eq!(
            rc.max_claim_exposure_daa() - without.max_claim_exposure_daa(),
            rc.fp_abandon_hold,
            "the hold is a term of ONE claim's exposure lifetime, so N claims hold N of them"
        );
        assert_eq!(rc.max_claim_exposure_daa(), 7_200);
    }

    /// **The state-root row, pinned the only way this module can pin it.**
    ///
    /// `PalwChainStateV2`'s fields are private, so the census cannot enumerate them from here and
    /// its hashed/rebuildable split is read off `state_root` (`palw_state_v2.rs:3190`) and off the
    /// `"indices: rebuildable, never serialized, never hashed"` marker (`:2899`). What IS
    /// assertable is the version that guards that body: the root's own doc says changing it "is a
    /// consensus change and needs a new version constant, a matching ADR-0043 amendment, and new
    /// golden vectors". So a bump here is the signal to re-read the two claim-state rows above.
    ///
    /// This is a tripwire, not a proof, and it is listed as such in the module doc's gaps.
    #[test]
    fn the_claim_state_rows_are_pinned_to_the_state_root_version() {
        assert_eq!(
            crate::palw_state_v2::PALW_STATE_V2_VERSION,
            17,
            "the census's hashed/rebuildable split was read at state version 17; a bump means the \
             root preimage moved and both claim-state rows need re-reading"
        );
    }

    /// **The payout queue is the row where segmentation breaks a stated premise, not just a
    /// budget.** `pending_payouts` holds one row per finalized claim and the transition drains at
    /// most [`PALW_V2_MAX_PAYOUTS_PER_BLOCK`] of them per block (`palw_state_v2.rs:4616`). That
    /// constant is not arbitrary: it is sized by an argument written twice in the source, at its
    /// definition (`:232`) and at the drain (`:4614`) —
    ///
    /// > *"Eight against at most one new claim per block: a backlog drains eight times faster
    /// > than it can be created, so this bounds latency, not throughput."*
    ///
    /// "At most one new claim per block" is precisely what a segmenting design negates. One
    /// answer arriving as N claims enqueues N payout rows, and the safety factor the constant was
    /// chosen for is divided by N: at N ≥ 8 the queue no longer drains faster than it fills, and
    /// the property the comment claims — bounds latency, NOT throughput — inverts.
    ///
    /// The chain does not break here (the drain is a prefix and the coinbase builder takes the
    /// same one, so nodes agree), but a design that segments must either keep N < 8 or re-argue
    /// this constant. That is the whole job of this census.
    #[test]
    fn the_payout_queue_drains_per_block_against_a_premise_segmentation_negates() {
        assert_eq!(PALW_V2_MAX_PAYOUTS_PER_BLOCK, 8);

        // One answer, one claim: one row, drained by the very next block.
        let single = palw_segmentation_cost_v1(1, 0);
        assert_eq!((single.payout_rows, single.payout_drain_blocks), (1, 1));

        // The premise holds while an answer is one claim, and fails the moment it is nine.
        let at_the_edge = palw_segmentation_cost_v1(PALW_V2_MAX_PAYOUTS_PER_BLOCK as u64, 0);
        assert_eq!(at_the_edge.payout_drain_blocks, 1, "eight segments still clear in one block");
        let past_the_edge = palw_segmentation_cost_v1(PALW_V2_MAX_PAYOUTS_PER_BLOCK as u64 + 1, 0);
        assert_eq!(past_the_edge.payout_drain_blocks, 2, "nine does not — one answer now outlives one block");

        // The census's running example.
        let many = palw_segmentation_cost_v1(37, 0);
        assert_eq!(many.payout_rows, 37, "one row per segment, each hashed into the state root");
        assert_eq!(many.payout_drain_blocks, 5, "⌈37/8⌉ — five blocks of coinbase to pay ONE answer");
        assert_eq!(
            many.payout_drain_blocks,
            5 * single.payout_drain_blocks,
            "the drain is a per-BLOCK ceiling met by a per-CLAIM queue, so segmentation is the only \
             term that can move it"
        );
    }

    /// The epoch budget is denominated in BLOCKS, and [`crate::palw_state_v2::derive_epoch_budgets_v2`]
    /// takes no pwu, no leaf count and no claim count at all: `⌊tol‰ · E · s_c / (1000 · denom_c)⌋`.
    /// It is neither multiplied nor divided by segmentation; it is simply a different currency,
    /// which is why the census lists it as `PerBlock` rather than as a cost. Its own doc says why:
    /// *"Blocks, never pwu"* — a pwu budget would shrink in block terms as a class got harder.
    #[test]
    fn the_epoch_budget_is_denominated_in_blocks_not_leaves() {
        use std::collections::{BTreeMap, BTreeSet};
        let (base, other) = (Hash64::from_u64_word(0xB0), Hash64::from_u64_word(0xB1));
        let shares = BTreeMap::from([(base, 500u16), (other, 500u16)]);
        let competing = BTreeSet::from([base, other]);
        let budgets = crate::palw_state_v2::derive_epoch_budgets_v2(&shares, &BTreeSet::new(), &competing, 264, 1_000, 7);

        assert_eq!(
            budgets.budget_blocks[&base], 132,
            "⌊1000 x 264 x 500 / (1000 x 1000)⌋ — half a 264-block epoch, at unit tolerance, against a 1000‰ census"
        );
        assert_eq!(budgets.budget_blocks[&base], budgets.budget_blocks[&other]);
        assert_eq!(
            budgets.epoch_index, 7,
            "a budget is only comparable within its own epoch — and the epoch is the only claim-ish \
             term anywhere in it"
        );

        // Halving the epoch halves the budget; nothing about the work inside a block can.
        let shorter = crate::palw_state_v2::derive_epoch_budgets_v2(&shares, &BTreeSet::new(), &competing, 132, 1_000, 7);
        assert_eq!(shorter.budget_blocks[&base], 66, "the only lever is blocks");
    }

    /// **Segmentation raises the price of POLICING at the same rate it raises the price of
    /// producing**, which is the row ADR-0080's refutation did not list. Opening a court reserves
    /// the claim's own `reserved` against the CHALLENGER's bond (`palw_state_v2.rs:6674`) — "the
    /// same number the executor has at stake in the dispute, so the two sides face the same
    /// figure" — and the comment there names the consequence: "the ceiling it already lives under
    /// does the counting". So a bond that could hold M concurrent challenges can hold only M/N
    /// challenges against N-way segmented answers, and the fraction of the network one honest
    /// challenger can police falls by N.
    #[test]
    fn prosecuting_a_segmented_answer_costs_the_challenger_n_reservations() {
        // The reservation `apply_court_opened` charges is `claim.reserved` itself, so the
        // challenger's per-claim figure IS the executor's — the same input to the same arithmetic.
        // (Read from the cited line; this module runs no transition. That is the gap, named.)
        let per_claim_exposure = 8_000u128; // 1,600 pwu x 5 sompi/pwu, the class built above
        assert_eq!(palw_segmentation_cost_v1(37, per_claim_exposure).exposure_sompi, 296_000);

        // What one fixed ceiling buys, counted in ANSWERS rather than in claims.
        let ceiling = 296_000u128;
        assert_eq!(ceiling / per_claim_exposure, 37, "37 unsegmented answers, one claim each");
        assert_eq!(
            ceiling / (37 * per_claim_exposure),
            1,
            "or exactly ONE 37-way segmented answer — the same collateral polices 1/37th as much chain"
        );
    }

    // -----------------------------------------------------------------------------------------
    // Make it cost something
    // -----------------------------------------------------------------------------------------

    /// The wire size of one receipt, derived twice: from the field widths in
    /// [`PALW_SEAT_RECEIPT_VALID_WIRE_BYTES_V1`] and by serialising a real
    /// `PalwSeatReceiptV2`. Two derivations of one number cannot disagree quietly.
    #[test]
    fn the_seat_receipt_wire_size_is_what_the_constant_says() {
        let receipt = crate::palw_panel_v2::PalwSeatReceiptV2 {
            claim: Hash64::from_u64_word(0xC1A),
            verdict: crate::palw_panel_v2::PalwReceiptVerdictV2::Valid,
            seat_bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint {
                transaction_id: crate::tx::TransactionId::from_u64_word(0xB0),
                index: 0,
            }),
            signed_daa: 1_234,
            signature: vec![0x5A; STAKE_ATTESTATION_SIG_LEN],
        };
        let bytes = borsh::to_vec(&receipt).expect("a receipt is borsh-serializable");
        assert_eq!(bytes.len() as u64, PALW_SEAT_RECEIPT_VALID_WIRE_BYTES_V1);
        assert_eq!(bytes.len(), 4_772, "64 claim + 1 verdict + 68 bond + 8 daa + 4 len + 4,627 signature");

        let object = crate::palw_state_v2::PalwConsensusObjectV2::ReceiptLicensed {
            claim: Hash64::from_u64_word(0xC1A),
            receipts: vec![receipt; PALW_V2_PANEL_QUORUM as usize],
        };
        let carried = borsh::to_vec(&object).expect("the object is borsh-serializable");
        assert_eq!(carried.len() as u64, palw_receipt_licensed_wire_bytes_v1(PALW_V2_PANEL_QUORUM as u64));
        assert_eq!(carried.len(), 14_385, "one licensed claim's whole on-chain receipt carriage");
    }

    /// **The number a designer reaching for "just use more claims" has to look at.**
    ///
    /// ADR-0080's refutation put a 37-segment answer at "111 ML-DSA-87 receipts at 4,627 bytes —
    /// about 514 KB of signatures". Reproduced here, and CORRECTED in two ways:
    ///
    /// * the signature total is 513,597 bytes exactly, which is 514 kB decimal but only 501.6
    ///   KiB — the ADR's "514 KB" is right in the unit it did not name;
    /// * signatures are not the bill. The chain carries `ReceiptLicensed` objects, whose
    ///   envelopes add 18,648 bytes, for **532,245 bytes** of consensus objects. The ADR
    ///   understated by the envelope.
    ///
    /// **And 532,245 is still a FLOOR, not a total.** It counts the receipt carriage only. It does
    /// not count the 37 commitment transactions, the 37 `PanelBound` objects, the 37 court closes
    /// a disputed answer would need (each of which ADR-0080 §1 measured as full-size, not 1/37th),
    /// or the 37 flat exposure reservations and 740 interval replays the sibling tests price
    /// separately. Understating this number once already cost an ADR its mechanism; the honest
    /// statement is that the cheapest half of the bill is half a megabyte.
    ///
    /// For scale, the carrier this whole exercise exists to respect is
    /// [`crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES`] = 80 KiB (81,920): the receipt carriage
    /// alone for a 37-way split
    /// is 6.5 court closes, and it is the part nobody was arguing about.
    #[test]
    fn a_thirty_seven_way_split_costs_half_a_megabyte_of_signatures() {
        let cost = palw_segmentation_cost_v1(37, 0);
        assert_eq!(cost.quorum_receipts, 111, "37 panels x 3 receipts to quorum");
        assert_eq!(cost.receipt_signature_bytes, 513_597, "111 x 4,627 — the ADR's number, reproduced");
        assert_eq!(cost.receipt_signature_bytes / 1_000, 513, "513.6 kB decimal: the ADR's '514 KB' rounds this");
        assert_eq!(cost.receipt_signature_bytes / 1_024, 501, "501.6 KiB — NOT 514 KiB; the ADR did not name its unit");

        assert_eq!(cost.licensed_object_bytes, 532_245, "the real carriage: 37 x 14,385");
        assert_eq!(
            cost.licensed_object_bytes - cost.receipt_signature_bytes,
            18_648,
            "the envelope the ADR left out: 37 x (1 tag + 64 claim + 4 len) + 111 x 145 bytes of receipt frame"
        );

        let carrier = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;
        assert_eq!(carrier, 81_920, "the 80 KiB carrier this whole exercise exists to respect");
        assert!(
            cost.licensed_object_bytes > 6 * carrier,
            "the receipts alone outweigh six of the 80 KiB carriers the segmentation was for"
        );

        // The one-claim baseline, so the multiple is on the record rather than in a reader's head.
        let single = palw_segmentation_cost_v1(1, 0);
        assert_eq!(single.licensed_object_bytes, 14_385);
        assert_eq!(cost.licensed_object_bytes, 37 * single.licensed_object_bytes);
        assert_eq!(cost.seat_interval_replays, 37 * single.seat_interval_replays);
    }

    /// The helper this module prices with is the transition's own arithmetic, taken from
    /// `apply_free_prompt` (`palw_state_v2.rs:6808-6814`). If the transition ever prices a claim
    /// differently, every number above is wrong — so the equality is asserted rather than assumed.
    #[test]
    fn the_price_here_is_the_transitions_price() {
        for (leaves, pwu_per_inference) in [(1_600u64, 1_600u64), (7, 160), (51_200, 1_600), (1, 3)] {
            let quantum = fp_class_quantum_leaves_v1(pwu_per_inference, QUANTA_PER_CANONICAL_JOB);
            let quanta = fp_quanta_v3(leaves, quantum, MAX_QUANTA_PER_RECEIPT);
            assert_eq!(price(leaves, pwu_per_inference), (quanta, quantum, quanta as u64 * quantum));
            // And the class's rule answers the leaf count the transition feeds the quantum.
            assert_eq!(class(pwu_per_inference, 5).pwu_rule.canonical_leaves_v1(), pwu_per_inference);
        }
    }
}
