//! **Every value the testnet-12 re-pin compares against, computed by this build — never typed.**
//!
//! `scripts/t12-repin.sh` (docs/t12-rcore-launch-checklist.md §5) reads each pinned literal out of
//! the tree and sets it beside a value this build computes. Most of those values come from here: one
//! `REPIN <key> <value>` line per value, from the same public constructors the pin tests use
//! (`Params::from(NetworkId)` where a pin says "what a node announces", the `*_shipped_params` presets
//! where a pin test builds them that way, the raw `*_PARAMS` consts where a pin reads a const). The
//! rest — the relational twins (a fence taken away), the v22 goldens, the t11 parity dump — are printed
//! by the pin tests themselves, before their assertions, because only those tests know how their value
//! is built. Nothing here asserts a pinned value; it only prints, so it is green on any build.
//!
//! The genesis values are computed from scratch the way `config::drill` computes a drill genesis: the
//! `utxo_commitment` is the MuHash of the premine this build mints for the network, the merkle root is
//! the coinbase's, and the hash is the header's over both — so a premine change shows up here as the
//! commitment AND the hash that follow from it, in one run.
//!
//! Run: `cargo test --locked -p kaspa-consensus-core --test t12_repin_values -- --nocapture`

use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::drill::{PALW_DRILL_SALT_LEN_V1, PalwDrillSaltV1, palw_t12_drill_genesis_block_v1};
use kaspa_consensus_core::config::genesis::GenesisBlock;
use kaspa_consensus_core::config::params::*;
use kaspa_consensus_core::config::premine::*;
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::merkle::calc_hash_merkle_root;
use kaspa_consensus_core::muhash::MuHashExtensions;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_muhash::MuHash;

fn out(key: &str, value: impl std::fmt::Display) {
    println!("REPIN {key} {value}");
}

fn ids(key: &str, p: &Params) {
    out(&format!("{key}.params_id"), p.consensus_params_id());
    out(&format!("{key}.identity_id"), p.consensus_identity_id());
    out(&format!("{key}.schedule_id"), p.consensus_schedule_id());
}

