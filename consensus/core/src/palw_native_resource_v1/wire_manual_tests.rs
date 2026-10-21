//! Source tests for manual codecs. These fixtures test byte shape only, not
//! class/job eligibility or independently reviewed resource/cost acceptance.
use super::*;
use crate::palw_freeprompt_v3::{PalwFpJobTailV1, PalwFreePromptJobV3};

#[test]
fn rfc23s2_optional_carriage_tail_presence_does_not_encode_an_extra_option_byte() {
    use crate::palw_state_v2::{PalwChainStateV2, PalwStateCarriageV2};
    fn check(c: &PalwStateCarriageV2) {
        let bytes = borsh::to_vec(c).unwrap();
        let shape = preflight_native_carriage_v1(bytes.as_slice(), bytes.len() as u64).unwrap();
        assert!(shape.memory_requests_v1().unwrap() >= c.native_heap_v1("tail fixture").bytes().unwrap());
        assert_eq!(borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap(), *c);
        assert!(preflight_native_carriage_v1(&bytes[..bytes.len() - 1], bytes.len() as u64 - 1).is_err());
    }
    let mut c = PalwStateCarriageV2::from_state(&PalwChainStateV2::genesis());
    c.floor_state = Some(crate::palw_real_share_v1::PalwFloorStateV1 {
        mode: crate::palw_real_share_v1::PalwFloorModeV1::Probe { until: 7 },
        last_probe_end: Some(3),
    });
    check(&c);
    c.bond_budget = Some(crate::palw_bond_budget_v1::PalwBondBudgetStateV1::new(
        1,
        &crate::palw_bond_budget_v1::PalwBondBudgetPolicyV1::unapproved_probe_v1(),
    ));
    check(&c);
    c.panel_v3 = Some(
        misaka_palw_panel::PermissionlessPanelStateV1::new(
            crate::Hash64::from_u64_word(1),
            crate::Hash64::from_u64_word(2),
            misaka_palw_panel::PanelPolicyV1 {
                seal_depth_blocks: 1,
                seal_wait_daa: 50,
                bond_maturity_daa: 1,
                beacon_period_daa: 10,
                beacon_wait_daa: 2,
                assignment_delay_daa: 1,
                receipt_window_daa: 3,
                seat_count: 2,
                outsider_seats: 0,
                max_retries: 1,
                min_collateral: 100,
                max_candidates: 16,
                max_pending: 8,
                max_pending_per_bond: 4,
                max_assignments_per_block: 8,
                max_admissions_per_block: 8,
                max_tracked_claims: 100,
                max_beacons_per_block: 2,
                max_beacon_proof_bytes: 100,
                beacon_scheme: crate::Hash64::from_u64_word(777),
            },
            crate::Hash64::from_u64_word(3),
            0,
            0,
        )
        .unwrap(),
    );
    check(&c);
}

#[test]
fn rfc23s2_checkpoint_floor_preflight_uses_the_exact_written_namespaces() {
    use crate::palw_security_certification_v1::{PalwCertifiedFrontierFloorV1, certification_frontier_floor_key_v1};
    fn accepts(key: &[u8], bytes: &[u8]) -> bool {
        let mut reader = NativeWireReaderV1::new(bytes, bytes.len() as u64);
        super::wire_kernel_aux::scan_native_aux_row_v1(136, key, &mut reader).is_ok() && reader.remaining() == 0
    }
    let bytes = borsh::to_vec(&PalwCertifiedFrontierFloorV1 { blue_score: 71, block: [8; 64] }).unwrap();
    let floor = certification_frontier_floor_key_v1();
    for key in [&floor[..], b"f"] {
        assert!(accepts(key, &bytes));
        assert!(!accepts(key, &bytes[..71]));
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(!accepts(key, &trailing));
    }
    let mut wrong = floor;
    wrong[1] = 1;
    assert!(!accepts(&wrong, &bytes));
    assert!(!accepts(&[b'f', 0], &bytes));
    assert!(!accepts(&[b'F'], &bytes));
}

#[test]
fn rfc23s2_native_mining_certificate_has_the_same_exact_preflight_as_chain_certificates() {
    use crate::palw_security_certification_v1::*;
    let row = PalwClaimCertificationRowV1 {
        claim: [1; 64],
        class_id: [2; 64],
        producer_bond: [3; 64],
        producer_principal: [4; 64],
        attempt: false,
        final_daa: 5,
        committed_daa: 6,
        committed_span: 7,
        anchor: None,
        draws: vec![],
        drawn_daa: 0,
        deadline_daa: 0,
        attestations: vec![],
        held_reward: 8,
        payout_payload: [9; 64],
        outcome: Some((PalwCertificationOutcomeV1::Certified, 10)),
    };
    let bytes = borsh::to_vec(&row).unwrap();
    let chain_key = certification_claim_key_v1(&[11; 64]);
    let native_key = certification_native_key_v39(&[11; 64]);
    let scan = |key: &[u8], bytes: &[u8]| {
        let mut reader = NativeWireReaderV1::new(bytes, bytes.len() as u64);
        let shape = super::wire_kernel_aux::scan_native_aux_row_v1(136, key, &mut reader)?;
        if reader.remaining() != 0 {
            return Err(std::io::Error::other("trailing certificate bytes"));
        }
        Ok(shape.heap)
    };
    assert_eq!(scan(&chain_key, &bytes).unwrap(), scan(&native_key, &bytes).unwrap());
    assert!(scan(&native_key[..64], &bytes).is_err());
    assert!(scan(&native_key, &bytes[..bytes.len() - 1]).is_err());
    let mut trailing = bytes;
    trailing.push(0);
    assert!(scan(&native_key, &trailing).is_err());
}

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
