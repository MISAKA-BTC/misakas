//! **RFC09: the remote client's fork choice under ADR-0178 rule E and RFC-0018 Gate C, from the full node's own readers.**
//!
//! Past rule E a header commits the envelope of a rule-E leaf (v2, or v3 with rule E v2 capital and Gate C certification state), not
//! the v1 leaf — so `crate::l2` STOPs (`LeafV1Insufficient`). This module is what replaces that STOP: the client reads the committed
//! leaf of every block on each candidate's path (op 203 v2) and checks it with `palw_verify_committed_leaf_v2` — the same function
//! the node's v39 anchor verifier calls — against the root the next header commits; it reads each tip's v3 rule-E window above the
//! fork point and the fork point's v3 registry openings, verifies them against the verified leaves (`verify_window_suffix_v3`,
//! `palw_rule_e_verify_registry_v3`), and evaluates the pair with the node's own accumulator (`palw_rule_e_pair_from_openings_v3`,
//! Gate C applied from each record's certification state) and order (`palw_rule_e_order_v2`). The remote's choice is the full node's
//! choice; anything missing, unverifiable or tampered is a STOP — the miner signs nothing.
//!
//! Trust: the attested tip's committed root (an issuer the client chose); every other byte is checked against a root the client
//! established. Nothing here proves a computation (PRINCIPLES §8): the leaves and openings are commitments and their openings.
use std::collections::BTreeMap;

use kaspa_consensus_core::config::params::{ForkActivation, Params};
use kaspa_consensus_core::palw_fork_choice_commitment_v1::PalwForkChoicePointV1;
use kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v2::PalwRuleEWindowOpeningV1;
use kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v3::{
    PalwCommitFormV1, PalwForkChoiceLeafV3, PalwRuleERegistryOpeningV3, palw_rule_e_pair_from_openings_v3, palw_rule_e_verify_registry_v3,
    palw_verify_committed_leaf_v2, verify_window_suffix_v3,
};
use kaspa_consensus_core::palw_fork_choice_rule_e_v1::{
    PalwRuleEClaimRecordV1, palw_rule_e_order_v2, palw_rule_e_participation_counts_v1,
};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_hashes::Hash64;

use crate::l2::{ForkChoiceAttestationV1, L2LimitsV1, L2StopV1, PeerViewV1, candidate_chains_v1};
use crate::verify::{ChainPointV1, ClientRulesetV1, VerifiedChainV1};

/// The attestation leaf version of a rule-E v3 post-state (the issuer names the form it attests).
pub const PALW_FORK_CHOICE_LEAF_VERSION_RULE_E_V3: u16 = 3;

/// **One op-203 v2 entry, as served** (untrusted): the committed leaf in its own form and the root it is enveloped with, and — as
/// asked — the v3 window above a blue score and the v3 registry openings of some bonds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommittedServedV2 {
    pub block: Hash64,
    pub leaf: Vec<u8>,
    pub inner_root: Hash64,
    /// `(from_blue_score, opening)`: the window opened above `from_blue_score`.
    pub window: Option<(u64, PalwRuleEWindowOpeningV1)>,
    pub registry: Vec<(PalwBondKeyV2, PalwRuleERegistryOpeningV3)>,
}

/// **The evidence at a tip**: an issuer's attestation of the tip's committed root (rule-E v3 leaf version) — the leaf itself comes
/// from the served entries and must envelope to the attested root.
#[derive(Clone, Debug)]
pub struct RuleEEvidenceV2 {
    pub attestation: ForkChoiceAttestationV1,
}

/// The inputs of one rule-E verification.
pub struct RuleEInputV2<'a> {
    pub views: &'a [PeerViewV1],
    pub evidence: &'a [RuleEEvidenceV2],
    pub served: &'a [CommittedServedV2],
    pub trusted: &'a [(Vec<u8>, Vec<u8>)],
    pub ruleset: &'a ClientRulesetV1,
    /// The client's own consensus parameters (the form each point commits, rule E, rule E v2 participation, finality depth).
    pub params: &'a Params,
    pub limits: &'a L2LimitsV1,
}

