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
//! The result, [`TirFamilyCertificateV1`], states what was drilled and what the court said.
//! [`tir_family_evidence_v1`] also records it in the form the chain grades
//! (`palw_tir_certify_v1::PalwTirE2eDrillEvidenceV1`, graded by `certify_tir_e2e_family_v1` — what a
//! `FamilyCertified` carries as `TirAttempt`): per drilled leaf, the honest run's refutation (which
//! the court acquits) and the lying run's (which it convicts), their programs stripped (the evidence
//! carries the class once), the two runs' prefix states around the leaf, and how many malformed
//! materials the seat's verb refused.

use std::collections::{BTreeMap, BTreeSet};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwMaterialVerdictV1};
use kaspa_consensus_core::palw_step::kernel_semantics_id_v1;
use kaspa_consensus_core::palw_step_leg::PalwStepFaultV1;
use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;
use kaspa_consensus_core::palw_tir_certify_v1::{PalwTirE2eDrillEvidenceV1, PalwTirE2eFaultVectorV1, palw_tir_strip_program_v1};
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

/// **The drill's covering set** — the first leaf of every `(unit, call class)` among a job's first
/// `count` step leaves: every committed node's tiles, every `Fixed` state's checkpoint and every
/// history's tile, in prefill and in decode. What a drill plants its lies at, and what a live court
/// battery (D-F2) tampers at, one commit-point kind at a time.
pub fn tir_drill_covering_leaves_v1(
    space: &kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1,
    job: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
    count: u64,
) -> Result<BTreeMap<(TirDrillUnitKindV1, TirDrillCallV1), u64>, String> {
    let mut candidates: BTreeMap<(TirDrillUnitKindV1, TirDrillCallV1), u64> = BTreeMap::new();
    for i in 0..count {
        let leaf = space.leaf_at(job, i).ok_or_else(|| format!("leaf {i} is not in the step space"))?;
        let kind = match leaf.kind {
            PalwTirLeafKindV1::Commit { block, node, .. } => TirDrillUnitKindV1::Commit { block, node },
            PalwTirLeafKindV1::State { state, .. } => TirDrillUnitKindV1::Checkpoint { state },
            PalwTirLeafKindV1::HistTile { state, .. } => TirDrillUnitKindV1::HistTile { state },
        };
        let call = if leaf.position < job.declared_prefill_tokens { TirDrillCallV1::Prefill } else { TirDrillCallV1::Decode };
        candidates.entry((kind, call)).or_insert(i);
    }
    Ok(candidates)
}

/// **One terminal close this node's builders produce, as it rides** ([`tir_terminal_close_sizes_v1`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirCloseSizeV1 {
    /// The step leaf the close is at: the first leaf of its commit-point kind and call.
    pub leaf: u64,
    pub kind: TirDrillUnitKindV1,
    pub call: TirDrillCallV1,
    /// `cone` (`TirCone`), `logits` (`TirLogits`), `decode token` (`TirDecodeToken[Tiled]`), or
    /// `dissected`: a commit point whose cone reduces over the history (F7), where the court's
    /// terminal move is the dissection's and no whole close is built.
    pub door: &'static str,
    /// The close's `borsh` bytes with its program stripped — what the carrier carries; 0 for a
    /// dissected point.
    pub carried_bytes: u64,
}

