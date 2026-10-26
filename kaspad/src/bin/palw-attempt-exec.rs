//! Official PALW attempt executor. Reads `PALW_ANCHOR` (128 hex) and prints the roots
//! `kaspad --palw-produce` would commit for that anchor, plus a material file.
//!
//! ```text
//! PALW_ANCHOR=<128 hex> palw-attempt-exec --material-out capture.bin
//! PALW_ANCHOR=<128 hex> palw-attempt-exec --class <8k class> --artifact-root <inventory root> \
//!     --artifact /path/qwen25-1.5b-a16-8k.palwart --material-out capture.bin
//! ```
//!
//! The floor (`PALW-BASE-0`) is the default and needs no artifact. An 8k draw needs the same
//! container the public producer holds.

use std::path::PathBuf;

use kaspa_hashes::Hash64;

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("[palw-attempt-exec] fatal: {msg}");
    std::process::exit(1);
}

fn main() {
    let mut network = "testnet-12".to_string();
    let mut class: Option<String> = None;
    let mut artifact_root: Option<String> = None;
    let mut artifacts: Vec<PathBuf> = Vec::new();
    let mut material_out: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = |name: &str| args.next().unwrap_or_else(|| die(format!("{name} needs a value")));
        match flag.as_str() {
            "--network" => network = value("--network"),
            "--class" => class = Some(value("--class")),
            "--artifact-root" => artifact_root = Some(value("--artifact-root")),
            "--artifact" => artifacts.push(PathBuf::from(value("--artifact"))),
            "--material-out" => material_out = Some(PathBuf::from(value("--material-out"))),
            "--help" | "-h" => {
                eprintln!(
                    "palw-attempt-exec [--network testnet-12] [--class HEX] [--artifact-root HEX] \
                     [--artifact PATH]... --material-out PATH\nPALW_ANCHOR must be set (128 hex)."
                );
                return;
            }
            other => die(format!("unknown flag {other}")),
        }
    }
    let anchor_text = std::env::var("PALW_ANCHOR").unwrap_or_else(|_| die("PALW_ANCHOR is not set"));
    let anchor: Hash64 = anchor_text.trim().parse().unwrap_or_else(|_| die("PALW_ANCHOR is not a 128-hex hash"));
    let material_out = material_out.unwrap_or_else(|| die("--material-out is required"));
    let params = kaspad_lib::palw_attempt_exec::params_for_network_v1(&network).unwrap_or_else(|e| die(e));
    let (floor_class, floor_root) = kaspad_lib::palw_attempt_exec::floor_class_and_root_v1(&params).unwrap_or_else(|e| die(e));
    let class_id =
        class.as_deref().map(|s| s.parse::<Hash64>().unwrap_or_else(|_| die("--class is not a 128-hex hash"))).unwrap_or(floor_class);
    let root = artifact_root
        .as_deref()
        .map(|s| s.parse::<Hash64>().unwrap_or_else(|_| die("--artifact-root is not a 128-hex hash")))
        .unwrap_or(floor_root);
    let outcome = kaspad_lib::palw_attempt_exec::execute_producer_anchor_v1(&params, class_id, root, &artifacts, anchor)
        .unwrap_or_else(|e| die(e));
    if let Some(parent) = material_out.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| die(format!("cannot create {}: {e}", parent.display())));
    }
    std::fs::write(&material_out, &outcome.material).unwrap_or_else(|e| die(format!("cannot write {}: {e}", material_out.display())));
    println!(
        "{}",
        serde_json::json!({
            "trace_root": outcome.trace_root.to_string(),
            "output_root": outcome.output_root.to_string(),
            "execution_root": outcome.execution_root.to_string(),
            "trace_manifest_root": outcome.trace_manifest_root.to_string(),
            "trace_chunk_count": outcome.trace_chunk_count,
            "material_file": material_out.display().to_string(),
        })
    );
}
