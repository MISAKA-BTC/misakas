//! **C4 round 3 — independent adversarial review of the kernel route on the REAL node** (branch `adv/c4r3-real-node`).
//!
//! Everything here runs through the same harness as the lane's own E2E (the parent module): signed `0x4b` carriers, the mempool,
//! the node's template, the chain block's fold, the read API. A test whose name says `observation` pins behaviour that is by design
//! or already a named GAP, quantified; a test `#[ignore = "FAIL F-C4R3-nn …"]` asserts the SAFE property and fails on this
//! revision (run it with `-- --ignored`). Findings and numbers: `docs/design/palw/adversarial-e2e-record.md`, round 3.
//!
//! **The node-less path.** The user's mandatory test 3 asks that the false claim arrive as a signed object relayed by a third party
//! (`misaka-palw-remote`), not only from inside the producer's process. [`relay`] builds the producer's carrier with the producer's
//! own keys, then hands the finished bytes to `misaka_palw_remote::relay::broadcast_signed_tx` — the library a node-less miner uses —
//! which pre-flights the funding signature, fans out to every node it knows (here: one that lies about what it accepted, and the
//! real node's mempool), and judges each reply against the id it computed from the bytes. Only what the real node accepted is mined.
use super::*;
use kaspa_consensus_core::config::params::Params;
use misaka_palw_remote::relay::{NodeOutcome, RelayNode, Reply, broadcast_signed_tx, tx_id_of_bytes};

// ---- the node-less path --------------------------------------------------------------------------------------------------

/// The real node as a relay target: the mempool's own check, and what it accepted is what its next template carries.
struct NodeRelay<'a> {
    net: &'a Net,
    accepted: std::cell::RefCell<Vec<Transaction>>,
}

impl RelayNode for NodeRelay<'_> {
    fn node_id(&self) -> &str {
        "the-real-node"
    }
    fn submit_raw_tx(&self, tx: &Transaction) -> Reply {
        match self.net.mempool(tx) {
            Ok(()) => {
                self.accepted.borrow_mut().push(tx.clone());
                Reply::Accepted(tx_id_of_bytes(tx))
            }
            Err(e) => Reply::Refused(e.to_string()),
        }
    }
}

/// A relay that claims to have accepted something else (it relayed other bytes, or lies): never counted, always reported.
struct LyingRelay;

impl RelayNode for LyingRelay {
    fn node_id(&self) -> &str {
        "a-lying-relay"
    }
    fn submit_raw_tx(&self, _tx: &Transaction) -> Reply {
        Reply::Accepted(Hash64::from_u64_word(0xBAD))
    }
}

/// **Relay one signed object node-less**: the carrier is built and signed with card `card`'s keys, then handed — as finished bytes —
/// to the remote library's broadcast; the real node's acceptance is the only one that counts. The carrying block is mined, then the
/// block that folds it (the harness's `send` contract).
async fn relay(net: &mut Net, card: usize, object: &Obj) {
    let funding = net.funding[card].1.clone();
    let tx = net.carrier(card, object);
    let (report, accepted) = {
        let node = NodeRelay { net, accepted: Default::default() };
        let report =
            broadcast_signed_tx(&tx, Some(&funding), &[&LyingRelay, &node], 1).expect("the real node takes the relayed carrier");
        (report, node.accepted.into_inner())
    };
    assert_eq!(report.id, tx_id_of_bytes(&tx), "the relay judges replies against the id of its own bytes");
    assert_eq!(report.tampered(), vec!["a-lying-relay"], "the lying relay is reported, never counted");
    assert!(report.per_node.iter().any(|(n, o)| n == "the-real-node" && *o == NodeOutcome::Accepted));
    assert_eq!(accepted.len(), 1);
    let ttpb = net.ttpb();
    let carrying = net.chain.heartbeat(ttpb, accepted).await;
    assert!(carrying.transactions.iter().any(|t| t.id() == tx.id()), "the node's template carries the relayed carrier");
    net.chain.heartbeat(ttpb, Vec::new()).await;
}

