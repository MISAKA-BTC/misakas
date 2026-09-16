//! ADR-0125 — the execution lane: its rounds, its schedule, its permits and its envelope, as pure
//! functions.
//!
//! The lane is a stream of light, fee-only blocks (`pow_layer0::POW_ALGO_ID_PALW_ROUND_V1`) that
//! carry transactions beside the PALW chain without being part of it: an execution block is never a
//! selected parent of a PALW block, never blue in anyone's mergeset, and never counted in the DAA
//! score, so every PALW window, epoch, retarget and depth reads exactly what it read before the lane
//! existed. What decides which bond may produce an execution block, and what a merging block checks,
//! is here:
//!
//! * a **round** is one second from the genesis timestamp ([`palw_execution_round_v1`]);
//! * a **schedule** is derived once per scheduler span from the attempt claims that reached `Final`
//!   in the closed span ([`palw_execution_schedule_v1`]): each security domain's credits, its quota
//!   (credits, capped at [`PALW_EXEC_DOMAIN_CAP_PERMILLE`], water-filled in integers —
//!   [`palw_execution_quotas_v1`]), its parity group ([`palw_execution_parities_v1`]), the bonds
//!   that earned it, and the span's seed — a hash of the finalized executions' roots, so no
//!   beacon, no header hash and no producer's choice enters it;
//! * a round's **permits** are drawn from the schedule alone ([`palw_execution_permits_v1`]): only
//!   domains of the round's parity may hold one, so no domain holds permits in two consecutive
//!   rounds; at most `⌈width / 3⌉` go to one domain and one to one operator;
//! * an execution block carries a **signed envelope** ([`PalwExecEnvelopeV1`]) naming its round,
//!   its permit and its bond; the header stage checks its shape, its round and its signature, and
//!   a merging block checks the permit against its own parent state;
//! * a **mergeset** holds at most `max_per_mergeset` execution blocks, at most `width` of any one
//!   round, one per permit, and an execution block only merges rounds older than its own
//!   ([`palw_execution_mergeset_rule_v1`]).
//!
//! Integer only: a quota or a draw that two platforms compute differently is a fork.

use crate::Hash64;
use crate::palw_state_v2::PalwBondKeyV2;
use std::collections::{BTreeMap, BTreeSet};

/// One round is one second.
pub const PALW_EXEC_ROUND_MS: u64 = 1_000;

/// Stage 1: one permit a round — 1 BPS. Stages 2, 5 and 10 are the same rule with a larger value
/// behind a fence of their own (ADR-0125 Decision 5).
pub const PALW_EXEC_PERMITS_PER_ROUND_V1: u16 = 1;

/// The widest round any stage may configure: the operator's 10 BPS target.
pub const PALW_EXEC_MAX_PERMITS_PER_ROUND_V1: u16 = 10;

/// No security domain holds more than 45 % of a span's quota, whatever its compute (ADR-0125
/// Decision 3).
pub const PALW_EXEC_DOMAIN_CAP_PERMILLE: u64 = 450;

/// The most domains one schedule lists — the ones with the most credits, ties to the lower id.
pub const PALW_EXEC_MAX_DOMAINS_V1: usize = 32;

/// The most bonds one domain lists — the ones with the most finalized attempts, ties to the lower
/// bond key. Bounds the schedule's bytes in the state root and a draw's work.
pub const PALW_EXEC_MAX_BONDS_PER_DOMAIN_V1: usize = 64;

/// **What the fold needs of the lane where it is open**: the span a schedule covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwExecLaneFoldV1 {
    pub schedule_span_daa: u64,
}

/// One round permit a chain block accepted: the span whose schedule granted it, its round and its
/// index. The ledger key is the span and the round, so a round's permit under one span's schedule
/// is a different permit from the same round's under the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PalwExecPermitUseV1 {
    pub span: u64,
    pub round: u64,
    pub permit_index: u16,
}

/// The most round blocks any network may let one mergeset hold
/// (`PalwExecutionLaneV1::max_per_mergeset`): ten rounds a second for four hundred seconds.
pub const PALW_EXEC_MAX_PER_MERGESET_BOUND_V1: u64 = 4_096;

/// **The most distinct bonds whose round blocks one mergeset may hold.** A merging block pays each
/// permitted bond one aggregate fee output, so this bounds the outputs the lane adds to a coinbase —
/// and the coinbase's size, which counts against the block's mass.
pub const PALW_EXEC_MAX_BONDS_PER_MERGESET_V1: usize = 64;

/// The envelope's wire version.
pub const PALW_EXEC_ENVELOPE_VERSION_V1: u8 = 1;

/// The header-carriage magic of an execution envelope — distinct from every other PALW carriage.
pub const PALW_EXEC_CARRIAGE_MAGIC_V1: [u8; 4] = *b"PXR1";

/// ML-DSA-87 signing context of an execution envelope — its own family domain.
pub const PALW_EXEC_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/exec-lane/permit/mldsa87/v1";

/// ML-DSA-87 public key length (FIPS 204).
pub const PALW_EXEC_MLDSA87_PUBKEY_LEN: usize = 2592;

/// ML-DSA-87 signature length (FIPS 204).
pub const PALW_EXEC_MLDSA87_SIGNATURE_LEN: usize = 4627;

/// The domain of a span's seed.
pub const PALW_EXEC_SEED_DOMAIN: &[u8] = b"misaka-palw/exec-lane/span-seed/v1";
/// The domain of a permit's domain pick.
pub const PALW_EXEC_DOMAIN_PICK_DOMAIN: &[u8] = b"misaka-palw/exec-lane/domain-pick/v1";
/// The domain of a permit's bond pick.
pub const PALW_EXEC_BOND_PICK_DOMAIN: &[u8] = b"misaka-palw/exec-lane/bond-pick/v1";
/// The domain of an envelope's signed message.
pub const PALW_EXEC_SIGNING_DOMAIN: &[u8] = b"misaka-palw/exec-lane/signing/v1";

