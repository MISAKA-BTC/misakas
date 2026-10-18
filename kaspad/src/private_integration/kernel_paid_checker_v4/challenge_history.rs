//! A bounded local sequence of exact signed subjects can be checked against
//! the ONE current own-native prefix. No local completion marker, broadcast,
//! spent input, or bare hash is native acceptance or computation approval.
use super::{
    outbox::{CheckerOutboxErrorV4 as Error, action_paths},
    *,
};
use kaspa_consensus_core::{
    palw_material_challenge_checkpoint_v4::{
        PalwMaterialChallengeCheckpointV4, material_challenge_checkpoint_from_route_v4, material_challenge_statement_v4,
    },
    palw_material_challenge_history_v4::{
        PalwMaterialChallengeHistoryAnchorV4 as Anchor, PalwMaterialChallengeHistoryV4,
        advance_material_challenge_history_anchor_v4 as advance, material_challenge_history_base_anchor_v4 as native_anchor,
        material_challenge_history_from_route_v4,
    },
    palw_monetary_material_v1::PalwMonetaryMaterialActionV1 as Action,
};

#[derive(Clone, Copy, borsh::BorshSerialize, borsh::BorshDeserialize)]
struct Base {
    version: u16,
    network: Hash64,
    actor: PalwBondKeyV2,
    claim: Digest,
    enrollment: Digest,
    round: u32,
    anchor: Anchor,
    /// At most the single latest old A2, before A3 existed. Its domain-tagged
    /// bridge must match the own native prefix; it cannot recover lost history.
    legacy: Option<PalwMaterialChallengeCheckpointV4>,
}
const BASE_BYTES: u64 = 339;
const LEGACY_BASE_BYTES: u64 = BASE_BYTES + 433;

fn challenge<'a>(object: &'a PalwConsensusObjectV2, actor: PalwBondKeyV2, claim: Digest) -> Result<Option<&'a Action>, Error> {
    let PalwConsensusObjectV2::MonetaryMaterialActionV1 { action, signer, .. } = object else { return Ok(None) };
    let Action::ChallengeCurrentV4 { root, dependency, .. } = &**action else {
        return Err("checker history refuses an unbound/foreign monetary action".into());
    };
    if *signer != actor || *root != Hash64::from_bytes(claim) || *dependency != Hash64::from_bytes(claim) {
        return Err("checker history signed subject actor/claim differs".into());
    }
    Ok(Some(&**action))
}

fn read_action(outbox: &Path, slot: usize, cap: u64) -> Result<Option<NativeRetainedV1<PalwConsensusObjectV2>>, Error> {
    let (directory, path, _) = action_paths(outbox, slot);
    if directory.exists() {
        owned_dir(&directory)?;
    }
    if !path.exists() {
        return Ok(None);
    }
    let (raw, _raw) = read_bounded(&path, cap)?;
    preflight_native_value_v1::<PalwConsensusObjectV2>(&raw, raw.len() as u64).map_err(|e| e.to_string())?;
    Ok(Some(retain_native_value_decode_v1(&raw, || borsh::from_slice::<PalwConsensusObjectV2>(&raw)).map_err(|e| e.to_string())?))
}

fn current(
    route: &PalwKernelRouteStateV1,
    actor: PalwBondKeyV2,
) -> Result<(Anchor, Option<PalwMaterialChallengeCheckpointV4>, Option<PalwMaterialChallengeHistoryV4>), Error> {
    let network = Hash64::from_bytes(route.header.policy.network_domain);
    let checkpoint = material_challenge_checkpoint_from_route_v4(route, actor).map_err(|e| Error::NativeView(e.to_string()))?;
    let history = material_challenge_history_from_route_v4(route, actor).map_err(|e| Error::NativeView(e.to_string()))?;
    let anchor = native_anchor(network, actor, checkpoint.as_ref(), history.as_ref()).map_err(|e| Error::NativeView(e.into()))?;
    Ok((anchor, checkpoint, history))
}

