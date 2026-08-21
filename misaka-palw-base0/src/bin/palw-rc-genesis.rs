//! **The PALW-RC genesis card: every constant a testnet-12 binary must ship, printed once.**
//!
//! Road-map Gate 4 calls the genesis artifact "the one input code cannot mint". Half of that is no
//! longer true — the floor's `artifact_root` is a derivation now (`misaka_palw_base0::rc`), so this
//! prints it rather than asking for it. The other half is still exactly true and always will be:
//! **which premine output backs the genesis bond, and which ML-DSA-87 keys sign under it.** Those
//! are operator facts. This tool takes them and produces the constants, so a human pastes hashes
//! rather than re-typing parameters — ADR-0042 Decision 11's own requirement.
//!
//! ```text
//! cargo run -p misaka-palw-base0 --bin palw-rc-genesis -- \
//!     --bond-index 0 \
//!     --bond-pubkey <hex> \
//!     --operator-pubkey <hex> \
//!     --payout-payload <64-byte hex>
//! ```
//!
//! Run with no arguments it prints what it CAN derive — the class id, the artifact root, the
//! geometry — and says which three facts it is waiting on. That is deliberate: a tool that
//! invented a key would be minting an identity, and the whole point of the bond is that somebody
//! holds one.
//!
//! **It generates no keys and touches no key material.** `misaka-cli` owns that, and an operator
//! who wants a fresh bond key makes it there, keeps the secret, and passes only the verification
//! key here.

use kaspa_consensus_core::palw_base0_profile::{
    PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, palw_rc_base0_registration_v1,
};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_hashes::Hash64;
use misaka_palw_base0::rc::{PALW_RC_BASE0_SEED, palw_rc_base0_artifact_root_v1};

