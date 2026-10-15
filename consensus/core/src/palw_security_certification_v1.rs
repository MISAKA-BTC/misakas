//! **RFC-0018 §7 — `SecurityCertified`: the per-claim independent positive verification behind Gate C** (ユーザー方針 2026-10-15:
//! 「この攻撃を塞ぐ方針で進める。DNS-BFTはRFCにある通り廃止したので使用できない。」). Fence `palw_independently_work_certified_v1`
//! (**DORMANT**: `None` on every preset, refused when armed; activation is a separate user decision).
//!
//! Two claim states are kept apart:
//!
//! * **`Final`** — the claim procedure completed; economic settlement of the user-funded escrow follows the existing rules.
//! * **`SecurityCertified`** — the claim may add CONSENSUS credit: safe-frontier advance, safe weight, provisional/live weight,
//!   settlement depth, rule-E participation, settled-anchor credit and the issuance-funded (coinbase) reward leg.
//!
//! A claim is `SecurityCertified` only by a **commit-then-draw, m-of-k positive attestation from the global verifier pool**:
//!
//! 1. **Commit** at `Final` (the claim, its producer and the participating stake are fixed: the population is cut at the commit DAA —
//!    no bond registered within `verifier_maturity_daa` of it, and never the producer's own operator).
//! 2. **Draw** `m` positions, stake-weighted with replacement, from that GLOBAL pool (never "the model's holders": a closed model's
//!    registrant cannot fill it), with a seed from the first execution-lane seed anchor strictly later than the commit
//!    (`round_seed_anchor`, ADR-0130) — randomness that did not exist when the claim was fixed.
//! 3. **Attest** within `attest_window_daa`: each drawn bond re-executes the claim and signs a verdict (the RFC-0017 attestation object,
//!    kernel inner 75, naming the certification id instead of an exam id).
//! 4. **Decide**: `Certified` iff at least `k` positions attested `Reproduced` (no mismatch), fewer than `veto` positions attested a
//!    negative verdict (`Unavailable`, `NotReproducible`, or a mismatch on the claim), the claim is still `Final` with no pending
//!    accusation, and the G14 public-prosecution window (`prosecution_window_daa` after `Final`) has elapsed. Otherwise, at the
//!    deadline, `NotCertified` / `Vetoed`; no population is `NoPopulation`; no draw by `certify_by_daa` is `Expired`. One attempt per
//!    claim, counted per class; never retried.
//!
//! A **public-check path** (`PublicCheckCertified`) exists for classes with a pinned deterministic public check (G14-complete public
//! material): [`PALW_SECURITY_PUBLIC_CHECK_CLASSES_PINNED_V1`] is EMPTY in this binary, so the path certifies nothing until a class is
//! pinned with its measurement record.
//!
//! **PREMISE-BASED, not an unconditional proof (user decision 2026-10-15).** The pillar is the drawn verifiers' signed approval, under
//! the explicit premise that **the honest fraction of drawn verifier stake is at least the threshold** (`k/m`). No cryptographic
//! computation proof is a required condition. The f = 1/3, f = 0.6 and worst-case figures the RFC reports are the impact if the premise
//! fails — not conditions that change the rule. A certified claim later convicted by G14 public prosecution is voided (its record
//! leaves `Final`), and every reader of Gate C then reads it uncertified: its safe weight is taken back (`reverse_convicted_final`),
//! its participation and settlement depth vanish.
//!
//! **Residual assumption (stated, not removed):** the honest fraction of drawn verifier STAKE. A closed (p = 0) model is never
//! reproduced by an honest drawn verifier, so it certifies only if the adversary holds ≥ k positions and fewer than `veto` honest
//! negatives arrive — probability [`palw_certification_false_pass_f64`] per claim at adversarial drawn-stake fraction `f`. Signatures
//! are attestations, not recomputation proofs: a certificate is as strong as that assumption and the court behind it.
//!
//! Every value here is a NAMED, UNAPPROVED design value (RFC-0018 §11); validation refuses the fence armed.

use crate::Hash64;
use crate::palw_model_eligibility_v1::{EligDigestV1, PalwEligibilityAttestationRowV1, PalwEligibilityVerdictV1, PalwEligibilityVerifierV1};
use crate::palw_state_v2::{PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2};
use borsh::{BorshDeserialize, BorshSerialize};

