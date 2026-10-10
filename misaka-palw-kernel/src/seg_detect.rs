//! **Detection at real scale: the re-execution check (K2-TIR-v4)** — `docs/design/palw/k2-real-scale.md` §11 (SOUND's SG-06).
//!
//! The element courts bound what a prosecution costs once a fault is known (two positions of material, one filing). Finding the fault
//! is a separate question. Sampling positions and reading their material finds a one-element lie with probability `m / P` for `m`
//! sampled positions, and a whole-claim read is `P · M_pos` (19.2 TB at 9B-8k). This module is the other detector: **re-execute
//! and compare roots**.
//!
//! The reference semantics are exact integers. A verifier that re-executes a claim therefore computes every value an honest producer
//! must have committed. It needs the job's prompt (public on chain in tiles), the claim's delivered ids, the public artifact and the
//! class's program. It hashes its values into position roots and segment roots ([`segment_roots_of_position_roots_v1`]) and compares
//! them with the claim's on-chain segment roots:
//!
//! * **every root equal**: every committed value is the verifier's own (collision resistance). Only the decode relation is left, and
//!   the verifier checks it on its own logits ([`first_decode_mismatch_v1`]);
//! * **a segment differs**: the verifier descends that segment's tree with the producer's position paths. A probe is one position root
//!   and its ≤ 10 siblings: part 0 of a demand, or the stream's position-root list. The descent finds the **first** divergent position
//!   `q` with at most `⌈log2 S⌉` probes. Every position before `q` is the verifier's own, so every input of `q`'s first wrong value is
//!   right. `q` is then checked by the element courts ([`check_positions_v1`]) from the producer's material of `q − 1` and `q`, and that
//!   check returns a filing the court convicts (the completeness argument of `crate::element`).
//!
//! What the check reads: the artifact, the on-chain roots, at most `⌈log2 S⌉` position paths, and two positions of material. What it
//! costs: one re-execution of the claim, which is the producer's own work in the reference semantics. Within those it is not a sample:
//! any wrong committed value or wrong delivered id is found with certainty. The economics of who runs it, and for which claims, are
//! §11 of the design. A sublinear-read proof that would make it cheaper than re-execution is a DESIGN_GAP there.

use std::collections::BTreeMap;

use misaka_palw_tir::Tensor;

use crate::element::{SegClaimContextV1, SegFindingV1, SegMaterialV1, check_positions_v1};
use crate::hash::Digest;
use crate::seg::{
    position_in_segment, seg_node, segment_bounds_v1, segment_count_v1, segment_leaf_v1, segment_roots_of_position_roots_v1,
};

