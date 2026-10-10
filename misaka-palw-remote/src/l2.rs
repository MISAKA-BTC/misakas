//! **RFC-0009 L2 — the PALW fork choice, verified without a full node** (design: `docs/design/palw/rfc9-l2-fork-choice.md`).
//!
//! Three things, kept apart:
//!
//! 1. **Values bound to a root.** Past `Params::palw_fork_choice_commitment_v1` a header commits `H(fork-choice leaf ‖ ADR-0043 root)`;
//!    an opening ([`PalwForkChoiceOpeningV1`]) gives the comparator's inputs in 356 bytes, checked here against the root. The leaf's
//!    weight-allocation slot (ADR-0176 D3) names the versioned bond-budget allocation the weights are read through; this build reads
//!    only "none in force", and a conflict where `palw_bond_budget_v1` may be in force STOPs. No leaf or opening carries a
//!    model-availability condition (ADR-0177).
//! 2. **Transition validity of the root.** Re-executing the fold is a full node's work (`FULL_NODE`). Here the root comes from an
//!    **attestation** ([`ForkChoiceAttestationV1`]) by an issuer the user chose before talking to any node — the user's "trusted
//!    checkpoint" — and is cross-checked against the attested block's chain child whenever a peer shows one. The issuer's trust is named
//!    on every output ([`L2VerdictV1::trust_line`]).
//! 3. **The same rule.** The node's own decision functions (`palw_fork_authority_v2`) are evaluated on the opened orders, in both
//!    directions, with every input this client cannot verify (the shallow-tie GHOSTDAG answer) taken both ways, and with every fence
//!    variant that may be in force anywhere between the fork point and the tips. A candidate is chosen only when every variant agrees;
//!    anything node-local that could intervene (the DNS BFT gate on a confirmed anchor, ADR-0065 D2 beyond its bound, the finality seal,
//!    a comparator whose inputs the v1 leaf does not carry) is a STOP, never a guess.
//!
//! **The selected chain, past the fence.** L1 checks only that each header names the previous one as *a* parent; it cannot check the
//! GHOSTDAG selected-parent choice. A header's committed root, past the fence, opens to a leaf that NAMES the block whose post-state it
//! commits — its selected parent. So walking openings down from an attested block ([`verify_selected_chain_v1`]) proves the path is that
//! block's selected chain: a path through a merged block whose own (never checked) root is a forgery, or through a non-selected parent,
//! fails the walk. Everything here that reads a path — the fork point, the finality seal, a DNS anchor's side, ADR-0065 D2's bound, L3 below
//! the attested block — reads only a walked path.
//!
//! Pure: a restarted client asking other peers recomputes everything from the checkpoint, the attestations and the bytes served now.

use kaspa_consensus_core::config::params::{ForkActivation, Params};
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_fork_authority_v2::{
    PalwDeepReorgV2, PalwIbdCommitV2, decide_deep_reorg_v2, decide_ibd_commit_v2, palw_ibd_commit_strict_economic_v1,
    palw_reorg_strict_economic_win_v1, select_palw_tip_hash_v2,
};
use kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1;
use kaspa_consensus_core::palw_fork_choice_commitment_v1::{
    PALW_FORK_CHOICE_LEAF_VERSION_V1, PALW_WEIGHT_ALLOCATION_NONE_V1, PalwDnsGateFactV1, PalwForkChoiceLeafV1,
    PalwForkChoiceOpeningV1, PalwForkChoicePointV1,
};
use kaspa_consensus_core::palw_panel_v2::{PalwPanelParamsV2, palw_minted_seats_can_reach_quorum_v1};
use kaspa_hashes::Hash64;

use crate::verify::{ChainPointV1, ClientRulesetV1, L2StatusV1, VerifiedChainV1};
use crate::{finish, keyed, put_len};

pub const DOMAIN_FORK_CHOICE_ATTESTATION: &[u8] = b"misaka-palw/remote/fork-choice-attestation/v1";

// ---------------------------------------------------------------------------------------------------------------------------------
// The rules — the client's ruleset, never a node's
// ---------------------------------------------------------------------------------------------------------------------------------

/// **The fork-choice rules this client's ruleset has in force** — read from the `Params` it was built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForkChoiceRulesV1 {
    /// `palw_fork_choice_commitment_v1`: where an opening exists at all.
    pub commitment: Option<ForkActivation>,
    /// `palw_reorg_strict_economic_win` (read at the incumbent's DAA by the node).
    pub strict_win: Option<ForkActivation>,
    /// `palw_pruning_proof_strict_economic_win` (the IBD commit's variant).
    pub ibd_strict: Option<ForkActivation>,
    /// `palw_frontier_provenance` (ADR-0065 D2).
    pub frontier_provenance: Option<ForkActivation>,
    /// The DNS BFT gate's activation (`dns_bft_gate`) on a network with the overlay; `None` without.
    pub dns_gate: Option<ForkActivation>,
    /// `palw_dns_retirement`: past it the DNS gate never runs.
    pub dns_retired: Option<ForkActivation>,
    /// **A comparator whose inputs the v1 leaf does not carry** — ADR-0178 rule E (`palw_fork_choice_rule_e_v1`, lane FINX). Where it
    /// may be in force at the fork point or a tip, a conflict cannot be weighed from v1 openings and STOPs; a single chain is unaffected.
    /// Its leaf (v2) ships under that fence. `None` in [`Self::of`] until that fence is in this tree: the integration that brings it
    /// sets this field from it.
    pub rule_e: Option<ForkActivation>,
    /// **ADR-0176 D3: `palw_bond_budget_v1`** (lane BUDGET, dormant). Where it is in force, Final weight is bounded per bond and the
    /// comparator reads the budget-capped weights of the leaf's allocation slot; this build reads no allocation version, so a
    /// conflict there STOPs and a version-0 slot at a point past it is refused. `None` in [`Self::of`] until that fence is in this
    /// tree: the integration that brings it sets this field from it (and the slot version it reads).
    pub bond_budget: Option<ForkActivation>,
    pub finality_depth: u64,
    pub panel: Option<PalwPanelParamsV2>,
}

