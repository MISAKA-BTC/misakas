//! **Generator of the G14 real-checkpoint registration fixtures** (lane D) — not a test of the SDK.
//!
//! `consensus/src/pipeline/virtual_processor/tests/g14_registration_e2e.rs` registers classes lowered from REAL local Hugging
//! Face checkpoints on the real consensus path. The consensus crate cannot depend on the SDK, so this program does the
//! lowering once and writes the class (program, layout, tokenizer id) to `tests/fixtures/g14/<name>.json`; the consensus test
//! reads the file and re-derives everything else (class id, canonical job, pwu) itself.
//!
//! **No weights are loaded**: the lowering is the preflight's shape-only one (the safetensors HEADER and `config.json`), so the
//! artifact root is a SYNTHETIC commitment (a keyed hash of the checkpoint's config and tokenizer bytes) — registration
//! never reads weights (the chain cannot), and a real inventory root needs the quantised artifact, which this lane does not
//! build. The fixture says so.
//!
//! The ruleset the layout is chosen under is exactly the consensus harness's: testnet-12 as launched with `palw_tir_v1` armed
//! from genesis (`Params` here = `palw_t12_launch_params_v1()` + `PalwTirFenceV1::testnet12_v1(0)`).
//!
//! Run (ignored by default; needs the checkpoints):
//! `G14_CKPT_ROOT=/Users/wata/Downloads/MISAKA-wt-b/hf-ckpt [G14_ONLY=<name>] [G14_CONTEXTS=1024,512,...] cargo test --offline -p misaka-palw-sdk --test g14_registration_fixture -- --ignored --nocapture`
use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_launch_params_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_hashes::Hash64;
use misaka_palw_sdk::preflight::{Options, model, source};
use misaka_palw_sdk::tir_layout::{TirLayoutChoiceV1, tir_choose_layout_judged_v1, tir_program_with_scheme_v1};
use std::path::PathBuf;

/// `G14_RULESET=shipped`: `palw_t12_shipped_params()` (every release fence on its shipped height, the preflight CLI's ruleset),
/// judged at DAA 5,585 where all are in force; default: the harness's (launch + `palw_tir_v1` from genesis).
fn shipped() -> bool {
    std::env::var("G14_RULESET").is_ok_and(|v| v == "shipped")
}

fn harness_params() -> Params {
    if shipped() {
        return kaspa_consensus_core::config::params::palw_t12_shipped_params();
    }
    let mut params = palw_t12_launch_params_v1();
    params.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(0)));
    params.sync_palw_tir_v1();
    params
}