/// **The premise, as the RPC states it** (user decision 2026-10-15): certification is premise-based, not an unconditional proof.
pub const PALW_SECURITY_CERTIFICATION_PREMISE_V1: &str = "SecurityCertified is premise-based: m-of-k positive attestations by verifiers \
     drawn from the global bonded pool after the claim was committed, plus the G14 public-prosecution window elapsed with no challenge \
     or conviction. It is sound only while the honest fraction of drawn verifier stake is at least the threshold; signatures are not \
     recomputation proofs. A certified claim later convicted is voided and its consensus credit revoked.";

/// **Kernel-route aux table 136** — the certification rows (65-byte keys: prefix ‖ 64-byte id). Grepped free (≤ 135 in use).
pub const PALW_SECURITY_CERTIFICATION_TABLE_V1: u8 = 136;
/// `'S' ‖ certification id` → [`PalwClaimCertificationRowV1`].
pub const CERTIFICATION_CLAIM_PREFIX_V1: u8 = 0x53;
/// `'C' ‖ class id` → [`PalwCertificationClassRowV1`] (attempt counters).
pub const CERTIFICATION_CLASS_PREFIX_V1: u8 = 0x43;
/// `'N' ‖ [0; 64]` → [`PalwCertificationNetworkRowV1`] (the per-epoch draw counter).
pub const CERTIFICATION_NETWORK_PREFIX_V1: u8 = 0x4E;
/// `'F' ‖ [0; 64]` → [`PalwCertifiedFrontierFloorV1`] (the frontier no conviction can reach any more).
pub const CERTIFICATION_FRONTIER_FLOOR_PREFIX_V1: u8 = 0x46;
/// `'V' ‖ native certification id` → [`PalwClaimCertificationRowV1`] of an owning39 MINING claim (RFC-0002 §II.14): the SAME
/// commit-then-draw, global-pool, m-of-k rule, policy, seed anchor, inner-75 attestation and verdict as a chain claim's — one
/// drawn-verifier mechanism, two subject kinds. Its `claim` is the native mining claim id, `class_id` the native model id, and
/// `final_daa` the complete Master's escrow-Final DAA. Certification credits nothing here: the native fold issues the receipt.
pub const CERTIFICATION_NATIVE_MINING_PREFIX_V39: u8 = 0x56;

pub const PALW_CERTIFICATION_ID_DOMAIN_V1: &[u8] = b"misaka-palw/security-certification/id/v1";
pub const PALW_CERTIFICATION_SEED_DOMAIN_V1: &[u8] = b"misaka-palw/security-certification/seed/v1";
pub const PALW_CERTIFICATION_PAYOUT_DOMAIN_V1: &[u8] = b"misaka-palw/security-certification/payout/v1";

// ---- named, unapproved design values (RFC-0018 §11) ------------------------------------------------------------------------------

/// `m` drawn positions and `k` positives (UNAPPROVED). Chosen so the premise-failure tail stays negligible at the claim volumes of the
/// 300M case: per-claim P(adversary ≥ k of m) at f = 1/3 is 1.19e-6 (47/32) versus 4.53e-2 (7/5) — over 23,076 claims ≈ 0.03 versus
/// ≈ 1,045 expected certified claims (RFC-0018 §7.3). The cost is `m` re-executions per certified claim (U-5).
pub const PALW_CERT_M_DESIGN_V1: u16 = 47;
pub const PALW_CERT_K_DESIGN_V1: u16 = 32;
/// `0`: no veto — the instructed rule is m-of-k positives alone. A veto (`v > 0`: `v` negative positions refuse) is a LEAD PROPOSAL
/// awaiting the user's decision (RFC-0018 U-6), evaluated in the RFC's table; it is not part of the adopted design.
pub const PALW_CERT_VETO_DESIGN_V1: u16 = 0;
pub const PALW_CERT_ANCHOR_DELAY_SPANS_DESIGN_V1: u64 = 1;
pub const PALW_CERT_ATTEST_WINDOW_DAYS_DESIGN_V1: u64 = 2;
pub const PALW_CERT_PROSECUTION_WINDOW_DAYS_DESIGN_V1: u64 = 7;
pub const PALW_CERT_CERTIFY_BY_DAYS_DESIGN_V1: u64 = 3;
pub const PALW_CERT_VERIFIER_MATURITY_DAYS_DESIGN_V1: u64 = 7;
pub const PALW_CERT_DRAWS_PER_EPOCH_DESIGN_V1: u32 = 4_096;
pub const PALW_CERT_EPOCH_DAYS_DESIGN_V1: u64 = 1;