/// Every keyed-BLAKE2b domain this module hashes under, for the distinctness test.
pub const PALW_EXEC_ALL_DOMAINS: &[&[u8]] = &[
    PALW_EXEC_SEED_DOMAIN,
    PALW_EXEC_DOMAIN_PICK_DOMAIN,
    PALW_EXEC_BOND_PICK_DOMAIN,
    PALW_EXEC_SIGNING_DOMAIN,
    PALW_EXEC_MLDSA87_CONTEXT,
];

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn lead_u64(hash: &Hash64) -> u64 {
    let mut lead = [0u8; 8];
    lead.copy_from_slice(&hash.as_byte_slice()[..8]);
    u64::from_le_bytes(lead)
}

fn update_bond(state: &mut blake2b_simd::State, bond: &PalwBondKeyV2) {
    state.update(bond.0.transaction_id.as_byte_slice());
    state.update(&bond.0.index.to_le_bytes());
}

/// The round a timestamp falls in: whole seconds since the genesis timestamp; a timestamp before
/// genesis is round 0.
pub fn palw_execution_round_v1(timestamp_ms: u64, genesis_timestamp_ms: u64) -> u64 {
    timestamp_ms.saturating_sub(genesis_timestamp_ms) / PALW_EXEC_ROUND_MS
}

/// The scheduler span a DAA score belongs to.
pub fn palw_execution_span_v1(daa_score: u64, span_daa: u64) -> u64 {
    daa_score / span_daa.max(1)
}

/// **A class's security domain.** The family the chain certified the class's free-prompt lane
/// under, where it holds such a row — so the quant variants of one certified family are one
/// domain — and the class itself otherwise. The attempt lane keeps no per-class family row, so a
/// class certified only by genesis is its own domain (ADR-0125 SA-8 states the residue).
pub fn palw_execution_domain_of_class_v1(fp_family_digest: Option<Hash64>, class_id: Hash64) -> Hash64 {
    fp_family_digest.unwrap_or(class_id)
}

/// **Quotas from credits: proportional, capped at 45 %, renormalised — in integers.**
///
/// `credits` is each domain's count of attempt claims finalized in the closed span. A domain with
/// no credits has no quota and is not listed. Among the rest:
///
/// * one domain holds the whole lane (there is nobody to cap it against);
/// * two domains split it evenly — no split of 1000‰ between two keeps both under 450‰, and the
///   even split is the one that minimises the larger;
/// * three or more are water-filled: a domain whose proportional share of what remains exceeds the
///   cap is fixed at the cap, and the rest is re-divided among the others until nobody exceeds it
///   (three domains at the cap already exceed 1000‰, so this always ends feasible).
///
/// Shares are exact rationals until the last step, then floored, and the missing permille go one
/// each to the largest remainders, ties to the earlier domain in `credits` order. The result sums
/// to exactly 1000 and is listed in `credits` order.
pub fn palw_execution_quotas_v1(credits: &[(Hash64, u64)]) -> Vec<(Hash64, u16)> {
    let live: Vec<(Hash64, u64)> = credits.iter().copied().filter(|(_, c)| *c > 0).collect();
    match live.len() {
        0 => return Vec::new(),
        1 => return vec![(live[0].0, 1000)],
        2 => return vec![(live[0].0, 500), (live[1].0, 500)],
        _ => {}
    }
    let cap = PALW_EXEC_DOMAIN_CAP_PERMILLE as u128;
    let mut capped = vec![false; live.len()];
    loop {
        let fixed = capped.iter().filter(|c| **c).count() as u128;
        let remaining = 1000u128 - fixed * cap;
        let total: u128 = live.iter().zip(&capped).filter(|(_, c)| !**c).map(|((_, credit), _)| *credit as u128).sum();
        // `remaining × credit / total > cap`, compared without division.
        let newly: Vec<usize> = live
            .iter()
            .enumerate()
            .filter(|(i, (_, credit))| !capped[*i] && remaining * (*credit as u128) > cap * total)
            .map(|(i, _)| i)
            .collect();
        if newly.is_empty() {
            let mut rows: Vec<(usize, u128, u128)> = live
                .iter()
                .enumerate()
                .map(|(i, (_, credit))| {
                    if capped[i] {
                        (i, cap, 0)
                    } else {
                        let scaled = remaining * (*credit as u128);
                        (i, scaled / total, scaled % total)
                    }
                })
                .collect();
            let assigned: u128 = rows.iter().map(|(_, q, _)| *q).sum();
            let mut missing = 1000u128 - assigned;
            // Every uncapped remainder shares the denominator `total`, so remainders compare
            // directly; capped rows carry remainder 0 and are never topped up above the cap.
            let mut order: Vec<usize> = (0..rows.len()).collect();
            order.sort_by(|a, b| rows[*b].2.cmp(&rows[*a].2).then(a.cmp(b)));
            for i in order {
                if missing == 0 {
                    break;
                }
                if !capped[rows[i].0] {
                    rows[i].1 += 1;
                    missing -= 1;
                }
            }
            return rows.into_iter().map(|(i, q, _)| (live[i].0, q as u16)).collect();
        }
        for i in newly {
            capped[i] = true;
        }
    }
}

/// **The parity groups: two sets of domains with quotas as even as a greedy split makes them.**
///
/// Domains are taken in descending quota (ties to the lower id) and each joins the group whose
/// total is smaller (ties to group 0). A domain may hold permits only in rounds of its group's
/// parity, so no domain holds permits in two consecutive rounds — the operator's "同一 security
/// domain 連続禁止" at every width, with no round's draw depending on the round before it. One
/// domain alone leaves the odd rounds empty (ADR-0125 SA-4). Returned in the input's order.
pub fn palw_execution_parities_v1(quotas: &[(Hash64, u16)]) -> Vec<(Hash64, u8)> {
    let mut order: Vec<usize> = (0..quotas.len()).collect();
    order.sort_by(|a, b| quotas[*b].1.cmp(&quotas[*a].1).then(quotas[*a].0.cmp(&quotas[*b].0)));
    let mut totals = [0u32; 2];
    let mut parity = vec![0u8; quotas.len()];
    for i in order {
        let group = if totals[1] < totals[0] { 1 } else { 0 };
        totals[group] += quotas[i].1 as u32;
        parity[i] = group as u8;
    }
    quotas.iter().zip(parity).map(|((domain, _), p)| (*domain, p)).collect()
}

/// One attempt claim that reached `Final` inside a closed scheduler span, as the schedule reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecFinalV1 {
    pub domain: Hash64,
    pub bond: PalwBondKeyV2,
    pub operator_id: Hash64,
    pub claim_id: Hash64,
    pub execution_root: Hash64,
}

