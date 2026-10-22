//! **RFC-0002 / genesis 8 audit item 02-A: a TIR class's `PALWART3` container, bound to its registration** (dormant).
//!
//! A genesis-8 model ships as ONE `PALWART3` artifact, and its catalog names the container's root
//! (`misaka_palw_tir_artifact::palwart_v3::palwart_root_v3`). A TIR class registers — and every court opens its operands —
//! under the TIR **inventory** root, which the container commits as its weight section's `inner_root`. Before this module
//! nothing on chain related the two: the same type (`Hash64`) named two different objects. Here the chain records, per IR
//! class, the container whose root commits that class's inventory root and program, so the path an adjudication walks is
//!
//! ```text
//! registered container root ──(root opening, recomputed here)──► weight section inner root == class artifact_root
//!                                                                  └► operand openings under artifact_root (unchanged courts)
//!                          └──(program record)──► program bytes == the registered class program (under its logits scheme)
//!                                                 tokenizer id == the class record's
//! ```
//!
//! * **What is opened.** The container root is BLAKE2b-512 over the version, the manifest's digest, the program record and
//!   every section's kind, family, name, inner-root scheme, inner root, leaf count and entry-table root. The opening carries
//!   exactly those (the manifest and each entry table as their digests, the program as its bytes), so the chain recomputes the
//!   root without reading a weight. The recomputation is the twin of `palwart_root_v3`; the SDK's test pins them equal on a
//!   real packed container.
//! * **What it refuses.** A root that the opening does not recompute (another container); a weight section whose inner root is
//!   not the class's artifact root (another inventory under the same container identity); a program record whose bytes, graph
//!   id or tokenizer are not the registered class's (another runtime under the same inventory); more than one weight section;
//!   an unsigned or foreign binding while the registrant is Active (the registrant signs, as for `ClassSemanticsDeclaredV1`);
//!   with no Active registrant the binding is open and unsigned (`palw_tir_container_binding_signer_v1`, audit C-01).
//! * **Not a proof of computation** (PRINCIPLES §8): hashes for identity only. Whether a claim over these bytes was computed
//!   correctly is still decided by sampled re-execution and the deterministic court.
//! * **Dormant.** Owned by `Params::palw_tir_container_identity_v1` (None on every preset): below it the kind rides unjudged.
use crate::Hash64;
use borsh::{BorshDeserialize, BorshSerialize};

pub const PALW_TIR_CONTAINER_BINDING_VERSION_V1: u16 = 1;
/// The `PALWART3` header version the opening recomputes.
pub const PALWART_VERSION_V3: u16 = 3;
pub const PALWART_ROOT_DOMAIN_V3: &[u8] = b"misaka-palw/palwart/v3/root";
pub const PALWART_MANIFEST_DOMAIN_V3: &[u8] = b"misaka-palw/palwart/v3/manifest";
pub const PALWART_PROGRAM_DOMAIN_V3: &[u8] = b"misaka-palw/palwart/v3/program";
/// `palwart_v3::PALWART_MAX_SECTIONS_V3`.
pub const PALWART_MAX_SECTIONS_V3: usize = 256;
/// `palwart_v3::section_kind::TIR_INVENTORY`.
pub const PALWART_SECTION_TIR_INVENTORY_V3: u16 = 3;
/// `palwart_v3::inner_scheme::TIR_INVENTORY_ROOT_V1`.
pub const PALWART_INNER_TIR_INVENTORY_ROOT_V3: u16 = 3;
/// `palwart_v3::program_kind::TIR_V1`.
pub const PALWART_PROGRAM_TIR_V1_V3: u16 = 3;
/// The ML-DSA-87 context the class registrant signs a binding under.
pub const PALW_TIR_CONTAINER_BINDING_SIGNATURE_CONTEXT_V1: &[u8] = b"misaka-palw/tir-container-binding/v1";
/// A section name's byte ceiling (a container's names are short tags).
pub const PALWART_MAX_SECTION_NAME_BYTES_V3: usize = 256;

/// `palwart_v3::section_kind::is_weights`.
pub fn palwart_section_is_weights_v3(kind: u16) -> bool {
    matches!(kind, 1..=4)
}

