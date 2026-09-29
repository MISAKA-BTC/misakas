//! **RFC-0004 §6.2–§6.3 on real adapters: tir-lower's LoRA candidates are composite IR classes.**
//!
//! Each of tir-lower's PEFT fixtures (`misaka-palw-tir-lower/tests/fixtures/hf-lora`, ranks 4–64,
//! q/k/v/o and MLPs, `all-linear`, rsLoRA) is lowered as tir-lower's own test lowers it — the
//! parent alone, then the candidate with its adapter's params last (`lower::adapter_params_last`),
//! materialised with the parent's calibration plus the adapter's own sites — and then:
//!
//! * the candidate's parent section (params `0..P`) roots to the PARENT's inventory root: the parent
//!   is reused byte for byte, never re-committed;
//! * the composite reference over the parent class, the parent root, the adapter section's root and
//!   `P` gives the candidate's artifact root, and its class id is Phase F's formula over it;
//! * admission of the composite passes: the family rule, the composite rule, the node budget, and
//!   every terminal close carriable as a composite close carries it (`TirCloseDemandV1` across the
//!   parent and adapter roots) under testnet-12's cap;
//! * a candidate that recalibrates a parent scale, reorders the parent's params, names a parent
//!   tensor, changes the tokenizer, or splits anywhere but the parent's params is refused by name.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_improve_composite_v1::{
    PalwTirCandidateArtifactV1, PalwTirCandidateFactsV1, PalwTirCompositeAdmissionV1, PalwTirCompositeErrorV1,
    palw_tir_candidate_artifact_admits_v1, palw_tir_composite_admits_v1, palw_tir_composite_ref_v1, palw_tir_composite_rule_v1,
    palw_tir_composite_split_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_tir_admission_v1::{palw_tir_carriable_close_bytes_v1, palw_tir_class_record_v1};
use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_root_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
use kaspa_consensus_core::palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_WORK_CAP_V1;
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_hashes::Hash64;
use misaka_palw_sdk::tir_layout::{TirLayoutChoiceV1, tir_default_layout_v1, tir_program_with_scheme_v1};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, SiteStat};
use misaka_palw_tir_lower::lower::{self, IntData, IntParams, LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::{Checkpoint, Overlay};
use misaka_palw_tir_lower::{fidelity, hf_weights, hl, lora};

const TOKENIZER: Hash64 = Hash64::from_bytes([0x70; 64]);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures")
}

fn params() -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(1_000)));
    p.sync_palw_tir_v1();
    p
}

struct Src<'a>(&'a IntParams);

impl PalwTirTensorSourceV1 for Src<'_> {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        self.0.tensors.get(&(param, layer)).map(|t| Cow::Owned(t.le_bytes()))
    }
}

/// The parent and the candidate, lowered and materialised as tir-lower's LoRA test does: `(parent
/// program, parent artifact, candidate program, candidate artifact, P)`.
fn lowered(name: &str) -> (TirProgramV1, IntParams, TirProgramV1, IntParams, usize) {
    let ad_dir = fixtures().join("hf-lora").join(name);
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(ad_dir.join("logits.json")).unwrap()).unwrap();
    let base_dir = fixtures().join("hf").join(meta["base"].as_str().unwrap());
    let cfg = std::fs::read_to_string(base_dir.join("config.json")).unwrap();
    let opts = LowerOpts { max_window: Some(64), ..LowerOpts::default() };
    let spec_p = misaka_palw_tir_lower::parse_config_str(&cfg).expect("parent spec");
    let hl_p = hl::build_program(&spec_p).expect("parent hl");
    let bind_p = hf_weights::bind(&spec_p, &hl_p).expect("parent binding");
    let ck = Checkpoint::open(&base_dir).expect("checkpoint");
    let (pf, _) = ParamStore::from_source(&hl_p, &bind_p, &ck).expect("parent params");
    let mut spec_c = spec_p.clone();
    lora::attach(&mut spec_c, &std::fs::read_to_string(ad_dir.join("adapter_config.json")).unwrap()).expect("attach");
    let hl_c = hl::build_program(&spec_c).expect("candidate hl");
    let bind_c = hf_weights::bind(&spec_c, &hl_c).expect("candidate binding");
    let ad = Checkpoint::open(&ad_dir.join("adapter_model.safetensors")).expect("adapter");
    let (cf, _) = ParamStore::from_source(&hl_c, &bind_c, &Overlay { base: &ck, over: &ad }).expect("candidate params");
    let calib = fidelity::random_sequences(hl_p.vocab, 6, 32, 7);
    let quiet = |_: usize, _: usize| {};
    let lp = Resident(Arc::new(pf));
    let stats_p = fidelity::calibrate(&hl_p, &lp, &calib, &quiet).expect("parent calibration");
    let lw_p = lower::lower(&hl_p, &opts).expect("parent lower");
    let mat_p = materialise(&lw_p, &hl_p, &lp, &stats_p, &QuantPolicy::default(), &quiet).expect("parent artifact");
    let lc = Resident(Arc::new(cf));
    let stats_c = fidelity::calibrate(&hl_c, &lc, &calib, &quiet).expect("candidate calibration");
    let mut stats: BTreeMap<String, SiteStat> = stats_p.clone();
    for (k, v) in stats_c {
        if k.contains(lower::LORA_MARK) {
            stats.insert(k, v);
        }
    }
    let mut lw_c = lower::lower(&hl_c, &opts).expect("candidate lower");
    let p = lower::adapter_params_last(&mut lw_c).expect("reorder");
    let mat_c = materialise(&lw_c, &hl_c, &lc, &stats, &QuantPolicy::default(), &quiet).expect("candidate artifact");
    // Both under the tiled logits scheme (the lowerer names none).
    let parent = tir_program_with_scheme_v1(&lw_p.program, None).expect("the parent under a scheme");
    let candidate = tir_program_with_scheme_v1(&lw_c.program, None).expect("the candidate under a scheme");
    (parent, mat_p.params, candidate, mat_c.params, p)
}

