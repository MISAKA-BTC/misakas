//! **What a block producer must read from chain state, and it reads it derived** (ADR-0042).
//!
//! A `ConsensusV2` attempt is refused unless six of its fields equal values the chain already
//! holds: the class's registered artifact root, the class target the per-class retarget maintains,
//! the pwu `palw_pwu_v1` computes from that target, the bond's registered verification key, the
//! operator id minted at registration, and — as a bound rather than an equality — what the bond's
//! collateral still has room to back. A producer that computes any of them from a second source
//! computes them wrong the first time the chain moves.
//!
//! So it does not compute them. [`PalwProducerFactsV2`] is assembled by the same code paths
//! admission uses, at the same chain point a block template builds on, and handed over whole. The
//! producer's only remaining freedom is its EXECUTION — which is the freedom the design means by
//! "work".
//!
//! # Why this is not an RPC type
//!
//! It lives beside the state it is read from because the derivation is the contract. Exposing the
//! ingredients (a target here, a rule there) and letting a miner multiply them would hand every
//! miner an independent chance to disagree with admission — the same shape of defect the audit
//! found five times over between the engine, the profile, the inventory and the court, and the
//! reason ADR-0046 wrote down "derive, never declare".

use crate::BlockHash;
use crate::palw_admission_v2::PalwAdmissionParamsV2;
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwPwuRuleV2, PalwStateParamsV2};
use kaspa_hashes::Hash64;

/// The bond half — read only when a producer names the bond it intends to sign under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwProducerBondFactsV2 {
    /// The ML-DSA-87 verification key the bond registered. Admission item 2 compares the carried
    /// key against this one, so a producer whose local key does not match it can be told at
    /// startup instead of after a block dies.
    pub registered_pubkey: Vec<u8>,
    /// Minted at registration from the operator key; admission item 3 is an equality.
    pub operator_id: Hash64,
    pub collateral: u64,
    /// What this bond already backs.
    pub reserved_exposure: u128,
    /// `collateral × max_exposure_ratio_permille / 1000` — admission item 8's ceiling.
    pub exposure_ceiling: u128,
    /// What ONE attempt at [`PalwProducerFactsV2::pwu`] would add to `reserved_exposure`.
    pub claim_exposure: u128,
    /// **ADR-0152 SR-7 (S-SPEC §2, P6): what the attempt ceiling measures this bond's backing as at
    /// the candidate DAA** — past `Params::palw_rcore_plus` the one committed ledger
    /// (`palw_bond_committed_v1` at the escaped depth of the raw depth [`palw_producer_facts_v4`]
    /// was handed), below it `reserved_exposure + registration_exposure` (admission item 8's
    /// `reserved`). Admission and the fold refuse `committed + claim_exposure > exposure_ceiling`,
    /// so a node that pre-checks against this never mines an attempt the chain refuses (T08).
    pub committed: u128,
    /// **ADR-0152 U2 (S-SPEC §10a): the producer floor's shortfall** —
    /// `palw_bond_producer_floor_shortfall_v1` at the candidate DAA: `None` meets the floor (or the
    /// fence is dormant), `Some(sompi)` is how far the posted collateral is below it. Past the fence
    /// admission and the fold refuse the attempt (`ProducerBelowFloor`, non-fatal for the own
    /// attempt) while this is `Some`.
    pub producer_floor_shortfall: Option<u64>,
    /// **ADR-0152 A-6: what this bond holds as an accuser** (`palw_accuser_exposure_v1`) past
    /// `Params::palw_rcore_plus`, `0` below it. The work gate never lets `committed + accuser` pass
    /// the collateral (the S review's M1).
    pub accuser_exposure: u128,
}

impl PalwProducerBondFactsV2 {
    /// Is there ceiling left for one more claim? Admission item 8 is `reserved + claim <= ceiling`.
    pub fn has_exposure_room(&self) -> bool {
        self.reserved_exposure.saturating_add(self.claim_exposure) <= self.exposure_ceiling
    }

    /// **The ceiling as admission and the fold measure it** (ADR-0152 SR-7): `committed + claim <=
    /// ceiling` and, past the fence, `committed + accuser + claim <= collateral` (the one invariant's
    /// work gate, `palw_rcore_gate_room_of_v1`). Equal to [`Self::has_exposure_room`] below the fence
    /// up to registration exposure, which admission item 8 always counted (the accuser ledger is `0`
    /// there and the ratio is at most 1000‰, so the second clause is implied).
    pub fn has_committed_room(&self) -> bool {
        self.committed.saturating_add(self.claim_exposure) <= self.exposure_ceiling
            && self.committed.saturating_add(self.accuser_exposure).saturating_add(self.claim_exposure) <= self.collateral as u128
    }
}

/// Everything a producer needs from the chain, at one chain point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwProducerFactsV2 {
    /// The block these facts were read at — virtual's selected parent, which is the point a block
    /// template builds on. A producer that sees this move knows its template is stale without
    /// having to guess from a timestamp.
    pub chain_point: BlockHash,
    pub daa_score: u64,
    pub class_id: Hash64,
    /// Admission item 5: equality against the attempt's.
    pub artifact_root: Hash64,
    /// Admission item 6b: the attempt's `class_ticket_v3` (drawn from its execution under the
    /// header's anchor, ADR-0072) must land at or under this.
    pub class_target: u128,
    /// Admission item 6: an EQUALITY, and both factors are chain state. Derived here so the
    /// producer cannot pick.
    pub pwu: u64,
    /// **Is this class seated on the free-prompt lane** (ADR-0075 `ClassLaneCertified`, genesis
    /// set ∪ chain set)? What `FreePromptCommitted` refuses as `FreePromptLaneUncertified`, read
    /// off the same two sets the transition reads, so a gateway (ADR-0077 Decision 3) learns
    /// before it commits whether its class can take a free-prompt claim at all. A params set
    /// with no certified-class gate (the ungated test bundles) reads as certified.
    pub fp_certified: bool,
    pub epoch_index: u64,
    /// Admission item 7: blocks of this class this epoch may not exceed this.
    pub epoch_budget_blocks: u64,
    pub epoch_produced_blocks: u64,
    /// **ADR-0123: the slots this class may borrow past its budget**, from
    /// [`crate::palw_state_v2::palw_epoch_budget_release_v1`] — the function admission calls. Always
    /// computed, because it is a pure function of the state this view already holds; whether it
    /// COUNTS is [`Self::epoch_budget_release_armed`], which only a caller holding `Params` can say.
    pub epoch_budget_released: u64,
    /// `Params::palw_epoch_budget_release` resolved at [`Self::daa_score`]. The builder cannot know
    /// it — it holds no `Params`, and must not, or this view would be a second place a fence is
    /// read — so it is `false` here and set by the consensus API that answers the question, which is
    /// the one path every producer and the RPC take.
    pub epoch_budget_release_armed: bool,
    /// ADR-0137: whether the chain still reads the epoch budget at this point. Past
    /// `Params::palw_work_target` admission prices a claim against `CCU / W` and reads no class
    /// budget, so the producer must not hold on one either — the devnet drill of 2026-09-18 found
    /// the producer holding "epoch budget spent" past the fence while admission would have
    /// accepted, and the class never produced a claim.
    pub epoch_budget_read: bool,
    /// Is this the liveness floor? **The floor is EXEMPT from the epoch budget** — admission says so
    /// at `palw_admission_v2.rs:234`, and the exemption is what makes ADR-0039 W6′'s deadlock
    /// unrepresentable: DAA only advances when blocks are produced, so a floor that could be capped
    /// could stop the chain and then never reach the epoch that would uncap it.
    ///
    /// Carried because a producer that applied the cap anyway would re-create that deadlock on the
    /// CLIENT side — refusing to build a block the chain would have accepted. It did: the budget
    /// table is written for the TIP's epoch and looked up for the CANDIDATE's, so at every epoch
    /// boundary the lookup missed, `unwrap_or(0)` made the budget zero, and the producer held
    /// forever.
    pub is_base_class: bool,
    /// How long a producer must promise to keep its trace: [`palw_min_trace_retention_daa_v1`].
    /// The attempt's `trace_retention_daa` MUST be the block's own DAA score plus this — admission
    /// pins it by equality (ADR-0072 Decision 8), so it is not the producer's to get right but the
    /// chain's, and this is where a producer reads it.
    pub min_trace_retention_daa: u64,
    pub bond: Option<PalwProducerBondFactsV2>,

    /// **The chain's PALW weight, and how many claims are still unresolved** — the two numbers that
    /// say whether this network is doing PALW at all.
    ///
    /// `safe_weight` is what fork choice orders by. It leaves zero only when a claim reaches
    /// `Final`, which needs a panel, receipts, a quorum and a submitted `ReceiptLicensed` — the
    /// whole lattice. A network producing blocks with `safe_weight == 0` is indistinguishable from
    /// a hash chain wearing PALW's clothes, and until this field existed there was no way to see
    /// that from outside a debugger: nothing logged it, no RPC returned it, and a fleet could run
    /// for a day looking healthy while every claim it ever made was quietly voiding.
    pub safe_weight: u128,
    /// Claims created and not yet resolved. Rising without bound while `safe_weight` stays zero is
    /// the signature of a lattice that never turns over.
    pub unresolved_claims: u64,
    /// **How many disputes are open right now.** Not decoration: a network whose challengers are
    /// working and whose responders are not looks, from every other number here, exactly like a
    /// network with nothing to dispute. Two drills were spent reading "the responder made no move"
    /// as a responder bug when the sessions may not have existed at all — this is the number that
    /// tells those apart from the log an operator already watches.
    pub open_courts: u64,
    /// Claims that have reached `Final` — the count of work this chain has actually certified.
    pub final_claims: u64,
    /// `safe_weight` plus the bounded immature contribution — the THIRD key of the fork-choice
    /// order (`palw_fork_choice::PalwCandidateOrderV1`), and the only one that can move on a
    /// young chain.
    ///
    /// A claim cannot finalize before `window_challenge` has passed, so `safe_weight` and the
    /// safe frontier are both zero for the whole first stretch of a network's life — at the
    /// frozen 120 s cadence, more than a day. That is by construction, not a fault, and this is
    /// how an operator tells the two apart: `live_total` climbing while `safe_weight` sits at
    /// zero is a chain ordering on immature PALW work exactly as designed, whereas both at zero
    /// is a chain whose lifecycle never started.
    pub live_total: u128,
    /// **Why the chain would refuse a new claim of this class now, if it would** — the fold's own
    /// class gate (`palw_class_admits_claim_v1`: the registry row's lifecycle, then its panel room
    /// or inflight cap) and, for a named bond past ADR-0152 R-core+, its T-2(a) share of the class
    /// (`palw_bond_class_share_admits_v1`, which the fold asks next), filled by the caller that
    /// holds the block's fences. `None` is "admits". Without it a class the registry held at
    /// `Prefetching` reported no reason not to produce, and its producer mined claims its own chain
    /// refused (the 2026-09-23 route-matrix audit's #7).
    pub class_admission_refusal: Option<String>,
}

