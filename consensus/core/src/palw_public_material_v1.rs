//! **Lane DA16 (RFC-0014 §16, RFC-0009 §4): public material — what a unit is, and what it is checked against.**
//!
//! **NON-CONSENSUS for every artifact unit (ADR-0177, 2026-10-10).** The chain does not interfere with model acquisition: no fold arm
//! reads `ArtifactLeaf`, `KernelCommitments`, `KernelRowNodes`, `KernelRow`, the manifest or the binding check any more. They remain the
//! optional off-chain tooling of `misaka-palw-remote::public_material` and `palw-evidence artifact-*` — a verifier's way to fetch and
//! check a model against the REGISTERED roots, never a duty anyone owes. The court's only unit is `ClaimPosition`
//! ([`crate::palw_court_scope_v1`] classifies every unit; the provider court refuses the artifact ones).
//!
//! A *unit* ([`PublicUnitV1`]) is the addressable piece of a subject's public material: one leaf of a V2 class's artifact inventory, the
//! kernel commitments its binding names, a run of row-tree nodes or one row of a bound kernel tensor, one committed position of a kernel
//! route claim. The SAME unit names a file in the transport (`misaka-palw-remote::public_material`) and a provider-court challenge
//! (`crate::palw_provider_court_v1`), and the SAME function judges an off-chain fetch and an on-chain answer.
//!
//! **The root of trust is always the chain's own root** — never a manifest, a provider's word or a signature:
//!
//! | Unit | Answer | Checked against |
//! |---|---|---|
//! | `ArtifactLeaf` | [`PalwArtifactOpeningV1`] | the class's registered `artifact_root` |
//! | `KernelCommitments` | [`ParamCommitmentsV1`] | `root() == kernel_param_root` (the binding's) |
//! | `KernelRowNodes` | the commitments + a [`TensorRowNodesV1`] run | the instance's tensor commitment under that root |
//! | `KernelRow` | the commitments + a row [`TensorOpeningV1`] | likewise |
//! | `ClaimPosition` | a `PositionResponseV1` | the claim row's committed values (the kernel's own `Respond` classification) |
//!
//! **Confirming a binding from the bytes** ([`binding_check_from_leaves_v1`]): every leaf at the coordinates the program's closed-form
//! layout fixes, their root equal to the class's `artifact_root`, every declared instance reassembled from its leaves, its
//! `ParamCommitmentsV1` computed — equal root: CONFIRMED (a verifier's own verdict; the chain needs only that no refutation was possible
//! to MISS, which the provider court's availability gives it); another root: the bound commitments are not of these bytes, and
//! [`row_refutation_v1`] turns one served kernel row into tag 105's existing proof.

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::merkle::{AXIS_ROW, LayoutV1, TensorOpeningV1, TensorRowNodesV1, level_count};
use misaka_palw_kernel::trace::ParamCommitmentsV1;
use misaka_palw_tir::{MapParams, Tensor, TirProgramV1};
use std::collections::{BTreeMap, BTreeSet};

use crate::Hash64;
use crate::palw_artifact::{
    PalwArtifactOpeningV1, PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1, open_artifact_leaves_v1,
    verify_artifact_opening_v1,
};
use crate::palw_onboarding_v1::{ArtifactMismatchProofV1, verify_artifact_mismatch_v1};
use crate::palw_tir_artifact_v1::{
    PalwTirInventoryRowV1, palw_tir_inventory_leaf_count_v1, palw_tir_leaf_index_v1, palw_tir_param_instances_v1,
    palw_tir_visit_inventory_rows_v1,
};

pub const PALW_PUBLIC_MATERIAL_VERSION_V1: u16 = 1;
/// The widest run of row-tree nodes one unit names: 1,024 hashes (64 KiB) — two runs localize a row of a 2^20-row tensor.
pub const PALW_PUBLIC_MATERIAL_MAX_RUN_V1: u32 = 1_024;
const ARTIFACT_MANIFEST_DOMAIN_V1: &[u8] = b"misaka-palw/public-material/artifact-manifest/v1";

