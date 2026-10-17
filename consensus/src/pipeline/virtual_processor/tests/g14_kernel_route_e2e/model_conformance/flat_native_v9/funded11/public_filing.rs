//! **G14 lane closure G-N1 / T-N13 on the v11 native funded lane (2026-10-17).**
//!
//! A real funded job (registered native class, prepared, consented, funded, committed with a lie) on the ordinary node. Four bonds
//! that collude with the producer take every retained job ticket (paid seals they never reveal); the honest outsider's own seal is
//! refused, yet it convicts in time with a self-contained public filing (`EXECUTION_ACTION_PUBLIC_FILING_V11`) whose material it
//! read from the producer's public DA answers (`EXECUTION_SERVED_TABLE_V11`) only — no ticket, no producer obligation beyond the
//! ordinary lane. The producer is slashed, the requester refunded, no producer credit, and an independent replay agrees.
//!
//! The fold harness (`fold_on_tip`) applies one object to the node's own tip state with the processor's own fold params: the job,
//! book, class and quota rows are the node's, nothing synthetic. T-N13 drains the producer's collateral there (another lane's slash,
//! modelled) and shows the conviction is not refused: void, refund, collect what the bond holds, finder scaled.
use super::*;
use kaspa_consensus_core::palw_flat_model_vector_da_v9::{FlatVectorAnswerV9, decode_flat_vector_answer_v9};
use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwStateCarriageV2, apply_palw_transition_v3};
use misaka_palw_kernel::flat_detect5::{FlatPublicReadsV5, FlatReadV5};
use misaka_palw_kernel::flat_detect9::{FlatHeaderReadV9, FlatPublicReadsV9, FlatValueReadV9};

/// The DA window short enough for the node's carriers (as `source16`'s config): the answer and its proof grace fit the claim.
fn config_public() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (mut cfg, b, p, f) = native_config();
    cfg.params.palw_funded_execution_v11.as_mut().unwrap().policies.execution.horizons.da_answer = 20;
    (cfg, b, p, f)
}

/// Fund and commit `f`'s job with the statement's `corrupt` case (None: honest).
async fn committed(net: &mut Net, f: &Sparse9, corrupt: Option<u8>) -> (Digest, Server9) {
    let (id, class, bind, record) = funded(net, f).await;
    let row = net.api().unwrap().execution_job_row_v11(&id).unwrap();
    let fixed = net.api().unwrap().execution_class_header_v11(&class).unwrap().checked_fixed_header().unwrap();
    let book = row.restore_book(&fixed).unwrap();
    let (body, server) = statement(net, f, bind, fixed.record().candidate9_id, &record, corrupt);
    action(
        net,
        0,
        EXECUTION_ACTION_COMMIT_V11,
        borsh::to_vec(&FlatExecutionPostV11 {
            version: 11,
            execution_class: class,
            job_id: id,
            requester_nonce: book.job().nonce,
            job_input_root: book.job().input.root(),
            actual_scope9_root: book.job().actual_scope9_root,
            actual_resource10_root: book.job().actual_resource10_root,
            quota_plan_root: book.job().quota_plan_root,
            body,
        })
        .unwrap(),
    )
    .await;
    assert_eq!(net.api().unwrap().execution_job_header_v11(&id).unwrap().terminal, ExecutionTerminalV11::Challengeable);
    (id, server)
}

fn pursuit(net: &Net, id: Digest) -> NativeExecutionPursuitV11 {
    net.api().unwrap().aux_row(EXECUTION_DEMAND_COUNT_TABLE_V11, &borsh::to_vec(&id).unwrap()).unwrap_or_default()
}

fn claim_of(net: &Net, id: Digest) -> misaka_palw_kernel::flat_execution11::ExecutionClaimRecordV11 {
    let row = net.api().unwrap().execution_job_row_v11(&id).unwrap();
    let class = net.api().unwrap().execution_class_header_v11(&row.header.class_id).unwrap().checked_fixed_header().unwrap();
    row.restore_book(&class).unwrap().claim().unwrap().clone()
}

