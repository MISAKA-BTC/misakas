//! **Typed waivers in arm-readiness records** (lane ACT2, user decisions 2026-10-27; narrowed by F1 2026-10-27).
//!
//! A record carries one 32-byte digest per requirement. Normally every digest is the bytes of a run's output. A waiver is a typed,
//! pinned statement that stands in ONE slot instead, and only where the user permitted that kind of statement:
//!
//! | kind | allowed in | carries |
//! |---|---|---|
//! | `DrillSkipped` | a drill slot | the user's decision to skip the drill (2026-10-27) |
//! | `E2ENotRun` | a node-E2E slot | the user's decision to arm without that E2E (2026-10-27) |
//! | `ProvisionalEstimate` | a measured-value slot | the estimate VALUE and its basis (user 2026-10-22: values may be estimates) |
//! | `AgentReview` | the review slot or a review requirement | the Claude-agent review report (user-approved reviewer) |
//! | `UserDecision` | a slot whose requirement IS a user decision / approval | the decision |
//!
//! Anything else — a regression / suite / restart / accounting slot that was not run, a kind in the wrong slot, an estimate without a
//! value, a review without a report — is refused at the record's form check (`Evidence(i)`). Its digest is
//! [`PalwArmingWaiverV1::digest`] over (fence, network, height, slot, kind, value, reason, decided_by); the waiver is pinned in
//! [`PALW_ARMING_WAIVERS_V1`], so a waived slot is always told apart from a run one ([`palw_arming_evidence_kind_v1`]) and never
//! reported as run or passed.
//!
//! **What a waiver does NOT do.** It stands only in a record's evidence slot. Ordering, a missing or placeholder value, a measurement
//! that does not derive the armed policy, missing wiring and open blockers are separate checks that never read a record's evidence
//! (tests in `consensus/core/tests/palw_t12_g14_17000_arming.rs`).
//!
//! Hashes here are data identity only (PRINCIPLES §8); nothing proves a computation.

use crate::Hash64;

/// The requirement index a waiver uses for a record's **review** slot (not a requirement of the list).
pub const PALW_ARMING_WAIVER_REVIEW_V1: u16 = u16::MAX;

/// The digest key (blake2b-256, keyed). `scripts/release/pin_arming_record.py` computes the same bytes.
pub const PALW_ARMING_WAIVER_KEY_V1: &[u8] = b"misaka-palw/arming-waiver/v2";

/// What a waiver says (F1: only these kinds exist).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwArmingWaiverKindV1 {
    /// The user decided to skip this drill (2026-10-27). Never "passed".
    DrillSkipped = 1,
    /// The user decided to arm without this node E2E (2026-10-27). Never "passed".
    E2ENotRun = 2,
    /// A measured value replaced by a provisional estimate: `value` is the estimate, `reason` its basis (user 2026-10-22).
    ProvisionalEstimate = 3,
    /// The independent review done by a Claude agent (user-approved): `value` names the report (commit or path).
    AgentReview = 4,
    /// The requirement is itself a user decision / approval: `value` states it.
    UserDecision = 5,
}

/// What a slot asks for, read from its requirement text (the review slot is [`PALW_ARMING_WAIVER_REVIEW_V1`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwArmingSlotClassV1 {
    Drill,
    NodeE2E,
    Measured,
    Review,
    Decision,
    /// A suite / regression / restart / accounting run: no waiver kind fills it.
    Run,
}

/// **The class of slot `slot` whose requirement text is `requirement`.**
pub fn palw_arming_slot_class_v1(slot: u16, requirement: &str) -> PalwArmingSlotClassV1 {
    use PalwArmingSlotClassV1::*;
    if slot == PALW_ARMING_WAIVER_REVIEW_V1 {
        return Review;
    }
    let t = requirement.to_ascii_lowercase();
    if t.contains("user decision") || t.contains("user's approval") {
        Decision
    } else if t.starts_with("drill") || t.contains(" drill") {
        Drill
    } else if t.contains("measured") {
        Measured
    } else if t.starts_with("review") || t.contains(" review") {
        Review
    } else if t.contains("e2e") {
        NodeE2E
    } else {
        Run
    }
}

/// **One waiver** — for exactly one (fence, network, height, slot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwArmingWaiverV1 {
    /// The `Params` field name of the fence (e.g. `palw_legacy_public_filer_v1`).
    pub fence: &'static str,
    pub genesis: Hash64,
    pub activation_daa: u64,
    /// The index into the fence's requirement list, or [`PALW_ARMING_WAIVER_REVIEW_V1`].
    pub requirement: u16,
    pub kind: PalwArmingWaiverKindV1,
    /// The estimate value (`ProvisionalEstimate`), the report (`AgentReview`) or the decision (`UserDecision`); empty otherwise.
    pub value: &'static str,
    /// Why (for an estimate: its basis).
    pub reason: &'static str,
    /// Who decided — must name the user (`user 2026-10-27`); an agent cannot waive.
    pub decided_by: &'static str,
}

