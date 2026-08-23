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
    network::{NetworkId, NetworkType},
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

/// The 9B main wallet for the PUBLIC PALW test nets (testnet-11 and the PALW-RC testnet-12),
/// operator-supplied 2026-08-20.
///
/// It replaces [`TESTNET_MAIN_ADDRESS`] on those two networks and nowhere else: testnet-10 has a
/// running chain whose `utxo_commitment` must not move, and devnet/simnet keep the regenerable
/// Claude-managed key their harnesses depend on. The 40 vault UTXOs are unchanged on every
/// network — this is the main wallet only, which is the ~9B of the 13B premine.
///
/// A text address, like every other allocation in this file, so the genesis is auditable by
/// reading it rather than by decoding a payload.
const PALW_PUBLIC_MAIN_ADDRESS: &str =
    "misakatest:qf7hzj76mg0wrch9mm89ag8s8apgrz7qgkk77j5z0ypykngrl2ayd2rnvleafk0fxhaxl70kr29x6fakav79jax9ul6jghrcs42nmlqx0tawqn8x";

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

/// The 9B main wallet for a network id — the suffix-aware form.
///
/// testnet-11 and the PALW-RC net (testnet-12) hold theirs at
/// [`PALW_PUBLIC_MAIN_ADDRESS`]; every other network keeps [`main_address`]'s answer. The split
/// is by NETWORK ID rather than by type because that is the granularity the fact has: t10 is a
/// chain with history and its commitment must not move, while t11 and t12 are the public PALW
/// nets whose genesis this operator is setting.
///
/// The 40 vault UTXOs are untouched everywhere — this replaces the main wallet only.
fn main_address_for(net: NetworkId) -> &'static str {
    if net.network_type == NetworkType::Testnet && matches!(net.suffix, Some(11) | Some(12)) {
        return PALW_PUBLIC_MAIN_ADDRESS;
    }
    main_address(net.network_type)
}

/// The canonical kaspa-pq genesis premine UTXO set for `network_type`: 40 vault UTXOs
/// of 0.1B KAS each (indices `0..VAULT_COUNT`) + one 9B main UTXO (index `VAULT_COUNT`)
/// = 13B KAS, all single-key ML-DSA-87 P2PKH and spendable from block 0
/// (`is_coinbase: false`, no maturity delay). The vault payloads are network-independent;
/// the 9B main wallet is per-network (see [`main_address`]).
/// The outpoint of premine vault `index` (`0..VAULT_COUNT`), or the 9B main wallet at
/// `VAULT_COUNT`.
///
/// Every genesis UTXO on every network sits at a distinct index on one fixed txid, so an
/// outpoint is fully determined by its index — and a caller that needs to NAME one (a PALW-RC
/// genesis bond locking a vault as its collateral, audit C-08) should not have to rebuild the
/// whole set and search it, nor re-derive the txid and get it subtly wrong.
pub fn premine_outpoint(index: u32) -> TransactionOutpoint {
    TransactionOutpoint { transaction_id: Hash64::from_bytes(MISAKA_PREMINE_TXID), index }
}

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
    let txid = Hash64::from_bytes(MISAKA_PREMINE_TXID);
    let mut utxos: Vec<(TransactionOutpoint, UtxoEntry)> = Vec::with_capacity(VAULT_COUNT + 1);
    for (i, addr) in VAULT_ADDRESSES.iter().enumerate() {
        let script_public_key = crate::dns_finality::p2pkh_mldsa87_spk(&owner_payload(addr));
        let outpoint = TransactionOutpoint { transaction_id: txid, index: i as u32 };
        utxos.push((outpoint, UtxoEntry { amount: VAULT_PREMINE_SOMPI, script_public_key, block_daa_score: 0, is_coinbase: false }));
    }
    let script_public_key = crate::dns_finality::p2pkh_mldsa87_spk(&owner_payload(main));
    let outpoint = TransactionOutpoint { transaction_id: txid, index: VAULT_COUNT as u32 };
    utxos.push((outpoint, UtxoEntry { amount: MAIN_PREMINE_SOMPI, script_public_key, block_daa_score: 0, is_coinbase: false }));
    UtxoCollection::from_iter(utxos)
}

