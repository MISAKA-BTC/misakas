//! **The drill genesis salt and the drill keyring** — ADR-0152 §8.2 ("the drill replay rule"),
//! `phase2-plan.md` §1.8 and P2-12; tested by T53.
//!
//! A drill of testnet-12 runs the SHIPPING binary, with the shipping rules, on a chain nobody else is
//! on. "Nobody else" is three separations, and each closes a different replay:
//!
//! 1. **The salt moves the genesis.** `config::premine::palw_t12_drill_premine_txid_v1` and
//!    `palw_t12_drill_community_txid_v1` are public testnet-12's own derivations with the salt
//!    appended, so every genesis outpoint moves, the `utxo_commitment` moves and the genesis hash
//!    moves. A drill transaction spends outpoints public testnet-12 never minted: refused there as a
//!    missing UTXO. The handshake refuses the cross-peer on the genesis hash before anything else
//!    (`WrongGenesis`), and `consensus_params_id` hashes the genesis too.
//! 2. **The genesis hash moves the network domain.** Every V2 signature — a bond registration, an
//!    attempt's challenge, a receipt, a DA accusation, a conviction's evidence — is separated by
//!    `palw_network_domain_v2_for(network id, genesis)` (audit M2-18), so a drill object verifies
//!    against nothing on public testnet-12 even where it names something that exists there.
//! 3. **Drill-only keys.** Neither 1 nor 2 binds a TRANSACTION signature to a chain: the ML-DSA
//!    sighash commits to the spent outpoint and to neither the network nor the genesis. An outpoint
//!    a coinbase mints is a function of that coinbase's content, so two chains whose coinbases pay
//!    the same script at the same blue score mint the same outpoint, and a spend of one is a spend
//!    of the other (the memory rule "premine spends replay across chains sharing the card", and the
//!    reason ADR-0152 §8.2 forbids spending a card key's premine on any private chain). Every key a
//!    drill signs or is paid with is therefore derived here from the salt ([`PalwDrillKeyringV1`]):
//!    the genesis seats' bond and operator keys, their payouts and fee floats, the main wallet, the
//!    heartbeat miners' addresses and the extra bonds a drill registers (D-9, D-10). No card key and
//!    no address public testnet-12 pays is one of them, and kaspad refuses a salted node configured
//!    with any other (`kaspad/src/palw_drill.rs`).
//!
//! **The salt never applies by accident.** It is an explicit argument of every function below — no
//! global, no environment variable, and `Params::from(testnet-12)` never reads it. It exists only on
//! testnet-12 ([`palw_drill_network_v1`]). kaspad takes it from the command line alone
//! (`--palw-drill-genesis-salt`, not from a config file or the environment), puts the salted params
//! and the salt into the node's `Config` together, and the start-up guard
//! (`consensus::utxo_set_override::set_genesis_utxo_commitment_from_config`) accepts a salted
//! genesis only when the salt is set and the public one only when it is not.
use crate::{
    config::{
        genesis::{GenesisBlock, PALW_T12_GENESIS},
        params::PALW_T12_GENESIS_BONDS,
        premine::{
            MAIN_PREMINE_INDEX, palw_t12_drill_bonded_utxos_v1, palw_t12_drill_premine_txid_v1, premine_txid_for,
            testnet12_community_txid,
        },
    },
    header::Header,
    muhash::MuHashExtensions,
    network::{NetworkId, NetworkType},
    palw_fp_devnet_v3::PalwGenesisBondSpecV1,
    palw_state_v2::PalwBondKeyV2,
    tx::TransactionOutpoint,
    utxo::utxo_collection::UtxoCollection,
};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_hashes::Hash64;
use kaspa_muhash::MuHash;

/// The salt's width: 32 bytes, 64 hex characters (`openssl rand -hex 32`). One width, so a salt is
/// one string an operator copies between hosts and never a prefix of another drill's.
pub const PALW_DRILL_SALT_LEN_V1: usize = 32;

/// **A drill's genesis salt.** Not a secret — the keys derived from it are value-less by
/// construction, like devnet's public-seed bonds — but a NAME: two drills with different salts are
/// two chains that refuse each other, and a salt is refused all-zero so no drill gets the salt
/// everybody would type.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PalwDrillSaltV1([u8; PALW_DRILL_SALT_LEN_V1]);