/// What the remote established: the chosen chain and the tip's verified committed leaf (its inner root is the ADR-0043 root L3
/// proves state against), and the candidates refused.
#[derive(Clone, Debug)]
pub struct RuleEEstablishedV2 {
    pub chosen: VerifiedChainV1,
    pub attested: Hash64,
    pub inner_root: Hash64,
    pub refused: Vec<Hash64>,
}

fn stop(why: impl Into<String>) -> L2StopV1 {
    L2StopV1::RuleEEvidence(why.into())
}

fn point(p: &ChainPointV1) -> PalwForkChoicePointV1 {
    PalwForkChoicePointV1 { block: p.hash, daa_score: p.daa_score, blue_score: p.blue_score }
}

fn fork_index(c: &VerifiedChainV1, o: &VerifiedChainV1) -> usize {
    (0..c.hashes.len()).rev().find(|&i| o.contains(&c.hashes[i])).unwrap_or(0)
}

fn active(fence: Option<ForkActivation>, daa: u64) -> bool {
    fence.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa))
}

/// **Verify an attestation of a rule-E v3 post-state**: trusted key, this build's ruleset, the v3 leaf version, the signature,
/// fresh against `now_daa` — `crate::l2::verify_attestation_v1`'s checks with the v3 leaf version.
pub fn verify_attestation_rule_e_v2(
    att: &ForkChoiceAttestationV1,
    trusted: &[(Vec<u8>, Vec<u8>)],
    ruleset: &ClientRulesetV1,
    now_daa: u64,
    limits: &L2LimitsV1,
    verify: &dyn Fn(&[u8], &[u8], &[u8]) -> bool,
) -> Result<(), String> {
    if att.leaf_version != PALW_FORK_CHOICE_LEAF_VERSION_RULE_E_V3 {
        return Err(format!("leaf version {} is not the rule-E v3 form", att.leaf_version));
    }
    let as_v1 = ForkChoiceAttestationV1 { leaf_version: kaspa_consensus_core::palw_fork_choice_commitment_v1::PALW_FORK_CHOICE_LEAF_VERSION_V1, ..att.clone() };
    // The signature covers the real leaf version: check it on `att`, and the remaining fields through the shared checker.
    let (_, pubkey) =
        trusted.iter().find(|(id, _)| *id == att.key_id).ok_or_else(|| format!("issuer '{}' is not trusted", att.issuer()))?;
    if !verify(pubkey, att.signing_digest().as_bytes().as_slice(), &att.signature) {
        return Err(format!("issuer '{}''s signature does not verify", att.issuer()));
    }
    crate::l2::verify_attestation_v1(&as_v1, trusted, ruleset, now_daa, limits, &|_, _, _| true)
}

/// The served entry of `block` whose committed leaf opens `committed_root` at `p` in the client's form, and its verified view.
fn opened<'a>(
    params: &Params,
    served: &'a [CommittedServedV2],
    committed_root: &Hash64,
    p: &ChainPointV1,
) -> Result<(&'a CommittedServedV2, PalwCommitFormV1), String> {
    let mut why = None;
    for s in served.iter().filter(|s| s.block == p.hash) {
        match palw_verify_committed_leaf_v2(params, committed_root, &point(p), &s.leaf, &s.inner_root) {
            Ok(view) => return Ok((s, view.form)),
            Err(e) => why = Some(format!("{e:?}")),
        }
    }
    Err(match why {
        Some(e) => format!("the committed leaf served for {} does not open the root {committed_root}: {e}", p.hash),
        None => format!("no committed leaf of {} was served", p.hash),
    })
}