/// The object the gate judges (the SDK preflight's `gate()`): the formula's canonical job, a placeholder bond and pricing.
fn probe_object(
    bundle: &kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2,
    class: &PalwTirClassV1,
    root: Hash64,
) -> Result<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2, String> {
    use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1};
    let program = class.decode_program().map_err(|e| e.to_string())?;
    let canonical = palw_tir_attempt_canonical_v1(class).ok_or("a context too narrow for a canonical job")?;
    let facts = PalwTirJobFactsV1::of(class, &program, class.class_id(&root));
    let bond = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
        kaspa_consensus_core::tx::TransactionId::from_bytes([0; 64]),
        0,
    ));
    kaspa_consensus_core::palw_tir_admission_v1::palw_tir_post_genesis_registration_v1(
        class.clone(),
        palw_tir_job_context_v1(&facts, canonical),
        root,
        0,
        u128::MAX,
        1,
        0,
        bond,
        Vec::new(),
        bundle.court.max_step_leaf_count(),
    )
    .map_err(|e| format!("{} ({e})", e.code()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
#[ignore]
fn write_g14_real_checkpoint_fixtures() {
    let root = PathBuf::from(std::env::var("G14_CKPT_ROOT").expect("G14_CKPT_ROOT names the hf-ckpt directory"));
    let mut out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus/src/pipeline/virtual_processor/tests/fixtures/g14");
    if shipped() {
        out = out.join("shipped");
    }
    std::fs::create_dir_all(&out).unwrap();
    let params = harness_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("testnet-12 is a V2 network") };
    let reg = misaka_palw_tir_lower::quantfmt::QuantRegistry::builtin();
    let models = [
        ("smollm2-1.7b", "HuggingFaceTB/SmolLM2-1.7B-Instruct"),
        ("mamba-370m", "state-spaces/mamba-370m-hf"),
        ("qwen3.5-0.8b", "Qwen/Qwen3.5-0.8B"),
        ("granite-3.1-1b-a400m", "ibm-granite/granite-3.1-1b-a400m-instruct"),
    ];
    let only = std::env::var("G14_ONLY").ok();
    let contexts: Vec<u32> = std::env::var("G14_CONTEXTS")
        .map(|v| v.split(',').map(|c| c.trim().parse().expect("a context")).collect())
        .unwrap_or_else(|_| vec![32_783, 16_384, 8_192, 4_096, 2_048, 1_024, 512, 256, 128, 64, 32, 16]);
    for (name, rel) in models {
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let dir = root.join(rel);
        let kind = source::detect(&dir).expect("a HF directory");
        let src = source::open(&dir, kind, None, reg).expect("the headers");
        let tokenizer_bytes = std::fs::read(dir.join("tokenizer.json")).unwrap_or_default();
        let tokenizer_id = Hash64::from_bytes(misaka_palw_tir_lower::artifact::tokenizer_id_of(&tokenizer_bytes));
        let config_bytes = std::fs::read(dir.join("config.json")).expect("config.json");
        let synthetic_root = {
            let mut st = blake2b_simd::Params::new().hash_length(64).key(b"g14-synthetic-artifact-root/v1").to_state();
            st.update(&config_bytes);
            st.update(&tokenizer_bytes);
            let mut b = [0u8; 64];
            b.copy_from_slice(st.finalize().as_bytes());
            Hash64::from_bytes(b)
        };
        // The widest context the harness's gate admits, searched downward (the program is lowered FOR a context: the history
        // window is bounded by it).
        let mut chosen = None;
        let mut last_refusal = String::new();
        for &ctx in &contexts {
            let opts = Options { max_context: Some(ctx), ..Options::default() };
            let analysis = model::analyze(&src, &opts, reg, None);
            let Some(program) = analysis.program.clone() else {
                last_refusal = format!("convert: {:?}", analysis.blockers.iter().map(|b| b.code.clone()).collect::<Vec<_>>());
                break;
            };
            let program = tir_program_with_scheme_v1(&program, None).expect("the tiled scheme");
            let leaves =
                analysis.artifact.as_ref().map(|a| a.inventory_leaves_estimate.min(u32::MAX as u64) as u32).unwrap_or(1 << 16).max(2);
            let choice = TirLayoutChoiceV1 { max_context: Some(ctx), ..Default::default() };
            let judge = |class: &PalwTirClassV1| -> Result<(), String> {
                if shipped() {
                    // The gate at DAA 5,585, where every fence of the schedule is in force.
                    use kaspa_consensus_core::palw_tir_admission_v1::palw_tir_registration_preflight_at_v1;
                    let object = probe_object(bundle, class, synthetic_root)?;
                    palw_tir_registration_preflight_at_v1(&params, bundle, &object, 5_585, &[])
                        .map(|_| ())
                        .map_err(|e| format!("{} ({e})", e.code()))?;
                    // The DA-answerability twin at the judged height (the preflight does the same; `composite: true` below skips the
                    // search's own, which reads the IR flag day's height).
                    misaka_palw_sdk::tir_layout::tir_canonical_job_answerable_at_v1(&params, class, 5_585)
                } else {
                    misaka_palw_sdk::tir_layout::tir_class_admission_offline_v1(&params, bundle, class, synthetic_root)
                }
            };
            match tir_choose_layout_judged_v1(&params, bundle, &program, tokenizer_id, leaves, &choice, shipped(), &judge) {
                Ok(c) if c.admission.is_ok() => {
                    let class = PalwTirClassV1 {
                        version: PALW_TIR_CLASS_VERSION_V1,
                        program: program.encode(),
                        layout: c.layout,
                        tokenizer_id,
                    };
                    chosen = Some((ctx, class));
                    break;
                }
                Ok(c) => last_refusal = format!("ctx {ctx}: {}", c.admission.unwrap_err()),
                Err(e) => last_refusal = format!("ctx {ctx}: {e}"),
            }
        }
        match chosen {
            Some((ctx, class)) => {
                let json = serde_json::json!({
                    "name": name,
                    "checkpoint": rel,
                    "generator": "misaka-palw-sdk/tests/g14_registration_fixture.rs",
                    "weights_loaded": false,
                    "artifact_root_synthetic": true,
                    "artifact_root_hex": hex(synthetic_root.as_byte_slice()),
                    "tokenizer_id_hex": hex(tokenizer_id.as_byte_slice()),
                    "max_context": ctx,
                    "program_borsh_hex": hex(&class.program),
                    "layout_borsh_hex": hex(&borsh::to_vec(&class.layout).unwrap()),
                    "class_id_hex": hex(class.class_id(&synthetic_root).as_byte_slice()),
                    "ruleset": if shipped() { "palw_t12_shipped_params, judged at DAA 5585" } else { "testnet-12 as launched, palw_tir_v1 armed from genesis" },
                });
                std::fs::write(out.join(format!("{name}.json")), serde_json::to_vec_pretty(&json).unwrap()).unwrap();
                eprintln!("{name}: admitted at context {ctx} ({} program bytes)", class.program.len());
            }
            None => eprintln!("{name}: no context admitted by the harness's gate ({last_refusal})"),
        }
    }
}
