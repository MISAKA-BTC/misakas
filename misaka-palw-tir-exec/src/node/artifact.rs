//! **A PALWTIR1 artifact, mapped — or held within a budget** (RFC-0002 Phase F, design §2.10;
//! ADR-0112 for IR classes): the container opened and checked
//! (`misaka_palw_tir_artifact::PalwTirContainerV1::open`), the plan compiled once, and every param
//! instance bound — IN PLACE from a read-only mapping (the container aligns tensors at 64 bytes), or,
//! under a runtime residency ([`super::residency`]), from memory the residency owns, with the
//! instances it routes or gathers served by rows and nothing mapped at all. The inventory root —
//! what a registration pins as `artifact_root` — is streamed through the consensus inventory
//! (`palw_tir_inventory_root_v1` over the mapping, or the residency's one pass through the file
//! descriptor), never read from a sidecar here.
//!
//! **A composite candidate** (RFC-0004 §6.3, §6.7; [`TirArtifactV1::open_composite`]) is the same
//! thing over two files: its params `0..p` bound from its PARENT's container — the file the node
//! already holds, mapped again (a read-only mapping of one file shares its pages), or served by the
//! parent's residency, ONE store per parent root shared by every candidate of it — and its params
//! `p..` from the adapter section (`PALWTIRS`) it fetched, pinned. Its artifact root is the
//! composite root; its court openings are made under the two sub-roots
//! (`palw_tir_composite_carriage_v1`).

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, OnceLock};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::{PalwArtifactMultiproofV1, PalwArtifactOpeningV1};
use kaspa_consensus_core::palw_improve_composite_v1::{
    PalwTirCompositeRefV1, palw_tir_composite_carriage_v1, palw_tir_composite_rule_v1,
};
use kaspa_consensus_core::palw_tir_artifact_v1::{
    PalwTirTensorSourceV1, palw_tir_inventory_leaf_count_v1, palw_tir_inventory_root_v1, palw_tir_inventory_section_root_v1,
};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1};
use kaspa_consensus_core::palw_tir_court_v1::{PalwTirInventoryIndexV1, PalwTirParamOpeningV1};
use misaka_palw_tir_artifact::{PalwTirContainerHeaderV1, PalwTirContainerV1, PalwTirSectionV1};

use super::inventory::{TirByteSourceV1, TirInventoryTreeV1, TirParamOpenerV1};
use super::mapped::MappedFile;
use super::residency::{
    TirHeldBytesV1, TirResidencyDeclinedV1, TirResidencyPolicyV1, TirResidencyStatsV1, TirStoreOpenV1, TirWeightFileV1,
    TirWeightStoreV1, tir_stream_leaves_v1, tir_weight_store_for_root_v1,
};
use crate::params::{ParamData, TirParams};
use crate::plan::TirPlan;
use crate::tiers::{TirTierRulesV1, TirTiersV1};

/// Where an artifact's params `0..p` (all of them, for a single artifact) are.
enum TirWeightsV1 {
    /// Mapped in place; the page cache decides what is in memory.
    Mapped(MappedFile),
    /// Held within a budget, read through the file descriptor (ADR-0112).
    Resident(Arc<TirWeightStoreV1>),
}

/// An opened IR artifact. Field order is drop order: the params (which borrow the mappings, the
/// residency's pinned set, the adapters and the private pins) go before all of those.
pub struct TirArtifactV1 {
    params: TirParams<'static>,
    plan: TirPlan,
    /// The file's container — for a composite, the PARENT's, which serves params `0..p`.
    container: PalwTirContainerV1,
    /// The inventory tree, built on the first opening (a court close; never at load).
    tree: OnceLock<Result<TirInventoryTreeV1, String>>,
    /// The inventory root and leaf count, derived once.
    root: OnceLock<Result<(Hash64, u32), String>>,
    /// A composite candidate's adapter section, when this is one.
    composite: Option<Box<TirCompositePartsV1>>,
    /// Params `0..p` a composite candidate reads whole that its parent's residency serves by rows:
    /// pinned for this candidate alone (never for a single artifact).
    private: BTreeMap<(u16, Option<u16>), TirHeldBytesV1>,
    /// Why a default residency budget was not taken, when it was not (ADR-0112 Decision 2).
    declined: Option<TirResidencyDeclinedV1>,
    weights: TirWeightsV1,
}

