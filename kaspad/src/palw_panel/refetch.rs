//! **RFC-0007 Part II §II.8, the node's half: a failed block is refetched, recomputed, and accused or cleared.**
//!
//! When the sketch check of a weight product fails, [`misaka_palw_tir_sketch::TirCheckFailureV1::blocks`] names the free-axis blocks whose own
//! check fails. This module carries the seat's side of getting those blocks' weight bytes — **at most `F` = 2 MiB each** — from the producer or
//! any holder of the class, over the interval lane's existing request/answer messages (request kind: bit 28,
//! `kaspa_consensus_core::palw_weight_block_v1`), and hands them to the checker's escalation, which recomputes and concludes.
//!
//! * **Serving** ([`palw_weight_block_serve_v1`]): any node holding the class opens the inventory leaves that cover a block's byte ranges as one
//!   multiproof against the class's `artifact_root`, inside the lane's cap. Authentication, freshness, throttle and byte allowance are the lane's.
//! * **Accepting** ([`palw_weight_block_accept_v1`]): an answer is verified against the whole `artifact_root` and read only for the param and
//!   byte ranges the request named; anything else is refused and counts as withheld.
//! * **Pursuing** ([`WeightRefetchV1`]): asked once per block, answered from the interval pool a tick later, concluded by the escalation: an
//!   accusation names a committed row (the named leaf the existing IR court takes up), a cleared check says so, and a block nobody served by the
//!   patience is named in `Unavailable`. A cone node's weight that is not held is asked for the same way, up to [`REFETCH_ROUNDS_V1`] rounds.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::{PalwArtifactMultiproofV1, verify_artifact_multiproof_v1};
use kaspa_consensus_core::palw_weight_block_v1::palw_weight_block_request_index_v1;
use misaka_palw_sdk::lineage::PalwTirClassEntryV1;
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir_sketch::refetch::{TirBlockAddressV1, tir_block_address_v1, tir_weight_sites_v1};
use misaka_palw_tir_sketch::{TirBlockTransportV1, TirCheckFailureV1, TirEscalationV1, TirFetchRefusalV1, TirSketchAnalysisV1};

/// Rounds of asking for a cone node's weight after the failing blocks.
pub(crate) const REFETCH_ROUNDS_V1: u32 = 4;
/// DAA a seat waits for a block before it names it `Unavailable` (a node patience; the lane's own re-ask window is shorter).
pub(crate) const REFETCH_PATIENCE_DAA_V1: u64 = 12;
/// Open pursuits a node keeps.
pub(crate) const REFETCH_PURSUITS_CAP_V1: usize = 256;

/// **The serving half**: the multiproof (borsh) of the inventory leaves covering block `block` of weight site `ordinal` of `entry`'s class.
/// `None` for a block that has no address (routed, batched, derived, out of range), a leaf the artifact cannot read, or an answer past the lane's cap.
pub(crate) fn palw_weight_block_serve_v1(entry: &PalwTirClassEntryV1, ordinal: u32, block: u32) -> Option<Vec<u8>> {
    let artifact = &entry.artifact;
    let plan = artifact.plan();
    let analysis = TirSketchAnalysisV1::of(&plan.program);
    let address = tir_block_address_v1(plan, &analysis, ordinal, block)?;
    let tree = artifact.inventory_tree().ok()?;
    let index = tree.index();
    let mut leaves: BTreeSet<u32> = BTreeSet::new();
    for (start, len) in &address.ranges {
        if *len == 0 {
            return None;
        }
        let (first, last) =
            (index.leaf_of(address.param, address.layer, *start)?, index.leaf_of(address.param, address.layer, start + len - 1)?);
        leaves.extend(first..=last);
    }
    let leaves: Vec<u32> = leaves.into_iter().collect();
    let proof = tree.multiproof_with(&plan.program, artifact.as_ref(), &leaves)?;
    let bytes = borsh::to_vec(&proof).ok()?;
    (bytes.len() <= kaspa_p2p_flows::palw_gossip::PALW_INTERVAL_OPENING_MAX_BYTES).then_some(bytes)
}

