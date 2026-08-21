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
//! # 1. the operator makes two keys and keeps the secrets (this tool never sees them again)
//! misaka validator keygen --out /etc/misaka/t12-bond.key
//! misaka validator keygen --out /etc/misaka/t12-operator.key
//!
//! # 2. the card, from the seeds — no hex is typed by hand
//! cargo run -p misaka-palw-base0 --bin palw-rc-genesis -- \
//!     --bond-index 0 \
//!     --bond-seed /etc/misaka/t12-bond.key \
//!     --operator-seed /etc/misaka/t12-operator.key \
//!     --payout-address misakatest:q…
//! ```
//!
//! The raw forms `--bond-pubkey <hex>`, `--operator-pubkey <hex>` and `--payout-payload <hex>` are
//! still accepted for a card whose keys live somewhere this tool cannot read.
//!
//! **`--bond-seed` and `--operator-seed` read SECRET material and print only public values** — the
//! verification key and the address payload. They exist because the derivation has to be the same
//! one the producer uses (`ml_dsa_87::generate_key_pair`), and an operator transcribing a 2.6 KiB
//! hex blob by hand is an operator who will eventually transcribe it wrong. The seed file is read
//! with the hardened loader: owner-only permissions required, symlinks refused, fail closed.
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

/// Derive the ML-DSA-87 verification key from a seed file named by `flag`, or `Ok(None)` if the
/// flag was not given. Reads secret material; returns only the public half.
fn seed_pubkey(flag: &str) -> Result<Option<Vec<u8>>, String> {
    let Some(path) = arg(flag) else {
        return Ok(None);
    };
    let seed = kaspa_pq_validator_core::load_validator_seed(&path)?;
    Ok(Some(kaspa_pq_validator_core::ValidatorKey::from_seed(seed).public_key().to_vec()))
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
                println!(
                    "One block's work — canonical job ({} prefill, {} decode):",
                    PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1
                );
                println!("  step leaves         {leaves}");
                println!(
                    "  wall time           {:.3} s  (one inference per template; the nonce grind is free)",
                    elapsed.as_secs_f64()
                );
                println!("  execution_root      {}", run.execution_root);
                println!("  output tokens       {:?}", run.generated_token_ids);
            }
            Err(e) => println!("One block's work: REFUSED — {e}"),
        }
        println!();
    }

    let bond_index: Option<u32> = arg("--bond-index").and_then(|v| v.parse().ok());
    // A seed, when given, is the source of truth: deriving beats transcribing, and the derivation
    // is `ValidatorKey::from_seed`, which is `ml_dsa_87::generate_key_pair` — the same call the
    // producer makes on the same file, so the key the card registers is the key that will sign.
    let bond_pubkey = match seed_pubkey("--bond-seed") {
        Err(e) => {
            println!("REFUSED: --bond-seed: {e}");
            return;
        }
        Ok(Some(pk)) => Some(pk),
        Ok(None) => arg("--bond-pubkey").and_then(|v| hex_bytes(&v)),
    };
    let operator_pubkey = match seed_pubkey("--operator-seed") {
        Err(e) => {
            println!("REFUSED: --operator-seed: {e}");
            return;
        }
        Ok(Some(pk)) => Some(pk),
        Ok(None) => arg("--operator-pubkey").and_then(|v| hex_bytes(&v)),
    };
    let payout: Option<Hash64> = match arg("--payout-address") {
        Some(a) => match kaspa_addresses::Address::try_from(a.as_str()) {
            Ok(addr) if addr.version != kaspa_addresses::Version::PubKeyHashMlDsa87 => {
                println!("REFUSED: --payout-address is not an ML-DSA-87 P2PKH address");
                return;
            }
            Ok(addr) => match <[u8; 64]>::try_from(addr.payload.as_slice()) {
                Ok(bytes) => Some(Hash64::from_bytes(bytes)),
                Err(_) => {
                    println!("REFUSED: --payout-address payload is not 64 bytes");
                    return;
                }
            },
            Err(e) => {
                println!("REFUSED: --payout-address: {e}");
                return;
            }
        },
        None => arg("--payout-payload").and_then(|v| hex_bytes(&v)).and_then(|b| {
            let bytes: [u8; 64] = b.try_into().ok()?;
            Some(Hash64::from_bytes(bytes))
        }),
    };

    let (Some(bond_index), Some(bond_pubkey), Some(operator_pubkey), Some(payout)) =
        (bond_index, bond_pubkey, operator_pubkey, payout)
    else {
        println!("Waiting on the three facts code cannot mint:");
        println!("  --bond-index      which premine output backs the genesis bond (0..=40)");
        println!("  --bond-seed       path to the bond key's seed (or --bond-pubkey <hex>)");
        println!("  --operator-seed   path to the operator key's seed (or --operator-pubkey <hex>)");
        println!("  --payout-address  where matured rewards are paid (or --payout-payload <64-byte hex>)");
        println!();
        println!("  misaka validator keygen --out /etc/misaka/t12-bond.key");
        println!("  misaka validator keygen --out /etc/misaka/t12-operator.key");
        println!();
        println!("The seeds stay where keygen wrote them (0600, owner-only). This tool reads them to");
        println!("DERIVE public values and prints nothing secret. It mints no key: the whole point of");
        println!("a bond is that somebody holds one.");
        return;
    };

    let bond = PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(bond_index));
    // ONE bond is not a registry. `derive_panel_v2` excludes a claim's own executor by bond, by
    // operator and by key and seats one bond per operator, so a `seat_count`-seat panel needs
    // `seat_count + 1` DISTINCT operators — and `BondRegistered` may not ride a transaction, so a
    // registry too small has no later repair. This tool takes one row today; the gate below is what
    // says so out loud instead of minting a network that makes two blocks and stops.
    let registry = vec![kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
        bond,
        pubkey: bond_pubkey.clone(),
        operator_pubkey: operator_pubkey.clone(),
        payout_payload: payout,
    }];
    match kaspa_consensus_core::config::params::palw_rc_params_from_artifacts(artifact_root, registry) {
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
