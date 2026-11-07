//! kaspa-pq Phase 6: signature-verify cost benchmarks.
//!
//! Measures the median, p95, and p99 of:
//!
//!  - `secp256k1::schnorr::Signature::verify` — the upstream Kaspa baseline
//!    that the consensus `mass_per_sig_op = 1000` was originally tuned for.
//!  - `libcrux_ml_dsa::ml_dsa_87::verify` with `MLDSA87_TX_CONTEXT` — the
//!    kaspa-pq replacement.
//!
//! The ratio between the two medians, multiplied by a safety factor
//! >= 1.5, is the kaspa-pq `mass_per_sig_op` value. See
//! `docs/adr/0005-mass-policy.md` §"Calibration formula".
//!
//! Run with:
//!     cargo bench -p kaspa-txscript --bench bench

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use kaspa_txscript::MLDSA87_TX_CONTEXT;
use libcrux_ml_dsa::ml_dsa_87;
// kaspa-pq PQ-only: the legacy secp256k1 Schnorr baseline benchmark compiles only
// under `legacy-secp256k1` (ADR-0019 §14); the ML-DSA-87 benches are the default.
#[cfg(feature = "legacy-secp256k1")]
use secp256k1::{Message, Secp256k1};

/// Pre-build a deterministic ML-DSA-87 keypair + signature over a fixed
/// 32-byte message. The benchmark loop then calls `verify` repeatedly on
/// that same (vk, msg, sig) triple — exactly the verify-only cost.
///
/// `verify_default` exercises the runtime-multiplexed verify the script
/// engine actually calls (NEON / AVX2 / portable picked at runtime).
/// `verify_portable` explicitly exercises the portable variant, which is
/// what a no-SIMD low-end cloud reference platform would run. The ratio
/// between the two gives a conservative upper bound for the verify cost
/// that the mass-policy calibration must accommodate
/// (docs/adr/0005-mass-policy.md §"Phase 6 calibration result").
fn bench_mldsa87_verify(c: &mut Criterion) {
    let keypair = ml_dsa_87::generate_key_pair([0x11u8; 32]);
    let vk_bytes = *keypair.verification_key.as_ref();
    let vk = ml_dsa_87::MLDSA87VerificationKey::new(vk_bytes);

    let message = [0xa5u8; 32];
    let signature = ml_dsa_87::sign(&keypair.signing_key, &message, MLDSA87_TX_CONTEXT, [0x55u8; 32]).expect("ML-DSA sign");
    let sig_bytes = *signature.as_ref();
    let sig = ml_dsa_87::MLDSA87Signature::new(sig_bytes);

    c.bench_function("kaspa_pq::mldsa87_verify_default", |b| {
        b.iter(|| {
            let r = ml_dsa_87::verify(black_box(&vk), black_box(&message), black_box(MLDSA87_TX_CONTEXT), black_box(&sig));
            black_box(r.is_ok());
        });
    });

    c.bench_function("kaspa_pq::mldsa87_verify_portable", |b| {
        // libcrux explicit `portable` sub-module — no NEON / AVX2 — so the
        // measurement here is the "slowest reference platform" upper
        // bound on the platforms where libcrux ships SIMD acceleration.
        b.iter(|| {
            let r = ml_dsa_87::portable::verify(black_box(&vk), black_box(&message), black_box(MLDSA87_TX_CONTEXT), black_box(&sig));
            black_box(r.is_ok());
        });
    });
}

/// Schnorr (secp256k1) verify baseline. The script engine calls this via
/// `secp256k1::schnorr::Signature::verify` after parsing the 64-byte
/// signature and X-only public key.
fn bench_schnorr_verify(c: &mut Criterion) {
    // kaspa-pq PQ-only: secp256k1 is gated out of release builds, so the legacy
    // Schnorr baseline registers no benchmark unless `legacy-secp256k1` is on.
    #[cfg(not(feature = "legacy-secp256k1"))]
    let _ = c;
    #[cfg(feature = "legacy-secp256k1")]
    {
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
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .significance_level(0.05)
        .sample_size(50);
    targets = bench_mldsa87_verify, bench_schnorr_verify
}
criterion_main!(benches);
