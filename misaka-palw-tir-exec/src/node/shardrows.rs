//! **A shard-only holder's fetch (RFC-0006 §4.2, D-S6): only the shard's inventory rows, each proven against the registered root.**
//!
//! An outsider that holds no copy of a class cannot judge a cell without the weights its cell reads. It needs exactly one shard's
//! parameter rows — [`kaspa_consensus_core::palw_tir_shard_v1::palw_tir_shard_inventory_ranges_v1`] — and it takes them from any
//! holder through a [`TirRowFetcherV1`], believing nothing it does not check: every row arrives as an artifact opening and must
//! (a) be the leaf asked for and (b) hash up its Merkle path to the `artifact_root` the chain registered. The verified rows
//! assemble into the tensors of the shard's own instances ([`fetch_shard_params_v1`]) and into nothing else — the executor's
//! `new_cell` refuses a cell whose occurrences read an instance that was not fetched.

use std::collections::BTreeMap;
use std::path::PathBuf;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::{PalwArtifactOpeningV1, verify_artifact_opening_v1};
use kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1;
use kaspa_consensus_core::palw_tir_shard_v1 as shard_rules;
use kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1;
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{MapParams, Tensor};

use super::artifact::TirArtifactV1;
use super::cell::tir_shard_geometry_over_v1;
use super::inventory::TirParamOpenerV1;
use crate::params::TirParams;
use crate::plan::TirPlan;

/// Where a shard-only seat gets rows: a holder's answer to "open these inventory leaves". Nothing it returns is believed.
pub trait TirRowFetcherV1 {
    /// An opening for each leaf of `leaves`, in order.
    fn open_rows(&self, leaves: &[u32]) -> Result<Vec<PalwArtifactOpeningV1>, String>;
}

/// **A mirror on this host**: a class container read through the holder's own opening door, one leaf at a time. The seat keeps
/// the openings it asked for, not the container (the holder's side is dropped after the answer).
pub struct TirFileMirrorV1(pub PathBuf);

impl TirRowFetcherV1 for TirFileMirrorV1 {
    fn open_rows(&self, leaves: &[u32]) -> Result<Vec<PalwArtifactOpeningV1>, String> {
        let holder = TirArtifactV1::open(&self.0)?;
        leaves.iter().map(|i| holder.param_opening(*i).ok_or_else(|| format!("the mirror cannot open leaf {i}"))).collect()
    }
}

/// What a shard-only seat holds after its fetch.
pub struct TirShardHoldingV1 {
    pub space: PalwTirStepSpaceV1,
    pub plan: TirPlan,
    pub params: TirParams<'static>,
    pub class_id: Hash64,
    pub shard: u16,
    pub s_l: u16,
    /// Leaves fetched and the operand bytes they carried — the fraction of the class this seat held.
    pub leaves: u64,
    pub bytes: u64,
    /// The openings the rows arrived as, kept: a shard-only seat that finds a lie builds the court's close from them (the cone's
    /// multiproof is assembled from these very paths), so it needs no copy of the class to accuse.
    pub openings: BTreeMap<u32, PalwArtifactOpeningV1>,
}

