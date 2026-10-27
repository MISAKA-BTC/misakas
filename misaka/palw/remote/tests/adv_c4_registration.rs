//! **Agent C4 — attacking node-less model registration** (RFC-0009 stage A0: `model add --export-bundle`, `model sign`, `model submit`).
//!
//! The builder and the relay are hostile; the signer and the submitter see only JSON files. Every JSON leaf of the unsigned bundle and of
//! the signed registration is mutated, every byte of the carrier is flipped, and the multi-RPC quote agreement is perturbed field by
//! field. Expected results are stated before each run (and recorded in `docs/evidence/palw/implementation-records/adversarial-e2e-record.md`).

use std::collections::BTreeSet;

use kaspa_consensus_core::palw_base0_profile::rc_job_context;
use kaspa_consensus_core::palw_qwen25_profile::{QWEN25_1_5B_A16, QWEN25_A16_CANONICAL, qwen25_a16_profile_v1};
use kaspa_consensus_core::palw_state_v2::{
    PALW_CLASS_REGISTRATION_BURN_SOMPI_V1, PalwBondKeyV2, PalwClassAdmissionCarriageV2, PalwConsensusObjectV2, PalwPwuRuleV2,
};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;
use libcrux_ml_dsa::ml_dsa_87;
use misaka_palw_remote::bundle::*;
use misaka_palw_remote::register::{BondFactsV1, RegistrationFactsV1, quote_registration_v1};

fn h(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}

struct TestKey {
    kp: ml_dsa_87::MLDSA87KeyPair,
}

impl TestKey {
    fn new(seed: u8) -> Self {
        Self { kp: ml_dsa_87::generate_key_pair([seed; 32]) }
    }
}

impl DetachedSigner for TestKey {
    fn public_key(&self) -> Vec<u8> {
        let pk: &[u8] = self.kp.verification_key.as_ref();
        pk.to_vec()
    }
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
        ml_dsa_87::sign(&self.kp.signing_key, message, context, [9u8; 32])
            .map(|s| {
                let sig: &[u8] = s.as_ref();
                sig.to_vec()
            })
            .map_err(|e| format!("{e:?}"))
    }
}

const NET: &str = "testnet-12";

fn bond_outpoint() -> TransactionOutpoint {
    TransactionOutpoint::new(h(0x33), 1)
}

fn bond_str() -> String {
    format!("{}:{}", h(0x33), 1)
}

fn unsigned_object(root: Hash64) -> (PalwConsensusObjectV2, Hash64) {
    let profile = qwen25_a16_profile_v1(QWEN25_1_5B_A16).expect("the A16 geometry projects");
    let canonical = rc_job_context(&profile, QWEN25_A16_CANONICAL.0, QWEN25_A16_CANONICAL.1);
    let class_id = profile.shape_profile_id();
    (
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root: root,
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1_000 },
            initial_target: 1u128 << 100,
            share_permille: 10,
            activation_daa: 0,
            admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
                profile,
                canonical,
                registrant_bond: PalwBondKeyV2(bond_outpoint()),
                signature: Vec::new(),
            })),
        },
        class_id,
    )
}

struct Fx {
    owner: TestKey,
    payer: TestKey,
    bundle: RegistrationBundleV1,
    policy: SignerPolicyV1,
    submit: SubmitPolicyV1,
    class_id: Hash64,
}

fn facts_for(object: &PalwConsensusObjectV2, class_id: Hash64, root: Hash64, owner_pk: &[u8], fee: u64) -> RegistrationFactsV1 {
    RegistrationFactsV1 {
        network_domain: h(0x11),
        terms_digest: h(0x44),
        tip_hash: h(0x55),
        tip_daa: 1_000,
        class_id,
        artifact_root: root,
        object_digest: unsigned_object_digest_v1(object).unwrap(),
        exposure_sompi: 40_000,
        burn_sompi: PALW_CLASS_REGISTRATION_BURN_SOMPI_V1,
        bond: BondFactsV1 {
            outpoint: bond_str(),
            known: true,
            key_matches: !owner_pk.is_empty(),
            retiring: false,
            collateral_sompi: 100 * 100_000_000,
            backing_sompi: 10 * 100_000_000,
            live_locked_sompi: 0,
        },
        carrier_mass: 40_000,
        carrier_fee_sompi: fee,
        wallet_spendable_sompi: 5_000_000,
        filings: vec![],
    }
}

