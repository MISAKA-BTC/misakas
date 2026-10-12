//! SDK/CLI component verification. Synthetic proof codecs are not checkpoint or court evidence.
use kaspa_consensus_core::{
    Hash64,
    palw_flat_model_seal_v6::*,
    palw_flat_model_vector_v3::*,
    palw_kernel_route_v1::*,
    palw_model_vector_v2::ModelVectorPostV2,
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2 as O},
    tx::TransactionOutpoint,
};
use kaspa_pq_validator_core::ValidatorKey;
use misaka_palw_kernel::{
    evidence::*,
    flat_court5::*,
    flat_seal6::*,
    route::KernelRouteObjectV1 as K,
    seg::{NodeOpeningV1, SEG_LEN_V4, SegmentedEvidenceV2},
};
use misaka_palw_sdk::flat_candidate_v6::*;
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};
fn h(n: u8) -> Hash64 {
    Hash64::from_bytes([n; 64])
}
fn bond(n: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(h(n), n as u32))
}
fn body() -> FlatModelVectorPostV3 {
    let mut p = FlatModelVectorPostV3 {
        version: 3,
        profile: flat_trace_profile_id_v5(),
        scope_root: [11; 64],
        trace: ModelVectorPostV2 {
            version: 2,
            prompt: vec![0],
            max_new_tokens: 1,
            generated: vec![1],
            evidence: SegmentedEvidenceV2 {
                version: 2,
                header: EvidenceHeaderV1 {
                    network_domain: [1; 64],
                    ruleset_digest: [2; 64],
                    class_binding_id: [3; 64],
                    program_root: [4; 64],
                    artifact_root: [5; 64],
                    plan_root: [6; 64],
                },
                job_input_root: [7; 64],
                positions: 1,
                segment_len: SEG_LEN_V4,
                claim_root: [8; 64],
                suite: SuiteParamsV1::of(&misaka_palw_kernel::descriptor::k2_tir_v4_descriptor()),
            },
            segment_roots: vec![[9; 64]],
            implementation_roots: [[10; 64]; 3],
        },
    };
    p.trace.implementation_roots = [p.trace_root(h(1).as_bytes(), h(2).as_bytes()); 3];
    p
}
fn proof() -> Vec<u8> {
    FlatFaultV5::Malformed(FlatMalformedFaultV5 {
        output: NodeOpeningV1 {
            position: 0,
            occurrence: 0,
            node: 0,
            commitment: [1; 64],
            node_siblings: vec![],
            position_root: [2; 64],
            position_siblings: vec![],
        },
        header: FlatHeaderV5::Flat { dtype: 3, shape: vec![1], root: [3; 64] },
    })
    .to_bytes()
    .unwrap()
}
fn secret() -> FlatProofSecretV6 {
    FlatProofSecretV6::new(h(1), h(2), bond(3), [4; 64], proof()).unwrap()
}
fn row(secret: &FlatProofSecretV6) -> FlatNativeSealRowV6 {
    FlatNativeSealRowV6 {
        bond: secret.bond,
        reserved: 10,
        seal: FlatSealRowV6 {
            version: 6,
            vector: secret.vector.as_bytes(),
            actor: palw_kernel_bond_id_v1(&secret.bond),
            seal: flat_proof_seal_v6(&secret.vector.as_bytes(), &palw_kernel_bond_id_v1(&secret.bond), &secret.salt, &secret.proof)
                .unwrap(),
            posted_daa: 10,
            mature_daa: 13,
            expires_daa: 110,
            spent: false,
        },
    }
}
fn verify(key: &ValidatorKey, domain: Hash64, object: &O) -> bool {
    match object {
        O::KernelRouteV1 { bytes, signer, signature } => key.verify_with_context(
            palw_kernel_route_message_v1(domain, signer, bytes).as_byte_slice(),
            signature,
            PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
        ),
        _ => false,
    }
}
fn mixed_candidate_program() -> (misaka_palw_tir::TirProgramV1, misaka_palw_kernel::trace::ParamCommitmentsV1) {
    use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder, program::HISTORY_BOUND_V1_HELD};
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_HELD);
    let scores = b.param("independent.scores", DType::I32, &[65], false);
    let mut pre = b.block("topk", vec![]);
    let y = pre.topk(scores, 0, 2);
    let y = pre.cast(y, DType::I32);
    let pre = pre.finish(&[y]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[2])]);
    let y = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(y);
    let post = post.finish(&[]);
    let p = b.finish(pre, vec![], post, 0);
    let values = MapParams { tensors: [((0, None), Tensor::new(DType::I32, vec![65], (0..65).collect()).unwrap())].into() };
    (p, misaka_palw_kernel::trace::ParamCommitmentsV1::of_v3(&values))
}
#[test]
fn mixed_candidate7_sdk_rederives_identity_and_whole_scope_before_signing() {
    use misaka_palw_sdk::flat_candidate_v7::*;
    let (p, pc) = mixed_candidate_program();
    let d = misaka_palw_kernel::descriptor::k2_tir_v4_descriptor().digest();
    let prepared = prepare_flat_candidate_v7(h(1).as_bytes(), d, &p.encode(), &pc, 128).unwrap();
    assert_eq!(prepared.plan.version, 7);
    assert_eq!(&prepared.object.encode()[..2], &[1, 40]);
    assert_eq!(K::decode(&prepared.object.encode()).unwrap(), prepared.object);
    assert_ne!(prepared.class, prepare_flat_candidate_v7(h(2).as_bytes(), d, &p.encode(), &pc, 128).unwrap().class);
    assert_ne!(prepared.class, prepare_flat_candidate_v7(h(1).as_bytes(), d, &p.encode(), &pc, 127).unwrap().class);
    assert_ne!(prepared.plan.scope_root, misaka_palw_kernel::flat_scope5::flat_resource_scope_v5(&p, 128, None).unwrap().root());
    let key = ValidatorKey::from_seed([7; 32]);
    let signed = sign_flat_candidate_v7(&key, h(9), bond(3), &prepared.object).unwrap();
    assert!(verify(&key, h(9), &signed));
    assert!(!verify(&key, h(8), &signed));
    if let O::KernelRouteV1 { bytes, signer, .. } = &signed {
        assert_eq!(*signer, bond(3));
        assert_eq!(K::decode(bytes).unwrap(), prepared.object);
    } else {
        panic!()
    }
    let mut forged = prepared.object.clone();
    let K::RegisterFlatModelCandidateV7 { replay_root, .. } = &mut forged else { unreachable!() };
    replay_root[0] ^= 1;
    assert!(sign_flat_candidate_v7(&key, h(9), bond(3), &forged).is_err());
    assert!(sign_flat_object_v6(&key, h(9), bond(3), &prepared.object).is_err());
    assert!(sign_flat_candidate_v7(&key, h(9), bond(3), &K::SealFlatModelProofV6 { vector: [1; 64], seal: [2; 64] }).is_err());
}
#[test]
fn mixed_candidate7_sdk_refuses_inexact_ranges_missing_params_and_bad_extent() {
    use misaka_palw_sdk::flat_candidate_v7::*;
    let (p, pc) = mixed_candidate_program();
    let d = misaka_palw_kernel::descriptor::k2_tir_v4_descriptor().digest();
    for max in [0, misaka_palw_tir::program::HISTORY_BOUND_V1_HELD + 1] {
        assert!(prepare_flat_candidate_v7([1; 64], d, &p.encode(), &pc, max).is_err());
    }
    assert!(prepare_flat_candidate_v7([1; 64], [0; 64], &p.encode(), &pc, 1).is_err());
    assert!(prepare_flat_candidate_v7([1; 64], d, &p.encode(), &Default::default(), 1).is_err());
    let mut bytes = p.encode();
    bytes.push(0);
    assert!(prepare_flat_candidate_v7([1; 64], d, &bytes, &pc, 1).is_err());
    let mut p = p;
    p.blocks[0].nodes[1].out.dtype = misaka_palw_tir::DType::I8;
    assert!(prepare_flat_candidate_v7([1; 64], d, &p.encode(), &pc, 1).is_err());
}
#[test]
fn protected_sdk_post_binds_distinct_identity_and_all_implementation_records() {
    let original = body();
    let old = original.id(h(1).as_bytes(), h(2).as_bytes());
    let (id, object) = prepare_flat_model_post_v6(h(1), h(2), original.clone()).unwrap();
    let K::PostFlatModelVectorV6 { class, binding, post } = object else { panic!() };
    let p = decode_flat_model_vector_post_v6(&post).unwrap();
    assert_ne!(old, id);
    assert_eq!(p.id(class, binding), id);
    assert_eq!(p.body.trace.implementation_roots, [p.trace_root(class, binding); 3]);
    assert_eq!(p.body.scope_root, original.scope_root);
    assert_eq!(p.body.trace.evidence, original.trace.evidence);
    assert!(prepare_flat_model_post_v6(h(4), h(2), original.clone()).is_err());
    assert!(prepare_flat_model_post_v6(h(1), h(4), original.clone()).is_err());
    let mut divergent = original.clone();
    divergent.trace.implementation_roots[1][0] ^= 1;
    assert!(prepare_flat_model_post_v6(h(1), h(2), divergent).is_err());
    let mut bad = original.clone();
    bad.version = 6;
    assert!(prepare_flat_model_post_v6(h(1), h(2), bad).is_err());
    let mut bad = original;
    bad.profile = [0; 64];
    assert!(prepare_flat_model_post_v6(h(1), h(2), bad).is_err());
    let mut oversized = body();
    oversized.trace.prompt = vec![0; FLAT_MODEL_VECTOR_MAX_POST_BYTES_V3 / 4 + 1];
    assert!(prepare_flat_model_post_v6(h(1), h(2), oversized).is_err());
}
#[test]
fn private_sdk_codec_bounds_counts_and_preserves_exact_carrier() {
    let s = secret();
    let bytes = s.to_bytes().unwrap();
    assert!(!format!("{s:?}").contains("salt"));
    assert_eq!(bytes.len(), 266 + s.proof.len());
    assert_eq!(FlatProofSecretV6::from_bytes(&bytes).unwrap(), s);
    for i in 0..bytes.len() {
        assert!(FlatProofSecretV6::from_bytes(&bytes[..i]).is_err());
    }
    let mut bad = bytes.clone();
    bad[262..266].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(FlatProofSecretV6::from_bytes(&bad).is_err());
    let mut bad = bytes.clone();
    bad[0] = 5;
    assert!(FlatProofSecretV6::from_bytes(&bad).is_err());
    let mut bad = bytes;
    bad.push(0);
    assert!(FlatProofSecretV6::from_bytes(&bad).is_err());
    assert!(FlatProofSecretV6::new(h(1), h(2), bond(3), [4; 64], vec![0; FLAT_SEAL_MAX_PROOF_BYTES_V6 + 1]).is_err());
    assert!(FlatProofSecretV6::new(h(1), h(2), bond(3), [4; 64], vec![5, 0]).is_err());
    assert!(s.seal_object(h(2), &s.bond).is_err());
    assert!(s.seal_object(s.network_domain, &bond(4)).is_err());
}
#[test]
fn sdk_reveal_requires_the_same_network_actor_exact_proof_and_mature_unspent_row() {
    let s = secret();
    let r = row(&s);
    assert!(matches!(s.reveal_object(s.network_domain,&s.bond,&r,13).unwrap(),K::RevealFlatModelProofV6{proof,..} if proof==s.proof));
    for daa in [0, 12, 110, u64::MAX] {
        assert!(s.reveal_object(s.network_domain, &s.bond, &r, daa).is_err());
    }
    assert!(s.reveal_object(h(2), &s.bond, &r, 13).is_err());
    assert!(s.reveal_object(s.network_domain, &bond(4), &r, 13).is_err());
    for change in 0..6 {
        let mut bad = r.clone();
        match change {
            0 => bad.bond = bond(4),
            1 => bad.seal.actor[0] ^= 1,
            2 => bad.seal.vector[0] ^= 1,
            3 => bad.seal.seal[0] ^= 1,
            4 => bad.seal.spent = true,
            _ => bad.seal.version = 5,
        }
        assert!(s.reveal_object(s.network_domain, &s.bond, &bad, 13).is_err());
    }
    let mut altered = s.clone();
    altered.salt[0] ^= 1;
    assert!(altered.reveal_object(s.network_domain, &s.bond, &r, 13).is_err());
    let mut alternate = FlatFaultV5::from_bytes(&s.proof).unwrap();
    if let FlatFaultV5::Malformed(f) = &mut alternate {
        f.header = FlatHeaderV5::Flat { dtype: 3, shape: vec![1], root: [9; 64] };
    }
    altered = s.clone();
    altered.proof = alternate.to_bytes().unwrap();
    assert!(altered.reveal_object(s.network_domain, &s.bond, &r, 13).is_err());
}

