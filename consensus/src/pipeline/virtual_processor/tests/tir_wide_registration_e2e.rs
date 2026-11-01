//! Private typed wide-registration integration fixtures. These exercise actual
//! funding transactions; they do not establish native wide claim/court support.
mod hf3;
use super::{
    g14_registration_e2e::{arrive, chain_blocks, root_at},
    t12_round_lane_e2e::{
        T12Chain, card_payout_spk, sign_spend, t12_genesis_chain, t12_genesis_chain_on, t12_reopened_chain, t12_with_harness_cards,
    },
};
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::acceptance_data::AcceptanceDataStoreReader;
use kaspa_consensus_core::{
    Hash64,
    api::ConsensusApi,
    block::Block,
    config::params::ForkActivation,
    config::{Config, ConfigBuilder},
    palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2},
    palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2},
    palw_state_v2::{PalwBondStatusV2, PalwCertifiedLaneV1, PalwClassStatusV2, PalwConsensusObjectV2 as Obj},
    palw_tir_admission_v2::palw_tir_post_genesis_registration_v2,
    palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_root_v1},
    palw_tir_class_v1::{
        PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1, PalwTirClassV1, PalwTirLayoutV1, palw_tir_class_registration_message_v1,
    },
    palw_tir_profile_resolver_v2::ResolvedTirJobProfileV2,
    palw_tir_registration_dispatch_v1::palw_tir_registration_preflight_selected_at_v1,
    palw_tir_v1::PalwTirFenceV1,
    tx::{MutableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
};
use misaka_palw_tir::{
    DType, MapParams, Ref, Tensor, TensorType,
    builder::ProgramBuilder,
    program::{HISTORY_BOUND_V1_SMALL, LOGITS_I64_Q24_SCHEME_V2, MAX_PROGRAM_BYTES},
};
use std::borrow::Cow;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;
const FEE: u64 = 2_000_000;
const CARD: usize = 2;

fn config(version: u16) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (base, _, premine, floats) = t12_with_harness_cards();
    let mut params = base.params.clone();
    let mut fence = PalwTirFenceV1::testnet12_v1(ForkActivation::new(0));
    fence.court_version = version;
    params.palw_tir_v1 = Some(fence);
    params.sync_palw_tir_v1();
    params.palw_reorg_strict_economic_win = Some(ForkActivation::new(0));
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    config.params.validate_palw_v2().expect("the private typed registration rules validate");
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    (config.clone(), bundle.clone(), premine, floats)
}