/// **One addressable unit of a subject's public material** (module doc). A provider-court challenge may name only `ClaimPosition`;
/// the artifact variants are off-chain transport addresses (and refused by the court).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PublicUnitV1 {
    /// Leaf `index` of the V2 class's inventory (the program's closed-form layout fixes its coordinates).
    ArtifactLeaf { index: u32 } = 0,
    /// The kernel commitments the binding's root is the root of.
    KernelCommitments = 1,
    /// Nodes `[first, first + count)` at `level` (0 = the row leaves) of one instance's row tree under the bound root.
    KernelRowNodes { param: u16, layer: Option<u16>, level: u8, first: u64, count: u32 } = 2,
    /// One row (with its values) of one instance under the bound root.
    KernelRow { param: u16, layer: Option<u16>, row: u64 } = 3,
    /// One committed position of a kernel route claim (every value of it, as a `Respond` carries).
    ClaimPosition { stage: u8, position: u32 } = 4,
}

impl PublicUnitV1 {
    /// Whether this unit is a piece of an artifact (the other kind is a claim's).
    pub fn is_artifact_unit(&self) -> bool {
        !matches!(self, Self::ClaimPosition { .. })
    }

    /// The transport's file stem for this unit (stable, path-safe).
    pub fn file_stem(&self) -> String {
        let l = |layer: &Option<u16>| layer.map(|l| l.to_string()).unwrap_or_else(|| "g".to_string());
        match self {
            Self::ArtifactLeaf { index } => format!("leaf-{index}"),
            Self::KernelCommitments => "kernel-commitments".to_string(),
            Self::KernelRowNodes { param, layer, level, first, count } => {
                format!("kernel-nodes-{param}-{}-{level}-{first}-{count}", l(layer))
            }
            Self::KernelRow { param, layer, row } => format!("kernel-row-{param}-{}-{row}", l(layer)),
            Self::ClaimPosition { stage, position } => format!("position-{stage}-{position}"),
        }
    }
}

/// **The opened unit.**
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PublicUnitAnswerV1 {
    ArtifactLeaf {
        opening: PalwArtifactOpeningV1,
    } = 0,
    KernelCommitments {
        commitments: ParamCommitmentsV1,
    } = 1,
    KernelRowNodes {
        commitments: ParamCommitmentsV1,
        run: TensorRowNodesV1,
    } = 2,
    KernelRow {
        commitments: ParamCommitmentsV1,
        opening: TensorOpeningV1,
    } = 3,
    /// The `PositionResponseV1` bytes a `Respond` would carry.
    ClaimPosition {
        bytes: Vec<u8>,
    } = 4,
}

/// What the chain holds for an artifact subject: the class's program and registered root, and the kernel root the pair names.
#[derive(Clone, Copy, Debug)]
pub struct ArtifactFactsV1<'a> {
    pub program: &'a TirProgramV1,
    pub artifact_root: Hash64,
    pub kernel_param_root: Hash64,
}

/// The rows of one declared instance (`None`: not an instance the program declares, or a shape that overflows).
fn instance_rows(program: &TirProgramV1, param: u16, layer: Option<u16>) -> Option<u64> {
    let declared = palw_tir_param_instances_v1(program);
    if !declared.get(param as usize)?.contains(&layer) {
        return None;
    }
    let shape: Vec<usize> = program.params.get(param as usize)?.shape.iter().map(|d| *d as usize).collect();
    Some(LayoutV1::try_of(&shape)?.rows)
}

/// **Is `unit` a piece of this artifact subject?** — the challenge half: a unit outside what the subject commits is one nobody could
/// answer, so a challenge of it would be a default by construction; it is refused instead.
pub fn artifact_unit_in_scope_v1(facts: &ArtifactFactsV1<'_>, unit: &PublicUnitV1) -> Result<(), &'static str> {
    match *unit {
        PublicUnitV1::ArtifactLeaf { index } => {
            let count = palw_tir_inventory_leaf_count_v1(facts.program).map_err(|_| "the program has no inventory")?;
            (index < count).then_some(()).ok_or("the inventory has no such leaf")
        }
        PublicUnitV1::KernelCommitments => Ok(()),
        PublicUnitV1::KernelRowNodes { param, layer, level, first, count } => {
            let rows = instance_rows(facts.program, param, layer).ok_or("the program declares no such instance")?;
            let width = level_count(rows, level).ok_or("the instance's row tree has no such level")?;
            if count == 0 || count > PALW_PUBLIC_MATERIAL_MAX_RUN_V1 {
                return Err("a run is one to 1,024 nodes");
            }
            match first.checked_add(count as u64) {
                Some(end) if end <= width => Ok(()),
                _ => Err("the run ends past its level"),
            }
        }
        PublicUnitV1::KernelRow { param, layer, row } => {
            let rows = instance_rows(facts.program, param, layer).ok_or("the program declares no such instance")?;
            (row < rows).then_some(()).ok_or("the instance has no such row")
        }
        PublicUnitV1::ClaimPosition { .. } => Err("a claim position is not a piece of an artifact"),
    }
}

/// The bound commitments, checked against the binding's root.
fn bound<'c>(facts: &ArtifactFactsV1<'_>, commitments: &'c ParamCommitmentsV1) -> Result<&'c ParamCommitmentsV1, &'static str> {
    (commitments.root() == facts.kernel_param_root.as_bytes())
        .then_some(commitments)
        .ok_or("the commitments do not root to the bound kernel root")
}