/// Classes whose claims a deterministic public check certifies (G14-complete public material, with a pinned measurement record).
/// EMPTY: no class qualifies in this binary.
pub const PALW_SECURITY_PUBLIC_CHECK_CLASSES_PINNED_V1: &[[u8; 64]] = &[];

/// The certification policy (mirrored on the state params beside Gate C's fence).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwSecurityCertificationPolicyV1 {
    pub m: u16,
    pub k: u16,
    /// Negative positions at which a claim is vetoed; `0` disables the veto (the adopted m-of-k rule; a veto is proposal U-6).
    pub veto: u16,
    pub anchor_delay_spans: u64,
    pub attest_window_daa: u64,
    pub prosecution_window_daa: u64,
    /// From the commit: the latest DAA a draw may happen at; past it the claim is `Expired`.
    pub certify_by_daa: u64,
    pub verifier_maturity_daa: u64,
    pub verifier_min_collateral_sompi: u64,
    pub draws_per_epoch: u32,
    pub epoch_daa: u64,
}

impl PalwSecurityCertificationPolicyV1 {
    /// The unapproved design at `day_daa` DAA per day.
    pub const fn design_unapproved_v1(day_daa: u64) -> Self {
        Self {
            m: PALW_CERT_M_DESIGN_V1,
            k: PALW_CERT_K_DESIGN_V1,
            veto: PALW_CERT_VETO_DESIGN_V1,
            anchor_delay_spans: PALW_CERT_ANCHOR_DELAY_SPANS_DESIGN_V1,
            attest_window_daa: PALW_CERT_ATTEST_WINDOW_DAYS_DESIGN_V1 * day_daa,
            prosecution_window_daa: PALW_CERT_PROSECUTION_WINDOW_DAYS_DESIGN_V1 * day_daa,
            certify_by_daa: PALW_CERT_CERTIFY_BY_DAYS_DESIGN_V1 * day_daa,
            verifier_maturity_daa: PALW_CERT_VERIFIER_MATURITY_DAYS_DESIGN_V1 * day_daa,
            verifier_min_collateral_sompi: 1,
            draws_per_epoch: PALW_CERT_DRAWS_PER_EPOCH_DESIGN_V1,
            epoch_daa: PALW_CERT_EPOCH_DAYS_DESIGN_V1 * day_daa,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.k == 0 || self.k > self.m || self.m == 0 {
            return Err("certification needs 1 ≤ k ≤ m");
        }
        if self.k <= self.m / 2 {
            return Err("certification needs a strict majority of positions (k > m/2)");
        }
        if self.veto > self.m.saturating_sub(self.k).saturating_add(1) {
            return Err("a veto must be reachable while k positives remain possible");
        }
        if self.attest_window_daa == 0 || self.certify_by_daa == 0 || self.epoch_daa == 0 || self.draws_per_epoch == 0 {
            return Err("certification windows, epoch and capacity must be positive");
        }
        Ok(())
    }

    /// The longest a certification row lives after its commit (claim retirement must be at least this — RFC-0018 §7.6).
    pub fn lifetime_daa(&self) -> u64 {
        self.certify_by_daa.saturating_add(self.attest_window_daa).max(self.prosecution_window_daa)
    }
}

/// How a certification ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwCertificationOutcomeV1 {
    Certified,
    PublicCheckCertified,
    /// Fewer than `k` positives by the deadline (silence, unavailability, non-reproduction). Not a slash.
    NotCertified,
    /// `veto` or more negative positions.
    Vetoed,
    /// The pool could not fill `m` positions.
    NoPopulation,
    /// No draw happened by `certify_by_daa` (no later anchor, or the network draw capacity stayed full).
    Expired,
    /// The claim stopped being `Final` (voided, reversed) before a decision.
    Withdrawn,
}

impl PalwCertificationOutcomeV1 {
    pub fn certifies(self) -> bool {
        matches!(self, Self::Certified | Self::PublicCheckCertified)
    }
}