impl PalwProducerFactsV2 {
    /// Is the epoch budget spent? A producer that keeps mining past it produces blocks admission
    /// refuses — burning an inference each time and learning nothing.
    pub fn has_epoch_room(&self) -> bool {
        // Past the work target no budget is read at all (ADR-0137): room is not a question.
        if !self.epoch_budget_read {
            return true;
        }
        // The floor is exempt, exactly as admission exempts it. See `is_base_class`.
        let released = if self.epoch_budget_release_armed { self.epoch_budget_released } else { 0 };
        self.is_base_class || self.epoch_produced_blocks < self.epoch_budget_blocks.saturating_add(released)
    }

    /// **The receipt lane's preconditions, which are a strict subset of the attempt lane's.**
    ///
    /// A receipt block spends one quantum of a free-prompt claim that is already `Final`: it
    /// opens no claim, so it reserves no exposure, and it is counted in the receipt lane's own
    /// epoch census rather than the attempt class's budget (`apply_receipt_spend`). The two
    /// attempt-only conditions below therefore say nothing about it — and a producer that
    /// consulted [`Self::ready_to_produce`] before trying its receipts held its certified quanta
    /// back for exactly as long as its ATTEMPT lane was full. A quantum's win is spendable only
    /// inside its use window (`fp_spend_window_contains_v3`, "a win outside the window licenses
    /// nothing, forever"), so that hold did not delay the free-prompt executor's pay, it cancelled
    /// it — and the bond most likely to hold on exposure is precisely the one committing
    /// free-prompt claims, since those fill the same ceiling.
    pub fn ready_to_spend_receipts(&self, local_pubkey: &[u8]) -> Result<(), &'static str> {
        let bond = self.bond.as_ref().ok_or(PALW_NOT_READY_BOND_UNKNOWN_V2)?;
        if bond.registered_pubkey != local_pubkey {
            return Err(PALW_NOT_READY_KEY_MISMATCH_V2);
        }
        Ok(())
    }

    /// Every stateful precondition a producer can check BEFORE running an inference, in one
    /// answer. `Ok(())` is not a promise the block lands — the chain can move underneath it —
    /// but each `Err` is a reason it certainly would not have.
    pub fn ready_to_produce(&self, local_pubkey: &[u8]) -> Result<(), &'static str> {
        self.ready_to_spend_receipts(local_pubkey)?;
        let bond = self.bond.as_ref().ok_or(PALW_NOT_READY_BOND_UNKNOWN_V2)?;
        if self.class_admission_refusal.is_some() {
            return Err(PALW_NOT_READY_CLASS_NOT_ADMITTING_V2);
        }
        if !self.has_epoch_room() {
            return Err(PALW_NOT_READY_EPOCH_BUDGET_V2);
        }
        if !bond.has_exposure_room() {
            return Err(PALW_NOT_READY_EXPOSURE_FULL_V2);
        }
        Ok(())
    }
}

// **The four verdicts, by name** (ADR-0122 Decision 3). The sentences are the ones this function
// has always returned — the producer logs them after `holding:` and `getPalwProducerFacts` serves
// them as `not_ready_reason` — and the operator CLI turns each into a code and a fix. It matches on
// these constants rather than on a copy of the words, so rewording one breaks the CLI's build
// instead of quietly turning a known hold into an unrecognised one.

/// `ready_to_produce` / `ready_to_spend_receipts`: the chain has no bond at the named outpoint.
pub const PALW_NOT_READY_BOND_UNKNOWN_V2: &str = "the named bond is not registered on this chain";
/// `ready_to_produce` / `ready_to_spend_receipts`: the bond exists and registered another key.
pub const PALW_NOT_READY_KEY_MISMATCH_V2: &str = "the local signing key is not the one this bond registered";
/// `ready_to_produce`: the model registry admits no new claim of this class now — its lifecycle
/// state (a `Candidate` or `Prefetching` row), or its panel room or inflight cap — or, past
/// ADR-0152 R-core+, the bond already holds its T-2(a) share of the class. The detail is
/// `PalwProducerFactsV2::class_admission_refusal`.
pub const PALW_NOT_READY_CLASS_NOT_ADMITTING_V2: &str = "the model registry admits no new claim of this class now";
/// `ready_to_produce`: this class's blocks for the epoch are spent (the floor class is exempt).
pub const PALW_NOT_READY_EPOCH_BUDGET_V2: &str = "this class's epoch budget is already spent";
/// `ready_to_produce`: every sompi of the bond's exposure ceiling is reserved by live claims.
pub const PALW_NOT_READY_EXPOSURE_FULL_V2: &str = "the bond's exposure ceiling leaves no room for another claim";

/// Every sentence `ready_to_produce` can return, in the order it checks them.
pub const PALW_NOT_READY_REASONS_V2: [&str; 5] = [
    PALW_NOT_READY_BOND_UNKNOWN_V2,
    PALW_NOT_READY_KEY_MISMATCH_V2,
    PALW_NOT_READY_CLASS_NOT_ADMITTING_V2,
    PALW_NOT_READY_EPOCH_BUDGET_V2,
    PALW_NOT_READY_EXPOSURE_FULL_V2,
];

/// Read the facts for `class_id` (and optionally a bond) out of a state snapshot.
///
/// `daa_score` is the candidate's, because the epoch index admission uses is the CANDIDATE's, not
/// the tip's — a producer handed the tip's epoch at an epoch boundary would check its budget
/// against the wrong epoch and mine into a refusal.
/// **The retention a producer owes, and the only one admission accepts** (ADR-0072 Decision 8):
/// the four lattice windows a claim can be asked inside — bind, receipt, challenge, court. A
/// promise shorter than this discards the evidence before anyone can ask for it; a promise longer
/// was harmless and free to change, which made `trace_retention_daa` a draw. One spelling, read
/// by the facts a producer builds from and by the pin admission checks them against.
pub fn palw_min_trace_retention_daa_v1(state_params: &PalwStateParamsV2) -> u64 {
    state_params
        .window_bind()
        .saturating_add(state_params.window_receipt())
        .saturating_add(state_params.window_challenge())
        .saturating_add(state_params.window_court())
}

pub fn palw_producer_facts_v2(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    admission: &PalwAdmissionParamsV2,
    chain_point: BlockHash,
    daa_score: u64,
    class_id: Hash64,
    bond: Option<&PalwBondKeyV2>,
    work_target_floor: Option<u128>,
) -> Option<PalwProducerFactsV2> {
    // v2's callers predate the 2026-09-23 fence: the pre-fence headroom, byte for byte.
    palw_producer_facts_v3(state, state_params, admission, chain_point, daa_score, class_id, bond, work_target_floor, None, None, false, 0)
}

/// [`palw_producer_facts_v2`] with **ADR-0149's derived pwu**: `canonical_work_daa` is
/// `Params::palw_canonical_work_daa()`, and at or past it the producer is handed the ONE pwu the
/// admission will accept — `palw_attempt_derived_pwu_v1(target, derived draw)` — and the exposure
/// the admission will reserve for it (the derived draw in the collateral unit), so a producer never
/// builds an attempt its own chain refuses and never mispredicts its own headroom. A class the chain
/// has no derived draw for has no facts past the fence: it cannot produce, and saying so is the
/// producer holding rather than mining into a refusal.
///
/// `base_known_draw` is the admission's own (`PalwEpochBudgetFencesV1::base_known_draw`): the
/// floor's draw as the registry will write it, read only while the floor has no row (ADR-0149 §5).
/// Without it a fence armed at the registry's height would leave the floor with no facts on the
/// first blocks past it, and every producer holding at once is a chain that has stopped.
///
/// [`palw_producer_facts_v4`] with no second-clock depth — the facts as kaspad reads them until it
/// switches to v4 (ADR-0152 P6). Every field v3 always filled is unchanged; `committed` is read at
/// the DAA clock alone.
#[allow(clippy::too_many_arguments)]
pub fn palw_producer_facts_v3(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    admission: &PalwAdmissionParamsV2,
    chain_point: BlockHash,
    daa_score: u64,
    class_id: Hash64,
    bond: Option<&PalwBondKeyV2>,
    work_target_floor: Option<u128>,
    canonical_work_daa: Option<u64>,
    base_known_draw: Option<u128>,
    audit_2026_09_23_active: bool,
    claim_escrow: u64,
) -> Option<PalwProducerFactsV2> {
    palw_producer_facts_v4(
        state,
        state_params,
        admission,
        chain_point,
        daa_score,
        class_id,
        bond,
        work_target_floor,
        canonical_work_daa,
        base_known_draw,
        audit_2026_09_23_active,
        claim_escrow,
        None,
    )
}

