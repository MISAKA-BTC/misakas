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
//!    heartbeat miners' addresses, the validator keys, the EVM accounts and the extra bonds a drill
//!    registers (D-9, D-10). No card key and no address public testnet-12 pays is one of them, and
//!    kaspad refuses a salted node configured with any other (`kaspad/src/palw_drill.rs`).
//!
//! **What the salt does NOT separate: the EVM lane.** An EVM transaction binds `EVM_CHAIN_ID` (one
//! constant on every network) and its sender's nonce, never the genesis, so 1 and 2 do not reach it
//! and only 3 does — see [`PalwDrillKeyRoleV1::Evm`].
//!
//! **Off-node signers take the salt too** (P2-12 review finding 1). The `misaka` CLI and the gateway
//! rail build their params with [`palw_chain_params_v1`] — the constructor kaspad uses — and check
//! them against the node's reported genesis with [`palw_node_genesis_verdict_v1`] before they sign:
//! a tool that built `Params::from(testnet-12)` against a drill node signed under PUBLIC testnet-12's
//! domain, which is 2 inverted.
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
    /// A payout address that is not a bond's own (`--palw-producer-pay-address`, a
    /// `getBlockTemplate` pay address).
    Payout,
    /// A heartbeat miner's fee address (`--palw-heartbeat-miner-address`).
    Heartbeat,
    /// A DNS-finality validator's signing key (`--validator-key`, ADR-0018) — "bonds" in ADR-0152
    /// §8.2's list, and a key that signs attestations, so a drill's is drill-only like every other.
    Validator,
    /// **An EVM account** (the EVM lane, ADR-0020): a secp256k1 secret, not an ML-DSA-87 seed —
    /// [`palw_drill_evm_secret_v1`] derives it, and kaspad (which links the lane's curve; this crate
    /// is secp-free) turns it into the account address. The EVM lane is the one place the salt does
    /// NOT separate a drill from public testnet-12: an EVM transaction is bound to `EVM_CHAIN_ID`,
    /// one constant on every network, and to the sender's nonce — neither to the genesis. A transfer,
    /// a bridge withdrawal or a model-market action signed on a drill by an account that also holds
    /// value on public testnet-12 is valid there at the same nonce, and anybody who reaches the
    /// drill's P2P port can lift it out of a drill block. So every EVM account a drill signs with is
    /// one of these (the market step's generator included), a salted node pays its EVM fees only
    /// to one (`--evm-fee-recipient`), and its `eth_sendRawTransaction` admits only their
    /// transactions. A per-genesis EVM chain id would close it structurally; that is a consensus
    /// change and not the drill's to make.
    Evm,
}

impl PalwDrillKeyRoleV1 {
    /// Every ML-DSA-87 role — what a producer key, a validator key and every UTXO address a drill
    /// uses can be. [`Self::Evm`] is not one of them: its keys are secp256k1.
    pub const MLDSA87: [Self; 6] = [Self::Main, Self::Bond, Self::Operator, Self::Payout, Self::Heartbeat, Self::Validator];

    pub fn name(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Bond => "bond",
            Self::Operator => "operator",
            Self::Payout => "payout",
            Self::Heartbeat => "heartbeat",
            Self::Validator => "validator",
            Self::Evm => "evm",
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

/// The order `n` of secp256k1's group, big-endian — a secret must lie in `1..n`.
const SECP256K1_ORDER_BE: [u8; 32] = [
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE, 0xBA, 0xAE, 0xDC, 0xE6, 0xAF,
    0x48, 0xA0, 0x3B, 0xBF, 0xD2, 0x5E, 0x8C, 0xD0, 0x36, 0x41, 0x41,
];

/// **The secp256k1 secret of drill EVM account `n`** ([`PalwDrillKeyRoleV1::Evm`]):
/// [`palw_drill_key_seed_v1`]`(salt, Evm, n)`, re-hashed with a counter in the (≈ 2⁻¹²⁸) case it is
/// not a scalar in `1..n` — so every index has exactly one account, derived without a curve library.
/// The address is kaspad's to compute (`kaspa_evm::evm_address_of_secret_v1`).
pub fn palw_drill_evm_secret_v1(salt: &PalwDrillSaltV1, n: u32) -> [u8; 32] {
    let mut secret = palw_drill_key_seed_v1(salt, PalwDrillKeyRoleV1::Evm, n);
    let mut counter: u32 = 0;
    while secret == [0u8; 32] || secret >= SECP256K1_ORDER_BE {
        counter += 1;
        let mut state = blake2b_simd::Params::new().hash_length(32).key(PALW_DRILL_KEY_SEED_DOMAIN_V1).to_state();
        state.update(&secret);
        state.update(&counter.to_le_bytes());
        secret.copy_from_slice(state.finalize().as_bytes());
    }
    secret
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
    /// An ML-DSA-87 role's key ([`PalwDrillKeyRoleV1::MLDSA87`]). An EVM account is secp256k1 and
    /// has no such key: [`palw_drill_evm_secret_v1`].
    pub fn derive(salt: &PalwDrillSaltV1, role: PalwDrillKeyRoleV1, n: u32) -> Self {
        assert!(role != PalwDrillKeyRoleV1::Evm, "a drill EVM account is secp256k1: palw_drill_evm_secret_v1 derives it");
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

    /// The index of the drill VALIDATOR key with this verification key (`--validator-key`), or
    /// `None` when the key is not one of this drill's.
    pub fn find_validator_pubkey(&self, pubkey: &[u8]) -> Option<u32> {
        (0..PALW_DRILL_KEYRING_SPAN_V1).find(|n| self.key(PalwDrillKeyRoleV1::Validator, *n).pubkey == pubkey)
    }

    /// The drill key (of any ML-DSA-87 role) whose address carries this owner payload, or `None`
    /// when the payload is not one of this drill's — what a pay or heartbeat address must be.
    pub fn find_payload(&self, payload: &[u8]) -> Option<(PalwDrillKeyRoleV1, u32)> {
        PalwDrillKeyRoleV1::MLDSA87
            .into_iter()
            .flat_map(|role| (0..PALW_DRILL_KEYRING_SPAN_V1).map(move |n| (role, n)))
            .find(|(role, n)| self.key(*role, *n).payload[..] == *payload)
    }

    /// **Every owner payload this drill's keyring derives** (all ML-DSA-87 roles, the whole span) —
    /// the set a salted node computes ONCE and then asks per `getBlockTemplate` (at most
    /// `6 × 32` key generations, a few tens of milliseconds, never on the request path).
    pub fn payloads(&self) -> std::collections::HashSet<[u8; 64]> {
        PalwDrillKeyRoleV1::MLDSA87
            .into_iter()
            .flat_map(|role| (0..PALW_DRILL_KEYRING_SPAN_V1).map(move |n| (role, n)))
            .map(|(role, n)| self.key(role, n).payload)
            .collect()
    }

    /// The secrets of this drill's EVM accounts `0..PALW_DRILL_KEYRING_SPAN_V1`, in index order.
    pub fn evm_secrets(&self) -> Vec<[u8; 32]> {
        (0..PALW_DRILL_KEYRING_SPAN_V1).map(|n| palw_drill_evm_secret_v1(&self.salt, n)).collect()
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

/// **The params of the chain a salt names — the one constructor every signer shares** (kaspad's
/// `apply_to_config`, the `misaka` CLI, the gateway rail): `Params::from(network)` without a salt,
/// the drill's params with one, and a refusal for a salt on any network but testnet-12. An
/// off-node signer that built `Params::from(network)` against a drill node signed every object
/// under PUBLIC testnet-12's network domain — refused by the drill, and valid on public
/// testnet-12, the opposite of ADR-0152 §8.2 — which is why this takes the salt explicitly and
/// [`palw_node_genesis_verdict_v1`] checks the answer against the node.
pub fn palw_chain_params_v1(network: NetworkId, salt: Option<&PalwDrillSaltV1>) -> Result<crate::config::params::Params, String> {
    match salt {
        None => Ok(crate::config::params::Params::from(network)),
        Some(salt) if network == palw_drill_network_v1() => Ok(crate::config::params::palw_t12_drill_params_v1(salt)),
        Some(salt) => Err(format!(
            "a drill genesis salt (drill {}) applies to testnet-12 only, and this is {network}: a salt drills the network whose shipping \
             rules it keeps",
            salt.id()
        )),
    }
}

/// **Whether an off-node signer must ask the node its genesis before signing**: on testnet-12 — the
/// one network a drill shares a name with — and whenever a salt is given. Elsewhere there is no
/// second genesis under the name, and a node that predates `getPalwNodeStatus` would drop the
/// connection at the question (an unknown wRPC op closes the socket), so it is not asked.
pub fn palw_node_genesis_check_applies_v1(network: NetworkId, salt: Option<&PalwDrillSaltV1>) -> bool {
    salt.is_some() || network == palw_drill_network_v1()
}

/// **Is the node's chain the one these params describe?** — asked by every off-node signer before
/// it signs (ADR-0152 §8.2): a PALW signature is made under `palw_network_domain_v2_for(network,
/// genesis)`, so a signer whose genesis is not the node's signs objects that node refuses — and,
/// against a drill, objects public testnet-12 accepts. `node_genesis_hash` / `node_drill_salt_id`
/// are what the node reports (`getPalwNodeStatus`, version 4); empty is a node older than those
/// fields, which predates the salt and so is never a drill.
///
/// Every refusal names the fix: the drill's salt, the other drill's, none, or a build of the
/// network's own genesis.
pub fn palw_node_genesis_verdict_v1(
    local: &crate::config::params::Params,
    local_salt: Option<&PalwDrillSaltV1>,
    node_genesis_hash: &str,
    node_drill_salt_id: &str,
) -> Result<(), String> {
    let net = local.net;
    let ours = local.genesis.hash;
    if node_genesis_hash.is_empty() {
        return match local_salt {
            None => Ok(()),
            Some(salt) => Err(format!(
                "the node does not report its genesis, so it predates the drill salt and is not a drill node — yet this tool was given \
                 drill {}'s salt. Point it at that drill's node, or drop --palw-drill-genesis-salt",
                salt.id()
            )),
        };
    }
    let theirs: Hash64 = node_genesis_hash
        .parse()
        .map_err(|_| format!("the node reports genesis {node_genesis_hash:?}, which is not a 128-hex hash"))?;
    if theirs == ours {
        return Ok(());
    }
    Err(match (node_drill_salt_id.is_empty(), local_salt) {
        (false, None) => format!(
            "the node runs {net} DRILL {node_drill_salt_id} (genesis {theirs}), and this tool derived public {net}'s genesis {ours}: \
             everything it signed would be refused by the drill and valid on public {net}. Pass --palw-drill-genesis-salt=<drill \
             {node_drill_salt_id}'s salt>"
        ),
        (false, Some(salt)) => format!(
            "the node runs {net} drill {node_drill_salt_id} (genesis {theirs}), and this tool was given drill {}'s salt (genesis {ours}): \
             pass the salt of the drill this node runs",
            salt.id()
        ),
        (true, Some(salt)) => format!(
            "the node runs public {net} (genesis {theirs}), and this tool was given drill {}'s salt (genesis {ours}): drop \
             --palw-drill-genesis-salt, or point it at that drill's node",
            salt.id()
        ),
        (true, None) => format!(
            "the node runs {net} on genesis {theirs}, and this build's {net} is genesis {ours}: another incarnation of the network, \
             whose signatures neither side accepts — use a build of the node's network"
        ),
    })
}

/// **One post-launch fence a drill's `--palw-drill-fence-at` set** — see
/// [`palw_drill_post_launch_fences_at_v1`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwDrillFenceMoveV1 {
    /// The fence, as [`crate::config::params::Params::palw_fences_v1`] names it.
    pub name: &'static str,
    /// Its height on the shipping ruleset the drill started from — `None` while the release leaves
    /// it dormant (the flag ARMS it), `Some` once the release arms it (the flag MOVES it).
    pub was: Option<u64>,
    /// Its height on the drill chain.
    pub at: u64,
}

impl std::fmt::Display for PalwDrillFenceMoveV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.was {
            None => write!(f, "{} ARMED at DAA {} (dormant in the shipping release)", self.name, self.at),
            Some(was) if was == self.at => write!(f, "{} at DAA {} (the shipping release's own height)", self.name, self.at),
            Some(was) => write!(f, "{} MOVED from DAA {was} (the shipping release) to DAA {}", self.name, self.at),
        }
    }
}

/// **A drill crosses the post-launch release's flag day at a low height** — the one narrow
/// exception to "a drill runs the shipping rules unedited" (`--override-params-file` stays refused on
/// a drill, `kaspad/src/palw_drill.rs`). Sets every fence of
/// [`crate::config::params::PALW_T12_POST_LAUNCH_FENCES_V1`] to `at` on `params`: ARMS the ones the
/// release still leaves dormant and MOVES the ones it arms, each through its entry's own `set`, so
/// every mirror follows. Nothing else is touched — no other fence, no window, no value — and the
/// function checks that on the ruleset it returns rather than trusting the entries.
///
/// Refused (`Err`, nothing to apply) on:
///
/// * any network but testnet-12, and on public testnet-12's own genesis: the flag moves fences on a
///   salted drill chain only, whose genesis already makes it a different network;
/// * `at` 0 or `never()`: a drill CROSSES the fence, so a block must be validated below it and one
///   above it (a fence at genesis is the identity's business, not the schedule's);
/// * `at` equal to another fence's height on the ruleset: the fork id names heights and not fences,
///   so a drill node started without the flag would peer across the height and fork silently (the
///   memory rule "a fence at a scheduled height is invisible to the fork id"); on its own height the
///   fork id refuses it at the handshake;
/// * a move that moves nothing (every fence already at `at`): the flag would leave the drill chain's
///   params id the release's;
/// * a result `validate_palw_v2` refuses.
///
/// The drill chain's `consensus_params_id` and schedule id move with the heights (every post-launch
/// fence is hashed Some-only), on top of the genesis the salt already moved.
pub fn palw_drill_post_launch_fences_at_v1(
    params: &mut crate::config::params::Params,
    at: u64,
) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_V1)
}