/// `'S' ‖ certification id` — one claim's certification.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClaimCertificationRowV1 {
    pub claim: EligDigestV1,
    pub class_id: EligDigestV1,
    /// The producer's kernel bond id and operator (excluded from the pool).
    pub producer_bond: EligDigestV1,
    pub producer_principal: EligDigestV1,
    pub attempt: bool,
    pub final_daa: u64,
    pub committed_daa: u64,
    pub committed_span: u64,
    pub anchor: Option<(u64, EligDigestV1)>,
    pub draws: Vec<EligDigestV1>,
    pub drawn_daa: u64,
    pub deadline_daa: u64,
    pub attestations: Vec<PalwEligibilityAttestationRowV1>,
    /// The issuance-funded reward leg held at `Final` (paid at certification, never named otherwise).
    pub held_reward: u64,
    pub payout_payload: EligDigestV1,
    pub outcome: Option<(PalwCertificationOutcomeV1, u64)>,
}

impl PalwClaimCertificationRowV1 {
    /// Positive positions: each draw position whose bond attested `Reproduced` with no mismatch (multiplicity kept).
    pub fn positive_positions(&self) -> u32 {
        self.draws
            .iter()
            .filter(|bond| {
                self.attestations.iter().any(|a| {
                    &a.verifier == *bond && matches!(a.verdict, PalwEligibilityVerdictV1::Reproduced) && a.transcript != [0u8; 64]
                })
            })
            .count() as u32
    }

    /// Negative positions: a drawn bond's `Unavailable` / `NotReproducible` verdict (multiplicity kept). A `Reproduced` attestation
    /// that named the claim as mismatching is recorded with an all-zero transcript by the fold and counts here.
    pub fn negative_positions(&self) -> u32 {
        self.draws
            .iter()
            .filter(|bond| {
                self.attestations.iter().any(|a| {
                    &a.verifier == *bond && (!matches!(a.verdict, PalwEligibilityVerdictV1::Reproduced) || a.transcript == [0u8; 64])
                })
            })
            .count() as u32
    }
}

/// `'C' ‖ class id`: counted attempts (one per Final claim of the class past the fence).
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCertificationClassRowV1 {
    pub committed: u64,
    pub certified: u64,
    pub failed: u64,
}

/// `'N' ‖ 0`: draws in the current epoch.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCertificationNetworkRowV1 {
    pub epoch: u64,
    pub draws: u32,
}

/// `'F' ‖ 0`: **the frontier floor** — the deepest frontier-eligible claim that has RETIRED (a retired claim can no longer be convicted:
/// `reverse_convicted_final` leaves a retired claim alone), or the frontier the chain stood at when Gate C came into force. The
/// recomputed frontier never falls below it, so a retirement cannot move the frontier and a conviction can move it back only as far
/// as the floor.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCertifiedFrontierFloorV1 {
    pub blue_score: u64,
    pub block: EligDigestV1,
}

// `[u8; 64]` has no `Default`: the floor's default is the zero frontier.
impl Default for PalwCertifiedFrontierFloorV1 {
    fn default() -> Self {
        Self { blue_score: 0, block: [0; 64] }
    }
}

fn hash64(domain: &[u8], parts: &[&[u8]]) -> EligDigestV1 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    out
}

/// The certification id of a claim — what the attestation's `exam` field names.
pub fn palw_certification_id_v1(claim_id: &Hash64) -> EligDigestV1 {
    hash64(PALW_CERTIFICATION_ID_DOMAIN_V1, &[claim_id.as_byte_slice()])
}

/// The certification id of an owning39 mining claim (domain-separated from every chain claim's).
pub fn palw_native_certification_id_v39(mining_claim: &Hash64) -> EligDigestV1 {
    hash64(PALW_CERTIFICATION_ID_DOMAIN_V1, &[b"native39-mining", mining_claim.as_byte_slice()])
}
pub fn certification_native_key_v39(id: &EligDigestV1) -> Vec<u8> {
    prefixed(CERTIFICATION_NATIVE_MINING_PREFIX_V39, id)
}
/// The decided row of an owning39 mining claim, if any.
pub fn palw_native_mining_certification_row_v39(state: &PalwChainStateV2, mining_claim: &Hash64) -> Option<PalwClaimCertificationRowV1> {
    let key = certification_native_key_v39(&palw_native_certification_id_v39(mining_claim));
    state.kernel_route_state_v1().and_then(|route| route.aux_row::<PalwClaimCertificationRowV1>(PALW_SECURITY_CERTIFICATION_TABLE_V1, &key))
}
/// **The native subject's certified read** (the `'V'` row's outcome certifies). Gate C reaches it through
/// `crate::palw_rfc0018_v1::palw_native_independently_work_certified_v39`, never directly.
pub fn palw_native_mining_security_certified_v39(state: &PalwChainStateV2, mining_claim: &Hash64) -> bool {
    palw_native_mining_certification_row_v39(state, mining_claim).and_then(|row| row.outcome).is_some_and(|(o, _)| o.certifies())
}

