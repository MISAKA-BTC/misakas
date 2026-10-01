//! **RFC-0003: the pipeline inventory — the artifact layout a generative class's `artifact_root`
//! commits to**, and the court's reading of a class's params from openings authenticated against it.
//!
//! A pipeline class's weights are the declared params of its programs, so the inventory is Phase F's
//! (F3, [`crate::palw_tir_artifact_v1`]) applied program by program, in the pipeline's program order,
//! into ONE tree:
//!
//! * program `0`'s leaves, then program `1`'s, …; within a program, Phase F's order exactly — each
//!   declared param in declaration order (never an input: a version-2 program's inputs are the job's,
//!   and its version-1 view's appended params are not weights), each instance, each row, each 32 KiB
//!   piece;
//! * each leaf is the ordinary artifact leaf over `PalwArtifactOperandV1 { tensor_name:
//!   "p<k>/<ParamDecl.name>", layer, row_start, bytes }` — the program index prefixed, so two programs
//!   that declare a param of one name never share a leaf's identity — and the root is the ordinary
//!   artifact tree ([`artifact_root_v1`]), so every opening a generative court checks is an ordinary
//!   artifact opening ([`verify_artifact_opening_v1`]).
//!
//! Every leaf's position is a closed form of the declarations ([`PalwGenInventoryIndexV1`], Phase F's
//! index per program with a base), so a court names the leaf an operand lives in without the artifact.
//! A registration binds the weights by this root: an executor that runs other weights commits leaves
//! a court re-evaluates from the root's openings, and is convicted at the first divergent leaf.

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::Hash64;
use crate::palw_artifact::{
    PalwArtifactMerkleFrontierV1, PalwArtifactOpeningV1, PalwArtifactOperandV1, artifact_leaf_parts_v1, open_artifact_leaf_v1,
    verify_artifact_opening_v1,
};
use crate::palw_tir_artifact_v1::{
    PalwTirInventoryError, PalwTirInventoryRowV1, palw_tir_inventory_leaf_count_v1, palw_tir_visit_inventory_rows_v1,
};
use crate::palw_tir_court_v1::PalwTirInventoryIndexV1;
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir::pipeline::PipelineParams;
use misaka_palw_tir::program_v2::TirProgramV2;

/// **Program `k`'s param view**: its version-1 view with the declared params only (the appended
/// inputs cut), each named `p<k>/<name>` — what Phase F's inventory functions lay out.
pub fn palw_gen_param_view_v1(k: u16, program: &TirProgramV2) -> TirProgramV1 {
    let mut view = program.v1_view();
    view.params.truncate(program.params.len());
    for d in &mut view.params {
        d.name = format!("p{k}/{}", d.name);
    }
    view
}

/// **Where every param byte of a pipeline class is committed**: Phase F's index for each program,
/// with the program's first leaf as its base, and its inverse.
#[derive(Clone, Debug)]
pub struct PalwGenInventoryIndexV1 {
    views: Vec<TirProgramV1>,
    programs: Vec<(u64, PalwTirInventoryIndexV1)>,
    leaf_count: u32,
}

/// One pipeline inventory leaf's coordinates: `(program, param, layer, byte offset, bytes)`.
pub type PalwGenInventoryPieceV1 = (u16, u16, Option<u16>, u32, u32);

impl PalwGenInventoryIndexV1 {
    /// The index of a class's inventory, or `None` when the inventory refuses the class (a tensor
    /// past 4 GiB, past a `u32` of leaves, or no param in any program: no weights, no root).
    pub fn new(programs: &[TirProgramV2]) -> Option<Self> {
        let views: Vec<TirProgramV1> = programs.iter().enumerate().map(|(k, p)| palw_gen_param_view_v1(k as u16, p)).collect();
        let mut out = Vec::with_capacity(views.len());
        let mut base = 0u64;
        for view in &views {
            let index = PalwTirInventoryIndexV1::new(view)?;
            out.push((base, index));
            base = base.checked_add(out.last().expect("pushed").1.leaf_count() as u64)?;
        }
        let leaf_count = u32::try_from(base).ok().filter(|n| *n > 0)?;
        Some(Self { views, programs: out, leaf_count })
    }

