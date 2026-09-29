//! **A LoRA candidate as a composite artifact** (RFC-0004 §6.3, PALW-MIP-15): the tool side of the
//! candidate's container.
//!
//! `palw-tir-fidelity --adapter` writes a candidate's `PALWTIR1` container with the parent's params
//! `0..P` first, their tensors the parent artifact's byte for byte, and the adapter's params `P..`
//! after. Its provenance records `composite.p`. A composite artifact commits the two sections apart
//! (`kaspa_consensus_core::palw_improve_composite_v1`). This module does three things:
//! * it checks the candidate against its parent's container: the family facts a container already
//!   has, the composite rule, and the candidate's parent section rooting to the parent's inventory
//!   root;
//! * it derives the adapter section's root;
//! * it records both sections in the candidate's provenance, in the format the class binding
//!   (rfc4/cand) reads:
//!
//! ```json
//! "composite": { "p": P, "parent_root": "<hex>", "adapter_root": "<hex>",
//!                "parent_leaves": n, "adapter_leaves": m, "parent_class": "<hex>" }
//! ```
//!
//! `parent_class` is present only when it is given, because the parent's class id binds its layout
//! and tokenizer, which the lowering does not know. Hex is lowercase, 128 characters, with no `0x`.
//! The roots are a convenience, and readers recompute them. Every root here is a consensus
//! function's; this module states no rule of its own.

use std::path::Path;

use kaspa_consensus_core::palw_improve_composite_v1::{
    PalwTirCompositeRefV1, palw_improve_composite_root_v1, palw_tir_composite_rule_v1, palw_tir_composite_section_roots_v1,
    palw_tir_family_rule_v1,
};
use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_root_v1;
use kaspa_consensus_core::palw_tir_class_v1::{
    PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1,
};
use kaspa_hashes::Hash64;
use misaka_palw_tir_artifact::{PalwTirContainerV1, write_container_v1, write_section_v1};

use crate::tir_manifest::PalwTirContainerSourceV1;

/// **A candidate's composite**, as derived from its container and its parent's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TirCompositeV1 {
    /// The parent's param count: the candidate's params `0..p` are the parent's.
    pub p: u32,
    /// The parent's inventory root, which the candidate's section `0..p` equals.
    pub parent_root: Hash64,
    pub parent_leaves: u32,
    /// The adapter section's root: the leaves of params `p..` as a tree of their own.
    pub adapter_root: Hash64,
    pub adapter_leaves: u32,
    /// The parent's class id, when given.
    pub parent_class: Option<Hash64>,
}

impl TirCompositeV1 {
    /// The composite artifact root the candidate's class id commits to (RFC-0004 §6.3), which needs
    /// the parent class.
    pub fn artifact_root(&self) -> Option<Hash64> {
        self.parent_class.map(|c| palw_improve_composite_root_v1(&c, &self.parent_root, &self.adapter_root, self.p))
    }

    /// The provenance record (`meta.composite`).
    pub fn meta(&self) -> serde_json::Value {
        let mut v = serde_json::json!({
            "p": self.p,
            "parent_root": self.parent_root.to_string(),
            "adapter_root": self.adapter_root.to_string(),
            "parent_leaves": self.parent_leaves,
            "adapter_leaves": self.adapter_leaves,
        });
        if let Some(c) = self.parent_class {
            v["parent_class"] = serde_json::Value::String(c.to_string());
        }
        v
    }

    /// A record read back: every root 128 lowercase hex characters.
    pub fn from_meta(v: &serde_json::Value) -> Result<Self, String> {
        let hash = |k: &str| -> Result<Hash64, String> {
            let s = v.get(k).and_then(|x| x.as_str()).ok_or_else(|| format!("composite: no `{k}`"))?;
            if s.len() != 128 || !s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
                return Err(format!("composite: `{k}` is not 128 lowercase hex characters"));
            }
            s.parse::<Hash64>().map_err(|e| format!("composite: `{k}`: {e:?}"))
        };
        let number = |k: &str| -> Result<u32, String> {
            v.get(k).and_then(|x| x.as_u64()).and_then(|x| u32::try_from(x).ok()).ok_or_else(|| format!("composite: no `{k}`"))
        };
        Ok(Self {
            p: number("p")?,
            parent_root: hash("parent_root")?,
            parent_leaves: number("parent_leaves")?,
            adapter_root: hash("adapter_root")?,
            adapter_leaves: number("adapter_leaves")?,
            parent_class: if v.get("parent_class").is_some() { Some(hash("parent_class")?) } else { None },
        })
    }
}

/// The container's provenance as a JSON object (`{"provenance": …}` around anything else).
fn meta_of(c: &PalwTirContainerV1) -> serde_json::Value {
    match serde_json::from_str::<serde_json::Value>(&c.header.meta) {
        Ok(v) if v.is_object() => v,
        Ok(v) => serde_json::json!({ "provenance": v }),
        Err(_) => serde_json::json!({ "provenance": c.header.meta }),
    }
}

/// **`P` as the converter recorded it** (`meta.composite.p`, `palw-tir-fidelity --adapter`).
pub fn tir_composite_p_v1(candidate: &PalwTirContainerV1) -> Result<u32, String> {
    meta_of(candidate)
        .get("composite")
        .and_then(|c| c.get("p"))
        .and_then(|p| p.as_u64())
        .and_then(|p| u32::try_from(p).ok())
        .ok_or_else(|| {
            "the container records no composite.p: it is not a candidate written by palw-tir-fidelity --adapter".to_string()
        })
}

