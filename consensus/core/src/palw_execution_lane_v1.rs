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
//! * a **schedule** is derived in two steps, participants first and randomness after (ADR-0130).
//!   At the first chain block of span `s + 1` the attempt claims that reached `Final` in span `s`
//!   become a **snapshot** for span `s + 2` ([`palw_execution_schedule_snapshot_v1`]): each
//!   security domain's credits — the canonical compute its attempts certified
//!   ([`palw_execution_credit_v1`]) — its quota (credits, capped at
//!   [`PALW_EXEC_DOMAIN_CAP_PERMILLE`], water-filled in integers — [`palw_execution_quotas_v1`]),
//!   its parity group ([`palw_execution_parities_v1`]), and the bonds that earned it, each
//!   operator's bonds listed only in domains of that operator's one parity
//!   ([`palw_execution_operator_parities_v1`]). At the first chain block of span `s + 2` the
//!   snapshot is **seeded** ([`palw_execution_schedule_seeded_v1`]) from an anchor that did not
//!   exist when the participants were fixed — the execution of the latest attempt-carrying chain
//!   block of span `s + 1` ([`PalwExecSeedAnchorV1`]) — and the state's safe frontier
//!   ([`palw_execution_span_seed_v1`]). No beacon and no header hash enters it; without an
//!   anchor the span has no schedule;
//! * a round's **permits** are drawn from the schedule alone ([`palw_execution_permits_v1`]): only
//!   domains of the round's parity may hold one, so no domain — and, since an operator is listed
//!   in one parity only, no operator — holds permits in two consecutive rounds of a span; at most
//!   `⌈width / 3⌉` go to one domain and one to one operator;
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

/// Stage 1: one permit a round — 1 BPS. Every later stage is a widening behind a height of its own
/// (ADR-0125 Decision 7, §7.2's stage table).
pub const PALW_EXEC_PERMITS_PER_ROUND_V1: u16 = 1;

/// The widest round any stage may configure: the operator's 10 BPS target.
pub const PALW_EXEC_MAX_PERMITS_PER_ROUND_V1: u16 = 10;

/// **How many widenings one lane can schedule** — enough to go from one permit a round to ten one
/// permit at a time. Each is a height and a width, and a span keeps the width of the stage in force
/// at the DAA score it opens with, so history validates at the width it was produced at.
pub const PALW_EXEC_MAX_WIDENINGS_V1: usize = 9;

/// No security domain holds more than 45 % of a span's quota, whatever its compute (ADR-0125
/// Decision 3).
pub const PALW_EXEC_DOMAIN_CAP_PERMILLE: u64 = 450;

/// The most domains one schedule lists — the ones with the most credits, ties to the lower id.
pub const PALW_EXEC_MAX_DOMAINS_V1: usize = 32;

/// The most bonds one domain lists — the ones with the most finalized compute, ties to the lower
/// bond key. Bounds the schedule's bytes in the state root and a draw's work.
pub const PALW_EXEC_MAX_BONDS_PER_DOMAIN_V1: usize = 64;

/// **What the fold needs of the lane where it is open**: the span a schedule covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PalwExecLaneFoldV1 {
    pub schedule_span_daa: u64,
    /// CanonicalWork units per execution quantum. Zero keeps the ADR-0125 credit lottery.
    pub execution_quantum: u64,
    /// Wall-clock round of this block, used as the first eligible round when quanta are assigned.
    pub span_open_round: u64,
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

/// The domain of a span's seed (ADR-0130). `span-seed/v1` hashed the finalized executions' own
/// claim ids and roots, which a producer choosing among its executions could grind; it is retired
/// with that rule and never reused.
pub const PALW_EXEC_SEED_DOMAIN: &[u8] = b"misaka-palw/exec-lane/span-seed/v2";
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
/// `credits` is each domain's finalized compute in the closed span ([`palw_execution_credit_v1`],
/// summed). A domain with no credits has no quota and is not listed. Among the rest:
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

/// **A finalized attempt's credit: the canonical compute it certified** — the operator's
/// "Credit 量 = canonical compute", so a class whose inference costs more earns its domain more of
/// the lane per `Final`, and a light class cannot out-schedule a heavy one by finalizing more often.
///
/// `exposure_pwu` is the claim's `palw_exposure_pwu_v1` — one canonical inference of its class, the
/// number its exposure and its ADR-0124 price are read on — and `unit` is ADR-0124 Decision 6's: the
/// dearest exposure among the `Active`, weight-bearing model classes, zero where none bears weight.
/// Capped at the unit, so a class that bears no weight cannot buy lane share by declaring a dear
/// inference, as it cannot buy pay; uncapped where no class sets a unit, so a span's credits are
/// always pwu and never a mixture of pwu and counts. The liveness floor is not special-cased: its
/// credit is its own inference, which is what a floor attempt certified.
pub fn palw_execution_credit_v1(exposure_pwu: u64, unit: u64) -> u64 {
    if unit == 0 { exposure_pwu } else { exposure_pwu.min(unit) }
}

/// One attempt claim that reached `Final` inside a closed scheduler span, as the schedule reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecFinalV1 {
    pub domain: Hash64,
    pub bond: PalwBondKeyV2,
    pub operator_id: Hash64,
    pub claim_id: Hash64,
    pub execution_root: Hash64,
    /// [`palw_execution_credit_v1`], fixed by the block that finalized the claim.
    pub credit: u64,
}

/// A bond that may hold a permit in a span, and how many finalized attempts earned it the place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecBondV1 {
    pub bond: PalwBondKeyV2,
    pub operator_id: Hash64,
    /// The compute its finalized attempts credited in the closed span.
    pub credits: u64,
}

/// One security domain of a span's schedule.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecDomainV1 {
    pub domain: Hash64,
    /// The compute every final of the domain credited — the quota's input, whether or not each
    /// earner is still listed below.
    pub credits: u64,
    pub quota_permille: u16,
    pub parity: u8,
    /// Sorted by bond key. Only bonds whose operator's parity is this domain's
    /// ([`palw_execution_operator_parities_v1`]), so a domain can be listed with none: it keeps its
    /// quota and parity and holds no permits.
    pub bonds: Vec<PalwExecBondV1>,
}

