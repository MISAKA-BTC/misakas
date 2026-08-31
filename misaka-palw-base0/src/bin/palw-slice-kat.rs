//! **ADR-0067 Decision 6 tier ②: the validation artifact — try a class for megabytes before
//! fetching it for gigabytes.**
//!
//! A "validation artifact" is not a new container: it is an ordinary dense artifact CONVERTED
//! WITH `--layers N` (real weights, the model's own first N layers) plus this tool's KAT — a
//! known-answer test binding a token schedule to the bit-exact digest of every logits row the
//! slice produces. The publisher emits the KAT once; a candidate operator replays it on their
//! own machine and learns, before any large download, whether THIS build's kernels and
//! interpreter reproduce THIS family's arithmetic on real data. Cross-architecture bit-identity
//! is the family's measured property, so pass/fail is sharp.
//!
//! ```text
//! palw-slice-kat emit   --slice s.palwart --out kat.json \
//!     [--class-id HEX --artifact-root HEX --full-sha256 HEX --full-size N --source URL]
//! palw-slice-kat verify --slice s.palwart --kat kat.json
//! ```
//!
//! `verify` exits non-zero on any mismatch, naming the position. The optional identity fields
//! are the model card's verifiable facts carried machine-readably, so the same file that proves
//! runtime compatibility also says WHAT to fetch and how to check it once fetched.
//!
//! **What this deliberately does not license** (the ADR's own boundary): passing a slice KAT is
//! grounds to spend the bandwidth, not to declare capability. A seat's duty replays full jobs on
//! full weights; declaring on a slice is the documented `Incapable` trap.

use misaka_palw_base0::artifact::decode_artifact_file_v1;
use misaka_palw_base0::engine_a16::{A16Cache, A16Engine};

fn die(msg: String) -> ! {
    eprintln!("palw-slice-kat: {msg}");
    std::process::exit(1)
}

fn hex64(h: kaspa_hashes::Hash64) -> String {
    faster_hex::hex_string(h.as_byte_slice())
}

/// The schedule is fixed and derived from the slice's own vocab, so publisher and verifier
/// cannot disagree about it: 24 positions, tokens walking the vocab in a fixed stride.
fn schedule(vocab: usize) -> Vec<usize> {
    (0..24usize).map(|i| (i * 7 + 3) % vocab).collect()
}

/// One digest over every logits row, in order — the bit-exact answer the KAT pins.
fn rows_digest(rows: &[Vec<i32>]) -> kaspa_hashes::Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/slice-kat/v1").to_state();
    for row in rows {
        state.update(&(row.len() as u64).to_le_bytes());
        for v in row {
            state.update(&v.to_le_bytes());
        }
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    kaspa_hashes::Hash64::from_bytes(out)
}

fn run_slice(path: &str) -> (kaspa_hashes::Hash64, kaspa_hashes::Hash64, usize, usize) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| die(format!("{path}: {e}")));
    let artifact = decode_artifact_file_v1(&bytes).unwrap_or_else(|e| die(format!("{path}: {e}")));
    let engine = A16Engine::new(&artifact).unwrap_or_else(|e| die(format!("the slice is not an A16 artifact: {e:?}")));
    let mut cache = A16Cache::new(artifact.shape.n_layers);
    let tokens = schedule(artifact.shape.vocab);
    let mut rows = Vec::with_capacity(tokens.len());
    for (position, token) in tokens.iter().enumerate() {
        let row = engine.forward_token(&mut cache, *token, position).unwrap_or_else(|e| die(format!("position {position}: {e:?}")));
        rows.push(row);
    }
    (artifact.artifact_digest(), rows_digest(&rows), artifact.shape.n_layers, artifact.shape.vocab)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let mode = args.get(1).cloned().unwrap_or_default();
    let slice = flag("--slice").unwrap_or_else(|| die("--slice <file.palwart> is required".into()));

    match mode.as_str() {
        "emit" => {
            let out = flag("--out").unwrap_or_else(|| die("emit needs --out <kat.json>".into()));
            let (slice_digest, digest, layers, vocab) = run_slice(&slice);
            let doc = serde_json::json!({
                "schema": "misaka.palw.slice-kat.v1",
                "slice_layers": layers,
                "vocab": vocab,
                "slice_artifact_digest": hex64(slice_digest),
                "rows_digest": hex64(digest),
                // The full model's identity — the model card's verifiable facts, machine-readable.
                // Optional: a KAT without them still proves runtime compatibility.
                "class_id": flag("--class-id"),
                "artifact_root": flag("--artifact-root"),
                "full_sha256": flag("--full-sha256"),
                "full_size": flag("--full-size").and_then(|v| v.parse::<u64>().ok()),
                "source": flag("--source"),
            });
            std::fs::write(&out, serde_json::to_vec_pretty(&doc).unwrap()).unwrap_or_else(|e| die(format!("{out}: {e}")));
            println!("wrote {out}: {layers} layer(s), rows digest {}…", &hex64(digest)[..16]);
        }
        "verify" => {
            let kat_path = flag("--kat").unwrap_or_else(|| die("verify needs --kat <kat.json>".into()));
            let kat: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&kat_path).unwrap_or_else(|e| die(format!("{kat_path}: {e}"))))
                    .unwrap_or_else(|e| die(format!("{kat_path}: {e}")));
            let want = |k: &str| kat.get(k).and_then(|v| v.as_str()).map(str::to_string);
            let (slice_digest, digest, layers, _) = run_slice(&slice);
            if let Some(expected) = want("slice_artifact_digest")
                && expected != hex64(slice_digest)
            {
                die(format!("this is not the slice the KAT was emitted for (digest {}…)", &hex64(slice_digest)[..16]));
            }
            let expected = want("rows_digest").unwrap_or_else(|| die("the KAT carries no rows_digest".into()));
            if expected != hex64(digest) {
                die(format!(
                    "MISMATCH: this machine's rows digest {}… is not the KAT's {}… — this build does not reproduce the \
                     family's arithmetic; do not fetch, and do not declare",
                    &hex64(digest)[..16],
                    &expected[..16]
                ));
            }
            println!("OK: {layers}-layer slice reproduced bit-for-bit on this machine.");
            if let (Some(root), Some(sha)) = (want("artifact_root"), want("full_sha256")) {
                println!("fetch-worthy: full artifact_root {}…, sha256 {}…", &root[..16], &sha[..16]);
                if let Some(source) = want("source") {
                    println!("source: {source}");
                }
            }
            println!("note: a passing KAT licenses the FETCH, not a capability declaration (ADR-0067 Decision 6).");
        }
        other => die(format!("unknown mode {other:?} (emit | verify)")),
    }
}
