//! **Lane maturity at the processor, on a real testnet-12 chain that crosses the fence**
//! (`Params::palw_bond_maturity_early`, 2026-09-26: ADR-0065 D1 brought forward).
//!
//! The chain is `t12_round_lane_e2e`'s harness — testnet-12 as shipped, harness keys on the eight
//! genesis cards, the premine imported, the EVM lane inert — with ONE more harness difference: the
//! main premine output pays a ninth harness key (registry row 8), so a newcomer can fund a real
//! registration. On it, block by block:
//!
//! * at DAA ~12 the newcomer registers a bond of genesis-card collateral declaring the floor, through
//!   a real `0x4b` carrier with both signatures (the drill's "registered at DAA 13");
//! * cards 0, 1 and 7 make floor claims right after, and card 2's attempt at their anchor slot binds
//!   all three below the fence (SW-8: a panel binds in its anchor block, drawn by the processor);
//! * cards 3, 4 and 5 make claims right after that — ACCEPTED below the fence — and card 6's attempt
//!   at their slot binds them and card 2's claim, ANCHORED past it;
//! * the fence is armed on a copy at [`FENCE`], and the same script runs on testnet-12 as shipped (the
//!   twin). Each card produces once: the harness's producer check counts every unlicensed claim and
//!   seat against the card's ceiling, and nothing here licenses.
//!
//! What is asserted:
//!
//! * **below the fence the rule is the release's, byte for byte** — every block at a DAA below the
//!   fence has the same hash and the same PALW state root on both chains;
//! * **below the fence the newcomer is drawable** — the processor's resolved window is `None` at every
//!   anchor below it, and the chain actually SEATS the newcomer on a floor panel below the fence;
//! * **past the fence it is not** — at every anchor from the fence the processor's window is D1's, the
//!   newcomer is below its floor, and every claim anchored past the fence (all four ACCEPTED below
//!   it) is bound with a full jury of genesis cards: the genesis seats keep drawing and no claim voids
//!   for want of seats. The twin keeps drawing — and seating — the newcomer at the same anchors;
//! * **until registered + window** — the armed processor's resolution admits the newcomer at anchor
//!   `registered_daa + 1,000` and not one DAA before, and from testnet-12's own 1,000 fence on the
//!   armed and the released processor resolve one window (no double counting).
use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, sign_spend, t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::config::params::{ForkActivation, PALW_T12_BOND_MATURITY_WINDOW_DAA};
use kaspa_consensus_core::config::premine::{MAIN_PREMINE_INDEX, PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI, premine_outpoint_for};
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk;
use kaspa_consensus_core::muhash::MuHashExtensions;
use kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_panel_v2::palw_seat_maturity_floor_v1;
use kaspa_consensus_core::palw_state_v2::{
    PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT, PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT, PalwBondKeyV2, PalwChainStateV2,
    PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, palw_bond_registration_message_v2, palw_operator_possession_message_v1,
};
use kaspa_consensus_core::tx::{Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use kaspa_muhash::MuHash;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// The fence, armed on a copy: past the first anchor slot the script reaches (registration ~12, three
/// claims, anchor delay 20: ~36), before the claims accepted after that binding are anchored (~58).
const FENCE: u64 = 45;
/// The newcomer's harness key: registry row 8, the first row no genesis card uses.
const NEWCOMER: usize = 8;
/// The block the registration carrier rides; the chain block after it accepts it.
const CARRIER_DAA: u64 = 12;
/// The cards whose claims bind below the fence, the card whose attempt binds them, the cards whose
/// claims are accepted below the fence and anchored past it, and the card whose attempt anchors them.
const BELOW: [usize; 3] = [0, 1, 7];
const BINDS_BELOW: usize = 2;
const STRADDLE: [usize; 3] = [3, 4, 5];
const BINDS_PAST: usize = 6;
/// The collateral output's index in the carrier (output 0 is the change).
const COLLATERAL_INDEX: u32 = 1;

fn newcomer_key() -> &'static libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair {
    TestConsensus::palw_v2_registry_keypair(NEWCOMER as u64)
}

