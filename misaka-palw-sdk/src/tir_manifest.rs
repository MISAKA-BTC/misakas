//! **The `.palwmanifest` of a `PALWTIR1` artifact** (RFC-0002 Phase F, F3).
//!
//! A TIR artifact's inventory root is a function of the program's declarations and the tensors —
//! not of a profile — so unlike a legacy artifact (one root per class it pairs with) it has ONE
//! root, whatever layouts its registrations later declare (several layouts of one program are several
//! classes over the same `artifact_root`, design §2.3). The sidecar therefore records that one root,
//! beside the program's `graph_ir_root`, the tokenizer id and the file's digest:
//!
//! ```json
//! { "schema": "misaka.palw.tir-manifest.v1", "artifact_digest": …, "artifact_bytes": …,
//!   "graph_ir_root": …, "inventory_root": …, "leaf_count": …, "tokenizer_id": …, "program_bytes": …,
//!   "layout_digest": … | null, "class_id": … | null }
//! ```
//!
//! When the container declares a commitment layout (`borsh(PalwTirLayoutV1)`, F2) the sidecar also
//! records the layout's digest and the IR class id it makes with this artifact —
//! `PalwTirClassV1 { program, layout, tokenizer_id }.class_id(inventory_root)`, the id a registration
//! of this file under that layout must carry. A layout that does not decode is refused.
//!
//! Like the legacy manifest it is a cache, not an authority: [`PalwTirManifestV1::derive`] is the
//! one derivation (the consensus inventory, streamed from the container one tensor at a time), and
//! [`PalwTirManifestV1::check`] recomputes it and refuses any disagreement.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_graph_ir_root_v1, palw_tir_inventory_root_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1};
use misaka_palw_tir_artifact::{PalwTirContainerV1, file_digest_v1};
use std::borrow::Cow;
use std::path::Path;

pub const PALW_TIR_MANIFEST_SCHEMA_V1: &str = "misaka.palw.tir-manifest.v1";

/// The consensus inventory's tensor source over an opened container: each instance read from the
/// file when the walk reaches it, nothing retained.
pub struct PalwTirContainerSourceV1<'c>(pub &'c PalwTirContainerV1);