/// **A scheduler span's schedule** — everything a round's draw reads: a snapshot fixed a span
/// earlier and the seed its span opened with. Constant for the span.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecScheduleV1 {
    /// The span this schedule governs (`daa_score / span_daa`).
    pub span_index: u64,
    /// [`palw_execution_span_seed_v1`]: the seed anchor's execution, the span and the safe frontier
    /// at the block that opened the span — none of which existed, or could be chosen, when the
    /// participants below were fixed.
    pub seed: Hash64,
    /// Sorted by domain id.
    pub domains: Vec<PalwExecDomainV1>,
    /// The Finals that earned this schedule, in claim-id order. The seed mints execution quanta
    /// from these rather than from the aggregated domain credits.
    pub finals: Vec<PalwExecFinalV1>,
    /// Issued execution quanta. Empty means the ADR-0125 lottery still draws this span.
    pub quanta: Vec<crate::palw_execution_quanta_v1::PalwExecQuantumV1>,
}

impl PalwExecScheduleV1 {
    pub fn domain(&self, domain: &Hash64) -> Option<&PalwExecDomainV1> {
        self.domains.iter().find(|d| d.domain == *domain)
    }
}

/// **ADR-0130: a schedule's participants, fixed before its seed exists** — everything a
/// [`PalwExecScheduleV1`] holds but the seed. Taken at the first chain block of the span after the
/// one whose finals it lists, and seeded (or dropped) at the first chain block of `target_span`.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecSnapshotV1 {
    /// The span whose schedule this snapshot becomes: two after the span its finals were gathered in.
    pub target_span: u64,
    /// Sorted by domain id, exactly as the schedule will list them.
    pub domains: Vec<PalwExecDomainV1>,
    /// The Finals this snapshot was taken from, claim-id order, duplicates dropped. Trailing so a
    /// carriage written before execution quanta still decodes: empty maps stay empty, and a live
    /// snapshot of `(target_span, domains)` is the prefix of this layout.
    pub finals: Vec<PalwExecFinalV1>,
}

/// **ADR-0130: what seeds the span two after the one that recorded it** — the latest chain block of
/// a span that carried an admitted attempt, and that attempt's execution.
///
/// Only [`Self::execution_key`] enters the seed. It is the attempt's `execution_commitment_v3`
/// under its header's anchor — what the attempt's lottery tickets are drawn from (ADR-0072) — so a
/// producer that wants another value needs another execution that wins its draw. The block's hash
/// is recorded to name the anchor and never hashed into the seed: every nonce of the header's bucket
/// and every timestamp yields the same ticket and a different hash, so the hash is a value its
/// producer re-rolls for free once it holds a winning draw (ADR-0125 §2: a header hash is not seed
/// material). A heartbeat or receipt block carries no attempt and is never an anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecSeedAnchorV1 {
    /// The span of the chain block that recorded it.
    pub span: u64,
    /// That chain block.
    pub block: Hash64,
    /// Its attempt's execution commitment.
    pub execution_key: Hash64,
}

/// **ADR-0130: one parity per operator.** Each operator listed in `domains` takes the parity of the
/// domain where its bonds' credits sum highest, ties to the lower domain id. Listing an operator's
/// bonds only in domains of that parity is what keeps it out of two consecutive rounds of a span,
/// whatever the width: a round's permits come only from domains of the round's parity. Not
/// recursive — quotas and domain parities are read as given and never recomputed for what leaves.
/// Returned by operator id.
///
/// **An operator here is the registry's `operator_id`**, so a party that bonds under two operator
/// keys is two operators and takes a parity each — the same residual ADR-0125's "one permit an
/// operator a round" already carries, and the same place to close it if it ever needs closing.
pub fn palw_execution_operator_parities_v1(domains: &[PalwExecDomainV1]) -> BTreeMap<Hash64, u8> {
    let mut credit_by_operator: BTreeMap<Hash64, BTreeMap<Hash64, (u64, u8)>> = BTreeMap::new();
    for domain in domains {
        for bond in &domain.bonds {
            let row = credit_by_operator.entry(bond.operator_id).or_default().entry(domain.domain).or_insert((0, domain.parity));
            row.0 = row.0.saturating_add(bond.credits);
        }
    }
    credit_by_operator
        .into_iter()
        .filter_map(|(operator, by_domain)| {
            // The most credit; among equals the lower domain id compares greater, so `max_by` takes it.
            by_domain.iter().max_by(|a, b| a.1.0.cmp(&b.1.0).then(b.0.cmp(a.0))).map(|(_, (_, parity))| (operator, *parity))
        })
        .collect()
}