/// **Every terminal close this node's IR builders produce for `material`'s job, measured as it
/// rides** — the stable entry point a differential test holds admission v10's carried-close figure
/// (PALW-TIR-38) against: at the first leaf of every commit-point kind and call (the drill's covering
/// set, [`tir_drill_covering_leaves_v1`]) the cone close, and at the logits node's leaves the
/// logits-consistency close and the decode-token door too; each exactly as the node files it (its
/// program stripped), its `borsh` length. A dissected point (a cone reducing over the history,
/// `palw_tir_dissected_commit_points_v1`) is listed with no size: its terminal move is F7's
/// dissection, never a whole close.
pub fn tir_terminal_close_sizes_v1(
    backend: &TirBackendV1,
    material: &[u8],
    rules: &PalwTirCourtRulesV1,
) -> Result<Vec<TirCloseSizeV1>, String> {
    let capture = backend.decode_capture(material)?;
    let ctx = capture.binding.job_context.clone();
    let space = backend.space();
    let program = &space.program;
    let dissected = kaspa_consensus_core::palw_tir_dissect_v1::palw_tir_dissected_commit_points_v1(program);
    let post = (space.occurrences().len() - 1) as u32;
    let carried = |mut proof: kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2| -> Result<u64, String> {
        proof.tir_strip_program_v1();
        borsh::to_vec(&proof).map(|b| b.len() as u64).map_err(|e| e.to_string())
    };
    let mut out = Vec::new();
    for ((kind, call), leaf) in tir_drill_covering_leaves_v1(space, &ctx, capture.binding.step_leaf_count)? {
        if matches!(kind, TirDrillUnitKindV1::Commit { block, node } if dissected.contains(&(block, node))) {
            out.push(TirCloseSizeV1 { leaf, kind, call, door: "dissected", carried_bytes: 0 });
            continue;
        }
        out.push(TirCloseSizeV1 {
            leaf,
            kind,
            call,
            door: "cone",
            carried_bytes: carried(backend.cone_close(material, leaf, rules)?)?,
        });
        let at = space.leaf_at(&ctx, leaf).ok_or_else(|| format!("leaf {leaf} is not a leaf of the job"))?;
        let logits =
            matches!(at.kind, PalwTirLeafKindV1::Commit { occurrence, node, .. } if occurrence == post && node == program.logits);
        if logits {
            out.push(TirCloseSizeV1 {
                leaf,
                kind,
                call,
                door: "logits",
                carried_bytes: carried(backend.logits_close(material, leaf)?)?,
            });
            if let Some(row) = (at.position + 1).checked_sub(ctx.declared_prefill_tokens) {
                let beat = capture.generated.get(row as usize).map(|t| (t + 1) % program.token_bound.max(1)).unwrap_or(0);
                out.push(TirCloseSizeV1 {
                    leaf,
                    kind,
                    call,
                    door: "decode token",
                    carried_bytes: carried(backend.decode_token_close(material, row, beat)?)?,
                });
            }
        }
    }
    Ok(out)
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
    tir_family_evidence_v1(backend, anchor, rules, Hash64::default()).map(|(certificate, _)| certificate)
}

