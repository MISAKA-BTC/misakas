//! **RFC-0018 — DAG parallel multi-model consensus** (`docs/rfc/0018-dag-parallel-multi-model-consensus.md`). Five DORMANT fences:
//!
//! | fence | what it changes past its height | keyed on |
//! | --- | --- | --- |
//! | `palw_dag_sibling_attribution_v1` | merged attempts of one accepting block are admitted in a canonical, colour- and merge-order-independent order (round robin over model classes, each class by `H(domain ‖ execution key)`), so which concurrent sibling wins a contended per-block cap — and which admitted attempt is the ADR-0170 seed anchor — is a function of the mergeset SET | the accepting block's DAA |
//! | `palw_panel_anchor_permissionless_v1` | lane A's operator-only anchor is replaced: any ADMITTED attempt of a registered, matured bond (blue or red) contributes, one contribution per bond, and the seed is the aggregate of the contributions at or past the slot | the anchor block's DAA (as lane A) |
//! | `palw_rule_e_participation_v2` | rule E's participation is the capital (read in the common past `F`) of the distinct bonds with a non-voided, Gate-C-certified attempt in a side's exclusive past; the even split is a third of the common capital | the incumbent's DAA (as rule E) |
//! | `palw_independently_work_certified_v1` (Gate C) | an uncertified claim contributes nothing to participation, provisional weight, final weight, safe-frontier advance, settled-anchor credit or coinbase | the claim's acceptance DAA |
//! | `palw_dag_width_schedule_v1` | per-block k, parents, mergeset and merge depth; block time fixed; validation refuses arming | the selected parent's DAA |
//!
//! **Dormant**: every fence is `None` on every preset and in no flag-day list, hashed Some-only in both fingerprints, collapsed from
//! `Some(never())`, and refused when armed without a pinned arm-readiness record ([`Params::validate_palw_rfc0018_v1`], [`PALW_RFC0018_ARMING_RECORDS_V1`], empty). The fold, the processor and the RPC read the four
//! state-level fences through ONE mirror on the V2 state params ([`PalwRfc0018MirrorV1`], written by
//! [`Params::sync_palw_rfc0018_v1`] and by nothing else); `None` there is byte-for-byte the released behaviour (A-2: below each fence
//! every reader answers as before, and a claim accepted below Gate C's fence keeps its old rule forever).
//!
//! **Gate C — ADOPTED (ユーザー方針 2026-10-15「この攻撃を塞ぐ方針で進める。DNS-BFTはRFCにある通り廃止したので使用できない。」).**
//! [`palw_independently_work_certified_v1`] is the ONE predicate every site that feeds participation, provisional weight, final weight,
//! the safe frontier, settlement depth, settled anchors and the issuance-funded reward reads. Below its fence it is `true` (legacy).
//! Past it a claim counts only once `SecurityCertified` ([`crate::palw_security_certification_v1`]): `Final` (procedure complete,
//! user-funded settlement) and `SecurityCertified` (consensus credit) are separate states. No DNS-BFT gate or DNS finality anchor is
//! relied on anywhere in this RFC (DNS-BFT is abolished by RFC-0012). Activation stays a separate user decision.
//!
//! Nothing here is built or tested by this lane (no cargo build/check/test); the test sources are for the validation phase.

use crate::Hash64;
use crate::KType;
use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::{
    PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwClaimStateV2, PalwStateParamsV2,
};
use blake2b_simd::Params as Blake2bParams;
use std::collections::{BTreeMap, BTreeSet};

// -------------------------------------------------------------------------------------------------
// Domains
// -------------------------------------------------------------------------------------------------

/// The canonical sibling order's key domain.
pub const PALW_RFC0018_SIBLING_ORDER_DOMAIN_V1: &[u8] = b"misaka-palw/rfc0018/sibling-order/v1";
/// The class rank of the canonical merged-attempt order (long-run fairness, 2026-10-18; dormant with the sibling fence).
pub const PALW_RFC0018_SIBLING_CLASS_ORDER_DOMAIN_V1: &[u8] = b"misaka-palw/rfc0018/sibling-class-order/v1";
/// The permissionless anchor's aggregate-seed domain.
pub const PALW_RFC0018_ANCHOR_AGGREGATE_DOMAIN_V1: &[u8] = b"misaka-palw/rfc0018/anchor-aggregate/v1";
/// The DAG width schedule's id domain.
pub const PALW_RFC0018_DAG_WIDTH_DOMAIN_V1: &[u8] = b"misaka-palw/rfc0018/dag-width/v1";

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    Blake2bParams::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// -------------------------------------------------------------------------------------------------
// The mirror (fold / processor / RPC read this, never `Params`)
// -------------------------------------------------------------------------------------------------

/// **Gate C's rule past its fence** — ADOPTED (ユーザー方針 2026-10-15): `SecurityCertifiedV1`, the per-claim independent positive
/// verification of [`crate::palw_security_certification_v1`]. `FailClosedV1` is a FIXTURE (nothing certifies) for the hook tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PalwGateCRuleV1 {
    #[default]
    SecurityCertifiedV1,
    FailClosedV1,
}

/// **The four state-level fences' heights**, as one mirror on [`PalwStateParamsV2`] (`#[borsh(skip)]`), with Gate C's rule and the
/// certification policy (unapproved design values).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwRfc0018MirrorV1 {
    pub sibling_attribution_from_daa: Option<u64>,
    pub panel_anchor_permissionless_from_daa: Option<u64>,
    pub rule_e_participation_v2_from_daa: Option<u64>,
    pub gate_c_from_daa: Option<u64>,
    pub gate_c_rule: PalwGateCRuleV1,
    pub certification: crate::palw_security_certification_v1::PalwSecurityCertificationPolicyV1,
    /// **The free-prompt receipt lane's tail** past certification (`receipt_maturity + receipt_use_window` of the network's FP bundle):
    /// a governed FP claim must still exist that long after its certification can be decided (RFC-0018 18-A; user decision 10-17).
    pub fp_receipt_tail_daa: u64,
    /// lane GC-DUTY: `palw_gate_c_attestor_liability_v1` — a positive `'S'` / `'K'` attestation locks and a conviction slashes it.
    pub attestor_liability_from_daa: Option<u64>,
}

impl PalwRfc0018MirrorV1 {
    fn at(from: Option<u64>, daa: u64) -> bool {
        from.is_some_and(|from| daa >= from)
    }
    pub fn sibling_attribution_at(&self, accepting_daa: u64) -> bool {
        Self::at(self.sibling_attribution_from_daa, accepting_daa)
    }
    pub fn panel_anchor_permissionless_at(&self, anchor_daa: u64) -> bool {
        Self::at(self.panel_anchor_permissionless_from_daa, anchor_daa)
    }
    pub fn rule_e_participation_v2_at(&self, incumbent_daa: u64) -> bool {
        Self::at(self.rule_e_participation_v2_from_daa, incumbent_daa)
    }
    /// Gate C governs a claim accepted at `claim_accepted_daa` (claims accepted below keep their old rule forever: A-2).
    pub fn gate_c_governs(&self, claim_accepted_daa: u64) -> bool {
        Self::at(self.gate_c_from_daa, claim_accepted_daa)
    }
    /// Gate C is in force at a block (the certification sweep and the pruning rule read this).
    pub fn gate_c_at(&self, daa: u64) -> bool {
        Self::at(self.gate_c_from_daa, daa)
    }
    /// lane GC-DUTY: a positive `'S'` / `'K'` certification attestation at `daa` carries an attestor liability lock.
    pub fn attestor_liability_at(&self, daa: u64) -> bool {
        Self::at(self.attestor_liability_from_daa, daa)
    }
    pub fn is_empty(&self) -> bool {
        self.sibling_attribution_from_daa.is_none()
            && self.panel_anchor_permissionless_from_daa.is_none()
            && self.rule_e_participation_v2_from_daa.is_none()
            && self.gate_c_from_daa.is_none()
    }
}