/// The producer's public DA answers for `vector`, as a replay session reads them (the served row is `(header, answer)`).
struct Served11 {
    route: kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1,
    vector: Hash64,
}
impl Served11 {
    fn answer(&self, read: FlatReadV5, kind: FlatVectorUnitKindV9) -> Option<FlatVectorAnswerV9> {
        let unit = FlatVectorUnitV9 { position: read.position, occurrence: read.occurrence, node: read.node, kind };
        let raw = self.route.aux.get(&(EXECUTION_SERVED_TABLE_V11, unit.key(&self.vector)))?;
        // FlatVectorServedHeaderV9 (8 + 64) ‖ u32 n ‖ answer.
        let n = u32::from_le_bytes(raw.get(72..76)?.try_into().ok()?) as usize;
        let answer = raw.get(76..76 + n)?;
        decode_flat_vector_answer_v9(answer).ok()
    }
}
impl FlatPublicReadsV5 for Served11 {
    fn node(&self, read: FlatReadV5) -> Option<misaka_palw_kernel::seg::NodeOpeningV1> {
        if let FlatVectorAnswerV9::Hash(a) = self.answer(read, FlatVectorUnitKindV9::Node)? { Some(a.opening) } else { None }
    }
    fn hash(&self, read: FlatReadV5) -> Option<misaka_palw_kernel::merkle5::FlatLeafHashV5> {
        if let FlatVectorAnswerV9::Hash(a) = self.answer(read, FlatVectorUnitKindV9::Hash(read.leaf?))? { a.leaf } else { None }
    }
}
impl FlatPublicReadsV9 for Served11 {
    fn value(&self, read: FlatValueReadV9) -> Option<Vec<u8>> {
        if let FlatVectorAnswerV9::Value(a) = self.answer(read.node, FlatVectorUnitKindV9::Value(read.element))? {
            (a.scope == read.scope).then(|| a.to_bytes().ok()).flatten()
        } else {
            None
        }
    }
    fn header(&self, read: FlatHeaderReadV9) -> Option<Vec<u8>> {
        if let FlatVectorAnswerV9::Header { value, .. } = self.answer(read.node, FlatVectorUnitKindV9::Header)? {
            (value.scope == read.scope).then(|| value.to_bytes().ok()).flatten()
        } else {
            None
        }
    }
}

/// **The outsider's independent pursuit over the ordinary lane (lane 0) only**: it replays the job from the public view, its own copy
/// of the parameters and the producer's served answers; each demand is answered by the producer (card 0) on chain. Returns the
/// finding (Clean or a Fault) and the ordinary requests it used.
async fn pursue(net: &mut Net, f: &Sparse9, id: Digest, outsider: usize, server: &Server9) -> (FlatFindingV9, u32) {
    let own = sparse9(f.tie).params; // the outsider's own copy of the public parameters, no producer trace
    let vector = Hash64::from_bytes(claim_of(net, id).vector);
    let view = net.api().unwrap().execution_view_v11(&id).unwrap();
    let mut session = FlatReplaySessionV9::new(limits(&view.replay, 128));
    let mut used = 0;
    for _ in 0..128 {
        let view = net.api().unwrap().execution_view_v11(&id).unwrap();
        let reads = Served11 { route: net.api().unwrap(), vector };
        let result = session.check(
            &view.context(),
            &view.replay,
            &reads,
            &|i, l| own.tensors.get(&(i, l)).cloned(),
            view.trace.job.inline_prompt.as_ref().unwrap(),
        );
        let finding = match result {
            FlatReplayResultV9::Finding(f) => f,
            x => panic!("independent bounded checker: {x:?}"),
        };
        let unit = match finding {
            FlatFindingV9::Demand(u) => FlatVectorUnitV9 {
                position: u.position,
                occurrence: u.occurrence,
                node: u.node,
                kind: u.leaf.map(FlatVectorUnitKindV9::Hash).unwrap_or(FlatVectorUnitKindV9::Node),
            },
            FlatFindingV9::ValueDemand(u) => FlatVectorUnitV9 {
                position: u.node.position,
                occurrence: u.node.occurrence,
                node: u.node.node,
                kind: FlatVectorUnitKindV9::Value(u.element),
            },
            FlatFindingV9::HeaderDemand(u) => FlatVectorUnitV9 {
                position: u.node.position,
                occurrence: u.node.occurrence,
                node: u.node.node,
                kind: FlatVectorUnitKindV9::Header,
            },
            f => return (f, used),
        };
        let (kind, index) = match unit.kind {
            FlatVectorUnitKindV9::Node => (0, 0),
            FlatVectorUnitKindV9::Hash(i) => (1, i),
            FlatVectorUnitKindV9::Value(i) => (2, i),
            FlatVectorUnitKindV9::Header => (3, 0),
        };
        let request = NativeExecutionDemandRequestV11 {
            job_id: id,
            slot: 0,
            position: unit.position,
            occurrence: unit.occurrence,
            node: unit.node,
            kind,
            index,
            lane: 0,
            entitlement: [0; 64],
        };
        let before: Option<Hash64> =
            net.api().unwrap().aux_row(EXECUTION_DEMAND_SLOT_TABLE_V11, &borsh::to_vec(&(net.bond(outsider), 0u8)).unwrap());
        action(net, outsider, EXECUTION_ACTION_DEMAND_V11, borsh::to_vec(&request).unwrap()).await;
        used += 1;
        let demand: Hash64 = net
            .api()
            .unwrap()
            .aux_row(EXECUTION_DEMAND_SLOT_TABLE_V11, &borsh::to_vec(&(net.bond(outsider), 0u8)).unwrap())
            .expect("the outsider's ordinary demand is admitted");
        assert_ne!(Some(demand), before, "demand {used} for {unit:?} was refused at DAA {} (pursuit {:?})", net.daa(), pursuit(net, id));
        let answer = server.answer(&unit).to_bytes().unwrap();
        let mut body = demand.as_bytes().to_vec();
        body.extend_from_slice(&(answer.len() as u32).to_le_bytes());
        body.extend_from_slice(&answer);
        action(net, 0, EXECUTION_ACTION_ANSWER_V11, body).await;
        assert!(net.api().unwrap().execution_demand_v11(&demand).unwrap().body.settled, "the producer answered on chain");
    }
    panic!("bounded acquisition did not terminate")
}

