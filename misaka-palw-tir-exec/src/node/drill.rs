//! **The IR family drill** (RFC-0002 Phase F, D-F2; the shape of ADR-0069 Decision 3's
//! certification drill, `misaka_palw_base0::e2e_drill`, for an IR class).
//!
//! One harness over [`TirBackendV1`], driving only the verbs a node has — execute, execute with a
//! planted fault, the bisection's prefix states, the evidence builders — and asking the SHIPPED
//! court (`palw_tir_court_v1::check_tir_cone_refutation_v1`) which way each close reads:
//!
//! * **the covering set** — one leaf per `(unit, call class)` the job reaches, where a unit is a
//!   committed node of a block (carry-outs, `TopK` outputs, `HistAppend` rows, the logits …), a
//!   `Fixed` state's checkpoint, or a history's tile, and the call class is prefill or decode;
//! * **the executor lies** at each: its capture commits one lane off; the bisection's prefix states
//!   narrow an honest challenger to exactly that leaf; the refutation the challenger builds from its
//!   OWN re-execution and the accused's disputed leaf is byte-identical to the one the accused's
//!   capture opens; and the court convicts (a computation mismatch, or PALW-TIR-33);
//! * **the challenger lies** at each: the honest executor's refutation, from its own capture, is
//!   acquitted;
//! * **coverage** in kernels: every primitive the drilled cones evaluate, as
//!   `kernel_semantics_id_v1("palw-tir/v1/prim=<Name>")` (design §2.7's namespace), against every
//!   primitive the program uses.
//!
//! The result, [`TirFamilyCertificateV1`], states what was drilled and what the court said; the
//! chain-side certifier that would score it (the IR twin of `certify_e2e_family_v1`) is F6's.

use std::collections::{BTreeMap, BTreeSet};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_step::kernel_semantics_id_v1;
use kaspa_consensus_core::palw_step_leg::PalwStepFaultV1;
use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;
use kaspa_consensus_core::palw_tir_court_v1::{PalwTirCourtRulesV1, check_tir_cone_refutation_v1};
use kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1;
use misaka_palw_tir::admit::cone_nodes;
use misaka_palw_tir::demand::state_writer_v1;

use super::backend::{TirBackendV1, TirCaptureV1};

/// Which call of the job a drilled leaf belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TirDrillCallV1 {
    Prefill,
    Decode,
}

/// What a drilled leaf commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TirDrillUnitKindV1 {
    /// A tile of committed node `node` of block `block`.
    Commit { block: u8, node: u16 },
    /// A `Fixed` state's checkpoint tile.
    Checkpoint { state: u16 },
    /// A history's tile.
    HistTile { state: u16 },
}

/// One drilled leaf and what the court said, both ways.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirDrillUnitV1 {
    pub kind: TirDrillUnitKindV1,
    pub call: TirDrillCallV1,
    pub leaf: u64,
    /// The executor's planted lie, as the court convicted it.
    pub conviction: PalwStepFaultV1,
    /// The honest executor against a lying challenger: acquitted.
    pub acquitted: bool,
}

/// **What the drill establishes for one IR class.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirFamilyCertificateV1 {
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub units: Vec<TirDrillUnitV1>,
    /// The primitives the drilled cones evaluate (names, and their kernel ids).
    pub covered_prims: BTreeSet<&'static str>,
    pub covered_kernels: BTreeSet<Hash64>,
    /// Every primitive the program's scheduled blocks use.
    pub reachable_prims: BTreeSet<&'static str>,
}

impl TirFamilyCertificateV1 {
    /// Every unit convicted one way and acquitted the other, and every primitive the program uses
    /// evaluated by some drilled cone.
    pub fn holds(&self) -> bool {
        !self.units.is_empty() && self.units.iter().all(|u| u.acquitted) && self.reachable_prims.is_subset(&self.covered_prims)
    }
}

/// The kernel id of an IR primitive (design §2.7).
pub fn tir_prim_kernel_id_v1(name: &str) -> Hash64 {
    kernel_semantics_id_v1(&format!("palw-tir/v1/prim={name}"))
}

