//! mock-ticket — WIRING-ONLY (non-inference) ticket helper for the Phase-0 closed
//! two-node PALW testnet harness (`scripts/palw-shared-testnet/`).
//!
//! It produces the ticket cryptography that a real provider inference tool would emit
//! for a leaf, so a MOCK leaf can be minted end-to-end on a no-GPU box. It delegates
//! ENTIRELY to the real consensus/validator functions, so its output is byte-identical
//! to what consensus verifies and the miner loads:
//!   * `ticket_nullifier_commitment` — the exact
//!     `kaspa_consensus_core::palw::ticket_nullifier_commitment` (keyed BLAKE2b-512 over
//!     the raw nullifier under domain "misaka-palw-ticket-nf-commit-v1").
//!   * `ticket_authority_pk_hash` — `blake2b_512_keyed(PALW_AUTHORIZATION_DOMAIN, vk)`,
//!     which is the exact consensus clause-7 check (consensus/core/src/palw.rs) and the
//!     miner's `TicketAuthority::pk_hash` (kaspad/src/palw_mine_service.rs).
//!   * the raw nullifier is recorded into the miner's authority-bound `TicketSecretStore`
//!     via its own `record_and_flush`, so the on-disk key layout matches what the miner
//!     reads with `secret_for(batch_id, leaf_index)`.
//!
//! HONESTY: this NEVER runs real inference, NEVER fabricates a leaf beyond its ticket
//! fields, and NEVER touches the seeded test-only `palw_demo` path. The raw nullifier
//! is a SECRET: it is read from a file and written only into the 0600 store — never logged.
//!
//! Subcommands (the contract `create-lifecycle.sh` drives):
//!   mock-ticket commit    --authority-key <seed> --nullifier-file <128hex> [--network <net>]
//!       -> stdout: `ticket_nullifier_commitment: <128hex>`
//!                  `ticket_authority_pk_hash:    <128hex>`
//!   mock-ticket store-add --authority-key <seed> --secret-file <store.json>
//!                         --batch-id <128hex> --leaf-index <u32> --nullifier-file <128hex>
//!                         [--network <net>]
//!       -> upserts (batch_id, leaf_index) -> nullifier into the authority-bound store.
//!
//! `--network` is accepted for CLI symmetry but does not affect any output: the ticket
//! domains are network-independent constants.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::exit;
use std::str::FromStr;

use kaspa_consensus_core::palw::{PALW_AUTHORIZATION_DOMAIN, ticket_nullifier_commitment};
use kaspa_hashes::{Hash64, ZERO_HASH64, blake2b_512_keyed};
use kaspa_pq_validator_core::{TicketSecretStore, ValidatorKey, load_validator_seed};

fn die(msg: impl AsRef<str>) -> ! {
    eprintln!("mock-ticket: error: {}", msg.as_ref());
    exit(1);
}

/// Parse `--flag value` pairs (after the subcommand). Fail-closed on anything else.
fn parse_flags(args: &[String]) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        match a.strip_prefix("--") {
            Some(name) => {
                let val = args.get(i + 1).cloned().unwrap_or_else(|| die(format!("flag --{name} needs a value")));
                m.insert(name.to_string(), val);
                i += 2;
            }
            None => die(format!("unexpected argument '{a}' (flags must look like --name value)")),
        }
    }
    m
}

fn require<'a>(m: &'a HashMap<String, String>, k: &str) -> &'a str {
    m.get(k).map(String::as_str).unwrap_or_else(|| die(format!("missing required --{k}")))
}

/// Load the ticket authority the SAME way the miner does
/// (kaspad/src/palw_mine_service.rs::load_ticket_authority): `load_validator_seed` ->
/// `ValidatorKey::from_seed`, then keyed-BLAKE2b over the ML-DSA-87 verification key
/// under `PALW_AUTHORIZATION_DOMAIN` — the exact value the leaf's
/// `ticket_authority_pk_hash` must carry for clause 7 to accept the minted block.
fn authority_pk_hash(seed_path: &str) -> Hash64 {
    let seed = load_validator_seed(seed_path).unwrap_or_else(|e| die(format!("cannot load authority seed '{seed_path}': {e}")));
    let key = ValidatorKey::from_seed(seed);
    blake2b_512_keyed(PALW_AUTHORIZATION_DOMAIN, key.public_key())
}