#[test]
fn protected_sdk_chunks_bind_own_mature_seal_and_both_signature_layers() {
    let s = secret();
    let key = ValidatorKey::from_seed([7; 32]);
    let r = row(&s);
    let signed =
        sign_flat_object_v6(&key, s.network_domain, s.bond, &s.reveal_object(s.network_domain, &s.bond, &r, 13).unwrap()).unwrap();
    assert!(flat_reveal_carriers_v6(&key, s.network_domain, s.bond, signed.clone(), &r, 12, 1024).is_err());
    assert!(flat_reveal_carriers_v6(&key, h(2), s.bond, signed.clone(), &r, 13, 1024).is_err());
    assert!(flat_reveal_carriers_v6(&key, s.network_domain, bond(4), signed.clone(), &r, 13, 1024).is_err());
    assert!(flat_reveal_carriers_v6(&key, s.network_domain, s.bond, signed.clone(), &r, 13, 0).is_err());
    let parts = flat_reveal_carriers_v6(&key, s.network_domain, s.bond, signed.clone(), &r, 13, 1024).unwrap();
    assert!(parts.len() > 1);
    let mut bytes = Vec::new();
    for part in parts {
        let O::KernelRouteChunkV1 { chunk, signature } = part else { panic!() };
        assert_eq!(chunk.opener, s.bond);
        assert_eq!(chunk.target, PalwKernelChunkTargetV1::FlatProofV6 { vector: s.vector, actor: palw_kernel_bond_id_v1(&s.bond) });
        assert!(key.verify_with_context(
            palw_kernel_chunk_message_v1(s.network_domain, &chunk).as_byte_slice(),
            &signature,
            PALW_KERNEL_CHUNK_MLDSA87_CONTEXT_V1
        ));
        let mut altered = (*chunk).clone();
        altered.bytes[0] ^= 1;
        assert!(!key.verify_with_context(
            palw_kernel_chunk_message_v1(s.network_domain, &altered).as_byte_slice(),
            &signature,
            PALW_KERNEL_CHUNK_MLDSA87_CONTEXT_V1
        ));
        bytes.extend_from_slice(&chunk.bytes);
    }
    assert_eq!(borsh::from_slice::<O>(&bytes).unwrap(), signed);
    assert!(verify(&key, s.network_domain, &signed));
}
#[test]
fn signatures_and_generic_carriage_preserve_network_bond_and_exact_protected_bytes() {
    let s = secret();
    let key = ValidatorKey::from_seed([7; 32]);
    let ops = [
        prepare_flat_model_post_v6(h(1), h(2), body()).unwrap().1,
        s.seal_object(s.network_domain, &s.bond).unwrap(),
        s.reveal_object(s.network_domain, &s.bond, &row(&s), 13).unwrap(),
        K::ExpireFlatModelSealV6 { vector: s.vector.as_bytes(), actor: palw_kernel_bond_id_v1(&s.bond) },
    ];
    for op in ops {
        let signed = sign_flat_object_v6(&key, s.network_domain, s.bond, &op).unwrap();
        assert!(verify(&key, s.network_domain, &signed));
        assert!(!verify(&key, h(2), &signed));
        let mut changed = signed.clone();
        if let O::KernelRouteV1 { signer, .. } = &mut changed {
            *signer = bond(4)
        };
        assert!(!verify(&key, s.network_domain, &changed));
        let mut changed = signed.clone();
        if let O::KernelRouteV1 { bytes, .. } = &mut changed {
            bytes[0] ^= 1
        };
        assert!(!verify(&key, s.network_domain, &changed));
        assert_eq!(flat_object_carriers_v6(signed.clone()).unwrap(), [signed]);
    }
    assert!(
        sign_flat_object_v6(
            &key,
            s.network_domain,
            s.bond,
            &K::RefuteFlatModelVectorV3 { vector: s.vector.as_bytes(), proof: s.proof }
        )
        .is_err()
    );
    assert!(
        sign_flat_object_v6(
            &key,
            h(1),
            bond(3),
            &K::RevealFlatModelProofV6 { vector: h(2).as_bytes(), salt: [4; 64], proof: vec![0; FLAT_SEAL_MAX_PROOF_BYTES_V6 + 1] }
        )
        .is_err()
    );
    // A transport-only hostile carrier. Reassembly is not proof grammar/court acceptance.
    let large = sign_flat_object_v6(
        &key,
        h(1),
        bond(3),
        &K::RevealFlatModelProofV6 { vector: h(2).as_bytes(), salt: [4; 64], proof: vec![0; 150000] },
    )
    .unwrap();
    let raw = borsh::to_vec(&large).unwrap();
    assert!(flat_object_carriers_v6(large.clone()).is_err());
    let parts = kaspa_consensus_core::palw_state_v2::palw_object_chunks_v1(&large).unwrap().unwrap();
    assert_eq!(parts.len(), 2);
    let mut assembled = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let O::ObjectChunk { group, index, count, bytes } = p else { panic!() };
        assert_eq!(*index, i as u8);
        assert_eq!(*count, 2);
        assert_eq!(*group, kaspa_consensus_core::palw_state_v2::palw_object_chunk_group_id_v1(&raw));
        assembled.extend(bytes);
    }
    assert_eq!(assembled, raw);
    let decoded: O = borsh::from_slice(&assembled).unwrap();
    assert_eq!(decoded, large);
    assert!(verify(&key, h(1), &decoded));
}

