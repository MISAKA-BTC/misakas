//! Private authenticated retrieval of already retained material. This grants no
//! computation credit: signatures authenticate the parties/bytes, and local
//! replay checks data identity. Randomly selected verifier approval remains a
//! separate requirement under its explicit honest-participation assumptions.
use crate::{
    BlockHash, Hash64,
    palw_material_capacity_v1::PalwMaterialScopeV1,
    palw_native_resource_v1::{
        NativeRetainedV1, preflight_native_value_v1, reserve_native_requests_v1, retain_native_value_decode_v1,
    },
    palw_state_v2::{PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwStateParamsV2},
};
use borsh::{BorshDeserialize, BorshSerialize};
pub const PALW_KERNEL_MATERIAL_QUERY_VERSION_V4: u16 = 4;
pub const PALW_KERNEL_MATERIAL_QUERY_REQUEST_BYTES_V4: u64 = 267;
pub const PALW_KERNEL_MATERIAL_QUERY_FRAME_BYTES_V4: u64 =
    crate::palw_kernel_parameter_material_v4::PALW_KERNEL_MATERIAL_UNIT_BYTES_V4 + 65536;
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwKernelMaterialUnitQueryV4 {
    pub version: u16,
    pub root: Hash64,
    pub consent: Hash64,
    pub dependency: Hash64,
    pub provider: PalwBondKeyV2,
    pub stage: u8,
    pub position: u32,
}
impl PalwKernelMaterialUnitQueryV4 {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != PALW_KERNEL_MATERIAL_QUERY_VERSION_V4
            || [self.root, self.consent, self.dependency].contains(&Hash64::default())
        {
            return Err("kernel material query version/identity differs".into());
        }
        Ok(())
    }
}
#[derive(BorshSerialize, BorshDeserialize)]
pub enum PalwKernelMaterialUnitResponseV4 {
    Answer { query: PalwKernelMaterialUnitQueryV4, bytes: Vec<u8> },
    Refused { query: PalwKernelMaterialUnitQueryV4, reason: String },
}
/// Issued by own native state, not deserializable from a remote key assertion.
pub struct PalwKernelMaterialQueryAuthorityV4 {
    query: PalwKernelMaterialUnitQueryV4,
    point: BlockHash,
    network: Hash64,
    daa_score: u64,
    requester: PalwBondKeyV2,
    requester_pubkey: Vec<u8>,
    provider_pubkey: Vec<u8>,
    scope: PalwMaterialScopeV1,
}
impl PalwKernelMaterialQueryAuthorityV4 {
    pub fn query(&self) -> &PalwKernelMaterialUnitQueryV4 {
        &self.query
    }
    pub fn checkpoint(&self) -> BlockHash {
        self.point
    }
    pub fn network(&self) -> Hash64 {
        self.network
    }
    pub fn daa_score(&self) -> u64 {
        self.daa_score
    }
    pub fn requester(&self) -> PalwBondKeyV2 {
        self.requester
    }
    pub fn requester_pubkey(&self) -> &[u8] {
        &self.requester_pubkey
    }
    pub fn provider_pubkey(&self) -> &[u8] {
        &self.provider_pubkey
    }
    pub fn max_response_frame_bytes(&self) -> u64 {
        self.scope.bytes.min(crate::palw_kernel_parameter_material_v4::PALW_KERNEL_MATERIAL_UNIT_BYTES_V4) + 65536
    }
    pub fn scope(&self) -> &PalwMaterialScopeV1 {
        &self.scope
    }
}
pub fn kernel_material_query_authority_v4(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    point: BlockHash,
    now: u64,
    query: &PalwKernelMaterialUnitQueryV4,
    requester: PalwBondKeyV2,
    requester_pubkey: &[u8],
) -> Result<NativeRetainedV1<PalwKernelMaterialQueryAuthorityV4>, String> {
    use crate::palw_monetary_material_v1::{PALW_MONETARY_MATERIAL_TABLE_V1, PalwMonetaryMaterialRowV1};
    query.validate()?;
    let sender = state.bond(&requester).ok_or("material query requester native bond absent")?;
    if sender.status != PalwBondStatusV2::Active || sender.pubkey.as_slice() != requester_pubkey {
        return Err("material query requester is not its own Active native key".into());
    }
    let provider = state.bond(&query.provider).ok_or("material query provider native key absent")?;
    let route = state.kernel_route().ok_or("material query native route absent")?;
    let raw = route
        .aux
        .iter()
        .find(|((t, k), _)| *t == PALW_MONETARY_MATERIAL_TABLE_V1 && k.as_slice() == query.root.as_byte_slice())
        .map(|(_, b)| b.as_slice())
        .ok_or("material query current covenant absent")?;
    preflight_native_value_v1::<PalwMonetaryMaterialRowV1>(raw, raw.len() as u64).map_err(|e| e.to_string())?;
    let row = retain_native_value_decode_v1(raw, || borsh::from_slice::<PalwMonetaryMaterialRowV1>(raw)).map_err(|e| e.to_string())?;
    let scope = row
        .terms
        .material_manifest
        .as_ref()
        .and_then(|m| m.scope(query.dependency))
        .ok_or("material query exact complete kernel scope absent")?;
    if scope.codec != crate::palw_material_capacity_v1::PalwMaterialCodecV1::TypedKernelV4 {
        return Err("material query does not reinterpret legacy codec bytes".into());
    }
    crate::palw_kernel_material_enrollment_v4::require_material_generation_current_v4(
        state,
        params,
        now,
        query.root,
        query.consent,
        query.provider,
        scope,
    )?;
    // A provider may continue its own generation while other providers change,
    // but a newly funded re-enrollment supersedes its earlier read/challenge ID.
    if material_provider_generation_v4(state, query.root, query.provider).map_err(|e| e.to_string())? != Some(query.consent) {
        return Err("query does not name provider's latest accepted generation".into());
    }
    let count = (std::mem::size_of::<PalwKernelMaterialQueryAuthorityV4>() as u64)
        .checked_add(sender.pubkey.len() as u64)
        .and_then(|v| v.checked_add(provider.pubkey.len() as u64))
        .ok_or("query authority key size overflow")?;
    let lease = reserve_native_requests_v1(count).map_err(|e| e.to_string())?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        PalwKernelMaterialQueryAuthorityV4 {
            query: *query,
            point,
            network: Hash64::from_bytes(route.header.policy.network_domain),
            daa_score: now,
            requester,
            requester_pubkey: sender.pubkey.clone(),
            provider_pubkey: provider.pubkey.clone(),
            scope: scope.clone(),
        },
        lease,
    ))
}

