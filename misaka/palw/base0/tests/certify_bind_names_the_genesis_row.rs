//! **The certification tool, through its own argument parsing, names the class the genesis
//! registers.** Not "the two derivations agree" — that test existed (`classes::tests`) while
//! `palw-certify bind --artifact` took the other spelling — but the BINARY's output over the
//! bound dense artifact carries the graph-v5 512 row's id. Skipped, by name, when the artifact is
//! not on this machine; never a pass by absence.

use std::process::Command;

const BOUND_ARTIFACT: &str = "/private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet/71440f68-0f3b-4144-8b20-73c6aae7fb86/scratchpad/instruct-bound.palwart";
const SHIPPED_ARTIFACT: &str = "/Users/wata/Downloads/qwen25-1.5b-a16.palwart";

#[test]
fn bind_over_the_dense_artifact_names_the_genesis_graph_v5_row() {
    let artifact = [BOUND_ARTIFACT, SHIPPED_ARTIFACT].into_iter().find(|p| std::path::Path::new(p).is_file());
    let Some(artifact) = artifact else {
        eprintln!("SKIPPED: neither {BOUND_ARTIFACT} nor {SHIPPED_ARTIFACT} is on this machine — the tool-path check did not run");
        return;
    };
    let genesis = misaka_palw_base0::classes::a16_graph_v5_row_v1().expect("the genesis row");
    let expected = genesis.profile.shape_profile_id().to_string();
    let out_dir = std::env::temp_dir().join(format!("palw-certify-bind-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).expect("a scratch dir");
    let out = out_dir.join("bind.obj");
    let output = Command::new(env!("CARGO_BIN_EXE_palw-certify"))
        .args(["bind", "--artifact", artifact, "--lane", "fp", "--out"])
        .arg(&out)
        .output()
        .expect("palw-certify runs");
    let text = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(
        text.contains(&expected),
        "palw-certify bind --artifact {artifact} must name the genesis graph-v5 512 row {expected}; it printed:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&out_dir);
}