struct Source(MapParams);
impl PalwTirTensorSourceV1 for Source {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        self.0.tensors.get(&(param, layer)).map(|t| Cow::Owned(t.to_le_bytes()))
    }
}
fn class(seed: i128) -> (PalwTirClassV1, Hash64) {
    let mut p = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
    let embed = p.param("embed", DType::I8, &[4, 4], false);
    let scale = p.konst(DType::I64, &[4], &[1 << 32, 1 << 33, 1 << 34, 1 << 35]);
    let mut pre = p.block("pre", vec![]);
    let row = pre.gather(embed, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[row]);
    let mut post = p.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let narrow = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(narrow);
    let wide = post.mul(narrow, scale, DType::I64);
    post.commit(wide);
    let Ref::Node(logits) = wide else { unreachable!() };
    let post = post.finish(&[]);
    let mut program = p.finish(pre, vec![], post, logits);
    program.logits_scheme_id = LOGITS_I64_Q24_SCHEME_V2;
    misaka_palw_tir::interval::analyze_ranges(&program).unwrap();
    let mut tensors = MapParams::default();
    tensors.tensors.insert((0, None), Tensor::new(DType::I8, vec![4, 4], (-8..8).map(|n| i128::from(n) + seed).collect()).unwrap());
    let (artifact, _) = palw_tir_inventory_root_v1(&program, &Source(tensors)).unwrap();
    (
        PalwTirClassV1 {
            version: 1,
            program: program.encode(),
            tokenizer_id: Hash64::from_u64_word(18),
            layout: PalwTirLayoutV1 {
                version: 1,
                max_context: 64,
                checkpoint_interval: 2,
                h_tile: 2,
                commit_tiles: vec![4; 3],
                state_tiles: vec![],
            },
        },
        artifact,
    )
}
struct Env {
    chain: T12Chain,
    config: Config,
    bundle: PalwConsensusParamsV2,
    premine: Premine,
    floats: Premine,
}
impl Env {
    fn new(version: u16) -> Self {
        Self::over(config(version))
    }
    fn over((config, bundle, premine, floats): (Config, PalwConsensusParamsV2, Premine, Premine)) -> Self {
        let chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
        Self { chain, config, bundle, premine, floats }
    }
    fn domain(&self) -> Hash64 {
        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            self.config.params.net.to_string().as_bytes(),
            Some(self.config.params.genesis.hash),
        )
    }
    fn object(&self) -> Obj {
        self.object_for(0, CARD)
    }
    fn object_for(&self, seed: i128, card: usize) -> Obj {
        let (_, state) = self.chain.tip_state();
        let floor = self.bundle.base_class_id;
        let (class, artifact) = class(seed);
        let canonical = ResolvedTirJobProfileV2::new(&class, artifact, MAX_PROGRAM_BYTES).unwrap().ctx().unwrap();
        palw_tir_post_genesis_registration_v2(
            class,
            canonical,
            artifact,
            0,
            state.class_target(&floor).unwrap().target,
            state.class(&floor).unwrap().slash_value_per_pwu,
            self.chain.daa_of(self.chain.sink()) + 30,
            self.chain.bonds[card],
            vec![],
            self.bundle.court.max_step_leaf_count(),
        )
        .unwrap()
    }
    fn carrier(&self, object: &Obj, funding: (TransactionOutpoint, UtxoEntry)) -> Transaction {
        self.carrier_for(object, funding, CARD)
    }
    fn carrier_for(&self, object: &Obj, funding: (TransactionOutpoint, UtxoEntry), card: usize) -> Transaction {
        let (outpoint, entry) = funding;
        let payload =
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() }).unwrap();
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(entry.amount - FEE, card_payout_spk(card))],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        sign_spend(&mut tx, entry, card, self.config.params.storage_mass_parameter);
        tx
    }
    async fn mine(&mut self, carrier: &Transaction) -> (Block, Block) {
        self.chain.vp().validate_mempool_transaction(&mut MutableTransaction::from_tx(carrier.clone()), &Default::default()).unwrap();
        let t = self.config.params.target_time_per_block();
        let carrying = self.chain.heartbeat(t, vec![carrier.clone()]).await;
        assert!(carrying.transactions.iter().any(|tx| tx.id() == carrier.id()));
        let folding = self.chain.heartbeat(t, vec![]).await;
        let accepted = self.chain.vp().acceptance_data_store.get(folding.header.hash).unwrap();
        assert!(accepted.iter().any(|block| block.block_hash == carrying.header.hash
            && block.accepted_transactions.iter().any(|tx| tx.transaction_id == carrier.id())));
        (carrying, folding)
    }
}
fn sign(env: &Env, object: &mut Obj, card: usize) {
    let Obj::ClassRegisteredTirV1 {
        class_id,
        artifact_root,
        pwu_rule,
        initial_target,
        slash_value_per_pwu,
        share_permille,
        activation_daa,
        admission,
    } = object
    else {
        unreachable!()
    };
    let message = palw_tir_class_registration_message_v1(
        env.domain(),
        *class_id,
        *share_permille,
        *activation_daa,
        &admission.registrant_bond,
        *artifact_root,
        *slash_value_per_pwu,
        *initial_target,
        pwu_rule,
        &admission.canonical,
        &admission.class,
    );
    admission.signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &TestConsensus::palw_v2_registry_keypair(card as u64).signing_key,
        message.as_byte_slice(),
        PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1,
        [0x61; 32],
    )
    .unwrap()
    .as_ref()
    .to_vec();
}
fn id(object: &Obj) -> Hash64 {
    let Obj::ClassRegisteredTirV1 { class_id, .. } = object else { unreachable!() };
    *class_id
}