/// **ADR-0152 S-SPEC §2: the producer's facts with the one committed ledger** — v3's arguments plus
/// the second clock's RAW depth at the candidate DAA (`palw_settled_anchor_depth_at`), from which
/// [`PalwProducerBondFactsV2::committed`] reads live locks at the escaped depth exactly as the fold
/// and admission do. Also fills the producer floor's shortfall (U2).
#[allow(clippy::too_many_arguments)]
pub fn palw_producer_facts_v4(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    admission: &PalwAdmissionParamsV2,
    chain_point: BlockHash,
    daa_score: u64,
    class_id: Hash64,
    bond: Option<&PalwBondKeyV2>,
    work_target_floor: Option<u128>,
    canonical_work_daa: Option<u64>,
    base_known_draw: Option<u128>,
    // 2026-09-23 audit H-1: `Params::palw_audit_2026_09_23` at the block — past it the headroom
    // prediction reserves `attempts x` one draw, exactly as admission and the ledger do.
    audit_2026_09_23_active: bool,
    // **Option A: the escrow this producer's own-work claim would carry at `daa_score`** —
    // `palw_claim_escrow_v1` of the block's subsidy under the carve resolved at its DAA, computed by
    // the caller that holds the coinbase schedule. The headroom counts it through the same
    // reservation the ledger and the ceiling read; 0 where no escrow is priced.
    claim_escrow: u64,
    // ADR-0152: the second clock's RAW depth at `daa_score` (`None` below the audit fence).
    raw_depth: Option<u64>,
) -> Option<PalwProducerFactsV2> {
    let class = state.class(&class_id)?;
    // ADR-0137: past the work target a model class draws against `MAX · min(1, CCU / W₀)` from
    // its registry row — no row, no price, no facts (the producer holds); the floor keeps its
    // class target.
    // ADR-0137: one spelling of the rule — the same helper the chain's admission reads, so the
    // producer's ticket AND its pwu are the ones the chain will derive (2026-09-18 audit, C-2).
    let class_target =
        crate::palw_admission_v2::palw_effective_class_target_v1(state, state_params, &class_id, work_target_floor).ok()?;
    let derived_draw =
        state.palw_attempt_per_draw_v1(&state_params.base_class_id(), &class_id, daa_score, canonical_work_daa, base_known_draw);
    let past_the_unit = canonical_work_daa.is_some_and(|height| daa_score >= height);
    let pwu = if past_the_unit {
        crate::palw_admission_v2::palw_attempt_derived_pwu_v1(class_target, derived_draw?)
    } else {
        match class.pwu_rule {
            PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => crate::palw_pwu::palw_pwu_v1(class_target, pwu_per_inference),
            PalwPwuRuleV2::MaxPerAttempt(cap) => cap,
        }
    };
    let exposure_pwu = crate::palw_state_v2::palw_exposure_pwu_v3(
        class,
        pwu,
        derived_draw.map(|work| work.min(u64::MAX as u128) as u64),
        state.palw_exposure_basis_v2(&state_params.base_class_id(), daa_score, canonical_work_daa, base_known_draw),
    );
    let epoch_index = daa_score / state_params.epoch_length();
    let epoch_budget_blocks = state
        .epoch_budgets()
        .filter(|b| b.epoch_index == epoch_index)
        .and_then(|b| b.budget_blocks.get(&class_id).copied())
        .unwrap_or(0);
    let epoch_produced_blocks = match state.epoch_counter(&class_id) {
        Some(counter) if counter.epoch_index == epoch_index => counter.produced_blocks,
        _ => 0,
    };
    let epoch_budget_released = crate::palw_state_v2::palw_epoch_budget_release_v1(
        state,
        state_params.epoch_length(),
        daa_score,
        &class_id,
        epoch_budget_blocks,
    );
    let bond = bond.and_then(|key| {
        let bond_state = state.bond(key)?;
        Some(PalwProducerBondFactsV2 {
            registered_pubkey: bond_state.pubkey.clone(),
            operator_id: bond_state.operator_id,
            collateral: bond_state.collateral,
            reserved_exposure: state.reserved_exposure(key),
            exposure_ceiling: (bond_state.collateral as u128).saturating_mul(admission.max_exposure_ratio_permille() as u128) / 1000,
            // The SAME derivation admission applies, or the producer's own headroom prediction
            // disagrees with the rule that refuses it.
            // ADR-0149: the admission's own expression (`palw_exposure_pwu_v3`), which below the
            // fence is `palw_exposure_pwu_v1` of the claimed pwu byte for byte.
            claim_exposure: (exposure_pwu as u128)
                .saturating_mul(class.slash_value_per_pwu as u128)
                .saturating_mul(if audit_2026_09_23_active {
                    crate::palw_pwu::palw_claim_attempts_v1(pwu, derived_draw.map(|work| work.min(u64::MAX as u128) as u64)) as u128
                } else {
                    1
                })
                // Option A: the escrow term, outside the attempts factor, exactly as the ceiling adds it.
                .saturating_add(state_params.claim_escrow_reservation_v1(daa_score, claim_escrow)),
            committed: if state_params.rcore_plus_active_at(daa_score) {
                crate::palw_state_v2::palw_bond_committed_raw_v1(state, state_params, key, daa_score, raw_depth)
            } else {
                state.reserved_exposure(key).saturating_add(state.registration_exposure(key))
            },
            producer_floor_shortfall: crate::palw_state_v2::palw_bond_producer_floor_shortfall_v1(state, state_params, key, daa_score),
            accuser_exposure: if state_params.rcore_plus_active_at(daa_score) {
                crate::palw_state_v2::palw_accuser_exposure_v1(state, key)
            } else {
                0
            },
        })
    });
    Some(PalwProducerFactsV2 {
        // The caller that holds the block's fences asks the fold's class gate (route-matrix #7).
        class_admission_refusal: None,
        is_base_class: class_id == state_params.base_class_id(),
        fp_certified: state_params.fp_certified_classes().is_none_or(|set| set.contains(&class_id))
            || state.fp_lane_certification(&class_id).is_some(),
        min_trace_retention_daa: palw_min_trace_retention_daa_v1(state_params),
        chain_point,
        daa_score,
        class_id,
        artifact_root: class.artifact_root,
        class_target,
        pwu,
        epoch_index,
        epoch_budget_blocks,
        epoch_produced_blocks,
        epoch_budget_released,
        epoch_budget_release_armed: false,
        epoch_budget_read: work_target_floor.is_none(),
        bond,
        safe_weight: state.safe_weight(),
        live_total: state.safe_weight().saturating_add(state.bounded_immature()),
        unresolved_claims: state.claims_iter().filter(|(_, c)| !c.phase.is_terminal()).count() as u64,
        open_courts: state.court_sessions_len() as u64,
        final_claims: state
            .claims_iter()
            .filter(|(_, c)| matches!(c.phase, crate::palw_state_v2::PalwClaimPhaseV2::Final { .. }))
            .count() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_admission_v2::{PalwEpochBudgetFencesV1, check_palw_attempt_admission_v2};
    use crate::palw_attempt_v2::{
        PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, challenge_v2, class_ticket_v3, execution_anchor_v3,
    };
    use crate::palw_state_v2::{PalwBlockContextV2, PalwConsensusObjectV2, apply_palw_transition_v2, palw_operator_id_v2};
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    const NET: u64 = 0x4E45_5457;

    fn state_params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(500, 100, 100, 100, 100, 1_000, h64(1), 4, 1_000, 1_000, 100, 100).unwrap()
    }

    fn bond_outpoint() -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_u64_word(1), index: 0 }
    }

    /// One class on the DERIVED pwu rule — because a `MaxPerAttempt` class would let a producer
    /// guess the pwu and still be admitted, which is precisely the case this contract is not for.
    fn state() -> PalwChainStateV2 {
        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 7 },
                initial_target: u128::MAX / 4,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: PalwBondKeyV2(bond_outpoint()),
                pubkey: vec![7; 4],
                operator_pubkey: vec![0x21; 8],
                collateral: 1_000_000,
                payout_payload: h64(0x9A11),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
        ];
        let ctx = PalwBlockContextV2 { block: crate::BlockHash::from_u64_word(1), daa_score: 100, blue_score: 1, subsidy: 0 };
        apply_palw_transition_v2(&PalwChainStateV2::genesis(), &state_params(), &ctx, &objects, None).unwrap().0
    }

    /// **ADR-0152 S-SPEC §2 / §10a: v4 hands the producer the ledger admission measures it by** —
    /// below `palw_rcore_plus` admission item 8's `reserved + registration`, past it the one
    /// committed ledger at the escaped depth of the raw depth given — and the producer floor's
    /// shortfall (U2). v3 is v4 without a depth, field for field.
    #[test]
    fn v4_reports_the_committed_ledger_and_the_producer_floor() {
        let state = state();
        let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
        let bond_key = PalwBondKeyV2(bond_outpoint());
        let facts = |params: &PalwStateParamsV2, raw: Option<u64>| {
            palw_producer_facts_v4(
                &state,
                params,
                &admission,
                crate::BlockHash::from_u64_word(1),
                101,
                h64(1),
                Some(&bond_key),
                None,
                None,
                None,
                false,
                0,
                raw,
            )
            .expect("facts")
        };
        let below = facts(&state_params(), Some(3));
        let bond = below.bond.as_ref().unwrap();
        assert_eq!(bond.committed, state.reserved_exposure(&bond_key) + state.registration_exposure(&bond_key));
        assert_eq!(bond.producer_floor_shortfall, None, "dormant below the fence");
        let v3 = palw_producer_facts_v3(
            &state,
            &state_params(),
            &admission,
            crate::BlockHash::from_u64_word(1),
            101,
            h64(1),
            Some(&bond_key),
            None,
            None,
            None,
            false,
            0,
        )
        .unwrap();
        assert_eq!(v3, facts(&state_params(), None), "v3 is v4 with no depth");
        let armed = state_params().with_rcore_plus_mirrors(Some(0), 0, Vec::new());
        let past = facts(&armed, Some(3));
        let bond = past.bond.as_ref().unwrap();
        assert_eq!(bond.committed, crate::palw_state_v2::palw_bond_committed_raw_v1(&state, &armed, &bond_key, 101, Some(3)));
        assert_eq!(bond.producer_floor_shortfall, None, "1,000,000 posted against a 1,000 floor");
        assert!(bond.has_committed_room());
        // The same bond against a 2,000,000 floor is 1,000,000 short.
        let high_floor = PalwStateParamsV2::new(500, 100, 100, 100, 100, 1_000, h64(1), 4, 1_000, 2_000_000, 100, 100)
            .unwrap()
            .with_rcore_plus_mirrors(Some(0), 0, Vec::new());
        assert_eq!(facts(&high_floor, None).bond.unwrap().producer_floor_shortfall, Some(1_000_000));
    }

    /// **Build an attempt from NOTHING but the facts, and see whether the chain takes it.**
    ///
    /// This is the round trip the audit kept finding defects with: two sides that were reviewed
    /// separately and asked to agree only here. Every field below is either the producer's own
    /// (its execution, its keys) or copied straight out of `facts` — nothing is re-derived, so if
    /// the contract were missing a fact the attempt could not be built at all, and if a fact were
    /// derived differently from admission's the attempt would be refused.
    #[test]
    fn an_attempt_built_only_from_the_facts_is_admitted() {
        let state = state();
        let params = state_params();
        let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
        let bond_key = PalwBondKeyV2(bond_outpoint());
        let facts = palw_producer_facts_v2(
            &state,
            &params,
            &admission,
            crate::BlockHash::from_u64_word(1),
            101,
            h64(1),
            Some(&bond_key),
            None,
        )
        .expect("the class is registered, so it has facts");

        assert_eq!(facts.ready_to_produce(&[7; 4]), Ok(()), "the producer is clear to run an inference");
        assert_eq!(
            facts.pwu,
            crate::palw_pwu::palw_pwu_v1(facts.class_target, 7),
            "the pwu is the derivation, handed over rather than left to be recomputed"
        );

        let mut env = PalwAttemptEnvelopeV2 {
            attempt: PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain: h64(NET),
                challenge: challenge_v2(h64(NET), h64(0x5050_4800), 7, 1, facts.class_id, &bond_outpoint()),
                class_id: facts.class_id,
                executor_bond: bond_outpoint(),
                executor_pubkey: facts.bond.as_ref().unwrap().registered_pubkey.clone(),
                operator_id: facts.bond.as_ref().unwrap().operator_id,
                artifact_root: facts.artifact_root,
                // The producer's own: what its execution produced.
                trace_root: h64(31),
                output_root: h64(32),
                execution_root: h64(41),
                pwu: facts.pwu,
                trace_manifest_root: crate::palw_attempt_v2::attempt_trace_manifest_root_v1(h64(31), 1),
                trace_chunk_count: 1,
                trace_retention_daa: 999_999,
            },
            signature: vec![0x5A; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        };
        // The class lottery, run the way a producer runs it — over its own execution, under the
        // anchor the header derives (ADR-0072).
        let anchor = execution_anchor_v3(h64(NET), h64(0x5050_4800), facts.class_id, &bond_outpoint(), 1);
        let mut won = false;
        for n in 0u64..100_000 {
            env.attempt.trace_root = h64(0x3100_0000_0000_0000u64.wrapping_add(n));
            if class_ticket_v3(&env.attempt, anchor) <= facts.class_target {
                won = true;
                break;
            }
        }
        assert!(won, "a quarter-of-the-space target is winnable in 1e5 tries");

        let ctx = PalwBlockContextV2 { block: crate::BlockHash::from_u64_word(2), daa_score: 101, blue_score: 2, subsidy: 0 };
        check_palw_attempt_admission_v2(&state, &params, &admission, &ctx, &env, PalwEpochBudgetFencesV1::default())
            .expect("the chain takes it");
        crate::palw_admission_v2::check_palw_class_lottery_v3(&state, &env.attempt, anchor).expect("and its draw wins");
    }

    /// **Every fact is load-bearing.** Move one and admission refuses — which is what makes this a
    /// contract rather than a convenience: a producer that sourced any of them elsewhere would be
    /// sourcing the thing that decides whether its block exists.
    #[test]
    fn moving_any_single_fact_is_refused() {
        let state = state();
        let params = state_params();
        let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
        let bond_key = PalwBondKeyV2(bond_outpoint());
        let facts = palw_producer_facts_v2(
            &state,
            &params,
            &admission,
            crate::BlockHash::from_u64_word(1),
            101,
            h64(1),
            Some(&bond_key),
            None,
        )
        .unwrap();
        let ctx = PalwBlockContextV2 { block: crate::BlockHash::from_u64_word(2), daa_score: 101, blue_score: 2, subsidy: 0 };

        let build = |mutate: &dyn Fn(&mut PalwAttemptUnsignedV2)| {
            let mut env = PalwAttemptEnvelopeV2 {
                attempt: PalwAttemptUnsignedV2 {
                    version: PALW_ATTEMPT_V2_VERSION,
                    network_domain: h64(NET),
                    challenge: challenge_v2(h64(NET), h64(0x5050_4800), 7, 1, facts.class_id, &bond_outpoint()),
                    class_id: facts.class_id,
                    executor_bond: bond_outpoint(),
                    executor_pubkey: facts.bond.as_ref().unwrap().registered_pubkey.clone(),
                    operator_id: facts.bond.as_ref().unwrap().operator_id,
                    artifact_root: facts.artifact_root,
                    trace_root: h64(31),
                    output_root: h64(32),
                    execution_root: h64(41),
                    pwu: facts.pwu,
                    trace_manifest_root: crate::palw_attempt_v2::attempt_trace_manifest_root_v1(h64(31), 1),
                    trace_chunk_count: 1,
                    trace_retention_daa: 999_999,
                },
                signature: vec![0x5A; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            };
            let anchor = execution_anchor_v3(h64(NET), h64(0x5050_4800), facts.class_id, &bond_outpoint(), 1);
            for n in 0u64..100_000 {
                env.attempt.trace_root = h64(0x3100_0000_0000_0000u64.wrapping_add(n));
                mutate(&mut env.attempt);
                if class_ticket_v3(&env.attempt, anchor) <= facts.class_target {
                    break;
                }
            }
            env
        };

        // The artifact root, the pwu, and the key: one each, and each refused for its own reason.
        for (name, mutate) in [
            ("artifact root", &(|a: &mut PalwAttemptUnsignedV2| a.artifact_root = h64(0xBAD)) as &dyn Fn(&mut _)),
            ("pwu", &(|a: &mut PalwAttemptUnsignedV2| a.pwu = a.pwu.wrapping_add(1)) as &dyn Fn(&mut _)),
            ("executor key", &(|a: &mut PalwAttemptUnsignedV2| a.executor_pubkey = vec![9; 4]) as &dyn Fn(&mut _)),
            ("operator id", &(|a: &mut PalwAttemptUnsignedV2| a.operator_id = palw_operator_id_v2(&[0xEE; 8])) as &dyn Fn(&mut _)),
        ] {
            let env = build(mutate);
            assert!(
                check_palw_attempt_admission_v2(&state, &params, &admission, &ctx, &env, PalwEpochBudgetFencesV1::default()).is_err(),
                "a producer that got the {name} from anywhere but the facts is a producer with no blocks"
            );
        }
    }

    /// **The floor is exempt from the epoch budget, and the producer must agree with admission.**
    ///
    /// The budget table is written for the TIP's epoch and read for the CANDIDATE's, so at every
    /// epoch boundary the lookup misses and `unwrap_or(0)` makes the budget zero. Admission does not
    /// care — it exempts the floor (`palw_admission_v2.rs:234`) precisely so the ADR-0039 W6′
    /// deadlock is unrepresentable — but the producer applied the cap anyway and held forever,
    /// re-creating on the client side the chain-stopping deadlock `58291251` removed from consensus.
    ///
    /// This is the whole failure in one assertion: a floor with a ZERO budget is still producible.
    #[test]
    fn the_liveness_floor_is_never_capped_by_an_epoch_budget() {
        let state = state();
        let params = state_params();
        let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
        let bond_key = PalwBondKeyV2(bond_outpoint());

        // An epoch the chain has written no budget for — every epoch boundary, in other words.
        let far = params.epoch_length() * 9_999 + 1;
        let facts = palw_producer_facts_v2(
            &state,
            &params,
            &admission,
            crate::BlockHash::from_u64_word(1),
            far,
            h64(1),
            Some(&bond_key),
            None,
        )
        .expect("the class is still registered");
        assert!(facts.is_base_class, "h64(1) is this fixture's floor");
        assert_eq!(facts.epoch_budget_blocks, 0, "and the chain has written no budget for this epoch");
        assert!(facts.has_epoch_room(), "a zero budget must not stop the floor — that is the deadlock");
        assert_eq!(facts.ready_to_produce(&[7; 4]), Ok(()), "so the producer builds the epoch's first block");

        // A NON-floor class is still capped, because the cap is what Decision 2 is for. Nothing was
        // loosened; the exemption is exactly the one admission already makes.
        let entrant = palw_producer_facts_v2(
            &state,
            &params,
            &admission,
            crate::BlockHash::from_u64_word(1),
            far,
            h64(2),
            Some(&bond_key),
            None,
        );
        assert!(entrant.is_none(), "this fixture registers no entrant; the floor is the only class");
    }

    /// The pre-flight answers are the ones admission would give, not a second opinion: an    /// The pre-flight answers are the ones admission would give, not a second opinion: an
    /// unregistered bond has no facts to be ready with.
    #[test]
    fn a_bond_the_chain_does_not_know_is_not_ready() {
        let state = state();
        let params = state_params();
        let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
        let stranger = PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xDEAD), index: 0 });
        let facts = palw_producer_facts_v2(
            &state,
            &params,
            &admission,
            crate::BlockHash::from_u64_word(1),
            101,
            h64(1),
            Some(&stranger),
            None,
        )
        .unwrap();
        assert!(facts.bond.is_none());
        assert_eq!(facts.ready_to_produce(&[7; 4]), Err("the named bond is not registered on this chain"));
        assert_eq!(facts.ready_to_spend_receipts(&[7; 4]), Err("the named bond is not registered on this chain"));
        // And a class the chain does not know has no facts at all — there is nothing to be told.
        assert!(
            palw_producer_facts_v2(&state, &params, &admission, crate::BlockHash::from_u64_word(1), 101, h64(0xBAD), None, None)
                .is_none()
        );
    }

    /// **A full attempt lane does not hold the receipt lane.** A receipt block spends a quantum of
    /// a claim that is already `Final`: it opens no claim and draws on no attempt budget, so the
    /// two attempt-only holds must leave it clear — while the two bond holds (unknown bond, wrong
    /// key) apply to both, because a receipt is signed by the bond's key like any block.
    #[test]
    fn a_full_attempt_lane_does_not_hold_the_receipt_lane() {
        let state = state();
        let params = state_params();
        let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
        let bond_key = PalwBondKeyV2(bond_outpoint());
        let mut facts = palw_producer_facts_v2(
            &state,
            &params,
            &admission,
            crate::BlockHash::from_u64_word(1),
            101,
            h64(1),
            Some(&bond_key),
            None,
        )
        .unwrap();
        assert_eq!(facts.ready_to_spend_receipts(&[7; 4]), Ok(()));

        // The bond's ceiling is full — the state a bond that commits free-prompt claims reaches.
        let bond = facts.bond.as_mut().unwrap();
        bond.reserved_exposure = bond.exposure_ceiling;
        assert_eq!(facts.ready_to_produce(&[7; 4]), Err("the bond's exposure ceiling leaves no room for another claim"));
        assert_eq!(facts.ready_to_spend_receipts(&[7; 4]), Ok(()), "a receipt reserves no exposure");

        // And an attempt class whose epoch budget is spent (a non-floor class; the floor is exempt).
        facts.is_base_class = false;
        facts.epoch_budget_blocks = 0;
        assert_eq!(facts.ready_to_produce(&[7; 4]), Err("this class's epoch budget is already spent"));
        assert_eq!(facts.ready_to_spend_receipts(&[7; 4]), Ok(()), "a receipt draws on no attempt budget");
        // Route-matrix #7: a class the registry holds is the reason, ahead of the budget and the
        // ceiling — nothing else about the bond can make its claim land — and a receipt still spends.
        facts.class_admission_refusal = Some("class … is Prefetching under the model registry".to_string());
        assert_eq!(facts.ready_to_produce(&[7; 4]), Err(PALW_NOT_READY_CLASS_NOT_ADMITTING_V2));
        assert_eq!(facts.ready_to_spend_receipts(&[7; 4]), Ok(()), "the class gate holds attempts, not receipts");
        assert_eq!(facts.ready_to_produce(&[9; 4]), Err(PALW_NOT_READY_KEY_MISMATCH_V2), "the key is still asked first");
        facts.class_admission_refusal = None;
        // ADR-0137: past the work target the chain reads no budget, so neither does the producer —
        // the same spent budget is not a hold (the 2026-09-18 drill's stall).
        facts.epoch_budget_read = false;
        assert_eq!(
            facts.ready_to_produce(&[7; 4]),
            Err("the bond's exposure ceiling leaves no room for another claim"),
            "no budget is read past the work target: the full ceiling set above is what holds now"
        );
        assert!(facts.has_epoch_room(), "a spent budget is not a hold past the work target");
        facts.epoch_budget_read = true;
        let past_the_target = palw_producer_facts_v2(
            &state,
            &params,
            &admission,
            crate::BlockHash::from_u64_word(1),
            101,
            h64(1),
            Some(&bond_key),
            Some(1),
        )
        .unwrap();
        assert!(!past_the_target.epoch_budget_read, "the facts built past the fence say so themselves");
        assert!(past_the_target.has_epoch_room());

        // The key still matters to both lanes.
        assert_eq!(facts.ready_to_spend_receipts(&[9; 4]), Err("the local signing key is not the one this bond registered"));
    }
}