/// Why a drill salt was refused. Each names the fix.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwDrillSaltErrorV1 {
    #[error(
        "the drill genesis salt must be {} bytes as hex ({} hex characters), got {got} characters — generate one with `openssl rand -hex 32`",
        PALW_DRILL_SALT_LEN_V1,
        2 * PALW_DRILL_SALT_LEN_V1
    )]
    WrongLength { got: usize },
    #[error("the drill genesis salt is not hex: {0}")]
    NotHex(String),
    #[error(
        "the drill genesis salt is all zero — a salt everyone would type is a chain everyone shares; generate one with `openssl rand -hex 32`"
    )]
    AllZero,
}

/// The keyed-hash domain of [`PalwDrillSaltV1::id`].
const PALW_DRILL_SALT_ID_DOMAIN_V1: &[u8] = b"misaka-palw-drill-salt-id/v1";

impl PalwDrillSaltV1 {
    pub fn from_bytes(bytes: [u8; PALW_DRILL_SALT_LEN_V1]) -> Result<Self, PalwDrillSaltErrorV1> {
        if bytes.iter().all(|b| *b == 0) {
            return Err(PalwDrillSaltErrorV1::AllZero);
        }
        Ok(Self(bytes))
    }

    /// Parse the command line's form: exactly 64 hex characters (surrounding whitespace ignored).
    pub fn from_hex(text: &str) -> Result<Self, PalwDrillSaltErrorV1> {
        let text = text.trim();
        if text.len() != 2 * PALW_DRILL_SALT_LEN_V1 {
            return Err(PalwDrillSaltErrorV1::WrongLength { got: text.len() });
        }
        let mut bytes = [0u8; PALW_DRILL_SALT_LEN_V1];
        faster_hex::hex_decode(text.as_bytes(), &mut bytes).map_err(|e| PalwDrillSaltErrorV1::NotHex(e.to_string()))?;
        Self::from_bytes(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; PALW_DRILL_SALT_LEN_V1] {
        &self.0
    }

    /// **The salt's short name** — 16 hex characters of a keyed hash, what the logs, the datadir
    /// marker and the keyring manifest print. A different drill has a different id; the id is not
    /// the salt, so printing it names the chain without handing anybody the string that joins it.
    pub fn id(&self) -> String {
        let hash = blake2b_simd::Params::new().hash_length(8).key(PALW_DRILL_SALT_ID_DOMAIN_V1).to_state().update(&self.0).finalize();
        faster_hex::hex_string(hash.as_bytes())
    }
}

impl std::fmt::Debug for PalwDrillSaltV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PalwDrillSaltV1(id {})", self.id())
    }
}

impl std::str::FromStr for PalwDrillSaltV1 {
    type Err = PalwDrillSaltErrorV1;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

/// **The only network a drill salt applies to: testnet-12.** A salt drills the network whose
/// shipping rules it keeps; mainnet, testnet-11 (which this build refuses to run anyway, T80),
/// testnet-10, devnet and simnet have no drill genesis, and kaspad refuses the flag there.
pub fn palw_drill_network_v1() -> NetworkId {
    NetworkId::with_suffix(NetworkType::Testnet, 12)
}

/// **What a drill key is for.** Each role is its own derivation, so one role's key is never another's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwDrillKeyRoleV1 {
    /// The drill's main wallet — the 10B remainder, what funds D-9's and D-10's registrations.
    Main,
    /// A seat's signing key: a genesis card's `bond_pubkey` (n < the card count) or a bond a drill
    /// registers (D-9, D-10). A seat's payout and fee float are this key's own address, as on the
    /// public card, so the seed that signs for a seat also spends what it earns.
    Bond,
    /// A genesis card's operator identity (one per seat, as `derive_panel_v2` needs).
    Operator,
    /// A payout address that is not a bond's own (`--palw-producer-pay-address`).
    Payout,
    /// A heartbeat miner's fee address (`--palw-heartbeat-miner-address`).
    Heartbeat,
}

impl PalwDrillKeyRoleV1 {
    pub const ALL: [Self; 5] = [Self::Main, Self::Bond, Self::Operator, Self::Payout, Self::Heartbeat];