/// The mirror's readers on the state params (`None`: every shipped preset).
pub fn palw_rfc0018_mirror_v1(params: &PalwStateParamsV2) -> Option<&PalwRfc0018MirrorV1> {
    params.rfc0018_v1()
}

/// **Gate C's retirement floor** (user decision 2026-10-17): a governed claim does not retire before its certification can be decided
/// and, for a free-prompt claim, its receipt lane run: `final + certification lifetime (certify_by + attest window, at least the
/// prosecution window) + the FP receipt tail`. `retire_at` unchanged for a claim Gate C does not govern (every shipped network), and
/// for a claim that is not `Final` (a void retires as before: it earns nothing).
pub fn palw_gate_c_retirement_floor_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2, retire_at: u64) -> u64 {
    let Some(mirror) = palw_rfc0018_mirror_v1(params).filter(|m| m.gate_c_governs(claim.accepted_daa)) else { return retire_at };
    let PalwClaimPhaseV2::Final { final_daa } = claim.phase else { return retire_at };
    let tail = match claim.source {
        PalwClaimSourceV2::FreePrompt { .. } => mirror.fp_receipt_tail_daa,
        PalwClaimSourceV2::Attempt => 0,
    };
    retire_at.max(final_daa.saturating_add(mirror.certification.lifetime_daa()).saturating_add(tail))
}

/// **Gate C's trace-retention extension** (user decision 2026-10-17): past Gate C's fence a claim accepted at `daa` promises its trace
/// for the released minimum PLUS the certification lifetime and the FP receipt tail, so its material is still fetchable when its
/// certification is decided at the latest and through its receipt lane — the same span its retirement floor
/// ([`palw_gate_c_retirement_floor_v1`]) keeps the claim. The released minimum already reaches the claim's latest Final plus its court
/// window, so `acceptance + minimum + extension ≥ final + lifetime + tail`. One value for both lanes (no second canonical value);
/// `0` below the fence and on every shipped network.
pub fn palw_gate_c_retention_extension_daa_v1(params: &PalwStateParamsV2, daa: u64) -> u64 {
    palw_rfc0018_mirror_v1(params)
        .filter(|m| m.gate_c_governs(daa))
        .map_or(0, |m| m.certification.lifetime_daa().saturating_add(m.fp_receipt_tail_daa))
}

/// Whether Gate C governs `claim` (its acceptance DAA past the fence).
pub fn palw_gate_c_governs_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> bool {
    palw_rfc0018_mirror_v1(params).is_some_and(|mirror| mirror.gate_c_governs(claim.accepted_daa))
}

// -------------------------------------------------------------------------------------------------
// Gate C — SecurityCertified (RFC-0018 §7, adopted 2026-10-15)
// -------------------------------------------------------------------------------------------------

/// What Gate C reads besides the claim: the state params (the mirror) and the chain state holding the certification rows.
#[derive(Clone, Copy)]
pub struct PalwGateCViewV1<'a> {
    pub params: &'a PalwStateParamsV2,
    pub chain: &'a PalwChainStateV2,
}

impl<'a> PalwGateCViewV1<'a> {
    pub fn of(params: &'a PalwStateParamsV2, chain: &'a PalwChainStateV2) -> Self {
        Self { params, chain }
    }
}

/// **`palw_independently_work_certified_v1(claim, state) -> bool` — Gate C, the one predicate (`SecurityCertified`).**
///
/// * Below `palw_independently_work_certified_v1` at the claim's ACCEPTANCE DAA (every claim on every shipped network, and every claim
///   accepted before the fence on any network): `true` — the released behaviour, byte for byte.
/// * Past it: a `Voided` claim never; otherwise the claim's certification row in the state has a certifying outcome
///   ([`crate::palw_security_certification_v1::palw_claim_security_certified_v1`]) — commit at `Final`, global-pool draw on later
///   randomness, m-of-k positive attestations, no veto, nothing pending, the prosecution window elapsed. A Panel licence, a Panel
///   majority, the model's own holders or `Final` alone NEVER certify. (`FailClosedV1`: never — a fixture.)
///
/// The answer is a fact frozen in the state (the row's decided outcome), so the fold's addition at certification, every re-derivation
/// (consistency, retirement, reversal) and rule E read one answer.
pub fn palw_independently_work_certified_v1(claim_id: &Hash64, claim: &PalwClaimStateV2, state: &PalwGateCViewV1<'_>) -> bool {
    palw_work_certified_from_row_v1(state.params, claim, || {
        crate::palw_security_certification_v1::palw_claim_security_certified_v1(state.chain, claim_id)
    })
}

/// The same predicate over a borrowed historical row. Keep the row read lazy:
/// pre-fence callers must neither decode nor inspect new certification bytes.
pub(crate) fn palw_work_certified_from_row_v1(
    params: &PalwStateParamsV2,
    claim: &PalwClaimStateV2,
    row_certifies: impl FnOnce() -> bool,
) -> bool {
    let Some(mirror) = palw_rfc0018_mirror_v1(params) else { return true };
    if !mirror.gate_c_governs(claim.accepted_daa) {
        return true;
    }
    if matches!(claim.phase, PalwClaimPhaseV2::Voided { .. }) {
        return false;
    }
    // (Round 5, user decision 2026-10-15: model-level review is abolished, so U-2's Probation bar has no subject and is removed —
    // this per-claim drawn certification is the one safety gate, the same for every model tier.)
    match mirror.gate_c_rule {
        PalwGateCRuleV1::SecurityCertifiedV1 => row_certifies(),
        PalwGateCRuleV1::FailClosedV1 => false,
    }
}

/// **Gate C for an owning39 MINING claim** (RFC-0002 §II.14) — the same rule as [`palw_independently_work_certified_v1`] for the
/// second subject kind, inside this one module:
///
/// * a NEW lane has no released behaviour, so where no mirror governs the claim's acceptance DAA the answer is `false` (never the
///   legacy `true`): no private-job escrow Final, source identity or local flag earns consensus credit;
/// * otherwise the `'V'` certification row's decided outcome (m-of-k drawn positive attestations, prosecution window elapsed).
///
/// User decision 2026-10-15 (round 5): model-level review is abolished for every model; drawn-verifier approval exists ONLY per
/// claim, here. A model's tier (genesis/annual-fork 1.0, registered 0.3) is `palw_model_tier_v1`'s, applied once in the
/// allocation weight, never by this predicate.
///
/// Premise-based (the honest fraction of drawn verifier stake ≥ k/m), never a computation proof.
pub fn palw_native_independently_work_certified_v39(
    params: &PalwStateParamsV2,
    chain: &PalwChainStateV2,
    mining_claim: &Hash64,
    accepted_daa: u64,
) -> bool {
    let Some(mirror) = palw_rfc0018_mirror_v1(params) else { return false };
    if !mirror.gate_c_governs(accepted_daa) {
        return false;
    }
    match mirror.gate_c_rule {
        PalwGateCRuleV1::SecurityCertifiedV1 => {
            crate::palw_security_certification_v1::palw_native_mining_security_certified_v39(chain, mining_claim)
        }
        PalwGateCRuleV1::FailClosedV1 => false,
    }
}

/// **Gate C's provisional-weight site** (`bounded_immature` / `live_total`): past the fence 0 — a claim is never `SecurityCertified`
/// while live (certification commits at `Final`), and `bounded_immature` is maintained incrementally, so it must read ONE number over a
/// claim's life. Consensus credit arrives at certification, through the final-weight path. Below the fence: `weight`, unchanged.
pub fn palw_gate_c_provisional_weight_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2, weight: u128) -> u128 {
    if palw_gate_c_governs_v1(params, claim) { 0 } else { weight }
}