/// **ADR-0130: a span's participants from the attempt claims finalized two spans before it.**
///
/// Credits and bonds per domain are the compute each final certified, summed; a domain whose finals
/// credited nothing earns no quota and is not listed; at most [`PALW_EXEC_MAX_DOMAINS_V1`] domains
/// are listed, those with the most credits (ties to the lower id). Quotas and domain parities follow
/// from the listed domains' credits. Then each operator is given one parity over every bond it has
/// in a listed domain ([`palw_execution_operator_parities_v1`]), its bonds leave the domains of the
/// other parity, and each domain lists at most [`PALW_EXEC_MAX_BONDS_PER_DOMAIN_V1`] of those that
/// remain — the most productive, ties to the lower bond key. Nothing here reads a claim id or an
/// execution root beyond de-duplicating finals: the seed is not the finals' to choose.
pub fn palw_execution_schedule_snapshot_v1(target_span: u64, finals: &[PalwExecFinalV1]) -> PalwExecSnapshotV1 {
    let mut ordered: Vec<&PalwExecFinalV1> = finals.iter().collect();
    ordered.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
    ordered.dedup_by(|a, b| a.claim_id == b.claim_id);

    let mut per_domain: BTreeMap<Hash64, (u64, BTreeMap<PalwBondKeyV2, (Hash64, u64)>)> = BTreeMap::new();
    for f in &ordered {
        let entry = per_domain.entry(f.domain).or_default();
        entry.0 = entry.0.saturating_add(f.credit);
        let bond = entry.1.entry(f.bond).or_insert((f.operator_id, 0));
        bond.1 = bond.1.saturating_add(f.credit);
    }
    let mut listed: Vec<(Hash64, u64, BTreeMap<PalwBondKeyV2, (Hash64, u64)>)> = per_domain
        .into_iter()
        .filter(|(_, (credits, _))| *credits > 0)
        .map(|(domain, (credits, bonds))| (domain, credits, bonds))
        .collect();
    listed.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    listed.truncate(PALW_EXEC_MAX_DOMAINS_V1);
    listed.sort_by(|a, b| a.0.cmp(&b.0));

    let credits: Vec<(Hash64, u64)> = listed.iter().map(|(d, c, _)| (*d, *c)).collect();
    let quotas = palw_execution_quotas_v1(&credits);
    let parities = palw_execution_parities_v1(&quotas);
    let mut domains: Vec<PalwExecDomainV1> = listed
        .into_iter()
        .map(|(domain, credits, bonds)| {
            let quota_permille = quotas.iter().find(|(d, _)| *d == domain).map(|(_, q)| *q).unwrap_or(0);
            let parity = parities.iter().find(|(d, _)| *d == domain).map(|(_, p)| *p).unwrap_or(0);
            let bonds =
                bonds.into_iter().map(|(bond, (operator_id, credits))| PalwExecBondV1 { bond, operator_id, credits }).collect();
            PalwExecDomainV1 { domain, credits, quota_permille, parity, bonds }
        })
        .collect();
    // Every earner is still listed here, so an operator's parity is read over all of its compute.
    let operator_parities = palw_execution_operator_parities_v1(&domains);
    for domain in &mut domains {
        let parity = domain.parity;
        domain.bonds.retain(|b| operator_parities.get(&b.operator_id) == Some(&parity));
        domain.bonds.sort_by(|a, b| b.credits.cmp(&a.credits).then(a.bond.cmp(&b.bond)));
        domain.bonds.truncate(PALW_EXEC_MAX_BONDS_PER_DOMAIN_V1);
        domain.bonds.sort_by(|a, b| a.bond.cmp(&b.bond));
    }
    PalwExecSnapshotV1 { target_span, domains, finals: ordered.into_iter().copied().collect() }
}

/// **ADR-0130: a span's seed** — `H(domain ‖ anchor's execution key ‖ span ‖ frontier blue score ‖
/// frontier)`, the frontier being the state's safe frontier at the block that opens the span. The
/// anchor was recorded after the span's participants were fixed, and moving it costs an execution
/// that wins its draw ([`PalwExecSeedAnchorV1`]); the frontier moves only as claims reach `Final`.
/// No claim id, execution root or bond of the snapshot enters it.
pub fn palw_execution_span_seed_v1(anchor: &PalwExecSeedAnchorV1, span: u64, frontier_blue_score: u64, frontier: Hash64) -> Hash64 {
    let mut seed = keyed(PALW_EXEC_SEED_DOMAIN);
    seed.update(anchor.execution_key.as_byte_slice());
    seed.update(&span.to_le_bytes());
    seed.update(&frontier_blue_score.to_le_bytes());
    seed.update(frontier.as_byte_slice());
    finish(seed)
}

/// **ADR-0130: a snapshot becomes its span's schedule** under the anchor recorded in the span before
/// and the frontier the span opens with.
pub fn palw_execution_schedule_seeded_v1(
    snapshot: &PalwExecSnapshotV1,
    anchor: &PalwExecSeedAnchorV1,
    frontier_blue_score: u64,
    frontier: Hash64,
) -> PalwExecScheduleV1 {
    PalwExecScheduleV1 {
        span_index: snapshot.target_span,
        seed: palw_execution_span_seed_v1(anchor, snapshot.target_span, frontier_blue_score, frontier),
        domains: snapshot.domains.clone(),
        finals: snapshot.finals.clone(),
        quanta: Vec::new(),
    }
}

/// Assign execution quanta onto an already-seeded schedule. `quantum == 0` leaves the lottery in
/// force (empty `quanta`). A non-zero unit mints spend-once tickets from the snapshot's Finals.
pub fn palw_execution_schedule_assign_quanta_v1(schedule: &mut PalwExecScheduleV1, quantum: u64, open_round: u64) {
    palw_execution_schedule_assign_quanta_matured_v1(schedule, quantum, open_round, 0, &std::collections::BTreeSet::new())
}

