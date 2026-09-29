//! **RFC-0004 A11: the tools around a LoRA candidate** — the container `palw-tir-fidelity --adapter`
//! writes, `palw-class composite` over it and its parent's, and `check-architecture --lora-budget`.
//!
//! * The candidate is lowered, calibrated and materialised exactly as the converter does it
//!   (`fidelity::prepare_candidate`, the parent's statistics plus the adapter's sites through
//!   `fidelity::candidate_stats`), and both containers are written as the converter writes them.
//! * `tir_composite_derive_v1` over the two files must give the roots rfc4/cand's consensus
//!   functions give over the materialised tensors: the parent root is the parent's inventory root,
//!   and the adapter root is `palw_tir_composite_ref_v1`'s. The composite artifact root under a
//!   parent class is the reference's.
//! * The record written into the candidate's provenance reads back, in the agreed format.
//! * A candidate whose parent tensors were recalibrated, one with no recorded `P`, and one under
//!   another tokenizer are each refused.
//! * The LoRA budget lists every target of a llama, fits them all, and sizes each composite under
//!   the cap. On Phi-3 it refuses the fused projections by name.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_improve_composite_v1::{PalwTirCompositeRefV1, palw_tir_composite_ref_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_root_v1};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_hashes::Hash64;
use misaka_palw_sdk::check_architecture::{LoraBudgetChoiceV1, check_lora_budget_v1};
use misaka_palw_sdk::tir_composite::{TirCompositeV1, tir_composite_derive_v1, tir_composite_p_v1, tir_composite_write_v1};
use misaka_palw_tir_artifact::PalwTirContainerV1;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::float_ref::{ParamStore, SiteStat};
use misaka_palw_tir_lower::lower::{IntParams, LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::{Checkpoint, Overlay};
use misaka_palw_tir_lower::{artifact, fidelity};

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

/// A scratch directory of its own, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("palw-composite-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// What the converter produces for a parent and its candidate.
struct Converted {
    parent_program: misaka_palw_tir::TirProgramV1,
    parent_params: IntParams,
    candidate_program: misaka_palw_tir::TirProgramV1,
    candidate_params: IntParams,
    /// The candidate materialised with its OWN calibration at every site (not the parent's).
    recalibrated: IntParams,
    p: usize,
}

/// The parent and the candidate, as `palw-tir-fidelity` and `palw-tir-fidelity --adapter
/// --parent-stats` convert them.
fn converted(adapter: &str) -> Converted {
    let ad_dir = fixtures().join("hf-lora").join(adapter);
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(ad_dir.join("logits.json")).unwrap()).unwrap();
    let base_dir = fixtures().join("hf").join(meta["base"].as_str().unwrap());
    let cfg = std::fs::read_to_string(base_dir.join("config.json")).unwrap();
    let opts = LowerOpts { max_window: Some(64), ..LowerOpts::default() };
    let parent = fidelity::prepare(&cfg, &opts).expect("the parent");
    let (cand, p) = fidelity::prepare_candidate(&parent, &std::fs::read_to_string(ad_dir.join("adapter_config.json")).unwrap(), &opts)
        .expect("the candidate");
    let ck = Checkpoint::open(&base_dir).expect("checkpoint");
    let ad = Checkpoint::open(&ad_dir.join("adapter_model.safetensors")).expect("adapter");
    let (pf, _) = ParamStore::from_source(&parent.hl, &parent.binding, &ck).expect("parent params");
    let (cf, _) = ParamStore::from_source(&cand.hl, &cand.binding, &Overlay { base: &ck, over: &ad }).expect("candidate params");
    let calib = fidelity::random_sequences(parent.hl.vocab, 4, 32, 11);
    let quiet = |_: usize, _: usize| {};
    let (lp, lc) = (Resident(Arc::new(pf)), Resident(Arc::new(cf)));
    let stats_p = fidelity::calibrate(&parent.hl, &lp, &calib, &quiet).expect("parent calibration");
    let mat_p = materialise(&parent.lowered, &parent.hl, &lp, &stats_p, &QuantPolicy::default(), &quiet).expect("parent artifact");
    let own: BTreeMap<String, SiteStat> = fidelity::calibrate(&cand.hl, &lc, &calib, &quiet).expect("candidate calibration");
    let stats = fidelity::candidate_stats(&stats_p, own.clone());
    let mat_c = materialise(&cand.lowered, &cand.hl, &lc, &stats, &QuantPolicy::default(), &quiet).expect("candidate artifact");
    let recal = materialise(&cand.lowered, &cand.hl, &lc, &own, &QuantPolicy::default(), &quiet).expect("recalibrated candidate");
    Converted {
        parent_program: parent.lowered.program.clone(),
        parent_params: mat_p.params,
        candidate_program: cand.lowered.program.clone(),
        candidate_params: mat_c.params,
        recalibrated: recal.params,
        p,
    }
}