/// The authenticated transport caller supplies the expected query. This only
/// transfers the already admitted byte owner; it grants no computation credit.
pub fn take_kernel_material_query_bytes_v4(
    response: NativeRetainedV1<PalwKernelMaterialUnitResponseV4>,
    expected: &PalwKernelMaterialUnitQueryV4,
) -> Result<NativeRetainedV1<Vec<u8>>, String> {
    let (value, lease) = response.into_parts_v1();
    match value {
        PalwKernelMaterialUnitResponseV4::Answer { query, bytes }
            if query == *expected
                && bytes.len() as u64 <= crate::palw_kernel_parameter_material_v4::PALW_KERNEL_MATERIAL_UNIT_BYTES_V4 =>
        {
            Ok(NativeRetainedV1::from_owner_reserved_v1(bytes, lease))
        }
        PalwKernelMaterialUnitResponseV4::Refused { query, reason } if query == *expected && reason.len() <= 1024 => Err(reason),
        _ => Err("material query response substitutes request or exceeds its byte bound".into()),
    }
}

/// Move an existing admitted action's exact unit bytes into a transport answer.
/// The original byte reservation remains live through serialization/sending.
pub fn kernel_material_query_response_v4(
    action: NativeRetainedV1<crate::palw_monetary_material_v1::PalwMonetaryMaterialActionV1>,
    query: &PalwKernelMaterialUnitQueryV4,
) -> Result<NativeRetainedV1<PalwKernelMaterialUnitResponseV4>, String> {
    let (action, lease) = action.into_parts_v1();
    match action {
        crate::palw_monetary_material_v1::PalwMonetaryMaterialActionV1::Answer {
            root,
            provider,
            dependency,
            stage,
            position,
            bytes,
        } if root == query.root
            && provider == query.provider
            && dependency == query.dependency
            && stage == query.stage
            && position == query.position
            && bytes.len() as u64 <= crate::palw_kernel_parameter_material_v4::PALW_KERNEL_MATERIAL_UNIT_BYTES_V4 =>
        {
            Ok(NativeRetainedV1::from_owner_reserved_v1(PalwKernelMaterialUnitResponseV4::Answer { query: *query, bytes }, lease))
        }
        _ => Err("kernel query answer substitutes requested identity or unit".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{palw_monetary_material_v1::PalwMonetaryMaterialActionV1 as Action, tx::TransactionOutpoint};
    fn query() -> PalwKernelMaterialUnitQueryV4 {
        PalwKernelMaterialUnitQueryV4 {
            version: 4,
            root: Hash64::from_bytes([1; 64]),
            consent: Hash64::from_bytes([2; 64]),
            dependency: Hash64::from_bytes([3; 64]),
            provider: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([4; 64]), 5)),
            stage: 255,
            position: 6,
        }
    }
    #[test]
    fn query_is_fixed_canonical_wire_and_keeps_owned_bytes() {
        let q = query();
        let encoded = borsh::to_vec(&q).unwrap();
        assert_eq!(encoded.len() as u64, PALW_KERNEL_MATERIAL_QUERY_REQUEST_BYTES_V4);
        assert_eq!(borsh::from_slice::<PalwKernelMaterialUnitQueryV4>(&encoded).unwrap(), q);
        let bytes = vec![7, 8, 9];
        let address = bytes.as_ptr();
        let action = Action::Answer {
            root: q.root,
            provider: q.provider,
            dependency: q.dependency,
            stage: q.stage,
            position: q.position,
            bytes,
        };
        let response = kernel_material_query_response_v4(NativeRetainedV1::from_owner_reserved_v1(action, None), &q).unwrap();
        let bytes = take_kernel_material_query_bytes_v4(response, &q).unwrap();
        assert_eq!(bytes.as_ptr(), address);
        assert_eq!(bytes.as_slice(), [7, 8, 9]);
    }
    #[test]
    fn query_response_preflight_rejects_nested_length_bombs_before_decode() {
        let q = query();
        let mut malformed = vec![0u8];
        malformed.extend(borsh::to_vec(&q).unwrap());
        malformed.extend(u32::MAX.to_le_bytes());
        assert!(preflight_native_value_v1::<PalwKernelMaterialUnitResponseV4>(&malformed, malformed.len() as u64).is_err());
        let mut trailing = borsh::to_vec(&q).unwrap();
        trailing.push(0);
        assert!(preflight_native_value_v1::<PalwKernelMaterialUnitQueryV4>(&trailing, trailing.len() as u64).is_err());
    }
    #[test]
    fn query_does_not_accept_substituted_consent_or_unit_echo() {
        let q = query();
        let mut changed = q;
        changed.consent = Hash64::from_bytes([99; 64]);
        let response =
            NativeRetainedV1::from_owner_reserved_v1(PalwKernelMaterialUnitResponseV4::Answer { query: changed, bytes: vec![] }, None);
        assert!(take_kernel_material_query_bytes_v4(response, &q).is_err());
        let action = Action::Answer {
            root: q.root,
            provider: q.provider,
            dependency: q.dependency,
            stage: q.stage,
            position: q.position + 1,
            bytes: vec![],
        };
        assert!(kernel_material_query_response_v4(NativeRetainedV1::from_owner_reserved_v1(action, None), &q).is_err());
        let mut invalid = q;
        invalid.root = Hash64::default();
        assert!(invalid.validate().is_err());
        invalid = q;
        invalid.version = 3;
        assert!(invalid.validate().is_err());
    }
    #[test]
    fn provider_generation_follows_its_own_replacements_cold_route_and_rollback() {
        use crate::{
            palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_route_policy_v1},
            palw_material_release_recovery_v2::{
                PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2 as TABLE, PalwMaterialRecoveryContractV2 as Contract,
                PalwMaterialRecoveryGenerationV2 as Generation, PalwMaterialRecoveryRowV2 as Row,
                PalwMaterialRecoveryTermsV2 as Terms, PalwMaterialTerminalReceiptV2 as Terminal,
            },
            palw_native_resource_v1::NativeDecodeErrorV1,
        };
        // Projection-only fixtures, not an admission test or approved policy.
        let q = query();
        let bond = |n| PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([n; 64]), 0));
        let (a, b, c) = (bond(4), bond(5), bond(6));
        let first = Hash64::from_bytes([7; 64]);
        let next = Hash64::from_bytes([8; 64]);
        let mut contract = Contract {
            terms: Terms {
                version: 2,
                root: q.root,
                initial_consent: first,
                sponsor: bond(9),
                nonce: Hash64::from_bytes([10; 64]),
                policy: Hash64::from_bytes([11; 64]),
                max_generations: 4,
                all_generation_ack: true,
            },
            enrolled_daa: 1,
            generations: vec![Generation { consent: first, providers: vec![a, b] }],
        };
        let policy = palw_kernel_route_policy_v1(Hash64::from_bytes([12; 64]), Hash64::from_bytes([13; 64]));
        let mut route = PalwKernelRouteStateV1::new(policy, None, Default::default());
        let key = (TABLE, q.root.as_byte_slice().to_vec());
        let initial = borsh::to_vec(&Row::Active(contract.clone())).unwrap();
        route.aux.insert(key.clone(), initial.clone());
        assert_eq!(material_provider_generation_from_route_v4(&route, q.root, a).unwrap(), Some(first));
        contract.generations.push(Generation { consent: next, providers: vec![b, c] });
        route.aux.insert(key.clone(), borsh::to_vec(&Row::Active(contract.clone())).unwrap());
        assert_eq!(material_provider_generation_from_route_v4(&route, q.root, a).unwrap(), Some(first));
        assert_eq!(material_provider_generation_from_route_v4(&route, q.root, b).unwrap(), Some(next));
        assert_eq!(material_provider_generation_from_route_v4(&route, q.root, c).unwrap(), Some(next));
        assert_eq!(material_provider_generation_from_route_v4(&route, q.root, bond(14)).unwrap(), None);
        let cold: PalwKernelRouteStateV1 = borsh::from_slice(&borsh::to_vec(&route).unwrap()).unwrap();
        assert_eq!(material_provider_generation_from_route_v4(&cold, q.root, b).unwrap(), Some(next));
        route.aux.insert(key.clone(), initial);
        assert_eq!(material_provider_generation_from_route_v4(&route, q.root, b).unwrap(), Some(first));
        assert_eq!(material_provider_generation_from_route_v4(&route, q.root, c).unwrap(), None);
        let terminal = Row::Terminal(Terminal {
            contract: contract.clone(),
            final_consent: next,
            tail_block: Hash64::from_bytes([15; 64]),
            release_block: Hash64::from_bytes([16; 64]),
            unused_native_session: false,
            acknowledged: Default::default(),
        });
        route.aux.insert(key.clone(), borsh::to_vec(&terminal).unwrap());
        assert_eq!(material_provider_generation_from_route_v4(&route, q.root, b).unwrap(), None);
        contract.terms.root = Hash64::from_bytes([17; 64]);
        route.aux.insert(key.clone(), borsh::to_vec(&Row::Active(contract)).unwrap());
        assert!(matches!(material_provider_generation_from_route_v4(&route, q.root, b), Err(NativeDecodeErrorV1::Decode(_))));
        route.aux.insert(key, vec![0]);
        assert!(matches!(material_provider_generation_from_route_v4(&route, q.root, b), Err(NativeDecodeErrorV1::Decode(_))));
    }
}