/// **Does `answer` open `unit` of this artifact subject?** — the answer half, hash arithmetic against the chain's roots only.
pub fn verify_artifact_answer_v1(
    facts: &ArtifactFactsV1<'_>,
    unit: &PublicUnitV1,
    answer: &PublicUnitAnswerV1,
) -> Result<(), &'static str> {
    artifact_unit_in_scope_v1(facts, unit)?;
    match (*unit, answer) {
        (PublicUnitV1::ArtifactLeaf { index }, PublicUnitAnswerV1::ArtifactLeaf { opening }) => {
            let count = palw_tir_inventory_leaf_count_v1(facts.program).map_err(|_| "the program has no inventory")?;
            if opening.leaf_index != index || opening.leaf_count != count {
                return Err("the opening is of another leaf or another inventory size");
            }
            verify_artifact_opening_v1(opening, facts.artifact_root).map_err(|_| "the leaf does not reach the class's artifact root")
        }
        (PublicUnitV1::KernelCommitments, PublicUnitAnswerV1::KernelCommitments { commitments }) => {
            bound(facts, commitments).map(|_| ())
        }
        (
            PublicUnitV1::KernelRowNodes { param, layer, level, first, count },
            PublicUnitAnswerV1::KernelRowNodes { commitments, run },
        ) => {
            let c =
                bound(facts, commitments)?.by_instance.get(&(param, layer)).ok_or("the bound commitments hold no such instance")?;
            if run.level != level || run.first != first || run.nodes.len() as u64 != count as u64 {
                return Err("the run is of another level, start or length");
            }
            run.authenticates(c).then_some(()).ok_or("the run does not authenticate against the instance's commitment")
        }
        (PublicUnitV1::KernelRow { param, layer, row }, PublicUnitAnswerV1::KernelRow { commitments, opening }) => {
            let c =
                bound(facts, commitments)?.by_instance.get(&(param, layer)).ok_or("the bound commitments hold no such instance")?;
            if opening.axis != AXIS_ROW || opening.index != row {
                return Err("the opening is of another row or axis");
            }
            opening.authenticates(c).then_some(()).ok_or("the row does not authenticate against the instance's commitment")
        }
        _ => Err("the answer is not of the unit named"),
    }
}

/// **The answer to an artifact unit, from the TRUE bytes** (`leaves`) — what an honest provider of an honest binding produces (the kernel
/// side is computed from the same bytes). `None` when the unit is out of scope or the bytes do not decode.
pub fn answer_artifact_unit_v1(
    program: &TirProgramV1,
    leaves: &[PalwArtifactOperandV1],
    unit: &PublicUnitV1,
) -> Option<PublicUnitAnswerV1> {
    match *unit {
        PublicUnitV1::ArtifactLeaf { index } => {
            Some(PublicUnitAnswerV1::ArtifactLeaf { opening: open_artifact_leaves_v1(leaves, &[index])?.pop()? })
        }
        PublicUnitV1::KernelCommitments => {
            Some(PublicUnitAnswerV1::KernelCommitments { commitments: commitments_from_leaves_v1(program, leaves).ok()?.0 })
        }
        PublicUnitV1::KernelRowNodes { param, layer, level, first, count } => {
            let (commitments, params) = commitments_from_leaves_v1(program, leaves).ok()?;
            let run = TensorRowNodesV1::of(params.tensors.get(&(param, layer))?, level, first, count as u64)?;
            Some(PublicUnitAnswerV1::KernelRowNodes { commitments, run })
        }
        PublicUnitV1::KernelRow { param, layer, row } => {
            let (commitments, params) = commitments_from_leaves_v1(program, leaves).ok()?;
            let opening = TensorOpeningV1::row(params.tensors.get(&(param, layer))?, row)?;
            Some(PublicUnitAnswerV1::KernelRow { commitments, opening })
        }
        PublicUnitV1::ClaimPosition { .. } => None,
    }
}

// ---- the artifact from its bytes ------------------------------------------------------------------------------------------

