//! **A testnet-12 drill node: `--palw-drill-genesis-salt` and what it refuses** (ADR-0152 §8.2 and
//! §8.3 item 3, `phase2-plan.md` §1.8 and P2-12, T53).
//!
//! The salt, the drill genesis and the drill keyring are consensus-core's
//! (`kaspa_consensus_core::config::drill`); this module is the node half: which flags a salted node
//! must and must not carry, the datadir marker that keeps a drill and a public node out of each
//! other's app directory, and the keyring export the drill script reads
//! (`scripts/misaka-palw-t12-rcore-drill.sh`).
//!
//! **Everything here is refused at start-up, before a peer is dialed or a block is signed**, because
//! every failure it prevents is one a running drill cannot undo:
//!
//! * **testnet-12 only.** A salt drills the network whose shipping rules it keeps; there is no drill
//!   genesis for mainnet, testnet-11, testnet-10, devnet or simnet.
//! * **No discovery.** `--nodnsseed` and an explicit `--connect`/`--addpeer` list are required, and
//!   the drill params carry no DNS seeders, so a drill never asks public testnet-12's seeders for
//!   peers (the handshake would refuse them on the genesis anyway; the requirement keeps the drill
//!   from advertising itself to them).
//! * **The shipping rules, unedited.** `--override-params-file` is refused: the salt swaps the params
//!   for the drill's, and an override would either be discarded silently or drill rules nobody ships.
//! * **Drill-only keys.** The producer key must be a bond key of THIS drill's keyring, and the pay and
//!   heartbeat addresses must be addresses of it. A card key signing on a drill chain is the replay
//!   ADR-0152 §8.2 forbids, and a public miner script on a drill coinbase mints a public outpoint.
//!   `--palw-producer-bond` and `--palw-fee-outpoint` naming public testnet-12's genesis txids are
//!   refused by name: they are a public unit's flags copied onto a drill.
//! * **Its own app directory.** A drill writes a marker into `<appdir>/<network>/`; a salted node
//!   refuses a directory that holds another node's data without its marker, and an unsalted node
//!   refuses one with a marker. Without it a drill pointed at a public node's app dir meets
//!   "Genesis not found in active consensus DB … delete?" — and with `--yes` deletes that node's
//!   database — and shares its panel state (`palw-panel/palw-fee-outpoint`) besides.
use crate::args::Args;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::config::drill::{
    PALW_DRILL_KEYRING_SPAN_V1, PalwDrillKeyRoleV1, PalwDrillKeyringV1, PalwDrillSaltV1, palw_drill_network_v1,
    palw_t12_drill_cards_v1, palw_t12_drill_fee_float_outpoint_v1, palw_t12_drill_main_key_v1, palw_t12_drill_premine_outpoint_v1,
    palw_t12_public_genesis_txid_v1,
};
use kaspa_consensus_core::errors::config::{ConfigError, ConfigResult};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use std::path::{Path, PathBuf};

fn refused(why: impl Into<String>) -> ConfigError {
    ConfigError::PalwDrillRefused(why.into())
}

/// **The salt the command line carries, parsed** — `None` without the flag. One parser
/// (`PalwDrillSaltV1::from_hex`), so every refusal of a malformed salt says the same thing.
pub fn palw_drill_salt_of_v1(args: &Args) -> ConfigResult<Option<PalwDrillSaltV1>> {
    args.palw_drill_genesis_salt.as_deref().map(|hex| PalwDrillSaltV1::from_hex(hex).map_err(|e| refused(e.to_string()))).transpose()
}