/// **The accepting half**: `bytes` decoded as a multiproof, verified against `artifact_root`, and read as pieces `(byte offset in the instance,
/// bytes)` of exactly the instance `address` names. Refused if it does not verify, opens a leaf of another tensor or layer, or does not cover
/// every byte range of the block.
pub(crate) fn palw_weight_block_accept_v1(
    bytes: &[u8],
    artifact_root: Hash64,
    program: &TirProgramV1,
    address: &TirBlockAddressV1,
) -> Result<Vec<(u64, Vec<u8>)>, String> {
    if bytes.len() > kaspa_p2p_flows::palw_gossip::PALW_INTERVAL_OPENING_MAX_BYTES {
        return Err("an answer past the lane's cap".into());
    }
    let proof: PalwArtifactMultiproofV1 = borsh::from_slice(bytes).map_err(|e| format!("not a multiproof: {e}"))?;
    verify_artifact_multiproof_v1(&proof, artifact_root).map_err(|e| format!("the multiproof does not open against the artifact root: {e:?}"))?;
    let decl = program.params.get(address.param as usize).ok_or("the request names no param")?;
    let mut pieces = Vec::with_capacity(proof.opened.len());
    for (_, operand) in &proof.opened {
        if operand.tensor_name != decl.name || operand.layer != address.layer {
            return Err("an opened leaf belongs to another tensor".into());
        }
        pieces.push((u64::from(operand.row_start), operand.bytes.clone()));
    }
    pieces.sort_by_key(|(start, _)| *start);
    if !covers(&pieces, &address.ranges) {
        return Err("the answer does not cover the block".into());
    }
    Ok(pieces)
}

/// Whether `pieces` (sorted by offset) cover every `(start, len)` of `ranges`.
fn covers(pieces: &[(u64, Vec<u8>)], ranges: &[(u64, u64)]) -> bool {
    ranges.iter().all(|(start, len)| {
        let end = start + len;
        let mut at = *start;
        for (p, bytes) in pieces {
            let p_end = p + bytes.len() as u64;
            if *p <= at && p_end > at {
                at = p_end;
            }
            if at >= end {
                return true;
            }
        }
        at >= end
    })
}

/// What the seat has been served so far: verified pieces by instance. A block or a whole instance not covered is withheld (and a whole instance
/// asked for is remembered, for the next round).
pub(crate) struct HeldBlocksV1<'p> {
    program: &'p TirProgramV1,
    pieces: BTreeMap<(u16, Option<u16>), Vec<(u64, Vec<u8>)>>,
    wanted_params: RefCell<BTreeSet<(u16, Option<u16>)>>,
}

impl<'p> HeldBlocksV1<'p> {
    fn new(program: &'p TirProgramV1, pieces: &BTreeMap<(u16, Option<u16>), Vec<(u64, Vec<u8>)>>) -> Self {
        Self { program, pieces: pieces.clone(), wanted_params: RefCell::new(BTreeSet::new()) }
    }
}

impl TirBlockTransportV1 for HeldBlocksV1<'_> {
    fn fetch_ranges(&self, param: u16, layer: Option<u16>, ranges: &[(u64, u64)]) -> Result<Vec<(u64, Vec<u8>)>, TirFetchRefusalV1> {
        let held = self.pieces.get(&(param, layer)).ok_or(TirFetchRefusalV1::Withheld)?;
        if !covers(held, ranges) {
            return Err(TirFetchRefusalV1::Withheld);
        }
        Ok(held.clone())
    }

    fn fetch_param(&self, param: u16, layer: Option<u16>) -> Result<Vec<u8>, TirFetchRefusalV1> {
        let decl = self.program.params.get(param as usize).ok_or_else(|| TirFetchRefusalV1::Malformed("no such param".into()))?;
        let total = decl.shape.iter().map(|x| u64::from(*x)).product::<u64>() * decl.dtype.width() as u64;
        if let Some(held) = self.pieces.get(&(param, layer))
            && covers(held, &[(0, total)])
        {
            let mut out = vec![0u8; total as usize];
            for (start, bytes) in held {
                let s = *start as usize;
                if s < out.len() {
                    let n = bytes.len().min(out.len() - s);
                    out[s..s + n].copy_from_slice(&bytes[..n]);
                }
            }
            return Ok(out);
        }
        self.wanted_params.borrow_mut().insert((param, layer));
        Err(TirFetchRefusalV1::Withheld)
    }
}