/// The draw seed: `H(domain ‖ certification id ‖ anchor span ‖ anchor execution key)`.
pub fn palw_certification_seed_v1(id: &EligDigestV1, anchor_span: u64, anchor_execution_key: &EligDigestV1) -> EligDigestV1 {
    hash64(PALW_CERTIFICATION_SEED_DOMAIN_V1, &[id, &anchor_span.to_le_bytes(), anchor_execution_key])
}

/// The pending-payout key the certified reward is named under.
pub fn palw_certification_payout_key_v1(claim_id: &Hash64) -> Hash64 {
    Hash64::from_bytes(hash64(PALW_CERTIFICATION_PAYOUT_DOMAIN_V1, &[claim_id.as_byte_slice()]))
}

fn prefixed(prefix: u8, id: &EligDigestV1) -> Vec<u8> {
    let mut key = Vec::with_capacity(65);
    key.push(prefix);
    key.extend_from_slice(id);
    key
}
pub fn certification_claim_key_v1(id: &EligDigestV1) -> Vec<u8> {
    prefixed(CERTIFICATION_CLAIM_PREFIX_V1, id)
}
pub fn certification_class_key_v1(class_id: &EligDigestV1) -> Vec<u8> {
    prefixed(CERTIFICATION_CLASS_PREFIX_V1, class_id)
}
pub fn certification_network_key_v1() -> Vec<u8> {
    prefixed(CERTIFICATION_NETWORK_PREFIX_V1, &[0u8; 64])
}
pub fn certification_frontier_floor_key_v1() -> Vec<u8> {
    prefixed(CERTIFICATION_FRONTIER_FLOOR_PREFIX_V1, &[0u8; 64])
}

/// The frontier floor row (`None`: none written — Gate C not yet in force on this chain).
pub fn palw_certified_frontier_floor_v1(state: &PalwChainStateV2) -> Option<PalwCertifiedFrontierFloorV1> {
    state
        .kernel_route_state_v1()
        .and_then(|route| route.aux_row::<PalwCertifiedFrontierFloorV1>(PALW_SECURITY_CERTIFICATION_TABLE_V1, &certification_frontier_floor_key_v1()))
}

/// **The safe frontier past Gate C (RFC-0018 §7.5a), recomputed from the state every block** — the deepest frontier-eligible `Final`
/// claim at or below `resolved_through` (a SecurityCertified attempt; a claim accepted below the fence by the released rule), never
/// below the floor. Because it is recomputed, a certified claim later convicted (voided) stops standing as the frontier at once and
/// the frontier falls back to the deepest remaining eligible claim — or the floor. A pure function of the state: every node, on every
/// arrival order, after a reorg (the delta's `Frontier` entry restores any pair) and after IBD (the floor row travels in the
/// carriage), reads one answer.
pub fn palw_gate_c_frontier_v1(
    state: &PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    resolved_through: u64,
) -> (u64, crate::BlockHash) {
    let floor = palw_certified_frontier_floor_v1(state)
        .map(|f| (f.blue_score, Hash64::from_bytes(f.block)))
        .unwrap_or((0, crate::BlockHash::default()));
    let deepest = state
        .claims_iter()
        .filter(|(id, claim)| {
            matches!(claim.phase, PalwClaimPhaseV2::Final { .. })
                && claim.accepted_blue_score <= resolved_through
                && crate::palw_rfc0018_v1::palw_gate_c_advances_frontier_v1(params, state, id, claim)
        })
        .map(|(_, claim)| (claim.accepted_blue_score, claim.accepted_block))
        .max_by_key(|(blue_score, _)| *blue_score);
    match deepest {
        Some(candidate) if candidate.0 > floor.0 => candidate,
        _ => floor,
    }
}