// ---------------------------------------------------------------------------------------------
// The panel's half of the contract
// ---------------------------------------------------------------------------------------------

/// One seat duty this node holds: a claim whose panel names a bond it can sign for.
///
/// The claim's committed roots ride along, because that is what a seat DECIDES against — a seat
/// that had to fetch them separately could be handed a different pair than the chain holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwSeatDutyV2 {
    /// **The block this claim was accepted on** — the anchor's only unforgeable input.
    ///
    /// A party judging somebody else's material derives the job anchor from this block's pre-PoW
    /// hash rather than reading it out of the material. Carried on the view because that is where
    /// the judging happens, and because a view that omitted it left the verifier with no honest
    /// source for the question the claim was supposed to answer.
    pub accepted_block: Hash64,
    pub claim_id: Hash64,
    /// **Which class this claim is of** — and therefore which graph a seat re-executes.
    /// Carried rather than looked up: a seat that resolved the class separately could verify
    /// material against a class the chain does not say the claim is of. It comes off the claim
    /// record, so it is the chain's answer and not the seat's.
    pub class_id: Hash64,
    /// The class's registered artifact root, for the same reason the producer is handed one: the
    /// seat must hold the SAME weights, and "the same" is this value.
    pub artifact_root: Hash64,
    pub seat_bond: PalwBondKeyV2,
    /// The producer whose material this seat must judge. Never this seat: `derive_panel_v2`
    /// excludes a claim's own executor by bond, by operator and by key.
    pub executor_bond: PalwBondKeyV2,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    /// The claim's committed `output_root` (ADR-0084 Decision 1): what a seat binds a served
    /// answer envelope's ids to before it recomputes anything from them. Read off the claim
    /// record beside the two roots above, for the same reason they ride — a seat that fetched it
    /// separately could be handed a different value than the chain holds.
    pub output_root: Hash64,
    /// When the panel was bound. The receipt window runs from here, and a receipt signed outside
    /// it is refused by `validate_receipt_quorum_v2`.
    pub bound_daa: u64,
    /// The last DAA at which a receipt for this claim still counts.
    pub receipt_deadline: u64,
    /// **The beacon the panel was drawn from** — the block at the claim's anchor slot, which did
    /// not exist when the commitment was fixed (ADR-0044 F4/F5). A free-prompt seat draws the
    /// checkpoint intervals it must open from this and its own seat index
    /// (`palw_fp_interval_draw_v1`, ADR-0077 Decision 8), so the executor cannot know which
    /// intervals will be checked when it commits.
    pub panel_anchor: Hash64,
    /// This seat's position in the bound panel — the second input to the interval draw, so two
    /// seats of one panel open different intervals.
    pub seat_index: u8,
    /// How many seats the bound panel has — `K = seats − 1` for Verification V2, and the assignment
    /// is a function of this count together with the anchor.
    pub panel_seat_count: u16,
    /// **What the chain PRICED this claim at** — the seat's only handle on "is the material I was
    /// served the work this claim was paid for".
    ///
    /// The roots alone cannot answer that. A commitment's `execution_root` is carried verbatim
    /// from its payload and related to nothing the chain can recompute (the chain has no leg
    /// roots), while its `cu`/`quanta`/`pwu` are derived from the job shape the payload DECLARES.
    /// So a producer may declare a hundred-thousand-token job, serve a one-token material whose
    /// roots are genuinely that material's, and a seat comparing only roots certifies it — block
    /// work bought with recycled collateral instead of inference, which is the one property this
    /// lane exists to establish. A seat re-prices what it actually executed and compares against
    /// these, which is the check the chain cannot make for it.
    pub pwu: u64,
    /// Quanta the claim was opened for; `0` on the attempt lane, which has none.
    pub quanta: u32,
    /// **Which lane this claim's material speaks** — and therefore how a seat verifies it.
    ///
    /// An attempt claim's material is the run's own rows; the seat re-hashes them under the job
    /// the ANCHOR implies. A free-prompt claim's job is the CALLER's, underivable from any
    /// anchor: its material is the job itself ([`crate::palw_freeprompt_v3::PalwFpMaterialV1`])
    /// and the seat re-executes it — the replay `PublicDa` was named for. A seat that fed one
    /// lane's material to the other lane's verifier would file `Unavailable` against every
    /// honest free-prompt executor, and a quorum of those DEFAULTS the producer — the panel
    /// would convict the lane's every user for using it.
    pub free_prompt: bool,
    /// The leaf count the free-prompt claim was priced from (ADR-0074 Decision 5): a seat's one
    /// pricing check is that the capture it authenticated has exactly this many leaves. Zero on
    /// an attempt.
    pub work_leaves: u64,
}

