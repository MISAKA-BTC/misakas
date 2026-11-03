#![cfg(feature = "positive-panel")]
//! Private A4 duty fixture, with actual A2 provider fallback and A3 cold assembly.
#[path = "../../tir/exec/tests/positive_panel_r1_common/mod.rs"]
mod common;

use common::*;
use kaspa_consensus_core::{
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_ids_opening_v1, prompt_token_ids_commitment_v1},
    palw_tir_step_v1::PalwTirStepSpaceV1,
};
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION,
    identity::*,
    messages::*,
    ports::{MinerSource, NodeChecker, PanelBuilder},
};
use misaka_palw_remote::{miner::positive_v2::*, positive_panel_sources::*};
use misaka_palw_tir_exec::node::{
    TirClassRunnerV1, TirParamOpenerV1,
    positive_panel::{PanelAdmission, TirPanelBuilder},
};
use std::{
    cell::Cell,
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

fn budget() -> AggregateBudget {
    AggregateBudget {
        rounds: 8,
        unique_source_bytes: 1 << 20,
        transmitted_bytes: 1 << 22,
        additional_work: 1000,
        retained_bytes: 1 << 20,
        retention_until: 100,
        fee: 1,
        witness_attempts: 2,
    }
}
fn cache_limits() -> PanelSourceLimitsR1 {
    PanelSourceLimitsR1 {
        max_units: 16,
        max_unit_bytes: 4096,
        max_total_bytes: 1 << 20,
        max_class_bytes: 1 << 20,
        max_container_bytes: 1 << 20,
    }
}
fn contract(result: &mut MinerResultCommit) -> (MinerServiceContract, [u8; SIGNATURE_BYTES]) {
    let contract = MinerServiceContract {
        version: CONTRACT_VERSION,
        revision: CONTRACT_REVISION,
        case: result.case_id(),
        source_catalog: d(40),
        provider_delegations: d(41),
        budget: budget(),
        funding_consent: d(42),
        deadline: 90,
    };
    result.service_contract = contract.id();
    let pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([61; 32]);
    let signature =
        *libcrux_ml_dsa::ml_dsa_87::sign(&pair.signing_key, &result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    (contract, signature)
}
struct Provider {
    units: BTreeMap<(SourceKind, Digest, u64), StoredUnitV2>,
    reads: Cell<u64>,
}
impl StoredProviderV2 for Provider {
    fn unit(&self, kind: SourceKind, source: Digest, index: u64) -> Result<&StoredUnitV2, String> {
        self.reads.set(self.reads.get() + 1);
        self.units.get(&(kind, source, index)).ok_or_else(|| "missing stored unit".into())
    }
}
struct Duty {
    request: SourceRequest,
    result: ResultId,
    bounds: DutyBoundsV2,
    clock: Cell<u64>,
    revoked: Cell<bool>,
    complete: Cell<bool>,
}
impl CanonicalDataDutyV2 for Duty {
    fn request(&self) -> &SourceRequest {
        &self.request
    }
    fn authorize(&self, result: &MinerResultCommit) -> Result<DutyBoundsV2, MinerErrorV2> {
        if self.revoked.get() {
            return Err(MinerErrorV2::Closed);
        }
        if result.id() != self.result {
            return Err(MinerErrorV2::Binding);
        }
        Ok(DutyBoundsV2 { now: self.clock.get(), ..self.bounds })
    }
    fn status(&self, _index: u64) -> Result<MinerDataStatus, MinerErrorV2> {
        Ok(if self.complete.get() { MinerDataStatus::Complete } else { MinerDataStatus::Pending })
    }
}
fn duty(cache: &PanelSourceCacheR1, result: &MinerResultCommit, kind: SourceKind) -> Duty {
    Duty {
        request: cache.next_missing_request(kind, 16).unwrap().unwrap(),
        result: result.id(),
        bounds: DutyBoundsV2 {
            source_units: cache.units(kind),
            max_unit_bytes: 4096,
            work_per_read: 1,
            budget: budget(),
            now: 1,
            cutoff: 90,
        },
        clock: Cell::new(1),
        revoked: Cell::new(false),
        complete: Cell::new(false),
    }
}
struct TempPath(PathBuf);
impl TempPath {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(std::env::temp_dir().join(format!(
            "misaka-a3-r1-sources-{}-{}.palwtir",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn actual_a2_fallback_sources_rebuild_cold_panel_artifact_and_replay() {
    with_fixture(false, |f, _, inventory| {
        let expected_job = f.job.clone();
        let expected_execution = expected_job.context_hash();
        assert_eq!(expected_job.tokenizer_id, kaspa_consensus_core::Hash64::default());
        assert_ne!(f.class.tokenizer_id, expected_job.tokenizer_id);
        assert_eq!(f.result.tokenizer, digest(expected_job.tokenizer_id));
        assert_eq!(f.rooted.expected_execution, digest(expected_execution));
        let mut result = f.result.clone();
        let (contract, signature) = contract(&mut result);
        let mut cache = PanelSourceCacheR1::new(
            &f.rooted,
            result.clone(),
            contract,
            f.class.clone(),
            PalwPromptIdsFormV1::Flat,
            &f.key,
            &signature,
            cache_limits(),
            1,
        )
        .unwrap();
        let temp = TempPath::new();
        assert!(matches!(cache.assemble_artifact(&temp.0), Err(PanelSourceErrorR1::Incomplete)));
        assert!(!temp.0.exists());
        assert_eq!(cache.prompt(), Err(PanelSourceErrorR1::Incomplete));
        let mut good = Provider { units: BTreeMap::new(), reads: Cell::new(0) };
        for index in 0..cache.units(SourceKind::ArtifactInventory) {
            good.units.insert(
                (SourceKind::ArtifactInventory, result.artifact, index),
                StoredUnitV2::Artifact(inventory.param_opening(index as u32).unwrap()),
            );
        }
        good.units.insert((SourceKind::Input, result.input, 0), StoredUnitV2::FlatInput(PROMPT.to_vec()));
        let mut bad = Provider { units: good.units.clone(), reads: Cell::new(0) };
        for unit in bad.units.values_mut() {
            if let StoredUnitV2::Artifact(opening) = unit {
                opening.operand.bytes[0] ^= 1;
            }
        }
        let providers: [&dyn StoredProviderV2; 2] = [&bad, &good];
        let mut responder = MinerResponderV2::<Duty>::new(
            &f.rooted,
            result.clone(),
            contract,
            1,
            &f.key,
            &signature,
            &providers,
            ProcessLimitsV2 {
                max_units_per_request: 16,
                max_tracked_units: 16,
                max_provider_reads: 64,
                max_status_checks: 64,
                max_prepared_bytes: 1 << 22,
            },
        )
        .unwrap();
        let artifact_duty = duty(&cache, &result, SourceKind::ArtifactInventory);
        assert_eq!(cache.acquire(&mut responder, &artifact_duty).unwrap(), cache.units(SourceKind::ArtifactInventory));
        assert!(good.reads.get() > 0 && bad.reads.get() > 0);
        assert_eq!(artifact_duty.status(0).unwrap(), MinerDataStatus::Pending);
        let input_duty = duty(&cache, &result, SourceKind::Input);
        assert_eq!(cache.acquire(&mut responder, &input_duty).unwrap(), 1);
        assert_eq!(cache.prompt().unwrap(), PROMPT);
        let retained = cache.retained_bytes();
        // Repeated delivered preparation does not duplicate retained cache bytes.
        cache.acquire(&mut responder, &artifact_duty).unwrap();
        assert_eq!(cache.retained_bytes(), retained);
        let mut replacement = PanelSourceCacheR1::new(
            &f.rooted,
            result.clone(),
            contract,
            f.class.clone(),
            PalwPromptIdsFormV1::Flat,
            &f.key,
            &signature,
            cache_limits(),
            1,
        )
        .unwrap();
        let meter = responder.meter();
        for (kind, index, unit) in cache.authenticated_entries() {
            replacement.import_authenticated(kind, index, unit.clone()).unwrap();
        }
        assert_eq!(responder.meter(), meter);
        assert!(replacement.next_missing_request(SourceKind::ArtifactInventory, 16).unwrap().is_none());
        // Model a canonical A4 receipt, independently of the Panel's cache
        // import/ACK. A completed duty still requires bounded status lookups,
        // while it must not fetch or prepare those source units again.
        artifact_duty.complete.set(true);
        let reads = (good.reads.get(), bad.reads.get());
        let before_complete_lookup = responder.meter();
        assert_eq!(replacement.acquire(&mut responder, &artifact_duty).unwrap(), 0);
        let after_complete_lookup = responder.meter();
        assert_eq!((good.reads.get(), bad.reads.get()), reads);
        let expected_status_checks =
            before_complete_lookup.status_checks.checked_add(artifact_duty.request.end - artifact_duty.request.first).unwrap();
        assert_eq!(after_complete_lookup, PreparationMeterV2 { status_checks: expected_status_checks, ..before_complete_lookup });
        assert_eq!(replacement.retained_bytes(), retained);
        let artifact = replacement.assemble_artifact(&temp.0).unwrap();
        assert_eq!(artifact.inventory_root().unwrap().0, f.artifact);
        assert_eq!(artifact.class().unwrap(), f.class);
        let prompt = replacement.prompt().unwrap();
        let admitted = PanelAdmission::authenticate(
            &result,
            f.rooted,
            &f.class,
            &f.job,
            &prompt,
            PalwPromptIdsFormV1::Flat,
            &f.key,
            &signature,
            panel_limits(),
        )
        .unwrap();
        let space = PalwTirStepSpaceV1::new(&f.class).unwrap();
        let cold_runner = TirClassRunnerV1::new(&space, artifact.plan(), artifact.params(), f.job.shape_profile_id).unwrap();
        let builder = TirPanelBuilder::new(&cold_runner, &artifact);
        let capture = builder.replay_and_capture(&admitted).unwrap();
        assert_eq!(capture.generated(), EXPECTED);
        assert_eq!(capture.binding().job_context, expected_job);
        assert_eq!(f.job.context_hash(), expected_execution);
        // The SAME reconstructed candidate crosses the real A1 NodeChecker
        // port. No Node-side model or inference executor is supplied here.
        let plan = capture.admission().plan();
        let witness = builder.commit(&capture, &plan, witness_author(), 0).unwrap();
        let panel_pair = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([62; 32]);
        let panel_signature =
            *libcrux_ml_dsa::ml_dsa_87::sign(&panel_pair.signing_key, &witness.id().0.0, WITNESS_SIGN_CONTEXT, [0; 32])
                .unwrap()
                .as_ref();
        let mut limits = check_limits();
        limits.max_part_bytes = FRAME_CAP;
        let prepared = misaka_palw_tir_exec::node::positive_contract::PreparedCheck::authenticate(
            &f.rooted,
            &f.class,
            f.artifact,
            &f.job,
            PalwPromptIdsFormV1::Flat,
            limits,
            result.clone(),
            &f.key,
            &signature,
        )
        .unwrap();
        assert_eq!(prepared.descriptor(), plan);
        let context = prepared
            .authenticate_witness(
                witness.clone(),
                capture.binding().clone(),
                capture.generated().to_vec(),
                panel_pair.verification_key.as_ref(),
                &panel_signature,
            )
            .unwrap();
        let checker = misaka_palw_tir_exec::node::positive_contract::TirNodeChecker;
        let mut progress = context.start().unwrap();
        let parts = builder.prepare_parts(&capture).unwrap();
        let signed_part_cap = FRAME_CAP.checked_add(SIGNATURE_BYTES as u64).unwrap();
        let signed_total_cap = signed_part_cap.checked_mul(plan.mandatory_relations).unwrap();
        let mut signed_bytes = 0u64;
        let mut checked_parts = 0u64;
        for index in 0..plan.mandatory_relations {
            let part = parts.build_part(CoverageCursor { plan: plan.plan, next_relation: index }).unwrap();
            let bytes = part.frame(&witness, &plan).unwrap().encode(FRAME_CAP).unwrap();
            let frame = misaka_palw_positive_contract::wire::EvidenceFrame::decode(&bytes, FRAME_CAP).unwrap();
            let part_signature =
                *libcrux_ml_dsa::ml_dsa_87::sign(&panel_pair.signing_key, &frame.id().0.0, EVIDENCE_SIGN_CONTEXT, [0; 32])
                    .unwrap()
                    .as_ref();
            // Authorship of each exact part is checked before any Node cursor
            // mutation; an unsigned relay cannot blame altered bytes on Panel.
            let authenticated =
                frame.authenticate(&witness, panel_pair.verification_key.as_ref(), &part_signature, signed_part_cap).unwrap();
            assert_eq!(authenticated.author(), witness.author);
            signed_bytes = signed_bytes.checked_add(authenticated.frame().signed_wire_bytes()).unwrap();
            assert!(signed_bytes <= signed_total_cap);
            checker.check_part(&context, &mut progress, authenticated.frame()).unwrap();
            checked_parts += 1;
        }
        assert_eq!(checked_parts, plan.mandatory_relations);
        assert!(signed_bytes >= checked_parts * SIGNATURE_BYTES as u64);
        let terminal = checker.finish(&context, progress).unwrap();
        assert_eq!(terminal.outcome(), misaka_palw_tir_exec::node::positive_check::CheckOutcome::Matched);
        assert_eq!(f.job, expected_job);
    });
}

#[test]
fn source_cache_cannot_replace_registered_tokenizer_with_canonical_job_marker() {
    with_fixture(false, |f, _, _| {
        let mut result = f.result.clone();
        let (terms, signature) = contract(&mut result);
        let mut false_class = f.class.clone();
        false_class.tokenizer_id = f.job.tokenizer_id;
        assert_ne!(false_class.class_id(&f.artifact), f.job.shape_profile_id);
        assert!(
            PanelSourceCacheR1::new(
                &f.rooted,
                result,
                terms,
                false_class,
                PalwPromptIdsFormV1::Flat,
                &f.key,
                &signature,
                cache_limits(),
                1,
            )
            .is_err()
        );
    });
}

#[test]
fn altered_g14_units_and_foreign_request_cannot_populate_cache() {
    with_fixture(false, |f, _, inventory| {
        let mut result = f.result.clone();
        let (contract, signature) = contract(&mut result);
        let mut cache = PanelSourceCacheR1::new(
            &f.rooted,
            result.clone(),
            contract,
            f.class.clone(),
            PalwPromptIdsFormV1::Flat,
            &f.key,
            &signature,
            cache_limits(),
            1,
        )
        .unwrap();
        let mut opening = inventory.param_opening(0).unwrap();
        opening.operand.bytes[0] ^= 1;
        assert_eq!(
            cache.import_authenticated(SourceKind::ArtifactInventory, 0, StoredUnitV2::Artifact(opening)),
            Err(PanelSourceErrorR1::Binding)
        );
        assert_eq!(
            cache.import_authenticated(SourceKind::Input, 0, StoredUnitV2::FlatInput(vec![1])),
            Err(PanelSourceErrorR1::Binding)
        );
        assert_eq!(cache.retained_bytes(), 0);
        let providers: [&dyn StoredProviderV2; 0] = [];
        let mut responder = MinerResponderV2::<Duty>::new(
            &f.rooted,
            result.clone(),
            contract,
            1,
            &f.key,
            &signature,
            &providers,
            ProcessLimitsV2 {
                max_units_per_request: 16,
                max_tracked_units: 16,
                max_provider_reads: 64,
                max_status_checks: 64,
                max_prepared_bytes: 1 << 22,
            },
        )
        .unwrap();
        let mut wrong = duty(&cache, &result, SourceKind::Input);
        wrong.request.source = d(99);
        assert_eq!(cache.acquire(&mut responder, &wrong), Err(PanelSourceErrorR1::Binding));
        assert_eq!(responder.meter().provider_reads, 0);
        assert_eq!(responder.meter().status_checks, 0);
    });
}

#[test]
fn merkle_prompt_units_reconstruct_original_prompt_and_reject_substitution() {
    with_fixture(false, |f, _, _| {
        let prompt = vec![0; 65];
        let mut class = f.class.clone();
        class.layout.max_context = 128;
        let mut job = f.job.clone();
        job.shape_profile_id = class.class_id(&f.artifact);
        job.prompt_token_ids_hash = prompt_token_ids_commitment_v1(PalwPromptIdsFormV1::MerkleV1, &prompt).unwrap();
        job.declared_prefill_tokens = prompt.len() as u32;
        job.max_context_tokens = 128;
        let mut result = f.result.clone();
        result.registered_class = digest(job.shape_profile_id);
        result.input = digest(job.prompt_token_ids_hash);
        result.execution = digest(job.context_hash());
        result.prefill = prompt.len() as u32;
        result.max_context = job.max_context_tokens;
        let mut rooted = f.rooted;
        rooted.expected_input = result.input;
        rooted.expected_execution = result.execution;
        rooted.registered_class = result.registered_class;
        let (contract, signature) = contract(&mut result);
        let mut cache = PanelSourceCacheR1::new(
            &rooted,
            result,
            contract,
            class,
            PalwPromptIdsFormV1::MerkleV1,
            &f.key,
            &signature,
            cache_limits(),
            1,
        )
        .unwrap();
        assert_eq!(cache.units(SourceKind::Input), 3);
        for index in (0..3).rev() {
            let opening = prompt_ids_opening_v1(&prompt, index * 32).unwrap();
            cache.import_authenticated(SourceKind::Input, index as u64, StoredUnitV2::Input(opening)).unwrap();
        }
        assert_eq!(cache.prompt().unwrap(), prompt);
        let mut altered = prompt_ids_opening_v1(&prompt, 0).unwrap();
        altered.tile_ids[0] = 1;
        let retained = cache.retained_bytes();
        assert_eq!(cache.import_authenticated(SourceKind::Input, 0, StoredUnitV2::Input(altered)), Err(PanelSourceErrorR1::Binding));
        assert_eq!(cache.retained_bytes(), retained);
    });
}

#[test]
fn current_duty_expiry_or_branch_revocation_after_io_discards_prepared_units() {
    struct MovingSource {
        revoke: bool,
    }
    impl MinerSource for MovingSource {
        type AuthenticatedDuty = Duty;
        type G14Response = PreparedDataV2;
        type Error = MinerErrorV2;
        fn respond(&mut self, duty: &Duty) -> Result<PreparedDataV2, MinerErrorV2> {
            if self.revoke {
                duty.revoked.set(true);
            } else {
                duty.clock.set(90);
            }
            Ok(PreparedDataV2 { request: duty.request, units: vec![(0, StoredUnitV2::FlatInput(PROMPT.to_vec()))] })
        }
    }
    with_fixture(false, |f, _, _| {
        let mut result = f.result.clone();
        let (contract, signature) = contract(&mut result);
        for revoke in [false, true] {
            let mut cache = PanelSourceCacheR1::new(
                &f.rooted,
                result.clone(),
                contract,
                f.class.clone(),
                PalwPromptIdsFormV1::Flat,
                &f.key,
                &signature,
                cache_limits(),
                1,
            )
            .unwrap();
            let duty = duty(&cache, &result, SourceKind::Input);
            // This is an in-memory A4 capability fixture, never a peer flag or
            // a native admission implementation. A current capability re-read
            // must notice the state change after the source's blocking work.
            assert!(cache.acquire(&mut MovingSource { revoke }, &duty).is_err());
            assert_eq!(cache.retained_bytes(), 0);
            assert!(cache.get(SourceKind::Input, 0).is_none());
            assert_eq!(cache.prompt(), Err(PanelSourceErrorR1::Incomplete));
        }
    });
}

#[test]
fn retained_registered_class_discards_caller_spare_capacity() {
    with_fixture(false, |f, _, _| {
        let mut result = f.result.clone();
        let (contract, signature) = contract(&mut result);
        let mut oversized = f.class.clone();
        oversized.program.reserve_exact(1 << 18);
        oversized.layout.commit_tiles.reserve_exact(1 << 18);
        assert!(oversized.program.capacity() > 1 << 18);
        assert!(oversized.layout.commit_tiles.capacity() > 1 << 18);
        let cache = PanelSourceCacheR1::new(
            &f.rooted,
            result,
            contract,
            oversized,
            PalwPromptIdsFormV1::Flat,
            &f.key,
            &signature,
            cache_limits(),
            1,
        )
        .unwrap();
        assert_eq!(cache.registered_class(), &f.class);
        assert_eq!(cache.registered_class().program.capacity(), cache.registered_class().program.len());
        assert_eq!(cache.registered_class().layout.commit_tiles.capacity(), cache.registered_class().layout.commit_tiles.len());
    });
}

#[test]
fn missing_prompt_refuses_before_materializing_large_prefill() {
    with_fixture(false, |f, _, _| {
        let mut result = f.result.clone();
        // This source-only fixture deliberately has no input units. A large
        // signed length must return Incomplete before constructing its buffer.
        result.prefill = u32::MAX - 3;
        result.max_context = u32::MAX;
        let (contract, signature) = contract(&mut result);
        let limits = PanelSourceLimitsR1 { max_units: u32::MAX as u64, max_total_bytes: u64::from(u32::MAX) * 4, ..cache_limits() };
        let cache = PanelSourceCacheR1::new(
            &f.rooted,
            result,
            contract,
            f.class.clone(),
            PalwPromptIdsFormV1::Flat,
            &f.key,
            &signature,
            limits,
            1,
        )
        .unwrap();
        assert_eq!(cache.prompt(), Err(PanelSourceErrorR1::Incomplete));
        assert_eq!(cache.retained_bytes(), 0);
    });
}