/// `claim.immature_contribution` through [`palw_gate_c_provisional_weight_v1`] — the one spelling every old-rule `bounded_immature`
/// site (reserve, release, void, the two re-derivations, rule E's live record) reads.
pub fn palw_claim_provisional_weight_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> u128 {
    palw_gate_c_provisional_weight_v1(params, claim, claim.immature_contribution)
}

/// `weight` where Gate C certifies the claim, else 0 — the form every final-weight site takes.
pub fn palw_gate_c_weight_v1(
    params: &PalwStateParamsV2,
    chain: &PalwChainStateV2,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
    weight: u128,
) -> u128 {
    if palw_independently_work_certified_v1(claim_id, claim, &PalwGateCViewV1::of(params, chain)) { weight } else { 0 }
}

/// Whether `claim` may tick the settled-anchor clock at a licence / Final site. Past the fence never there — a governed claim's
/// settled-anchor credit is ticked once, at its certification (`palw_security_certification_fold_v1`).
pub fn palw_gate_c_settles_anchor_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> bool {
    !palw_gate_c_governs_v1(params, claim)
}

/// Whether a `Final` claim may stand as the safe frontier — Gate C's frontier site, with the ONE consensus-credit rule shared with
/// settlement depth ([`crate::palw_security_certification_v1::palw_consensus_credit_anchor_v1`]).
pub fn palw_gate_c_advances_frontier_v1(
    params: &PalwStateParamsV2,
    chain: &PalwChainStateV2,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
) -> bool {
    let governs = palw_gate_c_governs_v1(params, claim);
    let certified = governs && palw_independently_work_certified_v1(claim_id, claim, &PalwGateCViewV1::of(params, chain));
    // RFC-0023 stage 1: a claim Final only after its unresolved bound never stands as the frontier (the `now` is irrelevant for a
    // Final claim; the predicate reads its own `final_daa`).
    if crate::palw_unresolved_bound_v1::palw_claim_lapsed_bound_v1(params, claim, 0) {
        return false;
    }
    crate::palw_security_certification_v1::palw_consensus_credit_anchor_v1(governs, certified, &claim.source, &claim.phase)
}

/// **Gate C's receipt-lane refusal** (RFC-0018 18-A): a governed free-prompt claim that is `Final` but not `SecurityCertified` licenses no
/// receipt block. `status` is the RPC's certification state (`pending`, `no-population`, `not-certified`, `vetoed`, `expired`,
/// `withdrawn`, `revoked`) or `accusation-pending` (certified, but an accusation is open on it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("free-prompt claim {claim} is Final but not SecurityCertified ({status}): Gate C licenses no receipt block before certification")]
pub struct PalwFpReceiptUncertifiedV1 {
    pub claim: Hash64,
    pub status: &'static str,
}

/// **The receipt lane's clock under Gate C** (RFC-0018 18-A) — the ONE predicate the receipt pre-check (own block and merged
/// entitlement, so the coinbase worker share), the stateless-admission items, the spendable RPC (the producer's template source) and the
/// fold read for a `Final` free-prompt claim at `final_daa`:
///
/// * not governed (no mirror, or the claim accepted below Gate C's fence — every claim on every shipped network): `Ok(final_daa)`, the
///   released draw slot `final + receipt_maturity`, byte for byte;
/// * governed and `SecurityCertified` ([`palw_independently_work_certified_v1`]): `Ok(max(final_daa, certified_daa))` — the lane opens at
///   certification, so its draw beacon lies after the decision and the use window is not consumed while certification is pending (an
///   honest claim is paid in full once certified);
/// * governed, certified, but an accusation pending on it ([`PalwChainStateV2::palw_accusation_pending_v1`]): `Err`
///   (`accusation-pending`) until the accusation closes — the same one predicate, so admission, fold, RPC and coinbase agree;
/// * governed and not certified (pending, no population, rejected, voided — a late conviction voids the claim, so its later quanta
///   are refused here too): `Err` — no receipt block, no coinbase worker share. Weight 0 alone was not enough: the block right and the
///   worker share are paid outside the weight.
///
/// The answer is read from the chain state at the candidate's point, so siblings and reorgs read the certification their own chain holds.
pub fn palw_fp_receipt_clock_v1(
    params: &PalwStateParamsV2,
    chain: &PalwChainStateV2,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
    final_daa: u64,
) -> Result<u64, PalwFpReceiptUncertifiedV1> {
    if !palw_gate_c_governs_v1(params, claim) {
        return Ok(final_daa);
    }
    if !palw_independently_work_certified_v1(claim_id, claim, &PalwGateCViewV1::of(params, chain)) {
        let status = crate::palw_security_certification_v1::palw_claim_certification_status_v1(chain, params, claim_id, claim);
        return Err(PalwFpReceiptUncertifiedV1 { claim: *claim_id, status });
    }
    // G14 gatec-capacity: Gate C is `Final ∧ SecurityCertified ∧ ¬accusation-pending`. Certification decides only with nothing
    // pending, but an accusation opened AFTER it (a late court / DA session / dispute) must suspend the lane until its verdict: a
    // conviction voids the claim (refused above as `revoked`), an acquittal re-opens the lane. Governed claims only — below Gate C's
    // fence nothing here is read.
    if chain.palw_accusation_pending_v1(claim_id) {
        return Err(PalwFpReceiptUncertifiedV1 { claim: *claim_id, status: "accusation-pending" });
    }
    let certified_daa = crate::palw_security_certification_v1::palw_claim_certified_daa_v1(chain, claim_id).unwrap_or(final_daa);
    Ok(final_daa.max(certified_daa))
}

/// **G14 r5 (user decision 2026-10-22): the receipt use-window clock PAUSES during an accusation suspension.** Same predicate as
/// [`palw_fp_receipt_clock_v1`] (suspended while an accusation is pending), then the window's start moves forward by every DAA the
/// claim spent suspended before a dismissal ([`crate::palw_court_clock_v2::palw_fp_receipt_window_start_v2`]), so the remaining
/// window resumes on dismissal instead of having lapsed. `pause` is the claim's pause account; a conviction voids the claim (refused
/// above as `revoked`), so only dismissals credit time back. Below Gate C's fence `pause` is never consulted.
pub fn palw_fp_receipt_clock_paused_v2(
    params: &PalwStateParamsV2,
    chain: &PalwChainStateV2,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
    final_daa: u64,
    pause: &crate::palw_court_clock_v2::PalwReceiptPauseV2,
) -> Result<u64, PalwFpReceiptUncertifiedV1> {
    let base = palw_fp_receipt_clock_v1(params, chain, claim_id, claim, final_daa)?;
    if !palw_gate_c_governs_v1(params, claim) {
        return Ok(base);
    }
    crate::palw_court_clock_v2::palw_fp_receipt_window_start_v2(base, pause)
        .ok_or(PalwFpReceiptUncertifiedV1 { claim: *claim_id, status: "accusation-pending" })
}

/// **G14 r6: THE receipt clock every caller reads** — [`palw_fp_receipt_clock_paused_v2`] with the claim's ROOTED pause account
/// ([`crate::palw_receipt_pause_v2`], kernel route aux 145). Below Gate C's fence (claim not governed) this is
/// [`palw_fp_receipt_clock_v1`] byte for byte: the row is never read. A malformed carried row is a typed refusal
/// (`pause-row-malformed`), never "no pause".
pub fn palw_fp_receipt_clock_v2(
    params: &PalwStateParamsV2,
    chain: &PalwChainStateV2,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
    final_daa: u64,
) -> Result<u64, PalwFpReceiptUncertifiedV1> {
    if !palw_gate_c_governs_v1(params, claim) {
        return palw_fp_receipt_clock_v1(params, chain, claim_id, claim, final_daa);
    }
    let pause = crate::palw_receipt_pause_v2::palw_receipt_pause_account_v2(chain, claim_id)
        .map_err(|_| PalwFpReceiptUncertifiedV1 { claim: *claim_id, status: "pause-row-malformed" })?;
    palw_fp_receipt_clock_paused_v2(params, chain, claim_id, claim, final_daa, &pause)
}