impl ForkChoiceRulesV1 {
    pub fn of(params: &Params) -> Self {
        let panel = match &params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.panel),
            _ => None,
        };
        Self {
            commitment: params.palw_fork_choice_commitment_v1,
            strict_win: params.palw_reorg_strict_economic_win,
            ibd_strict: params.palw_pruning_proof_strict_economic_win,
            frontier_provenance: params.palw_frontier_provenance,
            dns_gate: params.dns_params.as_ref().and(params.dns_bft_gate.as_ref()).map(|g| g.activation),
            dns_retired: params.palw_dns_retirement.as_ref().map(|r| r.activation),
            rule_e: None,
            bond_budget: None,
            finality_depth: params.finality_depth(),
            panel,
        }
    }

    fn active(fence: Option<ForkActivation>, daa: u64) -> bool {
        fence.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa))
    }

    /// Can the DNS BFT gate run for an incumbent at `daa` (the gate is read at the incumbent's DAA)?
    pub fn dns_gate_may_run(&self, daa: u64) -> bool {
        Self::active(self.dns_gate, daa) && !Self::active(self.dns_retired, daa)
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The attestation — the trusted checkpoint, generalized
// ---------------------------------------------------------------------------------------------------------------------------------

/// **An issuer's statement that `block`'s post-state commits `committed_root`**, with the DNS BFT gate facts its node holds. Signed over
/// [`Self::signing_digest`]; the primitive is injected (ML-DSA-87 in binaries).
///
/// What an issuer states by signing: its full node computed `block`'s post-state — which a node does only for a block whose whole
/// selected chain passed validation, every header's committed root included (a block whose header commits another root is disqualified
/// from the chain and gets no PALW delta; op 203 serves no opening for it) — and that post-state commits `committed_root`. So an
/// attestation covers `block`'s own header root and, through [`verify_selected_chain_v1`], every root on its selected chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForkChoiceAttestationV1 {
    pub network_id: String,
    pub consensus_params_id: String,
    pub consensus_schedule_id: String,
    pub block: Hash64,
    pub block_daa: u64,
    pub committed_root: Hash64,
    pub leaf_version: u16,
    /// The issuer node's DNS BFT gate facts (`None`: the issuer did not attest them — a conflict where the gate may run then STOPs).
    pub dns_gate: Option<PalwDnsGateFactV1>,
    pub issued_at_daa: u64,
    pub key_id: Vec<u8>,
    pub signature: Vec<u8>,
}

impl ForkChoiceAttestationV1 {
    pub fn signing_digest(&self) -> Hash64 {
        let mut s = keyed(DOMAIN_FORK_CHOICE_ATTESTATION);
        put_len(&mut s, self.network_id.as_bytes());
        put_len(&mut s, self.consensus_params_id.as_bytes());
        put_len(&mut s, self.consensus_schedule_id.as_bytes());
        s.update(self.block.as_bytes().as_slice());
        s.update(&self.block_daa.to_le_bytes());
        s.update(self.committed_root.as_bytes().as_slice());
        s.update(&self.leaf_version.to_le_bytes());
        match &self.dns_gate {
            None => {
                s.update(&[0u8]);
            }
            Some(fact) => {
                s.update(&[1u8, fact.stage_active as u8]);
                match fact.confirmed_anchor {
                    None => {
                        s.update(&[0u8]);
                    }
                    Some((anchor, daa)) => {
                        s.update(&[1u8]);
                        s.update(anchor.as_bytes().as_slice());
                        s.update(&daa.to_le_bytes());
                    }
                }
            }
        }
        s.update(&self.issued_at_daa.to_le_bytes());
        put_len(&mut s, &self.key_id);
        finish(s)
    }

    /// The issuer, as the user configured it (`key_id` is the user's own label for the key).
    pub fn issuer(&self) -> String {
        String::from_utf8(self.key_id.clone()).unwrap_or_else(|_| faster_hex::hex_string(&self.key_id))
    }
}

/// One candidate's evidence: an attestation and the opening of the attested block's post-state (served by any node, op 203).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForkChoiceEvidenceV1 {
    pub attestation: ForkChoiceAttestationV1,
    pub opening: PalwForkChoiceOpeningV1,
}