/// **The pruning rollback bound past Gate C (RFC-0018 §7.7)**: the lowest frontier a conviction could still move the frontier back to
/// — the shallowest frontier-eligible `Final` claim the state still holds (each of them is convictable until it retires, and each is a
/// possible fall-back target). Pruning must stay at or below it, so no point a rollback makes the frontier again has been pruned. `None`:
/// no such claim (the frontier can fall only to the floor, which never moves back).
pub fn palw_frontier_rollback_bound_v1(state: &PalwChainStateV2, params: &crate::palw_state_v2::PalwStateParamsV2) -> Option<u64> {
    state
        .claims_iter()
        .filter(|(id, claim)| {
            matches!(claim.phase, PalwClaimPhaseV2::Final { .. })
                && crate::palw_rfc0018_v1::palw_gate_c_advances_frontier_v1(params, state, id, claim)
        })
        .map(|(_, claim)| claim.accepted_blue_score)
        .min()
}

/// **The pool predicate**: Active, registered at least `verifier_maturity_daa` before the commit, at least the minimum collateral,
/// and not the producer's operator. The GLOBAL registry — no model-holding filter (a holder set is the registrant's to fill).
#[allow(clippy::too_many_arguments)]
pub fn palw_certification_pool_admits_v1(
    policy: &PalwSecurityCertificationPolicyV1,
    committed_daa: u64,
    producer_principal: &EligDigestV1,
    active: bool,
    registered_daa: u64,
    collateral: u64,
    operator: &EligDigestV1,
) -> bool {
    active
        && registered_daa.saturating_add(policy.verifier_maturity_daa) <= committed_daa
        && collateral >= policy.verifier_min_collateral_sompi.max(1)
        && operator != producer_principal
}

/// `m` stake-weighted positions with replacement over the pool (the RFC-0017 / R7-ECON draw). `None` on an empty pool.
pub fn palw_certification_draw_v1(
    seed: &EligDigestV1,
    id: &EligDigestV1,
    pool: &[PalwEligibilityVerifierV1],
    m: u16,
) -> Option<Vec<EligDigestV1>> {
    crate::palw_model_eligibility_v1::palw_eligibility_draw_v1(seed, id, pool, m)
}

/// Whether a class has a pinned deterministic public check (the additional path).
pub fn palw_certification_public_check_v1(class_id: &EligDigestV1) -> bool {
    PALW_SECURITY_PUBLIC_CHECK_CLASSES_PINNED_V1.contains(class_id)
}

/// The facts about the claim the verdict reads, at `now`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCertificationClaimFactsV1 {
    pub still_final: bool,
    pub accusation_pending: bool,
}

/// **The verdict** at `now` (`None`: undecided — wait). Order: a claim no longer `Final` withdraws; a draw that never happened by
/// `certify_by_daa` expires; the veto; `k` positives with the prosecution window elapsed and nothing pending certify; the deadline
/// fails the rest. A public-check class certifies once the prosecution window has elapsed with nothing pending.
pub fn palw_certification_verdict_v1(
    policy: &PalwSecurityCertificationPolicyV1,
    row: &PalwClaimCertificationRowV1,
    facts: PalwCertificationClaimFactsV1,
    now: u64,
) -> Option<PalwCertificationOutcomeV1> {
    use PalwCertificationOutcomeV1 as O;
    if row.outcome.is_some() {
        return None;
    }
    if !facts.still_final {
        return Some(O::Withdrawn);
    }
    let prosecution_elapsed = now >= row.final_daa.saturating_add(policy.prosecution_window_daa);
    if palw_certification_public_check_v1(&row.class_id) {
        return (prosecution_elapsed && !facts.accusation_pending).then_some(O::PublicCheckCertified);
    }
    if row.anchor.is_none() {
        return (now > row.committed_daa.saturating_add(policy.certify_by_daa)).then_some(O::Expired);
    }
    if policy.veto > 0 && row.negative_positions() >= policy.veto as u32 {
        return Some(O::Vetoed);
    }
    if row.positive_positions() >= policy.k as u32 {
        if prosecution_elapsed && !facts.accusation_pending {
            return Some(O::Certified);
        }
        // k positives are in hand: the decision waits for the prosecution window (and any open accusation) — no deadline failure.
        return None;
    }
    (now > row.deadline_daa).then_some(O::NotCertified)
}

