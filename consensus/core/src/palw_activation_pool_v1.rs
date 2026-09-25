//! **The Activation Pool and the listing rules it rides on** (ADR-0152-adjacent: Activation Pool,
//! user decision 2026-09-25; the adversarial review's §4 "Minimal spec", its findings cited by id).
//!
//! A registration is a long-lived, asynchronous LISTING: nothing between a class's registration and
//! its first panel may be a deadline, and whoever objectively proves they prepared the model is paid
//! for it — never for the yes or no they then vote. `Params::palw_activation_pool` (`Some(0)` on
//! testnet-12 alone, genesis-only) arms three rules at once:
//!
//! * **R1 (the P1 fix):** silence reclamation (`apply_class_reclamation`) never reclaims a class
//!   whose registry row exists and does not admit claims — a `Candidate`, `Registered`,
//!   `Prefetching` or `Held` class cannot produce, so its silence is the network's absence, not the
//!   class's — nor a genesis row (no registrant bond), like the floor;
//! * **R2 (the review's M2):** the registry's span step skips the rows of `Dormant` and `Frozen`
//!   classes, and a `Candidate` row reads its ready seats and its jury only at its OWN staggered
//!   audit span, so the per-span cost of a listing does not grow with the listings nobody audits;
//! * **the pool itself** (the later sections).
//!
//! Below the fence — every network but testnet-12 — nothing here is reached, and every fold is
//! byte-identical to a build without this module.
//!
//! TODO(activation-pool v2): an audit whose span before recorded no seed anchor is SKIPPED, not
//! deferred (the review's M5) — at t12's anchor rate (λ ≈ 0.9 a span) about 41 % of a Candidate's
//! audits never sit. v2: defer a skipped audit to the first anchored span at or after its due span
//! (a per-row "audit owed since" field in the pool row), so the rate a listing meets its jury is
//! the period's and not the anchor lottery's.

use crate::Hash64;

/// **The pool's terms** — every number the rules below read, carried beside the fence in
/// `Params::palw_activation_pool` so a network states them once and every node folds one answer.
/// The defaults ([`PALW_ACTIVATION_POOL_TERMS_V1`]) are the user's illustrative scale of 2026-09-25;
/// they are the values to tune, not derivations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwActivationPoolTermsV1 {
    /// `A0`: the preparation reward's base per payee, in sompi. `A_MAX` ramps from `A0` at the
    /// pool's opening to `3·A0` after [`Self::ramp_daa`] (the waiting bonus).
    pub prep_base_sompi: u64,
    /// `α`: the preparation budget's share of every inflow, in permille; the rest is the bonus's.
    pub prep_share_permille: u16,
    /// `β`: the share of the bonus budget one activation event pays, in permille; the rest stays
    /// for a later re-formation and later sponsors.
    pub bonus_share_permille: u16,
    /// `W`: the DAA over which `A_MAX` ramps from `A0` to `3·A0` (5,040 DAA = 7 days at 120 s).
    pub ramp_daa: u64,
    /// The most operators one class's preparation reward is ever paid to (once each).
    pub prep_payee_cap: u16,
    /// The most operators one class's activation bonus is ever paid to (once each).
    pub bonus_payee_cap: u16,
    /// The least a top-up may add, in sompi.
    pub min_topup_sompi: u64,
    /// **`b_cap`: the most (b) pays one operator**, in sompi (the pool's P1, user decision
    /// 2026-09-25): `b = min(⌊bonus × β / n⌋, b_cap)`, and what the cap leaves stays in `bonus` for
    /// later operators and a re-formation. [`palw_activation_bonus_cap_v1`] of the heaviest claim's
    /// escrow — an activation pays an operator no more than one `Final` of the dearest class pays a
    /// seat, so a sponsor's pool cannot make a probation run worth more than the work it verified.
    pub bonus_cap_sompi: u64,
}

/// **The pool's terms on testnet-12** — the user's illustrative scale of 2026-09-25 (`A0 = 20 MSK`,
/// `α = 400‰`, `β = 500‰`, `W = 5,040 DAA`, a (a) payee cap of 64, a 1 MSK least top-up), with the
/// P1 decision of the same day derived, not typed:
///
/// * **the (b) payee cap is 50** — `probation_claims × seat_count` (10 × 5), every operator one
///   probation run can credit, so the cap never turns a credited operator away;
/// * **`b_cap` is 128.03 MSK** — the per-`Final` seat pay at the heaviest class: `E × 200 ‰ / 5`,
///   `E` the 3,200.85 MSK a genesis-era claim escrows (`PALW_T12_GENESIS_CLAIM_ESCROW_SOMPI`).
pub const PALW_ACTIVATION_POOL_TERMS_V1: PalwActivationPoolTermsV1 = PalwActivationPoolTermsV1 {
    prep_base_sompi: 20 * crate::constants::SOMPI_PER_KASPA,
    prep_share_permille: 400,
    bonus_share_permille: 500,
    ramp_daa: 5_040,
    prep_payee_cap: 64,
    bonus_payee_cap: crate::palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1.probation_claims as u16
        * crate::palw_fp_devnet_v3::PALW_V2_PANEL_SEATS,
    min_topup_sompi: crate::constants::SOMPI_PER_KASPA,
    bonus_cap_sompi: palw_activation_bonus_cap_v1(
        crate::config::params::PALW_T12_GENESIS_CLAIM_ESCROW_SOMPI,
        crate::palw_fp_devnet_v3::PALW_V2_PANEL_SEATS,
    ),
};

/// **`b_cap` from an escrow**: the per-`Final` seat pay of a claim escrowing `escrow` —
/// `palw_panel_split_v1(escrow, seat_count, 0).per_seat`, the panel pool's
/// `PALW_PANEL_POOL_PERMILLE_V1` (200 ‰) of it over the seats a panel is drawn with, in `const`
/// form (pinned equal to the split by `the_default_terms_are_the_users_scale_and_run`). 0 for no
/// seat.
pub const fn palw_activation_bonus_cap_v1(escrow: u64, seat_count: u16) -> u64 {
    if seat_count == 0 {
        return 0;
    }
    let pool = (escrow as u128 * crate::palw_panel_economy_v1::PALW_PANEL_POOL_PERMILLE_V1 as u128 / 1_000) as u64;
    pool / seat_count as u64
}

impl Default for PalwActivationPoolTermsV1 {
    fn default() -> Self {
        PALW_ACTIVATION_POOL_TERMS_V1
    }
}