impl World {
    /// [`World::claim`], but the producer runs no node: its seal and its reveal reach the chain only through [`relay`].
    async fn relayed_claim(&mut self, producer: usize, job: &KernelJobV1, lie: bool) -> Claim {
        let ledger = self.net.ledger();
        let generated = greedy(&self.fx, &ledger, &self.class, &job.prompt, job.max_new_tokens as usize);
        let at = matmul_at(&self.fx.program, 1);
        let kid = self.net.kid(producer);
        let produced = produce(&self.fx, &ledger, &self.class, job, kid, generated, |t| {
            if lie {
                bump(&mut t.values[at.0 as usize][at.1 as usize][at.2 as usize], 1)
            }
        });
        let id = produced.claim.id();
        let seal = self.net.route(producer, &K::SealClaim { producer: kid, job: produced.claim.job_id, seal: claim_seal_v1(&id) });
        relay(&mut self.net, producer, &seal).await;
        let reveal = self.net.route(producer, &produced.object);
        relay(&mut self.net, producer, &reveal).await;
        assert!(self.net.ledger().claims.contains_key(&id), "the relayed claim committed over its relayed seal");
        Claim { id, producer, trace: produced.trace, at }
    }

    /// The outsider's proof, relayed node-less too.
    async fn relayed_proof(&mut self, card: usize, claim: &Digest, proof: ProsecutionV1) {
        let o = self.net.route(card, &K::FileProof { accuser: self.net.kid(card), claim: *claim, proof });
        relay(&mut self.net, card, &o).await;
    }
}

// ---- mandatory test 3 (user): producer + whole Panel collude; an outside bonded verifier convicts from public material ----

/// **Mandatory test 3, Panel-licensed, node-less.** The producer runs no node: its signed seal and lying reveal reach the chain
/// through the remote relay library (one relay lies and is reported). Every interim seat signs a passing receipt. An ordinary
/// bonded outsider — no seat, no producer state, no `ServedView`, only the read API and the producer's public DA — fresh-verifies,
/// relays its proof the same node-less way, and the REAL producer bond is slashed; nothing lands on the relay or on the seats.
#[tokio::test]
async fn g14_c4r3_mandatory_3_a_relayed_lie_covered_by_every_seat_is_convicted_by_an_outsider() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let before = w.net.collateral(0);
    let lie = w.relayed_claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    let seat_collateral: Vec<u64> = seats.iter().map(|s| w.net.collateral(*s)).collect();
    w.cover(&lie.id).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0xC4);
    w.relayed_proof(outsider, &lie.id, proof).await;
    let pol = w.policy();
    assert!(w.net.ledger().claims[&lie.id].convicted, "convicted from public material alone");
    assert_eq!(w.net.collateral(0), before - pol.claim_collateral, "the real producer bond pays the whole reservation");
    assert_eq!(w.net.owed(outsider), pol.claim_collateral * u64::from(pol.accuser_reward_permille) / 1000);
    for (s, c) in seats.iter().zip(seat_collateral) {
        assert_eq!(w.net.collateral(*s), c, "the interim seats are not the route's to slash (G14 does not rest on them)");
    }
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **Mandatory test 3, OPV (Panel = 0), node-less, before and after Final.** Two lies relayed by their producer: one is convicted
/// inside the window, the other finalizes (nobody looked) and is convicted inside the liability horizon — both by an outsider
/// built from the read API, both relayed.
#[tokio::test]
async fn g14_c4r3_mandatory_3_opv_relayed_lies_are_convicted_before_and_after_final() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::opv().await;
    let job_a = w.job().await;
    let job_b = w.job().await;
    let before = w.net.collateral(0);
    let a = w.relayed_claim(0, &job_a, true).await;
    let b = w.relayed_claim(0, &job_b, true).await;
    let outsider = w.outsiders(&a, &[], 1)[0];
    let (policy, view_b) = opv_view(&w, &b.id);
    let proof = w.prosecution(&a.id, &a.published(&w.fx, &[]), 0xA1);
    w.relayed_proof(outsider, &a.id, proof).await;
    assert!(matches!(w.net.claim_state(&a.id), ClaimStateV1::Convicted { .. }), "{:?}", w.net.claim_state(&a.id));
    w.net.beat_to(view_b.final_floor_daa).await;
    assert!(matches!(w.net.claim_state(&b.id), ClaimStateV1::Final { .. }), "b finalized unprosecuted");
    let proof = w.prosecution(&b.id, &b.published(&w.fx, &[]), 0xB2);
    w.relayed_proof(outsider, &b.id, proof).await;
    assert!(w.net.ledger().claims[&b.id].convicted, "b convicted inside its liability horizon");
    let r = policy.economics.reservation_per_claim;
    assert_eq!(w.net.collateral(0), before - 2 * r, "both reservations slashed from the real bond");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

