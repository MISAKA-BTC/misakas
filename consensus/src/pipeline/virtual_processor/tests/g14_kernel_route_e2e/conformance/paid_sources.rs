//! **The beacon's sources on the real path after the RFC07 adapter: approved paid claims, bootstrapped from the pinned seed.**
//!
//! Since the adapter (`74921a40a`) the one source of a beacon event is a `PaidClaimV2` Final the ASSIGNED quorum approved. No such
//! Final exists on a fresh chain, and the paid lock needs a locked work beacon — so nothing could ever start (validation 2026-10-17).
//! User decision 2026-10-17: until the first approved paid Final exists the lock draws its checkers from the network's pinned
//! bootstrap seed (`PalwClaimChallengePolicyV2::bootstrap_seed`, dormant). These helpers drive every step through the fold, signed by
//! the actor the fold names, with no seam: a paid class over the source class (registrant consent), the payer's job enrollment, the
//! producer's signed monetary covenant, its seal and paid commit, the lock (bootstrap draw), the drawn seats' receipts, and the claim's
//! own clock to `Final` — whose canonical paid Final row makes `finals_read_v1` export the event.
use super::*;
use kaspa_consensus_core::palw_claim_challenge_v2::{
    PAID_CLAIM_LOCKS_V2, PalwClaimChallengeFenceV2, PalwPaidClaimAuthorityV2, PalwPaidClaimLockV2,
};
use kaspa_consensus_core::palw_monetary_material_v1::{
    PalwMonetaryMaterialEnrollmentV1, PalwMonetaryMaterialFenceV1, PalwMonetaryMaterialOfferV1, PalwMonetaryMaterialPolicyV1,
    PalwMonetaryMaterialTermsV1,
};
use kaspa_consensus_core::palw_provider_court_v1::{PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1, palw_provider_court_message_v1};
use misaka_palw_kernel::claim_paid_v2::{PaidClaimObjectV2, PaidClaimReceiptV2};

/// The test networks' pinned bootstrap seed (a FIXTURE value; a real network pins its own at genesis or an annual fork).
pub(super) const TEST_BOOTSTRAP_SEED: [u8; 64] = [0xB5; 64];

/// The monetary covenant policy the paid source claims enroll under (the native-Final suite's unapproved test terms).
fn material_policy(window_daa: u64) -> PalwMonetaryMaterialPolicyV1 {
    PalwMonetaryMaterialPolicyV1 {
        version: 1,
        max_contracts: 64,
        max_contracts_per_provider: 32,
        max_bytes_per_contract: 1_000_000,
        max_bytes_per_provider: 32_000_000,
        minimum_collateral_sompi: 100_000_000,
        minimum_fee_sompi: 1_000_000,
        window_daa,
        response_window_daa: 64,
        max_enrollment_daa: 200,
        cost_report_root: Hash64::from_bytes([0xD4; 64]),
    }
}

/// **Arm the paid-claim path on `params`** (every fence unapproved and refused by validation, as everywhere in this file): the paid
/// claim challenge with the bootstrap seed, the beacon source and source-fee fences its commit reads, the monetary covenant and the
/// income fences that covenant needs.
pub(crate) fn arm_paid_sources(params: &mut kaspa_consensus_core::config::params::Params) {
    use kaspa_consensus_core::palw_beacon_source_fee_v1::PalwBeaconSourceFeeFenceV1;
    use kaspa_consensus_core::palw_beacon_source_opt_in_v1::PalwBeaconSourceFenceV1;
    let mut challenge = PalwClaimChallengeFenceV2::unapproved_probe_at(ForkActivation::new(1));
    challenge.policy.bootstrap_seed = Some(TEST_BOOTSTRAP_SEED);
    params.palw_claim_challenge_v2 = Some(challenge);
    params.palw_beacon_source_opt_in_v1 = Some(PalwBeaconSourceFenceV1::unapproved_probe_at(ForkActivation::new(1)));
    params.palw_beacon_source_fee_v1 = Some(PalwBeaconSourceFeeFenceV1::unapproved_probe_at(ForkActivation::new(1)));
    params.palw_reporter_income_v2.get_or_insert(ForkActivation::new(1));
    params.palw_material_income_v3.get_or_insert(ForkActivation::new(1));
    // The covenant is a provider-court object (tag 240) …
    params.palw_provider_court_v1.get_or_insert(ForkActivation::new(1));
    // … and past `palw_provider_court_v1` the kernel's court-scope clear budget refuses every K2-TIR-v1/v2 claim (the source class
    // is v2) unless the commitment-only route is in force (validation 2026-10-17: "COURT_SCOPE_INCOMPLETE").
    params.palw_court_commitment_scope_v1.get_or_insert(ForkActivation::new(1));
    let window = params.palw_bond_budget_v1.as_ref().map_or(1_000, |f| f.policy.window_daa);
    params.palw_monetary_material_v1 =
        Some(PalwMonetaryMaterialFenceV1 { activation: ForkActivation::new(1), policy: material_policy(window) });
    params.sync_palw_claim_challenge_v2();
    params.sync_palw_bond_budget_v1();
}

