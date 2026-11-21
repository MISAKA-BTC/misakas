//! Calibrated 64-lane residual scales for the private integer candidate.
//! The parent is the complete fixed-scale conversion. The section holds only
//! the additional scale tables, rooted and checked by the native composite reader.
use super::{TOKENIZER, bind, layout};
use crate::tir_composite::{TirCompositeV1, tir_composite_ref_of_path_v1};
use kaspa_consensus_core::{
    palw_improve_composite_v1::{PalwTirCompositeRefV1, palw_tir_composite_rule_v1},
    palw_qf1::ROLES,
    palw_step_refute::tiled_logits_scheme_id_v1,
    palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_section_root_v1},
};
use kaspa_hashes::{Hash64, blake2b_512_keyed};
use misaka_palw_tir_artifact::{PALW_TIR_CONTAINER_MAGIC_V1, container_header_v1, write_section_v1};
use misaka_palw_tir_exec::node::{TirArtifactV1, qf1::LocalInstance};
use misaka_palw_tir_lower::qf1::{Qf1Program, residual_tile_program};
use serde::Deserialize;
use std::{borrow::Cow, collections::BTreeMap, path::Path};

const RECIPE: &[u8] = include_bytes!("residual-tiles.json");
type Result<T> = std::result::Result<T, String>;

#[derive(Deserialize)]
struct Layer {
    q: u8,
    k: u8,
    v: u8,
    up: u8,
    product: u8,
    residual: Vec<u8>,
}

fn epsilon(bits: u8) -> Vec<u8> {
    (((1_i64 << (24 + 2 * bits)) + 500_000) / 1_000_000).to_le_bytes().to_vec()
}

fn factors(bits: &[u8], common: u8) -> Vec<u8> {
    bits.iter().flat_map(|b| (1_i32 << (common - b)).to_le_bytes().repeat(64)).collect()
}

struct Tables(BTreeMap<(u16, Option<u16>), Vec<u8>>);
impl PalwTirTensorSourceV1 for Tables {
    fn tensor_bytes(&self, j: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        self.0.get(&(j, layer)).map(|raw| Cow::Borrowed(raw.as_slice()))
    }
}

// A16 narrowing clamps its shift. Refuse any conversion for which this would
// alter the requested scale, before creating a descriptor for the section.
fn check_shift(parent: &TirArtifactV1, projection: &str, layer: Option<u16>, deltas: &[u8]) -> Result<()> {
    let name = format!("{projection}.weight.shift");
    let j = parent.plan().program.params.iter().position(|p| p.name == name).ok_or("missing parent projection")?;
    let Some(misaka_palw_tir_exec::Slice::I8(base)) = parent.params().get(j as u16, layer) else {
        return Err("missing parent shifts".into());
    };
    if deltas.len() != 1 && deltas.len() != base.len() {
        return Err("wrong number of projection deltas".into());
    }
    if base.iter().zip(deltas.iter().cycle()).any(|(&s, &d)| !(0..=62).contains(&(i16::from(s) + i16::from(d as i8)))) {
        return Err(format!("{projection} shift out of range at {layer:?}"));
    }
    Ok(())
}

pub struct Candidate {
    model: Qf1Program,
    tables: Tables,
    reference: PalwTirCompositeRefV1,
    manifest: Hash64,
    metadata: String,
    scales: [Hash64; ROLES],
}

