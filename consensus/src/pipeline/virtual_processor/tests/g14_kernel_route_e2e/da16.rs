//! **Lane DA16 on the real node: the claim-material provider court (tags 150–153, `palw_provider_court_v1`) and, off chain, the
//! public-material transport.**
//!
//! ```text
//! artifact:  NOT a consensus matter (ADR-0177): 104 binds with no provider; an `Artifact` lease / challenge / answer is refused; the
//!            transport (misaka-palw-remote::public_material) is optional tooling — an outsider confirms a binding from the bytes, or
//!            refutes a false one (tag 105) from its OWN copy and the binder's OWN published tree
//! claim:     150 leases of a kernel claim → 153 moves its DA responsibility (irreversible) → an unanswered demand is the PROVIDERS'
//!            default (never the producer's); every lease charged on 151s → the claim lapses (void, never convicted; each charge
//!            burned whole); served bytes still convict a false computation (the miner's, through the kernel's unchanged FileProof)
//! ```
//!
//! The fences are test-armed through the harness's `Config` seam, WITHOUT their validation (which refuses every height), as everywhere
//! in this file. The transport runs over directory providers in a temp dir (the reference HTTP server shares the same layout and gate).

use super::*;
use kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1;
use kaspa_consensus_core::palw_provider_court_v1::{
    PALW_PROVIDER_CHALLENGE_BOND_SOMPI_V1, PALW_PROVIDER_CHALLENGE_FEE_SOMPI_V1, PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1,
    PALW_PROVIDER_COURT_TABLE_LEASES_V1, PALW_PROVIDER_LEASE_MIN_RESERVATION_SOMPI_V1, ProviderCourtReadV1, ProviderSubjectV1,
    palw_kernel_claim_horizon_bound_v1, palw_provider_court_message_v1, palw_provider_reporter_share_v1,
};
use kaspa_consensus_core::palw_public_material_v1::{
    ArtifactFactsV1, ArtifactManifestV1, BindingCheckV1, PublicUnitAnswerV1, PublicUnitV1, answer_artifact_unit_v1,
    differing_instances_v1, differing_rows_v1, row_refutation_v1, verify_artifact_answer_v1,
};
use misaka_palw_remote::evidence::fs::FsProvider;
use misaka_palw_remote::public_material::{
    MaterialProvider, artifact_leaf_path, check_binding_v1, fetch_artifact_unit_v1, fetch_artifact_v1, fetch_claim_position_v1,
    publish_artifact_v1, publish_claim_positions_v1,
};

const REGISTRANT: usize = 1;

// ---- the networks ----------------------------------------------------------------------------------------------------------

/// The onboarding network (route, IR and envelope fences) with the provider court armed at genesis.
fn court_onboarding_config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = kernel_config_onboarding();
    let mut params = config.params.clone();
    params.palw_provider_court_v1 = Some(ForkActivation::new(0));
    assert!(params.validate_palw_provider_court_v1().is_err(), "the court's own validation refuses every armed height");
    (Config::new(params), bundle, premine, floats)
}

/// `World`'s kernel network with the provider court armed at `at`.
fn court_kernel_config(at: u64) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = kernel_config();
    let mut params = config.params.clone();
    params.palw_provider_court_v1 = Some(ForkActivation::new(at));
    assert!(params.validate_palw_v2().is_err());
    (Config::new(params), bundle, premine, floats)
}

impl Net {
    /// A provider-court object's signature by card `card` (`kind` the tag, `payload` the Borsh of the object's other fields).
    fn court_signature(&mut self, card: usize, kind: u8, payload: &[u8]) -> Vec<u8> {
        self.rnd = self.rnd.wrapping_add(1);
        let signer = self.bond(card);
        let message = palw_provider_court_message_v1(self.domain, kind, &signer, payload);
        let key = TestConsensus::palw_v2_registry_keypair(card as u64);
        libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            message.as_byte_slice(),
            PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1,
            [self.rnd; 32],
        )
        .expect("ML-DSA-87 signs")
        .as_ref()
        .to_vec()
    }

    fn court_lease(&mut self, card: usize, subject: ProviderSubjectV1, reserved: u64, serve_until_daa: u64) -> Obj {
        let signature = self.court_signature(card, 150, &borsh::to_vec(&(subject, reserved, serve_until_daa)).unwrap());
        Obj::ProviderLeaseV1 { subject, reserved, serve_until_daa, provider: self.bond(card), signature }
    }

    /// A challenge by card `card` of `unit` of card `provider`'s lease, valid for ten DAA (inside one response window).
    fn court_challenge(&mut self, card: usize, subject: ProviderSubjectV1, provider: usize, unit: PublicUnitV1) -> Obj {
        let (provider, valid_until_daa) = (self.bond(provider), self.daa() + 10);
        let signature = self.court_signature(card, 151, &borsh::to_vec(&(subject, provider, unit, valid_until_daa)).unwrap());
        Obj::ProviderChallengeV1 { subject, provider, unit, valid_until_daa, challenger: self.bond(card), signature }
    }

    fn court_answer(&mut self, card: usize, subject: ProviderSubjectV1, unit: PublicUnitV1, answer: PublicUnitAnswerV1) -> Obj {
        let signature = self.court_signature(card, 152, &borsh::to_vec(&(subject, unit, &answer)).unwrap());
        Obj::ProviderAnswerV1 { subject, provider: self.bond(card), unit, answer: Box::new(answer), signature }
    }

    fn da_transfer(&mut self, card: usize, claim: Hash64) -> Obj {
        let signature = self.court_signature(card, 153, &borsh::to_vec(&claim).unwrap());
        Obj::DaTransferV1 { claim, producer: self.bond(card), signature }
    }

    /// The court's rows of `subject`, through the node's read API.
    fn court(&self, subject: &ProviderSubjectV1) -> ProviderCourtReadV1 {
        self.api().expect("the route").provider_court_read_v1(subject)
    }

    /// What the court holds against card `card`'s bond (V2's committed-collateral term).
    fn court_reserved(&self, card: usize) -> u128 {
        self.chain.tip_state().1.provider_court_reserved(&self.bond(card))
    }
}

