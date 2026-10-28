//! **Can a public prosecution of a flat-v5 candidate statement FINISH inside its liability horizon?** (RFC-0002 item 3, lane
//! RFC02-T, 2026-10-11) — G14-B's clock and carriage ([`crate::budget`]) applied to the flat-v5 hash-unit DA route
//! (`consensus-core::palw_flat_model_vector_da_v3`), so the candidate route's deadlines are derived from the same constants as the
//! claim route's, not invented.
//!
//! **The pursuit.** A fresh verifier holds the model and the public job; it replays, then localizes through public units only:
//!
//! 1. the first divergent position inside its segment — a node opening carries its position's path, so each round halves the
//!    segment: `⌈log2 min(P, SEG_LEN_V4)⌉` rounds (P = 1: none);
//! 2. the first divergent node of that position — the opening's node path halves the node set: `⌈log2 N⌉` rounds (N = committed
//!    nodes per position);
//! 3. the first divergent 64-element leaf of a large node — a hash-leaf opening's path halves the leaves: `⌈log2 L⌉` rounds (L = the
//!    most leaves any committed value has at the worst history);
//! 4. one round more for the court's operands, then the protected filing: its seal matures (`claim_seal_delay_daa`, the RFC-0002 P0
//!    rule), the court is assembled and the proof's chunks land.
//!
//! Each round is one demand answered at the LAST DAA of its window by a stalling poster: a reply is at most
//! `FLAT_VECTOR_ANSWER_MAX_BYTES_V3` (10 KiB, one route chunk), so the window is the base court deadline
//! ([`crate::budget::response_window_daa_v1`] with one chunk). Rounds are counted binary although a requester holds four slots — an
//! upper bound, never a measurement. Verifier compute enters only through the OPV policy's declared budgets.

use misaka_palw_tir::program::TirProgramV1;

use crate::budget::{
    FILE_PROOF_ENVELOPE_BYTES_V1, PursuitClockV1, ROUTE_DAA_PER_CHUNK_V1, response_window_daa_v1, route_object_carriable_v1,
    route_object_chunks_v1,
};
use crate::gate::ProsecutionGapV1;
use crate::ledger::LedgerPolicyV1;
use crate::opv::OpvPolicyV1;

/// The largest flat-v5 hash-unit reply (mirrors consensus-core's `FLAT_VECTOR_ANSWER_MAX_BYTES_V3`).
pub const FLAT_ANSWER_MAX_BYTES_V5: u64 = 10 << 10;

/// **A flat-v5 candidate's worst-case pursuit**, DAA from its posting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlatPursuitV5 {
    pub position_rounds: u32,
    pub node_rounds: u32,
    pub leaf_rounds: u32,
    /// Every localization round plus the operands' round.
    pub rounds: u32,
    pub response_window_daa: u64,
    pub filing_chunks: u64,
    /// When the filing's last chunk lands under the worst case (posting = 0).
    pub filed_daa: u64,
}

impl FlatPursuitV5 {
    pub fn fits(&self, horizon_daa: u64) -> bool {
        self.filed_daa < horizon_daa
    }
}

fn ceil_log2(n: u64) -> u32 {
    if n <= 1 { 0 } else { 64 - (n - 1).leading_zeros() }
}

/// **Derive the worst case** for `program` at `positions` (the candidate's plan), with the scope's worst filing bytes, on the route's
/// ledger and OPV clocks; `Err` by name when the filing cannot ride the route at all.
pub fn flat_pursuit_v5(
    program: &TirProgramV1,
    positions: u32,
    worst_filing_bytes: u64,
    ledger: &LedgerPolicyV1,
    opv: &OpvPolicyV1,
) -> Result<FlatPursuitV5, ProsecutionGapV1> {
    let gap = |what: &'static str, required: u128, limit: u128| ProsecutionGapV1::Unbounded { what, required, limit };
    let filing = (worst_filing_bytes as u128).saturating_add(FILE_PROOF_ENVELOPE_BYTES_V1 as u128);
    let filing_chunks = route_object_chunks_v1(filing);
    if !route_object_carriable_v1(filing) {
        return Err(gap(
            "FLAT_PURSUIT: the worst flat filing does not ride the route's chunk lane (chunks)",
            filing_chunks as u128,
            16,
        ));
    }
    let w = crate::trace::WiringV1::new(program).map_err(|_| gap("FLAT_PURSUIT: the program does not validate", 1, 0))?;
    let nodes: u64 = w.occurrences.iter().map(|(b, _)| program.blocks[*b as usize].nodes.len() as u64).sum();
    let last = positions.saturating_sub(1);
    let mut leaves = 0u64;
    for (s, (b, _)) in w.occurrences.iter().enumerate() {
        let h = w.h(s as u16, last);
        for node in &program.blocks[*b as usize].nodes {
            let shape: Vec<u64> = node.out.resolve(h).iter().map(|d| *d as u64).collect();
            if let Some(l) = crate::merkle5::FlatLayoutV5::of(&shape) {
                leaves = leaves.max(l.leaves());
            }
        }
    }
    let position_rounds = ceil_log2(u64::from(positions.clamp(1, crate::seg::SEG_LEN_V4)));
    let node_rounds = ceil_log2(nodes);
    let leaf_rounds = ceil_log2(leaves);
    let rounds = position_rounds + node_rounds + leaf_rounds + 1;
    let clock = PursuitClockV1::of_opv(ledger, opv);
    let window = response_window_daa_v1(ledger.court_deadline_daa, route_object_chunks_v1(FLAT_ANSWER_MAX_BYTES_V5 as u128));
    let localized = clock.first_step_daa.saturating_add(u64::from(rounds).saturating_mul(window.saturating_add(clock.turnaround_daa)));
    // The protected filing (RFC-0002 P0): seal, wait out its maturity, then reveal.
    let filed_daa = localized
        .saturating_add(ledger.claim_seal_delay_daa)
        .saturating_add(clock.prove_daa)
        .saturating_add(filing_chunks.saturating_sub(1).saturating_mul(ROUTE_DAA_PER_CHUNK_V1));
    Ok(FlatPursuitV5 { position_rounds, node_rounds, leaf_rounds, rounds, response_window_daa: window, filing_chunks, filed_daa })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ceil_log2_counts_halvings() {
        assert_eq!([1, 2, 3, 4, 5, 1024, 1025].map(ceil_log2), [0, 1, 2, 2, 3, 10, 11]);
    }
}