/// The public filing body: `job ‖ u32 n ‖ (11u16 ‖ kind 0 ‖ u32 m ‖ fault)`.
fn public_filing_raw(id: Digest, fault: Vec<u8>) -> Vec<u8> {
    let filing = borsh::to_vec(&FlatExecutionFilingV11 { version: 11, kind: 0, body: fault }).unwrap();
    let mut body = id.to_vec();
    body.extend_from_slice(&(filing.len() as u32).to_le_bytes());
    body.extend_from_slice(&filing);
    body
}
fn public_filing(id: Digest, fault: &misaka_palw_kernel::flat_court9::FlatFaultV9) -> Vec<u8> {
    public_filing_raw(id, fault.to_bytes().unwrap())
}

/// Four colluding bonds (cards 3–6) take every retained ticket with paid seals they will never reveal.
async fn squat(net: &mut Net, id: Digest) {
    for card in 3..=6usize {
        let mut body = id.to_vec();
        body.extend_from_slice(&[card as u8; 64]); // proof root of a filing that does not exist
        body.extend_from_slice(&[0xC0 | card as u8; 64]); // seal over it
        action(net, card, EXECUTION_ACTION_SEAL_V11, body).await;
    }
    assert_eq!(pursuit(net, id).retained_seal_keys.len(), 4, "the colluders hold every retained job ticket");
}

/// **One object folded on the node's own tip state** with the processor's own fold params (`mutate` edits that state first).
fn fold_on_tip(net: &mut Net, card: usize, object: &K, mutate: impl FnOnce(&mut PalwStateCarriageV2)) -> PalwChainStateV2 {
    let (_, tip) = net.chain.tip_state();
    let params = net.chain.vp().palw_state_params_v2.clone().expect("testnet-12 is ConsensusV2");
    // The node's own state, round-tripped through its carriage (the store's shape) so a test may edit one accumulator.
    let mut carriage = PalwStateCarriageV2::from_state(&tip);
    mutate(&mut carriage);
    let state = carriage
        .into_state_v3(
            &params,
            None,
            net.config.params.palw_uncertified_weightless.is_some_and(|f| f.is_active(net.daa())),
            net.config.params.palw_canonical_work_daa(),
        )
        .expect("the edited carriage rebuilds");
    let signed = net.route(card, object);
    let ctx = PalwBlockContextV2 {
        block: Hash64::from_u64_word(0x6E_1400 + net.daa()),
        daa_score: net.daa() + 1,
        blue_score: net.daa() + 1,
        subsidy: 0,
    };
    apply_palw_transition_v3(&state, &params, &ctx, &[signed], kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3::None)
        .expect("the fold applies the object")
        .0
}