/// **The chain from `chain.points[top]` down to `chain.points[low]`**, each parent's committed leaf verified against the root its
/// child's header commits (a covered header commits its selected parent's post-state). Returns the verified entries of
/// `points[low..top]`, index-aligned from `low`.
pub fn verify_selected_chain_v2<'a>(
    params: &Params,
    chain: &VerifiedChainV1,
    low: usize,
    top: usize,
    served: &'a [CommittedServedV2],
) -> Result<Vec<&'a CommittedServedV2>, String> {
    if low > top || top >= chain.points.len() {
        return Err(format!("no path from index {top} down to {low} on a chain of {} headers", chain.points.len()));
    }
    let mut out = Vec::with_capacity(top - low);
    for k in (low + 1..=top).rev() {
        let (parent, child) = (chain.points[k - 1], chain.points[k]);
        let (s, _) = opened(params, served, &child.palw_state_root, &parent)?;
        out.push(s);
    }
    out.reverse();
    Ok(out)
}

fn v3(entry: &CommittedServedV2) -> Result<PalwForkChoiceLeafV3, String> {
    PalwForkChoiceLeafV3::decode(&entry.leaf).map_err(|e| format!("the verified leaf of {} is not v3: {e:?}", entry.block))
}

/// **One pair under rule E, from verified openings**: `Greater` — `a` replaces `b`; `Less` — `b` stays; `Equal` resolved by the
/// node's own tie rule where it is a GHOSTDAG question (the heavier tip by blue work), `None` where the node's answer would depend on
/// which tip is its incumbent (no robust answer).
#[allow(clippy::too_many_arguments)]
fn pair_v2(
    params: &Params,
    a: &VerifiedChainV1,
    a_tip: &PalwForkChoiceLeafV3,
    a_window: &[PalwRuleEClaimRecordV1],
    b: &VerifiedChainV1,
    b_window: &[PalwRuleEClaimRecordV1],
    fork: &ChainPointV1,
    fork_leaf: &PalwForkChoiceLeafV3,
    fork_capital: &BTreeMap<PalwBondKeyV2, u64>,
) -> Result<Option<std::cmp::Ordering>, String> {
    use std::cmp::Ordering;
    let _ = a_tip;
    let pair = palw_rule_e_pair_from_openings_v3(a_window, b_window, |bond| fork_capital.get(bond).copied(), fork_leaf)
        .map_err(|e| format!("rule E's pair does not evaluate: {e:?}"))?;
    let lower_span = a.tip.daa_score.min(b.tip.daa_score).saturating_sub(fork.daa_score);
    let counts = palw_rule_e_participation_counts_v1(lower_span);
    // The node reads v2 participation at its incumbent's DAA: robust only if both tips agree.
    let (cap_a, cap_b) =
        (active(params.palw_rule_e_participation_v2, a.tip.daa_score), active(params.palw_rule_e_participation_v2, b.tip.daa_score));
    if cap_a != cap_b {
        return Ok(None);
    }
    let capital_v2 = cap_a;
    Ok(match palw_rule_e_order_v2(&pair.a, &pair.b, counts, capital_v2) {
        Ordering::Equal => {
            let no_credit = |s: &kaspa_consensus_core::palw_fork_choice_rule_e_v1::PalwRuleESideV1| {
                s.participation_capital_v2 == 0 && s.economic() == (0, 0, 0)
            };
            let even = |s: &kaspa_consensus_core::palw_fork_choice_rule_e_v1::PalwRuleESideV1| {
                if capital_v2 { s.participation_capital_v2 >= pair.even_split_min_capital_v2 } else { s.participation >= pair.even_split_min }
            };
            if (capital_v2 && no_credit(&pair.a) && no_credit(&pair.b)) || (counts && even(&pair.a) && even(&pair.b)) {
                Some((a.tip.blue_work, a.tip.hash).cmp(&(b.tip.blue_work, b.tip.hash)))
            } else {
                None
            }
        }
        decided => Some(decided),
    })
}