/// A bond that may hold a permit in a span, and how many finalized attempts earned it the place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecBondV1 {
    pub bond: PalwBondKeyV2,
    pub operator_id: Hash64,
    pub finals: u64,
}

/// One security domain of a span's schedule.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecDomainV1 {
    pub domain: Hash64,
    pub credits: u64,
    pub quota_permille: u16,
    pub parity: u8,
    /// Sorted by bond key.
    pub bonds: Vec<PalwExecBondV1>,
}

/// **A scheduler span's schedule** — everything a round's draw reads, derived at the boundary
/// that opens the span and constant for it.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecScheduleV1 {
    /// The span this schedule governs (`daa_score / span_daa`).
    pub span_index: u64,
    /// `H(domain ‖ span ‖ count ‖ (claim id ‖ execution root)*)` over the closed span's finalized
    /// attempts in claim-id order. Changing it costs an inference that reaches `Final`.
    pub seed: Hash64,
    /// Sorted by domain id.
    pub domains: Vec<PalwExecDomainV1>,
}

impl PalwExecScheduleV1 {
    pub fn domain(&self, domain: &Hash64) -> Option<&PalwExecDomainV1> {
        self.domains.iter().find(|d| d.domain == *domain)
    }
}

/// **Derive a span's schedule from the attempt claims finalized in the span before it.**
pub fn palw_execution_schedule_v1(span_index: u64, finals: &[PalwExecFinalV1]) -> PalwExecScheduleV1 {
    // The seed, over every final in claim-id order (the listing caps below do not reach it).
    let mut ordered: Vec<&PalwExecFinalV1> = finals.iter().collect();
    ordered.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
    ordered.dedup_by(|a, b| a.claim_id == b.claim_id);
    let mut seed = keyed(PALW_EXEC_SEED_DOMAIN);
    seed.update(&span_index.to_le_bytes());
    seed.update(&(ordered.len() as u64).to_le_bytes());
    for f in &ordered {
        seed.update(f.claim_id.as_byte_slice());
        seed.update(f.execution_root.as_byte_slice());
    }
    let seed = finish(seed);

    // Credits and bonds per domain.
    let mut per_domain: BTreeMap<Hash64, (u64, BTreeMap<PalwBondKeyV2, (Hash64, u64)>)> = BTreeMap::new();
    for f in &ordered {
        let entry = per_domain.entry(f.domain).or_default();
        entry.0 += 1;
        let bond = entry.1.entry(f.bond).or_insert((f.operator_id, 0));
        bond.1 += 1;
    }
    let mut domains: Vec<(Hash64, u64, Vec<PalwExecBondV1>)> = per_domain
        .into_iter()
        .map(|(domain, (credits, bonds))| {
            let mut bonds: Vec<PalwExecBondV1> =
                bonds.into_iter().map(|(bond, (operator_id, finals))| PalwExecBondV1 { bond, operator_id, finals }).collect();
            bonds.sort_by(|a, b| b.finals.cmp(&a.finals).then(a.bond.cmp(&b.bond)));
            bonds.truncate(PALW_EXEC_MAX_BONDS_PER_DOMAIN_V1);
            bonds.sort_by(|a, b| a.bond.cmp(&b.bond));
            (domain, credits, bonds)
        })
        .collect();
    domains.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    domains.truncate(PALW_EXEC_MAX_DOMAINS_V1);
    domains.sort_by(|a, b| a.0.cmp(&b.0));

    let credits: Vec<(Hash64, u64)> = domains.iter().map(|(d, c, _)| (*d, *c)).collect();
    let quotas = palw_execution_quotas_v1(&credits);
    let parities = palw_execution_parities_v1(&quotas);
    let domains = domains
        .into_iter()
        .map(|(domain, credits, bonds)| {
            let quota_permille = quotas.iter().find(|(d, _)| *d == domain).map(|(_, q)| *q).unwrap_or(0);
            let parity = parities.iter().find(|(d, _)| *d == domain).map(|(_, p)| *p).unwrap_or(0);
            PalwExecDomainV1 { domain, credits, quota_permille, parity, bonds }
        })
        .collect();
    PalwExecScheduleV1 { span_index, seed, domains }
}

/// The most permits one domain may hold in a round of `width`: a third, rounded up.
pub fn palw_execution_domain_cap_v1(width: u16) -> u16 {
    width.div_ceil(3).max(1)
}

/// One permit of one round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwExecPermitV1 {
    pub index: u16,
    pub domain: Hash64,
    pub bond: PalwBondKeyV2,
    pub operator_id: Hash64,
}

/// **A round's permits, from the schedule alone.**
///
/// For each permit index in turn: the domains of the round's parity that still have room (under
/// [`palw_execution_domain_cap_v1`]) and a bond whose operator holds no permit yet are the
/// candidates; one is picked with probability proportional to its quota (a quota rounded to zero
/// weighs one) by `H(seed ‖ round ‖ index)`; within it the bond with the lowest
/// `H(seed ‖ round ‖ bond)` among those whose operator is free takes the permit. When no domain has
/// room the round's remaining permits do not exist. No bond state is read: whether the chosen bond
/// is still able to produce is the merging block's question.
pub fn palw_execution_permits_v1(schedule: &PalwExecScheduleV1, round: u64, width: u16) -> Vec<PalwExecPermitV1> {
    let width = width.min(PALW_EXEC_MAX_PERMITS_PER_ROUND_V1);
    let parity = (round % 2) as u8;
    let cap = palw_execution_domain_cap_v1(width) as usize;
    let group: Vec<&PalwExecDomainV1> = schedule.domains.iter().filter(|d| d.parity == parity && !d.bonds.is_empty()).collect();
    let mut used_operators: BTreeSet<Hash64> = BTreeSet::new();
    let mut held: Vec<usize> = vec![0; group.len()];
    let mut permits = Vec::new();
    for index in 0..width {
        let available: Vec<usize> = (0..group.len())
            .filter(|i| held[*i] < cap && group[*i].bonds.iter().any(|b| !used_operators.contains(&b.operator_id)))
            .collect();
        if available.is_empty() {
            break;
        }
        let total: u64 = available.iter().map(|i| group[*i].quota_permille.max(1) as u64).sum();
        let mut pick = keyed(PALW_EXEC_DOMAIN_PICK_DOMAIN);
        pick.update(schedule.seed.as_byte_slice());
        pick.update(&round.to_le_bytes());
        pick.update(&index.to_le_bytes());
        let mut point = lead_u64(&finish(pick)) % total;
        let mut chosen = available[available.len() - 1];
        for i in &available {
            let weight = group[*i].quota_permille.max(1) as u64;
            if point < weight {
                chosen = *i;
                break;
            }
            point -= weight;
        }
        let domain = group[chosen];
        let bond = domain
            .bonds
            .iter()
            .filter(|b| !used_operators.contains(&b.operator_id))
            .map(|b| {
                let mut ticket = keyed(PALW_EXEC_BOND_PICK_DOMAIN);
                ticket.update(schedule.seed.as_byte_slice());
                ticket.update(&round.to_le_bytes());
                update_bond(&mut ticket, &b.bond);
                (finish(ticket), b)
            })
            .min_by(|a, b| a.0.cmp(&b.0).then(a.1.bond.cmp(&b.1.bond)))
            .map(|(_, b)| *b)
            .expect("an available domain has a bond whose operator is free");
        used_operators.insert(bond.operator_id);
        held[chosen] += 1;
        permits.push(PalwExecPermitV1 { index, domain: domain.domain, bond: bond.bond, operator_id: bond.operator_id });
    }
    permits
}

