//! ADR-0040 — the algo-4 mint interface between a node's mining loop and consensus.
//!
//! # Why this exists as a two-call interface
//!
//! Building an algo-4 block is not "fill in a template". Three of its fields are *consensus-derived*
//! and one is *secret to the miner*, and the derived ones cannot all be known before the template is
//! built:
//!
//! * `palw_target_daa_interval` must equal the block's own `daa_score` (clause 5), which GHOSTDAG fixes
//!   when the template is built.
//! * `palw_chain_commit` is a function of the finality-buried DNS anchor *and* that target interval
//!   (clause 6), so it cannot be computed before the interval is known.
//! * `bits` is the PALW lane's retarget over the DAA window (§16.3), not a miner choice.
//! * The ticket nullifier is the miner's secret, chosen at leaf-registration time and stored off-chain;
//!   consensus cannot supply it.
//!
//! The eligibility draw needs the first two, so the miner cannot draw before consensus has fixed them —
//! and consensus cannot stamp the ticket before the miner has drawn. [`PalwAlgo4MintFacts`] breaks that
//! cycle: consensus derives and publishes the frozen inputs, the miner evaluates its own tickets against
//! them, and the resulting [`PalwAlgo4Stamp`] goes back for the template to build against. The template
//! call RE-DERIVES every value in the stamp and refuses if any of them moved (a new sink, a new
//! interval) rather than building a block that would be rejected.
//!
//! # Nothing here is trusted
//!
//! Every field of [`PalwAlgo4Stamp`] except `ticket_nullifier` is re-derived by consensus and compared.
//! `ticket_nullifier` cannot be re-derived — it is the secret — but it is not trusted either: the
//! template pins `nonce = low64(ticket_nullifier)` itself (I-3) so a producer cannot grind the nonce,
//! and the clause-1 commitment check at body validation is what actually decides whether the nullifier
//! belongs to the leaf. A producer that lies here gets a block its own node rejects.

use crate::BlockHash;
use crate::palw::PalwPublicLeafV1;
use kaspa_hashes::Hash64;

/// Unsigned algo-4 block plus the exact consensus-derived Header-v4 stamp target. A v3/inert
/// network returns zero bits and performs no objective-stamp grind.
pub struct PalwAlgo4Template {
    pub block: crate::block::MutableBlock,
    pub spam_target_bits: u16,
}

/// The frozen, consensus-derived inputs a miner needs to evaluate its tickets for one interval.
///
/// Derived off the current sink, which is the selected parent the minted block will have — the same
/// coordinate body validation resolves the ticket against. All of it is read-only: producing these
/// facts writes nothing, seeds nothing, and is safe to call on any network.
#[derive(Clone, Debug)]
pub struct PalwAlgo4MintFacts {
    /// `params.net.suffix()` — the network number every PALW preimage binds.
    pub network_id: u32,
    /// The sink these facts were derived from. The template call refuses if the sink has since moved,
    /// because a different selected parent means a different anchor, view and interval.
    pub sink: BlockHash,
    /// The finality-buried anchor's retained `palw_beacon_seed` — the clause-9 lagged `R_E`.
    pub beacon_seed: Hash64,
    /// `chain_commit(anchor, dns_cert, target_interval, net)` — the clause-6 expected value.
    pub chain_commit: Hash64,
    /// The GHOSTDAG-fixed interval this draw targets; clause 5 pins it equal to the block's `daa_score`.
    pub target_daa_interval: u64,
    /// The PALW lane's retargeted `bits` (§16.3), derived through the same code the header stage runs.
    /// On a registry-active net this is the §12 PER-SET value for `compute_set_id`'s sublane.
    pub replica_bits: u32,
    /// ADR-MA §13.1 — the Header-v5 Compute Set references the template stamps: the leaf's
    /// committed set and its governing policy/plan at `target_daa_interval`, resolved from the
    /// sink's registry view. All-zero below the registry fence (every shipped preset).
    pub compute_set_id: Hash64,
    pub compute_policy_id: Hash64,
    pub allocation_plan_id: Hash64,
    /// The PALW epoch of the target interval — the coordinate the batch's block-eligibility is judged at.
    pub epoch: u64,
    /// The certificate hash the batch's view carries, to be stamped into the header.
    pub epoch_certificate_hash: Hash64,
    /// The leaf as it exists ON CHAIN, read from `palw_store`. Never fabricated: if the batch/leaf is
    /// absent the facts call fails rather than inventing provenance.
    pub leaf: PalwPublicLeafV1,
    /// `palw_template_lane_open(...)` — false when the beacon is Halted or the buried carry run has
    /// exceeded the grace window. Mining while false produces blocks clause 10 rejects, i.e. a node
    /// bricking its own lane, so a miner MUST stop rather than draw.
    pub lane_open: bool,
}

/// What the producer hands back for the template to build against.
///
/// Every field except `ticket_nullifier` is re-derived and compared by
/// `palw_build_algo4_template`; they are carried here so the comparison exists at all, not because
/// consensus needs the producer to tell it. Treat this as an assertion of what the producer *believed*,
/// which consensus then checks against what is *true*.
#[derive(Clone, Debug)]
pub struct PalwAlgo4Stamp {
    /// The sink the facts were derived from (staleness check).
    pub sink: BlockHash,
    pub batch_id: Hash64,
    pub leaf_index: u32,
    /// The miner's secret, disclosed here because the winning header discloses it (I-13 ends at mint).
    /// The template derives `nonce` from it rather than accepting a nonce.
    pub ticket_nullifier: Hash64,
    pub epoch_certificate_hash: Hash64,
    pub chain_commit: Hash64,
    pub target_daa_interval: u64,
    pub proof_type: u8,
    pub replica_bits: u32,
}

/// Why an algo-4 mint could not proceed.
///
/// The split matters operationally. `NotReady` is the normal case — no anchor yet, the lane is closed,
/// the sink moved between the two calls, no ticket won this interval — and a mining loop should log it
/// quietly and try again next tick. `Fault` means the node is misconfigured or the producer lied, and
/// should be loud. The previous service classified these by matching on error strings, which silently
/// reclassifies whenever a message is reworded.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwMintError {
    /// Expected, transient: try again next interval.
    #[error("PALW algo-4 mint not ready: {0}")]
    NotReady(String),
    /// Unexpected: configuration error, missing provenance, or a producer-supplied value that does not
    /// survive re-derivation.
    #[error("PALW algo-4 mint fault: {0}")]
    Fault(String),
}

impl PalwMintError {
    pub fn not_ready(m: impl Into<String>) -> Self {
        Self::NotReady(m.into())
    }
    pub fn fault(m: impl Into<String>) -> Self {
        Self::Fault(m.into())
    }
    /// True for the quiet, expected outcomes a mining loop hits on most ticks.
    pub fn is_not_ready(&self) -> bool {
        matches!(self, Self::NotReady(_))
    }
}