    pub fn name(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Bond => "bond",
            Self::Operator => "operator",
            Self::Payout => "payout",
            Self::Heartbeat => "heartbeat",
        }
    }
}

/// **How many keys of each role a drill keyring holds** — the indices `0..32` of every role. A
/// genesis registry of 8 seats, the D-9 and D-10 registrations and a heartbeat miner per host fit
/// with room to spare; a bound keeps "is this a drill key?" a finite search that kaspad answers at
/// start-up.
pub const PALW_DRILL_KEYRING_SPAN_V1: u32 = 32;

/// The keyed-hash domain of every drill key seed.
const PALW_DRILL_KEY_SEED_DOMAIN_V1: &[u8] = b"misaka-palw-drill-key-seed/v1";

/// **The 32-byte ML-DSA-87 seed of drill key `(role, n)`**:
/// `BLAKE2b-256(key = "misaka-palw-drill-key-seed/v1", len(role) ‖ role ‖ salt ‖ n)`. The seed file
/// kaspad reads (`--palw-producer-key`) is this, as hex.
pub fn palw_drill_key_seed_v1(salt: &PalwDrillSaltV1, role: PalwDrillKeyRoleV1, n: u32) -> [u8; 32] {
    let name = role.name().as_bytes();
    let mut state = blake2b_simd::Params::new().hash_length(32).key(PALW_DRILL_KEY_SEED_DOMAIN_V1).to_state();
    state.update(&(name.len() as u64).to_le_bytes());
    state.update(name);
    state.update(salt.as_bytes());
    state.update(&n.to_le_bytes());
    let mut seed = [0u8; 32];
    seed.copy_from_slice(state.finalize().as_bytes());
    seed
}

/// **One drill key**: its seed, its ML-DSA-87 verification key and its P2PKH owner payload.
#[derive(Clone, PartialEq, Eq)]
pub struct PalwDrillKeyV1 {
    pub role: PalwDrillKeyRoleV1,
    pub n: u32,
    pub seed: [u8; 32],
    pub pubkey: Vec<u8>,
    /// `blake2b_512_address_payload(pubkey)` — what a P2PKH-ML-DSA-87 script and an address carry.
    pub payload: [u8; 64],
}

impl PalwDrillKeyV1 {
    pub fn derive(salt: &PalwDrillSaltV1, role: PalwDrillKeyRoleV1, n: u32) -> Self {
        let seed = palw_drill_key_seed_v1(salt, role, n);
        let keypair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed);
        let pubkey = keypair.verification_key.as_ref().to_vec();
        let payload = kaspa_hashes::blake2b_512_address_payload(&pubkey).as_bytes();
        Self { role, n, seed, pubkey, payload }
    }

    /// The key pair, re-derived from the seed (what signs).
    pub fn keypair(&self) -> libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair {
        libcrux_ml_dsa::ml_dsa_87::generate_key_pair(self.seed)
    }

    pub fn address(&self, prefix: Prefix) -> Address {
        Address::new(prefix, Version::PubKeyHashMlDsa87, &self.payload)
    }
}

impl std::fmt::Debug for PalwDrillKeyV1 {
    // The seed is value-less but it is a signing key; a log line never needs it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PalwDrillKeyV1({} {}, payload {})", self.role.name(), self.n, faster_hex::hex_string(&self.payload[..8]))
    }
}

/// **A drill's keyring**: every key [`palw_drill_key_seed_v1`] derives for one salt, indices
/// `0..PALW_DRILL_KEYRING_SPAN_V1` of every role. Keys are derived on demand (an ML-DSA-87 key
/// generation each), so a membership question costs at most `5 × 32` of them.
#[derive(Clone, Debug)]
pub struct PalwDrillKeyringV1 {
    salt: PalwDrillSaltV1,
}

impl PalwDrillKeyringV1 {
    pub fn new(salt: PalwDrillSaltV1) -> Self {
        Self { salt }
    }

    pub fn salt(&self) -> &PalwDrillSaltV1 {
        &self.salt
    }

    pub fn key(&self, role: PalwDrillKeyRoleV1, n: u32) -> PalwDrillKeyV1 {
        PalwDrillKeyV1::derive(&self.salt, role, n)
    }

