//! kaspa-pq (misaka) genesis premine — **the 10B cap** (re-genesis 2026-08-30).
//!
//! **Every network's genesis mints exactly 10B MSK** ([`MISAKA_PREMINE_CAP_SOMPI`]; operator
//! decision 2026-08-30). One main-wallet UTXO holds the whole cap; everything else a genesis
//! carries — community allocations, genesis-bond collateral, bond fee floats — is **carved OUT
//! of the main wallet, never minted beside it**. The builder enforces this arithmetically
//! (`checked_sub` against the cap), so a community table that outgrows the cap fails the build
//! instead of inflating the genesis.
//!
//! History of the cap:
//! * 2026-06-17 re-genesis: 40 "vault" UTXOs × 0.1B + 9B main = 15B → **13B**.
//! * 2026-08-26: test networks' main reduced to 6B for a 10B total; mainnet kept 13B.
//! * **2026-08-30: the vault block is REMOVED and the cap is 10B on every network, mainnet
//!   included.** The 40 mainnet-custody vault addresses are gone from genesis; the genesis
//!   bonds' collateral is now carved from the main wallet and held at the main wallet's own
//!   key (see [`genesis_premine_utxos_for`]). Final supply follows the cap:
//!   10B genesis + 15B mined over 20 years = **25B** (`constants::MAX_SOMPI`; the emission
//!   schedule in `consensus/src/processes/coinbase.rs` is unchanged).
//!
//! Each UTXO locks to the standard single-key ML-DSA-87 P2PKH `scriptPubKey`
//! `OP_DUP OP_BLAKE2B_512 OP_DATA_64 <64-byte payload> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87`
//! (built by [`crate::dns_finality::p2pkh_mldsa87_spk`]), where the 64-byte payload
//! is the keyed BLAKE2b-512 address payload decoded from the recipient address. The
//! addresses are stored as text (not opaque hashes) so the premine is auditable.
//!
//! ## Custody — per-network main wallet (audit H-01)
//!
//! * **Mainnet** holds its 10B at the operator custody address ([`MAINNET_MAIN_ADDRESS`],
//!   ML-DSA-87 key held offline; ceremony complete).
//! * **testnet-11** (and formerly testnet-12) holds its main wallet at the operator's public
//!   PALW address ([`PALW_PUBLIC_MAIN_ADDRESS`], supplied 2026-08-20).
//! * **Every other test network** uses a Claude-managed key ([`TESTNET_MAIN_ADDRESS`]) derived
//!   from the PUBLIC seed [`tests::TESTNET_MAIN_SEED`] (regenerable, value-less) so a validator
//!   can be funded / stood up during re-genesis E2E validation. The
//!   `testnet_main_key_is_reproducible` test pins this.
//!
//! Multisig / P2SH is out of launch scope (ADR-0019 §8/§6.5).