/// **A drill crosses testnet-12's SECOND post-launch flag day at a low height** (lane F2, 2026-09-27)
/// — [`palw_drill_post_launch_fences_at_v1`] for
/// [`crate::config::params::PALW_T12_POST_LAUNCH_FENCES_V2`] (`--palw-drill-fence2-at`): ARMS the
/// list's fences (dormant while the list's height is `None`) or MOVES them, each through its entry's
/// own `set`, and nothing else. Every refusal of the DAA-750 move applies, named for this flag; a
/// height another fence uses — a DAA-750 fence's included, wherever `--palw-drill-fence-at` put it —
/// is refused, so a drill already past its first crossing crosses this one at a height of its own.
pub fn palw_drill_post_launch_fences_v2_at_v1(
    params: &mut crate::config::params::Params,
    at: u64,
) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_V2)
}

/// **A drill crosses testnet-12's THIRD post-launch flag day at a low height** (the capacity architecture at
/// ρ = 10) — [`palw_drill_post_launch_fences_at_v1`] for
/// [`crate::config::params::PALW_T12_POST_LAUNCH_FENCES_V3`] (`--palw-drill-fence3-at`), with every refusal of
/// the first two, named for this flag.
pub fn palw_drill_post_launch_fences_v3_at_v1(
    params: &mut crate::config::params::Params,
    at: u64,
) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_V3)
}

/// Move testnet-12's model-specific finite court-window fence on a salted drill chain.
pub fn palw_drill_model_court_window_at_v1(
    params: &mut crate::config::params::Params,
    at: u64,
) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_MODEL_COURT_V1)
}

/// **A drill crosses the IR flag day at a low height** (RFC-0002 Phase F, drills D-F1…D-F4;
/// `--palw-drill-tir-at`) — [`palw_drill_post_launch_fences_at_v1`] for
/// [`crate::config::params::PALW_T12_TIR_FLAG_DAY_FENCES_V1`]: MOVES `palw_tir_v1` from the release's
/// DAA 2,000 to `at` (ARMS it on a ruleset that leaves it dormant) — testnet-12's IR ceilings and this
/// build's primitive set, through the entry's own `set`, so the bundle's mirror follows — and moves
/// nothing else. Every refusal of the post-launch moves
/// applies, named for this flag, and `validate_palw_v2` refuses the result unless the IR fence's
/// prerequisites are in force at or below `at`: `palw_audit_2026_09_11` declared, `palw_kary_court`
/// and `palw_rcore_plus` armed at or below it (combine with `--palw-drill-fence-at` below `at` when the
/// release arms them later).
pub fn palw_drill_tir_fence_at_v1(params: &mut crate::config::params::Params, at: u64) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_TIR_V1)
}

/// **A drill arms the generative fence at a low height** (RFC-0003; `--palw-drill-gen-at`) —
/// [`palw_drill_post_launch_fences_at_v1`]'s machinery for the one-entry drill list
/// [`crate::palw_gen_v1::PALW_DRILL_GEN_FENCES_V1`]: ARMS `palw_gen_v1` at `at` with this build's ids
/// and the drill's ceilings, and moves nothing else. The fence is in no network's release, so there
/// is no release height to move from. Every refusal of the post-launch moves applies, named for this
/// flag, and `validate_palw_v2` refuses the result unless `palw_tir_v1` is in force at or below `at`
/// (move it with `--palw-drill-tir-at` when the release arms it later).
pub fn palw_drill_gen_fence_at_v1(params: &mut crate::config::params::Params, at: u64) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_GEN_V1)
}

/// **A drill arms FP Job V5 at a low height** (RFC-0003 §II.2.1; `--palw-drill-fp-v5-at`) — the same
/// machinery for the one-entry drill list [`crate::palw_fp_job_v5::PALW_DRILL_FP_V5_FENCES_V1`]: ARMS
/// `palw_fp_job_v5` at `at` and moves nothing else. `validate_palw_v2` refuses the result unless
/// `palw_gen_v1` and `palw_fp_decode_rules` are in force at or below `at` (arm them first with
/// `--palw-drill-gen-at` and `--palw-drill-decode-rules-at`).
pub fn palw_drill_fp_v5_at_v1(params: &mut crate::config::params::Params, at: u64) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_FP_V5_V1)
}

/// **A drill arms the held leaf challenge at a low height** (RFC-0003 decision 22;
/// `--palw-drill-held-chunks-at`) — the same machinery for the one-entry drill list
/// [`crate::palw_held_close_v1::PALW_DRILL_HELD_CLOSE_CHUNKS_FENCES_V1`]: ARMS `palw_held_close_chunks_v1` at
/// `at` and moves nothing else. `validate_palw_v2` refuses the result unless `palw_tir_v1` and
/// `palw_held_context` are in force at or below `at` (combine with `--palw-drill-tir-at`).
pub fn palw_drill_held_close_chunks_at_v1(
    params: &mut crate::config::params::Params,
    at: u64,
) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_HELD_CLOSE_CHUNKS_V1)
}

/// **A drill crosses testnet-12's DAA-3,600 flag day at a low height** (`--palw-drill-tir2-at`) —
/// [`palw_drill_post_launch_fences_at_v1`] for
/// [`crate::config::params::PALW_T12_TIR_FENCE2_FENCES_V1`]: MOVES `palw_tir_fence2` from the release's
/// DAA 3,600 to `at` (ARMS it on a ruleset that leaves it dormant), through the entry's own `set`, and
/// moves nothing else — `palw_tir_fence2` is the whole DAA-3,600 list. Every refusal of the post-launch
/// moves applies, named for this flag, and `validate_palw_v2` refuses the result unless `palw_tir_v1` is
/// in force at or below `at` (combine with `--palw-drill-tir-at`).
pub fn palw_drill_tir_fence2_at_v1(params: &mut crate::config::params::Params, at: u64) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_TIR_FENCE2_V1)
}

/// **A drill arms the decode rules at a low height** (ADR-0082 D10/D11; `--palw-drill-decode-rules-at`) —
/// [`palw_drill_post_launch_fences_at_v1`]'s machinery for
/// [`crate::config::params::PALW_T12_DECODE_RULES_FENCES_V1`]: ARMS `palw_fp_decode_rules` at `at` through
/// the entry's own `set` (which writes the bundle's mirror) and moves nothing else — the release leaves it
/// dormant, so there is no release height to move from. Every refusal of the post-launch moves applies,
/// named for this flag. It is the prerequisite RFC-0003's FP Job V5 and RFC-0004's improvement fence name:
/// arm it at or below `--palw-drill-fp-v5-at` and `--palw-drill-improve-at`.
pub fn palw_drill_decode_rules_at_v1(
    params: &mut crate::config::params::Params,
    at: u64,
) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_DECODE_RULES_V1)
}

