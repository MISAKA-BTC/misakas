//! Registration candidates retained at the original Assignment prefix. The
//! candidate keys restrict work at draw time. Eligibility is still read from
//! the actual live bond map under the frozen commit clock and policy.
use super::{positive_assignment_certification::reserve_p_workspace, *};
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1};
use crate::palw_positive_assignment_certification_v2::{
    assignment_verifier_registration_cutoff_v2, assignment_verifier_registry_bound_v2,
};
use crate::palw_security_certification_v1::PalwSecurityCertificationPolicyV1;

const MAGIC: &[u8; 8] = b"PC2ACOH1";
const HEADER: usize = MAGIC.len() + 4;
const KEY_BYTES: usize = 64 + 4;

fn refused(why: impl std::fmt::Debug) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(format!("Assignment verifier candidates: {why:?}"))
}

fn key_from_bytes(bytes: &[u8; KEY_BYTES]) -> PalwBondKeyV2 {
    let hash = Hash64::from_bytes(bytes[..64].try_into().expect("fixed hash width"));
    let index = u32::from_le_bytes(bytes[64..].try_into().expect("fixed index width"));
    PalwBondKeyV2(TransactionOutpoint::new(hash, index))
}

fn extent(count: u32) -> Result<usize, PalwStateV2Error> {
    (count as usize)
        .checked_mul(KEY_BYTES)
        .and_then(|bytes| bytes.checked_add(HEADER))
        .ok_or_else(|| refused("encoded extent overflow"))
}

/// Prefix of the existing reservation custody preimage. Remaining bytes are
/// reserved for the actual capacity owner and have their own validation. This
/// section alone establishes no reservation or authenticated ancestry.
pub(super) struct AssignmentVerifierCandidatesV2<'a>(&'a [[u8; KEY_BYTES]]);
impl<'a> AssignmentVerifierCandidatesV2<'a> {
    fn decode_framing(preimage: &'a [u8]) -> Result<Self, PalwStateV2Error> {
        let header = preimage.get(..HEADER).ok_or_else(|| refused("truncated cohort header"))?;
        if &header[..MAGIC.len()] != MAGIC {
            return Err(refused("wrong cohort format"));
        }
        let count = u32::from_le_bytes(header[MAGIC.len()..].try_into().expect("fixed count width"));
        let keys = preimage.get(HEADER..extent(count)?).ok_or_else(|| refused("truncated cohort keys"))?;
        let (keys, []) = keys.as_chunks::<KEY_BYTES>() else { unreachable!("checked extent has whole keys") };
        Ok(Self(keys))
    }

    pub(super) fn decode(preimage: &'a [u8]) -> Result<Self, PalwStateV2Error> {
        let view = Self::decode_framing(preimage)?;
        let mut previous = None;
        for key in view.keys() {
            if previous.is_some_and(|p| p >= key) {
                return Err(refused("cohort keys must be unique and in native order"));
            }
            previous = Some(key);
        }
        Ok(view)
    }

    pub(super) fn keys(&self) -> impl ExactSizeIterator<Item = PalwBondKeyV2> + '_ {
        self.0.iter().map(key_from_bytes)
    }
}

pub(super) struct LeasedAssignmentVerifierCandidatesV2 {
    bytes: Vec<u8>,
    count: u64,
    _lease: NativeDemandLeaseV1,
}
impl LeasedAssignmentVerifierCandidatesV2 {
    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) fn count(&self) -> u64 {
        self.count
    }
}

/// The real A prefix is scanned at most twice. Bound both passes before
/// touching the map and reserve the exact encoded extent before allocation.
pub(super) fn derive_assignment_verifier_candidates_v2(
    state: &PalwChainStateV2,
    policy: &PalwSecurityCertificationPolicyV1,
    anchor_daa: u64,
    result_before_daa: u64,
    intake_daa: u64,
    work_rows: u64,
    max_bytes: u64,
) -> Result<LeasedAssignmentVerifierCandidatesV2, PalwStateV2Error> {
    let cutoff = assignment_verifier_registration_cutoff_v2(policy, anchor_daa, result_before_daa).map_err(refused)?;
    let count = assignment_verifier_registry_bound_v2(
        policy,
        anchor_daa,
        result_before_daa,
        intake_daa,
        work_rows / 2,
        state.bonds.values().map(|bond| bond.registered_daa),
    )
    .map_err(refused)?;
    let encoded_count = u32::try_from(count).map_err(refused)?;
    let bytes = extent(encoded_count)?;
    if bytes as u64 > max_bytes {
        return Err(refused("cohort exceeds original byte budget"));
    }
    let lease = reserve_p_workspace(bytes as u64)?;
    let mut encoded = Vec::new();
    let allocation = || {
        PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<Vec<u8>>(
            "Assignment verifier candidates",
            "allocation did not fit the leased capacity",
        ))
    };
    encoded.try_reserve_exact(bytes).map_err(|_| allocation())?;
    if encoded.capacity() > bytes {
        return Err(allocation());
    }
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&encoded_count.to_le_bytes());
    if let Some(cutoff) = cutoff.filter(|_| count != 0) {
        for (key, bond) in &state.bonds {
            if bond.registered_daa <= cutoff {
                encoded.extend_from_slice(&key.0.transaction_id.as_bytes());
                encoded.extend_from_slice(&key.0.index.to_le_bytes());
            }
        }
    }
    debug_assert_eq!(encoded.len(), bytes);
    Ok(LeasedAssignmentVerifierCandidatesV2 { bytes: encoded, count, _lease: lease })
}

