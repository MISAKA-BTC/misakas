//! **Lane G14-V: every covered violation of the legacy floor route, carried by the REAL public verifier service to a recorded
//! outcome** (V-node; harness-only keys/premine, skipped PoW, inert EVM, the dormant public-filer fence TEST-ARMED without its
//! validation — the shipping activation validator still refuses it, asserted by `fixture_config`).
//!
//! **The scenario of every case.** Card 0 produces a REAL floor execution of its attempt's job (its own backend on the registered
//! floor artifact) with ONE localized model fault injected through the producer's own commit path — a step tile of a chosen op kind
//! (MatMul element, normalization/rounding, nonlinearity, embedding gather, …; an isolated lie followed by honest values), a logits
//! lane over an honest step tree, a forged decode token, a replaced output root, a prompt/binding lie — every root re-derived so the
//! material is self-consistent. EVERY seat of the claim's panel signs `Valid`; the licence is real. ONE bond registered AFTER genesis
//! through a real `0x4b` carrier (maturity waited, collateral locked) runs kaspad's own `PalwPanelService` on a node STARTED AFTER
//! THE CLAIM (a fresh database that validated every public block). The service replays the claim's job on its OWN floor copy, reads
//! only consensus API reads and accepted blocks, reserves, demands, localizes (the G14-V authenticated subtree descent; past an
//! agreeing step tree, the logits/token localization) and files through its own reporter door — every object through the actual PQ
//! mempool, the production template and the real block fold. The producer answers what it owes from ITS capture (its obligation,
//! never the verifier's material). The outcome is read back from the chain.
use super::palw_service_public_e2e::{
    Chain, Funding, OUTSIDER, ServiceRun, card_payout_payload, card_payout_spk, card_pubkey, fixture_config, panel_config,
    pool_heartbeat, signed,
};
use super::*;
use kaspa_consensus::consensus::test_consensus::TestConsensusFactory;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::TemplateBuildMode;
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwDrillBendV1, PalwDrillFaultV1, PalwExecutionBackendV1};
use kaspa_consensus_core::palw_da_rcore_v1::{
    PalwDaAnswerV1, PalwDaUnitV1, palw_da_answer_object_v1, palw_da_held_answer_v1, palw_da_held_disclosure_from_capture_v1,
    palw_da_step_leaf_is_fused_v1,
};
use kaspa_consensus_core::palw_state_v2::{
    PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT, PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT, PalwClaimPhaseV2, PalwVoidReasonV2,
    palw_bond_registration_message_v2, palw_operator_possession_message_v1,
};
use kaspa_consensus_core::palw_step::PalwStepOpKindV1;
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;
use kaspa_core::task::tick::TickService;
use kaspa_database::{create_temp_db, prelude::ConnBuilder};
use kaspa_mining::AttestationMempoolPolicy;
use kaspa_mining::manager::{MiningManager, MiningManagerProxy};
use kaspa_mining::model::tx_query::TransactionQuery;
use kaspa_p2p_mining::rule_engine::MiningRuleEngine;
use misaka_palw_base0::backend::Base0Backend;
use std::collections::BTreeSet;
use std::time::Duration;

/// The producer of every case.
const PRODUCER: usize = 0;
/// The card whose attempt at a claim's anchor slot binds its panel.
const BINDER: usize = 7;

/// One injected lie (or none).
#[derive(Clone, Copy, Debug)]
enum Lie {
    Honest,
    /// One committed tile of the first node of this op kind past the run's first third moved by one lane — the execution honest
    /// before and after it (an isolated lie followed by correct values).
    Step(PalwStepOpKindV1),
    /// The same, at the run's FIRST leaf of the op kind.
    StepFirst(PalwStepOpKindV1),
    /// A drill fault through the producer's own commit path.
    Drill(PalwDrillFaultV1),
}

/// What a case expects the chain to record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    /// A conviction of the producer (kind 4 or a court fraud void), by this verifier's pursuit.
    Convicted,
    /// Nothing filed; the claim reaches `Final`.
    Honest,
    /// The lie leaves the producer no answer the chain accepts (the carrier gate refuses its own binding, which answers another
    /// job): the verifier's reserved demand ends in the objective DA default (`ProducerWithholding`, the producer sanctioned) — or a
    /// conviction, if a direct proof lands first.
    Withholding,
}

/// What the producer committed and keeps (ITS obligation).
struct Produced {
    name: &'static str,
    claim: Hash64,
    material: Vec<u8>,
    prompt: Vec<u32>,
    roots: PalwClaimRootsV1,
    binding: PalwStepBindingV2,
    lie_leaf: Option<u64>,
    expect: Expect,
}

fn floor_backend(chain: &Chain) -> Base0Backend {
    use misaka_palw_base0::classes::resolve_class_v1;
    let artifact_root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("the floor's pinned root");
    Base0Backend::new(resolve_class_v1(&chain.bundle.court, chain.bundle.base_class_id, artifact_root, &[]).expect("the floor"))
        .with_step_ladder_cap(chain.bundle.court.max_step_leaf_count())
        .with_prompt_ids_form(chain.config.params.palw_prompt_ids_form_v1())
}

