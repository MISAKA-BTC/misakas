//! Real floor executions for integration fixtures that issue positive verdicts.
//! The older carriage-only helper intentionally uses synthetic roots; those
//! roots must never receive an honest replay or Gate C attestation.
use super::*;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_TICKET_NONCE_BUCKET_LOG2, PalwAttemptChainFactsV1, PalwAttemptEnvelopeV2,
    PalwAttemptExecutionV1, attempt_id_v2, class_ticket_v3, execution_anchor_v3, palw_attempt_from_execution_v1, palw_attempt_job_v1,
};
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwMaterialVerdictV1};
use misaka_palw_base0::{backend::Base0Backend, classes::resolve_class_v1};

pub(in crate::pipeline::virtual_processor::tests) struct RealAttemptV1 {
    pub(in crate::pipeline::virtual_processor::tests) block: Block,
    pub(in crate::pipeline::virtual_processor::tests) claim: Hash64,
    pub(in crate::pipeline::virtual_processor::tests) material: Vec<u8>,
}

fn backend(chain: &T12Chain, artifact: Hash64) -> Base0Backend {
    Base0Backend::new(
        resolve_class_v1(&chain.bundle.court, chain.bundle.base_class_id, artifact, &[]).expect("registered floor resolves"),
    )
    .with_step_ladder_cap(chain.bundle.court.max_step_leaf_count())
    .with_prompt_ids_form(chain.config.params.palw_prompt_ids_form_v1())
}

impl T12Chain {
    /// Execute each class draw's actual job and sign only the winning execution.
    /// Uses ordinary producer facts, the shared producer/remote assembler and
    /// full block admission. Only the harness's existing network PoW skip applies.
    pub(in crate::pipeline::virtual_processor::tests) async fn real_attempt(&mut self, card: usize, step_ms: u64) -> RealAttemptV1 {
        self.ctx.simulated_time += step_ms;
        self.arm_clock();
        let mut template = self
            .ctx
            .consensus
            .build_block_template(
                MinerData::new(card_payout_spk(card), vec![]),
                Box::new(OnetimeTxSelector::new(vec![])),
                TemplateBuildMode::Standard,
            )
            .expect("normal attempt template");
        stamp_harness_time(&self.config.params, &mut template.block.header, self.ctx.simulated_time);
        let header = &template.block.header;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        let bond = self.bonds[card];
        let facts = self.ctx.consensus.palw_producer_facts_v2(self.bundle.base_class_id, Some(bond.0)).expect("producer facts");
        facts.ready_to_produce(&card_pubkey(card)).expect("normal registered producer is ready");
        let chain_facts = PalwAttemptChainFactsV1 {
            class_id: facts.class_id,
            artifact_root: facts.artifact_root,
            pwu: facts.pwu,
            min_trace_retention_daa: facts.min_trace_retention_daa,
            witness_chunks: facts.witness_chunks,
            operator_id: facts.bond.as_ref().expect("registered bond").operator_id,
        };
        let backend = backend(self, facts.artifact_root);
        let prefill = self.config.params.palw_prefill_draw_active_at(header.daa_score);
        let started = std::time::Instant::now();
        eprintln!(
            "[real-attempt] unchanged class target {:032x}, pwu {}, witness chunks {}",
            facts.class_target, facts.pwu, facts.witness_chunks
        );
        for bucket in 0..4_000_000u64 {
            assert!(
                started.elapsed() < std::time::Duration::from_secs(900),
                "real class draws exceeded the fixture's 900 s budget after {bucket} executions"
            );
            if bucket > 0 && bucket % 128 == 0 {
                eprintln!("[real-attempt] {bucket} actual executions, {:?} elapsed", started.elapsed());
            }
            let nonce = bucket << PALW_TICKET_NONCE_BUCKET_LOG2;
            let anchor = execution_anchor_v3(self.network_domain, pre_pow, facts.class_id, &bond.0, nonce);
            let (canonical, prompt) = backend.job_for_anchor(anchor).expect("anchor defines the job");
            let job = palw_attempt_job_v1(canonical, prefill);
            let outcome = backend.execute(&job, &prompt).expect("the producer executes its actual job");
            let execution = PalwAttemptExecutionV1 {
                trace_root: outcome.trace_root,
                output_root: outcome.output_root,
                execution_root: outcome.execution_root,
                trace_manifest_root: outcome.trace_manifest_root,
                trace_chunk_count: outcome.trace_chunk_count,
            };
            let attempt = palw_attempt_from_execution_v1(
                self.network_domain,
                pre_pow,
                header.timestamp,
                nonce,
                bond.0,
                card_pubkey(card),
                &chain_facts,
                &execution,
                header.daa_score,
            );
            if class_ticket_v3(&attempt, anchor) > facts.class_target {
                continue;
            }
            let claim = attempt_id_v2(&attempt);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &card_key(card).signing_key,
                claim.as_byte_slice(),
                PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
                [0x6Du8; 32],
            )
            .expect("winning attempt signs")
            .as_ref()
            .to_vec();
            template.block.header.nonce = nonce;
            template.block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
            template.block.header.finalize();
            self.nonce = self.nonce.max(nonce);
            self.attempts += 1;
            let block = self.insert_chain_block(template.block, "real executed floor attempt").await;
            eprintln!("[real-attempt] {} executions before accepted claim {claim}", bucket + 1);
            return RealAttemptV1 { block, claim, material: outcome.material };
        }
        panic!("no real execution won the unchanged class lottery");
    }
}