fn fixture() -> Fx {
    let (owner, payer) = (TestKey::new(1), TestKey::new(2));
    let root = h(0x77);
    let (object, class_id) = unsigned_object(root);
    let payer_spk = funding_spk_of_pubkey_v1(&payer.public_key());
    let quote = quote_registration_v1(facts_for(&object, class_id, root, &owner.public_key(), 200_000), 600, 300_000)
        .expect("the quote holds");
    let bundle = build_bundle_v1(BundleInputsV1 {
        network: NET.into(),
        network_domain: h(0x11),
        ruleset_id: "ruleset-fingerprint-a".into(),
        unsigned_object: object,
        owner_pubkey: owner.public_key(),
        owner_proof: None,
        payer_address: "kaspatest:payer".into(),
        payer_spk: payer_spk.clone(),
        funding_outpoint: TransactionOutpoint::new(h(0x66), 0),
        funding_entry: UtxoEntry::new(5_000_000, payer_spk, 900, false),
        quote,
        sources: vec![QuoteSourceV1 { node: "a".into(), tip_hash: h(0x55), tip_daa: 1_000 }],
    })
    .expect("the bundle builds");
    let expect = ExpectationsV1 { class_id, artifact_root: root, owner_bond: bond_str() };
    let policy = SignerPolicyV1 {
        network: NET.into(),
        network_domain: h(0x11),
        ruleset_id: "ruleset-fingerprint-a".into(),
        registration_exposure_sompi: 40_000,
        expect: expect.clone(),
        max_wallet_sompi: 300_000,
        now_daa: Some(1_001),
        pin: None,
    };
    let submit = SubmitPolicyV1 { network: NET.into(), network_domain: h(0x11), expect, max_fee_sompi: 300_000, now_daa: Some(1_001) };
    Fx { owner, payer, bundle, policy, submit, class_id }
}

fn sign_both(fx: &Fx, bundle: &RegistrationBundleV1) -> Result<SignedRegistrationV1, BundleRefusalV1> {
    let owner_signed = owner_sign_v1(bundle, &fx.policy, &fx.owner)?;
    carrier_sign_v1(&owner_signed, &fx.policy, &fx.payer)
}

/// Every leaf of a JSON document with a mutation of it: strings (hex nibble flipped / truncated / emptied), numbers (±1, 0, max),
/// booleans (negated). Returns `(path, mutated whole document)`.
fn leaf_mutants(v: &serde_json::Value) -> Vec<(String, serde_json::Value)> {
    fn walk(v: &serde_json::Value, path: String, root: &serde_json::Value, out: &mut Vec<(String, serde_json::Value)>) {
        use serde_json::Value as J;
        let mut put = |new: J, out: &mut Vec<(String, J)>| {
            let mut doc = root.clone();
            let mut cur = &mut doc;
            for seg in path.split('/').filter(|s| !s.is_empty()) {
                cur = match cur {
                    J::Object(m) => m.get_mut(seg).unwrap(),
                    J::Array(a) => a.get_mut(seg.parse::<usize>().unwrap()).unwrap(),
                    _ => unreachable!(),
                };
            }
            *cur = new;
            out.push((path.clone(), doc));
        };
        match v {
            J::Object(m) => {
                for (k, x) in m {
                    walk(x, format!("{path}/{k}"), root, out);
                }
            }
            J::Array(a) => {
                for (i, x) in a.iter().enumerate() {
                    walk(x, format!("{path}/{i}"), root, out);
                }
            }
            J::String(s) => {
                if !s.is_empty() {
                    let mut b = s.clone().into_bytes();
                    let mid = b.len() / 2;
                    b[mid] = if b[mid] == b'0' { b'1' } else { b'0' };
                    put(J::String(String::from_utf8_lossy(&b).into_owned()), out);
                    put(J::String(s[..s.len() - 1].to_string()), out);
                    put(J::String(String::new()), out);
                    let mut b = s.clone().into_bytes();
                    b[0] = if b[0] == b'f' { b'e' } else { b'f' };
                    put(J::String(String::from_utf8_lossy(&b).into_owned()), out);
                }
            }
            J::Number(n) => {
                if let Some(u) = n.as_u64() {
                    for x in [u.wrapping_add(1), u.saturating_sub(1), 0, u64::MAX] {
                        if x != u {
                            put(J::from(x), out);
                        }
                    }
                }
            }
            J::Bool(b) => put(J::Bool(!b), out),
            J::Null => {}
        }
    }
    let mut out = Vec::new();
    walk(v, String::new(), v, &mut out);
    out
}