// ---- G14 pre-emption: a self-inflicted default ------------------------------------------------------------------------------

/// The pieces of the pre-emption run, measured.
struct Escape {
    producer_loss: u64,
    sybil_gain: i128,
    outsider_fee: u64,
    convicted: bool,
    state: ClaimStateV1,
    censored_daa: u64,
}

/// **The colluders' cheapest escape from a provable fraud**: the producer publishes EVERYTHING (the outsider can convict from public
/// material alone), and the colluders' own bond (`sybil`, any operator) files a demand on a position as soon as the claim is open;
/// the producer stays silent. The outsider's proof is kept out of the chain (censored, or merely slow) until the demand's deadline.
/// Then the outsider files.
async fn escape_by_self_default(mut w: World, cover: bool) -> Escape {
    let job = w.job().await;
    let before = w.net.collateral(0);
    let lie = w.claim(0, &job, true).await;
    let seats = if cover { w.seats(&lie.id) } else { Vec::new() };
    if cover {
        w.cover(&lie.id).await;
    }
    let cards = w.outsiders(&lie, &seats, 2);
    let (sybil, outsider) = (cards[0], cards[1]);
    let sybil_before = w.net.collateral(sybil) as i128 + w.net.owed(sybil) as i128;
    // The outsider holds a valid proof from the first block the claim is open.
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0xE5);
    let opened = w.net.daa();
    w.demand(sybil, &lie.id, 0).await;
    let deadline = w.net.ledger().demands[&(lie.id, 0, 0)].deadline_daa;
    w.net.beat_to(deadline).await;
    // Measured at the default's block, before the next coinbase pays the queued share out of the bond's view.
    let sybil_gain = w.net.collateral(sybil) as i128 + w.net.owed(sybil) as i128 - sybil_before;
    let producer_loss = before - w.net.collateral(0);
    let outsider_before = w.net.collateral(outsider);
    w.proof(outsider, &lie.id, proof).await;
    let ledger = w.net.ledger();
    Escape {
        producer_loss,
        sybil_gain,
        outsider_fee: outsider_before - w.net.collateral(outsider),
        convicted: ledger.claims[&lie.id].convicted,
        state: ledger.claims[&lie.id].life.state.clone(),
        censored_daa: deadline - opened,
    }
}

/// **F-C4R3-02 (P1, Panel-licensed): a provable fraud is laundered into a FREE availability default by keeping one valid proof out
/// of the chain for `court_deadline_daa` (20 DAA), not for the window plus the liability horizon (250 DAA).** The colluders' bond
/// demands a position and the producer stays silent: at the deadline the claim is `Unavailable`, the producer pays `default_penalty`
/// (100 KAS) — and the colluders' bond is paid the whole of it back as the sole demander. The reservation (1,000 KAS) is released.
/// The outsider's TRUE fraud proof, filed one block later, is then **dismissed with the filing fee** ("nothing is reserved any
/// more"): the honest prosecutor is the only party that pays. SAFE property asserted: a valid proof of a lie filed inside the
/// claim's own horizon convicts it, and an honest prosecutor is never charged for a true proof.
#[tokio::test]
#[ignore = "FAIL F-C4R3-02: a self-inflicted default releases the reservation and dismisses a later true proof with a fee"]
async fn g14_c4r3_a_self_inflicted_default_must_not_erase_a_provable_fraud() {
    kaspa_core::log::try_init_logger("warn");
    let e = escape_by_self_default(World::new().await, true).await;
    eprintln!(
        "[F-C4R3-02 panel] censored {} DAA; producer lost {}; colluding demander gained {}; outsider paid {}; convicted {}; {:?}",
        e.censored_daa, e.producer_loss, e.sybil_gain, e.outsider_fee, e.convicted, e.state
    );
    assert!(e.convicted, "a valid proof inside the claim's horizon convicts it (it was {:?})", e.state);
    assert_eq!(e.outsider_fee, 0, "a true fraud proof never costs its filer the dismissal fee");
}