/// What a pursuit asks the node to do.
#[derive(Debug)]
pub(crate) enum RefetchStepV1 {
    /// Send these requests now: `(interval index, address)`.
    Ask(Vec<(u32, TirBlockAddressV1)>),
    /// Still waiting for answers.
    Waiting,
    /// The pursuit is over.
    Done(TirEscalationV1),
}

/// **One failed check's refetch** — created by whoever holds the failure and the witness (a served witness the sketch checker refused), stepped
/// once per tick with the interval pool.
pub(crate) struct WeightRefetchV1 {
    pub claim: Hash64,
    pub artifact_root: Hash64,
    failure: TirCheckFailureV1,
    pieces: BTreeMap<(u16, Option<u16>), Vec<(u64, Vec<u8>)>>,
    /// The requests of this round not yet answered, and the ones answered.
    wanted: Vec<(u32, TirBlockAddressV1)>,
    accepted: BTreeSet<u32>,
    asked_daa: Option<u64>,
    round: u32,
}

impl WeightRefetchV1 {
    pub(crate) fn new(claim: Hash64, artifact_root: Hash64, failure: TirCheckFailureV1) -> Self {
        Self {
            claim,
            artifact_root,
            failure,
            pieces: BTreeMap::new(),
            wanted: Vec::new(),
            accepted: BTreeSet::new(),
            asked_daa: None,
            round: 0,
        }
    }

    fn requests_for(
        entry: &PalwTirClassEntryV1,
        ordinal: u32,
        blocks: impl IntoIterator<Item = u32>,
    ) -> Option<Vec<(u32, TirBlockAddressV1)>> {
        let plan = entry.artifact.plan();
        let analysis = TirSketchAnalysisV1::of(&plan.program);
        blocks
            .into_iter()
            .map(|b| Some((palw_weight_block_request_index_v1(ordinal, b)?, tir_block_address_v1(plan, &analysis, ordinal, b)?)))
            .collect()
    }