fn card_sign(card: usize, message: &[u8], context: &[u8], rnd: u8) -> Vec<u8> {
    let key = TestConsensus::palw_v2_registry_keypair(card as u64);
    libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [rnd; 32]).expect("ML-DSA-87 signs").as_ref().to_vec()
}

impl Cw {
    fn paid_policy(&self) -> kaspa_consensus_core::palw_claim_challenge_v2::PalwClaimChallengePolicyV2 {
        self.net.config.params.palw_claim_challenge_v2.expect("the paid path is armed").policy
    }

    /// The paid verification class over the SOURCE class (registered once, by the registrant).
    async fn paid_source_class(&mut self) -> Digest {
        if let Some(class) = self.paid_class {
            return class;
        }
        let source = self.net.config.params.palw_beacon_source_opt_in_v1.expect("the source fence").policy;
        let route = self.net.api().expect("the route");
        let source_policy = misaka_palw_challenge::paid_opt_in_source_policy_v5(2, 2, 20, 2, 1);
        let object = misaka_palw_sdk::claim_paid_v2::paid_claim_class_object_v2(
            &route,
            &self.paid_policy(),
            self.net.kid(REGISTRANT),
            self.src_class,
            source_policy,
            &source,
        )
        .expect("the paid class over the source class");
        let K::PaidClaimV2 { object: PaidClaimObjectV2::RegisterClass { class } } = &object else { unreachable!() };
        let id = class.id();
        let o = self.net.route(REGISTRANT, &object);
        self.net.send(vec![(REGISTRANT, o)]).await;
        assert!(
            self.net
                .api()
                .unwrap()
                .aux_row::<misaka_palw_kernel::claim_paid_v2::PaidClaimClassV2>(
                    kaspa_consensus_core::palw_claim_challenge_v2::PAID_CLAIM_CLASSES_V2,
                    &borsh::to_vec(&id).unwrap()
                )
                .is_some(),
            "the registrant's paid class is enrolled"
        );
        self.paid_class = Some(id);
        id
    }

    /// The producer's signed monetary covenant for `root` (the base claim), two providers' offers.
    async fn covenant(
        &mut self,
        producer: usize,
        root: Hash64,
        manifest: Option<kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialManifestV1>,
        max_material_bytes: u64,
    ) -> Hash64 {
        let policy = self.net.config.params.palw_monetary_material_v1.as_ref().expect("the covenant fence").policy.clone();
        let providers: Vec<usize> = PRODUCERS.iter().copied().filter(|c| *c != producer).take(2).collect();
        let mut enrollment = PalwMonetaryMaterialEnrollmentV1 {
            terms: PalwMonetaryMaterialTermsV1 {
                version: 1,
                root,
                dependencies: Vec::new(),
                exec_plan_root: None,
                sponsor: self.net.bond(producer),
                nonce: Hash64::from_bytes([self.jobs; 64]),
                policy: policy.id(),
                max_material_bytes,
                material_manifest: manifest,
                native_session: None,
                window_daa: policy.window_daa,
                enrollment_until_daa: self.net.daa() + 100,
                payment_relative_until_finality: true,
            },
            offers: providers
                .iter()
                .map(|card| PalwMonetaryMaterialOfferV1 {
                    provider: self.net.bond(*card),
                    collateral_sompi: policy.minimum_collateral_sompi,
                    prepaid_fee_sompi: policy.minimum_fee_sompi,
                    signature: Vec::new(),
                })
                .collect(),
        };
        enrollment.offers.sort_by_key(|o| o.provider);
        let payload = enrollment.payload();
        for offer in &mut enrollment.offers {
            let card = providers.iter().copied().find(|card| self.net.bond(*card) == offer.provider).unwrap();
            let message = palw_provider_court_message_v1(self.net.domain, 240, &offer.provider, &payload);
            offer.signature = card_sign(card, message.as_byte_slice(), PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1, 0xD1);
        }
        let message = palw_provider_court_message_v1(self.net.domain, 240, &enrollment.terms.sponsor, &payload);
        let object = Obj::MonetaryMaterialEnrolledV1 {
            enrollment: Box::new(enrollment),
            signature: card_sign(producer, message.as_byte_slice(), PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1, 0xD2),
        };
        let rehearsal = self.net.chain.ctx.consensus.palw_object_rehearsal_v1(&object);
        self.net.send(vec![(producer, object)]).await;
        self.net
            .chain
            .tip_state()
            .1
            .monetary_material_row_v1(root)
            .unwrap_or_else(|| panic!("the producer's signed covenant is enrolled (rehearsal: {rehearsal:?})"))
            .consent
    }