/// Publish before the first new challenge object/transaction. For an existing
/// WAL the exact current A2/A3 may recover its previous anchor; an unrelated
/// latest checkpoint is never treated as that WAL's acceptance.
pub(super) fn ensure_base(
    outbox: &Path,
    cap: u64,
    route: &PalwKernelRouteStateV1,
    actor: PalwBondKeyV2,
    claim: Digest,
    enrollment: Digest,
    round: u32,
    first_existing: Option<&PalwConsensusObjectV2>,
) -> Result<(), Error> {
    let path = outbox.join("challenge-base.borsh");
    let network = Hash64::from_bytes(route.header.policy.network_domain);
    if path.exists() {
        read_base(outbox, cap, network, actor, claim, enrollment, round)?;
        return Ok(());
    }
    let (mut anchor, checkpoint, history) = current(route, actor)?;
    let mut legacy = None;
    if let Some(action) = first_existing.map(|o| challenge(o, actor, claim)).transpose()?.flatten() {
        if checkpoint
            .as_ref()
            .map(|c| c.matches(network, actor, action))
            .transpose()
            .map_err(|e| Error::NativeView(e.into()))?
            .unwrap_or(false)
        {
            if let Some(history) = history {
                anchor = Anchor { sequence: history.sequence - 1, root: history.previous };
            } else {
                legacy = checkpoint;
            }
        } else if checkpoint.is_some() {
            // The baseline of an existing signature is missing. A later,
            // unrelated own-native subject cannot move that signature after
            // itself. Recover only an exact latest subject or empty genesis.
            return Err(Error::NativeView("checker existing challenge lacks its original prefix baseline; exact WAL retained".into()));
        }
    }
    let base = Base { version: 4, network, actor, claim, enrollment, round, anchor, legacy };
    publish_borsh(&path, &base, cap)?;
    Ok(())
}

fn read_base(
    outbox: &Path,
    cap: u64,
    network: Hash64,
    actor: PalwBondKeyV2,
    claim: Digest,
    enrollment: Digest,
    round: u32,
) -> Result<Base, Error> {
    let (raw, _raw) = read_bounded(&outbox.join("challenge-base.borsh"), cap.min(LEGACY_BASE_BYTES))?;
    if !matches!(raw.len() as u64, BASE_BYTES | LEGACY_BASE_BYTES) {
        return Err("checker challenge base fixed extent differs".into());
    }
    // All fields, including the optional old A2, are fixed. No nested Vec/map
    // allocation can be induced by this local metadata decode.
    let base: Base = borsh::from_slice(&raw).map_err(|e| e.to_string())?;
    if base.version != 4
        || base.network != network
        || base.actor != actor
        || base.claim != claim
        || base.enrollment != enrollment
        || base.round != round
        || base.anchor.root == Hash64::default()
    {
        return Err("checker challenge base belongs to another actor/network/job".into());
    }
    if let Some(legacy) = base.legacy {
        if native_anchor(network, actor, Some(&legacy), None).map_err(str::to_string)? != base.anchor {
            return Err("checker legacy challenge bridge differs".into());
        }
    }
    Ok(base)
}

/// Two bounded passes avoid retaining all decoded action objects. Only a
/// contiguous reconstruction that equals the CURRENT own native prefix marks
/// earlier challenge slots observed. A reorg selects a shorter prefix again.
pub(super) fn accepted_actions(
    outbox: &Path,
    cap: u64,
    route: &PalwKernelRouteStateV1,
    actor: PalwBondKeyV2,
    claim: Digest,
    enrollment: Digest,
    round: u32,
) -> Result<[bool; 64], Error> {
    let mut first = None;
    for slot in 0..64 {
        if let Some(object) = read_action(outbox, slot, cap)? {
            if challenge(&object, actor, claim)?.is_some() {
                first = Some(object);
                break;
            }
        }
    }
    if first.is_none() && !outbox.join("challenge-base.borsh").exists() {
        return Ok([false; 64]);
    }
    ensure_base(outbox, cap, route, actor, claim, enrollment, round, first.as_deref())?;
    let network = Hash64::from_bytes(route.header.policy.network_domain);
    let base = read_base(outbox, cap, network, actor, claim, enrollment, round)?;
    let (native, _, _) = current(route, actor)?;
    let mut expected = base.anchor;
    let mut seen = [false; 64];
    let mut accepted = [false; 64];
    let mut matched = base.legacy.is_none() && expected == native;
    let mut legacy = base.legacy;
    for slot in 0..64 {
        let Some(object) = read_action(outbox, slot, cap)? else { continue };
        let Some(action) = challenge(&object, actor, claim)? else { continue };
        if let Some(old) = legacy.take() {
            if !old.matches(network, actor, action).map_err(str::to_string)? {
                return Err("checker first old challenge differs from saved legacy bridge".into());
            }
        } else {
            let statement = material_challenge_statement_v4(network, actor, action).map_err(str::to_string)?;
            expected = advance(network, actor, expected, statement).map_err(str::to_string)?;
        }
        seen[slot] = true;
        if expected == native {
            accepted = seen;
            matched = true;
        }
    }
    if !matched {
        return Err(Error::NativeView("checker contiguous challenge prefix not on own native view; exact WAL retained".into()));
    }
    Ok(accepted)
}