/// Replay directly from the authenticated job and the acquired registered model. The result is reused to localize differing segments.
/// The caller supplies fed ids (prompt + generated ids except the last), or the complete prompt for a v5 encoder. They are
/// checked against the public job before use. Only position roots and one decode-mismatch index survive streaming replay.
/// Producer values are read exclusively by the subsequent authenticated two-position court prover.
pub fn prepare_reexecution_v1(
    c: &SegClaimContextV1<'_>,
    artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
    tokens: &[u32],
) -> Result<OwnReplayV1, String> {
    let inconsistent = |why: String| Err(why);
    let Some(prompt) = tokens.get(..c.prompt_len as usize) else {
        return inconsistent("the public job's prompt is not in hand".into());
    };
    if crate::seg::prompt_root_of_ids_v1(prompt) != c.prompt_root
        || c.inline_prompt.is_some_and(|ids| ids != prompt)
        || prompt.iter().any(|t| *t >= c.program.token_bound)
    {
        return inconsistent("the supplied ids are not the authenticated public job".into());
    }
    let job_inputs = if let Some(e) = c.encoder {
        if tokens.len() != c.prompt_len as usize || c.positions != 1 || !c.generated.is_empty() {
            return inconsistent("the encoder replay does not have its one public position".into());
        }
        match e.inputs(prompt) {
            Ok((ids, count)) => Some((e.first_input, ids, count)),
            Err(why) => return inconsistent(why),
        }
    } else {
        if tokens.len() != c.positions as usize
            || c.generated.is_empty()
            || tokens.get(c.prompt_len as usize..) != Some(&c.generated[..c.generated.len() - 1])
        {
            return inconsistent("the replay's fed ids are not the public claim's".into());
        }
        None
    };
    struct RegisteredModel<'a> {
        params: &'a crate::trace::ParamCommitmentsV1,
        source: &'a dyn Fn(u16, Option<u16>) -> Option<Tensor>,
        job_inputs: Option<(u16, Tensor, Tensor)>,
        error: std::cell::RefCell<Option<String>>,
    }
    impl misaka_palw_tir::interp::ParamSource for RegisteredModel<'_> {
        fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
            if let Some((first, ids, count)) = &self.job_inputs {
                if index == *first {
                    return Some(ids.clone());
                }
                if index == *first + 1 {
                    return Some(count.clone());
                }
            }
            let t = (self.source)(index, layer)?;
            if !crate::verify::canonical_tensor_v1(&t)
                || self.params.by_instance.get(&(index, layer)) != Some(&crate::merkle3::tensor_commitment_v3(&t))
            {
                *self.error.borrow_mut() = Some(format!("param {index}/{layer:?} is not the registered model's"));
                return None;
            }
            Some(t)
        }
    }
    let registered = RegisteredModel { params: c.params, source: artifact, job_inputs, error: Default::default() };
    let mut roots = Vec::with_capacity(c.positions as usize);
    let mut decode_mismatch = None;
    let fed = if c.encoder.is_some() { &[0u32][..] } else { tokens };
    let result = crate::trace::trace_streaming_v1(c.program, &registered, fed, &mut |p, values| {
        let commitments = values.iter().map(|o| o.iter().map(crate::merkle3::tensor_commitment_v3).collect()).collect::<Vec<_>>();
        roots.push(crate::seg::position_root_of_v1(p, &commitments));
        if c.encoder.is_none() && decode_mismatch.is_none() && p + 1 >= c.prompt_len {
            let index = (p + 1 - c.prompt_len) as usize;
            let logits = values.last().and_then(|o| o.get(c.program.logits as usize));
            if logits.and_then(|t| c.decode.select(t)) != c.generated.get(index).copied() {
                decode_mismatch = Some(p);
            }
        }
        Ok(())
    });
    if let Err(e) = result {
        return inconsistent(registered.error.into_inner().unwrap_or_else(|| format!("registered-model replay could not run: {e}")));
    }
    Ok(OwnReplayV1 { roots, decode_mismatch })
}

/// The small replay result reused between adaptive DA requests. It contains no node values or producer state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnReplayV1 {
    pub roots: Vec<Digest>,
    pub decode_mismatch: Option<u32>,
}

/// Replay and check material that is already public. For adaptive on-chain DA, call `prepare_reexecution_v1` once and reuse its
/// roots with `check_claim_by_reexecution_v1` after each response/default; re-running the model for every probe is unnecessary.
pub fn reexecute_claim_v1(
    c: &SegClaimContextV1<'_>,
    producer: &dyn SegMaterialV1,
    artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
    tokens: &[u32],
) -> ReexecutionCheckV1 {
    match prepare_reexecution_v1(c, artifact, tokens) {
        Ok(own) => check_claim_by_reexecution_v1(c, &own.roots, own.decode_mismatch, producer, artifact, tokens),
        Err(why) => ReexecutionCheckV1 { finding: SegFindingV1::Inconsistent(why), divergent: None, probes: 0 },
    }
}

/// What a re-execution check found, and what it had to read to find it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReexecutionCheckV1 {
    pub finding: SegFindingV1,
    /// The first position whose committed root is not the verifier's (`None`: every segment root agrees, or the descent stopped on
    /// material it could not read).
    pub divergent: Option<u32>,
    /// Position paths read to descend the divergent segment (at most `⌈log2 S⌉`).
    pub probes: u32,
}

/// **The first selecting position whose delivered id is not the decode rule's choice on the verifier's own logits** (`own_logits(p)`:
/// the logits of the verifier's re-execution at `p`). `None` when every delivered id is the rule's.
pub fn first_decode_mismatch_v1(c: &SegClaimContextV1<'_>, own_logits: &dyn Fn(u32) -> Option<Tensor>) -> Option<u32> {
    let first = c.prompt_len.checked_sub(1)?;
    for p in first..c.positions {
        let r = (p + 1 - c.prompt_len) as usize;
        let Some(&delivered) = c.generated.get(r) else { break };
        let chosen = own_logits(p).and_then(|l| c.decode.select(&l));
        if chosen != Some(delivered) {
            return Some(p);
        }
    }
    None
}