/// F-C4R3-02 on an OPV class: the same escape, with RFC-0015's 10 % default burn the colluders' only cost.
#[tokio::test]
#[ignore = "FAIL F-C4R3-02: a self-inflicted default releases the reservation and dismisses a later true proof with a fee (OPV)"]
async fn g14_c4r3_opv_a_self_inflicted_default_must_not_erase_a_provable_fraud() {
    kaspa_core::log::try_init_logger("warn");
    let e = escape_by_self_default(World::opv().await, false).await;
    eprintln!(
        "[F-C4R3-02 opv] censored {} DAA; producer lost {}; colluding demander gained {}; outsider paid {}; convicted {}; {:?}",
        e.censored_daa, e.producer_loss, e.sybil_gain, e.outsider_fee, e.convicted, e.state
    );
    assert!(e.convicted, "a valid proof inside the claim's horizon convicts it (it was {:?})", e.state);
    assert_eq!(e.outsider_fee, 0, "a true fraud proof never costs its filer the dismissal fee");
}

// ---- GAP-R7: proof front-running -----------------------------------------------------------------------------------------

/// **Observation, GAP-R7 quantified (open by design until the accuser seal).** A `FileProof`'s proof bytes name no accuser: anyone who
/// sees the honest outsider's carrier (every relay, every block producer) lifts the proof out of the PUBLIC carrier bytes, re-signs it
/// under its own bond and gets it into a block first. Measured on the real path: the copyist is paid the whole bounty (half the
/// reservation, 500 KAS), the outsider's own filing is a `Duplicate` worth nothing (it pays only its carrier fee). When the copyist is
/// the producer's own bond (the colluders mine, or simply pay a higher fee), the lie costs the colluders HALF its reservation and the
/// honest verifier is never paid — under any rational block producer the outsider's expected bounty is ~0.
#[tokio::test]
async fn g14_c4r3_observation_gap_r7_a_lifted_proof_takes_the_bounty_and_halves_the_colluders_loss() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let before = w.net.collateral(0);
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let cards = w.outsiders(&lie, &seats, 2);
    let (copyist, outsider) = (cards[0], cards[1]);
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x7A);
    let honest = w.net.route(outsider, &K::FileProof { accuser: w.net.kid(outsider), claim: lie.id, proof });
    // What a mempool watcher sees: the honest carrier's object bytes. The proof is lifted from them, nothing else.
    let Obj::KernelRouteV1 { bytes, .. } = &honest else { unreachable!() };
    let K::FileProof { proof: lifted, .. } = K::decode(bytes).expect("public bytes decode") else { panic!("a FileProof") };
    let copy = w.net.route(copyist, &K::FileProof { accuser: w.net.kid(copyist), claim: lie.id, proof: lifted });
    let outsider_before = w.net.collateral(outsider);
    w.net.send(vec![(copyist, copy)]).await; // the copy wins inclusion
    // (measured in the folding block, before the next coinbase pays the queue out)
    let copyist_paid = w.net.owed(copyist);
    w.net.send(vec![(outsider, honest)]).await;
    let pol = w.policy();
    let bounty = pol.claim_collateral * u64::from(pol.accuser_reward_permille) / 1000;
    assert!(w.net.ledger().claims[&lie.id].convicted);
    assert_eq!(copyist_paid, bounty, "the copyist is paid the whole bounty");
    assert_eq!(w.net.owed(outsider), 0, "the verifier who did the work is paid nothing");
    assert_eq!(w.net.collateral(outsider), outsider_before, "(its filing is a Duplicate: no fee either)");
    let colluders_net = (before - w.net.collateral(0)) - bounty;
    assert_eq!(colluders_net, pol.claim_collateral / 2, "self-prosecution halves the colluders' loss: {colluders_net}");
}

// ---- consensus robustness: the route's header and the next release -------------------------------------------------------