#[tokio::test]
async fn g14n1_four_colluding_owners_cannot_lock_out_an_honest_outsider_who_convicts_in_time() {
    kaspa_core::log::try_init_logger("warn");
    let f = sparse9(false);
    let mut net = Net::over_cfg(config_public(), TestConsensus::new);
    let (id, server) = committed(&mut net, &f, Some(4)).await;
    squat(&mut net, id).await;
    // The outsider (card 7) cannot take a ticket: its own paid seal is refused (the job's four tickets are held).
    let mut seal = id.to_vec();
    seal.extend_from_slice(&[0x77; 64]);
    seal.extend_from_slice(&[0x78; 64]);
    action(&mut net, 7, EXECUTION_ACTION_SEAL_V11, seal).await;
    assert_eq!(pursuit(&net, id).retained_seal_keys.len(), 4, "a fifth ticket is refused: the lockout the audit found (G-N1)");
    // It reads the producer's public answers over the ordinary lane and finds the fault.
    let (finding, used) = pursue(&mut net, &f, id, 7, &server).await;
    let FlatFindingV9::Fault(fault) = finding else { panic!("the lie is found from public material: {finding:?}") };
    let view = net.api().unwrap().execution_view_v11(&id).unwrap();
    verify_flat_fault_v9(&view.context(), &fault).expect("the fault verifies against the committed post alone");
    let claim = claim_of(&net, id);
    assert!(net.daa() < claim.challenge_end, "in time: before the challenge window closes ({} < {})", net.daa(), claim.challenge_end);
    let producer_before = net.collateral(0);
    let requester_before = payee_minted_and_owed_all_queues(&net, 2);
    // A filing from the producer's own bond is refused (no state change); the outsider's convicts.
    action(&mut net, 0, EXECUTION_ACTION_PUBLIC_FILING_V11, public_filing(id, &fault)).await;
    assert_eq!(net.api().unwrap().execution_job_header_v11(&id).unwrap().terminal, ExecutionTerminalV11::Challengeable);
    action(&mut net, 7, EXECUTION_ACTION_PUBLIC_FILING_V11, public_filing(id, &fault)).await;
    let terminal = net.api().unwrap().execution_job_header_v11(&id).unwrap().terminal;
    assert_eq!(terminal, ExecutionTerminalV11::ComputationFault, "convicted without a ticket");
    let credit: NativeExecutionCreditV11 =
        net.api().unwrap().aux_row(EXECUTION_CREDIT_TABLE_V11, &borsh::to_vec(&id).unwrap()).expect("the terminal credit");
    assert_eq!(credit.producer_credit, 0, "no producer credit");
    assert_eq!(credit.collected_original_slash, claim.reserved_original, "the whole reserve is collected");
    assert_eq!(producer_before - net.collateral(0), claim.reserved_original, "the producer is slashed");
    assert_eq!(credit.requester_refund, 20, "the requester's escrow is refunded");
    assert_eq!(payee_minted_and_owed_all_queues(&net, 2) - requester_before, 20);
    assert!(credit.finder_credit * 1000 <= credit.collected_original_slash * 490, "the finder is paid at most ADR-0032's share");
    assert_eq!(credit.finder, Some(net.kid(7)));
    println!(
        "G-N1 v11: committed at {}, challenge_end {}, convicted at {}; ordinary requests used {used}; reserve {}; finder {}",
        claim.committed_daa,
        claim.challenge_end,
        net.daa(),
        claim.reserved_original,
        credit.finder_credit
    );
    // A second filing of the same fault changes nothing (the book is terminal).
    let after = net.collateral(0);
    action(&mut net, 7, EXECUTION_ACTION_PUBLIC_FILING_V11, public_filing(id, &fault)).await;
    assert_eq!(net.collateral(0), after, "never slashed twice");
    let replay = net.replay().await;
    net.assert_same(&replay, "v11 public filing past four squatting owners: conviction, slash, refund");
}

