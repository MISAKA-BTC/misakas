//! **The PALW-RC genesis card: every constant a testnet-12 binary must ship, printed once.**
//!
//! Road-map Gate 4 calls the genesis artifact "the one input code cannot mint". Half of that is no
//! longer true — the floor's `artifact_root` is a derivation now (`misaka_palw_base0::rc`), so this
//! prints it rather than asking for it. The other half is still exactly true and always will be:
//! **which premine output backs the genesis bond, and which ML-DSA-87 keys sign under it.** Those
//! are operator facts. This tool takes them and produces the constants, so a human pastes hashes
//! rather than re-typing parameters — ADR-0042 Decision 11's own requirement.
//!
//! **A card is a REGISTRY, not a bond.** `derive_panel_v2` excludes a claim's own executor by
//! bond, by operator and by key and seats one bond per operator, so a 5-seat panel needs **6
//! distinct operators** — and `BondRegistered` may not ride a transaction, so a registry too small
//! has no later repair. A one-row card is refused by the genesis gate (`PanelCannotBeSeated`), which
//! is why this tool assembles many rows.
//!
//! Two commands, split exactly along the secrecy line: **rows are emitted where the secrets live,
//! and assembled where they do not.**
//!
//! ```text
//! # 1. ON EACH OPERATOR'S OWN HOST — two keys, secrets never leave
//! misaka validator keygen --out /etc/misaka/t12-bond.key
//! misaka validator keygen --out /etc/misaka/t12-operator.key
//!
//! # 2. ON THE SAME HOST — one public row, derived from those seeds
//! palw-rc-genesis --emit-row \
//!     --bond-index 3 \
//!     --bond-seed /etc/misaka/t12-bond.key \
//!     --operator-seed /etc/misaka/t12-operator.key \
//!     --payout-address misakatest:q…            # prints: row 3 <bond-pk> <op-pk> <payout>
//!
//! # 3. ANYWHERE — collect the rows into a file, one per line, and assemble
//! palw-rc-genesis --rows /tmp/t12-rows.txt
//! ```
//!
//! A row carries public values only: two ML-DSA-87 verification keys and an address payload.
//! Nothing in a row lets its holder sign anything.
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

/// A boolean flag. `arg()` returns the value AFTER a flag, so it answers `None` for a bare
/// `--skip-cost` at the end of the line and the flag silently did nothing — which is how
/// `--skip-cost` and `--emit-row` both read as absent however they were spelled.
fn has_flag(name: &str) -> bool {
    std::env::args().skip(1).any(|a| a == name || a.starts_with(&format!("{name}=")))
}