    /// The index of the drill BOND key with this verification key — the only role a producer or a
    /// seat signs under — or `None` when the key is not one of this drill's.
    pub fn find_bond_pubkey(&self, pubkey: &[u8]) -> Option<u32> {
        (0..PALW_DRILL_KEYRING_SPAN_V1).find(|n| self.key(PalwDrillKeyRoleV1::Bond, *n).pubkey == pubkey)
    }

    /// The drill key (of any role) whose address carries this owner payload, or `None` when the
    /// payload is not one of this drill's — what a pay or heartbeat address must be.
    pub fn find_payload(&self, payload: &[u8]) -> Option<(PalwDrillKeyRoleV1, u32)> {
        PalwDrillKeyRoleV1::ALL
            .into_iter()
            .flat_map(|role| (0..PALW_DRILL_KEYRING_SPAN_V1).map(move |n| (role, n)))
            .find(|(role, n)| self.key(*role, *n).payload[..] == *payload)
    }
}

/// **A drill genesis seat**: public testnet-12's card `n`, re-keyed. The premine index — and so the
/// seat's position in the registry, its collateral amount and its fee float index — is the public
/// card's, so the drill runs the registry the network ships; only who holds the keys differs.
#[derive(Clone, Debug)]
pub struct PalwDrillCardV1 {
    pub premine_index: u32,
    /// `(Bond, n)`: signs attempts and receipts; its address is the seat's payout and fee float.
    pub bond: PalwDrillKeyV1,
    /// `(Operator, n)`: the seat's operator identity.
    pub operator: PalwDrillKeyV1,
}

/// The drill's genesis seats, one per public testnet-12 card, in the card table's order.
pub fn palw_t12_drill_cards_v1(salt: &PalwDrillSaltV1) -> Vec<PalwDrillCardV1> {
    PALW_T12_GENESIS_BONDS
        .iter()
        .enumerate()
        .map(|(n, card)| PalwDrillCardV1 {
            premine_index: card.premine_index,
            bond: PalwDrillKeyV1::derive(salt, PalwDrillKeyRoleV1::Bond, n as u32),
            operator: PalwDrillKeyV1::derive(salt, PalwDrillKeyRoleV1::Operator, n as u32),
        })
        .collect()
}

/// The drill main wallet's key: `(Main, 0)`.
pub fn palw_t12_drill_main_key_v1(salt: &PalwDrillSaltV1) -> PalwDrillKeyV1 {
    PalwDrillKeyV1::derive(salt, PalwDrillKeyRoleV1::Main, 0)
}

/// The outpoint of drill premine output `index` — a genesis seat's bond identity at its premine
/// index, its fee float at `MAIN_PREMINE_INDEX + 1 + n`, the main wallet at `MAIN_PREMINE_INDEX`.
pub fn palw_t12_drill_premine_outpoint_v1(salt: &PalwDrillSaltV1, index: u32) -> TransactionOutpoint {
    TransactionOutpoint { transaction_id: palw_t12_drill_premine_txid_v1(salt), index }
}

/// The fee float outpoint of drill seat `n` (the `n`-th card, not its premine index).
pub fn palw_t12_drill_fee_float_outpoint_v1(salt: &PalwDrillSaltV1, n: u32) -> TransactionOutpoint {
    palw_t12_drill_premine_outpoint_v1(salt, MAIN_PREMINE_INDEX + 1 + n)
}

/// **The drill's genesis UTXO set** — public testnet-12's shape on the drill's txids and keys
/// (`config::premine::palw_t12_drill_bonded_utxos_v1`).
pub fn palw_t12_drill_genesis_utxos_v1(salt: &PalwDrillSaltV1) -> UtxoCollection {
    let rows: Vec<(u32, [u8; 64])> = palw_t12_drill_cards_v1(salt).iter().map(|c| (c.premine_index, c.bond.payload)).collect();
    palw_t12_drill_bonded_utxos_v1(salt, &rows, &palw_t12_drill_main_key_v1(salt).payload)
}