/// **Every start-up refusal a salted node answers to**, in the order an operator fixes them. Called
/// from `daemon::validate_args`, so every entry point that builds a node runs it. Without the salt
/// it only refuses the keyring export (which needs one) and returns.
pub fn palw_drill_validate_args_v1(args: &Args) -> ConfigResult<()> {
    let Some(salt) = palw_drill_salt_of_v1(args)? else {
        if args.palw_drill_write_keyring.is_some() {
            return Err(refused("--palw-drill-write-keyring writes a drill's keyring and needs --palw-drill-genesis-salt"));
        }
        return Ok(());
    };
    let network = args.network();
    if network != palw_drill_network_v1() {
        return Err(refused(format!(
            "a drill genesis salt applies to testnet-12 only (--testnet --netsuffix=12), and this node is on {network}: a salt drills \
             the network whose shipping rules it keeps"
        )));
    }
    // The export writes files and exits; it dials nobody and signs nothing, so the node-only
    // requirements below do not apply to it.
    if args.palw_drill_write_keyring.is_some() {
        return Ok(());
    }
    if !args.disable_dns_seeding {
        return Err(refused(
            "a drill node needs --nodnsseed: it finds its peers by explicit address only and never asks public testnet-12's seeders",
        ));
    }
    if args.connect_peers.is_empty() && args.add_peers.is_empty() {
        return Err(refused(
            "a drill node needs an explicit peer list (--addpeer=<ip:port> or --connect=<ip:port>, the other drill hosts)",
        ));
    }
    if args.override_params_file.is_some() {
        return Err(refused(
            "--override-params-file is refused on a drill: the salt installs testnet-12's shipping rules on the drill genesis, and a drill of \
             edited rules drills a network nobody runs",
        ));
    }
    let ring = PalwDrillKeyringV1::new(salt);
    palw_drill_keys_are_drill_only_v1(args, &ring)?;
    for (flag, value) in [("--palw-producer-bond", &args.palw_producer_bond), ("--palw-fee-outpoint", &args.palw_fee_outpoint)] {
        if let Some(outpoint) = value.as_deref().and_then(|s| crate::palw_producer::parse_outpoint(s).ok())
            && palw_t12_public_genesis_txid_v1(&outpoint.transaction_id)
        {
            return Err(refused(format!(
                "{flag}={} names public testnet-12's genesis txid — a public unit's flag copied onto a drill. A drill seat's bond and fee \
                 float are on the drill's own premine txid (the keyring manifest lists them: --palw-drill-write-keyring)",
                value.as_deref().unwrap_or_default()
            )));
        }
    }
    Ok(())
}

/// **The drill-only key rule** (ADR-0152 §8.2): the producer key is one of this drill's bond keys,
/// and the pay and heartbeat addresses are this drill's. A key or address the keyring does not
/// derive is refused whether or not public testnet-12 knows it: the keyring is the positive list,
/// so no deny-list has to stay complete.
fn palw_drill_keys_are_drill_only_v1(args: &Args, ring: &PalwDrillKeyringV1) -> ConfigResult<()> {
    let salt_id = ring.salt().id();
    if let Some(path) = args.palw_producer_key.as_deref() {
        let seed = kaspa_pq_validator_core::load_validator_seed(path).map_err(|e| refused(format!("--palw-producer-key: {e}")))?;
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed);
        if ring.find_bond_pubkey(key.public_key()).is_none() {
            return Err(refused(format!(
                "--palw-producer-key={path} is not a bond key of drill {salt_id} (bond keys 0..{PALW_DRILL_KEYRING_SPAN_V1}). A drill signs \
                 with drill-only keys, never a card key: take a seed from the keyring --palw-drill-write-keyring writes"
            )));
        }
    }
    for (flag, value) in [
        ("--palw-producer-pay-address", &args.palw_producer_pay_address),
        ("--palw-heartbeat-miner-address", &args.palw_heartbeat_miner_address),
    ] {
        let Some(text) = value.as_deref() else { continue };
        let address = Address::try_from(text).map_err(|e| refused(format!("{flag}={text} is not an address: {e}")))?;
        if address.version != Version::PubKeyHashMlDsa87 || ring.find_payload(address.payload.as_slice()).is_none() {
            return Err(refused(format!(
                "{flag}={text} is not an address of drill {salt_id}. A drill is paid only at drill-only addresses: a public miner script \
                 on a drill coinbase mints an outpoint public testnet-12 can mint too, and a spend of one is a spend of the other"
            )));
        }
    }
    Ok(())
}

/// **Where drill injectors that are otherwise devnet/simnet-only may run**: devnet, simnet, and a
/// salted testnet-12 — a private chain by construction. Public testnet-12 stays refused.
pub fn palw_private_drill_network_v1(network: NetworkId, args: &Args) -> bool {
    matches!(network.network_type, NetworkType::Devnet | NetworkType::Simnet) || args.palw_drill_genesis_salt.is_some()
}

