//! **The sealed-source PALW Work Beacon v3** (`docs/design/palw/opv-beacon-bootstrap.md` §6.3): commit–reveal over bonded claim
//! seals, mixing **every** qualifying seal of the window, with withholding and abandonment turned into a counted veto.
//!
//! ```text
//!            S = commitment + anchor delay
//!            |<------ seal window W ------>|<----- reveal window W ----->|  ... every mixed source Final ... + D = lock
//! seals:     [S, S + W)    each binds a secret salt     (claim seal = H(claim id ‖ salt), bonded: seal_deposit)
//! reveals:   [S + W, S + 2W)  the claim and its salt    (a reveal before S + W is public too early: not a source, no veto)
//! mixed:     ALL in-window seals of eligible profiles, one per producer (and per known consumer), in seal order
//! veto:      a mixed seal unrevealed by S + 2W, or a mixed source that ends without a standing Final → VETOED (counted retry)
//! ```
//!
//! What it closes (SOUND SG-01 / SG-01a):
//!
//! * **The last contributor** (v2's unbounded offline grinding): every mixed contribution is fixed — sealed — before any mixed salt
//!   is public, so no contribution is chosen after another is seen. An adversary's only decision after the reveals is to let the
//!   beacon lock or to veto it, and a veto ends the attempt as a counted retry: the choices per beacon are the fork alternatives `F`
//!   alone ([`sealed_beacon_grinding_choices_v3`]); the `R + 1` attempts are the retry term of the effective accounting.
//! * **First-`k` capture**: nothing is "the first `k`" — every qualifying seal of the window is mixed, so sealing early or sealing many
//!   never pushes an honest seal out. `work_count_k` is only the quorum of distinct producers.
//! * **Censorship of an honest reveal**: a mixed seal that is not revealed in the reveal window vetoes; it is never dropped. The one
//!   way to keep an honest salt out of a LOCKED beacon is to keep its seal out of the seal window entirely — censoring every honest
//!   seal for the window's length, which an adversary holding a share `ρ` of the blocks does with probability at most
//!   `ρ^((W − δ)·b)` ([`sealed_source_censorship_bits_v3`]): that, and an honest producer's participation, is `ε_src`.
//!
//! What it needs from the ledger (not in this crate): a claim seal that binds a SECRET salt and a reveal that carries it. A seal over
//! the claim id alone (`claim_seal_v1`) hides nothing — a claim of a deterministic class is a function of its public job and its
//! producer — so a v3 beacon over such seals would be a v2 beacon with extra steps (GAP-B1a in the design note).

use std::collections::BTreeSet;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::beacon::{
    BeaconContextV1, BeaconEvidenceRefusalV1, BeaconSourceV1, FinalPathV1, SourceAttributionV1, VerifiedWorkBeaconV1, WorkBeaconV1,
    WorkFinalEventV1, WorkSourceKindV1, challenge_anchor_v1, compare_presented_v1,
};
use crate::hash::{Digest, object_id};
use crate::policy::{PolicyRefusalV1, SourceRuleV1};
use crate::soundness::EffectiveBitsV1;
use crate::subject::{RootV1, SubjectKindV1};

pub const DOMAIN_SEALED_BEACON_V3: &[u8] = b"MISAKA/PALW/WORK-BEACON/SEALED/V3";
pub const DOMAIN_SEALED_BEACON_ITEM_V3: &[u8] = b"MISAKA/PALW/WORK-BEACON/SEALED/ITEM/V3";
pub const DOMAIN_SEALED_BEACON_MIX_V3: &[u8] = b"MISAKA/PALW/WORK-BEACON/SEALED/MIX/V3";

/// **One claim seal**, as a consumer derives it from authenticated state (never from what a producer asserts about itself): the
/// profile (the sealed job's class), who stands behind it, the seal digest and the position it was accepted at, and — once the
/// claim is revealed over it — the reveal.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct SealedSourceV3 {
    pub source_profile_id: Digest,
    pub attribution: SourceAttributionV1,
    pub seal: Digest,
    pub seal_position: u64,
    pub reveal: Option<SealRevealV3>,
}

/// The reveal of a sealed claim: where it was accepted, the salt the seal bound (public only from here), and the claim's fate.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct SealRevealV3 {
    pub reveal_position: u64,
    pub salt: Digest,
    pub fate: SourceFateV3,
}

/// What became of a revealed claim (as of the facts given).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum SourceFateV3 {
    /// Revealed and not yet decided.
    Live,
    /// Final, with the facts a beacon checks (`accepted_position` is the reveal's position).
    Final(WorkFinalEventV1),
    /// Ended without a standing Final (convicted, unavailable, timed out).
    Failed,
}