/// The V2 inventory leaves of `f`'s weights (what a publisher places).
fn onb_leaves(f: &OnbFixture) -> Vec<PalwArtifactOperandV1> {
    use super::super::g14_registration_e2e::Tensors;
    let tensors = f.params.tensors.iter().map(|(k, t)| (*k, t.to_le_bytes())).collect();
    kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_operands_v1(&f.program, &Tensors(tensors)).expect("the inventory")
}

/// Directory providers under a fresh temp dir.
struct Dirs {
    base: std::path::PathBuf,
    providers: Vec<FsProvider>,
}

impl Dirs {
    fn new(tag: &str, n: usize) -> Dirs {
        let base = std::env::temp_dir().join(format!("da16-e2e-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        Dirs { providers: (0..n).map(|i| FsProvider::new(base.join(format!("provider-{i}")))).collect(), base }
    }

    fn refs(&self) -> Vec<&dyn MaterialProvider> {
        self.providers.iter().map(|p| p as &dyn MaterialProvider).collect()
    }
}

impl Drop for Dirs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// The V2 class of `f` registered by the registrant; returns its id.
async fn register_v2(net: &mut Net, f: &OnbFixture) -> Hash64 {
    let class_obj = net.v2_registration(f, REGISTRANT, net.daa() + 30);
    let Obj::ClassRegisteredTirV1 { class_id, .. } = &class_obj else { unreachable!() };
    let v2_class = *class_id;
    net.send(vec![(REGISTRANT, class_obj)]).await;
    assert!(net.chain.tip_state().1.class(&v2_class).is_some(), "the V2 class registered");
    v2_class
}

// ---- the artifact: not a consensus matter (ADR-0177) ------------------------------------------------------------------------

/// The aux rows of the court's tables 43–45 the route holds.
fn court_rows(net: &Net) -> usize {
    net.api().map(|r| r.aux.keys().filter(|(t, _)| (PALW_PROVIDER_COURT_TABLE_LEASES_V1..=45).contains(t)).count()).unwrap_or(0)
}

/// **ADR-0177 non-interference: model availability never gates a binding, and the artifact subject is withdrawn.** Past the provider
/// court's fence, tag 104 binds with no provider at all and matures into an attested root; an `Artifact` lease, a challenge of it and an
/// answer are each refused (no row in tables 43–45, nothing reserved, nothing charged); and whether any provider serves the bytes (here
/// every directory is deleted before maturity) changes nothing the chain holds. The transport stays as optional, non-consensus tooling:
/// an outsider fetches the artifact cold from the providers it chose (one lying about a leaf, one empty) and confirms the binding from
/// the bytes against the CHAIN's roots, off chain. A replaying node agrees.
#[tokio::test]
async fn da16_model_availability_never_gates_a_binding_and_the_artifact_subject_is_refused() {
    kaspa_core::log::try_init_logger("warn");
    let truth = onb_fixture(11);
    let leaves = onb_leaves(&truth);
    let mut net = Net::over_cfg(court_onboarding_config(), TestConsensus::new);
    net.beat_to(1).await;
    let (p1, outsider) = (2usize, 4usize);
    let v2_class = register_v2(&mut net, &truth).await;
    let kernel_root = Hash64::from_bytes(truth.pc.root());
    let subject = ProviderSubjectV1::Artifact { v2_class, kernel_param_root: kernel_root };

    // ---- the binding: no lease, no provider — bound ----
    let o = net.artifact_bound(REGISTRANT, v2_class, kernel_root);
    net.send(vec![(REGISTRANT, o)]).await;
    let binding = net.api().unwrap().artifact_binding_v1(&v2_class, &kernel_root).expect("bound with no provider at all");

    // ---- the withdrawn subject: a lease, a challenge, an answer — each refused, whatever it names ----
    let serve_until = net.daa() + 400;
    let lease = net.court_lease(p1, subject, PALW_PROVIDER_LEASE_MIN_RESERVATION_SOMPI_V1, serve_until);
    net.send(vec![(p1, lease)]).await;
    let challenge = net.court_challenge(outsider, subject, p1, PublicUnitV1::ArtifactLeaf { index: 0 });
    let commitments = answer_artifact_unit_v1(&truth.program, &leaves, &PublicUnitV1::KernelCommitments).unwrap();
    let answer = net.court_answer(p1, subject, PublicUnitV1::KernelCommitments, commitments);
    net.send(vec![(outsider, challenge), (p1, answer)]).await;
    assert_eq!(court_rows(&net), 0, "no row: model availability is not a consensus matter (ADR-0177 D1)");
    assert_eq!((net.court_reserved(p1), net.court_reserved(outsider)), (0, 0), "nothing reserved, nothing to charge");
    assert_eq!((net.slashed(p1), net.slashed(outsider)), (0, 0));

    // ---- the transport, off chain: providers place the bytes; an outsider confirms the binding from them ----
    let dirs = Dirs::new("honest", 3);
    let manifest =
        ArtifactManifestV1::of(net.domain, truth.class.clone(), truth.artifact_root, kernel_root, &leaves, net.daa() + 1_000);
    publish_artifact_v1(&dirs.refs()[..2], &manifest, &leaves, &[], 2).expect("two verified copies");
    let mut lie = leaves[1].clone();
    lie.bytes[0] ^= 0x5A;
    dirs.providers[1].put(&artifact_leaf_path(&truth.artifact_root, 1), &borsh::to_vec(&lie).unwrap()).unwrap();
    let chain_root = net.chain.tip_state().1.class(&v2_class).unwrap().artifact_root;
    let order: Vec<&dyn MaterialProvider> = vec![dirs.refs()[2], dirs.refs()[1], dirs.refs()[0]];
    let fetched = fetch_artifact_v1(&order, net.domain, v2_class, chain_root, Some(kernel_root), Hash64::from_bytes([7; 64]))
        .expect("every leaf from some provider, each checked alone");
    assert_eq!(fetched.leaves, leaves, "whoever served them, the bytes are the class's");
    assert_eq!(check_binding_v1(&fetched, kernel_root).unwrap(), BindingCheckV1::Confirmed, "CONFIRMED from the bytes, off chain");

    // ---- every provider gone: the binding matures and attests exactly as it would have ----
    drop(dirs);
    net.beat_to(binding.matures_daa).await;
    assert_eq!(net.api().unwrap().onboarding_attested_roots_v1(net.daa()), vec![kernel_root], "attested: nobody served anything");
    assert_eq!(court_rows(&net), 0);
    let z = net.replay().await;
    net.assert_same(&z, "replay");
}

/// **A false binding is refuted from the verifier's OWN bytes and the binder's OWN published tree — no provider, no court demand.** The
/// binder binds OTHER weights' kernel commitments to its class (no lease exists or is needed). The outsider holds the true bytes (its own
/// copy, fetched off chain and checked against the CHAIN's artifact root), sees the roots differ, and reads the bound side from what the
/// binder itself published, each unit checked against the BOUND root by the court's own function (commitments, a run of row leaves, one
/// row); it localizes a differing row and refutes through tag 105: the binder is slashed and the refuter paid. (Had the binder published
/// nothing, no court could force its tree out any more — ADR-0177 forbids model-bytes demands; design doc §7.7 proposes the hash-only
/// binding demand that closes it.)
#[tokio::test]
async fn da16_a_false_binding_is_refuted_from_the_verifiers_own_bytes_and_the_binders_published_tree() {
    use kaspa_consensus_core::palw_onboarding_v1::{
        PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1 as RESERVED, PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1 as REWARD,
    };
    kaspa_core::log::try_init_logger("warn");
    let (truth, wrong) = (onb_fixture(11), onb_fixture(12));
    let (leaves, wrong_leaves) = (onb_leaves(&truth), onb_leaves(&wrong));
    let mut net = Net::over_cfg(court_onboarding_config(), TestConsensus::new);
    net.beat_to(1).await;
    let outsider = 4usize;
    let v2_class = register_v2(&mut net, &truth).await;
    let false_root = Hash64::from_bytes(wrong.pc.root());
    let before_binder = net.collateral(REGISTRANT);
    let o = net.artifact_bound(REGISTRANT, v2_class, false_root);
    net.send(vec![(REGISTRANT, o)]).await;
    assert!(net.api().unwrap().artifact_binding_v1(&v2_class, &false_root).is_some(), "bound: no availability condition");

    // ---- the outsider's own copy of the class's bytes (any source; checked against the chain's root): the roots differ ----
    let copy = Dirs::new("false-truth", 1);
    let true_manifest =
        ArtifactManifestV1::of(net.domain, truth.class.clone(), truth.artifact_root, false_root, &leaves, net.daa() + 1_000);
    publish_artifact_v1(&copy.refs(), &true_manifest, &leaves, &[], 1).expect("the outsider's copy");
    let chain_root = net.chain.tip_state().1.class(&v2_class).unwrap().artifact_root;
    let fetched = fetch_artifact_v1(&copy.refs(), net.domain, v2_class, chain_root, Some(false_root), Hash64::default()).unwrap();
    let BindingCheckV1::KernelRootDiffers { true_commitments } = check_binding_v1(&fetched, false_root).unwrap() else {
        panic!("a false binding is never confirmed")
    };
    assert_eq!(true_commitments, truth.pc);
    let facts = ArtifactFactsV1 { program: &fetched.program, artifact_root: chain_root, kernel_param_root: false_root };

    // ---- the bound side, from the binder's OWN publication (voluntary, off chain), each unit checked against the bound root ----
    let binder = Dirs::new("false-binder", 1);
    let publish = |unit: PublicUnitV1| {
        let answer = answer_artifact_unit_v1(&wrong.program, &wrong_leaves, &unit).unwrap();
        publish_artifact_v1(&binder.refs(), &true_manifest, &leaves, &[(unit, answer)], 1).expect("the binder's tree");
    };
    let read = |unit: &PublicUnitV1| fetch_artifact_unit_v1(&binder.refs(), &facts, unit, Hash64::default()).expect("checked");
    publish(PublicUnitV1::KernelCommitments);
    let PublicUnitAnswerV1::KernelCommitments { commitments: bound } = read(&PublicUnitV1::KernelCommitments) else { unreachable!() };
    let (param, layer) = differing_instances_v1(&true_commitments, &bound)[0];
    let rows = misaka_palw_kernel::merkle::LayoutV1::of(&truth.params.tensors[&(param, layer)].shape).rows;
    let run_unit = PublicUnitV1::KernelRowNodes { param, layer, level: 0, first: 0, count: rows as u32 };
    publish(run_unit);
    let PublicUnitAnswerV1::KernelRowNodes { run, .. } = read(&run_unit) else { unreachable!() };
    let row = differing_rows_v1(&truth.params.tensors[&(param, layer)], &run)[0];
    let row_unit = PublicUnitV1::KernelRow { param, layer, row };
    publish(row_unit);
    let PublicUnitAnswerV1::KernelRow { opening, .. } = read(&row_unit) else { unreachable!() };
    assert!(
        verify_artifact_answer_v1(
            &facts,
            &row_unit,
            &PublicUnitAnswerV1::KernelRow { commitments: bound.clone(), opening: opening.clone() }
        )
        .is_ok()
    );

    // ---- tag 105 from the true bytes and the binder's row ----
    let proof =
        row_refutation_v1(&fetched.program, &fetched.leaves, chain_root, &bound, param, layer, &opening).expect("a refutation");
    let o = net.artifact_challenged(outsider, v2_class, false_root, proof);
    let chunks = kaspa_consensus_core::palw_state_v2::palw_object_chunks_with_cap_v1(&o, 6 << 10).expect("chunks");
    match chunks {
        Some(chunks) => net.send(chunks.into_iter().map(|c| (outsider, c)).collect()).await,
        None => net.send(vec![(outsider, o)]).await,
    };
    let binding = net.api().unwrap().artifact_binding_v1(&v2_class, &false_root).unwrap();
    assert!(binding.refuted, "the false binding is refuted from the bytes");
    let burn = kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_BURN_SOMPI_V1;
    assert_eq!(net.collateral(REGISTRANT), before_binder - RESERVED, "the BINDER lost its reservation");
    assert!(net.slashed(REGISTRANT) >= burn + RESERVED);
    assert_eq!(net.owed(outsider), RESERVED * REWARD / 1000, "the refuter's reward is queued (the next coinbase pays it)");
    assert_eq!(court_rows(&net), 0, "no provider court row anywhere");
    let z = net.replay().await;
    net.assert_same(&z, "replay");
}

// ---- the claim ---------------------------------------------------------------------------------------------------------------

/// A covered claim of card 0 (a lie or not) on the court network (fence at `at`): `(world, claim, the two providers, the outsider,
/// the claim's seats)` — the providers and the outsider are the three bonded cards outside the producer, the registrant and the seats.
async fn covered_claim(lie: bool, at: u64) -> (World, Claim, [usize; 2], usize, Vec<usize>) {
    let mut w = World::on(Net::over_cfg(court_kernel_config(at), TestConsensus::new)).await;
    let job = w.job().await;
    let claim = w.claim(0, &job, lie).await;
    let seats = w.seats(&claim.id);
    w.cover(&claim.id).await;
    let cards = w.outsiders(&claim, &seats, 3);
    (w, claim, [cards[0], cards[1]], cards[2], seats)
}

/// The latest DAA `claim`'s liability can run to (what a transfer's leases must cover).
fn horizon_bound(w: &World, claim: &Digest) -> u64 {
    let ledger = w.net.ledger();
    palw_kernel_claim_horizon_bound_v1(&ledger.claims[claim], &ledger.policy, ledger.opv.policy.as_ref())
}

/// [`covered_claim`], leased by both providers (600 BILI each: Σ ≥ the claim's 1,000) through its horizon bound, and its DA
/// responsibility moved to them by the producer (a transfer signed by anyone else is refused).
async fn transferred_claim(lie: bool) -> (World, Claim, [usize; 2], usize, Vec<usize>) {
    let (mut w, claim, providers, outsider, seats) = covered_claim(lie, 0).await;
    let id = Hash64::from_bytes(claim.id);
    let subject = ProviderSubjectV1::KernelClaim { claim: id };
    let bound = horizon_bound(&w, &claim.id);
    let (l0, l1) = (
        w.net.court_lease(providers[0], subject, mega(600), bound + 5),
        w.net.court_lease(providers[1], subject, mega(600), bound + 5),
    );
    w.net.send(vec![(providers[0], l0), (providers[1], l1)]).await;
    let not_producer = w.net.da_transfer(outsider, id);
    w.net.send(vec![(outsider, not_producer)]).await;
    assert!(w.net.court(&subject).row.is_none(), "only the claim's producer moves its DA responsibility");
    let o = w.net.da_transfer(0, id);
    w.net.send(vec![(0, o)]).await;
    assert!(w.net.court(&subject).row.unwrap().transferred_daa.is_some(), "transferred");
    assert!(w.net.api().unwrap().provider_liable_claims_v1().contains(&id));
    assert_eq!(w.net.court_reserved(providers[0]), u128::from(mega(600)), "V2 holds each lease against its provider");
    (w, claim, providers, outsider, seats)
}

/// **A transfer needs two live operators through the claim's whole liability bound, none the producer's, reserving at least the claim;
/// it is irreversible, and never re-assigns a failure in flight.**
#[tokio::test]
async fn da16_a_transfer_needs_two_operators_through_the_bound_reserving_at_least_the_claim() {
    kaspa_core::log::try_init_logger("warn");
    let (mut w, claim, providers, outsider, seats) = covered_claim(false, 0).await;
    let id = Hash64::from_bytes(claim.id);
    let subject = ProviderSubjectV1::KernelClaim { claim: id };
    let bound = horizon_bound(&w, &claim.id);
    let transfer = |w: &mut World| w.net.da_transfer(0, id);
    let transferred = |w: &World| w.net.court(&subject).row.is_some_and(|r| r.transferred_daa.is_some());

    let o = transfer(&mut w);
    w.net.send(vec![(0, o)]).await;
    assert!(!transferred(&w), "no lease");
    // The producer's own lease is refused; a lease ending one DAA before the bound does not count; Σ 900 < 1,000 does not move it.
    let own = w.net.court_lease(0, subject, mega(600), bound + 5);
    let short = w.net.court_lease(seats[0], subject, mega(600), bound - 1);
    let small = w.net.court_lease(providers[0], subject, mega(300), bound + 5);
    w.net.send(vec![(0, own), (seats[0], short), (providers[0], small)]).await;
    assert_eq!(w.net.court(&subject).leases.len(), 2, "the producer's own lease was refused");
    let o = transfer(&mut w);
    w.net.send(vec![(0, o)]).await;
    assert!(!transferred(&w), "one lease through the bound is one operator");
    let o = w.net.court_lease(providers[1], subject, mega(600), bound + 5);
    w.net.send(vec![(providers[1], o)]).await;
    let o = transfer(&mut w);
    w.net.send(vec![(0, o)]).await;
    assert!(!transferred(&w), "two operators, but Σ 900 BILI reserves less than the claim's 1,000: withholding would get cheaper");
    // A demand in flight is never re-assigned: with an open demand (and a third lease making Σ 1,500), still refused.
    let o = w.net.court_lease(seats[1], subject, mega(600), bound + 5);
    w.net.send(vec![(seats[1], o)]).await;
    w.demand(outsider, &claim.id, 0).await;
    let o = transfer(&mut w);
    w.net.send(vec![(0, o)]).await;
    assert!(!transferred(&w), "a demand is open: a failure in flight stays the producer's");
    w.serve(0, &claim, 0).await;
    let o = transfer(&mut w);
    w.net.send(vec![(0, o)]).await;
    assert!(transferred(&w), "answered, Σ 1,500 through the bound by three operators: moved");
    let at = w.net.court(&subject).row.unwrap().transferred_daa;
    let o = transfer(&mut w);
    w.net.send(vec![(0, o)]).await;
    assert_eq!(w.net.court(&subject).row.unwrap().transferred_daa, at, "irreversible, and once");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **One failure, one party: a transferred claim's unanswered demand is its PROVIDERS' default, never the producer's.** The producer
/// moved a (lying) claim's material to two leases; an outsider demands the withheld position; nobody answers. At the deadline every
/// live lease is charged (the demander takes the default penalty from the charge, its bond back), the claim is void —
/// `Unavailable { producer_defaulted: false }`, no reward, never convicted — and the producer pays nothing: its reservation comes back.
#[tokio::test]
async fn da16_a_transferred_claims_unanswered_demand_charges_every_provider_never_the_producer() {
    kaspa_core::log::try_init_logger("warn");
    let (mut w, lie, providers, outsider, _) = transferred_claim(true).await;
    let subject = ProviderSubjectV1::KernelClaim { claim: Hash64::from_bytes(lie.id) };
    let (producer_before, p_before) = (w.net.collateral(0), providers.map(|p| w.net.collateral(p)));
    let da = lie.published(&w.fx, &[lie.at]);
    assert_eq!(w.fresh(0x31).check(lie.id, &da, &w.fx.params), OutsiderFindingV1::Demand(vec![(0, lie.at.0)]));
    w.demand(outsider, &lie.id, lie.at.0).await;
    // The court scope's tally (ADR-0177 D2): the demanded unit is claim-specific and counted against the requester's OPERATOR only.
    let operator = w.net.chain.tip_state().1.bond(&w.net.bond(outsider)).unwrap().operator_id;
    assert_eq!(w.net.court(&subject).row.unwrap().requested, vec![(operator, vec![(0, lie.at.0)])], "one unit, one requester");
    let deadline = w.net.ledger().demands[&(lie.id, 0, lie.at.0)].deadline_daa;
    w.net.beat_to(deadline).await;

    assert!(
        matches!(w.net.claim_state(&lie.id), ClaimStateV1::Unavailable { producer_defaulted: false, .. }),
        "void, and not the producer's default: {:?}",
        w.net.claim_state(&lie.id)
    );
    assert!(!w.net.ledger().claims[&lie.id].convicted, "a default is never a conviction");
    assert_eq!(w.net.collateral(0), producer_before, "the producer pays nothing for its providers' failure");
    assert_eq!(w.net.slashed(0), 0);
    assert_eq!(w.net.kernel_reserved(0), 0, "its reservation is released whole");
    for (i, p) in providers.iter().enumerate() {
        assert_eq!(w.net.collateral(*p), p_before[i] - mega(600), "provider {p}: its lease is charged");
        assert_eq!(w.net.court_reserved(*p), 0);
    }
    let penalty = w.policy().default_penalty;
    let paid = palw_provider_reporter_share_v1(penalty);
    assert_eq!(paid, penalty * 4_900 / 10_000, "ADR-0032's 49% of the penalty-sized part of the charge");
    // Conservation: the providers' charge (2 × 600 BILI) is collected whole; the demander is paid 49% of the penalty out of it; the
    // rest is burned — nothing minted.
    assert_eq!(w.net.owed(outsider), paid, "the demander's share comes out of the providers' charge");
    assert!(paid < 2 * mega(600));
    assert_eq!(w.net.kernel_reserved(outsider), 0, "its demand bond returns");
    let row = w.net.court(&subject).row.unwrap();
    assert!(row.lapsed_daa.is_some() && row.charged.len() == 2, "every provider charged once; the claim's material lapsed");
    w.net.beat_to(deadline + 100).await;
    assert!(!w.net.ledger().claims[&lie.id].rewarded, "void: no reward ever");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **A common-mode outage voids the claim and never convicts it.** Both providers of a transferred claim fail a challenge of a committed
/// position (one of them two challenges: charged ONCE, its other challenge settles moot). Each is charged once — its reservation
/// slashed; before Final its challenger is paid the PALW reporter share (49%, ADR-0032: the floor a self-reported conviction leaves, so
/// a self-lapse is no cheaper escape) and the rest is burned (conservation: share + burn = the slash) — the challengers get their bonds
/// back, no live lease is left, and the claim LAPSES: `Unavailable { producer_defaulted: false }`, no reward, never convicted, no
/// `SlashFraud`/`SlashDefault` of the miner, whose reservation comes back whole. A replaying node agrees.
#[tokio::test]
async fn da16_a_common_mode_provider_outage_voids_the_claim_without_a_miner_slash() {
    kaspa_core::log::try_init_logger("warn");
    let (mut w, claim, providers, outsider, seats) = transferred_claim(false).await;
    let subject = ProviderSubjectV1::KernelClaim { claim: Hash64::from_bytes(claim.id) };
    let (producer_before, p_before) = (w.net.collateral(0), providers.map(|p| w.net.collateral(p)));
    let (unit, unit2) = (PublicUnitV1::ClaimPosition { stage: 0, position: 1 }, PublicUnitV1::ClaimPosition { stage: 0, position: 2 });
    // Two challengers (any bonds of other operators — here a seat too: seats have no power here), in one block: one deadline.
    let second = seats[0];
    let (out_before, second_before) = (w.net.collateral(outsider), w.net.collateral(second));
    let (c0, c0b, c1) = (
        w.net.court_challenge(outsider, subject, providers[0], unit),
        w.net.court_challenge(outsider, subject, providers[0], unit2),
        w.net.court_challenge(second, subject, providers[1], unit),
    );
    w.net.send(vec![(outsider, c0), (outsider, c0b), (second, c1)]).await;
    // A position the claim does not commit is no unit of it (refused, not a default by construction); an artifact unit is model bytes.
    let none = w.net.court_challenge(outsider, subject, providers[0], PublicUnitV1::ClaimPosition { stage: 0, position: 9_999 });
    let model = w.net.court_challenge(outsider, subject, providers[0], PublicUnitV1::ArtifactLeaf { index: 0 });
    w.net.send(vec![(outsider, none), (outsider, model)]).await;
    assert_eq!(w.net.court(&subject).challenges.len(), 3);
    assert_eq!(w.net.court_reserved(outsider), 2 * u128::from(PALW_PROVIDER_CHALLENGE_BOND_SOMPI_V1), "two challenge bonds held");
    let deadlines: Vec<u64> = w.net.court(&subject).challenges.iter().map(|(_, _, r)| r.deadline_daa).collect();
    assert!(deadlines.iter().all(|d| *d == deadlines[0]), "filed in one block");
    w.net.beat_to(deadlines[0] + 1).await;

    assert!(
        matches!(w.net.claim_state(&claim.id), ClaimStateV1::Unavailable { producer_defaulted: false, .. }),
        "the claim lapsed: {:?}",
        w.net.claim_state(&claim.id)
    );
    assert!(!w.net.ledger().claims[&claim.id].convicted, "never convicted");
    assert_eq!((w.net.collateral(0), w.net.slashed(0), w.net.kernel_reserved(0)), (producer_before, 0, 0), "the miner pays nothing");
    let share = palw_provider_reporter_share_v1(mega(600));
    assert_eq!(share, mega(600) * 4_900 / 10_000, "ADR-0032's 49%");
    for (i, p) in providers.iter().enumerate() {
        assert_eq!(w.net.collateral(*p), p_before[i] - mega(600), "provider {p} charged ONCE");
        assert_eq!(w.net.slashed(*p), mega(600), "the whole reservation is collected");
    }
    // Conservation: each slash is paid out as one share to its challenger and the rest burned — nothing minted, nothing kept.
    assert_eq!((w.net.owed(outsider), w.net.owed(second)), (share, share), "one share per charge, never two (the moot one pays none)");
    assert!(share < mega(600) && mega(600) - share == mega(600) * 5_100 / 10_000, "the burned remainder");
    assert_eq!((w.net.court_reserved(outsider), w.net.court_reserved(second)), (0, 0), "the challenge bonds returned");
    assert_eq!((w.net.collateral(outsider), w.net.collateral(second)), (out_before, second_before), "no fee: nobody answered");
    let lapsed = w.net.court(&subject);
    assert!(lapsed.row.as_ref().unwrap().lapsed_daa.is_some() && lapsed.row.as_ref().unwrap().charged.len() == 2);
    assert!(lapsed.challenges.is_empty(), "the moot challenge went with the charge");
    w.net.beat_to(deadlines[0] + 100).await;
    assert!(!w.net.ledger().claims[&claim.id].rewarded, "void: no reward ever");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **A reorg takes a transfer back, and the producer is liable again.** The providers lease a covered claim; its producer moves the DA
/// responsibility; a heavier branch from before the transfer wins on a replaying node: the court's rows are the fork's exactly (leased,
/// not transferred), the claim is no longer provider-liable — and on that branch (continued on B's own node: a node that also holds A's
/// blocks would merge their carrier back, a DAG never drops a carried transaction) an unanswered demand is the PRODUCER's default, as if
/// no transfer had ever been made; the providers are never charged for it.
#[tokio::test]
async fn da16_a_reorg_takes_the_transfer_back_and_the_producer_is_liable_again() {
    kaspa_core::log::try_init_logger("warn");
    let (mut w, lie, providers, outsider, _) = covered_claim(true, 0).await;
    let id = Hash64::from_bytes(lie.id);
    let subject = ProviderSubjectV1::KernelClaim { claim: id };
    let bound = horizon_bound(&w, &lie.id);
    let (l0, l1) = (
        w.net.court_lease(providers[0], subject, mega(600), bound + 5),
        w.net.court_lease(providers[1], subject, mega(600), bound + 5),
    );
    w.net.send(vec![(providers[0], l0), (providers[1], l1)]).await;
    let fork = w.net.chain.sink();
    let at_fork = w.net.api().expect("the route");
    let o = w.net.da_transfer(0, id);
    w.net.send(vec![(0, o)]).await;
    assert!(w.net.court(&subject).row.unwrap().transferred_daa.is_some(), "A: transferred");

    // ---- Z replays A; B, from before the transfer, out-works A's two blocks: Z reorgs onto B ----
    let z = w.net.replay().await;
    w.net.assert_same(&z, "Z on A");
    let zn = w.net.on_chain(z);
    let b = t12_genesis_chain(&w.net.config, &w.net.bundle, &w.net.premine, &w.net.floats);
    let up_to_fork = chain_blocks(&w.net.chain, fork);
    let fork_timestamp = up_to_fork.last().unwrap().header.timestamp;
    for blk in up_to_fork {
        arrive(&b, blk, "a block up to the fork").await;
    }
    let mut b = b;
    b.ctx.simulated_time = fork_timestamp;
    let ttpb = w.net.ttpb();
    for _ in 0..4 {
        let blk = b.heartbeat(ttpb, Vec::new()).await;
        arrive(&zn.chain, blk, "B's block").await;
    }
    assert_eq!(zn.chain.sink(), b.sink(), "B out-works A's two blocks: Z reorgs onto B");
    assert_eq!(zn.chain.tip_state().1.state_root(), b.tip_state().1.state_root(), "Z on B: the root of B's fresh replay");
    let on_b = zn.api().expect("the route");
    assert_eq!(on_b.aux, at_fork.aux, "the court's rows are the fork's exactly: leased, not transferred");
    assert!(on_b.provider_liable_claims_v1().is_empty() && zn.court(&subject).row.is_none());

    // ---- on B the claim is its producer's again: an unanswered demand is its default ----
    let mut bn = w.net.on_chain(b);
    let (before, p_before) = (bn.collateral(0), providers.map(|p| bn.collateral(p)));
    let o = bn.route(outsider, &K::FileDemand { demander: bn.kid(outsider), claim: lie.id, stage: 0, position: lie.at.0 });
    bn.send(vec![(outsider, o)]).await;
    let deadline = bn.ledger().demands[&(lie.id, 0, lie.at.0)].deadline_daa;
    bn.beat_to(deadline).await;
    assert!(
        matches!(bn.claim_state(&lie.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. }),
        "the producer's default: {:?}",
        bn.claim_state(&lie.id)
    );
    assert_eq!(bn.collateral(0), before - w.policy().default_penalty, "the producer pays the fixed penalty");
    for (i, p) in providers.iter().enumerate() {
        assert_eq!(bn.collateral(*p), p_before[i], "provider {p} is never charged for its producer's failure");
    }
}

/// **A false computation stays the miner's.** A lying claim's material moved to two providers; the producer published every position
/// to the transport. An outsider fresh-verifies, cannot convict from what it holds and demands the withheld position; a PROVIDER fetches
/// it from the transport (checked by the kernel's own classification) and answers the kernel's demand; the served values convict the
/// producer through the kernel's unchanged `FileProof` — its reservation is slashed, the providers are untouched. On the way, a
/// provider-court challenge of the same position is answered from the transport and cleared.
#[tokio::test]
async fn da16_a_false_computation_on_a_transferred_claim_still_convicts_the_miner() {
    kaspa_core::log::try_init_logger("warn");
    let (mut w, lie, providers, outsider, _) = transferred_claim(true).await;
    let id = Hash64::from_bytes(lie.id);
    let subject = ProviderSubjectV1::KernelClaim { claim: id };
    // The producer places every committed position with the providers (each checked by the kernel's classification first).
    let dirs = Dirs::new("claim", 2);
    let positions: Vec<(u8, u32, Vec<u8>)> =
        (0..lie.trace.values.len() as u32).map(|p| (0u8, p, lie.position(&w.fx, p, |_| {}))).collect();
    let ledger = w.net.ledger();
    publish_claim_positions_v1(&dirs.refs(), w.net.domain, &ledger, id, &positions, w.net.daa() + 1_000, 2).expect("two copies");
    let mut forged = positions.clone();
    forged[0].2 = lie.position(&w.fx, 0, |r| {
        r.pop();
    });
    assert!(
        publish_claim_positions_v1(&dirs.refs(), w.net.domain, &ledger, id, &forged, 0, 1).is_err(),
        "a provider never places material that would not serve a demand"
    );
    let (producer_before, p_before) = (w.net.collateral(0), providers.map(|p| w.net.collateral(p)));

    // A provider-court challenge of the lying position, answered from the transport by the provider (the kernel's own check).
    let unit = PublicUnitV1::ClaimPosition { stage: 0, position: lie.at.0 };
    let out_before = w.net.collateral(outsider);
    let c = w.net.court_challenge(outsider, subject, providers[1], unit);
    w.net.send(vec![(outsider, c)]).await;
    let bytes = fetch_claim_position_v1(&[dirs.refs()[1]], &w.net.ledger(), id, 0, lie.at.0, Hash64::default()).expect("served");
    let a = w.net.court_answer(providers[1], subject, unit, PublicUnitAnswerV1::ClaimPosition { bytes: bytes.clone() });
    w.net.send(vec![(providers[1], a)]).await;
    assert!(w.net.court(&subject).challenges.iter().all(|(_, _, r)| r.answered), "answered: cleared");
    assert_eq!(w.net.collateral(outsider), out_before - PALW_PROVIDER_CHALLENGE_FEE_SOMPI_V1, "an answered challenge costs its fee");

    // The kernel's demand stays THE material demand; a provider answers it with the transport's bytes.
    let da = lie.published(&w.fx, &[lie.at]);
    assert_eq!(w.fresh(0x41).check(lie.id, &da, &w.fx.params), OutsiderFindingV1::Demand(vec![(0, lie.at.0)]));
    w.demand(outsider, &lie.id, lie.at.0).await;
    let o = w.net.route(providers[0], &K::Respond { claim: lie.id, stage: 0, position: lie.at.0, bytes });
    w.net.send(vec![(providers[0], o)]).await;
    assert!(w.net.ledger().served.contains_key(&(lie.id, 0, lie.at.0)), "served on chain by a provider");
    let proof = w.prosecution(&lie.id, &da, 0x42);
    w.proof(outsider, &lie.id, proof).await;

    assert!(w.net.ledger().claims[&lie.id].convicted, "the false computation is convicted");
    let slashed = w.policy().claim_collateral;
    assert_eq!(w.net.collateral(0), producer_before - slashed, "the MINER's reservation is slashed (SlashFraud)");
    assert_eq!(w.net.slashed(0), slashed);
    for (i, p) in providers.iter().enumerate() {
        assert_eq!(w.net.collateral(*p), p_before[i], "provider {p} served: untouched");
    }
    assert!(w.net.court(&subject).leases.iter().all(|(_, l)| !l.charged));
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **Old vs new, and the A-2 rule.** Below `palw_provider_court_v1` a court object is dropped by name (no row, no charge); past it the
/// leases land, but a claim committed BELOW the fence can never move its DA responsibility: its unanswered demand is its producer's
/// default exactly as before (the fixed penalty), and the providers are never charged for it.
#[tokio::test]
async fn da16_below_the_fence_court_objects_are_dropped_and_an_older_claim_stays_its_producers() {
    kaspa_core::log::try_init_logger("warn");
    let fence = 40;
    let (mut w, lie, providers, outsider, _) = covered_claim(true, fence).await;
    assert!(w.net.daa() < fence, "the claim is committed and covered below the fence");
    let id = Hash64::from_bytes(lie.id);
    let subject = ProviderSubjectV1::KernelClaim { claim: id };
    let bound = horizon_bound(&w, &lie.id);
    let court_rows = |w: &World| {
        w.net.api().map(|r| r.aux.keys().filter(|(t, _)| (PALW_PROVIDER_COURT_TABLE_LEASES_V1..=45).contains(t)).count()).unwrap_or(0)
    };
    let (l0, l1) = (
        w.net.court_lease(providers[0], subject, mega(600), bound + 5),
        w.net.court_lease(providers[1], subject, mega(600), bound + 5),
    );
    w.net.send(vec![(providers[0], l0.clone()), (providers[1], l1.clone())]).await;
    assert!(w.net.daa() < fence);
    assert_eq!(court_rows(&w), 0, "below the fence the objects were dropped by name: no row");
    assert_eq!(w.net.court_reserved(providers[0]), 0);

    w.net.beat_to(fence).await;
    w.net.send(vec![(providers[0], l0), (providers[1], l1)]).await;
    assert_eq!(w.net.court(&subject).leases.len(), 2, "past the fence the same signed leases land");
    let o = w.net.da_transfer(0, id);
    w.net.send(vec![(0, o)]).await;
    assert!(w.net.court(&subject).row.is_none(), "a claim committed below the fence stays its producer's");

    let before = w.net.collateral(0);
    let p_before = providers.map(|p| w.net.collateral(p));
    w.demand(outsider, &lie.id, lie.at.0).await;
    let deadline = w.net.ledger().demands[&(lie.id, 0, lie.at.0)].deadline_daa;
    w.net.beat_to(deadline).await;
    assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. }), "today's default");
    assert_eq!(w.net.collateral(0), before - w.policy().default_penalty, "the producer pays the fixed penalty, as before the court");
    for (i, p) in providers.iter().enumerate() {
        assert_eq!(w.net.collateral(*p), p_before[i], "provider {p} is never charged for its producer's failure");
    }
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

// C4R4 round 4b (F-C4R4-19): ADR-0177 non-interference, un-ignored after the re-scope.
mod c4r4;