/// The marker a drill writes into `<appdir>/<network>/`.
pub const PALW_DRILL_DATADIR_MARKER_V1: &str = "palw-drill-genesis";

/// **The app-directory guard** (see the module doc): `prefixed_dir` is `<appdir>/<network prefixed>`.
///
/// * salted: a marker naming another drill is refused; a directory holding anything but `logs/`
///   (the logger opens it before this runs) with no marker is refused — it may be a public node's;
///   otherwise the marker is written (idempotent) and the drill proceeds.
/// * unsalted: a marker is refused — the directory holds a drill chain.
pub fn palw_drill_datadir_guard_v1(prefixed_dir: &Path, salt: Option<&PalwDrillSaltV1>, genesis_hash: &str) -> Result<(), String> {
    let marker_path = prefixed_dir.join(PALW_DRILL_DATADIR_MARKER_V1);
    let marker_id = std::fs::read_to_string(&marker_path)
        .ok()
        .and_then(|text| text.lines().find_map(|line| line.strip_prefix("salt_id=").map(|id| id.trim().to_owned())));
    match (salt, marker_id) {
        (None, None) => Ok(()),
        (None, Some(id)) => Err(format!(
            "{} holds the chain of testnet-12 drill {id} ({}). Start it with that drill's --palw-drill-genesis-salt, or give this node its \
             own --appdir: an unsalted node would find the drill genesis in place of its own and offer to delete the database",
            prefixed_dir.display(),
            marker_path.display()
        )),
        (Some(salt), Some(id)) if id != salt.id() => Err(format!(
            "{} holds the chain of testnet-12 drill {id}, and this node runs drill {}: give each drill its own --appdir",
            prefixed_dir.display(),
            salt.id()
        )),
        (Some(_), Some(_)) => Ok(()),
        (Some(salt), None) => {
            let occupied = std::fs::read_dir(prefixed_dir)
                .map(|entries| entries.filter_map(|e| e.ok()).any(|e| e.file_name() != "logs"))
                .unwrap_or(false);
            if occupied {
                return Err(format!(
                    "{} holds a node's data and no drill marker — it may be a public testnet-12 node's. A drill never opens it: at the \
                     genesis check it would offer to delete that node's database (and with --yes would), and it would share that node's \
                     panel state. Give the drill a fresh --appdir",
                    prefixed_dir.display()
                ));
            }
            std::fs::create_dir_all(prefixed_dir).map_err(|e| format!("cannot create {}: {e}", prefixed_dir.display()))?;
            std::fs::write(&marker_path, format!("salt_id={}\ngenesis={genesis_hash}\n", salt.id()))
                .map_err(|e| format!("cannot write the drill marker {}: {e}", marker_path.display()))
        }
    }
}

/// How many keys of each exported role the keyring export writes (bond keys `0..16` covers the eight
/// genesis seats and D-9's and D-10's registrations with room to spare; the keyring itself spans
/// [`PALW_DRILL_KEYRING_SPAN_V1`]).
pub const PALW_DRILL_EXPORT_PER_ROLE_V1: u32 = 16;

/// The manifest's format tag; the drill script checks it before reading a field.
pub const PALW_DRILL_KEYRING_FORMAT_V1: &str = "misaka-palw-drill-keyring/v1";