/// The PALW public-testnet (testnet-11) COMMUNITY allocation — the operator-collected
/// address list for the t11 public relaunch (Discord, 2026-08-11 … 2026-08-19), baked into
/// the t11 genesis exactly like the premine: text addresses (auditable), one UTXO each on a
/// dedicated sentinel txid, committed by `TESTNET11_GENESIS.utxo_commitment`.
///
/// **testnet-11 ONLY.** testnet-10's running chain, devnet, simnet and mainnet carry none of
/// this — their commitments are untouched (see [`genesis_premine_utxos_for`]).
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
    // Kurenai (2026-08-11)
    ("misakatest:qtjw605sgh0uha25crcxy4sp8hl644x4ddl3msrtnurv3c4prz6cnag9hle8a5vyqkxgw54cl6tzyuap7j47yajf4wq3cl0tqdgup50rkdm9r4k3", 30_000_000),
    // タケヤマ #1 (2026-08-12)
    ("misakatest:q2utpunet56y6hxlm0pg39mx6sd6zertjqmrf2vrwhv9grr769pga6dsxhncyteexr6hvs8gcxyaumwxveth2qupe06l6maqpc5jhlp96s64ys7a", 100_000_000),
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
];

/// Total community allocation: 347M MSK (100+5+30+100+100+1+5+5+1).
pub const TESTNET11_COMMUNITY_SOMPI: u64 = 347_000_000 * SOMPI_PER_KASPA;

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
        utxos.push((outpoint, UtxoEntry { amount, script_public_key, block_daa_score: 0, is_coinbase: false }));
    }
    UtxoCollection::from_iter(utxos)
}

/// The FULL genesis UTXO set for one network id: the shared premine, plus — on testnet-11 and
/// only there — the community allocation. Keyed by [`NetworkId`] rather than [`NetworkType`]
/// because t10 and t11 share a type and must NOT share a UTXO set: t10 is a running chain whose
/// commitment cannot move.
/// **The fee float each PALW-RC genesis bond receives, and why a network needs one.**
///
/// A `ConsensusV2` producer earns NOTHING it can spend until one of its claims reaches `Final`:
/// the shipped split puts 62 % of the subsidy in the worker base and escrows exactly 62 %, so the
/// coinbase pays the producer `worker_base − escrow = 0` (measured: both are 27,562,844,868 sompi
/// on a 44,456,201,400 subsidy). The escrow is released by a `ReceiptLicensed` object, which rides
/// a 0x4b transaction, which needs a funded input. Mining income requires a finalized claim;
/// finalizing a claim requires mining income. **The loop is closed, and no amount of running the
/// chain opens it** — testnet-12's first launch produced 600 blocks and could not license one.
///
/// So the genesis opens it, because the genesis is the only place that can. Each registered bond's
/// PAYOUT address — an address the card already proves an operator holds the key for — receives a
/// small spendable float, carved OUT of the main premine rather than minted beside it, so the
/// supply is unchanged. 100 MSK covers roughly thirty thousand lifecycle submissions at the
/// production relay rate (~300k sompi each); the bonds need one working submitter, not an endowment.
///
/// Scoped to networks that actually run a `ConsensusV2` registry. Everything else — testnet-10's
/// running chain, testnet-11, devnet, simnet, mainnet — is byte-identical to before.
pub const PALW_RC_BOND_FEE_FLOAT_SOMPI: u64 = 100 * SOMPI_PER_KASPA;

/// The premine for `net`, plus whatever that network's genesis was minted to carry.
pub fn genesis_premine_utxos_for(net: NetworkId) -> UtxoCollection {
    let mut set = misaka_premine_utxos_for(net);
    if net.network_type == NetworkType::Testnet && net.suffix == Some(11) {
        // testnet-11 carries BOTH: the community allocation it was minted for, and — since the
        // PALW-RC network moved onto this suffix — the per-bond fee floats. The floats stayed
        // keyed to 12 through the move, so the RC genesis briefly funded nine community entries
        // and not one of its own bonds; a registry whose members cannot pay for a lifecycle
        // transaction is a registry that can license nothing.
        set.extend(testnet11_community_utxos());
        set.extend(palw_rc_bond_fee_floats(net));
    }
    set
}

