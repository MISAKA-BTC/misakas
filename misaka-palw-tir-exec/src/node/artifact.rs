//! **A PALWTIR1 artifact, mapped** (RFC-0002 Phase F, design §2.10): the container opened and
//! checked (`misaka_palw_tir_artifact::PalwTirContainerV1::open`), the file mapped read-only, every
//! param instance bound IN PLACE (the container aligns tensors at 64 bytes), and the plan compiled
//! once. The inventory root — what a registration pins as `artifact_root` — is streamed from the
//! mapping through the consensus inventory (`palw_tir_inventory_root_v1`), never read from a
//! sidecar here.
//!
//! **A composite candidate** (RFC-0004 §6.3, §6.7; [`TirArtifactV1::open_composite`]) is the same
//! thing over two files: its params `0..p` bound in place from its PARENT's container — the file the
//! node already holds, mapped again (a read-only mapping of one file shares its pages) — and its
//! params `p..` from the adapter section (`PALWTIRS`) it fetched. Its artifact root is the composite
//! root; its court openings are made under the two sub-roots (`palw_tir_composite_carriage_v1`).

use std::borrow::Cow;
use std::path::Path;
use std::sync::OnceLock;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::{PalwArtifactMultiproofV1, PalwArtifactOpeningV1};
use kaspa_consensus_core::palw_improve_composite_v1::{
    PalwTirCompositeRefV1, palw_tir_composite_carriage_v1, palw_tir_composite_rule_v1,
};
use kaspa_consensus_core::palw_tir_artifact_v1::{
    PalwTirTensorSourceV1, palw_tir_inventory_root_v1, palw_tir_inventory_section_root_v1,
};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1};
use kaspa_consensus_core::palw_tir_court_v1::{PalwTirInventoryIndexV1, PalwTirParamOpeningV1};
use misaka_palw_tir_artifact::{PalwTirContainerHeaderV1, PalwTirContainerV1, PalwTirSectionV1};

use super::inventory::{TirInventoryTreeV1, TirParamOpenerV1};
use super::mapped::MappedFile;
use crate::params::{ParamData, TirParams};
use crate::plan::TirPlan;

/// An opened, mapped IR artifact. Field order is drop order: the params (which borrow the mappings)
/// go before the mappings.
pub struct TirArtifactV1 {
    params: TirParams<'static>,
    plan: TirPlan,
    /// The file's container — for a composite, the PARENT's, which serves params `0..p`.
    container: PalwTirContainerV1,
    /// The inventory tree, built on the first opening (a court close; never at load).
    tree: OnceLock<Result<TirInventoryTreeV1, String>>,
    /// A composite candidate's adapter section, when this is one.
    composite: Option<Box<TirCompositePartsV1>>,
    map: MappedFile,
}

/// **A composite candidate's own half**: its reference (the chain's `PalwTirCompositeRefV1`), the
/// adapter section serving params `p..`, and where the section's leaves start in the candidate's
/// inventory (the parent's leaf count).
struct TirCompositePartsV1 {
    r: PalwTirCompositeRefV1,
    section: PalwTirSectionV1,
    split: u32,
    map: MappedFile,
}

/// Bind every param instance of `plan` in place, each from the mapping `locate` names for it.
fn bind<'m>(
    plan: &TirPlan,
    what: &str,
    locate: &dyn Fn(u16, Option<u16>) -> Option<(&'m MappedFile, u64, u64)>,
) -> Result<TirParams<'static>, String> {
    let mut params = TirParams::new(plan);
    for &(j, layer) in &plan.param_instances {
        let (map, off, len) = locate(j, layer).ok_or_else(|| format!("{what}: no tensor for param {j} at {layer:?}"))?;
        let bytes = map.bytes().get(off as usize..(off + len) as usize).ok_or_else(|| format!("{what}: a tensor past the end"))?;
        // SAFETY: `bytes` points into a mapping the artifact owns and drops after `params` (field
        // order), and no reference with the `'static` lifetime escapes: every accessor hands the
        // params out at the lifetime of `&self`.
        let bytes: &'static [u8] = unsafe { std::mem::transmute::<&[u8], &'static [u8]>(bytes) };
        let d = &plan.program.params[j as usize];
        let data = ParamData::from_le_bytes(d.dtype, bytes).map_err(|e| format!("{what}: {e}"))?;
        params.insert(plan, j, layer, data).map_err(|e| format!("{what}: {e}"))?;
    }
    Ok(params)
}

impl TirArtifactV1 {
    /// Open, check and map `path`; bind every param instance in place.
    pub fn open(path: &Path) -> Result<Self, String> {
        let container = PalwTirContainerV1::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let map = MappedFile::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let plan = TirPlan::compile(&container.program).map_err(|e| format!("{}: {e}", path.display()))?;
        let what = path.display().to_string();
        let params = bind(&plan, &what, &|j, layer| container.locate(j, layer).map(|(off, len)| (&map, off, len)))?;
        Ok(TirArtifactV1 { params, plan, container, tree: OnceLock::new(), composite: None, map })
    }