/// **Write a drill's keyring to `dir` and return the manifest's path** (`--palw-drill-write-keyring`).
///
/// Seeds are written as hex, mode 0600 (what `--palw-producer-key` accepts): the main wallet, bond
/// keys `0..16` (the genesis seats first), heartbeat and payout keys `0..16`. `manifest.json` names
/// the drill — salt id, genesis hash, fingerprint, public testnet-12's genesis beside it — and every
/// seat's bond outpoint, fee float and address, so the drill script reads the binary's own answer and
/// never re-derives one. The salt itself is not written: the operator already holds it.
///
/// A directory holding another drill's manifest is refused; the same drill's is rewritten.
pub fn palw_drill_write_keyring_v1(salt: &PalwDrillSaltV1, dir: &Path) -> Result<PathBuf, String> {
    let manifest_path = dir.join("manifest.json");
    if let Ok(existing) = std::fs::read_to_string(&manifest_path) {
        let existing: serde_json::Value = serde_json::from_str(&existing).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        if existing.get("salt_id").and_then(|v| v.as_str()) != Some(salt.id().as_str()) {
            return Err(format!("{} is another drill's keyring; write this drill's elsewhere", manifest_path.display()));
        }
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let prefix = Prefix::Testnet;
    let ring = PalwDrillKeyringV1::new(*salt);
    let write_seed = |name: String, seed: &[u8; 32]| -> Result<String, String> {
        let path = dir.join(&name);
        write_private(&path, faster_hex::hex_string(seed).as_bytes())?;
        Ok(name)
    };

    let params = kaspa_consensus_core::config::params::palw_t12_drill_params_v1(salt);
    let public = kaspa_consensus_core::config::params::Params::from(palw_drill_network_v1());
    let main = palw_t12_drill_main_key_v1(salt);
    let cards = palw_t12_drill_cards_v1(salt);
    let mut seats = Vec::new();
    for (n, card) in cards.iter().enumerate() {
        seats.push(serde_json::json!({
            "n": n,
            "premine_index": card.premine_index,
            "bond_outpoint": outpoint_text(palw_t12_drill_premine_outpoint_v1(salt, card.premine_index)),
            "fee_float_outpoint": outpoint_text(palw_t12_drill_fee_float_outpoint_v1(salt, n as u32)),
            "address": card.bond.address(prefix).to_string(),
            "seed_file": write_seed(format!("bond-{n}.seed"), &card.bond.seed)?,
            "operator_pubkey": faster_hex::hex_string(&card.operator.pubkey),
        }));
    }
    let role_rows = |role: PalwDrillKeyRoleV1, from: u32| -> Result<Vec<serde_json::Value>, String> {
        (from..PALW_DRILL_EXPORT_PER_ROLE_V1)
            .map(|n| {
                let key = ring.key(role, n);
                Ok(serde_json::json!({
                    "n": n,
                    "address": key.address(prefix).to_string(),
                    "seed_file": write_seed(format!("{}-{n}.seed", role.name()), &key.seed)?,
                }))
            })
            .collect()
    };
    let manifest = serde_json::json!({
        "format": PALW_DRILL_KEYRING_FORMAT_V1,
        "network": palw_drill_network_v1().to_string(),
        "salt_id": salt.id(),
        "genesis_hash": params.genesis.hash.to_string(),
        "consensus_params_id": params.consensus_params_id().to_string(),
        "public_genesis_hash": public.genesis.hash.to_string(),
        "public_consensus_params_id": public.consensus_params_id().to_string(),
        "premine_txid": kaspa_consensus_core::config::premine::palw_t12_drill_premine_txid_v1(salt).to_string(),
        "community_txid": kaspa_consensus_core::config::premine::palw_t12_drill_community_txid_v1(salt).to_string(),
        "main": {
            "address": main.address(prefix).to_string(),
            "outpoint": outpoint_text(palw_t12_drill_premine_outpoint_v1(salt, kaspa_consensus_core::config::premine::MAIN_PREMINE_INDEX)),
            "seed_file": write_seed("main-0.seed".to_owned(), &main.seed)?,
        },
        "seats": seats,
        // Bond keys past the genesis seats: what a drill registers (D-9's 130,000 MSK seat, D-10's
        // re-registration), each its own operator identity (`--palw-register-bond` signs both halves
        // with the one key).
        "bonds": role_rows(PalwDrillKeyRoleV1::Bond, cards.len() as u32)?,
        "heartbeat": role_rows(PalwDrillKeyRoleV1::Heartbeat, 0)?,
        "payout": role_rows(PalwDrillKeyRoleV1::Payout, 0)?,
    });
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&manifest_path, text).map_err(|e| format!("cannot write {}: {e}", manifest_path.display()))?;
    Ok(manifest_path)
}

fn outpoint_text(outpoint: kaspa_consensus_core::tx::TransactionOutpoint) -> String {
    format!("{}:{}", outpoint.transaction_id, outpoint.index)
}