fn put(s: &mut blake2b_simd::State, b: &[u8]) {
    s.update(&(b.len() as u32).to_le_bytes());
    s.update(b);
}

impl PalwArmingWaiverV1 {
    /// The digest a record carries in the waived slot.
    pub fn digest(&self) -> [u8; 32] {
        let mut s = blake2b_simd::Params::new().hash_length(32).key(PALW_ARMING_WAIVER_KEY_V1).to_state();
        put(&mut s, self.fence.as_bytes());
        put(&mut s, self.genesis.as_bytes().as_slice());
        s.update(&self.activation_daa.to_le_bytes());
        s.update(&self.requirement.to_le_bytes());
        s.update(&[self.kind as u8]);
        put(&mut s, self.value.as_bytes());
        put(&mut s, self.reason.as_bytes());
        put(&mut s, self.decided_by.as_bytes());
        s.finalize().as_bytes().try_into().expect("32-byte digest")
    }

    /// The waiver's own form, against the slot it stands in.
    pub fn form(&self, class: PalwArmingSlotClassV1) -> Result<(), &'static str> {
        use PalwArmingSlotClassV1 as C;
        use PalwArmingWaiverKindV1 as K;
        if self.fence.is_empty() || self.reason.trim().is_empty() || self.activation_daa == 0 {
            return Err("an arming waiver names no fence, height or reason");
        }
        if !self.decided_by.starts_with("user ") {
            return Err("an arming waiver must be decided by the user (decided_by \"user <date>\")");
        }
        let needs_value = matches!(self.kind, K::ProvisionalEstimate | K::AgentReview | K::UserDecision);
        if needs_value && self.value.trim().is_empty() {
            return Err("an estimate waiver carries its value, a review waiver its report, a decision waiver the decision");
        }
        let fits = matches!(
            (self.kind, class),
            (K::DrillSkipped, C::Drill)
                | (K::E2ENotRun, C::NodeE2E)
                | (K::ProvisionalEstimate, C::Measured)
                | (K::AgentReview, C::Review)
                | (K::UserDecision, C::Decision)
        );
        if !fits {
            return Err("this waiver kind is not permitted in this slot (F1)");
        }
        Ok(())
    }
}

/// What one evidence slot of a record is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwArmingEvidenceV1 {
    /// The digest of a run's output (not a pinned waiver).
    Run,
    /// A pinned waiver for exactly this slot.
    Waived(PalwArmingWaiverV1),
}

/// **Classify one slot.** A digest equal to a pinned waiver must be that waiver's own (fence, network, height, slot), of a kind the
/// slot permits and of good form; anything else is refused. Any other digest is a run's.
pub fn palw_arming_evidence_kind_v1(
    fence: &str,
    genesis: Hash64,
    activation_daa: u64,
    slot: u16,
    requirement: &str,
    digest: &[u8; 32],
    waivers: &[PalwArmingWaiverV1],
) -> Result<PalwArmingEvidenceV1, &'static str> {
    let Some(w) = waivers.iter().find(|w| w.digest() == *digest) else { return Ok(PalwArmingEvidenceV1::Run) };
    if w.fence != fence || w.genesis != genesis || w.activation_daa != activation_daa || w.requirement != slot {
        return Err("an arming waiver is used outside its own fence, network, height and requirement");
    }
    w.form(palw_arming_slot_class_v1(slot, requirement))?;
    Ok(PalwArmingEvidenceV1::Waived(*w))
}

/// Check every slot of a record (evidence against `requirements`, then the review) against the pinned waivers.
pub fn palw_arming_record_waivers_hold_v1(
    fence: &str,
    genesis: Hash64,
    activation_daa: u64,
    requirements: &[&str],
    evidence: &[[u8; 32]],
    review: &[u8; 32],
    waivers: &[PalwArmingWaiverV1],
) -> Result<(), (usize, &'static str)> {
    for (i, d) in evidence.iter().enumerate() {
        let text = requirements.get(i).copied().unwrap_or("");
        palw_arming_evidence_kind_v1(fence, genesis, activation_daa, i as u16, text, d, waivers).map_err(|e| (i, e))?;
    }
    palw_arming_evidence_kind_v1(fence, genesis, activation_daa, PALW_ARMING_WAIVER_REVIEW_V1, "", review, waivers)
        .map_err(|e| (evidence.len(), e))?;
    Ok(())
}

/// **The pinned waivers** (testnet-12 at the G14 flag day; generated by `scripts/release/gen_t12_17000_records.py`).
pub const PALW_ARMING_WAIVERS_V1: &[PalwArmingWaiverV1] = crate::palw_t12_17000_records_v1::PALW_T12_17000_WAIVERS_V1;