/// **A drill arms the improvement fence at a low height** (RFC-0004; `--palw-drill-improve-at`) —
/// the same machinery for the one-entry drill list
/// [`crate::palw_improve_v1::PALW_DRILL_IMPROVE_FENCES_V1`]: ARMS `palw_improvement_v1` at `at` with
/// this build's ids and the drill's ceilings, and moves nothing else. The fence is in no network's
/// release. Every refusal of the post-launch moves applies, named for this flag, and
/// `validate_palw_v2` refuses the result unless `palw_tir_v1`, `palw_gen_v1`, `palw_tir_fence2`,
/// `palw_kary_court` and `palw_fp_decode_rules` are in force at or below `at` (arm them first with
/// `--palw-drill-tir-at`, `--palw-drill-gen-at`, `--palw-drill-tir2-at`, `--palw-drill-fence-at` and
/// `--palw-drill-decode-rules-at`).
pub fn palw_drill_improve_fence_at_v1(
    params: &mut crate::config::params::Params,
    at: u64,
) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_IMPROVE_V1)
}

/// One post-launch flag day a drill may cross: its list and the command-line flag that moves it.
struct PalwDrillFlagDayV1 {
    list: &'static [crate::config::params::PalwPostLaunchFenceV1],
    flag: &'static str,
}

/// The DAA-750 release's flag day (`--palw-drill-fence-at`).
const PALW_DRILL_FLAG_DAY_V1: PalwDrillFlagDayV1 =
    PalwDrillFlagDayV1 { list: crate::config::params::PALW_T12_POST_LAUNCH_FENCES_V1, flag: "--palw-drill-fence-at" };

/// The second flag day (`--palw-drill-fence2-at`, lane F2).
const PALW_DRILL_FLAG_DAY_V2: PalwDrillFlagDayV1 =
    PalwDrillFlagDayV1 { list: crate::config::params::PALW_T12_POST_LAUNCH_FENCES_V2, flag: "--palw-drill-fence2-at" };

/// The third flag day (`--palw-drill-fence3-at`, the capacity package).
const PALW_DRILL_FLAG_DAY_V3: PalwDrillFlagDayV1 =
    PalwDrillFlagDayV1 { list: crate::config::params::PALW_T12_POST_LAUNCH_FENCES_V3, flag: "--palw-drill-fence3-at" };

/// The IR fence alone (`--palw-drill-tir-at`, RFC-0002 Phase F).
const PALW_DRILL_FLAG_DAY_TIR_V1: PalwDrillFlagDayV1 =
    PalwDrillFlagDayV1 { list: crate::config::params::PALW_T12_TIR_FLAG_DAY_FENCES_V1, flag: "--palw-drill-tir-at" };

/// The generative fence alone (`--palw-drill-gen-at`, RFC-0003): a drill-only list.
const PALW_DRILL_FLAG_DAY_GEN_V1: PalwDrillFlagDayV1 =
    PalwDrillFlagDayV1 { list: crate::palw_gen_v1::PALW_DRILL_GEN_FENCES_V1, flag: "--palw-drill-gen-at" };

/// FP Job V5 alone (`--palw-drill-fp-v5-at`): a drill-only list.
const PALW_DRILL_FLAG_DAY_FP_V5_V1: PalwDrillFlagDayV1 =
    PalwDrillFlagDayV1 { list: crate::palw_fp_job_v5::PALW_DRILL_FP_V5_FENCES_V1, flag: "--palw-drill-fp-v5-at" };

/// The held leaf challenge alone (`--palw-drill-held-chunks-at`): a drill-only list.
const PALW_DRILL_FLAG_DAY_HELD_CLOSE_CHUNKS_V1: PalwDrillFlagDayV1 = PalwDrillFlagDayV1 {
    list: crate::palw_held_close_v1::PALW_DRILL_HELD_CLOSE_CHUNKS_FENCES_V1,
    flag: "--palw-drill-held-chunks-at",
};

/// The second IR fence alone (`--palw-drill-tir2-at`).
const PALW_DRILL_FLAG_DAY_TIR_FENCE2_V1: PalwDrillFlagDayV1 =
    PalwDrillFlagDayV1 { list: crate::config::params::PALW_T12_TIR_FENCE2_FENCES_V1, flag: "--palw-drill-tir2-at" };

/// The decode rules alone (`--palw-drill-decode-rules-at`, ADR-0082 D10/D11): the dormant testnet-12 list.
const PALW_DRILL_FLAG_DAY_DECODE_RULES_V1: PalwDrillFlagDayV1 =
    PalwDrillFlagDayV1 { list: crate::config::params::PALW_T12_DECODE_RULES_FENCES_V1, flag: "--palw-drill-decode-rules-at" };

/// The improvement fence alone (`--palw-drill-improve-at`, RFC-0004): a drill-only list.
const PALW_DRILL_FLAG_DAY_IMPROVE_V1: PalwDrillFlagDayV1 =
    PalwDrillFlagDayV1 { list: crate::palw_improve_v1::PALW_DRILL_IMPROVE_FENCES_V1, flag: "--palw-drill-improve-at" };
/// The model-specific finite court window alone (`--palw-drill-model-court-at`).
const PALW_DRILL_FLAG_DAY_MODEL_COURT_V1: PalwDrillFlagDayV1 =
    PalwDrillFlagDayV1 { list: crate::config::params::PALW_T12_MODEL_COURT_WINDOW_FENCES_V1, flag: "--palw-drill-model-court-at" };

/// The capacity ramp's ready steps (ADR-0160 stage 3), one entry each: drill-only lists, in no flag day's list and at
/// no height on any ruleset — int-11 chooses its heights.
const PALW_DRILL_CAPACITY_STEP2_FENCES_V1: &[crate::config::params::PalwPostLaunchFenceV1] =
    &[crate::config::params::PALW_T12_CAPACITY_RHO25_STEP_2_V1];
const PALW_DRILL_CAPACITY_STEP3_FENCES_V1: &[crate::config::params::PalwPostLaunchFenceV1] =
    &[crate::config::params::PALW_T12_CAPACITY_RHO100_STEP_3_V1];
const PALW_DRILL_CAPACITY_RHO100_FENCES_V1: &[crate::config::params::PalwPostLaunchFenceV1] =
    &[crate::config::params::PALW_T12_CAPACITY_RHO100_STEP_2_V1];
/// F-N alone (`--palw-drill-capacity-network-room-at`): the network level and the work-conserving fair share.
const PALW_DRILL_CAPACITY_NETWORK_ROOM_FENCES_V1: &[crate::config::params::PalwPostLaunchFenceV1] =
    &[crate::config::params::PALW_T12_CAPACITY_NETWORK_ROOM_V1];
const PALW_DRILL_FLAG_DAY_CAPACITY_NETWORK_ROOM_V1: PalwDrillFlagDayV1 =
    PalwDrillFlagDayV1 { list: PALW_DRILL_CAPACITY_NETWORK_ROOM_FENCES_V1, flag: "--palw-drill-capacity-network-room-at" };
/// F-N's static verification term alone (`--palw-drill-capacity-network-verify-at`, int-11).
const PALW_DRILL_FLAG_DAY_CAPACITY_NETWORK_VERIFY_V1: PalwDrillFlagDayV1 = PalwDrillFlagDayV1 {
    list: crate::config::params::PALW_T12_CAPACITY_NETWORK_VERIFY_FENCES_V1,
    flag: "--palw-drill-capacity-network-verify-at",
};
/// ρ = 25 as F-L's second step (`--palw-drill-capacity-step2-at`).
const PALW_DRILL_FLAG_DAY_CAPACITY_STEP2_V1: PalwDrillFlagDayV1 =
    PalwDrillFlagDayV1 { list: PALW_DRILL_CAPACITY_STEP2_FENCES_V1, flag: "--palw-drill-capacity-step2-at" };
/// ρ = 100 as F-L's third step, after ρ = 25 (`--palw-drill-capacity-step3-at`).
const PALW_DRILL_FLAG_DAY_CAPACITY_STEP3_V1: PalwDrillFlagDayV1 =
    PalwDrillFlagDayV1 { list: PALW_DRILL_CAPACITY_STEP3_FENCES_V1, flag: "--palw-drill-capacity-step3-at" };
/// ρ = 100 as F-L's second step, straight after ρ = 10 (`--palw-drill-capacity-rho100-at`).
const PALW_DRILL_FLAG_DAY_CAPACITY_RHO100_V1: PalwDrillFlagDayV1 =
    PalwDrillFlagDayV1 { list: PALW_DRILL_CAPACITY_RHO100_FENCES_V1, flag: "--palw-drill-capacity-rho100-at" };

/// **What a ready capacity step needs of the ruleset it is armed on**, asked before the move so a drill that
/// misses it is refused by name and never reaches the entry's own `set`, which PANICS on a step whose
/// predecessors are not carried: the ρ = 10 flag day's F-L armed (move it with `--palw-drill-fence3-at`: the
/// release arms it at DAA 1,700) with the earlier steps in place, and a height strictly above the step it builds
/// on (the ramp's heights strictly increase — one height, one step).
fn palw_drill_capacity_step_prerequisite_v1(
    params: &crate::config::params::Params,
    slot: usize,
    at: u64,
    flag: &str,
) -> Result<(), String> {
    use crate::config::params::ForkActivation;
    let carried: &[crate::palw_aggregate_liability_v1::PalwCapacityStepV1] = params
        .palw_capacity_aggregate_liability
        .as_ref()
        .filter(|value| value.activation != ForkActivation::never())
        .map_or(&[], |value| value.steps.as_slice());
    if carried.len() < slot - 1 {
        return Err(format!(
            "{flag}={at}: step {slot} of the capacity ramp builds on {} earlier step(s) and this ruleset's palw_capacity_aggregate_liability \
             carries {} — arm the ρ = 10 capacity flag day first (--palw-drill-fence3-at, below this height) and the steps before it",
            slot - 1,
            carried.len()
        ));
    }
    let previous = carried[slot - 2].from_daa;
    if at <= previous {
        return Err(format!(
            "{flag}={at}: step {slot} must start above step {} (DAA {previous}): the ramp's heights strictly increase — one \
             height, one step",
            slot - 1
        ));
    }
    Ok(())
}