/// The leaf of `op` a step lie moves: held in the capture, with canonical coordinates, not a fused site.
fn pick_leaf(material: &[u8], op: PalwStepOpKindV1, first: bool) -> Option<u64> {
    let (binding, tiles, ..) = misaka_palw_base0::produce::base0_material_decode_v1(material).ok()?;
    let held: BTreeSet<u64> = tiles.iter().map(|(i, _)| *i).collect();
    let n = binding.step_leaf_count;
    let start = if first { 0 } else { n / 3 };
    (start..n).find(|leaf| {
        held.contains(leaf)
            && !palw_da_step_leaf_is_fused_v1(&binding, *leaf)
            && kaspa_consensus_core::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, *leaf)
                .and_then(|coord| binding.shape_profile.resolve_node_slot(coord.node_slot))
                .is_some_and(|(node, _)| node.op_kind == op)
    })
}

/// The floor's op kinds and their leaf counts in one honest capture (the probe the cases choose from).
fn op_census(material: &[u8]) -> BTreeMap<String, u64> {
    let (binding, ..) = misaka_palw_base0::produce::base0_material_decode_v1(material).expect("a dense floor capture");
    let mut out = BTreeMap::new();
    for leaf in 0..binding.step_leaf_count {
        if let Some((node, _)) =
            kaspa_consensus_core::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, leaf)
                .and_then(|coord| binding.shape_profile.resolve_node_slot(coord.node_slot))
        {
            *out.entry(format!("{:?}", node.op_kind)).or_insert(0) += 1;
        }
    }
    out
}

/// **Card `card`'s attempt over a REAL floor execution**, honest or with `lie`, winning its class lottery the way a producer does
/// (another nonce bucket is another anchor, another job, another inference); inserted through the real pipeline.
async fn real_attempt(
    chain: &mut Chain,
    backend: &Base0Backend,
    card: usize,
    name: &'static str,
    lie: Lie,
    expect: Expect,
) -> Produced {
    use kaspa_consensus_core::hashing::header::pre_pow_hash_64;
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_VERSION, PALW_TICKET_NONCE_BUCKET_LOG2, PalwAttemptEnvelopeV2,
        PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2, class_ticket_v3, execution_anchor_v3, palw_attempt_job_v1,
    };
    let bond = chain.bonds[card];
    // The step layout is the class's and the job shape's (every attempt job has the same prefill/decode): the lie's leaf is chosen
    // once from an honest run, then each try runs only the lying execution.
    let mut chosen: Option<u64> = None;
    for attempt in 0u64..2_000 {
        chain.time += chain.config.params.target_time_per_block();
        chain.nonce += 1;
        let mut t = chain
            .tc
            .build_block_template(
                MinerData::new(card_payout_spk(card), vec![]),
                Box::new(super::palw_service_public_e2e::OnetimeTxSelector::new(vec![])),
                TemplateBuildMode::Standard,
            )
            .expect("a template");
        t.block.header.timestamp = chain.time;
        t.block.header.nonce = (0x47_0000 + chain.nonce) << PALW_TICKET_NONCE_BUCKET_LOG2;
        let facts = chain.tc.palw_producer_facts_v2(chain.bundle.base_class_id, Some(bond.0)).expect("the floor's facts");
        let header = &t.block.header;
        let prefill_draw = chain.config.params.palw_prefill_draw_active_at(header.daa_score);
        let pre_pow = pre_pow_hash_64(header);
        let anchor = execution_anchor_v3(chain.network_domain, pre_pow, facts.class_id, &bond.0, header.nonce);
        let (canonical, prompt) = backend.job_for_anchor(anchor).expect("the floor implies a job");
        let job = palw_attempt_job_v1(canonical, prefill_draw);
        let (run, lie_leaf) = match lie {
            Lie::Honest => (backend.execute(&job, &prompt).expect("the floor runs its job"), None),
            Lie::Step(op) | Lie::StepFirst(op) => {
                let leaf = *chosen.get_or_insert_with(|| {
                    let honest = backend.execute(&job, &prompt).expect("the floor runs its job");
                    pick_leaf(&honest.material, op, matches!(lie, Lie::StepFirst(_)))
                        .unwrap_or_else(|| panic!("the floor has an openable {op:?} leaf"))
                });
                (backend.execute_with_injected_fault(&job, &prompt, leaf).expect("the step drill runs"), Some(leaf))
            }
            Lie::Drill(fault) => {
                (backend.execute_with_drill_fault(&job, &prompt, fault).unwrap_or_else(|e| panic!("drill {fault:?}: {e}")), None)
            }
        };
        let bond_facts = facts.bond.as_ref().expect("a registered bond");
        let unsigned = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: chain.network_domain,
            challenge: challenge_v2(chain.network_domain, pre_pow, header.timestamp, header.nonce, facts.class_id, &bond.0),
            class_id: facts.class_id,
            executor_bond: bond.0,
            executor_pubkey: card_pubkey(card),
            operator_id: bond_facts.operator_id,
            artifact_root: facts.artifact_root,
            trace_root: run.trace_root,
            output_root: run.output_root,
            execution_root: run.execution_root,
            pwu: facts.pwu,
            trace_manifest_root: run.trace_manifest_root,
            trace_chunk_count: run.trace_chunk_count,
            trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        if class_ticket_v3(&unsigned, anchor) > facts.class_target {
            continue;
        }
        let claim = attempt_id_v2(&unsigned);
        let signature = signed(card, claim.as_byte_slice(), PALW_ATTEMPT_V2_MLDSA87_CONTEXT);
        t.block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt: unsigned, signature }.encode_wire();
        t.block.header.finalize();
        chain.insert(t.block).await;
        assert_eq!(chain.view(claim).claim_id, claim, "the chain opened the claim");
        eprintln!(
            "[g14v] {name}: card {card}'s attempt won on try {attempt}; claim {claim}; {} step leaves, lie leaf {lie_leaf:?}",
            misaka_palw_base0::produce::base0_material_decode_any_v1(&run.material).map(|r| r.binding().step_leaf_count).unwrap_or(0)
        );
        let roots = PalwClaimRootsV1 {
            execution_root: run.execution_root,
            trace_root: run.trace_root,
            anchor,
            attempt_draw: Some(prefill_draw),
            output_root: None,
            job_pin: None,
        };
        let prompt: Vec<u32> = prompt.iter().map(|id| u32::try_from(*id).expect("a u32 id")).collect();
        let binding = backend.disclose_trace_event(&run.material, u32::MAX, u8::MAX).expect("its binding").binding().clone();
        return Produced { name, claim, material: run.material, prompt, roots, binding, lie_leaf, expect };
    }
    panic!("the floor's class lottery is winnable");
}