/// One peer's L1-verified view (`peer` is the client's own name for the peer: two views count as two peers only under two names).
#[derive(Clone, Debug)]
pub struct PeerViewV1 {
    pub peer: String,
    pub chain: VerifiedChainV1,
}

/// The bounds L2 runs under (the client's own).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct L2LimitsV1 {
    /// Independent peers a view must come from — one peer cannot show the tip it hides.
    pub min_peers: usize,
    /// An attestation older than this (in DAA, against the views' tip) is stale.
    pub max_attestation_age_daa: u64,
    /// A single-chain view is verified while its tip is at most this many DAA past the attested block.
    pub max_attested_lag_daa: u64,
}

impl Default for L2LimitsV1 {
    fn default() -> Self {
        Self { min_peers: 2, max_attestation_age_daa: 2, max_attested_lag_daa: 1 }
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The verdict
// ---------------------------------------------------------------------------------------------------------------------------------

/// What L2 established — or why not (a downgrade to `HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum L2VerdictV1 {
    Established {
        /// The chosen chain (the tip the client may build on).
        chosen: VerifiedChainV1,
        /// The attested block on it the values were opened at, the root attested for its post-state, and the verified opening of it.
        attested: Hash64,
        attested_daa: u64,
        attested_root: Hash64,
        attested_opening: PalwForkChoiceOpeningV1,
        issuer: String,
        issued_at_daa: u64,
        /// The candidates the comparator refused, by tip.
        refused: Vec<Hash64>,
        /// How the DNS BFT gate, which runs ahead of the comparator, was accounted for.
        dns_gate: L2DnsGateV1,
    },
    Unverified(String),
}

/// **How the DNS BFT gate was accounted for** in a verdict (printed on the trust line). The gate refuses only a candidate that abandons a
/// confirmed DNS-final anchor while the overlay is in its `Active` stage; live testnet-12's overlay is in `Bootstrap`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum L2DnsGateV1 {
    /// One chain: nothing for the gate to refuse.
    NotNeeded,
    /// The gate cannot run at any weighed DAA (no overlay, not yet active, or retired).
    NotRunning,
    /// Every weighed candidate's issuer attested the overlay outside its `Active` stage or with nothing confirmed (testnet-12:
    /// `Bootstrap`): the gate refuses nothing, and the comparator decided.
    NeverRefuses,
    /// A confirmed `Active`-stage anchor stands on every candidate's verified selected chain (or below the checkpoint, binding all
    /// alike): the gate refuses none of them, and the comparator decided.
    AnchorOnEveryCandidate,
}

impl L2DnsGateV1 {
    fn line(&self) -> &'static str {
        match self {
            L2DnsGateV1::NotNeeded => "",
            L2DnsGateV1::NotRunning => "; the DNS BFT gate cannot run here",
            L2DnsGateV1::NeverRefuses => {
                "; DNS BFT gate: attested outside its Active stage or with nothing confirmed (Bootstrap) — it refuses nothing, the \
                 comparator decided"
            }
            L2DnsGateV1::AnchorOnEveryCandidate => {
                "; DNS BFT gate: its confirmed anchor stands on every candidate — it refuses none, the comparator decided"
            }
        }
    }
}

impl L2VerdictV1 {
    /// **The trust this verdict rests on, in one line** — printed with the mode label on every output.
    pub fn trust_line(&self) -> String {
        match self {
            L2VerdictV1::Established { attested, attested_daa, issuer, issued_at_daa, refused, dns_gate, .. } => format!(
                "L2 fork choice: VERIFIED against a root attested by issuer '{issuer}' (block {attested}, DAA {attested_daa}, issued at DAA \
                 {issued_at_daa}); the issuer's re-execution is the trust, the values were opened and compared by this client{}{}",
                if refused.is_empty() {
                    String::new()
                } else {
                    format!("; {} competing tip(s) refused by the comparator", refused.len())
                },
                dns_gate.line()
            ),
            L2VerdictV1::Unverified(why) => format!("L2 fork choice: NOT verified — {why}"),
        }
    }

    /// The status `verify::mode_label_v1` reads.
    pub fn status(&self) -> L2StatusV1 {
        match self {
            L2VerdictV1::Established { .. } => L2StatusV1::EstablishedByAttestation { trust: self.trust_line() },
            L2VerdictV1::Unverified(_) => L2StatusV1::Unverified("the PALW fork choice is not verified (see the L2 line)"),
        }
    }
}