impl Candidate {
    pub fn over(parent: &TirArtifactV1) -> Result<Self> {
        let original = bind(parent)?.identity();
        let model = residual_tile_program(tiled_logits_scheme_id_v1().as_bytes()).map_err(|e| e.to_string())?;
        let program = &model.program;
        let split = u16::try_from(parent.plan().program.params.len()).map_err(|e| e.to_string())?;
        palw_tir_composite_rule_v1(&parent.plan().program, program, u32::from(split)).map_err(|e| e.to_string())?;
        let rows: Vec<Layer> = serde_json::from_slice(RECIPE).map_err(|e| e.to_string())?;
        if rows.len() != ROLES - 1 {
            return Err("wrong scale schedule depth".into());
        }
        let indices: BTreeMap<_, _> =
            program.params.iter().enumerate().skip(usize::from(split)).map(|(j, d)| (d.name.as_str(), j as u16)).collect();
        let mut data = BTreeMap::new();
        let mut add = |name: &str, layer: Option<u16>, raw: Vec<u8>| -> Result<()> {
            let &j = indices.get(name).ok_or("unknown scale table")?;
            let decl = &program.params[usize::from(j)];
            if decl.per_layer != layer.is_some()
                || decl.shape.iter().map(|d| *d as usize).product::<usize>() * decl.dtype.width() != raw.len()
                || data.insert((j, layer), raw).is_some()
            {
                return Err("invalid scale table instance".into());
            }
            Ok(())
        };
        let q10 = blake2b_512_keyed(b"MISAKA/private/QF1/scale/v1", b"i16 Q10");
        let mut scales = [q10; ROLES];
        let mut prev = vec![15_u8; 64];
        for (i, row) in rows.iter().enumerate() {
            let layer = Some(i as u16);
            if row.residual.len() != 64
                || row.residual.iter().zip(&prev).any(|(b, p)| b > p)
                || [row.q, row.k, row.v, row.up, row.product].iter().any(|b| *b > 15)
            {
                return Err("invalid frozen QF1 scale".into());
            }
            let before = *prev.iter().max().unwrap();
            let after = *row.residual.iter().max().unwrap();
            add("tile.input.eps", layer, epsilon(before))?;
            add("tile.post.eps", layer, epsilon(after))?;
            add("tile.q.eps", layer, epsilon(row.q))?;
            add("tile.k.eps", layer, epsilon(row.k))?;
            add("tile.input.factor", layer, factors(&prev, before))?;
            add("tile.post.factor", layer, factors(&row.residual, after))?;
            add("tile.skip.shift", layer, prev.iter().zip(&row.residual).flat_map(|(a, b)| [a - b; 64]).collect())?;
            for (name, projection, value) in [
                ("tile.q.delta", "self_attn.q_proj", row.q),
                ("tile.k.delta", "self_attn.k_proj", row.k),
                ("tile.v.delta", "self_attn.v_proj", row.v),
                ("tile.up.delta", "mlp.up_proj", row.up),
            ] {
                let delta = vec![(10_i8 - value as i8) as u8];
                check_shift(parent, projection, layer, &delta)?;
                add(name, layer, delta)?;
            }
            for (name, projection, value) in
                [("tile.o.delta", "self_attn.o_proj", row.v), ("tile.down.delta", "mlp.down_proj", row.product)]
            {
                let delta: Vec<_> = row.residual.iter().flat_map(|&b| [(5_i8 + value as i8 - b as i8) as u8; 64]).collect();
                check_shift(parent, projection, layer, &delta)?;
                add(name, layer, delta)?;
            }
            add("tile.product.shift", layer, vec![24 + row.up - row.product])?;
            scales[i] = blake2b_512_keyed(b"MISAKA/private/QF1/i16-contiguous-64-fraction-bits/v1", &row.residual);
            prev.clone_from(&row.residual);
        }
        let last = *prev.iter().max().unwrap();
        add("tile.final.factor", None, factors(&prev, last))?;
        add("tile.final.eps", None, epsilon(last))?;
        let tables = Tables(data);
        let (adapter_root, adapter_leaves) =
            palw_tir_inventory_section_root_v1(program, split..program.params.len() as u16, &tables).map_err(|e| e.to_string())?;
        let reference =
            PalwTirCompositeRefV1 { p: u32::from(split), parent_class: original.class, parent_root: original.inventory, adapter_root };
        let manifest = blake2b_512_keyed(b"MISAKA/private/QF1/residual-tiles/v1", RECIPE);
        let record = TirCompositeV1 {
            p: reference.p,
            parent_class: Some(original.class),
            parent_root: original.inventory,
            parent_leaves: parent.inventory_root()?.1,
            adapter_root,
            adapter_leaves,
        };
        let metadata = serde_json::json!({"manifest":manifest.to_string(),"composite":record.meta()}).to_string();
        Ok(Self { model, tables, reference, manifest, scales, metadata })
    }