/// The permit `(permit_index, bond)` names in `round`, if the schedule grants it.
pub fn palw_execution_permit_of_v1(
    schedule: &PalwExecScheduleV1,
    round: u64,
    width: u16,
    permit_index: u16,
    bond: &PalwBondKeyV2,
) -> Option<PalwExecPermitV1> {
    palw_execution_permits_v1(schedule, round, width).into_iter().find(|p| p.index == permit_index && p.bond == *bond)
}

/// **What a node's round producer reads before it builds anything** — the permits of one round as
/// the node's sink state grants them, and which of them its chain has already accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwExecRoundViewV1 {
    pub round: u64,
    /// The span of the anchor a round block built now hangs from — the schedule these permits come
    /// from.
    pub span: u64,
    pub width: u16,
    pub genesis_timestamp_ms: u64,
    pub permits: Vec<PalwExecPermitV1>,
    /// Permit indices of this round already accepted on the sink's chain.
    pub used: Vec<u16>,
}

/// Why an execution envelope was refused.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwExecEnvelopeError {
    #[error("execution envelope undecodable: {0}")]
    Undecodable(&'static str),
    #[error("execution envelope version {got}, expected {expected}")]
    UnsupportedVersion { got: u8, expected: u8 },
    #[error("execution envelope names another network")]
    NetworkDomainMismatch,
    #[error("execution envelope names round {declared} but the header's timestamp is in round {actual}")]
    RoundMismatch { declared: u64, actual: u64 },
    #[error("execution envelope permit index {index} is not below the widest round ({max})")]
    PermitIndexOutOfRange { index: u16, max: u16 },
    #[error("execution envelope public key is {got} bytes, expected {expected}")]
    PublicKeyLength { got: usize, expected: usize },
    #[error("execution envelope signature is {got} bytes, expected {expected}")]
    SignatureLength { got: usize, expected: usize },
    #[error("execution envelope signature does not verify")]
    SignatureInvalid,
}

/// **The envelope an execution block carries in `palw_commitment`.** The block's identity covers it
/// (the commitment is hash-visible past the PoW) and its signature covers the header position
/// (`pre_pow_hash` and `timestamp`), so a solved header cannot be re-announced under another permit
/// and a permit cannot be lifted onto another header.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecEnvelopeV1 {
    pub version: u8,
    pub network_domain: Hash64,
    pub round: u64,
    pub permit_index: u16,
    pub bond: PalwBondKeyV2,
    /// The bond's registered ML-DSA-87 key; the merging block requires equality with the registry.
    pub pubkey: Vec<u8>,
    pub signature: Vec<u8>,
}

/// `H(domain ‖ network ‖ pre-pow hash ‖ timestamp ‖ nonce ‖ round ‖ permit index ‖ bond)` — what
/// the permit holder signs, after it has solved the header.
///
/// **The nonce is signed.** The pre-PoW hash zeroes the nonce and the timestamp, and the envelope
/// is outside the PoW pre-image while inside the block identity. A signature over the pre-PoW hash
/// alone would let anyone re-solve the header under another nonce — a few milliseconds at the lane's
/// constant target — and re-announce the signed envelope as a distinct valid block, as often as they
/// liked. Signing the nonce leaves that to the permit holder alone.
pub fn palw_exec_signing_message_v1(
    network_domain: Hash64,
    pre_pow_hash: Hash64,
    timestamp_ms: u64,
    nonce: u64,
    round: u64,
    permit_index: u16,
    bond: &PalwBondKeyV2,
) -> Hash64 {
    let mut state = keyed(PALW_EXEC_SIGNING_DOMAIN);
    state.update(network_domain.as_byte_slice());
    state.update(pre_pow_hash.as_byte_slice());
    state.update(&timestamp_ms.to_le_bytes());
    state.update(&nonce.to_le_bytes());
    state.update(&round.to_le_bytes());
    state.update(&permit_index.to_le_bytes());
    update_bond(&mut state, bond);
    finish(state)
}