// -------------------------------------------------------------------------------------------------
// Rule E participation v2 (RFC-0018 §6)
// -------------------------------------------------------------------------------------------------

/// **One side's v2 participation**: the capital, read in the common past, of the DISTINCT common bonds with at least one counted
/// attempt in the side's exclusive past. Counted = an attempt claim that is not `Voided` and that Gate C certifies (the caller passes
/// records already mapped through Gate C: an uncertified record arrives `Voided`).
///
/// **Split-neutral**: linear in capital with no per-bond cap or floor, so a capital split over `n` bonds is credited at most what the
/// unsplit bond is (equal when every piece attempts) and a bond counts once however many claims it has. **Bounded by the bond formula
/// v2's capital**: the credit is the collateral `C` the formula's `u·C` and the budget engine's `F_max(C, W)` read — never a reservation,
/// never a post-fork deposit (the common past's value).
#[derive(Clone, Debug, Default)]
pub struct PalwRuleEParticipationV2Acc {
    bonds: BTreeMap<PalwBondKeyV2, u64>,
}

impl PalwRuleEParticipationV2Acc {
    /// `common_capital`: the bond's collateral in the common past (`None`: not a common bond — it never counts).
    pub fn push(&mut self, bond: PalwBondKeyV2, attempt: bool, counted: bool, common_capital: Option<u64>) {
        if let (true, true, Some(capital)) = (attempt, counted, common_capital) {
            self.bonds.entry(bond).or_insert(capital);
        }
    }
    pub fn capital(&self) -> u128 {
        self.bonds.values().map(|c| *c as u128).sum()
    }
    pub fn bonds(&self) -> usize {
        self.bonds.len()
    }
}

/// The even split's v2 threshold: a third of the common capital, rounded up, at least 1 (a heartbeat-only period — zero on both
/// sides — never reaches it).
pub fn palw_rule_e_even_split_min_capital_v2(common_capital: u128) -> u128 {
    common_capital.div_ceil(3).max(1)
}

/// The common capital of a registry (the relay path: `F`'s bonds and their collateral there).
pub fn palw_rule_e_common_capital_of_v2(fork: &PalwChainStateV2) -> u128 {
    fork.bonds_iter().map(|(_, bond)| bond.collateral as u128).sum()
}

/// The common capital where no fork block is known (IBD: the bonds both states register, each at the SMALLER of its two collaterals —
/// symmetric and never more than either side holds).
pub fn palw_rule_e_common_capital_by_claim_set_v2(a: &PalwChainStateV2, b: &PalwChainStateV2) -> BTreeMap<PalwBondKeyV2, u64> {
    a.bonds_iter().filter_map(|(k, ra)| b.bond(k).map(|rb| (*k, ra.collateral.min(rb.collateral)))).collect()
}

// -------------------------------------------------------------------------------------------------
// Sibling attribution: the canonical merged-attempt order (RFC-0018 §4)
// -------------------------------------------------------------------------------------------------

/// One merged work as the canonical order reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSiblingKeyV1 {
    /// `Some(class)` for an attempt; `None` for any other work (keeps its slot).
    pub class_id: Option<Hash64>,
    pub execution_key: Hash64,
}

/// `H(domain ‖ execution key)` — fixed by the sibling's own inference (the accepting miner cannot move it; the sibling's miner moves it
/// only by another winning inference).
pub fn palw_sibling_priority_v1(execution_key: &Hash64) -> Hash64 {
    let mut h = keyed(PALW_RFC0018_SIBLING_ORDER_DOMAIN_V1);
    h.update(execution_key.as_byte_slice());
    finish(h)
}

/// **The canonical order of one accepting block's merged works** — a permutation of `0..works.len()`.
///
/// Non-attempt works keep their positions (their order is the consensus order, as before). The attempt positions are refilled in a
/// round robin over model classes: classes ordered by `H(domain ‖ set seed ‖ class)` (the set seed hashes the mergeset's sorted
/// attempt priorities), each class's attempts by priority, one attempt per class per round. The result depends only on the SET of `(class, execution key)` pairs at the attempt positions — not on GHOSTDAG
/// colour, blue work or merge order — so concurrent siblings of different models share a contended per-block cap round by round, and
/// the same mergeset yields the same admitted set on every node and every arrival order. Ties (equal priority: the same execution key,
/// which the fold dedups anyway) fall back to the original index.
pub fn palw_sibling_canonical_order_v1(works: &[PalwSiblingKeyV1]) -> Vec<usize> {
    let attempt_slots: Vec<usize> = works.iter().enumerate().filter(|(_, w)| w.class_id.is_some()).map(|(i, _)| i).collect();
    let mut by_class: BTreeMap<Hash64, Vec<(Hash64, usize)>> = BTreeMap::new();
    for &i in &attempt_slots {
        let w = works[i];
        by_class.entry(w.class_id.expect("attempt slot")).or_default().push((palw_sibling_priority_v1(&w.execution_key), i));
    }
    let by_class_ids: Vec<Hash64> = by_class.keys().copied().collect();
    let classes: Vec<Vec<(Hash64, usize)>> = by_class
        .into_values()
        .map(|mut members| {
            members.sort();
            members
        })
        .collect();
    // Long-run fairness (2026-10-18): the class order is `H(domain ‖ set seed ‖ class)`, never the class's best member. Ranked by its
    // first member, a class with more attempts in the mergeset has a smaller minimum and comes first more often, so with more classes
    // than a contended cap admits (the 16-carve emission budget) a flooding set of models starves a one-attempt model outright
    // (`rfc18_long_run_fairness_*`). The set seed is the hash of the mergeset's sorted attempt priorities: still a function of the
    // SET only, and no class's attempt count moves its own rank in expectation.
    let mut seed = keyed(PALW_RFC0018_SIBLING_CLASS_ORDER_DOMAIN_V1);
    let mut all: Vec<Hash64> = classes.iter().flat_map(|c| c.iter().map(|(p, _)| *p)).collect();
    all.sort();
    for p in &all {
        seed.update(p.as_byte_slice());
    }
    let seed = finish(seed);
    let rank = |class: &Hash64| {
        let mut h = keyed(PALW_RFC0018_SIBLING_CLASS_ORDER_DOMAIN_V1);
        h.update(seed.as_byte_slice());
        h.update(class.as_byte_slice());
        finish(h)
    };
    let mut classes: Vec<(Hash64, Vec<(Hash64, usize)>)> = by_class_ids.into_iter().zip(classes).map(|(c, m)| (rank(&c), m)).collect();
    classes.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1[0].cmp(&b.1[0])));
    let classes: Vec<Vec<(Hash64, usize)>> = classes.into_iter().map(|(_, m)| m).collect();
    let mut ordered = Vec::with_capacity(attempt_slots.len());
    let mut round = 0usize;
    while ordered.len() < attempt_slots.len() {
        for class in &classes {
            if let Some((_, i)) = class.get(round) {
                ordered.push(*i);
            }
        }
        round += 1;
    }
    let mut out: Vec<usize> = (0..works.len()).collect();
    for (slot, index) in attempt_slots.iter().zip(ordered) {
        out[*slot] = index;
    }
    out
}

// -------------------------------------------------------------------------------------------------
// The permissionless Panel anchor (RFC-0018 §5)
// -------------------------------------------------------------------------------------------------