/// The state of a sealed-source beacon at a tip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SealedBeaconStateV3 {
    /// The seal window is open: `sealed` qualifying seals so far, `need` distinct producers for the quorum.
    Sealing {
        sealed: u32,
        need: u32,
    },
    /// The seal window closed with `mixed ≥ k`; the reveal window is open.
    Revealing {
        mixed: u32,
        revealed: u32,
    },
    /// Every mixed seal was revealed in time; some sources are not Final yet.
    Settling {
        mixed: u32,
        finals: u32,
    },
    /// Every mixed source is Final; the last settlement is not yet at depth `D`.
    Candidate {
        mixed: u32,
        lock_position: u64,
    },
    Locked(VerifiedWorkBeaconV1),
    /// `BEACON_UNAVAILABLE`: the seal window closed with fewer than `k` distinct producers. Not fraud, not a pass.
    Unavailable {
        mixed: u32,
        need: u32,
    },
    /// `BEACON_VETOED`: a mixed seal was not revealed in the reveal window (`withheld`), or a mixed source ended without a standing
    /// Final (`failed`). The beacon never locks without them; the attempt ends as a counted retry.
    Vetoed {
        withheld: u32,
        failed: u32,
    },
}

/// `S + W`: the end (exclusive) of the seal window, where the reveal window opens.
pub fn seal_window_end_v3(ctx: &BeaconContextV1) -> u64 {
    ctx.start().saturating_add(ctx.policy.beacon_window_slots)
}

/// `S + 2W`: the end (exclusive) of the reveal window.
pub fn reveal_window_end_v3(ctx: &BeaconContextV1) -> u64 {
    seal_window_end_v3(ctx).saturating_add(ctx.policy.beacon_window_slots)
}

fn profile_qualifies(ctx: &BeaconContextV1, profile: &Digest) -> bool {
    !ctx.excluded_profiles.contains(profile)
        && ctx.candidate_profile_id != RootV1::Present(*profile)
        && ctx.eligible_profiles.contains(profile)
}

/// **The mixed seals at `tip`**, canonical: every seal accepted in `[S, S + W)` on a profile frozen eligible (never the candidate or
/// an excluded profile), except one revealed before `S + W` (public while sealing was still open: not hidden, not a source); then
/// in seal order `(seal_position, seal, …)` the first seal of each producer and of each known consumer. Before `S + W` it is the
/// prefix sealed so far; from `S + W` it is final (every fact it reads is then on the branch).
pub fn mixed_seals_v3<'a>(ctx: &BeaconContextV1, seals: &'a [SealedSourceV3], tip: u64) -> Vec<&'a SealedSourceV3> {
    let (start, close) = (ctx.start(), seal_window_end_v3(ctx));
    let mut candidates: Vec<&SealedSourceV3> = seals
        .iter()
        .filter(|s| s.seal_position >= start && s.seal_position < close && s.seal_position <= tip)
        .filter(|s| profile_qualifies(ctx, &s.source_profile_id))
        .filter(|s| !s.reveal.as_ref().is_some_and(|r| r.reveal_position < close && r.reveal_position <= tip))
        .collect();
    candidates.sort_by(|a, b| (a.seal_position, a.seal).cmp(&(b.seal_position, b.seal)).then_with(|| a.cmp(b)));
    let (mut producers, mut consumers) = (BTreeSet::new(), BTreeSet::new());
    let mut out = Vec::new();
    for s in candidates {
        if producers.contains(&s.attribution.producer_id) {
            continue;
        }
        if let RootV1::Present(c) = s.attribution.consumer_id
            && consumers.contains(&c)
        {
            continue;
        }
        producers.insert(s.attribution.producer_id);
        if let RootV1::Present(c) = s.attribution.consumer_id {
            consumers.insert(c);
        }
        out.push(s);
    }
    out
}

/// Whether a mixed source's Final stands as a source: real useful work of the sealed profile, not depending on the candidate,
/// revealed after the seal window, standing Final, DA-satisfied, valid independently, Panel-independent for a Panel draw.
fn final_qualifies_v3(ctx: &BeaconContextV1, seal: &SealedSourceV3, reveal: &SealRevealV3, ev: &WorkFinalEventV1) -> bool {
    let is_candidate = |p: &Digest| ctx.candidate_profile_id == RootV1::Present(*p);
    ev.kind == WorkSourceKindV1::RealUsefulWork
        && ev.source_profile_id == seal.source_profile_id
        && !ev.depends_on_profiles.iter().any(|p| ctx.excluded_profiles.contains(p) || is_candidate(p))
        && ev.accepted_position == reveal.reveal_position
        && ev.accepted_position >= seal_window_end_v3(ctx)
        && ev.claim_final
        && ev.da_satisfied
        && ev.validity_independent
        && (ctx.subject_kind != SubjectKindV1::PanelAssignment || ev.final_path == FinalPathV1::PanelIndependent)
}