/// The family rule over two containers, before either has a layout. The rule reads only the classes'
/// tokenizers and the programs. A candidate whose program names no logits scheme is judged under
/// its parent's scheme, since it is declared under it.
fn family(parent: &PalwTirContainerV1, candidate: &PalwTirContainerV1) -> Result<(), String> {
    let class = |c: &PalwTirContainerV1| PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: Vec::new(),
        layout: PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 0,
            checkpoint_interval: 0,
            h_tile: 0,
            commit_tiles: Vec::new(),
            state_tiles: Vec::new(),
        },
        tokenizer_id: Hash64::from_bytes(c.header.tokenizer_id),
    };
    let mut program = candidate.program.clone();
    if program.logits_scheme_id == [0u8; 64] {
        program.logits_scheme_id = parent.program.logits_scheme_id;
    }
    palw_tir_family_rule_v1(&class(parent), &parent.program, &class(candidate), &program).map_err(|e| e.to_string())
}

/// **The composite of `candidate` over `parent`**, from their containers. It requires:
/// * the recorded `P`;
/// * the family rule;
/// * the composite rule (the parent's params first, unchanged; an adapter section; every block
///   within 512 nodes);
/// * the candidate's section `0..P` rooting to the parent's inventory root over as many leaves.
///
/// The last fails when the candidate was materialised with calibration other than the parent's.
/// Both inventories are streamed from the files, one tensor at a time.
pub fn tir_composite_derive_v1(
    parent: &PalwTirContainerV1,
    candidate: &PalwTirContainerV1,
    parent_class: Option<Hash64>,
) -> Result<TirCompositeV1, String> {
    let p = tir_composite_p_v1(candidate)?;
    family(parent, candidate)?;
    palw_tir_composite_rule_v1(&parent.program, &candidate.program, p).map_err(|e| e.to_string())?;
    let (parent_root, parent_leaves) =
        palw_tir_inventory_root_v1(&parent.program, &PalwTirContainerSourceV1(parent)).map_err(|e| format!("the parent: {e}"))?;
    let ((section_root, section_leaves), (adapter_root, adapter_leaves)) =
        palw_tir_composite_section_roots_v1(&candidate.program, p, &PalwTirContainerSourceV1(candidate))
            .map_err(|e| format!("the candidate: {e}"))?;
    if (section_root, section_leaves) != (parent_root, parent_leaves) {
        return Err(format!(
            "the candidate's params 0..{p} root to {section_root} over {section_leaves} leaves, not to the parent's inventory root \
             {parent_root} over {parent_leaves}: its parent tensors are not the parent artifact's (materialise the candidate with the \
             parent's calibration: palw-tir-fidelity --adapter … --parent-stats <the parent's --stats-out>)"
        ));
    }
    Ok(TirCompositeV1 { p, parent_root, parent_leaves, adapter_root, adapter_leaves, parent_class })
}

/// **Write `candidate` again to `output` with `composite` in its provenance** (`meta.composite`
/// replaced; the program, layout, tokenizer and every tensor unchanged). Returns the file digest.
pub fn tir_composite_write_v1(candidate: &PalwTirContainerV1, composite: &TirCompositeV1, output: &Path) -> Result<[u8; 64], String> {
    if output == candidate.path.as_path() {
        return Err("write the composite to another path than the candidate (the candidate is read while it is written)".into());
    }
    let mut meta = meta_of(candidate);
    meta["composite"] = composite.meta();
    write_container_v1(
        output,
        &candidate.program,
        candidate.header.layout.clone(),
        candidate.header.tokenizer_id,
        meta.to_string(),
        &mut |j, l| candidate.read_tensor_bytes(j, l).map_err(|e| e.to_string()),
    )
    .map_err(|e| format!("{}: {e}", output.display()))
}

/// **Write the candidate's adapter section** (`PALWTIRS`, RFC-0004 §6.7 — what a seat holding the
/// parent fetches instead of the whole candidate): params `composite.p..` of `candidate`, with its
/// program, layout and tokenizer, and the composite record in its provenance. The record must name
/// the parent class: a node matches the section to the parent it holds by it
/// ([`tir_composite_ref_of_meta_v1`]). Returns the file digest.
pub fn tir_composite_section_write_v1(
    candidate: &PalwTirContainerV1,
    composite: &TirCompositeV1,
    output: &Path,
) -> Result<[u8; 64], String> {
    if composite.parent_class.is_none() {
        return Err("a section names its parent class, which a node matches it to (--parent-class <hex>)".into());
    }
    if output == candidate.path.as_path() {
        return Err("write the section to another path than the candidate (the candidate is read while it is written)".into());
    }
    let mut meta = meta_of(candidate);
    meta["composite"] = composite.meta();
    write_section_v1(
        output,
        &candidate.program,
        composite.p,
        candidate.header.layout.clone(),
        candidate.header.tokenizer_id,
        meta.to_string(),
        &mut |j, l| candidate.read_tensor_bytes(j, l).map_err(|e| e.to_string()),
    )
    .map_err(|e| format!("{}: {e}", output.display()))
}

/// **The chain's reference a section's provenance records** (`meta.composite`, parent class
/// included) — what a node opens the section against and holds to the candidate's registration.
pub fn tir_composite_ref_of_meta_v1(meta: &str) -> Result<PalwTirCompositeRefV1, String> {
    let v: serde_json::Value = serde_json::from_str(meta).map_err(|e| format!("the provenance is not JSON: {e}"))?;
    let c = TirCompositeV1::from_meta(v.get("composite").ok_or("the provenance records no composite")?)?;
    let parent_class = c.parent_class.ok_or("the composite record names no parent class")?;
    Ok(PalwTirCompositeRefV1 { parent_class, parent_root: c.parent_root, adapter_root: c.adapter_root, p: c.p })
}
