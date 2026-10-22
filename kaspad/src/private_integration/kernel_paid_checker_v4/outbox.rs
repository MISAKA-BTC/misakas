//! Bounded append-only checker actions. Slot zero preserves the original WAL
//! names. Each later action has its own object and exact funded transaction;
//! an observed native demand never licenses overwriting an earlier signature.
use super::*;
use kaspa_consensus_core::tx::Transaction;

#[derive(Debug)]
pub(super) enum CheckerOutboxErrorV4 {
    /// Own native state is unavailable/undecodable/local-resource limited. No
    /// send, new action or positive verdict follows; poll the same WAL again.
    NativeView(String),
    Outbox(String),
}
impl From<String> for CheckerOutboxErrorV4 {
    fn from(error: String) -> Self {
        Self::Outbox(error)
    }
}
impl From<&str> for CheckerOutboxErrorV4 {
    fn from(error: &str) -> Self {
        Self::Outbox(error.into())
    }
}

pub(super) struct CheckerOutboxV4 {
    pub slot: usize,
    pub object: Option<NativeRetainedV1<PalwConsensusObjectV2>>,
    pub transaction: Option<Transaction>,
    pub transaction_lease: Option<NativeDemandLeaseV1>,
    pub used_funding: [bool; 64],
    pub accepted_challenges: [bool; 64],
    /// Receipts and negative terminal claims remain monitored. Applied demands
    /// and finalized challenge prefixes allow fresh checks, never approval.
    pub observed_terminal: bool,
    /// Applied but not finalized: neither resend nor fund a successor.
    pub awaiting_finality: bool,
    pub exhausted: bool,
    pub(super) _metadata: Option<NativeDemandLeaseV1>,
}

