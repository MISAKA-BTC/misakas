//! Rule E / Gate C reader. Every leaf is authenticated before its sub-openings
//! are consumed, and every registry absence must itself be opened.
use super::*;
use kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v2::{PalwForkChoiceLeafV2, palw_rule_e_pair_from_openings_v1};
use kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v3::{
    PALW_FORK_CHOICE_MAX_PROOF_BONDS_V2, PalwCommittedOpeningV2, PalwForkChoiceLeafV3, PalwForkChoiceProofQueryV2,
    PalwForkChoiceProofV2, PalwForkChoiceProofViewV2, palw_rule_e_pair_from_openings_v3,
};
use kaspa_consensus_core::palw_fork_choice_rule_e_v1::{palw_rule_e_decide_v2, palw_rule_e_participation_counts_v1};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct ForkChoiceEvidenceV2 {
    pub attestation: ForkChoiceAttestationV1,
    pub opening: PalwCommittedOpeningV2,
}

pub type FetchForkChoiceProofV2<'a> = dyn Fn(Hash64, &PalwForkChoiceProofQueryV2) -> Result<Vec<PalwForkChoiceProofV2>, String> + 'a;

pub struct L2InputV2<'a> {
    pub params: &'a Params,
    pub views: &'a [PeerViewV1],
    pub evidence: &'a [ForkChoiceEvidenceV2],
    pub chain_openings: &'a [PalwCommittedOpeningV2],
    pub trusted: &'a [(Vec<u8>, Vec<u8>)],
    pub ruleset: &'a ClientRulesetV1,
    pub limits: &'a L2LimitsV1,
    /// Untrusted sub-openings from any peer; the reader verifies each against
    /// the already-authenticated leaf, including all requested absences.
    pub fetch_proof: &'a FetchForkChoiceProofV2<'a>,
}

#[derive(Clone, Debug)]
pub struct L2EstablishedV2 {
    pub chosen: VerifiedChainV1,
    pub attestation: ForkChoiceAttestationV1,
    pub opening: PalwCommittedOpeningV2,
    pub refused: Vec<Hash64>,
}

impl L2EstablishedV2 {
    pub fn trust_line(&self) -> String {
        format!(
            "L2 fork choice: verified by issuer '{}' at {}, leaf v{}; {} competing tips refused",
            self.attestation.issuer(),
            self.attestation.block,
            self.attestation.leaf_version,
            self.refused.len()
        )
    }
    pub fn status(&self) -> L2StatusV1 {
        L2StatusV1::EstablishedByAttestation { trust: self.trust_line() }
    }
}

/// The selected-parent walk uses exactly the full node's root reader. A flat
/// root cannot authenticate which parent was selected and stops this walk.
pub fn verify_selected_chain_v2(
    params: &Params,
    chain: &VerifiedChainV1,
    low: usize,
    top: usize,
    openings: &[PalwCommittedOpeningV2],
) -> Result<Vec<PalwCommittedOpeningV2>, String> {
    if low > top || top >= chain.points.len() {
        return Err("selected-chain range is out of bounds".into());
    }
    let mut out = Vec::new();
    for k in (low + 1..=top).rev() {
        let (parent, child) = (chain.points[k - 1], chain.points[k]);
        let found = openings
            .iter()
            .find(|o| !o.leaf.is_empty() && o.verify(params, &child.palw_state_root, &fork_point(&parent)).is_ok())
            .ok_or_else(|| format!("no authenticated selected-parent opening of {} below {}", parent.hash, child.hash))?;
        out.push(found.clone());
    }
    out.reverse();
    Ok(out)
}

fn proof_view(
    input: &L2InputV2<'_>,
    block: Hash64,
    opening: &PalwCommittedOpeningV2,
    query: &PalwForkChoiceProofQueryV2,
) -> Result<PalwForkChoiceProofViewV2, String> {
    for proof in (input.fetch_proof)(block, query)? {
        if let Ok(view) = proof.verify(&opening.leaf, query) {
            return Ok(view);
        }
    }
    Err(format!("no complete authenticated Rule E sub-opening for {block}"))
}