/// Every leaf's coordinates, in inventory order (the program's closed form).
pub fn canonical_leaf_rows_v1(program: &TirProgramV1) -> Result<Vec<PalwTirInventoryRowV1>, String> {
    let mut rows = Vec::new();
    palw_tir_visit_inventory_rows_v1(program, &mut |r| rows.push(r)).map_err(|e| e.to_string())?;
    Ok(rows)
}

/// **Is `operand` leaf `index` of this program's canonical layout?** (name, layer, byte offset and length all as the layout fixes them).
pub fn leaf_at_canonical_coordinates_v1(program: &TirProgramV1, row: &PalwTirInventoryRowV1, operand: &PalwArtifactOperandV1) -> bool {
    program.params.get(row.param as usize).is_some_and(|d| d.name == operand.tensor_name)
        && operand.layer == row.layer
        && operand.row_start == row.row_start
        && operand.bytes.len() as u64 == row.len as u64
}

/// The declared tensors reassembled from canonical leaves, and their kernel commitments.
pub fn commitments_from_leaves_v1(
    program: &TirProgramV1,
    leaves: &[PalwArtifactOperandV1],
) -> Result<(ParamCommitmentsV1, MapParams), String> {
    let rows = canonical_leaf_rows_v1(program)?;
    if rows.len() != leaves.len() {
        return Err(format!("{} leaves, the program's inventory has {}", leaves.len(), rows.len()));
    }
    let mut bytes: BTreeMap<(u16, Option<u16>), Vec<u8>> = BTreeMap::new();
    for (i, (row, leaf)) in rows.iter().zip(leaves).enumerate() {
        if !leaf_at_canonical_coordinates_v1(program, row, leaf) {
            return Err(format!("leaf {i} is not at the coordinates the program's layout fixes"));
        }
        bytes.entry((row.param, row.layer)).or_default().extend_from_slice(&leaf.bytes);
    }
    let mut params = MapParams::default();
    for ((param, layer), b) in bytes {
        let decl = &program.params[param as usize];
        let shape: Vec<usize> = decl.shape.iter().map(|d| *d as usize).collect();
        let t = Tensor::from_le_bytes(decl.dtype, &shape, &b).map_err(|e| format!("param {param} layer {layer:?}: {e}"))?;
        params.tensors.insert((param, layer), t);
    }
    Ok((ParamCommitmentsV1::of(&params), params))
}

/// **What the bytes say about a binding.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingCheckV1 {
    /// The bytes root to the class's artifact root AND their kernel commitments root to the bound kernel root: the binding is true.
    Confirmed,
    /// The bytes are the class's, but their kernel commitments root elsewhere: the binding is false. `true_commitments` are the bytes'
    /// own; the refuter localizes the difference against the bound commitments ([`differing_instances_v1`], [`differing_rows_v1`]).
    KernelRootDiffers { true_commitments: ParamCommitmentsV1 },
}

/// **Confirm or refute a binding from the artifact's bytes** (module doc). `Err`: the leaves are not the class's artifact (another layout,
/// another root, undecodable) — that is a fetch failure, never a verdict about the binding.
pub fn binding_check_from_leaves_v1(
    program: &TirProgramV1,
    leaves: &[PalwArtifactOperandV1],
    artifact_root: Hash64,
    kernel_param_root: Hash64,
) -> Result<BindingCheckV1, String> {
    let count = palw_tir_inventory_leaf_count_v1(program).map_err(|e| e.to_string())?;
    if leaves.len() != count as usize {
        return Err(format!("{} leaves, the program's inventory has {count}", leaves.len()));
    }
    let root = artifact_root_v1(&leaves.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).ok_or("an empty inventory")?;
    if root != artifact_root {
        return Err("the leaves do not root to the class's artifact root: these are not its bytes".to_string());
    }
    let (true_commitments, _) = commitments_from_leaves_v1(program, leaves)?;
    if true_commitments.root() == kernel_param_root.as_bytes() {
        Ok(BindingCheckV1::Confirmed)
    } else {
        Ok(BindingCheckV1::KernelRootDiffers { true_commitments })
    }
}

/// The instances whose commitment differs between the true bytes and the bound commitments (a missing or surplus instance included).
pub fn differing_instances_v1(true_commitments: &ParamCommitmentsV1, bound: &ParamCommitmentsV1) -> Vec<(u16, Option<u16>)> {
    let keys: BTreeSet<(u16, Option<u16>)> = true_commitments.by_instance.keys().chain(bound.by_instance.keys()).copied().collect();
    keys.into_iter().filter(|k| true_commitments.by_instance.get(k) != bound.by_instance.get(k)).collect()
}