/// **A composite candidate's own half**: its reference (the chain's `PalwTirCompositeRefV1`), the
/// adapter section serving params `p..`, and where the section's leaves start in the candidate's
/// inventory (the parent's leaf count).
struct TirCompositePartsV1 {
    r: PalwTirCompositeRefV1,
    section: PalwTirSectionV1,
    split: u32,
    adapter: TirAdapterV1,
}

/// A section's tensors: mapped in place, or held (under a residency an adapter is pinned).
enum TirAdapterV1 {
    Mapped(MappedFile),
    Held(BTreeMap<(u16, Option<u16>), TirHeldBytesV1>),
}

impl TirAdapterV1 {
    fn bytes<'a>(&'a self, section: &PalwTirSectionV1, param: u16, layer: Option<u16>) -> Option<&'a [u8]> {
        match self {
            Self::Mapped(map) => {
                let (off, len) = section.locate(param, layer)?;
                map.bytes().get(off as usize..(off + len) as usize)
            }
            Self::Held(held) => held.get(&(param, layer)).map(|h| h.as_bytes()),
        }
    }
}

/// Extend a borrow of bytes the artifact owns to `'static`.
///
/// SAFETY (the caller's): the bytes live in a mapping, a residency's pinned set (behind an `Arc` the
/// artifact holds), an adapter or a private pin — every one dropped after `params` (field order) and
/// never mutated while the artifact lives — and no reference with the `'static` lifetime escapes:
/// every accessor hands the params out at the lifetime of `&self`.
unsafe fn held_forever(bytes: &[u8]) -> &'static [u8] {
    unsafe { std::mem::transmute::<&[u8], &'static [u8]>(bytes) }
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
        // SAFETY: see `held_forever` — the mapping is the artifact's and outlives `params`.
        let bytes = unsafe { held_forever(bytes) };
        let d = &plan.program.params[j as usize];
        let data = ParamData::from_le_bytes(d.dtype, bytes).map_err(|e| format!("{what}: {e}"))?;
        params.insert(plan, j, layer, data).map_err(|e| format!("{what}: {e}"))?;
    }
    Ok(params)
}

/// **Bind under a residency**: an instance `held` names is bound from those bytes (the pinned set,
/// an adapter, a private pin); every other one must be one `store` routes or gathers, and is served
/// by rows.
fn bind_resident<'h>(
    plan: &TirPlan,
    what: &str,
    store: &Arc<TirWeightStoreV1>,
    held: &dyn Fn(u16, Option<u16>) -> Option<&'h TirHeldBytesV1>,
) -> Result<TirParams<'static>, String> {
    let mut params = TirParams::new(plan);
    let mut served = BTreeSet::new();
    for &(j, layer) in &plan.param_instances {
        let d = &plan.program.params[j as usize];
        if let Some(h) = held(j, layer) {
            // SAFETY: see `held_forever` — every holder outlives `params`.
            let bytes = unsafe { held_forever(h.as_bytes()) };
            let data = ParamData::from_le_bytes(d.dtype, bytes).map_err(|e| format!("{what}: {e}"))?;
            params.insert(plan, j, layer, data).map_err(|e| format!("{what}: {e}"))?;
        } else if store.serves(j, layer) {
            served.insert((j, layer));
        } else {
            return Err(format!("{what}: no tensor for param {j} at {layer:?}"));
        }
    }
    let source: Arc<dyn crate::rows::TirRowSourceV1> = store.clone();
    params.serve_rows(plan, source, &|j, layer| served.contains(&(j, layer))).map_err(|e| format!("{what}: {e}"))?;
    Ok(params)
}