/// **[`palw_execution_schedule_assign_quanta_v1`] under ADR-0151's bundle.** The tickets mature before
/// they may be spent and a convicted Final mints none; with a zero maturity and an empty forfeiture
/// set it is the function above, ticket for ticket.
pub fn palw_execution_schedule_assign_quanta_matured_v1(
    schedule: &mut PalwExecScheduleV1,
    quantum: u64,
    open_round: u64,
    maturity_rounds: u64,
    forfeited_roots: &std::collections::BTreeSet<crate::Hash64>,
) {
    if quantum == 0 {
        schedule.quanta.clear();
        return;
    }
    schedule.quanta = crate::palw_execution_quanta_v1::palw_execution_mint_quanta_matured_v1(
        &schedule.finals,
        schedule.seed,
        u128::from(quantum),
        open_round,
        maturity_rounds,
        forfeited_roots,
    );
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
    /// **ADR-0151's lineage: the execution quantum this permit came from, and through it the Final
    /// and the `execution_root`.**
    ///
    /// `quantum_id = H(canonical_work_id(execution_root) ‖ final_id ‖ index)`, so a permit is
    /// traceable to the work that earned it without a second index. Zero for a permit the DOMAIN
    /// lottery handed out — that path is not earned by a Final and has no Final to forfeit against,
    /// which is a fact worth being able to read off the permit rather than infer from the schedule.
    ///
    /// Recorded because a right you cannot trace is a right you cannot revoke: the forfeiture in
    /// `palw_execution_mint_quanta_matured_v1` drops a convicted Final's UNUSED tickets, and this is
    /// what lets a reader say which permits a conviction should have reached.
    pub quantum_id: Hash64,
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
///
/// A schedule lists each operator's bonds in domains of one parity only
/// ([`palw_execution_schedule_snapshot_v1`]), so no operator holds permits in two consecutive rounds
/// of one span, and a round whose parity lists no bond has no permits — the round is missed, never
/// handed to the operator of the round before. **The residual:** a round block is judged by the
/// schedule of its anchor's span, so consecutive rounds around a span boundary can be judged by two
/// schedules, and nothing relates one span's operator parities to the next's — an operator can hold
/// the last round under one span and the next round under the other.
pub fn palw_execution_permits_v1(schedule: &PalwExecScheduleV1, round: u64, width: u16) -> Vec<PalwExecPermitV1> {
    if !schedule.quanta.is_empty() {
        return crate::palw_execution_quanta_v1::palw_execution_quantum_for_round_v1(&schedule.quanta, round)
            .into_iter()
            .take(width.min(PALW_EXEC_MAX_PERMITS_PER_ROUND_V1) as usize)
            .map(|q| PalwExecPermitV1 { index: 0, domain: q.domain, bond: q.bond, operator_id: q.operator_id, quantum_id: q.quantum_id })
            .collect();
    }
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
        // The lottery path earns no Final, so there is no quantum to name.
        permits.push(PalwExecPermitV1 {
            index,
            domain: domain.domain,
            bond: bond.bond,
            operator_id: bond.operator_id,
            quantum_id: Hash64::default(),
        });
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

/// **ADR-0125 §7.4: the lane as the sink's state holds it** — what an operator or an explorer asks
/// of a node: the round's permits, the span's schedule, how many permits the span has accepted, and
/// the finals recorded toward the schedule two spans on (ADR-0130). A pending snapshot is not a
/// schedule and is not reported as one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwExecLaneStatusV1 {
    pub view: PalwExecRoundViewV1,
    /// The schedule the view's permits are drawn from, where the span has one.
    pub schedule: Option<PalwExecScheduleV1>,
    /// Permits accepted on the sink's chain in the view's span.
    pub accepted_in_span: u64,
    /// The span whose finals are being recorded, and how many it holds. They become the snapshot of
    /// span `finals_span + 2` at the first chain block of the next span, and its schedule only if a
    /// seed anchor is recorded in between (ADR-0130).
    pub finals_span: u64,
    pub finals: u64,
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

/// **ADR-0125 §7.3: one signed round block, as equivocation evidence carries it** — the header
/// facts the permit signature covers besides the permit itself, and the signature.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecSignedRoundV1 {
    pub pre_pow_hash: Hash64,
    pub timestamp_ms: u64,
    pub nonce: u64,
    pub signature: Vec<u8>,
}

/// The evidence format this build reads.
pub const PALW_EXEC_EQUIVOCATION_VERSION_V1: u8 = 1;

/// **ADR-0125 §7.3: two round blocks signed for one permit.** A permit is one block: the holder's
/// key signs `(network, pre-PoW hash, timestamp, nonce, round, index, bond)`, and two signatures
/// over two different such messages for one `(round, index, bond)` are two blocks for one permit —
/// something only the holder can make, since nobody else holds the key. The chain burns the permit
/// and slashes the bond once for it (`PalwConsensusObjectV2::RoundPermitEquivocated`).
///
/// `span` names the schedule that granted the permit: rounds are clock seconds and spans are DAA
/// ranges, so a round near a span boundary could be granted by either span's draw, and the chain
/// checks the grant against the span the evidence names.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecEquivocationV1 {
    pub version: u8,
    pub span: u64,
    pub round: u64,
    pub permit_index: u16,
    pub bond: PalwBondKeyV2,
    pub first: PalwExecSignedRoundV1,
    pub second: PalwExecSignedRoundV1,
}

/// Why equivocation evidence was refused.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwExecEquivocationError {
    #[error("equivocation evidence version {0}, expected {PALW_EXEC_EQUIVOCATION_VERSION_V1}")]
    Version(u8),
    #[error("permit index {0} is above the widest round")]
    PermitIndex(u16),
    #[error("both sides sign the same block — one block signed twice is not two blocks")]
    SameBlock,
    #[error("a side carries a signature of {got} bytes, not an ML-DSA-87 signature")]
    SignatureLength { got: usize },
    #[error("a side's timestamp falls in round {actual}, not the evidence's round {declared}")]
    RoundMismatch { declared: u64, actual: u64 },
    #[error("a side's signature does not verify under the bond's registered key")]
    NotSigned,
}

impl PalwExecEquivocationV1 {
    /// What the evidence says without the chain: its format, a permit index a round can hold, two
    /// different signed blocks, and two signatures of the right length.
    pub fn validate_shape(&self) -> Result<(), PalwExecEquivocationError> {
        if self.version != PALW_EXEC_EQUIVOCATION_VERSION_V1 {
            return Err(PalwExecEquivocationError::Version(self.version));
        }
        if self.permit_index >= PALW_EXEC_MAX_PERMITS_PER_ROUND_V1 {
            return Err(PalwExecEquivocationError::PermitIndex(self.permit_index));
        }
        let facts = |side: &PalwExecSignedRoundV1| (side.pre_pow_hash, side.timestamp_ms, side.nonce);
        if facts(&self.first) == facts(&self.second) {
            return Err(PalwExecEquivocationError::SameBlock);
        }
        for side in [&self.first, &self.second] {
            if side.signature.len() != PALW_EXEC_MLDSA87_SIGNATURE_LEN {
                return Err(PalwExecEquivocationError::SignatureLength { got: side.signature.len() });
            }
        }
        Ok(())
    }

    /// The message each side's signature covers.
    pub fn messages(&self, network_domain: Hash64) -> [Hash64; 2] {
        let message = |side: &PalwExecSignedRoundV1| {
            palw_exec_signing_message_v1(
                network_domain,
                side.pre_pow_hash,
                side.timestamp_ms,
                side.nonce,
                self.round,
                self.permit_index,
                &self.bond,
            )
        };
        [message(&self.first), message(&self.second)]
    }

    /// **Everything the evidence proves by itself, against the bond's registered key**: the shape,
    /// both timestamps in the evidence's round, and both signatures under the key. Whether the
    /// named span granted that permit to that bond, and whether the chain already burned it, are
    /// the chain's questions.
    pub fn verify(
        &self,
        network_domain: Hash64,
        genesis_timestamp_ms: u64,
        pubkey: &[u8],
        verify: impl Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    ) -> Result<(), PalwExecEquivocationError> {
        self.validate_shape()?;
        for side in [&self.first, &self.second] {
            let actual = palw_execution_round_v1(side.timestamp_ms, genesis_timestamp_ms);
            if actual != self.round {
                return Err(PalwExecEquivocationError::RoundMismatch { declared: self.round, actual });
            }
        }
        let messages = self.messages(network_domain);
        for (side, message) in [&self.first, &self.second].into_iter().zip(messages.iter()) {
            if !verify(pubkey, message.as_byte_slice(), &side.signature, PALW_EXEC_MLDSA87_CONTEXT) {
                return Err(PalwExecEquivocationError::NotSigned);
            }
        }
        Ok(())
    }
}