/// **Every seat duty this node holds at one chain point** (launch blockers §2).
///
/// Nothing in the tree ever filed a `ReceiptLicensed`, so no claim reached `Final`: every panel
/// voided at `ReceiptTimeout` with all its seats slashed, `safe_weight` stayed zero, and the
/// escrowed worker carve of every block was burned. A seat cannot act on a duty it cannot see, and
/// this is where it sees them.
///
/// `mine` is the set of bonds this node can sign for. Derived from the state the chain holds rather
/// than assembled by the caller, for the same reason `palw_producer_facts_v2` is: a seat that
/// computed its own deadline would eventually disagree with the quorum check about it.
/// **A claim a challenger could still dispute** — licensed, not yet final, and with no session of
/// this bond's already open against it.
///
/// The court had no opener either: `CourtOpened` was constructed nowhere, so the only disputes on
/// any chain were the ones a test wrote by hand. Deciding WHETHER to dispute is the challenger's
/// (it costs the claim's own stake now), but finding the claims it could is a question about state,
/// and belongs here beside the seat and court duty lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwDisputableClaimV2 {
    /// **The block this claim was accepted on** — the anchor's only unforgeable input.
    ///
    /// A party judging somebody else's material derives the job anchor from this block's pre-PoW
    /// hash rather than reading it out of the material. Carried on the view because that is where
    /// the judging happens, and because a view that omitted it left the verifier with no honest
    /// source for the question the claim was supposed to answer.
    pub accepted_block: Hash64,
    pub claim_id: Hash64,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub executor_bond: PalwBondKeyV2,
    pub trace_root: Hash64,
    pub execution_root: Hash64,
    pub licensed_daa: u64,
    /// The claim is a free-prompt commitment (ADR-0073 Decision 1d): its job is the USER's, fixed
    /// on chain as `fp_job_id_v3(job)` with a hash-bound prompt, so a challenger re-executes THAT
    /// job — never `job_for_anchor`, whose answer is a job nobody asked and whose roots differ
    /// from every honest free-prompt claim's.
    pub free_prompt: bool,
}