impl TirArtifactV1 {
    /// Open, check and map `path`; bind every param instance in place (the page cache decides).
    pub fn open(path: &Path) -> Result<Self, String> {
        Self::open_with_residency(path, TirResidencyPolicyV1::PageCache)
    }

    /// **Open `path` under a residency policy** (ADR-0112; the node's default tier rules): a budget
    /// at or above the class's floor holds the class within it, read through the file descriptor
    /// and never mapped; the page cache, or a default under the floor, maps it as [`Self::open`]
    /// does; a stated budget under the floor is refused by name.
    pub fn open_with_residency(path: &Path, policy: TirResidencyPolicyV1) -> Result<Self, String> {
        Self::open_with_rules(path, policy, TirTierRulesV1::default())
    }

    /// [`Self::open_with_residency`] under explicit tier rules (tests hold the identity with every
    /// row-addressed param served).
    pub fn open_with_rules(path: &Path, policy: TirResidencyPolicyV1, rules: TirTierRulesV1) -> Result<Self, String> {
        let container = PalwTirContainerV1::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let plan = TirPlan::compile(&container.program).map_err(|e| format!("{}: {e}", path.display()))?;
        let what = path.display().to_string();
        let declined = match TirWeightStoreV1::open(&container, policy, rules).map_err(|e| format!("{what}: {e}"))? {
            TirStoreOpenV1::Resident(store) => {
                let params = bind_resident(&plan, &what, &store, &|j, layer| store.pinned(j, layer))?;
                return Ok(TirArtifactV1 {
                    params,
                    plan,
                    container,
                    tree: OnceLock::new(),
                    root: OnceLock::new(),
                    composite: None,
                    private: BTreeMap::new(),
                    declined: None,
                    weights: TirWeightsV1::Resident(store),
                });
            }
            TirStoreOpenV1::Declined(d) => Some(d),
            TirStoreOpenV1::PageCache => None,
        };
        let map = MappedFile::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let params = bind(&plan, &what, &|j, layer| container.locate(j, layer).map(|(off, len)| (&map, off, len)))?;
        Ok(TirArtifactV1 {
            params,
            plan,
            container,
            tree: OnceLock::new(),
            root: OnceLock::new(),
            composite: None,
            private: BTreeMap::new(),
            declined,
            weights: TirWeightsV1::Mapped(map),
        })
    }

