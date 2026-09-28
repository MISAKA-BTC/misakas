//! **A PALWTIR1 artifact, mapped** (RFC-0002 Phase F, design §2.10): the container opened and
//! checked (`misaka_palw_tir_artifact::PalwTirContainerV1::open`), the file mapped read-only, every
//! param instance bound IN PLACE (the container aligns tensors at 64 bytes), and the plan compiled
//! once. The inventory root — what a registration pins as `artifact_root` — is streamed from the
//! mapping through the consensus inventory (`palw_tir_inventory_root_v1`), never read from a
//! sidecar here.

use std::borrow::Cow;
use std::path::Path;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_root_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1};
use misaka_palw_tir_artifact::PalwTirContainerV1;

use super::mapped::MappedFile;
use crate::params::{ParamData, TirParams};
use crate::plan::TirPlan;

/// An opened, mapped IR artifact. Field order is drop order: the params (which borrow the mapping)
/// go before the mapping.
pub struct TirArtifactV1 {
    params: TirParams<'static>,
    plan: TirPlan,
    container: PalwTirContainerV1,
    map: MappedFile,
}

impl TirArtifactV1 {
    /// Open, check and map `path`; bind every param instance in place.
    pub fn open(path: &Path) -> Result<Self, String> {
        let container = PalwTirContainerV1::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let map = MappedFile::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let plan = TirPlan::compile(&container.program).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut params = TirParams::new(&plan);
        for &(j, layer) in &plan.param_instances {
            let (off, len) =
                container.locate(j, layer).ok_or_else(|| format!("{}: no tensor for param {j} at {layer:?}", path.display()))?;
            let bytes = map
                .bytes()
                .get(off as usize..(off + len) as usize)
                .ok_or_else(|| format!("{}: a tensor past the end", path.display()))?;
            // SAFETY: `bytes` points into `map`, which this struct owns and drops after `params`
            // (field order), and no reference with the `'static` lifetime escapes: every accessor
            // hands the params out at the lifetime of `&self`.
            let bytes: &'static [u8] = unsafe { std::mem::transmute::<&[u8], &'static [u8]>(bytes) };
            let d = &plan.program.params[j as usize];
            let data = ParamData::from_le_bytes(d.dtype, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            params.insert(&plan, j, layer, data).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok(TirArtifactV1 { params, plan, container, map })
    }

    pub fn plan(&self) -> &TirPlan {
        &self.plan
    }

    pub fn params(&self) -> &TirParams<'_> {
        &self.params
    }

    pub fn container(&self) -> &PalwTirContainerV1 {
        &self.container
    }

    /// The declared layout, if the container carries one.
    pub fn layout(&self) -> Result<Option<PalwTirLayoutV1>, String> {
        let b = &self.container.header.layout;
        if b.is_empty() {
            return Ok(None);
        }
        borsh::from_slice(b).map(Some).map_err(|e| format!("the container's layout does not decode: {e}"))
    }

    /// The IR class this artifact declares (program, layout, tokenizer).
    pub fn class(&self) -> Result<PalwTirClassV1, String> {
        let layout = self.layout()?.ok_or("the container declares no layout")?;
        Ok(PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: self.container.header.program.clone(),
            layout,
            tokenizer_id: Hash64::from_bytes(self.container.header.tokenizer_id),
        })
    }

    /// **The inventory root and leaf count** — streamed from the mapping, one instance at a time,
    /// through the consensus inventory.
    pub fn inventory_root(&self) -> Result<(Hash64, u32), String> {
        struct Src<'a>(&'a TirArtifactV1);
        impl PalwTirTensorSourceV1 for Src<'_> {
            fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
                let (off, len) = self.0.container.locate(param, layer)?;
                self.0.map.bytes().get(off as usize..(off + len) as usize).map(Cow::Borrowed)
            }
        }
        palw_tir_inventory_root_v1(&self.plan.program, &Src(self)).map_err(|e| e.to_string())
    }
}
