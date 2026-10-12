//! `palw-evidence` end to end, as an operator would run it: a miner's local evidence directory (what the rail's `--evidence-out` writes) is
//! published to two reference providers, read back and verified; `status` observes them; one provider's chunk rots and `repair` mends it from the
//! other; `fetch` returns the same bytes the claim's own roots admit. Everything is local (127.0.0.1 and temp directories).

use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use misaka_palw_remote::evidence::{EvidenceManifestV1, fs, manifest_id_v1};
use misaka_palw_remote::transport::server;
use std::path::PathBuf;
use std::process::Command;

fn h(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("misaka-evidence-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> (i32, serde_json::Value, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_palw-evidence")).args(args).output().expect("the binary runs");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let json = text.lines().last().and_then(|l| serde_json::from_str(l).ok()).unwrap_or(serde_json::Value::Null);
    (out.status.code().unwrap_or(-1), json, String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn publish_status_repair_and_fetch_through_the_command_line() {
    let claim = "cd".repeat(64);
    let chunks = vec![vec![1u8; 3000], vec![2u8; 1500], vec![3u8; 9]];
    let manifest =
        EvidenceManifestV1::build(h(9), &TransactionOutpoint::new(h(0xB0), 1), &[5u8; 32], h(1), h(2), h(3), 3, 10_000, &chunks);
    let id = manifest_id_v1(&manifest);
    // The miner's local staging directory.
    let local = scratch("local");
    fs::publish(&local, &claim, &manifest, &chunks).unwrap();
    // Two providers on the network.
    let (dir_a, dir_b) = (scratch("a"), scratch("b"));
    let (a, b) = (
        server::start("127.0.0.1:0", dir_a.clone(), Default::default()).unwrap(),
        server::start("127.0.0.1:0", dir_b.clone(), Default::default()).unwrap(),
    );
    let providers = format!("{},{}", a.url(), b.url());
    // The claim's commitments, as the chain holds them.
    let roots = scratch("roots").join("claim.json");
    std::fs::write(
        &roots,
        serde_json::json!({
            "network_domain": h(9).to_string(), "trace_root": h(1).to_string(), "output_root": h(2).to_string(), "execution_root": h(3).to_string(),
            "trace_chunk_count": 3, "retention_deadline": 9_000
        })
        .to_string(),
    )
    .unwrap();
    let (roots, local) = (roots.to_str().unwrap().to_string(), local.to_str().unwrap().to_string());

    // publish: every copy read back and verified; safe to switch off only with enough of them.
    let (code, v, err) = run(&["publish", "--claim", &claim, "--from", &local, "--providers", &providers, "--min-verified", "2"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(v["verified_copies"], 2);
    assert_eq!(v["safe_to_switch_off"], true);
    // status: healthy with 2 copies, labelled as a local observation.
    let (code, v, _) =
        run(&["status", "--claim", &claim, "--roots", &roots, "--providers", &providers, "--min-copies", "2", "--now-daa", "5000"]);
    assert_eq!(code, 0);
    assert_eq!(v["copies_per_chunk"], serde_json::json!([2, 2, 2]));
    assert!(v["verdict"].as_str().unwrap().contains("Healthy"));
    assert!(v["observation"].as_str().unwrap().starts_with("LOCAL_OBSERVATION"));
    // a chunk rots on A; status notices; repair mends it from B.
    let rotten = fs::chunk_path(&dir_a, id, 1);
    let mut bytes = std::fs::read(&rotten).unwrap();
    bytes[0] ^= 0xFF;
    std::fs::write(&rotten, bytes).unwrap();
    let (_, v, _) = run(&["status", "--claim", &claim, "--roots", &roots, "--providers", &providers, "--min-copies", "2"]);
    assert_eq!(v["copies_per_chunk"], serde_json::json!([2, 1, 2]));
    let (code, v, _) = run(&["repair", "--claim", &claim, "--roots", &roots, "--providers", &providers]);
    assert_eq!(code, 0);
    assert_eq!(v["copied"], 1);
    let (_, v, _) = run(&["status", "--claim", &claim, "--roots", &roots, "--providers", &providers, "--min-copies", "2"]);
    assert_eq!(v["copies_per_chunk"], serde_json::json!([2, 2, 2]));
    // fetch: the executor's bytes, whoever served them.
    let out = scratch("out").join("material.bin");
    let (code, v, err) =
        run(&["fetch", "--claim", &claim, "--roots", &roots, "--providers", &providers, "--out", out.to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(std::fs::read(&out).unwrap(), chunks.concat());
    assert_eq!(v["bytes"], chunks.concat().len());
    // a claim whose roots the manifest does not match: nothing is fetched.
    let wrong = scratch("wrong").join("claim.json");
    std::fs::write(
        &wrong,
        serde_json::json!({
            "network_domain": h(9).to_string(), "trace_root": h(1).to_string(), "output_root": h(2).to_string(), "execution_root": h(0x44).to_string(),
            "trace_chunk_count": 3, "retention_deadline": 9_000
        })
        .to_string(),
    )
    .unwrap();
    let (code, _, err) = run(&[
        "fetch",
        "--claim",
        &claim,
        "--roots",
        wrong.to_str().unwrap(),
        "--providers",
        &providers,
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_ne!(code, 0);
    assert!(err.contains("agrees with the claim"), "{err}");
    // publish with too few providers to be safe: exit 2, and it says so.
    let (code, v, _) = run(&["publish", "--claim", &claim, "--from", &local, "--providers", &a.url(), "--min-verified", "2"]);
    assert_eq!(code, 2);
    assert_eq!(v["safe_to_switch_off"], false);
    a.stop();
    b.stop();
}

#[test]
fn a_miner_files_a_redemption_authorization_and_a_builder_mirrors_it_into_its_node_directory() {
    use kaspa_consensus_core::palw_receipt_v4::{
        PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT, PALW_RECEIPT_V4_BEACON_RULE_SLOT, PALW_RECEIPT_V4_VERSION, PalwRedemptionAuthBundleV4,
        PalwRedemptionAuthV4, redeem_auth_id_v4,
    };
    let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x42; 32]);
    let authorization = PalwRedemptionAuthV4 {
        version: PALW_RECEIPT_V4_VERSION,
        network_domain: h(9),
        claim_id: h(0x31),
        executor_bond: TransactionOutpoint::new(h(0xB0), 0),
        quantum_lo: 0,
        quantum_hi: 4,
        beacon_rule: PALW_RECEIPT_V4_BEACON_RULE_SLOT,
        builder_fee_bps: 500,
        expiry_daa: u64::MAX,
    };
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &kp.signing_key,
        redeem_auth_id_v4(&authorization).as_byte_slice(),
        PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT,
        [0u8; 32],
    )
    .unwrap()
    .as_ref()
    .to_vec();
    let pk: &[u8] = kp.verification_key.as_ref();
    let bundle = PalwRedemptionAuthBundleV4 { authorization, executor_pubkey: pk.to_vec(), signature }.encode();
    let file = scratch("rda-file").join("claim.rda4");
    std::fs::write(&file, &bundle).unwrap();
    let (dir, node_dir) = (scratch("rda-provider"), scratch("rda-node"));
    let provider = server::start("127.0.0.1:0", dir, Default::default()).unwrap();
    let (code, v, err) = run(&["redemption-publish", "--file", file.to_str().unwrap(), "--providers", &provider.url()]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(v["filed_with"], 1);
    let (code, v, err) =
        run(&["redemption-sync", "--providers", &provider.url(), "--into", node_dir.to_str().unwrap(), "--now-daa", "100"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(v["written"].as_array().unwrap().len(), 1);
    let name = v["written"][0]["file"].as_str().unwrap();
    assert!(name.starts_with(&h(0x31).to_string()) && name.ends_with(".rda4"));
    assert_eq!(std::fs::read(node_dir.join(name)).unwrap(), bundle, "byte for byte what the miner signed");
    // An unsound bundle is refused by the publisher before any provider is asked.
    let mut forged = bundle.clone();
    let n = forged.len();
    forged[n - 3] ^= 1;
    let bad = scratch("rda-bad").join("forged.rda4");
    std::fs::write(&bad, forged).unwrap();
    let (code, _, _) = run(&["redemption-publish", "--file", bad.to_str().unwrap(), "--providers", &provider.url()]);
    assert_eq!(code, 2);
    provider.stop();
}