/// One section of a `PALWART3` header as its root commits it (the entry table as its root).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwArtSectionOpeningV1 {
    pub kind: u16,
    pub family: u16,
    pub name: String,
    pub inner_root_scheme: u16,
    pub inner_root: Hash64,
    pub inner_leaf_count: u64,
    /// `palwart_entries_root_v3` of the section's entries.
    pub entries_root: Hash64,
}

/// **Everything `palwart_root_v3` hashes**, with the manifest and each entry table reduced to their digests and the program
/// carried whole (the chain checks it against the registered class program).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwArtRootOpeningV1 {
    pub version: u16,
    /// BLAKE2b-512 keyed [`PALWART_MANIFEST_DOMAIN_V3`] over the canonical manifest bytes.
    pub manifest_digest: Hash64,
    pub program_kind: u16,
    pub program: Vec<u8>,
    pub kernel_id: Hash64,
    pub kernel_version: u16,
    pub tokenizer_id: Hash64,
    pub sections: Vec<PalwArtSectionOpeningV1>,
}

fn h64(domain: &[u8], bytes: &[u8]) -> [u8; 64] {
    let h = blake2b_simd::Params::new().hash_length(64).key(domain).hash(bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(h.as_bytes());
    out
}

fn put_str(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(&(s.len() as u64).to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
}

impl PalwArtRootOpeningV1 {
    /// **The container root this opening recomputes** — `palwart_root_v3`'s byte layout, field for field.
    pub fn root_v1(&self) -> Hash64 {
        let mut buf = Vec::with_capacity(1024 + self.sections.len() * 256);
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(self.manifest_digest.as_byte_slice());
        buf.extend_from_slice(&self.program_kind.to_le_bytes());
        buf.extend_from_slice(&h64(PALWART_PROGRAM_DOMAIN_V3, &self.program));
        buf.extend_from_slice(self.kernel_id.as_byte_slice());
        buf.extend_from_slice(&self.kernel_version.to_le_bytes());
        buf.extend_from_slice(self.tokenizer_id.as_byte_slice());
        buf.extend_from_slice(&(self.sections.len() as u32).to_le_bytes());
        for s in &self.sections {
            buf.extend_from_slice(&s.kind.to_le_bytes());
            buf.extend_from_slice(&s.family.to_le_bytes());
            put_str(&mut buf, &s.name);
            buf.extend_from_slice(&s.inner_root_scheme.to_le_bytes());
            buf.extend_from_slice(s.inner_root.as_byte_slice());
            buf.extend_from_slice(&s.inner_leaf_count.to_le_bytes());
            buf.extend_from_slice(s.entries_root.as_byte_slice());
        }
        Hash64::from_bytes(h64(PALWART_ROOT_DOMAIN_V3, &buf))
    }

    /// The manifest digest a canonical manifest has under this opening.
    pub fn manifest_digest_of_v1(manifest: &[u8]) -> Hash64 {
        Hash64::from_bytes(h64(PALWART_MANIFEST_DOMAIN_V3, manifest))
    }
}

/// **The binding a TIR class's registrant signs**: the class, its container's root, and the opening that recomputes it.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirContainerBindingV1 {
    pub version: u16,
    pub class_id: Hash64,
    pub container_root: Hash64,
    pub opening: PalwArtRootOpeningV1,
}

/// What the chain keeps once a binding folds (the opening's program is the class's own and is not kept twice).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirContainerRowV1 {
    pub container_root: Hash64,
    pub manifest_digest: Hash64,
    /// The weight section's index in the container.
    pub weights_section: u16,
    pub inner_leaf_count: u64,
    pub bound_daa: u64,
}

/// The message the registrant signs (network-domain separated).
pub fn palw_tir_container_binding_message_v1(network_domain: Hash64, binding: &PalwTirContainerBindingV1) -> Hash64 {
    let mut st = blake2b_simd::Params::new().hash_length(64).key(PALW_TIR_CONTAINER_BINDING_SIGNATURE_CONTEXT_V1).to_state();
    st.update(network_domain.as_byte_slice());
    st.update(&borsh::to_vec(binding).expect("an in-memory binding encodes"));
    Hash64::from_bytes(st.finalize().as_bytes().try_into().expect("64-byte digest"))
}