/// The chain binds the claim's panel (another card's attempt at its anchor slot), and EVERY seat signs `Valid`.
async fn bind_and_license(chain: &mut Chain, backend: &Base0Backend, floats: &mut [Funding], claim: Hash64) {
    let slot = chain.view(claim).accepted_daa + chain.bundle.panel.anchor_delay();
    chain.advance(slot).await;
    // The binding attempt is a REAL honest floor execution of another card: the verifier replays it and files nothing.
    real_attempt(chain, backend, BINDER, "anchor", Lie::Honest, Expect::Honest).await;
    let duties = chain.tc.palw_seat_duties_v2(chain.bonds.clone()).into_iter().filter(|d| d.claim_id == claim).collect::<Vec<_>>();
    assert!(!duties.is_empty(), "the anchor binds a panel");
    let signed_daa = chain.daa();
    let receipts = duties
        .iter()
        .map(|d| {
            let card = chain.bonds.iter().position(|b| *b == d.seat_bond).unwrap();
            let message = palw_receipt_message_v2(chain.network_domain, claim, PalwReceiptVerdictV2::Valid, signed_daa);
            PalwSeatReceiptV2 {
                claim,
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: d.seat_bond,
                signed_daa,
                signature: signed(card, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
            }
        })
        .collect();
    let object = chain.tc.palw_v2_receipt_quorum_assemble(claim, receipts).expect("every colluding seat licenses");
    let card = chain.bonds.iter().position(|b| *b == duties[0].seat_bond).unwrap();
    chain.send(card, &object, &mut floats[card], &[]).await;
    assert!(matches!(chain.view(claim).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "licensed by every seat");
}

/// A public participant's bond: registered after genesis through the published door, maturity waited.
async fn register_outsider(chain: &mut Chain, floats: &mut [Funding]) -> PalwBondKeyV2 {
    let signed_bond = PalwBondKeyV2(TransactionOutpoint::new(Default::default(), 0));
    let public_key = card_pubkey(OUTSIDER);
    let payout = card_payout_payload(OUTSIDER);
    let classes = BTreeSet::new();
    let collateral = PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let message =
        palw_bond_registration_message_v2(chain.network_domain, &signed_bond, &public_key, &public_key, collateral, &payout, &classes);
    let possession = palw_operator_possession_message_v1(chain.network_domain, &signed_bond, &public_key, &public_key);
    let mut signature = signed(OUTSIDER, message.as_byte_slice(), PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT);
    signature.extend(signed(OUTSIDER, possession.as_byte_slice(), PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT));
    let registration = PalwConsensusObjectV2::BondRegistered {
        bond: signed_bond,
        pubkey: public_key.clone(),
        operator_pubkey: public_key.clone(),
        collateral,
        payout_payload: payout,
        capable_classes: classes,
        signature,
    };
    let tx = chain
        .send(OUTSIDER, &registration, &mut floats[OUTSIDER], &[TransactionOutput::new(collateral, card_payout_spk(OUTSIDER))])
        .await;
    let bond = PalwBondKeyV2(TransactionOutpoint::new(tx.id(), 0));
    assert_eq!(chain.tc.palw_bond_of_pubkey_v2(&public_key).unwrap().0, bond);
    assert!(!chain.bonds.contains(&bond), "not a genesis card");
    let maturity = chain.config.params.palw_bond_maturity.unwrap().window_daa;
    let target = chain.daa() + maturity + 1;
    chain.advance(target).await;
    bond
}

/// **The colluding producer answers every unit it owes** from its own capture (or keeps silent on `withhold`).
async fn producer_answers(
    chain: &mut Chain,
    floats: &mut [Funding],
    backend: &Base0Backend,
    produced: &[Produced],
    withhold: &dyn Fn(&PalwDaUnitV1) -> bool,
    refused: &mut BTreeSet<(Hash64, PalwDaUnitV1)>,
) -> usize {
    let duties = chain.tc.palw_disclosure_duties_v1(vec![chain.bonds[PRODUCER]]);
    let form = chain.config.params.palw_prompt_ids_form_v1();
    let mut sent = 0;
    for duty in duties.duties.iter() {
        let Some(p) = produced.iter().find(|p| p.claim == duty.claim_id) else { continue };
        if withhold(&duty.unit) || refused.contains(&(p.claim, duty.unit)) {
            continue;
        }
        let answer = match duty.unit {
            PalwDaUnitV1::Event { row, tile } => {
                PalwDaAnswerV1::Event(backend.disclose_trace_event(&p.material, row, tile).expect("the capture opens the event"))
            }
            PalwDaUnitV1::Held(missing) => {
                let work_leaves = chain.view(p.claim).work_leaves;
                let (binding, disclosure) = palw_da_held_disclosure_from_capture_v1(
                    backend,
                    &p.material,
                    &p.prompt,
                    p.roots,
                    work_leaves,
                    missing,
                    form,
                    || backend.disclose_trace_event(&p.material, u32::MAX, u8::MAX).map(|d| d.binding().clone()),
                )
                .unwrap_or_else(|e| panic!("the producer's capture answers {missing:?}: {e}"));
                palw_da_held_answer_v1(p.claim, missing, binding, disclosure)
            }
            PalwDaUnitV1::LegacyHeldV2(unit) => {
                // LG14-B's tag 158 from the producer's own retention (its node's DA worker builds the same).
                let (binding, answer) = crate::palw_legacy_held_v2::palw_legacy_held_capture_answer_v2(
                    backend,
                    &p.material,
                    &p.prompt,
                    p.roots,
                    form,
                    &unit,
                )
                .unwrap_or_else(|e| panic!("the producer's capture answers {unit:?}: {e}"));
                let object = crate::palw_legacy_held_v2::palw_legacy_held_answer_object_v2(
                    &chain.network_domain,
                    p.claim,
                    unit,
                    binding,
                    answer,
                    duty.discloser,
                    |message, context| Some(signed(PRODUCER, message, context)),
                )
                .unwrap_or_else(|e| panic!("the tag-158 builder: {e}"));
                if producer_send(chain, floats, &object).await {
                    sent += 1;
                } else {
                    refused.insert((p.claim, duty.unit));
                }
                continue;
            }
            other => panic!("a unit the legacy floor does not owe: {other:?}"),
        };
        let object = palw_da_answer_object_v1(
            &chain.network_domain,
            p.claim,
            duty.unit,
            answer,
            duty.discloser,
            chain.bundle.court.max_close_bytes(),
            |message, context| Some(signed(PRODUCER, message, context)),
        )
        .unwrap_or_else(|e| panic!("the node's builder builds the answer: {e}"));
        if producer_send(chain, floats, &object).await {
            sent += 1;
        } else {
            refused.insert((p.claim, duty.unit));
        }
    }
    sent
}

/// The producer's carrier through the node's mempool check and a block; `false` when the chain refuses it (nothing is sent). The
/// carrier is funded by the colluding cards' richest fee float (the answer object carries the producer's own signature; a float
/// worn thin by many answers would otherwise grow a storage mass the harness builder does not commit).
async fn producer_send(chain: &mut Chain, floats: &mut [Funding], object: &PalwConsensusObjectV2) -> bool {
    let payer = (0..OUTSIDER).max_by_key(|card| floats[*card].1.amount).expect("a colluding card");
    let funding = &mut floats[payer];
    let tx = match super::palw_carrier_fees::build_lifecycle_carrier_v2(
        &chain.config,
        super::palw_service_public_e2e::key(payer),
        object,
        funding.0,
        &funding.1,
        &[],
        None,
        chain.daa(),
    ) {
        Ok(tx) => tx,
        Err(why) => {
            eprintln!("[g14v] the producer cannot build its answer carrier: {why:?}");
            return false;
        }
    };
    if let Err(why) = chain.tc.validate_mempool_transaction(&mut MutableTransaction::from_tx(tx.clone()), &Default::default()) {
        eprintln!("[g14v] the chain refuses the producer's answer ({}): {why}", super::object_name(object));
        return false;
    }
    chain.heartbeat(vec![tx.clone()]).await;
    chain.heartbeat(vec![]).await;
    let index = tx.outputs.len() - 1;
    let output = &tx.outputs[index];
    *funding = (
        TransactionOutpoint::new(tx.id(), index as u32),
        UtxoEntry::new(output.value, output.script_public_key.clone(), chain.daa(), false),
    );
    true
}

/// The verifier's service on the fresh node, its mempool and flow context.
struct Verifier {
    running: ServiceRun,
    manager: Arc<ConsensusManager>,
    pool: MiningManagerProxy,
    _flow: Arc<FlowContext>,
    _dir: tempfile::TempDir,
    _meta: Box<dyn std::any::Any>,
}

fn start_verifier(chain: &Chain, bond: PalwBondKeyV2, funding: &Funding) -> Verifier {
    let manager = Arc::new(ConsensusManager::new(Arc::new(TestConsensusFactory::new(chain.tc.clone()))).unwrap());
    let config = Arc::new(chain.config.clone());
    let tick = Arc::new(TickService::new());
    let (meta_lifetime, meta_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (addresses, _) = kaspa_addressmanager::AddressManager::new(config.clone(), meta_db, tick.clone());
    let pool = MiningManagerProxy::new(Arc::new(MiningManager::new_with_extended_config(
        config.target_time_per_block(),
        false,
        config.max_block_mass,
        config.ram_scale,
        config.block_template_cache_lifetime,
        Default::default(),
        true,
        config.palw_model_market.is_some(),
        config.palw_activation_pool_fence().is_some(),
        None,
        AttestationMempoolPolicy::disabled(),
        kaspa_mining::mempool::config::palw_h1_carrier_priority_for(&config.params),
    )));
    let hub = kaspa_p2p_lib::Hub::new();
    let rules = Arc::new(MiningRuleEngine::new(
        manager.clone(),
        config.clone(),
        Default::default(),
        tick.clone(),
        hub.clone(),
        Default::default(),
        Arc::new(kaspa_core::chain_participation::ChainParticipationGate::new(true)),
    ));
    let flow = Arc::new(FlowContext::new(
        manager.clone(),
        addresses,
        config.clone(),
        pool.clone(),
        tick,
        chain.tc.notification_root(),
        hub,
        rules,
    ));
    let dir = tempfile::tempdir().unwrap();
    let service =
        Arc::new(PalwPanelService::new(panel_config(chain, dir.path(), bond, funding), manager.clone(), flow.clone(), config));
    assert_eq!(service.bond, Some(bond.0));
    let cloned = service.clone();
    let task = tokio::spawn(async move {
        cloned.start().await.unwrap();
    });
    Verifier {
        running: ServiceRun { service, task: Some(task) },
        manager,
        pool,
        _flow: flow,
        _dir: dir,
        _meta: Box::new(meta_lifetime),
    }
}

/// Whether the chain recorded the producer's conviction on `claim` (a retired claim's view is gone: `false` here, the caller keeps
/// the first observation).
fn convicted(chain: &Chain, claim: Hash64) -> bool {
    let Some(view) = chain.tc.palw_legacy_dispute_v1(claim) else { return false };
    view.executor_refuted
        || view.court_convicted
        || matches!(
            view.phase,
            PalwClaimPhaseV2::Voided {
                reason: PalwVoidReasonV2::CourtFraud | PalwVoidReasonV2::CourtHeldVerdict | PalwVoidReasonV2::CourtDefault,
                ..
            }
        )
}

/// Whether the chain recorded the objective DA default of `claim` (withholding, never a fraud verdict).
fn defaulted(chain: &Chain, claim: Hash64) -> bool {
    let Some(view) = chain.tc.palw_legacy_dispute_v1(claim) else { return false };
    view.da_defaulted || matches!(view.phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. })
}

/// **One world, several cases**: the outsider registers, each lying (or honest) claim is produced and licensed by every seat,
/// optionally left to reach `Final`, then a node started after the claims runs the actual service until every expected conviction
/// is on chain. Returns the per-case record lines.
async fn run_cases(cases: Vec<(&'static str, Lie, Expect)>, post_final: bool, budget_secs: u64) -> Vec<String> {
    run_cases_on(cases, post_final, false, budget_secs).await
}

/// [`run_cases`], optionally with LG14-B's dormant `palw_legacy_held_da_v2` TEST-ARMED as well (its own validation still refuses
/// it): the service then descends by tag-157/158 frontiers and files the tag-159 recompute.
async fn run_cases_on(cases: Vec<(&'static str, Lie, Expect)>, post_final: bool, held_v2: bool, budget_secs: u64) -> Vec<String> {
    kaspa_core::log::try_init_logger("warn,kaspad_lib::palw_panel=info");
    let (config, bundle, premine, mut floats) = fixture_config();
    let config = if held_v2 {
        let mut params = config.params.clone();
        params.palw_legacy_held_da_v2 = Some(kaspa_consensus_core::config::params::ForkActivation::new(0));
        assert!(params.validate_palw_legacy_held_da_v2().is_err(), "LG14-B's own validation still refuses its fence");
        Config::new(params)
    } else {
        config
    };
    let mut chain = Chain::new(config, bundle, &premine);
    chain.advance(2).await;
    let bond = register_outsider(&mut chain, &mut floats).await;
    let backend = floor_backend(&chain);
    let mut produced = Vec::new();
    for (name, lie, expect) in cases {
        let p = real_attempt(&mut chain, &backend, PRODUCER, name, lie, expect).await;
        bind_and_license(&mut chain, &backend, &mut floats, p.claim).await;
        produced.push(p);
    }
    if post_final {
        let cap = chain.daa() + 20_000;
        while !produced.iter().all(|p| matches!(chain.view(p.claim).phase, PalwClaimPhaseV2::Final { .. })) {
            assert!(chain.daa() < cap, "the licensed claims reach Final");
            chain.heartbeat(vec![]).await;
        }
        eprintln!("[g14v] every claim is Final at DAA {}: the lies finalized unchallenged", chain.daa());
    }
    // The verifier's node starts AFTER the claims: an empty database validates every public block.
    chain.import_fresh(&premine).await;
    for p in &produced {
        assert!(
            chain.tc.palw_fraud_filer_candidates_v1(bond).iter().any(|c| c.claim_id == p.claim && !c.seat),
            "{}: a public candidate of the non-seat outsider",
            p.name
        );
    }
    let service_start = chain.history.len();
    let mut verifier = start_verifier(&chain, bond, &floats[OUTSIDER]);
    let deadline = std::time::Instant::now() + Duration::from_secs(budget_secs);
    let mut sessions: BTreeMap<Hash64, u8> = BTreeMap::new();
    let mut done_at: BTreeMap<Hash64, u64> = BTreeMap::new();
    let mut carried: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut refused: BTreeSet<(Hash64, PalwDaUnitV1)> = BTreeSet::new();
    loop {
        assert!(!verifier.running.task.as_ref().unwrap().is_finished(), "the production worker stopped");
        let pending_convictions: Vec<&Produced> =
            produced.iter().filter(|p| p.expect != Expect::Honest && !done_at.contains_key(&p.claim)).collect();
        if pending_convictions.is_empty() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the service made no objective progress on {:?}",
            pending_convictions
                .iter()
                .map(|p| (p.name, chain.tc.palw_legacy_dispute_v1(p.claim).map(|v| v.phase)))
                .collect::<Vec<_>>()
        );
        let (pending, _) = verifier.pool.clone().get_all_transactions(TransactionQuery::TransactionsOnly).await;
        if !pending.is_empty() {
            for t in &pending {
                if let Ok(payload) = borsh::from_slice::<PalwLifecycleTxPayloadV2>(&t.tx.payload) {
                    *carried.entry(super::object_name(&payload.object)).or_insert(0usize) += 1;
                }
            }
            let ids = pending.iter().map(|t| t.tx.id()).collect::<Vec<_>>();
            let block = pool_heartbeat(&mut chain, &verifier.manager, &verifier.pool).await;
            assert!(
                ids.iter().any(|id| block.transactions.iter().any(|tx| tx.id() == *id)),
                "the template includes a service carrier"
            );
            pool_heartbeat(&mut chain, &verifier.manager, &verifier.pool).await;
        }
        producer_answers(&mut chain, &mut floats, &backend, &produced, &|_| false, &mut refused).await;
        for p in &produced {
            let Some(view) = chain.tc.palw_legacy_dispute_v1(p.claim) else {
                assert!(
                    done_at.contains_key(&p.claim) || p.expect == Expect::Honest,
                    "{}: the claim left the state unconvicted",
                    p.name
                );
                continue;
            };
            if let Some(row) = view.record.as_ref().and_then(|r| r.live.get(&bond)) {
                let seen = sessions.entry(p.claim).or_default();
                *seen = (*seen).max(row.sessions_opened);
            }
            let objective = convicted(&chain, p.claim) || (p.expect == Expect::Withholding && defaulted(&chain, p.claim));
            if objective && !done_at.contains_key(&p.claim) {
                eprintln!(
                    "[g14v] {}: {} at DAA {} — executor_refuted {} court_convicted {} phase {:?}; verifier carriers so far {carried:?}",
                    p.name,
                    if convicted(&chain, p.claim) { "CONVICTED" } else { "DA DEFAULT" },
                    chain.daa(),
                    view.executor_refuted,
                    view.court_convicted,
                    view.phase
                );
                done_at.insert(p.claim, chain.daa());
            }
        }
        for _ in 0..3 {
            pool_heartbeat(&mut chain, &verifier.manager, &verifier.pool).await;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // Honest claims: a few more beats; nothing may be filed against them.
    for _ in 0..30 {
        pool_heartbeat(&mut chain, &verifier.manager, &verifier.pool).await;
    }
    let carriers: Vec<PalwConsensusObjectV2> = chain.history[service_start..]
        .iter()
        .flat_map(|b| b.transactions.iter())
        .filter_map(|t| borsh::from_slice::<PalwLifecycleTxPayloadV2>(&t.payload).ok())
        .map(|p| p.object)
        .collect();
    let mut lines = Vec::new();
    for p in &produced {
        let reserved = carriers.iter().any(|o| {
            matches!(o, PalwConsensusObjectV2::DisputeReservedV1 { reservation, .. } if reservation.claim == p.claim && reservation.reserver == bond)
        });
        let demands = carriers
            .iter()
            .filter(|o| match o {
                PalwConsensusObjectV2::DefaultAccused { claim, accuser, .. } => *claim == p.claim && *accuser == bond,
                PalwConsensusObjectV2::DefaultAccusedHeld { accusation } => accusation.claim == p.claim && accusation.accuser == bond,
                PalwConsensusObjectV2::LegacyHeldDemandedV2 { demand } => demand.claim == p.claim && demand.accuser == bond,
                _ => false,
            })
            .count();
        let view = chain.tc.palw_legacy_dispute_v1(p.claim);
        match p.expect {
            Expect::Convicted | Expect::Withholding => {
                assert!(done_at.contains_key(&p.claim), "{}: an objective outcome", p.name);
                assert!(reserved, "{}: the actual blocks carry this verifier's reservation", p.name);
                if post_final {
                    assert!(done_at[&p.claim] > 0);
                }
            }
            Expect::Honest => {
                assert!(!convicted(&chain, p.claim), "{}: an honest claim is never convicted", p.name);
                assert!(!reserved && demands == 0, "{}: nothing is filed against an honest claim", p.name);
            }
        }
        let line = format!(
            "{}: expect {:?}; lie leaf {:?} of {} leaves; reserved {reserved}; demands {demands}; max reserved sessions {}; phase now \
             {:?}; outcome at DAA {:?}",
            p.name,
            p.expect,
            p.lie_leaf,
            p.binding.step_leaf_count,
            sessions.get(&p.claim).copied().unwrap_or(0),
            view.map(|v| v.phase),
            done_at.get(&p.claim),
        );
        eprintln!("[g14v] {line}");
        lines.push(line);
    }
    eprintln!("[g14v] the verifier service's carriers through its own mempool: {carried:?}");
    verifier.running.finish().await;
    lines
}

/// **The probe**: the floor's attempt facts (leaf count, logits scheme, checkpoints, decode rows) and its op census — what the
/// cases below can inject into.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn g14v_probe_the_floor_attempt_facts() {
    let (config, bundle, premine, _) = fixture_config();
    let mut chain = Chain::new(config, bundle, &premine);
    chain.advance(2).await;
    let backend = floor_backend(&chain);
    let started = std::time::Instant::now();
    let p = real_attempt(&mut chain, &backend, PRODUCER, "probe", Lie::Honest, Expect::Honest).await;
    let b = &p.binding;
    eprintln!(
        "[g14v-probe] leaves {} decode {} prefill {} vocab {} flat {} checkpoints {} ({:?}) interval {}; attempt in {:?}",
        b.step_leaf_count,
        b.job_context.exact_decode_tokens,
        b.job_context.declared_prefill_tokens,
        b.shape_profile.vocab_size,
        b.shape_profile.logits_scheme_id == kaspa_consensus_core::palw_step_refute::flat_logits_scheme_id_v1(),
        b.checkpoint_count,
        b.checkpoint_merkle_root,
        b.checkpoint_profile.checkpoint_interval,
        started.elapsed()
    );
    eprintln!("[g14v-probe] op census {:?}", op_census(&p.material));
    eprintln!(
        "[g14v-probe] logits head provable: {:?}",
        kaspa_consensus_core::palw_step::palw_logits_head_v1(&b.shape_profile).map(|h| (h.slot, h.tile_len, h.tiles))
    );
}

/// **Step lies of every op kind the floor commits, each an isolated tile followed by honest values, convicted BEFORE `Final`** by
/// the service's authenticated subtree descent and the `StepLeaf` terminal the fold adjudicates; an honest twin is left alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn g14v_step_lies_of_every_floor_op_kind_are_localized_and_convicted_before_final() {
    super::palw_service_public_e2e::on_service_stack(g14v_step_lies_of_every_floor_op_kind_body).await
}

async fn g14v_step_lies_of_every_floor_op_kind_body() {
    use PalwStepOpKindV1 as K;
    let lines = run_cases(
        vec![
            ("matmul element (late)", Lie::Step(K::MatMulQuant), Expect::Convicted),
            ("matmul element (first)", Lie::StepFirst(K::MatMulQuant), Expect::Convicted),
            ("embedding gather", Lie::StepFirst(K::EmbedLookup), Expect::Convicted),
            ("normalization / rounding", Lie::Step(K::RmsNorm), Expect::Convicted),
            ("nonlinearity (silu)", Lie::Step(K::Silu), Expect::Convicted),
            ("honest twin", Lie::Honest, Expect::Honest),
        ],
        false,
        1_800,
    )
    .await;
    assert_eq!(lines.len(), 6);
}

/// **Past an agreeing step tree, and the claim's job/output bindings**: a logits lane bent over an honest step tree
/// (`LogitsNotStepOutput`), a forged decode token (`ForgedOutput`), a replaced output root (`OutputMismatch`), a relabelled prompt
/// and a garbage prompt root (input binding), a replaced activation leg and checkpoint interval (state-leg identity) — each
/// convicted BEFORE `Final` from the service's own reads and replay.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn g14v_trace_token_output_input_and_state_binding_lies_are_convicted_before_final() {
    super::palw_service_public_e2e::on_service_stack(g14v_trace_token_output_input_and_state_binding_lies_body).await
}

async fn g14v_trace_token_output_input_and_state_binding_lies_body() {
    let lines = run_cases(
        vec![
            (
                "logits lane over an honest step tree",
                Lie::Drill(PalwDrillFaultV1::BendLogits { row: 0, mode: PalwDrillBendV1::SameArgmax, lane: None }),
                Expect::Convicted,
            ),
            ("forged decode token", Lie::Drill(PalwDrillFaultV1::TokenNotSelected { pos: 0, lane: 7 }), Expect::Convicted),
            ("replaced output root", Lie::Drill(PalwDrillFaultV1::OutputRoot(Hash64::from_u64_word(0x0BAD))), Expect::Convicted),
            (
                "relabelled prompt",
                Lie::Drill(PalwDrillFaultV1::RelabelPrompt { from: Hash64::from_u64_word(0x5EED) }),
                Expect::Convicted,
            ),
            ("garbage prompt root", Lie::Drill(PalwDrillFaultV1::GarbagePromptRoot), Expect::Convicted),
            (
                "replaced activation leg",
                Lie::Drill(PalwDrillFaultV1::ActivationRoot(Hash64::from_u64_word(0xAC7))),
                Expect::Withholding,
            ),
            ("replaced checkpoint interval", Lie::Drill(PalwDrillFaultV1::CheckpointInterval(3)), Expect::Withholding),
        ],
        false,
        2_400,
    )
    .await;
    assert_eq!(lines.len(), 7);
}

/// **After `Final`**: a matmul lie and a logits lie that finalized unchallenged are convicted by a verifier whose node and service
/// start only after `Final` (the reservation at the Final row's stage, inside the liability horizon).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn g14v_a_step_lie_and_a_logits_lie_are_convicted_after_final() {
    super::palw_service_public_e2e::on_service_stack(g14v_a_step_lie_and_a_logits_lie_after_final_body).await
}

async fn g14v_a_step_lie_and_a_logits_lie_after_final_body() {
    let lines = run_cases(
        vec![
            ("matmul element after Final", Lie::Step(PalwStepOpKindV1::MatMulQuant), Expect::Convicted),
            (
                "logits lane after Final",
                Lie::Drill(PalwDrillFaultV1::BendLogits { row: 0, mode: PalwDrillBendV1::SameArgmax, lane: None }),
                Expect::Convicted,
            ),
        ],
        true,
        1_800,
    )
    .await;
    assert_eq!(lines.len(), 2);
}

/// **LG14-B armed as well**: the same service descends by authenticated tag-157/158 frontiers on its own base0 replica and files the
/// tag-159 leaf recompute from openings it builds itself — a late matmul element and the first embedding gather, before `Final`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn g14v_lg14b_frontier_descent_convicts_step_lies_through_the_service() {
    super::palw_service_public_e2e::on_service_stack(g14v_lg14b_frontier_descent_body).await
}

async fn g14v_lg14b_frontier_descent_body() {
    // One world per lie: both lying claims are the same producer's (card 0), so the first conviction forfeits the bond whole
    // (ADR-0160 AG-2, `forfeit_bond_v1`) and voids the other live claim `AggregateForfeit` before its own descent ends — which case
    // escapes then depends only on which attempt wins first. Each op's refutation through the service is asserted on its own.
    for case in [
        ("matmul element (LG14-B)", Lie::Step(PalwStepOpKindV1::MatMulQuant), Expect::Convicted),
        ("embedding gather (LG14-B)", Lie::StepFirst(PalwStepOpKindV1::EmbedLookup), Expect::Convicted),
    ] {
        let lines = run_cases_on(vec![case], false, true, 1_800).await;
        assert_eq!(lines.len(), 1);
    }
}