/// **The drill's genesis bond registry**: each seat named on the drill's premine txid, with the
/// drill's keys and its bond key's own address as payout.
pub fn palw_t12_drill_genesis_bonds_v1(salt: &PalwDrillSaltV1) -> Vec<PalwGenesisBondSpecV1> {
    palw_t12_drill_cards_v1(salt)
        .into_iter()
        .map(|c| PalwGenesisBondSpecV1 {
            bond: PalwBondKeyV2(palw_t12_drill_premine_outpoint_v1(salt, c.premine_index)),
            pubkey: c.bond.pubkey,
            operator_pubkey: c.operator.pubkey,
            payout_payload: Hash64::from_bytes(c.bond.payload),
        })
        .collect()
}

/// The coinbase payload of every drill genesis — [`PALW_T12_GENESIS`]'s layout with the marker
/// `misaka-palw-t12-drill`. A second, salt-independent separation from the public block (through
/// the merkle root), so a drill genesis is recognisable as one in its bytes; the salt's separation
/// is the `utxo_commitment`.
#[rustfmt::skip]
pub const PALW_T12_DRILL_GENESIS_COINBASE_PAYLOAD: &[u8] = &[
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
    0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
    0x00, 0x00, // Script version
    0x01,       // Varint
    0x00,       // OP-FALSE
    // "misaka-palw-t12-drill"
    0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x70, 0x61, 0x6c, 0x77, 0x2d, 0x74, 0x31, 0x32, 0x2d, 0x64, 0x72, 0x69, 0x6c, 0x6c,
];

/// **The drill's genesis block**, derived at start-up in the order `print_repinned_t12_genesis`
/// uses (each feeds the next): the merkle root over the drill coinbase, the `utxo_commitment` over
/// [`palw_t12_drill_genesis_utxos_v1`], then the hash over the completed header. Timestamp, bits,
/// version and DAA score are public testnet-12's.
pub fn palw_t12_drill_genesis_block_v1(salt: &PalwDrillSaltV1) -> GenesisBlock {
    let mut genesis = GenesisBlock { coinbase_payload: PALW_T12_DRILL_GENESIS_COINBASE_PAYLOAD, ..PALW_T12_GENESIS };
    genesis.hash_merkle_root = crate::merkle::calc_hash_merkle_root(genesis.build_genesis_transactions().iter());
    let mut multiset = MuHash::new();
    for (outpoint, entry) in palw_t12_drill_genesis_utxos_v1(salt) {
        multiset.add_utxo(&outpoint, &entry);
    }
    genesis.utxo_commitment = multiset.finalize();
    genesis.hash = Header::from(&genesis).hash;
    genesis
}