use crate::{
    constants::SOMPI_PER_KASPA,
    network::{NetworkId, NetworkType},
    tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use kaspa_addresses::{Address, Version};
use kaspa_hashes::Hash64;

/// **THE premine cap: 10B MSK, every network, one invariant.** A genesis totals exactly this —
/// the carve-outs below (collateral, floats, community) are paid for by the main wallet, so
/// adding to them moves the main wallet's amount and never this number. Raising the cap is a
/// re-genesis of every network and an explicit operator decision; no code path may derive a
/// bigger genesis from a longer table.
pub const MISAKA_PREMINE_CAP_SOMPI: u64 = 10_000_000_000 * SOMPI_PER_KASPA;

/// Collateral carved from the main wallet for each genesis bond seat: 0.1B MSK, the same value
/// the seats have staked since the registry was minted (2026-08-22), so the resolved
/// `BondRegistered` collateral — and with it `palw_ruleset_id` — does not move with the 10B-cap
/// re-genesis. The collateral outputs sit at the bond seats' own outpoint indices
/// (`premine_outpoint(0..cards)`) but are OWNED by the main wallet's key: the operator stakes
/// the main wallet's money, not a separate custody block. Consensus locks them while the bond
/// is not retired (`palw_bond_collateral_is_locked_v2`), and the wallet's input selector
/// excludes them via `palw_locked_bond_outpoints_v2`.
pub const GENESIS_BOND_COLLATERAL_SOMPI: u64 = 100_000_000 * SOMPI_PER_KASPA;

/// The main wallet's outpoint index on [`MISAKA_PREMINE_TXID`].
///
/// Historical: the 2026-06-17 layout put 40 vault UTXOs at indices 0–39 and the main wallet at
/// 40. The vaults are gone (2026-08-30), but the main wallet KEEPS index 40 — and the genesis
/// bond collateral keeps indices 0..cards, and the fee floats keep 41..(41+cards) — because
/// these indices are addressed from outside this file: a bond's identity IS its collateral
/// outpoint (`PalwBondKeyV2(premine_outpoint(i))`, inside `palw_ruleset_id`), and fleet units
/// name float outpoints in their configs. The gap at 6..40 is deliberate; an outpoint is a
/// (txid, index) name, not a position in a dense array.
pub const MAIN_PREMINE_INDEX: u32 = 40;

/// Mainnet main-wallet (10B) custody address (operator-held ML-DSA-87 key).
const MAINNET_MAIN_ADDRESS: &str =
    "misaka:q20f8cwx3uyhwhej6d994h28wxj2k4efd46grtkqpx4vaenaeyr5dsve3m3uzkhm6vx0897py3378qttk0dq0ndh9aqlwg25emf33jsgtcpswdj3";

/// The main wallet for the PUBLIC PALW test nets (testnet-11 and the PALW-RC testnet-12),
/// operator-supplied 2026-08-20.
///
/// It replaces [`TESTNET_MAIN_ADDRESS`] on those two networks and nowhere else: devnet/simnet
/// keep the regenerable Claude-managed key their harnesses depend on. A text address, like every
/// other allocation in this file, so the genesis is auditable by reading it rather than by
/// decoding a payload.
const PALW_PUBLIC_MAIN_ADDRESS: &str =
    "misakatest:qf7hzj76mg0wrch9mm89ag8s8apgrz7qgkk77j5z0ypykngrl2ayd2rnvleafk0fxhaxl70kr29x6fakav79jax9ul6jghrcs42nmlqx0tawqn8x";

/// Testnet/devnet/simnet main-wallet address — Claude-managed, regenerable from
/// `tests::TESTNET_MAIN_SEED` (value-less). Pinned by `testnet_main_key_is_reproducible`.
const TESTNET_MAIN_ADDRESS: &str =
    "misakatest:qtpflz03z576h02mtpn2vtwg5npj8fhlau3fgmsjl2a2uw0venj3573l07uahcs4gnsl8eqc7nlq5phakthxy606q2jyuxh2a08weduxa2yqlxuz";

/// audit H-01: the mainnet premine ceremony is **COMPLETE** — the custody address above replaces
/// the former all-zero unspendable placeholder, so mainnet is no longer locked. Guarded by
/// `mainnet_premine_is_spendable_custody`.
pub const MAINNET_PREMINE_CEREMONY_PENDING: bool = false;

/// Deterministic sentinel txid for the premine UTXOs: ASCII "misaka-premine" (14
/// bytes) zero-padded to the 64-byte `Hash64` width. Each premine UTXO sits at a
/// distinct index on this txid; fixed because it feeds the genesis `utxo_commitment`.
#[rustfmt::skip]
const MISAKA_PREMINE_TXID: [u8; 64] = [
    0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x70, 0x72, 0x65, 0x6d, 0x69, 0x6e, 0x65, // "misaka-premine"
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// Decode a premine recipient address to its 64-byte ML-DSA-87 owner payload. Panics
/// on a malformed address or wrong version — a startup guard analogous to the H-01
/// ceremony guard: a typo in a premine address must fail loudly, never silently lock
/// funds to the wrong script.
fn owner_payload(addr: &str) -> [u8; 64] {
    let a = Address::try_from(addr).unwrap_or_else(|e| panic!("premine address {addr} is invalid: {e:?}"));
    assert_eq!(a.version, Version::PubKeyHashMlDsa87, "premine address {addr} must be single-key ML-DSA-87 P2PKH");
    let p = a.payload.as_slice();
    assert_eq!(p.len(), 64, "premine address {addr} payload must be 64 bytes");
    let mut out = [0u8; 64];
    out.copy_from_slice(p);
    out
}

/// The main-wallet address for `network_type` (audit H-01): mainnet uses the
/// operator custody address; every test network uses the Claude-managed key.
fn main_address(network_type: NetworkType) -> &'static str {
    match network_type {
        NetworkType::Mainnet => MAINNET_MAIN_ADDRESS,
        NetworkType::Testnet | NetworkType::Devnet | NetworkType::Simnet => TESTNET_MAIN_ADDRESS,
    }
}

/// The main wallet for a network id — the suffix-aware form.
///
/// testnet-11 and the PALW-RC net (testnet-12) hold theirs at [`PALW_PUBLIC_MAIN_ADDRESS`];
/// every other network keeps [`main_address`]'s answer. The split is by NETWORK ID rather than
/// by type because that is the granularity the fact has: t11/t12 are the public PALW nets whose
/// genesis this operator is setting.
fn main_address_for(net: NetworkId) -> &'static str {
    if net.network_type == NetworkType::Testnet && matches!(net.suffix, Some(11) | Some(12)) {
        return PALW_PUBLIC_MAIN_ADDRESS;
    }
    main_address(net.network_type)
}

/// The outpoint of premine output `index` on the premine sentinel txid.
///
/// Every genesis output on the premine txid sits at a distinct index (bond collateral at
/// `0..cards`, the main wallet at [`MAIN_PREMINE_INDEX`], fee floats after it), so an outpoint
/// is fully determined by its index — and a caller that needs to NAME one (a PALW-RC genesis
/// bond's identity, audit C-08) should not have to rebuild the whole set and search it, nor
/// re-derive the txid and get it subtly wrong.
pub fn premine_outpoint(index: u32) -> TransactionOutpoint {
    TransactionOutpoint { transaction_id: Hash64::from_bytes(MISAKA_PREMINE_TXID), index }
}

fn premine_entry(amount: u64, script_public_key: ScriptPublicKey) -> UtxoEntry {
    UtxoEntry { amount, script_public_key, block_daa_score: 0, is_coinbase: false }
}

/// The canonical genesis premine for `network_type`: **one main-wallet UTXO of exactly the 10B
/// cap** at [`MAIN_PREMINE_INDEX`], single-key ML-DSA-87 P2PKH, spendable from block 0
/// (`is_coinbase: false`, no maturity delay).
pub fn misaka_premine_utxos(network_type: NetworkType) -> UtxoCollection {
    // `NetworkId::new` PANICS on a type that requires a suffix (testnet does), and this entry
    // point takes only the type — so the suffix-less answer is expressed directly rather than
    // by constructing an id that cannot exist. Callers who have a suffix use
    // `misaka_premine_utxos_for`, which is the only way to reach the public PALW nets' wallet.
    misaka_premine_utxos_inner(main_address(network_type))
}

