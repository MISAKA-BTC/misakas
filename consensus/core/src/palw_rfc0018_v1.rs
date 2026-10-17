//! **RFC-0018 — DAG parallel multi-model consensus** (`docs/rfc/0018-dag-parallel-multi-model-consensus.md`). Five DORMANT fences:
//!
//! | fence | what it changes past its height | keyed on |
//! | --- | --- | --- |
//! | `palw_dag_sibling_attribution_v1` | merged attempts of one accepting block are admitted in a canonical, colour- and merge-order-independent order (round robin over model classes, each class by `H(domain ‖ execution key)`), so which concurrent sibling wins a contended per-block cap — and which admitted attempt is the ADR-0170 seed anchor — is a function of the mergeset SET | the accepting block's DAA |
//! | `palw_panel_anchor_permissionless_v1` | lane A's operator-only anchor is replaced: any ADMITTED attempt of a registered, matured bond (blue or red) contributes, one contribution per bond, and the seed is the aggregate of the contributions at or past the slot | the anchor block's DAA (as lane A) |
//! | `palw_rule_e_participation_v2` | rule E's participation is the capital (read in the common past `F`) of the distinct bonds with a non-voided, Gate-C-certified attempt in a side's exclusive past; the even split is a third of the common capital | the incumbent's DAA (as rule E) |
//! | `palw_independently_work_certified_v1` (Gate C) | an uncertified claim contributes nothing to participation, provisional weight, final weight, safe-frontier advance, settled-anchor credit or coinbase | the claim's acceptance DAA |
//! | `palw_dag_width_schedule_v1` | a named (k, block time, parents, mergeset) schedule — SPEC AND STUB ONLY: no reader exists; validation refuses it | the selected parent's DAA (spec) |
//!
//! **Dormant**: every fence is `None` on every preset and in no flag-day list, hashed Some-only in both fingerprints, collapsed from
//! `Some(never())`, and refused when armed ([`Params::validate_palw_rfc0018_v1`]). The fold, the processor and the RPC read the four
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
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwClaimStateV2, PalwStateParamsV2};
use blake2b_simd::Params as Blake2bParams;
use std::collections::{BTreeMap, BTreeSet};

// -------------------------------------------------------------------------------------------------
// Domains
// -------------------------------------------------------------------------------------------------

/// The canonical sibling order's key domain.
pub const PALW_RFC0018_SIBLING_ORDER_DOMAIN_V1: &[u8] = b"misaka-palw/rfc0018/sibling-order/v1";
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
    let Some(mirror) = palw_rfc0018_mirror_v1(state.params) else { return true };
    if !mirror.gate_c_governs(claim.accepted_daa) {
        return true;
    }
    if matches!(claim.phase, PalwClaimPhaseV2::Voided { .. }) {
        return false;
    }
    // (Round 5, user decision 2026-10-15: model-level review is abolished, so U-2's Probation bar has no subject and is removed —
    // this per-claim drawn certification is the one safety gate, the same for every model tier.)
    match mirror.gate_c_rule {
        PalwGateCRuleV1::SecurityCertifiedV1 => {
            crate::palw_security_certification_v1::palw_claim_security_certified_v1(state.chain, claim_id)
        }
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
    crate::palw_security_certification_v1::palw_consensus_credit_anchor_v1(governs, certified, &claim.source, &claim.phase)
}

/// **Gate C's receipt-lane refusal** (RFC-0018 18-A): a governed free-prompt claim that is `Final` but not `SecurityCertified` licenses no
/// receipt block. `status` is the RPC's certification state (`pending`, `no-population`, `not-certified`, `vetoed`, `expired`,
/// `withdrawn`, `revoked`).
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
    let certified_daa = crate::palw_security_certification_v1::palw_claim_certified_daa_v1(chain, claim_id).unwrap_or(final_daa);
    Ok(final_daa.max(certified_daa))
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
/// round robin over model classes: classes ordered by their first member's priority, each class's attempts by priority, one attempt per
/// class per round. The result depends only on the SET of `(class, execution key)` pairs at the attempt positions — not on GHOSTDAG
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
    let mut classes: Vec<Vec<(Hash64, usize)>> = by_class
        .into_values()
        .map(|mut members| {
            members.sort();
            members
        })
        .collect();
    classes.sort_by(|a, b| a[0].cmp(&b[0]));
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
pub fn palw_permissionless_contributions_from_state_v1(state: &PalwChainStateV2, before_blue_score: u64) -> Vec<PalwAnchorContributionV1> {
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
// The DAG width schedule (RFC-0018 §8) — SPEC AND STUB
// -------------------------------------------------------------------------------------------------

/// **One step of the DAG width schedule.** No reader exists in this binary: GHOSTDAG's `k`, the parents bound and the mergeset bound are
/// constructor scalars of the GHOSTDAG manager and the header pipeline, and the block time scales every DAA-denominated PALW window
/// (RFC-0018 §8.4). A per-block reader (ForkedParam-style, keyed on the selected parent's DAA) is the follow-up the user decides on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwDagWidthScheduleV1 {
    pub ghostdag_k: KType,
    pub target_time_per_block_ms: u64,
    pub max_block_parents: u16,
    pub mergeset_size_limit: u64,
}