/// **Fetch and verify one shard's rows.** `class` is the class as a capture's binding carries it (its program and layout), checked
/// against `class_id` and `artifact_root` by the caller; each leaf of the shard's inventory ranges is fetched through `fetcher`,
/// proven against `artifact_root`, and the shard's instances assembled.
pub fn fetch_shard_params_v1(
    class: &PalwTirClassV1,
    class_id: Hash64,
    artifact_root: Hash64,
    s_l: u16,
    shard: u16,
    fetcher: &dyn TirRowFetcherV1,
) -> Result<TirShardHoldingV1, String> {
    if class.class_id(&artifact_root) != class_id {
        return Err("the class (program and layout) is not the one the chain registered under this root".into());
    }
    let space = PalwTirStepSpaceV1::new(class).map_err(|e| e.to_string())?;
    let program: TirProgramV1 = space.program.clone();
    let (parts, _) = tir_shard_geometry_over_v1(&space, s_l)?;
    let layers = parts.get(usize::from(shard)).cloned().ok_or_else(|| format!("shard {shard} of {s_l}"))?;
    let ranges = shard_rules::palw_tir_shard_inventory_ranges_v1(&program, layers, shard == 0, shard + 1 == s_l);
    let wanted: Vec<u32> = ranges.iter().flat_map(|r| r.clone()).collect();
    let mut pieces: BTreeMap<(u16, Option<u16>), Vec<(u32, Vec<u8>)>> = BTreeMap::new();
    let (mut leaves, mut bytes) = (0u64, 0u64);
    let mut kept: BTreeMap<u32, PalwArtifactOpeningV1> = BTreeMap::new();
    for chunk in wanted.chunks(256) {
        let openings = fetcher.open_rows(chunk)?;
        if openings.len() != chunk.len() {
            return Err("the holder answered a different number of rows than asked".into());
        }
        for (want, opening) in chunk.iter().zip(openings) {
            if opening.leaf_index != *want {
                return Err(format!("the holder opened leaf {} for leaf {want}", opening.leaf_index));
            }
            verify_artifact_opening_v1(&opening, artifact_root).map_err(|e| format!("leaf {want} does not open the registered root: {e}"))?;
            let Some(j) = program.params.iter().position(|d| d.name == opening.operand.tensor_name) else {
                return Err(format!("leaf {want} names a tensor the program does not declare"));
            };
            leaves += 1;
            bytes += opening.operand.bytes.len() as u64;
            pieces.entry((j as u16, opening.operand.layer)).or_default().push((opening.operand.row_start, opening.operand.bytes.clone()));
            kept.insert(*want, opening);
        }
    }
    let mut map = MapParams::default();
    for ((j, layer), mut parts) in pieces {
        parts.sort_by_key(|p| p.0);
        let d = &program.params[j as usize];
        let mut tensor_bytes: Vec<u8> = Vec::new();
        for (start, b) in parts {
            if start as usize != tensor_bytes.len() {
                return Err(format!("param {} has a gap before byte {start}", d.name));
            }
            tensor_bytes.extend_from_slice(&b);
        }
        let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
        map.tensors.insert((j, layer), Tensor::from_le_bytes(d.dtype, &shape, &tensor_bytes).map_err(|e| format!("param {}: {e}", d.name))?);
    }
    let plan = TirPlan::compile(&program).map_err(|e| e.to_string())?;
    let params = TirParams::from_map_lenient(&plan, &map).map_err(|e| e.to_string())?;
    Ok(TirShardHoldingV1 { space, plan, params, class_id, shard, s_l, leaves, bytes, openings: kept })
}

impl TirParamOpenerV1 for TirShardHoldingV1 {
    fn param_opening(&self, leaf: u32) -> Option<PalwArtifactOpeningV1> {
        self.openings.get(&leaf).cloned()
    }

    /// The multiproof the held paths make: `None` for a leaf outside the shard (the seat never fetched it, and a cone that reads
    /// one is not this seat's to build).
    fn param_multiproof(&self, leaves: &[u32]) -> Option<kaspa_consensus_core::palw_artifact::PalwArtifactMultiproofV1> {
        let openings: Option<Vec<PalwArtifactOpeningV1>> = leaves.iter().map(|l| self.openings.get(l).cloned()).collect();
        kaspa_consensus_core::palw_artifact::palw_artifact_multiproof_from_openings_v1(&openings?)
    }
}