fn newcomer_pubkey() -> Vec<u8> {
    newcomer_key().verification_key.as_ref().to_vec()
}

fn newcomer_payout() -> Hash64 {
    Hash64::from_bytes(kaspa_hashes::blake2b_512_address_payload(&newcomer_pubkey()).as_bytes())
}

/// **testnet-12 with harness cards and a funded newcomer**, the fence as asked: the main premine
/// output re-addressed to the newcomer's harness payout, the genesis commitment recomputed from that
/// set exactly as the harness recomputes it for the cards' floats.
fn harness(fence: Option<u64>) -> (Config, PalwConsensusParamsV2, Premine, Premine, (TransactionOutpoint, UtxoEntry)) {
    let (config, _, mut premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    let main = premine_outpoint_for(params.net, MAIN_PREMINE_INDEX);
    let funding = {
        let (outpoint, entry) = premine.iter_mut().find(|(o, _)| *o == main).expect("testnet-12 mints a main premine output");
        entry.script_public_key = card_payout_spk(NEWCOMER);
        assert!(entry.amount > 2 * PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI, "the main output funds a genesis-card collateral");
        (*outpoint, entry.clone())
    };
    let mut multiset = MuHash::new();
    for (outpoint, entry) in &premine {
        multiset.add_utxo(outpoint, entry);
    }
    params.genesis.utxo_commitment = multiset.finalize();
    params.genesis.hash = kaspa_consensus_core::header::Header::from(&params.genesis).hash;
    assert_eq!(params.palw_bond_maturity_early, None, "testnet-12 ships the fence dormant");
    params.palw_bond_maturity_early = fence.map(ForkActivation::new);
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    config.params.validate_palw_v2().expect("the harness, armed or not, is a runnable ruleset");
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    let bundle = bundle.clone();
    (config, bundle, premine, floats, funding)
}

/// **The newcomer's registration, carried as the node's `--palw-register-bond` carries one**: the
/// collateral at output 1, paid to the payout the registration names; the bond named by index with a
/// zero id; the bond key's signature over the registration and the operator key's over possession
/// (one harness key for both), under this chain's domain; change at output 0.
fn registration_carrier(config: &Config, bundle: &PalwConsensusParamsV2, funding: &(TransactionOutpoint, UtxoEntry)) -> Transaction {
    let domain = palw_network_domain_v2_for(config.params.net.to_string().as_bytes(), Some(config.params.genesis.hash));
    let signed = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::default(), COLLATERAL_INDEX));
    let collateral = PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let pubkey = newcomer_pubkey();
    let payout = newcomer_payout();
    let classes = std::collections::BTreeSet::from([bundle.base_class_id]);
    let sign = |message: &[u8], context: &[u8]| {
        libcrux_ml_dsa::ml_dsa_87::sign(&newcomer_key().signing_key, message, context, [0x5Eu8; 32]).expect("sign").as_ref().to_vec()
    };
    let message = palw_bond_registration_message_v2(domain, &signed, &pubkey, &pubkey, collateral, &payout, &classes);
    let possession = palw_operator_possession_message_v1(domain, &signed, &pubkey, &pubkey);
    let mut signature = sign(message.as_byte_slice(), PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT);
    signature.extend(sign(possession.as_byte_slice(), PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT));
    let object = Obj::BondRegistered {
        bond: signed,
        pubkey: pubkey.clone(),
        operator_pubkey: pubkey,
        collateral,
        payout_payload: payout,
        capable_classes: classes,
        signature,
    };
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes");
    let (outpoint, entry) = funding.clone();
    let fee = 1_000_000_000; // 10 MSK
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, vec![], 0, 1)],
        vec![
            TransactionOutput::new(entry.amount - collateral - fee, card_payout_spk(NEWCOMER)),
            TransactionOutput::new(collateral, p2pkh_mldsa87_spk(payout.as_byte_slice())),
        ],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    sign_spend(&mut tx, entry, NEWCOMER, config.params.storage_mass_parameter);
    tx
}