/// Establish the Rule E choice from all shown candidates, or refuse to sign.
/// The caller keeps the V1 verifier for comparisons wholly below Rule E.
pub fn verify_fork_choice_v2(input: &L2InputV2<'_>, verify: &dyn Fn(&[u8], &[u8], &[u8]) -> bool) -> Result<L2EstablishedV2, String> {
    let candidates = candidate_chains_v1(input.views).map_err(|e| e.to_string())?;
    let peers: BTreeSet<_> = input.views.iter().map(|v| &v.peer).collect();
    if candidates.is_empty() || peers.len() < input.limits.min_peers {
        return Err("insufficient independent verified peer views".into());
    }
    let now = candidates.iter().map(|c| c.tip.daa_score).max().unwrap();
    let mut evidence = BTreeMap::new();
    for ev in input.evidence {
        let att = &ev.attestation;
        if verify_attestation_v2(input.params, att, input.trusted, input.ruleset, now, input.limits, verify).is_err() {
            continue;
        }
        let Some((point, _)) = candidates.iter().find_map(|c| point_and_child_root(c, &att.block)) else {
            return Err(L2StopV1::AttestedBlockHidden(att.block).to_string());
        };
        if point.daa_score != att.block_daa {
            continue;
        }
        for root in candidates.iter().filter_map(|c| point_and_child_root(c, &att.block)).filter_map(|(_, r)| r) {
            if root != att.committed_root {
                return Err(format!("issuer's root for {} contradicts a chain child", att.block));
            }
        }
        if ev.opening.verify(input.params, &att.committed_root, &fork_point(&point)).is_ok() {
            evidence.insert(att.block, ev);
        }
    }
    let finish = |chain: &VerifiedChainV1, ev: &ForkChoiceEvidenceV2| L2EstablishedV2 {
        chosen: chain.clone(),
        attestation: ev.attestation.clone(),
        opening: ev.opening.clone(),
        refused: candidates.iter().filter(|c| c.tip_hash() != chain.tip_hash()).map(|c| c.tip_hash()).collect(),
    };
    if candidates.len() == 1 {
        let chain = &candidates[0];
        let ev = chain.hashes.iter().rev().find_map(|b| evidence.get(b)).ok_or("no fresh authenticated post-state on the chain")?;
        if chain.tip.daa_score.saturating_sub(ev.attestation.block_daa) > input.limits.max_attested_lag_daa {
            return Err("attested post-state is too far behind the tip".into());
        }
        return Ok(finish(chain, ev));
    }
    let rules = ForkChoiceRulesV1::of(input.params);
    // Refuse legacy DNS uncertainty exactly as V1 does. Retirement makes this
    // gate abstain; no DNS votes supply Rule E credit.
    let mut dns_anchors = BTreeSet::new();
    if candidates.iter().any(|c| rules.dns_gate_may_run(c.tip.daa_score)) {
        for c in &candidates {
            let ev = evidence.get(&c.tip_hash()).ok_or("unattested competing tip")?;
            let fact = ev.attestation.dns_gate.ok_or("missing legacy DNS gate fact")?;
            if let (true, Some((anchor, daa))) = (fact.stage_active, fact.confirmed_anchor) {
                if daa >= candidates[0].points[0].daa_score {
                    if !candidates.iter().all(|c| c.contains(&anchor)) {
                        return Err("legacy DNS anchor differs between competing candidates".into());
                    }
                    dns_anchors.insert(anchor);
                }
            }
        }
    }
    // Merely appearing in the peer's header list does not establish selected
    // ancestry. An older DNS anchor can precede the Rule E fork point, so walk
    // all the way to every relevant anchor before the gate may abstain.
    for anchor in dns_anchors {
        for chain in &candidates {
            let low = chain.index_of(&anchor).ok_or("DNS anchor disappeared from a candidate")?;
            verify_selected_chain_v2(input.params, chain, low, chain.points.len() - 1, input.chain_openings)?;
        }
    }
    let mut beats = vec![vec![false; candidates.len()]; candidates.len()];
    for i in 0..candidates.len() {
        for j in i + 1..candidates.len() {
            let (a, b) = (&candidates[i], &candidates[j]);
            let ai = fork_index(a, b);
            let bi = b.index_of(&a.points[ai].hash).ok_or("missing common fork point")?;
            let fork = a.points[ai];
            for c in [a, b] {
                if c.tip.blue_score.saturating_sub(fork.blue_score) >= rules.finality_depth {
                    return Err("competing fork is sealed by finality".into());
                }
            }
            let ea = evidence.get(&a.tip_hash()).ok_or("unattested competing tip")?;
            let eb = evidence.get(&b.tip_hash()).ok_or("unattested competing tip")?;
            let pa = verify_selected_chain_v2(input.params, a, ai, a.points.len() - 1, input.chain_openings)?;
            let pb = verify_selected_chain_v2(input.params, b, bi, b.points.len() - 1, input.chain_openings)?;
            let fo = pa.first().ok_or("fork point not opened")?;
            if pb.first() != Some(fo) {
                return Err("the two authenticated paths disagree at the fork".into());
            }
            let q = PalwForkChoiceProofQueryV2 { from_blue_score: Some(fork.blue_score), bonds: Vec::new() };
            let ra = proof_view(input, a.tip_hash(), &ea.opening, &q)?.records;
            let rb = proof_view(input, b.tip_hash(), &eb.opening, &q)?.records;
            let bonds: Vec<PalwBondKeyV2> = ra.iter().chain(&rb).map(|r| r.bond).collect::<BTreeSet<_>>().into_iter().collect();
            let mut capital = BTreeMap::new();
            for chunk in bonds.chunks(PALW_FORK_CHOICE_MAX_PROOF_BONDS_V2) {
                let q = PalwForkChoiceProofQueryV2 { from_blue_score: None, bonds: chunk.to_vec() };
                capital.extend(proof_view(input, fork.hash, fo, &q)?.capital);
            }
            let pair = if let Ok(f) = PalwForkChoiceLeafV3::decode(&fo.leaf) {
                palw_rule_e_pair_from_openings_v3(&ra, &rb, |k| capital.get(k).copied().flatten(), &f)
            } else {
                let f = PalwForkChoiceLeafV2::decode(&fo.leaf).map_err(|e| format!("fork registry unavailable: {e:?}"))?;
                palw_rule_e_pair_from_openings_v1(&ra, &rb, |k| capital.get(k).copied().flatten().is_some(), f.bonds_len)
            }
            .map_err(|e| format!("Rule E pair unavailable: {e:?}"))?;
            let counts = palw_rule_e_participation_counts_v1(a.tip.daa_score.min(b.tip.daa_score).saturating_sub(fork.daa_score));
            for (challenger, incumbent, cs, ins, ci, ii) in [(a, b, &pair.a, &pair.b, i, j), (b, a, &pair.b, &pair.a, j, i)] {
                if !input.params.palw_fork_choice_rule_e_active_at(incumbent.tip.daa_score) {
                    return Err("mixed pre-Rule-E fork requires the legacy comparator".into());
                }
                let capital_v2 = ForkChoiceRulesV1::active(input.params.palw_rule_e_participation_v2, incumbent.tip.daa_score);
                let heavier = (challenger.tip.blue_work, challenger.tip_hash()) > (incumbent.tip.blue_work, incumbent.tip_hash());
                let shallow = heavier
                    && challenger.tip.daa_score >= incumbent.tip.daa_score
                    && incumbent.tip.daa_score.saturating_sub(fork.daa_score)
                        <= kaspa_consensus_core::palw_fork_authority_v2::PALW_REORG_SHALLOW_TIE_DAA_V1;
                beats[ci][ii] =
                    palw_rule_e_decide_v2(ins, cs, counts, &pair, capital_v2, || heavier, || shallow) == PalwDeepReorgV2::Allow;
            }
        }
    }
    let winners: Vec<_> =
        (0..candidates.len()).filter(|&i| (0..candidates.len()).all(|j| i == j || (beats[i][j] && !beats[j][i]))).collect();
    let [win] = winners.as_slice() else {
        return Err("no unique Rule E winner independent of incumbent".into());
    };
    let chosen = &candidates[*win];
    // ADR-0065 D2 remains a separate veto where armed. Read counts only from
    // authenticated leaves, and refuse if the client's ruleset has no panel bound.
    let bonds_len = |o: &PalwCommittedOpeningV2| -> Result<u64, String> {
        if let Ok(l) = PalwForkChoiceLeafV3::decode(&o.leaf) {
            return Ok(l.base.bonds_len);
        }
        if let Ok(l) = PalwForkChoiceLeafV2::decode(&o.leaf) {
            return Ok(l.bonds_len);
        }
        PalwForkChoiceLeafV1::decode(&o.leaf).map(|l| l.bonds_len).ok_or("registry count unavailable".into())
    };
    for other in candidates.iter().filter(|c| c.tip_hash() != chosen.tip_hash()) {
        let at = fork_index(chosen, other);
        let daas = [chosen.tip.daa_score, other.tip.daa_score, chosen.points[at].daa_score];
        if daas.iter().any(|d| ForkChoiceRulesV1::active(rules.frontier_provenance, *d)) {
            let panel = rules.panel.ok_or("no panel parameters to bound frontier provenance")?;
            let path = verify_selected_chain_v2(input.params, chosen, at, chosen.points.len() - 1, input.chain_openings)?;
            let minted = bonds_len(&evidence[&chosen.tip_hash()].opening)?.saturating_sub(bonds_len(&path[0])?);
            if palw_minted_seats_can_reach_quorum_v1(usize::try_from(minted).unwrap_or(usize::MAX), &panel) {
                return Err("bonds minted after the fork can seat a quorum".into());
            }
        }
    }
    Ok(finish(chosen, evidence[&chosen.tip_hash()]))
}

pub fn l3_root_under_l2_v2(
    params: &Params,
    established: &L2EstablishedV2,
    header: &Header,
    openings: &[PalwCommittedOpeningV2],
) -> Result<Hash64, String> {
    if kaspa_consensus_core::hashing::header::hash(header) != header.hash {
        return Err("L3 header hash mismatch".into());
    }
    let at = established.chosen.index_of(&header.hash).ok_or("L3 header is off the chosen chain")?;
    let att_at = established.chosen.index_of(&established.attestation.block).ok_or("attested block left the chosen chain")?;
    if at == att_at + 1 && header.palw_state_root == established.attestation.committed_root {
        return Ok(established.opening.inner_root);
    }
    if at == 0 || at > att_at {
        return Err("L3 header is outside the attested selected-chain range".into());
    }
    let walked = verify_selected_chain_v2(params, &established.chosen, at - 1, att_at, openings)?;
    Ok(walked[0].inner_root)
}