impl PalwActivationPoolTermsV1 {
    /// Why these terms cannot run, or `None`. A permille past 1,000 would pay out more than a
    /// budget holds; a zero cap is a pool that can never pay; a zero ramp divides by nothing; a
    /// zero least top-up admits a row per dust output.
    pub fn refusal(&self) -> Option<&'static str> {
        if self.prep_share_permille > 1_000 {
            return Some("palw_activation_pool's prep_share_permille (α) is past 1,000 ‰");
        }
        if self.bonus_share_permille == 0 || self.bonus_share_permille > 1_000 {
            return Some("palw_activation_pool's bonus_share_permille (β) is not in 1..=1,000 ‰");
        }
        if self.ramp_daa == 0 {
            return Some("palw_activation_pool's ramp_daa (W) is zero");
        }
        if self.prep_payee_cap == 0 || self.bonus_payee_cap == 0 {
            return Some("palw_activation_pool's payee caps must both be positive");
        }
        if self.prep_payee_cap as usize > PALW_ACTIVATION_PAYEE_CAP_MAX_V1
            || self.bonus_payee_cap as usize > PALW_ACTIVATION_PAYEE_CAP_MAX_V1
        {
            return Some("palw_activation_pool's payee caps are past the structural 256");
        }
        if self.min_topup_sompi == 0 {
            return Some("palw_activation_pool's min_topup_sompi is zero");
        }
        if self.prep_base_sompi == 0 {
            return Some("palw_activation_pool's prep_base_sompi (A0) is zero");
        }
        if self.bonus_cap_sompi == 0 {
            return Some("palw_activation_pool's bonus_cap_sompi (b_cap) is zero: (b) could never pay");
        }
        None
    }
}

// ---------------------------------------------------------------------------------------------
// R2: every Candidate meets its jury at its own span (the review's M2 and the design's burst fix)
// ---------------------------------------------------------------------------------------------

/// The key of a class's audit offset (`H(domain ‖ class_id)`).
pub const PALW_ADMISSION_AUDIT_STAGGER_DOMAIN_V1: &[u8] = b"misaka-palw/admission-audit/stagger/v1";

/// **R2: where in the audit period a class's audit falls** — `H(domain ‖ class_id) mod period`,
/// the first eight bytes of the keyed BLAKE2b-512 read little-endian. Stateless: nothing records the
/// offset and nothing can move it, so a registrant cannot choose its audit span except by choosing
/// its class id — which it already chooses, and which buys it one fixed span in the period, the
/// same one draw every period as before.
pub fn palw_admission_audit_offset_v1(class_id: &Hash64, period_spans: u64) -> u64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_ADMISSION_AUDIT_STAGGER_DOMAIN_V1).to_state();
    state.update(class_id.as_byte_slice());
    let digest = state.finalize();
    let mut word = [0u8; 8];
    word.copy_from_slice(&digest.as_bytes()[..8]);
    u64::from_le_bytes(word) % period_spans.max(1)
}

/// **R2: is `span_now` this class's audit span** — `(span + H(class_id)) mod period == 0`, span zero
/// never (the jury's seed is the anchor of the span before, and there is none). The period is the
/// unstaggered one ([`crate::palw_model_registry_v1::palw_admission_audit_period_spans_v2`]), so a
/// class still meets exactly one jury per period; only WHICH span of the period it is moves, and
/// the network's readiness proofs for different classes stop landing in the same few blocks.
pub fn palw_admission_audit_due_staggered_v1(class_id: &Hash64, span_now: u64, period_spans: u64) -> bool {
    let period = period_spans.max(1);
    span_now > 0 && (span_now % period + palw_admission_audit_offset_v1(class_id, period)).is_multiple_of(period)
}

// ---------------------------------------------------------------------------------------------
// The pool (the review's §4 "Minimal spec", with the user's decisions of 2026-09-25)
// ---------------------------------------------------------------------------------------------
//
// **One row per class, in its own Some-only side map** (`PalwChainStateV2::activation_pools`,
// rooted in the `activation_pool/v1` block and carried in the `0xB5` tail, with delta entries 76
// and 77), opened with a zero balance when a class is bought past the fence and funded only by
// `ActivationPoolFunded` (object tag 58): MSK a carrier paid into an `OP_RETURN "MSKACT01" <class>`
// sink, bound at block validity (the review's A8 — an unbound or mis-bound activation sink makes
// the block invalid, so none can burn silently the way an unbound `MSKMDL01` can). The registrant
// funds its own pool with the same object; nothing is taken from its bond (the user: `B` stays
// 1 MSK, no deposit, no mandatory seed).
//
// **Two payouts, both once per (class, operator), both from the row's own budgets and never from
// emission:**
//
// * **(a) the preparation reward** — at a `Candidate`'s own staggered audit, to each drawn juror
//   that holds a READY population bond for the class with collateral ≥ the panel floor (ten
//   network floors: the bonds a panel can actually draw, review C3/A2-i), whose readiness proof
//   LANDED at least two spans before the audit (before the seed existed, review M6 — the span a
//   proof lands in, recorded by the fold, not the span it names: a proof may land up to 40 spans
//   after the span it names, the fix round's F4), and that is not the registrant's operator. The
//   jury is drawn from a seed its anchor's producer cannot re-roll (the fix round's F2). `a = min(A_MAX(age), ⌊prep/10⌋)`, `A_MAX` ramping from `A0` to
//   `3·A0` over `W` DAA of the pool's age (the waiting bonus; review A5), at most `seat_count × a`
//   an audit. Paid whatever the jury's verdict — the objectively proven fact is the preparation,
//   never the yes (the user's principle; review C5 is why the amount is small and fixed).
// * **(b) the activation bonus** — at `Probation → ActiveLimited`, never at `Prefetching →
//   Probation` (review A1: that population is the registrant's to fill), to each operator the
//   chain credited on the class's probe `Final`s while it was in `Probation` — the ADR-0147
//   outsider seat included — except the registrant's operator: `b = min(⌊bonus × β / n⌋, b_cap)`,
//   `b_cap` the per-`Final` seat pay at the heaviest class (128.03 MSK on testnet-12; P1, user
//   decision 2026-09-25) and what it leaves kept in `bonus`. A later `Held → … → ActiveLimited`
//   pays only operators not paid before.
//
// **A payout is SCHEDULED where it is decided and FLUSHED where the queue has room** (the fix round's
// F5). The span step moves the amount out of the budget into the row's `scheduled` and a side map
// `(class, payee payload) → sompi` (`PalwChainStateV2::activation_pool_scheduled`), marking the
// operator paid — the money is committed to it. Step 3d′, right after R-core+'s vesting moves, flushes
// scheduled amounts into `pending_payouts` using only the width vesting and the market left:
// `8 − non-market rows waiting − min(2, market rows waiting)` new keys, never past
// `PALW_V2_MAX_PENDING_PAYOUTS`. So R-core+'s queue lemma holds (vesting first, then the market's two
// slots, the pool what is left) and a 6-key vesting move is never stalled by a pool row. Payout rows
// are keyed per payee under the two-byte prefix `[0xFE, 0xFF]` — after the seat rows' `0xFE` and
// before the market's `0xFF` (review C4). Not vested, not slashable, not in the R-core+ committed
// ledger: a possession proof the chain already verified is not a claim a later conviction can reach
// (the design's B2).
//
// **Frozen** moves the row's `prep + bonus` to its own `withheld` (review C10: never into
// `panel_reserve_sompi`, whose number is ADR-0124's); a top-up that reaches a Frozen class is folded
// into `withheld` too, never refunded (F6). A Dormant class's pool stays and resumes with a
// re-registration of the class id.
//
// **Where an activation sink's MSK can still go unaccounted** (the fix round's F6, precisely): a
// top-up the fold REFUSES — a class the state does not hold, the floor, under the least top-up, or
// the fence dormant — is paid back through P-B1 to the carrier's P2PKH-ML-DSA-87 output (which the
// block rule now requires every activation carrier to have). That refund needs one payout-queue row;
// if the queue is full when the refusal is settled the row is not written and the MSK stays in the
// sink, logged by the processor. A node refuses such a carrier at its mempool and its template
// (`palw_model_market_carrier_refusal_v1`, and the P-B1 room budget), so only a carrier mined by a
// node that ignores both can reach that case.