/// A repeated failure for an already applied exact generation/unit waits for
/// reacquisition rather than charging a second immutable carrier for that same
/// observation. This is not an answered/default verdict or material receipt.
pub(super) fn has_accepted_unit(
    outbox: &Path,
    cap: u64,
    actor: PalwBondKeyV2,
    claim: Digest,
    accepted: &[bool; 64],
    query: &PalwKernelMaterialUnitQueryV4,
) -> Result<bool, Error> {
    for (slot, &observed) in accepted.iter().enumerate() {
        if !observed {
            continue;
        }
        let object = read_action(outbox, slot, cap)?.ok_or("checker accepted subject WAL disappeared")?;
        let Some(Action::ChallengeCurrentV4 { root, consent, provider, dependency, stage, position, .. }) =
            challenge(&object, actor, claim)?
        else {
            return Err("checker accepted history slot is not its challenge".into());
        };
        if (*root, *consent, *provider, *dependency, *stage, *position)
            == (query.root, query.consent, query.provider, query.dependency, query.stage, query.position)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw_kernel_route_v1::palw_kernel_route_policy_v1,
        palw_material_challenge_checkpoint_v4::material_challenge_checkpoint_key_v4,
        palw_material_challenge_history_v4::material_challenge_history_key_v4,
        palw_material_release_recovery_v2::PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2 as TABLE,
    };
    const CAP: u64 = 1024 * 1024;
    const CLAIM: Digest = [3; 64];
    const ENROLLMENT: Digest = [4; 64];
    fn actor(n: u32) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([9; 64]), n))
    }
    fn route() -> PalwKernelRouteStateV1 {
        PalwKernelRouteStateV1::new(
            palw_kernel_route_policy_v1(Hash64::from_bytes([1; 64]), Hash64::from_bytes([2; 64])),
            None,
            Default::default(),
        )
    }
    fn action(position: u32) -> Action {
        Action::ChallengeCurrentV4 {
            root: Hash64::from_bytes(CLAIM),
            consent: Hash64::from_bytes([5; 64]),
            provider: actor(2),
            dependency: Hash64::from_bytes(CLAIM),
            stage: 255,
            position,
            valid_until_daa: 100,
        }
    }
    fn save(outbox: &Path, slot: usize, position: u32) -> PalwConsensusObjectV2 {
        let object = PalwConsensusObjectV2::MonetaryMaterialActionV1 {
            action: Box::new(action(position)),
            signer: actor(1),
            signature: vec![],
        };
        let (directory, path, _) = action_paths(outbox, slot);
        owned_dir(&directory).unwrap();
        publish_borsh(&path, &object, CAP).unwrap();
        object
    }
    fn applied(route: &mut PalwKernelRouteStateV1, position: u32, with_history: bool) {
        // Synthetic own-native records test the adapter only. They do not
        // execute signature, role, funding, typed-unit or court admission.
        let network = Hash64::from_bytes(route.header.policy.network_domain);
        let previous = material_challenge_checkpoint_from_route_v4(route, actor(1)).unwrap();
        let old_history = material_challenge_history_from_route_v4(route, actor(1)).unwrap();
        let checkpoint = PalwMaterialChallengeCheckpointV4::next(previous, network, actor(1), &action(position), 10).unwrap();
        let history = with_history.then(|| {
            PalwMaterialChallengeHistoryV4::next(network, actor(1), previous.as_ref(), old_history.as_ref(), &checkpoint).unwrap()
        });
        route.aux.insert((TABLE, material_challenge_checkpoint_key_v4(&actor(1)).to_vec()), checkpoint.encoded().unwrap().to_vec());
        if let Some(history) = history {
            route.aux.insert((TABLE, material_challenge_history_key_v4(&actor(1)).to_vec()), history.encoded().unwrap().to_vec());
        }
    }
    fn accepted(outbox: &Path, native: &PalwKernelRouteStateV1) -> [bool; 64] {
        accepted_actions(outbox, CAP, native, actor(1), CLAIM, ENROLLMENT, 0).unwrap()
    }

    #[test]
    fn cold_current_prefix_confirms_multiple_subjects_and_reorg_reselects_original_pending_wal() {
        let directory = tempfile::tempdir().unwrap();
        let outbox = directory.path();
        let zero = route();
        ensure_base(outbox, CAP, &zero, actor(1), CLAIM, ENROLLMENT, 0, None).unwrap();
        assert_eq!(accepted(outbox, &zero), [false; 64], "crash after baseline publication creates no acceptance");
        save(outbox, 0, 7);
        save(outbox, 1, 8);
        let mut first = route();
        applied(&mut first, 7, true);
        let mut both: PalwKernelRouteStateV1 = borsh::from_slice(&borsh::to_vec(&first).unwrap()).unwrap();
        applied(&mut both, 8, true);
        let seen = accepted(outbox, &both);
        assert!(seen[0] && seen[1] && !seen[2]);
        let rolled = accepted(outbox, &first);
        assert!(rolled[0] && !rolled[1]);
        assert_eq!(accepted(outbox, &zero), [false; 64]);
        let funding = [actor(10).0, actor(11).0, actor(12).0];
        let after = super::super::outbox::scan_outbox(outbox, &funding, CAP, &both, actor(1), CLAIM, ENROLLMENT, 0).unwrap();
        assert_eq!(after.slot, 2);
        assert!(!after.observed_terminal && after.object.is_none());
        let retry = super::super::outbox::scan_outbox(outbox, &funding, CAP, &first, actor(1), CLAIM, ENROLLMENT, 0).unwrap();
        assert_eq!(retry.slot, 1);
        assert!(!retry.observed_terminal && retry.object.is_some());
        applied(&mut both, 9, true);
        assert!(
            matches!(accepted_actions(outbox, CAP, &both, actor(1), CLAIM, ENROLLMENT, 0), Err(Error::NativeView(_))),
            "a same-key external action missing from the contiguous WAL is not this client's acceptance"
        );
    }

    #[test]
    fn repeated_unit_waits_for_reacquisition_and_never_rebinds_an_accepted_consent() {
        let directory = tempfile::tempdir().unwrap();
        let outbox = directory.path();
        let mut native = route();
        ensure_base(outbox, CAP, &native, actor(1), CLAIM, ENROLLMENT, 0, None).unwrap();
        save(outbox, 0, 7);
        applied(&mut native, 7, true);
        let seen = accepted(outbox, &native);
        let mut query = PalwKernelMaterialUnitQueryV4 {
            version: 4,
            root: Hash64::from_bytes(CLAIM),
            consent: Hash64::from_bytes([5; 64]),
            provider: actor(2),
            dependency: Hash64::from_bytes(CLAIM),
            stage: 255,
            position: 7,
        };
        assert!(has_accepted_unit(outbox, CAP, actor(1), CLAIM, &seen, &query).unwrap());
        query.consent = Hash64::from_bytes([6; 64]);
        assert!(!has_accepted_unit(outbox, CAP, actor(1), CLAIM, &seen, &query).unwrap());
        assert!(!has_accepted_unit(outbox, CAP, actor(1), CLAIM, &[false; 64], &query).unwrap());
    }

    #[test]
    fn old_single_checkpoint_bridge_and_cold_applied_history_preserve_distinct_domains() {
        let legacy_directory = tempfile::tempdir().unwrap();
        let outbox = legacy_directory.path();
        save(outbox, 0, 7);
        let mut native = route();
        applied(&mut native, 7, false);
        assert!(accepted(outbox, &native)[0], "only the exact latest old A2 can bootstrap the legacy bridge");
        save(outbox, 1, 8);
        applied(&mut native, 8, true);
        let migrated = accepted(outbox, &native);
        assert!(migrated[0] && migrated[1]);
        assert!(
            matches!(accepted_actions(outbox, CAP, &route(), actor(1), CLAIM, ENROLLMENT, 0), Err(Error::NativeView(_))),
            "rollback before a legacy baseline is unknown, never silently reanchored"
        );
        let cold_directory = tempfile::tempdir().unwrap();
        let cold = cold_directory.path();
        save(cold, 0, 7);
        let mut first = route();
        applied(&mut first, 7, true);
        assert!(accepted(cold, &first)[0], "exact latest A3 provides the one existing WAL's predecessor");
        assert_eq!(accepted(cold, &route()), [false; 64]);
    }

    #[test]
    fn fixed_base_rejects_another_job_and_trailing_data() {
        let directory = tempfile::tempdir().unwrap();
        let outbox = directory.path();
        let network = Hash64::from_bytes([1; 64]);
        ensure_base(outbox, CAP, &route(), actor(1), CLAIM, ENROLLMENT, 0, None).unwrap();
        let path = outbox.join("challenge-base.borsh");
        assert_eq!(fs::metadata(&path).unwrap().len(), BASE_BYTES);
        assert!(read_base(outbox, CAP, network, actor(1), CLAIM, [8; 64], 0).is_err());
        assert!(read_base(outbox, CAP, network, actor(1), CLAIM, ENROLLMENT, 1).is_err());
        let mut bytes = fs::read(&path).unwrap();
        bytes.push(0);
        fs::write(path, bytes).unwrap();
        assert!(read_base(outbox, CAP, network, actor(1), CLAIM, ENROLLMENT, 0).is_err());
    }

    #[test]
    fn missing_baseline_cannot_move_existing_signed_subject_after_a_foreign_latest_one() {
        let directory = tempfile::tempdir().unwrap();
        let outbox = directory.path();
        save(outbox, 0, 7);
        let mut native = route();
        applied(&mut native, 8, true);
        assert!(matches!(accepted_actions(outbox, CAP, &native, actor(1), CLAIM, ENROLLMENT, 0), Err(Error::NativeView(_))));
        assert!(!outbox.join("challenge-base.borsh").exists(), "unknown history must not publish a rebound baseline");
    }
}
