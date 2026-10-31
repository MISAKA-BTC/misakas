//! **RFC-0006 RFC06-8: a class's registering block and admission digest, kept in state** — past the dormant fence
//! `palw_class_registration_origin_v1` (kernel route aux 141, keyed by the 64-byte class id).
//!
//! Why: a class id authenticates its profile, never its canonical job (`palw_class_sync_v1`), and `PalwClassStateV2` keeps no
//! trace of the registration that created it. A post-genesis class without retained semantics therefore had no authenticated
//! canonical identity: a snapshot peer's declaration bytes could not be told from a substitute, so complete snapshot capability
//! refused it ("missing authenticated legacy canonical identity"). Past this fence every registration door (legacy
//! `ClassRegistered`, `ClassRegisteredTirV1`, `ClassRegisteredGenV1`) writes one row: the chain block whose transition folded it,
//! its DAA score, the door, and a domain-separated digest of the admission carriage it carried. A declaration is then
//! authenticated by state alone: its bytes hash to the row's digest and the row's DAA is the class's `registered_daa`
//! ([`palw_class_declaration_origin_authenticated_v1`]). The digest is streamed (no copy of the carriage is allocated).
//!
//! **What this does not do:** classes registered BEFORE the fence have no row (their bytes are not in state), so they stay
//! refused exactly as before; the decision note in the RFC06–12 remaining-work document records the options for them. The
//! row is identity evidence only: no credit, weight, payment or approval reads it (RFC-0018 Gate C stays the only credit
//! path), and it proves nothing about computation (PRINCIPLES §8).
//!
//! Below the fence nothing is read or written, so live testnet-12 state and roots are unchanged; the fence is `None` on every
//! preset.
use crate::{
    Hash64,
    config::params::{ForkActivation, Params},
    palw_kernel_route_v1::PalwKernelRouteStateV1,
    palw_mode_v2::PalwModeV2Error,
    palw_state_v2::{PalwClassStateV2, PalwConsensusObjectV2},
};
use borsh::{BorshDeserialize, BorshSerialize};

/// Kernel route aux table of the registration origin rows (keyed by the 64-byte class id).
pub const PALW_CLASS_REGISTRATION_ORIGIN_TABLE_V1: u8 = 141;
pub const PALW_CLASS_REGISTRATION_ORIGIN_ROW_VERSION_V1: u16 = 1;
/// Domain of the admission digest: `H(domain ‖ len_le64 ‖ borsh(admission))` (`misaka_palw_kernel::hash::id`).
pub const PALW_CLASS_ADMISSION_DIGEST_DOMAIN_V1: &[u8] = b"misaka-palw/class-registration/admission-digest/v1";

/// Which registration door folded the class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwClassRegistrationDoorV1 {
    /// `ClassRegistered` (its optional `PalwClassAdmissionCarriageV2`).
    Legacy = 0,
    /// `ClassRegisteredTirV1` (its `PalwTirAdmissionCarriageV1`).
    Tir = 1,
    /// `ClassRegisteredGenV1` (its `PalwGenAdmissionCarriageV1`).
    Gen = 2,
}

/// **One class's registration origin** (aux 141).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClassRegistrationOriginRowV1 {
    pub version: u16,
    pub class_id: Hash64,
    /// The chain block whose transition folded the registration.
    pub block: Hash64,
    /// That block's DAA score — the class's `registered_daa` for this registration.
    pub daa_score: u64,
    pub door: PalwClassRegistrationDoorV1,
    /// The admission carriage's digest; `None` for a registration that carried none.
    pub admission_digest: Option<Hash64>,
}

pub fn palw_class_registration_origin_key_v1(class_id: &Hash64) -> Vec<u8> {
    class_id.as_byte_slice().to_vec()
}

/// The admission digest of a carried value, streamed into the hash (two passes: the borsh length, then the bytes).
pub fn palw_class_admission_digest_v1<T: BorshSerialize + ?Sized>(admission: &T) -> std::io::Result<Hash64> {
    let len = borsh::object_length(admission)?;
    let mut state = misaka_palw_kernel::hash::keyed(PALW_CLASS_ADMISSION_DIGEST_DOMAIN_V1);
    state.update(&(len as u64).to_le_bytes());
    admission.serialize(&mut state)?;
    Ok(Hash64::from_bytes(misaka_palw_kernel::hash::finish(state)))
}