/// **STOP**: nothing is signed or started, whatever the opt-in.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum L2StopV1 {
    #[error("the views are not rooted at the same checkpoint")]
    DifferentCheckpoints,
    #[error(
        "an attestation names block {0}, which no configured peer shows: a peer is hiding a tip (or the issuer is on another branch)"
    )]
    AttestedBlockHidden(Hash64),
    /// The attested root and the root the next header on a view commits for the same block differ. Either the issuer is wrong, or that
    /// header is (a block the chain disqualified, or a merged block on a path through a non-selected parent) — the client cannot tell
    /// which without a second source, so it stops; drop the issuer only when another source confirms the header.
    #[error(
        "issuer '{issuer}' attested root {attested} for block {block}, and the next header on a view commits {chain}: the issuer is contradicted \
         by the chain, unless that header is invalid or off the selected chain"
    )]
    IssuerContradicted { issuer: String, block: Hash64, attested: Hash64, chain: Hash64 },
    #[error("the peers show competing tips and {0} is not weighable (no attestation at its tip): its PALW order is unknown here")]
    Unweighable(Hash64),
    #[error("the peers show competing tips and {tip}'s path to the fork point is not its verified selected chain: {why}")]
    SelectedChainUnverified { tip: Hash64, why: String },
    #[error("the peers show competing tips while a comparator whose inputs the v1 leaf does not carry may be in force: {0}")]
    LeafV1Insufficient(&'static str),
    #[error("the peers show competing tips while a bond-budget allocation may be in force (ADR-0176 D3): {0}")]
    BondBudgetAllocation(&'static str),
    #[error("the peers show competing tips while the DNS BFT gate may run, and {0}")]
    DnsGateMayDecide(&'static str),
    #[error("the competing tips part {depth} blue below {tip}, at or past the finality depth {finality}: a sealed split")]
    Sealed { tip: Hash64, depth: u64, finality: u64 },
    #[error("ADR-0065 D2 may veto the switch: {0}")]
    FrontierProvenance(&'static str),
    #[error(
        "no candidate robustly wins every in-force variant of the node's own decision functions: a node's answer would depend on more than \
         the verified inputs"
    )]
    NoRobustWinner,
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The pieces
// ---------------------------------------------------------------------------------------------------------------------------------

/// **Verify an attestation**: a key the client holds, this build's ruleset, a signature over the digest, fresh against `now_daa`.
pub fn verify_attestation_v1(
    att: &ForkChoiceAttestationV1,
    trusted: &[(Vec<u8>, Vec<u8>)],
    ruleset: &ClientRulesetV1,
    now_daa: u64,
    limits: &L2LimitsV1,
    verify: &dyn Fn(&[u8], &[u8], &[u8]) -> bool,
) -> Result<(), String> {
    let (_, pubkey) =
        trusted.iter().find(|(id, _)| *id == att.key_id).ok_or_else(|| format!("issuer '{}' is not trusted", att.issuer()))?;
    if att.network_id != ruleset.network_id
        || att.consensus_params_id != ruleset.consensus_params_id
        || att.consensus_schedule_id != ruleset.consensus_schedule_id
    {
        return Err(format!("issuer '{}' attests under another ruleset", att.issuer()));
    }
    if att.leaf_version != PALW_FORK_CHOICE_LEAF_VERSION_V1 {
        return Err(format!("leaf version {} is not one this client reads", att.leaf_version));
    }
    if !verify(pubkey, att.signing_digest().as_bytes().as_slice(), &att.signature) {
        return Err(format!("issuer '{}''s signature does not verify", att.issuer()));
    }
    if att.issued_at_daa > now_daa {
        return Err(format!("the attestation is issued in the future of the view (DAA {} > {now_daa})", att.issued_at_daa));
    }
    if now_daa - att.issued_at_daa > limits.max_attestation_age_daa {
        return Err(format!(
            "Stale: issuer '{}''s attestation was issued at DAA {} and the view is at {now_daa}, older than the allowed {} — a fresh one is required",
            att.issuer(),
            att.issued_at_daa,
            limits.max_attestation_age_daa
        ));
    }
    Ok(())
}

/// **Maximal chains, by containment** — the candidates. A view inside another adds nothing; two that part are two candidates.
pub fn candidate_chains_v1(views: &[PeerViewV1]) -> Result<Vec<VerifiedChainV1>, L2StopV1> {
    let mut out: Vec<VerifiedChainV1> = Vec::new();
    for v in views {
        if out.first().is_some_and(|c| c.checkpoint != v.chain.checkpoint) {
            return Err(L2StopV1::DifferentCheckpoints);
        }
        if out.iter().any(|c| c.contains(&v.chain.tip_hash())) {
            continue;
        }
        out.retain(|c| !v.chain.contains(&c.tip_hash()));
        out.push(v.chain.clone());
    }
    Ok(out)
}

/// The point a verified chain holds for `block`, and the root its next header commits (if the chain shows one).
fn point_and_child_root(chain: &VerifiedChainV1, block: &Hash64) -> Option<(ChainPointV1, Option<Hash64>)> {
    let at = chain.index_of(block)?;
    Some((chain.points[at], chain.points.get(at + 1).map(|child| child.palw_state_root)))
}

fn fork_point(p: &ChainPointV1) -> PalwForkChoicePointV1 {
    PalwForkChoicePointV1 { block: p.hash, daa_score: p.daa_score, blue_score: p.blue_score }
}

/// **The selected chain from `chain.points[top]` down to `chain.points[low]`, verified by openings** — and each block's opening.
///
/// `points[top]` must be a block whose own header root is covered (an attested block: its post-state was computed by a node that checked
/// every root on its selected chain). For each `k` from `top` down to `low + 1` an opening of `points[k − 1]`'s post-state must hash to the
/// root `points[k]`'s header commits: a covered header commits its SELECTED parent's post-state, and the leaf names that parent, so
/// `points[k − 1]` IS the selected parent and its header is covered in turn. `openings` is untrusted (any peer, op 203); only openings
/// that verify are used. Returns the verified openings of `points[low..top]`, index-aligned from `low`. Below the commitment fence a leaf
/// does not exist and the walk cannot pass: named, never assumed.
pub fn verify_selected_chain_v1(
    chain: &VerifiedChainV1,
    low: usize,
    top: usize,
    openings: &[PalwForkChoiceOpeningV1],
    fence: Option<ForkActivation>,
) -> Result<Vec<PalwForkChoiceOpeningV1>, String> {
    if low > top || top >= chain.points.len() {
        return Err(format!("no path from index {top} down to {low} on a chain of {} headers", chain.points.len()));
    }
    let mut out = Vec::with_capacity(top - low);
    for k in (low + 1..=top).rev() {
        let (parent, child) = (chain.points[k - 1], chain.points[k]);
        let mut refused = None;
        let found = openings.iter().filter(|o| o.leaf.block == parent.hash).find_map(|o| {
            match o.verify(&child.palw_state_root, &fork_point(&parent), fence) {
                Ok(_) => Some(*o),
                Err(e) => {
                    refused = Some(e);
                    None
                }
            }
        });
        match (found, refused) {
            (Some(o), _) => out.push(o),
            (None, Some(e)) => {
                return Err(format!("the opening of {} does not open the root {} commits: {e}", parent.hash, child.hash));
            }
            (None, None) => {
                return Err(format!(
                    "no opening of {} was served: {} cannot be shown to be its selected parent's child",
                    parent.hash, child.hash
                ));
            }
        }
    }
    out.reverse();
    Ok(out)
}

/// **Robust dominance**: `c` keeps against `o` and wins against `o` under every variant of the node's own decision functions the
/// ruleset may have in force at any of `daas` (the tips and the fork point: a node's incumbent stands anywhere between them), the
/// shallow-tie GHOSTDAG answer taken both ways.
pub fn robustly_dominates_v1(c: &PalwCandidateOrderV1, o: &PalwCandidateOrderV1, rules: &ForkChoiceRulesV1, daas: &[u64]) -> bool {
    let any = |f: Option<ForkActivation>| daas.iter().any(|d| ForkChoiceRulesV1::active(f, *d));
    let all = |f: Option<ForkActivation>| daas.iter().all(|d| ForkChoiceRulesV1::active(f, *d));
    let mut ok = true;
    if !all(rules.strict_win) {
        ok &= decide_deep_reorg_v2(o, c) == PalwDeepReorgV2::Allow && decide_deep_reorg_v2(c, o) == PalwDeepReorgV2::Refuse;
    }
    if any(rules.strict_win) {
        for tie in [true, false] {
            ok &= palw_reorg_strict_economic_win_v1(o, c, || tie) == PalwDeepReorgV2::Allow
                && palw_reorg_strict_economic_win_v1(c, o, || tie) == PalwDeepReorgV2::Refuse;
        }
    }
    if !all(rules.ibd_strict) {
        ok &= decide_ibd_commit_v2(o, c) == PalwIbdCommitV2::Commit && decide_ibd_commit_v2(c, o) == PalwIbdCommitV2::KeepIncumbent;
    }
    if any(rules.ibd_strict) {
        ok &= palw_ibd_commit_strict_economic_v1(o, c) == PalwIbdCommitV2::Commit
            && palw_ibd_commit_strict_economic_v1(c, o) == PalwIbdCommitV2::KeepIncumbent;
    }
    ok
}

/// The inputs of one L2 verification.
pub struct L2InputV1<'a> {
    pub views: &'a [PeerViewV1],
    pub evidence: &'a [ForkChoiceEvidenceV1],
    /// Openings of chain blocks' post-states from any peer (op 203) — untrusted: each counts only where [`verify_selected_chain_v1`]
    /// accepts it. A conflict needs them for each candidate from its tip down to the fork point (and to a DNS anchor it must show).
    pub chain_openings: &'a [PalwForkChoiceOpeningV1],
    pub trusted: &'a [(Vec<u8>, Vec<u8>)],
    pub ruleset: &'a ClientRulesetV1,
    pub rules: &'a ForkChoiceRulesV1,
    pub limits: &'a L2LimitsV1,
}

struct Weighed {
    order: PalwCandidateOrderV1,
    opening: PalwForkChoiceOpeningV1,
    att: ForkChoiceAttestationV1,
}

/// One competing tip, weighed at its tip, its path verified down to `low`.
struct Candidate<'a> {
    chain: &'a VerifiedChainV1,
    weighed: &'a Weighed,
    low: usize,
    /// The verified openings of `chain.points[low..tip]`.
    walked: Vec<PalwForkChoiceOpeningV1>,
}