impl PalwExecEnvelopeV1 {
    /// The header-carriage wire form: magic, then borsh.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = PALW_EXEC_CARRIAGE_MAGIC_V1.to_vec();
        out.extend(borsh::to_vec(self).expect("borsh serialization of a plain struct cannot fail"));
        out
    }

    /// Magic, borsh, and nothing after it.
    pub fn decode(bytes: &[u8]) -> Result<Self, PalwExecEnvelopeError> {
        let Some(body) = bytes.strip_prefix(&PALW_EXEC_CARRIAGE_MAGIC_V1) else {
            return Err(PalwExecEnvelopeError::Undecodable("payload does not start with the PXR1 magic"));
        };
        let mut slice = body;
        let decoded = <Self as borsh::BorshDeserialize>::deserialize(&mut slice)
            .map_err(|_| PalwExecEnvelopeError::Undecodable("borsh body"))?;
        if !slice.is_empty() {
            return Err(PalwExecEnvelopeError::Undecodable("trailing bytes"));
        }
        Ok(decoded)
    }

    /// Shape only: the version, the permit index and the two ML-DSA-87 lengths.
    pub fn validate_shape(&self) -> Result<(), PalwExecEnvelopeError> {
        if self.version != PALW_EXEC_ENVELOPE_VERSION_V1 {
            return Err(PalwExecEnvelopeError::UnsupportedVersion { got: self.version, expected: PALW_EXEC_ENVELOPE_VERSION_V1 });
        }
        if self.permit_index >= PALW_EXEC_MAX_PERMITS_PER_ROUND_V1 {
            return Err(PalwExecEnvelopeError::PermitIndexOutOfRange {
                index: self.permit_index,
                max: PALW_EXEC_MAX_PERMITS_PER_ROUND_V1,
            });
        }
        if self.pubkey.len() != PALW_EXEC_MLDSA87_PUBKEY_LEN {
            return Err(PalwExecEnvelopeError::PublicKeyLength { got: self.pubkey.len(), expected: PALW_EXEC_MLDSA87_PUBKEY_LEN });
        }
        if self.signature.len() != PALW_EXEC_MLDSA87_SIGNATURE_LEN {
            return Err(PalwExecEnvelopeError::SignatureLength {
                got: self.signature.len(),
                expected: PALW_EXEC_MLDSA87_SIGNATURE_LEN,
            });
        }
        Ok(())
    }

    /// **The header stage's whole check**: shape, network, the round recomputed from the header's
    /// own timestamp, and the signature over the header position (nonce included) under the
    /// carried key.
    #[allow(clippy::too_many_arguments)]
    pub fn validate_stateless<V>(
        &self,
        network_domain: Hash64,
        pre_pow_hash: Hash64,
        timestamp_ms: u64,
        nonce: u64,
        genesis_timestamp_ms: u64,
        verify_mldsa87: V,
    ) -> Result<(), PalwExecEnvelopeError>
    where
        V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        self.validate_shape()?;
        if self.network_domain != network_domain {
            return Err(PalwExecEnvelopeError::NetworkDomainMismatch);
        }
        let actual = palw_execution_round_v1(timestamp_ms, genesis_timestamp_ms);
        if self.round != actual {
            return Err(PalwExecEnvelopeError::RoundMismatch { declared: self.round, actual });
        }
        let message = palw_exec_signing_message_v1(
            network_domain,
            pre_pow_hash,
            timestamp_ms,
            nonce,
            self.round,
            self.permit_index,
            &self.bond,
        );
        if !verify_mldsa87(&self.pubkey, message.as_byte_slice(), &self.signature, PALW_EXEC_MLDSA87_CONTEXT) {
            return Err(PalwExecEnvelopeError::SignatureInvalid);
        }
        Ok(())
    }
}

/// Why a block's mergeset breaks the execution lane's shape.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwExecMergesetError {
    #[error("the mergeset holds {count} execution blocks, above the lane's bound of {bound}")]
    TooMany { count: u64, bound: u64 },
    #[error("the mergeset holds {count} execution blocks of round {round}, above the round's width {width}")]
    RoundTooWide { round: u64, count: u64, width: u16 },
    #[error("the mergeset holds two execution blocks for round {round} permit {index}")]
    DuplicatePermit { round: u64, index: u16 },
    #[error("an execution block of round {block_round} merges an execution block of round {member_round}, which is not older")]
    RoundNotOlder { block_round: u64, member_round: u64 },
    #[error("the mergeset holds execution blocks of {count} bonds, above the payee bound of {bound}")]
    TooManyBonds { count: usize, bound: usize },
}