/// The networks by the names the pins use, with the id a node is started with.
fn networks() -> [(&'static str, NetworkId); 6] {
    [
        ("mainnet", NetworkId::new(NetworkType::Mainnet)),
        ("testnet-10", NetworkId::with_suffix(NetworkType::Testnet, 10)),
        ("testnet-11", NetworkId::with_suffix(NetworkType::Testnet, 11)),
        ("testnet-12", NetworkId::with_suffix(NetworkType::Testnet, 12)),
        ("simnet", NetworkId::new(NetworkType::Simnet)),
        ("devnet", NetworkId::new(NetworkType::Devnet)),
    ]
}

/// `genesis` with its commitment, merkle root and hash recomputed from `net`'s premine and its own
/// coinbase — what the `GenesisBlock` constant of `net` must say.
fn recomputed(genesis: &GenesisBlock, net: NetworkId) -> GenesisBlock {
    let mut g = genesis.clone();
    let mut multiset = MuHash::new();
    for (outpoint, entry) in genesis_premine_utxos_for(net) {
        multiset.add_utxo(&outpoint, &entry);
    }
    g.utxo_commitment = multiset.finalize();
    g.hash_merkle_root = calc_hash_merkle_root(g.build_genesis_transactions().iter());
    g.hash = Header::from(&g).hash;
    // The same hash through the block constructor the genesis tests use.
    assert_eq!(Block::from(&g).hash(), g.hash);
    g
}

#[test]
fn print_every_value_the_pins_hold() {
    // ---- 1. every preset's three ids, as a node builds its params (materialized) ----
    for (name, net) in networks() {
        ids(&format!("from.{name}"), &Params::from(net));
    }
    // …as the pin tests that call the shipped presets build them…
    ids("shipped.mainnet", &mainnet_shipped_params());
    ids("shipped.testnet-11", &palw_rc_shipped_params());
    ids("shipped.testnet-12", &palw_t12_shipped_params());
    ids("shipped.devnet", &devnet_shipped_params());
    // …and as the raw consts (`palw_the_release_did_not_move` reads `MAINNET_PARAMS` itself).
    ids("const.mainnet", &MAINNET_PARAMS);
    for (name, params) in [
        ("mainnet", MAINNET_PARAMS),
        ("testnet", TESTNET_PARAMS),
        ("testnet-11", TESTNET11_PARAMS),
        ("simnet", SIMNET_PARAMS),
        ("devnet", DEVNET_PARAMS),
    ] {
        // `shipped_presets_have_pinned_fingerprints` materializes each const through its own `net`.
        out(&format!("preset_net.{name}.params_id"), Params::from(params.net).consensus_params_id());
    }

    // ---- 2. testnet-12's identity as its startup log prints it ----
    let t12_net = NetworkId::with_suffix(NetworkType::Testnet, 12);
    let t12 = Params::from(t12_net);
    out("testnet-12.fence_schedule", t12.fence_schedule_v1().iter().map(|fence| fence.to_string()).collect::<Vec<_>>().join(","));
    out("rule_manifest.digest", kaspa_consensus_core::palw_rule_manifest_v1::palw_rule_manifest_digest_v1());
    out("rule_manifest.line", kaspa_consensus_core::palw_rule_manifest_v1::palw_rule_manifest_line_v1().replace(' ', "_"));
    out("testnet-12.pruning_depth", t12.pruning_depth());

    // ---- 3. every network's genesis, recomputed from the premine this build mints ----
    for (name, net) in networks() {
        let g = recomputed(&Params::from(net).genesis, net);
        out(&format!("genesis.{name}.hash"), g.hash);
        out(&format!("genesis.{name}.hash_merkle_root"), g.hash_merkle_root);
        out(&format!("genesis.{name}.utxo_commitment"), g.utxo_commitment);
    }

    // ---- 4. testnet-12's genesis txids ----
    out("testnet-12.premine_txid", premine_txid_for(t12_net));
    out("testnet-12.community_txid", testnet12_community_txid());

    // ---- 5. testnet-12's classes: the floor, the genesis model classes in order, the two rows ----
    let PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    out("testnet-12.class.floor", bundle.base_class_id);
    let registered: Vec<String> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, .. } if *class_id != bundle.base_class_id => Some(class_id.to_string()),
            _ => None,
        })
        .collect();
    out("testnet-12.class.registered", registered.join(","));
    let row = |n_ctx: u32| {
        kaspa_consensus_core::palw_context_ladder::palw_a16_context_row_profile_v7(n_ctx)
            .expect("the held dense row derives")
            .shape_profile_id()
    };
    out("testnet-12.class.8k", row(PALW_T12_NARROW_DENSE_N_CTX));
    out("testnet-12.class.2m", row(PALW_T12_DENSE_N_CTX));
    // C7 as the chain DERIVES it (the 2M row), not the pinned `PALW_T12_2M_CLASS_ID_BYTES` literal the
    // const preset carries — that literal is one of the pins compared against this.
    out("testnet-12.class.c7", palw_t12_rcore_conservative_classes_v1().iter().map(|c| c.to_string()).collect::<Vec<_>>().join(","));

    // ---- 6. the deploy kit's copies of chain facts ----
    out("kit.fee_float_base", MAIN_PREMINE_INDEX + 1);
    let card0 = PALW_T12_GENESIS_BONDS.first().expect("a genesis card");
    out("kit.hb_addr", Address::new(Prefix::Testnet, Version::PubKeyHashMlDsa87, &card0.payout_payload));
    out("kit.cards", PALW_T12_GENESIS_BONDS.iter().map(|c| c.premine_index.to_string()).collect::<Vec<_>>().join(","));

    // ---- 7. the checklist's example drill genesis (salt `53…53`, what `probe-identity-local.sh --drill-salt` prints) ----
    let salt = PalwDrillSaltV1::from_bytes([0x53; PALW_DRILL_SALT_LEN_V1]).expect("a non-zero salt");
    out("drill.testnet-12.salt53.genesis", palw_t12_drill_genesis_block_v1(&salt).hash);
}