    /// **Open a composite candidate** (RFC-0004 §6.3, §6.7): `parent` is the parent's PALWTIR1 file
    /// — the one this node holds — and `section` the candidate's adapter section (`PALWTIRS`) of
    /// params `r.p..`. Checked against the chain's reference `r`: the composite rule over the two
    /// programs (the candidate's params `0..p` are the parent's, unchanged), the parent's inventory
    /// root (`parent_root`: the root this node computed when it loaded the parent from that file, or
    /// `None` to stream it again), and the section's leaves rooting to `r.adapter_root`. The class
    /// this returns declares its id over `r.artifact_root()`, which the caller holds to the chain's.
    pub fn open_composite(
        parent: &Path,
        section: &Path,
        r: &PalwTirCompositeRefV1,
        parent_root: Option<Hash64>,
    ) -> Result<Self, String> {
        let container = PalwTirContainerV1::open(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        let map = MappedFile::open(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        let opened = PalwTirSectionV1::open(section, r.p).map_err(|e| format!("{}: {e}", section.display()))?;
        let section_map = MappedFile::open(section).map_err(|e| format!("{}: {e}", section.display()))?;
        palw_tir_composite_rule_v1(&container.program, &opened.program, r.p)
            .map_err(|e| format!("{} over {}: {e}", section.display(), parent.display()))?;
        let parent_root = match parent_root {
            Some(root) => root,
            None => {
                palw_tir_inventory_root_v1(&container.program, &ContainerSource { container: &container, map: &map })
                    .map_err(|e| format!("{}: {e}", parent.display()))?
                    .0
            }
        };
        if parent_root != r.parent_root {
            return Err(format!("{}: the parent roots to {parent_root}, the candidate names {}", parent.display(), r.parent_root));
        }
        let plan = TirPlan::compile(&opened.program).map_err(|e| format!("{}: {e}", section.display()))?;
        let what = format!("{} over {}", section.display(), parent.display());
        let params = bind(&plan, &what, &|j, layer| {
            if u32::from(j) < r.p {
                container.locate(j, layer).map(|(off, len)| (&map, off, len))
            } else {
                opened.locate(j, layer).map(|(off, len)| (&section_map, off, len))
            }
        })?;
        let n = u16::try_from(opened.program.params.len()).map_err(|_| format!("{what}: too many params"))?;
        let p = u16::try_from(r.p).map_err(|_| format!("{what}: p past u16"))?;
        let adapter =
            palw_tir_inventory_section_root_v1(&opened.program, p..n, &SectionSource { section: &opened, map: &section_map })
                .map_err(|e| format!("{what}: {e}"))?
                .0;
        if adapter != r.adapter_root {
            return Err(format!(
                "{}: the adapter section roots to {adapter}, the candidate names {}",
                section.display(),
                r.adapter_root
            ));
        }
        let split = PalwTirInventoryIndexV1::new(&opened.program)
            .and_then(|index| index.leaves_before(p))
            .ok_or_else(|| format!("{what}: the TIR inventory refuses the candidate's program"))?;
        let composite = Some(Box::new(TirCompositePartsV1 { r: *r, section: opened, split, map: section_map }));
        Ok(TirArtifactV1 { params, plan, container, tree: OnceLock::new(), composite, map })
    }

    pub fn plan(&self) -> &TirPlan {
        &self.plan
    }

    pub fn params(&self) -> &TirParams<'_> {
        &self.params
    }

    /// The file's container — for a composite candidate, its PARENT's (which serves params `0..p`;
    /// the candidate's own header is [`Self::header`]).
    pub fn container(&self) -> &PalwTirContainerV1 {
        &self.container
    }

    /// The header this artifact's class is declared by: the container's, or a composite's section's.
    pub fn header(&self) -> &PalwTirContainerHeaderV1 {
        match &self.composite {
            Some(c) => &c.section.header,
            None => &self.container.header,
        }
    }

    /// A composite candidate's reference (RFC-0004 §6.3), when this is one.
    pub fn composite_ref(&self) -> Option<&PalwTirCompositeRefV1> {
        self.composite.as_ref().map(|c| &c.r)
    }

    /// The declared layout, if the header carries one.
    pub fn layout(&self) -> Result<Option<PalwTirLayoutV1>, String> {
        let b = &self.header().layout;
        if b.is_empty() {
            return Ok(None);
        }
        borsh::from_slice(b).map(Some).map_err(|e| format!("the container's layout does not decode: {e}"))
    }

    /// The IR class this artifact declares (program, layout, tokenizer).
    pub fn class(&self) -> Result<PalwTirClassV1, String> {
        let layout = self.layout()?.ok_or("the container declares no layout")?;
        let header = self.header();
        Ok(PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: header.program.clone(),
            layout,
            tokenizer_id: Hash64::from_bytes(header.tokenizer_id),
        })
    }