impl PalwTirTensorSourceV1 for PalwTirContainerSourceV1<'_> {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        self.0.read_tensor_bytes(param, layer).ok().map(Cow::Owned)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirManifestV1 {
    pub artifact_digest: [u8; 64],
    pub artifact_bytes: u64,
    pub graph_ir_root: Hash64,
    pub inventory_root: Hash64,
    pub leaf_count: u32,
    pub tokenizer_id: [u8; 64],
    pub program_bytes: u64,
    /// With a declared layout: its digest (as the class id binds it) and the class id.
    pub layout_digest: Option<Hash64>,
    pub class_id: Option<Hash64>,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex64(s: &str) -> Result<[u8; 64], String> {
    if s.len() != 128 {
        return Err(format!("`{s}` is not 64 hex bytes"));
    }
    let mut out = [0u8; 64];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

impl PalwTirManifestV1 {
    /// **The one derivation**: open the container (every header claim checked against the program),
    /// hash the file, and stream the consensus inventory root over its tensors.
    pub fn derive(path: &Path) -> Result<Self, String> {
        let c = PalwTirContainerV1::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (inventory_root, leaf_count) =
            palw_tir_inventory_root_v1(&c.program, &PalwTirContainerSourceV1(&c)).map_err(|e| format!("{}: {e}", path.display()))?;
        let (layout_digest, class_id) = match c.header.layout.is_empty() {
            true => (None, None),
            false => {
                let layout: PalwTirLayoutV1 = borsh::from_slice(&c.header.layout)
                    .map_err(|e| format!("{}: the container's layout is not a PalwTirLayoutV1: {e}", path.display()))?;
                let class = PalwTirClassV1 {
                    version: PALW_TIR_CLASS_VERSION_V1,
                    program: c.header.program.clone(),
                    layout,
                    tokenizer_id: Hash64::from_bytes(c.header.tokenizer_id),
                };
                (Some(class.layout_digest()), Some(class.class_id(&inventory_root)))
            }
        };
        Ok(Self {
            artifact_digest: file_digest_v1(path).map_err(|e| e.to_string())?,
            artifact_bytes: c.file_len,
            graph_ir_root: palw_tir_graph_ir_root_v1(&c.header.program),
            inventory_root,
            leaf_count,
            tokenizer_id: c.header.tokenizer_id,
            program_bytes: c.header.program.len() as u64,
            layout_digest,
            class_id,
        })
    }

    /// Recompute from the file and refuse any disagreement.
    pub fn check(&self, path: &Path) -> Result<(), String> {
        let fresh = Self::derive(path)?;
        if fresh.artifact_digest != self.artifact_digest {
            return Err("this manifest describes a different file (artifact digest)".into());
        }
        if fresh != *self {
            return Err(format!(
                "the file derives inventory root {} over {} leaves; the manifest says {} over {}",
                fresh.inventory_root, fresh.leaf_count, self.inventory_root, self.leaf_count
            ));
        }
        Ok(())
    }

    /// Canonical JSON: one shape, fixed field order.
    pub fn to_json(&self) -> String {
        let opt = |h: &Option<Hash64>| h.map_or("null".to_string(), |h| format!("\"{h}\""));
        format!(
            "{{\n  \"schema\": \"{PALW_TIR_MANIFEST_SCHEMA_V1}\",\n  \"artifact_digest\": \"{}\",\n  \"artifact_bytes\": {},\n  \"graph_ir_root\": \"{}\",\n  \"inventory_root\": \"{}\",\n  \"leaf_count\": {},\n  \"tokenizer_id\": \"{}\",\n  \"program_bytes\": {},\n  \"layout_digest\": {},\n  \"class_id\": {}\n}}\n",
            hex(&self.artifact_digest),
            self.artifact_bytes,
            self.graph_ir_root,
            self.inventory_root,
            self.leaf_count,
            hex(&self.tokenizer_id),
            self.program_bytes,
            opt(&self.layout_digest),
            opt(&self.class_id)
        )
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| e.to_string())?;
        if v["schema"].as_str() != Some(PALW_TIR_MANIFEST_SCHEMA_V1) {
            return Err(format!("not a {PALW_TIR_MANIFEST_SCHEMA_V1} manifest"));
        }
        let text = |k: &str| v[k].as_str().ok_or_else(|| format!("`{k}` missing"));
        let num = |k: &str| v[k].as_u64().ok_or_else(|| format!("`{k}` missing"));
        let hash = |k: &str| -> Result<Hash64, String> { Ok(Hash64::from_bytes(unhex64(text(k)?)?)) };
        let opt_hash = |k: &str| -> Result<Option<Hash64>, String> {
            match &v[k] {
                serde_json::Value::Null => Ok(None),
                serde_json::Value::String(t) => Ok(Some(Hash64::from_bytes(unhex64(t)?))),
                _ => Err(format!("`{k}` is neither a hash nor null")),
            }
        };
        Ok(Self {
            artifact_digest: unhex64(text("artifact_digest")?)?,
            artifact_bytes: num("artifact_bytes")?,
            graph_ir_root: hash("graph_ir_root")?,
            inventory_root: hash("inventory_root")?,
            leaf_count: u32::try_from(num("leaf_count")?).map_err(|e| e.to_string())?,
            tokenizer_id: unhex64(text("tokenizer_id")?)?,
            program_bytes: num("program_bytes")?,
            layout_digest: opt_hash("layout_digest")?,
            class_id: opt_hash("class_id")?,
        })
    }

    /// Is this a `PALWTIR1` file? (A four-byte sniff so `palw-class manifest` routes it.)
    pub fn sniff(path: &Path) -> bool {
        use std::io::Read;
        let mut m = [0u8; 8];
        std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut m)).is_ok()
            && &m == misaka_palw_tir_artifact::PALW_TIR_CONTAINER_MAGIC_V1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_artifact::{artifact_leaf_v1, artifact_root_v1, verify_artifact_opening_v1};
    use kaspa_consensus_core::palw_tir_artifact_v1::{
        palw_tir_inventory_operands_v1, palw_tir_leaf_index_v1, palw_tir_open_leaf_v1, palw_tir_param_instances_v1,
    };
    use misaka_palw_base0::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use misaka_palw_base0::engine_a16::derived_a16_store;
    use misaka_palw_base0::tir_a16::convert_a16_to_tir;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;

    fn converted(tag: &str) -> std::path::PathBuf {
        let shape = Base0ShapeV1 {
            n_layers: 2,
            n_heads: 4,
            n_kv_heads: 2,
            d_head: 8,
            d_ff: 48,
            vocab: 64,
            max_position: 32,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        let a = Base0ArtifactV1::derive_deterministic(shape, 0x3F3)
            .expect("shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("store");
        let path = std::env::temp_dir().join(format!("tir-manifest-{tag}-{}.palwtir", std::process::id()));
        convert_a16_to_tir(&a, HISTORY_BOUND_V1_SMALL, &path, "{}".into()).expect("converted");
        path
    }

    #[test]
    fn the_container_and_the_consensus_inventory_agree_on_instances_and_root() {
        let path = converted("root");
        let c = PalwTirContainerV1::open(&path).expect("opens");
        // One instance rule, two spellings (the container crate does not depend on consensus).
        assert_eq!(misaka_palw_tir_artifact::param_instances_v1(&c.program), palw_tir_param_instances_v1(&c.program));
        // Streamed from the file = materialised from every leaf; openings verify.
        let src = PalwTirContainerSourceV1(&c);
        let (root, count) = palw_tir_inventory_root_v1(&c.program, &src).expect("root");
        let ops = palw_tir_inventory_operands_v1(&c.program, &src).expect("operands");
        assert_eq!(ops.len() as u32, count);
        assert_eq!(artifact_root_v1(&ops.iter().map(artifact_leaf_v1).collect::<Vec<_>>()), Some(root));
        for index in [0, count / 3, count - 1] {
            let o = palw_tir_open_leaf_v1(&c.program, &src, index).expect("opening");
            verify_artifact_opening_v1(&o, root).expect("verifies");
            let j = c.program.param_index(&o.operand.tensor_name).expect("a declared param");
            assert_eq!(palw_tir_leaf_index_v1(&c.program, j, o.operand.layer, o.operand.row_start as u64), Some(index));
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_lowerer_binds_the_tokenizer_commitment_base0_binds() {
        // One rule in two crates (the lowerer does not depend on base0): the same key, the same id.
        assert_eq!(
            misaka_palw_tir_lower::artifact::TOKENIZER_COMMITMENT_DOMAIN_V1,
            misaka_palw_base0::artifact::PALW_BASE0_TOKENIZER_DOMAIN
        );
        for bytes in [&b""[..], b"{\"model\":{\"type\":\"BPE\"}}", &[7u8; 4099][..]] {
            assert_eq!(
                &misaka_palw_tir_lower::artifact::tokenizer_id_of(bytes)[..],
                Base0ArtifactV1::tokenizer_commitment_of(bytes).as_byte_slice()
            );
        }
    }

    #[test]
    fn a_declared_layout_gives_the_class_id_a_registration_must_carry() {
        let path = converted("layout");
        let c = PalwTirContainerV1::open(&path).expect("opens");
        assert!(PalwTirManifestV1::derive(&path).expect("derived").class_id.is_none(), "no layout, no class id");
        let layout = PalwTirLayoutV1 {
            version: kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 16,
            checkpoint_interval: 1,
            h_tile: 16,
            commit_tiles: vec![64; c.program.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).count()],
            state_tiles: vec![16; c.program.states.len()],
        };
        let with = path.with_extension("layout.palwtir");
        let mut tensor = |p: u16, l: Option<u16>| c.read_tensor_bytes(p, l).map_err(|e| e.to_string());
        misaka_palw_tir_artifact::write_container_v1(
            &with,
            &c.program,
            borsh::to_vec(&layout).expect("borsh"),
            [5u8; 64],
            "{}".into(),
            &mut tensor,
        )
        .expect("written");
        let m = PalwTirManifestV1::derive(&with).expect("derived");
        let class = PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: c.header.program.clone(),
            layout,
            tokenizer_id: Hash64::from_bytes([5u8; 64]),
        };
        // The same tensors, so the same inventory root; the class id binds program, layout, root, tokenizer.
        assert_eq!(m.inventory_root, PalwTirManifestV1::derive(&path).expect("derived").inventory_root);
        assert_eq!(m.class_id, Some(class.class_id(&m.inventory_root)));
        assert_eq!(m.layout_digest, Some(class.layout_digest()));
        assert_eq!(PalwTirManifestV1::from_json(&m.to_json()).expect("parses"), m);
        // A layout that is not a PalwTirLayoutV1 is refused.
        let bad = path.with_extension("badlayout.palwtir");
        misaka_palw_tir_artifact::write_container_v1(&bad, &c.program, vec![9, 9, 9], [0u8; 64], "{}".into(), &mut tensor)
            .expect("written");
        assert!(PalwTirManifestV1::derive(&bad).expect_err("refused").contains("not a PalwTirLayoutV1"));
        for p in [&path, &with, &bad] {
            let _ = std::fs::remove_file(p);
        }
    }

    #[test]
    fn a_manifest_round_trips_and_catches_a_changed_file() {
        let path = converted("manifest");
        let m = PalwTirManifestV1::derive(&path).expect("derived");
        assert!(PalwTirManifestV1::sniff(&path));
        assert_eq!(PalwTirManifestV1::from_json(&m.to_json()).expect("parses"), m);
        m.check(&path).expect("agrees with its own file");
        // One tensor byte changed: the digest and the root both move.
        let c = PalwTirContainerV1::open(&path).expect("opens");
        let (off, _) = c.locate(0, None).expect("the embedding");
        let mut bytes = std::fs::read(&path).expect("read");
        bytes[off as usize] ^= 1;
        std::fs::write(&path, &bytes).expect("write");
        assert!(m.check(&path).is_err());
        let fresh = PalwTirManifestV1::derive(&path).expect("still a container");
        assert_ne!(fresh.inventory_root, m.inventory_root);
        let _ = std::fs::remove_file(&path);
    }
}