use crate::tx::ScriptPublicKey;

/// **(b)'s record of one credited operator** (the fix round's L2/L3): the operator, the seat bond the
/// chain credited on the probe `Final` (whose payload (b) pays), and that `Final`'s claim (so a
/// conviction of the claim takes the credit back).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwActivationCreditV1 {
    pub operator: Hash64,
    pub bond: crate::palw_state_v2::PalwBondKeyV2,
    pub claim: Hash64,
}

/// **One class's pool.** `funded == prep + bonus + scheduled + paid + withheld` always (I1); the
/// operator lists are sorted and unique and capped (I3). Never removed: the id is the listing.
#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwActivationPoolV1 {
    /// The budget of (a), the preparation reward.
    pub prep_sompi: u64,
    /// The budget of (b), the activation bonus.
    pub bonus_sompi: u64,
    /// Decided and owed, not yet flushed into `pending_payouts`: exactly the sum of this class's
    /// entries in `PalwChainStateV2::activation_pool_scheduled` (I5).
    pub scheduled_sompi: u64,
    /// Every sompi ever sunk into the pool.
    pub funded_sompi: u64,
    /// Every sompi flushed into `pending_payouts` from it.
    pub paid_sompi: u64,
    /// Every sompi a freeze took out of the budgets (never minted).
    pub withheld_sompi: u64,
    /// The ramp's origin: the registration, or — for a class nobody bought — the first top-up.
    pub opened_daa: u64,
    /// Operators paid (a), sorted; at most the terms' `prep_payee_cap`.
    pub prep_paid: Vec<Hash64>,
    /// Operators paid (b), sorted; at most the terms' `bonus_payee_cap`.
    pub bonus_paid: Vec<Hash64>,
    /// Operators credited on the class's probe `Final`s during its current probation run (and,
    /// until (b) is paid out of it, after): sorted by operator, one entry each, at most
    /// `probation_claims × seat_count`.
    pub probe_credited: Vec<PalwActivationCreditV1>,
}

impl PalwActivationPoolV1 {
    /// A row opened at `daa` with nothing in it.
    pub fn opened_at(daa: u64) -> Self {
        Self { opened_daa: daa, ..Default::default() }
    }

    /// **I1**: every sompi funded is in a budget, scheduled, paid, or withheld.
    pub fn is_balanced(&self) -> bool {
        u128::from(self.funded_sompi)
            == u128::from(self.prep_sompi)
                + u128::from(self.bonus_sompi)
                + u128::from(self.scheduled_sompi)
                + u128::from(self.paid_sompi)
                + u128::from(self.withheld_sompi)
    }

    /// Whether `probe_credited` is sorted by operator with one entry each (I3).
    pub fn credits_are_sorted_unique(&self) -> bool {
        self.probe_credited.windows(2).all(|pair| pair[0].operator < pair[1].operator)
    }

    /// Credit `credit` unless its operator is already credited; `false` if it was.
    pub fn credit(&mut self, credit: PalwActivationCreditV1) -> bool {
        match self.probe_credited.binary_search_by(|held| held.operator.cmp(&credit.operator)) {
            Ok(_) => false,
            Err(at) => {
                self.probe_credited.insert(at, credit);
                true
            }
        }
    }
}

/// **The pool's global counters** — each the sum of its field over every row (I2), and balanced as
/// a row is (I1 at the level of the chain). Wide, so no sum of rows can overflow them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwActivationPoolCountersV1 {
    pub funded_sompi: u128,
    pub prep_sompi: u128,
    pub bonus_sompi: u128,
    pub scheduled_sompi: u128,
    pub paid_sompi: u128,
    pub withheld_sompi: u128,
}

impl PalwActivationPoolCountersV1 {
    pub fn is_zero(&self) -> bool {
        *self == Self::default()
    }

    /// I1 over the counters.
    pub fn is_balanced(&self) -> bool {
        self.funded_sompi == self.prep_sompi + self.bonus_sompi + self.scheduled_sompi + self.paid_sompi + self.withheld_sompi
    }

    /// The counters of `rows`, summed — what I2 compares the stored counters with.
    pub fn of_rows<'a>(rows: impl IntoIterator<Item = &'a PalwActivationPoolV1>) -> Self {
        let mut sum = Self::default();
        for row in rows {
            sum.add(row);
        }
        sum
    }

    fn add(&mut self, row: &PalwActivationPoolV1) {
        self.funded_sompi += u128::from(row.funded_sompi);
        self.prep_sompi += u128::from(row.prep_sompi);
        self.bonus_sompi += u128::from(row.bonus_sompi);
        self.scheduled_sompi += u128::from(row.scheduled_sompi);
        self.paid_sompi += u128::from(row.paid_sompi);
        self.withheld_sompi += u128::from(row.withheld_sompi);
    }

    /// The counters after one row moved from `old` to `new` — the ONE writer's update, so the
    /// counters can never be moved by anything that did not move a row.
    pub fn moved(&self, old: Option<&PalwActivationPoolV1>, new: &PalwActivationPoolV1) -> Self {
        let mut next = *self;
        if let Some(old) = old {
            next.funded_sompi -= u128::from(old.funded_sompi);
            next.prep_sompi -= u128::from(old.prep_sompi);
            next.bonus_sompi -= u128::from(old.bonus_sompi);
            next.scheduled_sompi -= u128::from(old.scheduled_sompi);
            next.paid_sompi -= u128::from(old.paid_sompi);
            next.withheld_sompi -= u128::from(old.withheld_sompi);
        }
        next.add(new);
        next
    }
}