    /// **RFC08 F3: the producer's signed covenant over a kernel claim's complete served material** — every committed position, as
    /// served, verified against the ledger that admits the claim (`PalwKernelMaterialBundleV1::verify`), so the scope is the body's
    /// commitment. Past `palw_monetary_material_v1` no kernel claim commits without it. `reveal` is the exact commit object.
    pub(super) async fn kernel_covenant(&mut self, producer: usize, base: Digest, trace: &TraceV1, reveal: &K) -> Hash64 {
            let manifest = {
            let route = self.net.api().unwrap();
            let mut admitted: misaka_palw_kernel::ledger::KernelLedgerV1 = (*route.ledger().expect("the route's ledger")).clone();
            admitted.daa = self.net.daa() + 1;
            admitted
                .apply_object(reveal, &misaka_palw_kernel::ledger::AuthV1 { signer_bond: self.net.kid(producer) })
                .unwrap_or_else(|e| panic!("the claim is admissible: {e:?}"));
            // The ledger's own mask: under the commitment-only route model-dependent values are withheld too.
            let mask = admitted.derived_mask(&admitted.claims[&base], 0);
            let positions: Vec<(u8, u32, Vec<u8>)> = trace
                .values
                .iter()
                .enumerate()
                .map(|(p, stages)| {
                    let values = stages
                        .iter()
                        .enumerate()
                        .map(|(st, o)| {
                            o.iter()
                                .enumerate()
                                .map(|(n, t)| {
                                    if mask[st][n] {
                                        MaterialResponseV1::Omitted
                                    } else {
                                        MaterialResponseV1::Whole(TensorWireV1::of(t))
                                    }
                                })
                                .collect()
                        })
                        .collect();
                    let inputs = trace
                        .inputs
                        .get(p)
                        .map(|row| row.iter().map(|t| MaterialResponseV1::Whole(TensorWireV1::of(t))).collect())
                        .unwrap_or_default();
                    (0u8, p as u32, borsh::to_vec(&PositionResponseV1 { values, inputs }).unwrap())
                })
                .collect();
            let bundle = kaspa_consensus_core::palw_material_capacity_v1::PalwKernelMaterialBundleV1 {
                claim: Hash64::from_bytes(base),
                positions,
            };
            let row = &admitted.claims[&base];
            let shape = |p: u32| {
                row.body.position(0, p).map(|(v, i)| (v.iter().map(|o| o.len()).collect::<Vec<_>>(), i.len()))
            };
            let served = |p: usize| {
                (trace.values[p].iter().map(|o| o.len()).collect::<Vec<_>>(), trace.inputs.get(p).map_or(0, |i| i.len()))
            };
            let scope = bundle.verify(&admitted, u64::MAX).unwrap_or_else(|e| {
                panic!(
                    "the complete served material verifies: {e:?}; stages {:?}; committed p0 {:?} served p0 {:?}; positions {} vs trace {}",
                    row.body.stages(),
                    shape(0),
                    served(0),
                    bundle.positions.len(),
                    trace.values.len()
                )
            });
            kaspa_consensus_core::palw_material_capacity_v1::PalwMaterialManifestV1 { root: Hash64::from_bytes(base), scopes: vec![scope] }
        };
        // The signed byte cap covers the bundle plus the manifest and its provider framing (`PalwMaterialManifestV1::validate`).
        let bytes = manifest.scopes[0].bytes + 4_096;
        self.covenant(producer, Hash64::from_bytes(base), Some(manifest), bytes).await
    }