/// One run of the script: the chain, the newcomer's bond key and registration DAA, and every chain
/// block's `(DAA, hash, PALW state root)` in order.
struct Run {
    chain: T12Chain,
    newcomer: PalwBondKeyV2,
    registered_daa: u64,
    blocks: Vec<(u64, BlockHash, Hash64)>,
}

fn sink_daa(chain: &T12Chain) -> u64 {
    chain.daa_of(chain.sink())
}

fn record(chain: &T12Chain, blocks: &mut Vec<(u64, BlockHash, Hash64)>) {
    let (hash, state) = chain.tip_state();
    blocks.push((chain.daa_of(hash), hash, state.state_root()));
}

/// Card `card`'s floor attempt, its bond's ledger printed first (the harness refuses a card with no
/// room, and this says why before it does).
async fn attempt(chain: &mut T12Chain, card: usize, blocks: &mut Vec<(u64, BlockHash, Hash64)>) {
    let facts = chain.ctx.consensus.palw_producer_facts_v2(chain.bundle.base_class_id, Some(chain.bonds[card].0));
    if let Some(bond) = facts.and_then(|f| f.bond) {
        eprintln!(
            "[t12-maturity]   card {card} at sink DAA {}: reserved {} + claim {} of ceiling {}",
            sink_daa(chain),
            bond.reserved_exposure,
            bond.claim_exposure,
            bond.exposure_ceiling
        );
    }
    let ttpb = chain.config.params.target_time_per_block();
    chain.attempt(card, ttpb, Vec::new(), &|_| true).await;
    record(chain, blocks);
}

/// Heartbeats until the sink reaches the anchor slot of every claim still unbound — the next attempt
/// block is then every such claim's anchor, and binds them all in one block (SW-8).
async fn beat_to_the_slots(chain: &mut T12Chain, blocks: &mut Vec<(u64, BlockHash, Hash64)>) {
    let anchor_delay = chain.bundle.panel.anchor_delay();
    let slot = {
        let (_, state) = chain.tip_state();
        state
            .claims_iter()
            .filter(|(_, c)| c.phase == PalwClaimPhaseV2::Provisional)
            .map(|(_, c)| c.bind_base_daa() + anchor_delay)
            .max()
            .expect("an unbound claim")
    };
    let ttpb = chain.config.params.target_time_per_block();
    while sink_daa(chain) < slot {
        chain.heartbeat(ttpb, Vec::new()).await;
        record(chain, blocks);
    }
}