pub(super) fn action_paths(outbox: &Path, slot: usize) -> (PathBuf, PathBuf, PathBuf) {
    let directory = if slot == 0 { outbox.to_path_buf() } else { outbox.join(format!("action-{slot:02}")) };
    let object = directory.join("receipt.borsh");
    let transaction = directory.join("transaction.borsh");
    (directory, object, transaction)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Observation {
    Pending,
    DemandPresent,
    AwaitFinality,
    TerminalPresent,
    ClaimDecided,
}

fn observation(
    route: &PalwKernelRouteStateV1,
    object: &PalwConsensusObjectV2,
    actor: PalwBondKeyV2,
    base: Digest,
    enrollment: Digest,
    round: u32,
    challenge_accepted: bool,
    challenge_finalized: bool,
) -> Result<Observation, CheckerOutboxErrorV4> {
    if let PalwConsensusObjectV2::MonetaryMaterialActionV1 { action, signer, .. } = object {
        let kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialActionV1::ChallengeCurrentV4 {
            root, dependency, ..
        } = &**action
        else {
            return Err("checker saved monetary subject is not a generation-bound challenge".into());
        };
        if *signer != actor || *root != Hash64::from_bytes(base) || *dependency != Hash64::from_bytes(base) {
            return Err("checker saved monetary subject actor/claim differs".into());
        }
        return Ok(if challenge_finalized {
            Observation::DemandPresent
        } else if challenge_accepted {
            Observation::AwaitFinality
        } else {
            Observation::Pending
        });
    }
    let decoded =
        if matches!(object, PalwConsensusObjectV2::KernelRouteV1 { .. }) { Some(saved_kernel_object(object, actor)?) } else { None };
    if let Some(decoded) = &decoded {
        receipt_namespace_matches_v6(route, decoded, &enrollment).map_err(CheckerOutboxErrorV4::NativeView)?;
        if let KernelRouteObjectV1::PaidClaimV2 {
            object:
                PaidClaimObjectV2::Receipt { receipt }
                | PaidClaimObjectV2::RoundReceipt { receipt }
                | PaidClaimObjectV2::ReceiptApprovedBootstrapV6 { receipt },
        } = &**decoded
        {
            if receipt.enrollment != enrollment || receipt.round != round || receipt.signer != palw_kernel_bond_id_v1(&actor) {
                return Err("checker receipt outbox enrollment/round/actor differs".into());
            }
        }
        if let KernelRouteObjectV1::FileDemand { claim, demander, .. } = &**decoded {
            if *claim != base || *demander != palw_kernel_bond_id_v1(&actor) {
                return Err("checker demand outbox claim/actor differs".into());
            }
            let ledger = route.ledger().map_err(CheckerOutboxErrorV4::NativeView)?;
            if ledger.claims.get(&base).is_some_and(|row| !row.holds_job()) {
                // A canonical negative terminal claim moots these demands:
                // convicted, unavailable or timed out. Unavailability is not
                // arithmetic guilt. No fee or positive approval is inferred.
                return Ok(Observation::ClaimDecided);
            }
        }
    }
    if court_action_accepted(route, object, actor, base).map_err(CheckerOutboxErrorV4::NativeView)? {
        return match object {
            PalwConsensusObjectV2::KernelRouteV1 { .. }
                if matches!(&*saved_kernel_object(object, actor)?, KernelRouteObjectV1::FileDemand { .. }) =>
            {
                Ok(Observation::DemandPresent)
            }
            PalwConsensusObjectV2::KernelRouteV1 { .. }
                if matches!(&*saved_kernel_object(object, actor)?, KernelRouteObjectV1::FileProof { .. }) =>
            {
                Ok(Observation::ClaimDecided)
            }
            _ => Ok(Observation::TerminalPresent),
        };
    }
    if let PalwConsensusObjectV2::KernelRouteV1 { .. } = object {
        let decoded = saved_kernel_object(object, actor)?;
        if let KernelRouteObjectV1::PaidClaimV2 {
            object:
                PaidClaimObjectV2::Receipt { receipt }
                | PaidClaimObjectV2::RoundReceipt { receipt }
                | PaidClaimObjectV2::ReceiptApprovedBootstrapV6 { receipt },
        } = &*decoded
        {
            if receipt.signer != palw_kernel_bond_id_v1(&actor) {
                return Err("checker receipt outbox actor differs".into());
            }
            let authority = route.paid_claim_enrollment_v3(&receipt.enrollment).map_err(CheckerOutboxErrorV4::NativeView)?;
            if authority.enrollment.base_claim != base {
                return Err("checker receipt outbox claim differs".into());
            }
            if receipt_accepted(route, &authority, receipt.round, receipt.signer).map_err(CheckerOutboxErrorV4::NativeView)? {
                return Ok(Observation::TerminalPresent);
            }
        }
    }
    Ok(Observation::Pending)
}

/// A cursor is derived anew from one own-native snapshot on every poll; it is
/// never persisted as an acceptance marker. Reorg can therefore select an
/// earlier exact WAL even after later actions were published. The maximum
/// actions is the explicit, distinct initial funding list (at most 64), with
/// no change discovery, automatic top-up or unbounded action history.
pub(super) fn scan_outbox(
    outbox: &Path,
    funding: &[TransactionOutpoint],
    cap: u64,
    route: &PalwKernelRouteStateV1,
    actor: PalwBondKeyV2,
    base: Digest,
    enrollment: Digest,
    round: u32,
    finalized: Option<&kaspa_consensus_core::palw_material_challenge_history_v4::PalwMaterialChallengeFinalizedViewV4>,
) -> Result<CheckerOutboxV4, CheckerOutboxErrorV4> {
    if funding.is_empty() || funding.len() > 64 {
        return Err("checker action history requires one to 64 explicit funding inputs".into());
    }
    misaka_palw_remote::checker_outbox_legacy_v4::refuse_legacy_demand_archive_v4(outbox)?;
    // Two retained records (the first pending plus the current scan item),
    // bounded path construction and fixed funding membership. Raw/decoded
    // object and transaction owners are reserved by their own decoders.
    let metadata = (outbox.as_os_str().len() as u64)
        .checked_add(128)
        .and_then(|n| n.checked_mul(16))
        .and_then(|n| n.checked_add(2 * std::mem::size_of::<CheckerOutboxV4>() as u64))
        .ok_or("checker action scan metadata overflow")?;
    let mut result = CheckerOutboxV4 {
        slot: funding.len(),
        object: None,
        transaction: None,
        transaction_lease: None,
        used_funding: [false; 64],
        accepted_challenges: [false; 64],
        observed_terminal: false,
        awaiting_finality: false,
        exhausted: true,
        _metadata: reserve_native_requests_v1(metadata).map_err(|e| e.to_string())?,
    };
    let challenges = super::challenge_history::accepted_actions(outbox, cap, route, actor, base, enrollment, round, finalized)?;
    result.accepted_challenges = challenges.applied;
    let mut first_gap = None;
    let mut terminal_seen = false;
    for slot in 0..64 {
        let (directory, object_path, tx_path) = action_paths(outbox, slot);
        if directory.exists() {
            owned_dir(&directory)?;
        }
        if !object_path.exists() {
            if tx_path.exists() {
                return Err("checker transaction WAL lacks its signed action; history retained".into());
            }
            if slot < funding.len() {
                first_gap.get_or_insert(slot);
            }
            continue;
        }
        if slot >= funding.len() {
            return Err("checker stored action exceeds configured funding/action budget; history retained".into());
        }
        if first_gap.is_some() || terminal_seen {
            return Err("checker action WAL has a gap or follows a terminal action; history retained".into());
        }
        let (raw, _raw) = read_bounded(&object_path, cap)?;
        preflight_native_value_v1::<PalwConsensusObjectV2>(&raw[..], raw.len() as u64).map_err(|e| e.to_string())?;
        let object =
            retain_native_value_decode_v1(&raw, || borsh::from_slice::<PalwConsensusObjectV2>(&raw)).map_err(|e| e.to_string())?;
        let (transaction, lease) = if tx_path.exists() {
            let (raw, _raw) = read_bounded(&tx_path, cap)?;
            let (tx, lease) = super::super::recovery_ack_carrier_v4::decode_carrier(&raw, &object)?;
            let point = tx.inputs[0].previous_outpoint;
            let i = funding.iter().position(|p| *p == point).ok_or("checker saved carrier uses unconfigured funding")?;
            if result.used_funding[i] {
                return Err("checker immutable carriers reuse a funding input; history retained".into());
            }
            result.used_funding[i] = true;
            (Some(tx), lease)
        } else {
            (None, None)
        };
        let status =
            observation(route, &object, actor, base, enrollment, round, result.accepted_challenges[slot], challenges.finalized[slot])?;
        // A receipt/proof is the last action. Demands and exact applied
        // challenge subjects may have successors, including when reorg makes
        // an older one pending again. Validate the immutable history shape.
        let is_demand = match &*object {
            PalwConsensusObjectV2::KernelRouteV1 { .. } => {
                matches!(&*saved_kernel_object(&object, actor)?, KernelRouteObjectV1::FileDemand { .. })
            }
            _ => false,
        };
        let is_challenge = matches!(&*object, PalwConsensusObjectV2::MonetaryMaterialActionV1 { action, .. }
            if matches!(&**action, kaspa_consensus_core::palw_monetary_material_v1::PalwMonetaryMaterialActionV1::ChallengeCurrentV4 { .. }));
        terminal_seen |= !is_demand && !is_challenge;
        if status == Observation::ClaimDecided || (result.object.is_none() && status != Observation::DemandPresent) {
            result.slot = slot;
            result.object = Some(object);
            result.transaction = transaction;
            result.transaction_lease = lease;
            result.observed_terminal = matches!(status, Observation::TerminalPresent | Observation::ClaimDecided);
            result.awaiting_finality = status == Observation::AwaitFinality;
            result.exhausted = false;
        }
    }
    if result.object.is_none() {
        if let Some(slot) = first_gap {
            result.slot = slot;
            result.exhausted = false;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        palw_kernel_route_v1::palw_kernel_route_policy_v1,
        tx::{TransactionId, TransactionInput, TransactionOutput},
    };
    use misaka_palw_kernel::{
        ledger::DemandRowV1,
        public::ServedPositionV1,
        rows::{LedgerScalarsV1, TABLE_DEMANDS_V1, TABLE_SERVED_V1},
    };

    fn scan_outbox(
        outbox: &Path,
        funding: &[TransactionOutpoint],
        cap: u64,
        route: &PalwKernelRouteStateV1,
        actor: PalwBondKeyV2,
        base: Digest,
        enrollment: Digest,
        round: u32,
    ) -> Result<CheckerOutboxV4, CheckerOutboxErrorV4> {
        let finalized = super::super::challenge_history::synthetic_finalized(route, actor);
        super::scan_outbox(outbox, funding, cap, route, actor, base, enrollment, round, Some(&finalized))
    }

    // These are adapter/WAL controls, not signed native admission tests. The
    // synthetic ledger rows let the source exercise canonical-view replacement
    // and rollback without falsely claiming a court fold was executed.
    const BASE: Digest = [3; 64];
    const ENROLLMENT: Digest = [4; 64];
    const CAP: u64 = 1024 * 1024;
    fn point(index: u32) -> TransactionOutpoint {
        TransactionOutpoint::new(TransactionId::from_u64_word(15), index)
    }
    fn actor() -> PalwBondKeyV2 {
        PalwBondKeyV2(point(99))
    }
    fn route(demanders: &[(u32, PalwBondKeyV2)], served: &[u32]) -> PalwKernelRouteStateV1 {
        let mut route = PalwKernelRouteStateV1::new(
            palw_kernel_route_policy_v1(Hash64::from_bytes([1; 64]), Hash64::from_bytes([2; 64])),
            None,
            LedgerScalarsV1::default(),
        );
        for &(position, actor) in demanders {
            route.rows.insert(
                (TABLE_DEMANDS_V1, borsh::to_vec(&(BASE, 0u8, position)).unwrap()),
                borsh::to_vec(&DemandRowV1 {
                    demanders: vec![(palw_kernel_bond_id_v1(&actor), 1)],
                    filed_daa: 1,
                    deadline_daa: 10,
                    last: None,
                })
                .unwrap(),
            );
        }
        for &position in served {
            route.rows.insert(
                (TABLE_SERVED_V1, borsh::to_vec(&(BASE, 0u8, position)).unwrap()),
                borsh::to_vec(&ServedPositionV1 { values: vec![], inputs: vec![] }).unwrap(),
            );
        }
        route
    }
    fn save(outbox: &Path, slot: usize, position: u32) -> PalwConsensusObjectV2 {
        let object = PalwConsensusObjectV2::KernelRouteV1 {
            bytes: KernelRouteObjectV1::FileDemand { demander: palw_kernel_bond_id_v1(&actor()), claim: BASE, stage: 0, position }
                .encode(),
            signer: actor(),
            signature: vec![],
        };
        let (directory, path, _) = action_paths(outbox, slot);
        owned_dir(&directory).unwrap();
        publish_borsh(&path, &object, CAP).unwrap();
        object
    }
    fn scan(outbox: &Path, funding: &[TransactionOutpoint], route: &PalwKernelRouteStateV1) -> CheckerOutboxV4 {
        scan_outbox(outbox, funding, CAP, route, actor(), BASE, ENROLLMENT, 0).unwrap()
    }

    #[test]
    fn cold_history_advances_only_native_own_or_served_demands_and_rewinds_after_reorg() {
        let directory = tempfile::tempdir().unwrap();
        let outbox = directory.path();
        let funding = [point(1), point(2), point(3)];
        let first = save(outbox, 0, 7);
        save(outbox, 1, 9);
        let old_bytes = borsh::to_vec(&first).unwrap();
        assert_eq!(scan(outbox, &funding, &route(&[], &[])).slot, 0);
        assert_eq!(scan(outbox, &funding, &route(&[(7, actor())], &[])).slot, 1);
        assert_eq!(scan(outbox, &funding, &route(&[], &[7])).slot, 1, "served answer survives demand-row retirement");
        let complete = scan(outbox, &funding, &route(&[(7, actor()), (9, actor())], &[]));
        assert_eq!(complete.slot, 2);
        assert!(complete.object.is_none() && !complete.exhausted && !complete.observed_terminal);
        assert_eq!(scan(outbox, &funding, &route(&[(9, actor())], &[])).slot, 0, "reorg cannot trust a local completed cursor");
        assert_eq!(scan(outbox, &funding, &route(&[(7, PalwBondKeyV2(point(98)))], &[])).slot, 0);
        assert_eq!(fs::read(outbox.join("receipt.borsh")).unwrap(), old_bytes, "never replace an earlier action signature");
    }

    #[test]
    fn history_rejects_gaps_or_smaller_funding_authorization_and_reports_exhaustion() {
        let directory = tempfile::tempdir().unwrap();
        let outbox = directory.path();
        save(outbox, 1, 9);
        assert!(scan_outbox(outbox, &[point(1), point(2)], CAP, &route(&[], &[]), actor(), BASE, ENROLLMENT, 0).is_err());
        save(outbox, 0, 7);
        assert!(scan_outbox(outbox, &[point(1)], CAP, &route(&[], &[]), actor(), BASE, ENROLLMENT, 0).is_err());
        let all = scan(outbox, &[point(1), point(2)], &route(&[(7, actor()), (9, actor())], &[]));
        assert!(all.exhausted && all.object.is_none());
    }

    fn save_carrier(outbox: &Path, slot: usize, object: &PalwConsensusObjectV2, funding: TransactionOutpoint) -> Transaction {
        let tx = Transaction::new(
            kaspa_consensus_core::constants::TX_VERSION,
            vec![TransactionInput::new(funding, vec![], 0, 0)],
            vec![TransactionOutput::new(1, kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&[1; 64]))],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            borsh::to_vec(&(kaspa_consensus_core::palw_lifecycle_objects_v2::PALW_LIFECYCLE_TX_VERSION_V2, object)).unwrap(),
        );
        publish_borsh(&action_paths(outbox, slot).2, &tx, CAP).unwrap();
        tx
    }

    #[test]
    fn cold_scan_retains_exact_transaction_and_rejects_funding_reuse_across_actions() {
        let directory = tempfile::tempdir().unwrap();
        let outbox = directory.path();
        let funding = [point(1), point(2)];
        let first = save(outbox, 0, 7);
        let tx = save_carrier(outbox, 0, &first, funding[0]);
        let first_scan = scan(outbox, &funding, &route(&[], &[]));
        assert_eq!(first_scan.transaction.as_ref().unwrap().id(), tx.id());
        assert!(first_scan.used_funding[0] && !first_scan.used_funding[1]);
        let accepted = scan(outbox, &funding, &route(&[(7, actor())], &[]));
        assert_eq!(accepted.slot, 1);
        assert!(accepted.object.is_none() && accepted.used_funding[0] && !accepted.used_funding[1]);
        // Advancing to a successor keeps the old signature and funding owner.
        // A canonical rollback reselects exactly the old transaction, without
        // turning its input into fresh funding for another obligation.
        let rollback = scan(outbox, &funding, &route(&[], &[]));
        assert_eq!(rollback.slot, 0);
        assert_eq!(rollback.transaction.as_ref().unwrap().id(), tx.id());
        assert!(rollback.used_funding[0] && !rollback.used_funding[1]);
        assert_eq!(fs::read(action_paths(outbox, 0).1).unwrap(), borsh::to_vec(&first).unwrap());
        assert_eq!(fs::read(action_paths(outbox, 0).2).unwrap(), borsh::to_vec(&tx).unwrap());
        let second = save(outbox, 1, 9);
        save_carrier(outbox, 1, &second, funding[0]);
        assert!(scan_outbox(outbox, &funding, CAP, &route(&[], &[]), actor(), BASE, ENROLLMENT, 0).is_err());
    }

    #[test]
    fn legacy_moved_demands_never_become_unused_funding_or_a_fresh_action_slot() {
        let directory = tempfile::tempdir().unwrap();
        let outbox = directory.path();
        let object = save(outbox, 0, 7);
        let carrier = save_carrier(outbox, 0, &object, point(1));
        let (_, object_path, tx_path) = action_paths(outbox, 0);
        let archived_object = outbox.join("demand-00000.borsh");
        let archived_tx = outbox.join("demand-00000-transaction.borsh");
        fs::rename(object_path, &archived_object).unwrap();
        fs::rename(tx_path, &archived_tx).unwrap();
        for native in [route(&[], &[]), route(&[(7, actor())], &[])] {
            assert!(matches!(scan_outbox(outbox, &[point(1), point(2)], CAP, &native, actor(), BASE, ENROLLMENT, 0),
                Err(CheckerOutboxErrorV4::Outbox(_))));
        }
        assert_eq!(fs::read(archived_object).unwrap(), borsh::to_vec(&object).unwrap());
        assert_eq!(fs::read(archived_tx).unwrap(), borsh::to_vec(&carrier).unwrap());
    }

    #[test]
    fn canonical_material_obligation_is_monitored_but_cannot_hide_an_earlier_rolled_back_demand() {
        use kaspa_consensus_core::{
            palw_material_challenge_checkpoint_v4::{PalwMaterialChallengeCheckpointV4, material_challenge_checkpoint_key_v4},
            palw_material_release_recovery_v2::PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2,
            palw_monetary_material_v1::PalwMonetaryMaterialActionV1,
        };
        let directory = tempfile::tempdir().unwrap();
        let outbox = directory.path();
        let funding = [point(1), point(2)];
        save(outbox, 0, 7);
        let action = PalwMonetaryMaterialActionV1::ChallengeCurrentV4 {
            root: Hash64::from_bytes(BASE),
            consent: Hash64::from_bytes([5; 64]),
            provider: PalwBondKeyV2(point(88)),
            dependency: Hash64::from_bytes(BASE),
            stage: 255,
            position: 9,
            valid_until_daa: 100,
        };
        let mut native = route(&[(7, actor())], &[]);
        let checkpoint = PalwMaterialChallengeCheckpointV4::next(
            None,
            Hash64::from_bytes(native.header.policy.network_domain),
            actor(),
            &action,
            10,
        )
        .unwrap();
        let object = PalwConsensusObjectV2::MonetaryMaterialActionV1 { action: Box::new(action), signer: actor(), signature: vec![] };
        let (path, object_path, _) = action_paths(outbox, 1);
        owned_dir(&path).unwrap();
        publish_borsh(&object_path, &object, CAP).unwrap();
        native.aux.insert(
            (PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, material_challenge_checkpoint_key_v4(&actor()).to_vec()),
            checkpoint.encoded().unwrap().to_vec(),
        );
        let accepted = scan(outbox, &funding, &native);
        assert_eq!(accepted.slot, 2);
        assert!(accepted.exhausted && accepted.accepted_challenges[1] && !accepted.observed_terminal);
        // Native ledger cache, if any, must be dropped when changing fixture
        // rows, just as a restored/updated own route rebuilds its projection.
        let mut rollback = route(&[], &[]);
        rollback.aux = native.aux.clone();
        let earlier = scan(outbox, &funding, &rollback);
        assert_eq!(earlier.slot, 0);
        assert!(!earlier.observed_terminal);
        assert!(
            matches!(
                scan_outbox(outbox, &funding, CAP, &route(&[(7, actor())], &[]), actor(), BASE, ENROLLMENT, 0),
                Err(CheckerOutboxErrorV4::NativeView(_))
            ),
            "a local persisted bridge is not native acceptance after its canonical prefix disappears"
        );
    }

    #[test]
    fn saved_receipt_for_another_enrollment_or_round_is_not_this_jobs_completion() {
        let receipt = PaidClaimReceiptV2 {
            version: 2,
            enrollment: ENROLLMENT,
            verification_class: [5; 64],
            assignment: [6; 64],
            source_context: [7; 64],
            sample_seed: [8; 64],
            evidence_root: [9; 64],
            transcript_root: [10; 64],
            signer: palw_kernel_bond_id_v1(&actor()),
            signed_daa: 1,
            complete: true,
            round: 0,
            message_root: [0; 64],
        };
        let object = PalwConsensusObjectV2::KernelRouteV1 {
            bytes: KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::RoundReceipt { receipt } }.encode(),
            signer: actor(),
            signature: vec![],
        };
        assert!(matches!(
            observation(&route(&[], &[]), &object, actor(), BASE, [11; 64], 0, false, false),
            Err(CheckerOutboxErrorV4::Outbox(_))
        ));
        assert!(matches!(
            observation(&route(&[], &[]), &object, actor(), BASE, ENROLLMENT, 1, false, false),
            Err(CheckerOutboxErrorV4::Outbox(_))
        ));
        assert!(
            matches!(
                observation(&route(&[], &[]), &object, actor(), BASE, ENROLLMENT, 0, false, false),
                Err(CheckerOutboxErrorV4::NativeView(_))
            ),
            "own enrollment absence is unknown, never a local success marker or permanent worker stop"
        );
    }

    #[test]
    fn canonical_negative_claim_moots_old_demand_but_rollback_reselects_exact_wal() {
        use misaka_palw_kernel::{
            evidence::{EvidenceHeaderV1, SuiteParamsV1, VerificationEvidenceV1},
            job::KernelClaimV1,
            ledger::{ClaimBodyV1, ClaimRowV1},
            lifecycle::{ClaimLifecycleV1, ClaimStateV1, LifecyclePolicyV1},
            rows::TABLE_CLAIMS_V1,
        };
        let directory = tempfile::tempdir().unwrap();
        let outbox = directory.path();
        let funding = [point(1), point(2)];
        save(outbox, 0, 7);
        save(outbox, 1, 9);
        let mut row = ClaimRowV1 {
            producer: [12; 64],
            class_binding_id: [13; 64],
            job_id: [14; 64],
            body: ClaimBodyV1::Program {
                claim: KernelClaimV1 { job_id: [14; 64], producer_bond: [12; 64], generated: vec![], evidence_root: [15; 64] },
                evidence: VerificationEvidenceV1 {
                    version: 1,
                    header: EvidenceHeaderV1 {
                        network_domain: [1; 64],
                        ruleset_digest: [2; 64],
                        class_binding_id: [13; 64],
                        program_root: [16; 64],
                        artifact_root: [17; 64],
                        plan_root: [18; 64],
                    },
                    job_input_root: [19; 64],
                    positions: 0,
                    initial_state_root: [20; 64],
                    final_state_root: [21; 64],
                    trace_root: [22; 64],
                    output_root: [23; 64],
                    segments: vec![],
                    suite: SuiteParamsV1 {
                        descriptor_digest: [24; 64],
                        checker_suite_id: 0,
                        challenge_policy_id: 0,
                        repetitions: 0,
                        target_bits: 0,
                        field_bits: 0,
                    },
                },
                commitments: vec![],
            },
            committed_daa: 0,
            life: ClaimLifecycleV1::new(LifecyclePolicyV1 { check_window_daa: 1, challenge_window_daa: 1 }),
            reserved: 0,
            liability_until: None,
            convicted: true,
            rewarded: false,
            sealed_daa: 0,
        };
        let mut native = route(&[], &[]);
        native.rows.insert((TABLE_CLAIMS_V1, borsh::to_vec(&BASE).unwrap()), borsh::to_vec(&row).unwrap());
        assert!(scan(outbox, &funding, &native).observed_terminal, "conviction must not rehearse already-mooted demands");
        row.convicted = false;
        row.life.state = ClaimStateV1::Unavailable { daa: 1, producer_defaulted: false };
        let mut unavailable = route(&[], &[]);
        unavailable.rows.insert((TABLE_CLAIMS_V1, borsh::to_vec(&BASE).unwrap()), borsh::to_vec(&row).unwrap());
        assert!(scan(outbox, &funding, &unavailable).observed_terminal, "unavailability holds without implying arithmetic guilt");
        let restored = scan(outbox, &funding, &route(&[], &[]));
        assert_eq!(restored.slot, 0);
        assert!(!restored.observed_terminal, "negative claim observation is not a durable local completion flag");
    }
}