fn item_v3(seal: &SealedSourceV3, salt: &Digest, ev: &WorkFinalEventV1) -> Digest {
    object_id(
        DOMAIN_SEALED_BEACON_ITEM_V3,
        &(
            seal.seal_position,
            seal.seal,
            seal.attribution.producer_id,
            seal.source_profile_id,
            ev.canonical_work_id,
            ev.execution_commitment,
            *salt,
        ),
    )
}

/// `acc_0` of a sealed-source beacon.
pub fn initial_accumulator_v3(ctx: &BeaconContextV1) -> Digest {
    object_id(DOMAIN_SEALED_BEACON_V3, &(ctx.chain_genesis, ctx.ruleset_id, ctx.policy.id(), ctx.commitment_root, ctx.challenge_epoch))
}

/// **The sealed-source beacon at `tip_position`** of the branch whose seals (with their reveals and fates) are `seals`, any order.
/// Only a policy whose randomness source is the sealed-source beacon is collected here; a v2 policy is refused (and v2's collectors
/// refuse a v3 policy).
pub fn collect_sealed_work_beacon_v3(
    ctx: &BeaconContextV1,
    seals: &[SealedSourceV3],
    tip_position: u64,
) -> Result<SealedBeaconStateV3, PolicyRefusalV1> {
    ctx.policy.validate()?;
    if !ctx.policy.is_sealed_source() {
        return Err(PolicyRefusalV1::WrongCollector("a v2 (accumulator) policy is collected by collect_attributed_work_beacon_v1"));
    }
    debug_assert_eq!(ctx.policy.source_rule(), Some(SourceRuleV1::Distinct), "validate() pins the v3 source rule");
    if matches!(ctx.subject_kind, SubjectKindV1::ModelConformance | SubjectKindV1::KernelConformance)
        && ctx.candidate_profile_id == RootV1::Absent
    {
        return Err(PolicyRefusalV1::Missing("candidate_profile_id"));
    }
    let need = ctx.policy.work_count_k;
    let mixed = mixed_seals_v3(ctx, seals, tip_position);
    let n = mixed.len() as u32;
    if tip_position < seal_window_end_v3(ctx) {
        return Ok(SealedBeaconStateV3::Sealing { sealed: n, need });
    }
    if n < need {
        return Ok(SealedBeaconStateV3::Unavailable { mixed: n, need });
    }
    let close = reveal_window_end_v3(ctx);
    // A reveal exists on this branch only once accepted by the tip, and counts only inside the reveal window.
    let in_time = |s: &SealedSourceV3| -> Option<SealRevealV3> {
        s.reveal.as_ref().filter(|r| r.reveal_position < close && r.reveal_position <= tip_position).cloned()
    };
    if tip_position < close {
        let revealed = mixed.iter().filter(|s| in_time(s).is_some()).count() as u32;
        return Ok(SealedBeaconStateV3::Revealing { mixed: n, revealed });
    }
    let (mut withheld, mut failed, mut live) = (0u32, 0u32, 0u32);
    let mut finals: Vec<(&SealedSourceV3, SealRevealV3, WorkFinalEventV1)> = Vec::new();
    for s in &mixed {
        let Some(r) = in_time(s) else {
            withheld += 1;
            continue;
        };
        match r.fate.clone() {
            SourceFateV3::Final(ev) if ev.settlement_position <= tip_position => {
                if final_qualifies_v3(ctx, s, &r, &ev) {
                    finals.push((*s, r, ev));
                } else {
                    failed += 1;
                }
            }
            SourceFateV3::Final(_) | SourceFateV3::Live => live += 1,
            SourceFateV3::Failed => failed += 1,
        }
    }
    if withheld > 0 || failed > 0 {
        return Ok(SealedBeaconStateV3::Vetoed { withheld, failed });
    }
    if live > 0 {
        return Ok(SealedBeaconStateV3::Settling { mixed: n, finals: finals.len() as u32 });
    }
    let last = finals.iter().map(|(_, _, ev)| ev.settlement_position).max().expect("k ≥ 1 mixed, all Final");
    let lock_position = last.saturating_add(ctx.policy.settlement_depth_d);
    if tip_position < lock_position {
        return Ok(SealedBeaconStateV3::Candidate { mixed: n, lock_position });
    }
    let mut accumulators = vec![initial_accumulator_v3(ctx)];
    let mut sources = Vec::with_capacity(finals.len());
    for (i, (s, r, ev)) in finals.iter().enumerate() {
        let item = item_v3(s, &r.salt, ev);
        let next = object_id(DOMAIN_SEALED_BEACON_MIX_V3, &(*accumulators.last().expect("acc_0"), (i + 1) as u32, item));
        accumulators.push(next);
        sources.push(BeaconSourceV1 {
            source_profile_id: s.source_profile_id,
            canonical_work_id: ev.canonical_work_id,
            execution_commitment: ev.execution_commitment,
            accepted_position: r.reveal_position,
            settlement_position: ev.settlement_position,
            occurrence_index: ev.occurrence_index,
        });
    }
    let output = *accumulators.last().expect("acc_n");
    let challenge_anchor = challenge_anchor_v1(ctx, &sources);
    Ok(SealedBeaconStateV3::Locked(VerifiedWorkBeaconV1::derived(
        WorkBeaconV1 { sources, accumulators, output, challenge_anchor, lock_position },
        ctx.id(),
    )))
}

