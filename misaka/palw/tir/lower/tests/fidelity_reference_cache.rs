//! Bounded float-row caching must not change any measured fidelity value.
use std::process::Command;
#[test]
fn cached_and_streamed_reference_logits_have_identical_scores() {
    for name in ["llama", "gemma4"] {
        let model = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hf").join(name);
        let run = |cache: &str| {
            let out = Command::new(env!("CARGO_BIN_EXE_palw-tir-fidelity"))
                .arg(&model).args(["--positions", "8", "--calib-seqs", "2", "--eval-seqs", "2", "--exec", "--json",
                    "--attention-weight-bits", "16", "--mlp-down-weight-bits", "16", "--reference-cache-mib", cache])
                .output().unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()
        };
        let streamed = run("0");
        let cached = run("256");
        for key in ["metrics", "program_digest", "evaluation", "calibration", "weight_precision"] {
            assert_eq!(streamed[key], cached[key], "{name}: {key}");
        }
    }
}