/// Fail-closed guard for a SECRET input file. Mirrors the validator core's private
/// `require_private_regular_file` (kaspa-pq-validator-core/src/lib.rs) — which is not
/// exported, so we reproduce its exact contract here: refuse a non-regular file
/// (symlink/device/fifo — `symlink_metadata` does NOT follow the link) and a
/// group/world-readable mode. The seed leg already enforces this via
/// `load_validator_seed` and the store leg via `TicketSecretStore::load_or_empty`;
/// this closes the nullifier leg the README §4 fail-closed list promises.
fn require_private_regular_file(path: &str, label: &str) {
    let meta = std::fs::symlink_metadata(path).unwrap_or_else(|e| die(format!("cannot stat {label} file '{path}': {e}")));
    if !meta.file_type().is_file() {
        die(format!("{label} file '{path}' is not a regular file (symlink/device/fifo refused)"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            die(format!("{label} file '{path}' is group/world-accessible (mode {mode:o}); restrict it to 0600 (chmod 600)"));
        }
    }
}

/// Read a trimmed 128-hex nullifier (64 bytes) from a file into a `Hash64`.
fn read_nullifier(path: &str) -> Hash64 {
    // The raw nullifier is a SECRET (see module docs): enforce the same private-file
    // contract as the seed/store legs BEFORE reading it, per README §4.
    require_private_regular_file(path, "--nullifier-file");
    let raw = std::fs::read_to_string(path).unwrap_or_else(|e| die(format!("cannot read --nullifier-file '{path}': {e}")));
    Hash64::from_str(raw.trim()).unwrap_or_else(|e| die(format!("--nullifier-file '{path}' is not a 128-hex Hash64: {e:?}")))
}

fn cmd_commit(args: &[String]) {
    let f = parse_flags(args);
    let pk_hash = authority_pk_hash(require(&f, "authority-key"));
    let nullifier = read_nullifier(require(&f, "nullifier-file"));
    let commitment = ticket_nullifier_commitment(&nullifier);
    // Exactly the two labels create-lifecycle.sh's _kv parser expects. The raw
    // nullifier itself is never printed.
    println!("ticket_nullifier_commitment: {commitment}");
    println!("ticket_authority_pk_hash:    {pk_hash}");
}

fn cmd_store_add(args: &[String]) {
    let f = parse_flags(args);
    let pk_hash = authority_pk_hash(require(&f, "authority-key"));
    let secret_file = PathBuf::from(require(&f, "secret-file"));
    let batch_id =
        Hash64::from_str(require(&f, "batch-id")).unwrap_or_else(|e| die(format!("--batch-id is not a 128-hex Hash64: {e:?}")));
    // README §4 fail-closed: the all-zero Hash64 is the "unset" sentinel, never a real
    // content-derived batch id. Keying a ticket secret under it would silently collide
    // every unset batch, so refuse it here (create-lifecycle.sh guards this too; this is
    // the binary-level defense-in-depth the README promises).
    if batch_id == ZERO_HASH64 {
        die("won't key a ticket secret under the all-zero batch_id; pass the real content-derived batch_id from batch-manifest");
    }
    let leaf_index: u32 = require(&f, "leaf-index").parse().unwrap_or_else(|e| die(format!("--leaf-index is not a u32: {e}")));
    let nullifier = read_nullifier(require(&f, "nullifier-file"));
    // load_or_empty refuses a store belonging to a DIFFERENT authority; record_and_flush
    // refuses to overwrite an existing entry with a different value (a registered leaf's
    // nullifier is immutable). Both are consensus-safety properties we deliberately keep.
    let mut store = TicketSecretStore::load_or_empty(secret_file, pk_hash).unwrap_or_else(|e| die(e));
    store.record_and_flush(batch_id, leaf_index, nullifier).unwrap_or_else(|e| die(e));
    // Public identifiers only; never the nullifier value.
    eprintln!("mock-ticket: recorded (batch_id, leaf_index={leaf_index}) into the authority-bound ticket-secret store.");
}

/// The ticket-authority pk-hash derived from an in-memory 32-byte seed, WITHOUT touching the
/// filesystem. Byte-identical to [`authority_pk_hash`] (the file path leg): both keyed-BLAKE2b the
/// `ValidatorKey`'s ML-DSA-87 verification key under `PALW_AUTHORIZATION_DOMAIN`. Used by the seam
/// test to compare against the miner authority without a temp file.
#[cfg(test)]
fn authority_pk_hash_from_seed(seed: [u8; 32]) -> Hash64 {
    let key = ValidatorKey::from_seed(seed);
    blake2b_512_keyed(PALW_AUTHORIZATION_DOMAIN, key.public_key())
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let sub = argv.get(1).map(String::as_str).unwrap_or("");
    let rest: &[String] = if argv.len() > 2 { &argv[2..] } else { &[] };
    match sub {
        "commit" => cmd_commit(rest),
        "store-add" => cmd_store_add(rest),
        _ => {
            eprintln!(
                "mock-ticket (WIRING-ONLY, non-inference) — Phase-0 PALW harness helper\n\
                 usage:\n  \
                 mock-ticket commit    --authority-key <seed> --nullifier-file <128hex> [--network <net>]\n  \
                 mock-ticket store-add --authority-key <seed> --secret-file <store.json> \
                 --batch-id <128hex> --leaf-index <u32> --nullifier-file <128hex> [--network <net>]"
            );
            exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_miner::authorization::{TicketAuthority, ticket_authority_pk_hash};

    // A fixed seed + nullifier for the seam checks (values are arbitrary; only the cross-crate
    // EQUALITIES matter). Not the harness's real authority key.
    const SEED: [u8; 32] = [0x9a; 32];

    /// THE seam this helper exists to keep honest: the pk-hash mock-ticket stamps into the leaf's
    /// `ticket_authority_pk_hash` MUST equal the pk-hash of the miner authority that actually signs
    /// the algo-4 block. If these diverge by one byte the leaf is admitted on-chain but every block
    /// referencing it fails consensus clause 7 — a silent dead ticket. Three independent derivations
    /// (this helper, the miner's `TicketAuthority::pk_hash`, the miner's free `ticket_authority_pk_hash`
    /// over the same vk) must all agree.
    #[test]
    fn helper_authority_pk_hash_equals_the_miner_signing_authority() {
        let helper = authority_pk_hash_from_seed(SEED);

        let miner_authority = TicketAuthority::from_seed(SEED);
        assert_eq!(
            helper,
            miner_authority.pk_hash(),
            "mock-ticket's authority pk-hash diverged from the miner's TicketAuthority::pk_hash — leaves would be unmineable (clause 7)"
        );
        assert_eq!(
            helper,
            ticket_authority_pk_hash(miner_authority.public_key()),
            "mock-ticket's authority pk-hash diverged from the miner's free ticket_authority_pk_hash over the same verification key"
        );
    }

    /// The store round trip is exactly what the running miner does: mock-ticket `store-add` writes an
    /// authority-bound `TicketSecretStore`; the node reads it back with `secret_for(batch_id, leaf)`.
    /// The recovered raw nullifier must open the leaf's on-chain commitment
    /// (`ticket_nullifier_commitment`) — i.e. the disclosed value the minted block carries is the one
    /// the leaf committed to.
    #[test]
    fn store_roundtrip_recovers_the_nullifier_that_opens_the_leaf_commitment() {
        let dir = std::env::temp_dir().join(format!("mock-ticket-seam-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store_path = dir.join("ticket-secret.json");
        let _ = std::fs::remove_file(&store_path);

        let pk_hash = authority_pk_hash_from_seed(SEED);
        let batch_id = Hash64::from_str(&"ab".repeat(64)).unwrap();
        let nullifier = Hash64::from_str(&"42".repeat(64)).unwrap();
        let leaf_commitment = ticket_nullifier_commitment(&nullifier);

        // write exactly as `store-add` does
        let mut store = TicketSecretStore::load_or_empty(store_path.clone(), pk_hash).unwrap();
        store.record_and_flush(batch_id, 0, nullifier).unwrap();

        // read exactly as the miner service does
        let reader = TicketSecretStore::load_or_empty(store_path.clone(), pk_hash).unwrap();
        let recovered = reader.secret_for(&batch_id, 0).expect("the miner must recover the stored nullifier for (batch, leaf)");
        assert_eq!(recovered, nullifier, "the store round-tripped a different nullifier than was recorded");
        assert_eq!(
            ticket_nullifier_commitment(&recovered),
            leaf_commitment,
            "the recovered nullifier does not open the leaf's on-chain commitment — the minted block's disclosure would be rejected"
        );

        // a foreign authority cannot read this store (dead-ticket / key-mixing guard)
        let foreign = authority_pk_hash_from_seed([0x11; 32]);
        assert!(TicketSecretStore::load_or_empty(store_path, foreign).is_err(), "the store must refuse a different ticket authority");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