/// EXPECTED (before running): a signed registration is the payer's signature over the whole carrier and the owner's over the object,
/// so a relay that edits ANY JSON field or ANY byte of the carrier either fails `verify_signed_registration_v1` or leaves the
/// verified carrier (tx id, object id, fee) unchanged. The fields NOT covered by any signature and used only by the client are
/// recorded (known gap G-EXPIRY: `expiry_daa`; the funding entry's maturity fields); any OTHER accepted edit is a defect.
#[test]
fn a_relay_that_edits_any_field_or_any_carrier_byte_of_a_signed_registration_changes_nothing_or_is_refused() {
    let fx = fixture();
    let signed = sign_both(&fx, &fx.bundle).expect("signs");
    let base = verify_signed_registration_v1(&signed, &fx.submit).expect("verifies");
    let doc: serde_json::Value = serde_json::from_str(&signed.to_json()).unwrap();
    let mut accepted_fields = BTreeSet::new();
    let mut tried = 0u32;
    for (path, m) in leaf_mutants(&doc) {
        tried += 1;
        let Ok(s) = serde_json::from_value::<SignedRegistrationV1>(m) else { continue };
        if s == signed {
            continue;
        }
        if let Ok(v) = verify_signed_registration_v1(&s, &fx.submit) {
            if (v.tx_id, v.object_id, v.fee_sompi) != (base.tx_id, base.object_id, base.fee_sompi) {
                panic!("an edit of {path} verified as a DIFFERENT carrier");
            }
            accepted_fields.insert(path);
        }
    }
    eprintln!("signed registration: {tried} field edits; accepted without changing the carrier: {accepted_fields:?}");
    // Every byte of the carrier itself, with the declared tx id fixed up so only the signatures can catch it.
    let tx = signed.transaction().unwrap();
    let bytes = borsh::to_vec(&tx).unwrap();
    let mut silent = Vec::new();
    for i in 0..bytes.len() {
        let mut b = bytes.clone();
        b[i] ^= 0x01;
        let Ok(t) = borsh::from_slice::<kaspa_consensus_core::tx::Transaction>(&b) else { continue };
        let mut m = signed.clone();
        m.tx_hex = b.iter().map(|x| format!("{x:02x}")).collect();
        m.tx_id = misaka_palw_remote::relay::tx_id_of_bytes(&t);
        if let Ok(v) = verify_signed_registration_v1(&m, &fx.submit)
            && v.tx_id != base.tx_id
        {
            silent.push(i);
        }
    }
    eprintln!("carrier bytes ({}): single-bit flips that still verify as a carrier: {silent:?}", bytes.len());
    // The allowed unsigned fields (documented): expiry and the funding entry's maturity fields.
    // `/ruleset_id` is the documented G-RULESET gap (not in the carrier); the displayed class / root / owner / object digest are
    // F-C4-12 below and are NOT allowed here.
    let allowed: BTreeSet<&str> = [
        "/expiry_daa",
        "/funding/block_daa_score",
        "/funding/is_coinbase",
        "/quote_digest",
        "/ruleset_id",
        "/class_id",
        "/artifact_root",
        "/owner_bond",
        "/object_digest",
    ]
    .into_iter()
    .collect();
    let unexpected: Vec<_> = accepted_fields.iter().filter(|f| !allowed.contains(f.as_str())).collect();
    eprintln!(
        "accepted display-only fields (see F-C4-12): {:?}",
        accepted_fields
            .iter()
            .filter(|f| ["/class_id", "/artifact_root", "/owner_bond", "/object_digest"].contains(&f.as_str()))
            .collect::<Vec<_>>()
    );
    assert!(
        unexpected.is_empty(),
        "edits accepted without changing the verified carrier, outside the documented unsigned set: {unexpected:?}"
    );
    assert!(silent.is_empty(), "carrier byte flips that verify as a different carrier: {silent:?}");
}