    /// Leaves of the inventory.
    pub fn leaf_count(&self) -> u32 {
        self.leaf_count
    }

    /// Program `k`'s param view.
    pub fn view(&self, k: u16) -> Option<&TirProgramV1> {
        self.views.get(k as usize)
    }

    /// The leaf holding byte `byte` of program `program`'s param instance `(param, layer)`.
    pub fn leaf_of(&self, program: u16, param: u16, layer: Option<u16>, byte: u64) -> Option<u32> {
        let (base, index) = self.programs.get(program as usize)?;
        u32::try_from(base + index.leaf_of(param, layer, byte)? as u64).ok()
    }

    /// The coordinates of leaf `leaf`.
    pub fn piece_of(&self, leaf: u32) -> Option<PalwGenInventoryPieceV1> {
        let k = self.programs.partition_point(|(base, _)| *base <= leaf as u64).checked_sub(1)?;
        let (base, index) = &self.programs[k];
        let (param, layer, start, len) = index.piece_of(u32::try_from(leaf as u64 - base).ok()?)?;
        Some((k as u16, param, layer, start, len))
    }
}

/// Every program's rows, in inventory order: `(program, row)`.
fn rows(programs: &[TirProgramV2]) -> Result<Vec<(u16, PalwTirInventoryRowV1)>, PalwTirInventoryError> {
    let mut out = Vec::new();
    for (k, p) in programs.iter().enumerate() {
        let view = palw_gen_param_view_v1(k as u16, p);
        if view.params.is_empty() {
            continue;
        }
        palw_tir_inventory_leaf_count_v1(&view)?;
        palw_tir_visit_inventory_rows_v1(&view, &mut |row| out.push((k as u16, row)))?;
    }
    if out.is_empty() {
        return Err(PalwTirInventoryError::Empty);
    }
    u32::try_from(out.len()).map_err(|_| PalwTirInventoryError::TooManyLeaves(out.len() as u64))?;
    Ok(out)
}

/// Program `k`'s param instance `(j, layer)` as the inventory reads it: little-endian at its
/// declared width, checked against its declaration.
fn instance_bytes<'s>(
    view: &TirProgramV1,
    params: &'s dyn PipelineParams,
    k: u16,
    j: u16,
    layer: Option<u16>,
) -> Result<Cow<'s, [u8]>, PalwTirInventoryError> {
    let d = &view.params[j as usize];
    let t = params.params(k).param(j, layer).ok_or_else(|| PalwTirInventoryError::Missing { name: d.name.clone(), layer })?;
    let bytes = t.to_le_bytes();
    let want = crate::palw_tir_artifact_v1::palw_tir_tensor_bytes_v1(view, j);
    if t.dtype != d.dtype || bytes.len() as u64 != want {
        return Err(PalwTirInventoryError::Length { name: d.name.clone(), layer, got: bytes.len() as u64, want });
    }
    Ok(Cow::Owned(bytes))
}

