//! Source tests for manual codecs. These fixtures test byte shape only, not
//! class/job eligibility or independently reviewed resource/cost acceptance.
use super::*;
use crate::palw_freeprompt_v3::{PalwFpJobTailV1, PalwFreePromptJobV3};

fn job() -> PalwFreePromptJobV3 {
    PalwFreePromptJobV3 {
        version: crate::palw_freeprompt_v3::PALW_FP_V3_VERSION,
        network_domain: crate::Hash64::default(),
        class_id: crate::Hash64::default(),
        executor_bond: crate::tx::TransactionOutpoint { transaction_id: crate::Hash64::default(), index: 0 },
        executor_pubkey: vec![3; 17],
        operator_id: crate::Hash64::default(),
        anchor_block: crate::Hash64::default(),
        anchor_daa: 0,
        job_nonce: [0; 32],
        tokenizer_id: crate::Hash64::default(),
        prompt_token_ids_hash: crate::Hash64::default(),
        prompt_tokens: 1,
        decode_token_limit: 1,
        max_context_tokens: 4,
        privacy_mode: 0,
        prompt_mode: 0,
        sampling_seed: [0; 32],
        temperature_q: 0,
        decode: None,
        tail: None,
    }
}
fn checks<T: borsh::BorshSerialize + borsh::BorshDeserialize + NativeWireScanV1 + NativeHeapV1>(value: &T) {
    let bytes = borsh::to_vec(value).unwrap();
    let proof = preflight_native_value_v1::<T>(bytes.as_slice(), bytes.len() as u64).unwrap();
    let decoded: T = borsh::from_slice(&bytes).unwrap();
    assert_eq!(borsh::to_vec(&decoded).unwrap(), bytes);
    assert!(
        proof.logical_requests_v1() >= value.native_heap_v1("manual codec fixture").bytes().unwrap() + std::mem::size_of::<T>() as u64
    );
    assert!(preflight_native_value_v1::<T>(&bytes[..bytes.len() - 1], bytes.len() as u64 - 1).is_err());
    let mut extra = bytes;
    extra.push(0);
    assert!(preflight_native_value_v1::<T>(extra.as_slice(), extra.len() as u64).is_err());
}
#[test]
fn fp_manual_versions_have_exact_unframed_tails() {
    checks(&job());
    let v4 = job().into_v4(crate::palw_decode_pipeline_v4::DecodeConfigV4::NOOP);
    checks(&v4);
    let v5 = crate::palw_fp_job_v5::PalwFreePromptJobV5 { v4: v4.clone(), images: Vec::new(), source: None };
    checks(&v5);
    let mut carried = v4.clone();
    carried.version = crate::palw_fp_job_v5::PALW_FP_V5_VERSION;
    carried.tail = Some(PalwFpJobTailV1::V5(crate::palw_fp_job_v5::PalwFpV5TailV1 { images: Vec::new(), source: None }));
    assert_eq!(borsh::to_vec(&carried).unwrap(), borsh::to_vec(&v5).unwrap());
    checks(&carried);
    for version in [crate::palw_fp_prefix_v1::PALW_FP_PREFIX_VERSION, crate::palw_fp_prefix_v1::PALW_FP_PREFIX_INHERIT_VERSION] {
        let mut prefix = v4.clone();
        prefix.version = version;
        prefix.tail = Some(PalwFpJobTailV1::Prefix(crate::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(crate::Hash64::default())));
        checks(&prefix);
    }
}
#[test]
fn narrow_leaf_wire_expands_to_actual_i128_storage() {
    use misaka_palw_kernel::merkle3::LeafOpeningV3;
    for dtype in misaka_palw_tir::DType::ALL {
        let leaf = LeafOpeningV3 {
            dtype: dtype.tag(),
            shape: vec![2],
            axis: 0,
            line: 0,
            tile: 0,
            values: vec![0, 1],
            siblings: vec![[5; 64]],
            other_root: [3; 64],
        };
        checks(&leaf);
        checks(&crate::palw_onboarding_tile_v3::ArtifactTileOpeningV3::new(leaf).unwrap());
    }
}
