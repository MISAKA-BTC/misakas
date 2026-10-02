//! **The preflight's four gaps closed** (RFC-0002 Part II §II.2.9): `--node` (the facts a live node gives), the `full` depth
//! (`PACK_NOT_VERIFIED`, `ARTIFACT_ROOT_KNOWN`, `READY_SEATS_INSUFFICIENT`), the class-specific court window's reading, and a
//! repository read by HTTP ranges against a loopback server that serves a fixture directory (no hub is ever touched).

use misaka_palw_sdk::preflight::full::FullInputs;
use misaka_palw_sdk::preflight::node::{NodeClassFact, NodeFacts, NodeSeatingFact};
use misaka_palw_sdk::preflight::remote::HttpRangeFetcher;
use misaka_palw_sdk::preflight::{Depth, InputKind, Options, StageStatus, run, run_remote};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures").join(rel)
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("preflight-gaps-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch");
    d
}

/// A copy of a fixture directory with a tokenizer beside it.
fn copy_fixture(src: &Path, tag: &str) -> PathBuf {
    let dst = scratch(tag);
    std::fs::write(dst.join("tokenizer.json"), "{}").expect("tokenizer");
    for e in std::fs::read_dir(src).expect("fixture").flatten() {
        std::fs::write(dst.join(e.file_name()), std::fs::read(e.path()).expect("read")).expect("write");
    }
    dst
}

fn opts(depth: Depth) -> Options {
    Options { depth, network: Some("testnet-12".into()), ..Options::default() }
}

fn facts(tip: u64) -> NodeFacts {
    NodeFacts {
        network: "testnet-12".into(),
        tip_daa: tip,
        seat_count: 5,
        spare_seats: 2,
        bonds_with_headroom: 14,
        classes: vec![NodeClassFact {
            class_id: "aa".repeat(64),
            artifact_root: "bb".repeat(64),
            state: "Prefetching".into(),
            ready_seats: 3,
            required_ready_seats: 7,
            seating: Some(NodeSeatingFact {
                ready_operators: 3,
                needed_operators: 5,
                independent_operators: 1,
                needed_independent: 3,
                base_operators: 20,
                licensable_share_permille: 50,
            }),
        }],
    }
}

// ---------------------------------------------------------------------------------------------
// --node
// ---------------------------------------------------------------------------------------------