/// **A drill moves F-N — the network level and the work-conserving fair share (ADR-0160 stage 4,
/// `palw_capacity_network_room`) — to its own height** (`--palw-drill-capacity-network-room-at`): the stage-4 gate that
/// must be in force before ρ = 25 / ρ = 100. The ρ = 10 flag day's list arms it with the other seven
/// (`--palw-drill-fence3-at`, which still takes the whole list: they arm together); this flag moves F-N alone, so a drill
/// can time it apart from them — never below F-R (the verify room) and F-S (the issuance slots), which
/// `validate_palw_v2` refuses (it needs both at or below it). Every refusal of the post-launch moves applies, named for
/// this flag.
pub fn palw_drill_capacity_network_room_at_v1(
    params: &mut crate::config::params::Params,
    at: u64,
) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_CAPACITY_NETWORK_ROOM_V1)
}

/// **A drill arms F-N's static verification term** (int-11, `palw_capacity_network_verify`; lane P's `L_ver` = 435 at the
/// shipped windows) at its own height (`--palw-drill-capacity-network-verify-at`): `L_net` also capped by what the seats can
/// verify inside the receipt window. It is dormant on every ruleset, so the move ARMS it; `validate_palw_v2` refuses a
/// height below F-N (`palw_capacity_network_room`), whose level it caps. Every refusal of the post-launch moves applies,
/// named for this flag.
pub fn palw_drill_capacity_network_verify_at_v1(
    params: &mut crate::config::params::Params,
    at: u64,
) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_CAPACITY_NETWORK_VERIFY_V1)
}

/// **A drill appends the ρ = 25 step** (ADR-0160 stage 3, `--palw-drill-capacity-step2-at`) — F-L's second step at
/// `at` on a ruleset whose F-L carries the ρ = 10 step, through [`crate::config::params::PALW_T12_CAPACITY_RHO25_STEP_2_V1`]'s own
/// `set` (which writes the fold's mirror) and nothing else. Every refusal of the post-launch moves applies, named
/// for this flag, and the prerequisite above.
pub fn palw_drill_capacity_step2_at_v1(
    params: &mut crate::config::params::Params,
    at: u64,
) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_chain_refusal_v1(params, at, PALW_DRILL_FLAG_DAY_CAPACITY_STEP2_V1.flag)?;
    palw_drill_capacity_step_prerequisite_v1(params, 2, at, PALW_DRILL_FLAG_DAY_CAPACITY_STEP2_V1.flag)?;
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_CAPACITY_STEP2_V1)
}

/// **A drill appends the ρ = 100 step after ρ = 25** (`--palw-drill-capacity-step3-at`): F-L's third step, on a
/// ruleset whose F-L carries two steps ([`palw_drill_capacity_step2_at_v1`] first).
pub fn palw_drill_capacity_step3_at_v1(
    params: &mut crate::config::params::Params,
    at: u64,
) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_chain_refusal_v1(params, at, PALW_DRILL_FLAG_DAY_CAPACITY_STEP3_V1.flag)?;
    palw_drill_capacity_step_prerequisite_v1(params, 3, at, PALW_DRILL_FLAG_DAY_CAPACITY_STEP3_V1.flag)?;
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_CAPACITY_STEP3_V1)
}

/// **A drill appends ρ = 100 straight after ρ = 10** (`--palw-drill-capacity-rho100-at`): F-L's second step with
/// ρ = 100 — the alternative to ρ = 25 then ρ = 100, never combined with it (both are the second step).
pub fn palw_drill_capacity_rho100_at_v1(
    params: &mut crate::config::params::Params,
    at: u64,
) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    palw_drill_chain_refusal_v1(params, at, PALW_DRILL_FLAG_DAY_CAPACITY_RHO100_V1.flag)?;
    palw_drill_capacity_step_prerequisite_v1(params, 2, at, PALW_DRILL_FLAG_DAY_CAPACITY_RHO100_V1.flag)?;
    palw_drill_move_fences_v1(params, at, &PALW_DRILL_FLAG_DAY_CAPACITY_RHO100_V1)
}

/// **The refusals every post-launch mover makes first**, in this order: a ruleset that is not testnet-12's, one
/// that runs PUBLIC testnet-12's genesis (the fences move on a salted drill chain only), a height that is not one a
/// drill can cross (0, never). One spelling for [`palw_drill_move_fences_v1`] and the capacity steps' prerequisites,
/// which are asked after these and before the move.
fn palw_drill_chain_refusal_v1(params: &crate::config::params::Params, at: u64, flag: &str) -> Result<(), String> {
    if params.net != palw_drill_network_v1() {
        return Err(format!(
            "the post-launch fences are testnet-12's release, and this ruleset is {}: {flag} runs on a salted \
             testnet-12 drill only",
            params.net
        ));
    }
    if params.genesis.hash == PALW_T12_GENESIS.hash {
        return Err("this ruleset runs PUBLIC testnet-12's genesis: the post-launch fences move on a salted drill chain only \
                    (--palw-drill-genesis-salt) — public testnet-12's are the release's, and every node must agree on them"
            .to_owned());
    }
    if at == 0 || at == u64::MAX {
        return Err(format!(
            "{flag}={at}: a drill CROSSES the release's flag day, so the fences need a height with blocks below and \
             above it — not genesis (0) and not never"
        ));
    }
    Ok(())
}

/// The one body both flag days share — see [`palw_drill_post_launch_fences_at_v1`].
fn palw_drill_move_fences_v1(
    params: &mut crate::config::params::Params,
    at: u64,
    day: &PalwDrillFlagDayV1,
) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    use crate::config::params::ForkActivation;
    let (list, flag) = (day.list, day.flag);
    palw_drill_chain_refusal_v1(params, at, flag)?;
    let listed = |name: &str| list.iter().any(|fence| fence.name == name);
    let before = params.palw_fences_v1();
    if let Some((other, _)) = before.iter().find(|(name, fence)| !listed(name) && fence.is_some_and(|fence| fence.daa_score() == at)) {
        return Err(format!(
            "{flag}={at}: DAA {at} is already {other}'s height on this ruleset. The fork id names heights, not \
             fences, so a drill node started without the flag would peer across it and fork silently — pick a height no other \
             fence uses"
        ));
    }
    // Set on a copy, installed only once every check below has passed: a refusal leaves `params`
    // exactly as it came.
    let mut moved = params.clone();
    let mut moves = Vec::with_capacity(list.len());
    for fence in list {
        let Some((_, was)) = before.iter().find(|(name, _)| *name == fence.name) else {
            return Err(format!("{} is not a fence of this ruleset (Params::palw_fences_v1 does not name it)", fence.name));
        };
        (fence.set)(&mut moved, Some(ForkActivation::new(at)));
        moves.push(PalwDrillFenceMoveV1 {
            name: fence.name,
            was: was.filter(|was| *was != ForkActivation::never()).map(|was| was.daa_score()),
            at,
        });
    }
    // The entries' own promise, checked on the ruleset itself: each listed fence at `at`, every
    // other fence exactly where it was.
    for ((name, was), (_, now)) in before.iter().zip(moved.palw_fences_v1().iter()) {
        if listed(name) {
            if *now != Some(ForkActivation::new(at)) {
                return Err(format!("{name}'s entry did not set it to DAA {at} (it reads {now:?})"));
            }
        } else if was != now {
            return Err(format!(
                "setting the post-launch fences moved {name} too ({was:?} -> {now:?}): the drill moves the release's fences and \
                 nothing else"
            ));
        }
    }
    if moves.iter().all(|m| m.was == Some(at)) {
        return Err(format!(
            "{flag}={at}: every post-launch fence is already at DAA {at} on the shipping release — the flag would \
             move nothing; drop it"
        ));
    }
    moved
        .validate_palw_v2()
        .map_err(|e| format!("the drill ruleset with the post-launch fences at DAA {at} does not validate: {e}"))?;
    *params = moved;
    Ok(moves)
}

