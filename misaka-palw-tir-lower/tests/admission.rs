//! **`tir_admit_v1` on what this lowerer emits** (spec 04b §10.3): every HF tiny fixture and every
//! real configuration the lowerer accepts is admitted at the legacy court's ceilings — normal form,
//! ranges, per-position costs, every commit point's court cone and every `Fixed` state's
//! checkpoint interval — except DeepSeek-V3, which is refused by name: its position costs more
//! than `2^40` MACs at a `2^18` window. `tile_len` 64 and `h_chunk` 64 are the inputs (a layout
//! declares its own; admission's success does not depend on either for these programs).

use misaka_palw_tir::admit::{TirAdmitInputsV1, TirCeilingsV1, tir_admit_program_v1};
use misaka_palw_tir_lower::lower::{LowerOpts, lower};
use std::path::Path;

fn inputs() -> TirAdmitInputsV1 {
    TirAdmitInputsV1 { tile_len: 64, h_chunk: 64, ceilings: TirCeilingsV1::legacy_court_v1() }
}

fn admit(cfg: &str) -> Result<String, String> {
    let spec = misaka_palw_tir_lower::parse_config_str(cfg).map_err(|e| format!("not lowerable: {e}"))?;
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).map_err(|e| format!("not lowerable: {e}"))?;
    let lw = lower(&hl, &LowerOpts::default()).map_err(|e| format!("not lowerable: {e}"))?;
    let a = tir_admit_program_v1(&lw.program, &inputs()).map_err(|e| format!("REFUSED {e}"))?;
    let worst = a.cones.iter().max_by_key(|c| c.terminal().macs).expect("a cone");
    Ok(format!(
        "{} cones, worst terminal {} MACs ({}:{}), C {}, cone work {}, step leaves {}",
        a.cones.len(),
        worst.terminal().macs,
        worst.block,
        worst.node,
        a.checkpoint_interval,
        a.cone_work,
        a.position.step_leaves
    ))
}

#[test]
fn every_hf_tiny_fixture_is_admitted() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf");
    let mut names: Vec<String> =
        std::fs::read_dir(&dir).expect("fixtures").map(|e| e.expect("entry").file_name().to_string_lossy().to_string()).collect();
    names.sort();
    let mut refused = Vec::new();
    for n in &names {
        let cfg = std::fs::read_to_string(dir.join(n).join("config.json")).expect("config");
        match admit(&cfg) {
            Ok(s) => eprintln!("{n:>24}: {s}"),
            Err(e) => {
                eprintln!("{n:>24}: {e}");
                refused.push(n.clone());
            }
        }
    }
    assert_eq!(names.len(), 57);
    assert!(refused.is_empty(), "refused: {refused:?}");
}

#[test]
fn every_lowerable_real_configuration_is_admitted() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/configs/real");
    let mut names: Vec<String> =
        std::fs::read_dir(&dir).expect("configs").map(|e| e.expect("entry").file_name().to_string_lossy().to_string()).collect();
    names.sort();
    let mut refused = Vec::new();
    for n in &names {
        let cfg = std::fs::read_to_string(dir.join(n)).expect("config");
        match admit(&cfg) {
            Ok(s) => eprintln!("{n:>40}: {s}"),
            Err(e) if e.starts_with("not lowerable") => eprintln!("{n:>40}: {e}"),
            Err(e) => {
                eprintln!("{n:>40}: {e}");
                assert!(e.starts_with("REFUSED position MACs"), "{n}: {e}");
                refused.push(n.clone());
            }
        }
    }
    // 671B parameters at a 2^18-position window: past the per-position MACs (2^40) — refused by
    // name and number, as it should be; every other lowerable configuration is admitted.
    assert_eq!(refused, vec!["deepseek-v3-bf16.json".to_string()]);
}
