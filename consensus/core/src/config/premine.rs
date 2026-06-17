//! kaspa-pq (misaka) genesis premine — 13B split (re-genesis 2026-06-17).
//!
//! 40 "vault" UTXOs of 0.1B KAS each + one "main" UTXO of 9B KAS = **13B KAS** total,
//! baked into genesis. This is the genesis portion of the **28B** final supply (the
//! other 15B is mined over 20 years; see the emission table in
//! `consensus/src/processes/coinbase.rs`). Premine was reduced 15B → 13B in this
//! re-genesis (total supply 30B → 28B; the mined half is unchanged).
//!
//! Each UTXO locks to the standard single-key ML-DSA-87 P2PKH `scriptPubKey`
//! `OP_DUP OP_BLAKE2B_512 OP_DATA_64 <64-byte payload> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87`
//! (built by [`crate::dns_finality::p2pkh_mldsa87_spk`]), where the 64-byte payload
//! is the keyed BLAKE2b-512 address payload decoded from the recipient address. The
//! addresses are stored as text (not opaque hashes) so the premine is auditable.
//!
//! ## Custody — per-network main wallet (audit H-01)
//!
//! * **40 vault addresses + the mainnet main-wallet address** are MAINNET custody
//!   addresses (ML-DSA-87 keys held offline by the operator). The 64-byte payloads
//!   are prefix-independent, so the same vault payloads are used on every network;
//!   on the value-less test networks they simply hold test coins.
//! * **The 9B main wallet differs per network:** mainnet uses the operator custody
//!   address ([`MAINNET_MAIN_ADDRESS`]); the test networks use a Claude-managed key
//!   ([`TESTNET_MAIN_ADDRESS`]) derived from the PUBLIC seed [`tests::TESTNET_MAIN_SEED`]
//!   (regenerable, value-less) so a validator can be funded / stood up during the
//!   re-genesis E2E validation. The `testnet_main_key_is_reproducible` test pins this.
//!
//! Multisig / P2SH is out of launch scope (ADR-0019 §8/§6.5).

use crate::{
    constants::SOMPI_PER_KASPA,
    network::NetworkType,
    tx::{TransactionOutpoint, UtxoEntry},
    utxo::utxo_collection::UtxoCollection,
};
use kaspa_addresses::{Address, Version};
use kaspa_hashes::Hash64;

/// Per-vault premine amount: 0.1B KAS.
pub const VAULT_PREMINE_SOMPI: u64 = 100_000_000 * SOMPI_PER_KASPA;
/// Main-wallet premine amount: 9B KAS.
pub const MAIN_PREMINE_SOMPI: u64 = 9_000_000_000 * SOMPI_PER_KASPA;
/// Number of vault UTXOs.
pub const VAULT_COUNT: usize = 40;
/// Total genesis premine = 40 × 0.1B + 9B = **13B KAS**.
pub const MISAKA_PREMINE_SOMPI: u64 = (VAULT_COUNT as u64) * VAULT_PREMINE_SOMPI + MAIN_PREMINE_SOMPI;