async fn run(fence: Option<u64>) -> Run {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats, funding) = harness(fence);
    let ttpb = config.params.target_time_per_block();
    assert_eq!(bundle.panel.anchor_delay(), 20, "testnet-12's anchor delay, which FENCE is placed by");
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let mut blocks = Vec::new();
    while sink_daa(&chain) + 1 < CARRIER_DAA {
        chain.heartbeat(ttpb, Vec::new()).await;
        record(&chain, &mut blocks);
    }
    let carrier = registration_carrier(&config, &bundle, &funding);
    let carrying = chain.heartbeat(ttpb, vec![carrier.clone()]).await;
    eprintln!(
        "[t12-maturity] fence {fence:?}: the carrier rides the block at DAA {} (sink before it at {})",
        carrying.header.daa_score,
        blocks.last().map_or(0, |b: &(u64, BlockHash, Hash64)| b.0)
    );
    assert!(carrying.transactions.iter().any(|tx| tx.id() == carrier.id()), "the carrier rides the block");
    record(&chain, &mut blocks);
    chain.heartbeat(ttpb, Vec::new()).await; // accepts the carrying block's transactions
    record(&chain, &mut blocks);
    let newcomer = PalwBondKeyV2(TransactionOutpoint::new(carrier.id(), COLLATERAL_INDEX));
    let registered_daa = {
        let (_, state) = chain.tip_state();
        let bond = state.bond(&newcomer).unwrap_or_else(|| panic!("fence {fence:?}: the chain registered the newcomer"));
        assert_eq!(bond.collateral, PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI);
        assert!(bond.capable_classes.contains(&bundle.base_class_id), "it declares the floor");
        assert!(!chain.bonds.contains(&newcomer), "not a genesis card");
        bond.registered_daa
    };
    for card in BELOW {
        attempt(&mut chain, card, &mut blocks).await;
    }
    beat_to_the_slots(&mut chain, &mut blocks).await;
    attempt(&mut chain, BINDS_BELOW, &mut blocks).await;
    for card in STRADDLE {
        attempt(&mut chain, card, &mut blocks).await;
    }
    beat_to_the_slots(&mut chain, &mut blocks).await;
    attempt(&mut chain, BINDS_PAST, &mut blocks).await;
    Run { chain, newcomer, registered_daa, blocks }
}

/// Every panel the chain bound: `(claim, accepted DAA, anchor DAA, seats as card indices — the
/// newcomer as [`NEWCOMER`])`, in anchor order. Every claim the script made but the last attempt's
/// own must be bound — none voids for want of seats.
fn panels(r: &Run) -> Vec<(Hash64, u64, u64, Vec<usize>)> {
    let (_, state) = r.chain.tip_state();
    let mut out = Vec::new();
    for (id, claim) in state.claims_iter() {
        match (&claim.phase, state.panel(id)) {
            (PalwClaimPhaseV2::PanelBound { bound_daa }, Some(panel)) => {
                let anchor_daa = r.chain.daa_of(panel.anchor);
                assert_eq!(*bound_daa, anchor_daa, "SW-8: bound in its anchor block");
                let seats = panel
                    .seats
                    .iter()
                    .map(|s| {
                        if s.bond == r.newcomer { NEWCOMER } else { r.chain.bonds.iter().position(|b| *b == s.bond).expect("a card") }
                    })
                    .collect();
                out.push((*id, claim.accepted_daa, anchor_daa, seats));
            }
            (PalwClaimPhaseV2::Provisional, None) => {
                assert_eq!(claim.bond, r.chain.bonds[BINDS_PAST], "only the last attempt's own claim is left unbound: {id}")
            }
            (phase, _) => panic!("claim {id} (accepted {}) is {phase:?}: every claim binds", claim.accepted_daa),
        }
    }
    out.sort_by_key(|p| p.2);
    out
}

/// The newcomer is drawable for a claim anchored at `anchor_daa`: the processor's one resolution
/// (`palw_bond_maturity_window_at`, D1 on both clocks) through the one subtraction the draw makes.
fn drawable(r: &Run, state: &PalwChainStateV2, anchor_daa: u64) -> bool {
    let window = r.chain.vp().palw_bond_maturity_window_at(state, anchor_daa);
    palw_seat_maturity_floor_v1(anchor_daa, window).is_none_or(|by| r.registered_daa <= by)
}

