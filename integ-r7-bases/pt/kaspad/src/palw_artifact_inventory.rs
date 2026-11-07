//! Pre-claim original tokenizer EOF inventory. A submission is never an acknowledgement;
//! every restart resumes the native accepted byte offset, not a local saved hash checkpoint.
use crate::palw_panel::court_outbox::CourtOutboxV1;
use kaspa_consensus_core::{
    Hash64,
    palw_class_semantics_v1::PalwClassSemanticsV1,
    palw_constraint_stream_carriage_v1::{PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1},
    palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
    palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1,
    palw_state_v2::PalwBondKeyV2,
    palw_tokenizer_artifact_inventory_v1::{
        PALW_ARTIFACT_INVENTORY_CONTEXT_V1, PALW_ARTIFACT_INVENTORY_PART_BYTES_V1, PalwTokenizerArtifactInventoryActionV1,
        PalwTokenizerArtifactInventoryCertificateV1, PalwTokenizerArtifactInventoryOfferV1, PalwTokenizerArtifactInventoryV1,
    },
};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    sync::Arc,
};

pub(super) const INVENTORY_ROUND_V1: u32 = u32::MAX - 16;
pub(super) const INVENTORY_HOST_BYTES_V1: u64 = 2 * 1024 * 1024;

/// These references must come from one authenticated native snapshot. In particular, a peer's
/// certificate or locally hashed file cannot populate `certificate`.
pub(super) struct InventorySnapshotV1<'a> {
    pub now_daa: u64,
    pub semantics: &'a PalwClassSemanticsV1,
    pub certificate: Option<&'a PalwTokenizerArtifactInventoryCertificateV1>,
    pub attempt: Option<&'a PalwTokenizerArtifactInventoryV1>,
}
pub(super) enum InventoryDecisionV1 {
    // Read by the restart regression; the service only needs readiness itself.
    #[cfg_attr(not(test), allow(dead_code))]
    Ready {
        actual_bytes: u64,
    },
    Wait,
    Move {
        movement: PalwConstraintStreamMoveV1,
        until_daa: u64,
    },
}
pub(super) struct ArtifactInventoryActorV1 {
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub tokenizer: Hash64,
    pub opener: PalwBondKeyV2,
    path: PathBuf,
    offered: Option<PalwTokenizerArtifactInventoryOfferV1>,
}
impl ArtifactInventoryActorV1 {
    pub fn new_v1(class_id: Hash64, artifact_root: Hash64, tokenizer: Hash64, opener: PalwBondKeyV2, path: PathBuf) -> Self {
        Self { class_id, artifact_root, tokenizer, opener, path, offered: None }
    }
    fn size_v1(&self) -> Result<u64, String> {
        File::open(&self.path).and_then(|file| file.metadata()).map(|meta| meta.len()).map_err(|e| e.to_string())
    }
    fn part_v1(&self, offset: u64, length: u64) -> Result<Vec<u8>, String> {
        let mut file = File::open(&self.path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() != length || offset >= length {
            return Err("inventory holding differs from original accepted operand length".into());
        }
        let n = (length - offset).min(PALW_ARTIFACT_INVENTORY_PART_BYTES_V1 as u64) as usize;
        let mut part = vec![0; n];
        file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        file.read_exact(&mut part).map_err(|e| e.to_string())?;
        Ok(part)
    }
    /// Caller holds the bounded host guard across native read, this file part and queued
    /// carrier. The local actual length is only a paid offer; ONLY native EOF yields Ready.
    pub fn next_v1<F>(
        &mut self,
        snapshot: InventorySnapshotV1<'_>,
        config: PalwStagedPublicCourtConfigV1,
        mut sign: F,
    ) -> Result<InventoryDecisionV1, String>
    where
        F: FnMut(&[u8], &[u8]) -> Result<Vec<u8>, String>,
    {
        config.check()?;
        let semantics = snapshot.semantics;
        if semantics.class_id != self.class_id
            || semantics.artifact_root != self.artifact_root
            || semantics.tokenizer_id != self.tokenizer
            || semantics.raw_tokenizer_bytes.is_none()
        {
            return Err("inventory source is not the qualified immutable registered class/tokenizer".into());
        }
        if let Some(cert) = snapshot.certificate {
            cert.check_v1()?;
            if cert.artifact_hash != self.tokenizer {
                return Err("inventory snapshot substitutes registered raw hash".into());
            }
            return Ok(InventoryDecisionV1::Ready { actual_bytes: cert.actual_bytes });
        }
        let (action, reserved_daa, seed_id, until_daa) = if let Some(row) = snapshot.attempt {
            if row.offer.opener != self.opener || row.offer.artifact_hash != self.tokenizer || snapshot.now_daa < row.opened_daa {
                return Err("inventory progress substitutes original literal paid opener or snapshot clock".into());
            }
            self.offered = Some(row.offer.clone());
            if snapshot.now_daa >= row.until_daa {
                return Ok(InventoryDecisionV1::Wait);
            }
            let action = if row.offset == row.offer.maximum_bytes {
                PalwTokenizerArtifactInventoryActionV1::Finish
            } else {
                PalwTokenizerArtifactInventoryActionV1::Part {
                    offset: row.offset,
                    bytes: self.part_v1(row.offset, row.offer.maximum_bytes)?,
                }
            };
            (PalwConstraintStreamActionV1::ArtifactInventoryMove(action), row.opened_daa, row.offer.id_v1(), row.until_daa)
        } else {
            if self.offered.as_ref().is_none_or(|offer| snapshot.now_daa >= offer.accept_until_daa) {
                let length = self.size_v1()?;
                PalwTokenizerArtifactInventoryV1::quote_v1(length, config)?;
                let mut offer = PalwTokenizerArtifactInventoryOfferV1 {
                    version: 1,
                    artifact_hash: self.tokenizer,
                    opener: self.opener,
                    maximum_bytes: length,
                    accept_until_daa: snapshot
                        .now_daa
                        .checked_add(config.acquisition_daa)
                        .ok_or("inventory offer admission clock overflow")?,
                    signature: Vec::new(),
                };
                offer.signature = sign(offer.id_v1().as_byte_slice(), PALW_ARTIFACT_INVENTORY_CONTEXT_V1)?;
                offer.check_v1()?;
                self.offered = Some(offer);
            }
            let offer = self.offered.as_ref().expect("installed bounded offer");
            (
                PalwConstraintStreamActionV1::ArtifactInventoryOpen { class_id: self.class_id, offer: offer.clone() },
                0,
                offer.id_v1(),
                offer.accept_until_daa,
            )
        };
        Ok(InventoryDecisionV1::Move {
            movement: PalwConstraintStreamMoveV1 {
                version: 1,
                claim: self.tokenizer,
                reserver: self.opener,
                reserved_daa,
                seed_id,
                phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                action,
                signature: Vec::new(),
            },
            until_daa,
        })
    }
}

#[derive(Default)]
pub(super) struct ArtifactInventoryBookV1 {
    scope: Option<InventoryScopeV1>,
}
struct InventoryScopeV1 {
    actor: ArtifactInventoryActorV1,
    body: Option<Hash64>,
    last_daa: u64,
    _guard: crate::palw_memory_ledger::PalwMemoryReservationV1,
}
impl super::PalwPanelService {
    /// Pre-execution gate. A registered length, an offered length or a submitted Finish is not
    /// a certificate. Historical unqualified semantics and dormant shipping remain unchanged.
    pub(super) fn artifact_inventory_ready_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        class: Hash64,
        artifact_root: Hash64,
        opener: PalwBondKeyV2,
    ) -> Result<bool, String> {
        let now = session.palw_v2_daa_score().ok_or("inventory readiness has no native DAA")?;
        if !self.consensus_config.params.palw_staged_public_court_v1_active_at(now) {
            return Ok(true);
        }
        // This getter returns only a fixed offer/continuation, not raw parts or journals.
        let _guard = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-artifact-inventory-ready",
                    class_id: class,
                    job: class,
                },
                65_536,
            )
            .map_err(|e| e.to_string())?;
        let Some(progress) = session.palw_tokenizer_artifact_inventory_v1(class, opener)? else {
            return Ok(true);
        };
        if progress.now_daa != now || progress.class_id != class || progress.semantics.artifact_root != artifact_root {
            return Err("inventory readiness changes original native class/snapshot".into());
        }
        let Some(cert) = progress.certificate else {
            return Ok(false);
        };
        cert.check_v1()?;
        if cert.artifact_hash != progress.semantics.tokenizer_id {
            return Err("inventory readiness substitutes registered hash".into());
        }
        Ok(true)
    }
    /// A single current producer/intake target. If the target changes, finish the already-paid
    /// original scope first; no admitted clock or public source is discarded to admit another.
    pub(super) async fn artifact_inventory_pump_v1(
        self: &Arc<Self>,
        session: &kaspa_consensusmanager::ConsensusProxy,
        opener: PalwBondKeyV2,
        class: Hash64,
        artifact_root: Hash64,
        book: &mut ArtifactInventoryBookV1,
        court_outbox: &mut CourtOutboxV1,
    ) -> Result<bool, String> {
        use kaspa_consensus_core::{
            palw_constraint_stream_carriage_v1::PALW_CONSTRAINT_STREAM_CONTEXT_V1, palw_state_v2::PalwConsensusObjectV2,
        };
        let now = session.palw_v2_daa_score().ok_or("inventory source has no native DAA")?;
        if !self.consensus_config.params.palw_staged_public_court_v1_active_at(now) {
            return Ok(true);
        }
        let config = PalwStagedPublicCourtConfigV1::network_v1();
        let read_class = book.scope.as_ref().map_or(class, |scope| scope.actor.class_id);
        let fresh_guard = if book.scope.is_none() {
            Some(
                crate::palw_memory_ledger::host_ledger_v1()
                    .reserve(
                        crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                            role: "public-artifact-inventory",
                            class_id: read_class,
                            job: read_class,
                        },
                        INVENTORY_HOST_BYTES_V1,
                    )
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        let Some(progress) = session.palw_tokenizer_artifact_inventory_v1(read_class, opener)? else {
            if let Some(scope) = book.scope.take() {
                let key = (scope.actor.tokenizer, INVENTORY_ROUND_V1, false);
                court_outbox.retain_pending(|(a, b, c, _)| (*a, *b, *c) != key);
                court_outbox.forget_due(&key);
                court_outbox.forget_submitted(&key);
            }
            return Ok(read_class == class);
        };
        if progress.class_id != read_class || progress.now_daa != now {
            return Err("inventory read changed native snapshot/class during producer planning".into());
        }
        if let Some(cert) = &progress.certificate {
            cert.check_v1()?;
            if cert.artifact_hash != progress.semantics.tokenizer_id {
                return Err("inventory native EOF differs from original registered tokenizer".into());
            }
            let key = (progress.semantics.tokenizer_id, INVENTORY_ROUND_V1, false);
            court_outbox.retain_pending(|(a, b, c, _)| (*a, *b, *c) != key);
            court_outbox.forget_due(&key);
            court_outbox.forget_submitted(&key);
            book.scope = None;
            return Ok(read_class == class && progress.semantics.artifact_root == artifact_root);
        }
        if book.scope.is_none() {
            if progress.semantics.artifact_root != artifact_root {
                return Err("inventory requested a different original registered artifact".into());
            }
            let path = self
                .backends()
                .held_tokenizer_path_or_chain_v1(class, artifact_root, |id| self.chain_carriage_v1(session, id))
                .ok_or("qualified tokenizer has no publicly registered local holding")?;
            book.scope = Some(InventoryScopeV1 {
                actor: ArtifactInventoryActorV1::new_v1(class, artifact_root, progress.semantics.tokenizer_id, opener, path),
                body: None,
                last_daa: now,
                _guard: fresh_guard.expect("reserved before native read"),
            });
        }
        let mut scope = book.scope.take().expect("installed exact source scope");
        let key = (scope.actor.tokenizer, INVENTORY_ROUND_V1, false);
        let keypair = self.keypair.clone();
        let result = tokio::task::spawn_blocking(move || {
            let result = (|| {
                let decision = scope.actor.next_v1(
                    InventorySnapshotV1 {
                        now_daa: progress.now_daa,
                        semantics: &progress.semantics,
                        certificate: progress.certificate.as_ref(),
                        attempt: progress.attempt.as_ref(),
                    },
                    config,
                    |message, context| {
                        let kp = keypair.as_ref().ok_or("inventory opener has no signing key")?;
                        libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, message, context, [0; 32])
                            .map(|sig| sig.as_ref().to_vec())
                            .map_err(|e| format!("inventory offer signature: {e:?}"))
                    },
                )?;
                let InventoryDecisionV1::Move { mut movement, until_daa } = decision else {
                    return Ok(None);
                };
                let message = movement.signing_message();
                let body = Hash64::from_bytes(
                    blake2b_simd::Params::new()
                        .hash_length(64)
                        .key(b"misaka-palw/node-inventory-move/v1")
                        .hash(&message)
                        .as_bytes()
                        .try_into()
                        .expect("64"),
                );
                let kp = keypair.as_ref().ok_or("inventory opener has no signing key")?;
                movement.signature =
                    libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, &message, PALW_CONSTRAINT_STREAM_CONTEXT_V1, [0; 32])
                        .map_err(|e| format!("inventory carrier signature: {e:?}"))?
                        .as_ref()
                        .to_vec();
                movement.check()?;
                let object = PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) };
                if borsh::object_length(&object).map_err(|e| e.to_string())? > 90_000 {
                    return Err("inventory carrier exceeds ordinary signed lane".into());
                }
                Ok::<_, String>(Some((object, until_daa, body)))
            })();
            (scope, result)
        })
        .await;
        let (returned, result) = match result {
            Ok(value) => value,
            Err(error) => {
                court_outbox.retain_pending(|(a, b, c, _)| (*a, *b, *c) != key);
                court_outbox.forget_due(&key);
                court_outbox.forget_submitted(&key);
                return Err(error.to_string());
            }
        };
        let mut scope = returned;
        let result = match result {
            Ok(result) => result,
            Err(why) => {
                book.scope = Some(scope);
                return Err(why);
            }
        };
        if let Some((object, until_daa, body)) = result {
            if scope.body != Some(body) || now < scope.last_daa {
                court_outbox.forget_submitted(&key);
            }
            court_outbox.retain_pending(|(a, b, c, old)| (*a, *b, *c) != key || *old == object);
            if !court_outbox.pending().iter().any(|(a, b, c, _)| (*a, *b, *c) == key)
                && court_outbox.submitted_at(&key).is_none_or(|at| now >= at.saturating_add(super::COURT_MOVE_REPLAN_DAA))
            {
                court_outbox.push_pending((key.0, key.1, key.2, object));
                court_outbox.set_due(key, until_daa);
            }
            scope.body = Some(body);
            scope.last_daa = now;
        } else {
            court_outbox.retain_pending(|(a, b, c, _)| (*a, *b, *c) != key);
            court_outbox.forget_due(&key);
        }
        book.scope = Some(scope);
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn semantics(hash: Hash64) -> PalwClassSemanticsV1 {
        PalwClassSemanticsV1 {
            version: 1,
            class_id: Hash64::from_u64_word(1),
            artifact_root: Hash64::from_u64_word(2),
            bound_from_daa: 0,
            model_profile_id: Hash64::default(),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(),
            tokenizer_id: hash,
            cu_ruleset_id: Hash64::default(),
            token_table_root: Some(Hash64::from_u64_word(3)),
            rendering_rule: Some(kaspa_consensus_core::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1),
            raw_tokenizer_bytes: Some(1),
            tokenizer_key_events: Some(1),
            prefix_state_relation: None,
        }
    }
    fn actor(path: PathBuf, s: &PalwClassSemanticsV1) -> ArtifactInventoryActorV1 {
        ArtifactInventoryActorV1::new_v1(
            s.class_id,
            s.artifact_root,
            s.tokenizer_id,
            PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_u64_word(4), 0)),
            path,
        )
    }
    fn next(
        actor: &mut ArtifactInventoryActorV1,
        s: &PalwClassSemanticsV1,
        row: Option<&PalwTokenizerArtifactInventoryV1>,
        cert: Option<&PalwTokenizerArtifactInventoryCertificateV1>,
        now: u64,
    ) -> InventoryDecisionV1 {
        actor
            .next_v1(
                InventorySnapshotV1 { now_daa: now, semantics: s, attempt: row, certificate: cert },
                PalwStagedPublicCourtConfigV1::network_v1(),
                |_, _| Ok(vec![0; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN]),
            )
            .unwrap()
    }
    #[test]
    fn source_actor_uses_actual_public_file_size_and_only_native_eof_ack() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tokenizer.json");
        let raw = vec![b' '; PALW_ARTIFACT_INVENTORY_PART_BYTES_V1 + 17];
        std::fs::write(&path, &raw).unwrap();
        let s = semantics(kaspa_consensus_core::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(&raw));
        let mut worker = actor(path.clone(), &s);
        let InventoryDecisionV1::Move { movement, until_daa } = next(&mut worker, &s, None, None, 10) else { panic!("no paid offer") };
        let PalwConstraintStreamActionV1::ArtifactInventoryOpen { offer, .. } = movement.action else { panic!("wrong control") };
        assert_eq!(offer.maximum_bytes, raw.len() as u64);
        assert_ne!(offer.maximum_bytes, s.raw_tokenizer_bytes.unwrap());
        assert_eq!(movement.reserved_daa, 0);
        assert_eq!(until_daa, offer.accept_until_daa);
        let mut row = PalwTokenizerArtifactInventoryV1::new_v1(offer, 11, PalwStagedPublicCourtConfigV1::network_v1()).unwrap();
        let original_until = row.until_daa;
        let InventoryDecisionV1::Move { movement, .. } = next(&mut worker, &s, Some(&row), None, 12) else {
            panic!("no first raw part")
        };
        let PalwConstraintStreamActionV1::ArtifactInventoryMove(action) = movement.action else { panic!("wrong raw control") };
        assert!(matches!(&action,PalwTokenizerArtifactInventoryActionV1::Part {offset:0,bytes} if bytes.len()==65_536));
        assert_eq!(movement.reserved_daa, row.opened_daa);
        row.apply_v1(&action, 12).unwrap();
        // No private cursor/checkpoint survives restart; resume exactly the accepted offset.
        let mut restarted = actor(path, &s);
        let InventoryDecisionV1::Move { movement, .. } = next(&mut restarted, &s, Some(&row), None, 13) else {
            panic!("no restarted part")
        };
        let PalwConstraintStreamActionV1::ArtifactInventoryMove(action) = movement.action else { panic!("wrong restarted control") };
        assert!(matches!(&action,PalwTokenizerArtifactInventoryActionV1::Part {offset:65_536,bytes} if bytes.len()==17));
        row.apply_v1(&action, 13).unwrap();
        let InventoryDecisionV1::Move { movement, .. } = next(&mut restarted, &s, Some(&row), None, 14) else {
            panic!("local EOF is not native ready")
        };
        let PalwConstraintStreamActionV1::ArtifactInventoryMove(action) = movement.action else { panic!("wrong EOF control") };
        assert_eq!(action, PalwTokenizerArtifactInventoryActionV1::Finish);
        let cert = row.apply_v1(&action, 14).unwrap().unwrap();
        assert!(
            matches!(next(&mut restarted,&s,None,Some(&cert),15),InventoryDecisionV1::Ready {actual_bytes} if actual_bytes==raw.len() as u64)
        );
        assert_eq!(row.until_daa, original_until);
    }
    #[test]
    fn expired_paid_inventory_never_resets_opened_clock_or_false_certifies_wrong_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tokenizer.json");
        std::fs::write(&path, b"different").unwrap();
        let s = semantics(kaspa_consensus_core::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(b"committed"));
        let mut worker = actor(path, &s);
        let InventoryDecisionV1::Move { movement, .. } = next(&mut worker, &s, None, None, 10) else { panic!("offer") };
        let PalwConstraintStreamActionV1::ArtifactInventoryOpen { offer, .. } = movement.action else { panic!("offer action") };
        let mut row = PalwTokenizerArtifactInventoryV1::new_v1(offer, 11, PalwStagedPublicCourtConfigV1::network_v1()).unwrap();
        let InventoryDecisionV1::Move { movement, .. } = next(&mut worker, &s, Some(&row), None, 12) else { panic!("raw part") };
        let PalwConstraintStreamActionV1::ArtifactInventoryMove(action) = movement.action else { panic!("raw action") };
        row.apply_v1(&action, 12).unwrap();
        assert!(row.apply_v1(&PalwTokenizerArtifactInventoryActionV1::Finish, 13).is_err());
        assert!(row.certificate.is_none());
        assert!(matches!(next(&mut worker, &s, Some(&row), None, row.until_daa), InventoryDecisionV1::Wait));
        assert_eq!(worker.offered.as_ref().unwrap().id_v1(), row.offer.id_v1());
    }
}
