//! **The deploy kit's copies of chain facts are this build's** (release-prep review, 2026-09-25).
//!
//! `contrib/t12-deploy-kit` and `contrib/misakascan-t12` carry literals that no node checks when it
//! starts: the 8k class id a producer is pointed at (`CLASS_8K`), the 8k artifact's byte count, the
//! 2M class prefix the kit refuses to stage, the premine index where card N's fee float sits
//! (`FEE_FLOAT_BASE + N`), and the explorer's class table and bond txid. The launch gate compares the
//! fingerprint and the genesis only, so a merge that moved one of these — an Activation Pool genesis
//! row, a re-ordered card list, a new held row — would pass `stage` and `switch` and fail later and
//! quietly: a producer idling on a class the registry does not hold, a seat carrying with another
//! card's float. This reads those files and fails, naming the file and the constant, when they
//! disagree with this build. Run it at the re-pin (`docs/t12-rcore-launch-checklist.md` §5):
//!
//! ```text
//! cargo test --locked -p kaspa-consensus-core --test t12_deploy_kit_constants -- --nocapture
//! ```
//!
//! It prints the premine layout it checked (card → bond index, fee float index, payout address), which
//! is what `probe-identity-local.sh --layout` shows the operator.

use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::config::params::*;
use kaspa_consensus_core::config::premine::*;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use std::collections::BTreeSet;
use std::path::PathBuf;

fn t12() -> NetworkId {
    NetworkId::with_suffix(NetworkType::Testnet, 12)
}