/// **A fresh node checks a presented v3 beacon** against its own seals: anything but the beacon it derives itself is refused.
pub fn verify_sealed_work_beacon_v3(
    ctx: &BeaconContextV1,
    presented: &WorkBeaconV1,
    seals: &[SealedSourceV3],
    tip_position: u64,
) -> Result<VerifiedWorkBeaconV1, BeaconEvidenceRefusalV1> {
    match collect_sealed_work_beacon_v3(ctx, seals, tip_position).map_err(BeaconEvidenceRefusalV1::Policy)? {
        SealedBeaconStateV3::Locked(b) => compare_presented_v1(b, presented),
        other => Err(BeaconEvidenceRefusalV1::NotLockedV3(format!("{other:?}"))),
    }
}

// =================================================================================================================================
// The accounting of a v3 beacon
// =================================================================================================================================

/// **The grinding choices per v3 beacon: the fork alternatives `F` alone.** Every mixed contribution is sealed before any mixed salt
/// is public; after the reveals an adversary can only let the beacon lock or veto it, and a veto is a counted retry (charged as
/// `⌈log2 (R + 1)⌉`, never here). Timing, ordering and selection among works are not choices: the mix is every qualifying seal, in
/// seal order.
pub fn sealed_beacon_grinding_choices_v3(fork_alternatives: u128) -> u128 {
    fork_alternatives.max(1)
}

/// **`ε_src` of a v3 beacon, in bits (floored): censoring every honest seal out of the seal window.** An honest producer's seal is
/// broadcast at the window's start; it is accepted in-window unless every block that could carry it before `S + W − δ` is the
/// adversary's (`δ` the bound on how late a block that carries it can be merged). With `b` blocks per DAA and an adversary holding a
/// share `ρ` of the blocks (stated as `−log2 ρ` in millibits: 1,000 for ½, 1,585 for ⅓), that is at most `ρ^((W − δ)·b)`.
///
/// It assumes an honest producer seals an eligible source in the window at all (participation), honest blocks are not filled by the
/// adversary's fee-paying spam, and `W ≤` the seal TTL / 2 (so every in-window reveal is legal): those are environment assumptions
/// (A-B2), stated beside the number, not inside it.
pub fn sealed_source_censorship_bits_v3(
    seal_window_daa: u64,
    merge_delay_daa: u64,
    blocks_per_daa: u64,
    adversary_neg_log2_millibits: u64,
) -> u64 {
    let blocks = (seal_window_daa.saturating_sub(merge_delay_daa) as u128).saturating_mul(blocks_per_daa as u128);
    (blocks.saturating_mul(adversary_neg_log2_millibits as u128) / 1_000).min(u64::MAX as u128) as u64
}

/// **Two failure events, one bound**: `2^−a + 2^−b ≤ 2^−(min(a, b) − 1)`. `Complete` (ε = 0) leaves the other term; two `Complete`
/// stay `Complete`.
pub fn combine_failure_bits_v1(a: EffectiveBitsV1, b: EffectiveBitsV1) -> EffectiveBitsV1 {
    match (a, b) {
        (EffectiveBitsV1::Complete, x) | (x, EffectiveBitsV1::Complete) => x,
        (EffectiveBitsV1::Bits(x), EffectiveBitsV1::Bits(y)) => EffectiveBitsV1::Bits(x.min(y).saturating_sub(1)),
    }
}