/// Walk every leaf's operand in inventory order, one param instance resident at a time.
fn walk(
    programs: &[TirProgramV2],
    params: &dyn PipelineParams,
    visit: &mut dyn FnMut(&str, Option<u16>, u32, &[u8]),
) -> Result<u32, PalwTirInventoryError> {
    let all = rows(programs)?;
    let views: Vec<TirProgramV1> = programs.iter().enumerate().map(|(k, p)| palw_gen_param_view_v1(k as u16, p)).collect();
    let mut held: Option<((u16, u16, Option<u16>), Cow<'_, [u8]>)> = None;
    for (k, row) in &all {
        let key = (*k, row.param, row.layer);
        if held.as_ref().is_none_or(|(h, _)| *h != key) {
            held = Some((key, instance_bytes(&views[*k as usize], params, *k, row.param, row.layer)?));
        }
        let bytes = &held.as_ref().expect("held").1;
        let piece = &bytes[row.row_start as usize..row.row_start as usize + row.len as usize];
        visit(&views[*k as usize].params[row.param as usize].name, row.layer, row.row_start, piece);
    }
    Ok(all.len() as u32)
}

/// **Walk a pipeline class's inventory in order, one param instance resident at a time**: `visit(name, layer,
/// row_start, bytes)` is each leaf's operand. Returns the leaf count. The public face of the walk the root, the
/// operands and the readiness material are all built from, so a node that proves possession (a readiness
/// multiproof) or serves an opening streams the one walk and never holds the artifact.
pub fn palw_gen_visit_inventory_v1(
    programs: &[TirProgramV2],
    params: &dyn PipelineParams,
    visit: &mut dyn FnMut(&str, Option<u16>, u32, &[u8]),
) -> Result<u32, PalwTirInventoryError> {
    walk(programs, params, visit)
}

/// **A pipeline class's artifact root, streamed** from its programs and their params. Returns
/// `(root, leaf_count)`.
pub fn palw_gen_inventory_root_v1(
    programs: &[TirProgramV2],
    params: &dyn PipelineParams,
) -> Result<(Hash64, u32), PalwTirInventoryError> {
    let mut frontier = PalwArtifactMerkleFrontierV1::new();
    let count =
        walk(programs, params, &mut |name, layer, start, piece| frontier.push(artifact_leaf_parts_v1(name, layer, start, piece)))?;
    debug_assert_eq!(frontier.leaf_count(), count as u64);
    Ok((frontier.root().ok_or(PalwTirInventoryError::Empty)?, count))
}

/// Every leaf's operand, materialised (small artifacts and tests; a real artifact streams).
pub fn palw_gen_inventory_operands_v1(
    programs: &[TirProgramV2],
    params: &dyn PipelineParams,
) -> Result<Vec<PalwArtifactOperandV1>, PalwTirInventoryError> {
    let mut out = Vec::new();
    walk(programs, params, &mut |name, layer, start, piece| {
        out.push(PalwArtifactOperandV1 { tensor_name: name.to_string(), layer, row_start: start, bytes: piece.to_vec() })
    })?;
    Ok(out)
}

/// **Does a node's copy of a class's weights hash to the class's `artifact_root`?** — what a worker
/// and a seat ask before they serve a class (a copy that does not is not the registered class).
pub fn palw_gen_artifact_matches_v1(
    programs: &[TirProgramV2],
    params: &dyn PipelineParams,
    artifact_root: &Hash64,
) -> Result<(), PalwGenParamRefusalV1> {
    let (root, _) = palw_gen_inventory_root_v1(programs, params).map_err(|e| PalwGenParamRefusalV1::Inventory(e.to_string()))?;
    if root != *artifact_root {
        return Err(PalwGenParamRefusalV1::NotTheRoot);
    }
    Ok(())
}

/// **The openings of `leaves`** from a node's copy (materialised; ascending, deduplicated).
pub fn palw_gen_open_leaves_v1(
    programs: &[TirProgramV2],
    params: &dyn PipelineParams,
    leaves: impl IntoIterator<Item = u32>,
) -> Result<Vec<PalwArtifactOpeningV1>, PalwTirInventoryError> {
    let operands = palw_gen_inventory_operands_v1(programs, params)?;
    let wanted: std::collections::BTreeSet<u32> = leaves.into_iter().collect();
    let count = operands.len() as u32;
    wanted
        .into_iter()
        .map(|i| open_artifact_leaf_v1(&operands, i).ok_or(PalwTirInventoryError::IndexOutOfRange { index: i, count }))
        .collect()
}

/// Why carried param openings are refused (the close then convicts nobody).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwGenParamRefusalV1 {
    #[error("the class's params have no inventory: {0}")]
    Inventory(String),
    #[error("the param openings are not in ascending leaf order")]
    NotAscending,
    #[error("a param opening is of another inventory (leaf count {got}, the class's {want})")]
    AnotherInventory { got: u32, want: u32 },
    #[error("param opening {0} is not its leaf's canonical piece")]
    NotCanonical(u32),
    #[error("param opening {0} does not reach the class's artifact root")]
    NotUnderTheRoot(u32),
    #[error("the weights do not hash to the class's artifact root")]
    NotTheRoot,
}