/// An ML-DSA-87 verification key is a fixed 2,592 bytes. Checked HERE, because nothing downstream
/// does: the genesis loader stores whatever it is handed, so a truncated paste mints a bond nobody
/// can ever sign for — and the only repair is a flag-day relaunch, since `BondRegistered` may not
/// ride a transaction.
fn check_pubkey_len(what: &str, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() != kaspa_txscript::MLDSA87_PK_LEN {
        return Err(format!(
            "{what} is {} bytes; an ML-DSA-87 verification key is exactly {} — a truncated paste mints a bond nobody can sign for",
            bytes.len(),
            kaspa_txscript::MLDSA87_PK_LEN
        ));
    }
    Ok(())
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
    if !has_flag("--skip-cost") {
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

    // ---- the two modes ----
    if let Some(rows_path) = arg("--rows") {
        assemble(&rows_path, artifact_root);
        return;
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
        None => match arg("--payout-payload").and_then(|v| hex_bytes(&v)).and_then(|b| {
            let bytes: [u8; 64] = b.try_into().ok()?;
            Some(Hash64::from_bytes(bytes))
        }) {
            Some(explicit) => Some(explicit),
            // **Default: the bond key's own address.** Matured rewards belong to the party that
            // staked, and the bond key is the one identity this row already proves the operator
            // holds — so the safe default needs no second key, no transcription and no third
            // party. It is the same derivation `ValidatorKey::funding_address` makes
            // (`blake2b_512` over the verification key), which is what makes the printed address
            // spendable by the seed that emitted the row.
            //
            // Without this the obvious move is to paste SOME address, and the obvious mistake is
            // to paste one whose key nobody on this network holds — a payout output nobody can
            // ever spend, discovered a settlement window after launch.
            None => bond_pubkey.as_ref().map(|pk| Hash64::from_bytes(kaspa_hashes::blake2b_512_address_payload(pk).as_bytes())),
        },
    };

    let (Some(bond_index), Some(bond_pubkey), Some(operator_pubkey), Some(payout)) =
        (bond_index, bond_pubkey, operator_pubkey, payout)
    else {
        println!("Two commands, and which one you want depends on where the secrets are:");
        println!();
        println!("  --emit-row   ON the host holding the seeds. Prints ONE public row.");
        println!("      --bond-index      which premine output backs this bond (0..=40)");
        println!("      --bond-seed       path to the bond key's seed (or --bond-pubkey <hex>)");
        println!("      --operator-seed   path to the operator key's seed (or --operator-pubkey <hex>)");
        println!("      --payout-address  where matured rewards are paid — DEFAULTS to the bond key's");
        println!("                        own address (or --payout-payload <64-byte hex>)");
        println!();
        println!("  --rows <file>  ANYWHERE. Assembles collected rows into the card and runs the gate.");
        println!();
        println!("  misaka validator keygen --out /etc/misaka/t12-bond.key");
        println!("  misaka validator keygen --out /etc/misaka/t12-operator.key");
        println!();
        println!("The seeds stay where keygen wrote them (0600, owner-only). This tool reads them to");
        println!("DERIVE public values and prints nothing secret. It mints no key: the whole point of");
        println!("a bond is that somebody holds one.");
        println!();
        println!("A registry needs {} DISTINCT operators — a 5-seat panel plus the executor it excludes.", min_bonds());
        return;
    };

    for (what, key) in [("--bond-pubkey", &bond_pubkey), ("--operator-pubkey", &operator_pubkey)] {
        if let Err(e) = check_pubkey_len(what, key) {
            println!("REFUSED: {e}");
            return;
        }
    }

    // One row, emitted for collection. NOT a card: the gate below needs a registry, and saying so
    // here is better than printing constants a node would refuse to boot on.
    println!("Your row — send this LINE (public values only) to whoever assembles the card:");
    println!();
    println!("row {bond_index} {} {} {}", hex_of(&bond_pubkey), hex_of(&operator_pubkey), hex_of(payout.as_byte_slice()));
    println!();
    println!(
        "  payout address    {}",
        kaspa_addresses::Address::new(
            kaspa_addresses::Prefix::Testnet,
            kaspa_addresses::Version::PubKeyHashMlDsa87,
            payout.as_byte_slice(),
        )
    );
    println!();
    println!("Collect {} such rows (one per operator, all different) into a file and run:", min_bonds());
    println!("  palw-rc-genesis --rows <file>");
    if !has_flag("--emit-row") {
        println!();
        println!("(--emit-row is the name for what just happened; a single row cannot become a card.)");
    }
}

fn min_bonds() -> usize {
    kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1()
}

fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Read collected rows, run the WHOLE genesis gate over them, and print the constants a binary
/// ships. The gate is `palw_rc_params_from_artifacts` — the same call a node makes at boot — so an
/// accepted card is one a node accepts, and a refused one names which invariant it missed.
fn assemble(path: &str, artifact_root: Hash64) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            println!("REFUSED: cannot read {path}: {e}");
            std::process::exit(1);
        }
    };
    let mut registry = Vec::new();
    let mut cards: Vec<(u32, Vec<u8>, Vec<u8>, Hash64)> = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        let bad = |why: &str| -> ! {
            println!("REFUSED: line {}: {why}", lineno + 1);
            println!();
            println!("A row is: row <bond-index> <bond-pubkey-hex> <operator-pubkey-hex> <payout-hex>");
            println!("Emit one with `palw-rc-genesis --emit-row …` on the host holding the seeds.");
            std::process::exit(1);
        };
        if f.len() != 5 || f[0] != "row" {
            bad("not a row (expected 5 fields starting with `row`)");
        }
        let Ok(index) = f[1].parse::<u32>() else { bad("bond index is not a number") };
        let (Some(bond_pk), Some(op_pk), Some(payout_bytes)) = (hex_bytes(f[2]), hex_bytes(f[3]), hex_bytes(f[4])) else {
            bad("a field is not hex")
        };
        if let Err(e) = check_pubkey_len("bond pubkey", &bond_pk) {
            bad(&e);
        }
        if let Err(e) = check_pubkey_len("operator pubkey", &op_pk) {
            bad(&e);
        }
        let Ok(payout) = <[u8; 64]>::try_from(payout_bytes.as_slice()) else { bad("payout payload is not 64 bytes") };
        let payout = Hash64::from_bytes(payout);
        registry.push(kaspa_consensus_core::palw_fp_devnet_v3::PalwGenesisBondSpecV1 {
            bond: PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(index)),
            pubkey: bond_pk.clone(),
            operator_pubkey: op_pk.clone(),
            payout_payload: payout,
        });
        cards.push((index, bond_pk, op_pk, payout));
    }
    println!("Assembling {} row(s) from {path}", registry.len());
    println!();

    match kaspa_consensus_core::config::params::palw_rc_params_from_artifacts(artifact_root, registry) {
        Err(e) => {
            println!("REFUSED: {e}");
            println!();
            println!("This is the genesis gate answering, not a tool failure. The two commonest causes:");
            println!("  * fewer than {} DISTINCT operators (a 5-seat panel excludes its own executor);", min_bonds());
            println!("  * a bond index whose premine output does not cover the bundle's collateral floor.");
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
            println!("  bonds               {} row(s), premine #{:?}", cards.len(), cards.iter().map(|c| c.0).collect::<Vec<_>>());
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
            println!("pub const PALW_RC_GENESIS_BONDS: &[PalwRcGenesisBondCard] = &[");
            for (index, bond_pk, op_pk, payout) in &cards {
                println!("    PalwRcGenesisBondCard {{");
                println!("        premine_index: {index},");
                println!("        bond_pubkey: &{},", rust_bytes(bond_pk));
                println!("        operator_pubkey: &{},", rust_bytes(op_pk));
                println!("        payout_payload: {},", rust_bytes(payout.as_byte_slice()));
                println!("    }},");
            }
            println!("];");
            println!();
            println!("Every node must ship the SAME table. It is inside consensus_params_id, so a node");
            println!("that ships a different one is refused at the handshake rather than forking —");
            println!("which is the check, not a reason to be casual about the paste.");
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