pub fn palw_disputable_claims_v2(state: &PalwChainStateV2, mine: &[PalwBondKeyV2]) -> Vec<PalwDisputableClaimV2> {
    let mut out = Vec::new();
    for (claim_id, claim) in state.claims_iter() {
        let crate::palw_state_v2::PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = claim.phase else { continue };
        // Never our own work: `derive_panel_v2` excludes the executor from its own panel for the
        // same reason, and `validate_court_opened_v2` refuses a self-challenge outright.
        if mine.contains(&claim.bond) {
            continue;
        }
        // One session per (claim, challenger) — the id is derived from both, so a second open
        // would collide rather than stack.
        if state.court_sessions_iter().any(|(_, s)| s.claim == *claim_id && mine.contains(&s.challenger_bond)) {
            continue;
        }
        let Some(artifact_root) = state.class(&claim.class_id).map(|c| c.artifact_root) else { continue };
        out.push(PalwDisputableClaimV2 {
            accepted_block: claim.accepted_block,
            claim_id: *claim_id,
            class_id: claim.class_id,
            artifact_root,
            executor_bond: claim.bond,
            trace_root: claim.trace_root,
            execution_root: claim.execution_root,
            licensed_daa,
            free_prompt: matches!(claim.source, crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { .. }),
        });
    }
    out
}

/// **What a party owes in a court session it is a party to** — the court's half of
/// [`palw_seat_duties_v2`].
///
/// Nothing in this tree constructed a `CourtDisclosed`, so a dispute could be opened and never
/// answered. That was not a missing feature so much as a missing QUESTION: the ladder knows whose
/// turn it is and what interval is open, and no code ever asked it on behalf of a node that holds
/// one of the two bonds. This is the asking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCourtDutyV2 {
    /// **The block this claim was accepted on** — the anchor's only unforgeable input.
    ///
    /// A party judging somebody else's material derives the job anchor from this block's pre-PoW
    /// hash rather than reading it out of the material. Carried on the view because that is where
    /// the judging happens, and because a view that omitted it left the verifier with no honest
    /// source for the question the claim was supposed to answer.
    pub accepted_block: Hash64,
    pub session_id: Hash64,
    pub claim_id: Hash64,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    /// The bond the claim was produced under — the RESPONDER, who discloses.
    pub executor_bond: PalwBondKeyV2,
    /// The bond that opened the session — the CHALLENGER, who posts verdicts.
    pub challenger_bond: PalwBondKeyV2,
    /// Which of the two this node is. Both is possible only in a self-challenge, which
    /// `validate_court_opened_v2` refuses, so exactly one side is ours.
    pub i_am_responder: bool,
    pub round: u32,
    /// The open interval, `[lo, hi)`.
    pub interval: (u64, u64),
    /// The index a disclosure must answer about, when it is our turn to disclose.
    pub midpoint: Option<u64>,
    /// `Some(index)` once the ladder has narrowed to one step — the index a close must adjudicate.
    pub terminal_index: Option<u64>,
    /// The rung the responder last answered, `(midpoint, disclosed state)` — what a challenger's
    /// verdict is a comparison against.
    pub last_disclosure: Option<(u64, Hash64)>,
    /// Whose move it is, so a caller does not have to re-derive the turn from the interval.
    pub turn: crate::palw_bisect::PalwBisectTurnV1,
    pub rung_deadline_daa: u64,
    pub session_deadline_daa: u64,
    pub trace_root: Hash64,
    pub execution_root: Hash64,
    /// The disputed claim is a free-prompt commitment (ADR-0073 Decision 1b). Its anchor is not
    /// derived from the accepted block — the user set the question — but read as
    /// `fp_job_id_v3(job)` off the claim's job material, and the prover is handed the user's
    /// prompt rather than deriving one. `PalwSeatDutyV2` carries the same bit for the same reason.
    pub free_prompt: bool,
    /// **The disputed class commits a FUSED attention site** (ADR-0082 Decision 2) — the fold's own
    /// record (`PalwClassStateV2::fused_attention`), so a fused terminal's opening move is the
    /// responder's root claim rather than a close.
    pub fused_class: bool,
    /// **The dissection phase, once a root claim opened one** (ADR-0093 as built): the range under
    /// dispute, the root's `(m*, S*)`, the children awaiting the challenger's index — everything a
    /// party's next move is computed against, read off the chain rather than remembered.
    pub dissection: Option<crate::palw_attn_court_v1::PalwAttnDissectPhaseV1>,
    /// ADR-0133 S1: seats on this claim's bound panel, so a close resumes the accused V2 segment
    /// from its published checkpoint rather than from genesis. Zero when no panel is bound yet
    /// (the bisection still runs; segment resume is then unavailable).
    pub panel_seat_count: u16,
}

/// Every open session in which `mine` holds the executor's bond or the challenger's.
pub fn palw_court_duties_v2(state: &PalwChainStateV2, mine: &[PalwBondKeyV2]) -> Vec<PalwCourtDutyV2> {
    let mut out = Vec::new();
    for (session_id, session) in state.court_sessions_iter() {
        let Some(claim) = state.claim(&session.claim) else { continue };
        let i_am_responder = mine.contains(&claim.bond);
        let i_am_challenger = mine.contains(&session.challenger_bond);
        if !i_am_responder && !i_am_challenger {
            continue;
        }
        // A claim whose class has left the registry cannot be adjudicated by anyone, so it yields
        // no duty rather than a duty nobody can discharge — the same rule the seat list uses.
        let Some(artifact_root) = state.class(&claim.class_id).map(|c| c.artifact_root) else { continue };
        let (lo, hi) = session.ladder.interval();
        // **Whose move it is, through the SAME helper the fold and the deadline index use**
        // (ADR-0082 Decision 2; mainnet audit 2026-09-06, H-5 item d). This view read the LADDER's
        // turn and the LADDER's rung deadline unconditionally, and `session.dissection` was never
        // looked at — so a session with an open phase reported `Terminal` to the responder that
        // owed the phase's next round, and a fused terminal reported `Terminal` to a responder the
        // chain was already clocking as `AwaitDisclosure`. Every court arm in the panel switches on
        // `duty.turn`, so this is the second half of the missing responder: it would misroute even
        // a correct one.
        let (turn, rung_deadline_daa) = crate::palw_state_v2::court_turn_and_rung_deadline_v2(
            session,
            crate::palw_state_v2::court_session_class_is_fused_v2(state, session),
        );
        // The round is the PHASE's once one is open, for the same reason the turn is.
        let round = session.dissection.as_ref().map_or_else(|| session.ladder.round(), |phase| phase.round());
        out.push(PalwCourtDutyV2 {
            accepted_block: claim.accepted_block,
            session_id: *session_id,
            claim_id: session.claim,
            class_id: claim.class_id,
            artifact_root,
            executor_bond: claim.bond,
            challenger_bond: session.challenger_bond,
            i_am_responder,
            round,
            interval: (lo, hi),
            midpoint: (hi.saturating_sub(lo) > 1).then(|| lo + (hi - lo) / 2),
            terminal_index: session.ladder.terminal_index(),
            last_disclosure: session.ladder.last_disclosure(),
            turn,
            rung_deadline_daa,
            session_deadline_daa: session.deadline_daa,
            trace_root: claim.trace_root,
            execution_root: claim.execution_root,
            free_prompt: matches!(claim.source, crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { .. }),
            fused_class: crate::palw_state_v2::court_session_class_is_fused_v2(state, session),
            dissection: session.dissection.clone(),
            panel_seat_count: state.panel(&session.claim).map(|panel| panel.seats.len() as u16).unwrap_or(0),
        });
    }
    out
}