/// **Whether `txid` names public testnet-12's genesis** (its premine or community txid). A drill
/// node handed one — `--palw-producer-bond`, `--palw-fee-outpoint` — was given a public unit's
/// flags, and kaspad refuses it by name rather than letting it hold forever on a bond that does not
/// exist on the drill chain.
pub fn palw_t12_public_genesis_txid_v1(txid: &Hash64) -> bool {
    *txid == premine_txid_for(palw_drill_network_v1()) || *txid == testnet12_community_txid()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::premine::{
        MISAKA_PREMINE_CAP_SOMPI, PALW_RC_BOND_FEE_FLOAT_SOMPI, genesis_premine_utxos_for, palw_t12_drill_community_txid_v1,
    };

    fn salt(byte: u8) -> PalwDrillSaltV1 {
        PalwDrillSaltV1::from_bytes([byte; PALW_DRILL_SALT_LEN_V1]).unwrap()
    }

    /// **The command line's form, and every refusal names its fix.** 64 hex characters, not all
    /// zero; the id is a stable 16-hex name that differs between salts and is not the salt.
    #[test]
    fn t53_the_salt_parses_and_refuses_by_name() {
        let hex = "0123456789abcdef".repeat(4);
        let parsed = PalwDrillSaltV1::from_hex(&format!("  {hex}\n")).expect("64 hex characters");
        assert_eq!(faster_hex::hex_string(parsed.as_bytes()), hex);
        assert_eq!(hex.parse::<PalwDrillSaltV1>().unwrap(), parsed, "FromStr is from_hex");
        assert!(matches!(PalwDrillSaltV1::from_hex(&hex[..62]), Err(PalwDrillSaltErrorV1::WrongLength { got: 62 })));
        assert!(matches!(PalwDrillSaltV1::from_hex(&format!("{hex}00")), Err(PalwDrillSaltErrorV1::WrongLength { got: 66 })));
        assert!(matches!(PalwDrillSaltV1::from_hex(&"zz".repeat(32)), Err(PalwDrillSaltErrorV1::NotHex(_))));
        assert_eq!(PalwDrillSaltV1::from_hex(&"00".repeat(32)), Err(PalwDrillSaltErrorV1::AllZero));
        assert!(PalwDrillSaltErrorV1::AllZero.to_string().contains("openssl rand -hex 32"));

        let id = parsed.id();
        assert_eq!(id.len(), 16);
        assert_eq!(id, PalwDrillSaltV1::from_hex(&hex).unwrap().id(), "stable");
        assert_ne!(id, salt(7).id(), "a different drill has a different id");
        assert!(!hex.contains(&id), "the id is not a slice of the salt");
        assert_eq!(format!("{parsed:?}"), format!("PalwDrillSaltV1(id {id})"), "Debug prints the id, never the salt");
        assert_eq!(palw_drill_network_v1().to_string(), "testnet-12");
    }

    /// **Drill keys are deterministic per (salt, role, n), distinct across all three, and never a
    /// key or an address public testnet-12 knows** — no card's bond, operator or payout key, no main
    /// wallet, no community member. The keyring finds its own keys and nothing else.
    #[test]
    fn t53_drill_keys_are_the_salts_own_and_never_a_public_key() {
        let a = salt(0xA1);
        let b = salt(0xB2);
        let key = |s: &PalwDrillSaltV1, role, n| PalwDrillKeyV1::derive(s, role, n);
        assert_eq!(key(&a, PalwDrillKeyRoleV1::Bond, 3), key(&a, PalwDrillKeyRoleV1::Bond, 3), "deterministic");
        assert_ne!(key(&a, PalwDrillKeyRoleV1::Bond, 3).pubkey, key(&b, PalwDrillKeyRoleV1::Bond, 3).pubkey, "per salt");
        assert_ne!(key(&a, PalwDrillKeyRoleV1::Bond, 3).pubkey, key(&a, PalwDrillKeyRoleV1::Bond, 4).pubkey, "per index");
        let mut seeds = std::collections::BTreeSet::new();
        for role in PalwDrillKeyRoleV1::ALL {
            for n in 0..4 {
                assert!(seeds.insert(palw_drill_key_seed_v1(&a, role, n)), "{role:?} {n}: a seed belongs to one role and one index");
            }
        }
        let k = key(&a, PalwDrillKeyRoleV1::Heartbeat, 1);
        assert_eq!(k.keypair().verification_key.as_ref(), k.pubkey.as_slice(), "the seed re-derives the key");
        assert!(format!("{k:?}").starts_with("PalwDrillKeyV1(heartbeat 1"), "and Debug never prints it");

        // Nothing public testnet-12 knows.
        let mut public_keys: Vec<Vec<u8>> = Vec::new();
        let mut public_payloads: Vec<[u8; 64]> = Vec::new();
        for card in PALW_T12_GENESIS_BONDS {
            public_keys.push(card.bond_pubkey.to_vec());
            public_keys.push(card.operator_pubkey.to_vec());
            public_payloads.push(card.payout_payload);
        }
        for entry in genesis_premine_utxos_for(palw_drill_network_v1()).values() {
            // Every public genesis owner — the main wallet, the floats, the community — is a
            // 69-byte P2PKH whose payload sits at bytes 3..67.
            let script = entry.script_public_key.script();
            let mut payload = [0u8; 64];
            payload.copy_from_slice(&script[3..67]);
            public_payloads.push(payload);
        }
        let ring = PalwDrillKeyringV1::new(a);
        for role in PalwDrillKeyRoleV1::ALL {
            for n in 0..PALW_DRILL_KEYRING_SPAN_V1 {
                let k = ring.key(role, n);
                assert!(!public_keys.contains(&k.pubkey), "{role:?} {n} is a card key");
                assert!(!public_payloads.contains(&k.payload), "{role:?} {n} is an address public testnet-12 pays");
            }
        }

        // The keyring answers for its own keys, and for nothing else.
        assert_eq!(ring.find_bond_pubkey(&ring.key(PalwDrillKeyRoleV1::Bond, 9).pubkey), Some(9));
        assert_eq!(ring.find_bond_pubkey(&ring.key(PalwDrillKeyRoleV1::Operator, 9).pubkey), None, "only a bond key signs");
        assert_eq!(ring.find_bond_pubkey(&key(&b, PalwDrillKeyRoleV1::Bond, 9).pubkey), None, "another drill's key");
        assert_eq!(ring.find_bond_pubkey(PALW_T12_GENESIS_BONDS[0].bond_pubkey), None, "a card key");
        let last = PALW_DRILL_KEYRING_SPAN_V1 - 1;
        assert_eq!(
            ring.find_payload(&ring.key(PalwDrillKeyRoleV1::Heartbeat, last).payload),
            Some((PalwDrillKeyRoleV1::Heartbeat, last))
        );
        assert_eq!(ring.find_payload(&ring.key(PalwDrillKeyRoleV1::Heartbeat, last + 1).payload), None, "past the span");
        assert_eq!(ring.find_payload(&PALW_T12_GENESIS_BONDS[0].payout_payload), None, "a card's payout");
    }

    /// **The salt moves every genesis name and keeps every genesis amount.** The drill set is public
    /// testnet-12's shape — the 10B cap, the output count, the amounts — on txids and keys no public
    /// outpoint shares; the block moves in its merkle root, its commitment and its hash; two salts
    /// are two chains; and the public derivations are untouched.
    #[test]
    fn t53_the_drill_genesis_moves_every_name_and_keeps_the_cap() {
        let (a, b) = (salt(0x5A), salt(0x5B));
        let t12 = palw_drill_network_v1();
        let public = genesis_premine_utxos_for(t12);
        let drill = palw_t12_drill_genesis_utxos_v1(&a);

        // Names: the txids and every outpoint.
        let public_premine = premine_txid_for(t12);
        assert_ne!(palw_t12_drill_premine_txid_v1(&a), public_premine);
        assert_ne!(palw_t12_drill_community_txid_v1(&a), testnet12_community_txid());
        assert_ne!(palw_t12_drill_premine_txid_v1(&a), palw_t12_drill_premine_txid_v1(&b), "two salts, two chains");
        assert!(drill.keys().all(|o| !public.contains_key(o)), "no drill outpoint exists on public testnet-12");
        assert!(
            drill
                .keys()
                .all(|o| o.transaction_id == palw_t12_drill_premine_txid_v1(&a)
                    || o.transaction_id == palw_t12_drill_community_txid_v1(&a)),
            "every drill outpoint is on the drill's own txids"
        );
        assert!(palw_t12_public_genesis_txid_v1(&public_premine) && palw_t12_public_genesis_txid_v1(&testnet12_community_txid()));
        assert!(!palw_t12_public_genesis_txid_v1(&palw_t12_drill_premine_txid_v1(&a)));

        // Amounts: the cap, the count, the multiset.
        let total: u64 = drill.values().map(|e| e.amount).sum();
        assert_eq!(total, MISAKA_PREMINE_CAP_SOMPI, "a drill genesis mints exactly the 10B cap");
        assert_eq!(drill.len(), public.len());
        let amounts = |set: &UtxoCollection| {
            let mut v: Vec<u64> = set.values().map(|e| e.amount).collect();
            v.sort_unstable();
            v
        };
        assert_eq!(amounts(&drill), amounts(&public), "the same amounts: the drill runs the network's premine");

        // Keys: collateral at the drill main wallet, floats at each seat's bond address.
        let main_spk = crate::mldsa87_primitives::p2pkh_mldsa87_spk(&palw_t12_drill_main_key_v1(&a).payload);
        for (n, card) in palw_t12_drill_cards_v1(&a).iter().enumerate() {
            assert_eq!(card.premine_index, PALW_T12_GENESIS_BONDS[n].premine_index, "the public card's index");
            let collateral = &drill[&palw_t12_drill_premine_outpoint_v1(&a, card.premine_index)];
            assert_eq!(collateral.script_public_key, main_spk, "seat {n}'s collateral is the drill main wallet's");
            let float = &drill[&palw_t12_drill_fee_float_outpoint_v1(&a, n as u32)];
            assert_eq!(float.amount, PALW_RC_BOND_FEE_FLOAT_SOMPI);
            assert_eq!(float.script_public_key, crate::mldsa87_primitives::p2pkh_mldsa87_spk(&card.bond.payload));
        }
        let bonds = palw_t12_drill_genesis_bonds_v1(&a);
        assert_eq!(bonds.len(), PALW_T12_GENESIS_BONDS.len());
        assert!(bonds.iter().all(|b| b.bond.0.transaction_id == palw_t12_drill_premine_txid_v1(&a)));

        // The block.
        let block = palw_t12_drill_genesis_block_v1(&a);
        assert_ne!(block.hash, PALW_T12_GENESIS.hash, "the drill genesis is not public testnet-12's");
        assert_ne!(block.utxo_commitment, PALW_T12_GENESIS.utxo_commitment);
        assert_ne!(block.hash_merkle_root, PALW_T12_GENESIS.hash_merkle_root, "the drill marker moves the merkle root too");
        assert_ne!(block.hash, palw_t12_drill_genesis_block_v1(&b).hash, "two salts, two genesis blocks");
        assert_eq!(block.hash, palw_t12_drill_genesis_block_v1(&a).hash, "derived, so every node derives the same one");
        assert_eq!((block.timestamp, block.bits, block.daa_score), (PALW_T12_GENESIS.timestamp, PALW_T12_GENESIS.bits, 0));
        assert!(PALW_T12_DRILL_GENESIS_COINBASE_PAYLOAD.ends_with(b"misaka-palw-t12-drill"));
        assert_eq!(
            PALW_T12_DRILL_GENESIS_COINBASE_PAYLOAD[..19],
            PALW_T12_GENESIS.coinbase_payload[..19],
            "public testnet-12's coinbase layout, marker aside"
        );
    }

    /// **A drill runs the shipping rules on its own genesis** (ADR-0152 §8.3 item 3: "the shipping
    /// binary with the drill salt"). The drill params validate; their genesis, fingerprint and
    /// identity id are not public testnet-12's, and neither is their network domain; they carry no
    /// DNS seeder. Put public testnet-12's genesis, seeders and genesis registry back and the
    /// fingerprint is public testnet-12's own — so the drill differs in exactly those three and in
    /// nothing a rule reads.
    #[test]
    fn t53_the_drill_runs_the_shipping_rules_on_its_own_genesis() {
        use crate::config::params::{Params, palw_t12_drill_params_v1};
        use crate::palw_mode_v2::PalwConsensusMode;
        let drill = palw_t12_drill_params_v1(&salt(0x53));
        let public = Params::from(palw_drill_network_v1());
        drill.validate_palw_v2().expect("the drill params are a runnable ruleset");
        assert_eq!(drill.net, public.net, "one network name");
        assert_eq!(drill.genesis.hash, palw_t12_drill_genesis_block_v1(&salt(0x53)).hash);
        assert_ne!(drill.genesis.hash, public.genesis.hash);
        assert_ne!(drill.consensus_params_id(), public.consensus_params_id(), "the handshake's fingerprint moves");
        assert_ne!(drill.consensus_identity_id(), public.consensus_identity_id(), "and the identity id, so no M1-6 escape");
        let domain =
            |p: &Params| crate::palw_attempt_v2::palw_network_domain_v2_for(p.net.to_string().as_bytes(), Some(p.genesis.hash));
        assert_ne!(domain(&drill), domain(&public), "every V2 signature is separated");
        assert!(drill.dns_seeders.is_empty() && !public.dns_seeders.is_empty(), "no public seeder");
        assert_ne!(
            palw_t12_drill_params_v1(&salt(0x35)).consensus_params_id(),
            drill.consensus_params_id(),
            "two drills, two fingerprints"
        );

        let mut normalized = drill.clone();
        normalized.genesis = public.genesis.clone();
        normalized.dns_seeders = public.dns_seeders;
        let (PalwConsensusMode::ConsensusV2(n), PalwConsensusMode::ConsensusV2(p)) =
            (&mut normalized.palw_consensus_mode, &public.palw_consensus_mode)
        else {
            panic!("testnet-12 is ConsensusV2");
        };
        n.genesis_objects = p.genesis_objects.clone();
        assert_eq!(
            normalized.consensus_params_id(),
            public.consensus_params_id(),
            "the genesis and the registry are the whole difference"
        );
    }
}