/// The same set, chosen by NETWORK ID so the public PALW nets can hold their main wallet at
/// their own address (see [`main_address_for`]). [`misaka_premine_utxos`] is this with a
/// suffix-less id, which is what every non-suffixed caller means.
pub fn misaka_premine_utxos_for(net: NetworkId) -> UtxoCollection {
    misaka_premine_utxos_inner(main_address_for(net))
}

fn misaka_premine_utxos_inner(main: &str) -> UtxoCollection {
    let script_public_key = crate::dns_finality::p2pkh_mldsa87_spk(&owner_payload(main));
    UtxoCollection::from_iter([(premine_outpoint(MAIN_PREMINE_INDEX), premine_entry(MISAKA_PREMINE_CAP_SOMPI, script_public_key))])
}

/// The PALW public-testnet (testnet-11) COMMUNITY allocation — the operator-collected
/// address list for the t11 public relaunch (Discord, 2026-08-11 … 2026-08-26), baked into
/// the t11 genesis exactly like the premine: text addresses (auditable), one UTXO each on a
/// dedicated sentinel txid, committed by `TESTNET11_GENESIS.utxo_commitment`. **The whole
/// allocation is carved out of the main wallet** — see [`genesis_premine_utxos_for`] — so
/// growing this table moves the main wallet's amount and never the 10B cap.
///
/// **testnet-11 ONLY.** testnet-10, devnet, simnet and mainnet carry none of this.
///
/// Two entrants CHANGED their address before the cut and the superseded ones are excluded
/// (recorded here so the audit trail is in the file, not in a chat log):
/// * tetsu31 2026-08-11 `qfdqr02rxqyqh4jqtcn8qhwgsad3xqqn502tw26yajv7jg7eqap5slhggrcyngq8g789cxymezhc8mjfr3q2fj0w8j5w7mk986fta7u049hfph2n`
///   (no prefix as posted) → replaced 2026-08-18 by the entry below.
/// * uki 2026-08-13 `misakatest:qfa2z97yspcra7pel80h06jg4a6mg0669fj5qx63e4v5y8geddd8hvyvy75rqaejgrq69e8yv4nd66rzlt5tqepw95q7q3k55qev84g6ey5yj8x8`
///   → replaced 2026-08-19 by the entry below.
///
/// Amounts are whole MSK (× [`SOMPI_PER_KASPA`] at build). The fixed order feeds the genesis
/// `utxo_commitment` via the outpoint index, so it must never be reordered.
#[rustfmt::skip]
pub const TESTNET11_COMMUNITY_ALLOCATIONS: &[(&str, u64)] = &[
    // operator (2026-08-11)
    ("misakatest:qt0meznnlhgxx9h99yn78erahuyql0fnaeh9fxwjhw5j2qftsvsdjy38hm89ul7dfvddy0v2uqkgr4tqgr9nxp23xtn4tylf370f2k9f8hpry2wz", 100_000_000),
    // tetsu31 (changed address, 2026-08-18)
    ("misakatest:qt8j52desseh38y3ed5wzt452fqycl5xz8ptdm0yu2m4jpppesa353nkr4wc6gsnu48ald2qy592j7sztzpj93nlaay2wcy90xme9urqkfzywukt", 5_000_000),
    // Kurenai (re-registered address, 2026-08-23)
    ("misakatest:q2hftf0vsn23n5lpqzq9lealff4frahnr420lz4zwfjadtdfnq8jm375wddxrqa60f5ma706jq2j2htlrvgx7qf2xx04canvrtjq64n9r9e7tf4w", 30_000_000),
    // タケヤマ #1 (changed address, 2026-08-23). #2 below is deliberately unchanged: the operator
    // confirmed one wallet moved, not both, so the two entries still name two different addresses
    // and the 200M total does not move.
    ("misakatest:qtdksp2u6vkc8pxem5kecpt4cc9g4kdc9qkmaw2ehv530suad40rfvs2xwtx0s3j2hwvff6cg0ruqhu4fqsq6fft050h0gfwf464ume679e56p50", 100_000_000),
    // タケヤマ #2 (2026-08-12)
    ("misakatest:qgm8ft3wk722xp8ju7mv0weuhq9anqcp3q3v37fq2dz4xfhhc96ujw2hf39k6ncjav27mp2hkajyyyu4m4s8rgggaxtj8g2qtmuqgsk5y34fsncq", 100_000_000),
    // コタヌキM (2026-08-12)
    ("misakatest:qtpu9le2jr93fv094jasvl92x2ewqvh9xsnutzh3tegwy9x8amac5xjl40cwjx2yrl0w4dqnf8fsamagr024nmrdfsd7v2d7m97dqa7qcelse3lx", 1_000_000),
    // uki (changed address, 2026-08-19)
    ("misakatest:qt4uw0l8pemv6l0pqeuc247g2h3sp40kve88acz5xfzer3hwjaafw83jv77s8hemyyxnktc2v5zdu3v22s7d4067gtzupttchy23ycqym5vn82w6", 5_000_000),
    // あかぼね (2026-08-17)
    ("misakatest:qfcqlqw7kfgtg9g09rsz3m0e808th2e0p4stz0r4hn8prtnmvp6xy9adngl3xhfkyplpppwehfh7vkqlvqenhh2rj5sp388mezrc8tnk5uyxt65r", 5_000_000),
    // kamil (2026-08-17)
    ("misakatest:qga0xgy5xctju8da7scuwfxj93e205er5fs59qcr5w57nejl9h93rgt9thjnd87mmv5z98wxv26ewzqha4496nnxnza66s9l3jgyk5pq0wmepk43", 1_000_000),
    // **New members are APPENDED, never inserted.** The outpoint index is the position in this
    // table, so inserting one in the middle silently re-points every entry after it at a different
    // person's money.
    // Daifuku (2026-08-22)
    ("misakatest:q2wr70tgc8rtnz54026l5xntydq5hp7nuvp0fyxv30e2sk9jx2kc4gyt7tet0htttl5p4d36lyt25dshm45kqefdjpu8kcjurrakwx74rlpceqyt", 100_000_000),
    // ほうじ茶 (2026-08-26)
    ("misakatest:qgz4mlmfhw39yfh2dg3xul2ex9es9pxyrn46wu7th4gl2ldchugw85j30a84kv3wwv5r6ls9wg8ffzd5huzecejh7fhf3gf7rjgtcvlwdeqfcy34", 100_000_000),
];