/// **The claim-level read every Gate C site uses past the fence**: a certification row of `claim_id` whose outcome certifies.
pub fn palw_claim_security_certified_v1(state: &PalwChainStateV2, claim_id: &Hash64) -> bool {
    let key = certification_claim_key_v1(&palw_certification_id_v1(claim_id));
    state
        .kernel_route_state_v1()
        .and_then(|route| route.aux_row::<PalwClaimCertificationRowV1>(PALW_SECURITY_CERTIFICATION_TABLE_V1, &key))
        .and_then(|row| row.outcome)
        .is_some_and(|(outcome, _)| outcome.certifies())
}

/// **A claim's certification state as the RPC names it** (`not-governed`, `pending`, `certified`, `public-check-certified`,
/// `not-certified`, `vetoed`, `no-population`, `expired`, `withdrawn`). `not-governed` below the fence (every shipped network);
/// `pending` for a governed claim with no decided row (not yet `Final`, or committed and undecided).
pub fn palw_claim_certification_status_v1(
    state: &PalwChainStateV2,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    claim_id: &Hash64,
    claim: &crate::palw_state_v2::PalwClaimStateV2,
) -> &'static str {
    if !crate::palw_rfc0018_v1::palw_gate_c_governs_v1(params, claim) {
        return "not-governed";
    }
    let key = certification_claim_key_v1(&palw_certification_id_v1(claim_id));
    let outcome = state
        .kernel_route_state_v1()
        .and_then(|route| route.aux_row::<PalwClaimCertificationRowV1>(PALW_SECURITY_CERTIFICATION_TABLE_V1, &key))
        .and_then(|row| row.outcome);
    // RFC-0018 I-C3 / G14 R6: a voided claim holds no consensus credit whatever its row says — the one predicate
    // (`palw_independently_work_certified_v1`) refuses a `Voided` claim, so the RPC must not keep naming it certified.
    if matches!(claim.phase, crate::palw_state_v2::PalwClaimPhaseV2::Voided { .. }) {
        return match outcome.map(|(o, _)| o) {
            Some(o) if o.certifies() => "revoked",
            _ => "withdrawn",
        };
    }
    match outcome.map(|(o, _)| o) {
        None => "pending",
        Some(PalwCertificationOutcomeV1::Certified) => "certified",
        Some(PalwCertificationOutcomeV1::PublicCheckCertified) => "public-check-certified",
        Some(PalwCertificationOutcomeV1::NotCertified) => "not-certified",
        Some(PalwCertificationOutcomeV1::Vetoed) => "vetoed",
        Some(PalwCertificationOutcomeV1::NoPopulation) => "no-population",
        Some(PalwCertificationOutcomeV1::Expired) => "expired",
        Some(PalwCertificationOutcomeV1::Withdrawn) => "withdrawn",
    }
}

/// **The one consensus-credit rule for frontier AND settlement depth (RFC-0018 §7.5)**: past the fence a claim earns frontier and
/// settlement credit iff it is a SecurityCertified **attempt** in `Final`. A free-prompt claim's consensus credit is its certified
/// spends' weight only; it never moves the frontier or the settlement depth (the "only a Free-Prompt Final exists" case: no frontier,
/// depth 0). Below the fence (`governs == false`) the released rules: the frontier takes a `Final` of any source, settlement attempts.
pub fn palw_consensus_credit_anchor_v1(governs: bool, certified: bool, source: &PalwClaimSourceV2, phase: &PalwClaimPhaseV2) -> bool {
    let is_final = matches!(phase, PalwClaimPhaseV2::Final { .. });
    if !governs {
        return is_final;
    }
    is_final && certified && matches!(source, PalwClaimSourceV2::Attempt)
}