    pub fn write(&self, parent: &TirArtifactV1, dest: &Path) -> Result<()> {
        let dir = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let tmp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
        let p = &self.model.program;
        let tokenizer: Hash64 = TOKENIZER.parse().map_err(|_| "invalid pinned tokenizer")?;
        write_section_v1(
            tmp.path(),
            p,
            self.reference.p,
            borsh::to_vec(&layout(p)).map_err(|e| e.to_string())?,
            tokenizer.as_bytes(),
            self.metadata.clone(),
            &mut |j, l| self.tables.0.get(&(j, l)).cloned().ok_or("missing scale".into()),
        )
        .map_err(|e| e.to_string())?;
        tmp.as_file().sync_all().map_err(|e| e.to_string())?;
        self.open(parent, tmp.path())?;
        tmp.persist_noclobber(dest).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn open(&self, parent: &TirArtifactV1, section: &Path) -> Result<TirArtifactV1> {
        if parent.class()?.class_id(&parent.inventory_root()?.0) != self.reference.parent_class {
            return Err("wrong QF1 parent".into());
        }
        let listed = tir_composite_ref_of_path_v1(section).ok_or("invalid section header")??;
        if listed != self.reference {
            return Err("section header does not name the frozen QF1 inventory".into());
        }
        let artifact = TirArtifactV1::open_composite_over(parent, section, &self.reference)?;
        self.bind(&artifact)?;
        Ok(artifact)
    }

    /// The ordinary inventory commitment over every actual tensor, in native inventory order.
    pub fn full_identity(&self, source: &TirArtifactV1) -> Result<(Hash64, Hash64)> {
        let id = self.bind_full(source)?.identity();
        Ok((id.class, id.inventory))
    }

    pub fn final_norm(&self) -> u16 {
        self.model.final_norm
    }

    /// Bind an ordinary single-root numerical instance. The graph and every
    /// tensor must belong to this conversion, whether supplied as a full file
    /// or read directly from the authenticated parent and local scale section.
    pub fn bind_full<'a>(&self, artifact: &'a TirArtifactV1) -> Result<LocalInstance<'a>> {
        if artifact.composite_ref().is_some() {
            self.check_section(artifact)?;
        } else {
            let class = artifact.class()?;
            if class.program != self.model.program.encode()
                || class.layout != layout(&self.model.program)
                || class.tokenizer_id.to_string() != TOKENIZER
                || artifact.header().meta != self.manifest.to_string()
            {
                return Err("wrong full QF1 graph or recipe".into());
            }
            let ((parent, _), (adapter, _)) = kaspa_consensus_core::palw_improve_composite_v1::palw_tir_composite_section_roots_v1(
                &self.model.program,
                self.reference.p,
                artifact,
            )
            .map_err(|e| e.to_string())?;
            if (parent, adapter) != (self.reference.parent_root, self.reference.adapter_root) {
                return Err("wrong full QF1 tensors".into());
            }
        }
        LocalInstance::bind_full_weights(artifact, self.manifest, self.model.final_norm, &self.scales)
    }

    /// Emit the complete portable PALWTIR1, including the parent's exact bytes.
    /// The caller can stream it directly to a verifier or to a new ordinary file.
    pub fn write_full(&self, source: &TirArtifactV1, mut out: impl std::io::Write) -> Result<()> {
        self.bind(source)?;
        let header = container_header_v1(
            &self.model.program,
            source.header().layout.clone(),
            source.header().tokenizer_id,
            self.manifest.to_string(),
        )
        .map_err(|e| e.to_string())?;
        let encoded = borsh::to_vec(&header).map_err(|e| e.to_string())?;
        let write = |r: std::io::Result<()>| r.map_err(|e| e.to_string());
        write(out.write_all(PALW_TIR_CONTAINER_MAGIC_V1))?;
        write(out.write_all(&header.version.to_le_bytes()))?;
        write(out.write_all(&u32::try_from(encoded.len()).map_err(|e| e.to_string())?.to_le_bytes()))?;
        write(out.write_all(&encoded))?;
        let mut pos = 14 + encoded.len() as u64;
        for e in header.tensors {
            let bytes = source.tensor_bytes(e.param, e.layer).ok_or("missing full candidate tensor")?;
            if bytes.len() as u64 != e.bytes || !(pos..pos + 64).contains(&e.offset) {
                return Err("invalid native tensor extent".into());
            }
            write(out.write_all(&[0; 64][..(e.offset - pos) as usize]))?;
            write(out.write_all(&bytes))?;
            pos = e.offset + e.bytes;
        }
        write(out.flush())?;
        source.check_weights_unmodified()
    }

    pub fn bind<'a>(&self, artifact: &'a TirArtifactV1) -> Result<LocalInstance<'a>> {
        self.check_section(artifact)?;
        LocalInstance::bind(artifact, self.manifest, self.model.final_norm, &self.scales)
    }

    fn check_section(&self, artifact: &TirArtifactV1) -> Result<()> {
        let class = artifact.class()?;
        if artifact.composite_ref() != Some(&self.reference)
            || class.program != self.model.program.encode()
            || class.layout != layout(&self.model.program)
            || class.tokenizer_id.to_string() != TOKENIZER
            || artifact.header().meta != self.metadata
        {
            return Err("QF1 residual-tile class or manifest mismatch".into());
        }
        Ok(())
    }
}