/// The rows a served run of the BOUND tensor covers that differ from the true tensor's (empty: the run agrees).
pub fn differing_rows_v1(true_tensor: &Tensor, run: &TensorRowNodesV1) -> Vec<u64> {
    let rows = LayoutV1::of(&true_tensor.shape).rows;
    let Some(truth) = misaka_palw_kernel::merkle::row_level_nodes(true_tensor, run.level) else { return Vec::new() };
    let mut out = Vec::new();
    for (k, node) in run.nodes.iter().enumerate() {
        let index = run.first + k as u64;
        if truth.get(index as usize) != Some(node) {
            out.extend(run.covered_rows(index, rows));
        }
    }
    out
}

/// **Tag 105's proof from the true bytes and one served row of the bound tensor** — the V2 leaf of the true bytes over the same
/// coordinates, checked by the very function the fold runs. `None` when the served row agrees with the true bytes on every byte the
/// leaves cover (or its instance's shape / dtype is the declared one and nothing differs).
pub fn row_refutation_v1(
    program: &TirProgramV1,
    leaves: &[PalwArtifactOperandV1],
    artifact_root: Hash64,
    bound: &ParamCommitmentsV1,
    param: u16,
    layer: Option<u16>,
    kernel_row: &TensorOpeningV1,
) -> Option<ArtifactMismatchProofV1> {
    let kernel_param_root = Hash64::from_bytes(bound.root());
    // A bound instance set that is not the declared one is refuted without any opening.
    let instances = ArtifactMismatchProofV1::Instances { commitments: bound.clone() };
    if verify_artifact_mismatch_v1(program, artifact_root, kernel_param_root, &instances).is_ok() {
        return Some(instances);
    }
    let decl = program.params.get(param as usize)?;
    let width = decl.dtype.width() as u64;
    let row_len = LayoutV1::try_of(&decl.shape.iter().map(|d| *d as usize).collect::<Vec<_>>())?.row_len;
    let start = kernel_row.index.checked_mul(row_len)?.checked_mul(width)?;
    let end = start.checked_add(row_len.checked_mul(width)?)?;
    // Every leaf the row's bytes fall in (a row over 32 KiB is several pieces), each tried with the fold's own judgement.
    let mut indices: Vec<u32> = Vec::new();
    let mut at = start;
    while at < end {
        let index = palw_tir_leaf_index_v1(program, param, layer, at)?;
        if indices.last() != Some(&index) {
            indices.push(index);
        }
        at = at.checked_add(1.max(crate::palw_tir_artifact_v1::PALW_TIR_ROW_PIECE_BYTES_V1.min(end - at)))?;
    }
    for opening in open_artifact_leaves_v1(leaves, &indices)? {
        let proof = ArtifactMismatchProofV1::Row {
            commitments: bound.clone(),
            param,
            layer,
            kernel_row: kernel_row.clone(),
            v2_opening: opening,
        };
        if verify_artifact_mismatch_v1(program, artifact_root, kernel_param_root, &proof).is_ok() {
            return Some(proof);
        }
    }
    None
}

// ---- the artifact manifest (an index, never a root of trust) ---------------------------------------------------------------

/// **The artifact's manifest**: the class record and every leaf's hash, so a fetcher checks each leaf on its own. Accepted only after
/// the class record derives the class id asked for over this root, the leaves root to the CHAIN's `artifact_root` and their count is the
/// program's ([`Self::check_against_chain`]) — so a node that knows only `(class id, root)` (its root-fetch hook) checks it completely.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ArtifactManifestV1 {
    pub version: u16,
    pub network_domain: Hash64,
    /// The V2 IR class record (program, layout, tokenizer id): `class.class_id(&artifact_root)` is the class id.
    pub class: crate::palw_tir_class_v1::PalwTirClassV1,
    pub artifact_root: Hash64,
    /// The kernel root of the pair the publisher vouches for (the binding's).
    pub kernel_param_root: Hash64,
    pub leaf_hashes: Vec<Hash64>,
    /// The publisher's retention promise (a promise, not proof: the provider court is what holds a provider to it).
    pub retain_until_daa: u64,
}