struct Mined {
    env: Env,
    object: Obj,
    carrier: Transaction,
    warm: Block,
    carrying: Block,
    folding: Block,
    collateral: u64,
}
async fn registered(mut env: Env) -> Mined {
    let t = env.config.params.target_time_per_block();
    let warm = env.chain.heartbeat(t, vec![]).await;
    let (_, state) = env.chain.tip_state();
    assert!(matches!(state.bond(&env.chain.bonds[CARD]).unwrap().status, PalwBondStatusV2::Active));
    let collateral = state.bond(&env.chain.bonds[CARD]).unwrap().collateral;
    let mut object = env.object();
    sign(&env, &mut object, CARD);
    let certified = state.chain_certified_families(PalwCertifiedLaneV1::Attempt);
    let (_, expected) = palw_tir_registration_preflight_selected_at_v1(
        &env.config.params,
        &env.bundle,
        &object,
        env.chain.daa_of(env.chain.sink()),
        &certified,
    )
    .unwrap();
    let carrier = env.carrier(&object, env.floats[CARD].clone());
    let (carrying, folding) = env.mine(&carrier).await;
    let (block, state) = env.chain.tip_state();
    assert_eq!(block, folding.header.hash);
    let row = state.class(&id(&object)).expect("actual accepted registration fold wrote the class");
    assert_eq!(row.registered_daa, env.chain.daa_of(folding.header.hash));
    assert!(matches!(row.status, PalwClassStatusV2::Registered { pending_share_permille: 0, .. }));
    assert_eq!(state.tir_class_v1(&id(&object)), Some(&expected));
    assert_eq!(
        state.bond(&env.chain.bonds[CARD]).unwrap().collateral,
        collateral - kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_BURN_SOMPI_V1
    );
    assert!(state.registration_exposure(&env.chain.bonds[CARD]) > 0);
    assert!(env.chain.ctx.consensus.get_virtual_utxo_entry(env.floats[CARD].0).is_none());
    assert!(env.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(carrier.id(), 0)).is_some());
    let rows = env.chain.ctx.consensus.palw_v2_class_table();
    assert_eq!(
        kaspa_consensus_core::palw_model_registration_v1::palw_registration_row_written_by_v1(&carrier, row.registered_daa, &rows)
            .map(|r| r.class_id),
        Some(id(&object))
    );
    Mined { env, object, carrier, warm, carrying, folding, collateral }
}

#[tokio::test]
async fn wide_registration_real_carrier_replays_and_rolls_back_across_a_reorg() {
    let mut a = registered(Env::new(2)).await;
    let class_id = id(&a.object);
    let (_, accepted) = a.env.chain.tip_state();
    let z = t12_genesis_chain(&a.env.config, &a.env.bundle, &a.env.premine, &a.env.floats);
    for block in chain_blocks(&a.env.chain, a.env.chain.sink()) {
        arrive(&z, block, "wide registration replay").await;
    }
    assert_eq!(z.tip_state().1.state_root(), accepted.state_root());
    assert_eq!(z.tip_state().1.tir_class_v1(&class_id), accepted.tir_class_v1(&class_id));
    for block in [&a.warm, &a.carrying, &a.folding] {
        assert_eq!(root_at(&z, block.header.hash), root_at(&a.env.chain, block.header.hash));
    }
    let mut alternate = t12_genesis_chain(&a.env.config, &a.env.bundle, &a.env.premine, &a.env.floats);
    arrive(&alternate, a.warm.clone(), "common warm block").await;
    alternate.ctx.simulated_time = a.warm.header.timestamp;
    let t = a.env.config.params.target_time_per_block();
    let mut heavier = vec![];
    for _ in 0..4 {
        heavier.push(alternate.heartbeat(t, vec![]).await);
    }
    for block in heavier {
        arrive(&z, block, "wide registration reorg branch").await;
    }
    assert_eq!(z.sink(), alternate.sink());
    let (_, removed) = z.tip_state();
    assert!(removed.class(&class_id).is_none());
    assert!(removed.tir_class_v1(&class_id).is_none());
    assert_eq!(removed.bond(&a.env.chain.bonds[CARD]).unwrap().collateral, a.collateral);
    assert!(z.ctx.consensus.get_virtual_utxo_entry(a.env.floats[CARD].0).is_some());
    assert!(z.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(a.carrier.id(), 0)).is_none());
    // Reapply the original class with a freshly signed registration at current
    // terms, through a carrier spending the restored original UTXO.
    a.env.chain = z;
    let mut renewed = a.env.object();
    sign(&a.env, &mut renewed, CARD);
    let carrier = a.env.carrier(&renewed, a.env.floats[CARD].clone());
    a.env.mine(&carrier).await;
    assert!(a.env.chain.tip_state().1.tir_class_v1(&class_id).is_some());
}