/// **The first leaf two captures of one job differ at**, found the way the court's ladder finds it:
/// by the parties' prefix states alone.
pub fn tir_ladder_divergence_v1(backend: &TirBackendV1, a: &[u8], b: &[u8], leaf_count: u64) -> Option<u64> {
    let state = |m: &[u8], i: u64| backend.bisect_prefix_state(m, i);
    if state(a, leaf_count)? == state(b, leaf_count)? {
        return None;
    }
    let (mut lo, mut hi) = (0u64, leaf_count);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if state(a, mid)? == state(b, mid)? {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(lo)
}

/// **Drill one IR class** at the job `anchor` implies. `Err` names the first thing the drill could
/// not do or the first verdict that was not the one an honest court owes.
pub fn tir_family_drill_v1(
    backend: &TirBackendV1,
    anchor: Hash64,
    rules: &PalwTirCourtRulesV1,
) -> Result<TirFamilyCertificateV1, String> {
    let (job, prompt) = backend.job_for_anchor(anchor)?;
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let honest = backend.execute(&job, &prompt)?.material;
    let honest_capture = TirCaptureV1::decode(&honest)?;
    if !honest_capture.is_dense() {
        return Err("the drill needs a dense capture: the job is past the dense-capture cap".into());
    }
    let space = backend.space();
    let program = &space.program;
    let count = honest_capture.binding.step_leaf_count;

    // The covering set: the first leaf of every (unit, call class).
    let mut candidates: BTreeMap<(TirDrillUnitKindV1, TirDrillCallV1), u64> = BTreeMap::new();
    for i in 0..count {
        let leaf = space.leaf_at(&job, i).ok_or_else(|| format!("leaf {i} is not in the step space"))?;
        let kind = match leaf.kind {
            PalwTirLeafKindV1::Commit { block, node, .. } => TirDrillUnitKindV1::Commit { block, node },
            PalwTirLeafKindV1::State { state, .. } => TirDrillUnitKindV1::Checkpoint { state },
            PalwTirLeafKindV1::HistTile { state, .. } => TirDrillUnitKindV1::HistTile { state },
        };
        let call = if leaf.position < job.declared_prefill_tokens { TirDrillCallV1::Prefill } else { TirDrillCallV1::Decode };
        candidates.entry((kind, call)).or_insert(i);
    }

    let mut units = Vec::with_capacity(candidates.len());
    let mut covered_prims = BTreeSet::new();
    for ((kind, call), leaf) in candidates {
        // The executor lies at `leaf`.
        let faulty = backend.execute_with_injected_fault(&job, &prompt, leaf)?.material;
        let faulty_capture = TirCaptureV1::decode(&faulty)?;
        let found = tir_ladder_divergence_v1(backend, &honest, &faulty, count);
        if found != Some(leaf) {
            return Err(format!("{kind:?} ({call:?}): the ladder narrowed to {found:?}, the lie is at {leaf}"));
        }
        let accused_own = backend.cone_refutation(&faulty, leaf, rules)?;
        let (opening, preimage) = backend.step_opening(&faulty, leaf)?;
        let challenger = backend.challenger_refutation(
            &faulty_capture.binding,
            &ids,
            &opening,
            preimage,
            &faulty_capture.logits_rows,
            &faulty_capture.generated,
            rules,
        )?;
        if challenger != accused_own {
            return Err(format!("{kind:?} ({call:?}) at {leaf}: the challenger's refutation is not the accused's canonical one"));
        }
        let conviction = match check_tir_cone_refutation_v1(&challenger, rules) {
            Ok(v) => v.fault,
            Err(e) => return Err(format!("{kind:?} ({call:?}) at {leaf}: the planted lie was not convicted ({e})")),
        };
        if !matches!(conviction, PalwStepFaultV1::ComputationMismatch { .. } | PalwStepFaultV1::TirValueOutsideProvenInterval { .. }) {
            return Err(format!("{kind:?} ({call:?}) at {leaf}: convicted of {conviction:?}, not of the lie"));
        }
        // The challenger lies at `leaf`: the honest executor's close.
        let defence = backend.cone_refutation(&honest, leaf, rules)?;
        let acquitted = check_tir_cone_refutation_v1(&defence, rules) == Err(PalwStepRefuteError::NoFaultFound);
        if !acquitted {
            return Err(format!("{kind:?} ({call:?}) at {leaf}: the honest executor was not acquitted"));
        }
        // What the drilled cone evaluates.
        let cone = match kind {
            TirDrillUnitKindV1::Commit { block, node } => Some((block as usize, node as usize)),
            TirDrillUnitKindV1::Checkpoint { state } => {
                let l = space.leaf_at(&job, leaf).expect("enumerated above");
                let layer = match l.kind {
                    PalwTirLeafKindV1::State { layer, .. } => layer,
                    _ => None,
                };
                state_writer_v1(program, state, layer)
                    .and_then(|(occ, node)| space.occurrences().get(occ as usize).map(|(b, _)| (*b as usize, node as usize)))
            }
            TirDrillUnitKindV1::HistTile { .. } => None,
        };
        if let Some((block, root)) = cone {
            for n in cone_nodes(program, block, root) {
                covered_prims.insert(program.blocks[block].nodes[n as usize].prim.name());
            }
        }
        units.push(TirDrillUnitV1 { kind, call, leaf, conviction, acquitted });
    }
    let mut scheduled: BTreeSet<usize> = [program.schedule.pre as usize, program.schedule.post as usize].into_iter().collect();
    scheduled.extend(program.schedule.layers.iter().map(|b| *b as usize));
    let reachable_prims: BTreeSet<&'static str> =
        scheduled.into_iter().flat_map(|b| program.blocks[b].nodes.iter().map(|n| n.prim.name())).collect();
    let covered_kernels = covered_prims.iter().map(|n| tir_prim_kernel_id_v1(n)).collect();
    Ok(TirFamilyCertificateV1 {
        class_id: backend.class_id(),
        artifact_root: backend.artifact_root(),
        units,
        covered_prims,
        covered_kernels,
        reachable_prims,
    })
}