    /// **The seal-reveal seed of a later paid target** (ADR-0185 F1): the payer funds the target's future source fee before its
    /// seal window; two producers other than the target's own seal honest claims of fresh source-class jobs inside the window and opt
    /// them in as paid sources; after the seal window each reveals (under its signed covenant); the caller locks after the reveal
    /// window. Nothing here is Final: the sources are sealed commitments, not credits.
    async fn seal_reveal_sources(&mut self, base: Digest, enrollment: Digest, sponsor: usize, target_producer: usize) {
        use misaka_palw_kernel::beacon_source_fee::BeaconSourceFeeTermsV1;
        let fee_policy = self.net.config.params.palw_beacon_source_fee_v1.expect("the source fee fence").policy;
        let route = self.net.api().unwrap();
        let authority: PalwPaidClaimAuthorityV2 = route.paid_claim_authority_for_base_unchecked_v2(&base).expect("the paid authority");
        let target = authority.source_target;
        let fee = fee_policy.minimum_fee_for(&target).expect("the frozen source target's fee floor");
        let k = authority.class.source_policy.work_count_k;
        let fund = misaka_palw_sdk::claim_paid_v2::paid_claim_funding_object_v2(&route, enrollment, self.net.kid(sponsor), fee, k, &fee_policy)
            .unwrap_or_else(|e| panic!("the payer funds the future sources: {e:?}"));
        let o = self.net.route(sponsor, &fund);
        let rehearsal = self.net.chain.ctx.consensus.palw_object_rehearsal_v1(&o);
        self.net.send(vec![(sponsor, o)]).await;
        let terms = BeaconSourceFeeTermsV1 { source: target.terms, sponsor: self.net.kid(sponsor), fee_per_source: fee, max_sources: k };
        assert!(
            self.net.ledger().beacon_source_fees.contains_key(&terms.source.context),
            "the future source fee is funded (rehearsal: {rehearsal:?})"
        );
        // The target's frozen eligible source profile is the OPV kernel class (the paid class's own base class is excluded):
        // its source jobs, posted by payers other than the sealers, then the seals inside the window.
        let source_class = self.kernel_class;
        assert!(target.context.eligible_profiles.contains(&source_class), "the kernel class is an eligible source profile");
        // RFC07-3 seal-before-commit: a producer seals as a source only before the commitment of every live source target of its
        // own, so the sealers are no live source target's producer.
        let paid_producers = self.net.live_target_producers();
        let sealers: Vec<usize> = PRODUCERS
            .iter()
            .copied()
            .filter(|p| *p != target_producer && *p != sponsor && !paid_producers.contains(&self.net.kid(*p)))
            .take(k as usize)
            .collect();
        assert_eq!(sealers.len(), k as usize, "enough producers with no live paid target of their own");
        let payers: Vec<usize> = [REGISTRANT, OUTSIDER].into_iter().chain(PRODUCERS).filter(|c| !sealers.contains(c)).take(k as usize).collect();
        let mut jobs = Vec::new();
        for payer in &payers {
            self.jobs += 1;
            let job = KernelJobV1 {
                class_binding_id: source_class,
                prompt: vec![5, 11, 2],
                max_new_tokens: 3,
                decode: DecodeRuleV1::Greedy,
                nonce: [self.jobs; 64],
            };
            let o = self.net.route(*payer, &K::PostJob { job: job.clone() });
            self.net.send(vec![(*payer, o)]).await;
            jobs.push(job);
        }
        if self.net.daa() < terms.source.seal_start {
            self.net.beat_to(terms.source.seal_start).await;
        }
        let ledger = self.net.ledger();
        let src = self.cand.clone_fixture();
        let mut seals = Vec::new();
        let mut reveals = Vec::new();
        for (job, p) in jobs.iter().zip(&sealers) {
            let generated = greedy(&src, &ledger, &source_class, &job.prompt, job.max_new_tokens as usize);
            let produced = produce(&src, &ledger, &source_class, job, self.net.kid(*p), generated, |_| {});
            let (seal, reveal) = seal_and_reveal(&ledger, self.net.kid(*p), &produced.object);
            seals.push((*p, self.net.route(*p, &seal)));
            reveals.push((*p, produced.claim.id(), produced.trace, reveal));
        }
        self.net.send(seals).await;
        let sealed = self.net.ledger();
        let mut opt_ins = Vec::new();
        for (job, p) in jobs.iter().zip(&sealers) {
            let producer = self.net.kid(*p);
            let seal = sealed.seals[&(job.id(), producer)].seal;
            let k = K::OptInPaidBeaconSource {
                producer,
                job: job.id(),
                seal,
                context: terms.source.context,
                source_terms: terms.source.id(),
                fee_terms: terms.id(),
            };
            opt_ins.push((*p, self.net.route(*p, &k)));
        }
        let rehearsals: Vec<_> = opt_ins.iter().map(|(_, o)| self.net.chain.ctx.consensus.palw_object_rehearsal_v1(o)).collect();
        let again = opt_ins.clone();
        self.net.send(opt_ins).await;
        let rehearsals: Vec<_> = rehearsals
            .into_iter()
            .zip(again.iter().map(|(_, o)| self.net.chain.ctx.consensus.palw_object_rehearsal_v1(o)))
            .collect();
        for p in &sealers {
            assert!(
                self.net.ledger().beacon_source_fee_promises.contains_key(&(terms.source.context, self.net.kid(*p))),
                "the sealed source opted in with its prepaid fee promise (rehearsals {rehearsals:?}; profiles: eligible {:?} excluded {:?} candidate {:?}, src {:02x} kernel {:02x}; daa {}, seal window {}..{}, sources {:?}, promises {:?})",
                target.context.eligible_profiles.iter().map(|p| p[0]).collect::<Vec<_>>(),
                target.context.excluded_profiles.iter().map(|p| p[0]).collect::<Vec<_>>(),
                target.context.candidate_profile_id,
                self.src_class[0],
                self.kernel_class[0],
                self.net.daa(),
                terms.source.seal_start,
                terms.source.seal_end,
                self.net.ledger().beacon_sources.keys().map(|(c, p)| (c[0], p[0])).collect::<Vec<_>>(),
                self.net.ledger().beacon_source_fee_promises.keys().map(|(c, p)| (c[0], p[0])).collect::<Vec<_>>()
            );
        }
        self.net.beat_to(terms.source.seal_end).await;
        for (p, id, trace, reveal) in &reveals {
            self.kernel_covenant(*p, *id, trace, reveal).await;
        }
        let carriers: Vec<_> = reveals.iter().map(|(p, _, _, r)| (*p, self.net.route(*p, r))).collect();
        self.net.send(carriers).await;
        for (_, id, _, _) in &reveals {
            assert!(self.net.ledger().claims.contains_key(id), "the opted-in source revealed inside its window");
        }
        self.net.beat_to(terms.source.reveal_end).await;
    }