/// The producer's tree nodes `(level, index) -> hash` an authenticated path shows: the path's own nodes and their siblings.
fn record_path(
    levels: &[Vec<Digest>],
    known: &mut BTreeMap<(usize, usize), Digest>,
    mut index: usize,
    leaf: Digest,
    siblings: &[Digest],
) {
    let mut cur = leaf;
    let mut it = siblings.iter();
    for (k, level) in levels.iter().enumerate() {
        known.insert((k, index), cur);
        if level.len() <= 1 {
            break;
        }
        let sib = index ^ 1;
        if sib < level.len() {
            let Some(s) = it.next() else { return };
            known.insert((k, sib), *s);
            cur = if index & 1 == 0 { seg_node(&cur, s) } else { seg_node(s, &cur) };
        }
        index >>= 1;
    }
}

/// **Descend segment `index`, whose on-chain root is not the verifier's, to its first divergent position.** `Ok((q, probes))`, or
/// `Err((positions to demand, probes))` when a probe's path is missing or does not authenticate.
fn first_divergent_in_segment(
    c: &SegClaimContextV1<'_>,
    index: u32,
    own_position_roots: &[Digest],
    producer: &dyn SegMaterialV1,
) -> Result<(u32, u32), (Vec<u32>, u32)> {
    let (first, end) = segment_bounds_v1(c.positions, index).ok_or((Vec::new(), 0))?;
    // The verifier's own tree, level by level (an unpaired last node is carried up, as the commitment does).
    let mut levels: Vec<Vec<Digest>> = vec![(first..end).map(|q| segment_leaf_v1(q, &own_position_roots[q as usize])).collect()];
    while levels.last().expect("a level").len() > 1 {
        let next =
            levels.last().expect("a level").chunks(2).map(|ch| if ch.len() == 2 { seg_node(&ch[0], &ch[1]) } else { ch[0] }).collect();
        levels.push(next);
    }
    let mut known: BTreeMap<(usize, usize), Digest> = BTreeMap::new();
    let mut probes = 0u32;
    // The root differs (the on-chain segment root is not the verifier's). Walk down, always into the leftmost child that differs.
    let (mut lvl, mut idx) = (levels.len() - 1, 0usize);
    while lvl > 0 {
        let below = &levels[lvl - 1];
        let (l, r) = (2 * idx, 2 * idx + 1);
        if r >= below.len() {
            // An unpaired node is its only child.
            lvl -= 1;
            idx = l;
            continue;
        }
        if !known.contains_key(&(lvl - 1, l)) {
            // Probe the subtree's leftmost position: its path shows both children of every node on the way down to it.
            let p = first + (idx << lvl) as u32;
            let Some((root, siblings)) = producer.position_path(p) else { return Err((vec![p], probes)) };
            probes += 1;
            if !position_in_segment(p, &root, &siblings, c.segment_roots, c.positions) {
                return Err((vec![p], probes));
            }
            record_path(&levels, &mut known, (p - first) as usize, segment_leaf_v1(p, &root), &siblings);
        }
        let left = known.get(&(lvl - 1, l)).copied().ok_or((Vec::new(), probes))?;
        idx = if left != below[l] { l } else { r };
        lvl -= 1;
    }
    Ok((first + idx as u32, probes))
}