impl PalwDagWidthScheduleV1 {
    /// The schedule's identity (hashed into both fingerprints beside the height).
    pub fn id(&self) -> Hash64 {
        let mut h = keyed(PALW_RFC0018_DAG_WIDTH_DOMAIN_V1);
        h.update(&self.ghostdag_k.to_le_bytes());
        h.update(&self.target_time_per_block_ms.to_le_bytes());
        h.update(&self.max_block_parents.to_le_bytes());
        h.update(&self.mergeset_size_limit.to_le_bytes());
        finish(h)
    }

    /// Structural refusals (the values themselves are the user's).
    pub fn refusal(&self) -> Option<&'static str> {
        if self.ghostdag_k == 0 || self.max_block_parents < 2 || self.target_time_per_block_ms == 0 {
            return Some("palw_dag_width_schedule_v1: k ≥ 1, at least two parents and a positive block time");
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

/// Every fence of this RFC can be armed in this binary? `false`: none can (RFC-0018 §12, user decisions open).
pub const PALW_RFC0018_ARMABLE_V1: bool = false;

impl Params {
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
    /// and verified ([`PALW_RFC0018_ARMABLE_V1`]); `None` and `Some(never())` pass. Ordering rules checked first so a test can tell them
    /// apart: rule E v2 needs rule E at or below it; Gate C needs nothing (it only removes credit).
    pub fn validate_palw_rfc0018_v1(&self) -> Result<(), PalwModeV2Error> {
        if let Some(v2) = Self::rfc0018_from(self.palw_rule_e_participation_v2) {
            let rule_e = Self::rfc0018_from(self.palw_fork_choice_rule_e_v1);
            if rule_e.is_none_or(|e| e > v2) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_rule_e_participation_v2 needs palw_fork_choice_rule_e_v1 armed at or below it (it replaces rule E's first key)",
                ));
            }
        }
        // Gate C reuses RFC-0017's attestation object (inner 75, dropped by the A-2 gate below `palw_model_eligibility_v1`) and keeps
        // its rows in the kernel route (aux 136, `palw_probabilistic_constraints_v1`): both at or below it.
        if let Some(c) = Self::rfc0018_from(self.palw_independently_work_certified_v1) {
            let below = |f: Option<ForkActivation>| Self::rfc0018_from(f).is_some_and(|at| at <= c);
            if !below(self.palw_model_eligibility_v1) || !below(self.palw_probabilistic_constraints_v1) {
                return Err(PalwModeV2Error::Invalid(
                    "palw_independently_work_certified_v1 needs palw_model_eligibility_v1 and palw_probabilistic_constraints_v1 at or below it",
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
            return Err(PalwModeV2Error::Invalid("RFC-0018's fences disagree with the V2 bundle's mirror: Params::sync_palw_rfc0018_v1"));
        }
        let armed = [
            ("palw_dag_sibling_attribution_v1", Self::rfc0018_from(self.palw_dag_sibling_attribution_v1)),
            ("palw_panel_anchor_permissionless_v1", Self::rfc0018_from(self.palw_panel_anchor_permissionless_v1)),
            ("palw_rule_e_participation_v2", Self::rfc0018_from(self.palw_rule_e_participation_v2)),
            ("palw_independently_work_certified_v1", Self::rfc0018_from(self.palw_independently_work_certified_v1)),
            (
                "palw_dag_width_schedule_v1",
                self.palw_dag_width_schedule_v1.filter(|f| f.activation != ForkActivation::never()).map(|f| f.activation.daa_score()),
            ),
        ];
        if !PALW_RFC0018_ARMABLE_V1 && armed.iter().any(|(_, at)| at.is_some()) {
            return Err(PalwModeV2Error::Invalid(
                "an RFC-0018 fence cannot be armed in this binary: implemented dormant, user decisions open (RFC-0018 §12)",
            ));
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
mod tests;
#[cfg(test)]
mod attack_tests;