/// Write `bytes` to `path` readable by the owner only — what `load_validator_seed` insists on.
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // `mode` applies at creation only; a rewritten seed file is narrowed too.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    file.write_all(bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// **The drill's pre-flight**, run by `daemon::create_core` before the logger opens a file under
/// the app directory: the argument rules, the keyring export (which exits), and the app-directory
/// guard. Prints and exits on a refusal, like the other start-up rails.
pub fn palw_drill_preflight_v1(args: &Args) {
    if let Err(e) = palw_drill_validate_args_v1(args) {
        println!("{e}");
        std::process::exit(1);
    }
    let salt = palw_drill_salt_of_v1(args).ok().flatten();
    if let (Some(salt), Some(dir)) = (salt.as_ref(), args.palw_drill_write_keyring.as_deref()) {
        match palw_drill_write_keyring_v1(salt, Path::new(dir)) {
            Ok(manifest) => {
                println!("testnet-12 drill {} keyring written: {}", salt.id(), manifest.display());
                std::process::exit(0);
            }
            Err(e) => {
                println!("--palw-drill-write-keyring: {e}");
                std::process::exit(1);
            }
        }
    }
    palw_drill_datadir_guard_or_exit_v1(args, salt.as_ref());
}

/// [`palw_drill_datadir_guard_v1`] on this node's `<appdir>/<network>/`, exiting on a refusal.
/// Runs from `create_core` (before the logger) and again from `create_core_with_runtime` (before
/// the database opens), so an entry point that skips the first still meets the second.
pub fn palw_drill_datadir_guard_or_exit_v1(args: &Args, salt: Option<&PalwDrillSaltV1>) {
    let prefixed = crate::daemon::get_app_dir_from_args(args).join(args.network().to_prefixed());
    let genesis =
        salt.map(|s| kaspa_consensus_core::config::drill::palw_t12_drill_genesis_block_v1(s).hash.to_string()).unwrap_or_default();
    if let Err(e) = palw_drill_datadir_guard_v1(&prefixed, salt, &genesis) {
        println!("{e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::Args;

    const SALT: &str = "53535353535353535353535353535353535353535353535353535353535353aa";

    fn parse(extra: &[&str]) -> Args {
        let mut argv = vec!["kaspad"];
        argv.extend_from_slice(extra);
        Args::parse(argv).expect("parses")
    }

    fn salt() -> PalwDrillSaltV1 {
        PalwDrillSaltV1::from_hex(SALT).unwrap()
    }

    fn refusal(args: &Args) -> String {
        match palw_drill_validate_args_v1(args) {
            Err(ConfigError::PalwDrillRefused(why)) => why,
            other => panic!("expected a drill refusal, got {other:?}"),
        }
    }

    fn seed_file(dir: &Path, name: &str, seed: &[u8; 32]) -> String {
        let path = dir.join(name);
        write_private(&path, faster_hex::hex_string(seed).as_bytes()).unwrap();
        path.to_str().unwrap().to_owned()
    }

    /// **The flag's own rules**: testnet-12 only, a well-formed salt, no discovery, an explicit peer
    /// list, no params override — and the node without the flag is untouched (the keyring export
    /// alone needs the salt). The flag is not read from the environment or a config file.
    #[test]
    fn t53_a_salted_node_is_testnet_12_with_no_discovery_and_the_shipping_rules() {
        let base = ["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311"];
        let with = |extra: &[&str]| {
            let mut argv: Vec<&str> = base.to_vec();
            argv.extend_from_slice(extra);
            parse(&argv)
        };
        let flag = format!("--palw-drill-genesis-salt={SALT}");
        assert!(palw_drill_validate_args_v1(&with(&[&flag])).is_ok(), "the minimal drill node");
        assert!(palw_drill_validate_args_v1(&with(&[])).is_ok(), "no salt, no drill rule");
        assert_eq!(palw_drill_salt_of_v1(&with(&[&flag])).unwrap(), Some(salt()));

        for (argv, needle) in [
            (vec!["--testnet", "--netsuffix=10", "--nodnsseed", "--addpeer=10.0.0.2:26311", flag.as_str()], "testnet-12 only"),
            (vec!["--devnet", "--nodnsseed", "--addpeer=10.0.0.2:26311", flag.as_str()], "testnet-12 only"),
            (vec!["--nodnsseed", "--addpeer=10.0.0.2:26311", flag.as_str()], "testnet-12 only"),
            (vec!["--testnet", "--netsuffix=12", "--addpeer=10.0.0.2:26311", flag.as_str()], "--nodnsseed"),
            (vec!["--testnet", "--netsuffix=12", "--nodnsseed", flag.as_str()], "explicit peer list"),
        ] {
            let why = refusal(&parse(&argv));
            assert!(why.contains(needle), "{argv:?}: {why}");
        }
        let why = refusal(&with(&[&flag, "--override-params-file=/tmp/p.json"]));
        assert!(why.contains("--override-params-file"), "{why}");
        let why = refusal(&with(&["--palw-drill-genesis-salt=00"]));
        assert!(why.contains("64 hex characters") || why.contains("hex"), "{why}");
        let why = refusal(&with(&[&format!("--palw-drill-genesis-salt={}", "00".repeat(32))]));
        assert!(why.contains("all zero"), "{why}");
        let why = refusal(&with(&["--palw-drill-write-keyring=/tmp/k"]));
        assert!(why.contains("needs --palw-drill-genesis-salt"), "{why}");
        // The export dials nobody: it needs the salt and testnet-12, nothing else.
        assert!(
            palw_drill_validate_args_v1(&parse(&["--testnet", "--netsuffix=12", &flag, "--palw-drill-write-keyring=/tmp/k"])).is_ok()
        );

        // Wired into the start-up check every entry point runs.
        assert!(matches!(
            crate::daemon::validate_args(&parse(&["--testnet", "--netsuffix=10", flag.as_str()])),
            Err(ConfigError::PalwDrillRefused(_))
        ));

        // Command line only: no environment variable and no config-file key reaches it.
        let help = crate::args::cli().render_long_help().to_string();
        assert!(help.contains("--palw-drill-genesis-salt"));
        assert!(!help.contains("KASPAD_PALW_DRILL_GENESIS_SALT"), "never from the environment");
        let from_file: Result<Args, _> = toml::from_str(&format!("palw-drill-genesis-salt = \"{SALT}\""));
        assert!(from_file.is_err(), "never from a config file: an unknown key is refused");

        // The private-drill predicate the injectors read.
        let public = with(&[]);
        assert!(!palw_private_drill_network_v1(public.network(), &public), "public testnet-12 is not a drill");
        let salted = with(&[&flag]);
        assert!(palw_private_drill_network_v1(salted.network(), &salted));
        let devnet = parse(&["--devnet"]);
        assert!(palw_private_drill_network_v1(devnet.network(), &devnet));
    }

    /// **Drill-only keys, by the keyring**: a drill bond seed and drill addresses pass; a card key,
    /// another drill's key, a non-drill address and a public genesis outpoint are refused by name.
    #[test]
    fn t53_a_salted_node_signs_and_is_paid_only_with_drill_keys() {
        let dir = tempfile::tempdir().unwrap();
        let ring = PalwDrillKeyringV1::new(salt());
        let other = PalwDrillKeyringV1::new(PalwDrillSaltV1::from_bytes([0x35; 32]).unwrap());
        let drill_bond = seed_file(dir.path(), "drill-bond", &ring.key(PalwDrillKeyRoleV1::Bond, 3).seed);
        let drill_operator = seed_file(dir.path(), "drill-operator", &ring.key(PalwDrillKeyRoleV1::Operator, 3).seed);
        let foreign_bond = seed_file(dir.path(), "foreign-bond", &other.key(PalwDrillKeyRoleV1::Bond, 3).seed);
        let heartbeat = ring.key(PalwDrillKeyRoleV1::Heartbeat, 0).address(Prefix::Testnet).to_string();
        let payout = ring.key(PalwDrillKeyRoleV1::Payout, 1).address(Prefix::Testnet).to_string();
        let foreign = other.key(PalwDrillKeyRoleV1::Heartbeat, 0).address(Prefix::Testnet).to_string();
        let card = Address::new(
            Prefix::Testnet,
            Version::PubKeyHashMlDsa87,
            &kaspa_consensus_core::config::params::PALW_T12_GENESIS_BONDS[0].payout_payload,
        )
        .to_string();
        let drill_seat = palw_t12_drill_premine_outpoint_v1(&salt(), 0);
        let public_seat = kaspa_consensus_core::config::premine::premine_outpoint_for(palw_drill_network_v1(), 0);

        let node = |extra: &[String]| {
            let mut argv: Vec<String> =
                ["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311"].iter().map(|s| s.to_string()).collect();
            argv.push(format!("--palw-drill-genesis-salt={SALT}"));
            argv.extend_from_slice(extra);
            let refs: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
            parse(&refs)
        };
        let ok = node(&[
            format!("--palw-producer-key={drill_bond}"),
            format!("--palw-producer-bond={}:{}", drill_seat.transaction_id, drill_seat.index),
            format!("--palw-producer-pay-address={payout}"),
            format!("--palw-heartbeat-miner-address={heartbeat}"),
        ]);
        assert!(palw_drill_validate_args_v1(&ok).is_ok(), "{:?}", palw_drill_validate_args_v1(&ok));

        for (extra, needle) in [
            (format!("--palw-producer-key={drill_operator}"), "is not a bond key of drill"),
            (format!("--palw-producer-key={foreign_bond}"), "is not a bond key of drill"),
            (format!("--palw-heartbeat-miner-address={foreign}"), "is not an address of drill"),
            (format!("--palw-heartbeat-miner-address={card}"), "is not an address of drill"),
            (format!("--palw-producer-pay-address={card}"), "is not an address of drill"),
            (
                format!("--palw-producer-bond={}:{}", public_seat.transaction_id, public_seat.index),
                "names public testnet-12's genesis txid",
            ),
            (format!("--palw-fee-outpoint={}:41", public_seat.transaction_id), "names public testnet-12's genesis txid"),
        ] {
            let why = refusal(&node(&[extra.clone()]));
            assert!(why.contains(needle), "{extra}: {why}");
        }
    }

    /// **The app-directory marker**: a fresh directory (the logger's `logs/` aside) takes a drill
    /// and keeps it; a directory with another node's data and no marker refuses a drill; another
    /// drill's marker refuses this one; an unsalted node refuses a drill's directory; an unsalted
    /// node's own directory is untouched.
    #[test]
    fn t53_a_drill_and_a_public_node_never_share_an_app_directory() {
        let (a, b) = (salt(), PalwDrillSaltV1::from_bytes([0x35; 32]).unwrap());
        let root = tempfile::tempdir().unwrap();

        let fresh = root.path().join("fresh/misaka-testnet-12");
        std::fs::create_dir_all(fresh.join("logs")).unwrap();
        palw_drill_datadir_guard_v1(&fresh, Some(&a), "g").expect("a fresh app dir takes the drill");
        assert!(std::fs::read_to_string(fresh.join(PALW_DRILL_DATADIR_MARKER_V1)).unwrap().contains(&format!("salt_id={}", a.id())));
        std::fs::create_dir_all(fresh.join("datadir")).unwrap();
        palw_drill_datadir_guard_v1(&fresh, Some(&a), "g").expect("and keeps it across restarts");
        let why = palw_drill_datadir_guard_v1(&fresh, Some(&b), "g").unwrap_err();
        assert!(why.contains(&a.id()) && why.contains(&b.id()), "{why}");
        let why = palw_drill_datadir_guard_v1(&fresh, None, "").unwrap_err();
        assert!(why.contains("holds the chain of testnet-12 drill"), "{why}");

        let public = root.path().join("public/misaka-testnet-12");
        std::fs::create_dir_all(public.join("datadir")).unwrap();
        std::fs::create_dir_all(public.join("palw-panel")).unwrap();
        palw_drill_datadir_guard_v1(&public, None, "").expect("a public node's own directory is untouched");
        let why = palw_drill_datadir_guard_v1(&public, Some(&a), "g").unwrap_err();
        assert!(why.contains("may be a public testnet-12 node's") && why.contains("delete"), "{why}");
        assert!(!public.join(PALW_DRILL_DATADIR_MARKER_V1).exists(), "a refused drill writes nothing");

        let absent = root.path().join("absent/misaka-testnet-12");
        palw_drill_datadir_guard_v1(&absent, None, "").expect("nothing there, nothing to refuse");
        palw_drill_datadir_guard_v1(&absent, Some(&a), "g").expect("created with its marker");
        assert!(absent.join(PALW_DRILL_DATADIR_MARKER_V1).exists());
    }

    /// **The keyring export is the binary's own answer**: seeds kaspad accepts (0600, the seat's
    /// bond key), outpoints on the drill premine, the drill genesis beside public testnet-12's; a
    /// directory holding another drill's keyring is refused, the same drill's is rewritten.
    #[test]
    fn t53_the_keyring_export_names_the_drill_and_its_seats() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = palw_drill_write_keyring_v1(&salt(), dir.path()).expect("written");
        let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
        assert_eq!(manifest["format"], PALW_DRILL_KEYRING_FORMAT_V1);
        assert_eq!(manifest["salt_id"], salt().id());
        assert_eq!(manifest["network"], "testnet-12");
        let drill_genesis = kaspa_consensus_core::config::drill::palw_t12_drill_genesis_block_v1(&salt()).hash.to_string();
        assert_eq!(manifest["genesis_hash"], drill_genesis.as_str());
        assert_ne!(manifest["genesis_hash"], manifest["public_genesis_hash"]);
        assert_ne!(manifest["consensus_params_id"], manifest["public_consensus_params_id"]);
        let seats = manifest["seats"].as_array().unwrap();
        assert_eq!(seats.len(), kaspa_consensus_core::config::params::PALW_T12_GENESIS_BONDS.len());
        let premine = manifest["premine_txid"].as_str().unwrap();
        for seat in seats {
            let bond = crate::palw_producer::parse_outpoint(seat["bond_outpoint"].as_str().unwrap()).unwrap();
            assert_eq!(bond.transaction_id.to_string(), premine);
            let seed_path = dir.path().join(seat["seed_file"].as_str().unwrap());
            let seed =
                kaspa_pq_validator_core::load_validator_seed(seed_path.to_str().unwrap()).expect("kaspad accepts the seed file");
            let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed);
            assert_eq!(key.funding_address(Prefix::Testnet).to_string(), seat["address"].as_str().unwrap(), "the seat's own address");
        }
        assert_eq!(manifest["bonds"].as_array().unwrap().len() + seats.len(), PALW_DRILL_EXPORT_PER_ROLE_V1 as usize);
        assert_eq!(manifest["heartbeat"].as_array().unwrap().len(), PALW_DRILL_EXPORT_PER_ROLE_V1 as usize);

        // A drill node configured from the manifest passes the drill-only key rule.
        let seat0 = &seats[0];
        let args = {
            let argv = [
                "--testnet".to_owned(),
                "--netsuffix=12".to_owned(),
                "--nodnsseed".to_owned(),
                "--addpeer=10.0.0.2:26311".to_owned(),
                format!("--palw-drill-genesis-salt={SALT}"),
                format!("--palw-producer-key={}", dir.path().join(seat0["seed_file"].as_str().unwrap()).display()),
                format!("--palw-producer-bond={}", seat0["bond_outpoint"].as_str().unwrap()),
                format!("--palw-fee-outpoint={}", seat0["fee_float_outpoint"].as_str().unwrap()),
                format!("--palw-heartbeat-miner-address={}", manifest["heartbeat"][0]["address"].as_str().unwrap()),
            ];
            let refs: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
            parse(&refs)
        };
        assert!(palw_drill_validate_args_v1(&args).is_ok(), "{:?}", palw_drill_validate_args_v1(&args));

        palw_drill_write_keyring_v1(&salt(), dir.path()).expect("the same drill rewrites its keyring");
        let other = PalwDrillSaltV1::from_bytes([0x35; 32]).unwrap();
        let why = palw_drill_write_keyring_v1(&other, dir.path()).unwrap_err();
        assert!(why.contains("another drill's keyring"), "{why}");
    }
}