/// **L2 under rule E.** `Ok` — the chosen tip, the full node's choice by its own functions; `Err` — STOP (missing, unverifiable or
/// tampered evidence; a sealed split; no robust winner): nothing is signed.
pub fn verify_fork_choice_rule_e_v2(
    input: &RuleEInputV2<'_>,
    verify: &dyn Fn(&[u8], &[u8], &[u8]) -> bool,
) -> Result<RuleEEstablishedV2, L2StopV1> {
    let candidates = candidate_chains_v1(input.views)?;
    if candidates.is_empty() {
        return Err(stop("no verified view"));
    }
    let mut peers: Vec<&str> = input.views.iter().map(|v| v.peer.as_str()).collect();
    peers.sort_unstable();
    peers.dedup();
    if peers.len() < input.limits.min_peers {
        return Err(stop(format!("{} independent peer(s) shown, {} required", peers.len(), input.limits.min_peers)));
    }
    let now_daa = candidates.iter().map(|c| c.tip.daa_score).max().unwrap_or(0);
    // Each candidate weighed AT its tip: a fresh trusted v3 attestation whose committed root a served leaf opens.
    let mut tips: Vec<(&VerifiedChainV1, &CommittedServedV2, Hash64)> = Vec::new();
    for chain in &candidates {
        let p = *chain.points.last().ok_or_else(|| stop("an empty verified chain"))?;
        let att = input
            .evidence
            .iter()
            .map(|e| &e.attestation)
            .find(|a| a.block == chain.tip_hash())
            .ok_or(L2StopV1::Unweighable(chain.tip_hash()))?;
        verify_attestation_rule_e_v2(att, input.trusted, input.ruleset, now_daa, input.limits, verify).map_err(stop)?;
        if att.block_daa != p.daa_score {
            return Err(stop(format!("the attestation of {} names DAA {}, the header {}", att.block, att.block_daa, p.daa_score)));
        }
        let (entry, form) = opened(input.params, input.served, &att.committed_root, &p).map_err(stop)?;
        if form != PalwCommitFormV1::RuleEV3 {
            return Err(stop(format!("the tip {} does not commit the rule-E v3 form", p.hash)));
        }
        tips.push((chain, entry, att.block));
    }
    if tips.len() == 1 {
        let (chain, entry, attested) = tips[0];
        return Ok(RuleEEstablishedV2 { chosen: chain.clone(), attested, inner_root: entry.inner_root, refused: Vec::new() });
    }
    // Every pair: verified paths down to the fork, the fork's v3 leaf, each tip's verified window above it, the fork's capital.
    let n = tips.len();
    let mut beats = vec![vec![false; n]; n];
    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            let (a, a_entry, _) = tips[i];
            let (b, b_entry, _) = tips[j];
            let fi = fork_index(a, b);
            let fork = a.points[fi];
            for x in [a, b] {
                let depth = x.tip.blue_score.saturating_sub(fork.blue_score);
                if depth >= input.params.finality_depth() {
                    return Err(L2StopV1::Sealed { tip: x.tip_hash(), depth, finality: input.params.finality_depth() });
                }
            }
            if !active(input.params.palw_fork_choice_rule_e_v1, fork.daa_score)
                || !active(input.params.palw_fork_choice_rule_e_v1, a.tip.daa_score)
                || !active(input.params.palw_fork_choice_rule_e_v1, b.tip.daa_score)
            {
                return Err(L2StopV1::LeafV1Insufficient("rule E is not in force over the whole conflict: the v1 path decides it"));
            }
            let a_path = verify_selected_chain_v2(input.params, a, fi, a.points.len() - 1, input.served)
                .map_err(|why| L2StopV1::SelectedChainUnverified { tip: a.tip_hash(), why })?;
            verify_selected_chain_v2(input.params, b, fork_index(b, a), b.points.len() - 1, input.served)
                .map_err(|why| L2StopV1::SelectedChainUnverified { tip: b.tip_hash(), why })?;
            let fork_entry = a_path.first().copied().ok_or_else(|| stop("the fork point has no verified leaf (the tip is the fork)"))?;
            let fork_leaf = v3(fork_entry).map_err(stop)?;
            let window = |entry: &CommittedServedV2| -> Result<Vec<PalwRuleEClaimRecordV1>, L2StopV1> {
                let leaf = v3(entry).map_err(stop)?;
                let (from, opening) =
                    entry.window.as_ref().ok_or_else(|| stop(format!("no rule-E window of {} was served", entry.block)))?;
                if *from != fork.blue_score {
                    return Err(stop(format!("the window of {} is opened above {from}, not the fork's {}", entry.block, fork.blue_score)));
                }
                verify_window_suffix_v3(&leaf, fork.blue_score, opening)
                    .map_err(|e| stop(format!("the rule-E window of {} does not verify: {e:?}", entry.block)))
            };
            let a_window = window(a_entry)?;
            let b_window = window(b_entry)?;
            // The fork's proven capital of every bond the windows name; a bond with no verified opening is not counted (the node's
            // accumulator counts only what the registry holds) — but a served opening that does not verify is tampering: STOP.
            let mut fork_capital = BTreeMap::new();
            for record in a_window.iter().chain(b_window.iter()) {
                if fork_capital.contains_key(&record.bond) {
                    continue;
                }
                let opening = fork_entry
                    .registry
                    .iter()
                    .find(|(bond, _)| *bond == record.bond)
                    .ok_or_else(|| stop(format!("no registry opening of a window bond at the fork {}", fork.hash)))?;
                match palw_rule_e_verify_registry_v3(&fork_leaf, &record.bond, &opening.1) {
                    Ok(Some(capital)) => {
                        fork_capital.insert(record.bond, capital);
                    }
                    Ok(None) => {}
                    Err(e) => return Err(stop(format!("the fork's registry opening does not verify: {e:?}"))),
                }
            }
            let a_leaf = v3(a_entry).map_err(stop)?;
            match pair_v2(input.params, a, &a_leaf, &a_window, b, &b_window, &fork, &fork_leaf, &fork_capital).map_err(stop)? {
                Some(std::cmp::Ordering::Greater) => beats[i][j] = true,
                Some(_) => {}
                None => return Err(L2StopV1::NoRobustWinner),
            }
        }
    }
    let winners: Vec<usize> = (0..n).filter(|&i| (0..n).all(|j| i == j || beats[i][j])).collect();
    let [win] = winners.as_slice() else { return Err(L2StopV1::NoRobustWinner) };
    let (chain, entry, attested) = tips[*win];
    let refused = tips.iter().enumerate().filter(|(i, _)| i != win).map(|(_, (c, _, _))| c.tip_hash()).collect();
    Ok(RuleEEstablishedV2 { chosen: chain.clone(), attested, inner_root: entry.inner_root, refused })
}