/// Build an explicitly funded public availability challenge, never arithmetic
/// guilt or a successful approval. The caller must retain its acquisition cause
/// (including local capacity), use its own key, and rehearse/submit with its own
/// fee payer. Native admission establishes the exact bounded answer obligation.
pub fn monetary_material_challenge_object_v4(
    network: Hash64,
    query: &PalwKernelMaterialUnitQueryV4,
    valid_until_daa: u64,
    challenger: PalwBondKeyV2,
    mut sign: impl FnMut(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<NativeRetainedV1<crate::palw_state_v2::PalwConsensusObjectV2>, String> {
    use crate::{
        palw_monetary_material_v1::PalwMonetaryMaterialActionV1 as A,
        palw_provider_court_v1::{PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1, palw_provider_court_message_v1},
        palw_state_v2::PalwConsensusObjectV2 as O,
    };
    query.validate()?;
    if valid_until_daa == 0 || challenger == query.provider {
        return Err("availability challenge deadline/identity differs".into());
    }
    let count = std::mem::size_of::<O>() as u64
        + std::mem::size_of::<A>() as u64
        + 16384
        + 2 * crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64;
    let lease = reserve_native_requests_v1(count).map_err(|e| e.to_string())?;
    let action = A::ChallengeCurrentV4 {
        root: query.root,
        consent: query.consent,
        provider: query.provider,
        dependency: query.dependency,
        stage: query.stage,
        position: query.position,
        valid_until_daa,
    };
    let bytes = borsh::to_vec(&action).map_err(|e| e.to_string())?;
    let message = palw_provider_court_message_v1(network, 242, &challenger, &bytes);
    let signature = sign(message.as_byte_slice(), PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1)
        .ok_or("availability challenge own signer unavailable")?;
    if signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
        return Err("availability challenge signature length differs".into());
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        O::MonetaryMaterialActionV1 { action: Box::new(action), signer: challenger, signature },
        lease,
    ))
}