/// Total community allocation: 547M MSK (100+5+30+100+100+1+5+5+1+100+100).
pub const TESTNET11_COMMUNITY_SOMPI: u64 = 547_000_000 * SOMPI_PER_KASPA;

/// Deterministic sentinel txid for the t11 community UTXOs: ASCII "misaka-t11-community"
/// (20 bytes) zero-padded to 64. Distinct from [`MISAKA_PREMINE_TXID`] so the two tables can
/// never collide on an outpoint whatever their lengths become.
#[rustfmt::skip]
const TESTNET11_COMMUNITY_TXID: [u8; 64] = [
    0x6d, 0x69, 0x73, 0x61, 0x6b, 0x61, 0x2d, 0x74, 0x31, 0x31, 0x2d, // "misaka-t11-"
    0x63, 0x6f, 0x6d, 0x6d, 0x75, 0x6e, 0x69, 0x74, 0x79,             // "community"
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// The t11 community UTXO set: one single-key ML-DSA-87 P2PKH UTXO per entry, spendable from
/// block 0, indices `0..TESTNET11_COMMUNITY_ALLOCATIONS.len()` on the community sentinel txid.
pub fn testnet11_community_utxos() -> UtxoCollection {
    let txid = Hash64::from_bytes(TESTNET11_COMMUNITY_TXID);
    let mut utxos: Vec<(TransactionOutpoint, UtxoEntry)> = Vec::with_capacity(TESTNET11_COMMUNITY_ALLOCATIONS.len());
    for (i, (addr, whole_msk)) in TESTNET11_COMMUNITY_ALLOCATIONS.iter().enumerate() {
        let script_public_key = crate::dns_finality::p2pkh_mldsa87_spk(&owner_payload(addr));
        let outpoint = TransactionOutpoint { transaction_id: txid, index: i as u32 };
        let amount = whole_msk.checked_mul(SOMPI_PER_KASPA).expect("a community allocation cannot overflow sompi");
        utxos.push((outpoint, premine_entry(amount, script_public_key)));
    }
    UtxoCollection::from_iter(utxos)
}

/// **The fee float each PALW-RC genesis bond receives, and why a network needs one.**
///
/// A `ConsensusV2` producer earns NOTHING it can spend until one of its claims reaches `Final`:
/// the shipped split puts 62 % of the subsidy in the worker base and escrows exactly 62 %, so the
/// coinbase pays the producer `worker_base − escrow = 0`. The escrow is released by a
/// `ReceiptLicensed` object, which rides a 0x4b transaction, which needs a funded input. Mining
/// income requires a finalized claim; finalizing a claim requires mining income. **The loop is
/// closed, and no amount of running the chain opens it** — testnet-12's first launch produced
/// 600 blocks and could not license one.
///
/// So the genesis opens it, because the genesis is the only place that can. Each registered
/// bond's PAYOUT address — an address the card already proves an operator holds the key for —
/// receives a small spendable float, carved out of the main wallet under the 10B cap. 100 MSK
/// covers roughly thirty thousand lifecycle submissions at the production relay rate (~300k
/// sompi each); the bonds need one working submitter, not an endowment.
pub const PALW_RC_BOND_FEE_FLOAT_SOMPI: u64 = 100 * SOMPI_PER_KASPA;

/// The FULL genesis UTXO set for one network id, and **the only place the 10B cap is spent**.
///
/// Keyed by [`NetworkId`] rather than [`NetworkType`] because t10 and t11 share a type and must
/// NOT share a UTXO set. Every network answers with UTXOs summing to EXACTLY
/// [`MISAKA_PREMINE_CAP_SOMPI`]:
///
/// * **testnet-11** — the network with a `ConsensusV2` registry — carves, out of the main
///   wallet: one collateral output per genesis bond (at the bond's own outpoint index
///   `0..cards`, owned by the main wallet's key — the operator stakes the main wallet's money),
///   one fee float per bond (at `MAIN_PREMINE_INDEX + 1 + i`, owned by that bond's payout key),
///   and the community allocation (its own txid). The main wallet holds the remainder.
/// * **every other network** — one main-wallet UTXO of the whole cap.
pub fn genesis_premine_utxos_for(net: NetworkId) -> UtxoCollection {
    if net.network_type == NetworkType::Testnet && net.suffix == Some(11) {
        return testnet11_genesis_utxos(net);
    }
    misaka_premine_utxos_for(net)
}

/// The testnet-11 genesis set: main wallet + per-bond collateral and floats + community, built
/// in ONE pass so the cap arithmetic is visible in one place. The predecessor of this function
/// inserted the main wallet twice (once at 6B, then overwritten at 9B−floats via `HashMap`
/// extend), which is exactly how the genesis quietly totalled 13.547B against a 10B decision —
/// a single construction path is the fix, not a smaller patch.
fn testnet11_genesis_utxos(net: NetworkId) -> UtxoCollection {
    let cards = crate::config::params::PALW_RC_GENESIS_BONDS;
    let main_spk = crate::dns_finality::p2pkh_mldsa87_spk(&owner_payload(main_address_for(net)));

    let mut utxos: Vec<(TransactionOutpoint, UtxoEntry)> = Vec::with_capacity(2 * cards.len() + TESTNET11_COMMUNITY_ALLOCATIONS.len() + 1);
    let mut carved: u64 = 0;

    // Genesis-bond collateral, at each card's DECLARED index (the outpoint IS the bond's
    // identity — `PalwBondKeyV2(premine_outpoint(card.premine_index))` — so the collateral goes
    // where the bond points, not where the card happens to sit in the list), owned by the main
    // wallet's key: "the main wallet bonds", there is no custody block.
    for card in cards {
        assert!(card.premine_index < MAIN_PREMINE_INDEX, "a genesis bond may not stake the main wallet itself or a float index");
        utxos.push((premine_outpoint(card.premine_index), premine_entry(GENESIS_BOND_COLLATERAL_SOMPI, main_spk.clone())));
        carved = carved.checked_add(GENESIS_BOND_COLLATERAL_SOMPI).expect("collateral cannot overflow");
    }

    // Per-bond fee floats, after the main wallet's index, at each bond's own payout key.
    for (i, card) in cards.iter().enumerate() {
        let script_public_key = crate::dns_finality::p2pkh_mldsa87_spk(&card.payout_payload);
        utxos.push((premine_outpoint(MAIN_PREMINE_INDEX + 1 + i as u32), premine_entry(PALW_RC_BOND_FEE_FLOAT_SOMPI, script_public_key)));
        carved = carved.checked_add(PALW_RC_BOND_FEE_FLOAT_SOMPI).expect("floats cannot overflow");
    }

    // The community allocation, on its own sentinel txid.
    for (outpoint, entry) in testnet11_community_utxos() {
        carved = carved.checked_add(entry.amount).expect("community cannot overflow");
        utxos.push((outpoint, entry));
    }

    // The main wallet pays for every carve-out above. `checked_sub` IS the 10B cap enforcement:
    // a carve list that outgrows the cap fails the build loudly, it never inflates the genesis.
    let main_amount = MISAKA_PREMINE_CAP_SOMPI
        .checked_sub(carved)
        .expect("the 10B premine cap is a hard invariant: collateral + floats + community exceed it — shrink the carve-outs, never raise the cap");
    utxos.push((premine_outpoint(MAIN_PREMINE_INDEX), premine_entry(main_amount, main_spk)));

    UtxoCollection::from_iter(utxos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::muhash::MuHashExtensions;
    use kaspa_muhash::MuHash;

    /// PUBLIC seed for the testnet main-wallet key. Claude-managed: the key is
    /// regenerable from this string (publicly recoverable, like any test key) and is
    /// for the VALUE-LESS test networks ONLY — used to fund / stand up a validator
    /// during the re-genesis E2E validation. NEVER mainnet.
    pub(super) const TESTNET_MAIN_SEED: &[u8] = b"misaka-testnet-premine-9b-claude-managed";

    /// **The operator's law (2026-08-30): every network's genesis mints exactly 10B MSK.**
    ///
    /// Community entries, bond collateral and fee floats are carved out of the main wallet,
    /// never minted beside it — so however those tables grow, this sum does not move. A future
    /// change to the cap has to move this test AND the constant, together, as an explicit
    /// re-genesis decision.
    #[test]
    fn every_network_genesis_mints_exactly_the_10b_cap() {
        let nets = [
            NetworkId::new(NetworkType::Mainnet),
            NetworkId::with_suffix(NetworkType::Testnet, 10),
            NetworkId::with_suffix(NetworkType::Testnet, 11),
            NetworkId::new(NetworkType::Devnet),
            NetworkId::new(NetworkType::Simnet),
        ];
        for net in nets {
            let total: u64 = genesis_premine_utxos_for(net).values().map(|e| e.amount).sum();
            assert_eq!(total, MISAKA_PREMINE_CAP_SOMPI, "{net} genesis must mint exactly the 10B cap");
            assert_eq!(total, 10_000_000_000 * SOMPI_PER_KASPA);
        }
        // The suffix-less type entry points answer the same cap.
        for net in [NetworkType::Mainnet, NetworkType::Testnet, NetworkType::Devnet, NetworkType::Simnet] {
            let total: u64 = misaka_premine_utxos(net).values().map(|e| e.amount).sum();
            assert_eq!(total, MISAKA_PREMINE_CAP_SOMPI, "{net:?} premine total");
        }
    }

    /// Every genesis output on every network is a 69-byte single-key ML-DSA-87 P2PKH spendable
    /// from block 0 — and the non-RC networks are exactly one main-wallet UTXO.
    #[test]
    fn premine_is_one_spendable_main_wallet_everywhere_else() {
        for net in [
            NetworkId::new(NetworkType::Mainnet),
            NetworkId::with_suffix(NetworkType::Testnet, 10),
            NetworkId::new(NetworkType::Devnet),
            NetworkId::new(NetworkType::Simnet),
        ] {
            let utxos = genesis_premine_utxos_for(net);
            assert_eq!(utxos.len(), 1, "{net} genesis is one main-wallet UTXO");
            let entry = &utxos[&premine_outpoint(MAIN_PREMINE_INDEX)];
            assert_eq!(entry.amount, MISAKA_PREMINE_CAP_SOMPI);
            assert!(!entry.is_coinbase, "premine must be non-coinbase (spendable from block 0)");
            assert_eq!(entry.block_daa_score, 0);
            assert_eq!(entry.script_public_key.script().len(), 69, "ML-DSA-87 P2PKH = 69 bytes");
        }
        for (_, entry) in genesis_premine_utxos_for(NetworkId::with_suffix(NetworkType::Testnet, 11)) {
            assert!(!entry.is_coinbase);
            assert_eq!(entry.block_daa_score, 0);
            assert_eq!(entry.script_public_key.script().len(), 69);
        }
    }

    /// **The RC genesis funds every bond and mints nothing extra.** The registry's collateral
    /// and floats, and the community allocation, are all inside the 10B cap; every registered
    /// bond can pay a fee at the address its own card names; and the collateral the genesis
    /// gate checks (audit C-08) sits at each bond's own outpoint, owned by the main wallet.
    #[test]
    fn the_rc_genesis_funds_every_bond_and_mints_nothing_extra() {
        let t11 = NetworkId::with_suffix(NetworkType::Testnet, 11);
        let set = genesis_premine_utxos_for(t11);
        let cards = crate::config::params::PALW_RC_GENESIS_BONDS;

        let total: u64 = set.values().map(|e| e.amount).sum();
        assert_eq!(total, MISAKA_PREMINE_CAP_SOMPI, "collateral, floats and community are carved from the main wallet, never minted");

        // NOT `if cards.is_empty() { return }`: that made every assertion below vacuous the moment
        // the card was unset, which is exactly when a reader would most want to know. The shipped
        // RC has a card, so the test demands one — a build that drops it fails here rather than
        // passing silently.
        assert!(!cards.is_empty(), "the shipped RC card must be set for this network to fund anything");
        assert_eq!(
            set.len(),
            2 * cards.len() + 1 + TESTNET11_COMMUNITY_ALLOCATIONS.len(),
            "one collateral + one float per bond, the main wallet, and t11's community entries"
        );

        let main_spk = set[&premine_outpoint(MAIN_PREMINE_INDEX)].script_public_key.clone();
        for (i, card) in cards.iter().enumerate() {
            // The collateral the bond's identity names is there, at the bond's declared index,
            // owned by the main wallet: the operator stakes the main wallet's money.
            let collateral = &set[&premine_outpoint(card.premine_index)];
            assert_eq!(collateral.amount, GENESIS_BOND_COLLATERAL_SOMPI, "bond {i} collateral");
            assert_eq!(collateral.script_public_key, main_spk, "bond {i} collateral is the main wallet bonding");
            // Every registered bond can pay a fee, at the address its own card names.
            let spk = crate::dns_finality::p2pkh_mldsa87_spk(&card.payout_payload);
            let funded: u64 = set.values().filter(|e| e.script_public_key == spk).map(|e| e.amount).sum();
            assert!(
                funded >= PALW_RC_BOND_FEE_FLOAT_SOMPI,
                "bond at premine #{} has no spendable float — it cannot submit a receipt quorum",
                card.premine_index
            );
        }

        // No OTHER network gains collateral or floats.
        let t10 = NetworkId::with_suffix(NetworkType::Testnet, 10);
        assert_eq!(genesis_premine_utxos_for(t10).len(), 1, "{t10} is one main-wallet UTXO");
    }

    /// **The public PALW net's main wallet is the operator's address, and ONLY there.**
    ///
    /// devnet and simnet keep the regenerable Claude-managed key their harnesses depend on, and
    /// mainnet's custody address is its own. On t11 the main wallet holds the cap minus exactly
    /// its carve-outs.
    #[test]
    fn the_public_palw_net_holds_the_main_wallet_at_the_operator_address() {
        let public_spk = crate::dns_finality::p2pkh_mldsa87_spk(&owner_payload(PALW_PUBLIC_MAIN_ADDRESS));
        let claude_spk = crate::dns_finality::p2pkh_mldsa87_spk(&owner_payload(TESTNET_MAIN_ADDRESS));
        assert_ne!(public_spk, claude_spk, "the fixture must actually differ or this test proves nothing");

        let main_of = |net: NetworkId| genesis_premine_utxos_for(net)[&premine_outpoint(MAIN_PREMINE_INDEX)].clone();

        let cards = crate::config::params::PALW_RC_GENESIS_BONDS;
        let t11_main = main_of(NetworkId::with_suffix(NetworkType::Testnet, 11));
        assert_eq!(t11_main.script_public_key, public_spk, "testnet-11 pays the operator address");
        let carved = (GENESIS_BOND_COLLATERAL_SOMPI + PALW_RC_BOND_FEE_FLOAT_SOMPI) * cards.len() as u64 + TESTNET11_COMMUNITY_SOMPI;
        assert_eq!(t11_main.amount, MISAKA_PREMINE_CAP_SOMPI - carved, "testnet-11 main wallet is the cap less its carve-outs");

        // testnet-10 and the suffix-less test networks keep the Claude-managed wallet.
        assert_eq!(main_of(NetworkId::with_suffix(NetworkType::Testnet, 10)).script_public_key, claude_spk);
        for net in [NetworkType::Devnet, NetworkType::Simnet] {
            let set = misaka_premine_utxos(net);
            assert_eq!(set[&premine_outpoint(MAIN_PREMINE_INDEX)].script_public_key, claude_spk, "{net:?} is untouched");
        }
        // Mainnet keeps its own custody address, which is neither of the above.
        let mainnet_main = main_of(NetworkId::new(NetworkType::Mainnet)).script_public_key;
        assert_ne!(mainnet_main, public_spk);
        assert_ne!(mainnet_main, claude_spk);
    }

    /// The community list is exactly what the operator collected: eleven entrants, 547M total, and
    /// everyone who changed address appears ONCE, at their new one.
    #[test]
    fn the_community_allocation_is_the_collected_list() {
        assert_eq!(TESTNET11_COMMUNITY_ALLOCATIONS.len(), 11);
        let total: u64 = TESTNET11_COMMUNITY_ALLOCATIONS.iter().map(|(_, msk)| *msk).sum();
        assert_eq!(total, 547_000_000, "100+5+30+100+100+1+5+5+1+100+100");
        assert_eq!(TESTNET11_COMMUNITY_SOMPI, total * SOMPI_PER_KASPA);

        // The superseded addresses are ABSENT — an entrant paid twice is an entrant paid wrong.
        // The 2026-08-23 round added two more: Kurenai re-registered and タケヤマ moved one of
        // their two wallets, and both old addresses must be gone rather than paid beside the new.
        for superseded in [
            "qfdqr02rxqyqh4jqtcn8qhwgsad3xqqn502tw26yajv7jg7eqap5slhggrcyngq8g789cxymezhc8mjfr3q2fj0w8j5w7mk986fta7u049hfph2n",
            "qfa2z97yspcra7pel80h06jg4a6mg0669fj5qx63e4v5y8geddd8hvyvy75rqaejgrq69e8yv4nd66rzlt5tqepw95q7q3k55qev84g6ey5yj8x8",
            "qtjw605sgh0uha25crcxy4sp8hl644x4ddl3msrtnurv3c4prz6cnag9hle8a5vyqkxgw54cl6tzyuap7j47yajf4wq3cl0tqdgup50rkdm9r4k3",
            "q2utpunet56y6hxlm0pg39mx6sd6zertjqmrf2vrwhv9grr769pga6dsxhncyteexr6hvs8gcxyaumwxveth2qupe06l6maqpc5jhlp96s64ys7a",
        ] {
            assert!(
                !TESTNET11_COMMUNITY_ALLOCATIONS.iter().any(|(a, _)| a.contains(superseded)),
                "a superseded address is still in the list"
            );
        }
        // …and every entry is distinct, so nobody is paid twice under two addresses either.
        let mut seen = std::collections::BTreeSet::new();
        for (addr, _) in TESTNET11_COMMUNITY_ALLOCATIONS {
            assert!(seen.insert(*addr), "duplicate community address {addr}");
            assert!(addr.starts_with("misakatest:"), "{addr} is not a testnet address");
        }
    }

    /// The community table is exactly the operator's collected list as a UTXO set: 11 UTXOs,
    /// 547M MSK, every address a well-formed testnet-prefix single-key ML-DSA-87 P2PKH (the
    /// bech32 checksum in `owner_payload` is what turns any transcription slip into a build
    /// failure instead of a silently mis-locked allocation), every owner distinct — including
    /// distinct from every main wallet — and the whole set confined to testnet-11.
    #[test]
    fn t11_community_allocation_is_the_collected_list() {
        use kaspa_addresses::Prefix;

        let utxos = testnet11_community_utxos();
        assert_eq!(utxos.len(), 11, "eleven entrants");
        let total: u64 = utxos.values().map(|e| e.amount).sum();
        assert_eq!(total, TESTNET11_COMMUNITY_SOMPI, "547M MSK exactly");
        assert_eq!(total, 547_000_000 * SOMPI_PER_KASPA);

        // Per-entry amounts, in table order (100/5/30/100/100/1/5/5/1/100/100 M). The order is the
        // outpoint index, so the two 2026-08-26 entrants are APPENDED — inserting either of them
        // earlier would hand every later index to a different person.
        let expected_msk = [
            100_000_000u64,
            5_000_000,
            30_000_000,
            100_000_000,
            100_000_000,
            1_000_000,
            5_000_000,
            5_000_000,
            1_000_000,
            100_000_000,
            100_000_000,
        ];
        let txid = Hash64::from_bytes(TESTNET11_COMMUNITY_TXID);
        for (i, want) in expected_msk.iter().enumerate() {
            let entry = &utxos[&TransactionOutpoint { transaction_id: txid, index: i as u32 }];
            assert_eq!(entry.amount, want * SOMPI_PER_KASPA, "entry {i} amount");
            assert!(!entry.is_coinbase, "spendable from block 0");
            assert_eq!(entry.block_daa_score, 0);
            assert_eq!(entry.script_public_key.script().len(), 69, "ML-DSA-87 P2PKH");
        }

        // Every address is testnet-prefixed (these are misakatest: recipients, never mainnet).
        for (addr, _) in TESTNET11_COMMUNITY_ALLOCATIONS {
            let parsed = Address::try_from(*addr).expect("community address parses");
            assert_eq!(parsed.prefix, Prefix::Testnet, "{addr} must be a testnet address");
        }

        // Distinct owners, and distinct from every main wallet.
        let mut owners: Vec<[u8; 64]> = TESTNET11_COMMUNITY_ALLOCATIONS.iter().map(|(a, _)| owner_payload(a)).collect();
        owners.push(owner_payload(MAINNET_MAIN_ADDRESS));
        owners.push(owner_payload(TESTNET_MAIN_ADDRESS));
        owners.push(owner_payload(PALW_PUBLIC_MAIN_ADDRESS));
        for i in 0..owners.len() {
            for j in (i + 1)..owners.len() {
                assert_ne!(owners[i], owners[j], "owner {i} and {j} collide");
            }
        }

        // Confinement: only testnet-11 carries the community set.
        let t11 = genesis_premine_utxos_for(NetworkId::with_suffix(NetworkType::Testnet, 11));
        assert_eq!(
            t11.len(),
            2 * crate::config::params::PALW_RC_GENESIS_BONDS.len() + 1 + TESTNET11_COMMUNITY_ALLOCATIONS.len(),
            "t11 = collateral + float per RC bond, the main wallet, and one per community entrant"
        );
        let t10 = genesis_premine_utxos_for(NetworkId::with_suffix(NetworkType::Testnet, 10));
        assert_eq!(t10.len(), 1, "t10 carries the main wallet alone");
        for net in [NetworkType::Mainnet, NetworkType::Devnet, NetworkType::Simnet] {
            assert_eq!(genesis_premine_utxos_for(NetworkId::new(net)).len(), 1, "{net:?} carries no community set");
        }
    }

    /// The three main wallets are pairwise distinct keys, so no network's premine is spendable
    /// by another custody domain's key.
    #[test]
    fn the_three_main_wallets_are_distinct() {
        let mainnet = owner_payload(MAINNET_MAIN_ADDRESS);
        let claude = owner_payload(TESTNET_MAIN_ADDRESS);
        let public = owner_payload(PALW_PUBLIC_MAIN_ADDRESS);
        assert_ne!(mainnet, claude);
        assert_ne!(mainnet, public);
        assert_ne!(claude, public);
    }

    /// audit H-01: the mainnet premine must be spendable custody (not the all-zero
    /// placeholder) and distinct from the publicly-recoverable testnet main key, so
    /// mainnet value can never be locked to an unspendable or public key.
    #[test]
    fn mainnet_premine_is_spendable_custody() {
        let mainnet_main = owner_payload(MAINNET_MAIN_ADDRESS);
        assert_ne!(mainnet_main, [0u8; 64], "mainnet main wallet must not be the all-zero placeholder");
        assert_ne!(mainnet_main, owner_payload(TESTNET_MAIN_ADDRESS), "mainnet main must differ from the public test key");
        assert!(!MAINNET_PREMINE_CEREMONY_PENDING, "ceremony is complete (custody addresses installed)");
    }

    /// The testnet main-wallet key is reproducible from [`TESTNET_MAIN_SEED`], so a
    /// validator can be funded / stood up during testing by regenerating the key. Pins
    /// [`TESTNET_MAIN_ADDRESS`] to the seed (any drift fails the build).
    #[test]
    fn testnet_main_key_is_reproducible() {
        use blake2b_simd::Params;
        use kaspa_hashes::blake2b_512_address_payload;
        use libcrux_ml_dsa::ml_dsa_87;

        let seed_hash = Params::new().hash_length(32).hash(TESTNET_MAIN_SEED);
        let mut seed = [0u8; 32];
        seed.copy_from_slice(seed_hash.as_bytes());
        let kp = ml_dsa_87::generate_key_pair(seed);
        let derived: [u8; 64] = blake2b_512_address_payload(kp.verification_key.as_ref()).as_bytes();
        assert_eq!(
            derived,
            owner_payload(TESTNET_MAIN_ADDRESS),
            "TESTNET_MAIN_ADDRESS must match the key derived from TESTNET_MAIN_SEED"
        );
    }

    /// Prints the per-network genesis `utxo_commitment`s to hardcode in `genesis.rs`.
    /// Run:
    /// `cargo test -p kaspa-consensus-core --lib config::premine::tests::print_premine_commitment -- --nocapture`
    #[test]
    fn print_premine_commitment() {
        for net in [NetworkType::Mainnet, NetworkType::Testnet, NetworkType::Devnet, NetworkType::Simnet] {
            let mut ms = MuHash::new();
            for (outpoint, entry) in misaka_premine_utxos(net) {
                ms.add_utxo(&outpoint, &entry);
            }
            let commitment = ms.finalize();
            let rust = commitment.as_bytes().iter().map(|b| format!("0x{b:02x}")).collect::<Vec<_>>().join(", ");
            println!("{net:?}_PREMINE_UTXO_COMMITMENT: Hash64::from_bytes([{rust}])");
        }
        // testnet-11: premine ∪ collateral ∪ floats ∪ community — the value
        // TESTNET11_GENESIS.utxo_commitment pins.
        let mut ms = MuHash::new();
        for (outpoint, entry) in genesis_premine_utxos_for(NetworkId::with_suffix(NetworkType::Testnet, 11)) {
            ms.add_utxo(&outpoint, &entry);
        }
        let commitment = ms.finalize();
        let rust = commitment.as_bytes().iter().map(|b| format!("0x{b:02x}")).collect::<Vec<_>>().join(", ");
        println!("TESTNET11_UTXO_COMMITMENT: Hash64::from_bytes([{rust}])");
    }
}

#[cfg(test)]
mod float_probe {
    use super::*;

    #[test]
    #[ignore = "probe"]
    fn print_float_addresses() {
        for (i, card) in crate::config::params::PALW_RC_GENESIS_BONDS.iter().enumerate() {
            let addr = Address::new(kaspa_addresses::Prefix::Testnet, Version::PubKeyHashMlDsa87, &card.payout_payload);
            println!("float {} -> {addr}", MAIN_PREMINE_INDEX + 1 + i as u32);
        }
    }
}