impl TirShardHoldingV1 {
    /// **The evidence store over a dense capture, params read from the held rows.** The same store a full holder builds
    /// (`TirEvidenceV1::dense`); only the param doors differ, and they answer for the shard's leaves alone.
    fn with_store<R>(
        &self,
        capture: &super::backend::TirCaptureV1,
        prompt_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
        cap: u64,
        f: impl FnOnce(&super::evidence::TirEvidenceV1<'_>) -> Result<R, String>,
    ) -> Result<R, String> {
        if !capture.is_dense() {
            return Err("a shard-only seat builds a close from a dense capture".into());
        }
        let runner = super::run::TirClassRunnerV1::new(&self.space, &self.plan, &self.params, self.class_id)?;
        let store = super::evidence::TirEvidenceV1::dense(
            &runner,
            &capture.binding,
            &capture.prompt,
            &capture.logits_rows,
            &capture.generated,
            &capture.leaves,
            self,
            prompt_form,
            cap,
        )?;
        f(&store)
    }

    /// **The cone close of step leaf `index`**, program stripped as a close rides — built from the capture and the held rows.
    pub fn cone_close(
        &self,
        capture: &super::backend::TirCaptureV1,
        index: u64,
        rules: &kaspa_consensus_core::palw_tir_court_v1::PalwTirCourtRulesV1,
    ) -> Result<kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2, String> {
        self.with_store(capture, rules.prompt_form, rules.max_step_leaf_count, |store| {
            let refutation = store.cone_refutation(index, rules).map_err(|e| e.to_string())?;
            let mut proof = kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2::TirCone { refutation: Box::new(refutation) };
            proof.tir_strip_program_v1();
            Ok(proof)
        })
    }

    /// **The named-leaf challenge of a dissected leaf** (RFC-0002 F7): the leaf and its opening, nothing else — no param is read,
    /// so it builds without rows too; it is here so a shard seat's finding at a dissected leaf has the same candidate a full
    /// holder's has.
    pub fn named_leaf_close(
        &self,
        capture: &super::backend::TirCaptureV1,
        index: u64,
        rules: &kaspa_consensus_core::palw_tir_court_v1::PalwTirCourtRulesV1,
    ) -> Result<kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2, String> {
        self.with_store(capture, rules.prompt_form, rules.max_step_leaf_count, |store| {
            let refutation =
                kaspa_consensus_core::palw_tir_court_v1::build_tir_named_leaf_refutation_v1(&capture.binding, index, store)
                    .map_err(|e| e.to_string())?;
            let mut proof = kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2::TirCone { refutation: Box::new(refutation) };
            proof.tir_strip_program_v1();
            Ok(proof)
        })
    }

    /// **The decode-token door of row `row`** with `beat_lane` the lane said to beat the committed token (the seat's own selection):
    /// capture and pin only, no weights.
    pub fn decode_token_close(
        &self,
        capture: &super::backend::TirCaptureV1,
        row: u32,
        beat_lane: u32,
    ) -> Result<kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2, String> {
        use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2 as P;
        let binding = Box::new(capture.binding.clone());
        let mut proof = if Hash64::from_bytes(self.space.program.logits_scheme_id)
            == kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1()
        {
            let pin = kaspa_consensus_core::palw_step_refute::tiled_decode_pin_v1(
                &capture.binding.job_context,
                &capture.logits_rows,
                &capture.generated,
                row,
                beat_lane,
            )
            .ok_or("the capture's trace has no such row or lane")?;
            P::TirDecodeTokenTiled { binding, pin }
        } else {
            let pin = kaspa_consensus_core::palw_step_refute::PalwBase0DecodeTokensV1 {
                logits_rows: capture.logits_rows.clone(),
                generated_token_ids: capture.generated.clone(),
            };
            P::TirDecodeToken { binding, pin, position: row }
        };
        proof.tir_strip_program_v1();
        Ok(proof)
    }
}

// ---------------------------------------------------------------------------------------------
// Over the network (D-S6): the serving half, the pursuit, and the in-memory fetcher
// ---------------------------------------------------------------------------------------------

/// **The serving half**: the `count` rows from `first` that this holder opens, as one reply — at most
/// [`kaspa_consensus_core::palw_tir_shard_v1::PALW_TIR_ROWS_PER_REPLY_MAX_V1`] of them and at most `byte_cap` bytes of
/// borsh, the longest prefix that fits (at least one row, or nothing). `None` when this holder cannot open `first` (it
/// does not hold the class, or `first` is past the inventory): silence, never an error on the wire.
pub fn serve_rows_v1(holder: &dyn TirParamOpenerV1, first: u32, count: u32, byte_cap: usize) -> Option<Vec<u8>> {
    use kaspa_consensus_core::palw_tir_shard_v1::PALW_TIR_ROWS_PER_REPLY_MAX_V1;
    if count == 0 {
        return None;
    }
    let head = holder.param_opening(first)?;
    let total = head.leaf_count;
    let mut openings = vec![head];
    let mut size = 4 + borsh::object_length(&openings[0]).ok()?;
    if size > byte_cap {
        return None;
    }
    let end = first.saturating_add(count.min(PALW_TIR_ROWS_PER_REPLY_MAX_V1)).min(total);
    for leaf in first + 1..end {
        let Some(opening) = holder.param_opening(leaf) else { break };
        let len = borsh::object_length(&opening).ok()?;
        if size + len > byte_cap {
            break;
        }
        size += len;
        openings.push(opening);
    }
    borsh::to_vec(&openings).ok()
}

/// **One shard's fetch as a state machine** the node drives a request per tick: it names the next leaf to ask for, takes a reply
/// and keeps the rows that are proven, and is `complete` when every leaf of the shard's ranges is held. Nothing a peer says is
/// believed: a reply is refused whole when any row in it is not the leaf its position asks for, is not a leaf of this shard's
/// ranges, or does not hash up to the registered root.
pub struct TirRowPursuitV1 {
    class: PalwTirClassV1,
    class_id: Hash64,
    root: Hash64,
    s_l: u16,
    shard: u16,
    wanted: std::collections::BTreeSet<u32>,
    got: BTreeMap<u32, PalwArtifactOpeningV1>,
    /// Replies refused, for the log and for the node's patience.
    pub refused: u32,
}

impl TirRowPursuitV1 {
    pub fn new(class: &PalwTirClassV1, class_id: Hash64, root: Hash64, s_l: u16, shard: u16) -> Result<Self, String> {
        if class.class_id(&root) != class_id {
            return Err("the class (program and layout) is not the one the chain registered under this root".into());
        }
        let space = PalwTirStepSpaceV1::new(class).map_err(|e| e.to_string())?;
        let (parts, _) = tir_shard_geometry_over_v1(&space, s_l)?;
        let layers = parts.get(usize::from(shard)).cloned().ok_or_else(|| format!("shard {shard} of {s_l}"))?;
        let ranges = shard_rules::palw_tir_shard_inventory_ranges_v1(&space.program, layers, shard == 0, shard + 1 == s_l);
        let wanted = ranges.iter().flat_map(|r| r.clone()).collect();
        Ok(Self { class: class.clone(), class_id, root, s_l, shard, wanted, got: BTreeMap::new(), refused: 0 })
    }