/// An honest job is never convicted by a public filing (a false fault does not verify) and the filer pays its work fee.
#[tokio::test]
async fn g14n1_a_public_filing_cannot_convict_an_honest_job() {
    kaspa_core::log::try_init_logger("warn");
    let f = sparse9(false);
    let mut net = Net::over_cfg(config_public(), TestConsensus::new);
    let (honest, server) = committed(&mut net, &f, None).await;
    let (finding, _) = pursue(&mut net, &f, honest, 7, &server).await;
    assert_eq!(finding, FlatFindingV9::Clean, "the honest job is clean to an independent replay");
    let producer_before = net.collateral(0);
    let filer_before = net.collateral(7);
    // A well-framed filing whose fault body is not a counterexample is dismissed at verification.
    action(&mut net, 7, EXECUTION_ACTION_PUBLIC_FILING_V11, public_filing_raw(honest, vec![9, 0, 0, 1, 2, 3])).await;
    assert_eq!(net.api().unwrap().execution_job_header_v11(&honest).unwrap().terminal, ExecutionTerminalV11::Challengeable);
    assert_eq!(net.collateral(0), producer_before, "the honest producer is not slashed");
    assert!(net.collateral(7) < filer_before, "the false filer paid its work fee");
    let replay = net.replay().await;
    net.assert_same(&replay, "a dismissed public filing");
}

/// **T-N13 (v11, fold over the node's state): a conviction is not refused for short collateral.** The producer's bond is drained
/// below its reserve (another lane's slash, modelled on the node's own tip state); the public filing still voids the claim, refunds
/// the requester from its escrow, collects what the bond holds and pays the finder in proportion.
#[tokio::test]
async fn g14n13_v11_native_conviction_is_not_refused_for_short_collateral_on_the_nodes_state() {
    kaspa_core::log::try_init_logger("warn");
    let f = sparse9(false);
    let mut net = Net::over_cfg(config_public(), TestConsensus::new);
    let (id, server) = committed(&mut net, &f, Some(4)).await;
    let (finding, _) = pursue(&mut net, &f, id, 7, &server).await;
    let FlatFindingV9::Fault(fault) = finding else { panic!("the lie is found: {finding:?}") };
    let reserve = claim_of(&net, id).reserved_original;
    assert!(reserve > 2, "the premise: a reserve to fall short of");
    let producer = net.bond(0);
    let object = K::FundedExecutionV11 { action: EXECUTION_ACTION_PUBLIC_FILING_V11, body: public_filing(id, &fault) };
    for left in [reserve / 3, 0] {
        let child = fold_on_tip(&mut net, 7, &object, |s| s.bonds.get_mut(&producer).unwrap().collateral = left);
        let route = child.kernel_route().unwrap();
        assert_eq!(
            route.execution_job_header_v11(&id).unwrap().terminal,
            ExecutionTerminalV11::ComputationFault,
            "convicted with {left} of {reserve} left"
        );
        let credit: NativeExecutionCreditV11 = route.aux_row(EXECUTION_CREDIT_TABLE_V11, &borsh::to_vec(&id).unwrap()).unwrap();
        assert_eq!(credit.collected_original_slash, left, "what the bond holds is collected, never the promised amount");
        assert_eq!(credit.requester_refund, 20, "the requester is refunded its own escrow");
        assert_eq!(credit.producer_credit, 0);
        assert!(credit.finder_credit <= left, "the finder is paid only out of a real collection");
        assert_eq!(child.bond(&producer).unwrap().collateral, 0, "the drained bond is emptied");
    }
    // The same fold with full collateral collects the whole reserve (the node's own state, untouched).
    let child = fold_on_tip(&mut net, 7, &object, |_| {});
    let credit: NativeExecutionCreditV11 =
        child.kernel_route().unwrap().aux_row(EXECUTION_CREDIT_TABLE_V11, &borsh::to_vec(&id).unwrap()).unwrap();
    assert_eq!(credit.collected_original_slash, reserve);
}

fn seal_of(net: &Net, key: &Digest) -> NativeExecutionSealV11 {
    net.api().unwrap().aux_row(EXECUTION_SEAL_TABLE_V11, &borsh::to_vec(key).unwrap()).expect("the seal row")
}