/// OBSERVATION O-C4-15 (descriptive, passes): the signer pins only the class, the root and the owner bond (what the USER expects).
/// Every JSON leaf of the unsigned bundle is mutated and each mutant is signed if the signer lets it; the signed OBJECT fields that
/// changed are listed. Fields a builder can change and still get a signature — `activation_daa`, `share_permille`,
/// `slash_value_per_pwu`, `initial_target`, `pwu_rule` — are policed only by consensus (a wrong value is a dropped carrier: the fee is
/// lost; a wrong `activation_daa` inside the lookahead is accepted and simply delays the class).
#[test]
fn observation_the_signer_signs_whatever_object_fields_the_builder_chose_beyond_class_root_and_owner() {
    let fx = fixture();
    let base = sign_both(&fx, &fx.bundle).expect("signs");
    let base_tx = base.tx_hex.clone();
    let doc: serde_json::Value = serde_json::from_str(&fx.bundle.to_json()).unwrap();
    let mut signed_other: BTreeSet<String> = BTreeSet::new();
    let mut signed_same = 0u32;
    let mut tried = 0u32;
    for (path, m) in leaf_mutants(&doc) {
        tried += 1;
        let Ok(b) = serde_json::from_value::<RegistrationBundleV1>(m) else { continue };
        if b == fx.bundle {
            continue;
        }
        match sign_both(&fx, &b) {
            Ok(s) if s.tx_hex == base_tx => signed_same += 1,
            Ok(_) => {
                signed_other.insert(path);
            }
            Err(_) => {}
        }
    }
    eprintln!(
        "unsigned bundle: {tried} edits; signed an IDENTICAL carrier: {signed_same}; signed a DIFFERENT carrier via edits of: {signed_other:?}"
    );
    // A targeted look at the signed object's fields: mutate the object itself (re-hex it, re-derive digest/message/quote so every
    // self-consistency check passes) and see which fields the signer will sign.
    let obj_hex = fx.bundle.object_hex.clone();
    let raw: Vec<u8> = (0..obj_hex.len() / 2).map(|i| u8::from_str_radix(&obj_hex[2 * i..2 * i + 2], 16).unwrap()).collect();
    let obj: PalwConsensusObjectV2 = borsh::from_slice(&raw).unwrap();
    let PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule,
        initial_target,
        share_permille,
        activation_daa,
        admission,
    } = obj.clone()
    else {
        unreachable!()
    };
    let variants: Vec<(&str, PalwConsensusObjectV2)> = vec![
        (
            "activation_daa",
            PalwConsensusObjectV2::ClassRegistered {
                class_id,
                artifact_root,
                slash_value_per_pwu,
                pwu_rule: pwu_rule.clone(),
                initial_target,
                share_permille,
                activation_daa: 3_999,
                admission: admission.clone(),
            },
        ),
        (
            "share_permille",
            PalwConsensusObjectV2::ClassRegistered {
                class_id,
                artifact_root,
                slash_value_per_pwu,
                pwu_rule: pwu_rule.clone(),
                initial_target,
                share_permille: 1000,
                activation_daa,
                admission: admission.clone(),
            },
        ),
        (
            "slash_value_per_pwu",
            PalwConsensusObjectV2::ClassRegistered {
                class_id,
                artifact_root,
                slash_value_per_pwu: 0,
                pwu_rule: pwu_rule.clone(),
                initial_target,
                share_permille,
                activation_daa,
                admission: admission.clone(),
            },
        ),
        (
            "initial_target",
            PalwConsensusObjectV2::ClassRegistered {
                class_id,
                artifact_root,
                slash_value_per_pwu,
                pwu_rule: pwu_rule.clone(),
                initial_target: 1,
                share_permille,
                activation_daa,
                admission: admission.clone(),
            },
        ),
        (
            "pwu_rule",
            PalwConsensusObjectV2::ClassRegistered {
                class_id,
                artifact_root,
                slash_value_per_pwu,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1),
                initial_target,
                share_permille,
                activation_daa,
                admission: admission.clone(),
            },
        ),
    ];
    let mut signable = Vec::new();
    for (what, o) in variants {
        let owner_pk = fx.owner.public_key();
        let payer_spk = funding_spk_of_pubkey_v1(&fx.payer.public_key());
        let quote = quote_registration_v1(facts_for(&o, fx.class_id, h(0x77), &owner_pk, 200_000), 600, 300_000).unwrap();
        let Ok(b) = build_bundle_v1(BundleInputsV1 {
            network: NET.into(),
            network_domain: h(0x11),
            ruleset_id: "ruleset-fingerprint-a".into(),
            unsigned_object: o,
            owner_pubkey: owner_pk,
            owner_proof: None,
            payer_address: "kaspatest:payer".into(),
            payer_spk: payer_spk.clone(),
            funding_outpoint: TransactionOutpoint::new(h(0x66), 0),
            funding_entry: UtxoEntry::new(5_000_000, payer_spk, 900, false),
            quote,
            sources: vec![QuoteSourceV1 { node: "a".into(), tip_hash: h(0x55), tip_daa: 1_000 }],
        }) else {
            continue;
        };
        if sign_both(&fx, &b).is_ok() {
            signable.push(what);
        }
    }
    eprintln!("O-C4-15: object fields the builder can change while the signer still signs: {signable:?}");
}