/// The same digest over declaration bytes a peer offers (identical construction).
pub fn palw_class_admission_bytes_digest_v1(bytes: &[u8]) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::id(PALW_CLASS_ADMISSION_DIGEST_DOMAIN_V1, bytes))
}

/// What a registration object contributes to its origin row: its door and admission digest. `None` for any other object.
pub fn palw_class_registration_origin_of_v1(
    object: &PalwConsensusObjectV2,
) -> std::io::Result<Option<(PalwClassRegistrationDoorV1, Option<Hash64>)>> {
    Ok(match object {
        PalwConsensusObjectV2::ClassRegistered { admission, .. } => Some((
            PalwClassRegistrationDoorV1::Legacy,
            match admission.as_deref() {
                Some(carriage) => Some(palw_class_admission_digest_v1(carriage)?),
                None => None,
            },
        )),
        PalwConsensusObjectV2::ClassRegisteredTirV1 { admission, .. } => {
            Some((PalwClassRegistrationDoorV1::Tir, Some(palw_class_admission_digest_v1(&**admission)?)))
        }
        PalwConsensusObjectV2::ClassRegisteredGenV1 { admission, .. } => {
            Some((PalwClassRegistrationDoorV1::Gen, Some(palw_class_admission_digest_v1(&**admission)?)))
        }
        _ => None,
    })
}

/// The class's origin row, if the state holds one.
pub fn palw_class_registration_origin_row_v1(
    route: Option<&PalwKernelRouteStateV1>,
    class_id: &Hash64,
) -> Option<PalwClassRegistrationOriginRowV1> {
    route?.aux_row::<PalwClassRegistrationOriginRowV1>(
        PALW_CLASS_REGISTRATION_ORIGIN_TABLE_V1,
        &palw_class_registration_origin_key_v1(class_id),
    )
}

/// **RFC06-8's identity migration test**: are `bytes` the original admission carriage of this `ClassRegistered` class, by
/// state alone? The row must be well formed, name this class and the legacy door, carry a non-default block, date the class's
/// CURRENT registration (`registered_daa`), and hold the digest of exactly these bytes. Anything else is `false` (the caller
/// keeps its existing refusal).
pub fn palw_class_declaration_origin_authenticated_v1(
    route: Option<&PalwKernelRouteStateV1>,
    class_id: &Hash64,
    class: &PalwClassStateV2,
    bytes: &[u8],
) -> bool {
    let Some(row) = palw_class_registration_origin_row_v1(route, class_id) else { return false };
    row.version == PALW_CLASS_REGISTRATION_ORIGIN_ROW_VERSION_V1
        && row.class_id == *class_id
        && row.door == PalwClassRegistrationDoorV1::Legacy
        && row.block != Hash64::default()
        && row.daa_score == class.registered_daa
        && row.admission_digest == Some(palw_class_admission_bytes_digest_v1(bytes))
}

impl Params {
    /// The fence's activation, `None` where unarmed (`never` counts as unarmed).
    pub fn palw_class_registration_origin_v1_fence(&self) -> Option<ForkActivation> {
        self.palw_class_registration_origin_v1.filter(|at| *at != ForkActivation::never())
    }

    /// The rows live in the kernel route's aux store, so the kernel route fence must be armed at or below this one; the
    /// V2 bundle's mirror must agree. Arming is otherwise allowed: the fence only records identity evidence (no value to
    /// measure). It is `None` on every preset.
    pub fn validate_palw_class_registration_origin_v1(&self) -> Result<(), PalwModeV2Error> {
        let Some(at) = self.palw_class_registration_origin_v1_fence() else {
            return Ok(());
        };
        if !self.palw_probabilistic_constraints_v1.is_some_and(|k| k != ForkActivation::never() && k.daa_score() <= at.daa_score()) {
            return Err(PalwModeV2Error::Invalid(
                "palw_class_registration_origin_v1 requires the kernel route fence (aux 141's store) at or below it",
            ));
        }
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode
            && bundle.state.class_registration_origin_from() != Some(at.daa_score())
        {
            return Err(PalwModeV2Error::Invalid(
                "palw_class_registration_origin_v1 disagrees with the V2 bundle's mirror: Params::sync_palw_class_registration_origin_v1",
            ));
        }
        Ok(())
    }

    /// Mirror the fence into the V2 bundle's state params (the fold holds only those). `None` where unarmed.
    pub fn sync_palw_class_registration_origin_v1(&mut self) {
        let from = self.palw_class_registration_origin_v1_fence().map(|at| at.daa_score());
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_class_registration_origin_from(from);
        }
    }
}