impl Candidate<'_> {
    /// The verified leaf of the block at `index` (`low ≤ index < tip`).
    fn leaf_at(&self, index: usize) -> Option<PalwForkChoiceLeafV1> {
        index.checked_sub(self.low).and_then(|i| self.walked.get(i)).map(|o| o.leaf)
    }
}

/// The highest index of `c` whose block `o` also shows. Two candidates' paths from one checkpoint always share index 0; on two verified
/// selected chains the block there is their fork point (above it the paths share nothing).
fn fork_index(c: &VerifiedChainV1, o: &VerifiedChainV1) -> usize {
    (0..c.hashes.len()).rev().find(|&i| o.contains(&c.hashes[i])).unwrap_or(0)
}

/// **The openings a verification needs** — what a client asks op 203 for. Every candidate's tip (the attested opening); for a single chain
/// also the tip's predecessor (L3 at an attested tip unwraps through it); for a conflict every block of each candidate's path from the
/// deepest fork point it takes part in up to its tip (the walk of [`verify_selected_chain_v1`]). In path order, each block once.
pub fn openings_wanted_v1(candidates: &[VerifiedChainV1]) -> Vec<Hash64> {
    let mut out: Vec<Hash64> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        let tip = c.hashes.len().saturating_sub(1);
        let low = if candidates.len() == 1 {
            tip.saturating_sub(1)
        } else {
            candidates.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, o)| fork_index(c, o)).min().unwrap_or(0)
        };
        for h in c.hashes.iter().take(tip + 1).skip(low) {
            if !out.contains(h) {
                out.push(*h);
            }
        }
    }
    out
}