impl PalwChainStateV2 {
    /// Audit retained keys against the exact original pre-event state. The
    /// native caller supplies the independently resolved signed ancestor DAA
    /// and charges this pass from the original replay budget. This establishes
    /// data equality only; it does not restore capacity or certify a claim.
    pub fn check_original_assignment_verifier_candidates_v2(
        &self,
        original: &crate::palw_positive_assignment_material_v2::AssignmentAdmissionMaterialViewV2<'_>,
        anchor_daa: u64,
        result_before_daa: u64,
        remaining_work: u64,
    ) -> Result<(), PalwStateV2Error> {
        use misaka_palw_positive_contract::assignment::open_material::AssignmentOpenPreimageKindV2;

        let material = &original.material;
        let preimage = original.open.preimage(AssignmentOpenPreimageKindV2::ReservationCustody);
        let per_row = material.native_preimages.source_policy.work.per_control;
        let max_rows = remaining_work.checked_div(per_row).ok_or_else(|| refused("invalid original work schedule"))?;
        self.compare_assignment_verifier_candidates_v2(
            preimage,
            &material.certification_policy,
            anchor_daa,
            result_before_daa,
            material.origin.accepted_daa,
            max_rows,
        )
    }

    fn compare_assignment_verifier_candidates_v2(
        &self,
        preimage: &[u8],
        policy: &PalwSecurityCertificationPolicyV1,
        anchor_daa: u64,
        result_before_daa: u64,
        intake_daa: u64,
        max_rows: u64,
    ) -> Result<(), PalwStateV2Error> {
        if anchor_daa > intake_daa || intake_daa >= result_before_daa {
            return Err(refused("original intake clock"));
        }
        let cutoff = assignment_verifier_registration_cutoff_v2(policy, anchor_daa, result_before_daa).map_err(refused)?;
        let retained = AssignmentVerifierCandidatesV2::decode_framing(preimage)?;
        let mut keys = retained.keys();
        if let Some(cutoff) = cutoff {
            if self.bonds.len() as u64 > max_rows {
                return Err(refused("original verifier replay work cap"));
            }
            for (key, bond) in &self.bonds {
                if bond.registered_daa > intake_daa {
                    return Err(refused("invalid original registration chronology"));
                }
                if bond.registered_daa <= cutoff && keys.next() != Some(*key) {
                    return Err(refused("retained verifier candidates differ from original registry"));
                }
            }
        }
        if keys.next().is_some() {
            return Err(refused("unexpected retained verifier candidate"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_kernel_route_v1::palw_kernel_bond_id_v1;
    use misaka_palw_positive_contract::identity::Digest;

    fn key(index: u32) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(1), index))
    }

    fn fixture() -> PalwChainStateV2 {
        let mut state = PalwChainStateV2::genesis();
        for n in 1..=6 {
            let mut bond =
                palw_bond_state_from_registration_v2(&[n as u8; 32], &[n as u8; 32], 100, Hash64::default(), 97, Default::default());
            bond.operator_id = Hash64::from_u64_word(u64::from(n));
            state.bonds.insert(key(n), bond);
        }
        state
    }

    fn policy() -> PalwSecurityCertificationPolicyV1 {
        PalwSecurityCertificationPolicyV1 {
            m: 1,
            k: 1,
            veto: 0,
            anchor_delay_spans: 2,
            attest_window_daa: 5,
            prosecution_window_daa: 19,
            certify_by_daa: 7,
            verifier_maturity_daa: 5,
            verifier_min_collateral_sompi: 10,
            draws_per_epoch: 4,
            epoch_daa: 11,
        }
    }

    fn preimage(indices: &[u32]) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&(indices.len() as u32).to_le_bytes());
        for index in indices {
            bytes.extend_from_slice(&borsh::to_vec(&key(*index)).unwrap());
        }
        bytes
    }

    #[test]
    fn cohort_codec_requires_full_unique_native_ordered_keys() {
        let mut raw = preimage(&[1, 256]);
        assert!(AssignmentVerifierCandidatesV2::decode(&raw[..raw.len() - 1]).is_err());
        raw.extend_from_slice(b"separate capacity owner data");
        let decoded = AssignmentVerifierCandidatesV2::decode(&raw).unwrap();
        assert_eq!(decoded.keys().collect::<Vec<_>>(), [key(1), key(256)]);
        assert!(AssignmentVerifierCandidatesV2::decode(&preimage(&[256, 1])).is_err());
        assert!(AssignmentVerifierCandidatesV2::decode(&preimage(&[1, 1])).is_err());
        assert_eq!(AssignmentVerifierCandidatesV2::decode(&preimage(&[])).unwrap().keys().len(), 0);
        let mut excessive = preimage(&[]);
        excessive[MAGIC.len()..HEADER].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(AssignmentVerifierCandidatesV2::decode(&excessive).is_err());
    }

    #[test]
    fn cohort_derivation_checks_both_passes_and_extent_before_request() {
        let mut state = fixture();
        let policy = policy();
        let check = |state: &PalwChainStateV2, work, bytes| {
            derive_assignment_verifier_candidates_v2(state, &policy, 100, 104, 100, work, bytes).err().unwrap().to_string()
        };
        assert!(check(&state, 11, 4096).contains("VerifierRegistryWorkBound"));
        assert!(check(&state, 12, 419).contains("cohort exceeds original byte budget"));
        state.bonds.get_mut(&key(6)).unwrap().registered_daa = 101;
        assert!(check(&state, 12, 4096).contains("InvalidRegistrationChronology"));
    }

    #[test]
    fn original_cohort_replay_rejects_partial_reordered_or_wrong_branch_data() {
        let mut state = fixture();
        let policy = policy();
        let check = |state: &PalwChainStateV2, indices: &[u32], work| {
            state.compare_assignment_verifier_candidates_v2(&preimage(indices), &policy, 100, 104, 100, work)
        };
        assert!(check(&state, &[1, 2, 3, 4, 5, 6], 6).is_ok());
        assert!(check(&state, &[1, 2, 3, 4, 5, 6], 5).is_err());
        for indices in [&[1, 2, 3, 4, 5][..], &[1, 3, 2, 4, 5, 6], &[1, 2, 3, 4, 5, 6, 7]] {
            assert!(check(&state, indices, 6).is_err());
        }
        let bond = state.bonds.remove(&key(6)).unwrap();
        state.bonds.insert(key(7), bond);
        assert!(check(&state, &[1, 2, 3, 4, 5, 6], 6).is_err());
        assert!(check(&state, &[1, 2, 3, 4, 5, 7], 6).is_ok());
        state.bonds.get_mut(&key(7)).unwrap().registered_daa = 101;
        assert!(check(&state, &[1, 2, 3, 4, 5], 6).is_err());
    }

    #[test]
    fn cohort_looks_up_live_status_stake_worker_and_reregistered_outpoints() {
        use super::super::positive_assignment_certification::live_verifiers;

        let mut state = fixture();
        let reused = state.bonds.get_mut(&key(2)).unwrap();
        reused.registered_daa = 100;
        let retiring = state.bonds.get_mut(&key(3)).unwrap();
        retiring.status = PalwBondStatusV2::Retiring { since_daa: 104, settled_at_since: 0 };
        state.bonds.get_mut(&key(4)).unwrap().collateral = 9;
        state.bonds.get_mut(&key(5)).unwrap().collateral = 17;
        let worker = Digest(state.bonds[&key(6)].operator_id.as_bytes());
        state.bonds.remove(&key(1));
        let raw = preimage(&[1, 2, 3, 4, 5, 6]);
        let keys = AssignmentVerifierCandidatesV2::decode(&raw).unwrap();
        let policy = policy();
        let members = live_verifiers(&state, &policy, 103, worker, keys.keys()).collect::<Vec<_>>();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].bond, palw_kernel_bond_id_v1(&key(5)));
        assert_eq!(members[0].stake, 17);
    }
}