/// One candidate contribution to a permissionless anchor: an attempt the anchor block is or merges (blue or red).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwAnchorContributionV1 {
    pub daa_score: u64,
    pub block: Hash64,
    pub bond: PalwBondKeyV2,
    /// The attempt's execution commitment (lane F1's seed unit).
    pub execution: Hash64,
    /// The fold admitted the attempt. Contributions are read from COMMITTED fold output
    /// ([`palw_permissionless_contributions_from_state_v1`]: the claims of the anchor block's selected-parent state), so this is
    /// `true` for every contribution the node builds; header-only operator anchoring had no such check (RFC-0018 F-6).
    pub admitted: bool,
    /// Kept for the pure tests' fixtures; the committed-output reading needs no maturity input (admission already ran on the bond).
    pub bond_matured: bool,
}

/// **The permissionless anchor's contributions, from committed fold output (RFC-0018 §5.2, exact)**: every ATTEMPT claim of `state`
/// (the anchor block's selected-parent state — fold output committed before the anchor block) accepted by a chain block below
/// `before_blue_score`, BLUE or RED alike (`accepted_block` is the carrying block). Phase-independent — a claim the fold admitted was
/// admitted, whatever happened to it later — so the processor (reading the selected parent's state) and the fold's step 4c (reading
/// its own state, which adds only claims accepted in the anchor block itself, excluded by the blue-score bound) read one set.
/// Recognition is therefore one chain block later than the attempt's acceptance.
pub fn palw_permissionless_contributions_from_state_v1(
    state: &PalwChainStateV2,
    before_blue_score: u64,
) -> Vec<PalwAnchorContributionV1> {
    state
        .claims_iter()
        .filter(|(_, claim)| matches!(claim.source, PalwClaimSourceV2::Attempt) && claim.accepted_blue_score < before_blue_score)
        .map(|(id, claim)| PalwAnchorContributionV1 {
            daa_score: claim.accepted_daa,
            block: claim.accepted_block,
            bond: claim.bond,
            execution: claim.work_id.unwrap_or(*id),
            admitted: true,
            bond_matured: true,
        })
        .collect()
}

/// **Whether these contributions anchor a claim of `producer` whose slot is `slot`**: at least the policy's minimum of OTHER bonds
/// contribute at or past the slot (the claim's own producer never seeds its own Panel). The one predicate the processor's walk, its
/// binding derivation and the fold's step 4c ask.
pub fn palw_permissionless_anchors_claim_v1(contributions: &[PalwAnchorContributionV1], slot: u64, producer: &PalwBondKeyV2) -> bool {
    palw_permissionless_anchor_seed_v1(contributions, slot, Some(producer), &PalwPermissionlessAnchorPolicyV1::design_unapproved_v1())
        .is_some()
}

/// The permissionless anchor's design values (UNAPPROVED; RFC-0018 §11).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPermissionlessAnchorPolicyV1 {
    /// Fewest distinct bonds whose contributions the aggregate must hold (≥ 2: one honest unpredictable contribution must be inside).
    pub min_distinct_bonds: u32,
}

/// Unapproved design value (RFC-0018 §11): at least one bond OTHER than the claim's producer (the producer is excluded per claim).
pub const PALW_RFC0018_ANCHOR_MIN_DISTINCT_BONDS_DESIGN_V1: u32 = 1;

impl PalwPermissionlessAnchorPolicyV1 {
    pub const fn design_unapproved_v1() -> Self {
        Self { min_distinct_bonds: PALW_RFC0018_ANCHOR_MIN_DISTINCT_BONDS_DESIGN_V1 }
    }
}

/// **The aggregate seed source of a permissionless anchor for a claim whose slot is `slot`** — `(draw DAA, aggregate)`, or `None` where
/// the anchor does not (yet) reach the slot.
///
/// Eligible: admitted, matured-bond contributions at or past the slot, excluding the claim's own producer bond. One contribution per
/// bond — its earliest `(DAA, block)` — so a bond's extra attempts neither add weight nor re-roll (its later wins are ignored). The
/// aggregate is `H(domain ‖ slot ‖ Σ sorted (bond, execution))` over those per-bond contributions; it needs at least
/// `min_distinct_bonds`. The draw DAA is the least eligible DAA (the draw's clock-keyed inputs resolve there, as lane A's do at its
/// seed attempt). Order-independent: blue and red alike, and the result is a function of the contribution SET.
///
/// **Grinding bound (RFC-0018 §5.3):** an adversary controlling `b` of the contributing bonds can choose, per bond, among withholding
/// and its own admitted attempts in the window; the honest contributions (unpredictable execution commitments) are inside the hash, so
/// its choice set is at most `Π_{its bonds}(1 + q_b)` where `q_b` is the bond's admitted attempts in the window — bounded by ADR-0176's
/// per-bond claim budget, each costing a winning inference. The panel-capture probability is multiplied by at most that factor.
pub fn palw_permissionless_anchor_seed_v1(
    contributions: &[PalwAnchorContributionV1],
    slot: u64,
    producer: Option<&PalwBondKeyV2>,
    policy: &PalwPermissionlessAnchorPolicyV1,
) -> Option<(u64, Hash64)> {
    let mut first: BTreeMap<PalwBondKeyV2, (u64, Hash64, Hash64)> = BTreeMap::new();
    for c in contributions.iter().filter(|c| c.admitted && c.bond_matured && c.daa_score >= slot && Some(&c.bond) != producer) {
        let entry = first.entry(c.bond).or_insert((c.daa_score, c.block, c.execution));
        if (c.daa_score, c.block) < (entry.0, entry.1) {
            *entry = (c.daa_score, c.block, c.execution);
        }
    }
    if (first.len() as u64) < policy.min_distinct_bonds.max(1) as u64 {
        return None;
    }
    let draw_daa = first.values().map(|(daa, _, _)| *daa).min()?;
    let mut h = keyed(PALW_RFC0018_ANCHOR_AGGREGATE_DOMAIN_V1);
    h.update(&slot.to_le_bytes());
    for (bond, (_, _, execution)) in &first {
        h.update(bond.0.transaction_id.as_bytes().as_slice());
        h.update(&bond.0.index.to_le_bytes());
        h.update(execution.as_byte_slice());
    }
    Some((draw_daa, finish(h)))
}

/// The highest slot a permissionless anchor reaches: the latest eligible contribution's DAA, capped at the anchor's own DAA (as lane A).
pub fn palw_permissionless_anchor_reach_v1(contributions: &[PalwAnchorContributionV1], anchor_daa: u64) -> Option<u64> {
    // Claim-independent: whether THIS claim is anchored (a non-producer contribution at or past its slot) is
    // `palw_permissionless_anchors_claim_v1`, asked per claim by the walk, the derivation and step 4c.
    let distinct: BTreeSet<PalwBondKeyV2> = contributions.iter().filter(|c| c.admitted && c.bond_matured).map(|c| c.bond).collect();
    if distinct.is_empty() {
        return None;
    }
    contributions.iter().filter(|c| c.admitted && c.bond_matured).map(|c| c.daa_score).max().map(|latest| latest.min(anchor_daa))
}

// -------------------------------------------------------------------------------------------------
// The DAG width schedule (RFC-0018 §8) — dormant per-block readers
// -------------------------------------------------------------------------------------------------

/// One dormant step, read at the selected parent's DAA at every coloring and validation site.
/// Block-time changes require a separate clock/window migration and remain refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwDagWidthScheduleV1 {
    pub ghostdag_k: KType,
    pub target_time_per_block_ms: u64,
    pub max_block_parents: u16,
    pub mergeset_size_limit: u64,
    pub merge_depth: u64,
}

