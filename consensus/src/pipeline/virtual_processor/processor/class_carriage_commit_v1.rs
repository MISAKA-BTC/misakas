//! Declarations selected by the successful native fold, retained until the
//! same database batch commits its PALW delta and UTXO outcome. Rehearsals
//! never publish a class declaration, including on a discarded retry.
use crate::model::stores::palw_class_carriage::PalwClassCarriageRecord;
use kaspa_consensus_core::{
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, reserve_native_pending_requests_v1},
    palw_state_v2::{
        PalwClassStateV2, PalwClassStatusV2, PalwConsensusObjectV2, PalwDeltaEntryV2, PalwStateDeltaV2,
        palw_genesis_registrant_bond_v1,
    },
};
use kaspa_hashes::Hash64;

pub(super) struct PreparedClassCarriagesV1 {
    pub rows: Vec<(Hash64, PalwClassCarriageRecord)>,
    pub lease: Option<NativeDemandLeaseV1>,
}

fn fault(why: &str) -> NativeCoverageV1 {
    NativeCoverageV1::unsupported::<PreparedClassCarriagesV1>("accepted class declaration staging", why)
}

fn accepted_class(
    delta: &PalwStateDeltaV2,
    id: Hash64,
    artifact: Hash64,
    model_immutable_active: bool,
) -> Result<(bool, &PalwClassStateV2), NativeCoverageV1> {
    let mut registrations = delta.entries.iter().filter_map(|entry| match entry {
        PalwDeltaEntryV2::Class { key, old, new: Some(row) }
            if *key == id
                && old.as_ref().is_none_or(|previous| matches!(previous.status, PalwClassStatusV2::Dormant { .. }))
                && matches!(row.status, PalwClassStatusV2::Active | PalwClassStatusV2::Registered { .. }) =>
        {
            Some((old.as_ref(), row))
        }
        _ => None,
    });
    let (old, row) =
        registrations.next().ok_or_else(|| fault("declaration has no class registration in the successful native delta"))?;
    let preserved = old.filter(|_| model_immutable_active);
    let expected_daa = preserved.map_or(delta.point.daa_score, |previous| previous.registered_daa);
    if registrations.next().is_some() || row.artifact_root != artifact || row.registered_daa != expected_daa {
        return Err(fault("declaration differs from the unique native class registration"));
    }
    if preserved.is_some_and(|previous| previous.artifact_root != artifact) {
        return Err(fault("immutable renewal changes its original artifact"));
    }
    Ok((preserved.is_none(), row))
}

pub(super) fn prepare_v1(
    objects: &[PalwConsensusObjectV2],
    delta: &PalwStateDeltaV2,
    model_immutable_active: bool,
) -> Result<PreparedClassCarriagesV1, NativeCoverageV1> {
    prepare_with_reservation_v1(objects, delta, model_immutable_active, reserve_native_pending_requests_v1)
}