#[cfg(test)]
mod tests {
    //! SOURCE ONLY (not compiled or executed in this pass).
    use super::*;
    use crate::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS};

    fn row(digest: Option<Hash64>) -> PalwClassRegistrationOriginRowV1 {
        PalwClassRegistrationOriginRowV1 {
            version: PALW_CLASS_REGISTRATION_ORIGIN_ROW_VERSION_V1,
            class_id: Hash64::from_bytes([1; 64]),
            block: Hash64::from_bytes([2; 64]),
            daa_score: 77,
            door: PalwClassRegistrationDoorV1::Legacy,
            admission_digest: digest,
        }
    }

    #[test]
    fn every_preset_keeps_the_fence_none_and_validates() {
        let t12 = Params::from(crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12));
        for p in [MAINNET_PARAMS, TESTNET_PARAMS, SIMNET_PARAMS, DEVNET_PARAMS, t12] {
            assert_eq!(p.palw_class_registration_origin_v1, None);
            assert_eq!(p.palw_class_registration_origin_v1_fence(), None);
            p.validate_palw_class_registration_origin_v1().unwrap();
        }
    }

    #[test]
    fn the_streamed_digest_equals_the_digest_of_the_bytes() {
        let value = (7u64, vec![3u8; 1000], Some(Hash64::from_bytes([9; 64])));
        let bytes = borsh::to_vec(&value).unwrap();
        assert_eq!(palw_class_admission_digest_v1(&value).unwrap(), palw_class_admission_bytes_digest_v1(&bytes));
        assert_ne!(palw_class_admission_bytes_digest_v1(&bytes), palw_class_admission_bytes_digest_v1(&bytes[1..]));
    }

    #[test]
    fn a_row_authenticates_exactly_its_bytes_door_class_and_registration() {
        let policy =
            crate::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::from_bytes([1; 64]), Hash64::from_bytes([2; 64]));
        let mut route = PalwKernelRouteStateV1::new(policy, None, Default::default());
        let bytes = vec![5u8; 300];
        let good = row(Some(palw_class_admission_bytes_digest_v1(&bytes)));
        let class_id = good.class_id;
        let class = PalwClassStateV2 {
            artifact_root: Hash64::from_u64_word(0xA57),
            slash_value_per_pwu: 5,
            pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 3 },
            status: crate::palw_state_v2::PalwClassStatusV2::Active,
            registered_daa: good.daa_score,
            registrant_bond: None,
            fused_attention: false,
        };
        // No row: not authenticated (the existing refusal stands).
        assert!(!palw_class_declaration_origin_authenticated_v1(Some(&route), &class_id, &class, &bytes));
        assert!(!palw_class_declaration_origin_authenticated_v1(None, &class_id, &class, &bytes));
        let put = |route: &mut PalwKernelRouteStateV1, r: &PalwClassRegistrationOriginRowV1| {
            route.aux.insert(
                (PALW_CLASS_REGISTRATION_ORIGIN_TABLE_V1, palw_class_registration_origin_key_v1(&class_id)),
                borsh::to_vec(r).unwrap(),
            );
        };
        put(&mut route, &good);
        assert!(palw_class_declaration_origin_authenticated_v1(Some(&route), &class_id, &class, &bytes));
        // Other bytes, another class, a later re-registration, another door, a default block, no digest: refused.
        assert!(!palw_class_declaration_origin_authenticated_v1(Some(&route), &class_id, &class, &bytes[1..]));
        assert!(!palw_class_declaration_origin_authenticated_v1(Some(&route), &Hash64::from_bytes([3; 64]), &class, &bytes));
        let mut later = class.clone();
        later.registered_daa += 1;
        assert!(!palw_class_declaration_origin_authenticated_v1(Some(&route), &class_id, &later, &bytes));
        for bad in [
            PalwClassRegistrationOriginRowV1 { door: PalwClassRegistrationDoorV1::Tir, ..good },
            PalwClassRegistrationOriginRowV1 { block: Hash64::default(), ..good },
            PalwClassRegistrationOriginRowV1 { admission_digest: None, ..good },
            PalwClassRegistrationOriginRowV1 { version: 2, ..good },
        ] {
            put(&mut route, &bad);
            assert!(!palw_class_declaration_origin_authenticated_v1(Some(&route), &class_id, &class, &bytes));
        }
    }
}