/// **A data-availability accusation this node must answer** (ADR-0062 D3; mainnet audit
/// 2026-09-05): a claim it produced is under an open accusation, and the event named must be
/// opened out of the capture it retains before `disclose_deadline_daa`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwDaDutyV2 {
    pub claim_id: Hash64,
    /// The block the claim rides — what an attempt claim's job is derived from, so a responder whose
    /// capture was pruned can re-make it by replaying that job (the court duty carries it already).
    pub accepted_block: Hash64,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    /// The bond the claim was produced under — the one that must sign the disclosure.
    pub executor_bond: PalwBondKeyV2,
    /// The packed `(row, tile)` the accusation names — `palw_da_event_index_parts_v1` unpacks it.
    pub missing_event_index: u32,
    pub accused_daa: u64,
    /// `accused_daa + W_disclose`: silence past it confirms the default (SA-3/SA-5).
    pub disclose_deadline_daa: u64,
    pub trace_root: Hash64,
    pub execution_root: Hash64,
    pub free_prompt: bool,
    /// **ADR-0103 Decision 4 / ADR-0111: the unit a HELD accusation named** — a prompt tile, a state
    /// chunk, a range of leaves or a leaf's evidence — when `missing_event_index` is the held
    /// sentinel. `None` for an event accusation. The producer answers in this unit or is defaulted.
    pub held_missing: Option<crate::palw_held_da_v1::PalwHeldMissingV1>,
}

/// The claims under accusation whose producing bond is in `mine`.
pub fn palw_da_duties_v2(state: &PalwChainStateV2, state_params: &PalwStateParamsV2, mine: &[PalwBondKeyV2]) -> Vec<PalwDaDutyV2> {
    let mut out = Vec::new();
    for (claim_id, claim) in state.claims_iter() {
        let crate::palw_state_v2::PalwClaimPhaseV2::DefaultDisputed { accused_daa, missing_event_index, .. } = claim.phase else {
            continue;
        };
        if !mine.contains(&claim.bond) {
            continue;
        }
        let Some(artifact_root) = state.class(&claim.class_id).map(|c| c.artifact_root) else { continue };
        out.push(PalwDaDutyV2 {
            claim_id: *claim_id,
            accepted_block: claim.accepted_block,
            class_id: claim.class_id,
            artifact_root,
            executor_bond: claim.bond,
            missing_event_index,
            accused_daa,
            disclose_deadline_daa: accused_daa.saturating_add(crate::palw_state_v2::palw_da_disclose_window_daa_v1(state_params)),
            trace_root: claim.trace_root,
            execution_root: claim.execution_root,
            free_prompt: matches!(claim.source, crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { .. }),
            held_missing: state.held_da_missing_of(claim_id),
        });
    }
    out
}

/// Which claims an operator asks about: the ones its bond made, or the ones its bond judges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwClaimRoleV1 {
    Executor,
    Seat,
}

/// **One claim as its operator reads it** (ADR-0122 §6.5, `getPalwClaims`): its phase and the next
/// date it moves by itself, what it reserves and what it will pay, and who judges it. Read off the
/// state at the tip; nothing here is node-local.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwClaimRowV1 {
    pub claim_id: Hash64,
    pub free_prompt: bool,
    pub quanta: u32,
    pub quanta_spent: u32,
    pub class_id: Hash64,
    pub executor_bond: PalwBondKeyV2,
    pub phase: crate::palw_state_v2::PalwClaimPhaseV2,
    pub accepted_daa: u64,
    pub accepted_block: BlockHash,
    /// Set when a receipt timeout sent the claim back for its one redraw.
    pub rebound_daa: Option<u64>,
    /// The bound panel's seats, in seat order, and when it bound.
    pub seats: Vec<PalwBondKeyV2>,
    pub bound_daa: Option<u64>,
    /// When the current phase ends by itself: a window closes, a court's backstop, or — for a
    /// final or voided claim — when its record retires from the state.
    pub deadline_daa: Option<u64>,
    /// The collateral this claim reserves on its bond until it ends.
    pub reserved: u128,
    /// The block lane's escrow (0 for a prompt-lane claim, and for a merged-blue attempt).
    pub escrowed_reward: u64,
    /// The payout queued for the next coinbase, once the claim is final — the producer's leg.
    /// Below `palw_rcore_plus` it is queued at `Final` under the claim id; past it the leg vests and
    /// is queued only when step 3d moves it, under its A-KEY key (`palw_vesting_payout_key_v1`), for
    /// the one block before the next coinbase mints it.
    pub payout_pending: Option<u64>,
    pub work_leaves: u64,
    pub open_courts: usize,
    /// **Where this claim's Final stands in the execution lane**, if it earned a credit there (the
    /// 2026-09-23 route-matrix audit's #7: a Final's CanonicalWork credit, its tickets and its wait
    /// had no observable at all).
    pub exec_lane: Option<PalwClaimExecLaneV1>,
}

/// **One claim's Final as the execution lane holds it** ([`palw_claim_exec_lane_v1`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwClaimExecLaneV1 {
    /// `credited` (gathered in the open span), `maturing` (in a snapshot waiting for its span) or
    /// `scheduled` (in a kept schedule, its tickets on that span's rounds).
    pub stage: &'static str,
    /// The CanonicalWork credit the Final earned its domain (`record_round_final`).
    pub credit: u64,
    /// The span it was gathered in, the span it waits for, or the span that scheduled it.
    pub span: u64,
    /// Its tickets in that schedule (0 before it is scheduled), how many were spent, and the rounds
    /// they occupy.
    pub tickets: u32,
    pub tickets_spent: u32,
    pub first_round: Option<u64>,
    pub last_round: Option<u64>,
}

/// **Where `claim_id`'s Final stands in the execution lane**, read from the state's three stages;
/// `None` for a claim that earned no credit there, or whose stage the state no longer keeps. A
/// Final collapsed onto another claim of the same execution (the mint keeps one per root) shows its
/// credit and no tickets of its own.
pub fn palw_claim_exec_lane_v1(state: &PalwChainStateV2, claim_id: &Hash64) -> Option<PalwClaimExecLaneV1> {
    let (open_span, finals) = state.round_finals();
    if let Some(f) = finals.get(claim_id) {
        return Some(PalwClaimExecLaneV1 {
            stage: "credited",
            credit: f.credit,
            span: open_span,
            tickets: 0,
            tickets_spent: 0,
            first_round: None,
            last_round: None,
        });
    }
    for (span, schedule) in state.round_schedules() {
        if let Some(f) = schedule.finals.iter().find(|f| f.claim_id == *claim_id) {
            let rounds: Vec<u64> = schedule.quanta.iter().filter(|q| q.final_id == *claim_id).map(|q| q.scheduled_round).collect();
            return Some(PalwClaimExecLaneV1 {
                stage: "scheduled",
                credit: f.credit,
                span: *span,
                tickets: rounds.len() as u32,
                tickets_spent: rounds.iter().filter(|round| state.round_permit_used(*span, **round, 0)).count() as u32,
                first_round: rounds.iter().min().copied(),
                last_round: rounds.iter().max().copied(),
            });
        }
    }
    state.round_pending_snapshots().iter().find_map(|(target, snapshot)| {
        snapshot.finals.iter().find(|f| f.claim_id == *claim_id).map(|f| PalwClaimExecLaneV1 {
            stage: "maturing",
            credit: f.credit,
            span: *target,
            tickets: 0,
            tickets_spent: 0,
            first_round: None,
            last_round: None,
        })
    })
}

/// **When a claim's phase ends by itself**, from the network's windows — the dates the sweep acts
/// on (`window_bind` from acceptance or the redraw, `window_receipt` from binding,
/// `window_challenge` from licensing, the disclose window from an accusation), a court's backstop
/// while one is open, and the retirement of a record that has ended.
pub fn palw_claim_phase_deadline_v1(
    claim: &crate::palw_state_v2::PalwClaimStateV2,
    state_params: &PalwStateParamsV2,
    court_backstop: Option<u64>,
) -> Option<u64> {
    use crate::palw_state_v2::PalwClaimPhaseV2 as P;
    match &claim.phase {
        P::Provisional => Some(claim.rebound_daa.unwrap_or(claim.accepted_daa).saturating_add(state_params.window_bind())),
        P::PanelBound { bound_daa } => Some(bound_daa.saturating_add(state_params.window_receipt())),
        P::ReceiptLicensed { licensed_daa } => court_backstop.or(Some(licensed_daa.saturating_add(state_params.window_challenge()))),
        P::DefaultDisputed { accused_daa, .. } => {
            Some(accused_daa.saturating_add(crate::palw_state_v2::palw_da_disclose_window_daa_v1(state_params)))
        }
        P::Final { final_daa } => {
            let retire = state_params.claim_retirement_daa();
            (retire > 0).then(|| final_daa.saturating_add(retire))
        }
        P::Voided { voided_daa, .. } => {
            let retire = state_params.claim_retirement_daa();
            (retire > 0).then(|| voided_daa.saturating_add(retire))
        }
    }
}