#[tokio::test]
async fn wide_registration_restarts_from_the_same_real_database() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    let (config, bundle, premine, floats) = config(2);
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, _rx) = async_channel::unbounded();
    let chain = t12_genesis_chain_on(TestConsensus::with_db(db.clone(), &config, sender), &config, &bundle, &premine, &floats);
    let mined = registered(Env { chain, config: config.clone(), bundle: bundle.clone(), premine, floats }).await;
    let class_id = id(&mined.object);
    let (sink, state) = mined.env.chain.tip_state();
    let expected_root = state.state_root();
    let expected_row = state.tir_class_v1(&class_id).cloned().unwrap();
    let change = TransactionOutpoint::new(mined.carrier.id(), 0);
    let time = mined.env.chain.ctx.simulated_time;
    let nonce = mined.env.chain.nonce_for_reopen();
    drop(mined);
    let mut resumed = config;
    resumed.process_genesis = false;
    let (sender, _rx2) = async_channel::unbounded();
    let second = TestConsensus::with_db(db.clone(), &resumed, sender);
    let reopened = t12_reopened_chain(second, &resumed, &bundle, time, nonce);
    assert_eq!(reopened.sink(), sink);
    assert_eq!(reopened.tip_state().1.state_root(), expected_root);
    assert_eq!(reopened.tip_state().1.tir_class_v1(&class_id), Some(&expected_row));
    assert!(reopened.ctx.consensus.get_virtual_utxo_entry(change).is_some());
}

#[tokio::test]
async fn wide_registration_invalid_bond_signature_or_target_still_spends_only_the_valid_carrier() {
    for version in [1, 2] {
        let mut env = Env::new(version);
        let t = env.config.params.target_time_per_block();
        env.chain.heartbeat(t, vec![]).await;
        let collateral = env.chain.tip_state().1.bond(&env.chain.bonds[CARD]).unwrap().collateral;
        let exposure = env.chain.tip_state().1.registration_exposure(&env.chain.bonds[CARD]);
        let mut funding = env.floats[CARD].clone();
        // Version1 also receives an otherwise honest signed object, isolating
        // the trusted court selector from signature/target rejection.
        for mutation in 0..if version == 1 { 4 } else { 3 } {
            let mut object = env.object();
            if mutation == 2 {
                let Obj::ClassRegisteredTirV1 { initial_target, .. } = &mut object else { unreachable!() };
                *initial_target /= 2;
            }
            sign(&env, &mut object, if mutation == 1 { CARD + 1 } else { CARD });
            if mutation == 0 {
                let Obj::ClassRegisteredTirV1 { admission, .. } = &mut object else { unreachable!() };
                admission.signature[0] ^= 1;
            }
            let carrier = env.carrier(&object, funding);
            env.mine(&carrier).await;
            let (_, state) = env.chain.tip_state();
            assert!(state.class(&id(&object)).is_none());
            assert!(state.tir_class_v1(&id(&object)).is_none());
            assert_eq!(state.bond(&env.chain.bonds[CARD]).unwrap().collateral, collateral);
            assert_eq!(state.registration_exposure(&env.chain.bonds[CARD]), exposure);
            funding =
                (TransactionOutpoint::new(carrier.id(), 0), UtxoEntry::new(carrier.outputs[0].value, card_payout_spk(CARD), 0, false));
            assert!(env.chain.ctx.consensus.get_virtual_utxo_entry(funding.0).is_some());
        }
    }
}

