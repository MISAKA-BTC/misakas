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
            pieces.entry((j as u16, opening.operand.layer)).or_default().push((opening.operand.row_start, opening.operand.bytes));
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
    Ok(TirShardHoldingV1 { space, plan, params, class_id, shard, s_l, leaves, bytes })
}