    /// **One tick.** `openings` is the node's interval pool; `escalate` runs the checker's escalation over a transport (blocking work — the caller
    /// decides the thread). Returns what to do next.
    pub(crate) fn step(
        &mut self,
        entry: &PalwTirClassEntryV1,
        openings: &HashMap<(Hash64, u32), Vec<Vec<u8>>>,
        daa: u64,
        escalate: &dyn Fn(&dyn TirBlockTransportV1, &TirCheckFailureV1) -> TirEscalationV1,
    ) -> RefetchStepV1 {
        let plan = entry.artifact.plan();
        let analysis = TirSketchAnalysisV1::of(&plan.program);
        let sites = tir_weight_sites_v1(plan, &analysis);
        let Some(node) = self.failure.node else {
            return RefetchStepV1::Done(TirEscalationV1::Inconclusive("the failure names no node".into()));
        };
        let Some(ordinal) = sites.iter().position(|s| *s == (self.failure.occurrence, node)).map(|o| o as u32) else {
            return RefetchStepV1::Done(TirEscalationV1::NotBlockAddressable("the failed node is not a weight product".into()));
        };
        let Some(asked) = self.asked_daa else {
            // Round zero: the blocks the check named.
            let named: Vec<u32> = self.failure.blocks.clone();
            let Some(wanted) = Self::requests_for(entry, ordinal, named) else {
                return RefetchStepV1::Done(TirEscalationV1::NotBlockAddressable("the failed node's weight has no block address".into()));
            };
            if wanted.is_empty() {
                return RefetchStepV1::Done(TirEscalationV1::NotBlockAddressable("the failure names no block".into()));
            }
            self.asked_daa = Some(daa);
            self.wanted = wanted.clone();
            return RefetchStepV1::Ask(wanted);
        };
        // Read the answers that arrived.
        for (index, address) in &self.wanted {
            if self.accepted.contains(index) {
                continue;
            }
            for bytes in openings.get(&(self.claim, *index)).into_iter().flatten() {
                if let Ok(pieces) = palw_weight_block_accept_v1(bytes, self.artifact_root, &plan.program, address) {
                    self.pieces.entry((address.param, address.layer)).or_default().extend(pieces);
                    self.accepted.insert(*index);
                    break;
                }
            }
        }
        if self.wanted.iter().all(|(index, _)| self.accepted.contains(index)) {
            let held = HeldBlocksV1::new(&plan.program, &self.pieces);
            let outcome = escalate(&held, &self.failure);
            let wanted_params: Vec<(u16, Option<u16>)> = held.wanted_params.borrow().iter().copied().collect();
            return match outcome {
                TirEscalationV1::Unavailable { param: Some(j), .. } if self.round < REFETCH_ROUNDS_V1 => {
                    // A cone node's weight: ask for every block of the site that reads it, then run again.
                    let again = wanted_params.iter().find(|(p, _)| *p == j).copied();
                    let site = (0..sites.len() as u32).find(|o| tir_block_address_v1(plan, &analysis, *o, 0).is_some_and(|a| a.param == j));
                    match (again, site) {
                        (Some(_), Some(o)) => {
                            let blocks = tir_block_address_v1(plan, &analysis, o, 0).map(|a| a.blocks).unwrap_or(1);
                            match Self::requests_for(entry, o, 0..blocks) {
                                Some(wanted) => {
                                    self.round += 1;
                                    self.asked_daa = Some(daa);
                                    self.wanted = wanted.clone();
                                    self.accepted.clear();
                                    RefetchStepV1::Ask(wanted)
                                }
                                None => RefetchStepV1::Done(outcome),
                            }
                        }
                        _ => RefetchStepV1::Done(outcome),
                    }
                }
                other => RefetchStepV1::Done(other),
            };
        }
        if daa >= asked.saturating_add(REFETCH_PATIENCE_DAA_V1) {
            let first = self.wanted.iter().find(|(index, _)| !self.accepted.contains(index)).map(|(_, a)| a.block);
            return RefetchStepV1::Done(TirEscalationV1::Unavailable { occurrence: self.failure.occurrence, node, block: first, param: None });
        }
        RefetchStepV1::Waiting
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_panel::tir_court_e2e::ir_class;
    use kaspa_consensus_core::palw_tir_artifact_v1::PalwTirTensorSourceV1;

    /// The class, its first plain weight site, and the entry that serves it.
    fn class_and_site() -> (crate::palw_panel::tir_court_e2e::Ir, PalwTirClassEntryV1, u32) {
        let ir = ir_class("refetch", false);
        let entry = ir.registry.tir_entry_v1(ir.class_id, ir.root).expect("the held IR class");
        let plan = entry.artifact.plan();
        let analysis = TirSketchAnalysisV1::of(&plan.program);
        let ordinal = (0..tir_weight_sites_v1(plan, &analysis).len() as u32)
            .find(|o| tir_block_address_v1(plan, &analysis, *o, 0).is_some())
            .expect("a plain weight site");
        (ir, entry, ordinal)
    }

    #[test]
    fn a_served_block_opens_against_the_root_and_a_forged_one_does_not() {
        let (ir, entry, ordinal) = class_and_site();
        let plan = entry.artifact.plan();
        let analysis = TirSketchAnalysisV1::of(&plan.program);
        let address = tir_block_address_v1(plan, &analysis, ordinal, 0).unwrap();
        let served = palw_weight_block_serve_v1(&entry, ordinal, 0).expect("a holder serves the block");
        let pieces = palw_weight_block_accept_v1(&served, ir.root, &plan.program, &address).expect("it opens against the artifact root");
        // The bytes are the artifact's own at those offsets.
        let whole = entry.artifact.tensor_bytes(address.param, address.layer).expect("the instance");
        for (start, bytes) in &pieces {
            assert_eq!(&whole[*start as usize..*start as usize + bytes.len()], &bytes[..]);
        }
        assert!(
            palw_weight_block_accept_v1(&served, Hash64::from_bytes([0x5A; 64]), &plan.program, &address).is_err(),
            "against another root it is refused"
        );
        let mut forged: PalwArtifactMultiproofV1 = borsh::from_slice(&served).unwrap();
        forged.opened[0].1.bytes[0] ^= 1;
        assert!(palw_weight_block_accept_v1(&borsh::to_vec(&forged).unwrap(), ir.root, &plan.program, &address).is_err(), "a flipped byte is refused");
        let other = TirBlockAddressV1 { param: address.param.wrapping_add(1) % plan.program.params.len() as u16, ..address.clone() };
        if other.param != address.param {
            assert!(palw_weight_block_accept_v1(&served, ir.root, &plan.program, &other).is_err(), "another tensor's leaves are refused");
        }
        assert!(palw_weight_block_serve_v1(&entry, ordinal, 1).is_none(), "a block past the count has no answer");
        assert!(palw_weight_block_serve_v1(&entry, u32::MAX >> 16, 0).is_none(), "nor a site past the sites");
    }

    #[test]
    fn a_pursuit_asks_once_reads_the_answer_and_concludes_or_names_the_withheld_block() {
        let (ir, entry, ordinal) = class_and_site();
        let plan = entry.artifact.plan();
        let analysis = TirSketchAnalysisV1::of(&plan.program);
        let sites = tir_weight_sites_v1(plan, &analysis);
        let (occurrence, node) = sites[ordinal as usize];
        let failure = TirCheckFailureV1 {
            pos: 3,
            occurrence,
            node: Some(node),
            fault: misaka_palw_tir_sketch::TirCheckFaultV1::Freivalds { modulus: 7 },
            blocks: vec![0],
        };
        let claim = Hash64::from_bytes([3; 64]);
        let verdict = TirEscalationV1::Accuse { pos: 3, occurrence, node, slot: 11 };
        let conclude = |_: &dyn TirBlockTransportV1, _: &TirCheckFailureV1| verdict.clone();

        // Answered: asks once, then concludes with the escalation's verdict once the block arrives.
        let mut p = WeightRefetchV1::new(claim, ir.root, failure.clone());
        let none = HashMap::new();
        let RefetchStepV1::Ask(asked) = p.step(&entry, &none, 100, &conclude) else { panic!("the first step asks") };
        assert_eq!(asked.len(), 1);
        assert!(matches!(p.step(&entry, &none, 101, &conclude), RefetchStepV1::Waiting), "no answer yet");
        let (index, address) = &asked[0];
        let served = palw_weight_block_serve_v1(&entry, ordinal, 0).unwrap();
        let mut pool = HashMap::new();
        pool.insert((claim, *index), vec![b"junk".to_vec(), served]);
        match p.step(&entry, &pool, 102, &conclude) {
            RefetchStepV1::Done(TirEscalationV1::Accuse { slot: 11, .. }) => {}
            other => panic!("{other:?}"),
        }
        assert_eq!(address.block, 0);

        // Withheld: nothing arrives, and past the patience the block is named.
        let mut p = WeightRefetchV1::new(claim, ir.root, failure.clone());
        assert!(matches!(p.step(&entry, &none, 200, &conclude), RefetchStepV1::Ask(_)));
        assert!(matches!(p.step(&entry, &none, 200 + REFETCH_PATIENCE_DAA_V1 - 1, &conclude), RefetchStepV1::Waiting));
        match p.step(&entry, &none, 200 + REFETCH_PATIENCE_DAA_V1, &conclude) {
            RefetchStepV1::Done(TirEscalationV1::Unavailable { occurrence: o, node: n, block: Some(0), .. }) => assert_eq!((o, n), (occurrence, node)),
            other => panic!("{other:?}"),
        }

        // A forged answer is no answer: it is the withheld case.
        let mut p = WeightRefetchV1::new(claim, ir.root, failure);
        let RefetchStepV1::Ask(asked) = p.step(&entry, &none, 300, &conclude) else { panic!() };
        let mut forged: PalwArtifactMultiproofV1 = borsh::from_slice(&palw_weight_block_serve_v1(&entry, ordinal, 0).unwrap()).unwrap();
        forged.opened[0].1.bytes[0] ^= 1;
        let mut pool = HashMap::new();
        pool.insert((claim, asked[0].0), vec![borsh::to_vec(&forged).unwrap()]);
        assert!(matches!(p.step(&entry, &pool, 300 + REFETCH_PATIENCE_DAA_V1, &conclude), RefetchStepV1::Done(TirEscalationV1::Unavailable { .. })));
    }
}