#[tokio::test]
async fn wide_registration_one_per_block_cap_preserves_both_real_carrier_utxo_acceptances() {
    let mut env = Env::new(2);
    let t = env.config.params.target_time_per_block();
    env.chain.heartbeat(t, vec![]).await;
    let (_, before) = env.chain.tip_state();
    let mut objects = [env.object_for(0, CARD), env.object_for(1, CARD + 1)];
    let mut carriers = vec![];
    for (i, object) in objects.iter_mut().enumerate() {
        let card = CARD + i;
        sign(&env, object, card);
        // Each object is admissible separately, including its actual signature
        // and bond; the paired carrier check below tests only the block cap.
        let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
            block: env.chain.sink(),
            daa_score: env.chain.daa_of(env.chain.sink()),
            blue_score: before.last_point().map_or(1, |p| p.blue_score + 1),
            subsidy: 0,
        };
        env.chain.vp().palw_v2_validate_objects(&before, &env.bundle.state, &point, std::slice::from_ref(object)).unwrap();
        let carrier = env.carrier_for(object, env.floats[card].clone(), card);
        env.chain.vp().validate_mempool_transaction(&mut MutableTransaction::from_tx(carrier.clone()), &Default::default()).unwrap();
        carriers.push(carrier);
    }
    assert_ne!(id(&objects[0]), id(&objects[1]));
    let carrying = env.chain.heartbeat(t, carriers.clone()).await;
    let order: Vec<_> = carrying.transactions.iter().filter_map(|tx| carriers.iter().position(|c| c.id() == tx.id())).collect();
    assert_eq!(order.len(), 2);
    let folding = env.chain.heartbeat(t, vec![]).await;
    let (_, after) = env.chain.tip_state();
    assert!(after.class(&id(&objects[order[0]])).is_some());
    assert!(after.tir_class_v1(&id(&objects[order[0]])).is_some());
    assert!(after.class(&id(&objects[order[1]])).is_none());
    assert!(after.tir_class_v1(&id(&objects[order[1]])).is_none());
    let accepted = env.chain.vp().acceptance_data_store.get(folding.header.hash).unwrap();
    for (i, carrier) in carriers.iter().enumerate() {
        assert!(accepted.iter().any(
            |b| b.block_hash == carrying.header.hash && b.accepted_transactions.iter().any(|tx| tx.transaction_id == carrier.id())
        ));
        assert!(env.chain.ctx.consensus.get_virtual_utxo_entry(env.floats[CARD + i].0).is_none());
        assert!(env.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(carrier.id(), 0)).is_some());
        let paid = if i == order[0] { kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_BURN_SOMPI_V1 } else { 0 };
        let bond = env.chain.bonds[CARD + i];
        assert_eq!(after.bond(&bond).unwrap().collateral, before.bond(&bond).unwrap().collateral - paid);
    }
}

// These test-only stages return before the next stage starts. In particular,
// the t12 builder is never below the court/isolated Params clone/build frames.
// Keep Config/bundle on the heap between stages; this does not enlarge libtest's
// stack, weaken validation, or change the remaining by-value config factory.
struct IsolatedWideCarrierOriginSetup {
    config: Box<Config>,
    bundle: Box<PalwConsensusParamsV2>,
    premine: Premine,
    floats: Premine,
}