    /// **Open a composite candidate** (RFC-0004 §6.3, §6.7): `parent` is the parent's PALWTIR1 file
    /// — the one this node holds — and `section` the candidate's adapter section (`PALWTIRS`) of
    /// params `r.p..`. Checked against the chain's reference `r`: the composite rule over the two
    /// programs (the candidate's params `0..p` are the parent's, unchanged), the parent's inventory
    /// root (`parent_root`: the root this node computed when it loaded the parent from that file, or
    /// `None` to stream it again), and the section's leaves rooting to `r.adapter_root`. The class
    /// this returns declares its id over `r.artifact_root()`, which the caller holds to the chain's.
    ///
    /// When this process holds a residency of the parent (any artifact opened from it under a
    /// budget, [`tir_weight_store_for_root_v1`]), the candidate is served by that store instead —
    /// one store per parent root, shared by every candidate of it.
    pub fn open_composite(
        parent: &Path,
        section: &Path,
        r: &PalwTirCompositeRefV1,
        parent_root: Option<Hash64>,
    ) -> Result<Self, String> {
        if let Some(store) = tir_weight_store_for_root_v1(&r.parent_root) {
            let container = PalwTirContainerV1::open(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            return Self::composite_resident(store, container, section, r);
        }
        Self::composite_mapped(parent, section, r, parent_root)
    }

    /// [`Self::open_composite`] under a residency policy: the parent's store when one is live, else
    /// the parent opened under `policy` (one pass) and its store shared from then on; mapped when the
    /// policy is the page cache or a default the parent's floor does not fit.
    pub fn open_composite_with_residency(
        parent: &Path,
        section: &Path,
        r: &PalwTirCompositeRefV1,
        parent_root: Option<Hash64>,
        policy: TirResidencyPolicyV1,
    ) -> Result<Self, String> {
        if let Some(store) = tir_weight_store_for_root_v1(&r.parent_root) {
            let container = PalwTirContainerV1::open(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            return Self::composite_resident(store, container, section, r);
        }
        if policy == TirResidencyPolicyV1::PageCache {
            return Self::composite_mapped(parent, section, r, parent_root);
        }
        let container = PalwTirContainerV1::open(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        match TirWeightStoreV1::open(&container, policy, TirTierRulesV1::default())
            .map_err(|e| format!("{}: {e}", parent.display()))?
        {
            TirStoreOpenV1::Resident(store) => Self::composite_resident(store, container, section, r),
            TirStoreOpenV1::Declined(d) => {
                let mut a = Self::composite_mapped(parent, section, r, parent_root)?;
                a.declined = Some(d);
                Ok(a)
            }
            TirStoreOpenV1::PageCache => Self::composite_mapped(parent, section, r, parent_root),
        }
    }

    /// **A composite candidate over a parent artifact this node holds** — served by the parent's
    /// residency when it has one (shared, not reopened), mapped beside it otherwise.
    pub fn open_composite_over(parent: &TirArtifactV1, section: &Path, r: &PalwTirCompositeRefV1) -> Result<Self, String> {
        if parent.composite.is_some() {
            return Err(format!("{}: a composite candidate is no parent", section.display()));
        }
        match &parent.weights {
            TirWeightsV1::Resident(store) => {
                let container = PalwTirContainerV1::open(&parent.container.path)
                    .map_err(|e| format!("{}: {e}", parent.container.path.display()))?;
                Self::composite_resident(store.clone(), container, section, r)
            }
            TirWeightsV1::Mapped(_) => {
                let (root, _) = parent.inventory_root()?;
                Self::composite_mapped(&parent.container.path, section, r, Some(root))
            }
        }
    }

    fn composite_mapped(
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
        let adapter = TirAdapterV1::Mapped(section_map);
        let split = Self::check_adapter(&opened, &adapter, r, &what, section)?;
        let composite = Some(Box::new(TirCompositePartsV1 { r: *r, section: opened, split, adapter }));
        Ok(TirArtifactV1 {
            params,
            plan,
            container,
            tree: OnceLock::new(),
            root: OnceLock::new(),
            composite,
            private: BTreeMap::new(),
            declined: None,
            weights: TirWeightsV1::Mapped(map),
        })
    }

    /// **A composite candidate served by its parent's store**: the adapter pinned (read whole through
    /// the descriptor), and any parent param the candidate's own program reads whole — or reads by
    /// rows of another shape — that the store serves by rows pinned for this candidate alone, so no
    /// read of the candidate's ever falls back to a whole one.
    fn composite_resident(
        store: Arc<TirWeightStoreV1>,
        container: PalwTirContainerV1,
        section: &Path,
        r: &PalwTirCompositeRefV1,
    ) -> Result<Self, String> {
        // One store, one rule: the candidate's tiers are read under the rules its parent's store was
        // opened with, so a param both programs read by the same rows is served by the same rows.
        let rules = store.tiers().rules;
        let parent = container.path.clone();
        let opened = PalwTirSectionV1::open(section, r.p).map_err(|e| format!("{}: {e}", section.display()))?;
        palw_tir_composite_rule_v1(&container.program, &opened.program, r.p)
            .map_err(|e| format!("{} over {}: {e}", section.display(), parent.display()))?;
        if store.root().0 != r.parent_root {
            return Err(format!(
                "{}: the parent roots to {}, the candidate names {}",
                parent.display(),
                store.root().0,
                r.parent_root
            ));
        }
        let plan = TirPlan::compile(&opened.program).map_err(|e| format!("{}: {e}", section.display()))?;
        let what = format!("{} over {}", section.display(), parent.display());
        // The adapter, read whole and pinned: the candidate's own params, read every forward.
        let file = TirWeightFileV1::open(section).map_err(|e| format!("{}: {e}", section.display()))?;
        let mut adapters = BTreeMap::new();
        for &(j, layer) in plan.param_instances.iter().filter(|(j, _)| u32::from(*j) >= r.p) {
            let (off, len) = opened.locate(j, layer).ok_or_else(|| format!("{what}: no tensor for param {j} at {layer:?}"))?;
            let mut held = TirHeldBytesV1::zeroed(len as usize);
            file.read_at_par(off, held.as_mut_bytes())?;
            adapters.insert((j, layer), held);
        }
        let adapter = TirAdapterV1::Held(adapters);
        let split = Self::check_adapter(&opened, &adapter, r, &what, section)?;
        // The candidate's tiers over its own program: a parent param it does not read the way the
        // store serves it is pinned here, for this candidate.
        let mine = TirTiersV1::of(&opened.program, rules);
        let mut private = BTreeMap::new();
        for &(j, layer) in plan.param_instances.iter().filter(|(j, _)| u32::from(*j) < r.p) {
            if !store.serves(j, layer) {
                continue;
            }
            let t = &mine.params[j as usize];
            let same = t.tier.is_rows() && store.tiers().params.get(j as usize).is_some_and(|s| (s.rows, s.unit) == (t.rows, t.unit));
            if !same {
                private.insert((j, layer), store.read_instance(j, layer)?);
            }
        }
        let TirAdapterV1::Held(adapters) = &adapter else { unreachable!("held above") };
        let params = bind_resident(&plan, &what, &store, &|j, layer| {
            if u32::from(j) >= r.p { adapters.get(&(j, layer)) } else { private.get(&(j, layer)).or_else(|| store.pinned(j, layer)) }
        })?;
        let composite = Some(Box::new(TirCompositePartsV1 { r: *r, section: opened, split, adapter }));
        Ok(TirArtifactV1 {
            params,
            plan,
            container,
            tree: OnceLock::new(),
            root: OnceLock::new(),
            composite,
            private,
            declined: None,
            weights: TirWeightsV1::Resident(store),
        })
    }

    /// The section's leaves root to the candidate's adapter root; returns where they start in the
    /// candidate's inventory.
    fn check_adapter(
        opened: &PalwTirSectionV1,
        adapter: &TirAdapterV1,
        r: &PalwTirCompositeRefV1,
        what: &str,
        section: &Path,
    ) -> Result<u32, String> {
        let n = u16::try_from(opened.program.params.len()).map_err(|_| format!("{what}: too many params"))?;
        let p = u16::try_from(r.p).map_err(|_| format!("{what}: p past u16"))?;
        let adapter_root = palw_tir_inventory_section_root_v1(&opened.program, p..n, &SectionSource { section: opened, adapter })
            .map_err(|e| format!("{what}: {e}"))?
            .0;
        if adapter_root != r.adapter_root {
            return Err(format!(
                "{}: the adapter section roots to {adapter_root}, the candidate names {}",
                section.display(),
                r.adapter_root
            ));
        }
        PalwTirInventoryIndexV1::new(&opened.program)
            .and_then(|index| index.leaves_before(p))
            .ok_or_else(|| format!("{what}: the TIR inventory refuses the candidate's program"))
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

    /// **The residency serving this artifact's params `0..p`**, when one does — shared by every
    /// candidate of the same parent root.
    pub fn weight_store(&self) -> Option<&Arc<TirWeightStoreV1>> {
        match &self.weights {
            TirWeightsV1::Resident(store) => Some(store),
            TirWeightsV1::Mapped(_) => None,
        }
    }

    /// Is anything of this artifact mapped? Never under a residency (ADR-0112 I-4: no weight a
    /// kernel reads arrives through a page fault).
    pub fn is_mapped(&self) -> bool {
        matches!(self.weights, TirWeightsV1::Mapped(_))
            || self.composite.as_ref().is_some_and(|c| matches!(c.adapter, TirAdapterV1::Mapped(_)))
    }

    /// The residency's numbers, or `None` when the page cache decides.
    pub fn residency_stats(&self) -> Option<TirResidencyStatsV1> {
        self.weight_store().map(|s| s.stats())
    }

    /// Why a default budget was not taken, when it was not (ADR-0112 Decision 2, amended).
    pub fn residency_declined(&self) -> Option<TirResidencyDeclinedV1> {
        self.declined
    }

    /// Bytes this artifact pins for itself beside a shared residency: a composite's adapter and its
    /// private pins (zero for a single artifact).
    pub fn own_pinned_bytes(&self) -> u64 {
        let adapters = match self.composite.as_ref().map(|c| &c.adapter) {
            Some(TirAdapterV1::Held(held)) => held.values().map(|h| h.len() as u64).sum(),
            _ => 0,
        };
        adapters + self.private.values().map(|h| h.len() as u64).sum::<u64>()
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

    /// **The artifact root and inventory leaf count** — derived once: streamed from the mapping
    /// through the consensus inventory, or the residency's open pass; a composite's root is the
    /// composite root over its two sections (checked at open), its count every leaf of both.
    pub fn inventory_root(&self) -> Result<(Hash64, u32), String> {
        self.root
            .get_or_init(|| match (&self.composite, &self.weights) {
                (Some(c), _) => {
                    palw_tir_inventory_leaf_count_v1(&self.plan.program).map(|n| (c.r.artifact_root(), n)).map_err(|e| e.to_string())
                }
                (None, TirWeightsV1::Resident(store)) => Ok(store.root()),
                (None, TirWeightsV1::Mapped(_)) => palw_tir_inventory_root_v1(&self.plan.program, self).map_err(|e| e.to_string()),
            })
            .clone()
    }

    /// The inventory tree (built once, on first use) — a composite's over every leaf of both
    /// sections, whose own root is no commitment (the composite root is). Under a residency the
    /// leaves are streamed through the file descriptor in chunks, never through a mapping.
    pub fn inventory_tree(&self) -> Result<&TirInventoryTreeV1, String> {
        self.tree
            .get_or_init(|| match &self.weights {
                TirWeightsV1::Mapped(_) => TirInventoryTreeV1::build(&self.plan.program, self),
                TirWeightsV1::Resident(_) => {
                    let instances: Vec<(u16, Option<u16>)> = self.plan.param_instances_in_order();
                    let mut leaves = Vec::new();
                    tir_stream_leaves_v1(
                        &self.plan.program,
                        &instances,
                        &|j, layer, at, buf| self.read_bytes(j, layer, at, buf),
                        &mut |leaf| leaves.push(leaf),
                        &mut |_, _, _| {},
                    )?;
                    TirInventoryTreeV1::from_leaves(&self.plan.program, leaves)
                }
            })
            .as_ref()
            .map_err(|e| e.clone())
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

/// A section's tensors as an inventory source (params `p..` only).
struct SectionSource<'a> {
    section: &'a PalwTirSectionV1,
    adapter: &'a TirAdapterV1,
}

impl PalwTirTensorSourceV1 for SectionSource<'_> {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        self.adapter.bytes(self.section, param, layer).map(Cow::Borrowed)
    }
}

/// Every instance's bytes, whole — in place from a mapping, a pinned set, an adapter or a private
/// pin; read from the file for an instance a residency serves by rows (the whole read the tiers
/// never plan, counted by the store).
impl PalwTirTensorSourceV1 for TirArtifactV1 {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        if let Some(c) = &self.composite
            && u32::from(param) >= c.r.p
        {
            return c.adapter.bytes(&c.section, param, layer).map(Cow::Borrowed);
        }
        if let Some(held) = self.private.get(&(param, layer)) {
            return Some(Cow::Borrowed(held.as_bytes()));
        }
        match &self.weights {
            TirWeightsV1::Mapped(map) => {
                let (off, len) = self.container.locate(param, layer)?;
                map.bytes().get(off as usize..(off + len) as usize).map(Cow::Borrowed)
            }
            TirWeightsV1::Resident(store) => match store.pinned(param, layer) {
                Some(held) => Some(Cow::Borrowed(held.as_bytes())),
                None => store.read_whole_bytes(param, layer).ok().map(Cow::Owned),
            },
        }
    }
}

/// Pieces and chunks of every instance, read exactly: from a mapping, a pinned set, an adapter or a
/// private pin, or through the residency's file descriptor.
impl TirByteSourceV1 for TirArtifactV1 {
    fn read_bytes(&self, param: u16, layer: Option<u16>, at: u64, buf: &mut [u8]) -> Result<(), String> {
        let copy = |bytes: &[u8], buf: &mut [u8]| -> Result<(), String> {
            let piece = usize::try_from(at)
                .ok()
                .and_then(|at| bytes.get(at..at.checked_add(buf.len())?))
                .ok_or_else(|| format!("param {param} at {layer:?}: bytes {at}..+{} leave the instance", buf.len()))?;
            buf.copy_from_slice(piece);
            Ok(())
        };
        if let Some(c) = &self.composite
            && u32::from(param) >= c.r.p
        {
            let bytes =
                c.adapter.bytes(&c.section, param, layer).ok_or_else(|| format!("no tensor for param {param} at {layer:?}"))?;
            return copy(bytes, buf);
        }
        if let Some(held) = self.private.get(&(param, layer)) {
            return copy(held.as_bytes(), buf);
        }
        match &self.weights {
            TirWeightsV1::Mapped(map) => {
                let (off, len) =
                    self.container.locate(param, layer).ok_or_else(|| format!("no tensor for param {param} at {layer:?}"))?;
                let bytes = map.bytes().get(off as usize..(off + len) as usize).ok_or("a tensor past the end of the file")?;
                copy(bytes, buf)
            }
            TirWeightsV1::Resident(store) => store.read_bytes(param, layer, at, buf),
        }
    }
}

/// A single artifact opens against its one root. A composite's leaf opening is against the tree over
/// its every leaf — what an evaluation reads the operand from, no commitment — and what a close
/// carries is made only under its sub-roots ([`TirParamOpenerV1::param_carriage`]): no multiproof
/// against one root is made for it. Every piece is read alone (a residency reads it through the file
/// descriptor; a mapping copies it).
impl TirParamOpenerV1 for TirArtifactV1 {
    fn param_opening(&self, leaf: u32) -> Option<PalwArtifactOpeningV1> {
        self.inventory_tree().ok()?.open_with(&self.plan.program, self, leaf)
    }

    fn param_multiproof(&self, leaves: &[u32]) -> Option<PalwArtifactMultiproofV1> {
        if self.composite.is_some() {
            return None;
        }
        self.inventory_tree().ok()?.multiproof_with(&self.plan.program, self, leaves)
    }

    /// A composite's leaves under its sub-roots (`palw_tir_composite_carriage_v1`): those below the
    /// split in one multiproof over the parent's leaves, the rest rebased in one over the section's.
    fn param_carriage(&self, leaves: &[u32]) -> Option<PalwTirParamOpeningV1> {
        let Some(c) = &self.composite else {
            return self.param_multiproof(leaves).map(PalwTirParamOpeningV1::Single);
        };
        let tree = self.inventory_tree().ok()?;
        let opened = tree.operands_with(&self.plan.program, self, leaves)?;
        let (parent, adapter) = tree.leaves().split_at(usize::try_from(c.split).ok()?.min(tree.leaves().len()));
        palw_tir_composite_carriage_v1(&c.r, c.split, parent, adapter, &opened)
    }
}