impl ArtifactManifestV1 {
    pub fn of(
        network_domain: Hash64,
        class: crate::palw_tir_class_v1::PalwTirClassV1,
        artifact_root: Hash64,
        kernel_param_root: Hash64,
        leaves: &[PalwArtifactOperandV1],
        retain_until_daa: u64,
    ) -> Self {
        Self {
            version: PALW_PUBLIC_MATERIAL_VERSION_V1,
            network_domain,
            class,
            artifact_root,
            kernel_param_root,
            leaf_hashes: leaves.iter().map(artifact_leaf_v1).collect(),
            retain_until_daa,
        }
    }

    pub fn id(&self) -> Hash64 {
        let mut s = blake2b_simd::Params::new().hash_length(64).key(ARTIFACT_MANIFEST_DOMAIN_V1).to_state();
        let bytes = borsh::to_vec(self).expect("a manifest serializes");
        s.update(&(bytes.len() as u64).to_le_bytes());
        s.update(&bytes);
        let mut out = [0u8; 64];
        out.copy_from_slice(s.finalize().as_bytes());
        Hash64::from_bytes(out)
    }

    /// The V2 class id this manifest's class record derives over its root.
    pub fn v2_class(&self) -> Hash64 {
        self.class.class_id(&self.artifact_root)
    }

    /// The class's program (canonical bytes, decoded).
    pub fn program(&self) -> Result<TirProgramV1, String> {
        self.class.decode_program().map_err(|e| e.to_string())
    }

    /// The manifest agrees with the CHAIN: this network, the class id asked for (derived from the carried record over the root), the
    /// registered root, the pair's kernel root (`None`: any — a fetch for the V2 bytes alone), the program's leaf count and Merkle root.
    pub fn check_against_chain(
        &self,
        network_domain: Hash64,
        v2_class: Hash64,
        artifact_root: Hash64,
        kernel_param_root: Option<Hash64>,
    ) -> Result<TirProgramV1, String> {
        if self.version != PALW_PUBLIC_MATERIAL_VERSION_V1 {
            return Err(format!("manifest version {}", self.version));
        }
        if self.network_domain != network_domain {
            return Err("the manifest is of another network".to_string());
        }
        if self.artifact_root != artifact_root || kernel_param_root.is_some_and(|k| k != self.kernel_param_root) {
            return Err("the manifest names another pair of roots than the chain".to_string());
        }
        if self.v2_class() != v2_class {
            return Err("the manifest's class record does not derive the class id asked for over this root".to_string());
        }
        let program = self.program()?;
        let count = palw_tir_inventory_leaf_count_v1(&program).map_err(|e| e.to_string())?;
        if self.leaf_hashes.len() != count as usize {
            return Err(format!("{} leaf hashes, the program's inventory has {count}", self.leaf_hashes.len()));
        }
        if artifact_root_v1(&self.leaf_hashes) != Some(artifact_root) {
            return Err("the manifest's leaf hashes do not root to the class's artifact root".to_string());
        }
        Ok(program)
    }