impl RealAttemptV1 {
    /// An honest controller derives the job from the actual accepted header,
    /// checks the capture against chain roots, and independently executes it.
    pub(in crate::pipeline::virtual_processor::tests) fn replay_transcript(&self, chain: &T12Chain) -> Result<[u8; 64], String> {
        let (_, state) = chain.tip_state();
        let claim = state.claim(&self.claim).ok_or("accepted claim absent")?;
        if claim.accepted_block != self.block.header.hash {
            return Err("claim is not from this header".into());
        }
        let artifact = state.class(&claim.class_id).ok_or("registered class absent")?.artifact_root;
        let backend = backend(chain, artifact);
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&self.block.header);
        let anchor = execution_anchor_v3(chain.network_domain, pre_pow, claim.class_id, &claim.bond.0, self.block.header.nonce);
        let prefill = chain.config.params.palw_prefill_draw_active_at(self.block.header.daa_score);
        let roots = PalwClaimRootsV1 {
            execution_root: claim.execution_root,
            trace_root: claim.trace_root,
            output_root: Some(claim.output_root),
            anchor,
            attempt_draw: Some(prefill),
            job_pin: None,
        };
        if backend.verify_material(&self.material, roots) != PalwMaterialVerdictV1::Matches {
            return Err("material does not answer the accepted claim".into());
        }
        let (canonical, prompt) = backend.job_for_anchor(anchor)?;
        let replay = backend.execute(&palw_attempt_job_v1(canonical, prefill), &prompt)?;
        if (replay.trace_root, replay.output_root, replay.execution_root)
            != (claim.trace_root, claim.output_root, claim.execution_root)
        {
            return Err("independent execution differs from the accepted claim".into());
        }
        Ok(misaka_palw_kernel::hash::object_id(
            b"misaka-test/real-claim-replay/v1",
            &(self.claim, anchor, replay.trace_root, replay.output_root, replay.execution_root),
        ))
    }
}

#[tokio::test]
async fn real_floor_attempt_is_admitted_and_replayed_before_any_positive_attestation() {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    chain.heartbeat_paying(MinerData::new(card_payout_spk(0), vec![]), config.params.target_time_per_block(), vec![]).await;
    let mut executed = chain.real_attempt(0, config.params.target_time_per_block()).await;
    assert_ne!(executed.replay_transcript(&chain).expect("independent honest replay"), [0; 64]);
    assert!(!executed.material.is_empty());
    executed.material.pop();
    assert!(executed.replay_transcript(&chain).is_err(), "truncated material cannot earn a positive verdict");
}