/// Insert `id` into a sorted list; `false` if it was already there.
pub fn palw_sorted_insert_v1(list: &mut Vec<Hash64>, id: Hash64) -> bool {
    match list.binary_search(&id) {
        Ok(_) => false,
        Err(at) => {
            list.insert(at, id);
            true
        }
    }
}

/// Whether a list is strictly increasing — sorted, and so unique.
pub fn palw_sorted_unique_v1(list: &[Hash64]) -> bool {
    list.windows(2).all(|pair| pair[0] < pair[1])
}

/// **(b)'s tracking cap**: the operators one probation run can credit — `probation_claims` probe
/// `Final`s of `seat_count` seats each.
pub fn palw_activation_probe_credit_cap_v1(probation_claims: usize, seat_count: usize) -> usize {
    probation_claims.saturating_mul(seat_count.max(1))
}

/// **The structural cap on either payee list** (I3): the terms may name any cap up to it, and the
/// consistency check holds every row to it — a row's operator list is at most 256 × 64 bytes.
pub const PALW_ACTIVATION_PAYEE_CAP_MAX_V1: usize = 256;

/// **The structural cap on `probe_credited`** (I3): the fold stops at `probation_claims ×
/// seat_count` (50 on testnet-12) and never past this.
pub const PALW_ACTIVATION_PROBE_CREDITED_MAX_V1: usize = 256;

// ---- the sink (the model market's `MSKMDL01` shape, its own tag) ----------------------------------

/// The activation sink script's tag: `OP_RETURN OP_DATA8 "MSKACT01" OP_DATA64 <class id>`.
pub const PALW_ACTIVATION_SINK_TAG_V1: &[u8; 8] = b"MSKACT01";
const OP_RETURN: u8 = 0x6a;
const OP_DATA8: u8 = 0x08;
const OP_DATA64: u8 = 0x40;

/// The sink a top-up pays into (75 bytes, script version 0). Unspendable by construction; its value
/// leaves circulation when the carrier is accepted and is credited to the class's pool by the fold.
pub fn palw_activation_sink_spk_v1(class_id: &Hash64) -> ScriptPublicKey {
    let mut script = Vec::with_capacity(1 + 1 + 8 + 1 + 64);
    script.push(OP_RETURN);
    script.push(OP_DATA8);
    script.extend_from_slice(PALW_ACTIVATION_SINK_TAG_V1);
    script.push(OP_DATA64);
    script.extend_from_slice(class_id.as_byte_slice());
    ScriptPublicKey::new(0, crate::tx::ScriptVec::from_slice(&script))
}

/// The class an activation sink script names, if it is one — recognised by its EXACT script, never
/// by its shape.
pub fn palw_activation_sink_class_v1(spk: &ScriptPublicKey) -> Option<Hash64> {
    if spk.version() != 0 {
        return None;
    }
    let script = spk.script();
    if script.len() != 75
        || script[0] != OP_RETURN
        || script[1] != OP_DATA8
        || &script[2..10] != PALW_ACTIVATION_SINK_TAG_V1
        || script[10] != OP_DATA64
    {
        return None;
    }
    let mut id = [0u8; 64];
    id.copy_from_slice(&script[11..75]);
    Some(Hash64::from_bytes(id))
}

/// **The review's A8, as a block rule: why an activation sink output of `tx` is not bound**, as
/// `(output index, reason)`, or `None` when every one is. A sink is bound iff `tx` is a lifecycle
/// carrier whose payload decodes, at the current wire version, to an `ActivationPoolFunded` naming
/// that output's index, its value and the class its script names. One carrier carries one object,
/// so a second sink in the same carrier is unbound by construction.
///
/// Context-free (it reads the transaction alone), so it is answered at isolation, where a block
/// that carries an unbound sink is refused whole — never only at the mempool, and never left to the
/// fold, which could only drop the object and keep the MSK.
pub fn palw_activation_sink_binding_refusal_v1(tx: &crate::tx::Transaction) -> Option<(usize, &'static str)> {
    use crate::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    let mut sinks = tx
        .outputs
        .iter()
        .enumerate()
        .filter_map(|(i, output)| palw_activation_sink_class_v1(&output.script_public_key).map(|c| (i, c)));
    let first = sinks.next()?;
    if tx.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE {
        return Some((first.0, "an activation sink rides only a lifecycle carrier"));
    }
    // **The fix round's F6 (i): a refusal must have somewhere to go.** A top-up the fold refuses is
    // paid back to the carrier's first P2PKH-ML-DSA-87 output (`palw_model_carrier_refund_v1`, P-B1);
    // a carrier without one would leave the MSK in the sink with nobody to pay, so it is refused
    // here, at block validity, rather than logged as a burn after the fact.
    if !tx.outputs.iter().any(|output| crate::mldsa87_primitives::p2pkh_mldsa87_payload(&output.script_public_key).is_some()) {
        return Some((first.0, "an activation sink's carrier pays no P2PKH-ML-DSA-87 output a refusal could be paid back to"));
    }
    let bound = match borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload) {
        Ok(payload) if payload.version == PALW_LIFECYCLE_TX_VERSION_V2 => match payload.object {
            crate::palw_state_v2::PalwConsensusObjectV2::ActivationPoolFunded { class_id, amount, sink_index } => {
                Some((sink_index as usize, class_id, amount))
            }
            _ => None,
        },
        _ => None,
    };
    for (index, class) in std::iter::once(first).chain(sinks) {
        match bound {
            None => return Some((index, "an activation sink's carrier carries no ActivationPoolFunded")),
            Some((named, _, _)) if named != index => {
                return Some((index, "an activation sink is not the output its carrier's ActivationPoolFunded names"));
            }
            Some((_, named_class, _)) if named_class != class => {
                return Some((index, "an activation sink names another class than its carrier's ActivationPoolFunded"));
            }
            Some((_, _, amount)) if amount != tx.outputs[index].value => {
                return Some((index, "an activation sink holds another amount than its carrier's ActivationPoolFunded declares"));
            }
            Some(_) => {}
        }
    }
    None
}