/// The 40 mainnet vault custody addresses (single-key ML-DSA-87 P2PKH). The payloads
/// are network-independent (used on every network); the fixed order feeds the genesis
/// `utxo_commitment` via the premine outpoint index, so it must never be reordered.
#[rustfmt::skip]
const VAULT_ADDRESSES: [&str; VAULT_COUNT] = [
    "misaka:q2sde8teys5z6302gw9ufz3edr3z330p0gvacpsgnn3hsdqdsat9mx9t6eu257kazeespx0s8628fmf3y7anwstkm7pkmjrahzf5xsmfhed42jdu",
    "misaka:qtsusn3gy7vqg9ewuhn078g3gwv2eg4vyjq06qe75m4ck7jh4r0j5jgulf3gmkznxwt5xhyancrujyj3vc20gsy8gsprht5g73yt8p0xlpdd3cx9",
    "misaka:qgd53sv9tvkpeep2at8lpdhs5m8jwced4538vdwxnvhf3j6km95yacjndvfm28unae8f66kvxfz0yq3mgzsy0lugrfputxt8ksnrlp47jpuy979s",
    "misaka:qf0rth45pqtray6c00sghsx537z5qmaz0ncr3gqanc4grkvvpk67xzrtgy9fycwags2a4cusz6wz2eu4xx87t0gsxg768lehesz6va8scljtf2wx",
    "misaka:qftzk72qe3fywjfa43en9r7854zw7rkk2jyzf7lzruu7kl0kawc7wcxvfyyswgwpsuq6pmh7fe842fkdy2ull29ky8vzy3z57ve7mr6fl3gad3qv",
    "misaka:qg7tpvwjrrgdh80pq2et6w29qkd7r7lczrp4u4w0fen2wg7qpe50pcxajphx7n97lppn0cualmzknx8f4ljmjyh49yepdt8xnz7ltgtgfck3pncp",
    "misaka:q2kl5trhgpaetj3ecp342q55td2ntvk3h9d2srd9x2p638t54zmpkcy4vj303d42ucrydmht0cppk7xf2lsw9ksd4hp9npyc2547ewax6sdetlru",
    "misaka:qt4n0ce5j3s70rsdewg4kct7jc33w7qxy6dkzhldr5lw429vwgy7j7fqqns9axkykhcfn7h3e78nys9g5p9hhyp4ax66a7pkjy0zh9ypsc59euju",
    "misaka:q2ut0gvkw2awqwm8cs08we4g7gyyk8fe4eaqwju9avh8tvu2e85z9pwmdxej2tkqudjw2ea4c7snjgsv5tckgm2g4jaffre85zqe9pjvu4juqnme",
    "misaka:qf78s5y6lz9q47ldmgj5dgvml0gvctgz545r4wdghw54s64dctw0kl3drkz840nxnx6a6qkd3jmlhesrk63uh0ga2fptt039ksvth4384xfzjme7",
    "misaka:qgvmgrxnmh000lnd8mznenlqqv7rckqmachj4vmldtnr39kck4rjwzar9qmgnaxaa89ttz2rfmja7f52phxrz9tltfy7f4rz9srr0k8799nrm5nv",
    "misaka:qt5rj4qqkuscxp002y088re6y4s7yy2lkpytatk4m0fgaqx2l87j6d3ry5m2f0deczh6qfuaptynzgj7z7zm2zkzdvjcnjg6jknghlphmttprru6",
    "misaka:qfa8nsdvdljwmtn5h7avmmgzwr4sr4e4uf5mfjq6cfqdpug8grx6uv73y0q2mgqk7542sl2pfd600w7hrz6zrfhrluu3hr3039zpka8msjzydqnl",
    "misaka:qt9pl87ukpz68v57s4xeknw3etsns2zquechttwegx9k9mt24ch8mty6tj0py0ufj89c8znkahhwd327a50fvm8lxxhcz0jc6zeaerfnctv7fr04",
    "misaka:qfqkhpw272twz53pz5zkmfekr7vfsx9k4r3s6fwyzeddk2anr0pnpfsw4ze5va628hu3lw3hnuwzm9qdren2t3zu7x7ljlnhhwp34m3my4tjgvwr",
    "misaka:q2k5wn53fsf7d0v8eq8hw7n22esq6u084cg2dyd3akyxfn8jqsmh4pama4w4u6jr8y2z08yajezsf0rsjx7rl76sm4d7z33pkk4ay0wgt0csmfy2",
    "misaka:qf6g4mn4j5hfc4dnh7k7escf8gzrk0e9e3vfycmvg9nnddfy3qskkxd8vsuteakv08zneyghxr0228fvtdnzrdasrf78k2ngndh03zhtl3sllz39",
    "misaka:qgezw07xtqpvq5dleawnqmc5yyluv2s07pg6zjepld7fl6na70c997g5tt73xdeqqne59m9qpmj8mchngv2ah33jujh6pg4sm6tqa7rfag66xrec",
    "misaka:qg4d0v9rs8m0rksdup8hv98nqpy53r7mw5hzqsvsf9jlk40ym26qhnyxdsxjs9jrsuumpz9nz85hh5dqjkad2frl3fwahrjywrmte8sewuqww79h",
    "misaka:qthru0q9737uart0vahnwefnwcd9325qn2cjx03kdr8ekfkh3rk6uvx8rnhpag6hazc5f8jtt42rfwqnjsz9xfwdtzafp9q8weeqplsnwu9mxf7j",
    "misaka:qt6kfzu97evtyv8xt7qqy4g9k5gh0xk4y8vhpjle69ls6829gvkysa5tma9aw5j5z4v4cv4qxhs4mm0m6n0wq60uy0vhxl9kfnrttaqyqz2p8tls",
    "misaka:qt6s3qldvvm3p44u2u5wu33gvy3whrjmd0ve6zllaj9zyh9fl26u30jzcxtcqk0y7tzk7hwa536m26afxylj63eum7r5e6rwv6hufkelnrv8hcm3",
    "misaka:q2cczy4e80cz9cfvmyvxd8tfl80l22k6w7v3vj656jtfajkh97m72pu3j0qtw7c6kdy3psafejkukgp0gl0whhp98qlqc2az4r42zs2frrj44uyp",
    "misaka:qt340a3r8dhrwmzvwhtlp8sy4p9r6r7xwkjkkj8vvf8zv0y593ceqs7c3vmct5hgxcw6ux9357vy5jjff9ps5wtyznpc7l29d42xlkrunhe5mp6f",
    "misaka:qfg5zgn4cxkhc4chw77zz8usrwkr07retgf3sgw54ss3ttaratfzq6lcv2t4v035324fr2pylxwgcv4e4jt78z8mq99jcmgmflq07l94ztx5fpn6",
    "misaka:qf2jprg2eh8uhuvqeaau89p4zzczg0xze44gkep5az8y5qsnpsasv6mpgfts5svqv9n0y84q4zejavmy4yc95u8jgc7y8j2fpuwuneursdapjr0v",
    "misaka:qff4lfgk4t3e4xp5awsy3adxy7yzrgcq8axkf60463q29m8pdz2ghxmwdv2gvz4amfmhdhlcncgd5tg8saahg3qt9u8sfwkrdp4amx8npzg3zea9",
    "misaka:q26dkxe8dzhcnwm97eg4ss29wcv3pfpk4eqsyenp8dwr9nc2fhssazuus600ghsgn2c2hucpm579hnvwdx9vghcq2y0wpk0ak6m5sx25evnj9hnf",
    "misaka:qgyme2jcerl7qch3v6la3gmk90v3225e9q5ysjpdzmk7hgfzm5c8y967msn0raj4q0hjezt542qfcxwq2ghkguarm2mqnqkethpgkdzsqjqexr56",
    "misaka:qfc0xt9gu4m5d5nn9ca5vfdtm2qs34a6hltglykpauezxwed0gg4z6ej6sfwpxw7s65vqqd208x9u9pczgdn90nvvp2jk7s8uelrmq7rd7nphc54",
    "misaka:qtgf3558nrlnyt0h03rk8shwuj4q0vfggxw7qjkqyzure9ej5uhawgkpajs46js2e3pa8t0vde09zg26mk6pgm0s646cdp7la4s2styke78zepms",
    "misaka:qfd2hrwjs8f0thgkjj88cqje2haq4hvwz00z3ze6wmvlph23jaa7v2em6q2lrhwh08c49kuhv6wv20shpe2sy482sm5xmvgjm4gc6uww7vdyc93y",
    "misaka:qg4w8alw7ztpmxng30xzgz4g2ud8yjjfu827v247dxj97luwswgrcmurwztjewgd5vjhlv2wkwdcdzwe8mkhhw99826t7y5f9p0swsqkgjgnhd0x",
    "misaka:qg4u70pe2u0hymj7fuvhsgsf2yxftvqm9gd9qqwj3hypqfkxurxcr9f3hvpdsxssgg0ha02jgn36qq0lh86tvcs7s696sed0l7knu00u0s9n5tl0",
    "misaka:qffn9zwgtz2vg8uprhxvwlxceh2llupevjts07qhsngu2404szr788kpmhn7wtmsvgcn8zu5t5q49cewwg32xyha94tawq4utt3lxxnchv63gkjs",
    "misaka:q29h7krvngrd06m04cn0d5w649247wm2dv7u8h0txttcdqyyufvyfufrjt60a6a0nurghsawe80t74at8wxnvd3j3wx6y8w0gnjn6tygenf2qquc",
    "misaka:qtylp2rwewz43zgxhkvx06qxtv7u2n3rhykvsrq7gfcnye9sxahdrv36ahp2qa9tt9f79cjgpqlcr78ljlxqyz537se7zna023x3q2ekre3u2ylx",
    "misaka:q20rut58ahknmvkarp288saw0l570cfls5qnrpvcfmludv6svfggj0987qz4yjfs0klyh47tr8ch266rx4pczrc7eqnj0h3p25smh8glqq0lwehr",
    "misaka:qfrc3w55s9ry966czynwgke0fvqfh9twwvwn6mcv9zww9wd5hytddqvrus7vsep2gq9ncgsys387drd4dgm777tla4z3weu20mj4j7ncflmhvctf",
    "misaka:qf08kurlrnqluqcdtrwpqellpkxcu2n0hreddl07t3fcfmq3a33h9f8mxqe9ktswcshpv0qnfr9d3ly9egf3drhz7ldkg79r7wdv6jsx4rmlm6m0",
];