struct Temp {
    path: PathBuf,
}
impl Temp {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("flat-sdk-v6-{name}-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self { path }
    }
    fn p(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.p(name);
        std::fs::write(&path, bytes).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        path
    }
    fn run(&self, kind: &str, extra: &[(&str, String)]) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_palw-class"));
        c.args(["flat-v6", kind, "--network", "devnet", "--key-file"]).arg(self.p("key")).arg("--bond").arg(format!("{}:3", h(3)));
        for (flag, value) in extra {
            c.arg(flag).arg(value);
        }
        c.output().unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
fn path(p: &Path) -> String {
    p.to_str().unwrap().into()
}
fn object(p: &Path) -> O {
    borsh::from_slice(&std::fs::read(p).unwrap()).unwrap()
}
#[test]
fn cli_secret_permissions_no_overwrite_and_checked_reveal_roundtrip() {
    let t = Temp::new("reveal");
    t.write("key", "07".repeat(32).as_bytes());
    t.write("proof", &proof());
    let opts = [
        ("--vector", h(2).to_string()),
        ("--proof", path(&t.p("proof"))),
        ("--secret-out", path(&t.p("private"))),
        ("--out", path(&t.p("seal"))),
    ];
    let o = t.run("seal", &opts);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let report: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(report["operation"], "seal");
    assert!(report.get("salt").is_none());
    assert!(report.get("proof").is_none());
    let private = FlatProofSecretV6::from_bytes(&std::fs::read(t.p("private")).unwrap()).unwrap();
    assert_ne!(private.salt, [0; 64]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(t.p("private")).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let key = ValidatorKey::from_seed([7; 32]);
    assert!(verify(&key, private.network_domain, &object(&t.p("seal"))));
    let previous = std::fs::read(t.p("private")).unwrap();
    assert!(!t.run("seal", &opts).status.success());
    assert_eq!(std::fs::read(t.p("private")).unwrap(), previous);
    t.write("row", &borsh::to_vec(&row(&private)).unwrap());
    let mut opts = vec![
        ("--secret", path(&t.p("private"))),
        ("--row", path(&t.p("row"))),
        ("--height", "12".into()),
        ("--out", path(&t.p("reveal"))),
    ];
    assert!(!t.run("reveal", &opts).status.success());
    assert!(!t.p("reveal").exists());
    opts[2].1 = "13".into();
    let o = t.run("reveal", &opts);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let reveal = object(&t.p("reveal"));
    assert!(verify(&key, private.network_domain, &reveal));
    let O::KernelRouteV1 { bytes, .. } = reveal else { panic!() };
    assert_eq!(
        K::decode(&bytes).unwrap(),
        K::RevealFlatModelProofV6 { vector: private.vector.as_bytes(), salt: private.salt, proof: private.proof }
    );
    let other = [
        ("--vector", h(2).to_string()),
        ("--proof", path(&t.p("proof"))),
        ("--secret-out", path(&t.p("private"))),
        ("--out", path(&t.p("no-partial"))),
    ];
    assert!(!t.run("seal", &other).status.success());
    assert!(!t.p("no-partial").exists());
}
#[test]
fn cli_post_expiry_and_invalid_inputs_write_only_reviewable_carriers() {
    let t = Temp::new("post");
    t.write("key", "07".repeat(32).as_bytes());
    t.write("body", &borsh::to_vec(&body()).unwrap());
    let opts = [
        ("--class", h(1).to_string()),
        ("--binding", h(2).to_string()),
        ("--post", path(&t.p("body"))),
        ("--out", path(&t.p("post"))),
    ];
    let out = t.run("post", &opts);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let O::KernelRouteV1 { bytes, .. } = object(&t.p("post")) else { panic!() };
    let K::PostFlatModelVectorV6 { class, binding, post } = K::decode(&bytes).unwrap() else { panic!() };
    assert_eq!(report["vector"], decode_flat_model_vector_post_v6(&post).unwrap().id(class, binding).to_string());
    let out = t.run("expire", &[("--vector", h(2).to_string()), ("--actor", h(3).to_string()), ("--out", path(&t.p("expire")))]);
    assert!(out.status.success());
    let mut bad = opts.to_vec();
    bad.push(("--class", h(4).to_string()));
    assert!(!t.run("post", &bad).status.success());
    let mut bad = opts.to_vec();
    bad[3].1 = path(&t.p("unknown"));
    bad.push(("--unknown", "value".into()));
    assert!(!t.run("post", &bad).status.success());
    assert!(!t.p("unknown").exists());
    t.write("badproof", &vec![0; FLAT_SEAL_MAX_PROOF_BYTES_V6 + 1]);
    let out = t.run(
        "seal",
        &[
            ("--vector", h(2).to_string()),
            ("--proof", path(&t.p("badproof"))),
            ("--secret-out", path(&t.p("no-secret"))),
            ("--out", path(&t.p("no-seal"))),
        ],
    );
    assert!(!out.status.success());
    assert!(!t.p("no-secret").exists());
    assert!(!t.p("no-seal").exists());
}