// ---- the arithmetic -------------------------------------------------------------------------------

/// **An inflow's split**: `⌊amount × α / 1000⌋` to the preparation budget and the rest to the bonus —
/// while the class is a `Candidate` (`prep_open`). Once it is not, its preparation budget has no
/// payee left ((a) is paid at a Candidate's audit and nowhere else), so the whole inflow is bonus;
/// that is the same rule that moves a seated Candidate's `prep` into its `bonus`.
pub fn palw_activation_inflow_split_v1(amount: u64, prep_share_permille: u16, prep_open: bool) -> (u64, u64) {
    if !prep_open {
        return (0, amount);
    }
    let prep = (u128::from(amount) * u128::from(prep_share_permille.min(1_000)) / 1_000) as u64;
    (prep, amount - prep)
}

/// **`A_MAX(age)`**: `A0 + 2·A0·min(age, W)/W` — `A0` at the pool's opening, `3·A0` from `W` on.
pub fn palw_activation_prep_cap_v1(terms: &PalwActivationPoolTermsV1, age_daa: u64) -> u64 {
    let base = u128::from(terms.prep_base_sompi);
    let ramp = u128::from(terms.ramp_daa.max(1));
    let waited = u128::from(age_daa.min(terms.ramp_daa));
    (base + 2 * base * waited / ramp).min(u128::from(u64::MAX)) as u64
}

/// **(a)'s per-payee amount**: `min(A_MAX(age), ⌊prep / 10⌋)`.
pub fn palw_activation_prep_reward_v1(terms: &PalwActivationPoolTermsV1, prep_sompi: u64, age_daa: u64) -> u64 {
    palw_activation_prep_cap_v1(terms, age_daa).min(prep_sompi / 10)
}

/// **`P_full`'s multiplier: sixteen full preparation rewards** (the user's figure, 2026-09-25). A
/// preparation budget of `16·A_MAX` is paid at the full `A_MAX` by the first two juries of five
/// under `a = min(A_MAX, ⌊prep/10⌋)` (`16 → 11 → 6` rewards left), and the tail keeps paying at the
/// `⌊prep/10⌋` rate after them.
pub const PALW_ACTIVATION_RECOMMENDED_PREP_REWARDS_V1: u64 = 16;

/// **`P_full`, the NON-BINDING recommended pool of a listing**: `16 · A_MAX / α`, `A_MAX = 3·A0` the
/// ramp's top ([`palw_activation_prep_cap_v1`] at `W`), rounded up so that `α` of it — the inflow
/// split's `⌊amount × α / 1000⌋` — is at least `16·A_MAX` of preparation budget. 2,400 MSK at the
/// terms of 2026-09-25 (`A0` 20 MSK, `α` 400 ‰). What a sponsor is told a listing needs to pay its
/// preparers in full: nothing in the fold reads it, no top-up is refused or scaled by it, and it
/// replaces the registry's derived `registration_bond_sompi` (1,000 MSK a window span, never
/// charged) as the figure a reader is shown (user decision 2026-09-25). `0` where `α` is 0: no
/// inflow reaches (a), so no pool pays a preparer.
pub fn palw_activation_recommended_pool_sompi_v1(terms: &PalwActivationPoolTermsV1) -> u64 {
    let alpha = u128::from(terms.prep_share_permille.min(1_000));
    if alpha == 0 {
        return 0;
    }
    let a_max = u128::from(palw_activation_prep_cap_v1(terms, terms.ramp_daa));
    (u128::from(PALW_ACTIVATION_RECOMMENDED_PREP_REWARDS_V1) * a_max * 1_000).div_ceil(alpha).min(u128::from(u64::MAX)) as u64
}

/// **(b)'s per-payee amount**: `min(⌊bonus × β / 1000 / n⌋, b_cap)` — zero for no payee. The cap
/// (P1) keeps a large sponsored pool from paying one activation more per operator than a `Final` of
/// the heaviest class pays a seat; what it leaves stays in `bonus`.
pub fn palw_activation_bonus_reward_v1(terms: &PalwActivationPoolTermsV1, bonus_sompi: u64, payees: usize) -> u64 {
    if payees == 0 {
        return 0;
    }
    ((u128::from(bonus_sompi) * u128::from(terms.bonus_share_permille.min(1_000)) / 1_000 / payees as u128) as u64)
        .min(terms.bonus_cap_sompi)
}

// ---- the payout rows ------------------------------------------------------------------------------

/// **The first two bytes of every pool payout key**: `0xFE` (the seats') then `0xFF`, so a pool row
/// sorts after every seat row whose second byte is not `0xFF` and before every market row (`0xFF`):
/// the drain order stays claims → seats → pool → market (the review's C4 — the design's `0xFD`
/// sorted before the seats). One payee holds one row however many pools pay it.
pub const PALW_ACTIVATION_POOL_PAYOUT_KEY_PREFIX_V1: [u8; 2] = [0xFE, 0xFF];

/// The pool payout row's key domain (`H(domain ‖ payee payload)`). A ROW key, kept out of
/// `PALW_STATE_V2_ALL_DOMAINS` as the other payout-row domains are.
pub const PALW_STATE_V2_DOMAIN_ACTIVATION_POOL_PAYOUT: &[u8] = b"misaka-palw/state-v2/activation-pool-payout/v1";

/// **A payee's pool payout row**: `H(DOMAIN ‖ payload)` with its first two bytes forced to
/// [`PALW_ACTIVATION_POOL_PAYOUT_KEY_PREFIX_V1`].
pub fn palw_activation_pool_payout_key_v1(payload: &Hash64) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_STATE_V2_DOMAIN_ACTIVATION_POOL_PAYOUT).to_state();
    state.update(payload.as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    out[..2].copy_from_slice(&PALW_ACTIVATION_POOL_PAYOUT_KEY_PREFIX_V1);
    Hash64::from_bytes(out)
}

// ---- the read (op 200) ------------------------------------------------------------------------------