/// **The re-execution check of a segmented claim** (module doc).
///
/// * `own_position_roots`: the position roots of the verifier's own re-execution of the claim's fed ids (prompt, then every delivered
///   id but the last), one per position of the claim.
/// * `decode_mismatch`: [`first_decode_mismatch_v1`] on the verifier's own logits.
/// * `producer`: where the producer's material is read (its stream, or what demands made it serve). Only the divergent segment's
///   position paths and positions `q − 1`, `q` are ever read.
pub fn check_claim_by_reexecution_v1(
    c: &SegClaimContextV1<'_>,
    own_position_roots: &[Digest],
    decode_mismatch: Option<u32>,
    producer: &dyn SegMaterialV1,
    artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,
    tokens: &[u32],
) -> ReexecutionCheckV1 {
    let at = |finding, divergent, probes| ReexecutionCheckV1 { finding, divergent, probes };
    if own_position_roots.len() != c.positions as usize || segment_count_v1(c.positions) as usize != c.segment_roots.len() {
        return at(SegFindingV1::Inconsistent("the re-execution does not have the claim's positions".into()), None, 0);
    }
    let own = segment_roots_of_position_roots_v1(own_position_roots);
    let Some(index) = (0..own.len()).find(|i| own[*i] != c.segment_roots[*i]) else {
        // Every committed value is the verifier's own: only the decode relation is left.
        return match decode_mismatch {
            None => at(SegFindingV1::Clean, None, 0),
            Some(p) => match check_positions_v1(c, producer, artifact, tokens, &[p]) {
                SegFindingV1::Clean => {
                    at(SegFindingV1::Inconsistent(format!("the delivered id at {p} is not the rule's yet checks clean")), None, 0)
                }
                f => at(f, None, 0),
            },
        };
    };
    match first_divergent_in_segment(c, index as u32, own_position_roots, producer) {
        Err((missing, probes)) if missing.is_empty() => {
            at(SegFindingV1::Inconsistent("the descent lost its own path".into()), None, probes)
        }
        Err((missing, probes)) => at(SegFindingV1::Demand(missing), None, probes),
        Ok((q, probes)) => {
            let finding = match check_positions_v1(c, producer, artifact, tokens, &[q]) {
                SegFindingV1::Clean => {
                    SegFindingV1::Inconsistent(format!("position {q} departs from the re-execution yet checks clean"))
                }
                f => f,
            };
            at(finding, Some(q), probes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seg::SegmentedCommitmentsV1;
    use std::cell::Cell;

    /// A producer's tree with some positions' roots replaced; probes counted.
    struct Paths {
        c: SegmentedCommitmentsV1,
        probes: Cell<u32>,
    }

    impl SegMaterialV1 for Paths {
        fn position(&self, _: u32) -> Option<Vec<Vec<Tensor>>> {
            None
        }
        fn position_siblings(&self, p: u32) -> Option<Vec<Digest>> {
            (p < self.c.positions()).then(|| self.c.position_path(p).1)
        }
        fn position_path(&self, p: u32) -> Option<(Digest, Vec<Digest>)> {
            self.probes.set(self.probes.get() + 1);
            (p < self.c.positions()).then(|| self.c.position_path(p))
        }
    }

    fn roots(positions: u32, salt: u8, lies: &[u32]) -> SegmentedCommitmentsV1 {
        SegmentedCommitmentsV1::new(
            (0..positions)
                .map(|p| {
                    let tag = if lies.contains(&p) { salt } else { 0 };
                    vec![vec![crate::hash::id(b"v", &[p as u8, (p >> 8) as u8, (p >> 16) as u8, tag])]]
                })
                .collect(),
        )
    }

    /// The descent finds the FIRST divergent position of the first divergent segment, with at most ⌈log2 S⌉ probes, wherever the lies
    /// are (one, several, a whole tail, the unpaired last leaf of a short segment).
    #[test]
    fn the_descent_finds_the_first_divergent_position_in_at_most_log2_s_probes() {
        let program = misaka_palw_tir_sketch::fixture::wide128_v1(1).program;
        let params = crate::trace::ParamCommitmentsV1 { by_instance: Default::default() };
        for (positions, lies) in [
            (1u32, vec![0u32]),
            (5, vec![4]),
            (1024, vec![0]),
            (1024, vec![1023]),
            (1025, vec![1024]),
            (3000, vec![1500, 1700, 2999]),
            (3000, vec![2047, 2048]),
            (2500, (2100..2500).collect()),
            (4096, vec![3072 + 511, 3072 + 512]),
        ] {
            let own = roots(positions, 0, &[]);
            let producer = Paths { c: roots(positions, 9, &lies), probes: Cell::new(0) };
            let segment_roots = producer.c.segment_roots();
            let c = SegClaimContextV1 {
                program: &program,
                params: &params,
                segment_roots: &segment_roots,
                positions,
                prompt_len: positions,
                prompt_root: [0; 64],
                inline_prompt: None,
                generated: &[],
                decode: crate::job::DecodeRuleV1::Greedy,
                encoder: None,
            };
            let first = *lies.iter().min().unwrap();
            let index = (0..segment_roots.len()).find(|i| segment_roots[*i] != own.segment_roots()[*i]).unwrap() as u32;
            let (q, probes) = first_divergent_in_segment(&c, index, &own.position_roots, &producer).unwrap();
            assert_eq!(q, first, "{positions} {lies:?}");
            assert!(probes <= 10 && probes == producer.probes.get(), "{positions} {lies:?}: {probes} probes");
            // A producer that withholds a path is demanded exactly the probed position.
            struct Mute;
            impl SegMaterialV1 for Mute {
                fn position(&self, _: u32) -> Option<Vec<Vec<Tensor>>> {
                    None
                }
                fn position_siblings(&self, _: u32) -> Option<Vec<Digest>> {
                    None
                }
            }
            let (f, e) = segment_bounds_v1(positions, index).unwrap();
            if e - f > 1 {
                let (missing, _) = first_divergent_in_segment(&c, index, &own.position_roots, &Mute).unwrap_err();
                assert_eq!(missing, vec![f], "the first probe is the segment's first position");
            }
        }
    }
}
