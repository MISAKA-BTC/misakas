//! **Agent C4 — attacking the V4 public redemption authorization (`RDA4`) and its split arithmetic** (RFC-0009 stage C), at the level
//! of the pure consensus-core functions every node and every relay shares. (The real-processor cases of the same RFC are lane D's
//! `rfc9_redemption_v4`; those are not re-run here.)

use kaspa_consensus_core::palw_receipt_v4::{
    PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT, PALW_RECEIPT_V4_BEACON_RULE_SLOT, PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS, PALW_RECEIPT_V4_VERSION,
    PalwReceiptV4Error, PalwRedemptionAuthBundleV4, PalwRedemptionAuthV4, palw_receipt_v4_split_v1, redeem_auth_id_v4,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use libcrux_ml_dsa::ml_dsa_87;
use misaka_palw_remote::transport::check_redemption_bundle_v1;

fn h(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}

fn verify(pk: &[u8], msg: &[u8], sig: &[u8], ctx: &[u8]) -> bool {
    matches!(kaspa_txscript::verify_mldsa87_with_context(pk, msg, sig, ctx), Ok(true))
}

fn auth() -> PalwRedemptionAuthV4 {
    PalwRedemptionAuthV4 {
        version: PALW_RECEIPT_V4_VERSION,
        network_domain: h(9),
        claim_id: h(0xAB),
        executor_bond: TransactionOutpoint::new(TransactionId::from_u64_word(0xB1), 0),
        quantum_lo: 0,
        quantum_hi: 8,
        beacon_rule: PALW_RECEIPT_V4_BEACON_RULE_SLOT,
        builder_fee_bps: 500,
        expiry_daa: 10_000,
    }
}

fn sign(a: &PalwRedemptionAuthV4) -> PalwRedemptionAuthBundleV4 {
    let kp = ml_dsa_87::generate_key_pair([4u8; 32]);
    let pk: &[u8] = kp.verification_key.as_ref();
    let msg = redeem_auth_id_v4(a);
    let sig = ml_dsa_87::sign(&kp.signing_key, msg.as_bytes().as_slice(), PALW_RECEIPT_V4_AUTH_MLDSA87_CONTEXT, [7u8; 32]).expect("signs");
    let sig: &[u8] = sig.as_ref();
    PalwRedemptionAuthBundleV4 { authorization: a.clone(), executor_pubkey: pk.to_vec(), signature: sig.to_vec() }
}

/// EXPECTED (before running): the stateless rules hold at their edges — fee at the cap passes, one basis point above is refused, `u16::MAX`
/// is refused; an empty or inverted quantum range, another network, another version, another beacon rule are refused; expiry `0` and
/// `u64::MAX` are accepted statelessly (expiry is a chain-time rule); and the signature covers every authorization field: editing ANY
/// field of a signed authorization (without re-signing) is `SignatureInvalid` or an earlier named refusal, never Ok. VERDICT: PASS.
#[test]
fn the_redemption_authorization_rules_hold_at_their_edges_and_the_signature_covers_every_field() {
    let ok = sign(&auth());
    ok.validate_v4(h(9), verify).expect("the honest bundle validates");
    let check = |mutate: &dyn Fn(&mut PalwRedemptionAuthV4)| -> Result<(), PalwReceiptV4Error> {
        let mut a = auth();
        mutate(&mut a);
        sign(&a).validate_v4(h(9), verify)
    };
    assert!(check(&|a| a.builder_fee_bps = PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS).is_ok());
    assert!(matches!(check(&|a| a.builder_fee_bps = PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS + 1), Err(PalwReceiptV4Error::FeeAboveCap { .. })));
    assert!(matches!(check(&|a| a.builder_fee_bps = u16::MAX), Err(PalwReceiptV4Error::FeeAboveCap { .. })));
    assert!(check(&|a| a.builder_fee_bps = 0).is_ok());
    assert!(matches!(check(&|a| a.quantum_hi = a.quantum_lo), Err(PalwReceiptV4Error::QuantumRangeEmpty { .. })));
    assert!(matches!(check(&|a| { a.quantum_lo = 5; a.quantum_hi = 2; }), Err(PalwReceiptV4Error::QuantumRangeEmpty { .. })));
    assert!(check(&|a| { a.quantum_lo = 0; a.quantum_hi = u32::MAX; }).is_ok());
    assert!(matches!(check(&|a| a.network_domain = h(10)), Err(PalwReceiptV4Error::NetworkDomainMismatch)));
    assert!(matches!(check(&|a| a.version = 0), Err(PalwReceiptV4Error::UnsupportedVersion { .. })));
    assert!(matches!(check(&|a| a.version = 2), Err(PalwReceiptV4Error::UnsupportedVersion { .. })));
    assert!(matches!(check(&|a| a.beacon_rule = 0), Err(PalwReceiptV4Error::BeaconRuleUnknown(0))));
    assert!(matches!(check(&|a| a.beacon_rule = 2), Err(PalwReceiptV4Error::BeaconRuleUnknown(2))));
    assert!(check(&|a| a.expiry_daa = 0).is_ok() && check(&|a| a.expiry_daa = u64::MAX).is_ok());
    // Every field edited AFTER signing: never valid.
    let edits: Vec<(&str, Box<dyn Fn(&mut PalwRedemptionAuthV4)>)> = vec![
        ("claim", Box::new(|a| a.claim_id = h(0xAC))),
        ("executor bond index", Box::new(|a| a.executor_bond = TransactionOutpoint::new(a.executor_bond.transaction_id, 1))),
        ("executor bond txid", Box::new(|a| a.executor_bond = TransactionOutpoint::new(TransactionId::from_u64_word(0xB2), 0))),
        ("quantum lo", Box::new(|a| a.quantum_lo = 1)),
        ("quantum hi", Box::new(|a| a.quantum_hi = 9)),
        ("fee", Box::new(|a| a.builder_fee_bps = 501)),
        ("expiry", Box::new(|a| a.expiry_daa = u64::MAX)),
    ];
    for (what, e) in edits {
        let mut b = ok.clone();
        e(&mut b.authorization);
        assert_eq!(b.validate_v4(h(9), verify), Err(PalwReceiptV4Error::SignatureInvalid), "{what} edited after signing");
    }
}

/// EXPECTED (before running): flipping any single byte of an encoded `RDA4` bundle, or truncating or extending it, never yields a
/// bundle that decodes AND validates; nothing panics. VERDICT: PASS.
#[test]
fn no_byte_flip_truncation_or_extension_of_an_rda4_bundle_validates() {
    let ok = sign(&auth());
    let bytes = ok.encode();
    let mut accepted = Vec::new();
    for i in 0..bytes.len() {
        for f in [0x01u8, 0x80] {
            let mut b = bytes.clone();
            b[i] ^= f;
            let r = std::panic::catch_unwind(|| PalwRedemptionAuthBundleV4::decode(&b).ok().map(|d| d.validate_v4(h(9), verify).is_ok()));
            match r {
                Ok(Some(true)) => accepted.push(i),
                Ok(_) => {}
                Err(_) => panic!("decode/validate panicked on a flip at byte {i}"),
            }
        }
    }
    for cut in [0, 1, 3, 4, 5, bytes.len() / 2, bytes.len() - 1] {
        assert!(PalwRedemptionAuthBundleV4::decode(&bytes[..cut]).is_err(), "a truncation to {cut} decoded");
    }
    let mut longer = bytes.clone();
    longer.push(0);
    assert!(PalwRedemptionAuthBundleV4::decode(&longer).is_err());
    assert!(accepted.is_empty(), "bundle bytes whose flip still validates: {accepted:?}");
    // The provider-side check also binds the bundle to the claim it is filed under and to the network.
    let claim_hex = auth().claim_id.to_string();
    assert!(check_redemption_bundle_v1(&bytes, &claim_hex, None).is_ok());
    assert!(check_redemption_bundle_v1(&bytes, &h(0xAC).to_string(), None).is_err(), "filed under another claim");
    assert!(check_redemption_bundle_v1(&bytes, &claim_hex, Some(h(10))).is_err(), "another network");
    assert!(check_redemption_bundle_v1(&vec![0u8; 20_000], &claim_hex, None).is_err(), "oversized");
}

/// EXPECTED (before running): the builder/miner split conserves value exactly for every subsidy part and every fee, never panics, never
/// gives the builder more than the stated share (fee <= subsidy*bps/10000), and clamps a hostile bps at 100 %. At or below the cap the
/// builder's fee never exceeds 10 % of the part. VERDICT: PASS.
#[test]
fn the_builder_miner_split_conserves_value_and_never_overpays_the_builder() {
    let parts = [0u64, 1, 9, 10, 99, 10_000, 12_345_678_901, u64::MAX / 2, u64::MAX - 1, u64::MAX];
    let bps = [0u16, 1, 500, 999, 1000, 1001, 5000, 9999, 10_000, 10_001, u16::MAX];
    for &p in &parts {
        for &b in &bps {
            let (miner, fee) = palw_receipt_v4_split_v1(p, b);
            assert_eq!(miner as u128 + fee as u128, p as u128, "value is conserved for {p} at {b} bps");
            assert!(fee as u128 * 10_000 <= p as u128 * (b.min(10_000) as u128), "the builder never gets more than its share ({p}, {b})");
            if b <= PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS {
                assert!(fee as u128 * 10_000 <= p as u128 * PALW_RECEIPT_V4_BUILDER_FEE_CAP_BPS as u128);
            }
        }
    }
}