/// **L2.** `Ok(Established)` — the chosen tip and the trust it rests on; `Ok(Unverified)` — a downgrade, named; `Err` — STOP.
pub fn verify_fork_choice_v1(input: &L2InputV1<'_>, verify: &dyn Fn(&[u8], &[u8], &[u8]) -> bool) -> Result<L2VerdictV1, L2StopV1> {
    let mut peers: Vec<&str> = input.views.iter().map(|v| v.peer.as_str()).collect();
    peers.sort_unstable();
    peers.dedup();
    let candidates = candidate_chains_v1(input.views)?;
    if candidates.is_empty() {
        return Ok(L2VerdictV1::Unverified("no verified view".into()));
    }
    let now_daa = candidates.iter().map(|c| c.tip.daa_score).max().unwrap_or(0);

    // Every attestation: trusted, fresh, on a view, opening the attested root, agreeing with every next header any view shows for it.
    let mut weighed: Vec<Weighed> = Vec::new();
    let mut refusals: Vec<String> = Vec::new();
    for ev in input.evidence {
        let att = &ev.attestation;
        if let Err(why) = verify_attestation_v1(att, input.trusted, input.ruleset, now_daa, input.limits, verify) {
            refusals.push(why);
            continue;
        }
        let Some((point, _)) = candidates.iter().find_map(|c| point_and_child_root(c, &att.block)) else {
            return Err(L2StopV1::AttestedBlockHidden(att.block));
        };
        if point.daa_score != att.block_daa {
            refusals.push(format!("the attestation of {} names DAA {}, the header {}", att.block, att.block_daa, point.daa_score));
            continue;
        }
        for chain_root in candidates.iter().filter_map(|c| point_and_child_root(c, &att.block)).filter_map(|(_, r)| r) {
            if chain_root != att.committed_root {
                return Err(L2StopV1::IssuerContradicted {
                    issuer: att.issuer(),
                    block: att.block,
                    attested: att.committed_root,
                    chain: chain_root,
                });
            }
        }
        match ev.opening.verify(&att.committed_root, &fork_point(&point), input.rules.commitment) {
            // ADR-0176 D3: past `palw_bond_budget_v1` every reader reads the versioned allocation; a leaf that names none there is not
            // the node's (it would be weighed by uncapped weights).
            Ok(_)
                if ForkChoiceRulesV1::active(input.rules.bond_budget, point.daa_score)
                    && ev.opening.leaf.weight_allocation.version == PALW_WEIGHT_ALLOCATION_NONE_V1 =>
            {
                refusals.push(format!(
                    "the opening of {} names no bond-budget allocation at a point where palw_bond_budget_v1 is in force",
                    att.block
                ))
            }
            Ok(order) => weighed.push(Weighed { order, opening: ev.opening, att: att.clone() }),
            Err(e) => refusals.push(format!("the opening of {} does not hold: {e}", att.block)),
        }
    }
    let unverified = |why: String| -> Result<L2VerdictV1, L2StopV1> {
        let why = if refusals.is_empty() { why } else { format!("{why} ({})", refusals.join("; ")) };
        Ok(L2VerdictV1::Unverified(why))
    };
    if peers.len() < input.limits.min_peers {
        return unverified(format!(
            "{} independent peer(s) shown, {} required: one peer cannot show the tip it hides",
            peers.len(),
            input.limits.min_peers
        ));
    }

    let established = |chain: &VerifiedChainV1, w: &Weighed, refused: Vec<Hash64>, dns_gate: L2DnsGateV1| L2VerdictV1::Established {
        chosen: chain.clone(),
        attested: w.att.block,
        attested_daa: w.att.block_daa,
        attested_root: w.att.committed_root,
        attested_opening: w.opening,
        issuer: w.att.issuer(),
        issued_at_daa: w.att.issued_at_daa,
        refused,
        dns_gate,
    };

    // One chain: the newest attested block on it, within the lag bound.
    if candidates.len() == 1 {
        let chain = &candidates[0];
        let newest = weighed.iter().filter(|w| chain.contains(&w.att.block)).max_by_key(|w| chain.index_of(&w.att.block));
        let Some(w) = newest else {
            return unverified("no fresh attestation of a block on the chain".into());
        };
        let lag = chain.tip.daa_score.saturating_sub(w.att.block_daa);
        if lag > input.limits.max_attested_lag_daa {
            return unverified(format!(
                "the tip is {lag} DAA past the attested block, more than {}: the transition after it is not attested",
                input.limits.max_attested_lag_daa
            ));
        }
        return Ok(established(chain, w, Vec::new(), L2DnsGateV1::NotNeeded));
    }

    // ADR-0176 D3: where a bond-budget allocation may be in force the comparator reads its budget-capped weights, which this build
    // does not read — named before anything is weighed. (The fence only moves forward, so a tip's DAA is the latest it can start.)
    if candidates.iter().any(|c| ForkChoiceRulesV1::active(input.rules.bond_budget, c.tip.daa_score)) {
        return Err(L2StopV1::BondBudgetAllocation(
            "this build reads no allocation version; the weights the node compares there are the allocation's",
        ));
    }

    // Competing tips: each must be weighed AT its tip.
    let mut at_tip: Vec<(&VerifiedChainV1, &Weighed)> = Vec::new();
    for chain in &candidates {
        match weighed.iter().find(|w| w.att.block == chain.tip_hash()) {
            Some(w) => at_tip.push((chain, w)),
            None => return Err(L2StopV1::Unweighable(chain.tip_hash())),
        }
    }

    // The DNS BFT gate refuses only a candidate abandoning a confirmed anchor in its Active stage. In Bootstrap, or with nothing
    // confirmed, it never refuses (testnet-12 today). An anchor at or above the checkpoint must stand on every candidate's verified
    // selected chain; one below it binds every candidate alike.
    let tip_daas: Vec<u64> = at_tip.iter().map(|(c, _)| c.tip.daa_score).collect();
    let checkpoint_daa = candidates[0].points[0].daa_score;
    let mut anchors: Vec<Hash64> = Vec::new();
    let mut dns_gate = L2DnsGateV1::NotRunning;
    if tip_daas.iter().any(|d| input.rules.dns_gate_may_run(*d)) {
        dns_gate = L2DnsGateV1::NeverRefuses;
        for (_, w) in &at_tip {
            let Some(fact) = w.att.dns_gate else {
                return Err(L2StopV1::DnsGateMayDecide("an attestation carries no DNS gate facts"));
            };
            if let (true, Some((anchor, anchor_daa))) = (fact.stage_active, fact.confirmed_anchor) {
                dns_gate = L2DnsGateV1::AnchorOnEveryCandidate;
                if anchor_daa >= checkpoint_daa {
                    if !at_tip.iter().all(|(c, _)| c.contains(&anchor)) {
                        return Err(L2StopV1::DnsGateMayDecide("a confirmed DNS-final anchor stands on one side only"));
                    }
                    anchors.push(anchor);
                }
            }
        }
    }

    // Each candidate's path, verified as its selected chain from the tip down to the deepest fork point it takes part in (and to every
    // anchor it must show) — the fork point, the seal, the anchor's side and D2's base are read only off walked paths.
    let mut walked: Vec<Candidate<'_>> = Vec::with_capacity(at_tip.len());
    for (i, (chain, w)) in at_tip.iter().enumerate() {
        let mut low = chain.points.len() - 1;
        for (j, (other, _)) in at_tip.iter().enumerate() {
            if i != j {
                low = low.min(fork_index(chain, other));
            }
        }
        for anchor in &anchors {
            low = low.min(chain.index_of(anchor).unwrap_or(0));
        }
        let path = verify_selected_chain_v1(chain, low, chain.points.len() - 1, input.chain_openings, input.rules.commitment)
            .map_err(|why| L2StopV1::SelectedChainUnverified { tip: chain.tip_hash(), why })?;
        walked.push(Candidate { chain, weighed: w, low, walked: path });
    }

    // The finality seal: past it, nodes never weigh the other side. The fork points' DAAs join the tips' for the fence variants.
    let mut daas = tip_daas.clone();
    for (i, c) in walked.iter().enumerate() {
        for o in walked.iter().skip(i + 1) {
            let fork = c.chain.points[fork_index(c.chain, o.chain)];
            daas.push(fork.daa_score);
            for x in [c, o] {
                let depth = x.chain.tip.blue_score.saturating_sub(fork.blue_score);
                if depth >= input.rules.finality_depth {
                    return Err(L2StopV1::Sealed { tip: x.chain.tip_hash(), depth, finality: input.rules.finality_depth });
                }
            }
        }
    }

    // A comparator the v1 leaf cannot feed decides here: the openings do not carry its inputs.
    if daas.iter().any(|d| ForkChoiceRulesV1::active(input.rules.rule_e, *d)) {
        return Err(L2StopV1::LeafV1Insufficient(
            "ADR-0178 rule E reads inputs the v1 leaf does not carry; its leaf (v2) ships under its fence",
        ));
    }

    // The unique robust winner, by the node's own functions.
    let winners: Vec<usize> = (0..walked.len())
        .filter(|&i| {
            (0..walked.len())
                .all(|j| i == j || robustly_dominates_v1(&walked[i].weighed.order, &walked[j].weighed.order, input.rules, &daas))
        })
        .collect();
    let [win] = winners.as_slice() else {
        return Err(L2StopV1::NoRobustWinner);
    };
    let winner = &walked[*win];
    // The comparator's maximum is the same tip (a consistency assertion: the functions above are its faces).
    if select_palw_tip_hash_v2(walked.iter().map(|c| (c.weighed.order.candidate, c.weighed.order)))
        != Some(winner.weighed.order.candidate)
    {
        return Err(L2StopV1::NoRobustWinner);
    }

    // ADR-0065 D2 may still veto the winner's reorg where it is armed. The bond registry is append-only (ADR-0065 D5), so the bonds
    // minted on the winner's branch after its fork with `o` are exactly its count minus the fork block's — both read off verified
    // leaves; below the quorum threshold no panel can be seated from them.
    if daas.iter().any(|d| ForkChoiceRulesV1::active(input.rules.frontier_provenance, *d)) {
        let Some(panel) = input.rules.panel else {
            return Err(L2StopV1::FrontierProvenance("the ruleset names no panel parameters to bound a quorum with"));
        };
        let tip_bonds = winner.weighed.opening.leaf.bonds_len;
        for (j, o) in walked.iter().enumerate() {
            if j == *win {
                continue;
            }
            let Some(base) = winner.leaf_at(fork_index(winner.chain, o.chain)) else {
                return Err(L2StopV1::FrontierProvenance("the fork block's leaf is not on the winner's verified path"));
            };
            let minted = tip_bonds.saturating_sub(base.bonds_len) as usize;
            if palw_minted_seats_can_reach_quorum_v1(minted, &panel) {
                return Err(L2StopV1::FrontierProvenance("enough bonds were minted after the fork to seat a quorum"));
            }
        }
    }
    let refused = walked.iter().enumerate().filter(|(i, _)| i != win).map(|(_, c)| c.chain.tip_hash()).collect();
    Ok(established(winner.chain, winner.weighed, refused, dns_gate))
}