fn class_of(net: &Params, program: &TirProgramV1, tokenizer: Hash64) -> PalwTirClassV1 {
    let choice = TirLayoutChoiceV1 { max_context: Some(64), ..TirLayoutChoiceV1::default() };
    let layout = tir_default_layout_v1(net, program, &choice).expect("a layout");
    PalwTirClassV1 { version: PALW_TIR_CLASS_VERSION_V1, program: program.encode(), layout, tokenizer_id: tokenizer }
}

fn rules(net: &Params) -> PalwTirCompositeAdmissionV1 {
    let PalwConsensusMode::ConsensusV2(bundle) = &net.palw_consensus_mode else { panic!("testnet-12 is a V2 network") };
    PalwTirCompositeAdmissionV1 {
        court: true,
        carriable: palw_tir_carriable_close_bytes_v1(&bundle.court),
        work_cap: PALW_TIR_CLOSE_SIZING_WORK_CAP_V1,
    }
}

fn check(name: &str) {
    let net = params();
    let (parent, mat_p, candidate, mat_c, p) = lowered(name);
    let p32 = p as u32;
    assert_eq!(p, parent.params.len(), "{name}: P is the parent's param count");
    // The parent class.
    let (parent_root, parent_leaves) = palw_tir_inventory_root_v1(&parent, &Src(&mat_p)).expect("the parent's root");
    let parent_class = class_of(&net, &parent, TOKENIZER);
    let parent_id = parent_class.class_id(&parent_root);
    // The candidate as a composite: its parent section IS the parent's inventory.
    let r = palw_tir_composite_ref_v1(parent_id, &candidate, p32, &Src(&mat_c)).expect("the sections");
    assert_eq!(r.parent_root, parent_root, "{name}: the candidate's parent section is the parent's inventory, byte for byte");
    let split = palw_tir_composite_split_v1(&candidate, p32).expect("a split");
    assert_eq!(split.parent_leaves, parent_leaves, "{name}: the parent's leaves are the candidate's first");
    let class = class_of(&net, &candidate, TOKENIZER);
    let root = r.artifact_root();
    let id = class.class_id(&root);
    let rules = rules(&net);
    type E = PalwTirCompositeErrorV1;
    let started = std::time::Instant::now();
    let bounds = palw_tir_composite_admits_v1(&parent_class, &parent_id, &parent_root, &class, &id, &root, &r, &rules)
        .unwrap_or_else(|e| panic!("{name}: the composite is admitted: {e}"));
    let worst = bounds.iter().map(|b| b.close_bytes).max().unwrap_or(0);
    let adapter_bytes: usize = mat_c.tensors.iter().filter(|((j, _), _)| *j as usize >= p).map(|(_, t)| t.le_bytes().len()).sum();
    eprintln!(
        "{name}: P {p} of {} params; parent {parent_leaves} leaves, adapter section {} leaves ({adapter_bytes} B); \
         admitted in {:?}, largest close {worst} B of {}",
        candidate.params.len(),
        split.adapter_leaves,
        started.elapsed(),
        rules.carriable
    );

    // The same over the chain's records, as the candidate's fold asks it: the composite; the layout
    // the record's digest names and no other; full weights only where the policy allows them.
    let (record, _) = palw_tir_class_record_v1(&class, &root).expect("the candidate's record");
    let (parent_record, _) = palw_tir_class_record_v1(&parent_class, &parent_root).expect("the parent's record");
    let facts = PalwTirCandidateFactsV1 {
        record: &record,
        artifact_root: root,
        layout: &class.layout,
        parent_class_id: parent_id,
        parent_record: &parent_record,
        parent_root,
        full_weights_allowed: false,
        rules,
    };
    assert!(palw_tir_candidate_artifact_admits_v1(&id, PalwTirCandidateArtifactV1::Composite(&r), &facts).is_ok(), "{name}");
    let mut other_layout = class.layout.clone();
    other_layout.h_tile += 1;
    let wrong = PalwTirCandidateFactsV1 { layout: &other_layout, ..facts };
    assert!(
        matches!(palw_tir_candidate_artifact_admits_v1(&id, PalwTirCandidateArtifactV1::Composite(&r), &wrong), Err(E::Admission(_))),
        "{name}: a layout other than the registered one"
    );
    let (full_root, _) = palw_tir_inventory_root_v1(&candidate, &Src(&mat_c)).expect("the candidate's whole inventory");
    let full_id = class.class_id(&full_root);
    let (full_record, _) = palw_tir_class_record_v1(&class, &full_root).expect("a full-weight record");
    let full = PalwTirCandidateFactsV1 { record: &full_record, artifact_root: full_root, ..facts };
    let single = PalwTirCandidateArtifactV1::Single(full_root);
    assert!(matches!(palw_tir_candidate_artifact_admits_v1(&full_id, single, &full), Err(E::Admission(_))), "{name}: no full weights");
    let allowed = PalwTirCandidateFactsV1 { full_weights_allowed: true, ..full };
    assert_eq!(palw_tir_candidate_artifact_admits_v1(&full_id, single, &allowed), Ok(Vec::new()), "{name}: full weights allowed");
    assert!(
        matches!(palw_tir_candidate_artifact_admits_v1(&id, single, &allowed), Err(E::NotTheClassId { .. })),
        "{name}: full weights under another class's id"
    );

    // Refusals, by name.
    let admit =
        |pc: &PalwTirClassV1, c: &PalwTirClassV1, r: &kaspa_consensus_core::palw_improve_composite_v1::PalwTirCompositeRefV1| {
            let root = r.artifact_root();
            palw_tir_composite_admits_v1(pc, &parent_id, &parent_root, c, &c.class_id(&root), &root, r, &rules).map(|_| ())
        };
    // A recalibrated parent scale: the parent section is no longer the parent's inventory.
    let mut recal = IntParams { tensors: mat_c.tensors.clone() };
    let (_, t) = recal
        .tensors
        .iter_mut()
        .find(|((j, _), t)| (*j as usize) < p && candidate.params[*j as usize].shape.len() <= 1 && !t.le_bytes().is_empty())
        .expect("a parent scale vector");
    match &mut t.data {
        IntData::I8(v) => v[0] = v[0].wrapping_add(1),
        IntData::I16(v) => v[0] = v[0].wrapping_add(1),
        IntData::I32(v) => v[0] = v[0].wrapping_add(1),
        IntData::I64(v) => v[0] = v[0].wrapping_add(1),
        IntData::Idx(v) => v[0] = v[0].wrapping_add(1),
    }
    let moved = palw_tir_composite_ref_v1(parent_id, &candidate, p32, &Src(&recal)).expect("sections");
    assert!(matches!(admit(&parent_class, &class, &moved), Err(E::NotTheParentRoot { .. })), "{name}: a recalibrated parent scale");
    // A split anywhere but the parent's params.
    let early = palw_tir_composite_ref_v1(parent_id, &candidate, p32 - 1, &Src(&mat_c)).expect("sections");
    let early = kaspa_consensus_core::palw_improve_composite_v1::PalwTirCompositeRefV1 { parent_root, ..early };
    assert!(matches!(admit(&parent_class, &class, &early), Err(E::NotTheParentsParams { .. })), "{name}: P - 1");
    // Another tokenizer.
    let other = class_of(&net, &candidate, Hash64::from_bytes([0x71; 64]));
    assert!(matches!(admit(&parent_class, &other, &r), Err(E::Family(_))), "{name}: another tokenizer");
    // The parent's params reordered, and an adapter param named as a parent tensor.
    let mut swapped = candidate.clone();
    swapped.params.swap(0, 1);
    assert!(matches!(palw_tir_composite_rule_v1(&parent, &swapped, p32), Err(E::ParentParamChanged { .. })), "{name}: reordered");
    let mut renamed = candidate.clone();
    renamed.params[p].name = parent.params[0].name.clone();
    assert!(matches!(palw_tir_composite_rule_v1(&parent, &renamed, p32), Err(E::AdapterNamesAParentTensor { .. })), "{name}: renamed");
    assert!(matches!(palw_tir_composite_rule_v1(&parent, &parent, p32), Err(E::NoAdapterSection(_))), "{name}: no adapter");
}

#[test]
fn llama_q_k_v_o_and_mlp_rank_16_is_a_composite_of_its_parent() {
    check("llama_r16");
}

#[test]
fn qwen2_all_linear_rank_4_is_a_composite_of_its_parent() {
    check("qwen2_all_r4");
}

#[test]
fn phi_rslora_rank_64_is_a_composite_of_its_parent() {
    check("phi_rs_r64");
}

#[test]
fn mistral_q_v_rank_8_is_a_composite_of_its_parent() {
    check("mistral_qv_r8");
}
