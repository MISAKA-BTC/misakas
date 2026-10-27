//! **Typed waivers in arm-readiness records** (lane ACT2, user decision 2026-10-27: arm the DAA 17,000 list with the drill skipped).
//!
//! A record carries one 32-byte digest per requirement. Until now every digest had to be the bytes of a run's output. A waiver is the
//! other kind: a typed statement that the requirement was **not run** (or is a user decision, not a run), by whom, and why. Its digest
//! is [`PalwArmingWaiverV1::digest`] over exactly that (fence, network, height, requirement index, kind, reason, decided_by), and the
//! waiver itself is pinned in [`PALW_ARMING_WAIVERS_V1`], so the binary can always tell a waived requirement from a run one
//! ([`palw_arming_evidence_kind_v1`]). A waiver is never reported as run or passed.
//!
//! **What a waiver does NOT do.** It stands only in a record's evidence slot, i.e. it answers "is there a record for this fence,
//! network and height". Every other refusal — ordering ("X needs Y at or below it"), a missing or placeholder value, a measurement
//! that does not derive the armed policy, missing wiring, an open blocker (`PALW_RFC0608_OPEN_BLOCKERS_V1`) — is a separate check
//! that never reads a record's evidence, so no waiver can lift it (tests in `consensus/core/tests/palw_t12_g14_17000_arming.rs`).
//!
//! Hashes here are data identity only (PRINCIPLES §8); nothing proves a computation.

use crate::Hash64;

/// The requirement index a waiver uses for a record's **review** slot (not a requirement of the list).
pub const PALW_ARMING_WAIVER_REVIEW_V1: u16 = u16::MAX;

/// The digest key (blake2b-256, keyed). `scripts/release/pin_arming_record.py` computes the same bytes.
pub const PALW_ARMING_WAIVER_KEY_V1: &[u8] = b"misaka-palw/arming-waiver/v1";

/// What a waiver says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwArmingWaiverKindV1 {
    /// The requirement was not run (drill, E2E, measurement, suite, review). Never "passed".
    NotRun = 1,
    /// The requirement is a user decision (activation height, a ruling), recorded as the decision, not as a run.
    UserDecision = 2,
}

/// **One waiver** — for exactly one (fence, network, height, requirement).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwArmingWaiverV1 {
    /// The `Params` field name of the fence (e.g. `palw_legacy_public_filer_v1`).
    pub fence: &'static str,
    pub genesis: Hash64,
    pub activation_daa: u64,
    /// The index into the fence's requirement list, or [`PALW_ARMING_WAIVER_REVIEW_V1`].
    pub requirement: u16,
    pub kind: PalwArmingWaiverKindV1,
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
        put(&mut s, self.reason.as_bytes());
        put(&mut s, self.decided_by.as_bytes());
        s.finalize().as_bytes().try_into().expect("32-byte digest")
    }

    /// The waiver's own form.
    pub fn form(&self) -> Result<(), &'static str> {
        if self.fence.is_empty() || self.reason.trim().is_empty() {
            return Err("an arming waiver names no fence or no reason");
        }
        if !self.decided_by.starts_with("user ") {
            return Err("an arming waiver must be decided by the user (decided_by \"user <date>\")");
        }
        if self.activation_daa == 0 {
            return Err("an arming waiver names no height");
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

/// **Classify one slot.** A digest equal to a pinned waiver must be that waiver's own (fence, network, height, slot) and of good
/// form; a waiver reused elsewhere is refused. Any other digest is a run's.
pub fn palw_arming_evidence_kind_v1(
    fence: &str,
    genesis: Hash64,
    activation_daa: u64,
    slot: u16,
    digest: &[u8; 32],
    waivers: &[PalwArmingWaiverV1],
) -> Result<PalwArmingEvidenceV1, &'static str> {
    let Some(w) = waivers.iter().find(|w| w.digest() == *digest) else { return Ok(PalwArmingEvidenceV1::Run) };
    w.form()?;
    if w.fence != fence || w.genesis != genesis || w.activation_daa != activation_daa || w.requirement != slot {
        return Err("an arming waiver is used outside its own fence, network, height and requirement");
    }
    Ok(PalwArmingEvidenceV1::Waived(*w))
}

/// Check every slot of a record (evidence then review) against the pinned waivers.
pub fn palw_arming_record_waivers_hold_v1(
    fence: &str,
    genesis: Hash64,
    activation_daa: u64,
    evidence: &[[u8; 32]],
    review: &[u8; 32],
    waivers: &[PalwArmingWaiverV1],
) -> Result<(), (usize, &'static str)> {
    for (i, d) in evidence.iter().enumerate() {
        palw_arming_evidence_kind_v1(fence, genesis, activation_daa, i as u16, d, waivers).map_err(|e| (i, e))?;
    }
    palw_arming_evidence_kind_v1(fence, genesis, activation_daa, PALW_ARMING_WAIVER_REVIEW_V1, review, waivers)
        .map_err(|e| (evidence.len(), e))?;
    Ok(())
}

/// **The pinned waivers** (testnet-12, DAA 17,000; generated by `scripts/release/gen_t12_17000_records.py`).
pub const PALW_ARMING_WAIVERS_V1: &[PalwArmingWaiverV1] = crate::palw_t12_17000_records_v1::PALW_T12_17000_WAIVERS_V1;