/// **A bond's claims, newest first** — as executor, or as a seat on their panels. `limit` bounds
/// the rows (0 = no bound); the bool says whether any were left out.
pub fn palw_claim_rows_v1(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    bond: &PalwBondKeyV2,
    role: PalwClaimRoleV1,
    include_terminal: bool,
    limit: usize,
) -> (Vec<PalwClaimRowV1>, bool) {
    use crate::palw_state_v2::{PalwClaimPhaseV2 as P, PalwClaimSourceV2 as S};
    let payouts: std::collections::BTreeMap<&Hash64, u64> = state.pending_payouts_iter().map(|(id, p)| (id, p.amount)).collect();
    let mut courts: std::collections::BTreeMap<Hash64, (usize, u64)> = std::collections::BTreeMap::new();
    for (_, session) in state.court_sessions_iter() {
        let entry = courts.entry(session.claim).or_insert((0, u64::MAX));
        entry.0 += 1;
        entry.1 = entry.1.min(session.deadline_daa);
    }
    let mut rows: Vec<PalwClaimRowV1> = state
        .claims_iter()
        .filter(|(_, claim)| include_terminal || !matches!(claim.phase, P::Final { .. } | P::Voided { .. }))
        .filter_map(|(id, claim)| {
            let panel = state.panel(id);
            let seated = panel.is_some_and(|p| p.seats.iter().any(|s| s.bond == *bond));
            let ours = match role {
                PalwClaimRoleV1::Executor => claim.bond == *bond,
                PalwClaimRoleV1::Seat => seated,
            };
            if !ours {
                return None;
            }
            let (quanta, quanta_spent, free_prompt) = match &claim.source {
                S::FreePrompt { quanta, spent } => (*quanta, spent.len() as u32, true),
                S::Attempt => (0, 0, false),
            };
            let court = courts.get(id).copied();
            Some(PalwClaimRowV1 {
                claim_id: *id,
                free_prompt,
                quanta,
                quanta_spent,
                class_id: claim.class_id,
                executor_bond: claim.bond,
                phase: claim.phase.clone(),
                accepted_daa: claim.accepted_daa,
                accepted_block: claim.accepted_block,
                rebound_daa: claim.rebound_daa,
                seats: panel.map(|p| p.seats.iter().map(|s| s.bond).collect()).unwrap_or_default(),
                bound_daa: panel.map(|p| p.bound_daa),
                deadline_daa: palw_claim_phase_deadline_v1(claim, state_params, court.map(|c| c.1)),
                reserved: claim.reserved,
                escrowed_reward: claim.escrowed_reward,
                // ADR-0152 A-KEY: a vested Final's producer leg is queued under its own key, never
                // under the raw claim id (phase2-plan §2.7); below the fence, exactly as before.
                payout_pending: match claim.phase {
                    P::Final { final_daa } if state_params.rcore_plus_active_at(final_daa) => {
                        payouts.get(&crate::palw_vesting_v1::palw_vesting_payout_key_v1(id)).copied()
                    }
                    _ => payouts.get(id).copied(),
                },
                work_leaves: claim.work_leaves,
                open_courts: court.map(|c| c.0).unwrap_or(0),
                exec_lane: palw_claim_exec_lane_v1(state, id),
            })
        })
        .collect();
    rows.sort_by(|a, b| b.accepted_daa.cmp(&a.accepted_daa).then_with(|| a.claim_id.cmp(&b.claim_id)));
    let truncated = limit > 0 && rows.len() > limit;
    if truncated {
        rows.truncate(limit);
    }
    (rows, truncated)
}

/// **The bond itself, read beside its claims** (ADR-0122 §6.5, `getPalwClaims`): what the registry
/// holds about it.
///
/// It is here for the one question no other read answered: which classes this bond is seated for.
/// A bond judges only the classes it declared (`palw_bond_may_judge_class_v2`), a registration
/// declares none, and nothing reported the set — so a setup could not tell a declared bond from an
/// undeclared one, and could only declare again and pay again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwBondSummaryV1 {
    /// The key the bond was registered under: who may sign for it.
    pub pubkey: Vec<u8>,
    /// `None` while Active; the DAA its retirement was requested at otherwise.
    pub retiring_since_daa: Option<u64>,
    pub collateral: u64,
    pub slashed: u64,
    pub registered_daa: u64,
    pub capable_classes: Vec<Hash64>,
}

pub fn palw_bond_summary_v1(state: &PalwChainStateV2, bond: &PalwBondKeyV2) -> Option<PalwBondSummaryV1> {
    let b = state.bond(bond)?;
    Some(PalwBondSummaryV1 {
        pubkey: b.pubkey.clone(),
        retiring_since_daa: match b.status {
            crate::palw_state_v2::PalwBondStatusV2::Active => None,
            crate::palw_state_v2::PalwBondStatusV2::Retiring { since_daa, .. } => Some(since_daa),
        },
        collateral: b.collateral,
        slashed: b.slashed,
        registered_daa: b.registered_daa,
        capable_classes: b.capable_classes.iter().copied().collect(),
    })
}

/// A bond's claims and the bond, at one tip — what `getPalwClaims` answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwBondClaimsV1 {
    pub tip_daa: u64,
    pub rows: Vec<PalwClaimRowV1>,
    pub truncated: bool,
    /// `None`: the registry holds no bond at that outpoint.
    pub bond: Option<PalwBondSummaryV1>,
    /// **Claim row v3 (ADR-0152 R-core+, phase2-plan §1.6): where each row's vested reward stands**,
    /// by claim id — only the rows that vested (past `palw_rcore_plus` at their Final). Kept beside
    /// the rows rather than in [`PalwClaimRowV1`], whose readers build it by hand. Empty from the
    /// node-policy entry ([`palw_bond_claims_v1`] with no `vesting_at`).
    pub vesting: std::collections::BTreeMap<Hash64, crate::palw_vesting_read_v1::PalwClaimVestingV1>,
    /// The rows of claims that RETIRED while their reward still vests (`include_terminal` only):
    /// retirement comes `claim_retirement` after Final, the row lives until it moves, and from
    /// retirement on the row is the reward's only record. Newest Final first.
    pub vesting_only: Vec<crate::palw_vesting_read_v1::PalwVestingRowReadV1>,
    pub vesting_only_truncated: bool,
}

/// **`getPalwClaims`' whole answer at one tip** (ADR-0122 §6.5, claim row v3): the rows
/// ([`palw_claim_rows_v1`]), the bond, and — with `vesting_at` — the vesting half read at the next
/// block: `(next_daa, raw_depth)`, the DAA the next block folds at and the raw second-clock depth
/// there, the facts its step 3d reads (`palw_vesting_read_v1`'s I-8 inputs). Below
/// `palw_rcore_plus` no row exists and the vesting half is empty.
///
/// **`vesting_at: None` is the node-policy entry: the rows alone**, the vesting half empty. Only the
/// RPC asks for claim row v3's half; the panel loop's per-bond reads (`kaspad`'s
/// `palw_claim_rows_v1` callers read a row's phase and deadline) must not pay for a next-block plan,
/// up to 500 claim positions and a retired-row read each 30 s per bond, only to drop them
/// (review of P2-10, finding 4).
pub fn palw_bond_claims_v1(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    bond: &PalwBondKeyV2,
    role: PalwClaimRoleV1,
    include_terminal: bool,
    limit: usize,
    vesting_at: Option<(u64, Option<u64>)>,
) -> PalwBondClaimsV1 {
    use crate::palw_vesting_read_v1::{PalwVestingReaderV1, palw_vesting_only_rows_v1};
    let tip_daa = state.last_point().map(|p| p.daa_score).unwrap_or(0);
    let (rows, truncated) = palw_claim_rows_v1(state, state_params, bond, role, include_terminal, limit);
    let (vesting, (vesting_only, vesting_only_truncated)) = match vesting_at {
        Some((next_daa, raw_depth)) => {
            let reader = PalwVestingReaderV1::new(state, state_params, next_daa, raw_depth);
            let claims: Vec<(Hash64, Option<&crate::palw_state_v2::PalwClaimStateV2>)> =
                rows.iter().map(|row| (row.claim_id, state.claim(&row.claim_id))).collect();
            let only = if include_terminal {
                palw_vesting_only_rows_v1(&reader, bond, role == PalwClaimRoleV1::Executor, limit)
            } else {
                (Vec::new(), false)
            };
            (reader.claim_stages(&claims), only)
        }
        None => (Default::default(), (Vec::new(), false)),
    };
    PalwBondClaimsV1 {
        tip_daa,
        rows,
        truncated,
        bond: palw_bond_summary_v1(state, bond),
        vesting,
        vesting_only,
        vesting_only_truncated,
    }
}

pub fn palw_seat_duties_v2(state: &PalwChainStateV2, state_params: &PalwStateParamsV2, mine: &[PalwBondKeyV2]) -> Vec<PalwSeatDutyV2> {
    let mut out = Vec::new();
    for (claim_id, claim) in state.claims_iter() {
        // Only a bound panel owes receipts; every other phase is somebody else's edge.
        let crate::palw_state_v2::PalwClaimPhaseV2::PanelBound { bound_daa } = claim.phase else {
            continue;
        };
        let Some(panel) = state.panel(claim_id) else { continue };
        // The class's registered root, read where the claim is read. A claim whose class is gone
        // from the registry is not judgeable by anyone, so it yields no duty rather than a duty
        // nobody can act on.
        let Some(class_artifact_root) = state.class(&claim.class_id).map(|c| c.artifact_root) else {
            continue;
        };
        for (seat_index, seat) in panel.seats.iter().enumerate() {
            if !mine.contains(&seat.bond) {
                continue;
            }
            out.push(PalwSeatDutyV2 {
                panel_anchor: panel.anchor,
                seat_index: seat_index as u8,
                panel_seat_count: panel.seats.len() as u16,
                accepted_block: claim.accepted_block,
                claim_id: *claim_id,
                class_id: claim.class_id,
                artifact_root: class_artifact_root,
                seat_bond: seat.bond,
                executor_bond: claim.bond,
                execution_root: claim.execution_root,
                trace_root: claim.trace_root,
                output_root: claim.output_root,
                bound_daa,
                receipt_deadline: bound_daa.saturating_add(state_params.window_receipt()),
                pwu: claim.pwu,
                quanta: match claim.source {
                    crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { quanta, .. } => quanta,
                    _ => 0,
                },
                free_prompt: matches!(claim.source, crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { .. }),
                work_leaves: claim.work_leaves,
            });
        }
    }
    out
}