/// **The pruning-progress ceiling past the fence (RFC-0018 §7.7)** — independent of security credit, never above history an open
/// claim or court still needs: one below the oldest of (a) the oldest unresolved claim, (b) the oldest `Final` claim whose
/// certification is still undecided (its prosecution window and draws still read its history). `None`: nothing holds history, the
/// caller's own bound (the chain point) applies.
pub fn palw_pruning_progress_ceiling_v1(state: &PalwChainStateV2) -> Option<u64> {
    let oldest_unresolved = state.unresolved_first_blue_score_v1();
    let pending_certification = state.kernel_route_state_v1().and_then(|route| {
        route
            .aux
            .range((PALW_SECURITY_CERTIFICATION_TABLE_V1, Vec::new())..(PALW_SECURITY_CERTIFICATION_TABLE_V1 + 1, Vec::new()))
            .filter(|((_, key), _)| key.first() == Some(&CERTIFICATION_CLAIM_PREFIX_V1))
            .filter_map(|(_, bytes)| borsh::from_slice::<PalwClaimCertificationRowV1>(bytes).ok())
            .filter(|row| row.outcome.is_none())
            .filter_map(|row| state.claim(&Hash64::from_bytes(row.claim)).map(|claim| claim.accepted_blue_score))
            .min()
    });
    match (oldest_unresolved, pending_certification) {
        (None, None) => None,
        (a, b) => Some(a.into_iter().chain(b).min().unwrap_or(0).saturating_sub(1)),
    }
}

// ---- analysis (the residual assumption, reported; never a gate) ----------------------------------------------------------------

fn binomial_tail(m: u32, p: f64, at_least: u32) -> f64 {
    let mut total = 0.0;
    for x in at_least..=m {
        let mut c = 1.0;
        for i in 0..x {
            c = c * (m - i) as f64 / (i + 1) as f64;
        }
        total += c * p.powi(x as i32) * (1.0 - p).powi((m - x) as i32);
    }
    total
}

/// **A closed model's per-claim certification probability** at adversarial drawn-stake fraction `f`: the adversary must hold at least
/// `k` positions and the honest positions (who cannot reproduce it and attest negative) must stay below `veto`. Every honest drawn
/// verifier is assumed to attest (a silent honest verifier only helps the adversary: report `f` with the silent share added).
pub fn palw_certification_false_pass_f64(f: f64, m: u32, k: u32, veto: u32) -> f64 {
    // X = adversarial positions ~ Bin(m, f); honest = m − X; pass iff X ≥ k and (no veto, or m − X < veto).
    let lower = if veto == 0 { k } else { k.max(m.saturating_sub(veto.saturating_sub(1))) };
    binomial_tail(m, f, lower)
}

/// **An honest open model's per-claim refusal probability** under adversarial vetoes (griefing, a liveness cost — never a credit):
/// the adversary holds at least `veto` positions, or fewer than `k` honest positions remain.
pub fn palw_certification_grief_f64(f: f64, m: u32, k: u32, veto: u32) -> f64 {
    let veto_or_short = |x: u32| (veto > 0 && x >= veto) || m - x < k;
    let mut total = 0.0;
    for x in 0..=m {
        if veto_or_short(x) {
            let mut c = 1.0;
            for i in 0..x {
                c = c * (m - i) as f64 / (i + 1) as f64;
            }
            total += c * f.powi(x as i32) * (1.0 - f).powi((m - x) as i32);
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc18_cert_design_policy_validates_and_requires_a_majority() {
        let p = PalwSecurityCertificationPolicyV1::design_unapproved_v1(720);
        p.validate().unwrap();
        let minority = PalwSecurityCertificationPolicyV1 { k: 3, ..p };
        assert!(minority.validate().is_err(), "a Panel-style minority/majority of a set the registrant fills is not the rule");
        assert!(p.lifetime_daa() >= p.prosecution_window_daa);
    }

    #[test]
    fn rfc18_cert_closed_model_pass_probability_is_small_and_reported() {
        let (m, k, v) = (PALW_CERT_M_DESIGN_V1 as u32, PALW_CERT_K_DESIGN_V1 as u32, PALW_CERT_VETO_DESIGN_V1 as u32);
        let at = |f: f64| palw_certification_false_pass_f64(f, m, k, v);
        assert!(at(0.03) < 1e-6, "3 % drawn stake: {}", at(0.03));
        assert!(at(1.0 / 3.0) < at(0.6), "monotone in f");
        assert!((at(1.0) - 1.0).abs() < 1e-9, "an all-adversary pool certifies anything: the residual assumption, stated");
        assert!(palw_certification_grief_f64(0.0, m, k, v) < 1e-12, "no adversary, no refusal");
    }
}