/// The converter's provenance for a candidate (`palw-tir-fidelity --adapter`), or a parent's.
fn meta(p: Option<usize>) -> serde_json::Value {
    let mut m = serde_json::json!({ "architecture": "test", "calibrated_context": 32, "max_window": 64 });
    if let Some(p) = p {
        m["composite"] = serde_json::json!({ "p": p });
        m["adapter"] = serde_json::json!({ "config": { "peft_type": "LORA" } });
    }
    m
}

fn check(adapter: &str) {
    let c = converted(adapter);
    let dir = Scratch::new(adapter);
    let tokenizer = [0x61u8; 64];
    let (pp, cp) = (dir.0.join("parent.palwtir"), dir.0.join("candidate.palwtir"));
    artifact::write(&pp, &c.parent_program, &c.parent_params, tokenizer, meta(None)).expect("parent container");
    artifact::write(&cp, &c.candidate_program, &c.candidate_params, tokenizer, meta(Some(c.p))).expect("candidate container");
    let pc = PalwTirContainerV1::open(&pp).unwrap();
    let cc = PalwTirContainerV1::open(&cp).unwrap();
    assert_eq!(tir_composite_p_v1(&cc), Ok(c.p as u32), "{adapter}: the recorded P");

    // The derivation over the files is the consensus functions' over the tensors.
    let d = tir_composite_derive_v1(&pc, &cc, None).unwrap_or_else(|e| panic!("{adapter}: {e}"));
    let (parent_root, parent_leaves) = palw_tir_inventory_root_v1(&c.parent_program, &Src(&c.parent_params)).unwrap();
    let parent_class = Hash64::from_bytes([0x3c; 64]);
    let r = palw_tir_composite_ref_v1(parent_class, &c.candidate_program, c.p as u32, &Src(&c.candidate_params)).unwrap();
    assert_eq!((d.p, d.parent_root, d.parent_leaves), (c.p as u32, parent_root, parent_leaves), "{adapter}: the parent section");
    assert_eq!(d.adapter_root, r.adapter_root, "{adapter}: the adapter section's root");
    assert!(d.adapter_leaves > 0 && d.parent_class.is_none() && d.artifact_root().is_none(), "{adapter}");
    let with_class = tir_composite_derive_v1(&pc, &cc, Some(parent_class)).unwrap();
    assert_eq!(
        with_class.artifact_root(),
        Some(PalwTirCompositeRefV1 { parent_root, ..r }.artifact_root()),
        "{adapter}: the artifact root"
    );

    // The record, written into the candidate's provenance, reads back; nothing else moved.
    for (composite, name) in [(d, "plain.palwtir"), (with_class, "classed.palwtir")] {
        let out = dir.0.join(name);
        tir_composite_write_v1(&cc, &composite, &out).expect("written");
        let oc = PalwTirContainerV1::open(&out).unwrap();
        let m: serde_json::Value = serde_json::from_str(&oc.header.meta).unwrap();
        let rec = &m["composite"];
        assert_eq!(TirCompositeV1::from_meta(rec), Ok(composite), "{adapter}: the record round-trips");
        assert_eq!(rec.get("parent_class").is_some(), composite.parent_class.is_some(), "{adapter}: parent_class only when given");
        for k in ["parent_root", "adapter_root"] {
            let h = rec[k].as_str().unwrap();
            assert!(h.len() == 128 && h.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()), "{adapter}: `{k}` {h}");
        }
        assert_eq!(m["adapter"], meta(Some(c.p))["adapter"], "{adapter}: the rest of the provenance is kept");
        assert_eq!(
            (oc.program.clone(), oc.header.tokenizer_id, oc.header.layout.clone()),
            (cc.program.clone(), tokenizer, Vec::new())
        );
        assert_eq!(
            tir_composite_derive_v1(&pc, &oc, composite.parent_class),
            Ok(composite),
            "{adapter}: the written file derives the same"
        );
    }
    assert!(tir_composite_write_v1(&cc, &d, &cp).is_err(), "{adapter}: never over the file it reads");

    // Refusals.
    let bad = dir.0.join("recalibrated.palwtir");
    artifact::write(&bad, &c.candidate_program, &c.recalibrated, tokenizer, meta(Some(c.p))).unwrap();
    let e = tir_composite_derive_v1(&pc, &PalwTirContainerV1::open(&bad).unwrap(), None).unwrap_err();
    assert!(e.contains("--parent-stats"), "{adapter}: a recalibrated parent section: {e}");
    let bare = dir.0.join("bare.palwtir");
    artifact::write(&bare, &c.candidate_program, &c.candidate_params, tokenizer, meta(None)).unwrap();
    let e = tir_composite_derive_v1(&pc, &PalwTirContainerV1::open(&bare).unwrap(), None).unwrap_err();
    assert!(e.contains("composite.p"), "{adapter}: no recorded P: {e}");
    let other = dir.0.join("other-tokenizer.palwtir");
    artifact::write(&other, &c.candidate_program, &c.candidate_params, [0x62; 64], meta(Some(c.p))).unwrap();
    let e = tir_composite_derive_v1(&pc, &PalwTirContainerV1::open(&other).unwrap(), None).unwrap_err();
    assert!(e.contains("tokenizer"), "{adapter}: another tokenizer: {e}");
    let own = dir.0.join("parent-as-candidate.palwtir");
    artifact::write(&own, &c.parent_program, &c.parent_params, tokenizer, meta(Some(c.p))).unwrap();
    let e = tir_composite_derive_v1(&pc, &PalwTirContainerV1::open(&own).unwrap(), None).unwrap_err();
    assert!(e.contains("no adapter section"), "{adapter}: the parent itself: {e}");
    eprintln!(
        "{adapter}: P {} of {}; parent {} leaves, adapter section {} leaves",
        c.p,
        c.candidate_program.params.len(),
        d.parent_leaves,
        d.adapter_leaves
    );
}