fn prepare_with_reservation_v1(
    objects: &[PalwConsensusObjectV2],
    delta: &PalwStateDeltaV2,
    model_immutable_active: bool,
    reserve: impl FnOnce(u64) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1>,
) -> Result<PreparedClassCarriagesV1, NativeCoverageV1> {
    let declarations = || {
        objects.iter().enumerate().filter_map(|(index, object)| match object {
            PalwConsensusObjectV2::ClassRegistered {
                class_id,
                artifact_root,
                slash_value_per_pwu,
                pwu_rule,
                admission: Some(carriage),
                ..
            } => Some((index, *class_id, *artifact_root, *slash_value_per_pwu, *pwu_rule, carriage.as_ref())),
            _ => None,
        })
    };
    let mut count = 0usize;
    let mut requests = 0u64;
    for (index, id, artifact, slash, pwu_rule, carriage) in declarations() {
        let (stage, row) = accepted_class(delta, id, artifact, model_immutable_active)?;
        if objects[..index].iter().any(|object| {
            matches!(object,
                PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(_), .. } if *class_id == id
            )
        }) {
            return Err(fault("multiple accepted declarations claim the same native class registration"));
        }
        let registrant = (carriage.registrant_bond != palw_genesis_registrant_bond_v1()).then_some(carriage.registrant_bond);
        if carriage.canonical.shape_profile_id != id
            || (row.slash_value_per_pwu, row.pwu_rule, row.registrant_bond) != (slash, pwu_rule, registrant)
        {
            return Err(fault("declaration does not match the accepted class profile and registration facts"));
        }
        if !stage {
            continue;
        }
        let bytes = borsh::object_length(carriage).map_err(|e| fault(&e.to_string()))? as u64;
        // Same serialization envelope as genesis: canonical Vec, record/Arc,
        // bincode and DB batch copies, plus outer vector allocation framing.
        let frames = 8 * size_of::<(Hash64, PalwClassCarriageRecord)>() as u64
            + 2 * size_of::<PalwClassCarriageRecord>() as u64
            + 2 * size_of::<[usize; 2]>() as u64;
        requests = bytes
            .checked_mul(6)
            .and_then(|n| n.checked_add(frames))
            .and_then(|n| requests.checked_add(n))
            .ok_or_else(|| fault("declaration workspace overflows"))?;
        count = count.checked_add(1).ok_or_else(|| fault("declaration count overflows"))?;
    }
    let lease = if count == 0 { None } else { reserve(requests)? };
    let mut rows = Vec::new();
    rows.try_reserve_exact(count).map_err(|_| fault("declaration list allocation refused"))?;
    if rows.capacity() != count {
        return Err(fault("declaration list allocation differs from its reservation"));
    }
    for (_, id, artifact, _, _, carriage) in declarations() {
        if !accepted_class(delta, id, artifact, model_immutable_active)?.0 {
            continue;
        }
        // The legacy profile ID helper serializes into its own Vec. The
        // staging lease must exist before invoking it.
        if carriage.profile.shape_profile_id() != id {
            return Err(fault("declaration profile differs from the accepted class ID"));
        }
        let length = borsh::object_length(carriage).map_err(|e| fault(&e.to_string()))?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(length).map_err(|_| fault("declaration body allocation refused"))?;
        if bytes.capacity() != length {
            return Err(fault("declaration body allocation differs from its reservation"));
        }
        // Write through a fixed slice so serialization cannot silently grow
        // beyond the already reserved canonical length.
        bytes.resize(length, 0);
        let mut remaining = bytes.as_mut_slice();
        borsh::to_writer(&mut remaining, carriage).map_err(|e| fault(&e.to_string()))?;
        if !remaining.is_empty() {
            return Err(fault("declaration canonical length changed during serialization"));
        }
        rows.push((id, PalwClassCarriageRecord { registered_daa: delta.point.daa_score, artifact_root: artifact, carriage: bytes }));
    }
    // Later bincode serialization and the RocksDB batch copy still need
    // capacity. Keep the full ticket pending until commit or abandonment.
    Ok(PreparedClassCarriagesV1 { rows, lease })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2, PalwPwuRuleV2};

    #[test]
    fn declaration_requires_the_exact_new_class_in_the_committed_delta() {
        let id = Hash64::from_u64_word(1);
        let artifact = Hash64::from_u64_word(2);
        let row = PalwClassStateV2 {
            artifact_root: artifact,
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1),
            status: PalwClassStatusV2::Active,
            registered_daa: 10,
            registrant_bond: None,
            fused_attention: false,
        };
        let mut delta = PalwStateDeltaV2 {
            point: PalwBlockContextV2 { block: Hash64::from_u64_word(3), daa_score: 10, blue_score: 10, subsidy: 0 },
            entries: vec![PalwDeltaEntryV2::Class { key: id, old: None, new: Some(row.clone()) }],
        };
        assert_eq!(accepted_class(&delta, id, artifact, false).unwrap(), (true, &row));
        assert!(accepted_class(&delta, Hash64::from_u64_word(4), artifact, false).is_err());
        assert!(accepted_class(&delta, id, Hash64::from_u64_word(4), false).is_err());
        delta.point.daa_score += 1;
        assert!(accepted_class(&delta, id, artifact, false).is_err());
        delta.point.daa_score -= 1;
        delta.entries.push(delta.entries[0].clone());
        assert!(accepted_class(&delta, id, artifact, false).is_err());
        delta.entries.truncate(1);
        let PalwDeltaEntryV2::Class { old, .. } = &mut delta.entries[0] else { unreachable!() };
        *old = Some(row);
        assert!(accepted_class(&delta, id, artifact, false).is_err(), "an update is not the registration's birth");
        delta.entries.clear();
        assert!(accepted_class(&delta, id, artifact, false).is_err());
    }

    #[test]
    fn prepare_stages_the_exact_class_carriage_from_the_native_registration_delta() {
        use kaspa_consensus_core::{
            palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context},
            palw_class_admission_v2::palw_post_genesis_registration_v1,
            palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2, apply_palw_transition_v2},
            tx::TransactionOutpoint,
        };
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).unwrap();
        let id = profile.shape_profile_id();
        let canonical = rc_job_context(&profile, 8, 4);
        let artifact = Hash64::from_u64_word(2);
        let bond = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(5), 0));
        // The native fold takes already admitted objects. Transaction/bond
        // authentication is upstream and outside this staging regression.
        let object = palw_post_genesis_registration_v1(profile, canonical, artifact, 1_000, 1, 1, 0, bond, Vec::new()).unwrap();
        let public_key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([13; 32]).verification_key.as_ref().to_vec();
        let bond_object = PalwConsensusObjectV2::BondRegistered {
            bond,
            payout_payload: kaspa_hashes::blake2b_512_address_payload(&public_key),
            pubkey: public_key.clone(),
            operator_pubkey: public_key,
            collateral: 1_000_000_000_000,
            capable_classes: Default::default(),
            signature: Vec::new(),
        };
        let params = PalwStateParamsV2::new(100, 10, 40, 20, 500, 1000, id, 4, 1000, 100, 1000, 600).unwrap();
        let point = PalwBlockContextV2 { block: Hash64::from_u64_word(3), daa_score: 10, blue_score: 10, subsidy: 0 };
        let (_, delta) =
            apply_palw_transition_v2(&PalwChainStateV2::genesis(), &params, &point, &[bond_object, object.clone()], None).unwrap();
        let objects = std::slice::from_ref(&object);
        let PalwConsensusObjectV2::ClassRegistered { admission: Some(carriage), .. } = &object else { unreachable!() };
        let prepared = prepare_v1(objects, &delta, false).unwrap();
        assert_eq!(
            prepared.rows,
            [(
                id,
                PalwClassCarriageRecord {
                    registered_daa: point.daa_score,
                    artifact_root: artifact,
                    carriage: borsh::to_vec(carriage.as_ref()).unwrap(),
                }
            )]
        );
        assert!(prepared.lease.is_some());

        let missing = PalwStateDeltaV2 { entries: Vec::new(), ..delta.clone() };
        assert!(prepare_v1(objects, &missing, false).is_err());
        let mut changed = object.clone();
        let PalwConsensusObjectV2::ClassRegistered { admission: Some(carriage), .. } = &mut changed else { unreachable!() };
        carriage.canonical.shape_profile_id = Hash64::from_u64_word(99);
        assert!(prepare_v1(&[changed], &delta, false).is_err());
        assert!(prepare_v1(&[object.clone(), object.clone()], &delta, false).is_err());
        let mut reservation_attempted = false;
        assert!(
            prepare_with_reservation_v1(objects, &delta, false, |requests| {
                reservation_attempted = true;
                assert!(requests > 0);
                Err(fault("fixture has zero free capacity"))
            })
            .is_err()
        );
        assert!(reservation_attempted);

        // Exercise the two journal shapes of an accepted dormant renewal.
        // Only the legacy rule gives it a new declaration/registration DAA.
        let mut renewed = delta.clone();
        renewed.point.daa_score += 1;
        let PalwDeltaEntryV2::Class { old, new: Some(new), .. } = renewed
            .entries
            .iter_mut()
            .find(|entry| matches!(entry, PalwDeltaEntryV2::Class { key, old: None, new: Some(_) } if *key == id))
            .unwrap()
        else {
            unreachable!()
        };
        let mut dormant = new.clone();
        dormant.status = PalwClassStatusV2::Dormant { since_daa: point.daa_score };
        *old = Some(dormant);
        let preserved =
            prepare_with_reservation_v1(objects, &renewed, true, |_| panic!("immutable renewal must not allocate a declaration"))
                .unwrap();
        assert!(preserved.rows.is_empty());
        assert!(preserved.lease.is_none());

        let PalwDeltaEntryV2::Class { new: Some(new), .. } = renewed
            .entries
            .iter_mut()
            .find(|entry| matches!(entry, PalwDeltaEntryV2::Class { key, old: Some(_), new: Some(_) } if *key == id))
            .unwrap()
        else {
            unreachable!()
        };
        new.registered_daa = renewed.point.daa_score;
        let legacy = prepare_v1(objects, &renewed, false).unwrap();
        assert_eq!(legacy.rows[0].1.registered_daa, renewed.point.daa_score);
        assert_eq!(legacy.rows[0].1.carriage, prepared.rows[0].1.carriage);
        assert!(prepare_v1(objects, &renewed, true).is_err());
    }
}