#[inline(never)]
fn isolated_wide_carrier_origin_initial_setup() -> Box<IsolatedWideCarrierOriginSetup> {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    Box::new(IsolatedWideCarrierOriginSetup { config: Box::new(config), bundle: Box::new(bundle), premine, floats })
}

#[inline(never)]
fn isolated_wide_carrier_origin_court_setup(
    mut setup: Box<IsolatedWideCarrierOriginSetup>,
    version: u16,
) -> Box<IsolatedWideCarrierOriginSetup> {
    let mut params = setup.config.params.clone();
    let mut fence = PalwTirFenceV1::testnet12_v1(ForkActivation::new(0));
    fence.court_version = version;
    params.palw_tir_v1 = Some(fence);
    params.sync_palw_tir_v1();
    params.palw_reorg_strict_economic_win = Some(ForkActivation::new(0));
    let config = Box::new(ConfigBuilder::new(params).skip_proof_of_work().build());
    config.params.validate_palw_v2().expect("the private typed registration rules validate");
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    setup.bundle = Box::new(bundle.clone());
    setup.config = config;
    setup
}

#[inline(never)]
fn isolated_wide_carrier_origin_isolated_setup(mut setup: Box<IsolatedWideCarrierOriginSetup>) -> Box<IsolatedWideCarrierOriginSetup> {
    let mut params = setup.config.params.clone();
    params.genesis = kaspa_consensus_core::palw_g14_gates_v1::palw_isolated_g14_genesis_v1(
        &params.genesis,
        Hash64::from_u64_word(0x5749_4445_4f52_4947),
    )
    .unwrap();
    params.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(0));
    params.palw_class_registration_origin_v1 = Some(ForkActivation::new(0));
    params.sync_palw_class_registration_origin_v1();
    assert!(params.has_isolated_g14_genesis_v1());
    assert_ne!(params.genesis.hash, kaspa_consensus_core::config::genesis::PALW_T12_GENESIS.hash);
    params.validate_palw_v2().expect("documented isolated Params, with all validation enabled");
    let config = Box::new(ConfigBuilder::new(params).skip_proof_of_work().build());
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    setup.bundle = Box::new(bundle.clone());
    setup.config = config;
    setup
}

// Keep setup, future construction, and runtime/poll in sibling stages. Each
// heavy setup stage returns before entering the next (no nested factory call).
#[inline(never)]
fn isolated_wide_carrier_origin_boxed_setup() -> Box<IsolatedWideCarrierOriginSetup> {
    let setup = isolated_wide_carrier_origin_initial_setup();
    let setup = isolated_wide_carrier_origin_court_setup(setup, 2);
    isolated_wide_carrier_origin_isolated_setup(setup)
}

// Retain the original notification receiver AND DB lifetime through the full
// verification/restore. Field order drops chain/DB references before lifetime.
struct IsolatedWideCarrierOriginPrepared {
    chain: Box<T12Chain>,
    setup: Box<IsolatedWideCarrierOriginSetup>,
    _rx: async_channel::Receiver<kaspa_consensus_notify::notification::Notification>,
    db: std::sync::Arc<kaspa_database::prelude::DB>,
    _lifetime: kaspa_database::utils::DbLifetime,
}

// Called while only the SMALL outer future is active. The large verification
// continuation is not even constructed until this synchronous helper returns.
#[inline(never)]
fn isolated_wide_carrier_origin_prepare(setup: Box<IsolatedWideCarrierOriginSetup>) -> Box<IsolatedWideCarrierOriginPrepared> {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, _rx) = async_channel::unbounded();
    let consensus = TestConsensus::with_db(db.clone(), &setup.config, sender);
    let chain = Box::new(t12_genesis_chain_on(consensus, &setup.config, &setup.bundle, &setup.premine, &setup.floats));
    Box::new(IsolatedWideCarrierOriginPrepared { chain, setup, _rx, db, _lifetime })
}