#[tokio::test]
async fn t12_a_bond_registered_after_launch_is_drawable_below_the_fence_and_waits_its_window_past_it() {
    let armed = run(Some(FENCE)).await;
    let twin = run(None).await;
    assert_eq!(armed.registered_daa, twin.registered_daa);
    assert_eq!(armed.newcomer, twin.newcomer, "one carrier, one bond key");
    eprintln!("[t12-maturity] the newcomer registered at DAA {} (carrier at {CARRIER_DAA})", armed.registered_daa);
    assert!((CARRIER_DAA - 1..=CARRIER_DAA + 2).contains(&armed.registered_daa), "registered at DAA ~13, the drill's");

    // ---- below the fence: the release, byte for byte -------------------------------------------------
    // Two runs cannot be compared block for block (the harness's heartbeat payout is random), so the
    // RELEASED chain's own blocks are fed to an ARMED node, as a syncing peer would feed them: every
    // block is accepted and, up to the first anchor past the fence, folds the released PALW root — the
    // heartbeats between the fence's height and that anchor included (the fence moves the draw and
    // nothing else). At that anchor the armed node draws without the newcomer.
    let (config, bundle, premine, floats, _) = harness(Some(FENCE));
    let replay = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let first_past = twin.blocks.len() - 1;
    assert!(
        twin.blocks[first_past].0 >= FENCE && twin.blocks[..first_past].iter().any(|b| b.0 >= FENCE),
        "the script crosses the fence"
    );
    for (i, (daa, hash, released_root)) in twin.blocks.iter().enumerate() {
        let block = twin.chain.ctx.consensus.get_block(*hash).expect("the released node holds its chain");
        replay
            .ctx
            .consensus
            .validate_and_insert_block(block)
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("released block #{i} (DAA {daa}) was refused by an armed node: {e}"));
        assert_eq!(replay.sink(), *hash, "released block #{i} (DAA {daa}) is the armed node's sink too");
        let armed_root = replay.tip_state().1.state_root();
        if i < first_past {
            assert_eq!(
                armed_root, *released_root,
                "block #{i} (DAA {daa}): before the first anchor past the fence, the released root"
            );
        } else if panels(&twin).iter().any(|p| p.2 >= FENCE && p.3.contains(&NEWCOMER)) {
            assert_ne!(
                armed_root, *released_root,
                "block #{i} (DAA {daa}): the released draw seated the newcomer; the armed one cannot"
            );
        }
    }
    let (_, replayed) = replay.tip_state();
    for (id, _, anchor, seats) in panels(&twin) {
        let panel = replayed.panel(&id).expect("the armed node bound every claim the released chain bound");
        let armed_seats: Vec<usize> =
            panel
                .seats
                .iter()
                .map(|s| {
                    if s.bond == twin.newcomer {
                        NEWCOMER
                    } else {
                        twin.chain.bonds.iter().position(|b| *b == s.bond).expect("a card")
                    }
                })
                .collect();
        eprintln!("[t12-maturity] replay: claim {id} anchored {anchor:>3}: released seats {seats:?}, armed seats {armed_seats:?}");
        if anchor < FENCE {
            assert_eq!(armed_seats, seats, "anchored {anchor} below the fence: the released panel");
        } else {
            assert!(!armed_seats.contains(&NEWCOMER), "anchored {anchor} past the fence: the armed node never seats the newcomer");
            assert_eq!(armed_seats.len(), seats.len(), "a full jury of genesis cards");
        }
    }

    // ---- the panels ----------------------------------------------------------------------------------
    let (_, armed_state) = armed.chain.tip_state();
    let (_, twin_state) = twin.chain.tip_state();
    let armed_panels = panels(&armed);
    let twin_panels = panels(&twin);
    for (id, accepted, anchor, seats) in &armed_panels {
        eprintln!(
            "[t12-maturity] armed: claim {id} accepted {accepted:>3} anchored {anchor:>3}: seats {seats:?}; newcomer drawable {}",
            drawable(&armed, &armed_state, *anchor)
        );
    }
    for (id, accepted, anchor, seats) in &twin_panels {
        eprintln!("[t12-maturity] twin:  claim {id} accepted {accepted:>3} anchored {anchor:>3}: seats {seats:?}");
    }
    let seat_count = armed.chain.bundle.panel.seat_count() as usize;
    let (below_fence, past_fence): (Vec<_>, Vec<_>) = armed_panels.iter().partition(|p| p.2 < FENCE);
    assert_eq!(below_fence.len(), BELOW.len(), "cards 0, 1 and 7's claims bind below the fence");
    assert!(below_fence.iter().all(|p| p.2 > armed.registered_daa), "anchored after the registration");
    assert_eq!(past_fence.len(), STRADDLE.len() + 1, "cards 2, 3, 4 and 5's claims bind past it");
    assert!(past_fence.iter().all(|p| p.1 < FENCE), "each accepted below the fence and anchored past it");

    // Below the fence the newcomer is drawable, and the chain seats it.
    for (_, _, anchor, seats) in &below_fence {
        assert_eq!(
            armed.chain.vp().palw_bond_maturity_window_at(&armed_state, *anchor),
            None,
            "anchor {anchor}: no window below the fence"
        );
        assert!(
            *anchor <= armed.registered_daa || drawable(&armed, &armed_state, *anchor),
            "anchor {anchor}: drawable below the fence"
        );
        assert_eq!(seats.len(), seat_count, "anchor {anchor}: a full jury");
    }
    assert!(
        below_fence.iter().any(|p| p.3.contains(&NEWCOMER)),
        "below the fence the chain seats the newcomer (the drill's 'registered at DAA 13, seated at bind DAA 21')"
    );

    // Past the fence it is not, and the genesis seats carry every claim.
    for (id, accepted, anchor, seats) in &past_fence {
        assert_eq!(
            armed.chain.vp().palw_bond_maturity_window_at(&armed_state, *anchor),
            Some(PALW_T12_BOND_MATURITY_WINDOW_DAA),
            "anchor {anchor}: D1's window from the fence"
        );
        assert!(!drawable(&armed, &armed_state, *anchor), "anchor {anchor}: the newcomer is below its floor");
        assert_eq!(seats.len(), seat_count, "claim {id} (accepted {accepted}, anchored {anchor}): a full jury");
        assert!(!seats.contains(&NEWCOMER), "claim {id} anchored {anchor}: the newcomer is never seated past the fence");
    }
    // The twin, at the same anchors, still draws it — and seats it past the fence's height.
    for (_, _, anchor, _) in twin_panels.iter().filter(|p| p.2 >= FENCE) {
        assert_eq!(twin.chain.vp().palw_bond_maturity_window_at(&twin_state, *anchor), None, "the release: no window before 1,000");
        assert!(drawable(&twin, &twin_state, *anchor), "the release: drawable at anchor {anchor}");
    }
    assert!(
        twin_panels.iter().any(|p| p.2 >= FENCE && p.3.contains(&NEWCOMER)),
        "the released rule keeps seating the newcomer past the fence's height"
    );

    // ---- until registered + window, and one window from 1,000 ----------------------------------------
    let back = armed.registered_daa + PALW_T12_BOND_MATURITY_WINDOW_DAA;
    for anchor in [FENCE, 500, 999, 1_000, back - 1] {
        assert!(!drawable(&armed, &armed_state, anchor), "armed: not drawable at anchor {anchor}");
    }
    for anchor in [back, back + 1, 5_000] {
        assert!(drawable(&armed, &armed_state, anchor), "armed: drawable again at anchor {anchor} (registered + window)");
    }
    for anchor in [1_000, back - 1, back, 5_000] {
        assert_eq!(
            armed.chain.vp().palw_bond_maturity_window_at(&armed_state, anchor),
            twin.chain.vp().palw_bond_maturity_window_at(&armed_state, anchor),
            "anchor {anchor}: from 1,000 the armed and the released processor resolve one window"
        );
        assert_eq!(drawable(&armed, &armed_state, anchor), drawable(&twin, &armed_state, anchor), "anchor {anchor}");
    }
    eprintln!(
        "[t12-maturity] {} panels below the fence ({} seat the newcomer), {} past it (none does); drawable again at anchor {back}",
        below_fence.len(),
        below_fence.iter().filter(|p| p.3.contains(&NEWCOMER)).count(),
        past_fence.len()
    );
}