/// **Check a binding against the registered class** (`artifact_root` and `record` are the chain's): the opening recomputes the
/// container root, its only weight section is a TIR inventory whose inner root is the class's artifact root, and its program
/// record is the class's program (under the class's logits scheme), graph id and tokenizer. Returns the row to keep.
pub fn check_tir_container_binding_v1(
    binding: &PalwTirContainerBindingV1,
    class_id: &Hash64,
    artifact_root: &Hash64,
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    bound_daa: u64,
) -> Result<PalwTirContainerRowV1, &'static str> {
    let o = &binding.opening;
    if binding.version != PALW_TIR_CONTAINER_BINDING_VERSION_V1 || o.version != PALWART_VERSION_V3 {
        return Err("a container binding names an unknown binding or container version");
    }
    if binding.class_id != *class_id || record.facts.class_id != *class_id {
        return Err("a container binding names another class");
    }
    if o.sections.len() > PALWART_MAX_SECTIONS_V3 || o.sections.iter().any(|s| s.name.len() > PALWART_MAX_SECTION_NAME_BYTES_V3) {
        return Err("a container opening exceeds the container's section caps");
    }
    if o.root_v1() != binding.container_root || binding.container_root == Hash64::default() {
        return Err("the container opening does not recompute the bound container root");
    }
    let mut weights = o.sections.iter().enumerate().filter(|(_, s)| palwart_section_is_weights_v3(s.kind));
    let (index, section) = weights.next().ok_or("the bound container holds no weight section")?;
    if weights.next().is_some() {
        return Err("the bound container holds more than one weight section");
    }
    if section.kind != PALWART_SECTION_TIR_INVENTORY_V3 || section.inner_root_scheme != PALWART_INNER_TIR_INVENTORY_ROOT_V3 {
        return Err("the bound container's weight section is not a TIR inventory");
    }
    if section.inner_root != *artifact_root || section.inner_leaf_count == 0 {
        return Err("the bound container's inner inventory root is not the class's registered artifact root");
    }
    // The runtime: the container's program is the registered program, the class's logits scheme substituted (a registration
    // may choose the scheme of a container program that names none, `tir_program_with_scheme_v1`), and nothing else.
    if o.program_kind != PALWART_PROGRAM_TIR_V1_V3 || o.kernel_version != 0 {
        return Err("the bound container's program record is not a TIR v1 program");
    }
    if o.kernel_id != crate::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&o.program) {
        return Err("the bound container's kernel id is not its program's graph root");
    }
    if o.tokenizer_id != record.tokenizer_id {
        return Err("the bound container's tokenizer is not the registered class's");
    }
    let program = misaka_palw_tir::TirProgramV1::decode_canonical(&o.program)
        .map_err(|_| "the bound container's program does not decode canonically")?;
    let registered = misaka_palw_tir::TirProgramV1::decode_canonical(record.program.as_slice())
        .map_err(|_| "the registered class program does not decode")?;
    let mut rescheme = program;
    let named = rescheme.logits_scheme_id;
    if named != registered.logits_scheme_id {
        if named != [0u8; 64] {
            return Err("the bound container's program names another logits scheme than the registered class");
        }
        rescheme.logits_scheme_id = registered.logits_scheme_id;
    }
    if rescheme.encode() != *record.program.as_ref() {
        return Err("the bound container's program is not the registered class program");
    }
    Ok(PalwTirContainerRowV1 {
        container_root: binding.container_root,
        manifest_digest: o.manifest_digest,
        weights_section: index as u16,
        inner_leaf_count: section.inner_leaf_count,
        bound_daa,
    })
}

/// **The container a TIR class is bound to**, if its registrant bound one (the state's staged row).
pub fn palw_tir_bound_container_v1(state: &crate::palw_state_v2::PalwChainStateV2, class_id: Hash64) -> Option<&PalwTirContainerRowV1> {
    use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1, PalwStagedCourtRowV1};
    match state.staged_court_row_v1(&PalwStagedCourtKeyV1::TirContainer(class_id)) {
        Some(PalwStagedCourtRowV1::TirContainer(row)) => Some(row.as_ref()),
        _ => None,
    }
}