fn repo_file(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `NAME=value` from a shell file: the first line that starts with the assignment, its value up to
/// the first blank (a trailing `# comment` is dropped), quotes removed. Multi-line values are not read.
fn shell_value(file: &str, text: &str, name: &str) -> String {
    let prefix = format!("{name}=");
    let line = text
        .lines()
        .find(|l| l.starts_with(&prefix))
        .unwrap_or_else(|| panic!("{file}: no line assigns {name}"));
    let value = line[prefix.len()..].split_whitespace().next().unwrap_or("");
    value.trim_matches('"').to_string()
}

/// Every `id:"<128 hex>"` in the explorer's `LLM_CLASSES` table.
fn app_js_class_ids(app: &str) -> BTreeSet<String> {
    let start = app.find("const LLM_CLASSES = [").expect("app.js: no LLM_CLASSES table");
    let end = start + app[start..].find("];").expect("app.js: LLM_CLASSES is not closed");
    app[start..end]
        .split("id:\"")
        .skip(1)
        .map(|rest| rest.chars().take_while(|c| c.is_ascii_hexdigit()).collect::<String>())
        .inspect(|id| assert_eq!(id.len(), 128, "app.js LLM_CLASSES: {id} is not a 128-hex class id"))
        .collect()
}

fn app_js_string_const(app: &str, name: &str) -> String {
    let prefix = format!("const {name} = \"");
    let start = app.find(&prefix).unwrap_or_else(|| panic!("app.js: no {name}")) + prefix.len();
    app[start..].chars().take_while(|c| *c != '"').collect()
}

#[test]
fn the_deploy_kit_and_the_explorer_name_this_builds_classes_and_premine_layout() {
    const FLEET: &str = "contrib/t12-deploy-kit/fleet.env.example";
    const LIB: &str = "contrib/t12-deploy-kit/lib.sh";
    const APP: &str = "contrib/misakascan-t12/app.js";
    const SIDECAR_8K: &str = "consensus/core/src/config/class-manifests/qwen25-1.5b-a16-8k.palwmanifest";
    let fleet = repo_file(FLEET);
    let lib = repo_file(LIB);
    let app = repo_file(APP);
    let sidecar = repo_file(SIDECAR_8K);

    let p = Params::from(t12());
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };

    // ---- the premine layout: card N is bond index N and fee float index FEE_FLOAT_BASE + N ----
    let fee_float_base: u32 = shell_value(FLEET, &fleet, "FEE_FLOAT_BASE").parse().expect("FEE_FLOAT_BASE is a number");
    assert_eq!(
        fee_float_base,
        MAIN_PREMINE_INDEX + 1,
        "{FLEET} FEE_FLOAT_BASE: the premine carves the floats after the main wallet (premine.rs MAIN_PREMINE_INDEX + 1)"
    );
    assert_eq!(PALW_T12_GENESIS_BONDS.len(), 8, "the kit's node tables are cards 0..=7 (install-ibm/113/5104.sh)");
    let set = genesis_premine_utxos_for(t12());
    let txid = premine_outpoint_for(t12(), 0).transaction_id;
    for (i, card) in PALW_T12_GENESIS_BONDS.iter().enumerate() {
        let n = i as u32;
        // `bonded_genesis_utxos_on` puts the collateral at the card's DECLARED index and the float at
        // MAIN + 1 + its POSITION in the list; the kit names both by one card number, so the two must agree.
        assert_eq!(card.premine_index, n, "card in list position {i} declares premine index {}: the kit's `$PREMINE_TXID:{n}` would name another bond", card.premine_index);
        let bond = premine_outpoint_for(t12(), n);
        let float = premine_outpoint_for(t12(), fee_float_base + n);
        assert!(set.contains_key(&bond), "card {n}: no collateral at {bond:?}");
        let entry = set.get(&float).unwrap_or_else(|| panic!("card {n}: no fee float at index {}", fee_float_base + n));
        let spk = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&card.payout_payload);
        assert_eq!(entry.script_public_key, spk, "card {n}: the float at index {} is not paid to this card's payout key", fee_float_base + n);
        let addr = Address::new(Prefix::Testnet, Version::PubKeyHashMlDsa87, &card.payout_payload);
        println!("LAYOUT card {n}: bond {txid}:{n}  fee float {txid}:{}  payout {addr}", fee_float_base + n);
    }
    assert_eq!(
        app_js_string_const(&app, "PANEL_BOND_TX"),
        txid.to_string(),
        "{APP} PANEL_BOND_TX: the genesis bonds sit on testnet-12's own premine txid"
    );

    // ---- the classes ----
    let registered: Vec<String> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, .. } if *class_id != bundle.base_class_id => Some(class_id.to_string()),
            _ => None,
        })
        .collect();
    let class_8k = shell_value(FLEET, &fleet, "CLASS_8K");
    let narrow = kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v7(PALW_T12_NARROW_DENSE_N_CTX)
        .expect("the 8k row derives")
        .shape_profile_id()
        .to_string();
    assert_eq!(class_8k, narrow, "{FLEET} CLASS_8K: the 8k genesis row's class id at this build's graph-v7 profile");
    assert!(registered.contains(&class_8k), "{FLEET} CLASS_8K {class_8k} is not a genesis class of this build: {registered:?}");
    let two_m = PALW_T12_RCORE_CONSERVATIVE_CLASSES[0].to_string();
    assert!(registered.contains(&two_m), "the 2M row is registered at genesis");
    let prefix_2m = shell_value(LIB, &lib, "CLASS_2M_PREFIX");
    assert!(
        prefix_2m.len() >= 8 && two_m.starts_with(&prefix_2m),
        "{LIB} CLASS_2M_PREFIX {prefix_2m}: the kit refuses the 2M class by this prefix, and the 2M class is {two_m}"
    );
    let mut on_chain: BTreeSet<String> = registered.iter().cloned().collect();
    on_chain.insert(bundle.base_class_id.to_string());
    assert_eq!(app_js_class_ids(&app), on_chain, "{APP} LLM_CLASSES: the floor and every genesis model class, and nothing else");

    // ---- the 8k artifact the kit stages ----
    let sidecar_bytes: u64 = sidecar
        .split("\"artifact_bytes\":")
        .nth(1)
        .and_then(|rest| rest.trim_start().split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse().ok())
        .expect("the committed 8k sidecar states artifact_bytes");
    assert_eq!(
        shell_value(FLEET, &fleet, "ART_8K_BYTES").parse::<u64>().expect("ART_8K_BYTES is a number"),
        sidecar_bytes,
        "{FLEET} ART_8K_BYTES: the committed 8k sidecar's artifact_bytes"
    );
    assert!(sidecar.contains(&format!("\"class_id\": \"{class_8k}\"")), "{SIDECAR_8K} names another class than CLASS_8K");

    // ---- the genesis is not one the kit forbids ----
    let genesis = p.genesis.hash.to_string();
    let start = fleet.find("FORBIDDEN_GENESIS=\"").expect("FORBIDDEN_GENESIS") + "FORBIDDEN_GENESIS=\"".len();
    let forbidden: Vec<&str> = fleet[start..fleet[start..].find('"').map(|e| start + e).expect("closed")].split_whitespace().collect();
    assert!(forbidden.len() >= 6, "{FLEET} FORBIDDEN_GENESIS: the six retired/private/drill geneses");
    for f in &forbidden {
        assert_eq!(f.len(), 128, "{FLEET} FORBIDDEN_GENESIS: {f} is not a full hash");
        assert_ne!(*f, genesis, "{FLEET} FORBIDDEN_GENESIS lists this build's own genesis {genesis}");
    }
    println!("CLASSES floor {} · 8k {class_8k} · 2M {two_m}", bundle.base_class_id);
    println!("GENESIS {genesis} (not in FORBIDDEN_GENESIS: {} entries)", forbidden.len());
}