/// **L3 under rule E**: the ADR-0043 root a collection proof at `proof_header` is checked against — only for a header on the chosen
/// chain at or below the attested tip, its parent's committed leaf verified down from the tip (`verify_selected_chain_v2`).
pub fn l3_root_under_rule_e_v2(
    est: &RuleEEstablishedV2,
    proof_header: &kaspa_consensus_core::header::Header,
    served: &[CommittedServedV2],
    params: &Params,
) -> Result<Hash64, String> {
    if kaspa_consensus_core::hashing::header::hash(proof_header) != proof_header.hash {
        return Err("the proof's header does not hash to itself".into());
    }
    let at = est.chosen.index_of(&proof_header.hash).ok_or_else(|| format!("the proof's header {} is not on the chosen chain", proof_header.hash))?;
    let attested_at = est.chosen.index_of(&est.attested).ok_or("the attested block left the chosen chain")?;
    if at > attested_at || at == 0 {
        return Err(format!("the proof's header {} is not between the checkpoint and the attested tip", proof_header.hash));
    }
    let walked = verify_selected_chain_v2(params, &est.chosen, at - 1, attested_at, served)?;
    Ok(walked[0].inner_root)
}

/// The bonds the (unverified) tip windows of `served` name — what the fork's registry openings must be asked for. The windows are
/// verified later against the verified leaves; a bond asked for in vain only costs an opening.
pub fn window_bonds_v2(served: &[CommittedServedV2], tips: &[Hash64]) -> Vec<PalwBondKeyV2> {
    let mut bonds: Vec<PalwBondKeyV2> = served
        .iter()
        .filter(|s| tips.contains(&s.block))
        .filter_map(|s| s.window.as_ref())
        .flat_map(|(_, w)| w.records.iter().map(|r| r.bond))
        .collect();
    bonds.sort();
    bonds.dedup();
    bonds
}