/// **One class's pool as a reader sees it at a tip** (`getPalwActivationPool`, op 200).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwActivationPoolReadV1 {
    /// The terms in force at `now_daa`; `None` where the pool is not armed.
    pub terms: Option<PalwActivationPoolTermsV1>,
    pub class_found: bool,
    /// The class is the network's floor, which takes no top-up (F3).
    pub class_is_floor: bool,
    /// `active`, `dormant`, `frozen`, `registered`, or empty for an unknown class.
    pub class_status: &'static str,
    /// The registry row's state, `Debug`-printed; `None` without a row.
    pub lifecycle: Option<String>,
    pub pool: Option<PalwActivationPoolV1>,
    pub counters: PalwActivationPoolCountersV1,
    pub registrant_operator: Option<Hash64>,
    /// (a)'s per-payee amount were this Candidate's audit now; 0 for any other class.
    pub prep_reward_now_sompi: u64,
    /// `A_MAX(age)` now (0 without a pool or terms).
    pub prep_cap_now_sompi: u64,
    /// The span this Candidate next meets its jury at (R2's stagger).
    pub next_audit_span: Option<u64>,
    pub span_daa: u64,
    pub now_daa: u64,
}

/// **The read, from a state**: the row, the terms, what (a) would pay now and — for a Candidate — the
/// first span after `now_daa`'s whose audit is due, given the registry's `(span_daa, period)`.
pub fn palw_activation_pool_read_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    class_id: &Hash64,
    base_class_id: &Hash64,
    terms: Option<PalwActivationPoolTermsV1>,
    now_daa: u64,
    schedule: Option<(u64, u64)>,
) -> PalwActivationPoolReadV1 {
    use crate::palw_model_registry_v1::PalwModelLifecycleV1;
    use crate::palw_state_v2::PalwClassStatusV2;
    let class = state.class(class_id);
    let row = state.model_lifecycle(class_id);
    let pool = state.activation_pool(class_id).cloned();
    let candidate = row.is_some_and(|row| matches!(row.state, PalwModelLifecycleV1::Candidate));
    let age = pool.as_ref().map(|pool| now_daa.saturating_sub(pool.opened_daa)).unwrap_or(0);
    let (prep_cap_now_sompi, prep_reward_now_sompi) = match (&terms, &pool) {
        (Some(terms), Some(pool)) => (
            palw_activation_prep_cap_v1(terms, age),
            if candidate { palw_activation_prep_reward_v1(terms, pool.prep_sompi, age) } else { 0 },
        ),
        _ => (0, 0),
    };
    let next_audit_span = match schedule {
        Some((span_daa, period)) if candidate && terms.is_some() => {
            let span_now = now_daa / span_daa.max(1);
            (span_now + 1..=span_now + period.max(1)).find(|span| palw_admission_audit_due_staggered_v1(class_id, *span, period))
        }
        _ => None,
    };
    PalwActivationPoolReadV1 {
        terms,
        class_found: class.is_some(),
        class_is_floor: class_id == base_class_id,
        class_status: match class.map(|c| &c.status) {
            Some(PalwClassStatusV2::Active) => "active",
            Some(PalwClassStatusV2::Dormant { .. }) => "dormant",
            Some(PalwClassStatusV2::Frozen { .. }) => "frozen",
            Some(_) => "registered",
            None => "",
        },
        lifecycle: row.map(|row| format!("{:?}", row.state)),
        pool,
        counters: state.activation_pool_counters(),
        registrant_operator: class.and_then(|c| c.registrant_bond).and_then(|key| state.bond(&key)).map(|bond| bond.operator_id),
        prep_reward_now_sompi,
        prep_cap_now_sompi,
        next_audit_span,
        span_daa: schedule.map(|(span_daa, _)| span_daa).unwrap_or(0),
        now_daa,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_terms_are_the_users_scale_and_run() {
        let t = PALW_ACTIVATION_POOL_TERMS_V1;
        assert_eq!(t.prep_base_sompi, 2_000_000_000, "A0 = 20 MSK");
        assert_eq!((t.prep_share_permille, t.bonus_share_permille), (400, 500), "α = 400 ‰, β = 500 ‰");
        assert_eq!(t.ramp_daa, 5_040, "W = 7 days of 120 s DAA");
        assert_eq!((t.prep_payee_cap, t.bonus_payee_cap), (64, 50), "(b)'s cap is P1's 50");
        assert_eq!(
            t.bonus_payee_cap as usize,
            palw_activation_probe_credit_cap_v1(
                crate::palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1.probation_claims as usize,
                crate::palw_fp_devnet_v3::PALW_V2_PANEL_SEATS as usize
            ),
            "every operator one probation run can credit"
        );
        assert!(t.bonus_payee_cap as usize <= PALW_ACTIVATION_PAYEE_CAP_MAX_V1 && 50 <= PALW_ACTIVATION_PROBE_CREDITED_MAX_V1);
        assert_eq!(t.min_topup_sompi, 100_000_000, "1 MSK");
        // P1: b_cap is the per-Final seat pay at the heaviest class, E × 200 ‰ / 5 — the split's own.
        let e = crate::config::params::PALW_T12_GENESIS_CLAIM_ESCROW_SOMPI;
        assert_eq!(t.bonus_cap_sompi, crate::palw_panel_economy_v1::palw_panel_split_v1(e, 5, 0).per_seat);
        assert_eq!(t.bonus_cap_sompi, 12_803_386_003, "128.03 MSK");
        for (escrow, seats) in [(0u64, 5u16), (1, 1), (999, 3), (e, 1), (u64::MAX, 7)] {
            assert_eq!(
                palw_activation_bonus_cap_v1(escrow, seats),
                crate::palw_panel_economy_v1::palw_panel_split_v1(escrow, seats as usize, 0).per_seat,
                "the const form is the split at {escrow} over {seats}"
            );
        }
        assert_eq!(palw_activation_bonus_cap_v1(e, 0), 0);
        assert_eq!(t.refusal(), None);
        assert!(PalwActivationPoolTermsV1 { bonus_cap_sompi: 0, ..t }.refusal().is_some());
        assert!(PalwActivationPoolTermsV1 { bonus_share_permille: 0, ..t }.refusal().is_some());
        assert!(PalwActivationPoolTermsV1 { prep_share_permille: 1_001, ..t }.refusal().is_some());
        assert!(PalwActivationPoolTermsV1 { ramp_daa: 0, ..t }.refusal().is_some());
        assert!(PalwActivationPoolTermsV1 { prep_payee_cap: 0, ..t }.refusal().is_some());
    }

    /// R2: the stagger is one span per period per class, spread over the period, and stateless.
    #[test]
    fn a_class_meets_one_jury_per_period_at_its_own_span() {
        let period = 100u64;
        let mut offsets = std::collections::BTreeSet::new();
        for n in 0..64u64 {
            let class = Hash64::from_u64_word(0xC1A5_5000 + n);
            let due: Vec<u64> = (1..=3 * period).filter(|span| palw_admission_audit_due_staggered_v1(&class, *span, period)).collect();
            assert_eq!(due.len(), 3, "one audit per period: {due:?}");
            assert!(due.windows(2).all(|w| w[1] - w[0] == period), "exactly a period apart: {due:?}");
            assert_eq!((due[0] + palw_admission_audit_offset_v1(&class, period)) % period, 0);
            offsets.insert(palw_admission_audit_offset_v1(&class, period));
        }
        assert!(offsets.len() > 40, "64 classes spread over the period, not stacked on one span: {} offsets", offsets.len());
        assert!(!palw_admission_audit_due_staggered_v1(&Hash64::from_u64_word(1), 0, period), "span zero is never an audit");
        assert!(
            (1..10).all(|span| palw_admission_audit_due_staggered_v1(&Hash64::from_u64_word(9), span, 1)),
            "a one-span period audits every span"
        );
    }

    /// The split, the ramp and the two amounts, on the user's scale.
    #[test]
    fn the_pool_arithmetic_is_the_specs() {
        let t = PALW_ACTIVATION_POOL_TERMS_V1;
        assert_eq!(
            palw_activation_inflow_split_v1(300 * 100_000_000, 400, true),
            (120 * 100_000_000, 180 * 100_000_000),
            "the design's 300 MSK: prep 120, bonus 180"
        );
        assert_eq!(palw_activation_inflow_split_v1(7, 400, true), (2, 5), "floor to prep, the rest to bonus");
        assert_eq!(palw_activation_inflow_split_v1(300, 400, false), (0, 300), "past Candidate every sompi is bonus");
        let a0 = t.prep_base_sompi;
        assert_eq!(palw_activation_prep_cap_v1(&t, 0), a0, "A0 at the opening");
        assert_eq!(palw_activation_prep_cap_v1(&t, t.ramp_daa / 2), 2 * a0, "2·A0 half way");
        assert_eq!(palw_activation_prep_cap_v1(&t, t.ramp_daa), 3 * a0, "3·A0 at W");
        assert_eq!(palw_activation_prep_cap_v1(&t, 10 * t.ramp_daa), 3 * a0, "and never past it");
        assert_eq!(palw_activation_prep_reward_v1(&t, 1_000 * 100_000_000, 0), a0, "a large budget pays A_MAX");
        assert_eq!(
            palw_activation_prep_reward_v1(&t, 50 * 100_000_000, t.ramp_daa),
            5 * 100_000_000,
            "a small one pays a tenth of itself"
        );
        assert_eq!(palw_activation_prep_reward_v1(&t, 9, 0), 0, "under ten sompi pays nothing");
        assert_eq!(
            palw_activation_bonus_reward_v1(&t, 180 * 100_000_000, 6),
            15 * 100_000_000,
            "the design's example: 6 payees, 15 MSK each"
        );
        assert_eq!(palw_activation_bonus_reward_v1(&t, 1_000, 0), 0);
        // P1: past the cap every payee is paid b_cap, however large the budget.
        let huge = 1_000_000 * 100_000_000;
        assert_eq!(palw_activation_bonus_reward_v1(&t, huge, 1), t.bonus_cap_sompi);
        assert_eq!(palw_activation_bonus_reward_v1(&t, huge, 50), t.bonus_cap_sompi);
        assert_eq!(palw_activation_bonus_reward_v1(&t, 2 * t.bonus_cap_sompi, 1), t.bonus_cap_sompi, "β of it is exactly b_cap");
        assert!(palw_activation_bonus_reward_v1(&t, 2 * t.bonus_cap_sompi - 2, 1) < t.bonus_cap_sompi, "under it, the share");
        assert_eq!(palw_activation_probe_credit_cap_v1(10, 5), 50);
    }

    /// The sink is `MSKMDL01`'s shape with its own tag, read back only off its exact script.
    #[test]
    fn the_activation_sink_is_its_exact_script() {
        let class = Hash64::from_u64_word(0xC1A5);
        let spk = palw_activation_sink_spk_v1(&class);
        assert_eq!(spk.script().len(), 75);
        assert_eq!(&spk.script()[2..10], b"MSKACT01");
        assert_eq!(palw_activation_sink_class_v1(&spk), Some(class));
        assert_eq!(
            palw_activation_sink_class_v1(&crate::palw_model_market_v1::palw_model_sink_spk_v1(&class)),
            None,
            "a market sink is not one"
        );
        assert_eq!(crate::palw_model_market_v1::palw_model_sink_class_v1(&spk), None, "nor the other way");
        let mut forged = spk.script().to_vec();
        forged[9] ^= 1;
        assert_eq!(palw_activation_sink_class_v1(&ScriptPublicKey::new(0, crate::tx::ScriptVec::from_slice(&forged))), None);
        assert_eq!(
            palw_activation_sink_class_v1(&ScriptPublicKey::new(1, crate::tx::ScriptVec::from_slice(spk.script()))),
            None,
            "script version 0 only"
        );
        assert_eq!(crate::mldsa87_primitives::p2pkh_mldsa87_payload(&spk), None, "an OP_RETURN is never a payee");
    }

    /// **P4 (user decision 2026-09-25): the recommended pool is `16·A_MAX/α`, derived** — 2,400 MSK
    /// at the terms, and `α` of it is exactly sixteen full rewards, of which the first two juries of
    /// five are paid in full.
    #[test]
    fn the_recommended_pool_is_sixteen_full_rewards_over_alpha() {
        let t = PALW_ACTIVATION_POOL_TERMS_V1;
        let msk = crate::constants::SOMPI_PER_KASPA;
        let p_full = palw_activation_recommended_pool_sompi_v1(&t);
        assert_eq!(p_full, 2_400 * msk, "16 × 60 MSK / 0.4");
        let a_max = palw_activation_prep_cap_v1(&t, t.ramp_daa);
        assert_eq!(a_max, 3 * t.prep_base_sompi, "A_MAX is the ramp's top");
        let (mut prep, _) = palw_activation_inflow_split_v1(p_full, t.prep_share_permille, true);
        assert_eq!(prep, 16 * a_max);
        for jury in 0..2 {
            let a = palw_activation_prep_reward_v1(&t, prep, t.ramp_daa);
            assert_eq!(a, a_max, "jury {jury} is paid the full A_MAX");
            prep -= 5 * a;
        }
        assert!(palw_activation_prep_reward_v1(&t, prep, t.ramp_daa) < a_max, "the third is not");
        // Rounded up: α of the figure never falls short of sixteen rewards.
        let odd = PalwActivationPoolTermsV1 { prep_share_permille: 333, ..t };
        let p_odd = palw_activation_recommended_pool_sompi_v1(&odd);
        assert!(palw_activation_inflow_split_v1(p_odd, 333, true).0 >= 16 * a_max);
        assert!(palw_activation_inflow_split_v1(p_odd - 1, 333, true).0 < 16 * a_max);
        assert_eq!(palw_activation_recommended_pool_sompi_v1(&PalwActivationPoolTermsV1 { prep_share_permille: 0, ..t }), 0);
    }

    /// The pool payout key sorts after the seats' `0xFE` rows and before the market's `0xFF`.
    #[test]
    fn a_pool_payout_row_drains_after_the_seats_and_before_the_market() {
        let payee = Hash64::from_u64_word(0x9A01);
        let key = palw_activation_pool_payout_key_v1(&payee);
        assert_eq!(&key.as_byte_slice()[..2], &[0xFE, 0xFF]);
        let seat = crate::palw_state_v2::palw_panel_payout_key_v1(&payee);
        assert_eq!(seat.as_byte_slice()[0], crate::palw_state_v2::PALW_STATE_V2_PANEL_PAYOUT_KEY_PREFIX);
        if seat.as_byte_slice()[1] != 0xFF {
            assert!(seat < key, "a seat row drains first");
        }
        let mut market = [0xFFu8; 64];
        market[1] = 0x00;
        assert!(key < Hash64::from_bytes(market), "and every market row after it");
        assert_ne!(palw_activation_pool_payout_key_v1(&Hash64::from_u64_word(0x9A02)), key, "one row per payee");
    }

    /// **The review's A8 as a block rule**: an activation sink is bound, or refused by name.
    #[test]
    fn an_activation_sink_is_bound_or_refused() {
        use crate::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
        use crate::palw_state_v2::PalwConsensusObjectV2;
        use crate::subnets::{SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_PALW_LIFECYCLE};
        use crate::tx::{Transaction, TransactionOutput};
        let class = Hash64::from_u64_word(0xC1A5);
        let payee = crate::mldsa87_primitives::p2pkh_mldsa87_spk(&[0x11; 64]);
        let carrier = |object: Option<PalwConsensusObjectV2>, outputs: Vec<TransactionOutput>, subnet| {
            let payload = object
                .map(|object| borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap())
                .unwrap_or_default();
            Transaction::new(0, vec![], outputs, 0, subnet, 0, payload)
        };
        let funded = |amount, sink_index, class_id| Some(PalwConsensusObjectV2::ActivationPoolFunded { class_id, amount, sink_index });
        let outs =
            |value| vec![TransactionOutput::new(5, payee.clone()), TransactionOutput::new(value, palw_activation_sink_spk_v1(&class))];
        assert_eq!(
            palw_activation_sink_binding_refusal_v1(&carrier(funded(700, 1, class), outs(700), SUBNETWORK_ID_PALW_LIFECYCLE)),
            None,
            "bound"
        );
        assert_eq!(
            palw_activation_sink_binding_refusal_v1(&carrier(
                None,
                vec![TransactionOutput::new(5, payee.clone())],
                SUBNETWORK_ID_NATIVE
            )),
            None,
            "no sink, no question"
        );
        for (tx, why) in [
            (carrier(None, outs(700), SUBNETWORK_ID_NATIVE), "rides only a lifecycle carrier"),
            (carrier(None, outs(700), SUBNETWORK_ID_PALW_LIFECYCLE), "carries no ActivationPoolFunded"),
            (carrier(funded(700, 0, class), outs(700), SUBNETWORK_ID_PALW_LIFECYCLE), "is not the output"),
            (carrier(funded(701, 1, class), outs(700), SUBNETWORK_ID_PALW_LIFECYCLE), "another amount"),
            (carrier(funded(700, 1, Hash64::from_u64_word(9)), outs(700), SUBNETWORK_ID_PALW_LIFECYCLE), "another class"),
        ] {
            let refusal = palw_activation_sink_binding_refusal_v1(&tx);
            assert!(refusal.is_some_and(|(index, reason)| index == 1 && reason.contains(why)), "{why}: {refusal:?}");
        }
        // **The review's P4 / the fix round's F6 (i)**: a bound carrier with no P2PKH-ML-DSA-87 output
        // would have nowhere to pay a refusal back — refused at block validity.
        let no_change = carrier(
            funded(700, 0, class),
            vec![TransactionOutput::new(700, palw_activation_sink_spk_v1(&class))],
            SUBNETWORK_ID_PALW_LIFECYCLE,
        );
        assert!(
            palw_activation_sink_binding_refusal_v1(&no_change)
                .is_some_and(|(index, why)| index == 0 && why.contains("P2PKH-ML-DSA-87")),
            "{:?}",
            palw_activation_sink_binding_refusal_v1(&no_change)
        );
        // Two sinks in one carrier: one object binds one of them, the other is unbound.
        let mut two = outs(700);
        two.push(TransactionOutput::new(700, palw_activation_sink_spk_v1(&class)));
        let refusal = palw_activation_sink_binding_refusal_v1(&carrier(funded(700, 1, class), two, SUBNETWORK_ID_PALW_LIFECYCLE));
        assert_eq!(refusal.map(|(index, _)| index), Some(2), "the second sink is the unbound one");
    }

    /// The counters' one update is the rows' sum, balanced.
    #[test]
    fn the_counters_move_with_their_row() {
        let old = PalwActivationPoolV1 { prep_sompi: 40, bonus_sompi: 60, funded_sompi: 100, ..PalwActivationPoolV1::opened_at(5) };
        let new = PalwActivationPoolV1 { prep_sompi: 30, scheduled_sompi: 6, paid_sompi: 4, ..old.clone() };
        assert!(old.is_balanced() && new.is_balanced());
        let base = PalwActivationPoolCountersV1::of_rows([&old]);
        let moved = base.moved(Some(&old), &new);
        assert_eq!(moved, PalwActivationPoolCountersV1::of_rows([&new]));
        assert!(moved.is_balanced());
        assert!(!PalwActivationPoolV1 { funded_sompi: 99, ..new.clone() }.is_balanced(), "I1 catches a sompi out of place");
        // Credits are one per operator, sorted.
        let mut row = new;
        let bond = crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Default::default(), 0));
        let credit = |n: u64| PalwActivationCreditV1 { operator: Hash64::from_u64_word(n), bond, claim: Hash64::from_u64_word(99) };
        assert!(row.credit(credit(3)) && row.credit(credit(1)) && !row.credit(credit(3)));
        assert!(row.credits_are_sorted_unique() && row.probe_credited.len() == 2);
    }
}