/// **Who must sign a TIR class's container binding** (audit C-01; only judged past `palw_tir_container_identity_v1`).
///
/// The binding is a deterministic identity check (`check_tir_container_binding_v1` recomputes the root, the inventory root, the
/// program and the tokenizer from the opening), so its signature is not what makes it true: it only gives the class's registrant
/// PRIORITY, because several distinct containers (other manifests or non-weight sections) can satisfy the same check and the first
/// one bound is immutable. Hence:
///
/// * the registrant bond exists and is `Active` → `Ok(Some(registrant))`: its signature is required, exactly as before;
/// * otherwise (a sentinel registration with no registrant, or a registrant that is retiring, retired, ejected or gone) →
///   `Ok(None)`: nobody can sign any more, and requiring it would leave the class unrewardable in the free-prompt lane forever.
///   The binding is then open and must carry an EMPTY signature (no malleable bytes ride); the identity check stays strict.
pub fn palw_tir_container_binding_signer_v1<'a>(
    state: &'a crate::palw_state_v2::PalwChainStateV2,
    binding: &PalwTirContainerBindingV1,
) -> Result<Option<(&'a crate::palw_state_v2::PalwBondKeyV2, &'a crate::palw_state_v2::PalwBondStateV2)>, &'static str> {
    let class = state.class(&binding.class_id).ok_or("a container binding names an unknown class")?;
    Ok(class
        .registrant_bond
        .as_ref()
        .and_then(|key| state.bond(key).map(|bond| (key, bond)))
        .filter(|(_, bond)| matches!(bond.status, crate::palw_state_v2::PalwBondStatusV2::Active)))
}

/// The refusal code of a TIR original whose class is unbound past the container fence (`ClassNotRewardable`).
pub const PALW_TIR_UNBOUND_CONTAINER_CODE_V1: &str = "TIR_CLASS_CONTAINER_UNBOUND";