/// The node is shut down and started again over the same database as the NEXT release: `edit` changes its parameters the way a
/// release that schedules a further fence does (the parameter fingerprint moves; nothing in force moves).
fn restart_as_next_release(w: World, db: std::sync::Arc<kaspa_database::prelude::DB>, edit: impl FnOnce(&mut Params)) -> World {
    let World { net, fx, class, jobs } = w;
    let Net { chain, config, bundle, premine, floats, funding, domain, rnd, _keep } = net;
    let (simulated_time, nonce) = (chain.ctx.simulated_time, chain.nonce_for_reopen());
    drop(chain);
    let mut params = config.params.clone();
    edit(&mut params);
    let config = Config::new(params);
    let mut resumed = config.clone();
    resumed.process_genesis = false;
    let (sender, receiver) = async_channel::unbounded();
    let second = TestConsensus::with_db(db, &resumed, sender);
    let chain = t12_reopened_chain(second, &resumed, &bundle, simulated_time, nonce);
    let mut keep = _keep;
    keep.push(Box::new(receiver));
    let net = Net { chain, config, bundle, premine, floats, funding, domain, rnd, _keep: keep };
    World { net, fx, class, jobs }
}

/// **F-C4R3-01 (P0 wherever the route is armed — latent: the fence is refused at every real height): the first release after
/// activation that schedules ANY further fence halts every node that takes it.** The route's ledger policy binds
/// `ruleset_digest = Params::consensus_params_id()`, the stored route header keeps it, and `ensure_route_header` refuses a stored
/// route "folded under another policy". But `consensus_params_id` hashes every SCHEDULED fence (that is how the int-13 re-pin moved
/// testnet-12's from `5ee7fd8e…` to `2e567642…` with nothing in force changing). So the upgraded node's closing tick fails on the
/// first block with a live claim or demand — the block is invalid for it — while un-upgraded nodes accept it: a split at the binary
/// swap, not at any height. SAFE property asserted: a release that schedules an unrelated future fence keeps folding the route and
/// the outsider still convicts.
#[tokio::test]
#[ignore = "FAIL F-C4R3-01: the upgraded node disqualifies the next block (the stored kernel route was folded under another policy)"]
async fn g14_c4r3_a_release_that_schedules_an_unrelated_fence_keeps_the_route_folding() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut net = Net::over(|c| TestConsensus::with_db(db.clone(), c, sender));
    net._keep.push(Box::new(receiver));
    let mut w = World::on(net).await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x01);
    let id_before = w.net.config.params.consensus_params_id();
    let sink = w.net.chain.sink();

    // The next release schedules an unrelated fence far in the future (any flag-day release does this).
    let mut w = restart_as_next_release(w, db.clone(), |p| p.palw_signed_registration_v1 = Some(ForkActivation::new(1_000_000)));
    assert_ne!(w.net.config.params.consensus_params_id(), id_before, "the release moved the parameter fingerprint");
    assert_eq!(w.net.chain.sink(), sink);
    // It carries on: a block over the live claim folds, and the outsider's proof convicts.
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "the upgraded node still folds the route");
}

// ---- OPV: one global live-claim cap ---------------------------------------------------------------------------------------

/// **F-C4R3-05 (P2 DoS, OPV lane capture).** RFC-0015's `max_live_claims_total` is ONE network-wide counter, a claim counts while its
/// reservation is held — through the window AND the whole liability horizon (≈ 250 DAA at the interim terms) — and jobs are free.
/// So `⌈total / per_producer⌉` bonds (11 at the interim 32 / 3) fill the lane with HONEST claims on jobs they posted themselves, and
/// every other producer's OPV claim is refused for as long as they keep refilling; the occupiers are even paid the (unfunded)
/// `claim_reward` at each Final. The harness has eight cards, so the cap is scaled (total 6, per producer 3: two occupying bonds);
/// the relation is the same. SAFE property asserted: an honest producer outside the occupiers can still get an OPV claim admitted.
#[tokio::test]
#[ignore = "FAIL F-C4R3-05: two bonds' self-posted honest claims hold the OPV lane's global cap; a third producer is refused"]
async fn g14_c4r3_opv_two_bonds_must_not_be_able_to_hold_the_whole_opv_lane() {
    kaspa_core::log::try_init_logger("warn");
    let mut fence = PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(1), opv_admitted());
    fence.economics.max_live_claims_total = 6;
    fence.economics.max_live_claims_per_producer = 3;
    let mut w = World::on_opv(Net::over_cfg(kernel_config_with(Some(fence)), TestConsensus::new)).await;
    let mut held = Vec::new();
    for occupier in [2usize, 3] {
        for _ in 0..3 {
            let job = w.job().await;
            held.push(w.claim(occupier, &job, false).await);
        }
    }
    let ledger = w.net.ledger();
    assert_eq!(ledger.opv_live_counts(&w.net.kid(2)).1, 6, "six live OPV claims: the lane is full");
    let job = w.job().await;
    // An honest producer outside the occupiers now tries.
    let honest = w.claim_with(4, &job, false, Delivery::Direct).await;
    let admitted = w.net.ledger().claims.contains_key(&honest.id);
    let (_, view) = opv_view(&w, &held[0].id);
    eprintln!(
        "[F-C4R3-05] lane full: honest claim admitted = {admitted}; an occupier's slot frees only at Final + liability (Final {} , liability {} DAA)",
        view.final_floor_daa,
        w.policy().liability_daa
    );
    assert!(admitted, "an honest producer outside two occupying bonds can still have an OPV claim admitted");
}