/// Off-chain acquisition cause. None is arithmetic guilt or a successful
/// verifier approval. Legacy unclassified adapters map to Unknown, never infer
/// provider fault from an error string.
#[derive(Debug, thiserror::Error)]
pub enum PalwKernelMaterialFetchErrorV4 {
    #[error("local material capacity: {0}")]
    LocalCapacity(String),
    #[error("material unavailable: {0}")]
    Unavailable(String),
    #[error("material authentication/framing: {0}")]
    InvalidMaterial(String),
    #[error("material acquisition unclassified: {0}")]
    Unknown(String),
}

/// Typed native read used by admission as well as off-chain read permission.
/// Resource exhaustion retains its cause and cannot convict a valid carrier.
pub fn material_provider_generation_v4(
    state: &PalwChainStateV2,
    root: Hash64,
    provider: PalwBondKeyV2,
) -> Result<Option<Hash64>, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
    let Some(route) = state.kernel_route() else { return Ok(None) };
    material_provider_generation_from_route_v4(route, root, provider)
}

/// Same generation selection for a worker's own accepted route snapshot. This
/// only selects a data-retention generation; it grants no query authority,
/// verifier assignment or computation approval. Each actual query must obtain
/// fresh own-native authority, including across reorg/import and provider swap.
/// A caller-supplied or remote route is not authenticated by this helper.
pub fn material_provider_generation_from_route_v4(
    route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1,
    root: Hash64,
    provider: PalwBondKeyV2,
) -> Result<Option<Hash64>, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
    use crate::{
        palw_material_release_recovery_v2::{PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, PalwMaterialRecoveryRowV2},
        palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1 as E, retain_native_value_decode_classified_v1},
    };
    let Some(raw) = route
        .aux
        .iter()
        .find(|((t, k), _)| *t == PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2 && k.as_slice() == root.as_byte_slice())
        .map(|(_, b)| b.as_slice())
    else {
        return Ok(None);
    };
    preflight_native_value_v1::<PalwMaterialRecoveryRowV2>(raw, raw.len() as u64).map_err(|e| {
        if e.kind() == std::io::ErrorKind::Unsupported {
            E::Resource(NativeCoverageV1::unsupported::<PalwMaterialRecoveryRowV2>("current material generation", &e.to_string()))
        } else {
            E::Decode(e.to_string())
        }
    })?;
    let row =
        retain_native_value_decode_classified_v1(raw, || borsh::from_slice::<PalwMaterialRecoveryRowV2>(raw)).map_err(
            |e| match e {
                E::Resource(e) => E::Resource(e),
                E::Decode(e) => E::Decode(e.to_string()),
            },
        )?;
    if row.contract().terms.root != root {
        return Err(E::Decode("current material generation row substitutes its root key".into()));
    }
    let PalwMaterialRecoveryRowV2::Active(contract) = &*row else { return Ok(None) };
    Ok(contract.generations.iter().rev().find(|g| g.providers.contains(&provider)).map(|g| g.consent))
}