/// The float outputs, one per shipped genesis bond, at indices after the main wallet — and the
/// main wallet reduced by exactly their total, so `MISAKA_PREMINE_SOMPI` still names the supply.
///
/// Empty when the card is unset, which is what keeps a bundle-free testnet-12 identical to the
/// network it was before any of this.
fn palw_rc_bond_fee_floats(net: NetworkId) -> UtxoCollection {
    let cards = crate::config::params::PALW_RC_GENESIS_BONDS;
    if cards.is_empty() {
        return UtxoCollection::default();
    }
    let txid = Hash64::from_bytes(MISAKA_PREMINE_TXID);
    let mut utxos: Vec<(TransactionOutpoint, UtxoEntry)> = Vec::with_capacity(cards.len() + 1);
    for (i, card) in cards.iter().enumerate() {
        let script_public_key = crate::dns_finality::p2pkh_mldsa87_spk(&card.payout_payload);
        // After the main wallet, so no vault index moves and no bond outpoint is disturbed.
        let outpoint = TransactionOutpoint { transaction_id: txid, index: (VAULT_COUNT + 1 + i) as u32 };
        utxos.push((
            outpoint,
            UtxoEntry { amount: PALW_RC_BOND_FEE_FLOAT_SOMPI, script_public_key, block_daa_score: 0, is_coinbase: false },
        ));
    }
    // The carve: the main wallet pays for every float, so the total premine is unchanged.
    let total_float = PALW_RC_BOND_FEE_FLOAT_SOMPI
        .checked_mul(cards.len() as u64)
        .expect("a genesis registry is six rows, not enough to overflow");
    // **The network being built, not a literal.** This named testnet-12 through the move to
    // testnet-11, and it happened to be harmless only because `main_address_for` answers with the
    // same address for both suffixes. That is a coincidence with an expiry date: testnet-12 is now
    // an identifier `Params::from` PANICS on, so the day suffix 12 leaves that match arm, this
    // line silently re-owns the 9B main premine — a re-carve of the whole supply, with no
    // compilation error and no test that names it. Deriving the script from `net` makes the carve
    // a fact about the network it is carving.
    let main = crate::dns_finality::p2pkh_mldsa87_spk(&owner_payload(main_address_for(net)));
    let main_outpoint = TransactionOutpoint { transaction_id: txid, index: VAULT_COUNT as u32 };
    utxos.push((
        main_outpoint,
        UtxoEntry {
            amount: MAIN_PREMINE_SOMPI.checked_sub(total_float).expect("the floats are a rounding error against 9B"),
            script_public_key: main,
            block_daa_score: 0,
            is_coinbase: false,
        },
    ));
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

    /// **A PALW network can fund its own first submitter** — the loop the launch found closed.
    ///
    /// The producer's coinbase pays it `worker_base − escrow`, and the shipped split makes those
    /// two equal: mining income needs a finalized claim, finalizing needs a funded 0x4b
    /// transaction, funding needs mining income. The genesis float is the only thing that opens
    /// it, so this asserts the three properties that make it work at all: the floats exist at the
    /// registry's own payout addresses, the supply does not move, and no other network is touched.
    #[test]
    fn the_rc_genesis_funds_every_bond_and_mints_nothing_extra() {
        let t11 = NetworkId::with_suffix(NetworkType::Testnet, 11);
        let set = genesis_premine_utxos_for(t11);
        let total: u64 = set.values().map(|e| e.amount).sum();
        let cards = crate::config::params::PALW_RC_GENESIS_BONDS;

        // **The RC's premine is the 13B split PLUS testnet-11's community allocation**, now that
        // the RC network is testnet-11. The bond fee floats are still carved out of the main
        // wallet rather than minted beside it — that is what this equality is for — and the 347M
        // is a separate, deliberate, network-keyed set that predates the move
        // (`TESTNET11_COMMUNITY_ALLOCATIONS`, nine entries burned into t11's genesis).
        assert_eq!(
            total,
            MISAKA_PREMINE_SOMPI + TESTNET11_COMMUNITY_SOMPI,
            "the floats are carved from the main wallet, never minted beside it — and the t11 community set is the only thing added"
        );

        // NOT `if cards.is_empty() { return }`: that made every assertion below vacuous the moment
        // the card was unset, which is exactly when a reader would most want to know. The shipped
        // RC has a card, so the test demands one — a build that drops it fails here rather than
        // passing silently.
        assert!(!cards.is_empty(), "the shipped RC card must be set for this network to fund anything");
        assert_eq!(
            set.len(),
            VAULT_COUNT + 1 + cards.len() + TESTNET11_COMMUNITY_ALLOCATIONS.len(),
            "40 vaults + the main wallet + one float per bond + t11's community entries"
        );
        // Every registered bond can pay a fee, at the address its own card names.
        for card in cards {
            let spk = crate::dns_finality::p2pkh_mldsa87_spk(&card.payout_payload);
            let funded: u64 = set.values().filter(|e| e.script_public_key == spk).map(|e| e.amount).sum();
            assert!(
                funded >= PALW_RC_BOND_FEE_FLOAT_SOMPI,
                "bond at premine #{} has no spendable float — it cannot submit a receipt quorum",
                card.premine_index
            );
        }
        // And the vault outputs the bonds are staked against are untouched, or the collateral the
        // genesis gate checked would no longer be there.
        for i in 0..VAULT_COUNT as u32 {
            let outpoint = TransactionOutpoint { transaction_id: Hash64::from_bytes(MISAKA_PREMINE_TXID), index: i };
            assert_eq!(set.get(&outpoint).expect("vault present").amount, VAULT_PREMINE_SOMPI, "vault {i} moved");
        }
        // No OTHER network gains a float. testnet-11 is the RC now, so it is the one that has
        // them; testnet-10 is a chain with history whose commitment must not move.
        let t10 = NetworkId::with_suffix(NetworkType::Testnet, 10);
        assert_eq!(genesis_premine_utxos_for(t10).len(), VAULT_COUNT + 1, "{t10} must not gain RC floats");
    }

    /// **The public PALW nets' 9B main wallet is the operator's address, and ONLY on those two.**
    ///
    /// The 2026-08-20 change moves the main wallet on testnet-11 and the PALW-RC net
    /// (testnet-12) to `PALW_PUBLIC_MAIN_ADDRESS`. Everything else is deliberately untouched:
    /// testnet-10 has a running chain whose commitment must not move, devnet and simnet keep the
    /// regenerable Claude-managed key their harnesses depend on, and mainnet's custody address is
    /// its own. The 40 vault UTXOs are unchanged everywhere — this is the main wallet only.
    #[test]
    fn the_public_palw_nets_hold_the_main_wallet_at_the_operator_address() {
        let public_spk = crate::dns_finality::p2pkh_mldsa87_spk(&owner_payload(PALW_PUBLIC_MAIN_ADDRESS));
        let claude_spk = crate::dns_finality::p2pkh_mldsa87_spk(&owner_payload(TESTNET_MAIN_ADDRESS));
        assert_ne!(public_spk, claude_spk, "the fixture must actually differ or this test proves nothing");

        let main_of = |net: NetworkId| {
            let txid = Hash64::from_bytes(MISAKA_PREMINE_TXID);
            let outpoint = TransactionOutpoint { transaction_id: txid, index: VAULT_COUNT as u32 };
            let set = genesis_premine_utxos_for(net);
            set.get(&outpoint).expect("the main UTXO sits at index VAULT_COUNT").clone()
        };

        // Only 11 now: the PALW-RC network moved onto it and 12 is refused at `From<NetworkId>`.
        for suffix in [11u32] {
            let entry = main_of(NetworkId::with_suffix(NetworkType::Testnet, suffix));
            assert_eq!(entry.script_public_key, public_spk, "testnet-{suffix} pays the operator address");
            // testnet-11 carves each genesis bond's fee float OUT of this output (see
            // `palw_rc_bond_fee_floats`) rather than minting beside it, so the main wallet is the
            // whole 9B minus exactly those floats — and the SUPPLY is what stays unchanged, which
            // `the_rc_genesis_funds_every_bond_and_mints_nothing_extra` asserts directly.
            let carved = if suffix == 11 {
                PALW_RC_BOND_FEE_FLOAT_SOMPI * crate::config::params::PALW_RC_GENESIS_BONDS.len() as u64
            } else {
                0
            };
            assert_eq!(entry.amount, MAIN_PREMINE_SOMPI - carved, "testnet-{suffix} main wallet is 9B less its carved floats");
        }
        // testnet-10 and the suffix-less testnet answer keep the Claude-managed wallet.
        assert_eq!(main_of(NetworkId::with_suffix(NetworkType::Testnet, 10)).script_public_key, claude_spk);
        for net in [NetworkType::Devnet, NetworkType::Simnet] {
            let txid = Hash64::from_bytes(MISAKA_PREMINE_TXID);
            let outpoint = TransactionOutpoint { transaction_id: txid, index: VAULT_COUNT as u32 };
            let set = misaka_premine_utxos(net);
            assert_eq!(set.get(&outpoint).unwrap().script_public_key, claude_spk, "{net:?} is untouched");
        }
        // Mainnet keeps its own custody address, which is neither of the above.
        let txid = Hash64::from_bytes(MISAKA_PREMINE_TXID);
        let outpoint = TransactionOutpoint { transaction_id: txid, index: VAULT_COUNT as u32 };
        let mainnet_main = misaka_premine_utxos(NetworkType::Mainnet).get(&outpoint).unwrap().script_public_key.clone();
        assert_ne!(mainnet_main, public_spk);
        assert_ne!(mainnet_main, claude_spk);

        // The vaults did not move on any network: same count, same amount, same scripts as the
        // mainnet set (their payloads are network-independent).
        for net in [NetworkType::Mainnet, NetworkType::Devnet, NetworkType::Simnet] {
            let set = misaka_premine_utxos(net);
            assert_eq!(set.len(), VAULT_COUNT + 1);
        }
        let t11 = genesis_premine_utxos_for(NetworkId::with_suffix(NetworkType::Testnet, 11));
        assert_eq!(
            t11.len(),
            VAULT_COUNT + 1 + TESTNET11_COMMUNITY_ALLOCATIONS.len() + crate::config::params::PALW_RC_GENESIS_BONDS.len(),
            "testnet-11 is the premine, the community list, and one fee float per RC bond"
        );
        for i in 0..VAULT_COUNT as u32 {
            let outpoint = TransactionOutpoint { transaction_id: txid, index: i };
            assert_eq!(t11.get(&outpoint).unwrap().amount, VAULT_PREMINE_SOMPI, "vault {i} is untouched");
        }
    }

    /// The community list is exactly what the operator collected: nine entrants, 347M total, and
    /// the two who changed address appear ONCE, at their new one.
    #[test]
    fn the_community_allocation_is_the_collected_list() {
        assert_eq!(TESTNET11_COMMUNITY_ALLOCATIONS.len(), 9);
        let total: u64 = TESTNET11_COMMUNITY_ALLOCATIONS.iter().map(|(_, msk)| *msk).sum();
        assert_eq!(total, 347_000_000, "100+5+30+100+100+1+5+5+1");
        assert_eq!(TESTNET11_COMMUNITY_SOMPI, total * SOMPI_PER_KASPA);

        // The superseded addresses are ABSENT — an entrant paid twice is an entrant paid wrong.
        for superseded in [
            "qfdqr02rxqyqh4jqtcn8qhwgsad3xqqn502tw26yajv7jg7eqap5slhggrcyngq8g789cxymezhc8mjfr3q2fj0w8j5w7mk986fta7u049hfph2n",
            "qfa2z97yspcra7pel80h06jg4a6mg0669fj5qx63e4v5y8geddd8hvyvy75rqaejgrq69e8yv4nd66rzlt5tqepw95q7q3k55qev84g6ey5yj8x8",
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
        // The PALW-RC net (testnet-12): premine with the public main wallet, no community set.
        let mut ms = MuHash::new();
        for (outpoint, entry) in genesis_premine_utxos_for(NetworkId::with_suffix(NetworkType::Testnet, 11)) {
            ms.add_utxo(&outpoint, &entry);
        }
        let commitment = ms.finalize();
        let rust = commitment.as_bytes().iter().map(|b| format!("0x{b:02x}")).collect::<Vec<_>>().join(", ");
        println!("PALW_RC_UTXO_COMMITMENT: Hash64::from_bytes([{rust}])");
        // testnet-11: premine ∪ community — the value TESTNET11_GENESIS.utxo_commitment pins.
        let mut ms = MuHash::new();
        for (outpoint, entry) in genesis_premine_utxos_for(NetworkId::with_suffix(NetworkType::Testnet, 11)) {
            ms.add_utxo(&outpoint, &entry);
        }
        let commitment = ms.finalize();
        let rust = commitment.as_bytes().iter().map(|b| format!("0x{b:02x}")).collect::<Vec<_>>().join(", ");
        println!("TESTNET11_UTXO_COMMITMENT: Hash64::from_bytes([{rust}])");
    }

    /// The community table is exactly the operator's collected list: 9 UTXOs, 347M MSK, every
    /// address a well-formed testnet-prefix single-key ML-DSA-87 P2PKH (the bech32 checksum in
    /// `owner_payload` is what turns any transcription slip into a build failure instead of a
    /// silently mis-locked allocation), every owner distinct — including distinct from all 41
    /// premine owners — and the whole set confined to testnet-11.
    #[test]
    fn t11_community_allocation_is_the_collected_list() {
        use kaspa_addresses::Prefix;

        let utxos = testnet11_community_utxos();
        assert_eq!(utxos.len(), 9, "nine entrants");
        let total: u64 = utxos.values().map(|e| e.amount).sum();
        assert_eq!(total, TESTNET11_COMMUNITY_SOMPI, "347M MSK exactly");
        assert_eq!(total, 347_000_000 * SOMPI_PER_KASPA);

        // Per-entry amounts, in table order (100/5/30/100/100/1/5/5/1 M).
        let expected_msk =
            [100_000_000u64, 5_000_000, 30_000_000, 100_000_000, 100_000_000, 1_000_000, 5_000_000, 5_000_000, 1_000_000];
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

        // Distinct owners, and distinct from every premine owner.
        let mut owners: Vec<[u8; 64]> = TESTNET11_COMMUNITY_ALLOCATIONS.iter().map(|(a, _)| owner_payload(a)).collect();
        for vault in VAULT_ADDRESSES {
            owners.push(owner_payload(vault));
        }
        owners.push(owner_payload(main_address(NetworkType::Testnet)));
        for i in 0..owners.len() {
            for j in (i + 1)..owners.len() {
                assert_ne!(owners[i], owners[j], "owner {i} and {j} collide");
            }
        }

        // Confinement: only testnet-11 carries the community set — and, since the PALW-RC network
        // moved onto that suffix, its per-bond fee floats as well.
        let t11 = genesis_premine_utxos_for(NetworkId::with_suffix(NetworkType::Testnet, 11));
        assert_eq!(
            t11.len(),
            VAULT_COUNT + 1 + 9 + crate::config::params::PALW_RC_GENESIS_BONDS.len(),
            "t11 = 41 premine + 9 community + one float per RC bond"
        );
        let t10 = genesis_premine_utxos_for(NetworkId::with_suffix(NetworkType::Testnet, 10));
        assert_eq!(t10.len(), VAULT_COUNT + 1, "t10 keeps the running chain's exact set");
        for net in [NetworkType::Mainnet, NetworkType::Devnet, NetworkType::Simnet] {
            assert_eq!(genesis_premine_utxos_for(NetworkId::new(net)).len(), VAULT_COUNT + 1, "{net:?} carries no community set");
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
        assert_eq!(
            derived,
            owner_payload(TESTNET_MAIN_ADDRESS),
            "TESTNET_MAIN_ADDRESS must match the key derived from TESTNET_MAIN_SEED"
        );
    }
}