/// **F-C4R3-01, second instance (G-RULESET, tag 108).** The envelope must name `Params::consensus_params_id`, which moves with every
/// SCHEDULED fence. During a rollout the fleet runs two builds that differ only in a future fence (exactly what the handshake keeps
/// as peers, M1-6): an envelope signed for the old build's id is registered by old nodes and dropped by upgraded ones, so the two
/// compute different PALW roots for the same block and the upgraded node disqualifies it — a split at the binary swap, at no height.
/// SAFE property asserted: a node of the next release (one more fence, far in the future) follows the same chain.
#[tokio::test]
#[ignore = "FAIL F-C4R3-01 (G-RULESET): an envelope in flight during a rollout splits builds that differ only in a future fence"]
async fn g14_c4r3_an_envelope_in_flight_must_not_split_builds_that_differ_only_in_a_future_fence() {
    kaspa_core::log::try_init_logger("warn");
    let f = onb_fixture(11);
    let mut net = Net::over_cfg(kernel_config_onboarding(), TestConsensus::new);
    net.beat_to(1).await;
    let registrant = 1usize;
    let params_id = net.config.params.consensus_params_id();
    let class_obj = net.v2_registration(&f, registrant, net.daa() + 30);
    let Obj::ClassRegisteredTirV1 { class_id: v2_class, .. } = &class_obj else { unreachable!() };
    let v2_class = *v2_class;
    let envelope = net.envelope(registrant, class_obj, net.daa() + 200, params_id);
    net.send(vec![(registrant, envelope)]).await;
    assert!(net.chain.tip_state().1.class(&v2_class).is_some(), "the old build registered the class through the envelope");
    // The next chain block commits the root of that state (a header commits its selected parent's root).
    let ttpb = net.ttpb();
    net.chain.heartbeat(ttpb, Vec::new()).await;
    // The next release: the same rules in force, one more fence scheduled far ahead.
    let mut next = net.config.params.clone();
    next.palw_receipt_spend_v4 = Some(ForkActivation::new(1_000_000));
    assert_ne!(next.consensus_params_id(), params_id);
    assert_eq!(next.consensus_identity_id(), net.config.params.consensus_identity_id(), "the handshake keeps the two builds as peers");
    let upgraded = t12_genesis_chain(&Config::new(next), &net.bundle, &net.premine, &net.floats);
    for b in chain_blocks(&net.chain, net.chain.sink()) {
        arrive(&upgraded, b, "a block of the old build").await;
    }
    eprintln!(
        "[F-C4R3-01 envelope] upgraded sink == old sink: {}; class registered on the upgraded node: {}; PALW roots equal: {}",
        upgraded.sink() == net.chain.sink(),
        upgraded.tip_state().1.class(&v2_class).is_some(),
        upgraded.tip_state().1.state_root() == net.chain.tip_state().1.state_root()
    );
    assert_eq!(upgraded.sink(), net.chain.sink(), "the upgraded node follows the chain the old build produced");
    assert_eq!(upgraded.tip_state().1.state_root(), net.chain.tip_state().1.state_root(), "and agrees about its PALW state");
}