#[test]
fn a_node_gives_the_default_height_and_the_independence_the_network_has() {
    let dir = copy_fixture(&fixture("hf/llama"), "node");
    let r = run(&dir, &Options { node: Some(facts(7_150)), ..opts(Depth::Shape) }).expect("preflight");
    let net = r.network.as_ref().expect("network");
    assert!(net.daa_choice.contains("the node's tip"), "{}", net.daa_choice);
    assert!(net.daa >= 7_150 || net.what_if.is_some(), "judged at the tip (or the fence's own height): {} {:?}", net.daa, net.what_if);
    let node = r.node.as_ref().expect("the node's facts are in the report");
    assert_eq!((node.network.as_str(), node.tip_daa, node.classes, node.base_operators), ("testnet-12", 7_150, 1, Some(20)));
    let independence = r.forecast.as_ref().and_then(|f| f.independence.as_ref()).expect("the forecast reads the seating floor");
    assert_eq!((independence.independent_floor, independence.base_operators, independence.licensable_share_at_floor_permille), (3, Some(20), Some(150)));
    assert!(!independence.fence_in_force, "testnet-12 ships palw_class_seating dormant");
    assert!(r.render().contains("independence:"), "{}", r.render());
    // The node's own height wins over the default, a given one over the node's.
    let given = run(&dir, &Options { node: Some(facts(7_150)), height: Some(9_000), ..opts(Depth::Shape) }).expect("preflight");
    assert_eq!(given.network.as_ref().map(|n| n.daa_choice.clone()).as_deref(), Some("given (--height)"));
    // A node on another chain is refused: the conditions would be judged on the wrong one.
    let mut other = facts(10);
    other.network = "testnet-11".into();
    let err = run(&dir, &Options { node: Some(other), ..opts(Depth::Shape) }).unwrap_err();
    assert!(err.contains("--node is on testnet-11"), "{err}");
    // And the facts round-trip through JSON (what `--node-facts` reads).
    let text = serde_json::to_string(&facts(1)).expect("json");
    assert_eq!(NodeFacts::parse(&text).expect("parses"), facts(1));
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------------------------
// the full depth
// ---------------------------------------------------------------------------------------------

#[test]
fn the_full_depth_reads_the_pack_and_the_chains_view_of_the_artifact_root() {
    use misaka_palw_sdk::runtime_pack::{BuildOpts, build_pack};
    use misaka_palw_tir_lower::convert::{CalibInput, ConvertRequest};
    let work = scratch("full-work");
    let src = copy_fixture(&fixture("hf/llama"), "full-src");
    let pack_dir = work.join("pack");
    let mut req = ConvertRequest::new(&src, work.join("artifact.palwtir"));
    req.calib = Some(CalibInput {
        sequences: misaka_palw_tir_lower::fidelity::random_sequences(64, 3, 12, 11),
        source: serde_json::json!("random (seed 11)"),
    });
    let mut o = BuildOpts::new(req, &pack_dir, "fixture");
    o.prompts = 2;
    o.prefill = 5;
    o.decode = 2;
    let built = build_pack(&o, &|_| {}).unwrap_or_else(|e| panic!("build: {e}"));
    let root = built.pack.result.inventory_root.clone();
    let artifact = work.join("artifact.palwtir");

    // The pack, the artifact and the source: verified (or, when nothing failed but a check could not be made, not fully).
    let inputs = FullInputs { pack: Some(pack_dir.clone()), artifact: Some(artifact.clone()) };
    let r = run(&src, &Options { full: inputs.clone(), ..opts(Depth::Full) }).expect("preflight");
    assert_eq!(r.depth.reached, Depth::Full, "{:?}", r.depth);
    let full = r.full.as_ref().expect("the full depth's findings");
    assert_eq!(full.artifact_root.as_deref(), Some(root.as_str()));
    assert!(full.checks.iter().all(|c| c.status != "FAIL"), "{:?}", full.checks);
    let not_verified = r.blockers().iter().any(|b| b.code == "PACK_NOT_VERIFIED");
    assert_eq!(not_verified, full.pack != "VERIFIED", "PACK_NOT_VERIFIED is raised exactly when the pack is not VERIFIED ({})", full.pack);

    // A tampered artifact is a FAILED pack: named, with the failing check.
    let bad = work.join("bad.palwtir");
    let mut bytes = std::fs::read(&artifact).expect("artifact");
    let n = bytes.len();
    bytes[n - 1] ^= 1;
    std::fs::write(&bad, bytes).expect("write");
    let r = run(&src, &Options { full: FullInputs { pack: Some(pack_dir.clone()), artifact: Some(bad) }, ..opts(Depth::Full) }).expect("preflight");
    let b = r.blockers().into_iter().find(|b| b.code == "PACK_NOT_VERIFIED").expect("a tampered artifact is not verified");
    assert!(b.evidence.iter().any(|e| e.starts_with("FAIL")), "{:?}", b.evidence);

    // The node holds a class over this root, and one the pack declares with too few seats.
    let mut node = facts(7_150);
    node.classes[0].artifact_root = root.clone();
    let r = run(&src, &Options { full: inputs, node: Some(node), ..opts(Depth::Full) }).expect("preflight");
    let known = r.blockers().into_iter().find(|b| b.code == "ARTIFACT_ROOT_KNOWN").expect("the root is known on the chain");
    assert_eq!(known.stage, misaka_palw_sdk::preflight::Stage::Register);
    assert_eq!(r.verdict.register.status, StageStatus::Blocked);
    let _ = std::fs::remove_dir_all(work);
    let _ = std::fs::remove_dir_all(src);
}

#[test]
fn a_declared_class_already_on_the_chain_is_read_by_its_seats_and_its_independent_operators() {
    use misaka_palw_sdk::preflight::full::judge_full;
    use misaka_palw_sdk::runtime_pack::{BuildOpts, DeclareOpts, build_pack};
    use misaka_palw_sdk::tir_layout::TirLayoutChoiceV1;
    use misaka_palw_tir_lower::convert::{CalibInput, ConvertRequest};
    let work = scratch("declared-work");
    let src = copy_fixture(&fixture("hf/llama"), "declared-src");
    let pack_dir = work.join("pack");
    let mut req = ConvertRequest::new(&src, work.join("artifact.palwtir"));
    req.calib = Some(CalibInput {
        sequences: misaka_palw_tir_lower::fidelity::random_sequences(64, 3, 12, 11),
        source: serde_json::json!("random (seed 11)"),
    });
    let mut o = BuildOpts::new(req, &pack_dir, "fixture");
    o.prompts = 2;
    o.prefill = 5;
    o.decode = 2;
    o.declare = vec![DeclareOpts { network: "testnet-12".into(), choice: TirLayoutChoiceV1 { max_context: Some(256), ..TirLayoutChoiceV1::default() } }];
    let built = build_pack(&o, &|_| {}).unwrap_or_else(|e| panic!("build: {e}"));
    let declared = built.pack.declared[0].clone();
    // The node holds that class with 3 of 7 seats and 1 of 3 independent operators.
    let mut node = facts(7_150);
    node.classes[0].class_id = declared.class_id.clone();
    let (info, blockers) = judge_full(&FullInputs { pack: Some(pack_dir.clone()), artifact: None }, None, Some("testnet-12"), Some(&node));
    assert_eq!(info.declared, vec![("testnet-12".to_string(), declared.class_id)]);
    let codes: Vec<&str> = blockers.iter().map(|b| b.code.as_str()).collect();
    assert!(codes.contains(&"READY_SEATS_INSUFFICIENT") && codes.contains(&"INDEPENDENT_OPERATORS"), "{codes:?}");
    let seats = blockers.iter().find(|b| b.code == "READY_SEATS_INSUFFICIENT").expect("seats");
    assert_eq!((seats.have, seats.need), (Some(3), Some(7)));
    let ind = blockers.iter().find(|b| b.code == "INDEPENDENT_OPERATORS").expect("independent");
    assert_eq!((ind.have, ind.need), (Some(1), Some(3)));
    // On another network the declared class is not the node's: nothing is said of its seats.
    let (_, other) = judge_full(&FullInputs { pack: Some(pack_dir), artifact: None }, None, Some("testnet-11"), Some(&node));
    assert!(other.iter().all(|b| b.code != "READY_SEATS_INSUFFICIENT"), "{other:?}");
    let _ = std::fs::remove_dir_all(work);
    let _ = std::fs::remove_dir_all(src);
}

// ---------------------------------------------------------------------------------------------
// the class-specific court window
// ---------------------------------------------------------------------------------------------

#[test]
fn where_the_class_specific_window_is_dormant_the_report_says_what_it_would_be() {
    let dir = copy_fixture(&fixture("hf/llama"), "window");
    let r = run(&dir, &Options { network: Some("devnet".into()), max_context: Some(16_384), ..opts(Depth::Shape) }).expect("preflight");
    let c = r.chain.iter().find(|c| c.id == "court_window").expect("the court window condition");
    assert!(c.source.contains("palw_model_court_window is not in force"), "{}", c.source);
    assert!(
        r.notes.iter().any(|n| n.contains("under palw_model_court_window (dormant on this network) the class would be given its own window")),
        "{:?}",
        r.notes
    );
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------------------------
// a repository by HTTP ranges, against a loopback server
// ---------------------------------------------------------------------------------------------

struct Server {
    base: String,
    requests: Arc<AtomicU64>,
    ranged: Arc<AtomicU64>,
    bytes: Arc<AtomicU64>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

fn serve(dir: PathBuf) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.set_nonblocking(true).expect("nonblocking");
    let addr = listener.local_addr().expect("addr");
    let (requests, ranged, bytes) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (rq, rg, by, st) = (requests.clone(), ranged.clone(), bytes.clone(), stop.clone());
    std::thread::spawn(move || {
        while !st.load(Ordering::Relaxed) {
            let Ok((mut s, _)) = listener.accept() else {
                std::thread::sleep(std::time::Duration::from_millis(2));
                continue;
            };
            s.set_nonblocking(false).ok();
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            rq.fetch_add(1, Ordering::Relaxed);
            let path = req.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("/").trim_start_matches("/m/").to_string();
            let range = req
                .lines()
                .find_map(|l| l.to_ascii_lowercase().strip_prefix("range: bytes=").map(str::to_string))
                .and_then(|r| r.split_once('-').and_then(|(a, b)| Some((a.trim().parse::<u64>().ok()?, b.trim().parse::<u64>().ok()?))));
            match std::fs::read(dir.join(&path)) {
                Err(_) => {
                    let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                }
                Ok(file) => match range {
                    Some((a, b)) if (a as usize) < file.len() => {
                        rg.fetch_add(1, Ordering::Relaxed);
                        let end = (b as usize).min(file.len() - 1);
                        let body = &file[a as usize..=end];
                        by.fetch_add(body.len() as u64, Ordering::Relaxed);
                        let head = format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {a}-{end}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            file.len(),
                            body.len()
                        );
                        let _ = s.write_all(head.as_bytes());
                        let _ = s.write_all(body);
                    }
                    _ => {
                        by.fetch_add(file.len() as u64, Ordering::Relaxed);
                        let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", file.len());
                        let _ = s.write_all(head.as_bytes());
                        let _ = s.write_all(&file);
                    }
                },
            }
        }
    });
    Server { base: format!("http://{addr}/m"), requests, ranged, bytes, stop }
}

#[test]
fn a_repository_by_http_ranges_gives_the_directorys_report_and_fetches_only_headers() {
    let dir = copy_fixture(&fixture("hf/llama"), "remote");
    let server = serve(dir.clone());
    let local = run(&dir, &opts(Depth::Shape)).expect("local");
    let remote = run_remote(&server.base, &HttpRangeFetcher::default(), &opts(Depth::Shape)).expect("remote");
    server.stop.store(true, Ordering::Relaxed);
    // The same verdict, the same model, the same program: only the input's kind, label and the bytes read differ.
    assert_eq!(remote.input.kind, InputKind::Remote);
    assert_eq!(remote.input.label, server.base);
    assert_eq!(local.model.as_ref().map(|m| m.spec_digest.clone()), remote.model.as_ref().map(|m| m.spec_digest.clone()));
    assert_eq!(local.verdict, remote.verdict);
    assert_eq!(local.tensors.bound, remote.tensors.bound);
    assert_eq!(local.source.weight_bytes, remote.source.weight_bytes);
    assert_eq!(local.chain, remote.chain);
    assert_eq!(local.admission.as_ref().map(|a| a.verdict.clone()), remote.admission.as_ref().map(|a| a.verdict.clone()));
    // Only headers moved: the server saw ranged requests, and sent a fraction of the checkpoint.
    let weights = local.source.weight_bytes.expect("declared weight bytes");
    let sent = server.bytes.load(Ordering::Relaxed);
    assert!(server.ranged.load(Ordering::Relaxed) > 0, "range requests were made");
    assert!(sent < weights, "the server sent {sent} bytes of a checkpoint of {weights}");
    assert!(remote.input.bytes_read <= sent, "the report's bytes read are what was fetched: {} of {sent}", remote.input.bytes_read);
    assert!(server.requests.load(Ordering::Relaxed) >= 3);
    // The first line says which mode ran.
    assert!(remote.mode_line().starts_with("preflight: model · model repository (HTTP ranges)"), "{}", remote.mode_line());
    // A repository without a config.json is named, not guessed.
    let empty = scratch("remote-empty");
    let server = serve(empty.clone());
    let err = run_remote(&server.base, &HttpRangeFetcher::default(), &opts(Depth::Headers)).unwrap_err();
    server.stop.store(true, Ordering::Relaxed);
    assert!(err.contains("config.json: not found"), "{err}");
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(empty);
}