/// **The family id an IR drill names by default**: the keyed digest of the kernel ids of every
/// primitive the program reaches (`palw_tir_reachable_prims_v1`, the set the chain's grader
/// certifies the family for) — so two drills of one primitive set name one family.
pub fn tir_family_id_v1(kernel_ids: &BTreeSet<Hash64>) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/tir/family-id/v1").to_state();
    s.update(&(kernel_ids.len() as u64).to_le_bytes());
    for id in kernel_ids {
        s.update(id.as_byte_slice());
    }
    Hash64::from_bytes(s.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// Material a seat may be handed that is not a capture of this class's job: truncations and an
/// extension of the class's own honest capture (a decoder's most likely wrong turn: a value that
/// parses and does not cohere), and bytes that are not the format at all.
fn tir_malformed_variants_v1(material: &[u8]) -> Vec<Vec<u8>> {
    let n = material.len();
    let mut cuts: Vec<usize> = vec![0, 1, 7, 8, 9, 16, n / 4, n / 2, n.saturating_sub(64), n.saturating_sub(1)];
    cuts.sort_unstable();
    cuts.dedup();
    let mut out: Vec<Vec<u8>> = cuts.into_iter().filter(|c| *c < n).map(|c| material[..c].to_vec()).collect();
    let mut extended = material.to_vec();
    extended.extend_from_slice(&[0u8; 16]);
    out.push(extended);
    out.push(b"not a capture of any class".to_vec());
    let mut head = material[..material.len().min(8)].to_vec();
    head.extend_from_slice(&[0xFF; 64]);
    out.push(head);
    out
}

/// **Drill one IR class, and record it as the chain's evidence** under `family_id` (the default,
/// [`tir_family_id_v1`] of the drilled kernels, when `Hash64::default()`). The certificate is
/// [`tir_family_drill_v1`]'s; the evidence is what `FamilyCertified { TirAttempt }` carries.
pub fn tir_family_evidence_v1(
    backend: &TirBackendV1,
    anchor: Hash64,
    rules: &PalwTirCourtRulesV1,
    family_id: Hash64,
) -> Result<(TirFamilyCertificateV1, PalwTirE2eDrillEvidenceV1), String> {
    let (job, prompt) = backend.job_for_anchor(anchor)?;
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let honest_run = backend.execute(&job, &prompt)?;
    let honest = honest_run.material.clone();
    let honest_capture = TirCaptureV1::decode(&honest)?;
    if !honest_capture.is_dense() {
        return Err("the drill needs a dense capture: the job is past the dense-capture cap".into());
    }
    let space = backend.space();
    let program = &space.program;
    let count = honest_capture.binding.step_leaf_count;

    // The covering set: the first leaf of every (unit, call class).
    let candidates = tir_drill_covering_leaves_v1(space, &job, count)?;

    let mut units = Vec::with_capacity(candidates.len());
    let mut vectors = Vec::with_capacity(candidates.len());
    let prefix =
        |material: &[u8], at: u64| backend.bisect_prefix_state(material, at).ok_or_else(|| format!("no prefix state at {at}"));
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
        let (mut honest_refutation, mut guilty_refutation) = (defence, challenger);
        palw_tir_strip_program_v1(&mut honest_refutation);
        palw_tir_strip_program_v1(&mut guilty_refutation);
        vectors.push(PalwTirE2eFaultVectorV1 {
            leaf_index: leaf,
            honest: honest_refutation,
            guilty: guilty_refutation,
            honest_prefix: (prefix(&honest, leaf)?, prefix(&honest, leaf + 1)?),
            guilty_prefix: (prefix(&faulty, leaf)?, prefix(&faulty, leaf + 1)?),
        });
        units.push(TirDrillUnitV1 { kind, call, leaf, conviction, acquitted });
    }
    // **The seat's verb, pointed at a stranger's bytes** (the legacy drill's rule): it answers
    // every malformed material without crashing, and never `Matches`.
    let claim = PalwClaimRootsV1 {
        execution_root: honest_run.execution_root,
        trace_root: honest_run.trace_root,
        anchor,
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let mut malformed_inputs_refused = 0u32;
    for bytes in tir_malformed_variants_v1(&honest) {
        if backend.verify_material(&bytes, claim) == PalwMaterialVerdictV1::Matches {
            return Err(format!("a {}-byte malformed material verified as Matches", bytes.len()));
        }
        malformed_inputs_refused += 1;
    }
    let mut scheduled: BTreeSet<usize> = [program.schedule.pre as usize, program.schedule.post as usize].into_iter().collect();
    scheduled.extend(program.schedule.layers.iter().map(|b| *b as usize));
    let reachable_prims: BTreeSet<&'static str> =
        scheduled.into_iter().flat_map(|b| program.blocks[b].nodes.iter().map(|n| n.prim.name())).collect();
    let covered_kernels = covered_prims.iter().map(|n| tir_prim_kernel_id_v1(n)).collect();
    let family_id = if family_id == Hash64::default() {
        tir_family_id_v1(&kaspa_consensus_core::palw_tir_admission_v1::palw_tir_reachable_prims_v1(program))
    } else {
        family_id
    };
    let evidence = PalwTirE2eDrillEvidenceV1 {
        family_id,
        class: backend.class().clone(),
        artifact_root: backend.artifact_root(),
        vectors,
        malformed_inputs_refused,
    };
    let certificate = TirFamilyCertificateV1 {
        class_id: backend.class_id(),
        artifact_root: backend.artifact_root(),
        units,
        covered_prims,
        covered_kernels,
        reachable_prims,
    };
    Ok((certificate, evidence))
}