/// EXPECTED (before running): the file's displayed `class_id`, `artifact_root`, `owner_bond` and `object_digest` are claims about the
/// carrier; `verify_signed_registration_v1` must refuse a file whose displayed values are not the carrier's own (they feed the CLI's
/// registry verdict, the sent-journal path and the duplicate detection). FAILS today: verification reads the class, root and owner out of
/// the PAYLOAD and checks them against the user's expectations, and never compares them with the file's own fields, so a relay can edit
/// the file's `class_id` to an already-registered class (the CLI then reports "already registered — nothing is sent" and never sends the
/// user's registration), or file the journal under another class (so a later duplicate is not detected and the fee is paid twice).
#[test]
#[ignore = "FAIL F-C4-12: a relay can edit the signed file's displayed class / root / owner / object digest; verification ignores them and `model submit` acts on them"]
fn a_signed_registration_whose_displayed_class_root_or_owner_is_not_its_carriers_must_be_refused() {
    let fx = fixture();
    let signed = sign_both(&fx, &fx.bundle).expect("signs");
    let flip = |h: Hash64| {
        let mut b = h.as_bytes().to_vec();
        b[0] ^= 1;
        Hash64::from_slice(&b)
    };
    let mut accepted = Vec::new();
    let mut m = signed.clone();
    m.class_id = flip(m.class_id);
    if verify_signed_registration_v1(&m, &fx.submit).is_ok() {
        accepted.push("class_id");
    }
    let mut m = signed.clone();
    m.artifact_root = flip(m.artifact_root);
    if verify_signed_registration_v1(&m, &fx.submit).is_ok() {
        accepted.push("artifact_root");
    }
    let mut m = signed.clone();
    m.owner_bond = format!("{}:{}", h(0x34), 1);
    if verify_signed_registration_v1(&m, &fx.submit).is_ok() {
        accepted.push("owner_bond");
    }
    let mut m = signed.clone();
    m.object_digest = flip(m.object_digest);
    if verify_signed_registration_v1(&m, &fx.submit).is_ok() {
        accepted.push("object_digest");
    }
    assert!(accepted.is_empty(), "displayed fields a relay can edit without failing verification: {accepted:?}");
}