/// **L3 under L2**: the ADR-0043 root a collection proof at `proof_header` is checked against.
///
/// Only for a header on the CHOSEN chain at or before the attested block's child:
/// * the attested block's child must commit exactly the attested root, and the attested opening's inner root is returned;
/// * a header at or below the attested block must be on the attested block's VERIFIED selected chain — the openings from the attested
///   block down to the header's parent are walked ([`verify_selected_chain_v1`]), and the header's parent's opening (which the header's
///   root commits) gives the inner root. A header L1 accepted on a path through a merged block or a non-selected parent fails the walk:
///   its root was never checked by anyone.
///
/// Below the commitment fence no leaf names a selected parent, so L3 under L2 does not exist there (the C1 checkpoint rule remains).
pub fn l3_root_under_l2_v1(
    verdict: &L2VerdictV1,
    proof_header: &Header,
    chain_openings: &[PalwForkChoiceOpeningV1],
    rules: &ForkChoiceRulesV1,
) -> Result<Hash64, String> {
    let L2VerdictV1::Established { chosen, attested, attested_root, attested_opening, .. } = verdict else {
        return Err("L2 is not established: a proof proves a row under a root, not that the root's chain is canonical".into());
    };
    if kaspa_consensus_core::hashing::header::hash(proof_header) != proof_header.hash {
        return Err("the proof's header does not hash to itself".into());
    }
    let Some(at) = chosen.index_of(&proof_header.hash) else {
        return Err(format!(
            "the proof's header {} is not on the chosen chain: a correct proof of a non-canonical state proves nothing here",
            proof_header.hash
        ));
    };
    let attested_at = chosen.index_of(attested).ok_or("the attested block left the chosen chain")?;
    if at > attested_at + 1 {
        return Err(format!(
            "the proof's header {} is past the attested block's child: its transition is not attested",
            proof_header.hash
        ));
    }
    if at == attested_at + 1 {
        if proof_header.palw_state_root != *attested_root {
            return Err(format!(
                "the proof's header {} is the attested block's child and commits {}, not the attested root {attested_root}",
                proof_header.hash, proof_header.palw_state_root
            ));
        }
        return Ok(attested_opening.inner_root);
    }
    if at == 0 {
        return Err("the checkpoint's own header commits a state before the view: prove at a later header".into());
    }
    let walked = verify_selected_chain_v1(chosen, at - 1, attested_at, chain_openings, rules.commitment).map_err(|why| {
        format!("the proof's header {} is not on the attested block's verified selected chain: {why}", proof_header.hash)
    })?;
    Ok(walked[0].inner_root)
}

#[cfg(test)]
mod tests;