/// **A test container binding for `record`** — a two-section opening (the TIR inventory at `artifact_root`, a tokenizer section)
/// over the record's own program and tokenizer, under the manifest `manifest`.
#[cfg(test)]
pub(crate) fn test_binding_v1(
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    artifact_root: Hash64,
    manifest: &[u8],
) -> PalwTirContainerBindingV1 {
    let program = record.program.as_ref().clone();
    let opening = PalwArtRootOpeningV1 {
        version: PALWART_VERSION_V3,
        manifest_digest: PalwArtRootOpeningV1::manifest_digest_of_v1(manifest),
        program_kind: PALWART_PROGRAM_TIR_V1_V3,
        kernel_id: crate::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&program),
        program,
        kernel_version: 0,
        tokenizer_id: record.tokenizer_id,
        sections: vec![
            PalwArtSectionOpeningV1 {
                kind: PALWART_SECTION_TIR_INVENTORY_V3,
                family: 1,
                name: "tir-inventory".into(),
                inner_root_scheme: PALWART_INNER_TIR_INVENTORY_ROOT_V3,
                inner_root: artifact_root,
                inner_leaf_count: 3,
                entries_root: Hash64::from_u64_word(7),
            },
            PalwArtSectionOpeningV1 {
                kind: 16,
                family: 0,
                name: "tokenizer".into(),
                inner_root_scheme: 0,
                inner_root: Hash64::default(),
                inner_leaf_count: 0,
                entries_root: Hash64::from_u64_word(8),
            },
        ],
    };
    PalwTirContainerBindingV1 {
        version: PALW_TIR_CONTAINER_BINDING_VERSION_V1,
        class_id: record.facts.class_id,
        container_root: opening.root_v1(),
        opening,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tiny court program's class record and a container opening that holds it — the honest case and each one-field change.
    fn case() -> (PalwTirContainerBindingV1, Hash64, Hash64, crate::palw_tir_admission_v1::PalwTirClassRecordV1) {
        let x = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let artifact_root = x.binding.artifact_root;
        let (record, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&x.binding.class, &artifact_root).unwrap();
        let class_id = record.facts.class_id;
        (test_binding_v1(&record, artifact_root, br#"{"model":"tiny"}"#), class_id, artifact_root, record)
    }

    #[test]
    fn a_container_that_commits_the_registered_inventory_and_program_binds() {
        let (binding, class_id, root, record) = case();
        let row = check_tir_container_binding_v1(&binding, &class_id, &root, &record, 9).expect("the honest binding");
        assert_eq!((row.container_root, row.weights_section, row.inner_leaf_count, row.bound_daa), (binding.container_root, 0, 3, 9));
    }

    #[test]
    fn changing_only_the_container_only_the_inner_root_or_only_the_runtime_is_refused() {
        let (honest, class_id, root, record) = case();
        let check = |b: &PalwTirContainerBindingV1| check_tir_container_binding_v1(b, &class_id, &root, &record, 9);
        // Only the container: another root named over the same opening.
        let mut b = honest.clone();
        b.container_root = Hash64::from_u64_word(1);
        assert_eq!(check(&b), Err("the container opening does not recompute the bound container root"));
        // Only the container: the opening of a container with another manifest, under the honest root.
        let mut b = honest.clone();
        b.opening.manifest_digest = PalwArtRootOpeningV1::manifest_digest_of_v1(br#"{"model":"other"}"#);
        assert_eq!(check(&b), Err("the container opening does not recompute the bound container root"));
        // Only the inner root: a container (consistently re-rooted) whose inventory is another.
        let mut b = honest.clone();
        b.opening.sections[0].inner_root = Hash64::from_u64_word(2);
        b.container_root = b.opening.root_v1();
        assert_eq!(check(&b), Err("the bound container's inner inventory root is not the class's registered artifact root"));
        // Only the runtime: the same inventory under another tokenizer…
        let mut b = honest.clone();
        b.opening.tokenizer_id = Hash64::from_u64_word(3);
        b.container_root = b.opening.root_v1();
        assert_eq!(check(&b), Err("the bound container's tokenizer is not the registered class's"));
        // …or another program (one parameter's name changed), its graph id re-derived so only the program differs.
        let mut b = honest.clone();
        let mut p = misaka_palw_tir::TirProgramV1::decode_canonical(&b.opening.program).unwrap();
        p.params[0].name = "embed2".into();
        b.opening.program = p.encode();
        b.opening.kernel_id = crate::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&b.opening.program);
        b.container_root = b.opening.root_v1();
        assert_eq!(check(&b), Err("the bound container's program is not the registered class program"));
        // …or a kernel id that is not the program's.
        let mut b = honest.clone();
        b.opening.kernel_id = Hash64::from_u64_word(4);
        b.container_root = b.opening.root_v1();
        assert_eq!(check(&b), Err("the bound container's kernel id is not its program's graph root"));
        // A second weight section, and a binding for another class.
        let mut b = honest.clone();
        let mut extra = b.opening.sections[0].clone();
        extra.name = "second".into();
        b.opening.sections.push(extra);
        b.container_root = b.opening.root_v1();
        assert_eq!(check(&b), Err("the bound container holds more than one weight section"));
        let mut b = honest.clone();
        b.class_id = Hash64::from_u64_word(5);
        assert_eq!(check(&b), Err("a container binding names another class"));
    }

    #[test]
    fn a_container_program_that_names_no_scheme_binds_under_the_registered_scheme_and_another_scheme_does_not() {
        let (honest, class_id, root, record) = case();
        let check = |b: &PalwTirContainerBindingV1| check_tir_container_binding_v1(b, &class_id, &root, &record, 9);
        let mut p = misaka_palw_tir::TirProgramV1::decode_canonical(&honest.opening.program).unwrap();
        p.logits_scheme_id = [0u8; 64];
        let mut b = honest.clone();
        b.opening.program = p.encode();
        b.opening.kernel_id = crate::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&b.opening.program);
        b.container_root = b.opening.root_v1();
        assert!(check(&b).is_ok(), "{:?}", check(&b));
        p.logits_scheme_id = crate::palw_step_refute::flat_logits_scheme_id_v1().as_bytes();
        b.opening.program = p.encode();
        b.opening.kernel_id = crate::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&b.opening.program);
        b.container_root = b.opening.root_v1();
        assert_eq!(check(&b), Err("the bound container's program names another logits scheme than the registered class"));
    }
}