fn facts() -> RegistrationFactsV1 {
    let (object, class_id) = unsigned_object(h(0x77));
    facts_for(&object, class_id, h(0x77), &[1u8; 4], 200_000)
}

/// EXPECTED (before running): the multi-RPC agreement. Three honest nodes agree; ANY single-field difference of one node in an
/// identity field (network, terms, class, root, object digest, burn, exposure, carrier mass, bond identity or standing, filings) stops
/// everything with a named field; balances / fee / tip are merged to the WORST case (least collateral and spendable, most backing and
/// locks and fee, lowest tip); a tip spread past the bound, one node, or the same endpoint twice is refused; the result is labelled by
/// its sources. VERDICT: PASS. OBSERVATION O-C4-16 (descriptive): the fee is the MAX over the nodes, so one lying node among three
/// raises the quoted carrier fee to whatever it says (bounded only by the user's cap).
#[test]
fn quote_agreement_stops_on_any_identity_difference_and_merges_the_rest_to_the_worst_case() {
    let base = facts();
    let nodes = |b: RegistrationFactsV1| vec![("a".to_string(), base.clone()), ("b".to_string(), base.clone()), ("c".to_string(), b)];
    assert!(agree_quote_facts_v1(&nodes(base.clone()), 2, 12).is_ok());
    let diffs: Vec<(&str, Box<dyn Fn(&mut RegistrationFactsV1)>)> = vec![
        ("network", Box::new(|f| f.network_domain = h(0x12))),
        ("terms", Box::new(|f| f.terms_digest = h(0x45))),
        ("class", Box::new(|f| f.class_id = h(0x13))),
        ("root", Box::new(|f| f.artifact_root = h(0x14))),
        ("object digest", Box::new(|f| f.object_digest = h(0x15))),
        ("burn", Box::new(|f| f.burn_sompi += 1)),
        ("exposure", Box::new(|f| f.exposure_sompi += 1)),
        ("mass", Box::new(|f| f.carrier_mass += 1)),
        ("bond outpoint", Box::new(|f| f.bond.outpoint = format!("{}:2", h(0x33)))),
        ("bond known", Box::new(|f| f.bond.known = !f.bond.known)),
        ("bond key", Box::new(|f| f.bond.key_matches = !f.bond.key_matches)),
        ("bond retiring", Box::new(|f| f.bond.retiring = !f.bond.retiring)),
        (
            "filings",
            Box::new(|f| f.filings.push(misaka_palw_remote::register::FilingV1 { what: "x".into(), sompi: 1, from_wallet: true })),
        ),
    ];
    for (what, d) in diffs {
        let mut f = base.clone();
        d(&mut f);
        assert!(agree_quote_facts_v1(&nodes(f), 2, 12).is_err(), "a difference in {what} was averaged away");
    }
    // The merge is the worst case.
    let mut low = base.clone();
    low.bond.collateral_sompi -= 5;
    low.wallet_spendable_sompi -= 7;
    low.tip_daa = 995;
    low.tip_hash = h(0x56);
    low.bond.backing_sompi += 9;
    low.bond.live_locked_sompi += 3;
    low.carrier_fee_sompi += 11;
    let merged = agree_quote_facts_v1(&nodes(low.clone()), 2, 12).unwrap();
    assert_eq!(merged.facts.bond.collateral_sompi, base.bond.collateral_sompi - 5);
    assert_eq!(merged.facts.wallet_spendable_sompi, base.wallet_spendable_sompi - 7);
    assert_eq!(merged.facts.bond.backing_sompi, base.bond.backing_sompi + 9);
    assert_eq!(merged.facts.bond.live_locked_sompi, base.bond.live_locked_sompi + 3);
    assert_eq!((merged.facts.tip_daa, merged.facts.tip_hash), (995, h(0x56)), "the slowest clock");
    assert_eq!(merged.sources.len(), 3);
    // O-C4-16: one lying node raises the fee.
    let mut liar = base.clone();
    liar.carrier_fee_sompi = 299_999;
    assert_eq!(agree_quote_facts_v1(&nodes(liar), 2, 12).unwrap().facts.carrier_fee_sompi, 299_999);
    // Skew, a single node, the same endpoint twice.
    let mut far = base.clone();
    far.tip_daa += 13;
    assert!(agree_quote_facts_v1(&nodes(far), 2, 12).is_err());
    assert!(agree_quote_facts_v1(&[("a".into(), base.clone())], 1, 12).is_err());
    assert!(agree_quote_facts_v1(&[("a".into(), base.clone()), ("a".into(), base.clone())], 2, 12).is_err());
}