impl PalwDagWidthScheduleV1 {
    /// The schedule's identity (hashed into both fingerprints beside the height).
    pub fn id(&self) -> Hash64 {
        let mut h = keyed(PALW_RFC0018_DAG_WIDTH_DOMAIN_V1);
        h.update(&self.ghostdag_k.to_le_bytes());
        h.update(&self.target_time_per_block_ms.to_le_bytes());
        h.update(&self.max_block_parents.to_le_bytes());
        h.update(&self.mergeset_size_limit.to_le_bytes());
        h.update(&self.merge_depth.to_le_bytes());
        finish(h)
    }

    /// Structural refusals (the values themselves are the user's).
    pub fn refusal(&self) -> Option<&'static str> {
        if self.ghostdag_k == 0 || self.max_block_parents < 2 || self.target_time_per_block_ms == 0 || self.merge_depth == 0 {
            return Some("palw_dag_width_schedule_v1: k ≥ 1, at least two parents, a positive block time and merge depth");
        }
        if self.mergeset_size_limit < self.ghostdag_k as u64 * 10 {
            return Some("palw_dag_width_schedule_v1: the mergeset bound must exceed 10·k (Kaspa's own relation)");
        }
        None
    }
}

/// **The fence**: the height and the schedule step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwDagWidthScheduleFenceV1 {
    pub activation: ForkActivation,
    pub schedule: PalwDagWidthScheduleV1,
}

impl PalwDagWidthScheduleFenceV1 {
    /// Structural fixture values for the fork-id probe and the A-2 harness. NOT a proposal: validation refuses every armed value.
    pub fn unapproved_probe_at(activation: ForkActivation) -> Self {
        Self {
            activation,
            schedule: PalwDagWidthScheduleV1 {
                ghostdag_k: 3,
                target_time_per_block_ms: 120_000,
                max_block_parents: 16,
                mergeset_size_limit: 360,
                merge_depth: 30,
            },
        }
    }
}

/// **The width analysis (RFC-0018 §8.2)**: the expected fraction of honest blocks a selected chain colours RED at GHOSTDAG `k`, for
/// blocks whose effective publication delay is `delay_ms` (propagation plus, for an attempt, the inference time since its parent) at a
/// network block interval of `interval_ms` — the Poisson tail `P(anticone > k)` with `λ = 2·delay/interval`, in permille. A planning
/// estimate (independent arrivals), not a security proof.
pub fn palw_dag_width_red_permille_v1(delay_ms: u64, interval_ms: u64, k: KType) -> u32 {
    if interval_ms == 0 {
        return 1000;
    }
    let lambda = 2.0 * delay_ms as f64 / interval_ms as f64;
    let mut term = (-lambda).exp();
    let mut cdf = term;
    for i in 1..=k as u32 {
        term *= lambda / i as f64;
        cdf += term;
    }
    ((1.0 - cdf).clamp(0.0, 1.0) * 1000.0).round() as u32
}

// -------------------------------------------------------------------------------------------------
// Params: fences, the mirror's sync, validation
// -------------------------------------------------------------------------------------------------

/// **RFC-0018's arm-readiness record gate** (lane gatec-decouple, 2026-10-22). It replaces the former bare
/// `PALW_RFC0018_ARMABLE_V1: bool = false`, which refused every fence of this RFC at every real height. That pin gated nothing but
/// arming (no fold, processor or RPC reader consulted it), and its documented condition was "the RFC's user decisions are made and the
/// rules are built and verified" (§12 U-1..U-8, §10 T-0..T-5). Those are approvals and measurements, so they are now carried the
/// same way as the G14 gates (`crate::palw_g14_gates_v1`): a fence armed at a real height passes only when a pinned record names
/// exactly this (fence, network genesis, height) and carries one non-zero, distinct evidence digest per requirement plus a review
/// digest. **The pinned list is EMPTY**, so every armed fence is still refused, exactly as with the bool. No isolated-genesis
/// exception is added (the bool had none).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwRfc0018GatedFenceV1 {
    DagSiblingAttributionV1,
    PanelAnchorPermissionlessV1,
    RuleEParticipationV2,
    IndependentlyWorkCertifiedV1,
    DagWidthScheduleV1,
}

/// §12 U-1 (fence activation and order) is a requirement of every fence; the rest are per fence.
const RFC0018_U1: &str = "U-1 user decision: this fence's activation height and its order relative to the other fences (RFC-0018 §12)";

impl PalwRfc0018GatedFenceV1 {
    pub const ALL: [Self; 5] = [
        Self::DagSiblingAttributionV1,
        Self::PanelAnchorPermissionlessV1,
        Self::RuleEParticipationV2,
        Self::IndependentlyWorkCertifiedV1,
        Self::DagWidthScheduleV1,
    ];

    /// The `Params` field's name.
    pub fn name(self) -> &'static str {
        match self {
            Self::DagSiblingAttributionV1 => "palw_dag_sibling_attribution_v1",
            Self::PanelAnchorPermissionlessV1 => "palw_panel_anchor_permissionless_v1",
            Self::RuleEParticipationV2 => "palw_rule_e_participation_v2",
            Self::IndependentlyWorkCertifiedV1 => "palw_independently_work_certified_v1",
            Self::DagWidthScheduleV1 => "palw_dag_width_schedule_v1",
        }
    }

    /// The fence's height on `p` (`Some(never())` reads as `None`).
    pub fn armed(self, p: &Params) -> Option<u64> {
        match self {
            Self::DagSiblingAttributionV1 => Params::rfc0018_from(p.palw_dag_sibling_attribution_v1),
            Self::PanelAnchorPermissionlessV1 => Params::rfc0018_from(p.palw_panel_anchor_permissionless_v1),
            Self::RuleEParticipationV2 => Params::rfc0018_from(p.palw_rule_e_participation_v2),
            Self::IndependentlyWorkCertifiedV1 => Params::rfc0018_from(p.palw_independently_work_certified_v1),
            Self::DagWidthScheduleV1 => {
                p.palw_dag_width_schedule_v1.filter(|f| f.activation != ForkActivation::never()).map(|f| f.activation.daa_score())
            }
        }
    }

    /// The evidence a record must carry, one digest per entry, in this order (RFC-0018 §10 tests, §12 decisions).
    pub fn requirements(self) -> &'static [&'static str] {
        match self {
            Self::DagSiblingAttributionV1 => &[
                RFC0018_U1,
                "T-1 (§4.4): sibling order is a permutation, merge-order independent, round robin over models (palw_rfc0018_v1 tests)",
                "§10 real-process drill: three concurrent models on real nodes, partition/IBD convergence (scripts/drills/rfc0018_*)",
            ],
            Self::PanelAnchorPermissionlessV1 => &[
                RFC0018_U1,
                "T-2 (§5): anchor seed excludes the producer, ignores unadmitted contributions, is committed fold output",
                "T-2b: measured grinding bound prod(1+q_i) report",
            ],
            Self::RuleEParticipationV2 => &[
                RFC0018_U1,
                "T-3 (§6.5): participation v2 capital-split neutral, once per bond, excludes voided/uncertified",
                "U-7 user decision: no-credit tie broken by GHOSTDAG order (§6.4)",
                "T-5: DNS-retired calibration devnet run (retired)",
            ],
            Self::IndependentlyWorkCertifiedV1 => &[
                RFC0018_U1,
                "T-0 (§7.11): uncertified claims earn no participation, weight, frontier, settlement or coinbase credit (all tiers)",
                "U-5 user decision: certification m/k and who bears the m re-executions per certification",
                "U-6 user decision: veto adopted or not",
                "U-2/U-3 user decisions: issuance-leg hold payout and free-prompt pre-certification coinbase carve",
                "§10 real-process drill: Gate C fence crossing, restart/IBD/reorg identical certification rows (scripts/drills/rfc0018_*)",
            ],
            Self::DagWidthScheduleV1 => &[
                RFC0018_U1,
                "U-4 user decision and measured record: the DAG width schedule's values (T-4 structural check)",
                "§8.2 measured red fraction at the scheduled k on real propagation",
            ],
        }
    }

    /// The refusal while no record for this fence, network and height is pinned.
    pub fn no_record_refusal(self) -> &'static str {
        match self {
            Self::DagSiblingAttributionV1 => {
                "palw_dag_sibling_attribution_v1 cannot be armed: no RFC-0018 arm-readiness record is pinned for this network and height \
                 (PALW_RFC0018_ARMING_RECORDS_V1 is empty; RFC-0018 §12)"
            }
            Self::PanelAnchorPermissionlessV1 => {
                "palw_panel_anchor_permissionless_v1 cannot be armed: no RFC-0018 arm-readiness record is pinned for this network and \
                 height (PALW_RFC0018_ARMING_RECORDS_V1 is empty; RFC-0018 §12)"
            }
            Self::RuleEParticipationV2 => {
                "palw_rule_e_participation_v2 cannot be armed: no RFC-0018 arm-readiness record is pinned for this network and height \
                 (PALW_RFC0018_ARMING_RECORDS_V1 is empty; RFC-0018 §12)"
            }
            Self::IndependentlyWorkCertifiedV1 => {
                "palw_independently_work_certified_v1 cannot be armed: no RFC-0018 arm-readiness record is pinned for this network and \
                 height (PALW_RFC0018_ARMING_RECORDS_V1 is empty; RFC-0018 §12)"
            }
            Self::DagWidthScheduleV1 => {
                "palw_dag_width_schedule_v1 cannot be armed: no RFC-0018 arm-readiness record is pinned for this network and height \
                 (PALW_RFC0018_ARMING_RECORDS_V1 is empty; RFC-0018 §12)"
            }
        }
    }
}