/// **The header stage's mergeset rule.** `members` are the execution blocks in a block's mergeset
/// as `(round, permit index, bond)`; `block_round` is `Some` when the block is itself an execution
/// block. No state and no walk: a property of the mergeset's headers, like the heartbeat width rule.
pub fn palw_execution_mergeset_rule_v1(
    block_round: Option<u64>,
    members: &[(u64, u16, PalwBondKeyV2)],
    width: u16,
    max_per_mergeset: u64,
) -> Result<(), PalwExecMergesetError> {
    if members.len() as u64 > max_per_mergeset {
        return Err(PalwExecMergesetError::TooMany { count: members.len() as u64, bound: max_per_mergeset });
    }
    let mut per_round: BTreeMap<u64, u64> = BTreeMap::new();
    let mut permits: BTreeSet<(u64, u16)> = BTreeSet::new();
    let mut bonds: BTreeSet<PalwBondKeyV2> = BTreeSet::new();
    for (round, index, bond) in members {
        if let Some(block_round) = block_round
            && *round >= block_round
        {
            return Err(PalwExecMergesetError::RoundNotOlder { block_round, member_round: *round });
        }
        if !permits.insert((*round, *index)) {
            return Err(PalwExecMergesetError::DuplicatePermit { round: *round, index: *index });
        }
        let count = per_round.entry(*round).or_insert(0);
        *count += 1;
        if *count > width as u64 {
            return Err(PalwExecMergesetError::RoundTooWide { round: *round, count: *count, width });
        }
        bonds.insert(*bond);
        if bonds.len() > PALW_EXEC_MAX_BONDS_PER_MERGESET_V1 {
            return Err(PalwExecMergesetError::TooManyBonds { count: bonds.len(), bound: PALW_EXEC_MAX_BONDS_PER_MERGESET_V1 });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn bond(v: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(v), 0))
    }

    fn final_of(domain: u64, bond_word: u64, operator: u64, claim: u64) -> PalwExecFinalV1 {
        PalwExecFinalV1 {
            domain: h(domain),
            bond: bond(bond_word),
            operator_id: h(operator),
            claim_id: h(claim),
            execution_root: h(claim + 1_000_000),
        }
    }

    #[test]
    fn a_round_is_a_second_from_genesis_and_a_span_is_daa() {
        assert_eq!(palw_execution_round_v1(1_000_000, 1_000_000), 0);
        assert_eq!(palw_execution_round_v1(1_000_999, 1_000_000), 0);
        assert_eq!(palw_execution_round_v1(1_001_000, 1_000_000), 1);
        assert_eq!(palw_execution_round_v1(0, 1_000_000), 0, "before genesis is round 0, never a wrap");
        assert_eq!(palw_execution_span_v1(59, 30), 1);
        assert_eq!(palw_execution_span_v1(59, 0), 59, "a zero span is read as one, never a division by zero");
    }

    #[test]
    fn the_domains_are_distinct() {
        let set: BTreeSet<&[u8]> = PALW_EXEC_ALL_DOMAINS.iter().copied().collect();
        assert_eq!(set.len(), PALW_EXEC_ALL_DOMAINS.len());
    }

    #[test]
    fn a_class_is_its_certified_familys_domain_or_its_own() {
        assert_eq!(palw_execution_domain_of_class_v1(Some(h(9)), h(1)), h(9));
        assert_eq!(palw_execution_domain_of_class_v1(None, h(1)), h(1));
    }

    #[test]
    fn quotas_follow_credits_up_to_the_cap_in_integers_and_sum_to_a_thousand() {
        assert!(palw_execution_quotas_v1(&[]).is_empty());
        assert!(palw_execution_quotas_v1(&[(h(1), 0)]).is_empty(), "no credits, no quota");
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 5)]), vec![(h(1), 1000)], "one domain holds the lane");
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 99), (h(2), 1)]), vec![(h(1), 500), (h(2), 500)], "two split evenly");
        assert_eq!(
            palw_execution_quotas_v1(&[(h(1), 5), (h(2), 0), (h(3), 0)]),
            vec![(h(1), 1000)],
            "zero-credit domains are not listed"
        );
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 70), (h(2), 20), (h(3), 10)]), vec![(h(1), 450), (h(2), 367), (h(3), 183)]);
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 40), (h(2), 35), (h(3), 25)]), vec![(h(1), 400), (h(2), 350), (h(3), 250)]);
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 1), (h(2), 1), (h(3), 1)]), vec![(h(1), 334), (h(2), 333), (h(3), 333)]);
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 49), (h(2), 49), (h(3), 2)]), vec![(h(1), 450), (h(2), 450), (h(3), 100)]);
        // A cascade: pass one caps the 60; the 39 is then 550 × 39 / 40 = 536 > 450, capped in pass two.
        assert_eq!(palw_execution_quotas_v1(&[(h(1), 60), (h(2), 39), (h(3), 1)]), vec![(h(1), 450), (h(2), 450), (h(3), 100)]);
        // Rounding after a cap: 550 split 30 : 5 : 5 = 412.5, 68.75, 68.75.
        assert_eq!(
            palw_execution_quotas_v1(&[(h(1), 60), (h(2), 30), (h(3), 5), (h(4), 5)]),
            vec![(h(1), 450), (h(2), 412), (h(3), 69), (h(4), 69)]
        );
        for census in [
            vec![(h(1), 1), (h(2), 1), (h(3), 1)],
            vec![(h(1), 3), (h(2), 3), (h(3), 3), (h(4), 1)],
            vec![(h(1), 1_000_000), (h(2), 7), (h(3), 3), (h(4), 1)],
            vec![(h(1), u64::MAX), (h(2), u64::MAX), (h(3), u64::MAX)],
        ] {
            let quotas = palw_execution_quotas_v1(&census);
            assert_eq!(quotas.iter().map(|(_, p)| *p as u32).sum::<u32>(), 1000, "{census:?}");
            assert!(quotas.iter().all(|(_, p)| (*p as u64) <= PALW_EXEC_DOMAIN_CAP_PERMILLE), "{census:?} breaches the cap");
        }
    }

    #[test]
    fn parity_groups_balance_quotas_greedily() {
        assert_eq!(palw_execution_parities_v1(&[(h(1), 1000)]), vec![(h(1), 0)], "one domain: even rounds only");
        assert_eq!(palw_execution_parities_v1(&[(h(1), 500), (h(2), 500)]), vec![(h(1), 0), (h(2), 1)]);
        // 450 / 367 / 183: 450 → group 0; 367 → group 1; 183 → group 1 (367 < 450).
        assert_eq!(palw_execution_parities_v1(&[(h(1), 450), (h(2), 367), (h(3), 183)]), vec![(h(1), 0), (h(2), 1), (h(3), 1)]);
        // 400 / 350 / 250: 400 → 0; 350 → 1; 250 → 1 (350 < 400).
        assert_eq!(palw_execution_parities_v1(&[(h(1), 400), (h(2), 350), (h(3), 250)]), vec![(h(1), 0), (h(2), 1), (h(3), 1)]);
    }

    #[test]
    fn the_schedule_counts_credits_lists_earners_and_seeds_from_the_executions() {
        let finals = vec![
            final_of(1, 10, 100, 1),
            final_of(1, 10, 100, 2),
            final_of(1, 11, 101, 3),
            final_of(2, 20, 200, 4),
            final_of(3, 30, 300, 5),
        ];
        let schedule = palw_execution_schedule_v1(7, &finals);
        assert_eq!(schedule.span_index, 7);
        assert_eq!(schedule.domains.iter().map(|d| d.domain).collect::<Vec<_>>(), vec![h(1), h(2), h(3)], "sorted by domain id");
        let d1 = schedule.domain(&h(1)).unwrap();
        assert_eq!(d1.credits, 3);
        assert_eq!(d1.bonds.iter().map(|b| (b.bond, b.finals)).collect::<Vec<_>>(), vec![(bond(10), 2), (bond(11), 1)]);
        assert_eq!(schedule.domains.iter().map(|d| d.quota_permille as u32).sum::<u32>(), 1000);
        assert!(schedule.domains.iter().all(|d| (d.quota_permille as u64) <= PALW_EXEC_DOMAIN_CAP_PERMILLE));
        // Order-independent, duplicate-proof, and the seed moves with any execution root.
        let mut shuffled = finals.clone();
        shuffled.reverse();
        shuffled.push(finals[0]);
        assert_eq!(palw_execution_schedule_v1(7, &shuffled), schedule);
        let mut forged = finals.clone();
        forged[4].execution_root = h(42);
        assert_ne!(palw_execution_schedule_v1(7, &forged).seed, schedule.seed);
        assert_ne!(palw_execution_schedule_v1(8, &finals).seed, schedule.seed, "the span is in the seed");
        // An empty span has no domains, and so no permits.
        let empty = palw_execution_schedule_v1(7, &[]);
        assert!(empty.domains.is_empty());
        assert!(palw_execution_permits_v1(&empty, 3, 10).is_empty());
    }

    #[test]
    fn the_listing_caps_keep_the_most_productive() {
        let mut finals = Vec::new();
        for b in 0..(PALW_EXEC_MAX_BONDS_PER_DOMAIN_V1 as u64 + 5) {
            for c in 0..=(b % 3) {
                finals.push(final_of(1, 1_000 + b, 5_000 + b, 10_000 * (b + 1) + c));
            }
        }
        let schedule = palw_execution_schedule_v1(0, &finals);
        let d = schedule.domain(&h(1)).unwrap();
        assert_eq!(d.bonds.len(), PALW_EXEC_MAX_BONDS_PER_DOMAIN_V1);
        assert!(d.bonds.windows(2).all(|w| w[0].bond < w[1].bond), "stored in bond order");
        let productive = (0..(PALW_EXEC_MAX_BONDS_PER_DOMAIN_V1 as u64 + 5)).filter(|b| b % 3 != 0).count();
        assert_eq!(
            d.bonds.iter().filter(|b| b.finals >= 2).count(),
            productive,
            "every bond with more than one final is listed; only one-final bonds are dropped"
        );
    }

    fn three_domain_schedule() -> PalwExecScheduleV1 {
        let mut finals = Vec::new();
        let mut claim = 0;
        for (domain, bonds, per_bond) in [(1u64, 6u64, 7u64), (2, 4, 5), (3, 3, 3)] {
            for b in 0..bonds {
                for _ in 0..per_bond {
                    claim += 1;
                    finals.push(final_of(domain, domain * 100 + b, domain * 1_000 + b, claim));
                }
            }
        }
        palw_execution_schedule_v1(3, &finals)
    }

    #[test]
    fn a_round_draws_from_its_parity_caps_domains_and_operators_and_is_deterministic() {
        let schedule = three_domain_schedule();
        for round in 0..200u64 {
            for width in [1u16, 2, 5, 10] {
                let permits = palw_execution_permits_v1(&schedule, round, width);
                assert_eq!(permits, palw_execution_permits_v1(&schedule, round, width), "deterministic");
                assert!(permits.len() <= width as usize);
                let parity = (round % 2) as u8;
                for p in &permits {
                    assert_eq!(schedule.domain(&p.domain).unwrap().parity, parity, "only the round's parity holds permits");
                }
                let operators: BTreeSet<Hash64> = permits.iter().map(|p| p.operator_id).collect();
                assert_eq!(operators.len(), permits.len(), "one permit an operator");
                let mut per_domain: BTreeMap<Hash64, u16> = BTreeMap::new();
                for p in &permits {
                    *per_domain.entry(p.domain).or_insert(0) += 1;
                }
                assert!(per_domain.values().all(|n| *n <= palw_execution_domain_cap_v1(width)), "the per-round cap");
                for (i, p) in permits.iter().enumerate() {
                    assert_eq!(p.index as usize, i);
                    assert_eq!(palw_execution_permit_of_v1(&schedule, round, width, p.index, &p.bond), Some(*p));
                }
                // A permit is not transferable: another bond at the same index is not granted.
                if let Some(p) = permits.first() {
                    assert_eq!(palw_execution_permit_of_v1(&schedule, round, width, p.index, &bond(999_999)), None);
                }
            }
        }
    }

    #[test]
    fn no_domain_holds_permits_in_two_consecutive_rounds() {
        let schedule = three_domain_schedule();
        for width in [1u16, 2, 5, 10] {
            let mut previous: BTreeSet<Hash64> = BTreeSet::new();
            for round in 0..300u64 {
                let domains: BTreeSet<Hash64> = palw_execution_permits_v1(&schedule, round, width).iter().map(|p| p.domain).collect();
                assert!(domains.is_disjoint(&previous), "width {width}, round {round}: a domain held permits in two rounds running");
                previous = domains;
            }
        }
    }

    #[test]
    fn a_long_run_tracks_the_quotas_within_a_parity_group() {
        // Group 1 holds domains 2 and 3; their share of group-1 permits follows their quotas.
        let schedule = three_domain_schedule();
        assert_eq!(
            schedule.domain(&h(2)).unwrap().parity,
            schedule.domain(&h(3)).unwrap().parity,
            "the fixture puts 2 and 3 together"
        );
        let mut counts: BTreeMap<Hash64, u64> = BTreeMap::new();
        for round in (1..40_001u64).step_by(2) {
            for p in palw_execution_permits_v1(&schedule, round, 1) {
                *counts.entry(p.domain).or_insert(0) += 1;
            }
        }
        let (c2, c3) = (counts.get(&h(2)).copied().unwrap_or(0), counts.get(&h(3)).copied().unwrap_or(0));
        let (w2, w3) = (schedule.domain(&h(2)).unwrap().quota_permille as u64, schedule.domain(&h(3)).unwrap().quota_permille as u64);
        // c2 / (c2 + c3) ≈ w2 / (w2 + w3), within one percentage point over 20,000 rounds.
        let lhs = c2 * (w2 + w3) * 100;
        let rhs = w2 * (c2 + c3) * 100;
        let tolerance = (c2 + c3) * (w2 + w3);
        assert!(lhs.abs_diff(rhs) <= tolerance, "domain 2 held {c2} and domain 3 held {c3} against quotas {w2}:{w3}");
    }

    #[test]
    fn one_live_domain_runs_at_half_the_rounds() {
        let finals: Vec<PalwExecFinalV1> = (0..3).map(|b| final_of(1, 10 + b, 100 + b, b + 1)).collect();
        let schedule = palw_execution_schedule_v1(0, &finals);
        let produced = (0..100u64).filter(|r| !palw_execution_permits_v1(&schedule, *r, 1).is_empty()).count();
        assert_eq!(produced, 50, "exactly the even rounds");
    }

    fn envelope(round: u64) -> PalwExecEnvelopeV1 {
        PalwExecEnvelopeV1 {
            version: PALW_EXEC_ENVELOPE_VERSION_V1,
            network_domain: h(77),
            round,
            permit_index: 0,
            bond: bond(5),
            pubkey: vec![7; PALW_EXEC_MLDSA87_PUBKEY_LEN],
            signature: vec![9; PALW_EXEC_MLDSA87_SIGNATURE_LEN],
        }
    }

    #[test]
    fn the_envelope_round_trips_and_refuses_what_is_not_its_own() {
        let env = envelope(3);
        let bytes = env.encode();
        assert!(bytes.len() < crate::pow_layer0::PALW_COMMITMENT_MAX_BYTES, "fits the header carriage");
        assert_eq!(PalwExecEnvelopeV1::decode(&bytes).unwrap(), env);
        assert!(PalwExecEnvelopeV1::decode(&bytes[4..]).is_err(), "no magic");
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(PalwExecEnvelopeV1::decode(&trailing).is_err(), "trailing bytes");

        let genesis = 1_000_000u64;
        let timestamp = genesis + 3_500;
        let pre_pow = h(123);
        let nonce = 41u64;
        let signed = palw_exec_signing_message_v1(h(77), pre_pow, timestamp, nonce, 3, 0, &bond(5));
        let verify = |pk: &[u8], msg: &[u8], sig: &[u8], ctx: &[u8]| {
            pk == [7u8; PALW_EXEC_MLDSA87_PUBKEY_LEN].as_slice()
                && msg == signed.as_byte_slice()
                && sig == [9u8; PALW_EXEC_MLDSA87_SIGNATURE_LEN].as_slice()
                && ctx == PALW_EXEC_MLDSA87_CONTEXT
        };
        assert_eq!(env.validate_stateless(h(77), pre_pow, timestamp, nonce, genesis, verify), Ok(()));
        assert_eq!(
            env.validate_stateless(h(78), pre_pow, timestamp, nonce, genesis, verify),
            Err(PalwExecEnvelopeError::NetworkDomainMismatch)
        );
        assert_eq!(
            env.validate_stateless(h(77), pre_pow, timestamp + 1_000, nonce, genesis, verify),
            Err(PalwExecEnvelopeError::RoundMismatch { declared: 3, actual: 4 })
        );
        assert_eq!(
            env.validate_stateless(h(77), h(124), timestamp, nonce, genesis, verify),
            Err(PalwExecEnvelopeError::SignatureInvalid)
        );
        // Re-solving the same header under another nonce does not carry the signature with it.
        assert_eq!(
            env.validate_stateless(h(77), pre_pow, timestamp, nonce + 1, genesis, verify),
            Err(PalwExecEnvelopeError::SignatureInvalid)
        );
        let mut wide = env.clone();
        wide.permit_index = PALW_EXEC_MAX_PERMITS_PER_ROUND_V1;
        assert!(matches!(wide.validate_shape(), Err(PalwExecEnvelopeError::PermitIndexOutOfRange { .. })));
        let mut short = env.clone();
        short.signature.pop();
        assert!(matches!(short.validate_shape(), Err(PalwExecEnvelopeError::SignatureLength { .. })));
        let mut old = env;
        old.version = 0;
        assert!(matches!(old.validate_shape(), Err(PalwExecEnvelopeError::UnsupportedVersion { .. })));
    }

    #[test]
    fn the_mergeset_rule_bounds_width_duplicates_count_and_order() {
        assert_eq!(palw_execution_mergeset_rule_v1(None, &[], 1, 10), Ok(()));
        assert_eq!(palw_execution_mergeset_rule_v1(Some(5), &[(1, 0, bond(1)), (2, 0, bond(1)), (4, 0, bond(1))], 1, 10), Ok(()));
        assert_eq!(
            palw_execution_mergeset_rule_v1(Some(5), &[(5, 0, bond(1))], 1, 10),
            Err(PalwExecMergesetError::RoundNotOlder { block_round: 5, member_round: 5 })
        );
        assert_eq!(palw_execution_mergeset_rule_v1(None, &[(9, 0, bond(1))], 1, 10), Ok(()), "a PALW block merges any round");
        assert_eq!(
            palw_execution_mergeset_rule_v1(None, &[(3, 0, bond(1)), (3, 1, bond(1))], 1, 10),
            Err(PalwExecMergesetError::RoundTooWide { round: 3, count: 2, width: 1 })
        );
        assert_eq!(palw_execution_mergeset_rule_v1(None, &[(3, 0, bond(1)), (3, 1, bond(1))], 2, 10), Ok(()));
        assert_eq!(
            palw_execution_mergeset_rule_v1(None, &[(3, 1, bond(1)), (3, 1, bond(1))], 2, 10),
            Err(PalwExecMergesetError::DuplicatePermit { round: 3, index: 1 })
        );
        assert_eq!(
            palw_execution_mergeset_rule_v1(None, &[(1, 0, bond(1)), (2, 0, bond(1)), (3, 0, bond(1))], 1, 2),
            Err(PalwExecMergesetError::TooMany { count: 3, bound: 2 })
        );
        // One aggregate fee output per bond: the 65th bond in one mergeset is refused.
        let many: Vec<(u64, u16, PalwBondKeyV2)> =
            (0..=PALW_EXEC_MAX_BONDS_PER_MERGESET_V1 as u64).map(|i| (i, 0u16, bond(i))).collect();
        assert_eq!(
            palw_execution_mergeset_rule_v1(None, &many, 1, 1_000),
            Err(PalwExecMergesetError::TooManyBonds { count: 65, bound: PALW_EXEC_MAX_BONDS_PER_MERGESET_V1 })
        );
        assert_eq!(palw_execution_mergeset_rule_v1(None, &many[..64], 1, 1_000), Ok(()));
    }

    /// **Integer only.** A consensus quota or draw that two platforms round differently is a fork,
    /// so no floating-point type may be spelled in this file's non-test code.
    #[test]
    fn no_floating_point_type_is_spelled_outside_the_tests() {
        let source = include_str!("palw_execution_lane_v1.rs");
        let code = source.split("#[cfg(test)]").next().expect("the file has code before its tests");
        let code: String = code.lines().filter(|line| !line.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        for token in [concat!("f", "32"), concat!("f", "64")] {
            assert!(
                !code.split(|c: char| !c.is_alphanumeric() && c != '_').any(|word| word == token),
                "{token} is spelled in this file"
            );
        }
    }
}