/// EXPECTED (before running): resend vs duplicate, against an independent oracle over every combination of registry verdict and local
/// journal: the registry's verdict always wins (a registered class is never re-filed; a conflict is never sent); otherwise the same tx
/// id on record is a resend, another tx for the same class AND root is a duplicate registration, a record for another class or root is
/// no record, and nothing on record is a first send. VERDICT: PASS.
#[test]
fn resend_versus_duplicate_matches_an_independent_oracle_over_every_combination() {
    use misaka_palw_remote::register::DuplicateVerdictV1 as V;
    let (class, root, tx) = (h(0x10), h(0x20), h(0x30));
    let rec = |c: Hash64, r: Hash64, t: Hash64| SentRecordV1 { class_id: c, artifact_root: r, tx_id: t, object_id: h(0x40) };
    let journals: Vec<Vec<SentRecordV1>> = vec![
        vec![],
        vec![rec(class, root, tx)],
        vec![rec(class, root, h(0x31))],
        vec![rec(h(0x11), root, h(0x31))],
        vec![rec(class, h(0x21), h(0x31))],
        vec![rec(class, root, h(0x31)), rec(class, root, tx)],
        vec![rec(class, root, h(0x31)), rec(class, root, h(0x32))],
        vec![rec(h(0x11), root, tx)],
    ];
    let verdicts = [V::New, V::Reuse { lifecycle: "Active".into(), registrant_bond: None }, V::Conflict("other".into())];
    for v in &verdicts {
        for j in &journals {
            let got = classify_submission_v1(tx, class, root, j, v);
            let same: Vec<&SentRecordV1> = j.iter().filter(|r| r.class_id == class && r.artifact_root == root).collect();
            let want = match v {
                V::Reuse { .. } => "already",
                V::Conflict(_) => "conflict",
                V::New if same.iter().any(|r| r.tx_id == tx) => "resend",
                V::New if !same.is_empty() => "duplicate",
                V::New => "first",
            };
            let name = match got {
                SubmissionKindV1::AlreadyRegistered { .. } => "already",
                SubmissionKindV1::Conflict(_) => "conflict",
                SubmissionKindV1::Resend => "resend",
                SubmissionKindV1::DuplicateRegistration { .. } => "duplicate",
                SubmissionKindV1::First => "first",
            };
            assert_eq!(name, want, "verdict {v:?} journal {j:?}");
        }
    }
}