/// The record format's version.
pub const PALW_RFC0018_ARMING_RECORD_VERSION_V1: u16 = 1;

/// **An RFC-0018 arm-readiness record** — same shape and form rules as `crate::palw_g14_gates_v1::PalwG14ArmingRecordV1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRfc0018ArmingRecordV1 {
    pub version: u16,
    pub fence: PalwRfc0018GatedFenceV1,
    pub genesis: Hash64,
    pub activation_daa: u64,
    pub source_commit: [u8; 20],
    pub evidence: &'static [[u8; 32]],
    pub review: [u8; 32],
}

/// **The pinned records — EMPTY.** Every RFC-0018 fence armed at a real height is refused (the old `ARMABLE = false`).
// Lane ACT2 (2026-10-27): testnet-12 DAA 17,000 only, every slot a typed waiver (crate::palw_arming_waiver_v1).
pub const PALW_RFC0018_ARMING_RECORDS_V1: &[PalwRfc0018ArmingRecordV1] = crate::palw_t12_17000_records_v1::PALW_T12_17000_RFC0018_RECORDS_V1;

/// Does `records` authorize arming `fence` at `activation_daa` on the network `genesis`? The first record naming exactly this
/// (fence, network, height) decides; its form must hold. `Err` carries the refusal text.
pub fn palw_rfc0018_arming_record_admits_v1(
    fence: PalwRfc0018GatedFenceV1,
    genesis: Hash64,
    activation_daa: u64,
    records: &[PalwRfc0018ArmingRecordV1],
) -> Result<(), &'static str> {
    let Some(r) = records.iter().find(|r| r.fence == fence && r.genesis == genesis && r.activation_daa == activation_daa) else {
        return Err(fence.no_record_refusal());
    };
    if r.version != PALW_RFC0018_ARMING_RECORD_VERSION_V1 {
        return Err("the RFC-0018 arm-readiness record's version is not this binary's");
    }
    if r.source_commit == [0; 20] {
        return Err("the RFC-0018 arm-readiness record names no source commit");
    }
    if r.evidence.len() != fence.requirements().len() {
        return Err("the RFC-0018 arm-readiness record does not carry one evidence digest per requirement");
    }
    if r.review == [0; 32] {
        return Err("the RFC-0018 arm-readiness record carries no review digest");
    }
    for (i, d) in r.evidence.iter().enumerate() {
        if *d == [0; 32] || *d == r.review || r.evidence[..i].contains(d) {
            return Err("an RFC-0018 arm-readiness evidence digest is zero, repeated, or equal to the review");
        }
    }
    // Lane ACT2: a slot that is a pinned waiver must be that waiver's own slot (`crate::palw_arming_waiver_v1`).
    crate::palw_arming_waiver_v1::palw_arming_record_waivers_hold_v1(
        fence.name(),
        r.genesis,
        r.activation_daa,
        fence.requirements(),
        r.evidence,
        &r.review,
        crate::palw_arming_waiver_v1::PALW_ARMING_WAIVERS_V1,
    )
    .map_err(|(_, e)| e)?;
    Ok(())
}

impl Params {
    /// Histories are keyed on the selected parent's authenticated DAA, never a candidate's declared score.
    /// None/never is exactly the old scalar, including for pre-fence replay and reorg.
    fn dag_width_history<T: Copy>(
        &self,
        current: T,
        step: impl FnOnce(PalwDagWidthScheduleV1) -> T,
    ) -> crate::config::params::ForkedParam<T> {
        match self.palw_dag_width_schedule_v1.filter(|f| f.activation != ForkActivation::never()) {
            Some(f) => crate::config::params::ForkedParam::new(current, step(f.schedule), f.activation),
            None => crate::config::params::ForkedParam::new_const(current),
        }
    }

    pub fn ghostdag_k_history(&self) -> crate::config::params::ForkedParam<KType> {
        self.dag_width_history(self.ghostdag_k(), |s| s.ghostdag_k)
    }

    pub fn max_block_parents_history(&self) -> crate::config::params::ForkedParam<u16> {
        self.dag_width_history(self.max_block_parents() as u16, |s| s.max_block_parents)
    }

    pub fn mergeset_size_limit_history(&self) -> crate::config::params::ForkedParam<u64> {
        self.dag_width_history(self.mergeset_size_limit(), |s| s.mergeset_size_limit)
    }

    pub fn merge_depth_history(&self) -> crate::config::params::ForkedParam<u64> {
        self.dag_width_history(self.merge_depth(), |s| s.merge_depth)
    }

    fn rfc0018_from(f: Option<ForkActivation>) -> Option<u64> {
        f.filter(|a| *a != ForkActivation::never()).map(|a| a.daa_score())
    }

    /// The mirror the bundle should carry (`None`: no fence of the four is armed).
    pub fn palw_rfc0018_expected_mirror_v1(&self) -> Option<PalwRfc0018MirrorV1> {
        let mirror = PalwRfc0018MirrorV1 {
            sibling_attribution_from_daa: Self::rfc0018_from(self.palw_dag_sibling_attribution_v1),
            panel_anchor_permissionless_from_daa: Self::rfc0018_from(self.palw_panel_anchor_permissionless_v1),
            rule_e_participation_v2_from_daa: Self::rfc0018_from(self.palw_rule_e_participation_v2),
            gate_c_from_daa: Self::rfc0018_from(self.palw_independently_work_certified_v1),
            gate_c_rule: PalwGateCRuleV1::SecurityCertifiedV1,
            certification: crate::palw_security_certification_v1::PalwSecurityCertificationPolicyV1::design_unapproved_v1(
                (86_400_000u64 / self.target_time_per_block().max(1)).max(1),
            ),
            fp_receipt_tail_daa: match &self.palw_consensus_mode {
                PalwConsensusMode::ConsensusV2(bundle) => {
                    bundle.freeprompt.receipt_maturity_daa().saturating_add(bundle.freeprompt.receipt_use_window_daa())
                }
                _ => 0,
            },
            attestor_liability_from_daa: Self::rfc0018_from(self.palw_gate_c_attestor_liability_v1),
        };
        (!mirror.is_empty()).then_some(mirror)
    }