    /// **`n` approved paid source claims of the SOURCE class** (the real path — see the module doc): each job funded by a distinct
    /// consumer, enrolled by it, committed under its producer's signed covenant, locked (the bootstrap draw while no approved paid
    /// Final exists), receipted by the drawn seats. They reach their canonical paid Final on their own clock. Returns the base ids.
    pub(super) async fn paid_source_claims(&mut self, n: usize) -> Vec<Digest> {
        kaspa_core::log::try_init_logger("warn,kaspa_consensus::pipeline::virtual_processor=info");
        let paid_class = self.paid_source_class().await;
        let consumers = [REGISTRANT, OUTSIDER, 0, 2, 4, 5, 6, 7];
        assert!(n <= consumers.len(), "the fixture must fund independent source consumers");
        let mut enrollments = Vec::new();
        for consumer in consumers.into_iter().take(n) {
            self.jobs += 1;
            let job = KernelJobV1 {
                class_binding_id: self.src_class,
                prompt: vec![3, 17, 9],
                max_new_tokens: 3,
                decode: DecodeRuleV1::Greedy,
                nonce: [self.jobs; 64],
            };
            let o = self.net.route(consumer, &K::PostJob { job: job.clone() });
            self.net.send(vec![(consumer, o)]).await;
            // The payer's prospective consent, before any seal of the job.
            let route = self.net.api().unwrap();
            let enroll = misaka_palw_sdk::claim_paid_v2::paid_claim_job_object_v2(
                &route,
                paid_class,
                job.id(),
                self.net.kid(consumer),
                [self.jobs; 64],
                self.net.daa() + 200,
                1,
            )
            .unwrap_or_else(|e| panic!("the payer enrolls its job: {e:?}"));
            let K::PaidClaimV2 { object: PaidClaimObjectV2::EnrollJob { job: paid_job } } = &enroll else { unreachable!() };
            let paid_job = paid_job.id();
            let o = self.net.route(consumer, &enroll);
            let rehearsal = self.net.chain.ctx.consensus.palw_object_rehearsal_v1(&o);
            self.net.send(vec![(consumer, o)]).await;
            assert!(
                self.net.api().unwrap().aux.keys().any(|(t, k)| *t == kaspa_consensus_core::palw_claim_challenge_v2::PAID_CLAIM_JOBS_V2
                    && *k == borsh::to_vec(&paid_job).unwrap()),
                "the payer's job enrollment landed (rehearsal: {rehearsal:?})"
            );
            // The producer's run, its seal, its covenant, then its paid commit (the fold applies the salted reveal inside).
            let producer = loop {
                let p = self.producer();
                if p != consumer {
                    break p;
                }
            };
            let ledger = self.net.ledger();
            let src = self.src.clone_fixture();
            let class = self.src_class;
            let generated = greedy(&src, &ledger, &class, &job.prompt, job.max_new_tokens as usize);
            let produced = produce(&src, &ledger, &class, &job, self.net.kid(producer), generated, |_| {});
            let base = produced.claim.id();
            let (seal, reveal) = seal_and_reveal(&ledger, self.net.kid(producer), &produced.object);
            let K::CommitClaimSalted { salt, commit } = reveal else { panic!("salted seals are in force on this network") };
            let o = self.net.route(producer, &seal);
            self.net.send(vec![(producer, o)]).await;
            // RFC08 F3: the covenant carries the claim body's complete material — every committed position, as served — verified
            // against the ledger that admits the claim (`PalwKernelMaterialBundleV1::verify`), so its scope is the body's commitment.
            let consent = self.kernel_covenant(producer, base, &produced.trace, &K::CommitClaimSalted { commit: commit.clone(), salt }).await;
            let route = self.net.api().unwrap();
            let replays: Vec<String> = (0..2u64)
                .map(|d| {
                    let mut l: misaka_palw_kernel::ledger::KernelLedgerV1 = (*route.ledger().expect("the route's ledger")).clone();
                    l.daa = self.net.daa() + d;
                    let inner = K::CommitClaimSalted { commit: commit.clone(), salt };
                    format!(
                        "+{d}: {:?}",
                        l.apply_object(&inner, &misaka_palw_kernel::ledger::AuthV1 { signer_bond: self.net.kid(producer) })
                            .map(|_| ())
                    )
                })
                .collect();
            let replays = {
                let l = route.ledger().expect("the route's ledger");
                let row = l.bonds.get(&self.net.kid(producer)).cloned();
                let mut z: misaka_palw_kernel::ledger::KernelLedgerV1 = (*l).clone();
                z.daa = self.net.daa() + 1;
                z.bonds.entry(self.net.kid(producer)).and_modify(|b| b.collateral = b.reserved);
                let starved = z
                    .apply_object(
                        &K::CommitClaimSalted { commit: commit.clone(), salt },
                        &misaka_palw_kernel::ledger::AuthV1 { signer_bond: self.net.kid(producer) },
                    )
                    .map(|_| ());
                format!(
                    "{replays:?}; kernel bond {row:?}, claim_collateral {}, consensus collateral {}, with no free collateral: {starved:?}",
                    l.policy.claim_collateral,
                    self.net.collateral(producer)
                )
            };
            let paid = misaka_palw_sdk::claim_paid_v2::paid_claim_commit_object_derived_v2(
                &route,
                paid_class,
                paid_job,
                commit,
                salt,
                consent.as_bytes(),
                // The signed lifetime starts at or below the DAA of the block that will carry it (`ctx.daa_score ≥ committed_daa`).
                self.net.daa(),
            )
            .unwrap_or_else(|e| panic!("the producer's paid commit: {e:?}"));
            let K::PaidClaimV2 { object: PaidClaimObjectV2::Commit { enrollment, .. } } = &paid else { unreachable!() };
            let enrollment = enrollment.id();
            let o = self.net.route(producer, &paid);
            let rehearsal = self.net.chain.ctx.consensus.palw_object_rehearsal_v1(&o);
            self.net.send(vec![(producer, o)]).await;
            assert!(self.net.ledger().claims.contains_key(&base), "the paid commit admitted the base claim at DAA {} (rehearsal: {rehearsal:?}; ledger replays {replays:?})", self.net.daa());
            // ADR-0185: once an approved paid Final exists the bootstrap draw is closed, and a later target locks on the
            // seal-reveal seed — its payer funds the future source fee, other producers seal and opt in after its commitment,
            // reveal after the seal window, and the lock follows the reveal window.
            if self.net.api().unwrap().paid_claim_approved_final_exists_v2() {
                eprintln!("[paid-sources] an approved paid Final exists at DAA {}: seal-reveal sources", self.net.daa());
                self.seal_reveal_sources(base, enrollment, consumer, producer).await;
            }
            enrollments.push((base, enrollment));
        }
        // Every lock is taken before any of them is Final (inside the bootstrap window), or after its own reveal window.
        for (_, enrollment) in &enrollments {
            let lock = K::PaidClaimV2 { object: PaidClaimObjectV2::Lock { enrollment: *enrollment } };
            let o = self.net.route(REGISTRANT, &lock);
            let rehearsal = self.net.chain.ctx.consensus.palw_object_rehearsal_v1(&o);
            eprintln!("[paid-sources] lock rehearsal: {rehearsal:?}");
            self.net.send(vec![(REGISTRANT, o)]).await;
            assert!(
                self.net.api().unwrap().aux.contains_key(&(PAID_CLAIM_LOCKS_V2, borsh::to_vec(enrollment).unwrap())),
                "the lock is taken (rehearsal: {rehearsal:?})"
            );
        }
        for (base, enrollment) in &enrollments {
            let route = self.net.api().unwrap();
            let lock: PalwPaidClaimLockV2 = route
                .aux_row(PAID_CLAIM_LOCKS_V2, &borsh::to_vec(enrollment).unwrap())
                .unwrap_or_else(|| panic!("the bootstrap lock of {:02x}{:02x} is taken", base[0], base[1]));
            assert!(
                [misaka_palw_kernel::claim_paid_v2::paid_claim_bootstrap_anchor_v2(), misaka_palw_kernel::claim_paid_v2::paid_claim_seal_anchor_v2()]
                    .contains(&lock.anchor),
                "a bootstrap or seal-reveal draw"
            );
            let authority: PalwPaidClaimAuthorityV2 =
                route.paid_claim_authority_for_base_unchecked_v2(base).expect("the paid authority");
            let mut items = Vec::new();
            for seat in &lock.seats {
                let card = (0..8).find(|c| self.net.bond(*c) == *seat).expect("a drawn seat is a harness card");
                let receipt = PaidClaimReceiptV2 {
                    version: 2,
                    enrollment: *enrollment,
                    verification_class: authority.class.id(),
                    assignment: lock.assignment,
                    source_context: lock.context,
                    sample_seed: lock.seed,
                    evidence_root: authority.enrollment.commit_root,
                    transcript_root: authority.enrollment.prior_transcript,
                    signer: self.net.kid(card),
                    signed_daa: self.net.daa(),
                    complete: true,
                    round: 0,
                    message_root: [0; 64],
                };
                let o = self.net.route(card, &K::PaidClaimV2 { object: PaidClaimObjectV2::Receipt { receipt } });
                items.push((card, o));
            }
            let rehearsals: Vec<_> = items.iter().map(|(_, o)| self.net.chain.ctx.consensus.palw_object_rehearsal_v1(o)).collect();
            // One receipt a block (validation 2026-10-17: three in one block landed two).
            for item in items {
                self.net.send(vec![item]).await;
            }
            assert!(
                self.net.ledger().paid_claim_consumers_v2.get(base).is_some_and(|c| c.complete),
                "the assigned quorum's receipts complete the paid claim (seats {}, rehearsals {rehearsals:?}, consumer {:?})",
                lock.seats.len(),
                (
                    self.net.ledger().paid_claim_consumers_v2.get(base).cloned(),
                    self.net.api().unwrap().aux.keys().filter(|(t, _)| *t == kaspa_consensus_core::palw_claim_challenge_v2::PAID_CLAIM_RECEIPTS_V2).count(),
                    self.net.api().unwrap().aux_row::<PalwPaidClaimLockV2>(PAID_CLAIM_LOCKS_V2, &borsh::to_vec(enrollment).unwrap()).map(|l| l.source_valid)
                )
            );
        }
        enrollments.into_iter().map(|(base, _)| base).collect()
    }
}
