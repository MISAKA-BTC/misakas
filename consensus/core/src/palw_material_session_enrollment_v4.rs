//! Prospective native session enrollment before any execution or draw. The
//! proposal is canonical for an authenticated prior state and explicit roster;
//! no final carrying-header hash, execution output or provider signature is a
//! nonce in this proposal. A quote is not block acceptance or economic approval.
use crate::palw_native_resource_v1::{
    NativeRetainedV1, NativeWireReaderV1, NativeWireScanV1, preflight_native_value_v1, reserve_native_requests_v1,
    retain_native_value_decode_v1,
};
use crate::{
    BlockHash, Hash64,
    palw_attempt_material_v3::*,
    palw_material_capacity_v1::*,
    palw_material_release_recovery_v2::*,
    palw_monetary_material_v1::*,
    palw_state_v2::{PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwStateParamsV2, palw_bond_committed_v1},
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::{collections::BTreeSet, io::Read};
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwNativeRecoverySessionQuoteV4 {
    pub version: u16,
    pub checkpoint: BlockHash,
    pub daa: u64,
    pub network: Hash64,
    pub class: Hash64,
    pub executor: PalwBondKeyV2,
    pub executor_pubkey: Vec<u8>,
    pub active_session: Option<Hash64>,
    pub proposal: Option<PalwMaterialRecoveryActionV2>,
    pub provider_keys: Vec<(PalwBondKeyV2, Vec<u8>)>,
}
pub fn native_recovery_session_quote_v4(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    checkpoint: BlockHash,
    daa: u64,
    network: Hash64,
    class_id: Hash64,
    executor: PalwBondKeyV2,
    roster: &[PalwBondKeyV2],
) -> Result<NativeRetainedV1<PalwNativeRecoverySessionQuoteV4>, String> {
    let p = params.monetary_material_at_v1(daa).ok_or("native monetary gate dormant")?;
    let recovery = params.material_release_recovery_at_v2(daa).ok_or("native recovery gate dormant")?;
    p.validate().map_err(str::to_owned)?;
    recovery.validate().map_err(str::to_owned)?;
    if !(2..=8).contains(&roster.len()) || roster.windows(2).any(|p| p[0] >= p[1]) {
        return Err("recovery providers must be explicit, sorted and distinct 2..8 native bonds".into());
    }
    let executor_row = state
        .bond(&executor)
        .filter(|b| matches!(b.status, PalwBondStatusV2::Active))
        .ok_or("recovery executor is not an active native bond")?;
    let class = state.class(&class_id).ok_or("recovery class absent")?;
    if class_id == params.base_class_id() {
        return Err("BASE floor has no paid native recovery session".into());
    }
    let n = roster.len() as u64;
    let stream = std::io::Cursor::new((n as u32).to_le_bytes()).chain(std::io::repeat(0).take(n * 64));
    let mut reader = NativeWireReaderV1::new(stream, 4 + n * 64);
    let operator_heap = BTreeSet::<Hash64>::scan_native_wire_v1(&mut reader).map_err(|e| e.to_string())?.heap;
    let key_bytes = roster
        .iter()
        .try_fold(executor_row.pubkey.len() as u64, |sum, b| sum.checked_add(state.bond(b)?.pubkey.len() as u64))
        .ok_or("provider key bound missing/overflow")?;
    let fixed = std::mem::size_of::<PalwNativeRecoverySessionQuoteV4>() + std::mem::size_of::<PalwMonetaryMaterialEnrollmentV1>();
    let rows = std::mem::size_of::<(PalwBondKeyV2, Vec<u8>)>()
        + std::mem::size_of::<PalwMonetaryMaterialOfferV1>()
        + std::mem::size_of::<PalwMaterialRecoverySignatureV2>();
    let policy_wire = borsh::object_length(&(p, recovery)).map_err(|e| e.to_string())? as u64;
    let requests = n
        .checked_mul(rows as u64)
        .and_then(|v| v.checked_add(key_bytes))
        .and_then(|v| v.checked_add(fixed as u64))
        .and_then(|v| v.checked_add(operator_heap))
        .and_then(|v| v.checked_add(policy_wire))
        .and_then(|v| v.checked_add(64))
        .ok_or("native recovery quote ownership overflow")?;
    let lease = reserve_native_requests_v1(requests).map_err(|e| e.to_string())?;
    let mut operators = BTreeSet::new();
    let mut provider_keys = Vec::with_capacity(roster.len());
    for provider in roster {
        let b =
            state.bond(provider).filter(|b| matches!(b.status, PalwBondStatusV2::Active)).ok_or("recovery provider is not active")?;
        if !crate::palw_state_v2::palw_bond_may_judge_class_v2(b, &class_id) {
            return Err("recovery provider has no native class capability".into());
        }
        if b.operator_id == executor_row.operator_id || !operators.insert(b.operator_id) {
            return Err("recovery provider operators are dependent or self-custody".into());
        }
        provider_keys.push((*provider, b.pubkey.clone()));
    }
    let mut active: Option<(Hash64, bool)> = None;
    if let Some(route) = state.kernel_route() {
        for ((table, _), bytes) in &route.aux {
            if *table != PALW_MONETARY_MATERIAL_TABLE_V1 {
                continue;
            }
            preflight_native_value_v1::<PalwMonetaryMaterialRowV1>(bytes.as_slice(), bytes.len() as u64).map_err(|e| e.to_string())?;
            let row = retain_native_value_decode_v1(bytes, || borsh::from_slice::<PalwMonetaryMaterialRowV1>(bytes))
                .map_err(|e| e.to_string())?;
            let Some(s) = row.terms.native_session.as_ref() else { continue };
            if row.native_binding.is_some()
                || s.class_id != class_id
                || s.artifact_root != class.artifact_root
                || s.executor != executor
                || s.operator != executor_row.operator_id
                || s.version != 3
                || s.job_policy != native_job_policy_v3()
                || s.one_claim_cap != 1
                || s.admit_from_daa > daa
                || daa > s.admit_until_daa
                || !has_two_provider_operators_v4(&row, state)
            {
                continue;
            }
            let Some(bytes) = route.aux.get(&(PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, row.terms.root.as_byte_slice().to_vec()))
            else {
                continue;
            };
            preflight_native_value_v1::<PalwMaterialRecoveryRowV2>(bytes.as_slice(), bytes.len() as u64).map_err(|e| e.to_string())?;
            let recovery_row = retain_native_value_decode_v1(bytes, || borsh::from_slice::<PalwMaterialRecoveryRowV2>(bytes))
                .map_err(|e| e.to_string())?;
            let PalwMaterialRecoveryRowV2::Active(contract) = &*recovery_row else { continue };
            if recovery_row.validate(recovery).is_err() || contract.generations.last().is_none_or(|g| g.consent != row.consent) {
                continue;
            }
            if active.is_none_or(|(root, _)| row.terms.root < root) {
                active =
                    Some((row.terms.root, row.providers.iter().filter(|p| !p.charged).map(|p| p.provider).eq(roster.iter().copied())));
            }
        }
    }
    if let Some(active) = active {
        if !active.1 {
            return Err("canonical prior session uses another roster; explicit signed replacement is required".into());
        }
        return Ok(NativeRetainedV1::from_owner_reserved_v1(
            PalwNativeRecoverySessionQuoteV4 {
                version: 4,
                checkpoint,
                daa,
                network,
                class: class_id,
                executor,
                executor_pubkey: executor_row.pubkey.clone(),
                active_session: Some(active.0),
                proposal: None,
                provider_keys,
            },
            lease,
        ));
    }
    for provider in roster {
        let b = state.bond(provider).unwrap();
        if u128::from(b.collateral).saturating_sub(palw_bond_committed_v1(state, provider, daa, None, params.window_court()))
            < u128::from(p.minimum_collateral_sompi)
        {
            return Err("recovery provider has no free native collateral".into());
        }
    }
    let fees = p.minimum_fee_sompi.checked_mul(roster.len() as u64).ok_or("recovery fees overflow")?;
    if u128::from(executor_row.collateral).saturating_sub(palw_bond_committed_v1(state, &executor, daa, None, params.window_court()))
        < u128::from(fees)
    {
        return Err("recovery sponsor has no own unused funding collateral".into());
    }
    let until = daa.checked_add(p.max_enrollment_daa).ok_or("recovery enrollment clock overflows")?;
    let nonce = stream_hash_v4(
        b"MISAKA/PALW/NATIVE-RECOVERY-SESSION-NONCE/V4",
        &(network, checkpoint, daa, class_id, executor, roster, p.id(), recovery.id()),
    )?;
    let root = stream_hash_v4(b"MISAKA/PALW/NATIVE-RECOVERY-SESSION-ROOT/V4", &nonce)?;
    let monetary = PalwMonetaryMaterialEnrollmentV1 {
        terms: PalwMonetaryMaterialTermsV1 {
            version: 1,
            root,
            dependencies: vec![],
            exec_plan_root: None,
            sponsor: executor,
            nonce,
            policy: p.id(),
            max_material_bytes: p.max_bytes_per_contract,
            material_manifest: None,
            native_session: Some(PalwNativeMaterialSessionV3 {
                version: 3,
                class_id,
                artifact_root: class.artifact_root,
                executor,
                operator: executor_row.operator_id,
                job_policy: native_job_policy_v3(),
                codec: PalwMaterialCodecV1::NativeHeldComplete,
                max_units: PALW_MATERIAL_MAX_UNITS_V1 as u32,
                one_claim_cap: 1,
                admit_from_daa: daa,
                admit_until_daa: until,
            }),
            window_daa: p.window_daa,
            enrollment_until_daa: until,
            payment_relative_until_finality: true,
        },
        offers: roster
            .iter()
            .map(|provider| PalwMonetaryMaterialOfferV1 {
                provider: *provider,
                collateral_sompi: p.minimum_collateral_sompi,
                prepaid_fee_sompi: p.minimum_fee_sompi,
                signature: vec![],
            })
            .collect(),
    };
    let projection_bytes =
        n.checked_mul(std::mem::size_of::<(PalwBondKeyV2, u64, u64)>() as u64).ok_or("consent projection overflow")?;
    let payload_bytes = (borsh::object_length(&monetary.terms).map_err(|e| e.to_string())? as u64)
        .checked_add(4)
        .and_then(|v| n.checked_mul(68 + 8 + 8).and_then(|m| v.checked_add(m)))
        .ok_or("consent payload overflow")?;
    let _consent_workspace = reserve_native_requests_v1(
        payload_bytes
            .checked_mul(3)
            .and_then(|v| v.checked_add(projection_bytes))
            .and_then(|v| v.checked_add(4))
            .ok_or("consent scratch overflow")?,
    )
    .map_err(|e| e.to_string())?;
    let terms = PalwMaterialRecoveryTermsV2 {
        version: 2,
        root,
        initial_consent: monetary.id(),
        sponsor: executor,
        nonce,
        policy: recovery.id(),
        max_generations: recovery.max_generations_per_contract,
        all_generation_ack: true,
    };
    let proposal = PalwMaterialRecoveryActionV2::FundedEnroll {
        monetary: Box::new(monetary),
        terms,
        providers: roster.iter().map(|provider| PalwMaterialRecoverySignatureV2 { provider: *provider, signature: vec![] }).collect(),
        sponsor_signature: vec![],
    };
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        PalwNativeRecoverySessionQuoteV4 {
            version: 4,
            checkpoint,
            daa,
            network,
            class: class_id,
            executor,
            executor_pubkey: executor_row.pubkey.clone(),
            active_session: None,
            proposal: Some(proposal),
            provider_keys,
        },
        lease,
    ))
}

fn stream_hash_v4<T: BorshSerialize>(domain: &[u8], value: &T) -> Result<Hash64, String> {
    struct Sink(blake2b_simd::State);
    impl std::io::Write for Sink {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut sink = Sink(blake2b_simd::Params::new().hash_length(64).key(domain).to_state());
    let len = borsh::object_length(value).map_err(|e| e.to_string())? as u64;
    sink.0.update(&len.to_le_bytes());
    borsh::to_writer(&mut sink, value).map_err(|e| e.to_string())?;
    let mut hash = [0u8; 64];
    hash.copy_from_slice(sink.0.finalize().as_bytes());
    Ok(Hash64::from_bytes(hash))
}

fn has_two_provider_operators_v4(row: &PalwMonetaryMaterialRowV1, state: &PalwChainStateV2) -> bool {
    let mut first = None;
    for p in row.providers.iter().filter(|p| !p.charged) {
        if let Some(b) = state.bond(&p.provider) {
            match first {
                None => first = Some(b.operator_id),
                Some(a) if a != b.operator_id => return true,
                _ => {}
            }
        }
    }
    false
}