/// **G-N1 (v11): a lapsed seal gives its retained ticket back; a seal that compelled a protected answer keeps it.** Four colluders
/// squat; once their reveal windows close, anyone releases a seal that compelled nothing and the outsider's own seal is admitted
/// in its place. A colluder that spent one protected DA request keeps its ticket (it holds the producer's prepaid answers), so the
/// producer's bound of 2 × 4 protected answers per job is unchanged; its answer is public material.
#[tokio::test]
async fn g14n1_a_lapsed_seal_gives_its_ticket_back_but_one_that_compelled_answers_keeps_it() {
    kaspa_core::log::try_init_logger("warn");
    let f = sparse9(false);
    let mut net = Net::over_cfg(config_public(), TestConsensus::new);
    let (id, server) = committed(&mut net, &f, Some(4)).await;
    squat(&mut net, id).await;
    let keys = pursuit(&net, id).retained_seal_keys;
    let seals: Vec<NativeExecutionSealV11> = keys.iter().map(|k| seal_of(&net, k)).collect();
    // Colluder card 3 compels one protected answer once its seal matures.
    let compelling = seals.iter().position(|s| s.bond == net.bond(3)).unwrap();
    net.beat_to(seals[compelling].mature_daa).await;
    let request = NativeExecutionDemandRequestV11 {
        job_id: id,
        slot: 1,
        position: 0,
        occurrence: 0,
        node: 0,
        kind: 0,
        index: 0,
        lane: 1,
        entitlement: keys[compelling],
    };
    action(&mut net, 3, EXECUTION_ACTION_DEMAND_V11, borsh::to_vec(&request).unwrap()).await;
    let demand: Hash64 = net
        .api()
        .unwrap()
        .aux_row(EXECUTION_DEMAND_SLOT_TABLE_V11, &borsh::to_vec(&(net.bond(3), 1u8)).unwrap())
        .expect("the colluder's protected demand is admitted");
    let unit = FlatVectorUnitV9 { position: 0, occurrence: 0, node: 0, kind: FlatVectorUnitKindV9::Node };
    let answer = server.answer(&unit).to_bytes().unwrap();
    let mut body = demand.as_bytes().to_vec();
    body.extend_from_slice(&(answer.len() as u32).to_le_bytes());
    body.extend_from_slice(&answer);
    action(&mut net, 0, EXECUTION_ACTION_ANSWER_V11, body).await;
    // Before the reveal window closes nothing is released.
    let release = |k: &Digest| {
        let mut b = id.to_vec();
        b.extend_from_slice(k);
        b
    };
    let lapse = seals.iter().map(|s| s.reveal_until).max().unwrap();
    action(&mut net, 7, EXECUTION_ACTION_RELEASE_LAPSED_SEAL_V11, release(&keys[(compelling + 1) % 4])).await;
    assert_eq!(pursuit(&net, id).retained_seal_keys.len(), 4, "a revealable seal keeps its ticket");
    net.beat_to(lapse).await;
    let claim = claim_of(&net, id);
    // The compelling colluder keeps its ticket.
    action(&mut net, 7, EXECUTION_ACTION_RELEASE_LAPSED_SEAL_V11, release(&keys[compelling])).await;
    assert!(pursuit(&net, id).retained_seal_keys.contains(&keys[compelling]), "a seal that compelled answers keeps its slot");
    // The three that compelled nothing are released by the outsider (any bond may).
    for (i, k) in keys.iter().enumerate() {
        if i != compelling {
            action(&mut net, 7, EXECUTION_ACTION_RELEASE_LAPSED_SEAL_V11, release(k)).await;
        }
    }
    let left = pursuit(&net, id).retained_seal_keys;
    assert_eq!(left, vec![keys[compelling]], "three lapsed tickets came back");
    // The kernel subject counter fell with them (the next seal is admitted by the kernel, not only by the pursuit list).
    let mut seal = id.to_vec();
    seal.extend_from_slice(&[0x77; 64]);
    seal.extend_from_slice(&[0x78; 64]);
    action(&mut net, 7, EXECUTION_ACTION_SEAL_V11, seal).await;
    let now = pursuit(&net, id).retained_seal_keys;
    assert_eq!(now.len(), 2, "the outsider's own seal is admitted in a released place");
    assert_eq!(seal_of(&net, &now[1]).bond, net.bond(7));
    println!("G-N1 v11 lapse: reveal windows closed by {lapse}; challenge_end {}; outsider sealed at {}", claim.challenge_end, net.daa());
    let replay = net.replay().await;
    net.assert_same(&replay, "lapsed seals released, compelling seal kept, outsider admitted");
}