/// Mainnet main-wallet (9B) custody address (operator-held ML-DSA-87 key).
const MAINNET_MAIN_ADDRESS: &str =
    "misaka:q20f8cwx3uyhwhej6d994h28wxj2k4efd46grtkqpx4vaenaeyr5dsve3m3uzkhm6vx0897py3378qttk0dq0ndh9aqlwg25emf33jsgtcpswdj3";

/// Testnet/devnet/simnet main-wallet (9B) address — Claude-managed, regenerable from
/// `tests::TESTNET_MAIN_SEED` (value-less). Pinned by `testnet_main_key_is_reproducible`.
const TESTNET_MAIN_ADDRESS: &str =
    "misakatest:qtpflz03z576h02mtpn2vtwg5npj8fhlau3fgmsjl2a2uw0venj3573l07uahcs4gnsl8eqc7nlq5phakthxy606q2jyuxh2a08weduxa2yqlxuz";

/// audit H-01: the mainnet premine ceremony is **COMPLETE** — the custody addresses
/// above replace the former all-zero unspendable placeholder, so mainnet is no longer
/// locked. Guarded by `mainnet_premine_is_spendable_custody`.
pub const MAINNET_PREMINE_CEREMONY_PENDING: bool = false;

/// Deterministic sentinel txid for the premine UTXOs: ASCII "misaka-premine" (14
/// bytes) zero-padded to the 64-byte `Hash64` width. Each premine UTXO sits at a
/// distinct index `0..=VAULT_COUNT` on this txid; fixed because it feeds the genesis
/// `utxo_commitment`.
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