    /// One fetched leaf: its hash is the manifest's entry and it sits at the coordinates the layout fixes.
    pub fn verify_leaf(
        &self,
        program: &TirProgramV1,
        row: &PalwTirInventoryRowV1,
        index: u32,
        operand: &PalwArtifactOperandV1,
    ) -> Result<(), String> {
        let want = self.leaf_hashes.get(index as usize).ok_or_else(|| format!("leaf {index} is not in the manifest"))?;
        if !leaf_at_canonical_coordinates_v1(program, row, operand) {
            return Err(format!("leaf {index} is not at the coordinates the program's layout fixes"));
        }
        (artifact_leaf_v1(operand) == *want).then_some(()).ok_or_else(|| format!("leaf {index} does not hash to the manifest's entry"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_operands_v1};
    use std::borrow::Cow;

    struct Src(BTreeMap<(u16, Option<u16>), Vec<u8>>);
    impl PalwTirTensorSourceV1 for Src {
        fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
            self.0.get(&(param, layer)).map(|b| Cow::Borrowed(b.as_slice()))
        }
    }

    fn class_of(program: &TirProgramV1) -> crate::palw_tir_class_v1::PalwTirClassV1 {
        crate::palw_tir_class_v1::PalwTirClassV1 {
            version: crate::palw_tir_class_v1::PALW_TIR_CLASS_VERSION_V1,
            program: program.encode(),
            layout: crate::palw_tir_class_v1::PalwTirLayoutV1 {
                version: crate::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1,
                max_context: 64,
                checkpoint_interval: 2,
                h_tile: 2,
                commit_tiles: Vec::new(),
                state_tiles: Vec::new(),
            },
            tokenizer_id: Hash64::from_bytes([0x70; 64]),
        }
    }

    fn fixture(seed: u64) -> (TirProgramV1, MapParams, Vec<PalwArtifactOperandV1>, Hash64) {
        let fx = misaka_palw_tir_sketch::fixture::wide128_v1(seed);
        let src = Src(fx.params.tensors.iter().map(|(k, t)| (*k, t.to_le_bytes())).collect());
        let leaves = palw_tir_inventory_operands_v1(&fx.program, &src).unwrap();
        let root = artifact_root_v1(&leaves.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
        (fx.program, fx.params, leaves, root)
    }

    /// The bytes confirm an honest binding; another artifact's commitments are refuted from the true bytes and ONE served row of the
    /// bound tensor, localized by a coarse run then a leaf run — and the proof is the fold's own (`verify_artifact_mismatch_v1`).
    #[test]
    fn the_bytes_confirm_an_honest_binding_and_refute_a_false_one_through_two_runs_and_one_row() {
        let (program, params, leaves, root) = fixture(11);
        let pc = ParamCommitmentsV1::of(&params);
        let honest = Hash64::from_bytes(pc.root());
        assert_eq!(binding_check_from_leaves_v1(&program, &leaves, root, honest).unwrap(), BindingCheckV1::Confirmed);
        // The commitments of OTHER weights of the same program.
        let (_, wrong_params, wrong_leaves, _) = fixture(12);
        let wrong = ParamCommitmentsV1::of(&wrong_params);
        let false_root = Hash64::from_bytes(wrong.root());
        let BindingCheckV1::KernelRootDiffers { true_commitments } =
            binding_check_from_leaves_v1(&program, &leaves, root, false_root).unwrap()
        else {
            panic!("a false binding is not confirmed")
        };
        assert_eq!(true_commitments, pc);
        let instances = differing_instances_v1(&true_commitments, &wrong);
        assert!(!instances.is_empty());
        // The units the refuter forces out of the pair's provider (here answered from the BOUND bytes), each checked as the court does.
        let facts = ArtifactFactsV1 { program: &program, artifact_root: root, kernel_param_root: false_root };
        let (param, layer) = instances[0];
        let rows = LayoutV1::of(&params.tensors[&(param, layer)].shape).rows;
        let unit = PublicUnitV1::KernelRowNodes { param, layer, level: 0, first: 0, count: rows as u32 };
        let answer = answer_artifact_unit_v1(&program, &wrong_leaves, &unit).unwrap();
        verify_artifact_answer_v1(&facts, &unit, &answer).unwrap();
        let PublicUnitAnswerV1::KernelRowNodes { run, .. } = &answer else { unreachable!() };
        let differing = differing_rows_v1(&params.tensors[&(param, layer)], run);
        assert!(!differing.is_empty(), "the leaf run localizes a differing row");
        let unit = PublicUnitV1::KernelRow { param, layer, row: differing[0] };
        let answer = answer_artifact_unit_v1(&program, &wrong_leaves, &unit).unwrap();
        verify_artifact_answer_v1(&facts, &unit, &answer).unwrap();
        let PublicUnitAnswerV1::KernelRow { opening, .. } = &answer else { unreachable!() };
        let proof = row_refutation_v1(&program, &leaves, root, &wrong, param, layer, opening).expect("a refutation");
        verify_artifact_mismatch_v1(&program, root, false_root, &proof).unwrap();
        // Against the honest binding, the honest row refutes nothing.
        let honest_row = TensorOpeningV1::row(&params.tensors[&(param, layer)], differing[0]).unwrap();
        assert!(row_refutation_v1(&program, &leaves, root, &pc, param, layer, &honest_row).is_none());
    }

    /// Every artifact unit an honest provider of an honest binding answers from the bytes verifies; the same answers against a false
    /// kernel root, another unit, or a tampered opening do not; out-of-scope units are refused at the challenge half.
    #[test]
    fn honest_answers_verify_and_tampered_or_misplaced_ones_do_not() {
        let (program, params, leaves, root) = fixture(5);
        let pc = ParamCommitmentsV1::of(&params);
        let facts = ArtifactFactsV1 { program: &program, artifact_root: root, kernel_param_root: Hash64::from_bytes(pc.root()) };
        let (param, layer) = *params.tensors.keys().next().unwrap();
        let rows = LayoutV1::of(&params.tensors[&(param, layer)].shape).rows;
        let units = [
            PublicUnitV1::ArtifactLeaf { index: 0 },
            PublicUnitV1::ArtifactLeaf { index: leaves.len() as u32 - 1 },
            PublicUnitV1::KernelCommitments,
            PublicUnitV1::KernelRowNodes { param, layer, level: 0, first: 0, count: rows as u32 },
            PublicUnitV1::KernelRow { param, layer, row: rows - 1 },
        ];
        for unit in units {
            let answer = answer_artifact_unit_v1(&program, &leaves, &unit).unwrap();
            verify_artifact_answer_v1(&facts, &unit, &answer).unwrap_or_else(|e| panic!("{unit:?}: {e}"));
            let other = ArtifactFactsV1 { kernel_param_root: Hash64::from_bytes([9; 64]), ..facts };
            if !matches!(unit, PublicUnitV1::ArtifactLeaf { .. }) {
                assert!(verify_artifact_answer_v1(&other, &unit, &answer).is_err(), "{unit:?} under another kernel root");
            }
        }
        // Tampered leaf, misplaced leaf, the wrong unit kind.
        let PublicUnitAnswerV1::ArtifactLeaf { mut opening } = answer_artifact_unit_v1(&program, &leaves, &units[0]).unwrap() else {
            unreachable!()
        };
        opening.operand.bytes[0] ^= 1;
        assert!(verify_artifact_answer_v1(&facts, &units[0], &PublicUnitAnswerV1::ArtifactLeaf { opening: opening.clone() }).is_err());
        let good = answer_artifact_unit_v1(&program, &leaves, &units[0]).unwrap();
        assert!(verify_artifact_answer_v1(&facts, &units[1], &good).is_err(), "leaf 0 is not leaf n-1");
        assert!(verify_artifact_answer_v1(&facts, &units[2], &good).is_err(), "a leaf is not the commitments");
        // Out of scope.
        assert!(artifact_unit_in_scope_v1(&facts, &PublicUnitV1::ArtifactLeaf { index: leaves.len() as u32 }).is_err());
        assert!(artifact_unit_in_scope_v1(&facts, &PublicUnitV1::KernelRow { param, layer, row: rows }).is_err());
        assert!(artifact_unit_in_scope_v1(&facts, &PublicUnitV1::KernelRow { param: 999, layer: None, row: 0 }).is_err());
        assert!(
            artifact_unit_in_scope_v1(&facts, &PublicUnitV1::KernelRowNodes { param, layer, level: 0, first: 0, count: 0 }).is_err()
        );
        assert!(artifact_unit_in_scope_v1(&facts, &PublicUnitV1::ClaimPosition { stage: 0, position: 0 }).is_err());
    }

    /// The manifest is an index checked against the chain before any byte: another root, count, class or network is refused; every leaf
    /// is then checked alone, at its canonical coordinates.
    #[test]
    fn a_manifest_is_checked_against_the_chain_and_every_leaf_alone() {
        let (program, params, leaves, root) = fixture(3);
        let kr = Hash64::from_bytes(ParamCommitmentsV1::of(&params).root());
        let net = Hash64::from_bytes([1; 64]);
        let m = ArtifactManifestV1::of(net, class_of(&program), root, kr, &leaves, 500);
        let class = m.v2_class();
        assert_eq!(m.check_against_chain(net, class, root, Some(kr)).unwrap(), program);
        m.check_against_chain(net, class, root, None).unwrap();
        assert!(m.check_against_chain(Hash64::from_bytes([3; 64]), class, root, Some(kr)).is_err());
        assert!(m.check_against_chain(net, Hash64::from_bytes([2; 64]), root, Some(kr)).is_err(), "another class id");
        assert!(m.check_against_chain(net, class, Hash64::from_bytes([4; 64]), Some(kr)).is_err());
        assert!(m.check_against_chain(net, class, root, Some(Hash64::from_bytes([6; 64]))).is_err());
        let mut short = m.clone();
        short.leaf_hashes.pop();
        assert!(short.check_against_chain(net, class, root, Some(kr)).is_err());
        let mut forged = m.clone();
        forged.leaf_hashes[0] = Hash64::from_bytes([5; 64]);
        assert!(forged.check_against_chain(net, class, root, Some(kr)).is_err());
        let rows = canonical_leaf_rows_v1(&program).unwrap();
        for (i, (row, leaf)) in rows.iter().zip(&leaves).enumerate() {
            m.verify_leaf(&program, row, i as u32, leaf).unwrap();
        }
        let mut moved = leaves[0].clone();
        moved.row_start += 1;
        assert!(m.verify_leaf(&program, &rows[0], 0, &moved).is_err());
        assert_ne!(m.id(), forged.id());
    }
}