/// **A class's params as a court reads them**: the carried openings, each one whole leaf of the
/// class's inventory, authenticated against its `artifact_root` ([`Self::authenticate`]); an element
/// is read from the leaf the index names, and an element of an uncarried leaf is missing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwGenOpenedParamsV1 {
    /// Leaf → `(row_start, bytes)`.
    leaves: BTreeMap<u32, (u32, Vec<u8>)>,
}

impl PalwGenOpenedParamsV1 {
    /// Authenticate carried openings: ascending, each of this inventory, each its leaf's canonical
    /// piece (`p<k>/<name>`, layer, offset, length), each reaching `artifact_root`.
    pub fn authenticate(
        inventory: &PalwGenInventoryIndexV1,
        artifact_root: &Hash64,
        openings: &[PalwArtifactOpeningV1],
    ) -> Result<Self, PalwGenParamRefusalV1> {
        let mut leaves = BTreeMap::new();
        let mut last: Option<u32> = None;
        for o in openings {
            if last.is_some_and(|l| l >= o.leaf_index) {
                return Err(PalwGenParamRefusalV1::NotAscending);
            }
            last = Some(o.leaf_index);
            if o.leaf_count != inventory.leaf_count() {
                return Err(PalwGenParamRefusalV1::AnotherInventory { got: o.leaf_count, want: inventory.leaf_count() });
            }
            let (k, j, layer, start, len) =
                inventory.piece_of(o.leaf_index).ok_or(PalwGenParamRefusalV1::NotCanonical(o.leaf_index))?;
            let name = &inventory.view(k).expect("a piece names a program").params[j as usize].name;
            if o.operand.tensor_name != *name
                || o.operand.layer != layer
                || o.operand.row_start != start
                || o.operand.bytes.len() != len as usize
            {
                return Err(PalwGenParamRefusalV1::NotCanonical(o.leaf_index));
            }
            verify_artifact_opening_v1(o, *artifact_root).map_err(|_| PalwGenParamRefusalV1::NotUnderTheRoot(o.leaf_index))?;
            leaves.insert(o.leaf_index, (o.operand.row_start, o.operand.bytes.clone()));
        }
        Ok(Self { leaves })
    }

    /// Element `index` of program `program`'s param instance `(param, layer)`, from its carried leaf:
    /// `Ok(None)` when the leaf is not carried, `Err` when no such element exists.
    pub fn element(
        &self,
        inventory: &PalwGenInventoryIndexV1,
        program: u16,
        param: u16,
        layer: Option<u16>,
        index: usize,
    ) -> Result<Option<i128>, String> {
        let view = inventory.view(program).ok_or_else(|| format!("no program {program}"))?;
        let d = view.params.get(param as usize).ok_or_else(|| format!("program {program} has no param {param}"))?;
        let width = d.dtype.width();
        let byte = (index as u64).saturating_mul(width as u64);
        let leaf = inventory
            .leaf_of(program, param, layer, byte)
            .ok_or_else(|| format!("program {program} param {param} layer {layer:?} has no byte {byte}"))?;
        let Some((row_start, bytes)) = self.leaves.get(&leaf) else { return Ok(None) };
        let at = byte.checked_sub(*row_start as u64).map(|a| a as usize);
        let element = at.and_then(|a| bytes.get(a..a + width)).ok_or_else(|| format!("leaf {leaf} does not hold byte {byte}"))?;
        Ok(Some(d.dtype.decode_le(element)))
    }

    /// The inventory leaf element `index` of program `program`'s param instance `(param, layer)`
    /// lives in, carried or not (`None`: no such element).
    pub fn leaf_of_element(
        &self,
        inventory: &PalwGenInventoryIndexV1,
        program: u16,
        param: u16,
        layer: Option<u16>,
        index: usize,
    ) -> Option<u32> {
        let d = inventory.view(program)?.params.get(param as usize)?;
        inventory.leaf_of(program, param, layer, (index as u64).saturating_mul(d.dtype.width() as u64))
    }

    /// The carried leaves.
    pub fn leaves(&self) -> impl Iterator<Item = u32> + '_ {
        self.leaves.keys().copied()
    }
}