/// **A drill crosses ADR-0160's capacity flag day** (rcore/cap-s1) — the sibling of
/// [`palw_drill_post_launch_fences_at_v1`] for [`crate::config::params::PALW_T12_CAPACITY_FENCES_V1`],
/// which no shipped ruleset arms (the memory rule "a flag day needs a drill that crosses it"). ARMS
/// every capacity fence at `at` on a salted testnet-12 drill ruleset, each through its entry's own
/// `set`, and moves nothing else — checked on the ruleset it returns. Refused (`Err`, `params`
/// untouched) on: any network but testnet-12 and public testnet-12's own genesis; `at` 0 or `never()`;
/// `at` equal to another (non-capacity) fence's height, which the fork id could not tell apart; a
/// result `validate_palw_v2` refuses — among them a height below the post-launch fences the capacity
/// fences require (strict-win and lane A for F-W: combine with `--palw-drill-fence-at` at or below
/// `at`). Not wired to a kaspad flag yet: stage 3's drill does that with the release that arms them.
pub fn palw_drill_capacity_fences_at_v1(
    params: &mut crate::config::params::Params,
    at: u64,
) -> Result<Vec<PalwDrillFenceMoveV1>, String> {
    use crate::config::params::{ForkActivation, PALW_T12_CAPACITY_FENCES_V1};
    if params.net != palw_drill_network_v1() {
        return Err(format!("the capacity fences are testnet-12's, and this ruleset is {}: a salted testnet-12 drill only", params.net));
    }
    if params.genesis.hash == PALW_T12_GENESIS.hash {
        return Err("this ruleset runs PUBLIC testnet-12's genesis: the capacity fences are crossed on a salted drill chain only".to_owned());
    }
    if at == 0 || at == u64::MAX {
        return Err(format!("capacity fences at {at}: a drill CROSSES the flag day, so not genesis (0) and not never"));
    }
    let listed = |name: &str| PALW_T12_CAPACITY_FENCES_V1.iter().any(|fence| fence.name == name);
    let before = params.palw_fences_v1();
    if let Some((other, _)) = before.iter().find(|(name, fence)| !listed(name) && fence.is_some_and(|fence| fence.daa_score() == at)) {
        return Err(format!(
            "capacity fences at {at}: DAA {at} is already {other}'s height on this ruleset — the fork id names heights, not fences"
        ));
    }
    let mut moved = params.clone();
    let mut moves = Vec::with_capacity(PALW_T12_CAPACITY_FENCES_V1.len());
    for fence in PALW_T12_CAPACITY_FENCES_V1 {
        let Some((_, was)) = before.iter().find(|(name, _)| *name == fence.name) else {
            return Err(format!("{} is not a fence of this ruleset (Params::palw_fences_v1 does not name it)", fence.name));
        };
        (fence.set)(&mut moved, Some(ForkActivation::new(at)));
        moves.push(PalwDrillFenceMoveV1 { name: fence.name, was: was.filter(|was| *was != ForkActivation::never()).map(|was| was.daa_score()), at });
    }
    for ((name, was), (_, now)) in before.iter().zip(moved.palw_fences_v1().iter()) {
        if listed(name) {
            // A valued fence may name later steps' slots after its own height (F-L's schedule): the
            // entry's own name is at `at`.
            if PALW_T12_CAPACITY_FENCES_V1.iter().any(|fence| fence.name == *name) && *now != Some(ForkActivation::new(at)) {
                return Err(format!("{name}'s entry did not set it to DAA {at} (it reads {now:?})"));
            }
        } else if was != now && !name.starts_with("palw_capacity_") {
            return Err(format!("arming the capacity fences moved {name} too ({was:?} -> {now:?}): the drill moves them and nothing else"));
        }
    }
    moved.validate_palw_v2().map_err(|e| format!("the drill ruleset with the capacity fences at DAA {at} does not validate: {e}"))?;
    *params = moved;
    Ok(moves)
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
        for role in PalwDrillKeyRoleV1::MLDSA87.into_iter().chain([PalwDrillKeyRoleV1::Evm]) {
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
        for role in PalwDrillKeyRoleV1::MLDSA87 {
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

    /// **The roles the review added** (P2-12 review, findings 2 and 6): a validator key is found
    /// as a validator key and not as a bond; an EVM account's secret is a secp256k1 scalar in
    /// `1..n`, the Evm role's own seed, per salt and index; the payload set is every ML-DSA-87
    /// role's whole span and nothing public testnet-12 pays.
    #[test]
    fn t53_validator_and_evm_drill_keys() {
        let (a, b) = (salt(0x71), salt(0x72));
        let ring = PalwDrillKeyringV1::new(a);
        let validator = ring.key(PalwDrillKeyRoleV1::Validator, 5);
        assert_eq!(ring.find_validator_pubkey(&validator.pubkey), Some(5));
        assert_eq!(ring.find_validator_pubkey(&ring.key(PalwDrillKeyRoleV1::Bond, 5).pubkey), None, "a bond key is not a validator's");
        assert_eq!(ring.find_bond_pubkey(&validator.pubkey), None, "nor the reverse");
        assert_eq!(ring.find_payload(&validator.payload), Some((PalwDrillKeyRoleV1::Validator, 5)));

        let secret = palw_drill_evm_secret_v1(&a, 3);
        assert_eq!(secret, palw_drill_key_seed_v1(&a, PalwDrillKeyRoleV1::Evm, 3), "the Evm role's own seed (in range)");
        assert!(secret != [0u8; 32] && secret < SECP256K1_ORDER_BE, "a secp256k1 scalar");
        assert_ne!(secret, palw_drill_evm_secret_v1(&b, 3), "per salt");
        assert_ne!(secret, palw_drill_evm_secret_v1(&a, 4), "per index");
        let secrets = ring.evm_secrets();
        assert_eq!(secrets.len(), PALW_DRILL_KEYRING_SPAN_V1 as usize);
        assert_eq!(secrets[3], secret);
        assert!(
            PalwDrillKeyRoleV1::MLDSA87.iter().all(|role| palw_drill_key_seed_v1(&a, *role, 3) != secret),
            "no ML-DSA seed doubles as an EVM secret"
        );
        assert!(std::panic::catch_unwind(|| PalwDrillKeyV1::derive(&a, PalwDrillKeyRoleV1::Evm, 0)).is_err(), "no ML-DSA EVM key");

        let payloads = ring.payloads();
        assert_eq!(payloads.len(), PalwDrillKeyRoleV1::MLDSA87.len() * PALW_DRILL_KEYRING_SPAN_V1 as usize, "all distinct");
        assert!(payloads.contains(&ring.key(PalwDrillKeyRoleV1::Payout, 31).payload));
        assert!(!payloads.contains(&PALW_T12_GENESIS_BONDS[0].payout_payload), "a card's payout is not a drill's");
    }

    /// **Off-node signers take their params from the salt and check them against the node**
    /// (review finding 1). Against a drill node, a tool without the salt is refused and told the
    /// drill's id; with the drill's salt it derives the drill's params, whose network domain is the
    /// drill's and not public testnet-12's; with another drill's salt, or a salt against a public
    /// node, it is refused; a node too old to report is accepted only without a salt.
    #[test]
    fn t53_an_off_node_signer_signs_under_the_nodes_genesis() {
        use crate::config::params::Params;
        let t12 = palw_drill_network_v1();
        let (a, b) = (salt(0x81), salt(0x82));
        let public = palw_chain_params_v1(t12, None).unwrap();
        let drill = palw_chain_params_v1(t12, Some(&a)).unwrap();
        assert_eq!(public.genesis.hash, Params::from(t12).genesis.hash);
        assert_eq!(drill.genesis.hash, palw_t12_drill_genesis_block_v1(&a).hash);
        let why = palw_chain_params_v1(NetworkId::new(NetworkType::Devnet), Some(&a)).unwrap_err();
        assert!(why.contains("testnet-12 only"), "{why}");
        let domain =
            |p: &Params| crate::palw_attempt_v2::palw_network_domain_v2_for(p.net.to_string().as_bytes(), Some(p.genesis.hash));
        assert_ne!(domain(&drill), domain(&public), "the salted params sign under the drill's domain");

        let drill_node = (drill.genesis.hash.to_string(), a.id());
        let public_node = (public.genesis.hash.to_string(), String::new());
        assert_eq!(palw_node_genesis_verdict_v1(&drill, Some(&a), &drill_node.0, &drill_node.1), Ok(()));
        assert_eq!(palw_node_genesis_verdict_v1(&public, None, &public_node.0, &public_node.1), Ok(()));
        let why = palw_node_genesis_verdict_v1(&public, None, &drill_node.0, &drill_node.1).unwrap_err();
        assert!(why.contains(&format!("DRILL {}", a.id())) && why.contains("--palw-drill-genesis-salt"), "{why}");
        let other = palw_chain_params_v1(t12, Some(&b)).unwrap();
        let why = palw_node_genesis_verdict_v1(&other, Some(&b), &drill_node.0, &drill_node.1).unwrap_err();
        assert!(why.contains(&a.id()) && why.contains(&b.id()), "{why}");
        let why = palw_node_genesis_verdict_v1(&drill, Some(&a), &public_node.0, &public_node.1).unwrap_err();
        assert!(why.contains("drop") && why.contains("public testnet-12"), "{why}");
        let why = palw_node_genesis_verdict_v1(&public, None, &Hash64::from_u64_word(7).to_string(), "").unwrap_err();
        assert!(why.contains("another incarnation"), "{why}");
        assert_eq!(palw_node_genesis_verdict_v1(&public, None, "", ""), Ok(()), "a node predating the field is not a drill");
        assert!(palw_node_genesis_verdict_v1(&drill, Some(&a), "", "").is_err(), "but a salt needs a node that says its genesis");
        assert!(palw_node_genesis_verdict_v1(&public, None, "zz", "").is_err());

        // Who is asked: testnet-12 always, any network with a salt, no other network without one.
        assert!(palw_node_genesis_check_applies_v1(t12, None));
        assert!(palw_node_genesis_check_applies_v1(NetworkId::new(NetworkType::Devnet), Some(&a)));
        assert!(!palw_node_genesis_check_applies_v1(NetworkId::new(NetworkType::Devnet), None));
        assert!(!palw_node_genesis_check_applies_v1(NetworkId::new(NetworkType::Mainnet), None));
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
        // Lane A's operator list (armed with the post-launch release) is the genesis REGISTRY's bonds,
        // derived by its entry over the registry it is set on: re-derived over the public registry, it
        // is public testnet-12's — part of the registry difference, not another one.
        let lane_a = crate::config::params::PALW_T12_POST_LAUNCH_FENCES_V1
            .iter()
            .find(|f| f.name == "palw_operator_anchor")
            .expect("lane A is listed");
        let at = normalized.palw_operator_anchor.as_ref().map(|rule| rule.activation);
        assert!(at.is_some(), "the drill runs the release's lane A");
        (lane_a.set)(&mut normalized, at);
        assert_eq!(
            normalized.consensus_params_id(),
            public.consensus_params_id(),
            "the genesis and the registry are the whole difference"
        );
    }

    /// **The release's one list names fences, and each entry sets its own fence and nothing else.**
    /// Every entry is a name `Params::palw_fences_v1` knows, once; it is dormant on the four base
    /// presets; and, on the drill ruleset, its `set` alone moves exactly its own fence to the height
    /// and — set back to the height it had — leaves the ruleset byte-identical (`Debug`) to where it
    /// started, so a `set` that touched anything else (another fence, a window, a mirror it does not
    /// restore) fails here, for every entry a later lane adds.
    #[test]
    fn the_post_launch_fence_list_names_fences_and_each_entry_sets_its_own_alone() {
        use crate::config::params::{
            DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, PALW_T12_POST_LAUNCH_FENCES_V1, SIMNET_PARAMS, TESTNET_PARAMS,
            palw_t12_drill_params_v1,
        };
        assert!(!PALW_T12_POST_LAUNCH_FENCES_V1.is_empty());
        let drill = palw_t12_drill_params_v1(&salt(0x53));
        let fences = drill.palw_fences_v1();
        let height = |p: &crate::config::params::Params, name: &str| {
            p.palw_fences_v1().into_iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("{name} is a fence")).1
        };
        let original = format!("{drill:?}");
        for (i, fence) in PALW_T12_POST_LAUNCH_FENCES_V1.iter().enumerate() {
            assert!(fences.iter().any(|(name, _)| *name == fence.name), "{} is a Params::palw_fences_v1 name", fence.name);
            assert!(PALW_T12_POST_LAUNCH_FENCES_V1[..i].iter().all(|other| other.name != fence.name), "{} listed once", fence.name);
            for preset in [&MAINNET_PARAMS, &TESTNET_PARAMS, &SIMNET_PARAMS, &DEVNET_PARAMS] {
                assert_eq!(height(preset, fence.name), None, "{} is dormant on {}", fence.name, preset.net);
            }
            let was = height(&drill, fence.name);
            let mut one = drill.clone();
            (fence.set)(&mut one, Some(ForkActivation::new(40)));
            for ((name, before), (_, after)) in fences.iter().zip(one.palw_fences_v1().iter()) {
                if *name == fence.name {
                    assert_eq!(*after, Some(ForkActivation::new(40)), "{name}'s entry sets it");
                } else {
                    assert_eq!(before, after, "{}'s entry moved {name}", fence.name);
                }
            }
            assert_ne!(one.consensus_params_id(), drill.consensus_params_id(), "{} is in the fingerprint", fence.name);
            (fence.set)(&mut one, was);
            assert_eq!(format!("{one:?}"), original, "{}'s entry touches its own fence and mirrors alone", fence.name);
        }
    }

    /// **ADR-0160's capacity list (rcore/cap-s1): fences, dormant EVERYWHERE, never on the DAA-750 list.**
    /// Every entry names a `Params::palw_fences_v1` fence, is listed once, is dormant on every preset —
    /// public testnet-12 and a drill ruleset included, which the post-launch list is not — and is not an
    /// entry of `PALW_T12_POST_LAUNCH_FENCES_V1` (armed at DAA 750 on every testnet-12 ruleset). Its `set`
    /// moves its own fence alone and, set back, leaves the ruleset byte-identical.
    #[test]
    fn the_capacity_fence_list_is_dormant_everywhere_and_apart_from_the_post_launch_list() {
        use crate::config::params::{
            DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, PALW_T12_CAPACITY_FENCES_V1, PALW_T12_POST_LAUNCH_FENCE_DAA,
            PALW_T12_CAPACITY_RHO10_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V1, SIMNET_PARAMS, TESTNET_PARAMS,
            palw_t12_drill_params_v1, palw_t12_release_v2_params, palw_t12_shipped_params,
        };
        assert!(!PALW_T12_CAPACITY_FENCES_V1.is_empty());
        let drill = palw_t12_drill_params_v1(&salt(0x53));
        // The DAA-1,300 release (every capacity entry dormant) is the "public" baseline here.
        let public = palw_t12_release_v2_params();
        let shipped = palw_t12_shipped_params();
        let height = |p: &crate::config::params::Params, name: &str| {
            p.palw_fences_v1().into_iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("{name} is a fence")).1
        };
        for (i, fence) in PALW_T12_CAPACITY_FENCES_V1.iter().enumerate() {
            assert!(PALW_T12_CAPACITY_FENCES_V1[..i].iter().all(|other| other.name != fence.name), "{} listed once", fence.name);
            assert!(
                PALW_T12_POST_LAUNCH_FENCES_V1.iter().all(|other| other.name != fence.name),
                "{} is not on the DAA-750 list (that list is armed on the live chain)",
                fence.name
            );
            for preset in [&MAINNET_PARAMS, &TESTNET_PARAMS, &SIMNET_PARAMS, &DEVNET_PARAMS, &public] {
                assert_eq!(height(preset, fence.name), None, "{} is dormant on {}", fence.name, preset.net);
            }
            // testnet-12's third post-launch flag day arms the ρ = 10 package at 1,700 (the drill inherits it);
            // the later ρ steps stay dormant there too.
            let armed = PALW_T12_CAPACITY_RHO10_FENCES_V1.iter().any(|f| f.name == fence.name);
            let expect = armed.then(|| ForkActivation::new(1_700));
            assert_eq!(height(&shipped, fence.name), expect, "{} on testnet-12 as shipped", fence.name);
            assert_eq!(height(&drill, fence.name), expect, "{} on the release's drill", fence.name);
            let original = format!("{public:?}");
            let fences = public.palw_fences_v1();
            let mut one = public.clone();
            (fence.set)(&mut one, Some(ForkActivation::new(PALW_T12_POST_LAUNCH_FENCE_DAA + 1)));
            for ((name, before), (_, after)) in fences.iter().zip(one.palw_fences_v1().iter()) {
                if *name != fence.name && !name.starts_with(fence.name) {
                    assert_eq!(before, after, "{}'s entry moved {name}", fence.name);
                }
            }
            assert_ne!(one.consensus_params_id(), public.consensus_params_id(), "{} is in the fingerprint", fence.name);
            (fence.set)(&mut one, None);
            assert_eq!(format!("{one:?}"), original, "{}'s entry touches its own fence and mirrors alone", fence.name);
        }
    }

    /// **A drill crosses the capacity flag day** ([`palw_drill_capacity_fences_at_v1`]): refused on public
    /// testnet-12's genesis, at 0 / never, at another fence's height, and below the release's post-launch
    /// fences the capacity fences require (strict-win and lane A are at 750 on the release's drill); at a
    /// free height past them every capacity fence is ARMED, nothing else moves, the ruleset validates and
    /// the params id moves. With `--palw-drill-fence-at` first (the release's fences at 40), a capacity
    /// height of 60 is legal too.
    #[test]
    fn a_drill_crosses_the_capacity_flag_day_and_nothing_else_moves() {
        use crate::config::params::{ForkActivation, PALW_T12_CAPACITY_FENCES_V1, palw_t12_drill_params_v1, palw_t12_shipped_params};
        let drill = palw_t12_drill_params_v1(&salt(0x53));
        let mut public = palw_t12_shipped_params();
        assert!(palw_drill_capacity_fences_at_v1(&mut public, 1_234).unwrap_err().contains("PUBLIC testnet-12"));
        for bad in [0, u64::MAX] {
            let mut p = drill.clone();
            assert!(palw_drill_capacity_fences_at_v1(&mut p, bad).is_err(), "{bad}");
            assert_eq!(format!("{p:?}"), format!("{drill:?}"), "a refusal leaves the ruleset untouched");
        }
        let mut taken = drill.clone();
        assert!(palw_drill_capacity_fences_at_v1(&mut taken, 750).unwrap_err().contains("already"), "750 is the release's height");
        let mut early = drill.clone();
        assert!(palw_drill_capacity_fences_at_v1(&mut early, 700).unwrap_err().contains("does not validate"), "below strict-win and lane A");
        let mut moved = drill.clone();
        let moves = palw_drill_capacity_fences_at_v1(&mut moved, 1_234).expect("past the release's fences");
        assert_eq!(moves.iter().map(|m| m.name).collect::<Vec<_>>(), PALW_T12_CAPACITY_FENCES_V1.iter().map(|f| f.name).collect::<Vec<_>>());
        let rho10 = |name: &str| crate::config::params::PALW_T12_CAPACITY_RHO10_FENCES_V1.iter().any(|f| f.name == name);
        assert!(moves.iter().all(|m| m.at == 1_234
            && if rho10(m.name) { m.was == Some(1_700) && !m.to_string().contains("ARMED") } else { m.was.is_none() && m.to_string().contains("ARMED") }));
        assert!(moved.palw_capacity_weight_cap_active_at(1_234) && !moved.palw_capacity_weight_cap_active_at(1_233));
        assert_ne!(moved.consensus_params_id(), drill.consensus_params_id());
        for ((name, before), (_, after)) in drill.palw_fences_v1().iter().zip(moved.palw_fences_v1().iter()) {
            if !name.starts_with("palw_capacity_") {
                assert_eq!(before, after, "{name} did not move");
            }
        }
        let mut both = drill.clone();
        palw_drill_post_launch_fences_at_v1(&mut both, 40).expect("the release's flag day at 40");
        palw_drill_capacity_fences_at_v1(&mut both, 60).expect("the capacity flag day at 60, over the release's");
        assert_eq!(both.palw_capacity_weight_cap, Some(ForkActivation::new(60)));
    }

    /// **A drill appends the capacity ramp's ready steps and nothing else moves** ([`palw_drill_capacity_step2_at_v1`],
    /// `--palw-drill-capacity-step2-at`, ρ = 25; [`palw_drill_capacity_step3_at_v1`], `--palw-drill-capacity-step3-at`,
    /// ρ = 100 after it; [`palw_drill_capacity_rho100_at_v1`], `--palw-drill-capacity-rho100-at`, ρ = 100 straight
    /// after ρ = 10): refused on public testnet-12, at 0 and at `u64::MAX`, at another fence's height; refused by name
    /// (never panicking in the entry's `set`) where F-L does not carry the steps they build on or the height is not
    /// above the step before; on a drill whose ρ = 10 flag day is moved low, each appends exactly its one step with
    /// its ρ, the fork id names its height, the params id moves and no other fence moves.
    #[test]
    fn a_drill_appends_the_capacity_ramp_steps_and_nothing_else_moves() {
        use crate::config::params::{ForkActivation, palw_t12_drill_params_v1, palw_t12_shipped_params};
        let drill = palw_t12_drill_params_v1(&salt(0x57));
        let ramp = |p: &crate::config::params::Params| -> Vec<(u64, u32)> {
            p.palw_capacity_aggregate_liability.as_ref().map(|v| v.steps.iter().map(|s| (s.from_daa, s.rho)).collect()).unwrap_or_default()
        };
        let mut public = palw_t12_shipped_params();
        assert!(palw_drill_capacity_step2_at_v1(&mut public, 560).unwrap_err().contains("PUBLIC testnet-12"));
        for bad in [0, u64::MAX] {
            let mut p = drill.clone();
            assert!(palw_drill_capacity_step2_at_v1(&mut p, bad).is_err(), "{bad}");
            assert_eq!(format!("{p:?}"), format!("{drill:?}"), "a refusal leaves the ruleset untouched");
        }
        // The release arms F-L at DAA 1,700: a step below it is refused by name, not by a panic.
        let mut release_ramp = drill.clone();
        let why = palw_drill_capacity_step2_at_v1(&mut release_ramp, 560).unwrap_err();
        assert!(why.contains("must start above step 1") && why.contains("1700"), "{why}");
        assert_eq!(format!("{release_ramp:?}"), format!("{drill:?}"));
        // The ρ = 10 flag day moved low (the release's flag days first), then the ramp above it.
        let mut base = drill.clone();
        palw_drill_post_launch_fences_at_v1(&mut base, 40).expect("the release's fences at 40");
        palw_drill_post_launch_fences_v2_at_v1(&mut base, 60).expect("the second flag day at 60");
        palw_drill_post_launch_fences_v3_at_v1(&mut base, 80).expect("the capacity package at 80");
        assert_eq!(ramp(&base), vec![(80, 10)]);
        // Step 3 builds on step 2.
        let mut early = base.clone();
        let why = palw_drill_capacity_step3_at_v1(&mut early, 655).unwrap_err();
        assert!(why.contains("builds on 2 earlier step(s)") && why.contains("carries 1"), "{why}");
        // Another fence's height, and a height not above the step before.
        let mut taken = base.clone();
        assert!(
            palw_drill_capacity_step2_at_v1(&mut taken, 3_600).unwrap_err().contains("already"),
            "3,600 is the DAA-3,600 flag day's height, which the moves above left alone"
        );
        let mut flat = base.clone();
        assert!(palw_drill_capacity_step2_at_v1(&mut flat, 80).unwrap_err().contains("must start above"));
        // ρ = 25, then ρ = 100.
        let mut ramped = base.clone();
        let moves = palw_drill_capacity_step2_at_v1(&mut ramped, 560).expect("step 2 at 560");
        assert_eq!(moves.len(), 1);
        assert_eq!((moves[0].name, moves[0].was, moves[0].at), ("palw_capacity_aggregate_liability_step_2", None, 560));
        assert_eq!(ramp(&ramped), vec![(80, 10), (560, 25)]);
        assert_ne!(ramped.consensus_params_id(), base.consensus_params_id(), "the step is in the fingerprint");
        let named = |p: &crate::config::params::Params, name: &str| p.palw_fences_v1().iter().find(|(n, _)| *n == name).and_then(|(_, f)| *f);
        assert_eq!(named(&ramped, "palw_capacity_aggregate_liability_step_2"), Some(ForkActivation::new(560)), "the fork id names it");
        for ((name, before), (_, after)) in base.palw_fences_v1().iter().zip(ramped.palw_fences_v1().iter()) {
            if *name != "palw_capacity_aggregate_liability_step_2" {
                assert_eq!(before, after, "{name} did not move");
            }
        }
        let mut late = ramped.clone();
        assert!(palw_drill_capacity_step3_at_v1(&mut late, 560).unwrap_err().contains("must start above step 2"));
        let moves = palw_drill_capacity_step3_at_v1(&mut ramped, 655).expect("step 3 at 655");
        assert_eq!((moves[0].name, moves[0].was, moves[0].at), ("palw_capacity_aggregate_liability_step_3", None, 655));
        assert_eq!(ramp(&ramped), vec![(80, 10), (560, 25), (655, 100)]);
        ramped.validate_palw_v2().expect("the ramp validates");
        // ρ = 100 straight after ρ = 10 is the second step too.
        let mut straight = base.clone();
        let moves = palw_drill_capacity_rho100_at_v1(&mut straight, 560).expect("rho 100 straight at 560");
        assert_eq!(moves[0].name, "palw_capacity_aggregate_liability_step_2");
        assert_eq!(ramp(&straight), vec![(80, 10), (560, 100)]);
        assert_ne!(straight.consensus_params_id(), ramped.consensus_params_id());
        // F-N moves alone, never below the verify room and the issuance slots it needs (armed at 80 with the list).
        let mut room = base.clone();
        let mut public = palw_t12_shipped_params();
        assert!(palw_drill_capacity_network_room_at_v1(&mut public, 120).unwrap_err().contains("PUBLIC testnet-12"));
        let moves = palw_drill_capacity_network_room_at_v1(&mut room, 120).expect("F-N later than the rest of the list");
        assert_eq!((moves[0].name, moves[0].was, moves[0].at), ("palw_capacity_network_room", Some(80), 120));
        assert_eq!(moves.len(), 1);
        assert_ne!(room.consensus_params_id(), base.consensus_params_id());
        for ((name, before), (_, after)) in base.palw_fences_v1().iter().zip(room.palw_fences_v1().iter()) {
            if *name != "palw_capacity_network_room" {
                assert_eq!(before, after, "{name} did not move");
            }
        }
        let mut early = base.clone();
        assert!(
            palw_drill_capacity_network_room_at_v1(&mut early, 70).unwrap_err().contains("does not validate"),
            "below the verify room and the issuance slots"
        );
        // int-11: the static verification term arms (it is dormant everywhere) at or above F-N, its mirror follows, and
        // nothing else moves; below F-N it is refused.
        let mut verify = base.clone();
        let moves = palw_drill_capacity_network_verify_at_v1(&mut verify, 120).expect("the term above F-N (80)");
        assert_eq!((moves[0].name, moves[0].was, moves[0].at), ("palw_capacity_network_verify", None, 120));
        assert_eq!(moves.len(), 1);
        match &verify.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                assert_eq!(bundle.state.capacity_network_verify_from_daa(), Some(120), "the fold's mirror");
                assert!(bundle.state.capacity_network_verify_active_at(120) && !bundle.state.capacity_network_verify_active_at(119));
            }
            _ => panic!("testnet-12 is ConsensusV2"),
        }
        assert_ne!(verify.consensus_params_id(), base.consensus_params_id(), "the term is in the fingerprint");
        let mut below = base.clone();
        assert!(
            palw_drill_capacity_network_verify_at_v1(&mut below, 70).unwrap_err().contains("does not validate"),
            "below F-N, which sits at 80"
        );
        let mut later_room = base.clone();
        palw_drill_capacity_network_room_at_v1(&mut later_room, 150).expect("F-N later");
        assert!(
            palw_drill_capacity_network_verify_at_v1(&mut later_room, 120).unwrap_err().contains("does not validate"),
            "F-N moved above the term"
        );
        let mut public = palw_t12_shipped_params();
        assert!(palw_drill_capacity_network_verify_at_v1(&mut public, 120).unwrap_err().contains("PUBLIC testnet-12"));
    }

    /// **A drill crosses the DAA-3,600 flag day and nothing else moves** ([`palw_drill_tir_fence2_at_v1`],
    /// `--palw-drill-tir2-at`): refused on public testnet-12, at 0 and at `u64::MAX`; below the IR fence (moved
    /// low with `--palw-drill-tir-at`) the result does not validate; past it `palw_tir_fence2` alone is MOVED
    /// from the release's DAA 3,600 (the model court window stays dormant, as on the release), its mirror
    /// follows, and the params id moves.
    #[test]
    fn a_drill_arms_the_second_ir_fence_and_nothing_else_moves() {
        use crate::config::params::palw_t12_shipped_params;
        let mut drill = crate::config::params::palw_t12_drill_params_v1(&salt(0x55));
        palw_drill_tir_fence_at_v1(&mut drill, 1_020).expect("the IR fence low");
        let mut public = palw_t12_shipped_params();
        assert!(palw_drill_tir_fence2_at_v1(&mut public, 1_234).unwrap_err().contains("PUBLIC testnet-12"));
        for bad in [0, u64::MAX] {
            let mut p = drill.clone();
            assert!(palw_drill_tir_fence2_at_v1(&mut p, bad).is_err(), "{bad}");
            assert_eq!(format!("{p:?}"), format!("{drill:?}"), "a refusal leaves the ruleset untouched");
        }
        let mut early = drill.clone();
        assert!(palw_drill_tir_fence2_at_v1(&mut early, 1_019).unwrap_err().contains("does not validate"), "below palw_tir_v1");
        let mut moved = drill.clone();
        let moves = palw_drill_tir_fence2_at_v1(&mut moved, 1_030).expect("past the IR fence");
        assert_eq!(moves.len(), 1);
        assert_eq!(
            (moves[0].name, moves[0].was),
            ("palw_tir_fence2", crate::config::params::PALW_T12_TIR_FENCE2_DAA),
            "the release arms it at DAA 3,600, alone: the flag moves it and nothing else"
        );
        assert!(moves[0].to_string().contains("MOVED"), "{}", moves[0]);
        assert!(moved.palw_tir_fence2_active_at(1_030) && !moved.palw_tir_fence2_active_at(1_029));
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &moved.palw_consensus_mode else { panic!("V2") };
        assert_eq!(bundle.state.tir_fence2_from_daa(), Some(1_030), "the fold's mirror");
        assert_ne!(moved.consensus_params_id(), drill.consensus_params_id());
        assert_eq!(moved.consensus_identity_id(), drill.consensus_identity_id());
        for ((name, before), (_, after)) in drill.palw_fences_v1().iter().zip(moved.palw_fences_v1().iter()) {
            if *name != "palw_tir_fence2" {
                assert_eq!(before, after, "{name} did not move");
            }
        }
    }

    /// **A drill crosses the IR fence** ([`palw_drill_tir_fence_at_v1`], `--palw-drill-tir-at`): refused on
    /// public testnet-12's genesis, at 0 / never and at another fence's height; below its prerequisites
    /// the result does not validate; at a free height past them `palw_tir_v1` alone is MOVED (live from
    /// the height, not below it, its bundle mirror following), nothing else moves and the params id moves.
    #[test]
    fn a_drill_crosses_the_ir_fence_and_nothing_else_moves() {
        use crate::config::params::{palw_t12_drill_params_v1, palw_t12_shipped_params};
        let drill = palw_t12_drill_params_v1(&salt(0x54));
        let mut public = palw_t12_shipped_params();
        assert!(palw_drill_tir_fence_at_v1(&mut public, 1_234).unwrap_err().contains("PUBLIC testnet-12"));
        for bad in [0, u64::MAX] {
            let mut p = drill.clone();
            assert!(palw_drill_tir_fence_at_v1(&mut p, bad).is_err(), "{bad}");
            assert_eq!(format!("{p:?}"), format!("{drill:?}"), "a refusal leaves the ruleset untouched");
        }
        let prerequisite = [drill.palw_kary_court, drill.palw_rcore_plus]
            .into_iter()
            .map(|f| f.map(|f| f.daa_score()).unwrap_or(u64::MAX))
            .max()
            .expect("two prerequisites");
        if prerequisite > 1 && prerequisite < u64::MAX {
            let mut early = drill.clone();
            let below =
                (1..prerequisite).rev().find(|h| drill.palw_fences_v1().iter().all(|(_, f)| f.is_none_or(|f| f.daa_score() != *h)));
            if let Some(below) = below {
                assert!(
                    palw_drill_tir_fence_at_v1(&mut early, below).unwrap_err().contains("does not validate"),
                    "below {prerequisite}"
                );
            }
        }
        let at = prerequisite.max(1_000) + 234;
        let mut moved = drill.clone();
        let moves = palw_drill_tir_fence_at_v1(&mut moved, at).expect("past the prerequisites");
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].name, "palw_tir_v1");
        assert_eq!(moves[0].was, crate::config::params::PALW_T12_TIR_FLAG_DAY_DAA, "the release's height");
        assert!(moves[0].to_string().contains("MOVED"), "{}", moves[0]);
        assert!(moved.palw_tir_v1_active_at(at) && !moved.palw_tir_v1_active_at(at - 1));
        assert_ne!(moved.consensus_params_id(), drill.consensus_params_id());
        for ((name, before), (_, after)) in drill.palw_fences_v1().iter().zip(moved.palw_fences_v1().iter()) {
            if *name != "palw_tir_v1" {
                assert_eq!(before, after, "{name} did not move");
            }
        }
    }

    #[test]
    fn a_drill_moves_the_model_court_window_fence_independently() {
        use crate::config::params::{PALW_T12_MODEL_COURT_WINDOW_FENCES_V1, palw_t12_drill_params_v1, palw_t12_shipped_params};
        let drill = palw_t12_drill_params_v1(&salt(0x5D));
        let mut public = palw_t12_shipped_params();
        assert!(palw_drill_model_court_window_at_v1(&mut public, 3_100).unwrap_err().contains("PUBLIC testnet-12"));
        let mut base = drill.clone();
        palw_drill_post_launch_fences_at_v1(&mut base, 40).expect("move prerequisites below the independent flag");
        let mut moved = base.clone();
        let moves = palw_drill_model_court_window_at_v1(&mut moved, 60).expect("the independent model court flag day");
        assert_eq!(
            moves.iter().map(|m| m.name).collect::<Vec<_>>(),
            PALW_T12_MODEL_COURT_WINDOW_FENCES_V1.iter().map(|f| f.name).collect::<Vec<_>>()
        );
        assert_eq!(moves[0].was, crate::config::params::PALW_T12_MODEL_COURT_WINDOW_DAA, "the release arms it nowhere");
        assert_eq!(moves[0].was, None);
        assert!(moves[0].to_string().contains("ARMED"), "{}", moves[0]);
        assert!(!base.palw_model_court_window_active_at(60), "dormant on the drill's base, like the release");
        assert!(moved.palw_model_court_window_active_at(60) && !moved.palw_model_court_window_active_at(59));
        assert_ne!(moved.consensus_params_id(), base.consensus_params_id());
        for ((name, before), (_, after)) in base.palw_fences_v1().iter().zip(moved.palw_fences_v1().iter()) {
            if *name != "palw_model_court_window" {
                assert_eq!(before, after, "{name} did not move");
            }
        }
    }

    /// **`--palw-drill-fence-at` on a drill ruleset: the release's fences at the drill's height, and
    /// nothing else.** Every listed fence is ARMED from the release's dormant state (or MOVED once the
    /// release arms it), each rule is live from the height on and not below it (the registry mirror
    /// included), the ruleset validates, and the fingerprint, the schedule and the fork id move — the
    /// drill chain's params id is not the release drill's. Every other fence is where it was, and
    /// setting the listed ones back gives the release drill byte for byte.
    #[test]
    fn a_drill_crosses_the_post_launch_flag_day_at_a_low_height_and_nothing_else_moves() {
        use crate::config::params::{ForkActivation, PALW_T12_POST_LAUNCH_FENCES_V1, palw_t12_drill_params_v1};
        use crate::fork_id_v1::{fork_id_gate_fences_v1, fork_id_v1};
        let drill = palw_t12_drill_params_v1(&salt(0x53));
        let mut moved = drill.clone();
        let moves = palw_drill_post_launch_fences_at_v1(&mut moved, 40).expect("a drill crosses the flag day at DAA 40");
        assert_eq!(
            moves.iter().map(|m| m.name).collect::<Vec<_>>(),
            PALW_T12_POST_LAUNCH_FENCES_V1.iter().map(|f| f.name).collect::<Vec<_>>(),
            "every post-launch fence, in the list's order"
        );
        let fences = drill.palw_fences_v1();
        for m in &moves {
            let release = fences.iter().find(|(name, _)| *name == m.name).unwrap().1;
            assert_eq!(
                m.was,
                release.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score()),
                "{}: the release's height",
                m.name
            );
            assert_eq!(m.at, 40);
            let line = m.to_string();
            assert!(line.contains(m.name) && line.contains("DAA 40"), "{line}");
            assert!(
                if m.was.is_none() { line.contains("ARMED") } else { line.contains("MOVED") || line.contains("own height") },
                "{line}"
            );
        }
        // Each rule is live from the height, and not below it — the fold's mirror included.
        assert_eq!(moved.palw_registry_resilience, Some(ForkActivation::new(40)));
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &moved.palw_consensus_mode else { panic!("V2") };
        assert_eq!(bundle.state.registry_resilience_from_daa(), Some(40), "the fold's mirror follows");
        assert!(moved.palw_panel_seed_execution_active_at(40) && !moved.palw_panel_seed_execution_active_at(39));
        assert_eq!(
            moved.palw_heartbeat_transparent_same_chain_fence(),
            Some(ForkActivation::new(40)),
            "under a live transparency rule"
        );
        assert_eq!(moved.palw_reorg_strict_economic_win, Some(ForkActivation::new(40)), "the strict-economic-win reorg rule");
        // ADR-0160's capacity fences are NOT the post-launch release's (rcore/cap-s1): the release's drill
        // leaves them dormant; `palw_drill_capacity_fences_at_v1` crosses them.
        // The third flag day's capacity package keeps its own height (1,700) under this flag;
        // `--palw-drill-fence3-at` moves it.
        assert_eq!(moved.palw_capacity_weight_cap, Some(ForkActivation::new(1_700)), "the weight cap keeps the release's 1,700");
        assert_eq!(bundle.state.capacity_weight_cap_from_daa(), Some(1_700), "and so does its fold mirror");
        assert_eq!(
            (moved.palw_bond_maturity_window_at(39), moved.palw_bond_maturity_window_at(40)),
            (None, Some(1_000)),
            "D1 from 40"
        );
        assert!(moved.palw_model_sink_bound_active_at(40) && !moved.palw_model_sink_bound_active_at(39));
        assert!(moved.palw_operator_anchor_active_at(40) && !moved.palw_operator_anchor_active_at(39));
        assert!(moved.palw_final_lock_full_collateral_active_at(40) && !moved.palw_final_lock_full_collateral_active_at(39));
        assert!(moved.palw_final_lock_life_active_at(40) && !moved.palw_final_lock_life_active_at(39));
        assert_eq!(
            (bundle.state.final_lock_full_collateral_from_daa(), bundle.state.final_lock_life_from_daa()),
            (Some(40), Some(40)),
            "V02's two mirrors follow"
        );
        assert!(moved.palw_anchor_at_ceiling_active_at(40) && !moved.palw_anchor_at_ceiling_active_at(39));
        assert!(moved.palw_slashing_evidence_utxo_genuine_at(40) && !moved.palw_slashing_evidence_utxo_genuine_at(39));
        assert!(moved.palw_pruning_proof_strict_economic_win.is_some_and(|f| f.is_active(40) && !f.is_active(39)));
        // ADR-0160 lane escrow (F-E) is a capacity fence, not the DAA-750 release's: it keeps the third flag
        // day's 1,700 under this flag.
        assert!(!moved.palw_capacity_escrow_active_at(40) && !moved.palw_capacity_escrow_active_at(1_699));
        assert!(moved.palw_capacity_escrow_active_at(1_700));
        assert_eq!(bundle.state.capacity_escrow_from_daa(), Some(1_700), "the escrow lane's mirror keeps 1,700");
        assert_eq!(
            moved.palw_operator_anchor.as_ref().map(|rule| rule.operators.len()),
            Some(8),
            "lane A over the drill's eight genesis bonds (its own registry, not public testnet-12's)"
        );
        moved.validate_palw_v2().expect("the moved drill ruleset validates");
        // The drill chain's ids move with the heights; the genesis is the salt's, untouched.
        assert_eq!(moved.genesis.hash, drill.genesis.hash);
        assert_ne!(moved.consensus_params_id(), drill.consensus_params_id(), "the drill chain's params id differs");
        assert_ne!(moved.consensus_schedule_id(), drill.consensus_schedule_id());
        assert!(fork_id_gate_fences_v1(&moved).contains(&40) && !fork_id_gate_fences_v1(&drill).contains(&40));
        assert_ne!(fork_id_v1(&moved, 0), fork_id_v1(&drill, 0), "a drill node without the flag announces another fork id");
        // Nothing else: every other fence where it was, and the listed ones set back give the release drill.
        for ((name, before), (_, after)) in fences.iter().zip(moved.palw_fences_v1().iter()) {
            if PALW_T12_POST_LAUNCH_FENCES_V1.iter().all(|f| f.name != *name) {
                assert_eq!(before, after, "{name} did not move");
            }
        }
        let mut back = moved.clone();
        for (fence, m) in PALW_T12_POST_LAUNCH_FENCES_V1.iter().zip(&moves) {
            (fence.set)(&mut back, m.was.map(ForkActivation::new));
        }
        assert_eq!(format!("{back:?}"), format!("{drill:?}"), "the release's fences are the whole difference");
        assert_eq!(back.consensus_params_id(), drill.consensus_params_id());
    }

    /// **The move refuses by name, and a refusal leaves the ruleset as it came:** public testnet-12
    /// (its genesis is the release's), any other network, a height of 0 or never, another fence's
    /// height (invisible to the fork id), and a move that moves nothing.
    #[test]
    fn the_drill_fence_move_refuses_by_name_and_leaves_the_ruleset_alone() {
        use crate::config::params::{PALW_T12_POST_LAUNCH_FENCES_V1, Params, palw_t12_drill_params_v1};
        let drill = palw_t12_drill_params_v1(&salt(0x53));
        let refusal = |mut params: Params, at: u64| -> String {
            let before = format!("{params:?}");
            let why = palw_drill_post_launch_fences_at_v1(&mut params, at).expect_err("refused");
            assert_eq!(format!("{params:?}"), before, "a refusal leaves the ruleset as it came ({why})");
            why
        };
        assert!(refusal(Params::from(palw_drill_network_v1()), 40).contains("PUBLIC testnet-12"));
        for net in [NetworkType::Devnet, NetworkType::Simnet, NetworkType::Mainnet] {
            assert!(refusal(Params::from(NetworkId::new(net)), 40).contains("testnet-12 drill only"), "{net:?}");
        }
        assert!(refusal(Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11)), 40).contains("testnet-12 drill only"));
        assert!(refusal(drill.clone(), 0).contains("not genesis"));
        assert!(refusal(drill.clone(), u64::MAX).contains("not never"));
        let (other, height) = drill
            .palw_fences_v1()
            .into_iter()
            .filter(|(name, _)| PALW_T12_POST_LAUNCH_FENCES_V1.iter().all(|f| f.name != *name))
            .find_map(|(name, fence)| fence.map(|f| (name, f.daa_score())).filter(|(_, h)| *h != 0 && *h != u64::MAX))
            .expect("testnet-12 schedules a fence past genesis (palw_bond_maturity)");
        let why = refusal(drill.clone(), height);
        assert!(why.contains(other) && why.contains("fork id"), "{why}");
        let mut once = drill.clone();
        palw_drill_post_launch_fences_at_v1(&mut once, 40).unwrap();
        assert!(refusal(once, 40).contains("move nothing"));
    }
}