    /// **The artifact root and inventory leaf count** — streamed from the mapping, one instance at a
    /// time, through the consensus inventory; a composite's root is the composite root over its two
    /// sections (checked at open), its count every leaf of both.
    pub fn inventory_root(&self) -> Result<(Hash64, u32), String> {
        match &self.composite {
            Some(c) => Ok((c.r.artifact_root(), self.inventory_tree()?.leaf_count())),
            None => palw_tir_inventory_root_v1(&self.plan.program, self).map_err(|e| e.to_string()),
        }
    }

    /// The inventory tree (built once, on first use) — a composite's over every leaf of both
    /// sections, whose own root is no commitment (the composite root is).
    pub fn inventory_tree(&self) -> Result<&TirInventoryTreeV1, String> {
        self.tree.get_or_init(|| TirInventoryTreeV1::build(&self.plan.program, self)).as_ref().map_err(|e| e.clone())
    }

    /// **The tree a possession proof of this artifact opens** (RFC-0004 §6.3/§6.7, spec 17 §17.7.1): `None`
    /// for a single artifact, whose class's registered root is its inventory's; for a composite candidate its
    /// ADAPTER section — where the section starts in the candidate's inventory, the section's own root (the
    /// chain's `adapter_root`, which `open_composite` checked the section's leaves to) and the section's leaf
    /// hashes in inventory order. The composite's registered root commits two trees and opens neither: a seat
    /// proves it holds the section, and holds the parent through the parent class's own proof.
    pub fn possession_section(&self) -> Result<Option<(u32, Hash64, &[Hash64])>, String> {
        let Some(c) = &self.composite else { return Ok(None) };
        let section = self
            .inventory_tree()?
            .leaves()
            .get(c.split as usize..)
            .filter(|leaves| !leaves.is_empty())
            .ok_or("the composite's adapter section holds no leaf")?;
        Ok(Some((c.split, c.r.adapter_root, section)))
    }
}

/// A container and its mapping as an inventory source (the parent's root, streamed).
struct ContainerSource<'a> {
    container: &'a PalwTirContainerV1,
    map: &'a MappedFile,
}

impl PalwTirTensorSourceV1 for ContainerSource<'_> {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        let (off, len) = self.container.locate(param, layer)?;
        self.map.bytes().get(off as usize..(off + len) as usize).map(Cow::Borrowed)
    }
}

/// A section and its mapping as an inventory source (params `p..` only).
struct SectionSource<'a> {
    section: &'a PalwTirSectionV1,
    map: &'a MappedFile,
}

impl PalwTirTensorSourceV1 for SectionSource<'_> {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        let (off, len) = self.section.locate(param, layer)?;
        self.map.bytes().get(off as usize..(off + len) as usize).map(Cow::Borrowed)
    }
}

/// The mapping serves every instance's bytes in place — a composite's params `0..p` from the
/// parent's container, the rest from its section.
impl PalwTirTensorSourceV1 for TirArtifactV1 {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        let (map, (off, len)) = match &self.composite {
            Some(c) if u32::from(param) >= c.r.p => (&c.map, c.section.locate(param, layer)?),
            _ => (&self.map, self.container.locate(param, layer)?),
        };
        map.bytes().get(off as usize..(off + len) as usize).map(Cow::Borrowed)
    }
}

/// A single artifact opens against its one root. A composite's leaf opening is against the tree over
/// its every leaf — what an evaluation reads the operand from, no commitment — and what a close
/// carries is made only under its sub-roots ([`TirParamOpenerV1::param_carriage`]): no multiproof
/// against one root is made for it.
impl TirParamOpenerV1 for TirArtifactV1 {
    fn param_opening(&self, leaf: u32) -> Option<PalwArtifactOpeningV1> {
        self.inventory_tree().ok()?.open(&self.plan.program, self, leaf)
    }

    fn param_multiproof(&self, leaves: &[u32]) -> Option<PalwArtifactMultiproofV1> {
        if self.composite.is_some() {
            return None;
        }
        self.inventory_tree().ok()?.multiproof(&self.plan.program, self, leaves)
    }

    /// A composite's leaves under its sub-roots (`palw_tir_composite_carriage_v1`): those below the
    /// split in one multiproof over the parent's leaves, the rest rebased in one over the section's.
    fn param_carriage(&self, leaves: &[u32]) -> Option<PalwTirParamOpeningV1> {
        let Some(c) = &self.composite else {
            return self.param_multiproof(leaves).map(PalwTirParamOpeningV1::Single);
        };
        let tree = self.inventory_tree().ok()?;
        let opened = tree.operands(&self.plan.program, self, leaves)?;
        let (parent, adapter) = tree.leaves().split_at(usize::try_from(c.split).ok()?.min(tree.leaves().len()));
        palw_tir_composite_carriage_v1(&c.r, c.split, parent, adapter, &opened)
    }
}
