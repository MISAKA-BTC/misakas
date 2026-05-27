//! kaspa-pq Phase 6: signature-verify cost benchmarks.
//!
//! Measures the median, p95, and p99 of:
//!
//!  - `secp256k1::schnorr::Signature::verify` — the upstream Kaspa baseline
//!    that the consensus `mass_per_sig_op = 1000` was originally tuned for.
//!  - `libcrux_ml_dsa::ml_dsa_65::verify` with `MLDSA65_TX_CONTEXT` — the
//!    kaspa-pq replacement.
//!
//! The ratio between the two medians, multiplied by a safety factor
//! >= 1.5, is the kaspa-pq `mass_per_sig_op` value. See
//! `docs/adr/0005-mass-policy.md` §"Calibration formula".
//!
//! Run with:
//!     cargo bench -p kaspa-txscript --bench bench

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use kaspa_txscript::MLDSA65_TX_CONTEXT;
use libcrux_ml_dsa::ml_dsa_65;
use secp256k1::{Message, Secp256k1};

/// Pre-build a deterministic ML-DSA-65 keypair + signature over a fixed
/// 32-byte message. The benchmark loop then calls `verify` repeatedly on
/// that same (vk, msg, sig) triple — exactly the verify-only cost.
fn bench_mldsa65_verify(c: &mut Criterion) {
    let keypair = ml_dsa_65::generate_key_pair([0x11u8; 32]);
    let vk_bytes = *keypair.verification_key.as_ref();
    let vk = ml_dsa_65::MLDSA65VerificationKey::new(vk_bytes);

    let message = [0xa5u8; 32];
    let signature =
        ml_dsa_65::sign(&keypair.signing_key, &message, MLDSA65_TX_CONTEXT, [0x55u8; 32]).expect("ML-DSA sign");
    let sig_bytes = *signature.as_ref();
    let sig = ml_dsa_65::MLDSA65Signature::new(sig_bytes);

    c.bench_function("kaspa_pq::mldsa65_verify", |b| {
        b.iter(|| {
            let r = ml_dsa_65::verify(black_box(&vk), black_box(&message), black_box(MLDSA65_TX_CONTEXT), black_box(&sig));
            black_box(r.is_ok());
        });
    });
}

/// Schnorr (secp256k1) verify baseline. The script engine calls this via
/// `secp256k1::schnorr::Signature::verify` after parsing the 64-byte
/// signature and X-only public key.
fn bench_schnorr_verify(c: &mut Criterion) {
    let secp = Secp256k1::new();
    let mut rng = secp256k1::rand::thread_rng();
    let (sk, _pk) = secp.generate_keypair(&mut rng);
    let kp = secp256k1::Keypair::from_secret_key(&secp, &sk);
    let xonly = secp256k1::XOnlyPublicKey::from_keypair(&kp).0;

    let msg_bytes = [0x5au8; 32];
    let msg = Message::from_digest_slice(&msg_bytes).unwrap();
    let sig = secp.sign_schnorr_no_aux_rand(&msg, &kp);

    c.bench_function("kaspa_pq::schnorr_verify_baseline", |b| {
        b.iter(|| {
            let r = sig.verify(black_box(&msg), black_box(&xonly));
            black_box(r.is_ok());
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .significance_level(0.05)
        .sample_size(50);
    targets = bench_mldsa65_verify, bench_schnorr_verify
}
criterion_main!(benches);