    /// The next run to ask for: the first leaf not yet held and how many consecutive wanted leaves follow it still lacking (at most the
    /// per-reply ceiling) — exactly the rows needed, none of the other shards'. `None` when the shard is complete.
    pub fn next_ask(&self) -> Option<(u32, u32)> {
        use kaspa_consensus_core::palw_tir_shard_v1::PALW_TIR_ROWS_PER_REPLY_MAX_V1;
        let first = self.wanted.iter().copied().find(|l| !self.got.contains_key(l))?;
        let mut count = 1u32;
        while count < PALW_TIR_ROWS_PER_REPLY_MAX_V1 {
            let Some(next) = first.checked_add(count) else { break };
            if !self.wanted.contains(&next) || self.got.contains_key(&next) {
                break;
            }
            count += 1;
        }
        Some((first, count))
    }

    pub fn complete(&self) -> bool {
        self.next_ask().is_none()
    }

    /// Leaves held of leaves wanted.
    pub fn progress(&self) -> (usize, usize) {
        (self.got.len(), self.wanted.len())
    }

    /// **Take a reply to the ask for `count` rows from `first`.** The rows must start at `first` and run consecutively, no more than
    /// asked; each must be a wanted leaf and prove against the root. Returns the rows newly kept; an `Err` keeps none.
    pub fn admit(&mut self, first: u32, count: u32, reply: &[u8]) -> Result<usize, String> {
        use kaspa_consensus_core::palw_tir_shard_v1::PALW_TIR_ROWS_PER_REPLY_MAX_V1;
        let refuse = |me: &mut Self, why: String| {
            me.refused += 1;
            Err(why)
        };
        let Ok(openings) = borsh::from_slice::<Vec<PalwArtifactOpeningV1>>(reply) else {
            return refuse(self, "the reply is not a list of openings".into());
        };
        if openings.is_empty() || openings.len() as u32 > count.min(PALW_TIR_ROWS_PER_REPLY_MAX_V1) {
            return refuse(self, format!("a reply of {} rows to an ask for {count} (1 ..= {PALW_TIR_ROWS_PER_REPLY_MAX_V1})", openings.len()));
        }
        let mut proven = Vec::with_capacity(openings.len());
        for (i, opening) in openings.into_iter().enumerate() {
            let want = first.wrapping_add(i as u32);
            if opening.leaf_index != want {
                return refuse(self, format!("the holder opened leaf {} where leaf {want} was asked", opening.leaf_index));
            }
            if !self.wanted.contains(&want) {
                return refuse(self, format!("leaf {want} is not one of shard {}'s rows", self.shard));
            }
            if verify_artifact_opening_v1(&opening, self.root).is_err() {
                return refuse(self, format!("leaf {want} does not open the registered root"));
            }
            proven.push(opening);
        }
        let mut fresh = 0;
        for opening in proven {
            if self.got.insert(opening.leaf_index, opening).is_none() {
                fresh += 1;
            }
        }
        Ok(fresh)
    }

    /// The holding the proven rows make (each opening is checked again by the same code the file mirror's fetch runs).
    pub fn holding(&self) -> Result<TirShardHoldingV1, String> {
        if !self.complete() {
            return Err("the shard's rows are not all held".into());
        }
        fetch_shard_params_v1(&self.class, self.class_id, self.root, self.s_l, self.shard, &TirHeldRowsV1(&self.got))
    }
}

/// A fetcher over rows already held (proven), by leaf.
pub struct TirHeldRowsV1<'a>(pub &'a BTreeMap<u32, PalwArtifactOpeningV1>);

impl TirRowFetcherV1 for TirHeldRowsV1<'_> {
    fn open_rows(&self, leaves: &[u32]) -> Result<Vec<PalwArtifactOpeningV1>, String> {
        leaves.iter().map(|l| self.0.get(l).cloned().ok_or_else(|| format!("leaf {l} is not held"))).collect()
    }
}
