//! Read-only component timing, not a capacity profile or an admission benchmark.
//! Keys and signatures are disposable fixtures; nothing is broadcast or funded.
//! Run release builds. Native UTXO, case/slot checks, indexes, undo, storage IO,
//! outer transaction carriage and Panel re-execution are outside this program.
use misaka_palw_positive_contract::{
    ContractError,
    identity::{Digest, PrincipalId, principal},
    public_one_honest::{Claimant, MANIFEST_REQUEST_SIGN_CONTEXT, ManifestRequest, OriginalId, PanelBond, SignedManifestRequestV1},
};
use std::{hint::black_box, time::Instant};

const SAMPLES: usize = 256;
const WARMUP: usize = 32;

fn authenticate(wire: &[u8], signer: PrincipalId) -> Result<(), ContractError> {
    let signed = SignedManifestRequestV1::decode(black_box(wire))?;
    signed.request.authenticate(black_box(signer), &signed.key, &signed.signature)
}

fn report(label: &str, wires: &[Vec<u8>], signers: &[PrincipalId], expected: Result<(), ContractError>) {
    assert_eq!(wires.len(), signers.len());
    assert!(!wires.is_empty());
    for _ in 0..WARMUP {
        for (wire, signer) in wires.iter().zip(signers) {
            assert_eq!(authenticate(wire, *signer), expected);
        }
    }
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let start = Instant::now();
        for (wire, signer) in wires.iter().zip(signers) {
            assert_eq!(black_box(authenticate(wire, *signer)), expected);
        }
        samples.push(u64::try_from(start.elapsed().as_nanos()).expect("bounded sample duration"));
    }
    let mut sorted = samples.clone();
    sorted.sort_unstable();
    let percentile = |p: usize| sorted[(p * sorted.len()).div_ceil(100).saturating_sub(1)];
    println!(
        "{}",
        serde_json::json!({
            "kind": "componentTimingOnly",
            "case": label,
            "requests_per_sample": wires.len(),
            "signed_request_bytes": wires.iter().map(Vec::len).collect::<Vec<_>>(),
            "samples_ns": samples,
            "min_ns": sorted[0], "p50_ns": percentile(50), "p95_ns": percentile(95),
            "p99_ns": percentile(99), "max_observed_ns": sorted[sorted.len()-1],
            "admission_established": false, "capacity_selected": false,
            "scope": "shared-contract decode, principal hash, signing digest and portable ML-DSA-87 authentication",
            "excluded": ["native original/case/slot checks", "UTXO and outer transaction", "indexes and undo", "archive IO", "Panel replay"],
            "measurement_limit": "observed warm-cache samples on this host; not a worst-case or slowest-supported-node guarantee"
        })
    );
}

fn main() {
    assert!(!cfg!(debug_assertions), "run this component timing in a release build");
    let mut valid = Vec::new();
    let mut forged = Vec::new();
    let mut signers = Vec::new();
    for rank in 0..5u8 {
        // Public, deterministic test seeds. These keys must never own funds.
        let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([61 + rank; 32]);
        let key = *pair.verification_key.as_ref();
        let signer = principal(&key);
        let request = ManifestRequest {
            original: OriginalId(Digest([1; 64])),
            manifest_root: Digest([2; 64]),
            claimant: Claimant::DrawnBond(PanelBond { transaction: Digest([3; 64]), index: u32::from(rank) }),
            signer,
            inspection_scope: Digest([4; 64]),
            supply_obligation: Digest([5; 64]),
            collect_until: 9_220,
            reservation: Digest([6; 64]),
        };
        let signature =
            libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &request.signing_digest().0, MANIFEST_REQUEST_SIGN_CONTEXT, [0; 32])
                .unwrap();
        let mut signed = SignedManifestRequestV1 { request, key, signature: *signature.as_ref() };
        let wire = borsh::to_vec(&signed).unwrap();
        assert_eq!(wire.len(), SignedManifestRequestV1::MAX_WIRE_BYTES);
        valid.push(wire);
        // Keep the public key and canonical signature intact, but change a
        // signed coordinate. This reaches cryptographic verification failure.
        signed.request.reservation = Digest([7; 64]);
        forged.push(borsh::to_vec(&signed).unwrap());
        signers.push(signer);
    }
    report("valid_one", &valid[..1], &signers[..1], Ok(()));
    report("valid_five_distinct_signers", &valid, &signers, Ok(()));
    report("wrong_digest_one", &forged[..1], &signers[..1], Err(ContractError::Signature));
    report("wrong_digest_five_distinct_signers", &forged, &signers, Err(ContractError::Signature));
}