fn arg(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == name {
            return args.next();
        }
        if let Some(rest) = a.strip_prefix(&format!("{name}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

fn hex_bytes(s: &str) -> Option<Vec<u8>> {
    let s = s.trim().trim_start_matches("0x");
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()).collect()
}

fn main() {
    let geometry = PALW_RC_BASE0_GEOMETRY;
    let profile = base0_profile_v1(geometry).expect("the floor's graph is expressible");
    let artifact_root = palw_rc_base0_artifact_root_v1().expect("the floor's artifact derives");

    println!("PALW-RC (testnet-12) genesis card");
    println!("=================================");
    println!();
    println!("Derived — identical on every machine, nothing to host or mirror:");
    println!("  base0 seed          0x{PALW_RC_BASE0_SEED:016X}");
    println!(
        "  geometry            {} layers, hidden {}, ffn {}, heads {}x{}, vocab {}, n_ctx {}, tile {}",
        geometry.layer_count,
        geometry.hidden_dim,
        geometry.ffn_dim,
        geometry.attn_heads,
        geometry.attn_head_dim,
        geometry.vocab_size,
        geometry.n_ctx,
        geometry.tile_len,
    );
    println!("  execution_class_id  {}", profile.shape_profile_id());
    println!("  artifact_root       {artifact_root}");
    println!();

    // **What one block costs.** A `ConsensusV2` block is one inference plus a free nonce grind, so
    // this number and the target block time are the same question: if the job does not fit inside
    // the cadence, the network cannot hold it however fast the hashing is. Measured rather than
    // asserted, because the floor's geometry is the one thing an operator may not change.
    if arg("--skip-cost").is_none() {
        let (job, prompt) = misaka_palw_base0::produce::base0_rc_job_v1(
            &profile,
            Hash64::from_u64_word(0x5041_4C57_5F43_4F53),
            geometry.vocab_size as usize,
            PALW_RC_BASE0_CANONICAL.0,
            PALW_RC_BASE0_CANONICAL.1,
        );
        let artifact = misaka_palw_base0::rc::palw_rc_base0_artifact_v1().expect("the floor's artifact derives");
        let leaves = kaspa_consensus_core::palw_step::step_leaf_count(&profile, &job).expect("the job has a step space");
        let started = std::time::Instant::now();
        match misaka_palw_base0::produce::base0_execute_for_attempt_v1(&artifact, &profile, &job, &prompt) {
            Ok(run) => {
                let elapsed = started.elapsed();
                println!("One block's work — canonical job ({} prefill, {} decode):", PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
                println!("  step leaves         {leaves}");
                println!("  wall time           {:.3} s  (one inference per template; the nonce grind is free)", elapsed.as_secs_f64());
                println!("  execution_root      {}", run.execution_root);
                println!("  output tokens       {:?}", run.generated_token_ids);
            }
            Err(e) => println!("One block's work: REFUSED — {e}"),
        }
        println!();
    }

    let bond_index: Option<u32> = arg("--bond-index").and_then(|v| v.parse().ok());
    let bond_pubkey = arg("--bond-pubkey").and_then(|v| hex_bytes(&v));
    let operator_pubkey = arg("--operator-pubkey").and_then(|v| hex_bytes(&v));
    let payout: Option<Hash64> = arg("--payout-payload").and_then(|v| hex_bytes(&v)).and_then(|b| {
        let bytes: [u8; 64] = b.try_into().ok()?;
        Some(Hash64::from_bytes(bytes))
    });

    let (Some(bond_index), Some(bond_pubkey), Some(operator_pubkey), Some(payout)) =
        (bond_index, bond_pubkey, operator_pubkey, payout)
    else {
        println!("Waiting on the three facts code cannot mint:");
        println!("  --bond-index      which premine output backs the genesis bond (0..=40)");
        println!("  --bond-pubkey     the ML-DSA-87 VERIFICATION key that signs attempts under it (hex)");
        println!("  --operator-pubkey the operator identity key (hex) — panel dedup is keyed on it");
        println!("  --payout-payload  the 64-byte P2PKH-ML-DSA-87 owner payload matured rewards are paid to (hex)");
        println!();
        println!("Generate the keys with misaka-cli and keep the secrets there; pass only the");
        println!("verification keys here. A tool that minted a key would be minting an identity.");
        return;
    };

    let bond = PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(bond_index));
    match kaspa_consensus_core::config::params::palw_rc_params_from_artifacts(
        artifact_root,
        bond,
        bond_pubkey.clone(),
        operator_pubkey.clone(),
        payout,
    ) {
        Err(e) => {
            println!("REFUSED: {e}");
            println!();
            println!("This is the genesis gate answering, not a tool failure. The commonest cause is a");
            println!("bond index whose premine output does not cover the bundle's collateral floor.");
            std::process::exit(1);
        }
        Ok(params) => {
            let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
                unreachable!("palw_rc_params installs a ConsensusV2 bundle or returns Err")
            };
            let (_, catalog) = palw_rc_base0_registration_v1(artifact_root).expect("the RC registration derives");
            println!("ACCEPTED — every gate the genesis loader runs has passed.");
            println!();
            println!("  network             {}", params.net);
            println!("  genesis hash        {}", params.genesis.hash);
            println!("  bond outpoint       premine #{bond_index}");
            println!("  class_catalog_root  {}", catalog.root());
            println!("  court_catalog_root  {}", bundle.court_catalog_root);
            println!("  palw_ruleset_id     {}", kaspa_consensus_core::palw_mode_v2::palw_ruleset_id_v2(bundle));
            println!("  consensus_params_id {}", params.consensus_params_id());
            println!();
            println!("Paste into `consensus/core/src/config/params.rs`:");
            println!();
            println!(
                "pub const PALW_RC_GENESIS_ARTIFACT_ROOT: Hash64 = Hash64::from_bytes({});",
                rust_bytes(artifact_root.as_byte_slice())
            );
            println!("pub const PALW_RC_GENESIS_BOND_INDEX: u32 = {bond_index};");
            println!("pub const PALW_RC_GENESIS_BOND_PUBKEY: &[u8] = &{};", rust_bytes(&bond_pubkey));
            println!("pub const PALW_RC_GENESIS_OPERATOR_PUBKEY: &[u8] = &{};", rust_bytes(&operator_pubkey));
            println!("pub const PALW_RC_GENESIS_PAYOUT_PAYLOAD: Hash64 = Hash64::from_bytes({});", rust_bytes(payout.as_byte_slice()));
            println!();
            println!("Every node must ship the SAME five values. They are inside consensus_params_id,");
            println!("so a node that ships different ones is refused at the handshake rather than");
            println!("forking — which is the check, not a reason to be casual about the paste.");
        }
    }
}

/// A Rust byte-array literal, wrapped so a paste is readable.
fn rust_bytes(bytes: &[u8]) -> String {
    let mut out = String::from("[\n");
    for chunk in bytes.chunks(16) {
        out.push_str("    ");
        for b in chunk {
            out.push_str(&format!("0x{b:02x}, "));
        }
        out.push('\n');
    }
    out.push(']');
    out
}