impl PalwExecEnvelopeV1 {
    /// The side of an equivocation this header and its envelope are.
    pub fn signed_round_v1(&self, pre_pow_hash: Hash64, timestamp_ms: u64, nonce: u64) -> PalwExecSignedRoundV1 {
        PalwExecSignedRoundV1 { pre_pow_hash, timestamp_ms, nonce, signature: self.signature.clone() }
    }
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
        let message =
            palw_exec_signing_message_v1(network_domain, pre_pow_hash, timestamp_ms, nonce, self.round, self.permit_index, &self.bond);
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
    use crate::palw_execution_quanta_v1::palw_execution_quantum_for_round_v1;
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn bond(v: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(v), 0))
    }

    /// A final crediting one unit of compute, so a domain's credits are its count of finals.
    fn final_of(domain: u64, bond_word: u64, operator: u64, claim: u64) -> PalwExecFinalV1 {
        final_credited(domain, bond_word, operator, claim, 1)
    }

    fn final_credited(domain: u64, bond_word: u64, operator: u64, claim: u64, credit: u64) -> PalwExecFinalV1 {
        PalwExecFinalV1 {
            domain: h(domain),
            bond: bond(bond_word),
            operator_id: h(operator),
            claim_id: h(claim),
            execution_root: h(claim + 1_000_000),
            credit,
        }
    }

    /// **ADR-0125 §7.3: evidence proves two blocks under one permit, signed by the key it is checked
    /// against, in the round it names, on the network it is checked on — and nothing less.**
    #[test]
    fn equivocation_evidence_proves_two_blocks_under_one_permit_and_nothing_else() {
        const GENESIS_TS: u64 = 1_000_000;
        let round = 42;
        let ts = GENESIS_TS + round * 1_000 + 17;
        let key = [5u8; 8];
        let domain = h(77);
        // A mock signature: the message itself in the first 64 bytes, checked under one key and the
        // permit context.
        let sign = |message: Hash64| {
            let mut signature = vec![0u8; PALW_EXEC_MLDSA87_SIGNATURE_LEN];
            signature[..64].copy_from_slice(message.as_byte_slice());
            signature
        };
        let verify = |k: &[u8], m: &[u8], s: &[u8], c: &[u8]| k == key && c == PALW_EXEC_MLDSA87_CONTEXT && &s[..64] == m;
        let side = |pre: u64, nonce: u64| PalwExecSignedRoundV1 {
            pre_pow_hash: h(pre),
            timestamp_ms: ts,
            nonce,
            signature: vec![0; PALW_EXEC_MLDSA87_SIGNATURE_LEN],
        };
        let signed = |mut evidence: PalwExecEquivocationV1| {
            let [first, second] = evidence.messages(domain);
            evidence.first.signature = sign(first);
            evidence.second.signature = sign(second);
            evidence
        };
        let evidence = signed(PalwExecEquivocationV1 {
            version: PALW_EXEC_EQUIVOCATION_VERSION_V1,
            span: 3,
            round,
            permit_index: 1,
            bond: bond(9),
            first: side(1, 1),
            second: side(2, 1),
        });
        assert_eq!(evidence.verify(domain, GENESIS_TS, &key, verify), Ok(()), "two headers, one permit, one key");
        let renonced = signed(PalwExecEquivocationV1 { second: side(1, 2), ..evidence.clone() });
        assert_eq!(renonced.verify(domain, GENESIS_TS, &key, verify), Ok(()), "a re-solved nonce is a second block");

        let mut same = evidence.clone();
        same.second = same.first.clone();
        assert_eq!(same.validate_shape(), Err(PalwExecEquivocationError::SameBlock), "one block signed twice is not two");
        assert_eq!(
            evidence.verify(domain, GENESIS_TS, &[6u8; 8], verify),
            Err(PalwExecEquivocationError::NotSigned),
            "a stranger's key"
        );
        assert_eq!(evidence.verify(h(78), GENESIS_TS, &key, verify), Err(PalwExecEquivocationError::NotSigned), "another network");
        let other_bond = PalwExecEquivocationV1 { bond: bond(10), ..evidence.clone() };
        assert_eq!(other_bond.verify(domain, GENESIS_TS, &key, verify), Err(PalwExecEquivocationError::NotSigned), "another bond");
        let mut late = evidence.clone();
        late.second.timestamp_ms = ts + 1_000;
        assert_eq!(
            late.verify(domain, GENESIS_TS, &key, verify),
            Err(PalwExecEquivocationError::RoundMismatch { declared: round, actual: round + 1 }),
            "a block of the next round is that round's"
        );
        assert_eq!(
            PalwExecEquivocationV1 { version: 2, ..evidence.clone() }.validate_shape(),
            Err(PalwExecEquivocationError::Version(2))
        );
        assert_eq!(
            PalwExecEquivocationV1 { permit_index: PALW_EXEC_MAX_PERMITS_PER_ROUND_V1, ..evidence.clone() }.validate_shape(),
            Err(PalwExecEquivocationError::PermitIndex(PALW_EXEC_MAX_PERMITS_PER_ROUND_V1))
        );
        let mut short = evidence.clone();
        short.first.signature.pop();
        assert!(matches!(short.validate_shape(), Err(PalwExecEquivocationError::SignatureLength { .. })));
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

    /// A fixed anchor: a test's schedule is then a function of its finals and its span.
    fn anchor(key: u64) -> PalwExecSeedAnchorV1 {
        PalwExecSeedAnchorV1 { span: 0, block: h(key.wrapping_add(7_000_000)), execution_key: h(key) }
    }

    /// The schedule `finals` earn for `span`, seeded under a fixed anchor and frontier.
    fn schedule_of(span: u64, finals: &[PalwExecFinalV1]) -> PalwExecScheduleV1 {
        palw_execution_schedule_seeded_v1(&palw_execution_schedule_snapshot_v1(span, finals), &anchor(1), 0, h(0))
    }

    #[test]
    fn the_schedule_counts_credits_and_lists_earners() {
        let finals = vec![
            final_of(1, 10, 100, 1),
            final_of(1, 10, 100, 2),
            final_of(1, 11, 101, 3),
            final_of(2, 20, 200, 4),
            final_of(3, 30, 300, 5),
        ];
        let schedule = schedule_of(7, &finals);
        assert_eq!(schedule.span_index, 7);
        assert_eq!(schedule.domains.iter().map(|d| d.domain).collect::<Vec<_>>(), vec![h(1), h(2), h(3)], "sorted by domain id");
        let d1 = schedule.domain(&h(1)).unwrap();
        assert_eq!(d1.credits, 3);
        assert_eq!(d1.bonds.iter().map(|b| (b.bond, b.credits)).collect::<Vec<_>>(), vec![(bond(10), 2), (bond(11), 1)]);
        assert_eq!(schedule.domains.iter().map(|d| d.quota_permille as u32).sum::<u32>(), 1000);
        assert!(schedule.domains.iter().all(|d| (d.quota_permille as u64) <= PALW_EXEC_DOMAIN_CAP_PERMILLE));
        // Order-independent and duplicate-proof.
        let mut shuffled = finals.clone();
        shuffled.reverse();
        shuffled.push(finals[0]);
        assert_eq!(schedule_of(7, &shuffled), schedule);
        // An empty span has no domains, and so no permits.
        let empty = schedule_of(7, &[]);
        assert!(empty.domains.is_empty());
        assert!(palw_execution_permits_v1(&empty, 3, 10).is_empty());
    }

    /// **ADR-0130: the seed is the anchor's execution, the span and the frontier — and nothing the
    /// finals choose.** Each of the four inputs moves it; the anchor block's hash (which its producer
    /// re-rolls for free inside one winning draw) and the span that recorded the anchor do not. Finals
    /// recast under other claim ids and execution roots, with the same participants, give the same
    /// snapshot and so the same schedule; and the draw follows the seed.
    #[test]
    fn adr0130_the_seed_moves_with_the_anchor_span_and_frontier_and_nothing_the_finals_choose() {
        let finals = vec![final_of(1, 10, 100, 1), final_of(2, 20, 200, 2), final_of(2, 21, 201, 3)];
        let snapshot = palw_execution_schedule_snapshot_v1(9, &finals);
        let a = PalwExecSeedAnchorV1 { span: 8, block: h(500), execution_key: h(600) };
        let seed = palw_execution_span_seed_v1(&a, 9, 40, h(700));
        assert_eq!(palw_execution_span_seed_v1(&a, 9, 40, h(700)), seed, "deterministic");
        for (moved, what) in [
            (
                palw_execution_span_seed_v1(&PalwExecSeedAnchorV1 { execution_key: h(601), ..a }, 9, 40, h(700)),
                "the anchor's execution",
            ),
            (palw_execution_span_seed_v1(&a, 10, 40, h(700)), "the span"),
            (palw_execution_span_seed_v1(&a, 9, 41, h(700)), "the frontier's blue score"),
            (palw_execution_span_seed_v1(&a, 9, 40, h(701)), "the frontier"),
        ] {
            assert_ne!(moved, seed, "{what} is in the seed");
        }
        assert_eq!(
            palw_execution_span_seed_v1(&PalwExecSeedAnchorV1 { span: 3, block: h(501), ..a }, 9, 40, h(700)),
            seed,
            "the anchor block's hash and its span are not"
        );

        let schedule = palw_execution_schedule_seeded_v1(&snapshot, &a, 40, h(700));
        assert_eq!((schedule.span_index, schedule.seed), (9, seed));
        assert_eq!(schedule.domains, snapshot.domains, "a schedule is its snapshot and a seed");
        let recast: Vec<PalwExecFinalV1> = finals
            .iter()
            .enumerate()
            .map(|(i, f)| PalwExecFinalV1 { claim_id: h(9_000 + i as u64), execution_root: h(42), ..*f })
            .collect();
        let recast_snapshot = palw_execution_schedule_snapshot_v1(9, &recast);
        assert_eq!(recast_snapshot.domains, snapshot.domains, "claim ids and roots do not reach the participants");
        assert_ne!(recast_snapshot.finals, snapshot.finals, "the mint still names the Finals that earned it");
        let recast_schedule = palw_execution_schedule_seeded_v1(&recast_snapshot, &a, 40, h(700));
        assert_eq!(recast_schedule.seed, schedule.seed);
        assert_eq!(recast_schedule.domains, schedule.domains);

        // Domain 2 holds the odd rounds with two operators, so which bond takes a round is the seed's.
        let other = palw_execution_schedule_seeded_v1(&snapshot, &PalwExecSeedAnchorV1 { execution_key: h(601), ..a }, 40, h(700));
        assert!(
            (0..200u64).any(|round| palw_execution_permits_v1(&schedule, round, 1) != palw_execution_permits_v1(&other, round, 1)),
            "the draw follows the seed"
        );
    }

    /// **ADR-0130: one operator, one parity.** An operator that earned in two domains of different
    /// parities is listed only in the domain where it earned more; the other keeps its row, its quota
    /// and its parity and holds no permits — so every round of that parity is missed, explicitly,
    /// rather than handed to the operator that held the round before. A tie goes to the lower domain
    /// id, one bond certified in both domains is one operator, and another operator in the other
    /// domain holds that parity's rounds alone.
    #[test]
    fn adr0130_an_operator_spread_over_both_parities_misses_one_paritys_rounds() {
        let finals = vec![final_credited(1, 10, 100, 1, 5), final_credited(2, 11, 100, 2, 3)];
        let schedule = schedule_of(4, &finals);
        let (d1, d2) = (schedule.domain(&h(1)).unwrap(), schedule.domain(&h(2)).unwrap());
        assert_eq!((d1.quota_permille, d2.quota_permille), (500, 500), "two domains split the lane");
        assert_ne!(d1.parity, d2.parity);
        let every_earner = [(h(1), d1.parity, bond(10), 5u64), (h(2), d2.parity, bond(11), 3)]
            .map(|(domain, parity, bond, credits)| PalwExecDomainV1 {
                domain,
                credits,
                quota_permille: 500,
                parity,
                bonds: vec![PalwExecBondV1 { bond, operator_id: h(100), credits }],
            })
            .to_vec();
        assert_eq!(palw_execution_operator_parities_v1(&every_earner), BTreeMap::from([(h(100), d1.parity)]), "where it earned more");
        assert_eq!(d1.bonds.iter().map(|b| b.bond).collect::<Vec<_>>(), vec![bond(10)]);
        assert!(d2.bonds.is_empty(), "the operator's bond left the domain of the other parity");
        for width in [1u16, 2, 5, 10] {
            for round in 0..60u64 {
                let permits = palw_execution_permits_v1(&schedule, round, width);
                if round % 2 == d1.parity as u64 {
                    assert_eq!(permits.iter().map(|p| p.bond).collect::<Vec<_>>(), vec![bond(10)], "width {width}, round {round}");
                } else {
                    assert!(permits.is_empty(), "width {width}, round {round}: a round of the other parity is missed");
                }
            }
        }

        let tied = vec![final_credited(1, 10, 100, 1, 2), final_credited(2, 11, 100, 2, 2)];
        let schedule = schedule_of(4, &tied);
        assert_eq!(schedule.domain(&h(1)).unwrap().bonds.len(), 1);
        assert!(schedule.domain(&h(2)).unwrap().bonds.is_empty(), "a tie goes to the lower domain id");

        let one_bond_two_classes = vec![final_credited(1, 10, 100, 1, 1), final_credited(2, 10, 100, 2, 4)];
        let schedule = schedule_of(4, &one_bond_two_classes);
        assert!(schedule.domain(&h(1)).unwrap().bonds.is_empty());
        assert_eq!(schedule.domain(&h(2)).unwrap().bonds.len(), 1, "one bond in two domains is listed where it earned more");

        let mut shared = finals.clone();
        shared.push(final_credited(2, 20, 200, 3, 1));
        let schedule = schedule_of(4, &shared);
        let (d1, d2) = (schedule.domain(&h(1)).unwrap(), schedule.domain(&h(2)).unwrap());
        assert_eq!(d2.bonds.iter().map(|b| b.operator_id).collect::<Vec<_>>(), vec![h(200)]);
        for round in 0..60u64 {
            let holders: Vec<Hash64> = palw_execution_permits_v1(&schedule, round, 1).iter().map(|p| p.operator_id).collect();
            let expected = if round % 2 == d1.parity as u64 { h(100) } else { h(200) };
            assert_eq!(holders, vec![expected], "round {round}");
        }
    }

    /// splitmix64 — a fixed stream for the randomized censuses below.
    fn mix(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// **ADR-0130 over randomized censuses: no operator holds permits in two consecutive rounds of a
    /// span, at any width.** Sixty censuses of up to seven domains and twelve operators; every third
    /// operator earns in every domain, every fifth certifies one bond in all of its domains, the rest
    /// earn in a random few. In each, every operator is listed in one parity only, and over 200
    /// rounds at widths 1, 2, 5 and 10 no operator and no domain holds permits in two rounds running
    /// and no operator holds two permits of one round.
    #[test]
    fn adr0130_no_operator_holds_permits_in_two_consecutive_rounds_of_a_span() {
        let mut rng = 0x0130_u64;
        let mut permits_seen = 0usize;
        for census in 0..60u64 {
            let domain_count = 1 + mix(&mut rng) % 7;
            let operator_count = 1 + mix(&mut rng) % 12;
            let mut finals = Vec::new();
            let mut claim = 0u64;
            for operator in 1..=operator_count {
                let everywhere = operator % 3 == 0;
                let one_bond = operator % 5 == 0;
                for domain in 1..=domain_count {
                    if !everywhere && mix(&mut rng).is_multiple_of(2) {
                        continue;
                    }
                    for b in 0..(1 + mix(&mut rng) % 2) {
                        let bond_word = if one_bond { operator * 1_000 } else { operator * 1_000 + domain * 10 + b };
                        for _ in 0..(1 + mix(&mut rng) % 3) {
                            claim += 1;
                            finals.push(final_credited(domain, bond_word, operator, claim, 1 + mix(&mut rng) % 50));
                        }
                    }
                }
            }
            let snapshot = palw_execution_schedule_snapshot_v1(census, &finals);
            let schedule = palw_execution_schedule_seeded_v1(&snapshot, &anchor(mix(&mut rng)), census, h(census));
            let mut parity_of: BTreeMap<Hash64, u8> = BTreeMap::new();
            for d in &schedule.domains {
                for b in &d.bonds {
                    assert_eq!(
                        *parity_of.entry(b.operator_id).or_insert(d.parity),
                        d.parity,
                        "census {census}: an operator in both parities"
                    );
                }
            }
            for width in [1u16, 2, 5, 10] {
                let (mut previous_operators, mut previous_domains) = (BTreeSet::new(), BTreeSet::new());
                for round in 0..200u64 {
                    let permits = palw_execution_permits_v1(&schedule, round, width);
                    permits_seen += permits.len();
                    let operators: BTreeSet<Hash64> = permits.iter().map(|p| p.operator_id).collect();
                    let domains: BTreeSet<Hash64> = permits.iter().map(|p| p.domain).collect();
                    assert_eq!(
                        operators.len(),
                        permits.len(),
                        "census {census}, width {width}, round {round}: one permit an operator"
                    );
                    assert!(
                        operators.is_disjoint(&previous_operators),
                        "census {census}, width {width}, round {round}: an operator held permits in two rounds running"
                    );
                    assert!(
                        domains.is_disjoint(&previous_domains),
                        "census {census}, width {width}, round {round}: a domain held permits in two rounds running"
                    );
                    (previous_operators, previous_domains) = (operators, domains);
                }
            }
        }
        assert!(permits_seen > 10_000, "the censuses exercised the draw ({permits_seen} permits)");
    }

    #[test]
    fn the_listing_caps_keep_the_most_productive() {
        let mut finals = Vec::new();
        for b in 0..(PALW_EXEC_MAX_BONDS_PER_DOMAIN_V1 as u64 + 5) {
            for c in 0..=(b % 3) {
                finals.push(final_of(1, 1_000 + b, 5_000 + b, 10_000 * (b + 1) + c));
            }
        }
        let schedule = schedule_of(0, &finals);
        let d = schedule.domain(&h(1)).unwrap();
        assert_eq!(d.bonds.len(), PALW_EXEC_MAX_BONDS_PER_DOMAIN_V1);
        assert!(d.bonds.windows(2).all(|w| w[0].bond < w[1].bond), "stored in bond order");
        let productive = (0..(PALW_EXEC_MAX_BONDS_PER_DOMAIN_V1 as u64 + 5)).filter(|b| b % 3 != 0).count();
        assert_eq!(
            d.bonds.iter().filter(|b| b.credits >= 2).count(),
            productive,
            "every bond with more than one final is listed; only one-final bonds are dropped"
        );
    }

    /// **The operator's "Credit 量 = canonical compute": a domain's quota follows the compute its
    /// finals certified, not how many there were.** testnet-11's three classes in one span — ten
    /// finals of `PALW-QWEN36` (2,685,360 pwu an inference, the unit), twenty of `PALW-QWEN25-A16`
    /// (1,589,424) and a hundred of the floor (7,708). Counted, the floor would take the cap and the
    /// heaviest model the least (183 / 367 / 450); credited by compute, the two models hold the cap
    /// and the floor what is left (450 / 450 / 100).
    #[test]
    fn a_domains_quota_follows_the_compute_its_finals_certified_not_their_count() {
        const QWEN36: u64 = 2_685_360;
        const A16: u64 = 1_589_424;
        const FLOOR: u64 = 7_708;
        let span = |credit_of: &dyn Fn(u64) -> u64| {
            let mut finals = Vec::new();
            let mut claim = 0;
            for (domain, count, pwu) in [(1u64, 10u64, QWEN36), (2, 20, A16), (3, 100, FLOOR)] {
                for i in 0..count {
                    claim += 1;
                    finals.push(final_credited(domain, domain * 1_000 + i, domain * 10_000 + i, claim, credit_of(pwu)));
                }
            }
            schedule_of(3, &finals)
        };
        let quotas = |schedule: &PalwExecScheduleV1| schedule.domains.iter().map(|d| (d.domain, d.quota_permille)).collect::<Vec<_>>();
        assert_eq!(quotas(&span(&|_| 1)), vec![(h(1), 183), (h(2), 367), (h(3), 450)], "counted, the floor out-schedules both models");
        let computed = span(&|pwu| palw_execution_credit_v1(pwu, QWEN36));
        assert_eq!(quotas(&computed), vec![(h(1), 450), (h(2), 450), (h(3), 100)], "credited by compute, the models hold the cap");
        assert_eq!(computed.domain(&h(1)).unwrap().credits, 10 * QWEN36);
        assert_eq!(computed.domain(&h(3)).unwrap().credits, 100 * FLOOR);

        // The unit caps a class that bears no weight, and where no class sets one nothing is capped.
        assert_eq!(palw_execution_credit_v1(9_000_000, QWEN36), QWEN36, "a class above the unit is credited the unit");
        assert_eq!(palw_execution_credit_v1(FLOOR, QWEN36), FLOOR, "the floor is credited its own inference");
        assert_eq!(palw_execution_credit_v1(9_000_000, 0), 9_000_000, "no weight-bearing class, no cap");

        // A domain whose finals credited nothing is not listed, and none of its bonds is drawn.
        let schedule = schedule_of(3, &[final_credited(1, 10, 100, 1, 5), final_credited(2, 20, 200, 2, 0)]);
        assert_eq!(schedule.domains.iter().map(|d| d.domain).collect::<Vec<_>>(), vec![h(1)]);
        assert!((0..8).flat_map(|round| palw_execution_permits_v1(&schedule, round, 10)).all(|permit| permit.bond == bond(10)));
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
        schedule_of(3, &finals)
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
        let schedule = schedule_of(0, &finals);
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

    /// A verified Final mints N spend-once quanta; those, not the credit lottery, pick the rounds.
    #[test]
    fn a_seeded_schedule_mints_execution_quanta_and_each_round_holds_at_most_one_permit() {
        let finals = vec![
            PalwExecFinalV1 { execution_root: h(0xE0), ..final_credited(1, 10, 100, 1, 500_000) },
            PalwExecFinalV1 { execution_root: h(0xE1), ..final_credited(1, 10, 100, 2, 200_000) },
        ];
        let mut schedule = schedule_of(3, &finals);
        assert!(schedule.quanta.is_empty(), "seeded schedules start on the lottery until assigned");
        palw_execution_schedule_assign_quanta_v1(&mut schedule, 100_000, 1_000);
        assert_eq!(schedule.finals.len(), 2);
        assert!(schedule.quanta.len() >= 6, "500k + 200k at 100k is at least six tickets, got {}", schedule.quanta.len());
        let mut rounds = BTreeSet::new();
        for q in &schedule.quanta {
            assert!(rounds.insert(q.scheduled_round));
            let permits = palw_execution_permits_v1(&schedule, q.scheduled_round, 1);
            assert_eq!(permits.len(), 1, "one quantum is one permit");
            assert_eq!(permits[0].bond, q.bond);
            assert_eq!(permits[0].index, 0);
            assert_eq!(palw_execution_permit_of_v1(&schedule, q.scheduled_round, 1, 0, &q.bond).map(|p| p.bond), Some(q.bond));
        }
        let lottery_round =
            (0u64..4_000).find(|r| palw_execution_quantum_for_round_v1(&schedule.quanta, *r).is_none()).expect("a gap");
        assert!(palw_execution_permits_v1(&schedule, lottery_round, 1).is_empty(), "a round with no quantum is missed, not redrawn");
        palw_execution_schedule_assign_quanta_v1(&mut schedule, 0, 1_000);
        assert!(schedule.quanta.is_empty(), "quantum 0 restores the lottery");
    }

    #[test]
    fn heavier_canonical_work_holds_more_future_rounds_than_lighter_work() {
        let light = {
            let mut s = schedule_of(1, &[PalwExecFinalV1 { execution_root: h(0xA), ..final_credited(25, 10, 100, 25, 1_589_424) }]);
            palw_execution_schedule_assign_quanta_v1(&mut s, 100_000, 0);
            s.quanta.len()
        };
        let heavy = {
            let mut s = schedule_of(1, &[PalwExecFinalV1 { execution_root: h(0xB), ..final_credited(36, 10, 100, 36, 2_685_360) }]);
            palw_execution_schedule_assign_quanta_v1(&mut s, 100_000, 0);
            s.quanta.len()
        };
        assert!(heavy > light, "QWEN36-scale work ({heavy}) must out-mint QWEN25-scale work ({light})");
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