#[test]
fn a_llama_rank_16_candidate_container_is_a_composite_of_its_parents() {
    check("llama_r16");
}

#[test]
fn a_qwen2_all_linear_candidate_container_is_a_composite_of_its_parents() {
    check("qwen2_all_r4");
}

fn budget(model: &str) -> misaka_palw_sdk::check_architecture::LoraBudgetReportV1 {
    let net = params();
    let PalwConsensusMode::ConsensusV2(bundle) = &net.palw_consensus_mode else { panic!("testnet-12 is a V2 network") };
    let cfg = std::fs::read_to_string(fixtures().join("hf").join(model).join("config.json")).unwrap();
    let choice = LoraBudgetChoiceV1 { context: Some(64), ..LoraBudgetChoiceV1::default() };
    let r = check_lora_budget_v1(&net, bundle, &cfg, &choice).unwrap_or_else(|e| panic!("{model}: {e}"));
    eprint!("{}", r.render());
    r
}

#[test]
fn the_lora_budget_of_a_llama_fits_every_target() {
    let r = budget("llama");
    let names: Vec<&str> = r.rows.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(
        names,
        ["q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj", "attention", "mlp", "all-linear"],
        "every target alone, the two sets, all-linear, and no fallback"
    );
    assert!(r.parent.as_ref().is_ok_and(|s| s.work <= r.work_cap && s.carried.is_ok()), "the parent is sized: {:?}", r.parent);
    let layer = r.blocks.iter().position(|(_, n, _)| *n > 1).expect("a layer block");
    for x in &r.rows {
        assert!(x.admissible(r.work_cap), "{}: {:?} {:?}", x.name, x.refused, x.sized);
        assert!(x.adapter_params > 0 && x.adapter_bytes > 0, "{}", x.name);
        assert!(x.block_nodes[layer] > r.blocks[layer].2, "{}: the adapter adds nodes to the layer", x.name);
        assert!(x.sized.as_ref().unwrap().work >= r.parent.as_ref().unwrap().work, "{}: an adapter adds closes", x.name);
    }
    let all = r.rows.iter().find(|x| x.name == "all-linear").unwrap();
    let single_max = r.rows.iter().take(7).map(|x| x.block_nodes[layer]).max().unwrap();
    assert!(all.block_nodes[layer] > single_max && all.adapter_params > r.rows[0].adapter_params, "all-linear is the widest");
    let json = r.to_json();
    assert_eq!(json["rows"].as_array().unwrap().len(), r.rows.len());
    assert_eq!(json["max_nodes_per_block"], 512);
}

#[test]
fn the_lora_budget_names_a_fused_projection() {
    let r = budget("phi3_longrope");
    for x in &r.rows {
        match x.name.as_str() {
            "qkv_proj" | "gate_up_proj" => {
                assert!(x.refused.as_deref().is_some_and(|w| w.contains("fused")), "{}: {:?}", x.name, x.refused)
            }
            "all-linear" => assert!(x.refused.is_some(), "all-linear over fused projections is refused"),
            "fallback" => assert!(x.admissible(r.work_cap) && !x.targets.is_empty(), "the unfused targets remain: {:?}", x),
            _ => assert!(x.admissible(r.work_cap), "{}: {:?} {:?}", x.name, x.refused, x.sized),
        }
    }
    assert!(r.rows.iter().any(|x| x.name == "o_proj"), "the unfused o_proj is a target");
}