#[inline(never)]
fn isolated_wide_carrier_origin_boxed_case(
    setup: Box<IsolatedWideCarrierOriginSetup>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> {
    Box::pin(async move {
        let prepared = isolated_wide_carrier_origin_prepare(setup);
        let mut verification = isolated_wide_carrier_origin_boxed_verification(prepared);
        verification.as_mut().await;
    })
}

#[inline(never)]
fn isolated_wide_carrier_origin_boxed_verification(
    prepared: Box<IsolatedWideCarrierOriginPrepared>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> {
    Box::pin(async move {
        use kaspa_consensus_core::palw_class_registration_origin_v1::{
            PalwClassRegistrationDoorV1, palw_class_admission_digest_v1, palw_class_registration_origin_row_v1,
        };
        // Bind lifetime first, so reverse local drop order keeps it alive until
        // chain/restore/db/receiver owners have been released, as in the old case.
        let IsolatedWideCarrierOriginPrepared { _lifetime, db, _rx, setup, chain } = *prepared;
        let IsolatedWideCarrierOriginSetup { config, bundle, premine, floats } = *setup;
        let config = *config;
        let bundle = *bundle;
        let chain = *chain;
        let mined = registered(Env { chain, config: config.clone(), bundle: bundle.clone(), premine, floats }).await;
        let class_id = id(&mined.object);
        let (sink, state) = mined.env.chain.tip_state();
        let class_row = state.class(&class_id).unwrap();
        let origin =
            palw_class_registration_origin_row_v1(state.kernel_route(), &class_id).expect("actual registration origin aux141");
        let Obj::ClassRegisteredTirV1 { admission, .. } = &mined.object else { unreachable!() };
        assert_eq!(origin.version, 1);
        assert_eq!(origin.class_id, class_id);
        assert_eq!(origin.block, mined.folding.header.hash);
        assert_eq!(origin.block, sink);
        assert_eq!(origin.daa_score, class_row.registered_daa);
        assert_eq!(origin.daa_score, mined.env.chain.daa_of(sink));
        assert_eq!(origin.door, PalwClassRegistrationDoorV1::Tir);
        assert_eq!(origin.admission_digest, Some(palw_class_admission_digest_v1(&**admission).unwrap()));
        // registered() already checked this exact carrier's accepted transaction
        // entry and its class-table writer at the same accepting DAA.
        let expected_root = state.state_root();
        let expected_record = state.tir_class_v1(&class_id).cloned().unwrap();
        let expected_class = class_row.clone();
        let change = TransactionOutpoint::new(mined.carrier.id(), 0);
        let time = mined.env.chain.ctx.simulated_time;
        let nonce = mined.env.chain.nonce_for_reopen();
        drop(mined);
        let mut resumed = config;
        resumed.process_genesis = false;
        let (sender, _rx2) = async_channel::unbounded();
        let second = TestConsensus::with_db(db.clone(), &resumed, sender);
        let reopened = t12_reopened_chain(second, &resumed, &bundle, time, nonce);
        let (restored_tip, restored) = reopened.tip_state();
        assert_eq!(restored_tip, sink);
        assert_eq!(restored.state_root(), expected_root);
        assert_eq!(restored.class(&class_id), Some(&expected_class));
        assert_eq!(restored.tir_class_v1(&class_id), Some(&expected_record));
        assert_eq!(palw_class_registration_origin_row_v1(restored.kernel_route(), &class_id), Some(origin));
        assert!(reopened.ctx.consensus.get_virtual_utxo_entry(change).is_some());
    })
}

#[test]
fn isolated_wide_carrier_origin_matches_actual_accepting_block_and_survives_same_db_restore() {
    let setup = isolated_wide_carrier_origin_boxed_setup();
    let mut case = isolated_wide_carrier_origin_boxed_case(setup);
    tokio::runtime::Builder::new_current_thread().enable_all().build().expect("Failed building the Runtime").block_on(case.as_mut());
}