/// The 9B main-wallet address for `network_type` (audit H-01): mainnet uses the
/// operator custody address; every test network uses the Claude-managed key.
fn main_address(network_type: NetworkType) -> &'static str {
    match network_type {
        NetworkType::Mainnet => MAINNET_MAIN_ADDRESS,
        NetworkType::Testnet | NetworkType::Devnet | NetworkType::Simnet => TESTNET_MAIN_ADDRESS,
    }
}

/// The canonical kaspa-pq genesis premine UTXO set for `network_type`: 40 vault UTXOs
/// of 0.1B KAS each (indices `0..VAULT_COUNT`) + one 9B main UTXO (index `VAULT_COUNT`)
/// = 13B KAS, all single-key ML-DSA-87 P2PKH and spendable from block 0
/// (`is_coinbase: false`, no maturity delay). The vault payloads are network-independent;
/// the 9B main wallet is per-network (see [`main_address`]).
pub fn misaka_premine_utxos(network_type: NetworkType) -> UtxoCollection {
    let txid = Hash64::from_bytes(MISAKA_PREMINE_TXID);
    let mut utxos: Vec<(TransactionOutpoint, UtxoEntry)> = Vec::with_capacity(VAULT_COUNT + 1);
    for (i, addr) in VAULT_ADDRESSES.iter().enumerate() {
        let script_public_key = crate::dns_finality::p2pkh_mldsa87_spk(&owner_payload(addr));
        let outpoint = TransactionOutpoint { transaction_id: txid, index: i as u32 };
        utxos.push((outpoint, UtxoEntry { amount: VAULT_PREMINE_SOMPI, script_public_key, block_daa_score: 0, is_coinbase: false }));
    }
    let script_public_key = crate::dns_finality::p2pkh_mldsa87_spk(&owner_payload(main_address(network_type)));
    let outpoint = TransactionOutpoint { transaction_id: txid, index: VAULT_COUNT as u32 };
    utxos.push((outpoint, UtxoEntry { amount: MAIN_PREMINE_SOMPI, script_public_key, block_daa_score: 0, is_coinbase: false }));
    UtxoCollection::from_iter(utxos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::muhash::MuHashExtensions;
    use kaspa_muhash::MuHash;

    /// PUBLIC seed for the testnet 9B main-wallet key. Claude-managed: the key is
    /// regenerable from this string (publicly recoverable, like any test key) and is
    /// for the VALUE-LESS test networks ONLY — used to fund / stand up a validator
    /// during the re-genesis E2E validation. NEVER mainnet.
    pub(super) const TESTNET_MAIN_SEED: &[u8] = b"misaka-testnet-premine-9b-claude-managed";

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
    }

    /// The premine is exactly 41 UTXOs (40 vaults × 0.1B + 1 main × 9B) = 13B KAS,
    /// each a 69-byte ML-DSA-87 P2PKH spendable from block 0.
    #[test]
    fn premine_is_the_13b_split() {
        for net in [NetworkType::Mainnet, NetworkType::Testnet] {
            let utxos = misaka_premine_utxos(net);
            assert_eq!(utxos.len(), VAULT_COUNT + 1, "premine is 40 vaults + 1 main = 41 UTXOs");
            let total: u64 = utxos.values().map(|e| e.amount).sum();
            assert_eq!(total, MISAKA_PREMINE_SOMPI, "premine total");
            assert_eq!(total, 13_000_000_000 * SOMPI_PER_KASPA, "13B KAS");
            let vaults = utxos.values().filter(|e| e.amount == VAULT_PREMINE_SOMPI).count();
            let mains = utxos.values().filter(|e| e.amount == MAIN_PREMINE_SOMPI).count();
            assert_eq!(vaults, VAULT_COUNT, "40 vault UTXOs of 0.1B");
            assert_eq!(mains, 1, "1 main UTXO of 9B");
            for entry in utxos.values() {
                assert!(!entry.is_coinbase, "premine must be non-coinbase (spendable from block 0)");
                assert_eq!(entry.block_daa_score, 0);
                assert_eq!(entry.script_public_key.script().len(), 69, "ML-DSA-87 P2PKH = 69 bytes");
            }
        }
    }

    /// All 41 owner payloads (40 vaults + the network's main wallet) are distinct, so
    /// no two premine UTXOs collide on the same key.
    #[test]
    fn premine_owners_are_distinct() {
        for net in [NetworkType::Mainnet, NetworkType::Testnet] {
            let mut payloads: Vec<[u8; 64]> = VAULT_ADDRESSES.iter().map(|a| owner_payload(a)).collect();
            payloads.push(owner_payload(main_address(net)));
            for i in 0..payloads.len() {
                for j in (i + 1)..payloads.len() {
                    assert_ne!(payloads[i], payloads[j], "{net:?}: premine owner {i} and {j} collide");
                }
            }
        }
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

    /// The testnet 9B main-wallet key is reproducible from [`TESTNET_MAIN_SEED`], so a
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
        assert_eq!(derived, owner_payload(TESTNET_MAIN_ADDRESS), "TESTNET_MAIN_ADDRESS must match the key derived from TESTNET_MAIN_SEED");
    }
}