    /// **The mirror** on the V2 bundle's state params, which the fold, the processor and the RPC read. Written here and nothing else.
    pub fn sync_palw_rfc0018_v1(&mut self) {
        let mirror = self.palw_rfc0018_expected_mirror_v1();
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_rfc0018_v1(mirror);
        }
    }

    /// Whether rule E's v2 participation governs at the incumbent's DAA (the IBD path's reading).
    pub fn palw_rule_e_participation_v2_active_at(&self, incumbent_daa: u64) -> bool {
        Self::rfc0018_from(self.palw_rule_e_participation_v2).is_some_and(|from| incumbent_daa >= from)
    }

    /// **Refused when armed at a real height**, every one of the five, until the RFC's user decisions are made and the rules are built
    /// and verified (a pinned [`PalwRfc0018ArmingRecordV1`]; the list is empty); `None` and `Some(never())` pass. Ordering rules checked first so a test can tell them
    /// apart: rule E v2 needs rule E at or below it; Gate C needs the tier fence and aux 136 at or below it.
    pub fn validate_palw_rfc0018_v1(&self) -> Result<(), PalwModeV2Error> {
        self.validate_palw_rfc0018_with_records_v1(PALW_RFC0018_ARMING_RECORDS_V1)
    }

    /// [`Self::validate_palw_rfc0018_v1`] with `records` as the pinned list (tests and arm-readiness tooling only).
    pub fn validate_palw_rfc0018_with_records_v1(&self, records: &[PalwRfc0018ArmingRecordV1]) -> Result<(), PalwModeV2Error> {
        if let Some(v2) = Self::rfc0018_from(self.palw_rule_e_participation_v2) {
            let rule_e = Self::rfc0018_from(self.palw_fork_choice_rule_e_v1);
            if rule_e.is_none_or(|e| e > v2) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_rule_e_participation_v2 needs palw_fork_choice_rule_e_v1 armed at or below it (it replaces rule E's first key)",
                ));
            }
        }
        // Gate C's certification attestations ride inner 75's wire form, but the processor admits them on Gate C's own fence (an
        // attestation naming an existing certification row, `palw_kernel_route_object_is_signed`) and the fold routes them to
        // `try_attest_claim_certification_v1` before RFC-0017's handler — RFC-0017's exam (`palw_model_eligibility_v1`) is NOT needed.
        // Model-level review is abolished (PRINCIPLES §9, 2026-10-15): the model tier Gate C sits beside is the frozen two-tier rule
        // (`palw_model_tier_v1`, genesis/annual-fork 1.0, registered 0.3, applied once in A_m's c_m), so that fence is required at or
        // below Gate C instead. Its rows live in the kernel route (aux 136, `palw_probabilistic_constraints_v1`): also at or below.
        // A configuration that still sets `palw_model_eligibility_v1` keeps the old ordering check (that fence can never be armed).
        // lane GC-DUTY: the attestor liability needs Gate C and sits at or below it (Gate C never runs without it once both are set).
        if let Some(l) = Self::rfc0018_from(self.palw_gate_c_attestor_liability_v1)
            && Self::rfc0018_from(self.palw_independently_work_certified_v1).is_none_or(|c| l > c)
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_gate_c_attestor_liability_v1 needs palw_independently_work_certified_v1 armed, at or above it",
            ));
        }
        if let Some(c) = Self::rfc0018_from(self.palw_independently_work_certified_v1) {
            let below = |f: Option<ForkActivation>| Self::rfc0018_from(f).is_some_and(|at| at <= c);
            if !below(self.palw_probabilistic_constraints_v1) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_independently_work_certified_v1 needs palw_probabilistic_constraints_v1 at or below it (kernel route aux 136)",
                ));
            }
            if !self.palw_model_tier_v1_from_daa().is_some_and(|tier| tier <= c) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_independently_work_certified_v1 needs palw_model_tier_v1 at or below it: the frozen two-tier rule is the model tier \
                     source (no per-model exam, PRINCIPLES §9)",
                ));
            }
            if Self::rfc0018_from(self.palw_model_eligibility_v1).is_some() && !below(self.palw_model_eligibility_v1) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_independently_work_certified_v1 needs palw_model_eligibility_v1, where set, at or below it",
                ));
            }
            if let Some(mirror) = self.palw_rfc0018_expected_mirror_v1()
                && let Err(why) = mirror.certification.validate()
            {
                return Err(PalwModeV2Error::Invalid(why));
            }
        }
        if let Some(f) = self.palw_dag_width_schedule_v1.filter(|f| f.activation != ForkActivation::never())
            && let Some(refusal) = f.schedule.refusal()
        {
            return Err(PalwModeV2Error::Invalid(refusal));
        }
        if let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
            && bundle.state.rfc0018_v1().copied() != self.palw_rfc0018_expected_mirror_v1()
        {
            return Err(PalwModeV2Error::Invalid(
                "RFC-0018's fences disagree with the V2 bundle's mirror: Params::sync_palw_rfc0018_v1",
            ));
        }
        if let Some(f) = self.palw_dag_width_schedule_v1.filter(|f| f.activation != ForkActivation::never()) {
            if f.schedule.ghostdag_k < self.ghostdag_k() {
                return Err(PalwModeV2Error::Invalid("DAG width schedule cannot decrease k across an existing k-cluster"));
            }
            if f.schedule.target_time_per_block_ms != self.target_time_per_block() {
                return Err(PalwModeV2Error::Invalid("DAG width schedule cannot change block time without a clock/window migration"));
            }
            let k = self.ghostdag_k_history().upper_bound() as u64;
            let merge = self.merge_depth_history().upper_bound();
            let size = self.mergeset_size_limit_history().upper_bound();
            let lower = self
                .finality_depth()
                .saturating_add(merge.saturating_mul(2))
                .saturating_add(size.saturating_mul(k).saturating_mul(4))
                .saturating_add(k.saturating_mul(2))
                .saturating_add(2);
            if self.pruning_depth() < lower || merge > self.finality_depth() {
                return Err(PalwModeV2Error::Invalid("DAG width schedule exceeds retained pruning/finality geometry"));
            }
        }
        self.validate_palw_rfc0018_records_in_v1(records)?;
        Ok(())
    }
}

impl Params {
    /// **The record gate** against `records` in place of the binary's pinned list (tests and arm-readiness tooling; the node validates
    /// with [`PALW_RFC0018_ARMING_RECORDS_V1`]). Unarmed fences pass.
    pub fn validate_palw_rfc0018_records_in_v1(&self, records: &[PalwRfc0018ArmingRecordV1]) -> Result<(), PalwModeV2Error> {
        for fence in PalwRfc0018GatedFenceV1::ALL {
            if let Some(at) = fence.armed(self) {
                palw_rfc0018_arming_record_admits_v1(fence, self.genesis.hash, at, records).map_err(PalwModeV2Error::Invalid)?;
            }
        }
        Ok(())
    }

}

/// Re-exported for the processor's sibling-order call site (the attempt's model class).
pub fn palw_sibling_key_of_attempt_v1(class_id: Hash64, execution_key: Hash64) -> PalwSiblingKeyV1 {
    PalwSiblingKeyV1 { class_id: Some(class_id), execution_key }
}

/// A claim's model class for the anchor/attribution reports (attempt and free-prompt alike).
pub fn palw_claim_is_attempt_v1(claim: &PalwClaimStateV2) -> bool {
    matches!(claim.source, PalwClaimSourceV2::Attempt)
}

#[cfg(test)]
mod attack_tests;
#[cfg(test)]
mod tests;
